//! D39 L2 — Semantic graph (relationships)
//!
//! Provides semantic relationship graph between definitions.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};

/// Kind of semantic relationship.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SemanticRelationKind {
    /// Calls or invokes another definition.
    Calls,
    /// Defines or implements a member.
    Defines,
    /// References or uses a symbol.
    References,
    /// Inherits from or extends.
    InheritsFrom,
    /// Implements a trait.
    Implements,
    /// Module contains or exports.
    Contains,
    /// Type specialisation.
    Specializes,
    /// Other relationship.
    Other,
}

/// A semantic relationship between two definitions.
#[derive(Debug, Clone)]
pub struct SemanticRelation {
    /// Source definition name.
    pub from: String,
    /// Target definition name.
    pub to: String,
    /// Kind of relationship.
    pub kind: SemanticRelationKind,
    /// Optional metadata (e.g., call site location).
    pub metadata: Option<String>,
}

impl PartialEq for SemanticRelation {
    fn eq(&self, other: &Self) -> bool {
        self.from == other.from && self.to == other.to && self.kind == other.kind
    }
}

impl Eq for SemanticRelation {}

impl Hash for SemanticRelation {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.from.hash(state);
        self.to.hash(state);
        self.kind.hash(state);
    }
}

/// A node in the semantic graph.
#[derive(Debug, Clone)]
pub struct SemanticNode {
    /// Node identifier (definition name).
    pub id: String,
    /// File path.
    pub file_path: String,
    /// Line number.
    pub line: u32,
    /// Node kind.
    pub kind: String,
}

/// Semantic graph - L2 knowledge layer.
///
/// Represents semantic relationships between code definitions as a graph.
pub struct SemanticGraph {
    /// Nodes indexed by id.
    nodes: HashMap<String, SemanticNode>,
    /// Adjacency list: from -> Vec<(to, relation)>
    edges: HashMap<String, Vec<(String, SemanticRelation)>>,
    /// Reverse edges: to -> Vec<(from, relation)>
    reverse_edges: HashMap<String, Vec<(String, SemanticRelation)>>,
}

impl SemanticGraph {
    /// Create a new empty semantic graph.
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            edges: HashMap::new(),
            reverse_edges: HashMap::new(),
        }
    }

    /// Add a node to the graph.
    pub fn add_node(&mut self, node: SemanticNode) {
        self.nodes.entry(node.id.clone()).or_insert(node);
    }

    /// Add a directed edge to the graph.
    pub fn add_edge(&mut self, relation: SemanticRelation) {
        // Ensure nodes exist
        if !self.nodes.contains_key(&relation.from) {
            self.add_node(SemanticNode {
                id: relation.from.clone(),
                file_path: String::new(),
                line: 0,
                kind: "unknown".to_string(),
            });
        }
        if !self.nodes.contains_key(&relation.to) {
            self.add_node(SemanticNode {
                id: relation.to.clone(),
                file_path: String::new(),
                line: 0,
                kind: "unknown".to_string(),
            });
        }

        // Add edge
        self.edges
            .entry(relation.from.clone())
            .or_default()
            .push((relation.to.clone(), relation.clone()));

        // Add reverse edge
        self.reverse_edges
            .entry(relation.to.clone())
            .or_default()
            .push((relation.from.clone(), relation));
    }

    /// Get nodes reachable from a starting node.
    pub fn reachability(&self, start: &str) -> Vec<String> {
        let mut visited: Vec<String> = Vec::new();
        let mut queue: Vec<String> = vec![start.to_string()];

        while let Some(current) = queue.pop() {
            if visited.contains(&current) {
                continue;
            }
            visited.push(current.clone());

            if let Some(edges) = self.edges.get(&current) {
                for (next, _) in edges {
                    if !visited.contains(next) {
                        queue.push(next.clone());
                    }
                }
            }
        }

        visited
    }

    /// Get direct relationships for a definition.
    pub fn get_relations(&self, id: &str) -> Vec<SemanticRelation> {
        let mut results = Vec::new();

        if let Some(outgoing) = self.edges.get(id) {
            for (_, rel) in outgoing {
                results.push(rel.clone());
            }
        }

        if let Some(incoming) = self.reverse_edges.get(id) {
            for (_, rel) in incoming {
                results.push(rel.clone());
            }
        }

        results
    }
}

impl Default for SemanticGraph {
    fn default() -> Self {
        Self::new()
    }
}
