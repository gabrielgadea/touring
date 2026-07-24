//! E2E Integration Tests — Token Efficiency Enhancement (v31+v32)
//!
//! Proves that all modules work together as an integrated system:
//! 1. DetailLevel truncates output correctly across all levels
//! 2. Suggestions engine provides context-aware hints
//! 3. Session hints evolve with tool call history
//! 4. Diff parser correctly maps changes to symbols
//! 5. Hybrid search produces fused results
//! 6. Summary cache persists and retrieves with TTL
//! 7. Risk scoring combines multiple signals into unified score
//! 8. Context tools aggregate all subsystems into compact output

use touring_server::tools::{
    context_tools, diff_parser, hybrid_search, risk_scoring,
    session_hints::{SessionHintEngine, SessionIntent},
    suggestions,
    summary_cache::SummaryCache,
};
use touring_server::{DetailLevel, apply_detail_level};

// ── E2E Test 1: Full DetailLevel Pipeline ──────────────────────────────

#[test]
fn e2e_detail_level_reduces_tokens_across_levels() {
    let mut sample = serde_json::json!({
        "matches": (0..50).map(|i| serde_json::json!({
            "key": format!("lesson:{}", i),
            "value": format!("A long description of pattern {} with implementation details and context that spans many characters to simulate real output.", i),
            "score": 0.9 - (i as f64 * 0.01),
        })).collect::<Vec<_>>(),
        "stats": {"total": 50, "filtered": 10},
    });

    let full_size = serde_json::to_string(&sample).unwrap().len();

    let mut standard = sample.clone();
    apply_detail_level(&mut standard, DetailLevel::Standard);
    let standard_size = serde_json::to_string(&standard).unwrap().len();

    apply_detail_level(&mut sample, DetailLevel::Minimal);
    let minimal_size = serde_json::to_string(&sample).unwrap().len();

    // Full > Standard > Minimal
    assert!(
        full_size > standard_size,
        "Full ({}) should be larger than Standard ({})",
        full_size,
        standard_size
    );
    assert!(
        standard_size > minimal_size,
        "Standard ({}) should be larger than Minimal ({})",
        standard_size,
        minimal_size
    );

    // Minimal should be at least 3x smaller than Full
    let ratio = full_size as f64 / minimal_size as f64;
    assert!(
        ratio >= 3.0,
        "Minimal should be >=3x smaller: {:.1}x",
        ratio
    );
}

// ── E2E Test 2: Suggestions + Session Hints Integration ────────────────

#[test]
fn e2e_session_hints_evolve_from_static_to_dynamic() {
    // Phase 1: Static suggestions (no session history)
    let static_hints = suggestions::get_suggestions("touring_ast_find", 3);
    assert!(
        !static_hints.is_empty(),
        "Static suggestions should always return results"
    );

    // Phase 2: Session-aware hints evolve with history
    let mut engine = SessionHintEngine::new();
    assert_eq!(engine.intent(), SessionIntent::Exploring);

    // Simulate a review workflow
    engine.record_call("touring_detect_changes");
    engine.record_call("touring_wiring_audit");
    engine.record_call("touring_gotcha");
    assert_eq!(
        engine.intent(),
        SessionIntent::Reviewing,
        "Should detect review intent"
    );

    // Get dynamic hints (should differ from static)
    let dynamic_hints = engine.get_hints("touring_gotcha", 3);
    assert!(
        !dynamic_hints.is_empty(),
        "Dynamic hints should return results"
    );

    // Dynamic hints should NOT suggest recently-called tools
    assert!(
        !dynamic_hints
            .iter()
            .any(|h| h.tool.as_str() == "touring_gotcha"),
        "Should not suggest tool just called"
    );

    // Phase 3: Intent should shift when workflow changes
    engine.record_call("touring_ast_find");
    engine.record_call("touring_speculate");
    engine.record_call("touring_refactor");
    engine.record_call("touring_ast_overview");
    engine.record_call("touring_speculate");
    // Now predominantly refactoring tools
    assert_eq!(
        engine.intent(),
        SessionIntent::Refactoring,
        "Should shift to refactoring intent"
    );
}

// ── E2E Test 3: Diff Parser → Risk Scoring Pipeline ────────────────────

#[test]
fn e2e_diff_to_risk_pipeline() {
    // Step 1: Parse a diff
    let diff = r#"diff --git a/src/auth.rs b/src/auth.rs
--- a/src/auth.rs
+++ b/src/auth.rs
@@ -15,3 +15,8 @@ fn authenticate() {
+    validate_token(token);
+    check_permissions(user);
+    audit_log(user, action);
+    rate_limit(ip);
+    encrypt_response(data);
diff --git a/src/handler.rs b/src/handler.rs
--- a/src/handler.rs
+++ b/src/handler.rs
@@ -42,2 +42,4 @@ fn handle_request() {
+    parse_body(request);
+    validate_input(body);
"#;

    let hunks = diff_parser::parse_unified_diff(diff);
    assert_eq!(hunks.len(), 2, "Should find 2 hunks in 2 files");
    assert_eq!(hunks[0].file_path, "src/auth.rs");
    assert_eq!(hunks[1].file_path, "src/handler.rs");

    // Step 2: Map hunks to symbols
    let symbols_fn = |path: &str| -> Vec<(String, String, u32, u32)> {
        match path {
            "src/auth.rs" => vec![
                ("authenticate".into(), "Function".into(), 10, 30),
                ("validate_session".into(), "Function".into(), 35, 50),
            ],
            "src/handler.rs" => vec![("handle_request".into(), "Function".into(), 40, 60)],
            _ => vec![],
        }
    };

    let affected = diff_parser::map_hunks_to_symbols(&hunks, symbols_fn);
    assert_eq!(affected.len(), 2, "authenticate + handle_request overlap");
    assert!(
        affected.iter().any(|a| a.name == "authenticate"),
        "authenticate should be affected"
    );
    assert!(
        affected.iter().any(|a| a.name == "handle_request"),
        "handle_request should be affected"
    );

    // Step 3: Compute criticality for affected symbols
    let crit = risk_scoring::compute_criticality(
        10,    // fan-in: 10 callers
        3,     // fan-out: 3 deps
        0.8,   // high churn
        false, // no tests!
        2,     // 2 gotchas
    );
    assert!(crit.score > 0.5, "High-risk symbol: {}", crit.score);
    assert!(!crit.factors.is_empty(), "Should have contributing factors");

    // Step 4: Build change risk report
    let report = risk_scoring::build_change_risk_report(risk_scoring::ChangeRiskInput {
        changed_files: vec!["src/auth.rs".into(), "src/handler.rs".into()],
        affected_files: vec!["src/middleware.rs".into(), "src/session.rs".into()],
        affected_symbols: 4,
        test_gaps: vec![risk_scoring::TestGap {
            symbol: "authenticate".into(),
            file_path: "src/auth.rs".into(),
            criticality: crit.score,
        }],
        hotspots: vec![risk_scoring::Hotspot {
            symbol: "authenticate".into(),
            file_path: "src/auth.rs".into(),
            criticality: crit.score,
            factors: crit.factors.clone(),
        }],
        gotcha_warnings: vec!["[high] auth.rs: known session leak".into()],
        wiring_score: 0.6,
        detail_level: DetailLevel::Standard,
    });

    let risk = report
        .get("overall_risk")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    assert!(risk > 0.3, "Should be medium+ risk: {}", risk);
    assert!(
        report.get("suggested_actions").is_some(),
        "Should have suggested actions"
    );
}

// ── E2E Test 4: Hybrid Search Fusion ───────────────────────────────────

#[test]
fn e2e_hybrid_search_fuses_and_boosts() {
    let fts_results = vec![
        ("parse_config".into(), 5.2),
        ("ConfigStore::new".into(), 3.1),
        ("validate_config".into(), 2.0),
    ];
    let vec_results = vec![
        ("ConfigStore::new".into(), 0.92),
        ("config_loader".into(), 0.85),
        ("parse_config".into(), 0.70),
    ];

    // PascalCase query → should boost struct-like names
    let results = hybrid_search::hybrid_search(&fts_results, &vec_results, "ConfigStore", 5);

    assert!(!results.is_empty());
    // ConfigStore::new appears in both lists AND gets PascalCase boost
    assert_eq!(
        results[0].key, "ConfigStore::new",
        "ConfigStore::new should rank first (both lists + boost)"
    );
    assert!(
        results[0].fused_score > results[1].fused_score,
        "Top result should have highest fused score"
    );
}

// ── E2E Test 5: Summary Cache Round-Trip ───────────────────────────────

#[test]
fn e2e_summary_cache_lifecycle() {
    let cache = SummaryCache::open(std::path::Path::new(":memory:")).unwrap();

    // Set various summaries
    let wiring = serde_json::json!({"orphans": 100, "modules": 50, "score": 0.45});
    let health = serde_json::json!({"symbols": 1000, "files": 200, "e2e": 0.75});

    cache.set("wiring:status", &wiring, 60).unwrap();
    cache.set("health:snapshot", &health, 30).unwrap();

    // Retrieve
    let got_wiring = cache.get("wiring:status").unwrap();
    assert_eq!(got_wiring["orphans"], 100);

    // Update
    let updated = serde_json::json!({"orphans": 50, "modules": 55, "score": 0.65});
    cache.set("wiring:status", &updated, 60).unwrap();
    let got = cache.get("wiring:status").unwrap();
    assert_eq!(got["orphans"], 50, "Should reflect updated value");

    // Invalidate specific
    cache.invalidate("health:snapshot").unwrap();
    assert!(
        cache.get("health:snapshot").is_none(),
        "Should be invalidated"
    );
    assert!(
        cache.get("wiring:status").is_some(),
        "Other keys unaffected"
    );

    // Count
    assert_eq!(cache.count(), 1);
}

// ── E2E Test 6: Minimal Context Aggregation ────────────────────────────

#[test]
fn e2e_minimal_context_aggregates_all_signals() {
    let result = context_tools::build_minimal_context(context_tools::MinimalContextInput {
        symbol_count: 50000,
        file_count: 2000,
        orphan_count: 15000,
        e2e_health: 0.65,
        rl_reward: 0.42,
        top_gotchas: vec![
            "[high] parser.rs: stack overflow on deep AST".into(),
            "[medium] cache.rs: TTL not respected".into(),
        ],
        task_hint: Some("debug parser crash"),
        detail_level: DetailLevel::Standard,
    });

    // Verify structure
    assert!(result.get("stats").is_some(), "Must have stats");
    assert!(result.get("risk").is_some(), "Must have risk");
    assert!(result.get("top_issues").is_some(), "Must have issues");
    assert!(
        result.get("suggested_tools").is_some(),
        "Must have suggestions"
    );
    assert!(
        result.get("task_context").is_some(),
        "Standard should include task context"
    );

    // Verify risk is computed
    let risk_score = result
        .pointer("/risk/score")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(0.0);
    assert!(risk_score > 0.0, "Should compute non-zero risk");

    // Verify minimal is compact
    let minimal = context_tools::build_minimal_context(context_tools::MinimalContextInput {
        symbol_count: 50000,
        file_count: 2000,
        orphan_count: 15000,
        e2e_health: 0.65,
        rl_reward: 0.42,
        top_gotchas: vec!["a".into(), "b".into(), "c".into(), "d".into(), "e".into()],
        task_hint: Some("debug"),
        detail_level: DetailLevel::Minimal,
    });
    assert!(
        minimal.get("task_context").is_none(),
        "Minimal should NOT include task context"
    );
    let issues = minimal
        .get("top_issues")
        .and_then(|v| v.as_array())
        .unwrap();
    assert!(issues.len() <= 3, "Minimal limits to 3 issues");
}

// ── E2E Test 7: Full Workflow Integration ──────────────────────────────

#[test]
fn e2e_full_workflow_minimal_context_to_detect_changes() {
    // Simulate the recommended workflow:
    // 1. Call get_minimal_context → get risk overview
    // 2. If risk medium+, call detect_changes
    // 3. Use session hints to guide next steps

    // Step 1: Minimal context
    let context = context_tools::build_minimal_context(context_tools::MinimalContextInput {
        symbol_count: 10000,
        file_count: 500,
        orphan_count: 8000,
        e2e_health: 0.5,
        rl_reward: 0.1,
        top_gotchas: vec!["[high] known issue".into()],
        task_hint: Some("review changes"),
        detail_level: DetailLevel::Minimal,
    });
    let risk_level = context
        .pointer("/risk/level")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    assert!(
        risk_level == "medium" || risk_level == "high",
        "High orphan ratio should give medium+ risk: {}",
        risk_level
    );

    // Step 2: Detect changes (simulated)
    let report = risk_scoring::build_change_risk_report(risk_scoring::ChangeRiskInput {
        changed_files: vec!["src/main.rs".into()],
        affected_files: vec!["src/lib.rs".into(), "src/utils.rs".into()],
        affected_symbols: 10,
        test_gaps: vec![],
        hotspots: vec![],
        gotcha_warnings: vec!["[medium] main.rs: known gotcha".into()],
        wiring_score: 0.5,
        detail_level: DetailLevel::Standard,
    });
    assert!(report.get("overall_risk").is_some());

    // Step 3: Session hints guide workflow
    let mut engine = SessionHintEngine::new();
    engine.record_call("touring_minimal_context");
    engine.record_call("touring_detect_changes");
    assert_eq!(engine.intent(), SessionIntent::Reviewing);

    let hints = engine.get_hints("touring_detect_changes", 3);
    assert!(
        !hints.is_empty(),
        "Should suggest next steps after detect_changes"
    );
    // Should suggest wiring audit or gotcha (review workflow)
    let tools: Vec<&str> = hints.iter().map(|h| h.tool.as_str()).collect();
    assert!(
        tools
            .iter()
            .any(|t| t.contains("wiring") || t.contains("gotcha")),
        "Review hints should include wiring or gotcha: {:?}",
        tools
    );
}

// ── E2E Test 8: Token Savings Measurement ──────────────────────────────

#[test]
fn e2e_token_savings_meet_targets() {
    // Generate realistic large output
    let large_output = serde_json::json!({
        "rlm_matches": (0..40).map(|i| serde_json::json!({
            "key": format!("pattern:error_handling:retry_logic_{}", i),
            "tier": "semantic",
            "value": format!("When encountering a transient failure in module {} the recommended pattern is exponential backoff with jitter, starting at 100ms and capping at 30s. This applies to network calls, database operations, and external API invocations. The retry count should be configurable per-operation.", i),
            "entry_type": "lesson",
            "score": 0.95 - (i as f64 * 0.015),
            "access_count": 20 - i,
        })).collect::<Vec<_>>(),
        "semantic_matches": (0..20).map(|i| serde_json::json!({
            "id": i,
            "content": format!("Code pattern {} demonstrates the builder pattern with validation, ensuring all required fields are set before construction completes.", i),
            "metadata": {"source": "auto-extract", "confidence": 0.9},
            "score": 0.88 - (i as f64 * 0.02),
        })).collect::<Vec<_>>(),
        "graph_neighbors": {
            "file": "/project/src/main.rs",
            "imports": (0..15).map(|i| format!("dep_{}.rs", i)).collect::<Vec<_>>(),
            "imported_by": (0..10).map(|i| format!("consumer_{}.rs", i)).collect::<Vec<_>>(),
        },
    });

    let full_size = serde_json::to_string(&large_output).unwrap().len();

    let mut standard = large_output.clone();
    apply_detail_level(&mut standard, DetailLevel::Standard);
    let standard_size = serde_json::to_string(&standard).unwrap().len();

    let mut minimal = large_output;
    apply_detail_level(&mut minimal, DetailLevel::Minimal);
    let minimal_size = serde_json::to_string(&minimal).unwrap().len();

    let standard_ratio = full_size as f64 / standard_size as f64;
    let minimal_ratio = full_size as f64 / minimal_size as f64;

    // Targets from plan: Standard >=1.5x, Minimal >=3x
    assert!(
        standard_ratio >= 1.5,
        "Standard savings target: >=1.5x, got {:.1}x ({} → {} bytes)",
        standard_ratio,
        full_size,
        standard_size
    );
    assert!(
        minimal_ratio >= 3.0,
        "Minimal savings target: >=3.0x, got {:.1}x ({} → {} bytes)",
        minimal_ratio,
        full_size,
        minimal_size
    );

    eprintln!(
        "TOKEN SAVINGS VERIFIED: Standard={:.1}x, Minimal={:.1}x",
        standard_ratio, minimal_ratio
    );
}
