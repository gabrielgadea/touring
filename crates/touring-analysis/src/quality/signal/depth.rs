//! Lakos depth — longest path in the (acyclic) dependency graph.
//!
//! For acyclic graphs we compute the longest path via memoised DFS in
//! `O(V + E)`. For graphs with cycles the longest path problem is NP-hard
//! in general; we degrade gracefully by skipping back-edges (cycle members)
//! and reporting the longest acyclic chain we find. This matches the
//! Sentrux convention (`max_depth` is meaningful only on the DAG part of
//! the graph; cycles are accounted for separately by `acyclicity`).

use std::collections::{HashMap, HashSet};

use super::types::Workspace;

/// Maximum depth (longest path length in edges) in the workspace graph.
///
/// Returns `0` for empty graphs and for graphs with no edges. Returns the
/// longest acyclic path length when cycles exist (skipping back-edges).
#[must_use]
pub fn compute_max_depth(ws: &Workspace) -> u32 {
    let adj = build_adjacency(ws);
    if adj.is_empty() {
        return 0;
    }

    let mut memo: HashMap<&str, u32> = HashMap::new();
    let mut visiting: HashSet<&str> = HashSet::new();
    let mut best: u32 = 0;
    for &node in adj.keys() {
        let depth = dfs_longest(node, &adj, &mut memo, &mut visiting);
        if depth > best {
            best = depth;
        }
    }
    best
}

/// Depth score: sigmoid `1 / (1 + max_depth / 8)`. Midpoint = 8 levels.
///
/// * depth 0 → score 1.0
/// * depth 8 → score 0.5
/// * depth 16 → score 0.333
#[must_use]
pub fn depth_score(max_depth: u32) -> f64 {
    1.0 / (1.0 + (max_depth as f64) / 8.0)
}

/// Witness path that produced the longest depth (file paths in order).
///
/// Recomputed independently from `compute_max_depth` to avoid threading
/// path tracking through the memoised DFS hot loop. For workspaces of
/// realistic size (< 50k nodes) the extra pass is negligible.
#[must_use]
pub fn longest_path_witness(ws: &Workspace) -> Vec<String> {
    let adj = build_adjacency(ws);
    if adj.is_empty() {
        return Vec::new();
    }

    let mut memo_path: HashMap<&str, Vec<&str>> = HashMap::new();
    let mut visiting: HashSet<&str> = HashSet::new();
    let mut best: Vec<&str> = Vec::new();
    for &node in adj.keys() {
        let path = dfs_longest_path(node, &adj, &mut memo_path, &mut visiting);
        if path.len() > best.len() {
            best = path;
        }
    }
    best.into_iter().map(String::from).collect()
}

fn build_adjacency(ws: &Workspace) -> HashMap<&str, Vec<&str>> {
    let mut adj: HashMap<&str, Vec<&str>> = HashMap::new();
    for (a, b) in &ws.edges {
        adj.entry(a.as_str()).or_default().push(b.as_str());
        adj.entry(b.as_str()).or_default();
    }
    adj
}

fn dfs_longest<'a>(
    node: &'a str,
    adj: &'a HashMap<&'a str, Vec<&'a str>>,
    memo: &mut HashMap<&'a str, u32>,
    visiting: &mut HashSet<&'a str>,
) -> u32 {
    if let Some(&v) = memo.get(node) {
        return v;
    }
    if visiting.contains(node) {
        // back-edge: skip to break cycle without panicking
        return 0;
    }
    visiting.insert(node);
    let mut best = 0u32;
    if let Some(children) = adj.get(node) {
        for child in children {
            let d = dfs_longest(child, adj, memo, visiting).saturating_add(1);
            if d > best {
                best = d;
            }
        }
    }
    visiting.remove(node);
    memo.insert(node, best);
    best
}

fn dfs_longest_path<'a>(
    node: &'a str,
    adj: &'a HashMap<&'a str, Vec<&'a str>>,
    memo: &mut HashMap<&'a str, Vec<&'a str>>,
    visiting: &mut HashSet<&'a str>,
) -> Vec<&'a str> {
    if let Some(v) = memo.get(node) {
        return v.clone();
    }
    if visiting.contains(node) {
        return Vec::new();
    }
    visiting.insert(node);
    let mut best: Vec<&'a str> = Vec::new();
    if let Some(children) = adj.get(node) {
        for child in children {
            let mut p = dfs_longest_path(child, adj, memo, visiting);
            if p.len() > best.len() {
                p.insert(0, node);
                best = p;
            }
        }
    }
    if best.is_empty() {
        best.push(node);
    }
    visiting.remove(node);
    memo.insert(node, best.clone());
    best
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
    fn empty_graph_zero_depth() {
        let ws = Workspace::empty("/tmp");
        assert_eq!(compute_max_depth(&ws), 0);
        assert!((depth_score(0) - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn linear_chain_depth_equals_edges() {
        let ws = ws_from(&[("a", "b"), ("b", "c"), ("c", "d"), ("d", "e")]);
        assert_eq!(compute_max_depth(&ws), 4);
    }

    #[test]
    fn diamond_depth_two() {
        let ws = ws_from(&[("a", "b"), ("a", "c"), ("b", "d"), ("c", "d")]);
        assert_eq!(compute_max_depth(&ws), 2);
    }

    #[test]
    fn depth_score_midpoint_eight() {
        let s = depth_score(8);
        assert!((s - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn cycle_does_not_panic() {
        let ws = ws_from(&[("a", "b"), ("b", "a")]);
        // any finite answer is acceptable — we just don't want to recurse forever
        let _ = compute_max_depth(&ws);
    }

    #[test]
    fn longest_path_witness_contains_endpoints() {
        let ws = ws_from(&[("a", "b"), ("b", "c"), ("c", "d")]);
        let path = longest_path_witness(&ws);
        assert!(path.first().map(String::as_str) == Some("a"));
        assert!(path.last().map(String::as_str) == Some("d"));
        assert_eq!(path.len(), 4);
    }
}
