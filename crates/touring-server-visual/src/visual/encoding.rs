//! Visual encoding for graph nodes and edges.
//!
//! Provides style functions that map graph properties (quality, safety,
//! feature gates, node type, edge kind) to DOT/Mermaid attributes.

use serde::{Deserialize, Serialize};
use touring_intelligence::reasoning::semantic_graph::NodeType;

/// Node visual style derived from graph properties.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeStyle {
    /// DOT shape attribute.
    pub shape: String,
    /// Fillcolor attribute.
    pub fillcolor: String,
    /// Style attribute (solid, dashed, dotted, double).
    pub style: String,
    /// Font size in points.
    pub fontsize: u8,
}

/// Edge visual style derived from edge kind.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdgeStyle {
    /// Color attribute.
    pub color: String,
    /// Style attribute (solid, dashed, dotted, bold).
    pub style: String,
}

/// Determine node shape based on node type.
///
/// - `is_god_node`: diamond (fan-in > threshold, no outgoing)
/// - `is_orphan`: triangle (no dependencies either way)
/// - `is_test`: note (test file)
/// - default: box
pub fn node_shape(is_orphan: bool, is_test: bool, is_god_node: bool) -> &'static str {
    if is_god_node {
        "diamond"
    } else if is_test {
        "note"
    } else if is_orphan {
        "triangle"
    } else {
        "box"
    }
}

/// Determine fill color based on quality score.
///
/// - >= 0.8: green `#a5d6a7`
/// - >= 0.5: yellow `#fff59d`
/// - > 0.0: red `#ef9a9a`
/// - None: grey `#eeeeee`
pub fn quality_fillcolor(quality: Option<f32>) -> &'static str {
    match quality {
        Some(q) if q >= 0.8 => "#a5d6a7",
        Some(q) if q >= 0.5 => "#fff59d",
        Some(_) => "#ef9a9a",
        None => "#eeeeee",
    }
}

/// Determine border style based on code properties.
///
/// - `has_unsafe`: double border
/// - `is_feature_gated`: dashed border
/// - default: solid
pub fn border_style(has_unsafe: bool, is_feature_gated: bool) -> &'static str {
    if has_unsafe {
        "double"
    } else if is_feature_gated {
        "dashed"
    } else {
        "solid"
    }
}

/// Fillcolor for NodeType semantic classification.
///
/// - `Some(Symbol)`: blue `#1976d2`
/// - `Some(File)`: green `#388e3c`
/// - `Some(Concept)`: yellow `#fff59d`
/// - `Some(Session)`: purple `#7b1fa2`
/// - `None`: grey `#eeeeee`
pub fn nodetype_fillcolor(node_type: Option<NodeType>) -> &'static str {
    match node_type {
        Some(NodeType::Symbol) => "#1976d2",
        Some(NodeType::File) => "#388e3c",
        Some(NodeType::Concept) => "#fff59d",
        Some(NodeType::Session) => "#7b1fa2",
        None => "#eeeeee",
    }
}

/// Edge color by dependency kind.
///
/// - dev_dependency: blue `#1976d2`
/// - build_dependency: green `#388e3c`
/// - cross_feature: purple `#7b1fa2`
/// - cycle: red `#d32f2f`
/// - default: black `#000000`
pub fn edge_color(kind: &str) -> &'static str {
    match kind {
        "dev_dependency" => "#1976d2",
        "build_dependency" => "#388e3c",
        "cross_feature" => "#7b1fa2",
        "cycle" => "#d32f2f",
        "optional" => "#9e9e9e",
        "transitively_optional" => "#bdbdbd",
        _ => "#000000",
    }
}

/// Edge style by dependency kind.
///
/// - optional: dotted
/// - transitively_optional: dashed
/// - cycle: bold
/// - default: solid
pub fn edge_style(kind: &str) -> &'static str {
    match kind {
        "optional" => "dotted",
        "transitively_optional" => "dashed",
        "cycle" => "bold",
        _ => "solid",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_shape_defaults() {
        assert_eq!(node_shape(false, false, false), "box");
        assert_eq!(node_shape(true, false, false), "triangle");
        assert_eq!(node_shape(false, true, false), "note");
        assert_eq!(node_shape(false, false, true), "diamond");
    }

    #[test]
    fn test_quality_fillcolor() {
        assert_eq!(quality_fillcolor(Some(0.9)), "#a5d6a7");
        assert_eq!(quality_fillcolor(Some(0.8)), "#a5d6a7");
        assert_eq!(quality_fillcolor(Some(0.5)), "#fff59d");
        assert_eq!(quality_fillcolor(Some(0.1)), "#ef9a9a");
        assert_eq!(quality_fillcolor(None), "#eeeeee");
    }

    #[test]
    fn test_border_style() {
        assert_eq!(border_style(false, false), "solid");
        assert_eq!(border_style(true, false), "double");
        assert_eq!(border_style(false, true), "dashed");
        assert_eq!(border_style(true, true), "double");
    }

    #[test]
    fn test_edge_color() {
        assert_eq!(edge_color("dev_dependency"), "#1976d2");
        assert_eq!(edge_color("build_dependency"), "#388e3c");
        assert_eq!(edge_color("cross_feature"), "#7b1fa2");
        assert_eq!(edge_color("cycle"), "#d32f2f");
        assert_eq!(edge_color("optional"), "#9e9e9e");
        assert_eq!(edge_color("unknown"), "#000000");
    }

    #[test]
    fn test_edge_style() {
        assert_eq!(edge_style("optional"), "dotted");
        assert_eq!(edge_style("transitively_optional"), "dashed");
        assert_eq!(edge_style("cycle"), "bold");
        assert_eq!(edge_style("normal"), "solid");
    }

    #[test]
    fn test_nodetype_fillcolor() {
        use touring_intelligence::reasoning::semantic_graph::NodeType;
        assert_eq!(nodetype_fillcolor(Some(NodeType::Symbol)), "#1976d2");
        assert_eq!(nodetype_fillcolor(Some(NodeType::File)), "#388e3c");
        assert_eq!(nodetype_fillcolor(Some(NodeType::Concept)), "#fff59d");
        assert_eq!(nodetype_fillcolor(Some(NodeType::Session)), "#7b1fa2");
        assert_eq!(nodetype_fillcolor(None), "#eeeeee");
    }
}
