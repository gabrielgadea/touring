//! touring-hooks-core — the data/intelligence engine layer of the Touring
//! Neural Hooks system, carved from the `touring-hooks` monolith
//! (daemon-lib-rearch Phase C, 2026-06-10).
//!
//! This crate owns the engines that hold state and intelligence but have
//! ZERO coupling to the daemon dispatch layer (`HookRuntime`, `cli/`,
//! `hook_registry`):
//!
//! - **Knowledge** — SQLite WAL file graph (`knowledge`, `async_knowledge`,
//!   `knowledge_symbol_bridge`)
//! - **Search** — Tantivy FTS index (`tantivy_index`, feature `tantivy-fts`)
//! - **Quality** — health delta + audit trail (`health_delta`,
//!   `health_delta_audit`, `mutation_test`, `conformal`)
//! - **Bridges** — AST / ACO / cognitive / NLP engine integrations
//! - **Safety** — circuit breaker + state machine, BranchFs snapshots,
//!   panic forensics
//! - **Session** — session bus, session guide, session insights
//!
//! The `touring-hooks` façade re-exports every module here at its historical
//! path (`touring_hooks::knowledge`, `crate::tantivy_index`, …) so no
//! consumer is affected by the carve.

// Test-only lints: mirror the parent façade so the moved test modules keep
// compiling identically (they fire only in #[cfg(test)] blocks).
#![cfg_attr(test, allow(clippy::all))]
#![warn(rustdoc::broken_intra_doc_links)]
// Require docs on the public API surface (skipped under cfg(test) so the moved
// test modules' pub helpers don't need doc comments).
#![cfg_attr(not(test), deny(missing_docs))]
// RBP-01 elite-lint ratchet (2026-06-16): prod-unwrap-free (4 fixes: tantivy_index.rs
// ×3 `TOOL_OUTPUTS_GLOBAL.lock().unwrap()` + guarded `guard.as_ref().unwrap()`,
// hook_response.rs `cache.get().unwrap()` after `set()` → `.expect(..)`). The
// W72-orphaned `knowledge*.rs` files on disk are NOT in the module tree
// (`pub use touring_storage::knowledge`) so they don't compile. Lock against future
// bare unwrap in non-test code.
#![cfg_attr(not(test), deny(clippy::unwrap_used))]

// ── Engines (root modules, alphabetical) ────────────────────────────────
pub mod aco_processor;
pub mod aco_wiring;
pub mod approval_store;
pub mod async_knowledge;
#[cfg(feature = "utilities")]
pub mod audit;
pub mod branch_fs;
pub mod callgraph_enrichment;
#[cfg(feature = "capnp-server")]
pub mod capnp_embed;
pub mod circuit_breaker;
pub mod circuit_state_machine;
pub mod compression_profiles;
pub mod conformal;
pub mod cortex_dispatcher;
pub mod cross_agent_ledger;
pub mod dependency_cache;
pub mod ecosystem;
pub mod error_predictor;
pub use touring_storage::functional_wiring;
pub mod generator_hints;
pub mod health_delta;
pub mod health_delta_audit;
pub mod hook_response;
pub mod inventory_registry;
pub mod ipc;
pub use touring_storage::knowledge;
pub use touring_storage::knowledge_wiring;
pub mod mutation_test;
pub mod output_capture;
pub mod panic_log;
pub mod pipeline;
pub mod proc_identity;
#[cfg(feature = "tantivy-fts")]
pub mod sandbox_output_store;
#[cfg(feature = "shadow-workspace")]
pub mod shadow_v2;
pub mod symbol_extractors;
#[cfg(feature = "tantivy-fts")]
pub mod tantivy_index;
pub mod throttle;
pub mod tool_output_router;
pub mod wave3_extended;
pub mod wave5_workflow;
pub mod workflow_templates;

// ── Bridges (engine integrations) ───────────────────────────────────────
#[path = "bridges/aco_bridge.rs"]
pub mod aco_bridge;
#[path = "bridges/ast_bridge.rs"]
pub mod ast_bridge;
#[path = "bridges/cognitive_bridge.rs"]
pub mod cognitive_bridge;
#[path = "bridges/knowledge_symbol_bridge.rs"]
pub mod knowledge_symbol_bridge;
#[cfg(feature = "nlp-enrichment")]
#[path = "bridges/nlp_bridge.rs"]
pub mod nlp_bridge;

// ── Hook helpers with zero HookRuntime coupling ─────────────────────────
#[path = "hooks/activity_hook.rs"]
pub mod activity_hook;
#[path = "hooks/pre_tool_validator.rs"]
pub mod pre_tool_validator;
#[path = "hooks/session_guide.rs"]
pub mod session_guide;
#[cfg(feature = "session-hooks")]
#[path = "hooks/session_insights.rs"]
pub mod session_insights;

// ── Shared facade (7 leaf re-exports + session_bus) ─────────────────────
pub mod shared;

// ── Leaf re-exports so the moved modules' `crate::X` paths resolve ──────
pub use touring_hooks_prediction::ann_memory;
pub use touring_hooks_shared::action_signature;
pub use touring_hooks_shared::errors;
// Root-level error re-exports mirror the parent façade (`crate::TouringError`
// is the historical import path used by knowledge.rs et al.).
pub use touring_hooks_shared::errors::{ErrorContext, Result, TouringError};
pub use touring_hooks_shared::user_filters;

// Root-level knowledge re-exports mirror the parent façade
// (`crate::BashOutcome` / `crate::FileKnowledgeDB` are the historical import
// paths used by session_insights et al.).
pub use knowledge::{
    BashOutcome, EditEvent, FileKnowledge, FileKnowledgeDB, FileKnowledgeEnriched, FileRelation,
    Gotcha, KnowledgeStats, ThreadSafeKnowledgeDB, WeightedErrorPattern,
};

// CEG re-exports: tool_output_router reaches the gateway + sandbox executor
// through `crate::{gateway,sandbox_executor}` exactly like in the parent.
pub use touring_ceg::gateway;
pub use touring_ceg::gateway::sandbox_executor;

// ── FFI: current Unix user ID ───────────────────────────────────────────
// Mirrors the touring-hooks façade copy (13-line libc wrapper): both crates
// call `crate::current_uid()`; a shared home in the leaf would force an FFI
// block there for a single getter.
/// Returns the current Unix user ID (0 on non-Unix platforms).
pub(crate) fn current_uid() -> u32 {
    #[cfg(unix)]
    {
        unsafe extern "C" {
            fn getuid() -> u32;
        }
        unsafe { getuid() }
    }
    #[cfg(not(unix))]
    {
        0
    }
}
