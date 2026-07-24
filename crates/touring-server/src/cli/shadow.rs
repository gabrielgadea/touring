//! `touring shadow validate` — Speculative validation in shadow branch.
//!
//! Validates code changes in a shadow/speculative context before applying.
//!
//! Wave P3-1.3 W2 (2026-06-11): migrated to clap derive (pilot pattern).

use super::daemon_query;
use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "touring shadow",
    bin_name = "touring shadow",
    about = "Speculative validation in a shadow branch",
    disable_help_subcommand = true
)]
struct ShadowCli {
    #[command(subcommand)]
    cmd: Option<ShadowCmd>,
}

#[derive(Subcommand, Debug)]
enum ShadowCmd {
    /// Validate a speculative change (reads file_path + content JSON from stdin).
    Validate,
}

/// Entry point for the `touring shadow` CLI handler — runs the `validate`
/// subcommand (the default), reading the `file_path`/`content` JSON payload from
/// stdin, dispatching the `cli-shadow-validate` daemon query for speculative
/// validation, and printing the response.
pub fn run(args: &[String]) -> anyhow::Result<()> {
    let cli = match ShadowCli::try_parse_from(args.iter().skip(1)) {
        Ok(cli) => cli,
        Err(e) => e.exit(),
    };
    match cli.cmd.unwrap_or(ShadowCmd::Validate) {
        ShadowCmd::Validate => {
            // Read from stdin for shadow validation (file_path and content)
            let payload = super::read_stdin_safe();
            let output = daemon_query("cli-shadow-validate", payload)?;
            println!("{output}");
        }
    }
    Ok(())
}

/// Expose this handler's clap Command for the completions aggregator (W7).
pub(super) fn command() -> clap::Command {
    use clap::CommandFactory;
    ShadowCli::command()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bare_shadow_defaults_to_validate() {
        let cli = ShadowCli::try_parse_from(["shadow"]).expect("parse");
        assert!(cli.cmd.is_none()); // run() maps None -> Validate
    }

    #[test]
    fn explicit_validate_parses() {
        let cli = ShadowCli::try_parse_from(["shadow", "validate"]).expect("parse");
        assert!(matches!(cli.cmd, Some(ShadowCmd::Validate)));
    }

    #[test]
    fn unknown_subcommand_is_parse_error() {
        assert!(ShadowCli::try_parse_from(["shadow", "frobnicate"]).is_err());
    }
}
