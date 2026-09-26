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

/// Top-level fields of Claude Code's `SyncHookJSONOutput` (Context7, 25/09/2026) —
/// the platform's list, written here independently of the binary under test.
const TOP_LEVEL_FIELDS: &[&str] = &[
    "continue",
    "suppressOutput",
    "stopReason",
    "decision",
    "systemMessage",
    "terminalSequence",
    "reason",
    "hookSpecificOutput",
];

fn run_standalone(subcommand: &str, payload: &str, home: &Path) -> (String, String) {
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
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// S219 made the standalone path run the daemon's handler; S224 (26/09/2026) puts
/// its receipt on the channel the protocol leaves for it. `TaskCompleted` admits
/// top-level fields only, and the receipt carried 13 the platform does not know
/// (`event`, `RL`, `finalize_status`, `diary_hint`, …) — so it reached no one. It
/// now goes to stderr (diagnostics), and stdout stays inside the contract.
#[test]
fn standalone_task_completed_keeps_stdout_in_contract_and_the_receipt_on_stderr() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let payload = r#"{"session_id":"s219","hook_event_name":"TaskCompleted","task_id":"s219-task","task_subject":"probe the standalone receipt","success":true}"#;
    let (stdout, stderr) = run_standalone("task-completed", payload, tmp.path());

    if !stdout.trim().is_empty() {
        let answer: serde_json::Value = serde_json::from_str(stdout.trim())
            .unwrap_or_else(|e| panic!("stdout is not JSON ({e}): {stdout:?}"));
        let unknown: Vec<&String> = answer
            .as_object()
            .expect("stdout is a JSON object")
            .keys()
            .filter(|k| !TOP_LEVEL_FIELDS.contains(&k.as_str()))
            .collect();
        assert!(
            unknown.is_empty(),
            "unknown top-level fields {unknown:?}: {answer}"
        );
        assert!(
            answer.get("hookSpecificOutput").is_none(),
            "TaskCompleted accepts no hookSpecificOutput: {answer}"
        );
    }
    let receipt: serde_json::Value = stderr
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line.trim()).ok())
        .find(|v| v.get("event").is_some())
        .unwrap_or_else(|| panic!("no receipt on stderr: {stderr:?}"));
    assert_eq!(receipt["event"], "task-completed", "{receipt}");
    assert_eq!(receipt["task_id"], "s219-task", "{receipt}");
    assert_eq!(receipt["outcome"], "success", "{receipt}");
}
