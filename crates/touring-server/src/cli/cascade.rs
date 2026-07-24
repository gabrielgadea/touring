//! `touring cascade queue|drain` — Wave C D5 (2026-04-20).
//!
//! Exposes the in-memory `CascadeQueue` that accumulates high-severity API
//! surface change proposals detected during `post_edit` analysis. The queue
//! is populated by `post_edit.rs` after each Rust edit and drained by the
//! `touring_decompose drain_cascades` MCP action.
//!
//! Subcommands:
//! - `queue` — show current queue length and empty status (default).
//! - `drain` — evict stale items and drain fresh proposals (returns JSON).
//!
//! Wave P3-1.3 W4 (2026-06-11): migrated from manual `arg_or` to clap derive.

use super::daemon_query;
use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "touring cascade",
    bin_name = "touring cascade",
    about = "CascadeQueue operations: queue (default), drain",
    disable_help_subcommand = true
)]
struct CascadeCli {
    #[command(subcommand)]
    cmd: Option<CascadeCmd>,
}

#[derive(Subcommand, Debug)]
enum CascadeCmd {
    /// Show current queue length and empty status (default).
    Queue,
    /// Evict stale items and drain fresh proposals.
    Drain,
}

/// Entry point for the `touring cascade …` subcommand.
pub fn run(args: &[String]) -> anyhow::Result<()> {
    let cli = match CascadeCli::try_parse_from(args.iter().skip(1)) {
        Ok(cli) => cli,
        Err(e) => e.exit(),
    };

    match cli.cmd.unwrap_or(CascadeCmd::Queue) {
        CascadeCmd::Queue => {
            let output = daemon_query("cli-cascade-queue-status", serde_json::json!({}))?;
            println!("{output}");
        }
        CascadeCmd::Drain => {
            let output = daemon_query("cli-cascade-queue-drain", serde_json::json!({}))?;
            println!("{output}");
        }
    }
    Ok(())
}

/// Expose this handler's clap Command for the completions aggregator (W7).
pub(super) fn command() -> clap::Command {
    use clap::CommandFactory;
    CascadeCli::command()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> CascadeCli {
        CascadeCli::try_parse_from(args).expect("args should parse")
    }

    #[test]
    fn bare_cascade_defaults_to_queue() {
        let cli = parse(&["cascade"]);
        assert!(cli.cmd.is_none()); // run() maps None -> Queue
    }

    #[test]
    fn explicit_queue_parses() {
        let cli = parse(&["cascade", "queue"]);
        assert!(matches!(cli.cmd, Some(CascadeCmd::Queue)));
    }

    #[test]
    fn explicit_drain_parses() {
        let cli = parse(&["cascade", "drain"]);
        assert!(matches!(cli.cmd, Some(CascadeCmd::Drain)));
    }

    #[test]
    fn unknown_subcommand_is_parse_error() {
        assert!(CascadeCli::try_parse_from(["cascade", "frobnicate"]).is_err());
    }
}
