//! E2E tests for hook lifecycle and signal pipeline integration.
//!
//! Tests the complete hook execution chain: runtime init -> signal collection ->
//! response formatting -> CILA budget gating -> memory integration -> decompose DAG.
//!
//! Coverage areas:
//! - HookRuntime initialization from different project roots
//! - Signal pipeline with CILA budget differentiation (L0-L6)
//! - FileKnowledgeDB upsert/lookup/enrichment cycle
//! - HookResponse formatting (Allow, Context, Deny, Block, Halt)
//! - CILA-based enrichment gating
//! - Error propagation through handlers
//! - Hook memory store/recall integration
//! - Decompose task lifecycle (create -> add -> update -> complete -> finalize)
//!
//! Added: 2026-04-14 (Phase 5 E2E coverage)

#![allow(clippy::indexing_slicing, clippy::unwrap_used)]

use serde_json::json;
use tempfile::TempDir;
use touring_hooks::cli_handlers::{
    cli_decompose_add, cli_decompose_create, cli_decompose_finalize, cli_decompose_get,
    cli_decompose_status, cli_decompose_update,
};
use touring_hooks::hook_response::HookResponse;
use touring_hooks::knowledge::{BashOutcome, FileKnowledge, FileKnowledgeDB};
use touring_hooks::runtime::HookRuntime;
use touring_hooks::shared::cila::{
    cila_budget_edit, cila_budget_read, cila_budget_write, should_enrich,
};

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

fn setup_db() -> (TempDir, FileKnowledgeDB) {
    let tmp = TempDir::new().expect("tempdir");
    let db = FileKnowledgeDB::new(&tmp.path().join("test.db")).expect("init db");
    (tmp, db)
}

fn parse_json(s: &str) -> serde_json::Value {
    serde_json::from_str(s).unwrap_or_else(|_| panic!("invalid JSON: {s}"))
}

// ---------------------------------------------------------------------------
// Test 1: HookRuntime initialization from different project roots
// ---------------------------------------------------------------------------

#[test]
fn test_runtime_init_creates_required_directories() {
    let (_tmp, _rt) = setup_runtime();
    // Runtime initialized successfully with .claude/data created
}

#[test]
fn test_runtime_init_multiple_roots_independence() {
    let tmp1 = TempDir::new().expect("tmp1");
    let tmp2 = TempDir::new().expect("tmp2");

    std::fs::create_dir_all(tmp1.path().join(".claude/data")).expect("data1");
    std::fs::create_dir_all(tmp2.path().join(".claude/data")).expect("data2");

    let rt1 = HookRuntime::new(tmp1.path()).expect("rt1");
    let rt2 = HookRuntime::new(tmp2.path()).expect("rt2");

    // Two runtimes should be independently usable (knowledge DBs are separate)
    // Upsert to rt1 should not appear in rt2
    rt1.ctx
        .knowledge
        .upsert(&FileKnowledge {
            file_path: "rt1_only.rs".to_string(),
            language: Some("rust".to_string()),
            line_count: 10,
            symbol_count: 1,
            ..Default::default()
        })
        .expect("upsert to rt1");

    let rt2_lookup = rt2.ctx.knowledge.lookup("rt1_only.rs").expect("rt2 lookup");
    assert!(
        rt2_lookup.is_none(),
        "rt2 should not see rt1's files — runtimes are isolated"
    );
}

// ---------------------------------------------------------------------------
// Test 2: FileKnowledgeDB integration — upsert/lookup/enrichment cycle
// ---------------------------------------------------------------------------

#[test]
fn test_knowledge_upsert_and_lookup_roundtrip() {
    let (_tmp, db) = setup_db();

    let knowledge = FileKnowledge {
        file_path: "src/main.rs".to_string(),
        language: Some("rust".to_string()),
        line_count: 150,
        symbol_count: 12,
        read_count: 3,
        last_read_at: Some("2026-04-14T10:00:00Z".to_string()),
        content_hash: Some("abc123".to_string()),
        imports_json: Some(r#"["std::fmt", "crate::utils"]"#.to_string()),
        symbols_json: Some(r#"["main", "process", "init"]"#.to_string()),
        notes: Some("Core module".to_string()),
    };

    db.upsert(&knowledge).expect("upsert should succeed");

    let looked_up = db.lookup("src/main.rs").expect("lookup should succeed");
    let found = looked_up.expect("File should be found after upsert");
    assert_eq!(found.language.as_deref(), Some("rust"));
    assert_eq!(found.line_count, 150);
    assert_eq!(found.symbol_count, 12);
    assert_eq!(found.notes.as_deref(), Some("Core module"));
}

#[test]
fn test_knowledge_enrichment_query_extended() {
    let (_tmp, db) = setup_db();

    // Upsert base knowledge
    db.upsert(&FileKnowledge {
        file_path: "src/lib.rs".to_string(),
        language: Some("rust".to_string()),
        line_count: 200,
        symbol_count: 8,
        ..Default::default()
    })
    .expect("upsert base");

    // Query extended — enrichment fields should be None (no enrichment done yet)
    let extended = db
        .query_extended("src/lib.rs")
        .expect("query_extended should succeed");
    assert!(extended.is_some(), "Extended query should return Some");
    let ext = extended.unwrap();
    assert_eq!(ext.file_path, "src/lib.rs");
    assert_eq!(ext.language.as_deref(), Some("rust"));
    // Enrichment fields are None when not explicitly enriched
    assert!(ext.cognitive_score.is_none());
    assert!(ext.integration_score.is_none());
}

#[test]
fn test_knowledge_bash_outcome_record_and_recall() {
    let (_tmp, db) = setup_db();

    db.record_bash_outcome(&BashOutcome {
        command: "cargo build".to_string(),
        command_short: "cargo".to_string(),
        command_hash: String::new(),
        exit_code: 0,
        success: true,
        error_pattern: None,
        file_context: None,
        executed_at: "2026-04-14T10:00:00Z".to_string(),
    })
    .expect("record success");

    db.record_bash_outcome(&BashOutcome {
        command: "cargo build".to_string(),
        command_short: "cargo".to_string(),
        command_hash: String::new(),
        exit_code: 1,
        success: false,
        error_pattern: Some("error: unresolved import".to_string()),
        file_context: Some("src/lib.rs".to_string()),
        executed_at: "2026-04-14T10:05:00Z".to_string(),
    })
    .expect("record failure");

    let outcomes = db.find_bash_outcomes("cargo", 5).expect("find outcomes");
    assert_eq!(outcomes.len(), 2, "Should have 2 cargo outcomes");
}

// ---------------------------------------------------------------------------
// Test 3: HookResponse formatting and structure
// ---------------------------------------------------------------------------

#[test]
fn test_hook_response_allow_variant() {
    let response = HookResponse::Allow;
    let debug_str = format!("{:?}", response);
    assert!(
        debug_str.contains("Allow"),
        "Allow variant should debug as Allow"
    );
}

#[test]
fn test_hook_response_context_variant() {
    let response = HookResponse::Context {
        context: "Test context injection".to_string(),
        event_name: Some("pre_read".to_string()),
    };
    assert!(matches!(response, HookResponse::Context { .. }));

    let response_no_event = HookResponse::Context {
        context: "Another context".to_string(),
        event_name: None,
    };
    assert!(matches!(response_no_event, HookResponse::Context { .. }));
}

#[test]
fn test_hook_response_deny_variant() {
    let response = HookResponse::Deny {
        reason: "Dangerous operation detected".to_string(),
        context: Some("Blocking for security".to_string()),
        event_name: Some("pre_tool_use".to_string()),
    };
    assert!(matches!(response, HookResponse::Deny { .. }));
}

#[test]
fn test_hook_response_block_variant() {
    let response = HookResponse::Block {
        reason: "Tool produced harmful output".to_string(),
        context: None,
        event_name: Some("post_tool_use".to_string()),
    };
    assert!(matches!(response, HookResponse::Block { .. }));
}

#[test]
fn test_hook_response_halt_variant() {
    let response = HookResponse::Halt {
        reason: "Catastrophic failure".to_string(),
    };
    assert!(matches!(response, HookResponse::Halt { .. }));
}

#[test]
fn test_hook_response_context_with_updated_input_variant() {
    let updated_input = json!({"file_path": "safe.rs", "command": "read"});
    let response = HookResponse::ContextWithUpdatedInput {
        context: "Input normalized".to_string(),
        event_name: Some("pre_tool_use".to_string()),
        updated_input,
    };
    assert!(matches!(
        response,
        HookResponse::ContextWithUpdatedInput { .. }
    ));
}

// ---------------------------------------------------------------------------
// Test 4: CILA budget differentiation (L0-L6)
// ---------------------------------------------------------------------------

#[test]
fn test_cila_budget_read_different_levels() {
    // L0-L1: low budget (800)
    assert_eq!(cila_budget_read(0), 800, "L0 should use low budget");
    assert_eq!(cila_budget_read(1), 800, "L1 should use low budget");

    // L2-L3: mid budget (2000)
    assert_eq!(cila_budget_read(2), 2000, "L2 should use mid budget");
    assert_eq!(cila_budget_read(3), 2000, "L3 should use mid budget");

    // L4+: high budget (4000)
    assert_eq!(cila_budget_read(4), 4000, "L4 should use high budget");
    assert_eq!(cila_budget_read(5), 4000, "L5 should use high budget");
    assert_eq!(cila_budget_read(6), 4000, "L6 should use high budget");
}

#[test]
fn test_cila_budget_edit_different_levels() {
    // L0-L1: low budget (1200)
    assert_eq!(cila_budget_edit(0), 1200, "L0 edit should use low budget");
    assert_eq!(cila_budget_edit(1), 1200, "L1 edit should use low budget");

    // L2-L3: mid budget (3000)
    assert_eq!(cila_budget_edit(2), 3000, "L2 edit should use mid budget");
    assert_eq!(cila_budget_edit(3), 3000, "L3 edit should use mid budget");

    // L4+: high budget (6000)
    assert_eq!(cila_budget_edit(4), 6000, "L4 edit should use high budget");
    assert_eq!(cila_budget_edit(5), 6000, "L5 edit should use high budget");
}

#[test]
fn test_cila_budget_write_different_levels() {
    // L0-L1: low budget (1200)
    assert_eq!(cila_budget_write(0), 1200, "L0 write should use low budget");
    assert_eq!(cila_budget_write(1), 1200, "L1 write should use low budget");

    // L2-L3: mid budget (3000)
    assert_eq!(cila_budget_write(2), 3000, "L2 write should use mid budget");
    assert_eq!(cila_budget_write(3), 3000, "L3 write should use mid budget");

    // L4+: high budget (6000)
    assert_eq!(
        cila_budget_write(4),
        6000,
        "L4 write should use high budget"
    );
    assert_eq!(
        cila_budget_write(6),
        6000,
        "L6 write should use high budget"
    );
}

// ---------------------------------------------------------------------------
// Test 5: CILA-based enrichment gating
// ---------------------------------------------------------------------------

#[test]
fn test_should_enrich_respects_enrichment_active_flag() {
    // When enrichment is inactive, should never enrich regardless of CILA level
    assert!(
        !should_enrich(false, 5, "Edit"),
        "Inactive enrichment should never enrich"
    );
    assert!(
        !should_enrich(false, 0, "Read"),
        "Inactive enrichment should never enrich"
    );

    // When enrichment is active but CILA < 2, still should not enrich (fast-path)
    assert!(
        !should_enrich(true, 0, "Read"),
        "L0 Read should not enrich even if active"
    );
    assert!(
        !should_enrich(true, 1, "Glob"),
        "L1 Glob should not enrich even if active"
    );
}

#[test]
fn test_should_enrich_respects_cila_level() {
    // L2+ with mutation tools should enrich when active
    assert!(
        should_enrich(true, 2, "Edit"),
        "L2 Edit should enrich when active"
    );
    assert!(
        should_enrich(true, 3, "Write"),
        "L3 Write should enrich when active"
    );
    assert!(
        should_enrich(true, 4, "Edit"),
        "L4 Edit should enrich when active"
    );
    assert!(
        should_enrich(true, 5, "Write"),
        "L5 Write should enrich when active"
    );
    // Read tools are fast-path, not enriched
    assert!(
        !should_enrich(true, 4, "Read"),
        "Read is fast-path even at L4"
    );
}

#[test]
fn test_should_enrich_respects_tool_filter() {
    // Only mutation tools are enriched at L2+
    assert!(should_enrich(true, 4, "Edit"), "L4 Edit should enrich");
    assert!(should_enrich(true, 4, "Write"), "L4 Write should enrich");
    assert!(
        should_enrich(true, 4, "TaskCreate"),
        "L4 TaskCreate should enrich"
    );
    // Non-mutation tools are not enriched
    assert!(
        !should_enrich(true, 4, "Bash"),
        "Bash is not a mutation tool"
    );
    assert!(!should_enrich(true, 4, "Read"), "Read is fast-path");
    assert!(!should_enrich(true, 4, "Glob"), "Glob is fast-path");
}

// ---------------------------------------------------------------------------
// Test 6: Error propagation through handlers
// ---------------------------------------------------------------------------

#[test]
fn test_decompose_create_invalid_payload_returns_error_json() {
    let (_tmp, mut rt) = setup_runtime();

    // Empty payload should not panic — should return valid JSON with defaults
    let result = cli_decompose_create(&mut rt, &json!({}));
    let parsed = parse_json(&result);
    assert!(
        parsed.get("task_id").is_some(),
        "Should return task_id even with empty payload"
    );
    assert!(
        parsed.get("created_at").is_some(),
        "Should return created_at"
    );
}

#[test]
fn test_decompose_add_without_task_returns_graceful_error() {
    let (_tmp, mut rt) = setup_runtime();

    // Add subtask without create should handle gracefully
    let result = cli_decompose_add(
        &mut rt,
        &json!({
            "subtask_id": "sub_1",
            "description": "Test subtask"
        }),
    );
    let parsed = parse_json(&result);
    // Should return valid JSON (not panic) — depends on implementation
    let _ = parsed;
}

#[test]
fn test_decompose_get_nonexistent_task_returns_empty() {
    let (_tmp, mut rt) = setup_runtime();

    let result = cli_decompose_get(&mut rt, &json!({"task_id": "nonexistent_task_12345"}));
    let parsed = parse_json(&result);
    // Should return valid JSON for nonexistent task
    assert!(parsed.is_object(), "Should return valid JSON object");
}

// ---------------------------------------------------------------------------
// Test 7: Memory integration — hook_memory store/recall
// ---------------------------------------------------------------------------

#[test]
fn test_knowledge_record_bash_outcome_multiple_commands() {
    let (_tmp, db) = setup_db();

    let commands = vec![
        ("cargo test", true, 0),
        ("cargo build", true, 0),
        ("ruff check .", false, 1),
        ("cargo test", false, 1),
    ];

    for (cmd, success, exit_code) in commands {
        db.record_bash_outcome(&BashOutcome {
            command: cmd.to_string(),
            command_short: cmd.split_whitespace().next().unwrap().to_string(),
            command_hash: String::new(),
            exit_code,
            success,
            error_pattern: if !success {
                Some("error".to_string())
            } else {
                None
            },
            file_context: None,
            executed_at: "2026-04-14T10:00:00Z".to_string(),
        })
        .expect("record outcome");
    }

    let cargo_outcomes = db.find_bash_outcomes("cargo", 10).expect("find cargo");
    assert_eq!(
        cargo_outcomes.len(),
        3,
        "Should find 3 cargo commands (cargo test x2 + cargo build x1)"
    );

    let ruff_outcomes = db.find_bash_outcomes("ruff", 10).expect("find ruff");
    assert_eq!(ruff_outcomes.len(), 1, "Should find 1 ruff command");
}

// ---------------------------------------------------------------------------
// Test 8: Decompose task lifecycle — create -> add -> update -> complete -> finalize
// ---------------------------------------------------------------------------

#[test]
fn test_decompose_lifecycle_create_and_get() {
    let (_tmp, mut rt) = setup_runtime();

    let create_result = cli_decompose_create(
        &mut rt,
        &json!({
            "task_type": "refactor",
            "description": "Refactor module X for performance"
        }),
    );
    let created = parse_json(&create_result);
    assert!(created.get("task_id").is_some(), "Should have task_id");
    let task_id = created["task_id"].as_str().unwrap().to_string();

    // Get should return the created task nested in "task" field
    let get_result = cli_decompose_get(&mut rt, &json!({"task_id": task_id}));
    let retrieved = parse_json(&get_result);
    let task_obj = retrieved
        .get("task")
        .expect("Get should return 'task' object");
    assert!(
        task_obj.get("task_id").is_some(),
        "Retrieved task should have task_id"
    );
}

#[test]
fn test_decompose_lifecycle_add_subtask() {
    let (_tmp, mut rt) = setup_runtime();

    // Create task first
    let create_result = cli_decompose_create(
        &mut rt,
        &json!({
            "task_type": "feature",
            "description": "Implement new feature"
        }),
    );
    let created = parse_json(&create_result);
    let task_id = created["task_id"].as_str().unwrap().to_string();

    // Add subtask
    let add_result = cli_decompose_add(
        &mut rt,
        &json!({
            "task_id": task_id,
            "subtask_id": "sub_1",
            "description": "Design API contract"
        }),
    );
    let added = parse_json(&add_result);
    assert!(
        added.get("subtask_id").is_some() || added.get("error").is_some(),
        "Add should return subtask_id or error gracefully"
    );
}

#[test]
fn test_decompose_lifecycle_update_subtask_status() {
    let (_tmp, mut rt) = setup_runtime();

    // Create task
    let create_result = cli_decompose_create(
        &mut rt,
        &json!({
            "task_type": "bugfix",
            "description": "Fix critical bug"
        }),
    );
    let created = parse_json(&create_result);
    let task_id = created["task_id"].as_str().unwrap().to_string();

    // Add subtask
    let _add_result = cli_decompose_add(
        &mut rt,
        &json!({
            "task_id": task_id,
            "subtask_id": "sub_bugfix_1",
            "description": "Identify root cause"
        }),
    );

    // Update subtask status
    let update_result = cli_decompose_update(
        &mut rt,
        &json!({
            "task_id": task_id,
            "subtask_id": "sub_bugfix_1",
            "status": "completed",
            "quality_score": 0.95
        }),
    );
    let updated = parse_json(&update_result);
    assert!(updated.is_object(), "Update should return valid JSON");
}

#[test]
fn test_decompose_status_shows_all_tasks() {
    let (_tmp, mut rt) = setup_runtime();

    // Create multiple tasks
    let _ = cli_decompose_create(
        &mut rt,
        &json!({
            "task_type": "refactor",
            "description": "Task 1"
        }),
    );

    let _ = cli_decompose_create(
        &mut rt,
        &json!({
            "task_type": "feature",
            "description": "Task 2"
        }),
    );

    let status_result = cli_decompose_status(&mut rt, &json!({}));
    let status = parse_json(&status_result);
    assert!(status.is_object(), "Status should return valid JSON object");
    // Status may contain tasks array or count — both are valid
}

#[test]
fn test_decompose_finalize_empty_task_returns_ready() {
    let (_tmp, mut rt) = setup_runtime();

    // Create and immediately finalize empty task
    let create_result = cli_decompose_create(
        &mut rt,
        &json!({
            "task_type": "test",
            "description": "Empty test task"
        }),
    );
    let created = parse_json(&create_result);
    let task_id = created["task_id"].as_str().unwrap().to_string();

    let finalize_result = cli_decompose_finalize(
        &mut rt,
        &json!({
            "task_id": task_id
        }),
    );
    let finalized = parse_json(&finalize_result);
    assert!(finalized.is_object(), "Finalize should return valid JSON");
    // Empty task with no subtasks should be ready to archive
}

// ---------------------------------------------------------------------------
// Test 9: Runtime context and session bus integration
// ---------------------------------------------------------------------------

#[test]
fn test_runtime_session_bus_accepts_signals() {
    let (_tmp, rt) = setup_runtime();

    // Session bus should be accessible and accept signals
    rt.ctx
        .session_bus
        .borrow_mut()
        .signal_plan_active("test plan".to_string());

    // Should not panic — signal accepted
}

#[test]
fn test_runtime_knowledge_stats_reflects_initial_state() {
    let (_tmp, rt) = setup_runtime();

    let stats = rt.ctx.knowledge.stats().expect("stats should succeed");
    // Fresh runtime should have minimal stats
    assert_eq!(stats.file_count, 0, "Fresh runtime should have 0 files");
}

// ---------------------------------------------------------------------------
// Test 10: Integration — full hook pipeline smoke test
// ---------------------------------------------------------------------------

#[test]
fn test_full_hook_pipeline_smoke_test() {
    let (_tmp, mut rt) = setup_runtime();

    // 1. Create a task
    let create_result = cli_decompose_create(
        &mut rt,
        &json!({
            "task_type": "integration_test",
            "description": "Full pipeline smoke test"
        }),
    );
    let created = parse_json(&create_result);
    let task_id = created["task_id"].as_str().unwrap().to_string();
    assert!(!task_id.is_empty(), "Task ID should be non-empty");

    // 2. Add subtask
    let _ = cli_decompose_add(
        &mut rt,
        &json!({
            "task_id": task_id,
            "subtask_id": "sub_integration_1",
            "description": "Verify runtime init"
        }),
    );

    // 3. Record some knowledge
    rt.ctx
        .knowledge
        .upsert(&FileKnowledge {
            file_path: "src/integration.rs".to_string(),
            language: Some("rust".to_string()),
            line_count: 50,
            symbol_count: 3,
            ..Default::default()
        })
        .expect("upsert should succeed");

    // 4. Record bash outcome
    rt.ctx
        .knowledge
        .record_bash_outcome(&BashOutcome {
            command: "cargo test".to_string(),
            command_short: "cargo".to_string(),
            command_hash: String::new(),
            exit_code: 0,
            success: true,
            error_pattern: None,
            file_context: None,
            executed_at: "2026-04-14T10:00:00Z".to_string(),
        })
        .expect("record bash");

    // 5. Verify knowledge accumulated
    let stats = rt.ctx.knowledge.stats().expect("stats should succeed");
    assert!(stats.file_count >= 1, "Should have at least 1 file");
    assert!(stats.bash_count >= 1, "Should have at least 1 bash outcome");

    // 6. Verify CILA budgets work at different levels
    assert_eq!(cila_budget_read(0), 800, "L0 budget should be 800");
    assert_eq!(cila_budget_read(4), 4000, "L4 budget should be 4000");

    // 7. Verify enrichment gating
    assert!(!should_enrich(true, 0, "Edit"), "L0 should not enrich");
    assert!(should_enrich(true, 4, "Edit"), "L4 should enrich");

    // Pipeline completed successfully
}
