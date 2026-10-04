//! `touring workflow run|stats|status|slowest|compare|resume` — Task execution
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
    /// Poll a task: subtask counts by status, timing and the attempt completion rate.
    Status {
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
/// (default), `stats`, `status`, `slowest`, `compare`, and `resume` subcommands of the
/// task execution visualizer to their respective `cli-workflow-*` daemon
/// queries and prints each response. `resume` reuses the `cli-workflow-run`
/// handler with `resume: true`.
pub fn run(args: &[String]) -> anyhow::Result<()> {
    let cli = match WorkflowCli::try_parse_from(args.iter().skip(1)) {
        Ok(cli) => cli,
        Err(e) => e.exit(),
    };
    let (handler, payload) = route(cli.cmd.unwrap_or_else(default_run));
    let output = daemon_query(handler, payload)?;
    println!("{output}");
    Ok(())
}

/// The daemon handler and payload each subcommand sends — the one place the CLI names a
/// `cli-workflow-*` handler, so a subcommand that reaches the wrong one is a unit-test failure.
fn route(cmd: WorkflowCmd) -> (&'static str, serde_json::Value) {
    match cmd {
        WorkflowCmd::Run {
            task_id,
            watch,
            json,
        } => (
            "cli-workflow-run",
            serde_json::json!({ "task_id": task_id, "watch": watch, "json": json }),
        ),
        WorkflowCmd::Stats { task_id } => ("cli-workflow-stats", serde_json::json!({ "task_id": task_id })),
        WorkflowCmd::Status { task_id } => ("cli-workflow-status", serde_json::json!({ "task_id": task_id })),
        WorkflowCmd::Slowest { task_id, top } => (
            "cli-workflow-slowest",
            serde_json::json!({ "task_id": task_id, "top": top }),
        ),
        WorkflowCmd::Compare {
            task_id_a,
            task_id_b,
        } => (
            "cli-workflow-compare",
            serde_json::json!({ "task_id_a": task_id_a, "task_id_b": task_id_b }),
        ),
        WorkflowCmd::Resume { task_id } => (
            "cli-workflow-run",
            serde_json::json!({ "task_id": task_id, "resume": true }),
        ),
    }
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

    /// `cli-workflow-status` was registered in the daemon and documented as `touring workflow
    /// status <task_id>`, but the CLI had no such subcommand: the documented command answered
    /// "unrecognized subcommand" (live proof, 04/10/2026).
    #[test]
    fn status_parses_task_id() {
        let cli = WorkflowCli::try_parse_from(["workflow", "status", "task_7"]).expect("parse");
        let Some(WorkflowCmd::Status { task_id }) = cli.cmd else {
            panic!("expected Status");
        };
        assert_eq!(task_id, "task_7");
    }

    /// Each subcommand reaches the handler that answers it. `status` had no route at all, and
    /// `resume` reuses the run handler on purpose (`resume: true`), not `cli-workflow-resume`.
    #[test]
    fn every_subcommand_routes_to_its_handler() {
        let routed = |argv: &[&str]| {
            let cli = WorkflowCli::try_parse_from(argv).expect("parse");
            route(cli.cmd.unwrap_or_else(default_run))
        };
        let (handler, payload) = routed(&["workflow", "status", "task_7"]);
        assert_eq!(handler, "cli-workflow-status");
        assert_eq!(payload["task_id"], "task_7");
        assert_eq!(routed(&["workflow", "stats", "t"]).0, "cli-workflow-stats");
        assert_eq!(routed(&["workflow", "slowest", "t"]).0, "cli-workflow-slowest");
        assert_eq!(routed(&["workflow", "compare", "a", "b"]).0, "cli-workflow-compare");
        assert_eq!(routed(&["workflow"]).0, "cli-workflow-run");
        let (handler, payload) = routed(&["workflow", "resume", "t"]);
        assert_eq!((handler, payload["resume"].as_bool()), ("cli-workflow-run", Some(true)));
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
