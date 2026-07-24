//! touring-learning — Unified intelligence crate.
//!
//! Merges QTable (TD(λ)), 5-tier RLM Memory, SemanticRecall,
//! WilsonRanker, DriftDetector, Evolution, Clustering, ACO, Templates, and Observability.
//!
//! # Architecture
//!
//! ```text
//! touring-learning
//! ├── rl/              QTable TD(λ) with eligibility traces
//! ├── memory/          RLM (SQLite), SemanticRecall (FTS5), WorkingMemory (LRU+SIMD),
//! │                     CrdtDelta (CRDT sync), rle (Run-Length Encoding compression)
//! ├── evolution/       EvolutionAnalyzer, InsightEngine, LearningPersistence
//! ├── ranking/         WilsonRanker, DriftDetector, DriftMonitor (KS-test), EbpfObserver (stub)
//! ├── clustering/      SkillClusterer (cosine similarity)
//! ├── aco/             Models, GeneratorGraph (DAG), GoalTracker, ESAA (QueryCache, EventBuffer)
//! ├── n1/              ToolSequenceGenerator + TRIAD (execute+validate+rollback)
//! ├── n3/              MetaGenerator for domain-specific generator specification
//! ├── templates/       Self-improving context injection templates (UCB1 + mutation)
//! ├── observability/   RingBuffer (fixed-capacity circular buffer, FIFO overwrite)
//! ├── online_learning/ FTRL feature importance layer (feature = "ftrl")
//! └── bandit/          LinUCB, AstEnrichedBandit, TransferLinUCB, ReminderBandit
//! ```
//!
//! # Public API
//!
//! All items below are re-exported at the crate root for ergonomic access.
//!
//! ## Error
//! - `LearningError`, `LearningResult` — unified error type for all learning crate operations
//!
//! ## Bandit / RL
//! - `LinUCBBandit`, `extract_features`, `FEATURE_DIM`, `NUM_ARMS`, `ArmKind` — LinUCB contextual bandit with feature extraction
//! - `AstEnrichedBandit`, `FEATURE_DIM_AST` — AST-enriched bandit (symbol complexity, kind, blast radius)
//! - `TransferLinUCB` — transfer LinUCB across related tasks
//! - `BanditSnapshot`, `ContextualBandit` — bandit state persistence
//! - `ReminderBandit`, `ReminderContext`, `ReminderKind` — adaptive reminder injection via LinUCB
//! - `AdaptiveAlpha` — adaptive alpha for exponential moving average
//!
//! ## RL
//! - `QLearning`, `QTable` — Q-learning with TD(λ) eligibility traces
//! - `RiskAdjustedQLearning` — risk-adjusted Q-learning (penalizes high-variance outcomes)
//! - `QLearningMetrics`, `RewardBreakdown`, `RewardStats` — Q-table diagnostics
//! - `CuriosityModule`, `DoubleQTable`, `PrioritizedReplayBuffer` — curiosity-driven exploration, double Q-learning, prioritised replay
//!
//! ## Memory
//! - `RlmMemory`, `AsyncRlmMemory` — persistent episodic memory (SQLite WAL)
//! - `SemanticRecall` — full-text search over memory via FTS5
//! - `LruWorkingMemory`, `WorkingMemory` — in-process LRU cache
//! - `RecallCache` — memoised recall results
//! - `MemoryMatch` — recall query result type
//! - `GraphMeta`, `MemoryTier` — RLM memory graph metadata and tier classification
//! - `ActorId`, `CrdtDelta`, `CrdtNodeId`, `CrdtSemanticGraph`, `NodeWeight` — CRDT semantic graph
//!
//! ## Ranking
//! - `WilsonRanker` — Wilson score confidence interval ranker
//! - `DriftDetector`, `DriftMonitor`, `DriftReport` — KS-test based drift tracking
//! - `EbpfObserver` — (stub) kernel-level latency observer
//! - `LatencyDriftDetector` — latency-specific drift monitor
//! - `DriftSignal`, `DriftDirection` — CUSUM drift signal and direction
//! - `RankedItem`, `WilsonScore` — Wilson score ranked item and score computation
//! - `PageHinkley` — Page Hinkley change detection algorithm
//!
//! ## Meta-learning
//! - `HyperparamAdjustment`, `SelfOptimizer` — hyperparameter adjustment and self-optimisation
//! - `EvolutionCandidate`, `OpportunityCategory`, `OpportunityScorer`, `OpportunitySummary`, `TelemetrySnapshot` — evolution candidate tracking and opportunity scoring
//!
//! ## Observability
//! - `RingBuffer` — fixed-capacity circular buffer (FIFO overwrite)
//!
//! ## Clustering
//! - `SkillClusterer` — cosine similarity based skill grouping
//!
//! ## Evolution
//! - `EvolutionAnalyzer`, `InsightEngine`, `LearningPersistence` — pattern drift detection, insight generation, SQLite persistence
//!
//! ## ACO (Ant Colony Optimization)
//! - `MutableGeneratorGraph`, `GraphError` — DAG of ACO generators with pheromone state
//! - `build_report`, `compute_composite`, `determine_status`, `TrackerReport`, `TrackerStatus` — ACO reporting
//! - `PheroKey`, `UnifiedPheromoneBus` — pheromone key and unified pheromone bus for inter-hook communication
//! - `MultiObjectiveMapping`, `TrackerRlBridge` — multi-objective ACO mapping and RL bridge for TD(λ) reward propagation
//!
//! ## Templates
//! - `ContextTemplate`, `TemplateLibrary` — self-improving context injection templates with UCB1 mutation
//!
//! ## Online RL
//! - `OnlineRLEngine`, `OnlineRLConfig` — online RL engine and configuration
//! - `Experience`, `ReplayBuffer` — experience tuple and replay buffer
//! - `ImmediateReward` — reward signal type for online updates
//!
//! ## FTRL (feature-gated)
//! - `FtrlConfig`, `FtrlLayer` — Follow-The-Regularised-Leader online learning layer

// Tier A (2026-04-19): test-only prelude with pretty_assertions shadow.
#[cfg(test)]
pub(crate) mod test_util;

pub mod aco;
pub mod bandit;
pub mod clustering;
pub mod constants;
pub mod data;
pub mod error;
pub mod evolution;
pub mod experiment_log;
pub mod learning_signals;
pub mod memory;
pub mod meta;
pub mod metacognitive_pipeline;
pub mod n1;
pub mod n3;
pub mod observability;
pub mod online_learning;
pub mod online_rl;
pub mod ranking;
#[allow(clippy::module_inception)]
pub mod rl;
pub mod semantic;
pub mod session_bus_evidence_subscriber;
pub mod streaming_hook_integration;
pub mod templates;

// ── Top-level re-exports ────────────────────────────────────────────────

// Error
/// Unified error type for all learning crate operations.
pub use error::{LearningError, LearningResult};

// Bandit
/// LinUCB contextual bandit with feature extraction.
/// - `LinUCBBandit`: primary bandit algorithm
/// - `extract_features`: converts context into feature vectors
/// - `FEATURE_DIM` / `NUM_ARMS`: algorithm constants
pub use bandit::{ArmKind, FEATURE_DIM, LinUCBBandit, LinUcbError, NUM_ARMS, extract_features};

/// AST-enriched bandit — uses symbol complexity, kind, and blast radius as context.
/// - `AstEnrichedBandit`: extends LinUCB with AST-derived features
/// - `FEATURE_DIM_AST`: feature dimensionality for AST context
pub use bandit::{AstEnrichedBandit, FEATURE_DIM_AST};

/// Transfer LinUCB — transfers learned arms across related tasks.
pub use bandit::TransferLinUCB;

/// Snapshot and trait for bandit state persistence.
pub use bandit::{BanditSnapshot, ContextualBandit};

/// Adaptive reminder injection via LinUCB bandit.
/// - `ReminderBandit`: selects reminder type based on context
/// - `ReminderContext` / `ReminderKind`: context parameters and reminder variants
pub use bandit::{ReminderBandit, ReminderContext, ReminderKind};

// RL
/// Q-learning with TD(λ) eligibility traces.
/// - `QLearning`: main Q-learning algorithm
/// - `QTable`: hash-map backed Q-value table
pub use rl::{QLearning, QTable, QTableError};

/// Risk-adjusted Q-learning — penalises high-variance outcomes.
pub use rl::RiskAdjustedQLearning;

/// Q-table metrics and reward breakdowns for diagnostics.
pub use rl::qtable::{QLearningMetrics, RewardBreakdown, RewardStats};

/// Curiosity-driven exploration, double Q-learning, and prioritised replay.
pub use rl::{CuriosityModule, DoubleQTable, PrioritizedReplayBuffer};

/// Actor-Critic: policy gradient (actor) + value estimation (critic).
/// - `ActorCritic`: combined agent with actor network and critic network
/// - `ActorCriticConfig` / `ActorCriticStats`: configuration and learning statistics
/// - `ActionSelection`: action selection result with exploration info
pub use rl::actor_critic::{ActionSelection, ActorCritic, ActorCriticConfig, ActorCriticStats};

// Memory
/// Memory hierarchy — RLM (SQLite WAL), semantic recall (FTS5), LRU working memory.
/// - `RlmMemory` / `AsyncRlmMemory`: persistent episodic memory
/// - `SemanticRecall`: full-text search over memory via FTS5
/// - `LruWorkingMemory` / `WorkingMemory`: in-process LRU cache
/// - `RecallCache`: memoised recall results
/// - `MemoryMatch`: recall query result type
pub use memory::{
    AsyncRlmMemory, LruWorkingMemory, MemoryMatch, PalaceHierarchy, RecallCache, RlmMemory,
    SemanticRecall, WorkingMemory,
};

/// RLM memory graph metadata and tier classification.
pub use memory::rlm::{GraphMeta, MemoryTier};

/// CRDT semantic graph for distributed graph state synchronization.
pub use memory::crdt_graph::{ActorId, CrdtDelta, CrdtNodeId, CrdtSemanticGraph, NodeWeight};

// Ranking
/// Ranking and drift detection — Wilson score, CUSUM, KS-test.
/// - `WilsonRanker`: Wilson score confidence interval ranker
/// - `DriftDetector` / `DriftMonitor` / `DriftReport`: KS-test based drift tracking
/// - `EbpfObserver`: (stub) kernel-level latency observer
/// - `LatencyDriftDetector`: latency-specific drift monitor
pub use ranking::{
    DriftDetector, DriftMonitor, DriftReport, EbpfObserver, LatencyDriftDetector, WilsonRanker,
};

/// CUSUM drift signal and direction enum.
pub use ranking::cusum::{DriftDirection, DriftSignal};

/// Wilson score ranked item and score computation.
pub use ranking::wilson::{RankedItem, WilsonScore};

/// Page Hinkley change detection algorithm.
pub use ranking::PageHinkley;

/// Adaptive alpha for exponential moving average.
pub use bandit::AdaptiveAlpha;

// Meta-learning
/// Meta-learning — hyperparameter adjustment and self-optimisation.
/// - `HyperparamAdjustment`: suggested parameter changes from learning feedback
/// - `SelfOptimizer`: orchestrates the self-improvement cycle
pub use meta::{HyperparamAdjustment, SelfOptimizer};

/// Evolution candidate tracking and opportunity scoring.
/// - `EvolutionCandidate`: a potential improvement with expected uplift
/// - `OpportunityCategory` / `OpportunityScorer` / `OpportunitySummary`: scoring pipeline
/// - `TelemetrySnapshot`: point-in-time system metrics for comparison
pub use meta::{
    EvolutionCandidate, OpportunityCategory, OpportunityScorer, OpportunitySummary,
    TelemetrySnapshot,
};

// Latency Adaptation Pipeline
/// Latency adaptation pipeline — unified CUSUM + ACO + Actor-Critic for hook adaptation.
/// Not to be confused with touring_simd::cortex::MetacognitivePipeline which handles
/// evidence fusion via DriftDetector+Wilson. This pipeline handles latency adaptation
/// via CUSUM drift detection, ACO pheromone threshold adaptation, and Actor-Critic RL
/// for hook execution parallelism tuning.
/// - `LatencyAdaptationPipeline`: main pipeline combining all three mechanisms
/// - `PipelineContext` / `PipelineContextBuilder`: input context for a hook observation
/// - `MetacognitiveDecision`: decision output (Stable, IncreaseParallelism, etc.)
/// - `DecisionMetadata`: metadata about how the decision was reached
/// - `EvidenceSource`: source of evidence contributing to the decision
/// - `ExplorationAction`: exploration action suggested by Actor-Critic
/// - `FalliblePipeline`: error-handling wrapper for production use
pub use metacognitive_pipeline::{
    DecisionMetadata, EvidenceSource, ExplorationAction, FalliblePipeline,
    LatencyAdaptationPipeline, LatencyConfig, LatencyStats, MetacognitiveDecision, PipelineContext,
    PipelineContextBuilder,
};
// EvidenceSubscriber requires async-memory (tokio); SyncEvidenceProcessor is always available
#[cfg(not(feature = "async-memory"))]
pub use session_bus_evidence_subscriber::SyncEvidenceProcessor;
#[cfg(feature = "async-memory")]
pub use session_bus_evidence_subscriber::{EvidenceSubscriber, SyncEvidenceProcessor};

// Observability
/// Fixed-capacity circular buffer for time-series observability metrics.
/// FIFO overwrite when capacity is reached.
pub use observability::{RingBuffer, RlMetrics, RlMetricsCollector};

// Data loading — audit, checkpoint, stats, telemetry
pub use data::{AuditLoader, CheckpointLoader, SessionStats, TelemetryLoader};
pub use experiment_log::ExperimentLog;

// Clustering
/// Skill clusterer — cosine similarity based grouping of related skills.
pub use clustering::SkillClusterer;
/// Leiden community detector — graph-based skill clustering via Leiden algorithm.
pub use clustering::{
    Community, Graph, LeidenAlgorithmConfig, LeidenCommunityDetector, LeidenResult,
};

// Evolution
/// Evolution analysis — pattern drift, insight generation, persistence.
/// - `EvolutionAnalyzer`: detects drift across all tracked metrics
/// - `InsightEngine`: generates actionable insights from evolution data
/// - `LearningPersistence`: SQLite-backed persistence for evolution state
pub use evolution::{EvolutionAnalyzer, InsightEngine, LearningPersistence};

// ACO
/// Ant Colony Optimisation — pheromone graph, generator, and tracker.
/// - `MutableGeneratorGraph`: DAG of ACO generators with pheromone state
/// - `GraphError`: graph operation errors
/// - `build_report` / `compute_composite` / `determine_status` / `TrackerReport` / `TrackerStatus`: ACO reporting
pub use aco::{
    GraphError, MutableGeneratorGraph, TrackerReport, TrackerStatus, build_report,
    compute_composite, determine_status,
};

/// Pheromone key and unified pheromone bus for inter-hook communication.
pub use aco::{PheroKey, UnifiedPheromoneBus};

/// External metrics surface for [`UnifiedPheromoneBus::evaporation_rate()`].
pub use crate::rl::aco::pheromone_bus_metrics::EvaporationRateMetrics;

/// Multi-objective ACO mapping and RL bridge for TD(λ) reward propagation.
pub use aco::{MultiObjectiveMapping, TrackerRlBridge};

// Templates
/// Self-improving context injection templates with UCB1 mutation.
/// - `ContextTemplate`: individual template with mutation operators
/// - `TemplateLibrary`: collection with UCB1-based selection
pub use templates::{ContextTemplate, TemplateLibrary};

// Online RL
pub use online_rl::{Experience, ImmediateReward, OnlineRLConfig, OnlineRLEngine, ReplayBuffer};
/// Online RL engine with experience replay and immediate reward signalling.
/// - `OnlineRLEngine` / `OnlineRLConfig`: engine and its configuration
/// - `Experience` / `ReplayBuffer`: experience tuple and replay buffer
/// - `ImmediateReward`: reward signal type for online updates
///
/// Bridge from ACO hook quality assessments to RL subsystems.
/// - `HookQualitySummary`: DTO summarizing hook quality metrics
/// - `HookStatsConsumer`: trait for consumers of hook quality data
/// - `HookQualityBuffer`: ring buffer for recent hook quality summaries
pub use streaming_hook_integration::{HookQualityBuffer, HookQualitySummary, HookStatsConsumer};

// FTRL online learning layer (feature-gated)
/// FTRL (Follow-The-Regularised-Leader) online learning layer.
#[cfg(feature = "ftrl")]
pub use online_learning::ftrl::{FtrlConfig, FtrlLayer};
