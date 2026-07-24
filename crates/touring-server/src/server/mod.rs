//! MCP Server implementation for Touring
//!
//! Architecture (v9.1 modular):
//!   - `params.rs`  — 32 parameter structs (JsonSchema for auto inputSchema)
//!   - `mod.rs`     — TouringServer struct, init, 32 `#[tool]` methods, ServerHandler
//!
//! Implements 32 tools via rmcp SDK macros (`#[tool_router]` + `#[tool]`).

mod metrics;
pub mod params;

pub use metrics::AnalysisServerMetrics;

use std::fmt::Debug;
use std::path::PathBuf;
use std::sync::Arc;

use rmcp::{
    ErrorData as McpError, ServerHandler, handler::server::tool::ToolRouter,
    handler::server::wrapper::Parameters, model::*, tool, tool_router,
};
use tokio::sync::{Mutex, RwLock};
use tracing::{info, warn};

use crate::hooks::{CognitiveTechnique, IntentClassifier, PIIScanner};
use crate::ingest::TranscriptMiner;
use crate::ingest::watcher::{JsonlWatcher, WatcherConfig, discover_jsonl_paths};
use crate::memory_store::{MemoryEntry, MemoryQuery, MemoryStore};
use crate::reasoning::{CheckpointManager, TaskDecomposer};
use crate::session::SessionManager;
use touring_code::ast::graph::SymbolIndex;
use touring_code::ast::store::SymbolStore;
use touring_foundation::TouringConfig;
use touring_intelligence::index::FileWatcherBuilder;
use touring_intelligence::rl::aco::UnifiedPheromoneBus;
use touring_intelligence::rl::evolution::{
    Axis, EvolutionAnalyzer, InsightEngine, LearningPersistence, Severity,
};
use touring_intelligence::rl::memory::rlm::RlmMemory;
use touring_intelligence::rl::online_rl::{ImmediateReward, OnlineRLEngine};
use touring_intelligence::rl::{
    DriftDetector, LinUCBBandit, QLearning, QTable, SkillClusterer, WilsonRanker,
};
use touring_storage::embedding::{Embedder, GpuEmbedder};
// Re-export PheroKey so external consumers (touring-cognitive bridges,
// telemetry sinks) can build phero keys without importing touring-learning
// directly. Wires PheroKey into the public surface — REGRA #0 potencializar.
pub use touring_intelligence::rl::aco::PheroKey;

use params::*;

// ── RL state/action mapping — single source of truth in rl_mapping.rs ──
use crate::rl_mapping::{
    event_to_state as event_type_to_state, tool_to_action as tool_name_to_action,
};
use crate::tools::drift;

// ── WASM Plugin System ────────────────────────────────────────────────
#[cfg(feature = "wasm-plugins")]
use crate::plugins::WasmPluginRunner;
use touring_bindings::wasm::MAX_FUEL;

// ── C2 (coupling backlog) — curated default MCP tool surface ────────────────
//
// `list_tools` returns only the curated allowlist below (~22 high-value tools)
// so the MCP handshake ships ~22 schemas instead of ~160 (Anthropic "Tool
// Search": fewer, well-chosen tools → better selection + ~86% fewer schema
// tokens). Hidden tools stay registered and CALLABLE, and are discoverable on
// demand via `touring_search` (C3). Set `TOURING_MCP_ALL_TOOLS=1` to list every
// tool (runtime toggle — no rebuild). The list is a plain array: edit to taste.

/// Tool names exposed by `list_tools` in the curated (default) MCP surface.
const CURATED_TOOLS: &[&str] = &[
    // Discovery + compute-in-code — the two entry points.
    "touring_search",
    "touring_ctx_execute",
    // AST + symbols.
    "touring_ast_overview",
    "touring_ast_find",
    "touring_ast_edit",
    "touring_ast_meta",
    "touring_find_references",
    // Memory + intent.
    "touring_memory_store",
    "touring_memory_recall",
    "touring_classify_intent",
    // Index + full-text search.
    "touring_index_status",
    "touring_tantivy_search",
    "touring_tantivy_fuzzy",
    // Health + wiring + change intelligence.
    "touring_health",
    "touring_minimal_context",
    "touring_wiring",
    "touring_wiring_audit",
    "touring_gotcha",
    "touring_detect_changes",
    "touring_blast_radius_analysis",
    // Master workflow tools — orchestrate multiple engines in one call.
    "touring_audit",
    // Planning.
    "touring_decompose",
    "touring_generator_submit_plan",
];

/// Whether `name` is in the curated default MCP surface ([`CURATED_TOOLS`]).
fn is_curated(name: &str) -> bool {
    CURATED_TOOLS.contains(&name)
}

/// Apply the curated allowlist to a full tool list. Returns the list unchanged
/// when `TOURING_MCP_ALL_TOOLS` is set (any value) — the runtime escape hatch;
/// otherwise keeps only [`CURATED_TOOLS`] (input order preserved). A curated
/// name absent from `all` is simply skipped, so the filter is rename-resilient.
fn apply_curation(all: Vec<rmcp::model::Tool>) -> Vec<rmcp::model::Tool> {
    if std::env::var_os("TOURING_MCP_ALL_TOOLS").is_some() {
        return all;
    }
    all.into_iter()
        .filter(|t| is_curated(t.name.as_ref()))
        .collect()
}

#[cfg(test)]
mod curation_tests {
    use super::{CURATED_TOOLS, is_curated};

    #[test]
    fn curated_surface_is_lean_with_entry_points() {
        assert!(
            (18..=26).contains(&CURATED_TOOLS.len()),
            "curated set should be ~22, was {}",
            CURATED_TOOLS.len()
        );
        // The two progressive-disclosure entry points must always be listed.
        assert!(is_curated("touring_search"));
        assert!(is_curated("touring_ctx_execute"));
        // A few more essentials.
        assert!(is_curated("touring_memory_recall"));
        assert!(is_curated("touring_wiring"));
        // Hidden/legacy tools are not in the default surface.
        assert!(!is_curated("touring_ctx_smart"));
        assert!(!is_curated("touring_evolution_drift"));
        assert!(!is_curated("nonexistent_tool"));
    }

    #[test]
    fn curated_names_are_unique() {
        let mut sorted = CURATED_TOOLS.to_vec();
        sorted.sort_unstable();
        let len = sorted.len();
        sorted.dedup();
        assert_eq!(len, sorted.len(), "CURATED_TOOLS has duplicates");
    }
}

// ============================================================================
// Initialization helpers — extracted to reduce CC of TouringServer::new()
// ============================================================================

/// Initialize classifier and PII scanner (independent, no I/O).
fn init_classifiers() -> (Arc<IntentClassifier>, Arc<PIIScanner>) {
    let classifier = Arc::new(IntentClassifier::new());
    let pii_scanner = Arc::new(PIIScanner::new());
    (classifier, pii_scanner)
}

/// Initialize embedder from config (optional GPU service).
fn init_embedder(config: &TouringConfig) -> Option<Arc<GpuEmbedder>> {
    if !config.auto_embed {
        info!("Auto-embedding disabled by config");
        return None;
    }
    let client = GpuEmbedder::new(&config.gpu_service_url, config.embedding_dim);
    info!(
        "GpuEmbedder configured: url={}, dim={}",
        config.gpu_service_url, config.embedding_dim
    );
    Some(Arc::new(client))
}

/// Initialize memory store with optional embedder.
/// Wrapped in Mutex because rusqlite Connection is not Sync.
fn init_memory_store(
    rlm_path: &std::path::Path,
    semantic_path: &std::path::Path,
    embedder: Option<Arc<GpuEmbedder>>,
) -> Option<Arc<Mutex<MemoryStore>>> {
    let store = match MemoryStore::new(rlm_path, semantic_path) {
        Ok(s) => s,
        Err(e) => {
            warn!("MemoryStore init failed (degraded mode): {}", e);
            return None;
        }
    };
    let store = if let Some(ref emb) = embedder {
        store.with_embedder(Arc::clone(emb))
    } else {
        store
    };
    info!(
        "MemoryStore initialized: rlm={}, semantic={}, embedder={}",
        rlm_path.display(),
        semantic_path.display(),
        if embedder.is_some() { "active" } else { "none" }
    );
    Some(Arc::new(Mutex::new(store)))
}

/// Load persisted learning state (WilsonRanker, DriftDetector, QTable).
fn init_learning_state(
    db_path: &std::path::Path,
    qtable: &mut QTable,
) -> (WilsonRanker, DriftDetector) {
    let persistence = LearningPersistence::new(db_path);
    if let Err(e) = persistence.ensure_tables() {
        warn!("Failed to create learning tables: {}", e);
    }

    let mut ranker = WilsonRanker::new();
    let mut drift = DriftDetector::new();

    match persistence.load_wilson(&mut ranker) {
        Ok(n) => info!("Loaded {} Wilson ranking items from persistence", n),
        Err(e) => warn!("Failed to load Wilson state: {}", e),
    }

    match persistence.load_drift(&mut drift) {
        Ok(n) => info!("Loaded {} drift metrics from persistence", n),
        Err(e) => warn!("Failed to load drift state: {}", e),
    }

    match persistence.load_qtable(qtable) {
        Ok(n) => info!("Loaded {} QTable entries from persistence", n),
        Err(e) => warn!("Failed to load QTable state: {}", e),
    }

    (ranker, drift)
}

/// Initialize SymbolStore and load symbols into the graph index.
fn init_symbol_store(
    db_path: &std::path::Path,
    graph_index: &mut SymbolIndex,
) -> Option<Arc<Mutex<SymbolStore>>> {
    match SymbolStore::new(db_path) {
        Ok(store) => {
            match store.load_into_index(graph_index) {
                // S-03 warm self-check: a populated DB that loads 0 symbols is a
                // cold-index symptom (stale/empty symbols.db). Surface it loudly so
                // a `touring status` symbol_count=0 is diagnosable at the source
                // rather than silently degrading every downstream index query.
                Ok(0) => warn!(
                    "SymbolStore opened but loaded 0 symbols from {} — index is COLD; \
                     run `touring index rebuild` if this persists",
                    db_path.display()
                ),
                Ok(n) => info!(
                    "SymbolStore opened: loaded {} symbols from {} (index warm-on-start)",
                    n,
                    db_path.display()
                ),
                Err(e) => warn!("SymbolStore load failed (starting empty): {}", e),
            }
            Some(Arc::new(Mutex::new(store)))
        }
        Err(e) => {
            warn!("SymbolStore init failed (in-memory only): {}", e);
            None
        }
    }
}

/// Merge symbols from all other projects into the unified index.
fn merge_cross_project_symbols(project_root: &std::path::Path, graph_index: &mut SymbolIndex) {
    let Some(home) = std::env::var_os("HOME") else {
        return;
    };
    let projects_dir = PathBuf::from(home).join(".claude").join("projects");
    let Some(entries) = std::fs::read_dir(&projects_dir).ok() else {
        return;
    };

    for entry in entries.flatten() {
        let project_path = entry.path();
        if project_path == project_root {
            continue; // already loaded above
        }
        let symbols_db = project_path
            .join(".claude")
            .join("touring")
            .join("symbols.db");
        if !symbols_db.exists() {
            continue;
        }
        if let Ok(store) = SymbolStore::new(&symbols_db) {
            if let Ok(n) = store.load_into_index(graph_index) {
                info!(
                    "Cross-project loaded: {} symbols from {}",
                    n,
                    symbols_db.display()
                );
            }
        }
    }
}

// ============================================================================
// TouringServer -- the rmcp service
// ============================================================================

/// Main Touring MCP Server with real module integrations.
///
/// All fields use `Arc` for `Clone` and `Send+Sync`.
/// MemoryStore wraps rusqlite (not Sync), so it's behind `Arc<Mutex<..>>`.
///
/// Some fields are intentionally read only inside `#[tool]` handlers via
/// `&self` — under non-default feature combinations a handler may be cfg'd
/// out, leaving its field formally "dead". The dead_code allowance below
/// is scoped to that contract; do NOT add new dead fields under it.
#[derive(Clone)]
#[allow(dead_code, reason = "fields consumed via cfg-gated #[tool] handlers")]
pub struct TouringServer {
    config: TouringConfig,
    classifier: Arc<IntentClassifier>,
    pii_scanner: Arc<PIIScanner>,
    memory: Option<Arc<Mutex<MemoryStore>>>,
    embedder: Option<Arc<GpuEmbedder>>,
    qtable: Arc<Mutex<QTable>>,
    clusterer: Arc<Mutex<SkillClusterer>>,
    // New state fields for 6 new tools
    graph_svc: Arc<crate::graph_service::GraphService>,
    symbol_store: Option<Arc<Mutex<SymbolStore>>>,
    decomposer: Arc<RwLock<TaskDecomposer>>,
    checkpoint_manager: Arc<Mutex<CheckpointManager>>,
    session_manager: Arc<Mutex<SessionManager>>,
    ranker: Arc<Mutex<WilsonRanker>>,
    drift_detector: Arc<Mutex<DriftDetector>>,
    linucb: Arc<Mutex<LinUCBBandit>>,
    online_rl: Arc<Mutex<OnlineRLEngine>>,
    nexus: Arc<touring_intelligence::reasoning::CognitiveNexus>,
    /// Session-aware hint engine for dynamic tool suggestions (v32 S1).
    hint_engine: Arc<Mutex<crate::tools::session_hints::SessionHintEngine>>,
    /// Shared registry tracking in-flight plan executors (PLN2 S3).
    plan_registry: touring_generator::SharedPlanRegistry,
    /// W4-3: Shared ACO pheromone bus for decomposer event injection.
    aco_bus: Arc<UnifiedPheromoneBus>,
    #[cfg(feature = "ebpf-telemetry")]
    telemetry: Arc<Mutex<Option<touring_foundation::telemetry::TelemetryCollector>>>,
    #[cfg(not(feature = "ebpf-telemetry"))]
    _telemetry_placeholder: core::marker::PhantomData<()>,
    tool_router: ToolRouter<Self>,
}

impl Debug for TouringServer {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("TouringServer").finish()
    }
}

#[tool_router]
impl TouringServer {
    /// Create a new server instance with all modules initialized.
    pub fn new() -> touring_foundation::Result<Self> {
        let config = TouringConfig::load()?;
        config.ensure_dirs()?;

        // Phase 1: Classifiers (fast, no I/O)
        let (classifier, pii_scanner) = init_classifiers();

        // Phase 2: Embedder and memory (may contact GPU service)
        // Use consolidated memory.db path instead of legacy rlm/semantic paths.
        let memory_db = TouringConfig::memory_db_canonical(&config.project_root);
        let embedder = init_embedder(&config);
        let memory = init_memory_store(&memory_db, &memory_db, embedder.clone());

        // Phase 3: QTable and clusterer
        let mut qtable_inner = QTable::new();
        let clusterer = Arc::new(Mutex::new(SkillClusterer::new()));

        // Phase 4: Symbol store and graph index
        let mut graph_index_inner = SymbolIndex::new();
        let symbol_store = init_symbol_store(&config.symbols_db_path, &mut graph_index_inner);

        // H3: async-pipeline — verify stream subscription API is accessible.
        // Each consumer calls subscribe_stream() independently to get their own broadcast Receiver.
        if let Some(ref store_arc) = symbol_store {
            if let Ok(store) = store_arc.try_lock() {
                let _ = store.subscribe_stream();
                info!("SymbolStore async-pipeline stream ready");
            }
        }

        // Phase 5: Cross-project symbol merging
        merge_cross_project_symbols(&config.project_root, &mut graph_index_inner);

        let graph_index = Arc::new(Mutex::new(graph_index_inner));

        // GS-EC11: Initialize AsyncFileKnowledgeDB before GraphService so it can be wired in.
        // Uses the canonical knowledge_db path — same file used by HookRuntime (WAL-shared).
        let knowledge_db = TouringConfig::knowledge_db_canonical(&config.project_root);
        let graph_svc = {
            let mut svc = crate::graph_service::GraphService::new(
                Arc::clone(&graph_index),
                config.project_root.clone(),
            );
            match touring_hooks::async_knowledge::AsyncFileKnowledgeDB::new(&knowledge_db) {
                Ok(adb) => {
                    svc = svc.with_async_knowledge(adb);
                    tracing::info!("GraphService: async_knowledge wired (co-edit signal active)");
                }
                _ => {
                    tracing::warn!(
                        "GraphService: async_knowledge init failed — coedit_files will be empty"
                    );
                }
            }
            Arc::new(svc)
        };
        let decomposer = Arc::new(RwLock::new(TaskDecomposer::new()));
        // W4-3: Shared ACO pheromone bus for decomposer event injection
        let aco_bus = Arc::new(UnifiedPheromoneBus::new(0.05));
        // Phase 5b: CheckpointManager for TaskDecomposer persistence
        let checkpoint_manager = match CheckpointManager::new(&knowledge_db) {
            Ok(cm) => Arc::new(Mutex::new(cm)),
            Err(e) => {
                tracing::warn!("CheckpointManager init failed (non-fatal): {}", e);
                Arc::new(Mutex::new(
                    CheckpointManager::new(&PathBuf::from(".touring_checkpoints.db"))
                        .expect("fallback checkpoint DB must init"),
                ))
            }
        };
        let session_manager = Arc::new(Mutex::new(SessionManager::new()));

        // Phase 6: Persisted learning state (consolidated graph.db)
        let graph_db = TouringConfig::graph_db_canonical(&config.project_root);
        let (ranker_inner, drift_inner) = init_learning_state(&graph_db, &mut qtable_inner);
        let qtable = Arc::new(Mutex::new(qtable_inner));
        let ranker = Arc::new(Mutex::new(ranker_inner));
        let drift_detector = Arc::new(Mutex::new(drift_inner));

        // Online RL: immediate per-tool reward processing (complements 300s batch auto_learn)
        let linucb = Arc::new(Mutex::new(LinUCBBandit::new()));
        let online_rl = Arc::new(Mutex::new(OnlineRLEngine::with_defaults()));
        info!("OnlineRLEngine + LinUCBBandit initialized for immediate reward processing");

        // Phase 7: eBPF telemetry (graceful degradation on unsupported kernels).
        //
        // Previously used `Handle::current().block_on(async { ... })` directly
        // inside the `#[tokio::main]` runtime, which panics with "Cannot start
        // a runtime from within a runtime" (this is a synchronous constructor
        // called from async context). `block_in_place` parks the current
        // worker so `block_on` can drive a nested future safely on the
        // multi-thread runtime.
        #[cfg(feature = "ebpf-telemetry")]
        let telemetry: Arc<
            Mutex<Option<touring_foundation::telemetry::TelemetryCollector>>,
        > = {
            let telemetry_config = touring_foundation::telemetry::TelemetryConfig::default();
            let result = tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current().block_on(async {
                    touring_foundation::telemetry::TelemetryCollector::new(telemetry_config).await
                })
            });
            match result {
                Ok(collector) => {
                    info!("TelemetryCollector initialized (eBPF active)");
                    Arc::new(Mutex::new(Some(collector)))
                }
                Err(e) => {
                    tracing::warn!(
                        "TelemetryCollector init failed (graceful degradation): {}",
                        e
                    );
                    Arc::new(Mutex::new(None))
                }
            }
        };

        info!(
            "TouringServer created -- classifier: {} patterns, pii: {} patterns, memory: {}, embedder: {}",
            classifier.pattern_count(),
            pii_scanner.pii_pattern_count(),
            if memory.is_some() {
                "active"
            } else {
                "degraded"
            },
            if embedder.is_some() { "active" } else { "none" }
        );

        Ok(Self {
            config,
            classifier,
            pii_scanner,
            memory,
            embedder,
            qtable,
            clusterer,
            graph_svc,
            symbol_store,
            decomposer,
            checkpoint_manager,
            session_manager,
            ranker,
            drift_detector,
            linucb,
            online_rl,
            nexus: Arc::new(touring_intelligence::reasoning::CognitiveNexus::default()),
            hint_engine: Arc::new(Mutex::new(
                crate::tools::session_hints::SessionHintEngine::new(),
            )),
            plan_registry: Arc::new(touring_generator::PlanRegistry::new()),
            aco_bus,
            #[cfg(feature = "ebpf-telemetry")]
            telemetry,
            #[cfg(not(feature = "ebpf-telemetry"))]
            _telemetry_placeholder: core::marker::PhantomData,
            tool_router: {
                // FIX 2026-05-23 (gotcha:mcp-tools-empty-tool-router-split-impl):
                // Combine 10 sub-routers split across tools_*.rs into one ToolRouter.
                // Each tools_*.rs decorates its impl TouringServer with
                // `#[tool_router(router = router_X, vis = pub(crate))]` and we
                // merge them here so all 159 #[tool(...)] become visible at runtime
                // (rmcp 1.5 canonical pattern — ToolRouter::merge).
                let mut tr = Self::tool_router();
                tr.merge(Self::router_activity());
                tr.merge(Self::router_analysis());
                tr.merge(Self::router_analysis_ext());
                tr.merge(Self::router_context_router());
                tr.merge(Self::router_core());
                tr.merge(Self::router_ctx_execute());
                tr.merge(Self::router_generator());
                tr.merge(Self::router_infra());
                tr.merge(Self::router_infra_ext());
                tr.merge(Self::router_metadata());
                tr.merge(Self::router_quality_signal());
                tr.merge(Self::router_tantivy());
                tr.merge(Self::router_search()); // C3 — touring_search meta-tool
                tr.merge(Self::router_workflow()); // master tools — touring_audit (offensive+quality)
                // W2 (task_1780763041476850005) — gated behind mcp-curated.
                // Each W2 tool has its own #[tool_router] macro (one per
                // impl block): router_tdg, router_hook_metrics,
                // router_cortex_classify. W1.2 FamilyRouter is a skeleton
                // only (StatusFamily enum + StatusInput struct); no
                // #[tool_router] needed until W2.5 adds the actual
                // touring_status tool.
                #[cfg(feature = "mcp-curated")]
                {
                    tr.merge(Self::router_tdg());
                    tr.merge(Self::router_hook_metrics());
                    tr.merge(Self::router_cortex_classify());
                }
                tr
            },
        })
    }
}

// ── Tool method bodies — split across sub-modules for maintainability ──
// Each module contains `impl TouringServer { #[tool] ... }` blocks.
// rmcp discovers #[tool] methods across multiple impl blocks.
pub(crate) mod tools_activity;
pub(crate) mod tools_analysis;
mod tools_analysis_evolution;
mod tools_context_router;
pub(crate) mod tools_core;
mod tools_ctx_execute;
mod tools_generator;
mod tools_infra;
mod tools_infra_ext;
mod tools_metadata;
// Wave 2 P5 (Sentrux master plan, 2026-05-09) — 3 quality-signal MCP tools.
pub(crate) mod tools_quality_signal;
// C3 (coupling backlog) — `touring_search` meta-tool: always-on intent-ranked
// tool discovery (progressive disclosure) so the LLM finds the right tool
// without loading every schema upfront.
mod tools_search;
mod tools_tantivy;
// Master workflow tools (coupling backlog) — orchestrators that fan one MCP
// call across several detection engines. `touring_audit` = offensive CWE/OWASP
// engine + 6 P0 BLOCK quality dims → one ranked failure/gap report.
pub(crate) mod tools_workflow; // R3 — run_audit engine reused by the `touring audit` CLI adapter
// W1.2 (task_1780763041476850005) — FamilyRouter consolidating 9 *_status
// tools into 1 touring_status MCP tool with enum family. Opt-in via
// --features mcp-curated during the 30-day migration window.
// `pub` so the documented curated `touring_status` contract (StatusFamily +
// StatusInput) is reachable public API until the W2.5 `#[tool]` router
// consumes it internally — keeps the tested skeleton wired, not dead.
#[cfg(feature = "mcp-curated")]
pub mod tools_status;
// W2 (task_1780763041476850005) — 3 new MCP tools (tdg, hook_metrics,
// cortex_classify) that fill gaps identified in FASE 1 SCOUT. Opt-in
// via --features mcp-curated. Includes 5 unit tests.
#[cfg(feature = "mcp-curated")]
mod tools_new;

// ============================================================================
// ServerHandler implementation (rmcp integration)
// ============================================================================

// Manual ServerHandler impl (replaces #[tool_handler] macro) so we can add
// opt-in pagination to `list_tools` per MCP spec ("Listing Tools — supports
// pagination"). The 3 macro-generated methods (call_tool, list_tools, get_tool)
// are reproduced verbatim from rmcp-macros 1.2 except for the pagination logic
// added to list_tools. Default behavior (TOURING_TOOLS_LIST_PAGE_SIZE unset or 0)
// is byte-identical to the previous macro output — zero regression.
impl ServerHandler for TouringServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(
            ServerCapabilities::builder()
                .enable_tools()
                .build(),
        )
        .with_server_info(Implementation::new("touring", env!("CARGO_PKG_VERSION")))
        .with_instructions(concat!(
            "Touring MCP: ≈22 curated tools for code intelligence, memory, learning, and self-improvement. ",
            "Call touring_search(intent) to discover more tools on demand; set TOURING_MCP_ALL_TOOLS=1 to list all ≈160. ",
            "TOKEN EFFICIENCY RULES (v31) — ",
            "1. ALWAYS call touring_minimal_context FIRST with a task description (~100 tokens). ",
            "2. Use detail_level='minimal' on ALL tool calls unless minimal output is insufficient. ",
            "3. Only escalate to 'standard' or 'full' for specific entities needing deeper inspection. ",
            "4. Every response includes _next_tools suggestions — follow them for optimal workflow. ",
            "5. For change review: touring_detect_changes → expand only high-risk items. ",
            "TOOL SELECTION GUIDE — ",
            "READ-ONLY queries (symbol lookup, memory search, wiring check): ",
            "  prefer Bash `touring index find <symbol>` or `touring memory recall '<query>'` (<10ms) ",
            "  over MCP tools (~200ms) when scripting or chaining results. ",
            "WRITE / COMPLEX ops (store memory, start session, decompose task, speculate): ",
            "  always use MCP tools — CLI does not support write operations. ",
            "CATEGORIES — ",
            "  touring_minimal_context: ALWAYS call first (~100 tokens entry point); ",
            "  touring_detect_changes: risk-scored change impact (blast+wiring+gotchas); ",
            "  ast_overview/find/edit: code navigation and surgical edits; ",
            "  memory_store/recall: persist and retrieve lessons, schemas, patterns; ",
            "  suggest/learn_pattern/cluster_skills: RL-backed next-action guidance; ",
            "  graph/decompose/session: project structure and task planning; ",
            "  evolve/insights/evolution_status: self-improvement after sessions; ",
            "  speculate: ALWAYS run before Write/Edit on existing files; ",
            "  wiring/wiring_audit: orphan detection and module integration health; ",
            "  gotcha: look up known pitfalls before editing a file; ",
            "  analysis_report: unified deep code health analysis.",
        ))
    }

    // ── call_tool — reproduced from rmcp-macros 1.2 tool_handler.rs verbatim ──
    async fn call_tool(
        &self,
        request: rmcp::model::CallToolRequestParams,
        context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> Result<rmcp::model::CallToolResult, rmcp::ErrorData> {
        let tcc = rmcp::handler::server::tool::ToolCallContext::new(self, request, context);
        self.tool_router.call(tcc).await
    }

    // ── list_tools — MCP-spec opt-in cursor pagination ──────────────────────
    //
    // Default behavior (TOURING_TOOLS_LIST_PAGE_SIZE unset or 0): returns the
    // full tool list with next_cursor=None — byte-identical to the previous
    // #[tool_handler] macro output. Clients without pagination support see no
    // change.
    //
    // Opt-in: setting TOURING_TOOLS_LIST_PAGE_SIZE=N (N>0) chunks the response
    // into pages of N tools. The cursor is an opaque ASCII-decimal offset into
    // the canonical list_all() ordering. Clients follow next_cursor until it
    // is None. This is the canonical MCP "Listing Tools — Pagination" pattern
    // (modelcontextprotocol.org/specification/2025-11-25/server/tools).
    async fn list_tools(
        &self,
        request: Option<rmcp::model::PaginatedRequestParams>,
        _context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> Result<rmcp::model::ListToolsResult, rmcp::ErrorData> {
        // C2 — curate the default surface to ~22 tools (TOURING_MCP_ALL_TOOLS
        // lists all). `call_tool` is unfiltered, so hidden tools stay callable.
        let all = apply_curation(self.tool_router.list_all());

        let page_size = std::env::var("TOURING_TOOLS_LIST_PAGE_SIZE")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .filter(|&n| n > 0);

        let Some(page_size) = page_size else {
            return Ok(rmcp::model::ListToolsResult {
                tools: all,
                meta: None,
                next_cursor: None,
            });
        };

        let offset: usize = request
            .as_ref()
            .and_then(|p| p.cursor.as_deref())
            .and_then(|c| c.parse().ok())
            .unwrap_or(0);

        let tools = if offset >= all.len() {
            Vec::new()
        } else {
            let end = (offset + page_size).min(all.len());
            all[offset..end].to_vec()
        };
        let next_offset = offset.saturating_add(page_size);
        let next_cursor = if next_offset < all.len() {
            Some(next_offset.to_string())
        } else {
            None
        };

        Ok(rmcp::model::ListToolsResult {
            tools,
            meta: None,
            next_cursor,
        })
    }

    // ── get_tool — reproduced from rmcp-macros 1.2 tool_handler.rs verbatim ──
    fn get_tool(&self, name: &str) -> Option<rmcp::model::Tool> {
        self.tool_router.get(name).cloned()
    }
}

impl TouringServer {
    // ── Services Diagnostic — non-#[tool] helper that exercises the
    // `pub(crate)` accessor surface (`graph_svc.index`, `graph_svc.inject`,
    // `AgentDiaryBuilder`, `MemoryEntry::with_embedding`). This lives
    // outside the `#[tool_router]` macro so cargo can see the call sites
    // (REGRA #0 — wire orphan pub symbols instead of deleting them).

    /// Capture a JSON snapshot of internal service-layer state. Used by
    /// `touring diagnose services` and by audit-trail collectors. Wires
    /// four otherwise-orphan helpers into a single non-macro caller.
    pub async fn services_diagnostic(&self) -> serde_json::Value {
        // 1) graph_service.index() — verifies the Arc is alive
        let idx_arc = self.graph_svc.index();
        let idx_strong_count = std::sync::Arc::strong_count(idx_arc);

        // 2) graph_service.inject() — confirms inject path works with a
        //    synthetic empty context (no daemon round-trip required).
        let mut output = serde_json::json!({});
        let synthetic_ctx = crate::graph_service::GraphFocusCtx::default();
        self.graph_svc.inject(&mut output, &synthetic_ctx);

        // 3) AgentDiaryBuilder — exercise the builder so the unused
        //    associated items (`new`, `agent_name`, `build`) become reachable.
        let diary_ok = crate::agent_diary::AgentDiaryBuilder::new()
            .agent_name("services-diagnostic")
            .build()
            .is_ok();

        // 4) MemoryEntry::with_embedding — small synthetic embedding to
        //    exercise the builder method without persisting anything.
        let entry =
            crate::memory_store::MemoryEntry::new("services-diagnostic", "ephemeral", "value")
                .with_embedding(vec![0.0_f32; 4]);
        let entry_has_embedding = entry.embedding.is_some();

        // 5) DecomposeValidationMetrics::diagnostic_exercise — wires
        //    `new` + `record_validation` (private fns) by going through
        //    the in-module helper.
        let metrics_diag =
            crate::reasoning::decomposer::DecomposeValidationMetrics::diagnostic_exercise();

        // 6) SubTask::diagnostic_lifecycle — wires `mark_in_progress`,
        //    `is_timed_out`, `duration_ms` and the `started_at` field
        //    via a synthetic single-attempt lifecycle.
        let subtask_diag = crate::reasoning::decomposer::SubTask::diagnostic_lifecycle();

        // 7) TaskDecomposer::diagnostic_lifecycle — wires `new`,
        //    `create_task`, `create_task_with_cila`, `take_pending_aco_events`
        //    plus the `next_id` and `pending_aco_events` fields.
        let decomposer_diag = crate::reasoning::decomposer::TaskDecomposer::diagnostic_lifecycle();

        // 8) SessionManager::diagnostic_lifecycle — wires the full
        //    session lifecycle (`start_session`, `checkpoint`,
        //    `update_metric`, `get_session`, `list_sessions`,
        //    `end_session`, `abandon_all_active`).
        let session_diag = crate::session::manager::SessionManager::diagnostic_lifecycle();

        serde_json::json!({
            "graph_index_strong_count": idx_strong_count,
            "graph_inject_synthetic": output.get("graph_ctx").is_some(),
            "agent_diary_builder_ok": diary_ok,
            "memory_entry_with_embedding": entry_has_embedding,
            "decomposer_validation_metrics": metrics_diag,
            "decomposer_subtask_lifecycle": subtask_diag,
            "decomposer_task_lifecycle": decomposer_diag,
            "session_manager_lifecycle": session_diag,
        })
    }

    // ── Analysis Report (touring-analysis v0.2.0) ───────────────────────

    /// Run unified deep code analysis via touring-analysis pipeline.
    pub async fn analysis_report_impl(&self, depth_str: &str) -> String {
        let depth = touring_analysis::Depth::from_str_lossy(depth_str);
        let config = depth.to_config();

        let knowledge_db_path =
            touring_foundation::TouringConfig::knowledge_db_canonical(&self.config.project_root);
        if !knowledge_db_path.exists() {
            return r#"{"error": "knowledge.db not found"}"#.to_string();
        }

        let graph_db_path =
            touring_foundation::TouringConfig::graph_db_canonical(&self.config.project_root);
        let project_root = self.config.project_root.to_string_lossy().to_string();

        // Run the entire pipeline (rusqlite::Connection is !Send) on the blocking
        // thread pool. Only owned Send values are returned across the await boundary.
        type PipelineResult = Result<(f64, u64, bool, String, String), String>;
        let result: PipelineResult = tokio::task::spawn_blocking(move || {
            let knowledge_conn = rusqlite::Connection::open_with_flags(
                &knowledge_db_path,
                rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY
                    | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
            )
            .map_err(|e| format!(r#"{{"error": "knowledge.db: {e}"}}"#))?;
            let _ = knowledge_conn.execute_batch("PRAGMA busy_timeout = 1000;");

            let graph_conn = rusqlite::Connection::open_with_flags(
                &graph_db_path,
                rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY
                    | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
            )
            .ok();

            let mut builder =
                touring_analysis::pipeline::AnalysisPipelineBuilder::new(&knowledge_conn)
                    .config(config);
            if let Some(ref gc) = graph_conn {
                builder = builder.graph_conn(gc);
            }

            let cached_pipeline = touring_analysis::CachedAnalysisPipeline::new(builder.build());
            let report = cached_pipeline.run_cached(&project_root, depth);

            let composite_score = report.composite_score;
            let first_duration = report
                .dimensions
                .first()
                .map(|d| d.duration_ms)
                .unwrap_or(0);
            let accepted = composite_score >= 0.5;
            let report_json_str = report.to_json_pretty();
            let dashboard_json =
                serde_json::to_string(&serde_json::json!({"dashboard": report.to_dashboard()}))
                    .unwrap_or_default();

            Ok((
                composite_score,
                first_duration,
                accepted,
                report_json_str,
                dashboard_json,
            ))
        })
        .await
        .unwrap_or_else(|e| Err(format!(r#"{{"error": "spawn_blocking panicked: {e}"}}"#)));

        let (composite_score, first_duration, accepted, report_json_str, dashboard_json) =
            match result {
                Ok(v) => v,
                Err(e) => return e,
            };

        // F2: Record Prometheus-compatible metrics after each analysis run.
        {
            let m = AnalysisServerMetrics::global();
            m.inc_analysis_run();
            m.add_duration_ms(first_duration);
            // CachedAnalysisPipeline is freshly created per request → always a cache miss.
            // When a session-level pipeline is introduced this becomes hit/miss tracking.
            m.inc_cache_miss();
        }

        // D7: Feed composite_score as RL reward — quality_score drives process_reward EMA.
        // rusqlite::Connection is fully dropped before this .await point.
        let immediate = ImmediateReward {
            tool_name: "analysis_quality".to_string(),
            accepted,
            latency_ms: first_duration,
            error_count: if accepted { 0 } else { 1 },
            cila_level: 2,
            file_type: 1, // rust
            quality_score: Some(composite_score),
        };
        {
            let mut rl = self.online_rl.lock().await;
            let mut qt = self.qtable.lock().await;
            let mut bandit = self.linucb.lock().await;
            let _ = rl.process_reward(&immediate, &mut qt, &mut bandit);
        }

        let report_json_val: serde_json::Value =
            serde_json::from_str(&report_json_str).unwrap_or_default();
        let dashboard_val: serde_json::Value = serde_json::from_str(&dashboard_json)
            .ok()
            .and_then(|v: serde_json::Value| v.get("dashboard").cloned())
            .unwrap_or_default();

        serde_json::to_string_pretty(&serde_json::json!({
            "report": report_json_val,
            "dashboard": dashboard_val,
        }))
        .unwrap_or(report_json_str)
    }
    /// Get server configuration.
    pub fn config(&self) -> &TouringConfig {
        &self.config
    }

    /// Spawn background tasks for live data ingestion and evolution analysis.
    /// Must be called before `.serve()` consumes self.
    pub fn spawn_background_tasks(&self) {
        // 0. Spawn FileWatcher for incremental symbol indexing (hot path for current project)
        {
            let graph_svc = Arc::clone(&self.graph_svc);
            let project_root = self.config.project_root.clone();

            tokio::spawn(async move {
                // Create file watcher for current project (with gitignore filtering)
                let builder = match FileWatcherBuilder::new(&project_root) {
                    Ok(b) => b,
                    Err(e) => {
                        warn!("FileWatcher: failed to create builder: {}", e);
                        return;
                    }
                };
                let mut watcher = match builder.debounce_ms(100).build() {
                    Ok(w) => w,
                    Err(e) => {
                        warn!("FileWatcher: failed to build: {}", e);
                        return;
                    }
                };

                if let Err(e) = watcher.start() {
                    warn!("FileWatcher: failed to start: {}", e);
                    return;
                }

                info!("FileWatcher started: {:?}", project_root);
                loop {
                    if let Some(event) = watcher.next_event().await {
                        graph_svc.on_file_event(&event).await;
                    }
                }
            });
        }

        // 1. Spawn JSONL watcher for live data ingestion
        if self.config.jsonl_watch_enabled {
            if let Some(ref memory) = self.memory {
                let store = Arc::clone(memory);
                let embedder = self.embedder.clone();
                let poll_interval = self.config.jsonl_poll_interval_s;
                let data_dir = self.config.project_root.join("data");
                let state_path = self.config.project_root.join("data/watcher_state.json");

                tokio::spawn(async move {
                    let watch_paths = discover_jsonl_paths(&data_dir);
                    if watch_paths.is_empty() {
                        info!(
                            "JsonlWatcher: no JSONL files found in {}",
                            data_dir.display()
                        );
                        return;
                    }

                    let watcher_config = WatcherConfig {
                        watch_paths,
                        poll_interval_s: poll_interval,
                        state_path,
                        embed_batch_size: 32,
                    };

                    let watcher = JsonlWatcher::new(watcher_config, store, embedder);
                    info!("JsonlWatcher started: polling every {}s", poll_interval);
                    watcher.run().await;
                });
            }
        }

        // 2. Spawn evolution analysis engine
        if self.config.evolution_enabled {
            if let Some(ref memory) = self.memory {
                let store = Arc::clone(memory);
                let qtable_bg = Arc::clone(&self.qtable);
                let linucb_bg = Arc::clone(&self.linucb);
                let online_rl_bg = Arc::clone(&self.online_rl);
                let interval_s = self.config.evolution_interval_s;
                let db_path = self.config.rlm_db_path.clone();

                tokio::spawn(async move {
                    let mut ticker =
                        tokio::time::interval(std::time::Duration::from_secs(interval_s));
                    // Skip first tick (fires immediately)
                    ticker.tick().await;

                    // P0-1 fix: Track last processed event ID to avoid reprocessing
                    let mut last_processed_id: i64 = 0;

                    loop {
                        ticker.tick().await;

                        // Create fresh std::sync::Mutex instances each tick (read-only analysis)
                        let rlm_tick = match RlmMemory::new(&db_path) {
                            Ok(r) => r,
                            Err(e) => {
                                warn!("Evolution: RLM open failed: {}", e);
                                continue;
                            }
                        };
                        let mut ranker_tick = WilsonRanker::new();
                        let mut drift_tick = DriftDetector::new();
                        {
                            let p = LearningPersistence::new(&db_path);
                            let _ = p.load_wilson(&mut ranker_tick);
                            let _ = p.load_drift(&mut drift_tick);
                        }
                        let analyzer = EvolutionAnalyzer::new(rlm_tick, ranker_tick, drift_tick);

                        let results = analyzer.analyze_all();
                        let insights = InsightEngine::generate(&results);

                        if !insights.is_empty() {
                            info!("Evolution engine: {} insights generated", insights.len());

                            // Store insights as memory entries
                            let store_lock = store.lock().await;
                            for insight in &insights {
                                let entry = MemoryEntry::new(
                                    format!("insight:{}:{}", insight.category, insight.created_at),
                                    "reference",
                                    &insight.message,
                                )
                                .with_entry_type("insight");
                                if let Err(e) = store_lock.store(entry) {
                                    warn!("Failed to store insight: {}", e);
                                }
                            }
                            drop(store_lock);
                        }

                        // Auto-learn: feed recent hook events into QTable (Bellman updates)
                        let persistence = LearningPersistence::new(&db_path);
                        // P0-1: Use tracked last_processed_id instead of 0
                        let events = persistence.load_hook_events_since(last_processed_id, 200);
                        if !events.is_empty() {
                            // P0-3: Minimize lock scope — collect data first, then lock briefly
                            let updates: Vec<_> = events
                                .iter()
                                .map(|(id, event_type, tool_name, reward)| {
                                    let state = event_type_to_state(event_type);
                                    let action_id = tool_name_to_action(tool_name);
                                    (*id, state, action_id, tool_name.clone(), *reward)
                                })
                                .collect();

                            // P0-1: Track highest event ID to avoid reprocessing
                            if let Some(max_id) = events.iter().map(|(id, _, _, _)| *id).max() {
                                last_processed_id = max_id;
                            }

                            let mut qt = qtable_bg.lock().await;
                            let mut bandit = linucb_bg.lock().await;
                            let mut rl_engine = online_rl_bg.lock().await;
                            let mut online_processed: usize = 0;

                            // P0-3: Process pre-collected updates under lock (minimal hold time)
                            for (_id, state, action_id, tool_name, reward) in &updates {
                                let next_state = state.saturating_add(1) % 9;
                                let _td_error =
                                    qt.update(*state, *action_id, *reward, next_state, None, false);

                                let immediate = ImmediateReward {
                                    tool_name: tool_name.clone(),
                                    accepted: *reward > 0.0,
                                    latency_ms: 0,
                                    error_count: if *reward < 0.0 { 1 } else { 0 },
                                    cila_level: (state / 4).min(6) as u8,
                                    file_type: (state % 4).min(3) as u8,
                                    quality_score: None,
                                };
                                if rl_engine
                                    .process_reward(&immediate, &mut qt, &mut bandit)
                                    .is_some()
                                {
                                    online_processed += 1;
                                }
                            }

                            info!(
                                "Evolution auto_learn: {} events processed ({} online RL updates, since_id={})",
                                updates.len(),
                                online_processed,
                                last_processed_id
                            );
                            // P0-2: Persist state atomically — save all before releasing locks
                            if let Err(e) = persistence.save_qtable(&qt) {
                                warn!("Failed to persist QTable after auto_learn: {}", e);
                            }
                            if rl_engine.should_save() {
                                tracing::debug!(
                                    ema_reward = rl_engine.ema_reward(),
                                    total_updates = rl_engine.update_count(),
                                    "OnlineRLEngine save checkpoint reached"
                                );
                            }
                            // P0-3: Release all locks together after persistence
                            drop(rl_engine);
                            drop(bandit);
                            drop(qt);
                        } else {
                            // Persist QTable state even without new events
                            let qt = qtable_bg.lock().await;
                            if let Err(e) = persistence.save_qtable(&qt) {
                                warn!("Failed to persist QTable state: {}", e);
                            }
                        }
                    }
                });
            }
        }

        // 3. Spawn periodic symbol index refresh (bootstrap script → clear → reload ALL projects)
        //    Fires once 5s after startup, then every 30min.
        //    This ensures new symbols written to symbols.db by external tools are
        //    picked up without requiring a manual `touring_graph(action: "reload")`.
        //    NOTE: After clear, reloads ALL projects (current + cross-project) to preserve
        //    cross-project data that was merged at startup.
        {
            let graph_index = self.graph_svc.inner();
            let symbol_store = self.symbol_store.clone();
            let project_root = self.config.project_root.clone();
            tokio::spawn(async move {
                // Initial delay: let server fully initialize before first bootstrap
                tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                loop {
                    let script = project_root
                        .join("scripts")
                        .join("touring_bootstrap_symbols.py");
                    // Defensive skip: if the optional bootstrap script is absent,
                    // do not spawn python3 only to fail with ENOENT. The script
                    // is an optional refresh hook — its absence is not an error.
                    // Logging at debug! keeps default-level logs quiet while
                    // still being discoverable via RUST_LOG=debug when needed.
                    if !script.exists() {
                        tracing::debug!(
                            "SymbolRefresh: bootstrap script not present at {}; skipping refresh cycle",
                            script.display()
                        );
                        tokio::time::sleep(tokio::time::Duration::from_secs(30 * 60)).await;
                        continue;
                    }
                    info!("SymbolRefresh: running bootstrap: {}", script.display());
                    let result = tokio::process::Command::new("python3")
                        .arg(&script)
                        .current_dir(&project_root)
                        .output()
                        .await;
                    match result {
                        Ok(output) if output.status.success() => {
                            info!("SymbolRefresh: bootstrap complete, reloading index");
                            if let Some(ref store_arc) = symbol_store {
                                let store = store_arc.lock().await;
                                let mut idx = graph_index.lock().await;
                                idx.clear();
                                // Reload current project
                                match store.load_into_index(&mut idx) {
                                    Ok(n) => info!(
                                        "SymbolRefresh: loaded {} symbol records from current",
                                        n
                                    ),
                                    Err(e) => warn!("SymbolRefresh: load_into_index failed: {}", e),
                                }
                                // Reload all cross-project databases (merge into same index)
                                if let Some(home) = std::env::var_os("HOME") {
                                    let projects_dir =
                                        PathBuf::from(home).join(".claude").join("projects");
                                    if let Ok(entries) = std::fs::read_dir(&projects_dir) {
                                        for entry in entries.flatten() {
                                            let cross_path = entry.path();
                                            if cross_path == project_root {
                                                continue;
                                            }
                                            let symbols_db = cross_path
                                                .join(".claude")
                                                .join("touring")
                                                .join("symbols.db");
                                            if symbols_db.exists() {
                                                if let Ok(cross_store) =
                                                    touring_code::ast::store::SymbolStore::new(
                                                        &symbols_db,
                                                    )
                                                {
                                                    match cross_store.load_into_index(&mut idx) {
                                                        Ok(n) => info!(
                                                            "SymbolRefresh: loaded {} symbols from cross-project {}",
                                                            n,
                                                            symbols_db.display()
                                                        ),
                                                        Err(e) => warn!(
                                                            "SymbolRefresh: cross-project load failed ({}): {}",
                                                            symbols_db.display(),
                                                            e
                                                        ),
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        Ok(output) => {
                            warn!(
                                "SymbolRefresh: bootstrap non-zero exit: {}",
                                String::from_utf8_lossy(&output.stderr)
                                    .lines()
                                    .next()
                                    .unwrap_or("no stderr")
                            );
                        }
                        Err(e) => warn!("SymbolRefresh: failed to spawn bootstrap: {}", e),
                    }
                    tokio::time::sleep(tokio::time::Duration::from_secs(30 * 60)).await;
                }
            });
        }

        // 4. Spawn CC transcript miner (Phase 2 Slice 2.3).
        //    Mines error→resolution pairs from ~/.claude/projects/**/*.jsonl and persists
        //    lessons to MemoryStore (key "outcome:<tool_class>:transcript-<hash>:failure",
        //    tier "reference"). Retrieved by cli_suggester::collect_memory_lessons via the
        //    SQL `LIKE 'outcome:<tool_class>:%:failure'` query — same writer↔reader contract.
        //    Gated by env var TOURING_TRANSCRIPT_MINER (default: enabled; set to "0" to disable).
        if std::env::var("TOURING_TRANSCRIPT_MINER").as_deref() != Ok("0") {
            if let Some(ref memory) = self.memory {
                let store = Arc::clone(memory);
                let state_path = self
                    .config
                    .project_root
                    .join(".claude")
                    .join("touring")
                    .join("transcript_miner_state.json");

                tokio::spawn(async move {
                    // Resolve ~/.claude/projects at spawn time.
                    let projects_root = match std::env::var_os("HOME") {
                        Some(home) => std::path::PathBuf::from(home)
                            .join(".claude")
                            .join("projects"),
                        None => {
                            tracing::debug!(
                                "TranscriptMiner: HOME not set, skipping transcript mining"
                            );
                            return;
                        }
                    };

                    let mut miner = TranscriptMiner::new(state_path);

                    // Initial sweep at startup (catch up on any unprocessed transcripts).
                    {
                        let store_lock = store.lock().await;
                        let stats = miner.sweep(&projects_root, &store_lock);
                        if stats.pairs_persisted > 0 {
                            tracing::info!(
                                "TranscriptMiner startup: scanned={} lines={} mined={} persisted={} deduped={}",
                                stats.files_scanned,
                                stats.lines_read,
                                stats.pairs_mined,
                                stats.pairs_persisted,
                                stats.pairs_deduped,
                            );
                        } else {
                            tracing::debug!(
                                "TranscriptMiner startup: scanned={} lines={} mined={} persisted=0",
                                stats.files_scanned,
                                stats.lines_read,
                                stats.pairs_mined,
                            );
                        }
                    }

                    // Periodic sweep every 300s.
                    let mut ticker = tokio::time::interval(std::time::Duration::from_secs(300));
                    // Skip the first immediate tick (startup sweep already ran above).
                    ticker.tick().await;

                    loop {
                        ticker.tick().await;
                        let store_lock = store.lock().await;
                        let stats = miner.sweep(&projects_root, &store_lock);
                        if stats.pairs_persisted > 0 || stats.lines_read > 0 {
                            tracing::info!(
                                "TranscriptMiner tick: scanned={} lines={} mined={} persisted={} deduped={}",
                                stats.files_scanned,
                                stats.lines_read,
                                stats.pairs_mined,
                                stats.pairs_persisted,
                                stats.pairs_deduped,
                            );
                        }
                    }
                });
            } else {
                tracing::debug!(
                    "TranscriptMiner: MemoryStore unavailable (degraded mode), skipping"
                );
            }
        }

        info!(
            "Background tasks spawned: watcher={}, evolution={}, transcript_miner={}",
            self.config.jsonl_watch_enabled,
            self.config.evolution_enabled,
            std::env::var("TOURING_TRANSCRIPT_MINER").as_deref() != Ok("0"),
        );
    }
}

/// Legacy Track A lure pattern — static match table (deprecated).
///
/// Production path uses [`CognitiveNexus::resolve()`] which calls
/// [`SessionPredictor::predict_next()`] internally. This static version is kept
/// only for the test suite which validates the lure pattern behaviour.
#[cfg(test)]
fn suggest_next_tool_simple(tool: &str, result: &str) -> Option<String> {
    match tool {
        "memory_recall" if result.contains("symbol") => {
            Some("mcp__touring__touring_ast_find para ver detalhes do símbolo encontrado".into())
        }
        "memory_recall" => {
            Some("mcp__touring__touring_suggest para próxima ação baseada em histórico".into())
        }
        "search_codebase" | "touring_index_status" => {
            Some("mcp__touring__touring_ast_overview para mapear os arquivos encontrados".into())
        }
        "ast_overview" | "ast_find" => {
            Some("mcp__touring__touring_graph para ver blast_radius destes símbolos".into())
        }
        "decompose" => Some(
            "mcp__touring__touring_memory_store para persistir este plano de decomposição".into(),
        ),
        _ => None,
    }
}

#[cfg(test)]
mod suggest_next_tool_tests {
    use super::*;

    #[test]
    fn test_memory_recall_with_symbol_suggests_get_symbol() {
        let result = suggest_next_tool_simple("memory_recall", "found symbol BM25Index");
        assert_eq!(
            result.as_deref(),
            Some("mcp__touring__touring_ast_find para ver detalhes do símbolo encontrado")
        );
    }

    #[test]
    fn test_search_codebase_suggests_overview() {
        let result = suggest_next_tool_simple("search_codebase", "found 3 files");
        assert!(result.is_some());
        assert!(result.unwrap().contains("touring_ast_overview"));
    }

    #[test]
    fn test_unknown_tool_returns_none() {
        let result = suggest_next_tool_simple("unknown_tool", "some result");
        assert!(result.is_none());
    }
}

// ── S-3.3: risk_scoring RL reward signal integration test ──────────────────────

#[cfg(test)]
mod risk_scoring_rl_tests {
    use super::*;
    use crate::tools::risk_scoring::{ChangeRiskInput, Hotspot, TestGap, build_change_risk_report};
    use touring_intelligence::rl::rl::djb2_hash;

    #[test]
    fn test_risk_scoring_feeds_qtable_reward_signal() {
        // Build a high-risk report (overall_risk should be > 0)
        let changed_files = vec!["src/core.rs".to_string()];
        let affected_files = vec![
            "src/core.rs".to_string(),
            "src/derived.rs".to_string(),
            "tests/test_core.rs".to_string(),
        ];
        let test_gaps = vec![TestGap {
            symbol: "critical_fn".to_string(),
            file_path: "src/core.rs".to_string(),
            criticality: 0.8,
        }];
        let hotspots = vec![Hotspot {
            symbol: "Core::process".to_string(),
            file_path: "src/core.rs".to_string(),
            criticality: 0.95,
            factors: vec!["high fan-in".to_string()],
        }];
        let gotcha_warnings = vec!["known pitfall: unsafe block".to_string()];

        let output = build_change_risk_report(ChangeRiskInput {
            changed_files,
            affected_files,
            affected_symbols: 10,
            test_gaps,
            hotspots,
            gotcha_warnings,
            wiring_score: 0.7,
            detail_level: DetailLevel::Standard,
        });

        // Verify overall_risk field exists and is extracted correctly
        let overall_risk = output
            .get("overall_risk")
            .and_then(|v| v.as_f64())
            .expect("overall_risk field must be present");

        // Compute quality_score the same way server/mod.rs does (line 4341)
        let quality_score = (1.0 - overall_risk) * 100.0;

        // Verify quality_score is in valid range [0, 100]
        assert!(
            (0.0..=100.0).contains(&quality_score),
            "quality_score should be in [0, 100], got {}",
            quality_score
        );

        // Now feed this quality_score into QTable::update_from_hook_event
        let mut qtable = QTable::new();
        let file_type = "rust";
        let tool_name = "risk_scoring";
        let cila_level = 0;

        // This is the RL reward pathway: S-3.2 wired qtable.update_from_hook_event
        let td_error =
            qtable.update_from_hook_event(cila_level, file_type, tool_name, quality_score);

        // First update should produce a TD error (positive for good quality_score)
        assert!(
            td_error >= 0.0,
            "first update should produce non-negative TD error, got {}",
            td_error
        );

        // Verify QTable has an entry for this state-action
        let file_type_idx = 1; // rust
        let state = (cila_level as u64) * 4 + file_type_idx;
        let action = djb2_hash(tool_name) % 64;
        let q_value = qtable.get_q(state, action);

        assert!(
            q_value > 0.0,
            "Q-value should be positive after reward signal, got {} for state={}, action={}",
            q_value,
            state,
            action
        );
    }

    #[test]
    fn test_risk_scoring_low_risk_produces_high_quality_score() {
        // Low risk → high quality_score → positive RL reward
        let output = build_change_risk_report(ChangeRiskInput {
            changed_files: vec!["docs/README.md".to_string()],
            affected_files: vec!["docs/README.md".to_string()],
            affected_symbols: 1,
            test_gaps: vec![],
            hotspots: vec![],
            gotcha_warnings: vec![],
            wiring_score: 1.0,
            detail_level: DetailLevel::Minimal,
        });

        let overall_risk = output
            .get("overall_risk")
            .and_then(|v| v.as_f64())
            .expect("overall_risk must be present");

        let quality_score = (1.0 - overall_risk) * 100.0;

        // Low risk should yield high quality_score
        assert!(
            quality_score > 50.0,
            "low risk should produce quality_score > 50, got {} (risk={})",
            quality_score,
            overall_risk
        );

        // Verify QTable update with this high quality_score
        let mut qtable = QTable::new();
        let td_error = qtable.update_from_hook_event(0, "other", "risk_scoring", quality_score);
        assert!(
            td_error >= 0.0,
            "low risk (high quality) should produce non-negative TD error"
        );
    }

    #[test]
    fn test_risk_scoring_high_risk_produces_low_quality_score() {
        // High risk → low quality_score
        let changed: Vec<String> = (0..10).map(|i| format!("src/module_{}.rs", i)).collect();
        let affected: Vec<String> = (0..50).map(|i| format!("src/affected_{}.rs", i)).collect();
        let test_gaps = (0..5)
            .map(|i| TestGap {
                symbol: format!("func_{}", i),
                file_path: format!("src/module_{}.rs", i),
                criticality: 0.9,
            })
            .collect();
        let hotspots = vec![Hotspot {
            symbol: "critical_api".to_string(),
            file_path: "src/core.rs".to_string(),
            criticality: 0.98,
            factors: vec!["high fan-in".to_string(), "no tests".to_string()],
        }];
        let gotcha_warnings = vec![
            "pitfall A".to_string(),
            "pitfall B".to_string(),
            "pitfall C".to_string(),
        ];

        let output = build_change_risk_report(ChangeRiskInput {
            changed_files: changed,
            affected_files: affected,
            affected_symbols: 100,
            test_gaps,
            hotspots,
            gotcha_warnings,
            wiring_score: 0.3,
            detail_level: DetailLevel::Full,
        });

        let overall_risk = output
            .get("overall_risk")
            .and_then(|v| v.as_f64())
            .expect("overall_risk must be present");

        let quality_score = (1.0 - overall_risk) * 100.0;

        // High risk should yield low quality_score
        assert!(
            quality_score < 50.0,
            "high risk should produce quality_score < 50, got {} (risk={})",
            quality_score,
            overall_risk
        );

        // Verify QTable update (reward will still be positive but smaller)
        let mut qtable = QTable::new();
        let td_error = qtable.update_from_hook_event(0, "rust", "risk_scoring", quality_score);
        assert!(
            td_error >= 0.0,
            "TD error should be non-negative, got {}",
            td_error
        );
    }
}
