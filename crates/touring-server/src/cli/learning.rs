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
    /// Machine output: stdout is JSON even on failure (`{"error": …}`, exit 1).
    /// Every subcommand prints JSON already; the flag was rejected (exit 2,
    /// usage on stderr, EMPTY stdout), so a caller reading stdout took `-j` —
    /// the flag the rest of the CLI takes — for "no data" (18/09/2026).
    #[arg(short = 'j', long = "json", global = true)]
    json: bool,
}

#[derive(Subcommand, Debug)]
enum LearningCmd {
    /// Show LinUCB/QTable/OnlineRL engine status (default).
    Status,
    /// Inject a manual reward signal into the RL feedback loop.
    Reward {
        /// Tool name receiving the reward (e.g. `edit`, `speculate`).
        tool_name: String,
        /// Reward value (float, default 1.0). Negative values are penalties:
        /// `-0.5` was read as an unknown flag `-0` until 18/09/2026, so no
        /// penalty could be given from the CLI.
        #[arg(default_value_t = 1.0, allow_negative_numbers = true)]
        reward: f64,
        /// Optional free-form context (remaining positional args, joined).
        context: Vec<String>,
    },
    /// A/B experiment log (R4, 29/08): record a judged variant / read them back.
    #[command(subcommand)]
    Experiment(ExperimentCmd),
    /// P2 (29/08): replay the rewarded-outcome corpus into the OnlineRL engine
    /// (offline pretrain — the online per-tool trickle continues on the same
    /// engine). Durable cursor: each outcome is consumed once.
    Replay {
        /// Maximum outcomes to replay in this call.
        #[arg(long, default_value_t = 2000)]
        limit: u64,
        /// Count what WOULD be replayed without touching engine or cursor.
        #[arg(long, default_value_t = false)]
        dry_run: bool,
    },
}

#[derive(Subcommand, Debug)]
enum ExperimentCmd {
    /// Record one judged variant into the project's experiment log.
    Record {
        /// What was attempted (the variant text).
        #[arg(long)]
        variant: String,
        /// What the variant attacks (the target/rubric).
        #[arg(long)]
        target: String,
        /// Measured reward/score for the variant.
        #[arg(long, default_value_t = 0.0)]
        reward: f64,
        /// keep (passed the gate) or discard.
        #[arg(long, default_value = "keep")]
        decision: String,
        /// Session id for attribution.
        #[arg(long)]
        session_id: Option<String>,
    },
    /// List recorded experiments (newest first).
    List {
        /// Maximum rows to return.
        #[arg(long, default_value_t = 20)]
        limit: u64,
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

    let (hook, payload) = request_for(cli.cmd.unwrap_or(LearningCmd::Status));
    match daemon_query(hook, payload) {
        Ok(output) => println!("{output}"),
        Err(e) if cli.json => {
            println!(
                "{}",
                serde_json::json!({ "error": e.to_string(), "hook": hook })
            );
            std::process::exit(1);
        }
        Err(e) => return Err(e),
    }
    Ok(())
}

/// The daemon hook and payload a `learning` subcommand sends.
fn request_for(cmd: LearningCmd) -> (&'static str, serde_json::Value) {
    match cmd {
        LearningCmd::Status => ("cli-learning-status", serde_json::json!({})),
        LearningCmd::Reward {
            tool_name,
            reward,
            context,
        } => (
            "cli-learning-reward",
            serde_json::json!({
                "tool_name": tool_name,
                "reward": reward,
                "context": context.join(" "),
            }),
        ),
        LearningCmd::Experiment(ExperimentCmd::Record {
            variant,
            target,
            reward,
            decision,
            session_id,
        }) => (
            "cli-experiment-record",
            serde_json::json!({
                "variant": variant, "target": target, "reward": reward,
                "decision": decision, "session_id": session_id,
            }),
        ),
        LearningCmd::Experiment(ExperimentCmd::List { limit }) => {
            ("cli-experiment-list", serde_json::json!({ "limit": limit }))
        }
        LearningCmd::Replay { limit, dry_run } => (
            "cli-learning-replay",
            serde_json::json!({ "limit": limit, "dry_run": dry_run }),
        ),
    }
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

    /// 18/09/2026 (analise): `learning status -j` exited 2 with the usage on
    /// stderr and nothing on stdout. `-j` is the rest of the CLI's flag, so it
    /// is accepted before or after every subcommand.
    #[test]
    fn json_flag_is_accepted_everywhere() {
        for args in [
            &["learning", "status", "-j"][..],
            &["learning", "-j", "status"],
            &["learning", "--json"],
            &["learning", "reward", "edit", "0.5", "-j"],
            &["learning", "replay", "--dry-run", "-j"],
            &["learning", "experiment", "list", "-j"],
        ] {
            let cli = LearningCli::try_parse_from(args)
                .unwrap_or_else(|e| panic!("{args:?} rejected: {e}"));
            assert!(cli.json, "{args:?}");
        }
        assert!(!parse(&["learning", "status"]).json);
    }

    /// A penalty is a negative reward, and must not read as an unknown flag.
    #[test]
    fn reward_accepts_a_negative_value() {
        let cli = parse(&["learning", "reward", "edit", "-0.5"]);
        let Some(LearningCmd::Reward { reward, .. }) = cli.cmd else {
            panic!("expected Reward");
        };
        assert!((reward + 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn every_subcommand_names_its_daemon_hook() {
        assert_eq!(request_for(LearningCmd::Status).0, "cli-learning-status");
        let (hook, payload) = request_for(LearningCmd::Reward {
            tool_name: "edit".into(),
            reward: 0.5,
            context: vec!["a".into(), "b".into()],
        });
        assert_eq!(hook, "cli-learning-reward");
        assert_eq!(payload["context"], "a b");
        assert_eq!(
            request_for(LearningCmd::Replay {
                limit: 7,
                dry_run: true
            }),
            (
                "cli-learning-replay",
                serde_json::json!({ "limit": 7, "dry_run": true })
            )
        );
    }

    #[test]
    fn unknown_subcommand_is_parse_error() {
        assert!(LearningCli::try_parse_from(["learning", "frobnicate"]).is_err());
    }
}
