//! E2E tests for bidirectional task flow (Claude Code ↔ Touring).
//!
//! Validates the three anti-loop invariants from
//! `docs/2026-04-13-bidirectional-task-flow.md`:
//!   1. Tasks carry explicit `origin` (claude-code vs touring-cli/external-agent)
//!   2. CC-originated tasks never surface in the digest (mirrored_to_cc=1)
//!   3. CC adopting Touring task via external_ref breaks the loop (no DAG duplication)
//!
//! Added: 2026-04-13 (Pln2 bidirectional task flow — B6)

#![allow(clippy::indexing_slicing)]

use serde_json::json;
use tempfile::TempDir;
use touring_hooks::cli_handlers::{
    cli_decompose_create, cli_decompose_mark_mirrored, cli_decompose_status,
};
use touring_hooks::runtime::HookRuntime;
use touring_hooks::task_digest::digest_pending_tasks;

fn setup_runtime() -> (TempDir, HookRuntime) {
    let tmp = TempDir::new().expect("tempdir");
    let root = tmp.path().to_path_buf();
    std::fs::create_dir_all(root.join(".claude/data")).expect("data dir");
    let rt = HookRuntime::new(&root).expect("runtime init");
    (tmp, rt)
}

fn parse_json(s: &str) -> serde_json::Value {
    serde_json::from_str(s).unwrap_or_else(|e| panic!("invalid JSON: {e}\nraw: {s}"))
}

// ───────────────────────────────────────────────────────────────────────
// I1: origin tracking — default vs explicit
// ───────────────────────────────────────────────────────────────────────

#[test]
fn cc_originated_task_defaults_to_claude_code_origin() {
    let (_tmp, mut rt) = setup_runtime();
    let result = parse_json(&cli_decompose_create(
        &mut rt,
        &json!({"task_type": "intent", "description": "CC task"}),
    ));
    assert_eq!(result["origin"], "claude-code");
    assert_eq!(result["mirrored_to_cc"], true);
}

#[test]
fn external_origin_sets_mirrored_to_cc_false() {
    let (_tmp, mut rt) = setup_runtime();
    let result = parse_json(&cli_decompose_create(
        &mut rt,
        &json!({
            "task_type": "intent",
            "description": "external task",
            "origin": "external-agent",
        }),
    ));
    assert_eq!(result["origin"], "external-agent");
    assert_eq!(result["mirrored_to_cc"], false);
}

#[test]
fn touring_cli_origin_sets_mirrored_to_cc_false() {
    let (_tmp, mut rt) = setup_runtime();
    let result = parse_json(&cli_decompose_create(
        &mut rt,
        &json!({
            "task_type": "bug",
            "description": "scout-initiated",
            "origin": "touring-cli",
        }),
    ));
    assert_eq!(result["origin"], "touring-cli");
    assert_eq!(result["mirrored_to_cc"], false);
}

// ───────────────────────────────────────────────────────────────────────
// I2: digest filter — only surfaces non-mirrored, non-CC tasks
// ───────────────────────────────────────────────────────────────────────

#[test]
fn digest_skips_cc_originated_tasks() {
    let (_tmp, mut rt) = setup_runtime();
    // Two CC tasks + one external task
    cli_decompose_create(
        &mut rt,
        &json!({"task_type": "intent", "description": "CC-1"}),
    );
    cli_decompose_create(
        &mut rt,
        &json!({"task_type": "intent", "description": "CC-2"}),
    );
    let ext = parse_json(&cli_decompose_create(
        &mut rt,
        &json!({"task_type": "intent", "description": "external task XYZ", "origin": "external-agent"}),
    ));
    let ext_id = ext["task_id"].as_str().unwrap();

    let digest = digest_pending_tasks(&rt).expect("digest should have pending external task");
    assert!(
        digest.contains(ext_id),
        "digest should mention external task_id"
    );
    assert!(
        digest.contains("external-agent"),
        "digest should tag origin"
    );
    assert!(
        !digest.contains("CC-1") && !digest.contains("CC-2"),
        "digest must NOT include CC-originated tasks: {digest}"
    );
}

#[test]
fn digest_returns_none_when_only_cc_tasks() {
    let (_tmp, mut rt) = setup_runtime();
    cli_decompose_create(
        &mut rt,
        &json!({"task_type": "intent", "description": "CC only"}),
    );
    assert!(digest_pending_tasks(&rt).is_none());
}

#[test]
fn digest_returns_none_on_empty_db() {
    let (_tmp, rt) = setup_runtime();
    // No tasks created — digest must be None, not panic
    assert!(digest_pending_tasks(&rt).is_none());
}

// ───────────────────────────────────────────────────────────────────────
// I3: loop breaking — external_ref adoption path
// ───────────────────────────────────────────────────────────────────────

#[test]
fn mark_mirrored_flips_flag_and_removes_from_digest() {
    let (_tmp, mut rt) = setup_runtime();
    let ext = parse_json(&cli_decompose_create(
        &mut rt,
        &json!({"task_type": "intent", "description": "to be adopted", "origin": "touring-cli"}),
    ));
    let ext_id = ext["task_id"].as_str().unwrap().to_string();

    // Pre-adoption: task visible in digest
    assert!(digest_pending_tasks(&rt).unwrap().contains(&ext_id));

    // Adoption: mark mirrored
    let mark = parse_json(&cli_decompose_mark_mirrored(
        &mut rt,
        &json!({"task_id": ext_id}),
    ));
    assert_eq!(mark["marked"], true);
    assert_eq!(mark["rows_updated"], 1);

    // Post-adoption: task NOT in digest anymore
    assert!(
        digest_pending_tasks(&rt).is_none(),
        "adopted task must not appear in digest"
    );
}

#[test]
fn adoption_via_mark_mirrored_preserves_single_dag_entry() {
    let (_tmp, mut rt) = setup_runtime();
    // Create external task (origin != claude-code)
    let ext = parse_json(&cli_decompose_create(
        &mut rt,
        &json!({"task_type": "intent", "description": "external", "origin": "external-agent"}),
    ));
    let ext_id = ext["task_id"].as_str().unwrap().to_string();

    let before = parse_json(&cli_decompose_status(&mut rt, &json!({})));
    let total_before = before["total_tasks"].as_i64().unwrap_or(0);

    // Simulate adoption: hook would call cli_decompose_mark_mirrored directly
    // on the external ref instead of cli_decompose_create — this is what
    // handle_task_sync_post_create does internally when external_ref is present.
    let mark = parse_json(&cli_decompose_mark_mirrored(
        &mut rt,
        &json!({"task_id": ext_id}),
    ));
    assert_eq!(mark["marked"], true, "mark must succeed for external task");

    let after = parse_json(&cli_decompose_status(&mut rt, &json!({})));
    let total_after = after["total_tasks"].as_i64().unwrap_or(0);

    assert_eq!(
        total_before, total_after,
        "adoption must not create new DAG entry (was {total_before}, now {total_after})"
    );
}

#[test]
fn mark_mirrored_rejects_empty_task_id() {
    let (_tmp, mut rt) = setup_runtime();
    let result = parse_json(&cli_decompose_mark_mirrored(&mut rt, &json!({})));
    assert_eq!(result["marked"], false);
    assert_eq!(result["reason"], "missing task_id");
}

#[test]
fn mark_mirrored_on_unknown_task_id_reports_zero_rows() {
    let (_tmp, mut rt) = setup_runtime();
    let result = parse_json(&cli_decompose_mark_mirrored(
        &mut rt,
        &json!({"task_id": "nonexistent"}),
    ));
    assert_eq!(result["marked"], false);
    assert_eq!(result["rows_updated"], 0);
}

// ───────────────────────────────────────────────────────────────────────
// I4: end-to-end cycle breaking (integration)
// ───────────────────────────────────────────────────────────────────────

#[test]
fn full_touring_to_cc_to_touring_cycle_breaks_loop() {
    let (_tmp, mut rt) = setup_runtime();

    // Step 1: Touring creates task (e.g., scout-initiated)
    let created = parse_json(&cli_decompose_create(
        &mut rt,
        &json!({
            "task_type": "refactor",
            "description": "refactor auth module",
            "origin": "touring-cli",
        }),
    ));
    let task_id = created["task_id"].as_str().unwrap().to_string();
    assert_eq!(created["mirrored_to_cc"], false);

    // Step 2: Next session — digest surfaces the task
    let digest1 = digest_pending_tasks(&rt).expect("digest at session start");
    assert!(digest1.contains(&task_id));

    // Step 3: CC adopts (simulated via the anti-loop primitive the hook calls)
    let mark = parse_json(&cli_decompose_mark_mirrored(
        &mut rt,
        &json!({"task_id": task_id}),
    ));
    assert_eq!(mark["marked"], true);

    // Step 4: Digest NOT re-surfaced (loop broken)
    assert!(
        digest_pending_tasks(&rt).is_none(),
        "adopted task must not re-appear in digest — loop would mean infinite suggestion"
    );
}
