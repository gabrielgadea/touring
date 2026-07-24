//! `FailureReport`, `FailureReason`, `NextAction` — structured failure diagnostics.

use crate::plan::contracts::SymbolRef;
use crate::plan::result::{LayerResult, TemplateError};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use uuid::Uuid;

/// Comprehensive failure report emitted when a plan cannot complete.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FailureReport {
    /// Identifier of the plan that failed.
    pub plan_id: Uuid,
    /// Zero-based retry iteration at which the failure occurred.
    pub iteration: u8,
    /// Primary categorized reason the plan failed.
    pub reason: FailureReason,
    /// Required symbols that could not be found, with fuzzy suggestions.
    pub missing_symbols: Vec<MissingSymbol>,
    /// Symbols that collided with already-existing definitions.
    pub collisions: Vec<SymbolRef>,
    /// Per-layer results for the pipeline layers that failed.
    pub failing_layers: Vec<LayerResult>,
    /// Errors raised while rendering Tera templates.
    pub template_errors: Vec<TemplateError>,
    /// Filesystem I/O errors encountered during the run.
    pub io_errors: Vec<IoErrorEntry>,
    /// Source code excerpts captured for diagnostic context.
    pub code_excerpts: Vec<CodeExcerpt>,
    /// Human-readable remediation hints.
    pub suggestions: Vec<String>,
    /// The action the engine recommends taking next.
    pub recommended_next_action: NextAction,
    /// Whether the failure warrants human intervention.
    pub escalate_to_human: bool,
    /// Total wall-clock time spent before failing, in milliseconds.
    pub elapsed_ms: u64,
}

impl FailureReport {
    /// Returns true if the failure should be escalated to a human.
    #[must_use]
    pub fn requires_escalation(&self) -> bool {
        self.escalate_to_human
    }
}

/// The primary reason a plan failed.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub enum FailureReason {
    /// Verified Generation Protocol rejected the plan's symbols.
    VgpFailed,
    /// Speculative validation scored below the acceptance threshold.
    SpeculateFailed {
        /// Speculative validation score that was achieved.
        score: f64,
        /// Minimum score required to pass.
        threshold: f64,
    },
    /// A Tera template failed to render.
    TemplateError,
    /// A filesystem I/O operation failed.
    IoError,
    /// The circuit breaker tripped and aborted execution.
    CircuitBreaker,
    /// The plan's schema version did not match the engine's.
    SchemaVersionMismatch {
        /// Schema version declared by the plan.
        plan_version: String,
        /// Schema version supported by the engine.
        engine_version: String,
    },
    /// A configured capacity or quota limit was exceeded.
    CapacityExceeded {
        /// Name of the limit that was breached.
        limit: String,
        /// Amount that was requested, exceeding the limit.
        requested: u64,
    },
    /// One or more plan invariants failed validation.
    ValidationFailed {
        /// Names of the invariants that did not hold.
        failed_invariants: Vec<String>,
    },
    /// A target path escaped the allowed root (path traversal).
    PathTraversalDenied {
        /// The offending path that was denied.
        path: String,
    },
    /// A secret was detected in generated content.
    SecretLeakDetected {
        /// Name of the field where the secret was found.
        field: String,
    },
    /// Creating a pre-commit backup failed.
    BackupFailure,
    /// A concurrent commit race was detected.
    CommitRaceCondition,
    /// A human reviewer rejected the plan.
    HumanRejected {
        /// Reason given by the human for the rejection.
        reason: String,
    },
}

/// A symbol that was required but not found, with fuzzy suggestions.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MissingSymbol {
    /// The symbol that was requested but not found.
    pub requested: SymbolRef,
    /// Fuzzy-matched candidates ranked by similarity.
    pub suggestions: Vec<FuzzySuggestion>,
    /// Names of nearby crates that might contain the symbol.
    pub nearest_crate_alternatives: Vec<String>,
}

/// A fuzzy-matched suggestion for a missing symbol.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FuzzySuggestion {
    /// The candidate symbol being suggested.
    pub symbol: SymbolRef,
    /// Edit distance between the candidate and the requested symbol.
    pub distance: u8,
    /// Confidence score for this suggestion in `[0.0, 1.0]`.
    pub confidence: f64,
    /// Where this suggestion was sourced from.
    pub source: SuggestionSource,
}

/// Where a fuzzy suggestion originated.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub enum SuggestionSource {
    /// `touring-simd` `TopKSearcher` (approximate nearest neighbor).
    SimdTopK,
    /// Simple trigram fallback.
    TrigramIndex,
    /// Past memory recall pattern.
    MemoryRecall,
    /// LLM hypothesis via `DSPy`.
    LlmHypothesis,
}

/// The recommended action after a failure.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub enum NextAction {
    /// Re-plan the generation, concentrating on the listed areas.
    Replan {
        /// Aspects the new plan should focus on.
        focus: Vec<String>,
    },
    /// Retry with an adjusted template and variable bindings.
    AdjustTemplate {
        /// Identifier of the template to adjust.
        template_id: String,
        /// Suggested variable values for the adjusted template.
        suggested_vars: BTreeMap<String, serde_json::Value>,
    },
    /// Retry after relaxing the named invariants.
    RelaxInvariants {
        /// Invariants that should be relaxed.
        which: Vec<String>,
    },
    /// Hand the failure off to a human operator.
    EscalateToHuman {
        /// Reason for the escalation.
        reason: String,
    },
    /// Submit a previously generated variant instead.
    SubmitVariant {
        /// Identifier of the variant to submit.
        variant_id: String,
    },
    /// Abandon the plan entirely.
    GiveUp {
        /// Explanation for giving up.
        rationale: String,
    },
}

/// An I/O error recorded in a failure report.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct IoErrorEntry {
    /// Path where the I/O error occurred (UTF-8 string).
    pub path: String,
    /// The I/O operation that failed (e.g. `read`, `write`, `create`).
    pub operation: String,
    /// Human-readable error message from the I/O failure.
    pub message: String,
}

/// A code excerpt included for diagnostic context.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CodeExcerpt {
    /// Source file path (UTF-8 string).
    pub file: String,
    /// First line of the excerpt (1-based, inclusive).
    pub start_line: u32,
    /// Last line of the excerpt (1-based, inclusive).
    pub end_line: u32,
    /// Raw source text of the excerpt.
    pub content: String,
    /// Line numbers to highlight within the excerpt.
    pub highlight_lines: Vec<u32>,
}
