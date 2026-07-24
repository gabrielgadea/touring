//! `touring inferlets list|run|install` — L7-B Gamma WASM inferlet observability.
//!
//! Provides visibility into the sandboxed WASM inferlet pools managed by the
//! daemon's `InferletService`. The `list` subcommand reports which inferlet
//! kinds are currently loaded; `run` dispatches evaluation to a specific kind;
//! `install` delegates to the handlers_inferlet installer.
//!
//! Wave P3-1.3 W4 (2026-06-11): migrated from manual `arg_or` to clap derive.

use super::daemon_query;
use super::handlers_inferlet;
use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "touring inferlets",
    bin_name = "touring inferlets",
    about = "WASM inferlet operations: list (default), run, install",
    disable_help_subcommand = true
)]
struct InferletsCli {
    #[command(subcommand)]
    cmd: Option<InferletsCmd>,
}

#[derive(Subcommand, Debug)]
enum InferletsCmd {
    /// List loaded InferletKind variants (default).
    List,
    /// Dispatch evaluation to a named inferlet.
    Run {
        /// Inferlet name (e.g. always_success, memory, pattern, classifier).
        name: String,
        /// Optional input string (remaining args joined).
        input: Vec<String>,
    },
    /// Install an inferlet (delegates to handlers_inferlet).
    Install {
        /// Remaining install args passed through verbatim.
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
}

/// Run the `inferlets` CLI subcommand dispatcher.
///
/// # Errors
///
/// Returns an error for unknown subcommands, missing required arguments,
/// or daemon communication failures.
pub fn run(args: &[String]) -> anyhow::Result<()> {
    let cli = match InferletsCli::try_parse_from(args.iter().skip(1)) {
        Ok(cli) => cli,
        Err(e) => e.exit(),
    };

    match cli.cmd.unwrap_or(InferletsCmd::List) {
        InferletsCmd::List => {
            handlers_inferlet::list(args)?;
        }
        InferletsCmd::Run { name, input } => {
            if name.is_empty() {
                anyhow::bail!(
                    "Usage: touring inferlets run <name> [<input>]\n\
                     Valid names: always_success, memory, pattern, classifier"
                );
            }
            let payload = serde_json::json!({
                "name": name,
                "input": input.join(" "),
            });
            let output = daemon_query("cli-inferlets-run", payload)?;
            println!("{output}");
        }
        InferletsCmd::Install { .. } => {
            // Pass original args through to handlers_inferlet::run — it expects
            // the full args slice starting with "touring inferlets install ...".
            handlers_inferlet::run(args)?;
        }
    }
    Ok(())
}

/// Expose this handler's clap Command for the completions aggregator (W7).
pub(super) fn command() -> clap::Command {
    use clap::CommandFactory;
    InferletsCli::command()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> InferletsCli {
        InferletsCli::try_parse_from(args).expect("args should parse")
    }

    #[test]
    fn bare_inferlets_defaults_to_list() {
        let cli = parse(&["inferlets"]);
        assert!(cli.cmd.is_none()); // run() maps None -> List
    }

    #[test]
    fn explicit_list_parses() {
        let cli = parse(&["inferlets", "list"]);
        assert!(matches!(cli.cmd, Some(InferletsCmd::List)));
    }

    #[test]
    fn run_parses_name_and_input() {
        let cli = parse(&["inferlets", "run", "memory", "some", "input"]);
        let Some(InferletsCmd::Run { name, input }) = cli.cmd else {
            panic!("expected Run");
        };
        assert_eq!(name, "memory");
        assert_eq!(input.join(" "), "some input");
    }

    #[test]
    fn run_with_no_input() {
        let cli = parse(&["inferlets", "run", "always_success"]);
        let Some(InferletsCmd::Run { name, input }) = cli.cmd else {
            panic!("expected Run");
        };
        assert_eq!(name, "always_success");
        assert!(input.is_empty());
    }

    #[test]
    fn run_missing_name_is_parse_error() {
        assert!(InferletsCli::try_parse_from(["inferlets", "run"]).is_err());
    }

    #[test]
    fn install_parses() {
        let cli = parse(&["inferlets", "install", "--path", "/tmp/foo.wasm"]);
        assert!(matches!(cli.cmd, Some(InferletsCmd::Install { .. })));
    }

    #[test]
    fn unknown_subcommand_is_parse_error() {
        assert!(InferletsCli::try_parse_from(["inferlets", "frobnicate"]).is_err());
    }
}
