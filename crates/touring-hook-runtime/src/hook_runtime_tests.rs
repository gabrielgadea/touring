#![allow(clippy::indexing_slicing, clippy::len_zero)]
use super::*;
use tempfile::TempDir;
#[test]
fn test_runtime_init() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    std::fs::create_dir_all(root.join(".claude/data")).unwrap();
    let rt = HookRuntime::new(root);
    assert!(rt.is_ok());
}
#[test]
fn test_runtime_creates_data_dir() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    let rt = HookRuntime::new(root);
    assert!(rt.is_ok());
    assert!(root.join(".claude/touring/knowledge.db").exists());
}
#[test]
fn test_runtime_quality_tracking_lifecycle() {
    let tmp = TempDir::new().unwrap();
    let mut rt = HookRuntime::new(tmp.path()).unwrap();
    assert!(rt.ctx.quality_assessment.is_none());
    assert!(rt.quality_report(0).is_none());
    rt.reset_quality_tracking("test-session");
    assert!(rt.ctx.quality_assessment.is_some());
    rt.record_hook_outcome(HookOutcome {
        hook_name: "pre_read".into(),
        success: true,
        latency_ms: 5,
        context_injected: true,
        knowledge_captured: false,
        error: None,
    });
    rt.record_hook_outcome(HookOutcome {
        hook_name: "post_read".into(),
        success: true,
        latency_ms: 12,
        context_injected: false,
        knowledge_captured: true,
        error: None,
    });
    let report = rt.quality_report(1).unwrap();
    assert_eq!(report.iteration, 1);
    assert_eq!(report.dims.len(), 9);
    assert!(report.composite > 0.0);
}
#[test]
fn test_runtime_cache_integration() {
    let tmp = TempDir::new().unwrap();
    let rt = HookRuntime::new(tmp.path()).unwrap();
    assert!(rt.check_cache("pre_read", "foo.py").is_none());
    rt.store_cache("pre_read", "foo.py", r#"{"symbols":3}"#.into());
    assert_eq!(
        rt.check_cache("pre_read", "foo.py"),
        Some(r#"{"symbols":3}"#.into())
    );
    let count = rt.invalidate_cache_for_file("foo.py");
    assert_eq!(count, 1);
    assert!(rt.check_cache("pre_read", "foo.py").is_none());
}
#[test]
fn test_runtime_record_without_tracking_is_noop() {
    let tmp = TempDir::new().unwrap();
    let mut rt = HookRuntime::new(tmp.path()).unwrap();
    rt.record_hook_outcome(HookOutcome {
        hook_name: "pre_read".into(),
        success: true,
        latency_ms: 5,
        context_injected: false,
        knowledge_captured: false,
        error: None,
    });
    assert!(rt.ctx.quality_assessment.is_none());
}
#[test]
fn test_hook_response_allow() {
    let resp = HookRuntime::build_allow();
    assert_eq!(resp, HookResponse::Allow);
    assert_eq!(resp.to_json(), "{}");
}
#[test]
fn test_hook_response_context() {
    let resp = HookRuntime::build_context("test context");
    match &resp {
        HookResponse::Context {
            context,
            event_name,
        } => {
            assert_eq!(context, "test context");
            assert!(event_name.is_none());
        }
        _ => panic!("Expected Context variant"),
    }
    let json = resp.to_json();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(
        parsed["hookSpecificOutput"]["additionalContext"],
        "test context"
    );
    assert!(parsed.get("suppressOutput").is_none());
}
#[test]
fn test_hook_response_context_with_event() {
    let resp = HookRuntime::build_context_for_event("session ctx", "SessionStart");
    match &resp {
        HookResponse::Context {
            context,
            event_name,
        } => {
            assert_eq!(context, "session ctx");
            assert_eq!(event_name.as_deref(), Some("SessionStart"));
        }
        _ => panic!("Expected Context variant"),
    }
    let json = resp.to_json();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(
        parsed["hookSpecificOutput"]["hookEventName"],
        "SessionStart"
    );
}
#[test]
fn test_hook_response_equality() {
    assert_eq!(HookResponse::Allow, HookResponse::Allow);
    assert_ne!(
        HookResponse::Allow,
        HookResponse::Context {
            context: "x".into(),
            event_name: None
        }
    );
}
#[test]
fn test_runtime_has_pipeline() {
    let tmp = TempDir::new().unwrap();
    let runtime = HookRuntime::new(tmp.path()).unwrap();
    assert!(
        runtime.infra.pipeline.is_some(),
        "Pipeline should be initialized"
    );
}
#[test]
fn test_pipeline_process_file_extracts_symbols() {
    let tmp = TempDir::new().unwrap();
    let runtime = HookRuntime::new(tmp.path()).unwrap();
    let result = runtime
        .process_file(
            "test.py",
            "def foo():\n    pass\n\ndef bar():\n    return 42\n",
        )
        .unwrap();
    assert!(!result.symbols_added.is_empty(), "Should extract symbols");
    let names: Vec<&str> = result
        .symbols_added
        .iter()
        .map(|s| s.symbol_name.as_str())
        .collect();
    assert!(names.contains(&"foo"), "Should find 'foo' in {names:?}");
    assert!(names.contains(&"bar"), "Should find 'bar' in {names:?}");
}
#[test]
fn test_get_cached_symbols_after_process() {
    let tmp = TempDir::new().unwrap();
    let runtime = HookRuntime::new(tmp.path()).unwrap();
    assert!(runtime.get_cached_symbols("f.py").is_empty());
    runtime
        .process_file("f.py", "def foo():\n    pass\n")
        .unwrap();
    let syms = runtime.get_cached_symbols("f.py");
    assert!(!syms.is_empty(), "Cached symbols should not be empty");
    assert!(
        syms.iter().any(|s| s.symbol_name == "foo"),
        "Should find 'foo' in cached symbols"
    );
}
#[test]
fn test_second_read_uses_cache() {
    let tmp = TempDir::new().unwrap();
    let runtime = HookRuntime::new(tmp.path()).unwrap();
    let source = "def foo():\n    pass\n";
    let t1 = std::time::Instant::now();
    runtime.process_file("f.py", source).unwrap();
    let first = t1.elapsed();
    let t2 = std::time::Instant::now();
    let syms = runtime.get_cached_symbols("f.py");
    let second = t2.elapsed();
    assert!(!syms.is_empty(), "Cache should return symbols");
    assert!(
        second.as_millis() < 10,
        "Cache lookup took {}ms — should be near-instant",
        second.as_millis()
    );
    assert!(
        first.as_micros() > 0,
        "First parse should take measurable time"
    );
}
#[test]
fn test_pipeline_persists_symbols() {
    let tmp = TempDir::new().unwrap();
    let runtime = HookRuntime::new(tmp.path()).unwrap();
    runtime
        .process_file("f.py", "def foo():\n    pass\n")
        .unwrap();
    let syms = runtime.get_cached_symbols("f.py");
    assert!(!syms.is_empty(), "Pipeline should persist symbols");
    assert!(
        syms.iter().any(|s| s.symbol_name == "foo"),
        "Should contain 'foo'"
    );
}
#[test]
fn test_pipeline_cache_stats() {
    let tmp = TempDir::new().unwrap();
    let runtime = HookRuntime::new(tmp.path()).unwrap();
    let stats = runtime.pipeline_cache_stats();
    assert!(stats.is_some(), "Cache stats should be available");
    assert_eq!(stats.unwrap(), (0, 0), "Initially empty");
    runtime.process_file("a.py", "x = 1\n").unwrap();
    let (docs, trees) = runtime.pipeline_cache_stats().unwrap();
    assert_eq!(docs, 1);
    assert_eq!(trees, 1);
}
#[test]
fn test_pipeline_debug_includes_field() {
    let tmp = TempDir::new().unwrap();
    let runtime = HookRuntime::new(tmp.path()).unwrap();
    let debug = format!("{runtime:?}");
    assert!(
        debug.contains("pipeline"),
        "Debug output should include pipeline field"
    );
}
#[test]
fn test_export_metrics_without_quality_tracking() {
    let tmp = TempDir::new().unwrap();
    let rt = HookRuntime::new(tmp.path()).unwrap();
    let metrics = rt.export_metrics(None);
    assert_eq!(metrics.hooks.total_hooks_fired, 0);
    assert_eq!(metrics.hooks.success_count, 0);
    assert!((metrics.hooks.success_rate - 0.0).abs() < f64::EPSILON);
    assert!(metrics.rl.is_none());
    assert!(metrics.bandit.is_none());
    assert_eq!(metrics.session_turn, 0);
}
#[test]
fn test_export_metrics_with_quality_tracking() {
    let tmp = TempDir::new().unwrap();
    let mut rt = HookRuntime::new(tmp.path()).unwrap();
    rt.reset_quality_tracking("metrics-test");
    rt.record_hook_outcome(HookOutcome {
        hook_name: "pre_read".into(),
        success: true,
        latency_ms: 10,
        context_injected: true,
        knowledge_captured: false,
        error: None,
    });
    rt.record_hook_outcome(HookOutcome {
        hook_name: "post_bash".into(),
        success: false,
        latency_ms: 200,
        context_injected: false,
        knowledge_captured: false,
        error: Some("timeout".into()),
    });
    let metrics = rt.export_metrics(None);
    assert_eq!(metrics.hooks.total_hooks_fired, 2);
    assert_eq!(metrics.hooks.success_count, 1);
    assert_eq!(metrics.hooks.failure_count, 1);
    assert!((metrics.hooks.success_rate - 0.5).abs() < f64::EPSILON);
    assert_eq!(metrics.hooks.max_latency_ms, 200);
}
#[test]
fn test_export_metrics_with_qtable() {
    use touring_intelligence::rl::QLearning;
    let tmp = TempDir::new().unwrap();
    let rt = HookRuntime::new(tmp.path()).unwrap();
    let mut qt = touring_intelligence::rl::QTable::new();
    for _ in 0..20 {
        let _ = qt.update(0, 0, 0.8, 1, None, false);
    }
    let metrics = rt.export_metrics(Some(&qt));
    let rl = metrics.rl.expect("RL metrics should be present");
    assert_eq!(rl.total_updates, 20);
    assert!(rl.td_error_ema >= 0.0);
}
#[test]
fn test_export_metrics_serializes_to_json() {
    let tmp = TempDir::new().unwrap();
    let mut rt = HookRuntime::new(tmp.path()).unwrap();
    rt.reset_quality_tracking("json-test");
    rt.record_hook_outcome(HookOutcome {
        hook_name: "pre_read".into(),
        success: true,
        latency_ms: 5,
        context_injected: true,
        knowledge_captured: false,
        error: None,
    });
    let metrics = rt.export_metrics(None);
    let json = serde_json::to_string_pretty(&metrics);
    assert!(json.is_ok(), "Metrics should serialize to JSON");
    let json_str = json.unwrap();
    assert!(json_str.contains("\"total_hooks_fired\""));
    assert!(json_str.contains("\"cache\""));
    assert!(json_str.contains("\"session_turn\""));
}
#[test]
fn test_session_turn_starts_at_zero() {
    let tmp = TempDir::new().unwrap();
    let rt = HookRuntime::new(tmp.path()).unwrap();
    assert_eq!(rt.session_turn(), 0);
}
#[test]
fn test_session_turn_increment() {
    let tmp = TempDir::new().unwrap();
    let rt = HookRuntime::new(tmp.path()).unwrap();
    let first = rt.advance_session_turn();
    assert_eq!(first, 1);
    assert_eq!(rt.session_turn(), 1);
    let second = rt.advance_session_turn();
    assert_eq!(second, 2);
    assert_eq!(rt.session_turn(), 2);
}
#[test]
fn test_session_turn_concurrent_increments() {
    let tmp = TempDir::new().unwrap();
    let rt = HookRuntime::new(tmp.path()).unwrap();
    for i in 1..=10 {
        assert_eq!(rt.advance_session_turn(), i);
    }
    assert_eq!(rt.session_turn(), 10);
}
#[test]
fn test_hook_result_cache_new() {
    let cache: HookResultCache = HookResultCache::new(256, None);
    assert_eq!(cache.get_result("pre_read", "foo.py"), None);
    let cache_ttl: HookResultCache = HookResultCache::new(256, Some(1));
    assert_eq!(cache_ttl.get_result("pre_read", "foo.py"), None);
}
#[test]
fn test_hook_result_cache_set_and_get() {
    let cache: HookResultCache = HookResultCache::new(256, None);
    assert!(cache.get_result("pre_read", "foo.py").is_none());
    cache.cache_result("pre_read", "foo.py", r#"{"gotchas":["G1"]}"#.into());
    let result = cache.get_result("pre_read", "foo.py");
    assert!(result.is_some());
    assert_eq!(result.unwrap(), r#"{"gotchas":["G1"]}"#);
}
#[test]
fn test_hook_result_cache_hit_rate() {
    let cache: HookResultCache = HookResultCache::new(256, None);
    assert!(cache.get_result("pre_read", "missing.py").is_none());
    let miss_rate = cache.hit_rate();
    assert_eq!(miss_rate, 0.0);
    cache.cache_result("pre_read", "foo.py", r#"{"x":1}"#.into());
    let _ = cache.get_result("pre_read", "foo.py");
    let hit_rate = cache.hit_rate();
    assert!((hit_rate - 0.5).abs() < f64::EPSILON);
}
#[test]
fn test_hook_result_cache_invalidation() {
    let cache: HookResultCache = HookResultCache::new(256, None);
    cache.cache_result("pre_read", "foo.py", r#"{"a":1}"#.into());
    cache.cache_result("pre_read", "bar.py", r#"{"b":2}"#.into());
    cache.cache_result("post_edit", "foo.py", r#"{"c":3}"#.into());
    let count = cache.invalidate_file("foo.py");
    assert_eq!(count, 2);
    assert!(cache.get_result("pre_read", "foo.py").is_none());
    assert!(cache.get_result("post_edit", "foo.py").is_none());
    assert_eq!(
        cache.get_result("pre_read", "bar.py"),
        Some(r#"{"b":2}"#.into())
    );
}
#[test]
fn test_hook_result_cache_capacity_bounded() {
    let cache: HookResultCache = HookResultCache::new(2, None);
    cache.cache_result("pre_read", "a.py", "v1".into());
    cache.cache_result("pre_read", "b.py", "v2".into());
    assert_eq!(cache.get_result("pre_read", "a.py"), Some("v1".into()));
    assert_eq!(cache.get_result("pre_read", "b.py"), Some("v2".into()));
    for i in 0..20 {
        cache.cache_result("pre_read", &format!("extra_{i}.py"), format!("x{i}"));
    }
    cache.run_pending();
    let mut present = 0;
    if cache.get_result("pre_read", "a.py").is_some() {
        present += 1;
    }
    if cache.get_result("pre_read", "b.py").is_some() {
        present += 1;
    }
    for i in 0..20 {
        if cache
            .get_result("pre_read", &format!("extra_{i}.py"))
            .is_some()
        {
            present += 1;
        }
    }
    assert!(
        present <= 4,
        "Expected at most ~2 entries after eviction, found {present}"
    );
}
#[test]
fn test_hook_response_clone() {
    let resp = HookRuntime::build_context("test");
    let cloned = resp.clone();
    assert_eq!(resp, cloned);
}
#[test]
fn test_hook_response_partial_eq() {
    assert_eq!(HookResponse::Allow, HookResponse::Allow);
    let a = HookResponse::Context {
        context: "ctx1".into(),
        event_name: Some("ev1".into()),
    };
    let b = HookResponse::Context {
        context: "ctx1".into(),
        event_name: Some("ev1".into()),
    };
    assert_eq!(a, b);
    let c = HookResponse::Context {
        context: "ctx2".into(),
        event_name: Some("ev1".into()),
    };
    assert_ne!(a, c);
    assert_ne!(HookResponse::Allow, a);
}
#[test]
fn test_hook_response_allow_to_json() {
    let resp = HookResponse::Allow;
    assert_eq!(resp.to_json(), "{}");
}
#[test]
fn test_hook_response_context_to_json() {
    let resp = HookResponse::Context {
        context: "injected context".into(),
        event_name: None,
    };
    let json = resp.to_json();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(
        parsed["hookSpecificOutput"]["additionalContext"],
        "injected context"
    );
    assert!(parsed["hookSpecificOutput"].get("hookEventName").is_none());
}
#[test]
fn test_aco_wiring_state_new() {
    let state = crate::aco_wiring::AcoWiringState::new();
    let debug = format!("{:?}", state);
    assert!(debug.contains("AcoWiringState"));
}
#[test]
fn test_aco_wiring_state_pheromone_deposit_and_query() {
    let state = crate::aco_wiring::AcoWiringState::new();
    state.deposit_file_edit("src/main.rs");
    assert_eq!(state.task_heat("src/main.rs"), 0.0);
    assert_eq!(state.teammate_heat("src/main.rs"), 0.0);
}
#[test]
fn test_aco_wiring_state_task_completion_pheromone() {
    let state = crate::aco_wiring::AcoWiringState::new();
    state.deposit_task_completion("task-42", true);
    let heat = state.task_heat("task-42");
    assert!(
        heat > 0.0,
        "Successful task should have positive heat, got {heat}"
    );
}
#[test]
fn test_aco_wiring_state_task_failure_pheromone() {
    let state = crate::aco_wiring::AcoWiringState::new();
    state.deposit_task_completion("task-99", false);
    let heat = state.task_heat("task-99");
    assert!(
        heat < 0.0,
        "Failed task should have negative heat, got {heat}"
    );
}
#[test]
fn test_aco_wiring_state_teammate_idle_pheromone() {
    let state = crate::aco_wiring::AcoWiringState::new();
    state.deposit_teammate_idle("worker-1", 3);
    let heat = state.teammate_heat("worker-1");
    assert!(
        heat > 0.0,
        "Productive idle should have positive heat, got {heat}"
    );
}
#[test]
fn test_aco_wiring_state_limbo_pheromone() {
    let state = crate::aco_wiring::AcoWiringState::new();
    state.deposit_teammate_limbo("worker-2", 2, 5);
    let heat = state.limbo_heat("worker-2");
    assert!(
        heat < 0.0,
        "Limbo pattern should have negative heat, got {heat}"
    );
}
#[test]
fn test_aco_wiring_state_multiple_deposits_accumulate() {
    let state = crate::aco_wiring::AcoWiringState::new();
    state.deposit_task_completion("task-multi", true);
    state.deposit_task_completion("task-multi", true);
    let heat1 = state.task_heat("task-multi");
    state.deposit_task_completion("task-multi", true);
    let heat2 = state.task_heat("task-multi");
    assert!(
        heat2 > heat1,
        "Multiple successful completions should accumulate: {heat1} < {heat2}"
    );
}
#[test]
fn init_ann_memory_creates_file_backed_db() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    let mut rt = HookRuntime::new(root).expect("runtime init");
    assert!(
        rt.ctx.ann_recall.borrow().is_none(),
        "ann_recall must start as None"
    );
    rt.init_ann_memory();
    assert!(
        rt.ctx.ann_recall.borrow().is_some(),
        "ann_recall must be Some after init"
    );
    assert!(
        root.join(".claude/touring/memory.db").exists(),
        "memory.db must exist after init_ann_memory"
    );
}
#[test]
fn test_hook_response_deny_to_json() {
    let resp = HookResponse::Deny {
        reason: "syntax error".to_string(),
        context: Some("details here".to_string()),
        event_name: Some("PreToolUse".to_string()),
    };
    let json = resp.to_json();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed["hookSpecificOutput"]["permissionDecision"], "deny");
    assert_eq!(
        parsed["hookSpecificOutput"]["permissionDecisionReason"],
        "syntax error"
    );
    assert_eq!(
        parsed["hookSpecificOutput"]["additionalContext"],
        "details here"
    );
}
#[test]
fn test_hook_response_deny_without_context() {
    let resp = HookResponse::Deny {
        reason: "blocked".to_string(),
        context: None,
        event_name: None,
    };
    let json = resp.to_json();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed["hookSpecificOutput"]["permissionDecision"], "deny");
    assert_eq!(
        parsed["hookSpecificOutput"]["permissionDecisionReason"],
        "blocked"
    );
    assert!(
        parsed["hookSpecificOutput"]
            .get("additionalContext")
            .is_none()
    );
    assert!(parsed["hookSpecificOutput"].get("hookEventName").is_none());
}
#[test]
fn test_hook_response_block_to_json() {
    let resp = HookResponse::Block {
        reason: "regression detected".to_string(),
        context: Some("3 new warnings".to_string()),
        event_name: Some("PostToolUse".to_string()),
    };
    let json = resp.to_json();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed["decision"], "block");
    assert_eq!(parsed["reason"], "regression detected");
    assert_eq!(
        parsed["hookSpecificOutput"]["additionalContext"],
        "3 new warnings"
    );
}
#[test]
fn test_hook_response_block_without_context() {
    let resp = HookResponse::Block {
        reason: "failed".to_string(),
        context: None,
        event_name: None,
    };
    let json = resp.to_json();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed["decision"], "block");
    assert_eq!(parsed["reason"], "failed");
    assert!(
        parsed["hookSpecificOutput"]
            .get("additionalContext")
            .is_none()
    );
}
#[test]
fn test_hook_response_halt_to_json() {
    let resp = HookResponse::Halt {
        reason: "circuit breaker triggered".to_string(),
    };
    let json = resp.to_json();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed["continue"], false);
    assert_eq!(parsed["stopReason"], "circuit breaker triggered");
}
#[test]
fn test_build_deny() {
    let resp = HookRuntime::build_deny("test reason");
    match resp {
        HookResponse::Deny {
            reason,
            context,
            event_name,
        } => {
            assert_eq!(reason, "test reason");
            assert!(context.is_none());
            assert_eq!(event_name, Some("PreToolUse".to_string()));
        }
        _ => panic!("Expected Deny variant"),
    }
}
#[test]
fn test_build_deny_with_context() {
    let resp = HookRuntime::build_deny_with_context("deny reason", "extra info");
    match resp {
        HookResponse::Deny {
            reason,
            context,
            event_name,
        } => {
            assert_eq!(reason, "deny reason");
            assert_eq!(context, Some("extra info".to_string()));
            assert_eq!(event_name, Some("PreToolUse".to_string()));
        }
        _ => panic!("Expected Deny variant"),
    }
}
#[test]
fn test_build_block() {
    let resp = HookRuntime::build_block("block reason");
    match resp {
        HookResponse::Block {
            reason,
            context,
            event_name,
        } => {
            assert_eq!(reason, "block reason");
            assert!(context.is_none());
            assert_eq!(event_name, Some("PostToolUse".to_string()));
        }
        _ => panic!("Expected Block variant"),
    }
}
#[test]
fn test_build_block_with_context() {
    let resp = HookRuntime::build_block_with_context("block reason", "context info");
    match resp {
        HookResponse::Block {
            reason,
            context,
            event_name,
        } => {
            assert_eq!(reason, "block reason");
            assert_eq!(context, Some("context info".to_string()));
            assert_eq!(event_name, Some("PostToolUse".to_string()));
        }
        _ => panic!("Expected Block variant"),
    }
}
#[test]
fn test_build_halt() {
    let resp = HookRuntime::build_halt("halt reason");
    match resp {
        HookResponse::Halt { reason } => assert_eq!(reason, "halt reason"),
        _ => panic!("Expected Halt variant"),
    }
}
#[test]
fn test_build_context_with_updated_input() {
    let updated = serde_json::json!({ "file_path" : "/normalized/path.rs" });
    let resp = HookRuntime::build_context_with_updated_input("path normalized", updated.clone());
    match resp {
        HookResponse::ContextWithUpdatedInput {
            context,
            event_name,
            updated_input,
        } => {
            assert_eq!(context, "path normalized");
            assert_eq!(event_name, Some("PreToolUse".to_string()));
            assert_eq!(updated_input, updated);
        }
        _ => panic!("Expected ContextWithUpdatedInput variant"),
    }
}
#[test]
fn test_hook_response_context_with_updated_input_json() {
    let updated = serde_json::json!({ "file_path" : "/normalized/path.rs" });
    let resp = HookResponse::ContextWithUpdatedInput {
        context: "path normalized".to_string(),
        event_name: Some("PreToolUse".to_string()),
        updated_input: updated,
    };
    let json = resp.to_json();
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
    assert_eq!(
        parsed["hookSpecificOutput"]["additionalContext"],
        "path normalized"
    );
    assert_eq!(
        parsed["hookSpecificOutput"]["updatedInput"]["file_path"],
        "/normalized/path.rs"
    );
    assert_eq!(parsed["hookSpecificOutput"]["hookEventName"], "PreToolUse");
}
#[test]
fn test_truncate_context_short_string() {
    let short = "hello world";
    let result = crate::hook_response::truncate_context(short);
    assert_eq!(result, short);
}
#[test]
fn test_truncate_context_at_limit() {
    let exact = "a".repeat(9_500);
    let result = crate::hook_response::truncate_context(&exact);
    assert_eq!(result, exact);
}
#[test]
fn test_truncate_context_over_limit() {
    let long = "a".repeat(10_000);
    let result = crate::hook_response::truncate_context(&long);
    assert!(result.len() < long.len());
    assert!(result.contains("[truncated: 10000 chars total]"));
    assert!(result.ends_with(']'));
}
#[test]
fn test_truncate_context_utf8_safe() {
    let emoji = "😀".repeat(3_000);
    let result = crate::hook_response::truncate_context(&emoji);
    assert!(result.contains("[truncated:"));
    let _: &str = &result;
}
#[test]
fn test_context_response_truncates_in_to_json() {
    let long_ctx = "x".repeat(10_000);
    let resp = HookResponse::Context {
        context: long_ctx,
        event_name: None,
    };
    let json = resp.to_json();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    let ctx = parsed["hookSpecificOutput"]["additionalContext"]
        .as_str()
        .expect("additionalContext should be a string");
    assert!(ctx.contains("[truncated: 10000 chars total]"));
    assert!(ctx.len() < 10_000);
}
