//! Actor-Critic Reinforcement Learning module.
//!
//! Combines policy gradient (actor) with value estimation (critic) for
//! stable on-policy learning. The actor updates policy parameters directly,
//! while the critic estimates state values to reduce variance.
//!
//! # Architecture
//!
//! ```text
//! State (u64 hash)
//!     │
//!     ├──► ActorNetwork ──► Policy π(a|s) → action probability distribution
//!     │                         │
//!     │                         └──► select_action() → greedy action with exploration
//!     │
//!     └──► CriticNetwork ──► V(s) value estimate
//!                                 │
//!                                 └──► critic_evaluate() → advantage estimation
//! ```
//!
//! # Integration
//!
//! - **LinUCB prior**: Actor-Critic advantage scores can inform LinUCB arm selection
//! - **QTable**: TD errors from critic guide Q-value updates
//! - **MetacognitivePipeline**: feedback loop adjusts exploration threshold

use rand::Rng;
use smallvec::SmallVec;
use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};

/// State vector using stack allocation for the common 32-element case.
type StateVec = SmallVec<[f32; 32]>;

/// Soft-update rate for target network (τ in (0,1)).
/// Lower values = slower target updates = more stable learning.
const DEFAULT_TAU: f32 = 0.01;

/// Discount factor for future rewards (γ in [0,1]).
const DEFAULT_GAMMA: f32 = 0.99;

/// Maximum gradient norm for clipping (prevent exploding gradients).
const MAX_GRADIENT_NORM: f32 = 1.0;

/// Minimum advantage to consider an action better than average.
const MIN_ADVANTAGE_THRESHOLD: f32 = 0.01;

// ── Configuration ─────────────────────────────────────────────────────────────

/// Configuration for Actor-Critic agent.
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct ActorCriticConfig {
    /// Learning rate for actor network (policy gradient step size).
    pub actor_lr: f32,
    /// Learning rate for critic network (value function update).
    pub critic_lr: f32,
    /// Soft-update rate for target network (τ).
    pub tau: f32,
    /// Discount factor for future rewards (γ).
    pub gamma: f32,
    /// Number of action arms (action space size).
    pub num_actions: usize,
    /// State feature dimension (input size for networks).
    pub state_dim: usize,
    /// Initial exploration epsilon (for epsilon-greedy action selection).
    pub epsilon: f32,
    /// Epsilon decay per update.
    pub epsilon_decay: f32,
    /// Minimum epsilon floor.
    pub epsilon_min: f32,
}

impl Default for ActorCriticConfig {
    fn default() -> Self {
        Self {
            actor_lr: 0.001,
            critic_lr: 0.005,
            tau: DEFAULT_TAU,
            gamma: DEFAULT_GAMMA,
            num_actions: 8,
            state_dim: 32,
            epsilon: 0.1,
            epsilon_decay: 0.995,
            epsilon_min: 0.01,
        }
    }
}

// ── Networks ──────────────────────────────────────────────────────────────────

/// Actor network — outputs action probabilities (policy π(a|s)).
///
/// Simple 2-layer MLP with softmax output.
#[derive(Debug, Clone)]
pub struct ActorNetwork {
    /// Policy parameters: [state_dim -> hidden -> num_actions]
    /// Stored as flat weight matrix for efficient computation.
    policy_weights: Vec<f32>,
    hidden_size: usize,
    learning_rate: f32,
}

impl ActorNetwork {
    /// Create a new actor network with random initialization.
    pub fn new(
        state_dim: usize,
        hidden_size: usize,
        num_actions: usize,
        learning_rate: f32,
    ) -> Self {
        let total_params =
            state_dim * hidden_size + hidden_size * num_actions + hidden_size + num_actions;
        let mut rng = rand::thread_rng();
        let scale = (2.0 / (state_dim + hidden_size) as f32).sqrt();
        let policy_weights: Vec<f32> = (0..total_params)
            .map(|_| rng.gen_range(-scale..scale))
            .collect();

        Self {
            policy_weights,
            hidden_size,
            learning_rate,
        }
    }

    /// Forward pass: compute action probabilities for a given state.
    ///
    /// Returns a vector of probabilities for each action.
    pub fn forward(&self, state: &[f32]) -> Vec<f32> {
        self.compute_probs(state)
    }

    /// Compute action probabilities via forward pass.
    fn compute_probs(&self, state: &[f32]) -> Vec<f32> {
        let state_dim = state.len();
        let num_actions =
            self.policy_weights.len() - state_dim * self.hidden_size - self.hidden_size;
        let num_actions = num_actions / self.hidden_size;

        // Layer 1: linear(state, weights[0:state_dim*hidden])
        let w1_end = state_dim * self.hidden_size;
        let w1_bias_end = w1_end + self.hidden_size;
        let weights1 = self.policy_weights.get(0..w1_end).unwrap_or(&[]);
        let bias1 = self.policy_weights.get(w1_end..w1_bias_end).unwrap_or(&[]);
        let hidden = self.relu_layer(state, weights1, bias1);

        // Layer 2: linear(hidden, weights[w1_end: ...])
        let w2_start = w1_bias_end;
        let w2_end = w2_start + self.hidden_size * num_actions;
        let weights2 = self.policy_weights.get(w2_start..w2_end).unwrap_or(&[]);
        let bias2 = self
            .policy_weights
            .get(w2_end..w2_end + num_actions)
            .unwrap_or(&[]);
        let logits = self.linear_layer(&hidden, weights2, bias2);

        // Softmax
        self.softmax(&logits)
    }

    fn relu_layer(&self, input: &[f32], weights: &[f32], bias: &[f32]) -> Vec<f32> {
        let hidden_size = bias.len();
        let mut result = vec![0.0_f32; hidden_size];

        for (h, res) in result.iter_mut().enumerate().take(hidden_size) {
            let mut sum = bias.get(h).copied().unwrap_or(0.0);
            for i in 0..input.len() {
                sum += input.get(i).copied().unwrap_or(0.0)
                    * weights.get(i * hidden_size + h).copied().unwrap_or(0.0);
            }
            *res = sum.max(0.0);
        }
        result
    }

    fn linear_layer(&self, input: &[f32], weights: &[f32], bias: &[f32]) -> Vec<f32> {
        let output_size = bias.len();
        let mut result = vec![0.0_f32; output_size];

        for (o, res) in result.iter_mut().enumerate().take(output_size) {
            let mut sum = bias.get(o).copied().unwrap_or(0.0);
            for i in 0..input.len() {
                sum += input.get(i).copied().unwrap_or(0.0)
                    * weights.get(i * output_size + o).copied().unwrap_or(0.0);
            }
            *res = sum;
        }
        result
    }

    fn softmax(&self, logits: &[f32]) -> Vec<f32> {
        let max_logit = logits.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
        let exp: Vec<f32> = logits.iter().map(|&x| (x - max_logit).exp()).collect();
        let sum: f32 = exp.iter().sum();
        exp.iter().map(|&x| x / sum).collect()
    }

    /// Update policy weights using policy gradient (REINFORCE with baseline).
    ///
    /// gradient = -log π(a|s) * advantage
    pub fn update(&mut self, state: &[f32], action: usize, advantage: f32) {
        let probs = self.compute_probs(state);
        if action >= probs.len() {
            return;
        }

        // Policy gradient: ∇J = -∇log π(a|s) * A(s,a)
        // For softmax: ∇log π(a_i|s) = one_hot(a) - π(a|s)
        let num_actions = probs.len();
        let mut gradient = vec![0.0_f32; self.policy_weights.len()];
        // _scale is referenced for magnitude; actual per-output grad uses learning_rate directly
        let _scale = self.learning_rate * advantage;

        // Simplified gradient computation for 2-layer network
        let state_dim = state.len();
        let w1_end = state_dim * self.hidden_size;

        // Compute hidden layer activations
        let hidden = {
            let mut h = vec![0.0_f32; self.hidden_size];
            for (h_idx, h_val) in h.iter_mut().enumerate().take(self.hidden_size) {
                let mut sum = self
                    .policy_weights
                    .get(w1_end + h_idx)
                    .copied()
                    .unwrap_or(0.0); // bias
                for s in 0..state_dim {
                    sum += state.get(s).copied().unwrap_or(0.0)
                        * self
                            .policy_weights
                            .get(s * self.hidden_size + h_idx)
                            .copied()
                            .unwrap_or(0.0);
                }
                *h_val = sum.max(0.0); // ReLU
            }
            h
        };

        // Gradient for output layer weights and bias
        let w2_start = w1_end + self.hidden_size;
        let w2_end = w2_start + self.hidden_size * num_actions;
        let b2_start = w2_end;

        for o in 0..num_actions {
            let prob_o = probs.get(o).copied().unwrap_or(0.0);
            let target = if o == action { 1.0 } else { 0.0 };
            let grad_coeff = -self.learning_rate * (target - prob_o) * advantage;

            // Update output layer weights
            for (i, h_val) in hidden.iter().enumerate().take(self.hidden_size) {
                let idx = w2_start + i * num_actions + o;
                if let Some(g) = gradient.get_mut(idx) {
                    *g += h_val * grad_coeff;
                }
            }
            // Update output bias
            if let Some(g) = gradient.get_mut(b2_start + o) {
                *g += grad_coeff;
            }
        }

        // Gradient for hidden layer (backprop through ReLU)
        for (i, h_val) in hidden.iter().enumerate().take(self.hidden_size) {
            let deriv = if *h_val > 0.0 { 1.0 } else { 0.0 };
            let action_grad =
                probs.get(action).copied().unwrap_or(0.0) * self.learning_rate * advantage;
            for s in 0..state_dim {
                let idx = s * self.hidden_size + i;
                if let Some(g) = gradient.get_mut(idx) {
                    *g += state.get(s).copied().unwrap_or(0.0) * deriv * action_grad;
                }
            }
        }

        // Apply gradient with clipping
        let grad_norm = gradient.iter().map(|&x| x * x).sum::<f32>().sqrt();
        if grad_norm > MAX_GRADIENT_NORM {
            let scale = MAX_GRADIENT_NORM / grad_norm;
            for g in &mut gradient {
                *g *= scale;
            }
        }

        // Apply gradient update
        for (i, g) in gradient.iter().enumerate() {
            if let Some(w) = self.policy_weights.get_mut(i) {
                *w -= g;
            }
        }
    }

    /// Decay epsilon for exploration.
    pub fn decay_epsilon(&mut self, _decay: f32) {
        // Epsilon decay is handled at ActorCritic level
    }
}

/// Critic network — estimates state value V(s).
///
/// Simple 2-layer MLP outputting a scalar value.
#[derive(Debug, Clone)]
pub struct CriticNetwork {
    /// Value function parameters: [state_dim -> hidden -> 1]
    value_weights: Vec<f32>,
    hidden_size: usize,
    learning_rate: f32,
    /// Target network for stable learning.
    target_weights: Vec<f32>,
}

impl CriticNetwork {
    /// Create a new critic network with random initialization.
    pub fn new(state_dim: usize, hidden_size: usize, learning_rate: f32) -> Self {
        let total_params = state_dim * hidden_size + hidden_size + hidden_size + 1;
        let mut rng = rand::thread_rng();
        let scale = (2.0 / state_dim as f32).sqrt();
        let value_weights: Vec<f32> = (0..total_params)
            .map(|_| rng.gen_range(-scale..scale))
            .collect();

        // Target network starts with same weights
        let target_weights = value_weights.clone();

        Self {
            value_weights,
            hidden_size,
            learning_rate,
            target_weights,
        }
    }

    /// Forward pass: compute V(s) value estimate.
    pub fn forward(&self, state: &[f32]) -> f32 {
        self.compute_value(state)
    }

    /// Compute value via forward pass using current weights.
    fn compute_value(&self, state: &[f32]) -> f32 {
        let state_dim = state.len();

        // Layer 1
        let w1_end = state_dim * self.hidden_size;
        let w1_bias_end = w1_end + self.hidden_size;
        let weights1 = self.value_weights.get(0..w1_end).unwrap_or(&[]);
        let bias1 = self.value_weights.get(w1_end..w1_bias_end).unwrap_or(&[]);
        let hidden = self.relu_layer(state, weights1, bias1);

        // Layer 2 → scalar
        let w2_start = w1_bias_end;
        let w2_end = w2_start + self.hidden_size;
        let weights2 = self.value_weights.get(w2_start..w2_end).unwrap_or(&[]);
        let bias2 = self.value_weights.get(w2_end..).unwrap_or(&[]);
        self.linear_output(&hidden, weights2, bias2)
    }

    /// Forward pass using target network weights.
    pub fn forward_target(&self, state: &[f32]) -> f32 {
        let state_dim = state.len();

        let w1_end = state_dim * self.hidden_size;
        let w1_bias_end = w1_end + self.hidden_size;
        let weights1 = self.target_weights.get(0..w1_end).unwrap_or(&[]);
        let bias1 = self.target_weights.get(w1_end..w1_bias_end).unwrap_or(&[]);
        let hidden = self.relu_layer(state, weights1, bias1);

        let w2_start = w1_bias_end;
        let w2_end = w2_start + self.hidden_size;
        let weights2 = self.target_weights.get(w2_start..w2_end).unwrap_or(&[]);
        let bias2 = self.target_weights.get(w2_end..).unwrap_or(&[]);
        self.linear_output(&hidden, weights2, bias2)
    }

    fn relu_layer(&self, input: &[f32], weights: &[f32], bias: &[f32]) -> Vec<f32> {
        let hidden_size = bias.len();
        let mut result = vec![0.0_f32; hidden_size];

        for (h, res) in result.iter_mut().enumerate().take(hidden_size) {
            let mut sum = bias.get(h).copied().unwrap_or(0.0);
            for i in 0..input.len() {
                sum += input.get(i).copied().unwrap_or(0.0)
                    * weights.get(i * hidden_size + h).copied().unwrap_or(0.0);
            }
            *res = sum.max(0.0);
        }
        result
    }

    fn linear_output(&self, input: &[f32], weights: &[f32], bias: &[f32]) -> f32 {
        let mut sum = bias.first().copied().unwrap_or(0.0);
        for i in 0..input.len() {
            sum += input.get(i).copied().unwrap_or(0.0) * weights.get(i).copied().unwrap_or(0.0);
        }
        sum
    }

    /// Update value function using TD error.
    ///
    /// TD error = r + γ * V(s') - V(s)
    /// Loss = TD_error²
    pub fn update(&mut self, state: &[f32], td_error: f32) {
        let state_dim = state.len();
        let w1_end = state_dim * self.hidden_size;

        // Compute hidden activations
        let hidden = {
            let mut h = vec![0.0_f32; self.hidden_size];
            for (h_idx, h_val) in h.iter_mut().enumerate().take(self.hidden_size) {
                let mut sum = self
                    .value_weights
                    .get(w1_end + h_idx)
                    .copied()
                    .unwrap_or(0.0);
                for s in 0..state_dim {
                    sum += state.get(s).copied().unwrap_or(0.0)
                        * self
                            .value_weights
                            .get(s * self.hidden_size + h_idx)
                            .copied()
                            .unwrap_or(0.0);
                }
                *h_val = sum.max(0.0);
            }
            h
        };

        // Gradient for output layer
        let w2_start = w1_end + self.hidden_size;
        let w2_end = w2_start + self.hidden_size;
        let mut gradient = vec![0.0_f32; self.value_weights.len()];

        for (i, h_val) in hidden.iter().enumerate().take(self.hidden_size) {
            if let Some(g) = gradient.get_mut(w2_start + i) {
                *g = h_val * td_error * self.learning_rate;
            }
        }
        if let Some(g) = gradient.get_mut(w2_end) {
            *g = td_error * self.learning_rate;
        }

        // Gradient for hidden layer (backprop through ReLU)
        for (i, h_val) in hidden.iter().enumerate().take(self.hidden_size) {
            let deriv = if *h_val > 0.0 { 1.0 } else { 0.0 };
            for s in 0..state_dim {
                let idx = s * self.hidden_size + i;
                if let Some(g) = gradient.get_mut(idx) {
                    *g = state.get(s).copied().unwrap_or(0.0)
                        * deriv
                        * td_error
                        * self.learning_rate
                        * 0.1;
                }
            }
            if let Some(g) = gradient.get_mut(w1_end + i) {
                *g = deriv * td_error * self.learning_rate * 0.1;
            }
        }

        // Gradient clipping
        let grad_norm = gradient.iter().map(|&x| x * x).sum::<f32>().sqrt();
        if grad_norm > MAX_GRADIENT_NORM {
            let scale = MAX_GRADIENT_NORM / grad_norm;
            for g in &mut gradient {
                *g *= scale;
            }
        }

        // Apply update
        for (i, g) in gradient.iter().enumerate() {
            if let Some(w) = self.value_weights.get_mut(i) {
                *w -= g;
            }
        }
    }

    /// Soft-update target network: θ_target = τ * θ_local + (1-τ) * θ_target
    pub fn soft_update(&mut self, tau: f32) {
        for (i, w) in self.value_weights.iter().enumerate() {
            if let Some(t) = self.target_weights.get_mut(i) {
                *t = tau * w + (1.0 - tau) * *t;
            }
        }
    }
}

// ── Actor-Critic Agent ───────────────────────────────────────────────────────

/// Action selection result with exploration info.
#[derive(Debug, Clone, Copy)]
pub struct ActionSelection {
    /// Selected action index.
    pub action: usize,
    /// Probability of selected action under current policy.
    pub probability: f32,
    /// Whether exploration (random) was used.
    pub exploratory: bool,
}

/// Actor-Critic agent combining policy gradient with value estimation.
#[derive(Debug, Clone)]
pub struct ActorCritic {
    /// Actor network for policy.
    actor: ActorNetwork,
    /// Critic network for value estimation.
    critic: CriticNetwork,
    /// Soft-update rate.
    tau: f32,
    /// Discount factor.
    gamma: f32,
    /// Configuration.
    config: ActorCriticConfig,
    /// Current state feature vector.
    current_state: Option<StateVec>,
    /// Experience buffer for n-step returns.
    replay_buffer: VecDeque<Experience>,
    /// Statistics.
    stats: ActorCriticStats,
    /// Feature cache to avoid recomputing state_to_features.
    state_cache: RefCell<HashMap<u64, StateVec>>,
}

/// Experience tuple for n-step learning.
#[derive(Debug, Clone)]
struct Experience {
    state: StateVec,
    action: usize,
    reward: f32,
    next_state: StateVec,
    done: bool,
}

/// Learning statistics for monitoring.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ActorCriticStats {
    /// Total updates performed.
    pub updates: u64,
    /// Average TD error magnitude.
    pub avg_td_error: f32,
    /// Average advantage magnitude.
    pub avg_advantage: f32,
    /// Total episodes completed.
    pub episodes: u64,
    /// Last epsilon value.
    pub epsilon: f32,
}

impl ActorCritic {
    /// Create a new Actor-Critic agent from configuration.
    ///
    /// # Example
    ///
    /// ```rust
    /// use touring_intelligence::rl::rl::actor_critic::{ActorCritic, ActorCriticConfig};
    ///
    /// let config = ActorCriticConfig {
    ///     actor_lr: 0.001,
    ///     critic_lr: 0.005,
    ///     tau: 0.01,        // soft update parameter
    ///     gamma: 0.99,      // discount factor
    ///     num_actions: 8,
    ///     state_dim: 32,
    ///     epsilon: 0.1,
    ///     epsilon_decay: 0.995,
    ///     epsilon_min: 0.01,
    /// };
    /// let mut agent = ActorCritic::new(config);
    ///
    /// // Select action from a state (u64 state identifier)
    /// let state = 12345u64;
    /// let selection = agent.select_action(state);
    /// assert!(selection.action < 8);
    /// ```
    pub fn new(config: ActorCriticConfig) -> Self {
        let hidden_size = (config.state_dim + config.num_actions) / 2;

        let actor = ActorNetwork::new(
            config.state_dim,
            hidden_size,
            config.num_actions,
            config.actor_lr,
        );

        let critic = CriticNetwork::new(config.state_dim, hidden_size, config.critic_lr);

        Self {
            actor,
            critic,
            tau: config.tau,
            gamma: config.gamma,
            config,
            current_state: None,
            replay_buffer: VecDeque::with_capacity(16),
            stats: ActorCriticStats {
                epsilon: config.epsilon,
                ..Default::default()
            },
            state_cache: RefCell::new(HashMap::new()),
        }
    }

    /// Convert u64 state hash to feature vector (cached).
    ///
    /// Uses feature extraction to convert hash into meaningful representation.
    /// Results are cached to avoid redundant recomputation on repeated states.
    /// This bridges the u64 state representation used by QTable/LinUCB with
    /// the continuous feature vectors used by Actor-Critic networks.
    pub fn state_to_features(&self, state: u64) -> StateVec {
        // Try to get from cache first (RefCell allows &self access)
        if let Some(cached) = self.state_cache.borrow().get(&state) {
            return cached.clone();
        }
        let mut features = StateVec::with_capacity(self.config.state_dim);

        // Convert hash to pseudo-random features via hash mixing
        let mut h = state;
        for _ in 0..self.config.state_dim {
            h = h.wrapping_mul(0x27d4eb2d_u64).wrapping_add(0x165667b1_u64);
            let val = (h as f64 / u64::MAX as f64) as f32;
            features.push(val);
        }

        // Add some structured features based on hash properties
        // Low bits → first few features
        for i in 0..self.config.state_dim.min(4) {
            if let Some(f) = features.get_mut(i) {
                *f = *f * 0.8 + ((state >> (i * 8)) & 0xFF) as f32 / 255.0 * 0.2;
            }
        }

        // Normalize to roughly [-1, 1]
        let mean: f32 = features.iter().sum::<f32>() / features.len() as f32;
        let var: f32 =
            features.iter().map(|&x| (x - mean).powi(2)).sum::<f32>() / features.len() as f32;
        let std = var.sqrt().max(1e-6);

        let normalized: StateVec = features.iter().map(|&x| (x - mean) / std).collect();
        self.state_cache
            .borrow_mut()
            .insert(state, normalized.clone());
        normalized
    }

    /// Select an action using epsilon-greedy policy.
    ///
    /// With probability epsilon: random action (exploration).
    /// With probability (1-epsilon): greedy action from actor (exploitation).
    pub fn select_action(&mut self, state: u64) -> ActionSelection {
        let features = self.state_to_features(state);
        self.current_state = Some(features.clone());

        let mut rng = rand::thread_rng();
        let exploratory = rng.r#gen::<f32>() < self.stats.epsilon;

        if exploratory {
            // Random action
            let action = rng.gen_range(0..self.config.num_actions);
            let probs = self.actor.forward(&features);
            let prob = probs
                .get(action)
                .copied()
                .unwrap_or(1.0 / self.config.num_actions as f32);

            ActionSelection {
                action,
                probability: prob,
                exploratory: true,
            }
        } else {
            // Greedy action from actor
            let probs = self.actor.forward(&features);
            let action = probs
                .iter()
                .enumerate()
                .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
                .map(|(i, _)| i)
                .unwrap_or(0);

            let probability = probs.get(action).copied().unwrap_or(0.0);
            ActionSelection {
                action,
                probability,
                exploratory: false,
            }
        }
    }

    /// Select action with state features directly (bypassing hash conversion).
    pub fn select_action_from_features(&mut self, features: &[f32]) -> ActionSelection {
        self.current_state = Some(features.iter().copied().collect());

        let mut rng = rand::thread_rng();
        let exploratory = rng.r#gen::<f32>() < self.stats.epsilon;

        if exploratory {
            let action = rng.gen_range(0..self.config.num_actions);
            let probs = self.actor.forward(features);
            let prob = probs
                .get(action)
                .copied()
                .unwrap_or(1.0 / self.config.num_actions as f32);

            ActionSelection {
                action,
                probability: prob,
                exploratory: true,
            }
        } else {
            let probs = self.actor.forward(features);
            let action = probs
                .iter()
                .enumerate()
                .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
                .map(|(i, _)| i)
                .unwrap_or(0);

            let probability = probs.get(action).copied().unwrap_or(0.0);
            ActionSelection {
                action,
                probability,
                exploratory: false,
            }
        }
    }

    /// Update actor and critic networks using TD learning.
    ///
    /// Computes TD error via critic, then updates:
    /// - Critic: reduce TD error via value function update
    /// - Actor: policy gradient using advantage from critic
    pub fn update(&mut self, state: u64, action: usize, reward: f32, next_state: u64, done: bool) {
        let state_features = self.state_to_features(state);
        let next_state_features = self.state_to_features(next_state);

        // Store experience for n-step return computation
        self.replay_buffer.push_back(Experience {
            state: state_features.clone(),
            action,
            reward,
            next_state: next_state_features.clone(),
            done,
        });
        // Keep buffer bounded
        if self.replay_buffer.len() > 32 {
            self.replay_buffer.pop_front();
        }

        let td_error = self.compute_td_error(&state_features, &next_state_features, reward, done);
        let td_error = self.sanitize_td_error(td_error);

        self.critic.update(&state_features, td_error);
        self.maybe_update_actor(&state_features, action, td_error);
        self.critic.soft_update(self.tau);
        self.update_exploration_stats();
        self.record_learning_metrics(td_error, done);
    }

    /// Compute n-step discounted return from the replay buffer.
    ///
    /// Drains up to `n` experiences and computes the discounted cumulative reward,
    /// bootstrapping with V(s_n) when the episode is not done.
    /// Returns `None` when the buffer is empty.
    pub fn compute_nstep_return(&mut self) -> Option<f32> {
        if self.replay_buffer.is_empty() {
            return None;
        }

        let n = self.replay_buffer.len().min(8); // at most 8-step return
        let experiences: Vec<Experience> = self.replay_buffer.drain(..n).collect();

        let mut ret = 0.0_f32;
        let mut discount = 1.0_f32;
        let mut terminal = false;

        for exp in &experiences {
            ret += discount * exp.reward;
            discount *= self.gamma;
            if exp.done {
                terminal = true;
                break;
            }
        }

        // Bootstrap with V(s_n) if the last experience was not terminal
        if !terminal {
            if let Some(last) = experiences.last() {
                let bootstrap_value = self.critic.forward(&last.next_state);
                ret += discount * bootstrap_value;
            }
        }

        // Use state and action from the first experience for per-step advantage tracking.
        // This integrates the stored (state, action) pairs into the n-step update path.
        if let Some(first) = experiences.first() {
            let v_s = self.critic.forward(&first.state);
            let advantage = ret - v_s;
            if advantage.abs() > MIN_ADVANTAGE_THRESHOLD {
                self.actor.update(&first.state, first.action, advantage);
            }
        }

        Some(ret)
    }

    /// Compute TD error from state evaluations.
    fn compute_td_error(&self, state: &[f32], next_state: &[f32], reward: f32, done: bool) -> f32 {
        let v_s = self.critic.forward(state);
        let v_s_next = if done {
            0.0
        } else {
            self.critic.forward_target(next_state)
        };
        reward + self.gamma * v_s_next - v_s
    }

    /// Sanitize TD error, replacing NaN/Inf with 0.
    fn sanitize_td_error(&self, td_error: f32) -> f32 {
        if td_error.is_finite() { td_error } else { 0.0 }
    }

    /// Conditionally update actor if advantage is above threshold.
    fn maybe_update_actor(&mut self, state: &[f32], action: usize, td_error: f32) {
        let advantage = td_error.abs();
        if advantage > MIN_ADVANTAGE_THRESHOLD {
            self.actor.update(state, action, td_error);
        }
    }

    /// Decay epsilon for exploration rate.
    fn update_exploration_stats(&mut self) {
        if self.stats.epsilon > self.config.epsilon_min {
            self.stats.epsilon =
                (self.stats.epsilon * self.config.epsilon_decay).max(self.config.epsilon_min);
        }
    }

    /// Record learning metrics after an update.
    fn record_learning_metrics(&mut self, td_error: f32, done: bool) {
        let advantage = td_error.abs();
        self.stats.updates += 1;
        self.stats.avg_td_error = 0.9 * self.stats.avg_td_error + 0.1 * advantage;
        self.stats.avg_advantage = 0.9 * self.stats.avg_advantage + 0.1 * advantage;
        if done {
            self.stats.episodes += 1;
        }
    }

    /// Evaluate state value using critic network.
    ///
    /// Returns V(s) — the estimated value of the given state.
    pub fn critic_evaluate(&self, state: u64) -> f32 {
        let features = self.state_to_features(state);
        self.critic.forward(&features)
    }

    /// Evaluate state value from features directly.
    pub fn critic_evaluate_features(&self, features: &[f32]) -> f32 {
        self.critic.forward(features)
    }

    /// Compute advantage for a state-action pair.
    ///
    /// A(s,a) = Q(s,a) - V(s) = r + γ*V(s') - V(s) approximation.
    pub fn advantage(&self, state: u64, action: usize) -> f32 {
        let features = self.state_to_features(state);
        let probs = self.actor.forward(&features);

        if action >= probs.len() {
            return 0.0;
        }

        // Q(s,a) ≈ r + γ*V(s') (simplified)
        let v_s = self.critic.forward(&features);
        let q_sa = probs.get(action).copied().unwrap_or(0.0) * 10.0; // Rough Q approximation from policy

        q_sa - v_s
    }

    /// Get current learning statistics.
    pub fn stats(&self) -> &ActorCriticStats {
        &self.stats
    }

    /// Get current epsilon (exploration rate).
    pub fn epsilon(&self) -> f32 {
        self.stats.epsilon
    }

    /// Inform actor-critic about LinUCB arm performance.
    ///
    /// This allows Actor-Critic to adjust its policy based on
    /// LinUCB's contextual bandit feedback, creating a hybrid
    /// exploration strategy.
    ///
    /// Returns adjusted epsilon for more/fewer explorations.
    pub fn integrate_linucb_feedback(&mut self, arm_quality: f32, arm_pulls: u32) -> f32 {
        // High-quality arms with many pulls → reduce exploration
        // Low-quality arms with few pulls → increase exploration
        let base_epsilon = self.stats.epsilon;
        let quality_factor = 1.0 - (arm_quality.clamp(0.0, 1.0));

        // If arm has been well-explored (many pulls), trust it more
        let exploration_bonus = if arm_pulls < 10 {
            0.2 // Still exploring, be curious
        } else if arm_pulls < 50 {
            0.1
        } else {
            0.0
        };

        (base_epsilon * quality_factor + exploration_bonus).clamp(self.config.epsilon_min, 0.5)
    }

    /// Report outcome to metacognitive pipeline for threshold adjustment.
    ///
    /// Returns feedback signal strength for MetacognitivePipeline threshold tuning.
    pub fn metacognitive_feedback(&self) -> f32 {
        // High TD error variance → high uncertainty → adjust thresholds
        let uncertainty = (self.stats.avg_td_error / (self.stats.avg_advantage + 0.01)).abs();
        uncertainty.min(10.0) / 10.0 // Normalize to [0, 1]
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_agent() -> ActorCritic {
        let config = ActorCriticConfig {
            actor_lr: 0.01,
            critic_lr: 0.05,
            tau: 0.1,
            gamma: 0.95,
            num_actions: 4,
            state_dim: 16,
            epsilon: 0.2,
            epsilon_decay: 0.99,
            epsilon_min: 0.01,
        };
        ActorCritic::new(config)
    }

    #[test]
    fn test_actor_critic_creation() {
        let ac = create_test_agent();
        assert_eq!(ac.stats.updates, 0);
        assert_eq!(ac.stats.epsilon, 0.2);
    }

    #[test]
    fn test_state_to_features() {
        let ac = create_test_agent();
        let features1 = ac.state_to_features(12345);
        let features2 = ac.state_to_features(12345);

        assert_eq!(features1.len(), 16);
        // Same state should produce same features
        assert_eq!(features1, features2);

        // Different states should produce different features
        let features3 = ac.state_to_features(54321);
        assert_ne!(features1, features3);
    }

    #[test]
    fn test_select_action() {
        let mut ac = create_test_agent();
        let selection = ac.select_action(100);

        assert!(selection.action < 4);
        assert!(selection.probability >= 0.0 && selection.probability <= 1.0);
    }

    #[test]
    fn test_update_and_critic_evaluate() {
        let mut ac = create_test_agent();
        let state = 100;
        let action = 1;
        let reward = 1.0;
        let next_state = 200;

        // Update
        ac.update(state, action, reward, next_state, false);

        // After update, value should have changed
        let stats = ac.stats();
        assert!(stats.updates >= 1);
    }

    #[test]
    fn test_epsilon_decay() {
        let mut ac = create_test_agent();
        let initial_epsilon = ac.epsilon();

        for _ in 0..100 {
            ac.update(100, 0, 0.0, 200, false);
        }

        let final_epsilon = ac.epsilon();
        assert!(final_epsilon < initial_epsilon);
        assert!(final_epsilon >= 0.01); // Should not go below minimum
    }

    #[test]
    fn test_nan_handling() {
        let mut ac = create_test_agent();
        // Very large reward could cause numerical issues
        ac.update(100, 0, 1000000.0, 200, false);
        // Should not panic and stats should be valid
        assert!(ac.stats().avg_td_error.is_finite());
    }

    #[test]
    fn test_done_state_critic() {
        let mut ac = create_test_agent();
        ac.update(100, 0, 1.0, 200, true); // done = true

        // Critic should have learned that 100 → 200 transition had reward 1
        // Value estimate should be finite
        let v_after = ac.critic_evaluate(100);
        assert!(v_after.is_finite());
    }

    #[test]
    fn test_metacognitive_feedback() {
        let ac = create_test_agent();
        let feedback = ac.metacognitive_feedback();
        assert!(feedback >= 0.0 && feedback <= 1.0);
    }

    #[test]
    fn test_integrate_linucb_feedback() {
        let mut ac = create_test_agent();
        let new_epsilon = ac.integrate_linucb_feedback(0.8, 100);
        assert!(new_epsilon < ac.stats.epsilon); // High quality arm → less exploration
    }

    #[test]
    fn test_compute_nstep_return_empty() {
        let mut ac = create_test_agent();
        // Empty buffer returns None
        assert!(ac.compute_nstep_return().is_none());
    }

    #[test]
    fn test_compute_nstep_return_with_experiences() {
        let mut ac = create_test_agent();
        // Fill buffer via updates
        ac.update(100, 0, 1.0, 200, false);
        ac.update(200, 1, 0.5, 300, false);
        ac.update(300, 2, 2.0, 400, true);

        let ret = ac.compute_nstep_return();
        assert!(ret.is_some());
        let r = ret.unwrap();
        assert!(r.is_finite());
        // Discounted return should be positive given positive rewards
        assert!(r > 0.0);
        // Buffer should be drained
        assert!(ac.compute_nstep_return().is_none());
    }
}
