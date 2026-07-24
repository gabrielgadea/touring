//! Pheromone maps — ACO-style co-access and dependency-edge trails.
//!
//! Uses [`indexmap::IndexMap`] for O(1) key lookup while preserving insertion
//! order.  This enables both positional access (via `get_index()`) and
//! stable iteration order which matters for deterministic behavior in tests
//! and in production diagnostics.
//!
//! For pheromone strength matrices, we use a nested IndexMap approach
//! (`IndexMap<String, IndexMap<String, f64>>`) where the outer map tracks
//! symbol names and each inner map tracks co-access weights to other symbols.
//! This is symmetric by convention — `record_co_access(a, b)` writes both
//! directions, but the structure itself does not enforce symmetry.

use indexmap::IndexMap;

// ---------------------------------------------------------------------------
// IA-2: SymbolPheromoneMap — co-access pheromone for symbol reranking
// ---------------------------------------------------------------------------

/// IA-2: Tracks co-access patterns between symbols as pheromone trails.
///
/// When two symbols are accessed together (e.g., in the same query or edit
/// session), their co-access strength increases. This pheromone signal can
/// be used to rerank symbol search results — symbols frequently co-accessed
/// with the current context rank higher.
///
/// Pheromone is symmetric: `record_co_access(a, b)` = `record_co_access(b, a)`.
/// Evaporation decays all strengths each tick; values below `prune_threshold`
/// are removed to keep memory bounded.
pub struct SymbolPheromoneMap {
    /// co_access[a][b] = pheromone strength between symbols a and b.
    /// Both outer and inner maps preserve insertion order.
    co_access: IndexMap<String, IndexMap<String, f64>>,
    /// Fraction of pheromone lost per `evaporate()` call.
    evaporation_rate: f64,
    /// Maximum pheromone strength per pair (prevents runaway accumulation).
    cap: f64,
    /// Pairs below this strength are pruned on evaporation.
    prune_threshold: f64,
}

impl SymbolPheromoneMap {
    /// Create a new map with the given evaporation rate.
    ///
    /// * `evaporation_rate` — fraction lost per tick (0.0 = no decay, 1.0 = full reset).
    #[must_use]
    pub fn new(evaporation_rate: f64) -> Self {
        Self {
            co_access: IndexMap::new(),
            evaporation_rate: evaporation_rate.clamp(0.0, 1.0),
            cap: 10.0,
            prune_threshold: 0.001,
        }
    }

    /// Record a co-access event between two symbols.
    ///
    /// Increments the pheromone strength for both `(sym_a, sym_b)` and
    /// `(sym_b, sym_a)` by 1.0, capped at `self.cap`.
    pub fn record_co_access(&mut self, sym_a: &str, sym_b: &str) {
        if sym_a == sym_b {
            return; // self-loops are meaningless
        }
        let cap = self.cap;
        let update = |map: &mut IndexMap<String, IndexMap<String, f64>>, from: &str, to: &str| {
            let entry = map.entry(from.to_string()).or_default();
            let new_weight = entry.get(to).copied().unwrap_or(0.0) + 1.0;
            entry.insert(to.to_string(), new_weight.min(cap));
        };
        update(&mut self.co_access, sym_a, sym_b);
        update(&mut self.co_access, sym_b, sym_a);
    }

    /// Return the pheromone strength between two symbols (0.0 if unknown).
    #[must_use]
    pub fn pheromone_strength(&self, sym_a: &str, sym_b: &str) -> f64 {
        self.co_access
            .get(sym_a)
            .and_then(|m| m.get(sym_b))
            .copied()
            .unwrap_or(0.0)
    }

    /// Apply exponential decay to all pairs and prune weak ones.
    ///
    /// Complexity: O(E) where E is total number of pheromone entries.
    pub fn evaporate(&mut self) {
        let rate = self.evaporation_rate;
        let threshold = self.prune_threshold;
        for inner in self.co_access.values_mut() {
            inner.values_mut().for_each(|v| *v *= 1.0 - rate);
            inner.retain(|_, v| *v >= threshold);
        }
        self.co_access.retain(|_, inner| !inner.is_empty());
    }

    /// Rerank a list of candidate symbols by co-access strength with `context_sym`.
    ///
    /// Returns the candidates sorted descending by pheromone strength.
    #[must_use]
    pub fn rerank(&self, context_sym: &str, candidates: &[String]) -> Vec<String> {
        let mut ranked: Vec<(f64, &String)> = candidates
            .iter()
            .map(|c| (self.pheromone_strength(context_sym, c), c))
            .collect();
        ranked.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        ranked.into_iter().map(|(_, c)| c.clone()).collect()
    }

    /// Number of symbols tracked in the map.
    #[must_use]
    pub fn symbol_count(&self) -> usize {
        self.co_access.len()
    }

    /// Evaporates pheromone and returns the KS statistic measuring how much
    /// the pheromone distribution shifted. Returns `None` if fewer than 2
    /// pheromone values exist (insufficient data for a meaningful test).
    #[cfg(feature = "simd-search")]
    #[must_use]
    pub fn evaporate_with_drift_check(&mut self) -> Option<f64> {
        use touring_simd::{DriftDetection, DriftDetector};

        // Snapshot distribution BEFORE evaporation.
        let before: Vec<f64> = self
            .co_access
            .values()
            .flat_map(|m| m.values().copied())
            .collect();

        if before.len() < 2 {
            self.evaporate();
            return None;
        }

        self.evaporate();

        // Snapshot distribution AFTER evaporation.
        let after: Vec<f64> = self
            .co_access
            .values()
            .flat_map(|m| m.values().copied())
            .collect();

        if after.len() < 2 {
            // Pruning removed most entries — maximum drift.
            return Some(1.0);
        }

        let detector = DriftDetector::new();
        Some(detector.ks_statistic(&before, &after))
    }
}

// ---------------------------------------------------------------------------
// IA-4: PheromoneGraph — pheromone trails on dependency edges
// ---------------------------------------------------------------------------

/// IA-4: ACO-style pheromone map over file-dependency edges.
///
/// Records how often each (from_file → to_file) dependency edge is traversed
/// during analysis, allowing hot paths in the import graph to be surfaced.
/// Evaporation prevents stale edges from dominating; `hot_edges` returns the
/// top-k strongest trails for prioritised re-analysis.
pub struct PheromoneGraph {
    /// Pheromone strength per directed edge (from, to).
    /// IndexMap preserves insertion order for deterministic iteration.
    edge_pheromone: IndexMap<(String, String), f64>,
    /// Fraction of pheromone that evaporates each tick (0.0–1.0).
    evaporation_rate: f64,
    /// Minimum strength below which an edge is pruned.
    prune_threshold: f64,
}

impl PheromoneGraph {
    /// Create a new `PheromoneGraph` with the given evaporation rate.
    ///
    /// `evaporation_rate` should be in `[0.0, 1.0)`.  A value of `0.1` means
    /// 10% of each edge's pheromone evaporates per tick.
    #[must_use]
    pub fn new(evaporation_rate: f64) -> Self {
        Self {
            edge_pheromone: IndexMap::new(),
            evaporation_rate,
            prune_threshold: 0.001,
        }
    }

    /// Deposit one unit of pheromone on every consecutive edge in `path`.
    ///
    /// `path` is a slice of file paths forming a traversal chain.
    /// Each consecutive pair `(path[i], path[i+1])` receives `+1.0`.
    /// Self-edges (same source and target) are silently ignored.
    pub fn reinforce_path(&mut self, path: &[&str]) {
        for window in path.windows(2) {
            let [from, to] = window else { continue };
            if from == to {
                continue;
            }
            let key = (from.to_string(), to.to_string());
            let new_weight = self.edge_pheromone.get(&key).copied().unwrap_or(0.0) + 1.0;
            self.edge_pheromone.insert(key, new_weight);
        }
    }

    /// Pheromone strength of the directed edge `from → to` (0.0 if unknown).
    #[must_use]
    pub fn edge_strength(&self, from: &str, to: &str) -> f64 {
        self.edge_pheromone
            .get(&(from.to_string(), to.to_string()))
            .copied()
            .unwrap_or(0.0)
    }

    /// Apply exponential decay to all edges and prune weak ones.
    ///
    /// Complexity: O(E) where E is the number of directed edges.
    pub fn evaporate(&mut self) {
        let rate = self.evaporation_rate;
        let threshold = self.prune_threshold;
        self.edge_pheromone
            .values_mut()
            .for_each(|v| *v *= 1.0 - rate);
        self.edge_pheromone.retain(|_, v| *v >= threshold);
    }

    /// Return the top-`k` strongest edges as `(strength, from, to)` sorted
    /// by strength descending.  Returns fewer than `k` if fewer edges exist.
    #[must_use]
    pub fn hot_edges(&self, k: usize) -> Vec<(f64, &str, &str)> {
        let mut edges: Vec<(f64, &str, &str)> = self
            .edge_pheromone
            .iter()
            .map(|((from, to), &v)| (v, from.as_str(), to.as_str()))
            .collect();
        edges.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        edges.truncate(k);
        edges
    }

    /// Total number of active (non-pruned) edges.
    #[must_use]
    pub fn edge_count(&self) -> usize {
        self.edge_pheromone.len()
    }

    /// Evaporates pheromone on all edges and returns the KS statistic measuring
    /// how much the edge-pheromone distribution shifted. Returns `None` if fewer
    /// than 2 edges exist (insufficient data for a meaningful test).
    #[cfg(feature = "simd-search")]
    #[must_use]
    pub fn evaporate_with_drift_check(&mut self) -> Option<f64> {
        use touring_simd::{DriftDetection, DriftDetector};

        // Snapshot distribution BEFORE evaporation.
        let before: Vec<f64> = self.edge_pheromone.values().copied().collect();

        if before.len() < 2 {
            self.evaporate();
            return None;
        }

        self.evaporate();

        // Snapshot distribution AFTER evaporation.
        let after: Vec<f64> = self.edge_pheromone.values().copied().collect();

        if after.len() < 2 {
            // Pruning removed most entries — maximum drift.
            return Some(1.0);
        }

        let detector = DriftDetector::new();
        Some(detector.ks_statistic(&before, &after))
    }
}

#[cfg(test)]
mod pheromone_graph_tests {
    use super::PheromoneGraph;

    #[test]
    fn test_reinforce_single_path() {
        let mut g = PheromoneGraph::new(0.0);
        g.reinforce_path(&["a.rs", "b.rs", "c.rs"]);
        assert!((g.edge_strength("a.rs", "b.rs") - 1.0).abs() < 1e-9);
        assert!((g.edge_strength("b.rs", "c.rs") - 1.0).abs() < 1e-9);
        assert_eq!(g.edge_strength("a.rs", "c.rs"), 0.0); // non-consecutive
    }

    #[test]
    fn test_reinforce_accumulates() {
        let mut g = PheromoneGraph::new(0.0);
        for _ in 0..5 {
            g.reinforce_path(&["x.rs", "y.rs"]);
        }
        assert!((g.edge_strength("x.rs", "y.rs") - 5.0).abs() < 1e-9);
    }

    #[test]
    fn test_self_edge_ignored() {
        let mut g = PheromoneGraph::new(0.0);
        g.reinforce_path(&["a.rs", "a.rs"]);
        assert_eq!(g.edge_count(), 0);
        assert_eq!(g.edge_strength("a.rs", "a.rs"), 0.0);
    }

    #[test]
    fn test_evaporate_decays() {
        let mut g = PheromoneGraph::new(0.5);
        g.reinforce_path(&["a.rs", "b.rs"]);
        assert!((g.edge_strength("a.rs", "b.rs") - 1.0).abs() < 1e-9);
        g.evaporate();
        assert!((g.edge_strength("a.rs", "b.rs") - 0.5).abs() < 1e-9);
    }

    #[test]
    fn test_evaporate_prunes_weak() {
        let mut g = PheromoneGraph::new(0.9999);
        g.reinforce_path(&["p.rs", "q.rs"]);
        g.evaporate();
        assert_eq!(g.edge_count(), 0);
    }

    #[test]
    fn test_hot_edges_sorted() {
        let mut g = PheromoneGraph::new(0.0);
        g.reinforce_path(&["a.rs", "b.rs"]);
        g.reinforce_path(&["a.rs", "b.rs"]); // strength = 2
        g.reinforce_path(&["c.rs", "d.rs"]); // strength = 1
        let hot = g.hot_edges(2);
        assert_eq!(hot.len(), 2);
        assert!((hot[0].0 - 2.0).abs() < 1e-9, "hottest should be 2.0");
        assert!((hot[1].0 - 1.0).abs() < 1e-9, "second should be 1.0");
    }

    #[test]
    fn test_hot_edges_k_larger_than_count() {
        let mut g = PheromoneGraph::new(0.0);
        g.reinforce_path(&["a.rs", "b.rs"]);
        let hot = g.hot_edges(100);
        assert_eq!(hot.len(), 1);
    }
}

#[cfg(test)]
mod symbol_pheromone_tests {
    use super::SymbolPheromoneMap;

    #[test]
    fn test_record_co_access_symmetric() {
        let mut map = SymbolPheromoneMap::new(0.0);
        map.record_co_access("foo", "bar");
        assert_eq!(map.pheromone_strength("foo", "bar"), 1.0);
        assert_eq!(map.pheromone_strength("bar", "foo"), 1.0);
    }

    #[test]
    fn test_record_co_access_caps_at_max() {
        let mut map = SymbolPheromoneMap::new(0.0);
        for _ in 0..20 {
            map.record_co_access("a", "b");
        }
        assert_eq!(map.pheromone_strength("a", "b"), 10.0);
    }

    #[test]
    fn test_self_loop_ignored() {
        let mut map = SymbolPheromoneMap::new(0.0);
        map.record_co_access("a", "a");
        assert_eq!(map.pheromone_strength("a", "a"), 0.0);
        assert_eq!(map.symbol_count(), 0);
    }

    #[test]
    fn test_evaporate_decays() {
        let mut map = SymbolPheromoneMap::new(0.5);
        map.record_co_access("x", "y");
        assert_eq!(map.pheromone_strength("x", "y"), 1.0);
        map.evaporate();
        assert!((map.pheromone_strength("x", "y") - 0.5).abs() < 1e-9);
    }

    #[test]
    fn test_evaporate_prunes_weak_trails() {
        let mut map = SymbolPheromoneMap::new(0.9999);
        map.record_co_access("p", "q");
        map.evaporate();
        // After one tick at 99.99% rate, value < 0.001 → pruned.
        assert_eq!(map.symbol_count(), 0);
    }

    #[test]
    fn test_rerank_orders_by_strength() {
        let mut map = SymbolPheromoneMap::new(0.0);
        map.record_co_access("ctx", "hot");
        map.record_co_access("ctx", "hot");
        map.record_co_access("ctx", "cold");

        let candidates = vec!["cold".to_string(), "hot".to_string()];
        let ranked = map.rerank("ctx", &candidates);
        assert_eq!(ranked[0], "hot");
        assert_eq!(ranked[1], "cold");
    }
}
