//! `GenerateError` — 24 variants covering all failure modes of the generator pipeline.

use crate::generator::kinds::GeneratorKind;
use crate::plan::schema::CilaLevel;
use thiserror::Error;
use uuid::Uuid;

/// Convenient `Result` alias for the touring-generator crate.
pub type Result<T, E = GenerateError> = std::result::Result<T, E>;

/// Primary error type for the touring-generator crate.
///
/// Every variant carries sufficient context for diagnostic display and
/// for the `FailureReport` to be populated without additional lookups.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum GenerateError {
    /// A required symbol was not found in the index.
    #[error("VGP failed: {missing_count} symbol(s) missing, {collision_count} collision(s)")]
    VgpFailed {
        /// Number of required symbols that were absent from the index.
        missing_count: usize,
        /// Number of symbols that collided with existing definitions.
        collision_count: usize,
        /// Identifier of the plan that failed verification.
        plan_id: Uuid,
    },

    /// Speculative validation score is below the required threshold.
    #[error("speculate score {actual:.3} below threshold {required:.3} for plan {plan_id}")]
    SpeculateBelowThreshold {
        /// The speculative validation score that was actually computed.
        actual: f64,
        /// The minimum score required to pass the gate.
        required: f64,
        /// Identifier of the plan whose speculation fell short.
        plan_id: Uuid,
    },

    /// Template rendering failed.
    #[error("template render error in engine {engine:?}: {message}")]
    TemplateError {
        /// The rendering engine that raised the error.
        engine: RenderEngine,
        /// Human-readable description of the rendering failure.
        message: String,
    },

    /// Template name not found in the pre-compiled registry.
    #[error("template '{template_id}' not found for kind {kind:?}")]
    TemplateNotFound {
        /// The template identifier that was looked up.
        template_id: String,
        /// The generator kind the template was expected to serve.
        kind: GeneratorKind,
    },

    /// A template variable key was rejected by the allowlist validator.
    #[error("template variable '{key}' rejected: {reason}")]
    TemplateVariableRejected {
        /// The variable key that failed the allowlist check.
        key: String,
        /// Why the variable was rejected.
        reason: String,
    },

    /// A [`NormalizedScore`](crate::core::score::NormalizedScore) was constructed with a value outside [0.0, 1.0].
    #[error("score {value} is out of range [0.0, 1.0]")]
    ScoreOutOfRange {
        /// The offending value that fell outside `[0.0, 1.0]`.
        value: f64,
    },

    /// Path traversal attempt detected in output path.
    #[error("path traversal denied: '{path}' escapes project root")]
    PathTraversalDenied {
        /// The output path that attempted to escape the project root.
        path: String,
    },

    /// Potential secret detected in generated content.
    #[error("secret leak detected in field '{field}' — refusing to commit")]
    SecretLeakDetected {
        /// The field of generated content where a potential secret was found.
        field: String,
    },

    /// Plan schema version mismatch.
    #[error(
        "schema version mismatch: plan is v{plan_version} but engine supports v{engine_version}"
    )]
    SchemaVersionMismatch {
        /// Schema version declared by the incoming plan.
        plan_version: String,
        /// Schema version the engine actually supports.
        engine_version: String,
    },

    /// Capacity limit exceeded.
    #[error("capacity limit '{limit}' exceeded: requested {requested}, allowed {allowed}")]
    CapacityExceeded {
        /// Name of the capacity limit that was breached.
        limit: String,
        /// The amount that was requested.
        requested: u64,
        /// The maximum amount permitted.
        allowed: u64,
    },

    /// Max iterations reached without successful completion.
    #[error("max iterations ({max}) reached for plan {plan_id} — escalating to human")]
    MaxIterationsReached {
        /// Identifier of the plan that exhausted its retry budget.
        plan_id: Uuid,
        /// The maximum number of iterations that was allowed.
        max: u8,
    },

    /// Circuit breaker tripped.
    #[error("circuit breaker tripped for plan {plan_id}: {reason}")]
    CircuitBreaker {
        /// Identifier of the plan that tripped the breaker.
        plan_id: Uuid,
        /// Why the circuit breaker engaged.
        reason: String,
    },

    /// Plan was rejected by human review.
    #[error("plan {plan_id} rejected by human: {reason}")]
    HumanRejected {
        /// Identifier of the plan that was rejected.
        plan_id: Uuid,
        /// Reviewer-provided reason for the rejection.
        reason: String,
    },

    /// Backup of an existing file failed before commit.
    #[error("backup failed for '{path}': {source}")]
    BackupFailure {
        /// The file path whose backup could not be created.
        path: String,
        /// The underlying I/O error that caused the backup to fail.
        #[source]
        source: std::io::Error,
    },

    /// Atomic write commit detected a race condition.
    #[error("commit race condition for plan {plan_id}: file '{path}' modified externally")]
    CommitRaceCondition {
        /// Identifier of the plan whose commit raced.
        plan_id: Uuid,
        /// The file path that was modified externally during the commit.
        path: String,
    },

    /// I/O error during file operations.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// JSON serialization/deserialization error.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// Tokio task join error (from [`tokio::task::spawn_blocking`]).
    #[error("task join error: {0}")]
    TaskJoinError(#[from] tokio::task::JoinError),

    /// An invariant contract was violated.
    #[error("invariant violated: {invariant_id} — {detail}")]
    InvariantViolated {
        /// Identifier of the invariant contract that was broken.
        invariant_id: String,
        /// Description of how the invariant was violated.
        detail: String,
    },

    /// Plan validation failed (structural checks).
    #[error("validation failed for plan {plan_id}: {failed_count} check(s) failed")]
    ValidationFailed {
        /// Identifier of the plan that failed validation.
        plan_id: Uuid,
        /// Number of structural checks that failed.
        failed_count: usize,
    },

    /// The requested generator kind is not supported.
    #[error("unsupported generator kind: {kind:?}")]
    UnsupportedKind {
        /// The generator kind that is not supported.
        kind: GeneratorKind,
    },

    /// The requested CILA level exceeds the engine's current budget.
    #[error("CILA level {level:?} exceeds current complexity budget")]
    CilaBudgetExceeded {
        /// The requested CILA level that exceeded the budget.
        level: CilaLevel,
    },

    /// LLM provider returned an error.
    #[error("LLM provider '{provider}' error: {message}")]
    LlmError {
        /// Name of the LLM provider that returned the error.
        provider: String,
        /// Provider-supplied error message.
        message: String,
    },

    /// Memory provider error (store/recall).
    #[error("memory error: {0}")]
    MemoryError(String),

    /// Generator typestate attempted to edit a frozen skip-region (Q-310 blocking).
    #[error("skip-region violation: {0}")]
    Q310(String),

    /// Generic internal error (last resort — prefer specific variants).
    #[error("internal error: {0}")]
    Internal(String),
}

/// Template rendering engine identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderEngine {
    /// Tera template engine (primary).
    Tera,
    /// Raw string interpolation (fallback for simple cases).
    Raw,
}

// ─────────────────────────────────────────────────────────────────────────────
// Wave Q4 (RFC-100): DiagnosticCode mapping for the G- range (400..499).
//
// Only error variants that map to a stable, machine-checkable failure mode
// receive a code. Internal / generic variants (Internal, LlmError,
// MemoryError, etc.) intentionally return None via to_diagnostic_opt().
// ─────────────────────────────────────────────────────────────────────────────

use touring_foundation::diagnostic::{Diagnostic, Severity, codes};

impl GenerateError {
    /// Returns the canonical diagnostic code for this error variant, or
    /// `None` for variants that do not have a stable code.
    ///
    /// Mapping (RFC-100 §5):
    /// - `VgpFailed` → `G-400`
    /// - `SpeculateBelowThreshold` → `G-401`
    /// - `TemplateError` / `TemplateNotFound` / `TemplateVariableRejected` → `G-420`
    #[must_use]
    pub fn diagnostic_code(&self) -> Option<&'static str> {
        match self {
            Self::VgpFailed { .. } => Some(codes::G_400_VGP_FAILED),
            Self::SpeculateBelowThreshold { .. } => Some(codes::G_401_SPECULATE_LOW),
            Self::TemplateError { .. }
            | Self::TemplateNotFound { .. }
            | Self::TemplateVariableRejected { .. } => Some(codes::G_420_RENDER_ANTIPATTERNS),
            _ => None,
        }
    }

    /// Severity classification — most generator errors are unrecoverable.
    #[must_use]
    pub fn diagnostic_severity(&self) -> Severity {
        Severity::Error
    }

    /// Build a diagnostic for variants that have a code; `None` otherwise.
    ///
    /// Adds a help line pointing to the relevant remediation.
    #[must_use]
    pub fn to_diagnostic_opt(&self) -> Option<Diagnostic> {
        let code = self.diagnostic_code()?;
        let d = Diagnostic::new(code, self.diagnostic_severity(), self.to_string());
        Some(match self {
            Self::VgpFailed { .. } => {
                d.with_help("verify symbols via `touring index find <name>` and remove unverified entries from the plan")
            }
            Self::SpeculateBelowThreshold { .. } => {
                d.with_help("review template output for antipatterns; speculate score must be >= threshold")
            }
            Self::TemplateError { .. }
            | Self::TemplateNotFound { .. }
            | Self::TemplateVariableRejected { .. } => {
                d.with_help("inspect the template registry via `touring generate template-list` and verify variable allowlist")
            }
            _ => d,
        })
    }
}

// Static success-path emitter — exposes the G-410 code as a constructor
// that callers can invoke after speculate passes (defaulted Severity::Info).
/// Construct a `G-410` "speculate passed" informational diagnostic.
///
/// Used by the typestate transition Speculated → Committed to record a
/// positive outcome alongside the existing RL reward injection.
#[must_use]
pub fn diagnostic_speculate_passed(score: f64, plan_id: Uuid) -> Diagnostic {
    Diagnostic::new(
        codes::G_410_SPECULATE_PASSED,
        Severity::Info,
        format!("speculate passed for plan {plan_id} with score {score:.3}"),
    )
    .with_help("artifact committed atomically; RL reward injected for orchestrate")
}

#[cfg(test)]
mod diagnostic_tests {
    use super::*;
    use uuid::Uuid;

    fn dummy_uuid() -> Uuid {
        Uuid::nil()
    }

    #[test]
    fn vgp_failed_maps_to_g400() {
        let e = GenerateError::VgpFailed {
            missing_count: 3,
            collision_count: 1,
            plan_id: dummy_uuid(),
        };
        assert_eq!(e.diagnostic_code(), Some("G-400"));
    }

    #[test]
    fn speculate_below_threshold_maps_to_g401() {
        let e = GenerateError::SpeculateBelowThreshold {
            actual: 0.5,
            required: 0.8,
            plan_id: dummy_uuid(),
        };
        assert_eq!(e.diagnostic_code(), Some("G-401"));
    }

    #[test]
    fn template_error_maps_to_g420() {
        let e = GenerateError::TemplateError {
            engine: RenderEngine::Tera,
            message: "missing var".to_string(),
        };
        assert_eq!(e.diagnostic_code(), Some("G-420"));
    }

    #[test]
    fn internal_variants_have_no_code() {
        let e = GenerateError::Internal("anything".to_string());
        assert_eq!(e.diagnostic_code(), None);
        assert_eq!(e.to_diagnostic_opt(), None);
    }

    #[test]
    fn diagnostic_carries_help() {
        let e = GenerateError::VgpFailed {
            missing_count: 1,
            collision_count: 0,
            plan_id: dummy_uuid(),
        };
        let d = e.to_diagnostic_opt().expect("vgp_failed has a code");
        assert!(d.help.is_some());
        assert!(
            d.help
                .as_deref()
                .unwrap_or("")
                .contains("touring index find")
        );
    }

    #[test]
    fn json_round_trip_preserves_code() {
        let e = GenerateError::SpeculateBelowThreshold {
            actual: 0.5,
            required: 0.8,
            plan_id: dummy_uuid(),
        };
        let d = e.to_diagnostic_opt().expect("has code");
        let json = serde_json::to_string(&d).unwrap_or_default();
        assert!(json.contains("\"code\":\"G-401\""), "json: {json}");
        assert!(json.contains("\"severity\":\"error\""), "json: {json}");
    }

    #[test]
    fn speculate_passed_emits_g410_info() {
        let d = diagnostic_speculate_passed(0.92, dummy_uuid());
        assert_eq!(d.code, "G-410");
        assert_eq!(d.severity, Severity::Info);
        assert!(d.message.contains("0.92"));
        assert!(d.help.is_some());
    }

    #[test]
    fn all_g_codes_pass_validation() {
        for code in [
            codes::G_400_VGP_FAILED,
            codes::G_401_SPECULATE_LOW,
            codes::G_410_SPECULATE_PASSED,
            codes::G_420_RENDER_ANTIPATTERNS,
        ] {
            assert!(Diagnostic::is_valid_code(code), "invalid: {code}");
        }
    }
}
