//! Mermaid output formatter.

use super::encoding;
use crate::visual::{GraphData, MermaidOpts};
use std::fmt::Write;

/// Map border_style encoding to Mermaid class name.
fn border_class(border_style: &str) -> Option<&'static str> {
    match border_style {
        "double" => Some("border-unsafe"),
        "dashed" => Some("border-feature"),
        _ => None,
    }
}

/// Emit node-level styling lines for fillcolor, border, and node type.
fn write_node_styles(
    out: &mut String,
    node_id: &str,
    fillcolor: &str,
    border_style: &str,
    node_type_color: &str,
) {
    writeln!(out, "  style {node_id} fillcolor={fillcolor}").ok();
    if let Some(cls) = border_class(border_style) {
        writeln!(out, "  {node_id}:::{cls}").ok();
    }
    if node_type_color != "#eeeeee" {
        writeln!(out, "  {node_id}:::nodeType").ok();
    }
}

/// Map edge style keyword to Mermaid linkStyle dash pattern.
fn edge_link_style(style: &str) -> &'static str {
    match style {
        "dotted" => "-.",
        "dashed" => "--",
        "bold" => "==",
        _ => "--",
    }
}

/// Emit a single edge with style and optional color.
fn write_edge(out: &mut String, edge_idx: usize, from: &str, to: &str, kind: &str) {
    let style = encoding::edge_style(kind);
    let dash = edge_link_style(style);
    writeln!(out, "  {from} {dash}--> {to}").ok();
    let color = encoding::edge_color(kind);
    if color != "#000000" {
        writeln!(out, "  linkStyle {edge_idx} stroke:{color}").ok();
    }
}

/// Convert graph data to Mermaid flowchart format.
pub fn to_mermaid(graph: &GraphData, opts: &MermaidOpts) -> String {
    let mut out = String::new();
    writeln!(out, "flowchart TD").ok();

    for node in &graph.nodes {
        if node.is_orphan && !opts.include_orphans {
            continue;
        }
        let shape = encoding::node_shape(node.is_orphan, opts.include_tests, false);
        let fillcolor = encoding::quality_fillcolor(node.quality_score);
        let border_style = encoding::border_style(node.has_unsafe, false);
        let node_type_color = encoding::nodetype_fillcolor(node.node_type.clone());
        let label = node.label.replace('"', "\\\"");
        let node_repr = match shape {
            "diamond" => format!("{}(({}))", node.id, label),
            "note" => format!("{}(({}))", node.id, label),
            "triangle" => format!("{}{}", node.id, label),
            _ => format!("{}[{}]", node.id, label),
        };
        writeln!(out, "  {}", node_repr).ok();
        if fillcolor != "#eeeeee" || border_style != "solid" || node_type_color != "#eeeeee" {
            write_node_styles(&mut out, &node.id, fillcolor, border_style, node_type_color);
        }
    }

    for (edge_idx, edge) in graph.edges.iter().enumerate() {
        write_edge(&mut out, edge_idx, &edge.from, &edge.to, &edge.kind);
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::visual::{EdgeData, GraphData, MermaidOpts, NodeData, OutputFormat};
    use touring_intelligence::reasoning::semantic_graph::NodeType;

    fn make_test_graph() -> GraphData {
        GraphData {
            nodes: vec![
                NodeData {
                    id: "foo".into(),
                    label: "foo.rs".into(),
                    quality_score: Some(0.9),
                    fan_in: Some(3),
                    fan_out: Some(1),
                    is_orphan: false,
                    has_unsafe: false,
                    is_test: false,
                    node_type: None,
                },
                NodeData {
                    id: "bar".into(),
                    label: "bar.rs".into(),
                    quality_score: Some(0.3),
                    fan_in: Some(0),
                    fan_out: Some(0),
                    is_orphan: true,
                    has_unsafe: true,
                    is_test: false,
                    node_type: None,
                },
            ],
            edges: vec![EdgeData {
                from: "foo".into(),
                to: "bar".into(),
                kind: "imports".into(),
            }],
        }
    }

    /// Graph with all encoding features exercised.
    fn make_full_test_graph() -> GraphData {
        GraphData {
            nodes: vec![
                // High-quality non-orphan non-test node
                NodeData {
                    id: "good".into(),
                    label: "good.rs".into(),
                    quality_score: Some(0.9),
                    fan_in: Some(3),
                    fan_out: Some(1),
                    is_orphan: false,
                    has_unsafe: false,
                    is_test: false,
                    node_type: Some(NodeType::File),
                },
                // Test file
                NodeData {
                    id: "my_test".into(),
                    label: "my_test.rs".into(),
                    quality_score: None,
                    fan_in: Some(1),
                    fan_out: Some(0),
                    is_orphan: false,
                    has_unsafe: false,
                    is_test: true,
                    node_type: None,
                },
                // Low-quality orphan with unsafe
                NodeData {
                    id: "bad".into(),
                    label: "bad.rs".into(),
                    quality_score: Some(0.2),
                    fan_in: Some(0),
                    fan_out: Some(0),
                    is_orphan: true,
                    has_unsafe: true,
                    is_test: false,
                    node_type: Some(NodeType::Symbol),
                },
            ],
            edges: vec![
                EdgeData {
                    from: "good".into(),
                    to: "bad".into(),
                    kind: "build_dependency".into(),
                },
                EdgeData {
                    from: "bad".into(),
                    to: "my_test".into(),
                    kind: "cycle".into(),
                },
                EdgeData {
                    from: "my_test".into(),
                    to: "good".into(),
                    kind: "optional".into(),
                },
            ],
        }
    }

    #[test]
    fn test_to_mermaid_basic() {
        let graph = make_test_graph();
        // foo: non-orphan → box format {id}[{label}]
        // bar: orphan → triangle format {id}{label}
        let opts = MermaidOpts {
            include_orphans: true,
            ..Default::default()
        };
        let mermaid = to_mermaid(&graph, &opts);
        assert!(mermaid.contains("flowchart TD"));
        assert!(mermaid.contains("foo[foo.rs]"), "mermaid=\n{}", mermaid);
        // bar is orphan → triangle shape {id}{label} (no brackets, no separator)
        assert!(mermaid.contains("barbar.rs"), "mermaid=\n{}", mermaid);
        assert!(mermaid.contains("foo --> bar") || mermaid.contains("foo ----> bar"));
    }

    #[test]
    fn test_to_mermaid_orphan_filter() {
        let graph = make_test_graph();
        let opts = MermaidOpts {
            include_orphans: false,
            ..Default::default()
        };
        let mermaid = to_mermaid(&graph, &opts);
        assert!(mermaid.contains("flowchart TD"));
        assert!(mermaid.contains("foo[foo.rs]"));
        // bar is orphan, should be filtered out
        assert!(!mermaid.contains("barbar.rs"), "mermaid=\n{}", mermaid);
    }

    #[test]
    fn test_to_mermaid_orphan_include() {
        let graph = make_test_graph();
        let opts = MermaidOpts {
            include_orphans: true,
            ..Default::default()
        };
        let mermaid = to_mermaid(&graph, &opts);
        assert!(mermaid.contains("foo[foo.rs]"));
        // bar is orphan → triangle format {id}{label} (no brackets, no separator)
        assert!(mermaid.contains("barbar.rs"), "mermaid=\n{}", mermaid);
    }

    #[test]
    fn test_to_mermaid_full_encoding() {
        let graph = make_full_test_graph();
        let opts = MermaidOpts {
            include_orphans: true,
            include_tests: true,
            output_file: None,
            format: OutputFormat::Mermaid,
        };
        let mermaid = to_mermaid(&graph, &opts);

        // Node shape: test file → stadium (rounded box) shape for is_test
        // (encoding::node_shape maps is_test → "note" but mermaid renders "note" as rounded box)
        assert!(
            mermaid.contains("my_test(("),
            "test node should have stadium/note shape: mermaid=\n{}",
            mermaid
        );

        // Quality fillcolor: good (0.9) → green #a5d6a7, bad (0.2) → red #ef9a9a
        assert!(
            mermaid.contains("#a5d6a7"),
            "high quality should be green: mermaid=\n{}",
            mermaid
        );
        assert!(
            mermaid.contains("#ef9a9a"),
            "low quality should be red: mermaid=\n{}",
            mermaid
        );

        // Border style: bad has unsafe → double → border-unsafe class
        assert!(
            mermaid.contains(":::border-unsafe"),
            "unsafe node should have border-unsafe class: mermaid=\n{}",
            mermaid
        );

        // Node type color: good=File (#388e3c), bad=Symbol (#1976d2)
        assert!(
            mermaid.contains(":::nodeType"),
            "node with node_type should have nodeType class: mermaid=\n{}",
            mermaid
        );

        // Edge color: cycle → red #d32f2f via linkStyle stroke
        assert!(
            mermaid.contains("linkStyle 1 stroke:#d32f2f"),
            "cycle edge should be red: mermaid=\n{}",
            mermaid
        );
        // optional → dotted style via linkStyle stroke:#9e9e9e (grey)
        assert!(
            mermaid.contains("linkStyle 2 stroke:#9e9e9e"),
            "optional edge should be grey: mermaid=\n{}",
            mermaid
        );
    }

    #[test]
    fn test_to_mermaid_edge_colored_only_non_default() {
        // Edge with kind that maps to non-black color
        let graph = GraphData {
            nodes: vec![NodeData {
                id: "a".into(),
                label: "a.rs".into(),
                quality_score: None,
                fan_in: None,
                fan_out: None,
                is_orphan: false,
                has_unsafe: false,
                is_test: false,
                node_type: None,
            }],
            edges: vec![EdgeData {
                from: "a".into(),
                to: "a".into(),
                kind: "dev_dependency".into(),
            }],
        };
        let mermaid = to_mermaid(&graph, &MermaidOpts::default());
        // dev_dependency → #1976d2 (blue)
        assert!(
            mermaid.contains("linkStyle 0 stroke:#1976d2"),
            "dev_dependency edge should be blue: mermaid=\n{}",
            mermaid
        );
    }
}
