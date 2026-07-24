//! W12.5 E2E — daemon multi-instance, one per project (Pln2 productization F1).
//!
//! RED (2026-07-24): `daemon_lock_path()` is uid-global, so a second daemon on
//! a DIFFERENT socket exits `AlreadyAlive` (empirically reproduced against the
//! live host daemon). GREEN: the lock derives from the socket path, so N
//! daemons (one per project socket) coexist — while two daemons racing for the
//! SAME socket still resolve idempotently (REGRA #19).
//!
//! REGRA #19 compliance: every process this test touches is a direct child
//! (`Child::kill`) — never a name-based pkill, never another session's daemon.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};
use tempfile::TempDir;

fn daemon_bin() -> PathBuf {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let workspace = Path::new(manifest_dir).parent().unwrap().parent().unwrap();
    // Release binary: the workspace build (F4-3) and validate_phase1.sh both
    // produce it; tests must exercise the freshly-built daemon, not ~/.local/bin.
    workspace.join("target/release/touring-daemon")
}

fn touring_bin() -> PathBuf {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let workspace = Path::new(manifest_dir).parent().unwrap().parent().unwrap();
    workspace.join("target/release/touring")
}

fn spawn_daemon_on(socket: &Path) -> Child {
    Command::new(daemon_bin())
        .env("TOURING_DAEMON_SOCKET", socket)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("daemon spawn failed")
}

fn wait_socket(socket: &Path, timeout: Duration) -> bool {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if socket.exists() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    false
}

fn still_running(child: &mut Child) -> bool {
    matches!(child.try_wait(), Ok(None))
}

struct Reaper(Vec<Child>);
impl Drop for Reaper {
    fn drop(&mut self) {
        for c in &mut self.0 {
            let _ = c.kill();
            let _ = c.wait();
        }
    }
}

#[test]
fn test_two_projects_two_daemons() {
    let proj_a = TempDir::new().unwrap();
    let proj_b = TempDir::new().unwrap();
    let sock_a = proj_a.path().join(".touring").join("daemon.sock");
    let sock_b = proj_b.path().join(".touring").join("daemon.sock");
    std::fs::create_dir_all(sock_a.parent().unwrap()).unwrap();
    std::fs::create_dir_all(sock_b.parent().unwrap()).unwrap();

    let mut reaper = Reaper(vec![]);
    reaper.0.push(spawn_daemon_on(&sock_a));
    assert!(
        wait_socket(&sock_a, Duration::from_secs(10)),
        "daemon A never bound its per-project socket (uid-global lock still \
         serializes daemons — W12.5 RED)"
    );

    reaper.0.push(spawn_daemon_on(&sock_b));
    assert!(
        wait_socket(&sock_b, Duration::from_secs(10)),
        "daemon B never bound its socket while A is alive — the lock is not \
         per-socket yet (W12.5 RED)"
    );

    // Both LISTEN simultaneously and both processes are alive.
    assert!(sock_a.exists() && sock_b.exists());
    for child in &mut reaper.0 {
        assert!(still_running(child), "a daemon exited after binding");
    }
}

#[test]
fn test_decompose_routes_to_task_home_store_across_cwds() {
    // F1-ROUTING regression (2026-07-24): a task created from a cwd that
    // normalizes to the GLOBAL store must remain readable AND writable from a
    // DIFFERENT project's cwd — the pre-fix behavior answered "not found" on
    // get and orphan-wrote 0 rows (while reporting success) on update, which
    // masqueraded as "the DAG was lost on daemon restart" twice in production.
    let touring = touring_bin();
    let home = TempDir::new().unwrap();
    std::fs::create_dir_all(home.path().join(".claude/touring")).unwrap();
    // A REAL project dir (has .touring marker → its own per-project store).
    let proj = TempDir::new().unwrap();
    std::fs::create_dir_all(proj.path().join(".touring")).unwrap();

    let sock = home.path().join("daemon.sock");
    let mut daemon = Command::new(daemon_bin())
        .env("TOURING_DAEMON_SOCKET", &sock)
        .env("HOME", home.path())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("daemon spawn failed");
    assert!(
        wait_socket(&sock, Duration::from_secs(10)),
        "daemon must bind"
    );

    let cli = |args: &[&str], cwd: &Path| -> serde_json::Value {
        let out = Command::new(&touring)
            .args(args)
            .current_dir(cwd)
            .env("TOURING_DAEMON_SOCKET", &sock)
            .env("HOME", home.path())
            .output()
            .expect("touring CLI failed");
        serde_json::from_slice(&out.stdout).unwrap_or_else(|_| {
            panic!(
                "JSON parse failed. stdout: {} stderr: {}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            )
        })
    };

    // Create from HOME (no project marker → global store).
    let created = cli(
        &["decompose", "create", "plan", "routing", "e2e"],
        home.path(),
    );
    let task_id = created["task_id"].as_str().expect("task_id").to_string();
    cli(
        &["decompose", "add", &task_id, "S1", "step one"],
        home.path(),
    );

    // Read from the OTHER project's cwd — pre-fix: "not found".
    let got = cli(&["decompose", "get", &task_id], proj.path());
    assert!(
        got.get("error").is_none() && got["subtask_count"].as_i64() == Some(1),
        "cross-cwd get must find the task in its home store, got: {got}"
    );

    // Write from the OTHER project's cwd — pre-fix: orphan 0-row success.
    let upd = cli(
        &[
            "decompose",
            "update",
            &task_id,
            "S1",
            "--status",
            "completed",
        ],
        proj.path(),
    );
    assert!(
        upd["subtask_updated"].as_bool() == Some(true),
        "cross-cwd update must hit the home store, got: {upd}"
    );
    let verify = cli(&["decompose", "get", &task_id], home.path());
    assert!(
        verify["subtasks"][0]["status"].as_str() == Some("completed"),
        "the update must be durable in the HOME store, got: {verify}"
    );

    // A task in NO store must fail LOUD, never orphan-write.
    let ghost = cli(
        &[
            "decompose",
            "update",
            "task_DOES_NOT_EXIST",
            "X",
            "--status",
            "completed",
        ],
        proj.path(),
    );
    assert!(
        ghost.get("error").is_some(),
        "updating a nonexistent task must be a loud error, got: {ghost}"
    );

    let _ = daemon.kill();
    let _ = daemon.wait();
}

#[test]
fn test_same_socket_second_daemon_exits_idempotently() {
    // The REGRA #19 invariant must SURVIVE the per-socket lock: two daemons
    // racing for the SAME socket still resolve to one winner + one silent
    // Ok(0) exit — never two binds, never an error exit.
    let proj = TempDir::new().unwrap();
    let sock = proj.path().join(".touring").join("daemon.sock");
    std::fs::create_dir_all(sock.parent().unwrap()).unwrap();

    let mut reaper = Reaper(vec![]);
    reaper.0.push(spawn_daemon_on(&sock));
    assert!(
        wait_socket(&sock, Duration::from_secs(10)),
        "first daemon must bind"
    );

    let mut second = spawn_daemon_on(&sock);
    let status = {
        // The loser exits on its own within the lock probe window.
        let start = Instant::now();
        loop {
            if let Ok(Some(st)) = second.try_wait() {
                break st;
            }
            assert!(
                start.elapsed() < Duration::from_secs(10),
                "second daemon on the SAME socket neither exited nor yielded"
            );
            std::thread::sleep(Duration::from_millis(200));
        }
    };
    assert!(
        status.success(),
        "idempotent race resolution must exit 0 (REGRA #19), got {status:?}"
    );
    assert!(sock.exists(), "winner's socket must survive the race");
}
