//! Reinforcement Learning — QTable with TD(λ) eligibility traces + Tiny Transformer.
//!
//! # Modules
//!
//! - [`qtable`] — TD(λ) Q-learning with sparse Q-table and eligibility traces.
//! - [`tiny_transformer`] — Lightweight tool prediction via Markov/transformer models.
//! - [`risk_adjusted`] — Blast-radius-aware Q-learning that modulates exploration by risk.
//! - [`ndarray_mlp`] — Context-aware arm selection via lightweight MLP (ndarray-based).
//! - [`double_qtable`] — Double Q-learning to reduce overestimation bias (van Hasselt 2015).
//! - [`per_buffer`] — Prioritized Experience Replay buffer (Schaul et al. 2016).
//! - [`curiosity`] — Count-based intrinsic curiosity bonus for state exploration.
//! - `burn_transformer` — Feed-forward transformer via the `burn` crate (feature-gated).

// feature-gated: requires dep:burn (disabled, replaced by ndarray_mlp)
#[cfg(feature = "burn-transformer")]
pub mod burn_transformer;

pub mod actor_critic;
pub mod curiosity;
pub mod double_qtable;
pub mod ndarray_mlp;
pub mod per_buffer;
pub mod qtable;
pub mod risk_adjusted;
pub mod tiny_transformer;

pub use actor_critic::{ActionSelection, ActorCritic, ActorCriticConfig, ActorCriticStats};
pub use curiosity::CuriosityModule;
pub use double_qtable::DoubleQTable;
pub use ndarray_mlp::ContextMlp;
pub use per_buffer::PrioritizedReplayBuffer;
pub use qtable::{
    LearningParams, QLearning, QTable, QTableError, StateAction, djb2_hash, global_qtable,
};
pub use risk_adjusted::RiskAdjustedQLearning;
pub use tiny_transformer::{
    MarkovPredictor, PredictionContext, TinyTransformerError, TinyTransformerPredictor,
    ToolPrediction, ToolPredictor,
};
