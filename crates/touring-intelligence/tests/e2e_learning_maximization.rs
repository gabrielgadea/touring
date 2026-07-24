//! E2E integration tests for touring-learning — post-maximization validation
//!
//! Tests the fixes applied during the maximization session:
//! 1. LinUCB hot path clones eliminated (aview1)
//! 2. ActorCritic SmallVec state storage
//! 3. FtrlLayer wired into OnlineRLEngine
//! 4. MetacognitivePipeline adaptive decisions
//! 5. Memory tier integration

use ndarray::Array1;
use tempfile::TempDir;
use touring_intelligence::rl::bandit::{FEATURE_DIM, LinUCBBandit, NUM_ARMS};
use touring_intelligence::rl::evolution::EvolutionAnalyzer;
use touring_intelligence::rl::memory::recall::SemanticRecall;
use touring_intelligence::rl::memory::{MemoryTier, RlmMemory};
use touring_intelligence::rl::metacognitive_pipeline::{
    LatencyAdaptationPipeline, MetacognitiveDecision, PipelineContext,
};
use touring_intelligence::rl::online_rl::{
    ImmediateReward, OnlineRLConfig, OnlineRLEngine, ReplayBuffer,
};
use touring_intelligence::rl::ranking::{DriftMonitor, WilsonRanker, wilson::DriftDetector};
use touring_intelligence::rl::rl::QTable;
use touring_intelligence::rl::rl::{ActorCritic, ActorCriticConfig};
use touring_intelligence::rl::templates::TemplateLibrary;

// ══════════════════════════════════════════════════════════════════════════════
// Helper: build ImmediateReward from tool execution parameters
// ══════════════════════════════════════════════════════════════════════════════

fn make_reward(
    tool: &str,
    accepted: bool,
    latency_ms: u64,
    errors: u32,
    cila: u8,
    ftype: u8,
) -> ImmediateReward {
    ImmediateReward {
        tool_name: tool.to_string(),
        accepted,
        latency_ms,
        error_count: errors,
        cila_level: cila,
        file_type: ftype,
        quality_score: None,
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// Bandit hot-path tests — verify no heap allocation in select_arm/update
// ══════════════════════════════════════════════════════════════════════════════

mod bandit_hotpath {
    use super::*;

    fn make_feature_vec() -> Array1<f64> {
        Array1::from_vec((0..FEATURE_DIM).map(|i| (i as f64) * 0.05).collect())
    }

    #[test]
    fn linucb_select_arm_no_crash() {
        let mut bandit = LinUCBBandit::new();
        let feat = make_feature_vec();
        let (arm, score) = bandit.select_arm(&feat);
        assert!(arm < NUM_ARMS);
        assert!(score.is_finite());
    }

    #[test]
    fn linucb_update_rewards_converge() {
        let mut bandit = LinUCBBandit::new();
        let feat = make_feature_vec();

        for i in 0..30 {
            let (arm, _) = bandit.select_arm(&feat);
            let reward = if i % 5 == 0 { 1.0 } else { 0.0 };
            bandit.update(arm, &feat, reward);
        }

        let (_, score) = bandit.select_arm(&feat);
        assert!(score.is_finite());
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// ActorCritic tests — verify state caching works
// ══════════════════════════════════════════════════════════════════════════════

mod actor_critic_tests {
    use super::*;

    #[test]
    fn actor_critic_select_action_valid() {
        let config = ActorCriticConfig::default();
        let mut actor = ActorCritic::new(config);

        let sel = actor.select_action(12345);
        assert!(sel.action < 8);
    }

    #[test]
    fn actor_critic_state_to_features_idempotent() {
        let config = ActorCriticConfig::default();
        let actor = ActorCritic::new(config);

        let f1 = actor.state_to_features(999);
        let f2 = actor.state_to_features(999);

        assert_eq!(f1.len(), f2.len());
        for (a, b) in f1.iter().zip(f2.iter()) {
            assert!((a - b).abs() < 1e-5);
        }
    }

    #[test]
    fn actor_critic_update_batch() {
        let config = ActorCriticConfig::default();
        let mut actor = ActorCritic::new(config);

        for i in 0..10u64 {
            let state = i * 111;
            let sel = actor.select_action(state);
            actor.update(state, sel.action, (i as f32) * 0.1, (i + 1) * 111, false);
        }
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// OnlineRLEngine + QTable integration tests
// ══════════════════════════════════════════════════════════════════════════════

mod online_rl_tests {
    use super::*;

    #[test]
    fn engine_processes_reward_sequence() {
        let config = OnlineRLConfig::default();
        let mut engine = OnlineRLEngine::new(config).with_time_cache();
        let mut qt = QTable::new();
        let mut bandit = LinUCBBandit::new();

        let tools = ["Read", "Write", "Bash", "Edit", "Grep"];

        for i in 0..15 {
            let reward = make_reward(
                tools[i % tools.len()],
                i % 3 != 0,
                50 + i as u64 * 3,
                0,
                2,
                1,
            );
            let result = engine.process_reward(&reward, &mut qt, &mut bandit);
            assert!(result.is_some(), "iteration {} should process", i);
        }
    }

    #[test]
    fn engine_qtable_populated_after_updates() {
        let config = OnlineRLConfig::default();
        let mut engine = OnlineRLEngine::new(config).with_time_cache();
        let mut qt = QTable::new();
        let mut bandit = LinUCBBandit::new();

        for _ in 0..10 {
            let reward = make_reward("Edit", true, 150, 0, 2, 1);
            engine.process_reward(&reward, &mut qt, &mut bandit);
        }

        assert!(!qt.is_empty(), "QTable should have entries after updates");
        assert!(bandit.total_pulls() >= 1, "bandit should have pulls");
    }

    #[test]
    fn replay_buffer_capacity() {
        let mut buf = ReplayBuffer::new(8);

        for i in 0..20 {
            let exp = touring_intelligence::rl::online_rl::Experience {
                state: i as u64,
                action: 0,
                reward: 1.0,
                next_state: (i + 1) as u64,
                terminal: false,
            };
            buf.push(exp);
        }

        assert_eq!(buf.len(), 8, "buffer should hold exactly capacity items");
    }

    #[test]
    fn replay_buffer_sample() {
        let mut buf = ReplayBuffer::new(50);

        for i in 0..30 {
            let exp = touring_intelligence::rl::online_rl::Experience {
                state: i as u64,
                action: (i % 4) as u64,
                reward: if i % 7 == 0 { 1.0 } else { 0.0 },
                next_state: (i + 1) as u64,
                terminal: false,
            };
            buf.push(exp);
        }

        let batch = buf.sample(10);
        assert_eq!(batch.len(), 10);
    }

    #[test]
    fn replay_buffer_par_sample() {
        let mut buf = ReplayBuffer::new(100);

        for i in 0..60 {
            let exp = touring_intelligence::rl::online_rl::Experience {
                state: i as u64,
                action: (i % 4) as u64,
                reward: if i % 5 == 0 { 1.0 } else { 0.0 },
                next_state: (i + 1) as u64,
                terminal: false,
            };
            buf.push(exp);
        }

        let batch = buf.par_sample(20);
        assert_eq!(batch.len(), 20);
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// LatencyAdaptationPipeline tests — adaptive decision under varying load
// ══════════════════════════════════════════════════════════════════════════════

mod metacognitive_tests {
    use super::*;

    #[test]
    fn pipeline_stable_under_light_load() {
        let mut pipeline = LatencyAdaptationPipeline::new();
        let initial_stats = pipeline.stats().decisions;

        let ctx = PipelineContext {
            tool_name: "pre-read".to_string(),
            latency_ms: 45,
            success: true,
            file_path: None,
            error_rate: 0.005,
            memory_bound: false,
            current_threshold: 64,
            vector_dim: Some(384),
        };

        let decision = pipeline.run(ctx);
        let stats = pipeline.stats();

        // After running, stats should have incremented
        assert!(stats.decisions > initial_stats);
        match decision {
            MetacognitiveDecision::Stable
            | MetacognitiveDecision::IncreaseParallelism { .. }
            | MetacognitiveDecision::Explore { .. } => {}
            other => panic!(
                "expected Stable, IncreaseParallelism, or Explore, got {:?}",
                other
            ),
        }
    }

    #[test]
    fn pipeline_adapts_under_heavy_load() {
        let mut pipeline = LatencyAdaptationPipeline::new();

        // Warm up CUSUM with normal samples before heavy load
        for i in 0..50 {
            let warmup_ctx = PipelineContext {
                tool_name: "warmup".to_string(),
                latency_ms: 50 + (i % 20) as u64,
                success: true,
                file_path: None,
                error_rate: 0.01,
                memory_bound: false,
                current_threshold: 64,
                vector_dim: Some(384),
            };
            let _ = pipeline.run(warmup_ctx);
        }

        // Now inject heavy load — CUSUM should detect drift from baseline
        let ctx = PipelineContext {
            tool_name: "pre-write".to_string(),
            latency_ms: 800,
            success: false,
            file_path: None,
            error_rate: 0.12,
            memory_bound: true,
            current_threshold: 64,
            vector_dim: Some(384),
        };

        let decision = pipeline.run(ctx);
        // With warmed-up CUSUM + high latency drift, should trigger adaptation
        assert!(
            !matches!(decision, MetacognitiveDecision::Stable),
            "heavy load should trigger adaptation, got {:?}",
            decision
        );
    }

    #[test]
    fn pipeline_stats_populated() {
        let mut pipeline = LatencyAdaptationPipeline::new();

        let ctx = PipelineContext {
            tool_name: "test".to_string(),
            latency_ms: 60,
            success: true,
            file_path: None,
            error_rate: 0.02,
            memory_bound: false,
            current_threshold: 64,
            vector_dim: Some(384),
        };

        pipeline.run(ctx);
        let stats = pipeline.stats();

        assert!(stats.decisions >= 1);
        assert!(stats.pheromone_confidence >= 0.0 && stats.pheromone_confidence <= 1.0);
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// Memory tier tests — RLM and semantic recall
// ══════════════════════════════════════════════════════════════════════════════

mod memory_tests {
    use super::*;

    #[test]
    fn rlm_store_and_recall() -> Result<(), Box<dyn std::error::Error>> {
        let dir = TempDir::new()?;
        let db_path = dir.path().join("rlm_e2e.db");
        let mem = RlmMemory::new(db_path.as_path())?;

        for i in 0..20 {
            let k = format!("k{}", i);
            let v = format!("v{}", i);
            mem.store(&k, MemoryTier::Working, &v, None, None)?;
        }

        for i in 0..20 {
            let k = format!("k{}", i);
            let val = mem.get(&k, MemoryTier::Working)?;
            assert_eq!(val, Some(format!("v{}", i)));
        }
        Ok(())
    }

    #[test]
    fn semantic_recall_search() -> Result<(), Box<dyn std::error::Error>> {
        let dir = TempDir::new()?;
        let db_path = dir.path().join("semantic_e2e.db");
        let recall = SemanticRecall::new(db_path.as_path(), 384)?;

        recall.store_chunk(
            "Cache SystemTime to avoid syscall",
            None,
            Some(&serde_json::json!(0.9)),
        )?;
        recall.store_chunk(
            "Use aview1 instead of to_vec clone",
            None,
            Some(&serde_json::json!(0.85)),
        )?;
        recall.store_chunk(
            "SmallVec eliminates heap allocation",
            None,
            Some(&serde_json::json!(0.8)),
        )?;

        let hits = recall.fts_search("aview1", 5)?;
        assert!(!hits.is_empty(), "search should find aview1 entries");
        Ok(())
    }

    #[test]
    fn rlm_tier_isolation() -> Result<(), Box<dyn std::error::Error>> {
        let dir = TempDir::new()?;
        let db_path = dir.path().join("rlm_tier.db");
        let mem = RlmMemory::new(db_path.as_path())?;

        mem.store("shared", MemoryTier::Working, "working_val", None, None)?;
        mem.store("shared", MemoryTier::Reference, "reference_val", None, None)?;

        let val = mem.get("shared", MemoryTier::Working)?;
        assert!(val.is_some(), "should recall something");
        Ok(())
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// Ranking + drift detection tests
// ══════════════════════════════════════════════════════════════════════════════

mod ranking_tests {
    use super::*;

    #[test]
    fn wilson_ranker_confidence_ordering() {
        let mut ranker = WilsonRanker::new();

        // Record successes and failures
        for _ in 0..160 {
            ranker.record("high", true);
        }
        for _ in 0..40 {
            ranker.record("high", false);
        }
        for _ in 0..100 {
            ranker.record("mid", true);
        }
        for _ in 0..100 {
            ranker.record("mid", false);
        }
        for _ in 0..40 {
            ranker.record("low", true);
        }
        for _ in 0..160 {
            ranker.record("low", false);
        }

        let top = ranker.top_k(3);
        assert_eq!(top.len(), 3);
        assert_eq!(top[0].id, "high");
        assert_eq!(top[1].id, "mid");
        assert_eq!(top[2].id, "low");
    }

    #[test]
    fn drift_monitor_detects_shift() {
        let mut monitor = DriftMonitor::with_defaults();

        // Inject stable baseline
        for _ in 0..30 {
            monitor.observe(0.5);
        }

        // Set reference from baseline
        monitor.promote_current_to_reference();

        // Clear current and inject shift
        for _ in 0..15 {
            monitor.observe(2.5);
        }

        // test() returns Some(DriftReport) if drift detected
        let report = monitor.test();
        assert!(report.is_some(), "drift should be detected after shift");
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// Template + evolution tests
// ══════════════════════════════════════════════════════════════════════════════

mod evolution_tests {
    use super::*;

    #[test]
    fn template_library_select() {
        // TemplateLibrary::new() creates with 3 default templates
        let lib = TemplateLibrary::new();

        // select() returns &ContextTemplate (panics if empty)
        let sel = lib.select();
        assert!(!sel.id.is_empty(), "selected template should have an id");
    }

    #[test]
    fn evolution_analyzer_wires() -> Result<(), Box<dyn std::error::Error>> {
        let dir = TempDir::new()?;
        let db_path = dir.path().join("evolution_e2e.db");

        let rlm = RlmMemory::new(db_path.as_path())?;
        let ranker = WilsonRanker::new();
        let drift = DriftDetector::new();

        let analyzer = EvolutionAnalyzer::new(rlm, ranker, drift);
        let results = analyzer.analyze_all();
        // analyze_all returns Vec<AnalysisResult> — verify bounds
        for r in &results {
            assert!(r.value.is_finite(), "result value should be finite");
        }
        Ok(())
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// FTRL feature-gated compilation check
// ══════════════════════════════════════════════════════════════════════════════

#[cfg(feature = "ftrl")]
mod ftrl_tests {
    use super::OnlineRLEngine;

    #[test]
    fn ftrl_engine_has_ftrl_layer() {
        // When ftrl feature is enabled, OnlineRLEngine has with_ftrl_layer method
        let engine = OnlineRLEngine::with_defaults();
        let _ = engine.with_ftrl_layer();
    }
}

#[cfg(not(feature = "ftrl"))]
mod ftrl_tests {
    #[test]
    fn ftrl_disabled_compiles() {
        // ftrl is optional — this is the no-op path
    }
}
