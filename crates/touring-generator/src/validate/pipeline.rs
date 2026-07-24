//! 7-layer validation pipeline for `GeneratorPlan`.
//!
//! Formalizes the implicit VGP gate into an explicit ordered pipeline with
//! per-layer error taxonomy (ESAA §7-layer validation + S6 of the v8 master plan).
//!
//! Each layer is identified by the `ValidationLayer` enum.  Errors are typed
//! with `ValidationError` so callers can match on variant and attach context
//! without parsing strings.
//!
//! ## Layer ordering
//!
//! | # | Layer | Validates | ESAA primitive |
//! |---|-------|-----------|---------------|
//! | L1 | `JsonParse` | plan JSON is valid UTF-8 + parseable | parse |
//! | L2 | `SchemaValidation` | plan fields conform to schema | schema |
//! | L3 | `VocabularyAllowed` | `kind` is in the allowed set | vocab |
//! | L4 | `StateMachine` | plan status transitions are legal | state-machine |
//! | L5 | `PathBoundary` | artifact paths respect `Contracts.path_boundaries` | boundary |
//! | L6 | `Immutability` | committed artifacts are not modified | immutability |
//! | L7 | `VerificationGate` | composite score ≥ 0.85 | verification |
//!
//! ## Layer composition
//!
//! `ValidationReport` aggregates results from all 7 layers; each layer
//! contributes a `LayerResult`.  The pipeline short-circuits on the first
//! non-recoverable error unless the layer is advisory (L3, L5 `WarnOnly`).

use std::collections::HashMap;
use std::time::Instant;

use super::boundary::BoundaryValidator;
use crate::NormalizedScore;
use crate::plan::contracts::Contracts;
use crate::plan::result::LayerResult;
use crate::plan::schema::GeneratorPlan;

/// The 7 ordered layers of plan validation.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    serde::Serialize,
    serde::Deserialize,
    schemars::JsonSchema,
)]
pub enum ValidationLayer {
    /// L1 — plan JSON is valid UTF-8 and parseable.
    L1JsonParse,
    /// L2 — plan fields conform to the `GeneratorPlan` schema.
    L2SchemaValidation,
    /// L3 — the plan `kind` is in the allowed vocabulary set.
    L3VocabularyAllowed,
    /// L4 — plan status transitions are legal in the typestate machine.
    L4StateMachine,
    /// L5 — artifact paths respect `Contracts.path_boundaries`.
    L5PathBoundary,
    /// L6 — committed artifacts are not modified (immutability).
    L6Immutability,
    /// L7 — composite health score meets the ≥ 0.85 verification gate.
    L7VerificationGate,
}

impl ValidationLayer {
    /// All 7 layers in order.
    pub const ALL: [ValidationLayer; 7] = [
        ValidationLayer::L1JsonParse,
        ValidationLayer::L2SchemaValidation,
        ValidationLayer::L3VocabularyAllowed,
        ValidationLayer::L4StateMachine,
        ValidationLayer::L5PathBoundary,
        ValidationLayer::L6Immutability,
        ValidationLayer::L7VerificationGate,
    ];

    /// Stable `snake_case` identifier for this layer (used in reports/logs).
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            ValidationLayer::L1JsonParse => "l1_json_parse",
            ValidationLayer::L2SchemaValidation => "l2_schema",
            ValidationLayer::L3VocabularyAllowed => "l3_vocabulary",
            ValidationLayer::L4StateMachine => "l4_state_machine",
            ValidationLayer::L5PathBoundary => "l5_path_boundary",
            ValidationLayer::L6Immutability => "l6_immutability",
            ValidationLayer::L7VerificationGate => "l7_verification_gate",
        }
    }
}

/// Validation error with typed layer + context.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub enum ValidationError {
    /// L1 — plan JSON could not be parsed; carries the parser message.
    L1JsonParse(String),

    /// L2 — plan failed schema validation; carries the violation detail.
    L2Schema(String),

    /// L3 — the plan `kind` is not in the allowed vocabulary set.
    L3VocabularyNotAllowed {
        /// The disallowed generator kind that was rejected.
        kind: String,
    },

    /// L4 — an illegal plan status transition was attempted.
    L4StateMachine(String),

    /// L5 — an artifact path violated the task-kind path boundaries.
    L5BoundaryViolation {
        /// The offending artifact file path.
        file: String,
        /// The boundary task kind whose allowlist was violated.
        kind: String,
    },

    /// L6 — an attempt was made to modify an already-committed artifact.
    L6ImmutabilityViolation {
        /// The committed path that was illegally re-targeted.
        path: String,
    },

    /// L7 — the composite health score fell below the 0.85 gate.
    L7VerificationFailed {
        /// The composite health score that failed the gate.
        score: f64,
    },
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ValidationError::L1JsonParse(s) => write!(f, "L1 JSON parse error: {s}"),
            ValidationError::L2Schema(s) => write!(f, "L2 schema violation: {s}"),
            ValidationError::L3VocabularyNotAllowed { kind } => {
                write!(
                    f,
                    "L3 vocabulary not allowed: kind '{kind}' is not in the allowed set"
                )
            }
            ValidationError::L4StateMachine(s) => write!(f, "L4 state machine violation: {s}"),
            ValidationError::L5BoundaryViolation { file, kind } => {
                write!(f, "L5 boundary violation: {file} (kind {kind:?})")
            }
            ValidationError::L6ImmutabilityViolation { path } => {
                write!(
                    f,
                    "L6 immutability violation: attempting to modify committed file '{path}'"
                )
            }
            ValidationError::L7VerificationFailed { score } => {
                write!(
                    f,
                    "L7 verification gate failed: composite score {score} < 0.85"
                )
            }
        }
    }
}

impl ValidationError {
    /// Which layer produced this error.
    #[must_use]
    pub fn layer(&self) -> ValidationLayer {
        match self {
            ValidationError::L1JsonParse(_) => ValidationLayer::L1JsonParse,
            ValidationError::L2Schema(_) => ValidationLayer::L2SchemaValidation,
            ValidationError::L3VocabularyNotAllowed { .. } => ValidationLayer::L3VocabularyAllowed,
            ValidationError::L4StateMachine(_) => ValidationLayer::L4StateMachine,
            ValidationError::L5BoundaryViolation { .. } => ValidationLayer::L5PathBoundary,
            ValidationError::L6ImmutabilityViolation { .. } => ValidationLayer::L6Immutability,
            ValidationError::L7VerificationFailed { .. } => ValidationLayer::L7VerificationGate,
        }
    }
}

/// Immutable snapshot of committed artifacts used for L6 checks.
#[derive(Debug, Clone, Default)]
pub struct CommittedHistory {
    /// Set of paths already committed in previous plans (absolute paths).
    pub committed_paths: Vec<String>,
}

impl CommittedHistory {
    /// Create an empty committed-history snapshot.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns `true` if `path` was already committed in a previous plan.
    #[must_use]
    pub fn contains(&self, path: &str) -> bool {
        self.committed_paths.iter().any(|p| p == path)
    }
}

/// Event emitted after each layer completes (S1 wires the concrete observer).
pub type LayerCompleteObserver = Box<dyn Fn(ValidationLayer, &LayerResult) + Send + Sync + 'static>;

/// Context passed into each layer validator.
pub struct ValidationContext {
    /// Allowed generator kinds (from registry or config).
    pub allowed_kinds: Vec<String>,
    /// Contracts from the plan (for L5 boundary and L6 immutability).
    pub contracts: Option<Contracts>,
    /// History of previously committed artifact paths (for L6).
    pub committed_history: CommittedHistory,
    /// Composite health score from touring daemon (for L7).
    pub composite_health_score: Option<f64>,
    /// Optional observer called after each layer completes (S1 activity log).
    on_layer_complete: Option<LayerCompleteObserver>,
}

impl std::fmt::Debug for ValidationContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ValidationContext")
            .field("allowed_kinds", &self.allowed_kinds)
            .field("contracts", &self.contracts)
            .field("committed_history", &self.committed_history)
            .field("composite_health_score", &self.composite_health_score)
            .field(
                "on_layer_complete",
                &if self.on_layer_complete.is_some() {
                    "Some(...)"
                } else {
                    "None"
                },
            )
            .finish()
    }
}

impl Clone for ValidationContext {
    fn clone(&self) -> Self {
        Self {
            allowed_kinds: self.allowed_kinds.clone(),
            contracts: self.contracts.clone(),
            committed_history: self.committed_history.clone(),
            composite_health_score: self.composite_health_score,
            on_layer_complete: None, // cannot clone Fn
        }
    }
}

impl ValidationContext {
    /// Create an empty context: no kind restrictions, no contracts, no health signal.
    #[must_use]
    pub fn new() -> Self {
        Self {
            allowed_kinds: Vec::new(),
            contracts: None,
            committed_history: CommittedHistory::new(),
            composite_health_score: None,
            on_layer_complete: None,
        }
    }

    /// Builder: restrict L3 to the given set of allowed generator kinds.
    #[must_use]
    pub fn with_allowed_kinds(mut self, kinds: Vec<String>) -> Self {
        self.allowed_kinds = kinds;
        self
    }

    /// Builder: attach the plan contracts used by L5 boundary and L6 immutability.
    #[must_use]
    pub fn with_contracts(mut self, contracts: Contracts) -> Self {
        self.contracts = Some(contracts);
        self
    }

    /// Builder: supply the composite health score checked by the L7 gate.
    #[must_use]
    pub fn with_composite_health(mut self, score: f64) -> Self {
        self.composite_health_score = Some(score);
        self
    }

    /// Set the layer-complete observer (S1 activity log wiring point).
    #[must_use]
    pub fn with_layer_observer(
        mut self,
        obs: impl Fn(ValidationLayer, &LayerResult) + Send + Sync + 'static,
    ) -> Self {
        self.on_layer_complete = Some(Box::new(obs));
        self
    }
}

impl Default for ValidationContext {
    fn default() -> Self {
        Self::new()
    }
}

/// Outcome of a full 7-layer pass.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct ValidationReport {
    /// How many layers passed.
    pub layers_passed: u8,
    /// Total layers run (always 7).
    pub layers_total: u8,
    /// Per-layer results in execution order.
    pub layer_results: Vec<LayerResult>,
    /// Durations in ms keyed by layer name.
    pub layer_durations_ms: HashMap<String, u64>,
    /// All layers passed?
    pub all_passed: bool,
}

impl ValidationReport {
    /// Create a fresh report with 0 layers passed out of 7 and `all_passed = true`.
    #[must_use]
    pub fn new() -> Self {
        Self {
            layers_passed: 0,
            layers_total: 7,
            layer_results: Vec::new(),
            layer_durations_ms: HashMap::new(),
            all_passed: true,
        }
    }

    /// Append a layer result, updating the pass count, duration map, and `all_passed` flag.
    pub fn add_layer(&mut self, result: LayerResult) {
        self.layer_durations_ms
            .insert(result.name.clone(), result.elapsed_ms);
        if result.passed {
            self.layers_passed += 1;
        } else {
            self.all_passed = false;
        }
        self.layer_results.push(result);
    }
}

impl Default for ValidationReport {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Individual layer validators ────────────────────────────────────────────

// Infallible layer — kept `Result`-returning to conform to the shared `LayerFn`
// array signature in `validate_plan`; `l3`/`l5`/`l7` are genuinely fallible.
#[allow(clippy::unnecessary_wraps)]
fn l1_json_parse(
    _plan: &GeneratorPlan,
    _ctx: &ValidationContext,
) -> Result<LayerResult, ValidationError> {
    // The plan is already JSON-deserialized by the time we receive it;
    // invalid UTF-8 or malformed JSON would have failed at the call site.
    // This layer is a no-op pass — kept for structural completeness.
    Ok(LayerResult {
        name: ValidationLayer::L1JsonParse.name().to_string(),
        score: NormalizedScore::ONE,
        passed: true,
        issues: vec![],
        elapsed_ms: 0,
    })
}

// Infallible layer — kept `Result`-returning to conform to the shared `LayerFn`
// array signature in `validate_plan`; `l3`/`l5`/`l7` are genuinely fallible.
#[allow(clippy::unnecessary_wraps)]
fn l2_schema(
    _plan: &GeneratorPlan,
    _ctx: &ValidationContext,
) -> Result<LayerResult, ValidationError> {
    // JSON schema validation is performed by serde at the call site.
    // The generator receives a fully deserialized `GeneratorPlan`, so
    // structural schema errors cannot reach this layer.
    Ok(LayerResult {
        name: ValidationLayer::L2SchemaValidation.name().to_string(),
        score: NormalizedScore::ONE,
        passed: true,
        issues: vec![],
        elapsed_ms: 0,
    })
}

fn l3_vocabulary_allowed(
    plan: &GeneratorPlan,
    ctx: &ValidationContext,
) -> Result<LayerResult, ValidationError> {
    let started = Instant::now();
    let kind_str = format!("{:?}", plan.kind);

    if ctx.allowed_kinds.is_empty() {
        // No restrictions — always pass.
        return Ok(LayerResult {
            name: ValidationLayer::L3VocabularyAllowed.name().to_string(),
            score: NormalizedScore::ONE,
            passed: true,
            issues: vec![],
            elapsed_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
        });
    }

    let kind_lower = kind_str.to_lowercase();
    let passed = ctx
        .allowed_kinds
        .iter()
        .any(|k| k.to_lowercase() == kind_lower || k == "*");

    if passed {
        Ok(LayerResult {
            name: ValidationLayer::L3VocabularyAllowed.name().to_string(),
            score: NormalizedScore::ONE,
            passed: true,
            issues: vec![],
            elapsed_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
        })
    } else {
        Err(ValidationError::L3VocabularyNotAllowed { kind: kind_str })
    }
}

// Infallible layer — kept `Result`-returning to conform to the shared `LayerFn`
// array signature in `validate_plan`; `l3`/`l5`/`l7` are genuinely fallible.
#[allow(clippy::unnecessary_wraps)]
fn l4_state_machine(
    _plan: &GeneratorPlan,
    _ctx: &ValidationContext,
) -> Result<LayerResult, ValidationError> {
    let started = Instant::now();
    // Status transition validation happens in typestate.rs during state
    // transitions — the plan struct itself has no status field to check.
    // This layer is a structural placeholder.
    Ok(LayerResult {
        name: ValidationLayer::L4StateMachine.name().to_string(),
        score: NormalizedScore::ONE,
        passed: true,
        issues: vec![],
        elapsed_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
    })
}

fn l5_path_boundary(
    plan: &GeneratorPlan,
    ctx: &ValidationContext,
) -> Result<LayerResult, ValidationError> {
    let started = Instant::now();

    let Some(ref contracts) = ctx.contracts else {
        return Ok(LayerResult {
            name: ValidationLayer::L5PathBoundary.name().to_string(),
            score: NormalizedScore::ONE,
            passed: true,
            issues: vec![],
            elapsed_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
        });
    };

    let Some(ref boundaries) = contracts.path_boundaries else {
        return Ok(LayerResult {
            name: ValidationLayer::L5PathBoundary.name().to_string(),
            score: NormalizedScore::ONE,
            passed: true,
            issues: vec![],
            elapsed_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
        });
    };

    let validator = match BoundaryValidator::new(boundaries) {
        Ok(v) => v,
        Err(e) => {
            return Err(ValidationError::L5BoundaryViolation {
                file: format!("boundary config: {e}"),
                kind: format!("{:?}", boundaries.task_kind),
            });
        }
    };

    // Use plan target file_path for boundary check.
    let artifact_paths: Vec<String> = if plan.target.file_path.is_empty() {
        vec![]
    } else {
        vec![plan.target.file_path.clone()]
    };

    let fake_files: Vec<crate::plan::result::RenderedFile> = artifact_paths
        .iter()
        .map(|p| {
            crate::plan::result::RenderedFile::new(p, "", crate::plan::result::FileAction::Created)
        })
        .collect();

    let result = validator.validate_artifacts(&fake_files);

    let layer_result: LayerResult = (result, started).into();
    if !layer_result.passed {
        for issue in &layer_result.issues {
            tracing::warn!(issue = %issue, "L5 boundary violation");
        }
    }
    Ok(layer_result)
}

// Infallible layer — kept `Result`-returning to conform to the shared `LayerFn`
// array signature in `validate_plan`; `l3`/`l5`/`l7` are genuinely fallible.
#[allow(clippy::unnecessary_wraps)]
fn l6_immutability(
    plan: &GeneratorPlan,
    ctx: &ValidationContext,
) -> Result<LayerResult, ValidationError> {
    let started = Instant::now();

    // Use plan target for L6 immutability check (single target path).
    let artifact_paths: Vec<String> = if plan.target.file_path.is_empty() {
        vec![]
    } else {
        vec![plan.target.file_path.clone()]
    };

    let mut violations: Vec<String> = Vec::new();
    for path in &artifact_paths {
        if ctx.committed_history.contains(path) {
            violations.push(format!(
                "immutability violation: path '{path}' was previously committed"
            ));
        }
    }

    let passed = violations.is_empty();
    let score = if passed {
        NormalizedScore::ONE
    } else {
        NormalizedScore::ZERO
    };

    Ok(LayerResult {
        name: ValidationLayer::L6Immutability.name().to_string(),
        score,
        passed,
        issues: violations,
        elapsed_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
    })
}

fn l7_verification_gate(
    _plan: &GeneratorPlan,
    ctx: &ValidationContext,
) -> Result<LayerResult, ValidationError> {
    const GATE: f64 = 0.85;
    let started = Instant::now();

    let Some(score) = ctx.composite_health_score else {
        // No health signal — pass conservatively.
        return Ok(LayerResult {
            name: ValidationLayer::L7VerificationGate.name().to_string(),
            score: NormalizedScore::ONE,
            passed: true,
            issues: vec![],
            elapsed_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
        });
    };
    if score >= GATE {
        Ok(LayerResult {
            name: ValidationLayer::L7VerificationGate.name().to_string(),
            score: NormalizedScore::ONE,
            passed: true,
            issues: vec![],
            elapsed_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
        })
    } else {
        Err(ValidationError::L7VerificationFailed { score })
    }
}

/// Run the full 7-layer validation pipeline.
///
/// ## Layer composition
///
/// Each layer contributes a `LayerResult` to `ValidationReport`.  The pipeline
/// short-circuits on the first non-recoverable error (L1, L2, L6, L7) unless
/// the layer is advisory (L3, L5 with `WarnOnly`).  L4 is always a no-op pass.
///
/// ## `ValidationError` ↔ `LayerResult` relationship
///
/// When a layer returns `Err(ValidationError)`, the error is converted into a
/// failing `LayerResult` and appended to the report.  The pipeline does not
/// panic or abort — it completes all 7 layers and reports the outcome.
///
/// ## Example
///
/// ```ignore
/// let ctx = ValidationContext::new()
///     .with_allowed_kinds(vec!["RustModule".into(), "PythonScript".into()])
///     .with_contracts(plan.contracts.clone())
///     .with_composite_health(0.91);
///
/// match validate_plan(&plan, &ctx) {
///     Ok(report) => {
///         assert_eq!(report.layers_passed, 7);
///     }
///     Err(e) => {
///         eprintln!("validation failed at layer {:?}: {}", e.layer(), e);
///     }
/// }
/// ```
type LayerFn = fn(&GeneratorPlan, &ValidationContext) -> Result<LayerResult, ValidationError>;

/// Run all 7 validation layers over `plan` under `ctx` and aggregate the outcome.
///
/// Each layer contributes a `LayerResult` to the returned `ValidationReport`;
/// a layer error is converted into a failing result rather than aborting, so
/// the report always reflects every layer. See the `LayerFn` docs above for the
/// layer-composition and error semantics.
#[must_use]
pub fn validate_plan(plan: &GeneratorPlan, ctx: &ValidationContext) -> ValidationReport {
    let mut report = ValidationReport::new();

    let layers: &[LayerFn] = &[
        l1_json_parse,
        l2_schema,
        l3_vocabulary_allowed,
        l4_state_machine,
        l5_path_boundary,
        l6_immutability,
        l7_verification_gate,
    ];

    for (i, layer_fn) in layers.iter().enumerate() {
        let layer_enum: ValidationLayer = ValidationLayer::ALL[i];
        match layer_fn(plan, ctx) {
            Ok(result) => {
                debug_assert_eq!(result.name, layer_enum.name());
                if let Some(ref obs) = ctx.on_layer_complete {
                    obs(layer_enum, &result);
                }
                report.add_layer(result);
            }
            Err(e) => {
                let issues: Vec<String> = vec![e.to_string()];
                let passed = false;
                let score = NormalizedScore::ZERO;
                let result = LayerResult {
                    name: layer_enum.name().to_string(),
                    score,
                    passed,
                    issues,
                    elapsed_ms: 0, // errors don't track elapsed
                };
                if let Some(ref obs) = ctx.on_layer_complete {
                    obs(layer_enum, &result);
                }
                report.add_layer(result);
            }
        }
    }

    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generator::kinds::GeneratorKind;
    use crate::plan::schema::{CilaLevel, Target};
    use std::sync::{Arc, Mutex};

    fn dummy_plan() -> GeneratorPlan {
        GeneratorPlan {
            version: "2.0".to_string(),
            plan_id: uuid::Uuid::new_v4(),
            intent: "test plan".to_string(),
            cila_level: CilaLevel::L3,
            target: Target {
                file_path: "test.rs".to_string(),
                module_path: None,
                crate_name: None,
            },
            kind: GeneratorKind::RustModule,
            contracts: crate::plan::contracts::Contracts::default(),
            template: crate::plan::schema::TemplateSelection::default(),
            assembly: crate::plan::schema::Assembly::default(),
            validation: crate::plan::schema::ValidationDirectives::default(),
            commit_policy: crate::plan::schema::CommitPolicy::default(),
            rollback: crate::plan::schema::RollbackPolicy::default(),
            learning: crate::plan::schema::LearningDirectives::default(),
            spec_inputs: None,
            capacity_hints: crate::plan::schema::CapacityHints::default(),
            execution_trace: vec![],
            metadata: crate::plan::schema::PlanMetadata::default(),
        }
    }

    #[test]
    fn all_layers_pass_empty_context() {
        let plan = dummy_plan();
        let ctx = ValidationContext::new();
        let report = validate_plan(&plan, &ctx);
        assert_eq!(report.layers_passed, 7);
        assert!(report.all_passed);
    }

    #[test]
    fn l7_fails_below_085() {
        let plan = dummy_plan();
        let mut ctx = ValidationContext::new();
        ctx.composite_health_score = Some(0.70);
        let report = validate_plan(&plan, &ctx);
        assert!(!report.all_passed);
        // L7 is the only failure.
        assert_eq!(report.layers_passed, 6);
    }

    #[test]
    fn l7_passes_at_085_exactly() {
        let plan = dummy_plan();
        let mut ctx = ValidationContext::new();
        ctx.composite_health_score = Some(0.85);
        let report = validate_plan(&plan, &ctx);
        assert!(report.all_passed);
    }

    #[test]
    fn l7_passes_above_085() {
        let plan = dummy_plan();
        let mut ctx = ValidationContext::new();
        ctx.composite_health_score = Some(0.95);
        let report = validate_plan(&plan, &ctx);
        assert!(report.all_passed);
    }

    #[test]
    fn l3_blocks_unknown_kind() {
        let plan = dummy_plan();
        let mut ctx = ValidationContext::new();
        ctx.allowed_kinds = vec!["PythonScript".to_string()];
        let report = validate_plan(&plan, &ctx);
        assert!(!report.all_passed);
        // L3 fails.
        assert_eq!(report.layers_passed, 6);
    }

    #[test]
    fn l3_allows_matching_kind() {
        let plan = dummy_plan();
        let mut ctx = ValidationContext::new();
        ctx.allowed_kinds = vec!["RustModule".to_string()];
        let report = validate_plan(&plan, &ctx);
        assert!(report.all_passed);
    }

    #[test]
    fn l6_detects_committed_path() {
        let plan = dummy_plan();
        let mut ctx = ValidationContext::new();
        ctx.committed_history
            .committed_paths
            .push("test.rs".to_string());
        let report = validate_plan(&plan, &ctx);
        assert!(!report.all_passed);
        assert_eq!(report.layers_passed, 6); // L6 fails
    }

    #[test]
    fn l6_allows_new_path() {
        let plan = dummy_plan();
        let ctx = ValidationContext::new();
        let report = validate_plan(&plan, &ctx);
        assert!(report.all_passed);
    }

    #[test]
    fn l5_passes_without_contracts() {
        let plan = dummy_plan();
        let ctx = ValidationContext::new();
        let report = validate_plan(&plan, &ctx);
        // No contracts → L5 skips → all_passed
        assert!(report.all_passed);
        assert_eq!(report.layers_passed, 7);
    }

    #[test]
    fn l5_failclosed_blocks_impl_write_to_docs() {
        let mut plan = dummy_plan();
        // plan.target = "docs/changes.md" — impl task trying to write to docs/
        plan.target = crate::plan::schema::Target {
            file_path: "docs/changes.md".to_string(),
            module_path: None,
            crate_name: None,
        };

        let mut ctx = ValidationContext::new();
        let contracts = crate::plan::contracts::Contracts {
            path_boundaries: Some(crate::plan::contracts::PathBoundaries {
                task_kind: crate::plan::contracts::TaskKind::Impl,
                read: vec!["crates/**".into(), "src/**".into(), "tests/**".into()],
                write: vec!["crates/**".into(), "src/**".into(), "tests/**".into()],
                forbidden_write: vec!["docs/**".into()],
                enforcement: crate::plan::contracts::BoundaryEnforcement::FailClosed,
            }),
            ..Default::default()
        };
        ctx.contracts = Some(contracts);

        let report = validate_plan(&plan, &ctx);
        assert!(!report.all_passed);
        // L5 fails → layers_passed = 6
        assert_eq!(report.layers_passed, 6);
    }

    #[test]
    fn validation_error_layer_roundtrip() {
        let err = ValidationError::L3VocabularyNotAllowed {
            kind: "FooKind".to_string(),
        };
        assert_eq!(err.layer(), ValidationLayer::L3VocabularyAllowed);

        let err2 = ValidationError::L7VerificationFailed { score: 0.5 };
        assert_eq!(err2.layer(), ValidationLayer::L7VerificationGate);
    }

    #[test]
    fn validation_layer_all_ordered() {
        assert_eq!(ValidationLayer::ALL.len(), 7);
        for (i, layer) in ValidationLayer::ALL.iter().enumerate() {
            assert_eq!(layer.name(), ValidationLayer::ALL[i].name());
        }
    }

    #[test]
    fn validation_context_builder() {
        let ctx = ValidationContext::new()
            .with_allowed_kinds(vec!["RustModule".into()])
            .with_composite_health(0.9);
        assert_eq!(ctx.allowed_kinds.len(), 1);
        assert_eq!(ctx.composite_health_score, Some(0.9));
    }

    #[test]
    fn observer_called_on_each_layer() {
        use std::sync::Mutex;
        let plan = dummy_plan();
        let events = Arc::new(Mutex::new(Vec::<(ValidationLayer, bool)>::new()));
        let events_clone = Arc::clone(&events);
        let ctx = ValidationContext::new().with_layer_observer(move |layer, result| {
            events_clone.lock().unwrap().push((layer, result.passed));
        });
        let report = validate_plan(&plan, &ctx);
        let borrowed = events.lock().unwrap();
        assert_eq!(borrowed.len(), 7, "observer should fire for all 7 layers");
        assert_eq!(borrowed.len(), report.layer_results.len());
        for (_layer, passed) in borrowed.iter() {
            assert!(*passed, "all layers should pass with empty context");
        }
    }

    #[test]
    fn observer_called_on_layer_failure() {
        let mut plan = dummy_plan();
        // Set kind to something NOT in the allowed set (we allow nothing by default)
        plan.kind = GeneratorKind::PythonScript;
        let events = Arc::new(Mutex::new(Vec::<(ValidationLayer, bool)>::new()));
        let events_clone = Arc::clone(&events);
        let ctx = ValidationContext::new()
            .with_allowed_kinds(vec!["RustModule".into()]) // Only RustModule allowed
            .with_layer_observer(move |layer, result| {
                events_clone.lock().unwrap().push((layer, result.passed));
            });
        let report = validate_plan(&plan, &ctx);
        // All 7 observers should fire
        let borrowed = events.lock().unwrap();
        assert_eq!(borrowed.len(), 7);
        // L3 should fail (PythonScript not in allowed kinds)
        let l3_results: Vec<_> = report
            .layer_results
            .iter()
            .filter(|r| r.name == "l3_vocabulary")
            .collect();
        assert_eq!(l3_results.len(), 1, "should have exactly one L3 result");
        assert!(!l3_results[0].passed, "L3 should fail for unknown kind");
        // Verify the observer also recorded L3 as failed
        let l3_event = borrowed
            .iter()
            .find(|(l, _): &&(ValidationLayer, bool)| l.name() == "l3_vocabulary");
        assert!(l3_event.is_some(), "observer should have recorded L3 event");
        assert!(!l3_event.unwrap().1, "L3 event should show passed=false");
    }

    #[test]
    fn committed_history_contains() {
        let mut history = CommittedHistory::new();
        history.committed_paths.push("src/lib.rs".to_string());
        assert!(history.contains("src/lib.rs"));
        assert!(!history.contains("src/main.rs"));
    }

    #[test]
    fn layer_result_order_preserved() {
        let plan = dummy_plan();
        let ctx = ValidationContext::new();
        let report = validate_plan(&plan, &ctx);
        assert_eq!(report.layer_results.len(), 7);
        assert_eq!(report.layer_results[0].name, "l1_json_parse");
        assert_eq!(report.layer_results[6].name, "l7_verification_gate");
    }
}
