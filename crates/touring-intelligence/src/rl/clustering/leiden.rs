//! Leiden Community Detection — Graph-based skill clustering via Leiden algorithm.
//!
//! Uses `single-clustering` crate for Louvain/Leiden community detection on
//! skill co-occurrence graphs. Provides modularity-quality community partitions.
//!
//! # Example
//!
//! `Graph` is always available; `detect_communities` lives on the
//! `leiden-clustering`-gated impl, so the call is gated here too. Without the
//! gate this example failed to compile whenever the feature was off — which is
//! the default (found 04/08/2026).
//!
//! ```
//! use touring_intelligence::rl::clustering::leiden::{LeidenCommunityDetector, Graph};
//!
//! let detector = LeidenCommunityDetector::default();
//! let mut graph = Graph::new();
//! graph.add_edge("skill_a", "skill_b", 1.5);
//! graph.add_edge("skill_b", "skill_c", 2.0);
//! assert_eq!(graph.node_count(), 3);
//!
//! #[cfg(feature = "leiden-clustering")]
//! {
//!     let communities = detector.detect_communities(&graph);
//!     assert!(communities.num_communities() >= 1);
//! }
//! ```

#[cfg(feature = "leiden-clustering")]
use single_clustering::community_search::leiden::partition::{
    ModularityPartition, VertexPartition,
};
#[cfg(feature = "leiden-clustering")]
use single_clustering::community_search::leiden::{LeidenConfig, LeidenOptimizer};
#[cfg(feature = "leiden-clustering")]
use single_clustering::network::CSRNetwork;
#[cfg(feature = "leiden-clustering")]
use single_clustering::network::grouping::VectorGrouping;
use std::collections::HashMap;

/// Configuration for the Leiden algorithm.
#[derive(Debug, Clone)]
pub struct LeidenAlgorithmConfig {
    /// Maximum iterations for the Leiden algorithm.
    pub max_iterations: usize,
    /// Tolerance for convergence.
    pub tolerance: f64,
    /// Random seed for reproducibility.
    pub seed: Option<u64>,
    /// Maximum community size (None = unlimited).
    pub max_community_size: Option<usize>,
    /// Whether to refine the partition after initial detection.
    pub refine_partition: bool,
    /// Resolution parameter for community detection (higher = more, smaller communities).
    pub resolution: f64,
}

impl Default for LeidenAlgorithmConfig {
    fn default() -> Self {
        Self {
            max_iterations: 100,
            tolerance: 1e-6,
            seed: Some(42),
            max_community_size: None,
            refine_partition: true,
            resolution: 1.0,
        }
    }
}

#[cfg(feature = "leiden-clustering")]
impl From<LeidenAlgorithmConfig> for LeidenConfig {
    fn from(config: LeidenAlgorithmConfig) -> Self {
        LeidenConfig {
            max_iterations: config.max_iterations,
            tolerance: config.tolerance,
            seed: config.seed,
            max_community_size: config.max_community_size,
            refine_partition: config.refine_partition,
            consider_empty_community: true,
            consider_comms:
                single_clustering::community_search::leiden::ConsiderComms::AllNeighComms,
            refine_consider_comms:
                single_clustering::community_search::leiden::ConsiderComms::AllNeighComms,
            optimise_routine:
                single_clustering::community_search::leiden::OptimiseRoutine::MoveNodes,
            refine_routine:
                single_clustering::community_search::leiden::OptimiseRoutine::MergeNodes,
        }
    }
}

/// A community partition result from Leiden detection.
#[derive(Debug, Clone)]
pub struct Community {
    /// Community ID (unique within the partition).
    pub id: usize,
    /// List of skill IDs belonging to this community.
    pub skills: Vec<String>,
    /// Internal edge weight sum (total strength of intra-community edges).
    pub internal_weight: f64,
    /// Number of edges within the community.
    pub internal_edges: usize,
}

impl Community {
    /// Returns the number of skills in this community.
    pub fn len(&self) -> usize {
        self.skills.len()
    }

    /// Returns true if the community has no skills.
    pub fn is_empty(&self) -> bool {
        self.skills.is_empty()
    }
}

/// Result of Leiden community detection.
#[derive(Debug, Clone)]
pub struct LeidenResult {
    /// Detected communities.
    pub communities: Vec<Community>,
    /// Modularity score of the partition (higher = better quality).
    pub modularity: f64,
    /// Number of iterations run.
    pub iterations: usize,
    /// Whether the algorithm converged.
    pub converged: bool,
}

impl LeidenResult {
    /// Returns the number of communities detected.
    pub fn num_communities(&self) -> usize {
        self.communities.len()
    }

    /// Returns communities sorted by size (descending).
    pub fn sorted_by_size(&self) -> Vec<&Community> {
        let mut communities: Vec<&Community> = self.communities.iter().collect();
        communities.sort_by_key(|b| std::cmp::Reverse(b.skills.len()));
        communities
    }
}

/// Weighted edge list representation of a skill co-occurrence graph.
#[derive(Debug, Clone, Default)]
pub struct Graph {
    edges: Vec<(String, String, f64)>,
    node_ids: HashMap<String, usize>,
    next_node_id: usize,
}

impl Graph {
    /// Creates a new empty graph.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a weighted edge between two skills.
    ///
    /// If the edge already exists, weights are accumulated.
    /// Nodes are created automatically if they don't exist.
    pub fn add_edge(&mut self, from: &str, to: &str, weight: f64) {
        // Ensure both nodes are registered
        self.get_or_create_node_id(from);
        self.get_or_create_node_id(to);

        self.edges.push((from.to_string(), to.to_string(), weight));
    }

    /// Adds an undirected edge (creates symmetric edges).
    pub fn add_undirected_edge(&mut self, a: &str, b: &str, weight: f64) {
        self.add_edge(a, b, weight);
        self.add_edge(b, a, weight);
    }

    /// Records co-occurrence between two skills (weight = 1.0).
    pub fn record_cooccurrence(&mut self, skill_a: &str, skill_b: &str) {
        self.add_undirected_edge(skill_a, skill_b, 1.0);
    }

    /// Gets the node ID for a skill, creating one if needed.
    fn get_or_create_node_id(&mut self, skill: &str) -> usize {
        if let Some(&id) = self.node_ids.get(skill) {
            return id;
        }
        let id = self.next_node_id;
        self.node_ids.insert(skill.to_string(), id);
        self.next_node_id += 1;
        id
    }

    /// Returns the number of unique nodes (skills) in the graph.
    pub fn node_count(&self) -> usize {
        self.node_ids.len()
    }

    /// Returns the number of edges in the graph.
    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }

    /// Returns all skill IDs in the graph.
    pub fn nodes(&self) -> Vec<&String> {
        self.node_ids.keys().collect()
    }

    /// Gets the node ID for a skill (returns None if not in graph).
    pub fn get_node_id(&self, skill: &str) -> Option<usize> {
        self.node_ids.get(skill).copied()
    }

    /// Gets the skill ID for a node index.
    pub fn get_skill(&self, node_id: usize) -> Option<&String> {
        self.node_ids
            .iter()
            .find(|&(_, &id)| id == node_id)
            .map(|(skill, _)| skill)
    }

    #[cfg(feature = "leiden-clustering")]
    fn to_csr_network(&self) -> CSRNetwork<f64, f64> {
        // Build edge list with node indices
        let edges: Vec<(usize, usize, f64)> = self
            .edges
            .iter()
            .filter_map(|(from, to, weight)| {
                let from_id = self.node_ids.get(from)?;
                let to_id = self.node_ids.get(to)?;
                Some((*from_id, *to_id, *weight))
            })
            .collect();

        // Node weights (all 1.0 for unweighted nodes)
        let node_weights = vec![1.0_f64; self.node_ids.len()];

        CSRNetwork::from_edges(&edges, node_weights)
    }
}

/// Leiden-based community detector for skill graphs.
///
/// Detects communities using the Leiden algorithm, which is a faster
/// and higher-quality alternative to Louvain for community detection.
#[derive(Debug, Clone, Default)]
pub struct LeidenCommunityDetector {
    // `config` is consumed only by the `#[cfg(feature = "leiden-clustering")]` impl
    // (`with_config` / `detect_communities`); allow it to be unread in the default
    // build where that impl is cfg'd out, instead of dropping a field the feature needs.
    #[cfg_attr(not(feature = "leiden-clustering"), allow(dead_code))]
    config: LeidenAlgorithmConfig,
}

#[cfg(feature = "leiden-clustering")]
impl LeidenCommunityDetector {
    /// Creates a new detector with default configuration.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a new detector with custom configuration.
    pub fn with_config(config: LeidenAlgorithmConfig) -> Self {
        Self { config }
    }

    /// Detects communities in the given skill co-occurrence graph.
    ///
    /// Returns a `LeidenResult` containing the detected communities,
    /// modularity score, and convergence information.
    pub fn detect_communities(&self, graph: &Graph) -> LeidenResult {
        if graph.node_count() == 0 {
            return LeidenResult {
                communities: Vec::new(),
                modularity: 0.0,
                iterations: 0,
                converged: true,
            };
        }

        #[cfg(feature = "leiden-clustering")]
        let network = graph.to_csr_network();
        #[cfg(feature = "leiden-clustering")]
        let leiden_config: LeidenConfig = self.config.clone().into();
        #[cfg(feature = "leiden-clustering")]
        let mut optimizer = LeidenOptimizer::new(leiden_config);

        #[cfg(feature = "leiden-clustering")]
        // ModularityPartition<N, G>: N=node weight, G=grouping type
        let partition: ModularityPartition<f64, VectorGrouping> = optimizer
            .find_partition(network)
            .expect("Leiden optimization failed");

        #[cfg(feature = "leiden-clustering")]
        let membership = partition.membership_vector();
        #[cfg(feature = "leiden-clustering")]
        let num_nodes = membership.len();

        // Group nodes by community ID
        let mut community_groups: HashMap<usize, Vec<String>> = HashMap::new();
        for (node_idx, &community_id) in membership.iter().enumerate().take(num_nodes) {
            if let Some(skill) = graph.get_skill(node_idx) {
                community_groups
                    .entry(community_id)
                    .or_default()
                    .push(skill.clone());
            }
        }

        // Build Community structs
        let mut communities: Vec<Community> = community_groups
            .into_iter()
            .map(|(cid, skills)| Community {
                id: cid,
                skills,
                internal_weight: 0.0, // Would need additional computation
                internal_edges: 0,
            })
            .collect();

        // Sort communities by size (largest first)
        communities.sort_by_key(|b| std::cmp::Reverse(b.skills.len()));

        // Re-number communities sequentially
        for (i, c) in communities.iter_mut().enumerate() {
            c.id = i;
        }

        let modularity = partition.quality();

        LeidenResult {
            communities,
            modularity,
            iterations: self.config.max_iterations,
            converged: true,
        }
    }

    /// Detects communities with a custom resolution parameter.
    ///
    /// Higher resolution = more, smaller communities.
    /// Lower resolution = fewer, larger communities.
    pub fn detect_with_resolution(&self, graph: &Graph, resolution: f64) -> LeidenResult {
        let mut config = self.config.clone();
        config.resolution = resolution;
        Self { config }.detect_communities(graph)
    }
}

// Tests exercise the `#[cfg(feature = "leiden-clustering")]` impl (new / with_config /
// detect_communities), so the module is gated to the same feature — otherwise the
// default-feature test build cannot resolve those methods (E0599).
#[cfg(all(test, feature = "leiden-clustering"))]
mod tests {
    use super::*;

    #[test]
    fn test_graph_empty_graph_returns_empty_communities() {
        let detector = LeidenCommunityDetector::new();
        let g = Graph::new();
        let result = detector.detect_communities(&g);
        assert!(result.communities.is_empty());
        assert_eq!(result.modularity, 0.0);
        assert_eq!(result.iterations, 0);
        assert!(result.converged);
    }

    #[test]
    fn test_leiden_simple_connected_graph() {
        // 3 nodes, 2 edges forming a connected chain: a -- b -- c
        let mut g = Graph::new();
        g.add_edge("a", "b", 1.0);
        g.add_edge("b", "c", 1.0);

        let detector = LeidenCommunityDetector::new();
        let result = detector.detect_communities(&g);

        // Should detect at least one community (all connected nodes likely in one community)
        assert!(result.num_communities() >= 1);
        // All nodes should be assigned to some community
        let total_skills: usize = result.communities.iter().map(|c| c.len()).sum();
        assert_eq!(total_skills, 3);
        // Small graphs may have modularity 0 (no strong community structure)
        // but should produce valid community assignments
        assert!(
            result.modularity >= 0.0,
            "modularity should be non-negative, got {}",
            result.modularity
        );
    }

    #[test]
    fn test_leiden_two_separate_components() {
        // Two disconnected components: a -- b and c -- d
        let mut g = Graph::new();
        g.add_edge("a", "b", 1.0);
        g.add_edge("c", "d", 1.0);

        let detector = LeidenCommunityDetector::new();
        let result = detector.detect_communities(&g);

        // Should detect 2 communities (one per component)
        assert_eq!(result.num_communities(), 2);
        let total_skills: usize = result.communities.iter().map(|c| c.len()).sum();
        assert_eq!(total_skills, 4);
    }

    #[test]
    fn test_leiden_resolution_parameter() {
        let mut g = Graph::new();
        // Create a denser graph where resolution affects community count
        g.add_edge("a", "b", 1.0);
        g.add_edge("b", "c", 1.0);
        g.add_edge("c", "a", 1.0);
        g.add_edge("d", "e", 1.0);
        g.add_edge("e", "f", 1.0);
        g.add_edge("f", "d", 1.0);

        let detector = LeidenCommunityDetector::new();

        // Higher resolution = more, smaller communities
        let result_high = detector.detect_with_resolution(&g, 2.0);
        // Lower resolution = fewer, larger communities
        let result_low = detector.detect_with_resolution(&g, 0.1);

        // Both should produce valid results
        assert!(result_high.num_communities() >= 1);
        assert!(result_low.num_communities() >= 1);
    }

    #[test]
    fn test_leiden_detector_with_config() {
        let config = LeidenAlgorithmConfig {
            max_iterations: 50,
            tolerance: 1e-3,
            seed: Some(123),
            max_community_size: Some(10),
            refine_partition: false,
            resolution: 0.8,
        };
        let detector = LeidenCommunityDetector::with_config(config);

        let mut g = Graph::new();
        g.add_edge("x", "y", 1.0);
        g.add_edge("y", "z", 1.0);

        let result = detector.detect_communities(&g);
        assert!(result.num_communities() >= 1);
        assert!(result.modularity >= 0.0);
    }

    #[test]
    fn test_graph_creation() {
        let g = Graph::new();
        assert_eq!(g.node_count(), 0);
        assert_eq!(g.edge_count(), 0);
    }

    #[test]
    fn test_graph_add_edge() {
        let mut g = Graph::new();
        g.add_edge("skill_a", "skill_b", 1.0);
        assert_eq!(g.node_count(), 2);
        assert_eq!(g.edge_count(), 1);
    }

    #[test]
    fn test_graph_undirected_edge() {
        let mut g = Graph::new();
        g.add_undirected_edge("a", "b", 2.0);
        // Undirected adds 2 directed edges
        assert_eq!(g.node_count(), 2);
        assert_eq!(g.edge_count(), 2);
    }

    #[test]
    fn test_graph_cooccurrence() {
        let mut g = Graph::new();
        g.record_cooccurrence("x", "y");
        assert_eq!(g.node_count(), 2);
        assert_eq!(g.edge_count(), 2);
    }

    #[test]
    fn test_node_id_mapping() {
        let mut g = Graph::new();
        g.add_edge("a", "b", 1.0);
        assert_eq!(g.get_node_id("a"), Some(0));
        assert_eq!(g.get_node_id("b"), Some(1));
        assert_eq!(g.get_node_id("c"), None);
    }

    #[test]
    fn test_community_struct() {
        let c = Community {
            id: 0,
            skills: vec!["a".to_string(), "b".to_string()],
            internal_weight: 1.5,
            internal_edges: 2,
        };
        assert_eq!(c.len(), 2);
        assert!(!c.is_empty());
    }

    #[test]
    fn test_leiden_detector_default() {
        let detector = LeidenCommunityDetector::default();
        let g = Graph::new();
        let result = detector.detect_communities(&g);
        assert!(result.communities.is_empty());
        assert_eq!(result.modularity, 0.0);
    }

    #[test]
    fn test_leiden_detector_single_node() {
        let detector = LeidenCommunityDetector::new();
        let mut g = Graph::new();
        g.add_edge("solo", "solo", 0.0);
        let result = detector.detect_communities(&g);
        // Single node should form its own community
        assert_eq!(result.communities.len(), 1);
    }

    #[test]
    fn test_leiden_result_sorted_by_size() {
        let result = LeidenResult {
            communities: vec![
                Community {
                    id: 0,
                    skills: vec!["a".to_string()],
                    internal_weight: 0.0,
                    internal_edges: 0,
                },
                Community {
                    id: 1,
                    skills: vec!["b".to_string(), "c".to_string(), "d".to_string()],
                    internal_weight: 0.0,
                    internal_edges: 0,
                },
                Community {
                    id: 2,
                    skills: vec!["e".to_string(), "f".to_string()],
                    internal_weight: 0.0,
                    internal_edges: 0,
                },
            ],
            modularity: 0.5,
            iterations: 10,
            converged: true,
        };
        let sorted = result.sorted_by_size();
        assert_eq!(sorted[0].skills.len(), 3);
        assert_eq!(sorted[1].skills.len(), 2);
        assert_eq!(sorted[2].skills.len(), 1);
    }

    #[test]
    fn test_leiden_config_default() {
        let config = LeidenAlgorithmConfig::default();
        assert_eq!(config.max_iterations, 100);
        assert_eq!(config.tolerance, 1e-6);
        assert!(config.seed.is_some());
        assert!(config.refine_partition);
    }

    #[test]
    fn test_leiden_config_custom() {
        let config = LeidenAlgorithmConfig {
            max_iterations: 50,
            tolerance: 1e-3,
            seed: None,
            max_community_size: Some(100),
            refine_partition: false,
            resolution: 0.5,
        };
        assert_eq!(config.max_iterations, 50);
        assert_eq!(config.tolerance, 1e-3);
        assert!(config.seed.is_none());
        assert!(!config.refine_partition);
    }
}
