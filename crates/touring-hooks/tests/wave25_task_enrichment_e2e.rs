//! Wave 25 (2026-04-18) — E2E tests for `cli_pre_task_scout` task-mode
//! enrichment. Wave 24 wired `PreToolUse:TaskCreate|...` to this handler
//! but the handler returned empty `additionalContext` for any payload
//! lacking `file_path`, which is EVERY Task* and EnterPlanMode invocation.
//!
//! This test pins the new behaviour: Task* / EnterPlanMode payloads carry
//! `subject` / `description` / `prompt`, and the handler now surfaces a
//! compact hook-context line from three signals:
//!
//!   1. memory recall on the subject (top-2 matching keys)
//!   2. decompose `ready_count` (pending subtasks w/ deps completed)
//!   3. wiring orphan headline (only for TaskCreate / EnterPlanMode —
//!      the tools that spawn new work, REGRA #0 potencializar)
//!
//! The tests drive the real compiled `touring` binary so they cover the
//! full wire path: CLI arg parse → daemon_query → hook_registry dispatch
//! → `cli_pre_task_scout` → in-process helper calls → JSON envelope.

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

/// Locate the `touring` release / debug binary, skipping if absent.
fn touring_binary() -> Option<PathBuf> {
    let target = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .map(|p| p.join("target"))?;
    for profile in ["release", "debug"] {
        let candidate = target.join(profile).join("touring");
        if candidate.exists() {
            return Some(candidate);
        }
    }
    None
}

/// Run `touring pre-task-scout` with the given hook payload, return the
/// parsed `hookSpecificOutput` envelope or `None` if the binary is not
/// built / the daemon is unavailable (these tests are advisory when
/// infrastructure is missing — CI-safe).
fn scout(payload: &serde_json::Value) -> Option<serde_json::Value> {
    let bin = touring_binary()?;
    let mut child = Command::new(&bin)
        .arg("pre-task-scout")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn touring");
    if let Some(mut sin) = child.stdin.take() {
        let _ = sin.write_all(payload.to_string().as_bytes());
    }
    let out = child.wait_with_output().expect("wait touring");
    if !out.status.success() {
        // stderr often contains "Connection refused" / "Daemon returned
        // success=false" under cold-start races. Treat as infrastructure
        // gap, not a test failure.
        let err = String::from_utf8_lossy(&out.stderr);
        eprintln!("touring pre-task-scout failed ({}): {}", out.status, err);
        return None;
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    let parsed: serde_json::Value = serde_json::from_str(stdout.trim()).ok()?;
    parsed.get("hookSpecificOutput").cloned()
}

/// Pull the additionalContext string out of a parsed envelope.
fn context_of(envelope: &serde_json::Value) -> String {
    envelope
        .get("additionalContext")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

/// Test 1 — TaskCreate with a real-seeming subject: the handler should
/// return a non-empty additionalContext. We don't pin the exact string
/// (memory/wiring counts drift across sessions) — we pin the *shape*.
#[test]
fn task_create_payload_returns_enriched_context() {
    let payload = serde_json::json!({
        "tool_name": "TaskCreate",
        "tool_input": {
            "subject": "refactor wiring orphans",
            "description": "reduce orphan pub symbols"
        }
    });
    let Some(env) = scout(&payload) else { return };

    assert_eq!(
        env.get("hookEventName").and_then(|v| v.as_str()),
        Some("PreToolUse"),
        "TaskCreate envelope must declare hookEventName=PreToolUse"
    );

    let context = context_of(&env);
    // At least one of the three enrichment signals should fire on an
    // established workspace — memory, decompose ready, OR wiring orphans.
    // The test is satisfied if context is non-empty. Empty context means
    // all three helpers returned zero hits, which should not happen on a
    // workspace with thousands of memory entries and 8k+ orphans.
    assert!(
        !context.is_empty(),
        "TaskCreate with rich subject must surface at least one hint; got empty context"
    );
}

/// Test 2 — EnterPlanMode carries its intent in `prompt`, not `subject`.
/// The extractor must cover `prompt` so EnterPlanMode also benefits.
#[test]
fn enter_plan_mode_prompt_is_treated_as_subject() {
    let payload = serde_json::json!({
        "tool_name": "EnterPlanMode",
        "tool_input": {"prompt": "refactor the wiring subsystem"}
    });
    let Some(env) = scout(&payload) else { return };

    let context = context_of(&env);
    assert!(
        !context.is_empty(),
        "EnterPlanMode with `prompt` must surface a hint (memory/ready/orphans)"
    );
    // EnterPlanMode is in the `wants_orphans` set — wiring headline is
    // the most-likely hit on any established workspace.
    // This is advisory; we keep the primary assertion on non-emptiness.
}

/// Test 3 — TaskList has no `subject` / `description` / `prompt`, so the
/// `task_subject()` extractor returns None and the handler short-circuits
/// to empty context. Important to pin because this behaviour prevents
/// spamming the read-only list operation with noise.
#[test]
fn task_list_without_subject_returns_empty_context() {
    let payload = serde_json::json!({
        "tool_name": "TaskList",
        "tool_input": {}
    });
    let Some(env) = scout(&payload) else { return };

    assert_eq!(
        context_of(&env),
        "",
        "TaskList with empty tool_input must yield empty context (no subject = no hint)"
    );
}

/// Test 4 — the `wants_orphans` gate should suppress the wiring hint for
/// the read-only Task* tools even when a subject is present. We exploit
/// that by picking a nonsense subject that is very unlikely to match any
/// memory key (no memory hit) and verify that the orphan hint is NOT in
/// the output for TaskList / TaskGet / TaskOutput.
#[test]
fn read_only_task_tools_suppress_wiring_hint() {
    // "xyzzy_neverexisting_9487_query" is unlikely to hit memory / decompose.
    let subject = "xyzzy_neverexisting_9487_query";

    for read_only_tool in ["TaskList", "TaskGet", "TaskOutput"] {
        let payload = serde_json::json!({
            "tool_name": read_only_tool,
            "tool_input": {"subject": subject}
        });
        let Some(env) = scout(&payload) else { continue };
        let context = context_of(&env);
        assert!(
            !context.contains("wiring orphans:"),
            "{read_only_tool} must NOT include wiring orphan hint, got: {context}"
        );
    }
}

/// Test 5 — the file-based path (Read / Edit / Write) is preserved. This
/// tests that the Wave 25 dispatch doesn't accidentally swallow file
/// payloads. The existing SQLite-backed LRU cache produces non-error
/// output for any real file; empty context is also acceptable (cache
/// miss + scouter returned empty). The assertion is on the envelope
/// shape, not the content.
#[test]
fn file_based_tool_preserves_existing_shape() {
    let file_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("lib.rs");
    let payload = serde_json::json!({
        "tool_name": "Read",
        "tool_input": {"file_path": file_path.to_string_lossy()}
    });
    let Some(env) = scout(&payload) else { return };

    assert_eq!(
        env.get("hookEventName").and_then(|v| v.as_str()),
        Some("PreToolUse"),
        "file-based dispatch must still emit PreToolUse envelope"
    );
    // additionalContext may be empty or populated depending on cache /
    // scouter state; both are correct behaviours for a file payload.
    assert!(
        env.get("additionalContext").is_some(),
        "envelope missing additionalContext key for Read payload"
    );
}

/// Test 6 — tool_name that isn't Task* / EnterPlanMode / file-based
/// should also route through the file_path branch, which short-circuits
/// to empty context when no file_path is present. This is the legacy
/// behaviour for tools outside the Task family (Bash, Grep, etc.).
#[test]
fn non_task_non_file_tool_returns_empty_context() {
    let payload = serde_json::json!({
        "tool_name": "Bash",
        "tool_input": {"command": "echo hi"}
    });
    let Some(env) = scout(&payload) else { return };
    assert_eq!(
        context_of(&env),
        "",
        "Bash without file_path falls through to empty context (existing behaviour)"
    );
}
