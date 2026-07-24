//! `touring viz` — Rich visual encoding of workspace graphs.
//!
//! Provides 6 subcommands that emit DOT/Mermaid/SVG for different graph views.
//! Each subcommand delegates to the daemon via a named hook and prints the
//! raw output (DOT/Mermaid/SVG/JSON depending on format flag).
//!
//! Wave P3-1.3 W5a (2026-06-11): migrated from manual `arg_or` positional
//! parsing to clap derive. The dispatch contract is unchanged — `run` still
//! receives the full argv slice; clap parses `args[1..]` so the subcommand
//! name ("viz") acts as the program name.
//!
//! G6 payload byte-compatible: every `daemon_query` call keeps exactly the
//! same hook name and JSON keys as the original `arg_or` implementation.

use super::daemon_query;
use crate::visual::{self, DotOpts, GraphData, MermaidOpts, OutputFormat};
use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "touring viz",
    bin_name = "touring viz",
    about = "Visual encoding of workspace graphs: workspace (default), blast, wiring, cycles, orphans, feature",
    disable_help_subcommand = true
)]
struct VizCli {
    /// Output format: dot, mermaid, svg, json (default: dot).
    #[arg(long, default_value = "dot", global = true)]
    format: String,

    /// Include orphan nodes in output.
    #[arg(long, global = true)]
    include_orphans: bool,

    /// Include test files in output.
    #[arg(long, global = true)]
    include_tests: bool,

    #[command(subcommand)]
    cmd: Option<VizCmd>,
}

#[derive(Subcommand, Debug)]
enum VizCmd {
    /// Full crate dependency graph (default).
    Workspace,
    /// Blast radius for a symbol.
    Blast {
        /// Symbol name to analyse.
        #[arg(default_value = "")]
        symbol: String,
    },
    /// Wiring / consumer graph.
    Wiring,
    /// Dependency cycles.
    Cycles,
    /// Orphan symbols only (include_orphans defaults to true for this subcommand).
    Orphans,
    /// Symbols gated by a feature flag.
    Feature {
        /// Feature name to filter by.
        #[arg(default_value = "")]
        feature: String,
    },
}

/// Run the `touring viz` subcommand dispatcher.
///
/// # Errors
///
/// Returns an error if the daemon socket is unreachable or the daemon reports
/// a failure.
pub fn run(args: &[String]) -> anyhow::Result<()> {
    let cli = match VizCli::try_parse_from(args.iter().skip(1)) {
        Ok(cli) => cli,
        Err(e) => e.exit(),
    };

    let format = cli.format.as_str();
    let include_tests = cli.include_tests;

    // `orphans` inherits include_orphans=true unless the user overrode the flag.
    let cmd = cli.cmd.unwrap_or(VizCmd::Workspace);
    let include_orphans = match &cmd {
        VizCmd::Orphans => true,
        _ => cli.include_orphans,
    };

    let opts = DotOpts {
        include_orphans,
        include_tests,
        output_file: None,
        format: OutputFormat::Dot,
    };

    let (hook, payload) = match &cmd {
        VizCmd::Workspace => (
            "cli-viz-workspace",
            serde_json::json!({
                "format": format,
                "include_orphans": include_orphans,
                "include_tests": include_tests
            }),
        ),
        VizCmd::Blast { symbol } => (
            "cli-viz-blast",
            serde_json::json!({
                "symbol": symbol,
                "format": format,
                "include_orphans": include_orphans,
                "include_tests": include_tests
            }),
        ),
        VizCmd::Wiring => (
            "cli-viz-wiring",
            serde_json::json!({
                "format": format,
                "include_orphans": include_orphans,
                "include_tests": include_tests
            }),
        ),
        VizCmd::Cycles => (
            "cli-viz-cycles",
            serde_json::json!({
                "min_depth": 2,
                "format": format,
                "include_orphans": include_orphans,
                "include_tests": include_tests
            }),
        ),
        VizCmd::Orphans => (
            "cli-viz-orphans",
            serde_json::json!({
                "format": format,
                "include_orphans": include_orphans,
                "include_tests": include_tests
            }),
        ),
        VizCmd::Feature { feature } => (
            "cli-viz-feature",
            serde_json::json!({
                "feature": feature,
                "format": format,
                "include_orphans": include_orphans,
                "include_tests": include_tests
            }),
        ),
    };

    let output = daemon_query(hook, payload)?;
    if let Ok(graph) = serde_json::from_str::<GraphData>(&output) {
        emit_graph(&graph, format, &opts);
    } else {
        println!("{}", output);
    }
    Ok(())
}

/// Emit graph in the requested format.
///
/// ## SVG Architecture (Intentional Design)
/// The daemon returns raw GraphData as JSON; SVG conversion is performed
/// locally in the CLI via `dot_pipe_svg()`. This keeps the daemon host
/// free of graphviz dependencies while allowing the CLI to fall back to
/// DOT output when `dot` is unavailable on the client side.
fn emit_graph(graph: &GraphData, format: &str, opts: &DotOpts) {
    match format {
        "dot" => println!("{}", visual::to_dot(graph, opts)),
        "svg" => {
            let dot_output = visual::to_dot(graph, opts);
            if let Some(svg) = visual::dot_pipe_svg(&dot_output) {
                println!("{}", svg);
            } else {
                eprintln!("Warning: graphviz 'dot' not available, falling back to DOT");
                println!("{}", dot_output);
            }
        }
        "mermaid" => {
            let mermaid_opts = MermaidOpts {
                include_orphans: opts.include_orphans,
                include_tests: opts.include_tests,
                output_file: opts.output_file.clone(),
                format: OutputFormat::Mermaid,
            };
            println!("{}", visual::to_mermaid(graph, &mermaid_opts));
        }
        "json" => println!("{}", serde_json::to_string(graph).unwrap_or_default()),
        _ => println!("{}", visual::to_dot(graph, opts)),
    }
}

/// Expose this handler's clap Command for the completions aggregator (W7).
pub(super) fn command() -> clap::Command {
    use clap::CommandFactory;
    VizCli::command()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> VizCli {
        VizCli::try_parse_from(args).expect("args should parse")
    }

    // ── defaults ──────────────────────────────────────────────────────────────

    #[test]
    fn bare_viz_defaults_to_workspace() {
        let cli = parse(&["viz"]);
        assert!(cli.cmd.is_none()); // run() maps None -> Workspace
        assert_eq!(cli.format, "dot");
        assert!(!cli.include_orphans);
        assert!(!cli.include_tests);
    }

    #[test]
    fn explicit_workspace_subcommand() {
        let cli = parse(&["viz", "workspace"]);
        assert!(matches!(cli.cmd, Some(VizCmd::Workspace)));
    }

    // ── subcommand parsing ────────────────────────────────────────────────────

    #[test]
    fn blast_parses_symbol() {
        let cli = parse(&["viz", "blast", "MySymbol"]);
        let Some(VizCmd::Blast { symbol }) = cli.cmd else {
            panic!("expected Blast");
        };
        assert_eq!(symbol, "MySymbol");
    }

    #[test]
    fn blast_defaults_symbol_to_empty() {
        let cli = parse(&["viz", "blast"]);
        let Some(VizCmd::Blast { symbol }) = cli.cmd else {
            panic!("expected Blast");
        };
        assert_eq!(symbol, "");
    }

    #[test]
    fn feature_parses_feature_name() {
        let cli = parse(&["viz", "feature", "simd-fuzzy"]);
        let Some(VizCmd::Feature { feature }) = cli.cmd else {
            panic!("expected Feature");
        };
        assert_eq!(feature, "simd-fuzzy");
    }

    #[test]
    fn feature_defaults_to_empty() {
        let cli = parse(&["viz", "feature"]);
        let Some(VizCmd::Feature { feature }) = cli.cmd else {
            panic!("expected Feature");
        };
        assert_eq!(feature, "");
    }

    #[test]
    fn wiring_subcommand_parses() {
        let cli = parse(&["viz", "wiring"]);
        assert!(matches!(cli.cmd, Some(VizCmd::Wiring)));
    }

    #[test]
    fn cycles_subcommand_parses() {
        let cli = parse(&["viz", "cycles"]);
        assert!(matches!(cli.cmd, Some(VizCmd::Cycles)));
    }

    #[test]
    fn orphans_subcommand_parses() {
        let cli = parse(&["viz", "orphans"]);
        assert!(matches!(cli.cmd, Some(VizCmd::Orphans)));
    }

    // ── flag parsing ──────────────────────────────────────────────────────────

    #[test]
    fn format_flag_dot() {
        let cli = parse(&["viz", "workspace", "--format", "dot"]);
        assert_eq!(cli.format, "dot");
    }

    #[test]
    fn format_flag_mermaid() {
        let cli = parse(&["viz", "--format", "mermaid", "workspace"]);
        assert_eq!(cli.format, "mermaid");
    }

    #[test]
    fn include_orphans_flag() {
        let cli = parse(&["viz", "workspace", "--include-orphans"]);
        assert!(cli.include_orphans);
    }

    #[test]
    fn include_tests_flag() {
        let cli = parse(&["viz", "workspace", "--include-tests"]);
        assert!(cli.include_tests);
    }

    #[test]
    fn global_flags_before_subcommand() {
        let cli = parse(&[
            "viz",
            "--include-orphans",
            "--include-tests",
            "--format",
            "json",
            "wiring",
        ]);
        assert!(cli.include_orphans);
        assert!(cli.include_tests);
        assert_eq!(cli.format, "json");
        assert!(matches!(cli.cmd, Some(VizCmd::Wiring)));
    }

    // ── error cases ───────────────────────────────────────────────────────────

    #[test]
    fn unknown_subcommand_is_parse_error() {
        assert!(VizCli::try_parse_from(["viz", "frobnicate"]).is_err());
    }
}

#[cfg(test)]
mod encoding_tests {
    use super::super::super::visual::encoding;

    #[test]
    fn test_edge_color_cross_feature() {
        assert_eq!(encoding::edge_color("cross_feature"), "#7b1fa2");
    }

    #[test]
    fn test_edge_color_optional() {
        assert_eq!(encoding::edge_color("optional"), "#9e9e9e");
    }

    #[test]
    fn test_quality_fillcolor_boundary_high() {
        assert_eq!(encoding::quality_fillcolor(Some(0.79)), "#fff59d");
    }

    #[test]
    fn test_quality_fillcolor_boundary_low() {
        assert_eq!(encoding::quality_fillcolor(Some(0.0)), "#ef9a9a");
    }

    #[test]
    fn test_node_shape_god_node_priority() {
        // god_node takes priority over orphan
        assert_eq!(encoding::node_shape(true, false, true), "diamond");
    }

    #[test]
    fn test_border_style_unsafe_overrides_feature() {
        // unsafe takes priority (double not dashed)
        assert_eq!(encoding::border_style(true, true), "double");
    }
}

#[cfg(test)]
mod theme_tests {
    use crate::visual::theme::Theme;

    #[test]
    fn test_theme_default_node_size() {
        let theme = Theme::default();
        assert_eq!(theme.node.size.base_font_size, 8);
        assert_eq!(theme.node.size.log_factor, 1.5);
        assert!(theme.node.size.min_size <= theme.node.size.max_size);
    }

    #[test]
    fn test_theme_default_cluster() {
        let theme = Theme::default();
        assert_eq!(theme.cluster.workspace_root, "lightblue");
        assert_eq!(theme.cluster.test_dir, "lightgrey");
    }

    #[test]
    fn test_theme_load_returns_default_on_invalid_path() {
        // Non-existent path should return default
        let theme = Theme::load();
        assert_eq!(theme.node.size.base_font_size, 8);
    }

    #[test]
    fn test_node_theme_has_required_fields() {
        let theme = Theme::default();
        assert!(theme.node.shape.is_empty());
        assert!(theme.node.fill.is_empty());
        assert!(theme.node.border.is_empty());
    }

    #[test]
    fn test_edge_theme_has_required_fields() {
        let theme = Theme::default();
        assert!(theme.edge.color.is_empty());
        assert!(theme.edge.style.is_empty());
    }
}
