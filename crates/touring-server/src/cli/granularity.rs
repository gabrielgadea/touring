//! `touring granularity status|reset|hint` — Wave C1.7 + C2-D2 (2026-04-20).
//!
//! Inspects the granularity bandit that governs task split-factor decisions
//! made by the decomposer. The bandit lives in the daemon's `HookRuntime`;
//! this subcommand forwards to the `cli-granularity-*` hook handlers.
//!
//! The `hint` subcommand queries `reasoning::granularity_adapter` to obtain
//! a split recommendation for a given task size, language, and CILA level.
//!
//! Wave P3-1.3 W4 (2026-06-11): migrated from manual `arg_or` to clap derive.
//! `query_granularity_hint` is called directly (no daemon) — no payload change.

use super::daemon_query;
use crate::reasoning::{GranularityHint, query_granularity_hint};
use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "touring granularity",
    bin_name = "touring granularity",
    about = "GranularityBandit: status (default), reset, hint",
    disable_help_subcommand = true
)]
struct GranularityCli {
    #[command(subcommand)]
    cmd: Option<GranularityCmd>,
}

#[derive(Subcommand, Debug)]
enum GranularityCmd {
    /// Show current bandit arm weights and statistics (default).
    Status,
    /// Reset the bandit to uniform priors.
    Reset,
    /// Query the bandit for a split recommendation.
    ///
    /// Usage: `touring granularity hint [size_loc] [language] [cila_level]`
    Hint {
        /// Task size in lines of code (default: 200).
        #[arg(default_value_t = 200usize)]
        size_loc: usize,
        /// Programming language (default: rust).
        #[arg(default_value = "rust")]
        language: String,
        /// CILA complexity level 0-4 (default: 1).
        #[arg(default_value_t = 1u8)]
        cila_level: u8,
    },
}

/// Entry point for the `touring granularity …` subcommand.
pub fn run(args: &[String]) -> anyhow::Result<()> {
    let cli = match GranularityCli::try_parse_from(args.iter().skip(1)) {
        Ok(cli) => cli,
        Err(e) => e.exit(),
    };

    match cli.cmd.unwrap_or(GranularityCmd::Status) {
        GranularityCmd::Status => {
            let output = daemon_query("cli-granularity-status", serde_json::json!({}))?;
            println!("{output}");
        }
        GranularityCmd::Reset => {
            let output = daemon_query("cli-granularity-reset", serde_json::json!({}))?;
            println!("{output}");
        }
        GranularityCmd::Hint {
            size_loc,
            language,
            cila_level,
        } => {
            let hint: GranularityHint = query_granularity_hint(size_loc, &language, cila_level);
            let output = serde_json::json!({
                "split_factor":  hint.split_factor,
                "subtask_count": hint.subtask_count,
                "size_loc":      hint.size_loc,
                "language":      hint.language,
                "cila_level":    hint.cila_level,
            });
            println!("{output}");
        }
    }
    Ok(())
}

/// Expose this handler's clap Command for the completions aggregator (W7).
pub(super) fn command() -> clap::Command {
    use clap::CommandFactory;
    GranularityCli::command()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> GranularityCli {
        GranularityCli::try_parse_from(args).expect("args should parse")
    }

    #[test]
    fn bare_granularity_defaults_to_status() {
        let cli = parse(&["granularity"]);
        assert!(cli.cmd.is_none()); // run() maps None -> Status
    }

    #[test]
    fn explicit_status_parses() {
        let cli = parse(&["granularity", "status"]);
        assert!(matches!(cli.cmd, Some(GranularityCmd::Status)));
    }

    #[test]
    fn explicit_reset_parses() {
        let cli = parse(&["granularity", "reset"]);
        assert!(matches!(cli.cmd, Some(GranularityCmd::Reset)));
    }

    #[test]
    fn hint_with_all_args() {
        let cli = parse(&["granularity", "hint", "300", "rust", "2"]);
        let Some(GranularityCmd::Hint {
            size_loc,
            language,
            cila_level,
        }) = cli.cmd
        else {
            panic!("expected Hint");
        };
        assert_eq!(size_loc, 300);
        assert_eq!(language, "rust");
        assert_eq!(cila_level, 2);
    }

    #[test]
    fn hint_uses_defaults_when_no_args() {
        let cli = parse(&["granularity", "hint"]);
        let Some(GranularityCmd::Hint {
            size_loc,
            language,
            cila_level,
        }) = cli.cmd
        else {
            panic!("expected Hint");
        };
        assert_eq!(size_loc, 200);
        assert_eq!(language, "rust");
        assert_eq!(cila_level, 1);
    }

    #[test]
    fn hint_subcommand_returns_ok_without_daemon() {
        let args: Vec<String> = vec![
            "touring".into(),
            "granularity".into(),
            "hint".into(),
            "300".into(),
            "rust".into(),
            "2".into(),
        ];
        let result = run(&args);
        assert!(result.is_ok(), "hint subcommand must not error: {result:?}");
    }

    #[test]
    fn hint_subcommand_uses_defaults_when_args_absent() {
        let args: Vec<String> = vec!["touring".into(), "granularity".into(), "hint".into()];
        let result = run(&args);
        assert!(
            result.is_ok(),
            "hint with defaults must not error: {result:?}"
        );
    }

    #[test]
    fn unknown_subcommand_is_parse_error() {
        assert!(GranularityCli::try_parse_from(["granularity", "frobnicate"]).is_err());
    }
}
