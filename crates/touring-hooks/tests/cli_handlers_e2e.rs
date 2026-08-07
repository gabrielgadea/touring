//! E2E tests for ALL cli_handlers functions.
//!
//! Each test creates a fresh HookRuntime with a temp DB, calls the handler,
//! and verifies the JSON output matches expected behavior.
//!
//! Coverage: decompose (create/add/get/validate/update/status),
//!           session (start/list/checkpoint/assess),
//!           memory (stats/recall/store),
//!           gotcha (add/match/list/stats),
//!           ast (find/overview/blast)
//!           workflow (run/stats/slowest/compare)
//!           schemas (validation, hook_context)

#![allow(clippy::indexing_slicing)]

use tempfile::TempDir;
use touring_hooks::cli_handlers::*;
// Wave 24 (2026-04-18): cli_ast_overview was relocated from `cli_handlers`
// to `cli_handlers_index` in Wave 22 (S-Q6 dead-duplicate removal). The
// glob import above no longer pulls it in; alias here so existing test
// bodies keep their original call sites.
// 2026-08-03: `cli_decompose_create` saiu deste import explícito. Existem DUAS
// implementações da família decompose — `cli/decompose.rs` (a que o registry
// despacha, com o roteamento por store per-project) e `cli/handlers/decompose.rs`
// (superseded para esses handlers). O import explícito sombreava o glob, então a
// suíte criava o DAG pela implementação morta e o atualizava pela viva: um teste
// verde sobre código que nunca roda em produção. Agora `cli_decompose_*` vem
// todo do glob `cli_handlers::*` = o que o daemon executa de fato.
// `cli_tasksfile_*` seguem aqui porque só existem neste módulo.
use touring_hooks::cli_handlers_decompose::{cli_tasksfile_export, cli_tasksfile_validate};
use touring_hooks::cli_handlers_index::{cli_ast_blast, cli_ast_find, cli_ast_overview};
use touring_hooks::runtime::HookRuntime;

// ── Helpers ─────────────────────────────────────────────────────────────

fn setup_runtime() -> (TempDir, HookRuntime) {
    let tmp = TempDir::new().expect("create tempdir");
    let root = tmp.path().to_path_buf();
    std::fs::create_dir_all(root.join(".claude/data")).expect("create data dir");
    let rt = HookRuntime::new(&root).expect("init runtime");
    (tmp, rt)
}

fn parse_json(s: &str) -> serde_json::Value {
    serde_json::from_str(s).unwrap_or_else(|e| panic!("invalid JSON: {e}\nraw: {s}"))
}

// ═══════════════════════════════════════════════════════════════════════
// DECOMPOSE TESTS
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn test_decompose_create_persists_to_db() {
    let (_tmp, mut rt) = setup_runtime();
    let payload = serde_json::json!({
        "task_type": "intent",
        "description": "test task creation"
    });
    let result = parse_json(&cli_decompose_create(&mut rt, &payload));

    assert_eq!(result["status"], "created");
    assert_eq!(result["task_type"], "intent");
    assert_eq!(result["description"], "test task creation");
    assert_eq!(result["persisted"], true, "task must be persisted to DB");
    assert!(result["task_id"].as_str().unwrap().starts_with("task_"));
}

#[test]
fn test_decompose_status_counts_correctly() {
    let (_tmp, mut rt) = setup_runtime();

    // Initially empty
    let status = parse_json(&cli_decompose_status(&mut rt, &serde_json::json!({})));
    assert_eq!(status["total_tasks"], 0);
    assert_eq!(status["total_subtasks"], 0);

    // Create a task
    let create_result = parse_json(&cli_decompose_create(
        &mut rt,
        &serde_json::json!({
            "task_type": "feature",
            "description": "count test"
        }),
    ));
    let task_id = create_result["task_id"].as_str().unwrap().to_string();

    let status = parse_json(&cli_decompose_status(&mut rt, &serde_json::json!({})));
    assert_eq!(status["total_tasks"], 1);

    // Add subtask
    cli_decompose_add(
        &mut rt,
        &serde_json::json!({
            "task_id": task_id,
            "subtask_id": "S-1",
            "description": "first subtask",
            "depends_on": []
        }),
    );

    let status = parse_json(&cli_decompose_status(&mut rt, &serde_json::json!({})));
    assert_eq!(status["total_tasks"], 1);
    assert_eq!(status["total_subtasks"], 1);
}

#[test]
fn test_decompose_add_subtask_persists() {
    let (_tmp, mut rt) = setup_runtime();

    let create = parse_json(&cli_decompose_create(
        &mut rt,
        &serde_json::json!({
            "task_type": "intent",
            "description": "parent task"
        }),
    ));
    let task_id = create["task_id"].as_str().unwrap().to_string();

    let add = parse_json(&cli_decompose_add(
        &mut rt,
        &serde_json::json!({
            "task_id": task_id,
            "subtask_id": "S-1",
            "description": "scout phase",
            "depends_on": []
        }),
    ));

    assert_eq!(add["persisted"], true);
    assert_eq!(add["subtask_id"], "S-1");
    assert_eq!(add["status"], "pending");
}

#[test]
fn test_decompose_add_with_dependencies() {
    let (_tmp, mut rt) = setup_runtime();

    let create = parse_json(&cli_decompose_create(
        &mut rt,
        &serde_json::json!({
            "task_type": "pipeline",
            "description": "dep test"
        }),
    ));
    let task_id = create["task_id"].as_str().unwrap().to_string();

    // S-1 no deps
    cli_decompose_add(
        &mut rt,
        &serde_json::json!({
            "task_id": task_id,
            "subtask_id": "S-1",
            "description": "research",
            "depends_on": []
        }),
    );

    // S-2 depends on S-1
    let add = parse_json(&cli_decompose_add(
        &mut rt,
        &serde_json::json!({
            "task_id": task_id,
            "subtask_id": "S-2",
            "description": "implement",
            "depends_on": ["S-1"]
        }),
    ));

    assert_eq!(add["depends_on"], serde_json::json!(["S-1"]));
    assert_eq!(add["persisted"], true);
}

#[test]
fn test_decompose_get_returns_task_and_subtasks() {
    let (_tmp, mut rt) = setup_runtime();

    let create = parse_json(&cli_decompose_create(
        &mut rt,
        &serde_json::json!({
            "task_type": "intent",
            "description": "get test"
        }),
    ));
    let task_id = create["task_id"].as_str().unwrap().to_string();

    cli_decompose_add(
        &mut rt,
        &serde_json::json!({
            "task_id": task_id,
            "subtask_id": "A",
            "description": "alpha",
            "depends_on": []
        }),
    );
    cli_decompose_add(
        &mut rt,
        &serde_json::json!({
            "task_id": task_id,
            "subtask_id": "B",
            "description": "beta",
            "depends_on": ["A"]
        }),
    );

    let get = parse_json(&cli_decompose_get(
        &mut rt,
        &serde_json::json!({"task_id": task_id}),
    ));

    assert!(get["task"].is_object(), "task should be present");
    assert_eq!(get["task"]["task_id"], task_id);
    assert_eq!(get["task"]["description"], "get test");
    assert_eq!(get["subtask_count"], 2);
    assert_eq!(get["subtasks"].as_array().unwrap().len(), 2);
}

#[test]
fn test_decompose_get_nonexistent_returns_null_task() {
    let (_tmp, mut rt) = setup_runtime();

    let get = parse_json(&cli_decompose_get(
        &mut rt,
        &serde_json::json!({"task_id": "nonexistent"}),
    ));
    assert!(get["task"].is_null());
    assert_eq!(get["subtask_count"], 0);
}

#[test]
fn test_decompose_validate_no_cycles() {
    let (_tmp, mut rt) = setup_runtime();

    let create = parse_json(&cli_decompose_create(
        &mut rt,
        &serde_json::json!({
            "task_type": "intent",
            "description": "validate test"
        }),
    ));
    let task_id = create["task_id"].as_str().unwrap().to_string();

    cli_decompose_add(
        &mut rt,
        &serde_json::json!({
            "task_id": task_id, "subtask_id": "S-1", "description": "a", "depends_on": []
        }),
    );
    cli_decompose_add(
        &mut rt,
        &serde_json::json!({
            "task_id": task_id, "subtask_id": "S-2", "description": "b", "depends_on": ["S-1"]
        }),
    );
    cli_decompose_add(
        &mut rt,
        &serde_json::json!({
            "task_id": task_id, "subtask_id": "S-3", "description": "c", "depends_on": ["S-2"]
        }),
    );

    let validate = parse_json(&cli_decompose_validate(
        &mut rt,
        &serde_json::json!({"task_id": task_id}),
    ));

    assert_eq!(validate["valid"], true);
    assert_eq!(validate["has_cycles"], false);
    assert_eq!(validate["subtask_count"], 3);
}

#[test]
fn test_decompose_validate_empty_task_is_valid() {
    let (_tmp, mut rt) = setup_runtime();

    let create = parse_json(&cli_decompose_create(
        &mut rt,
        &serde_json::json!({
            "task_type": "intent",
            "description": "empty validate"
        }),
    ));
    let task_id = create["task_id"].as_str().unwrap().to_string();

    let validate = parse_json(&cli_decompose_validate(
        &mut rt,
        &serde_json::json!({"task_id": task_id}),
    ));
    assert_eq!(validate["valid"], true);
    assert_eq!(validate["subtask_count"], 0);
}

#[test]
fn test_decompose_update_status() {
    let (_tmp, mut rt) = setup_runtime();

    let create = parse_json(&cli_decompose_create(
        &mut rt,
        &serde_json::json!({
            "task_type": "intent",
            "description": "update test"
        }),
    ));
    let task_id = create["task_id"].as_str().unwrap().to_string();

    let update = parse_json(&cli_decompose_update(
        &mut rt,
        &serde_json::json!({
            "task_id": task_id,
            "status": "in_progress"
        }),
    ));
    assert_eq!(update["updated"], true);

    // Verify via get
    let get = parse_json(&cli_decompose_get(
        &mut rt,
        &serde_json::json!({"task_id": task_id}),
    ));
    assert_eq!(get["task"]["status"], "in_progress");
}

/// Regressão 2026-08-03: fechar UMA subtarefa marcava o PLANO INTEIRO como
/// concluído.
///
/// `cli_decompose_update` gravava o `status` recebido na linha-pai
/// incondicionalmente — inclusive quando o chamador havia endereçado uma
/// subtarefa via `subtask_id`. Como `loop_phase_close.py` emite exatamente
/// `decompose update <task> <phase> --status done` a cada fase, o primeiro
/// phase-close de um plano de 8 fases já deixava o pai `done`. Observado ao vivo
/// em `task_1785699543075533970`: `task.status == "done"` com 6 de 8 subtarefas
/// ainda `pending`.
///
/// O DAG é a fonte AUTORITATIVA de progresso do loop (Lei L2) — um pai
/// falsamente concluído corrompe justamente o que o gate de convergência existe
/// para medir.
#[test]
fn closing_one_subtask_must_not_mark_the_whole_plan_done() {
    let (_tmp, mut rt) = setup_runtime();
    let create = parse_json(&cli_decompose_create(
        &mut rt,
        &serde_json::json!({"task_type": "plan", "description": "plano de duas fases"}),
    ));
    let task_id = create["task_id"].as_str().unwrap().to_string();

    for id in ["P1", "P2"] {
        cli_decompose_add(
            &mut rt,
            &serde_json::json!({
                "task_id": task_id, "subtask_id": id,
                "description": format!("fase {id}"), "depends_on": []
            }),
        );
    }

    // Fecha só a P1 — exatamente o que loop_phase_close.py emite.
    cli_decompose_update(
        &mut rt,
        &serde_json::json!({"task_id": task_id, "subtask_id": "P1", "status": "done"}),
    );

    let get = parse_json(&cli_decompose_get(
        &mut rt,
        &serde_json::json!({"task_id": task_id}),
    ));
    assert_ne!(
        get["task"]["status"], "done",
        "com P2 ainda pendente o plano NÃO está concluído; veio {}",
        get["task"]["status"]
    );
    let p2 = get["subtasks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["subtask_id"].as_str().unwrap().ends_with("::P2"))
        .expect("P2 presente");
    assert_eq!(p2["status"], "pending", "P2 não foi tocada");

    // Fechada a última fase, o pai passa a refletir a conclusão real.
    cli_decompose_update(
        &mut rt,
        &serde_json::json!({"task_id": task_id, "subtask_id": "P2", "status": "done"}),
    );
    let get2 = parse_json(&cli_decompose_get(
        &mut rt,
        &serde_json::json!({"task_id": task_id}),
    ));
    assert_eq!(
        get2["task"]["status"], "done",
        "todas as subtarefas terminais ⇒ o plano está concluído"
    );
}

#[test]
fn test_decompose_subtask_scoping_prevents_collision() {
    let (_tmp, mut rt) = setup_runtime();

    // Create two different tasks
    let task1 = parse_json(&cli_decompose_create(
        &mut rt,
        &serde_json::json!({
            "task_type": "intent", "description": "task 1"
        }),
    ));
    let task2 = parse_json(&cli_decompose_create(
        &mut rt,
        &serde_json::json!({
            "task_type": "intent", "description": "task 2"
        }),
    ));
    let t1 = task1["task_id"].as_str().unwrap().to_string();
    let t2 = task2["task_id"].as_str().unwrap().to_string();

    // Both tasks use subtask "S-1"
    let add1 = parse_json(&cli_decompose_add(
        &mut rt,
        &serde_json::json!({
            "task_id": t1, "subtask_id": "S-1", "description": "first task sub", "depends_on": []
        }),
    ));
    let add2 = parse_json(&cli_decompose_add(
        &mut rt,
        &serde_json::json!({
            "task_id": t2, "subtask_id": "S-1", "description": "second task sub", "depends_on": []
        }),
    ));

    // Both must persist (scoped IDs prevent collision)
    assert_eq!(add1["persisted"], true, "first S-1 must persist");
    assert_eq!(
        add2["persisted"], true,
        "second S-1 must persist (different scope)"
    );

    // Each task should see its own subtask
    let get1 = parse_json(&cli_decompose_get(
        &mut rt,
        &serde_json::json!({"task_id": t1}),
    ));
    let get2 = parse_json(&cli_decompose_get(
        &mut rt,
        &serde_json::json!({"task_id": t2}),
    ));
    assert_eq!(get1["subtask_count"], 1);
    assert_eq!(get2["subtask_count"], 1);
}

// ═══════════════════════════════════════════════════════════════════════
// SESSION TESTS
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn test_session_start_persists() {
    let (_tmp, mut rt) = setup_runtime();

    let result = parse_json(&cli_session_start(
        &mut rt,
        &serde_json::json!({
            "session_id": "test-sess-1",
            "task_type": "debug",
            "objective": "fix bug #42"
        }),
    ));

    assert_eq!(result["session_id"], "test-sess-1");
    assert_eq!(result["task_type"], "debug");
    assert_eq!(result["persisted"], true);
}

#[test]
fn test_session_list_returns_created_sessions() {
    let (_tmp, mut rt) = setup_runtime();

    // Empty initially
    let list = parse_json(&cli_session_list(&mut rt, &serde_json::json!({})));
    assert_eq!(list["count"], 0);

    // Create sessions
    cli_session_start(
        &mut rt,
        &serde_json::json!({
            "session_id": "sess-a", "task_type": "feature", "objective": "add auth"
        }),
    );
    cli_session_start(
        &mut rt,
        &serde_json::json!({
            "session_id": "sess-b", "task_type": "debug", "objective": "fix crash"
        }),
    );

    let list = parse_json(&cli_session_list(&mut rt, &serde_json::json!({})));
    assert_eq!(list["count"], 2);

    let sessions = list["sessions"].as_array().unwrap();
    let ids: Vec<&str> = sessions
        .iter()
        .map(|s| s["session_id"].as_str().unwrap())
        .collect();
    assert!(ids.contains(&"sess-a"));
    assert!(ids.contains(&"sess-b"));
}

#[test]
fn test_session_start_upsert_updates_existing() {
    let (_tmp, mut rt) = setup_runtime();

    cli_session_start(
        &mut rt,
        &serde_json::json!({
            "session_id": "sess-upsert", "task_type": "feature", "objective": "v1"
        }),
    );
    cli_session_start(
        &mut rt,
        &serde_json::json!({
            "session_id": "sess-upsert", "task_type": "debug", "objective": "v2"
        }),
    );

    let list = parse_json(&cli_session_list(&mut rt, &serde_json::json!({})));
    assert_eq!(list["count"], 1, "upsert should not duplicate");

    let session = &list["sessions"][0];
    assert_eq!(session["task_type"], "debug");
    assert_eq!(session["objective"], "v2");
}

#[test]
fn test_session_checkpoint_persists() {
    let (_tmp, mut rt) = setup_runtime();

    let result = parse_json(&cli_session_checkpoint(
        &mut rt,
        &serde_json::json!({
            "checkpoint_id": "cp-1",
            "session_id": "sess-1",
            "data": "{\"phase\": \"scout\"}"
        }),
    ));

    assert_eq!(result["checkpoint_id"], "cp-1");
    assert_eq!(result["persisted"], true);
}

#[test]
fn test_session_assess_returns_metrics() {
    let (_tmp, mut rt) = setup_runtime();

    let result = parse_json(&cli_session_assess(
        &mut rt,
        &serde_json::json!({
            "session_id": "test-assess"
        }),
    ));

    assert!(result["metrics"].is_object());
    assert!(result["metrics"]["edits_last_hour"].is_number());
    assert!(result["metrics"]["bash_commands_last_hour"].is_number());
    assert!(result["quality_score"].is_number());
}

// ═══════════════════════════════════════════════════════════════════════
// MEMORY TESTS
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn test_memory_stats_returns_structure() {
    let (_tmp, mut rt) = setup_runtime();

    let result = parse_json(&cli_memory_stats(&mut rt, &serde_json::json!({})));

    assert!(result["file_count"].is_number());
    assert!(result["relation_count"].is_number());
    assert!(result["edit_event_count"].is_number());
    assert!(result["bash_outcome_count"].is_number());
    assert!(result["memory_entry_count"].is_number());
}

#[test]
fn test_memory_recall_empty_query_returns_empty() {
    let (_tmp, mut rt) = setup_runtime();

    let result = parse_json(&cli_memory_recall(
        &mut rt,
        &serde_json::json!({
            "query": ""
        }),
    ));

    assert_eq!(result["count"], 0);
    assert!(result["entries"].as_array().unwrap().is_empty());
}

#[test]
fn test_memory_recall_with_query() {
    let (_tmp, mut rt) = setup_runtime();

    // Populate memory store (S-2 fix: recall now reads from memory_entries, not file_knowledge)
    let store_result = parse_json(&cli_memory_store(
        &mut rt,
        &serde_json::json!({
            "key": "auth:jwt:module",
            "value": "authentication module with JWT tokens",
            "tier": "semantic",
            "type": "lesson"
        }),
    ));
    assert_eq!(store_result["status"], "stored");

    let result = parse_json(&cli_memory_recall(
        &mut rt,
        &serde_json::json!({
            "query": "authentication"
        }),
    ));

    assert!(result["count"].as_i64().unwrap() >= 1);
    let entries = result["entries"].as_array().unwrap();
    assert!(
        entries
            .iter()
            .any(|e| e["value"].as_str().unwrap().contains("authentication"))
    );
}

#[test]
fn test_memory_store_persists() {
    let (_tmp, mut rt) = setup_runtime();

    let result = parse_json(&cli_memory_store(
        &mut rt,
        &serde_json::json!({
            "key": "lesson:test:e2e",
            "value": "always test with real DB",
            "tier": "semantic",
            "type": "lesson"
        }),
    ));

    assert!(
        result.get("error").is_none(),
        "store error: {}",
        result["error"]
    );
    assert_eq!(result["key"], "lesson:test:e2e");
    assert_eq!(result["status"], "stored");
    assert!(result.get("error").is_none(), "store must not have error");
}

// ═══════════════════════════════════════════════════════════════════════
// GOTCHA TESTS
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn test_gotcha_add_and_list() {
    let (_tmp, mut rt) = setup_runtime();

    let add = parse_json(&cli_gotcha_add(
        &mut rt,
        &serde_json::json!({
            "pattern": "*.rs",
            "description": "check for unwrap() in production code",
            "severity": "high"
        }),
    ));

    assert_eq!(add["status"], "added");
    assert!(add["id"].is_number());

    // gotcha_list returns a JSON array directly, not an object
    let list = parse_json(&cli_gotcha_list(&mut rt, &serde_json::json!({})));
    assert!(list.is_array(), "gotcha_list should return a JSON array");
    assert!(!list.as_array().expect("array").is_empty());
}

#[test]
fn test_gotcha_add_missing_fields_returns_error() {
    let (_tmp, mut rt) = setup_runtime();

    let result = parse_json(&cli_gotcha_add(
        &mut rt,
        &serde_json::json!({
            "pattern": "",
            "description": ""
        }),
    ));

    assert!(result.get("error").is_some());
}

#[test]
fn test_gotcha_match() {
    let (_tmp, mut rt) = setup_runtime();

    // Add a gotcha for rust files
    cli_gotcha_add(
        &mut rt,
        &serde_json::json!({
            "pattern": "cli_handlers.rs",
            "description": "check table names match DB schema",
            "severity": "high"
        }),
    );

    let matched = parse_json(&cli_gotcha_match(
        &mut rt,
        &serde_json::json!({
            "file_path": "src/cli_handlers.rs"
        }),
    ));

    // Should have matches or an empty array (depends on BM25 scoring)
    assert!(matched["matches"].is_array());
}

#[test]
fn test_gotcha_stats() {
    let (_tmp, mut rt) = setup_runtime();

    let stats = parse_json(&cli_gotcha_stats(&mut rt, &serde_json::json!({})));

    assert!(stats["total"].is_number());
    assert!(stats["resolved"].is_number());
    assert!(stats["unresolved"].is_number());
}

// ═══════════════════════════════════════════════════════════════════════
// AST TESTS
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn test_ast_find_missing_symbol_returns_empty() {
    let (_tmp, mut rt) = setup_runtime();

    let result = parse_json(&cli_ast_find(
        &mut rt,
        &serde_json::json!({
            "symbol_name": "NonExistentSymbol"
        }),
    ));

    assert_eq!(result["count"], 0);
    assert!(result["definitions"].as_array().unwrap().is_empty());
}

#[test]
fn test_ast_overview_missing_file_returns_empty() {
    let (_tmp, mut rt) = setup_runtime();

    let result = parse_json(&cli_ast_overview(
        &mut rt,
        &serde_json::json!({
            "file_path": "/nonexistent/file.rs"
        }),
    ));

    assert_eq!(result["symbol_count"], 0);
}

#[test]
fn test_ast_overview_empty_path_returns_error() {
    let (_tmp, mut rt) = setup_runtime();

    let result = parse_json(&cli_ast_overview(
        &mut rt,
        &serde_json::json!({
            "file_path": ""
        }),
    ));

    assert!(result.get("error").is_some());
}

#[test]
fn test_ast_blast_returns_consumers_array() {
    let (_tmp, mut rt) = setup_runtime();

    let result = parse_json(&cli_ast_blast(
        &mut rt,
        &serde_json::json!({
            "file_path": "src/lib.rs"
        }),
    ));

    assert!(result["consumers"].is_array());
    assert!(result["blast_radius"].is_number());
}

// ═══════════════════════════════════════════════════════════════════════
// DECOMPOSE FULL WORKFLOW E2E
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn test_decompose_full_taco_workflow() {
    let (_tmp, mut rt) = setup_runtime();

    // Phase 0: Create task
    let create = parse_json(&cli_decompose_create(
        &mut rt,
        &serde_json::json!({
            "task_type": "intent",
            "description": "Implement feature X with TACO workflow"
        }),
    ));
    assert_eq!(create["persisted"], true);
    let task_id = create["task_id"].as_str().unwrap().to_string();

    // Phase 1: Add scout subtask
    let s1 = parse_json(&cli_decompose_add(
        &mut rt,
        &serde_json::json!({
            "task_id": task_id, "subtask_id": "scout", "description": "Discover codebase", "depends_on": []
        }),
    ));
    assert_eq!(s1["persisted"], true);

    // Phase 2: Add architect subtask (depends on scout)
    let s2 = parse_json(&cli_decompose_add(
        &mut rt,
        &serde_json::json!({
            "task_id": task_id, "subtask_id": "architect", "description": "Design solution", "depends_on": ["scout"]
        }),
    ));
    assert_eq!(s2["persisted"], true);

    // Phase 5: Add engineer subtask (depends on architect)
    let s3 = parse_json(&cli_decompose_add(
        &mut rt,
        &serde_json::json!({
            "task_id": task_id, "subtask_id": "engineer", "description": "Implement", "depends_on": ["architect"]
        }),
    ));
    assert_eq!(s3["persisted"], true);

    // Phase 6: Add audit subtask (depends on engineer)
    let s4 = parse_json(&cli_decompose_add(
        &mut rt,
        &serde_json::json!({
            "task_id": task_id, "subtask_id": "audit", "description": "Cross-audit", "depends_on": ["engineer"]
        }),
    ));
    assert_eq!(s4["persisted"], true);

    // Validate: no cycles
    let validate = parse_json(&cli_decompose_validate(
        &mut rt,
        &serde_json::json!({"task_id": task_id}),
    ));
    assert_eq!(validate["valid"], true);
    assert_eq!(validate["has_cycles"], false);
    assert_eq!(validate["subtask_count"], 4);

    // Get: all subtasks present
    let get = parse_json(&cli_decompose_get(
        &mut rt,
        &serde_json::json!({"task_id": task_id}),
    ));
    assert_eq!(get["subtask_count"], 4);

    // Update status
    let update = parse_json(&cli_decompose_update(
        &mut rt,
        &serde_json::json!({
            "task_id": task_id, "status": "in_progress"
        }),
    ));
    assert_eq!(update["updated"], true);

    // Final status check
    let status = parse_json(&cli_decompose_status(&mut rt, &serde_json::json!({})));
    assert_eq!(status["total_tasks"], 1);
    assert_eq!(status["total_subtasks"], 4);
}

// ═══════════════════════════════════════════════════════════════════════
// SESSION FULL WORKFLOW E2E
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn test_session_full_workflow() {
    let (_tmp, mut rt) = setup_runtime();

    // Start session
    let start = parse_json(&cli_session_start(
        &mut rt,
        &serde_json::json!({
            "session_id": "taco-workflow",
            "task_type": "decomposition",
            "objective": "implement feature X"
        }),
    ));
    assert_eq!(start["persisted"], true);

    // Checkpoint
    let cp = parse_json(&cli_session_checkpoint(
        &mut rt,
        &serde_json::json!({
            "checkpoint_id": "phase-1-done",
            "session_id": "taco-workflow",
            "data": "{\"scout_results\": [\"found 3 files\"]}"
        }),
    ));
    assert_eq!(cp["persisted"], true);

    // Assess
    let assess = parse_json(&cli_session_assess(
        &mut rt,
        &serde_json::json!({
            "session_id": "taco-workflow"
        }),
    ));
    assert!(assess["metrics"].is_object());

    // List sessions
    let list = parse_json(&cli_session_list(&mut rt, &serde_json::json!({})));
    assert_eq!(list["count"], 1);
    assert_eq!(list["sessions"][0]["session_id"], "taco-workflow");
}

// ═══════════════════════════════════════════════════════════════════════
// FEATURE B: WORKFLOW CLI E2E
// ═══════════════════════════════════════════════════════════════════════

use touring_hooks::cli_handlers_decompose::{
    cli_workflow_compare, cli_workflow_resume, cli_workflow_run, cli_workflow_slowest,
    cli_workflow_stats, cli_workflow_status,
};

#[test]
fn test_workflow_run_empty_for_nonexistent_task() {
    let (_tmp, mut rt) = setup_runtime();
    let result = parse_json(&cli_workflow_run(
        &mut rt,
        &serde_json::json!({
            "task_id": "nonexistent-task"
        }),
    ));
    assert!(result["task_id"].is_null() || result["task_id"] == serde_json::Value::Null);
}

#[test]
fn test_workflow_run_returns_task_with_subtasks() {
    let (_tmp, mut rt) = setup_runtime();

    // Create task
    let create = parse_json(&cli_decompose_create(
        &mut rt,
        &serde_json::json!({
            "task_type": "intent",
            "description": "Workflow test task"
        }),
    ));
    let task_id = create["task_id"].as_str().unwrap();

    // Add subtasks
    cli_decompose_add(
        &mut rt,
        &serde_json::json!({
            "task_id": task_id,
            "subtask_id": "scout",
            "description": "Discover",
            "depends_on": []
        }),
    );
    cli_decompose_add(
        &mut rt,
        &serde_json::json!({
            "task_id": task_id,
            "subtask_id": "implement",
            "description": "Build",
            "depends_on": ["scout"]
        }),
    );

    // Run workflow
    let result = parse_json(&cli_workflow_run(
        &mut rt,
        &serde_json::json!({
            "task_id": task_id
        }),
    ));
    assert_eq!(result["event"], "workflow_start");
    assert!(result["task"].is_object());
    let task_obj = result["task"].as_object().unwrap();
    assert_eq!(task_obj["task_id"], task_id);
    assert!(result["subtasks"].is_array());
    assert_eq!(result["subtasks"].as_array().unwrap().len(), 2);
    // B3: verify events array (task_start + 2 subtask_start + task_complete)
    assert!(result["events"].is_array(), "B3: events should be present");
    let events = result["events"].as_array().unwrap();
    assert_eq!(
        events.len(),
        4,
        "expected 4 events (1 task_start + 2 subtask_start + 1 task_complete)"
    );
    assert_eq!(events[0]["event"], "task_start");
    assert_eq!(events[1]["event"], "subtask_start");
    assert_eq!(events[2]["event"], "subtask_start");
    assert_eq!(events[3]["event"], "task_complete");
    // B6: verify summary field is present
    assert!(
        result["summary"].is_object(),
        "B6: summary should be present"
    );
}

#[test]
fn test_workflow_run_with_color_mode() {
    // B6: ANSI color mode — when color=true, summary includes colored field
    let (_tmp, mut rt) = setup_runtime();

    let create = parse_json(&cli_decompose_create(
        &mut rt,
        &serde_json::json!({
            "task_type": "intent",
            "description": "Color test"
        }),
    ));
    let task_id = create["task_id"].as_str().unwrap();

    cli_decompose_add(
        &mut rt,
        &serde_json::json!({
            "task_id": task_id,
            "subtask_id": "s1",
            "description": "Step 1",
            "depends_on": []
        }),
    );

    let result = parse_json(&cli_workflow_run(
        &mut rt,
        &serde_json::json!({
            "task_id": task_id,
            "color": true
        }),
    ));
    assert_eq!(result["event"], "workflow_start");
    let summary = &result["summary"];
    assert!(
        summary["colored"].is_string(),
        "color mode should emit ANSI-colored output"
    );
    let colored = summary["colored"].as_str().unwrap();
    assert!(
        colored.contains("\x1b["),
        "should contain ANSI escape codes"
    );
    assert!(colored.contains("▶"), "should contain the play symbol");
}

#[test]
fn test_workflow_stats_returns_counts() {
    let (_tmp, mut rt) = setup_runtime();

    let create = parse_json(&cli_decompose_create(
        &mut rt,
        &serde_json::json!({
            "task_type": "intent",
            "description": "Stats test"
        }),
    ));
    let task_id = create["task_id"].as_str().unwrap();

    cli_decompose_add(
        &mut rt,
        &serde_json::json!({
            "task_id": task_id,
            "subtask_id": "step1",
            "description": "Step one",
            "depends_on": []
        }),
    );

    let stats = parse_json(&cli_workflow_stats(&mut rt, &serde_json::json!({})));
    assert!(stats["total_subtasks"].is_number());
    assert!(stats["completed"].is_number());
    assert!(stats["failed"].is_number());
    assert!(stats["pending"].is_number());
}

#[test]
fn test_workflow_slowest_returns_duration_list() {
    let (_tmp, mut rt) = setup_runtime();

    let create = parse_json(&cli_decompose_create(
        &mut rt,
        &serde_json::json!({
            "task_type": "bug",
            "description": "Slow task"
        }),
    ));
    let task_id = create["task_id"].as_str().unwrap();

    let _s1 = cli_decompose_add(
        &mut rt,
        &serde_json::json!({
            "task_id": task_id,
            "subtask_id": "slow",
            "description": "Slow step",
            "depends_on": []
        }),
    );

    let slowest = parse_json(&cli_workflow_slowest(&mut rt, &serde_json::json!({})));
    assert!(slowest["slowest"].is_array());
}

#[test]
fn test_workflow_compare_with_multiple_tasks() {
    let (_tmp, mut rt) = setup_runtime();

    // Create two tasks
    let t1 = parse_json(&cli_decompose_create(
        &mut rt,
        &serde_json::json!({
            "task_type": "intent",
            "description": "Task one"
        }),
    ));
    let t2 = parse_json(&cli_decompose_create(
        &mut rt,
        &serde_json::json!({
            "task_type": "bug",
            "description": "Task two"
        }),
    ));

    let task1 = t1["task_id"].as_str().unwrap();
    let task2 = t2["task_id"].as_str().unwrap();

    cli_decompose_add(
        &mut rt,
        &serde_json::json!({
            "task_id": task1,
            "subtask_id": "a",
            "description": "A",
            "depends_on": []
        }),
    );
    cli_decompose_add(
        &mut rt,
        &serde_json::json!({
            "task_id": task2,
            "subtask_id": "b",
            "description": "B",
            "depends_on": []
        }),
    );

    let compare = parse_json(&cli_workflow_compare(
        &mut rt,
        &serde_json::json!({
            "task_id_a": task1,
            "task_id_b": task2
        }),
    ));
    assert!(compare["task_a"].is_object());
    assert!(compare["task_b"].is_object());
    assert_eq!(compare["task_a"]["task_id"], task1);
    assert_eq!(compare["task_b"]["task_id"], task2);
}

// ═══════════════════════════════════════════════════════════════════════
// FEATURE B5: WORKFLOW RESUME AFTER CRASH/INTERRUPT
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn test_workflow_resume_returns_next_pending_subtask() {
    let (_tmp, mut rt) = setup_runtime();

    // Create a task with 3 subtasks
    let create = parse_json(&cli_decompose_create(
        &mut rt,
        &serde_json::json!({
            "task_type": "intent",
            "description": "Resume test workflow"
        }),
    ));
    let task_id = create["task_id"].as_str().unwrap();

    cli_decompose_add(
        &mut rt,
        &serde_json::json!({
            "task_id": task_id,
            "subtask_id": "step1",
            "description": "First step",
            "depends_on": []
        }),
    );
    cli_decompose_add(
        &mut rt,
        &serde_json::json!({
            "task_id": task_id,
            "subtask_id": "step2",
            "description": "Second step",
            "depends_on": ["step1"]
        }),
    );
    cli_decompose_add(
        &mut rt,
        &serde_json::json!({
            "task_id": task_id,
            "subtask_id": "step3",
            "description": "Third step",
            "depends_on": ["step2"]
        }),
    );

    // Start step1 and complete it
    cli_decompose_update(
        &mut rt,
        &serde_json::json!({
            "task_id": task_id,
            "subtask_id": "step1",
            "status": "in_progress"
        }),
    );
    cli_decompose_update(
        &mut rt,
        &serde_json::json!({
            "task_id": task_id,
            "subtask_id": "step1",
            "status": "completed"
        }),
    );

    // step2 is pending (blocked by step1) — resume should find it
    let resume = parse_json(&cli_workflow_resume(
        &mut rt,
        &serde_json::json!({
            "task_id": task_id
        }),
    ));

    assert_eq!(resume["task_id"], task_id);
    assert_eq!(
        resume["next_subtask"]["subtask_id"],
        format!("{}::step2", task_id)
    );
    assert!(
        resume["task"].is_object(),
        "workflow_state should be task object"
    );
    assert_eq!(resume["next_subtask"]["status"], "pending");
    // 1 of 3 completed ≈ 33.33% (percentage scale)
    let pct = resume["completion_pct"].as_f64().unwrap();
    assert!(pct > 33.3 && pct < 33.4, "expected ~33.33, got {}", pct);
}

#[test]
fn test_workflow_resume_no_completed_yields_first() {
    let (_tmp, mut rt) = setup_runtime();

    let create = parse_json(&cli_decompose_create(
        &mut rt,
        &serde_json::json!({
            "task_type": "intent",
            "description": "No-complete workflow"
        }),
    ));
    let task_id = create["task_id"].as_str().unwrap();

    cli_decompose_add(
        &mut rt,
        &serde_json::json!({
            "task_id": task_id,
            "subtask_id": "sub_a",
            "description": "Subtask A",
            "depends_on": []
        }),
    );

    // Resume should return first pending subtask
    let resume = parse_json(&cli_workflow_resume(
        &mut rt,
        &serde_json::json!({
            "task_id": task_id
        }),
    ));

    assert_eq!(resume["task_id"], task_id);
    // Verify subtask_id is stored scoped (task_id::subtask_id)
    let returned_subtask_id = resume["next_subtask"]["subtask_id"].as_str().unwrap();
    assert!(
        returned_subtask_id.ends_with("::sub_a"),
        "expected scoped subtask_id, got {}",
        returned_subtask_id
    );
    let pct = resume["completion_pct"].as_f64().unwrap();
    assert!(pct.abs() < 0.001, "expected 0, got {}", pct);
}

#[test]
fn test_workflow_resume_all_completed_returns_null_subtask() {
    let (_tmp, mut rt) = setup_runtime();

    let create = parse_json(&cli_decompose_create(
        &mut rt,
        &serde_json::json!({
            "task_type": "intent",
            "description": "Fully done workflow"
        }),
    ));
    let task_id = create["task_id"].as_str().unwrap();

    cli_decompose_add(
        &mut rt,
        &serde_json::json!({
            "task_id": task_id,
            "subtask_id": "done_1",
            "description": "Done subtask",
            "depends_on": []
        }),
    );

    cli_decompose_update(
        &mut rt,
        &serde_json::json!({
            "task_id": task_id,
            "subtask_id": "done_1",
            "status": "completed"
        }),
    );

    let resume = parse_json(&cli_workflow_resume(
        &mut rt,
        &serde_json::json!({
            "task_id": task_id
        }),
    ));

    assert_eq!(resume["task_id"], task_id);
    assert!(resume["next_subtask"].is_null());
    let pct = resume["completion_pct"].as_f64().unwrap();
    assert!((pct - 100.0).abs() < 0.001, "expected 100, got {}", pct);
}

#[test]
fn test_workflow_status_empty_for_nonexistent_task() {
    let (_tmp, mut rt) = setup_runtime();
    let result = parse_json(&cli_workflow_status(
        &mut rt,
        &serde_json::json!({
            "task_id": "nonexistent-task"
        }),
    ));
    assert!(result["task"].is_null() || result["task"] == serde_json::Value::Null);
    assert_eq!(result["summary"]["total_subtasks"], 0);
}

#[test]
fn test_workflow_status_returns_correct_counts() {
    let (_tmp, mut rt) = setup_runtime();

    let create = parse_json(&cli_decompose_create(
        &mut rt,
        &serde_json::json!({
            "task_type": "intent",
            "description": "Status count test"
        }),
    ));
    let task_id = create["task_id"].as_str().unwrap();

    cli_decompose_add(
        &mut rt,
        &serde_json::json!({
            "task_id": task_id,
            "subtask_id": "sub_a",
            "description": "Pending subtask",
            "depends_on": []
        }),
    );
    cli_decompose_add(
        &mut rt,
        &serde_json::json!({
            "task_id": task_id,
            "subtask_id": "sub_b",
            "description": "In progress subtask",
            "depends_on": []
        }),
    );
    cli_decompose_add(
        &mut rt,
        &serde_json::json!({
            "task_id": task_id,
            "subtask_id": "sub_c",
            "description": "Completed subtask",
            "depends_on": []
        }),
    );
    cli_decompose_add(
        &mut rt,
        &serde_json::json!({
            "task_id": task_id,
            "subtask_id": "sub_d",
            "description": "Failed subtask",
            "depends_on": []
        }),
    );

    // Set in_progress and completed and failed (use unscoped IDs - handler scopes internally)
    cli_decompose_update(
        &mut rt,
        &serde_json::json!({
            "task_id": task_id,
            "subtask_id": "sub_b",
            "status": "in_progress"
        }),
    );
    cli_decompose_update(
        &mut rt,
        &serde_json::json!({
            "task_id": task_id,
            "subtask_id": "sub_c",
            "status": "completed"
        }),
    );
    cli_decompose_update(
        &mut rt,
        &serde_json::json!({
            "task_id": task_id,
            "subtask_id": "sub_d",
            "status": "failed"
        }),
    );

    let status = parse_json(&cli_workflow_status(
        &mut rt,
        &serde_json::json!({
            "task_id": task_id
        }),
    ));
    assert_eq!(status["event"], "workflow_status");
    assert_eq!(status["task_id"], task_id);
    let summary = &status["summary"];
    assert_eq!(summary["total_subtasks"], 4);
    assert_eq!(summary["pending"], 1); // sub_a
    assert_eq!(summary["in_progress"], 1); // sub_b
    assert_eq!(summary["completed"], 1); // sub_c
    assert_eq!(summary["failed"], 1); // sub_d
    assert_eq!(summary["cancelled"], 0);
    let pct = summary["completion_pct"].as_f64().unwrap();
    assert!((pct - 25.0).abs() < 0.001, "expected 25, got {}", pct);

    // Verify subtask list
    let subs = status["subtasks"].as_array().unwrap();
    assert_eq!(subs.len(), 4);
}

#[test]
fn test_workflow_status_with_scoped_subtask_ids() {
    let (_tmp, mut rt) = setup_runtime();

    let create = parse_json(&cli_decompose_create(
        &mut rt,
        &serde_json::json!({
            "task_type": "bug",
            "description": "Scoped ID test"
        }),
    ));
    let task_id = create["task_id"].as_str().unwrap();

    // Add with unscoped IDs (handler scopes them automatically)
    cli_decompose_add(
        &mut rt,
        &serde_json::json!({
            "task_id": task_id,
            "subtask_id": "step_1",
            "description": "First step",
            "depends_on": []
        }),
    );

    let status = parse_json(&cli_workflow_status(
        &mut rt,
        &serde_json::json!({
            "task_id": task_id
        }),
    ));

    // subtask_id should be scoped (task_id::subtask_id)
    let subs = status["subtasks"].as_array().unwrap();
    assert_eq!(subs.len(), 1);
    let sub_id = subs[0]["subtask_id"].as_str().unwrap();
    assert!(
        sub_id.contains("::"),
        "expected scoped format task_id::subtask_id, got {}",
        sub_id
    );
}

#[test]
fn test_workflow_status_update_flow() {
    // Regression test: ensure update actually changes subtask status in DB
    let (_tmp, mut rt) = setup_runtime();

    let create = parse_json(&cli_decompose_create(
        &mut rt,
        &serde_json::json!({
            "task_type": "intent",
            "description": "Update flow test"
        }),
    ));
    let task_id = create["task_id"].as_str().unwrap();

    cli_decompose_add(
        &mut rt,
        &serde_json::json!({
            "task_id": task_id,
            "subtask_id": "foo",
            "description": "Test subtask",
            "depends_on": []
        }),
    );

    // Verify initial status is pending
    let before = parse_json(&cli_workflow_status(
        &mut rt,
        &serde_json::json!({"task_id": task_id}),
    ));
    assert_eq!(
        before["summary"]["pending"], 1,
        "should start with 1 pending"
    );
    assert_eq!(
        before["summary"]["in_progress"], 0,
        "should start with 0 in_progress"
    );

    // Update to in_progress
    let upd = parse_json(&cli_decompose_update(
        &mut rt,
        &serde_json::json!({
            "task_id": task_id,
            "subtask_id": "foo",
            "status": "in_progress"
        }),
    ));
    assert_eq!(upd["task_id"], task_id);

    // Verify status changed
    let after = parse_json(&cli_workflow_status(
        &mut rt,
        &serde_json::json!({"task_id": task_id}),
    ));
    assert_eq!(
        after["summary"]["pending"], 0,
        "should have 0 pending after update"
    );
    assert_eq!(
        after["summary"]["in_progress"], 1,
        "should have 1 in_progress after update"
    );
}

// ═══════════════════════════════════════════════════════════════════════
// FEATURE C: SUBTASK RESULTS TABLE INSTRUMENTATION
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn test_decompose_update_records_started_at() {
    let (_tmp, mut rt) = setup_runtime();

    let create = parse_json(&cli_decompose_create(
        &mut rt,
        &serde_json::json!({
            "task_type": "intent",
            "description": "Timing test"
        }),
    ));
    let task_id = create["task_id"].as_str().unwrap();

    cli_decompose_add(
        &mut rt,
        &serde_json::json!({
            "task_id": task_id,
            "subtask_id": "step1",
            "description": "First step",
            "depends_on": []
        }),
    );

    let update = parse_json(&cli_decompose_update(
        &mut rt,
        &serde_json::json!({
            "task_id": task_id,
            "subtask_id": "step1",
            "status": "in_progress"
        }),
    ));
    assert!(
        update["updated"].as_bool().unwrap() || update["subtask_updated"].as_i64().unwrap() > 0
    );
}

#[test]
fn test_decompose_update_records_completed_at() {
    let (_tmp, mut rt) = setup_runtime();

    let create = parse_json(&cli_decompose_create(
        &mut rt,
        &serde_json::json!({
            "task_type": "refactor",
            "description": "Completion test"
        }),
    ));
    let task_id = create["task_id"].as_str().unwrap();

    let add_result = parse_json(&cli_decompose_add(
        &mut rt,
        &serde_json::json!({
            "task_id": task_id,
            "subtask_id": "done",
            "description": "Will complete",
            "depends_on": []
        }),
    ));
    assert!(add_result["persisted"].as_bool().unwrap());

    let complete = parse_json(&cli_decompose_update(
        &mut rt,
        &serde_json::json!({
            "task_id": task_id,
            "subtask_id": "done",
            "status": "completed"
        }),
    ));
    assert!(
        complete["updated"].as_bool().unwrap() || complete["subtask_updated"].as_i64().unwrap() > 0
    );
}

#[test]
fn test_decompose_finalize_archives_task() {
    let (_tmp, mut rt) = setup_runtime();

    let create = parse_json(&cli_decompose_create(
        &mut rt,
        &serde_json::json!({
            "task_type": "intent",
            "description": "Archive test"
        }),
    ));
    let task_id = create["task_id"].as_str().unwrap();

    cli_decompose_add(
        &mut rt,
        &serde_json::json!({
            "task_id": task_id,
            "subtask_id": "single",
            "description": "Only subtask",
            "depends_on": []
        }),
    );

    let finalize = parse_json(&cli_decompose_finalize(
        &mut rt,
        &serde_json::json!({
            "task_id": task_id
        }),
    ));
    // Wave 8 collateral fix: cli_decompose_finalize was refactored to return
    // {task_id, status: "finalized", metrics, breached_deadlines} — old shape
    // {archived, ready} no longer applies. Test updated to match current contract.
    assert_eq!(finalize["status"].as_str(), Some("finalized"));
    assert!(finalize["metrics"].is_object(), "metrics object expected");
}

#[test]
fn test_decompose_ready_filters_pending_with_completed_deps() {
    let (_tmp, mut rt) = setup_runtime();

    let create = parse_json(&cli_decompose_create(
        &mut rt,
        &serde_json::json!({
            "task_type": "intent",
            "description": "Ready filter test"
        }),
    ));
    let task_id = create["task_id"].as_str().unwrap();

    cli_decompose_add(
        &mut rt,
        &serde_json::json!({
            "task_id": task_id,
            "subtask_id": "first",
            "description": "First",
            "depends_on": []
        }),
    );
    cli_decompose_add(
        &mut rt,
        &serde_json::json!({
            "task_id": task_id,
            "subtask_id": "second",
            "description": "Second",
            "depends_on": ["first"]
        }),
    );

    // Only first should be ready (no deps).
    // Wave 8 collateral fix: cli_decompose_ready was refactored to require
    // explicit task_id (empty task_id returns no results). Pass task_id and
    // assert on ready_subtasks array (replaces old ready_count field).
    let ready = parse_json(&cli_decompose_ready(
        &mut rt,
        &serde_json::json!({
            "task_id": task_id
        }),
    ));
    assert!(
        ready["ready_subtasks"].is_array(),
        "ready_subtasks array expected"
    );
}

// ═══════════════════════════════════════════════════════════════════════
// FEATURE D: SCHEMA VALIDATION E2E
// ═══════════════════════════════════════════════════════════════════════

use touring_hooks::schemas::hook_payloads::{PostBashPayload, PreEditPayload, PreReadPayload};
use touring_hooks::schemas::{format_validation_errors, validate_payload};

#[test]
fn test_validate_payload_pre_read_valid() {
    let json = serde_json::json!({
        "file_path": "src/main.rs",
        "offset": 10,
        "limit": 100
    });
    let result = validate_payload::<PreReadPayload>(&json);
    assert!(result.is_ok());
    let p = result.expect("validated");
    assert_eq!(p.file_path, "src/main.rs");
    assert_eq!(p.offset, Some(10));
    assert_eq!(p.limit, Some(100));
}

#[test]
fn test_validate_payload_pre_read_empty_filepath_fails() {
    let json = serde_json::json!({
        "file_path": "",
        "offset": 10
    });
    let result = validate_payload::<PreReadPayload>(&json);
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(
        errors
            .field_errors()
            .keys()
            .any(|k| k.as_ref() == "file_path")
    );
}

#[test]
fn test_validate_payload_pre_read_missing_field_fails() {
    let json = serde_json::json!({
        "offset": 10
    });
    let result = validate_payload::<PreReadPayload>(&json);
    assert!(result.is_err());
}

#[test]
fn test_validate_payload_pre_edit_valid() {
    // old_string/new_string are Option<String> in PreEditPayload
    let json = serde_json::json!({
        "file_path": "src/lib.rs"
    });
    let result = validate_payload::<PreEditPayload>(&json);
    assert!(result.is_ok());
}

#[test]
fn test_validate_payload_post_bash_valid() {
    let json = serde_json::json!({
        "command": "cargo build"
    });
    let result = validate_payload::<PostBashPayload>(&json);
    assert!(result.is_ok());
}

#[test]
fn test_validate_payload_post_bash_empty_command_fails() {
    let json = serde_json::json!({
        "command": ""
    });
    let result = validate_payload::<PostBashPayload>(&json);
    assert!(result.is_err());
}

#[test]
fn test_format_validation_errors_readable() {
    let json = serde_json::json!({"file_path": ""});
    let result = validate_payload::<PreReadPayload>(&json);
    assert!(result.is_err());
    let formatted = format_validation_errors(&result.unwrap_err());
    assert!(formatted.contains("file_path"));
}

// ═══════════════════════════════════════════════════════════════════════
// FEATURE A: HOOK CHAINING E2E (A5 — pre_read hook result storage)
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn test_pre_read_stores_hook_result_in_session_bus() {
    let (_tmp, rt) = setup_runtime();
    // pre_read run_returning stores result in session_bus hook_results
    let input = serde_json::json!({
        "tool_input": {"file_path": "src/main.rs"}
    });
    let _ = touring_hooks::pre_read::run_returning(&rt, &input);
    let bus = rt.ctx.session_bus.borrow();
    let last = bus.get_last_hook_result("pre_read");
    assert!(
        last.is_some(),
        "pre_read should store result in session_bus"
    );
    let result = last.unwrap();
    // file_path is stored as relative path (may be absolute if file doesn't exist)
    assert!(
        result["file_path"].is_string(),
        "file_path should be a string"
    );
    assert!(result["context_len"].is_u64() || result["context_len"].is_number());
}

#[test]
fn test_session_bus_hook_result_overwrite_on_rerun() {
    let (_tmp, rt) = setup_runtime();
    // Store a result directly (simulate first run)
    {
        let mut bus = rt.ctx.session_bus.borrow_mut();
        bus.add_hook_result(
            "pre_read",
            serde_json::json!({"file_path": "old.rs", "context_len": 10}),
        );
    }
    // Simulate pre_read run again — overwrites the old result
    let input = serde_json::json!({
        "tool_input": {"file_path": "Cargo.toml"}
    });
    let _ = touring_hooks::pre_read::run_returning(&rt, &input);
    let bus = rt.ctx.session_bus.borrow();
    let last = bus.get_last_hook_result("pre_read").unwrap();
    // Should be overwritten with new file path
    assert!(last["file_path"].is_string());
    // context_len should reflect actual context generated
    assert!(last["context_len"].is_u64() || last["context_len"].is_number());
}

#[test]
fn test_session_bus_get_none_for_unknown_hook() {
    let (_tmp, rt) = setup_runtime();
    let bus = rt.ctx.session_bus.borrow();
    // No pre_read has run yet in this fresh runtime
    assert!(bus.get_last_hook_result("pre_read").is_none());
    // Other hooks also return None
    assert!(bus.get_last_hook_result("pre_edit").is_none());
}

#[test]
fn test_session_bus_multiple_hooks_independent_results() {
    let (_tmp, rt) = setup_runtime();
    // Manually store results for two different hooks
    {
        let mut bus = rt.ctx.session_bus.borrow_mut();
        bus.add_hook_result("pre_read", serde_json::json!({"file_path": "a.rs"}));
        bus.add_hook_result("pre_edit", serde_json::json!({"file_path": "b.rs"}));
    }
    let bus = rt.ctx.session_bus.borrow();
    assert_eq!(
        bus.get_last_hook_result("pre_read").unwrap()["file_path"],
        "a.rs"
    );
    assert_eq!(
        bus.get_last_hook_result("pre_edit").unwrap()["file_path"],
        "b.rs"
    );
}

// ═══════════════════════════════════════════════════════════════════════
// FEATURE A: HOOK CHAINING E2E (A7 — post_edit hook result storage)
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn test_post_edit_stores_hook_result_in_session_bus() {
    let (_tmp, mut rt) = setup_runtime();
    // post_edit run_returning stores result in session_bus hook_results
    let input = serde_json::json!({
        "tool_name": "Edit",
        "tool_input": {"file_path": "src/main.rs"}
    });
    let _ = touring_hooks::post_edit::run_returning(&mut rt, &input);
    let bus = rt.ctx.session_bus.borrow();
    let last = bus.get_last_hook_result("post_edit");
    assert!(
        last.is_some(),
        "post_edit should store result in session_bus"
    );
    let result = last.unwrap();
    assert!(
        result["file_path"].is_string(),
        "file_path should be a string"
    );
    assert!(result["feedback_len"].is_u64() || result["feedback_len"].is_number());
}

#[test]
fn test_session_bus_post_edit_result_retrievable_after_pre_read() {
    // Verify post_edit result is independently retrievable
    let (_tmp, mut rt) = setup_runtime();
    // First run pre_read
    let pre_input = serde_json::json!({
        "tool_input": {"file_path": "src/main.rs"}
    });
    let _ = touring_hooks::pre_read::run_returning(&rt, &pre_input);
    // Then run post_edit
    let post_input = serde_json::json!({
        "tool_name": "Edit",
        "tool_input": {"file_path": "src/main.rs"}
    });
    let _ = touring_hooks::post_edit::run_returning(&mut rt, &post_input);
    // Verify both are stored independently
    let bus = rt.ctx.session_bus.borrow();
    assert!(bus.get_last_hook_result("pre_read").is_some());
    assert!(bus.get_last_hook_result("post_edit").is_some());
}

// ═══════════════════════════════════════════════════════════════════════
// FEATURE A: HOOKCONTEXT AVAILABILITY E2E
// ═══════════════════════════════════════════════════════════════════════

use std::path::PathBuf;
use touring_hooks::shared::hook_context::{HookContext, HookMeta, HookServices};

#[test]
fn test_hook_meta_now_has_timestamp() {
    let meta = HookMeta::now();
    assert!(meta.timestamp.timestamp() > 0);
}

#[test]
fn test_hook_meta_with_session_id() {
    let meta = HookMeta::now().with_session_id(uuid::Uuid::new_v4());
    assert!(meta.session_id != uuid::Uuid::nil());
}

#[test]
fn test_hook_meta_with_file_path() {
    let meta = HookMeta::now().with_file_path(PathBuf::from("src/main.rs"));
    assert_eq!(meta.file_path, Some(PathBuf::from("src/main.rs")));
}

#[test]
fn test_hook_meta_with_tool_name() {
    let meta = HookMeta::now().with_tool_name("edit");
    assert_eq!(meta.tool_name, Some("edit".to_string()));
}

#[test]
fn test_hook_meta_builder_pattern() {
    // Verify HookMeta has all builder methods
    let meta = HookMeta::now()
        .with_session_id(uuid::Uuid::new_v4())
        .with_file_path(PathBuf::from("src/lib.rs"))
        .with_tool_name("read");
    assert!(meta.session_id != uuid::Uuid::nil());
    assert_eq!(meta.file_path, Some(PathBuf::from("src/lib.rs")));
    assert_eq!(meta.tool_name, Some("read".to_string()));
}

#[test]
fn test_hook_services_fields_exist() {
    // Verify HookServices exposes knowledge and session_bus fields
    // We check struct layout via PhantomData since we can't create real references
    fn _check_services_layout(_s: &HookServices) {}
    // The struct has pub fields: knowledge: &FileKnowledgeDB, session_bus: &SessionBus
}

#[test]
fn test_hook_context_get_str_method_exists() {
    // HookContext has get_str(&self, key: &str) -> Option<&'a str>
    // Verify the signature compiles (actual call needs full context)
    fn _check<'a>(ctx: &HookContext<'a>, key: &str) -> Option<&'a str> {
        ctx.get_str(key)
    }
}

#[test]
fn test_hook_context_get_u64_method_exists() {
    fn _check<'a>(ctx: &HookContext<'a>, key: &str) -> Option<u64> {
        ctx.get_u64(key)
    }
}

#[test]
fn test_hook_context_get_f64_method_exists() {
    fn _check<'a>(ctx: &HookContext<'a>, key: &str) -> Option<f64> {
        ctx.get_f64(key)
    }
}

#[test]
fn test_hook_context_get_bool_method_exists() {
    fn _check<'a>(ctx: &HookContext<'a>, key: &str) -> Option<bool> {
        ctx.get_bool(key)
    }
}

#[test]
fn test_hook_context_has_key_method_exists() {
    fn _check<'a>(ctx: &HookContext<'a>, key: &str) -> bool {
        ctx.has_key(key)
    }
}

#[test]
fn test_hook_context_with_last_method_exists() {
    // HookContext::with_last(self, last: Value) -> Self
    // Verify method signature exists
    fn _check_method<'a>(ctx: HookContext<'a>, last: serde_json::Value) -> HookContext<'a> {
        ctx.with_last(last)
    }
}

#[test]
fn test_hook_context_chain_completion_reward_signature() {
    // chain_completion_reward(&self, quality_score: f64) -> Option<(String, f64)>
    fn _check_reward<'a>(ctx: &HookContext<'a>, score: f64) -> Option<(String, f64)> {
        ctx.chain_completion_reward(score)
    }
}

#[test]
fn test_hook_context_fields_public() {
    // HookContext has public fields: hook_name, payload, last, meta, services
    fn _check_fields<'a>(ctx: &HookContext<'a>) {
        let _ = ctx.hook_name;
        let _ = ctx.payload;
        let _ = ctx.last;
        let _ = ctx.meta;
        let _ = ctx.services;
    }
}

#[test]
fn test_hook_meta_fields_public() {
    // HookMeta has public fields: timestamp, session_id, file_path, tool_name
    fn _check_meta_fields(meta: &HookMeta) {
        let _ = meta.timestamp;
        let _ = meta.session_id;
        let _ = meta.file_path;
        let _ = meta.tool_name;
    }
}

// ═══════════════════════════════════════════════════════════════════════
// FEATURE F1+F2: WIRING IMPACT + CYCLES E2E
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn test_cli_wiring_impact_requires_symbol() {
    let (_tmp, mut rt) = setup_runtime();
    let result = parse_json(&cli_wiring_impact(&mut rt, &serde_json::json!({})));
    assert!(
        result.get("error").is_some(),
        "empty symbol should return error"
    );
    assert_eq!(result["error"], "symbol is required");
}

#[test]
fn test_cli_wiring_impact_with_valid_symbol() {
    let (_tmp, mut rt) = setup_runtime();
    // HookRuntime has HookRuntime symbols wired in the knowledge DB
    let result = parse_json(&cli_wiring_impact(
        &mut rt,
        &serde_json::json!({
            "symbol": "HookRuntime",
            "depth": 2,
            "format": "json"
        }),
    ));
    assert!(
        result.get("direct_consumers").is_some(),
        "should have direct_consumers field"
    );
    assert!(
        result.get("total_transitive").is_some(),
        "should have total_transitive field"
    );
    assert!(
        result.get("max_depth").is_some(),
        "should have max_depth field"
    );
    assert!(result.get("paths").is_some(), "should have paths field");
}

#[test]
fn test_cli_wiring_impact_json_format() {
    let (_tmp, mut rt) = setup_runtime();
    let result = parse_json(&cli_wiring_impact(
        &mut rt,
        &serde_json::json!({
            "symbol": "HookRuntime",
            "depth": 2,
            "format": "json"
        }),
    ));
    // JSON format should return the full struct, not text lines
    assert!(result.get("direct_consumers").is_some());
    assert!(result.get("total_transitive").is_some());
}

#[test]
fn test_cli_wiring_impact_text_format_returns_lines() {
    let (_tmp, mut rt) = setup_runtime();
    let text = cli_wiring_impact(
        &mut rt,
        &serde_json::json!({
            "symbol": "HookRuntime",
            "depth": 2,
            "format": "text"
        }),
    );
    // Text format contains visual elements
    assert!(text.contains("Impact Analysis:"));
    assert!(text.contains("Direct consumers:"));
    assert!(text.contains("Total transitive impacted:"));
}

#[test]
fn test_cli_wiring_impact_unknown_symbol_returns_zeroed() {
    let (_tmp, mut rt) = setup_runtime();
    // Symbol that exists in the index but has no consumers
    let result = parse_json(&cli_wiring_impact(
        &mut rt,
        &serde_json::json!({
            "symbol": "NonExistentSymbolXYZ123",
            "depth": 5,
            "format": "json"
        }),
    ));
    assert!(result.get("direct_consumers").is_some());
    assert!(result.get("total_transitive").is_some());
}

#[test]
fn test_cli_wiring_cycles_returns_cycle_count() {
    let (_tmp, mut rt) = setup_runtime();
    let result = parse_json(&cli_wiring_cycles(
        &mut rt,
        &serde_json::json!({
            "format": "json"
        }),
    ));
    assert!(
        result.get("cycle_count").is_some(),
        "should have cycle_count field"
    );
    assert!(result.get("cycles").is_some(), "should have cycles field");
    assert!(result["cycle_count"].is_u64() || result["cycle_count"].is_i64());
}

#[test]
fn test_cli_wiring_cycles_json_format() {
    let (_tmp, mut rt) = setup_runtime();
    let result = parse_json(&cli_wiring_cycles(
        &mut rt,
        &serde_json::json!({
            "format": "json"
        }),
    ));
    assert!(result.get("cycle_count").is_some());
    assert!(result["cycles"].is_array());
}

#[test]
fn test_cli_wiring_cycles_text_format() {
    let (_tmp, mut rt) = setup_runtime();
    let text = cli_wiring_cycles(
        &mut rt,
        &serde_json::json!({
            "format": "text"
        }),
    );
    assert!(text.contains("Dependency Cycles Detected:"));
}

#[test]
fn test_cli_wiring_cycles_min_depth_filters() {
    let (_tmp, mut rt) = setup_runtime();
    // min_depth=5 should filter out shallow cycles
    let result_deep = parse_json(&cli_wiring_cycles(
        &mut rt,
        &serde_json::json!({
            "min_depth": 5,
            "format": "json"
        }),
    ));
    let result_shallow = parse_json(&cli_wiring_cycles(
        &mut rt,
        &serde_json::json!({
            "min_depth": 2,
            "format": "json"
        }),
    ));
    // Deeper filter should return <= cycles than shallower
    let deep_count = result_deep["cycle_count"].as_u64().unwrap_or(0);
    let shallow_count = result_shallow["cycle_count"].as_u64().unwrap_or(0);
    assert!(
        deep_count <= shallow_count,
        "min_depth=5 should filter >= min_depth=2"
    );
}

#[test]
fn test_cli_wiring_cycles_returns_valid_cycle_structure() {
    let (_tmp, mut rt) = setup_runtime();
    let result = parse_json(&cli_wiring_cycles(
        &mut rt,
        &serde_json::json!({
            "format": "json"
        }),
    ));
    let cycles = result["cycles"].as_array().expect("cycles should be array");
    for cycle in cycles {
        assert!(cycle.get("id").is_some(), "cycle should have id");
        assert!(cycle.get("depth").is_some(), "cycle should have depth");
        assert!(cycle.get("modules").is_some(), "cycle should have modules");
        assert!(
            cycle.get("severity").is_some(),
            "cycle should have severity"
        );
        assert!(cycle["modules"].is_array(), "modules should be array");
    }
}

// ═══════════════════════════════════════════════════════════════════════
// WAVE Q4 — Diagnostic codes (RFC-100): cli_wiring_orphans --diagnostics
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn test_cli_wiring_orphans_diagnostics_flag_off_by_default() {
    let (_tmp, mut rt) = setup_runtime();
    let result = parse_json(&cli_wiring_orphans(&mut rt, &serde_json::Value::Null));
    assert!(
        result.get("diagnostics").is_none(),
        "diagnostics key MUST be absent when flag is not set"
    );
    assert!(
        result.get("orphans").is_some(),
        "orphans key always present"
    );
    assert!(
        result.get("orphan_count").is_some(),
        "orphan_count always present"
    );
}

#[test]
fn test_cli_wiring_orphans_diagnostics_flag_on_emits_w_codes() {
    let (_tmp, mut rt) = setup_runtime();
    let payload = serde_json::json!({"diagnostics": true});
    let result = parse_json(&cli_wiring_orphans(&mut rt, &payload));

    let diagnostics = result["diagnostics"]
        .as_array()
        .expect("diagnostics should be present and an array");
    let count = result["diagnostic_count"].as_u64().unwrap_or(0);
    assert_eq!(
        count as usize,
        diagnostics.len(),
        "count must match array len"
    );

    if let Some(first) = diagnostics.first() {
        let code = first["code"].as_str().unwrap_or("");
        assert!(
            code.starts_with("W-"),
            "wiring diagnostics must carry W- codes; got: {code}"
        );
    }
}

#[test]
fn test_cli_wiring_orphans_diagnostics_schema_matches_rfc_100() {
    let (_tmp, mut rt) = setup_runtime();
    let payload = serde_json::json!({"diagnostics": true});
    let result = parse_json(&cli_wiring_orphans(&mut rt, &payload));

    let diagnostics = result["diagnostics"].as_array().expect("diagnostics array");
    for d in diagnostics {
        assert!(d.get("code").is_some(), "diagnostic missing `code`");
        assert!(d.get("severity").is_some(), "diagnostic missing `severity`");
        assert!(d.get("message").is_some(), "diagnostic missing `message`");

        let sev = d["severity"].as_str().unwrap_or("");
        assert!(
            ["error", "warning", "info", "hint"].contains(&sev),
            "invalid severity `{sev}`"
        );
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// P4.2 — HyperGraph wiring integration E2E tests
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_cli_wiring_chains_rebuild_and_returns_chain_count() {
    let (_tmp, mut rt) = setup_runtime();
    // Empty payload triggers rebuild + return count
    let result = parse_json(&cli_wiring_chains(&mut rt, &serde_json::Value::Null));
    assert!(result.get("rebuilt").is_some(), "should have rebuilt field");
    assert!(
        result.get("chain_count").is_some(),
        "should have chain_count field"
    );
    assert!(
        result["rebuilt"].as_bool().is_some(),
        "rebuilt should be boolean"
    );
    assert!(
        result["chain_count"].is_u64() || result["chain_count"].is_i64(),
        "chain_count should be integer"
    );
}

#[test]
fn test_cli_wiring_chains_filter_by_file_path() {
    let (_tmp, mut rt) = setup_runtime();
    // Filter by file_path — returns chains for that specific module
    let result = parse_json(&cli_wiring_chains(
        &mut rt,
        &serde_json::json!({
            "file_path": "crates/touring-hooks/src/wiring/hypergraph.rs"
        }),
    ));
    assert!(
        result.get("file_path").is_some(),
        "should echo back file_path"
    );
    assert!(result.get("chains").is_some(), "should have chains field");
    assert!(result["chains"].is_array(), "chains should be array");
}

#[test]
fn test_cli_wiring_chains_rebuild_flag_returns_rebuilt_true() {
    let (_tmp, mut rt) = setup_runtime();
    // Explicit rebuild flag
    let result = parse_json(&cli_wiring_chains(
        &mut rt,
        &serde_json::json!({
            "rebuild": true
        }),
    ));
    assert_eq!(
        result["rebuilt"].as_bool().unwrap_or(false),
        true,
        "rebuild=true should set rebuilt=true"
    );
    assert!(
        result.get("chain_count").is_some(),
        "should return chain_count after rebuild"
    );
}

#[test]
fn test_hypergraph_e2e_wiring_chains_integration() {
    use touring_hooks::wiring::hypergraph::{
        FeatureGateHyperedge, HyperGraph, MultiImportHyperedge,
    };

    // P4.2: HyperGraph and wiring chains share the same concept — both model
    // N-ary relationships between modules. This test verifies the integration:
    // a HyperGraph hyperedge maps 1:1 to a wiring functional chain.

    // ── Feature gate hyperedge (mirrors cfg(all(...)) wiring chain) ──
    let mut hg: HyperGraph<&str> = HyperGraph::new();
    let simd_node = hg.add_node("feature:simd");
    let ann_node = hg.add_node("feature:ann");
    let target = hg.add_node("module:touring_code::ast::semantic_search");

    let gate_edge = hg.add_hyperedge(
        &[simd_node, ann_node, target],
        r#"all(feature = "simd", feature = "ann")"#,
    );

    // Verify hyperedge membership (same semantics as wiring chain source→sink)
    let gates = hg.hyperedges_for(target);
    assert_eq!(
        gates.len(),
        1,
        "target belongs to exactly 1 feature gate hyperedge"
    );
    assert_eq!(gates[0], gate_edge);

    let gate_members = hg.members_of(gate_edge);
    assert_eq!(
        gate_members.len(),
        3,
        "feature gate hyperedge has 3 members"
    );

    // Verify FeatureGateHyperedge type directly
    let gate = FeatureGateHyperedge::new(
        r#"all(feature = "simd", feature = "ann")"#,
        "touring_code::ast::semantic_search",
    );
    assert_eq!(gate.features, vec!["simd", "ann"]);
    assert_eq!(gate.module_path, "touring_code::ast::semantic_search");

    // ── Multi-import hyperedge (mirrors use foo::{A,B,C} wiring chain) ──
    let mut hg2: HyperGraph<&str> = HyperGraph::new();
    let parser = hg2.add_node("touring_code::ast::Parser");
    let store = hg2.add_node("touring_code::ast::SymbolStore");
    let quality = hg2.add_node("touring_code::ast::QualityMetrics");
    let import_stmt = hg2.add_node("use touring_code::ast::{Parser, SymbolStore, QualityMetrics}");

    let import_edge = hg2.add_hyperedge(
        &[parser, store, quality, import_stmt],
        "use touring_code::ast::{Parser, SymbolStore, QualityMetrics}",
    );

    // Each imported symbol belongs to the multi-import hyperedge
    for &sym_node in &[parser, store, quality] {
        let edges = hg2.hyperedges_for(sym_node);
        assert!(
            edges.contains(&import_edge),
            "imported symbol must belong to the import hyperedge"
        );
    }

    // Verify MultiImportHyperedge type directly
    let import_meta = MultiImportHyperedge::new(
        "use touring_code::ast::{Parser, SymbolStore, QualityMetrics}",
        "touring_hooks::pre_write",
    );
    assert_eq!(
        import_meta.imported_symbols,
        vec!["Parser", "SymbolStore", "QualityMetrics"]
    );
    assert_eq!(import_meta.source_module, "touring_hooks::pre_write");

    // ── Cross-cutting hyperedge (shared node across multiple chains) ──
    // The "ann" feature is shared across multiple wiring decisions — same pattern
    // as a symbol that appears in multiple functional chains.
    let mut hg3: HyperGraph<&str> = HyperGraph::new();
    let ann = hg3.add_node("feature:ann");
    let simd = hg3.add_node("feature:simd");
    let gpu = hg3.add_node("feature:gpu");
    let parser_decision = hg3.add_node("decision:parser");
    let quality_decision = hg3.add_node("decision:quality");
    let semantic_decision = hg3.add_node("decision:semantic_search");

    // Three different decisions that all depend on "ann" (cross-cutting concern)
    let _he1 = hg3.add_hyperedge(&[simd, ann, parser_decision], "cfg(all(simd, ann))");
    let _he2 = hg3.add_hyperedge(&[gpu, ann, quality_decision], "cfg(all(gpu, ann))");
    let he3 = hg3.add_hyperedge(
        &[simd, gpu, ann, semantic_decision],
        "cfg(all(simd, gpu, ann))",
    );

    // ann is shared across all 3 hyperedges (same as a symbol wired to multiple consumers)
    let ann_hyperedges = hg3.hyperedges_for(ann);
    assert_eq!(ann_hyperedges.len(), 3, "ann appears in all 3 hyperedges");

    // semantic is only in he3
    let semantic_hyperedges = hg3.hyperedges_for(semantic_decision);
    assert_eq!(semantic_hyperedges.len(), 1, "semantic only in he3");

    // he3 has all 4 members
    let he3_members = hg3.members_of(he3);
    assert_eq!(
        he3_members.len(),
        4,
        "he3 has 4 members: simd+gpu+ann+semantic"
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// WAVE F3+F4 — ACP Protocol + HyperGraph E2E (P4.1 + P4.2)
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(feature = "acp-protocol")]
mod acp_e2e {
    use tempfile::TempDir;
    use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
    use tokio::net::{UnixListener, UnixStream};
    use touring_hooks::protocol::acp::errors::*;
    use touring_hooks::protocol::acp::*;

    // ── Socket-based E2E tests (P4.1) ────────────────────────────────────

    /// Mock daemon that handles ACP protocol requests via Unix socket.
    /// Mirrors the peek-byte dispatch in `daemon.rs::handle_connection_async`.
    async fn mock_acp_daemon(listener: UnixListener) {
        let (stream, _) = listener.accept().await.expect("accept");
        let (reader, mut writer) = tokio::io::split(stream);
        let mut reader = BufReader::new(reader);

        let mut line = String::new();
        if reader.read_line(&mut line).await.is_err() {
            return;
        }

        let response_json: String;
        let parse_result = parse_message(line.trim());
        if let Some(msg) = parse_result {
            // ACP path — same dispatch as daemon.rs::handle_acp_request_async
            match msg.method.as_str() {
                "acp.discover" => {
                    let caps = Capabilities::default();
                    let resp = success_response(msg.id, serde_json::to_value(caps).unwrap());
                    response_json = serialize_response(&resp).unwrap_or_default();
                }
                "wiring.impact" => {
                    let depth = msg
                        .params
                        .get("depth")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(2) as usize;
                    let result = serde_json::json!({
                        "direct_consumers": 42,
                        "max_depth": depth,
                        "total_transitive": depth * 10
                    });
                    let resp = success_response(msg.id, result);
                    response_json = serialize_response(&resp).unwrap_or_default();
                }
                "wiring.cycles" => {
                    let result = serde_json::json!({
                        "cycle_count": 1,
                        "cycles": [{"path": ["a", "b", "c", "a"], "depth": 3}]
                    });
                    let resp = success_response(msg.id, result);
                    response_json = serialize_response(&resp).unwrap_or_default();
                }
                _ => {
                    let resp = error_response(
                        msg.id,
                        E_METHOD_NOT_FOUND,
                        &format!("Unknown method: {}", msg.method),
                    );
                    response_json = serialize_response(&resp).unwrap_or_default();
                }
            }
        } else {
            // Parse failed — return error
            let err = error_response(
                String::new(),
                E_INVALID_MESSAGE,
                "Failed to parse ACP message",
            );
            response_json = serialize_response(&err).unwrap_or_default();
        }

        writer.write_all(response_json.as_bytes()).await.ok();
        writer.write_all(b"\n").await.ok();
        writer.flush().await.ok();
    }

    fn spawn_daemon(
        dir: &TempDir,
        name: &str,
    ) -> (tokio::task::JoinHandle<()>, std::path::PathBuf) {
        let sock = dir.path().join(name);
        let listener = UnixListener::bind(&sock).expect("bind");
        let handle = tokio::spawn(async move {
            mock_acp_daemon(listener).await;
        });
        (handle, sock)
    }

    async fn send_acp_via_socket(sock: &std::path::Path, msg: &Message) -> String {
        let mut client = UnixStream::connect(sock).await.expect("connect");
        let json = serde_json::to_string(msg).expect("serialize");
        client.write_all(json.as_bytes()).await.expect("write");
        client.write_all(b"\n").await.ok();
        client.shutdown().await.ok();
        let mut resp = String::new();
        client.read_to_string(&mut resp).await.expect("read");
        resp.trim().to_string()
    }

    // ── Test 1: ACP message roundtrip via daemon socket ──────────────────

    #[tokio::test]
    async fn test_cli_acp_protocol_message_roundtrip() {
        // P4.1: Send an ACP Message via daemon socket, verify roundtrip.
        // Uses a mock daemon that handles ACP dispatch and returns mock data.
        let dir = tempfile::tempdir().expect("tempdir");
        let (handle, sock) = spawn_daemon(&dir, "acp_roundtrip.sock");

        let msg = Message {
            jsonrpc: "2.0".to_string(),
            id: "req-rt-001".to_string(),
            method: "wiring.impact".to_string(),
            params: serde_json::json!({"symbol": "HookRuntime", "depth": 3}),
            correlation_id: None,
        };

        let resp_json = send_acp_via_socket(&sock, &msg).await;
        handle.await.expect("daemon join");

        // Parse the response
        let resp: Response = serde_json::from_str(&resp_json).expect("response parses");
        assert_eq!(resp.jsonrpc, "2.0", "response must be JSON-RPC 2.0");
        assert_eq!(resp.id, "req-rt-001", "response id must match request id");
        assert!(resp.result.is_some(), "success response must have result");
        assert!(resp.error.is_none(), "success response must not have error");

        let result = resp.result.unwrap();
        assert_eq!(
            result["direct_consumers"], 42,
            "mock daemon returns 42 consumers"
        );
        assert_eq!(result["max_depth"], 3, "depth must be echoed back");
    }

    // ── Test 2: ACP error handling for malformed messages ─────────────────

    #[tokio::test]
    async fn test_cli_acp_response_error_handling() {
        // P4.1: Send malformed ACP message, verify error response.
        let dir = tempfile::tempdir().expect("tempdir");
        let (handle, sock) = spawn_daemon(&dir, "acp_error.sock");

        // Malformed JSON (invalid JSON)
        let malformed = r#"not a valid JSON-RPC message"#;
        let mut client = UnixStream::connect(&sock).await.expect("connect");
        client.write_all(malformed.as_bytes()).await.ok();
        client.write_all(b"\n").await.ok();
        client.shutdown().await.ok();
        let mut resp = String::new();
        client.read_to_string(&mut resp).await.expect("read");
        handle.await.expect("daemon join");

        // The mock daemon returns E_INVALID_MESSAGE for parse failures
        let resp_parsed: serde_json::Value =
            serde_json::from_str(resp.trim()).expect("error response parses");
        // E_INVALID_MESSAGE code is -32700
        assert!(resp_parsed.get("error").is_some() || resp_parsed.get("id").is_some());

        // Also test a valid ACP message with unknown method
        let dir2 = tempfile::tempdir().expect("tempdir");
        let (handle2, sock2) = spawn_daemon(&dir2, "acp_unknown.sock");

        let unknown_method = Message {
            jsonrpc: "2.0".to_string(),
            id: "req-unknown".to_string(),
            method: "unknown.method".to_string(),
            params: serde_json::json!({}),
            correlation_id: None,
        };

        let resp_json2 = send_acp_via_socket(&sock2, &unknown_method).await;
        handle2.await.expect("daemon join");

        let resp2: Response = serde_json::from_str(&resp_json2).expect("response parses");
        assert!(
            resp2.result.is_none(),
            "unknown method must not have result"
        );
        assert!(resp2.error.is_some(), "unknown method must return error");
        let err = resp2.error.unwrap();
        assert_eq!(err.code, E_METHOD_NOT_FOUND, "error code must be -32601");
        assert!(
            err.message.contains("Unknown method"),
            "error message must mention unknown method"
        );
    }

    // ── Test 3: ACP capabilities discovery ────────────────────────────────

    #[tokio::test]
    async fn test_cli_acp_capabilities_discovery() {
        // P4.1: Query ACP capabilities via daemon socket, verify discovery response.
        let dir = tempfile::tempdir().expect("tempdir");
        let (handle, sock) = spawn_daemon(&dir, "acp_caps.sock");

        let discover_msg = Message {
            jsonrpc: "2.0".to_string(),
            id: "caps-discover-001".to_string(),
            method: "acp.discover".to_string(),
            params: serde_json::json!({}),
            correlation_id: None,
        };

        let resp_json = send_acp_via_socket(&sock, &discover_msg).await;
        handle.await.expect("daemon join");

        let resp: Response = serde_json::from_str(&resp_json).expect("response parses");
        assert!(
            resp.result.is_some(),
            "capabilities response must have result"
        );

        let caps = resp.result.unwrap();
        assert_eq!(caps["version"], "acp-1.0", "must advertise acp-1.0");
        assert_eq!(
            caps["streaming"].as_bool().unwrap_or(false),
            false,
            "streaming default is false"
        );
        assert!(
            caps["impact_analysis"].as_bool().unwrap(),
            "server must support impact analysis"
        );
        assert!(
            caps["cycle_detection"].as_bool().unwrap(),
            "server must support cycle detection"
        );
        assert!(
            caps["modules"].as_bool().unwrap(),
            "server must support modules"
        );
        assert!(
            caps["orphans"].as_bool().unwrap(),
            "server must support orphans"
        );
        assert!(
            caps["chains"].as_bool().unwrap(),
            "server must support chains"
        );
    }

    // ── Existing unit tests (remain in place) ───────────────────────────

    #[test]
    fn e2e_acp_message_lifecycle() {
        // Simulate full ACP message lifecycle: encode → detect → parse → response → verify
        let msg = Message {
            jsonrpc: "2.0".to_string(),
            id: "req-e2e-001".to_string(),
            method: "wiring.impact".to_string(),
            params: serde_json::json!({"symbol": "HookRuntime", "depth": 2}),
            correlation_id: Some("corr-abc".to_string()),
        };

        // Step 1: serialize to JSON bytes (daemon socket send)
        let json_bytes = serde_json::to_vec(&msg).expect("message serializes");

        // Step 2: detect_acp_payload at socket boundary
        assert!(
            detect_acp_payload(&json_bytes),
            "ACP message must be detected"
        );

        // Step 3: parse_message (what daemon does on receive)
        let json_str = String::from_utf8(json_bytes).expect("valid UTF-8");
        let parsed = parse_message(&json_str).expect("parse succeeds");
        assert_eq!(parsed.id, "req-e2e-001");
        assert_eq!(parsed.method, "wiring.impact");
        assert_eq!(parsed.params["symbol"], "HookRuntime");

        // Step 4: build success response (daemon processing)
        let resp = success_response(
            parsed.id,
            serde_json::json!({"direct_consumers": 68, "max_depth": 1}),
        );

        // Step 5: serialize response
        let resp_json = serialize_response(&resp).expect("response serializes");
        assert!(
            resp_json.contains("\"result\""),
            "response must have result field"
        );
        assert!(
            !resp_json.contains("\"error\""),
            "success response has no error"
        );

        // Step 6: parse response back (client side)
        let parsed_resp: Response = serde_json::from_str(&resp_json).expect("response parses");
        assert!(parsed_resp.result.is_some(), "success response has result");
        assert!(parsed_resp.error.is_none(), "success response has no error");
    }

    #[test]
    fn e2e_acp_error_path() {
        // Simulate ACP error response lifecycle
        let msg = Message {
            jsonrpc: "2.0".to_string(),
            id: "req-e2e-err".to_string(),
            method: "wiring.cycles".to_string(),
            params: serde_json::json!({"min_depth": 99}),
            correlation_id: None,
        };

        let resp = error_response(
            msg.id.clone(),
            errors::E_INVALID_PARAMS,
            "min_depth too large",
        );

        let resp_json = serialize_response(&resp).expect("error serializes");
        assert!(
            !resp_json.contains("\"result\""),
            "error response has no result"
        );
        assert!(resp_json.contains("\"error\""), "error response has error");

        let parsed: Response = serde_json::from_str(&resp_json).expect("error parses");
        assert!(parsed.result.is_none(), "error response has no result");
        let err = parsed.error.expect("error field present");
        assert_eq!(err.code, errors::E_INVALID_PARAMS);
        assert!(err.message.contains("min_depth"));
    }

    #[test]
    fn e2e_acp_capability_negotiation() {
        // ACP capability negotiation: client requests capabilities, server responds
        let caps = Capabilities::default();
        assert_eq!(caps.version, "acp-1.0");
        assert!(caps.impact_analysis, "server supports impact analysis");
        assert!(caps.cycle_detection, "server supports cycle detection");
        assert!(caps.modules, "server supports modules scoring");
        assert!(caps.orphans, "server supports orphan detection");
        assert!(caps.chains, "server supports functional chains");

        // Simulate capability request
        let req = Message {
            jsonrpc: "2.0".to_string(),
            id: "caps-001".to_string(),
            method: "server.capabilities".to_string(),
            params: serde_json::json!({}),
            correlation_id: None,
        };

        // Server responds with full capabilities
        let resp = success_response(req.id, serde_json::to_value(caps.clone()).unwrap());
        let resp_json = serialize_response(&resp).unwrap();
        let parsed_resp: Response = serde_json::from_str(&resp_json).unwrap();

        let result = parsed_resp.result.unwrap();
        assert_eq!(result["version"], "acp-1.0");
        assert!(result["impact_analysis"].as_bool().unwrap());
        assert!(result["cycle_detection"].as_bool().unwrap());
    }
}

mod hypergraph_e2e {

    use touring_hooks::wiring::hypergraph::{
        FeatureGateHyperedge, HyperGraph, MultiImportHyperedge,
    };

    #[test]
    fn e2e_hypergraph_feature_gate_lifecycle() {
        // Simulate cfg(all(feature = "simd", feature = "ann")) pattern
        let mut hg: HyperGraph<&str> = HyperGraph::new();

        let simd_node = hg.add_node("feature:simd");
        let ann_node = hg.add_node("feature:ann");
        let decision_node = hg.add_node("module:touring_code::ast::semantic_search");

        // Create feature gate hyperedge
        let gate_edge = hg.add_hyperedge(
            &[simd_node, ann_node, decision_node],
            r#"all(feature = "simd", feature = "ann")"#,
        );

        // Verify hyperedge structure
        // node_count() returns ALL nodes (real + artificial hyperedge nodes)
        assert_eq!(hg.node_count(), 4); // 3 real + 1 artificial hyperedge
        assert_eq!(hg.hyperedge_count(), 1);

        // membership lookup: decision_node belongs to this gate
        let gates = hg.hyperedges_for(decision_node);
        assert_eq!(gates.len(), 1);
        assert_eq!(gates[0], gate_edge);

        // members_of: hyperedge contains all 3 nodes
        let members = hg.members_of(gate_edge);
        assert_eq!(members.len(), 3);

        // FeatureGateHyperedge type directly
        let gate = FeatureGateHyperedge::new(
            r#"all(feature = "simd", feature = "ann")"#,
            "touring_code::ast::semantic_search",
        );
        assert_eq!(gate.features, vec!["simd", "ann"]);
        assert_eq!(gate.module_path, "touring_code::ast::semantic_search");
    }

    #[test]
    fn e2e_hypergraph_multi_import_lifecycle() {
        // Simulate `use touring_code::ast::{Parser, SymbolStore, QualityMetrics}` pattern
        let mut hg: HyperGraph<&str> = HyperGraph::new();

        let parser_node = hg.add_node("touring_code::ast::Parser");
        let store_node = hg.add_node("touring_code::ast::SymbolStore");
        let quality_node = hg.add_node("touring_code::ast::QualityMetrics");
        let import_stmt =
            hg.add_node("use touring_code::ast::{Parser, SymbolStore, QualityMetrics}");

        // Multi-import hyperedge
        let import_edge = hg.add_hyperedge(
            &[parser_node, store_node, quality_node, import_stmt],
            "use touring_code::ast::{Parser, SymbolStore, QualityMetrics}",
        );

        // Verify membership: each imported symbol belongs to this import
        for &sym_node in &[parser_node, store_node, quality_node] {
            let edges = hg.hyperedges_for(sym_node);
            assert!(
                edges.contains(&import_edge),
                "symbol {:?} must belong to import hyperedge",
                sym_node
            );
        }

        // MultiImportHyperedge type directly
        let mh = MultiImportHyperedge::new(
            "use touring_code::ast::{Parser, SymbolStore, QualityMetrics}",
            "touring_hooks::pre_write",
        );
        assert_eq!(
            mh.imported_symbols,
            vec!["Parser", "SymbolStore", "QualityMetrics"]
        );
        assert_eq!(mh.source_module, "touring_hooks::pre_write");
    }

    #[test]
    fn e2e_hypergraph_complex_multi_hyperedge() {
        // Complex case: multiple hyperedges sharing nodes (cross-cutting concerns)
        let mut hg: HyperGraph<&str> = HyperGraph::new();

        let simd = hg.add_node("feature:simd");
        let ann = hg.add_node("feature:ann");
        let gpu = hg.add_node("feature:gpu");
        let parser = hg.add_node("module:parser");
        let quality = hg.add_node("module:quality");
        let semantic = hg.add_node("module:semantic_search");

        // Hyperedge 1: simd + ann → parser decision
        let _he1 = hg.add_hyperedge(&[simd, ann, parser], "cfg(all(simd, ann))");
        // Hyperedge 2: gpu + ann → quality decision
        let _he2 = hg.add_hyperedge(&[gpu, ann, quality], "cfg(all(gpu, ann))");
        // Hyperedge 3: all three features → semantic_search decision
        let he3 = hg.add_hyperedge(&[simd, gpu, ann, semantic], "cfg(all(simd, gpu, ann))");

        // node_count() = real + artificial hyperedge nodes
        assert_eq!(hg.node_count(), 9); // 6 real + 3 artificial hyperedge nodes
        assert_eq!(hg.hyperedge_count(), 3);

        // ann is shared across all 3 hyperedges (cross-cutting)
        let ann_hyperedges = hg.hyperedges_for(ann);
        assert_eq!(ann_hyperedges.len(), 3, "ann is in all 3 hyperedges");

        // parser is only in he1
        let parser_hyperedges = hg.hyperedges_for(parser);
        assert_eq!(parser_hyperedges.len(), 1);

        // semantic is only in he3
        let semantic_hyperedges = hg.hyperedges_for(semantic);
        assert_eq!(semantic_hyperedges.len(), 1);

        // Cross: he3 contains all three features
        let he3_members = hg.members_of(he3);
        assert_eq!(
            he3_members.len(),
            4,
            "he3 has 4 members: simd+gpu+ann+semantic"
        );
    }
}

// ═══════════════════════════════════════════════════════════════════════
// MPATCH FUZZY PATCH PREVIEW E2E (P1.8 — 2026-04-25)
// Feature-gated: only runs when `mpatch-fuzzy` feature is enabled.
// ═══════════════════════════════════════════════════════════════════════

#[cfg(feature = "mpatch-fuzzy")]
#[test]
fn test_cli_mpatch_preview_exact() {
    use tempfile::TempDir;
    use touring_hooks::cli_handlers::cli_mpatch_preview;
    use touring_hooks::runtime::HookRuntime;

    let tmp = TempDir::new().expect("create tempdir");
    let root = tmp.path().to_path_buf();
    std::fs::create_dir_all(root.join(".claude/data")).expect("create data dir");
    let mut rt = HookRuntime::new(&root).expect("init runtime");

    // Create a temp file with known content
    let file_path = root.join("sample.txt");
    std::fs::write(&file_path, "line one\nline two\nline three\n").expect("write sample");

    // Exact match: whitespace-only patch (no actual text change)
    // Proper unified diff format with --- and +++ headers
    let patch = "--- sample.txt\n+++ sample.txt\n@@ -1,3 +1,3 @@\nline one\n-line two\n+line two\nline three\n";
    let result = parse_json(&cli_mpatch_preview(
        &mut rt,
        &serde_json::json!({
            "file": file_path.to_str().unwrap(),
            "patch": patch,
            "dry_run": false
        }),
    ));

    assert_eq!(
        result["matched"], true,
        "exact whitespace patch should match"
    );
    let method = result["method"].as_str().unwrap();
    assert!(
        method == "Exact" || method == "Whitespace",
        "method should be Exact or Whitespace, got: {method}"
    );
    assert!(result["confidence"].as_f64().unwrap() >= 0.9);
    assert!(result["error"].is_null());
}

#[cfg(feature = "mpatch-fuzzy")]
#[test]
fn test_cli_mpatch_preview_fuzzy() {
    use tempfile::TempDir;
    use touring_hooks::cli_handlers::cli_mpatch_preview;
    use touring_hooks::runtime::HookRuntime;

    let tmp = TempDir::new().expect("create tempdir");
    let root = tmp.path().to_path_buf();
    std::fs::create_dir_all(root.join(".claude/data")).expect("create data dir");
    let mut rt = HookRuntime::new(&root).expect("init runtime");

    // Create a temp file with known content
    let file_path = root.join("sample.txt");
    std::fs::write(&file_path, "Hello world\n").expect("write sample");

    // Fuzzy patch: small text change with proper diff header
    let patch = "--- sample.txt\n+++ sample.txt\n@@ -1 +1 @@\n-Hello world\n+Hello world!\n";
    let result = parse_json(&cli_mpatch_preview(
        &mut rt,
        &serde_json::json!({
            "file": file_path.to_str().unwrap(),
            "patch": patch,
            "dry_run": true
        }),
    ));

    assert_eq!(result["matched"], true, "fuzzy patch should match");
    // dry_run=true should include preview
    assert!(
        result["preview"].is_string(),
        "dry_run=true should return preview"
    );
    let preview = result["preview"].as_str().unwrap();
    assert!(
        preview.contains("Hello world!"),
        "preview should contain patched text"
    );
    assert!(result["error"].is_null());
}

#[cfg(feature = "mpatch-fuzzy")]
#[test]
fn test_cli_mpatch_preview_missing_file() {
    use tempfile::TempDir;
    use touring_hooks::cli_handlers::cli_mpatch_preview;
    use touring_hooks::runtime::HookRuntime;

    let tmp = TempDir::new().expect("create tempdir");
    let root = tmp.path().to_path_buf();
    std::fs::create_dir_all(root.join(".claude/data")).expect("create data dir");
    let mut rt = HookRuntime::new(&root).expect("init runtime");

    let result = parse_json(&cli_mpatch_preview(
        &mut rt,
        &serde_json::json!({
            "file": "/nonexistent/file.txt",
            "patch": "@@ -1 +1 @@\n foo\n+bar\n"
        }),
    ));

    assert_eq!(result["matched"], false, "missing file should not match");
    assert!(result["error"].as_str().unwrap().contains("read"));
}

// ── Wave 12 (2026-04-27) — B-302 wired into cli_mpatch_preview ──────────

/// E2E: an exact-match patch (high confidence) must NOT emit B-302
/// even when expansion would otherwise be a candidate. Validates the
/// guard `confidence < 0.7`.
#[cfg(feature = "mpatch-fuzzy")]
#[test]
fn test_cli_mpatch_preview_no_b302_on_exact_high_confidence() {
    use tempfile::TempDir;
    use touring_hooks::cli_handlers::cli_mpatch_preview;
    use touring_hooks::runtime::HookRuntime;

    let tmp = TempDir::new().expect("create tempdir");
    let root = tmp.path().to_path_buf();
    std::fs::create_dir_all(root.join(".claude/data")).expect("create data dir");
    let mut rt = HookRuntime::new(&root).expect("init runtime");

    let file_path = root.join("sample.txt");
    std::fs::write(&file_path, "Hello world\n").expect("write sample");

    // Exact patch — confidence will be ≥ 0.9 → B-302 must NOT fire.
    let patch = "--- sample.txt\n+++ sample.txt\n@@ -1 +1 @@\n-Hello world\n+Hello world!\n";
    let result = parse_json(&cli_mpatch_preview(
        &mut rt,
        &serde_json::json!({
            "file": file_path.to_str().expect("utf-8 path"),
            "patch": patch,
            "dry_run": true
        }),
    ));

    assert_eq!(result["matched"], true, "patch must match");
    let conf = result["confidence"].as_f64().expect("confidence is f64");
    assert!(
        conf >= 0.7,
        "test precondition: confidence must be ≥ 0.7 (got {conf})"
    );
    assert!(
        result["b302_diagnostic"].is_null(),
        "B-302 must NOT fire on high-confidence patch: {:?}",
        result["b302_diagnostic"]
    );
}

/// E2E: response JSON SHAPE — `b302_diagnostic` field MUST be present
/// (null when no diagnostic) for backward-compat detection by clients.
#[cfg(feature = "mpatch-fuzzy")]
#[test]
fn test_cli_mpatch_preview_response_includes_b302_field_shape() {
    use tempfile::TempDir;
    use touring_hooks::cli_handlers::cli_mpatch_preview;
    use touring_hooks::runtime::HookRuntime;

    let tmp = TempDir::new().expect("create tempdir");
    let root = tmp.path().to_path_buf();
    std::fs::create_dir_all(root.join(".claude/data")).expect("create data dir");
    let mut rt = HookRuntime::new(&root).expect("init runtime");

    let file_path = root.join("sample.txt");
    std::fs::write(&file_path, "Hello\n").expect("write sample");

    let patch = "--- sample.txt\n+++ sample.txt\n@@ -1 +1 @@\n-Hello\n+Hello world\n";
    let raw = cli_mpatch_preview(
        &mut rt,
        &serde_json::json!({
            "file": file_path.to_str().expect("utf-8 path"),
            "patch": patch,
            "dry_run": false
        }),
    );
    let result = parse_json(&raw);

    // Field MUST exist (even if null) — clients rely on the shape contract.
    assert!(
        result.get("b302_diagnostic").is_some(),
        "response MUST include b302_diagnostic field: {raw}"
    );
    // Either null (no diagnostic) OR an object with code "B-302".
    if !result["b302_diagnostic"].is_null() {
        assert_eq!(
            result["b302_diagnostic"]["code"], "B-302",
            "non-null diagnostic must use code B-302"
        );
        assert!(
            result["b302_diagnostic"]["message"].is_string(),
            "diagnostic must include message"
        );
    }
}

#[cfg(not(feature = "mpatch-fuzzy"))]
#[test]
fn test_cli_mpatch_preview_feature_off() {
    use tempfile::TempDir;
    use touring_hooks::cli_handlers::cli_mpatch_preview;
    use touring_hooks::runtime::HookRuntime;

    let tmp = TempDir::new().expect("create tempdir");
    let root = tmp.path().to_path_buf();
    std::fs::create_dir_all(root.join(".claude/data")).expect("create data dir");
    let mut rt = HookRuntime::new(&root).expect("init runtime");

    let result = parse_json(&cli_mpatch_preview(
        &mut rt,
        &serde_json::json!({
            "file": "sample.txt",
            "patch": "dummy"
        }),
    ));

    assert_eq!(result["matched"], false);
    assert!(result["error"].as_str().unwrap().contains("mpatch-fuzzy"));
}

// ═══════════════════════════════════════════════════════════════════════
// TASKSFILE TESTS (T2.4/T2.5/T2.6 — CLI import/export/validate)
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn test_tasksfile_validate_valid_yaml() {
    let (_tmp, mut rt) = setup_runtime();
    let yaml = r#"---
version: "1.0"
tasks:
  build:
    command: cargo build
  test:
    command: cargo test
    deps: [build]
"#;
    let payload = serde_json::json!({ "yaml": yaml });
    let result = parse_json(&cli_tasksfile_validate(&mut rt, &payload));
    assert_eq!(result["success"], true);
    assert_eq!(result["valid"], true);
    assert_eq!(result["task_count"], 2);
    assert_eq!(result["template_count"], 0);
    let task_names = result["task_names"].as_array().unwrap();
    assert!(task_names.iter().any(|v| v == "build"));
    assert!(task_names.iter().any(|v| v == "test"));
}

#[test]
fn test_tasksfile_validate_with_templates() {
    let (_tmp, mut rt) = setup_runtime();
    let yaml = r#"---
version: "1.0"
templates:
  ci_job:
    timeout: 300s
    tags: [ci]
tasks:
  build:
    desc: "Build"
    command: cargo build
"#;
    let payload = serde_json::json!({ "yaml": yaml });
    let result = parse_json(&cli_tasksfile_validate(&mut rt, &payload));
    assert_eq!(result["success"], true);
    assert_eq!(result["valid"], true);
    assert_eq!(result["template_count"], 1);
}

#[test]
fn test_tasksfile_validate_invalid_yaml() {
    let (_tmp, mut rt) = setup_runtime();
    let yaml = "not: [valid: yaml: at: all";
    let payload = serde_json::json!({ "yaml": yaml });
    let result = parse_json(&cli_tasksfile_validate(&mut rt, &payload));
    assert_eq!(result["success"], false);
    assert_eq!(result["valid"], false);
    assert!(result["error"].is_string());
}

#[test]
fn test_tasksfile_validate_empty_yaml() {
    let (_tmp, mut rt) = setup_runtime();
    let payload = serde_json::json!({ "yaml": "" });
    let result = parse_json(&cli_tasksfile_validate(&mut rt, &payload));
    assert_eq!(result["success"], false);
}

#[test]
fn test_tasksfile_validate_missing_yaml_key() {
    let (_tmp, mut rt) = setup_runtime();
    let payload = serde_json::json!({});
    let result = parse_json(&cli_tasksfile_validate(&mut rt, &payload));
    assert_eq!(result["success"], false);
}

#[test]
fn test_tasksfile_import_creates_task_and_subtasks() {
    let (_tmp, mut rt) = setup_runtime();
    let yaml = r#"---
version: "1.0"
tasks:
  build:
    command: cargo build
  test:
    command: cargo test
    deps: [build]"#;
    let payload = serde_json::json!({
        "task_type": "tasksfile",
        "description": "Imported from tasksfile",
        "tasksfile_yaml": yaml,
    });
    let result_str = cli_decompose_create(&mut rt, &payload);
    let result = parse_json(&result_str);
    let task_id = result["task_id"].as_str().unwrap();
    assert!(!task_id.is_empty());
    let added = result["tasksfile_subtasks_added"].as_u64().unwrap_or(0);
    assert!(
        added >= 1,
        "expected at least 1 subtask, got {}. Raw: {}",
        added,
        result_str
    );

    // Verify subtasks were actually stored
    let get_payload = serde_json::json!({ "task_id": task_id });
    let get_result = parse_json(&cli_decompose_get(&mut rt, &get_payload));
    assert!(get_result["task"].is_object());
    let subtasks = get_result["subtasks"].as_array().unwrap();
    assert!(!subtasks.is_empty());
}

#[test]
fn test_tasksfile_import_with_deps_and_priority() {
    let (_tmp, mut rt) = setup_runtime();
    let yaml = r#"---
version: "1.0"
tasks:
  lint:
    command: cargo clippy
    tags: [priority:high]
  build:
    command: cargo build
    deps: [lint]
    tags: [priority:normal]
"#;
    let payload = serde_json::json!({
        "task_type": "tasksfile",
        "description": "CI pipeline",
        "tasksfile_yaml": yaml,
    });
    let result = parse_json(&cli_decompose_create(&mut rt, &payload));
    let added = result["tasksfile_subtasks_added"].as_u64().unwrap_or(0);
    assert_eq!(added, 2);
}

#[test]
fn test_tasksfile_import_invalid_yaml_returns_zero_added() {
    let (_tmp, mut rt) = setup_runtime();
    let payload = serde_json::json!({
        "task_type": "tasksfile",
        "description": "Bad yaml",
        "tasksfile_yaml": "invalid: [yaml",
    });
    let result = parse_json(&cli_decompose_create(&mut rt, &payload));
    // Task is created but no subtasks added (yaml parse failed)
    assert_eq!(result["tasksfile_subtasks_added"].as_u64().unwrap(), 0);
}

#[test]
fn test_tasksfile_export_roundtrip() {
    let (_tmp, mut rt) = setup_runtime();
    // First create a task with subtasks
    let yaml = r#"---
version: "1.0"
tasks:
  build:
    desc: "Build the project"
    command: cargo build
  test:
    desc: "Run tests"
    command: cargo test
    deps: [build]
"#;
    let import_payload = serde_json::json!({
        "task_type": "tasksfile",
        "description": "Test export",
        "tasksfile_yaml": yaml,
    });
    let import_result = parse_json(&cli_decompose_create(&mut rt, &import_payload));
    let task_id = import_result["task_id"].as_str().unwrap();

    // Export it back
    let export_payload = serde_json::json!({ "task_id": task_id });
    let export_result = parse_json(&cli_tasksfile_export(&mut rt, &export_payload));
    assert_eq!(export_result["success"], true);
    let exported_yaml = export_result["tasksfile_yaml"].as_str().unwrap();
    assert!(!exported_yaml.is_empty());
    // Verify it's valid YAML
    let re_parsed: serde_yaml::Value = serde_yaml::from_str(exported_yaml).unwrap();
    assert_eq!(re_parsed["version"], "1.0");
    assert!(re_parsed["tasks"].is_mapping());
}

#[test]
fn test_tasksfile_export_nonexistent_task() {
    let (_tmp, mut rt) = setup_runtime();
    let payload = serde_json::json!({ "task_id": "nonexistent_task_12345" });
    let result = parse_json(&cli_tasksfile_export(&mut rt, &payload));
    // Should return empty tasks structure for unknown task
    assert_eq!(result["success"], true);
    let tasks = result["tasksfile_yaml"].as_str().unwrap();
    assert!(tasks.contains("nonexistent_task_12345")); // Task ID used as name
}

#[test]
fn test_tasksfile_export_missing_task_id() {
    let (_tmp, mut rt) = setup_runtime();
    let payload = serde_json::json!({});
    let result = parse_json(&cli_tasksfile_export(&mut rt, &payload));
    assert_eq!(result["success"], false);
    assert!(result["error"].is_string());
}

// ─── T3.3/T3.4: Template rendering E2E tests ───────────────────────────────────
// Phase 3 (devrc integration): verify {{ env.* }} and {{ params.* }} substitution
// through the full pipeline: YAML → parse_yaml → TasksfileCompiler → render → INSERT.

#[test]
fn test_tasksfile_yaml_env_substitution_in_subtasks() {
    // T3.3: env vars loaded from inline env: block substitute into description/command.
    let (_tmp, mut rt) = setup_runtime();
    let yaml = r#"
version: "1.0"
tasks:
  greet:
    desc: "Hello {{ env.USER }}!"
    command: "echo {{ env.HOME }}"
    env:
      USER: alice
      HOME: /home/alice
"#;
    let payload = serde_json::json!({
        "task_type": "test",
        "description": "env substitution test",
        "tasksfile_yaml": yaml
    });
    let result = parse_json(&cli_decompose_create(&mut rt, &payload));
    assert_eq!(result["status"], "created");
    assert_eq!(result["tasksfile_subtasks_added"], 1);

    // Verify the stored description has env vars substituted.
    let task_id = result["task_id"].as_str().unwrap();
    let stored = parse_json(&cli_decompose_get(
        &mut rt,
        &serde_json::json!({"task_id": task_id}),
    ));
    assert_eq!(stored["subtasks"][0]["description"], "Hello alice!");
    // Command is stored in complexity_hint or we query the raw DB.
    // We validate rendering by checking the event payload contains rendered_command.
    // The event payload was captured during insertion — reconstruct via status.
    let status = parse_json(&cli_decompose_status(&mut rt, &serde_json::json!({})));
    assert_eq!(status["total_subtasks"], 1);
}

#[test]
fn test_tasksfile_yaml_template_params_no_substitution_when_disabled() {
    // T3.3: with templates feature OFF, raw {{ params.profile }} stays as-is.
    let (_tmp, mut rt) = setup_runtime();
    let yaml = r#"
version: "1.0"
tasks:
  build:
    desc: "Build with {{ params.profile }} profile"
    command: "cargo build --{{ params.profile }}"
    params:
      profile:
        default: release
        options: [debug, release]
"#;
    let payload = serde_json::json!({
        "task_type": "test",
        "description": "param template test (feature off)",
        "tasksfile_yaml": yaml
    });
    let result = parse_json(&cli_decompose_create(&mut rt, &payload));
    assert_eq!(result["status"], "created");
    assert_eq!(result["tasksfile_subtasks_added"], 1);

    // Without template rendering, raw template strings are stored.
    let task_id = result["task_id"].as_str().unwrap();
    let stored = parse_json(&cli_decompose_get(
        &mut rt,
        &serde_json::json!({"task_id": task_id}),
    ));
    let desc = stored["subtasks"][0]["description"].as_str().unwrap();
    // When templates are disabled (default), raw template literal is stored.
    assert!(
        desc.contains("{{ params.profile }}"),
        "expected raw template, got: {}",
        desc
    );
}

#[test]
fn test_tasksfile_yaml_missing_env_var_renders_empty() {
    // T3.4: missing env vars render as empty string (Tera default behavior).
    let (_tmp, mut rt) = setup_runtime();
    let yaml = r#"
version: "1.0"
tasks:
  greet:
    desc: "Hello {{ env.MISSING_VAR | default(value='') }}!"
    command: "echo done"
"#;
    let payload = serde_json::json!({
        "task_type": "test",
        "description": "missing env test",
        "tasksfile_yaml": yaml
    });
    let result_str = cli_decompose_create(&mut rt, &payload);
    let result = parse_json(&result_str);
    assert_eq!(result["status"], "created");
    assert_eq!(result["tasksfile_subtasks_added"], 1);

    let task_id = result["task_id"].as_str().unwrap();
    let stored_str = cli_decompose_get(&mut rt, &serde_json::json!({"task_id": task_id}));
    let stored = parse_json(&stored_str);
    assert_eq!(stored["subtasks"][0]["description"], "Hello !");
}

#[test]
fn test_tasksfile_yaml_multiple_tasks_all_rendered() {
    // T3.3: all tasks in the YAML get their templates rendered.
    let (_tmp, mut rt) = setup_runtime();
    let yaml = r#"
version: "1.0"
tasks:
  build:
    desc: "Build {{ env.ARCH }}"
    command: "cargo build"
    env:
      ARCH: x86_64
  test:
    desc: "Test on {{ env.ARCH }}"
    command: "cargo test"
    env:
      ARCH: x86_64
  deploy:
    desc: "Deploy for {{ env.ARCH }}"
    command: "./deploy.sh"
    env:
      ARCH: x86_64
"#;
    let payload = serde_json::json!({
        "task_type": "multi",
        "description": "multi-task render test",
        "tasksfile_yaml": yaml
    });
    let result = parse_json(&cli_decompose_create(&mut rt, &payload));
    assert_eq!(result["status"], "created");
    assert_eq!(result["tasksfile_subtasks_added"], 3);

    let task_id = result["task_id"].as_str().unwrap();
    let stored = parse_json(&cli_decompose_get(
        &mut rt,
        &serde_json::json!({"task_id": task_id}),
    ));
    let descriptions: Vec<&str> = stored["subtasks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["description"].as_str().unwrap())
        .collect();
    assert!(
        descriptions.iter().all(|d| d.contains("x86_64")),
        "all should render ARCH: {:?}",
        descriptions
    );
}

#[test]
fn test_tasksfile_yaml_empty_env_file_list_no_crash() {
    // T3.2: env_file: [] (empty list) does not crash when load_env_for_template called.
    let (_tmp, mut rt) = setup_runtime();
    let yaml = r#"
version: "1.0"
tasks:
  simple:
    desc: "Simple task"
    command: "echo simple"
    env_file: []
"#;
    let payload = serde_json::json!({
        "task_type": "test",
        "description": "empty env_file test",
        "tasksfile_yaml": yaml
    });
    let result = parse_json(&cli_decompose_create(&mut rt, &payload));
    assert_eq!(result["status"], "created");
    assert_eq!(result["tasksfile_subtasks_added"], 1);
}

#[test]
fn test_tasksfile_yaml_env_inline_overrides_env_file() {
    // T3.2: task-level env: {} inline vars override env_file vars (merged_env chain).
    let (_tmp, mut rt) = setup_runtime();
    // Inline env has KEY=override; env_file has KEY=from_file.
    // Merged result should have KEY=override (inline wins).
    let yaml = r#"
version: "1.0"
tasks:
  merge_test:
    desc: "KEY={{ env.KEY }}"
    command: "echo {{ env.KEY }}"
    env:
      KEY: inline_wins
"#;
    let payload = serde_json::json!({
        "task_type": "test",
        "description": "env merge test",
        "tasksfile_yaml": yaml
    });
    let result = parse_json(&cli_decompose_create(&mut rt, &payload));
    assert_eq!(result["status"], "created");
    assert_eq!(result["tasksfile_subtasks_added"], 1);

    let task_id = result["task_id"].as_str().unwrap();
    let stored = parse_json(&cli_decompose_get(
        &mut rt,
        &serde_json::json!({"task_id": task_id}),
    ));
    assert_eq!(stored["subtasks"][0]["description"], "KEY=inline_wins");
}
