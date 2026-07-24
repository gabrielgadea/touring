use super::*;

#[test]
fn test_known_structs_are_sorted() {
    // Verify KNOWN_STRUCTS list is reasonable
    assert!(KNOWN_STRUCTS.len() >= 10);
    assert!(KNOWN_STRUCTS.contains(&"BlastRadius"));
    assert!(KNOWN_STRUCTS.contains(&"MemoryMatch"));
}

#[test]
fn test_relevant_patterns() {
    assert!(
        RELEVANT_PATTERNS
            .iter()
            .any(|p| "antt-vote-drafter".contains(p))
    );
    assert!(
        !RELEVANT_PATTERNS
            .iter()
            .any(|p| "general-purpose".contains(p))
    );
}

#[test]
fn test_code_exts_coverage() {
    assert!(CODE_EXTS.contains(&".py"));
    assert!(CODE_EXTS.contains(&".rs"));
    assert!(CODE_EXTS.contains(&".ts"));
    assert!(!CODE_EXTS.contains(&".txt"));
}

#[test]
fn test_skip_parts_coverage() {
    assert!(SKIP_PARTS.contains(&".venv"));
    assert!(SKIP_PARTS.contains(&"node_modules"));
    assert!(SKIP_PARTS.contains(&"target"));
}

// ── H60: PermissionAutoApproverHandler ───────────────────────────

#[test]
fn test_permission_auto_approver_name_and_events() {
    let h = PermissionAutoApproverHandler;
    assert_eq!(h.name(), "permission_auto_approver");
    assert_eq!(h.events(), &[HookEvent::PermissionRequest]);
    assert!(!h.is_async());
}

#[test]
fn test_permission_auto_approver_approves_touring_tool() {
    use std::sync::Arc;
    use tempfile::TempDir;
    use touring_hooks::knowledge::FileKnowledgeDB;

    let tmp = TempDir::new().unwrap();
    let db = FileKnowledgeDB::new(&tmp.path().join("k.db")).unwrap();
    #[allow(clippy::arc_with_non_send_sync)]
    let knowledge = Arc::new(db);
    let input = serde_json::json!({
        "tool_name": "mcp__touring__touring_ast_find",
    });
    let mut ctx = crate::context::CortexContext::from_input(
        HookEvent::PermissionRequest,
        input,
        knowledge,
        tmp.path().to_path_buf(),
    );

    let result = PermissionAutoApproverHandler.execute(&mut ctx);
    assert_eq!(result.decision, crate::types::Decision::Allow);
    assert!(!result.context_lines.is_empty());
    assert!(result.context_lines[0].contains("mcp__touring__touring_ast_find"));
}

#[test]
fn test_permission_auto_approver_skips_non_touring_tool() {
    use std::sync::Arc;
    use tempfile::TempDir;
    use touring_hooks::knowledge::FileKnowledgeDB;

    let tmp = TempDir::new().unwrap();
    let db = FileKnowledgeDB::new(&tmp.path().join("k.db")).unwrap();
    #[allow(clippy::arc_with_non_send_sync)]
    let knowledge = Arc::new(db);
    let input = serde_json::json!({
        "tool_name": "Bash",
    });
    let mut ctx = crate::context::CortexContext::from_input(
        HookEvent::PermissionRequest,
        input,
        knowledge,
        tmp.path().to_path_buf(),
    );

    let result = PermissionAutoApproverHandler.execute(&mut ctx);
    assert_eq!(result.decision, crate::types::Decision::Skip);
}

#[test]
fn test_permission_auto_approver_skips_empty_tool_name() {
    use std::sync::Arc;
    use tempfile::TempDir;
    use touring_hooks::knowledge::FileKnowledgeDB;

    let tmp = TempDir::new().unwrap();
    let db = FileKnowledgeDB::new(&tmp.path().join("k.db")).unwrap();
    #[allow(clippy::arc_with_non_send_sync)]
    let knowledge = Arc::new(db);
    let input = serde_json::json!({});
    let mut ctx = crate::context::CortexContext::from_input(
        HookEvent::PermissionRequest,
        input,
        knowledge,
        tmp.path().to_path_buf(),
    );

    let result = PermissionAutoApproverHandler.execute(&mut ctx);
    assert_eq!(result.decision, crate::types::Decision::Skip);
}

// ── H61: SymbolEnricherHandler ───────────────────────────────────

#[test]
fn test_symbol_enricher_name_and_events() {
    let h = SymbolEnricherHandler;
    assert_eq!(h.name(), "symbol_enricher");
    assert_eq!(h.events(), &[HookEvent::PreToolUse]);
    assert_eq!(h.tool_matcher(), Some("Read|Edit"));
    assert!(!h.is_async());
}

#[test]
fn test_symbol_enricher_skips_no_file_path() {
    use std::sync::Arc;
    use tempfile::TempDir;
    use touring_hooks::knowledge::FileKnowledgeDB;

    let tmp = TempDir::new().unwrap();
    let db = FileKnowledgeDB::new(&tmp.path().join("k.db")).unwrap();
    #[allow(clippy::arc_with_non_send_sync)]
    let knowledge = Arc::new(db);
    let input = serde_json::json!({ "tool_name": "Read" });
    let mut ctx = crate::context::CortexContext::from_input(
        HookEvent::PreToolUse,
        input,
        knowledge,
        tmp.path().to_path_buf(),
    );

    let result = SymbolEnricherHandler.execute(&mut ctx);
    assert_eq!(result.decision, crate::types::Decision::Skip);
}

#[test]
fn test_symbol_enricher_skips_non_code_file() {
    use std::sync::Arc;
    use tempfile::TempDir;
    use touring_hooks::knowledge::FileKnowledgeDB;

    let tmp = TempDir::new().unwrap();
    let db = FileKnowledgeDB::new(&tmp.path().join("k.db")).unwrap();
    #[allow(clippy::arc_with_non_send_sync)]
    let knowledge = Arc::new(db);
    let input = serde_json::json!({
        "tool_name": "Read",
        "tool_input": { "file_path": "/tmp/data.csv" }
    });
    let mut ctx = crate::context::CortexContext::from_input(
        HookEvent::PreToolUse,
        input,
        knowledge,
        tmp.path().to_path_buf(),
    );

    let result = SymbolEnricherHandler.execute(&mut ctx);
    assert_eq!(result.decision, crate::types::Decision::Skip);
}

#[test]
fn test_symbol_enricher_skips_low_budget() {
    use std::sync::Arc;
    use tempfile::TempDir;
    use touring_hooks::knowledge::FileKnowledgeDB;

    let tmp = TempDir::new().unwrap();
    let db = FileKnowledgeDB::new(&tmp.path().join("k.db")).unwrap();
    #[allow(clippy::arc_with_non_send_sync)]
    let knowledge = Arc::new(db);
    let input = serde_json::json!({
        "tool_name": "Read",
        "tool_input": { "file_path": "/project/src/main.rs" }
    });
    let mut ctx = crate::context::CortexContext::from_input(
        HookEvent::PreToolUse,
        input,
        knowledge,
        tmp.path().to_path_buf(),
    );
    // Force low budget
    ctx.context_budget_remaining = 10;

    let result = SymbolEnricherHandler.execute(&mut ctx);
    assert_eq!(result.decision, crate::types::Decision::Skip);
}

// ── H62: SemanticReadEnricherHandler ─────────────────────────────

#[test]
fn test_semantic_read_enricher_name_and_events() {
    let h = SemanticReadEnricherHandler;
    assert_eq!(h.name(), "semantic_read_enricher");
    assert_eq!(h.events(), &[HookEvent::PreToolUse]);
    assert_eq!(h.tool_matcher(), Some("Read"));
    assert!(!h.is_async());
}

#[test]
fn test_semantic_read_enricher_skips_no_file_path() {
    use std::sync::Arc;
    use tempfile::TempDir;
    use touring_hooks::knowledge::FileKnowledgeDB;

    let tmp = TempDir::new().unwrap();
    let db = FileKnowledgeDB::new(&tmp.path().join("k.db")).unwrap();
    #[allow(clippy::arc_with_non_send_sync)]
    let knowledge = Arc::new(db);
    let input = serde_json::json!({ "tool_name": "Read" });
    let mut ctx = crate::context::CortexContext::from_input(
        HookEvent::PreToolUse,
        input,
        knowledge,
        tmp.path().to_path_buf(),
    );
    let result = SemanticReadEnricherHandler.execute(&mut ctx);
    assert_eq!(result.decision, crate::types::Decision::Skip);
}

#[test]
fn test_semantic_read_enricher_skips_non_code_file() {
    use std::sync::Arc;
    use tempfile::TempDir;
    use touring_hooks::knowledge::FileKnowledgeDB;

    let tmp = TempDir::new().unwrap();
    let db = FileKnowledgeDB::new(&tmp.path().join("k.db")).unwrap();
    #[allow(clippy::arc_with_non_send_sync)]
    let knowledge = Arc::new(db);
    let input = serde_json::json!({
        "tool_name": "Read",
        "tool_input": { "file_path": "/tmp/readme.txt" }
    });
    let mut ctx = crate::context::CortexContext::from_input(
        HookEvent::PreToolUse,
        input,
        knowledge,
        tmp.path().to_path_buf(),
    );
    let result = SemanticReadEnricherHandler.execute(&mut ctx);
    assert_eq!(result.decision, crate::types::Decision::Skip);
}

#[test]
fn test_semantic_read_enricher_skips_venv_path() {
    use std::sync::Arc;
    use tempfile::TempDir;
    use touring_hooks::knowledge::FileKnowledgeDB;

    let tmp = TempDir::new().unwrap();
    let db = FileKnowledgeDB::new(&tmp.path().join("k.db")).unwrap();
    #[allow(clippy::arc_with_non_send_sync)]
    let knowledge = Arc::new(db);
    let input = serde_json::json!({
        "tool_name": "Read",
        "tool_input": { "file_path": "/project/.venv/lib/something.py" }
    });
    let mut ctx = crate::context::CortexContext::from_input(
        HookEvent::PreToolUse,
        input,
        knowledge,
        tmp.path().to_path_buf(),
    );
    let result = SemanticReadEnricherHandler.execute(&mut ctx);
    assert_eq!(result.decision, crate::types::Decision::Skip);
}

#[test]
fn test_semantic_read_enricher_skips_node_modules() {
    use std::sync::Arc;
    use tempfile::TempDir;
    use touring_hooks::knowledge::FileKnowledgeDB;

    let tmp = TempDir::new().unwrap();
    let db = FileKnowledgeDB::new(&tmp.path().join("k.db")).unwrap();
    #[allow(clippy::arc_with_non_send_sync)]
    let knowledge = Arc::new(db);
    let input = serde_json::json!({
        "tool_name": "Read",
        "tool_input": { "file_path": "/project/node_modules/lib/index.ts" }
    });
    let mut ctx = crate::context::CortexContext::from_input(
        HookEvent::PreToolUse,
        input,
        knowledge,
        tmp.path().to_path_buf(),
    );
    let result = SemanticReadEnricherHandler.execute(&mut ctx);
    assert_eq!(result.decision, crate::types::Decision::Skip);
}

#[test]
fn test_semantic_read_enricher_skips_low_budget() {
    use std::sync::Arc;
    use tempfile::TempDir;
    use touring_hooks::knowledge::FileKnowledgeDB;

    let tmp = TempDir::new().unwrap();
    let db = FileKnowledgeDB::new(&tmp.path().join("k.db")).unwrap();
    #[allow(clippy::arc_with_non_send_sync)]
    let knowledge = Arc::new(db);
    let input = serde_json::json!({
        "tool_name": "Read",
        "tool_input": { "file_path": "/project/src/main.py" }
    });
    let mut ctx = crate::context::CortexContext::from_input(
        HookEvent::PreToolUse,
        input,
        knowledge,
        tmp.path().to_path_buf(),
    );
    ctx.context_budget_remaining = 50;
    let result = SemanticReadEnricherHandler.execute(&mut ctx);
    assert_eq!(result.decision, crate::types::Decision::Skip);
}

// ── H63: VgpAdvisoryHandler ──────────────────────────────────────

#[test]
fn test_vgp_advisory_name_and_events() {
    let h = VgpAdvisoryHandler;
    assert_eq!(h.name(), "vgp_advisory");
    assert_eq!(h.events(), &[HookEvent::PreToolUse]);
    assert_eq!(h.tool_matcher(), Some("Write"));
    assert!(!h.is_async());
}

#[test]
fn test_vgp_advisory_skips_no_file_path() {
    use std::sync::Arc;
    use tempfile::TempDir;
    use touring_hooks::knowledge::FileKnowledgeDB;

    let tmp = TempDir::new().unwrap();
    let db = FileKnowledgeDB::new(&tmp.path().join("k.db")).unwrap();
    #[allow(clippy::arc_with_non_send_sync)]
    let knowledge = Arc::new(db);
    let input = serde_json::json!({ "tool_name": "Write" });
    let mut ctx = crate::context::CortexContext::from_input(
        HookEvent::PreToolUse,
        input,
        knowledge,
        tmp.path().to_path_buf(),
    );
    let result = VgpAdvisoryHandler.execute(&mut ctx);
    assert_eq!(result.decision, crate::types::Decision::Skip);
}

#[test]
fn test_vgp_advisory_skips_non_code_file() {
    use std::sync::Arc;
    use tempfile::TempDir;
    use touring_hooks::knowledge::FileKnowledgeDB;

    let tmp = TempDir::new().unwrap();
    let db = FileKnowledgeDB::new(&tmp.path().join("k.db")).unwrap();
    #[allow(clippy::arc_with_non_send_sync)]
    let knowledge = Arc::new(db);
    let input = serde_json::json!({
        "tool_name": "Write",
        "tool_input": { "file_path": "/tmp/data.json", "content": "BlastRadius" }
    });
    let mut ctx = crate::context::CortexContext::from_input(
        HookEvent::PreToolUse,
        input,
        knowledge,
        tmp.path().to_path_buf(),
    );
    let result = VgpAdvisoryHandler.execute(&mut ctx);
    assert_eq!(result.decision, crate::types::Decision::Skip);
}

#[test]
fn test_vgp_advisory_warns_on_unverified_struct() {
    use std::sync::Arc;
    use tempfile::TempDir;
    use touring_hooks::knowledge::FileKnowledgeDB;

    let tmp = TempDir::new().unwrap();
    let db = FileKnowledgeDB::new(&tmp.path().join("k.db")).unwrap();
    #[allow(clippy::arc_with_non_send_sync)]
    let knowledge = Arc::new(db);
    let input = serde_json::json!({
        "tool_name": "Write",
        "tool_input": {
            "file_path": "/project/src/main.rs",
            "content": "let x: BlastRadius = BlastRadius::new();"
        }
    });
    let mut ctx = crate::context::CortexContext::from_input(
        HookEvent::PreToolUse,
        input,
        knowledge,
        tmp.path().to_path_buf(),
    );
    let result = VgpAdvisoryHandler.execute(&mut ctx);
    assert_eq!(result.decision, crate::types::Decision::Allow);
    assert!(!result.context_lines.is_empty());
    assert!(result.context_lines[0].contains("VGP Advisory"));
    assert!(result.context_lines[0].contains("BlastRadius"));
}

#[test]
fn test_vgp_advisory_skips_empty_content() {
    use std::sync::Arc;
    use tempfile::TempDir;
    use touring_hooks::knowledge::FileKnowledgeDB;

    let tmp = TempDir::new().unwrap();
    let db = FileKnowledgeDB::new(&tmp.path().join("k.db")).unwrap();
    #[allow(clippy::arc_with_non_send_sync)]
    let knowledge = Arc::new(db);
    let input = serde_json::json!({
        "tool_name": "Write",
        "tool_input": { "file_path": "/project/src/lib.rs", "content": "" }
    });
    let mut ctx = crate::context::CortexContext::from_input(
        HookEvent::PreToolUse,
        input,
        knowledge,
        tmp.path().to_path_buf(),
    );
    let result = VgpAdvisoryHandler.execute(&mut ctx);
    assert_eq!(result.decision, crate::types::Decision::Skip);
}

#[test]
fn test_vgp_advisory_skips_content_without_known_structs() {
    use std::sync::Arc;
    use tempfile::TempDir;
    use touring_hooks::knowledge::FileKnowledgeDB;

    let tmp = TempDir::new().unwrap();
    let db = FileKnowledgeDB::new(&tmp.path().join("k.db")).unwrap();
    #[allow(clippy::arc_with_non_send_sync)]
    let knowledge = Arc::new(db);
    let input = serde_json::json!({
        "tool_name": "Write",
        "tool_input": {
            "file_path": "/project/src/lib.rs",
            "content": "fn hello() { println!(\"no known structs here\"); }"
        }
    });
    let mut ctx = crate::context::CortexContext::from_input(
        HookEvent::PreToolUse,
        input,
        knowledge,
        tmp.path().to_path_buf(),
    );
    let result = VgpAdvisoryHandler.execute(&mut ctx);
    assert_eq!(result.decision, crate::types::Decision::Skip);
}

// ── H64: BlastRadiusGuardHandler ─────────────────────────────────

#[test]
fn test_blast_radius_guard_name_and_events() {
    let h = BlastRadiusGuardHandler;
    assert_eq!(h.name(), "blast_radius_guard");
    assert_eq!(h.events(), &[HookEvent::PreToolUse]);
    assert_eq!(h.tool_matcher(), Some("Edit"));
    assert!(!h.is_async());
}

#[test]
fn test_blast_radius_guard_skips_no_file_path() {
    use std::sync::Arc;
    use tempfile::TempDir;
    use touring_hooks::knowledge::FileKnowledgeDB;

    let tmp = TempDir::new().unwrap();
    let db = FileKnowledgeDB::new(&tmp.path().join("k.db")).unwrap();
    #[allow(clippy::arc_with_non_send_sync)]
    let knowledge = Arc::new(db);
    let input = serde_json::json!({ "tool_name": "Edit" });
    let mut ctx = crate::context::CortexContext::from_input(
        HookEvent::PreToolUse,
        input,
        knowledge,
        tmp.path().to_path_buf(),
    );
    let result = BlastRadiusGuardHandler.execute(&mut ctx);
    assert_eq!(result.decision, crate::types::Decision::Skip);
}

#[test]
fn test_blast_radius_guard_skips_non_code_file() {
    use std::sync::Arc;
    use tempfile::TempDir;
    use touring_hooks::knowledge::FileKnowledgeDB;

    let tmp = TempDir::new().unwrap();
    let db = FileKnowledgeDB::new(&tmp.path().join("k.db")).unwrap();
    #[allow(clippy::arc_with_non_send_sync)]
    let knowledge = Arc::new(db);
    let input = serde_json::json!({
        "tool_name": "Edit",
        "tool_input": { "file_path": "/project/README.md" }
    });
    let mut ctx = crate::context::CortexContext::from_input(
        HookEvent::PreToolUse,
        input,
        knowledge,
        tmp.path().to_path_buf(),
    );
    let result = BlastRadiusGuardHandler.execute(&mut ctx);
    assert_eq!(result.decision, crate::types::Decision::Skip);
}

#[test]
fn test_blast_radius_guard_skips_no_dependents() {
    use std::sync::Arc;
    use tempfile::TempDir;
    use touring_hooks::knowledge::FileKnowledgeDB;

    let tmp = TempDir::new().unwrap();
    let db = FileKnowledgeDB::new(&tmp.path().join("k.db")).unwrap();
    #[allow(clippy::arc_with_non_send_sync)]
    let knowledge = Arc::new(db);
    // File path inside project root so rel_path resolves correctly
    let project_root = tmp.path().to_path_buf();
    let file_path = project_root.join("src/isolated.rs");
    let input = serde_json::json!({
        "tool_name": "Edit",
        "tool_input": { "file_path": file_path.to_string_lossy() }
    });
    let mut ctx = crate::context::CortexContext::from_input(
        HookEvent::PreToolUse,
        input,
        knowledge,
        project_root,
    );
    // No entries in DB → no dependents → skip
    let result = BlastRadiusGuardHandler.execute(&mut ctx);
    assert_eq!(result.decision, crate::types::Decision::Skip);
}

// ── H65: RlmExecutionCaptureHandler ──────────────────────────────

#[test]
fn test_rlm_execution_capture_name_and_events() {
    let h = RlmExecutionCaptureHandler;
    assert_eq!(h.name(), "rlm_execution_capture");
    assert_eq!(h.events(), &[HookEvent::PostToolUse]);
    assert_eq!(h.tool_matcher(), Some("Task"));
    assert!(h.is_async());
}

#[test]
fn test_rlm_execution_capture_skips_non_relevant_agent() {
    use std::sync::Arc;
    use tempfile::TempDir;
    use touring_hooks::knowledge::FileKnowledgeDB;

    let tmp = TempDir::new().unwrap();
    let db = FileKnowledgeDB::new(&tmp.path().join("k.db")).unwrap();
    #[allow(clippy::arc_with_non_send_sync)]
    let knowledge = Arc::new(db);
    let input = serde_json::json!({
        "tool_name": "Task",
        "tool_input": { "subagent_type": "general-assistant" }
    });
    let mut ctx = crate::context::CortexContext::from_input(
        HookEvent::PostToolUse,
        input,
        knowledge,
        tmp.path().to_path_buf(),
    );
    let result = RlmExecutionCaptureHandler.execute(&mut ctx);
    assert_eq!(result.decision, crate::types::Decision::Skip);
}

#[test]
fn test_rlm_execution_capture_skips_when_no_subagent_type() {
    use std::sync::Arc;
    use tempfile::TempDir;
    use touring_hooks::knowledge::FileKnowledgeDB;

    let tmp = TempDir::new().unwrap();
    let db = FileKnowledgeDB::new(&tmp.path().join("k.db")).unwrap();
    #[allow(clippy::arc_with_non_send_sync)]
    let knowledge = Arc::new(db);
    let input = serde_json::json!({
        "tool_name": "Task",
        "tool_input": {}
    });
    let mut ctx = crate::context::CortexContext::from_input(
        HookEvent::PostToolUse,
        input,
        knowledge,
        tmp.path().to_path_buf(),
    );
    let result = RlmExecutionCaptureHandler.execute(&mut ctx);
    assert_eq!(result.decision, crate::types::Decision::Skip);
}

#[test]
fn test_relevant_patterns_matches_antt_types() {
    // Verify patterns cover expected ANTT agent types
    let antt_types = [
        "antt-legal-analyzer",
        "vote-drafter",
        "voto-final",
        "reequilibrio-check",
    ];
    for t in &antt_types {
        assert!(
            RELEVANT_PATTERNS.iter().any(|p| t.contains(p)),
            "Pattern not matched for: {t}"
        );
    }
}

// ── H66: PgccDriftValidatorHandler ───────────────────────────────

#[test]
fn test_pgcc_drift_validator_name_and_events() {
    let h = PgccDriftValidatorHandler;
    assert_eq!(h.name(), "pgcc_drift_validator");
    assert_eq!(h.events(), &[HookEvent::PreToolUse]);
    assert_eq!(h.tool_matcher(), Some("Write|Edit|MultiEdit"));
    assert!(!h.is_async());
}

#[test]
fn test_pgcc_drift_validator_skips_non_py_rs_file() {
    use std::sync::Arc;
    use tempfile::TempDir;
    use touring_hooks::knowledge::FileKnowledgeDB;

    let tmp = TempDir::new().unwrap();
    let db = FileKnowledgeDB::new(&tmp.path().join("k.db")).unwrap();
    #[allow(clippy::arc_with_non_send_sync)]
    let knowledge = Arc::new(db);
    let input = serde_json::json!({
        "tool_name": "Edit",
        "tool_input": {
            "file_path": "/project/src/main.ts",
            "new_string": "const x = 1;"
        }
    });
    let mut ctx = crate::context::CortexContext::from_input(
        HookEvent::PreToolUse,
        input,
        knowledge,
        tmp.path().to_path_buf(),
    );
    let result = PgccDriftValidatorHandler.execute(&mut ctx);
    assert_eq!(result.decision, crate::types::Decision::Skip);
}

#[test]
fn test_pgcc_drift_validator_skips_short_content() {
    use std::sync::Arc;
    use tempfile::TempDir;
    use touring_hooks::knowledge::FileKnowledgeDB;

    let tmp = TempDir::new().unwrap();
    let db = FileKnowledgeDB::new(&tmp.path().join("k.db")).unwrap();
    #[allow(clippy::arc_with_non_send_sync)]
    let knowledge = Arc::new(db);
    let input = serde_json::json!({
        "tool_name": "Edit",
        "tool_input": {
            "file_path": "/project/src/main.py",
            "new_string": "x = 1"
        }
    });
    let mut ctx = crate::context::CortexContext::from_input(
        HookEvent::PreToolUse,
        input,
        knowledge,
        tmp.path().to_path_buf(),
    );
    let result = PgccDriftValidatorHandler.execute(&mut ctx);
    assert_eq!(result.decision, crate::types::Decision::Skip);
}

#[test]
fn test_pgcc_drift_validator_skips_new_file() {
    use std::sync::Arc;
    use tempfile::TempDir;
    use touring_hooks::knowledge::FileKnowledgeDB;

    let tmp = TempDir::new().unwrap();
    let db = FileKnowledgeDB::new(&tmp.path().join("k.db")).unwrap();
    #[allow(clippy::arc_with_non_send_sync)]
    let knowledge = Arc::new(db);
    // File doesn't exist on disk → treated as new file → skip
    let input = serde_json::json!({
        "tool_name": "Write",
        "tool_input": {
            "file_path": "/nonexistent/project/new_module.py",
            "content": "def foo():\n    pass\n\ndef bar():\n    return 42\n\ndef baz():\n    return True\n"
        }
    });
    let mut ctx = crate::context::CortexContext::from_input(
        HookEvent::PreToolUse,
        input,
        knowledge,
        tmp.path().to_path_buf(),
    );
    let result = PgccDriftValidatorHandler.execute(&mut ctx);
    // New file (doesn't exist) → skip
    assert_eq!(result.decision, crate::types::Decision::Skip);
}

// ── H67: RustAcceleratorCheckHandler ─────────────────────────────

#[test]
fn test_rust_accelerator_check_name_and_events() {
    let h = RustAcceleratorCheckHandler;
    assert_eq!(h.name(), "rust_accelerator_check");
    assert_eq!(h.events(), &[HookEvent::SessionStart]);
    assert!(h.is_async());
}

#[test]
fn test_rust_accelerator_check_executes_and_skips_context() {
    use std::sync::Arc;
    use tempfile::TempDir;
    use touring_hooks::knowledge::FileKnowledgeDB;

    let tmp = TempDir::new().unwrap();
    let db = FileKnowledgeDB::new(&tmp.path().join("k.db")).unwrap();
    #[allow(clippy::arc_with_non_send_sync)]
    let knowledge = Arc::new(db);
    let input = serde_json::json!({});
    let mut ctx = crate::context::CortexContext::from_input(
        HookEvent::SessionStart,
        input,
        knowledge,
        tmp.path().to_path_buf(),
    );
    let result = RustAcceleratorCheckHandler.execute(&mut ctx);
    // Always skips context injection (logging only)
    assert_eq!(result.decision, crate::types::Decision::Skip);
}

// ── H68: PlanFactCheckerHandler ──────────────────────────────────

#[test]
fn test_plan_fact_checker_name_and_events() {
    let h = PlanFactCheckerHandler;
    assert_eq!(h.name(), "plan_fact_checker");
    assert_eq!(h.events(), &[HookEvent::PostToolUse]);
    assert_eq!(h.tool_matcher(), Some("Write|Edit|MultiEdit"));
    assert!(h.is_async());
}

#[test]
fn test_plan_fact_checker_skips_non_plan_md() {
    use std::sync::Arc;
    use tempfile::TempDir;
    use touring_hooks::knowledge::FileKnowledgeDB;

    let tmp = TempDir::new().unwrap();
    let db = FileKnowledgeDB::new(&tmp.path().join("k.db")).unwrap();
    #[allow(clippy::arc_with_non_send_sync)]
    let knowledge = Arc::new(db);
    let input = serde_json::json!({
        "tool_name": "Write",
        "tool_input": { "file_path": "/project/src/main.rs" }
    });
    let mut ctx = crate::context::CortexContext::from_input(
        HookEvent::PostToolUse,
        input,
        knowledge,
        tmp.path().to_path_buf(),
    );
    let result = PlanFactCheckerHandler.execute(&mut ctx);
    assert_eq!(result.decision, crate::types::Decision::Skip);
}

#[test]
fn test_plan_fact_checker_skips_md_not_in_plans() {
    use std::sync::Arc;
    use tempfile::TempDir;
    use touring_hooks::knowledge::FileKnowledgeDB;

    let tmp = TempDir::new().unwrap();
    let db = FileKnowledgeDB::new(&tmp.path().join("k.db")).unwrap();
    #[allow(clippy::arc_with_non_send_sync)]
    let knowledge = Arc::new(db);
    let input = serde_json::json!({
        "tool_name": "Write",
        "tool_input": { "file_path": "/project/README.md" }
    });
    let mut ctx = crate::context::CortexContext::from_input(
        HookEvent::PostToolUse,
        input,
        knowledge,
        tmp.path().to_path_buf(),
    );
    let result = PlanFactCheckerHandler.execute(&mut ctx);
    assert_eq!(result.decision, crate::types::Decision::Skip);
}

#[test]
fn test_plan_fact_checker_skips_nonexistent_plan_file() {
    use std::sync::Arc;
    use tempfile::TempDir;
    use touring_hooks::knowledge::FileKnowledgeDB;

    let tmp = TempDir::new().unwrap();
    let db = FileKnowledgeDB::new(&tmp.path().join("k.db")).unwrap();
    #[allow(clippy::arc_with_non_send_sync)]
    let knowledge = Arc::new(db);
    let input = serde_json::json!({
        "tool_name": "Write",
        "tool_input": { "file_path": "/project/plans/my_plan.md" }
    });
    let mut ctx = crate::context::CortexContext::from_input(
        HookEvent::PostToolUse,
        input,
        knowledge,
        tmp.path().to_path_buf(),
    );
    // File doesn't exist → can't read → skip
    let result = PlanFactCheckerHandler.execute(&mut ctx);
    assert_eq!(result.decision, crate::types::Decision::Skip);
}

#[test]
fn test_plan_fact_checker_allows_with_missing_paths() {
    use std::sync::Arc;
    use tempfile::TempDir;
    use touring_hooks::knowledge::FileKnowledgeDB;

    let tmp = TempDir::new().unwrap();
    let db = FileKnowledgeDB::new(&tmp.path().join("k.db")).unwrap();
    #[allow(clippy::arc_with_non_send_sync)]
    let knowledge = Arc::new(db);

    // Create a plans directory and plan file referencing a non-existent path
    let plans_dir = tmp.path().join("plans");
    std::fs::create_dir_all(&plans_dir).unwrap();
    let plan_file = plans_dir.join("todo.md");
    std::fs::write(
        &plan_file,
        "See `.claude/data/nonexistent_file.json` for details.",
    )
    .unwrap();

    let input = serde_json::json!({
        "tool_name": "Write",
        "tool_input": { "file_path": plan_file.to_string_lossy() }
    });
    let mut ctx = crate::context::CortexContext::from_input(
        HookEvent::PostToolUse,
        input,
        knowledge,
        tmp.path().to_path_buf(),
    );
    let result = PlanFactCheckerHandler.execute(&mut ctx);
    // Found missing paths → Allow with context
    assert_eq!(result.decision, crate::types::Decision::Allow);
    assert!(!result.context_lines.is_empty());
    assert!(result.context_lines[0].contains("PlanFactCheck"));
}

// ── H69: LearningSessionStartupHandler ───────────────────────────

#[test]
fn test_learning_session_startup_name_and_events() {
    let h = LearningSessionStartupHandler;
    assert_eq!(h.name(), "learning_session_startup");
    assert_eq!(h.events(), &[HookEvent::SessionStart]);
    assert!(!h.is_async());
}

#[test]
fn test_learning_session_startup_skips_low_budget() {
    use std::sync::Arc;
    use tempfile::TempDir;
    use touring_hooks::knowledge::FileKnowledgeDB;

    let tmp = TempDir::new().unwrap();
    let db = FileKnowledgeDB::new(&tmp.path().join("k.db")).unwrap();
    #[allow(clippy::arc_with_non_send_sync)]
    let knowledge = Arc::new(db);
    let input = serde_json::json!({});
    let mut ctx = crate::context::CortexContext::from_input(
        HookEvent::SessionStart,
        input,
        knowledge,
        tmp.path().to_path_buf(),
    );
    ctx.context_budget_remaining = 100;
    let result = LearningSessionStartupHandler.execute(&mut ctx);
    assert_eq!(result.decision, crate::types::Decision::Skip);
}

#[test]
fn test_learning_session_startup_skips_when_no_persistence() {
    use std::sync::Arc;
    use tempfile::TempDir;
    use touring_hooks::knowledge::FileKnowledgeDB;

    let tmp = TempDir::new().unwrap();
    let db = FileKnowledgeDB::new(&tmp.path().join("k.db")).unwrap();
    #[allow(clippy::arc_with_non_send_sync)]
    let knowledge = Arc::new(db);
    let input = serde_json::json!({});
    let mut ctx = crate::context::CortexContext::from_input(
        HookEvent::SessionStart,
        input,
        knowledge,
        tmp.path().to_path_buf(),
    );
    // No persistence, no rlm → empty parts → skip
    let result = LearningSessionStartupHandler.execute(&mut ctx);
    assert_eq!(result.decision, crate::types::Decision::Skip);
}

// ── H70: CodeFirstPipelineGuardHandler ───────────────────────────

#[test]
fn test_code_first_pipeline_guard_name_and_events() {
    let h = CodeFirstPipelineGuardHandler;
    assert_eq!(h.name(), "code_first_pipeline_guard");
    assert_eq!(h.events(), &[HookEvent::PreToolUse]);
    assert_eq!(h.tool_matcher(), Some("Skill|Task"));
    assert!(!h.is_async());
}

#[test]
fn test_code_first_pipeline_guard_skips_non_antt_skill() {
    use std::sync::Arc;
    use tempfile::TempDir;
    use touring_hooks::knowledge::FileKnowledgeDB;

    let tmp = TempDir::new().unwrap();
    let db = FileKnowledgeDB::new(&tmp.path().join("k.db")).unwrap();
    #[allow(clippy::arc_with_non_send_sync)]
    let knowledge = Arc::new(db);
    let input = serde_json::json!({
        "tool_name": "Skill",
        "tool_input": { "skill": "general-research" }
    });
    let mut ctx = crate::context::CortexContext::from_input(
        HookEvent::PreToolUse,
        input,
        knowledge,
        tmp.path().to_path_buf(),
    );
    let result = CodeFirstPipelineGuardHandler.execute(&mut ctx);
    assert_eq!(result.decision, crate::types::Decision::Skip);
}

#[test]
fn test_code_first_pipeline_guard_skips_empty_skill_name() {
    use std::sync::Arc;
    use tempfile::TempDir;
    use touring_hooks::knowledge::FileKnowledgeDB;

    let tmp = TempDir::new().unwrap();
    let db = FileKnowledgeDB::new(&tmp.path().join("k.db")).unwrap();
    #[allow(clippy::arc_with_non_send_sync)]
    let knowledge = Arc::new(db);
    let input = serde_json::json!({
        "tool_name": "Skill",
        "tool_input": {}
    });
    let mut ctx = crate::context::CortexContext::from_input(
        HookEvent::PreToolUse,
        input,
        knowledge,
        tmp.path().to_path_buf(),
    );
    let result = CodeFirstPipelineGuardHandler.execute(&mut ctx);
    assert_eq!(result.decision, crate::types::Decision::Skip);
}

#[test]
fn test_code_first_pipeline_guard_warns_when_no_relatoria_dir() {
    use std::sync::Arc;
    use tempfile::TempDir;
    use touring_hooks::knowledge::FileKnowledgeDB;

    let tmp = TempDir::new().unwrap();
    let db = FileKnowledgeDB::new(&tmp.path().join("k.db")).unwrap();
    #[allow(clippy::arc_with_non_send_sync)]
    let knowledge = Arc::new(db);
    let input = serde_json::json!({
        "tool_name": "Skill",
        "tool_input": { "skill": "antt-legal-analyzer" }
    });
    let mut ctx = crate::context::CortexContext::from_input(
        HookEvent::PreToolUse,
        input,
        knowledge,
        tmp.path().to_path_buf(),
    );
    let result = CodeFirstPipelineGuardHandler.execute(&mut ctx);
    // No relatoria dir → warn (Allow) but don't block
    assert_eq!(result.decision, crate::types::Decision::Allow);
    assert!(!result.context_lines.is_empty());
    assert!(result.context_lines[0].contains("CODE-FIRST WARN"));
}

#[test]
fn test_code_first_pipeline_guard_blocks_when_no_pipeline_state() {
    use std::sync::Arc;
    use tempfile::TempDir;
    use touring_hooks::knowledge::FileKnowledgeDB;

    let tmp = TempDir::new().unwrap();
    let db = FileKnowledgeDB::new(&tmp.path().join("k.db")).unwrap();
    #[allow(clippy::arc_with_non_send_sync)]
    let knowledge = Arc::new(db);

    // Create relatoria dir but no pipeline_state file
    let process_dir = tmp.path().join("analise/relatoria/processo_001");
    std::fs::create_dir_all(&process_dir).unwrap();
    // Create a non-pipeline-state file to ensure dir exists but no match
    std::fs::write(process_dir.join("notes.txt"), "notes").unwrap();

    let input = serde_json::json!({
        "tool_name": "Skill",
        "tool_input": { "skill": "antt-technical-analyzer" }
    });
    let mut ctx = crate::context::CortexContext::from_input(
        HookEvent::PreToolUse,
        input,
        knowledge,
        tmp.path().to_path_buf(),
    );
    let result = CodeFirstPipelineGuardHandler.execute(&mut ctx);
    // No pipeline_state file → Block
    assert!(matches!(result.decision, crate::types::Decision::Block(_)));
    if let crate::types::Decision::Block(ref reason) = result.decision {
        assert!(reason.contains("CODE-FIRST BLOCK"));
    }
}

#[test]
fn test_antt_skills_list_coverage() {
    // Verify key ANTT skills are covered
    assert!(ANTT_SKILLS.contains(&"antt-legal-analyzer"));
    assert!(ANTT_SKILLS.contains(&"antt-technical-analyzer"));
    assert!(ANTT_SKILLS.contains(&"antt-final-synthesizer"));
    assert!(ANTT_SKILLS.contains(&"antt-vote-architect"));
}

// ── H71: CipherKnowledgeRetrievalHandler ─────────────────────────

#[test]
fn test_cipher_knowledge_retrieval_name_and_events() {
    let h = CipherKnowledgeRetrievalHandler;
    assert_eq!(h.name(), "cipher_knowledge_retrieval");
    assert_eq!(h.events(), &[HookEvent::PreToolUse]);
    assert_eq!(h.tool_matcher(), Some("Read|Write|Edit|Grep|Glob"));
    assert!(h.is_async());
}

#[test]
fn test_cipher_knowledge_retrieval_skips_no_file_path() {
    use std::sync::Arc;
    use tempfile::TempDir;
    use touring_hooks::knowledge::FileKnowledgeDB;

    let tmp = TempDir::new().unwrap();
    let db = FileKnowledgeDB::new(&tmp.path().join("k.db")).unwrap();
    #[allow(clippy::arc_with_non_send_sync)]
    let knowledge = Arc::new(db);
    let input = serde_json::json!({ "tool_name": "Read" });
    let mut ctx = crate::context::CortexContext::from_input(
        HookEvent::PreToolUse,
        input,
        knowledge,
        tmp.path().to_path_buf(),
    );
    let result = CipherKnowledgeRetrievalHandler.execute(&mut ctx);
    assert_eq!(result.decision, crate::types::Decision::Skip);
}

#[test]
fn test_cipher_knowledge_retrieval_skips_low_budget() {
    use std::sync::Arc;
    use tempfile::TempDir;
    use touring_hooks::knowledge::FileKnowledgeDB;

    let tmp = TempDir::new().unwrap();
    let db = FileKnowledgeDB::new(&tmp.path().join("k.db")).unwrap();
    #[allow(clippy::arc_with_non_send_sync)]
    let knowledge = Arc::new(db);
    let input = serde_json::json!({
        "tool_name": "Read",
        "tool_input": { "file_path": "/project/src/main.rs" }
    });
    let mut ctx = crate::context::CortexContext::from_input(
        HookEvent::PreToolUse,
        input,
        knowledge,
        tmp.path().to_path_buf(),
    );
    ctx.context_budget_remaining = 50;
    let result = CipherKnowledgeRetrievalHandler.execute(&mut ctx);
    assert_eq!(result.decision, crate::types::Decision::Skip);
}

#[test]
fn test_cipher_knowledge_retrieval_skips_no_cipher_dir() {
    use std::sync::Arc;
    use tempfile::TempDir;
    use touring_hooks::knowledge::FileKnowledgeDB;

    let tmp = TempDir::new().unwrap();
    let db = FileKnowledgeDB::new(&tmp.path().join("k.db")).unwrap();
    #[allow(clippy::arc_with_non_send_sync)]
    let knowledge = Arc::new(db);
    let file_path = tmp.path().join("src/main.rs");
    let input = serde_json::json!({
        "tool_name": "Read",
        "tool_input": { "file_path": file_path.to_string_lossy() }
    });
    let mut ctx = crate::context::CortexContext::from_input(
        HookEvent::PreToolUse,
        input,
        knowledge,
        tmp.path().to_path_buf(),
    );
    // No .cipher/patterns dir → skip
    let result = CipherKnowledgeRetrievalHandler.execute(&mut ctx);
    assert_eq!(result.decision, crate::types::Decision::Skip);
}

// ── H72: CipherKnowledgeCaptureHandler ───────────────────────────

#[test]
fn test_cipher_knowledge_capture_name_and_events() {
    let h = CipherKnowledgeCaptureHandler;
    assert_eq!(h.name(), "cipher_knowledge_capture");
    assert_eq!(h.events(), &[HookEvent::PostToolUse]);
    assert_eq!(h.tool_matcher(), Some("Write|Edit|MultiEdit"));
    assert!(h.is_async());
}

#[test]
fn test_cipher_knowledge_capture_skips_non_tracked_ext() {
    use std::sync::Arc;
    use tempfile::TempDir;
    use touring_hooks::knowledge::FileKnowledgeDB;

    let tmp = TempDir::new().unwrap();
    let db = FileKnowledgeDB::new(&tmp.path().join("k.db")).unwrap();
    #[allow(clippy::arc_with_non_send_sync)]
    let knowledge = Arc::new(db);
    let input = serde_json::json!({
        "tool_name": "Write",
        "tool_input": { "file_path": "/project/config.toml" }
    });
    let mut ctx = crate::context::CortexContext::from_input(
        HookEvent::PostToolUse,
        input,
        knowledge,
        tmp.path().to_path_buf(),
    );
    let result = CipherKnowledgeCaptureHandler.execute(&mut ctx);
    assert_eq!(result.decision, crate::types::Decision::Skip);
}

#[test]
fn test_cipher_knowledge_capture_skips_no_file_path() {
    use std::sync::Arc;
    use tempfile::TempDir;
    use touring_hooks::knowledge::FileKnowledgeDB;

    let tmp = TempDir::new().unwrap();
    let db = FileKnowledgeDB::new(&tmp.path().join("k.db")).unwrap();
    #[allow(clippy::arc_with_non_send_sync)]
    let knowledge = Arc::new(db);
    let input = serde_json::json!({ "tool_name": "Write" });
    let mut ctx = crate::context::CortexContext::from_input(
        HookEvent::PostToolUse,
        input,
        knowledge,
        tmp.path().to_path_buf(),
    );
    let result = CipherKnowledgeCaptureHandler.execute(&mut ctx);
    assert_eq!(result.decision, crate::types::Decision::Skip);
}

#[test]
fn test_cipher_knowledge_capture_handles_rs_file() {
    use std::sync::Arc;
    use tempfile::TempDir;
    use touring_hooks::knowledge::FileKnowledgeDB;

    let tmp = TempDir::new().unwrap();
    let db = FileKnowledgeDB::new(&tmp.path().join("k.db")).unwrap();
    #[allow(clippy::arc_with_non_send_sync)]
    let knowledge = Arc::new(db);
    let input = serde_json::json!({
        "tool_name": "Write",
        "tool_input": { "file_path": "/project/src/lib.rs" }
    });
    let mut ctx = crate::context::CortexContext::from_input(
        HookEvent::PostToolUse,
        input,
        knowledge,
        tmp.path().to_path_buf(),
    );
    // .rs is tracked → capture only (skip context injection)
    let result = CipherKnowledgeCaptureHandler.execute(&mut ctx);
    assert_eq!(result.decision, crate::types::Decision::Skip);
}

// ── H73: CipherAgentOutputCaptureHandler ─────────────────────────

#[test]
fn test_cipher_agent_output_capture_name_and_events() {
    let h = CipherAgentOutputCaptureHandler;
    assert_eq!(h.name(), "cipher_agent_output_capture");
    assert_eq!(h.events(), &[HookEvent::SubagentStop]);
    assert!(h.is_async());
}

#[test]
fn test_cipher_agent_output_capture_skips_empty_output() {
    use std::sync::Arc;
    use tempfile::TempDir;
    use touring_hooks::knowledge::FileKnowledgeDB;

    let tmp = TempDir::new().unwrap();
    let db = FileKnowledgeDB::new(&tmp.path().join("k.db")).unwrap();
    #[allow(clippy::arc_with_non_send_sync)]
    let knowledge = Arc::new(db);
    let input = serde_json::json!({
        "subagent_type": "antt-legal-analyzer",
        "output": ""
    });
    let mut ctx = crate::context::CortexContext::from_input(
        HookEvent::SubagentStop,
        input,
        knowledge,
        tmp.path().to_path_buf(),
    );
    let result = CipherAgentOutputCaptureHandler.execute(&mut ctx);
    assert_eq!(result.decision, crate::types::Decision::Skip);
}

#[test]
fn test_cipher_agent_output_capture_skips_short_output() {
    use std::sync::Arc;
    use tempfile::TempDir;
    use touring_hooks::knowledge::FileKnowledgeDB;

    let tmp = TempDir::new().unwrap();
    let db = FileKnowledgeDB::new(&tmp.path().join("k.db")).unwrap();
    #[allow(clippy::arc_with_non_send_sync)]
    let knowledge = Arc::new(db);
    let input = serde_json::json!({
        "subagent_type": "antt-legal-analyzer",
        "output": "short"
    });
    let mut ctx = crate::context::CortexContext::from_input(
        HookEvent::SubagentStop,
        input,
        knowledge,
        tmp.path().to_path_buf(),
    );
    let result = CipherAgentOutputCaptureHandler.execute(&mut ctx);
    assert_eq!(result.decision, crate::types::Decision::Skip);
}

#[test]
fn test_cipher_agent_output_capture_processes_valid_output() {
    use std::sync::Arc;
    use tempfile::TempDir;
    use touring_hooks::knowledge::FileKnowledgeDB;

    let tmp = TempDir::new().unwrap();
    let db = FileKnowledgeDB::new(&tmp.path().join("k.db")).unwrap();
    #[allow(clippy::arc_with_non_send_sync)]
    let knowledge = Arc::new(db);
    let output = "✅ Analysis completed successfully. Found 15 legal precedents. Generated detailed report with 3 sections.";
    let input = serde_json::json!({
        "subagent_type": "antt-legal-analyzer",
        "output": output
    });
    let mut ctx = crate::context::CortexContext::from_input(
        HookEvent::SubagentStop,
        input,
        knowledge,
        tmp.path().to_path_buf(),
    );
    // Capture only → always Skip (no context injection)
    let result = CipherAgentOutputCaptureHandler.execute(&mut ctx);
    assert_eq!(result.decision, crate::types::Decision::Skip);
}

// ── H74: ObserverLearningLoopHandler ─────────────────────────────

#[test]
fn test_observer_learning_loop_name_and_events() {
    let h = ObserverLearningLoopHandler;
    assert_eq!(h.name(), "observer_learning_loop");
    assert_eq!(h.events(), &[HookEvent::Stop]);
    assert!(h.is_async());
}

#[test]
fn test_observer_learning_loop_skips_context_injection() {
    use std::sync::Arc;
    use tempfile::TempDir;
    use touring_hooks::knowledge::FileKnowledgeDB;

    let tmp = TempDir::new().unwrap();
    let db = FileKnowledgeDB::new(&tmp.path().join("k.db")).unwrap();
    #[allow(clippy::arc_with_non_send_sync)]
    let knowledge = Arc::new(db);
    let input = serde_json::json!({
        "usage": {
            "input_tokens": 50000,
            "output_tokens": 10000,
            "cache_read_input_tokens": 20000
        },
        "turn_count": 5
    });
    let mut ctx = crate::context::CortexContext::from_input(
        HookEvent::Stop,
        input,
        knowledge,
        tmp.path().to_path_buf(),
    );
    let result = ObserverLearningLoopHandler.execute(&mut ctx);
    // Learning only, no context injection → always Skip
    assert_eq!(result.decision, crate::types::Decision::Skip);
}

#[test]
fn test_observer_learning_loop_handles_no_usage() {
    use std::sync::Arc;
    use tempfile::TempDir;
    use touring_hooks::knowledge::FileKnowledgeDB;

    let tmp = TempDir::new().unwrap();
    let db = FileKnowledgeDB::new(&tmp.path().join("k.db")).unwrap();
    #[allow(clippy::arc_with_non_send_sync)]
    let knowledge = Arc::new(db);
    let input = serde_json::json!({});
    let mut ctx = crate::context::CortexContext::from_input(
        HookEvent::Stop,
        input,
        knowledge,
        tmp.path().to_path_buf(),
    );
    let result = ObserverLearningLoopHandler.execute(&mut ctx);
    assert_eq!(result.decision, crate::types::Decision::Skip);
}

#[test]
fn test_observer_learning_loop_handles_high_turn_count() {
    use std::sync::Arc;
    use tempfile::TempDir;
    use touring_hooks::knowledge::FileKnowledgeDB;

    let tmp = TempDir::new().unwrap();
    let db = FileKnowledgeDB::new(&tmp.path().join("k.db")).unwrap();
    #[allow(clippy::arc_with_non_send_sync)]
    let knowledge = Arc::new(db);
    let input = serde_json::json!({ "turn_count": 25 });
    let mut ctx = crate::context::CortexContext::from_input(
        HookEvent::Stop,
        input,
        knowledge,
        tmp.path().to_path_buf(),
    );
    // High turn count (retry storm) still just Skip for context
    let result = ObserverLearningLoopHandler.execute(&mut ctx);
    assert_eq!(result.decision, crate::types::Decision::Skip);
}

// ── Registration ─────────────────────────────────────────────────

#[test]
fn test_register_all_enrichment_handlers() {
    let mut pipeline = Pipeline::new();
    register(&mut pipeline);
    // 15 handlers: H60-H74
    assert_eq!(pipeline.handler_count(), 15);
}
