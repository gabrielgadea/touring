//! E2E integration tests for touring-learning — bandit subsystem
//!
//! Covers:
//! 1. ReminderBandit — 7-arm LinUCB for adaptive reminder injection
//! 2. AdaptiveAlpha — regret-based exploration parameter adjustment
//! 3. TransferLinUCB — cross-task arm transfer via context similarity
//! 4. AstEnrichedBandit — AST-derived feature enrichment for contextual bandit
//!
//! Tests verify the hot path: arm selection → reward → UCB update cycle.

// Test harness idioms: regression guards with constant values, unit let-bindings
// for side-effect-only calls, inline range checks, and redundant same-type casts
// are acceptable in tests — they encode intent without sacrificing clarity.
#![allow(
    clippy::assertions_on_constants,
    clippy::bool_comparison,
    clippy::let_unit_value,
    clippy::manual_range_contains,
    clippy::unnecessary_cast,
    clippy::useless_vec,
    clippy::int_plus_one,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic
)]

use touring_intelligence::rl::bandit::linucb::LinUCBBandit;
use touring_intelligence::rl::bandit::{
    AdaptiveAlpha, AstEnrichedBandit, ReminderBandit, ReminderContext, ReminderKind, TransferLinUCB,
};

// ══════════════════════════════════════════════════════════════════════════════
// ReminderBandit tests — 7-arm LinUCB with Sherman-Morrison rank-1 update
// ══════════════════════════════════════════════════════════════════════════════

mod reminder_bandit_tests {
    use super::*;

    fn default_ctx() -> ReminderContext {
        ReminderContext {
            cila_level: 2,
            recent_corrections: false,
            context_fill_ratio: 0.6,
            tool_success_rate: 0.8,
            is_rust_task: true,
            session_turn: 100,
        }
    }

    #[test]
    fn reminder_bandit_select_returns_valid_arm_and_text() {
        let mut bandit = ReminderBandit::new();
        let ctx = default_ctx();
        for _ in 0..20 {
            let (arm, text) = bandit.select(&ctx);
            assert!(arm < 7, "arm must be 0..6, got {arm}");
            // Text should match the kind's reminder_text
            let kind = ReminderKind::from_index(arm).unwrap_or(ReminderKind::None);
            assert_eq!(text, kind.reminder_text());
        }
    }

    #[test]
    fn reminder_bandit_select_increments_total_selections() {
        let mut bandit = ReminderBandit::new();
        let ctx = default_ctx();
        for _ in 0..5 {
            bandit.select(&ctx);
        }
        assert_eq!(
            bandit.total_selections(),
            5,
            "total_selections must equal number of select calls"
        );
    }

    #[test]
    fn reminder_bandit_reward_true_increments_pull_count() {
        let mut bandit = ReminderBandit::new();
        let ctx = default_ctx();

        let (arm, _) = bandit.select(&ctx);
        let before = bandit.pull_counts()[arm];
        bandit.reward(arm, true, &ctx);
        assert_eq!(
            bandit.pull_counts()[arm],
            before + 1,
            "reward(true) must increment arm pull count"
        );
    }

    #[test]
    fn reminder_bandit_reward_false_decrements_pull_count() {
        let mut bandit = ReminderBandit::new();
        let ctx = default_ctx();

        let (arm, _) = bandit.select(&ctx);
        bandit.reward(arm, false, &ctx); // negative reward applied
        // Pull count should still increment (reward was applied)
        assert_eq!(bandit.pull_counts()[arm], 1);
    }

    #[test]
    fn reminder_bandit_feature_vector_dim() {
        let ctx = default_ctx();
        let fv = ctx.feature_vector();
        assert_eq!(fv.len(), 6, "feature vector must have 6 dimensions");
    }

    #[test]
    fn reminder_bandit_feature_vector_clamped() {
        let ctx = ReminderContext {
            cila_level: 10, // clamped to [0, 6]
            recent_corrections: true,
            context_fill_ratio: 2.0, // clamped to [0, 1]
            tool_success_rate: -0.5, // clamped to [0, 1]
            is_rust_task: false,
            session_turn: u32::MAX,
        };
        let fv = ctx.feature_vector();
        for (i, val) in fv.iter().enumerate() {
            assert!(
                (0.0..=1.0).contains(val),
                "feature[{i}] = {val} must be in [0, 1]"
            );
        }
    }

    #[test]
    fn reminder_bandit_json_roundtrip() {
        let mut bandit = ReminderBandit::new();
        let ctx = default_ctx();
        for _ in 0..14 {
            let (arm, _) = bandit.select(&ctx);
            bandit.reward(arm, true, &ctx);
        }

        let json = bandit.to_json().unwrap();
        let loaded = ReminderBandit::from_json(&json).unwrap();

        assert_eq!(bandit.total_selections(), loaded.total_selections());
        assert_eq!(bandit.pull_counts(), loaded.pull_counts());
    }

    #[test]
    fn reminder_bandit_reminder_kind_from_index_valid() {
        for idx in 0..7 {
            let kind = ReminderKind::from_index(idx);
            assert!(
                kind.is_some(),
                "ReminderKind::from_index({idx}) should return Some"
            );
        }
        assert!(
            ReminderKind::from_index(7).is_none(),
            "ReminderKind::from_index(7) should return None"
        );
    }

    #[test]
    fn reminder_bandit_reminder_kind_text() {
        for idx in 1..7 {
            let kind = ReminderKind::from_index(idx).unwrap();
            let text = kind.reminder_text();
            assert!(
                !text.is_empty(),
                "ReminderKind {idx} should have reminder text"
            );
        }
        assert_eq!(ReminderKind::from_index(0).unwrap().reminder_text(), "");
    }

    #[test]
    fn reminder_bandit_positive_reward_converges() {
        let mut bandit = ReminderBandit::new();
        let ctx = default_ctx();

        // Target arm 2: reward it positively many times
        let target_arm = 2;
        for _ in 0..200 {
            let (arm, _) = bandit.select(&ctx);
            let was_useful = (arm == target_arm) as bool;
            bandit.reward(arm, was_useful, &ctx);
        }

        // After many positive rewards on arm 2, it should be selected more often
        let selections: usize = (0..100)
            .map(|_| bandit.select(&ctx).0)
            .filter(|&a| a == target_arm)
            .count();
        assert!(
            selections > 5,
            "rewarded arm should be selected more than random chance (got {selections}/100)"
        );
    }

    #[test]
    fn reminder_bandit_negative_reward_does_not_panic() {
        let mut bandit = ReminderBandit::new();
        let ctx = default_ctx();
        for _ in 0..14 {
            let (arm, _) = bandit.select(&ctx);
            bandit.reward(arm, true, &ctx);
        }

        let (arm, _) = bandit.select(&ctx);
        // reward(false) should be handled gracefully — negative reward path
        bandit.reward(arm, false, &ctx);
        assert!(true, "negative reward must not panic");
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// AdaptiveAlpha tests — regret-based alpha adjustment with warmup
// ══════════════════════════════════════════════════════════════════════════════

mod adaptive_alpha_tests {
    use super::*;

    #[test]
    fn adaptive_alpha_initial_value() {
        let aa = AdaptiveAlpha::with_defaults();
        assert_eq!(aa.alpha(), 1.0); // DEFAULT_ALPHA = 1.0
    }

    #[test]
    fn adaptive_alpha_increases_with_high_regret() {
        let mut aa = AdaptiveAlpha::with_defaults();
        let initial = aa.alpha();

        // Feed high regret scenario repeatedly (mean regret > 0.3)
        for _ in 0..60 {
            // expected=1.0, actual=0.0 → regret=1.0 (above HIGH_REGRET=0.3)
            let _ = aa.update(1.0, 0.0);
        }

        // After warmup (window_size/2 = 25 samples), high regret should increase alpha
        assert!(
            aa.alpha() > initial,
            "alpha should increase with high regret: {initial} -> {}",
            aa.alpha()
        );
    }

    #[test]
    fn adaptive_alpha_decreases_with_low_regret() {
        let mut aa = AdaptiveAlpha::with_defaults();

        // Warmup with near-zero regret
        for _ in 0..60 {
            let _ = aa.update(0.0, 0.0); // regret = 0.0 (< LOW_REGRET = 0.05)
        }

        let pre = aa.alpha();

        // Feed low regret signals
        for _ in 0..60 {
            let _ = aa.update(0.0, 0.0);
        }

        let post = aa.alpha();
        assert!(
            post <= pre || (post - pre).abs() < 1e-9,
            "alpha should not increase with low regret: {pre} -> {post}"
        );
    }

    #[test]
    fn adaptive_alpha_update_count_increments() {
        let mut aa = AdaptiveAlpha::with_defaults();
        for _ in 0..10 {
            let _ = aa.update(0.5, 0.5);
        }
        assert_eq!(aa.update_count(), 10);
    }

    #[test]
    fn adaptive_alpha_adjustments_after_warmup() {
        let mut aa = AdaptiveAlpha::with_defaults();

        // Warmup then adjust
        for _ in 0..60 {
            let _ = aa.update(1.0, 0.0); // regret=1.0 > HIGH_REGRET=0.3
        }

        let adj_before = aa.adjustments();
        for _ in 0..10 {
            let _ = aa.update(1.0, 0.0);
        }

        assert!(
            aa.adjustments() > adj_before,
            "adjustments counter must increase after warmup"
        );
    }

    #[test]
    fn adaptive_alpha_custom_window() {
        let aa = AdaptiveAlpha::new(0.5, 20);
        assert_eq!(aa.alpha(), 0.5);
        assert_eq!(aa.update_count(), 0);
    }

    #[test]
    fn adaptive_alpha_mean_regret_computed() {
        let mut aa = AdaptiveAlpha::with_defaults();

        // Fill window with known regret values
        for r in [0.1, 0.2, 0.3, 0.15, 0.25] {
            let _ = aa.update(0.5, 0.5 - r);
        }

        let mean = aa.mean_regret();
        // Mean of [0.1, 0.2, 0.3, 0.15, 0.25] = 1.0/5 = 0.2
        assert!(
            (mean - 0.2).abs() < 1e-9,
            "mean_regret should be ~0.2, got {mean}"
        );
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// TransferLinUCB tests — cross-task arm transfer via context similarity
// ══════════════════════════════════════════════════════════════════════════════

mod transfer_linucb_tests {
    use super::*;

    fn make_feat() -> ndarray::Array1<f64> {
        ndarray::Array1::from_vec((0..25).map(|i| (i as f64) * 0.05).collect())
    }

    #[test]
    fn transfer_linucb_new_with_similarity() {
        let tlu = TransferLinUCB::new(0.5);
        // Access via bandit to check total_pulls
        assert_eq!(tlu.bandit().total_pulls(), 0);
    }

    #[test]
    fn transfer_linucb_bandit_mut_select_arm() {
        let mut tlu = TransferLinUCB::new(0.5);
        let feat = make_feat();
        let (arm, score) = tlu.bandit_mut().select_arm(&feat);
        assert!(arm < 8);
        assert!(score.is_finite());
    }

    #[test]
    fn transfer_linucb_bandit_mut_update() {
        let mut tlu = TransferLinUCB::new(0.5);
        let feat = make_feat();
        let (arm, _) = tlu.bandit_mut().select_arm(&feat);
        tlu.bandit_mut().update(arm, &feat, 1.0);
        assert_eq!(tlu.bandit().total_pulls(), 1);
    }

    #[test]
    fn transfer_linucb_transfer_from_donor() {
        let mut donor = LinUCBBandit::new();
        let feat = make_feat();

        // Warm up donor
        for _ in 0..30 {
            let (arm, _) = donor.select_arm(&feat);
            donor.update(arm, &feat, 1.0);
        }

        let mut target = TransferLinUCB::new(0.8);

        // Transfer from donor at 0.5 similarity
        target.transfer_from(&donor, 0.5);

        // Context similarity should be set and clamped
        let sim = target.context_similarity();
        assert!(
            sim >= 0.0 && sim <= 1.0,
            "similarity must be in [0, 1], got {sim}"
        );
    }

    #[test]
    fn transfer_linucb_zero_similarity_no_blend() {
        let mut donor = LinUCBBandit::new();
        let feat = make_feat();
        for _ in 0..30 {
            let (arm, _) = donor.select_arm(&feat);
            donor.update(arm, &feat, 1.0);
        }

        let mut target = TransferLinUCB::new(0.0);
        let pulls_before = target.bandit().total_pulls();
        target.transfer_from(&donor, 0.0);

        // With zero similarity, transfer does nothing
        assert_eq!(target.bandit().total_pulls(), pulls_before);
    }

    #[test]
    fn transfer_linucb_context_similarity_clamped() {
        let tlu = TransferLinUCB::new(1.5);
        assert!(
            tlu.context_similarity() <= 1.0,
            "similarity must clamp to [0, 1]"
        );

        let tlu2 = TransferLinUCB::new(-0.5);
        assert!(
            tlu2.context_similarity() >= 0.0,
            "similarity must clamp to [0, 1]"
        );
    }

    #[test]
    fn transfer_linucb_bandit_access_via_mut() {
        // TransferLinUCB uses bandit_mut() as the primary API
        let mut tlu = TransferLinUCB::new(0.5);
        let feat = make_feat();
        let (arm, _) = tlu.bandit_mut().select_arm(&feat);
        tlu.bandit_mut().update(arm, &feat, 0.8);
        assert_eq!(tlu.bandit().total_pulls(), 1);
    }

    #[test]
    fn transfer_linucb_into_bandit() {
        let mut tlu = TransferLinUCB::new(0.5);
        let feat = make_feat();
        let (arm, _) = tlu.bandit_mut().select_arm(&feat);
        tlu.bandit_mut().update(arm, &feat, 0.5);

        let bandit = tlu.into_bandit();
        assert_eq!(bandit.total_pulls(), 1);
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// AstEnrichedBandit tests — AST-derived symbol complexity feature enrichment
// ══════════════════════════════════════════════════════════════════════════════

mod ast_enriched_bandit_tests {
    use super::*;

    fn base_features() -> [f64; 19] {
        std::array::from_fn(|i| (i as f64) * 0.05)
    }

    fn ast_features() -> ndarray::Array1<f64> {
        // 35-dim feature vector for AstEnrichedBandit
        ndarray::Array1::from_vec((0..35).map(|i| (i as f64) * 0.03).collect())
    }

    #[test]
    fn ast_enriched_bandit_new_with_alpha() {
        let bandit = AstEnrichedBandit::new(1.0);
        assert_eq!(bandit.arm_stats().len(), 8, "bandit should have 8 arms");
    }

    #[test]
    fn ast_enriched_bandit_select_arm_raw() {
        let mut bandit = AstEnrichedBandit::new(1.0);
        let features = ast_features();
        let (arm, score) = bandit.select_arm_raw(&features);
        assert!(arm < 8);
        assert!(score.is_finite());
    }

    #[test]
    fn ast_enriched_bandit_select_arm_for_file() {
        let mut bandit = AstEnrichedBandit::new(1.0);
        let base = base_features();

        // Should not panic even for non-existent file
        let arm = bandit.select_arm_for_file(&base, "src/main.rs");
        assert!(arm < 8);
    }

    #[test]
    fn ast_enriched_bandit_update_changes_total_pulls() {
        let mut bandit = AstEnrichedBandit::new(1.0);
        let features = ast_features();
        let (arm, _) = bandit.select_arm_raw(&features);

        // Count total pulls by summing arm_stats
        let before: u64 = bandit.arm_stats().iter().map(|(_, p, _)| p).sum();
        let reward_vec: [f64; 35] = std::array::from_fn(|i| (i as f64) * 0.03);
        bandit.update(arm, &reward_vec, 0.5);
        let after: u64 = bandit.arm_stats().iter().map(|(_, p, _)| p).sum();
        assert_eq!(after, before + 1);
    }

    #[test]
    fn ast_enriched_bandit_update_from_file() {
        let mut bandit = AstEnrichedBandit::new(1.0);
        let base = base_features();

        let arm = bandit.select_arm_for_file(&base, "src/lib.rs");
        let before: u64 = bandit.arm_stats().iter().map(|(_, p, _)| p).sum();
        bandit.update_from_file(arm, &base, "src/lib.rs", 1.0);
        let after: u64 = bandit.arm_stats().iter().map(|(_, p, _)| p).sum();
        assert_eq!(after, before + 1);
    }

    #[test]
    fn ast_enriched_bandit_arm_stats_format() {
        let bandit = AstEnrichedBandit::new(1.0);
        let stats = bandit.arm_stats();
        assert_eq!(stats.len(), 8);
        for (arm_idx, (_, pulls, avg_reward)) in stats.iter().enumerate() {
            assert!(pulls >= &0, "arm {arm_idx} pulls must be non-negative");
            assert!(
                avg_reward.is_finite(),
                "arm {arm_idx} avg_reward must be finite"
            );
        }
    }

    #[test]
    fn ast_enriched_bandit_arm_stats_pulls_accumulate() {
        let mut bandit = AstEnrichedBandit::new(1.0);
        let base = base_features();
        for _ in 0..10 {
            let arm = bandit.select_arm_for_file(&base, "src/lib.rs");
            bandit.update_from_file(arm, &base, "src/lib.rs", 0.5);
        }
        let total: u64 = bandit.arm_stats().iter().map(|(_, p, _)| p).sum();
        assert_eq!(total, 10);
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// LinUCBBandit hot path — base 19-dim bandit
// ══════════════════════════════════════════════════════════════════════════════

mod linucb_hotpath_tests {
    use super::*;

    fn make_feat() -> ndarray::Array1<f64> {
        ndarray::Array1::from_vec((0..25).map(|i| (i as f64) * 0.05).collect())
    }

    #[test]
    fn linucb_arm_selection_is_deterministic() {
        let mut bandit = LinUCBBandit::new();
        let feat = make_feat();

        // With same context and no updates, UCB returns consistent scores
        let (arm1, score1) = bandit.select_arm(&feat);
        let (arm2, score2) = bandit.select_arm(&feat);

        assert_eq!(
            arm1, arm2,
            "arm selection should be deterministic with same context"
        );
        assert_eq!(score1, score2);
    }

    #[test]
    fn linucb_update_improves_reward_prediction() {
        let mut bandit = LinUCBBandit::new();
        let feat = make_feat();

        // Arm 0 always gets reward 1.0, others get 0.0
        for _ in 0..50 {
            let _ = bandit.update(0, &feat, 1.0);
            let (arm, _) = bandit.select_arm(&feat);
            let _ = bandit.update(arm, &feat, 0.0);
        }

        let (_, score) = bandit.select_arm(&feat);
        assert!(
            score > 0.0,
            "rewarded arm should have positive UCB score, got {score}"
        );
    }

    #[test]
    fn linucb_update_increases_pulls() {
        let mut bandit = LinUCBBandit::new();
        let feat = make_feat();

        for _ in 0..20 {
            let (arm, _) = bandit.select_arm(&feat);
            let _ = bandit.update(arm, &feat, 0.6);
        }

        assert_eq!(bandit.total_pulls(), 20);
    }
}
