//! `GenerateResult`, `VgpReport`, `ValidationReport`, `SpeculateReport`, `RenderedFile` and related types.

use crate::core::score::NormalizedScore;
use crate::generator::kinds::GeneratorKind;
use crate::plan::contracts::SymbolRef;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Final result returned after a plan completes (successfully or not).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GenerateResult {
    /// Unique identifier of the plan this result belongs to.
    pub plan_id: Uuid,
    /// Generator kind that produced this result.
    pub kind: GeneratorKind,
    /// Final lifecycle status reached by the plan.
    pub status: ExecutionStatus,
    /// Files generated and written to disk during execution.
    pub artifacts: Vec<Artifact>,
    /// Result of VGP symbol verification.
    pub vgp_report: VgpReport,
    /// Result of speculative (shadow) validation.
    pub speculate_report: SpeculateReport,
    /// Result of structural and invariant validation.
    pub validation_report: ValidationReport,
    /// Whether the changes were committed to disk.
    pub committed: bool,
    /// Whether a rollback (backup restore) is still available.
    pub rollback_available: bool,
    /// Memory key under which this plan outcome was persisted, if any.
    pub memory_key: Option<String>,
    /// Reinforcement-learning reward injected for this outcome, if any.
    pub rl_reward_injected: Option<NormalizedScore>,
    /// Total wall-clock time the plan took, in milliseconds.
    pub elapsed_ms: u64,
    /// LLM token usage accumulated during execution.
    pub token_usage: TokenUsage,
}

/// Current lifecycle status of a plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum ExecutionStatus {
    /// Plan created but not yet verified.
    Draft,
    /// Symbols verified via VGP.
    Verified,
    /// Templates rendered into file content.
    Rendered,
    /// Speculative validation completed.
    Speculated,
    /// Awaiting human approval before committing.
    AwaitingHuman,
    /// Changes successfully committed to disk.
    Committed,
    /// Dry run finished without writing to disk.
    DryRunComplete,
    /// Previously committed changes were rolled back.
    RolledBack,
    /// Plan rejected (by a gate or by a human).
    Rejected,
    /// Plan failed during execution.
    Failed,
}

/// A generated artifact (file written to disk).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Artifact {
    /// Destination path (UTF-8 string, relative to project root).
    pub path: String,
    /// Hex-encoded SHA-256 digest of the written file content.
    pub sha256: String,
    /// Number of bytes written to disk.
    pub bytes_written: u64,
    /// Backup path if an existing file was overwritten.
    pub backup_path: Option<String>,
    /// Action taken on the file (create, overwrite, append, etc.).
    pub action: FileAction,
}

/// What action was taken on the artifact file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum FileAction {
    /// A new file was created.
    Created,
    /// An existing file was fully replaced.
    Overwritten,
    /// Content was appended to an existing file.
    Appended,
    /// An existing file was patched in place.
    Patched,
    /// No write occurred (dry-run mode).
    DryRun,
}

/// Report from VGP batch verification.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct VgpReport {
    /// Identifier of the plan that was verified.
    pub plan_id: Uuid,
    /// Whether every symbol passed verification.
    pub all_passed: bool,
    /// Symbols that were confirmed to exist in the index.
    pub verified_symbols: Vec<SymbolRef>,
    /// Symbols that were expected but not found in the index.
    pub missing_symbols: Vec<crate::plan::failure::MissingSymbol>,
    /// Symbols that collide with already-existing definitions.
    pub collisions: Vec<SymbolRef>,
    /// Time spent on verification, in milliseconds.
    pub elapsed_ms: u64,
    /// Number of VGP cache hits during verification.
    pub cache_hits: u32,
    /// Number of VGP cache misses during verification.
    pub cache_misses: u32,
}

impl VgpReport {
    /// Construct an empty passing report (used when contracts are empty).
    #[must_use]
    pub fn empty_pass(plan_id: Uuid) -> Self {
        Self {
            plan_id,
            all_passed: true,
            verified_symbols: Vec::new(),
            missing_symbols: Vec::new(),
            collisions: Vec::new(),
            elapsed_ms: 0,
            cache_hits: 0,
            cache_misses: 0,
        }
    }
}

/// Report from structural validation checks.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ValidationReport {
    /// Results of structural (syntax/shape) checks.
    pub structural_checks: Vec<CheckResult>,
    /// Results of invariant (semantic guarantee) checks.
    pub invariant_checks: Vec<InvariantCheckResult>,
    /// Whether all checks passed.
    pub all_passed: bool,
    /// Non-fatal warnings raised during validation.
    pub warnings: Vec<String>,
    /// Fatal errors raised during validation.
    pub errors: Vec<String>,
}

impl ValidationReport {
    /// Construct a trivially passing report.
    #[must_use]
    pub fn pass() -> Self {
        Self {
            structural_checks: Vec::new(),
            invariant_checks: Vec::new(),
            all_passed: true,
            warnings: Vec::new(),
            errors: Vec::new(),
        }
    }
}

/// Result of a single structural check.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CheckResult {
    /// Name of the structural check.
    pub name: String,
    /// Whether the check passed.
    pub passed: bool,
    /// Optional human-readable detail about the result.
    pub message: Option<String>,
}

/// Result of a single invariant check.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct InvariantCheckResult {
    /// Identifier of the invariant being checked.
    pub invariant_id: String,
    /// Whether the invariant held.
    pub passed: bool,
    /// Optional supporting evidence for the verdict.
    pub evidence: Option<String>,
}

/// Report from speculative validation (touring shadow validate).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SpeculateReport {
    /// Identifier of the plan that was speculatively validated.
    pub plan_id: Uuid,
    /// Aggregate score across all speculate layers.
    pub composite_score: NormalizedScore,
    /// Whether every layer passed.
    pub all_passed: bool,
    /// Per-layer speculate results.
    pub layers: Vec<LayerResult>,
    /// Time spent on speculation, in milliseconds.
    pub elapsed_ms: u64,
}

impl SpeculateReport {
    /// Construct a trivially passing report with score 1.0.
    #[must_use]
    pub fn pass(plan_id: Uuid) -> Self {
        Self {
            plan_id,
            composite_score: NormalizedScore::ONE,
            all_passed: true,
            layers: Vec::new(),
            elapsed_ms: 0,
        }
    }
}

/// Result of a single speculate layer.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct LayerResult {
    /// Name of the speculate layer.
    pub name: String,
    /// Score produced by this layer.
    pub score: NormalizedScore,
    /// Whether this layer passed.
    pub passed: bool,
    /// Issues reported by this layer.
    pub issues: Vec<String>,
    /// Time spent on this layer, in milliseconds.
    pub elapsed_ms: u64,
}

/// LLM token usage for a plan execution.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct TokenUsage {
    /// Number of input (prompt) tokens consumed by the LLM.
    pub llm_input_tokens: u32,
    /// Number of output (completion) tokens produced by the LLM.
    pub llm_output_tokens: u32,
    /// Number of tokens served from the prompt cache.
    pub cached_tokens: u32,
    /// Name of the LLM provider that produced these tokens.
    pub provider: String,
}

/// An audit log entry (human override, security bypass, etc.).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AuditEntry {
    /// UTC timestamp when the audited action occurred.
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// Identity of the actor (human or system) responsible.
    pub actor: String,
    /// Description of the action that was audited.
    pub action: String,
    /// Identifier of the plan the action applied to.
    pub plan_id: Uuid,
    /// Optional reason or justification for the action.
    pub reason: Option<String>,
    /// Whether the action was approved.
    pub approved: bool,
}

/// A template rendering error entry.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TemplateError {
    /// Identifier of the template that failed to render.
    pub template_id: String,
    /// Human-readable rendering error message.
    pub message: String,
    /// Line number in the template where the error occurred, if known.
    pub line: Option<u32>,
}

/// A rendered file before it is committed to disk (internal pipeline type — not serialized).
#[derive(Debug, Clone)]
pub struct RenderedFile {
    /// Destination path (UTF-8 string, relative to project root).
    pub path: String,
    /// Rendered content string.
    pub content: String,
    /// Action to take when committing.
    pub action: FileAction,
}

impl RenderedFile {
    /// Construct a new rendered file.
    #[must_use]
    pub fn new(path: impl Into<String>, content: impl Into<String>, action: FileAction) -> Self {
        Self {
            path: path.into(),
            content: content.into(),
            action,
        }
    }

    /// Compute SHA-256 of the content for integrity checking.
    #[must_use]
    pub fn sha256(&self) -> String {
        use sha2::Digest;
        let hash = sha2::Sha256::digest(self.content.as_bytes());
        format!("{hash:x}")
    }
}

/// Commit report produced after atomic write completes.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CommitReport {
    /// Identifier of the plan that was committed.
    pub plan_id: Uuid,
    /// Files written to disk during the commit.
    pub files_written: Vec<Artifact>,
    /// Time spent on the commit, in milliseconds.
    pub elapsed_ms: u64,
}
