//! Pattern Bandit — Q-Learning for CILA pattern effectiveness.
//!
//! Tracks which CILA patterns lead to successful tool outcomes and uses
//! Q-values to re-rank semantic matches in the classifier pipeline.

use rustc_hash::FxHashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Default learning parameters for pattern bandit.
const DEFAULT_ALPHA: f64 = 0.1;
const DEFAULT_GAMMA: f64 = 0.99;
const DEFAULT_LAMBDA: f64 = 0.8;
const DEFAULT_EPSILON: f64 = 0.15;
const DEFAULT_INITIAL_Q: f64 = 0.5;

/// Thread-safe Pattern Q-Learning wrapper.
#[derive(Debug)]
pub struct PatternBandit {
    qtable: FxHashMap<u64, f64>,
    traces: FxHashMap<u64, f64>,
    update_counts: FxHashMap<u64, u64>,
    alpha: f64,
    gamma: f64,
    lambda: f64,
    initial_q: f64,
    epsilon: f64,
    total_updates: u64,
    pending_updates: u32,
}

impl Default for PatternBandit {
    fn default() -> Self {
        Self::new()
    }
}

impl PatternBandit {
    /// Creates a bandit with default learning hyperparameters and empty Q-tables.
    pub fn new() -> Self {
        Self {
            qtable: FxHashMap::default(),
            traces: FxHashMap::default(),
            update_counts: FxHashMap::default(),
            alpha: DEFAULT_ALPHA,
            gamma: DEFAULT_GAMMA,
            lambda: DEFAULT_LAMBDA,
            initial_q: DEFAULT_INITIAL_Q,
            epsilon: DEFAULT_EPSILON,
            total_updates: 0,
            pending_updates: 0,
        }
    }

    /// Compute djb2 hash for a pattern string.
    #[inline]
    pub fn hash_pattern(pattern: &str) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = rustc_hash::FxHasher::default();
        pattern.hash(&mut hasher);
        hasher.finish()
    }

    /// Get Q-value for a pattern.
    #[inline]
    pub fn q_value(&self, pattern: &str) -> f64 {
        let state = Self::hash_pattern(pattern);
        self.qtable.get(&state).copied().unwrap_or(self.initial_q)
    }

    /// Get effectiveness score (same as Q-value).
    #[inline]
    pub fn effectiveness(&self, pattern: &str) -> f64 {
        self.q_value(pattern)
    }

    /// Check if pattern has been seen.
    #[inline]
    pub fn is_known(&self, pattern: &str) -> bool {
        let state = Self::hash_pattern(pattern);
        self.update_counts.contains_key(&state)
    }

    /// Get update count for a pattern.
    #[inline]
    pub fn pattern_updates(&self, pattern: &str) -> u64 {
        let state = Self::hash_pattern(pattern);
        self.update_counts.get(&state).copied().unwrap_or(0)
    }

    /// Number of tracked patterns.
    #[inline]
    pub fn num_patterns(&self) -> usize {
        self.update_counts.len()
    }

    /// Total updates.
    #[inline]
    pub fn total_updates(&self) -> u64 {
        self.total_updates
    }

    /// Current epsilon.
    #[inline]
    pub fn epsilon(&self) -> f64 {
        self.epsilon
    }

    /// Pending updates for batch persistence.
    #[inline]
    pub fn pending_updates(&self) -> u32 {
        self.pending_updates
    }

    /// Reset pending counter.
    #[inline]
    pub fn reset_pending(&mut self) {
        self.pending_updates = 0;
    }

    /// Update with reward (TD learning).
    pub fn update(&mut self, pattern: &str, reward: f64) {
        let state = Self::hash_pattern(pattern);

        let q_current = *self.qtable.get(&state).unwrap_or(&self.initial_q);
        let td_error = reward - q_current;

        let trace = self.traces.entry(state).or_insert(0.0);
        let new_trace = self.gamma * self.lambda * (*trace) + 1.0;
        *trace = new_trace;

        let new_q = q_current + self.alpha * td_error * new_trace;
        self.qtable.insert(state, new_q);

        let gamma_lambda = self.gamma * self.lambda;
        for (old_state, trace_old) in self.traces.iter_mut() {
            if *old_state != state {
                *trace_old *= gamma_lambda;
            }
        }

        *self.update_counts.entry(state).or_insert(0) += 1;
        self.total_updates += 1;
        self.pending_updates += 1;
    }

    /// Update with success reward (+1.0).
    #[inline]
    pub fn update_success(&mut self, pattern: &str) {
        self.update(pattern, 1.0);
    }

    /// Update with failure reward (-0.5).
    #[inline]
    pub fn update_failure(&mut self, pattern: &str) {
        self.update(pattern, -0.5);
    }

    /// Re-rank patterns by combining semantic score with Q-value effectiveness.
    pub fn rerank_patterns(&self, patterns: &[(String, f64)]) -> Vec<(String, f64, f64)> {
        let mut results: Vec<_> = patterns
            .iter()
            .map(|(text, semantic_score)| {
                let q = self.effectiveness(text);
                let combined = 0.7 * semantic_score + 0.3 * (0.5 + 0.5 * q);
                (text.clone(), combined, q)
            })
            .collect();

        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        results
    }

    /// Get top-K most effective patterns.
    pub fn top_patterns(&self, k: usize) -> Vec<(u64, f64)> {
        let mut patterns: Vec<_> = self
            .update_counts
            .keys()
            .map(|&state| {
                let q = self.qtable.get(&state).copied().unwrap_or(self.initial_q);
                (state, q)
            })
            .collect();

        patterns.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        patterns.truncate(k);
        patterns
    }

    /// Export snapshot.
    pub fn export_snapshot(&self) -> PatternBanditSnapshot {
        PatternBanditSnapshot {
            qtable: self.qtable.clone(),
            traces: self.traces.clone(),
            update_counts: self.update_counts.clone(),
            epsilon: self.epsilon,
            total_updates: self.total_updates,
        }
    }

    /// Import snapshot.
    pub fn import_snapshot(&mut self, snapshot: &PatternBanditSnapshot) {
        self.qtable = snapshot.qtable.clone();
        self.traces = snapshot.traces.clone();
        self.update_counts = snapshot.update_counts.clone();
        self.epsilon = snapshot.epsilon;
        self.total_updates = snapshot.total_updates;
        self.pending_updates = 0;
    }

    /// Batch get effectiveness scores.
    pub fn batch_effectiveness<'a>(
        &self,
        patterns: impl Iterator<Item = &'a str>,
    ) -> FxHashMap<u64, f64> {
        let mut results = FxHashMap::default();
        for pattern in patterns {
            let state = Self::hash_pattern(pattern);
            let q = self.q_value(pattern);
            results.insert(state, q);
        }
        results
    }
}

/// Serializable snapshot of PatternBandit state.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PatternBanditSnapshot {
    /// Estimated Q-value per pattern state, keyed by the pattern's hashed id.
    #[serde(with = "serde_qtable")]
    pub qtable: FxHashMap<u64, f64>,
    /// Eligibility traces per pattern state for TD(λ) credit assignment.
    #[serde(with = "serde_traces")]
    pub traces: FxHashMap<u64, f64>,
    /// Number of updates applied to each pattern state.
    #[serde(with = "serde_counts")]
    pub update_counts: FxHashMap<u64, u64>,
    /// Current exploration rate for ε-greedy action selection.
    pub epsilon: f64,
    /// Total number of updates applied across all pattern states.
    pub total_updates: u64,
}

mod serde_qtable {
    use rustc_hash::FxHashMap;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    pub fn serialize<S>(map: &FxHashMap<u64, f64>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let vec: Vec<(u64, f64)> = map.iter().map(|(k, v)| (*k, *v)).collect();
        vec.serialize(serializer)
    }
    pub fn deserialize<'de, D>(deserializer: D) -> Result<FxHashMap<u64, f64>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let vec: Vec<(u64, f64)> = Vec::deserialize(deserializer)?;
        Ok(vec.into_iter().collect())
    }
}

mod serde_traces {
    use super::serde_qtable;
    pub use serde_qtable::{deserialize, serialize};
}

mod serde_counts {
    use rustc_hash::FxHashMap;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    pub fn serialize<S>(map: &FxHashMap<u64, u64>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let vec: Vec<(u64, u64)> = map.iter().map(|(k, v)| (*k, *v)).collect();
        vec.serialize(serializer)
    }
    pub fn deserialize<'de, D>(deserializer: D) -> Result<FxHashMap<u64, u64>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let vec: Vec<(u64, u64)> = Vec::deserialize(deserializer)?;
        Ok(vec.into_iter().collect())
    }
}

/// Async wrapper for PatternBandit.
#[derive(Debug, Clone)]
pub struct AsyncPatternBandit {
    inner: Arc<RwLock<PatternBandit>>,
}

impl Default for AsyncPatternBandit {
    fn default() -> Self {
        Self::new()
    }
}

impl AsyncPatternBandit {
    /// Creates a fresh async bandit wrapping a default [`PatternBandit`] behind an `RwLock`.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(PatternBandit::new())),
        }
    }

    /// Records a successful outcome for `pattern` (reward `+1`).
    pub async fn update_success(&self, pattern: &str) {
        let mut bandit = self.inner.write().await;
        bandit.update_success(pattern);
    }

    /// Records a failed outcome for `pattern` (reward `-1`).
    pub async fn update_failure(&self, pattern: &str) {
        let mut bandit = self.inner.write().await;
        bandit.update_failure(pattern);
    }

    /// Applies an arbitrary `reward` to `pattern` and runs a TD update.
    pub async fn update(&self, pattern: &str, reward: f64) {
        let mut bandit = self.inner.write().await;
        bandit.update(pattern, reward);
    }

    /// Returns the current estimated Q-value for `pattern`.
    pub async fn q_value(&self, pattern: &str) -> f64 {
        let bandit = self.inner.read().await;
        bandit.q_value(pattern)
    }

    /// Returns the effectiveness score (Q-value normalized to `[0,1]`) for `pattern`.
    pub async fn effectiveness(&self, pattern: &str) -> f64 {
        let bandit = self.inner.read().await;
        bandit.effectiveness(pattern)
    }

    /// Returns the total number of updates applied across all patterns.
    pub async fn total_updates(&self) -> u64 {
        let bandit = self.inner.read().await;
        bandit.total_updates()
    }

    /// Returns the number of distinct patterns the bandit has observed.
    pub async fn num_patterns(&self) -> usize {
        let bandit = self.inner.read().await;
        bandit.num_patterns()
    }

    /// Returns the `k` highest-valued patterns as `(pattern_id, q_value)` pairs.
    pub async fn top_patterns(&self, k: usize) -> Vec<(u64, f64)> {
        let bandit = self.inner.read().await;
        bandit.top_patterns(k)
    }

    /// Exports the bandit's full state as a serializable [`PatternBanditSnapshot`].
    pub async fn export_snapshot(&self) -> PatternBanditSnapshot {
        let bandit = self.inner.read().await;
        bandit.export_snapshot()
    }

    /// Restores the bandit's state from a previously exported snapshot.
    pub async fn import_snapshot(&self, snapshot: &PatternBanditSnapshot) {
        let mut bandit = self.inner.write().await;
        bandit.import_snapshot(snapshot);
    }

    /// Re-ranks `(pattern, prior_score)` pairs by blending the prior with the learned Q-value,
    /// returning `(pattern, prior_score, blended_score)` triples.
    pub async fn rerank_patterns(&self, patterns: &[(String, f64)]) -> Vec<(String, f64, f64)> {
        let bandit = self.inner.read().await;
        bandit.rerank_patterns(patterns)
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pattern_hash_deterministic() {
        let h1 = PatternBandit::hash_pattern("calculate");
        let h2 = PatternBandit::hash_pattern("calculate");
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_pattern_hash_different() {
        let h1 = PatternBandit::hash_pattern("calculate");
        let h2 = PatternBandit::hash_pattern("compute");
        assert_ne!(h1, h2);
    }

    #[test]
    fn test_initial_q_value() {
        let bandit = PatternBandit::new();
        assert_eq!(bandit.q_value("unknown"), 0.5);
    }

    #[test]
    fn test_update_success() {
        let mut bandit = PatternBandit::new();
        let initial = bandit.q_value("calc");
        bandit.update_success("calc");
        let new = bandit.q_value("calc");
        assert!(new > initial);
    }

    #[test]
    fn test_update_failure() {
        let mut bandit = PatternBandit::new();
        let initial = bandit.q_value("calc");
        bandit.update_failure("calc");
        let new = bandit.q_value("calc");
        assert!(new < initial);
    }

    #[test]
    fn test_is_known() {
        let mut bandit = PatternBandit::new();
        assert!(!bandit.is_known("calc"));
        bandit.update_success("calc");
        assert!(bandit.is_known("calc"));
    }

    #[test]
    fn test_snapshot_roundtrip() {
        let mut bandit = PatternBandit::new();
        bandit.update_success("p1");
        bandit.update_failure("p2");

        let snap = bandit.export_snapshot();
        assert_eq!(snap.total_updates, 2);

        let mut bandit2 = PatternBandit::new();
        bandit2.import_snapshot(&snap);
        assert_eq!(bandit2.total_updates(), 2);
    }

    #[tokio::test]
    async fn test_async_bandit() {
        let bandit = AsyncPatternBandit::new();
        bandit.update_success("calc").await;
        assert!(bandit.q_value("calc").await > 0.5);
        assert_eq!(bandit.num_patterns().await, 1);
    }

    #[test]
    fn test_rerank_patterns() {
        let mut bandit = PatternBandit::new();
        bandit.update_success("good");
        bandit.update_success("good");
        bandit.update_success("good");
        bandit.update_failure("bad");
        bandit.update_failure("bad");

        // "bad" has higher semantic score but much lower effectiveness
        // Combined = 0.7 * semantic + 0.3 * (0.5 + 0.5 * q)
        // good: 0.7*0.7 + 0.3*(0.5+0.5*0.55) = 0.49 + 0.3*0.775 = 0.49+0.2325 = 0.7225
        // bad:  0.7*0.6 + 0.3*(0.5+0.5*0.4) = 0.42 + 0.3*0.7 = 0.42+0.21 = 0.63
        let patterns = vec![("good".to_string(), 0.7), ("bad".to_string(), 0.6)];

        let reranked = bandit.rerank_patterns(&patterns);
        assert_eq!(reranked[0].0, "good", "good pattern should rank first");
        assert_eq!(reranked[1].0, "bad", "bad pattern should rank second");
    }

    #[test]
    fn test_top_patterns() {
        let mut bandit = PatternBandit::new();
        bandit.update_success("p1");
        bandit.update_success("p1");
        bandit.update_success("p1");
        bandit.update_failure("p2");

        let top = bandit.top_patterns(2);
        assert_eq!(top.len(), 2);
        assert!(top[0].1 >= top[1].1);
    }
}
