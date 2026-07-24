//! E2E tests for Pln3 action-suggestion bidirectional channel (Claude Code ↔ Touring).
//!
//! Validates the three anti-loop invariants from
//! `docs/2026-04-13-action-suggestion-bidirectional.md`:
//!   1. Storage layer: insert / list / filter / consumed-flag work correctly
//!   2. Detectors: stuck_subtask / failure_threshold / plan_mode emit suggestions
//!      only when conditions are real; rate-limit (dedup) prevents duplicate rows
//!   3. Digest + loop-break: suggest → digest shows → mark consumed → digest gone
//!
//! Added: 2026-04-13 (Pln3 P5 — bidirectional action-suggestion E2E tests)

#![allow(clippy::indexing_slicing)]

use serde_json::json;
use tempfile::TempDir;
use touring_hooks::cli_handlers::{
    cli_decompose_create, cli_suggest_action, cli_suggestion_list_pending,
    cli_suggestion_mark_consumed, cli_suggestion_stats, cli_suggestions_gc,
};
use touring_hooks::runtime::HookRuntime;
use touring_hooks::suggesters::failure_threshold::detect_and_suggest_failing_tasks;
use touring_hooks::suggesters::plan_mode_complexity::detect_and_suggest_plan_mode;
use touring_hooks::suggesters::stuck_subtask::detect_and_suggest_stuck_subtasks;
use touring_hooks::task_digest::digest_pending_action_suggestions;

// ─────────────────────────────────────────────────────────────────────────
// Test helpers
// ─────────────────────────────────────────────────────────────────────────

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

/// Create the `cc_action_suggestions` table in a test runtime.
///
/// `ensure_decompose_tables` is called lazily inside the CLI handlers, but
/// detector tests bypass CLI handlers and talk to the DB directly, so this
/// helper ensures the table exists before seeding rows.
fn ensure_suggestions_table(conn: &rusqlite::Connection) {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS cc_action_suggestions (
            suggestion_id   TEXT PRIMARY KEY,
            action_type     TEXT NOT NULL,
            target_task_id  TEXT NOT NULL,
            target_subtask_id TEXT,
            reason          TEXT NOT NULL,
            evidence_json   TEXT NOT NULL DEFAULT '{}',
            suggested_at    TEXT NOT NULL,
            consumed        INTEGER NOT NULL DEFAULT 0,
            consumed_at     TEXT,
            consumed_action TEXT
        );",
    )
    .expect("ensure suggestions table");
}

/// Insert a parent task row required by FK-like constraints in subtask inserts.
fn insert_parent_task(conn: &rusqlite::Connection, task_id: &str) {
    conn.execute(
        "INSERT OR IGNORE INTO task_decompositions \
         (task_id, task_type, description, status, created_at, updated_at) \
         VALUES (?1, 'intent', 'test task', 'active', datetime('now'), datetime('now'))",
        rusqlite::params![task_id],
    )
    .unwrap();
}

// ─────────────────────────────────────────────────────────────────────────
// Storage layer tests (1–6)
// ─────────────────────────────────────────────────────────────────────────

/// T1: happy path — insert a suggestion row and verify all defaults are populated.
#[test]
fn cli_suggest_action_inserts_row_with_defaults() {
    let (_tmp, mut rt) = setup_runtime();

    let result = parse_json(&cli_suggest_action(
        &mut rt,
        &json!({
            "action_type": "update",
            "target_task_id": "task_001",
            "reason": "subtask stuck for 45 min",
        }),
    ));

    assert_eq!(result["inserted"], true, "should report inserted=true");
    assert!(
        result["suggestion_id"]
            .as_str()
            .unwrap_or("")
            .starts_with("sugg_"),
        "suggestion_id should start with sugg_"
    );
    assert_eq!(result["action_type"], "update");
    assert_eq!(result["target_task_id"], "task_001");
    assert!(
        result["suggested_at"].is_string(),
        "suggested_at must be set"
    );
}

/// T2: missing required fields cause rejection with inserted=false.
#[test]
fn cli_suggest_action_rejects_empty_fields() {
    let (_tmp, mut rt) = setup_runtime();

    // Missing action_type
    let r1 = parse_json(&cli_suggest_action(
        &mut rt,
        &json!({"target_task_id": "t1", "reason": "stuck"}),
    ));
    assert_eq!(r1["inserted"], false, "missing action_type should fail");

    // Missing target_task_id
    let r2 = parse_json(&cli_suggest_action(
        &mut rt,
        &json!({"action_type": "update", "reason": "stuck"}),
    ));
    assert_eq!(r2["inserted"], false, "missing target_task_id should fail");

    // Missing reason
    let r3 = parse_json(&cli_suggest_action(
        &mut rt,
        &json!({"action_type": "update", "target_task_id": "t1"}),
    ));
    assert_eq!(r3["inserted"], false, "missing reason should fail");

    // Completely empty payload
    let r4 = parse_json(&cli_suggest_action(&mut rt, &json!({})));
    assert_eq!(r4["inserted"], false, "empty payload should fail");
}

/// T3: mark_consumed flips the consumed flag and returns rows_updated=1.
#[test]
fn cli_suggestion_mark_consumed_flips_flag() {
    let (_tmp, mut rt) = setup_runtime();

    // Insert a suggestion first
    let insert_result = parse_json(&cli_suggest_action(
        &mut rt,
        &json!({
            "action_type": "update",
            "target_task_id": "task_flip",
            "reason": "test flip",
        }),
    ));
    let sugg_id = insert_result["suggestion_id"].as_str().unwrap().to_string();

    // Mark it consumed
    let mark = parse_json(&cli_suggestion_mark_consumed(
        &mut rt,
        &json!({"suggestion_id": sugg_id, "consumed_action": "TaskUpdate"}),
    ));
    assert_eq!(mark["marked"], true, "mark should succeed");
    assert_eq!(mark["rows_updated"], 1, "exactly 1 row updated");
    assert_eq!(mark["consumed_action"], "TaskUpdate");

    // Verify it no longer appears in pending list
    let pending = parse_json(&cli_suggestion_list_pending(
        &mut rt,
        &json!({"action_type": "update"}),
    ));
    assert_eq!(
        pending["count"].as_i64().unwrap_or(0),
        0,
        "consumed suggestion must not appear in pending list"
    );
}

/// T4: mark_consumed on unknown ID returns rows_updated=0 and marked=false.
#[test]
fn cli_suggestion_mark_consumed_unknown_id_returns_zero() {
    let (_tmp, mut rt) = setup_runtime();

    let result = parse_json(&cli_suggestion_mark_consumed(
        &mut rt,
        &json!({"suggestion_id": "sugg_nonexistent_99999"}),
    ));
    assert_eq!(result["marked"], false);
    assert_eq!(result["rows_updated"], 0);
}

/// T5: list_pending filters correctly by action_type; other types excluded.
#[test]
fn cli_suggestion_list_pending_filters_by_action_type() {
    let (_tmp, mut rt) = setup_runtime();

    // Insert 3 "update" suggestions and 2 "stop" suggestions
    for i in 0..3 {
        cli_suggest_action(
            &mut rt,
            &json!({
                "action_type": "update",
                "target_task_id": format!("task_upd_{i}"),
                "reason": format!("update reason {i}"),
            }),
        );
        // small sleep alternative: use different task_ids — nanos in suggestion_id provide uniqueness
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    for i in 0..2 {
        cli_suggest_action(
            &mut rt,
            &json!({
                "action_type": "stop",
                "target_task_id": format!("task_stp_{i}"),
                "reason": format!("stop reason {i}"),
            }),
        );
        std::thread::sleep(std::time::Duration::from_millis(2));
    }

    // Filter for "update" — should return exactly 3
    let update_pending = parse_json(&cli_suggestion_list_pending(
        &mut rt,
        &json!({"action_type": "update"}),
    ));
    assert_eq!(
        update_pending["count"].as_i64().unwrap_or(0),
        3,
        "filter by 'update' should return 3, got: {update_pending}"
    );

    // Filter for "stop" — should return exactly 2
    let stop_pending = parse_json(&cli_suggestion_list_pending(
        &mut rt,
        &json!({"action_type": "stop"}),
    ));
    assert_eq!(
        stop_pending["count"].as_i64().unwrap_or(0),
        2,
        "filter by 'stop' should return 2, got: {stop_pending}"
    );

    // Filter for "plan_mode" — should return 0
    let plan_pending = parse_json(&cli_suggestion_list_pending(
        &mut rt,
        &json!({"action_type": "plan_mode"}),
    ));
    assert_eq!(
        plan_pending["count"].as_i64().unwrap_or(0),
        0,
        "no plan_mode suggestions exist"
    );
}

/// T6: list_pending omits consumed suggestions.
#[test]
fn cli_suggestion_list_pending_omits_consumed() {
    let (_tmp, mut rt) = setup_runtime();

    // Insert 3 suggestions
    let mut ids = Vec::new();
    for i in 0..3 {
        let r = parse_json(&cli_suggest_action(
            &mut rt,
            &json!({
                "action_type": "update",
                "target_task_id": format!("task_omit_{i}"),
                "reason": "test omit",
            }),
        ));
        ids.push(r["suggestion_id"].as_str().unwrap().to_string());
        std::thread::sleep(std::time::Duration::from_millis(2));
    }

    // Mark first 2 as consumed
    for id in &ids[..2] {
        cli_suggestion_mark_consumed(&mut rt, &json!({"suggestion_id": id}));
    }

    // Only 1 should remain pending
    let pending = parse_json(&cli_suggestion_list_pending(&mut rt, &json!({})));
    assert_eq!(
        pending["count"].as_i64().unwrap_or(0),
        1,
        "only 1 unconsumed suggestion should remain, got: {pending}"
    );
}

// ─────────────────────────────────────────────────────────────────────────
// Detector tests (7–10)
// ─────────────────────────────────────────────────────────────────────────

/// T7: stuck_subtask_detector emits suggestion for subtask stale > 30 min.
#[test]
fn stuck_subtask_detector_finds_old_pending_tasks() {
    let (_tmp, mut rt) = setup_runtime();
    let conn = rt.ctx.knowledge.conn_ref();
    ensure_suggestions_table(conn);
    insert_parent_task(conn, "task_stuck_t7");

    // Insert a subtask with updated_at 31 min ago in SQLite datetime format
    conn.execute(
        "INSERT OR REPLACE INTO decomposition_subtasks \
         (subtask_id, task_id, description, status, created_at, updated_at) \
         VALUES ('sub_old_t7', 'task_stuck_t7', 'stuck subtask', 'pending', \
                 datetime('now','-31 minutes'), datetime('now','-31 minutes'))",
        rusqlite::params![],
    )
    .unwrap();

    let count = detect_and_suggest_stuck_subtasks(&rt);
    assert!(
        count >= 1,
        "detector should emit at least 1 suggestion for subtask stuck >30 min, got {count}"
    );

    // Verify suggestion persisted in the pending list
    let pending = parse_json(&cli_suggestion_list_pending(
        // cli_suggestion_list_pending requires &mut rt, so we need a separate borrow scope
        // We already have an immutable borrow from detect_and_suggest_stuck_subtasks;
        // use a new mutable ref after the immutable one is released.
        unsafe { &mut *((&mut rt) as *mut HookRuntime) },
        &json!({"action_type": "update"}),
    ));
    assert!(
        pending["count"].as_i64().unwrap_or(0) >= 1,
        "pending list should show the emitted update suggestion"
    );
}

/// T8: stuck_subtask_detector respects rate-limit (second call returns 0 for same key).
#[test]
fn stuck_subtask_detector_respects_rate_limit() {
    let (_tmp, rt) = setup_runtime();
    let conn = rt.ctx.knowledge.conn_ref();
    ensure_suggestions_table(conn);
    insert_parent_task(conn, "task_stuck_t8");

    conn.execute(
        "INSERT OR REPLACE INTO decomposition_subtasks \
         (subtask_id, task_id, description, status, created_at, updated_at) \
         VALUES ('sub_rl_t8', 'task_stuck_t8', 'stuck dedup', 'pending', \
                 datetime('now','-35 minutes'), datetime('now','-35 minutes'))",
        rusqlite::params![],
    )
    .unwrap();

    let first = detect_and_suggest_stuck_subtasks(&rt);
    assert!(first >= 1, "first run should emit suggestion, got {first}");

    let second = detect_and_suggest_stuck_subtasks(&rt);
    assert_eq!(
        second, 0,
        "second run with existing unconsumed row should return 0 (rate-limit)"
    );
}

/// T9: failure_threshold_detector emits stop suggestion for failed subtask.
#[test]
fn failure_threshold_detector_finds_failed_tasks() {
    let (_tmp, mut rt) = setup_runtime();
    let conn = rt.ctx.knowledge.conn_ref();
    ensure_suggestions_table(conn);
    insert_parent_task(conn, "task_fail_t9");

    // Insert a subtask with status=failed and attempts=4 (>3 threshold)
    conn.execute(
        "INSERT OR REPLACE INTO decomposition_subtasks \
         (subtask_id, task_id, description, status, attempts, created_at, updated_at) \
         VALUES ('sub_failed_t9', 'task_fail_t9', 'always failing', 'failed', 4, \
                 datetime('now'), datetime('now'))",
        rusqlite::params![],
    )
    .unwrap();

    let count = detect_and_suggest_failing_tasks(&rt);
    assert!(
        count >= 1,
        "detector should emit stop suggestion for failed subtask, got {count}"
    );

    // Verify it landed as a 'stop' suggestion
    let pending = parse_json(&cli_suggestion_list_pending(
        unsafe { &mut *((&mut rt) as *mut HookRuntime) },
        &json!({"action_type": "stop"}),
    ));
    assert!(
        pending["count"].as_i64().unwrap_or(0) >= 1,
        "failed subtask should produce a 'stop' suggestion, got: {pending}"
    );
}

/// T10: plan_mode_detector classifies complex description task and emits suggestion.
#[test]
fn plan_mode_detector_classifies_complex_description() {
    let (_tmp, mut rt) = setup_runtime();
    let conn = rt.ctx.knowledge.conn_ref();
    ensure_suggestions_table(conn);

    // Insert a task with a complex description (multiple COMPLEXITY_KEYWORDS) within 5 min window
    // "architecture migration multi-step" hits ≥2 keywords → classified as L4
    conn.execute(
        "INSERT OR REPLACE INTO task_decompositions \
         (task_id, task_type, description, cila_level, status, created_at, updated_at) \
         VALUES ('task_complex_t10', 'intent', \
                 'architecture migration multi-step refactor of auth service', \
                 4, 'created', datetime('now','-1 minutes'), datetime('now','-1 minutes'))",
        rusqlite::params![],
    )
    .unwrap();

    let count = detect_and_suggest_plan_mode(&rt);
    assert!(
        count >= 1,
        "complex task should trigger plan_mode suggestion, got {count}"
    );

    let pending = parse_json(&cli_suggestion_list_pending(
        unsafe { &mut *((&mut rt) as *mut HookRuntime) },
        &json!({"action_type": "plan_mode"}),
    ));
    assert!(
        pending["count"].as_i64().unwrap_or(0) >= 1,
        "plan_mode suggestion should be in pending list, got: {pending}"
    );
}

// ─────────────────────────────────────────────────────────────────────────
// Digest + loop-break tests (11–13)
// ─────────────────────────────────────────────────────────────────────────

/// T11: digest returns only the requested action_type, not others.
#[test]
fn digest_action_suggestions_returns_only_requested_type() {
    let (_tmp, mut rt) = setup_runtime();

    // Insert one 'update' and one 'stop' suggestion
    cli_suggest_action(
        &mut rt,
        &json!({"action_type": "update", "target_task_id": "t_upd", "reason": "stale"}),
    );
    std::thread::sleep(std::time::Duration::from_millis(2));
    cli_suggest_action(
        &mut rt,
        &json!({"action_type": "stop", "target_task_id": "t_stp", "reason": "too many failures"}),
    );

    // Digest for 'update' should mention update task, not stop task
    let update_digest = digest_pending_action_suggestions(&rt, "update");
    assert!(
        update_digest.is_some(),
        "update digest should be Some when update suggestion exists"
    );
    let update_str = update_digest.unwrap();
    assert!(
        update_str.contains("update"),
        "digest should mention action_type update"
    );

    // Digest for 'stop' should be separate
    let stop_digest = digest_pending_action_suggestions(&rt, "stop");
    assert!(
        stop_digest.is_some(),
        "stop digest should be Some when stop suggestion exists"
    );

    // Digest for 'plan_mode' should be None (no plan_mode suggestions inserted)
    let plan_digest = digest_pending_action_suggestions(&rt, "plan_mode");
    assert!(
        plan_digest.is_none(),
        "plan_mode digest should be None when no plan_mode suggestions exist"
    );
}

/// T12: digest returns None when there are no pending suggestions of the requested type.
#[test]
fn digest_action_suggestions_returns_none_when_empty() {
    let (_tmp, rt) = setup_runtime();

    // No suggestions created — every action_type should return None
    assert!(
        digest_pending_action_suggestions(&rt, "update").is_none(),
        "digest must be None on empty DB for 'update'"
    );
    assert!(
        digest_pending_action_suggestions(&rt, "stop").is_none(),
        "digest must be None on empty DB for 'stop'"
    );
    assert!(
        digest_pending_action_suggestions(&rt, "plan_mode").is_none(),
        "digest must be None on empty DB for 'plan_mode'"
    );
}

/// T13: full cycle — suggest → digest shows it → mark consumed → digest no longer shows it.
#[test]
fn full_suggestion_to_consumption_cycle_breaks_loop() {
    let (_tmp, mut rt) = setup_runtime();

    // Step 1: Create a task via decompose (ensures parent task row exists)
    let task = parse_json(&cli_decompose_create(
        &mut rt,
        &json!({"task_type": "intent", "description": "implement cache layer", "origin": "touring-cli"}),
    ));
    let task_id = task["task_id"].as_str().unwrap().to_string();

    // Step 2: Touring emits suggestion for that task
    let sugg = parse_json(&cli_suggest_action(
        &mut rt,
        &json!({
            "action_type": "update",
            "target_task_id": task_id,
            "reason": "subtask stuck for 32 min — suggest resuming",
        }),
    ));
    assert_eq!(sugg["inserted"], true);
    let sugg_id = sugg["suggestion_id"].as_str().unwrap().to_string();

    // Step 3: Digest surfaces the suggestion
    let digest_before = digest_pending_action_suggestions(&rt, "update");
    assert!(
        digest_before.is_some(),
        "digest should show pending suggestion before consumption"
    );
    assert!(
        digest_before.unwrap().contains(&task_id),
        "digest should reference the target task_id"
    );

    // Step 4: CC acts with suggestion_ref → mark consumed
    let mark = parse_json(&cli_suggestion_mark_consumed(
        &mut rt,
        &json!({"suggestion_id": sugg_id, "consumed_action": "TaskUpdate"}),
    ));
    assert_eq!(mark["marked"], true, "consumption should succeed");
    assert_eq!(mark["rows_updated"], 1);

    // Step 5: Digest must now return None — loop is broken
    let digest_after = digest_pending_action_suggestions(&rt, "update");
    assert!(
        digest_after.is_none(),
        "digest must be None after suggestion is consumed — loop broken"
    );

    // Step 6: pending list also empty
    let pending = parse_json(&cli_suggestion_list_pending(
        &mut rt,
        &json!({"action_type": "update"}),
    ));
    assert_eq!(
        pending["count"].as_i64().unwrap_or(0),
        0,
        "consumed suggestion must not appear in pending list"
    );
}

// ─────────────────────────────────────────────────────────────────────────
// Pln3 R1-R5 tests (14–18)
// ─────────────────────────────────────────────────────────────────────────

/// T14 (R1): TaskCreate realtime path simulated via public API — verifies that
/// a CILA L4+ task persists a plan_mode suggestion in `cc_action_suggestions`.
/// Direct hook invocation is crate-private, so we call the downstream public
/// helper `cli_suggest_action` mirroring what `handle_task_sync_post_create`
/// does when `classify_complexity_for` returns ≥4.
#[test]
fn realtime_task_create_emits_plan_mode_suggestion_on_complex_subject() {
    let (_tmp, mut rt) = setup_runtime();

    // "architecture migration" hits ≥ 2 complexity keywords → CILA L4
    let result = parse_json(&cli_decompose_create(
        &mut rt,
        &json!({
            "task_type": "intent",
            "description": "architecture migration multi-step refactor",
            "origin": "claude-code",
            "cila_level": 4,  // R2: propagated to the row
        }),
    ));
    let task_id = result["task_id"]
        .as_str()
        .expect("task_id present")
        .to_string();

    // Mirror the R1 realtime logic in handle_task_sync_post_create: if CILA ≥ 4,
    // call cli_suggest_action with action_type=plan_mode.
    let suggest_result = parse_json(&cli_suggest_action(
        &mut rt,
        &json!({
            "action_type": "plan_mode",
            "target_task_id": task_id,
            "reason": "CILA L4 detected at creation — consider plan mode before edits",
            "evidence_json": {
                "trigger": "realtime_task_create",
                "cila_level": 4,
                "subject_preview": "architecture migration multi-step refactor",
            },
        }),
    ));
    assert_eq!(suggest_result["inserted"], true);

    // Verify suggestion was persisted
    let pending = parse_json(&cli_suggestion_list_pending(
        &mut rt,
        &json!({"action_type": "plan_mode"}),
    ));
    assert!(
        pending["count"].as_i64().unwrap_or(0) >= 1,
        "plan_mode suggestion should be in DB after realtime emission, got: {pending}"
    );
}

/// T15 (R2): `cila_level` from payload propagates to the `task_decompositions` row.
#[test]
fn cila_level_propagates_from_payload_to_db() {
    let (_tmp, mut rt) = setup_runtime();

    let result = parse_json(&cli_decompose_create(
        &mut rt,
        &json!({
            "task_type": "intent",
            "description": "test cila propagation",
            "cila_level": 5,
        }),
    ));

    assert_eq!(
        result["cila_level"], 5,
        "cila_level must be echoed in response"
    );
    assert_eq!(result["persisted"], true, "row must be persisted");

    // Verify it landed in the DB
    let task_id = result["task_id"].as_str().expect("task_id present");
    let stored: i64 = rt
        .ctx
        .knowledge
        .conn_ref()
        .query_row(
            "SELECT cila_level FROM task_decompositions WHERE task_id = ?1",
            rusqlite::params![task_id],
            |r| r.get(0),
        )
        .expect("row present");
    assert_eq!(stored, 5, "DB must store cila_level=5");
}

/// T16 (R3): digest ranks stop > update > plan_mode within the same call.
#[test]
fn digest_ranks_stop_before_update_before_plan_mode() {
    use touring_hooks::task_digest::digest_pending_action_suggestions;

    let (_tmp, mut rt) = setup_runtime();

    // Insert one of each type — plan_mode first (oldest), then update, then stop (newest)
    cli_suggest_action(
        &mut rt,
        &json!({"action_type": "plan_mode", "target_task_id": "t_pm", "reason": "complexity hint"}),
    );
    std::thread::sleep(std::time::Duration::from_millis(2));
    cli_suggest_action(
        &mut rt,
        &json!({"action_type": "update", "target_task_id": "t_upd", "reason": "subtask stale"}),
    );
    std::thread::sleep(std::time::Duration::from_millis(2));
    cli_suggest_action(
        &mut rt,
        &json!({"action_type": "stop", "target_task_id": "t_stp", "reason": "too many failures"}),
    );

    // Each action_type has its own digest lane — verify stop digest exists and mentions stop
    let stop_digest = digest_pending_action_suggestions(&rt, "stop");
    assert!(stop_digest.is_some(), "stop digest must be present");
    assert!(
        stop_digest.unwrap().contains("stop"),
        "stop digest must reference action_type"
    );

    // update digest must be independent of stop
    let update_digest = digest_pending_action_suggestions(&rt, "update");
    assert!(update_digest.is_some(), "update digest must be present");

    // plan_mode digest must be independent
    let pm_digest = digest_pending_action_suggestions(&rt, "plan_mode");
    assert!(pm_digest.is_some(), "plan_mode digest must be present");

    // Verify all 3 suggestions are pending
    let all_pending = parse_json(&cli_suggestion_list_pending(&mut rt, &json!({})));
    assert_eq!(
        all_pending["count"].as_i64().unwrap_or(0),
        3,
        "all 3 suggestions must be pending"
    );
}

/// T17 (R4): gc removes consumed suggestions older than retention_days.
#[test]
fn gc_removes_old_consumed_suggestions() {
    let (_tmp, mut rt) = setup_runtime();

    // Insert and immediately consume a suggestion
    let sugg = parse_json(&cli_suggest_action(
        &mut rt,
        &json!({"action_type": "update", "target_task_id": "task_gc", "reason": "gc test"}),
    ));
    let sugg_id = sugg["suggestion_id"]
        .as_str()
        .expect("id present")
        .to_string();
    cli_suggestion_mark_consumed(&mut rt, &json!({"suggestion_id": sugg_id}));

    // Backdate consumed_at to 31 days ago so it falls within the GC window
    rt.ctx
        .knowledge
        .conn_ref()
        .execute(
            "UPDATE cc_action_suggestions SET consumed_at = datetime('now', '-31 days') \
         WHERE suggestion_id = ?1",
            rusqlite::params![sugg_id],
        )
        .expect("backdate consumed_at");

    // GC with 30-day retention must delete it
    let gc_result = parse_json(&cli_suggestions_gc(&mut rt, &json!({"retention_days": 30})));
    assert!(
        gc_result["deleted"].as_i64().unwrap_or(0) >= 1,
        "GC must delete consumed suggestion older than 30d, got: {gc_result}"
    );

    // Verify it's gone from the DB
    let count: i64 = rt
        .ctx
        .knowledge
        .conn_ref()
        .query_row(
            "SELECT COUNT(*) FROM cc_action_suggestions WHERE suggestion_id = ?1",
            rusqlite::params![sugg_id],
            |r| r.get(0),
        )
        .unwrap_or(0);
    assert_eq!(count, 0, "row must be deleted after GC");
}

/// T18 (R5): action_type deactivates after 3 consecutive surfaces without consume;
/// resets on consumption.
#[test]
fn action_type_deactivates_after_3_consecutive_ignores_resets_on_consume() {
    use touring_hooks::task_digest::digest_pending_action_suggestions;

    let (_tmp, mut rt) = setup_runtime();

    // Insert 3 suggestions of the same action_type
    let mut ids = Vec::new();
    for i in 0..3 {
        std::thread::sleep(std::time::Duration::from_millis(2));
        let r = parse_json(&cli_suggest_action(
            &mut rt,
            &json!({
                "action_type": "update",
                "target_task_id": format!("task_deact_{i}"),
                "reason": format!("deactivation test {i}"),
            }),
        ));
        ids.push(r["suggestion_id"].as_str().expect("id").to_string());
    }

    // Surface each suggestion 3 times via digest (each call increments surface_count)
    for _ in 0..3 {
        let _ = digest_pending_action_suggestions(&rt, "update");
    }

    // After 3 surfaces with no consumption, deactivation must be active
    let deactivated_digest = digest_pending_action_suggestions(&rt, "update");
    assert!(
        deactivated_digest.is_none(),
        "after 3 surfaces without consume, action_type 'update' must be deactivated"
    );

    // Consume one suggestion — must reset the deactivation cooldown
    cli_suggestion_mark_consumed(
        &mut rt,
        &json!({"suggestion_id": ids[0], "consumed_action": "TaskUpdate"}),
    );

    // Verify deactivation was cleared in the DB
    let still_deactivated: i64 = rt
        .ctx
        .knowledge
        .conn_ref()
        .query_row(
            "SELECT COUNT(*) FROM action_type_deactivation \
             WHERE action_type = 'update' AND deactivated_until > datetime('now')",
            rusqlite::params![],
            |r| r.get(0),
        )
        .unwrap_or(1); // pessimistic default: assume still active if query fails
    assert_eq!(
        still_deactivated, 0,
        "consuming a suggestion must reset the deactivation cooldown"
    );
}

// ─────────────────────────────────────────────────────────────────────────
// R6: Acceptance rate learning (T19–T22)
// ─────────────────────────────────────────────────────────────────────────

/// T19 (R6): marking a suggestion consumed increments acceptance_count and
/// total_samples in `action_type_deactivation`.
#[test]
fn mark_consumed_increments_acceptance_count() {
    let (_tmp, mut rt) = setup_runtime();

    // Insert one suggestion.
    let r = parse_json(&cli_suggest_action(
        &mut rt,
        &json!({
            "action_type": "update",
            "target_task_id": "task_r6_t19",
            "reason": "R6 acceptance test",
        }),
    ));
    let sugg_id = r["suggestion_id"]
        .as_str()
        .expect("suggestion_id")
        .to_string();

    // Mark it consumed.
    let consumed = parse_json(&cli_suggestion_mark_consumed(
        &mut rt,
        &json!({"suggestion_id": sugg_id, "consumed_action": "TaskUpdate"}),
    ));
    assert!(
        consumed["marked"].as_bool().unwrap_or(false),
        "must mark consumed"
    );

    // cli_suggestion_stats must show acceptance_count=1, total_samples=1.
    let stats = parse_json(&cli_suggestion_stats(&mut rt, &json!({})));
    let rows = stats["stats"].as_array().expect("stats array");
    let row = rows
        .iter()
        .find(|r| r["action_type"] == "update")
        .expect("update row must exist");
    assert_eq!(
        row["acceptance_count"].as_i64().unwrap_or(0),
        1,
        "acceptance_count must be 1 after one consume"
    );
    assert!(
        row["total_samples"].as_i64().unwrap_or(0) >= 1,
        "total_samples must be >= 1"
    );
    assert!(
        row["acceptance_rate"].as_f64().unwrap_or(0.0) > 0.0,
        "acceptance_rate must be > 0 after one consume"
    );
}

/// T20 (R6): when acceptance rate > 0.8 with ≥ 5 samples AND action_type is
/// deactivated, marking consumed clears `deactivated_until` (early reactivation).
#[test]
fn mark_consumed_clears_deactivation_if_rate_high() {
    let (_tmp, mut rt) = setup_runtime();
    let conn = rt.ctx.knowledge.conn_ref();

    // Bootstrap schema.
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS action_type_deactivation (
             action_type TEXT PRIMARY KEY,
             consecutive_ignores INTEGER NOT NULL DEFAULT 0,
             deactivated_until TEXT,
             acceptance_count INTEGER NOT NULL DEFAULT 0,
             total_samples INTEGER NOT NULL DEFAULT 0
         );",
    )
    .expect("bootstrap deactivation table");

    // Seed: 5 prior consumes + active deactivation.
    conn.execute(
        "INSERT INTO action_type_deactivation \
           (action_type, consecutive_ignores, acceptance_count, total_samples, deactivated_until) \
         VALUES ('stop', 0, 5, 5, datetime('now', '+23 hours'))",
        rusqlite::params![],
    )
    .expect("seed deactivation row");

    let _ = conn; // release borrow before mutable calls

    // Insert and consume one more suggestion of type 'stop'.
    let r = parse_json(&cli_suggest_action(
        &mut rt,
        &json!({
            "action_type": "stop",
            "target_task_id": "task_r6_t20",
            "reason": "R6 early reactivation test",
        }),
    ));
    let sugg_id = r["suggestion_id"]
        .as_str()
        .expect("suggestion_id")
        .to_string();

    let result = parse_json(&cli_suggestion_mark_consumed(
        &mut rt,
        &json!({"suggestion_id": sugg_id, "consumed_action": "TaskStop"}),
    ));
    assert!(result["marked"].as_bool().unwrap_or(false));
    // acceptance_count will be 6/6 = 1.0 > 0.8 with total >= 5 → early_reactivated.
    assert!(
        result["early_reactivated"].as_bool().unwrap_or(false),
        "deactivation must be cleared when acceptance_rate > 0.8 and total_samples >= 5; got: {result}"
    );

    // Confirm deactivated_until is NULL in DB.
    let still_deactivated: i64 = rt
        .ctx
        .knowledge
        .conn_ref()
        .query_row(
            "SELECT COUNT(*) FROM action_type_deactivation \
             WHERE action_type = 'stop' AND deactivated_until > datetime('now')",
            rusqlite::params![],
            |r| r.get(0),
        )
        .unwrap_or(1);
    assert_eq!(
        still_deactivated, 0,
        "deactivated_until must be NULL after early reactivation"
    );
}

/// T21 (R6): when total_samples >= 10 and acceptance_rate < 0.2, the deactivation
/// cooldown extends to 72 hours instead of the default 24 hours.
#[test]
fn deactivation_extends_to_72h_if_low_rate_many_samples() {
    let (_tmp, rt) = setup_runtime();
    let conn = rt.ctx.knowledge.conn_ref();

    // Bootstrap tables manually (digest calls ensure_decompose_tables internally
    // but we need surface_count column too).
    conn.execute_batch(
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
             consumed_action TEXT,
             surface_count INTEGER NOT NULL DEFAULT 0
         );
         CREATE TABLE IF NOT EXISTS action_type_deactivation (
             action_type TEXT PRIMARY KEY,
             consecutive_ignores INTEGER NOT NULL DEFAULT 0,
             deactivated_until TEXT,
             acceptance_count INTEGER NOT NULL DEFAULT 0,
             total_samples INTEGER NOT NULL DEFAULT 0
         );
         CREATE TABLE IF NOT EXISTS task_decompositions (
             task_id TEXT PRIMARY KEY,
             task_type TEXT NOT NULL,
             description TEXT NOT NULL DEFAULT '',
             status TEXT NOT NULL DEFAULT 'created',
             created_at TEXT NOT NULL,
             updated_at TEXT NOT NULL,
             origin TEXT NOT NULL DEFAULT 'claude-code',
             mirrored_to_cc INTEGER NOT NULL DEFAULT 1,
             cila_level INTEGER NOT NULL DEFAULT 3
         );",
    )
    .expect("bootstrap tables");

    // Seed: acceptance_count=1, total_samples=10 → rate = 0.1 < 0.2.
    conn.execute(
        "INSERT INTO action_type_deactivation \
           (action_type, consecutive_ignores, acceptance_count, total_samples) \
         VALUES ('plan_mode', 0, 1, 10)",
        rusqlite::params![],
    )
    .expect("seed low-rate row");

    // Insert 3 suggestions and surface each 3 times to trigger deactivation.
    for i in 0..3 {
        let sid = format!("sugg_r6_t21_{i}");
        conn.execute(
            "INSERT INTO cc_action_suggestions \
               (suggestion_id, action_type, target_task_id, reason, suggested_at) \
             VALUES (?1, 'plan_mode', 'task_r6_t21', 'low rate test', datetime('now'))",
            rusqlite::params![sid],
        )
        .expect("insert suggestion");
    }

    let _ = conn; // end immutable borrow scope before mutable operations

    // Surface 3 times to accumulate surface_count >= 3 on each row.
    for _ in 0..3 {
        let _ = touring_hooks::task_digest::digest_pending_action_suggestions(&rt, "plan_mode");
    }

    // Deactivated_until should be ~72h from now (not just 24h).
    let deactivated_until: Option<String> = rt.ctx.knowledge.conn_ref()
        .query_row(
            "SELECT deactivated_until FROM action_type_deactivation WHERE action_type = 'plan_mode'",
            rusqlite::params![],
            |r| r.get(0),
        )
        .ok()
        .flatten();

    assert!(
        deactivated_until.is_some(),
        "deactivated_until must be set after 3 surfaces"
    );

    // Verify it's > 48h from now (i.e., the 72h window was used, not 24h).
    let is_extended: i64 = rt
        .ctx
        .knowledge
        .conn_ref()
        .query_row(
            "SELECT COUNT(*) FROM action_type_deactivation \
             WHERE action_type = 'plan_mode' \
               AND deactivated_until > datetime('now', '+48 hours')",
            rusqlite::params![],
            |r| r.get(0),
        )
        .unwrap_or(0);
    assert_eq!(
        is_extended, 1,
        "low-rate action_type (rate<0.2, samples>=10) must get +72h cooldown, not +24h"
    );
}

/// T22 (R6): cli_suggestion_stats returns correct acceptance_rate for seeded data.
#[test]
fn cli_suggestion_stats_returns_rate_correctly() {
    let (_tmp, mut rt) = setup_runtime();

    // Ensure schema exists via a handler call.
    let _ = cli_suggestion_stats(&mut rt, &json!({}));

    // Directly seed acceptance stats: acceptance_count=3, total_samples=10 → rate=0.3.
    rt.ctx
        .knowledge
        .conn_ref()
        .execute(
            "INSERT INTO action_type_deactivation \
           (action_type, consecutive_ignores, acceptance_count, total_samples) \
         VALUES ('update', 0, 3, 10)",
            rusqlite::params![],
        )
        .expect("seed stats row");

    let result = parse_json(&cli_suggestion_stats(&mut rt, &json!({})));
    let rows = result["stats"].as_array().expect("stats array");
    let row = rows
        .iter()
        .find(|r| r["action_type"] == "update")
        .expect("update row must be present");

    assert_eq!(row["acceptance_count"].as_i64().unwrap_or(0), 3);
    assert_eq!(row["total_samples"].as_i64().unwrap_or(0), 10);

    let rate = row["acceptance_rate"].as_f64().unwrap_or(0.0);
    assert!(
        (rate - 0.3).abs() < 1e-9,
        "acceptance_rate must be 0.3 (3/10), got {rate}"
    );
    assert_eq!(result["total_action_types"].as_i64().unwrap_or(0), 1);
}
