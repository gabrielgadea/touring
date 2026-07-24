//! N1 — Tool Sequence Generation Layer.
//!
//! Generates sequences of tool calls guided by ACO pheromone trails.
//! Part of the N0→N1→N2→N3 hierarchy in Touring.
//!
//! # Architecture
//!
//! ```text
//! ObjectiveSpec ──N1──► GeneratedSequence ◄──N3──┐
//!    (raw intent)     (tool_calls + triad)            │
//!          │                                        │
//!          ▼                                        │
//!    HookRuntime ──N0──► HookResponse                │
//!          │                                        │
//!          ▼                                        │
//!    TACO Orchestrator ──N2──► ExecutionResult      │
//!          │                                        │
//!          ▼                                        │
//!    ACO Wiring ──feedback loop──► PheromoneDeposit  │
//!          │                                        │
//!          └────────────────────────────────────────┘
//! ```
//!
//! # TRIAD Pattern
//!
//! Every generated sequence implements the TRIAD pattern (transplanted from ACO
//! `generator_engine.py`):
//!
//! 1. **execute**: ordered `Vec<ToolCall>` respecting dependency graph
//! 2. **validate**: `ValidationCriteria` defining success/failure conditions
//! 3. **rollback**: `RollbackPlan` for restoring state on failure

pub mod basic_generator;
pub mod generated_sequence;
pub mod objective_spec;
pub mod pheromone_integration;
pub mod rollback_plan;
pub mod tool_call;
pub mod tool_catalog;
pub mod validation_criteria;

// Re-exports
pub use basic_generator::BasicGenerator;
pub use error::{N1Error, N1Result};
pub use generated_sequence::{GeneratedSequence, PheromoneDeposit, SequenceMetadata};
pub use objective_spec::{ObjectiveSpec, QualityGate};
pub use pheromone_integration::{PheromoneIntegrator, learn_from_outcome};
pub use rollback_plan::RollbackPlan;
pub use tool_call::{ToolCall, ToolCallId, ToolCallResult};
pub use tool_catalog::{InvocationType, ToolArg, ToolCatalog, ToolCategory, ToolDescriptor};
pub use validation_criteria::ValidationCriteria;

// ── Top-level errors ─────────────────────────────────────────────────────────

/// Error types for the N1 tool-sequence layer.
pub mod error {
    use thiserror::Error;

    /// Errors produced by N1 tool-sequence generation, validation, and rollback.
    #[derive(Debug, Error)]
    pub enum N1Error {
        /// Tool-sequence generation failed.
        #[error("generation failed: {0}")]
        GenerationFailed(String),

        /// Generated sequence failed validation.
        #[error("validation failed: {0}")]
        ValidationFailed(String),

        /// A rollback action could not be applied.
        #[error("rollback failed: {0}")]
        RollbackFailed(String),

        /// A dependency cycle was detected among tool calls.
        #[error("dependency cycle detected: {0}")]
        CyclicDependency(String),

        /// A referenced tool was not present in the catalog.
        #[error("tool not found: {0}")]
        ToolNotFound(String),

        /// A pheromone query failed.
        #[error("pheromone query failed: {0}")]
        PheromoneError(String),

        /// The supplied objective was invalid.
        #[error("invalid objective: {0}")]
        InvalidObjective(String),
    }

    /// Convenience `Result` alias for N1 operations.
    pub type N1Result<T> = Result<T, N1Error>;
}

// ── ToolSequenceGenerator trait ───────────────────────────────────────────────

use crate::rl::LearningResult;

/// Trait for generating tool sequences from objectives.
///
/// Implementors can be rule-based (basic_generator) or ML-guided
/// (future: learned generators using the ACO pheromone bus).
pub trait ToolSequenceGenerator: Send + Sync {
    /// Generate a sequence of tool calls for the given objective.
    fn generate(&self, objective: &ObjectiveSpec) -> N1Result<GeneratedSequence>;

    /// Validate a generated sequence against the objective.
    fn validate(
        &self,
        seq: &GeneratedSequence,
        objective: &ObjectiveSpec,
    ) -> N1Result<ValidationResult>;

    /// Learn from execution outcomes, updating pheromone trails.
    fn learn(&self, seq: &GeneratedSequence, outcome: &ExecutionOutcome) -> LearningResult<()>;

    /// Return the pheromone key prefix used by this generator.
    fn pheromone_prefix(&self) -> &'static str;
}

/// Outcome of executing a generated sequence.
#[derive(Debug, Clone)]
pub struct ExecutionOutcome {
    /// Whether the sequence succeeded.
    pub success: bool,
    /// Tool calls that were executed.
    pub executed: Vec<ToolCallResult>,
    /// Index of the first failure (if any).
    pub first_failure_index: Option<usize>,
    /// Quality score from 0.0 to 1.0.
    pub quality_score: f64,
    /// Error message if failed.
    pub error_message: Option<String>,
}

/// Result of validating a generated sequence.
#[derive(Debug, Clone)]
pub struct ValidationResult {
    /// Whether validation passed.
    pub passed: bool,
    /// List of validation failures.
    pub failures: Vec<String>,
    /// Confidence score from 0.0 to 1.0.
    pub confidence: f64,
}
