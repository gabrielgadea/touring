//! Learning Loop v2 — Quality feedback tracking with bounded buffers and EMA.
//!
//! Records generation events (success/failure), tracks per-strategy
//! success rates via exponential moving average, per-symbol rewards
//! with LRU eviction, and per-language statistics for context-aware
//! strategy recommendations.

use std::collections::{HashMap, VecDeque};

use serde::{Deserialize, Serialize};

// ─── Constants ─────────────────────────────────────────────────────────

/// Maximum events retained in the ring buffer.
const MAX_EVENTS: usize = 1024;

/// Maximum symbols tracked in per-symbol reward map.
const MAX_SYMBOLS: usize = 512;

/// EMA smoothing factor (alpha). Higher = more weight on recent events.
const EMA_ALPHA: f64 = 0.1;

// ─── Types ──────────────────────────────────────────────────────────────

/// A recorded code generation event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerationEvent {
    /// Symbol that was generated/edited
    pub symbol_name: String,
    /// Language of the target file
    pub language: String,
    /// Whether the generation was successful (compiled, passed speculate)
    pub success: bool,
    /// Which strategy was used (e.g., "vgp_v2", "scope_aware")
    pub strategy_used: String,
    /// Timestamp in milliseconds
    pub timestamp_ms: u64,
}

/// Per-strategy statistics with EMA success rate and per-language breakdown.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyStats {
    /// Total events for this strategy
    pub total: usize,
    /// Total successes for this strategy
    pub successes: usize,
    /// Exponential moving average of success rate (0.0 to 1.0)
    pub ema_rate: f64,
    /// Per-language (total, successes) counts
    pub per_language: HashMap<String, (usize, usize)>,
}

impl Default for StrategyStats {
    fn default() -> Self {
        Self {
            total: 0,
            successes: 0,
            ema_rate: 0.5, // neutral prior
            per_language: HashMap::new(),
        }
    }
}

/// Learning loop v2 that tracks generation quality over time.
///
/// Uses bounded ring buffer for events, EMA for success rates,
/// per-symbol reward tracking with LRU eviction, and per-language
/// statistics for context-aware recommendations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearningLoop {
    /// Bounded ring buffer of recent events
    recent_events: VecDeque<GenerationEvent>,
    /// Cumulative reward sum
    pub reward_sum: f64,
    /// Total event count (lifetime, not bounded by ring buffer)
    pub event_count: usize,
    /// Per-strategy statistics with EMA
    strategy_stats: HashMap<String, StrategyStats>,
    /// Per-symbol cumulative rewards
    symbol_rewards: HashMap<String, f64>,
    /// LRU order for symbol rewards (front = oldest)
    symbol_lru: VecDeque<String>,
}

impl Default for LearningLoop {
    fn default() -> Self {
        Self {
            recent_events: VecDeque::new(),
            reward_sum: 0.0,
            event_count: 0,
            strategy_stats: HashMap::new(),
            symbol_rewards: HashMap::new(),
            symbol_lru: VecDeque::new(),
        }
    }
}

// ─── Implementation ─────────────────────────────────────────────────────

impl LearningLoop {
    /// Create a new empty learning loop.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a generation event.
    ///
    /// Maintains a bounded ring buffer (drops oldest when full),
    /// updates per-strategy EMA and per-language stats.
    pub fn record_event(&mut self, event: GenerationEvent) {
        self.event_count += 1;
        if event.success {
            self.reward_sum += 1.0;
        }

        // Update per-strategy stats with EMA
        let stats = self
            .strategy_stats
            .entry(event.strategy_used.clone())
            .or_default();
        stats.total += 1;
        if event.success {
            stats.successes += 1;
        }
        let observation = if event.success { 1.0 } else { 0.0 };
        stats.ema_rate = EMA_ALPHA * observation + (1.0 - EMA_ALPHA) * stats.ema_rate;

        // Update per-language breakdown
        let lang_entry = stats
            .per_language
            .entry(event.language.clone())
            .or_insert((0, 0));
        lang_entry.0 += 1;
        if event.success {
            lang_entry.1 += 1;
        }

        // Bounded ring buffer
        if self.recent_events.len() >= MAX_EVENTS {
            self.recent_events.pop_front();
        }
        self.recent_events.push_back(event);
    }

    /// Apply a reward (positive) or penalty (negative) for a symbol.
    ///
    /// Tracks per-symbol rewards with LRU eviction when the map is full.
    pub fn reward(&mut self, symbol_name: &str, delta: f64) {
        self.reward_sum += delta;

        // Update per-symbol reward
        let entry = self
            .symbol_rewards
            .entry(symbol_name.to_string())
            .or_insert(0.0);
        *entry += delta;

        // Update LRU: move to back (most recently used)
        if let Some(pos) = self.symbol_lru.iter().position(|s| s == symbol_name) {
            self.symbol_lru.remove(pos);
        }
        self.symbol_lru.push_back(symbol_name.to_string());

        // Evict oldest if over capacity
        while self.symbol_lru.len() > MAX_SYMBOLS {
            if let Some(evicted) = self.symbol_lru.pop_front() {
                self.symbol_rewards.remove(&evicted);
            }
        }
    }

    /// Get the cumulative reward for a specific symbol.
    pub fn symbol_reward(&self, symbol_name: &str) -> f64 {
        self.symbol_rewards.get(symbol_name).copied().unwrap_or(0.0)
    }

    /// Compute the success rate for a given strategy.
    ///
    /// Returns the EMA rate if available, else 0.0.
    pub fn success_rate_for(&self, strategy: &str) -> f64 {
        match self.strategy_stats.get(strategy) {
            Some(stats) if stats.total > 0 => stats.successes as f64 / stats.total as f64,
            _ => 0.0,
        }
    }

    /// Recommend the best strategy for a given language and complexity.
    ///
    /// Prefers strategies with high EMA rate for the specified language.
    /// Falls back to global EMA if no language-specific data exists.
    /// Returns `"vgp_v2"` as default if no history.
    pub fn recommend_strategy(&self, language: &str, _complexity: usize) -> String {
        if self.strategy_stats.is_empty() {
            return "vgp_v2".to_string();
        }

        self.strategy_stats
            .iter()
            .max_by(|a, b| {
                let rate_a = language_aware_rate(a.1, language);
                let rate_b = language_aware_rate(b.1, language);
                rate_a
                    .partial_cmp(&rate_b)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|(name, _)| name.clone())
            .unwrap_or_else(|| "vgp_v2".to_string())
    }

    /// Get recent events (read-only, bounded ring buffer).
    pub fn events(&self) -> &VecDeque<GenerationEvent> {
        &self.recent_events
    }

    /// Average reward per event.
    pub fn average_reward(&self) -> f64 {
        if self.event_count == 0 {
            return 0.0;
        }
        self.reward_sum / self.event_count as f64
    }

    /// Get all unique strategy names that have been used.
    pub fn strategies_used(&self) -> Vec<String> {
        let mut names: Vec<String> = self.strategy_stats.keys().cloned().collect();
        names.sort();
        names
    }

    /// Get per-strategy statistics (read-only).
    pub fn strategy_stats(&self) -> &HashMap<String, StrategyStats> {
        &self.strategy_stats
    }
}

/// Compute a language-aware score for strategy recommendation.
///
/// If language-specific data exists, blend it 70/30 with global EMA.
/// Otherwise fall back to global EMA alone.
fn language_aware_rate(stats: &StrategyStats, language: &str) -> f64 {
    if let Some(&(total, successes)) = stats.per_language.get(language) {
        if total > 0 {
            let lang_rate = successes as f64 / total as f64;
            // Blend: 70% language-specific, 30% global EMA
            return 0.7 * lang_rate + 0.3 * stats.ema_rate;
        }
    }
    stats.ema_rate
}

// ─── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_learning_loop_records() {
        let mut lloop = LearningLoop::new();
        lloop.record_event(GenerationEvent {
            symbol_name: "HashMap".to_string(),
            language: "rust".to_string(),
            success: true,
            strategy_used: "vgp_v2".to_string(),
            timestamp_ms: 1000,
        });
        assert_eq!(lloop.event_count, 1);
        assert_eq!(lloop.events().len(), 1);
    }

    #[test]
    fn test_success_rate() {
        let mut lloop = LearningLoop::new();
        for i in 0..10 {
            lloop.record_event(GenerationEvent {
                symbol_name: format!("sym_{i}"),
                language: "rust".to_string(),
                success: i % 3 != 0, // 0,3,6,9 fail = 4 fail, 6 success → 60%
                strategy_used: "vgp_v2".to_string(),
                timestamp_ms: i as u64 * 100,
            });
        }
        let rate = lloop.success_rate_for("vgp_v2");
        assert!((rate - 0.6).abs() < 0.01, "expected ~0.6, got {rate}");
    }

    #[test]
    fn test_success_rate_no_events() {
        let lloop = LearningLoop::new();
        assert_eq!(lloop.success_rate_for("unknown"), 0.0);
    }

    #[test]
    fn test_reward() {
        let mut lloop = LearningLoop::new();
        lloop.record_event(GenerationEvent {
            symbol_name: "Foo".to_string(),
            language: "rust".to_string(),
            success: true,
            strategy_used: "vgp_v2".to_string(),
            timestamp_ms: 100,
        });
        lloop.reward("Foo", 1.5);
        assert!((lloop.reward_sum - 2.5).abs() < 0.01); // 1.0 (success) + 1.5 (reward)
    }

    #[test]
    fn test_recommend_strategy() {
        let mut lloop = LearningLoop::new();
        // vgp_v2: 80% success
        for i in 0..10 {
            lloop.record_event(GenerationEvent {
                symbol_name: format!("sym_{i}"),
                language: "rust".to_string(),
                success: i < 8,
                strategy_used: "vgp_v2".to_string(),
                timestamp_ms: i as u64 * 100,
            });
        }
        // scope_aware: 50% success
        for i in 0..10 {
            lloop.record_event(GenerationEvent {
                symbol_name: format!("sym_{i}"),
                language: "rust".to_string(),
                success: i < 5,
                strategy_used: "scope_aware".to_string(),
                timestamp_ms: i as u64 * 100,
            });
        }

        let rec = lloop.recommend_strategy("rust", 5);
        assert_eq!(rec, "vgp_v2", "vgp_v2 has higher rate");
    }

    #[test]
    fn test_recommend_default_no_history() {
        let lloop = LearningLoop::new();
        assert_eq!(lloop.recommend_strategy("rust", 5), "vgp_v2");
    }

    #[test]
    fn test_average_reward() {
        let mut lloop = LearningLoop::new();
        lloop.record_event(GenerationEvent {
            symbol_name: "A".to_string(),
            language: "rust".to_string(),
            success: true,
            strategy_used: "vgp_v2".to_string(),
            timestamp_ms: 100,
        });
        lloop.record_event(GenerationEvent {
            symbol_name: "B".to_string(),
            language: "rust".to_string(),
            success: false,
            strategy_used: "vgp_v2".to_string(),
            timestamp_ms: 200,
        });
        assert!((lloop.average_reward() - 0.5).abs() < 0.01);
    }

    #[test]
    fn test_strategies_used() {
        let mut lloop = LearningLoop::new();
        lloop.record_event(GenerationEvent {
            symbol_name: "A".to_string(),
            language: "rust".to_string(),
            success: true,
            strategy_used: "vgp_v2".to_string(),
            timestamp_ms: 100,
        });
        lloop.record_event(GenerationEvent {
            symbol_name: "B".to_string(),
            language: "rust".to_string(),
            success: true,
            strategy_used: "scope_aware".to_string(),
            timestamp_ms: 200,
        });
        let strats = lloop.strategies_used();
        assert!(strats.contains(&"vgp_v2".to_string()));
        assert!(strats.contains(&"scope_aware".to_string()));
    }

    // ─── S7: LearningLoop v2 tests ─────────────────────────────────────

    #[test]
    fn test_ring_buffer_bounded() {
        let mut lloop = LearningLoop::new();
        for i in 0..(MAX_EVENTS + 100) {
            lloop.record_event(GenerationEvent {
                symbol_name: format!("sym_{i}"),
                language: "rust".to_string(),
                success: true,
                strategy_used: "vgp_v2".to_string(),
                timestamp_ms: i as u64,
            });
        }
        assert_eq!(lloop.events().len(), MAX_EVENTS);
        assert_eq!(lloop.event_count, MAX_EVENTS + 100); // lifetime count
    }

    #[test]
    fn test_per_symbol_reward() {
        let mut lloop = LearningLoop::new();
        lloop.reward("Foo", 1.0);
        lloop.reward("Foo", 0.5);
        lloop.reward("Bar", -0.3);
        assert!((lloop.symbol_reward("Foo") - 1.5).abs() < 0.01);
        assert!((lloop.symbol_reward("Bar") - (-0.3)).abs() < 0.01);
        assert_eq!(lloop.symbol_reward("Unknown"), 0.0);
    }

    #[test]
    fn test_symbol_lru_eviction() {
        let mut lloop = LearningLoop::new();
        for i in 0..(MAX_SYMBOLS + 10) {
            lloop.reward(&format!("sym_{i}"), 1.0);
        }
        // Oldest symbols should be evicted
        assert!(lloop.symbol_rewards.len() <= MAX_SYMBOLS);
        // Most recent should still exist
        assert!(lloop.symbol_reward(&format!("sym_{}", MAX_SYMBOLS + 9)) > 0.0);
    }

    #[test]
    fn test_ema_updates() {
        let mut lloop = LearningLoop::new();
        // Record 10 successes — EMA should trend toward 1.0
        for i in 0..10 {
            lloop.record_event(GenerationEvent {
                symbol_name: format!("s{i}"),
                language: "rust".to_string(),
                success: true,
                strategy_used: "test_strat".to_string(),
                timestamp_ms: i as u64,
            });
        }
        let stats = lloop
            .strategy_stats()
            .get("test_strat")
            .expect("stats should exist");
        assert!(
            stats.ema_rate > 0.7,
            "EMA should be high after all successes: {}",
            stats.ema_rate
        );
    }

    #[test]
    fn test_per_language_tracking() {
        let mut lloop = LearningLoop::new();
        // rust: 100% success
        for i in 0..5 {
            lloop.record_event(GenerationEvent {
                symbol_name: format!("r{i}"),
                language: "rust".to_string(),
                success: true,
                strategy_used: "vgp_v2".to_string(),
                timestamp_ms: i as u64,
            });
        }
        // python: 0% success
        for i in 0..5 {
            lloop.record_event(GenerationEvent {
                symbol_name: format!("p{i}"),
                language: "python".to_string(),
                success: false,
                strategy_used: "vgp_v2".to_string(),
                timestamp_ms: 100 + i as u64,
            });
        }
        let stats = lloop
            .strategy_stats()
            .get("vgp_v2")
            .expect("stats should exist");
        let (rt, rs) = stats.per_language.get("rust").expect("rust lang");
        assert_eq!(*rt, 5);
        assert_eq!(*rs, 5);
        let (pt, ps) = stats.per_language.get("python").expect("python lang");
        assert_eq!(*pt, 5);
        assert_eq!(*ps, 0);
    }

    #[test]
    fn test_recommend_language_aware() {
        let mut lloop = LearningLoop::new();
        // strat_a: good at rust (100%), bad at python (0%)
        for i in 0..10 {
            lloop.record_event(GenerationEvent {
                symbol_name: format!("r{i}"),
                language: "rust".to_string(),
                success: true,
                strategy_used: "strat_a".to_string(),
                timestamp_ms: i as u64,
            });
            lloop.record_event(GenerationEvent {
                symbol_name: format!("p{i}"),
                language: "python".to_string(),
                success: false,
                strategy_used: "strat_a".to_string(),
                timestamp_ms: 100 + i as u64,
            });
        }
        // strat_b: good at python (100%), bad at rust (0%)
        for i in 0..10 {
            lloop.record_event(GenerationEvent {
                symbol_name: format!("r{i}"),
                language: "rust".to_string(),
                success: false,
                strategy_used: "strat_b".to_string(),
                timestamp_ms: 200 + i as u64,
            });
            lloop.record_event(GenerationEvent {
                symbol_name: format!("p{i}"),
                language: "python".to_string(),
                success: true,
                strategy_used: "strat_b".to_string(),
                timestamp_ms: 300 + i as u64,
            });
        }
        assert_eq!(lloop.recommend_strategy("rust", 0), "strat_a");
        assert_eq!(lloop.recommend_strategy("python", 0), "strat_b");
    }
}
