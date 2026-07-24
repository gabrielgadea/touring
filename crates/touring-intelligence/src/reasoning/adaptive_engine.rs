//! S14: Adaptive reasoning engine with bandit-based strategy selection.
//!
//! Uses a contextual bandit (UCB1-style) to select between MCTS, GoT, and
//! Hybrid reasoning engines based on the CILA complexity level and historical
//! success rates. The bandit learns which engine performs best for which
//! context, allocating compute adaptively.
//!
//! - **L0-L1** (Direct/PAL): No search needed, skip entirely
//! - **L2-L3** (Tool-Augmented/Pipelines): MCTS preferred (structured search)
//! - **L4+** (Agent Loops/Multi-Agent): GoT/Hybrid preferred (deep reasoning)
//!
//! The bandit adapts these defaults over time based on actual outcome quality.

use std::collections::HashMap;
use std::sync::RwLock;

use crate::reasoning::reasoning_engine::{ReasoningEngine, ReasoningQuery, ReasoningResult};

/// Default RSS threshold (in MB) above which L4+ reasoning searches are
/// skipped. Tuned conservatively for a dev machine with ≥ 8 GB RAM; daemons
/// embedded in constrained environments should pass a lower bound.
///
/// Added in Wave C3 (2026-04-20) together with
/// [`AdaptiveEngine::search_with_memory_pressure`].
pub const DEFAULT_MEMORY_PRESSURE_THRESHOLD_MB: f64 = 2048.0;

/// Statistics for a reasoning arm (MCTS, GoT, Hybrid, etc.).
///
/// Stored per arm index so the cognitive engine can correlate arm
/// effectiveness with RL signals via `resolve_reasoning`.
#[derive(Debug, Clone)]
pub struct ArmStats {
    /// Number of times this arm has been selected.
    pub pulls: u32,
    /// Running average reward for this arm.
    pub avg_reward: f64,
    /// Most recent reward observed for this arm.
    pub last_reward: f64,
}

impl ArmStats {
    fn new() -> Self {
        Self {
            pulls: 0,
            avg_reward: 0.5,
            last_reward: 0.0,
        }
    }
}

/// UCB1 exploration constant for engine selection.
const UCB_EXPLORATION: f64 = 1.414;

/// Minimum number of trials before an engine's average is trusted.
const MIN_TRIALS: u64 = 3;

/// Per-engine bandit statistics.
#[derive(Debug, Clone)]
struct EngineStats {
    /// Total reward accumulated.
    total_reward: f64,
    /// Number of times this engine was selected.
    trials: u64,
}

impl EngineStats {
    fn new() -> Self {
        Self {
            total_reward: 0.0,
            trials: 0,
        }
    }

    fn avg_reward(&self) -> f64 {
        if self.trials == 0 {
            return 0.5; // optimistic default
        }
        self.total_reward / self.trials as f64
    }

    fn ucb1(&self, total_trials: u64) -> f64 {
        if self.trials < MIN_TRIALS {
            return f64::INFINITY; // always try under-explored engines
        }
        let avg = self.avg_reward();
        let exploration =
            UCB_EXPLORATION * ((total_trials as f64).ln() / self.trials as f64).sqrt();
        avg + exploration
    }
}

/// Bandit state per CILA level.
#[derive(Debug)]
struct BanditState {
    /// Stats per engine name, per CILA level.
    stats: HashMap<(u8, String), EngineStats>,
    /// Total selections across all engines.
    total_trials: u64,
}

impl BanditState {
    fn new() -> Self {
        Self {
            stats: HashMap::new(),
            total_trials: 0,
        }
    }

    /// Select the best engine name for the given CILA level using UCB1.
    fn select(&self, cila_level: u8, engine_names: &[String]) -> String {
        let mut best_name = engine_names.first().cloned().unwrap_or_default();
        let mut best_ucb = f64::NEG_INFINITY;

        for name in engine_names {
            let key = (cila_level, name.clone());
            let stats = self
                .stats
                .get(&key)
                .cloned()
                .unwrap_or_else(EngineStats::new);
            let ucb = stats.ucb1(self.total_trials);
            if ucb > best_ucb {
                best_ucb = ucb;
                best_name = name.clone();
            }
        }

        best_name
    }

    /// Record a reward for an engine at a CILA level.
    fn record(&mut self, cila_level: u8, engine_name: &str, reward: f64) {
        let key = (cila_level, engine_name.to_string());
        let stats = self.stats.entry(key).or_insert_with(EngineStats::new);
        stats.total_reward += reward;
        stats.trials += 1;
        self.total_trials += 1;
    }
}

/// S14: Adaptive reasoning engine that selects between strategies via bandit.
///
/// Wraps multiple `ReasoningEngine` implementations and uses UCB1 to pick
/// the best one for each query based on CILA level and historical performance.
pub struct AdaptiveEngine {
    /// Available reasoning engines.
    engines: Vec<Box<dyn ReasoningEngine>>,
    /// Bandit state for adaptive selection.
    bandit: RwLock<BanditState>,
    /// Per-arm statistics for RL signal correlation.
    arm_stats: RwLock<HashMap<usize, ArmStats>>,
}

impl AdaptiveEngine {
    /// Create with the given set of engines.
    pub fn new(engines: Vec<Box<dyn ReasoningEngine>>) -> Self {
        Self {
            engines,
            bandit: RwLock::new(BanditState::new()),
            arm_stats: RwLock::new(HashMap::new()),
        }
    }

    /// Create with default engines (MCTS + Hybrid).
    pub fn with_defaults() -> Self {
        Self::new(vec![
            Box::new(crate::reasoning::reasoning_engine::MCTSReasoningEngine::new()),
            Box::new(crate::reasoning::reasoning_engine::HybridReasoningEngine::new()),
        ])
    }

    /// Search variant with memory-pressure awareness (Wave C3, 2026-04-20).
    ///
    /// If `current_memory_mb` exceeds `threshold_mb` *and* the query is
    /// CILA L4+ (heavyweight reasoning), skips the expensive engine entirely
    /// and returns `None` — letting the caller fall back to a lighter path
    /// (cached heuristics, direct tool dispatch, etc.). Under normal memory
    /// conditions delegates to [`Self::search`] unchanged.
    ///
    /// The threshold is caller-supplied (not stored as engine state) to keep
    /// `AdaptiveEngine` free of configuration knobs and to let the daemon
    /// feed in live RSS samples from `shared::memory_stats_probe`.
    ///
    /// Use [`DEFAULT_MEMORY_PRESSURE_THRESHOLD_MB`] as a sane default when
    /// the operator has not supplied one.
    pub fn search_with_memory_pressure(
        &self,
        query: &ReasoningQuery,
        current_memory_mb: f64,
        threshold_mb: f64,
    ) -> Option<ReasoningResult> {
        if current_memory_mb > threshold_mb && query.cila_level >= 4 {
            return None;
        }
        self.search(query)
    }

    /// Select and execute the best engine for the query.
    ///
    /// Uses UCB1 bandit to pick the engine, runs it, and records the result.
    /// For L0-L1, returns None (no search needed for trivial queries).
    pub fn search(&self, query: &ReasoningQuery) -> Option<ReasoningResult> {
        // L0-L1: skip search entirely for trivial queries
        if query.cila_level <= 1 {
            return None;
        }

        if self.engines.is_empty() {
            return None;
        }

        let engine_names: Vec<String> = self.engines.iter().map(|e| e.name().to_string()).collect();

        // Select engine via bandit
        let selected_name = {
            let bandit = self.bandit.read().unwrap_or_else(|e| e.into_inner());
            bandit.select(query.cila_level, &engine_names)
        };

        // Find and run the selected engine
        let engine = self.engines.iter().find(|e| e.name() == selected_name)?;
        let result = engine.search(query);

        // Record reward (confidence as proxy for quality)
        let reward = result.as_ref().map(|r| r.confidence).unwrap_or(0.0);
        if let Ok(mut bandit) = self.bandit.write() {
            bandit.record(query.cila_level, &selected_name, reward);
        }

        result
    }

    /// Record an external reward signal for the last engine selection.
    ///
    /// Call this after observing the actual outcome quality (e.g., tool success).
    pub fn record_outcome(&self, cila_level: u8, engine_name: &str, reward: f64) {
        if let Ok(mut bandit) = self.bandit.write() {
            bandit.record(cila_level, engine_name, reward);
        }
    }

    /// Record an outcome with arm statistics for RL signal correlation.
    ///
    /// Stores `ArmStats` per arm index so `resolve_reasoning` can correlate
    /// which arm (MCTS vs GoT vs Hybrid) produces the best outcomes.
    pub fn record_outcome_with_stats(&self, arm: usize, _reward: f64, stats: ArmStats) {
        if let Ok(mut arm_stats) = self.arm_stats.write() {
            let entry = arm_stats.entry(arm).or_insert_with(ArmStats::new);
            entry.pulls = stats.pulls;
            entry.avg_reward = stats.avg_reward;
            entry.last_reward = stats.last_reward;
        }
    }

    /// Retrieve arm statistics for a given arm index.
    pub fn get_arm_stats(&self, arm: usize) -> Option<ArmStats> {
        self.arm_stats
            .read()
            .ok()
            .and_then(|stats| stats.get(&arm).cloned())
    }

    /// Get all arm statistics as a snapshot.
    pub fn all_arm_stats(&self) -> HashMap<usize, ArmStats> {
        self.arm_stats
            .read()
            .map(|stats| stats.clone())
            .unwrap_or_default()
    }

    /// Get bandit statistics as a snapshot.
    pub fn stats(&self) -> HashMap<String, (f64, u64)> {
        let bandit = self.bandit.read().unwrap_or_else(|e| e.into_inner());
        bandit
            .stats
            .iter()
            .map(|((level, name), stats)| {
                (
                    format!("L{}:{}", level, name),
                    (stats.avg_reward(), stats.trials),
                )
            })
            .collect()
    }

    /// Number of registered engines.
    pub fn engine_count(&self) -> usize {
        self.engines.len()
    }

    /// Total bandit trials across all engines.
    pub fn total_trials(&self) -> u64 {
        self.bandit.read().map(|b| b.total_trials).unwrap_or(0)
    }
}

impl std::fmt::Debug for AdaptiveEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AdaptiveEngine")
            .field("engine_count", &self.engines.len())
            .field("total_trials", &self.total_trials())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reasoning::reasoning_engine::ReasoningQuery;

    #[test]
    fn test_adaptive_engine_with_defaults() {
        let engine = AdaptiveEngine::with_defaults();
        assert_eq!(engine.engine_count(), 2);
    }

    #[test]
    fn test_skip_trivial_queries() {
        let engine = AdaptiveEngine::with_defaults();
        let query = ReasoningQuery::new(1, "simple")
            .with_actions(vec![1, 2])
            .with_cila_level(0);
        assert!(engine.search(&query).is_none(), "L0 should skip search");

        let query = ReasoningQuery::new(1, "simple")
            .with_actions(vec![1, 2])
            .with_cila_level(1);
        assert!(engine.search(&query).is_none(), "L1 should skip search");
    }

    #[test]
    fn test_search_l2_plus() {
        let engine = AdaptiveEngine::with_defaults();
        let query = ReasoningQuery::new(1, "plan action")
            .with_actions(vec![10, 20, 30])
            .with_cila_level(3);
        let result = engine.search(&query);
        assert!(result.is_some(), "L3 query should produce a result");
    }

    #[test]
    fn test_memory_pressure_skips_l4_plus_queries() {
        // Wave C3 (2026-04-20): under RSS pressure, L4+ queries must short-circuit.
        let engine = AdaptiveEngine::with_defaults();
        let query = ReasoningQuery::new(1, "deep reasoning")
            .with_actions(vec![1, 2, 3])
            .with_cila_level(4);

        // Memory well above threshold → skip.
        let result = engine.search_with_memory_pressure(&query, 4096.0, 2048.0);
        assert!(
            result.is_none(),
            "L4 under memory pressure must return None"
        );
    }

    #[test]
    fn test_memory_pressure_allows_lower_cila_levels() {
        // Under the same pressure, L3 queries still proceed.
        let engine = AdaptiveEngine::with_defaults();
        let query = ReasoningQuery::new(1, "plan")
            .with_actions(vec![1, 2, 3])
            .with_cila_level(3);

        let result = engine.search_with_memory_pressure(&query, 4096.0, 2048.0);
        assert!(result.is_some(), "L3 must not be pruned by memory pressure");
    }

    #[test]
    fn test_memory_pressure_below_threshold_passes_through() {
        // No pressure → delegates to plain search even for L5.
        let engine = AdaptiveEngine::with_defaults();
        let query = ReasoningQuery::new(1, "complex")
            .with_actions(vec![1, 2, 3])
            .with_cila_level(5);

        let result = engine.search_with_memory_pressure(&query, 500.0, 2048.0);
        assert!(
            result.is_some(),
            "L5 under normal memory must delegate to search()"
        );
    }

    #[test]
    fn test_default_memory_pressure_threshold_is_sensible() {
        // Guard against accidental drift of the public constant.
        assert!(
            DEFAULT_MEMORY_PRESSURE_THRESHOLD_MB >= 1024.0,
            "threshold must accommodate a typical dev daemon"
        );
        assert!(
            DEFAULT_MEMORY_PRESSURE_THRESHOLD_MB <= 8192.0,
            "threshold must still guard against runaway growth"
        );
    }

    #[test]
    fn test_bandit_learns_over_time() {
        let engine = AdaptiveEngine::with_defaults();
        let query = ReasoningQuery::new(1, "plan")
            .with_actions(vec![10, 20])
            .with_cila_level(3);

        // Run multiple searches to build up bandit stats
        for _ in 0..20 {
            let _ = engine.search(&query);
        }

        assert!(engine.total_trials() >= 20);
        let stats = engine.stats();
        assert!(!stats.is_empty(), "should have stats after searches");
    }

    #[test]
    fn test_record_external_outcome() {
        let engine = AdaptiveEngine::with_defaults();
        engine.record_outcome(3, "MCTS", 0.9);
        engine.record_outcome(3, "MCTS", 0.8);
        engine.record_outcome(3, "Hybrid", 0.3);

        let stats = engine.stats();
        let mcts_stats = stats.get("L3:MCTS");
        assert!(mcts_stats.is_some());
        let (avg, trials) = mcts_stats.unwrap();
        assert_eq!(*trials, 2);
        assert!((*avg - 0.85).abs() < 0.01);
    }

    #[test]
    fn test_empty_engines() {
        let engine = AdaptiveEngine::new(vec![]);
        let query = ReasoningQuery::new(1, "test")
            .with_actions(vec![1])
            .with_cila_level(3);
        assert!(engine.search(&query).is_none());
    }

    #[test]
    fn test_engine_stats_new() {
        let stats = EngineStats::new();
        assert_eq!(stats.avg_reward(), 0.5); // optimistic default
        assert_eq!(stats.trials, 0);
    }

    #[test]
    fn test_ucb1_exploration() {
        let mut stats = EngineStats::new();
        stats.total_reward = 3.0;
        stats.trials = 10;
        let ucb = stats.ucb1(100);
        // avg = 0.3, exploration bonus > 0
        assert!(ucb > 0.3, "UCB1 should be > avg: {ucb}");
    }

    #[test]
    fn test_under_explored_gets_infinity() {
        let stats = EngineStats::new();
        let ucb = stats.ucb1(100);
        assert!(
            ucb.is_infinite(),
            "under-explored engine should get infinity"
        );
    }

    #[test]
    fn test_debug_impl() {
        let engine = AdaptiveEngine::with_defaults();
        let debug = format!("{:?}", engine);
        assert!(debug.contains("AdaptiveEngine"));
        assert!(debug.contains("engine_count: 2"));
    }
}
