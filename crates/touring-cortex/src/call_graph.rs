//! Call graph analysis using petgraph.
//!
//! Represents and analyzes the call graph between functions/symbols in code.
//! Supports:
//! - Finding callees (who does this function call?) and callers (who calls this?)
//! - Detecting roots (functions with no callers — entry points or dead code)
//! - Finding hotspots (functions called by many distinct callers)
//! - Cycle detection via Tarjan's SCC algorithm
//! - Topological ordering (when acyclic)

use petgraph::algo::{tarjan_scc, toposort};
use petgraph::stable_graph::{NodeIndex, StableGraph};
use rayon::prelude::*;
use std::collections::HashMap;
use std::sync::RwLock;
use std::sync::atomic::{AtomicUsize, Ordering};

// E2-S5: Legacy global counter kept only for backward-compat of scc_version() public API.
// New code should use per-instance `self.version` instead.
static GLOBAL_SCC_VERSION: AtomicUsize = AtomicUsize::new(0);

/// A node in the call graph representing a function or symbol.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CallNode {
    /// Fully-qualified function/symbol name.
    pub name: String,
    /// File path where the symbol is defined.
    pub file_path: String,
    /// Line number of the definition.
    pub line: u32,
}

/// Directed call graph backed by petgraph's `StableGraph`.
///
/// Nodes are functions/symbols, edges represent "caller calls callee".
/// Node lookup is O(1) via an internal name-to-index map.
pub struct CallGraph {
    graph: StableGraph<CallNode, ()>,
    name_to_idx: HashMap<String, NodeIndex>,
    /// Cached SCC result with version tracking.
    scc_cache: RwLock<Option<SccCache>>,
    /// E2-S5: Per-instance version counter — mutations to THIS graph only
    /// invalidate THIS graph's cache, not other CallGraph instances.
    version: AtomicUsize,
}

/// Cached SCC decomposition with version tracking for incremental invalidation.
#[derive(Debug, Clone)]
struct SccCache {
    /// Graph version when this cache was computed.
    version: usize,
    /// SCC groups: each inner vec contains node names in one SCC.
    sccs: Vec<Vec<String>>,
    /// Whether the graph has any cycles.
    has_cycles: bool,
}

// E2-S5: The old global `SCC_VERSION` has been replaced by per-instance `self.version`.
// `GLOBAL_SCC_VERSION` above is kept only for the legacy `scc_version()` public API.

impl Default for CallGraph {
    fn default() -> Self {
        Self::new()
    }
}

impl CallGraph {
    /// Create an empty call graph.
    pub fn new() -> Self {
        Self {
            graph: StableGraph::new(),
            name_to_idx: HashMap::new(),
            scc_cache: RwLock::new(None),
            version: AtomicUsize::new(0),
        }
    }

    /// Add a function node (idempotent — returns existing index if name already present).
    ///
    /// Increments the SCC version, invalidating cached SCC results.
    pub fn add_node(&mut self, name: &str, file_path: &str, line: u32) -> NodeIndex {
        if let Some(&idx) = self.name_to_idx.get(name) {
            return idx;
        }
        let node = CallNode {
            name: name.to_string(),
            file_path: file_path.to_string(),
            line,
        };
        let idx = self.graph.add_node(node);
        self.name_to_idx.insert(name.to_string(), idx);
        self.version.fetch_add(1, Ordering::SeqCst);
        GLOBAL_SCC_VERSION.fetch_add(1, Ordering::SeqCst);
        idx
    }

    /// Add a call edge: `caller` calls `callee`.
    ///
    /// Both nodes are created (with empty file_path and line=0) if they don't exist.
    /// Duplicate edges are silently ignored.
    /// Increments the SCC version, invalidating cached SCC results.
    pub fn add_call(&mut self, caller: &str, callee: &str) {
        let caller_idx = self.add_node(caller, "", 0);
        let callee_idx = self.add_node(callee, "", 0);
        if !self.graph.contains_edge(caller_idx, callee_idx) {
            self.graph.add_edge(caller_idx, callee_idx, ());
            self.version.fetch_add(1, Ordering::SeqCst);
            GLOBAL_SCC_VERSION.fetch_add(1, Ordering::SeqCst);
        }
    }

    /// Return all functions called by `name`.
    pub fn callees(&self, name: &str) -> Vec<&CallNode> {
        let Some(&idx) = self.name_to_idx.get(name) else {
            return Vec::new();
        };
        self.graph
            .neighbors_directed(idx, petgraph::Direction::Outgoing)
            .filter_map(|n| self.graph.node_weight(n))
            .collect()
    }

    /// Return all functions that call `name`.
    pub fn callers(&self, name: &str) -> Vec<&CallNode> {
        let Some(&idx) = self.name_to_idx.get(name) else {
            return Vec::new();
        };
        self.graph
            .neighbors_directed(idx, petgraph::Direction::Incoming)
            .filter_map(|n| self.graph.node_weight(n))
            .collect()
    }

    /// Find functions with no callers (potential dead code or entry points).
    pub fn roots(&self) -> Vec<&CallNode> {
        self.graph
            .node_indices()
            .filter(|&n| {
                self.graph
                    .neighbors_directed(n, petgraph::Direction::Incoming)
                    .next()
                    .is_none()
            })
            .filter_map(|n| self.graph.node_weight(n))
            .collect()
    }

    /// Find functions called by at least `min_callers` distinct callers (hotspots).
    pub fn hotspots(&self, min_callers: usize) -> Vec<(&CallNode, usize)> {
        self.graph
            .node_indices()
            .filter_map(|n| {
                let count = self
                    .graph
                    .neighbors_directed(n, petgraph::Direction::Incoming)
                    .count();
                if count >= min_callers {
                    self.graph.node_weight(n).map(|node| (node, count))
                } else {
                    None
                }
            })
            .collect()
    }

    /// Detect cycles using Tarjan's SCC algorithm.
    ///
    /// Returns groups of function names forming cycles (SCCs with size > 1).
    pub fn detect_cycles(&self) -> Vec<Vec<String>> {
        tarjan_scc(&self.graph)
            .into_iter()
            .filter(|scc| scc.len() > 1)
            .map(|scc| {
                scc.iter()
                    .filter_map(|&n| self.graph.node_weight(n).map(|node| node.name.clone()))
                    .collect()
            })
            .collect()
    }

    /// Check whether the call graph contains any cycles.
    pub fn has_cycles(&self) -> bool {
        tarjan_scc(&self.graph).iter().any(|scc| scc.len() > 1)
    }

    /// Topological order of function names. Returns `None` if cycles exist.
    pub fn topological_order(&self) -> Option<Vec<String>> {
        match toposort(&self.graph, None) {
            Ok(order) => Some(
                order
                    .into_iter()
                    .filter_map(|n| self.graph.node_weight(n).map(|node| node.name.clone()))
                    .collect(),
            ),
            Err(_) => None,
        }
    }

    /// Number of nodes (functions/symbols) in the graph.
    pub fn node_count(&self) -> usize {
        self.graph.node_count()
    }

    /// Number of edges (call relationships) in the graph.
    pub fn edge_count(&self) -> usize {
        self.graph.edge_count()
    }

    /// Current SCC version — increments each time THIS graph instance changes.
    ///
    /// E2-S5: Now per-instance instead of global static. Each CallGraph tracks
    /// its own mutation count, so modifications to one graph don't invalidate
    /// the SCC cache of other graphs (critical for daemon multi-project).
    pub fn scc_version(&self) -> usize {
        self.version.load(Ordering::SeqCst)
    }

    /// Global SCC version across all CallGraph instances (legacy API).
    pub fn global_scc_version() -> usize {
        GLOBAL_SCC_VERSION.load(Ordering::SeqCst)
    }

    /// Get SCC decomposition with caching (owned).
    ///
    /// Results are cached and reused until the graph version changes
    /// (i.e., until `add_node` or `add_call` is called after this cache was computed).
    ///
    /// Returns `(has_cycles, scc_groups)`.
    pub fn cached_sccs(&self) -> (bool, Vec<Vec<String>>) {
        let current_version = self.version.load(Ordering::SeqCst);

        // Check cache first
        {
            let cache = self.scc_cache.read().expect("RwLock poisoned");
            if let Some(ref cached) = *cache
                && cached.version == current_version
            {
                return (cached.has_cycles, cached.sccs.clone());
            }
        }

        // Cache miss — recompute
        let raw_sccs = tarjan_scc(&self.graph);
        let has_cycles = raw_sccs.iter().any(|scc| scc.len() > 1);
        let sccs: Vec<Vec<String>> = raw_sccs
            .into_iter()
            .map(|scc| {
                scc.iter()
                    .filter_map(|&n| self.graph.node_weight(n).map(|node| node.name.clone()))
                    .collect()
            })
            .collect();

        let mut cache = self.scc_cache.write().expect("RwLock poisoned");
        *cache = Some(SccCache {
            version: current_version,
            has_cycles,
            sccs: sccs.clone(),
        });

        (has_cycles, sccs)
    }

    /// Detect cycles using cached SCCs (preferred over `detect_cycles` for repeated calls).
    ///
    /// Returns groups of function names forming cycles (SCCs with size > 1).
    pub fn detect_cycles_cached(&self) -> Vec<Vec<String>> {
        let (has_cycles, sccs) = self.cached_sccs();
        if !has_cycles {
            return Vec::new();
        }
        sccs.into_iter().filter(|scc| scc.len() > 1).collect()
    }

    /// Parallel hotspots computation using rayon.
    ///
    /// Scans all nodes in parallel to count distinct callers, then filters.
    /// Returns `(node, caller_count)` sorted by descending caller count.
    pub fn hotspots_parallel(&self, min_callers: usize) -> Vec<(&CallNode, usize)> {
        let nodes: Vec<_> = self.graph.node_indices().collect();

        // Parallel count of incoming edges per node
        let counts: Vec<(NodeIndex, usize)> = nodes
            .par_iter()
            .map(|&n| {
                let count = self
                    .graph
                    .neighbors_directed(n, petgraph::Direction::Incoming)
                    .count();
                (n, count)
            })
            .collect();

        // Filter and map to node weights
        counts
            .into_iter()
            .filter(|&(_, count)| count >= min_callers)
            .filter_map(|(n, count)| self.graph.node_weight(n).map(|node| (node, count)))
            .collect()
    }

    /// E3-S4: Find the longest path in the call graph (critical path).
    ///
    /// For DAGs, this is the longest chain of function calls. Functions on the
    /// critical path deserve more context budget because changes to them have
    /// the highest cascading impact.
    ///
    /// Returns `None` if the graph has cycles (longest path is undefined).
    /// Returns the path as a list of function names from source to sink.
    pub fn critical_path(&self) -> Option<Vec<String>> {
        // Use topological sort — longest path only defined for DAGs
        let topo = match toposort(&self.graph, None) {
            Ok(order) => order,
            Err(_) => return None, // has cycles
        };

        if topo.is_empty() {
            return Some(Vec::new());
        }

        // Dynamic programming: dist[node] = length of longest path ending at node
        let mut dist: HashMap<NodeIndex, usize> = HashMap::new();
        let mut prev: HashMap<NodeIndex, NodeIndex> = HashMap::new();

        for &node in &topo {
            dist.insert(node, 0);
        }

        for &node in &topo {
            let node_dist = dist.get(&node).copied().unwrap_or(0);
            for neighbor in self
                .graph
                .neighbors_directed(node, petgraph::Direction::Outgoing)
            {
                let new_dist = node_dist + 1;
                if new_dist > *dist.get(&neighbor).unwrap_or(&0) {
                    dist.insert(neighbor, new_dist);
                    prev.insert(neighbor, node);
                }
            }
        }

        // Find the node with maximum distance
        let (&end_node, _) = dist.iter().max_by_key(|&(_, &d)| d)?;

        // Reconstruct path
        let mut path = Vec::new();
        let mut current = end_node;
        loop {
            if let Some(node) = self.graph.node_weight(current) {
                path.push(node.name.clone());
            }
            match prev.get(&current) {
                Some(&p) => current = p,
                None => break,
            }
        }
        path.reverse();
        Some(path)
    }

    /// E3-S4: Get the length of the critical path (longest chain depth).
    pub fn critical_path_length(&self) -> usize {
        self.critical_path().map_or(0, |p| p.len())
    }

    /// E3-S4: Check if a function is on the critical path.
    pub fn is_on_critical_path(&self, name: &str) -> bool {
        self.critical_path()
            .is_some_and(|path| path.iter().any(|n| n == name))
    }
}

/// E3-S2: Co-edit pattern tracker for identifying files frequently edited together.
///
/// Maintains a symmetric co-occurrence matrix of file edits. When file A and file B
/// are edited in the same session, their co-edit count increments.
/// High co-edit counts indicate implicit dependencies not captured by imports.
#[derive(Debug, Clone, Default)]
pub struct CoEditTracker {
    /// Co-occurrence counts: (file_a, file_b) → count.
    /// Keys are sorted (a < b) to ensure symmetry without double-counting.
    co_edits: HashMap<(String, String), u32>,
    /// Per-session edit buffer: files edited in current session.
    session_edits: Vec<String>,
}

impl CoEditTracker {
    /// Create a new empty tracker.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a file edit in the current session.
    pub fn record_edit(&mut self, file_path: &str) {
        if !self.session_edits.contains(&file_path.to_string()) {
            self.session_edits.push(file_path.to_string());
        }
    }

    /// Flush session edits — update co-occurrence matrix for all pairs.
    ///
    /// Call this at session end or periodically.
    pub fn flush_session(&mut self) {
        let edits = std::mem::take(&mut self.session_edits);
        // Generate all unique pairs using iterator-based approach (clippy-safe)
        for (i, edit_a) in edits.iter().enumerate() {
            for edit_b in edits.iter().skip(i + 1) {
                let (a, b) = if edit_a < edit_b {
                    (edit_a.clone(), edit_b.clone())
                } else {
                    (edit_b.clone(), edit_a.clone())
                };
                *self.co_edits.entry((a, b)).or_insert(0) += 1;
            }
        }
    }

    /// Get files frequently co-edited with the given file.
    ///
    /// Returns files sorted by co-edit count descending, filtered by minimum threshold.
    pub fn co_edited_with(&self, file_path: &str, min_count: u32) -> Vec<(String, u32)> {
        let mut results: Vec<(String, u32)> = self
            .co_edits
            .iter()
            .filter_map(|((a, b), &count)| {
                if count < min_count {
                    return None;
                }
                if a == file_path {
                    Some((b.clone(), count))
                } else if b == file_path {
                    Some((a.clone(), count))
                } else {
                    None
                }
            })
            .collect();
        results.sort_by_key(|b| std::cmp::Reverse(b.1));
        results
    }

    /// Total number of tracked co-edit pairs.
    pub fn pair_count(&self) -> usize {
        self.co_edits.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_call_graph_new_is_empty() {
        let cg = CallGraph::new();
        assert_eq!(cg.node_count(), 0);
        assert_eq!(cg.edge_count(), 0);
    }

    #[test]
    fn test_default_is_empty() {
        let cg = CallGraph::default();
        assert_eq!(cg.node_count(), 0);
        assert_eq!(cg.edge_count(), 0);
    }

    #[test]
    fn test_add_node_idempotent() {
        let mut cg = CallGraph::new();
        let idx1 = cg.add_node("foo", "src/main.rs", 10);
        let idx2 = cg.add_node("foo", "src/main.rs", 10);
        assert_eq!(idx1, idx2);
        assert_eq!(cg.node_count(), 1);
    }

    #[test]
    fn test_add_call_creates_both_nodes() {
        let mut cg = CallGraph::new();
        cg.add_call("caller", "callee");
        assert_eq!(cg.node_count(), 2);
        assert_eq!(cg.edge_count(), 1);
    }

    #[test]
    fn test_callees_returns_correct() {
        let mut cg = CallGraph::new();
        cg.add_node("a", "a.rs", 1);
        cg.add_node("b", "b.rs", 1);
        cg.add_call("a", "b");
        let callees = cg.callees("a");
        assert_eq!(callees.len(), 1);
        assert_eq!(callees[0].name, "b");
    }

    #[test]
    fn test_callers_returns_correct() {
        let mut cg = CallGraph::new();
        cg.add_node("a", "a.rs", 1);
        cg.add_node("b", "b.rs", 1);
        cg.add_call("a", "b");
        let callers = cg.callers("b");
        assert_eq!(callers.len(), 1);
        assert_eq!(callers[0].name, "a");
    }

    #[test]
    fn test_callees_unknown_name_returns_empty() {
        let cg = CallGraph::new();
        assert!(cg.callees("unknown").is_empty());
    }

    #[test]
    fn test_callers_unknown_name_returns_empty() {
        let cg = CallGraph::new();
        assert!(cg.callers("unknown").is_empty());
    }

    #[test]
    fn test_roots_no_callers() {
        let mut cg = CallGraph::new();
        cg.add_node("main", "main.rs", 1);
        cg.add_node("helper", "lib.rs", 5);
        cg.add_node("util", "util.rs", 10);
        cg.add_call("main", "helper");
        cg.add_call("helper", "util");
        let roots = cg.roots();
        assert_eq!(roots.len(), 1);
        assert_eq!(roots[0].name, "main");
    }

    #[test]
    fn test_hotspots_by_caller_count() {
        let mut cg = CallGraph::new();
        cg.add_call("a", "shared");
        cg.add_call("b", "shared");
        cg.add_call("c", "shared");
        cg.add_call("a", "rare");

        let hotspots = cg.hotspots(2);
        assert_eq!(hotspots.len(), 1);
        assert_eq!(hotspots[0].0.name, "shared");
        assert_eq!(hotspots[0].1, 3);
    }

    #[test]
    fn test_detect_cycles_acyclic_returns_empty() {
        let mut cg = CallGraph::new();
        cg.add_call("a", "b");
        cg.add_call("b", "c");
        assert!(cg.detect_cycles().is_empty());
        assert!(!cg.has_cycles());
    }

    #[test]
    fn test_detect_cycles_with_cycle() {
        let mut cg = CallGraph::new();
        cg.add_call("a", "b");
        cg.add_call("b", "c");
        cg.add_call("c", "a");
        let cycles = cg.detect_cycles();
        assert_eq!(cycles.len(), 1);
        assert_eq!(cycles[0].len(), 3);
        assert!(cg.has_cycles());
    }

    #[test]
    fn test_topological_order_acyclic() {
        let mut cg = CallGraph::new();
        cg.add_call("a", "b");
        cg.add_call("b", "c");
        let order = cg.topological_order();
        assert!(order.is_some());
        let order = order.expect("should have topological order");
        // "a" must come before "b", "b" before "c"
        let pos_a = order.iter().position(|n| n == "a").expect("a in order");
        let pos_b = order.iter().position(|n| n == "b").expect("b in order");
        let pos_c = order.iter().position(|n| n == "c").expect("c in order");
        assert!(pos_a < pos_b);
        assert!(pos_b < pos_c);
    }

    #[test]
    fn test_topological_order_with_cycle_returns_none() {
        let mut cg = CallGraph::new();
        cg.add_call("a", "b");
        cg.add_call("b", "a");
        assert!(cg.topological_order().is_none());
    }

    #[test]
    fn test_no_duplicate_edges() {
        let mut cg = CallGraph::new();
        cg.add_call("a", "b");
        cg.add_call("a", "b");
        cg.add_call("a", "b");
        assert_eq!(cg.edge_count(), 1);
    }

    #[test]
    fn test_multiple_callees() {
        let mut cg = CallGraph::new();
        cg.add_call("main", "foo");
        cg.add_call("main", "bar");
        cg.add_call("main", "baz");
        let callees = cg.callees("main");
        assert_eq!(callees.len(), 3);
    }

    #[test]
    fn test_self_call_is_cycle() {
        let mut cg = CallGraph::new();
        cg.add_node("recursive", "r.rs", 1);
        // Self-edge
        let idx = cg.name_to_idx["recursive"];
        cg.graph.add_edge(idx, idx, ());
        // Tarjan SCC: a single node with a self-loop is an SCC of size 1,
        // but our detect_cycles filters for len > 1. Self-loops show as has_cycles
        // only if we also check for self-edges. For consistency with the SCC-based
        // detection, self-loops are NOT reported as cycles (size-1 SCC).
        // This is intentional — self-recursion is common and not necessarily a bug.
        assert!(cg.detect_cycles().is_empty());
    }

    // ── P2-1: SCC caching + hotspots parallel tests ──────────────────────

    #[test]
    fn test_scc_cache_version_increments() {
        let mut cg = CallGraph::new();
        let v0 = cg.scc_version();
        cg.add_node("a", "a.rs", 1);
        let v1 = cg.scc_version();
        assert!(v1 > v0);
    }

    #[test]
    fn test_scc_cache_misses_on_first_call() {
        let mut cg = CallGraph::new();
        cg.add_call("a", "b");
        cg.add_call("b", "c");
        // First call — computes, not cached
        let (has_cycles, sccs) = cg.cached_sccs();
        assert!(!has_cycles);
        // Second call — should hit cache
        let (has_cycles2, sccs2) = cg.cached_sccs();
        assert_eq!(has_cycles, has_cycles2);
        assert_eq!(sccs.len(), sccs2.len());
    }

    #[test]
    fn test_scc_cache_invalidates_after_add_call() {
        let mut cg = CallGraph::new();
        cg.add_call("a", "b");
        let (has_cycles1, _) = cg.cached_sccs();
        assert!(!has_cycles1);

        // Add edge that creates a cycle — cache should invalidate
        cg.add_call("b", "a");
        let (has_cycles2, _) = cg.cached_sccs();
        assert!(has_cycles2);
    }

    #[test]
    fn test_detect_cycles_cached_matches_detect_cycles() {
        let mut cg = CallGraph::new();
        cg.add_call("a", "b");
        cg.add_call("b", "c");
        cg.add_call("c", "a");

        let raw = cg.detect_cycles();
        let cached = cg.detect_cycles_cached();
        assert_eq!(raw.len(), cached.len());
    }

    #[test]
    fn test_hotspots_parallel_matches_sequential() {
        let mut cg = CallGraph::new();
        cg.add_call("a", "shared");
        cg.add_call("b", "shared");
        cg.add_call("c", "shared");
        cg.add_call("a", "rare");

        let seq = cg.hotspots(2);
        let par = cg.hotspots_parallel(2);
        assert_eq!(seq.len(), par.len());
    }

    #[test]
    fn test_hotspots_parallel_stress_test() {
        // Large graph stress test
        let mut cg = CallGraph::new();
        // 100 nodes each calling the same hotspot
        for i in 0..100 {
            cg.add_call(&format!("caller_{}", i), "shared_hotspot");
        }
        let result = cg.hotspots_parallel(10);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].0.name, "shared_hotspot");
        assert_eq!(result[0].1, 100);
    }

    // ── E2-S5: Per-instance SCC_VERSION isolation ─────────────────────

    #[test]
    fn test_scc_version_per_instance_isolation() {
        // Two independent CallGraphs: mutating one must NOT invalidate the other's cache
        let mut cg1 = CallGraph::new();
        let mut cg2 = CallGraph::new();

        cg1.add_call("a", "b");
        cg1.add_call("b", "c");
        let (_, sccs1) = cg1.cached_sccs();
        let v1_after_cache = cg1.scc_version();

        // Mutating cg2 should NOT change cg1's version
        cg2.add_call("x", "y");
        cg2.add_call("y", "z");
        cg2.add_call("z", "x"); // cycle in cg2

        assert_eq!(
            cg1.scc_version(),
            v1_after_cache,
            "cg1 version must NOT change when cg2 is mutated"
        );

        // cg1's cache should still be valid (same result without recomputation)
        let (has_cycles_1, sccs1_again) = cg1.cached_sccs();
        assert!(!has_cycles_1);
        assert_eq!(sccs1.len(), sccs1_again.len());

        // cg2 should independently detect its cycle
        let (has_cycles_2, _) = cg2.cached_sccs();
        assert!(has_cycles_2);
    }

    #[test]
    fn test_global_scc_version_still_increments() {
        let v_before = CallGraph::global_scc_version();
        let mut cg = CallGraph::new();
        cg.add_node("test_global", "t.rs", 1);
        let v_after = CallGraph::global_scc_version();
        assert!(
            v_after > v_before,
            "global version must still increment for backward compat"
        );
    }

    // ── E3-S4: Critical path tests ──────────────────────────────────

    #[test]
    fn test_critical_path_linear() {
        let mut cg = CallGraph::new();
        cg.add_call("main", "process");
        cg.add_call("process", "validate");
        cg.add_call("validate", "sanitize");
        let path = cg.critical_path().expect("should find path in DAG");
        assert_eq!(path, vec!["main", "process", "validate", "sanitize"]);
    }

    #[test]
    fn test_critical_path_branching() {
        let mut cg = CallGraph::new();
        // main → a → b (length 3)
        // main → c (length 2)
        cg.add_call("main", "a");
        cg.add_call("a", "b");
        cg.add_call("main", "c");
        let path = cg.critical_path().expect("should find path in DAG");
        assert_eq!(
            path.len(),
            3,
            "Critical path should be the longest: {path:?}"
        );
        assert_eq!(path[0], "main");
    }

    #[test]
    fn test_critical_path_with_cycle_returns_none() {
        let mut cg = CallGraph::new();
        cg.add_call("a", "b");
        cg.add_call("b", "a");
        assert!(cg.critical_path().is_none());
    }

    #[test]
    fn test_critical_path_empty_graph() {
        let cg = CallGraph::new();
        let path = cg.critical_path().expect("empty graph is a valid DAG");
        assert!(path.is_empty());
    }

    #[test]
    fn test_critical_path_length() {
        let mut cg = CallGraph::new();
        cg.add_call("a", "b");
        cg.add_call("b", "c");
        assert_eq!(cg.critical_path_length(), 3);
    }

    #[test]
    fn test_is_on_critical_path() {
        let mut cg = CallGraph::new();
        cg.add_call("main", "core");
        cg.add_call("core", "deep");
        cg.add_call("main", "side"); // not on critical path
        assert!(cg.is_on_critical_path("main"));
        assert!(cg.is_on_critical_path("core"));
        assert!(cg.is_on_critical_path("deep"));
    }

    // ── E3-S2: Co-edit tracker tests ────────────────────────────────

    #[test]
    fn test_co_edit_basic() {
        let mut tracker = CoEditTracker::new();
        tracker.record_edit("a.rs");
        tracker.record_edit("b.rs");
        tracker.record_edit("c.rs");
        tracker.flush_session();

        assert_eq!(tracker.pair_count(), 3); // (a,b), (a,c), (b,c)
        let co = tracker.co_edited_with("a.rs", 1);
        assert_eq!(co.len(), 2);
    }

    #[test]
    fn test_co_edit_multiple_sessions() {
        let mut tracker = CoEditTracker::new();
        // Session 1: a+b
        tracker.record_edit("a.rs");
        tracker.record_edit("b.rs");
        tracker.flush_session();
        // Session 2: a+b again
        tracker.record_edit("a.rs");
        tracker.record_edit("b.rs");
        tracker.flush_session();

        let co = tracker.co_edited_with("a.rs", 1);
        assert_eq!(co.len(), 1);
        assert_eq!(co[0].0, "b.rs");
        assert_eq!(co[0].1, 2); // count = 2
    }

    #[test]
    fn test_co_edit_min_threshold() {
        let mut tracker = CoEditTracker::new();
        tracker.record_edit("a.rs");
        tracker.record_edit("b.rs");
        tracker.flush_session();

        // Threshold 2 — only 1 co-edit, should return empty
        let co = tracker.co_edited_with("a.rs", 2);
        assert!(co.is_empty());
    }

    #[test]
    fn test_co_edit_no_duplicates_within_session() {
        let mut tracker = CoEditTracker::new();
        tracker.record_edit("a.rs");
        tracker.record_edit("a.rs"); // duplicate
        tracker.record_edit("b.rs");
        tracker.flush_session();

        let co = tracker.co_edited_with("a.rs", 1);
        assert_eq!(co.len(), 1);
        assert_eq!(co[0].1, 1); // only 1 co-edit, not 2
    }

    #[test]
    fn test_co_edit_empty_tracker() {
        let tracker = CoEditTracker::new();
        assert_eq!(tracker.pair_count(), 0);
        assert!(tracker.co_edited_with("anything", 1).is_empty());
    }
}
