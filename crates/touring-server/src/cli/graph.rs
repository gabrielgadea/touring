//! `touring graph file|god-nodes|shortest-path|communities` — Graph analysis and topology queries.
//!
//! Provides file dependency graphs, god-node identification, path analysis,
//! and community detection using existing wiring and AST blast-radius data.
//!
//! Supports multiple output formats: JSON (default), DOT, and Mermaid.
//!
//! ## Cap options
//! `--max-nodes <N>`   Cap graph to N nodes (default: 200)
//! `--max-edges <M>`   Cap graph to M edges (default: 500)
//! --reduce          Apply transitive reduction (DAGs only)

use super::daemon_query;
use crate::visual::{
    self, DotOpts, EdgeData, FlowOpts, FlowResult, GraphData, MermaidOpts, NodeData,
    OutputFormat as VisualOutputFormat, cap_graph, flow_result_to_dot, flow_result_to_mermaid,
    flow_to_json, transitive_reduction,
};
use clap::{Parser, Subcommand};
use thiserror::Error;
use touring_intelligence::reasoning::NodeType;

/// Error type for graph conversions.
#[derive(Debug, Error)]
pub enum GraphConvertError {
    /// The daemon's JSON response could not be deserialized into the expected
    /// graph response shape.
    #[error("failed to parse daemon response: {0}")]
    ParseError(#[from] serde_json::Error),
    /// The daemon returned an error message instead of a valid response.
    #[error("daemon returned error: {0}")]
    DaemonError(String),
}

/// Intermediate structure returned by `cli-ast-blast` daemon handler.
#[derive(Debug, Clone, serde::Deserialize)]
struct AstBlastResponse {
    #[serde(rename = "blast_radius")]
    blast_radius: usize,
    consumers: Vec<String>,
    #[serde(rename = "file_path")]
    file_path: String,
    #[serde(rename = "coedit_files")]
    coedit_files: Vec<String>,
    diagnostics: Vec<serde_json::Value>,
}

/// Convert a `cli-ast-blast` JSON response into `GraphData` for visualization.
///
/// The source file becomes the root node (id = `response.file_path`,
/// labeled with `blast_radius` for visual weight). Each consumer file is a
/// child node, with a directed edge from root → consumer carrying
/// `kind="blast"`. Co-edit relationships are added as auxiliary nodes
/// connected via `kind="coedit"` edges. Diagnostics are attached as a
/// separate node-type annotation when present, exposing daemon findings
/// at the visualization layer.
fn shortest_path_to_graph(json_response: &str) -> Result<GraphData, GraphConvertError> {
    let response: AstBlastResponse = serde_json::from_str(json_response)?;

    let root_label = if response.blast_radius > 0 {
        format!("{} (blast={})", response.file_path, response.blast_radius)
    } else {
        response.file_path.clone()
    };

    let mut nodes: Vec<NodeData> =
        Vec::with_capacity(response.consumers.len() + response.coedit_files.len() + 1);
    let mut edges: Vec<EdgeData> =
        Vec::with_capacity(response.consumers.len() + response.coedit_files.len());

    // Root node (source file) — only emit when file_path is non-empty.
    if !response.file_path.is_empty() {
        nodes.push(NodeData {
            id: response.file_path.clone(),
            label: root_label,
            quality_score: None,
            fan_in: Some(0),
            fan_out: Some(response.consumers.len()),
            is_orphan: false,
            has_unsafe: false,
            is_test: false,
            node_type: Some(NodeType::File),
        });
    }

    for consumer in &response.consumers {
        nodes.push(NodeData {
            id: consumer.clone(),
            label: consumer.clone(),
            quality_score: None,
            fan_in: None,
            fan_out: None,
            is_orphan: false,
            has_unsafe: false,
            is_test: consumer.ends_with("_test.rs")
                || consumer.starts_with("tests/")
                || consumer.contains("/tests/"),
            node_type: Some(NodeType::File),
        });
        if !response.file_path.is_empty() {
            edges.push(EdgeData {
                from: response.file_path.clone(),
                to: consumer.clone(),
                kind: "blast".to_string(),
            });
        }
    }

    for coedit in &response.coedit_files {
        nodes.push(NodeData {
            id: coedit.clone(),
            label: coedit.clone(),
            quality_score: None,
            fan_in: None,
            fan_out: None,
            is_orphan: false,
            has_unsafe: false,
            is_test: false,
            node_type: Some(NodeType::File),
        });
        if !response.file_path.is_empty() {
            edges.push(EdgeData {
                from: response.file_path.clone(),
                to: coedit.clone(),
                kind: "coedit".to_string(),
            });
        }
    }

    // Diagnostics surface as a single annotation node when present
    // (concise; not flooding the graph with per-diagnostic nodes).
    if !response.diagnostics.is_empty() {
        let diag_id = format!("__diagnostics__:{}", response.file_path);
        nodes.push(NodeData {
            id: diag_id.clone(),
            label: format!("{} diagnostic(s)", response.diagnostics.len()),
            quality_score: None,
            fan_in: None,
            fan_out: None,
            is_orphan: false,
            has_unsafe: false,
            is_test: false,
            node_type: Some(NodeType::Concept),
        });
        if !response.file_path.is_empty() {
            edges.push(EdgeData {
                from: response.file_path.clone(),
                to: diag_id,
                kind: "diagnostic".to_string(),
            });
        }
    }

    Ok(GraphData { nodes, edges })
}

/// Wrapper to convert WiringModuleStatus[] (cli-wiring-modules) response
/// into GraphData for visualization.
fn communities_to_graph(json_response: &str) -> Result<GraphData, GraphConvertError> {
    // WiringModuleStatus has fields: module_id, module_path, integration_score,
    // public_api_count, orphan_count, quality_score, fan_in, fan_out
    #[derive(Debug, serde::Deserialize)]
    struct WiringModuleStatus {
        module_id: Option<String>,
        module_path: Option<String>,
        integration_score: Option<f32>,
        quality_score: Option<f32>,
        fan_in: Option<usize>,
        fan_out: Option<usize>,
        public_api_count: Option<usize>,
        orphan_count: Option<usize>,
    }

    let modules: Vec<WiringModuleStatus> =
        serde_json::from_str(json_response).map_err(GraphConvertError::from)?;

    let nodes: Vec<NodeData> = modules
        .iter()
        .map(|m| {
            let id = m
                .module_id
                .clone()
                .unwrap_or_else(|| m.module_path.clone().unwrap_or_default());
            // Enrich the visual label with integration_score and
            // public_api_count when present — these previously-unread
            // fields now drive at-a-glance triage in DOT/Mermaid output.
            let base_label = m.module_path.clone().unwrap_or_else(|| id.clone());
            let mut annotations: Vec<String> = Vec::new();
            if let Some(score) = m.integration_score {
                annotations.push(format!("int={:.2}", score));
            }
            if let Some(api) = m.public_api_count {
                annotations.push(format!("api={}", api));
            }
            let label = if annotations.is_empty() {
                base_label
            } else {
                format!("{} [{}]", base_label, annotations.join(" "))
            };
            NodeData {
                id,
                label,
                quality_score: m.quality_score,
                fan_in: m.fan_in,
                fan_out: m.fan_out,
                is_orphan: m.orphan_count.map(|c| c > 0).unwrap_or(false),
                has_unsafe: false,
                is_test: false,
                node_type: m.integration_score.map(|_| NodeType::File),
            }
        })
        .collect();

    Ok(GraphData {
        nodes,
        edges: vec![],
    })
}

/// Output format for graph queries.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
enum OutputFormat {
    #[default]
    Json,
    Dot,
    Mermaid,
    Svg,
}

impl std::str::FromStr for OutputFormat {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "json" => Ok(Self::Json),
            "dot" => Ok(Self::Dot),
            "mermaid" => Ok(Self::Mermaid),
            "svg" => Ok(Self::Svg),
            _ => Err(format!(
                "unknown format '{}': use json, dot, mermaid, or svg",
                s
            )),
        }
    }
}

/// Apply cap and/or reduction options to graph data.
fn apply_graph_options(
    graph: GraphData,
    max_nodes: usize,
    max_edges: usize,
    reduce: bool,
) -> GraphData {
    let node_ids: Vec<String> = graph.nodes.iter().map(|n| n.id.clone()).collect();
    let node_map: std::collections::HashMap<&str, usize> = node_ids
        .iter()
        .enumerate()
        .map(|(i, id)| (id.as_str(), i))
        .collect();

    let mut edges: Vec<(usize, usize, ())> = Vec::new();
    for edge in &graph.edges {
        if let (Some(&from_idx), Some(&to_idx)) = (
            node_map.get(edge.from.as_str()),
            node_map.get(edge.to.as_str()),
        ) {
            edges.push((from_idx, to_idx, ()));
        }
    }

    if reduce {
        match transitive_reduction(&node_ids, &edges) {
            Ok(reduced) => edges = reduced,
            Err(msg) => eprintln!("Warning: {}", msg),
        }
    }

    let (selected_nodes, capped_edges, _) = cap_graph(&node_ids, &edges, max_nodes, max_edges);

    let selected_set: std::collections::HashSet<usize> = selected_nodes.iter().cloned().collect();
    let new_nodes: Vec<_> = graph
        .nodes
        .into_iter()
        .enumerate()
        .filter(|(i, _)| selected_set.contains(i))
        .map(|(_, n)| n)
        .collect();

    let new_edges: Vec<_> = capped_edges
        .iter()
        .filter(|(from, to, _)| selected_set.contains(from) && selected_set.contains(to))
        .map(|(from, to, _)| EdgeData {
            from: node_ids.get(*from).unwrap_or(&"?".to_string()).clone(),
            to: node_ids.get(*to).unwrap_or(&"?".to_string()).clone(),
            kind: "depends".to_string(),
        })
        .collect();

    GraphData {
        nodes: new_nodes,
        edges: new_edges,
    }
}

/// Handle output formatting based on requested format.
fn handle_format(
    json_output: &str,
    format: OutputFormat,
    max_nodes: usize,
    max_edges: usize,
    reduce: bool,
    include_tests: bool,
) -> anyhow::Result<()> {
    match format {
        OutputFormat::Json => {
            if let Ok(graph) = serde_json::from_str::<GraphData>(json_output) {
                let processed = apply_graph_options(graph, max_nodes, max_edges, reduce);
                println!(
                    "{}",
                    serde_json::to_string(&processed).unwrap_or_else(|_| json_output.to_string())
                );
            } else {
                println!("{}", json_output);
            }
        }
        OutputFormat::Dot | OutputFormat::Mermaid | OutputFormat::Svg => {
            match serde_json::from_str::<GraphData>(json_output) {
                Ok(graph) => {
                    let processed = apply_graph_options(graph, max_nodes, max_edges, reduce);
                    match format {
                        OutputFormat::Dot => {
                            let opts = DotOpts {
                                include_orphans: false,
                                include_tests,
                                output_file: None,
                                format: VisualOutputFormat::Dot,
                            };
                            println!("{}", visual::to_dot(&processed, &opts));
                        }
                        OutputFormat::Mermaid => {
                            let opts = MermaidOpts {
                                include_orphans: false,
                                include_tests,
                                output_file: None,
                                format: VisualOutputFormat::Mermaid,
                            };
                            println!("{}", visual::to_mermaid(&processed, &opts));
                        }
                        OutputFormat::Svg => {
                            let opts = DotOpts {
                                include_orphans: false,
                                include_tests,
                                output_file: None,
                                format: VisualOutputFormat::Dot,
                            };
                            let dot_output = visual::to_dot(&processed, &opts);
                            match run_dot_to_svg(&dot_output) {
                                Ok(svg_output) => println!("{}", svg_output),
                                Err(_) => {
                                    eprintln!(
                                        "warning: graphviz 'dot' not available, outputting DOT format instead"
                                    );
                                    println!("{}", dot_output);
                                }
                            }
                        }
                        _ => println!("{}", json_output),
                    }
                }
                Err(_) => {
                    println!("{}", json_output);
                }
            }
        }
    }
    Ok(())
}

/// Dispatch to visual::dot_pipe_svg for DOT→SVG conversion.
fn run_dot_to_svg(dot_input: &str) -> anyhow::Result<String> {
    visual::dot_pipe_svg(dot_input)
        .ok_or_else(|| anyhow::anyhow!("dot (graphviz) unavailable or produced empty output"))
}

/// Dispatch graph query based on subcommand and parsed CLI args.
fn dispatch_graph_cmd(cmd: &GraphCmd, format: OutputFormat) -> anyhow::Result<()> {
    let (payload, query_name, max_nodes, max_edges, reduce, include_tests) = match cmd {
        GraphCmd::File {
            path,
            max_nodes,
            max_edges,
            reduce,
            include_tests,
            ..
        } => (
            serde_json::json!({ "file_path": path }),
            "cli-ast-blast",
            *max_nodes,
            *max_edges,
            *reduce,
            *include_tests,
        ),
        GraphCmd::GodNodes {
            top,
            max_nodes,
            max_edges,
            reduce,
            include_tests,
            ..
        } => (
            serde_json::json!({ "sort": "score_desc", "limit": top }),
            "cli-wiring-modules",
            *max_nodes,
            *max_edges,
            *reduce,
            *include_tests,
        ),
        GraphCmd::ShortestPath {
            from,
            to,
            max_nodes,
            max_edges,
            reduce,
            include_tests,
            ..
        } => (
            serde_json::json!({ "file_path": from, "target": to }),
            "cli-ast-blast",
            *max_nodes,
            *max_edges,
            *reduce,
            *include_tests,
        ),
        GraphCmd::Communities {
            max_nodes,
            max_edges,
            reduce,
            include_tests,
            ..
        } => (
            serde_json::json!({}),
            "cli-wiring-modules",
            *max_nodes,
            *max_edges,
            *reduce,
            *include_tests,
        ),
        GraphCmd::Flow {
            from,
            to,
            max_paths,
            max_depth,
            max_nodes,
            max_edges,
            reduce,
            include_tests,
            ..
        } => {
            let opts = FlowOpts::default()
                .max_paths(*max_paths)
                .max_depth(*max_depth);
            (
                serde_json::json!({
                    "from": from,
                    "to": to,
                    "max_paths": opts.max_paths,
                    "max_depth": opts.max_depth
                }),
                "cli-graph-flow",
                *max_nodes,
                *max_edges,
                *reduce,
                *include_tests,
            )
        }
        GraphCmd::Rename {
            from,
            to,
            apply,
            max_nodes,
            max_edges,
            reduce,
            include_tests,
            ..
        } => {
            let (file, line, col) = if from.contains(':') {
                let parts: Vec<&str> = from.split(':').collect();
                let fp = parts.first().copied().unwrap_or(from.as_str()).to_string();
                let ln = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
                let cl = parts.get(2).and_then(|s| s.parse().ok()).unwrap_or(0);
                (fp, ln, cl)
            } else {
                (from.clone(), 0, 0)
            };
            (
                serde_json::json!({
                    "file": file,
                    "line": line,
                    "column": col,
                    "new_name": to,
                    "apply": apply
                }),
                "cli-rename",
                *max_nodes,
                *max_edges,
                *reduce,
                *include_tests,
            )
        }
    };

    let subcommand_name = match cmd {
        GraphCmd::File { .. } => "file",
        GraphCmd::GodNodes { .. } => "god-nodes",
        GraphCmd::ShortestPath { .. } => "shortest-path",
        GraphCmd::Communities { .. } => "communities",
        GraphCmd::Flow { .. } => "flow",
        GraphCmd::Rename { .. } => "rename",
    };

    let output = daemon_query(query_name, payload)?;

    // Normalise daemon schemas to GraphData for shortest-path / communities
    let graph_data = match subcommand_name {
        "shortest-path" => match shortest_path_to_graph(&output) {
            Ok(gd) => Some(gd),
            Err(e) => {
                eprintln!("warning: shortest-path conversion failed: {}", e);
                None
            }
        },
        "communities" => match communities_to_graph(&output) {
            Ok(gd) => Some(gd),
            Err(e) => {
                eprintln!("warning: communities conversion failed: {}", e);
                None
            }
        },
        _ => None,
    };

    // FlowResult branch — dedicated DOT/Mermaid formatters
    if subcommand_name == "flow" {
        match serde_json::from_str::<FlowResult>(&output) {
            Ok(flow_result) => {
                match format {
                    OutputFormat::Json => {
                        println!("{}", flow_to_json(&flow_result));
                    }
                    OutputFormat::Dot => {
                        let opts = DotOpts {
                            include_orphans: false,
                            include_tests,
                            output_file: None,
                            format: VisualOutputFormat::Dot,
                        };
                        println!("{}", flow_result_to_dot(&flow_result, &opts));
                    }
                    OutputFormat::Mermaid => {
                        let opts = MermaidOpts {
                            include_orphans: false,
                            include_tests,
                            output_file: None,
                            format: VisualOutputFormat::Mermaid,
                        };
                        println!("{}", flow_result_to_mermaid(&flow_result, &opts));
                    }
                    OutputFormat::Svg => {
                        let opts = DotOpts {
                            include_orphans: false,
                            include_tests,
                            output_file: None,
                            format: VisualOutputFormat::Dot,
                        };
                        let dot_output = flow_result_to_dot(&flow_result, &opts);
                        match run_dot_to_svg(&dot_output) {
                            Ok(svg_output) => println!("{}", svg_output),
                            Err(_) => {
                                eprintln!(
                                    "warning: graphviz 'dot' not available, outputting DOT format instead"
                                );
                                println!("{}", dot_output);
                            }
                        }
                    }
                }
                return Ok(());
            }
            Err(e) => {
                eprintln!("warning: flow result parse failed: {}", e);
                println!("{}", output);
                return Ok(());
            }
        }
    }

    // If we have a converted GraphData, use it directly
    if let Some(gd) = graph_data {
        let processed = apply_graph_options(gd, max_nodes, max_edges, reduce);
        match format {
            OutputFormat::Json => {
                println!(
                    "{}",
                    serde_json::to_string(&processed).unwrap_or_else(|_| output.clone())
                );
            }
            OutputFormat::Dot => {
                let opts = DotOpts {
                    include_orphans: false,
                    include_tests,
                    output_file: None,
                    format: VisualOutputFormat::Dot,
                };
                println!("{}", visual::to_dot(&processed, &opts));
            }
            OutputFormat::Mermaid => {
                let opts = MermaidOpts {
                    include_orphans: false,
                    include_tests,
                    output_file: None,
                    format: VisualOutputFormat::Mermaid,
                };
                println!("{}", visual::to_mermaid(&processed, &opts));
            }
            OutputFormat::Svg => {
                let opts = DotOpts {
                    include_orphans: false,
                    include_tests,
                    output_file: None,
                    format: VisualOutputFormat::Dot,
                };
                let dot_output = visual::to_dot(&processed, &opts);
                match run_dot_to_svg(&dot_output) {
                    Ok(svg_output) => println!("{}", svg_output),
                    Err(_) => {
                        eprintln!(
                            "warning: graphviz 'dot' not available, outputting DOT format instead"
                        );
                        println!("{}", dot_output);
                    }
                }
            }
        }
        return Ok(());
    }

    handle_format(&output, format, max_nodes, max_edges, reduce, include_tests)
}

/// Clap CLI struct for `touring graph`.
#[derive(Parser, Debug)]
#[command(
    name = "touring graph",
    bin_name = "touring graph",
    about = "Graph analysis and topology queries (file, god-nodes, shortest-path, communities, flow, rename)",
    disable_help_subcommand = true
)]
struct GraphCli {
    #[command(subcommand)]
    cmd: Option<GraphCmd>,
}

/// Subcommands for `touring graph`.
#[derive(Subcommand, Debug)]
enum GraphCmd {
    /// Show dependency graph for a file (alias: blast).
    File {
        /// Target file path.
        path: String,
        /// Output format: json, dot, mermaid, svg (default: json).
        #[arg(long, default_value = "json")]
        format: String,
        /// Cap graph to N nodes (default: 200).
        #[arg(long, default_value_t = 200)]
        max_nodes: usize,
        /// Cap graph to M edges (default: 500).
        #[arg(long, default_value_t = 500)]
        max_edges: usize,
        /// Apply transitive reduction (DAGs only).
        #[arg(long)]
        reduce: bool,
        /// Include test files in output.
        #[arg(long)]
        include_tests: bool,
    },
    /// Identify highly-connected god nodes.
    #[command(name = "god-nodes")]
    GodNodes {
        /// Number of top nodes to return (default: 10).
        #[arg(long, default_value_t = 10)]
        top: i64,
        /// Output format: json, dot, mermaid, svg (default: json).
        #[arg(long, default_value = "json")]
        format: String,
        /// Cap graph to N nodes (default: 200).
        #[arg(long, default_value_t = 200)]
        max_nodes: usize,
        /// Cap graph to M edges (default: 500).
        #[arg(long, default_value_t = 500)]
        max_edges: usize,
        /// Apply transitive reduction (DAGs only).
        #[arg(long)]
        reduce: bool,
        /// Include test files in output.
        #[arg(long)]
        include_tests: bool,
    },
    /// Find shortest path between two files.
    #[command(name = "shortest-path")]
    ShortestPath {
        /// Source file path.
        from: String,
        /// Target file path.
        to: String,
        /// Output format: json, dot, mermaid, svg (default: json).
        #[arg(long, default_value = "json")]
        format: String,
        /// Cap graph to N nodes (default: 200).
        #[arg(long, default_value_t = 200)]
        max_nodes: usize,
        /// Cap graph to M edges (default: 500).
        #[arg(long, default_value_t = 500)]
        max_edges: usize,
        /// Apply transitive reduction (DAGs only).
        #[arg(long)]
        reduce: bool,
        /// Include test files in output.
        #[arg(long)]
        include_tests: bool,
    },
    /// Show module community graph.
    Communities {
        /// Output format: json, dot, mermaid, svg (default: json).
        #[arg(long, default_value = "json")]
        format: String,
        /// Cap graph to N nodes (default: 200).
        #[arg(long, default_value_t = 200)]
        max_nodes: usize,
        /// Cap graph to M edges (default: 500).
        #[arg(long, default_value_t = 500)]
        max_edges: usize,
        /// Apply transitive reduction (DAGs only).
        #[arg(long)]
        reduce: bool,
        /// Include test files in output.
        #[arg(long)]
        include_tests: bool,
    },
    /// Trace data flow paths between two symbols.
    Flow {
        /// Source symbol or file.
        from: String,
        /// Destination symbol or file.
        to: String,
        /// Maximum number of paths (default: 10).
        #[arg(long, default_value_t = 10)]
        max_paths: usize,
        /// Maximum search depth (default: 8).
        #[arg(long, default_value_t = 8)]
        max_depth: usize,
        /// Output format: json, dot, mermaid, svg (default: json).
        #[arg(long, default_value = "json")]
        format: String,
        /// Cap graph to N nodes (default: 200).
        #[arg(long, default_value_t = 200)]
        max_nodes: usize,
        /// Cap graph to M edges (default: 500).
        #[arg(long, default_value_t = 500)]
        max_edges: usize,
        /// Apply transitive reduction (DAGs only).
        #[arg(long)]
        reduce: bool,
        /// Include test files in output.
        #[arg(long)]
        include_tests: bool,
    },
    /// Rename a symbol at the given location.
    Rename {
        /// Source location: file, file:line, or file:line:col.
        from: String,
        /// New name for the symbol.
        to: String,
        /// Preview changes without applying.
        #[arg(long)]
        dry_run: bool,
        /// Apply the rename.
        #[arg(long)]
        apply: bool,
        /// Output format: json, dot, mermaid, svg (default: json).
        #[arg(long, default_value = "json")]
        format: String,
        /// Cap graph to N nodes (default: 200).
        #[arg(long, default_value_t = 200)]
        max_nodes: usize,
        /// Cap graph to M edges (default: 500).
        #[arg(long, default_value_t = 500)]
        max_edges: usize,
        /// Apply transitive reduction (DAGs only).
        #[arg(long)]
        reduce: bool,
        /// Include test files in output.
        #[arg(long)]
        include_tests: bool,
    },
}

/// Parse output format string into `OutputFormat`.
fn parse_format_str(s: &str) -> OutputFormat {
    s.parse::<OutputFormat>().unwrap_or(OutputFormat::Json)
}

/// Extract the format flag from a `GraphCmd` variant.
fn cmd_format(cmd: &GraphCmd) -> OutputFormat {
    let s = match cmd {
        GraphCmd::File { format, .. } => format.as_str(),
        GraphCmd::GodNodes { format, .. } => format.as_str(),
        GraphCmd::ShortestPath { format, .. } => format.as_str(),
        GraphCmd::Communities { format, .. } => format.as_str(),
        GraphCmd::Flow { format, .. } => format.as_str(),
        GraphCmd::Rename { format, .. } => format.as_str(),
    };
    parse_format_str(s)
}

/// Entry point for the `touring graph` CLI handler — parses the subcommand
/// (`file`, `god-nodes`, `shortest-path`, `communities`, `flow`, `rename`),
/// defaulting to an empty-path `file` query, then dispatches to the daemon and
/// prints the result in the requested output format.
pub fn run(args: &[String]) -> anyhow::Result<()> {
    let cli = match GraphCli::try_parse_from(args.iter().skip(1)) {
        Ok(c) => c,
        Err(e) => e.exit(),
    };

    let cmd = cli.cmd.unwrap_or(GraphCmd::File {
        path: String::new(),
        format: "json".to_string(),
        max_nodes: 200,
        max_edges: 500,
        reduce: false,
        include_tests: false,
    });

    let format = cmd_format(&cmd);
    dispatch_graph_cmd(&cmd, format)
}

/// Expose this handler's clap Command for the completions aggregator (W7).
pub(super) fn command() -> clap::Command {
    use clap::CommandFactory;
    GraphCli::command()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_output_format_parsing() {
        assert_eq!("json".parse::<OutputFormat>().unwrap(), OutputFormat::Json);
        assert_eq!("dot".parse::<OutputFormat>().unwrap(), OutputFormat::Dot);
        assert_eq!(
            "mermaid".parse::<OutputFormat>().unwrap(),
            OutputFormat::Mermaid
        );
        assert_eq!(
            "Mermaid".parse::<OutputFormat>().unwrap(),
            OutputFormat::Mermaid
        );
        assert!("unknown".parse::<OutputFormat>().is_err());
    }

    #[test]
    fn test_shortest_path_to_graph_basic() {
        // After Wave 2026-05-14 enrichment, shortest_path_to_graph emits a
        // root node for `file_path` plus one node per consumer with a
        // directed edge from root → consumer (kind="blast"). The previous
        // assertion shape `nodes.len() == 2` was for the pre-enriched
        // schema where consumers were the entire node set.
        let json = r#"{
            "blast_radius": 2,
            "consumers": ["foo.rs", "bar.rs"],
            "file_path": "src/lib.rs",
            "coedit_files": [],
            "diagnostics": []
        }"#;
        let graph = shortest_path_to_graph(json).unwrap();
        assert_eq!(graph.nodes.len(), 3, "expected root + 2 consumers");
        assert_eq!(graph.edges.len(), 2, "expected 2 blast edges from root");
        // Root node first (file_path), then consumers in input order.
        assert_eq!(graph.nodes[0].id, "src/lib.rs");
        assert!(graph.nodes[0].label.contains("blast=2"));
        assert_eq!(graph.nodes[1].id, "foo.rs");
        assert_eq!(graph.nodes[2].label, "bar.rs");
        // Edges originate from root.
        assert!(graph.edges.iter().all(|e| e.from == "src/lib.rs"));
        assert!(graph.edges.iter().all(|e| e.kind == "blast"));
    }

    #[test]
    fn test_shortest_path_to_graph_empty_consumers() {
        // With empty consumers, blast_radius=0, and a non-empty file_path,
        // the enriched schema still emits the root node (so the graph has
        // a single anchor for downstream tooling). Edges remain empty.
        let json = r#"{
            "blast_radius": 0,
            "consumers": [],
            "file_path": "src/lib.rs",
            "coedit_files": [],
            "diagnostics": []
        }"#;
        let graph = shortest_path_to_graph(json).unwrap();
        assert_eq!(graph.nodes.len(), 1, "expected root node only");
        assert_eq!(graph.nodes[0].id, "src/lib.rs");
        assert!(graph.edges.is_empty());
    }

    #[test]
    fn test_shortest_path_to_graph_truly_empty_when_no_file_path() {
        // When file_path is empty AND consumers are empty, no nodes emit.
        let json = r#"{
            "blast_radius": 0,
            "consumers": [],
            "file_path": "",
            "coedit_files": [],
            "diagnostics": []
        }"#;
        let graph = shortest_path_to_graph(json).unwrap();
        assert!(graph.nodes.is_empty());
        assert!(graph.edges.is_empty());
    }

    #[test]
    fn test_shortest_path_to_graph_invalid_json() {
        let json = "not valid json";
        assert!(shortest_path_to_graph(json).is_err());
    }

    #[test]
    fn test_shortest_path_to_graph_is_test_detection() {
        // After enrichment: nodes[0] is root (src/lib.rs, never a test),
        // nodes[1..] are consumers — same detection rules as before.
        let json = r#"{
            "blast_radius": 3,
            "consumers": ["normal.rs", "my_test.rs", "tests/integration.rs"],
            "file_path": "src/lib.rs",
            "coedit_files": [],
            "diagnostics": []
        }"#;
        let graph = shortest_path_to_graph(json).unwrap();
        assert_eq!(graph.nodes.len(), 4); // root + 3 consumers
        assert!(
            !graph.nodes[0].is_test,
            "root should never be flagged is_test"
        );
        assert!(!graph.nodes[1].is_test, "normal.rs is not a test");
        assert!(graph.nodes[2].is_test, "my_test.rs is a test");
        assert!(graph.nodes[3].is_test, "tests/integration.rs is a test");
    }

    #[test]
    fn test_shortest_path_to_graph_coedit_and_diagnostics() {
        // New schema also surfaces coedit_files and diagnostics. coedit
        // files produce `kind="coedit"` edges; diagnostics collapse into
        // a single annotation node with `kind="diagnostic"` edge.
        let json = r#"{
            "blast_radius": 1,
            "consumers": ["consumer.rs"],
            "file_path": "src/lib.rs",
            "coedit_files": ["sibling.rs"],
            "diagnostics": [{"severity": "warn", "message": "x"}]
        }"#;
        let graph = shortest_path_to_graph(json).unwrap();
        // root + 1 consumer + 1 coedit + 1 diagnostics annotation
        assert_eq!(graph.nodes.len(), 4);
        let kinds: std::collections::BTreeSet<&str> =
            graph.edges.iter().map(|e| e.kind.as_str()).collect();
        assert!(kinds.contains("blast"));
        assert!(kinds.contains("coedit"));
        assert!(kinds.contains("diagnostic"));
        assert!(
            graph
                .nodes
                .iter()
                .any(|n| n.id.starts_with("__diagnostics__:"))
        );
    }

    // ── clap-derive tests ────────────────────────────────────────────────────

    #[test]
    fn clap_graph_file_subcommand_parses_path() {
        let args: Vec<String> = vec!["graph".into(), "file".into(), "src/lib.rs".into()];
        let cli = GraphCli::try_parse_from(&args).unwrap();
        match cli.cmd.unwrap() {
            GraphCmd::File { path, .. } => assert_eq!(path, "src/lib.rs"),
            _ => panic!("expected File"),
        }
    }

    #[test]
    fn clap_graph_file_default_format_is_json() {
        let args: Vec<String> = vec!["graph".into(), "file".into(), "src/lib.rs".into()];
        let cli = GraphCli::try_parse_from(&args).unwrap();
        match cli.cmd.unwrap() {
            GraphCmd::File {
                format,
                max_nodes,
                max_edges,
                reduce,
                include_tests,
                ..
            } => {
                assert_eq!(format, "json");
                assert_eq!(max_nodes, 200);
                assert_eq!(max_edges, 500);
                assert!(!reduce);
                assert!(!include_tests);
            }
            _ => panic!("expected File"),
        }
    }

    #[test]
    fn clap_graph_file_format_dot() {
        let args: Vec<String> = vec![
            "graph".into(),
            "file".into(),
            "src/lib.rs".into(),
            "--format".into(),
            "dot".into(),
        ];
        let cli = GraphCli::try_parse_from(&args).unwrap();
        match cli.cmd.unwrap() {
            GraphCmd::File { format, .. } => assert_eq!(format, "dot"),
            _ => panic!("expected File"),
        }
    }

    #[test]
    fn clap_graph_god_nodes_default_top() {
        let args: Vec<String> = vec!["graph".into(), "god-nodes".into()];
        let cli = GraphCli::try_parse_from(&args).unwrap();
        match cli.cmd.unwrap() {
            GraphCmd::GodNodes { top, .. } => assert_eq!(top, 10),
            _ => panic!("expected GodNodes"),
        }
    }

    #[test]
    fn clap_graph_god_nodes_custom_top() {
        let args: Vec<String> = vec![
            "graph".into(),
            "god-nodes".into(),
            "--top".into(),
            "20".into(),
        ];
        let cli = GraphCli::try_parse_from(&args).unwrap();
        match cli.cmd.unwrap() {
            GraphCmd::GodNodes { top, .. } => assert_eq!(top, 20),
            _ => panic!("expected GodNodes"),
        }
    }

    #[test]
    fn clap_graph_shortest_path_positionals() {
        let args: Vec<String> = vec![
            "graph".into(),
            "shortest-path".into(),
            "a.rs".into(),
            "b.rs".into(),
        ];
        let cli = GraphCli::try_parse_from(&args).unwrap();
        match cli.cmd.unwrap() {
            GraphCmd::ShortestPath { from, to, .. } => {
                assert_eq!(from, "a.rs");
                assert_eq!(to, "b.rs");
            }
            _ => panic!("expected ShortestPath"),
        }
    }

    #[test]
    fn clap_graph_communities_defaults() {
        let args: Vec<String> = vec!["graph".into(), "communities".into()];
        let cli = GraphCli::try_parse_from(&args).unwrap();
        match cli.cmd.unwrap() {
            GraphCmd::Communities {
                format,
                max_nodes,
                max_edges,
                ..
            } => {
                assert_eq!(format, "json");
                assert_eq!(max_nodes, 200);
                assert_eq!(max_edges, 500);
            }
            _ => panic!("expected Communities"),
        }
    }

    #[test]
    fn clap_graph_flow_defaults() {
        let args: Vec<String> = vec![
            "graph".into(),
            "flow".into(),
            "from.rs".into(),
            "to.rs".into(),
        ];
        let cli = GraphCli::try_parse_from(&args).unwrap();
        match cli.cmd.unwrap() {
            GraphCmd::Flow {
                from,
                to,
                max_paths,
                max_depth,
                ..
            } => {
                assert_eq!(from, "from.rs");
                assert_eq!(to, "to.rs");
                assert_eq!(max_paths, 10);
                assert_eq!(max_depth, 8);
            }
            _ => panic!("expected Flow"),
        }
    }

    #[test]
    fn clap_graph_flow_custom_max_paths() {
        let args: Vec<String> = vec![
            "graph".into(),
            "flow".into(),
            "a.rs".into(),
            "b.rs".into(),
            "--max-paths".into(),
            "5".into(),
            "--max-depth".into(),
            "4".into(),
        ];
        let cli = GraphCli::try_parse_from(&args).unwrap();
        match cli.cmd.unwrap() {
            GraphCmd::Flow {
                max_paths,
                max_depth,
                ..
            } => {
                assert_eq!(max_paths, 5);
                assert_eq!(max_depth, 4);
            }
            _ => panic!("expected Flow"),
        }
    }

    #[test]
    fn clap_graph_rename_positionals() {
        let args: Vec<String> = vec![
            "graph".into(),
            "rename".into(),
            "src/lib.rs:42:5".into(),
            "new_name".into(),
        ];
        let cli = GraphCli::try_parse_from(&args).unwrap();
        match cli.cmd.unwrap() {
            GraphCmd::Rename {
                from,
                to,
                dry_run,
                apply,
                ..
            } => {
                assert_eq!(from, "src/lib.rs:42:5");
                assert_eq!(to, "new_name");
                assert!(!dry_run);
                assert!(!apply);
            }
            _ => panic!("expected Rename"),
        }
    }

    #[test]
    fn clap_graph_rename_apply_flag() {
        let args: Vec<String> = vec![
            "graph".into(),
            "rename".into(),
            "src/lib.rs".into(),
            "foo".into(),
            "--apply".into(),
        ];
        let cli = GraphCli::try_parse_from(&args).unwrap();
        match cli.cmd.unwrap() {
            GraphCmd::Rename { apply, .. } => assert!(apply),
            _ => panic!("expected Rename"),
        }
    }

    #[test]
    fn clap_graph_reduce_flag() {
        let args: Vec<String> = vec![
            "graph".into(),
            "file".into(),
            "src/lib.rs".into(),
            "--reduce".into(),
        ];
        let cli = GraphCli::try_parse_from(&args).unwrap();
        match cli.cmd.unwrap() {
            GraphCmd::File { reduce, .. } => assert!(reduce),
            _ => panic!("expected File"),
        }
    }

    #[test]
    fn clap_graph_include_tests_flag() {
        let args: Vec<String> = vec![
            "graph".into(),
            "file".into(),
            "src/lib.rs".into(),
            "--include-tests".into(),
        ];
        let cli = GraphCli::try_parse_from(&args).unwrap();
        match cli.cmd.unwrap() {
            GraphCmd::File { include_tests, .. } => assert!(include_tests),
            _ => panic!("expected File"),
        }
    }

    #[test]
    fn clap_graph_max_nodes_and_edges() {
        let args: Vec<String> = vec![
            "graph".into(),
            "file".into(),
            "src/lib.rs".into(),
            "--max-nodes".into(),
            "50".into(),
            "--max-edges".into(),
            "100".into(),
        ];
        let cli = GraphCli::try_parse_from(&args).unwrap();
        match cli.cmd.unwrap() {
            GraphCmd::File {
                max_nodes,
                max_edges,
                ..
            } => {
                assert_eq!(max_nodes, 50);
                assert_eq!(max_edges, 100);
            }
            _ => panic!("expected File"),
        }
    }

    #[test]
    fn parse_format_str_json() {
        assert_eq!(parse_format_str("json"), OutputFormat::Json);
        assert_eq!(parse_format_str("dot"), OutputFormat::Dot);
        assert_eq!(parse_format_str("mermaid"), OutputFormat::Mermaid);
        assert_eq!(parse_format_str("svg"), OutputFormat::Svg);
        // unknown falls back to Json
        assert_eq!(parse_format_str("bogus"), OutputFormat::Json);
    }
}
