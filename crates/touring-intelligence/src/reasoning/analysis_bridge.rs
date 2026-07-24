//! Analysis bridge — wires `touring-analysis` KnowledgeReport / LearningReport
//! into the [`AdaptiveEngine`] bandit feedback loop.
//!
//! Enabled only when the `analysis-bridge` feature is active. The bridge is
//! intentionally **stateless**: it reads from the analysis reports and calls
//! `AdaptiveEngine::record_outcome` so the bandit can adjust future engine
//! selection without any additional mutable state.
//!
//! # Feature gate
//! ```toml
//! touring-cognitive = { features = ["analysis-bridge"] }
//! ```

use touring_analysis::knowledge::KnowledgeReport;
use touring_analysis::learning::{LearningReport, RewardTrend};

use crate::reasoning::adaptive_engine::AdaptiveEngine;

// ── CILA levels used for bandit feedback ────────────────────────────────────

/// CILA level used for knowledge-derived rewards (L2 — Tool-Augmented queries).
const CILA_KNOWLEDGE: u8 = 2;
/// CILA level used for learning-derived rewards (L4 — Agent Loop queries).
const CILA_LEARNING: u8 = 4;

// ── Engine name constants ────────────────────────────────────────────────────

const ENGINE_MCTS: &str = "MCTS";
const ENGINE_HYBRID: &str = "Hybrid";

// ── Public API ───────────────────────────────────────────────────────────────

/// Enrich the [`AdaptiveEngine`] bandit with codebase health signals.
///
/// Maps `KnowledgeReport` and `LearningReport` metrics to reward signals and
/// injects them via `AdaptiveEngine::record_outcome`, so the bandit learns to
/// prefer faster/simpler engines on healthy codebases and deeper engines when
/// the codebase shows instability or learning decay.
///
/// # Reward mapping
///
/// **Knowledge → MCTS (L2)**
/// - `knowledge.score` used directly as base reward
/// - Hot files > 10 → −0.10 penalty; > 5 → −0.05; ≤ 5 → 0.0
///
/// **Learning → Hybrid (L4)**
/// - `Improving` → +0.10 bonus
/// - `Stable` → +0.00 (no change)
/// - `Insufficient` → −0.05 penalty
/// - `Degrading` → −0.20 penalty
pub fn enrich_with_analysis(
    engine: &AdaptiveEngine,
    knowledge: &KnowledgeReport,
    learning: &LearningReport,
) {
    // ── Knowledge → MCTS bandit feedback (L2) ────────────────────────────
    let hot_penalty = if knowledge.hot_files > 10 {
        -0.10
    } else if knowledge.hot_files > 5 {
        -0.05
    } else {
        0.0
    };
    let mcts_reward = (knowledge.score + hot_penalty).clamp(0.0, 1.0);
    engine.record_outcome(CILA_KNOWLEDGE, ENGINE_MCTS, mcts_reward);

    // ── Learning → Hybrid bandit feedback (L4) ───────────────────────────
    let trend_delta = match learning.reward_trend {
        RewardTrend::Improving => 0.10,
        RewardTrend::Stable => 0.0,
        RewardTrend::Insufficient => -0.05,
        RewardTrend::Degrading => -0.20,
    };
    let hybrid_reward = (learning.score + trend_delta).clamp(0.0, 1.0);
    engine.record_outcome(CILA_LEARNING, ENGINE_HYBRID, hybrid_reward);
}

/// Produce a human-readable calibration summary from analysis reports.
///
/// Intended for telemetry, logging, and the `touring cognitive metrics` CLI.
pub fn calibration_summary(knowledge: &KnowledgeReport, learning: &LearningReport) -> String {
    let trend_label = match learning.reward_trend {
        RewardTrend::Improving => "improving",
        RewardTrend::Stable => "stable",
        RewardTrend::Insufficient => "insufficient",
        RewardTrend::Degrading => "degrading",
    };
    format!(
        "knowledge_score={:.3} hot_files={} active_gotchas={} import_health={:.3} | \
         learning_score={:.3} reward_trend={} rl_active={} avg_wilson={:.3}",
        knowledge.score,
        knowledge.hot_files,
        knowledge.active_gotchas,
        knowledge.import_graph_health,
        learning.score,
        trend_label,
        learning.rl_active,
        learning.avg_wilson_score,
    )
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn healthy_knowledge() -> KnowledgeReport {
        KnowledgeReport {
            total_files: 50,
            language_distribution: HashMap::from([("rust".to_string(), 50)]),
            avg_line_count: 120.0,
            avg_symbol_density: 0.08,
            hot_files: 1,
            active_gotchas: 0,
            import_graph_health: 0.75,
            score: 0.85,
        }
    }

    fn healthy_learning() -> LearningReport {
        LearningReport {
            wilson_tool_count: 10,
            avg_wilson_score: 0.72,
            qtable_entry_count: 200,
            linucb_arm_count: 8,
            linucb_total_pulls: 500,
            rl_active: true,
            reward_trend: RewardTrend::Stable,
            score: 0.80,
        }
    }

    // ── Test 1: enrich does not panic on a healthy codebase ──────────────

    #[test]
    fn enrich_does_not_panic_on_healthy_codebase() {
        let engine = AdaptiveEngine::with_defaults();
        let knowledge = healthy_knowledge();
        let learning = healthy_learning();

        // Must not panic under any circumstances
        enrich_with_analysis(&engine, &knowledge, &learning);

        // Bandit should have recorded 2 outcomes (MCTS@L2 + Hybrid@L4)
        assert_eq!(engine.total_trials(), 2);
    }

    // ── Test 2: degrading trend reduces Hybrid bandit reward ─────────────

    #[test]
    fn degrading_trend_reduces_hybrid_reward() {
        let engine_degrading = AdaptiveEngine::with_defaults();
        let engine_stable = AdaptiveEngine::with_defaults();

        let knowledge = healthy_knowledge();

        let mut learning_degrading = healthy_learning();
        learning_degrading.reward_trend = RewardTrend::Degrading;
        learning_degrading.score = 0.80;

        let mut learning_stable = healthy_learning();
        learning_stable.reward_trend = RewardTrend::Stable;
        learning_stable.score = 0.80;

        enrich_with_analysis(&engine_degrading, &knowledge, &learning_degrading);
        enrich_with_analysis(&engine_stable, &knowledge, &learning_stable);

        let stats_degrading = engine_degrading.stats();
        let stats_stable = engine_stable.stats();

        let (avg_degrading, _) = stats_degrading
            .get(&format!("L{}:{}", CILA_LEARNING, ENGINE_HYBRID))
            .copied()
            .expect("Hybrid@L4 stats must exist for degrading");

        let (avg_stable, _) = stats_stable
            .get(&format!("L{}:{}", CILA_LEARNING, ENGINE_HYBRID))
            .copied()
            .expect("Hybrid@L4 stats must exist for stable");

        assert!(
            avg_degrading < avg_stable,
            "degrading reward ({avg_degrading:.3}) should be less than stable ({avg_stable:.3})"
        );
    }

    // ── Test 3: hot files penalty is applied to MCTS reward ──────────────

    #[test]
    fn hot_files_penalty_applied() {
        // No penalty: hot_files = 1
        let engine_cool = AdaptiveEngine::with_defaults();
        let mut knowledge_cool = healthy_knowledge();
        knowledge_cool.hot_files = 1;
        knowledge_cool.score = 0.80;
        enrich_with_analysis(&engine_cool, &knowledge_cool, &healthy_learning());

        // Moderate penalty: hot_files = 7 (> 5, ≤ 10)
        let engine_warm = AdaptiveEngine::with_defaults();
        let mut knowledge_warm = healthy_knowledge();
        knowledge_warm.hot_files = 7;
        knowledge_warm.score = 0.80;
        enrich_with_analysis(&engine_warm, &knowledge_warm, &healthy_learning());

        // Heavy penalty: hot_files = 15 (> 10)
        let engine_hot = AdaptiveEngine::with_defaults();
        let mut knowledge_hot = healthy_knowledge();
        knowledge_hot.hot_files = 15;
        knowledge_hot.score = 0.80;
        enrich_with_analysis(&engine_hot, &knowledge_hot, &healthy_learning());

        let key = format!("L{}:{}", CILA_KNOWLEDGE, ENGINE_MCTS);

        let (cool_reward, _) = engine_cool
            .stats()
            .get(&key)
            .copied()
            .expect("MCTS@L2 cool");
        let (warm_reward, _) = engine_warm
            .stats()
            .get(&key)
            .copied()
            .expect("MCTS@L2 warm");
        let (hot_reward, _) = engine_hot.stats().get(&key).copied().expect("MCTS@L2 hot");

        assert!(
            cool_reward > warm_reward,
            "cool ({cool_reward:.3}) should beat warm ({warm_reward:.3})"
        );
        assert!(
            warm_reward > hot_reward,
            "warm ({warm_reward:.3}) should beat hot ({hot_reward:.3})"
        );
    }

    // ── Test 4: calibration_summary contains key fields ──────────────────

    #[test]
    fn calibration_summary_contains_key_fields() {
        let knowledge = healthy_knowledge();
        let mut learning = healthy_learning();
        learning.reward_trend = RewardTrend::Improving;

        let summary = calibration_summary(&knowledge, &learning);

        assert!(
            summary.contains("knowledge_score="),
            "missing knowledge_score: {summary}"
        );
        assert!(
            summary.contains("hot_files="),
            "missing hot_files: {summary}"
        );
        assert!(
            summary.contains("active_gotchas="),
            "missing active_gotchas: {summary}"
        );
        assert!(
            summary.contains("import_health="),
            "missing import_health: {summary}"
        );
        assert!(
            summary.contains("learning_score="),
            "missing learning_score: {summary}"
        );
        assert!(
            summary.contains("reward_trend=improving"),
            "missing reward_trend: {summary}"
        );
        assert!(
            summary.contains("rl_active=true"),
            "missing rl_active: {summary}"
        );
        assert!(
            summary.contains("avg_wilson="),
            "missing avg_wilson: {summary}"
        );
    }
}
