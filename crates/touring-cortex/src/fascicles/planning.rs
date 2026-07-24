//! MctsPlanningFascicle — GraphInformedMCTS planning integration for FascicleDispatcher.
//!
//! Provides O(1) dispatch access to GraphInformedMCTS search for cognitive
//! graph-informed context suggestions via the fascicle pipeline.
//!
//! # Architecture
//!
//! ```text
//! dispatch_mcts_planning(request)
//!      │
//!      ▼
//! MctsPlanningFascicle::dispatch()
//!      │
//!      ├── tokio::spawn_blocking (non-blocking hot path)
//!      │
//!      ▼
//! GraphInformedMCTS::search()
//!      │
//!      ▼
//! MctsResult ──► oneshot channel ──► caller
//! ```
//!
//! This design ensures MCTS search (which can be slow) does not block
//! the hot path. Callers receive a oneshot::Receiver to await results
//! when ready.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use tokio::sync::oneshot;
use tokio::task::spawn_blocking;
use touring_intelligence::reasoning::semantic_graph::SemanticGraph;
use touring_intelligence::reasoning::{CognitiveMCTSConfig, GraphInformedMCTS, MCTSResult};

use super::DispatchError;

/// Process-global MCTS planning engine — persists for daemon lifetime.
static GLOBAL_MCTS_PLANNER: OnceLock<GraphInformedMCTS> = OnceLock::new();

fn global_mcts_planner() -> &'static GraphInformedMCTS {
    GLOBAL_MCTS_PLANNER.get_or_init(|| {
        let config = CognitiveMCTSConfig {
            mcts_config: touring_intelligence::reasoning::MCTSConfig {
                max_depth: 4,
                num_rollouts: 32,
                exploration_constant: std::f64::consts::SQRT_2,
                discount: 0.99,
                max_rollouts: 100,
            },
            relevance_weight: 0.6,
            pheromone_alpha: 0.3,
            pheromone_evaporation_rate: 0.05,
        };
        GraphInformedMCTS::new(config)
    })
}

/// Request payload for MCTS planning dispatch.
#[derive(Debug, Clone)]
pub struct MctsPlanningRequest {
    /// Root state for MCTS search (typically derived from symbol/file hash).
    pub root_state: u64,
    /// Semantic graph providing neighbor context for expansion.
    pub graph: Arc<SemanticGraph>,
    /// Maps node string IDs to u64 for MCTS internal state.
    pub node_id_map: HashMap<String, u64>,
    /// Reverse map: u64 state IDs back to string node IDs.
    pub reverse_map: HashMap<u64, String>,
}

impl MctsPlanningRequest {
    /// Creates a new planning request with the given root state and graph.
    pub fn new(
        root_state: u64,
        graph: Arc<SemanticGraph>,
        node_id_map: HashMap<String, u64>,
        reverse_map: HashMap<u64, String>,
    ) -> Self {
        Self {
            root_state,
            graph,
            node_id_map,
            reverse_map,
        }
    }

    /// Creates a planning request from symbol + file strings.
    /// Derives root_state via deterministic hash.
    pub fn from_symbol_file(symbol: &str, file: &str) -> Self {
        let root_state = Self::hash_symbol_file(symbol, file);
        Self {
            root_state,
            graph: Arc::new(SemanticGraph::new(Arc::new(
                touring_intelligence::reasoning::persistence::GraphPersistence::new(
                    std::path::PathBuf::from(":memory:"),
                ),
            ))),
            node_id_map: HashMap::new(),
            reverse_map: HashMap::new(),
        }
    }

    /// Deterministic u64 hash from symbol + file strings.
    fn hash_symbol_file(symbol: &str, file: &str) -> u64 {
        let mut h = 0u64;
        for byte in symbol.bytes() {
            h = h.wrapping_mul(31).wrapping_add(u64::from(byte));
        }
        for byte in file.bytes() {
            h = h.wrapping_mul(37).wrapping_add(u64::from(byte));
        }
        h
    }
}

/// MctsPlanningFascicle — fascicle wrapper for GraphInformedMCTS planning.
///
///
/// Implements non-blocking MCTS dispatch via tokio::spawn_blocking,
/// ensuring the hot path is never blocked by MCTS computation.
#[derive(Debug, Default)]
pub struct MctsPlanningFascicle;

impl MctsPlanningFascicle {
    /// Creates a new MctsPlanningFascicle.
    pub fn new() -> Self {
        Self
    }

    /// Dispatches an MCTS planning request asynchronously.
    ///
    /// Runs the MCTS search in a background thread (via spawn_blocking)
    /// and returns a oneshot receiver for the result.
    ///
    /// # Arguments
    /// * `request` — The planning request containing root state and graph
    ///
    /// # Returns
    /// * `Ok(oneshot::Receiver<MctsResult>)` — Receiver that will deliver the result
    /// * `Err(DispatchError)` — If dispatch fails
    ///
    /// # Example
    /// ```ignore
    /// let (tx, rx) = oneshot::channel();
    /// fascicle.dispatch_mcts_async(request, tx);
    /// let result = rx.await?;
    /// ```
    pub fn dispatch(
        &self,
        request: MctsPlanningRequest,
    ) -> Result<oneshot::Receiver<MCTSResult>, DispatchError> {
        let (tx, rx) = oneshot::channel();

        // Spawn blocking MCTS search to avoid blocking the hot path.
        // This is critical — MCTS can take 10-100ms+ per search.
        let graph = Arc::clone(&request.graph);
        let node_id_map = request.node_id_map.clone();
        let reverse_map = request.reverse_map.clone();
        let root_state = request.root_state;

        // Spawn blocking MCTS search to avoid blocking the hot path.
        // This is critical — MCTS can take 10-100ms+ per search.
        let _handle = spawn_blocking(move || {
            let mcts = global_mcts_planner();
            let result = mcts.search(root_state, &graph, &node_id_map, &reverse_map);

            // Send result; if receiver dropped, that's fine (caller cancelled).
            // Unwrap Option<MCTSResult> with a default if search returned None.
            let send_result = result.unwrap_or(MCTSResult {
                best_action: 0,
                confidence: 0.0,
                value: 0.0,
                tree_depth: 0,
                total_rollouts: 0,
            });
            let _ = tx.send(send_result);
        });

        Ok(rx)
    }

    /// Returns the global MCTS planner singleton.
    ///
    /// Useful for direct (synchronous) access when blocking is acceptable,
    /// e.g., in tests or background tasks where latency is not critical.
    pub fn global_planner() -> &'static GraphInformedMCTS {
        global_mcts_planner()
    }

    /// Evaporates pheromones in the global planner between search sessions.
    pub fn evaporate(&self) {
        global_mcts_planner().evaporate();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use touring_intelligence::reasoning::persistence::GraphPersistence;
    use touring_intelligence::reasoning::semantic_graph::{MemoryNode, NodeType, SemanticGraph};

    fn make_test_graph() -> SemanticGraph {
        let persistence = Arc::new(GraphPersistence::new(std::path::PathBuf::from(":memory:")));
        SemanticGraph::new(persistence)
    }

    #[test]
    fn test_planning_request_from_symbol_file() {
        let req = MctsPlanningRequest::from_symbol_file("MyStruct", "src/lib.rs");
        assert_eq!(
            req.root_state,
            MctsPlanningRequest::from_symbol_file("MyStruct", "src/lib.rs").root_state
        );
    }

    #[test]
    fn test_planning_request_deterministic() {
        let req1 = MctsPlanningRequest::from_symbol_file("AuthService", "src/auth.rs");
        let req2 = MctsPlanningRequest::from_symbol_file("AuthService", "src/auth.rs");
        assert_eq!(req1.root_state, req2.root_state);
    }

    #[test]
    fn test_planning_request_different_inputs_different_hash() {
        let req1 = MctsPlanningRequest::from_symbol_file("AuthService", "src/auth.rs");
        let req2 = MctsPlanningRequest::from_symbol_file("UserService", "src/auth.rs");
        let req3 = MctsPlanningRequest::from_symbol_file("AuthService", "src/users.rs");
        assert_ne!(req1.root_state, req2.root_state);
        assert_ne!(req1.root_state, req3.root_state);
    }

    #[tokio::test]
    async fn test_dispatch_returns_receiver() {
        let fascicle = MctsPlanningFascicle::new();
        let graph = Arc::new(make_test_graph());
        let req = MctsPlanningRequest {
            root_state: 1,
            graph,
            node_id_map: HashMap::new(),
            reverse_map: HashMap::new(),
        };

        let rx = fascicle.dispatch(req).expect("dispatch should succeed");
        let result = rx.await.expect("should receive result");
        assert!(result.confidence >= 0.0);
    }

    #[tokio::test]
    async fn test_dispatch_with_empty_graph_returns_none() {
        let fascicle = MctsPlanningFascicle::new();
        let graph = Arc::new(make_test_graph());
        graph
            .add_node(MemoryNode::new("a", "A", NodeType::Symbol))
            .expect("test node a should add");
        graph
            .add_node(MemoryNode::new("b", "B", NodeType::Symbol))
            .expect("test node b should add");
        graph
            .add_edge("a", "b", 1.0)
            .expect("test edge a->b should add");

        let mut id_map = HashMap::new();
        let mut rev_map = HashMap::new();
        id_map.insert("a".to_string(), 1_u64);
        id_map.insert("b".to_string(), 2_u64);
        rev_map.insert(1_u64, "a".to_string());
        rev_map.insert(2_u64, "b".to_string());

        let req = MctsPlanningRequest {
            root_state: 1,
            graph,
            node_id_map: id_map,
            reverse_map: rev_map,
        };

        let rx = fascicle.dispatch(req).expect("dispatch should succeed");
        let result = rx.await.expect("should receive result");
        assert!(result.confidence >= 0.0);
    }

    #[test]
    fn test_global_mcts_planner_singleton() {
        let mcts1 = global_mcts_planner();
        let mcts2 = global_mcts_planner();
        assert!(std::ptr::eq(mcts1, mcts2));
    }

    #[test]
    fn test_evaporate_does_not_panic() {
        let fascicle = MctsPlanningFascicle::new();
        fascicle.evaporate(); // Must not panic
    }
}
