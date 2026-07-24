//! Wave 24 (2026-04-18) — E2E tests for the new Claude Code hook surface
//! integration: PreToolUse:Task* matchers + top-level TaskCreated/TaskCompleted
//! events. Proves the touring binary + hook binary respond with the correct
//! protocol shape under each event surface configured in `~/.claude/settings.json`.
//!
//! These are pure black-box tests — they spawn the production binaries
//! exactly the way the Claude Code harness invokes them, so the contract
//! covered here is the *user-visible* one, not an internal API.
//!
//! Run with:
//!   cargo test -p touring-hooks --test wave24_hook_integration_e2e -- --nocapture
//!
//! Skipped automatically when the `touring` / `touring-hook` binaries are
//! not present (e.g., on a fresh checkout before `cargo build --release`).

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

/// Locate a workspace binary, preferring release over debug.
fn locate_binary(name: &str) -> Option<PathBuf> {
    // tests run from `crates/touring-hooks/`; bins live at `../../target/{release,debug}/`.
    let workspace_target = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .map(|p| p.join("target"))?;

    for profile in ["release", "debug"] {
        let candidate = workspace_target.join(profile).join(name);
        if candidate.exists() {
            return Some(candidate);
        }
    }
    None
}

/// Run a binary with `stdin` and capture stdout. Returns `None` if the
/// binary isn't built — caller should treat that as a skip, not a failure.
fn run_with_stdin(
    bin_name: &str,
    args: &[&str],
    stdin_payload: &str,
) -> Option<(String, String, i32)> {
    let bin = locate_binary(bin_name)?;
    let mut child = Command::new(&bin)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn");

    if let Some(mut sin) = child.stdin.take() {
        sin.write_all(stdin_payload.as_bytes())
            .expect("write stdin");
    }
    let out = child.wait_with_output().expect("wait child");
    Some((
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.code().unwrap_or(-1),
    ))
}

/// Test 1 — PreToolUse:Task* matcher contract.
///
/// `~/.claude/settings.json` registers `touring pre-task-scout` as the
/// PreToolUse handler for `TaskCreate|TaskUpdate|TaskList|TaskOutput|TaskGet`.
/// Claude Code expects a JSON object containing `hookSpecificOutput` with the
/// `hookEventName == "PreToolUse"` and an `additionalContext` field.
#[test]
fn pre_task_scout_returns_pretooluse_envelope() {
    let payload = r#"{"tool_name":"TaskCreate","tool_input":{"subject":"wave24 e2e","description":"validate scout"}}"#;
    let Some((stdout, _stderr, exit)) = run_with_stdin("touring", &["pre-task-scout"], payload)
    else {
        eprintln!("touring binary not built — skipping pre_task_scout test");
        return;
    };

    assert_eq!(exit, 0, "pre-task-scout must exit 0, got {exit}");
    let parsed: serde_json::Value =
        serde_json::from_str(stdout.trim()).expect("pre-task-scout output must be valid JSON");

    let hook_output = parsed
        .get("hookSpecificOutput")
        .expect("response missing hookSpecificOutput envelope");
    assert_eq!(
        hook_output.get("hookEventName").and_then(|v| v.as_str()),
        Some("PreToolUse"),
        "hookEventName must be PreToolUse"
    );
    assert!(
        hook_output.get("additionalContext").is_some(),
        "PreToolUse envelope must include additionalContext key (may be empty)"
    );
}

/// Test 2 — TaskCreated top-level event handler.
///
/// `task-created` subcommand on the touring-hook binary responds with a
/// human-readable receipt that mentions: scaffolding (scout/implement/validate),
/// session start, and a deterministic RL reward (+0.1). All three signals
/// must be present so the integration cannot silently regress to no-op.
#[test]
fn task_created_event_returns_scaffolded_receipt() {
    let payload = r#"{"task_id":"wave24-test","subject":"e2e validation"}"#;
    let Some((stdout, _stderr, exit)) = run_with_stdin("touring-hook", &["task-created"], payload)
    else {
        eprintln!("touring-hook binary not built — skipping task_created test");
        return;
    };

    assert_eq!(exit, 0, "task-created must exit 0, got {exit}");
    let response = stdout.trim();
    assert!(!response.is_empty(), "task-created produced empty stdout");

    // Three orthogonal signals — losing any one is a real regression.
    assert!(
        response.contains("scaffolded"),
        "missing 'scaffolded' DAG signal in: {response}"
    );
    assert!(
        response.contains("session:"),
        "missing 'session:' bootstrap signal in: {response}"
    );
    assert!(
        response.contains("RL"),
        "missing RL-reward signal in: {response}"
    );
}

/// Test 3 — TaskCompleted top-level event handler.
///
/// `task-completed` returns a richer receipt — DAG archive, RL reward,
/// lesson storage, diary suggestion, and a wiring-orphan check command.
/// Asserting only on the non-negotiable subset (archive + RL + wiring hint).
#[test]
fn task_completed_event_returns_full_outcome_receipt() {
    let payload = r#"{"task_id":"wave24-test","status":"completed"}"#;
    let Some((stdout, _stderr, exit)) =
        run_with_stdin("touring-hook", &["task-completed"], payload)
    else {
        eprintln!("touring-hook binary not built — skipping task_completed test");
        return;
    };

    assert_eq!(exit, 0, "task-completed must exit 0, got {exit}");
    let response = stdout.trim();
    // R33-S3 fix: accept both "archived" (all subtasks terminal) and
    // "partial (pending subtasks)" (no subtasks created — valid for e2e test
    // that doesn't pre-populate the DAG). The hook correctly distinguishes
    // between full archive and partial state based on actual subtask status.
    assert!(
        response.contains("DAG archived") || response.contains("DAG partial"),
        "missing DAG signal (archived or partial) in: {response}"
    );
    assert!(
        response.contains("RL"),
        "missing RL-reward signal in: {response}"
    );
    assert!(
        response.contains("wiring") || response.contains("orphan"),
        "missing wiring/orphan follow-up signal in: {response}"
    );
}

/// Test 4 — Daemon health post-restart.
///
/// After the `systemctl --user restart touring-daemon.service` cycle in
/// Wave 24, all 5 doctor checks must pass. This test exists as a
/// regression guard against future Cargo.toml changes that accidentally
/// break the binary's ability to attach to the daemon socket.
#[test]
fn daemon_doctor_passes_after_session_warmup() {
    let Some(bin) = locate_binary("touring") else {
        eprintln!("touring binary not built — skipping doctor test");
        return;
    };
    let out = Command::new(&bin)
        .args(["doctor", "-j"])
        .output()
        .expect("spawn doctor");
    assert_eq!(out.status.code().unwrap_or(-1), 0, "doctor -j must exit 0");
    let json: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("doctor -j must emit JSON");
    let checks = json.as_array().expect("doctor -j returns array");
    let failures: Vec<_> = checks
        .iter()
        .filter(|c| {
            // "warning" (e.g. wiring_diagnostic data-quality notes) is not a hard failure.
            !matches!(
                c.get("status").and_then(|s| s.as_str()),
                Some("ok") | Some("warning")
            )
        })
        .collect();
    assert!(
        failures.is_empty(),
        "doctor reports {} failing check(s): {:#?}",
        failures.len(),
        failures
    );
}

/// Test 5 — Settings.json invariants.
///
/// Wave 24 added two new event handlers and one new PreToolUse matcher.
/// Read the live `~/.claude/settings.json` and prove all three remain
/// wired. Skipped when the file isn't readable (CI environments).
#[test]
fn settings_json_contains_wave24_hooks() {
    let path = match std::env::var("HOME") {
        Ok(h) => PathBuf::from(h).join(".claude/settings.json"),
        Err(_) => {
            eprintln!("HOME unset — skipping settings.json check");
            return;
        }
    };
    let Ok(raw) = std::fs::read_to_string(&path) else {
        eprintln!("{} not readable — skipping", path.display());
        return;
    };
    let json: serde_json::Value = serde_json::from_str(&raw).expect("settings.json must be valid");

    let hooks = json.get("hooks").expect("missing hooks key");

    // (1) Top-level TaskCreated event handler.
    assert!(
        hooks.get("TaskCreated").is_some(),
        "missing TaskCreated event handler"
    );
    // (2) Top-level TaskCompleted event handler.
    assert!(
        hooks.get("TaskCompleted").is_some(),
        "missing TaskCompleted event handler"
    );
    // (3) PreToolUse matcher for the Task* tool family.
    let pre_tool_use = hooks
        .get("PreToolUse")
        .and_then(|v| v.as_array())
        .expect("PreToolUse must be array");
    let has_task_matcher = pre_tool_use.iter().any(|entry| {
        entry
            .get("matcher")
            .and_then(|m| m.as_str())
            .is_some_and(|s| s.contains("TaskCreate") && s.contains("TaskUpdate"))
    });
    assert!(
        has_task_matcher,
        "PreToolUse missing Task* matcher (TaskCreate|TaskUpdate|...)"
    );
}
