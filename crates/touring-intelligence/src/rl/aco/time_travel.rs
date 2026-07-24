//! TimeTravelDebugger — snapshot-based graph state recorder for debugging.
//!
//! INS-9 (ROI=1.17): Records snapshots of graph state at each transition,
//! enabling replay and diff between any two points in time.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use super::graph::{GraphMetrics, MutableGeneratorGraph};
use super::models::ExecutionStatus;

// ---------------------------------------------------------------------------
// Snapshot
// ---------------------------------------------------------------------------

/// A point-in-time snapshot of the graph state.
#[derive(Debug, Clone)]
pub struct GraphSnapshot {
    /// Monotonically increasing sequence number.
    pub sequence: usize,
    /// Human-readable label describing this snapshot.
    pub label: String,
    /// Cloned execution status of every node at snapshot time.
    pub status_map: HashMap<String, ExecutionStatus>,
    /// Graph metrics at snapshot time.
    pub metrics: GraphMetrics,
    /// Node IDs in topological order at snapshot time.
    pub node_ids: Vec<String>,
}

// ---------------------------------------------------------------------------
// Diff
// ---------------------------------------------------------------------------

/// Diff between two snapshots.
#[derive(Debug, Clone)]
pub struct SnapshotDiff {
    /// Sequence of the earlier snapshot.
    pub from_seq: usize,
    /// Sequence of the later snapshot.
    pub to_seq: usize,
    /// Nodes whose status changed: (id, before, after).
    pub changed_nodes: Vec<(String, ExecutionStatus, ExecutionStatus)>,
    /// Delta in node count (to - from).
    pub node_count_delta: i64,
    /// Delta in edge count (to - from).
    pub edge_count_delta: i64,
}

// ---------------------------------------------------------------------------
// Debugger
// ---------------------------------------------------------------------------

/// Snapshot-based graph state recorder for time-travel debugging.
///
/// Records up to `max_snapshots` snapshots (FIFO eviction when full).
/// INS-L9: Consecutive identical snapshots (same status_map hash) are skipped.
pub struct TimeTravelDebugger {
    snapshots: Vec<GraphSnapshot>,
    max_snapshots: usize,
    /// INS-L9: Hash of the last recorded status_map; used for dedup.
    last_snapshot_hash: u64,
}

impl TimeTravelDebugger {
    /// Create a new debugger with the given snapshot capacity.
    pub fn new(max_snapshots: usize) -> Self {
        Self {
            snapshots: Vec::new(),
            max_snapshots,
            last_snapshot_hash: 0,
        }
    }

    /// Compute a deterministic hash for a (label, status_map) pair.
    ///
    /// Including the label ensures that two consecutive snapshots with the same
    /// graph state but different labels are NOT considered duplicates — only a
    /// snapshot with identical label AND identical graph state is skipped.
    /// Sorts entries by key before hashing so HashMap ordering is irrelevant.
    fn hash_status_map(status_map: &HashMap<String, ExecutionStatus>) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        let mut hasher = DefaultHasher::new();
        let mut pairs: Vec<(&String, &ExecutionStatus)> = status_map.iter().collect();
        pairs.sort_by_key(|(k, _)| k.as_str());
        for (k, v) in pairs {
            k.hash(&mut hasher);
            // Hash the Debug representation of the enum (stable for known variants).
            format!("{v:?}").hash(&mut hasher);
        }
        hasher.finish()
    }

    /// Compute a dedup key combining label and status_map hash.
    ///
    /// Two consecutive records are considered duplicates only when BOTH
    /// the label and the graph state are identical.
    fn dedup_key(label: &str, status_hash: u64) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        let mut hasher = DefaultHasher::new();
        label.hash(&mut hasher);
        status_hash.hash(&mut hasher);
        hasher.finish()
    }

    /// Record a snapshot of the current graph state.
    ///
    /// The snapshot captures execution status of every node, graph metrics,
    /// and topological ordering.  When the snapshot buffer is full the oldest
    /// snapshot is evicted (FIFO).
    /// INS-L9: Returns early without recording if the status_map is identical
    /// to the previous snapshot (hash-based dedup).
    pub fn record(&mut self, label: &str, graph: &mut MutableGeneratorGraph) {
        let sequence = self.snapshots.last().map_or(0, |s| s.sequence + 1);

        // Build status_map by iterating all node IDs and querying their status.
        let ids = graph.node_ids();
        let mut status_map = HashMap::with_capacity(ids.len());
        for id in &ids {
            let status = graph
                .execution_status(id)
                .copied()
                .unwrap_or(ExecutionStatus::Pending);
            status_map.insert(id.clone(), status);
        }

        // INS-L9: skip only if both label AND graph state are identical to the
        // last snapshot (prevents duplicate noise without breaking label-sequencing).
        let status_hash = Self::hash_status_map(&status_map);
        let current_key = Self::dedup_key(label, status_hash);
        if current_key == self.last_snapshot_hash && !self.snapshots.is_empty() {
            return;
        }
        self.last_snapshot_hash = current_key;

        let metrics = graph.compute_graph_metrics();
        let node_ids = graph.topological_sort_deterministic().unwrap_or_default();

        let snapshot = GraphSnapshot {
            sequence,
            label: label.to_string(),
            status_map,
            metrics,
            node_ids,
        };

        if self.snapshots.len() >= self.max_snapshots {
            self.snapshots.remove(0);
        }
        self.snapshots.push(snapshot);
    }

    /// Number of snapshots currently stored.
    pub fn snapshot_count(&self) -> usize {
        self.snapshots.len()
    }

    /// Retrieve a snapshot by its sequence number (not by index).
    pub fn get(&self, sequence: usize) -> Option<&GraphSnapshot> {
        self.snapshots.iter().find(|s| s.sequence == sequence)
    }

    /// The most recent snapshot, if any.
    pub fn latest(&self) -> Option<&GraphSnapshot> {
        self.snapshots.last()
    }

    /// Compute the diff between two snapshots identified by sequence number.
    ///
    /// Returns `None` if either sequence is not found.
    pub fn diff(&self, seq_a: usize, seq_b: usize) -> Option<SnapshotDiff> {
        let snap_a = self.get(seq_a)?;
        let snap_b = self.get(seq_b)?;

        // Collect all node IDs present in either snapshot.
        let mut all_ids: Vec<&String> = snap_a
            .status_map
            .keys()
            .chain(snap_b.status_map.keys())
            .collect();
        all_ids.sort();
        all_ids.dedup();

        let mut changed_nodes = Vec::new();
        for id in all_ids {
            let status_a = snap_a
                .status_map
                .get(id)
                .copied()
                .unwrap_or(ExecutionStatus::Pending);
            let status_b = snap_b
                .status_map
                .get(id)
                .copied()
                .unwrap_or(ExecutionStatus::Pending);
            if status_a != status_b {
                changed_nodes.push((id.clone(), status_a, status_b));
            }
        }

        let node_count_delta = snap_b.metrics.node_count as i64 - snap_a.metrics.node_count as i64;
        let edge_count_delta = snap_b.metrics.edge_count as i64 - snap_a.metrics.edge_count as i64;

        Some(SnapshotDiff {
            from_seq: seq_a,
            to_seq: seq_b,
            changed_nodes,
            node_count_delta,
            edge_count_delta,
        })
    }

    /// Labels of all stored snapshots in insertion order.
    pub fn replay_labels(&self) -> Vec<&str> {
        self.snapshots.iter().map(|s| s.label.as_str()).collect()
    }

    /// INS-L3: Record a lightweight label-only snapshot (no graph required).
    ///
    /// Used by `SagaOrchestrator` to mark failure points without access to a
    /// `MutableGeneratorGraph`.  The snapshot has an empty `status_map`, zeroed
    /// metrics, and empty `node_ids` — it serves as a timeline marker only.
    pub fn take_snapshot(&mut self, label: &str) {
        use super::graph::GraphMetrics;
        let sequence = self.snapshots.last().map_or(0, |s| s.sequence + 1);
        let status_map: HashMap<String, ExecutionStatus> = HashMap::new();
        // Label-only marker snapshots are always recorded.
        // Do NOT update last_snapshot_hash — this is a timeline marker, not a
        // real graph snapshot, so it must not interfere with dedup of record().
        let snapshot = GraphSnapshot {
            sequence,
            label: label.to_string(),
            status_map,
            metrics: GraphMetrics {
                node_count: 0,
                edge_count: 0,
                density: 0.0,
                max_fan_in: 0,
                max_fan_out: 0,
                critical_path_length: 0,
            },
            node_ids: Vec::new(),
        };
        if self.snapshots.len() >= self.max_snapshots {
            self.snapshots.remove(0);
        }
        self.snapshots.push(snapshot);
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rl::aco::models::{
        GeneratorContract, GeneratorNode, GeneratorOutputs, GeneratorType,
    };

    fn make_node(id: &str, deps: Vec<&str>) -> GeneratorNode {
        GeneratorNode {
            id: id.to_string(),
            description: format!("node {id}"),
            generator_type: GeneratorType::Template,
            inputs_data: vec![],
            inputs_templates: vec![],
            inputs_constraints: vec![],
            outputs: GeneratorOutputs {
                execution_script: String::new(),
                validation_script: String::new(),
                rollback_script: String::new(),
            },
            contract: GeneratorContract {
                precondition: String::new(),
                postcondition: String::new(),
                invariant: String::new(),
            },
            acceptance_criteria: String::new(),
            depends_on: deps.into_iter().map(|s| s.to_string()).collect(),
        }
    }

    // -----------------------------------------------------------------------
    // 1. record creates a snapshot
    // -----------------------------------------------------------------------
    #[test]
    fn test_time_travel_record_creates_snapshot() {
        let mut graph = MutableGeneratorGraph::default();
        graph
            .add_node(make_node("a", vec![]))
            .expect("add_node should succeed");

        let mut debugger = TimeTravelDebugger::new(10);
        debugger.record("initial", &mut graph);

        assert_eq!(debugger.snapshot_count(), 1);
    }

    // -----------------------------------------------------------------------
    // 2. snapshot has correct label
    // -----------------------------------------------------------------------
    #[test]
    fn test_time_travel_snapshot_has_correct_label() {
        let mut graph = MutableGeneratorGraph::default();
        graph
            .add_node(make_node("a", vec![]))
            .expect("add_node should succeed");

        let mut debugger = TimeTravelDebugger::new(10);
        debugger.record("my-label", &mut graph);

        let snap = debugger.latest().expect("should have a snapshot");
        assert_eq!(snap.label, "my-label");
    }

    // -----------------------------------------------------------------------
    // 3. sequence is monotonically increasing
    // -----------------------------------------------------------------------
    #[test]
    fn test_time_travel_sequence_monotonic() {
        let mut graph = MutableGeneratorGraph::default();
        graph
            .add_node(make_node("a", vec![]))
            .expect("add_node should succeed");

        let mut debugger = TimeTravelDebugger::new(10);
        debugger.record("s0", &mut graph);
        debugger.record("s1", &mut graph);
        debugger.record("s2", &mut graph);

        let seqs: Vec<usize> = (0..3)
            .filter_map(|i| debugger.get(i).map(|s| s.sequence))
            .collect();
        assert_eq!(seqs, vec![0, 1, 2]);
    }

    // -----------------------------------------------------------------------
    // 4. latest returns the last snapshot
    // -----------------------------------------------------------------------
    #[test]
    fn test_time_travel_latest_returns_last() {
        let mut graph = MutableGeneratorGraph::default();
        graph
            .add_node(make_node("a", vec![]))
            .expect("add_node should succeed");

        let mut debugger = TimeTravelDebugger::new(10);
        debugger.record("s0", &mut graph);
        debugger.record("s1", &mut graph);
        debugger.record("s2", &mut graph);

        let latest = debugger.latest().expect("should have a snapshot");
        assert_eq!(latest.sequence, 2);
    }

    // -----------------------------------------------------------------------
    // 5. get by sequence
    // -----------------------------------------------------------------------
    #[test]
    fn test_time_travel_get_by_sequence() {
        let mut graph = MutableGeneratorGraph::default();
        graph
            .add_node(make_node("a", vec![]))
            .expect("add_node should succeed");

        let mut debugger = TimeTravelDebugger::new(10);
        debugger.record("s0", &mut graph);
        debugger.record("s1", &mut graph);
        debugger.record("s2", &mut graph);

        let snap = debugger.get(1).expect("should find seq 1");
        assert_eq!(snap.sequence, 1);
        assert_eq!(snap.label, "s1");
    }

    // -----------------------------------------------------------------------
    // 6. get unknown returns None
    // -----------------------------------------------------------------------
    #[test]
    fn test_time_travel_get_unknown_returns_none() {
        let debugger = TimeTravelDebugger::new(10);
        assert!(debugger.get(999).is_none());
    }

    // -----------------------------------------------------------------------
    // 7. diff detects changed nodes after transition_status
    // -----------------------------------------------------------------------
    #[test]
    fn test_time_travel_diff_changed_nodes() {
        let mut graph = MutableGeneratorGraph::default();
        graph
            .add_node(make_node("a", vec![]))
            .expect("add_node should succeed");

        let mut debugger = TimeTravelDebugger::new(10);
        debugger.record("before", &mut graph);

        graph
            .transition_status("a", ExecutionStatus::Running)
            .expect("transition should succeed");
        debugger.record("after", &mut graph);

        let diff = debugger.diff(0, 1).expect("diff should succeed");
        assert_eq!(diff.from_seq, 0);
        assert_eq!(diff.to_seq, 1);
        assert_eq!(diff.changed_nodes.len(), 1);

        let (ref id, before, after) = diff.changed_nodes[0];
        assert_eq!(id, "a");
        assert_eq!(before, ExecutionStatus::Pending);
        assert_eq!(after, ExecutionStatus::Running);
    }

    // -----------------------------------------------------------------------
    // 8. diff node_count_delta after adding a node
    // -----------------------------------------------------------------------
    #[test]
    fn test_time_travel_diff_node_count_delta() {
        let mut graph = MutableGeneratorGraph::default();
        graph
            .add_node(make_node("a", vec![]))
            .expect("add_node should succeed");

        let mut debugger = TimeTravelDebugger::new(10);
        debugger.record("one-node", &mut graph);

        graph
            .add_node(make_node("b", vec![]))
            .expect("add_node should succeed");
        debugger.record("two-nodes", &mut graph);

        let diff = debugger.diff(0, 1).expect("diff should succeed");
        assert_eq!(diff.node_count_delta, 1);
    }

    // -----------------------------------------------------------------------
    // 9. max_snapshots evicts oldest (FIFO)
    // -----------------------------------------------------------------------
    #[test]
    fn test_time_travel_max_snapshots_evicts_oldest() {
        let mut graph = MutableGeneratorGraph::default();
        graph
            .add_node(make_node("a", vec![]))
            .expect("add_node should succeed");

        let mut debugger = TimeTravelDebugger::new(2);
        debugger.record("s0", &mut graph);
        debugger.record("s1", &mut graph);
        debugger.record("s2", &mut graph);

        assert_eq!(debugger.snapshot_count(), 2);
        assert!(debugger.get(0).is_none(), "seq 0 should have been evicted");
        assert!(debugger.get(1).is_some());
        assert!(debugger.get(2).is_some());
    }

    // -----------------------------------------------------------------------
    // 10. replay_labels returns labels in insertion order
    // -----------------------------------------------------------------------
    #[test]
    fn test_time_travel_replay_labels_order() {
        let mut graph = MutableGeneratorGraph::default();
        graph
            .add_node(make_node("a", vec![]))
            .expect("add_node should succeed");

        let mut debugger = TimeTravelDebugger::new(10);
        debugger.record("alpha", &mut graph);
        debugger.record("beta", &mut graph);
        debugger.record("gamma", &mut graph);

        assert_eq!(debugger.replay_labels(), vec!["alpha", "beta", "gamma"]);
    }

    // -----------------------------------------------------------------------
    // 11. snapshot captures correct node_ids (topo order)
    // -----------------------------------------------------------------------
    #[test]
    fn test_time_travel_snapshot_node_ids_topo_order() {
        let mut graph = MutableGeneratorGraph::default();
        graph
            .add_node(make_node("a", vec![]))
            .expect("add_node should succeed");
        graph
            .add_node(make_node("b", vec!["a"]))
            .expect("add_node should succeed");

        let mut debugger = TimeTravelDebugger::new(10);
        debugger.record("topo", &mut graph);

        let snap = debugger.latest().expect("should have a snapshot");
        assert_eq!(snap.node_ids, vec!["a", "b"]);
    }

    // -----------------------------------------------------------------------
    // 12. diff with unknown sequence returns None
    // -----------------------------------------------------------------------
    #[test]
    fn test_time_travel_diff_unknown_seq_returns_none() {
        let mut graph = MutableGeneratorGraph::default();
        graph
            .add_node(make_node("a", vec![]))
            .expect("add_node should succeed");

        let mut debugger = TimeTravelDebugger::new(10);
        debugger.record("s0", &mut graph);

        assert!(debugger.diff(0, 999).is_none());
        assert!(debugger.diff(999, 0).is_none());
    }
}
