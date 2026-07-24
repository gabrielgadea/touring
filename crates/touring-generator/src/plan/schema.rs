//! `GeneratorPlan` v2.0 — versioned, migration-safe, cross-platform schema.

use crate::core::capacity::PlanPriority;
use crate::generator::kinds::GeneratorKind;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use uuid::Uuid;

/// Top-level plan submitted to the generator pipeline.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GeneratorPlan {
    /// Schema version for migration safety (semver string).
    pub version: String,
    /// Unique plan identifier.
    pub plan_id: Uuid,
    /// Natural language or structured intent.
    #[schemars(length(min = 1, max = 4096))]
    pub intent: String,
    /// CILA complexity level for budget allocation.
    pub cila_level: CilaLevel,
    /// Target file / module for generation.
    pub target: Target,
    /// Kind of artifact to generate.
    pub kind: GeneratorKind,
    /// Symbol contracts (`must_exist` / `must_not_exist`).
    pub contracts: super::contracts::Contracts,
    /// Template selection override (optional).
    pub template: TemplateSelection,
    /// Assembly directives.
    pub assembly: Assembly,
    /// Validation configuration.
    pub validation: ValidationDirectives,
    /// Commit policy.
    pub commit_policy: CommitPolicy,
    /// Rollback policy.
    pub rollback: RollbackPolicy,
    /// RL learning directives.
    pub learning: LearningDirectives,
    /// Spec-driven inputs (alternative to natural language).
    pub spec_inputs: Option<SpecInputs>,
    /// Capacity hints from the LLM planner.
    pub capacity_hints: CapacityHints,
    /// Execution trace populated by the engine during lifecycle.
    #[serde(default)]
    pub execution_trace: Vec<TraceEntry>,
    /// Plan metadata.
    pub metadata: PlanMetadata,
}

impl GeneratorPlan {
    /// Returns the plan version string.
    #[must_use]
    pub fn version(&self) -> &str {
        &self.version
    }
}

/// CILA complexity level — controls token budget and timeout.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
pub enum CilaLevel {
    /// Trivial complexity — minimal budget, single-step generation.
    L0,
    /// Low complexity — small artifact, low token budget.
    L1,
    /// Medium-low complexity — moderate token budget.
    L2,
    /// Medium complexity — moderate token budget.
    L3,
    /// High complexity — large token budget.
    L4,
    /// Maximum complexity — largest token budget and timeout.
    L5,
}

impl CilaLevel {
    /// Character budget for this CILA level.
    #[must_use]
    pub fn char_budget(self) -> usize {
        match self {
            Self::L0 | Self::L1 => 1_200,
            Self::L2 | Self::L3 => 3_000,
            Self::L4 | Self::L5 => 6_000,
        }
    }
}

/// Target output location.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Target {
    /// Output file path (UTF-8 string, relative to project root).
    pub file_path: String,
    /// Optional parent module path (e.g. `crate::plan`).
    pub module_path: Option<String>,

    /// Crate name for context.
    pub crate_name: Option<String>,
}

/// Template selection — allows overriding the default template for a kind.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct TemplateSelection {
    /// Explicit template ID (overrides `GeneratorKind::template_name()`).
    pub template_id: Option<String>,
    /// Additional variables injected into the template context.
    #[serde(default)]
    pub extra_vars: BTreeMap<String, serde_json::Value>,
}

/// Assembly directives — controls how generated files are assembled.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Assembly {
    /// If true, append to existing file rather than replace.
    pub append_mode: bool,
    /// If true, emit only a unified diff (`IncrementalPatch` mode).
    pub diff_only: bool,
    /// If true, do not write to disk (dry run).
    pub dry_run: bool,
    /// Encoding for output files.
    pub encoding: String,
}

impl Default for Assembly {
    fn default() -> Self {
        Self {
            append_mode: false,
            diff_only: false,
            dry_run: false,
            encoding: "utf-8".to_owned(),
        }
    }
}

/// Validation directives — controls which checks run post-generation.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ValidationDirectives {
    /// Run speculative validation before commit.
    pub run_speculate: bool,
    /// Run wiring audit after commit.
    pub run_wiring_audit: bool,
    /// Minimum speculate score to proceed.
    pub min_speculate_score: f64,
    /// Invariants to enforce (referenced by ID).
    #[serde(default)]
    pub invariant_ids: Vec<String>,
}

impl Default for ValidationDirectives {
    fn default() -> Self {
        Self {
            run_speculate: true,
            run_wiring_audit: true,
            min_speculate_score: 0.8,
            invariant_ids: Vec::new(),
        }
    }
}

/// Commit policy — controls when and how files are written to disk.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CommitPolicy {
    /// Auto-commit if speculate score meets threshold.
    pub auto_commit: bool,
    /// Require human review before commit.
    pub require_human_review: bool,
    /// Minimum speculate score for auto-commit.
    pub auto_commit_threshold: f64,
    /// Write atomically via temp file + rename.
    pub atomic_write: bool,
}

impl Default for CommitPolicy {
    fn default() -> Self {
        Self {
            auto_commit: true,
            require_human_review: false,
            auto_commit_threshold: 0.8,
            atomic_write: true,
        }
    }
}

/// Rollback policy — controls backup and restore behavior.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RollbackPolicy {
    /// Create a backup before overwriting existing files.
    pub create_backup: bool,
    /// Backup directory (relative to project root).
    pub backup_dir: Option<String>,
    /// Auto-rollback on test failure after commit.
    pub auto_rollback_on_test_failure: bool,
}

impl Default for RollbackPolicy {
    fn default() -> Self {
        Self {
            create_backup: true,
            backup_dir: None,
            auto_rollback_on_test_failure: true,
        }
    }
}

/// RL learning directives — controls reward injection after execution.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct LearningDirectives {
    /// Inject RL reward after successful commit.
    pub inject_rl_reward: bool,
    /// Store plan as memory entry after success.
    pub store_memory: bool,
    /// Memory key prefix (plan ID appended).
    pub memory_key_prefix: String,
}

impl Default for LearningDirectives {
    fn default() -> Self {
        Self {
            inject_rl_reward: true,
            store_memory: true,
            memory_key_prefix: "plan:generator:".to_owned(),
        }
    }
}

/// Spec-driven inputs — structured alternatives to natural language intent.
/// Paths are UTF-8 strings (relative to project root); use `camino::Utf8PathBuf::new` at runtime.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub enum SpecInputs {
    /// Generate from an `OpenAPI` specification document.
    OpenApi {
        /// Path to the `OpenAPI` spec file (relative to project root).
        spec_path: String,
    },
    /// Generate from a Protocol Buffers definition.
    Protobuf {
        /// Path to the `.proto` definition file (relative to project root).
        proto_path: String,
    },
    /// Generate from a JSON Schema document.
    JsonSchema {
        /// Path to the JSON Schema file (relative to project root).
        schema_path: String,
    },
    /// Generate from a GraphQL schema document.
    GraphQl {
        /// Path to the GraphQL schema file (relative to project root).
        schema_path: String,
    },
    /// Generate from an `AsyncAPI` specification document.
    AsyncApi {
        /// Path to the `AsyncAPI` spec file (relative to project root).
        spec_path: String,
    },
    /// Reverse-engineer a plan from an existing source file.
    ExistingFile {
        /// Path to the existing file to reverse-engineer (relative to project root).
        file_path: String,
        /// Reverse-engineering mode controlling what is inferred.
        mode: ReverseMode,
    },
}

/// Mode for reverse-engineering an existing file into a plan.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub enum ReverseMode {
    /// Infer a full `GeneratorPlan` from the existing file.
    InferPlan,
    /// Infer a reusable template from the existing file.
    InferTemplate,
    /// Infer a migration that transforms the existing file.
    InferMigration,
}

/// Capacity hints provided by the LLM planner.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CapacityHints {
    /// Estimated total bytes of generated output.
    pub estimated_output_bytes: u32,
    /// Estimated number of files the plan will produce.
    pub estimated_files: u16,
    /// Estimated number of symbols the VGP layer must verify.
    pub estimated_symbols_to_verify: u16,
    /// Estimated LLM token consumption for the plan.
    pub estimated_llm_tokens: u32,
    /// Scheduling priority of the plan within the generator queue.
    pub priority: PlanPriority,
}

impl Default for CapacityHints {
    fn default() -> Self {
        Self {
            estimated_output_bytes: 4_096,
            estimated_files: 1,
            estimated_symbols_to_verify: 8,
            estimated_llm_tokens: 2_048,
            priority: PlanPriority::Normal,
        }
    }
}

/// Single entry in the execution trace.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TraceEntry {
    /// UTC timestamp when the stage was recorded.
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// Stage name matching `PlanStage` type names.
    pub stage: String,
    /// Wall-clock milliseconds the stage took to execute.
    pub elapsed_ms: u64,
    /// Optional free-form note attached to the trace entry.
    pub note: Option<String>,
}

/// Plan metadata — provenance and tagging.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PlanMetadata {
    /// UTC timestamp when the plan was created.
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Author or originating system of the plan.
    pub author: String,
    /// Free-form tags for categorizing and querying the plan.
    pub tags: Vec<String>,
    /// Optional TACO session ID that produced this plan.
    pub taco_session_id: Option<String>,
    /// Optional ID of the parent plan this one was derived from.
    pub parent_plan_id: Option<Uuid>,
}

impl Default for PlanMetadata {
    fn default() -> Self {
        Self {
            created_at: chrono::Utc::now(),
            author: "touring-generator".to_owned(),
            tags: Vec::new(),
            taco_session_id: None,
            parent_plan_id: None,
        }
    }
}

/// Crate dependency reference in a plan contract.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CrateDep {
    /// Crate name as it appears in `Cargo.toml`.
    pub name: String,
    /// Optional semver version requirement string.
    pub version_req: Option<String>,
    /// Cargo features to enable for this dependency.
    pub features: Vec<String>,
}

/// An invariant contract (structural rule to enforce).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Invariant {
    /// Stable identifier referenced by `ValidationDirectives::invariant_ids`.
    pub id: String,
    /// Human-readable description of the structural rule.
    pub description: String,
    /// How the invariant is checked at validation time.
    pub enforcement: InvariantEnforcement,
}

/// How an invariant is checked.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub enum InvariantEnforcement {
    /// Check via `touring wiring orphans`.
    WiringAudit,
    /// Check via regex pattern match on generated content.
    RegexMatch {
        /// Regex pattern the generated content must match.
        pattern: String,
    },
    /// Check via speculate validation layer.
    SpeculateLayer {
        /// Name of the speculate validation layer to invoke.
        layer_name: String,
    },
    /// Custom check (description only, human-verified).
    Manual {
        /// Human-readable description of the manual check to perform.
        description: String,
    },
}
