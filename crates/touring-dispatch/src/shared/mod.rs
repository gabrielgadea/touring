// Shared utilities for touring-hooks.
//
// Centralizes duplicated helper functions that were previously copied across
// multiple pre-/post-hook modules (detect_language, is_test_file,
// measure_quality_snapshot, reindex_file).
//
// ── shared/ relocation status (Fronteira 2 + follow-up, 2026-06-10) ──────────
//
// 33 leaf-safe submodules + result_ext + forbidden_patterns + sandbox_language
// were relocated to `touring-hooks-shared` (re-exported below at
// `crate::shared::<mod>`). The 9 modules that REMAIN as real `pub mod` here are
// genuinely parent-coupled — each binds to a parent *engine* singleton or the
// HookRuntime God-object, so they belong with their engine, NOT in the leaf:
//
//   | module             | keystone (why it stays)                              |
//   |--------------------|------------------------------------------------------|
//   | signals            | `crate::tantivy_index::{SearchHit, global_tantivy}`  |
//   | signal_pipeline    | `super::signals::{normalize_scores, score_cmp}`      |
//   |                    |   (vocab already split to leaf `signal_layer` F4-pre)|
//   | metadata_collector | `crate::tantivy_index::{global_tantivy, ext_to_lang}`|
//   | tantivy_stream     | `crate::tantivy_index` + `crate::circuit_breaker`    |
//   | hook_context       | `crate::knowledge::FileKnowledgeDB`                  |
//   | session_context    | `crate::knowledge::{FileKnowledge, FileKnowledgeDB}` |
//   | quality            | `crate::ast_bridge::{FileQualityMetrics, analyze_*}` |
//   | session_bus        | `crate::ann_memory::SearchResult`                   |
//   | reindex            | `crate::runtime::HookRuntime` + ast_bridge/knowledge/|
//   |                    |   post_read/wiring (5-way; deepest coupling)        |
//
// To relocate any of these, the keystone ENGINE (tantivy_index / knowledge /
// ast_bridge / ann_memory) would have to move first — a much larger wave, and
// architecturally these query layers belong with their engine. `reindex` is
// never a leaf candidate (HookRuntime). This block is the canonical verdict so
// a future session does not re-scout them as trivial candidates.

pub use touring_hooks_shared::antipatterns; // Fronteira 2 (2026-06-10): relocated to the leaf crate
pub use touring_hooks_shared::api_cascade_bridge; // Fronteira 2 (2026-06-10): relocated to the leaf crate
pub use touring_hooks_shared::ast_grep_signal; // Session B F4-pre (2026-06-10): relocated to the leaf crate
pub use touring_hooks_shared::async_runtime; // Fronteira 2 (2026-06-10): relocated to the leaf crate
pub use touring_hooks_shared::bash_ast_validator; // Session B F4-pre (2026-06-10): relocated to the leaf crate
pub use touring_hooks_shared::cascade_queue; // Fronteira 2 (2026-06-10): relocated to the leaf crate
pub use touring_hooks_shared::cila; // Fronteira 2 (2026-06-10): relocated to the leaf crate
pub use touring_hooks_shared::command_hash; // Fronteira 2 (2026-06-10): relocated to the leaf crate
pub use touring_hooks_shared::cursor_pool; // Fronteira 2 (2026-06-10): relocated to the leaf crate
pub use touring_hooks_shared::detect_language;
pub use touring_hooks_shared::risk_patterns; // Session B F4-pre (2026-06-10): relocated to the leaf crate // Fronteira 2 (2026-06-10): relocated to the leaf crate
/// Sentrux Master Plan Wave 3 P7 (2026-05-09) — multi-workspace
/// Sentrux quality signal aggregator. Pure data layer; no I/O.
pub mod federation;
pub use touring_hook_handlers::shared::metadata_collector; // Wave H (2026-06-10): hook-only engine, lives in touring-hook-handlers
pub use touring_hook_runtime::shared::quality; // Carve R (2026-06-10): session engine moved to the runtime layer
pub use touring_hook_runtime::shared::reindex; // Carve R (2026-06-10): session engine moved to the runtime layer
pub use touring_hooks_shared::gate_metrics; // Session B F4-pre (2026-06-10): relocated to the leaf crate
pub use touring_hooks_shared::job_registry; // Fronteira 2 (2026-06-10): relocated to the leaf crate
pub use touring_hooks_shared::memory_stats_probe; // Session B F4-pre (2026-06-10): relocated to the leaf crate
pub use touring_hooks_shared::metadata_dedup; // Fronteira 2 (2026-06-10): relocated to the leaf crate
pub use touring_hooks_shared::moka_policies; // Fronteira 2 (2026-06-10): relocated to the leaf crate
pub use touring_hooks_shared::mpatch_preview; // Fronteira 2 (2026-06-10): relocated to the leaf crate
pub use touring_hooks_shared::parser_cache; // Fronteira 2 (2026-06-10): relocated to the leaf crate
pub use touring_hooks_shared::parser_cache_global; // Fronteira 2 (2026-06-10): relocated to the leaf crate
pub use touring_hooks_shared::patterns; // Fronteira 2 (2026-06-10): relocated to the leaf crate
pub use touring_hooks_shared::query_cache; // Fronteira 2 (2026-06-10): relocated to the leaf crate
pub use touring_hooks_shared::recursion_guard; // Fronteira 2 (2026-06-10): relocated to the leaf crate
pub use touring_hooks_shared::result_ext; // Fronteira 2 follow-up (2026-06-10): relocated to leaf
pub use touring_hooks_shared::terminal_job_cache; // Fronteira 2 (2026-06-10): relocated to the leaf crate
// Phase C carve (2026-06-10): the `ResultExt` crate-local re-export moved to
// touring-hooks-core's shared facade (its only consumer, branch_fs, was carved).
pub use touring_hooks_shared::feature_flags; // Fronteira 2 (2026-06-10): relocated to the leaf crate
pub use touring_hooks_shared::forbidden_patterns; // Fronteira 2 follow-up (2026-06-10): relocated to leaf (SandboxLanguage vocab in leaf)
pub mod hook_context;
pub use touring_hooks_shared::hook_events; // Session B F4-pre (2026-06-10): relocated to the leaf crate
pub use touring_hooks_shared::latency_marker; // Fronteira 2 (2026-06-10): relocated to the leaf crate
pub use touring_hooks_shared::leiden; // Fronteira 2 (2026-06-10): relocated to the leaf crate
#[cfg(test)]
mod result_ext_integration_test;
pub use task_features::{TaskRoutingDecision, extract_task_features};
pub use touring_hook_handlers::shared::signal_pipeline; // Wave H (2026-06-10): hook-only engine, lives in touring-hook-handlers
pub use touring_hook_runtime::shared::session_context; // Carve R (2026-06-10): session engine moved to the runtime layer
pub use touring_hook_runtime::shared::signals; // Carve R (2026-06-10): session engine moved to the runtime layer
#[cfg(feature = "tantivy-fts")]
pub use touring_hook_runtime::shared::tantivy_stream;
pub use touring_hooks_core::shared::session_bus; // Phase C carve (2026-06-10): session_bus joined touring-hooks-core (it consumes ann_memory; the prediction crate already deps on the leaf → leaf relocation would cycle)
pub use touring_hooks_shared::file_prefetch; // Fronteira 2 (2026-06-10): relocated to the leaf crate
pub use touring_hooks_shared::shadow_rollout; // Fronteira 2 (2026-06-10): relocated to the leaf crate
pub use touring_hooks_shared::span_context; // Fronteira 2 (2026-06-10): relocated to the leaf crate
pub use touring_hooks_shared::task_features; // Fronteira 2 (2026-06-10): relocated to the leaf crate
pub use touring_hooks_shared::thread_pool; // Fronteira 2 (2026-06-10): relocated to the leaf crate
// `touring_error` re-export removed (RBP cleanup 2026-06-15): the never-adopted
// 3rd `TouringError` enum was dropped from touring-hooks-shared (orphan, 0 consumers).
