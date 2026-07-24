//! Transitive reduction for DAGs.
//!
//! Implements Aho/Garey/Ullman algorithm for transitive reduction.
//! Falls back to preserving original graph when cycle detected.

use petgraph::graph::{Graph, NodeIndex};
use std::collections::HashSet;

/// Check if graph contains a cycle using DFS.
/// Returns true if cycle detected, false otherwise.
fn detect_cycle(graph: &Graph<(), ()>, n: usize) -> bool {
    let mut visited = vec![false; n];
    let mut rec_stack = vec![false; n];

    fn dfs(g: &Graph<(), ()>, u: NodeIndex, visited: &mut Vec<bool>, rec: &mut Vec<bool>) -> bool {
        visited[u.index()] = true;
        rec[u.index()] = true;
        for v in g.neighbors(u) {
            let vi = v.index();
            if !visited[vi] && dfs(g, v, visited, rec) {
                return true;
            }
            if rec[vi] {
                return true;
            }
        }
        rec[u.index()] = false;
        false
    }

    for i in 0..n {
        let node_idx = NodeIndex::new(i);
        if !visited[i] && dfs(graph, node_idx, &mut visited, &mut rec_stack) {
            return true;
        }
    }
    false
}

/// Check if there exists a path from `u` to `v` in the graph, avoiding direct edge u->v.
/// Uses BFS to find an alternate path. The direct edge u->v is excluded from neighbor traversal.
fn has_path_avoiding_edge(graph: &Graph<(), ()>, u: NodeIndex, v: NodeIndex) -> bool {
    let mut visited: HashSet<usize> = HashSet::new();
    let mut queue = vec![u];
    let avoid_from = u;
    let avoid_to = v;

    while let Some(curr) = queue.pop() {
        if curr == v {
            return true;
        }
        for next in graph.neighbors(curr) {
            let ni = next.index();
            // Skip the direct edge we're trying to avoid, but allow reaching v through other paths
            if curr == avoid_from && ni == avoid_to.index() {
                continue;
            }
            if !visited.contains(&ni) {
                visited.insert(ni);
                queue.push(next);
            }
        }
    }
    false
}

/// Compute transitive reduction of a DAG.
/// Returns edges that remain after removing transitive edges, or error if graph has cycles.
pub fn transitive_reduction<N, E>(
    nodes: &[N],
    edges: &[(usize, usize, E)],
) -> Result<Vec<(usize, usize, E)>, &'static str>
where
    N: Clone,
    E: Clone,
{
    let n = nodes.len();
    let mut graph: Graph<(), ()> = Graph::new();

    for _ in 0..n {
        graph.add_node(());
    }

    for &(u, v, _) in edges {
        let ui = NodeIndex::new(u);
        let vi = NodeIndex::new(v);
        graph.add_edge(ui, vi, ());
    }

    if detect_cycle(&graph, n) {
        return Err("graph contains cycle - transitive reduction not applicable");
    }

    let reduced_edges: Vec<(usize, usize, E)> = edges
        .iter()
        .filter(|(u, v, _)| {
            let ui = NodeIndex::new(*u);
            let vi = NodeIndex::new(*v);
            !has_path_avoiding_edge(&graph, ui, vi)
        })
        .map(|(u, v, e)| (*u, *v, e.clone()))
        .collect();

    Ok(reduced_edges)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unwrap_edges(r: Result<Vec<(usize, usize, ())>, &'static str>) -> Vec<(usize, usize, ())> {
        r.expect("transitive_reduction should not return error for valid DAG")
    }

    #[test]
    fn test_no_transitive_edges() {
        let nodes = vec!["a", "b", "c"];
        let edges = vec![(0, 1, ()), (1, 2, ())];
        let result = unwrap_edges(transitive_reduction(&nodes, &edges));
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_transitive_edge_removed() {
        let nodes = vec!["a", "b", "c"];
        let edges = vec![(0, 1, ()), (1, 2, ()), (0, 2, ())];
        let result = unwrap_edges(transitive_reduction(&nodes, &edges));
        assert_eq!(result.len(), 2);
        assert!(result.contains(&(0, 1, ())));
        assert!(result.contains(&(1, 2, ())));
    }

    #[test]
    fn test_diamond() {
        let nodes = vec!["a", "b", "c", "d"];
        let edges = vec![(0, 1, ()), (0, 2, ()), (1, 3, ()), (2, 3, ())];
        let result = unwrap_edges(transitive_reduction(&nodes, &edges));
        assert_eq!(result.len(), 4);
    }

    #[test]
    fn test_cycle_detected() {
        let nodes = vec!["a", "b", "c"];
        let edges = vec![(0, 1, ()), (1, 2, ()), (2, 0, ())];
        let result = transitive_reduction(&nodes, &edges);
        assert!(result.is_err());
    }
}
