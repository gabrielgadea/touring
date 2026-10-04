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
use touring_hooks::cli_handlers::{
    cli_decompose_add, cli_decompose_create, cli_decompose_finalize, cli_decompose_update,
};
use touring_foundation::task_lifecycle::TERMINAL_TASK_STATUSES;
use touring_hooks::cli_handlers_decompose::{
    auto_archive_pass, auto_archive_pass_all_stores, cli_decompose_archive,
    cli_decompose_reconcile_stages, cli_workflow_stats,
};
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

    let finalized = parse_json(&cli_decompose_finalize(
        &mut rt,
        &json!({"task_id": task_id}),
    ));

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
            "review_required": true,
        }),
    );

    let refused = parse_json(&cli_decompose_finalize(
        &mut rt,
        &json!({"task_id": task_id}),
    ));

    assert!(
        refused.get("error").is_some(),
        "review_required without a score must block: {refused}"
    );
    assert!(
        archived_at_of(&rt, &task_id).is_none(),
        "a refused finalize must leave the task open"
    );
}

/// `review_required` is the gate `finalize` enforces — and the only way to set
/// it was the tasksfile importer. A flag no payload can reach is a gate nobody
/// can arm: `finalize` refuses what nothing could mark.
#[test]
fn add_accepts_the_review_flag_the_finalize_gate_reads() {
    let (_tmp, mut rt) = setup_runtime();
    let created = parse_json(&cli_decompose_create(
        &mut rt,
        &json!({"task_type": "plan", "description": "needs a reviewer", "origin": "touring-cli"}),
    ));
    let task_id = created["task_id"].as_str().expect("task_id").to_string();

    cli_decompose_add(
        &mut rt,
        &json!({
            "task_id": task_id,
            "subtask_id": format!("{task_id}::gated"),
            "description": "awaits review",
            "review_required": true,
        }),
    );

    let flag: i64 = rt
        .ctx
        .knowledge
        .conn_ref()
        .query_row(
            "SELECT review_required FROM decomposition_subtasks WHERE task_id = ?1",
            [&task_id],
            |row| row.get(0),
        )
        .expect("subtask must exist");
    assert_eq!(flag, 1, "the payload's review_required must reach the row");

    // And the gate it feeds must now actually fire.
    let refused = parse_json(&cli_decompose_finalize(
        &mut rt,
        &json!({"task_id": task_id}),
    ));
    assert!(
        refused.get("error").is_some(),
        "a subtask marked for review must block finalize: {refused}"
    );
}

/// Omitting the flag keeps the historical default — no surprise gates.
#[test]
fn add_defaults_review_required_to_off() {
    let (_tmp, mut rt) = setup_runtime();
    let created = parse_json(&cli_decompose_create(
        &mut rt,
        &json!({"task_type": "plan", "description": "ordinary", "origin": "touring-cli"}),
    ));
    let task_id = created["task_id"].as_str().expect("task_id").to_string();

    cli_decompose_add(
        &mut rt,
        &json!({
            "task_id": task_id,
            "subtask_id": format!("{task_id}::plain"),
            "description": "no review",
        }),
    );

    let flag: i64 = rt
        .ctx
        .knowledge
        .conn_ref()
        .query_row(
            "SELECT review_required FROM decomposition_subtasks WHERE task_id = ?1",
            [&task_id],
            |row| row.get(0),
        )
        .expect("subtask must exist");
    assert_eq!(flag, 0);
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
// P3 — `add` re-derives the parent (cross-audit 03/10/2026, finding H)
// ───────────────────────────────────────────────────────────────────────
//
// The parent's status is DERIVED from its children (`refresh_parent_status`) and is
// recomputed only when a caller addresses a SUBTASK. `add` is the other verb that
// can invalidate it, and it never recomputed: closing 4 subtasks and then adding 7
// left the plan `done` with 7 pending children (observed 03/10/2026). Only a CLOSED
// parent can go stale that way — a pending child changes nothing for a `pending` or
// `in_progress` plan — so the fix is deliberately narrow.

/// The PARENT row's status, straight from the store — the fact, not the report.
fn parent_status_of(rt: &HookRuntime, task_id: &str) -> String {
    rt.ctx
        .knowledge
        .conn_ref()
        .query_row(
            "SELECT status FROM task_decompositions WHERE task_id = ?1",
            [task_id],
            |row| row.get::<_, String>(0),
        )
        .expect("task row must exist")
}

fn new_plan(rt: &mut HookRuntime, description: &str) -> String {
    let created = parse_json(&cli_decompose_create(
        rt,
        &json!({"task_type": "plan", "description": description, "origin": "touring-cli"}),
    ));
    created["task_id"].as_str().expect("task_id").to_string()
}

/// Add one subtask and return its scoped id.
fn add_step(rt: &mut HookRuntime, task_id: &str, name: &str) -> String {
    let scoped = format!("{task_id}::{name}");
    let reply = parse_json(&cli_decompose_add(
        rt,
        &json!({"task_id": task_id, "subtask_id": scoped, "description": name}),
    ));
    assert_eq!(reply["persisted"], true, "the add must land: {reply}");
    scoped
}

fn close_step(rt: &mut HookRuntime, task_id: &str, scoped: &str) {
    let reply = parse_json(&cli_decompose_update(
        rt,
        &json!({"task_id": task_id, "subtask_id": scoped, "status": "completed"}),
    ));
    assert_eq!(reply["subtask_updated"], true, "the close must land: {reply}");
}

/// The observed defect: close every subtask (the plan derives `done`), then add one.
#[test]
fn add_to_a_done_plan_reopens_it() {
    let (_tmp, mut rt) = setup_runtime();
    let task_id = new_plan(&mut rt, "closed, then it grows");
    let first = add_step(&mut rt, &task_id, "first");
    close_step(&mut rt, &task_id, &first);
    assert_eq!(
        parent_status_of(&rt, &task_id),
        "done",
        "premise: the last open subtask closed, so the plan derives done"
    );

    add_step(&mut rt, &task_id, "late");

    assert_eq!(
        open_stage_count(&rt, &task_id),
        1,
        "premise: the new subtask is open"
    );
    assert_eq!(
        parent_status_of(&rt, &task_id),
        "in_progress",
        "a plan with an open subtask is not done — `done` over pending work is the defect"
    );
}

/// One value out of the store — the fact, not the report.
fn scalar<T: rusqlite::types::FromSql>(rt: &HookRuntime, sql: &str, id: &str) -> T {
    rt.ctx
        .knowledge
        .conn_ref()
        .query_row(sql, [id], |row| row.get::<_, T>(0))
        .unwrap_or_else(|e| panic!("{sql} for {id}: {e}"))
}

/// `INSERT OR REPLACE` used to put a re-added subtask back to `pending` — over a CLOSED one too —
/// and to wipe every column the INSERT did not name. Re-adding is a way to restate what a step IS
/// (its description, its edges); it is not a way to un-do the work the row already carries.
#[test]
fn re_adding_a_closed_subtask_keeps_its_work() {
    let (_tmp, mut rt) = setup_runtime();
    let task_id = new_plan(&mut rt, "same step again");
    let first = add_step(&mut rt, &task_id, "first");
    close_step(&mut rt, &task_id, &first);
    assert_eq!(parent_status_of(&rt, &task_id), "done", "premise");

    let reply = parse_json(&cli_decompose_add(
        &mut rt,
        &json!({"task_id": task_id, "subtask_id": first, "description": "first, reworded"}),
    ));

    assert_eq!(reply["persisted"], true, "{reply}");
    assert_eq!(reply["existed"], true, "the reply must say it met a row: {reply}");
    assert_eq!(
        reply["status"], "completed",
        "the reply reports the REAL status, not a hard-coded `pending`: {reply}"
    );
    assert_eq!(status_of(&rt, &first), "completed", "closed work is not re-opened by a restatement");
    assert_eq!(
        parent_status_of(&rt, &task_id),
        "done",
        "every subtask is still closed, so the plan is still done"
    );
    let described: String = scalar(&rt, "SELECT description FROM decomposition_subtasks WHERE subtask_id = ?1", &first);
    assert_eq!(described, "first, reworded", "the restatement itself must land");
}

/// The columns a REPLACE would have reset, each asserted: a written column list is exactly where
/// one gets forgotten (touring-a4, 03/10/2026 — attempts, quality_score, retry_policy, the claim).
#[test]
fn re_adding_keeps_what_the_row_accumulated() {
    let (_tmp, mut rt) = setup_runtime();
    let task_id = new_plan(&mut rt, "a step with a history");
    let first = add_step(&mut rt, &task_id, "first");
    close_step(&mut rt, &task_id, &first);
    rt.ctx
        .knowledge
        .conn_ref()
        .execute(
            "UPDATE decomposition_subtasks SET attempts = 3, quality_score = 0.9, \
             retry_policy = '{\"max_attempts\":5}', claimed_by = 'sess-a', \
             claim_expires_at = 4102444800, created_at = '2026-01-01T00:00:00+00:00' \
             WHERE subtask_id = ?1",
            [&first],
        )
        .expect("give the row a history");

    add_step(&mut rt, &task_id, "first");

    let attempts: i64 = scalar(&rt, "SELECT attempts FROM decomposition_subtasks WHERE subtask_id = ?1", &first);
    let quality: f64 = scalar(&rt, "SELECT quality_score FROM decomposition_subtasks WHERE subtask_id = ?1", &first);
    let policy: String = scalar(&rt, "SELECT retry_policy FROM decomposition_subtasks WHERE subtask_id = ?1", &first);
    let claimer: String = scalar(&rt, "SELECT claimed_by FROM decomposition_subtasks WHERE subtask_id = ?1", &first);
    let lease: i64 = scalar(&rt, "SELECT claim_expires_at FROM decomposition_subtasks WHERE subtask_id = ?1", &first);
    let born: String = scalar(&rt, "SELECT created_at FROM decomposition_subtasks WHERE subtask_id = ?1", &first);
    assert_eq!(attempts, 3, "attempts");
    assert!((quality - 0.9).abs() < 1e-9, "quality_score: {quality}");
    assert_eq!(policy, "{\"max_attempts\":5}", "retry_policy");
    assert_eq!(claimer, "sess-a", "a live claim belongs to its session, not to the restatement");
    assert_eq!(lease, 4102444800, "claim_expires_at");
    assert_eq!(born, "2026-01-01T00:00:00+00:00", "created_at: the row was not born again");
}

/// What the old reopen path covered stays covered: a plan marked closed while a child is still
/// open (the state the pre-fix `add` left behind) is re-derived when that child is re-added.
#[test]
fn re_adding_an_open_subtask_still_reopens_a_plan_wrongly_marked_closed() {
    let (_tmp, mut rt) = setup_runtime();
    let task_id = new_plan(&mut rt, "marked closed too early");
    add_step(&mut rt, &task_id, "first");
    rt.ctx
        .knowledge
        .conn_ref()
        .execute("UPDATE task_decompositions SET status = 'done' WHERE task_id = ?1", [&task_id])
        .expect("the stale state");

    add_step(&mut rt, &task_id, "first");

    assert_eq!(
        parent_status_of(&rt, &task_id),
        "pending",
        "a `done` plan whose only step is pending is stale"
    );
}

/// An id that already belongs to ANOTHER plan is refused. The conflict arm updates the existing row
/// in place, so without the refusal `add` would report `persisted` for a step that never reached the
/// plan the caller named (`INSERT OR REPLACE` used to MOVE the row, resetting it on the way).
#[test]
fn re_adding_an_id_that_belongs_to_another_plan_is_refused() {
    let (_tmp, mut rt) = setup_runtime();
    let owner = new_plan(&mut rt, "owns the step");
    let other = new_plan(&mut rt, "wants the same id");
    let step = add_step(&mut rt, &owner, "shared");

    let reply = parse_json(&cli_decompose_add(
        &mut rt,
        &json!({"task_id": other, "subtask_id": step, "description": "stolen"}),
    ));

    assert_eq!(reply["persisted"], false, "{reply}");
    let reason = reply["error"].as_str().unwrap_or("");
    assert!(reason.contains(&owner), "the refusal must name the owner: {reply}");
    let kept: String = scalar(&rt, "SELECT task_id FROM decomposition_subtasks WHERE subtask_id = ?1", &step);
    assert_eq!(kept, owner, "the step stays where it was");
}

/// Narrow on purpose: a status the caller set on an OPEN plan is theirs. Deriving over it
/// would read `pending` from all-pending children and rewrite what the caller chose.
#[test]
fn add_leaves_the_status_a_caller_set_on_an_open_plan_alone() {
    let (_tmp, mut rt) = setup_runtime();
    let task_id = new_plan(&mut rt, "caller-owned status");
    cli_decompose_update(&mut rt, &json!({"task_id": task_id, "status": "in_progress"}));
    assert_eq!(
        parent_status_of(&rt, &task_id),
        "in_progress",
        "premise: a parent-level update sets the status directly"
    );

    add_step(&mut rt, &task_id, "only");

    assert_eq!(
        parent_status_of(&rt, &task_id),
        "in_progress",
        "add must not derive over an open plan"
    );
}

/// `finalized` is a lifecycle step (it stamps the archive), not a derivation: add does not undo it.
#[test]
fn add_does_not_reopen_a_finalized_plan() {
    let (_tmp, mut rt) = setup_runtime();
    let task_id = new_plan(&mut rt, "finalized, then touched");
    add_step(&mut rt, &task_id, "only");
    let finalized = parse_json(&cli_decompose_finalize(&mut rt, &json!({"task_id": task_id})));
    assert_eq!(finalized["status"], "finalized", "premise: {finalized}");

    add_step(&mut rt, &task_id, "after");

    assert_eq!(parent_status_of(&rt, &task_id), "finalized");
}

/// The parent can also be CLOSED with the other spelling: the RPC door takes `{"status":
/// "completed"}` without a `subtask_id` and writes it straight onto the parent row. A guard that
/// listed only `done` would leave this spelling of the same defect alive (touring-a4, 03/10/2026).
#[test]
fn add_to_a_plan_closed_as_completed_reopens_it_too() {
    let (_tmp, mut rt) = setup_runtime();
    let task_id = new_plan(&mut rt, "closed with the other spelling");
    let first = add_step(&mut rt, &task_id, "first");
    close_step(&mut rt, &task_id, &first);
    cli_decompose_update(&mut rt, &json!({"task_id": task_id, "status": "completed"}));
    assert_eq!(
        parent_status_of(&rt, &task_id),
        "completed",
        "premise: a parent-level update writes the spelling it was given"
    );

    add_step(&mut rt, &task_id, "late");

    assert_eq!(
        parent_status_of(&rt, &task_id),
        "in_progress",
        "`completed` over a pending subtask is the same defect as `done` over one"
    );
}

/// The retention pass stamps `archived_at` on a closed plan. Reopening it by `add` must lift the
/// stamp: a plan that is `in_progress` and archived at once is live work hidden from every query
/// that filters on the archive, and the pass never un-stamps (it requires `archived_at IS NULL`).
/// Without this, the fix would CREATE that state (touring-a4, 03/10/2026; same shape as 30.4.63).
#[test]
fn reopening_a_plan_the_retention_pass_archived_lifts_the_stamp() {
    let (_tmp, mut rt) = setup_runtime();
    let task_id = new_plan(&mut rt, "archived, then it grows");
    let first = add_step(&mut rt, &task_id, "first");
    close_step(&mut rt, &task_id, &first);
    let swept = parse_json(&cli_decompose_archive(&mut rt, &json!({})));
    assert_eq!(swept["archived"], 1, "premise: the retention pass archives it: {swept}");
    assert!(archived_at_of(&rt, &task_id).is_some(), "premise: stamped");

    add_step(&mut rt, &task_id, "late");

    assert_eq!(parent_status_of(&rt, &task_id), "in_progress");
    assert!(
        archived_at_of(&rt, &task_id).is_none(),
        "a reopened plan is live work: it cannot stay archived"
    );
}

/// Re-run the CALLING test in a child process whose `HOME` is private. Returns `true` in the parent
/// (the child ran the case and passed) and `false` in the child, which must then run the case.
///
/// `HOME` is read at call time by `locate_task_store`/`open_global_store`, so the case re-runs
/// itself rather than calling `set_var` in a binary whose tests run in parallel. `--exact` with a
/// name that matches nothing runs ZERO tests and libtest still exits 0: a rename of the calling fn
/// would leave the whole case green and unproven, so the child's log must say `1 passed`
/// (touring-a4, 03/10/2026).
fn rerun_with_private_home(self_name: &str) -> bool {
    const CHILD: &str = "DAG_E2E_PRIVATE_HOME_CHILD";
    if std::env::var_os(CHILD).is_some() {
        return false;
    }
    let home = TempDir::new().expect("private HOME");
    let out = std::process::Command::new(std::env::current_exe().expect("test binary"))
        .args(["--exact", self_name, "--nocapture", "--test-threads=1"])
        .env("HOME", home.path())
        .env(CHILD, "1")
        .output()
        .expect("child test process");
    assert!(
        out.status.success(),
        "child failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let log = String::from_utf8_lossy(&out.stdout);
    assert!(
        log.contains("1 passed"),
        "the child must have RUN the case, not zero tests:\n{log}"
    );
    true
}

/// `add` finds the task by `locate_task_store` (the project store first, then the GLOBAL
/// `$HOME/.claude/touring/knowledge.db`) and the INSERT goes to THAT store, so the parent must be
/// re-derived in the same one. A fix that read `rt.ctx.knowledge` instead would pass every test
/// above (their tasks are born in the project store) and break here (touring-a4, 03/10/2026).
/// `HOME` is read at call time, so the case re-runs itself in a child process with a private HOME
/// rather than calling `set_var` in a binary whose tests run in parallel.
#[test]
fn add_reopens_a_done_plan_that_lives_in_the_global_store() {
    if rerun_with_private_home("add_reopens_a_done_plan_that_lives_in_the_global_store") {
        return;
    }

    // Child: HOME is private. A runtime rooted AT home keeps its store at the global path.
    let home = std::path::PathBuf::from(std::env::var("HOME").expect("HOME"));
    std::fs::create_dir_all(home.join(".claude/data")).expect("data dir");
    let mut global_rt = HookRuntime::new(&home).expect("global-side runtime");
    let task_id = new_plan(&mut global_rt, "lives in the global store");
    let first = add_step(&mut global_rt, &task_id, "first");
    close_step(&mut global_rt, &task_id, &first);
    assert_eq!(parent_status_of(&global_rt, &task_id), "done", "premise");

    // Another project's runtime: the task is NOT in its store, so add must route to the global one.
    let (_tmp, mut project_rt) = setup_runtime();
    let reply = parse_json(&cli_decompose_add(
        &mut project_rt,
        &json!({"task_id": task_id, "subtask_id": format!("{task_id}::late"), "description": "late"}),
    ));
    assert_eq!(
        reply["persisted"], true,
        "premise: add must reach the global store: {reply}"
    );

    assert_eq!(
        parent_status_of(&global_rt, &task_id),
        "in_progress",
        "the parent must be re-derived in the store the INSERT went to"
    );
}

/// The parent is DERIVED from its children by the one `update` production runs. A second copy of
/// this verb used to sit in `handlers/decompose.rs`, reached only from a test (touring-a4,
/// 03/10/2026), and it carried two features the live copy had lost — the next tests.
#[test]
fn update_derives_the_parent_from_its_children() {
    let (_tmp, mut rt) = setup_runtime();
    let task_id = new_plan(&mut rt, "two steps");
    let a = add_step(&mut rt, &task_id, "a");
    let b = add_step(&mut rt, &task_id, "b");

    close_step(&mut rt, &task_id, &a);
    assert_eq!(parent_status_of(&rt, &task_id), "in_progress", "one of two closed");

    close_step(&mut rt, &task_id, &b);
    assert_eq!(parent_status_of(&rt, &task_id), "done", "every subtask closed");
}

/// `refresh_parent_status` kept a private copy of the terminal vocabulary that the foundation also
/// owns (`TERMINAL_TASK_STATUSES`): a status added there left a plan whose children all sat in it
/// deriving `in_progress`, not `done`. Walking the FOUNDATION'S list keeps the test honest about
/// which list the code must follow.
#[test]
fn the_parent_derivation_closes_on_every_status_the_foundation_calls_terminal() {
    for status in TERMINAL_TASK_STATUSES {
        let (_tmp, mut rt) = setup_runtime();
        let task_id = new_plan(&mut rt, status);
        let step = add_step(&mut rt, &task_id, "only");
        let reply = parse_json(&cli_decompose_update(
            &mut rt,
            &json!({"task_id": task_id, "subtask_id": step, "status": status}),
        ));
        assert_eq!(reply["subtask_updated"], true, "{reply}");
        assert_eq!(
            parent_status_of(&rt, &task_id),
            "done",
            "a plan whose only step is `{status}` is closed"
        );
    }
}

// ───────────────────────────────────────────────────────────────────────
// P4 — what the live `update` writes is what the readers read
// ───────────────────────────────────────────────────────────────────────
//
// `cli-workflow-stats` / `-status` average `subtask_results.duration_ms`, and nothing in production
// wrote that table: the only writer was the dead copy of `update`. A reader whose producer is
// missing answers `null` forever and looks healthy — the same shape as a consumer with no producer.

#[test]
fn a_subtask_that_runs_leaves_the_timing_the_workflow_reader_averages() {
    let (_tmp, mut rt) = setup_runtime();
    let task_id = new_plan(&mut rt, "timed");
    let step = add_step(&mut rt, &task_id, "work");
    let started = parse_json(&cli_decompose_update(
        &mut rt,
        &json!({"task_id": task_id, "subtask_id": step, "status": "in_progress"}),
    ));
    assert_eq!(started["subtask_updated"], true, "{started}");
    // duration_ms is the difference of two RFC 3339 stamps: let the clock move.
    std::thread::sleep(std::time::Duration::from_millis(20));
    close_step(&mut rt, &task_id, &step);

    let rows: i64 = scalar(&rt, "SELECT COUNT(*) FROM subtask_results WHERE subtask_id = ?1", &step);
    assert_eq!(rows, 1, "one start, one row — the update must write the execution record");
    let finished: Option<String> =
        scalar(&rt, "SELECT completed_at FROM subtask_results WHERE subtask_id = ?1", &step);
    assert!(finished.is_some(), "the close must stamp the row it opened");

    let stats = parse_json(&cli_workflow_stats(&mut rt, &json!({"task_id": task_id})));
    let average = stats["avg_duration_ms"]
        .as_f64()
        .unwrap_or_else(|| panic!("the reader must see a duration, not null: {stats}"));
    assert!(average >= 15.0, "the measured run lasted at least the sleep: {average}");
}

fn fail_step(rt: &mut HookRuntime, task_id: &str, step: &str, policy: Option<serde_json::Value>) -> serde_json::Value {
    let mut payload = json!({"task_id": task_id, "subtask_id": step, "status": "failed"});
    if let Some(policy) = policy {
        payload["retry_policy"] = policy;
    }
    parse_json(&cli_decompose_update(rt, &payload))
}

/// The retry loop (S1.3) lived only in the dead copy. A failure that arrives WITH a policy puts the
/// step back to `pending` and counts the attempt, until the attempts run out.
#[test]
fn a_failed_subtask_with_a_retry_policy_goes_back_to_pending_until_the_attempts_run_out() {
    let (_tmp, mut rt) = setup_runtime();
    let task_id = new_plan(&mut rt, "flaky");
    let step = add_step(&mut rt, &task_id, "flaky");
    let policy = || Some(json!({"max_attempts": 2}));

    let first = fail_step(&mut rt, &task_id, &step, policy());
    assert_eq!(first["retry_scheduled"], true, "{first}");
    assert_eq!(status_of(&rt, &step), "pending");
    let second = fail_step(&mut rt, &task_id, &step, policy());
    assert_eq!(second["retry_scheduled"], true, "{second}");
    assert_eq!(status_of(&rt, &step), "pending");
    let third = fail_step(&mut rt, &task_id, &step, policy());
    assert_eq!(third["retry_scheduled"], false, "the attempts ran out: {third}");
    assert_eq!(status_of(&rt, &step), "failed", "no attempts left, so the failure stands");
    let attempts: i64 = scalar(&rt, "SELECT attempts FROM decomposition_subtasks WHERE subtask_id = ?1", &step);
    assert_eq!(attempts, 2);
}

/// The default is unchanged: no policy anywhere, no retry. Every subtask `add` creates is in this case.
#[test]
fn a_failed_subtask_with_no_policy_stays_failed() {
    let (_tmp, mut rt) = setup_runtime();
    let task_id = new_plan(&mut rt, "no policy");
    let step = add_step(&mut rt, &task_id, "once");

    let reply = fail_step(&mut rt, &task_id, &step, None);

    assert_eq!(reply["retry_scheduled"], false, "{reply}");
    assert_eq!(status_of(&rt, &step), "failed");
}

/// `{"subtask_updated":false,"updated":true}` is the payload that made a phase close declare success
/// over a subtask that did not exist (25/08/2026). `subtask_missing` answers the question the
/// caller actually has — "did the id I passed match anything?" — and only the dead copy of `update`
/// said it.
#[test]
fn updating_a_subtask_that_does_not_exist_says_so() {
    let (_tmp, mut rt) = setup_runtime();
    let task_id = new_plan(&mut rt, "a ghost step");
    let step = add_step(&mut rt, &task_id, "real");

    let ghost = parse_json(&cli_decompose_update(
        &mut rt,
        &json!({"task_id": task_id, "subtask_id": "no-such-step", "status": "completed"}),
    ));
    assert_eq!(ghost["subtask_missing"], true, "{ghost}");
    assert_eq!(ghost["subtask_updated"], false, "{ghost}");

    let real = parse_json(&cli_decompose_update(
        &mut rt,
        &json!({"task_id": task_id, "subtask_id": step, "status": "completed"}),
    ));
    assert_eq!(real["subtask_missing"], false, "{real}");
}

// ───────────────────────────────────────────────────────────────────────
// P5 — the retention pass runs by itself, and it derives instead of trusting
// ───────────────────────────────────────────────────────────────────────

/// Push a plan's `updated_at` into the past; the SQLite format sorts below an RFC 3339 cutoff.
fn age_plan(rt: &HookRuntime, task_id: &str, days: i64) {
    rt.ctx
        .knowledge
        .conn_ref()
        .execute(
            "UPDATE task_decompositions SET updated_at = datetime('now', ?2) WHERE task_id = ?1",
            rusqlite::params![task_id, format!("-{days} days")],
        )
        .expect("age the plan");
}

/// The parent row alone is not evidence of a finished plan: before the `add` fix, 381 plans were
/// left `done` over pending children, and a stamp on them is permanent (nothing un-stamps a plan
/// nobody touches again). The pass must DERIVE (touring-a4, 03/10/2026).
#[test]
fn archive_leaves_a_closed_plan_with_open_children_alone() {
    let (_tmp, mut rt) = setup_runtime();
    let task_id = new_plan(&mut rt, "marked done over pending work");
    add_step(&mut rt, &task_id, "still open");
    rt.ctx
        .knowledge
        .conn_ref()
        .execute("UPDATE task_decompositions SET status = 'done' WHERE task_id = ?1", [&task_id])
        .expect("the stale state");

    let preview = parse_json(&cli_decompose_archive(&mut rt, &json!({"dry_run": true})));
    assert_eq!(preview["candidates"], 0, "an open child is not a candidate: {preview}");
    let done = parse_json(&cli_decompose_archive(&mut rt, &json!({})));

    assert_eq!(done["archived"], 0, "{done}");
    assert!(archived_at_of(&rt, &task_id).is_none());
}

/// The automatic pass: only a plan that has been closed for a week AND has no open child.
#[test]
fn the_automatic_pass_archives_only_plans_closed_for_a_week() {
    let (_tmp, mut rt) = setup_runtime();
    let old_closed = new_plan(&mut rt, "closed long ago");
    let step = add_step(&mut rt, &old_closed, "only");
    close_step(&mut rt, &old_closed, &step);

    let fresh_closed = new_plan(&mut rt, "closed just now");
    let step = add_step(&mut rt, &fresh_closed, "only");
    close_step(&mut rt, &fresh_closed, &step);

    let stale_done = new_plan(&mut rt, "marked done over pending work, long ago");
    add_step(&mut rt, &stale_done, "still open");
    rt.ctx
        .knowledge
        .conn_ref()
        .execute("UPDATE task_decompositions SET status = 'done' WHERE task_id = ?1", [&stale_done])
        .expect("the stale state");

    let still_open = new_plan(&mut rt, "open, long ago");
    add_step(&mut rt, &still_open, "open");

    // Age them only after every plan exists: creating a plan also runs the retention pass
    // (see `creating_a_plan_retires_the_closed_ones_nobody_finalized`), and a plan aged early
    // would be stamped by a later `new_plan` instead of by the pass this test calls.
    for aged in [&old_closed, &stale_done, &still_open] {
        age_plan(&rt, aged, 8);
    }
    let archived = auto_archive_pass(&rt.ctx.knowledge);

    assert_eq!(archived, 1, "exactly the plan that is closed AND old AND fully terminal");
    assert!(archived_at_of(&rt, &old_closed).is_some(), "closed for 8 days, children done");
    assert!(archived_at_of(&rt, &fresh_closed).is_none(), "closed today: inside the retention window");
    assert!(archived_at_of(&rt, &stale_done).is_none(), "a `done` row over a pending child is not closed");
    assert!(archived_at_of(&rt, &still_open).is_none(), "open work is never archived");
}

/// `task-completed` fires a few times a day (hook-trace census, 04/10/2026), so a retention pass that
/// ran only there would run too seldom to count as automatic. Starting a plan — the beginning of work —
/// retires the closed ones nobody finalized, in the project store only: a create must not reach into
/// the user's global store from every hook and test.
#[test]
fn creating_a_plan_retires_the_closed_ones_nobody_finalized() {
    let (_tmp, mut rt) = setup_runtime();
    let old = new_plan(&mut rt, "closed long ago");
    let step = add_step(&mut rt, &old, "only");
    close_step(&mut rt, &old, &step);
    let fresh = new_plan(&mut rt, "closed just now");
    let step = add_step(&mut rt, &fresh, "only");
    close_step(&mut rt, &fresh, &step);
    age_plan(&rt, &old, 8);
    assert!(archived_at_of(&rt, &old).is_none(), "premise: nothing swept it yet");

    let _next = new_plan(&mut rt, "the next piece of work");

    assert!(archived_at_of(&rt, &old).is_some(), "starting a plan sweeps the old closed one");
    assert!(archived_at_of(&rt, &fresh).is_none(), "closed today: inside the retention window");
}

/// A pass over the project store alone leaves a plan that lives in the GLOBAL store un-stamped for
/// good (`locate_task_store` routes a task to either — touring-a4, 03/10/2026). Private HOME, so the
/// "global" store is the test's own.
#[test]
fn the_automatic_pass_also_reaches_the_global_store() {
    if rerun_with_private_home("the_automatic_pass_also_reaches_the_global_store") {
        return;
    }

    // Child: HOME is private. A runtime rooted AT home keeps its store at the global path.
    let home = std::path::PathBuf::from(std::env::var("HOME").expect("HOME"));
    std::fs::create_dir_all(home.join(".claude/data")).expect("data dir");
    let mut global_rt = HookRuntime::new(&home).expect("global-side runtime");
    let task_id = new_plan(&mut global_rt, "closed long ago, in the global store");
    let step = add_step(&mut global_rt, &task_id, "only");
    close_step(&mut global_rt, &task_id, &step);
    age_plan(&global_rt, &task_id, 8);

    // Another project's runtime: its own store holds no such plan.
    let (_tmp, project_rt) = setup_runtime();
    let stamped = auto_archive_pass_all_stores(&project_rt.ctx.knowledge);

    assert_eq!(stamped, 1, "the global plan, and only it");
    assert!(
        archived_at_of(&global_rt, &task_id).is_some(),
        "the pass must reach the store the plan lives in"
    );
}

// ───────────────────────────────────────────────────────────────────────
// F3 — reconciling the stages the old closer left behind
// ───────────────────────────────────────────────────────────────────────

/// A stage whose every SIBLING is terminal can be closed by deduction.
///
/// 74 `::scout` rows sat open under mirrors the fixed closer will never be
/// called for: their task never emitted a completion event. 72 of them have
/// all other stages terminal — and `scout` is the stage the others DEPEND on,
/// so if `implement` and `validate` finished, the scouting happened. That is
/// deduction from the DAG's own edges, not invention.
#[test]
fn reconcile_closes_a_stage_whose_siblings_all_finished() {
    let (_tmp, mut rt) = setup_runtime();
    bridge_task_created(
        &mut rt,
        "90",
        "a mirror the closer never saw",
        "sess",
        None,
        None,
    )
    .expect("mirror");
    let mirror = "cc_task_90";
    // The historical shape: implement + validate closed, scout left open.
    for stage in ["implement", "validate"] {
        cli_decompose_update(
            &mut rt,
            &json!({
                "task_id": mirror,
                "subtask_id": format!("{mirror}::{stage}"),
                "status": "completed",
            }),
        );
    }
    assert_eq!(status_of(&rt, &format!("{mirror}::scout")), "pending");

    let preview = parse_json(&cli_decompose_reconcile_stages(
        &mut rt,
        &json!({"dry_run": true}),
    ));
    assert_eq!(preview["candidates"], 1, "the dry run must see the row");
    assert_eq!(preview["closed"], 0);
    assert_eq!(status_of(&rt, &format!("{mirror}::scout")), "pending");

    let done = parse_json(&cli_decompose_reconcile_stages(&mut rt, &json!({})));
    assert_eq!(done["closed"], 1);
    assert_eq!(
        status_of(&rt, &format!("{mirror}::scout")),
        "completed",
        "a stage every sibling outlived is closed by deduction"
    );
}

/// A stage with an OPEN sibling is real pending work and must be left alone.
#[test]
fn reconcile_refuses_to_guess_while_a_sibling_is_still_open() {
    let (_tmp, mut rt) = setup_runtime();
    bridge_task_created(
        &mut rt,
        "91",
        "a mirror still in flight",
        "sess",
        None,
        None,
    )
    .expect("mirror");
    let mirror = "cc_task_91";
    // Only `implement` closed: `validate` is still open, so nothing is decidable.
    cli_decompose_update(
        &mut rt,
        &json!({
            "task_id": mirror,
            "subtask_id": format!("{mirror}::implement"),
            "status": "completed",
        }),
    );

    let done = parse_json(&cli_decompose_reconcile_stages(&mut rt, &json!({})));

    assert_eq!(done["closed"], 0, "an open sibling means undecidable");
    assert_eq!(status_of(&rt, &format!("{mirror}::scout")), "pending");
    assert_eq!(status_of(&rt, &format!("{mirror}::validate")), "pending");
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

    // O id CRU, como o hook `task-completed` o entrega — NÃO `mirror`.
    // Até 20/09/2026 esta linha passava "cc_task_42", normalizando à mão o que
    // a produção não normalizava: o teste corrigia a entrada em vez de exercitar
    // o caminho, e por isso ficou verde sobre um fechador que, vivo, procurava
    // `42::scout` onde havia `cc_task_42::scout`. Teste de componente não é
    // teste do caminho.
    let closed = close_scaffold_stages(&mut rt, "42", true);
    assert_eq!(
        i64::try_from(closed).expect("contagem cabe em i64"),
        scaffolded,
        "o retorno deve contar as linhas REALMENTE escritas ({scaffolded} \
         estágios existiam); contar a ausência de `error` devolvia 3 sobre zero"
    );

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
fn fechar_uma_task_sem_scaffold_nao_conta_estagio_nenhum() {
    let (_tmp, mut rt) = setup_runtime();

    // Nenhum scaffold: a maioria das tasks do banco vivo é assim (criadas por
    // `decompose create`, sem os três estágios do espelho).
    let closed = close_scaffold_stages(&mut rt, "task_que_nunca_existiu", true);

    // O handler NÃO devolve `error` para um subtask inexistente — devolve
    // `"subtask_updated": false`. Contar a ausência de `error` como escrita
    // fazia esta chamada responder 3 tendo escrito 0, e o doc-comment vende o
    // retorno como "how many stages were written". O campo `subtask_missing`
    // foi criado em 25/08/2026 exatamente depois de um fechamento de fase
    // declarar sucesso sobre um subtask que não existia; o fechador repetia o
    // erro que aquele campo existe para impedir.
    assert_eq!(
        closed, 0,
        "nenhuma linha existe para fechar — o contador não pode inventar escrita"
    );
}

#[test]
fn a_failed_task_marks_every_stage_failed() {
    let (_tmp, mut rt) = setup_runtime();
    bridge_task_created(&mut rt, "43", "a CC task that failed", "sess", None, None)
        .expect("mirror must be created");
    let mirror = "cc_task_43";

    // Id CRU, como o hook entrega — este é o único teste do caminho `success=false`.
    close_scaffold_stages(&mut rt, "43", false);

    // Os três nomes vêm LITERAIS de propósito. Percorrer
    // `MIRROR_SCAFFOLD_STAGES` aqui era uma tautologia: o teste iterava a mesma
    // lista que `close_scaffold_stages` itera, então apagar um estágio da
    // constante deixava o teste VERDE (provado por mutação no cross-audit de
    // 20/09/2026 — os outros testes da família só caíram porque carregam
    // `::scout` escrito à mão). Um teste que percorre a própria constante do
    // código não distingue "fechou todos" de "a lista encolheu".
    for stage in ["scout", "implement", "validate"] {
        assert_eq!(
            status_of(&rt, &format!("{mirror}::{stage}")),
            "failed",
            "a failed task mirrors its outcome onto every stage"
        );
    }

    // E a lista do código não pode encolher sem que alguém decida isso: se um
    // estágio sair da constante, é AQUI que se descobre, não num relatório de
    // DAG meses depois.
    assert_eq!(
        touring_foundation::task_lifecycle::MIRROR_SCAFFOLD_STAGES
            .map(|s| s.name)
            .as_slice(),
        ["scout", "implement", "validate"].as_slice(),
        "o espelho tem três estágios; mudá-los é uma decisão, não um efeito colateral"
    );
}
