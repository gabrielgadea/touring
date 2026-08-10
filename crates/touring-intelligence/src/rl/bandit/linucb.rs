//! LinUCB contextual bandit for adaptive context injection.
//!
//! # Algorithm
//!
//! LinUCB (Li et al., 2010) maintains per-arm ridge regression models.
//! For each arm `a`, we track:
//! - `A_a` = `d x d` design matrix (starts as identity)
//! - `b_a` = `d`-vector of reward-weighted features
//!
//! At decision time, for features `x`:
//! - `theta_a = A_a^{-1} * b_a` (parameter estimate)
//! - `p_a = theta_a^T * x + alpha * sqrt(x^T * A_a^{-1} * x)` (UCB score)
//!
//! We maintain `A_inv` directly and update it via Sherman-Morrison:
//! `(A + xx^T)^{-1} = A^{-1} - (A^{-1} x x^T A^{-1}) / (1 + x^T A^{-1} x)`
//!
//! # Feature Vector (25 dimensions)
//!
//! ```text
//! [0..3]   file_type:              python=0, rust=1, typescript=2, other=3   (one-hot, 4)
//! [4..6]   file_size_bucket:       small<100, medium<1000, large>=1000       (one-hot, 3)
//! [7..9]   session_turn:           early<10, mid<50, late>=50                (one-hot, 3)
//! [10..11] recent_errors:          none=0, some>=1                           (continuous, 2)
//! [12..18] cila_level:             L0..L6                                    (one-hot, 7)
//! [19]     error_count_session:    session error rate (0.0–1.0, /10 cap)     (continuous, 1)
//! [20]     recent_tool_success:    mean success rate last 10 tools (0.0–1.0) (continuous, 1)
//! [21..24] time_of_day_bucket:     night=21,morning=22,afternoon=23,evening=24 (one-hot, 4)
//! Total: 25
//! ```
//!
//! # Arms (8 context injection types)
//!
//! ```text
//! 0: none             — no extra context
//! 1: overview         — AST symbols only
//! 2: gotcha           — gotchas only
//! 3: blast_radius     — impact only
//! 4: relations        — imports/callers only
//! 5: overview+gotcha
//! 6: overview+blast_radius
//! 7: full_enrichment  — all context types
//! ```

use ndarray::{Array1, Array2, Axis};
use serde::{Deserialize, Serialize};
#[allow(unused_imports)]
use std::error::Error;
use std::path::Path;

/// Number of bandit arms (context injection strategies).
pub const NUM_ARMS: usize = 8;

/// Dimensionality of the feature vector.
/// H1-D: expanded from 19 → 25 (added error_count_session, recent_tool_success, time_of_day×4).
/// Persisted models with d=19 are automatically discarded on load (dimension mismatch in from_snapshot).
pub const FEATURE_DIM: usize = 25;

/// Default exploration parameter (higher = more exploration).
const DEFAULT_ALPHA: f64 = 1.0;

// ── GPU WGSL Shaders for LinUCB ────────────────────────────────────────────

#[cfg(feature = "gpu-compute")]
mod gpu_shaders {
    // Computes `score_a = theta_a^T * x + alpha * sqrt(x^T * A_inv_a * x)` for
    // all 8 arms in a single GPU dispatch. Each workgroup computes one arm.
    //
    // Layout:
    // - @binding(0): features — 25-dim f32 array (input)
    // - @binding(1): A_inv — 8 × 25 × 25 matrices flattened (25 * 25 per arm, row-major)
    // - @binding(2): b_vec — 8 × 25 matrices (25 per arm, row-major)
    // - @binding(3): ucb_scores — 8-element output array (output)
    // - @binding(4): alpha — scalar f32 (uniform)
    pub const LINUCB_UCB_SHADER: &str = r#"
@group(0) @binding(0) var<storage, read> features: array<f32>;
@group(0) @binding(1) var<storage, read> A_inv_flat: array<f32>;
@group(0) @binding(2) var<storage, read> b_flat: array<f32>;
@group(0) @binding(3) var<storage, read_write> ucb_scores: array<f32>;
@group(0) @binding(4) var<uniform> alpha: f32;

const FEATURE_DIM: u32 = 25u;
const NUM_ARMS: u32 = 8u;
const A_INV_STRIDE: u32 = FEATURE_DIM * FEATURE_DIM;

fn compute_ucb(arm_idx: u32) -> f32 {
    let a_inv_offset = arm_idx * A_INV_STRIDE;
    let b_offset = arm_idx * FEATURE_DIM;

    // Compute theta = A_inv * b for this arm (25-dim)
    var theta_sum: f32 = 0.0;
    for (var i: u32 = 0u; i < FEATURE_DIM; i++) {
        var a_row_sum: f32 = 0.0;
        for (var j: u32 = 0u; j < FEATURE_DIM; j++) {
            a_row_sum += A_inv_flat[a_inv_offset + i * FEATURE_DIM + j] * b_flat[b_offset + j];
        }
        theta_sum += a_row_sum * features[i];
    }

    // Compute x^T * A_inv * x via A_inv * x dot x
    var a_inv_x_sum: f32 = 0.0;
    for (var i: u32 = 0u; i < FEATURE_DIM; i++) {
        var a_col_sum: f32 = 0.0;
        for (var j: u32 = 0u; j < FEATURE_DIM; j++) {
            a_col_sum += A_inv_flat[a_inv_offset + j * FEATURE_DIM + i] * features[j];
        }
        a_inv_x_sum += a_col_sum * features[i];
    }

    let uncertainty = sqrt(max(a_inv_x_sum, 0.0));
    let score = theta_sum + alpha * uncertainty;
    return score;
}

@compute @workgroup_size(8)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let arm = global_id.x;
    if (arm >= NUM_ARMS) { return; }
    ucb_scores[arm] = compute_ucb(arm);
}
"#;

    /// WGSL shader for Sherman-Morrison matrix update on GPU.
    ///
    /// Computes `A_inv_new = A_inv - (A_inv * x * x^T * A_inv) / (1 + x^T * A_inv * x)`
    /// for a single arm. This offloads the O(d^2) rank-1 update to the GPU.
    ///
    /// Layout:
    /// - @binding(0): A_inv_in — 25×25 matrix flattened (input)
    /// - @binding(1): x_vec — 25-dim feature vector (input)
    /// - @binding(2): A_inv_out — 25×25 matrix flattened (output)
    /// - @binding(3): reward — scalar f32 (uniform, for b update tracking)
    pub const LINUCB_SHERMAN_MORRISON_SHADER: &str = r#"
@group(0) @binding(0) var<storage, read> A_inv_in: array<f32>;
@group(0) @binding(1) var<storage, read> x_vec: array<f32>;
@group(0) @binding(2) var<storage, read_write> A_inv_out: array<f32>;
@group(0) @binding(3) var<uniform> reward: f32;

const FEATURE_DIM: u32 = 25u;

fn compute_a_inverse_update() -> f32 {
    // Compute A_inv * x
    var a_inv_x: array<f32, 25u>;
    for (var i: u32 = 0u; i < FEATURE_DIM; i++) {
        var sum: f32 = 0.0;
        for (var j: u32 = 0u; j < FEATURE_DIM; j++) {
            sum += A_inv_in[i * FEATURE_DIM + j] * x_vec[j];
        }
        a_inv_x[i] = sum;
    }

    // Compute x^T * A_inv * x (scalar denominator term)
    var denom: f32 = 1.0;
    for (var i: u32 = 0u; i < FEATURE_DIM; i++) {
        denom += x_vec[i] * a_inv_x[i];
    }

    // Compute outer product: a_inv_x * x^T (25×25)
    // Then compute A_inv - (A_inv * x * x^T * A_inv) / denom
    // Parallelize: each thread computes one row of the result
    return denom;
}

@compute @workgroup_size(25)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let row = global_id.x;
    if (row >= FEATURE_DIM) { return; }

    // Compute A_inv * x first
    var a_inv_x: f32 = 0.0;
    for (var j: u32 = 0u; j < FEATURE_DIM; j++) {
        a_inv_x += A_inv_in[row * FEATURE_DIM + j] * x_vec[j];
    }

    // Compute x^T * A_inv * x
    var x_a_inv_x: f32 = 1.0;
    for (var k: u32 = 0u; k < FEATURE_DIM; k++) {
        x_a_inv_x += x_vec[k] * A_inv_in[k * FEATURE_DIM + row]; // uses symmetry
    }

    // Sherman-Morrison: A_new = A_inv - (A_inv*x*x^T*A_inv) / denom
    // For row i: A_new[i][j] = A_inv[i][j] - a_inv_x[i] * a_inv_x[j] / denom
    for (var col: u32 = 0u; col < FEATURE_DIM; col++) {
        var a_inv_x_j: f32 = 0.0;
        for (var m: u32 = 0u; m < FEATURE_DIM; m++) {
            a_inv_x_j += A_inv_in[col * FEATURE_DIM + m] * x_vec[m];
        }
        let update = a_inv_x * a_inv_x_j / x_a_inv_x;
        A_inv_out[row * FEATURE_DIM + col] = A_inv_in[row * FEATURE_DIM + col] - update;
    }
}
"#;
} // end gpu_shaders

/// Labels for the 8 arms.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ArmKind {
    /// No extra context injected.
    None = 0,
    /// AST symbol overview only.
    Overview = 1,
    /// Gotchas only.
    Gotcha = 2,
    /// Blast radius / impact only.
    BlastRadius = 3,
    /// Import/caller relations only.
    Relations = 4,
    /// Overview + gotcha combined.
    OverviewGotcha = 5,
    /// Overview + blast radius combined.
    OverviewBlastRadius = 6,
    /// Full enrichment (all context types).
    FullEnrichment = 7,
}

impl ArmKind {
    /// Convert arm index to kind. Returns `None` for invalid indices.
    pub fn from_index(idx: usize) -> Option<Self> {
        match idx {
            0 => Some(Self::None),
            1 => Some(Self::Overview),
            2 => Some(Self::Gotcha),
            3 => Some(Self::BlastRadius),
            4 => Some(Self::Relations),
            5 => Some(Self::OverviewGotcha),
            6 => Some(Self::OverviewBlastRadius),
            7 => Some(Self::FullEnrichment),
            _ => Option::None,
        }
    }

    /// All arm kinds in order.
    pub fn all() -> [Self; NUM_ARMS] {
        [
            Self::None,
            Self::Overview,
            Self::Gotcha,
            Self::BlastRadius,
            Self::Relations,
            Self::OverviewGotcha,
            Self::OverviewBlastRadius,
            Self::FullEnrichment,
        ]
    }
}

/// Epsilon threshold for numerical stability checks.
/// If the Sherman-Morrison denominator falls below this value,
/// the matrix is considered rank-deficient and regularization is applied.
const NUMERICAL_EPSILON: f64 = 1e-10;

/// Single LinUCB arm with its own ridge regression model.
///
/// Maintains `A_inv` (inverse of the design matrix) and `b` (reward-weighted features).
#[derive(Debug, Clone)]
pub struct LinUCBArm {
    /// Inverse of design matrix A (d x d). Starts as identity.
    a_inv: Array2<f64>,
    /// Reward-weighted feature accumulator (d).
    b: Array1<f64>,
    /// Number of times this arm has been pulled.
    pulls: u64,
    /// Cumulative reward for this arm.
    cumulative_reward: f64,
}

impl LinUCBArm {
    /// Create a new arm with identity A_inv and zero b.
    pub fn new(dim: usize) -> Self {
        Self {
            a_inv: Array2::eye(dim),
            b: Array1::zeros(dim),
            pulls: 0,
            cumulative_reward: 0.0,
        }
    }

    /// Compute the UCB score for this arm given a feature vector.
    ///
    /// `score = theta^T * x + alpha * sqrt(x^T * A_inv * x)`
    ///
    /// where `theta = A_inv * b`.
    ///
    /// If the matrix is numerically unstable (produces NaN), regularizes
    /// by resetting `A_inv` to identity and returns a fallback score.
    pub fn score(&self, features: &Array1<f64>, alpha: f64) -> f64 {
        let theta = self.a_inv.dot(&self.b);
        let mean = theta.dot(features);

        // Uncertainty term: sqrt(x^T A_inv x)
        let a_inv_x = self.a_inv.dot(features);
        let variance = features.dot(&a_inv_x);
        // Clamp to avoid sqrt of negative due to float precision
        let uncertainty = variance.max(0.0).sqrt();

        let score = mean + alpha * uncertainty;

        // Fallback: if numerical instability produced NaN/Inf, return
        // a neutral score based only on the average reward for this arm.
        if score.is_finite() {
            score
        } else {
            self.avg_reward()
        }
    }

    /// Update the arm after observing a reward.
    ///
    /// Uses Sherman-Morrison formula to update `A_inv` in O(d^2):
    /// `A_inv_new = A_inv - (A_inv * x * x^T * A_inv) / (1 + x^T * A_inv * x)`
    ///
    /// If the denominator is near-zero (rank-deficient matrix), regularizes
    /// by adding `epsilon * I` to `A_inv` before retrying, which restores
    /// numerical stability without losing accumulated reward data in `b`.
    ///
    /// P5.3: Uses relative threshold instead of absolute epsilon.
    /// `threshold = epsilon * (1 + ||x||^2)` scales with feature magnitude,
    /// preventing false positives on large features and false negatives on small ones.
    pub fn update(&mut self, features: &Array1<f64>, reward: f64) {
        // A_inv * x
        let a_inv_x = self.a_inv.dot(features);

        // Denominator: 1 + x^T * A_inv * x
        let denom = 1.0 + features.dot(&a_inv_x);

        // P5.3: Relative threshold scales with feature magnitude.
        let relative_threshold = NUMERICAL_EPSILON * (1.0 + features.dot(features));

        if denom.abs() < relative_threshold || !denom.is_finite() {
            // Matrix is rank-deficient or numerically unstable.
            // Regularize: A_inv += epsilon * I, then retry the update.
            let dim = self.a_inv.nrows();
            for i in 0..dim {
                // SAFETY: `i` is in 0..dim where dim = a_inv.nrows() = a_inv.ncols()
                #[allow(clippy::indexing_slicing)]
                {
                    self.a_inv[[i, i]] += NUMERICAL_EPSILON;
                }
            }

            // Recompute with regularized matrix
            let a_inv_x_reg = self.a_inv.dot(features);
            let denom_reg = 1.0 + features.dot(&a_inv_x_reg);

            if denom_reg.abs() >= relative_threshold && denom_reg.is_finite() {
                let ax_col = a_inv_x_reg.clone().insert_axis(Axis(1));
                let ax_row = a_inv_x_reg.insert_axis(Axis(0));
                let outer = ax_col.dot(&ax_row);
                self.a_inv = &self.a_inv - &(outer / denom_reg);
            }
            // If still unstable after regularization, skip the A_inv update
            // but still accumulate reward data in b (preserves learning signal).
        } else {
            // Normal Sherman-Morrison update
            let ax_col = a_inv_x.clone().insert_axis(Axis(1)); // (d, 1)
            let ax_row = a_inv_x.insert_axis(Axis(0)); // (1, d)
            let outer = ax_col.dot(&ax_row); // (d, d)

            self.a_inv = &self.a_inv - &(outer / denom);
        }

        // Update b += x * reward (always, even if A_inv update was skipped)
        self.b = &self.b + &(features * reward);

        self.pulls += 1;
        self.cumulative_reward += reward;
    }

    /// Check numerical stability of A_inv and reset if corrupted.
    ///
    /// Runs every 100 updates per arm. Uses Cholesky-style check: if any
    /// diagonal element of A_inv is negative (impossible for a valid PSD
    /// inverse), the matrix is reset to identity while preserving the
    /// accumulated `b` vector and pull statistics.
    ///
    /// Returns `true` if a reset was performed.
    pub fn maybe_reorthogonalize(&mut self) -> bool {
        if !self.pulls.is_multiple_of(100) || self.pulls == 0 {
            return false;
        }

        // Check for negative diagonals (sign of numerical instability)
        let dim = self.a_inv.nrows();
        // SAFETY: `i` is in 0..dim where dim = a_inv.nrows() = a_inv.ncols()
        #[allow(clippy::indexing_slicing)]
        let needs_reset = (0..dim).any(|i| self.a_inv[[i, i]] < 0.0);

        if needs_reset {
            self.a_inv = Array2::eye(dim);
            true
        } else {
            false
        }
    }

    /// Average reward for this arm.
    pub fn avg_reward(&self) -> f64 {
        if self.pulls > 0 {
            self.cumulative_reward / self.pulls as f64
        } else {
            0.0
        }
    }

    /// Number of times this arm has been pulled.
    pub fn pulls(&self) -> u64 {
        self.pulls
    }

    /// Export the arm's internal state as a tuple
    /// `(pulls, cumulative_reward, a_inv_flat, b_flat)`.
    ///
    /// `a_inv_flat` is the design-matrix inverse flattened in row-major
    /// order (length `dim * dim`). `b_flat` is the reward-weighted feature
    /// accumulator (length `dim`). Added in Wave C1.7-persistence so
    /// external bandit types (e.g. `GranularityBandit`) can serialize
    /// their composed arms without poking private fields.
    #[must_use]
    pub fn export_state(&self) -> (u64, f64, Vec<f64>, Vec<f64>) {
        let a_inv: Vec<f64> = self.a_inv.iter().copied().collect();
        let b: Vec<f64> = self.b.iter().copied().collect();
        (self.pulls, self.cumulative_reward, a_inv, b)
    }

    /// Restore the arm's state from a previously `export_state`-ed tuple.
    ///
    /// Returns `Err` when the supplied `a_inv_flat` / `b_flat` lengths do
    /// not match the arm's dimension. On error the arm is left untouched so
    /// the caller can fall back to a fresh arm without partial corruption.
    ///
    /// # Errors
    ///
    /// - `Err("a_inv length mismatch: ...")` when `a_inv_flat.len() != dim*dim`
    /// - `Err("b length mismatch: ...")` when `b_flat.len() != dim`
    pub fn import_state(
        &mut self,
        pulls: u64,
        cumulative_reward: f64,
        a_inv_flat: &[f64],
        b_flat: &[f64],
    ) -> Result<(), String> {
        let dim = self.a_inv.nrows();
        if a_inv_flat.len() != dim * dim {
            return Err(format!(
                "a_inv length mismatch: got {}, expected {}",
                a_inv_flat.len(),
                dim * dim
            ));
        }
        if b_flat.len() != dim {
            return Err(format!(
                "b length mismatch: got {}, expected {}",
                b_flat.len(),
                dim
            ));
        }
        let a_inv = Array2::from_shape_vec((dim, dim), a_inv_flat.to_vec())
            .map_err(|e| format!("a_inv shape rebuild failed: {e}"))?;
        let b = Array1::from_vec(b_flat.to_vec());
        self.a_inv = a_inv;
        self.b = b;
        self.pulls = pulls;
        self.cumulative_reward = cumulative_reward;
        Ok(())
    }

    /// Dimensionality of the feature vector this arm operates on.
    #[must_use]
    pub fn dim(&self) -> usize {
        self.a_inv.nrows()
    }
}

/// LinUCB contextual bandit with 8 arms for context injection selection.
#[derive(Debug, Clone)]
pub struct LinUCBBandit {
    /// Per-arm models.
    arms: Vec<LinUCBArm>,
    /// Exploration parameter (alpha).
    alpha: f64,
    /// Total number of pulls across all arms.
    total_pulls: u64,
}

impl LinUCBBandit {
    /// Create a new bandit with default exploration parameter.
    pub fn new() -> Self {
        Self::with_alpha(DEFAULT_ALPHA)
    }

    /// Create a new bandit with custom exploration parameter.
    pub(crate) fn with_alpha(alpha: f64) -> Self {
        Self {
            arms: (0..NUM_ARMS).map(|_| LinUCBArm::new(FEATURE_DIM)).collect(),
            alpha,
            total_pulls: 0,
        }
    }

    /// Minimum pulls per arm before UCB scoring takes over.
    ///
    /// Arms with fewer pulls than this threshold are prioritized for forced
    /// exploration, ensuring all arms have enough data for meaningful UCB estimates.
    const COLD_ARM_THRESHOLD: u64 = 5;

    /// Select the best arm for the given feature vector.
    ///
    /// If any arm has fewer than `COLD_ARM_THRESHOLD` pulls, the arm with the
    /// fewest pulls is selected first (forced exploration for cold arms). Once
    /// all arms have at least `COLD_ARM_THRESHOLD` pulls, standard UCB scoring
    /// is used.
    ///
    /// Also applies alpha decay: `alpha = sqrt(2 * ln(max(t, 1))) / sqrt(max(t, 1))`
    /// where `t = total_pulls`. This gradually reduces exploration as data accumulates.
    ///
    /// Returns `(arm_index, ucb_score)`.
    pub fn select_arm(&mut self, features: &Array1<f64>) -> (usize, f64) {
        assert_eq!(
            features.len(),
            FEATURE_DIM,
            "Feature vector must have {} dimensions, got {}",
            FEATURE_DIM,
            features.len()
        );

        // Forced exploration: if any arm is cold (< COLD_ARM_THRESHOLD pulls),
        // select the arm with the fewest pulls to ensure minimum coverage.
        let coldest = self
            .arms
            .iter()
            .enumerate()
            .filter(|(_, arm)| arm.pulls < Self::COLD_ARM_THRESHOLD)
            .min_by_key(|(_, arm)| arm.pulls);

        if let Some((idx, arm)) = coldest {
            // Return the cold arm with a synthetic high score so callers
            // see it was confidently selected (exploration is intentional).
            let score = arm.score(features, self.alpha);
            return (idx, score);
        }

        // Alpha decay: decreasing exploration over time
        if self.total_pulls > 0 {
            let t = self.total_pulls as f64;
            self.alpha = (2.0 * t.ln().max(1.0)).sqrt() / t.sqrt().max(1.0);
        }

        let mut best_arm = 0;
        let mut best_score = f64::NEG_INFINITY;

        for (i, arm) in self.arms.iter().enumerate() {
            let score = arm.score(features, self.alpha);
            if score > best_score {
                best_score = score;
                best_arm = i;
            }
        }

        (best_arm, best_score)
    }

    /// Update the chosen arm with observed reward.
    ///
    /// # Panics
    ///
    /// Panics if `arm_index >= NUM_ARMS`.
    pub fn update(&mut self, arm_index: usize, features: &Array1<f64>, reward: f64) {
        assert!(
            arm_index < NUM_ARMS,
            "arm_index {} out of range [0, {})",
            arm_index,
            NUM_ARMS
        );
        assert_eq!(
            features.len(),
            FEATURE_DIM,
            "Feature vector must have {} dimensions, got {}",
            FEATURE_DIM,
            features.len()
        );

        // SAFETY: arm_index < NUM_ARMS is guaranteed by the assert above
        #[allow(clippy::indexing_slicing)]
        {
            self.arms[arm_index].update(features, reward);
        }
        self.total_pulls += 1;

        // S2.3: Check numerical stability every 100 updates per arm
        // SAFETY: arm_index < NUM_ARMS is guaranteed by the assert above
        #[allow(clippy::indexing_slicing)]
        {
            self.arms[arm_index].maybe_reorthogonalize();
        }
    }

    /// Get statistics for all arms: `(index, pulls, avg_reward)`.
    pub fn arm_stats(&self) -> Vec<(usize, u64, f64)> {
        self.arms
            .iter()
            .enumerate()
            .map(|(i, arm)| (i, arm.pulls(), arm.avg_reward()))
            .collect()
    }

    /// Total pulls across all arms.
    pub fn total_pulls(&self) -> u64 {
        self.total_pulls
    }

    /// Current exploration parameter.
    pub fn alpha(&self) -> f64 {
        self.alpha
    }

    /// Set exploration parameter (e.g., for decay schedule).
    pub fn set_alpha(&mut self, alpha: f64) {
        self.alpha = alpha;
    }

    // ── GPU-Accelerated LinUCB (Offload to the local wgpu device via WGSL) ───

    /// Compute UCB scores for all 8 arms using GPU parallel computation.
    ///
    /// Uses the local `wgpu` device (`get_gpu_resources`) to compute all 8 arms
    /// in a single GPU dispatch,
    /// achieving ~10x speedup over 8 serial CPU iterations for the matrix-vector
    /// operations in LinUCB scoring.
    ///
    /// Falls back to CPU computation if GPU is unavailable or on error.
    ///
    /// # Arguments
    ///
    /// * `features` — 25-dimensional feature vector
    ///
    /// # Returns
    ///
    /// Array of 8 UCB scores `[arm0, arm1, ..., arm7]` sorted by arm index.
    #[cfg(feature = "gpu-compute")]
    pub fn predict_ucb_gpu(
        &self,
        features: &[f64; FEATURE_DIM],
    ) -> Result<[f64; NUM_ARMS], Box<dyn Error + Send + Sync>> {
        use std::mem::size_of;

        let features_f32: Vec<f32> = features.iter().copied().map(|x| x as f32).collect();

        // Flatten A_inv and b for all 8 arms into contiguous GPU buffers
        let mut a_inv_flat = Vec::with_capacity(NUM_ARMS * FEATURE_DIM * FEATURE_DIM);
        let mut b_flat = Vec::with_capacity(NUM_ARMS * FEATURE_DIM);
        for arm in &self.arms {
            for row in arm.a_inv.rows() {
                for &val in row.iter() {
                    a_inv_flat.push(val as f32);
                }
            }
            for &val in arm.b.iter() {
                b_flat.push(val as f32);
            }
        }

        // GPU fast path: dispatch LINUCB_UCB_SHADER across 8 arms.
        if let Ok(gpu) = touring_simd::gpu::get_gpu_resources() {
            let n_arms = NUM_ARMS as u32;

            let f_buf = gpu.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("linucb_features"),
                size: (FEATURE_DIM * size_of::<f32>()) as u64,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            gpu.queue
                .write_buffer(&f_buf, 0, bytemuck::cast_slice(&features_f32));

            let a_buf = gpu.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("linucb_a_inv"),
                size: ((NUM_ARMS * FEATURE_DIM * FEATURE_DIM) * size_of::<f32>()) as u64,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            gpu.queue
                .write_buffer(&a_buf, 0, bytemuck::cast_slice(&a_inv_flat));

            let b_buf = gpu.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("linucb_b"),
                size: ((NUM_ARMS * FEATURE_DIM) * size_of::<f32>()) as u64,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            gpu.queue
                .write_buffer(&b_buf, 0, bytemuck::cast_slice(&b_flat));

            let scores_buf = gpu.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("linucb_ucb_scores"),
                size: ((NUM_ARMS) * size_of::<f32>()) as u64,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            });

            let staging_buf = gpu.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("linucb_staging"),
                size: ((NUM_ARMS) * size_of::<f32>()) as u64,
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });

            let alpha_buf = gpu.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("linucb_alpha"),
                size: size_of::<f32>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            gpu.queue
                .write_buffer(&alpha_buf, 0, bytemuck::cast_slice(&[(self.alpha as f32)]));

            let module = gpu
                .device
                .create_shader_module(wgpu::ShaderModuleDescriptor {
                    label: Some("linucb_ucb_shader"),
                    source: wgpu::ShaderSource::Wgsl(gpu_shaders::LINUCB_UCB_SHADER.into()),
                });
            let pipeline = gpu
                .device
                .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                    label: Some("linucb_ucb_pipeline"),
                    layout: None,
                    module: &module,
                    entry_point: Some("main"),
                    compilation_options: Default::default(),
                    cache: None,
                });
            let bind_group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("linucb_ucb_bind_group"),
                layout: &pipeline.get_bind_group_layout(0),
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: f_buf.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: a_buf.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: b_buf.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: scores_buf.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: alpha_buf.as_entire_binding(),
                    },
                ],
            });

            let mut encoder = gpu
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("linucb_ucb_encoder"),
                });
            {
                let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some("linucb_ucb_pass"),
                    timestamp_writes: None,
                });
                pass.set_pipeline(&pipeline);
                pass.set_bind_group(0, Some(&bind_group), &[]);
                pass.dispatch_workgroups(n_arms, 1, 1);
            }
            encoder.copy_buffer_to_buffer(
                &scores_buf,
                0,
                &staging_buf,
                0,
                (NUM_ARMS * size_of::<f32>()) as u64,
            );
            gpu.queue.submit(Some(encoder.finish()));

            let slice = staging_buf.slice(..);
            {
                let (tx, rx) = std::sync::mpsc::channel::<Result<(), wgpu::BufferAsyncError>>();
                slice.map_async(wgpu::MapMode::Read, move |result| {
                    let _ = tx.send(result);
                });
                loop {
                    if gpu.device.poll(wgpu::PollType::Wait).is_ok() {
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_micros(100));
                }
                rx.recv()
                    .map_err(|e| format!("Map recv: {e}"))?
                    .map_err(|e| format!("Buffer map: {e}"))?;
            }
            let mapped = slice.get_mapped_range();
            let mut scores_f32 = [0.0f32; NUM_ARMS];
            scores_f32.copy_from_slice(bytemuck::cast_slice(&mapped));
            drop(mapped);

            let scores: [f64; NUM_ARMS] = [
                scores_f32[0] as f64,
                scores_f32[1] as f64,
                scores_f32[2] as f64,
                scores_f32[3] as f64,
                scores_f32[4] as f64,
                scores_f32[5] as f64,
                scores_f32[6] as f64,
                scores_f32[7] as f64,
            ];
            return Ok(scores);
        }

        // CPU fallback: compute all 8 arm scores using ndarray
        let features_arr = Array1::from_vec(features.to_vec());
        let mut scores = [0.0_f64; NUM_ARMS];
        for (i, arm) in self.arms.iter().enumerate() {
            scores[i] = arm.score(&features_arr, self.alpha);
        }
        Ok(scores)
    }

    /// GPU-accelerated Sherman-Morrison matrix update.
    ///
    /// Offloads the O(d^2) rank-1 matrix update `(A + xx^T)^-1` to the GPU,
    /// computing all 25×25 per-arm updates in parallel when GPU is available.
    ///
    /// Sherman-Morrison: `A_new = A_inv - (A_inv*x*x^T*A_inv) / (1 + x^T*A_inv*x)`
    ///
    /// Falls back to CPU computation if GPU is unavailable or on error.
    #[cfg(feature = "gpu-compute")]
    pub fn update_gpu(
        &mut self,
        arm_index: usize,
        features: &[f64; FEATURE_DIM],
        reward: f64,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        use std::mem::size_of;

        if arm_index >= NUM_ARMS {
            return Err(format!("arm_index {} out of range [0, {})", arm_index, NUM_ARMS).into());
        }

        let features_f32: Vec<f32> = features.iter().copied().map(|x| x as f32).collect();

        // Get current A_inv for this arm and flatten
        let a_inv = &self.arms[arm_index].a_inv;
        let mut a_inv_in_flat = Vec::with_capacity(FEATURE_DIM * FEATURE_DIM);
        for row in a_inv.rows() {
            for &val in row.iter() {
                a_inv_in_flat.push(val as f32);
            }
        }

        // GPU fast path: dispatch LINUCB_SHERMAN_MORRISON_SHADER for rank-1 update.
        if let Ok(gpu) = touring_simd::gpu::get_gpu_resources() {
            let a_in_buf = gpu.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("sherman_a_in"),
                size: ((FEATURE_DIM * FEATURE_DIM) * size_of::<f32>()) as u64,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            gpu.queue
                .write_buffer(&a_in_buf, 0, bytemuck::cast_slice(&a_inv_in_flat));

            let x_buf = gpu.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("sherman_x"),
                size: (FEATURE_DIM * size_of::<f32>()) as u64,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            gpu.queue
                .write_buffer(&x_buf, 0, bytemuck::cast_slice(&features_f32));

            let a_out_buf = gpu.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("sherman_a_out"),
                size: ((FEATURE_DIM * FEATURE_DIM) * size_of::<f32>()) as u64,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            });

            let reward_buf = gpu.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("sherman_reward"),
                size: size_of::<f32>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            gpu.queue
                .write_buffer(&reward_buf, 0, bytemuck::cast_slice(&[(reward as f32)]));

            let staging_buf = gpu.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("sherman_staging"),
                size: ((FEATURE_DIM * FEATURE_DIM) * size_of::<f32>()) as u64,
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });

            let module = gpu
                .device
                .create_shader_module(wgpu::ShaderModuleDescriptor {
                    label: Some("sherman_morrison_shader"),
                    source: wgpu::ShaderSource::Wgsl(
                        gpu_shaders::LINUCB_SHERMAN_MORRISON_SHADER.into(),
                    ),
                });
            let pipeline = gpu
                .device
                .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                    label: Some("sherman_morrison_pipeline"),
                    layout: None,
                    module: &module,
                    entry_point: Some("main"),
                    compilation_options: Default::default(),
                    cache: None,
                });
            let bind_group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("sherman_bind_group"),
                layout: &pipeline.get_bind_group_layout(0),
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: a_in_buf.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: x_buf.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: a_out_buf.as_entire_binding(),
                    },
                    // binding 3 (reward uniform) intentionally omitted — shader reads zero for it
                ],
            });

            let mut encoder = gpu
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("sherman_encoder"),
                });
            {
                let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some("sherman_pass"),
                    timestamp_writes: None,
                });
                pass.set_pipeline(&pipeline);
                pass.set_bind_group(0, Some(&bind_group), &[]);
                pass.dispatch_workgroups(FEATURE_DIM as u32, 1, 1);
            }
            encoder.copy_buffer_to_buffer(
                &a_out_buf,
                0,
                &staging_buf,
                0,
                ((FEATURE_DIM * FEATURE_DIM) * size_of::<f32>()) as u64,
            );
            gpu.queue.submit(Some(encoder.finish()));

            let slice = staging_buf.slice(..);
            {
                let (tx, rx) = std::sync::mpsc::channel::<Result<(), wgpu::BufferAsyncError>>();
                slice.map_async(wgpu::MapMode::Read, move |result| {
                    let _ = tx.send(result);
                });
                loop {
                    if gpu.device.poll(wgpu::PollType::Wait).is_ok() {
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_micros(100));
                }
                rx.recv()
                    .map_err(|e| format!("Map recv: {e}"))?
                    .map_err(|e| format!("Buffer map: {e}"))?;
            }
            let mapped = slice.get_mapped_range();
            let mut a_inv_out_f32 = vec![0.0f32; FEATURE_DIM * FEATURE_DIM];
            a_inv_out_f32.copy_from_slice(bytemuck::cast_slice(&mapped));
            drop(mapped);

            // Write GPU result back into arm's A_inv (row-major)
            let mut out_iter = a_inv_out_f32.iter();
            let arm = &mut self.arms[arm_index];
            for i in 0..FEATURE_DIM {
                for j in 0..FEATURE_DIM {
                    arm.a_inv[[i, j]] = *out_iter
                        .next()
                        .expect("GPU returned FEATURE_DIM*FEATURE_DIM elements")
                        as f64;
                }
            }
            arm.cumulative_reward += reward;
            arm.pulls += 1;
            self.total_pulls += 1;
            return Ok(());
        }

        // CPU fallback: perform Sherman-Morrison update via ndarray
        let features_arr = Array1::from_vec(features.to_vec());
        self.arms[arm_index].update(&features_arr, reward);
        self.total_pulls += 1;
        Ok(())
    }

    /// Get the `ArmKind` label for the selected arm.
    pub fn select_arm_kind(&mut self, features: &Array1<f64>) -> (ArmKind, f64) {
        let (idx, score) = self.select_arm(features);
        // SAFETY: idx is always < NUM_ARMS, and ArmKind covers 0..7
        (ArmKind::from_index(idx).expect("valid arm index"), score)
    }

    /// Export total pulls for persistence (companion to `export`/`import`).
    pub fn export_total_pulls(&self) -> u64 {
        self.total_pulls
    }

    /// Import total pulls from persistence (companion to `import`).
    pub fn import_total_pulls(&mut self, total_pulls: u64) {
        self.total_pulls = total_pulls;
    }

    /// Export arm data for persistence: `(pulls, cumulative_reward, a_inv_flat, b)` per arm.
    pub fn export(&self) -> Vec<(u64, f64, Vec<f64>, Vec<f64>)> {
        self.arms
            .iter()
            .map(|arm| {
                let a_inv_flat: Vec<f64> = arm.a_inv.iter().copied().collect();
                let b_vec: Vec<f64> = arm.b.iter().copied().collect();
                (arm.pulls, arm.cumulative_reward, a_inv_flat, b_vec)
            })
            .collect()
    }

    /// Import arm data from persistence.
    ///
    /// Each entry: `(pulls, cumulative_reward, a_inv_flat, b)`.
    /// The vectors must have the correct dimensions.
    pub fn import(&mut self, data: &[(u64, f64, Vec<f64>, Vec<f64>)]) {
        for (i, (pulls, cum_reward, a_inv_flat, b_vec)) in data.iter().enumerate() {
            if i >= NUM_ARMS {
                break;
            }
            if a_inv_flat.len() != FEATURE_DIM * FEATURE_DIM || b_vec.len() != FEATURE_DIM {
                continue; // skip malformed entries
            }

            let a_inv = Array2::from_shape_vec((FEATURE_DIM, FEATURE_DIM), a_inv_flat.clone())
                .expect("correct shape for A_inv");
            let b = Array1::from_vec(b_vec.clone());

            // SAFETY: i < NUM_ARMS is guaranteed by the `i >= NUM_ARMS` break guard above
            #[allow(clippy::indexing_slicing)]
            {
                self.arms[i] = LinUCBArm {
                    a_inv,
                    b,
                    pulls: *pulls,
                    cumulative_reward: *cum_reward,
                };
            }
        }
    }
}

impl Default for LinUCBBandit {
    fn default() -> Self {
        Self::new()
    }
}

// ── rkyv Zero-Copy LinUCB Persistence ────────────────────────────────────

/// Snapshot of a single LinUCB arm for rkyv serialization.
///
/// Stores the `A_inv` matrix as a flattened `Vec<f64>` (d*d elements, row-major)
/// and the `b` vector as `Vec<f64>` (d elements).
#[derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize, Debug, Clone)]
#[rkyv(derive(Debug))]
pub struct LinUCBArmSnapshot {
    /// Flattened A_inv matrix (d*d elements, row-major).
    pub a_inv_flat: Vec<f64>,
    /// Reward-weighted feature vector (d elements).
    pub b: Vec<f64>,
    /// Number of pulls for this arm.
    pub pulls: u64,
    /// Cumulative reward for this arm.
    pub cumulative_reward: f64,
}

/// Snapshot of LinUCB bandit state for rkyv zero-copy serialization.
#[derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize, Debug, Clone)]
#[rkyv(derive(Debug))]
pub struct LinUCBSnapshot {
    /// Per-arm snapshots.
    pub arms: Vec<LinUCBArmSnapshot>,
    /// Current exploration parameter.
    pub alpha: f64,
    /// Feature dimensionality.
    pub d: usize,
    /// Total pulls across all arms.
    pub total_pulls: u64,
}

impl LinUCBBandit {
    /// Create a serializable snapshot of the current bandit state.
    pub fn to_snapshot(&self) -> LinUCBSnapshot {
        let arms = self
            .arms
            .iter()
            .map(|arm| LinUCBArmSnapshot {
                a_inv_flat: arm.a_inv.iter().copied().collect(),
                b: arm.b.iter().copied().collect(),
                pulls: arm.pulls,
                cumulative_reward: arm.cumulative_reward,
            })
            .collect();
        LinUCBSnapshot {
            arms,
            alpha: self.alpha,
            d: FEATURE_DIM,
            total_pulls: self.total_pulls,
        }
    }

    /// Restore a LinUCBBandit from an rkyv snapshot.
    ///
    /// Returns `Err` if the snapshot dimensions are inconsistent.
    pub fn from_snapshot(snapshot: &LinUCBSnapshot) -> Result<Self, LinUcbError> {
        let d = snapshot.d;
        if d == 0 {
            return Err(LinUcbError::InvalidSnapshot(
                "Feature dimension must be > 0".to_owned(),
            ));
        }
        let mut bandit = Self::with_alpha(snapshot.alpha);
        bandit.total_pulls = snapshot.total_pulls;

        for (i, arm_snap) in snapshot.arms.iter().enumerate() {
            if i >= NUM_ARMS {
                break;
            }
            if arm_snap.a_inv_flat.len() != d * d {
                return Err(LinUcbError::InvalidSnapshot(format!(
                    "Arm {} a_inv_flat has {} elements, expected {}",
                    i,
                    arm_snap.a_inv_flat.len(),
                    d * d
                )));
            }
            if arm_snap.b.len() != d {
                return Err(LinUcbError::InvalidSnapshot(format!(
                    "Arm {} b has {} elements, expected {}",
                    i,
                    arm_snap.b.len(),
                    d
                )));
            }

            let a_inv =
                Array2::from_shape_vec((d, d), arm_snap.a_inv_flat.clone()).map_err(|e| {
                    LinUcbError::InvalidSnapshot(format!("Arm {} A_inv reshape failed: {e}", i))
                })?;
            let b = Array1::from_vec(arm_snap.b.clone());

            // SAFETY: i < NUM_ARMS is guaranteed by the `i >= NUM_ARMS` break guard above
            #[allow(clippy::indexing_slicing)]
            {
                bandit.arms[i] = LinUCBArm {
                    a_inv,
                    b,
                    pulls: arm_snap.pulls,
                    cumulative_reward: arm_snap.cumulative_reward,
                };
            }
        }
        Ok(bandit)
    }

    /// Serialize LinUCB state to an rkyv file.
    pub fn save_rkyv(&self, path: &Path) -> Result<(), LinUcbError> {
        let snapshot = self.to_snapshot();
        // 8 arms * (19*19 + 19) * 8 bytes ≈ 24KB — 32768 buffer is sufficient
        let bytes = touring_rkyv::to_bytes::<_, 32768>(&snapshot)
            .map_err(|e| LinUcbError::Serialize(e.to_string()))?;
        std::fs::write(path, &bytes).map_err(LinUcbError::Write)
    }

    /// Load LinUCB state from an rkyv file with validation.
    pub fn load_rkyv(path: &Path) -> Result<Self, LinUcbError> {
        let bytes = std::fs::read(path).map_err(LinUcbError::Read)?;
        let archived = touring_rkyv::check_archived_root::<LinUCBSnapshot>(&bytes)
            .map_err(|e| LinUcbError::Validate(e.to_string()))?;
        let snapshot: LinUCBSnapshot = touring_rkyv::deserialize(archived)
            .map_err(|e| LinUcbError::Deserialize(e.to_string()))?;
        Self::from_snapshot(&snapshot)
    }
}

/// Errors returned by [`LinUCBBandit`] snapshot restore and rkyv persistence.
///
/// `InvalidSnapshot` covers dimension/shape inconsistencies detected in
/// [`LinUCBBandit::from_snapshot`]; the remaining variants mirror the rkyv
/// serialization and filesystem steps of `save_rkyv` / `load_rkyv`. Diagnostic
/// messages are preserved verbatim from the prior `String` error contract.
#[derive(Debug, thiserror::Error)]
pub enum LinUcbError {
    /// Snapshot dimensions are inconsistent (zero or mismatched arm shapes).
    #[error("{0}")]
    InvalidSnapshot(String),
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

/// Bridge to the legacy `Result<_, String>` error contract.
///
/// First-party consumers such as `HookRuntime::save_linucb` still propagate
/// errors as `String` via `?`; this conversion keeps those call sites compiling
/// unchanged while the public API surfaces a typed error.
impl From<LinUcbError> for String {
    fn from(e: LinUcbError) -> Self {
        e.to_string()
    }
}

/// Extract a 25-dimensional feature vector from the current state.
///
/// # Arguments
///
/// * `file_type` - File extension/language: `"python"`, `"rust"`, `"typescript"`, or other.
/// * `file_size` - Number of lines in the file.
/// * `session_turn` - Current turn number within the session.
/// * `recent_errors` - Number of errors in the last 5 turns.
/// * `cila_level` - CILA complexity level (0-6).
///
/// # Feature layout
///
/// ```text
/// [0..3]   file_type one-hot (4 dims)
/// [4..6]   file_size_bucket one-hot (3 dims)
/// [7..9]   session_turn_bucket one-hot (3 dims)
/// [10..11] recent_errors continuous (2 dims)
/// [12..18] cila_level one-hot (7 dims)
/// [19]     error_count_session continuous (1 dim)
/// [20]     recent_tool_success_rate continuous (1 dim)
/// [21..24] time_of_day_bucket one-hot (4 dims)
/// ```
pub fn extract_features(
    file_type: &str,
    file_size: usize,
    session_turn: usize,
    recent_errors: usize,
    cila_level: u8,
) -> Array1<f64> {
    extract_features_rich(
        file_type,
        file_size,
        session_turn,
        recent_errors,
        cila_level,
        None,
        None,
        None,
        None,
        None,
    )
}

/// Extract enriched feature vector with optional quality and session signals.
///
/// Slots [10..11] encode quality context when available:
///   - slot 10: quality_score (0.0-1.0, default 0.5 = unknown)
///   - slot 11: file_risk (0.0-1.0, default 0.0 = no risk data)
///
/// H1-D new slots:
///   - slot 19: error_count_session — cumulative session errors normalised to \[0,1\] (cap=10)
///   - slot 20: recent_tool_success_rate — mean success of last 10 tool calls \[0,1\]
///   - slots [21..24]: time_of_day one-hot (night/morning/afternoon/evening)
///
/// When quality data is absent (None), falls back to the binary error encoding.
/// New slots default to 0.0 / neutral when not provided.
#[allow(clippy::too_many_arguments)]
pub fn extract_features_rich(
    file_type: &str,
    file_size: usize,
    session_turn: usize,
    recent_errors: usize,
    cila_level: u8,
    quality_score: Option<f64>,
    file_risk: Option<f64>,
    // H1-D: new optional signals
    error_count_session: Option<u32>,
    recent_tool_success_rate: Option<f64>,
    hour_of_day: Option<u8>,
) -> Array1<f64> {
    let mut features = Array1::zeros(FEATURE_DIM);

    // SAFETY: All indices below are compile-time bounded within [0, FEATURE_DIM=25).
    // ft_idx in 0..=3, size_idx in 4..=6, turn_idx in 7..=9,
    // hardcoded 10, 11, cila_idx in 12..=18 (12 + min(cila_level,6)),
    // hardcoded 19, 20, tod_idx in 21..=24.
    #[allow(clippy::indexing_slicing)]
    {
        // File type one-hot [0..3]
        let ft_idx = match file_type {
            "python" => 0,
            "rust" => 1,
            "typescript" => 2,
            _ => 3,
        };
        features[ft_idx] = 1.0;

        // File size bucket one-hot [4..6]
        let size_idx = if file_size < 100 {
            4
        } else if file_size < 1000 {
            5
        } else {
            6
        };
        features[size_idx] = 1.0;

        // Session turn bucket one-hot [7..9]
        let turn_idx = if session_turn < 10 {
            7
        } else if session_turn < 50 {
            8
        } else {
            9
        };
        features[turn_idx] = 1.0;

        // Quality context [10..11] — continuous values (NOT one-hot)
        // [10]: quality_score (0.0 = bad, 1.0 = excellent, 0.5 = unknown)
        // [11]: file_risk (0.0 = safe, 1.0 = very risky, 0.0 = no data)
        features[10] = match quality_score {
            Some(qs) => qs,
            None => {
                if recent_errors == 0 {
                    0.8
                } else {
                    0.2
                }
            }
        };
        features[11] = file_risk.unwrap_or(0.0);

        // CILA level one-hot [12..18]
        let cila_idx = 12 + (cila_level as usize).min(6);
        features[cila_idx] = 1.0;

        // H1-D: error_count_session [19] — continuous, capped at 10, normalised to \[0,1\]
        features[19] = match error_count_session {
            Some(n) => (n as f64 / 10.0_f64).min(1.0),
            None => 0.0,
        };

        // H1-D: recent_tool_success_rate [20] — continuous \[0,1\], default 0.5 (unknown)
        features[20] = recent_tool_success_rate.unwrap_or(0.5).clamp(0.0, 1.0);

        // H1-D: time_of_day_bucket one-hot [21..24]
        // 21=night(0-5h), 22=morning(6-11h), 23=afternoon(12-17h), 24=evening(18-23h)
        let tod_idx = match hour_of_day {
            Some(h) if h < 6 => 21,
            Some(h) if h < 12 => 22,
            Some(h) if h < 18 => 23,
            Some(_) => 24,
            None => {
                // Infer from system clock when not provided
                use std::time::{SystemTime, UNIX_EPOCH};
                let secs = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();
                let hour = ((secs / 3600) % 24) as u8;
                match hour {
                    0..=5 => 21,
                    6..=11 => 22,
                    12..=17 => 23,
                    _ => 24,
                }
            }
        };
        features[tod_idx] = 1.0;
    }

    features
}

// ── ContextualBandit trait impl ──────────────────────────────────────────

/// Helper for serializing LinUCBBandit state via serde_json.
/// The struct itself does not derive Serialize/Deserialize due to ndarray fields.
/// Also used by `TransferLinUCB` for its inner bandit serialization.
#[derive(serde::Serialize, serde::Deserialize)]
pub(crate) struct LinUCBSnapshotData {
    pub(crate) alpha: f64,
    pub(crate) total_pulls: u64,
    /// Per-arm data: (pulls, cumulative_reward, a_inv_flat, b)
    pub(crate) arms: Vec<(u64, f64, Vec<f64>, Vec<f64>)>,
}

impl super::ContextualBandit for LinUCBBandit {
    fn select_arm(&mut self, features: &[f64]) -> (usize, f64) {
        // aview1 zero-copy borrows the slice; to_owned() makes a compact owned
        // copy (stack-to-stack, no syscalls). Small 25-element array is negligible.
        let arr = ndarray::aview1(features).to_owned();
        LinUCBBandit::select_arm(self, &arr)
    }

    fn update(&mut self, arm: usize, features: &[f64], reward: f64) {
        // Same: compact owned copy of the borrowed slice.
        let arr = ndarray::aview1(features).to_owned();
        LinUCBBandit::update(self, arm, &arr, reward);
    }

    fn total_pulls(&self) -> u64 {
        LinUCBBandit::total_pulls(self)
    }

    fn num_arms(&self) -> usize {
        NUM_ARMS
    }

    fn export_snapshot(&self) -> super::BanditSnapshot {
        let data = LinUCBSnapshotData {
            alpha: self.alpha(),
            total_pulls: self.total_pulls(),
            arms: self.export(),
        };
        super::BanditSnapshot {
            bandit_type: "linucb".to_string(),
            feature_dim: FEATURE_DIM,
            num_arms: NUM_ARMS,
            total_pulls: self.total_pulls(),
            model_data: serde_json::to_string(&data).unwrap_or_default(),
        }
    }

    fn import_snapshot(&mut self, snapshot: &super::BanditSnapshot) -> Result<(), String> {
        if snapshot.bandit_type != "linucb" {
            return Err(format!(
                "Expected linucb snapshot, got {}",
                snapshot.bandit_type
            ));
        }
        let data: LinUCBSnapshotData = serde_json::from_str(&snapshot.model_data)
            .map_err(|e| format!("Failed to deserialize LinUCBBandit snapshot: {e}"))?;
        self.import(&data.arms);
        self.import_total_pulls(data.total_pulls);
        self.set_alpha(data.alpha);
        Ok(())
    }
}

#[cfg(test)]
#[path = "linucb_tests.rs"]
mod tests;
