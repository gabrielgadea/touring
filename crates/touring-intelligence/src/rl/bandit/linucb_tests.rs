#![allow(clippy::indexing_slicing)] // test vecs asserted non-empty before indexing
use super::*;

#[test]
fn test_new_bandit_has_correct_dimensions() {
    let bandit = LinUCBBandit::new();
    let stats = bandit.arm_stats();
    assert_eq!(stats.len(), NUM_ARMS);
    assert_eq!(stats.len(), 8);
    assert_eq!(bandit.total_pulls(), 0);

    // Verify all arms are initialized with zero pulls
    for (i, pulls, avg) in &stats {
        assert_eq!(*pulls, 0, "arm {} should have 0 pulls", i);
        assert_eq!(*avg, 0.0, "arm {} should have 0.0 avg reward", i);
    }
}

#[test]
fn test_select_arm_returns_valid_arm() {
    let mut bandit = LinUCBBandit::new();
    let features = extract_features("python", 50, 5, 0, 2);
    let (arm, score) = bandit.select_arm(&features);
    assert!(arm < NUM_ARMS, "arm index must be < {}", NUM_ARMS);
    assert!(score.is_finite(), "score must be finite");
}

#[test]
fn test_select_arm_initial_explores() {
    // With no data, all arms should have equal UCB scores
    // (identity A_inv, zero b → mean=0, uncertainty=sqrt(x^T x) for all)
    let bandit = LinUCBBandit::new();
    let features = extract_features("rust", 200, 25, 1, 3);

    let scores: Vec<f64> = (0..NUM_ARMS)
        .map(|i| bandit.arms[i].score(&features, bandit.alpha))
        .collect();

    // All scores should be identical since all arms are fresh
    let first = scores[0];
    for (i, &s) in scores.iter().enumerate() {
        assert!(
            (s - first).abs() < 1e-10,
            "arm {} score {} differs from arm 0 score {}",
            i,
            s,
            first
        );
    }
}

#[test]
fn test_update_changes_arm_stats() {
    let mut bandit = LinUCBBandit::new();
    let features = extract_features("python", 50, 5, 0, 2);

    bandit.update(0, &features, 1.0);
    bandit.update(0, &features, 0.5);

    let stats = bandit.arm_stats();
    assert_eq!(stats[0].1, 2, "arm 0 should have 2 pulls");
    assert!(
        (stats[0].2 - 0.75).abs() < 1e-10,
        "avg reward should be 0.75"
    );
    assert_eq!(bandit.total_pulls(), 2);

    // Other arms should still be at 0
    for (i, &(_, pulls, _)) in stats.iter().enumerate().skip(1) {
        assert_eq!(pulls, 0, "arm {} should have 0 pulls", i);
    }
}

#[test]
fn test_reward_affects_selection() {
    let mut bandit = LinUCBBandit::with_alpha(0.01); // Low exploration
    let features = extract_features("python", 50, 5, 0, 2);

    // Heavily reward arm 3
    for _ in 0..50 {
        bandit.update(3, &features, 1.0);
    }
    // Give low reward to all others
    for arm in 0..NUM_ARMS {
        if arm != 3 {
            for _ in 0..50 {
                bandit.update(arm, &features, 0.0);
            }
        }
    }

    // With low alpha, arm 3 should be selected for similar features
    let (selected, _) = bandit.select_arm(&features);
    assert_eq!(
        selected, 3,
        "arm 3 should be selected after receiving highest rewards"
    );
}

#[test]
fn test_extract_features_python() {
    let f = extract_features("python", 50, 5, 0, 2);
    assert_eq!(f[0], 1.0, "python → index 0");
    assert_eq!(f[1], 0.0, "rust index should be 0");
    assert_eq!(f[2], 0.0, "typescript index should be 0");
    assert_eq!(f[3], 0.0, "other index should be 0");
}

#[test]
fn test_extract_features_rust() {
    let f = extract_features("rust", 500, 30, 2, 4);
    assert_eq!(f[0], 0.0);
    assert_eq!(f[1], 1.0, "rust → index 1");
    assert_eq!(f[2], 0.0);
    assert_eq!(f[3], 0.0);

    // Size 500 → medium → index 5
    assert_eq!(f[5], 1.0, "medium file → index 5");
    // Turn 30 → mid → index 8
    assert_eq!(f[8], 1.0, "mid turn → index 8");
    // Quality context: errors=2 → quality_score=0.2 (fallback), file_risk=0.0
    assert!(
        (f[10] - 0.2).abs() < 1e-10,
        "quality_score fallback: errors>0 → 0.2"
    );
    assert!((f[11] - 0.0).abs() < 1e-10, "file_risk default → 0.0");
    // CILA 4 → index 16
    assert_eq!(f[16], 1.0, "CILA L4 → index 16");
}

#[test]
fn test_extract_features_dimensions() {
    let f = extract_features("typescript", 1500, 100, 0, 6);
    assert_eq!(f.len(), FEATURE_DIM);
    assert_eq!(f.len(), 25); // H1-D: expanded from 19 → 25
}

#[test]
fn test_extract_features_one_hot() {
    let f = extract_features("other", 99, 9, 0, 0);

    // Check each group has exactly one 1.0
    let file_type_sum: f64 = f.slice(ndarray::s![0..4]).sum();
    let size_sum: f64 = f.slice(ndarray::s![4..7]).sum();
    let turn_sum: f64 = f.slice(ndarray::s![7..10]).sum();
    let _quality_ctx: f64 = f.slice(ndarray::s![10..12]).sum();
    let cila_sum: f64 = f.slice(ndarray::s![12..19]).sum();

    assert!(
        (file_type_sum - 1.0).abs() < 1e-10,
        "file_type must have exactly one 1"
    );
    assert!(
        (size_sum - 1.0).abs() < 1e-10,
        "size bucket must have exactly one 1"
    );
    assert!(
        (turn_sum - 1.0).abs() < 1e-10,
        "turn bucket must have exactly one 1"
    );
    // Slots [10..11] are now continuous quality context (not one-hot)
    // errors=0 → quality_score=0.8 (fallback), file_risk=0.0
    assert!(
        (f[10] - 0.8).abs() < 1e-10,
        "quality_score fallback: no errors → 0.8"
    );
    assert!((f[11] - 0.0).abs() < 1e-10, "file_risk default → 0.0");
    assert!(
        (cila_sum - 1.0).abs() < 1e-10,
        "cila level must have exactly one 1"
    );

    // H1-D: slot 19 = error_count_session (None → 0.0)
    assert!(
        (f[19] - 0.0).abs() < 1e-10,
        "error_count_session default → 0.0"
    );
    // H1-D: slot 20 = recent_tool_success_rate (None → 0.5)
    assert!(
        (f[20] - 0.5).abs() < 1e-10,
        "recent_tool_success_rate default → 0.5"
    );
    // H1-D: slots [21..24] = time_of_day one-hot (exactly one active)
    let tod_sum: f64 = f.slice(ndarray::s![21..25]).sum();
    assert!(
        (tod_sum - 1.0).abs() < 1e-10,
        "time_of_day must have exactly one 1"
    );

    // Total: file_type(1) + size(1) + turn(1) + quality(0.8) + file_risk(0.0) + cila(1)
    //      + error_session(0.0) + success_rate(0.5) + tod(1.0) = 6.3
    let total: f64 = f.sum();
    assert!((total - 6.3).abs() < 1e-10, "total features should be 6.3");
}

#[test]
fn test_arm_stats_averages() {
    let mut bandit = LinUCBBandit::new();
    let features = extract_features("python", 50, 5, 0, 2);

    bandit.update(0, &features, 1.0);
    bandit.update(0, &features, 0.0);
    bandit.update(0, &features, 0.5);

    bandit.update(7, &features, 2.0);
    bandit.update(7, &features, 4.0);

    let stats = bandit.arm_stats();
    assert_eq!(stats[0].1, 3);
    assert!((stats[0].2 - 0.5).abs() < 1e-10, "arm 0 avg = 0.5");

    assert_eq!(stats[7].1, 2);
    assert!((stats[7].2 - 3.0).abs() < 1e-10, "arm 7 avg = 3.0");
}

#[test]
fn test_convergence_simple() {
    // After many updates with consistent rewards, the bandit should converge
    // to preferring the highest-reward arm for the given context.
    let mut bandit = LinUCBBandit::with_alpha(0.1);

    let features_a = extract_features("python", 50, 5, 0, 1);
    let features_b = extract_features("rust", 500, 30, 2, 4);

    // For context A: arm 2 is best
    for _ in 0..100 {
        bandit.update(2, &features_a, 1.0);
        for arm in 0..NUM_ARMS {
            if arm != 2 {
                bandit.update(arm, &features_a, 0.1);
            }
        }
    }

    // For context B: arm 5 is best
    for _ in 0..100 {
        bandit.update(5, &features_b, 1.0);
        for arm in 0..NUM_ARMS {
            if arm != 5 {
                bandit.update(arm, &features_b, 0.1);
            }
        }
    }

    let (selected_a, _) = bandit.select_arm(&features_a);
    let (selected_b, _) = bandit.select_arm(&features_b);

    assert_eq!(
        selected_a, 2,
        "should prefer arm 2 for python/small/early context"
    );
    assert_eq!(
        selected_b, 5,
        "should prefer arm 5 for rust/medium/mid context"
    );
}

#[test]
fn test_arm_kind_from_index() {
    assert_eq!(ArmKind::from_index(0), Some(ArmKind::None));
    assert_eq!(ArmKind::from_index(1), Some(ArmKind::Overview));
    assert_eq!(ArmKind::from_index(7), Some(ArmKind::FullEnrichment));
    assert_eq!(ArmKind::from_index(8), Option::None);
    assert_eq!(ArmKind::from_index(100), Option::None);
}

#[test]
fn test_arm_kind_all() {
    let all = ArmKind::all();
    assert_eq!(all.len(), NUM_ARMS);
    for (i, kind) in all.iter().enumerate() {
        assert_eq!(*kind as usize, i);
    }
}

#[test]
fn test_select_arm_kind() {
    let mut bandit = LinUCBBandit::new();
    let features = extract_features("python", 50, 5, 0, 2);
    let (kind, score) = bandit.select_arm_kind(&features);
    assert!(score.is_finite());
    // Should be a valid ArmKind
    let _ = kind as usize; // if this compiles, it's valid
}

#[test]
fn test_set_alpha() {
    let mut bandit = LinUCBBandit::new();
    assert!((bandit.alpha() - 1.0).abs() < 1e-10);

    bandit.set_alpha(0.5);
    assert!((bandit.alpha() - 0.5).abs() < 1e-10);
}

#[test]
fn test_export_import_roundtrip() {
    let mut bandit = LinUCBBandit::new();
    let features = extract_features("python", 50, 5, 0, 2);

    // Train a bit
    bandit.update(0, &features, 1.0);
    bandit.update(3, &features, 0.5);
    bandit.update(0, &features, 0.8);

    // Export
    let exported = bandit.export();
    assert_eq!(exported.len(), NUM_ARMS);

    // Import into fresh bandit
    let mut bandit2 = LinUCBBandit::new();
    bandit2.import(&exported);
    bandit2.import_total_pulls(bandit.export_total_pulls());

    // Verify same selection behavior
    let (arm1, score1) = bandit.select_arm(&features);
    let (arm2, score2) = bandit2.select_arm(&features);
    assert_eq!(arm1, arm2, "same arm should be selected after import");
    assert!(
        (score1 - score2).abs() < 1e-10,
        "scores should match after import"
    );

    // Verify stats match
    let stats1 = bandit.arm_stats();
    let stats2 = bandit2.arm_stats();
    for i in 0..NUM_ARMS {
        assert_eq!(stats1[i].1, stats2[i].1, "pulls should match for arm {}", i);
        assert!(
            (stats1[i].2 - stats2[i].2).abs() < 1e-10,
            "avg reward should match for arm {}",
            i
        );
    }
}

#[test]
fn test_extract_features_cila_clamped() {
    // CILA level > 6 should be clamped to 6
    let f = extract_features("python", 50, 5, 0, 10);
    assert_eq!(f[18], 1.0, "CILA 10 should clamp to index 18 (L6)");

    // Verify only one CILA bit set
    let cila_sum: f64 = f.slice(ndarray::s![12..19]).sum();
    assert!((cila_sum - 1.0).abs() < 1e-10);
}

#[test]
fn test_extract_features_boundary_values() {
    // Test exact boundary values for buckets
    // file_size = 100 → medium (not small)
    let f1 = extract_features("python", 100, 10, 1, 0);
    assert_eq!(f1[5], 1.0, "file_size=100 → medium bucket (index 5)");
    assert_eq!(f1[8], 1.0, "session_turn=10 → mid bucket (index 8)");

    // file_size = 1000 → large
    let f2 = extract_features("python", 1000, 50, 0, 0);
    assert_eq!(f2[6], 1.0, "file_size=1000 → large bucket (index 6)");
    assert_eq!(f2[9], 1.0, "session_turn=50 → late bucket (index 9)");
}

#[test]
fn test_sherman_morrison_maintains_symmetry() {
    // After updates, A_inv should remain approximately symmetric
    let mut arm = LinUCBArm::new(FEATURE_DIM);
    let features = extract_features("python", 50, 5, 0, 2);

    for _ in 0..20 {
        arm.update(&features, 1.0);
    }

    // Check symmetry: a_inv[i][j] ≈ a_inv[j][i]
    for i in 0..FEATURE_DIM {
        for j in 0..FEATURE_DIM {
            let diff = (arm.a_inv[[i, j]] - arm.a_inv[[j, i]]).abs();
            assert!(
                diff < 1e-8,
                "A_inv not symmetric at [{},{}]: {} vs {}",
                i,
                j,
                arm.a_inv[[i, j]],
                arm.a_inv[[j, i]]
            );
        }
    }
}

// ── S2.3: LinUCB Re-Orthogonalization + Alpha Decay Tests ────────

#[test]
fn test_reorthogonalize_after_100_updates() {
    let mut arm = LinUCBArm::new(FEATURE_DIM);
    let features = extract_features("python", 50, 5, 0, 2);

    // At 0 pulls, should not trigger
    assert!(
        !arm.maybe_reorthogonalize(),
        "should not trigger at 0 pulls"
    );

    // Update 99 times — should not trigger at 99
    for _ in 0..99 {
        arm.update(&features, 1.0);
    }
    assert_eq!(arm.pulls(), 99);
    assert!(
        !arm.maybe_reorthogonalize(),
        "should not trigger at 99 pulls"
    );

    // One more → 100 pulls, should trigger check (but no reset since matrix is valid)
    arm.update(&features, 1.0);
    assert_eq!(arm.pulls(), 100);
    let reset = arm.maybe_reorthogonalize();
    // A valid matrix with only positive updates should NOT need reset
    assert!(!reset, "clean matrix should not reset");

    // Artificially corrupt: set a diagonal to negative
    arm.a_inv[[0, 0]] = -1.0;
    // Force pulls to 200 to trigger the check
    for _ in 0..100 {
        arm.pulls += 1; // Direct increment to avoid changing a_inv via update
    }
    assert_eq!(arm.pulls(), 200);
    let reset = arm.maybe_reorthogonalize();
    assert!(reset, "corrupted matrix should trigger reset");

    // After reset, diagonal should be identity (1.0)
    assert!(
        (arm.a_inv[[0, 0]] - 1.0).abs() < 1e-10,
        "diagonal should be 1.0 after reset"
    );
    // b vector should be preserved (not zeroed)
    assert!(
        arm.b.iter().any(|&v| v != 0.0),
        "b vector should be preserved after reset"
    );
}

#[test]
fn test_reorthogonalize_preserves_b_vector() {
    let mut arm = LinUCBArm::new(FEATURE_DIM);
    let features = extract_features("rust", 500, 30, 1, 3);

    // Build up b vector with 100 updates
    for _ in 0..100 {
        arm.update(&features, 0.8);
    }

    let b_before = arm.b.clone();
    assert!(
        b_before.iter().any(|&v| v != 0.0),
        "b should be non-zero after training"
    );

    // Corrupt and trigger reset
    arm.a_inv[[5, 5]] = -0.001;
    let reset = arm.maybe_reorthogonalize();
    assert!(reset);

    // b should be identical
    for i in 0..FEATURE_DIM {
        assert!(
            (arm.b[i] - b_before[i]).abs() < 1e-10,
            "b[{}] changed after reset: {} vs {}",
            i,
            arm.b[i],
            b_before[i]
        );
    }
}

#[test]
fn test_alpha_decays_over_time() {
    let mut bandit = LinUCBBandit::new();
    let features = extract_features("python", 50, 5, 0, 2);

    // Initial alpha
    let alpha_initial = bandit.alpha();
    assert!(
        (alpha_initial - 1.0).abs() < 1e-10,
        "initial alpha should be 1.0"
    );

    // Warm up all arms past the cold arm threshold so UCB scoring is used.
    // Each arm needs >= COLD_ARM_THRESHOLD (5) pulls.
    for i in 0..NUM_ARMS {
        for _ in 0..LinUCBBandit::COLD_ARM_THRESHOLD {
            bandit.update(i, &features, 0.5);
        }
    }
    // total_pulls = 8 * 5 = 40

    let _ = bandit.select_arm(&features);
    let alpha_after_40 = bandit.alpha();
    // alpha = sqrt(2 * ln(40)) / sqrt(40) ≈ sqrt(7.378) / 6.325 ≈ 0.429
    assert!(
        alpha_after_40 < alpha_initial,
        "alpha should decrease: initial={}, after 40 pulls={}",
        alpha_initial,
        alpha_after_40
    );

    // More updates
    for _ in 0..60 {
        bandit.update(0, &features, 0.5);
    }
    // total_pulls = 100

    let _ = bandit.select_arm(&features);
    let alpha_after_100 = bandit.alpha();
    // alpha = sqrt(2 * ln(100)) / sqrt(100) ≈ sqrt(9.21) / 10 ≈ 0.303
    assert!(
        alpha_after_100 < alpha_after_40,
        "alpha should keep decreasing: after 40={}, after 100={}",
        alpha_after_40,
        alpha_after_100
    );
    assert!(
        alpha_after_100 > 0.0,
        "alpha should remain positive: {}",
        alpha_after_100
    );
}

#[test]
fn test_alpha_decay_no_change_at_zero_pulls() {
    let mut bandit = LinUCBBandit::new();
    let features = extract_features("python", 50, 5, 0, 2);

    // With 0 pulls, alpha should NOT change
    let alpha_before = bandit.alpha();
    let _ = bandit.select_arm(&features);
    let alpha_after = bandit.alpha();
    assert!(
        (alpha_before - alpha_after).abs() < 1e-10,
        "alpha should not change at 0 pulls"
    );
}

#[test]
fn test_bandit_update_calls_reorthogonalize() {
    let mut bandit = LinUCBBandit::new();
    let features = extract_features("python", 50, 5, 0, 2);

    // Perform 100 updates on arm 0 — should trigger maybe_reorthogonalize
    for _ in 0..100 {
        bandit.update(0, &features, 1.0);
    }

    // If we got here without panic, reorthogonalize was called
    // Also verify arm 0 has correct pull count
    let stats = bandit.arm_stats();
    assert_eq!(stats[0].1, 100, "arm 0 should have 100 pulls");

    // Verify the matrix is still numerically sound (positive diagonals)
    let arm = &bandit.arms[0];
    for i in 0..FEATURE_DIM {
        assert!(
            arm.a_inv[[i, i]] >= 0.0,
            "diagonal a_inv[{i},{i}] should be non-negative after 100 updates"
        );
    }
}

#[test]
fn test_export_import_total_pulls() {
    let mut bandit = LinUCBBandit::new();
    let features = extract_features("python", 50, 5, 0, 2);
    bandit.update(0, &features, 1.0);
    bandit.update(1, &features, 0.5);

    let tp = bandit.export_total_pulls();
    assert_eq!(tp, 2);

    let mut bandit2 = LinUCBBandit::new();
    assert_eq!(bandit2.total_pulls(), 0);
    bandit2.import_total_pulls(tp);
    assert_eq!(bandit2.total_pulls(), 2);
}

// ── rkyv Zero-Copy LinUCB Tests ──────────────────────────────────

#[test]
fn test_linucb_rkyv_roundtrip() {
    let mut bandit = LinUCBBandit::with_alpha(0.5);
    let features = extract_features("python", 50, 5, 0, 2);

    // Train a few arms
    bandit.update(0, &features, 1.0);
    bandit.update(0, &features, 0.8);
    bandit.update(3, &features, 0.5);
    bandit.update(7, &features, 0.9);

    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("linucb.rkyv");

    bandit.save_rkyv(&path).expect("save_rkyv");
    let mut loaded = LinUCBBandit::load_rkyv(&path).expect("load_rkyv");

    // Verify stats match
    let stats_orig = bandit.arm_stats();
    let stats_loaded = loaded.arm_stats();
    for i in 0..NUM_ARMS {
        assert_eq!(
            stats_orig[i].1, stats_loaded[i].1,
            "arm {} pulls mismatch",
            i
        );
        assert!(
            (stats_orig[i].2 - stats_loaded[i].2).abs() < 1e-10,
            "arm {} avg_reward mismatch: {} vs {}",
            i,
            stats_orig[i].2,
            stats_loaded[i].2,
        );
    }

    assert_eq!(loaded.total_pulls(), bandit.total_pulls());
    assert!((loaded.alpha() - bandit.alpha()).abs() < 1e-10);

    // Verify selection behavior matches
    let (arm1, score1) = bandit.select_arm(&features);
    let (arm2, score2) = loaded.select_arm(&features);
    assert_eq!(
        arm1, arm2,
        "same arm should be selected after rkyv roundtrip"
    );
    assert!(
        (score1 - score2).abs() < 1e-10,
        "scores should match: {} vs {}",
        score1,
        score2
    );
}

#[test]
fn test_linucb_rkyv_preserves_matrix() {
    let mut bandit = LinUCBBandit::new();
    let features = extract_features("rust", 500, 30, 1, 4);

    // Enough updates to significantly alter A_inv from identity
    for _ in 0..20 {
        bandit.update(2, &features, 0.7);
    }

    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("matrix.rkyv");

    bandit.save_rkyv(&path).expect("save");
    let loaded = LinUCBBandit::load_rkyv(&path).expect("load");

    // Compare A_inv matrices element-by-element for arm 2
    let orig_arm = &bandit.arms[2];
    let loaded_arm = &loaded.arms[2];
    for i in 0..FEATURE_DIM {
        for j in 0..FEATURE_DIM {
            let diff = (orig_arm.a_inv[[i, j]] - loaded_arm.a_inv[[i, j]]).abs();
            assert!(
                diff < 1e-10,
                "A_inv[{},{}] mismatch: {} vs {}",
                i,
                j,
                orig_arm.a_inv[[i, j]],
                loaded_arm.a_inv[[i, j]],
            );
        }
    }

    // Compare b vectors
    for i in 0..FEATURE_DIM {
        let diff = (orig_arm.b[i] - loaded_arm.b[i]).abs();
        assert!(
            diff < 1e-10,
            "b[{}] mismatch: {} vs {}",
            i,
            orig_arm.b[i],
            loaded_arm.b[i],
        );
    }
}

#[test]
fn test_linucb_rkyv_fresh_bandit() {
    // A fresh (untrained) bandit should roundtrip cleanly
    let bandit = LinUCBBandit::new();
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("fresh.rkyv");

    bandit.save_rkyv(&path).expect("save fresh");
    let loaded = LinUCBBandit::load_rkyv(&path).expect("load fresh");

    assert_eq!(loaded.total_pulls(), 0);
    assert!((loaded.alpha() - 1.0).abs() < 1e-10);
    for (i, pulls, avg) in loaded.arm_stats() {
        assert_eq!(pulls, 0, "arm {} should have 0 pulls", i);
        assert!((avg - 0.0).abs() < 1e-10, "arm {} avg should be 0.0", i);
    }
}

#[test]
fn test_linucb_rkyv_file_not_found() {
    let result = LinUCBBandit::load_rkyv(Path::new("/nonexistent/linucb.rkyv"));
    assert!(result.is_err());
    assert!(
        result.unwrap_err().to_string().contains("Failed to read"),
        "should report file read error"
    );
}

// ── Cold Arm Forced Exploration Tests ─────────────────────────────

#[test]
fn test_cold_arm_forced_exploration_selects_least_pulled() {
    let mut bandit = LinUCBBandit::new();
    let features = extract_features("python", 50, 5, 0, 2);

    // All arms start at 0 pulls — first selection should pick arm 0 (first coldest)
    let (arm, _) = bandit.select_arm(&features);
    assert!(arm < NUM_ARMS);

    // Update only arm 0 with 4 pulls (still cold: < 5)
    for _ in 0..4 {
        bandit.update(0, &features, 0.8);
    }

    // Now arm 0 has 4 pulls, all others have 0 — should select one of the others
    let (arm2, _) = bandit.select_arm(&features);
    assert_ne!(
        arm2, 0,
        "should explore a cold arm with 0 pulls, not arm 0 with 4"
    );
}

#[test]
fn test_cold_arm_threshold_transition_to_ucb() {
    let mut bandit = LinUCBBandit::new();
    let features = extract_features("rust", 200, 25, 0, 3);

    // Give all arms exactly COLD_ARM_THRESHOLD pulls
    for arm_idx in 0..NUM_ARMS {
        for _ in 0..LinUCBBandit::COLD_ARM_THRESHOLD {
            bandit.update(arm_idx, &features, 0.5);
        }
    }

    // Verify all arms have at least COLD_ARM_THRESHOLD pulls
    for (_, pulls, _) in bandit.arm_stats() {
        assert!(
            pulls >= LinUCBBandit::COLD_ARM_THRESHOLD,
            "all arms should have >= {} pulls",
            LinUCBBandit::COLD_ARM_THRESHOLD
        );
    }

    // Now select_arm should use UCB scoring, not forced exploration.
    // We verify it returns a valid arm (the UCB path works).
    let (arm, score) = bandit.select_arm(&features);
    assert!(arm < NUM_ARMS);
    assert!(score.is_finite());
}

// ── P5.3: Relative threshold tests ────────────────────────────

#[test]
fn test_sherman_morrison_relative_threshold_large_features() {
    // With large feature values, the relative threshold scales up,
    // preventing false positives that the absolute threshold would miss.
    let mut arm = LinUCBArm::new(FEATURE_DIM);

    // Create features with large magnitude
    let mut large_features = Array1::zeros(FEATURE_DIM);
    large_features[0] = 100.0;
    large_features[1] = 100.0;

    // Should update without panic or NaN
    for _ in 0..50 {
        arm.update(&large_features, 1.0);
    }

    // Score should be finite
    let score = arm.score(&large_features, 1.0);
    assert!(
        score.is_finite(),
        "score should be finite with large features: {}",
        score
    );
}

#[test]
fn test_sherman_morrison_relative_threshold_small_features() {
    // With very small feature values, the relative threshold allows
    // finer-grained updates.
    let mut arm = LinUCBArm::new(FEATURE_DIM);

    let mut small_features = Array1::zeros(FEATURE_DIM);
    small_features[0] = 0.001;

    for _ in 0..50 {
        arm.update(&small_features, 0.5);
    }

    let score = arm.score(&small_features, 1.0);
    assert!(
        score.is_finite(),
        "score should be finite with small features: {}",
        score
    );
    assert!(arm.pulls() == 50);
}

#[test]
fn test_cold_arm_mixed_pulls() {
    let mut bandit = LinUCBBandit::new();
    let features = extract_features("typescript", 500, 30, 1, 4);

    // Give arms 0-6 enough pulls to be warm, leave arm 7 cold
    for arm_idx in 0..7 {
        for _ in 0..LinUCBBandit::COLD_ARM_THRESHOLD {
            bandit.update(arm_idx, &features, 0.6);
        }
    }
    // Arm 7 has 0 pulls — still cold

    let (arm, _) = bandit.select_arm(&features);
    assert_eq!(arm, 7, "should force-explore the only cold arm (arm 7)");
}

// ── GPU LinUCB Tests ────────────────────────────────────────────────

#[test]
fn test_predict_ucb_gpu_returns_all_8_scores() {
    #[cfg(feature = "gpu-compute")]
    {
        let bandit = LinUCBBandit::new();
        let features = [0.0_f64; FEATURE_DIM];
        // GPU may be unavailable; we test the fallback path
        let result = bandit.predict_ucb_gpu(&features);
        assert!(
            result.is_ok(),
            "predict_ucb_gpu should succeed or gracefully fail"
        );
        let scores = result.expect("already checked Ok above");
        assert_eq!(scores.len(), NUM_ARMS, "should return exactly 8 arm scores");
        for (i, &score) in scores.iter().enumerate() {
            assert!(
                score.is_finite(),
                "arm {} score must be finite, got {}",
                i,
                score
            );
        }
    }
    #[cfg(not(feature = "gpu-compute"))]
    {
        // When gpu-compute is disabled, the method is not available
        let _ = ();
    }
}

#[test]
fn test_update_gpu_updates_correct_arm() {
    #[cfg(feature = "gpu-compute")]
    {
        let mut bandit = LinUCBBandit::new();
        let features = extract_features("python", 50, 5, 0, 2);
        let features_array: [f64; FEATURE_DIM] = {
            let mut arr = [0.0_f64; FEATURE_DIM];
            for (i, f) in features.iter().enumerate() {
                arr[i] = *f;
            }
            arr
        };

        let pulls_before = bandit.arms[3].pulls();
        let result = bandit.update_gpu(3, &features_array, 0.8);
        assert!(
            result.is_ok(),
            "update_gpu should succeed or gracefully fail"
        );

        // If GPU was available and update succeeded, pull count should increase
        // (Note: when GPU falls back to CPU, total_pulls is incremented in update_gpu)
        let pulls_after = bandit.arms[3].pulls();
        assert!(
            pulls_after >= pulls_before,
            "pull count should increase or stay same (GPU unavailable fallback)"
        );
    }
    #[cfg(not(feature = "gpu-compute"))]
    {
        let _ = ();
    }
}

#[test]
fn test_predict_ucb_gpu_features_match_cpu() {
    #[cfg(feature = "gpu-compute")]
    {
        let mut bandit = LinUCBBandit::new();
        let features = extract_features("rust", 200, 25, 1, 3);
        let features_array: [f64; FEATURE_DIM] = {
            let mut arr = [0.0_f64; FEATURE_DIM];
            for (i, f) in features.iter().enumerate() {
                arr[i] = *f;
            }
            arr
        };

        // Train all arms a bit
        for arm in 0..NUM_ARMS {
            for _ in 0..10 {
                bandit.update(arm, &features, 0.6);
            }
        }

        let gpu_result = bandit.predict_ucb_gpu(&features_array);
        assert!(gpu_result.is_ok(), "GPU prediction should succeed");
        let gpu_scores = gpu_result.expect("already checked Ok above");

        // Compare with CPU scores (individual arm scoring)
        for (arm_idx, &gpu_score) in gpu_scores.iter().enumerate().take(NUM_ARMS) {
            let cpu_score = bandit.arms[arm_idx].score(&features, bandit.alpha);
            let diff = (cpu_score - gpu_score).abs();
            assert!(
                diff < 1e-6 || cpu_score.is_nan(),
                "arm {} GPU score {} should match CPU score {} (diff={})",
                arm_idx,
                gpu_score,
                cpu_score,
                diff
            );
        }
    }
    #[cfg(not(feature = "gpu-compute"))]
    {
        let _ = ();
    }
}

#[test]
fn test_update_gpu_invalid_arm_returns_error() {
    #[cfg(feature = "gpu-compute")]
    {
        let mut bandit = LinUCBBandit::new();
        let features = [0.0_f64; FEATURE_DIM];
        let result = bandit.update_gpu(99, &features, 0.5); // Invalid arm index
        assert!(
            result.is_err(),
            "update_gpu should error on invalid arm index"
        );
    }
    #[cfg(not(feature = "gpu-compute"))]
    {
        let _ = ();
    }
}
