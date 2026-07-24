//! `touring flywheel status|health` — Flywheel component health.
//!
//! Queries the daemon for flywheel status and component-level health.
//!
//! Wave P3-1.3 W3 (2026-06-11): migrated from manual positional parsing
//! (`arg_or`) to clap derive. Both `status` and `health` route to the same
//! daemon hook (original behaviour preserved).

use super::daemon_query;
use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "touring flywheel",
    bin_name = "touring flywheel",
    about = "Flywheel component health and status",
    disable_help_subcommand = true
)]
struct FlywheelCli {
    #[command(subcommand)]
    cmd: Option<FlywheelCmd>,
}

#[derive(Subcommand, Debug)]
enum FlywheelCmd {
    /// Query flywheel component status (default).
    Status,
    /// Alias for `status` — same daemon hook.
    Health,
}

/// Entry point for the `touring flywheel` CLI handler — parses the argv slice
/// and queries the `cli-flywheel-status` daemon hook for flywheel component
/// health, printing the JSON response. Both `status` (default) and its `health`
/// alias route to the same hook.
pub fn run(args: &[String]) -> anyhow::Result<()> {
    let cli = match FlywheelCli::try_parse_from(args.iter().skip(1)) {
        Ok(cli) => cli,
        Err(e) => e.exit(),
    };

    // Both variants send the same hook; `status` is the default.
    match cli.cmd.unwrap_or(FlywheelCmd::Status) {
        FlywheelCmd::Status | FlywheelCmd::Health => {
            let output = daemon_query("cli-flywheel-status", serde_json::json!({}))?;
            println!("{output}");
        }
    }
    Ok(())
}

/// Expose this handler's clap Command for the completions aggregator (W7).
pub(super) fn command() -> clap::Command {
    use clap::CommandFactory;
    FlywheelCli::command()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> FlywheelCli {
        FlywheelCli::try_parse_from(args).expect("args should parse")
    }

    #[test]
    fn bare_flywheel_defaults_to_status() {
        let cli = parse(&["flywheel"]);
        assert!(cli.cmd.is_none()); // run() maps None -> Status
    }

    #[test]
    fn status_subcommand_parses() {
        let cli = parse(&["flywheel", "status"]);
        assert!(matches!(cli.cmd, Some(FlywheelCmd::Status)));
    }

    #[test]
    fn health_subcommand_parses() {
        let cli = parse(&["flywheel", "health"]);
        assert!(matches!(cli.cmd, Some(FlywheelCmd::Health)));
    }

    #[test]
    fn unknown_subcommand_is_parse_error() {
        assert!(FlywheelCli::try_parse_from(["flywheel", "frobnicate"]).is_err());
    }
}
