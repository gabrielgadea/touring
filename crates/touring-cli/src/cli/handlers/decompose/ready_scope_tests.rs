//! The keys the hook readers depend on (aprofundamento #7, 04/11/2026).
//!
//! Before this, `ready` with no `task_id` ran `WHERE task_id = ''` and answered empty,
//! no producer emitted `ready_count`, and `status` never emitted the `task_count` the
//! readers looked for — so four `touring-dispatch` hints never fired and one fired on
//! every TaskList (with a -0.05 reward each time). These tests pin the producer side
//! of each contract against a real store.

use super::*;
use touring_hook_runtime::hook_runtime::HookRuntime;

fn rt() -> (tempfile::TempDir, HookRuntime) {
    let tmp = tempfile::tempdir().expect("tempdir");
    let rt = HookRuntime::new(tmp.path()).expect("HookRuntime::new");
    ensure_decompose_tables(&rt.ctx.knowledge);
    (tmp, rt)
}

fn task(rt: &HookRuntime, id: &str, status: &str, updated_at: &str) {
    rt.ctx
        .knowledge
        .conn_ref()
        .execute(
            "INSERT INTO task_decompositions (task_id, task_type, description, created_at, updated_at, status)
             VALUES (?1, 'feature', 'd', ?2, ?2, ?3)",
            rusqlite::params![id, updated_at, status],
        )
        .expect("insert task");
}

fn subtask(rt: &HookRuntime, task_id: &str, short: &str, status: &str, deps: &str) {
    rt.ctx
        .knowledge
        .conn_ref()
        .execute(
            "INSERT INTO decomposition_subtasks (subtask_id, task_id, description, depends_on, status, created_at, updated_at)
             VALUES (?1, ?2, 'd', ?3, ?4, '2026-11-04', '2026-11-04')",
            rusqlite::params![format!("{task_id}::{short}"), task_id, deps, status],
        )
        .expect("insert subtask");
}

fn json(s: &str) -> serde_json::Value {
    serde_json::from_str(s).expect("producer emits JSON")
}

fn ready_ids(v: &serde_json::Value) -> Vec<String> {
    let mut ids: Vec<String> = v["ready_subtasks"]
        .as_array()
        .expect("ready_subtasks array")
        .iter()
        .map(|s| s["subtask_id"].as_str().unwrap().to_owned())
        .collect();
    ids.sort();
    ids
}

/// Two open plans (one older, one newer) and one finalized plan with a pending child.
fn three_plans(rt: &HookRuntime) {
    task(rt, "T-old", "in_progress", "2026-11-01T00:00:00Z");
    subtask(rt, "T-old", "a", "pending", "[]");
    task(rt, "T-new", "in_progress", "2026-11-04T00:00:00Z");
    subtask(rt, "T-new", "b", "pending", "[]");
    subtask(rt, "T-new", "c", "pending", "[\"b\"]");
    task(rt, "T-done", "finalized", "2026-11-05T00:00:00Z");
    subtask(rt, "T-done", "z", "pending", "[]");
}

#[test]
fn ready_without_task_id_lists_every_open_plan_not_nothing() {
    let (_tmp, mut rt) = rt();
    three_plans(&rt);
    let v = json(&cli_decompose_ready(&mut rt, &serde_json::json!({})));
    assert_eq!(ready_ids(&v), vec!["T-new::b", "T-old::a"], "{v}");
    assert_eq!(v["ready_count"], 2, "{v}");
}

#[test]
fn ready_scope_latest_reads_only_the_most_recently_updated_open_plan() {
    let (_tmp, mut rt) = rt();
    three_plans(&rt);
    let v = json(&cli_decompose_ready(
        &mut rt,
        &serde_json::json!({"scope": "latest"}),
    ));
    // T-done is newer but finalized; T-new is the latest OPEN plan.
    assert_eq!(ready_ids(&v), vec!["T-new::b"], "{v}");
    assert_eq!(v["ready_count"], 1, "{v}");
}

#[test]
fn ready_with_task_id_is_unchanged_and_now_carries_ready_count() {
    let (_tmp, mut rt) = rt();
    three_plans(&rt);
    let v = json(&cli_decompose_ready(
        &mut rt,
        &serde_json::json!({"task_id": "T-done"}),
    ));
    // An explicit task id is honoured even for a finalized plan, as before.
    assert_eq!(ready_ids(&v), vec!["T-done::z"], "{v}");
    assert_eq!(v["ready_count"], 1, "{v}");
    let only = json(&cli_decompose_ready(
        &mut rt,
        &serde_json::json!({"task_id": "T-new", "only_ready": true}),
    ));
    assert_eq!(only["ready_count"], 1, "{only}");
}

#[test]
fn ready_scope_latest_on_an_empty_store_is_zero_not_an_error() {
    let (_tmp, mut rt) = rt();
    let v = json(&cli_decompose_ready(
        &mut rt,
        &serde_json::json!({"scope": "latest"}),
    ));
    assert_eq!(v["ready_count"], 0, "{v}");
    assert!(v.get("error").is_none(), "{v}");
}

#[test]
fn status_counts_open_tasks_apart_from_the_total() {
    let (_tmp, mut rt) = rt();
    three_plans(&rt);
    let v = json(&cli_decompose_status(&mut rt, &serde_json::json!({})));
    assert_eq!(v["total_tasks"], 3, "{v}");
    assert_eq!(v["open_tasks"], 2, "{v}");
    assert_eq!(v["total_subtasks"], 4, "{v}");
    assert!(v.get("task_count").is_none(), "no reader may key on it: {v}");
}
