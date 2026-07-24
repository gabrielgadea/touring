//! touring-hook-handlers — pre/post lifecycle hook handlers.
//!
//! Carved from `touring-dispatch` on 2026-06-10 (Wave H, PoNR #5). Owns the
//! 25-file hooks/ tree (pre_*/post_*/session/instructions_loaded/...) plus its
//! two single-consumer companions (`hook_decompose_bridge`, `mcts_materializer`)
//! and the two hook-only shared engines (`shared::{metadata_collector,
//! signal_pipeline}`). The dispatch crate's hook_registry calls DOWNWARD into
//! this crate and re-exports every module at its historical path.

// Test-only lints: these fire only in #[cfg(test)] blocks and never reach production.
#![cfg_attr(test, allow(clippy::all))]
#![warn(rustdoc::broken_intra_doc_links)]
// Enforce documentation on the production public-API surface only; test fixtures
// (incl. raw-string source samples) under #[cfg(test)] are exempt.
#![cfg_attr(not(test), deny(missing_docs))]
// RBP-01 elite-lint ratchet (2026-06-16): prod-unwrap-free (team_hooks double-unwrap
// replaced by safe-bind) — lock against future bare unwrap in non-test code
// (`.expect("…")` stays the sanctioned escape).
#![cfg_attr(not(test), deny(clippy::unwrap_used))]

// ── The hook handler modules (identical #[path] layout + cfg gates to the
//    dispatch crate) ──────────────────────────────────────────────────────────
#[path = "hooks/hooks_task_lifecycle.rs"]
pub mod hooks_task_lifecycle;
#[path = "hooks/instructions_loaded.rs"]
pub mod instructions_loaded;
#[path = "hooks/permission_request.rs"]
pub mod permission_request;
#[cfg(feature = "post-hooks")]
#[path = "hooks/post_bash.rs"]
pub mod post_bash;
#[path = "hooks/post_compact_handler.rs"]
pub mod post_compact_handler;
#[cfg(feature = "post-hooks")]
#[path = "hooks/post_edit.rs"]
pub mod post_edit;
#[cfg(feature = "post-hooks")]
#[path = "hooks/post_edit_rule_engine.rs"]
pub mod post_edit_rule_engine;
#[cfg(feature = "post-hooks")]
#[path = "hooks/post_read.rs"]
pub mod post_read;
#[cfg(feature = "post-hooks")]
#[path = "hooks/post_tool_batch.rs"]
pub mod post_tool_batch;
#[cfg(feature = "post-hooks")]
#[path = "hooks/post_tool_failure.rs"]
pub mod post_tool_failure;
#[cfg(feature = "post-hooks")]
#[path = "hooks/post_tool_rl.rs"]
pub mod post_tool_rl;
#[cfg(feature = "post-hooks")]
#[path = "hooks/post_tool_use.rs"]
pub mod post_tool_use;
#[cfg(feature = "post-hooks")]
#[path = "hooks/post_write.rs"]
pub mod post_write;
#[cfg(feature = "pre-hooks")]
#[path = "hooks/pre_bash.rs"]
pub mod pre_bash;
#[cfg(feature = "pre-hooks")]
#[path = "hooks/pre_edit.rs"]
pub mod pre_edit;
#[cfg(feature = "pre-hooks")]
#[path = "hooks/pre_edit_prevention.rs"]
pub mod pre_edit_prevention;
#[cfg(feature = "pre-hooks")]
#[path = "hooks/pre_glob.rs"]
pub mod pre_glob;
#[cfg(feature = "pre-hooks")]
#[path = "hooks/pre_grep.rs"]
pub mod pre_grep;
#[cfg(feature = "pre-hooks")]
#[path = "hooks/pre_read.rs"]
pub mod pre_read;
#[cfg(feature = "pre-hooks")]
#[path = "hooks/pre_tool_use.rs"]
pub mod pre_tool_use;
#[cfg(feature = "pre-hooks")]
#[path = "hooks/pre_write.rs"]
pub mod pre_write;
#[cfg(feature = "session-hooks")]
#[path = "hooks/session_hooks.rs"]
pub mod session_hooks;
#[path = "hooks/stop.rs"]
pub mod stop;
#[path = "hooks/team_hooks.rs"]
pub mod team_hooks;

// Integration tests — post_tool_rl RL feedback loop
#[cfg(all(test, feature = "post-hooks"))]
#[path = "hooks/post_tool_rl_tests.rs"]
mod post_tool_rl_tests;

// ── Single-consumer companions (their only callers are hook handlers) ────────
pub mod hook_decompose_bridge;
pub mod mcts_materializer;

// ── Re-export mirrors so the moved modules' historical `crate::X` paths
//    resolve unchanged (same technique as the C/D/R/C2 carves) ────────────────

// Runtime layer (touring-hook-runtime)
pub use touring_hook_runtime::{HookResponse, HookRuntime};
pub use touring_hook_runtime::{
    bidirectional, ceg_adapter, gotcha_loader, hook_memory, hook_runtime, prompt_enhance, runtime,
    schemas, suggesters, task_digest, triad_hook, wiring,
};

// Core engines (touring-hooks-core)
#[cfg(feature = "nlp-enrichment")]
pub use touring_hooks_core::nlp_bridge;
#[cfg(feature = "session-hooks")]
pub use touring_hooks_core::session_insights;
#[cfg(feature = "tantivy-fts")]
pub use touring_hooks_core::tantivy_index;
pub use touring_hooks_core::{
    aco_bridge, activity_hook, approval_store, ast_bridge, callgraph_enrichment,
    cross_agent_ledger, ecosystem, error_predictor, functional_wiring, health_delta, knowledge,
    output_capture, pipeline, symbol_extractors, tool_output_router, wave5_workflow,
};

// Leaves
pub use touring_ceg::gateway;
pub use touring_hooks_prediction::{ann_memory, layer7_prediction, llm_judge, pii};
pub use touring_hooks_rl as agentic_rl;
pub use touring_hooks_shared::{
    action_signature, got_snapshot_store, idempotency, mcp_overhead, n1_bridge, precomputed_signals,
};

// CLI layer (touring-cli) — hooks_task_lifecycle + hook_decompose_bridge +
// mcts_materializer call cli handlers downward.
pub use touring_cli::{cli_handlers, cli_handlers_decompose};

// `crate::shared::<submodule>` facade — the two hook-only engines live here;
// the rest mirror the leaf/runtime homes (byte-identical paths).
/// Facade aggregating the hook-only shared engines (`metadata_collector`,
/// `signal_pipeline`) with re-exports of the runtime/leaf shared homes, so the
/// historical `crate::shared::<submodule>` paths resolve byte-identically.
pub mod shared {
    pub mod hook_helpers;
    pub mod metadata_collector;
    pub mod signal_pipeline;
    #[cfg(feature = "tantivy-fts")]
    pub use touring_hook_runtime::shared::tantivy_stream;
    pub use touring_hook_runtime::shared::{quality, reindex, session_context, signals};
    pub use touring_hooks_shared::{
        antipatterns, api_cascade_bridge, ast_grep_signal, bash_ast_validator, cila, command_hash,
        detect_language, feature_flags, gate_metrics, job_registry, metadata_dedup, mpatch_preview,
        parser_cache_global, patterns, query_cache, result_ext, risk_patterns, span_context,
        thread_pool,
    };
}
