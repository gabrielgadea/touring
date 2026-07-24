//! touring-hooks-shared — leaf utilities extracted from touring-hooks (W8 pragmatic split).
//!
//! Internal workspace crate. Cycle-free LEAF: depends on no other touring-hooks-* crate.
//! Re-exported verbatim by the `touring-hooks` facade so the external API is unchanged.

#![deny(missing_docs)]
// RBP-01 elite-lint ratchet (2026-06-16): prod-unwrap-free leaf — lock against
// future bare unwrap in non-test code (`.expect("…")` stays the sanctioned escape).
#![cfg_attr(not(test), deny(clippy::unwrap_used))]

// S-13 (2026-06-06) — ActionSignature relocated here from touring-hooks (leaf-safe:
// zero crate:: deps). Re-exported by the touring-hooks facade as
// `crate::action_signature` so every call site + the public API stay unchanged.
pub mod action_signature;
pub mod errors;
pub mod got_snapshot_store;
pub mod idempotency;
// S-13 (2026-06-06) — IsolationMode policy enum relocated from touring-hooks
// hook_runtime (leaf-safe, std-only). Re-exported by hook_runtime for compat.
pub mod isolation_mode;
pub mod mcp_overhead;
pub mod memory_finding;
pub mod metrics;
pub mod n1_bridge;
pub mod pattern_bandit;
pub mod plugin;
pub mod precomputed_signals;
pub mod qa_syntax;
pub mod query_dsl;
pub mod reranked_context;
pub mod rfc100_emission;
// S-13 (2026-06-06) — StaticSeverity relocated here (shared severity vocabulary
// for the gateway X2 stage + workflow antipattern). Re-exported by gateway::static_stage.
pub mod severity;
pub mod user_filters;
// S-13 (2026-06-06) — Workflow Intelligence core (baseline/stage/antipattern)
// relocated here from touring-hooks::workflow (leaf-safe after StaticSeverity moved
// to crate::severity). Re-exported by touring-hooks::workflow for advise/convert/cli_suggester.
pub mod workflow;
// Session B F4-pre (2026-06-10): 6 submodules relocated from touring-hooks
// `src/shared/` + the signal-layer vocabulary extracted from `signal_pipeline`
// — closes the last gateway→parent production edges before the touring-ceg
// physical move. The parent re-exports each one at `crate::shared::<mod>` so
// every historical path keeps resolving.
pub mod ast_grep_signal;
pub mod bash_ast_validator;
// gate_metrics relocated to touring-foundation (A5 Path-A step-2, 2026-06-16);
// re-exported so `touring_hooks_shared::gate_metrics::*` resolves unchanged for all
// ~73 consumers (incl. no-touch touring-cli + this crate's own query_cache, which
// calls crate::gate_metrics::record_query_cache_*). Old src/gate_metrics.rs orphaned (git-rm).
pub use touring_foundation::gate_metrics;
pub mod hook_events;
// memory_stats_probe relocated to touring-foundation (A5 Path-A step-1, 2026-06-16);
// re-exported so `touring_hooks_shared::memory_stats_probe` resolves unchanged for all
// consumers (incl. no-touch touring-cli). Old src/memory_stats_probe.rs orphaned (git-rm).
pub use touring_foundation::memory_stats_probe;
pub mod risk_patterns;
pub mod signal_layer;
// Fronteira 2 follow-up (2026-06-10): result_ext (clean) + sandbox_language
// (vocab extracted from touring-ceg) + forbidden_patterns (unblocked by it).
pub mod forbidden_patterns;
pub mod result_ext;
pub mod sandbox_language;
// Fronteira 2 batch (2026-06-10): 27 leaf-safe shared submodules relocated
// from touring-hooks src/shared/ — the parent re-exports each at
// `crate::shared::<mod>`, so all historical paths keep resolving.
pub mod antipatterns;
pub mod api_cascade_bridge;
pub mod async_runtime;
pub mod cascade_queue;
pub mod cila;
pub mod command_hash;
pub mod cursor_pool;
pub mod detect_language;
pub mod feature_flags;
pub mod file_prefetch;
pub mod job_registry;
pub mod latency_marker;
pub mod leiden;
pub mod metadata_dedup;
// moka_policies relocated to touring-foundation (A5 step-2, 2026-06-15); re-exported
// so `touring_hooks_shared::moka_policies` (and hooks-core's re-export of it) resolve
// unchanged. Old src/moka_policies.rs orphaned on disk (Gabriel git-rm, REGRA #11).
pub use touring_foundation::moka_policies;
pub mod mpatch_preview;
pub mod parser_cache;
pub mod parser_cache_global;
pub mod patterns;
// query_cache relocated to touring-foundation (A5 Path-A step-3, 2026-06-16);
// re-exported so `touring_hooks_shared::query_cache::*` resolves unchanged for all
// consumers (incl. no-touch touring-cli). Old src/query_cache.rs orphaned (git-rm).
pub use touring_foundation::query_cache;
pub mod recursion_guard;
pub mod shadow_rollout;
pub mod span_context;
pub mod task_features;
pub mod terminal_job_cache;
pub mod thread_pool;
// `touring_error` (a 3rd, never-adopted `TouringError` enum — the aspirational
// "future unified error" with ZERO type consumers) removed from the module tree
// (RBP cleanup, 2026-06-15): reduces the workspace from 3 → 2 `TouringError` enums.
// Old src/touring_error.rs orphaned on disk (Gabriel git-rm, REGRA #11).
