//! LinUCB contextual bandit for adaptive system reminder injection.
//!
//! Separate from the context-injection bandit (`linucb.rs`) — this bandit
//! manages *which reminder* to show (VGP, memory recall, speculate, etc.)
//! based on session state, not which context sections to inject.
//!
//! # Feature Vector (6 dimensions)
//!
//! ```text
//! [0] cila_level_normalized   — CILA L0-L6 normalized to 0.0–1.0
//! [1] recent_corrections      — 1.0 if user corrected in last 5 turns
//! [2] context_fill_ratio      — context window fill ratio (0.0–1.0)
//! [3] tool_success_rate       — mean tool success rate last 10 calls (0.0–1.0)
//! [4] is_rust_task            — 1.0 if active files are .rs, else 0.0
//! [5] session_turn_norm       — min(session_turn / 50.0, 1.0)
//! ```
//!
//! # Arms (7 reminder strategies)
//!
//! ```text
//! 0: None           — no reminder injected (silence is also a strategy)
//! 1: VgpCheck       — run touring_ast_find before referencing existing types
//! 2: MemoryRecall   — check touring_memory_recall for past lessons
//! 3: SpeculateFirst — run touring_speculate before Write/Edit
//! 4: BlastRadius    — check blast_radius before editing shared modules
//! 5: TestValidation — run cargo test / pytest to validate current state
//! 6: CircuitBreaker — stop and re-diagnose (same error 3+ times pattern)
//! ```
//!
//! # Reward signal
//!
//! - `was_useful = true`  → `+1.0`: reminder prevented a mistake or was acted on correctly
//! - `was_useful = false` → `NEGATIVE_REWARD`: reminder was noise (user corrected soon after)
//!
//! The `NEGATIVE_REWARD` constant is intentionally moderate (-0.3) — strong enough to reduce
//! false positives without suppressing useful reminders in unseen contexts. Adjust based on
//! observed false-positive rate in practice.

use ndarray::{Array1, Array2};
use serde::{Deserialize, Serialize};

/// Feature vector dimensionality for the reminder bandit.
pub const REMINDER_FEATURE_DIM: usize = 6;

/// Number of reminder strategy arms.
pub const REMINDER_NUM_ARMS: usize = 7;

/// Exploration parameter. Higher = more exploration of less-tried reminders.
const REMINDER_ALPHA: f64 = 1.2;

/// Negative reward when a reminder was not useful.
///
/// Moderate penalty: the bandit learns to avoid reminders that don't prevent
/// mistakes in the current context, without suppressing exploration entirely.
/// Tune this value based on observed false-positive rate:
/// - High false positives (noisy system) → increase magnitude (e.g. -0.5)
/// - Low false positives (accurate system) → decrease magnitude (e.g. -0.1)
const NEGATIVE_REWARD: f64 = -0.3;

/// Which reminder to inject into the prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ReminderKind {
    /// No reminder — silence is a valid strategy when context is clean.
    None = 0,
    /// Run `touring_ast_find` before referencing existing types (VGP gate).
    VgpCheck = 1,
    /// Check `touring_memory_recall` for past lessons before starting.
    MemoryRecall = 2,
    /// Run `touring_speculate` before any Write/Edit on existing files.
    SpeculateFirst = 3,
    /// Check blast_radius before editing high-impact modules.
    BlastRadius = 4,
    /// Run test suite to validate current state before continuing.
    TestValidation = 5,
    /// Stop and re-diagnose — same-error circuit breaker triggered.
    CircuitBreaker = 6,
}

impl ReminderKind {
    /// Human-readable reminder text ready for context injection.
    ///
    /// Returns an empty string for [`ReminderKind::None`] — callers can skip injection.
    pub fn reminder_text(self) -> &'static str {
        match self {
            Self::None => "",
            Self::VgpCheck => {
                "⚠️ VGP: run `touring_ast_find` to verify real field names \
                 before generating code that references existing types."
            }
            Self::MemoryRecall => {
                "💡 Memory: check `touring_memory_recall` for past lessons \
                 on this pattern before proceeding."
            }
            Self::SpeculateFirst => {
                "🔬 Speculate: run `touring_speculate` on the target file \
                 before applying any Write/Edit."
            }
            Self::BlastRadius => {
                "💥 Blast radius: check `touring_graph(blast_radius)` before \
                 editing — this file may have many transitive dependents."
            }
            Self::TestValidation => {
                "✅ Tests: validate current state with `cargo test` / `pytest` \
                 before continuing to the next step."
            }
            Self::CircuitBreaker => {
                "🛑 Circuit breaker: same error pattern detected 3+ times. \
                 Stop. Re-diagnose root cause before retrying."
            }
        }
    }

    /// Convert arm index to kind. Returns `None` for out-of-range indices.
    pub fn from_index(idx: usize) -> Option<Self> {
        match idx {
            0 => Some(Self::None),
            1 => Some(Self::VgpCheck),
            2 => Some(Self::MemoryRecall),
            3 => Some(Self::SpeculateFirst),
            4 => Some(Self::BlastRadius),
            5 => Some(Self::TestValidation),
            6 => Some(Self::CircuitBreaker),
            _ => None,
        }
    }
}

/// Session context used to build the 6-dimensional feature vector.
#[derive(Debug, Clone)]
pub struct ReminderContext {
    /// CILA complexity level (0–6).
    pub cila_level: u8,
    /// Whether the user corrected the assistant in the last 5 turns.
    pub recent_corrections: bool,
    /// Context window fill ratio (0.0–1.0).
    pub context_fill_ratio: f64,
    /// Mean tool success rate over the last 10 calls (0.0–1.0).
    pub tool_success_rate: f64,
    /// Whether the current task primarily involves Rust (`.rs`) files.
    pub is_rust_task: bool,
    /// Current session turn counter.
    pub session_turn: u32,
}

impl ReminderContext {
    /// Build the normalized 6-dimensional feature vector.
    pub fn feature_vector(&self) -> Array1<f64> {
        Array1::from_vec(vec![
            (f64::from(self.cila_level) / 6.0).clamp(0.0, 1.0),
            if self.recent_corrections { 1.0 } else { 0.0 },
            self.context_fill_ratio.clamp(0.0, 1.0),
            self.tool_success_rate.clamp(0.0, 1.0),
            if self.is_rust_task { 1.0 } else { 0.0 },
            (f64::from(self.session_turn) / 50.0).clamp(0.0, 1.0),
        ])
    }
}

/// Single LinUCB arm with its own ridge regression model.
///
/// Maintains `A_inv` (inverse of the design matrix) and `b` (reward-weighted features).
/// Parameters are serialized as flat `Vec<f64>` for JSON compatibility.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ReminderArm {
    /// Inverse of the design matrix, row-major flattened (REMINDER_FEATURE_DIM²).
    a_inv: Vec<f64>,
    /// Reward-weighted feature accumulator (REMINDER_FEATURE_DIM elements).
    b: Vec<f64>,
    /// Total times this arm has been updated via reward.
    pulls: u64,
}

impl ReminderArm {
    fn new() -> Self {
        let d = REMINDER_FEATURE_DIM;
        let mut a_inv = vec![0.0_f64; d * d];
        // A starts as identity matrix → A_inv = identity
        for i in 0..d {
            if let Some(v) = a_inv.get_mut(i * d + i) {
                *v = 1.0;
            }
        }
        Self {
            a_inv,
            b: vec![0.0; d],
            pulls: 0,
        }
    }

    fn a_inv_matrix(&self) -> Array2<f64> {
        let d = REMINDER_FEATURE_DIM;
        Array2::from_shape_vec((d, d), self.a_inv.clone())
            .expect("ReminderArm: a_inv has correct shape invariant")
    }

    fn b_vector(&self) -> Array1<f64> {
        Array1::from_vec(self.b.clone())
    }

    /// UCB score: `theta^T x + alpha * sqrt(x^T A_inv x)`.
    fn ucb_score(&self, x: &Array1<f64>) -> f64 {
        let a_inv = self.a_inv_matrix();
        let b = self.b_vector();
        let theta = a_inv.dot(&b);
        let mean = theta.dot(x);
        let a_inv_x = a_inv.dot(x);
        let variance = x.dot(&a_inv_x).max(0.0);
        mean + REMINDER_ALPHA * variance.sqrt()
    }

    /// Sherman-Morrison rank-1 update after observing reward `r` for feature `x`.
    ///
    /// `(A + x xᵀ)⁻¹ = A_inv - (A_inv x)(A_inv x)ᵀ / (1 + xᵀ A_inv x)`
    fn update(&mut self, x: &Array1<f64>, reward: f64) {
        let a_inv = self.a_inv_matrix();
        let a_inv_x = a_inv.dot(x);
        let denom = 1.0 + x.dot(&a_inv_x);

        if denom.abs() < 1e-10 {
            // Numerically degenerate — skip update to preserve model stability.
            return;
        }

        // Sherman-Morrison outer product: (A_inv x)(A_inv x)ᵀ / denom
        // insert_axis + broadcast multiply — no deprecated reshape, no per-element indexing.
        let col = a_inv_x.view().insert_axis(ndarray::Axis(1)); // (d, 1)
        let row = a_inv_x.view().insert_axis(ndarray::Axis(0)); // (1, d)
        let mut new_a_inv = a_inv;
        new_a_inv -= &((&col * &row) / denom);
        self.a_inv = new_a_inv.into_raw_vec_and_offset().0;

        for (bi, xi) in self.b.iter_mut().zip(x.iter()) {
            *bi += reward * xi;
        }
        self.pulls += 1;
    }
}

/// LinUCB contextual bandit for selecting the most useful session reminder.
///
/// Operates independently from the context-injection bandit (`LinUCBBandit`) —
/// it learns *which reminder* to show based on session state, not which context
/// sections to enrich.
///
/// # Example
///
/// ```
/// use touring_intelligence::rl::bandit::reminder_bandit::{ReminderBandit, ReminderContext};
///
/// let mut bandit = ReminderBandit::new();
/// let ctx = ReminderContext {
///     cila_level: 4,
///     recent_corrections: true,
///     context_fill_ratio: 0.7,
///     tool_success_rate: 0.6,
///     is_rust_task: true,
///     session_turn: 15,
/// };
///
/// let (arm, text) = bandit.select(&ctx);
/// // inject `text` into prompt if non-empty...
/// // after observing outcome:
/// bandit.reward(arm, /*was_useful=*/ true, &ctx);
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReminderBandit {
    arms: Vec<ReminderArm>,
    total_selections: u64,
}

impl ReminderBandit {
    /// Create a new bandit with uniform priors (no prior experience).
    pub fn new() -> Self {
        Self {
            arms: (0..REMINDER_NUM_ARMS).map(|_| ReminderArm::new()).collect(),
            total_selections: 0,
        }
    }

    /// Select the highest-UCB reminder arm for the current session context.
    ///
    /// Returns `(arm_index, reminder_text)`. If the selected arm is `None`,
    /// `reminder_text` is empty and no injection should occur.
    pub fn select(&mut self, ctx: &ReminderContext) -> (usize, &'static str) {
        let features = ctx.feature_vector();
        let best_arm = self
            .arms
            .iter()
            .enumerate()
            .map(|(i, arm)| (i, arm.ucb_score(&features)))
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(i, _)| i)
            .unwrap_or(0);

        self.total_selections += 1;
        let kind = ReminderKind::from_index(best_arm).unwrap_or(ReminderKind::None);
        (best_arm, kind.reminder_text())
    }

    /// Update the model after observing whether the reminder was useful.
    ///
    /// - `arm`: index returned by [`ReminderBandit::select`]
    /// - `was_useful`: `true` if the reminder prevented a mistake or was acted on;
    ///   `false` if the user corrected shortly after (reminder was noise)
    /// - `ctx`: the same context used during selection (needed to update the model)
    pub fn reward(&mut self, arm: usize, was_useful: bool, ctx: &ReminderContext) {
        let Some(arm_ref) = self.arms.get_mut(arm) else {
            return;
        };
        let reward = if was_useful { 1.0 } else { NEGATIVE_REWARD };
        let features = ctx.feature_vector();
        arm_ref.update(&features, reward);
    }

    /// Pull counts per arm (how many times each arm was updated via reward).
    pub fn pull_counts(&self) -> Vec<u64> {
        self.arms.iter().map(|a| a.pulls).collect()
    }

    /// Total reminder selections made (includes arms where reward was not called).
    pub fn total_selections(&self) -> u64 {
        self.total_selections
    }

    /// Serialize to JSON for persistence.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Deserialize from JSON.
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
}

impl Default for ReminderBandit {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_ctx() -> ReminderContext {
        ReminderContext {
            cila_level: 3,
            recent_corrections: false,
            context_fill_ratio: 0.5,
            tool_success_rate: 0.8,
            is_rust_task: true,
            session_turn: 10,
        }
    }

    #[test]
    fn test_new_has_correct_arm_count() {
        let bandit = ReminderBandit::new();
        assert_eq!(bandit.arms.len(), REMINDER_NUM_ARMS);
        assert_eq!(bandit.total_selections(), 0);
        assert!(bandit.pull_counts().iter().all(|&c| c == 0));
    }

    #[test]
    fn test_select_returns_valid_arm_and_text() {
        let mut bandit = ReminderBandit::new();
        let (arm, text) = bandit.select(&default_ctx());
        assert!(
            arm < REMINDER_NUM_ARMS,
            "arm must be in [0, REMINDER_NUM_ARMS)"
        );
        let kind = ReminderKind::from_index(arm).unwrap();
        assert_eq!(text, kind.reminder_text(), "text must match arm kind");
    }

    #[test]
    fn test_select_increments_total_selections() {
        let mut bandit = ReminderBandit::new();
        bandit.select(&default_ctx());
        bandit.select(&default_ctx());
        assert_eq!(bandit.total_selections(), 2);
    }

    #[test]
    fn test_reward_increments_pull_count() {
        let mut bandit = ReminderBandit::new();
        let ctx = default_ctx();
        let (arm, _) = bandit.select(&ctx);
        let before = bandit.pull_counts()[arm];
        bandit.reward(arm, true, &ctx);
        assert_eq!(bandit.pull_counts()[arm], before + 1);
    }

    #[test]
    fn test_reward_out_of_range_does_not_panic() {
        let mut bandit = ReminderBandit::new();
        bandit.reward(999, true, &default_ctx()); // must not panic
    }

    #[test]
    fn test_feature_vector_has_correct_dim() {
        let features = default_ctx().feature_vector();
        assert_eq!(features.len(), REMINDER_FEATURE_DIM);
    }

    #[test]
    fn test_feature_vector_values_clamped() {
        let ctx = ReminderContext {
            cila_level: 6,
            recent_corrections: true,
            context_fill_ratio: 2.0, // out of range → clamped to 1.0
            tool_success_rate: -1.0, // out of range → clamped to 0.0
            is_rust_task: false,
            session_turn: 200, // far above 50 → clamped to 1.0
        };
        let f = ctx.feature_vector();
        assert!((f[0] - 1.0).abs() < 1e-9); // 6/6
        assert!((f[1] - 1.0).abs() < 1e-9); // recent_corrections
        assert!((f[2] - 1.0).abs() < 1e-9); // clamped from 2.0
        assert!((f[3] - 0.0).abs() < 1e-9); // clamped from -1.0
        assert!((f[4] - 0.0).abs() < 1e-9); // not rust
        assert!((f[5] - 1.0).abs() < 1e-9); // capped at 1.0
    }

    #[test]
    fn test_positive_reward_shifts_selection_toward_arm() {
        // LinUCB: unexplored arms have a permanent exploration bonus α·‖x‖ ≈ 1.6
        // that outweighs any single-arm reward history. We must prime ALL arms with
        // a few pulls first to suppress that bonus, then heavily reward arm 1.
        let mut bandit = ReminderBandit::new();
        let rust_ctx = ReminderContext {
            cila_level: 3,
            recent_corrections: false,
            context_fill_ratio: 0.3,
            tool_success_rate: 0.7,
            is_rust_task: true,
            session_turn: 5,
        };
        // Prime every arm (3× neutral/negative) to level the exploration bonus.
        for _ in 0..3 {
            for arm in 0..REMINDER_NUM_ARMS {
                bandit.reward(arm, false, &rust_ctx);
            }
        }
        // Heavily reward arm 1 (VgpCheck) — UCB now clearly favours it.
        for _ in 0..30 {
            bandit.reward(1, true, &rust_ctx);
        }
        let (arm, _) = bandit.select(&rust_ctx);
        assert_eq!(
            arm, 1,
            "after 30 positive rewards, VgpCheck (arm 1) must be selected"
        );
    }

    #[test]
    fn test_json_roundtrip() {
        let mut bandit = ReminderBandit::new();
        let ctx = default_ctx();
        bandit.reward(2, true, &ctx);
        bandit.reward(3, false, &ctx);

        let json = bandit.to_json().expect("must serialize");
        let restored = ReminderBandit::from_json(&json).expect("must deserialize");

        assert_eq!(bandit.total_selections(), restored.total_selections());
        assert_eq!(bandit.pull_counts(), restored.pull_counts());
    }

    #[test]
    fn test_reminder_kind_all_have_valid_from_index() {
        for i in 0..REMINDER_NUM_ARMS {
            assert!(
                ReminderKind::from_index(i).is_some(),
                "index {i} must map to a ReminderKind"
            );
        }
        assert!(ReminderKind::from_index(REMINDER_NUM_ARMS).is_none());
    }

    #[test]
    fn test_non_none_arms_have_non_empty_text() {
        for i in 1..REMINDER_NUM_ARMS {
            let kind = ReminderKind::from_index(i).unwrap();
            assert!(
                !kind.reminder_text().is_empty(),
                "arm {i} ({kind:?}) must have non-empty reminder text"
            );
        }
        assert_eq!(ReminderKind::None.reminder_text(), "");
    }

    #[test]
    fn test_negative_reward_does_not_panic_or_corrupt() {
        let mut bandit = ReminderBandit::new();
        let ctx = default_ctx();
        // Simulate several not-useful reminders
        for arm in 0..REMINDER_NUM_ARMS {
            bandit.reward(arm, false, &ctx);
        }
        // Must still select a valid arm
        let (arm, _) = bandit.select(&ctx);
        assert!(arm < REMINDER_NUM_ARMS);
    }
}
