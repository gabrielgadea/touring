//! Handler-surface tests: the fourteen `handle_*` entry points of `lifecycle` ARE the test surface.
//!
//! The hint helpers behind them (`maybe_*_hint_on_*` and the rest of the `*_hint` family) used to be
//! pinned one by one from `tests.rs`, which forced every helper to be re-exported `pub(crate)` only
//! for the tests and made any helper refactor break tests without a behavior change. Here the
//! scenario is what the hook runtime really sees: a payload into `handle_*`, a string out.
//!
//! Two fixtures, both recorded from the code BEFORE the visibility refactor:
//!
//! * `lifecycle_surface_scenarios.jsonl` — `{id, handler, setup, input, out}`. The REPLAY corpus:
//!   every scenario must reproduce `out` byte for byte (after the declared normalizations).
//!   `JUSTIFIED_DIFFERENCES` lists the scenario ids whose output is allowed to differ, each with
//!   its written reason; a difference is information that has to be justified, not a test to
//!   silence. Each scenario runs with a per-thread `~/.claude` inside its temp root
//!   (`with_claude_dir`), so the federated memory recall cannot read the machine's other projects.
//!   Five recorded outputs (s08bbcf7ed, s1055e59ab, s441cd476c, s7226eb678: a `lesson:` hint;
//!   s3578bf227: `recall: 2` instead of `1`) were corrected on 04/11/2026: their setup stores no
//!   such memory, so those fragments came from the recording machine's real memory store — the
//!   corpus was machine-dependent, not the code.
//! * `lifecycle_surface_checks.jsonl` — `{id, old, scenario, has, lacks}`. The CONTRACT the old
//!   helper tests expressed, restated on the handler output: every `has` string must appear and no
//!   `lacks` string may. `old` is the name of the helper test the check came from; the mapping of
//!   every old test to its new home is `old_dispatch_tests.tsv`, enforced by `contract_tests.rs`.
//!
//! Setup language (the `setup` array, applied to a fresh runtime before the handler runs):
//! `create` (a decompose task from a description, optionally with a DAG of `subtasks`, bound to a
//! variable), `memory` (store a memory entry), `session` (start a Touring session), `dependent`
//! (record that a file imports another). A `{{name}}` in the input, in a setup operation, in
//! `has`/`lacks` or in `out` stands for the value bound to `name` (task ids are time-derived).
//!
//! Normalizations applied to a live output before it is compared with `out` (nothing else is
//! touched): the temp project root becomes `{{ROOT}}`; a bound variable becomes `{{name}}`; any
//! other `task_<digits>` id becomes `{{TASK}}`; an RFC 3339 timestamp becomes `{{TS}}`; and the cut
//! a `[live: ...]` snippet makes inside the `"task_id"` member becomes `"task_id{{CUT}}]`.
//!
//! Regenerate `out` ONLY on purpose (a deliberate behavior change):
//! `SURFACE_DRAFT=<scenarios without out> SURFACE_OUT=<file> cargo test -p touring-dispatch --lib \
//!  record_scenarios -- --ignored`.

use super::tests::{create_intent_task, make_runtime};
use super::*;
use regex::Regex;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::OnceLock;

const SCENARIOS: &str = include_str!("../../tests/fixtures/lifecycle_surface_scenarios.jsonl");
const CHECKS: &str = include_str!("../../tests/fixtures/lifecycle_surface_checks.jsonl");

/// Scenario ids whose live output may differ from the recorded one, each with the written reason.
/// The refactor this corpus guards changed visibility, not behavior; the entries below are the
/// deliberate behavior changes made AFTER it, approved by Gabriel (aprofundamento #7, 04/11/2026).
const JUSTIFIED_DIFFERENCES: &[(&str, &str)] = &[
    (
        "s254c11d4c",
        "#7: task_get reads subtasks by `subtask_id` (the key cli_decompose_get emits) — the \
         scout-pending hint now fires where the recorded output, keyed on a never-emitted `id`, \
         fell through to the chain hint",
    ),
    (
        "s393fbcd09",
        "#7: same key fix — the scout-in-progress hint now fires instead of the MCTS fallback",
    ),
];

type Handler = fn(&mut HookRuntime, &Value) -> String;

/// The fourteen lifecycle entry points, by the name the fixtures use.
const HANDLERS: &[(&str, Handler)] = &[
    ("file_changed", handle_file_changed),
    ("cwd_changed", handle_cwd_changed),
    ("subagent_start", handle_subagent_start),
    ("pre_compact", handle_pre_compact),
    ("worktree_create", handle_worktree_create),
    ("enter_plan_mode", handle_enter_plan_mode),
    ("exit_plan_mode", handle_exit_plan_mode),
    ("task_create", handle_task_sync_post_create),
    ("task_update", handle_task_sync_post_update),
    ("task_list", handle_task_sync_post_list),
    ("task_get", handle_task_sync_post_get),
    ("task_output", handle_task_sync_post_output),
    ("task_stop", handle_task_sync_post_stop),
    ("task_delete", handle_task_sync_post_delete),
];

type SetupOp = fn(&mut HookRuntime, &Value, &mut BTreeMap<String, String>);

const SETUP_OPS: &[(&str, SetupOp)] = &[
    ("create", op_create),
    ("memory", op_memory),
    ("session", op_session),
    ("dependent", op_dependent),
];

fn jsonl(text: &str) -> Vec<Value> {
    serde_json::Deserializer::from_str(text)
        .into_iter::<Value>()
        .map(|v| v.expect("fixture line is valid JSON"))
        .collect()
}

pub(super) fn s<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or("")
}

fn strings(v: &Value, key: &str) -> Vec<String> {
    v.get(key)
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

/// Replace every `{{name}}` by its bound value, recursively through a JSON value.
fn expand(v: &Value, vars: &BTreeMap<String, String>) -> Value {
    match v {
        Value::String(t) => Value::String(expand_str(t, vars)),
        Value::Array(a) => Value::Array(a.iter().map(|x| expand(x, vars)).collect()),
        Value::Object(o) => Value::Object(
            o.iter()
                .map(|(k, x)| (k.clone(), expand(x, vars)))
                .collect(),
        ),
        other => other.clone(),
    }
}

fn expand_str(t: &str, vars: &BTreeMap<String, String>) -> String {
    let mut out = t.to_owned();
    for (name, value) in vars {
        out = out.replace(&format!("{{{{{name}}}}}"), value);
    }
    out
}

/// A full RFC 3339 timestamp (what `chrono::Utc::now().to_rfc3339()` prints).
fn timestamp_re() -> &'static Regex {
    static CELL: OnceLock<Regex> = OnceLock::new();
    CELL.get_or_init(|| {
        Regex::new(r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z|[+-]\d{2}:\d{2})")
            .expect("timestamp regex")
    })
}

/// `[live: {json}]` is the DAG state cut at a fixed byte count. The creation timestamp inside it prints
/// 0/3/6/9 fractional digits, so WHERE the cut lands after that timestamp (even in the middle of the next
/// number) changes from run to run. A snippet that fits whole (ends with `}`) is kept; a truncated one
/// keeps what precedes its first timestamp and that timestamp, and shows `{{CUT}}` for the rest.
fn mask_truncated_snippets(text: &str) -> String {
    static LIVE: OnceLock<Regex> = OnceLock::new();
    static PARTIAL: OnceLock<Regex> = OnceLock::new();
    let live = LIVE
        .get_or_init(|| Regex::new(r"\[live: (\{[^\n]*?)\]( \| |$)").expect("live snippet regex"));
    let partial = PARTIAL.get_or_init(|| {
        Regex::new(r"\d{4}-\d{2}-\d{2}T[\d:.+Z-]*$").expect("partial timestamp regex")
    });
    live.replace_all(text, |m: &regex::Captures| {
        let snippet = &m[1];
        if snippet.ends_with('}') {
            return format!("[live: {snippet}]{}", &m[2]);
        }
        // A cut that fell INSIDE the timestamp leaves a partial one, which is dropped too.
        let keep = timestamp_re()
            .find(snippet)
            .map(|t| t.end())
            .unwrap_or_else(|| partial.find(snippet).map_or(snippet.len(), |p| p.start()));
        format!("[live: {}{{{{CUT}}}}]{}", &snippet[..keep], &m[2])
    })
    .into_owned()
}

fn normalize(out: &str, root: &Path, vars: &BTreeMap<String, String>) -> String {
    static TASK: OnceLock<Regex> = OnceLock::new();
    // A task id is `task_<nanoseconds>`.
    let task = TASK.get_or_init(|| Regex::new(r"task_\d{6,}").expect("task id regex"));
    let mut text = out.to_owned();
    let mut roots = vec![root.to_string_lossy().into_owned()];
    if let Ok(c) = root.canonicalize() {
        roots.push(c.to_string_lossy().into_owned());
    }
    roots.sort_by_key(|r| std::cmp::Reverse(r.len()));
    for r in roots {
        text = text.replace(&r, "{{ROOT}}");
    }
    for (name, value) in vars {
        text = text.replace(value, &format!("{{{{{name}}}}}"));
    }
    let text = mask_truncated_snippets(&text);
    let text = task.replace_all(&text, "{{TASK}}");
    timestamp_re().replace_all(&text, "{{TS}}").into_owned()
}

/// `create`: a decompose task, optionally with a DAG of `subtasks` (`id`, `description`,
/// `depends_on`, `status`) scoped under it.
fn op_create(rt: &mut HookRuntime, op: &Value, vars: &mut BTreeMap<String, String>) {
    let id = create_intent_task(rt, s(op, "description"), "task_missing");
    for sub in op
        .get("subtasks")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        add_subtask(rt, &id, sub);
    }
    vars.insert(s(op, "as").to_owned(), id);
}

fn add_subtask(rt: &mut HookRuntime, task: &str, sub: &Value) {
    let deps: Vec<String> = strings(sub, "depends_on")
        .into_iter()
        .map(|d| {
            if d.contains("::") {
                d
            } else {
                format!("{task}::{d}")
            }
        })
        .collect();
    let _ = crate::cli_handlers::cli_decompose_add(
        rt,
        &json!({
            "task_id": task,
            "subtask_id": s(sub, "id"),
            "description": s(sub, "description"),
            "depends_on": deps,
        }),
    );
    let status = s(sub, "status");
    if !status.is_empty() && status != "pending" {
        let _ = crate::cli_handlers::cli_decompose_update(
            rt,
            &json!({
                "task_id": task,
                "subtask_id": s(sub, "id"),
                "status": status,
                "priority": 5,
            }),
        );
    }
}

fn op_memory(rt: &mut HookRuntime, op: &Value, _vars: &mut BTreeMap<String, String>) {
    let _ = crate::cli_handlers::cli_memory_store(
        rt,
        &json!({
            "key": s(op, "key"),
            "value": s(op, "value"),
            "tier": "semantic",
            "entry_type": "lesson",
        }),
    );
}

fn op_session(rt: &mut HookRuntime, op: &Value, _vars: &mut BTreeMap<String, String>) {
    let _ = crate::cli_handlers::cli_session_start(
        rt,
        &json!({
            "session_id": s(op, "id"),
            "task_type": "task",
            "objective": "handler-surface scenario",
        }),
    );
}

fn op_dependent(rt: &mut HookRuntime, op: &Value, _vars: &mut BTreeMap<String, String>) {
    let rel = crate::knowledge::FileRelation {
        source: s(op, "by").into(),
        target: s(op, "file").into(),
        relation_type: "imports".into(),
    };
    let _ = rt.ctx.knowledge.upsert_relation(&rel);
}

fn apply_setup(rt: &mut HookRuntime, ops: &[Value], vars: &mut BTreeMap<String, String>) {
    for op in ops {
        let op = expand(op, vars);
        let name = s(&op, "op");
        match SETUP_OPS.iter().find(|(n, _)| *n == name) {
            Some((_, run)) => run(rt, &op, vars),
            None => panic!("unknown setup op {name:?}"),
        }
    }
}

fn dispatch(handler: &str, rt: &mut HookRuntime, input: &Value) -> String {
    match HANDLERS.iter().find(|(n, _)| *n == handler) {
        Some((_, run)) => run(rt, input),
        None => panic!("unknown handler {handler:?}"),
    }
}

/// One scenario on a fresh runtime: the NORMALIZED output (so a `{{name}}` in a check is compared as is).
fn run_scenario(sc: &Value) -> String {
    let (tmp, mut rt) = make_runtime();
    let mut vars = BTreeMap::new();
    let setup = sc
        .get("setup")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    // Hermetic (04/11/2026): the WHOLE scenario — setup included — sees a `~/.claude` inside its
    // temp root. Setup stores memories; outside this scope they landed in the machine's real
    // memory store (the test wrote into the user's memory) and the recall below never saw them.
    let claude_dir = tmp.path().join(".claude");
    let out = touring_cli::cli::shared::with_claude_dir(&claude_dir, || {
        apply_setup(&mut rt, &setup, &mut vars);
        let input = expand(&sc["input"], &vars);
        dispatch(s(sc, "handler"), &mut rt, &input)
    });
    normalize(&out, tmp.path(), &vars)
}

type Live = BTreeMap<String, String>;

/// Run every scenario, a few threads at a time (each thread builds its own runtime per scenario).
fn run_all(scenarios: &[Value]) -> Live {
    let workers = 6;
    let chunk = scenarios.len().div_ceil(workers).max(1);
    let mut all = BTreeMap::new();
    std::thread::scope(|scope| {
        let handles: Vec<_> = scenarios
            .chunks(chunk)
            .map(|part| {
                scope.spawn(move || {
                    part.iter()
                        .map(|sc| (s(sc, "id").to_owned(), run_scenario(sc)))
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        for h in handles {
            all.extend(h.join().expect("scenario worker"));
        }
    });
    all
}

fn scenarios() -> &'static Vec<Value> {
    static CELL: OnceLock<Vec<Value>> = OnceLock::new();
    CELL.get_or_init(|| jsonl(SCENARIOS))
}

pub(super) fn checks() -> &'static Vec<Value> {
    static CELL: OnceLock<Vec<Value>> = OnceLock::new();
    CELL.get_or_init(|| jsonl(CHECKS))
}

/// Live outputs of every recorded scenario, computed once and shared by the tests below.
fn live() -> &'static Live {
    static CELL: OnceLock<Live> = OnceLock::new();
    CELL.get_or_init(|| run_all(scenarios()))
}

#[test]
fn replay_reproduces_every_recorded_output_byte_for_byte() {
    let mut failures = Vec::new();
    for sc in scenarios() {
        let id = s(sc, "id");
        if JUSTIFIED_DIFFERENCES.iter().any(|(j, _)| *j == id) {
            continue;
        }
        let got = &live()[id];
        if got != s(sc, "out") {
            failures.push(format!(
                "{id} ({}):\n  recorded: {:?}\n  live:     {:?}",
                s(sc, "handler"),
                s(sc, "out"),
                got
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} scenarios differ from the recorded corpus:\n{}",
        failures.len(),
        scenarios().len(),
        failures.join("\n")
    );
}

#[test]
fn justified_differences_name_real_scenarios_and_carry_a_reason() {
    let ids: BTreeSet<&str> = scenarios().iter().map(|sc| s(sc, "id")).collect();
    for (id, reason) in JUSTIFIED_DIFFERENCES {
        assert!(
            ids.contains(id),
            "JUSTIFIED_DIFFERENCES names unknown scenario {id}"
        );
        assert!(reason.len() > 20, "scenario {id} needs a written reason");
    }
}

#[test]
fn every_check_holds_on_the_handler_output() {
    let mut failures = Vec::new();
    for ck in checks() {
        let id = s(ck, "id");
        // Needles are written in the normalized domain (`{{t}}`, `{{TASK}}`, ...), like the output.
        let out = &live()[s(ck, "scenario")];
        for needle in strings(ck, "has") {
            if !out.contains(&needle) {
                failures.push(format!("{id}: output lacks {needle:?}\n  output: {out}"));
            }
        }
        for needle in strings(ck, "lacks") {
            if out.contains(&needle) {
                failures.push(format!(
                    "{id}: output must not contain {needle:?}\n  output: {out}"
                ));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} checks fail:\n{}",
        failures.len(),
        checks().len(),
        failures.join("\n")
    );
}

#[test]
fn the_fixtures_are_well_formed() {
    let ids: BTreeSet<&str> = scenarios().iter().map(|sc| s(sc, "id")).collect();
    assert_eq!(ids.len(), scenarios().len(), "scenario ids are unique");
    let known: BTreeSet<&str> = HANDLERS.iter().map(|(n, _)| *n).collect();
    assert_eq!(known.len(), HANDLERS.len(), "handler names are unique");
    let mut check_ids = BTreeSet::new();
    let mut used = BTreeSet::new();
    for ck in checks() {
        let id = s(ck, "id");
        assert!(check_ids.insert(id), "check id {id} is duplicated");
        assert!(
            ids.contains(s(ck, "scenario")),
            "check {id} points at an unknown scenario"
        );
        assert!(
            !strings(ck, "has").is_empty() || !strings(ck, "lacks").is_empty(),
            "check {id} asserts nothing"
        );
        assert!(
            !s(ck, "old").is_empty(),
            "check {id} must name the old test it came from"
        );
        used.insert(s(ck, "scenario"));
    }
    for sc in scenarios() {
        let id = s(sc, "id");
        assert!(used.contains(id), "scenario {id} is not used by any check");
        assert!(
            known.contains(s(sc, "handler")),
            "scenario {id} names an unknown handler"
        );
        // An empty `out` is a real contract (a handler given no path returns nothing); the replay pins it.
    }
}

/// A fixture is committed to a public repository: the only home directory allowed in it is the
/// synthetic `/home/u/`.
#[test]
fn the_fixtures_hold_only_synthetic_home_paths() {
    for (name, text) in [("scenarios", SCENARIOS), ("checks", CHECKS)] {
        for (at, _) in text.match_indices("/home/") {
            assert!(
                text[at..].starts_with("/home/u/"),
                "fixture {name} holds a non-synthetic home path near: {}",
                &text[at..(at + 40).min(text.len())]
            );
        }
        // The account that recorded the corpus must not leak outside a path either.
        if let Some(user) = std::env::var("USER").ok().filter(|u| u.len() > 3) {
            assert!(
                !text.contains(&user),
                "fixture {name} holds the recording user name"
            );
        }
    }
}

/// Re-records `out` for a draft scenario file. Deliberate, never run by default.
#[test]
#[ignore = "regenerates the replay corpus; run only to record a deliberate behavior change"]
fn record_scenarios() {
    let draft = std::env::var("SURFACE_DRAFT").expect("SURFACE_DRAFT=<scenarios.jsonl>");
    let target = std::env::var("SURFACE_OUT").expect("SURFACE_OUT=<output.jsonl>");
    let text = std::fs::read_to_string(draft).expect("read draft");
    let mut drafts = jsonl(&text);
    let outs = run_all(&drafts);
    let mut lines = String::new();
    for sc in &mut drafts {
        sc["out"] = Value::String(outs[s(sc, "id")].clone());
        lines.push_str(&serde_json::to_string(sc).expect("serialize scenario"));
        lines.push('\n');
    }
    std::fs::write(target, lines).expect("write recorded scenarios");
}
