//! Integration tests for the new RL and bandit modules.
#![allow(clippy::indexing_slicing, clippy::needless_range_loop)]

use touring_intelligence::rl::bandit::linucb::{FEATURE_DIM, NUM_ARMS, extract_features};
use touring_intelligence::rl::rl::qtable::{LearningParams, QLearning, QTable};
use touring_intelligence::rl::rl::risk_adjusted::RiskAdjustedQLearning;

/// Simulate 100 tool-execution iterations through QTable + LinUCB pipeline.
///
/// Verifies that:
/// - QTable learns (not all Q-values remain at initial_q)
/// - LinUCB selects different arms over time (not stuck on one)
#[test]
fn qtable_linucb_pipeline() {
    let mut qtable = QTable::new();
    let mut bandit = touring_intelligence::rl::LinUCBBandit::new();

    // Track which arms are selected
    let mut arm_selections = [0u32; NUM_ARMS];

    for i in 0..100u64 {
        // Simulate varying context
        let file_type = match i % 4 {
            0 => "python",
            1 => "rust",
            2 => "typescript",
            _ => "other",
        };
        let file_size = (i * 37 % 2000) as usize;
        let session_turn = (i * 3 % 80) as usize;
        let recent_errors = (i % 5) as usize;
        let cila_level = (i % 7) as u8;

        let features = extract_features(
            file_type,
            file_size,
            session_turn,
            recent_errors,
            cila_level,
        );
        assert_eq!(features.len(), FEATURE_DIM);

        // Bandit selects arm
        let (arm_idx, _score) = bandit.select_arm(&features);
        assert!(arm_idx < NUM_ARMS);

        // SAFETY: arm_idx is validated above
        #[allow(clippy::indexing_slicing)]
        {
            arm_selections[arm_idx] += 1;
        }

        // Simulate reward based on arm choice (some arms "better" for certain contexts)
        let reward = match arm_idx {
            7 => 0.9, // full enrichment often good
            0 => 0.3, // no context often bad
            _ => 0.5 + (arm_idx as f64) * 0.05,
        };

        bandit.update(arm_idx, &features, reward);

        // QTable: state = file_type hash, action = arm_idx
        let state = i % 10;
        let action = arm_idx as u64;
        let next_state = (i + 1) % 10;
        let td_error = qtable.update(state, action, reward, next_state, Some(0), false);

        // TD error should be finite
        assert!(
            td_error.is_finite(),
            "TD error should be finite at iteration {i}"
        );
    }

    // Verify QTable learned: at least some Q-values should be non-zero
    let mut has_nonzero_q = false;
    for state in 0..10u64 {
        if let Some(action) = qtable.best_action(state) {
            let q = qtable.get_q(state, action);
            if q.abs() > 1e-10 {
                has_nonzero_q = true;
                break;
            }
        }
    }
    assert!(
        has_nonzero_q,
        "QTable should have learned non-zero Q-values after 100 iterations"
    );

    // Verify LinUCB explored multiple arms (at least 3 different arms selected)
    let arms_used = arm_selections.iter().filter(|&&count| count > 0).count();
    assert!(
        arms_used >= 3,
        "LinUCB should explore at least 3 different arms, only used {arms_used}"
    );

    // Verify bandit state is consistent
    assert_eq!(bandit.total_pulls(), 100);
}

/// Verify that risk-adjusted Q-learning reduces epsilon for high blast radius
/// and preserves it for low blast radius.
#[test]
fn risk_adjusted_reduces_epsilon_on_high_blast() {
    let params = LearningParams {
        epsilon: 0.3,
        epsilon_decay: 1.0, // no decay for deterministic testing
        epsilon_min: 0.01,
        ..LearningParams::default()
    };

    let rl = RiskAdjustedQLearning::with_threshold(params, 10);

    // Low blast radius => epsilon stays near original
    let eps_low = rl.epsilon_with_risk(1);
    assert!(
        eps_low > 0.25,
        "blast_radius=1 should keep epsilon near 0.3, got {eps_low}"
    );

    // At threshold => epsilon halved
    let eps_threshold = rl.epsilon_with_risk(10);
    assert!(
        (eps_threshold - 0.15).abs() < 0.02,
        "blast_radius=threshold should halve epsilon, got {eps_threshold}"
    );

    // High blast radius => epsilon much lower
    let eps_high = rl.epsilon_with_risk(50);
    assert!(
        eps_high < 0.06,
        "blast_radius=50 should greatly reduce epsilon, got {eps_high}"
    );

    // Very high blast radius => near zero
    let eps_extreme = rl.epsilon_with_risk(1000);
    assert!(
        eps_extreme < 0.005,
        "blast_radius=1000 should nearly eliminate exploration, got {eps_extreme}"
    );

    // Monotonicity
    assert!(eps_low > eps_threshold);
    assert!(eps_threshold > eps_high);
    assert!(eps_high > eps_extreme);
}

/// Risk-adjusted learner should successfully complete update-select cycles.
#[test]
fn risk_adjusted_update_select_cycle() {
    let params = LearningParams::default();
    let mut rl = RiskAdjustedQLearning::new(params);

    // Seed some transitions
    for i in 0..20u64 {
        let state = i % 5;
        let action = i % 3;
        let reward = if action == 1 { 1.0 } else { 0.2 };
        let next_state = (i + 1) % 5;
        let next_action = (i + 1) % 3;

        let td = rl.update(state, action, reward, next_state, next_action, false);
        assert!(td.is_finite());
    }

    // Select with various blast radii
    for blast in [0, 1, 5, 10, 50, 100] {
        let action = rl.select_action(0, blast);
        if let Some(a) = action {
            assert!(a < 3, "action should be one of the seeded ones");
        }
    }
}

/// AST-enriched bandit tests (feature-gated).
#[cfg(feature = "ast-features")]
mod ast_features_tests {
    use touring_intelligence::rl::bandit::ast_enriched::AstEnrichedBandit;
    use touring_intelligence::rl::bandit::ast_features::{
        AST_FEATURE_COUNT, FEATURE_DIM_AST, enrich_features, extract_ast_features,
    };
    use touring_intelligence::rl::bandit::linucb::NUM_ARMS;

    #[test]
    fn ast_features_returns_35_dims() {
        let base = [0.5f64; 19];
        let enriched = enrich_features(&base, "/nonexistent/file.py");

        assert_eq!(enriched.len(), FEATURE_DIM_AST);
        assert_eq!(enriched.len(), 35);

        // Base features preserved
        for i in 0..19 {
            assert!((enriched[i] - 0.5).abs() < f64::EPSILON);
        }

        // AST features are zeros for nonexistent file
        for i in 19..35 {
            assert!(enriched[i].abs() < f64::EPSILON);
        }
    }

    #[test]
    fn ast_enriched_bandit_select_and_update() {
        let mut bandit = AstEnrichedBandit::new(1.0);
        let base = [0.0f64; 19];

        for _ in 0..20 {
            let arm = bandit.select_arm_for_file(&base, "/nonexistent.rs");
            assert!(arm < NUM_ARMS);
            bandit.update_from_file(arm, &base, "/nonexistent.rs", 0.7);
        }

        assert!(bandit.total_pulls() >= 20);

        let stats = bandit.arm_stats();
        assert_eq!(stats.len(), NUM_ARMS);
    }

    #[test]
    fn ast_features_graceful_degradation() {
        // All of these should return [0.0; 16] without panicking
        let cases = ["/nonexistent.py", "/tmp/binary_file.bin", "", "/dev/null"];

        for path in cases {
            let features = extract_ast_features(path);
            assert_eq!(
                features, [0.0; AST_FEATURE_COUNT],
                "should return zeros for path: {path}"
            );
        }
    }
}

/// AST-enriched bandit tests that work WITHOUT the ast-features flag
/// (verifying graceful degradation when touring-ast is not compiled in).
#[cfg(not(feature = "ast-features"))]
mod ast_no_feature_tests {
    use touring_intelligence::rl::bandit::ast_enriched::AstEnrichedBandit;
    use touring_intelligence::rl::bandit::ast_features::{
        AST_FEATURE_COUNT, FEATURE_DIM_AST, enrich_features, extract_ast_features,
    };
    use touring_intelligence::rl::bandit::linucb::NUM_ARMS;

    #[test]
    fn ast_features_all_zeros_without_feature_flag() {
        let features = extract_ast_features("src/lib.rs");
        assert_eq!(features, [0.0; AST_FEATURE_COUNT]);
    }

    #[test]
    fn enriched_features_have_correct_dimension() {
        let base = [1.0f64; 19];
        let enriched = enrich_features(&base, "src/lib.rs");
        assert_eq!(enriched.len(), FEATURE_DIM_AST);

        // Base preserved
        for i in 0..19 {
            assert!((enriched[i] - 1.0).abs() < f64::EPSILON);
        }
        // AST all zeros (feature disabled)
        for i in 19..35 {
            assert!(enriched[i].abs() < f64::EPSILON);
        }
    }

    #[test]
    fn ast_enriched_bandit_works_without_feature_flag() {
        let mut bandit = AstEnrichedBandit::new(1.0);
        let base = [0.0f64; 19];

        let arm = bandit.select_arm_for_file(&base, "src/lib.rs");
        assert!(arm < NUM_ARMS);

        bandit.update_from_file(arm, &base, "src/lib.rs", 0.5);
        assert_eq!(bandit.total_pulls(), 1);
    }
}
