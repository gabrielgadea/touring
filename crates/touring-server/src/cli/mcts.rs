//! `touring mcts search` — Monte Carlo Tree Search for multi-path decisions.
//!
//! Uses MCTS reasoning to explore decision trees and select optimal actions.
//!
//! Wave P3-1.3 W3 (2026-06-11): migrated from manual positional parsing
//! (`arg_or`) to clap derive. When no `--state` flag is given and no
//! positional text is provided, reads from stdin (original behaviour).

use super::daemon_query;
use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "touring mcts",
    bin_name = "touring mcts",
    about = "Monte Carlo Tree Search for multi-path decision making",
    disable_help_subcommand = true
)]
struct MctsCli {
    #[command(subcommand)]
    cmd: Option<MctsCmd>,
}

#[derive(Subcommand, Debug)]
enum MctsCmd {
    /// Run MCTS search from a root state description (default).
    Search {
        /// Root state description (remaining positional args, joined).
        /// When empty, root state is read from stdin.
        root_state: Vec<String>,
    },
}

/// Entry point for the `touring mcts` CLI handler — parses the `search`
/// subcommand (the default), joins the root-state positional args (or reads
/// the root state from stdin when none are given), dispatches the
/// `cli-mcts-search` daemon query, and prints the response.
pub fn run(args: &[String]) -> anyhow::Result<()> {
    let cli = match MctsCli::try_parse_from(args.iter().skip(1)) {
        Ok(cli) => cli,
        Err(e) => e.exit(),
    };

    match cli.cmd.unwrap_or(MctsCmd::Search {
        root_state: Vec::new(),
    }) {
        MctsCmd::Search { root_state } => {
            let joined = root_state.join(" ");
            let payload = if joined.is_empty() {
                super::read_stdin_safe()
            } else {
                serde_json::json!({ "root_state": joined })
            };
            let output = daemon_query("cli-mcts-search", payload)?;
            println!("{output}");
        }
    }
    Ok(())
}

/// Expose this handler's clap Command for the completions aggregator (W7).
pub(super) fn command() -> clap::Command {
    use clap::CommandFactory;
    MctsCli::command()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> MctsCli {
        MctsCli::try_parse_from(args).expect("args should parse")
    }

    #[test]
    fn bare_mcts_defaults_to_search_empty() {
        let cli = parse(&["mcts"]);
        assert!(cli.cmd.is_none()); // run() maps None -> Search { root_state: [] }
    }

    #[test]
    fn search_subcommand_captures_positionals() {
        let cli = parse(&["mcts", "search", "explore", "the", "decision", "space"]);
        let Some(MctsCmd::Search { root_state }) = cli.cmd else {
            panic!("expected Search");
        };
        assert_eq!(root_state.join(" "), "explore the decision space");
    }

    #[test]
    fn search_with_empty_state_parses() {
        let cli = parse(&["mcts", "search"]);
        let Some(MctsCmd::Search { root_state }) = cli.cmd else {
            panic!("expected Search");
        };
        assert!(root_state.is_empty()); // triggers stdin path
    }

    #[test]
    fn unknown_subcommand_is_parse_error() {
        assert!(MctsCli::try_parse_from(["mcts", "frobnicate"]).is_err());
    }
}
