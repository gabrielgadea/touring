//! Q-table implementation with eligibility traces.
//!
//! TD(λ) Q-learning with sparse Q-table representation.
//!
//! Unified from touring/src/learning/qtable.rs (457 LOC)

use std::collections::VecDeque;
use std::path::Path;
use std::sync::{Arc, Mutex};

use rustc_hash::FxHashMap;

/// Learning parameters for Q-learning.
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct LearningParams {
    /// Learning rate (alpha).
    pub alpha: f64,
    /// Discount factor (gamma).
    pub gamma: f64,
    /// Eligibility trace decay (lambda).
    pub lambda: f64,
    /// Initial Q-value for unexplored state-action pairs.
    pub initial_q: f64,
    /// Epsilon for epsilon-greedy exploration (probability of random action).
    pub epsilon: f64,
    /// Decay factor applied to epsilon after each epsilon-greedy selection.
    pub epsilon_decay: f64,
    /// Minimum epsilon value (floor for decay).
    pub epsilon_min: f64,
}

impl Default for LearningParams {
    fn default() -> Self {
        Self {
            alpha: 0.1,
            gamma: 0.99,
            lambda: 0.9,
            initial_q: 0.0,
            epsilon: 0.15,
            epsilon_decay: 0.995,
            epsilon_min: 0.05,
        }
    }
}

/// State-action pair key.
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub struct StateAction {
    /// State identifier.
    pub state: u64,
    /// Action identifier.
    pub action: u64,
}

impl StateAction {
    /// Create a new state-action pair.
    pub fn new(state: u64, action: u64) -> Self {
        Self { state, action }
    }
}

/// Q-learning with eligibility traces (TD-lambda).
pub trait QLearning {
    /// Update Q-value with TD error and eligibility traces.
    ///
    /// Returns the TD error for monitoring.
    #[must_use = "TD error indicates learning progress"]
    fn update(
        &mut self,
        state: u64,
        action: u64,
        reward: f64,
        next_state: u64,
        next_action: Option<u64>,
        terminal: bool,
    ) -> f64;

    /// Get Q-value for state-action pair.
    fn get_q(&self, state: u64, action: u64) -> f64;

    /// Get best action for state (argmax Q).
    fn best_action(&self, state: u64) -> Option<u64>;

    /// Reset eligibility traces (call at episode start).
    fn reset_traces(&mut self);

    /// Get learning parameters.
    fn get_params(&self) -> LearningParams;

    /// Select an action using epsilon-greedy exploration.
    ///
    /// With probability epsilon: selects a random action from known actions for that state.
    /// With probability (1-epsilon): selects the greedy best action (argmax Q).
    /// After each call, decays epsilon: `epsilon *= epsilon_decay`, floored at `epsilon_min`.
    ///
    /// Returns `None` if no actions are known for the state.
    fn epsilon_greedy_action(&mut self, state: u64) -> Option<u64>;
}

/// Granular reward breakdown for multi-dimensional RL feedback.
/// Weights: compilation(0.25) + lint(0.20) + type_safe(0.20) + tests(0.25) + coverage(0.10) = 1.0
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct RewardBreakdown {
    /// Code compiles without errors (ruff parse check). Weight: 0.25
    pub compilation: f64,
    /// Lint clean (ruff check 0 errors). Weight: 0.20
    pub lint: f64,
    /// Type safe (pyright 0 errors). Weight: 0.20
    pub type_safe: f64,
    /// Tests pass (pytest exit 0). Weight: 0.25
    pub tests: f64,
    /// Coverage adequate (>=80%). Weight: 0.10
    pub coverage: f64,
}

impl RewardBreakdown {
    const WEIGHTS: [f64; 5] = [0.25, 0.20, 0.20, 0.25, 0.10];

    /// Compute weighted total, clamping each sub-reward to [0.0, 1.0].
    pub fn weighted_total(&self) -> f64 {
        let values = [
            self.compilation,
            self.lint,
            self.type_safe,
            self.tests,
            self.coverage,
        ];
        values
            .iter()
            .zip(Self::WEIGHTS.iter())
            .map(|(v, w)| v.clamp(0.0, 1.0) * w)
            .sum()
    }
}

/// Accumulated statistics about granular reward updates.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct RewardStats {
    /// Total number of granular updates performed.
    pub total_granular_updates: u64,
    /// Running average of compilation sub-rewards.
    pub avg_compilation: f64,
    /// Running average of lint sub-rewards.
    pub avg_lint: f64,
    /// Running average of type_safe sub-rewards.
    pub avg_type_safe: f64,
    /// Running average of tests sub-rewards.
    pub avg_tests: f64,
    /// Running average of coverage sub-rewards.
    pub avg_coverage: f64,
}

/// Real-time convergence metrics for monitoring RL health.
///
/// Tracks exponential moving average of TD error magnitude and a
/// sliding-window reward average. These two signals together indicate
/// whether the Q-learning loop is converging, stable, or diverging.
#[derive(Debug, Clone)]
pub struct QLearningMetrics {
    /// Exponential moving average of absolute TD error.
    td_error_ema: f64,
    /// Sliding window for reward averaging.
    reward_window: VecDeque<f64>,
    /// Window size for reward averaging.
    window_size: usize,
    /// EMA decay factor.
    ema_alpha: f64,
    /// Total updates tracked.
    total_updates: u64,
}

impl QLearningMetrics {
    /// Create new metrics tracker with a given sliding-window size.
    pub fn new(window_size: usize) -> Self {
        Self {
            td_error_ema: 0.0,
            reward_window: VecDeque::with_capacity(window_size),
            window_size,
            ema_alpha: 0.1,
            total_updates: 0,
        }
    }

    /// Record a TD error and reward observation.
    pub fn record(&mut self, td_error: f64, reward: f64) {
        self.td_error_ema =
            self.ema_alpha * td_error.abs() + (1.0 - self.ema_alpha) * self.td_error_ema;

        self.reward_window.push_back(reward);
        if self.reward_window.len() > self.window_size {
            self.reward_window.pop_front();
        }
        self.total_updates += 1;
    }

    /// Current EMA of absolute TD error.
    pub fn td_error_ema(&self) -> f64 {
        self.td_error_ema
    }

    /// Average reward over the sliding window.
    pub fn avg_reward(&self) -> f64 {
        if self.reward_window.is_empty() {
            return 0.0;
        }
        self.reward_window.iter().sum::<f64>() / self.reward_window.len() as f64
    }

    /// Total number of tracked updates.
    pub fn total_updates(&self) -> u64 {
        self.total_updates
    }

    /// Is the RL converging? (TD error decreasing, reward stable/increasing)
    pub fn is_converging(&self) -> bool {
        self.total_updates >= 10 && self.td_error_ema < 0.1 && self.avg_reward() > 0.3
    }

    /// Is the RL diverging? (TD error high, reward low)
    pub fn is_diverging(&self) -> bool {
        self.total_updates >= 10 && (self.td_error_ema > 1.0 || self.avg_reward() < -0.5)
    }
}

impl Default for QLearningMetrics {
    fn default() -> Self {
        Self::new(100)
    }
}

/// Maximum number of entries before eviction triggers.
/// Prevents unbounded memory growth in long-running sessions.
pub const MAX_ENTRIES: usize = 50_000;

/// Q-table with eligibility traces for TD(λ) learning.
#[derive(Debug)]
pub struct QTable {
    /// Q-values for state-action pairs (sparse).
    q_values: FxHashMap<StateAction, f64>,
    /// Eligibility traces for state-action pairs.
    traces: FxHashMap<StateAction, f64>,
    /// Learning parameters.
    params: LearningParams,
    /// Known actions for each state (for best_action lookup).
    state_actions: FxHashMap<u64, Vec<u64>>,
    /// Count of granular reward updates.
    granular_update_count: u64,
    /// Running sums for each sub-reward dimension [compilation, lint, type_safe, tests, coverage].
    reward_sums: [f64; 5],
    /// Convergence metrics tracker (transient — not serialized).
    metrics: QLearningMetrics,
}

impl QTable {
    /// Create new Q-table with default parameters.
    pub fn new() -> Self {
        Self::with_params(LearningParams::default())
    }

    /// Create Q-table with custom parameters.
    pub(crate) fn with_params(params: LearningParams) -> Self {
        Self {
            q_values: FxHashMap::default(),
            traces: FxHashMap::default(),
            params,
            state_actions: FxHashMap::default(),
            granular_update_count: 0,
            reward_sums: [0.0; 5],
            metrics: QLearningMetrics::default(),
        }
    }

    /// Evict the lowest absolute Q-value entries to free capacity.
    ///
    /// Retains approximately `keep_ratio` fraction of entries (by count),
    /// removing those with the smallest absolute Q-value (least decisive).
    ///
    /// `keep_ratio` is clamped to `[0.1, 1.0]`. A value of `0.75` keeps 75%
    /// of entries and evicts the 25% with smallest |Q|.
    pub fn evict_low_value_entries(&mut self, keep_ratio: f64) {
        let keep_ratio = keep_ratio.clamp(0.1, 1.0);
        let keep_count = ((self.q_values.len() as f64) * keep_ratio).ceil() as usize;
        if keep_count >= self.q_values.len() {
            return;
        }

        // Sort entries by absolute Q-value descending; keep the top `keep_count`.
        let mut entries: Vec<(StateAction, f64)> =
            self.q_values.iter().map(|(&sa, &q)| (sa, q)).collect();
        entries.sort_unstable_by(|a, b| {
            b.1.abs()
                .partial_cmp(&a.1.abs())
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Determine which state-actions survive eviction.
        let kept: std::collections::HashSet<StateAction> =
            entries.iter().take(keep_count).map(|(sa, _)| *sa).collect();

        // Rebuild q_values with only the kept entries.
        self.q_values.clear();
        self.state_actions.clear();
        // Preserve eligibility traces for kept entries; evicting mid-episode would
        // corrupt TD(λ) credit assignment for the active episode.
        self.traces.retain(|sa, _| kept.contains(sa));

        for (sa, q) in entries.into_iter().take(keep_count) {
            self.q_values.insert(sa, q);
            self.state_actions
                .entry(sa.state)
                .or_default()
                .push(sa.action);
        }
    }

    /// Initialize Q-value if not present. Triggers eviction when at capacity.
    fn ensure_q(&mut self, state: u64, action: u64) {
        let sa = StateAction::new(state, action);
        if !self.q_values.contains_key(&sa) {
            // Evict when at capacity before inserting new entry
            if self.q_values.len() >= MAX_ENTRIES {
                self.evict_low_value_entries(0.75);
            }
            self.q_values.insert(sa, self.params.initial_q);
            self.state_actions.entry(state).or_default().push(action);
        }
    }

    /// Get Q-value for a (state, action) pair, returning initial_q if not present.
    pub fn get_value(&self, state: u64, action: u64) -> f64 {
        self.get_q_internal(state, action)
    }

    /// Get Q-value, returning initial_q if not present.
    fn get_q_internal(&self, state: u64, action: u64) -> f64 {
        self.q_values
            .get(&StateAction::new(state, action))
            .copied()
            .unwrap_or(self.params.initial_q)
    }

    /// Get number of stored Q-values.
    pub fn len(&self) -> usize {
        self.q_values.len()
    }

    /// Check if table is empty.
    pub fn is_empty(&self) -> bool {
        self.q_values.is_empty()
    }

    /// Export all Q-values for persistence.
    /// Returns (state, action, q_value) tuples.
    pub fn all_q_values(&self) -> Vec<(u64, u64, f64)> {
        self.q_values
            .iter()
            .map(|(sa, &q)| (sa.state, sa.action, q))
            .collect()
    }

    /// Load a single Q-value from persistence.
    /// Rebuilds the state_actions index automatically.
    pub(crate) fn load_q_value(&mut self, state: u64, action: u64, q_value: f64) {
        let sa = StateAction::new(state, action);
        self.q_values.insert(sa, q_value);
        self.state_actions.entry(state).or_default().push(action);
    }

    /// Update Q-table from a hook event (online RL wiring).
    ///
    /// Encodes the event context into state/action space and performs a
    /// single Q-learning update. This is the main entry point for
    /// integrating hook-level quality signals into the RL loop.
    ///
    /// # State encoding
    ///
    /// `state = cila_level * 4 + file_type_idx`
    /// where file_type_idx: python=0, rust=1, typescript=2, other=3
    ///
    /// # Action encoding
    ///
    /// `action = djb2_hash(tool_name) % 64`
    ///
    /// # Reward
    ///
    /// `reward = quality_score / 100.0` (normalized to [0, 1])
    pub fn update_from_hook_event(
        &mut self,
        cila_level: u8,
        file_type: &str,
        tool_name: &str,
        quality_score: f64,
    ) -> f64 {
        let file_type_idx: u64 = match file_type {
            "python" | "py" => 0,
            "rust" | "rs" => 1,
            "typescript" | "ts" => 2,
            _ => 3,
        };
        let state = (cila_level as u64) * 4 + file_type_idx;
        let action = djb2_hash(tool_name) % 64;
        let reward = (quality_score / 100.0).clamp(0.0, 1.0);

        // Terminal single-step update (no next state in hook context)
        self.update(state, action, reward, state, None, true)
    }

    /// Get all Q-values for a state.
    pub fn get_state_q_values(&self, state: u64) -> Vec<(u64, f64)> {
        self.state_actions
            .get(&state)
            .map(|actions| {
                actions
                    .iter()
                    .map(|&a| (a, self.get_q_internal(state, a)))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Update Q-table from granular reward breakdown (5 sub-rewards).
    ///
    /// Computes a weighted total from the breakdown dimensions and delegates
    /// to `update_from_hook_event` for the actual TD(lambda) update.
    /// Also tracks per-dimension statistics for monitoring.
    pub fn update_from_granular_reward(
        &mut self,
        cila_level: u8,
        file_type: &str,
        tool_name: &str,
        breakdown: &RewardBreakdown,
    ) -> f64 {
        let reward = breakdown.weighted_total();
        // Track stats
        self.granular_update_count += 1;
        let values = [
            breakdown.compilation,
            breakdown.lint,
            breakdown.type_safe,
            breakdown.tests,
            breakdown.coverage,
        ];
        for (i, v) in values.iter().enumerate() {
            // SAFETY: `values` has exactly 5 elements (compilation, lint, type_safe, tests, coverage)
            // and reward_sums is initialized with the same fixed size, so i is always in-bounds.
            #[allow(clippy::indexing_slicing)]
            {
                self.reward_sums[i] += v.clamp(0.0, 1.0);
            }
        }
        // Delegate to existing update logic (weighted_total is already in [0,1], scale to [0,100])
        self.update_from_hook_event(cila_level, file_type, tool_name, reward * 100.0)
    }

    /// Get accumulated reward statistics from granular updates.
    pub fn reward_stats(&self) -> RewardStats {
        let n = self.granular_update_count.max(1) as f64;
        RewardStats {
            total_granular_updates: self.granular_update_count,
            avg_compilation: self.reward_sums[0] / n,
            avg_lint: self.reward_sums[1] / n,
            avg_type_safe: self.reward_sums[2] / n,
            avg_tests: self.reward_sums[3] / n,
            avg_coverage: self.reward_sums[4] / n,
        }
    }

    /// Set the exploration rate (epsilon) directly.
    ///
    /// Used by [`super::risk_adjusted::RiskAdjustedQLearning`] to temporarily
    /// modulate epsilon based on blast-radius risk.
    ///
    /// The value is clamped to `[epsilon_min, 1.0]`.
    pub(crate) fn set_epsilon(&mut self, epsilon: f64) {
        self.params.epsilon = epsilon.clamp(self.params.epsilon_min, 1.0);
    }

    /// Access convergence metrics (read-only).
    pub fn metrics(&self) -> &QLearningMetrics {
        &self.metrics
    }

    /// The discount factor this table bootstraps with.
    ///
    /// Exposed so callers that build their own multi-step returns discount with
    /// the SAME gamma the table uses, instead of a private constant of their
    /// own. `OnlineRLEngine` used to hardcode `0.95` while the table ran at
    /// `0.99` — two effective horizons inside one update (04/08/2026).
    pub fn gamma(&self) -> f64 {
        self.params.gamma
    }
}

impl Default for QTable {
    fn default() -> Self {
        Self::new()
    }
}

impl QLearning for QTable {
    fn update(
        &mut self,
        state: u64,
        action: u64,
        reward: f64,
        next_state: u64,
        next_action: Option<u64>,
        terminal: bool,
    ) -> f64 {
        // Ensure state-action is tracked
        self.ensure_q(state, action);

        // Special case for terminal states with no prior Q-value:
        // directly set Q-value to reward.
        let current_q = self.get_q_internal(state, action);
        if terminal && current_q == 0.0 {
            let sa = StateAction::new(state, action);
            self.q_values.insert(sa, reward);
            self.metrics.record(reward, reward);
            return reward;
        }

        // Compute TD error. A terminal transition has no successor, so it MUST
        // NOT bootstrap: `next_q = 0`.
        //
        // Until 04/08/2026 `terminal` was honoured only on the first visit (the
        // branch above, `current_q == 0.0`); every later update bootstrapped off
        // `next_state` regardless. `OnlineRLEngine::process_reward` passes
        // `next_state == state`, so that was a self-loop whose fixed point is
        // `reward / (1 - gamma)` — 100x the reward scale at gamma = 0.99.
        // Simulating this rule with the real constants drove Q to ~655 while
        // rewards are bounded in [-1, 1] (Memento cross-audit, 04/08/2026).
        let current_q = self.get_q_internal(state, action);
        let next_q = if terminal {
            0.0
        } else {
            match next_action {
                // SARSA: use next action Q-value
                Some(na) => {
                    self.ensure_q(next_state, na);
                    self.get_q_internal(next_state, na)
                }
                // Q-learning: use max Q-value
                None => self
                    .best_action(next_state)
                    .map_or(0.0, |best| self.get_q_internal(next_state, best)),
            }
        };

        let td_error = reward + self.params.gamma * next_q - current_q;

        // Update eligibility trace for current state-action
        let sa = StateAction::new(state, action);
        // Replacing traces: set to 1.0
        self.traces.insert(sa, 1.0);

        // Update all Q-values using eligibility traces
        let alpha = self.params.alpha;
        let gamma_lambda = self.params.gamma * self.params.lambda;

        // Collect keys to avoid borrow issues
        let trace_keys: Vec<StateAction> = self.traces.keys().copied().collect();

        for key in trace_keys {
            let trace = self.traces.get(&key).copied().unwrap_or(0.0);
            if trace > 1e-10 {
                // Update Q-value
                let q = self.get_q_internal(key.state, key.action);
                let new_q = q + alpha * td_error * trace;
                self.q_values.insert(key, new_q);

                // Decay trace
                let new_trace = trace * gamma_lambda;
                if new_trace > 1e-10 {
                    self.traces.insert(key, new_trace);
                } else {
                    self.traces.remove(&key);
                }
            }
        }

        // A terminal transition ends the episode, so eligibility traces must not
        // survive into the next one. `process_reward` marks every transition
        // terminal, so keeping them applied one tool's TD error to the Q-values
        // of unrelated tools observed earlier — contamination, not credit
        // assignment.
        if terminal {
            self.traces.clear();
        }

        // Track convergence metrics
        self.metrics.record(td_error, reward);

        td_error
    }

    fn get_q(&self, state: u64, action: u64) -> f64 {
        self.get_q_internal(state, action)
    }

    fn best_action(&self, state: u64) -> Option<u64> {
        self.state_actions.get(&state).and_then(|actions| {
            actions
                .iter()
                .map(|&a| (a, self.get_q_internal(state, a)))
                .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
                .map(|(a, _)| a)
        })
    }

    fn reset_traces(&mut self) {
        self.traces.clear();
    }

    fn get_params(&self) -> LearningParams {
        self.params
    }

    fn epsilon_greedy_action(&mut self, state: u64) -> Option<u64> {
        let actions = self.state_actions.get(&state)?;
        if actions.is_empty() {
            return None;
        }

        let epsilon = self.params.epsilon;

        // Deterministic pseudo-random using state + wall-clock nanos (Knuth multiplicative hash)
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .subsec_nanos() as u64;
        let rand_val = (state.wrapping_mul(2_654_435_761) ^ nanos) % 100;

        let chosen = if rand_val < (epsilon * 100.0) as u64 {
            // Explore: pick a pseudo-random action from known actions
            let idx = (nanos as usize).wrapping_mul(state as usize | 1) % actions.len();
            // SAFETY: idx is computed via `% actions.len()` so it is always < actions.len().
            #[allow(clippy::indexing_slicing)]
            actions[idx]
        } else {
            // Exploit: best action (argmax Q)
            actions
                .iter()
                .map(|&a| (a, self.get_q_internal(state, a)))
                .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
                .map(|(a, _)| a)?
        };

        // Decay epsilon after each call
        self.params.epsilon =
            (self.params.epsilon * self.params.epsilon_decay).max(self.params.epsilon_min);

        Some(chosen)
    }
}

/// DJB2 hash function for tool name -> action ID encoding.
///
/// Classic DJB2 by Daniel J. Bernstein. Produces a well-distributed
/// u64 hash suitable for modular reduction into action space.
pub fn djb2_hash(s: &str) -> u64 {
    let mut hash: u64 = 5381;
    for byte in s.bytes() {
        hash = hash.wrapping_mul(33).wrapping_add(byte as u64);
    }
    hash
}

// ── rkyv Zero-Copy QTable Persistence ────────────────────────────────────

/// Snapshot of QTable learning parameters for rkyv serialization.
#[derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize, Debug, Clone)]
#[rkyv(derive(Debug))]
pub struct LearningParamsSnapshot {
    /// Learning rate (step size) for Q-value updates.
    pub alpha: f64,
    /// Discount factor for future rewards.
    pub gamma: f64,
    /// Eligibility-trace decay factor.
    pub lambda: f64,
    /// Initial Q-value for unseen state-action pairs.
    pub initial_q: f64,
    /// Current exploration rate for epsilon-greedy action selection.
    pub epsilon: f64,
    /// Multiplicative decay applied to `epsilon` over time.
    pub epsilon_decay: f64,
    /// Lower bound below which `epsilon` will not decay.
    pub epsilon_min: f64,
}

/// Snapshot of QTable state for rkyv zero-copy serialization.
///
/// Converts the sparse `HashMap<StateAction, f64>` to a flat `Vec<(u64, u64, f64)>`
/// for efficient serialization. The `state_actions` index is rebuilt on load.
#[derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize, Debug, Clone)]
#[rkyv(derive(Debug))]
pub struct QTableSnapshot {
    /// Q-values as (state, action, value) triples.
    pub q_values: Vec<(u64, u64, f64)>,
    /// Learning parameters.
    pub params: LearningParamsSnapshot,
    /// Monotonic revision counter for optimistic concurrency.
    pub revision: u64,
    /// Granular reward update count.
    pub granular_update_count: u64,
    /// Running sums for each sub-reward dimension [compilation, lint, type_safe, tests, coverage].
    pub reward_sums: [f64; 5],
}

impl QTable {
    /// Create a serializable snapshot of the current QTable state.
    pub fn to_snapshot(&self, revision: u64) -> QTableSnapshot {
        QTableSnapshot {
            q_values: self.all_q_values(),
            params: LearningParamsSnapshot {
                alpha: self.params.alpha,
                gamma: self.params.gamma,
                lambda: self.params.lambda,
                initial_q: self.params.initial_q,
                epsilon: self.params.epsilon,
                epsilon_decay: self.params.epsilon_decay,
                epsilon_min: self.params.epsilon_min,
            },
            revision,
            granular_update_count: self.granular_update_count,
            reward_sums: self.reward_sums,
        }
    }

    /// Restore a QTable from an rkyv snapshot.
    pub fn from_snapshot(snapshot: &QTableSnapshot) -> Self {
        let params = LearningParams {
            alpha: snapshot.params.alpha,
            gamma: snapshot.params.gamma,
            lambda: snapshot.params.lambda,
            initial_q: snapshot.params.initial_q,
            epsilon: snapshot.params.epsilon,
            epsilon_decay: snapshot.params.epsilon_decay,
            epsilon_min: snapshot.params.epsilon_min,
        };
        let mut qt = Self::with_params(params);
        for &(state, action, q_value) in &snapshot.q_values {
            qt.load_q_value(state, action, q_value);
        }
        qt.granular_update_count = snapshot.granular_update_count;
        qt.reward_sums = snapshot.reward_sums;
        qt
    }

    /// Serialize QTable state to an rkyv file.
    pub fn save_rkyv(&self, path: &Path, revision: u64) -> Result<(), QTableError> {
        let snapshot = self.to_snapshot(revision);
        let bytes = touring_rkyv::to_bytes::<_, 4096>(&snapshot)
            .map_err(|e| QTableError::Serialize(e.to_string()))?;
        std::fs::write(path, &bytes).map_err(QTableError::Write)
    }

    /// Load QTable state from an rkyv file with validation.
    ///
    /// Returns `(QTable, revision)`.
    pub fn load_rkyv(path: &Path) -> Result<(Self, u64), QTableError> {
        let bytes = std::fs::read(path).map_err(QTableError::Read)?;
        let archived = touring_rkyv::check_archived_root::<QTableSnapshot>(&bytes)
            .map_err(|e| QTableError::Validate(e.to_string()))?;
        // Deserialize from archived form
        let snapshot: QTableSnapshot = touring_rkyv::deserialize(archived)
            .map_err(|e| QTableError::Deserialize(e.to_string()))?;
        let revision = snapshot.revision;
        Ok((Self::from_snapshot(&snapshot), revision))
    }
}

/// Errors returned by [`QTable`] rkyv persistence (`save_rkyv` / `load_rkyv`).
///
/// Each variant preserves the exact diagnostic message previously returned as a
/// `String`, while `#[source]` on the I/O variants exposes the underlying
/// [`std::io::Error`] for programmatic inspection by consumers.
#[derive(Debug, thiserror::Error)]
pub enum QTableError {
    /// rkyv serialization of the snapshot failed.
    #[error("rkyv serialization failed: {0}")]
    Serialize(String),
    /// Writing the serialized snapshot to disk failed.
    #[error("Failed to write rkyv file: {0}")]
    Write(#[source] std::io::Error),
    /// Reading the rkyv file from disk failed.
    #[error("Failed to read rkyv file: {0}")]
    Read(#[source] std::io::Error),
    /// rkyv archive validation failed (corrupt or incompatible file).
    #[error("rkyv validation failed: {0}")]
    Validate(String),
    /// rkyv deserialization from the archived form failed.
    #[error("rkyv deserialization failed: {0}")]
    Deserialize(String),
}

/// Global QTable singleton for cross-crate sharing.
///
/// Initialized lazily on first access. Thread-safe via `Arc<Mutex<QTable>>`.
#[allow(clippy::type_complexity)]
pub static GLOBAL_QTABLE: std::sync::LazyLock<Arc<Mutex<QTable>>, fn() -> Arc<Mutex<QTable>>> =
    std::sync::LazyLock::new(|| Arc::new(Mutex::new(QTable::new())));

/// Returns a shared reference to the global QTable.
///
/// The returned `Arc<Mutex<QTable>>` can be cloned and shared across
/// crates (touring-cognitive, touring-hooks, touring-server) without
/// duplicating the underlying table.
pub fn global_qtable() -> Arc<Mutex<QTable>> {
    GLOBAL_QTABLE.clone()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::indexing_slicing, unused_must_use)] // vecs asserted non-empty; update() TD error intentionally discarded in setup code
    use super::*;

    #[test]
    fn test_new_qtable() {
        let q = QTable::new();
        assert!(q.is_empty());
        assert_eq!(q.len(), 0);
    }

    #[test]
    fn test_get_q_default() {
        let q = QTable::new();
        assert_eq!(q.get_q(0, 0), 0.0);
        assert_eq!(q.get_q(100, 50), 0.0);
    }

    #[test]
    fn test_q_learning_update() {
        let params = LearningParams {
            alpha: 0.1,
            gamma: 0.9,
            lambda: 0.0,
            initial_q: 0.0,
            ..Default::default()
        };
        let mut q = QTable::with_params(params);

        let td_error = q.update(0, 0, 1.0, 1, None, false);
        assert!((td_error - 1.0).abs() < 1e-10);
        assert!((q.get_q(0, 0) - 0.1).abs() < 1e-10);
    }

    #[test]
    fn test_best_action_selection() {
        let mut q = QTable::new();
        q.update(0, 0, 1.0, 1, None, true);
        q.update(0, 1, 2.0, 1, None, true);
        q.update(0, 2, 0.5, 1, None, true);

        let best = q.best_action(0);
        assert!(best.is_some());
        assert_eq!(best.unwrap(), 1);
    }

    #[test]
    fn test_reset_traces() {
        let params = LearningParams {
            alpha: 0.1,
            gamma: 0.9,
            lambda: 0.9,
            initial_q: 0.0,
            ..Default::default()
        };
        let mut q = QTable::with_params(params);

        q.update(0, 0, 1.0, 1, Some(0), false);
        q.update(1, 0, 1.0, 2, Some(0), false);
        q.reset_traces();

        let q_00_before = q.get_q(0, 0);
        q.update(5, 5, 1.0, 6, None, true);
        let q_00_after = q.get_q(0, 0);
        assert!((q_00_before - q_00_after).abs() < 1e-10);
    }

    #[test]
    fn test_custom_initial_q() {
        let params = LearningParams {
            alpha: 0.1,
            gamma: 0.9,
            lambda: 0.0,
            initial_q: 10.0,
            ..Default::default()
        };
        let q = QTable::with_params(params);
        assert_eq!(q.get_q(0, 0), 10.0);
        assert_eq!(q.get_q(100, 50), 10.0);
    }

    // ── S2.1: Online RL Reward Wiring Tests ──────────────────────────

    #[test]
    fn test_update_from_hook_event_increases_value() {
        let mut q = QTable::new();

        // First update: quality_score=80.0 → reward=0.8
        let td1 = q.update_from_hook_event(2, "python", "Edit", 80.0);
        assert!(td1 > 0.0, "first update should produce positive TD error");

        // State = 2*4 + 0 = 8 (cila=2, python=0)
        let action = djb2_hash("Edit") % 64;
        let q_val = q.get_q(8, action);
        assert!(q_val > 0.0, "Q-value should be positive after reward=0.8");

        // Second update with same context and high quality
        let q_before = q.get_q(8, action);
        q.update_from_hook_event(2, "python", "Edit", 90.0);
        let q_after = q.get_q(8, action);
        assert!(
            q_after >= q_before,
            "Q-value should not decrease with high reward: before={}, after={}",
            q_before,
            q_after
        );
    }

    #[test]
    #[allow(clippy::identity_op, clippy::erasing_op)]
    fn test_file_type_encoding() {
        let mut q = QTable::new();

        // python/py → file_type_idx=0
        q.update_from_hook_event(0, "python", "Read", 50.0);
        let action = djb2_hash("Read") % 64;
        assert!(q.get_q(0 * 4 + 0, action) != 0.0, "python → state offset 0");

        // rust/rs → file_type_idx=1
        q.update_from_hook_event(0, "rust", "Read", 50.0);
        assert!(q.get_q(0 * 4 + 1, action) != 0.0, "rust → state offset 1");

        // Also accept short aliases
        let mut q2 = QTable::new();
        q2.update_from_hook_event(1, "rs", "Bash", 70.0);
        let action_bash = djb2_hash("Bash") % 64;
        assert!(
            q2.get_q(1 * 4 + 1, action_bash) != 0.0,
            "rs alias → state offset 1"
        );

        q2.update_from_hook_event(1, "ts", "Bash", 70.0);
        assert!(
            q2.get_q(1 * 4 + 2, action_bash) != 0.0,
            "ts alias → state offset 2"
        );

        q2.update_from_hook_event(1, "py", "Bash", 70.0);
        assert!(
            q2.get_q(1 * 4 + 0, action_bash) != 0.0,
            "py alias → state offset 0"
        );

        // unknown → file_type_idx=3
        q2.update_from_hook_event(3, "markdown", "Write", 60.0);
        let action_write = djb2_hash("Write") % 64;
        assert!(
            q2.get_q(3 * 4 + 3, action_write) != 0.0,
            "unknown → state offset 3"
        );
    }

    #[test]
    fn test_djb2_hash_deterministic() {
        assert_eq!(djb2_hash("Edit"), djb2_hash("Edit"));
        assert_eq!(djb2_hash("Read"), djb2_hash("Read"));
        assert_ne!(djb2_hash("Edit"), djb2_hash("Read"));
    }

    #[test]
    fn test_djb2_hash_mod64_range() {
        for name in &[
            "Edit", "Read", "Write", "Bash", "Task", "Skill", "Glob", "Grep",
        ] {
            let h = djb2_hash(name) % 64;
            assert!(h < 64, "djb2 % 64 must be < 64, got {} for {}", h, name);
        }
    }

    #[test]
    fn test_update_from_hook_event_clamps_reward() {
        let mut q = QTable::new();

        // quality_score=200 should clamp to reward=1.0
        q.update_from_hook_event(0, "python", "Edit", 200.0);
        let action = djb2_hash("Edit") % 64;
        let q_val = q.get_q(0, action);
        // With terminal + initial_q=0, first update sets Q directly to reward
        assert!(
            q_val <= 1.0 + 1e-10,
            "Q-value should reflect clamped reward <= 1.0, got {}",
            q_val
        );

        // Negative quality_score should clamp to 0.0
        let mut q2 = QTable::new();
        q2.update_from_hook_event(0, "python", "Edit", -50.0);
        let q_val2 = q2.get_q(0, action);
        assert!(
            q_val2 >= -1e-10,
            "Q-value should reflect clamped reward >= 0.0, got {}",
            q_val2
        );
    }

    // ── S0.1: Granular RewardBreakdown Tests ─────────────────────────

    #[test]
    fn test_reward_breakdown_weighted_total() {
        let rb = RewardBreakdown {
            compilation: 1.0,
            lint: 1.0,
            type_safe: 1.0,
            tests: 1.0,
            coverage: 1.0,
        };
        let total = rb.weighted_total();
        assert!((total - 1.0).abs() < 1e-10, "All 1.0 should give total 1.0");

        let rb_zero = RewardBreakdown::default();
        assert!(
            (rb_zero.weighted_total()).abs() < 1e-10,
            "All 0.0 should give total 0.0"
        );

        let rb_partial = RewardBreakdown {
            compilation: 1.0, // weight 0.25
            lint: 0.0,
            type_safe: 0.0,
            tests: 1.0, // weight 0.25
            coverage: 0.0,
        };
        assert!((rb_partial.weighted_total() - 0.50).abs() < 1e-10);
    }

    #[test]
    fn test_granular_reward_updates_qtable() {
        let mut qt = QTable::new();
        let rb = RewardBreakdown {
            compilation: 0.8,
            lint: 0.7,
            type_safe: 0.9,
            tests: 0.6,
            coverage: 0.5,
        };
        let q = qt.update_from_granular_reward(2, "python", "Edit", &rb);
        assert!(q > 0.0, "Q-value should be positive after granular update");
    }

    #[test]
    fn test_backward_compat_f64_reward_still_works() {
        let mut qt = QTable::new();
        // Old API must still work unchanged
        let q = qt.update_from_hook_event(1, "rust", "Bash", 85.0);
        assert!(q > 0.0);
    }

    #[test]
    fn test_reward_stats_accumulates() {
        let mut qt = QTable::new();
        let rb1 = RewardBreakdown {
            compilation: 1.0,
            lint: 0.5,
            type_safe: 0.8,
            tests: 0.9,
            coverage: 0.7,
        };
        let rb2 = RewardBreakdown {
            compilation: 0.6,
            lint: 0.8,
            type_safe: 0.7,
            tests: 0.5,
            coverage: 0.3,
        };
        qt.update_from_granular_reward(1, "python", "Write", &rb1);
        qt.update_from_granular_reward(1, "python", "Write", &rb2);
        let stats = qt.reward_stats();
        assert_eq!(stats.total_granular_updates, 2);
    }

    #[test]
    fn test_reward_breakdown_clamps_values() {
        let rb = RewardBreakdown {
            compilation: 1.5, // above 1.0
            lint: -0.3,       // below 0.0
            type_safe: 0.5,
            tests: 0.5,
            coverage: 0.5,
        };
        let total = rb.weighted_total();
        // After clamping: compilation=1.0(x0.25=0.25), lint=0.0(x0.20=0.0),
        // type_safe=0.5(x0.20=0.10), tests=0.5(x0.25=0.125), coverage=0.5(x0.10=0.05)
        // Total = 0.25 + 0.0 + 0.10 + 0.125 + 0.05 = 0.525
        assert!(
            (total - 0.525).abs() < 1e-10,
            "Clamped values should give 0.525, got {}",
            total
        );
    }

    // ── rkyv Zero-Copy QTable Tests ──────────────────────────────────

    #[test]
    fn test_qtable_rkyv_roundtrip() {
        let mut qt = QTable::new();
        qt.update(0, 1, 0.8, 1, None, true);
        qt.update(2, 3, 0.6, 3, None, true);
        qt.update(5, 10, 1.0, 6, None, true);

        // Add some granular reward data
        let rb = RewardBreakdown {
            compilation: 0.9,
            lint: 0.8,
            type_safe: 0.7,
            tests: 0.6,
            coverage: 0.5,
        };
        qt.update_from_granular_reward(1, "python", "Edit", &rb);

        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("qtable.rkyv");

        qt.save_rkyv(&path, 42).expect("save_rkyv");
        let (loaded, revision) = QTable::load_rkyv(&path).expect("load_rkyv");

        assert_eq!(revision, 42);
        assert_eq!(loaded.len(), qt.len());

        // Verify all Q-values match
        let orig_vals = qt.all_q_values();
        for (state, action, q_val) in &orig_vals {
            let loaded_q = loaded.get_q(*state, *action);
            assert!(
                (loaded_q - q_val).abs() < 1e-10,
                "Q({},{}) mismatch: {} vs {}",
                state,
                action,
                loaded_q,
                q_val,
            );
        }

        // Verify reward stats survived
        let orig_stats = qt.reward_stats();
        let loaded_stats = loaded.reward_stats();
        assert_eq!(
            orig_stats.total_granular_updates,
            loaded_stats.total_granular_updates
        );
        assert!((orig_stats.avg_compilation - loaded_stats.avg_compilation).abs() < 1e-10);
    }

    #[test]
    fn test_qtable_rkyv_empty_state() {
        let qt = QTable::new();
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("empty.rkyv");

        qt.save_rkyv(&path, 0).expect("save empty");
        let (loaded, revision) = QTable::load_rkyv(&path).expect("load empty");

        assert_eq!(revision, 0);
        assert!(loaded.is_empty());
        assert_eq!(loaded.len(), 0);
        assert_eq!(loaded.get_q(0, 0), 0.0);
    }

    #[test]
    fn test_qtable_rkyv_preserves_params() {
        let params = LearningParams {
            alpha: 0.05,
            gamma: 0.95,
            lambda: 0.8,
            initial_q: 5.0,
            epsilon: 0.20,
            epsilon_decay: 0.99,
            epsilon_min: 0.01,
        };
        let qt = QTable::with_params(params);
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("params.rkyv");

        qt.save_rkyv(&path, 1).expect("save");
        let (loaded, _) = QTable::load_rkyv(&path).expect("load");

        let lp = loaded.get_params();
        assert!((lp.alpha - 0.05).abs() < 1e-10);
        assert!((lp.gamma - 0.95).abs() < 1e-10);
        assert!((lp.lambda - 0.8).abs() < 1e-10);
        assert!((lp.initial_q - 5.0).abs() < 1e-10);
        assert!((lp.epsilon - 0.20).abs() < 1e-10);
        assert!((lp.epsilon_decay - 0.99).abs() < 1e-10);
        assert!((lp.epsilon_min - 0.01).abs() < 1e-10);
    }

    #[test]
    fn test_qtable_rkyv_file_not_found() {
        let result = QTable::load_rkyv(Path::new("/nonexistent/path/qtable.rkyv"));
        assert!(result.is_err());
        assert!(
            result.unwrap_err().to_string().contains("Failed to read"),
            "should report file read error"
        );
    }

    // ── Epsilon-Greedy Exploration Tests ──────────────────────────────

    #[test]
    fn test_epsilon_greedy_returns_none_for_unknown_state() {
        let mut q = QTable::new();
        // No actions registered for state 99
        assert!(q.epsilon_greedy_action(99).is_none());
    }

    #[test]
    fn test_epsilon_greedy_returns_action_for_known_state() {
        let mut q = QTable::new();
        q.update(0, 10, 1.0, 1, None, true);
        q.update(0, 20, 0.5, 1, None, true);

        let action = q.epsilon_greedy_action(0);
        assert!(action.is_some());
        let a = action.unwrap();
        // Must be one of the known actions
        assert!(a == 10 || a == 20, "got unexpected action {}", a);
    }

    #[test]
    fn test_epsilon_greedy_decays_epsilon() {
        let params = LearningParams {
            epsilon: 0.50,
            epsilon_decay: 0.90,
            epsilon_min: 0.01,
            ..Default::default()
        };
        let mut q = QTable::with_params(params);
        q.update(0, 1, 1.0, 1, None, true);
        q.update(0, 2, 0.5, 1, None, true);

        let eps_before = q.get_params().epsilon;
        assert!((eps_before - 0.50).abs() < 1e-10);

        q.epsilon_greedy_action(0);
        let eps_after = q.get_params().epsilon;
        // After one call: 0.50 * 0.90 = 0.45
        assert!(
            (eps_after - 0.45).abs() < 1e-10,
            "epsilon should decay to 0.45, got {}",
            eps_after
        );

        q.epsilon_greedy_action(0);
        let eps_after2 = q.get_params().epsilon;
        // After two calls: 0.45 * 0.90 = 0.405
        assert!(
            (eps_after2 - 0.405).abs() < 1e-10,
            "epsilon should decay to 0.405, got {}",
            eps_after2
        );
    }

    #[test]
    fn test_epsilon_greedy_respects_minimum() {
        let params = LearningParams {
            epsilon: 0.06,
            epsilon_decay: 0.50,
            epsilon_min: 0.05,
            ..Default::default()
        };
        let mut q = QTable::with_params(params);
        q.update(0, 1, 1.0, 1, None, true);

        q.epsilon_greedy_action(0);
        let eps = q.get_params().epsilon;
        // 0.06 * 0.50 = 0.03, but clamped to min 0.05
        assert!(
            (eps - 0.05).abs() < 1e-10,
            "epsilon should be clamped to min 0.05, got {}",
            eps
        );
    }

    #[test]
    fn test_epsilon_greedy_default_params() {
        let params = LearningParams::default();
        assert!((params.epsilon - 0.15).abs() < 1e-10);
        assert!((params.epsilon_decay - 0.995).abs() < 1e-10);
        assert!((params.epsilon_min - 0.05).abs() < 1e-10);
    }

    #[test]
    fn test_epsilon_greedy_with_zero_epsilon_is_greedy() {
        let params = LearningParams {
            epsilon: 0.0,
            epsilon_decay: 1.0,
            epsilon_min: 0.0,
            ..Default::default()
        };
        let mut q = QTable::with_params(params);
        q.update(0, 10, 0.1, 1, None, true);
        q.update(0, 20, 0.9, 1, None, true);

        // With epsilon=0, should always pick best action (action 20 has Q=0.9)
        for _ in 0..20 {
            let action = q.epsilon_greedy_action(0).unwrap();
            assert_eq!(action, 20, "with epsilon=0, should always pick best action");
        }
    }

    // ── R5: QLearningMetrics Convergence Tests ──────────────────────────

    #[test]
    fn test_metrics_default_state() {
        let m = QLearningMetrics::default();
        assert_eq!(m.total_updates(), 0);
        assert!((m.td_error_ema() - 0.0).abs() < 1e-10);
        assert!((m.avg_reward() - 0.0).abs() < 1e-10);
        assert!(!m.is_converging());
        assert!(!m.is_diverging());
    }

    #[test]
    fn test_metrics_record_updates_ema() {
        let mut m = QLearningMetrics::new(10);
        m.record(1.0, 0.5);
        assert_eq!(m.total_updates(), 1);
        // EMA after first record: 0.1 * 1.0 + 0.9 * 0.0 = 0.1
        assert!((m.td_error_ema() - 0.1).abs() < 1e-10);
        assert!((m.avg_reward() - 0.5).abs() < 1e-10);
    }

    #[test]
    fn test_metrics_sliding_window_eviction() {
        let mut m = QLearningMetrics::new(3);
        m.record(0.0, 1.0);
        m.record(0.0, 2.0);
        m.record(0.0, 3.0);
        // Window: [1.0, 2.0, 3.0], avg = 2.0
        assert!((m.avg_reward() - 2.0).abs() < 1e-10);

        m.record(0.0, 4.0);
        // Window: [2.0, 3.0, 4.0], avg = 3.0
        assert!((m.avg_reward() - 3.0).abs() < 1e-10);
        assert_eq!(m.total_updates(), 4);
    }

    #[test]
    fn test_metrics_is_converging_after_positive_sequence() {
        let mut m = QLearningMetrics::new(100);
        // Feed 20 small-TD-error, high-reward observations
        for _ in 0..20 {
            m.record(0.01, 0.8);
        }
        assert!(
            m.is_converging(),
            "should converge: td_ema={}, avg_reward={}",
            m.td_error_ema(),
            m.avg_reward()
        );
        assert!(!m.is_diverging());
    }

    #[test]
    fn test_metrics_is_diverging_high_td_error() {
        let mut m = QLearningMetrics::new(100);
        // Feed 20 large-TD-error observations
        for _ in 0..20 {
            m.record(5.0, 0.5);
        }
        assert!(
            m.is_diverging(),
            "should diverge: td_ema={}, avg_reward={}",
            m.td_error_ema(),
            m.avg_reward()
        );
        assert!(!m.is_converging());
    }

    #[test]
    fn test_metrics_is_diverging_negative_reward() {
        let mut m = QLearningMetrics::new(100);
        for _ in 0..20 {
            m.record(0.05, -1.0);
        }
        assert!(
            m.is_diverging(),
            "should diverge with negative reward: avg_reward={}",
            m.avg_reward()
        );
    }

    #[test]
    fn test_metrics_not_converging_before_min_updates() {
        let mut m = QLearningMetrics::new(100);
        // Only 5 updates (< 10 minimum)
        for _ in 0..5 {
            m.record(0.01, 0.9);
        }
        assert!(
            !m.is_converging(),
            "should not converge with fewer than 10 updates"
        );
        assert!(!m.is_diverging());
    }

    #[test]
    fn test_qtable_metrics_tracked_via_update() {
        let mut q = QTable::new();
        assert_eq!(q.metrics().total_updates(), 0);

        // Perform a Q-learning update
        q.update(0, 0, 1.0, 1, None, true);
        assert_eq!(q.metrics().total_updates(), 1);
        assert!(q.metrics().td_error_ema() > 0.0);

        // Multiple updates accumulate
        for _ in 0..9 {
            q.update(0, 0, 0.8, 1, None, true);
        }
        assert_eq!(q.metrics().total_updates(), 10);
        assert!(q.metrics().avg_reward() > 0.0);
    }

    #[test]
    fn test_qtable_metrics_converges_with_consistent_rewards() {
        let params = LearningParams {
            alpha: 0.1,
            gamma: 0.9,
            lambda: 0.0,
            initial_q: 0.0,
            ..Default::default()
        };
        let mut q = QTable::with_params(params);

        // Feed 30 consistent positive-reward terminal updates.
        // TD error shrinks as Q-value converges to reward.
        for _ in 0..30 {
            q.update(0, 0, 0.8, 1, None, true);
        }
        assert!(
            q.metrics().is_converging(),
            "should converge: td_ema={}, avg_reward={}",
            q.metrics().td_error_ema(),
            q.metrics().avg_reward()
        );
    }

    #[test]
    fn test_qtable_metrics_transient_not_in_snapshot() {
        let mut q = QTable::new();
        for _ in 0..15 {
            q.update(0, 0, 0.8, 1, None, true);
        }
        assert!(q.metrics().total_updates() > 0);

        // Snapshot + restore: metrics reset to default (transient)
        let snapshot = q.to_snapshot(1);
        let restored = QTable::from_snapshot(&snapshot);
        assert_eq!(
            restored.metrics().total_updates(),
            0,
            "metrics should be transient and reset on restore"
        );
    }

    /// A terminal transition has no successor, so it must not bootstrap.
    ///
    /// Regression for the 04/08/2026 divergence: `terminal` was honoured only
    /// on the first visit, so every later update added `gamma * next_q`.
    #[test]
    fn terminal_transitions_never_bootstrap() {
        let params = LearningParams {
            alpha: 1.0, // full step so the assertion reads the target directly
            gamma: 0.99,
            lambda: 0.0,
            initial_q: 0.0,
            ..Default::default()
        };
        let mut q = QTable::with_params(params);

        // Seed a large Q on a sibling action of the SAME state, so a bootstrap
        // over `next_state == state` would visibly leak into the TD error.
        q.update(7, 1, 0.0, 7, None, false);
        for _ in 0..50 {
            q.update(7, 1, 10.0, 7, None, false);
        }
        assert!(
            q.get_q(7, 1) > 5.0,
            "sibling action must carry a large Q for this test to discriminate"
        );

        // First visit of (7, 0) takes the warm-start branch; the SECOND is the
        // one that used to bootstrap.
        q.update(7, 0, 1.0, 7, None, true);
        let td = q.update(7, 0, 1.0, 7, None, true);

        assert!(
            td.abs() < 1e-9,
            "terminal target is the reward alone: Q was already 1.0, so td must \
             be ~0, got {td} (a bootstrap off the sibling's Q leaks in here)"
        );
    }

    /// Q-values must stay on the reward scale, never `reward / (1 - gamma)`.
    ///
    /// This is the exact loop `OnlineRLEngine::process_reward` drives —
    /// `next_state == state`, `next_action == None`, `terminal == true`. Before
    /// the fix it converged to 100x the reward at gamma = 0.99 (simulated: Q
    /// reached ~655 for rewards bounded in [-1, 1]).
    #[test]
    fn repeated_terminal_self_loop_updates_stay_on_the_reward_scale() {
        let params = LearningParams {
            alpha: 0.1,
            gamma: 0.99,
            lambda: 0.9,
            initial_q: 0.0,
            ..Default::default()
        };
        let mut q = QTable::with_params(params);

        const REWARD: f64 = 1.0;
        for i in 0..4000u64 {
            q.update(5, i % 3, REWARD, 5, None, true);
        }

        for action in 0..3u64 {
            let value = q.get_q(5, action);
            assert!(
                value.abs() <= REWARD + 1e-6,
                "Q({action}) = {value} exceeded the reward scale; the old \
                 self-loop bootstrap converged to REWARD/(1-gamma) = {}",
                REWARD / (1.0 - 0.99)
            );
        }
    }

    /// Eligibility traces must not survive a terminal transition.
    ///
    /// `process_reward` marks every transition terminal, so surviving traces
    /// applied one tool's TD error to Q-values of unrelated tools seen earlier.
    #[test]
    fn terminal_update_clears_eligibility_traces() {
        let params = LearningParams {
            alpha: 0.5,
            gamma: 0.99,
            lambda: 0.9,
            initial_q: 0.0,
            ..Default::default()
        };
        let mut q = QTable::with_params(params);

        // Establish (1, 1) with a non-zero Q, then close its episode.
        q.update(1, 1, 0.5, 1, None, true);
        q.update(1, 1, 0.5, 1, None, true);
        let before = q.get_q(1, 1);

        // A completely unrelated state-action fires next. Its TD error must not
        // touch (1, 1).
        q.update(99, 99, -1.0, 99, None, true);
        q.update(99, 99, -1.0, 99, None, true);

        assert!(
            (q.get_q(1, 1) - before).abs() < 1e-12,
            "unrelated terminal transition moved Q(1,1) from {before} to {} — \
             traces leaked across episodes",
            q.get_q(1, 1)
        );
    }

    /// Non-terminal transitions must STILL bootstrap — the fix is scoped.
    #[test]
    fn non_terminal_transitions_still_bootstrap() {
        let params = LearningParams {
            alpha: 1.0,
            gamma: 0.5,
            lambda: 0.0,
            initial_q: 0.0,
            ..Default::default()
        };
        let mut q = QTable::with_params(params);

        // Give state 2 a known value, then step into it from state 1.
        q.update(2, 0, 4.0, 2, None, true);
        assert!((q.get_q(2, 0) - 4.0).abs() < 1e-9);

        // The warm-start shortcut is terminal-only, so this non-terminal update
        // goes straight through the TD path: td = 0 + gamma * max_a Q(2,a) - 0.
        let td = q.update(1, 0, 0.0, 2, None, false);

        assert!(
            (td - 2.0).abs() < 1e-9,
            "non-terminal update must bootstrap gamma * Q(2,0) = 2.0, got {td}"
        );
    }
}
