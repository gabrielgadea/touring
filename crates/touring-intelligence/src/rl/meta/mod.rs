//! Meta-learning — SelfOptimizer + OpportunityScorer for autonomous self-evolution.
//!
//! - [`self_optimizer`] — KEEP/DISCARD/RESET loop for hyperparameter auto-tuning.
//! - [`opportunity_scorer`] — Telemetry-driven evolution candidate ranking.

pub mod opportunity_scorer;
pub mod self_optimizer;

pub use opportunity_scorer::{
    EvolutionCandidate, OpportunityCategory, OpportunityScorer, OpportunitySummary,
    TelemetrySnapshot,
};
pub use self_optimizer::{HyperparamAdjustment, SelfOptimizer};
