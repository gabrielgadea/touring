//! touring-cortex — Centralized hook execution engine (Cortex).
//!
//! Extracted from `touring-server` as an autonomous crate (S4).
//!
//! Architecture:
//! - `types.rs`: HookEvent, Decision, HandlerResult, CortexOutput
//! - `handler.rs`: Handler trait — the contract for all hook handlers
//! - `context.rs`: CortexContext — shared mutable state through the pipeline
//! - `pipeline.rs`: Pipeline — ordered execution with event/tool filtering
//! - `runtime.rs`: CortexRuntime — initialization and main entry point
//! - `cache_strategy.rs`: Prompt cache stratification
//! - `enrichment.rs`: Context enrichment pipeline
//! - `metrics.rs`: Pipeline observability
//! - `rl_mapping.rs`: RL state/action mapping (shared with touring-server)
//! - `handlers/`: 84+ builtin handlers organized by domain (H1–H84, H90–H97)
//! - `fusion.rs`: Reciprocal Rank Fusion (RRF) for combining ranked retrieval results
//! - `call_graph.rs`: Directed call graph analysis via petgraph (callees, callers, cycles, hotspots)

#![deny(missing_docs)]
// RBP-01 elite-lint ratchet (2026-06-16): prod-unwrap-free (8 fixes: 2 NaN-unsafe
// float sorts → unwrap_or(Equal), 2 mutex-lock → expect, 1 NonZeroUsize → MIN, 3
// e2e_audit asserts → expect) — lock against future bare unwrap in non-test code.
#![cfg_attr(not(test), deny(clippy::unwrap_used))]

pub mod cache_strategy;
pub mod call_graph;
pub mod circuit_breaker;
pub mod context;
pub mod cross_audit;
pub mod dspy;
pub mod enrichment;
pub mod fascicles;
pub mod fusion;
pub mod handler;
pub mod handlers;
pub mod metrics;
pub mod pipeline;
pub mod rl_mapping;
pub mod runtime;
pub mod scoring;
pub mod signal_fusion;
pub mod similarity;
pub mod types;

/// Legacy alias module re-exporting handler registration from [`handlers`].
///
/// Kept for backward compatibility with consumers that imported the builtin
/// handler registry under its former path.
pub mod builtin_handlers {
    pub use super::handlers::{BUILTIN_HANDLER_COUNT, register_all};
}

// Re-exports — public API surface for external crate consumers
/// Hook lifecycle events and routing decisions.
/// - `HookEvent`: variants for SessionStart, SessionEnd, PreToolUse, PostToolUse, etc.
/// - `Decision`: block/allow/skip verdict from handler evaluation
/// - `HandlerResult`: outcome with context line injection
/// - `CortexOutput`: full pipeline output envelope
/// - `HookSpecificOutput`: per-hook typed response container
pub use types::{CortexOutput, Decision, HandlerResult, HookEvent, HookSpecificOutput};

/// Core contract for all cortex handlers.
/// Implement this trait to create a custom handler.
pub use handler::Handler;

/// Execution context passed through the handler pipeline.
/// Accumulates state across handlers for context enrichment.
pub use context::CortexContext;

/// Cache stratification strategies for prompt context.
/// - `StableSessionContext`: session-persistent cached data
/// - `VolatilePromptContext`: per-prompt ephemeral data
/// - `compose_stratified_context`: combines both with staleness weighting
pub use cache_strategy::{StableSessionContext, VolatilePromptContext, compose_stratified_context};

/// Ordered handler execution with event filtering.
/// Owns the registered handlers and routes HookEvents to matching handlers.
pub use pipeline::Pipeline;

/// Context enrichment combining scored signals from multiple sources.
/// Returns a prioritized list of context lines within a token budget.
pub use enrichment::compose_enriched_context;

/// Cortex runtime initialization and entry point.
/// - `CortexRuntime`: manages handler registry and pipeline execution
/// - `KnowledgeRef`: shared reference to the knowledge DB (from touring-hooks)
pub use runtime::{CortexRuntime, KnowledgeRef};

/// Reciprocal Rank Fusion for combining ranked retrieval results.
/// - `reciprocal_rank_fusion`: generic RRF scorer
/// - `rrf`: alias for the k=60 default
/// - `rrf_strings`: string-keyed RRF for tool/ranking results
/// - `rrf_adaptive`: k parameter tuned by query complexity
pub use fusion::{reciprocal_rank_fusion, rrf, rrf_adaptive, rrf_strings};

/// Directed call graph via petgraph for cross-symbol analysis.
/// - `CallGraph`: adjacency-list graph of symbol call relationships
/// - `CallNode`: individual node with symbol metadata
/// - `CoEditTracker`: tracks files edited together for coupling signals
pub use call_graph::{CallGraph, CallNode, CoEditTracker};

/// SIMD-accelerated similarity search over symbol embeddings.
/// - `Embedding`: f32 vector type alias
/// - `EmbeddingIndex`: ANN index for fast k-NN retrieval
/// - `SearchResult`: scored match with metadata
/// - `batch_search`: parallel multi-query search
pub use similarity::{Embedding, EmbeddingIndex, SearchResult, batch_search};

/// DSPy automatic prompt compiler integration.
/// - `DspyCompiler`: program compilation from signatures
/// - `DspyModule`, `DspySignature`, `SignatureField`: compiler primitives
/// - `Teleprompter` variants: BootstrapFewShot, MCTSTeleprompter
/// - `CompiledPrompt`, `Demo`: compiled artifact types
/// - Signature helpers: `code_generation_sig`, `code_reflection_sig`, `test_generation_sig`
pub use dspy::{
    BootstrapFewShot, CompiledPrompt, Demo, DspyCompiler, DspyModule, DspySignature,
    MCTSTeleprompter, ModuleResult, SignatureField, Teleprompter, code_generation_sig,
    code_reflection_sig, test_generation_sig,
};

/// GraphDB — SQLite-backed graph store from touring-cognitive.
///
/// Re-exports `SqliteGraphStore` so cortex consumers can persist call-graph
/// and co-edit relationships to disk without depending directly on touring-cognitive.
pub use touring_intelligence::reasoning::SqliteGraphStore;
