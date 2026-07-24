//! touring-cli — CLI daemon-side query handlers.
//!
//! Carved from `touring-dispatch` on 2026-06-10 (Wave C2, PoNR #4 —
//! `~/.claude/plans/daemon-lib-rearch/data/wave_runtime_cli_manifest.md`).
//!
//! Owns the daemon-side cli/ handler tree (55 files), the `cli_suggester`
//! (PreToolUse CLI suggestion engine) and `cli_e2e` (comprehensive code
//! analysis handler). The dispatch crate's hook_registry dispatch table calls
//! DOWNWARD into this crate; the dispatch crate re-exports every module at its
//! historical path (`touring_hooks::cli_handlers_*` etc.) so cross-crate
//! consumers (touring-server's 22 imports) are unchanged.

// Test-only lints: these fire only in #[cfg(test)] blocks and never reach production.
#![cfg_attr(test, allow(clippy::all))]
#![warn(rustdoc::broken_intra_doc_links)]
// Require docs on the public API surface (test-only pub items are exempt).
#![cfg_attr(not(test), deny(missing_docs))]
// RBP-01 elite-lint ratchet (2026-06-16): prod-unwrap-free (6 fixes: inferlets ×3
// runtime/spawn/join, health json-object, acp serde, cli_e2e guarded-regex →
// `.expect(..)`) + 7 `ctx_*` tantivy-disabled fallback stubs documented — lock
// against future bare unwrap in non-test code.
#![cfg_attr(not(test), deny(clippy::unwrap_used))]

// ── The carved modules (identical #[path] layout to the dispatch crate) ──────
pub mod cli;
pub mod cli_e2e;
#[path = "cli/handlers/dispatch.rs"]
pub mod cli_handlers;
#[path = "cli/handlers/decompose.rs"]
pub mod cli_handlers_decompose;
#[path = "cli/handlers/decompose_io.rs"]
pub mod cli_handlers_decompose_io;
#[path = "cli/handlers/decompose_workflow.rs"]
pub mod cli_handlers_decompose_workflow;
#[path = "cli/handlers/entity.rs"]
pub mod cli_handlers_entity;
#[path = "cli/handlers/file_knowledge.rs"]
pub mod cli_handlers_file_knowledge;
#[path = "cli/handlers/index.rs"]
pub mod cli_handlers_index;
#[path = "cli/handlers/mcp.rs"]
pub mod cli_handlers_mcp;
#[path = "cli/handlers/mutation_test.rs"]
pub mod cli_handlers_mutation_test;
#[path = "cli/handlers/semantics.rs"]
pub mod cli_handlers_semantics;
#[path = "cli/handlers/session.rs"]
pub mod cli_handlers_session;
#[path = "cli/handlers/wiring_repair.rs"]
pub mod cli_handlers_wiring_repair;
pub mod cli_suggester;
// CEG Pln2 P8 Workflow Intelligence Layer (stage/antipattern/advise) — moved
// with its single consumer (cli_suggester); its `crate::gateway` refs resolve
// via the touring-ceg mirror below.
pub mod workflow;

// ── Re-export mirrors so the moved modules' historical `crate::X` paths
//    resolve unchanged (same technique as the C/D/R carves) ──────────────────

// Runtime layer (touring-hook-runtime)
#[cfg(feature = "acp-protocol")]
pub use touring_hook_runtime::protocol;
pub use touring_hook_runtime::{
    gotcha_loader, hook_memory, hook_runtime, inferlets, prompt_enhance, runtime, schemas, wiring,
};

// Core engines (touring-hooks-core)
#[cfg(feature = "tantivy-fts")]
pub use touring_hooks_core::tantivy_index;
pub use touring_hooks_core::{
    approval_store, ast_bridge, compression_profiles, conformal, health_delta, health_delta_audit,
    knowledge, mutation_test, session_guide, throttle,
};

// Leaves
pub use touring_ceg::gateway;
pub use touring_ceg::gateway::sandbox_executor;
pub use touring_hooks_prediction::{ann_memory, semantic_classifier, tfidf_retriever};
pub use touring_hooks_shared::{
    action_signature, mcp_overhead, memory_finding, rfc100_emission, user_filters,
};
// The dispatch crate aliases the RL leaf as `agentic_rl` — mirror it here so
// `crate::agentic_rl::*` call sites in cli/predict.rs resolve identically.
pub use touring_hooks_rl as agentic_rl;

// `crate::shared::<submodule>` mirror — the dispatch crate's shared/ facade is
// a mix of leaf re-exports and runtime engines; mirror exactly the submodules
// the cli layer consumes (byte-identical paths).
/// Mirror of the dispatch crate's `shared/` facade, re-exporting the leaf and
/// runtime submodules the cli layer consumes at their byte-identical paths.
pub mod shared {
    pub use touring_hook_runtime::shared::{quality, reindex, signals};
    pub use touring_hooks_shared::{
        detect_language, feature_flags, gate_metrics, job_registry, memory_stats_probe,
        mpatch_preview, query_cache,
    };
}
