//! E2E: a finalized task is archived, and every scaffold stage is closed.
//!
//! Both invariants were measured false on 19/09/2026 against the live DAG:
//!   * `archived_at` was NULL on all 381 tasks, including the 18 `finalized`
//!     ones — `finalize` stamps the status and never the archive, while the
//!     retention routine only matches `status = 'completed'`, which `finalize`
//!     never produces. The two ends never met.
//!   * 74 of the 78 open subtasks under mirrored CC tasks were the SAME slot,
//!     `::scout` — the closer names `::validate` and `::implement` literally
//!     and knows nothing of the list the scaffolder writes.
//!
//! Added: 2026-09-19.

#![allow(clippy::indexing_slicing)]

use serde_json::json;
use tempfile::TempDir;
use touring_hooks::cli_handlers::{cli_decompose_add, cli_decompose_create, cli_decompose_finalize};
use touring_hooks::cli_handlers_decompose::cli_decompose_archive;
use touring_hooks::hook_decompose_bridge::{bridge_task_created, close_scaffold_stages};
use touring_hooks::runtime::HookRuntime;

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

/// Read one task's `archived_at` straight from the store — the fact, not the report.
fn archived_at_of(rt: &HookRuntime, task_id: &str) -> Option<String> {
    rt.ctx
        .knowledge
        .conn_ref()
        .query_row(
            "SELECT archived_at FROM task_decompositions WHERE task_id = ?1",
            [task_id],
            |row| row.get::<_, Option<String>>(0),
        )
        .expect("task row must exist")
}

/// How many subtasks of this task are still open, asked of the STORE.
///
/// The assertion must not iterate `MIRROR_SCAFFOLD_STAGES`: a test that walks
/// the same list the code walks passes whatever the list says, including a list
/// with a stage removed. The store holds what the scaffolder actually wrote.
fn open_stage_count(rt: &HookRuntime, task_id: &str) -> i64 {
    rt.ctx
        .knowledge
        .conn_ref()
        .query_row(
            "SELECT COUNT(*) FROM decomposition_subtasks WHERE task_id = ?1 \
             AND status NOT IN ('completed', 'failed', 'done', 'skipped', 'cancelled')",
            [task_id],
            |row| row.get::<_, i64>(0),
        )
        .expect("count must run")
}

fn status_of(rt: &HookRuntime, subtask_id: &str) -> String {
    rt.ctx
        .knowledge
        .conn_ref()
        .query_row(
            "SELECT status FROM decomposition_subtasks WHERE subtask_id = ?1",
            [subtask_id],
            |row| row.get::<_, String>(0),
        )
        .unwrap_or_else(|e| panic!("subtask {subtask_id} must exist: {e}"))
}

// ───────────────────────────────────────────────────────────────────────
// P1 — finalize archives, and says it archived
// ───────────────────────────────────────────────────────────────────────

#[test]
fn finalize_stamps_archived_at_and_reports_it() {
    let (_tmp, mut rt) = setup_runtime();
    let created = parse_json(&cli_decompose_create(
        &mut rt,
        &json!({"task_type": "plan", "description": "work that ends", "origin": "touring-cli"}),
    ));
    let task_id = created["task_id"].as_str().expect("task_id").to_string();
    cli_decompose_add(
        &mut rt,
        &json!({
            "task_id": task_id,
            "subtask_id": format!("{task_id}::only"),
            "description": "the single step",
        }),
    );

    assert!(
        archived_at_of(&rt, &task_id).is_none(),
        "a fresh task is not archived"
    );

    let finalized = parse_json(&cli_decompose_finalize(&mut rt, &json!({"task_id": task_id})));

    assert_eq!(finalized["status"], "finalized");
    assert_eq!(
        finalized["archived"], true,
        "the caller in hook_registry gates on `archived:true`; a finalize that \
         archives without saying so leaves that branch dead forever"
    );
    assert!(
        archived_at_of(&rt, &task_id).is_some(),
        "finalize must stamp archived_at — otherwise every query that filters \
         on the archive sees the whole history as live"
    );
}

/// The gate must not archive what it refuses: a blocked finalize leaves no stamp.
#[test]
fn a_refused_finalize_archives_nothing() {
    let (_tmp, mut rt) = setup_runtime();
    let created = parse_json(&cli_decompose_create(
        &mut rt,
        &json!({"task_type": "plan", "description": "needs review", "origin": "touring-cli"}),
    ));
    let task_id = created["task_id"].as_str().expect("task_id").to_string();
    cli_decompose_add(
        &mut rt,
        &json!({
            "task_id": task_id,
            "subtask_id": format!("{task_id}::gated"),
            "description": "awaits review",
        }),
    );
    // `cli_decompose_add` writes `review_required` as a literal 0 — the flag has
    // no payload route today (only the tasksfile importer sets it), so the
    // condition is armed directly in the fixture.
    rt.ctx
        .knowledge
        .conn_ref()
        .execute(
            "UPDATE decomposition_subtasks SET review_required = 1 WHERE task_id = ?1",
            [&task_id],
        )
        .expect("arm the review gate");

    let refused = parse_json(&cli_decompose_finalize(&mut rt, &json!({"task_id": task_id})));

    assert!(
        refused.get("error").is_some(),
        "review_required without a score must block: {refused}"
    );
    assert!(
        archived_at_of(&rt, &task_id).is_none(),
        "a refused finalize must leave the task open"
    );
}

/// The retention pass, which had no caller at all until 19/09/2026.
#[test]
fn archive_stamps_terminal_tasks_that_finalize_never_touched() {
    let (_tmp, mut rt) = setup_runtime();
    let created = parse_json(&cli_decompose_create(
        &mut rt,
        &json!({"task_type": "plan", "description": "legacy", "origin": "touring-cli"}),
    ));
    let task_id = created["task_id"].as_str().expect("task_id").to_string();
    // A task that reached a terminal status WITHOUT going through finalize —
    // the shape of the 110 rows measured in the live DAG.
    rt.ctx
        .knowledge
        .conn_ref()
        .execute(
            "UPDATE task_decompositions SET status = 'done' WHERE task_id = ?1",
            [&task_id],
        )
        .expect("mark done");
    assert!(archived_at_of(&rt, &task_id).is_none());

    let row: (String, Option<String>) = rt
        .ctx
        .knowledge
        .conn_ref()
        .query_row(
            "SELECT status, updated_at FROM task_decompositions WHERE task_id = ?1",
            [&task_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .expect("row");

    let preview = parse_json(&cli_decompose_archive(&mut rt, &json!({"dry_run": true})));
    assert_eq!(
        preview["candidates"], 1,
        "the dry run must SEE the row (status={:?} updated_at={:?})",
        row.0, row.1
    );
    assert_eq!(preview["archived"], 0, "a dry run writes nothing");
    assert!(
        archived_at_of(&rt, &task_id).is_none(),
        "dry run must leave the row untouched"
    );

    let done = parse_json(&cli_decompose_archive(&mut rt, &json!({})));
    assert_eq!(done["archived"], 1);
    assert!(archived_at_of(&rt, &task_id).is_some());
}

/// Work still in flight is never archived by the retention pass.
#[test]
fn archive_leaves_unfinished_work_alone() {
    let (_tmp, mut rt) = setup_runtime();
    let created = parse_json(&cli_decompose_create(
        &mut rt,
        &json!({"task_type": "plan", "description": "still running", "origin": "touring-cli"}),
    ));
    let task_id = created["task_id"].as_str().expect("task_id").to_string();

    let done = parse_json(&cli_decompose_archive(&mut rt, &json!({})));

    assert_eq!(done["archived"], 0, "a `created` task is not terminal");
    assert!(archived_at_of(&rt, &task_id).is_none());
}

// ───────────────────────────────────────────────────────────────────────
// P2 — the closer knows every stage the scaffolder writes
// ───────────────────────────────────────────────────────────────────────

#[test]
fn completing_a_mirrored_task_closes_every_scaffold_stage() {
    let (_tmp, mut rt) = setup_runtime();
    bridge_task_created(&mut rt, "42", "a CC task", "sess", None, None)
        .expect("mirror must be created");
    let mirror = "cc_task_42";

    let scaffolded = open_stage_count(&rt, mirror);
    assert!(
        scaffolded > 0,
        "the fixture must actually scaffold stages, or this test proves nothing"
    );

    let closed = close_scaffold_stages(&mut rt, mirror, true);

    // The load-bearing assertion, asked of the STORE: not one row the
    // scaffolder wrote may stay open. Counting against
    // `MIRROR_SCAFFOLD_STAGES` would pass with a stage deleted from it —
    // proven by mutation, which is why this asks the database instead.
    assert_eq!(
        open_stage_count(&rt, mirror),
        0,
        "{scaffolded} stages were scaffolded and {closed} closed, yet rows stayed \
         open — this is the 74-row `::scout` leak measured on 19/09/2026"
    );
    assert_eq!(
        status_of(&rt, &format!("{mirror}::scout")),
        "completed",
        "`scout` is the slot the old closer never named"
    );
}

#[test]
fn a_failed_task_marks_every_stage_failed() {
    let (_tmp, mut rt) = setup_runtime();
    bridge_task_created(&mut rt, "43", "a CC task that failed", "sess", None, None)
        .expect("mirror must be created");
    let mirror = "cc_task_43";

    close_scaffold_stages(&mut rt, mirror, false);

    for stage in touring_foundation::task_lifecycle::MIRROR_SCAFFOLD_STAGES {
        assert_eq!(
            status_of(&rt, &format!("{mirror}::{}", stage.name)),
            "failed",
            "a failed task mirrors its outcome onto every stage"
        );
    }
}
