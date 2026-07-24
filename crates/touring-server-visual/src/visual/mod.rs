//! Graph visualization output formatters.
//!
//! Provides DOT, Mermaid, and JSON export formats for graph data.

pub mod bundling;
pub mod cap;
pub mod dot;
pub mod encoding;
pub mod flow;
pub mod mermaid;
pub mod theme;
pub mod tred;

use serde::{Deserialize, Serialize};
use touring_intelligence::reasoning::semantic_graph::NodeType;

/// Confidence tier for blast/impact scores.
///
/// Represents the reliability of a numeric score based on data completeness
/// and structural properties of the code being analyzed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfidenceTier {
    /// Score is highly reliable — complete data, well-connected node.
    High,
    /// Score is moderately reliable — minor gaps in data or connectivity.
    Medium,
    /// Score has limited reliability — significant gaps or isolated node.
    Low,
    /// Score unavailable or cannot be computed.
    Unknown,
}

impl ConfidenceTier {
    /// Derive a confidence tier from a blast radius count.
    ///
    /// - 0: isolated node, limited signal (Low)
    /// - 1..=8: normal hub (Medium)
    /// - 9..=20: well-connected (High)
    /// - >20: very high fan-out but diminishing returns on confidence (Medium)
    pub fn from_blast_radius(count: usize) -> Self {
        match count {
            0 => ConfidenceTier::Low,
            1..=8 => ConfidenceTier::Medium,
            9..=20 => ConfidenceTier::High,
            _ => ConfidenceTier::Medium,
        }
    }

    /// Derive a confidence tier from a numeric quality score.
    ///
    /// - >= 0.8: High
    /// - >= 0.5: Medium
    /// - > 0.0: Low
    /// - NaN or negative: Unknown
    pub fn from_score(score: f64) -> Self {
        if score.is_nan() || score < 0.0 {
            return ConfidenceTier::Unknown;
        }
        if score >= 0.8 {
            ConfidenceTier::High
        } else if score >= 0.5 {
            ConfidenceTier::Medium
        } else {
            ConfidenceTier::Low
        }
    }

    /// Returns a human-readable label for the tier.
    pub fn label(&self) -> &'static str {
        match self {
            ConfidenceTier::High => "high",
            ConfidenceTier::Medium => "medium",
            ConfidenceTier::Low => "low",
            ConfidenceTier::Unknown => "unknown",
        }
    }
}

/// Options for blast/impact output.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BlastOpts {
    /// Include orphan nodes in output.
    pub include_orphans: bool,
    /// Maximum number of nodes to include in blast output.
    pub max_nodes: Option<usize>,
    /// Minimum blast radius count to include a node.
    pub min_blast_radius: Option<usize>,
    /// Include confidence tier metadata.
    pub include_tier: bool,
}

impl BlastOpts {
    /// Create BlastOpts with defaults.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set whether to include orphan nodes.
    pub fn include_orphans(mut self, yes: bool) -> Self {
        self.include_orphans = yes;
        self
    }

    /// Set maximum nodes in blast output.
    pub fn max_nodes(mut self, n: usize) -> Self {
        self.max_nodes = Some(n);
        self
    }

    /// Set minimum blast radius count threshold.
    pub fn min_blast_radius(mut self, n: usize) -> Self {
        self.min_blast_radius = Some(n);
        self
    }

    /// Enable confidence tier metadata in output.
    pub fn with_tier(mut self) -> Self {
        self.include_tier = true;
        self
    }
}

use std::str::FromStr;

/// Unified output format for graph visualization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OutputFormat {
    /// Machine-readable JSON serialization of the graph.
    Json,
    /// Graphviz DOT source (the default format).
    #[default]
    Dot,
    /// Mermaid diagram markup for Markdown/web rendering.
    Mermaid,
    /// Rendered SVG vector image.
    Svg,
}

impl FromStr for OutputFormat {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "json" => Ok(OutputFormat::Json),
            "dot" => Ok(OutputFormat::Dot),
            "mermaid" => Ok(OutputFormat::Mermaid),
            "svg" => Ok(OutputFormat::Svg),
            _ => Err(format!("unknown output format: {}", s)),
        }
    }
}

impl OutputFormat {
    /// Returns the canonical file extension for this output format.
    pub fn extension(&self) -> &'static str {
        match self {
            OutputFormat::Json => "json",
            OutputFormat::Dot => "dot",
            OutputFormat::Mermaid => "mmd",
            OutputFormat::Svg => "svg",
        }
    }
}

/// Options for DOT output.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DotOpts {
    /// Include orphan nodes in output.
    pub include_orphans: bool,
    /// Include test files in output.
    pub include_tests: bool,
    /// Output file path (None = stdout).
    pub output_file: Option<String>,
    /// Output format variant.
    #[serde(default)]
    pub format: OutputFormat,
}

/// Options for Mermaid output.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MermaidOpts {
    /// Include orphan nodes in output.
    pub include_orphans: bool,
    /// Include test files in output.
    pub include_tests: bool,
    /// Output file path (None = stdout).
    pub output_file: Option<String>,
    /// Output format variant.
    #[serde(default)]
    pub format: OutputFormat,
}

/// Graph data structure for visualization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphData {
    /// Nodes in the graph.
    pub nodes: Vec<NodeData>,
    /// Edges in the graph.
    pub edges: Vec<EdgeData>,
}

/// Node data for visualization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeData {
    /// Unique node identifier.
    pub id: String,
    /// Display label for the node.
    pub label: String,
    /// Quality score (0.0 - 1.0).
    pub quality_score: Option<f32>,
    /// Fan-in count (incoming dependencies).
    pub fan_in: Option<usize>,
    /// Fan-out count (outgoing dependencies).
    pub fan_out: Option<usize>,
    /// Whether the node is an orphan (no dependencies).
    pub is_orphan: bool,
    /// Whether the node contains unsafe code.
    pub has_unsafe: bool,
    /// Whether the node is a test file.
    #[serde(default)]
    pub is_test: bool,
    /// Semantic node type classification (Symbol/File/Concept/Session).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_type: Option<NodeType>,
}

/// Edge data for visualization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdgeData {
    /// Source node ID.
    pub from: String,
    /// Target node ID.
    pub to: String,
    /// Edge kind (e.g., "imports", "calls", "extends").
    pub kind: String,
}

pub use bundling::{BundlingConfig, CompatibilityThresholds, bundling_bundled};
pub use cap::cap_graph;
pub use dot::to_dot;
pub use flow::{
    FlowOpts, FlowResult, Path, find_flow_paths, flow_result_to_dot, flow_result_to_mermaid,
    flow_to_json,
};
pub use mermaid::to_mermaid;
pub use tred::transitive_reduction;

/// Poll `child` until it exits, killing it once `deadline` passes.
///
/// Returns `true` if `dot` finished on its own, `false` if it was killed on
/// timeout (or `try_wait` errored). std-only bound — no extra dependency.
fn wait_bounded(child: &mut std::process::Child, deadline: std::time::Instant) -> bool {
    use std::time::{Duration, Instant};
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return true,
            Ok(None) if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return false;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(_) => return false,
        }
    }
}

/// Maximum DOT input size we attempt to render to SVG. Graphviz `dot` layout is
/// super-linear; multi-megabyte inputs (e.g. the full workspace graph, ~50k edges,
/// ~8 MiB DOT) can take minutes or never converge. Above this we skip SVG and let
/// the caller fall back to raw DOT.
const MAX_DOT_BYTES_FOR_SVG: usize = 2 * 1024 * 1024;

/// Wall-clock bound for the `dot` subprocess. On timeout we kill it and return
/// `None` so the caller falls back to raw DOT instead of hanging forever.
const SVG_RENDER_TIMEOUT_SECS: u64 = 15;

/// Pipe DOT input through `dot` (graphviz) to generate SVG.
///
/// Returns the SVG string on success, or `None` if graphviz is unavailable, the
/// graph is too large to lay out (see `MAX_DOT_BYTES_FOR_SVG`), or rendering
/// exceeds `SVG_RENDER_TIMEOUT_SECS` — in which cases the caller falls back to DOT.
pub fn dot_pipe_svg(dot_input: &str) -> Option<String> {
    use std::io::{Read, Write};
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    // Fast path: graphs too large for graphviz to lay out in bounded time skip
    // straight to the DOT fallback instead of spawning a doomed `dot` process.
    if dot_input.len() > MAX_DOT_BYTES_FOR_SVG {
        return None;
    }

    let mut child = Command::new("dot")
        .args(["-Tsvg"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;

    // Write stdin and read stdout on dedicated threads so the pipes are drained
    // concurrently — writing the whole (large) DOT input before reading stdout
    // deadlocks once `dot`'s stdout buffer fills. The main thread enforces a
    // wall-clock bound and kills `dot` if its layout never converges.
    let mut stdin = child.stdin.take()?;
    let mut stdout = child.stdout.take()?;
    let input = dot_input.to_owned();
    let writer = std::thread::spawn(move || {
        let _ = stdin.write_all(input.as_bytes());
        // `stdin` dropped here → EOF so `dot` finishes reading input.
    });
    let reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stdout.read_to_end(&mut buf);
        buf
    });

    let deadline = Instant::now() + Duration::from_secs(SVG_RENDER_TIMEOUT_SECS);
    let finished = wait_bounded(&mut child, deadline);
    let _ = writer.join();
    let buf = reader.join().ok()?;
    if !finished {
        return None;
    }
    let svg = String::from_utf8(buf).ok()?;
    if svg.is_empty() {
        return None;
    }
    Some(svg)
}

#[cfg(test)]
mod tests {
    use super::*;

    // === ConfidenceTier tests ===

    #[test]
    fn test_confidence_tier_from_blast_radius_zero() {
        assert_eq!(ConfidenceTier::from_blast_radius(0), ConfidenceTier::Low);
    }

    #[test]
    fn test_confidence_tier_from_blast_radius_small() {
        assert_eq!(ConfidenceTier::from_blast_radius(1), ConfidenceTier::Medium);
        assert_eq!(ConfidenceTier::from_blast_radius(5), ConfidenceTier::Medium);
        assert_eq!(ConfidenceTier::from_blast_radius(8), ConfidenceTier::Medium);
    }

    #[test]
    fn test_confidence_tier_from_blast_radius_large() {
        assert_eq!(ConfidenceTier::from_blast_radius(9), ConfidenceTier::High);
        assert_eq!(ConfidenceTier::from_blast_radius(15), ConfidenceTier::High);
        assert_eq!(ConfidenceTier::from_blast_radius(20), ConfidenceTier::High);
    }

    #[test]
    fn test_confidence_tier_from_blast_radius_very_large() {
        // >20 falls back to Medium
        assert_eq!(
            ConfidenceTier::from_blast_radius(21),
            ConfidenceTier::Medium
        );
        assert_eq!(
            ConfidenceTier::from_blast_radius(100),
            ConfidenceTier::Medium
        );
    }

    #[test]
    fn test_confidence_tier_from_score_high() {
        assert_eq!(ConfidenceTier::from_score(0.8), ConfidenceTier::High);
        assert_eq!(ConfidenceTier::from_score(1.0), ConfidenceTier::High);
        assert_eq!(ConfidenceTier::from_score(0.95), ConfidenceTier::High);
    }

    #[test]
    fn test_confidence_tier_from_score_medium() {
        assert_eq!(ConfidenceTier::from_score(0.5), ConfidenceTier::Medium);
        assert_eq!(ConfidenceTier::from_score(0.79), ConfidenceTier::Medium);
    }

    #[test]
    fn test_confidence_tier_from_score_low() {
        assert_eq!(ConfidenceTier::from_score(0.49), ConfidenceTier::Low);
        assert_eq!(ConfidenceTier::from_score(0.1), ConfidenceTier::Low);
        assert_eq!(ConfidenceTier::from_score(0.0), ConfidenceTier::Low);
    }

    #[test]
    fn test_confidence_tier_from_score_unknown() {
        assert_eq!(
            ConfidenceTier::from_score(f64::NAN),
            ConfidenceTier::Unknown
        );
        assert_eq!(ConfidenceTier::from_score(-0.1), ConfidenceTier::Unknown);
    }

    #[test]
    fn test_confidence_tier_label() {
        assert_eq!(ConfidenceTier::High.label(), "high");
        assert_eq!(ConfidenceTier::Medium.label(), "medium");
        assert_eq!(ConfidenceTier::Low.label(), "low");
        assert_eq!(ConfidenceTier::Unknown.label(), "unknown");
    }

    #[test]
    fn test_confidence_tier_serde_serialize() {
        let json = serde_json::to_string(&ConfidenceTier::High).unwrap();
        assert_eq!(json, "\"high\"");

        let json = serde_json::to_string(&ConfidenceTier::Medium).unwrap();
        assert_eq!(json, "\"medium\"");

        let json = serde_json::to_string(&ConfidenceTier::Low).unwrap();
        assert_eq!(json, "\"low\"");

        let json = serde_json::to_string(&ConfidenceTier::Unknown).unwrap();
        assert_eq!(json, "\"unknown\"");
    }

    #[test]
    fn test_confidence_tier_serde_deserialize() {
        let high: ConfidenceTier = serde_json::from_str("\"high\"").unwrap();
        assert_eq!(high, ConfidenceTier::High);

        let medium: ConfidenceTier = serde_json::from_str("\"medium\"").unwrap();
        assert_eq!(medium, ConfidenceTier::Medium);

        let low: ConfidenceTier = serde_json::from_str("\"low\"").unwrap();
        assert_eq!(low, ConfidenceTier::Low);

        let unknown: ConfidenceTier = serde_json::from_str("\"unknown\"").unwrap();
        assert_eq!(unknown, ConfidenceTier::Unknown);
    }

    // === BlastOpts tests ===

    #[test]
    fn test_blast_opts_default() {
        let opts = BlastOpts::default();
        assert!(!opts.include_orphans);
        assert!(opts.max_nodes.is_none());
        assert!(opts.min_blast_radius.is_none());
        assert!(!opts.include_tier);
    }

    #[test]
    fn test_blast_opts_builder_include_orphans() {
        let opts = BlastOpts::new().include_orphans(true);
        assert!(opts.include_orphans);
    }

    #[test]
    fn test_blast_opts_builder_max_nodes() {
        let opts = BlastOpts::new().max_nodes(100);
        assert_eq!(opts.max_nodes, Some(100));
    }

    #[test]
    fn test_blast_opts_builder_min_blast_radius() {
        let opts = BlastOpts::new().min_blast_radius(5);
        assert_eq!(opts.min_blast_radius, Some(5));
    }

    #[test]
    fn test_blast_opts_builder_with_tier() {
        let opts = BlastOpts::new().with_tier();
        assert!(opts.include_tier);
    }

    #[test]
    fn test_blast_opts_chaining() {
        let opts = BlastOpts::new()
            .include_orphans(true)
            .max_nodes(50)
            .min_blast_radius(3)
            .with_tier();

        assert!(opts.include_orphans);
        assert_eq!(opts.max_nodes, Some(50));
        assert_eq!(opts.min_blast_radius, Some(3));
        assert!(opts.include_tier);
    }

    #[test]
    fn test_blast_opts_serde_roundtrip() {
        let opts = BlastOpts::new()
            .include_orphans(true)
            .max_nodes(100)
            .min_blast_radius(5)
            .with_tier();

        let json = serde_json::to_string(&opts).unwrap();
        let restored: BlastOpts = serde_json::from_str(&json).unwrap();

        assert_eq!(restored.include_orphans, opts.include_orphans);
        assert_eq!(restored.max_nodes, opts.max_nodes);
        assert_eq!(restored.min_blast_radius, opts.min_blast_radius);
        assert_eq!(restored.include_tier, opts.include_tier);
    }

    // === GraphData with tier integration tests ===

    #[test]
    fn test_node_data_with_all_fields() {
        let node = NodeData {
            id: "test.rs".into(),
            label: "test.rs".into(),
            quality_score: Some(0.85),
            fan_in: Some(3),
            fan_out: Some(2),
            is_orphan: false,
            has_unsafe: true,
            is_test: true,
            node_type: None,
        };
        assert_eq!(node.id, "test.rs");
        assert_eq!(node.quality_score, Some(0.85));
    }

    #[test]
    fn test_graph_data_empty() {
        let graph = GraphData {
            nodes: vec![],
            edges: vec![],
        };
        assert!(graph.nodes.is_empty());
        assert!(graph.edges.is_empty());
    }
}
