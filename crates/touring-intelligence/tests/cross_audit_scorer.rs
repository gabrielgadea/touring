//! Cross-audit integration tests for OpportunityScorer.
//!
//! These tests verify the **documented purpose** of each component, not just
//! that it compiles. Purpose: "Analyze telemetry signals and produce a ranked
//! list of EvolutionCandidate improvements for the Touring workspace."
//!
//! Audit dimensions verified:
//! 1. CONTRACT   — composite_score = impact * feasibility (mathematical invariant)
//! 2. SORTING    — candidates always sorted DESC by composite_score
//! 3. TRIGGER    — all 10 categories trigger at correct threshold conditions
//! 4. ISOLATION  — categories do not interfere with each other
//! 5. PIPELINE   — full telemetry → scorer → filter → summary E2E flow
//! 6. ROBUSTNESS — empty, minimal, and maximal snapshots all handled correctly

use std::collections::HashMap;

use touring_intelligence::rl::meta::opportunity_scorer::{
    EvolutionCandidate, OpportunityCategory, OpportunityScorer, TelemetrySnapshot,
};

// ── Helpers ──────────────────────────────────────────────────────────────────

fn scorer() -> OpportunityScorer {
    OpportunityScorer::new()
}

fn empty_snapshot() -> TelemetrySnapshot {
    TelemetrySnapshot::default()
}

/// Snapshot that activates ALL 10 opportunity categories simultaneously.
fn all_signals_snapshot() -> TelemetrySnapshot {
    TelemetrySnapshot {
        // IntegrationGap
        wiring_orphan_count: 5,
        wiring_orphans: vec!["SymA".to_string(), "SymB".to_string()],
        // ConvergenceIssue
        mean_td_error: Some(0.9),
        ema_reward: Some(0.2),
        rl_update_count: 1000,
        // PerformanceHotspot
        avg_hook_latency_ms: Some(25.0),
        // DriftResponse
        drift_detected: true,
        // TestCoverageGap
        undertested_modules: vec!["alpha".to_string(), "beta".to_string()],
        // HyperparamDegradation
        optimizer_reset_count: 3,
        // MissingIntegration
        low_integration_modules: vec![("mod_x".to_string(), 0.2)],
        // CacheInefficiency
        parser_cache_hit_rate: Some(0.40),
        // GotchaRecurrence
        recurring_gotcha_count: 8,
        // CognitiveBottleneck
        cognitive_engine_degraded: true,
        linucb_arms: HashMap::new(),
    }
}

// ── CONTRACT: composite_score = impact * feasibility ─────────────────────────

#[test]
fn contract_composite_score_is_impact_times_feasibility() {
    let candidates = scorer().analyze(&all_signals_snapshot());
    assert!(
        !candidates.is_empty(),
        "all-signals snapshot must produce candidates"
    );

    for c in &candidates {
        let expected = c.impact_score * c.feasibility_score;
        assert!(
            (c.composite_score - expected).abs() < 1e-10,
            "candidate '{}': composite_score {:.6} != impact {:.6} * feasibility {:.6} = {:.6}",
            c.id,
            c.composite_score,
            c.impact_score,
            c.feasibility_score,
            expected
        );
    }
}

#[test]
fn contract_scores_clamped_to_unit_interval() {
    let candidates = scorer().analyze(&all_signals_snapshot());
    for c in &candidates {
        assert!(
            c.impact_score >= 0.0 && c.impact_score <= 1.0,
            "impact_score out of [0,1]: {} = {:.4}",
            c.id,
            c.impact_score
        );
        assert!(
            c.feasibility_score >= 0.0 && c.feasibility_score <= 1.0,
            "feasibility_score out of [0,1]: {} = {:.4}",
            c.id,
            c.feasibility_score
        );
        assert!(
            c.composite_score >= 0.0 && c.composite_score <= 1.0,
            "composite_score out of [0,1]: {} = {:.4}",
            c.id,
            c.composite_score
        );
    }
}

// ── SORTING: candidates always DESC by composite_score ────────────────────────

#[test]
fn sorting_invariant_all_signals() {
    let candidates = scorer().analyze(&all_signals_snapshot());
    assert_sorted_desc(&candidates);
}

#[test]
fn sorting_invariant_single_category_each() {
    let snapshots: &[(&str, TelemetrySnapshot)] = &[
        (
            "IntegrationGap",
            TelemetrySnapshot {
                wiring_orphan_count: 3,
                wiring_orphans: vec!["X".to_string()],
                ..Default::default()
            },
        ),
        (
            "ConvergenceIssue",
            TelemetrySnapshot {
                mean_td_error: Some(0.8),
                ..Default::default()
            },
        ),
        (
            "PerformanceHotspot",
            TelemetrySnapshot {
                avg_hook_latency_ms: Some(20.0),
                ..Default::default()
            },
        ),
        (
            "DriftResponse",
            TelemetrySnapshot {
                drift_detected: true,
                ..Default::default()
            },
        ),
        (
            "TestCoverageGap",
            TelemetrySnapshot {
                undertested_modules: vec!["mod_z".to_string()],
                ..Default::default()
            },
        ),
        (
            "HyperparamDegradation",
            TelemetrySnapshot {
                optimizer_reset_count: 2,
                ..Default::default()
            },
        ),
        (
            "MissingIntegration",
            TelemetrySnapshot {
                low_integration_modules: vec![("low_mod".to_string(), 0.3)],
                ..Default::default()
            },
        ),
        (
            "CacheInefficiency",
            TelemetrySnapshot {
                parser_cache_hit_rate: Some(0.50),
                ..Default::default()
            },
        ),
        (
            "GotchaRecurrence",
            TelemetrySnapshot {
                recurring_gotcha_count: 10,
                ..Default::default()
            },
        ),
        (
            "CognitiveBottleneck",
            TelemetrySnapshot {
                cognitive_engine_degraded: true,
                ..Default::default()
            },
        ),
    ];

    for (name, snapshot) in snapshots {
        let candidates = scorer().analyze(snapshot);
        assert_sorted_desc(&candidates);
        assert_eq!(
            candidates.len(),
            1,
            "single signal '{name}' must produce exactly 1 candidate"
        );
    }
}

fn assert_sorted_desc(candidates: &[EvolutionCandidate]) {
    for w in candidates.windows(2) {
        assert!(
            w[0].composite_score >= w[1].composite_score,
            "sorting invariant violated: {:.4} < {:.4} ({} before {})",
            w[0].composite_score,
            w[1].composite_score,
            w[0].id,
            w[1].id
        );
    }
}

// ── TRIGGER: all 10 categories trigger at correct thresholds ─────────────────

#[test]
fn trigger_integration_gap_on_orphans() {
    let snapshot = TelemetrySnapshot {
        wiring_orphan_count: 2,
        wiring_orphans: vec!["OrphanFn".to_string()],
        ..Default::default()
    };
    let candidates = scorer().analyze(&snapshot);
    assert_category_present(
        &candidates,
        &OpportunityCategory::IntegrationGap,
        "IntegrationGap must trigger on wiring_orphan_count > 0",
    );
    let c = find_category(&candidates, &OpportunityCategory::IntegrationGap).unwrap();
    assert!(
        c.signals.iter().any(|s| s.contains("wiring_orphan_count")),
        "signal must reference wiring_orphan_count"
    );
    assert!(
        c.suggested_implementation.contains("pub(crate)"),
        "implementation hint must include pub(crate) suggestion"
    );
}

#[test]
fn trigger_convergence_issue_above_threshold() {
    let snapshot = TelemetrySnapshot {
        mean_td_error: Some(0.6), // above 0.5 threshold
        ..Default::default()
    };
    let candidates = scorer().analyze(&snapshot);
    assert_category_present(
        &candidates,
        &OpportunityCategory::ConvergenceIssue,
        "ConvergenceIssue must trigger when mean_td_error > 0.5",
    );
}

#[test]
fn no_trigger_convergence_at_threshold_boundary() {
    // Exactly at threshold — must NOT trigger (strictly >)
    let snapshot = TelemetrySnapshot {
        mean_td_error: Some(0.5),
        ..Default::default()
    };
    let candidates = scorer().analyze(&snapshot);
    assert_category_absent(
        &candidates,
        &OpportunityCategory::ConvergenceIssue,
        "ConvergenceIssue must not trigger at exactly threshold 0.5",
    );
}

#[test]
fn trigger_performance_hotspot_above_10ms() {
    let snapshot = TelemetrySnapshot {
        avg_hook_latency_ms: Some(10.1),
        ..Default::default()
    };
    let candidates = scorer().analyze(&snapshot);
    assert_category_present(
        &candidates,
        &OpportunityCategory::PerformanceHotspot,
        "PerformanceHotspot must trigger above 10ms",
    );
}

#[test]
fn no_trigger_performance_at_threshold_boundary() {
    let snapshot = TelemetrySnapshot {
        avg_hook_latency_ms: Some(10.0), // at threshold — not above
        ..Default::default()
    };
    let candidates = scorer().analyze(&snapshot);
    assert_category_absent(
        &candidates,
        &OpportunityCategory::PerformanceHotspot,
        "PerformanceHotspot must not trigger at exactly 10.0ms",
    );
}

#[test]
fn trigger_drift_response_when_detected() {
    let snapshot = TelemetrySnapshot {
        drift_detected: true,
        ..Default::default()
    };
    let candidates = scorer().analyze(&snapshot);
    assert_category_present(
        &candidates,
        &OpportunityCategory::DriftResponse,
        "DriftResponse must trigger on drift_detected=true",
    );
    // Verify impact: DriftResponse has base_impact=0.85 with impact_mod=1.0
    let c = find_category(&candidates, &OpportunityCategory::DriftResponse).unwrap();
    assert!(
        (c.impact_score - 0.85).abs() < 0.001,
        "DriftResponse impact must be 0.85"
    );
}

#[test]
fn trigger_test_coverage_gap_on_undertested() {
    let snapshot = TelemetrySnapshot {
        undertested_modules: vec!["mod_a".to_string()],
        ..Default::default()
    };
    let candidates = scorer().analyze(&snapshot);
    assert_category_present(
        &candidates,
        &OpportunityCategory::TestCoverageGap,
        "TestCoverageGap must trigger on undertested modules",
    );
}

#[test]
fn trigger_hyperparam_degradation_on_resets() {
    let snapshot = TelemetrySnapshot {
        optimizer_reset_count: 1,
        ..Default::default()
    };
    let candidates = scorer().analyze(&snapshot);
    assert_category_present(
        &candidates,
        &OpportunityCategory::HyperparamDegradation,
        "HyperparamDegradation must trigger on optimizer_reset_count > 0",
    );
}

#[test]
fn trigger_missing_integration_below_threshold() {
    let snapshot = TelemetrySnapshot {
        low_integration_modules: vec![("isolated_mod".to_string(), 0.1)],
        ..Default::default()
    };
    let candidates = scorer().analyze(&snapshot);
    assert_category_present(
        &candidates,
        &OpportunityCategory::MissingIntegration,
        "MissingIntegration must trigger for modules with integration_score < 0.5",
    );
}

#[test]
fn no_trigger_missing_integration_above_threshold() {
    let snapshot = TelemetrySnapshot {
        low_integration_modules: vec![("ok_mod".to_string(), 0.7)], // above 0.5
        ..Default::default()
    };
    let candidates = scorer().analyze(&snapshot);
    assert_category_absent(
        &candidates,
        &OpportunityCategory::MissingIntegration,
        "MissingIntegration must not trigger when integration_score >= 0.5",
    );
}

#[test]
fn trigger_cache_inefficiency_below_70_percent() {
    let snapshot = TelemetrySnapshot {
        parser_cache_hit_rate: Some(0.69),
        ..Default::default()
    };
    let candidates = scorer().analyze(&snapshot);
    assert_category_present(
        &candidates,
        &OpportunityCategory::CacheInefficiency,
        "CacheInefficiency must trigger at 69% (below 70% threshold)",
    );
}

#[test]
fn no_trigger_cache_inefficiency_at_exact_threshold() {
    let snapshot = TelemetrySnapshot {
        parser_cache_hit_rate: Some(0.70), // exactly at threshold — not below
        ..Default::default()
    };
    let candidates = scorer().analyze(&snapshot);
    assert_category_absent(
        &candidates,
        &OpportunityCategory::CacheInefficiency,
        "CacheInefficiency must not trigger at exactly 70% hit rate",
    );
}

#[test]
fn trigger_gotcha_recurrence_above_5() {
    let snapshot = TelemetrySnapshot {
        recurring_gotcha_count: 6, // one above threshold
        ..Default::default()
    };
    let candidates = scorer().analyze(&snapshot);
    assert_category_present(
        &candidates,
        &OpportunityCategory::GotchaRecurrence,
        "GotchaRecurrence must trigger when count > 5",
    );
}

#[test]
fn no_trigger_gotcha_recurrence_at_5() {
    let snapshot = TelemetrySnapshot {
        recurring_gotcha_count: 5, // at threshold (not above)
        ..Default::default()
    };
    let candidates = scorer().analyze(&snapshot);
    assert_category_absent(
        &candidates,
        &OpportunityCategory::GotchaRecurrence,
        "GotchaRecurrence must not trigger at exactly 5 (boundary is strict >)",
    );
}

#[test]
fn trigger_cognitive_bottleneck_when_degraded() {
    let snapshot = TelemetrySnapshot {
        cognitive_engine_degraded: true,
        ..Default::default()
    };
    let candidates = scorer().analyze(&snapshot);
    assert_category_present(
        &candidates,
        &OpportunityCategory::CognitiveBottleneck,
        "CognitiveBottleneck must trigger on cognitive_engine_degraded=true",
    );
    let c = find_category(&candidates, &OpportunityCategory::CognitiveBottleneck).unwrap();
    assert!(
        (c.impact_score - 0.75).abs() < 0.001,
        "CognitiveBottleneck base_impact must be 0.75"
    );
    assert!(
        (c.feasibility_score - 0.70).abs() < 0.001,
        "CognitiveBottleneck base_feasibility must be 0.70"
    );
}

#[test]
fn no_trigger_cognitive_bottleneck_when_healthy() {
    let snapshot = TelemetrySnapshot {
        cognitive_engine_degraded: false,
        ..Default::default()
    };
    let candidates = scorer().analyze(&snapshot);
    assert_category_absent(
        &candidates,
        &OpportunityCategory::CognitiveBottleneck,
        "CognitiveBottleneck must not trigger when engine is healthy",
    );
}

// ── ISOLATION: categories do not interfere with each other ───────────────────

#[test]
fn isolation_each_category_independent() {
    // Activate each signal individually, ensure only that category appears.
    let only_drift = TelemetrySnapshot {
        drift_detected: true,
        ..Default::default()
    };
    let candidates = scorer().analyze(&only_drift);
    assert_eq!(
        candidates.len(),
        1,
        "only DriftResponse signal → exactly 1 candidate"
    );

    let only_cognitive = TelemetrySnapshot {
        cognitive_engine_degraded: true,
        ..Default::default()
    };
    let candidates = scorer().analyze(&only_cognitive);
    assert_eq!(
        candidates.len(),
        1,
        "only CognitiveBottleneck signal → exactly 1 candidate"
    );
}

#[test]
fn isolation_all_10_categories_present_when_all_signals_active() {
    let candidates = scorer().analyze(&all_signals_snapshot());

    let expected_categories = [
        OpportunityCategory::IntegrationGap,
        OpportunityCategory::ConvergenceIssue,
        OpportunityCategory::PerformanceHotspot,
        OpportunityCategory::DriftResponse,
        OpportunityCategory::TestCoverageGap,
        OpportunityCategory::HyperparamDegradation,
        OpportunityCategory::MissingIntegration,
        OpportunityCategory::CacheInefficiency,
        OpportunityCategory::GotchaRecurrence,
        OpportunityCategory::CognitiveBottleneck,
    ];

    for expected in &expected_categories {
        assert_category_present(
            &candidates,
            expected,
            &format!("all-signals snapshot must produce {:?}", expected),
        );
    }

    // Total must equal exactly 10 (1 per category)
    assert_eq!(
        candidates.len(),
        10,
        "all-signals snapshot must produce exactly 10 candidates (one per category)"
    );
}

// ── PIPELINE: full E2E telemetry → scorer → filter → summary ─────────────────

#[test]
fn pipeline_empty_snapshot_produces_no_candidates() {
    let candidates = scorer().analyze(&empty_snapshot());
    assert!(
        candidates.is_empty(),
        "empty snapshot must produce zero candidates (nothing to evolve)"
    );
}

#[test]
fn pipeline_filter_by_score_removes_low_quality() {
    let candidates = scorer().analyze(&all_signals_snapshot());
    let high_quality = OpportunityScorer::filter_by_score(candidates.clone(), 0.7);

    // All surviving candidates must meet the threshold
    for c in &high_quality {
        assert!(
            c.composite_score >= 0.7,
            "filter_by_score(0.7) let through candidate with score {:.4}",
            c.composite_score
        );
    }

    // The filtered set must be a subset of the original
    assert!(high_quality.len() <= candidates.len());
}

#[test]
fn pipeline_summary_reflects_actual_candidates() {
    let candidates = scorer().analyze(&all_signals_snapshot());
    let total = candidates.len();
    let summary = OpportunityScorer::summary(&candidates);

    assert_eq!(
        summary.total, total,
        "summary.total must match candidate count"
    );
    assert!(
        summary.top_candidate.is_some(),
        "summary must have a top candidate"
    );
    assert!(
        summary.mean_score > 0.0 && summary.mean_score <= 1.0,
        "summary.mean_score must be in (0, 1]"
    );

    // Top candidate must be the actual highest-scoring candidate
    let top = summary.top_candidate.as_ref().unwrap();
    assert_eq!(
        top.id, candidates[0].id,
        "top_candidate must match first element of sorted list"
    );

    // by_category must account for all candidates
    let category_total: usize = summary.by_category.values().sum();
    assert_eq!(
        category_total, total,
        "by_category counts must sum to total"
    );
}

#[test]
fn pipeline_summary_empty_produces_defaults() {
    let summary = OpportunityScorer::summary(&[]);
    assert_eq!(summary.total, 0);
    assert!((summary.mean_score - 0.0).abs() < 1e-10);
    assert!(summary.top_candidate.is_none());
    assert!(summary.by_category.is_empty());
}

// ── ROBUSTNESS: purpose-level edge cases ─────────────────────────────────────

#[test]
fn robustness_candidate_signals_are_non_empty() {
    // Purpose: signals field documents the telemetry evidence that triggered the candidate.
    // A candidate without signals is not useful for the engineer implementing it.
    let candidates = scorer().analyze(&all_signals_snapshot());
    for c in &candidates {
        assert!(
            !c.signals.is_empty(),
            "candidate '{}' has no signals — engineers need evidence",
            c.id
        );
    }
}

#[test]
fn robustness_candidate_description_mentions_threshold() {
    // CacheInefficiency must mention the threshold in its description
    let snapshot = TelemetrySnapshot {
        parser_cache_hit_rate: Some(0.50),
        ..Default::default()
    };
    let candidates = scorer().analyze(&snapshot);
    let c = find_category(&candidates, &OpportunityCategory::CacheInefficiency).unwrap();
    assert!(
        c.description.contains("70%"),
        "CacheInefficiency description must mention the 70% threshold"
    );
}

#[test]
fn robustness_suggested_implementation_non_empty_for_all() {
    // Purpose: suggested_implementation guides the evolution subagent.
    // Empty implementation hints block the skill from executing.
    let candidates = scorer().analyze(&all_signals_snapshot());
    for c in &candidates {
        assert!(
            !c.suggested_implementation.is_empty(),
            "candidate '{}' has empty suggested_implementation",
            c.id
        );
    }
}

#[test]
fn robustness_candidate_id_is_unique() {
    let candidates = scorer().analyze(&all_signals_snapshot());
    let ids: std::collections::HashSet<_> = candidates.iter().map(|c| &c.id).collect();
    assert_eq!(
        ids.len(),
        candidates.len(),
        "candidate IDs must be unique — duplicates would break deduplication"
    );
}

#[test]
fn robustness_missing_integration_generates_per_module_candidate() {
    // Multiple low-integration modules → multiple candidates
    let snapshot = TelemetrySnapshot {
        low_integration_modules: vec![("mod_a".to_string(), 0.1), ("mod_b".to_string(), 0.2)],
        ..Default::default()
    };
    let candidates = scorer().analyze(&snapshot);
    let integration_candidates: Vec<_> = candidates
        .iter()
        .filter(|c| c.category == OpportunityCategory::MissingIntegration)
        .collect();
    assert_eq!(
        integration_candidates.len(),
        2,
        "2 low-integration modules must generate 2 MissingIntegration candidates"
    );
    // Each must reference its specific module
    assert!(
        integration_candidates
            .iter()
            .any(|c| c.id.contains("mod_a"))
    );
    assert!(
        integration_candidates
            .iter()
            .any(|c| c.id.contains("mod_b"))
    );
}

// ── Helper functions ──────────────────────────────────────────────────────────

fn assert_category_present(
    candidates: &[EvolutionCandidate],
    cat: &OpportunityCategory,
    msg: &str,
) {
    assert!(
        candidates.iter().any(|c| &c.category == cat),
        "{msg}: category {:?} not found in candidates: [{}]",
        cat,
        candidates
            .iter()
            .map(|c| c.category.label())
            .collect::<Vec<_>>()
            .join(", ")
    );
}

fn assert_category_absent(candidates: &[EvolutionCandidate], cat: &OpportunityCategory, msg: &str) {
    assert!(
        !candidates.iter().any(|c| &c.category == cat),
        "{msg}: category {:?} unexpectedly found",
        cat
    );
}

fn find_category<'a>(
    candidates: &'a [EvolutionCandidate],
    cat: &OpportunityCategory,
) -> Option<&'a EvolutionCandidate> {
    candidates.iter().find(|c| &c.category == cat)
}
