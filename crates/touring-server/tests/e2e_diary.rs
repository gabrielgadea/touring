//! E2E tests for touring diary CLI — cross-audit PLN2 P4.1
//!
//! Tests the full diary stack:
//! - AgentDiary write/read/list/meta
//! - AAAK marker parsing
//! - Palace hierarchy storage
//! - Topic filtering

use std::path::PathBuf;
use std::process::{Command, Stdio};
use tempfile::TempDir;

/// Resolve a touring binary, preferring the WORKSPACE build over the installed one.
///
/// Both helpers used to hardcode `/home/gabrielgadea/.local/bin/…`, so this
/// suite exercised whatever was last *deployed* rather than the code under
/// test — a green run said nothing about the working tree, and the paths break
/// on any other machine. Workspace first, installed as fallback.
fn resolve_bin(name: &str) -> PathBuf {
    let workspace_target = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .map(|p| p.join("target"));
    if let Some(target) = workspace_target {
        for profile in ["debug", "release"] {
            let candidate = target.join(profile).join(name);
            if candidate.exists() {
                return candidate;
            }
        }
    }
    // Fallback: the user's own install dir, derived from $HOME. It used to be
    // the literal `/home/gabrielgadea/.local/bin/…`, which resolves nowhere on
    // a CI runner — and a test whose fallback cannot exist is a test that only
    // ever ran on one machine (run 31166066285, 2026-08-07).
    let home = std::env::var_os("HOME").map(PathBuf::from);
    home.map_or_else(
        || PathBuf::from(name),
        |h| h.join(".local").join("bin").join(name),
    )
}

fn touring_bin() -> PathBuf {
    resolve_bin("touring")
}

fn daemon_bin() -> PathBuf {
    resolve_bin("touring-daemon")
}

/// Start a fresh daemon for test isolation.
///
/// REGRA #19: per-process socket isolation (TOURING_DAEMON_SOCKET below)
/// makes broad `pkill -9 -f touring-daemon` unnecessary AND dangerous —
/// it would also kill the daemon of any parallel test process or of
/// other CC sessions on the same host. Clean only OUR socket/lock here.
/// Socket path unique to the CURRENT TEST, not merely the current process.
///
/// Keying on `process::id()` alone gave all 9 tests in this binary ONE socket,
/// and `start_daemon` opens with `remove_file(&socket_path)` — so a test
/// starting up deleted the endpoint a concurrently-running neighbour was
/// already talking to, and that neighbour's next CLI call died with
/// "No such file or directory (os error 2)" (2026-08-02). libtest names the
/// worker thread after the test, so both `start_daemon` and `touring` derive
/// the same value inside one test without threading any parameter through.
fn socket_path() -> String {
    let test = std::thread::current()
        .name()
        .unwrap_or("main")
        .replace([':', '/'], "_");
    format!("/tmp/touring-daemon-{}-{}.sock", std::process::id(), test)
}

fn start_daemon() -> u32 {
    let socket_path = socket_path();
    let _ = std::fs::remove_file(&socket_path);
    let _ = std::fs::remove_file(format!("{socket_path}.lock"));

    // We intentionally leak the Child: the daemon must outlive this helper so
    // subsequent touring CLI calls can reach the socket. `stop_daemon()` later
    // kills it via SIGKILL + socket cleanup. `mem::forget` prevents Drop from
    // reaping the handle prematurely.
    // The socket must be handed to the DAEMON, not only to the client. Without
    // this the daemon bound the global default socket while `touring()` dialed
    // `/tmp/touring-daemon-<pid>.sock`, so every CLI call failed with
    // "No such file or directory (os error 2)" — the isolation the doc comment
    // above promised never actually existed (found 2026-08-02).
    let daemon = Command::new(daemon_bin())
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

/// Run touring CLI in a temp directory
fn touring(args: &[&str], tmpdir: &TempDir) -> std::process::Output {
    let socket_path = socket_path();
    let mut cmd = Command::new(touring_bin());
    for arg in args {
        cmd.arg(arg);
    }
    cmd.current_dir(tmpdir.path())
        .env("TOURING_DAEMON_SOCKET", &socket_path);
    cmd.output().expect("touring CLI failed")
}

fn stop_daemon(pid: u32) {
    let _ = Command::new("kill").arg("-9").arg(pid.to_string()).output();
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

fn parse_json_opt(output: &std::process::Output) -> serde_json::Value {
    serde_json::from_slice(&output.stdout).unwrap_or(serde_json::Value::Null)
}

#[test]
fn test_diary_write_and_read() {
    let tmpdir = TempDir::new().unwrap();
    let daemon_pid = start_daemon();

    let out = touring(
        &[
            "diary",
            "write",
            "test_agent",
            "minha primeira entrada",
            "--topic",
            "onboarding",
        ],
        &tmpdir,
    );
    assert_eq!(
        out.status.code(),
        Some(0),
        "diary write failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let json = parse_json(&out);
    assert_eq!(json["status"], "written");
    assert_eq!(json["agent"], "test_agent");
    assert_eq!(json["is_aaak"], false);
    assert!(json["timestamp"].as_str().is_some());

    let out = touring(&["diary", "read", "test_agent"], &tmpdir);
    assert_eq!(out.status.code(), Some(0));
    let json = parse_json(&out);
    assert_eq!(json["count"], 1);
    let entry = &json["entries"][0];
    assert!(
        entry["content"]
            .as_str()
            .unwrap()
            .contains("primeira entrada")
    );

    stop_daemon(daemon_pid);
}

#[test]
fn test_diary_aaak_markers() {
    let tmpdir = TempDir::new().unwrap();
    let daemon_pid = start_daemon();

    let out = touring(
        &[
            "diary",
            "write",
            "audit_agent",
            "#[P:scouting] #[R:0.95] #[L:nenhum orphan encontrado",
            "--aaak",
            "--topic",
            "cross_audit",
        ],
        &tmpdir,
    );
    assert_eq!(out.status.code(), Some(0));
    let json = parse_json(&out);
    assert_eq!(json["is_aaak"], true);

    let out = touring(&["diary", "read", "audit_agent"], &tmpdir);
    let json = parse_json(&out);
    assert_eq!(json["count"], 1);

    let markers: Vec<(String, String)> = json["entries"][0]["markers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| {
            (
                m[0].as_str().unwrap().to_string(),
                m[1].as_str().unwrap().to_string(),
            )
        })
        .collect();
    assert!(
        markers.iter().any(|(k, v)| k == "phase" && v == "scouting"),
        "phase marker missing"
    );
    assert!(
        markers.iter().any(|(k, v)| k == "result" && v == "0.95"),
        "result marker missing"
    );

    stop_daemon(daemon_pid);
}

#[test]
fn test_diary_topic_filter() {
    let tmpdir = TempDir::new().unwrap();
    let daemon_pid = start_daemon();

    touring(
        &[
            "diary",
            "write",
            "topic_agent",
            "entrada alpha",
            "--topic",
            "alpha",
        ],
        &tmpdir,
    );
    touring(
        &[
            "diary",
            "write",
            "topic_agent",
            "entrada beta",
            "--topic",
            "beta",
        ],
        &tmpdir,
    );
    touring(
        &[
            "diary",
            "write",
            "topic_agent",
            "entrada alpha 2",
            "--topic",
            "alpha",
        ],
        &tmpdir,
    );

    let out = touring(
        &["diary", "read", "topic_agent", "--topic", "alpha"],
        &tmpdir,
    );
    let json = parse_json(&out);
    assert_eq!(json["count"], 2);
    assert_eq!(json["topic_filter"], "alpha");

    stop_daemon(daemon_pid);
}

#[test]
fn test_diary_last_n() {
    let tmpdir = TempDir::new().unwrap();
    let daemon_pid = start_daemon();

    for i in 0..5 {
        touring(
            &["diary", "write", "last_agent", &format!("entrada {}", i)],
            &tmpdir,
        );
    }

    let out = touring(&["diary", "read", "last_agent", "--last", "2"], &tmpdir);
    let json = parse_json(&out);
    assert_eq!(json["count"], 2);
    assert_eq!(json["last_n"], 2);

    stop_daemon(daemon_pid);
}

#[test]
fn test_diary_meta_after_write() {
    let tmpdir = TempDir::new().unwrap();
    let daemon_pid = start_daemon();

    touring(&["diary", "write", "agent_a", "entrada A"], &tmpdir);

    let out = touring(&["diary", "meta", "agent_a"], &tmpdir);
    let json = parse_json(&out);
    assert_eq!(json["status"], "ok");
    assert_eq!(json["agent"], "agent_a");
    assert!(json["entry_count"].as_i64().unwrap_or(0) >= 1);

    stop_daemon(daemon_pid);
}

#[test]
fn test_diary_write_exit_code() {
    let tmpdir = TempDir::new().unwrap();
    let daemon_pid = start_daemon();

    let out = touring(&["diary", "write", "exit_test", "test content"], &tmpdir);
    assert_eq!(out.status.code(), Some(0), "valid write must exit 0");

    stop_daemon(daemon_pid);
}

#[test]
fn test_diary_multiple_entries_ordered() {
    let tmpdir = TempDir::new().unwrap();
    let daemon_pid = start_daemon();

    touring(&["diary", "write", "order_agent", "primeira"], &tmpdir);
    std::thread::sleep(std::time::Duration::from_millis(50));
    touring(&["diary", "write", "order_agent", "segunda"], &tmpdir);
    std::thread::sleep(std::time::Duration::from_millis(50));
    touring(&["diary", "write", "order_agent", "terceira"], &tmpdir);

    let out = touring(&["diary", "read", "order_agent"], &tmpdir);
    let json = parse_json(&out);
    // Newest first
    let contents: Vec<&str> = json["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["content"].as_str().unwrap())
        .collect();
    assert_eq!(contents[0], "terceira");
    assert_eq!(contents[1], "segunda");
    assert_eq!(contents[2], "primeira");

    stop_daemon(daemon_pid);
}

#[test]
fn test_diary_no_diary_status() {
    let tmpdir = TempDir::new().unwrap();
    let daemon_pid = start_daemon();

    let out = touring(&["diary", "meta", "nonexistent_agent"], &tmpdir);
    let json = parse_json_opt(&out);
    // Status depends on whether diary was found — use raw output
    assert!(out.status.code() == Some(0) || json.get("status").is_some());

    stop_daemon(daemon_pid);
}

#[test]
fn test_diary_fts5_searchable() {
    // Verify diary entries are ingested into FTS5 so `touring memory recall`
    // can find them via text search.
    let tmpdir = TempDir::new().unwrap();
    let daemon_pid = start_daemon();

    // Write a unique entry with a rare search term
    touring(
        &[
            "diary",
            "write",
            "fts5_agent",
            "diário busca FTS5 integração semantic recall funcionando",
        ],
        &tmpdir,
    );

    // Query memory via recall (FTS5)
    let out = touring(
        &["memory", "recall", "FTS5 integração semantic recall"],
        &tmpdir,
    );
    assert_eq!(
        out.status.code(),
        Some(0),
        "memory recall failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let json = parse_json(&out);
    // Should find the diary entry we just wrote — entries[] is the RLM layer,
    // matches[] is the SemanticRecall FTS5 layer (may be 0 if no embedding stored).
    let diary_found = json["entries"]
        .as_array()
        .map(|arr| {
            arr.iter().any(|m| {
                m["value"]
                    .as_str()
                    .map(|v| v.contains("FTS5") || v.contains("semantic recall"))
                    .unwrap_or(false)
            })
        })
        .unwrap_or(false);

    assert!(
        diary_found,
        "diary entry should appear in recall output. Got: {}",
        String::from_utf8_lossy(&out.stdout)
    );

    stop_daemon(daemon_pid);
}
