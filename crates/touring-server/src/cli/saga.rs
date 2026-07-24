//! `touring saga register|begin|status|abort` — Distributed 2PC saga coordinator.
//!
//! Manages distributed two-phase commit transactions with remote subagents
//! via the daemon socket. Uses rkyv zero-copy IPC for efficient message framing.
//!
//! Wave P3-1.3 W6a (2026-06-11): migrated from manual `arg_or` positional
//! parsing to clap derive. The `run` signature is unchanged — receives full
//! argv slice; clap parses `args[1..]`.

use super::daemon_query;
use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "touring saga",
    bin_name = "touring saga",
    about = "Distributed 2PC saga coordinator (register, begin, status, abort)",
    disable_help_subcommand = true
)]
struct SagaCli {
    #[command(subcommand)]
    cmd: Option<SagaCmd>,
}

#[derive(Subcommand, Debug)]
enum SagaCmd {
    /// Register a subagent with the saga coordinator.
    Register {
        /// Subagent identifier.
        agent_id: String,
        /// Number of transaction steps this agent participates in.
        #[arg(default_value = "0")]
        step_count: u32,
    },
    /// Begin a 2PC transaction with one or more step_id/action pairs.
    ///
    /// Arguments must be provided as alternating `<step_id> <action>` pairs,
    /// e.g. `touring saga begin step-1 commit step-2 rollback`.
    Begin {
        /// Alternating step_id/action pairs (trailing_var_arg).
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        pairs: Vec<String>,
    },
    /// Show coordinator state (all transactions, or a specific tx) (default).
    Status {
        /// Optional transaction id (omit to list all).
        tx_id: Option<String>,
    },
    /// Abort an in-flight transaction.
    Abort {
        /// Transaction id to abort.
        tx_id: String,
    },
}
/// Entry point for the `touring saga` CLI handler — dispatches the `register`,
/// `begin`, `status` (default), and `abort` subcommands of the distributed 2PC
/// saga coordinator to their respective `cli-saga-*` daemon queries and prints
/// each response. `begin` pairs the trailing args as alternating
/// `step_id`/`action` entries.
pub fn run(args: &[String]) -> anyhow::Result<()> {
    let cli = match SagaCli::try_parse_from(args.iter().skip(1)) {
        Ok(cli) => cli,
        Err(e) => e.exit(),
    };
    match cli.cmd.unwrap_or(SagaCmd::Status { tx_id: None }) {
        SagaCmd::Register {
            agent_id,
            step_count,
        } => {
            if agent_id.is_empty() {
                anyhow::bail!("Usage: touring saga register <agent_id> <step_count>");
            }
            let payload = serde_json::json!(
                { "agent_id" : agent_id, "step_count" : step_count }
            );
            let output = daemon_query("cli-saga-register", payload)?;
            println!("{}", output);
        }
        SagaCmd::Begin { pairs } => {
            if pairs.is_empty() {
                anyhow::bail!(
                    "Usage: touring saga begin <step_id> <action> [step_id action ...]\n\
                     Error: at least one step_id and action pair required"
                );
            }
            let mut steps: Vec<serde_json::Value> = Vec::new();
            let mut iter = pairs.iter();
            while let Some(step_id) = iter.next() {
                if let Some(action) = iter.next() {
                    steps.push(serde_json::json!(
                        { "step_id" : step_id.as_str(), "action" : action.as_str() }
                    ));
                }
            }
            if steps.is_empty() {
                anyhow::bail!("Error: odd number of arguments — need step_id/action pairs");
            }
            let payload = serde_json::json!({ "steps" : steps });
            let output = daemon_query("cli-saga-begin", payload)?;
            println!("{}", output);
        }
        SagaCmd::Status { tx_id } => {
            let payload = match tx_id.as_deref() {
                Some(id) if !id.is_empty() => {
                    serde_json::json!({ "transaction_id" : id })
                }
                _ => serde_json::json!({}),
            };
            let output = daemon_query("cli-saga-status", payload)?;
            println!("{}", output);
        }
        SagaCmd::Abort { tx_id } => {
            if tx_id.is_empty() {
                anyhow::bail!("Usage: touring saga abort <tx_id>");
            }
            let payload = serde_json::json!({ "transaction_id" : tx_id });
            let output = daemon_query("cli-saga-abort", payload)?;
            println!("{}", output);
        }
    }
    Ok(())
}

/// Expose this handler's clap Command for the completions aggregator (W7).
pub(super) fn command() -> clap::Command {
    use clap::CommandFactory;
    SagaCli::command()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn s(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|p| p.to_string()).collect()
    }
    #[test]
    fn defaults_to_status_no_tx_id() {
        let cli = SagaCli::try_parse_from(s(&["saga"]).iter()).unwrap();
        let cmd = cli.cmd.unwrap_or(SagaCmd::Status { tx_id: None });
        assert!(matches!(cmd, SagaCmd::Status { tx_id: None }));
    }
    #[test]
    fn register_parses_agent_id_and_step_count() {
        let cli =
            SagaCli::try_parse_from(s(&["saga", "register", "agent-42", "3"]).iter()).unwrap();
        assert!(
            matches!(cli.cmd.unwrap(), SagaCmd::Register { agent_id, step_count } if
            agent_id == "agent-42" && step_count == 3)
        );
    }
    #[test]
    fn register_default_step_count_is_zero() {
        let cli = SagaCli::try_parse_from(s(&["saga", "register", "agent-1"]).iter()).unwrap();
        assert!(
            matches!(cli.cmd.unwrap(), SagaCmd::Register { step_count, .. } if step_count
            == 0)
        );
    }
    #[test]
    fn begin_collects_pairs() {
        let cli =
            SagaCli::try_parse_from(s(&["saga", "begin", "s1", "commit", "s2", "rollback"]).iter())
                .unwrap();
        assert!(matches!(cli.cmd.unwrap(), SagaCmd::Begin { pairs } if pairs.len() == 4));
    }
    #[test]
    fn status_with_tx_id() {
        let cli = SagaCli::try_parse_from(s(&["saga", "status", "tx-999"]).iter()).unwrap();
        assert!(
            matches!(cli.cmd.unwrap(), SagaCmd::Status { tx_id : Some(id) } if id ==
            "tx-999")
        );
    }
    #[test]
    fn abort_requires_tx_id() {
        let result = SagaCli::try_parse_from(s(&["saga", "abort"]).iter());
        assert!(result.is_err());
    }
    #[test]
    fn abort_with_tx_id() {
        let cli = SagaCli::try_parse_from(s(&["saga", "abort", "tx-42"]).iter()).unwrap();
        assert!(matches!(cli.cmd.unwrap(), SagaCmd::Abort { tx_id } if tx_id == "tx-42"));
    }
}
