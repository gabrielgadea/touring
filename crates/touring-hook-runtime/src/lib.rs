//! touring-hook-runtime — the HookRuntime substrate layer, carved from
//! `touring-dispatch` (Wave R+C PoNR #3, 2026-06-10).
//!
//! This crate owns the boundary object every hook/cli handler receives:
//!
//! - **HookRuntime** (`hook_runtime`) — the God-object, decomposed by domain
//!   into `runtime/{traits, impls_aco, impls_cognitive, impls_context,
//!   impls_hook, impls_knowledge, impls_rl, impls_symbols}`
//! - **Inferlets** — WASM sandbox service (`inferlets`, `inferlets_assets`)
//! - **Memory/wiring engines** — `hook_memory`, `wiring` (+hypergraph)
//! - **Protocol** — `daemon_protocol::ProjectCommand` (actor envelope),
//!   `schemas/` payload validation
//! - **Session engines** — `shared::{reindex, signals, session_context,
//!   quality}`, `auto_save_hook`, `triad_hook`, `gotcha_loader`
//!
//! `touring-dispatch` re-exports every module here at its historical path, so
//! `crate::hook_runtime::HookRuntime` / `touring_hooks::runtime::*` consumers
//! are unchanged. Sits between `touring-hooks-core` (engines) and
//! `touring-dispatch` (hooks + registry + daemon); `touring-cli` (next carve)
//! will depend on this layer without touching the dispatch.

#![cfg_attr(test, allow(clippy::all))]
#![warn(rustdoc::broken_intra_doc_links)]
#![deny(missing_docs)]
// RBP-01 elite-lint ratchet (2026-06-16): prod-unwrap-free (8 fixes: wiring.rs ×5
// Tarjan-SCC invariant unwraps + inferlets/hook_runtime ×3 mutex-lock → `.expect(..)`;
// signals.rs dead-code helpers cfg-gated to their tantivy-fts callers) — lock against
// future bare unwrap in non-test code.
#![cfg_attr(not(test), deny(clippy::unwrap_used))]

pub mod auto_save_hook;
// Wave H inversion (2026-06-10): 4 shared modules moved down from the dispatch
// crate — each had consumers on BOTH sides of the hooks/ carve (dispatch-rest
// AND the hook handlers): ceg_adapter (hook_registry + pre_bash), task_digest
// (mcts_materializer + instructions_loaded), suggesters+bidirectional
// (lifecycle/task_create + instructions_loaded).
pub mod bidirectional;
pub mod ceg_adapter;
pub mod ceg_impls;
pub mod daemon_protocol;
pub mod embeddings;
pub mod gotcha_loader;
pub mod hook_memory;
pub mod hook_runtime;
pub mod hook_runtime_ext;
pub mod inferlets;
#[cfg(feature = "inferlets-wasm")]
pub mod inferlets_assets;
// Wave C2 inversion (2026-06-10): ACP shim layer moved down from the dispatch
// crate so cli/acp.rs (touring-cli) and daemon.rs (dispatch) both reach it.
#[cfg(feature = "acp-protocol")]
pub mod protocol;
// Wave C2 inversion (2026-06-10): prompt enhancement engine moved down from
// the dispatch crate (was hooks/prompt_enhance.rs) — consumed by both the
// dispatch hook_registry (UserPromptSubmit) and cli/health.rs (touring-cli).
pub mod prompt_enhance;
pub mod runtime;
pub mod schemas;
pub mod shared;
pub mod suggesters;
pub mod task_digest;
pub mod triad_hook;
pub mod wiring;

// ── Re-exports so the moved modules' historical `crate::X` paths resolve ──
pub use classifier::{
    CILALevel, CILAResult, CachedIntentClassifier, CognitiveTechnique, IntentClassifier,
};
pub use pii::{PIIFinding, PIIScanner, PIIType};
pub use runtime::{HookResponse, HookRuntime, HookTimer, make_relative};
pub use touring_ceg::capability; // Wave H: ceg_adapter consumes crate::capability
pub use touring_ceg::gateway;
pub use touring_hooks_core::aco_bridge;
pub use touring_hooks_core::aco_processor;
pub use touring_hooks_core::aco_wiring;
pub use touring_hooks_core::ast_bridge;
pub use touring_hooks_core::async_knowledge;
pub use touring_hooks_core::branch_fs;
pub use touring_hooks_core::circuit_breaker;
pub use touring_hooks_core::cortex_dispatcher;
pub use touring_hooks_core::cross_agent_ledger;
pub use touring_hooks_core::dependency_cache;
pub use touring_hooks_core::ecosystem;
pub use touring_hooks_core::error_predictor;
pub use touring_hooks_core::health_delta;
pub use touring_hooks_core::hook_response;
pub use touring_hooks_core::knowledge;
pub use touring_hooks_core::knowledge::{
    BashOutcome, EditEvent, FileKnowledge, FileKnowledgeDB, FileKnowledgeEnriched, FileRelation,
    Gotcha, KnowledgeStats, ThreadSafeKnowledgeDB, WeightedErrorPattern,
};
pub use touring_hooks_core::knowledge_symbol_bridge;
pub use touring_hooks_core::knowledge_wiring;
pub use touring_hooks_core::pre_tool_validator;
pub use touring_hooks_core::symbol_extractors;
#[cfg(feature = "tantivy-fts")]
pub use touring_hooks_core::tantivy_index;
pub use touring_hooks_prediction::ann_memory;
pub use touring_hooks_prediction::classifier;
pub use touring_hooks_prediction::layer7_prediction;
pub use touring_hooks_prediction::pii;
pub use touring_hooks_rl as agentic_rl;
pub use touring_hooks_saga as saga;
pub use touring_hooks_shared::errors;
pub use touring_hooks_shared::errors::{ErrorContext, Result, TouringError};
pub use touring_hooks_shared::metrics;
pub use touring_hooks_shared::n1_bridge;
pub use touring_hooks_shared::pattern_bandit;
pub use touring_hooks_shared::precomputed_signals;
