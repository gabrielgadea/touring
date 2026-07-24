//! `touring incremental status` — Incremental parser cache status.
//!
//! Queries the daemon for tree-sitter incremental cache hit/miss rates.
//!
//! Wave P3-1.3 W6a (2026-06-11): migrated from manual `arg_or` positional
//! parsing to clap derive. The `run` signature is unchanged — receives full
//! argv slice; clap parses `args[1..]`.

use super::daemon_query;
use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "touring incremental",
    bin_name = "touring incremental",
    about = "Incremental parser cache status",
    disable_help_subcommand = true
)]
struct IncrementalCli {
    #[command(subcommand)]
    cmd: Option<IncrementalCmd>,
}

#[derive(Subcommand, Debug)]
enum IncrementalCmd {
    /// Show tree-sitter incremental cache hit/miss rates (default).
    Status,
}

/// Entry point for the `touring incremental` CLI handler — parses the argv
/// slice and queries the `cli-incremental-status` daemon hook for tree-sitter
/// incremental parser cache hit/miss rates (`status` is the only and default
/// subcommand), printing the JSON response.
pub fn run(args: &[String]) -> anyhow::Result<()> {
    let cli = match IncrementalCli::try_parse_from(args.iter().skip(1)) {
        Ok(cli) => cli,
        Err(e) => e.exit(),
    };

    match cli.cmd.unwrap_or(IncrementalCmd::Status) {
        IncrementalCmd::Status => {
            let output = daemon_query("cli-incremental-status", serde_json::json!({}))?;
            println!("{output}");
        }
    }
    Ok(())
}

/// Expose this handler's clap Command for the completions aggregator (W7).
pub(super) fn command() -> clap::Command {
    use clap::CommandFactory;
    IncrementalCli::command()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|p| p.to_string()).collect()
    }

    #[test]
    fn defaults_to_status_when_no_subcommand() {
        let cli = IncrementalCli::try_parse_from(s(&["incremental"]).iter()).unwrap();
        assert!(matches!(
            cli.cmd.unwrap_or(IncrementalCmd::Status),
            IncrementalCmd::Status
        ));
    }

    #[test]
    fn explicit_status_subcommand_parses() {
        let cli = IncrementalCli::try_parse_from(s(&["incremental", "status"]).iter()).unwrap();
        assert!(matches!(cli.cmd.unwrap(), IncrementalCmd::Status));
    }

    #[test]
    fn unknown_subcommand_is_rejected() {
        let result = IncrementalCli::try_parse_from(s(&["incremental", "bogus"]).iter());
        assert!(result.is_err());
    }
}
