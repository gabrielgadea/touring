//! `touring conflict scan|list|resolve` — Detect unresolved merge conflicts in source files.
//!
//! Scans workspace for conflict markers (<<<<<<, ======, >>>>>>) and
//! reports files with unresolved conflicts grouped by severity.
//!
//! Wave P3-1.3 W6a (2026-06-11): migrated from manual `arg_or` positional
//! parsing to clap derive. The `run` signature is unchanged — receives full
//! argv slice; clap parses `args[1..]`.

use crate::cli::daemon_query;
use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "touring conflict",
    bin_name = "touring conflict",
    about = "Detect and resolve unresolved merge conflicts in source files",
    disable_help_subcommand = true
)]
struct ConflictCli {
    #[command(subcommand)]
    cmd: Option<ConflictCmd>,
}

#[derive(Subcommand, Debug)]
enum ConflictCmd {
    /// Scan path for conflict markers and report counts (default).
    Scan {
        /// Path to scan (default: current directory).
        #[arg(default_value = ".")]
        path: String,
    },
    /// List files containing conflict markers.
    List {
        /// Path to scan (default: current directory).
        #[arg(default_value = ".")]
        path: String,
    },
    /// Apply a resolution strategy to a file with conflicts.
    Resolve {
        /// File to resolve.
        #[arg(default_value = "")]
        file: String,
        /// Resolution strategy: `ours` or `theirs`.
        #[arg(long, default_value = "ours")]
        strategy: String,
        /// Dry-run — report what would change without writing.
        #[arg(long)]
        dry_run: bool,
    },
}
/// Run the conflict subcommand.
pub fn run(args: &[String]) -> anyhow::Result<()> {
    let cli = match ConflictCli::try_parse_from(args.iter().skip(1)) {
        Ok(cli) => cli,
        Err(e) => e.exit(),
    };
    match cli.cmd.unwrap_or(ConflictCmd::Scan {
        path: ".".to_string(),
    }) {
        ConflictCmd::Scan { path } => {
            let payload = serde_json::json!({ "path" : path, "action" : "scan" });
            let output = daemon_query("cli-conflict-scan", payload)?;
            println!("{}", output);
        }
        ConflictCmd::List { path } => {
            let payload = serde_json::json!({ "path" : path, "action" : "list" });
            let output = daemon_query("cli-conflict-list", payload)?;
            println!("{}", output);
        }
        ConflictCmd::Resolve {
            file,
            strategy,
            dry_run,
        } => {
            let payload = serde_json::json!(
                { "file" : file, "strategy" : strategy, "dry_run" : dry_run }
            );
            let output = daemon_query("cli-conflict-resolve", payload)?;
            println!("{}", output);
        }
    }
    Ok(())
}

/// Expose this handler's clap Command for the completions aggregator (W7).
pub(super) fn command() -> clap::Command {
    use clap::CommandFactory;
    ConflictCli::command()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn s(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|p| p.to_string()).collect()
    }
    #[test]
    fn defaults_to_scan_with_dot_path() {
        let cli = ConflictCli::try_parse_from(s(&["conflict"]).iter()).unwrap();
        let cmd = cli.cmd.unwrap_or(ConflictCmd::Scan {
            path: ".".to_string(),
        });
        assert!(matches!(cmd, ConflictCmd::Scan { path } if path == "."));
    }
    #[test]
    fn scan_with_explicit_path() {
        let cli = ConflictCli::try_parse_from(s(&["conflict", "scan", "/repo"]).iter()).unwrap();
        assert!(matches!(cli.cmd.unwrap(), ConflictCmd::Scan { path } if path == "/repo"));
    }
    #[test]
    fn list_default_path() {
        let cli = ConflictCli::try_parse_from(s(&["conflict", "list"]).iter()).unwrap();
        assert!(matches!(cli.cmd.unwrap(), ConflictCmd::List { path } if path == "."));
    }
    #[test]
    fn resolve_default_strategy_is_ours() {
        let cli =
            ConflictCli::try_parse_from(s(&["conflict", "resolve", "src/lib.rs"]).iter()).unwrap();
        assert!(
            matches!(cli.cmd.unwrap(), ConflictCmd::Resolve { strategy, .. } if strategy
            == "ours")
        );
    }
    #[test]
    fn resolve_theirs_strategy() {
        let cli = ConflictCli::try_parse_from(
            s(&["conflict", "resolve", "src/lib.rs", "--strategy", "theirs"]).iter(),
        )
        .unwrap();
        assert!(
            matches!(cli.cmd.unwrap(), ConflictCmd::Resolve { strategy, .. } if strategy
            == "theirs")
        );
    }
    #[test]
    fn resolve_dry_run_flag() {
        let cli = ConflictCli::try_parse_from(
            s(&["conflict", "resolve", "src/lib.rs", "--dry-run"]).iter(),
        )
        .unwrap();
        assert!(matches!(cli.cmd.unwrap(), ConflictCmd::Resolve { dry_run, .. } if dry_run));
    }
    #[test]
    fn unknown_subcommand_is_rejected() {
        let result = ConflictCli::try_parse_from(s(&["conflict", "bogus"]).iter());
        assert!(result.is_err());
    }
}
