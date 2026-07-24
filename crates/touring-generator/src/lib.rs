//! touring-generator — LLM-as-Planner / Touring-as-Generator
//!
//! Multi-kind artifact generator with VGP verification, typestate lifecycle,
//! 28 generator kinds, and full Touring workspace integration.
//!
//! # Architecture
//!
//! ```text
//! GeneratorPlan (JSON)
//!     ↓ submit
//! PlanExecutor<Draft>
//!     ↓ verify()     → VgpEngine: batch symbol lookup (moka + rayon)
//! PlanExecutor<Verified>
//!     ↓ render()     → TemplateEngine: OnceLock<Tera> pre-compiled
//! PlanExecutor<Rendered>
//!     ↓ speculate()  → SpeculateBridge: touring shadow validate
//! PlanExecutor<Speculated>
//!     ↓ commit()     → atomic write + memory store + RL reward
//! PlanExecutor<Committed>
//! ```

// Allow unwrap/expect in test code — the deny applies only to production paths.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
// D.W2.P3 (2026-06-06): rustdoc hygiene gate — surfaces broken intra-doc links
// (warn, not deny, to avoid breaking doc builds on legacy items).
#![warn(rustdoc::broken_intra_doc_links)]
// D.W2.P3.T6 (2026-06-13): all 340 previously-undocumented public items are now
// documented (`cargo rustc -p touring-generator --lib -- -W missing_docs` = 0), so
// the lint is enforced in-source at parity with touring-hooks (which already denies).
#![deny(missing_docs)]
// RBP-01 elite-lint ratchet (2026-06-16): prod-unwrap-free (crate doc invariant
// "No unwrap() in production") — lock it in (`.expect("…")` stays the escape).
#![cfg_attr(not(test), deny(clippy::unwrap_used))]

pub mod core;
pub mod error;
pub mod executor;
pub mod generator;
pub mod plan;
pub mod registry;
pub mod shape;
pub mod skip;
pub mod source_change;
pub mod speculate;
pub mod template;
pub mod validate;
pub mod vgp;

// Source change re-exports (B.5)
pub use source_change::{
    Applier, ApplyError, ApplyResult, FileId, FileSystemEdit, Indel, NonOverlapError, SnippetEdit,
    SourceChange, TabStop, TextEdit,
};

// Top-level re-exports for ergonomic use.
pub use core::score::NormalizedScore;
pub use error::{GenerateError, Result};
pub use executor::replan::{CompletedPlan, RejectedPlan, ReplanRequest};
pub use executor::typestate::{Committed, Draft, PlanExecutor, Rendered, Speculated, Verified};
pub use generator::kinds::GeneratorKind;
pub use plan::contracts::{Contracts, SymbolRef};
pub use plan::failure::FailureReport;
pub use plan::result::{
    Artifact, CommitReport, ExecutionStatus, FileAction, GenerateResult, RenderedFile,
    SpeculateReport, ValidationReport, VgpReport,
};
pub use plan::schema::GeneratorPlan;
pub use registry::plan_registry::{PlanExecutorHandle, PlanRegistry, SharedPlanRegistry};
pub use shape::RenderShape;

// Context and provider types.
pub use core::context::{
    AuditLog,
    CognitiveNexusFn,
    DecomposeCreateFn,
    DecomposeUpdateFn,
    DspyInputs,
    DspyOutputs,
    DspySigFn,
    DspySignatureName,
    // PLN2 section 8.1 — new types
    FuzzyMatcher,
    FuzzySuggestion,
    GeneratorContext,
    KnowledgeUpsertFn,
    LlmError,
    LlmProvider,
    MctsEvalFn,
    MemoryEntry,
    MemoryError,
    MemoryKind,
    MemoryProvider,
    MemoryStats,
    MemoryTier,
    NoopAuditLog,
    NoopFuzzyMatcher,
    NoopLlm,
    NoopMemory,
    NoopRlSink,
    NoopTelemetry,
    PheromoneUpdateFn,
    PlanSimilarityScore,
    RlRewardSink,
    SchemaRegistry,
    SemanticGraphFn,
    SessionAssessFn,
    SessionCheckpointFn,
    // P2+P3: session lifecycle + decompose bridge
    SessionStartFn,
    TelemetrySink,
    WasmSandboxFn,
    WiringGateFn,
};
// PLN2 production adapters — feature-gated, separate pub use required for #[cfg]
pub use core::capacity::CapacityLimits;
#[cfg(feature = "analysis-gate")]
pub use core::context::AnalysisGateAdapter;
#[cfg(feature = "simd-fuzzy")]
pub use core::context::BkTreeFuzzyAdapter;
#[cfg(feature = "analysis-gate")]
pub use core::context::CompositeWiringGate;
#[cfg(feature = "rl-integration")]
pub use core::context::LinUCBRewardSink;
#[cfg(feature = "mcts-synthesis")]
pub use core::context::McctsEvalAdapter;
#[cfg(feature = "nlp-reranking")]
pub use core::context::NlpPlanRankerAdapter;
#[cfg(feature = "zero-copy")]
pub use core::context::RkyvFileSnapshotAdapter;
#[cfg(feature = "cognitive-nexus")]
pub use core::context::SemanticGraphAdapter;
pub use core::context::SynWiringGateAdapter;
#[cfg(feature = "memory-integration")]
pub use core::context::TouringMemoryProvider;
#[cfg(feature = "observability")]
pub use core::context::TracingAuditLog;
#[cfg(feature = "observability")]
pub use core::context::TracingTelemetrySink;
#[cfg(feature = "analysis-gate")]
pub use core::context::WIRING_GATE_BYPASSED_COUNT;
#[cfg(feature = "wasm-sandbox")]
pub use core::context::WasmSandboxAdapter;
#[cfg(feature = "analysis-gate")]
pub use core::context::WiringGateError;

// Engine types.
pub use speculate::bridge::SpeculateBridge;
pub use template::engine::TemplateEngine;
pub use vgp::engine::{VgpCacheStats, VgpEngine};

// Generator trait.
pub use generator::trait_def::{DynGenerator, ErasedGenerator, Generator};
