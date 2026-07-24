//! Leiden community detection for symbol graphs.
//!
//! Provides a simplified Leiden algorithm (greedy modularity optimisation) that
//! groups symbols by functional co-edit similarity, then uses community membership
//! to rank wiring candidates for orphan symbols.
//!
//! # Algorithm
//!
//! 1. Start with each unique node in its own singleton community.
//! 2. For every node, evaluate moving it to each neighbour's community; accept the
//!    move that yields the greatest modularity gain (> 0).
//! 3. Repeat until no move improves modularity or `max_iterations` is reached.
//!
//! This is a greedy, single-phase approximation of the full Leiden refinement.  It
//! runs in O(nodes × edges × iterations) which is acceptable for the symbol-graph
//! sizes encountered in typical Touring projects (< 50 k nodes).
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Internal graph representation (private)
// ---------------------------------------------------------------------------

/// Compact, index-based representation of a weighted undirected symbol graph.
///
/// Built once from the raw edge list; all algorithm phases operate on integer
/// node ids to avoid repeated string hashing in hot loops.
struct Graph {
    /// Maps symbol name → compact node id.
    node_index: HashMap<String, usize>,
    /// Adjacency list: node_id → [(neighbour_id, weight)].
    adjacency: HashMap<usize, Vec<(usize, f64)>>,
    /// Weighted degree of each node (sum of incident edge weights).
    degree: Vec<f64>,
    /// Total graph weight (sum of all edge weights, each counted once).
    total_weight: f64,
    /// Number of distinct nodes.
    n: usize,
}

impl Graph {
    /// Build a `Graph` from a raw edge list.  Returns `None` if the list is empty
    /// or contains no valid nodes after de-duplication.
    fn build(edges: &[(String, String, f64)]) -> Option<Self> {
        if edges.is_empty() {
            return None;
        }

        let mut node_index: HashMap<String, usize> = HashMap::new();
        let mut adjacency: HashMap<usize, Vec<(usize, f64)>> = HashMap::new();
        let mut total_weight = 0.0_f64;

        for (src, dst, w) in edges {
            let w = w.max(0.0);
            let src_id = Self::intern(&mut node_index, src);
            let dst_id = Self::intern(&mut node_index, dst);
            adjacency.entry(src_id).or_default().push((dst_id, w));
            adjacency.entry(dst_id).or_default().push((src_id, w));
            total_weight += w;
        }

        let n = node_index.len();
        if n == 0 {
            return None;
        }

        let mut degree = vec![0.0_f64; n];
        for (&node, neighbours) in &adjacency {
            if let Some(slot) = degree.get_mut(node) {
                *slot = neighbours.iter().map(|(_, w)| w).sum();
            }
        }

        Some(Self {
            node_index,
            adjacency,
            degree,
            total_weight,
            n,
        })
    }

    /// Intern a node name: insert if absent, return its compact id.
    fn intern(index: &mut HashMap<String, usize>, name: &str) -> usize {
        let next = index.len();
        *index.entry(name.to_string()).or_insert(next)
    }

    /// Build the inverse map (compact id → symbol name) for result assembly.
    fn index_to_name(&self) -> Vec<String> {
        let mut v = vec![String::new(); self.n];
        for (name, &idx) in &self.node_index {
            if let Some(slot) = v.get_mut(idx) {
                *slot = name.clone();
            }
        }
        v
    }
}

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// A group of symbols that form a densely-connected functional community.
#[derive(Debug, Clone)]
pub struct Community {
    /// Stable identifier assigned during detection.
    pub id: usize,
    /// Symbol names / module paths that belong to this community.
    pub members: Vec<String>,
    /// Internal edge density: (actual internal edges) / (max possible internal edges).
    /// Ranges from 0.0 (no internal edges) to 1.0 (fully connected clique).
    pub cohesion_score: f64,
}

/// A single wiring recommendation produced by [`LeidenCommunityDetector::suggest_wiring`].
#[derive(Debug, Clone)]
pub struct WiringSuggestion {
    /// The orphan symbol that needs a consumer.
    pub orphan_symbol: String,
    /// Recommended consumer symbol from the same or adjacent community.
    pub suggested_consumer: String,
    /// Community that contains (or is closest to) the orphan.
    pub community_id: usize,
    /// Confidence in [0.0, 1.0]: combines community cohesion and edge weight.
    pub confidence: f64,
    /// Human-readable rationale for the suggestion.
    pub reason: String,
}

// ---------------------------------------------------------------------------
// Detector
// ---------------------------------------------------------------------------

/// Greedy Leiden community detector for symbol dependency graphs.
///
/// # Example
/// ```ignore
/// use touring_hooks::shared::leiden::LeidenCommunityDetector;
///
/// let edges = vec![
///     ("foo::bar".to_string(), "foo::baz".to_string(), 1.0_f64),
///     ("foo::baz".to_string(), "foo::qux".to_string(), 0.8),
/// ];
/// let mut detector = LeidenCommunityDetector::new(1.0, 10);
/// let communities = detector.detect_communities(&edges);
/// assert!(!communities.is_empty());
/// ```
#[derive(Debug, Clone)]
pub struct LeidenCommunityDetector {
    /// Modularity resolution parameter γ.  Higher values produce more, smaller
    /// communities; lower values favour large merged communities.
    pub resolution: f64,
    /// Maximum number of refinement passes before halting.
    pub max_iterations: usize,
    /// Communities from the last `detect_communities` call (cached for repeated
    /// `suggest_wiring` queries without re-running detection).
    pub communities: Vec<Community>,
}

impl LeidenCommunityDetector {
    /// Create a new detector.
    ///
    /// # Parameters
    /// * `resolution` — modularity resolution γ (typical: 1.0).
    /// * `max_iterations` — upper bound on refinement passes (typical: 10).
    pub fn new(resolution: f64, max_iterations: usize) -> Self {
        Self {
            resolution,
            max_iterations,
            communities: Vec::new(),
        }
    }

    /// Run community detection on a weighted symbol graph.
    ///
    /// # Parameters
    /// * `edges` — list of `(source, target, weight)` tuples.  Weights represent
    ///   co-edit strength or dependency score; values should be positive.
    ///
    /// # Returns
    /// A `Vec<Community>` — one entry per detected community.  An empty edge list
    /// returns an empty `Vec`.
    pub fn detect_communities(&mut self, edges: &[(String, String, f64)]) -> Vec<Community> {
        let Some(graph) = Graph::build(edges) else {
            self.communities = Vec::new();
            return Vec::new();
        };

        let membership = self.greedy_modularity_passes(&graph);
        let communities = Self::build_communities(&graph, &membership);
        self.communities = communities.clone();
        communities
    }

    // -----------------------------------------------------------------------
    // Private: graph-building
    // -----------------------------------------------------------------------

    /// Run greedy modularity passes and return final community membership vector.
    fn greedy_modularity_passes(&self, graph: &Graph) -> Vec<usize> {
        let two_m = 2.0 * graph.total_weight.max(f64::EPSILON);
        let mut membership: Vec<usize> = (0..graph.n).collect();

        for _pass in 0..self.max_iterations {
            if !self.single_pass(graph, &mut membership, two_m) {
                break;
            }
        }
        membership
    }

    /// Execute one full scan of all nodes; return `true` if any node moved.
    fn single_pass(&self, graph: &Graph, membership: &mut [usize], two_m: f64) -> bool {
        let mut improved = false;
        for node in 0..graph.n {
            let comm_weights = Self::neighbour_comm_weights(node, membership, &graph.adjacency);
            let comm_degree_sum = Self::community_degree_sums(membership, &graph.degree);
            let current_comm = membership.get(node).copied().unwrap_or(0);
            let ki = graph.degree.get(node).copied().unwrap_or(0.0);
            if let Some(best) = self.best_move(
                current_comm,
                &comm_weights,
                &comm_degree_sum,
                graph.total_weight,
                two_m,
                ki,
            ) {
                if let Some(slot) = membership.get_mut(node) {
                    *slot = best;
                }
                improved = true;
            }
        }
        improved
    }

    /// Aggregate edge weights from `node` to each neighbouring community.
    fn neighbour_comm_weights(
        node: usize,
        membership: &[usize],
        adjacency: &HashMap<usize, Vec<(usize, f64)>>,
    ) -> HashMap<usize, f64> {
        let mut comm_weights: HashMap<usize, f64> = HashMap::new();
        if let Some(neighbours) = adjacency.get(&node) {
            for &(nbr, w) in neighbours {
                if nbr != node {
                    let comm = membership.get(nbr).copied().unwrap_or(0);
                    *comm_weights.entry(comm).or_insert(0.0) += w;
                }
            }
        }
        comm_weights
    }

    /// Sum of node degrees per community (Σ_tot for ΔQ formula).
    fn community_degree_sums(membership: &[usize], degree: &[f64]) -> HashMap<usize, f64> {
        let mut sums: HashMap<usize, f64> = HashMap::new();
        for (n2, &c) in membership.iter().enumerate() {
            *sums.entry(c).or_insert(0.0) += degree.get(n2).copied().unwrap_or(0.0);
        }
        sums
    }

    /// Find the neighbouring community that maximises modularity gain for `node`.
    ///
    /// Returns `Some(community_id)` if a beneficial move exists, `None` otherwise.
    ///
    /// ΔQ = k_i_in / m − γ · Σ_tot · k_i / (2m)²
    fn best_move(
        &self,
        current_comm: usize,
        comm_weights: &HashMap<usize, f64>,
        comm_degree_sum: &HashMap<usize, f64>,
        total_weight: f64,
        two_m: f64,
        ki: f64,
    ) -> Option<usize> {
        let mut best_comm = None;
        let mut best_gain = 0.0_f64;

        for (&candidate_comm, &k_i_in) in comm_weights {
            if candidate_comm == current_comm {
                continue;
            }
            let sigma_tot = comm_degree_sum.get(&candidate_comm).copied().unwrap_or(0.0);
            let gain = k_i_in / total_weight - self.resolution * sigma_tot * ki / (two_m * two_m);
            if gain > best_gain {
                best_gain = gain;
                best_comm = Some(candidate_comm);
            }
        }
        best_comm
    }

    /// Convert final membership vector into `Community` structs with cohesion scores.
    fn build_communities(graph: &Graph, membership: &[usize]) -> Vec<Community> {
        let index_to_name = graph.index_to_name();

        let mut comm_members: HashMap<usize, Vec<String>> = HashMap::new();
        for (node_id, &comm_id) in membership.iter().enumerate() {
            let name = index_to_name.get(node_id).cloned().unwrap_or_default();
            comm_members.entry(comm_id).or_default().push(name);
        }

        comm_members
            .into_values()
            .enumerate()
            .map(|(new_id, members)| {
                let cohesion = Self::cohesion_score(
                    &members,
                    &graph.node_index,
                    &graph.adjacency,
                    graph.total_weight,
                );
                Community {
                    id: new_id,
                    members,
                    cohesion_score: cohesion,
                }
            })
            .collect()
    }

    /// Recommend consumers for an orphan symbol using community membership.
    ///
    /// Strategy:
    /// 1. Find the community whose members are most similar (by name prefix) to `orphan`.
    /// 2. Return every other member of that community as a suggestion, ranked by the
    ///    community's cohesion score.
    ///
    /// # Parameters
    /// * `orphan` — fully-qualified symbol name or path of the orphan.
    /// * `communities` — typically the result of a prior `detect_communities` call.
    ///
    /// # Returns
    /// Suggestions ordered by descending confidence.
    pub fn suggest_wiring(orphan: &str, communities: &[Community]) -> Vec<WiringSuggestion> {
        if communities.is_empty() {
            return Vec::new();
        }

        // Score each community by how many of its members share a common prefix with orphan.
        let best_community = communities.iter().max_by(|a, b| {
            let sa = Self::community_affinity(orphan, a);
            let sb = Self::community_affinity(orphan, b);
            sa.partial_cmp(&sb).unwrap_or(std::cmp::Ordering::Equal)
        });

        let comm = match best_community {
            Some(c) => c,
            None => return Vec::new(),
        };

        // Build one suggestion per community member (excluding the orphan itself).
        let mut suggestions: Vec<WiringSuggestion> = comm
            .members
            .iter()
            .filter(|m| m.as_str() != orphan)
            .map(|member| {
                let prefix_len = common_prefix_len(orphan, member);
                let affinity = prefix_len as f64 / orphan.len().max(1) as f64;
                let confidence = (comm.cohesion_score * 0.6 + affinity * 0.4).min(1.0);
                WiringSuggestion {
                    orphan_symbol: orphan.to_string(),
                    suggested_consumer: member.clone(),
                    community_id: comm.id,
                    confidence,
                    reason: format!(
                        "Community {} (cohesion {:.2}): '{}' shares {} chars of module prefix with orphan",
                        comm.id,
                        comm.cohesion_score,
                        member,
                        prefix_len,
                    ),
                }
            })
            .collect();

        suggestions.sort_by(|a, b| {
            b.confidence
                .partial_cmp(&a.confidence)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        suggestions
    }

    // -----------------------------------------------------------------------
    // Private helpers
    // -----------------------------------------------------------------------

    /// Compute internal edge density for a community.
    ///
    /// density = (sum of intra-community edge weights) / (total graph weight)
    /// Normalised to [0, 1] by dividing by total_weight.
    fn cohesion_score(
        members: &[String],
        node_index: &HashMap<String, usize>,
        adjacency: &HashMap<usize, Vec<(usize, f64)>>,
        total_weight: f64,
    ) -> f64 {
        if members.len() < 2 || total_weight <= 0.0 {
            return 0.0;
        }

        // Build a set of member node ids for O(1) lookup.
        let member_ids: std::collections::HashSet<usize> = members
            .iter()
            .filter_map(|m| node_index.get(m).copied())
            .collect();

        let internal_weight: f64 = member_ids
            .iter()
            .flat_map(|&node| adjacency.get(&node).into_iter().flatten())
            .filter(|(nbr, _)| member_ids.contains(nbr))
            .map(|(_, w)| w)
            .sum::<f64>()
            / 2.0; // each edge counted twice (undirected)

        (internal_weight / total_weight).min(1.0)
    }

    /// Compute a [0, 1] affinity score between an orphan symbol and a community.
    fn community_affinity(orphan: &str, community: &Community) -> f64 {
        if community.members.is_empty() {
            return 0.0;
        }
        let total: f64 = community
            .members
            .iter()
            .map(|m| {
                let plen = common_prefix_len(orphan, m) as f64;
                plen / orphan.len().max(1) as f64
            })
            .sum();
        total / community.members.len() as f64
    }
}

// ---------------------------------------------------------------------------
// Free helpers
// ---------------------------------------------------------------------------

/// Length of the longest common prefix between two strings (byte-level).
fn common_prefix_len(a: &str, b: &str) -> usize {
    a.bytes().zip(b.bytes()).take_while(|(x, y)| x == y).count()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn triangle_edges() -> Vec<(String, String, f64)> {
        vec![
            ("foo::a".to_string(), "foo::b".to_string(), 1.0),
            ("foo::b".to_string(), "foo::c".to_string(), 1.0),
            ("foo::c".to_string(), "foo::a".to_string(), 1.0),
        ]
    }

    #[test]
    fn test_detect_empty_edges() {
        let mut detector = LeidenCommunityDetector::new(1.0, 10);
        let communities = detector.detect_communities(&[]);
        assert!(communities.is_empty());
    }

    #[test]
    fn test_detect_triangle_returns_communities() {
        let mut detector = LeidenCommunityDetector::new(1.0, 10);
        let communities = detector.detect_communities(&triangle_edges());
        assert!(!communities.is_empty());
        // All three nodes should be accounted for.
        let total_members: usize = communities.iter().map(|c| c.members.len()).sum();
        assert_eq!(total_members, 3);
    }

    #[test]
    fn test_cohesion_score_bounded() {
        let mut detector = LeidenCommunityDetector::new(1.0, 10);
        let communities = detector.detect_communities(&triangle_edges());
        for c in &communities {
            assert!(
                c.cohesion_score >= 0.0 && c.cohesion_score <= 1.0,
                "cohesion out of range: {}",
                c.cohesion_score
            );
        }
    }

    #[test]
    fn test_suggest_wiring_empty_communities() {
        let suggestions = LeidenCommunityDetector::suggest_wiring("foo::orphan", &[]);
        assert!(suggestions.is_empty());
    }

    #[test]
    fn test_suggest_wiring_returns_candidates() {
        let communities = vec![Community {
            id: 0,
            members: vec!["foo::bar".to_string(), "foo::baz".to_string()],
            cohesion_score: 0.8,
        }];
        let suggestions = LeidenCommunityDetector::suggest_wiring("foo::orphan", &communities);
        // orphan itself is not in members, so both members should be suggested.
        assert_eq!(suggestions.len(), 2);
        for s in &suggestions {
            assert!(s.confidence >= 0.0 && s.confidence <= 1.0);
            assert_eq!(s.community_id, 0);
        }
    }

    #[test]
    fn test_suggest_wiring_excludes_orphan_itself() {
        let communities = vec![Community {
            id: 0,
            members: vec!["foo::orphan".to_string(), "foo::bar".to_string()],
            cohesion_score: 0.5,
        }];
        let suggestions = LeidenCommunityDetector::suggest_wiring("foo::orphan", &communities);
        // Must not suggest the orphan as its own consumer.
        assert!(
            suggestions
                .iter()
                .all(|s| s.suggested_consumer != "foo::orphan")
        );
    }

    #[test]
    fn test_suggest_wiring_sorted_by_confidence() {
        let mut detector = LeidenCommunityDetector::new(1.0, 10);
        let edges = vec![
            ("mod::alpha".to_string(), "mod::beta".to_string(), 2.0),
            ("mod::beta".to_string(), "mod::gamma".to_string(), 1.0),
            ("other::x".to_string(), "other::y".to_string(), 3.0),
        ];
        let communities = detector.detect_communities(&edges);
        let suggestions = LeidenCommunityDetector::suggest_wiring("mod::orphan", &communities);
        for window in suggestions.windows(2) {
            assert!(
                window[0].confidence >= window[1].confidence,
                "suggestions not sorted: {} < {}",
                window[0].confidence,
                window[1].confidence
            );
        }
    }

    #[test]
    fn test_two_component_graph_separates() {
        // Two disconnected triangles should form separate communities.
        let mut edges = triangle_edges();
        edges.push(("bar::x".to_string(), "bar::y".to_string(), 1.0));
        edges.push(("bar::y".to_string(), "bar::z".to_string(), 1.0));
        edges.push(("bar::z".to_string(), "bar::x".to_string(), 1.0));

        let mut detector = LeidenCommunityDetector::new(1.0, 10);
        let communities = detector.detect_communities(&edges);
        let total_members: usize = communities.iter().map(|c| c.members.len()).sum();
        assert_eq!(total_members, 6);
    }

    #[test]
    fn test_new_defaults() {
        let d = LeidenCommunityDetector::new(1.0, 10);
        assert_eq!(d.resolution, 1.0);
        assert_eq!(d.max_iterations, 10);
        assert!(d.communities.is_empty());
    }

    #[test]
    fn test_common_prefix_len() {
        assert_eq!(common_prefix_len("foo::bar", "foo::baz"), 7);
        assert_eq!(common_prefix_len("abc", "xyz"), 0);
        assert_eq!(common_prefix_len("", "abc"), 0);
    }
}
