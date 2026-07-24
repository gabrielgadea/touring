//! `touring learning status|reward` — RL learning engine status and reward injection.
//!
//! Queries the daemon for LinUCB/QTable/OnlineRL status and allows
//! manual reward signal injection for feedback loops.
//!
//! Wave P3-1.3 W3 (2026-06-11): migrated from manual positional parsing
//! (`arg_or`) to clap derive. Dispatch contract unchanged.

use super::daemon_query;
use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "touring learning",
    bin_name = "touring learning",
    about = "RL learning engine: status query and manual reward injection",
    disable_help_subcommand = true
)]
struct LearningCli {
    #[command(subcommand)]
    cmd: Option<LearningCmd>,
}

#[derive(Subcommand, Debug)]
enum LearningCmd {
    /// Show LinUCB/QTable/OnlineRL engine status (default).
    Status,
    /// Inject a manual reward signal into the RL feedback loop.
    Reward {
        /// Tool name receiving the reward (e.g. `edit`, `speculate`).
        tool_name: String,
        /// Reward value (float, default 1.0).
        #[arg(default_value_t = 1.0)]
        reward: f64,
        /// Optional free-form context (remaining positional args, joined).
        context: Vec<String>,
    },
}

/// Entry point for the `touring learning` CLI handler — parses the argv slice
/// and dispatches `status` (default; queries the LinUCB/QTable/OnlineRL engine
/// via `cli-learning-status`) or `reward` (injects a manual reward signal for
/// a tool, defaulting to 1.0, via `cli-learning-reward`) to the daemon,
/// printing the JSON response.
pub fn run(args: &[String]) -> anyhow::Result<()> {
    let cli = match LearningCli::try_parse_from(args.iter().skip(1)) {
        Ok(cli) => cli,
        Err(e) => e.exit(),
    };

    match cli.cmd.unwrap_or(LearningCmd::Status) {
        LearningCmd::Status => {
            let output = daemon_query("cli-learning-status", serde_json::json!({}))?;
            println!("{output}");
        }
        LearningCmd::Reward {
            tool_name,
            reward,
            context,
        } => {
            let payload = serde_json::json!({
                "tool_name": tool_name,
                "reward": reward,
                "context": context.join(" "),
            });
            let output = daemon_query("cli-learning-reward", payload)?;
            println!("{output}");
        }
    }
    Ok(())
}

/// Expose this handler's clap Command for the completions aggregator (W7).
pub(super) fn command() -> clap::Command {
    use clap::CommandFactory;
    LearningCli::command()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> LearningCli {
        LearningCli::try_parse_from(args).expect("args should parse")
    }

    #[test]
    fn bare_learning_defaults_to_status() {
        let cli = parse(&["learning"]);
        assert!(cli.cmd.is_none()); // run() maps None -> Status
    }

    #[test]
    fn status_subcommand_parses() {
        let cli = parse(&["learning", "status"]);
        assert!(matches!(cli.cmd, Some(LearningCmd::Status)));
    }

    #[test]
    fn reward_parses_tool_name_and_value() {
        let cli = parse(&["learning", "reward", "edit", "1.0"]);
        let Some(LearningCmd::Reward {
            tool_name,
            reward,
            context,
        }) = cli.cmd
        else {
            panic!("expected Reward");
        };
        assert_eq!(tool_name, "edit");
        assert!((reward - 1.0).abs() < f64::EPSILON);
        assert!(context.is_empty());
    }

    #[test]
    fn reward_defaults_reward_to_1_0() {
        let cli = parse(&["learning", "reward", "speculate"]);
        let Some(LearningCmd::Reward { reward, .. }) = cli.cmd else {
            panic!("expected Reward");
        };
        assert!((reward - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn reward_captures_context_positionals() {
        let cli = parse(&["learning", "reward", "edit", "0.5", "successful", "impl"]);
        let Some(LearningCmd::Reward { context, .. }) = cli.cmd else {
            panic!("expected Reward");
        };
        assert_eq!(context.join(" "), "successful impl");
    }

    #[test]
    fn reward_missing_tool_name_is_parse_error() {
        assert!(LearningCli::try_parse_from(["learning", "reward"]).is_err());
    }

    #[test]
    fn reward_non_numeric_value_is_parse_error() {
        assert!(LearningCli::try_parse_from(["learning", "reward", "edit", "abc"]).is_err());
    }

    #[test]
    fn unknown_subcommand_is_parse_error() {
        assert!(LearningCli::try_parse_from(["learning", "frobnicate"]).is_err());
    }
}
