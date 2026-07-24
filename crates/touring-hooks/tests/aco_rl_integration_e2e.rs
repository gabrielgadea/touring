//! E2E tests for ACO pheromone wiring and RL reward integration.
//!
//! Validates that ACO metrics (HookQualityAssessment, HookResultCache)
//! integrate correctly with the RL reward system.

#![allow(
    clippy::indexing_slicing,
    clippy::assertions_on_constants,
    clippy::let_unit_value,
    clippy::manual_range_contains,
    clippy::useless_vec,
    clippy::int_plus_one,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic
)]

use tempfile::TempDir;
use touring_hooks::aco_bridge::{
    HookOutcome, HookQualityAssessment, HookResultCache, StreamingHookStats,
};
use touring_hooks::cli_handlers::cli_learning_reward;

// ── Helpers ──────────────────────────────────────────────────────────────────

fn setup_runtime() -> (TempDir, touring_hooks::runtime::HookRuntime) {
    let tmp = TempDir::new().expect("tempdir");
    let root = tmp.path().to_path_buf();
    std::fs::create_dir_all(root.join(".claude/data")).expect("data dir");
    let rt = touring_hooks::runtime::HookRuntime::new(&root).expect("runtime init");
    (tmp, rt)
}

fn parse_json(s: &str) -> serde_json::Value {
    serde_json::from_str(s).expect("valid JSON")
}

// ═══════════════════════════════════════════════════════════════════════════════
// TEST GROUP 1: ACO Pheromone Cycle
// ═══════════════════════════════════════════════════════════════════════════════

/// E2E Test 1: HookQualityAssessment records outcomes correctly
#[test]
fn test_hook_quality_assessment_records_outcomes() {
    let outcome = HookOutcome {
        hook_name: "pre_read".to_string(),
        success: true,
        latency_ms: 12,
        context_injected: true,
        knowledge_captured: false,
        error: None,
    };

    let mut assessment = HookQualityAssessment::new("session-1");
    assessment.record(outcome);

    assert_eq!(assessment.session_id, "session-1");
    assert_eq!(assessment.total_hooks_fired, 1);
    assert_eq!(assessment.streaming_stats.success_count, 1);
    assert_eq!(assessment.streaming_stats.failure_count, 0);
}

/// E2E Test 2: HookQualityAssessment computes latency stats correctly
#[test]
fn test_hook_quality_latency_stats() {
    let fast_outcome = HookOutcome {
        hook_name: "pre_read".to_string(),
        success: true,
        latency_ms: 50, // under target
        context_injected: true,
        knowledge_captured: false,
        error: None,
    };
    let slow_outcome = HookOutcome {
        hook_name: "pre_read".to_string(),
        success: true,
        latency_ms: 200, // over target
        context_injected: true,
        knowledge_captured: false,
        error: None,
    };

    let mut assessment = HookQualityAssessment::new("session-2");
    assessment.record(fast_outcome);
    assessment.record(slow_outcome);

    // One fast (50ms <= 100), one slow (200ms > 100)
    assert_eq!(assessment.streaming_stats.fast_hooks_count, 1);
    assert_eq!(assessment.streaming_stats.latency_sum_ms, 250);
    assert!((assessment.streaming_stats.avg_latency_ms() - 125.0).abs() < 0.001);
}

/// E2E Test 3: HookQualityAssessment computes reliability via success_rate
#[test]
fn test_hook_quality_success_rate() {
    let success_outcome = HookOutcome {
        hook_name: "pre_read".to_string(),
        success: true,
        latency_ms: 10,
        context_injected: true,
        knowledge_captured: false,
        error: None,
    };
    let failure_outcome = HookOutcome {
        hook_name: "post_read".to_string(),
        success: false,
        latency_ms: 5,
        context_injected: false,
        knowledge_captured: true,
        error: Some("connection timeout".to_string()),
    };

    let mut assessment = HookQualityAssessment::new("session-3");
    assessment.record(success_outcome);
    assessment.record(failure_outcome);

    assert_eq!(assessment.streaming_stats.success_count, 1);
    assert_eq!(assessment.streaming_stats.failure_count, 1);
    assert!((assessment.streaming_stats.success_rate() - 0.5).abs() < 0.001);
}

/// E2E Test 4: HookResultCache stores and retrieves results
#[test]
fn test_hook_result_cache_store_and_retrieve() {
    let cache = HookResultCache::new(100, Some(60_000));

    cache.cache_result(
        "pre_read",
        "/src/main.rs",
        r#"{"context": "injected"}"#.to_string(),
    );

    let result = cache.get_result("pre_read", "/src/main.rs");
    assert!(result.is_some(), "cached result should be retrievable");
    assert_eq!(result.unwrap(), r#"{"context": "injected"}"#);
}

/// E2E Test 5: HookResultCache returns None for missing entries
#[test]
fn test_hook_result_cache_miss() {
    let cache = HookResultCache::new(100, Some(60_000));

    let result = cache.get_result("pre_read", "/nonexistent.rs");
    assert!(result.is_none(), "missing entries should return None");
}

/// E2E Test 6: HookResultCache invalidates by file path
#[test]
fn test_hook_result_cache_invalidate_file() {
    let cache = HookResultCache::new(100, Some(60_000));

    cache.cache_result("pre_read", "/src/main.rs", r#"{"a": 1}"#.to_string());
    cache.cache_result("pre_read", "/src/utils.rs", r#"{"b": 2}"#.to_string());
    cache.cache_result("post_edit", "/src/main.rs", r#"{"c": 3}"#.to_string());

    // Invalidate all entries for /src/main.rs
    let count = cache.invalidate_file("/src/main.rs");
    assert_eq!(count, 2, "should invalidate 2 entries for main.rs");

    // main.rs entries gone
    assert!(cache.get_result("pre_read", "/src/main.rs").is_none());
    assert!(cache.get_result("post_edit", "/src/main.rs").is_none());

    // utils.rs entry still present
    assert!(cache.get_result("pre_read", "/src/utils.rs").is_some());
}

/// E2E Test 7: HookResultCache hit rate tracking
#[test]
fn test_hook_result_cache_hit_rate() {
    let cache = HookResultCache::new(100, Some(60_000));

    // Cache an entry, then access it
    cache.cache_result("pre_read", "/a.rs", r#"{"x": 1}"#.to_string());
    cache.cache_result("pre_read", "/b.rs", r#"{"y": 2}"#.to_string());

    // Access /a.rs twice (hit), /c.rs once (miss)
    let _ = cache.get_result("pre_read", "/a.rs"); // hit
    let _ = cache.get_result("pre_read", "/c.rs"); // miss
    let _ = cache.get_result("pre_read", "/a.rs"); // hit

    let hit_rate = cache.hit_rate();
    // 2 hits / 3 total = 0.666
    assert!(
        (hit_rate - 2.0 / 3.0).abs() < 0.001,
        "hit rate should be ~0.666, got {hit_rate}"
    );
}

// ═══════════════════════════════════════════════════════════════════════════════
// TEST GROUP 2: RL Reward Injection
// ═══════════════════════════════════════════════════════════════════════════════

/// E2E Test 8: cli_learning_reward injects positive reward
#[test]
fn test_cli_learning_reward_positive() {
    let (_tmp, mut rt) = setup_runtime();

    let payload = serde_json::json!({
        "tool": "edit",
        "reward": 0.8,
        "context": "successful edit operation"
    });

    let result = parse_json(&cli_learning_reward(&mut rt, &payload));

    assert_eq!(result["status"], "reward_injected");
    assert_eq!(result["tool"], "edit");
    assert!((result["value"].as_f64().unwrap() - 0.8).abs() < 0.001);
    assert_eq!(result["context"], "successful edit operation");
}

/// E2E Test 9: cli_learning_reward clamps negative reward
#[test]
fn test_cli_learning_reward_negative_clamped() {
    let (_tmp, mut rt) = setup_runtime();

    let payload = serde_json::json!({
        "tool": "edit",
        "reward": -2.0, // exceeds -1.0 bound
        "context": "failing edit"
    });

    let result = parse_json(&cli_learning_reward(&mut rt, &payload));

    assert_eq!(result["status"], "reward_injected");
    // Should be clamped to -1.0
    assert!((result["value"].as_f64().unwrap() - (-1.0)).abs() < 0.001);
}

/// E2E Test 10: cli_learning_reward clamps positive reward exceeding 1.0
#[test]
fn test_cli_learning_reward_positive_clamped() {
    let (_tmp, mut rt) = setup_runtime();

    let payload = serde_json::json!({
        "tool": "read",
        "reward": 1.5,
        "context": "exceptional read"
    });

    let result = parse_json(&cli_learning_reward(&mut rt, &payload));

    assert_eq!(result["status"], "reward_injected");
    // Should be clamped to 1.0
    assert!((result["value"].as_f64().unwrap() - 1.0).abs() < 0.001);
}

/// E2E Test 11: cli_learning_reward rejects empty tool name
#[test]
fn test_cli_learning_reward_requires_tool_name() {
    let (_tmp, mut rt) = setup_runtime();

    let payload = serde_json::json!({
        "reward": 0.5
    });

    let result = parse_json(&cli_learning_reward(&mut rt, &payload));

    assert!(
        result.get("error").is_some(),
        "should return error for empty tool name"
    );
    assert!(result["error"].as_str().unwrap().contains("tool name"));
}

/// E2E Test 12: cli_learning_reward handles zero (neutral) reward
#[test]
fn test_cli_learning_reward_neutral() {
    let (_tmp, mut rt) = setup_runtime();

    let payload = serde_json::json!({
        "tool": "bash",
        "reward": 0.0,
        "context": "neutral outcome"
    });

    let result = parse_json(&cli_learning_reward(&mut rt, &payload));

    assert_eq!(result["status"], "reward_injected");
    assert!((result["value"].as_f64().unwrap() - 0.0).abs() < 0.001);
}

// ═══════════════════════════════════════════════════════════════════════════════
// TEST GROUP 3: ACO Metrics Collection
// ═══════════════════════════════════════════════════════════════════════════════

/// E2E Test 13: HookQualityAssessment tracker report builds with all dimensions
#[test]
fn test_hook_quality_tracker_report_dimensions() {
    let outcomes = vec![
        HookOutcome {
            hook_name: "pre_read".to_string(),
            success: true,
            latency_ms: 50,
            context_injected: true,
            knowledge_captured: false,
            error: None,
        },
        HookOutcome {
            hook_name: "post_read".to_string(),
            success: true,
            latency_ms: 30,
            context_injected: false,
            knowledge_captured: true,
            error: None,
        },
    ];

    let mut assessment = HookQualityAssessment::new("report-session");
    for outcome in outcomes {
        assessment.record(outcome);
    }

    let report = assessment.to_tracker_report(1);

    // Verify all 9 dimensions present
    assert_eq!(report.dims.len(), 9);

    let dim_ids: Vec<_> = report.dims.iter().map(|d| d.dim_id.clone()).collect();
    assert!(dim_ids.contains(&"D1".to_string())); // Precision
    assert!(dim_ids.contains(&"D3".to_string())); // Latency
    assert!(dim_ids.contains(&"D6".to_string())); // Reliability
}

/// E2E Test 14: LATENCY_TARGET_MS constant is respected in streaming stats
#[test]
fn test_latency_target_ms_constant() {
    assert_eq!(StreamingHookStats::LATENCY_TARGET_MS, 100);
}

/// E2E Test 15: HookQualityAssessment pre/post hook classification
#[test]
fn test_hook_quality_pre_post_classification() {
    let pre_outcome = HookOutcome {
        hook_name: "pre_edit".to_string(),
        success: true,
        latency_ms: 5,
        context_injected: true,
        knowledge_captured: false,
        error: None,
    };
    let post_outcome = HookOutcome {
        hook_name: "post_edit".to_string(),
        success: true,
        latency_ms: 5,
        context_injected: false,
        knowledge_captured: true,
        error: None,
    };

    let mut assessment = HookQualityAssessment::new("pre-post");
    assessment.record(pre_outcome);
    assessment.record(post_outcome);

    assert_eq!(assessment.streaming_stats.pre_hook_count, 1);
    assert_eq!(assessment.streaming_stats.post_hook_count, 1);
    assert_eq!(assessment.streaming_stats.context_injected_count, 1);
    assert_eq!(assessment.streaming_stats.knowledge_captured_count, 1);
}

// ═══════════════════════════════════════════════════════════════════════════════
// TEST GROUP 4: Circuit Breaker Integration
// ═══════════════════════════════════════════════════════════════════════════════

/// E2E Test 16: HookRuntime initializes without circuit breaker panic
#[test]
fn test_runtime_circuit_breaker_initialized() {
    let (_tmp, _rt) = setup_runtime();
    // If we get here without panic, circuit breaker initialization is fine
    assert!(true);
}

/// E2E Test 17: Circuit breaker OpClass variants exist
#[test]
fn test_circuit_breaker_op_class_defaults() {
    use touring_hooks::circuit_breaker::OpClass;

    assert!(matches!(OpClass::Light, OpClass::Light));
    assert!(matches!(OpClass::Medium, OpClass::Medium));
    assert!(matches!(OpClass::Heavy, OpClass::Heavy));
    assert!(matches!(OpClass::Critical, OpClass::Critical));
}

// ═══════════════════════════════════════════════════════════════════════════════
// TEST GROUP 5: RL Reward for Hook Outcomes
// ═══════════════════════════════════════════════════════════════════════════════

/// E2E Test 18: HookOutcome success maps to positive streaming stats
#[test]
fn test_hook_outcome_success_positive_stats() {
    let outcome = HookOutcome {
        hook_name: "pre_bash".to_string(),
        success: true,
        latency_ms: 15,
        context_injected: true,
        knowledge_captured: false,
        error: None,
    };

    let mut assessment = HookQualityAssessment::new("quality-test");
    assessment.record(outcome);

    assert_eq!(assessment.streaming_stats.success_rate(), 1.0);
}

/// E2E Test 19: HookOutcome failure degrades success rate
#[test]
fn test_hook_outcome_failure_degraded_stats() {
    let outcome = HookOutcome {
        hook_name: "pre_write".to_string(),
        success: false,
        latency_ms: 500,
        context_injected: false,
        knowledge_captured: false,
        error: Some("disk full".to_string()),
    };

    let mut assessment = HookQualityAssessment::new("failure-test");
    assessment.record(outcome);

    assert_eq!(assessment.streaming_stats.success_rate(), 0.0);
}

/// E2E Test 20: Multiple outcomes compute correct aggregate scores
#[test]
fn test_multiple_outcomes_aggregate_correctly() {
    let outcomes = vec![
        HookOutcome {
            hook_name: "pre_read".to_string(),
            success: true,
            latency_ms: 10,
            context_injected: true,
            knowledge_captured: false,
            error: None,
        },
        HookOutcome {
            hook_name: "pre_read".to_string(),
            success: true,
            latency_ms: 20,
            context_injected: true,
            knowledge_captured: false,
            error: None,
        },
        HookOutcome {
            hook_name: "pre_read".to_string(),
            success: false,
            latency_ms: 100,
            context_injected: false,
            knowledge_captured: false,
            error: Some("timeout".to_string()),
        },
        HookOutcome {
            hook_name: "pre_read".to_string(),
            success: true,
            latency_ms: 30,
            context_injected: true,
            knowledge_captured: false,
            error: None,
        },
    ];

    let mut assessment = HookQualityAssessment::new("aggregate-test");
    for outcome in outcomes {
        assessment.record(outcome);
    }

    assert_eq!(assessment.streaming_stats.success_count, 3);
    assert_eq!(assessment.streaming_stats.failure_count, 1);
    assert_eq!(assessment.streaming_stats.total(), 4);

    // avg = (10+20+100+30)/4 = 40ms, all under 100 target
    assert_eq!(assessment.streaming_stats.fast_hooks_count, 4);
}

/// E2E Test 21: Empty assessment returns neutral scores
#[test]
fn test_empty_assessment_neutral_scores() {
    let assessment = HookQualityAssessment::new("empty-session");

    assert_eq!(assessment.streaming_stats.success_rate(), 1.0); // neutral for empty
    assert_eq!(assessment.total_hooks_fired, 0);
}

/// E2E Test 22: HookResultCache hit rate with no requests
#[test]
fn test_hook_result_cache_hit_rate_empty() {
    let cache = HookResultCache::new(100, Some(60_000));
    assert_eq!(cache.hit_rate(), 0.0, "empty cache should have 0 hit rate");
}

/// E2E Test 23: cli_learning_reward with alternate field names
#[test]
fn test_cli_learning_reward_alternate_field_names() {
    let (_tmp, mut rt) = setup_runtime();

    // Test with "tool_name" instead of "tool"
    let payload = serde_json::json!({
        "tool_name": "read",
        "value": 0.6,
        "context": "good read performance"
    });

    let result = parse_json(&cli_learning_reward(&mut rt, &payload));
    assert_eq!(result["status"], "reward_injected");
    assert_eq!(result["tool"], "read");
}

/// E2E Test 24: cli_learning_reward with reward field (not value)
#[test]
fn test_cli_learning_reward_reward_field() {
    let (_tmp, mut rt) = setup_runtime();

    let payload = serde_json::json!({
        "tool": "edit",
        "reward": 0.7,
        "context": "positive edit"
    });

    let result = parse_json(&cli_learning_reward(&mut rt, &payload));
    assert_eq!(result["status"], "reward_injected");
    assert!((result["value"].as_f64().unwrap() - 0.7).abs() < 0.001);
}

/// E2E Test 25: HookQualityAssessment average latency calculation
#[test]
fn test_streaming_stats_avg_latency() {
    let mut stats = StreamingHookStats::default();
    stats.record(&HookOutcome {
        hook_name: "pre_read".to_string(),
        success: true,
        latency_ms: 100,
        context_injected: true,
        knowledge_captured: false,
        error: None,
    });
    stats.record(&HookOutcome {
        hook_name: "pre_read".to_string(),
        success: true,
        latency_ms: 200,
        context_injected: true,
        knowledge_captured: false,
        error: None,
    });

    assert!(
        (stats.avg_latency_ms() - 150.0).abs() < 0.001,
        "avg should be 150ms"
    );
    assert_eq!(stats.max_latency_ms, 200);
}

/// E2E Test 26: HookResultCache with small TTL still works
#[test]
fn test_hook_result_cache_small_ttl_still_works() {
    // Using a small but non-zero TTL
    let cache = HookResultCache::new(100, Some(100));
    cache.cache_result("test", "/a.rs", r#"{"x": 1}"#.to_string());
    // Should be retrievable immediately
    assert!(cache.get_result("test", "/a.rs").is_some());
}

/// E2E Test 27: HookQualityAssessment context injection tracking
#[test]
fn test_hook_quality_context_tracking() {
    let outcomes = vec![
        HookOutcome {
            hook_name: "pre_read".to_string(),
            success: true,
            latency_ms: 5,
            context_injected: true,
            knowledge_captured: false,
            error: None,
        },
        HookOutcome {
            hook_name: "pre_read".to_string(),
            success: true,
            latency_ms: 5,
            context_injected: false, // not injected
            knowledge_captured: false,
            error: None,
        },
        HookOutcome {
            hook_name: "pre_read".to_string(),
            success: true,
            latency_ms: 5,
            context_injected: true,
            knowledge_captured: false,
            error: None,
        },
    ];

    let mut assessment = HookQualityAssessment::new("context-test");
    for outcome in outcomes {
        assessment.record(outcome);
    }

    assert_eq!(assessment.streaming_stats.context_injected_count, 2);
    assert_eq!(assessment.streaming_stats.pre_hook_count, 3);
}

/// E2E Test 28: HookQualityAssessment knowledge tracking
#[test]
fn test_hook_quality_knowledge_tracking() {
    let outcomes = vec![
        HookOutcome {
            hook_name: "post_read".to_string(),
            success: true,
            latency_ms: 5,
            context_injected: false,
            knowledge_captured: true,
            error: None,
        },
        HookOutcome {
            hook_name: "post_read".to_string(),
            success: true,
            latency_ms: 5,
            context_injected: false,
            knowledge_captured: false, // not captured
            error: None,
        },
    ];

    let mut assessment = HookQualityAssessment::new("knowledge-test");
    for outcome in outcomes {
        assessment.record(outcome);
    }

    assert_eq!(assessment.streaming_stats.knowledge_captured_count, 1);
    assert_eq!(assessment.streaming_stats.post_hook_count, 2);
}

/// E2E Test 29: HookResultCache stats returns correct tuple
#[test]
fn test_hook_result_cache_stats() {
    let cache = HookResultCache::new(100, Some(60_000));

    cache.cache_result("pre", "/a.rs", r#"{}"#.to_string());
    let _ = cache.get_result("pre", "/a.rs"); // hit
    let _ = cache.get_result("pre", "/b.rs"); // miss

    let (hits, misses, _) = cache.stats();
    assert_eq!(hits, 1);
    assert_eq!(misses, 1);
}

/// E2E Test 30: HookQualityAssessment integration with HookResultCache E2E
#[test]
fn test_aco_metrics_cache_integration_e2e() {
    let cache = HookResultCache::new(100, Some(60_000));

    // Simulate hook execution flow: cache result + quality assessment
    let outcome = HookOutcome {
        hook_name: "pre_edit".to_string(),
        success: true,
        latency_ms: 8,
        context_injected: true,
        knowledge_captured: false,
        error: None,
    };

    // Record quality
    let mut assessment = HookQualityAssessment::new("integration-test");
    assessment.record(outcome.clone());

    // Cache the result
    cache.cache_result(
        &outcome.hook_name,
        "/test/file.rs",
        serde_json::to_string(&outcome).expect("json"),
    );

    // Verify both are functional
    assert_eq!(assessment.streaming_stats.success_count, 1);
    assert!(cache.get_result("pre_edit", "/test/file.rs").is_some());
    assert_eq!(assessment.streaming_stats.success_rate(), 1.0);
}
