//! Acyclicity score via Tarjan's strongly-connected-components algorithm.
//!
//! Computes the count of strongly-connected components with size > 1 in the
//! directed dependency graph. A workspace with zero such SCCs has score 1.0;
//! the score decays sigmoidally as cycles accumulate.
//!
//! # Reuse
//!
//! Internally builds a `petgraph::graphmap::DiGraphMap` and calls
//! [`petgraph::algo::tarjan_scc`]. Touring already uses `tarjan_scc` in
//! `crates/touring-hooks/src/dependency_cache.rs:31,194,209`; this module
//! exposes the same primitive over the abstract `Workspace` graph.

use petgraph::algo::tarjan_scc;
use petgraph::graphmap::DiGraphMap;

use super::types::Workspace;

/// Number of cycles in `ws`'s dependency graph.
///
/// A "cycle" here is a strongly-connected component containing more than one
/// node. Self-loops (a single node with an edge to itself) are also counted
/// because they make the build order undefined.
#[must_use]
pub fn compute_cycle_count(ws: &Workspace) -> usize {
    let graph = build_petgraph(ws);
    tarjan_scc(&graph)
        .into_iter()
        .filter(|scc| scc.len() > 1 || has_self_loop(&graph, scc))
        .count()
}

/// Acyclicity score: `1 / (1 + cycle_count)` (sigmoid, midpoint=1).
///
/// * 0 cycles → score 1.0
/// * 1 cycle  → score 0.5
/// * 4 cycles → score 0.2
#[must_use]
pub fn acyclicity_score(cycle_count: usize) -> f64 {
    1.0 / (1.0 + cycle_count as f64)
}

/// Cycle paths (each path is a list of node names that form one SCC).
///
/// Returned in the order Tarjan's algorithm produces them. Useful for the
/// `Diagnostics::acyclicity.cycle_paths` field.
#[must_use]
pub fn collect_cycle_paths(ws: &Workspace) -> Vec<Vec<String>> {
    let graph = build_petgraph(ws);
    tarjan_scc(&graph)
        .into_iter()
        .filter(|scc| scc.len() > 1 || has_self_loop(&graph, scc))
        .map(|scc| scc.iter().map(|n| (*n).to_string()).collect())
        .collect()
}

fn build_petgraph(ws: &Workspace) -> DiGraphMap<&str, ()> {
    let mut g: DiGraphMap<&str, ()> = DiGraphMap::new();
    for (a, b) in &ws.edges {
        g.add_edge(a.as_str(), b.as_str(), ());
    }
    g
}

fn has_self_loop(graph: &DiGraphMap<&str, ()>, scc: &[&str]) -> bool {
    if scc.len() != 1 {
        return false;
    }
    let n = scc[0];
    graph.contains_edge(n, n)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ws_from(edges: &[(&str, &str)]) -> Workspace {
        let mut ws = Workspace::empty("/tmp/test");
        ws.edges = edges
            .iter()
            .map(|(a, b)| ((*a).to_string(), (*b).to_string()))
            .collect();
        ws
    }

    #[test]
    fn empty_graph_zero_cycles() {
        let ws = Workspace::empty("/tmp");
        assert_eq!(compute_cycle_count(&ws), 0);
        assert!((acyclicity_score(0) - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn dag_zero_cycles() {
        let ws = ws_from(&[("a", "b"), ("b", "c"), ("a", "c")]);
        assert_eq!(compute_cycle_count(&ws), 0);
    }

    #[test]
    fn two_node_cycle_detected() {
        let ws = ws_from(&[("a", "b"), ("b", "a")]);
        assert_eq!(compute_cycle_count(&ws), 1);
        assert!((acyclicity_score(1) - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn three_node_cycle_detected() {
        let ws = ws_from(&[("a", "b"), ("b", "c"), ("c", "a")]);
        assert_eq!(compute_cycle_count(&ws), 1);
    }

    #[test]
    fn self_loop_counts_as_cycle() {
        let ws = ws_from(&[("a", "a"), ("a", "b")]);
        assert_eq!(compute_cycle_count(&ws), 1);
    }

    #[test]
    fn multiple_disjoint_cycles_counted() {
        let ws = ws_from(&[("a", "b"), ("b", "a"), ("c", "d"), ("d", "c"), ("e", "f")]);
        assert_eq!(compute_cycle_count(&ws), 2);
        assert!((acyclicity_score(2) - 1.0 / 3.0).abs() < 1e-9);
    }

    #[test]
    fn collect_cycle_paths_returns_node_lists() {
        let ws = ws_from(&[("a", "b"), ("b", "a"), ("c", "d"), ("d", "c")]);
        let paths = collect_cycle_paths(&ws);
        assert_eq!(paths.len(), 2);
        for p in &paths {
            assert_eq!(p.len(), 2);
        }
    }
}
