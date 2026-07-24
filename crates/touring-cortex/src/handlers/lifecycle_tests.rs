use super::*;
use crate::context::CortexContext;
use crate::types::Decision;
use std::sync::Arc;
use tempfile::TempDir;
use touring_hooks::knowledge::FileKnowledgeDB;

#[allow(clippy::arc_with_non_send_sync)] // single-threaded test context
fn make_test_ctx(event: HookEvent, input: serde_json::Value) -> (TempDir, CortexContext) {
    let tmp = TempDir::new().unwrap();
    let db = FileKnowledgeDB::new(&tmp.path().join("test.db")).unwrap();
    let knowledge = Arc::new(db);
    let ctx = CortexContext::from_input(event, input, knowledge, tmp.path().to_path_buf());
    (tmp, ctx)
}

// ── PreCompactHandler ───────────────────────────────────────────

#[test]
fn test_pre_compact_injects_crystal() {
    let input = serde_json::json!({"session_id": "test-session"});
    let (_tmp, mut ctx) = make_test_ctx(HookEvent::PreCompact, input);

    // Add some knowledge first
    ctx.knowledge
        .upsert(&touring_hooks::knowledge::FileKnowledge {
            file_path: "src/main.py".to_string(),
            ..Default::default()
        })
        .unwrap();

    let handler = PreCompactHandler;
    let result = handler.execute(&mut ctx);
    assert!(matches!(result.decision, Decision::Allow));
    assert!(!result.context_lines.is_empty());
    let crystal = &result.context_lines[0];
    assert!(crystal.contains("PRE-COMPACT CRYSTAL"));
    assert!(crystal.contains("CODE-FIRST"));
    assert!(crystal.contains("files:1"));
}

#[test]
fn test_pre_compact_records_access() {
    let input = serde_json::json!({"session_id": "sess-123"});
    let (_tmp, mut ctx) = make_test_ctx(HookEvent::PreCompact, input);
    let handler = PreCompactHandler;
    let _ = handler.execute(&mut ctx);
    let count = ctx
        .knowledge
        .access_count("__pre_compact_crystal__")
        .unwrap();
    assert_eq!(count, 1);
}

#[test]
fn test_pre_compact_empty_knowledge() {
    let input = serde_json::json!({"session_id": "test-session"});
    let (_tmp, mut ctx) = make_test_ctx(HookEvent::PreCompact, input);
    let handler = PreCompactHandler;
    let result = handler.execute(&mut ctx);
    // Even with empty knowledge, should inject critical rules
    assert!(matches!(result.decision, Decision::Allow));
    assert!(!result.context_lines.is_empty());
    assert!(result.context_lines[0].contains("CODE-FIRST"));
}

// ── SubagentStartHandler ────────────────────────────────────────

#[test]
fn test_subagent_start_with_knowledge() {
    let input = serde_json::json!({
        "session_id": "s1",
        "subagent_id": "atlas",
        "task_description": "analyze pipeline files"
    });
    let (_tmp, mut ctx) = make_test_ctx(HookEvent::SubagentStart, input);

    ctx.knowledge
        .upsert(&touring_hooks::knowledge::FileKnowledge {
            file_path: "pipeline.py".to_string(),
            ..Default::default()
        })
        .unwrap();

    let handler = SubagentStartHandler;
    let result = handler.execute(&mut ctx);
    assert!(matches!(result.decision, Decision::Allow));
    assert!(!result.context_lines.is_empty());
    assert!(result.context_lines[0].contains("atlas started"));
    assert!(result.context_lines[0].contains("CODE-FIRST"));
}

#[test]
fn test_subagent_start_empty_task() {
    let input = serde_json::json!({
        "session_id": "s1",
        "subagent_id": "sub1",
        "task_description": ""
    });
    let (_tmp, mut ctx) = make_test_ctx(HookEvent::SubagentStart, input);
    let handler = SubagentStartHandler;
    let result = handler.execute(&mut ctx);
    assert!(matches!(result.decision, Decision::Skip));
}

#[test]
fn test_subagent_start_records_access() {
    let input = serde_json::json!({
        "session_id": "s1",
        "subagent_id": "sub1",
        "task_description": "run tests"
    });
    let (_tmp, mut ctx) = make_test_ctx(HookEvent::SubagentStart, input);
    let handler = SubagentStartHandler;
    let _ = handler.execute(&mut ctx);
    let count = ctx
        .knowledge
        .access_count("__subagent_start:sub1__")
        .unwrap();
    assert_eq!(count, 1);
}

// ── SubagentStopHandler ─────────────────────────────────────────

#[test]
fn test_subagent_stop_records_success() {
    let input = serde_json::json!({
        "session_id": "s1",
        "subagent_id": "atlas",
        "success": true,
        "result_summary": "all tests passed"
    });
    let (_tmp, mut ctx) = make_test_ctx(HookEvent::SubagentStop, input);
    let handler = SubagentStopHandler;
    let result = handler.execute(&mut ctx);
    assert!(matches!(result.decision, Decision::Skip)); // Async learning
    // Verify it recorded
    let count = ctx
        .knowledge
        .access_count("__subagent_stop:atlas:SUCCESS__")
        .unwrap();
    assert_eq!(count, 1);
}

#[test]
fn test_subagent_stop_records_failure() {
    let input = serde_json::json!({
        "session_id": "s1",
        "subagent_id": "broken",
        "success": false,
        "result_summary": "ruff errors found"
    });
    let (_tmp, mut ctx) = make_test_ctx(HookEvent::SubagentStop, input);
    let handler = SubagentStopHandler;
    let result = handler.execute(&mut ctx);
    assert!(matches!(result.decision, Decision::Skip));
    let count = ctx
        .knowledge
        .access_count("__subagent_stop:broken:FAIL__")
        .unwrap();
    assert_eq!(count, 1);
}

#[test]
fn test_subagent_stop_is_async() {
    assert!(SubagentStopHandler.is_async());
}

// ── TeammateIdleHandler ─────────────────────────────────────────

#[test]
fn test_teammate_idle_allows_quality_pass() {
    let input = serde_json::json!({
        "session_id": "s1",
        "teammate_id": "atlas",
        "quality_passed": true,
        "checkpoint_written": true
    });
    let (_tmp, mut ctx) = make_test_ctx(HookEvent::TeammateIdle, input);
    let handler = TeammateIdleHandler;
    let result = handler.execute(&mut ctx);
    assert!(matches!(result.decision, Decision::Skip));
}

#[test]
fn test_teammate_idle_blocks_quality_fail() {
    let input = serde_json::json!({
        "session_id": "s1",
        "teammate_id": "themis",
        "quality_passed": false,
        "checkpoint_written": false
    });
    let (_tmp, mut ctx) = make_test_ctx(HookEvent::TeammateIdle, input);
    let handler = TeammateIdleHandler;
    let result = handler.execute(&mut ctx);
    assert!(
        matches!(result.decision, Decision::Block(ref r) if r.contains("TEAMMATE_QUALITY_GATE"))
    );
}

#[test]
fn test_teammate_idle_block_includes_teammate_id() {
    let input = serde_json::json!({
        "session_id": "s1",
        "teammate_id": "praetor",
        "quality_passed": false,
        "checkpoint_written": true
    });
    let (_tmp, mut ctx) = make_test_ctx(HookEvent::TeammateIdle, input);
    let handler = TeammateIdleHandler;
    let result = handler.execute(&mut ctx);
    assert!(matches!(result.decision, Decision::Block(ref r) if r.contains("praetor")));
}

#[test]
fn test_teammate_idle_records_no_checkpoint() {
    let input = serde_json::json!({
        "session_id": "s1",
        "teammate_id": "argus",
        "quality_passed": true,
        "checkpoint_written": false
    });
    let (_tmp, mut ctx) = make_test_ctx(HookEvent::TeammateIdle, input);
    let handler = TeammateIdleHandler;
    let _ = handler.execute(&mut ctx);
    let count = ctx
        .knowledge
        .access_count("__teammate_no_checkpoint:argus__")
        .unwrap();
    assert_eq!(count, 1);
}

#[test]
fn test_teammate_idle_fail_open_missing_fields() {
    // Missing quality_passed field — should default to true (fail-open)
    let input = serde_json::json!({
        "session_id": "s1",
        "teammate_id": "unknown"
    });
    let (_tmp, mut ctx) = make_test_ctx(HookEvent::TeammateIdle, input);
    let handler = TeammateIdleHandler;
    let result = handler.execute(&mut ctx);
    assert!(matches!(result.decision, Decision::Skip));
}

// ── TaskCompletedHandler ────────────────────────────────────────

#[test]
fn test_task_completed_records_success() {
    let input = serde_json::json!({
        "session_id": "s1",
        "task_id": "task_042",
        "success": true,
        "output_summary": "pipeline complete"
    });
    let (_tmp, mut ctx) = make_test_ctx(HookEvent::TaskCompleted, input);
    let handler = TaskCompletedHandler;
    let result = handler.execute(&mut ctx);
    assert!(matches!(result.decision, Decision::Skip)); // Async learning
    let count = ctx
        .knowledge
        .access_count("__task_completed:task_042:COMPLETED__")
        .unwrap();
    assert_eq!(count, 1);
}

#[test]
fn test_task_completed_records_failure() {
    let input = serde_json::json!({
        "session_id": "s1",
        "task_id": "task_bad",
        "success": false,
        "output_summary": "ruff errors"
    });
    let (_tmp, mut ctx) = make_test_ctx(HookEvent::TaskCompleted, input);
    let handler = TaskCompletedHandler;
    let result = handler.execute(&mut ctx);
    assert!(matches!(result.decision, Decision::Skip));
    let count = ctx
        .knowledge
        .access_count("__task_completed:task_bad:FAILED__")
        .unwrap();
    assert_eq!(count, 1);
}

#[test]
fn test_task_completed_stores_bash_outcome() {
    let input = serde_json::json!({
        "session_id": "s1",
        "task_id": "task_x",
        "success": false,
        "output_summary": "compilation error on line 42"
    });
    let (_tmp, mut ctx) = make_test_ctx(HookEvent::TaskCompleted, input);
    let handler = TaskCompletedHandler;
    let _ = handler.execute(&mut ctx);

    // Verify the bash outcome was recorded
    let outcomes = ctx.knowledge.find_bash_outcomes("task:task_x", 5).unwrap();
    assert_eq!(outcomes.len(), 1);
    assert!(!outcomes[0].success);
    assert!(
        outcomes[0]
            .error_pattern
            .as_ref()
            .unwrap()
            .contains("compilation error")
    );
}

#[test]
fn test_task_completed_is_async() {
    assert!(TaskCompletedHandler.is_async());
}

// ── PostCompactHandler ────────────────────────────────────────

#[test]
fn test_post_compact_emits_within_budget() {
    let input = serde_json::json!({"session_id": "test-session"});
    let (_tmp, mut ctx) = make_test_ctx(HookEvent::PostCompact, input);

    // Add knowledge so Tier 2 activates
    ctx.knowledge
        .upsert(&touring_hooks::knowledge::FileKnowledge {
            file_path: "src/main.py".to_string(),
            ..Default::default()
        })
        .unwrap();

    let handler = PostCompactHandler::new();
    let result = handler.execute(&mut ctx);

    assert!(matches!(result.decision, Decision::Allow));
    assert!(!result.context_lines.is_empty());

    // CRITICAL: output must never exceed 3000 chars
    let total_chars: usize = result.context_lines.iter().map(|l| l.len()).sum();
    assert!(
        total_chars <= 3000,
        "PostCompact output {} chars exceeds 3000 char budget",
        total_chars
    );
}

#[test]
fn test_post_compact_tier1_always_present() {
    let input = serde_json::json!({"session_id": "sess-abc"});
    let (_tmp, mut ctx) = make_test_ctx(HookEvent::PostCompact, input);

    let handler = PostCompactHandler::new();
    let result = handler.execute(&mut ctx);

    assert!(matches!(result.decision, Decision::Allow));
    let context = &result.context_lines[0];
    assert!(context.contains("POST-COMPACT RECOVERY"));
    assert!(context.contains("CODE-FIRST"));
    assert!(context.contains("CILA routing"));
    assert!(context.contains("Zero-hallucination"));
    assert!(context.contains("Hooks inviolable"));
}

#[test]
fn test_post_compact_tier2_includes_knowledge_stats() {
    let input = serde_json::json!({"session_id": "sess-t2"});
    let (_tmp, mut ctx) = make_test_ctx(HookEvent::PostCompact, input);

    // Add knowledge entries
    ctx.knowledge
        .upsert(&touring_hooks::knowledge::FileKnowledge {
            file_path: "a.py".to_string(),
            ..Default::default()
        })
        .unwrap();
    ctx.knowledge
        .upsert(&touring_hooks::knowledge::FileKnowledge {
            file_path: "b.py".to_string(),
            ..Default::default()
        })
        .unwrap();

    let handler = PostCompactHandler::new();
    let result = handler.execute(&mut ctx);

    let context = &result.context_lines[0];
    assert!(
        context.contains("Knowledge: files=2"),
        "Tier 2 should include file count, got: {}",
        context
    );
}

#[test]
fn test_post_compact_anti_flood_skips_after_threshold() {
    let input = serde_json::json!({"session_id": "flood-test"});

    let handler = PostCompactHandler::new();

    // Fire ANTI_FLOOD_MAX_COMPACTIONS times — all should produce output
    for i in 0..5 {
        let (_tmp, mut ctx) = make_test_ctx(HookEvent::PostCompact, input.clone());
        let result = handler.execute(&mut ctx);
        assert!(
            matches!(result.decision, Decision::Allow),
            "Compaction {} should Allow",
            i
        );
    }

    // 6th compaction within window — should be skipped (anti-flood)
    let (_tmp, mut ctx) = make_test_ctx(HookEvent::PostCompact, input.clone());
    let result = handler.execute(&mut ctx);
    assert!(
        matches!(result.decision, Decision::Skip),
        "6th compaction should Skip due to anti-flood"
    );
}

#[test]
fn test_post_compact_records_access() {
    let input = serde_json::json!({"session_id": "access-test"});
    let (_tmp, mut ctx) = make_test_ctx(HookEvent::PostCompact, input);

    let handler = PostCompactHandler::new();
    let _ = handler.execute(&mut ctx);

    let count = ctx
        .knowledge
        .access_count("__post_compact_reinjection__")
        .unwrap();
    assert_eq!(count, 1);
}

// ── ConfigChangeHandler ─────────────────────────────────────────

#[test]
fn test_config_change_local_settings() {
    let input = serde_json::json!({
        "session_id": "s1",
        "source": "local_settings"
    });
    let (_tmp, mut ctx) = make_test_ctx(HookEvent::ConfigChange, input);

    let handler = ConfigChangeHandler;
    let result = handler.execute(&mut ctx);

    assert!(matches!(result.decision, Decision::Allow));
    assert!(!result.context_lines.is_empty());
    assert!(result.context_lines[0].contains("Config changed"));
    assert!(result.context_lines[0].contains("local_settings"));
}

#[test]
fn test_config_change_project_settings() {
    let input = serde_json::json!({
        "session_id": "s1",
        "source": "project_settings"
    });
    let (_tmp, mut ctx) = make_test_ctx(HookEvent::ConfigChange, input);

    let handler = ConfigChangeHandler;
    let result = handler.execute(&mut ctx);

    assert!(matches!(result.decision, Decision::Allow));
    assert!(result.context_lines[0].contains("project_settings"));
}

#[test]
fn test_config_change_unknown_source_skips() {
    let input = serde_json::json!({
        "session_id": "s1",
        "source": "unknown_source"
    });
    let (_tmp, mut ctx) = make_test_ctx(HookEvent::ConfigChange, input);

    let handler = ConfigChangeHandler;
    let result = handler.execute(&mut ctx);

    assert!(matches!(result.decision, Decision::Skip));
}

#[test]
fn test_config_change_missing_source_skips() {
    let input = serde_json::json!({
        "session_id": "s1"
    });
    let (_tmp, mut ctx) = make_test_ctx(HookEvent::ConfigChange, input);

    let handler = ConfigChangeHandler;
    let result = handler.execute(&mut ctx);

    assert!(matches!(result.decision, Decision::Skip));
}

#[test]
fn test_config_change_records_access() {
    let input = serde_json::json!({
        "session_id": "s1",
        "source": "local_settings"
    });
    let (_tmp, mut ctx) = make_test_ctx(HookEvent::ConfigChange, input);

    let handler = ConfigChangeHandler;
    let _ = handler.execute(&mut ctx);

    let count = ctx
        .knowledge
        .access_count("__config_change:local_settings__")
        .unwrap();
    assert_eq!(count, 1);
}

// ── WorktreeEnterHandler (Wave C Subtask 3) ─────────────────────

#[test]
fn test_worktree_enter_skips_empty_path() {
    let input = serde_json::json!({"worktree_path": ""});
    let (_tmp, mut ctx) = make_test_ctx(HookEvent::WorktreeCreate, input);
    let handler = WorktreeEnterHandler;
    let result = handler.execute(&mut ctx);
    assert_eq!(result.decision, Decision::Skip);
}

#[test]
fn test_worktree_enter_missing_path_skips() {
    let input = serde_json::json!({});
    let (_tmp, mut ctx) = make_test_ctx(HookEvent::WorktreeCreate, input);
    let handler = WorktreeEnterHandler;
    let result = handler.execute(&mut ctx);
    assert_eq!(result.decision, Decision::Skip);
}

#[test]
fn test_worktree_enter_returns_path_as_context() {
    let input = serde_json::json!({
        "worktree_path": "/tmp/test-worktree-xyz",
        "branch": "feature/test"
    });
    let (_tmp, mut ctx) = make_test_ctx(HookEvent::WorktreeCreate, input);
    let handler = WorktreeEnterHandler;
    let result = handler.execute(&mut ctx);
    // Should return Allow decision with worktree path as context
    assert_eq!(result.decision, Decision::Allow);
    assert!(
        result
            .context_lines
            .iter()
            .any(|l| l.contains("/tmp/test-worktree-xyz")),
        "context should contain worktree path, got: {:?}",
        result.context_lines
    );
}

#[test]
fn test_worktree_enter_allow_without_rlm() {
    // Without RLM, should still return Allow (memory store is best-effort)
    let input = serde_json::json!({
        "worktree_path": "/tmp/test-no-rlm",
        "branch": "main"
    });
    let (_tmp, mut ctx) = make_test_ctx(HookEvent::WorktreeCreate, input);
    // rlm is None by default in make_test_ctx
    assert!(ctx.rlm.is_none());
    let handler = WorktreeEnterHandler;
    let result = handler.execute(&mut ctx);
    assert_eq!(result.decision, Decision::Allow);
}

#[test]
fn test_worktree_enter_env_file_written_when_exists() {
    use std::io::Read;
    let tmp = tempfile::TempDir::new().expect("tmp dir");
    let env_file = tmp.path().join("claude.env");
    std::fs::write(&env_file, "").expect("create env file");

    // Set CLAUDE_ENV_FILE to our temp file
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::set_var("CLAUDE_ENV_FILE", env_file.to_str().expect("utf8 path")) };

    let input = serde_json::json!({
        "worktree_path": "/tmp/worktree-env-test",
        "branch": "main"
    });
    let (ctx_tmp, mut ctx) = make_test_ctx(HookEvent::WorktreeCreate, input);
    let handler = WorktreeEnterHandler;
    let _ = handler.execute(&mut ctx);

    // Read the env file and verify
    let mut content = String::new();
    std::fs::File::open(&env_file)
        .expect("open env file")
        .read_to_string(&mut content)
        .expect("read env file");

    assert!(
        content.contains("CLAUDE_PROJECT_DIR"),
        "env file should contain CLAUDE_PROJECT_DIR, got: {content}"
    );
    assert!(
        content.contains("/tmp/worktree-env-test"),
        "env file should contain worktree path, got: {content}"
    );

    // Cleanup env var to avoid test pollution
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::remove_var("CLAUDE_ENV_FILE") };
    drop(ctx_tmp);
}

// ── Registration ────────────────────────────────────────────────

#[test]
fn test_lifecycle_registration() {
    let mut pipeline = Pipeline::new();
    register(&mut pipeline);
    assert_eq!(pipeline.handler_count(), 21); // 15 original + 6 new (H77-H82)
}
