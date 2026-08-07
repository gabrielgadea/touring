//! `AstEnrichedBandit` — LinUCB with 35-dim AST-enriched features.
//!
//! A separate bandit from [`super::linucb::LinUCBBandit`] (which uses 19-dim features),
//! preserving full backward compatibility. The algorithm is identical (LinUCB with
//! Sherman-Morrison updates), but operates in a higher-dimensional feature space
//! that includes AST-derived signals.
//!
//! # Usage
//!
//! ```rust,ignore
//! use touring_intelligence::rl::bandit::ast_enriched::AstEnrichedBandit;
//!
//! let mut bandit = AstEnrichedBandit::new(1.0);
//! let base_features = [0.0f64; 19]; // from extract_features()
//! let arm = bandit.select_arm_for_file(&base_features, "src/main.rs");
//! bandit.update_from_file(arm, &base_features, "src/main.rs", 0.8);
//! ```

use ndarray::{Array1, Array2, Axis, azip};
use serde::{Deserialize, Serialize};

use super::ast_features::{FEATURE_DIM_AST, enrich_features};
use super::linucb::{ArmKind, NUM_ARMS};

/// Epsilon threshold for numerical stability (same as in linucb).
const NUMERICAL_EPSILON: f64 = 1e-10;

/// Default exploration parameter.
const DEFAULT_ALPHA: f64 = 1.0;

/// Minimum pulls per arm before UCB scoring activates.
const COLD_ARM_THRESHOLD: u64 = 5;

/// Single LinUCB arm with 35-dim feature space.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AstEnrichedArm {
    /// Inverse of design matrix A (d x d). Starts as identity.
    #[serde(with = "array2_serde")]
    a_inv: Array2<f64>,
    /// Reward-weighted feature accumulator (d).
    #[serde(with = "array1_serde")]
    b: Array1<f64>,
    /// Number of times this arm has been pulled.
    pulls: u64,
    /// Cumulative reward for this arm.
    cumulative_reward: f64,
}

impl AstEnrichedArm {
    /// Create a new arm with identity A_inv and zero b for 35-dim features.
    fn new() -> Self {
        Self {
            a_inv: Array2::eye(FEATURE_DIM_AST),
            b: Array1::zeros(FEATURE_DIM_AST),
            pulls: 0,
            cumulative_reward: 0.0,
        }
    }

    /// Compute UCB score for given features.
    ///
    /// `score = theta^T * x + alpha * sqrt(x^T * A_inv * x)`
    fn score(&self, features: &Array1<f64>, alpha: f64) -> f64 {
        let theta = self.a_inv.dot(&self.b);
        let mean = theta.dot(features);

        let a_inv_x = self.a_inv.dot(features);
        let variance = features.dot(&a_inv_x);
        let uncertainty = variance.max(0.0).sqrt();

        let score = mean + alpha * uncertainty;

        if score.is_finite() {
            score
        } else {
            self.avg_reward()
        }
    }

    /// Update arm via Sherman-Morrison after observing a reward.
    fn update(&mut self, features: &Array1<f64>, reward: f64) {
        let a_inv_x = self.a_inv.dot(features);
        let denom = 1.0 + features.dot(&a_inv_x);

        let relative_threshold = NUMERICAL_EPSILON * (1.0 + features.dot(features));

        if denom.abs() < relative_threshold || !denom.is_finite() {
            // Regularize: A_inv += epsilon * I
            let dim = self.a_inv.nrows();
            for i in 0..dim {
                #[allow(clippy::indexing_slicing)]
                {
                    self.a_inv[[i, i]] += NUMERICAL_EPSILON;
                }
            }

            let a_inv_x_reg = self.a_inv.dot(features);
            let denom_reg = 1.0 + features.dot(&a_inv_x_reg);

            if denom_reg.abs() >= relative_threshold && denom_reg.is_finite() {
                let ax_col = a_inv_x_reg.clone().insert_axis(Axis(1));
                let ax_row = a_inv_x_reg.insert_axis(Axis(0));
                let outer = ax_col.dot(&ax_row);
                self.a_inv = &self.a_inv - &(outer / denom_reg);
            }
        } else {
            let ax_col = a_inv_x.clone().insert_axis(Axis(1));
            let ax_row = a_inv_x.insert_axis(Axis(0));
            let outer = ax_col.dot(&ax_row);
            self.a_inv = &self.a_inv - &(outer / denom);
        }

        self.b = &self.b + &(features * reward);
        self.pulls += 1;
        self.cumulative_reward += reward;
    }

    /// Check and reset numerically unstable A_inv (every 100 pulls).
    fn maybe_reorthogonalize(&mut self) -> bool {
        if !self.pulls.is_multiple_of(100) || self.pulls == 0 {
            return false;
        }
        let dim = self.a_inv.nrows();
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
    fn avg_reward(&self) -> f64 {
        if self.pulls > 0 {
            self.cumulative_reward / self.pulls as f64
        } else {
            0.0
        }
    }
}

/// Contextual bandit with 35-dim AST-enriched features.
///
/// Same LinUCB algorithm as [`super::linucb::LinUCBBandit`], but operating in a
/// 35-dimensional feature space (19 base + 16 AST-derived). This is a separate
/// type to preserve backward compatibility of the existing 19-dim bandit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AstEnrichedBandit {
    /// Per-arm models (8 arms, same as base LinUCB).
    arms: Vec<AstEnrichedArm>,
    /// Exploration parameter (alpha).
    alpha: f64,
    /// Total number of pulls across all arms.
    total_pulls: u64,
}

impl AstEnrichedBandit {
    /// Create a new AST-enriched bandit with the given exploration parameter.
    ///
    /// # Arguments
    ///
    /// * `alpha` - Exploration parameter. Higher values = more exploration.
    ///   Use `1.0` as a reasonable default.
    pub fn new(alpha: f64) -> Self {
        Self {
            arms: (0..NUM_ARMS).map(|_| AstEnrichedArm::new()).collect(),
            alpha,
            total_pulls: 0,
        }
    }

    /// Select the best arm for a file using AST-enriched features.
    ///
    /// Combines the base 19-dim features with 16 AST features extracted from
    /// `file_path`, then runs LinUCB arm selection on the 35-dim vector.
    ///
    /// If the file cannot be parsed, AST features default to zeros, making
    /// this equivalent to the base 19-dim selection.
    ///
    /// Returns the arm index (0..7).
    pub fn select_arm_for_file(&mut self, base_features: &[f64; 19], file_path: &str) -> usize {
        let enriched = enrich_features(base_features, file_path);
        // from_iter iterates over the array without creating an intermediate Vec.
        let features = Array1::from_iter(enriched.iter().copied());
        self.select_arm_raw(&features).0
    }

    /// Select arm from a raw 35-dim feature vector.
    ///
    /// Returns `(arm_index, ucb_score)`.
    pub fn select_arm_raw(&mut self, features: &Array1<f64>) -> (usize, f64) {
        assert_eq!(
            features.len(),
            FEATURE_DIM_AST,
            "Feature vector must have {} dimensions, got {}",
            FEATURE_DIM_AST,
            features.len()
        );

        // Forced exploration for cold arms
        let coldest = self
            .arms
            .iter()
            .enumerate()
            .filter(|(_, arm)| arm.pulls < COLD_ARM_THRESHOLD)
            .min_by_key(|(_, arm)| arm.pulls);

        if let Some((idx, arm)) = coldest {
            let score = arm.score(features, self.alpha);
            return (idx, score);
        }

        // Alpha decay over time
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

    /// Update the chosen arm with observed reward using a raw 35-dim feature vector.
    ///
    /// # Panics
    ///
    /// Panics if `arm >= NUM_ARMS` or if `features.len() != FEATURE_DIM_AST`.
    pub fn update(&mut self, arm: usize, features: &[f64; FEATURE_DIM_AST], reward: f64) {
        assert!(
            arm < NUM_ARMS,
            "arm index {} out of range [0, {})",
            arm,
            NUM_ARMS
        );

        let features_arr = ndarray::aview1(features).to_owned();

        #[allow(clippy::indexing_slicing)]
        {
            self.arms[arm].update(&features_arr, reward);
        }
        self.total_pulls += 1;

        #[allow(clippy::indexing_slicing)]
        {
            self.arms[arm].maybe_reorthogonalize();
        }
    }

    /// Convenience: update arm using base features + file path (auto-enriches).
    ///
    /// # Panics
    ///
    /// Panics if `arm >= NUM_ARMS`.
    pub fn update_from_file(
        &mut self,
        arm: usize,
        base_features: &[f64; 19],
        file_path: &str,
        reward: f64,
    ) {
        let enriched = enrich_features(base_features, file_path);
        self.update(arm, &enriched, reward);
    }

    /// Get statistics for all arms: `(ArmKind, pulls, avg_reward)`.
    pub fn arm_stats(&self) -> Vec<(ArmKind, u64, f64)> {
        self.arms
            .iter()
            .enumerate()
            .map(|(i, arm)| {
                let kind = ArmKind::from_index(i).unwrap_or(ArmKind::None);
                (kind, arm.pulls, arm.avg_reward())
            })
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
}

impl Default for AstEnrichedBandit {
    fn default() -> Self {
        Self::new(DEFAULT_ALPHA)
    }
}

// ── Serde helpers for ndarray types ──────────────────────────────────────

mod array2_serde {
    use ndarray::Array2;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    #[derive(Serialize, Deserialize)]
    struct Array2Proxy {
        rows: usize,
        cols: usize,
        data: Vec<f64>,
    }

    pub fn serialize<S>(arr: &Array2<f64>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let proxy = Array2Proxy {
            rows: arr.nrows(),
            cols: arr.ncols(),
            data: arr.iter().copied().collect(),
        };
        proxy.serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Array2<f64>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let proxy = Array2Proxy::deserialize(deserializer)?;
        Array2::from_shape_vec((proxy.rows, proxy.cols), proxy.data)
            .map_err(serde::de::Error::custom)
    }
}

mod array1_serde {
    use ndarray::Array1;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S>(arr: &Array1<f64>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let data: Vec<f64> = arr.iter().copied().collect();
        data.serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Array1<f64>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let data = Vec::<f64>::deserialize(deserializer)?;
        Ok(Array1::from_vec(data))
    }
}

// ── ContextualBandit trait impl ──────────────────────────────────────────

impl super::ContextualBandit for AstEnrichedBandit {
    fn select_arm(&mut self, features: &[f64]) -> (usize, f64) {
        // Accept any length: if 35-dim, use directly; if shorter, pad with zeros
        let mut padded = [0.0f64; FEATURE_DIM_AST];
        let copy_len = features.len().min(FEATURE_DIM_AST);
        // SAFETY: copy_len <= FEATURE_DIM_AST and copy_len <= features.len()
        padded
            .get_mut(..copy_len)
            .expect("copy_len <= FEATURE_DIM_AST")
            .copy_from_slice(
                features
                    .get(..copy_len)
                    .expect("copy_len <= features.len()"),
            );
        let mut arr = Array1::zeros(FEATURE_DIM_AST);
        azip!(&mut arr, &padded).for_each(|a, &p| *a = p);
        self.select_arm_raw(&arr)
    }

    fn update(&mut self, arm: usize, features: &[f64], reward: f64) {
        let mut padded = [0.0f64; FEATURE_DIM_AST];
        let copy_len = features.len().min(FEATURE_DIM_AST);
        padded
            .get_mut(..copy_len)
            .expect("copy_len <= FEATURE_DIM_AST")
            .copy_from_slice(
                features
                    .get(..copy_len)
                    .expect("copy_len <= features.len()"),
            );
        AstEnrichedBandit::update(self, arm, &padded, reward);
    }

    fn total_pulls(&self) -> u64 {
        AstEnrichedBandit::total_pulls(self)
    }

    fn num_arms(&self) -> usize {
        NUM_ARMS
    }

    fn export_snapshot(&self) -> super::BanditSnapshot {
        super::BanditSnapshot {
            bandit_type: "ast_enriched".to_string(),
            feature_dim: FEATURE_DIM_AST,
            num_arms: NUM_ARMS,
            total_pulls: AstEnrichedBandit::total_pulls(self),
            model_data: serde_json::to_string(self).unwrap_or_default(),
        }
    }

    fn import_snapshot(&mut self, snapshot: &super::BanditSnapshot) -> Result<(), String> {
        if snapshot.bandit_type != "ast_enriched" {
            return Err(format!(
                "Expected ast_enriched snapshot, got {}",
                snapshot.bandit_type
            ));
        }
        let restored: AstEnrichedBandit = serde_json::from_str(&snapshot.model_data)
            .map_err(|e| format!("Failed to deserialize AstEnrichedBandit: {e}"))?;
        *self = restored;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_bandit_has_correct_dimensions() {
        let bandit = AstEnrichedBandit::new(1.0);
        assert_eq!(bandit.arms.len(), NUM_ARMS);
        assert_eq!(bandit.total_pulls, 0);
    }

    #[test]
    fn select_and_update_cycle() {
        let mut bandit = AstEnrichedBandit::new(1.0);
        let base = [0.0f64; 19];

        // First selections should explore cold arms
        for _ in 0..NUM_ARMS {
            let arm = bandit.select_arm_for_file(&base, "/nonexistent.py");
            assert!(arm < NUM_ARMS);
            let enriched = enrich_features(&base, "/nonexistent.py");
            bandit.update(arm, &enriched, 0.5);
        }

        assert!(bandit.total_pulls() >= NUM_ARMS as u64);
    }

    #[test]
    fn arm_stats_returns_all_arms() {
        let bandit = AstEnrichedBandit::new(1.0);
        let stats = bandit.arm_stats();
        assert_eq!(stats.len(), NUM_ARMS);
    }

    #[test]
    fn default_creates_standard_bandit() {
        let bandit = AstEnrichedBandit::default();
        assert!((bandit.alpha() - DEFAULT_ALPHA).abs() < f64::EPSILON);
    }

    #[test]
    fn serde_roundtrip() {
        let mut bandit = AstEnrichedBandit::new(0.5);
        let base = [0.0f64; 19];
        let enriched = enrich_features(&base, "/nonexistent.py");
        bandit.update(0, &enriched, 0.7);

        let json = serde_json::to_string(&bandit).expect("serialize");
        let restored: AstEnrichedBandit = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(restored.total_pulls(), bandit.total_pulls());
        assert!((restored.alpha() - bandit.alpha()).abs() < f64::EPSILON);
    }

    #[test]
    #[should_panic(expected = "arm index")]
    fn update_panics_on_invalid_arm() {
        let mut bandit = AstEnrichedBandit::new(1.0);
        let features = [0.0f64; FEATURE_DIM_AST];
        bandit.update(NUM_ARMS, &features, 0.5);
    }
}
