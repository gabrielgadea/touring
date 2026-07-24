//! Property-based tests for touring-learning algorithms.
#![allow(clippy::indexing_slicing)] // proptest vecs are asserted non-empty before indexing
//!
//! Uses proptest to verify invariants that must hold for ALL inputs,
//! complementing the 916+ example-based tests with mathematical guarantees.

use proptest::prelude::*;
use std::collections::HashSet;

use touring_intelligence::rl::memory::recall::rrf_fuse;
use touring_intelligence::rl::ranking::cusum::{DriftSignal, LatencyDriftDetector};
use touring_intelligence::rl::ranking::wilson::WilsonScore;
use touring_intelligence::rl::ranking::wilson::{DriftDetector, WilsonRanker};
use touring_intelligence::rl::rl::qtable::QLearning;
use touring_intelligence::rl::rl::qtable::QTable;

// ============================================================================
// Strategies
// ============================================================================

/// Generate a ranked list for RRF: Vec<(String, f64)> with unique doc IDs.
fn ranked_list_strategy(max_len: usize) -> impl Strategy<Value = Vec<(String, f64)>> {
    prop::collection::vec(
        ("[a-z]{1,8}".prop_map(|s| format!("doc_{s}")), 0.0..1.0_f64),
        0..=max_len,
    )
    .prop_map(|mut items| {
        // Deduplicate by doc_id within a single list
        let mut seen = HashSet::new();
        items.retain(|(id, _)| seen.insert(id.clone()));
        items
    })
}

/// Generate multiple ranked lists for RRF fusion.
fn ranked_lists_strategy(
    max_lists: usize,
    max_items: usize,
) -> impl Strategy<Value = Vec<Vec<(String, f64)>>> {
    prop::collection::vec(ranked_list_strategy(max_items), 0..=max_lists)
}

// ============================================================================
// 1. RRF Fusion Properties
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    /// Output length <= sum of unique doc IDs across all input lists.
    #[test]
    fn rrf_output_len_bounded(
        lists in ranked_lists_strategy(5, 20),
        k in 1.0..200.0_f64,
        top_n in 1..100_usize,
    ) {
        let all_ids: HashSet<&str> = lists
            .iter()
            .flat_map(|list| list.iter().map(|(id, _)| id.as_str()))
            .collect();

        let result = rrf_fuse(&lists, k, top_n);

        // Output cannot exceed the number of unique doc IDs
        prop_assert!(result.len() <= all_ids.len());
        // Output cannot exceed top_n
        prop_assert!(result.len() <= top_n);
    }

    /// All RRF output scores are strictly positive.
    #[test]
    fn rrf_scores_always_positive(
        lists in ranked_lists_strategy(4, 15),
        k in 1.0..200.0_f64,
    ) {
        let result = rrf_fuse(&lists, k, 1000);
        for (doc_id, score) in &result {
            prop_assert!(
                *score > 0.0,
                "Score for {} was {}, expected > 0.0", doc_id, score
            );
        }
    }

    /// Empty input lists produce empty output.
    #[test]
    fn rrf_empty_input_empty_output(k in 1.0..200.0_f64, top_n in 1..100_usize) {
        let result = rrf_fuse(&[], k, top_n);
        prop_assert!(result.is_empty());

        // Also: list of empty lists
        let empty_lists: Vec<Vec<(String, f64)>> = vec![vec![], vec![]];
        let result2 = rrf_fuse(&empty_lists, k, top_n);
        prop_assert!(result2.is_empty());
    }

    /// Single list preserves relative ordering (rank 0 > rank 1 > ...).
    #[test]
    fn rrf_single_list_preserves_order(
        list in ranked_list_strategy(20).prop_filter(
            "need at least 2 items",
            |l| l.len() >= 2
        ),
        k in 1.0..200.0_f64,
    ) {
        let result = rrf_fuse(std::slice::from_ref(&list), k, 1000);

        // For a single list, output order should match input order
        // because RRF score = 1/(k+rank), which is monotonically decreasing.
        for i in 0..result.len().saturating_sub(1) {
            prop_assert!(
                result[i].1 >= result[i + 1].1,
                "Score at position {} ({}) < score at position {} ({})",
                i, result[i].1, i + 1, result[i + 1].1
            );
        }
    }

    /// Commutativity: changing the order of input lists produces the same
    /// set of doc IDs with the same scores (ordering may differ only on ties).
    #[test]
    fn rrf_commutative_result_set(
        list_a in ranked_list_strategy(10),
        list_b in ranked_list_strategy(10),
        k in 1.0..200.0_f64,
    ) {
        let result_ab = rrf_fuse(&[list_a.clone(), list_b.clone()], k, 1000);
        let result_ba = rrf_fuse(&[list_b, list_a], k, 1000);

        // Same set of doc IDs
        let ids_ab: HashSet<&str> = result_ab.iter().map(|(id, _)| id.as_str()).collect();
        let ids_ba: HashSet<&str> = result_ba.iter().map(|(id, _)| id.as_str()).collect();
        prop_assert_eq!(&ids_ab, &ids_ba);

        // Same scores for each doc
        let scores_ab: std::collections::HashMap<&str, f64> =
            result_ab.iter().map(|(id, s)| (id.as_str(), *s)).collect();
        let scores_ba: std::collections::HashMap<&str, f64> =
            result_ba.iter().map(|(id, s)| (id.as_str(), *s)).collect();
        for (id, score_ab) in &scores_ab {
            let score_ba = scores_ba.get(id).unwrap();
            prop_assert!(
                (score_ab - score_ba).abs() < 1e-12,
                "Score mismatch for {}: {} vs {}", id, score_ab, score_ba
            );
        }
    }

    /// A doc appearing in more lists gets a higher RRF score than one in fewer lists
    /// (when they appear at the same rank position).
    #[test]
    fn rrf_more_lists_higher_score(k in 10.0..200.0_f64) {
        // doc_a appears in 2 lists at rank 0, doc_b in 1 list at rank 0
        let list1 = vec![("doc_a".to_string(), 1.0)];
        let list2 = vec![("doc_a".to_string(), 1.0), ("doc_b".to_string(), 0.5)];
        let list3 = vec![("doc_b".to_string(), 1.0)];

        let result = rrf_fuse(&[list1, list2, list3], k, 10);

        let score_a = result.iter().find(|(id, _)| id == "doc_a").map(|(_, s)| *s);
        let score_b = result.iter().find(|(id, _)| id == "doc_b").map(|(_, s)| *s);

        // doc_a: 1/(k+0) + 1/(k+0) = 2/k
        // doc_b: 1/(k+1) + 1/(k+0) = 1/(k+1) + 1/k = (2k+1)/(k(k+1))
        // 2/k vs (2k+1)/(k(k+1)) => 2(k+1) vs 2k+1 => 2k+2 vs 2k+1 => doc_a > doc_b
        if let (Some(sa), Some(sb)) = (score_a, score_b) {
            prop_assert!(sa > sb, "doc_a ({}) should score higher than doc_b ({})", sa, sb);
        }
    }
}

// ============================================================================
// 2. Wilson Score Properties
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1000))]

    /// lower_bound <= upper_bound always.
    #[test]
    fn wilson_lower_le_upper(
        successes in 0..10_000_u64,
        extra_failures in 0..10_000_u64,
        confidence in prop_oneof![Just(0.90), Just(0.95), Just(0.99)],
    ) {
        let trials = successes + extra_failures;
        if let Some(score) = WilsonScore::calculate(successes, trials, confidence) {
            prop_assert!(
                score.lower <= score.upper,
                "lower ({}) > upper ({}) for {}/{} at confidence {}",
                score.lower, score.upper, successes, trials, confidence
            );
        }
    }

    /// Wilson score bounds are in [0.0, 1.0].
    #[test]
    fn wilson_bounds_in_unit_interval(
        successes in 0..5_000_u64,
        extra_failures in 0..5_000_u64,
        confidence in prop_oneof![Just(0.90), Just(0.95), Just(0.99)],
    ) {
        let trials = successes + extra_failures;
        if let Some(score) = WilsonScore::calculate(successes, trials, confidence) {
            prop_assert!(score.lower >= 0.0, "lower {} < 0.0", score.lower);
            prop_assert!(score.upper <= 1.0, "upper {} > 1.0", score.upper);
            prop_assert!(score.center >= 0.0, "center {} < 0.0", score.center);
            prop_assert!(score.center <= 1.0, "center {} > 1.0", score.center);
        }
    }

    /// Zero trials always returns None.
    #[test]
    fn wilson_zero_trials_returns_none(successes in 0..100_u64) {
        prop_assert!(WilsonScore::calculate(successes, 0, 0.95).is_none());
    }

    /// Zero successes => lower_bound very close to 0.
    #[test]
    fn wilson_zero_successes_low_lower(
        trials in 1..10_000_u64,
        confidence in prop_oneof![Just(0.90), Just(0.95), Just(0.99)],
    ) {
        if let Some(score) = WilsonScore::calculate(0, trials, confidence) {
            prop_assert!(
                score.lower < 0.05,
                "lower {} too high for 0/{}", score.lower, trials
            );
        }
    }

    /// All successes => lower_bound > 0.
    #[test]
    fn wilson_all_successes_positive_lower(
        trials in 1..10_000_u64,
        confidence in prop_oneof![Just(0.90), Just(0.95), Just(0.99)],
    ) {
        if let Some(score) = WilsonScore::calculate(trials, trials, confidence) {
            prop_assert!(
                score.lower > 0.0,
                "lower {} should be > 0 for {}/{}", score.lower, trials, trials
            );
        }
    }

    /// More trials with the same success ratio => narrower interval.
    #[test]
    fn wilson_more_trials_narrower_interval(
        ratio_pct in 10..90_u64,
        n_small in 10..100_u64,
        multiplier in 2..10_u64,
    ) {
        let n_large = n_small * multiplier;
        let successes_small = (n_small * ratio_pct) / 100;
        let successes_large = (n_large * ratio_pct) / 100;

        if let (Some(small), Some(large)) = (
            WilsonScore::calculate(successes_small, n_small, 0.95),
            WilsonScore::calculate(successes_large, n_large, 0.95),
        ) {
            let width_small = small.upper - small.lower;
            let width_large = large.upper - large.lower;
            prop_assert!(
                width_large <= width_small + 1e-10,
                "Larger sample ({}) interval width ({}) > smaller sample ({}) width ({})",
                n_large, width_large, n_small, width_small
            );
        }
    }
}

// ============================================================================
// 3. QTable Update Properties
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(300))]

    /// After a single update, the Q-value for the updated state-action is non-zero
    /// (assuming non-zero reward).
    #[test]
    fn qtable_update_sets_nonzero(
        state in 0..100_u64,
        action in 0..20_u64,
        reward in 0.01..1.0_f64,
    ) {
        let mut qt = QTable::new();
        let _td_error = qt.update(state, action, reward, state, None, true);
        let q = qt.get_q(state, action);
        prop_assert!(
            q.abs() > 1e-15,
            "Q-value should be non-zero after update with reward={}, got q={}",
            reward, q
        );
    }

    /// Convergence: repeated updates with the same reward transitioning to
    /// an absorbing terminal state converge Q-value toward that reward.
    /// When next_state has no known actions, best_action returns None => next_q=0,
    /// so the TD target is just reward, and Q converges to reward.
    #[test]
    fn qtable_convergence_toward_reward(
        state in 0..50_u64,
        action in 0..10_u64,
        reward in 0.1..1.0_f64,
    ) {
        let mut qt = QTable::new();
        // Use a terminal_state (99999) that has no known actions => next_q = 0
        let terminal_state = 99999_u64;

        // Run many updates: (state, action) -> terminal_state with given reward
        for _ in 0..500 {
            qt.reset_traces();
            let _ = qt.update(state, action, reward, terminal_state, None, true);
        }

        let q = qt.get_q(state, action);
        // With next_q = 0, td_target = reward, Q converges to reward via alpha=0.1
        prop_assert!(
            (q - reward).abs() < 0.05,
            "Q-value {} did not converge to reward {} (delta: {})",
            q, reward, (q - reward).abs()
        );
    }

    /// Q-values for different state-action pairs are independent:
    /// updating (s1, a1) should not change (s2, a2) if they are distinct
    /// AND there are no eligibility traces linking them.
    #[test]
    fn qtable_independent_pairs(
        s1 in 0..50_u64,
        a1 in 0..10_u64,
        s2 in 50..100_u64,
        a2 in 10..20_u64,
        reward in 0.1..1.0_f64,
    ) {
        let mut qt = QTable::new();

        // First establish s2,a2 with a known value
        qt.reset_traces();
        let _ = qt.update(s2, a2, 0.5, s2, None, true);
        let q_before = qt.get_q(s2, a2);

        // Now update s1,a1 in fresh trace context
        qt.reset_traces();
        let _ = qt.update(s1, a1, reward, s1, None, true);

        let q_after = qt.get_q(s2, a2);
        prop_assert_eq!(
            q_before, q_after,
            "Updating ({},{}) changed Q({},{}) from {} to {}",
            s1, a1, s2, a2, q_before, q_after
        );
    }

    /// td_error is finite for bounded inputs.
    #[test]
    fn qtable_td_error_finite(
        state in 0..100_u64,
        action in 0..20_u64,
        reward in -10.0..10.0_f64,
        next_state in 0..100_u64,
        terminal in proptest::bool::ANY,
    ) {
        let mut qt = QTable::new();
        let td_error = qt.update(state, action, reward, next_state, None, terminal);
        prop_assert!(
            td_error.is_finite(),
            "TD error should be finite, got: {}", td_error
        );
    }

    /// The QTable never grows beyond MAX_ENTRIES + 1 (eviction triggers at MAX_ENTRIES).
    #[test]
    fn qtable_bounded_size(
        updates in prop::collection::vec(
            (0..200_u64, 0..200_u64, -1.0..1.0_f64),
            1..500
        ),
    ) {
        let mut qt = QTable::new();
        for (state, action, reward) in updates {
            qt.reset_traces();
            let _ = qt.update(state, action, reward, state, None, true);
        }
        // MAX_ENTRIES is 50_000 -- with at most 500 updates we should be well under
        prop_assert!(qt.len() <= 500);
    }
}

// ============================================================================
// 4. CUSUM Drift Detection Properties
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    /// Constant input never triggers drift (default params: k=5.0, h=50.0).
    #[test]
    fn cusum_constant_no_drift(value in 10..1000_u64) {
        let mut detector = LatencyDriftDetector::new();
        // Push 200 identical values -- should never detect drift
        for _ in 0..200 {
            let signal = detector.push(value);
            prop_assert_eq!(
                signal, DriftSignal::Normal,
                "Constant input {} should not trigger drift", value
            );
        }
    }

    /// After sufficient samples, mean should reflect the input value.
    #[test]
    fn cusum_mean_reflects_input(value in 1..1000_u64) {
        let mut detector = LatencyDriftDetector::new();
        for _ in 0..50 {
            detector.push(value);
        }
        let mean = detector.mean();
        prop_assert!(
            (mean - value as f64).abs() < 0.01,
            "Mean {} should equal constant input {}", mean, value
        );
    }

    /// Total samples counter increments correctly.
    #[test]
    fn cusum_total_samples_accurate(
        values in prop::collection::vec(1..1000_u64, 1..200),
    ) {
        let mut detector = LatencyDriftDetector::new();
        for v in &values {
            detector.push(*v);
        }
        prop_assert_eq!(
            detector.total_samples() as usize,
            values.len(),
            "total_samples should equal number of pushes"
        );
    }

    /// Window length is bounded by window_size (default 100).
    #[test]
    fn cusum_window_bounded(
        values in prop::collection::vec(1..1000_u64, 1..300),
    ) {
        let mut detector = LatencyDriftDetector::new();
        for v in &values {
            detector.push(*v);
        }
        prop_assert!(
            detector.current_window_len() <= 100,
            "Window len {} exceeds max 100", detector.current_window_len()
        );
    }

    /// Fewer than 10 samples always returns Normal (insufficient data).
    #[test]
    fn cusum_insufficient_samples_normal(
        values in prop::collection::vec(1..10_000_u64, 1..10),
    ) {
        let mut detector = LatencyDriftDetector::new();
        for v in &values {
            let signal = detector.push(*v);
            if detector.total_samples() < 10 {
                prop_assert_eq!(
                    signal, DriftSignal::Normal,
                    "Should be Normal with < 10 samples"
                );
            }
        }
    }

    /// Reset clears all state.
    #[test]
    fn cusum_reset_clears_state(
        values in prop::collection::vec(1..1000_u64, 10..100),
    ) {
        let mut detector = LatencyDriftDetector::new();
        for v in &values {
            detector.push(*v);
        }
        detector.reset();
        prop_assert_eq!(detector.total_samples(), 0);
        prop_assert_eq!(detector.current_window_len(), 0);
        prop_assert!((detector.mean() - 0.0).abs() < 1e-10);
    }
}

// ============================================================================
// 5. Wilson DriftDetector Properties (simple mean-based drift)
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    /// Constant values produce no drift.
    #[test]
    fn drift_constant_no_detection(
        value in 0.0..100.0_f64,
        n in 20..200_usize,
    ) {
        let mut d = DriftDetector::with_params(200, 0.05);
        for _ in 0..n {
            d.record("metric", value);
        }
        let result = d.detect("metric");
        prop_assert!(
            !result.drift_detected,
            "Constant value {} should not trigger drift", value
        );
    }

    /// Step change in values eventually triggers drift (if magnitude > threshold).
    #[test]
    fn drift_step_change_detected(
        low in 0.0..10.0_f64,
        high_delta in 1.0..50.0_f64,
    ) {
        let high = low + high_delta;
        let mut d = DriftDetector::with_params(40, 0.05);
        for _ in 0..20 {
            d.record("metric", low);
        }
        for _ in 0..20 {
            d.record("metric", high);
        }
        let result = d.detect("metric");
        if high_delta >= 0.05 {
            prop_assert!(
                result.drift_detected,
                "Step from {} to {} (delta={}) should trigger drift (mag={})",
                low, high, high_delta, result.magnitude
            );
        }
    }

    /// Magnitude is always non-negative.
    #[test]
    fn drift_magnitude_nonneg(
        values in prop::collection::vec(-100.0..100.0_f64, 20..100),
    ) {
        let mut d = DriftDetector::new();
        for v in &values {
            d.record("metric", *v);
        }
        let result = d.detect("metric");
        prop_assert!(
            result.magnitude >= 0.0,
            "Magnitude should be >= 0, got {}", result.magnitude
        );
    }

    /// Confidence is in [0.0, 1.0].
    #[test]
    fn drift_confidence_bounded(
        values in prop::collection::vec(0.0..100.0_f64, 10..200),
    ) {
        let mut d = DriftDetector::new();
        for v in &values {
            d.record("metric", *v);
        }
        let result = d.detect("metric");
        prop_assert!(result.confidence >= 0.0 && result.confidence <= 1.0);
    }
}

// ============================================================================
// 6. WilsonRanker Integration Properties
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    /// Ranker with higher success rate ranks above one with lower rate
    /// (given sufficient trials for confidence).
    #[test]
    fn ranker_higher_rate_ranks_first(
        n in 50..500_u64,
    ) {
        let mut ranker = WilsonRanker::new();

        // item_high: 90% success
        let high_successes = (n * 90) / 100;
        for _ in 0..high_successes {
            ranker.record("high", true);
        }
        for _ in 0..(n - high_successes) {
            ranker.record("high", false);
        }

        // item_low: 30% success
        let low_successes = (n * 30) / 100;
        for _ in 0..low_successes {
            ranker.record("low", true);
        }
        for _ in 0..(n - low_successes) {
            ranker.record("low", false);
        }

        let ranked = ranker.rank();
        prop_assert!(ranked.len() >= 2);
        prop_assert_eq!(&ranked[0].id, "high");
    }

    /// Score is in [0.0, 1.0] for any recorded item.
    #[test]
    fn ranker_score_bounded(
        successes in 0..100_u64,
        failures in 0..100_u64,
    ) {
        let trials = successes + failures;
        if trials == 0 {
            return Ok(());
        }
        let mut ranker = WilsonRanker::new();
        for _ in 0..successes {
            ranker.record("item", true);
        }
        for _ in 0..failures {
            ranker.record("item", false);
        }
        let score = ranker.score("item");
        prop_assert!(
            (0.0..=1.0).contains(&score),
            "Score {} out of [0, 1]", score
        );
    }
}

// ============================================================================
// 7. Cosine Similarity Properties (via SkillClusterer)
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    /// Self-similarity: a skill is always most similar to itself (threshold permitting).
    /// After clustering, a skill always ends up in the same cluster as itself.
    #[test]
    fn clusterer_skill_in_own_cluster(
        n_contexts in 1..5_usize,
        n_usages in 1..10_usize,
    ) {
        let mut clusterer = touring_intelligence::rl::SkillClusterer::with_params(0.5, 10);
        for i in 0..n_contexts {
            let ctx = format!("ctx_{}", i);
            for _ in 0..n_usages {
                clusterer.record_usage("skill_a", &ctx, true);
            }
        }
        let clusters = clusterer.cluster();
        // skill_a should appear in exactly one cluster
        let containing: Vec<_> = clusters
            .iter()
            .filter(|c| c.skills.contains(&"skill_a".to_string()))
            .collect();
        prop_assert_eq!(
            containing.len(), 1,
            "skill_a should be in exactly 1 cluster, found {}", containing.len()
        );
    }

    /// Skills with identical usage patterns end up in the same cluster.
    #[test]
    fn clusterer_identical_patterns_same_cluster(
        n_usages in 2..10_usize,
    ) {
        let mut clusterer = touring_intelligence::rl::SkillClusterer::with_params(0.5, 10);
        // Both skills used in the same contexts with the same frequency
        for _ in 0..n_usages {
            clusterer.record_usage("alpha", "context_x", true);
            clusterer.record_usage("beta", "context_x", true);
        }
        let clusters = clusterer.cluster();
        // Find which cluster each is in
        let cluster_of_alpha = clusters.iter().find(|c| c.skills.contains(&"alpha".to_string()));
        let cluster_of_beta = clusters.iter().find(|c| c.skills.contains(&"beta".to_string()));
        if let (Some(ca), Some(cb)) = (cluster_of_alpha, cluster_of_beta) {
            prop_assert_eq!(
                ca.id, cb.id,
                "Identical usage patterns should cluster together"
            );
        }
    }

    /// Cohesion is always in (0.0, 1.0].
    #[test]
    fn clusterer_cohesion_bounded(
        n_skills in 1..8_usize,
    ) {
        let mut clusterer = touring_intelligence::rl::SkillClusterer::with_params(0.9, 20);
        // Each skill uses the same single context => all should cluster together
        for i in 0..n_skills {
            clusterer.record_usage(&format!("s{}", i), "shared_ctx", true);
        }
        let clusters = clusterer.cluster();
        for cluster in &clusters {
            prop_assert!(
                cluster.cohesion > 0.0 && cluster.cohesion <= 1.0,
                "Cohesion {} out of (0, 1]", cluster.cohesion
            );
        }
    }
}
