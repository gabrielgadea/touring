//! CLI command handlers for the daemon's dispatch table.
//!
//! These are NOT Claude Code hooks — they are CLI-specific query handlers
//! registered in the dispatch table under "cli-*" names (e.g. "cli-learning-status").
//!
//! Each handler has signature: `fn(&mut HookRuntime, &serde_json::Value) -> String`
//! The returned String is JSON to send back to the CLI client.
//!
//! The handlers themselves now live in `cli/*` submodules (Master Plan A.W2.P5);
//! this file is the facade: response structs + `pub use` re-exports preserving the
//! `crate::cli_handlers::cli_*` paths the dispatch table (hook_registry.rs) uses.
// `HookRuntime` is referenced only by the `#[path]` test submodules via `super::*`;
// gate to cfg(test) so the non-test build does not flag it unused.
#[cfg(test)]
use crate::runtime::HookRuntime;
use serde::Serialize;
use std::path::Path;
/// Normalize `file_path` to be relative to `project_root`.
///
/// All knowledge DB tables store relative paths. This helper ensures CLI
/// handlers accept both absolute and relative paths and always query with
/// relative paths. Inspired by `HookRuntime::make_relative` pattern used in
/// hooks — but factored here so CLI handlers can use it without accessing
/// HookRuntime internals.
pub(crate) fn normalize_to_relative(file_path: &str, project_root: &Path) -> String {
    let p = Path::new(file_path);
    if p.is_absolute() {
        p.strip_prefix(project_root)
            .map(|r| r.to_string_lossy().to_string())
            .unwrap_or_else(|_| file_path.to_string())
    } else {
        file_path.to_string()
    }
}
/// Snapshot of the RL learning engine's state for `cli_learning_status`.
#[derive(Serialize)]
pub struct LearningStatus {
    /// Number of reward updates applied so far.
    pub update_count: u64,
    /// Exponential moving average of received rewards.
    pub ema_reward: f64,
    /// Mean temporal-difference error across updates.
    pub mean_td_error: f64,
    /// Whether the LinUCB bandit model was loaded.
    pub linucb_loaded: bool,
    /// Identifier of the active bandit algorithm.
    pub bandit_type: String,
    /// Number of bandit arms (candidate actions).
    pub arm_count: usize,
    /// AgenticRL state snapshot — None if AgenticRL was never activated.
    pub agentic_rl_state: Option<crate::agentic_rl::AgenticRLState>,
}
/// A public symbol with no consumers, reported by wiring orphan analysis.
#[derive(Serialize)]
pub struct WiringOrphan {
    /// Module file declaring the orphan symbol.
    pub module_file: String,
    /// Name of the orphan symbol.
    pub symbol_name: String,
    /// Kind of the symbol (fn, struct, enum, etc.).
    pub symbol_kind: String,
    /// Declared visibility of the symbol.
    pub visibility: String,
}
/// Per-module wiring integration status.
#[derive(Serialize)]
pub struct WiringModuleStatus {
    /// Path of the module file.
    pub file_path: String,
    /// Integration score in `0.0..=1.0` (fraction of pub symbols with consumers).
    pub integration_score: f64,
    /// Total number of public symbols in the module.
    pub total_pub_symbols: usize,
    /// Number of orphan symbols in the module.
    pub orphan_count: usize,
    /// Names of the orphan symbols in the module.
    pub orphan_symbols: Vec<String>,
}
/// Workspace-wide wiring summary counts.
#[derive(Serialize)]
pub struct WiringStatus {
    /// Total orphan symbols across the workspace.
    pub orphan_count: usize,
    /// Number of modules analyzed.
    pub module_count: usize,
    /// Total public symbols across all modules.
    pub total_pub_symbols: usize,
    /// Total consumer references across all symbols.
    pub total_consumers: usize,
}
/// A recorded pitfall entry from the gotcha database.
#[derive(Serialize)]
pub struct GotchaEntry {
    /// Row id of the gotcha entry.
    pub id: i64,
    /// Pattern that triggers the gotcha.
    pub pattern: String,
    /// The pitfall description and remediation.
    pub gotcha: String,
    /// Severity classification of the pitfall.
    pub severity: String,
    /// Number of times this gotcha matched.
    pub hit_count: i64,
    /// Number of errors estimated to have been prevented by this gotcha.
    pub prevented_errors: i64,
    /// Time-decayed relevance score, or `None` when not yet computed.
    pub decay_score: Option<f64>,
    /// Whether the pitfall has been marked resolved.
    pub resolved: bool,
}
/// Aggregate counts over the gotcha database.
#[derive(Serialize)]
pub struct GotchaStats {
    /// Total number of gotcha entries.
    pub total_count: usize,
    /// Number of unresolved gotcha entries.
    pub unresolved_count: usize,
    /// Number of resolved gotcha entries.
    pub resolved_count: usize,
}
/// Aggregate counts over the knowledge store.
#[derive(Serialize)]
pub struct KnowledgeStats {
    /// Number of indexed files.
    pub file_count: usize,
    /// Number of code relations recorded.
    pub relation_count: usize,
    /// Number of recorded bash command outcomes.
    pub bash_outcome_count: i64,
    /// Number of recorded edit events.
    pub edit_event_count: i64,
    /// Aggregate gotcha statistics.
    pub gotcha_stats: GotchaStats,
    /// Number of stored memory entries.
    pub memory_entry_count: usize,
}
/// Status of the incremental indexing subsystem.
#[derive(Serialize)]
pub struct IncrementalStatus {
    /// Hook result cache hit rate (NOT parser cache).
    /// Bug 6 (2026-05-02): the underlying cache is populated only during hook
    /// dispatch (pre_read/post_edit/etc.) — direct CLI calls bypass it, so a
    /// fresh session reports 0.0 even with active workspace activity.
    pub cache_hit_rate: f64,
    /// Number of session turns recorded by the runtime.
    pub total_accesses: usize,
    /// Total symbols indexed across all files.
    pub symbol_count: usize,
    /// Total files indexed.
    pub file_count: usize,
    /// Bug 6 (2026-05-02): explanatory note clarifying what `cache_hit_rate`
    /// actually measures, and how to populate it (run hooks first).
    pub note: &'static str,
    /// Bug 6 (2026-05-02): cache backing layer for transparency.
    pub backing_cache: &'static str,
}
#[cfg(feature = "acp-protocol")]
pub use crate::cli::acp::cli_acp_discover;
#[cfg(feature = "acp-protocol")]
pub use crate::cli::acp::cli_acp_message;
pub use crate::cli::ast::cli_ast_blast_cross_feature;
pub use crate::cli::ast::cli_ast_blast_enriched;
pub use crate::cli::ast::cli_ast_callgraph;
pub use crate::cli::ast::cli_ast_features;
pub use crate::cli::ast::cli_ast_meta;
pub use crate::cli::ast::cli_ast_rationale;
pub use crate::cli::ast::cli_ast_skeleton;
pub use crate::cli::ast::cli_ast_tdg;
pub use crate::cli::ast::cli_ast_todos;
pub use crate::cli::cascade::cli_cascade_queue_drain;
pub use crate::cli::cascade::cli_cascade_queue_status;
pub use crate::cli::cognitive::cli_cognitive_engines;
pub use crate::cli::cognitive::cli_cognitive_metrics;
pub use crate::cli::contract::cli_attest_contract;
pub use crate::cli::contract::cli_change_contract;
pub use crate::cli::decompose::cli_decompose_add;
pub use crate::cli::decompose::cli_decompose_create;
pub use crate::cli::decompose::cli_decompose_event;
pub use crate::cli::decompose::cli_decompose_finalize;
pub use crate::cli::decompose::cli_decompose_get;
pub use crate::cli::decompose::cli_decompose_mark_mirrored;
pub use crate::cli::decompose::cli_decompose_ready;
pub use crate::cli::decompose::cli_decompose_status;
pub use crate::cli::decompose::cli_decompose_update;
pub use crate::cli::decompose::cli_decompose_validate;
pub use crate::cli::evolution::cli_evolution_drift;
pub use crate::cli::evolution::cli_evolution_insights;
pub use crate::cli::evolution::cli_evolution_tools;
pub use crate::cli::gate::cli_gate_event;
pub use crate::cli::gate::cli_gate_metrics;
pub use crate::cli::gotcha::cli_gotcha_add;
pub use crate::cli::gotcha::cli_gotcha_init;
pub use crate::cli::gotcha::cli_gotcha_list;
pub use crate::cli::gotcha::cli_gotcha_match;
pub use crate::cli::gotcha::cli_gotcha_stats;
pub use crate::cli::gotcha::cli_gotcha_sync;
pub use crate::cli::granularity::cli_granularity_hint;
pub use crate::cli::granularity::cli_granularity_reset;
pub use crate::cli::granularity::cli_granularity_status;
pub use crate::cli::health::cli_health_delta_history;
pub use crate::cli::health::cli_health_delta_reset;
pub use crate::cli::health::cli_health_delta_status;
pub use crate::cli::hook::cli_hook_memory_recall;
pub use crate::cli::hook::cli_hook_memory_store;
pub use crate::cli::inferlets::cli_inferlets_exec;
pub use crate::cli::inferlets::cli_inferlets_list;
pub use crate::cli::jobs::cli_jobs_drop;
pub use crate::cli::jobs::cli_jobs_list;
pub use crate::cli::jobs::cli_jobs_poll;
pub use crate::cli::jobs::cli_jobs_spawn;
pub use crate::cli::knowledge::cli_bench_run;
pub use crate::cli::knowledge::cli_file_knowledge_extended;
pub use crate::cli::knowledge::cli_metadata_backfill;
pub use crate::cli::knowledge::cli_session_summary;
pub use crate::cli::learning::cli_learning_reward;
pub use crate::cli::learning::cli_learning_status;
pub use crate::cli::memory::cli_memory_list;
pub use crate::cli::memory::cli_memory_recall;
pub use crate::cli::memory::cli_memory_reindex;
pub use crate::cli::memory::cli_memory_stats;
pub use crate::cli::memory::cli_memory_store;
pub use crate::cli::metrics::cli_mcp_overhead;
pub use crate::cli::metrics::cli_profile_status;
pub use crate::cli::metrics::cli_tokio_metrics;
#[cfg(feature = "mpatch-fuzzy")]
pub use crate::cli::mpatch::cli_mpatch_preview;
#[cfg(not(feature = "mpatch-fuzzy"))]
pub use crate::cli::mpatch::cli_mpatch_preview;
pub use crate::cli::plugin::cli_plugin_list;
pub use crate::cli::plugin::cli_plugin_reload;
pub use crate::cli::plugin::cli_plugin_status;
pub use crate::cli::plugin::cli_plugin_unregister;
pub use crate::cli::predict::cli_agentic_rl_status;
pub use crate::cli::predict::cli_predict_action;
pub use crate::cli::predict::cli_world_model_status;
pub use crate::cli::query::cli_graph_flow;
pub use crate::cli::query::cli_query_dsl;
pub use crate::cli::rlsearch::cli_mcts_search;
pub use crate::cli::rlsearch::cli_shadow_validate;
pub use crate::cli::saga::cli_saga_abort;
pub use crate::cli::saga::cli_saga_begin;
pub use crate::cli::saga::cli_saga_decide;
pub use crate::cli::saga::cli_saga_delta;
pub use crate::cli::saga::cli_saga_prepare;
pub use crate::cli::saga::cli_saga_register;
pub use crate::cli::saga::cli_saga_status;
pub use crate::cli::search::cli_search_docs;
pub use crate::cli::search::cli_search_symbols;
pub use crate::cli::shared::touring_claude_dir;
/// S-9: Inject synthetic rewards for common tool patterns to bootstrap RL learning.
/// Called from cli_learning_status and phase_learning when system is in cold-start (update_count < 5).
// Shared cross-module helpers moved to `cli/shared.rs` (Master Plan A.W2.P5
// mechanical extraction). Re-exported so existing `crate::cli_handlers::<helper>`
// call sites across the grouped cli/* modules resolve unchanged.
pub(crate) use crate::cli::shared::{
    ARCTIC_QUERY_PREFIX, discover_canonical_dbs, ensure_decompose_tables,
    inject_synthetic_tool_rewards, keyword_skill_match, memory_recall_sql_federated,
    semantic_or_hash_embedding, semantic_text_embedding,
};
pub use crate::cli::skip::cli_skip_list;
pub use crate::cli::skip::cli_skip_validate;
pub use crate::cli::ssr::cli_ssr_apply;
pub use crate::cli::ssr::cli_ssr_status;
pub use crate::cli::suggest::cli_suggest_action;
pub use crate::cli::suggest::cli_suggest_next;
pub use crate::cli::suggest::cli_suggest_skill;
pub use crate::cli::suggest::cli_suggestion_list_pending;
pub use crate::cli::suggest::cli_suggestion_mark_consumed;
pub use crate::cli::suggest::cli_suggestion_stats;
pub use crate::cli::suggest::cli_suggestions_gc;
pub use crate::cli::tantivy::cli_tantivy_fuzzy;
pub use crate::cli::tantivy::cli_tantivy_reindex;
pub use crate::cli::tantivy::cli_tantivy_search;
pub use crate::cli::tantivy::cli_tantivy_stats;
pub use crate::cli::tantivy::cli_tantivy_suggest;
pub use crate::cli::viz::cli_viz_blast;
pub use crate::cli::viz::cli_viz_cycles;
pub use crate::cli::viz::cli_viz_feature;
pub use crate::cli::viz::cli_viz_orphans;
pub use crate::cli::viz::cli_viz_wiring;
pub use crate::cli::viz::cli_viz_workspace;
pub use crate::cli::wiring::cli_wiring_chains;
pub use crate::cli::wiring::cli_wiring_community;
pub use crate::cli::wiring::cli_wiring_cycles;
pub use crate::cli::wiring::cli_wiring_impact;
pub use crate::cli::wiring::cli_wiring_modules;
pub use crate::cli::wiring::cli_wiring_orphans;
pub use crate::cli::wiring::cli_wiring_purpose;
pub use crate::cli::wiring::cli_wiring_status;
pub use crate::cli::wiring::cli_wiring_suggest;
pub use crate::cli::workflow::cli_workflow_compare;
pub use crate::cli::workflow::cli_workflow_run;
pub use crate::cli::workflow::cli_workflow_slowest;
pub use crate::cli::workflow::cli_workflow_stats;
pub use crate::cli_handlers_session::cli_session_assess;
pub use crate::cli_handlers_session::cli_session_checkpoint;
pub use crate::cli_handlers_session::cli_session_list;
pub use crate::cli_handlers_session::cli_session_start;
// Test-only re-exports: the `tests` module reaches these via `super::*`.
#[cfg(test)]
pub(crate) use crate::cli::shared::{memory_recall_fts5_expr, memory_recall_sql};
// Daemon health/flywheel/prompt-enhance/harness-metric/doctor/status handlers
// moved to `cli/health.rs` (Master Plan A.W2.P5). Re-exported so dispatch
// closures `crate::cli_handlers::cli_*` (hook_registry.rs) resolve unchanged.
pub use crate::cli::health::{
    cli_doctor, cli_flywheel_status, cli_harness_metric, cli_incremental_status,
    cli_prompt_enhance, cli_status,
};
// Live `cli_pre_task_scout` (+ scout cache helpers, run_scouter_*,
// quick_quality_context, PreToolUsePayload) moved to `cli/pretask.rs`
// (Master Plan A.W2.P5 mechanical extraction). Re-exported so the dispatch
// closure `crate::cli_handlers::cli_pre_task_scout` (hook_registry.rs) resolves.
pub use crate::cli::pretask::cli_pre_task_scout;
// `cli_rl_warmstart` (+ read_corpus_bash_outcomes) moved to `cli/rlsearch.rs`
// (Master Plan A.W2.P5). Re-exported for the dispatch closure.
pub use crate::cli::rlsearch::cli_rl_warmstart;

// `cli_ctx_execute` (+ parse/format helpers + tests) moved to `cli/execute.rs`;
// `cli_calibrate_confidence` moved to `cli/calibrate.rs`
// (Master Plan A.W2.P5 mechanical extraction). Re-exported so dispatch
// closures `crate::cli_handlers::cli_ctx_execute` / `cli_calibrate_confidence` resolve.
pub use crate::cli::calibrate::cli_calibrate_confidence;
pub use crate::cli::execute::cli_ctx_execute;

// `cli_gate_event_tests` lives beside this hub in `cli/handlers/gate_tests.rs`
// (A01 fold 2026-06-06); `#[path]` keeps it a `cli_handlers` submodule so `super::*` resolves.
#[cfg(test)]
#[path = "gate_tests.rs"]
mod cli_gate_event_tests;

/// Self-contained skip-region parser (mirrors post_edit.rs).
#[derive(Debug, Clone, serde::Serialize)]
pub(crate) struct SkipRegionRaw {
    pub(crate) start: u64,
    pub(crate) end: u64,
    pub(crate) style: &'static str,
}
/// `cli-file-knowledge-extended` — Return 23-field enriched metadata for a file.
///
/// Queries `FileKnowledgeDB::query_extended()` which LEFT JOINs across:
/// file_knowledge + cognitive_enrichment + module_ecosystem + file_blake3_registry
/// + file_test_coverage + file_communities
///
/// Payload: `{"file_path": "src/lib.rs"}`
/// OnceLock guard — DDL runs exactly once per daemon process lifetime.
///
/// Wave 22 (S-Q5): the 5× `CREATE TABLE IF NOT EXISTS` batch previously ran on
/// every `cli-file-knowledge-extended` invocation. `OnceLock` ensures the
/// execute_batch fires once; subsequent calls see `get() = Some(())` and skip.
pub(crate) static FK_EXTENDED_DDL_DONE: std::sync::OnceLock<()> = std::sync::OnceLock::new();
// `mod tests` lives beside this hub in `cli/handlers/tests.rs` (A01 fold 2026-06-06);
// `#[path]` keeps it a `cli_handlers` submodule so `super::*` + `crate::` paths resolve.
#[cfg(test)]
#[path = "tests.rs"]
mod tests;
/// GraphData, NodeData, EdgeData — mirrored from touring-server/src/visual/mod.rs
/// Re-defined here to avoid cross-crate dependency (touring-hooks cannot depend on touring-server)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct VizGraphData {
    /// Nodes of the visualization graph.
    pub nodes: Vec<VizNodeData>,
    /// Edges connecting the graph nodes.
    pub edges: Vec<VizEdgeData>,
}
/// A node in the code visualization graph (one symbol or module).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct VizNodeData {
    /// Stable identifier of the node.
    pub id: String,
    /// Human-readable label for display.
    pub label: String,
    /// Quality score in `0.0..=1.0`, or `None` when unknown.
    pub quality_score: Option<f32>,
    /// Number of inbound references, or `None` when unknown.
    pub fan_in: Option<usize>,
    /// Number of outbound references, or `None` when unknown.
    pub fan_out: Option<usize>,
    /// Whether the node has no consumers.
    pub is_orphan: bool,
    /// Whether the node contains unsafe code.
    pub has_unsafe: bool,
}
/// An edge in the code visualization graph (a relation between two nodes).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct VizEdgeData {
    /// Id of the source node.
    pub from: String,
    /// Id of the target node.
    pub to: String,
    /// Kind of relation the edge represents.
    pub kind: String,
}

// =============================================================================
// ES1 P4 (CAH roadmap 2026-05-30) — `cli-prove-claim` MCP/CLI handler
// =============================================================================
//
// Wires the standalone SMT service from `touring-offensive` into the
// `touring-hooks` dispatch table. The handler is a thin JSON adapter:
// it parses the request payload, calls `prove_claim`, and serializes
// the `ProofReport` as a JSON value with a stable schema.
//
// P4 is the SHIPPING surface. The actual proof engine lives in
// `touring_offensive::solver::prove_claim`. P3 (deferred) will add
// X3.5 gateway wiring + an `Execution<Proven>` typestate on top of
// this minimal adapter.

// `cli_prove_claim` + its `error_json` helper moved to `cli/prove.rs`
// (Master Plan A.W2.P5 mechanical extraction). Re-exported so the
// dispatch table closure `crate::cli_handlers::cli_prove_claim` resolves.
pub use crate::cli::prove::cli_prove_claim;

// Carve R (2026-06-10): the LearnRuntime/CegRuntime impls for HookRuntime moved
// to touring-hook-runtime::ceg_impls (orphan rule — the type lives there now).
