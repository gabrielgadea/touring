//! `touring workflow run|stats|slowest|compare|resume` — Task execution
//! visualization (Feature B).
//!
//! Provides real-time execution visualization, analytics, and step-level
//! tracking for decomposed tasks.
//!
//! Wave P3-1.3 W2 (2026-06-11): migrated to clap derive (pilot pattern).

use super::daemon_query;
use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "touring workflow",
    bin_name = "touring workflow",
    about = "Task execution visualization and analytics",
    disable_help_subcommand = true
)]
struct WorkflowCli {
    #[command(subcommand)]
    cmd: Option<WorkflowCmd>,
}

#[derive(Subcommand, Debug)]
enum WorkflowCmd {
    /// Run execution visualization for a task (default subcommand).
    Run {
        /// Task identifier.
        #[arg(default_value = "")]
        task_id: String,
        /// Watch mode (live updates).
        #[arg(long)]
        watch: bool,
        /// JSON output.
        #[arg(long)]
        json: bool,
    },
    /// Aggregate execution stats for a task.
    Stats {
        /// Task identifier.
        #[arg(default_value = "")]
        task_id: String,
    },
    /// Top-N slowest steps of a task.
    Slowest {
        /// Task identifier.
        #[arg(default_value = "")]
        task_id: String,
        /// How many entries to show.
        #[arg(default_value_t = 5)]
        top: i64,
    },
    /// Compare two task executions side by side.
    Compare {
        /// First task id.
        #[arg(default_value = "")]
        task_id_a: String,
        /// Second task id.
        #[arg(default_value = "")]
        task_id_b: String,
    },
    /// Resume a paused run.
    Resume {
        /// Task identifier.
        #[arg(default_value = "")]
        task_id: String,
    },
}

fn default_run() -> WorkflowCmd {
    WorkflowCmd::Run {
        task_id: String::new(),
        watch: false,
        json: false,
    }
}

/// Entry point for the `touring workflow` CLI handler — dispatches the `run`
/// (default), `stats`, `slowest`, `compare`, and `resume` subcommands of the
/// task execution visualizer to their respective `cli-workflow-*` daemon
/// queries and prints each response. `resume` reuses the `cli-workflow-run`
/// handler with `resume: true`.
pub fn run(args: &[String]) -> anyhow::Result<()> {
    let cli = match WorkflowCli::try_parse_from(args.iter().skip(1)) {
        Ok(cli) => cli,
        Err(e) => e.exit(),
    };
    match cli.cmd.unwrap_or_else(default_run) {
        WorkflowCmd::Run {
            task_id,
            watch,
            json,
        } => {
            let payload = serde_json::json!({
                "task_id": task_id,
                "watch": watch,
                "json": json,
            });
            let output = daemon_query("cli-workflow-run", payload)?;
            println!("{output}");
        }
        WorkflowCmd::Stats { task_id } => {
            let payload = serde_json::json!({ "task_id": task_id });
            let output = daemon_query("cli-workflow-stats", payload)?;
            println!("{output}");
        }
        WorkflowCmd::Slowest { task_id, top } => {
            let payload = serde_json::json!({
                "task_id": task_id,
                "top": top,
            });
            let output = daemon_query("cli-workflow-slowest", payload)?;
            println!("{output}");
        }
        WorkflowCmd::Compare {
            task_id_a,
            task_id_b,
        } => {
            let payload = serde_json::json!({
                "task_id_a": task_id_a,
                "task_id_b": task_id_b,
            });
            let output = daemon_query("cli-workflow-compare", payload)?;
            println!("{output}");
        }
        WorkflowCmd::Resume { task_id } => {
            let payload = serde_json::json!({
                "task_id": task_id,
                "resume": true,
            });
            let output = daemon_query("cli-workflow-run", payload)?;
            println!("{output}");
        }
    }
    Ok(())
}

/// Expose this handler's clap Command for the completions aggregator (W7).
pub(super) fn command() -> clap::Command {
    use clap::CommandFactory;
    WorkflowCli::command()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bare_workflow_defaults_to_run() {
        let cli = WorkflowCli::try_parse_from(["workflow"]).expect("parse");
        assert!(cli.cmd.is_none()); // run() maps None -> Run defaults
    }

    #[test]
    fn run_parses_watch_and_json_flags() {
        let cli = WorkflowCli::try_parse_from(["workflow", "run", "task_1", "--watch", "--json"])
            .expect("parse");
        let Some(WorkflowCmd::Run {
            task_id,
            watch,
            json,
        }) = cli.cmd
        else {
            panic!("expected Run");
        };
        assert_eq!(task_id, "task_1");
        assert!(watch);
        assert!(json);
    }

    #[test]
    fn slowest_top_defaults_to_5_and_parses_explicit() {
        let cli = WorkflowCli::try_parse_from(["workflow", "slowest", "task_1"]).expect("parse");
        let Some(WorkflowCmd::Slowest { top, .. }) = cli.cmd else {
            panic!("expected Slowest");
        };
        assert_eq!(top, 5);

        let cli =
            WorkflowCli::try_parse_from(["workflow", "slowest", "task_1", "12"]).expect("parse");
        let Some(WorkflowCmd::Slowest { top, .. }) = cli.cmd else {
            panic!("expected Slowest");
        };
        assert_eq!(top, 12);
    }

    /// Legacy parser silently fell back to 5 on non-numeric input; clap
    /// rejects it loudly (the bug class this migration eliminates).
    #[test]
    fn slowest_non_numeric_top_is_a_parse_error() {
        assert!(WorkflowCli::try_parse_from(["workflow", "slowest", "task_1", "abc"]).is_err());
    }

    #[test]
    fn compare_binds_both_task_ids() {
        let cli = WorkflowCli::try_parse_from(["workflow", "compare", "task_a", "task_b"])
            .expect("parse");
        let Some(WorkflowCmd::Compare {
            task_id_a,
            task_id_b,
        }) = cli.cmd
        else {
            panic!("expected Compare");
        };
        assert_eq!(task_id_a, "task_a");
        assert_eq!(task_id_b, "task_b");
    }

    #[test]
    fn resume_parses_task_id() {
        let cli = WorkflowCli::try_parse_from(["workflow", "resume", "task_9"]).expect("parse");
        let Some(WorkflowCmd::Resume { task_id }) = cli.cmd else {
            panic!("expected Resume");
        };
        assert_eq!(task_id, "task_9");
    }
}
