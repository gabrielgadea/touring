//! `touring plugin list|status|reload|unregister` — Plugin registry management.
//!
//! Provides visibility and control over the daemon's plugin registry.
//! All operations communicate with the running daemon via UNIX socket IPC.
//!
//! ## Plugin families
//! Plugins are organized by family (e.g. `Embeddings`, `Reranker`).
//! Each family can have multiple backend plugins (e.g. `fastembed`,
//! `sentence-transformers`) with one active at runtime.
//!
//! ## Subcommands
//! - `list` — list all registered plugin families and their backends
//! - `status <id>` — show detailed status for a specific plugin by id
//! - `reload <id>` — hot-reload a plugin (re-reads config, swaps backend atomically)
//! - `unregister <id>` — remove a plugin from the registry
//!
//! ## Daemon IPC
//! Each subcommand sends a `cli-plugin-*` request to the daemon socket.
//! The daemon handler queries the `PluginRegistry` via `global_registry()`.
//!
//! Wave P3-1.3 W6a (2026-06-11): migrated from manual `arg_or` positional
//! parsing to clap derive. The `run` signature is unchanged — receives full
//! argv slice; clap parses `args[1..]`.

use super::daemon_query;
use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "touring plugin",
    bin_name = "touring plugin",
    about = "Plugin registry management (list, status, reload, unregister)",
    disable_help_subcommand = true
)]
struct PluginCli {
    #[command(subcommand)]
    cmd: Option<PluginCmd>,
}

#[derive(Subcommand, Debug)]
enum PluginCmd {
    /// List all registered plugin families and their backends (default).
    List,
    /// Show detailed status for a specific plugin.
    Status {
        /// Plugin id (see `touring plugin list`).
        id: String,
    },
    /// Hot-reload a plugin (re-reads config, swaps backend atomically).
    Reload {
        /// Plugin id (see `touring plugin list`).
        id: String,
    },
    /// Remove a plugin from the registry.
    Unregister {
        /// Plugin id (see `touring plugin list`).
        id: String,
    },
}
/// Run the `plugin` CLI subcommand dispatcher.
///
/// # Errors
///
/// Returns an error for unknown subcommands, missing arguments, or daemon
/// communication failures.
pub fn run(args: &[String]) -> anyhow::Result<()> {
    let cli = match PluginCli::try_parse_from(args.iter().skip(1)) {
        Ok(cli) => cli,
        Err(e) => e.exit(),
    };
    match cli.cmd.unwrap_or(PluginCmd::List) {
        PluginCmd::List => {
            let output = daemon_query("cli-plugin-list", serde_json::json!({}))?;
            println!("{output}");
        }
        PluginCmd::Status { id } => {
            let payload = serde_json::json!({ "id" : id });
            let output = daemon_query("cli-plugin-status", payload)?;
            println!("{output}");
        }
        PluginCmd::Reload { id } => {
            let payload = serde_json::json!({ "id" : id });
            let output = daemon_query("cli-plugin-reload", payload)?;
            println!("{output}");
        }
        PluginCmd::Unregister { id } => {
            let payload = serde_json::json!({ "id" : id });
            let output = daemon_query("cli-plugin-unregister", payload)?;
            println!("{output}");
        }
    }
    Ok(())
}

/// Expose this handler's clap Command for the completions aggregator (W7).
pub(super) fn command() -> clap::Command {
    use clap::CommandFactory;
    PluginCli::command()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn s(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|p| p.to_string()).collect()
    }
    #[test]
    fn defaults_to_list() {
        let cli = PluginCli::try_parse_from(s(&["plugin"]).iter()).unwrap();
        assert!(matches!(
            cli.cmd.unwrap_or(PluginCmd::List),
            PluginCmd::List
        ));
    }
    #[test]
    fn status_requires_id() {
        let result = PluginCli::try_parse_from(s(&["plugin", "status"]).iter());
        assert!(result.is_err());
    }
    #[test]
    fn status_with_id() {
        let cli = PluginCli::try_parse_from(s(&["plugin", "status", "fastembed"]).iter()).unwrap();
        assert!(matches!(cli.cmd.unwrap(), PluginCmd::Status { id } if id == "fastembed"));
    }
    #[test]
    fn reload_with_id() {
        let cli = PluginCli::try_parse_from(s(&["plugin", "reload", "fastembed"]).iter()).unwrap();
        assert!(matches!(cli.cmd.unwrap(), PluginCmd::Reload { id } if id == "fastembed"));
    }
    #[test]
    fn unregister_with_id() {
        let cli =
            PluginCli::try_parse_from(s(&["plugin", "unregister", "old-plugin"]).iter()).unwrap();
        assert!(
            matches!(cli.cmd.unwrap(), PluginCmd::Unregister { id } if id ==
            "old-plugin")
        );
    }
    #[test]
    fn unknown_subcommand_is_rejected() {
        let result = PluginCli::try_parse_from(s(&["plugin", "bogus"]).iter());
        assert!(result.is_err());
    }
}
