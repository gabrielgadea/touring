//! Context-aware strategy selection via a lightweight transformer (feature-gated).
//!
//! This module is only compiled when the `burn-transformer` feature is enabled.
//! It provides [`ContextTransformer`], a feed-forward neural network that learns
//! to map 19-dimensional context feature vectors to LinUCB arm scores.
//!
//! # Feature Flag
//!
//! ```toml
//! [features]
//! burn-transformer = ["dep:burn"]
//! ```
//!
//! # Architecture
//!
//! ```text
//! Input (19D) → Linear(19→64) → ReLU → Linear(64→64) → ReLU → Linear(64→8)
//!                                                                     ↓
//!                                               arm scores (8 LinUCB arms)
//! ```
//!
//! The transformer is trained via a simple online gradient descent step per
//! interaction. The `burn` crate provides auto-differentiation and device-agnostic
//! tensor operations (ndarray backend for CPU-only, wgpu for GPU when available).
//!
//! # Why `ndarray` backend?
//!
//! The `ndarray` burn backend requires no GPU drivers and works in any CI/CD
//! environment. For production GPU acceleration, swap to `burn::backend::Wgpu`.

#[cfg(feature = "burn-transformer")]
pub use inner::ContextTransformer;
// EC65: re-export train_step — pub API was trapped inside the private `inner` mod.
#[cfg(feature = "burn-transformer")]
pub use inner::train_step;

#[cfg(feature = "burn-transformer")]
mod inner {
    use burn::{
        module::Module,
        nn::{Linear, LinearConfig, Relu},
        optim::{GradientsParams, Optimizer, SgdConfig},
        prelude::*,
        tensor::{Tensor, backend::AutodiffBackend},
    };

    use crate::rl::bandit::linucb::{FEATURE_DIM, NUM_ARMS};

    /// Hidden layer width for the context transformer network.
    const HIDDEN_DIM: usize = 64;

    /// Learning rate for online gradient descent.
    const LEARNING_RATE: f64 = 1e-3;

    /// Lightweight feed-forward transformer for context → arm score prediction.
    ///
    /// Takes a 19-dimensional LinUCB feature vector and outputs 8 arm scores.
    /// The arm with the highest predicted score is selected for context injection.
    ///
    /// # Training
    ///
    /// Call [`train_step`] after each arm selection to provide a target arm index
    /// (the arm that yielded the highest reward). The network minimises cross-entropy
    /// loss between predicted scores and the one-hot target.
    ///
    /// # Inference
    ///
    /// Call [`forward`] to get raw logits, or [`predict_arm`] to get the argmax arm.
    #[derive(Module, Debug)]
    pub struct ContextTransformer<B: Backend> {
        /// First hidden layer: 19 → 64
        fc1: Linear<B>,
        /// Second hidden layer: 64 → 64
        fc2: Linear<B>,
        /// Output layer: 64 → 8 (one score per arm)
        fc3: Linear<B>,
        /// ReLU activation
        relu: Relu,
    }

    impl<B: Backend> ContextTransformer<B> {
        /// Initialise the transformer on the given device.
        pub fn new(device: &B::Device) -> Self {
            Self {
                fc1: LinearConfig::new(FEATURE_DIM, HIDDEN_DIM)
                    .with_bias(true)
                    .init(device),
                fc2: LinearConfig::new(HIDDEN_DIM, HIDDEN_DIM)
                    .with_bias(true)
                    .init(device),
                fc3: LinearConfig::new(HIDDEN_DIM, NUM_ARMS)
                    .with_bias(true)
                    .init(device),
                relu: Relu::new(),
            }
        }

        /// Forward pass: features (batch × FEATURE_DIM) → logits (batch × NUM_ARMS).
        pub fn forward(&self, features: Tensor<B, 2>) -> Tensor<B, 2> {
            let x = self.fc1.forward(features);
            let x = self.relu.forward(x);
            let x = self.fc2.forward(x);
            let x = self.relu.forward(x);
            self.fc3.forward(x)
        }

        /// Predict the best arm index for a single feature vector (1 × FEATURE_DIM).
        ///
        /// Returns the arm index with the highest predicted score.
        pub fn predict_arm(&self, features: Tensor<B, 2>) -> usize {
            let logits = self.forward(features);
            // argmax over the arms dimension
            let arm_idx = logits.argmax(1).into_scalar().elem::<i32>() as usize;
            arm_idx.min(NUM_ARMS - 1)
        }
    }

    /// Training step for the [`ContextTransformer`] using cross-entropy loss.
    ///
    /// # Arguments
    ///
    /// * `model` — The transformer model with an autodiff backend.
    /// * `features` — Input features (1 × FEATURE_DIM).
    /// * `target_arm` — The arm index that received the reward (used as class label).
    /// * `device` — The backend device.
    ///
    /// # Returns
    ///
    /// The updated model and the scalar loss value.
    pub fn train_step<B: AutodiffBackend>(
        model: ContextTransformer<B>,
        features: Tensor<B, 2>,
        target_arm: usize,
        device: &B::Device,
    ) -> (ContextTransformer<B>, f32) {
        use burn::tensor::Int;

        // Forward pass (with gradient tracking)
        let logits = model.forward(features);

        // Build one-hot target as integer class label
        let target_arm_clamped = target_arm.min(NUM_ARMS - 1) as i32;
        let target: Tensor<B, 1, Int> = Tensor::from_ints([target_arm_clamped], device);

        // Cross-entropy loss
        let loss = burn::nn::loss::CrossEntropyLossConfig::new()
            .init(device)
            .forward(logits.clone(), target);

        let loss_scalar = loss.clone().into_scalar().elem::<f32>();

        // Backward + SGD parameter update (burn 0.16: GradientsParams + Optimizer::step)
        let gradients = loss.backward();
        let grads = GradientsParams::from_grads(gradients, &model);
        let mut optim = SgdConfig::new().init::<B, ContextTransformer<B>>();
        let model = optim.step(LEARNING_RATE, model, grads);

        (model, loss_scalar)
    }
}

// ── Stub for non-feature builds (keeps module compilable) ────────────────────

#[cfg(not(feature = "burn-transformer"))]
/// Placeholder — compile this module with `--features burn-transformer` to enable.
pub struct ContextTransformerStub;

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(all(test, feature = "burn-transformer"))]
mod tests {
    use super::inner::ContextTransformer;
    use crate::rl::bandit::linucb::FEATURE_DIM;
    use burn::backend::NdArray;
    use burn::tensor::Tensor;

    type TestBackend = NdArray;
    type TestDevice = <TestBackend as burn::tensor::backend::Backend>::Device;

    fn test_device() -> TestDevice {
        Default::default()
    }

    #[test]
    fn test_forward_shape() {
        let device = test_device();
        let model: ContextTransformer<TestBackend> = ContextTransformer::new(&device);

        let features: Tensor<TestBackend, 2> = Tensor::zeros([1, FEATURE_DIM], &device);
        let logits = model.forward(features);

        let shape = logits.dims();
        assert_eq!(shape, [1, 8], "Output should be (batch=1, arms=8)");
    }

    #[test]
    fn test_predict_arm_valid_index() {
        let device = test_device();
        let model: ContextTransformer<TestBackend> = ContextTransformer::new(&device);

        let features: Tensor<TestBackend, 2> = Tensor::zeros([1, FEATURE_DIM], &device);
        let arm = model.predict_arm(features);
        assert!(
            arm < 8,
            "Predicted arm index should be in [0, 7], got {}",
            arm
        );
    }
}
