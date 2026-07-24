//! touring-hooks — Neural Hooks Architecture v12.0.0
//!
//! The Touring Neural Hooks system transforms Claude Code's hook events
//! into a feedback-driven intelligence engine:
//!
//! - **Sensory** (Pre-hooks): Inject accumulated knowledge before tool execution
//! - **Motor** (Post-hooks): Capture outcomes and learn after tool execution
//! - **Knowledge** (SQLite WAL): Persistent file graph, command outcomes, edit history
//! - **Quality** (ACO Bridge): 9D goal tracking integrated into HookRuntime (v12.0.0)
//! - **OutputCapture**: Intelligent output summarization with structured metrics (v12.0.0)
//!
//! Feedback loops:
//! - File Knowledge: post-read → knowledge DB → pre-read (next time)
//! - Command Learning: post-bash → outcomes DB → pre-bash (next time)
//! - Edit Impact: post-edit → relations DB → pre-edit (next time)
//! - Cross-cutting: post-bash(error) → pre-read(warns about file)
//! - Quality: session-start → quality tracking → session-stop (report) (v12.0.0)

// Test-only lints: these fire only in #[cfg(test)] blocks and never reach production.
// Using clippy::all avoids editing 30+ test files individually.
#![cfg_attr(test, allow(clippy::all))]
// D.W2.P3 (2026-06-06): rustdoc hygiene gate. `warn` (not `deny`) is deliberate —
// `deny(missing_docs)` would break the build on thousands of legacy pub items;
// broken intra-doc links are the actionable signal and surface without blocking.
#![warn(rustdoc::broken_intra_doc_links)]
// RBP-01 elite-lint ratchet (2026-06-16): prod-unwrap-free (1 fix: daemon.rs:1261
// `serde_json::to_value(caps).unwrap()` → `.expect(..)`; the other 35 `.unwrap()`
// live in the `#[cfg(test)] mod tests` 19k testfile) — lock against future bare
// unwrap in non-test code.
#![cfg_attr(not(test), deny(clippy::unwrap_used))]

// ── Always-on modules (core infrastructure) ────────────────────────────
pub use touring_ceg::capability; // Session B F4 (2026-06-10): CEG capability model now lives in the touring-ceg leaf crate — re-exported so every touring_hooks::capability / crate::capability path keeps resolving
pub use touring_ceg::gateway;
pub use touring_hook_runtime::bidirectional; // Wave H (2026-06-10): lives in the runtime layer
pub use touring_hook_runtime::ceg_adapter; // CEG hook-driver adapter — Wave H (2026-06-10): lives in the runtime layer (the HookRuntime/HookResponse seam) // Session B F4 (2026-06-10): the X0..X9 CEG pipeline now lives in the touring-ceg leaf crate — re-exported so every touring_hooks::gateway / crate::gateway path keeps resolving
// S-13 (2026-06-06): the touring-offensive solver facade now lives inside the
// gateway (`gateway/offensive_integration.rs`) so it travels with the CEG at
// crate extraction. Re-exported here to preserve the public API
// (`touring_hooks::offensive_integration::*`) and every `crate::offensive_integration`
// call site unchanged — zero behavioural change.
pub use crate::gateway::offensive_integration; // ES1 P3 (2026-06-01): re-exports SMT solver types (ProofReport/ClaimKind/etc) without leaking touring-offensive internal layout
pub use touring_cli::cli_handlers_mutation_test;
pub use touring_hooks_core::cross_agent_ledger; // ES3 P4 (2026-06-03): cross-agent outcome ledger (rusqlite WAL) for N-agent feedback sync (CAH OP4 §5.2.5)
pub use touring_hooks_prediction::classifier; // Wave T1 handler — Wave C2 (2026-06-10): lives in touring-cli
// S-13 (2026-06-06): ActionSignature relocated to the touring-hooks-shared LEAF
// crate (leaf-safe — zero crate:: deps). Re-exported here to preserve the public
// API (`touring_hooks::action_signature::*`) and every `crate::action_signature`
// call site unchanged — zero behavioural change.
pub use touring_hooks_core::proc_identity;
pub use touring_hooks_shared::action_signature; // 2026-05-15: ActionSignature — (tool_class, intent_class, context_qualifier) key for action-scoped outcome persistence
// Wave R+C I2 (2026-06-10): pure extractors carved from post_read to the core.
pub use touring_cli::cli_suggester; // PreToolUse classifier — Wave C2 (2026-06-10): lives in touring-cli
pub use touring_cli::workflow; // CEG Pln2 P8 Workflow Intelligence — Wave C2 (2026-06-10): lives in touring-cli with its single consumer (cli_suggester)
pub use touring_hook_runtime::gotcha_loader; // Wave Q3: YAML rule library loader for gotchas
pub use touring_hook_runtime::suggesters; // Pln3 suggestion detectors — Wave H (2026-06-10): lives in the runtime layer
pub use touring_hook_runtime::task_digest; // Wave H (2026-06-10): lives in the runtime layer
pub use touring_hooks_core::mutation_test; // Wave T1: cargo-mutants wrapper lib (config + report + parser + cache)
pub use touring_hooks_core::panic_log; // Sprint 4.5 (2026-05-23): std::panic::set_hook captures forensics to ~/.claude/touring/daemon-crash.jsonl before process abort
pub use touring_hooks_core::pre_tool_validator;
pub use touring_hooks_core::symbol_extractors; // Sprint 3 PC-1 (2026-05-23): set_process_name via extern prctl(PR_SET_NAME) — REGRA #19 process distinguishability
pub use touring_hooks_core::workflow_templates; // TR-5 2026-05-19: W1-W10 reusable decompose blueprint catalog (Rn2 §6)
pub use touring_hooks_prediction::pii;
pub use touring_hooks_prediction::tfidf_retriever;
pub use touring_hooks_shared::memory_finding; // Wave Q4: M-500..M-530 diagnostic codes for memory recall outcomes // Wave M1 reescopado: TF-IDF over touring memory + decompose corpus // Pln3-P3: uniform Suggester trait + storage helpers for suggesters

// Wave 5 (2026-04-18) — link-time declarative hook registration (side-by-side
// with the manual ALL_DAEMON_HOOK_NAMES table). See inventory_registry.rs.
pub use touring_hooks_core::inventory_registry;

// Wave 5 (2026-04-18) — public Rust workflow advisory helpers
// (hint + reward + combined). Consumed by post_edit::run_returning and
// the cross-crate integration test in touring-integration-tests.
pub use touring_hooks_core::wave5_workflow;
pub use wave5_workflow::{
    code_workflow_advisory, code_workflow_hint, code_workflow_reward, rust_workflow_advisory,
    rust_workflow_hint, rust_workflow_reward,
};

// ACO Bridge — touring-learning ACO integration
pub use touring_hooks_core::aco_bridge;

// W4-4: ACO event processor for decomposer events
pub use touring_hooks_core::aco_processor;

// THSF Phase 5 Opt A (2026-04-24) — embedded capnp RPC server co-hosted
// in touring-daemon. Preserves in-process broadcast between
// compute_signals_delta (producer) and GeneratorHealthImpl (consumer).
#[cfg(feature = "capnp-server")]
pub use touring_hooks_core::capnp_embed;

// Wave 9 (2026-04-18) — Cross-hook health delta bridge (pre_edit→post_edit)
pub use touring_hooks_core::health_delta;

// THSF Phase 5 Wave I (2026-04-24) — grow-only SQLite audit trail for
// HealthDeltaEvent. Parallel writer to touring-core::publish_health_event.
pub use health_delta::{
    HealthDelta, STREAK_ALERT_THRESHOLD, StreakCounters, compute_health_delta,
    compute_signals_delta, delta_reward, discard_pre_health, format_delta_hint, improvement_streak,
    improvement_streak_hint, pending_len as health_delta_pending_len, record_pre_health,
    record_pre_signals, regression_streak, reset_json as health_delta_reset_json, reset_streak,
    status_json as health_delta_status_json, streak_counters, streak_warning_hint,
};
pub use touring_hooks_core::health_delta_audit;

// ACO Wiring — UnifiedPheromoneBus + TrackerRlBridge E2E chains
pub use touring_hooks_core::aco_wiring;

// Pipeline adapter layer — bridges touring-flow Stage types into touring-hooks
pub use touring_hooks_core::pipeline;

// R19: Runtime metrics export for observability
pub use touring_hooks_shared::metrics;

// Cognitive Bridge — touring-cognitive integration
// CortexDispatcher — thin tool call dispatcher for touring-cortex integration
pub use touring_hooks_core::cognitive_bridge;
pub use touring_hooks_core::cortex_dispatcher;

// AST Bridge — touring-ast integration
pub use touring_hooks_core::ast_bridge;

// Prompt Enhancement (native Rust replacement for prompt_enhancer.py)
// Wave C2 inversion (2026-06-10): moved to touring-hook-runtime (was
// hooks/prompt_enhance.rs) — hook_registry keeps `crate::prompt_enhance::*`.
pub use touring_hook_runtime::prompt_enhance;

// Neural Hooks core modules (always available)
pub use touring_hooks_core::knowledge;

// Wiring Intelligence: orphan detection + integration scoring
pub use touring_hook_runtime::wiring;

// RFC-100 emission helpers: centralized W-code diagnostic emitters (W-101, W-110, W-120)
pub use touring_hooks_shared::rfc100_emission;

// Query DSL: recursive descent parser for structured symbol queries
pub use touring_hooks_shared::query_dsl;

// Custom error types — Track B2
pub use touring_hooks_shared::errors;

// Ecosystem: module role classification and project structure
pub use touring_hooks_core::ecosystem;
pub use touring_hooks_core::error_predictor;
// S-13 (2026-06-06): drift_corrector relocated into the gateway (it uses only
// gateway types + is used only by gateway/learn.rs). Re-exported to preserve the
// public API + the `crate::drift_corrector` call site (learn.rs) unchanged.
pub use crate::gateway::drift_corrector; // S-14/R13: system-wide drift-correction loop (re-ground vs deterministic sensors)
pub use touring_hook_runtime::runtime;
pub use touring_hooks_core::approval_store; // S-15/R14: durable cross-session HITL approvals (pending_approvals table)
pub use touring_hooks_core::conformal; // S-08/A-A1: split-conformal calibration of the skill-selection gate (KnowNo) — replaces the hardcoded 0.7 cut

// H1-C: In-memory petgraph dependency cache for fast blast_radius + Tarjan SCC cycle detection
pub use touring_hooks_core::dependency_cache;

// ACP shim layer — opt-in protocol envelope over daemon socket (acp-protocol feature gate)
// Wave C2 inversion (2026-06-10): moved to touring-hook-runtime; re-exported so
// daemon.rs (`crate::protocol::acp`) and cross-crate callers keep their paths.
#[cfg(feature = "acp-protocol")]
pub use touring_hook_runtime::protocol;

// Daemon IPC — protocol types shared between thin client and daemon server
pub use touring_hooks_core::ipc;

// D1.6 Activity hook integration — direct EventStore (no IPC overhead)
pub use touring_hooks_core::activity_hook;

// S14: File-based circuit breaker for IPC daemon calls
pub use touring_hooks_core::circuit_breaker;

// Circuit state machine — extracted types from circuit_breaker.rs (Track A2)
pub use touring_hooks_core::circuit_state_machine;

// S4: Intelligent lifecycle hook handlers (file-changed, cwd-changed, pre-compact, etc.)
pub mod lifecycle;

// Daemon server — persistent process that keeps HookRuntime alive
pub mod daemon;

// Wave R+C I3 (2026-06-10): actor protocol type, decoupled from the daemon so it
// can descend to the touring-hook-runtime layer at the next carve.
pub use touring_hook_runtime::daemon_protocol;

// S10: Centralized hook registry — single source of truth for hook names + dispatch
pub mod hook_registry;

// D2.1: Tool output router for PreToolUse sandbox routing
pub use touring_hooks_core::tool_output_router;

// D2.2: Sandbox subprocess executor (context-mode integration)
// S-13 (2026-06-06): the sandbox runner now lives inside the gateway
// (`gateway/sandbox_executor.rs`) so it travels with the CEG at crate extraction;
// this collapses the gateway<->sandbox_executor module cycle to intra-gateway.
// Re-exported to preserve the public API (`touring_hooks::sandbox_executor::*`)
// and every `crate::sandbox_executor` call site unchanged (incl. the 5 non-gateway
// consumers) — zero behavioural change.
pub use crate::gateway::sandbox_executor;
// S-13 cross-audit (2026-06-06): the sandbox-result → Tantivy storage bridge lives in
// the parent (not the gateway) so the gateway carries no tantivy_index/compression edge.
// Gated on `tantivy-fts` (its fns only exist under that feature).
#[cfg(feature = "tantivy-fts")]
pub use touring_hooks_core::sandbox_output_store;
// S-13 (2026-06-06): the temporal-split classification module moved into the
// gateway as `staging_classify` (renamed to avoid colliding with gateway's own
// `staging` area/GC module). Aliased back to `crate::staging` so the public API
// (`touring_hooks::staging::*`) and `staging_registry`'s `crate::staging` import
// are unchanged.
pub use crate::gateway::staging_classify as staging; // CEG Pln2 P1.6: heredoc temporal-split detection (StagingRegistry stub — script written one turn, executed later, still gated)

// D6: MCP Context Router for multi-agent context sharing (ctx_search/ctx_index/...)
pub use touring_cli::cli_handlers_mcp; // Wave C2 (2026-06-10): lives in touring-cli (path preserved: touring_hooks::cli_handlers_mcp::ctx_*)

// I-10: Progressive throttling (3-tier session-based call rate limiting)
pub use touring_hooks_core::throttle;

// I-15: SessionStart Guide builder — 15 structured sections for context resume
pub use touring_hooks_core::session_guide;

// NEW-1: Per-command compression profiles (RTK parity, 15+ profiles)
pub use touring_hooks_core::compression_profiles;

// NEW-4: User-defined TOML filter DSL (~/.config/touring/filters.toml)
pub use touring_hooks_shared::user_filters;

// Wave 3 INTELLIGENCE — Extended (T2 + T3) — 25 envelope implementations
pub use touring_hooks_core::wave3_extended;

// WS4: Output capture with metrics extraction (autoresearch P6)
pub use touring_hooks_core::output_capture;

// Shared utilities (detect_language, quality, antipatterns, signals, etc.)
pub mod shared;

// Feature D (2026-04-24) — Schema validation layer for hook payloads and MCP params.
pub use touring_hook_runtime::schemas;
// `#[macro_export]` macros live at the defining crate's root (now the runtime
// layer) — re-export explicitly so the touring-hooks façade keeps resolving it.
pub use touring_hook_runtime::with_validation;

// HookRuntime concrete implementation (HookResponse, HookTimer, etc.)
pub use touring_hook_runtime::hook_runtime;

// HookResponse — extracted from hook_runtime.rs (Track A1)
pub use touring_hooks_core::hook_response;

// Async knowledge DB wrapper
pub use touring_hooks_core::async_knowledge;

// WASM Inferlet service
pub use touring_hook_runtime::inferlets;

// WASM Inferlet embedded assets
#[cfg(feature = "inferlets-wasm")]
pub use touring_hook_runtime::inferlets_assets;

// Tantivy FTS integration (feature-gated)
#[cfg(feature = "tantivy-fts")]
pub use touring_hooks_core::tantivy_index;

// CLI daemon-side query handlers
// Wave C2 (2026-06-10, PoNR #4): the entire cli/ handler tree + cli_e2e moved to
// the touring-cli crate. Re-exported at the historical module paths so the
// hook_registry dispatch table here (288 downward call sites) and cross-crate
// consumers (touring_hooks::cli_handlers_* — touring-server's 22 imports) are
// unchanged, byte-for-byte.
pub use touring_cli::{
    cli, cli_e2e, cli_handlers, cli_handlers_decompose, cli_handlers_entity,
    cli_handlers_file_knowledge, cli_handlers_index, cli_handlers_semantics, cli_handlers_session,
    cli_handlers_wiring_repair,
};

// ANN Memory Recall — approximate nearest neighbor semantic search
pub use touring_hooks_prediction::ann_memory;

// GoT Snapshot Store — persistent GoT snapshots
pub use touring_hooks_shared::got_snapshot_store;

// Layer 7: Prediction — anticipatory intelligence
pub use touring_hooks_prediction::layer7_prediction;

// Functional wiring — purpose-based chains
pub use touring_hooks_core::functional_wiring;

// Precomputed signals cache
pub use touring_hooks_shared::precomputed_signals;

// Pattern bandit for semantic classification
pub use touring_hooks_shared::pattern_bandit;

// P4-S2: Agentic RL — POMDP state + PPO policy optimization with pattern_bandit integration.
// A01 (2026-06-06): extracted to the leaf crate `touring-hooks-rl`; re-exported here as
// `touring_hooks::agentic_rl` so every call-site (cli_handlers, post_tool_rl, hook_runtime,
// cli/predict, integration-tests) is preserved.
pub use touring_hooks_rl as agentic_rl;

pub use agentic_rl::{AgenticRL, AgenticRLState, BeliefState, ObservableState, ToolType};

// Callgraph enrichment for pre_read blast radius
pub use touring_hooks_core::callgraph_enrichment;

// Reranked context — RRF reranking for pre_read
pub use touring_hooks_shared::reranked_context;

// Semantic classifier — intent classification with pattern matching
pub use touring_hooks_prediction::semantic_classifier;

// Saga — distributed 2PC coordinator for multi-agent subagent orchestration.
// A01 (2026-06-06): extracted to the leaf crate `touring-hooks-saga`; re-exported
// here as `touring_hooks::saga` so every call-site (hook_runtime, cli/saga) is preserved.
pub use touring_hooks_saga as saga;

// N1 bridge — Agent Teams integration
pub use touring_hooks_shared::n1_bridge;

// Hook ↔ Decompose Bridge — wires hook events to decompose task system
pub use touring_hook_handlers::hook_decompose_bridge; // Wave H (2026-06-10): lives in touring-hook-handlers

// MCTS subgoal materialization — Pln3 R7 bridge (MCTS → Pln2 bidirectional channel)
pub use touring_hook_handlers::mcts_materializer; // Wave H (2026-06-10): lives in touring-hook-handlers

// HookMemoryBridge — tiered memory for hook events (ephemeral → working → semantic)
pub use touring_hook_runtime::hook_memory;

// AutoSaveHook — interval-based checkpointing (replaces mempal_save_hook.sh)
pub use touring_hook_runtime::auto_save_hook;

// Plugin system — extensible hook plugins
pub use touring_hooks_shared::plugin;

// Triad hook — code/docs/skill co-evolution
pub use touring_hook_runtime::triad_hook;

// ── Pre-hooks (feature-gated) ──────────────────────────────────────────
#[cfg(feature = "pre-hooks")]
pub use touring_hook_handlers::pre_bash;
#[cfg(feature = "pre-hooks")]
pub use touring_hook_handlers::pre_edit;
#[cfg(feature = "pre-hooks")]
pub use touring_hook_handlers::pre_edit_prevention;
#[cfg(feature = "pre-hooks")]
pub use touring_hook_handlers::pre_glob;
#[cfg(feature = "pre-hooks")]
pub use touring_hook_handlers::pre_grep;
#[cfg(feature = "pre-hooks")]
pub use touring_hook_handlers::pre_read;
#[cfg(feature = "pre-hooks")]
pub use touring_hook_handlers::pre_tool_use;
#[cfg(feature = "pre-hooks")]
pub use touring_hook_handlers::pre_write;

// ── Post-hooks (feature-gated) ─────────────────────────────────────────
#[cfg(feature = "post-hooks")]
pub use touring_hook_handlers::post_bash;
#[cfg(feature = "post-hooks")]
pub use touring_hook_handlers::post_edit;

// Post-edit RuleEngine bridge — wires RuleEngine classification into post_edit
#[cfg(feature = "post-hooks")]
pub use touring_hook_handlers::post_edit_rule_engine;

#[cfg(feature = "post-hooks")]
pub use touring_hook_handlers::post_read;
#[cfg(feature = "post-hooks")]
pub use touring_hook_handlers::post_tool_batch;
#[cfg(feature = "post-hooks")]
pub use touring_hook_handlers::post_tool_failure;
#[cfg(feature = "post-hooks")]
pub use touring_hook_handlers::post_tool_rl;
#[cfg(feature = "post-hooks")]
pub use touring_hook_handlers::post_tool_use;
#[cfg(feature = "post-hooks")]
pub use touring_hook_handlers::post_write;

// ── Lifecycle hooks (always-on) ─────────────────────────────────────────
pub use touring_hook_handlers::permission_request;
pub use touring_hook_handlers::post_compact_handler;
pub use touring_hook_handlers::stop;

// ── D28: MCP overhead self-report telemetry ────────────────────────────
pub use touring_hooks_shared::mcp_overhead;

// ── Instructions loaded handler (lifecycle, always-on) ─────────────────
pub use touring_hook_handlers::instructions_loaded;

// ── Session hooks (feature-gated) ──────────────────────────────────────
#[cfg(feature = "session-hooks")]
pub use touring_hook_handlers::session_hooks;
#[cfg(feature = "session-hooks")]
pub use touring_hooks_core::session_insights;

// ── Task Lifecycle Hooks — task metrics and escalation ────────────────────
pub use touring_hook_handlers::hooks_task_lifecycle;

// ── Team Hooks — N1: Agent Teams ↔ ACO Gateway (always-on) ─────────────
pub use touring_hook_handlers::team_hooks;

// ── LLM-as-a-Judge — P3: Failure severity classification + repair recommendations ──
pub use touring_hooks_prediction::llm_judge;

// ── KnowledgeSymbolBridge — D2: FileKnowledgeDB ↔ SymbolIndex sync ─────
pub use knowledge_symbol_bridge::KnowledgeSymbolBridge;
pub use touring_hooks_core::knowledge_symbol_bridge;

// ── BranchFs — S8: Copy-on-write file snapshots for safe edits ─────────
pub use touring_hooks_core::branch_fs;

// ── Utilities (feature-gated) ──────────────────────────────────────────
#[cfg(feature = "utilities")]
pub use touring_hooks_core::audit;
#[cfg(feature = "utilities")]
pub use touring_hooks_shared::qa_syntax;

// ── Idempotency gate (A.3 — format(format(x)) == format(x)) ────────────
pub use touring_hooks_shared::idempotency;

// ── Shadow workspace (advanced, feature-gated) ─────────────────────────
#[cfg(feature = "shadow-workspace")]
pub use touring_hooks_core::shadow_v2;

// ── NLP enrichment bridge (feature-gated) ───────────────────────────────
#[cfg(feature = "nlp-enrichment")]
pub use touring_hooks_core::nlp_bridge;

// Integration tests — ACO bridge + HookRuntime end-to-end
#[cfg(test)]
mod integration_tests;

// post_tool_rl integration tests live in touring-hook-handlers (Wave H).

// ── Re-exports — always available ──────────────────────────────────────
pub use aco_bridge::{
    HookEventBuffer, HookEventBufferError, HookOutcome, HookQualityAssessment, HookResultCache,
    StreamingHookStats,
};
pub use ast_bridge::{
    EditImpactResult, FileQualityMetrics, analyze_file_quality,
    build_enriched_knowledge_with_quality, check_symbol_complexity, extract_enriched_symbols,
    quality_summary, validate_edit_impact,
};
pub use branch_fs::{BranchFs, BranchFsError, BranchMetadata, SnapshotEntry};
pub use circuit_state_machine::{CircuitCheck, CircuitState, ClassBreaker, GlobalState, OpClass};
pub use classifier::{
    CILALevel, CILAResult, CachedIntentClassifier, CognitiveTechnique, IntentClassifier,
};
pub use errors::{ErrorContext, Result, TouringError};
pub use knowledge::{
    BashOutcome, EditEvent, FileKnowledge, FileKnowledgeDB, FileKnowledgeEnriched, FileRelation,
    Gotcha, KnowledgeStats, ThreadSafeKnowledgeDB, WeightedErrorPattern,
};
pub use metrics::RuntimeMetrics;
pub use pii::{PIIFinding, PIIScanner, PIIType};
pub use runtime::{HookResponse, HookRuntime, HookTimer, make_relative};
pub use shared::async_runtime::{
    AsyncConfig, AsyncRuntimeCheck, TokioRuntime, assert_no_leaked_tasks,
};

// ── Re-exports — feature-gated ─────────────────────────────────────────
#[cfg(feature = "session-hooks")]
pub use session_insights::{
    RlConvergenceInsight, SessionInsights, SessionTrend, ToolEffectivenessInsight, compute_trend,
    extract_evolution_insights, extract_session_insights,
};

#[cfg(feature = "nlp-enrichment")]
pub use nlp_bridge::{
    chunk_for_knowledge, extract_keywords, extract_monetary_values, has_regulatory_content,
    has_technical_content, keyword_category_counts,
};

// Phase C carve (2026-06-10): the `current_uid()` FFI helper moved to
// touring-hooks-core (every caller was a carved engine module).
