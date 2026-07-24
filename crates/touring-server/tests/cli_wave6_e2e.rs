//! E2E tests for Wave 6 diary project-scoping (W6) and session-resume features.
//!
//! Covers:
//! - `diary write --project <p> --task <id> --subtask <id>`
//! - `diary read --project <p>`
//! - `diary read --project <p> --task <id>`
//! - `diary projects <agent>`

use std::path::PathBuf;
use std::process::{Command, Stdio};
use tempfile::TempDir;

fn touring_bin() -> PathBuf {
    // Always use the debug binary so tests run against the most recently compiled code.
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let workspace = std::path::Path::new(manifest_dir)
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    workspace.join("target/debug/touring")
}

fn start_daemon() -> u32 {
    // REGRA #19: per-process socket isolation (TOURING_DAEMON_SOCKET below)
    // makes broad `pkill -9 -f touring-daemon` unnecessary AND dangerous —
    // it would also kill the daemon of any parallel test process or of
    // other CC sessions on the same host. Clean only OUR socket/lock here.
    let socket_path = format!("/tmp/touring-daemon-{}.sock", std::process::id());
    let _ = std::fs::remove_file(&socket_path);
    let _ = std::fs::remove_file(format!("{socket_path}.lock"));

    let daemon = Command::new("/home/gabrielgadea/.local/bin/touring-daemon")
        .env("TOURING_DAEMON_SOCKET", &socket_path)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("daemon spawn failed");
    std::thread::sleep(std::time::Duration::from_secs(2));
    let pid = daemon.id();
    std::mem::forget(daemon);
    pid
}

fn stop_daemon(pid: u32) {
    let _ = Command::new("kill").arg("-9").arg(pid.to_string()).output();
}

fn touring(args: &[&str], tmpdir: &TempDir) -> std::process::Output {
    let socket_path = format!("/tmp/touring-daemon-{}.sock", std::process::id());
    let mut cmd = Command::new(touring_bin());
    for arg in args {
        cmd.arg(arg);
    }
    cmd.current_dir(tmpdir.path())
        .env("TOURING_DAEMON_SOCKET", &socket_path);
    cmd.output().expect("touring CLI failed")
}

fn parse_json(output: &std::process::Output) -> serde_json::Value {
    serde_json::from_slice(&output.stdout).unwrap_or_else(|_| {
        panic!(
            "JSON parse failed. stdout: {}, stderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

// ─── W6: project-scoped write ────────────────────────────────────────────────

#[test]
fn test_diary_write_project_scoped() {
    let tmpdir = TempDir::new().unwrap();
    let daemon_pid = start_daemon();

    let out = touring(
        &[
            "diary",
            "write",
            "scout",
            "scouting wave6 integration",
            "--project",
            "touring-hooks",
            "--task",
            "T-001",
            "--subtask",
            "S-001",
            "--topic",
            "scouting",
        ],
        &tmpdir,
    );
    assert_eq!(
        out.status.code(),
        Some(0),
        "diary write --project failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let json = parse_json(&out);
    assert_eq!(json["status"], "written");
    assert_eq!(json["agent"], "scout");
    assert_eq!(json["project"], "touring-hooks");
    assert_eq!(json["task_id"], "T-001");
    assert_eq!(json["subtask_id"], "S-001");
    assert_eq!(json["project_scoped"], true);

    stop_daemon(daemon_pid);
}

// ─── W6: read by project ─────────────────────────────────────────────────────

#[test]
fn test_diary_read_by_project() {
    let tmpdir = TempDir::new().unwrap();
    let daemon_pid = start_daemon();

    // Write two entries for same project, one for another project
    let _ = touring(
        &[
            "diary",
            "write",
            "architect",
            "designing module A",
            "--project",
            "alpha",
        ],
        &tmpdir,
    );
    let _ = touring(
        &[
            "diary",
            "write",
            "architect",
            "designing module B",
            "--project",
            "alpha",
        ],
        &tmpdir,
    );
    let _ = touring(
        &[
            "diary",
            "write",
            "architect",
            "unrelated entry",
            "--project",
            "beta",
        ],
        &tmpdir,
    );

    // Read only project alpha
    let out = touring(
        &["diary", "read", "architect", "--project", "alpha"],
        &tmpdir,
    );
    assert_eq!(out.status.code(), Some(0));
    let json = parse_json(&out);
    assert_eq!(json["count"], 2, "expected 2 entries for project alpha");

    let contents: Vec<&str> = json["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["content"].as_str().unwrap())
        .collect();
    assert!(contents.iter().any(|c| c.contains("module A")));
    assert!(contents.iter().any(|c| c.contains("module B")));
    assert!(!contents.iter().any(|c| c.contains("unrelated")));

    stop_daemon(daemon_pid);
}

// ─── W6: read by project + task ──────────────────────────────────────────────

#[test]
fn test_diary_read_by_task() {
    let tmpdir = TempDir::new().unwrap();
    let daemon_pid = start_daemon();

    let _ = touring(
        &[
            "diary",
            "write",
            "engineer",
            "working on auth",
            "--project",
            "myapp",
            "--task",
            "T-10",
        ],
        &tmpdir,
    );
    let _ = touring(
        &[
            "diary",
            "write",
            "engineer",
            "working on cache",
            "--project",
            "myapp",
            "--task",
            "T-11",
        ],
        &tmpdir,
    );

    let out = touring(
        &[
            "diary",
            "read",
            "engineer",
            "--project",
            "myapp",
            "--task",
            "T-10",
        ],
        &tmpdir,
    );
    assert_eq!(out.status.code(), Some(0));
    let json = parse_json(&out);
    assert_eq!(json["count"], 1, "expected exactly 1 entry for task T-10");
    assert!(
        json["entries"][0]["content"]
            .as_str()
            .unwrap()
            .contains("auth")
    );

    stop_daemon(daemon_pid);
}

// ─── W6: diary projects subcommand ───────────────────────────────────────────

#[test]
fn test_diary_projects_lists_known_projects() {
    let tmpdir = TempDir::new().unwrap();
    let daemon_pid = start_daemon();

    let _ = touring(
        &[
            "diary",
            "write",
            "auditor",
            "entry for proj-x",
            "--project",
            "proj-x",
        ],
        &tmpdir,
    );
    let _ = touring(
        &[
            "diary",
            "write",
            "auditor",
            "entry for proj-y",
            "--project",
            "proj-y",
        ],
        &tmpdir,
    );

    let out = touring(&["diary", "projects", "auditor"], &tmpdir);
    assert_eq!(
        out.status.code(),
        Some(0),
        "diary projects failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let json = parse_json(&out);
    assert_eq!(json["agent"], "auditor");
    let projects: Vec<&str> = json["projects"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p.as_str().unwrap())
        .collect();
    assert!(
        projects.contains(&"proj-x"),
        "proj-x missing from projects list"
    );
    assert!(
        projects.contains(&"proj-y"),
        "proj-y missing from projects list"
    );
    assert_eq!(json["count"], 2);

    stop_daemon(daemon_pid);
}

// ─── W6: project + topic filtering ───────────────────────────────────────────

#[test]
fn test_diary_write_project_with_topic() {
    let tmpdir = TempDir::new().unwrap();
    let daemon_pid = start_daemon();

    let out = touring(
        &[
            "diary",
            "write",
            "planner",
            "planned wave6 decomposition",
            "--project",
            "wave6",
            "--topic",
            "planning",
        ],
        &tmpdir,
    );
    assert_eq!(out.status.code(), Some(0));
    let json = parse_json(&out);
    assert_eq!(json["project_scoped"], true);
    assert_eq!(json["topic"], "planning");

    // Read by project should return the entry
    let out = touring(&["diary", "read", "planner", "--project", "wave6"], &tmpdir);
    let json = parse_json(&out);
    assert!(json["count"].as_u64().unwrap_or(0) >= 1);

    stop_daemon(daemon_pid);
}
