//! Snapshot diff — visual diff between two graph snapshots.

use super::{GraphData, NodeData, EdgeData};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Represents a change between two snapshots.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotDiff {
    /// Nodes added in the second snapshot.
    pub added_nodes: Vec<NodeData>,
    /// Nodes removed from the first snapshot.
    pub removed_nodes: Vec<String>,
    /// Nodes modified (quality score changed > 0.1).
    pub modified_nodes: Vec<ModifiedNode>,
    /// Edges added.
    pub added_edges: Vec<EdgeData>,
    /// Edges removed.
    pub removed_edges: Vec<EdgeData>,
    /// Summary statistics.
    pub stats: DiffStats,
}

/// A node that changed between snapshots.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModifiedNode {
    pub id: String,
    pub old_quality: Option<f32>,
    pub new_quality: Option<f32>,
    pub old_fan_in: Option<usize>,
    pub new_fan_in: Option<usize>,
    pub old_fan_out: Option<usize>,
    pub new_fan_out: Option<usize>,
}

/// Summary statistics for a diff.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffStats {
    pub nodes_added: usize,
    pub nodes_removed: usize,
    pub nodes_modified: usize,
    pub edges_added: usize,
    pub edges_removed: usize,
}

impl SnapshotDiff {
    /// Compute the diff between two snapshots.
    pub fn compute(a: &GraphData, b: &GraphData) -> Self {
        let ids_a: HashSet<_> = a.nodes.iter().map(|n| n.id.clone()).collect();
        let ids_b: HashSet<_> = b.nodes.iter().map(|n| n.id.clone()).collect();

        // Nodes added (in B but not in A)
        let added_ids: HashSet<_> = ids_b.difference(&ids_a).cloned().collect();
        let added_nodes: Vec<NodeData> = b.nodes
            .iter()
            .filter(|n| added_ids.contains(&n.id))
            .cloned()
            .collect();

        // Nodes removed (in A but not in B)
        let removed_ids: HashSet<_> = ids_a.difference(&ids_b).cloned().collect();
        let removed_nodes: Vec<String> = ids_a
            .difference(&ids_b)
            .cloned()
            .collect();

        // Nodes modified (exist in both but changed)
        let modified_nodes: Vec<ModifiedNode> = a.nodes
            .iter()
            .filter_map(|node_a| {
                let id = &node_a.id;
                if added_ids.contains(id) || removed_ids.contains(id) {
                    return None;
                }
                let node_b = b.nodes.iter().find(|n| n.id == *id)?;
                if Self::nodes_differ(node_a, node_b) {
                    Some(ModifiedNode {
                        id: id.clone(),
                        old_quality: node_a.quality_score,
                        new_quality: node_b.quality_score,
                        old_fan_in: node_a.fan_in,
                        new_fan_in: node_b.fan_in,
                        old_fan_out: node_a.fan_out,
                        new_fan_out: node_b.fan_out,
                    })
                } else {
                    None
                }
            })
            .collect();

        // Edges added/removed
        let edges_a: HashSet<_> = a.edges.iter().map(|e| (e.from.clone(), e.to.clone(), e.kind.clone())).collect();
        let edges_b: HashSet<_> = b.edges.iter().map(|e| (e.from.clone(), e.to.clone(), e.kind.clone())).collect();

        let added_edge_keys = edges_b.difference(&edges_a);
        let added_edges: Vec<EdgeData> = b.edges
            .iter()
            .filter(|e| added_edge_keys.clone().any(|(from, to, kind)| from == e.from && to == e.to && kind == e.kind))
            .cloned()
            .collect();

        let removed_edge_keys = edges_a.difference(&edges_b);
        let removed_edges: Vec<EdgeData> = a.edges
            .iter()
            .filter(|e| removed_edge_keys.clone().any(|(from, to, kind)| from == e.from && to == e.to && kind == e.kind))
            .cloned()
            .collect();

        let stats = DiffStats {
            nodes_added: added_nodes.len(),
            nodes_removed: removed_nodes.len(),
            nodes_modified: modified_nodes.len(),
            edges_added: added_edges.len(),
            edges_removed: removed_edges.len(),
        };

        Self {
            added_nodes,
            removed_nodes,
            modified_nodes,
            added_edges,
            removed_edges,
            stats,
        }
    }

    /// Check if two nodes have meaningful differences.
    fn nodes_differ(a: &NodeData, b: &NodeData) -> bool {
        // Quality score changed > 0.1
        let quality_changed = match (a.quality_score, b.quality_score) {
            (Some(qa), Some(qb)) => (qa - qb).abs() > 0.1,
            _ => a.quality_score != b.quality_score,
        };

        // Fan-in changed > 5
        let fan_in_changed = match (a.fan_in, b.fan_in) {
            (Some(fa), Some(fb)) => (fa as i32 - fb as i32).abs() > 5,
            _ => a.fan_in != b.fan_in,
        };

        // Fan-out changed > 5
        let fan_out_changed = match (a.fan_out, b.fan_out) {
            (Some(fa), Some(fb)) => (fa as i32 - fb as i32).abs() > 5,
            _ => a.fan_out != b.fan_out,
        };

        quality_changed || fan_in_changed || fan_out_changed
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_node(id: &str, quality: f32, fan_in: usize, fan_out: usize) -> NodeData {
        NodeData {
            id: id.to_string(),
            label: id.to_string(),
            quality_score: Some(quality),
            fan_in: Some(fan_in),
            fan_out: Some(fan_out),
            is_orphan: false,
            has_unsafe: false,
            is_test: false,
            node_type: None,
        }
    }

    fn make_edge(from: &str, to: &str, kind: &str) -> EdgeData {
        EdgeData {
            from: from.to_string(),
            to: to.to_string(),
            kind: kind.to_string(),
        }
    }

    #[test]
    fn test_diff_empty_to_simple() {
        let a = GraphData { nodes: vec![], edges: vec![] };
        let b = GraphData {
            nodes: vec![make_node("a", 0.8, 1, 2)],
            edges: vec![make_edge("a", "b", "depends")],
        };

        let diff = SnapshotDiff::compute(&a, &b);
        assert_eq!(diff.stats.nodes_added, 1);
        assert_eq!(diff.stats.nodes_removed, 0);
        assert_eq!(diff.stats.edges_added, 1);
    }

    #[test]
    fn test_diff_no_changes() {
        let a = GraphData {
            nodes: vec![make_node("a", 0.8, 1, 2)],
            edges: vec![],
        };
        let b = a.clone();

        let diff = SnapshotDiff::compute(&a, &b);
        assert_eq!(diff.stats.nodes_added, 0);
        assert_eq!(diff.stats.nodes_removed, 0);
        assert_eq!(diff.stats.nodes_modified, 0);
    }

    #[test]
    fn test_diff_modified_node() {
        let a = GraphData {
            nodes: vec![make_node("a", 0.8, 1, 2)],
            edges: vec![],
        };
        let b = GraphData {
            nodes: vec![make_node("a", 0.6, 3, 4)],
            edges: vec![],
        };

        let diff = SnapshotDiff::compute(&a, &b);
        assert_eq!(diff.stats.nodes_modified, 1);
        assert!(diff.modified_nodes[0].old_quality.unwrap() > diff.modified_nodes[0].new_quality.unwrap());
    }

    #[test]
    fn test_diff_removed_node() {
        let a = GraphData {
            nodes: vec![make_node("a", 0.8, 1, 2)],
            edges: vec![],
        };
        let b = GraphData { nodes: vec![], edges: vec![] };

        let diff = SnapshotDiff::compute(&a, &b);
        assert_eq!(diff.stats.nodes_removed, 1);
        assert_eq!(diff.removed_nodes[0], "a");
    }
}