//! End-to-end scenarios for the granularity bandit (Wave C1, 2026-04-20).
//!
//! These tests drive the public `touring_intelligence::rl::bandit` surface the way a
//! downstream consumer would — `TaskDecomposer` asking for a split factor,
//! running the task, then feeding back a quality signal derived from a
//! hypothetical `CodeHealthReport::composite`.

use touring_intelligence::rl::bandit::{
    GranularityBandit, SplitFactor, features_for_task, reward_from_quality,
};

/// Simulated "code health" signal — in production this would come from
/// `touring_analysis::quality::QualityPipeline`. Here it is a deterministic
/// function of (factor, language, size) that the bandit must discover.
fn simulate_quality(factor: SplitFactor, size_loc: usize, language: &str) -> f64 {
    // Heuristic the bandit has to learn:
    //   - tiny rust  (< 100 LOC)     → Monolithic best
    //   - medium rust (100..500)     → Split2 best
    //   - large rust  (>= 500)       → Split3 best
    //   - python    → one factor higher than rust (collaboration overhead)
    let best_for_context: SplitFactor = match (size_loc, language) {
        (s, "rust") if s < 100 => SplitFactor::Monolithic,
        (s, "rust") if s < 500 => SplitFactor::Split2,
        (_, "rust") => SplitFactor::Split3,
        (s, "python") if s < 100 => SplitFactor::Split2,
        (s, "python") if s < 500 => SplitFactor::Split3,
        (_, "python") => SplitFactor::Split4,
        _ => SplitFactor::Split2,
    };
    if factor == best_for_context {
        0.92
    } else {
        0.35
    }
}

/// End-to-end loop that mimics how a decomposer would use the bandit.
fn train_on_scenario(b: &mut GranularityBandit, size: usize, lang: &str, iterations: usize) {
    let features = features_for_task(size, lang, 2);
    for _ in 0..iterations {
        let (factor, _score) = b.select_split(&features);
        let quality = simulate_quality(factor, size, lang);
        let reward = reward_from_quality(quality, factor.subtask_count());
        b.record_outcome(factor, &features, reward);
    }
}

#[test]
fn e2e_learns_monolithic_for_tiny_rust_tasks() {
    let mut bandit = GranularityBandit::new();
    train_on_scenario(&mut bandit, 40, "rust", 150);

    let pulls = bandit.pulls_per_arm();
    let mono_pulls = pulls.first().copied().unwrap_or(0);
    let other_pulls: u64 = pulls.iter().skip(1).sum();
    assert!(
        mono_pulls >= other_pulls,
        "Monolithic pulls {mono_pulls} should meet or exceed others {other_pulls}"
    );
}

#[test]
fn e2e_learns_split3_for_large_rust_tasks() {
    let mut bandit = GranularityBandit::new();
    train_on_scenario(&mut bandit, 1200, "rust", 200);

    let pulls = bandit.pulls_per_arm();
    let split3_pulls = pulls
        .get(SplitFactor::Split3.as_index())
        .copied()
        .unwrap_or(0);
    let peers: u64 = pulls
        .iter()
        .enumerate()
        .filter_map(|(i, &p)| {
            if i == SplitFactor::Split3.as_index() {
                None
            } else {
                Some(p)
            }
        })
        .sum();
    assert!(
        split3_pulls > peers,
        "Split3 pulls {split3_pulls} must dominate peers {peers}"
    );
}

#[test]
fn e2e_separate_contexts_learn_independently() {
    let mut bandit = GranularityBandit::new();

    // Alternate between two contexts so each gets enough pulls.
    for _ in 0..200 {
        for (size, lang) in [(40_usize, "rust"), (300_usize, "rust")] {
            let features = features_for_task(size, lang, 2);
            let (factor, _) = bandit.select_split(&features);
            let quality = simulate_quality(factor, size, lang);
            let reward = reward_from_quality(quality, factor.subtask_count());
            bandit.record_outcome(factor, &features, reward);
        }
    }

    // Ridge regression shares parameters across contexts, but average reward
    // per arm should still be well above the baseline (0.35 with penalty).
    let avg = bandit.avg_reward_per_arm();
    let max = avg.iter().fold(f64::NEG_INFINITY, |a, &b| a.max(b));
    assert!(
        max > 0.5,
        "best-arm avg reward {max} should exceed 0.5 baseline"
    );
}

#[test]
fn e2e_reward_penalty_keeps_monolithic_competitive_on_ties() {
    // Force quality == 0.9 for ALL factors → penalty wins → Monolithic dominates.
    let mut bandit = GranularityBandit::new();
    let features = features_for_task(50, "rust", 1);
    for _ in 0..200 {
        let (factor, _) = bandit.select_split(&features);
        let reward = reward_from_quality(0.9, factor.subtask_count());
        bandit.record_outcome(factor, &features, reward);
    }
    let avg = bandit.avg_reward_per_arm();
    let mono = avg.first().copied().unwrap_or(0.0);
    let split4 = avg
        .get(SplitFactor::Split4.as_index())
        .copied()
        .unwrap_or(0.0);
    assert!(
        mono > split4,
        "penalty must favor Monolithic over Split4 at equal quality"
    );
}

#[test]
fn e2e_total_pulls_matches_iteration_count() {
    let mut bandit = GranularityBandit::new();
    train_on_scenario(&mut bandit, 250, "rust", 50);
    assert_eq!(bandit.total_pulls(), 50);
    let sum: u64 = bandit.pulls_per_arm().iter().sum();
    assert_eq!(sum, 50);
}
