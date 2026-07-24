//! FTRL (Follow The Regularized Leader) online learning layer.
//!
//! Learns which of the 19 context features matter most for reward prediction.
//! Complements the LinUCB bandit with feature-level importance weights that
//! adapt incrementally per tool execution.
//!
//! # Algorithm
//!
//! FTRL-Proximal (McMahan et al., 2013) with L1+L2 regularization:
//! - L1 (`l1_ratio`) promotes **sparsity** — unimportant features → weight 0
//! - L2 (`l2_ratio`) provides **stability** — prevents large weight oscillation
//! - `alpha` and `beta` control the per-coordinate adaptive learning rate
//!
//! # Usage
//!
//! ```rust,no_run
//! # #[cfg(feature = "ftrl")]
//! # {
//! use touring_intelligence::rl::online_learning::ftrl::{FtrlLayer, FtrlConfig};
//!
//! let mut ftrl = FtrlLayer::new(25); // 25-dim LinUCB feature vector
//! let features = vec![1.0_f64; 25];
//! let predicted = ftrl.update(&features, 0.8).unwrap();
//! # }
//! ```

// NOTE: linfa ecosystem (0.7) uses ndarray 0.15; workspace uses ndarray 0.16.
// We alias the linfa-compatible ndarray as `nd` to avoid version conflicts.
#[cfg(feature = "ftrl")]
use linfa::prelude::*;
#[cfg(feature = "ftrl")]
use linfa_ftrl::Ftrl;
#[cfg(feature = "ftrl")]
use ndarray015::{Array1, Array2};

use crate::rl::error::{LearningError, LearningResult};

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Hyperparameters for the FTRL-Proximal online learner.
///
/// Defaults are tuned for the 19-dimensional LinUCB feature space with
/// reward signals in [-1, 1].
#[derive(Debug, Clone)]
pub struct FtrlConfig {
    /// Per-coordinate learning rate numerator.
    /// Smaller = slower adaptation, larger = faster but noisier.
    /// Default: 0.005
    pub alpha: f64,
    /// Learning rate smoothing term (added to denominator).
    /// Prevents divide-by-zero on early steps.
    /// Default: 1.0
    pub beta: f64,
    /// L1 regularization coefficient.
    /// Promotes weight sparsity — unimportant features converge to 0.
    /// Default: 0.005
    pub l1_ratio: f64,
    /// L2 regularization coefficient.
    /// Stabilizes weight updates and prevents oscillation.
    /// Default: 1.0
    pub l2_ratio: f64,
}

impl Default for FtrlConfig {
    fn default() -> Self {
        Self {
            alpha: 0.005,
            beta: 1.0,
            l1_ratio: 0.005,
            l2_ratio: 1.0,
        }
    }
}

// ---------------------------------------------------------------------------
// FtrlLayer
// ---------------------------------------------------------------------------

/// FTRL model wrapper for incremental feature importance learning.
///
/// Wraps `linfa_ftrl::Ftrl` with an ergonomic incremental API that accepts
/// raw feature slices and scalar rewards, internally constructing the
/// single-row `linfa::Dataset` required by `fit_with`.
///
/// # Thread safety
///
/// `FtrlLayer` is **not** `Sync` — the inner model is mutated on every
/// `update()` call. Use `Mutex<FtrlLayer>` for shared access.
pub struct FtrlLayer {
    #[cfg(feature = "ftrl")]
    model: Option<Ftrl<f64>>,
    config: FtrlConfig,
    feature_dim: usize,
    update_count: u64,
}

impl std::fmt::Debug for FtrlLayer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FtrlLayer")
            .field("feature_dim", &self.feature_dim)
            .field("update_count", &self.update_count)
            .field("alpha", &self.config.alpha)
            .field("l1_ratio", &self.config.l1_ratio)
            .finish()
    }
}

impl FtrlLayer {
    /// Create a new FTRL layer with default hyperparameters.
    ///
    /// `feature_dim` must match the feature vectors passed to [`update`] and
    /// [`predict`]. For the LinUCB context, this is `FEATURE_DIM` = 19.
    ///
    /// [`update`]: Self::update
    /// [`predict`]: Self::predict
    pub fn new(feature_dim: usize) -> Self {
        Self::new_with_config(feature_dim, FtrlConfig::default())
    }

    /// Create a new FTRL layer with explicit hyperparameters.
    pub fn new_with_config(feature_dim: usize, config: FtrlConfig) -> Self {
        Self {
            #[cfg(feature = "ftrl")]
            model: None,
            config,
            feature_dim,
            update_count: 0,
        }
    }

    /// Incremental update with a (features, reward) pair.
    ///
    /// Performs one FTRL-Proximal step using `fit_with(Some(model), &dataset)`.
    /// The reward is binarized (reward > 0.0 → `true`) to satisfy the FTRL
    /// binary classification interface, while still encoding the reward signal
    /// direction.
    ///
    /// # Returns
    ///
    /// Predicted reward **before** the update (prior prediction), useful for
    /// measuring prediction error over time.
    ///
    /// # Errors
    ///
    /// Returns [`LearningError::DimensionMismatch`] if `features.len()` ≠
    /// `feature_dim`. Returns [`LearningError::Config`] if the FTRL internal
    /// fit fails.
    pub fn update(&mut self, features: &[f64], reward: f64) -> LearningResult<f64> {
        if features.len() != self.feature_dim {
            return Err(LearningError::DimensionMismatch {
                expected: self.feature_dim,
                actual: features.len(),
            });
        }

        #[cfg(feature = "ftrl")]
        {
            let prior_pred = self.predict_internal(features);

            let x = Array2::from_shape_vec((1, self.feature_dim), features.to_vec())
                .map_err(|e| LearningError::Config(e.to_string()))?;

            // Binarize reward: positive → true, non-positive → false
            let y: Array1<bool> = Array1::from_vec(vec![reward > 0.0]);

            let dataset = linfa::Dataset::new(x, y);

            let params = Ftrl::params()
                .alpha(self.config.alpha)
                .beta(self.config.beta)
                .l1_ratio(self.config.l1_ratio)
                .l2_ratio(self.config.l2_ratio);

            // fit_with performs one incremental FTRL step
            let updated_model = params
                .fit_with(self.model.take(), &dataset)
                .map_err(|e| LearningError::Config(format!("ftrl fit_with failed: {e}")))?;

            self.model = Some(updated_model);
            self.update_count += 1;

            Ok(prior_pred)
        }

        #[cfg(not(feature = "ftrl"))]
        {
            // No-op: return neutral prediction when feature is not compiled
            let _ = reward;
            self.update_count += 1;
            Ok(0.0)
        }
    }

    /// Predict reward direction for features without updating the model.
    ///
    /// Returns a value in [0.0, 1.0] representing P(reward > 0) per the
    /// FTRL sigmoid output. Returns 0.5 (neutral) if no updates have been
    /// applied yet.
    ///
    /// # Errors
    ///
    /// Returns [`LearningError::DimensionMismatch`] if `features.len()` ≠
    /// `feature_dim`.
    pub fn predict(&self, features: &[f64]) -> LearningResult<f64> {
        if features.len() != self.feature_dim {
            return Err(LearningError::DimensionMismatch {
                expected: self.feature_dim,
                actual: features.len(),
            });
        }

        #[cfg(feature = "ftrl")]
        {
            Ok(self.predict_internal(features))
        }

        #[cfg(not(feature = "ftrl"))]
        {
            Ok(0.5)
        }
    }

    /// Number of incremental updates applied since creation.
    #[inline]
    pub fn update_count(&self) -> u64 {
        self.update_count
    }

    /// Whether the model has received at least one update (warm model).
    #[inline]
    pub fn is_warm(&self) -> bool {
        self.update_count > 0
    }

    /// Current feature dimension.
    #[inline]
    pub fn feature_dim(&self) -> usize {
        self.feature_dim
    }

    // Internal predict — returns probability in [0.0, 1.0] without dimension check.
    // linfa-ftrl's predict() returns Array1<Pr> where Pr(f32) is a probability newtype.
    #[cfg(feature = "ftrl")]
    #[allow(clippy::indexing_slicing)] // predictions[0] is valid: Array1 created from single-row dataset
    fn predict_internal(&self, features: &[f64]) -> f64 {
        let Some(ref model) = self.model else {
            return 0.5; // Prior: neutral before first update
        };

        let Ok(x) = Array2::from_shape_vec((1, self.feature_dim), features.to_vec()) else {
            return 0.5;
        };

        // Dummy target required by DatasetBase constructor; values are irrelevant for predict
        let y: Array1<bool> = Array1::from_elem(1, false);
        let dataset = linfa::Dataset::new(x, y);

        // predict() returns Array1<Pr>; Pr implements Deref<Target = f32>
        let predictions = model.predict(&dataset);
        f64::from(*predictions[0])
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(all(test, feature = "ftrl"))]
mod tests {
    #![allow(clippy::indexing_slicing)] // test vecs asserted non-empty before indexing
    use super::*;

    fn features(val: f64, dim: usize) -> Vec<f64> {
        vec![val; dim]
    }

    #[test]
    fn new_layer_is_cold() {
        let layer = FtrlLayer::new(19);
        assert_eq!(layer.update_count(), 0);
        assert!(!layer.is_warm());
        assert_eq!(layer.feature_dim(), 19);
    }

    #[test]
    fn dimension_mismatch_returns_error() {
        let mut layer = FtrlLayer::new(19);
        let result = layer.update(&features(1.0, 10), 0.5);
        assert!(matches!(
            result,
            Err(LearningError::DimensionMismatch {
                expected: 19,
                actual: 10
            })
        ));
    }

    #[test]
    fn update_increments_count() {
        let mut layer = FtrlLayer::new(19);
        layer.update(&features(1.0, 19), 0.8).expect("update ok");
        assert_eq!(layer.update_count(), 1);
        assert!(layer.is_warm());
    }

    #[test]
    fn predict_before_update_returns_neutral() {
        let layer = FtrlLayer::new(19);
        let pred = layer.predict(&features(0.5, 19)).expect("predict ok");
        assert!((pred - 0.5).abs() < 1e-9, "expected 0.5, got {pred}");
    }

    #[test]
    fn multiple_updates_converge_direction() {
        let mut layer = FtrlLayer::new(19);
        // Repeatedly signal that these features → positive reward
        for _ in 0..50 {
            layer.update(&features(1.0, 19), 0.9).expect("update ok");
        }
        let pred = layer.predict(&features(1.0, 19)).expect("predict ok");
        // After 50 positive updates, model should lean toward positive (> 0.5)
        assert!(pred > 0.5, "expected prediction > 0.5, got {pred}");
    }

    #[test]
    fn custom_config_accepted() {
        let config = FtrlConfig {
            alpha: 0.01,
            beta: 2.0,
            l1_ratio: 0.001,
            l2_ratio: 0.5,
        };
        let mut layer = FtrlLayer::new_with_config(5, config);
        layer.update(&features(0.1, 5), -0.5).expect("update ok");
        assert_eq!(layer.update_count(), 1);
    }
}

#[cfg(all(test, not(feature = "ftrl")))]
mod noop_tests {
    use super::*;

    #[test]
    fn noop_update_returns_neutral() {
        let mut layer = FtrlLayer::new(19);
        let pred = layer.update(&vec![1.0; 19], 0.8).expect("noop ok");
        assert_eq!(pred, 0.0);
    }

    #[test]
    fn noop_predict_returns_half() {
        let layer = FtrlLayer::new(19);
        let pred = layer.predict(&vec![0.5; 19]).expect("noop ok");
        assert_eq!(pred, 0.5);
    }
}
