//! `touring evolution drift|insights|tools` — Evolution analyzer status.
//!
//! Queries the daemon for drift detection, pattern insights, and tool effectiveness.
//!
//! Wave P3-1.3 W3 (2026-06-11): migrated from manual positional parsing
//! (`arg_or`) to clap derive. Dispatch contract unchanged — `run` still
//! receives the full argv slice; clap parses `args[1..]`.

use super::daemon_query;
use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "touring evolution",
    bin_name = "touring evolution",
    about = "Evolution analyzer: drift detection, pattern insights, tool effectiveness",
    disable_help_subcommand = true
)]
struct EvolutionCli {
    #[command(subcommand)]
    cmd: Option<EvolutionCmd>,
}

#[derive(Subcommand, Debug)]
enum EvolutionCmd {
    /// Detect quality/metric drift across the codebase.
    Drift,
    /// Summarize learned pattern insights.
    Insights,
    /// Report per-tool effectiveness from RL history.
    Tools,
}

/// Entry point for the `touring evolution` CLI handler — parses the argv slice
/// and dispatches `drift` (default), `insights`, or `tools` to the matching
/// `cli-evolution-*` daemon hook, printing the JSON response.
pub fn run(args: &[String]) -> anyhow::Result<()> {
    let cli = match EvolutionCli::try_parse_from(args.iter().skip(1)) {
        Ok(cli) => cli,
        Err(e) => e.exit(),
    };

    let hook = match cli.cmd.unwrap_or(EvolutionCmd::Drift) {
        EvolutionCmd::Drift => "cli-evolution-drift",
        EvolutionCmd::Insights => "cli-evolution-insights",
        EvolutionCmd::Tools => "cli-evolution-tools",
    };

    let output = daemon_query(hook, serde_json::json!({}))?;
    println!("{output}");
    Ok(())
}

/// Expose this handler's clap Command for the completions aggregator (W7).
pub(super) fn command() -> clap::Command {
    use clap::CommandFactory;
    EvolutionCli::command()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> EvolutionCli {
        EvolutionCli::try_parse_from(args).expect("args should parse")
    }

    #[test]
    fn bare_evolution_defaults_to_drift() {
        let cli = parse(&["evolution"]);
        assert!(cli.cmd.is_none()); // run() maps None -> Drift
    }

    #[test]
    fn drift_subcommand_parses() {
        let cli = parse(&["evolution", "drift"]);
        assert!(matches!(cli.cmd, Some(EvolutionCmd::Drift)));
    }

    #[test]
    fn insights_subcommand_parses() {
        let cli = parse(&["evolution", "insights"]);
        assert!(matches!(cli.cmd, Some(EvolutionCmd::Insights)));
    }

    #[test]
    fn tools_subcommand_parses() {
        let cli = parse(&["evolution", "tools"]);
        assert!(matches!(cli.cmd, Some(EvolutionCmd::Tools)));
    }

    #[test]
    fn unknown_subcommand_is_parse_error() {
        assert!(EvolutionCli::try_parse_from(["evolution", "frobnicate"]).is_err());
    }
}
