//! `touring e2e` — Comprehensive E2E code analysis test.
//!
//! CLI frontend that sends a single daemon query to exercise the full
//! Touring analysis pipeline against the current project.
//!
//! Wave P3-1.3 W3 (2026-06-11): migrated from manual `parse_global_flags` /
//! `flag_value` parsing to clap derive. The `-j` / `--json` flag and
//! `--depth` are now typed clap fields; all formatting logic is unchanged.

use super::common::{human_to_stderr, json_to_stdout};
use super::daemon_query;
use clap::Parser;

#[derive(Parser, Debug)]
#[command(
    name = "touring e2e",
    bin_name = "touring e2e",
    about = "Comprehensive E2E code analysis (index + AST + wiring + quality + learning)",
    disable_help_subcommand = true
)]
struct E2eCli {
    /// Analysis depth: quick, standard (default), or deep.
    #[arg(long, default_value = "standard")]
    depth: String,
    /// Output raw JSON to stdout (machine-readable).
    #[arg(short = 'j', long)]
    json: bool,
}

/// Entry point for the `touring e2e` CLI handler — parses the argv slice,
/// queries the daemon for the end-to-end composite health score, and prints
/// the result (human-readable, or raw JSON with `-j`).
pub fn run(args: &[String]) -> anyhow::Result<()> {
    let cli = match E2eCli::try_parse_from(args.iter().skip(1)) {
        Ok(cli) => cli,
        Err(e) => e.exit(),
    };

    let payload = serde_json::json!({
        "depth": cli.depth,
    });

    let output = daemon_query("cli-e2e", payload)?;

    if cli.json {
        json_to_stdout(&output);
    } else {
        // Parse the report and format human-readable output
        match serde_json::from_str::<touring_hooks::cli_e2e::E2eReport>(&output) {
            Ok(report) => {
                let formatted = touring_hooks::cli_e2e::format_human(&report);
                human_to_stderr(&formatted);
            }
            Err(_) => {
                // Fallback: pretty-print raw JSON
                match serde_json::from_str::<serde_json::Value>(&output) {
                    Ok(val) => {
                        if let Ok(pretty) = serde_json::to_string_pretty(&val) {
                            json_to_stdout(&pretty);
                        } else {
                            json_to_stdout(&output);
                        }
                    }
                    Err(_) => json_to_stdout(&output),
                }
            }
        }
    }

    Ok(())
}

/// Expose this handler's clap Command for the completions aggregator (W7).
pub(super) fn command() -> clap::Command {
    use clap::CommandFactory;
    E2eCli::command()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> E2eCli {
        E2eCli::try_parse_from(args).expect("args should parse")
    }

    #[test]
    fn defaults_to_standard_depth_and_no_json() {
        let cli = parse(&["e2e"]);
        assert_eq!(cli.depth, "standard");
        assert!(!cli.json);
    }

    #[test]
    fn parses_depth_flag() {
        let cli = parse(&["e2e", "--depth", "deep"]);
        assert_eq!(cli.depth, "deep");
    }

    #[test]
    fn parses_depth_equals_form() {
        let cli = parse(&["e2e", "--depth=quick"]);
        assert_eq!(cli.depth, "quick");
    }

    #[test]
    fn json_short_flag() {
        let cli = parse(&["e2e", "-j"]);
        assert!(cli.json);
    }

    #[test]
    fn json_long_flag() {
        let cli = parse(&["e2e", "--json"]);
        assert!(cli.json);
    }

    #[test]
    fn json_and_depth_together() {
        let cli = parse(&["e2e", "-j", "--depth", "quick"]);
        assert!(cli.json);
        assert_eq!(cli.depth, "quick");
    }

    #[test]
    fn unknown_flag_is_parse_error() {
        assert!(E2eCli::try_parse_from(["e2e", "--frobnicate"]).is_err());
    }
}
