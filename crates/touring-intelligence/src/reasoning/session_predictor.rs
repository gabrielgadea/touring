//! Markov-chain session predictor with EMA reinforcement learning.
//!
//! S11: Consolidated from 4 separate RwLocks into a single RwLock over
//! `PredictorState`. The 4 locks were always acquired sequentially (never
//! in parallel read paths), so a single lock reduces overhead and simplifies
//! the code without measurable contention increase.
//!
//! Lock ordering: SessionPredictor.state (RwLock, L2) — acquired AFTER
//! SemanticGraph.graph (L1) if both are held simultaneously.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::RwLock;

/// Maximum number of recent tool invocations to retain in history.
const MAX_HISTORY: usize = 64;

/// EMA learning rate for Q-value updates (α).
/// 0.1 gives ~10% weight to latest outcome, 90% to accumulated history.
const EMA_ALPHA: f64 = 0.1;

/// A single tool invocation recorded in session history.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolInvocation {
    /// Name of the tool that was invoked.
    pub tool_name: String,
    /// Invocation time as milliseconds since the Unix epoch.
    pub timestamp_ms: u64,
    /// Whether the invocation completed successfully.
    pub success: bool,
}

/// Markov transition matrix: from_tool -> (to_tool -> count).
/// INS-C2: counts stored as f64 to support exponential decay scaling.
type TransitionMatrix = HashMap<String, HashMap<String, f64>>;

/// Bigram transition matrix: (tool_a, tool_b) -> (tool_c -> count).
/// INS-C2: counts stored as f64 to support exponential decay scaling.
type BigramMatrix = HashMap<(String, String), HashMap<String, f64>>;

/// Q-value table: tool_name -> EMA of success rate (0.0 to 1.0).
type QTable = HashMap<String, f64>;

/// S11: Consolidated inner state protected by a single RwLock.
#[derive(Debug)]
struct PredictorState {
    /// Recent invocation history (bounded to MAX_HISTORY).
    history: VecDeque<ToolInvocation>,
    /// Unigram Markov transition counts: tool_a -> tool_b -> count.
    transitions: TransitionMatrix,
    /// Bigram Markov transitions: (tool_a, tool_b) -> tool_c -> count.
    bigram_transitions: BigramMatrix,
    /// Q-values: EMA of tool success rates.
    q_values: QTable,
}

impl PredictorState {
    fn new() -> Self {
        Self {
            history: VecDeque::with_capacity(MAX_HISTORY),
            transitions: HashMap::new(),
            bigram_transitions: HashMap::new(),
            q_values: HashMap::new(),
        }
    }
}

/// Predicts the next tool based on Markov transitions over session history.
///
/// Combines two signals:
/// 1. **Markov transitions**: which tool typically follows which
/// 2. **Q-values (EMA)**: which tools tend to succeed (reinforcement learning)
///
/// Prediction score = transition_probability × (0.5 + 0.5 × Q-value)
/// This weights likely-to-succeed transitions higher.
///
/// # Lock ordering
/// S11: Single `state` RwLock (L2) — always acquire SemanticGraph.graph (L1)
/// before this lock if both must be held simultaneously.
#[derive(Debug)]
pub struct SessionPredictor {
    /// S11: All predictor state behind a single lock.
    state: RwLock<PredictorState>,
}

impl SessionPredictor {
    /// Create a new empty predictor.
    pub fn new() -> Self {
        Self {
            state: RwLock::new(PredictorState::new()),
        }
    }

    /// Record a tool invocation and update transition counts.
    ///
    /// S11: Single write lock acquisition replaces 4 separate lock ops.
    pub fn record(&self, invocation: ToolInvocation) {
        let mut s = self.state.write().unwrap_or_else(|e| e.into_inner());

        // Step 1: read previous 2 tool names from history
        let prev_tool = s.history.back().map(|inv| inv.tool_name.clone());
        let prev_prev_tool = if s.history.len() >= 2 {
            s.history
                .get(s.history.len() - 2)
                .map(|inv| inv.tool_name.clone())
        } else {
            None
        };

        // Step 2a: update unigram Markov transition counts
        if let Some(ref from) = prev_tool {
            let to = invocation.tool_name.clone();
            *s.transitions
                .entry(from.clone())
                .or_default()
                .entry(to)
                .or_insert(0.0) += 1.0;
        }

        // Step 2b: update bigram transition counts
        if let (Some(pp), Some(p)) = (prev_prev_tool, prev_tool) {
            let to = invocation.tool_name.clone();
            *s.bigram_transitions
                .entry((pp, p))
                .or_default()
                .entry(to)
                .or_insert(0.0) += 1.0;
        }

        // Step 3: update Q-value via EMA
        let reward = if invocation.success { 1.0 } else { 0.0 };
        let current_q = s
            .q_values
            .entry(invocation.tool_name.clone())
            .or_insert(0.5);
        *current_q = EMA_ALPHA * reward + (1.0 - EMA_ALPHA) * *current_q;

        // Step 4: push invocation into history
        if s.history.len() >= MAX_HISTORY {
            s.history.pop_front();
        }
        s.history.push_back(invocation);
    }

    /// Predict the most likely next tool given the current last tool.
    ///
    /// Uses Q-value weighted prediction:
    /// score = transition_probability × (0.5 + 0.5 × Q-value)
    ///
    /// Returns `None` if no transitions have been recorded for `current_tool`.
    pub fn predict_next(&self, current_tool: &str) -> Option<(String, f64)> {
        let s = self.state.read().ok()?;
        let counts = s.transitions.get(current_tool)?;
        let total: f64 = counts.values().sum();
        if total == 0.0 {
            return None;
        }

        let mut best_tool = String::new();
        let mut best_score = f64::NEG_INFINITY;

        for (tool, &count) in counts.iter() {
            let transition_prob = count / total;
            let q = s.q_values.get(tool).copied().unwrap_or(0.5);
            let score = transition_prob * (0.5 + 0.5 * q);
            if score > best_score {
                best_score = score;
                best_tool = tool.clone();
            }
        }

        if best_tool.is_empty() {
            return None;
        }

        let confidence = counts.get(&best_tool).copied().unwrap_or(0.0) / total;
        Some((best_tool, confidence))
    }

    /// Return a cloned snapshot of recent history (lock-safe for background tasks).
    pub fn clone_recent_history(&self) -> Vec<ToolInvocation> {
        self.state
            .read()
            .map(|s| s.history.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Register an outcome for reinforcement learning via EMA.
    ///
    /// Updates Q-value: Q(tool) = α × reward + (1 - α) × Q(tool)
    /// where reward = 1.0 for success, 0.0 for failure, α = 0.1.
    pub fn register_outcome(&self, tool: &str, success: bool) {
        let reward = if success { 1.0 } else { 0.0 };
        let mut s = self.state.write().unwrap_or_else(|e| e.into_inner());
        let current = s.q_values.entry(tool.to_string()).or_insert(0.5);
        *current = EMA_ALPHA * reward + (1.0 - EMA_ALPHA) * *current;
    }

    /// Get the Q-value for a tool. Returns None if not yet recorded.
    pub fn q_value(&self, tool: &str) -> Option<f64> {
        self.state
            .read()
            .ok()
            .and_then(|s| s.q_values.get(tool).copied())
    }

    /// Return all Q-values as a snapshot (for diagnostics/serialization).
    pub fn q_values_snapshot(&self) -> HashMap<String, f64> {
        self.state
            .read()
            .map(|s| s.q_values.clone())
            .unwrap_or_default()
    }

    /// Predict the top-k next tools with scores.
    ///
    /// Blends unigram (tool_a → tool_c) and bigram (tool_a, tool_b → tool_c)
    /// transitions when bigram context is available:
    /// score = (0.4 × unigram_prob + 0.6 × bigram_prob) × (0.5 + 0.5 × Q-value)
    pub fn predict_top_k(&self, current_tool: &str, k: usize) -> Vec<(String, f64)> {
        let s = match self.state.read() {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        let counts = match s.transitions.get(current_tool) {
            Some(c) => c,
            None => return vec![],
        };
        let total: f64 = counts.values().sum();
        if total == 0.0 {
            return vec![];
        }

        // Try bigram context: (last_tool_in_history, current_tool)
        let bigram_context = s
            .history
            .back()
            .map(|inv| (inv.tool_name.clone(), current_tool.to_string()));

        let bigram_counts = bigram_context.and_then(|ctx| s.bigram_transitions.get(&ctx).cloned());

        // Collect all candidate tools from both unigram and bigram
        let mut all_tools: std::collections::HashSet<String> = counts.keys().cloned().collect();
        if let Some(ref bc) = bigram_counts {
            all_tools.extend(bc.keys().cloned());
        }

        let bigram_total: f64 = bigram_counts
            .as_ref()
            .map(|bc| bc.values().sum())
            .unwrap_or(0.0);

        let mut scored: Vec<(String, f64)> = all_tools
            .into_iter()
            .map(|tool| {
                let unigram_prob = counts.get(&tool).copied().unwrap_or(0.0) / total;
                let prob = if let Some(ref bc) = bigram_counts {
                    if bigram_total > 0.0 {
                        let bigram_prob = bc.get(&tool).copied().unwrap_or(0.0) / bigram_total;
                        0.4 * unigram_prob + 0.6 * bigram_prob
                    } else {
                        unigram_prob
                    }
                } else {
                    unigram_prob
                };
                let q = s.q_values.get(&tool).copied().unwrap_or(0.5);
                (tool, prob * (0.5 + 0.5 * q))
            })
            .collect();

        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(k);
        scored
    }

    /// S3: Warm the prediction cache by pre-loading bigram transitions for hint tools.
    ///
    /// For each tool in `hints`, pre-computes predict_top_k(tool, 3) to bring
    /// the transition matrices into CPU cache. This reduces latency for the
    /// first real prediction after session start.
    ///
    /// NOTE (ARCH-3): Current implementation is a best-effort cache warm.
    /// - `predict_top_k` reads from existing `transitions` and `bigram_transitions`
    /// - It does NOT pre-compute or build new bigram data — only reads existing
    /// - A full implementation would pre-build transition matrices from history
    ///   and cache the resulting prediction vectors for common tool sequences
    ///
    /// The predictor_task.rs calls this every 500ms with recent tool history.
    /// Without pre-computed cached predictions, each call merely triggers HashMap
    /// lookups rather than actual cache warming.
    pub fn warm_cache(&self, hints: &[String]) {
        crate::reasoning::metrics::CognitiveMetrics::inc(
            &crate::reasoning::metrics::CognitiveMetrics::global().warm_cache_calls,
        );

        for tool in hints {
            let _ = self.predict_top_k(tool, 3);
        }

        if !hints.is_empty() {
            tracing::debug!(tools = hints.len(), "warmed session predictor cache");
        }
    }

    /// INS-C2: Apply exponential decay to all transition counts.
    ///
    /// Multiplies every count by `decay_factor` (default 0.8). Call this at the
    /// start of a new session to discount older transition history.
    /// Entries that decay below 0.001 are pruned to keep memory bounded.
    pub fn apply_session_decay(&self, decay_factor: f64) {
        let factor = decay_factor.clamp(0.0, 1.0);
        let threshold = 0.001_f64;
        let mut s = self.state.write().unwrap_or_else(|e| e.into_inner());

        // Decay unigram transitions.
        for inner in s.transitions.values_mut() {
            inner.values_mut().for_each(|v| *v *= factor);
            inner.retain(|_, v| *v >= threshold);
        }
        s.transitions.retain(|_, inner| !inner.is_empty());

        // Decay bigram transitions.
        for inner in s.bigram_transitions.values_mut() {
            inner.values_mut().for_each(|v| *v *= factor);
            inner.retain(|_, v| *v >= threshold);
        }
        s.bigram_transitions.retain(|_, inner| !inner.is_empty());
    }
}

impl Default for SessionPredictor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inv(name: &str) -> ToolInvocation {
        ToolInvocation {
            tool_name: name.to_string(),
            timestamp_ms: 0,
            success: true,
        }
    }

    #[test]
    fn test_record_and_predict() {
        let p = SessionPredictor::new();
        p.record(inv("Read"));
        p.record(inv("Edit"));
        p.record(inv("Read"));
        p.record(inv("Edit"));
        let pred = p.predict_next("Read");
        assert!(pred.is_some());
        let (tool, confidence) = pred.unwrap();
        assert_eq!(tool, "Edit");
        assert!(confidence > 0.5);
    }

    #[test]
    fn test_predict_no_data() {
        let p = SessionPredictor::new();
        assert!(p.predict_next("Read").is_none());
    }

    #[test]
    fn test_predict_single_transition() {
        let p = SessionPredictor::new();
        p.record(inv("A"));
        p.record(inv("B"));
        let pred = p.predict_next("A").unwrap();
        assert_eq!(pred.0, "B");
        assert!((pred.1 - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_predict_multiple_successors() {
        let p = SessionPredictor::new();
        for _ in 0..3 {
            p.record(inv("A"));
            p.record(inv("B"));
        }
        p.record(inv("A"));
        p.record(inv("C"));
        let pred = p.predict_next("A").unwrap();
        assert_eq!(pred.0, "B");
        assert!((pred.1 - 0.75).abs() < 1e-6);
    }

    #[test]
    fn test_history_bounded_at_max() {
        let p = SessionPredictor::new();
        for i in 0..100 {
            p.record(inv(&format!("tool_{i}")));
        }
        let history = p.clone_recent_history();
        assert_eq!(history.len(), MAX_HISTORY);
        assert_eq!(history.last().unwrap().tool_name, "tool_99");
    }

    #[test]
    fn test_clone_recent_history_empty() {
        let p = SessionPredictor::new();
        assert!(p.clone_recent_history().is_empty());
    }

    #[test]
    fn test_default_trait() {
        let p = SessionPredictor::default();
        assert!(p.clone_recent_history().is_empty());
    }

    #[test]
    fn test_register_outcome_updates_q_value() {
        let p = SessionPredictor::new();

        // Initial Q-value should be None (not yet recorded)
        assert!(p.q_value("Read").is_none());

        // Register success — Q starts at 0.5, then EMA: 0.1*1.0 + 0.9*0.5 = 0.55
        p.register_outcome("Read", true);
        let q = p.q_value("Read").unwrap();
        assert!((q - 0.55).abs() < 1e-6, "Q after 1 success: {q}");

        // Register failure — 0.1*0.0 + 0.9*0.55 = 0.495
        p.register_outcome("Read", false);
        let q = p.q_value("Read").unwrap();
        assert!((q - 0.495).abs() < 1e-6, "Q after failure: {q}");
    }

    #[test]
    fn test_q_value_converges_on_repeated_success() {
        let p = SessionPredictor::new();
        for _ in 0..100 {
            p.register_outcome("Edit", true);
        }
        let q = p.q_value("Edit").unwrap();
        // After 100 successes, Q should converge close to 1.0
        assert!(q > 0.95, "Q after 100 successes: {q}");
    }

    #[test]
    fn test_record_auto_updates_q_values() {
        let p = SessionPredictor::new();
        p.record(inv("Read")); // success=true, updates Q
        assert!(p.q_value("Read").is_some());
    }

    #[test]
    fn test_predict_top_k() {
        let p = SessionPredictor::new();
        for _ in 0..5 {
            p.record(inv("A"));
            p.record(inv("B"));
        }
        p.record(inv("A"));
        p.record(inv("C"));

        let top = p.predict_top_k("A", 2);
        assert_eq!(top.len(), 2);
        assert_eq!(top[0].0, "B"); // 5 transitions to B vs 1 to C
    }

    #[test]
    fn test_warm_cache_no_panic() {
        let p = SessionPredictor::new();
        p.warm_cache(&["Read".to_string(), "Edit".to_string()]);
    }

    // S11: Test concurrent access with single lock
    #[test]
    fn test_concurrent_record_and_predict() {
        use std::sync::Arc;
        let p = Arc::new(SessionPredictor::new());

        // Pre-populate some data
        for _ in 0..10 {
            p.record(inv("Read"));
            p.record(inv("Edit"));
        }

        let handles: Vec<_> = (0..4)
            .map(|i| {
                let p = Arc::clone(&p);
                std::thread::spawn(move || {
                    for _ in 0..50 {
                        if i % 2 == 0 {
                            p.record(inv("Read"));
                            p.record(inv("Edit"));
                        } else {
                            let _ = p.predict_next("Read");
                            let _ = p.predict_top_k("Read", 3);
                        }
                    }
                })
            })
            .collect();

        for h in handles {
            h.join().expect("thread panicked");
        }

        // Verify state is consistent
        let history = p.clone_recent_history();
        assert!(!history.is_empty());
        assert!(history.len() <= MAX_HISTORY);
    }

    #[test]
    fn test_session_decay() {
        let p = SessionPredictor::new();
        for _ in 0..10 {
            p.record(inv("A"));
            p.record(inv("B"));
        }

        // Verify transitions exist
        assert!(p.predict_next("A").is_some());

        // Apply heavy decay multiple times to reduce counts below threshold
        // Each call multiplies by factor; need counts < 0.001 to prune.
        // Counts start at 10.0; 10.0 * 0.001^3 = 1e-8 < 0.001
        for _ in 0..3 {
            p.apply_session_decay(0.001);
        }

        // After severe decay, transitions should be pruned
        assert!(p.predict_next("A").is_none());
    }
}
