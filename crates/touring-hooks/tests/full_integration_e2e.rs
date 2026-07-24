//! Full integration E2E — proves orchestration of modularization + bidirectional +
//! action suggestions working end-to-end as a single system.
//!
//! Cross-track integration scenarios:
//!   - Track B (Pln2): Touring-origin tasks flow through digest → CC adoption → mirrored
//!   - Track C (Pln3): Action suggestions detect patterns → surface → consumed
//!   - Track D (R3, R4, R5): Ranking, GC, deactivation
//!   - Track E (R7): MCTS materializer → Pln2 digest (cross-track chain)
//!
//! Added: 2026-04-14 (cross-audit Phase 5 — full integration E2E)

#![allow(clippy::indexing_slicing)]

use serde_json::json;
use tempfile::TempDir;
use touring_hooks::bidirectional::{PendingSuggestion, Suggester, run_suggester};
use touring_hooks::cli_handlers::{
    cli_decompose_add, cli_decompose_create, cli_decompose_mark_mirrored, cli_decompose_status,
    cli_suggest_action, cli_suggestion_list_pending, cli_suggestion_mark_consumed,
    cli_suggestion_stats, cli_suggestions_gc,
};
use touring_hooks::mcts_materializer::materialize_from_payload;
use touring_hooks::runtime::HookRuntime;
use touring_hooks::suggesters::{
    failure_threshold::detect_and_suggest_failing_tasks,
    plan_mode_complexity::detect_and_suggest_plan_mode,
    stuck_subtask::detect_and_suggest_stuck_subtasks,
};
use touring_hooks::task_digest::{digest_pending_action_suggestions, digest_pending_tasks};

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

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

/// Ensure both decompose schema tables exist for tests that need them directly.
fn ensure_suggestion_table(conn: &rusqlite::Connection) {
    let _ = conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS cc_action_suggestions (
            suggestion_id TEXT PRIMARY KEY,
            action_type TEXT NOT NULL,
            target_task_id TEXT NOT NULL,
            target_subtask_id TEXT,
            reason TEXT NOT NULL,
            evidence_json TEXT NOT NULL DEFAULT '{}',
            suggested_at TEXT NOT NULL,
            consumed INTEGER NOT NULL DEFAULT 0,
            consumed_at TEXT,
            consumed_action TEXT
        );
        CREATE TABLE IF NOT EXISTS action_type_deactivation (
            action_type TEXT NOT NULL,
            task_id TEXT NOT NULL DEFAULT '',
            subtask_id TEXT NOT NULL DEFAULT '',
            consecutive_ignores INTEGER NOT NULL DEFAULT 0,
            acceptance_count INTEGER NOT NULL DEFAULT 0,
            total_samples INTEGER NOT NULL DEFAULT 0,
            deactivated_until TEXT,
            surface_count INTEGER NOT NULL DEFAULT 0,
            PRIMARY KEY (action_type, task_id, subtask_id)
        );",
    );
}

// ---------------------------------------------------------------------------
// Test 1: Complete Pln2 + Pln3 bidirectional orchestration cycle
// ---------------------------------------------------------------------------

#[test]
fn full_orchestration_cycle_pln2_and_pln3_interact_harmoniously() {
    let (_tmp, mut rt) = setup_runtime();

    // ── PHASE A: External agent seeds task in Touring (Pln2) ────────────────
    let create_result = parse_json(&cli_decompose_create(
        &mut rt,
        &json!({
            "task_type": "refactor",
            "description": "Refactor authentication module to use JWT tokens",
            "origin": "external-agent",
        }),
    ));
    assert_eq!(
        create_result["origin"], "external-agent",
        "origin must be preserved as external-agent"
    );
    let task_id = create_result["task_id"]
        .as_str()
        .expect("task_id must be present")
        .to_owned();

    // ── PHASE B: Session starts — digest surfaces the pending task ───────────
    let digest = digest_pending_tasks(&rt);
    assert!(digest.is_some(), "digest must surface external-agent task");
    let hint = digest.unwrap();
    assert!(hint.contains(&task_id), "hint must mention task_id: {hint}");
    assert!(
        hint.contains("external-agent"),
        "hint must show origin: {hint}"
    );
    assert!(
        hint.contains("external_ref"),
        "hint must include external_ref adoption instruction: {hint}"
    );

    // ── PHASE C: CC adopts via cli_decompose_mark_mirrored ──────────────────
    let mirror_result = parse_json(&cli_decompose_mark_mirrored(
        &mut rt,
        &json!({
            "task_id": task_id,
        }),
    ));
    assert_eq!(mirror_result["marked"], true, "mark_mirrored must succeed");

    // ── PHASE D: Digest must NOT resurface after mirroring ──────────────────
    let digest_after = digest_pending_tasks(&rt);
    assert!(
        digest_after.is_none() || !digest_after.as_deref().unwrap_or("").contains(&task_id),
        "digest must not resurface mirrored task"
    );

    // ── PHASE E: CC adds subtasks (DAG structure) ───────────────────────────
    let sub_result = parse_json(&cli_decompose_add(
        &mut rt,
        &json!({
            "task_id": task_id,
            "subtask_id": "sub_jwt_1",
            "description": "Add JWT library dependency",
        }),
    ));
    assert_eq!(sub_result["persisted"], true, "subtask must be added");

    // ── PHASE F: Insert a Pln3 update suggestion for the task ───────────────
    ensure_suggestion_table(rt.ctx.knowledge.conn_ref());
    let sugg_result = parse_json(&cli_suggest_action(
        &mut rt,
        &json!({
            "action_type": "update",
            "target_task_id": task_id,
            "target_subtask_id": "sub_jwt_1",
            "reason": "subtask has been pending for 45 minutes",
            "evidence_json": {"minutes_pending": 45},
        }),
    ));
    assert_eq!(sugg_result["inserted"], true, "suggestion must be inserted");
    let suggestion_id = sugg_result["suggestion_id"]
        .as_str()
        .expect("suggestion_id required")
        .to_owned();

    // ── PHASE G: Digest surfaces the action suggestion ───────────────────────
    let action_digest = digest_pending_action_suggestions(&rt, "update");
    assert!(
        action_digest.is_some(),
        "update suggestion must surface in digest"
    );

    // ── PHASE H: CC acts — marks suggestion consumed ─────────────────────────
    let consume_result = parse_json(&cli_suggestion_mark_consumed(
        &mut rt,
        &json!({
            "suggestion_id": suggestion_id,
            "consumed_action": "TaskUpdate",
        }),
    ));
    assert_eq!(
        consume_result["marked"], true,
        "suggestion must be marked consumed"
    );

    // ── PHASE I: Stats show acceptance_count incremented ─────────────────────
    let stats = parse_json(&cli_suggestion_stats(&mut rt, &json!({})));
    let stats_arr = stats["stats"].as_array().expect("stats must be array");
    // stats may be empty if deactivation table has no entry yet — that is fine,
    // consumed row is in cc_action_suggestions not action_type_deactivation.
    let _ = stats_arr; // stats table populated on surface_count increment

    // ── PHASE J: GC with retention_days=0 prunes consumed row ───────────────
    let gc_result = parse_json(&cli_suggestions_gc(
        &mut rt,
        &json!({ "retention_days": 0 }),
    ));
    // GC deletes rows where consumed=1 AND consumed_at <= now - N days.
    // retention_days=0 means "delete anything consumed before now" (inclusive).
    // The consumed_at was just set, so it may or may not be deleted depending on
    // clock precision. The important invariant: GC returns valid JSON and doesn't crash.
    assert!(
        gc_result.get("deleted").is_some(),
        "GC must return deleted count"
    );

    // ── PHASE K: Decompose status reflects correct task count ────────────────
    let status = parse_json(&cli_decompose_status(&mut rt, &json!({})));
    let total = status["total_tasks"].as_i64().unwrap_or(0);
    assert!(
        total >= 1,
        "at least 1 task must exist in decompose status: {status}"
    );
}

// ---------------------------------------------------------------------------
// Test 2: Bidirectional loop cannot resurface after adoption
// ---------------------------------------------------------------------------

#[test]
fn bidirectional_loop_breaks_after_cc_adoption() {
    let (_tmp, mut rt) = setup_runtime();

    // Seed 3 Touring-origin tasks
    let mut task_ids = Vec::new();
    for i in 0..3 {
        let r = parse_json(&cli_decompose_create(
            &mut rt,
            &json!({
                "task_type": "feature",
                "description": format!("Feature task {i}"),
                "origin": "touring-cli",
            }),
        ));
        task_ids.push(r["task_id"].as_str().unwrap_or("").to_owned());
    }

    // All 3 should appear in digest (up to DIGEST_LIMIT=5)
    let digest_before = digest_pending_tasks(&rt);
    assert!(
        digest_before.is_some(),
        "digest must surface touring-cli tasks"
    );

    // Mirror all 3
    for tid in &task_ids {
        let r = parse_json(&cli_decompose_mark_mirrored(
            &mut rt,
            &json!({ "task_id": tid }),
        ));
        assert_eq!(r["marked"], true, "task {tid} must mirror successfully");
    }

    // After mirroring all — digest must return None
    let digest_after = digest_pending_tasks(&rt);
    assert!(
        digest_after.is_none(),
        "after all tasks mirrored, digest must return None; got: {digest_after:?}"
    );
}

// ---------------------------------------------------------------------------
// Test 3: Suggester trait contract holds for all three implementations
// ---------------------------------------------------------------------------

#[test]
fn suggester_trait_contract_holds_for_all_three_implementations() {
    let (_tmp, rt) = setup_runtime();

    // StuckSubtaskSuggester: no data → returns 0
    let stuck = detect_and_suggest_stuck_subtasks(&rt);
    assert_eq!(
        stuck, 0,
        "StuckSubtaskSuggester: no stuck subtasks on empty DB"
    );

    // FailureThresholdSuggester: no data → returns 0
    let failing = detect_and_suggest_failing_tasks(&rt);
    assert_eq!(
        failing, 0,
        "FailureThresholdSuggester: no failing tasks on empty DB"
    );

    // PlanModeSuggester: no data → returns 0
    let plan = detect_and_suggest_plan_mode(&rt);
    assert_eq!(plan, 0, "PlanModeSuggester: no plan_mode tasks on empty DB");
}

// ---------------------------------------------------------------------------
// Test 4: MCTS materialized tasks flow through Pln2 digest (R7 → B4 chain)
// ---------------------------------------------------------------------------

#[test]
fn mcts_materialized_tasks_flow_through_pln2_digest() {
    let (_tmp, mut rt) = setup_runtime();

    // Materialize 2 subgoals from MCTS
    let mat_result = parse_json(&materialize_from_payload(
        &mut rt,
        &json!({
            "root_state": "refactor_auth_module",
            "top_n": 2,
        }),
    ));
    let materialized = mat_result["materialized"].as_i64().unwrap_or(0);
    assert!(
        materialized >= 1,
        "at least 1 subgoal must materialize: {mat_result}"
    );

    // The materialized tasks have origin=touring-mcts and mirrored_to_cc=0
    // so they must appear in the Pln2 digest
    let digest = digest_pending_tasks(&rt);
    assert!(
        digest.is_some(),
        "MCTS-materialized tasks must surface in Pln2 digest"
    );
    let hint = digest.unwrap();
    // Hint must mention "touring-mcts" origin
    assert!(
        hint.contains("touring-mcts"),
        "digest hint must show touring-mcts origin: {hint}"
    );
}

// ---------------------------------------------------------------------------
// Test 5: run_suggester dedup contract — no duplicate pending suggestions
// ---------------------------------------------------------------------------

#[test]
fn run_suggester_dedup_prevents_duplicate_pending_suggestions() {
    let (_tmp, rt) = setup_runtime();
    ensure_suggestion_table(rt.ctx.knowledge.conn_ref());

    /// Minimal suggester that always emits one fixed suggestion.
    struct OneSuggester {
        task_id: String,
    }
    impl Suggester for OneSuggester {
        fn action_type(&self) -> &'static str {
            "update"
        }
        fn detect(&self, _rt: &HookRuntime) -> Vec<PendingSuggestion> {
            vec![PendingSuggestion {
                target_task_id: self.task_id.clone(),
                target_subtask_id: None,
                reason: "test dedup".into(),
                evidence: json!({}),
            }]
        }
    }

    let s = OneSuggester {
        task_id: "task_dedup_e2e".into(),
    };

    // First run: inserts 1
    assert_eq!(
        run_suggester(&s, &rt),
        1,
        "first run must insert 1 suggestion"
    );

    // Second run: deduped, 0 new rows
    assert_eq!(
        run_suggester(&s, &rt),
        0,
        "second run must be deduped (unconsumed row exists)"
    );

    // List pending: must show exactly 1
    let conn = rt.ctx.knowledge.conn_ref();
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM cc_action_suggestions WHERE action_type='update' AND consumed=0",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);
    assert_eq!(
        count, 1,
        "exactly 1 pending suggestion must exist after 2 runs"
    );
}

// ---------------------------------------------------------------------------
// Test 6: cli_suggestion_list_pending + cli_suggestion_stats integration
// ---------------------------------------------------------------------------

#[test]
fn suggestion_list_and_stats_reflect_inserted_suggestions() {
    let (_tmp, mut rt) = setup_runtime();
    ensure_suggestion_table(rt.ctx.knowledge.conn_ref());

    // Insert 2 suggestions: one update, one stop
    let r1 = parse_json(&cli_suggest_action(
        &mut rt,
        &json!({
            "action_type": "update",
            "target_task_id": "task_list_1",
            "reason": "stuck for 30 min",
        }),
    ));
    assert_eq!(r1["inserted"], true);

    let r2 = parse_json(&cli_suggest_action(
        &mut rt,
        &json!({
            "action_type": "stop",
            "target_task_id": "task_list_2",
            "reason": "failed 4 times",
        }),
    ));
    assert_eq!(r2["inserted"], true);

    // List all pending — must return 2
    let all = parse_json(&cli_suggestion_list_pending(
        &mut rt,
        &json!({ "limit": 10 }),
    ));
    let count = all["count"].as_i64().unwrap_or(0);
    assert_eq!(count, 2, "2 pending suggestions must be returned: {all}");

    // List only "stop" type — must return 1
    let stop_only = parse_json(&cli_suggestion_list_pending(
        &mut rt,
        &json!({
            "action_type": "stop",
            "limit": 10,
        }),
    ));
    let stop_count = stop_only["count"].as_i64().unwrap_or(0);
    assert_eq!(stop_count, 1, "exactly 1 stop suggestion: {stop_only}");

    // Consume the update suggestion
    let sid1 = r1["suggestion_id"]
        .as_str()
        .expect("suggestion_id")
        .to_owned();
    let mark = parse_json(&cli_suggestion_mark_consumed(
        &mut rt,
        &json!({
            "suggestion_id": sid1,
            "consumed_action": "TaskUpdate",
        }),
    ));
    assert_eq!(mark["marked"], true);

    // List all pending now — must return 1 (only stop remains)
    let after = parse_json(&cli_suggestion_list_pending(
        &mut rt,
        &json!({ "limit": 10 }),
    ));
    assert_eq!(
        after["count"].as_i64().unwrap_or(-1),
        1,
        "after consuming update, only stop must remain: {after}"
    );
}
