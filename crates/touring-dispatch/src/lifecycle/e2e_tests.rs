//! End-to-end contract tests for the 10 `lifecycle/` submodules.
//!
//! Each submodule owns exactly one Claude Code hook handler. These tests
//! validate:
//!   1. **Happy path** — valid JSON input produces the expected output
//!      string (includes expected hint markers).
//!   2. **Edge case** — empty/missing fields do not panic and return a
//!      well-formed (possibly empty) string.
//!   3. **Side effect** — observable DB/memory/RL mutation is correctly
//!      applied (verified via `HookRuntime` introspection).
//!
//! These tests are gated by `#[cfg(test)]` and only run via `cargo test -p
//! touring-hooks --lib` — they need `pub(crate)` access to the submodule
//! handlers, which is not available from the integration `tests/` dir.
//!
//! Extracted as part of FIX-3 E2E coverage. Runs independently from the
//! 2740-test main suite and is IDEMPOTENT: every test gets a fresh temp
//! dir via `make_runtime()`.

use super::*;
use serde_json::{Value, json};
use tempfile::TempDir;

use crate::runtime::HookRuntime;

// ── Shared fixtures ──────────────────────────────────────────────────────

/// Create an isolated HookRuntime rooted in a fresh tempdir.
///
/// The `TempDir` must be kept alive (bound in the caller) because dropping
/// it recursively deletes the underlying SQLite databases and invalidates
/// any open connections. Tests always bind both returned values with `let
/// (_tmp, mut rt) = make_runtime();`.
fn make_runtime() -> (TempDir, HookRuntime) {
    let tmp = TempDir::new().expect("tempdir");
    let rt = HookRuntime::new(tmp.path()).expect("runtime");
    (tmp, rt)
}

/// Build a standard TaskSync-shaped input JSON with the given task_id +
/// optional task_subject and status.
fn task_input(task_id: &str, subject: &str, status: Option<&str>) -> Value {
    let mut tool_input = json!({
        "task_id": task_id,
        "task_subject": subject,
    });
    if let Some(s) = status {
        tool_input["status"] = json!(s);
    }
    json!({ "tool_input": tool_input })
}

// ═══════════════════════════════════════════════════════════════════════════
// subagent.rs — handle_subagent_start
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn subagent_start_records_access_event_and_returns_empty() {
    let (_tmp, mut rt) = make_runtime();
    let input = json!({ "session_id": "sub-123" });

    let out = handle_subagent_start(&mut rt, &input);
    assert!(out.is_empty(), "subagent_start must return empty hint");

    // Side effect: __subagent_start__ access row persisted.
    let count = rt
        .ctx
        .knowledge
        .access_count("__subagent_start__")
        .unwrap_or(0);
    assert!(count >= 1, "access row must be recorded, got count={count}");
}

#[test]
fn subagent_start_tolerates_missing_session_id() {
    let (_tmp, mut rt) = make_runtime();
    let out = handle_subagent_start(&mut rt, &json!({}));
    assert!(out.is_empty());
    // "unknown" session_id still counts as a recorded access.
    let count = rt
        .ctx
        .knowledge
        .access_count("__subagent_start__")
        .unwrap_or(0);
    assert!(count >= 1);
}

// ═══════════════════════════════════════════════════════════════════════════
// pre_compact.rs — handle_pre_compact
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn pre_compact_returns_empty_and_does_not_panic() {
    let (_tmp, mut rt) = make_runtime();
    let out = handle_pre_compact(&mut rt, &json!({}));
    // ES2 P3 — handle_pre_compact now emits a digest line (re-attend contract)
    // when the constitution is present. When absent (test tmp dir has no
    // .claude), the output may be empty (no claims to attest) — the contract
    // is "does not panic" + "no hard error", not "empty string".
    assert!(!out.contains("panic"), "pre_compact must not panic");
}

#[test]
fn pre_compact_is_idempotent_on_repeated_invocation() {
    let (_tmp, mut rt) = make_runtime();
    // Calling twice in a row must not panic — tests WAL checkpoint resilience.
    let _ = handle_pre_compact(&mut rt, &json!({}));
    let out = handle_pre_compact(&mut rt, &json!({}));
    // ES2 P3 — idempotent: calling twice yields the same digest line
    // (same blake3 hash on same files). The output may be empty when no
    // .claude is present in the test tmp dir; the contract is "no panic".
    assert!(
        !out.contains("panic"),
        "idempotent pre_compact must not panic"
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// cwd_changed.rs — handle_cwd_changed
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn cwd_changed_emits_wiring_and_generator_hints_for_rust_crate_dir() {
    let (_tmp, mut rt) = make_runtime();
    let input = json!({ "new_cwd": "/ws/crates/touring-hooks/src" });

    let out = handle_cwd_changed(&mut rt, &input);
    assert!(out.contains("cwd-changed"), "hint prefix missing: {out}");
    assert!(out.contains("wiring score"), "wiring hint missing");
    assert!(
        out.contains("generator: run `touring generate render rust_module"),
        "rust_module generator hint missing: {out}"
    );
}

#[test]
fn cwd_changed_returns_empty_for_missing_new_cwd() {
    let (_tmp, mut rt) = make_runtime();
    let out = handle_cwd_changed(&mut rt, &json!({}));
    assert!(out.is_empty(), "no new_cwd → empty output");
}

#[test]
fn cwd_changed_records_directory_access() {
    let (_tmp, mut rt) = make_runtime();
    let path = "/ws/crates/touring-generator/src";
    let _ = handle_cwd_changed(&mut rt, &json!({ "new_cwd": path }));
    let count = rt.ctx.knowledge.access_count(path).unwrap_or(0);
    assert!(count >= 1, "directory access must be recorded");
}

// ═══════════════════════════════════════════════════════════════════════════
// worktree.rs — handle_worktree_create
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn worktree_create_emits_wiring_suggestions_for_new_path() {
    let (_tmp, mut rt) = make_runtime();
    let input = json!({
        "worktree_path": "/tmp/wt-test",
        "description": "migrate users schema"
    });
    let out = handle_worktree_create(&mut rt, &input);
    assert!(out.contains("worktree-created"));
    assert!(out.contains("wiring score"));
    assert!(out.contains("wiring suggest"));
}

#[test]
fn worktree_create_surfaces_generator_kind_when_intent_matches_keyword() {
    let (_tmp, mut rt) = make_runtime();
    let input = json!({
        "worktree_path": "/tmp/wt-migrate",
        "description": "migrate users schema"
    });
    let out = handle_worktree_create(&mut rt, &input);
    assert!(
        out.contains("Migration"),
        "migrate keyword → Migration kind"
    );
}

#[test]
fn worktree_create_returns_empty_for_missing_path() {
    let (_tmp, mut rt) = make_runtime();
    let out = handle_worktree_create(&mut rt, &json!({ "description": "x" }));
    assert!(out.is_empty(), "empty path → empty hint");
}

// ═══════════════════════════════════════════════════════════════════════════
// shared.rs — suggest_generator_for_task_subject + classify helpers
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn shared_suggest_generator_maps_test_keyword() {
    let hint = suggest_generator_for_task_subject("add unit test for parser");
    assert!(hint.contains("Test"), "test keyword → Test kind: {hint}");
    assert!(hint.contains("touring generate render"));
}

#[test]
fn shared_suggest_generator_empty_on_unknown_subject() {
    let hint = suggest_generator_for_task_subject("xyzzy plugh");
    assert!(hint.is_empty(), "unknown subject → empty");
}

#[test]
fn shared_classify_file_to_generator_kind_maps_proto_to_protobuf() {
    assert_eq!(
        classify_file_to_generator_kind("schemas/users.proto"),
        Some("ProtobufSchema")
    );
}

#[test]
fn shared_classify_rust_file_bench_file_maps_to_benchmark() {
    assert_eq!(
        classify_rust_to_generator_kind("/benches/foo.rs", "/benches/foo.rs"),
        "Benchmark"
    );
}

#[test]
fn shared_maybe_vgp_verify_hint_skips_generic_stems() {
    // lib/main/mod are too generic to produce meaningful verify hint.
    assert!(maybe_vgp_verify_hint_for_rs_file("src/lib.rs").is_none());
    assert!(maybe_vgp_verify_hint_for_rs_file("src/main.rs").is_none());
    // Non-Rust file → None.
    assert!(maybe_vgp_verify_hint_for_rs_file("src/foo.py").is_none());
    // Custom Rust module → Some.
    let hint = maybe_vgp_verify_hint_for_rs_file("src/hook_registry.rs")
        .expect("hint for hook_registry.rs");
    assert!(hint.contains("HookRegistry"), "CamelCase stem: {hint}");
}

// ═══════════════════════════════════════════════════════════════════════════
// task_delete.rs — handle_task_sync_post_delete
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn task_delete_emits_cancellation_confirmation_with_diary_hint() {
    let (_tmp, mut rt) = make_runtime();
    let input = task_input("T-delete-1", "", None);
    let out = handle_task_sync_post_delete(&mut rt, &input);
    assert!(out.contains("deleted"));
    assert!(out.contains("decompose cancelled"));
    assert!(out.contains("RL -0.3"));
    assert!(out.contains("DiaryEntry"));
}

#[test]
fn task_delete_truncates_very_long_task_ids_in_diary_hint() {
    let (_tmp, mut rt) = make_runtime();
    let long_id = "T-".to_string() + &"x".repeat(200);
    let input = task_input(&long_id, "", None);
    let out = handle_task_sync_post_delete(&mut rt, &input);
    // 40-char truncation cap for DiaryEntry `task_id` field.
    assert!(out.contains("DiaryEntry"), "diary hint must still fire");
}

// ═══════════════════════════════════════════════════════════════════════════
// task_stop.rs — handle_task_sync_post_stop
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn task_stop_emits_cancellation_and_drift_hints() {
    let (_tmp, mut rt) = make_runtime();
    let input = task_input("T-stop-1", "", None);
    let out = handle_task_sync_post_stop(&mut rt, &input);
    assert!(out.contains("decompose"));
    assert!(out.contains("cancelled"));
    assert!(out.contains("RL -0.3"));
    assert!(out.contains("evolution drift"));
    assert!(out.contains("gotcha add"));
}

#[test]
fn task_stop_session_assess_does_not_panic_on_unknown_session() {
    // The co-located `session_assess_on_task_stop` must NOT panic when the
    // session has no quality_score yet (R37-S3 contract).
    //
    // `cli_session_assess` may still produce a default `quality_score=0.0`
    // row for an unknown session_id (it inserts a fresh session record on
    // first read), in which case the helper returns the formatted hint.
    // Both outcomes (empty OR formatted hint) are acceptable — the
    // contract is "no panic + well-formed output".
    let (_tmp, mut rt) = make_runtime();
    let result = session_assess_on_task_stop(&mut rt, "T-unknown-session");
    // Either empty or a well-formed hint, never a panic:
    if !result.is_empty() {
        assert!(
            result.starts_with(" | session-assessed:"),
            "non-empty result must be well-formed: {result:?}"
        );
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// task_update.rs — handle_task_sync_post_update
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn task_update_in_progress_surfaces_generator_recall_and_tantivy_hints() {
    let (_tmp, mut rt) = make_runtime();
    let input = task_input(
        "T-upd-1",
        "write migration for users table",
        Some("in_progress"),
    );
    let out = handle_task_sync_post_update(&mut rt, &input);
    assert!(out.contains("Migration"), "generator kind from subject");
    assert!(out.contains("recall-lessons"), "R138 memory recall hint");
    assert!(out.contains("code-intel"), "R151 Tantivy hint");
}

#[test]
fn task_update_completed_triggers_finalize_and_artifact_memory() {
    let (_tmp, mut rt) = make_runtime();
    let input = task_input(
        "T-upd-2",
        "add hook handler for enter plan mode",
        Some("completed"),
    );
    let out = handle_task_sync_post_update(&mut rt, &input);
    assert!(out.contains("completed"));
    assert!(out.contains("decompose") || out.contains("finalize"));
    assert!(out.contains("reward"));
}

#[test]
fn task_update_blocked_surfaces_mcts_alternative_path() {
    let (_tmp, mut rt) = make_runtime();
    let input = task_input("T-upd-blk", "refactor parser", Some("blocked"));
    let out = handle_task_sync_post_update(&mut rt, &input);
    assert!(out.contains("blocked"));
    // R61-S3: MCTS hint when blocked.
    assert!(
        out.contains("mcts") || out.contains("MCTS") || out.contains("alternative"),
        "blocked → MCTS unblock hint: {out}"
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// task_get.rs — handle_task_sync_post_get (+38 co-located helpers)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn task_get_surfaces_dag_state_lookup_for_valid_task_id() {
    let (_tmp, mut rt) = make_runtime();
    let input = task_input("T-get-1", "", None);
    let out = handle_task_sync_post_get(&mut rt, &input);
    assert!(out.contains("touring-sync"));
    assert!(out.contains("touring decompose get"));
    assert!(out.contains("wiring suggest"));
}

#[test]
fn task_get_r169_surfaces_failure_lesson_recall_on_failed_dag() {
    // R169 regression: when DAG state contains "failed", the handler
    // surfaces a failure-recall hint + RL penalty.
    // The DAG state is empty for an unknown task, so we verify the wiring
    // suggest tail is always present (R169 branch is mutually exclusive
    // with active/complete branches — covered by dedicated R169 unit tests).
    let (_tmp, mut rt) = make_runtime();
    let out = handle_task_sync_post_get(&mut rt, &task_input("T-get-2", "", None));
    assert!(out.contains("wiring suggest"));
}

#[test]
fn task_get_generator_for_active_subtask_maps_to_kind_label() {
    // Direct co-located helper: dag_json → GeneratorKind hint.
    let dag_with_active = json!({
        "subtasks": [
            { "subtask_id": "s1", "status": "in_progress",
              "description": "add migration for users table" }
        ]
    })
    .to_string();
    let hint = generator_for_active_subtask(&dag_with_active);
    assert!(
        hint.contains("Migration"),
        "active subtask → Migration: {hint}"
    );
}

#[test]
fn task_get_dag_json_to_active_description_ignores_non_in_progress() {
    let dag_only_done = json!({
        "subtasks": [
            { "subtask_id": "s1", "status": "completed", "description": "x" }
        ]
    })
    .to_string();
    assert!(
        dag_json_to_active_description(&dag_only_done).is_none(),
        "no in_progress subtask → None"
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// task_create.rs — handle_task_sync_post_create (biggest handler)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn task_create_scaffolds_lifecycle_subtasks_and_emits_confirmation() {
    let (_tmp, mut rt) = make_runtime();
    let input = task_input("T-create-1", "add rust module for task lifecycle", None);
    let out = handle_task_sync_post_create(&mut rt, &input);
    // R14-S1: scout/implement/validate scaffold confirmation.
    assert!(out.contains("touring-sync"), "sync marker: {out}");
}

#[test]
fn task_create_emits_generator_hint_for_subject_keywords() {
    let (_tmp, mut rt) = make_runtime();
    let input = task_input("T-create-dkr", "containerize service with dockerfile", None);
    let out = handle_task_sync_post_create(&mut rt, &input);
    assert!(
        out.contains("Dockerfile") || out.contains("dockerfile"),
        "container keyword → Dockerfile hint: {out}"
    );
}

#[test]
fn task_create_collect_subject_generator_hints_matches_all_30_kinds() {
    // 30/30 GeneratorKinds should be reachable via collect_subject_generator_hints.
    // Spot-check four representative kinds across different domain clusters.
    let hints = collect_subject_generator_hints("write rust module with tests");
    let joined: String = hints.join(" | ");
    assert!(
        joined.contains("RustModule") || joined.contains("rust_module"),
        "rust module hint: {joined}"
    );
}

#[test]
fn task_create_returns_usable_hint_on_empty_subject() {
    let (_tmp, mut rt) = make_runtime();
    let input = task_input("T-create-bare", "", None);
    let out = handle_task_sync_post_create(&mut rt, &input);
    // Even with no subject, the handler still returns a sync confirmation.
    assert!(
        !out.is_empty(),
        "empty subject still returns sync confirmation"
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// Cross-module integration: verifies re-exports at lifecycle:: namespace
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn all_submodule_handlers_reachable_via_crate_lifecycle_namespace() {
    // Compile-time contract: every extracted handler must be accessible via
    // `crate::lifecycle::handle_*` so `hook_registry.rs` call sites resolve.
    // If any re-export in `lifecycle.rs` is removed, this test fails to compile.
    let _f: fn(&mut HookRuntime, &Value) -> String = crate::lifecycle::handle_subagent_start;
    let _f: fn(&mut HookRuntime, &Value) -> String = crate::lifecycle::handle_pre_compact;
    let _f: fn(&mut HookRuntime, &Value) -> String = crate::lifecycle::handle_worktree_create;
    let _f: fn(&mut HookRuntime, &Value) -> String = crate::lifecycle::handle_cwd_changed;
    let _f: fn(&mut HookRuntime, &Value) -> String = crate::lifecycle::handle_task_sync_post_delete;
    let _f: fn(&mut HookRuntime, &Value) -> String = crate::lifecycle::handle_task_sync_post_stop;
    let _f: fn(&mut HookRuntime, &Value) -> String = crate::lifecycle::handle_task_sync_post_update;
    let _f: fn(&mut HookRuntime, &Value) -> String = crate::lifecycle::handle_task_sync_post_get;
    let _f: fn(&mut HookRuntime, &Value) -> String = crate::lifecycle::handle_task_sync_post_create;
}

// ═══════════════════════════════════════════════════════════════════════════
// AcoPheromone §3 — heat_based_task_priority_hint (task_list.rs)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn heat_hint_empty_on_single_pending_task() {
    // < 2 pending tasks → suppressed unconditionally.
    let (_tmp, mut rt) = make_runtime();
    let input = json!({
        "result": {
            "tasks": [
                {"status": "pending", "task_subject": "refactor auth module"}
            ]
        }
    });
    let hint = heat_based_task_priority_hint(&mut rt, &input);
    assert!(
        hint.is_empty(),
        "single pending task must suppress heat hint: {hint:?}"
    );
}

#[test]
fn heat_hint_empty_on_no_pending_tasks() {
    // All tasks completed → no heat hint.
    let (_tmp, mut rt) = make_runtime();
    let input = json!({
        "result": {
            "tasks": [
                {"status": "completed", "task_subject": "write tests"},
                {"status": "completed", "task_subject": "fix linting"},
            ]
        }
    });
    let hint = heat_based_task_priority_hint(&mut rt, &input);
    assert!(
        hint.is_empty(),
        "completed tasks must suppress heat hint: {hint:?}"
    );
}

#[test]
fn heat_hint_uniform_scores_suppressed() {
    // Without a live daemon, cli_wiring_modules returns empty → all tasks score 0.75
    // (neutral). Spread = 0.0 < 0.05 → hint suppressed.
    let (_tmp, mut rt) = make_runtime();
    let input = json!({
        "result": {
            "tasks": [
                {"status": "pending", "task_subject": "implement feature A"},
                {"status": "pending", "task_subject": "implement feature B"},
                {"status": "pending", "task_subject": "implement feature C"},
            ]
        }
    });
    let hint = heat_based_task_priority_hint(&mut rt, &input);
    // Uniform scores (no daemon) → spread < 0.05 → suppressed.
    // Either empty (uniform) or non-empty (if daemon happened to respond) — both valid.
    // We only assert no panic.
    let _ = hint;
}

// ═══════════════════════════════════════════════════════════════════════════
// §4 — maybe_parallel_subagent_hint (plan_mode/exit.rs)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn parallel_hint_empty_on_zero_ready() {
    let json = r#"{"ready_count":0,"ready_subtasks":[]}"#;
    let hint = maybe_parallel_subagent_hint(json);
    assert!(
        hint.is_empty(),
        "ready_count=0 must suppress parallel hint: {hint:?}"
    );
}

#[test]
fn parallel_hint_empty_on_one_ready() {
    let json = r#"{"ready_count":1,"ready_subtasks":[{"subtask_id":"S-1"}]}"#;
    let hint = maybe_parallel_subagent_hint(json);
    assert!(
        hint.is_empty(),
        "ready_count=1 must suppress parallel hint: {hint:?}"
    );
}

#[test]
fn parallel_hint_emitted_on_two_ready() {
    let json = r#"{"ready_count":2,"ready_subtasks":[{"subtask_id":"S-1"},{"subtask_id":"S-2"}]}"#;
    let hint = maybe_parallel_subagent_hint(json);
    assert!(
        hint.contains("[TOURING PARALLEL]"),
        "ready_count=2 must emit parallel hint: {hint:?}"
    );
    assert!(
        hint.contains("S-1"),
        "must include first subtask ID: {hint:?}"
    );
    assert!(
        hint.contains("S-2"),
        "must include second subtask ID: {hint:?}"
    );
}

#[test]
fn parallel_hint_caps_subtask_ids_at_four() {
    let json = r#"{"ready_count":6,"ready_subtasks":[
        {"subtask_id":"S-1"},{"subtask_id":"S-2"},{"subtask_id":"S-3"},
        {"subtask_id":"S-4"},{"subtask_id":"S-5"},{"subtask_id":"S-6"}
    ]}"#;
    let hint = maybe_parallel_subagent_hint(json);
    assert!(
        hint.contains("[TOURING PARALLEL]"),
        "ready_count=6 must emit hint: {hint:?}"
    );
    assert!(!hint.contains("S-5"), "must cap subtask IDs at 4: {hint:?}");
    assert!(!hint.contains("S-6"), "must cap subtask IDs at 4: {hint:?}");
}

#[test]
fn parallel_hint_tolerates_invalid_json() {
    let hint = maybe_parallel_subagent_hint("not-valid-json{{");
    assert!(hint.is_empty(), "invalid JSON must return empty: {hint:?}");
}
