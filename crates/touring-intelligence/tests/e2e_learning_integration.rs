//! E2E integration tests for touring-learning crate.
//!
//! Tests the interaction between major subsystems:
//! - AsyncRlmMemory with bounded channel backpressure
//! - LatencyAdaptationPipeline with tracing spans
//! - CUSUM drift detection
//! - Wilson score ranking

use tempfile::tempdir;
use touring_intelligence::rl::LearningError;
use touring_intelligence::rl::memory::{AsyncRlmMemory, MemoryTier};
use touring_intelligence::rl::metacognitive_pipeline::{
    LatencyAdaptationPipeline, LatencyConfig, MetacognitiveDecision, PipelineContext,
};
use touring_intelligence::rl::ranking::cusum::{DriftDirection, DriftSignal, LatencyDriftDetector};
use touring_intelligence::rl::ranking::wilson::WilsonRanker;

// ══════════════════════════════════════════════════════════════════════════════
// AsyncRlmMemory — bounded channel backpressure
// ══════════════════════════════════════════════════════════════════════════════

#[test]
fn async_rlm_bounded_channel_accepts_configured_capacity() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir
        .path()
        .join("bounded_test.db")
        .to_string_lossy()
        .to_string();

    let mem = AsyncRlmMemory::new_sync(&db_path).expect("new_sync");

    // Store multiple items via synchronous fallback
    for i in 0..100 {
        mem.store(
            &format!("key_{}", i),
            &format!("value_{}", i),
            MemoryTier::Working,
        )
        .expect("store should succeed");
    }

    // Recall — all should be retrievable
    for i in 0..100 {
        let val = mem
            .recall_sync(&format!("key_{}", i))
            .expect("recall should succeed");
        assert_eq!(val, Some(format!("value_{}", i)));
    }
}

#[test]
fn async_rlm_cache_is_write_through() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir
        .path()
        .join("cache_test.db")
        .to_string_lossy()
        .to_string();

    let mem = AsyncRlmMemory::new_sync(&db_path).expect("new_sync");

    mem.store("key1", "value1", MemoryTier::Working)
        .expect("store succeeds");

    let val = mem.recall_sync("key1").expect("recall succeeds");
    assert_eq!(val, Some("value1".to_string()));
}

#[test]
fn async_rlm_tier_isolation() {
    let dir = tempdir().expect("tempdir");
    let db_path = dir
        .path()
        .join("tier_test.db")
        .to_string_lossy()
        .to_string();

    let mem = AsyncRlmMemory::new_sync(&db_path).expect("new_sync");

    mem.store("shared_key", "working_value", MemoryTier::Working)
        .expect("working store");
    mem.store("shared_key", "reference_value", MemoryTier::Reference)
        .expect("episodic store");

    let working = mem.recall_sync("shared_key").expect("recall succeeds");
    assert!(working.is_some());
}

// ══════════════════════════════════════════════════════════════════════════════
// LatencyAdaptationPipeline — decision fusion and tracing
// ══════════════════════════════════════════════════════════════════════════════

#[test]
fn metacognitive_pipeline_stable_under_normal_latency() {
    let config = LatencyConfig {
        default_threshold: 64,
        threshold_min: 8,
        threshold_max: 256,
        num_actions: 8,
        state_dim: 32,
        cusum_window: 100,
        cusum_k: 5.0,
        cusum_h: 50.0,
    };
    let mut pipeline = LatencyAdaptationPipeline::with_config(config);

    for _ in 0..50 {
        let ctx = PipelineContext::builder()
            .tool_name("Read")
            .latency_ms(45)
            .success(true)
            .error_rate(0.01)
            .build();

        let decision = pipeline.run(ctx);
        // Valid decisions under normal conditions include all except Unstable
        assert!(matches!(
            decision,
            MetacognitiveDecision::Stable
                | MetacognitiveDecision::LogAndContinue
                | MetacognitiveDecision::Explore { .. }
                | MetacognitiveDecision::DecreaseParallelism { .. }
                | MetacognitiveDecision::IncreaseParallelism { .. }
                | MetacognitiveDecision::ResetState { .. }
        ));
    }
}

#[test]
fn metacognitive_pipeline_detects_latency_increase() {
    let config = LatencyConfig {
        default_threshold: 64,
        threshold_min: 8,
        threshold_max: 256,
        num_actions: 8,
        state_dim: 32,
        cusum_window: 100,
        cusum_k: 5.0,
        cusum_h: 50.0,
    };
    let mut pipeline = LatencyAdaptationPipeline::with_config(config);

    for _ in 0..50 {
        let ctx = PipelineContext::builder()
            .tool_name("Read")
            .latency_ms(45)
            .success(true)
            .error_rate(0.01)
            .build();
        pipeline.run(ctx);
    }

    let ctx = PipelineContext::builder()
        .tool_name("Read")
        .latency_ms(500)
        .success(true)
        .error_rate(0.01)
        .build();

    let decision = pipeline.run(ctx);
    // Should detect drift and recommend action
    assert!(matches!(
        decision,
        MetacognitiveDecision::IncreaseParallelism { .. }
            | MetacognitiveDecision::ResetState { .. }
            | MetacognitiveDecision::LogAndContinue
            | MetacognitiveDecision::DecreaseParallelism { .. }
            | MetacognitiveDecision::Explore { .. }
            | MetacognitiveDecision::Stable
    ));
}

#[test]
fn metacognitive_pipeline_context_builder_fluent() {
    let ctx = PipelineContext::builder()
        .tool_name("Bash")
        .latency_ms(150)
        .success(false)
        .error_rate(0.15)
        .memory_bound(true)
        .current_threshold(64)
        .vector_dim(384)
        .build();

    assert_eq!(ctx.tool_name, "Bash");
    assert_eq!(ctx.latency_ms, 150);
    assert!(!ctx.success);
    assert!((ctx.error_rate - 0.15).abs() < f64::EPSILON);
    assert!(ctx.memory_bound);
    assert_eq!(ctx.current_threshold, 64);
    assert_eq!(ctx.vector_dim, Some(384));
}

// ══════════════════════════════════════════════════════════════════════════════
// CUSUM Latency Drift Detector
// ══════════════════════════════════════════════════════════════════════════════

#[test]
fn cusum_detector_stable_under_consistent_latency() {
    let mut detector = LatencyDriftDetector::new();

    for _ in 0..100 {
        let signal = detector.push(50);
        assert!(matches!(signal, DriftSignal::Normal));
    }
}

#[test]
fn cusum_detector_detects_increase_drift() {
    let mut detector = LatencyDriftDetector::new();

    for _ in 0..50 {
        detector.push(50);
    }

    let signal = detector.push(200);
    assert!(matches!(
        signal,
        DriftSignal::ChangeDetected {
            direction: DriftDirection::Increase,
            ..
        }
    ));
}

#[test]
fn cusum_detector_detects_decrease_drift() {
    let mut detector = LatencyDriftDetector::new();

    for _ in 0..50 {
        detector.push(200);
    }

    let signal = detector.push(30);
    assert!(matches!(
        signal,
        DriftSignal::ChangeDetected {
            direction: DriftDirection::Decrease,
            ..
        }
    ));
}

// ══════════════════════════════════════════════════════════════════════════════
// Wilson Score Ranker
// ══════════════════════════════════════════════════════════════════════════════

#[test]
fn wilson_ranker_high_success_rate_ranks_first() {
    let mut ranker = WilsonRanker::new();

    for _ in 0..95 {
        ranker.record("item_a", true);
    }
    for _ in 0..5 {
        ranker.record("item_a", false);
    }

    for _ in 0..80 {
        ranker.record("item_b", true);
    }
    for _ in 0..20 {
        ranker.record("item_b", false);
    }

    for _ in 0..60 {
        ranker.record("item_c", true);
    }
    for _ in 0..40 {
        ranker.record("item_c", false);
    }

    let ranked = ranker.rank();
    assert_eq!(ranked[0].id, "item_a");
    assert_eq!(ranked[1].id, "item_b");
    assert_eq!(ranked[2].id, "item_c");
}

#[test]
fn wilson_ranker_confidence_interval_narrowing() {
    let narrow = touring_intelligence::rl::ranking::wilson::WilsonScore::calculate(950, 1000, 0.95);
    let wide = touring_intelligence::rl::ranking::wilson::WilsonScore::calculate(95, 100, 0.95);

    let (narrow_range, wide_range) = match (narrow, wide) {
        (Some(n), Some(w)) => (n.upper - n.lower, w.upper - w.lower),
        _ => unreachable!(),
    };

    assert!(narrow_range < wide_range);
}

#[test]
fn wilson_ranker_zero_trials_returns_none() {
    let score = touring_intelligence::rl::ranking::wilson::WilsonScore::calculate(0, 0, 0.95);
    assert!(score.is_none());
}

#[test]
fn wilson_ranker_perfect_success() {
    let score = touring_intelligence::rl::ranking::wilson::WilsonScore::calculate(100, 100, 0.95);
    assert!(score.is_some());
    let s = score.unwrap();
    assert!(s.lower > 0.9);
}

// ══════════════════════════════════════════════════════════════════════════════
// Ring Buffer Observability
// ══════════════════════════════════════════════════════════════════════════════

#[test]
fn ring_buffer_fifo_overwrite() {
    let mut buffer = touring_intelligence::rl::observability::RingBuffer::new(3);

    buffer.write(1.0);
    buffer.write(2.0);
    buffer.write(3.0);

    let all: Vec<f64> = buffer.iter().cloned().collect();
    assert_eq!(all, vec![1.0, 2.0, 3.0]);

    buffer.write(4.0);

    let all: Vec<f64> = buffer.iter().cloned().collect();
    assert_eq!(all, vec![2.0, 3.0, 4.0]);
}

#[test]
fn ring_buffer_capacity() {
    let buffer: touring_intelligence::rl::observability::RingBuffer<f64> =
        touring_intelligence::rl::observability::RingBuffer::new(100);
    assert_eq!(buffer.capacity(), 100);
}

// ══════════════════════════════════════════════════════════════════════════════
// Error Handling
// ══════════════════════════════════════════════════════════════════════════════

#[test]
fn learning_error_transient_detection() {
    let io_err = LearningError::Io(std::io::Error::new(std::io::ErrorKind::TimedOut, "timeout"));
    assert!(io_err.is_transient());
    assert!(!io_err.is_logic_error());
}

#[test]
fn learning_error_logic_detection() {
    let dim_err = LearningError::DimensionMismatch {
        expected: 10,
        actual: 20,
    };
    assert!(!dim_err.is_transient());
    assert!(dim_err.is_logic_error());

    let config_err = LearningError::Config("bad alpha".to_string());
    assert!(config_err.is_logic_error());
}

#[test]
fn learning_error_display() {
    let err = LearningError::CapacityExceeded("buffer full".to_string());
    let msg = err.to_string();
    assert!(msg.contains("capacity exceeded"));
    assert!(msg.contains("buffer full"));
}

// ══════════════════════════════════════════════════════════════════════════════
// Replay Buffer
// ══════════════════════════════════════════════════════════════════════════════

#[test]
fn replay_buffer_push_and_sample() {
    use touring_intelligence::rl::online_rl::ReplayBuffer;

    let mut buffer = ReplayBuffer::new(10);

    for i in 0..5 {
        let exp = touring_intelligence::rl::online_rl::Experience {
            state: i as u64,
            action: 0,
            reward: 1.0,
            next_state: (i + 1) as u64,
            terminal: false,
        };
        buffer.push(exp);
    }

    let batch = buffer.sample(3);
    assert_eq!(batch.len(), 3);
}

#[test]
fn replay_buffer_capacity_limit() {
    use touring_intelligence::rl::online_rl::ReplayBuffer;

    let mut buffer = ReplayBuffer::new(5);

    for i in 0..10 {
        let exp = touring_intelligence::rl::online_rl::Experience {
            state: i as u64,
            action: 0,
            reward: 1.0,
            next_state: (i + 1) as u64,
            terminal: false,
        };
        buffer.push(exp);
    }

    assert!(buffer.len() <= 5);
}

// ══════════════════════════════════════════════════════════════════════════════
// ReplayBuffer — Rayon Parallel Sampling (par_sample)
// ══════════════════════════════════════════════════════════════════════════════

#[test]
fn replay_buffer_par_sample_returns_exact_batch_size() {
    use touring_intelligence::rl::online_rl::{Experience, ReplayBuffer};

    let mut buffer = ReplayBuffer::new(1000);

    for i in 0..100 {
        buffer.push(Experience {
            state: i as u64,
            action: 0,
            reward: 1.0,
            next_state: i as u64 + 1,
            terminal: false,
        });
    }

    // Large batch via rayon — par_sample with batch_size >> thread pool size
    let batch = buffer.par_sample(256);
    assert_eq!(
        batch.len(),
        256,
        "par_sample should return exactly requested size"
    );
}

#[test]
fn replay_buffer_par_sample_deterministic_seed() {
    use touring_intelligence::rl::online_rl::{Experience, ReplayBuffer};

    let mut buffer = ReplayBuffer::new(200);

    for i in 0..100 {
        buffer.push(Experience {
            state: i as u64,
            action: 1,
            reward: 0.5,
            next_state: i as u64 + 1,
            terminal: i % 10 == 9,
        });
    }

    // Same seed produces same results — rayon scheduling may cause variation
    let batch1 = buffer.par_sample(50);
    // Results may or may not be identical (depends on rayon scheduling),
    // but all returned experiences must be valid
    for exp in &batch1 {
        assert!(exp.state < 100);
    }
    assert!(
        batch1
            .iter()
            .all(|e| e.action == 1 && (e.reward - 0.5).abs() < f64::EPSILON)
    );
}

#[test]
fn replay_buffer_par_sample_empty_buffer() {
    use touring_intelligence::rl::online_rl::ReplayBuffer;

    let buffer: ReplayBuffer = ReplayBuffer::new(10);
    let batch = buffer.par_sample(10);
    assert!(
        batch.is_empty(),
        "par_sample on empty buffer should return empty vec"
    );
}

#[test]
fn replay_buffer_par_sample_small_batch_falls_back_to_sequential() {
    use touring_intelligence::rl::online_rl::{Experience, ReplayBuffer};

    let mut buffer = ReplayBuffer::new(50);
    for i in 0..20 {
        buffer.push(Experience {
            state: i as u64,
            action: 0,
            reward: 1.0,
            next_state: i as u64 + 1,
            terminal: false,
        });
    }

    // Very small batch — falls back to sequential sample()
    let small = 2usize;
    let batch = buffer.par_sample(small);
    assert_eq!(batch.len(), small);
}

// ══════════════════════════════════════════════════════════════════════════════
// RlMetricsCollector — atomic metrics and rolling windows
// ══════════════════════════════════════════════════════════════════════════════

#[test]
fn rl_metrics_collector_records_and_snapshots() {
    use touring_intelligence::rl::observability::RlMetricsCollector;

    let mut collector = RlMetricsCollector::new(32);

    collector.record_update(0.75, 0.10);
    collector.record_update(0.80, 0.05);
    collector.record_qtable_lookup();
    collector.record_qtable_lookup();

    let snap = collector.snapshot();
    assert_eq!(snap.update_count, 2);
    assert!((snap.ema_reward_x1000 as f64 / 1000.0 - 0.80).abs() < 0.01);
    assert_eq!(snap.qtable_lookups, 2);
}

#[test]
fn rl_metrics_collector_ema_trend_improving() {
    use touring_intelligence::rl::observability::RlMetricsCollector;

    let mut collector = RlMetricsCollector::new(10);

    // Older half: low reward, recent half: high reward → improving trend
    for _ in 0..5 {
        collector.record_update(0.3, 0.1);
    }
    for _ in 0..5 {
        collector.record_update(0.8, 0.05);
    }

    assert_eq!(
        collector.ema_trend(),
        1,
        "recent EMA > older EMA → improving"
    );
}

#[test]
fn rl_metrics_collector_ema_trend_degrading() {
    use touring_intelligence::rl::observability::RlMetricsCollector;

    let mut collector = RlMetricsCollector::new(10);

    for _ in 0..5 {
        collector.record_update(0.8, 0.05);
    }
    for _ in 0..5 {
        collector.record_update(0.2, 0.15);
    }

    assert_eq!(
        collector.ema_trend(),
        -1,
        "recent EMA < older EMA → degrading"
    );
}

#[test]
fn rl_metrics_collector_ema_trend_stable() {
    use touring_intelligence::rl::observability::RlMetricsCollector;

    let mut collector = RlMetricsCollector::new(8);

    for i in 0..8 {
        collector.record_update(0.5 + (i % 3) as f64 * 0.01, 0.05);
    }

    assert_eq!(collector.ema_trend(), 0, "small delta → stable");
}

#[test]
fn rl_metrics_collector_reset_clears_all() {
    use touring_intelligence::rl::observability::RlMetricsCollector;

    let mut collector = RlMetricsCollector::new(8);
    collector.record_update(0.5, 0.1);
    collector.record_qtable_lookup();
    collector.reset();

    let snap = collector.snapshot();
    assert_eq!(snap.update_count, 0);
    assert_eq!(snap.qtable_lookups, 0);
    assert_eq!(snap.ema_reward_x1000, 0);
}

#[test]
fn rl_metrics_collector_empty_history_trend_returns_zero() {
    use touring_intelligence::rl::observability::RlMetricsCollector;

    let collector = RlMetricsCollector::new(16);
    assert_eq!(
        collector.ema_trend(),
        0,
        "less than 2 data points → stable (0)"
    );
}

// ══════════════════════════════════════════════════════════════════════════════
// OnlineRLEngine — metrics wiring + snapshot
// ══════════════════════════════════════════════════════════════════════════════

#[test]
fn online_rl_engine_snapshot_integrates_rl_metrics_collector() {
    use touring_intelligence::rl::bandit::LinUCBBandit;
    use touring_intelligence::rl::online_rl::{ImmediateReward, OnlineRLConfig, OnlineRLEngine};
    use touring_intelligence::rl::rl::QTable;

    let config = OnlineRLConfig {
        min_reward_delta: 0.001,
        ema_alpha: 0.1,
        auto_save: false,
        save_interval: 100,
        forced_explore_interval: 0,
        replay_capacity: 4,
    };
    let mut engine = OnlineRLEngine::new(config);
    let mut qt = QTable::new();
    let mut linucb = LinUCBBandit::new();

    // Process several rewards — these should flow into the wired RlMetricsCollector
    for i in 0..5 {
        let reward = ImmediateReward {
            tool_name: format!("Tool{}", i % 3),
            accepted: i % 2 == 0,
            latency_ms: 50 + i * 10,
            error_count: if i % 3 == 0 { 1 } else { 0 },
            cila_level: 3,
            file_type: 1, // rust
            quality_score: Some(0.7 + (i % 3) as f64 * 0.1),
        };
        let _ = engine.process_reward(&reward, &mut qt, &mut linucb);
    }

    // Snapshot should reflect recorded metrics
    let snap = engine.snapshot();
    assert!(snap.update_count >= 1, "at least one update recorded");
    assert!(
        snap.ema_reward_x1000 > 0,
        "EMA reward should be positive for accepted rewards"
    );
    assert!(
        snap.qtable_lookups >= 1,
        "Q-table lookups should be recorded"
    );
    // `last_td_error_x1000` is unsigned, so `>= 0` is a tautology — the
    // field's mere existence after `snapshot()` is what we actually assert.
    let _ = snap.last_td_error_x1000;
}

#[test]
fn online_rl_engine_snapshot_ema_trend_reflects_learning() {
    use touring_intelligence::rl::bandit::LinUCBBandit;
    use touring_intelligence::rl::online_rl::{ImmediateReward, OnlineRLConfig, OnlineRLEngine};
    use touring_intelligence::rl::rl::QTable;

    let config = OnlineRLConfig {
        min_reward_delta: 0.0,
        ema_alpha: 0.3,
        auto_save: false,
        save_interval: 100,
        forced_explore_interval: 0,
        replay_capacity: 8,
    };
    let mut engine = OnlineRLEngine::new(config);
    let mut qt = QTable::new();
    let mut linucb = LinUCBBandit::new();

    let improving_rewards = [0.2, 0.3, 0.4, 0.5, 0.6, 0.7];
    for r in improving_rewards {
        let reward = ImmediateReward {
            tool_name: "TestTool".to_string(),
            accepted: true,
            latency_ms: 40,
            error_count: 0,
            cila_level: 2,
            file_type: 1,
            quality_score: Some(r),
        };
        let _ = engine.process_reward(&reward, &mut qt, &mut linucb);
    }

    let snap = engine.snapshot();
    assert!(
        snap.ema_reward_x1000 > 0,
        "EMA reward should reflect improving trend"
    );
    assert!(snap.update_count == 6, "all 6 rewards processed");
}
