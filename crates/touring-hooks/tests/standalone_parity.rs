//! S219 guard (2026-09-25) — the standalone path answers a hook the way the daemon does.
//!
//! A cold daemon took 2222 ms on the first `task-completed` after a restart (warm:
//! 57/37 ms); under load that passes the client's 3000 ms read timeout and the hook
//! falls back to STANDALONE — where `task-completed` ran only the knowledge/ACO
//! recording: no DAG finalize, no RL reward, no lesson, and 0 bytes on stdout
//! (measured with `TOURING_NO_DAEMON=1`). These tests drive the REAL binary with the
//! daemon skipped, in a scratch HOME/project.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

fn run_standalone(subcommand: &str, payload: &str, home: &Path) -> String {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_touring-hook"));
    cmd.arg(subcommand)
        .current_dir(home)
        .env_clear()
        .env("PATH", std::env::var("PATH").unwrap_or_default())
        .env("HOME", home)
        .env("CLAUDE_PROJECT_DIR", home)
        .env("TOURING_NO_DAEMON", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = cmd.spawn().expect("spawn touring-hook");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(payload.as_bytes())
        .expect("write payload");
    let out = child.wait_with_output().expect("wait touring-hook");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn standalone_task_completed_prints_the_receipt() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let payload = r#"{"session_id":"s219","hook_event_name":"TaskCompleted","task_id":"s219-task","task_subject":"probe the standalone receipt","success":true}"#;
    let out = run_standalone("task-completed", payload, tmp.path());
    let receipt: serde_json::Value = serde_json::from_str(out.trim())
        .unwrap_or_else(|e| panic!("standalone printed no receipt ({e}): {out:?}"));
    assert_eq!(receipt["event"], "task-completed", "{receipt}");
    assert_eq!(receipt["task_id"], "s219-task", "{receipt}");
    assert_eq!(receipt["outcome"], "success", "{receipt}");
}
