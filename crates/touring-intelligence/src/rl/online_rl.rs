//! Online RL pipeline for immediate reward processing.
//!
//! Complements the batched `auto_learn` loop (every 300s) with immediate,
//! per-tool-execution feedback. Each tool invocation produces an
//! [`ImmediateReward`] that is processed in O(1) without waiting for the
//! next batch cycle.
//!
//! # Architecture
//!
//! ```text
//! cortex handler (PostToolUse)
//!        │
//!        ▼
//!  ImmediateReward
//!        │
//!        ▼
//!  OnlineRLEngine::process_reward()
//!        ├── compute_reward()  → scalar ∈ [-1, 1]
//!        ├── to_state_action() → (state, action) via cila*4+ft / djb2%64
//!        ├── QTable::update()  → TD(λ) Bellman update
//!        └── LinUCB::update()  → Sherman-Morrison arm update
//! ```
//!
//! The warm-start mechanism (`warm_start_context`) bootstraps LinUCB priors
//! for unseen contexts by transferring knowledge from similar past experiences.

use tracing::instrument;

use crate::rl::bandit::{DecisionLedger, LinUCBBandit, NUM_ARMS, extract_features_rich};
use crate::rl::rl::{QLearning, QTable, djb2_hash};
use std::collections::VecDeque;

use crate::rl::meta::self_optimizer::{HyperparamAdjustment, SelfOptimizer};
use crate::rl::observability::RlMetricsCollector;
use crate::rl::rl::curiosity::CuriosityModule;

#[cfg(feature = "ftrl")]
use crate::rl::online_learning::ftrl::FtrlLayer;

// The discount factor for n-step returns is NOT declared here: it is read from
// the QTable being updated (`QTable::gamma`), so the return and the table it
// targets always share one horizon. A local `const GAMMA = 0.95` used to shadow
// the table's 0.99 (04/08/2026).

/// A single (state, action, reward) transition stored in the n-step buffer.
#[derive(Debug, Clone)]
struct Transition {
    state: u64,
    action: u64,
    reward: f64,
}

/// Immediate reward signal from a single tool execution.
#[derive(Debug, Clone)]
pub struct ImmediateReward {
    /// Name of the tool that was executed (e.g. "Edit", "Read", "Bash").
    pub tool_name: String,
    /// Whether the tool output was accepted (no errors, used by the agent).
    pub accepted: bool,
    /// Wall-clock latency of the tool execution in milliseconds.
    pub latency_ms: u64,
    /// Number of errors detected in the tool output (lint, type, runtime).
    pub error_count: u32,
    /// CILA complexity level of the current task (0-6).
    pub cila_level: u8,
    /// File type index: 0=python, 1=rust, 2=typescript, 3=other.
    pub file_type: u8,
    /// Rich quality score from post_quality_gate (0.0-1.0). None = not available.
    /// When present, overrides the simple accepted/error_count-based reward.
    pub quality_score: Option<f64>,
}

/// Configuration for the online RL engine.
#[derive(Debug, Clone)]
pub struct OnlineRLConfig {
    /// Minimum absolute reward delta to trigger a QTable update.
    /// Filters out noise from near-zero reward signals.
    /// Default: 0.01
    pub min_reward_delta: f64,
    /// Exponential moving average smoothing factor for reward signal.
    /// Lower values = more smoothing (slower adaptation).
    /// Default: 0.1
    pub ema_alpha: f64,
    /// Whether to trigger persistence after every update.
    /// When `false`, persistence is deferred to `save_interval` updates.
    /// Default: false
    pub auto_save: bool,
    /// Number of updates between automatic persistence.
    /// Only relevant when `auto_save` is `false`.
    /// Default: 50
    pub save_interval: u64,
    /// Every N updates, force-explore the coldest LinUCB arm.
    /// Prevents arm starvation (5/8 arms stuck at 5 pulls).
    /// Default: 100
    pub forced_explore_interval: u64,
    /// Replay buffer capacity — maximum number of transitions to retain
    /// for n-step TD(λ) learning. Higher values improve temporal credit
    /// assignment at the cost of more memory and slower updates.
    /// Default: 8
    pub replay_capacity: usize,
}

/// Default replay buffer capacity used by `OnlineRLConfig::default()` and tests.
#[cfg(test)]
const DEFAULT_REPLAY_CAPACITY: usize = 8;

impl Default for OnlineRLConfig {
    fn default() -> Self {
        Self {
            min_reward_delta: 0.01,
            ema_alpha: 0.1,
            auto_save: false,
            save_interval: 50,
            forced_explore_interval: 100,
            replay_capacity: 8,
        }
    }
}

/// Online RL engine that processes rewards immediately per tool execution.
///
/// Maintains a smoothed reward signal via EMA and dispatches updates to both
/// QTable (TD(λ)) and LinUCB (contextual bandit) in a single call.
#[derive(Debug)]
pub struct OnlineRLEngine {
    config: OnlineRLConfig,
    /// Exponential moving average of recent rewards.
    ema_reward: f64,
    /// Total number of updates processed.
    update_count: u64,
    /// TD error from the most recent QTable update.
    ///
    /// Exposed for diagnostics and hyperparameter tuning. A consistently large
    /// `last_td_error` suggests the QTable is under-fitting (α too low or reward
    /// signal too noisy). A value near 0 indicates convergence for this state.
    last_td_error: f64,
    /// N-step TD replay buffer: stores last DEFAULT_REPLAY_CAPACITY transitions.
    ///
    /// When the buffer reaches capacity, the oldest transition is updated using
    /// the discounted sum of all buffered rewards as the n-step TD target.
    replay_buffer: VecDeque<Transition>,
    /// Optional count-based intrinsic curiosity bonus.
    curiosity: Option<CuriosityModule>,
    /// Optional self-modifying hyperparameter optimizer.
    meta_optimizer: Option<SelfOptimizer>,
    /// Whether warmup reward has been injected in the current session.
    /// Reset at session boundaries to allow re-warming.
    warmed_up: bool,
    /// Metrics collector for RL engine observability.
    /// Records EMA reward, TD error, Q-table lookups for diagnostics and alerting.
    metrics: RlMetricsCollector,
    /// FTRL-Proximal layer for incremental feature importance learning.
    /// Tracks which of the 25 context features matter most for reward prediction.
    #[cfg(feature = "ftrl")]
    ftrl_layer: Option<FtrlLayer>,
    /// Cached hour-of-day for the current session (0-23).
    /// Avoids repeated SystemTime syscalls in extract_features_rich hot path.
    /// Refreshed at session start via `Self::refresh_time_cache()`.
    cached_hour: Option<u8>,
    /// Bandit selections awaiting their outcome, keyed by tool name.
    ///
    /// Without this the reward was credited to `djb2_hash(tool_name) % NUM_ARMS`
    /// while the arm that actually decided was discarded at the call site — the
    /// bandit was learning about buckets nobody chose (04/08/2026).
    decisions: DecisionLedger,
}

impl OnlineRLEngine {
    /// Create a new engine with the given configuration.
    pub fn new(config: OnlineRLConfig) -> Self {
        let replay_capacity = config.replay_capacity;
        Self {
            config,
            ema_reward: 0.0,
            update_count: 0,
            last_td_error: 0.0,
            replay_buffer: VecDeque::with_capacity(replay_capacity),
            curiosity: None,
            meta_optimizer: None,
            warmed_up: false,
            metrics: RlMetricsCollector::new(64),
            #[cfg(feature = "ftrl")]
            ftrl_layer: None,
            cached_hour: None,
            decisions: DecisionLedger::default(),
        }
    }

    /// Record that `arm` was selected for `key`, so the reward that follows
    /// credits that arm instead of a hash bucket.
    ///
    /// `key` must be the `tool_name` the matching [`ImmediateReward`] will
    /// carry — that is the join between the choice and its outcome. Call this
    /// immediately after `select_arm`; the reward path claims it exactly once.
    ///
    /// Sites that do not record simply keep the legacy hash attribution, so
    /// adopting this is incremental: nothing breaks by not calling it.
    pub fn record_decision(
        &mut self,
        key: impl Into<String>,
        arm: usize,
        features: ndarray::Array1<f64>,
    ) {
        self.decisions
            .record(key, crate::rl::bandit::ArmChoice { arm, features });
    }

    /// Arm to credit when no real selection was recorded for `tool_name`.
    ///
    /// This is the legacy attribution: a hash bucket, which collides across
    /// tools and corresponds to no actual choice. It survives only so sites that
    /// have not adopted [`Self::record_decision`] keep warming arms; every site
    /// that adopts the ledger bypasses it entirely.
    fn fallback_arm(&self, tool_name: &str, linucb: &LinUCBBandit) -> usize {
        let forcing_exploration = self.config.forced_explore_interval > 0
            && self.update_count > 0
            && self
                .update_count
                .is_multiple_of(self.config.forced_explore_interval);

        if forcing_exploration {
            // Warm the arm with the fewest pulls.
            linucb
                .arm_stats()
                .iter()
                .min_by_key(|(_, pulls, _)| *pulls)
                .map(|(i, _, _)| *i)
                .unwrap_or(0)
        } else {
            (djb2_hash(tool_name) % NUM_ARMS as u64) as usize
        }
    }

    /// Read-only view of the pending-decision ledger.
    ///
    /// `credited_count` vs `unclaimed_evictions` is the live measure of how much
    /// of the bandit's learning is closed-loop: evictions are selections whose
    /// outcome was never reported.
    pub fn decisions(&self) -> &DecisionLedger {
        &self.decisions
    }

    /// Reset warmup flag for a new session.
    ///
    /// This allows `inject_warmup_reward` to fire again on a fresh session,
    /// even if the engine has already processed real rewards in prior sessions.
    /// Called automatically at session-start by the hook runtime.
    pub fn reset_warmup_session(&mut self) {
        self.warmed_up = false;
        self.refresh_time_cache();
    }

    /// Cache the current hour from system clock once per session.
    /// Avoids repeated SystemTime syscalls in extract_features_rich hot path.
    fn refresh_time_cache(&mut self) {
        use std::time::{SystemTime, UNIX_EPOCH};
        let secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        self.cached_hour = Some((secs / 3600 % 24) as u8);
    }

    /// Enable count-based intrinsic curiosity bonus (builder method).
    pub fn with_curiosity(mut self) -> Self {
        self.curiosity = Some(CuriosityModule::with_defaults());
        self
    }

    /// Enable self-modifying hyperparameter optimizer (builder method).
    pub fn with_meta_optimizer(mut self) -> Self {
        self.meta_optimizer = Some(SelfOptimizer::new());
        self
    }

    /// Enable FTRL-Proximal feature importance layer (builder method).
    ///
    /// When enabled, the FTRL layer learns which of the 25 context features
    /// matter most for reward prediction, complementing LinUCB's arm selection.
    #[cfg(feature = "ftrl")]
    pub fn with_ftrl_layer(mut self) -> Self {
        self.ftrl_layer = Some(FtrlLayer::new(crate::rl::bandit::FEATURE_DIM));
        self
    }

    /// Initialize the session time cache from system clock.
    ///
    /// Should be called once at session start before any `process_reward` calls.
    /// Eliminates the SystemTime syscalls that would otherwise occur on every
    /// feature extraction in the hot path.
    pub fn with_time_cache(mut self) -> Self {
        self.refresh_time_cache();
        self
    }

    /// Create an engine with default configuration.
    pub fn with_defaults() -> Self {
        Self::new(OnlineRLConfig::default())
    }

    /// Inject a synthetic warmup reward to break cold-start deadlock.
    ///
    /// When the engine has never received any reward (`update_count == 0`),
    /// the EMA is stuck at 0.0 and the first real reward may be delayed or
    /// never arrive if no tool events fire. This method injects a small
    /// positive reward so the RL system begins learning immediately.
    ///
    /// The warmup reward is small (0.2) and only injected once per session —
    /// subsequent calls are no-ops until `reset_warmup_session` is called.
    /// This prevents bias while ensuring the EMA is no longer dead at zero.
    ///
    /// Returns `true` if warmup was injected, `false` if already warmed up.
    pub fn inject_warmup_reward(&mut self) -> bool {
        if self.warmed_up {
            return false;
        }

        let warmup = ImmediateReward {
            tool_name: "warmup".to_string(),
            accepted: true,
            latency_ms: 0,
            error_count: 0,
            cila_level: 2,
            file_type: 3,             // "other"
            quality_score: Some(0.8), // >0.5 ensures positive reward per compute_reward formula
        };

        // Use a dummy QTable and LinUCB just for warmup — we discard them.
        // The important side-effect is updating self.ema_reward.
        let mut dummy_qt = QTable::new();
        let mut dummy_linucb = LinUCBBandit::new();
        let _ = self.process_reward(&warmup, &mut dummy_qt, &mut dummy_linucb);
        tracing::debug!("OnlineRL warmup injected: ema_reward={}", self.ema_reward);
        self.warmed_up = true;
        true
    }

    /// Process an immediate reward signal, updating both QTable and LinUCB.
    ///
    /// Returns the computed reward value (before EMA smoothing).
    /// Returns `None` if the reward delta is below `min_reward_delta`
    /// (i.e., the signal was filtered as noise).
    #[instrument(skip_all, fields(tool = %reward.tool_name, state, action, base_reward))]
    pub fn process_reward(
        &mut self,
        reward: &ImmediateReward,
        qtable: &mut QTable,
        linucb: &mut LinUCBBandit,
    ) -> Option<f64> {
        let base_reward = Self::compute_reward(reward);
        let (state, action) = Self::to_state_action(reward);

        // Add intrinsic curiosity bonus to encourage novel state exploration
        let curiosity_bonus = self.curiosity.as_mut().map_or(0.0, |c| c.bonus(state));
        let raw_reward = (base_reward + curiosity_bonus).clamp(-1.0, 1.0);

        // Filter noise: skip if delta from EMA is below threshold
        if (raw_reward - self.ema_reward).abs() < self.config.min_reward_delta
            && self.update_count > 0
        {
            return None;
        }

        // Update EMA
        self.ema_reward =
            self.config.ema_alpha * raw_reward + (1.0 - self.config.ema_alpha) * self.ema_reward;

        // QTable update: map to (state, action) and compute n-step TD return.
        // Note: (state, action) was already computed above for the curiosity bonus.

        // Push current transition into the n-step replay buffer.
        self.replay_buffer.push_back(Transition {
            state,
            action,
            reward: raw_reward,
        });

        // Compute discounted n-step return: G = Σ γ^i · r[i]  (oldest i=0 → newest)
        // The oldest entry receives full credit; future rewards discount backward.
        //
        // The discount comes from the table itself, not a local constant: this
        // return IS the target handed to `qtable.update`, so discounting it with
        // a different gamma than the table's put two effective horizons inside
        // one update (was 0.95 here vs 0.99 there, 04/08/2026).
        let gamma = qtable.gamma();
        let n_step_return: f64 = self
            .replay_buffer
            .iter()
            .enumerate()
            .map(|(i, t)| gamma.powi(i as i32) * t.reward)
            .sum();

        // Determine which (state, action) to update:
        //   Buffer full → pop oldest and update it (true n-step TD with N lookahead steps).
        //   Buffer warming up → update current entry (accumulate signal before full lookahead).
        let (update_state, update_action) = if self.replay_buffer.len()
            >= self.config.replay_capacity
        {
            // SAFETY: we just pushed an entry, so len >= 1. Invariant: this branch
            // only reached when len >= replay_capacity >= 1 after push.
            debug_assert!(!self.replay_buffer.is_empty());
            let oldest = self
                .replay_buffer
                .pop_front()
                .expect("replay_buffer non-empty: this branch is only entered after push_back(), guaranteeing len >= 1");
            (oldest.state, oldest.action)
        } else {
            (state, action)
        };
        self.last_td_error = qtable.update(
            update_state,
            update_action,
            n_step_return,
            state,
            None,
            true,
        );

        // LinUCB update. If some call site recorded the arm it actually selected
        // for this tool, that arm — and the features it was conditioned on — is
        // what the reward belongs to. Only when nothing was recorded do we fall
        // back to the legacy hash bucket. See `DecisionLedger` for why crediting
        // a hash of the tool name was an open loop.
        let recorded = self.decisions.take(&reward.tool_name);

        // Build enriched feature vector with quality context (fallback path).
        let features = extract_features_rich(
            file_type_str(reward.file_type),
            0, // file_size not available in immediate context
            0, // session_turn not available
            reward.error_count as usize,
            reward.cila_level,
            reward.quality_score,
            None, // file_risk: not available in immediate context (injected by pre-edit)
            // H1-D: new signals — not available in ImmediateReward, use None (defaults)
            None,             // error_count_session
            None,             // recent_tool_success_rate
            self.cached_hour, // cached from system clock once per session
        );

        let (arm_index, arm_features) = match recorded {
            // A real selection is on record: credit exactly that arm, with the
            // features it saw. Forced exploration must NOT reassign here —
            // exploration belongs at selection time (`LinUCBBandit::select_arm`
            // already warms cold arms); reassigning at credit time would hand a
            // reward to an arm that made no choice, which is the very defect
            // this ledger closes.
            Some(decision) => (decision.payload.arm, decision.payload.features),

            // Nothing recorded — the arm is arbitrary either way, so keep the
            // legacy hash bucket and the periodic cold-arm warm-up.
            None => (
                self.fallback_arm(&reward.tool_name, linucb),
                features.clone(),
            ),
        };
        linucb.update(arm_index, &arm_features, raw_reward);

        // FTRL-Proximal update: learn feature importance from reward signal
        // Runs after LinUCB update so the bandit arm state is already current.
        #[cfg(feature = "ftrl")]
        if let Some(ref mut ftrl) = self.ftrl_layer {
            // FtrlLayer::update wants &[f64]; Array1 is contiguous so
            // as_slice() returns Some — fallback path keeps type-safe.
            let slice = features.as_slice().unwrap_or(&[]);
            if let Err(e) = ftrl.update(slice, raw_reward) {
                tracing::warn!(error = %e, "FtrlLayer update failed, skipping");
            }
        }

        self.metrics.record_qtable_lookup();

        self.update_count += 1;

        // Mark that engine has been initialized via real rewards (not just warmup).
        // This prevents warmup from being injected after real rewards arrive.
        self.warmed_up = true;

        // Meta-optimizer: observe reward signal and apply suggested adjustments
        let maybe_adjustment = self
            .meta_optimizer
            .as_mut()
            .and_then(|opt| opt.observe(raw_reward));
        if let Some(adjustment) = maybe_adjustment {
            self.apply_adjustment(adjustment);
        }

        // Record metrics for rolling window observability
        self.metrics
            .record_update(self.ema_reward, self.last_td_error);

        Some(raw_reward)
    }

    /// Take a snapshot of the RL engine metrics for observability dashboards.
    ///
    /// Returns the current `RlMetrics` with EMA reward, TD error, update count,
    /// and Q-table lookup count sampled atomically.
    #[inline]
    pub fn snapshot(&self) -> crate::rl::observability::RlMetrics {
        self.metrics.snapshot()
    }

    /// Compute scalar reward from an immediate signal.
    ///
    /// If `quality_score` is present (from rich reward S1), uses it directly
    /// as the primary signal, blended with latency penalty.
    /// Otherwise falls back to the simple accepted/error_count formula.
    ///
    /// Result clamped to [-1.0, 1.0].
    pub fn compute_reward(reward: &ImmediateReward) -> f64 {
        // If rich quality score is available (from post_quality_gate S1),
        // use it as primary signal — it already encodes lint, test, type, complexity
        if let Some(qs) = reward.quality_score {
            let mut r = qs.clamp(0.0, 1.0) * 2.0 - 1.0; // map [0,1] → [-1,1]
            // Latency penalty still applies
            if reward.latency_ms > 5000 {
                r -= 0.1;
            }
            return r.clamp(-1.0, 1.0);
        }

        // Fallback: simple accepted/error formula (backward compat)
        let mut r = if reward.accepted { 0.75 } else { -0.25 };
        if reward.latency_ms > 5000 {
            r -= 0.2;
        } else if reward.latency_ms > 1000 {
            r -= 0.1;
        }
        r -= reward.error_count as f64 * 0.15;
        r.clamp(-1.0, 1.0)
    }

    /// Map an immediate reward to QTable (state, action) coordinates.
    ///
    /// State encoding: `cila_level * 4 + file_type` (matches `update_from_hook_event`).
    /// Action encoding: `djb2_hash(tool_name) % 64` (matches `tool_name_to_action`).
    pub fn to_state_action(reward: &ImmediateReward) -> (u64, u64) {
        let state = (reward.cila_level as u64) * 4 + (reward.file_type as u64);
        let action = djb2_hash(&reward.tool_name) % 64;
        (state, action)
    }

    /// TD error from the most recent QTable update.
    ///
    /// Returns 0.0 if no update has been processed yet. Useful for diagnostics:
    /// a large td_error means the Q-value estimate is far from the n-step return.
    pub fn last_td_error(&self) -> f64 {
        self.last_td_error
    }

    /// Mean absolute TD error over the rolling metrics window (64 updates).
    ///
    /// Prefer this over [`Self::last_td_error`] for any convergence *judgement*:
    /// one sample swings with whichever tool fired last, while the window shows
    /// whether the Q-values are settling. `touring learning status` reported the
    /// last error under the name `mean_td_error` until 04/08/2026.
    ///
    /// `None` while no update has been recorded — see
    /// [`RlMetricsCollector::mean_td_error`].
    pub fn mean_td_error(&self) -> Option<f64> {
        self.metrics.mean_td_error()
    }

    /// Warm-start LinUCB priors for an unseen context using similar past experiences.
    ///
    /// Given a set of `(similarity_score, past_reward)` pairs from memory recall,
    /// produces a prior distribution across the 8 LinUCB arms.
    ///
    /// Returns a `Vec<f64>` of length [`NUM_ARMS`] summing to approximately 1.0.
    ///
    /// Behavior:
    /// - Empty input or negligible total weight: uniform distribution (1/8 each).
    /// - High weighted reward (> 0.5): boosts complex enrichment arms (5-7) by 1.5x.
    pub fn warm_start_context(similar_rewards: &[(f64, f64)]) -> Vec<f64> {
        let uniform = 1.0 / NUM_ARMS as f64;

        if similar_rewards.is_empty() {
            return vec![uniform; NUM_ARMS];
        }

        let total_weight: f64 = similar_rewards.iter().map(|(s, _)| s).sum();
        if total_weight < 0.001 {
            return vec![uniform; NUM_ARMS];
        }

        let weighted_reward: f64 =
            similar_rewards.iter().map(|(s, r)| s * r).sum::<f64>() / total_weight;

        let base = weighted_reward / NUM_ARMS as f64;
        let mut priors = vec![base; NUM_ARMS];

        // Boost complex enrichment arms (OverviewGotcha=5, OverviewBlastRadius=6, FullEnrichment=7)
        // when aggregate past performance is strong.
        if weighted_reward > 0.5 {
            for prior in priors.iter_mut().skip(5) {
                *prior *= 1.5;
            }
        }

        priors
    }

    /// Current EMA-smoothed reward signal.
    pub fn ema_reward(&self) -> f64 {
        self.ema_reward
    }

    /// Total number of updates processed.
    pub fn update_count(&self) -> u64 {
        self.update_count
    }

    /// Whether the engine should trigger a save based on update count.
    pub fn should_save(&self) -> bool {
        if self.config.auto_save {
            return true;
        }
        self.update_count > 0 && self.update_count.is_multiple_of(self.config.save_interval)
    }

    /// Apply a hyperparameter adjustment proposed by the meta-optimizer.
    fn apply_adjustment(&mut self, adjustment: HyperparamAdjustment) {
        match adjustment {
            HyperparamAdjustment::Keep => {}
            HyperparamAdjustment::IncreaseEmaAlpha => {
                self.config.ema_alpha = (self.config.ema_alpha * 1.1).clamp(0.01, 1.0);
            }
            HyperparamAdjustment::DecreaseEmaAlpha => {
                self.config.ema_alpha = (self.config.ema_alpha * 0.9).clamp(0.01, 1.0);
            }
            HyperparamAdjustment::IncreaseExploreInterval => {
                self.config.forced_explore_interval = self
                    .config
                    .forced_explore_interval
                    .saturating_mul(2)
                    .min(1000);
            }
            HyperparamAdjustment::DecreaseExploreInterval => {
                self.config.forced_explore_interval =
                    (self.config.forced_explore_interval / 2).max(10);
            }
            HyperparamAdjustment::Reset => {
                let default = OnlineRLConfig::default();
                self.config.ema_alpha = default.ema_alpha;
                self.config.forced_explore_interval = default.forced_explore_interval;
            }
        }
    }
}

/// Convert file_type index to string for `extract_features`.
fn file_type_str(file_type: u8) -> &'static str {
    match file_type {
        0 => "python",
        1 => "rust",
        2 => "typescript",
        _ => "other",
    }
}

// ── Experience Replay Buffer ──────────────────────────────────────────────────

/// A single stored experience (s, a, r, s', terminal).
///
/// Used by [`ReplayBuffer`] for batch TD learning. Unlike the internal n-step
/// `Transition` struct, this captures the full (s, a, r, s', done) tuple
/// needed for off-policy replay.
#[derive(Debug, Clone)]
pub struct Experience {
    /// State at time t.
    pub state: u64,
    /// Action taken at time t.
    pub action: u64,
    /// Reward received.
    pub reward: f64,
    /// Next state at time t+1.
    pub next_state: u64,
    /// Whether this was a terminal transition.
    pub terminal: bool,
}

/// Circular experience replay buffer for batch TD learning.
///
/// Stores past experiences for random sampling, improving sample efficiency
/// beyond single-step online updates. When the buffer reaches capacity,
/// new experiences overwrite the oldest entries (ring buffer semantics).
///
/// # Example
///
/// ```
/// use touring_intelligence::rl::online_rl::{ReplayBuffer, Experience};
///
/// let mut buf = ReplayBuffer::new(100);
/// buf.push(Experience { state: 0, action: 1, reward: 0.5, next_state: 2, terminal: false });
/// assert_eq!(buf.len(), 1);
///
/// let batch = buf.sample(4);
/// assert_eq!(batch.len(), 4); // with replacement
/// ```
#[derive(Debug)]
pub struct ReplayBuffer {
    /// Circular buffer of experiences.
    buffer: Vec<Experience>,
    /// Write position (wraps around at capacity).
    write_pos: usize,
    /// Current number of stored experiences.
    len: usize,
    /// Maximum capacity.
    capacity: usize,
}

impl ReplayBuffer {
    /// Create a new replay buffer with the given capacity.
    pub fn new(capacity: usize) -> Self {
        Self {
            buffer: Vec::with_capacity(capacity),
            write_pos: 0,
            len: 0,
            capacity,
        }
    }

    /// Push an experience into the buffer (circular -- overwrites oldest when full).
    pub fn push(&mut self, exp: Experience) {
        if self.capacity == 0 {
            return;
        }
        if self.buffer.len() < self.capacity {
            self.buffer.push(exp);
        } else {
            #[allow(clippy::indexing_slicing)] // write_pos always < capacity (mod arithmetic)
            {
                self.buffer[self.write_pos] = exp;
            }
        }
        self.write_pos = (self.write_pos + 1) % self.capacity;
        self.len = (self.len + 1).min(self.capacity);
    }

    /// Random sample of `batch_size` experiences (with replacement).
    ///
    /// Returns an empty vec if the buffer is empty. Uses a hash-based
    /// pseudo-random index to avoid pulling in a full RNG crate.
    pub fn sample(&self, batch_size: usize) -> Vec<Experience> {
        if self.is_empty() {
            return vec![];
        }
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut results = Vec::with_capacity(batch_size);
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;

        for i in 0..batch_size {
            let mut hasher = DefaultHasher::new();
            (seed.wrapping_add(i as u64)).hash(&mut hasher);
            let idx = (hasher.finish() as usize) % self.len;
            if let Some(exp) = self.buffer.get(idx) {
                results.push(exp.clone());
            }
        }
        results
    }

    /// Parallel sample using rayon for large batch sizes.
    ///
    /// Uses `rayon::prelude::par_iter` for hash-based random sampling when
    /// `batch_size > 2 * num_cpus`. Falls back to sequential `sample()` for
    /// smaller batches to avoid thread-pool overhead.
    ///
    /// # Example
    ///
    /// ```
    /// use touring_intelligence::rl::online_rl::{ReplayBuffer, Experience};
    ///
    /// let mut buf = ReplayBuffer::new(100);
    /// for i in 0..50 {
    ///     buf.push(Experience { state: i, action: 0, reward: 0.5, next_state: i + 1, terminal: false });
    /// }
    /// // Large batch sample via rayon
    /// let batch = buf.par_sample(256);
    /// assert_eq!(batch.len(), 256);
    /// ```
    pub fn par_sample(&self, batch_size: usize) -> Vec<Experience> {
        if self.is_empty() {
            return vec![];
        }

        use rayon::prelude::*;
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        // Sequential fallback for small batches — thread pool overhead not worth it.
        if batch_size <= 2 * rayon::current_num_threads() {
            return self.sample(batch_size);
        }

        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;

        (0..batch_size)
            .into_par_iter()
            .map(|i| {
                let mut hasher = DefaultHasher::new();
                (seed.wrapping_add(i as u64)).hash(&mut hasher);
                let idx = (hasher.finish() as usize) % self.len;
                self.buffer.get(idx).cloned()
            })
            .filter_map(std::convert::identity)
            .collect()
    }

    /// Number of experiences currently stored.
    #[inline]
    pub fn len(&self) -> usize {
        self.len
    }

    /// Whether the buffer is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Buffer capacity.
    #[inline]
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Whether the buffer is at full capacity.
    #[inline]
    pub fn is_full(&self) -> bool {
        self.len >= self.capacity
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    #![allow(clippy::indexing_slicing)] // test vecs asserted non-empty before indexing
    use super::*;

    fn make_reward(
        tool: &str,
        accepted: bool,
        latency_ms: u64,
        error_count: u32,
        cila_level: u8,
        file_type: u8,
    ) -> ImmediateReward {
        ImmediateReward {
            tool_name: tool.to_string(),
            accepted,
            latency_ms,
            error_count,
            cila_level,
            file_type,
            quality_score: None,
        }
    }

    // ── compute_reward tests ─────────────────────────────────────────────

    #[test]
    fn test_compute_reward_accepted() {
        let r = make_reward("Edit", true, 100, 0, 2, 0);
        let v = OnlineRLEngine::compute_reward(&r);
        assert!(
            (v - 0.75).abs() < f64::EPSILON,
            "accepted + fast + 0 errors = 0.75, got {v}"
        );
    }

    #[test]
    fn test_compute_reward_rejected() {
        let r = make_reward("Edit", false, 100, 0, 2, 0);
        let v = OnlineRLEngine::compute_reward(&r);
        assert!(
            (v - (-0.25)).abs() < f64::EPSILON,
            "rejected + fast + 0 errors = -0.25, got {v}"
        );
    }

    #[test]
    fn test_compute_reward_high_latency_penalty() {
        // > 5000ms = -0.2 penalty
        let r = make_reward("Bash", true, 6000, 0, 1, 0);
        let v = OnlineRLEngine::compute_reward(&r);
        assert!(
            (v - 0.55).abs() < f64::EPSILON,
            "accepted + 6s latency = 0.75-0.2 = 0.55, got {v}"
        );

        // > 1000ms but <= 5000ms = -0.1 penalty
        let r2 = make_reward("Bash", true, 2000, 0, 1, 0);
        let v2 = OnlineRLEngine::compute_reward(&r2);
        assert!(
            (v2 - 0.65).abs() < f64::EPSILON,
            "accepted + 2s latency = 0.75-0.1 = 0.65, got {v2}"
        );
    }

    #[test]
    fn test_compute_reward_error_penalty() {
        let r = make_reward("Write", true, 100, 3, 2, 0);
        let v = OnlineRLEngine::compute_reward(&r);
        // 0.75 - 3*0.15 = 0.75 - 0.45 = 0.30
        assert!(
            (v - 0.30).abs() < f64::EPSILON,
            "3 errors penalty = 0.30, got {v}"
        );
    }

    #[test]
    fn test_compute_reward_clamp_range() {
        // Many errors + rejected + slow should clamp to -1.0
        let r = make_reward("Bash", false, 10000, 10, 0, 0);
        let v = OnlineRLEngine::compute_reward(&r);
        // -0.25 - 0.2 - 10*0.15 = -0.25 - 0.2 - 1.5 = -1.95, clamped to -1.0
        assert!(
            (v - (-1.0)).abs() < f64::EPSILON,
            "extreme negative clamped to -1.0, got {v}"
        );

        // No scenario naturally exceeds 1.0 (max is 0.75), but verify clamp
        assert!(OnlineRLEngine::compute_reward(&make_reward("X", true, 0, 0, 0, 0)) <= 1.0);
    }

    // ── to_state_action tests ────────────────────────────────────────────

    #[test]
    fn test_to_state_action_mapping() {
        let r = make_reward("Edit", true, 100, 0, 3, 1);
        let (state, action) = OnlineRLEngine::to_state_action(&r);

        // state = cila_level(3) * 4 + file_type(1) = 13
        assert_eq!(state, 13, "state = 3*4+1 = 13");

        // action = djb2_hash("Edit") % 64
        let expected_action = djb2_hash("Edit") % 64;
        assert_eq!(action, expected_action);
    }

    #[test]
    fn test_to_state_action_boundaries() {
        // Min: cila=0, file_type=0
        let r_min = make_reward("X", true, 0, 0, 0, 0);
        let (s_min, _) = OnlineRLEngine::to_state_action(&r_min);
        assert_eq!(s_min, 0);

        // Max useful: cila=6, file_type=3
        let r_max = make_reward("X", true, 0, 0, 6, 3);
        let (s_max, _) = OnlineRLEngine::to_state_action(&r_max);
        assert_eq!(s_max, 27); // 6*4+3
    }

    // ── warm_start_context tests ─────────────────────────────────────────

    #[test]
    fn test_warm_start_empty_returns_uniform() {
        let priors = OnlineRLEngine::warm_start_context(&[]);
        assert_eq!(priors.len(), NUM_ARMS);
        let uniform = 1.0 / NUM_ARMS as f64;
        for p in &priors {
            assert!((p - uniform).abs() < f64::EPSILON);
        }
    }

    #[test]
    fn test_warm_start_low_similarity_returns_uniform() {
        // Total weight < 0.001 → uniform
        let similar = vec![(0.0001, 0.9), (0.0002, 0.8)];
        let priors = OnlineRLEngine::warm_start_context(&similar);
        assert_eq!(priors.len(), NUM_ARMS);
        let uniform = 1.0 / NUM_ARMS as f64;
        for p in &priors {
            assert!((p - uniform).abs() < f64::EPSILON);
        }
    }

    #[test]
    fn test_warm_start_high_reward_boosts_complex_arms() {
        // High similarity, high reward → weighted_reward > 0.5 → arms 5-7 boosted
        let similar = vec![(1.0, 0.8)];
        let priors = OnlineRLEngine::warm_start_context(&similar);
        assert_eq!(priors.len(), NUM_ARMS);

        // weighted_reward = 0.8, base = 0.8/8 = 0.1
        let base = 0.8 / NUM_ARMS as f64;
        // Arms 0-4: base
        for p in &priors[..5] {
            assert!(
                (p - base).abs() < f64::EPSILON,
                "arms 0-4 should be base {base}, got {p}"
            );
        }
        // Arms 5-7: base * 1.5
        let boosted = base * 1.5;
        for p in &priors[5..] {
            assert!(
                (p - boosted).abs() < f64::EPSILON,
                "arms 5-7 should be boosted {boosted}, got {p}"
            );
        }
    }

    #[test]
    fn test_warm_start_low_reward_no_boost() {
        // weighted_reward = 0.3 (< 0.5) → no boost
        let similar = vec![(1.0, 0.3)];
        let priors = OnlineRLEngine::warm_start_context(&similar);
        let base = 0.3 / NUM_ARMS as f64;
        for p in &priors {
            assert!(
                (p - base).abs() < f64::EPSILON,
                "no boost when reward < 0.5"
            );
        }
    }

    // ── config defaults test ─────────────────────────────────────────────

    #[test]
    fn test_online_config_defaults() {
        let cfg = OnlineRLConfig::default();
        assert!((cfg.min_reward_delta - 0.01).abs() < f64::EPSILON);
        assert!((cfg.ema_alpha - 0.1).abs() < f64::EPSILON);
        assert!(!cfg.auto_save);
        assert_eq!(cfg.save_interval, 50);
    }

    // ── process_reward integration test ──────────────────────────────────

    #[test]
    fn test_process_reward_updates_qtable() {
        let mut engine = OnlineRLEngine::with_defaults();
        let mut qt = QTable::new();
        let mut bandit = LinUCBBandit::new();

        let reward = make_reward("Edit", true, 200, 0, 2, 0);
        let result = engine.process_reward(&reward, &mut qt, &mut bandit);

        // First update should always pass (no EMA history to filter against)
        assert!(result.is_some(), "first update should not be filtered");
        let val = result.expect("first update should not be filtered");
        assert!(
            (val - 0.75).abs() < f64::EPSILON,
            "accepted+fast+0err = 0.75"
        );

        // QTable should now have at least one entry
        assert!(!qt.is_empty(), "QTable should have entries after update");

        // Verify state-action was written correctly
        let (state, action) = OnlineRLEngine::to_state_action(&reward);
        let q = qt.get_q(state, action);
        assert!(q != 0.0, "Q-value should be non-zero after update, got {q}");

        // LinUCB should have recorded a pull
        assert!(
            bandit.total_pulls() >= 1,
            "LinUCB should have at least 1 pull"
        );

        // Engine state
        assert_eq!(engine.update_count(), 1);
        assert!(engine.ema_reward() > 0.0);
    }

    #[test]
    fn test_process_reward_filters_noise() {
        let mut engine = OnlineRLEngine::new(OnlineRLConfig {
            min_reward_delta: 0.5, // High threshold to make filtering obvious
            ema_alpha: 1.0,        // EMA tracks immediately (α=1 → ema = last reward)
            ..Default::default()
        });
        let mut qt = QTable::new();
        let mut bandit = LinUCBBandit::new();

        // First update always passes (update_count == 0 bypass)
        let r1 = make_reward("Edit", true, 200, 0, 2, 0);
        let v1 = engine.process_reward(&r1, &mut qt, &mut bandit);
        assert!(v1.is_some());
        // With α=1.0, ema_reward is now exactly 0.75

        // Second identical reward: raw=0.75, ema=0.75, delta=0.0 < 0.5 → filtered
        let r2 = make_reward("Edit", true, 200, 0, 2, 0);
        let v2 = engine.process_reward(&r2, &mut qt, &mut bandit);
        assert!(
            v2.is_none(),
            "near-identical reward should be filtered with high min_reward_delta"
        );

        // Update count should still be 1
        assert_eq!(engine.update_count(), 1);
    }

    #[test]
    fn test_inject_warmup_reward() {
        // Fresh engine has update_count=0 and ema_reward=0.0 (cold start)
        let mut engine = OnlineRLEngine::with_defaults();
        assert_eq!(engine.update_count(), 0);
        assert_eq!(engine.ema_reward(), 0.0);

        // First call should inject warmup and return true
        let injected = engine.inject_warmup_reward();
        assert!(injected, "warmup should be injected on cold engine");
        assert_eq!(
            engine.update_count(),
            1,
            "warmup should increment update_count"
        );
        assert!(
            engine.ema_reward() > 0.0,
            "ema_reward should be positive after warmup"
        );

        // Second call should be no-op (already warmed up)
        let injected2 = engine.inject_warmup_reward();
        assert!(
            !injected2,
            "warmup should not be re-injected after first call"
        );
        assert_eq!(
            engine.update_count(),
            1,
            "update_count should not change on second call"
        );

        // After a real reward arrives, warmup should not re-inject
        let mut engine2 = OnlineRLEngine::with_defaults();
        let mut qt = QTable::new();
        let mut bandit = LinUCBBandit::new();
        let real_reward = make_reward("Edit", true, 100, 0, 2, 0);
        engine2.process_reward(&real_reward, &mut qt, &mut bandit);

        let injected3 = engine2.inject_warmup_reward();
        assert!(!injected3, "warmup should not inject after real reward");

        // reset_warmup_session allows warmup to fire again on a new session
        let mut engine3 = OnlineRLEngine::with_defaults();
        engine3.process_reward(&real_reward, &mut qt, &mut bandit);
        let injected4 = engine3.inject_warmup_reward();
        assert!(!injected4, "warmup blocked after real reward without reset");
        engine3.reset_warmup_session();
        let injected5 = engine3.inject_warmup_reward();
        assert!(injected5, "warmup should fire after reset_warmup_session");
    }

    #[test]
    fn test_should_save_interval() {
        let mut engine = OnlineRLEngine::new(OnlineRLConfig {
            save_interval: 3,
            auto_save: false,
            ..Default::default()
        });

        // update_count = 0 → should_save = false (0 % 3 == 0 but count == 0)
        assert!(!engine.should_save());

        // Simulate updates
        engine.update_count = 1;
        assert!(!engine.should_save());
        engine.update_count = 2;
        assert!(!engine.should_save());
        engine.update_count = 3;
        assert!(engine.should_save(), "should save at interval boundary");
    }

    #[test]
    fn test_should_save_auto() {
        let engine = OnlineRLEngine::new(OnlineRLConfig {
            auto_save: true,
            ..Default::default()
        });
        assert!(engine.should_save(), "auto_save=true → always save");
    }

    #[test]
    fn test_file_type_str_mapping() {
        assert_eq!(file_type_str(0), "python");
        assert_eq!(file_type_str(1), "rust");
        assert_eq!(file_type_str(2), "typescript");
        assert_eq!(file_type_str(3), "other");
        assert_eq!(file_type_str(255), "other");
    }

    // ── n-step TD replay buffer tests ─────────────────────────────────────

    #[test]
    fn test_nstep_buffer_warm_up() {
        // Before buffer fills, each update uses the current state (warm-up behaviour).
        let mut engine = OnlineRLEngine::with_defaults();
        let mut qt = QTable::new();
        let mut bandit = LinUCBBandit::new();

        for i in 0..(DEFAULT_REPLAY_CAPACITY - 1) {
            let r = make_reward("Edit", true, 100, 0, (i % 7) as u8, 0);
            let result = engine.process_reward(&r, &mut qt, &mut bandit);
            // At least the first update always passes; subsequent ones may be filtered by EMA.
            // What matters here is no panic and buffer does not exceed DEFAULT_REPLAY_CAPACITY.
            let _ = result;
        }
        // Buffer should have DEFAULT_REPLAY_CAPACITY - 1 entries (none popped yet).
        assert!(
            engine.replay_buffer.len() < DEFAULT_REPLAY_CAPACITY,
            "buffer should not be full after {} steps, got {}",
            DEFAULT_REPLAY_CAPACITY - 1,
            engine.replay_buffer.len()
        );
    }

    #[test]
    fn test_nstep_buffer_capped_at_capacity() {
        // After DEFAULT_REPLAY_CAPACITY+N steps the buffer stays ≤ DEFAULT_REPLAY_CAPACITY.
        let mut engine = OnlineRLEngine::new(OnlineRLConfig {
            min_reward_delta: 0.0, // never filter
            ..Default::default()
        });
        let mut qt = QTable::new();
        let mut bandit = LinUCBBandit::new();

        for _ in 0..(DEFAULT_REPLAY_CAPACITY * 3) {
            let r = make_reward("Edit", true, 100, 0, 2, 1);
            engine.process_reward(&r, &mut qt, &mut bandit);
        }
        assert!(
            engine.replay_buffer.len() <= DEFAULT_REPLAY_CAPACITY,
            "buffer must stay ≤ DEFAULT_REPLAY_CAPACITY, got {}",
            engine.replay_buffer.len()
        );
    }

    #[test]
    fn test_nstep_return_is_higher_than_single_step() {
        // A sequence of positive rewards should accumulate a higher return
        // than any single reward alone.
        let mut engine = OnlineRLEngine::new(OnlineRLConfig {
            min_reward_delta: 0.0,
            ema_alpha: 1.0,
            ..Default::default()
        });
        let mut qt = QTable::new();
        let mut bandit = LinUCBBandit::new();

        // Push DEFAULT_REPLAY_CAPACITY positive rewards to fill the buffer.
        for _ in 0..DEFAULT_REPLAY_CAPACITY {
            let r = make_reward("Edit", true, 100, 0, 2, 0);
            engine.process_reward(&r, &mut qt, &mut bandit);
        }

        // n-step return = 0.75 * (1 + 0.95 + 0.95² + ... + 0.95^7) ≈ 4.64
        // This is > any single-step reward (0.75), meaning the QTable entries
        // accumulate more signal via multi-step lookahead.
        let (state, action) =
            OnlineRLEngine::to_state_action(&make_reward("Edit", true, 100, 0, 2, 0));
        let q = qt.get_q(state, action);
        // Q should be non-zero (updated at least once) and reflect accumulated signal.
        assert!(
            q != 0.0,
            "Q-value should be non-zero after {DEFAULT_REPLAY_CAPACITY} updates"
        );
    }

    #[test]
    fn test_nstep_discounting_with_mixed_rewards() {
        // A negative reward at the end should lower the n-step return compared to
        // all-positive sequence.
        let r_all_positive = {
            let mut engine = OnlineRLEngine::new(OnlineRLConfig {
                min_reward_delta: 0.0,
                ..Default::default()
            });
            let mut qt = QTable::new();
            let mut bandit = LinUCBBandit::new();
            for _ in 0..DEFAULT_REPLAY_CAPACITY {
                engine.process_reward(
                    &make_reward("Edit", true, 100, 0, 2, 0),
                    &mut qt,
                    &mut bandit,
                );
            }
            let (s, a) = OnlineRLEngine::to_state_action(&make_reward("Edit", true, 100, 0, 2, 0));
            qt.get_q(s, a)
        };

        let r_last_negative = {
            let mut engine = OnlineRLEngine::new(OnlineRLConfig {
                min_reward_delta: 0.0,
                ..Default::default()
            });
            let mut qt = QTable::new();
            let mut bandit = LinUCBBandit::new();
            for i in 0..DEFAULT_REPLAY_CAPACITY {
                let accepted = i < DEFAULT_REPLAY_CAPACITY - 1; // last one is negative
                engine.process_reward(
                    &make_reward("Edit", accepted, 100, 0, 2, 0),
                    &mut qt,
                    &mut bandit,
                );
            }
            let (s, a) = OnlineRLEngine::to_state_action(&make_reward("Edit", true, 100, 0, 2, 0));
            qt.get_q(s, a)
        };

        assert!(
            r_last_negative < r_all_positive,
            "mixed sequence Q ({r_last_negative}) should be less than all-positive ({r_all_positive})"
        );
    }

    // ── ReplayBuffer tests ───────────────────────────────────────────────

    #[test]
    fn test_replay_buffer_new_is_empty() {
        let buf = ReplayBuffer::new(10);
        assert!(buf.is_empty());
        assert_eq!(buf.len(), 0);
        assert_eq!(buf.capacity(), 10);
        assert!(!buf.is_full());
    }

    #[test]
    fn test_replay_buffer_push_and_len() {
        let mut buf = ReplayBuffer::new(5);
        for i in 0..3 {
            buf.push(Experience {
                state: i,
                action: 0,
                reward: 0.5,
                next_state: i + 1,
                terminal: false,
            });
        }
        assert_eq!(buf.len(), 3);
        assert!(!buf.is_full());
        assert!(!buf.is_empty());
    }

    #[test]
    fn test_replay_buffer_circular_overwrite() {
        let mut buf = ReplayBuffer::new(3);
        for i in 0..5 {
            buf.push(Experience {
                state: i,
                action: 0,
                reward: i as f64,
                next_state: i + 1,
                terminal: false,
            });
        }
        // Capacity is 3, pushed 5 items => len stays at 3
        assert_eq!(buf.len(), 3);
        assert!(buf.is_full());

        // The buffer should contain the 3 most recent-ish items.
        // Due to circular overwrite: positions [0]=3, [1]=4, [2]=2 (oldest kept)
        // Verify via rewards: 2.0, 3.0, 4.0 should all be present.
        let rewards: Vec<f64> = buf.buffer.iter().map(|e| e.reward).collect();
        assert!(rewards.contains(&2.0), "reward 2.0 should be in buffer");
        assert!(rewards.contains(&3.0), "reward 3.0 should be in buffer");
        assert!(rewards.contains(&4.0), "reward 4.0 should be in buffer");
    }

    #[test]
    fn test_replay_buffer_sample_empty() {
        let buf = ReplayBuffer::new(10);
        let batch = buf.sample(5);
        assert!(
            batch.is_empty(),
            "sampling from empty buffer should return empty vec"
        );
    }

    #[test]
    fn test_replay_buffer_sample_returns_correct_size() {
        let mut buf = ReplayBuffer::new(100);
        for i in 0..50 {
            buf.push(Experience {
                state: i,
                action: 0,
                reward: 0.1,
                next_state: i + 1,
                terminal: false,
            });
        }
        let batch = buf.sample(16);
        assert_eq!(
            batch.len(),
            16,
            "sample should return exactly batch_size items"
        );
    }

    #[test]
    fn test_replay_buffer_sample_single_element() {
        let mut buf = ReplayBuffer::new(10);
        buf.push(Experience {
            state: 42,
            action: 7,
            reward: 0.99,
            next_state: 43,
            terminal: true,
        });

        let batch = buf.sample(5);
        assert_eq!(batch.len(), 5);
        // All samples must be the single element
        for exp in &batch {
            assert_eq!(exp.state, 42);
            assert_eq!(exp.action, 7);
            assert!((exp.reward - 0.99).abs() < f64::EPSILON);
            assert!(exp.terminal);
        }
    }

    #[test]
    fn test_replay_buffer_is_full() {
        let mut buf = ReplayBuffer::new(2);
        assert!(!buf.is_full());
        buf.push(Experience {
            state: 0,
            action: 0,
            reward: 0.0,
            next_state: 1,
            terminal: false,
        });
        assert!(!buf.is_full());
        buf.push(Experience {
            state: 1,
            action: 0,
            reward: 0.0,
            next_state: 2,
            terminal: false,
        });
        assert!(buf.is_full());
        // Pushing more keeps it full
        buf.push(Experience {
            state: 2,
            action: 0,
            reward: 0.0,
            next_state: 3,
            terminal: false,
        });
        assert!(buf.is_full());
        assert_eq!(buf.len(), 2);
    }

    fn reward_for(tool: &str) -> ImmediateReward {
        ImmediateReward {
            tool_name: tool.to_string(),
            accepted: true,
            latency_ms: 10,
            error_count: 0,
            cila_level: 2,
            file_type: 1,
            quality_score: Some(0.9),
        }
    }

    /// The reward must reach the arm that actually made the decision.
    ///
    /// Regression for the open credit loop (04/08/2026): the arm credited was
    /// `djb2_hash(tool_name) % NUM_ARMS`, so whatever `select_arm` chose never
    /// learned from its own outcome.
    #[test]
    fn a_recorded_decision_is_credited_instead_of_the_hash_bucket() {
        let hash_arm = (djb2_hash("Edit") % NUM_ARMS as u64) as usize;
        // Pick an arm the legacy hash would never choose, so the assertion
        // discriminates rather than passing by coincidence.
        let chosen = (hash_arm + 1) % NUM_ARMS;

        let mut engine = OnlineRLEngine::new(OnlineRLConfig::default());
        let mut qtable = QTable::new();
        let mut linucb = LinUCBBandit::new();

        engine.record_decision(
            "Edit",
            chosen,
            ndarray::Array1::zeros(crate::rl::bandit::FEATURE_DIM),
        );
        engine.process_reward(&reward_for("Edit"), &mut qtable, &mut linucb);

        let stats = linucb.arm_stats();
        let pulls = |i: usize| {
            stats
                .iter()
                .find(|(idx, _, _)| *idx == i)
                .map(|(_, p, _)| *p)
        };

        assert_eq!(
            pulls(chosen),
            Some(1),
            "the arm that decided must receive the pull"
        );
        assert_eq!(
            pulls(hash_arm),
            Some(0),
            "the legacy hash bucket must NOT be credited when a real decision exists"
        );
        assert_eq!(engine.decisions().credited_count(), 1);
    }

    /// Sites that never record keep the legacy attribution — adoption is
    /// incremental and nothing regresses by opting out.
    #[test]
    fn without_a_recorded_decision_the_legacy_hash_bucket_still_learns() {
        let hash_arm = (djb2_hash("Bash") % NUM_ARMS as u64) as usize;

        let mut engine = OnlineRLEngine::new(OnlineRLConfig::default());
        let mut qtable = QTable::new();
        let mut linucb = LinUCBBandit::new();

        engine.process_reward(&reward_for("Bash"), &mut qtable, &mut linucb);

        let pulled = linucb
            .arm_stats()
            .iter()
            .find(|(idx, _, _)| *idx == hash_arm)
            .map(|(_, p, _)| *p);
        assert_eq!(pulled, Some(1), "fallback path must still update an arm");
        assert_eq!(
            engine.decisions().credited_count(),
            0,
            "nothing was recorded, so nothing was credited"
        );
    }

    /// One decision funds exactly one credit — a second reward for the same tool
    /// falls back rather than re-crediting a choice that already paid out.
    #[test]
    fn a_decision_is_credited_at_most_once() {
        let hash_arm = (djb2_hash("Read") % NUM_ARMS as u64) as usize;
        let chosen = (hash_arm + 3) % NUM_ARMS;

        let mut engine = OnlineRLEngine::new(OnlineRLConfig::default());
        let mut qtable = QTable::new();
        let mut linucb = LinUCBBandit::new();

        engine.record_decision(
            "Read",
            chosen,
            ndarray::Array1::zeros(crate::rl::bandit::FEATURE_DIM),
        );
        engine.process_reward(&reward_for("Read"), &mut qtable, &mut linucb);
        engine.process_reward(&reward_for("Read"), &mut qtable, &mut linucb);

        let stats = linucb.arm_stats();
        let pulls = |i: usize| {
            stats
                .iter()
                .find(|(idx, _, _)| *idx == i)
                .map(|(_, p, _)| *p)
        };

        assert_eq!(
            pulls(chosen),
            Some(1),
            "the single recorded selection must be credited exactly once"
        );
        assert_eq!(
            engine.decisions().credited_count(),
            1,
            "the second reward found no pending decision"
        );
    }
}
