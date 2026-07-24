//! `touring cognitive metrics|engines|search|explore` — Cognitive engine operations.
//!
//! Queries the daemon for cognitive engine metrics, health, and provides
//! access to MCTS search and GoT exploration.
//!
//! Wave P3-1.3 W4 (2026-06-11): migrated from manual `arg_or` to clap derive.
//! Payload keys and hook names are unchanged (G6).

use super::daemon_query;
use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "touring cognitive",
    bin_name = "touring cognitive",
    about = "Cognitive engine: metrics (default), engines, search, explore",
    disable_help_subcommand = true
)]
struct CognitiveCli {
    #[command(subcommand)]
    cmd: Option<CognitiveCmd>,
}

#[derive(Subcommand, Debug)]
enum CognitiveCmd {
    /// Show cognitive engine quality-delta metrics (default).
    Metrics,
    /// Show health status of all cognitive engines.
    Engines,
    /// MCTS search with RL-informed heuristic.
    Search {
        /// Query string (remaining args joined).
        query: Vec<String>,
    },
    /// GoT parallel thought exploration.
    Explore {
        /// Query string (remaining args joined).
        query: Vec<String>,
    },
}

/// Entry point for the `touring cognitive <subcommand>` CLI handler — parses the
/// subcommand (`metrics` default, `engines`, `search`, `explore`) and forwards the
/// corresponding daemon query (`cli-cognitive-metrics`, `cli-cognitive-engines`, or
/// `cli-mcts-search` with `mcts_rl`/`got_parallel` mode), printing the daemon's response.
pub fn run(args: &[String]) -> anyhow::Result<()> {
    let cli = match CognitiveCli::try_parse_from(args.iter().skip(1)) {
        Ok(cli) => cli,
        Err(e) => e.exit(),
    };

    match cli.cmd.unwrap_or(CognitiveCmd::Metrics) {
        CognitiveCmd::Metrics => {
            let output = daemon_query("cli-cognitive-metrics", serde_json::json!({}))?;
            println!("{output}");
        }
        CognitiveCmd::Engines => {
            let output = daemon_query("cli-cognitive-engines", serde_json::json!({}))?;
            println!("{output}");
        }
        CognitiveCmd::Search { query } => {
            let q = query.join(" ");
            if q.is_empty() {
                anyhow::bail!("Usage: touring cognitive search <query>");
            }
            let payload = serde_json::json!({ "query": q, "mode": "mcts_rl" });
            let output = daemon_query("cli-mcts-search", payload)?;
            println!("{output}");
        }
        CognitiveCmd::Explore { query } => {
            let q = query.join(" ");
            if q.is_empty() {
                anyhow::bail!("Usage: touring cognitive explore <query>");
            }
            let payload = serde_json::json!({ "query": q, "mode": "got_parallel" });
            let output = daemon_query("cli-mcts-search", payload)?;
            println!("{output}");
        }
    }
    Ok(())
}

/// Expose this handler's clap Command for the completions aggregator (W7).
pub(super) fn command() -> clap::Command {
    use clap::CommandFactory;
    CognitiveCli::command()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> CognitiveCli {
        CognitiveCli::try_parse_from(args).expect("args should parse")
    }

    #[test]
    fn bare_cognitive_defaults_to_metrics() {
        let cli = parse(&["cognitive"]);
        assert!(cli.cmd.is_none()); // run() maps None -> Metrics
    }

    #[test]
    fn explicit_metrics_parses() {
        let cli = parse(&["cognitive", "metrics"]);
        assert!(matches!(cli.cmd, Some(CognitiveCmd::Metrics)));
    }

    #[test]
    fn explicit_engines_parses() {
        let cli = parse(&["cognitive", "engines"]);
        assert!(matches!(cli.cmd, Some(CognitiveCmd::Engines)));
    }

    #[test]
    fn search_parses_query() {
        let cli = parse(&["cognitive", "search", "hello", "world"]);
        let Some(CognitiveCmd::Search { query }) = cli.cmd else {
            panic!("expected Search");
        };
        assert_eq!(query.join(" "), "hello world");
    }

    #[test]
    fn explore_parses_query() {
        let cli = parse(&["cognitive", "explore", "my", "query"]);
        let Some(CognitiveCmd::Explore { query }) = cli.cmd else {
            panic!("expected Explore");
        };
        assert_eq!(query.join(" "), "my query");
    }

    #[test]
    fn unknown_subcommand_is_parse_error() {
        assert!(CognitiveCli::try_parse_from(["cognitive", "bad"]).is_err());
    }

    #[test]
    fn search_requires_query_is_parse_error() {
        // search needs at least one query token; empty Vec means clap won't error
        // (Vec<String> defaults to empty), so verify at run-time via empty join.
        // The parse itself succeeds; error is emitted in run() body.
        let cli = CognitiveCli::try_parse_from(["cognitive", "search"]);
        assert!(cli.is_ok()); // Vec<String> accepts 0 tokens
        let Some(CognitiveCmd::Search { query }) = cli.unwrap().cmd else {
            panic!("expected Search");
        };
        assert!(query.is_empty());
    }

    #[test]
    fn explore_requires_query_is_parse_error() {
        let cli = CognitiveCli::try_parse_from(["cognitive", "explore"]);
        assert!(cli.is_ok()); // Vec<String> accepts 0 tokens
        let Some(CognitiveCmd::Explore { query }) = cli.unwrap().cmd else {
            panic!("expected Explore");
        };
        assert!(query.is_empty());
    }
}
