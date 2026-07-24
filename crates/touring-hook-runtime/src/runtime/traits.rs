//! Public traits for HookRuntime extensibility.
//!
//! These traits expose HookRuntime's functionality in a composable, testable way.
//! Implementors can provide alternative backends (e.g., mock for testing,
//! remote proxy for distributed setups) while maintaining backward compatibility
//! with the concrete HookRuntime implementation.
//!
//! # Design Principles
//!
//! - **Coarse-grained traits**: Each trait groups related functionality by domain
//! - **Sync-first**: Traits use `&self` where possible for maximum compatibility
//! - **No runtime overhead**: Default implementations provided where sensible
//! - **Testable**: All traits can be implemented on mock types for unit testing

use std::path::{Path, PathBuf};

use touring_code::ast::graph::SymbolLocation;
use touring_code::ast::{
    BlastRadiusOutput, IncrementalEditResult, SharedPipeline, SymbolIndex, SymbolStore,
};
use touring_intelligence::reasoning::EnrichedCtx;
use touring_intelligence::rl::aco::tracker::TrackerReport;
use touring_intelligence::rl::bandit::ContextualBandit;
use touring_intelligence::rl::bandit::linucb::ArmKind;
use touring_intelligence::rl::{EvolutionAnalyzer, LinUCBBandit, OnlineRLEngine};

use crate::aco_bridge::{HookOutcome, HookQualityAssessment, HookResultCache};
use crate::async_knowledge::AsyncFileKnowledgeDB;
use crate::dependency_cache::DependencyCache;
use crate::inferlets::{InferletKind, InferletService};
use crate::knowledge::FileKnowledgeDB;
use crate::metrics::RuntimeMetrics;
use crate::{HookResponse, IntentClassifier, PIIScanner};

// ── HookExecutor ─────────────────────────────────────────────────────────

/// Hook execution and response building.
///
/// Provides non-diverging alternatives to the hook protocol's exit-based output.
/// All methods return `HookResponse` instead of calling `process::exit`.
pub trait HookExecutor {
    /// Build an "allow" response (no context, silent pass).
    ///
    /// Unlike `emit_allow()`, this does NOT call `process::exit`.
    fn build_allow(&self) -> HookResponse;

    /// Build a context response (injects additionalContext).
    ///
    /// Unlike `emit_context()`, this does NOT call `process::exit`.
    fn build_context(&self, context: &str) -> HookResponse;

    /// Build a context response with an explicit event name.
    ///
    /// Unlike `emit_context_for_event()`, this does NOT call `process::exit`.
    fn build_context_for_event(&self, context: &str, event_name: &str) -> HookResponse;

    /// Detect project root from `CLAUDE_PROJECT_DIR` env var or cwd.
    fn detect_project_root(&self) -> PathBuf;
}

// ── KnowledgeDB ─────────────────────────────────────────────────────────

/// Knowledge database operations for file metadata, relations, and gotchas.
///
/// Abstracts over the synchronous `FileKnowledgeDB` with optional async support.
pub trait KnowledgeDB {
    /// Record a hook execution outcome in the quality assessment.
    ///
    /// No-op if quality tracking has not been initialized.
    fn record_hook_outcome(&mut self, outcome: HookOutcome);

    /// Generate a quality report for the current session.
    ///
    /// Returns `None` if quality tracking has not been initialized.
    fn quality_report(&self, iteration: u32) -> Option<TrackerReport>;

    /// Initialize or reset quality tracking for a new session.
    fn reset_quality_tracking(&mut self, session_id: &str);

    /// Get the knowledge DB reference.
    fn knowledge_db(&self) -> &FileKnowledgeDB;

    /// Get async knowledge DB if initialized.
    fn async_knowledge_db(&self) -> Option<&AsyncFileKnowledgeDB>;

    /// Get the quality assessment if initialized.
    fn quality_assessment(&self) -> Option<&HookQualityAssessment>;

    /// Initialize the async knowledge database using deadpool-sqlite.
    fn init_async_knowledge(&mut self);

    /// Batch-fetch pre-read signals for a file.
    ///
    /// Returns a struct containing gotchas, latest failure, and dependent names.
    fn batch_pre_read_signals(&self, file_path: &str) -> crate::knowledge::PreReadSignals;
}

// ── ResultCache ─────────────────────────────────────────────────────────

/// Result cache for hook outcomes.
///
/// Avoids recomputing hook results for unchanged files.
pub trait ResultCache {
    /// Check the result cache before computing a hook result.
    ///
    /// Returns `Some(cached_json)` on cache hit, `None` on miss.
    fn check_cache(&self, hook_name: &str, file_path: &str) -> Option<String>;

    /// Store a computed hook result in the cache.
    fn store_cache(&self, hook_name: &str, file_path: &str, result_json: String);

    /// Invalidate all cached results for a file (after edit).
    ///
    /// Returns the number of entries invalidated.
    fn invalidate_cache_for_file(&self, file_path: &str) -> usize;

    /// Get the current cache hit rate (0.0 - 1.0).
    fn cache_hit_rate(&self) -> f64;

    /// Get the underlying cache reference.
    fn cache_ref(&self) -> &HookResultCache;
}

// ── AcoWiring ───────────────────────────────────────────────────────────

/// ACO Wiring operations for pheromone-based routing and learning.
///
/// UnifiedPheromoneBus + TrackerRlBridge E2E chains for task routing,
/// teammate productivity, and limbo detection.
pub trait AcoWiring {
    /// Cadeia 1: deposit file-edit heat on the bus and trigger periodic evaporation.
    ///
    /// Called from `post_edit::run()` after every successful edit.
    fn deposit_file_edit(&self, file_path: &str);

    /// Cadeia 2: distribute a `TrackerReport` to all 3 RL subsystems.
    ///
    /// Deposits Pareto objectives into MultiObjectivePheromonoLayer,
    /// scalar reward into the bus, and registers outcome in SessionPredictor.
    fn process_tracker_report(&self, report: &TrackerReport, state: u64, action: u64);

    /// Deposit task completion heat on the bus.
    ///
    /// Tracks which tasks are "hot" (frequently completed) for team routing.
    fn deposit_task_completion(&self, task_id: &str, success: bool);

    /// Deposit teammate idle heat on the bus.
    ///
    /// Tracks teammate reliability — idle after work means productive.
    fn deposit_teammate_idle(&self, teammate_id: &str, tasks_completed: u32);

    /// Deposit limbo pattern heat on the bus.
    ///
    /// Called when a teammate idles with uncompleted or blocked tasks.
    fn deposit_teammate_limbo(&self, teammate_id: &str, blocked_count: u32, uncompleted_count: u32);

    /// Query task heat for routing decisions.
    fn task_heat(&self, task_id: &str) -> f64;

    /// Query teammate heat for routing decisions.
    fn teammate_heat(&self, teammate_id: &str) -> f64;

    /// Query limbo heat for a teammate.
    fn limbo_heat(&self, teammate_id: &str) -> f64;

    /// F4-2: Flush ACO metrics to SessionBus via callback.
    ///
    /// Pushes stored Pareto objectives and scalar reward as arm effectiveness
    /// signals. Called from `post_tool_rl` after each tool execution.
    fn flush_aco_metrics_to_bus<F>(&self, push: F)
    where
        F: FnMut(u8, f64);
}

// ── SymbolIndexOps ──────────────────────────────────────────────────────

/// Symbol store and index operations for cross-session persistence.
///
/// P4.3/P4.4: SymbolStore for persistence + SymbolIndex for fast lookups.
pub trait SymbolIndexOps {
    /// Get a reference to the SymbolStore, if available.
    fn symbol_store(&self) -> Option<&SymbolStore>;

    /// Get or initialize the in-memory SymbolIndex.
    fn get_symbol_index(&mut self) -> &SymbolIndex;

    /// Find all locations of a symbol by name.
    fn find_symbol(&mut self, name: &str) -> Vec<SymbolLocation>;

    /// Calculate blast radius for a file with depth limit.
    ///
    /// Returns `BlastRadiusOutput::Rich` with full metadata.
    fn blast_radius(&mut self, file_path: &str, max_depth: usize) -> BlastRadiusOutput;

    /// Persist symbols for a file into the SymbolStore.
    fn persist_symbols(&self, file_path: &str, symbols: &[SymbolLocation]) -> Result<(), String>;

    /// Get the underlying symbol index reference (requires mutable access).
    fn symbol_index_ref(&self) -> Option<&SymbolIndex>;
}

// ── Pipeline ────────────────────────────────────────────────────────────

/// Incremental AST pipeline operations for O(edit) re-parsing.
///
/// P7.4: Incremental pipeline backed by SQLite for cross-session caching.
pub trait Pipeline {
    /// Process a file through the incremental pipeline (full parse on first call).
    ///
    /// Subsequent calls with the same `file_path` but different content use
    /// the cached tree for an O(edit) incremental re-parse.
    fn process_file(&self, file_path: &str, content: &str)
    -> Result<IncrementalEditResult, String>;

    /// Get cached symbols for a file from the incremental pipeline.
    ///
    /// Returns symbols from the pipeline's in-memory tree cache (O(1) lookup).
    fn get_cached_symbols(&self, file_path: &str) -> Vec<SymbolLocation>;

    /// Get the incremental pipeline's cache stats: (documents, trees).
    fn pipeline_cache_stats(&self) -> Option<(usize, usize)>;

    /// Get the underlying pipeline reference.
    fn pipeline_ref(&self) -> Option<&SharedPipeline>;
}

// ── DependencyCache ─────────────────────────────────────────────────────

/// Dependency cache operations for O(V+E) blast radius BFS.
///
/// H1-C: In-memory petgraph-backed dependency graph.
pub trait DependencyCacheOps {
    /// Build the in-memory petgraph DependencyCache from SQLite file relations.
    ///
    /// Idempotent — reinitializes on every call.
    fn init_dependency_cache(&mut self);

    /// Register a new dependency edge in the in-memory cache (incremental update).
    fn add_dependency(&mut self, from: &Path, to: &Path);

    /// Invalidate a file's edges in the cache (clears stale blast_radius data).
    fn invalidate_dependency_cache_for_file(&mut self, path: &Path);

    /// Compute transitive blast_radius via petgraph BFS.
    ///
    /// Returns `None` if the cache has not been initialized.
    fn petgraph_blast_radius(&self, path: &Path) -> Option<Vec<PathBuf>>;

    /// Get the underlying dependency cache reference.
    fn dependency_cache_ref(&self) -> Option<&DependencyCache>;
}

// ── LinUCB Bandit ───────────────────────────────────────────────────────

/// LinUCB adaptive context selection operations.
///
/// P4.1/P4.2: Adaptive context injection using LinUCB bandit.
pub trait LinUCBBanditOps {
    /// Get or create the LinUCB bandit instance.
    fn linucb_bandit_mut(&mut self) -> &mut LinUCBBandit;

    /// Select the best context injection strategy for the given file context.
    ///
    /// Returns the `ArmKind` (context strategy) and the UCB score.
    fn select_context_strategy(
        &mut self,
        file_type: &str,
        file_size: usize,
        session_turn: usize,
        recent_errors: usize,
        cila_level: usize,
    ) -> (ArmKind, f64);

    /// Record a reward for a context injection strategy.
    #[allow(clippy::too_many_arguments)]
    fn record_context_reward(
        &mut self,
        arm: usize,
        file_type: &str,
        file_size: usize,
        session_turn: usize,
        recent_errors: usize,
        cila_level: usize,
        reward: f64,
    );

    /// P4.2: Suggest a context verbosity level using QTable.
    ///
    /// Maps the current state to one of 4 levels (0-3).
    #[allow(clippy::too_many_arguments)]
    fn suggest_context_level(
        &mut self,
        file_type: &str,
        file_size: usize,
        session_turn: usize,
        recent_errors: usize,
        cila_level: usize,
    ) -> u8;

    /// Persist LinUCB state to disk.
    fn save_linucb(&self) -> Result<(), String>;
}

// ── Polymorphic Bandit ───────────────────────────────────────────────────

/// Polymorphic bandit access for R3: supports LinUCB, AstEnriched, Transfer.
pub trait PolymorphicBandit {
    /// Get or create the polymorphic bandit instance.
    ///
    /// Returns a mutable reference to Box\<dyn ContextualBandit\>.
    fn get_bandit_mut(&mut self) -> &mut Box<dyn ContextualBandit>;

    /// Save the polymorphic bandit state to disk via snapshot.
    fn save_bandit(&self) -> Result<(), String>;
}

// ── OnlineRL ────────────────────────────────────────────────────────────

/// Online RL engine operations for R6: n-step TD, EMA smoothing.
pub trait OnlineRLOps {
    /// Process an immediate reward signal from a PostToolUse event.
    fn process_immediate_reward(&mut self, reward: &touring_intelligence::rl::ImmediateReward);

    /// Get a reference to the OnlineRL engine, if available.
    fn online_rl_engine(&self) -> Option<&OnlineRLEngine>;
}

// ── Session ─────────────────────────────────────────────────────────────

/// Session management operations.
pub trait Session {
    /// Return the current session turn (number of pre-hook dispatches so far).
    fn session_turn(&self) -> usize;

    /// Increment the turn counter and return the new value.
    fn advance_session_turn(&self) -> usize;
}

// ── Cognitive ───────────────────────────────────────────────────────────

/// Cognitive engine operations for semantic enrichment.
pub trait Cognitive {
    /// Initialize the cognitive engine with a thread-safe knowledge source.
    fn init_cognitive(&mut self);

    /// Resolve enriched cognitive context for a tool invocation.
    ///
    /// Returns None if cognitive engine is not initialized.
    #[allow(async_fn_in_trait)]
    async fn resolve_cognitive_context(
        &self,
        tool_name: &str,
        file_path: Option<&str>,
        query_hint: &str,
    ) -> Option<EnrichedCtx>;

    /// Save cognitive graph state for cross-session transfer learning.
    fn save_cognitive_state(&self) -> Result<(), String>;

    /// Get the cognitive runtime reference if initialized.
    fn cognitive_ref(&self) -> Option<&touring_intelligence::reasoning::CognitiveRuntime>;
}

// ── Inferlets ───────────────────────────────────────────────────────────

/// WASM inferlet evaluation operations.
pub trait Inferlets {
    /// Initialize the WASM inferlet service and load all compiled inferlets.
    ///
    /// Loads all 4 inferlet types into `AsyncInferletPool` instances.
    #[allow(async_fn_in_trait)]
    async fn init_inferlets(&mut self, pool_size: usize);

    /// Evaluate input using a loaded WASM inferlet.
    ///
    /// Returns `None` if the inferlet service is not initialized
    /// or if the requested `kind` is not loaded.
    #[allow(async_fn_in_trait)]
    async fn evaluate_inferlet(
        &self,
        kind: InferletKind,
        input: &str,
    ) -> Option<touring_bindings::wasm::PluginResult>;

    /// Get the inferlet service reference if initialized.
    fn inferlet_service_ref(&self) -> Option<&InferletService>;
}

// ── Tool Prediction ─────────────────────────────────────────────────────

/// Tool sequence prediction operations for R17.
pub trait ToolPredictor {
    /// Predict the next likely tool(s) based on recent tool history.
    ///
    /// Returns top-k predictions as `ToolPrediction` (tool_name, confidence).
    fn predict_next_tools(
        &self,
        tool_history: &[String],
        cila_level: u8,
    ) -> Vec<touring_intelligence::rl::rl::tiny_transformer::ToolPrediction>;

    /// Get the predictor reference if initialized.
    fn predictor_ref(
        &self,
    ) -> Option<&touring_intelligence::rl::rl::tiny_transformer::TinyTransformerPredictor>;

    /// Returns prediction accuracy if available.
    ///
    /// None means accuracy has not been computed yet.
    fn accuracy(&self) -> Option<f64> {
        None
    }
}

// ── CRDT Graph ─────────────────────────────────────────────────────────

/// CRDT semantic graph operations for R18: multi-agent knowledge sharing.
pub trait CrdtGraph {
    /// Record a file relationship in the CRDT graph.
    ///
    /// Creates nodes for both files and an edge between them.
    fn record_file_relation(&mut self, from_file: &str, to_file: &str, relation: &str);

    /// Save CRDT graph state to disk.
    ///
    /// No-op if no graph has been initialized or loaded.
    fn save_crdt_graph(&self) -> Result<(), String>;

    /// Load CRDT graph state from disk (L7-B Alpha: warm-start).
    ///
    /// No-op (Ok) if the persisted file does not exist — cold start is legal.
    /// On success, replaces `self.learning.crdt_graph` with the loaded graph.
    fn load_crdt_graph(&mut self) -> Result<(), String>;

    /// Get the CRDT graph reference if initialized.
    fn crdt_graph_ref(
        &self,
    ) -> Option<&touring_intelligence::rl::memory::crdt_graph::CrdtSemanticGraph>;
}

// ── Evolution Analyzer ───────────────────────────────────────────────────

/// Evolution analyzer for tool effectiveness and drift detection.
pub trait Evolution {
    /// Get the evolution analyzer if initialized.
    fn evolution_analyzer_ref(&self) -> Option<&EvolutionAnalyzer>;
}

// ── Metrics Export ──────────────────────────────────────────────────────

/// Metrics export for R19: consolidated runtime metrics.
pub trait MetricsExport {
    /// Export consolidated metrics from all runtime subsystems.
    fn export_metrics(&self, qtable: Option<&touring_intelligence::rl::QTable>) -> RuntimeMetrics;
}

// ── Context Runtime ─────────────────────────────────────────────────────

/// Core context layer access.
pub trait ContextRuntime {
    /// Get the intent classifier.
    fn classifier(&self) -> &IntentClassifier;

    /// Get the PII scanner.
    fn pii_scanner(&self) -> &PIIScanner;

    /// Get the context injection file (S7: set by pre_read, read by post_tool_rl).
    fn context_injection_file(&self) -> Option<&String>;

    /// Set the context injection file.
    fn set_context_injection_file(&mut self, file_path: Option<String>);
}
