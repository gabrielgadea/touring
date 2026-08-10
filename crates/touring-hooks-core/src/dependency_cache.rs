//! DependencyCache — in-memory petgraph-backed dependency graph for fast blast_radius.
//!
//! Complements the SQL-backed `SymbolIndex` with an O(V+E) BFS traversal over a
//! `StableGraph`, avoiding lock contention on the daemon's shared index during
//! high-frequency pre-edit hook calls.
//!
//! # Build
//!
//! Populated incrementally via `add_relation(from, to)` — called from `post_edit`
//! and `post_read` after relations are inserted into SQLite. On daemon startup,
//! seeded from SQLite via `build_from_relations(iter)`.
//!
//! # Traversal
//!
//! `blast_radius(path)` performs BFS on the **reverse** graph (incoming edges) to
//! find all files that (transitively) depend on `path`. Uses
//! `petgraph::Direction::Incoming` to avoid materializing a separate transposed
//! graph.
//!
//! # Complexity
//!
//! | Operation       | Time         | Space  |
//! |-----------------|--------------|--------|
//! | `add_relation`  | O(1) amort.  | O(E)   |
//! | `blast_radius`  | O(V+E)       | O(V)   |
//! | `node_count`    | O(1)         | —      |

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use petgraph::Direction;
use petgraph::algo::{tarjan_scc, toposort};
use petgraph::stable_graph::{NodeIndex, StableGraph};
use petgraph::visit::Bfs;
use touring_code::ast::graph::BlastRadiusOutput;
use touring_rkyv::templates::ArchivedIndexSnapshot;

/// In-memory directed dependency graph.
///
/// An edge `A → B` means "A imports / depends on B".
/// `blast_radius(B)` traverses incoming edges to find all transitive dependents of B.
pub struct DependencyCache {
    /// Directed graph: A → B means A depends on B.
    graph: StableGraph<PathBuf, (), petgraph::Directed>,
    /// O(1) path → NodeIndex lookup.
    node_map: HashMap<PathBuf, NodeIndex>,
}

impl DependencyCache {
    /// Create an empty cache.
    pub fn new() -> Self {
        Self {
            graph: StableGraph::new(),
            node_map: HashMap::new(),
        }
    }

    /// Build from an iterator of `(from, to)` string pairs (e.g. from SQLite).
    ///
    /// # Example
    ///
    /// ```text
    /// let rows = conn.execute("SELECT source, target FROM file_relations", …);
    /// let cache = DependencyCache::build_from_relations(rows.map(|(s, t)| (s, t)));
    /// ```
    pub fn build_from_relations<I, S>(iter: I) -> Self
    where
        I: IntoIterator<Item = (S, S)>,
        S: AsRef<str>,
    {
        let mut cache = Self::new();
        for (from, to) in iter {
            cache.add_relation(&PathBuf::from(from.as_ref()), &PathBuf::from(to.as_ref()));
        }
        cache
    }

    /// Register a dependency edge: `from` imports `to`.
    ///
    /// Idempotent — duplicate edges are silently ignored.
    pub fn add_relation(&mut self, from: &PathBuf, to: &PathBuf) {
        let from_idx = self.get_or_insert(from);
        let to_idx = self.get_or_insert(to);
        // Avoid duplicate edges in O(degree) — acceptable for small out-degree.
        if !self.graph.contains_edge(from_idx, to_idx) {
            self.graph.add_edge(from_idx, to_idx, ());
        }
    }

    /// Remove a file node and all its edges (called on file deletion).
    ///
    /// `StableGraph` keeps remaining `NodeIndex` values valid after removal.
    pub fn remove_file(&mut self, path: &PathBuf) {
        if let Some(idx) = self.node_map.remove(path) {
            self.graph.remove_node(idx);
        }
    }

    /// Invalidate (remove and re-add) a file node to clear stale edges.
    ///
    /// Used after file edits when imports may have changed — removes the
    /// file's edges so the next `blast_radius` computes fresh dependencies.
    pub fn invalidate_file(&mut self, path: &PathBuf) {
        if let Some(idx) = self.node_map.remove(path) {
            self.graph.remove_node(idx);
        }
        // Re-insert the node so the file exists (but edges are cleared)
        self.get_or_insert(path);
    }

    /// BFS over incoming edges to find all transitive dependents of `path`.
    ///
    /// Returns the set of files that (directly or indirectly) depend on `path`,
    /// **excluding** `path` itself. Empty vec if `path` is unknown or isolated.
    ///
    /// Uses `petgraph::Direction::Incoming` so we don't materialise a transposed
    /// graph — `StableGraph` stores both forward and back adjacency internally.
    pub fn blast_radius(&self, path: &PathBuf) -> BlastRadiusOutput {
        let Some(&start_idx) = self.node_map.get(path) else {
            return BlastRadiusOutput::Files(Vec::new());
        };

        // petgraph's Bfs follows edges in one direction. We want "who imports me",
        // which is the Incoming direction on A→B edges (B's incoming = A files).
        // We pass the graph as a node-walker adapter with reversed direction via
        // `Reversed`. petgraph ≥ 0.6 exposes `Reversed` in `petgraph::visit`.
        use petgraph::visit::Reversed;
        let reversed = Reversed(&self.graph);
        let mut bfs = Bfs::new(&reversed, start_idx);

        let mut result = Vec::new();
        while let Some(idx) = bfs.next(&reversed) {
            if idx == start_idx {
                continue; // exclude the origin file
            }
            if let Some(p) = self.graph.node_weight(idx) {
                result.push(p.clone());
            }
        }
        BlastRadiusOutput::Files(result)
    }

    /// Direct (1-hop) dependents of `path` — O(in_degree).
    pub fn direct_dependents(&self, path: &PathBuf) -> Vec<PathBuf> {
        let Some(&idx) = self.node_map.get(path) else {
            return Vec::new();
        };
        self.graph
            .neighbors_directed(idx, Direction::Incoming)
            .filter_map(|n| self.graph.node_weight(n).cloned())
            .collect()
    }

    /// Direct dependencies of `path` (files it imports) — O(out_degree).
    pub fn direct_deps(&self, path: &PathBuf) -> Vec<PathBuf> {
        let Some(&idx) = self.node_map.get(path) else {
            return Vec::new();
        };
        self.graph
            .neighbors_directed(idx, Direction::Outgoing)
            .filter_map(|n| self.graph.node_weight(n).cloned())
            .collect()
    }

    /// Number of tracked files (nodes).
    pub fn node_count(&self) -> usize {
        self.graph.node_count()
    }

    /// Number of tracked dependency edges.
    pub fn edge_count(&self) -> usize {
        self.graph.edge_count()
    }

    /// Whether the cache contains any data.
    pub fn is_empty(&self) -> bool {
        self.graph.node_count() == 0
    }

    /// Detect strongly-connected components of size > 1 (i.e., dependency cycles).
    ///
    /// Returns a list of cycle groups — each group contains the files that form a cycle.
    /// Empty if the graph is a DAG (no cycles). Uses Tarjan's SCC algorithm: O(V+E).
    ///
    /// # Example
    ///
    /// ```ignore
    /// // a → b → a forms a cycle
    /// cache.add_relation(&p("a.rs"), &p("b.rs"));
    /// cache.add_relation(&p("b.rs"), &p("a.rs"));
    /// assert_eq!(cache.detect_cycles().len(), 1);
    /// ```
    pub fn detect_cycles(&self) -> Vec<Vec<PathBuf>> {
        tarjan_scc(&self.graph)
            .into_iter()
            .filter(|scc| scc.len() > 1)
            .map(|scc| {
                scc.iter()
                    .filter_map(|&n| self.graph.node_weight(n).cloned())
                    .collect()
            })
            .collect()
    }

    /// Return `true` if the dependency graph contains any circular imports.
    ///
    /// Cheaper than `detect_cycles()` when only the boolean result is needed.
    pub fn has_cycles(&self) -> bool {
        tarjan_scc(&self.graph).iter().any(|scc| scc.len() > 1)
    }

    /// Return files in topological order (dependents before dependencies).
    ///
    /// For each edge `A → B` (A imports B), A appears before B in the result.
    /// Returns `None` if the graph contains cycles (topological order is undefined).
    ///
    /// Useful for determining evaluation/build order across files.
    pub fn topological_order(&self) -> Option<Vec<PathBuf>> {
        match toposort(&self.graph, None) {
            Ok(order) => Some(
                order
                    .into_iter()
                    .filter_map(|n| self.graph.node_weight(n).cloned())
                    .collect(),
            ),
            Err(_) => None,
        }
    }

    // ── private helpers ───────────────────────────────────────────────────

    fn get_or_insert(&mut self, path: &PathBuf) -> NodeIndex {
        if let Some(&idx) = self.node_map.get(path) {
            return idx;
        }
        let idx = self.graph.add_node(path.clone());
        self.node_map.insert(path.clone(), idx);
        idx
    }
}

impl Default for DependencyCache {
    fn default() -> Self {
        Self::new()
    }
}

// ── rkyv IndexSnapshot Persistence ───────────────────────────────────────────

/// Zero-copy snapshot of the `DependencyCache` edge list for fast daemon cold start.
///
/// Serialized to `.claude/touring/index.rkyv` by the daemon at shutdown;
/// loaded on startup to skip the SQLite rebuild when the graph is unchanged.
///
/// The snapshot stores edges as `Vec<(String, String)>` — human-readable and
/// forward-compatible. On load, `DependencyCache::build_from_relations` rebuilds
/// the petgraph structure in O(E) time.
///
/// Uses `touring_rkyv::templates::ArchivedIndexSnapshot` for zero-copy
/// serialization via rkyv.
///
/// # Versioning
///
/// `schema_version` guards against stale snapshots. Bump it whenever the
/// snapshot layout changes; `load_rkyv` returns `Err` on mismatch so the caller
/// falls back to the SQLite seed path.
const INDEX_SNAPSHOT_SCHEMA_VERSION: u32 = 1;

/// Error from [`DependencyCache::save_rkyv`] / [`DependencyCache::load_rkyv`]
/// (F-8 / RBP-03: typed in place of `String`).
#[derive(Debug, thiserror::Error)]
pub enum DependencyCacheRkyvError {
    /// rkyv serialization of the index snapshot failed.
    #[error("ArchivedIndexSnapshot rkyv serialization failed: {0}")]
    Serialize(String),
    /// Writing the snapshot file failed.
    #[error("IndexSnapshot write failed: {0}")]
    Write(String),
    /// Reading the snapshot file failed.
    #[error("IndexSnapshot read failed: {0}")]
    Read(String),
    /// rkyv `check_bytes` validation of the snapshot failed.
    #[error("ArchivedIndexSnapshot rkyv validation failed: {0}")]
    Validate(String),
    /// The snapshot's `schema_version` does not match the current build.
    #[error("ArchivedIndexSnapshot schema mismatch: expected {expected}, got {got}")]
    SchemaMismatch {
        /// Schema version required by the current build.
        expected: u32,
        /// Schema version found in the snapshot.
        got: u32,
    },
    /// rkyv deserialization of the validated snapshot failed.
    #[error("ArchivedIndexSnapshot deserialization failed: {0}")]
    Deserialize(String),
}

impl DependencyCache {
    /// Serialize the current graph to an rkyv snapshot file.
    ///
    /// Collects all edges as `(from, to)` string pairs and writes them with
    /// `touring_rkyv::to_bytes`. Uses a 64 KiB alignment buffer — adequate for typical
    /// project graphs (< 10 k edges).
    ///
    /// # Errors
    ///
    /// Returns a descriptive `String` error on serialization or I/O failure.
    pub fn save_rkyv(&self, path: &Path) -> Result<(), DependencyCacheRkyvError> {
        let edges: Vec<(String, String)> = self
            .graph
            .edge_indices()
            .filter_map(|ei| {
                let (a, b) = self.graph.edge_endpoints(ei)?;
                let from = self.graph.node_weight(a)?.to_string_lossy().into_owned();
                let to = self.graph.node_weight(b)?.to_string_lossy().into_owned();
                Some((from, to))
            })
            .collect();

        let snapshot = ArchivedIndexSnapshot {
            node_count: self.graph.node_count(),
            schema_version: INDEX_SNAPSHOT_SCHEMA_VERSION,
            edges,
        };

        let bytes = touring_rkyv::to_bytes::<_, 65536>(&snapshot)
            .map_err(|e| DependencyCacheRkyvError::Serialize(e.to_string()))?;
        std::fs::write(path, &bytes).map_err(|e| DependencyCacheRkyvError::Write(e.to_string()))
    }

    /// Load a `DependencyCache` from an rkyv snapshot file.
    ///
    /// Validates `check_bytes` and `schema_version` before deserializing.
    /// Returns `Err` on any mismatch so callers can fall back to SQLite rebuild.
    ///
    /// # Errors
    ///
    /// Returns a descriptive `String` error if the file is missing, corrupted,
    /// or written with a different `schema_version`.
    pub fn load_rkyv(path: &Path) -> Result<Self, DependencyCacheRkyvError> {
        let bytes =
            std::fs::read(path).map_err(|e| DependencyCacheRkyvError::Read(e.to_string()))?;
        let archived = touring_rkyv::check_archived_root::<ArchivedIndexSnapshot>(&bytes)
            .map_err(|e| DependencyCacheRkyvError::Validate(e.to_string()))?;

        if archived.schema_version != INDEX_SNAPSHOT_SCHEMA_VERSION {
            return Err(DependencyCacheRkyvError::SchemaMismatch {
                expected: INDEX_SNAPSHOT_SCHEMA_VERSION,
                got: archived.schema_version.into(),
            });
        }

        let snapshot: ArchivedIndexSnapshot = touring_rkyv::deserialize(archived)
            .map_err(|e| DependencyCacheRkyvError::Deserialize(e.to_string()))?;

        Ok(Self::build_from_relations(snapshot.edges))
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    #[test]
    fn test_empty_cache() {
        let cache = DependencyCache::new();
        assert!(cache.is_empty());
        assert_eq!(
            cache.blast_radius(&p("a.rs")).files(),
            Vec::<PathBuf>::new()
        );
    }

    #[test]
    fn test_single_edge() {
        let mut cache = DependencyCache::new();
        cache.add_relation(&p("a.rs"), &p("b.rs")); // a imports b
        // b's blast_radius should include a
        let mut br = cache.blast_radius(&p("b.rs")).files();
        br.sort();
        assert_eq!(br, vec![p("a.rs")]);
        // a's blast_radius should be empty (nothing imports a)
        assert_eq!(
            cache.blast_radius(&p("a.rs")).files(),
            Vec::<PathBuf>::new()
        );
    }

    #[test]
    fn test_transitive_blast_radius() {
        // c ← b ← a (a imports b, b imports c)
        let mut cache = DependencyCache::new();
        cache.add_relation(&p("a.rs"), &p("b.rs"));
        cache.add_relation(&p("b.rs"), &p("c.rs"));
        // c's blast_radius = {b, a} (transitively)
        let mut br = cache.blast_radius(&p("c.rs")).files();
        br.sort();
        assert_eq!(br, vec![p("a.rs"), p("b.rs")]);
        // b's blast_radius = {a} only
        assert_eq!(cache.blast_radius(&p("b.rs")).files(), vec![p("a.rs")]);
    }

    #[test]
    fn test_diamond_dependency() {
        // d ← b ← a, d ← c ← a  (diamond)
        let mut cache = DependencyCache::new();
        cache.add_relation(&p("a.rs"), &p("b.rs"));
        cache.add_relation(&p("a.rs"), &p("c.rs"));
        cache.add_relation(&p("b.rs"), &p("d.rs"));
        cache.add_relation(&p("c.rs"), &p("d.rs"));
        // d's blast_radius = {b, c, a}
        let mut br = cache.blast_radius(&p("d.rs")).files();
        br.sort();
        assert_eq!(br, vec![p("a.rs"), p("b.rs"), p("c.rs")]);
    }

    #[test]
    fn test_idempotent_add_relation() {
        let mut cache = DependencyCache::new();
        cache.add_relation(&p("a.rs"), &p("b.rs"));
        cache.add_relation(&p("a.rs"), &p("b.rs")); // duplicate — ignored
        assert_eq!(cache.edge_count(), 1);
    }

    #[test]
    fn test_build_from_relations() {
        let relations = vec![
            ("src/main.rs", "src/lib.rs"),
            ("src/lib.rs", "src/utils.rs"),
        ];
        let cache = DependencyCache::build_from_relations(relations);
        assert_eq!(cache.node_count(), 3);
        assert_eq!(cache.edge_count(), 2);
        let mut br = cache.blast_radius(&p("src/utils.rs")).files();
        br.sort();
        assert_eq!(br, vec![p("src/lib.rs"), p("src/main.rs")]);
    }

    #[test]
    fn test_direct_dependents() {
        let mut cache = DependencyCache::new();
        cache.add_relation(&p("a.rs"), &p("c.rs"));
        cache.add_relation(&p("b.rs"), &p("c.rs"));
        let mut deps = cache.direct_dependents(&p("c.rs"));
        deps.sort();
        assert_eq!(deps, vec![p("a.rs"), p("b.rs")]);
    }

    #[test]
    fn test_direct_deps() {
        let mut cache = DependencyCache::new();
        cache.add_relation(&p("a.rs"), &p("b.rs"));
        cache.add_relation(&p("a.rs"), &p("c.rs"));
        let mut deps = cache.direct_deps(&p("a.rs"));
        deps.sort();
        assert_eq!(deps, vec![p("b.rs"), p("c.rs")]);
    }

    #[test]
    fn test_remove_file() {
        let mut cache = DependencyCache::new();
        cache.add_relation(&p("a.rs"), &p("b.rs"));
        assert_eq!(cache.node_count(), 2);
        cache.remove_file(&p("a.rs"));
        assert_eq!(cache.node_count(), 1);
        // After removing a, b has no dependents
        assert_eq!(
            cache.blast_radius(&p("b.rs")).files(),
            Vec::<PathBuf>::new()
        );
    }

    #[test]
    fn test_unknown_file_returns_empty() {
        let cache = DependencyCache::new();
        assert_eq!(
            cache.blast_radius(&p("nonexistent.rs")).files(),
            Vec::<PathBuf>::new()
        );
        assert_eq!(
            cache.direct_dependents(&p("nonexistent.rs")),
            Vec::<PathBuf>::new()
        );
    }

    // ── rkyv IndexSnapshot Tests ──────────────────────────────────────────────

    #[test]
    fn test_index_snapshot_roundtrip_empty() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let path = dir.path().join("index.rkyv");

        let cache = DependencyCache::new();
        cache.save_rkyv(&path).expect("save empty snapshot");

        let loaded = DependencyCache::load_rkyv(&path).expect("load empty snapshot");
        assert!(loaded.is_empty());
        assert_eq!(loaded.node_count(), 0);
        assert_eq!(loaded.edge_count(), 0);
    }

    #[test]
    fn test_index_snapshot_roundtrip_with_edges() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let path = dir.path().join("index.rkyv");

        let mut cache = DependencyCache::new();
        cache.add_relation(&p("src/main.rs"), &p("src/lib.rs"));
        cache.add_relation(&p("src/lib.rs"), &p("src/utils.rs"));
        cache.add_relation(&p("src/main.rs"), &p("src/utils.rs"));

        cache.save_rkyv(&path).expect("save snapshot");

        let loaded = DependencyCache::load_rkyv(&path).expect("load snapshot");
        assert_eq!(loaded.node_count(), 3);
        assert_eq!(loaded.edge_count(), 3);

        // blast_radius must be preserved after round-trip
        let mut br = loaded.blast_radius(&p("src/utils.rs")).files();
        br.sort();
        assert_eq!(br, vec![p("src/lib.rs"), p("src/main.rs")]);
    }

    #[test]
    fn test_index_snapshot_roundtrip_diamond() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let path = dir.path().join("diamond.rkyv");

        // d ← b ← a, d ← c ← a
        let mut cache = DependencyCache::new();
        cache.add_relation(&p("a.rs"), &p("b.rs"));
        cache.add_relation(&p("a.rs"), &p("c.rs"));
        cache.add_relation(&p("b.rs"), &p("d.rs"));
        cache.add_relation(&p("c.rs"), &p("d.rs"));

        cache.save_rkyv(&path).expect("save");
        let loaded = DependencyCache::load_rkyv(&path).expect("load");

        let mut br = loaded.blast_radius(&p("d.rs")).files();
        br.sort();
        assert_eq!(br, vec![p("a.rs"), p("b.rs"), p("c.rs")]);
    }

    #[test]
    fn test_index_snapshot_missing_file_returns_err() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let path = dir.path().join("nonexistent.rkyv");
        assert!(DependencyCache::load_rkyv(&path).is_err());
    }

    #[test]
    fn test_index_snapshot_corrupted_returns_err() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let path = dir.path().join("corrupted.rkyv");
        std::fs::write(&path, b"not valid rkyv bytes").expect("write");
        assert!(DependencyCache::load_rkyv(&path).is_err());
    }

    // ── Cycle detection tests ─────────────────────────────────────────────

    #[test]
    fn test_no_cycles_in_dag() {
        let mut cache = DependencyCache::new();
        cache.add_relation(&p("a.rs"), &p("b.rs"));
        cache.add_relation(&p("b.rs"), &p("c.rs"));
        assert!(!cache.has_cycles());
        assert!(cache.detect_cycles().is_empty());
    }

    #[test]
    fn test_simple_cycle_detected() {
        // a → b → a forms a 2-node SCC
        let mut cache = DependencyCache::new();
        cache.add_relation(&p("a.rs"), &p("b.rs"));
        cache.add_relation(&p("b.rs"), &p("a.rs"));
        assert!(cache.has_cycles());
        let cycles = cache.detect_cycles();
        assert_eq!(cycles.len(), 1);
        let mut cycle = cycles[0].clone();
        cycle.sort();
        assert_eq!(cycle, vec![p("a.rs"), p("b.rs")]);
    }

    #[test]
    fn test_three_node_cycle_detected() {
        // a → b → c → a
        let mut cache = DependencyCache::new();
        cache.add_relation(&p("a.rs"), &p("b.rs"));
        cache.add_relation(&p("b.rs"), &p("c.rs"));
        cache.add_relation(&p("c.rs"), &p("a.rs"));
        assert!(cache.has_cycles());
        let cycles = cache.detect_cycles();
        assert_eq!(cycles.len(), 1);
        assert_eq!(cycles[0].len(), 3);
    }

    #[test]
    fn test_mixed_graph_only_reports_cycle_nodes() {
        // a → b → a (cycle), c → d (no cycle)
        let mut cache = DependencyCache::new();
        cache.add_relation(&p("a.rs"), &p("b.rs"));
        cache.add_relation(&p("b.rs"), &p("a.rs"));
        cache.add_relation(&p("c.rs"), &p("d.rs"));
        let cycles = cache.detect_cycles();
        // Only a and b are in a cycle
        assert_eq!(cycles.len(), 1);
        let mut cycle = cycles[0].clone();
        cycle.sort();
        assert_eq!(cycle, vec![p("a.rs"), p("b.rs")]);
    }

    // ── Topological order tests ───────────────────────────────────────────

    #[test]
    fn test_topological_order_dag() {
        // a → b → c: topological order has a before b, b before c
        let mut cache = DependencyCache::new();
        cache.add_relation(&p("a.rs"), &p("b.rs"));
        cache.add_relation(&p("b.rs"), &p("c.rs"));
        let order = cache
            .topological_order()
            .expect("DAG must have topological order");
        assert_eq!(order.len(), 3);
        let pos_a = order.iter().position(|x| x == &p("a.rs")).unwrap();
        let pos_b = order.iter().position(|x| x == &p("b.rs")).unwrap();
        let pos_c = order.iter().position(|x| x == &p("c.rs")).unwrap();
        assert!(pos_a < pos_b, "a (importer) before b");
        assert!(pos_b < pos_c, "b (importer) before c");
    }

    #[test]
    fn test_topological_order_returns_none_on_cycle() {
        let mut cache = DependencyCache::new();
        cache.add_relation(&p("a.rs"), &p("b.rs"));
        cache.add_relation(&p("b.rs"), &p("a.rs"));
        assert!(cache.topological_order().is_none());
    }

    #[test]
    fn test_topological_order_empty_graph() {
        let cache = DependencyCache::new();
        let order = cache
            .topological_order()
            .expect("empty graph is a valid DAG");
        assert!(order.is_empty());
    }
}
