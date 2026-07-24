//! AcoReadModel — CQRS read-side projection of MutableGeneratorGraph state.
//!
//! INS-5 (ROI=1.40): Separates reads from writes. The read model is rebuilt
//! from the graph on demand and caches expensive computations (topo order,
//! metrics, critical path) so callers never have to recompute them.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use super::graph::{GraphMetrics, MutableGeneratorGraph};
use super::models::ExecutionStatus;

/// Immutable read-side projection of a `MutableGeneratorGraph`.
///
/// Built once via [`AcoReadModel::build`], then queried cheaply.
/// When the graph mutates, discard this snapshot and rebuild.
#[derive(Debug, Clone)]
pub struct AcoReadModel {
    /// Node IDs in deterministic topological order.
    pub node_ids: Vec<String>,
    /// Snapshot of execution status for every node.
    pub status_map: HashMap<String, ExecutionStatus>,
    /// Nodes eligible for execution: status == Pending AND all deps have Success.
    pub ready_nodes: Vec<String>,
    /// Cached graph analytics (density, fan-in/out, etc.).
    pub metrics: GraphMetrics,
    /// Critical (longest dependency) path through the DAG.
    pub critical_path: Vec<String>,
    /// Wall-clock instant when this snapshot was created.
    pub snapshot_at: Instant,
    /// IL-5: generation of the graph at snapshot time.
    /// If the graph's generation diverges from this value, the snapshot is stale.
    pub graph_generation: u64,
}

impl AcoReadModel {
    /// Build a read-model snapshot from the current graph state.
    ///
    /// This performs three potentially expensive operations once:
    /// topological sort, graph metrics, and critical-path computation.
    /// All subsequent reads are O(1) lookups on the cached data.
    pub fn build(graph: &mut MutableGeneratorGraph) -> Self {
        let node_ids = graph.topological_sort_deterministic().unwrap_or_default();

        // Snapshot every node's execution status.
        let mut status_map = HashMap::new();
        for id in &node_ids {
            if let Some(status) = graph.execution_status(id) {
                status_map.insert(id.clone(), *status);
            }
        }

        // Also include nodes that might not be in topo order (e.g. cycle members).
        for (id, _node) in graph.iter() {
            status_map.entry(id.to_string()).or_insert_with(|| {
                graph
                    .execution_status(id)
                    .copied()
                    .unwrap_or(ExecutionStatus::Pending)
            });
        }

        // Determine ready nodes: Pending with all dependencies at Success.
        let ready_nodes = node_ids
            .iter()
            .filter(|id| {
                let is_pending = status_map
                    .get(id.as_str())
                    .copied()
                    .unwrap_or(ExecutionStatus::Pending)
                    == ExecutionStatus::Pending;

                if !is_pending {
                    return false;
                }

                // Check that all depends_on are Success.
                graph
                    .get_node(id)
                    .map(|node| {
                        node.depends_on.iter().all(|dep| {
                            status_map
                                .get(dep.as_str())
                                .copied()
                                .unwrap_or(ExecutionStatus::Pending)
                                == ExecutionStatus::Success
                        })
                    })
                    .unwrap_or(false)
            })
            .cloned()
            .collect();

        let metrics = graph.compute_graph_metrics();
        let critical_path = graph.get_critical_path().unwrap_or_default();

        Self {
            node_ids,
            status_map,
            ready_nodes,
            metrics,
            critical_path,
            snapshot_at: Instant::now(),
            graph_generation: graph.generation(),
        }
    }

    /// Returns `true` if this snapshot is older than `max_age`.
    pub fn is_stale(&self, max_age: Duration) -> bool {
        self.snapshot_at.elapsed() > max_age
    }

    /// IL-5: Returns `true` if the graph has mutated since this snapshot was built,
    /// OR if the snapshot has exceeded its TTL.
    ///
    /// Prefer this over `is_stale` when you have access to the live graph,
    /// as it detects mutations that occur faster than the TTL window.
    pub fn is_stale_with_graph(&self, graph: &MutableGeneratorGraph, max_age: Duration) -> bool {
        graph.generation() != self.graph_generation || self.snapshot_at.elapsed() > max_age
    }

    /// INS-L4: Returns `true` purely based on generation counter divergence.
    ///
    /// Unlike `is_stale_with_graph` this ignores wall-clock TTL — useful when
    /// you want generation-based invalidation only (no time component).
    pub fn is_stale_by_generation(&self, current_gen: u64) -> bool {
        self.graph_generation < current_gen
    }

    /// INS-L4: Rebuild this snapshot from `graph` if it is stale by generation.
    ///
    /// Returns `true` if a refresh occurred, `false` if the snapshot was current.
    pub fn refresh_if_stale(&mut self, graph: &mut MutableGeneratorGraph) -> bool {
        if self.is_stale_by_generation(graph.generation()) {
            *self = Self::build(graph);
            true
        } else {
            false
        }
    }

    /// Total number of nodes in the graph at snapshot time.
    pub fn node_count(&self) -> usize {
        self.node_ids.len()
    }

    /// Count of nodes with `ExecutionStatus::Pending`.
    pub fn pending_count(&self) -> usize {
        self.status_map
            .values()
            .filter(|s| **s == ExecutionStatus::Pending)
            .count()
    }

    /// Count of nodes with `ExecutionStatus::Success`.
    pub fn success_count(&self) -> usize {
        self.status_map
            .values()
            .filter(|s| **s == ExecutionStatus::Success)
            .count()
    }

    /// Ratio of successfully completed nodes to total nodes.
    ///
    /// Returns `0.0` if the graph is empty (avoids division by zero).
    pub fn completion_ratio(&self) -> f64 {
        let total = self.node_count();
        if total == 0 {
            return 0.0;
        }
        self.success_count() as f64 / total as f64
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;
    use crate::rl::aco::models::{
        GeneratorContract, GeneratorNode, GeneratorOutputs, GeneratorType,
    };

    /// Helper: build a minimal `GeneratorNode` with the given id and dependencies.
    fn make_node(id: &str, depends_on: Vec<&str>) -> GeneratorNode {
        GeneratorNode {
            id: id.to_string(),
            description: format!("node {id}"),
            generator_type: GeneratorType::Template,
            inputs_data: vec![],
            inputs_templates: vec![],
            inputs_constraints: vec![],
            outputs: GeneratorOutputs {
                execution_script: "exec.sh".into(),
                validation_script: "val.sh".into(),
                rollback_script: "rb.sh".into(),
            },
            contract: GeneratorContract {
                precondition: "true".into(),
                postcondition: "true".into(),
                invariant: "true".into(),
            },
            acceptance_criteria: "done".into(),
            depends_on: depends_on.into_iter().map(String::from).collect(),
        }
    }

    // --- 1. Empty graph ---

    #[test]
    fn test_read_model_empty_graph() {
        let mut g = MutableGeneratorGraph::new();
        let rm = AcoReadModel::build(&mut g);

        assert_eq!(rm.node_count(), 0);
        assert!(rm.ready_nodes.is_empty());
        assert!(rm.node_ids.is_empty());
        assert!(rm.status_map.is_empty());
        assert!(rm.critical_path.is_empty());
    }

    // --- 2. Single pending node ---

    #[test]
    fn test_read_model_single_node_pending() {
        let mut g = MutableGeneratorGraph::new();
        g.add_node(make_node("a", vec![])).expect("add_node");

        let rm = AcoReadModel::build(&mut g);

        assert_eq!(rm.node_count(), 1);
        assert_eq!(rm.ready_nodes, vec!["a".to_string()]);
        assert_eq!(
            rm.status_map.get("a").copied(),
            Some(ExecutionStatus::Pending)
        );
    }

    // --- 3. Chain: only root is ready ---

    #[test]
    fn test_read_model_chain_ready_nodes() {
        let mut g = MutableGeneratorGraph::new();
        g.add_node(make_node("a", vec![])).expect("add a");
        g.add_node(make_node("b", vec!["a"])).expect("add b");
        g.add_node(make_node("c", vec!["b"])).expect("add c");

        let rm = AcoReadModel::build(&mut g);

        // Only "a" is ready — b depends on a (Pending), c depends on b (Pending).
        assert_eq!(rm.ready_nodes, vec!["a".to_string()]);
        assert_eq!(rm.node_count(), 3);
    }

    // --- 4. Completion ratio = 0.0 when no successes ---

    #[test]
    fn test_read_model_completion_ratio_zero() {
        let mut g = MutableGeneratorGraph::new();
        g.add_node(make_node("a", vec![])).expect("add a");
        g.add_node(make_node("b", vec![])).expect("add b");

        let rm = AcoReadModel::build(&mut g);

        assert!((rm.completion_ratio() - 0.0).abs() < f64::EPSILON);
    }

    // --- 5. Completion ratio = 1.0 when all succeed ---

    #[test]
    fn test_read_model_completion_ratio_full() {
        let mut g = MutableGeneratorGraph::new();
        g.add_node(make_node("a", vec![])).expect("add a");
        g.add_node(make_node("b", vec!["a"])).expect("add b");

        // Transition both to Success.
        g.transition_status("a", ExecutionStatus::Running)
            .expect("a→Running");
        g.transition_status("a", ExecutionStatus::Success)
            .expect("a→Success");
        g.transition_status("b", ExecutionStatus::Running)
            .expect("b→Running");
        g.transition_status("b", ExecutionStatus::Success)
            .expect("b→Success");

        let rm = AcoReadModel::build(&mut g);

        assert!((rm.completion_ratio() - 1.0).abs() < f64::EPSILON);
    }

    // --- 6. Metrics are present and match node count ---

    #[test]
    fn test_read_model_metrics_present() {
        let mut g = MutableGeneratorGraph::new();
        g.add_node(make_node("a", vec![])).expect("add a");
        g.add_node(make_node("b", vec!["a"])).expect("add b");
        g.add_node(make_node("c", vec!["a"])).expect("add c");

        let rm = AcoReadModel::build(&mut g);

        assert_eq!(rm.metrics.node_count, 3);
        assert_eq!(rm.metrics.edge_count, 2);
    }

    // --- 7. Topological order respected ---

    #[test]
    fn test_read_model_topo_order_respected() {
        let mut g = MutableGeneratorGraph::new();
        g.add_node(make_node("a", vec![])).expect("add a");
        g.add_node(make_node("b", vec!["a"])).expect("add b");
        g.add_node(make_node("c", vec!["b"])).expect("add c");

        let rm = AcoReadModel::build(&mut g);

        let pos_a = rm.node_ids.iter().position(|x| x == "a").expect("a exists");
        let pos_b = rm.node_ids.iter().position(|x| x == "b").expect("b exists");
        let pos_c = rm.node_ids.iter().position(|x| x == "c").expect("c exists");

        assert!(pos_a < pos_b, "a must come before b");
        assert!(pos_b < pos_c, "b must come before c");
    }

    // --- 8. Critical path on a linear graph ---

    #[test]
    fn test_read_model_critical_path() {
        let mut g = MutableGeneratorGraph::new();
        g.add_node(make_node("a", vec![])).expect("add a");
        g.add_node(make_node("b", vec!["a"])).expect("add b");
        g.add_node(make_node("c", vec!["b"])).expect("add c");

        let rm = AcoReadModel::build(&mut g);

        // Linear chain: critical path should include all nodes.
        assert!(
            !rm.critical_path.is_empty(),
            "critical path must not be empty"
        );
        assert_eq!(rm.critical_path.len(), 3);
    }

    // --- 9. is_stale with fresh snapshot ---

    #[test]
    fn test_read_model_is_stale_false() {
        let mut g = MutableGeneratorGraph::new();
        g.add_node(make_node("a", vec![])).expect("add a");

        let rm = AcoReadModel::build(&mut g);

        // Just created — should not be stale with a 1-second window.
        assert!(!rm.is_stale(Duration::from_secs(1)));
    }

    // --- 10. Pending + success counts ---

    #[test]
    fn test_read_model_pending_and_success_counts() {
        let mut g = MutableGeneratorGraph::new();
        g.add_node(make_node("a", vec![])).expect("add a");
        g.add_node(make_node("b", vec!["a"])).expect("add b");
        g.add_node(make_node("c", vec!["a"])).expect("add c");

        // Only "a" transitions to Success.
        g.transition_status("a", ExecutionStatus::Running)
            .expect("a→Running");
        g.transition_status("a", ExecutionStatus::Success)
            .expect("a→Success");

        let rm = AcoReadModel::build(&mut g);

        assert_eq!(rm.pending_count(), 2); // b, c
        assert_eq!(rm.success_count(), 1); // a
    }

    // --- 11. Ready nodes update when deps complete ---

    #[test]
    fn test_read_model_ready_after_dep_success() {
        let mut g = MutableGeneratorGraph::new();
        g.add_node(make_node("a", vec![])).expect("add a");
        g.add_node(make_node("b", vec!["a"])).expect("add b");

        // Initially only "a" is ready.
        let rm1 = AcoReadModel::build(&mut g);
        assert_eq!(rm1.ready_nodes, vec!["a".to_string()]);

        // After "a" succeeds, "b" becomes ready.
        g.transition_status("a", ExecutionStatus::Running)
            .expect("a→Running");
        g.transition_status("a", ExecutionStatus::Success)
            .expect("a→Success");

        let rm2 = AcoReadModel::build(&mut g);
        assert_eq!(rm2.ready_nodes, vec!["b".to_string()]);
    }

    // --- 12. Completion ratio on empty graph ---

    #[test]
    fn test_read_model_completion_ratio_empty() {
        let mut g = MutableGeneratorGraph::new();
        let rm = AcoReadModel::build(&mut g);
        assert!((rm.completion_ratio() - 0.0).abs() < f64::EPSILON);
    }

    // --- IL-5: generation counter tests ---

    #[test]
    fn test_generation_starts_at_zero() {
        let g = MutableGeneratorGraph::new();
        assert_eq!(g.generation(), 0);
    }

    #[test]
    fn test_generation_increments_on_add_node() {
        let mut g = MutableGeneratorGraph::new();
        assert_eq!(g.generation(), 0);
        g.add_node(make_node("a", vec![])).expect("add a");
        assert_eq!(g.generation(), 1);
        g.add_node(make_node("b", vec![])).expect("add b");
        assert_eq!(g.generation(), 2);
    }

    #[test]
    fn test_generation_increments_on_remove_node() {
        let mut g = MutableGeneratorGraph::new();
        g.add_node(make_node("a", vec![])).expect("add a");
        let gen_before = g.generation();
        g.remove_node("a").expect("remove a");
        assert_eq!(g.generation(), gen_before + 1);
    }

    #[test]
    fn test_generation_increments_on_transition_status() {
        use crate::rl::aco::models::ExecutionStatus;
        let mut g = MutableGeneratorGraph::new();
        g.add_node(make_node("a", vec![])).expect("add a");
        let gen_before = g.generation();
        g.transition_status("a", ExecutionStatus::Running)
            .expect("a→Running");
        assert_eq!(g.generation(), gen_before + 1);
    }

    #[test]
    fn test_read_model_stale_with_graph_on_mutation() {
        let mut g = MutableGeneratorGraph::new();
        g.add_node(make_node("a", vec![])).expect("add a");
        let rm = AcoReadModel::build(&mut g);

        // Fresh snapshot — not stale.
        assert!(!rm.is_stale_with_graph(&g, std::time::Duration::from_secs(60)));

        // After mutation, snapshot becomes stale.
        g.add_node(make_node("b", vec![])).expect("add b");
        assert!(rm.is_stale_with_graph(&g, std::time::Duration::from_secs(60)));
    }

    #[test]
    fn test_read_model_graph_generation_captured() {
        let mut g = MutableGeneratorGraph::new();
        g.add_node(make_node("a", vec![])).expect("add a");
        let rm = AcoReadModel::build(&mut g);
        assert_eq!(rm.graph_generation, g.generation());
    }
}
