//! D41 — Graph Attention Layer for Code Graph Model
//!
//! Provides graph attention layer for code graph representation.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Configuration for graph attention.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphAttentionConfig {
    /// Embedding dimension.
    pub embedding_dim: usize,
    /// Number of attention heads.
    pub num_heads: usize,
    /// Dropout rate.
    pub dropout: f32,
    /// Context compression ratio (e.g., 512x).
    pub compression_ratio: usize,
}

impl Default for GraphAttentionConfig {
    fn default() -> Self {
        Self {
            embedding_dim: 128,
            num_heads: 4,
            dropout: 0.1,
            compression_ratio: 512,
        }
    }
}

/// A node in the code graph for attention.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphNode {
    /// Node identifier.
    pub id: String,
    /// Node features (embedding vector).
    pub features: Vec<f32>,
    /// Node type (function, struct, etc.).
    pub node_type: String,
}

/// An edge in the code graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphEdge {
    /// Source node id.
    pub source: String,
    /// Target node id.
    pub target: String,
    /// Edge weight.
    pub weight: f32,
    /// Edge type (calls, references, etc.).
    pub edge_type: String,
}

/// A code graph for attention processing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeGraph {
    /// Graph nodes indexed by id.
    pub nodes: HashMap<String, GraphNode>,
    /// Graph edges.
    pub edges: Vec<GraphEdge>,
}

impl Default for CodeGraph {
    fn default() -> Self {
        Self::new()
    }
}

impl CodeGraph {
    /// Create a new empty code graph.
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            edges: Vec::new(),
        }
    }

    /// Add a node to the graph.
    pub fn add_node(&mut self, node: GraphNode) {
        self.nodes.insert(node.id.clone(), node);
    }

    /// Add an edge to the graph.
    pub fn add_edge(&mut self, edge: GraphEdge) {
        self.edges.push(edge);
    }

    /// Get the number of nodes.
    pub fn num_nodes(&self) -> usize {
        self.nodes.len()
    }

    /// Get the number of edges.
    pub fn num_edges(&self) -> usize {
        self.edges.len()
    }

    /// Get neighbors of a node.
    pub fn neighbors(&self, node_id: &str) -> Vec<(String, f32)> {
        let mut result = Vec::new();

        for edge in &self.edges {
            if edge.source == node_id {
                result.push((edge.target.clone(), edge.weight));
            } else if edge.target == node_id {
                result.push((edge.source.clone(), edge.weight));
            }
        }

        result
    }
}

/// Graph attention layer for compressing code context.
///
/// Copies node features from the code graph into a HashMap. Uses a simple
/// weighted average of neighbor embeddings for compression.
///
/// Note: `compression_ratio` field is stored but never used — reserved for
/// future learnable attention weights.
#[derive(Debug, Clone)]
pub struct GraphAttentionLayer {
    /// Layer configuration.
    config: GraphAttentionConfig,
    /// Node embeddings (copied from graph nodes, not learned).
    #[allow(dead_code)]
    node_embeddings: HashMap<String, Vec<f32>>,
}

impl GraphAttentionLayer {
    /// Create a new graph attention layer.
    pub fn new(config: GraphAttentionConfig) -> Self {
        Self {
            config,
            node_embeddings: HashMap::new(),
        }
    }

    /// Initialize with a code graph.
    pub fn from_graph(&mut self, graph: &CodeGraph) {
        // Initialize embeddings for each node
        for (id, node) in &graph.nodes {
            let embedding = if node.features.is_empty() {
                // Random initialization if no features provided
                vec![0.0; self.config.embedding_dim]
            } else {
                node.features.clone()
            };
            self.node_embeddings.insert(id.clone(), embedding);
        }
    }

    /// Apply attention to compress context.
    ///
    /// Returns compressed representation of the subgraph centered on `center_node`.
    pub fn compress(&self, center_node: &str, graph: &CodeGraph) -> Vec<f32> {
        let neighbors = graph.neighbors(center_node);

        if neighbors.is_empty() {
            // Return zero vector if no neighbors
            return vec![0.0; self.config.embedding_dim];
        }

        // Simple attention: weighted average of neighbor embeddings
        let mut compressed = vec![0.0; self.config.embedding_dim];
        let mut total_weight = 0.0;

        for (neighbor_id, weight) in neighbors {
            if let Some(embedding) = self.node_embeddings.get(&neighbor_id) {
                for (i, val) in embedding.iter().enumerate() {
                    compressed[i] += val * weight;
                }
                total_weight += weight;
            }
        }

        if total_weight > 0.0 {
            for val in &mut compressed {
                *val /= total_weight;
            }
        }

        compressed
    }

    /// Get the attention weights for visualization.
    pub fn attention_weights(&self, center_node: &str, graph: &CodeGraph) -> Vec<(String, f32)> {
        let neighbors = graph.neighbors(center_node);

        if neighbors.is_empty() {
            return Vec::new();
        }

        let total_weight: f32 = neighbors.iter().map(|(_, w)| w).sum();

        neighbors
            .into_iter()
            .map(|(id, weight)| (id, weight / total_weight))
            .collect()
    }
}

impl Default for GraphAttentionLayer {
    fn default() -> Self {
        Self::new(GraphAttentionConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_code_graph_basic() {
        let mut graph = CodeGraph::new();

        graph.add_node(GraphNode {
            id: "func_a".to_string(),
            features: vec![1.0, 2.0, 3.0],
            node_type: "function".to_string(),
        });

        graph.add_node(GraphNode {
            id: "func_b".to_string(),
            features: vec![4.0, 5.0, 6.0],
            node_type: "function".to_string(),
        });

        graph.add_edge(GraphEdge {
            source: "func_a".to_string(),
            target: "func_b".to_string(),
            weight: 1.0,
            edge_type: "calls".to_string(),
        });

        assert_eq!(graph.num_nodes(), 2);
        assert_eq!(graph.num_edges(), 1);

        let neighbors = graph.neighbors("func_a");
        assert_eq!(neighbors.len(), 1);
        assert_eq!(neighbors[0].0, "func_b");
    }

    #[test]
    fn test_graph_attention_compress() {
        let config = GraphAttentionConfig {
            embedding_dim: 4,
            num_heads: 2,
            dropout: 0.0,
            compression_ratio: 512,
        };

        let mut layer = GraphAttentionLayer::new(config);

        let mut graph = CodeGraph::new();
        graph.add_node(GraphNode {
            id: "node1".to_string(),
            features: vec![1.0, 0.0, 0.0, 0.0],
            node_type: "test".to_string(),
        });
        graph.add_node(GraphNode {
            id: "node2".to_string(),
            features: vec![0.0, 1.0, 0.0, 0.0],
            node_type: "test".to_string(),
        });

        layer.from_graph(&graph);

        let compressed = layer.compress("node1", &graph);
        assert_eq!(compressed.len(), 4);
    }
}
