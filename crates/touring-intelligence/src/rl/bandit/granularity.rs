//! Granularity bandit — contextual selector for task decomposition split factor.
//!
//! # Purpose (Wave C1, 2026-04-20)
//!
//! Closes the feedback loop between `TaskDecomposer`-style task splitting
//! and downstream code health. Given features of a proposed task, selects how
//! aggressively to pre-decompose it: keep it monolithic, split into 2, 3, or 4
//! subtasks. After the task finishes, the observed quality score (e.g. from
//! `CodeHealthReport.composite`) is fed back as a reward. Over time the bandit
//! learns which granularity produces healthier code for each context.
//!
//! # Arms (4)
//!
//! ```text
//! 0: Monolithic  — single atomic subtask
//! 1: Split2      — two parallel/sequential subtasks
//! 2: Split3      — three subtasks
//! 3: Split4      — four or more (capped at four here)
//! ```
//!
//! # Feature vector (12 dims)
//!
//! ```text
//! [0..3]  language:    rust=0, python=1, typescript=2, other=3    (one-hot, 4)
//! [4..6]  size_bucket: small<100 LOC, medium<500, large>=500      (one-hot, 3)
//! [7..11] cila_level:  L0..L4+ clamped                             (one-hot, 5)
//! ```
//!
//! Design intentionally compact: no language-bandit coupling, no
//! `touring-analysis` dep. Callers compute the reward (quality in `[0,1]`)
//! however they like — typically `CodeHealthReport::composite` with a small
//! coordination penalty per extra subtask.
//!
//! # Algorithm
//!
//! Reuses [`LinUCBArm`] (Sherman-Morrison ridge regression) for each of the
//! four arms. Cold-arm forced exploration ensures every split factor gets
//! tried before UCB ranking kicks in.

use ndarray::Array1;
use serde::{Deserialize, Serialize};

use super::linucb::LinUCBArm;

/// Feature-vector dimensionality for [`GranularityBandit`].
pub const GRANULARITY_FEATURE_DIM: usize = 12;

/// Number of bandit arms (split factors).
pub const GRANULARITY_NUM_ARMS: usize = 4;

/// Forced-exploration threshold: pull each arm at least this many times
/// before switching to pure UCB ranking.
const COLD_ARM_THRESHOLD: u64 = 3;

/// Default exploration parameter (ridge-UCB alpha).
const DEFAULT_ALPHA: f64 = 1.0;

/// Proposed task-split factor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SplitFactor {
    /// Keep the task as a single subtask.
    Monolithic,
    /// Decompose into two subtasks.
    Split2,
    /// Decompose into three subtasks.
    Split3,
    /// Decompose into four subtasks.
    Split4,
}

impl SplitFactor {
    /// Number of subtasks this factor produces (1, 2, 3, or 4).
    #[must_use]
    pub fn subtask_count(self) -> u8 {
        match self {
            Self::Monolithic => 1,
            Self::Split2 => 2,
            Self::Split3 => 3,
            Self::Split4 => 4,
        }
    }

    /// Arm index in `[0, GRANULARITY_NUM_ARMS)`.
    #[must_use]
    pub fn as_index(self) -> usize {
        match self {
            Self::Monolithic => 0,
            Self::Split2 => 1,
            Self::Split3 => 2,
            Self::Split4 => 3,
        }
    }

    /// Convert arm index back to a factor. Returns `None` for out-of-range.
    #[must_use]
    pub fn from_index(idx: usize) -> Option<Self> {
        match idx {
            0 => Some(Self::Monolithic),
            1 => Some(Self::Split2),
            2 => Some(Self::Split3),
            3 => Some(Self::Split4),
            _ => None,
        }
    }

    /// All split factors in arm-index order.
    #[must_use]
    pub fn all() -> [Self; GRANULARITY_NUM_ARMS] {
        [Self::Monolithic, Self::Split2, Self::Split3, Self::Split4]
    }
}

/// Build a 12-dim feature vector from task metadata.
///
/// `language` matching is case-insensitive; unknown values fall into the
/// `other` slot. `size_loc` is the estimated number of lines-of-code the
/// task will touch. `cila_level` is the CILA complexity level (clamped to
/// `[0, 4]`).
#[must_use]
pub fn features_for_task(size_loc: usize, language: &str, cila_level: u8) -> Array1<f64> {
    let mut features = Array1::<f64>::zeros(GRANULARITY_FEATURE_DIM);

    // [0..3] language one-hot
    let lang_lower = language.to_ascii_lowercase();
    let lang_idx: usize = match lang_lower.as_str() {
        "rust" | "rs" => 0,
        "python" | "py" => 1,
        "typescript" | "ts" | "tsx" | "javascript" | "js" | "jsx" => 2,
        _ => 3,
    };
    if let Some(slot) = features.get_mut(lang_idx) {
        *slot = 1.0;
    }

    // [4..6] size one-hot
    let size_idx = if size_loc < 100 {
        4
    } else if size_loc < 500 {
        5
    } else {
        6
    };
    if let Some(slot) = features.get_mut(size_idx) {
        *slot = 1.0;
    }

    // [7..11] cila one-hot (clamped 0..=4)
    let cila_clamped = (cila_level as usize).min(4);
    if let Some(slot) = features.get_mut(7 + cila_clamped) {
        *slot = 1.0;
    }

    features
}

/// Convert a raw quality score (`[0,1]`) into a reward that penalizes
/// over-splitting slightly so the bandit learns to prefer the smallest split
/// factor that still achieves high quality.
///
/// `quality` is clamped to `[0,1]`; `subtask_count` must be `>= 1`. The
/// coordination penalty is linear: `0.02 * (subtask_count - 1)`, capped so the
/// minimum possible reward is `quality - 0.08` when `subtask_count == 4`.
#[must_use]
pub fn reward_from_quality(quality: f64, subtask_count: u8) -> f64 {
    let q = quality.clamp(0.0, 1.0);
    let n = subtask_count.max(1) as f64;
    let penalty = 0.02 * (n - 1.0);
    (q - penalty).clamp(-1.0, 1.0)
}

/// Contextual bandit over 4 split factors with 12-dim task features.
#[derive(Debug, Clone)]
pub struct GranularityBandit {
    arms: Vec<LinUCBArm>,
    alpha: f64,
    total_pulls: u64,
}

impl Default for GranularityBandit {
    fn default() -> Self {
        Self::new()
    }
}

impl GranularityBandit {
    /// Create a new bandit with the default exploration parameter.
    #[must_use]
    pub fn new() -> Self {
        Self::with_alpha(DEFAULT_ALPHA)
    }

    /// Create a new bandit with a custom exploration parameter.
    #[must_use]
    pub fn with_alpha(alpha: f64) -> Self {
        Self {
            arms: (0..GRANULARITY_NUM_ARMS)
                .map(|_| LinUCBArm::new(GRANULARITY_FEATURE_DIM))
                .collect(),
            alpha,
            total_pulls: 0,
        }
    }

    /// Pick the best split factor for the given task features.
    ///
    /// During cold-start (< `COLD_ARM_THRESHOLD` pulls on some arm) returns
    /// the least-pulled arm to force exploration. After warm-up, ranks arms by
    /// UCB score. Returns the chosen factor plus its score.
    ///
    /// # Panics
    ///
    /// Panics if `features.len() != GRANULARITY_FEATURE_DIM`.
    pub fn select_split(&mut self, features: &Array1<f64>) -> (SplitFactor, f64) {
        assert_eq!(
            features.len(),
            GRANULARITY_FEATURE_DIM,
            "Feature vector must have {GRANULARITY_FEATURE_DIM} dimensions, got {}",
            features.len()
        );

        // Forced cold-arm exploration.
        if let Some((idx, arm)) = self
            .arms
            .iter()
            .enumerate()
            .filter(|(_, arm)| arm.pulls() < COLD_ARM_THRESHOLD)
            .min_by_key(|(_, arm)| arm.pulls())
        {
            let factor = SplitFactor::from_index(idx).unwrap_or(SplitFactor::Monolithic);
            return (factor, arm.score(features, self.alpha));
        }

        // Warm: pick the arm with the highest UCB score.
        let mut best_idx = 0usize;
        let mut best_score = f64::NEG_INFINITY;
        for (i, arm) in self.arms.iter().enumerate() {
            let s = arm.score(features, self.alpha);
            if s > best_score {
                best_score = s;
                best_idx = i;
            }
        }
        let factor = SplitFactor::from_index(best_idx).unwrap_or(SplitFactor::Monolithic);
        (factor, best_score)
    }

    /// Record the observed reward for a previously selected split factor.
    ///
    /// # Panics
    ///
    /// Panics if `features.len() != GRANULARITY_FEATURE_DIM` (guarded by the
    /// underlying `LinUCBArm::update`).
    pub fn record_outcome(&mut self, factor: SplitFactor, features: &Array1<f64>, reward: f64) {
        assert_eq!(
            features.len(),
            GRANULARITY_FEATURE_DIM,
            "Feature vector must have {GRANULARITY_FEATURE_DIM} dimensions, got {}",
            features.len()
        );
        let idx = factor.as_index();
        if let Some(arm) = self.arms.get_mut(idx) {
            arm.update(features, reward);
            arm.maybe_reorthogonalize();
            self.total_pulls += 1;
        }
    }

    /// Total pulls across all arms.
    #[must_use]
    pub fn total_pulls(&self) -> u64 {
        self.total_pulls
    }

    /// Average reward observed for each arm (by index).
    #[must_use]
    pub fn avg_reward_per_arm(&self) -> [f64; GRANULARITY_NUM_ARMS] {
        let mut out = [0.0; GRANULARITY_NUM_ARMS];
        for (slot, arm) in out.iter_mut().zip(self.arms.iter()) {
            *slot = arm.avg_reward();
        }
        out
    }

    /// Pulls observed for each arm (by index).
    #[must_use]
    pub fn pulls_per_arm(&self) -> [u64; GRANULARITY_NUM_ARMS] {
        let mut out = [0u64; GRANULARITY_NUM_ARMS];
        for (slot, arm) in out.iter_mut().zip(self.arms.iter()) {
            *slot = arm.pulls();
        }
        out
    }

    /// Current exploration parameter.
    #[must_use]
    pub fn alpha(&self) -> f64 {
        self.alpha
    }

    // ── Wave C1.7-persistence ──────────────────────────────────────────

    /// Export the bandit to a serializable snapshot.
    ///
    /// The snapshot carries each arm's ridge-regression state plus the
    /// exploration parameter and total pull count so the bandit can be
    /// restored across daemon restarts.
    #[must_use]
    pub fn to_snapshot(&self) -> GranularitySnapshot {
        let arms: Vec<GranularityArmState> = self
            .arms
            .iter()
            .map(|arm| {
                let (pulls, cum_reward, a_inv, b) = arm.export_state();
                GranularityArmState {
                    pulls,
                    cumulative_reward: cum_reward,
                    a_inv,
                    b,
                }
            })
            .collect();
        GranularitySnapshot {
            version: 1,
            alpha: self.alpha,
            total_pulls: self.total_pulls,
            feature_dim: GRANULARITY_FEATURE_DIM,
            num_arms: GRANULARITY_NUM_ARMS,
            arms,
        }
    }

    /// Restore a bandit from a previously `to_snapshot`-ed payload.
    ///
    /// # Errors
    ///
    /// - `Err("snapshot version …")` when the snapshot was produced by an
    ///   incompatible version of this module.
    /// - `Err("snapshot num_arms mismatch …")` / `feature_dim mismatch` when
    ///   the persisted shape differs from the current compile-time
    ///   constants — typically the sign of a schema change.
    /// - `Err("arm N import failed: …")` propagated from
    ///   [`LinUCBArm::import_state`] when any arm's flattened matrix length
    ///   is inconsistent with the feature dimension.
    pub fn from_snapshot(snapshot: &GranularitySnapshot) -> Result<Self, GranularitySnapshotError> {
        if snapshot.version != 1 {
            return Err(GranularitySnapshotError::UnsupportedVersion(
                snapshot.version,
            ));
        }
        if snapshot.num_arms != GRANULARITY_NUM_ARMS {
            return Err(GranularitySnapshotError::NumArmsMismatch {
                got: snapshot.num_arms,
                expected: GRANULARITY_NUM_ARMS,
            });
        }
        if snapshot.feature_dim != GRANULARITY_FEATURE_DIM {
            return Err(GranularitySnapshotError::FeatureDimMismatch {
                got: snapshot.feature_dim,
                expected: GRANULARITY_FEATURE_DIM,
            });
        }
        let mut bandit = Self::with_alpha(snapshot.alpha);
        for (i, arm_state) in snapshot.arms.iter().enumerate() {
            if let Some(arm) = bandit.arms.get_mut(i) {
                arm.import_state(
                    arm_state.pulls,
                    arm_state.cumulative_reward,
                    &arm_state.a_inv,
                    &arm_state.b,
                )
                .map_err(|e| GranularitySnapshotError::ArmImport { arm: i, msg: e })?;
            }
        }
        bandit.total_pulls = snapshot.total_pulls;
        Ok(bandit)
    }
}

/// Error from [`GranularityBandit::from_snapshot`] (F-8 / RBP-03: typed in place of `String`).
#[derive(Debug, thiserror::Error)]
pub enum GranularitySnapshotError {
    /// The snapshot schema version is not supported (only version 1).
    #[error("snapshot version {0} is not supported (expected 1)")]
    UnsupportedVersion(u32),
    /// The persisted arm count differs from the compile-time constant.
    #[error("snapshot num_arms mismatch: got {got}, expected {expected}")]
    NumArmsMismatch {
        /// Arm count found in the snapshot.
        got: usize,
        /// Arm count required by the current build.
        expected: usize,
    },
    /// The persisted feature dimension differs from the compile-time constant.
    #[error("snapshot feature_dim mismatch: got {got}, expected {expected}")]
    FeatureDimMismatch {
        /// Feature dimension found in the snapshot.
        got: usize,
        /// Feature dimension required by the current build.
        expected: usize,
    },
    /// A per-arm matrix import failed (inconsistent flattened length).
    #[error("arm {arm} import failed: {msg}")]
    ArmImport {
        /// Index of the arm whose import failed.
        arm: usize,
        /// Underlying import error message.
        msg: String,
    },
}

/// Serializable snapshot of a [`GranularityBandit`].
///
/// Tagged with an explicit `version` so future schema changes can be
/// detected on load without silently corrupting state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GranularitySnapshot {
    /// Schema version. Currently `1`.
    pub version: u32,
    /// Exploration parameter at time of snapshot.
    pub alpha: f64,
    /// Total pulls across all arms.
    pub total_pulls: u64,
    /// Feature dimensionality used when the snapshot was produced.
    pub feature_dim: usize,
    /// Number of arms in the bandit at snapshot time.
    pub num_arms: usize,
    /// Per-arm state.
    pub arms: Vec<GranularityArmState>,
}

/// Serializable per-arm state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GranularityArmState {
    /// Number of pulls observed for this arm.
    pub pulls: u64,
    /// Cumulative reward accumulated across pulls.
    pub cumulative_reward: f64,
    /// Ridge-regression `A_inv` matrix flattened row-major
    /// (length `feature_dim * feature_dim`).
    pub a_inv: Vec<f64>,
    /// Reward-weighted feature accumulator (length `feature_dim`).
    pub b: Vec<f64>,
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_factor_index_roundtrip() {
        for f in SplitFactor::all() {
            let i = f.as_index();
            assert_eq!(SplitFactor::from_index(i), Some(f));
        }
        assert_eq!(SplitFactor::from_index(99), None);
    }

    #[test]
    fn split_factor_subtask_count_matches_variant() {
        assert_eq!(SplitFactor::Monolithic.subtask_count(), 1);
        assert_eq!(SplitFactor::Split2.subtask_count(), 2);
        assert_eq!(SplitFactor::Split3.subtask_count(), 3);
        assert_eq!(SplitFactor::Split4.subtask_count(), 4);
    }

    #[test]
    fn features_are_one_hot_in_each_block() {
        let f = features_for_task(50, "rust", 2);
        assert_eq!(f.len(), GRANULARITY_FEATURE_DIM);
        // language block sums to 1
        let lang_sum: f64 = (0..4).filter_map(|i| f.get(i).copied()).sum();
        assert!(
            (lang_sum - 1.0).abs() < 1e-9,
            "lang block not one-hot: {lang_sum}"
        );
        // size block sums to 1
        let size_sum: f64 = (4..7).filter_map(|i| f.get(i).copied()).sum();
        assert!(
            (size_sum - 1.0).abs() < 1e-9,
            "size block not one-hot: {size_sum}"
        );
        // cila block sums to 1
        let cila_sum: f64 = (7..12).filter_map(|i| f.get(i).copied()).sum();
        assert!(
            (cila_sum - 1.0).abs() < 1e-9,
            "cila block not one-hot: {cila_sum}"
        );
    }

    #[test]
    fn features_respect_language_casing() {
        let f1 = features_for_task(10, "Rust", 0);
        let f2 = features_for_task(10, "rs", 0);
        let f3 = features_for_task(10, "RUST", 0);
        assert_eq!(f1, f2);
        assert_eq!(f1, f3);
        assert_eq!(f1.get(0), Some(&1.0));
    }

    #[test]
    fn features_bucket_size_boundaries() {
        assert_eq!(features_for_task(99, "rust", 0).get(4), Some(&1.0));
        assert_eq!(features_for_task(100, "rust", 0).get(5), Some(&1.0));
        assert_eq!(features_for_task(499, "rust", 0).get(5), Some(&1.0));
        assert_eq!(features_for_task(500, "rust", 0).get(6), Some(&1.0));
        assert_eq!(features_for_task(100_000, "rust", 0).get(6), Some(&1.0));
    }

    #[test]
    fn features_clamp_cila_level_above_four() {
        // cila=9 clamps to 4, hitting the last one-hot slot [11]
        let f = features_for_task(10, "rust", 9);
        assert_eq!(f.get(11), Some(&1.0));
    }

    #[test]
    fn features_default_to_other_language() {
        let f = features_for_task(10, "haskell", 1);
        assert_eq!(f.get(3), Some(&1.0));
    }

    #[test]
    fn reward_from_quality_clamps_inputs() {
        assert!((reward_from_quality(1.5, 1) - 1.0).abs() < 1e-9);
        let r = reward_from_quality(-0.5, 1);
        assert!((r - 0.0).abs() < 1e-9);
    }

    #[test]
    fn reward_penalizes_extra_subtasks_linearly() {
        let q = 0.9;
        let r1 = reward_from_quality(q, 1);
        let r4 = reward_from_quality(q, 4);
        assert!(r1 > r4, "more subtasks must reduce reward");
        // penalty is 0.02 per extra subtask → r1 - r4 == 0.06
        assert!((r1 - r4 - 0.06).abs() < 1e-9);
    }

    #[test]
    fn reward_floors_subtask_count_at_one() {
        // subtask_count=0 must behave like 1 (no panic, no negative penalty)
        let r_zero = reward_from_quality(0.8, 0);
        let r_one = reward_from_quality(0.8, 1);
        assert!((r_zero - r_one).abs() < 1e-9);
    }

    #[test]
    fn bandit_initial_state_is_all_zero() {
        let b = GranularityBandit::new();
        assert_eq!(b.total_pulls(), 0);
        assert_eq!(b.pulls_per_arm(), [0, 0, 0, 0]);
        for r in b.avg_reward_per_arm() {
            assert!((r - 0.0).abs() < 1e-9);
        }
    }

    #[test]
    fn bandit_cold_start_forces_exploration_of_all_arms() {
        let mut b = GranularityBandit::new();
        let features = features_for_task(50, "rust", 2);
        let mut seen = [false; GRANULARITY_NUM_ARMS];
        // With COLD_ARM_THRESHOLD=3 and 4 arms, 4 * 3 = 12 calls should touch them all
        for _ in 0..(GRANULARITY_NUM_ARMS as u64 * COLD_ARM_THRESHOLD) {
            let (f, _) = b.select_split(&features);
            b.record_outcome(f, &features, 0.5);
            if let Some(slot) = seen.get_mut(f.as_index()) {
                *slot = true;
            }
        }
        assert!(seen.iter().all(|v| *v), "cold-start must cover all arms");
    }

    #[test]
    fn bandit_converges_to_best_arm_under_deterministic_rewards() {
        // Monolithic always produces reward 0.95, every other arm 0.10.
        // After warm-up the bandit should prefer Monolithic.
        let mut b = GranularityBandit::new();
        let features = features_for_task(10, "rust", 0);
        for _ in 0..200 {
            let (f, _) = b.select_split(&features);
            let reward = if f == SplitFactor::Monolithic {
                0.95
            } else {
                0.10
            };
            b.record_outcome(f, &features, reward);
        }
        let avg = b.avg_reward_per_arm();
        // avg[0] = Monolithic; must beat every other arm comfortably
        let mono = avg.first().copied().unwrap_or(0.0);
        for i in 1..GRANULARITY_NUM_ARMS {
            let other = avg.get(i).copied().unwrap_or(0.0);
            assert!(
                mono > other,
                "arm {i} avg {other} must be below Monolithic avg {mono}"
            );
        }
        // Pulls should concentrate on Monolithic
        let pulls = b.pulls_per_arm();
        let mono_pulls = pulls.first().copied().unwrap_or(0);
        let total_other: u64 = pulls.iter().skip(1).sum();
        assert!(
            mono_pulls > total_other,
            "Monolithic pulls {mono_pulls} must dominate others {total_other}"
        );
    }

    #[test]
    fn bandit_prefers_split3_when_large_complex_tasks_reward_it() {
        let mut b = GranularityBandit::new();
        let features = features_for_task(800, "rust", 4);
        for _ in 0..200 {
            let (f, _) = b.select_split(&features);
            let reward = if f == SplitFactor::Split3 { 0.9 } else { 0.2 };
            b.record_outcome(f, &features, reward);
        }
        let pulls = b.pulls_per_arm();
        let best_idx = SplitFactor::Split3.as_index();
        let best_pulls = pulls.get(best_idx).copied().unwrap_or(0);
        let others: u64 = pulls
            .iter()
            .enumerate()
            .filter_map(|(i, &p)| if i == best_idx { None } else { Some(p) })
            .sum();
        assert!(
            best_pulls > others,
            "Split3 should dominate: best={best_pulls}, others={others}"
        );
    }

    #[test]
    fn bandit_total_pulls_increments_on_each_update() {
        let mut b = GranularityBandit::new();
        let features = features_for_task(50, "rust", 1);
        for i in 1..=10u64 {
            let (f, _) = b.select_split(&features);
            b.record_outcome(f, &features, 0.5);
            assert_eq!(b.total_pulls(), i);
        }
    }

    #[test]
    #[should_panic(expected = "Feature vector must have 12 dimensions")]
    fn bandit_select_panics_on_wrong_feature_dim() {
        let mut b = GranularityBandit::new();
        let wrong = Array1::<f64>::zeros(3);
        let _ = b.select_split(&wrong);
    }

    #[test]
    #[should_panic(expected = "Feature vector must have 12 dimensions")]
    fn bandit_record_panics_on_wrong_feature_dim() {
        let mut b = GranularityBandit::new();
        let wrong = Array1::<f64>::zeros(7);
        b.record_outcome(SplitFactor::Monolithic, &wrong, 0.5);
    }

    #[test]
    fn bandit_with_custom_alpha_stores_alpha() {
        let b = GranularityBandit::with_alpha(2.5);
        assert!((b.alpha() - 2.5).abs() < 1e-9);
    }

    #[test]
    fn bandit_is_cloneable_and_independent() {
        let mut b1 = GranularityBandit::new();
        let features = features_for_task(50, "rust", 1);
        b1.record_outcome(SplitFactor::Monolithic, &features, 0.8);
        let b2 = b1.clone();
        assert_eq!(b1.total_pulls(), b2.total_pulls());
        assert_eq!(b1.pulls_per_arm(), b2.pulls_per_arm());
    }

    // ── Wave C1.7-persistence: snapshot roundtrip ──────────────────────

    #[test]
    fn snapshot_roundtrip_preserves_state() {
        let mut original = GranularityBandit::new();
        // Drive some reward history across multiple arms.
        for (factor, size) in [
            (SplitFactor::Monolithic, 40usize),
            (SplitFactor::Split2, 200),
            (SplitFactor::Split3, 800),
        ] {
            let features = features_for_task(size, "rust", 2);
            for _ in 0..5 {
                original.record_outcome(factor, &features, 0.7);
            }
        }
        let pulls_before = original.pulls_per_arm();
        let avg_before = original.avg_reward_per_arm();
        let total_before = original.total_pulls();
        let alpha_before = original.alpha();

        // Serialize and restore through JSON to ensure the schema is portable.
        let snap = original.to_snapshot();
        let json = serde_json::to_string(&snap).expect("snapshot serializes");
        let restored_snap: GranularitySnapshot =
            serde_json::from_str(&json).expect("snapshot deserializes");
        let restored = GranularityBandit::from_snapshot(&restored_snap).expect("restore ok");

        assert_eq!(restored.total_pulls(), total_before);
        assert_eq!(restored.pulls_per_arm(), pulls_before);
        assert!((restored.alpha() - alpha_before).abs() < 1e-12);
        for (lhs, rhs) in restored.avg_reward_per_arm().iter().zip(avg_before.iter()) {
            assert!(
                (lhs - rhs).abs() < 1e-9,
                "avg reward drifted {lhs} vs {rhs}"
            );
        }
    }

    #[test]
    fn snapshot_rejects_incompatible_version() {
        let mut snap = GranularityBandit::new().to_snapshot();
        snap.version = 42;
        let err = GranularityBandit::from_snapshot(&snap)
            .unwrap_err()
            .to_string();
        assert!(err.contains("version"), "error must mention version: {err}");
    }

    #[test]
    fn snapshot_rejects_mismatched_num_arms() {
        let mut snap = GranularityBandit::new().to_snapshot();
        snap.num_arms = 7;
        let err = GranularityBandit::from_snapshot(&snap)
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("num_arms"),
            "error must mention num_arms: {err}"
        );
    }

    #[test]
    fn snapshot_rejects_mismatched_feature_dim() {
        let mut snap = GranularityBandit::new().to_snapshot();
        snap.feature_dim = GRANULARITY_FEATURE_DIM + 1;
        let err = GranularityBandit::from_snapshot(&snap)
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("feature_dim"),
            "error must mention feature_dim: {err}"
        );
    }

    #[test]
    fn snapshot_rejects_truncated_arm_state() {
        let mut snap = GranularityBandit::new().to_snapshot();
        if let Some(arm0) = snap.arms.get_mut(0) {
            // Drop the last element so the flat A_inv length no longer matches dim*dim.
            arm0.a_inv.pop();
        }
        let err = GranularityBandit::from_snapshot(&snap)
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("arm 0"),
            "error must cite the offending arm: {err}"
        );
    }

    #[test]
    fn restored_bandit_continues_learning_from_prior_state() {
        // Train → snapshot → restore → keep training. The restored bandit
        // must accumulate pulls on top of the prior count, not reset.
        let mut original = GranularityBandit::new();
        let features = features_for_task(10, "rust", 0);
        for _ in 0..10 {
            original.record_outcome(SplitFactor::Monolithic, &features, 0.9);
        }
        let snap = original.to_snapshot();

        let mut restored = GranularityBandit::from_snapshot(&snap).expect("restore ok");
        for _ in 0..5 {
            restored.record_outcome(SplitFactor::Monolithic, &features, 0.9);
        }
        assert_eq!(restored.total_pulls(), 15);
        let mono_pulls = restored.pulls_per_arm().first().copied().unwrap_or(0);
        assert_eq!(mono_pulls, 15);
    }
}
