//! `touring overlay status|diff|sync` — Workspace overlay visualization and diff tool.
//!
//! Shows differences between the actual filesystem state and what the
//! touring index believes exists, highlighting stale entries and gaps.
//!
//! Wave P3-1.3 W6a (2026-06-11): migrated from manual `arg_or` positional
//! parsing to clap derive. The `run` signature is unchanged — receives full
//! argv slice; clap parses `args[1..]`.

use crate::cli::daemon_query;
use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "touring overlay",
    bin_name = "touring overlay",
    about = "Workspace overlay visualization and diff tool",
    disable_help_subcommand = true
)]
struct OverlayCli {
    #[command(subcommand)]
    cmd: Option<OverlayCmd>,
}

#[derive(Subcommand, Debug)]
enum OverlayCmd {
    /// Show overlay status between filesystem and touring index (default).
    Status {
        /// Path to scan (default: current directory).
        #[arg(default_value = ".")]
        path: String,
    },
    /// Show diff between filesystem and index.
    Diff {
        /// Path to scan (default: current directory).
        #[arg(default_value = ".")]
        path: String,
    },
    /// Sync index with filesystem.
    Sync {
        /// Path to sync (default: current directory).
        #[arg(default_value = ".")]
        path: String,
        /// Dry-run — report what would change without modifying index.
        #[arg(long)]
        dry_run: bool,
    },
}

/// Run the overlay subcommand.
pub fn run(args: &[String]) -> anyhow::Result<()> {
    let cli = match OverlayCli::try_parse_from(args.iter().skip(1)) {
        Ok(cli) => cli,
        Err(e) => e.exit(),
    };

    match cli.cmd.unwrap_or(OverlayCmd::Status {
        path: ".".to_string(),
    }) {
        OverlayCmd::Status { path } => {
            let payload = serde_json::json!({
                "path": path,
                "action": "status"
            });
            let output = daemon_query("cli-overlay-status", payload)?;
            println!("{}", output);
        }
        OverlayCmd::Diff { path } => {
            let payload = serde_json::json!({
                "path": path,
                "action": "diff"
            });
            let output = daemon_query("cli-overlay-diff", payload)?;
            println!("{}", output);
        }
        OverlayCmd::Sync { path, dry_run } => {
            let payload = serde_json::json!({
                "path": path,
                "action": "sync",
                "dry_run": dry_run
            });
            let output = daemon_query("cli-overlay-sync", payload)?;
            println!("{}", output);
        }
    }
    Ok(())
}

/// Expose this handler's clap Command for the completions aggregator (W7).
pub(super) fn command() -> clap::Command {
    use clap::CommandFactory;
    OverlayCli::command()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|p| p.to_string()).collect()
    }

    #[test]
    fn defaults_to_status_with_dot_path() {
        let cli = OverlayCli::try_parse_from(s(&["overlay"]).iter()).unwrap();
        let cmd = cli.cmd.unwrap_or(OverlayCmd::Status {
            path: ".".to_string(),
        });
        assert!(matches!(cmd, OverlayCmd::Status { path } if path == "."));
    }

    #[test]
    fn status_with_explicit_path() {
        let cli =
            OverlayCli::try_parse_from(s(&["overlay", "status", "/some/path"]).iter()).unwrap();
        assert!(matches!(cli.cmd.unwrap(), OverlayCmd::Status { path } if path == "/some/path"));
    }

    #[test]
    fn diff_default_path() {
        let cli = OverlayCli::try_parse_from(s(&["overlay", "diff"]).iter()).unwrap();
        assert!(matches!(cli.cmd.unwrap(), OverlayCmd::Diff { path } if path == "."));
    }

    #[test]
    fn sync_dry_run_flag() {
        let cli =
            OverlayCli::try_parse_from(s(&["overlay", "sync", "/workspace", "--dry-run"]).iter())
                .unwrap();
        assert!(
            matches!(cli.cmd.unwrap(), OverlayCmd::Sync { path, dry_run } if path == "/workspace" && dry_run)
        );
    }

    #[test]
    fn sync_without_dry_run() {
        let cli = OverlayCli::try_parse_from(s(&["overlay", "sync"]).iter()).unwrap();
        assert!(matches!(cli.cmd.unwrap(), OverlayCmd::Sync { dry_run, .. } if !dry_run));
    }

    #[test]
    fn unknown_subcommand_is_rejected() {
        let result = OverlayCli::try_parse_from(s(&["overlay", "bogus"]).iter());
        assert!(result.is_err());
    }
}
