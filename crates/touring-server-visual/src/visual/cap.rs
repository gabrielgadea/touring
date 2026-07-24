//! Node/edge capping and relevance-based truncation.
//!
//! Provides `cap_graph()` which limits nodes and edges using BFS from
//! root nodes, preserving graph structure up to specified limits.

use std::collections::{HashMap, HashSet, VecDeque};

/// Cap the graph to max_nodes using BFS from root nodes with relevance scoring.
/// Returns (capped_node_indices, capped_edges, truncated_node_count)
pub fn cap_graph<N, E>(
    nodes: &[N],
    edges: &[(usize, usize, E)],
    max_nodes: usize,
    max_edges: usize,
) -> (Vec<usize>, Vec<(usize, usize, E)>, usize)
where
    N: Clone,
    E: Clone,
{
    if nodes.len() <= max_nodes && edges.len() <= max_edges {
        return (
            nodes.iter().enumerate().map(|(i, _)| i).collect(),
            edges.to_vec(),
            0,
        );
    }

    // Compute in-degree for each node (roots have indegree 0)
    let mut in_degree: HashMap<usize, usize> = HashMap::new();
    for &(_from, to, _) in edges {
        *in_degree.entry(to).or_insert(0) += 1;
    }

    // BFS from roots (nodes with indegree 0)
    let mut queue = VecDeque::new();
    let mut visited: HashSet<usize> = HashSet::new();

    for (i, _) in nodes.iter().enumerate() {
        if !in_degree.contains_key(&i) {
            queue.push_back(i);
            visited.insert(i);
        }
    }

    // BFS collecting reachable nodes up to max_nodes
    let mut selected_nodes = Vec::new();
    while let Some(node) = queue.pop_front() {
        if selected_nodes.len() >= max_nodes {
            break;
        }
        selected_nodes.push(node);
        for &(from, to, _) in edges {
            if from == node && !visited.contains(&to) {
                visited.insert(to);
                queue.push_back(to);
            }
        }
    }

    let truncated = nodes.len() - selected_nodes.len();
    let selected_set: HashSet<usize> = selected_nodes.iter().cloned().collect();
    let selected_edges: Vec<(usize, usize, E)> = edges
        .iter()
        .filter(|(from, to, _)| selected_set.contains(from) && selected_set.contains(to))
        .take(max_edges)
        .cloned()
        .collect();

    (selected_nodes, selected_edges, truncated)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cap_graph_no_op_when_small() {
        let nodes = vec!["a", "b", "c"];
        let edges = vec![(0, 1, ()), (1, 2, ())];
        let (caps, caps_edges, trunc) = cap_graph(&nodes, &edges, 10, 10);
        assert_eq!(caps.len(), 3);
        assert_eq!(caps_edges.len(), 2);
        assert_eq!(trunc, 0);
    }

    #[test]
    fn test_cap_graph_limits_nodes() {
        let nodes = vec!["a", "b", "c", "d", "e"];
        let edges = vec![(0, 1, ()), (1, 2, ()), (2, 3, ()), (3, 4, ())];
        let (caps, _, trunc) = cap_graph(&nodes, &edges, 3, 10);
        assert_eq!(caps.len(), 3);
        assert_eq!(trunc, 2);
    }

    #[test]
    fn test_cap_graph_limits_edges() {
        let nodes = vec!["a", "b", "c"];
        let edges = vec![(0, 1, ()), (1, 2, ()), (0, 2, ())];
        let (_, caps_edges, _) = cap_graph(&nodes, &edges, 10, 2);
        assert_eq!(caps_edges.len(), 2);
    }

    #[test]
    fn test_cap_graph_empty_roots() {
        // Isolated nodes are treated as roots (indegree 0)
        let nodes = vec!["a", "b", "c"];
        let edges: Vec<(usize, usize, ())> = vec![];
        let (caps, caps_edges, trunc) = cap_graph(&nodes, &edges[..], 2, 10);
        assert_eq!(caps.len(), 2);
        assert_eq!(caps_edges.len(), 0);
        assert_eq!(trunc, 1);
    }
}
