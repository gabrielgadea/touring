//! DOT (Graphviz) output formatter.

use super::encoding;
use crate::visual::{DotOpts, GraphData};
use std::fmt::Write;

/// Convert graph data to DOT format.
pub fn to_dot(graph: &GraphData, opts: &DotOpts) -> String {
    let mut out = String::new();
    writeln!(out, "digraph touring {{").ok();
    writeln!(out, "  rankdir=LR;").ok();
    writeln!(out, "  node [fontname=\"Helvetica\"];").ok();
    writeln!(out, "  edge [fontname=\"Helvetica\"];").ok();

    for node in &graph.nodes {
        if node.is_orphan && !opts.include_orphans {
            continue;
        }
        // Use encoding module for visual properties
        let shape = encoding::node_shape(node.is_orphan, opts.include_tests, false);
        let node_type_color = encoding::nodetype_fillcolor(node.node_type.clone());
        // Prefer nodetype color when available, fall back to quality color
        let fillcolor = if node_type_color != "#eeeeee" {
            node_type_color
        } else {
            encoding::quality_fillcolor(node.quality_score)
        };
        // Border style: unsafe = double, feature-gated = dashed, else solid
        let border_style = encoding::border_style(node.has_unsafe, false);
        // Filled style is applied regardless of node-type fill color.
        let style_attr = format!("\"{},filled\"", border_style);
        let label = node.label.replace('"', "\\\"");
        writeln!(
            out,
            "  \"{id}\" [shape={shape}, style={style_attr}, fillcolor=\"{fillcolor}\", label=\"{label}\"];",
            id = node.id,
            shape = shape,
            style_attr = style_attr,
            fillcolor = fillcolor,
            label = label
        )
        .ok();
    }

    for edge in &graph.edges {
        let kind = edge.kind.replace('"', "\\\"");
        let color = encoding::edge_color(&edge.kind);
        let style = encoding::edge_style(&edge.kind);
        writeln!(
            out,
            "  \"{from}\" -> \"{to}\" [color=\"{color}\", style=\"{style}\", label=\"{kind}\"];",
            from = edge.from,
            to = edge.to,
            kind = kind,
            color = color,
            style = style
        )
        .ok();
    }

    writeln!(out, "}}").ok();
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::visual::{DotOpts, EdgeData, GraphData, NodeData};

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

    #[test]
    fn test_to_dot_basic() {
        let graph = make_test_graph();
        let opts = DotOpts::default();
        let dot = to_dot(&graph, &opts);
        assert!(dot.contains("digraph touring"));
        assert!(dot.contains("\"foo\""));
        assert!(dot.contains("\"bar\""));
        assert!(dot.contains("->"));
    }

    #[test]
    fn test_to_dot_quality_colors() {
        let graph = make_test_graph();
        let opts = DotOpts {
            include_orphans: true,
            ..Default::default()
        };
        let dot = to_dot(&graph, &opts);
        // High quality (0.9) -> green
        assert!(dot.contains("#a5d6a7"));
        // Low quality (0.3) -> red
        assert!(dot.contains("#ef9a9a"));
    }

    #[test]
    fn test_to_dot_orphan_filter() {
        let graph = make_test_graph();
        let opts = DotOpts {
            include_orphans: false,
            ..Default::default()
        };
        let dot = to_dot(&graph, &opts);
        // Orphan bar node should not appear when filtered
        // But edge bar->bar or "bar" in label text might still match
        // Check for the actual node definition pattern
        assert!(
            !dot.contains("\"bar\" [shape="),
            "bar node should be filtered"
        );
    }

    #[test]
    fn test_to_dot_orphan_include() {
        let graph = make_test_graph();
        let opts = DotOpts {
            include_orphans: true,
            ..Default::default()
        };
        let dot = to_dot(&graph, &opts);
        // Orphan bar should appear when included
        assert!(
            dot.contains("\"bar\" [shape="),
            "bar node should be included"
        );
        // Triangle shape for orphan
        assert!(dot.contains("shape=triangle"));
    }
}
