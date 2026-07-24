//! `touring session start|checkpoint|list|assess` — Session management.
//!
//! Manages Touring session state, checkpoints, and history.
//!
//! Wave P3-1.3 W2 (2026-06-11): migrated to clap derive (pilot pattern).

use super::daemon_query;
use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "touring session",
    bin_name = "touring session",
    about = "Session management (state, checkpoints, history)",
    disable_help_subcommand = true
)]
struct SessionCli {
    #[command(subcommand)]
    cmd: Option<SessionCmd>,
}

#[derive(Subcommand, Debug)]
enum SessionCmd {
    /// Start a session (loads knowledge + RL state).
    Start {
        /// Session identifier.
        #[arg(default_value = "default")]
        session_id: String,
        /// Task type bucket.
        #[arg(default_value = "general")]
        task_type: String,
        /// Free-form objective (remaining positional args, joined).
        objective: Vec<String>,
    },
    /// Persist a checkpoint blob.
    Checkpoint {
        /// Checkpoint identifier.
        #[arg(default_value = "default")]
        checkpoint_id: String,
        /// Free-form checkpoint data (remaining positional args, joined).
        data: Vec<String>,
    },
    /// List sessions (default when no subcommand is given).
    List,
    /// Composite score + phase breakdown for a session.
    Assess {
        /// Session identifier.
        #[arg(default_value = "current")]
        session_id: String,
    },
}

/// Entry point for the `touring session` CLI handler — dispatches the `start`,
/// `checkpoint`, `list` (default), and `assess` subcommands to their respective
/// `cli-session-*` daemon queries and prints each response. `start` joins its
/// trailing args into the objective; `checkpoint` joins them into the data blob.
pub fn run(args: &[String]) -> anyhow::Result<()> {
    let cli = match SessionCli::try_parse_from(args.iter().skip(1)) {
        Ok(cli) => cli,
        Err(e) => e.exit(),
    };
    match cli.cmd.unwrap_or(SessionCmd::List) {
        SessionCmd::Start {
            session_id,
            task_type,
            objective,
        } => {
            let payload = serde_json::json!({
                "session_id": session_id,
                "task_type": task_type,
                "objective": objective.join(" "),
            });
            let output = daemon_query("cli-session-start", payload)?;
            println!("{output}");
        }
        SessionCmd::Checkpoint {
            checkpoint_id,
            data,
        } => {
            let payload = serde_json::json!({
                "checkpoint_id": checkpoint_id,
                "data": data.join(" "),
            });
            let output = daemon_query("cli-session-checkpoint", payload)?;
            println!("{output}");
        }
        SessionCmd::List => {
            let output = daemon_query("cli-session-list", serde_json::json!({}))?;
            println!("{output}");
        }
        SessionCmd::Assess { session_id } => {
            let payload = serde_json::json!({ "session_id": session_id });
            let output = daemon_query("cli-session-assess", payload)?;
            println!("{output}");
        }
    }
    Ok(())
}

/// Expose this handler's clap Command for the completions aggregator (W7).
pub(super) fn command() -> clap::Command {
    use clap::CommandFactory;
    SessionCli::command()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bare_session_defaults_to_list() {
        let cli = SessionCli::try_parse_from(["session"]).expect("parse");
        assert!(cli.cmd.is_none()); // run() maps None -> List
    }

    #[test]
    fn start_parses_ids_and_joined_objective() {
        let cli = SessionCli::try_parse_from([
            "session", "start", "sess_1", "refactor", "split", "the", "monolith",
        ])
        .expect("parse");
        let Some(SessionCmd::Start {
            session_id,
            task_type,
            objective,
        }) = cli.cmd
        else {
            panic!("expected Start");
        };
        assert_eq!(session_id, "sess_1");
        assert_eq!(task_type, "refactor");
        assert_eq!(objective.join(" "), "split the monolith");
    }

    #[test]
    fn start_defaults_match_legacy_behavior() {
        let cli = SessionCli::try_parse_from(["session", "start"]).expect("parse");
        let Some(SessionCmd::Start {
            session_id,
            task_type,
            objective,
        }) = cli.cmd
        else {
            panic!("expected Start");
        };
        assert_eq!(session_id, "default");
        assert_eq!(task_type, "general");
        assert!(objective.is_empty());
    }

    #[test]
    fn assess_defaults_to_current() {
        let cli = SessionCli::try_parse_from(["session", "assess"]).expect("parse");
        let Some(SessionCmd::Assess { session_id }) = cli.cmd else {
            panic!("expected Assess");
        };
        assert_eq!(session_id, "current");
    }

    #[test]
    fn unknown_subcommand_is_parse_error() {
        assert!(SessionCli::try_parse_from(["session", "frobnicate"]).is_err());
    }
}
