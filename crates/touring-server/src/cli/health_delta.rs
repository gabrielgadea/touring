//! `touring health-delta` — Wave 15 direct exposure of HealthDeltaCache state
//! + Wave I (Phase 5) grow-only SQLite audit trail query.
//!
//! Subcommands:
//!
//! - `touring health-delta status [file_path]` — per-path streak counters when
//!   `file_path` is given; aggregate gate_metrics counters when omitted.
//!
//! - `touring health-delta reset <file_path>` — clears streak counters and the
//!   pending pre-record entry for the given path.
//!
//! - `touring health-delta history [--since <ms>] [--path <prefix>] [--limit N]`
//!   — Wave I: query the grow-only SQLite audit trail of every emitted
//!   `HealthDeltaEvent`.
//!
//! Wave P3-1.3 W3 (2026-06-11): migrated from manual positional / flag-loop
//! parsing to clap derive. Dispatch contract and all payload keys unchanged (G6).

use super::daemon_query;
use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "touring health-delta",
    bin_name = "touring health-delta",
    about = "HealthDeltaCache inspection: streak counters, reset, and audit trail",
    disable_help_subcommand = true
)]
struct HealthDeltaCli {
    #[command(subcommand)]
    cmd: Option<HealthDeltaCmd>,
}

#[derive(Subcommand, Debug)]
enum HealthDeltaCmd {
    /// Per-path streak counters (default when no subcommand is given).
    ///
    /// When `file_path` is provided, returns per-path counters + hints.
    /// When omitted, returns aggregate gate_metrics counters.
    Status {
        /// Optional file path to query streak state for.
        file_path: Option<String>,
    },
    /// Clear streak counters and the pending pre-record entry for a path.
    Reset {
        /// File path whose streak state should be cleared.
        file_path: String,
    },
    /// Query the grow-only SQLite audit trail of HealthDeltaEvents.
    History {
        /// Only return events after this Unix timestamp in milliseconds.
        #[arg(long)]
        since: Option<u64>,
        /// Filter events by file path prefix.
        #[arg(long)]
        path: Option<String>,
        /// Maximum number of events to return.
        #[arg(long)]
        limit: Option<u64>,
    },
}

/// Entry point for the `touring health-delta` CLI handler — parses the argv
/// slice and dispatches `status` (default; per-path streak counters when a
/// `file_path` is given, aggregate gate_metrics counters otherwise), `reset`
/// (clears streak state for a path), or `history` (queries the grow-only
/// SQLite audit trail with optional `--since`/`--path`/`--limit` filters) to
/// the matching `cli-health-delta-*` daemon hook, printing the JSON response.
pub fn run(args: &[String]) -> anyhow::Result<()> {
    let cli = match HealthDeltaCli::try_parse_from(args.iter().skip(1)) {
        Ok(cli) => cli,
        Err(e) => e.exit(),
    };

    match cli
        .cmd
        .unwrap_or(HealthDeltaCmd::Status { file_path: None })
    {
        HealthDeltaCmd::Status { file_path } => {
            let payload = match file_path.as_deref() {
                Some(p) if !p.is_empty() => serde_json::json!({ "file_path": p }),
                _ => serde_json::json!({}),
            };
            let output = daemon_query("cli-health-delta-status", payload)?;
            println!("{output}");
        }
        HealthDeltaCmd::Reset { file_path } => {
            let output = daemon_query(
                "cli-health-delta-reset",
                serde_json::json!({ "file_path": file_path }),
            )?;
            println!("{output}");
        }
        HealthDeltaCmd::History { since, path, limit } => {
            let mut payload = serde_json::json!({});
            if let Some(s) = since {
                payload["since_ms"] = s.into();
            }
            if let Some(p) = path {
                payload["path_prefix"] = p.into();
            }
            if let Some(n) = limit {
                payload["limit"] = n.into();
            }
            let output = daemon_query("cli-health-delta-history", payload)?;
            println!("{output}");
        }
    }
    Ok(())
}

/// Expose this handler's clap Command for the completions aggregator (W7).
pub(super) fn command() -> clap::Command {
    use clap::CommandFactory;
    HealthDeltaCli::command()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> HealthDeltaCli {
        HealthDeltaCli::try_parse_from(args).expect("args should parse")
    }

    #[test]
    fn bare_health_delta_defaults_to_status_no_path() {
        let cli = parse(&["health-delta"]);
        assert!(cli.cmd.is_none()); // run() maps None -> Status { file_path: None }
    }

    #[test]
    fn status_without_path() {
        let cli = parse(&["health-delta", "status"]);
        let Some(HealthDeltaCmd::Status { file_path }) = cli.cmd else {
            panic!("expected Status");
        };
        assert!(file_path.is_none());
    }

    #[test]
    fn status_with_path() {
        let cli = parse(&["health-delta", "status", "src/lib.rs"]);
        let Some(HealthDeltaCmd::Status { file_path }) = cli.cmd else {
            panic!("expected Status");
        };
        assert_eq!(file_path.as_deref(), Some("src/lib.rs"));
    }

    #[test]
    fn reset_requires_path() {
        assert!(HealthDeltaCli::try_parse_from(["health-delta", "reset"]).is_err());
    }

    #[test]
    fn reset_parses_path() {
        let cli = parse(&["health-delta", "reset", "crates/foo/src/lib.rs"]);
        let Some(HealthDeltaCmd::Reset { file_path }) = cli.cmd else {
            panic!("expected Reset");
        };
        assert_eq!(file_path, "crates/foo/src/lib.rs");
    }

    #[test]
    fn history_all_flags() {
        let cli = parse(&[
            "health-delta",
            "history",
            "--since",
            "1234567890",
            "--path",
            "crates/",
            "--limit",
            "50",
        ]);
        let Some(HealthDeltaCmd::History { since, path, limit }) = cli.cmd else {
            panic!("expected History");
        };
        assert_eq!(since, Some(1_234_567_890u64));
        assert_eq!(path.as_deref(), Some("crates/"));
        assert_eq!(limit, Some(50));
    }

    #[test]
    fn history_no_flags() {
        let cli = parse(&["health-delta", "history"]);
        let Some(HealthDeltaCmd::History { since, path, limit }) = cli.cmd else {
            panic!("expected History");
        };
        assert!(since.is_none());
        assert!(path.is_none());
        assert!(limit.is_none());
    }

    #[test]
    fn unknown_flag_in_history_is_parse_error() {
        assert!(
            HealthDeltaCli::try_parse_from(["health-delta", "history", "--frobnicate", "val"])
                .is_err()
        );
    }

    #[test]
    fn unknown_subcommand_is_parse_error() {
        assert!(HealthDeltaCli::try_parse_from(["health-delta", "frobnicate"]).is_err());
    }
}
