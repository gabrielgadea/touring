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
use touring_hooks::cli_handlers_decompose::{
    cli_decompose_archive, cli_decompose_reconcile_stages,
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
            "review_required": true,
        }),
    );

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
    let refused = parse_json(&cli_decompose_finalize(&mut rt, &json!({"task_id": task_id})));
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
    bridge_task_created(&mut rt, "90", "a mirror the closer never saw", "sess", None, None)
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
    bridge_task_created(&mut rt, "91", "a mirror still in flight", "sess", None, None)
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
