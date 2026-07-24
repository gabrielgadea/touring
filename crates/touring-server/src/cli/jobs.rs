//! `touring jobs spawn|poll|list|drop` — L7-B Gamma Spawn & Poll async primitives.
//!
//! Implements the MCP "Grip Cognitivo" async worker pattern: long-running
//! programs can be started as background tokio tasks with immediate return
//! of a `job_id`, then polled later for status + result.
//!
//! Wave P3-1.3 W4 (2026-06-11): migrated from manual `arg_or` to clap derive.

use super::daemon_query;
use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "touring jobs",
    bin_name = "touring jobs",
    about = "Async job worker: list (default), spawn, poll, drop",
    disable_help_subcommand = true
)]
struct JobsCli {
    #[command(subcommand)]
    cmd: Option<JobsCmd>,
}

#[derive(Subcommand, Debug)]
enum JobsCmd {
    /// List all jobs in the registry with their statuses (default).
    List,
    /// Launch a background worker (no shell — execve semantics).
    Spawn {
        /// Program to execute (no shell interpolation).
        program: String,
        /// Arguments forwarded verbatim to the program.
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Return the current status/result of a job.
    Poll {
        /// Job ID returned by spawn.
        job_id: String,
    },
    /// Remove a completed or failed job from the registry.
    Drop {
        /// Job ID to drop.
        job_id: String,
    },
}

/// Run the `jobs` CLI subcommand dispatcher.
///
/// # Security
///
/// The `spawn` subcommand uses `execve` semantics — program + arguments are
/// passed literally, no shell interpolation. Safe from command injection.
///
/// # Errors
///
/// Returns an error for unknown subcommands, missing required arguments,
/// or daemon communication failures.
pub fn run(args: &[String]) -> anyhow::Result<()> {
    let cli = match JobsCli::try_parse_from(args.iter().skip(1)) {
        Ok(cli) => cli,
        Err(e) => e.exit(),
    };

    match cli.cmd.unwrap_or(JobsCmd::List) {
        JobsCmd::List => {
            let output = daemon_query("cli-jobs-list", serde_json::json!({}))?;
            println!("{output}");
        }
        JobsCmd::Spawn {
            program,
            args: worker_args,
        } => {
            if program.is_empty() {
                anyhow::bail!(
                    "Usage: touring jobs spawn <program> [args...]\n\
                     Example: touring jobs spawn cargo test --release"
                );
            }
            let payload = serde_json::json!({
                "tool_name": program,
                "program": program,
                "args": worker_args,
            });
            let output = daemon_query("cli-jobs-spawn", payload)?;
            println!("{output}");
        }
        JobsCmd::Poll { job_id } => {
            if job_id.is_empty() {
                anyhow::bail!("Usage: touring jobs poll <job_id>");
            }
            let payload = serde_json::json!({ "job_id": job_id });
            let output = daemon_query("cli-jobs-poll", payload)?;
            println!("{output}");
        }
        JobsCmd::Drop { job_id } => {
            if job_id.is_empty() {
                anyhow::bail!("Usage: touring jobs drop <job_id>");
            }
            let payload = serde_json::json!({ "job_id": job_id });
            let output = daemon_query("cli-jobs-drop", payload)?;
            println!("{output}");
        }
    }
    Ok(())
}

/// Expose this handler's clap Command for the completions aggregator (W7).
pub(super) fn command() -> clap::Command {
    use clap::CommandFactory;
    JobsCli::command()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> JobsCli {
        JobsCli::try_parse_from(args).expect("args should parse")
    }

    #[test]
    fn bare_jobs_defaults_to_list() {
        let cli = parse(&["jobs"]);
        assert!(cli.cmd.is_none()); // run() maps None -> List
    }

    #[test]
    fn explicit_list_parses() {
        let cli = parse(&["jobs", "list"]);
        assert!(matches!(cli.cmd, Some(JobsCmd::List)));
    }

    #[test]
    fn spawn_parses_program_and_args() {
        let cli = parse(&["jobs", "spawn", "cargo", "test", "--release"]);
        let Some(JobsCmd::Spawn { program, args }) = cli.cmd else {
            panic!("expected Spawn");
        };
        assert_eq!(program, "cargo");
        assert_eq!(args, &["test", "--release"]);
    }

    #[test]
    fn spawn_missing_program_is_parse_error() {
        assert!(JobsCli::try_parse_from(["jobs", "spawn"]).is_err());
    }

    #[test]
    fn poll_parses_job_id() {
        let cli = parse(&["jobs", "poll", "job_abc123"]);
        let Some(JobsCmd::Poll { job_id }) = cli.cmd else {
            panic!("expected Poll");
        };
        assert_eq!(job_id, "job_abc123");
    }

    #[test]
    fn poll_missing_job_id_is_parse_error() {
        assert!(JobsCli::try_parse_from(["jobs", "poll"]).is_err());
    }

    #[test]
    fn drop_parses_job_id() {
        let cli = parse(&["jobs", "drop", "job_xyz"]);
        let Some(JobsCmd::Drop { job_id }) = cli.cmd else {
            panic!("expected Drop");
        };
        assert_eq!(job_id, "job_xyz");
    }

    #[test]
    fn drop_missing_job_id_is_parse_error() {
        assert!(JobsCli::try_parse_from(["jobs", "drop"]).is_err());
    }

    #[test]
    fn unknown_subcommand_is_parse_error() {
        assert!(JobsCli::try_parse_from(["jobs", "frobnicate"]).is_err());
    }
}
