//! HookRuntime — Lightweight runtime for hook subcommands.
//!
//! Opens SQLite in WAL mode for shared state across hook invocations.
//! Integrates ACO quality tracking and result caching.
//!
//! Enhancement sprint fields: PredictiveFocusCache (E12), Pensieve (E15),
//! StableSessionContext (E19) for cached project-level context.
//!
//! Typical init time: <10ms.
use super::aco_bridge::{HookEventBuffer, HookOutcome, HookQualityAssessment, HookResultCache};
use super::async_knowledge::AsyncFileKnowledgeDB;
use super::dependency_cache::DependencyCache;
use super::inferlets::{InferletKind, InferletService};
use super::knowledge::FileKnowledgeDB;
use super::layer7_prediction::PredictionLayer;
use super::n1_bridge::N1Bridge;
use super::{IntentClassifier, PIIScanner};
use crate::aco_processor::{AcoEvent, AcoEventProcessor};
use crate::cortex_dispatcher::{
    CortexDispatcher, CortexEvent, NoOpEvidenceForwarder, TokioEvidenceForwarder,
};
use crate::pre_tool_validator::PreToolValidator;
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::RwLock;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::sync::broadcast::Receiver;
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::mpsc;
use touring_code::ast::{
    SharedPipeline, SymbolIndex, SymbolStore, graph::pheromone::PheromoneGraph,
};
use touring_foundation::schema::entity_registry::EntityRegistry;
use touring_intelligence::reasoning::semantic_graph::NodeType;
use touring_intelligence::reasoning::session_predictor::ToolInvocation;
use touring_intelligence::reasoning::{
    CognitiveRuntime, GraphPersistence, MemoryNode, Pensieve, PredictiveFocusCache,
};
use touring_intelligence::rl::aco::tracker::TrackerReport;
use touring_intelligence::rl::bandit::ContextualBandit;
use touring_intelligence::rl::bandit::granularity::{
    GranularityBandit, SplitFactor, features_for_task as granularity_features_for_task,
    reward_from_quality as granularity_reward_from_quality,
};
use touring_intelligence::rl::bandit::linucb::{ArmKind, LinUCBBandit};
use touring_intelligence::rl::memory::RlmMemory;
use touring_intelligence::rl::memory::crdt_graph::{CrdtNodeId, CrdtSemanticGraph};
use touring_intelligence::rl::rl::tiny_transformer::{MarkovPredictor, TinyTransformerPredictor};
use touring_intelligence::rl::{
    DriftDetector, EvolutionAnalyzer, ImmediateReward, OnlineRLConfig, OnlineRLEngine, WilsonRanker,
};
/// Response from a hook — testable alternative to `process::exit`.
///
/// Hooks can return this instead of calling the diverging `emit_*()` methods.
/// The binary entry point converts this to stdout + exit code.
#[derive(Debug, Clone, PartialEq)]
pub enum HookResponse {
    /// Allow the tool invocation (exit 0, no output).
    Allow,
    /// Inject context (exit 0, stdout JSON with additionalContext).
    Context {
        /// Additional context string injected into the hook output.
        context: String,
        /// Name of the lifecycle event this response belongs to, if known.
        event_name: Option<String>,
    },
    /// Deny tool execution (PreToolUse only).
    /// Returns permissionDecision: "deny" with reason.
    Deny {
        /// Human-readable explanation for denying the tool invocation.
        reason: String,
        /// Optional additional context emitted alongside the denial.
        context: Option<String>,
        /// Name of the lifecycle event this response belongs to, if known.
        event_name: Option<String>,
    },
    /// Block after tool execution (PostToolUse).
    /// Returns top-level decision: "block" with reason.
    Block {
        /// Human-readable explanation for blocking the executed tool.
        reason: String,
        /// Optional additional context emitted alongside the block.
        context: Option<String>,
        /// Name of the lifecycle event this response belongs to, if known.
        event_name: Option<String>,
    },
    /// Halt session entirely.
    /// Returns continue: false with stopReason.
    Halt {
        /// Reason reported to the user when halting the session.
        reason: String,
    },
    /// Context injection with modified tool input.
    /// Returns additionalContext + updatedInput in hookSpecificOutput.
    /// Used by PreToolUse hooks to normalize or correct tool inputs.
    ContextWithUpdatedInput {
        /// Additional context string injected into the hook output.
        context: String,
        /// Name of the lifecycle event this response belongs to, if known.
        event_name: Option<String>,
        /// Corrected tool input that replaces the original invocation payload.
        updated_input: serde_json::Value,
    },
}
impl HookResponse {
    /// Serialize this response to stdout and call `process::exit(0)`.
    ///
    /// This is the bridge between the testable `HookResponse` and the
    /// diverging behavior required by Claude Code hook protocol.
    pub fn emit(self) -> ! {
        // `Allow` emits nothing — an empty hook response is signalled by empty
        // stdout. Every other variant serializes byte-identically to `to_json`
        // (verified variant-by-variant), so delegate there: one source of truth
        // for the wire format, eliminating the emit/to_json drift-bug class.
        if !matches!(self, HookResponse::Allow) {
            println!("{}", self.to_json());
        }
        std::process::exit(0);
    }
    /// Build a context response with event name.
    pub fn context_with_event(context: impl Into<String>, event_name: impl Into<String>) -> Self {
        HookResponse::Context {
            context: context.into(),
            event_name: Some(event_name.into()),
        }
    }
    /// Serialize to JSON string without exiting (for tests and logging).
    pub fn to_json(&self) -> String {
        match self {
            HookResponse::Allow => "{}".to_string(),
            HookResponse::Context {
                context,
                event_name,
            } => {
                let truncated = crate::hook_response::truncate_context(context);
                let mut hso = serde_json::json!({ "additionalContext" : truncated, });
                if let Some(name) = event_name {
                    #[allow(clippy::indexing_slicing)]
                    {
                        hso["hookEventName"] = serde_json::Value::String(name.clone());
                    }
                }
                serde_json::json!({ "hookSpecificOutput" : hso }).to_string()
            }
            HookResponse::Deny {
                reason,
                context,
                event_name,
            } => {
                let mut hso = serde_json::json!(
                    { "permissionDecision" : "deny", "permissionDecisionReason" : reason,
                    }
                );
                if let Some(ctx) = context {
                    #[allow(clippy::indexing_slicing)]
                    {
                        hso["additionalContext"] = serde_json::Value::String(ctx.clone());
                    }
                }
                if let Some(name) = event_name {
                    #[allow(clippy::indexing_slicing)]
                    {
                        hso["hookEventName"] = serde_json::Value::String(name.clone());
                    }
                }
                serde_json::json!({ "hookSpecificOutput" : hso }).to_string()
            }
            HookResponse::Block {
                reason,
                context,
                event_name,
            } => {
                let mut hso = serde_json::json!({});
                if let Some(ctx) = context {
                    #[allow(clippy::indexing_slicing)]
                    {
                        hso["additionalContext"] = serde_json::Value::String(ctx.clone());
                    }
                }
                if let Some(name) = event_name {
                    #[allow(clippy::indexing_slicing)]
                    {
                        hso["hookEventName"] = serde_json::Value::String(name.clone());
                    }
                }
                serde_json::json!(
                    { "decision" : "block", "reason" : reason, "hookSpecificOutput" :
                    hso, }
                )
                .to_string()
            }
            HookResponse::Halt { reason } => {
                serde_json::json!({ "continue" : false, "stopReason" : reason, }).to_string()
            }
            HookResponse::ContextWithUpdatedInput {
                context,
                event_name,
                updated_input,
            } => {
                let truncated = crate::hook_response::truncate_context(context);
                let mut hso = serde_json::json!(
                    { "additionalContext" : truncated, "updatedInput" : updated_input, }
                );
                if let Some(name) = event_name {
                    #[allow(clippy::indexing_slicing)]
                    {
                        hso["hookEventName"] = serde_json::Value::String(name.clone());
                    }
                }
                serde_json::json!({ "hookSpecificOutput" : hso }).to_string()
            }
        }
    }
}
/// Core context layer — always initialized, used by ALL hooks.
///
/// Contains the knowledge DB, intent classifier, PII scanner,
/// result cache, and quality assessment tracking.
pub struct ContextRuntime {
    /// File knowledge database (SQLite WAL).
    pub knowledge: FileKnowledgeDB,
    /// Async knowledge database using deadpool-sqlite for non-blocking queries.
    /// Initialized on demand via `init_async_knowledge()`.
    pub async_knowledge: Option<AsyncFileKnowledgeDB>,
    /// ANN semantic memory recall — SQLite-backed persistent ANN index.
    /// Initialized on demand via `HookRuntime::init_ann_memory()`.
    /// Enables cross-session memory: similar files had similar errors.
    /// `RefCell` for interior mutability: post-edit hooks add memories
    /// through `&HookRuntime` (shared reference).
    pub ann_recall: RefCell<Option<crate::ann_memory::persistence::PersistedAnnMemoryRecall>>,
    /// Intent classifier (stateless, compiled RegexSet).
    pub classifier: IntentClassifier,
    /// PII scanner (stateless, compiled RegexSet).
    pub pii_scanner: PIIScanner,
    /// Pre-tool validator — AST gate for dangerous tool blocking.
    pub pre_tool_validator: PreToolValidator,
    /// ACO quality assessment — tracks hook outcomes across a session.
    /// Initialized via `reset_quality_tracking()` or session start.
    pub quality_assessment: Option<HookQualityAssessment>,
    /// ACO streaming event buffer — batches hook events for the ACO pipeline.
    /// Flush happens in post_compact_handler after cache re-warm.
    pub event_buffer: HookEventBuffer,
    /// ACO result cache — avoids recomputing hook results for unchanged files.
    /// Default: 256 entries, no TTL (invalidated on edit).
    pub result_cache: HookResultCache,
    /// WASM inferlet service — sandboxed plugin evaluation via AsyncInferletPool.
    /// Initialized on demand via `init_inferlets()`.
    pub inferlet_service: Option<InferletService>,
    /// Cached ErrorPredictor — avoids O(n) retrain from DB on every pre-edit/pre-write.
    /// Trained once on session-start, refreshed when stale (>60s since last train).
    pub error_predictor: Option<super::error_predictor::ErrorPredictor>,
    /// Timestamp of last ErrorPredictor training — used for staleness check.
    pub error_predictor_last_trained: Option<std::time::Instant>,
    /// C9-2: Name of the previous tool executed this session.
    /// Used by post_tool_rl to record Markov (prev_tool, cur_tool) transitions.
    pub last_tool_name: Option<String>,
    /// E19: Stratified session context — project-level data computed once at
    /// session-start and reused across all hook invocations. `None` until
    /// session-start populates it. Hooks fall back to direct DB queries
    /// when this is `None` (cold-start, standalone mode).
    /// `RefCell` for interior mutability: session-start sets it through `&HookRuntime`.
    pub stable_session: RefCell<Option<crate::shared::session_context::StableSessionContext>>,
    /// Fasciculus Arcuatus: typed bidirectional inter-hook communication bus.
    /// Replaces ad-hoc `result_cache["__meta__"]` keys with structured signals.
    /// `RefCell` for interior mutability: multiple hooks read/write through `&HookRuntime`.
    pub session_bus: RefCell<crate::shared::session_bus::SessionBus>,
    /// Wave C2-wiring (2026-04-20): per-session API surface cache for cascade analysis.
    /// Owned here so all hooks share a single cache without routing through the DB.
    pub api_cascade_cache: crate::shared::api_cascade_bridge::ApiSurfaceCache,
    /// Wave C4-D4 (2026-04-20): bounded queue of high-severity cascade proposals.
    /// Drained by `touring_decompose drain_cascades` MCP action → real subtasks.
    pub cascade_queue: crate::shared::cascade_queue::CascadeQueue,
    /// PLN2 (2026-04-21): distributed 2PC saga coordinator for multi-agent subagents.
    /// Always enabled. Manages Register/Prepare/Decide/Delta protocol with remote
    /// subagents over the daemon socket using rkyv zero-copy framing.
    pub distributed_saga: crate::saga::DistributedSagaCoordinator,
    /// WASM inferlet evaluation channel — sender to the project actor so that
    /// `cli_inferlets_exec` (which runs on the bare actor thread) can dispatch
    /// `RunInferlet` commands and await the result synchronously via oneshot.
    /// `Arc<Mutex<Option<...>>>` allows None before actor spawn and safe mutation
    /// through `&HookRuntime` shared reference post-spawn.
    cmd_tx: std::sync::Arc<
        std::sync::Mutex<Option<mpsc::Sender<crate::daemon_protocol::ProjectCommand>>>,
    >,
    /// Wave 13 (2026-04-27): distributed tracing context for hook chain observability.
    /// Records per-hop timing across `pre_read → pre_edit → post_edit → post_write`.
    /// `None` until first hook invocation; created at `pre_read` entry.
    /// `RefCell` for interior mutability through `&HookRuntime` (shared reference).
    pub span: RefCell<Option<crate::shared::span_context::SpanContext>>,
}

impl ContextRuntime {
    /// Returns the actor command sender injected at spawn time.
    /// Used by `cli_inferlets_exec` to dispatch RunInferlet from the bare actor thread.
    pub fn cmd_tx(&self) -> mpsc::Sender<crate::daemon_protocol::ProjectCommand> {
        let guard = self.cmd_tx.lock().expect("cmd_tx mutex poisoned");
        guard
            .as_ref()
            .expect("cmd_tx not initialized — actor not yet spawned")
            .clone()
    }

    /// Inject the command sender at actor spawn time. Called from `ProjectRuntime::new`.
    pub fn set_cmd_tx(&self, tx: mpsc::Sender<crate::daemon_protocol::ProjectCommand>) {
        let mut guard = self.cmd_tx.lock().expect("cmd_tx mutex poisoned");
        *guard = Some(tx);
    }
}
/// Learning layer — RL models, bandits, predictors.
///
/// Used primarily by post-tool-rl, session hooks, and context selection.
pub struct LearningRuntime {
    /// P4.1: LinUCB bandit for adaptive context injection selection.
    /// Initialized on first use via `linucb_bandit()`.
    pub linucb: Option<LinUCBBandit>,
    /// R3: Polymorphic bandit (Box\<dyn ContextualBandit\>) — supports LinUCB, AstEnriched, Transfer.
    /// Initialized from linucb on first use, or upgraded to AstEnrichedBandit when ast-features available.
    pub bandit: Option<Box<dyn ContextualBandit>>,
    /// R6: Online RL engine — n-step TD, EMA smoothing, forced exploration.
    /// Processes ImmediateReward signals from PostToolUse hooks.
    pub online_rl: Option<OnlineRLEngine>,
    /// R17: Tool sequence predictor for prefetching context.
    /// Uses a tiny transformer model to predict the next likely tool(s).
    pub predictor: Option<TinyTransformerPredictor>,
    /// R18: CRDT graph for multi-agent knowledge sharing.
    /// Loaded from disk on init; saved explicitly via `save_crdt_graph()`.
    pub crdt_graph: Option<CrdtSemanticGraph>,
    /// Evolution analyzer — tool effectiveness (WilsonRanker), drift detection
    /// (DriftDetector), and episodic memory (RlmMemory). Populated lazily via
    /// post-tool hooks; used by `cli_evolution_*` handlers.
    pub evolution_analyzer: Option<EvolutionAnalyzer>,
    /// In-memory QTable cache — eliminates disk I/O on each post-tool-rl invocation.
    /// Loaded from rkyv on session-start; persisted every QTABLE_BATCH_SIZE updates.
    pub qtable_cache: Option<touring_intelligence::rl::QTable>,
    /// C9-2: Markov transition tracker — records (prev_tool, next_tool) pairs so
    /// the MarkovPredictor can learn tool sequences at runtime.
    /// Completes the predict_next_tools training loop: record here → predict in pre_edit.
    pub markov_predictor: MarkovPredictor,
    /// E15: Pensieve — failed-command memory for bash hooks.
    /// Records failed bash commands as state embeddings; pre_bash checks
    /// for similar past failures before execution to warn Claude.
    /// `RefCell` for interior mutability: `post_bash` calls `record_failure`
    /// through `&HookRuntime` (shared reference), same pattern as `ann_recall`.
    /// Dim=32, threshold=0.4 (catches similar commands without false positives).
    pub pensieve: RefCell<Pensieve>,
    /// I-7: LearningLoop — per-strategy EMA success tracking for `touring suggest skill`.
    ///
    /// Records `GenerationEvent`s from post_read (AST vs regex path success),
    /// enabling `recommend_strategy(language, complexity)` to adapt over time.
    /// `RefCell` for interior mutability: hooks call `record_event` through `&HookRuntime`.
    /// `try_borrow_mut()` ensures fire-and-forget: a borrow failure silently skips recording.
    pub learning_loop: RefCell<touring_code::ast::learning_loop::LearningLoop>,
    /// S-26: HeatMap — tracks file edit frequency and recency for prioritization.
    ///
    /// Used by post_edit (records edits) and pre_read (records accesses) to build
    /// a heat score per file. `RefCell` for interior mutability through `&HookRuntime`.
    pub heat_map: RefCell<touring_code::ast::HeatMap>,
    /// P4-S2: Agentic RL — POMDP state + PPO policy optimization.
    /// Integrates with PatternBandit for semantic pattern learning signals.
    /// Activates when learning_phase_score > 0.5 (via OnlineRLEngine EMA).
    pub agentic_rl: Option<crate::agentic_rl::AgenticRL>,
    /// P4-S2: Shared PatternBandit for semantic pattern Q-Learning.
    /// Single canonical instance used by both AgenticRL and SemanticClassifier.
    /// Initialized once; avoids duplicate bandit instances causing conflicting Q-table updates.
    pub pattern_bandit:
        Option<std::sync::Arc<tokio::sync::RwLock<crate::pattern_bandit::PatternBandit>>>,
    /// S4: Last LinUCB arm selected during tool planning (pre_edit).
    /// Stored here so post_tool_rl can write it to SessionBus for cognitive engine correlation.
    pub last_arm_selected: Option<u8>,
    /// S1: Count of hook quality assessments consumed via HookStatsConsumer trait.
    pub hook_quality_assessments_consumed: u64,
    /// Wave C1.5 (2026-04-20): GranularityBandit — selects split factor
    /// (1/2/3/4 subtasks) for task decomposition. Lazily initialized via
    /// [`HookRuntime::granularity_bandit`]. Rewarded from `CodeHealthReport`
    /// deltas after tasks finalize.
    pub granularity_bandit: Option<GranularityBandit>,
}
impl LearningRuntime {
    /// S-25: Inject a reward signal for a CLI command after successful execution.
    ///
    /// Feeds the signal to the OnlineRLEngine for n-step TD learning, similar to
    /// `process_immediate_reward` but for CLI commands. Uses an in-memory QTable cache.
    pub fn inject_reward(&mut self, tool_name: &str, reward_val: f64, context: &str) {
        let reward = ImmediateReward {
            tool_name: tool_name.to_string(),
            accepted: reward_val > 0.0,
            latency_ms: 0,
            error_count: if reward_val > 0.0 { 0 } else { 1 },
            cila_level: 0,
            file_type: 0,
            quality_score: Some(reward_val),
        };
        if self.qtable_cache.is_none() {
            self.qtable_cache = Some(touring_intelligence::rl::QTable::new());
        }
        if let Some(mut qtable) = self.qtable_cache.take() {
            if self.linucb.is_none() {
                self.linucb = Some(LinUCBBandit::new());
            }
            if let Some(mut engine) = self.online_rl.take() {
                if let Some(ref mut linucb) = self.linucb {
                    engine.process_reward(&reward, &mut qtable, linucb);
                }
                self.online_rl = Some(engine);
            }
            self.qtable_cache = Some(qtable);
        }
        if let Ok(mut ll) = self.learning_loop.try_borrow_mut() {
            let timestamp_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0);
            ll.record_event(touring_code::ast::learning_loop::GenerationEvent {
                symbol_name: context.to_string(),
                language: "cli".to_string(),
                success: reward_val > 0.0,
                strategy_used: tool_name.to_string(),
                timestamp_ms,
            });
        }
        self.update_pensieve_threshold();
    }
    /// P4-S1: Adaptive Pensieve threshold — adjusts based on RL EMA reward signal.
    ///
    /// When `ema_reward` is low (< 0.2), we tighten the threshold (higher = stricter)
    /// to suppress more failure-path matches and reduce false positives.
    /// When `ema_reward` is high (> 0.5), we loosen it (lower = more matches)
    /// to give more warnings for potentially good paths.
    ///
    /// Threshold range: [0.25, 0.6] — clamped to avoid pathological extremes.
    pub fn update_pensieve_threshold(&mut self) {
        if let Some(ref mut engine) = self.online_rl {
            let ema = engine.ema_reward();
            let current = {
                let p = self.pensieve.borrow();
                p.threshold()
            };
            let target = if ema < 0.2 {
                0.6
            } else if ema > 0.5 {
                0.25
            } else {
                0.4
            };
            if (target - current).abs() > 0.05 {
                let mut p = self.pensieve.borrow_mut();
                p.set_threshold(target);
                tracing::debug!(
                    ema_reward = ema,
                    pensieve_threshold = target,
                    "P4-S1: adaptive pensieve threshold updated"
                );
            }
        }
    }
    /// P4-S2: Lazily initialize AgenticRL with shared PatternBandit reference.
    /// Uses the canonical pattern_bandit field to avoid duplicate Q-table instances.
    /// Called on first use via `agentic_rl_mut()` to avoid allocating until needed.
    pub fn agentic_rl_mut(&mut self) -> &mut crate::agentic_rl::AgenticRL {
        if self.agentic_rl.is_none() {
            if self.pattern_bandit.is_none() {
                self.pattern_bandit = Some(std::sync::Arc::new(tokio::sync::RwLock::new(
                    crate::pattern_bandit::PatternBandit::new(),
                )));
            }
            let bandit = self
                .pattern_bandit
                .clone()
                .expect("pattern_bandit set above");
            self.agentic_rl = Some(crate::agentic_rl::AgenticRL::new(bandit));
        }
        self.agentic_rl
            .as_mut()
            .expect("agentic_rl initialized above")
    }

    /// P4-S2 (audit-extension, 2026-06-03): read-only snapshot of the
    /// AgenticRL state for `cli_agentic_rl_status` (CAH `mech.evolution-agent`
    /// row observability). Returns `None` if the meta-loop has never been used
    /// (lazy init is the caller's responsibility — no allocation on read).
    pub fn agentic_rl_state(&self) -> Option<crate::agentic_rl::AgenticRLStateView> {
        self.agentic_rl.as_ref().map(|r| r.state_view())
    }
    /// P4-S2: Get or create the canonical shared PatternBandit.
    /// Used by SemanticClassifier for Q-Learning reranking.
    pub fn pattern_bandit_mut(
        &mut self,
    ) -> std::sync::Arc<tokio::sync::RwLock<crate::pattern_bandit::PatternBandit>> {
        if self.pattern_bandit.is_none() {
            self.pattern_bandit = Some(std::sync::Arc::new(tokio::sync::RwLock::new(
                crate::pattern_bandit::PatternBandit::new(),
            )));
        }
        self.pattern_bandit
            .clone()
            .expect("pattern_bandit set above")
    }
}
/// Infrastructure layer — AST, symbols, dependencies.
///
/// Used primarily by pre-edit, post-edit, post-read for symbol extraction
/// and dependency tracking.
pub struct InfraRuntime {
    /// P4.3: SymbolStore for cross-session symbol persistence.
    /// Opened lazily on first use.
    pub symbol_store: Option<SymbolStore>,
    /// P4.4: In-memory SymbolIndex for fast symbol lookups.
    /// Populated from SymbolStore on first access.
    pub symbol_index: Option<SymbolIndex>,
    /// P7.4: Incremental AST pipeline — caches parsed trees so that
    /// a second read of the same file pays O(edit) instead of O(file).
    /// Backed by SQLite for cross-session symbol persistence.
    pub pipeline: Option<SharedPipeline>,
    /// In-memory petgraph-backed dependency graph for O(V+E) blast_radius BFS.
    /// Built lazily from SQLite relations via `init_dependency_cache()`.
    /// `None` until first call to `init_dependency_cache()` or `add_dependency()`.
    pub dependency_cache: Option<DependencyCache>,
    /// Layer 7: Prediction engine — anticipatory context injection.
    /// Tracks co-edit patterns and session sequences to predict next files.
    /// Always initialized; zero cost when empty.
    pub prediction: PredictionLayer,
    /// E12: ACO pheromone-guided predictive cache from touring-cognitive.
    /// Delegates co-access pheromone tracking to the more sophisticated
    /// `PredictiveFocusCache` (SymbolPheromoneMap + LRU FocusCache) while
    /// keeping `PredictionLayer` for session-sequence and file-heat signals.
    /// Always initialized; 5% evaporation rate per tick (session-length).
    pub predictive_focus: PredictiveFocusCache,
    /// Last file edited in this session — used by post_edit/post_write to
    /// build co-edit pairs for Layer7 source 2 (co-edit graph).
    /// RefCell allows mutable access through &HookRuntime.
    pub last_edited_file: RefCell<Option<String>>,
    /// P3.3: Entity Registry — canonical codes for symbol disambiguation.
    /// Resolves homonimia (generic names like 'Index', 'Manager' across crates).
    /// Initialized lazily on first access via init_entity_registry().
    pub entity_registry: RefCell<Option<EntityRegistry>>,
    /// FA-4: CortexDispatcher — thin tool call dispatcher for touring-cortex integration.
    /// Broadcasts tool events to registered handlers and feeds evidence to
    /// MetacognitivePipeline for concept-drift detection.
    pub cortex_dispatcher: CortexDispatcher,
    /// ACO pheromone trail graph — shared with PheromoneGraphSignalLayer in pre_read.
    /// Initialized with 10% evaporation rate; populated by post_edit/post_read hooks
    /// as they record co-access edges. Arc<RwLock<_>> allows clone into SignalPipeline
    /// without moving InfraRuntime.
    pub pheromone_graph: Arc<RwLock<PheromoneGraph>>,
}
/// Lightweight runtime shared by all hook subcommands.
///
/// Integrates:
/// - File knowledge DB (SQLite WAL) via `ctx`
/// - RL models and bandits via `learning`
/// - AST pipeline and symbols via `infra`
/// - ACO quality assessment (9-dimensional GoalTracker)
/// - ACO result cache (avoids recomputing for unchanged files)
///
/// Decomposed into sub-structs by domain (S2 strategy):
/// - `ctx`: Core context layer (knowledge, classification, caching)
/// - `learning`: RL models, bandits, predictors, CRDT
/// - `infra`: AST pipeline, symbols, dependency graph
pub struct HookRuntime {
    /// Core context layer — knowledge DB, classifier, PII, cache, quality.
    pub ctx: ContextRuntime,
    /// Learning layer — LinUCB, polymorphic bandit, online RL, predictor, CRDT.
    pub learning: LearningRuntime,
    /// Infrastructure layer — symbol store/index, AST pipeline, dependency cache.
    pub infra: InfraRuntime,
    /// Project root directory.
    pub project_root: PathBuf,
    /// Auto-incrementing counter of pre-hook dispatches in the current session.
    /// Provides session_turn for LinUCB feature extraction without caller tracking.
    pub session_turn: AtomicUsize,
    /// Cognitive engine — semantic graph + predictor + MCTS + knowledge integration.
    /// Connects touring-cognitive to hook knowledge for enriched context.
    pub cognitive: Option<CognitiveRuntime>,
    /// N1 Bridge — eagerly-initialized connection to the N1 ToolSequenceGenerator.
    /// Initialized eagerly in `HookRuntime::new()` using the `aco_wiring.bus`.
    /// Bridges HookRuntime (N0) with the N1 layer for CILA L4+ complex tasks.
    pub n1_bridge: N1Bridge,
    /// F1: Receiver for CortexDispatcher broadcast channel.
    /// Stored in `Arc<Mutex>` so both HookRuntime and the background task can access it.
    /// Subscribed at session-start; processes incoming CortexEvents for drift detection.
    pub cortex_rx: Option<Arc<Mutex<Receiver<CortexEvent>>>>,
    /// P3.1: Enrichment pipeline state — activated after cognitive init.
    /// Enables auto-triggered context enrichment for pre-read hooks.
    pub enrichment_active: bool,
    /// Last loaded GoT snapshot from a previous session.
    /// Loaded at session-start, saved at session-stop.
    pub got_snapshot: Option<touring_intelligence::reasoning::GoTSnapshot>,
    /// Unified ACO wiring state: bus + bridge + multi_obj + session_predictor.
    /// Mutex wraps mutable state (MultiObjectivePheromonoLayer + evap counter).
    /// Bus/SessionPredictor use interior mutability internally.
    /// Access via `&self` from post_edit — fire-and-forget, never panics.
    pub aco_wiring: Mutex<crate::aco_wiring::AcoWiringState>,
    /// Last file for which pre-read context was injected.
    /// Used by post_tool_rl to correlate context utility with tool success.
    pub context_injection_file: Option<String>,
    /// S-3: Session-to-task mapping for decompose-event handler.
    /// Maps session_id -> task_id for ongoing task decomposition context.
    pub decompose_event_state: HashMap<String, String>,
    /// Auto-save hook for interval-based checkpointing (replaces mempal_save_hook.sh).
    /// Tracks tool exchanges and fires checkpoint at configured interval (default: 15).
    pub auto_save: crate::auto_save_hook::AutoSaveHook,
    /// TRIAD state for write operation protection (pre_write/post_write/rollback).
    /// Stores the BranchFs snapshot taken before a write; cleared after post_write.
    /// `RefCell` for interior mutability: pre_write sets, post_write reads/resets.
    pub triad_state: RefCell<Option<crate::triad_hook::TriadState>>,
    /// W4-2: ACO event processor — drains decomposer ACO events and injects
    /// them into the UnifiedPheromoneBus for downstream ACO consumers.
    pub aco_event_processor: AcoEventProcessor,
    /// ES2 P3 — last attested `HarnessContract` (EAGLE B-6 sink token).
    /// Re-attested on `session_start`, `pre_compact`, and `instructions_loaded`.
    /// Consumed by X9 LEARN (`gateway::learn::reconcile_drift`) so the
    /// `drift_corrector` has a `constitutional_digest` axis to compare
    /// pre vs post (ES2 P4 self-verifying loop).
    pub contract_attestation: Option<crate::gateway::harness_contract::HarnessContract>,
    /// ES3 P4 — cross-agent outcome ledger for feedback sharing across N
    /// concurrent agent processes (CAH OP4 §5.2.5). `None` in solo mode
    /// (no cross-agent contract). Opened at `session_start` if
    /// `{project_root}/.claude/touring` is writable.
    pub cross_agent_ledger: Option<Arc<crate::cross_agent_ledger::CrossAgentLedger>>,
    /// ES3 P4 — derived actor identity for this runtime. `None` until
    /// `ActorId::derive()` is called at session start with the agent role.
    pub actor_id: Option<crate::cross_agent_ledger::ActorId>,
    /// ES3 P5 — isolation mode. `Solo` by default; promoted to
    /// `Worktree(path)` when the `worktree-create` hook fires. Determines
    /// whether `AccessDeclaration` paths get rewritten to the worktree
    /// prefix.
    pub isolation_mode: IsolationMode,
}

// S-13 (2026-06-06): `IsolationMode` (ES3 P5 — concurrent-agent file-path
// isolation) relocated to the `touring-hooks-shared` leaf crate (leaf-safe,
// std-only). Re-exported here so `crate::hook_runtime::IsolationMode` call sites
// (`gate_metrics`, `lifecycle/worktree`) are unchanged; the gateway now names it
// directly from the leaf, breaking the gateway → hook_runtime edge.
pub use touring_hooks_shared::isolation_mode::IsolationMode;

// SEC-06/RBP-05: `HookRuntime` derives `Send` automatically — every field is
// already `Send` (verified: the crate compiles without a manual impl under both
// default and `--all-features`, and the whole workspace links cleanly). A blanket
// `unsafe impl Send for HookRuntime {}` was removed: it was redundant AND a latent
// UB hazard — it would silently keep forcing `Send` if a future `!Send` field were
// added, instead of producing a compile error. Leaving it auto-derived means the
// compiler enforces thread-safety going forward. Do NOT re-add a manual impl; if a
// field legitimately needs `!Send`-but-thread-safe handling, wrap it (Mutex/Arc)
// or document a field-specific SAFETY invariant instead.

/// Error from [`HookRuntime`] RL/cognitive persistence (save/load) methods
/// (F-8 / RBP-03: typed in place of `String`). The `From<String>` impl lets the
/// existing `?`-propagated `format!` messages convert transparently.
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct HookPersistError(pub String);

impl From<String> for HookPersistError {
    fn from(message: String) -> Self {
        Self(message)
    }
}

/// Error from [`HookRuntime::read_stdin`] (F-8 / RBP-03: typed in place of `String`).
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct StdinError(pub String);

/// Error from [`HookRuntime::new`] initialization (F-8 / RBP-03: typed in place
/// of `String`). `From<String>` lets the existing `?`-propagated `format!`
/// messages convert transparently.
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct HookRuntimeInitError(pub String);

impl From<String> for HookRuntimeInitError {
    fn from(message: String) -> Self {
        Self(message)
    }
}

/// Error returned by the CLI/daemon hook-dispatch entry points (the `run`
/// functions in `touring-hook-handlers` + `ceg_adapter::run`), unified so the
/// `touring-hooks` binary's dispatch `match` has one arm type
/// (F-8 / RBP-03: typed in place of `String`). Most producers diverge via
/// `HookResponse::emit() -> !`, so the value is rarely constructed; `From<String>`
/// transparently lifts the few `?`-propagated `format!` messages.
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct HookDispatchError(pub String);

impl From<String> for HookDispatchError {
    fn from(message: String) -> Self {
        Self(message)
    }
}

impl HookRuntime {
    /// Record a layer hop in the current trace context.
    ///
    /// Called by `pre_read`, `pre_edit`, `post_edit`, and `post_write` hooks
    /// to build up the per-hop timing trace. Creates the `SpanContext` on the
    /// first call (pre_read entry) and appends hops on subsequent calls.
    ///
    /// `layer` is a static string (e.g., "pre_read", "pre_edit", "post_edit").
    /// `enter_us` and `exit_us` are microsecond timestamps from `timestamp_us()`.
    pub fn record_span_layer(&self, layer: &'static str, enter_us: u64, exit_us: u64) {
        let mut span_borrow = self.ctx.span.borrow_mut();
        if span_borrow.is_none() {
            let trace_id = crate::shared::span_context::new_trace_id();
            *span_borrow = Some(crate::shared::span_context::SpanContext::new(trace_id));
        }
        if let Some(ref mut span) = *span_borrow {
            span.record_layer(layer, enter_us, exit_us);
        }
    }
    /// Returns a copy of the current span context if one exists.
    pub fn get_span(&self) -> Option<crate::shared::span_context::SpanContext> {
        self.ctx.span.borrow().clone()
    }
    /// Initialize the hook runtime.
    ///
    /// Opens the consolidated `knowledge.db` in the project's `.claude/touring/` directory.
    /// Creates the database and tables if they don't exist.
    /// Initializes result cache with 256-entry capacity and no TTL.
    pub fn new(project_root: &Path) -> Result<Self, HookRuntimeInitError> {
        let data_dir = project_root.join(".claude").join("data");
        if !data_dir.exists() {
            std::fs::create_dir_all(&data_dir)
                .map_err(|e| format!("Cannot create data dir: {e}"))?;
        }
        let touring_dir = project_root.join(".claude").join("touring");
        if !touring_dir.exists() {
            std::fs::create_dir_all(&touring_dir)
                .map_err(|e| format!("Cannot create touring dir: {e}"))?;
        }
        let db_path = touring_foundation::TouringConfig::knowledge_db_canonical(project_root);
        let knowledge =
            FileKnowledgeDB::new(&db_path).map_err(|e| format!("Cannot open knowledge DB: {e}"))?;
        let linucb_path = data_dir.join("linucb.rkyv");
        let linucb_result = LinUCBBandit::load_rkyv(&linucb_path);
        if let Err(ref e) = linucb_result {
            if linucb_path.exists() {
                eprintln!("[touring-hooks] WARN: linucb init failed (file corrupt?): {e}");
            }
        }
        let linucb = linucb_result.ok();
        let granularity_path = data_dir.join("granularity_bandit.json");
        let granularity_bandit: Option<GranularityBandit> = if granularity_path.exists() {
            match std::fs::read_to_string(&granularity_path)
                .map_err(|e| format!("read: {e}"))
                .and_then(|data| {
                    serde_json::from_str::<
                        touring_intelligence::rl::bandit::granularity::GranularitySnapshot,
                    >(&data)
                    .map_err(|e| format!("parse: {e}"))
                })
                .and_then(|snap| GranularityBandit::from_snapshot(&snap).map_err(|e| e.to_string()))
            {
                Ok(b) => Some(b),
                Err(e) => {
                    eprintln!(
                        "[touring-hooks] WARN: granularity bandit load failed \
                         (file corrupt?): {e}"
                    );
                    None
                }
            }
        } else {
            None
        };
        let health_delta_cache_path = touring_dir.join("health_delta_cache.json");
        if health_delta_cache_path.exists() {
            match crate::health_delta::load_health_delta_cache(project_root) {
                Ok(true) => {
                    tracing::debug!("health_delta cache restored from disk");
                }
                Ok(false) => {}
                Err(e) => {
                    eprintln!(
                        "[touring-hooks] WARN: health_delta cache load failed \
                         (file corrupt?): {e}"
                    );
                }
            }
        }
        let symbol_store_path =
            touring_foundation::TouringConfig::symbols_db_canonical(project_root);
        let symbol_store_result = SymbolStore::new(&symbol_store_path);
        if let Err(ref e) = symbol_store_result {
            eprintln!("[touring-hooks] WARN: symbol_store init failed: {e}");
        }
        let mut symbol_store = symbol_store_result.ok();
        if let Some(ref mut store) = symbol_store {
            use crate::knowledge_symbol_bridge::KnowledgeSymbolBridge;
            use std::sync::Arc;
            let bridge = Arc::new(KnowledgeSymbolBridge::new());
            store.subscribe(bridge);
        }
        let pipeline_db = touring_foundation::TouringConfig::graph_db_canonical(project_root);
        let pipeline = match pipeline_db.to_str() {
            Some(p) => {
                let r = SharedPipeline::with_symbol_store(p);
                if let Err(ref e) = r {
                    eprintln!("[touring-hooks] WARN: pipeline init failed: {e}");
                }
                r.ok()
            }
            None => {
                eprintln!("[touring-hooks] WARN: pipeline DB path contains non-UTF-8 characters");
                None
            }
        };
        let crdt_path = data_dir.join("crdt_graph.rkyv");
        let crdt_graph = CrdtSemanticGraph::load_from_mmap(&crdt_path).ok();
        let cache_ttl_ms = std::env::var("TOURING_CACHE_TTL_SECS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .map(|secs| secs * 1_000)
            .unwrap_or(300_000);
        let session_bus = crate::shared::session_bus::SessionBus::default();
        let cortex_dispatcher = CortexDispatcher::new();
        match TokioEvidenceForwarder::new() {
            Some(fwd) => {
                cortex_dispatcher.subscribe_to_bus(&session_bus, &fwd);
            }
            _ => {
                cortex_dispatcher.subscribe_to_bus(&session_bus, &NoOpEvidenceForwarder);
            }
        }
        Ok(Self {
            ctx: ContextRuntime {
                knowledge,
                async_knowledge: None,
                ann_recall: RefCell::new(None),
                classifier: IntentClassifier::new(),
                pii_scanner: PIIScanner::new(),
                pre_tool_validator: PreToolValidator::new(),
                quality_assessment: None,
                event_buffer: HookEventBuffer::new(100, 5_000),
                result_cache: HookResultCache::new(256, Some(cache_ttl_ms)),
                inferlet_service: None,
                error_predictor: None,
                error_predictor_last_trained: None,
                last_tool_name: None,
                stable_session: RefCell::new(None),
                session_bus: RefCell::new(session_bus),
                api_cascade_cache: crate::shared::api_cascade_bridge::ApiSurfaceCache::new(),
                cascade_queue: crate::shared::cascade_queue::CascadeQueue::new(),
                distributed_saga: crate::saga::DistributedSagaCoordinator::new(),
                span: RefCell::new(None),
                cmd_tx: std::sync::Arc::new(std::sync::Mutex::new(None)),
            },
            learning: LearningRuntime {
                linucb,
                bandit: None,
                online_rl: Some({
                    let mut engine = OnlineRLEngine::new(OnlineRLConfig::default());
                    engine.inject_warmup_reward();
                    engine
                }),
                predictor: Some(TinyTransformerPredictor::new_random(42)),
                crdt_graph,
                evolution_analyzer: Self::build_evolution_analyzer(&data_dir),
                qtable_cache: None,
                markov_predictor: MarkovPredictor::new(),
                pensieve: RefCell::new(Pensieve::new(32).with_threshold(0.4)),
                learning_loop: RefCell::new(touring_code::ast::learning_loop::LearningLoop::new()),
                heat_map: RefCell::new(touring_code::ast::HeatMap::new(100)),
                agentic_rl: None,
                pattern_bandit: None,
                last_arm_selected: None,
                hook_quality_assessments_consumed: 0,
                granularity_bandit,
            },
            infra: InfraRuntime {
                symbol_store,
                symbol_index: None,
                pipeline,
                dependency_cache: None,
                prediction: PredictionLayer::new(),
                predictive_focus: PredictiveFocusCache::default(),
                last_edited_file: RefCell::new(None),
                entity_registry: RefCell::new(None),
                cortex_dispatcher,
                pheromone_graph: Arc::new(RwLock::new(PheromoneGraph::new(0.1))),
            },
            project_root: project_root.to_path_buf(),
            session_turn: AtomicUsize::new(0),
            cognitive: None,
            n1_bridge: {
                let bus = Arc::new(crate::aco_wiring::AcoWiringState::new().bus.clone());
                N1Bridge::new(bus)
            },
            cortex_rx: None,
            enrichment_active: false,
            got_snapshot: None,
            aco_wiring: Mutex::new(crate::aco_wiring::AcoWiringState::new()),
            context_injection_file: None,
            decompose_event_state: HashMap::new(),
            auto_save: crate::auto_save_hook::AutoSaveHook::new(),
            triad_state: RefCell::new(None),
            aco_event_processor: {
                let aco_state = crate::aco_wiring::AcoWiringState::new();
                AcoEventProcessor::new(Arc::new(aco_state.bus))
            },
            contract_attestation: None,
            cross_agent_ledger: None,
            actor_id: None,
            isolation_mode: IsolationMode::default(),
        })
    }
    /// Build the evolution analyzer (WilsonRanker + DriftDetector + RlmMemory).
    ///
    /// RLM stores episodic data in `.claude/touring/memory.db`. The ranker and
    /// detector are in-memory and accumulated via post-tool hooks. Analyzers that
    /// fail to initialize (e.g., bad RLM path) return `None` — callers must handle
    /// this gracefully via SQL fallbacks.
    fn build_evolution_analyzer(data_dir: &Path) -> Option<EvolutionAnalyzer> {
        let project_root = data_dir
            .parent()
            .and_then(|p| p.parent())
            .unwrap_or(data_dir);
        let rlm_path = touring_foundation::TouringConfig::memory_db_canonical(project_root);
        let rlm = match RlmMemory::new(&rlm_path) {
            Ok(r) => r,
            Err(e) => {
                eprintln!(
                    "[touring-hooks] WARN: evolution_analyzer: RLM init failed ({rlm_path:?}): {e}"
                );
                return None;
            }
        };
        let ranker = WilsonRanker::new();
        let drift = DriftDetector::new();
        Some(EvolutionAnalyzer::new(rlm, ranker, drift))
    }
    /// Initialize the cognitive engine with a thread-safe knowledge source.
    /// Creates a separate DB connection for the cognitive runtime (WAL mode
    /// supports concurrent readers).
    pub fn init_cognitive(&mut self) {
        let data_dir = self.project_root.join(".claude").join("data");
        let graph_path = data_dir.join("cognitive_graph.json");
        let persistence = std::sync::Arc::new(GraphPersistence::new(graph_path));
        let db_path = touring_foundation::TouringConfig::knowledge_db_canonical(&self.project_root);
        let knowledge_db = match crate::knowledge::ThreadSafeKnowledgeDB::new(&db_path) {
            Ok(db) => db,
            Err(e) => {
                tracing::warn!("cognitive init failed (knowledge DB): {e}");
                let mut cognitive = CognitiveRuntime::new_standalone(persistence);
                cognitive.set_adaptive_engine(
                    touring_intelligence::reasoning::AdaptiveEngine::with_defaults(),
                );
                self.cognitive = Some(cognitive);
                return;
            }
        };
        let knowledge_arc: std::sync::Arc<
            dyn touring_intelligence::reasoning::bridge::KnowledgeSource,
        > = std::sync::Arc::new(knowledge_db);
        let mut cognitive = CognitiveRuntime::new_with_knowledge(persistence, knowledge_arc);
        cognitive
            .set_adaptive_engine(touring_intelligence::reasoning::AdaptiveEngine::with_defaults());
        self.cognitive = Some(cognitive);
        tracing::info!("cognitive engine initialized");
    }
    /// L7-B Alpha: Spawn cognitive runtime background tasks (warm cache loop).
    ///
    /// Spawns `context_predictor_task` (500ms warm cache loop) on the current
    /// tokio runtime. This transitions `cognitive_runtime` from `lazy_init/inactive`
    /// to `healthy` in daemon health reports.
    ///
    /// `maintenance_task` requires `Arc<CognitiveRuntime>` and is not spawned here;
    /// maintenance runs on-demand via post-tool hooks.
    ///
    /// No-op if:
    /// - `init_cognitive()` was not called (`self.cognitive` is `None`)
    /// - No tokio runtime is active (tests without `#[tokio::test]`)
    pub fn spawn_cognitive_background_tasks(&self) {
        let cognitive = match self.cognitive.as_ref() {
            Some(c) => c,
            None => {
                tracing::debug!("spawn_cognitive_background_tasks: cognitive not initialized");
                return;
            }
        };
        if tokio::runtime::Handle::try_current().is_err() {
            tracing::debug!("spawn_cognitive_background_tasks: no tokio runtime active");
            return;
        }
        let graph = cognitive.graph().clone();
        let predictor = cognitive.predictor().clone();
        tokio::spawn(
            touring_intelligence::reasoning::predictor_task::context_predictor_task(
                graph, predictor,
            ),
        );
        tracing::info!("cognitive_runtime: context_predictor_task spawned (500ms warm cache)");
    }
    /// F1: Subscribe HookRuntime's cognitive engine to CortexDispatcher.broadcast().
    ///
    /// Called at session-start after `init_cognitive()` succeeds.
    /// Spawns a background task that processes incoming CortexEvents:
    /// - Records ToolInvocation in SessionPredictor
    /// - Adds MemoryNode to SemanticGraph
    ///
    /// Non-blocking: if subscribe fails, logs warning and continues.
    pub fn subscribe_to_cortex_dispatcher(&mut self) {
        let rx_task = self.infra.cortex_dispatcher.subscribe();
        let rx_store = self.infra.cortex_dispatcher.subscribe();
        let cognitive = match self.cognitive.as_ref() {
            Some(c) => c,
            None => {
                tracing::warn!("cortex subscribe skipped: cognitive not initialized");
                return;
            }
        };
        let graph = cognitive.graph().clone();
        let predictor = cognitive.predictor().clone();
        if tokio::runtime::Handle::try_current().is_ok() {
            tokio::spawn(async move {
                let mut rx = rx_task;
                loop {
                    match rx.recv().await {
                        Ok(event) => {
                            let invocation = ToolInvocation {
                                tool_name: event.tool_name.clone(),
                                timestamp_ms: 0,
                                success: event.success,
                            };
                            predictor.record(invocation);
                            let node = MemoryNode::new(
                                event.file_path.clone(),
                                event.file_path.clone(),
                                NodeType::File,
                            );
                            if let Err(e) = graph.add_node(node) {
                                tracing::debug!(
                                    error = % e, "failed to add node to semantic graph"
                                );
                            }
                        }
                        Err(RecvError::Lagged(n)) => {
                            tracing::warn!(dropped = n, "cortex broadcast lagged — skipping");
                        }
                        Err(RecvError::Closed) => {
                            tracing::debug!("cortex broadcast channel closed");
                            break;
                        }
                    }
                }
            });
        } else {
            tracing::debug!("cortex dispatcher subscription skipped: no Tokio runtime");
        }
        self.cortex_rx = Some(Arc::new(Mutex::new(rx_store)));
        tracing::info!("cortex dispatcher subscription active");
    }
    /// P3.1: Trigger the enrichment pipeline after cognitive initialization.
    ///
    /// Activates auto-triggered context enrichment for pre-read hooks.
    /// Called at session-start after `init_cognitive()` succeeds.
    pub fn trigger_enrichment(&mut self) {
        self.enrichment_active = true;
        tracing::info!("enrichment_pipeline activated");
    }
    /// Build the in-memory petgraph DependencyCache from SQLite file relations.
    ///
    /// Reads all `(source, target)` pairs from `file_relations` and populates
    /// the petgraph graph. Idempotent — reinitializes on every call.
    /// Call once at daemon startup; then use `add_dependency()` for incremental updates.
    pub fn init_dependency_cache(&mut self) {
        let relations = self.ctx.knowledge.all_file_relations();
        let cache = DependencyCache::build_from_relations(
            relations.into_iter().map(|r| (r.source, r.target)),
        );
        tracing::info!(
            nodes = cache.node_count(),
            edges = cache.edge_count(),
            "dependency_cache initialized from SQLite"
        );
        self.infra.dependency_cache = Some(cache);
    }
    /// P3.3: Initialize the Entity Registry for symbol disambiguation.
    ///
    /// Opens `.claude/touring/entity_registry.db` and registers canonical entity codes.
    /// Lazy: does nothing if already initialized. Safe to call multiple times.
    pub fn init_entity_registry(&self) {
        if self.infra.entity_registry.borrow().is_some() {
            return;
        }
        let db_path = self
            .project_root
            .join(".claude")
            .join("touring")
            .join("entity_registry.db");
        if let Some(parent) = db_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        match rusqlite::Connection::open(&db_path) {
            Ok(conn) => match EntityRegistry::new(conn) {
                Ok(registry) => {
                    *self.infra.entity_registry.borrow_mut() = Some(registry);
                    tracing::info!("entity_registry initialized at {:?}", db_path);
                }
                Err(e) => {
                    tracing::warn!("entity_registry init failed: {}", e);
                }
            },
            Err(e) => {
                tracing::warn!("entity_registry db open failed: {}", e);
            }
        }
    }
    /// Register a new dependency edge in the in-memory cache (incremental update).
    ///
    /// Called from `post_edit` / `post_read` after a relation is written to SQLite.
    /// No-op if `init_dependency_cache()` has not been called yet.
    pub fn add_dependency(&mut self, from: &std::path::Path, to: &std::path::Path) {
        if let Some(ref mut cache) = self.infra.dependency_cache {
            cache.add_relation(&from.to_path_buf(), &to.to_path_buf());
        }
    }
    /// Invalidate a file's edges in the dependency cache.
    ///
    /// Called from `post_edit` when a file is modified so the next
    /// `blast_radius` call recomputes fresh dependencies.
    pub fn invalidate_dependency_cache_for_file(&mut self, path: &std::path::Path) {
        if let Some(ref mut cache) = self.infra.dependency_cache {
            cache.invalidate_file(&path.to_path_buf());
        }
    }
    /// Returns the current circuit breaker health report.
    ///
    /// Provides a structured view of all circuit breaker state:
    /// global catastrophic, per-operation-class, per-project, and per-session.
    ///
    /// Used by `pre_edit` and other hooks to perform circuit-aware checks before
    /// dispatching expensive operations. This is the Fasciculus Arcuatus wiring
    /// point that connects the file-based `CircuitBreaker` to the hook dispatch layer.
    ///
    /// # Example
    /// ```ignore
    /// let report = runtime.circuit_state();
    /// if report.global_state.catastrophic_count > 0 {
    ///     // Global circuit is in degraded state
    /// }
    /// ```
    pub fn circuit_state(&self) -> crate::circuit_breaker::CircuitHealthReport {
        crate::circuit_breaker::health()
    }
    /// Initialize the async knowledge database using deadpool-sqlite.
    ///
    /// This enables non-blocking SQLite operations via `pool.interact()`.
    /// Call this during daemon startup to warm the async pool.
    pub fn init_async_knowledge(&mut self) {
        let db_path = touring_foundation::TouringConfig::knowledge_db_canonical(&self.project_root);
        match crate::async_knowledge::AsyncFileKnowledgeDB::new(&db_path) {
            Ok(async_db) => {
                self.ctx.async_knowledge = Some(async_db);
                tracing::info!("async knowledge pool initialized");
            }
            Err(e) => {
                tracing::warn!("async knowledge pool init failed: {e} — falling back to sync");
            }
        }
    }
    /// Initialize the ANN semantic memory recall — SQLite-backed persistent index.
    ///
    /// Loads or creates the ANN index in `.claude/touring/memory.db`. This enables cross-session
    /// memory: "similar files had similar errors" via ANN similarity search on embeddings.
    ///
    /// Call this during daemon startup (after `init_async_knowledge`).
    /// If initialization fails, logs a warning and continues without ANN recall.
    pub fn init_ann_memory(&mut self) {
        let db_path = touring_foundation::TouringConfig::memory_db_canonical(&self.project_root);
        match crate::ann_memory::persistence::PersistedAnnMemoryRecall::load(&db_path) {
            Ok(ann) => {
                *self.ctx.ann_recall.borrow_mut() = Some(ann);
                tracing::info!("ANN memory recall initialized (persistent)");
            }
            Err(e) => {
                tracing::warn!("ANN memory recall init failed: {e} — falling back to in-memory");
                *self.ctx.ann_recall.borrow_mut() =
                    Some(crate::ann_memory::persistence::PersistedAnnMemoryRecall::new());
            }
        }
    }
    /// Reinitialize the N1 bridge with a fresh bus (e.g., after bus reset).
    ///
    /// S-7: N1 bridge is now eagerly initialized in `HookRuntime::new()`.
    /// This method exists for cases where a bus reset requires re-init.
    pub fn init_n1_bridge(&mut self) {
        let bus = {
            let wiring = self.aco_wiring.lock().expect("aco_wiring poisoned");
            Arc::new(wiring.bus.clone())
        };
        self.n1_bridge = N1Bridge::new(bus);
        tracing::info!("N1 bridge reinitialized");
    }
    /// Initialize the WASM inferlet service and load all compiled inferlets.
    ///
    /// Loads all 4 inferlet types (always_success, memory, pattern, classifier)
    /// into `AsyncInferletPool` instances of size `pool_size`.
    ///
    /// Uses embedded WASM bytes from `inferlets_assets` (requires `inferlets-wasm` feature).
    /// If the feature is not enabled or WASM files are stubs, this logs a warning and continues.
    ///
    /// Call this during daemon startup after `init_async_knowledge()`.
    pub async fn init_inferlets(&mut self, pool_size: usize) {
        #[cfg(feature = "inferlets-wasm")]
        {
            use crate::inferlets_assets::{INFERLET_WASM, load_all_inferlets};
            let wasm_len = INFERLET_WASM.len();
            tracing::info!(
                wasm_bytes_len = wasm_len,
                "init_inferlets: attempting WASM service init"
            );
            eprintln!(
                "[touring-hooks] init_inferlets: wasm_bytes_len={} pool_size={}",
                wasm_len, pool_size
            );
            if wasm_len == 0 {
                tracing::error!(
                    "INFERLET_WASM is empty — inferlets-wasm feature enabled but WASM binary not built"
                );
                eprintln!(
                    "[touring-hooks] ERROR: INFERLET_WASM is empty — run `cargo build --target wasm32-wasip1 --release -p inferlets` first"
                );
                return;
            }
            let service = match InferletService::new() {
                Ok(s) => s,
                Err(e) => {
                    tracing::error!("InferletService::new() failed: {e}");
                    eprintln!("[touring-hooks] ERROR: InferletService::new() failed: {e}");
                    return;
                }
            };
            match load_all_inferlets(&service, pool_size).await {
                Ok(()) => {
                    tracing::info!("WASM inferlets loaded (pool_size={pool_size})");
                    eprintln!("[touring-hooks] OK: WASM inferlets loaded (pool_size={pool_size})");
                    self.ctx.inferlet_service = Some(service);
                }
                Err(e) => {
                    tracing::error!("WASM inferlet loading failed: {e}");
                    eprintln!(
                        "[touring-hooks] ERROR: WASM inferlet load_all_inferlets failed: {e}"
                    );
                }
            }
        }
        #[cfg(not(feature = "inferlets-wasm"))]
        {
            let _ = pool_size;
            tracing::debug!("inferlets-wasm feature not enabled — WASM inferlets not loaded");
        }
    }
    /// Evaluate input using a loaded WASM inferlet.
    ///
    /// Returns `None` if the inferlet service is not initialized
    /// or if the requested `kind` is not loaded.
    pub async fn evaluate_inferlet(
        &self,
        kind: InferletKind,
        input: &str,
    ) -> Option<touring_bindings::wasm::PluginResult> {
        let service = self.ctx.inferlet_service.as_ref()?;
        service.evaluate(kind, input).await.ok()
    }
    /// Compute transitive blast_radius via petgraph BFS.
    ///
    /// Returns `None` if the cache has not been initialized.
    /// Falls back gracefully — callers should use `SymbolIndex::blast_radius` when `None`.
    pub fn petgraph_blast_radius(&self, path: &std::path::Path) -> Option<Vec<std::path::PathBuf>> {
        self.infra
            .dependency_cache
            .as_ref()
            .map(|c| c.blast_radius(&path.to_path_buf()).files())
    }
    /// Resolve enriched cognitive context for a tool invocation.
    /// Returns None if cognitive engine is not initialized.
    /// Note: this is an async method — caller needs a tokio runtime.
    pub async fn resolve_cognitive_context(
        &self,
        tool_name: &str,
        file_path: Option<&str>,
        query_hint: &str,
    ) -> Option<touring_intelligence::reasoning::EnrichedCtx> {
        let cognitive = self.cognitive.as_ref()?;
        Some(
            cognitive
                .resolve_enriched(tool_name, file_path, query_hint)
                .await,
        )
    }
    /// Save cognitive graph state for cross-session transfer learning.
    /// Called during session stop to persist the semantic graph and predictor state.
    pub fn save_cognitive_state(&self) -> Result<(), HookPersistError> {
        if let Some(ref cognitive) = self.cognitive {
            let removed = cognitive.graph().compact(1000);
            if removed > 0 {
                tracing::info!(removed, "compacted cognitive graph before save");
            }
            tracing::info!("cognitive state checkpointed");
        }
        Ok(())
    }
    /// Record a hook execution outcome in the quality assessment.
    ///
    /// No-op if quality tracking has not been initialized
    /// (call `reset_quality_tracking` first).
    pub fn record_hook_outcome(&mut self, outcome: HookOutcome) {
        if let Some(ref mut assessment) = self.ctx.quality_assessment {
            assessment.record(outcome);
        }
    }
    /// Generate a quality report for the current session.
    ///
    /// Returns `None` if quality tracking has not been initialized.
    pub fn quality_report(&self, iteration: u32) -> Option<TrackerReport> {
        self.ctx
            .quality_assessment
            .as_ref()
            .map(|a| a.to_tracker_report(iteration))
    }
    /// Initialize or reset quality tracking for a new session.
    ///
    /// Called by session-start hook to begin tracking.
    /// Also resets the session turn counter to zero.
    pub fn reset_quality_tracking(&mut self, session_id: &str) {
        self.ctx.quality_assessment = Some(HookQualityAssessment::new(session_id));
        self.session_turn.store(0, Ordering::Relaxed);
    }
    /// Return the current session turn (number of pre-hook dispatches so far).
    pub fn session_turn(&self) -> usize {
        self.session_turn.load(Ordering::Relaxed)
    }
    /// Increment the turn counter and return the new value.
    ///
    /// Called automatically at the start of each pre-hook dispatch so that
    /// callers no longer need to track the turn themselves.
    pub fn advance_session_turn(&self) -> usize {
        self.session_turn.fetch_add(1, Ordering::Relaxed) + 1
    }
    /// Check the result cache before computing a hook result.
    ///
    /// Returns `Some(cached_json)` on cache hit, `None` on miss.
    pub fn check_cache(&self, hook_name: &str, file_path: &str) -> Option<String> {
        self.ctx.result_cache.get_result(hook_name, file_path)
    }
    /// Store a computed hook result in the cache.
    pub fn store_cache(&self, hook_name: &str, file_path: &str, result_json: String) {
        self.ctx
            .result_cache
            .cache_result(hook_name, file_path, result_json);
    }
    /// W4-2: Process decomposer ACO events by draining and injecting into the ACO bus.
    ///
    /// Called by touring-server tools_analysis.rs after mutating decomposer state.
    /// Converts touring-server AcoEvent types into local AcoEvent and processes
    /// via the ACO pheromone pipeline.
    pub fn process_decomposer_aco_events(&mut self, events: Vec<AcoEvent>) {
        self.aco_event_processor.process_events(events);
    }
    /// Precompute and cache signals for a file after post_read.
    ///
    /// Computes all static signals (wiring, ecosystem, gotchas, dependents,
    /// notes, risk, blast_radius, similar_symbols, feature_gates) and stores
    /// them in the result cache under `__precomputed:{rel_path}`.
    /// Subsequent `pre_edit`/`pre_write` hooks consume these via O(1) cache lookup
    /// instead of N sequential DB/index queries.
    pub fn precompute_signals_for_file(
        &self,
        rel_path: &str,
        content_hash: Option<&str>,
        signals: Vec<(f32, String)>,
    ) {
        use crate::precomputed_signals::{PrecomputedSignal, PrecomputedSignals, cache_key};
        let precomputed = PrecomputedSignals::new(
            signals
                .into_iter()
                .map(|(s, t)| PrecomputedSignal(s, t))
                .collect(),
            content_hash.map(String::from),
        );
        if let Ok(json) = serde_json::to_string(&precomputed) {
            self.store_cache("pre_edit", &cache_key(rel_path), json);
        }
    }
    /// Invalidate all cached results for a file (after edit).
    ///
    /// Returns the number of entries invalidated.
    pub fn invalidate_cache_for_file(&self, file_path: &str) -> usize {
        self.ctx.result_cache.invalidate_file(file_path)
    }
    /// Get the current cache hit rate (0.0 - 1.0).
    pub fn cache_hit_rate(&self) -> f64 {
        self.ctx.result_cache.hit_rate()
    }
    /// Get or create the LinUCB bandit instance.
    pub fn linucb_bandit(&mut self) -> &mut LinUCBBandit {
        if self.learning.linucb.is_none() {
            self.learning.linucb = Some(LinUCBBandit::new());
        }
        self.learning.linucb.as_mut().expect("just created")
    }
    /// Select the best context injection strategy for the given file context.
    ///
    /// Returns the `ArmKind` (context strategy) and the UCB score.
    pub fn select_context_strategy(
        &mut self,
        file_type: &str,
        file_size: usize,
        session_turn: usize,
        recent_errors: usize,
        cila_level: usize,
    ) -> (ArmKind, f64) {
        let cila_u8 = (cila_level.min(255)) as u8;
        let features = touring_intelligence::rl::bandit::linucb::extract_features(
            file_type,
            file_size,
            session_turn,
            recent_errors,
            cila_u8,
        );
        let bandit = self.linucb_bandit();
        bandit.select_arm_kind(&features)
    }
    /// Record a reward for a context injection strategy.
    #[allow(clippy::too_many_arguments)]
    pub fn record_context_reward(
        &mut self,
        arm: usize,
        file_type: &str,
        file_size: usize,
        session_turn: usize,
        recent_errors: usize,
        cila_level: usize,
        reward: f64,
    ) {
        let cila_u8 = (cila_level.min(255)) as u8;
        let features = touring_intelligence::rl::bandit::linucb::extract_features(
            file_type,
            file_size,
            session_turn,
            recent_errors,
            cila_u8,
        );
        let bandit = self.linucb_bandit();
        bandit.update(arm, &features, reward);
    }
    /// Get or create the granularity bandit used for task split-factor
    /// decisions.
    pub fn granularity_bandit(&mut self) -> &mut GranularityBandit {
        if self.learning.granularity_bandit.is_none() {
            self.learning.granularity_bandit = Some(GranularityBandit::new());
        }
        self.learning
            .granularity_bandit
            .as_mut()
            .expect("just created")
    }
    /// Ask the granularity bandit how aggressively to split a proposed task.
    ///
    /// `size_loc` is the estimated lines-of-code the task will touch;
    /// `language` is matched case-insensitively (rust / python / typescript /
    /// javascript / other); `cila_level` is clamped to `[0, 4]`.
    pub fn select_task_split(
        &mut self,
        size_loc: usize,
        language: &str,
        cila_level: u8,
    ) -> SplitFactor {
        let features = granularity_features_for_task(size_loc, language, cila_level);
        let bandit = self.granularity_bandit();
        let (factor, _score) = bandit.select_split(&features);
        factor
    }
    /// Feed back an observed quality score (`[0,1]`) for a completed task.
    ///
    /// Applies a small linear penalty for extra subtasks so the bandit prefers
    /// the smallest factor that still hits high quality. Callers typically
    /// derive `quality` from `CodeHealthReport::composite`.
    pub fn record_task_split_outcome(
        &mut self,
        factor: SplitFactor,
        size_loc: usize,
        language: &str,
        cila_level: u8,
        quality: f64,
    ) {
        let features = granularity_features_for_task(size_loc, language, cila_level);
        let reward = granularity_reward_from_quality(quality, factor.subtask_count());
        let bandit = self.granularity_bandit();
        bandit.record_outcome(factor, &features, reward);
    }
    /// P4.2: Suggest a context verbosity level using QTable.
    ///
    /// Maps the current state (file_type * 4 + session_phase) to one of 4 levels:
    /// - 0: Minimal (just gotchas)
    /// - 1: Normal (gotchas + failures)
    /// - 2: Enriched (gotchas + failures + dependents)
    /// - 3: Full (all signals + touring suggestion)
    ///
    /// Falls back to level 1 (Normal) if no bandit is trained.
    #[allow(clippy::too_many_arguments)]
    pub fn suggest_context_level(
        &mut self,
        file_type: &str,
        file_size: usize,
        session_turn: usize,
        recent_errors: usize,
        cila_level: usize,
    ) -> u8 {
        let (arm, _score) = self.select_context_strategy(
            file_type,
            file_size,
            session_turn,
            recent_errors,
            cila_level,
        );
        match arm {
            ArmKind::None => 0,
            ArmKind::Overview | ArmKind::Gotcha => 1,
            ArmKind::BlastRadius | ArmKind::Relations | ArmKind::OverviewGotcha => 2,
            ArmKind::OverviewBlastRadius | ArmKind::FullEnrichment => 3,
        }
    }
    /// Persist LinUCB state to disk.
    pub fn save_linucb(&self) -> Result<(), HookPersistError> {
        if let Some(ref bandit) = self.learning.linucb {
            let path = self.project_root.join(".claude/data/linucb.rkyv");
            bandit
                .save_rkyv(&path)
                .map_err(|e| HookPersistError(e.to_string()))?;
        }
        Ok(())
    }
    /// Wave C1.7-persistence: persist the granularity bandit to disk as
    /// JSON. No-op when the bandit has never been accessed (Option is None),
    /// so first-run sessions stay cheap. The parent directory is created
    /// on-demand so callers don't need to pre-initialize `.claude/data/`.
    ///
    /// # Errors
    ///
    /// - Returns `Err` when JSON serialization fails (should never happen
    ///   for a valid `GranularitySnapshot`).
    /// - Returns `Err` when the filesystem write fails (permission, disk
    ///   full, read-only mount).
    pub fn save_granularity_bandit(&self) -> Result<(), HookPersistError> {
        let Some(ref bandit) = self.learning.granularity_bandit else {
            return Ok(());
        };
        let data_dir = self.project_root.join(".claude/data");
        std::fs::create_dir_all(&data_dir)
            .map_err(|e| format!("Failed to create data dir {data_dir:?}: {e}"))?;
        let path = data_dir.join("granularity_bandit.json");
        let snap = bandit.to_snapshot();
        let json = serde_json::to_string(&snap)
            .map_err(|e| format!("Failed to serialize granularity snapshot: {e}"))?;
        std::fs::write(&path, json)
            .map_err(|e| format!("Failed to write granularity snapshot to {path:?}: {e}"))?;
        Ok(())
    }
    /// Wave C1.7-persistence: load the granularity bandit from its on-disk
    /// JSON snapshot. Returns `Ok(false)` when the file does not exist
    /// (cold-start is not an error); returns `Ok(true)` on successful load.
    /// The bandit field is populated in place via
    /// [`GranularityBandit::from_snapshot`].
    ///
    /// # Errors
    ///
    /// - Returns `Err` when the snapshot file exists but cannot be read
    ///   (permission, I/O failure).
    /// - Returns `Err` when the JSON is malformed or fails validation
    ///   (version mismatch, incompatible num_arms / feature_dim).
    pub fn load_granularity_bandit(&mut self) -> Result<bool, HookPersistError> {
        use touring_intelligence::rl::bandit::granularity::{
            GranularityBandit, GranularitySnapshot,
        };
        let path = self
            .project_root
            .join(".claude/data/granularity_bandit.json");
        if !path.exists() {
            return Ok(false);
        }
        let data =
            std::fs::read_to_string(&path).map_err(|e| format!("Failed to read {path:?}: {e}"))?;
        let snap: GranularitySnapshot = serde_json::from_str(&data)
            .map_err(|e| format!("Failed to parse granularity snapshot: {e}"))?;
        let bandit = GranularityBandit::from_snapshot(&snap).map_err(|e| e.to_string())?;
        self.learning.granularity_bandit = Some(bandit);
        Ok(true)
    }
    /// Sync LinUCB arm effectiveness to SessionBus after each RL update.
    ///
    /// Reads the current avg_reward per arm from LinUCB and propagates it
    /// to `session_bus.arm_effectiveness` so that pre_read can query
    /// `is_arm_productive(arm_id)` to skip low-value arms.
    pub fn sync_arm_effectiveness(&mut self) {
        if let Some(ref linucb) = self.learning.linucb {
            let stats = linucb.arm_stats();
            let mut bus = self.ctx.session_bus.borrow_mut();
            for (arm_id, _pulls, avg_reward) in stats {
                bus.update_arm_effectiveness(arm_id as u8, avg_reward);
            }
        }
    }
    /// Get or create the polymorphic bandit instance.
    /// Returns a mutable reference to Box\<dyn ContextualBandit\>.
    /// On first call, wraps the existing LinUCB bandit (or creates a new one).
    pub fn get_bandit(&mut self) -> &mut Box<dyn ContextualBandit> {
        if self.learning.bandit.is_none() {
            let linucb = self.learning.linucb.take().unwrap_or_default();
            self.learning.bandit = Some(Box::new(linucb));
        }
        self.learning.bandit.as_mut().expect("just created")
    }
    /// Save the polymorphic bandit state to disk via snapshot.
    pub fn save_bandit(&self) -> Result<(), HookPersistError> {
        if let Some(ref bandit) = self.learning.bandit {
            let snapshot = bandit.export_snapshot();
            let path = self.project_root.join(".claude/data/bandit_snapshot.json");
            let json = serde_json::to_string(&snapshot)
                .map_err(|e| format!("Failed to serialize bandit snapshot: {e}"))?;
            std::fs::write(&path, json)
                .map_err(|e| format!("Failed to write bandit snapshot: {e}"))?;
        }
        Ok(())
    }
    /// Process an immediate reward signal from a PostToolUse event.
    /// Feeds the signal to the OnlineRLEngine for n-step TD learning.
    /// Requires mutable access to the LinUCB bandit and a QTable.
    ///
    /// Handles the dual-path issue: if `get_bandit()` was called earlier (which
    /// moves linucb into the Box), we ensure linucb is still available for the
    /// OnlineRLEngine by creating a fresh one. The polymorphic bandit in the Box
    /// tracks its own state independently.
    pub fn process_immediate_reward(
        &mut self,
        reward: &ImmediateReward,
        qtable: &mut touring_intelligence::rl::QTable,
    ) {
        if self.learning.linucb.is_none() {
            self.learning.linucb = Some(LinUCBBandit::new());
        }
        if let Some(mut engine) = self.learning.online_rl.take() {
            if let Some(ref mut linucb) = self.learning.linucb {
                engine.process_reward(reward, qtable, linucb);
            }
            self.learning.online_rl = Some(engine);
        }
    }
}

/// Walk upward from `start` searching for the nearest `Cargo.toml` declaring
/// a `[workspace]` table. Returns the directory that owns that manifest.
///
/// Cargo enforces that workspaces are not nested, so the first `[workspace]`
/// match is unambiguous — there cannot be another workspace above it.
///
/// The check is intentionally conservative: it looks for a literal
/// `[workspace]` section header at the start of a line (after whitespace) so
/// the heuristic does not parse TOML, avoiding a dependency on `toml::de` in
/// the hot path of every hook startup.
pub(crate) fn find_cargo_workspace_root(start: &std::path::Path) -> Option<PathBuf> {
    let mut dir = start.to_path_buf();
    loop {
        let manifest = dir.join("Cargo.toml");
        if manifest.exists() {
            if let Ok(contents) = std::fs::read_to_string(&manifest) {
                let has_workspace = contents
                    .lines()
                    .any(|l| l.trim_start().starts_with("[workspace]"));
                if has_workspace {
                    return Some(dir);
                }
            }
        }
        if !dir.pop() {
            return None;
        }
    }
}
/// Hash a file path to a CrdtNodeId (u64).
pub(crate) fn hash_path(path: &str) -> CrdtNodeId {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    path.hash(&mut hasher);
    hasher.finish()
}
/// Hash an arbitrary string to a u64 state/action identifier for ACO wiring.
pub fn hash_str(s: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    s.hash(&mut hasher);
    hasher.finish()
}
impl std::fmt::Debug for HookRuntime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HookRuntime")
            .field("knowledge", &self.ctx.knowledge)
            .field("project_root", &self.project_root)
            .field("quality_assessment", &self.ctx.quality_assessment)
            .field("result_cache", &"HookResultCache { .. }")
            .field("linucb", &self.learning.linucb.is_some())
            .field("symbol_store", &self.infra.symbol_store.is_some())
            .field("symbol_index", &self.infra.symbol_index.is_some())
            .field("pipeline", &self.infra.pipeline.is_some())
            .field("predictor", &self.learning.predictor.is_some())
            .field("crdt_graph", &self.learning.crdt_graph.is_some())
            .field("aco_wiring", &"Mutex<AcoWiringState>")
            .finish()
    }
}
#[cfg(test)]
#[path = "hook_runtime_tests.rs"]
mod tests;

/// Wall-clock timer for hook latency measurement.
///
/// P3.2: Lightweight wrapper around `std::time::Instant` that captures
/// elapsed time in microseconds for latency tracking and reporting.
#[derive(Debug, Clone)]
pub struct HookTimer {
    start: std::time::Instant,
    hook_name: String,
}
impl HookTimer {
    /// Start a new timer for a named hook.
    pub fn start(hook_name: &str) -> Self {
        Self {
            start: std::time::Instant::now(),
            hook_name: hook_name.to_string(),
        }
    }
    /// Elapsed time in microseconds.
    pub fn elapsed_us(&self) -> u64 {
        self.start.elapsed().as_micros() as u64
    }
    /// Elapsed time in milliseconds.
    pub fn elapsed_ms(&self) -> u64 {
        self.start.elapsed().as_millis() as u64
    }
    /// Return hook name and elapsed time as a tuple.
    pub fn finish(&self) -> (String, u64) {
        (self.hook_name.clone(), self.elapsed_us())
    }
    /// Check if elapsed time exceeds a threshold in milliseconds.
    pub fn exceeds_ms(&self, threshold_ms: u64) -> bool {
        self.elapsed_ms() > threshold_ms
    }
}
/// Convert an absolute path to a project-relative path.
///
/// If `path` starts with `project_root`, the prefix is stripped.
/// Otherwise returns the original path unchanged.
pub fn make_relative(path: &str, project_root: &Path) -> String {
    let p = Path::new(path);
    p.strip_prefix(project_root)
        .map(|r| r.to_string_lossy().to_string())
        .unwrap_or_else(|_| path.to_string())
}
