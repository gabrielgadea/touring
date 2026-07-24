//! touring-cognitive — Predictive context engine.
//!
//! CognitiveNexus: semantic graph traversal + predictive context.
//! SemanticGraph: StableGraph with focus tracking, decay, and safe node removal.
//! SessionPredictor: session state analysis and next-tool prediction.
//! GoT: Graph of Thoughts — concurrent heuristic evaluation + parallel node eval via JoinSet.
//! GoTSnapshot: rkyv zero-copy serializable snapshots for session persistence.
//! SessionPersistence: async SQLite storage for GoT snapshots via deadpool-sqlite.
//!   IC-2: `explore_with_pheromone_bias()` + `explore_and_reinforce()` close the
//!   ACO loop over thought paths — repeated calls converge toward high-scoring nodes.
//! FocusCache: LRU-16 memoization for graph context lookups.
//! Bridge: integration layer connecting cognitive engine to hooks knowledge.
//! AgentStateMachine: complete agent lifecycle FSM with typed states and transitions — INS-10.
//! PheromoneMCTS: MCTS with shared pheromone (`Arc<Mutex<MctsPheromonoLayer>>`).
//!   IC-1: `with_shared_pheromone()` wires the same layer to AcoRewardPropagator,
//!   enabling the IC-1↔IC-4 E2E feedback loop (MCTS deposits + TD(λ) augmentation).
//!   NOTE: `CognitiveMCTS` is now the canonical public alias for `GraphInformedMCTS`
//!   (graph-informed MCTS). The pheromone-backing variant is `PheromoneMCTS`.
//! PredictiveFocusCache: ACO pheromone-guided prefetch for FocusCache — IC-3.
//! AcoRewardPropagator: TD(λ) backward propagation of RL rewards into MCTS pheromone — IC-4.
//!
//! # ACO Integration Map (IC-1 ↔ IC-4 closed loop)
//!
//! ```text
//! ┌──────────────────────────────────────────────────────────┐
//! │                  Shared Arc<Mutex<MctsPheromonoLayer>>   │
//! │                                                          │
//! │  PheromoneMCTS ──deposit()──────────────────────────┐   │
//! │  (IC-1: search_with_pheromone)                      │   │
//! │                                                      ▼   │
//! │  AcoRewardPropagator ──TD(λ) backward propagation───►   │
//! │  (IC-4: propagate / propagate_from_report)               │
//! │                                                          │
//! │  GotEngine ──explore_and_reinforce()── GotPheromone  │   │
//! │  (IC-2: pheromone-biased GoT search)                 │   │
//! │                                                          │
//! │  PredictiveFocusCache ── SymbolPheromoneMap          │   │
//! │  (IC-3: co-access pheromone → prefetch ranking)         │
//! └──────────────────────────────────────────────────────────┘
//! ```

pub mod aco_adapter;
pub mod aco_reward;
pub mod aco_traits;
pub mod adaptive_engine;
pub mod agent_state_machine;
pub mod ann_index;
pub mod bm25_tfidf;
pub mod bridge;
pub mod coedit_predictor;
pub mod cognitive_mcts;
pub mod consistency_gate; // C14 — GED + cosine merge gate for parallel engineers (FASE 6)
pub mod error;
pub mod focus_cache;
pub mod gated_mcts;
pub mod got;
pub mod hybrid_engine;
pub mod mcts;
pub mod mcts_streaming;
pub mod metrics;
pub mod multi_objective;
pub mod nexus;
pub mod pensieve;
pub mod persistence;
pub mod predictive_focus_cache;
pub mod predictor_task;
pub mod qtable_bridge;
pub mod reasoning_engine;
pub mod refinement;
pub mod rl_bridge;
pub mod semantic_graph;
pub mod session_persistence;
pub mod session_predictor;
pub mod snapshot;
pub mod sqlite_graph;
pub mod tfidf;
pub mod tool_planning; // C12 — MCTS pointed at the tool/command graph (geodesic, TCA-Space)

#[cfg(feature = "analysis-bridge")]
pub mod analysis_bridge;

#[cfg(feature = "analysis-bridge")]
pub use analysis_bridge::{calibration_summary, enrich_with_analysis};

/// ACO reward propagator — TD(λ) backward reward propagation into MCTS pheromone.
/// Closes the IC-4 loop: rewards deposited by PheromoneMCTS are propagated
/// back through the ACO graph via temporal-difference learning.
pub use aco_reward::AcoRewardPropagator;

/// Adaptive cognitive engine — selects and composes reasoning strategies dynamically.
pub use adaptive_engine::AdaptiveEngine;

/// Agent state machine — complete lifecycle FSM with typed states and transitions (INS-10).
/// - `AgentStateMachine`: main FSM orchestrator
/// - `AgentEvent` / `AgentState` / `StateTransition`: event, state, and transition types
pub use agent_state_machine::{AgentEvent, AgentState, AgentStateMachine, StateTransition};

/// Refinement cycle — PLAN → ACT → OBSERVE → REFINE with config and outcomes.
/// - `RefinementConfig` / `RefinementCycle` / `RefinementOutcome`: cycle components
pub use refinement::{RefinementConfig, RefinementCycle, RefinementOutcome};

/// ANN index — approximate nearest-neighbour index for semantic symbol search.
pub use ann_index::AnnIndex;

/// Cognitive bridge — connects hook knowledge DB to the cognitive engine.
/// - `CognitiveRuntime`: main runtime context
/// - `EnrichedCtx`: context enriched with cognitive signals
/// - `KnowledgeSource`: provenance enum for enriched data
pub use bridge::{CognitiveRuntime, EnrichedCtx, KnowledgeSource};

/// Unified error type and result alias for the cognitive crate.
pub use error::{CognitiveError, CognitiveResult};

/// Co-edit predictor — predicts which files will be edited together based on history.
pub use coedit_predictor::CoEditPredictor;

/// Focus cache — LRU-16 memoisation for graph context lookups.
pub use focus_cache::FocusCache;

/// Graph of Thoughts (GoT) — concurrent heuristic evaluation with parallel node eval via JoinSet.
/// - `GotEngine` / `GotNode` / `ThoughtResult`: main GoT types
/// - `run_parallel_nodes`: executes multiple thought nodes concurrently
pub use got::{GotEngine, GotNode, ThoughtResult, run_parallel_nodes};

/// Cognitive MCTS config and graph-informed variant (IC-1).
/// - `CognitiveMCTSConfig`: configuration for the MCTS engine
/// - `GraphInformedMCTS`: MCTS variant that uses call-graph heuristics
/// - `CognitiveMCTS`: canonical public alias for `GraphInformedMCTS` (resolves homonimia with
///   legacy `mcts::CognitiveMCTS` struct, now renamed to `PheromoneMCTS`)
pub use cognitive_mcts::{CognitiveMCTS, CognitiveMCTSConfig, GraphInformedMCTS};

pub use consistency_gate::{ConsistencyVerdict, LabeledGraph, approx_ged, consistency_gate};
/// Monte Carlo Tree Search — shared pheromone layer and engine types.
/// - `MCTSEngine` / `MCTSConfig` / `MCTSResult`: core MCTS
/// - `PheromoneMCTS`: pheromone-backed MCTS variant with shared ACO layer (IC-1)
/// - `MctsPheromonoLayer`: pheromone state shared across MCTS iterations
pub use mcts::{MCTSConfig, MCTSEngine, MCTSResult, MctsPheromonoLayer, PheromoneMCTS};
pub use tool_planning::{ToolGraph, ToolPlan, plan_tool_chain};

/// ACO pheromone layer trait — formalizes the interface for pheromone-based ACO layers.
/// Allows any implementation (MctsPheromonoLayer, UnifiedPheromoneBus) to be accessed
/// through a common interface. See `aco_traits` for details.
pub use aco_traits::{PheromoneLayer, SharedPheromoneLayer};

/// Streaming MCTS — processes thought sequences as a stream.
pub use mcts_streaming::StreamingMCTS;

/// Predictive focus cache — ACO pheromone-guided prefetch for FocusCache (IC-3).
pub use predictive_focus_cache::PredictiveFocusCache;

/// QTable bridge — real [`RlBridge`] implementation wrapping touring-learning's QTable.
pub use qtable_bridge::QTableBridge;

/// RL bridge — wires RL rewards into cognitive search.
/// - `RlBridge`: general RL-to-cognitive wiring
/// - `UniformBridge`: equal-weight reward distribution
/// - `rl_informed_reward`: reward signal computation helper
pub use rl_bridge::{RlBridge, UniformBridge, rl_informed_reward};

/// Cognitive nexus — semantic graph traversal engine and session context.
/// - `CognitiveNexus`: main traversal engine
/// - `CognitiveCtx`: session-scoped context for the nexus
pub use nexus::{CognitiveCtx, CognitiveNexus};

/// Graph persistence — async SQLite storage for GoT snapshots via deadpool-sqlite.
/// - `GraphPersistence`: persistence trait
/// - `GraphSnapshot`: serialised snapshot envelope
pub use persistence::{GraphPersistence, GraphSnapshot};

/// Background cognitive tasks — context prediction, maintenance, and spawning helpers.
pub use predictor_task::{context_predictor_task, maintenance_task, spawn_cognitive_tasks};

/// Semantic graph — StableGraph with focus tracking, decay, and safe node removal.
/// - `SemanticGraph`: main graph container
/// - `MemoryNode` / `SemanticEdge` / `EdgeType` / `NodeType`: graph entity types
pub use semantic_graph::{
    EdgeType, MemoryNode, NodeType, SemanticEdge, SemanticGraph, SemanticGraphError,
};

/// Session predictor — predicts next tool from session state history.
/// - `SessionPredictor`: main predictor
/// - `ToolInvocation`: a single tool use event in a session
pub use session_predictor::{SessionPredictor, ToolInvocation};

/// Hybrid cognitive engine — combines MCTS, GoT, and TF-IDF strategies.
pub use hybrid_engine::HybridCognitiveEngine;

/// Cognitive metrics and snapshots for observability.
pub use metrics::{CognitiveMetrics, MetricsSnapshot};

/// Multi-objective pheromone layer — ACO with multiple competing objectives.
pub use multi_objective::MultiObjectivePheromonoLayer;

/// Co-edit record — a single co-edit observation for training the predictor.
pub use coedit_predictor::CoEditRecord;

/// Reasoning engine — typed reasoning over cognitive graph.
/// - `ReasoningEngine`: trait for pluggable reasoning strategies
/// - `HybridReasoningEngine`: combines multiple reasoning modes
/// - `MCTSReasoningEngine`: MCTS-based reasoning
/// - `ReasoningQuery` / `ReasoningResult`: query input and structured output
pub use reasoning_engine::{
    HybridReasoningEngine, MCTSReasoningEngine, ReasoningEngine, ReasoningQuery, ReasoningResult,
};

/// Session persistence — async SQLite storage for GoT snapshots.
pub use session_persistence::{SessionPersistence, SessionPersistenceError};

/// GoT snapshot — rkyv zero-copy serialisable snapshot of a GoT thought graph.
/// - `GoTSnapshot`: full graph snapshot
/// - `GotNodeSnapshot`: individual node snapshot
/// - `GoTSnapshotError`: typed error for snapshot serialization/deserialization (RBP-03)
pub use snapshot::{GoTSnapshot, GoTSnapshotError, GotNodeSnapshot};

/// SQLite-backed graph store for the semantic graph.
pub use sqlite_graph::SqliteGraphStore;

/// TF-IDF vectorizer for semantic search over the knowledge graph.
pub use tfidf::TfIdfVectorizer;

/// BM25-TF-IDF hybrid vectorizer and query result.
pub use bm25_tfidf::{BM25TfIdfVectorizer, QueryResult};

/// Pensieve — failed-path memory for cognitive search.
/// Records failed MCTS/GoT paths and penalises re-exploration via ANN similarity.
/// - `Pensieve`: main memory store with vector search
/// - `FailedPath`: metadata for a single recorded failure
pub use pensieve::{FailedPath, Pensieve};
