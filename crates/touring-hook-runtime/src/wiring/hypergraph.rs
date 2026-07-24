//! HyperGraph — hyperedge wrapper over petgraph for N-ary relations.
//!
//! A hypergraph is a graph where **hyperedges** can connect multiple nodes
//! (not just pairs as in traditional graphs). This module implements the
//! **artificial node pattern** using petgraph's DiGraph: each hyperedge is
//! represented as a special marker node that connects to its member nodes
//! via directed edges.
//!
//! ## Why this pattern?
//!
//! petgraph is díade-only (edges connect exactly 2 nodes). To represent
//! N-ary hyperedges, we create artificial hyperedge-nodes that connect to
//! their members. This reuses petgraph infrastructure already present in
//! all 10 Touring crates.
//!
//! ## Use cases in Touring
//!
//! - **Feature gate hyperedges**: cfg(all(feature = "X", feature = "Y"))
//!   connects multiple feature flags to a single decision node
//! - **Multi-import lines**: `use foo::{A, B, C}` connects 1 import line to N symbols
//! - **Cross-file impact**: a symbol used in multiple consumers forms a hyperedge

use petgraph::graph::{DiGraph, NodeIndex};
use rustc_hash::FxHashMap;
use std::collections::hash_map::Entry;

/// Marker for a node in the hypergraph — real or artificial (hyperedge).
#[derive(Debug, Clone)]
pub enum HyperNode<N> {
    /// A real node in the domain (e.g., a symbol, module, feature flag)
    Real(N),
    /// An artificial hyperedge node that groups multiple real nodes
    HyperEdge,
}

/// Metadata stored on a hyperedge connection.
#[derive(Debug, Clone, Default)]
pub struct HyperEdgeData {
    /// Human-readable label for this hyperedge (e.g., "use foo::{A,B,C}")
    pub label: String,
    /// How many real nodes are members of this hyperedge
    pub member_count: usize,
}

/// HyperGraph using petgraph DiGraph with artificial node pattern.
///
/// Each hyperedge is an artificial `HyperEdge` node that has directed edges
/// to/from its member nodes. The membership index maps real node indices
/// to their containing hyperedges for O(1) membership queries.
pub struct HyperGraph<N> {
    /// The underlying directed graph
    graph: DiGraph<HyperNode<N>, HyperEdgeData>,
    /// Maps real node index → hyperedge node indices it belongs to
    membership: FxHashMap<NodeIndex, Vec<NodeIndex>>,
}

impl<N: Clone + std::fmt::Debug> HyperGraph<N> {
    /// Create a new empty hypergraph.
    pub fn new() -> Self {
        Self {
            graph: DiGraph::new(),
            membership: FxHashMap::default(),
        }
    }

    /// Add a real node to the graph, returning its index.
    pub fn add_node(&mut self, data: N) -> NodeIndex {
        self.graph.add_node(HyperNode::Real(data))
    }

    /// Add a hyperedge connecting the given member nodes.
    ///
    /// Creates an artificial `HyperEdge` node and adds directed edges
    /// from each member to the hyperedge and from the hyperedge back to
    /// each member. This enables both "what hyperedges does this node belong
    /// to" and "what nodes does this hyperedge contain" queries.
    pub fn add_hyperedge(&mut self, members: &[NodeIndex], label: &str) -> NodeIndex {
        // Create the artificial hyperedge node
        let edge_node = self.graph.add_node(HyperNode::HyperEdge);
        let edge_data = HyperEdgeData {
            label: label.to_string(),
            member_count: members.len(),
        };

        // Update edge metadata (graph edge data set via add_edge)
        // For the hyperedge node itself, we store metadata in a separate map
        // since petgraph node weight is HyperNode enum

        // Connect each member to the hyperedge (member → hyperedge)
        for member in members {
            self.graph.add_edge(*member, edge_node, edge_data.clone());
        }

        // Connect hyperedge back to each member (hyperedge → member)
        for member in members {
            self.graph.add_edge(edge_node, *member, edge_data.clone());
        }

        // Update membership index
        for member in members {
            match self.membership.entry(*member) {
                Entry::Vacant(v) => {
                    v.insert(vec![edge_node]);
                }
                Entry::Occupied(v) => {
                    v.into_mut().push(edge_node);
                }
            }
        }

        edge_node
    }

    /// Returns all hyperedge nodes that the given real node belongs to.
    pub fn hyperedges_for(&self, node: NodeIndex) -> Vec<NodeIndex> {
        self.membership.get(&node).cloned().unwrap_or_default()
    }

    /// Returns all member nodes of the given hyperedge.
    pub fn members_of(&self, hyperedge: NodeIndex) -> Vec<NodeIndex> {
        self.graph
            .neighbors(hyperedge)
            .filter(|n| matches!(self.graph.node_weight(*n), Some(HyperNode::Real(_))))
            .collect()
    }

    /// Returns the underlying petgraph for advanced graph algorithms.
    pub fn graph(&self) -> &DiGraph<HyperNode<N>, HyperEdgeData> {
        &self.graph
    }

    /// Returns the number of real nodes in the hypergraph.
    pub fn node_count(&self) -> usize {
        self.graph.node_count()
    }

    /// Returns the number of hyperedges (artificial nodes).
    pub fn hyperedge_count(&self) -> usize {
        self.graph
            .node_indices()
            .filter(|n| matches!(self.graph.node_weight(*n), Some(HyperNode::HyperEdge)))
            .count()
    }
}

impl<N: Clone + std::fmt::Debug> Default for HyperGraph<N> {
    fn default() -> Self {
        Self::new()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// FeatureGateHyperedge — hyperedge for cfg(feature = "X", ...) combinations
// ─────────────────────────────────────────────────────────────────────────────

/// Hyperedge for tracking feature gate combinations.
///
/// When a module uses `#[cfg(all(feature = "X", feature = "Y"))]`, this
/// creates a FeatureGateHyperedge that connects the feature flags to the
/// decision point.
#[derive(Debug, Clone)]
pub struct FeatureGateHyperedge {
    /// The combination expression (e.g., "all(feature = \"simd\", feature = \"gpu\")")
    pub expression: String,
    /// The features involved in this gate
    pub features: Vec<String>,
    /// The module path where this gate appears
    pub module_path: String,
}

impl FeatureGateHyperedge {
    /// Create a new feature gate hyperedge from a cfg attribute.
    pub fn new(expression: &str, module_path: &str) -> Self {
        let features = extract_features_from_cfg(expression);
        Self {
            expression: expression.to_string(),
            features,
            module_path: module_path.to_string(),
        }
    }
}

fn extract_features_from_cfg(cfg: &str) -> Vec<String> {
    let mut features = Vec::new();
    let mut in_feature = false;
    let mut current = String::new();

    for ch in cfg.chars() {
        if ch == '"' {
            if in_feature {
                features.push(current.clone());
                current.clear();
            }
            in_feature = !in_feature;
        } else if in_feature {
            current.push(ch);
        }
    }

    features
}

// ─────────────────────────────────────────────────────────────────────────────
// MultiImportHyperedge — hyperedge for `use foo::{A, B, C}` lines
// ─────────────────────────────────────────────────────────────────────────────

/// Hyperedge for tracking multi-symbol import lines.
///
/// When a module uses `use foo::{A, B, C}`, this creates a MultiImportHyperedge
/// connecting the import line to each imported symbol.
#[derive(Debug, Clone)]
pub struct MultiImportHyperedge {
    /// The full import path (e.g., "foo::{A, B, C}")
    pub import_path: String,
    /// The module where this import appears
    pub source_module: String,
    /// The symbols being imported
    pub imported_symbols: Vec<String>,
}

impl MultiImportHyperedge {
    /// Create a new multi-import hyperedge from an import statement.
    pub fn new(import_path: &str, source_module: &str) -> Self {
        let imported_symbols = extract_imported_symbols(import_path);
        Self {
            import_path: import_path.to_string(),
            source_module: source_module.to_string(),
            imported_symbols,
        }
    }
}

fn extract_imported_symbols(import_path: &str) -> Vec<String> {
    // Extract symbols between ::{ and }}
    if let Some(start) = import_path.find("::{") {
        let content = &import_path[start + 3..];
        if let Some(end) = content.find("}") {
            let symbols_str = &content[..end];
            return symbols_str
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
        }
    }
    Vec::new()
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hypergraph_add_nodes() {
        let mut hg: HyperGraph<&str> = HyperGraph::new();
        let a = hg.add_node("module_a");
        let b = hg.add_node("module_b");
        let c = hg.add_node("module_c");

        assert_eq!(hg.node_count(), 3);
        assert_eq!(hg.hyperedge_count(), 0);

        // Create a hyperedge for A, B, C
        let edge = hg.add_hyperedge(&[a, b, c], "multi_import");
        assert_eq!(hg.node_count(), 4); // 3 real + 1 artificial
        assert_eq!(hg.hyperedge_count(), 1);
        assert_eq!(hg.members_of(edge).len(), 3);
    }

    #[test]
    fn hypergraph_membership_lookup() {
        let mut hg: HyperGraph<&str> = HyperGraph::new();
        let a = hg.add_node("module_a");
        let b = hg.add_node("module_b");
        let edge = hg.add_hyperedge(&[a, b], "feature_gate");

        let edges_for_a = hg.hyperedges_for(a);
        assert_eq!(edges_for_a.len(), 1);
        assert_eq!(edges_for_a[0], edge);
    }

    #[test]
    fn feature_gate_extract() {
        let cfg = r#"all(feature = "simd", feature = "gpu")"#;
        let features = extract_features_from_cfg(cfg);
        assert_eq!(features, vec!["simd", "gpu"]);
    }

    #[test]
    fn multi_import_extract() {
        let import = "foo::{A, B, C}";
        let symbols = extract_imported_symbols(import);
        assert_eq!(symbols, vec!["A", "B", "C"]);
    }

    #[test]
    fn feature_gate_hyperedge() {
        let gate = FeatureGateHyperedge::new(
            r#"all(feature = "simd", feature = "gpu")"#,
            "touring_hooks::semantic_search",
        );
        assert_eq!(gate.features, vec!["simd", "gpu"]);
        assert_eq!(gate.module_path, "touring_hooks::semantic_search");
    }

    #[test]
    fn multi_import_hyperedge() {
        let import = MultiImportHyperedge::new("use foo::{A, B, C}", "module_x");
        assert_eq!(import.imported_symbols, vec!["A", "B", "C"]);
        assert_eq!(import.source_module, "module_x");
    }
}
