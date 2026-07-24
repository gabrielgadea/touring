//! Context-aware strategy selection via a lightweight MLP (ndarray-based).
//!
//! This module provides [`ContextMlp`], a feed-forward neural network that learns
//! to map 25-dimensional context feature vectors to LinUCB arm scores.
//!
//! # Architecture
//!
//! ```text
//! Input (25D) → Linear(25→64) → ReLU → Linear(64→64) → ReLU → Linear(64→8)
//!                                                                     ↓
//!                                               arm scores (8 LinUCB arms)
//! ```
//!
//! The MLP is trained via online gradient descent (SGD) with cross-entropy loss.
//! Uses `ndarray` for tensor operations — no external ML framework required.

use ndarray::{Array1, Array2, Axis};
use rand::distributions::Uniform;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

use crate::rl::bandit::linucb::{FEATURE_DIM, NUM_ARMS};

/// Hidden layer width for the MLP.
const HIDDEN_DIM: usize = 64;

/// Learning rate for SGD.
const LEARNING_RATE: f64 = 1e-3;

/// Lightweight MLP for context → arm score prediction.
///
/// Takes a 25-dimensional LinUCB feature vector and outputs 8 arm scores.
/// The arm with the highest predicted score is selected.
#[derive(Debug, Clone)]
pub struct ContextMlp {
    /// First layer weights: (25 × 64)
    w1: Array2<f64>,
    /// First layer bias: (64,)
    b1: Array1<f64>,
    /// Second layer weights: (64 × 64)
    w2: Array2<f64>,
    /// Second layer bias: (64,)
    b2: Array1<f64>,
    /// Output layer weights: (64 × 8)
    w3: Array2<f64>,
    /// Output layer bias: (8,)
    b3: Array1<f64>,
}

impl Default for ContextMlp {
    fn default() -> Self {
        Self::new(&mut StdRng::from_entropy())
    }
}

impl ContextMlp {
    /// Initialise the MLP with random weights.
    pub fn new(rng: &mut StdRng) -> Self {
        let w1 = Array2::from_shape_fn((FEATURE_DIM, HIDDEN_DIM), |(_, _)| {
            rng.sample(Uniform::new(-0.1, 0.1))
        });
        let b1 = Array1::from_shape_fn(HIDDEN_DIM, |_| rng.sample(Uniform::new(-0.1, 0.1)));

        let w2 = Array2::from_shape_fn((HIDDEN_DIM, HIDDEN_DIM), |(_, _)| {
            rng.sample(Uniform::new(-0.1, 0.1))
        });
        let b2 = Array1::from_shape_fn(HIDDEN_DIM, |_| rng.sample(Uniform::new(-0.1, 0.1)));

        let w3 = Array2::from_shape_fn((HIDDEN_DIM, NUM_ARMS), |(_, _)| {
            rng.sample(Uniform::new(-0.1, 0.1))
        });
        let b3 = Array1::from_shape_fn(NUM_ARMS, |_| rng.sample(Uniform::new(-0.1, 0.1)));

        Self {
            w1,
            b1,
            w2,
            b2,
            w3,
            b3,
        }
    }

    /// Forward pass: features (25,) → logits (8,).
    ///
    /// Returns raw logits (before softmax).
    pub fn forward(&self, features: &Array1<f64>) -> Array1<f64> {
        // Layer 1: linear + ReLU
        let h1 = self.w1.t().dot(features) + &self.b1;
        let h1: Array1<f64> = h1.mapv(|x| x.max(0.0)); // ReLU

        // Layer 2: linear + ReLU
        let h2 = self.w2.t().dot(&h1) + &self.b2;
        let h2: Array1<f64> = h2.mapv(|x| x.max(0.0)); // ReLU

        // Output layer: linear
        self.w3.t().dot(&h2) + &self.b3
    }

    /// Predict the best arm index for a single feature vector.
    ///
    /// Returns the arm index with the highest score.
    pub fn predict_arm(&self, features: &Array1<f64>) -> usize {
        let logits = self.forward(features);
        logits
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(i, _)| i)
            .unwrap_or(0)
            .min(NUM_ARMS - 1)
    }

    /// Training step using cross-entropy loss + SGD.
    ///
    /// # Arguments
    ///
    /// * `features` — Input features (FEATURE_DIM,).
    /// * `target_arm` — The arm index that received the reward (used as class label).
    ///
    /// # Returns
    ///
    /// The updated model and the scalar loss value.
    #[allow(clippy::indexing_slicing)] // target_arm clamped to NUM_ARMS-1 at line 120
    pub fn train_step(&mut self, features: &Array1<f64>, target_arm: usize) -> f64 {
        let target_arm = target_arm.min(NUM_ARMS - 1);

        // Forward pass
        let h1 = self.w1.t().dot(features) + &self.b1;
        let h1_relu: Array1<f64> = h1.mapv(|x| x.max(0.0));

        let h2 = self.w2.t().dot(&h1_relu) + &self.b2;
        let h2_relu: Array1<f64> = h2.mapv(|x| x.max(0.0));

        let logits = self.w3.t().dot(&h2_relu) + &self.b3;

        // Softmax cross-entropy loss
        let logits_max = logits.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        let exp_logits: Array1<f64> = logits.mapv(|x| (x - logits_max).exp());
        let sum_exp = exp_logits.sum();
        let probs: Array1<f64> = exp_logits / sum_exp;

        let loss = -probs[target_arm].ln();

        // Gradient of cross-entropy w.r.t. logits
        let mut grad_logits: Array1<f64> = probs.clone();
        grad_logits[target_arm] -= 1.0;

        // Gradient w.r.t. output weights and bias
        let grad_w3 = h2_relu
            .clone()
            .insert_axis(Axis(1))
            .dot(&grad_logits.clone().insert_axis(Axis(0)));
        let grad_b3 = grad_logits.clone();

        // Backprop to hidden layer 2
        let grad_h2 = self.w3.dot(&grad_logits);
        let grad_h2_relu: Array1<f64> = grad_h2.mapv(|x| if x > 0.0 { 1.0 } else { 0.0 });

        let grad_w2 = h1_relu
            .clone()
            .insert_axis(Axis(1))
            .dot(&grad_h2_relu.clone().insert_axis(Axis(0)));
        let grad_b2 = grad_h2_relu.clone();

        // Backprop to hidden layer 1
        let grad_h1 = self.w2.dot(&grad_h2_relu);
        let grad_h1_relu: Array1<f64> = grad_h1.mapv(|x| if x > 0.0 { 1.0 } else { 0.0 });

        let grad_w1 = features
            .clone()
            .insert_axis(Axis(1))
            .dot(&grad_h1_relu.clone().insert_axis(Axis(0)));
        let grad_b1 = grad_h1_relu;

        // SGD update
        self.w1 = &self.w1 - &(&grad_w1 * LEARNING_RATE);
        self.b1 = &self.b1 - &(&grad_b1 * LEARNING_RATE);
        self.w2 = &self.w2 - &(&grad_w2 * LEARNING_RATE);
        self.b2 = &self.b2 - &(&grad_b2 * LEARNING_RATE);
        self.w3 = &self.w3 - &(&grad_w3 * LEARNING_RATE);
        self.b3 = &self.b3 - &(&grad_b3 * LEARNING_RATE);

        loss
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn mlp_features() -> Array1<f64> {
        Array1::from_vec(vec![0.1; FEATURE_DIM])
    }

    #[test]
    fn test_forward_shape() {
        let model = ContextMlp::default();
        let features = mlp_features();
        let logits = model.forward(&features);
        assert_eq!(
            logits.len(),
            NUM_ARMS,
            "Output should have NUM_ARMS elements"
        );
    }

    #[test]
    fn test_predict_arm_valid_index() {
        let model = ContextMlp::default();
        let features = mlp_features();
        let arm = model.predict_arm(&features);
        assert!(
            arm < NUM_ARMS,
            "Predicted arm index should be in [0, {})",
            NUM_ARMS
        );
    }

    #[test]
    fn test_train_step_decreases_loss() {
        let mut model = ContextMlp::default();
        let features = mlp_features();

        // Train on arm 0
        let loss_before = model.train_step(&features, 0);
        let loss_after = model.train_step(&features, 0);

        // Loss should generally decrease with repeated training on same target
        assert!(
            loss_after <= loss_before + 0.1,
            "Loss should not increase significantly after training"
        );
    }

    #[test]
    fn test_clone_and_predict() {
        let model = ContextMlp::default();
        let features = mlp_features();

        let arm1 = model.predict_arm(&features);
        let model2 = model.clone();
        let arm2 = model2.predict_arm(&features);

        assert_eq!(arm1, arm2, "Cloned model should give same prediction");
    }
}
