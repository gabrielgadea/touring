//! A subtask's body pointer and triage labels, end to end through the handlers the registry
//! dispatches to (S-05, 11/11/2026).
//!
//! Every test drives the LIVE handlers (`crate::cli::decompose::{add, update, get, ready}`) against
//! a real store in a temp project, never the superseded copies: a green test over code that never
//! runs in production is the failure the e2e suite already paid for once (see the 03/08/2026 note
//! in `cli_handlers_e2e.rs`).

use super::*;
use crate::cli::decompose::{
    cli_decompose_add, cli_decompose_create, cli_decompose_get, cli_decompose_ready as live_ready,
    cli_decompose_update,
};
use serde_json::{Value, json};
use touring_hook_runtime::hook_runtime::HookRuntime;

fn rt() -> (tempfile::TempDir, HookRuntime) {
    let tmp = tempfile::tempdir().expect("tempdir");
    let rt = HookRuntime::new(tmp.path()).expect("HookRuntime::new");
    ensure_decompose_tables(&rt.ctx.knowledge);
    (tmp, rt)
}

fn json_of(s: &str) -> Value {
    serde_json::from_str(s).expect("handler emits JSON")
}

/// A task plus a ticket body file under `tickets/`; returns the task id.
fn new_task(rt: &mut HookRuntime) -> String {
    let created = json_of(&cli_decompose_create(
        rt,
        &json!({"task_type": "feature", "description": "body + labels"}),
    ));
    created["task_id"].as_str().expect("task_id").to_string()
}

fn write_body(root: &std::path::Path, name: &str) {
    std::fs::create_dir_all(root.join("tickets")).expect("tickets dir");
    std::fs::write(root.join("tickets").join(name), "# ticket body\n").expect("write body");
}

fn add(rt: &mut HookRuntime, task: &str, sub: &str, extra: Value) -> Value {
    let mut payload =
        json!({"task_id": task, "subtask_id": sub, "description": format!("d {sub}")});
    for (k, v) in extra.as_object().expect("extra is an object") {
        payload[k] = v.clone();
    }
    json_of(&cli_decompose_add(rt, &payload))
}

fn update(rt: &mut HookRuntime, task: &str, sub: &str, extra: Value) -> Value {
    let mut payload = json!({"task_id": task, "subtask_id": sub, "status": ""});
    for (k, v) in extra.as_object().expect("extra is an object") {
        payload[k] = v.clone();
    }
    json_of(&cli_decompose_update(rt, &payload))
}

fn get_sub(rt: &mut HookRuntime, task: &str, sub: &str) -> Value {
    let got = json_of(&cli_decompose_get(rt, &json!({"task_id": task})));
    let scoped = format!("{task}::{sub}");
    got["subtasks"]
        .as_array()
        .and_then(|all| {
            all.iter()
                .find(|s| s["subtask_id"].as_str() == Some(scoped.as_str()))
        })
        .cloned()
        .unwrap_or_else(|| panic!("subtask {sub} not in {got}"))
}

fn ids(v: &Value, key: &str) -> Vec<String> {
    let mut out: Vec<String> = v[key]
        .as_array()
        .unwrap_or_else(|| panic!("{key} missing in {v}"))
        .iter()
        .map(|s| {
            let id = s["subtask_id"].as_str().expect("subtask_id");
            id.rsplit("::").next().unwrap_or(id).to_string()
        })
        .collect();
    out.sort();
    out
}

// (a) add with body-file + labels -> get returns them -------------------------------------------

#[test]
fn add_with_body_file_and_labels_is_returned_by_get() {
    let (tmp, mut rt) = rt();
    write_body(tmp.path(), "S-01.md");
    let task = new_task(&mut rt);

    let reply = add(
        &mut rt,
        &task,
        "S-01",
        json!({"body_file": "tickets/S-01.md", "labels": ["needs-triage", "kind:bug", "needs-triage"]}),
    );
    assert_eq!(reply["persisted"], json!(true), "{reply}");
    assert_eq!(reply["body_file"], json!("tickets/S-01.md"), "{reply}");
    assert_eq!(
        reply["labels"],
        json!(["kind:bug", "needs-triage"]),
        "ordered and duplicate-free: {reply}"
    );

    let sub = get_sub(&mut rt, &task, "S-01");
    assert_eq!(sub["body_file"], json!("tickets/S-01.md"), "{sub}");
    assert_eq!(sub["labels"], json!(["kind:bug", "needs-triage"]), "{sub}");
}

#[test]
fn an_absolute_body_path_under_the_root_is_stored_relative_to_it() {
    let (tmp, mut rt) = rt();
    write_body(tmp.path(), "S-02.md");
    let task = new_task(&mut rt);
    let abs = tmp.path().join("tickets/S-02.md");
    let reply = add(
        &mut rt,
        &task,
        "S-02",
        json!({"body_file": abs.to_str().expect("utf-8 path")}),
    );
    assert_eq!(reply["body_file"], json!("tickets/S-02.md"), "{reply}");
}

// (b) a body file that is not on disk is refused, naming the RESOLVED path ----------------------

#[test]
fn a_missing_body_file_is_refused_and_names_the_resolved_path() {
    let (tmp, mut rt) = rt();
    let task = new_task(&mut rt);
    let resolved = tmp.path().join("tickets/nope.md");

    let reply = add(
        &mut rt,
        &task,
        "S-01",
        json!({"body_file": "tickets/nope.md"}),
    );
    assert_eq!(reply["persisted"], json!(false), "{reply}");
    let error = reply["error"].as_str().expect("error names the path");
    assert!(error.contains(&resolved.display().to_string()), "{error}");
    let after = json_of(&cli_decompose_get(&mut rt, &json!({"task_id": task})));
    assert_eq!(
        after["subtask_count"],
        json!(0),
        "nothing was created: {after}"
    );
}

#[test]
fn a_missing_body_file_on_update_refuses_the_whole_update() {
    let (tmp, mut rt) = rt();
    write_body(tmp.path(), "S-01.md");
    let task = new_task(&mut rt);
    add(&mut rt, &task, "S-01", json!({"labels": ["a"]}));

    let reply = update(
        &mut rt,
        &task,
        "S-01",
        json!({"status": "in_progress", "body_file": "tickets/gone.md", "add_labels": ["b"]}),
    );
    assert_eq!(reply["subtask_updated"], json!(false), "{reply}");
    assert!(
        reply["error"]
            .as_str()
            .is_some_and(|e| e.contains(&tmp.path().join("tickets/gone.md").display().to_string())),
        "{reply}"
    );
    let sub = get_sub(&mut rt, &task, "S-01");
    assert_eq!(
        sub["status"],
        json!("pending"),
        "no half-applied update: {sub}"
    );
    assert_eq!(sub["labels"], json!(["a"]), "{sub}");
}

// (c) an invalid label is refused, showing the pattern ------------------------------------------

#[test]
fn an_invalid_label_is_refused_with_the_pattern_and_writes_nothing() {
    let (_tmp, mut rt) = rt();
    let task = new_task(&mut rt);

    let reply = add(
        &mut rt,
        &task,
        "S-01",
        json!({"labels": ["fine", "Not Fine"]}),
    );
    assert_eq!(reply["persisted"], json!(false), "{reply}");
    let error = reply["error"].as_str().expect("error");
    assert!(error.contains(ticket_meta::LABEL_PATTERN), "{error}");
    assert!(error.contains("Not Fine"), "{error}");
    let after = json_of(&cli_decompose_get(&mut rt, &json!({"task_id": task})));
    assert_eq!(after["subtask_count"], json!(0), "{after}");

    add(&mut rt, &task, "S-02", json!({}));
    for payload in [
        json!({"add_labels": ["UPPER"]}),
        json!({"remove_labels": ["-leading-dash"]}),
        json!({"add_labels": ["same"], "remove_labels": ["same"]}),
    ] {
        let reply = update(&mut rt, &task, "S-02", payload.clone());
        assert_eq!(
            reply["subtask_updated"],
            json!(false),
            "{payload} -> {reply}"
        );
        assert!(reply["error"].is_string(), "{payload} -> {reply}");
    }
    assert_eq!(get_sub(&mut rt, &task, "S-02")["labels"], json!([]));
}

// (d) update --add-label / --remove-label --------------------------------------------------------

#[test]
fn update_adds_and_removes_labels_and_repoints_the_body() {
    let (tmp, mut rt) = rt();
    write_body(tmp.path(), "S-01.md");
    write_body(tmp.path(), "S-01-v2.md");
    let task = new_task(&mut rt);
    add(
        &mut rt,
        &task,
        "S-01",
        json!({"labels": ["a", "b"], "body_file": "tickets/S-01.md"}),
    );

    let reply = update(
        &mut rt,
        &task,
        "S-01",
        json!({"add_labels": ["c", "b"], "remove_labels": ["a", "never-had-it"]}),
    );
    assert_eq!(
        reply["subtask_updated"],
        json!(true),
        "label-only update counts: {reply}"
    );
    assert_eq!(reply["subtask_missing"], json!(false), "{reply}");
    assert_eq!(reply["labels"], json!(["b", "c"]), "{reply}");
    assert_eq!(get_sub(&mut rt, &task, "S-01")["labels"], json!(["b", "c"]));

    let reply = update(
        &mut rt,
        &task,
        "S-01",
        json!({"body_file": "./tickets/S-01-v2.md"}),
    );
    assert_eq!(reply["body_file"], json!("tickets/S-01-v2.md"), "{reply}");
    assert!(
        reply.get("labels").is_none(),
        "labels were not asked for: {reply}"
    );
    let sub = get_sub(&mut rt, &task, "S-01");
    assert_eq!(sub["body_file"], json!("tickets/S-01-v2.md"), "{sub}");
    assert_eq!(
        sub["labels"],
        json!(["b", "c"]),
        "labels untouched by a body update: {sub}"
    );

    let reply = update(&mut rt, &task, "S-01", json!({"remove_labels": ["b", "c"]}));
    assert_eq!(reply["labels"], json!([]), "{reply}");
    assert_eq!(get_sub(&mut rt, &task, "S-01")["labels"], json!([]));
}

#[test]
fn labels_on_a_subtask_that_does_not_exist_report_it_and_write_nothing() {
    let (_tmp, mut rt) = rt();
    let task = new_task(&mut rt);
    let reply = update(&mut rt, &task, "S-99", json!({"add_labels": ["x"]}));
    assert_eq!(reply["subtask_updated"], json!(false), "{reply}");
    assert_eq!(reply["subtask_missing"], json!(true), "{reply}");
}

#[test]
fn a_re_add_that_omits_body_and_labels_keeps_them_and_one_that_states_labels_replaces() {
    let (tmp, mut rt) = rt();
    write_body(tmp.path(), "S-01.md");
    let task = new_task(&mut rt);
    add(
        &mut rt,
        &task,
        "S-01",
        json!({"body_file": "tickets/S-01.md", "labels": ["keep"]}),
    );

    let reply = add(&mut rt, &task, "S-01", json!({"description": "restated"}));
    assert_eq!(reply["existed"], json!(true), "{reply}");
    assert_eq!(
        reply["labels"],
        json!(["keep"]),
        "the reply reports the row, not the call: {reply}"
    );
    assert_eq!(reply["body_file"], json!("tickets/S-01.md"), "{reply}");

    let reply = add(&mut rt, &task, "S-01", json!({"labels": ["replaced"]}));
    assert_eq!(reply["labels"], json!(["replaced"]), "{reply}");
    assert_eq!(
        get_sub(&mut rt, &task, "S-01")["body_file"],
        json!("tickets/S-01.md")
    );
}

/// The compare-and-swap: a writer that lands between the read and the write must not be lost.
#[test]
fn a_label_rewrite_that_loses_a_race_re_applies_on_what_is_there_now() {
    let (_tmp, mut rt) = rt();
    let task = new_task(&mut rt);
    add(&mut rt, &task, "S-01", json!({"labels": ["base"]}));
    let scoped = format!("{task}::S-01");
    let db = &rt.ctx.knowledge;

    let mut interfered = false;
    let after = ticket_meta::rewrite_labels(
        db,
        &scoped,
        |set| {
            if !interfered {
                interfered = true;
                // Another session writes between our read and our write.
                db.conn_ref()
                    .execute(
                        "UPDATE decomposition_subtasks SET labels = ?1 WHERE subtask_id = ?2",
                        rusqlite::params![r#"["base","theirs"]"#, scoped],
                    )
                    .expect("interfering write");
            }
            set.insert("mine".to_string());
        },
        "2026-11-11T00:00:00Z",
    )
    .expect("no error")
    .expect("row exists");
    assert_eq!(
        after,
        vec!["base", "mine", "theirs"],
        "nobody's label was lost"
    );
    assert!(interfered, "the closure ran twice: once lost, once applied");
}

// (e) frontier / ready filter by label ----------------------------------------------------------

fn three_labelled(rt: &mut HookRuntime, tmp: &std::path::Path) -> String {
    write_body(tmp, "S-01.md");
    let task = new_task(rt);
    add(
        rt,
        &task,
        "S-01",
        json!({"body_file": "tickets/S-01.md", "labels": ["ready-for-agent"]}),
    );
    add(
        rt,
        &task,
        "S-02",
        json!({"labels": ["ready-for-agent", "needs-info"]}),
    );
    add(rt, &task, "S-03", json!({}));
    task
}

#[test]
fn ready_filters_by_label_conjunctively_and_returns_both_fields_per_item() {
    let (tmp, mut rt) = rt();
    let task = three_labelled(&mut rt, tmp.path());

    let all = json_of(&cli_decompose_ready(&mut rt, &json!({"task_id": task})));
    assert_eq!(
        ids(&all, "ready_subtasks"),
        vec!["S-01", "S-02", "S-03"],
        "{all}"
    );
    assert!(
        all.get("label_filter").is_none(),
        "no filter, no marker: {all}"
    );
    let first = all["ready_subtasks"]
        .as_array()
        .and_then(|a| {
            a.iter()
                .find(|s| s["subtask_id"] == json!(format!("{task}::S-01")))
        })
        .expect("S-01 listed");
    assert_eq!(first["body_file"], json!("tickets/S-01.md"), "{first}");
    assert_eq!(first["labels"], json!(["ready-for-agent"]), "{first}");

    let one = json_of(&cli_decompose_ready(
        &mut rt,
        &json!({"task_id": task, "labels": ["ready-for-agent"]}),
    ));
    assert_eq!(ids(&one, "ready_subtasks"), vec!["S-01", "S-02"], "{one}");
    assert_eq!(one["ready_count"], json!(2), "{one}");
    assert_eq!(one["label_filter"], json!(["ready-for-agent"]), "{one}");
    assert_eq!(
        one["label_filtered_out"]["ready"],
        json!(1),
        "what the filter hid is counted: {one}"
    );

    let both = json_of(&cli_decompose_ready(
        &mut rt,
        &json!({"task_id": task, "labels": ["ready-for-agent", "needs-info"]}),
    ));
    assert_eq!(
        ids(&both, "ready_subtasks"),
        vec!["S-02"],
        "conjunctive: {both}"
    );

    let none = json_of(&cli_decompose_ready(
        &mut rt,
        &json!({"task_id": task, "labels": ["nobody"]}),
    ));
    assert_eq!(none["ready_count"], json!(0), "{none}");

    let bad = json_of(&cli_decompose_ready(
        &mut rt,
        &json!({"task_id": task, "labels": ["BAD"]}),
    ));
    assert!(
        bad["error"]
            .as_str()
            .is_some_and(|e| e.contains(ticket_meta::LABEL_PATTERN)),
        "{bad}"
    );
}

/// THE defect the wrapper has bitten before: `cli::decompose::cli_decompose_ready` is what the
/// registry dispatches to, and a key it does not forward never reaches the delegate.
#[test]
fn the_dispatched_ready_wrapper_forwards_the_label_filter() {
    let (tmp, mut rt) = rt();
    let task = three_labelled(&mut rt, tmp.path());
    let filtered = json_of(&live_ready(
        &mut rt,
        &json!({"task_id": task, "labels": ["needs-info"]}),
    ));
    assert_eq!(ids(&filtered, "ready_subtasks"), vec!["S-02"], "{filtered}");
}

/// Readiness is a fact about the WHOLE plan: a finished dependency that carries another label still
/// releases its dependent, and the filter only narrows what is listed afterwards.
#[test]
fn the_label_filter_runs_after_readiness_so_a_dependency_with_another_label_still_releases() {
    let (_tmp, mut rt) = rt();
    let task = new_task(&mut rt);
    add(&mut rt, &task, "S-01", json!({"labels": ["infra"]}));
    add(
        &mut rt,
        &task,
        "S-02",
        json!({"labels": ["ui"], "depends_on": ["S-01"]}),
    );
    let blocked = json_of(&cli_decompose_ready(
        &mut rt,
        &json!({"task_id": task, "labels": ["ui"]}),
    ));
    assert_eq!(
        ids(&blocked, "ready_subtasks"),
        Vec::<String>::new(),
        "{blocked}"
    );
    assert_eq!(ids(&blocked, "blocked_subtasks"), vec!["S-02"], "{blocked}");

    update(&mut rt, &task, "S-01", json!({"status": "completed"}));
    let released = json_of(&cli_decompose_ready(
        &mut rt,
        &json!({"task_id": task, "labels": ["ui"]}),
    ));
    assert_eq!(ids(&released, "ready_subtasks"), vec!["S-02"], "{released}");
}

#[test]
fn frontier_filters_what_it_lists_but_not_what_is_true_about_the_map() {
    let (tmp, mut rt) = rt();
    write_body(tmp.path(), "S-00.md");
    let task = new_task(&mut rt);
    add(
        &mut rt,
        &task,
        "S-00",
        json!({"body_file": "tickets/S-00.md", "labels": ["needs-info"]}),
    );
    add(
        &mut rt,
        &task,
        "S-01",
        json!({"labels": ["ready-for-agent"]}),
    );
    add(&mut rt, &task, "S-02", json!({"labels": ["other"]}));
    cli_decompose_ticket(
        &mut rt,
        &json!({"task_id": task, "subtask_id": "S-00", "kind": "decision", "subtype": "grilling", "autonomy": "hitl", "fog": "hazy"}),
    );
    for sub in ["S-01", "S-02"] {
        cli_decompose_ticket(
            &mut rt,
            &json!({"task_id": task, "subtask_id": sub, "kind": "implementation", "origin_ticket": "S-00"}),
        );
    }

    let whole = json_of(&cli_decompose_frontier(&mut rt, &json!({"task_id": task})));
    assert_eq!(ids(&whole, "decisions"), vec!["S-00"], "{whole}");
    assert_eq!(
        ids(&whole, "implementation"),
        vec!["S-01", "S-02"],
        "{whole}"
    );
    let decision = &whole["decisions"][0];
    assert_eq!(
        decision["body_file"],
        json!("tickets/S-00.md"),
        "{decision}"
    );
    assert_eq!(decision["labels"], json!(["needs-info"]), "{decision}");

    let narrowed = json_of(&cli_decompose_frontier(
        &mut rt,
        &json!({"task_id": task, "labels": ["ready-for-agent"]}),
    ));
    assert_eq!(ids(&narrowed, "implementation"), vec!["S-01"], "{narrowed}");
    assert_eq!(
        ids(&narrowed, "decisions"),
        Vec::<String>::new(),
        "{narrowed}"
    );
    assert_eq!(
        narrowed["gated_by_open_decisions"],
        json!(true),
        "the open decision carries another label, and it still gates: {narrowed}"
    );
    assert!(
        narrowed["next_action"]
            .as_str()
            .is_some_and(|a| a.contains("resolve the decision tickets first")),
        "{narrowed}"
    );
    assert_eq!(
        narrowed["label_filter"],
        json!(["ready-for-agent"]),
        "{narrowed}"
    );
    assert_eq!(
        narrowed["label_filtered_out"],
        json!({"decisions": 1, "implementation": 1}),
        "{narrowed}"
    );
}

// (f) a database in the old schema opens and reads the defaults --------------------------------

#[test]
fn a_store_in_the_old_schema_opens_without_loss_and_reads_the_defaults() {
    // The store is reopened the way a daemon restart reopens it (see `reopened_old_schema_runtime`):
    // the migration runs at OPEN, so dropping columns under a live handle would test nothing.
    let (_tmp, mut rt, task) = reopened_old_schema_runtime();

    let got = json_of(&cli_decompose_get(&mut rt, &json!({"task_id": task})));
    let old = get_sub(&mut rt, &task, "OLD");
    assert_eq!(old["description"], json!("legacy row"), "no loss: {got}");
    assert_eq!(old["body_file"], Value::Null, "{old}");
    assert_eq!(old["labels"], json!([]), "{old}");

    let ready = json_of(&cli_decompose_ready(&mut rt, &json!({"task_id": task})));
    assert_eq!(ids(&ready, "ready_subtasks"), vec!["OLD"], "{ready}");
    assert_eq!(ready["ready_subtasks"][0]["labels"], json!([]), "{ready}");

    // ... and a second pass of the ensure is a no-op, not an error.
    ensure_decompose_tables(&rt.ctx.knowledge);
    assert_eq!(get_sub(&mut rt, &task, "OLD")["labels"], json!([]));
}

// (g) the finalize snapshot is the plan's replay record, so it carries both fields ---------------

#[test]
fn the_finalize_snapshot_carries_the_body_pointer_and_the_labels() {
    let (tmp, mut rt) = rt();
    write_body(tmp.path(), "S-01.md");
    let task = new_task(&mut rt);
    add(
        &mut rt,
        &task,
        "S-01",
        json!({"body_file": "tickets/S-01.md", "labels": ["wontfix"]}),
    );
    update(&mut rt, &task, "S-01", json!({"status": "completed"}));

    let finalized = json_of(&cli_decompose_finalize(&mut rt, &json!({"task_id": task})));
    assert!(finalized.get("error").is_none(), "{finalized}");
    let snapshot: String = rt
        .ctx
        .knowledge
        .conn_ref()
        .query_row(
            "SELECT subtasks_snapshot FROM decomposition_snapshots WHERE task_id = ?1",
            rusqlite::params![task],
            |r| r.get(0),
        )
        .expect("finalize wrote a snapshot");
    let rows = json_of(&snapshot);
    assert_eq!(rows[0]["body_file"], json!("tickets/S-01.md"), "{rows}");
    assert_eq!(rows[0]["labels"], json!(["wontfix"]), "{rows}");
}

// ── R6.4 review (11/11/2026): tasksfile round trip, the migration's one door, fail-loud readback ──

/// A subtask carrying a body pointer and labels, exported as a Tasksfile and imported into a NEW
/// task, comes back with both: the export emits them and the import reads them.
#[test]
fn a_tasksfile_round_trip_carries_body_and_labels() {
    let (tmp, mut rt) = rt();
    write_body(tmp.path(), "S-01.md");
    let task = new_task(&mut rt);
    add(
        &mut rt,
        &task,
        "S-01",
        json!({"body_file": "tickets/S-01.md", "labels": ["needs-triage", "kind:bug"]}),
    );

    let exported = json_of(&cli_tasksfile_export(&mut rt, &json!({"task_id": task})));
    let yaml = exported["tasksfile_yaml"].as_str().expect("tasksfile_yaml");
    let created = json_of(&cli_decompose_create(
        &mut rt,
        &json!({"task_type": "tasksfile", "description": "copy", "tasksfile_yaml": yaml}),
    ));
    assert_eq!(
        created["tasksfile_subtasks_added"],
        json!(1),
        "{created}\n{yaml}"
    );

    // Import scopes the exported (already scoped) key under the NEW task id.
    let copy = get_sub(
        &mut rt,
        created["task_id"].as_str().expect("id"),
        &format!("{task}::S-01"),
    );
    assert_eq!(
        copy["body_file"],
        json!("tickets/S-01.md"),
        "{copy}\n{yaml}"
    );
    assert_eq!(
        copy["labels"],
        json!(["kind:bug", "needs-triage"]),
        "{copy}\n{yaml}"
    );
}

/// An OLD Tasksfile (it never heard of these fields) imported over a row that has them must not
/// erase them — the same "not stated means kept" a re-add gives.
#[test]
fn importing_a_file_without_the_fields_keeps_the_rows_body_and_labels() {
    let (tmp, mut rt) = rt();
    write_body(tmp.path(), "S-01.md");
    let task = new_task(&mut rt);
    add(
        &mut rt,
        &task,
        "S-01",
        json!({"body_file": "tickets/S-01.md", "labels": ["keep-me"]}),
    );

    let old_file =
        "version: \"1.0\"\ntasks:\n  S-01:\n    desc: restated by the file\n    command: echo\n";
    let created = json_of(&cli_decompose_create(
        &mut rt,
        &json!({"task_id": task, "task_type": "tasksfile", "description": "x", "tasksfile_yaml": old_file}),
    ));
    assert_eq!(created["tasksfile_subtasks_added"], json!(1), "{created}");
    let row = get_sub(&mut rt, &task, "S-01");
    assert_eq!(
        row["description"],
        json!("restated by the file"),
        "the import did reach the row: {row}"
    );
    assert_eq!(row["body_file"], json!("tickets/S-01.md"), "{row}");
    assert_eq!(row["labels"], json!(["keep-me"]), "{row}");

    // ...and a file that STATES them replaces them, like `add --label`.
    let stating = "version: \"1.0\"\ntasks:\n  S-01:\n    desc: stated\n    command: echo\n    labels: [wontfix]\n";
    cli_decompose_create(
        &mut rt,
        &json!({"task_id": task, "task_type": "tasksfile", "description": "x", "tasksfile_yaml": stating}),
    );
    let row = get_sub(&mut rt, &task, "S-01");
    assert_eq!(row["labels"], json!(["wontfix"]), "{row}");
    assert_eq!(
        row["body_file"],
        json!("tickets/S-01.md"),
        "body not restated, so kept: {row}"
    );
}

/// Import validates what the file carries exactly as `add` does, and says which subtask it refused.
#[test]
fn import_refuses_a_malformed_label_or_a_dangling_body_and_names_the_subtask() {
    let (_tmp, mut rt) = rt();
    let yaml = "version: \"1.0\"\ntasks:\n  good:\n    command: echo\n  bad-label:\n    command: echo\n    labels: [\"Not Valid\"]\n  dangling:\n    command: echo\n    body_file: tickets/missing.md\n";
    let created = json_of(&cli_decompose_create(
        &mut rt,
        &json!({"task_type": "tasksfile", "description": "x", "tasksfile_yaml": yaml}),
    ));
    assert_eq!(
        created["tasksfile_subtasks_added"],
        json!(1),
        "only the good one: {created}"
    );
    let refused = created["tasksfile_subtasks_refused"]
        .as_array()
        .expect("refusals are reported");
    assert_eq!(refused.len(), 2, "{created}");
    let text = created["tasksfile_subtasks_refused"].to_string();
    assert!(
        text.contains("bad-label") && text.contains(ticket_meta::LABEL_PATTERN),
        "{text}"
    );
    assert!(
        text.contains("dangling") && text.contains("tickets/missing.md"),
        "{text}"
    );
}

// ── Item 2: the migration runs where the store is OPENED, not where a handler happens to look ──

const PRE_S05_SCHEMA_VERSION: u32 = 12;

/// Turns the store at `db_path` into the one an install had before S-05: no `body_file`, no
/// `labels`, and the user_version it carried. A legacy row and its task are written by hand.
fn downgrade_to_pre_s05(db_path: &std::path::Path, task: &str) {
    let conn = rusqlite::Connection::open(db_path).expect("open the store");
    conn.execute_batch(&format!(
        "ALTER TABLE decomposition_subtasks DROP COLUMN body_file;
         ALTER TABLE decomposition_subtasks DROP COLUMN labels;
         PRAGMA user_version = {PRE_S05_SCHEMA_VERSION};"
    ))
    .expect("downgrade");
    conn.execute(
        "INSERT OR IGNORE INTO task_decompositions (task_id, task_type, description, status, created_at, updated_at)
         VALUES (?1, 'feature', 'legacy plan', 'created', '2026-01-01', '2026-01-01')",
        rusqlite::params![task],
    )
    .expect("legacy task");
    conn.execute(
        "INSERT INTO decomposition_subtasks (subtask_id, task_id, description, depends_on, status, created_at, updated_at)
         VALUES (?1, ?2, 'legacy row', '[]', 'pending', '2026-01-01', '2026-01-01')",
        rusqlite::params![format!("{task}::OLD"), task],
    )
    .expect("legacy row");
}

/// A project store in the pre-S-05 schema, reopened the way a daemon restart reopens it.
fn reopened_old_schema_runtime() -> (tempfile::TempDir, HookRuntime, String) {
    let tmp = tempfile::tempdir().expect("tempdir");
    let task = {
        let mut first = HookRuntime::new(tmp.path()).expect("HookRuntime::new");
        new_task(&mut first)
    };
    let db_path = touring_foundation::TouringConfig::knowledge_db_canonical(tmp.path());
    downgrade_to_pre_s05(&db_path, &task);
    let rt = HookRuntime::new(tmp.path()).expect("reopen the old-schema store");
    (tmp, rt, task)
}

/// `ready` as the FIRST call on an old-schema store. (`ready` calls the ensure itself at its top,
/// so this passed before the fix too: the review's claim that it never does is not borne out.)
#[test]
fn an_old_schema_store_answers_ready_as_its_first_call() {
    let (_tmp, mut rt, task) = reopened_old_schema_runtime();
    let ready = json_of(&cli_decompose_ready(&mut rt, &json!({"task_id": task})));
    assert!(ready.get("error").is_none(), "{ready}");
    assert_eq!(ids(&ready, "ready_subtasks"), vec!["OLD"], "{ready}");
    assert_eq!(ready["ready_subtasks"][0]["labels"], json!([]), "{ready}");
    assert_eq!(
        ready["ready_subtasks"][0]["body_file"],
        Value::Null,
        "{ready}"
    );
}

/// Same for `frontier` (it also calls the ensure at its top).
#[test]
fn an_old_schema_store_answers_frontier_as_its_first_call() {
    let (_tmp, mut rt, task) = reopened_old_schema_runtime();
    let frontier = json_of(&cli_decompose_frontier(&mut rt, &json!({"task_id": task})));
    assert!(frontier.get("error").is_none(), "{frontier}");
    assert_eq!(ids(&frontier, "implementation"), vec!["OLD"], "{frontier}");
    assert_eq!(
        frontier["implementation"][0]["labels"],
        json!([]),
        "{frontier}"
    );
}

/// Restores HOME when dropped; the global store is found through it.
struct HomeGuard(Option<std::ffi::OsString>);

impl HomeGuard {
    fn set(home: &std::path::Path) -> Self {
        let prior = std::env::var_os("HOME");
        // SAFETY: the test is #[serial] and every HOME reader in this module runs inside it.
        unsafe { std::env::set_var("HOME", home) };
        Self(prior)
    }
}

impl Drop for HomeGuard {
    fn drop(&mut self) {
        // SAFETY: as in `set`.
        unsafe {
            match self.0.take() {
                Some(prior) => std::env::set_var("HOME", prior),
                None => std::env::remove_var("HOME"),
            }
        }
    }
}

/// A task homed in the GLOBAL store (a plan made from another project) in the pre-S-05 schema:
/// `add` and `get` write and read through `store.db()`, a handle no handler ever ensured.
#[test]
#[serial_test::serial(decompose_home)]
fn an_old_schema_global_store_takes_add_and_get() {
    let tmp_home = tempfile::tempdir().expect("home");
    let global_path = tmp_home.path().join(".claude/touring/knowledge.db");
    std::fs::create_dir_all(global_path.parent().expect("parent")).expect("global dir");
    {
        let db = crate::knowledge::FileKnowledgeDB::new(&global_path).expect("global store");
        ensure_decompose_tables(&db);
    }
    downgrade_to_pre_s05(&global_path, "task_global");

    let (tmp, mut rt) = rt();
    write_body(tmp.path(), "S-09.md");
    let _home = HomeGuard::set(tmp_home.path());

    let added = add(
        &mut rt,
        "task_global",
        "S-09",
        json!({"body_file": "tickets/S-09.md", "labels": ["needs-triage"]}),
    );
    assert_eq!(added["persisted"], json!(true), "{added}");
    // A body path in the global store has no stable base, so it is kept absolute.
    assert_eq!(
        added["body_file"],
        json!(tmp.path().join("tickets/S-09.md").to_string_lossy()),
        "{added}"
    );
    let got = json_of(&cli_decompose_get(
        &mut rt,
        &json!({"task_id": "task_global"}),
    ));
    assert!(got.get("error").is_none(), "{got}");
    let new = got["subtasks"]
        .as_array()
        .and_then(|all| {
            all.iter()
                .find(|s| s["subtask_id"] == json!("task_global::S-09"))
        })
        .unwrap_or_else(|| panic!("S-09 missing in {got}"));
    assert_eq!(new["labels"], json!(["needs-triage"]), "{got}");
    let old = got["subtasks"]
        .as_array()
        .and_then(|all| {
            all.iter()
                .find(|s| s["subtask_id"] == json!("task_global::OLD"))
        })
        .expect("the legacy row survives");
    assert_eq!(old["labels"], json!([]), "{got}");
}

// ── Item 3: a failed confirmation read is reported, never turned into "a bare row" ──

/// The add has written; reading the row back to confirm it fails (a BLOB in the `labels` cell — it
/// keeps its type under TEXT affinity, where an integer would be coerced to text — makes the typed
/// read error). The reply must SAY the confirmation failed and why, and must
/// not claim the row has no body and no labels.
#[test]
fn a_failed_confirmation_read_is_reported_not_turned_into_a_bare_row() {
    let (_tmp, mut rt) = rt();
    let task = new_task(&mut rt);
    add(&mut rt, &task, "S-01", json!({"labels": ["a"]}));
    rt.ctx
        .knowledge
        .conn_ref()
        .execute(
            "UPDATE decomposition_subtasks SET labels = X'FF00' WHERE subtask_id = ?1",
            rusqlite::params![format!("{task}::S-01")],
        )
        .expect("corrupt the cell");

    let reply = add(&mut rt, &task, "S-01", json!({"description": "restated"}));
    assert_eq!(
        reply["persisted"],
        json!(true),
        "the write itself succeeded: {reply}"
    );
    let why = reply["readback_error"]
        .as_str()
        .unwrap_or_else(|| panic!("no readback_error in {reply}"));
    assert!(!why.is_empty(), "{reply}");
    assert!(
        reply.get("labels").is_none() && reply.get("body_file").is_none(),
        "no invented bare row: {reply}"
    );
    assert!(
        reply.get("error").is_none(),
        "the bridge reads `error` as a failed add: {reply}"
    );
}

// ── Item 4: the validator and the documented pattern agree, over a corpus, not by sample ──

/// `LABEL_PATTERN` compiled as a regex is the specification; `validate_label` must agree with it
/// on every string of up to three characters over an alphabet of the troublesome ones (1.885
/// strings), plus the length edges 63/64/65 and a trailing newline.
#[test]
fn the_validator_agrees_with_the_pattern_compiled_as_a_regex_over_a_corpus() {
    let pattern = regex::Regex::new(ticket_meta::LABEL_PATTERN).expect("the pattern compiles");
    let alphabet = ['a', 'z', '0', '9', 'A', '-', ':', '_', ' ', '\n', 'é', '.'];
    let mut corpus: Vec<String> = vec![String::new()];
    let mut frontier = vec![String::new()];
    for _ in 0..3 {
        frontier = frontier
            .iter()
            .flat_map(|prefix| alphabet.iter().map(move |c| format!("{prefix}{c}")))
            .collect();
        corpus.extend(frontier.iter().cloned());
    }
    for rest in [62, 63, 64] {
        for first in ['a', '0', '-', ':', '_', 'A'] {
            corpus.push(format!("{first}{}", "a".repeat(rest)));
        }
    }
    corpus.push(format!("a{}\n", "a".repeat(10)));
    corpus.push("a".repeat(64));
    corpus.push("a".repeat(65));

    assert!(corpus.len() > 1900, "{}", corpus.len());
    let disagreements: Vec<&String> = corpus
        .iter()
        .filter(|s| pattern.is_match(s) != ticket_meta::validate_label(s).is_ok())
        .collect();
    assert!(
        disagreements.is_empty(),
        "validator and pattern disagree on {disagreements:?}"
    );
    // The corpus exercises both outcomes (a corpus that only ever matched would prove nothing).
    let accepted = corpus.iter().filter(|s| pattern.is_match(s)).count();
    assert!(
        accepted > 100 && accepted < corpus.len() / 2,
        "{accepted} of {}",
        corpus.len()
    );
}
