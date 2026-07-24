//! Plan domain: schema, contracts, failure, and result types.

pub mod contracts;
pub mod failure;
pub mod result;
pub mod schema;

pub use contracts::{Contracts, DefinitionHint, SymbolRef, WiringRequirements};
pub use failure::{
    CodeExcerpt, FailureReason, FailureReport, FuzzySuggestion, IoErrorEntry, MissingSymbol,
    NextAction, SuggestionSource,
};
pub use result::{
    Artifact, AuditEntry, CheckResult, CommitReport, ExecutionStatus, FileAction, GenerateResult,
    InvariantCheckResult, LayerResult, RenderedFile, SpeculateReport, TemplateError, TokenUsage,
    ValidationReport, VgpReport,
};
pub use schema::{
    Assembly, CapacityHints, CilaLevel, CommitPolicy, CrateDep, GeneratorPlan, Invariant,
    InvariantEnforcement, LearningDirectives, PlanMetadata, ReverseMode, RollbackPolicy,
    SpecInputs, Target, TemplateSelection, TraceEntry, ValidationDirectives,
};
// PlanPriority lives in core::capacity; re-export here for plan-domain consumers.
pub use crate::core::capacity::PlanPriority;
