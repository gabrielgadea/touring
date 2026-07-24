//! Unified cognitive metrics (S10).
//!
//! `CognitiveMetrics` provides lock-free atomic counters for all major
//! cognitive engine operations. Exposed as a global singleton via
//! `CognitiveMetrics::global()` — any module can increment without allocation.

use std::sync::atomic::{AtomicU64, Ordering};

/// Unified observability counters for the cognitive engine.
///
/// All counters use `Ordering::Relaxed` — they are advisory metrics, not
/// synchronization primitives. This gives maximum throughput with zero
/// contention across threads.
#[derive(Debug)]
pub struct CognitiveMetrics {
    /// Number of MCTS search invocations.
    pub mcts_searches: AtomicU64,
    /// Number of GoT explore invocations.
    pub got_explores: AtomicU64,
    /// Number of SemanticGraph retrieval operations.
    pub graph_retrievals: AtomicU64,
    /// Number of FocusCache hits.
    pub cache_hits: AtomicU64,
    /// Number of FocusCache misses.
    pub cache_misses: AtomicU64,
    /// Number of SessionPredictor queries.
    pub predictor_queries: AtomicU64,
    /// Number of refinement cycles executed.
    pub refinement_cycles: AtomicU64,
    /// Number of co-edit predictions.
    pub coedit_predictions: AtomicU64,
    /// Number of pheromone deposits (MCTS + GoT + ACO).
    pub pheromone_deposits: AtomicU64,
    /// Number of warm_cache calls.
    pub warm_cache_calls: AtomicU64,
    /// Analysis quality score from touring-analysis pipeline (scaled: score * 1000 as integer).
    /// Decode: score = value / 1000.0. Zero means not yet computed.
    pub analysis_quality_score: AtomicU64,
}

impl CognitiveMetrics {
    /// Create a new metrics instance with all counters at zero.
    pub const fn new() -> Self {
        Self {
            mcts_searches: AtomicU64::new(0),
            got_explores: AtomicU64::new(0),
            graph_retrievals: AtomicU64::new(0),
            cache_hits: AtomicU64::new(0),
            cache_misses: AtomicU64::new(0),
            predictor_queries: AtomicU64::new(0),
            refinement_cycles: AtomicU64::new(0),
            coedit_predictions: AtomicU64::new(0),
            pheromone_deposits: AtomicU64::new(0),
            warm_cache_calls: AtomicU64::new(0),
            analysis_quality_score: AtomicU64::new(0),
        }
    }

    /// Access the global metrics singleton.
    pub fn global() -> &'static Self {
        static INSTANCE: CognitiveMetrics = CognitiveMetrics::new();
        &INSTANCE
    }

    /// Increment a counter by 1.
    #[inline]
    pub fn inc(counter: &AtomicU64) {
        counter.fetch_add(1, Ordering::Relaxed);
    }

    /// Read a counter value.
    #[inline]
    pub fn get(counter: &AtomicU64) -> u64 {
        counter.load(Ordering::Relaxed)
    }

    /// Reset all counters to zero.
    pub fn reset(&self) {
        self.mcts_searches.store(0, Ordering::Relaxed);
        self.got_explores.store(0, Ordering::Relaxed);
        self.graph_retrievals.store(0, Ordering::Relaxed);
        self.cache_hits.store(0, Ordering::Relaxed);
        self.cache_misses.store(0, Ordering::Relaxed);
        self.predictor_queries.store(0, Ordering::Relaxed);
        self.refinement_cycles.store(0, Ordering::Relaxed);
        self.coedit_predictions.store(0, Ordering::Relaxed);
        self.pheromone_deposits.store(0, Ordering::Relaxed);
        self.warm_cache_calls.store(0, Ordering::Relaxed);
        self.analysis_quality_score.store(0, Ordering::Relaxed);
    }

    /// Return a snapshot of all counters as a serializable struct.
    pub fn snapshot(&self) -> MetricsSnapshot {
        MetricsSnapshot {
            mcts_searches: Self::get(&self.mcts_searches),
            got_explores: Self::get(&self.got_explores),
            graph_retrievals: Self::get(&self.graph_retrievals),
            cache_hits: Self::get(&self.cache_hits),
            cache_misses: Self::get(&self.cache_misses),
            predictor_queries: Self::get(&self.predictor_queries),
            refinement_cycles: Self::get(&self.refinement_cycles),
            coedit_predictions: Self::get(&self.coedit_predictions),
            pheromone_deposits: Self::get(&self.pheromone_deposits),
            warm_cache_calls: Self::get(&self.warm_cache_calls),
        }
    }

    /// Cache hit rate: hits / (hits + misses). Returns 0.0 if no cache activity.
    pub fn cache_hit_rate(&self) -> f64 {
        let hits = Self::get(&self.cache_hits);
        let misses = Self::get(&self.cache_misses);
        let total = hits + misses;
        if total == 0 {
            return 0.0;
        }
        hits as f64 / total as f64
    }

    /// Set the analysis quality score (0.0-1.0), stored as score * 1000.
    pub fn set_analysis_quality(&self, score: f64) {
        let stored = (score.clamp(0.0, 1.0) * 1000.0) as u64;
        self.analysis_quality_score.store(stored, Ordering::Relaxed);
    }

    /// Read the analysis quality score (0.0-1.0).
    pub fn analysis_quality(&self) -> f64 {
        self.analysis_quality_score.load(Ordering::Relaxed) as f64 / 1000.0
    }
}

impl Default for CognitiveMetrics {
    fn default() -> Self {
        Self::new()
    }
}

/// Serializable snapshot of all cognitive metrics.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MetricsSnapshot {
    /// Number of MCTS search invocations.
    pub mcts_searches: u64,
    /// Number of GoT explore invocations.
    pub got_explores: u64,
    /// Number of semantic-graph retrieval operations.
    pub graph_retrievals: u64,
    /// Number of focus-cache hits.
    pub cache_hits: u64,
    /// Number of focus-cache misses.
    pub cache_misses: u64,
    /// Number of session-predictor queries.
    pub predictor_queries: u64,
    /// Number of refinement cycles executed.
    pub refinement_cycles: u64,
    /// Number of co-edit predictions.
    pub coedit_predictions: u64,
    /// Number of pheromone deposits (MCTS + GoT + ACO).
    pub pheromone_deposits: u64,
    /// Number of warm-cache calls.
    pub warm_cache_calls: u64,
}

impl MetricsSnapshot {
    /// Total operations across all counter types.
    pub fn total_operations(&self) -> u64 {
        self.mcts_searches
            + self.got_explores
            + self.graph_retrievals
            + self.cache_hits
            + self.cache_misses
            + self.predictor_queries
            + self.refinement_cycles
            + self.coedit_predictions
            + self.pheromone_deposits
            + self.warm_cache_calls
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_new_all_zero() {
        let m = CognitiveMetrics::new();
        assert_eq!(CognitiveMetrics::get(&m.mcts_searches), 0);
        assert_eq!(CognitiveMetrics::get(&m.got_explores), 0);
        assert_eq!(CognitiveMetrics::get(&m.graph_retrievals), 0);
    }

    #[test]
    fn test_metrics_inc_and_get() {
        let m = CognitiveMetrics::new();
        CognitiveMetrics::inc(&m.mcts_searches);
        CognitiveMetrics::inc(&m.mcts_searches);
        CognitiveMetrics::inc(&m.got_explores);
        assert_eq!(CognitiveMetrics::get(&m.mcts_searches), 2);
        assert_eq!(CognitiveMetrics::get(&m.got_explores), 1);
    }

    #[test]
    fn test_metrics_reset() {
        let m = CognitiveMetrics::new();
        CognitiveMetrics::inc(&m.mcts_searches);
        CognitiveMetrics::inc(&m.cache_hits);
        m.reset();
        assert_eq!(CognitiveMetrics::get(&m.mcts_searches), 0);
        assert_eq!(CognitiveMetrics::get(&m.cache_hits), 0);
    }

    #[test]
    fn test_metrics_snapshot() {
        let m = CognitiveMetrics::new();
        CognitiveMetrics::inc(&m.mcts_searches);
        CognitiveMetrics::inc(&m.got_explores);
        CognitiveMetrics::inc(&m.got_explores);
        let snap = m.snapshot();
        assert_eq!(snap.mcts_searches, 1);
        assert_eq!(snap.got_explores, 2);
        assert_eq!(snap.graph_retrievals, 0);
    }

    #[test]
    fn test_snapshot_total_operations() {
        let m = CognitiveMetrics::new();
        CognitiveMetrics::inc(&m.mcts_searches);
        CognitiveMetrics::inc(&m.cache_hits);
        CognitiveMetrics::inc(&m.cache_misses);
        let snap = m.snapshot();
        assert_eq!(snap.total_operations(), 3);
    }

    #[test]
    fn test_cache_hit_rate_empty() {
        let m = CognitiveMetrics::new();
        assert_eq!(m.cache_hit_rate(), 0.0);
    }

    #[test]
    fn test_cache_hit_rate_calculated() {
        let m = CognitiveMetrics::new();
        for _ in 0..7 {
            CognitiveMetrics::inc(&m.cache_hits);
        }
        for _ in 0..3 {
            CognitiveMetrics::inc(&m.cache_misses);
        }
        assert!((m.cache_hit_rate() - 0.7).abs() < 1e-10);
    }

    #[test]
    fn test_global_singleton() {
        let g1 = CognitiveMetrics::global();
        let g2 = CognitiveMetrics::global();
        // Both should point to the same instance
        assert!(std::ptr::eq(g1, g2));
    }

    #[test]
    fn test_snapshot_serialization() {
        let m = CognitiveMetrics::new();
        CognitiveMetrics::inc(&m.mcts_searches);
        let snap = m.snapshot();
        let json = serde_json::to_string(&snap).unwrap();
        assert!(json.contains("\"mcts_searches\":1"));
        let parsed: MetricsSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.mcts_searches, 1);
    }

    #[test]
    fn test_metrics_default_trait() {
        let m = CognitiveMetrics::default();
        assert_eq!(CognitiveMetrics::get(&m.mcts_searches), 0);
    }
}
