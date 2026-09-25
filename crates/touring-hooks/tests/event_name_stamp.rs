//! S214 guard (2026-09-25) — the `hookEventName` a hook answers with IS the
//! event that invoked it.
//!
//! Claude Code rejects an output whose `hookSpecificOutput.hookEventName`
//! differs from the invoking event. Handlers named their event by hand and the
//! analise contract suite caught two shapes of the same defect after the
//! 30.4.70 deploy: a Rust identifier (`pre_bash`, from `validation_deny`) and a
//! fixed `PostToolUse` answering a `PostToolUseFailure` (`post_bash`, 3 sites).
//! The fix lives at the process boundary, not in each handler — so these tests
//! drive the REAL binary, daemon skipped, with the event on stdin.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

fn run_hook(subcommand: &str, payload: &str, home: &Path) -> String {
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

/// The answered event name; the hook must have SPOKEN, or the test is vacuous.
fn answered_event(stdout: &str) -> String {
    let v: serde_json::Value = serde_json::from_str(stdout.trim())
        .unwrap_or_else(|e| panic!("hook stdout is not JSON ({e}): {stdout:?}"));
    v.pointer("/hookSpecificOutput/hookEventName")
        .and_then(|n| n.as_str())
        .unwrap_or_else(|| panic!("no hookSpecificOutput.hookEventName in {v}"))
        .to_string()
}

#[test]
fn validation_deny_answers_with_the_invoking_event_not_the_handler_name() {
    let tmp = tempfile::tempdir().expect("tempdir");
    // A payload `pre-bash` cannot validate (no `command`) → the validation deny path.
    let payload = r#"{"session_id":"s214","hook_event_name":"PreToolUse","tool_name":"Read","tool_input":{"file_path":"/x/README.md"}}"#;
    let out = run_hook("pre-bash", payload, tmp.path());
    assert_eq!(answered_event(&out), "PreToolUse", "stdout: {out}");
}

#[test]
fn post_bash_answers_a_failure_as_post_tool_use_failure() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let payload = r#"{"session_id":"s214","hook_event_name":"PostToolUseFailure","tool_name":"Bash","tool_input":{"command":"false"},"error":"Exit code 1","is_interrupt":false}"#;
    let out = run_hook("post-bash", payload, tmp.path());
    assert_eq!(answered_event(&out), "PostToolUseFailure", "stdout: {out}");
}
// The two tests expect DIFFERENT names — together they prove the stamp copies the
// invoking event instead of forcing one constant.
