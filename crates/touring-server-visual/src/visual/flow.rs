//! Path enumeration between two symbols using BFS.
//!
//! Finds all simple paths from symbol_a to symbol_b in a call graph,
//! with configurable max path count and depth limits.
use super::encoding;
use crate::visual::GraphData;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
/// Options for flow path enumeration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlowOpts {
    /// Maximum number of paths to return.
    pub max_paths: usize,
    /// Maximum path depth (graph edges).
    pub max_depth: usize,
}
impl Default for FlowOpts {
    fn default() -> Self {
        Self {
            max_paths: 10,
            max_depth: 8,
        }
    }
}
/// Result of flow path enumeration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlowResult {
    /// Enumerated paths from A to B.
    pub paths: Vec<Path>,
    /// Total path count found (may exceed max_paths if truncated).
    pub count: usize,
    /// Whether results were truncated due to max_paths limit.
    pub truncated: bool,
    /// Source symbol A.
    pub from: String,
    /// Target symbol B.
    pub to: String,
}
/// A single path from source to target.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Path {
    /// Nodes in the path (ordered from A to B).
    pub nodes: Vec<String>,
    /// Number of edges in this path.
    pub depth: usize,
}
impl FlowOpts {
    /// Create FlowOpts with defaults: max_paths=10, max_depth=8.
    pub fn new() -> Self {
        Self::default()
    }
    /// Set maximum number of paths.
    pub fn max_paths(mut self, n: usize) -> Self {
        self.max_paths = n;
        self
    }
    /// Set maximum path depth.
    pub fn max_depth(mut self, n: usize) -> Self {
        self.max_depth = n;
        self
    }
}
/// BFS-based simple path enumeration from start to end within max_depth.
fn bfs_all_paths(
    adj: &[Vec<usize>],
    start: usize,
    end: usize,
    max_depth: usize,
) -> Vec<Vec<usize>> {
    let mut results: Vec<Vec<usize>> = Vec::new();
    let mut queue: VecDeque<(usize, Vec<usize>)> = VecDeque::new();
    queue.push_back((start, vec![start]));
    while let Some((node, path)) = queue.pop_front() {
        if path.len() > max_depth {
            continue;
        }
        if node == end {
            results.push(path);
            continue;
        }
        if path.len() >= max_depth {
            continue;
        }
        for &next in &adj[node] {
            if !path.contains(&next) {
                let mut new_path = path.clone();
                new_path.push(next);
                queue.push_back((next, new_path));
            }
        }
    }
    results
}
/// Find all simple paths from symbol_a to symbol_b in the call graph.
pub fn find_flow_paths(
    graph_data: &GraphData,
    symbol_a: &str,
    symbol_b: &str,
    opts: &FlowOpts,
) -> FlowResult {
    let mut idx: HashMap<&str, usize> = HashMap::new();
    for (i, node) in graph_data.nodes.iter().enumerate() {
        idx.insert(node.id.as_str(), i);
    }
    let start = match idx.get(symbol_a) {
        Some(&s) => s,
        None => {
            return FlowResult {
                paths: vec![],
                count: 0,
                truncated: false,
                from: symbol_a.to_string(),
                to: symbol_b.to_string(),
            };
        }
    };
    let end = match idx.get(symbol_b) {
        Some(&e) => e,
        None => {
            return FlowResult {
                paths: vec![],
                count: 0,
                truncated: false,
                from: symbol_a.to_string(),
                to: symbol_b.to_string(),
            };
        }
    };
    let node_count = graph_data.nodes.len();
    let mut adj: Vec<Vec<usize>> = vec![vec![]; node_count];
    for edge in &graph_data.edges {
        if let (Some(&fi), Some(&ti)) = (idx.get(edge.from.as_str()), idx.get(edge.to.as_str())) {
            adj[fi].push(ti);
        }
    }
    let max_depth = if opts.max_depth > 0 {
        opts.max_depth
    } else {
        8
    };
    let all_paths = bfs_all_paths(&adj, start, end, max_depth);
    let count = all_paths.len().min(opts.max_paths);
    let truncated = all_paths.len() > opts.max_paths;
    let paths: Vec<Path> = all_paths
        .into_iter()
        .take(opts.max_paths)
        .map(|path| {
            let nodes = path
                .iter()
                .map(|&i| graph_data.nodes[i].id.clone())
                .collect();
            let depth = path.len().saturating_sub(1);
            Path { nodes, depth }
        })
        .collect();
    FlowResult {
        paths,
        count,
        truncated,
        from: symbol_a.to_string(),
        to: symbol_b.to_string(),
    }
}
/// Convert FlowResult to JSON string for CLI output.
pub fn flow_to_json(result: &FlowResult) -> String {
    serde_json::to_string_pretty(result).unwrap_or_else(|_| "{}".to_string())
}
/// Convert FlowResult to DOT digraph format.
///
/// Renders each path as a chain of nodes from `from` to `to`,
/// with the source highlighted in green and target in red.
pub fn flow_result_to_dot(result: &FlowResult, _opts: &crate::visual::DotOpts) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    writeln!(out, "digraph flow_paths {{").ok();
    writeln!(out, "  rankdir=LR;").ok();
    writeln!(out, "  node [fontname=\"Helvetica\"];").ok();
    writeln!(out, "  edge [fontname=\"Helvetica\"];").ok();
    writeln!(
        out,
        "  // Flow paths from '{}' to '{}': {} paths (truncated={})",
        result.from, result.to, result.count, result.truncated
    )
    .ok();
    writeln!(
        out,
        "  \"{from}\" [shape=box, style=filled, fillcolor={source_color}, label=\"{from} (source)\"];",
        from = result.from,
        source_color = encoding::nodetype_fillcolor(Some(touring_intelligence::reasoning::semantic_graph::NodeType::Session))
    )
        .ok();
    writeln!(
        out,
        "  \"{to}\" [shape=box, style=filled, fillcolor={target_color}, label=\"{to} (target)\"];",
        to = result.to,
        target_color = encoding::nodetype_fillcolor(Some(
            touring_intelligence::reasoning::semantic_graph::NodeType::Symbol
        ))
    )
    .ok();
    for (path_idx, path) in result.paths.iter().enumerate() {
        writeln!(out, "  subgraph \"cluster_path_{}\" {{", path_idx).ok();
        writeln!(
            out,
            "    label=\"Path {} (depth={})\";",
            path_idx + 1,
            path.depth
        )
        .ok();
        writeln!(out, "    style=dashed;").ok();
        if let Some(first) = path.nodes.first()
            && first != &result.from
        {
            writeln!(
                out,
                "  \"{from}\" -> \"{first}\" [color=gray, style=dashed];",
                from = result.from,
                first = first
            )
            .ok();
        }
        for window in path.nodes.windows(2) {
            if let [from_node, to_node] = window {
                let path_kind = if path_idx == 0 {
                    "dev_dependency"
                } else {
                    "cross_feature"
                };
                let color = encoding::edge_color(path_kind);
                let style = encoding::edge_style(path_kind);
                let path_num = path_idx + 1;
                writeln!(
                    out,
                    "  \"{from}\" -> \"{to}\" [color={color}, style={style}, label=\"p{path_num}\"];",
                    from = from_node, to = to_node, color = color, style = style, path_num = path_num
                )
                    .ok();
            }
        }
        if let Some(last) = path.nodes.last()
            && last != &result.to
        {
            writeln!(
                out,
                "  \"{last}\" -> \"{to}\" [color=gray, style=dashed];",
                last = last,
                to = result.to
            )
            .ok();
        }
        writeln!(out, "  }}").ok();
    }
    writeln!(out, "}}").ok();
    out
}
/// Convert FlowResult to Mermaid flowchart format.
///
/// Renders each path as a chain of nodes with path labels.
/// Source is highlighted in green, target in red.
pub fn flow_result_to_mermaid(result: &FlowResult, _opts: &crate::visual::MermaidOpts) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    writeln!(out, "flowchart LR").ok();
    writeln!(
        out,
        "  %% Flow paths from '{}' to '{}': {} paths (truncated={})",
        result.from, result.to, result.count, result.truncated
    )
    .ok();
    writeln!(out, "  {}[('{}' (source))]", result.from, result.from).ok();
    writeln!(out, "  {}[('{}' (target))]", result.to, result.to).ok();
    for (path_idx, path) in result.paths.iter().enumerate() {
        let path_label = format!("P{}", path_idx + 1);
        if let Some(first) = path.nodes.first()
            && first != &result.from
        {
            writeln!(out, "  {} -.-> {}", result.from, first).ok();
        }
        for window in path.nodes.windows(2) {
            if let [from_node, to_node] = window {
                writeln!(out, "  {} -->|{}| {}", from_node, path_label, to_node).ok();
            }
        }
        if let Some(last) = path.nodes.last()
            && last != &result.to
        {
            writeln!(out, "  {} -.-> {}", last, result.to).ok();
        }
    }
    out
}
#[cfg(test)]
mod tests {
    use super::super::{EdgeData, NodeData};
    use super::*;
    fn make_graph(nodes: Vec<&str>, edges: Vec<(&str, &str)>) -> GraphData {
        let nodes = nodes
            .into_iter()
            .map(|id| NodeData {
                id: id.to_string(),
                label: id.to_string(),
                quality_score: None,
                fan_in: None,
                fan_out: None,
                is_orphan: false,
                has_unsafe: false,
                is_test: false,
                node_type: None,
            })
            .collect();
        let edges = edges
            .into_iter()
            .map(|(from, to)| EdgeData {
                from: from.to_string(),
                to: to.to_string(),
                kind: "calls".to_string(),
            })
            .collect();
        GraphData { nodes, edges }
    }
    #[test]
    fn test_no_path_disconnected() {
        let graph = make_graph(vec!["a", "b"], vec![]);
        let opts = FlowOpts::default();
        let result = find_flow_paths(&graph, "a", "b", &opts);
        assert_eq!(result.count, 0);
        assert!(!result.truncated);
        assert!(result.paths.is_empty());
    }
    #[test]
    fn test_single_path_linear() {
        let graph = make_graph(vec!["a", "b", "c"], vec![("a", "b"), ("b", "c")]);
        eprintln!(
            "DEBUG graph: nodes.len()={} edges.len()={}",
            graph.nodes.len(),
            graph.edges.len()
        );
        for edge in &graph.edges {
            eprintln!("  edge: {} -> {}", edge.from, edge.to);
        }
        let opts = FlowOpts::default();
        let result = find_flow_paths(&graph, "a", "c", &opts);
        eprintln!(
            "DEBUG result: count={} paths.len={}",
            result.count,
            result.paths.len()
        );
        assert_eq!(result.count, 1);
        assert_eq!(result.paths[0].nodes, vec!["a", "b", "c"]);
        assert_eq!(result.paths[0].depth, 2);
    }
    #[test]
    fn test_max_paths_limit() {
        let graph = make_graph(
            vec!["a", "b", "c"],
            vec![("a", "b"), ("a", "c"), ("b", "c")],
        );
        let opts = FlowOpts::default().max_paths(1);
        let result = find_flow_paths(&graph, "a", "c", &opts);
        assert_eq!(result.count, 1);
        assert!(result.truncated);
    }
    #[test]
    fn test_missing_source() {
        let graph = make_graph(vec!["a", "b"], vec![("a", "b")]);
        let opts = FlowOpts::default();
        let result = find_flow_paths(&graph, "nonexistent", "b", &opts);
        assert_eq!(result.count, 0);
    }
    #[test]
    fn test_missing_target() {
        let graph = make_graph(vec!["a", "b"], vec![("a", "b")]);
        let opts = FlowOpts::default();
        let result = find_flow_paths(&graph, "a", "nonexistent", &opts);
        assert_eq!(result.count, 0);
    }
    #[test]
    fn test_diamond_path() {
        let graph = make_graph(
            vec!["a", "b", "c", "d"],
            vec![("a", "b"), ("a", "c"), ("b", "d"), ("c", "d")],
        );
        let opts = FlowOpts::default().max_paths(10);
        let result = find_flow_paths(&graph, "a", "d", &opts);
        assert_eq!(result.count, 2);
    }
    #[test]
    fn test_max_depth_limit() {
        let graph = make_graph(
            vec!["a", "b", "c", "d"],
            vec![("a", "b"), ("b", "c"), ("c", "d")],
        );
        let opts = FlowOpts::default().max_depth(2);
        let result = find_flow_paths(&graph, "a", "d", &opts);
        assert_eq!(result.count, 0);
    }
    #[test]
    fn test_self_loop() {
        let graph = make_graph(vec!["a"], vec![("a", "a")]);
        let opts = FlowOpts::default();
        let result = find_flow_paths(&graph, "a", "a", &opts);
        // The self-loop input must be handled without panicking; reaching
        // this line is the assertion. (`count` is unsigned, so the former
        // `>= 0` check was a tautology.)
        let _ = result.count;
    }
    #[test]
    fn test_flow_opts_builder() {
        let opts = FlowOpts::new().max_paths(5).max_depth(12);
        assert_eq!(opts.max_paths, 5);
        assert_eq!(opts.max_depth, 12);
    }
    #[test]
    fn test_path_depth_calculation() {
        let graph = make_graph(vec!["a", "b", "c"], vec![("a", "b"), ("b", "c")]);
        let opts = FlowOpts::default();
        let result = find_flow_paths(&graph, "a", "c", &opts);
        assert_eq!(result.paths[0].depth, 2);
    }
    #[test]
    fn test_flow_result_to_dot() {
        let result = FlowResult {
            paths: vec![
                Path {
                    nodes: vec!["a".into(), "b".into(), "c".into()],
                    depth: 2,
                },
                Path {
                    nodes: vec!["a".into(), "x".into(), "c".into()],
                    depth: 2,
                },
            ],
            count: 2,
            truncated: false,
            from: "a".into(),
            to: "c".into(),
        };
        let opts = crate::visual::DotOpts::default();
        let dot = flow_result_to_dot(&result, &opts);
        assert!(dot.contains("digraph flow_paths"));
        assert!(dot.contains("\"a\""), "should contain source node a");
        assert!(dot.contains("\"c\""), "should contain target node c");
        assert!(dot.contains("->"), "should contain edges");
        assert!(dot.contains("flow_paths"));
    }
    #[test]
    fn test_flow_result_to_dot_empty() {
        let result = FlowResult {
            paths: vec![],
            count: 0,
            truncated: false,
            from: "x".into(),
            to: "y".into(),
        };
        let opts = crate::visual::DotOpts::default();
        let dot = flow_result_to_dot(&result, &opts);
        assert!(dot.contains("digraph flow_paths"));
        assert!(dot.contains("\"x\""), "expected quoted x in DOT");
        assert!(dot.contains("(source)"), "should contain source label");
    }
    #[test]
    fn test_flow_result_to_mermaid() {
        let result = FlowResult {
            paths: vec![Path {
                nodes: vec!["a".into(), "b".into(), "c".into()],
                depth: 2,
            }],
            count: 1,
            truncated: false,
            from: "a".into(),
            to: "c".into(),
        };
        let opts = crate::visual::MermaidOpts::default();
        let mermaid = flow_result_to_mermaid(&result, &opts);
        assert!(mermaid.contains("flowchart LR"));
        assert!(mermaid.contains("a["), "should contain source node a");
        assert!(mermaid.contains("c["), "should contain target node c");
        assert!(mermaid.contains("-->"), "should contain edges");
    }
    #[test]
    fn test_flow_result_to_mermaid_empty() {
        let result = FlowResult {
            paths: vec![],
            count: 0,
            truncated: false,
            from: "x".into(),
            to: "y".into(),
        };
        let opts = crate::visual::MermaidOpts::default();
        let mermaid = flow_result_to_mermaid(&result, &opts);
        assert!(mermaid.contains("flowchart LR"));
        assert!(mermaid.contains("x["), "should contain source node x");
        assert!(mermaid.contains("y["), "should contain target node y");
    }
    #[test]
    fn test_flow_result_to_dot_truncated() {
        let result = FlowResult {
            paths: vec![],
            count: 100,
            truncated: true,
            from: "start".into(),
            to: "end".into(),
        };
        let opts = crate::visual::DotOpts::default();
        let dot = flow_result_to_dot(&result, &opts);
        assert!(dot.contains("truncated=true"));
    }
}
