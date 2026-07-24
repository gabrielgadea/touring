//! `touring find-references <file>:<line>:<col> [--scope project|workspace]`
//!
//! Finds all references to the symbol at the given position.
//!
//! Wave P3-1.3 W6a (2026-06-11): migrated from manual `arg_or` positional
//! parsing to clap derive. The `run` signature is unchanged — receives full
//! argv slice; clap parses `args[1..]`.

use super::daemon_query;
use clap::Parser;

#[derive(Parser, Debug)]
#[command(
    name = "touring find-references",
    bin_name = "touring find-references",
    about = "Find all references to the symbol at a given file:line:col position",
    disable_help_subcommand = true
)]
struct FindReferencesCli {
    /// Position in `file:line:col` format (e.g. `src/lib.rs:42:5`).
    position: String,
    /// Scope of the reference search.
    #[arg(long, default_value = "workspace")]
    scope: String,
}

fn parse_position(s: &str) -> anyhow::Result<(String, usize, usize)> {
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() != 3 {
        anyhow::bail!("Position must be <file>:<line>:<col>, got: {}", s);
    }
    let file = parts[0].to_string();
    let line = parts[1]
        .parse::<usize>()
        .map_err(|_| anyhow::anyhow!("Invalid line: {}", parts[1]))?;
    let col = parts[2]
        .parse::<usize>()
        .map_err(|_| anyhow::anyhow!("Invalid column: {}", parts[2]))?;
    Ok((file, line, col))
}
/// Entry point for the `touring find-references` CLI handler — parses the
/// `<file>:<line>:<col>` position and optional `--scope project|workspace`
/// (default `workspace`), then queries the `cli-find-references` daemon hook
/// for all references to the symbol at that position, printing the JSON
/// response.
pub fn run(args: &[String]) -> anyhow::Result<()> {
    let cli = match FindReferencesCli::try_parse_from(args.iter().skip(1)) {
        Ok(cli) => cli,
        // Real parse errors (e.g. missing positional) return Err so direct callers
        // (unit tests invoking `run`) observe it instead of the process exiting;
        // --help/--version still print to stdout and exit 0 via clap.
        Err(e) if e.use_stderr() => return Err(anyhow::anyhow!("{e}")),
        Err(e) => e.exit(),
    };
    let scope = if cli.scope == "project" {
        "project"
    } else {
        "workspace"
    };
    let (file, line, col) = parse_position(&cli.position)?;
    let payload = serde_json::json!(
        { "file" : file, "line" : line, "column" : col, "scope" : scope }
    );
    let output = daemon_query("cli-find-references", payload)?;
    println!("{output}");
    Ok(())
}

/// Expose this handler's clap Command for the completions aggregator (W7).
pub(super) fn command() -> clap::Command {
    use clap::CommandFactory;
    FindReferencesCli::command()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn s(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|p| p.to_string()).collect()
    }
    #[test]
    fn parses_position_correctly() {
        let (file, line, col) = parse_position("src/lib.rs:42:5").unwrap();
        assert_eq!(file, "src/lib.rs");
        assert_eq!(line, 42);
        assert_eq!(col, 5);
    }
    #[test]
    fn position_invalid_format_is_rejected() {
        assert!(parse_position("src/lib.rs:42").is_err());
        assert!(parse_position("src/lib.rs").is_err());
    }
    #[test]
    fn position_non_numeric_line_is_rejected() {
        assert!(parse_position("src/lib.rs:abc:5").is_err());
    }
    #[test]
    fn default_scope_is_workspace() {
        let cli =
            FindReferencesCli::try_parse_from(s(&["find-references", "src/lib.rs:10:3"]).iter())
                .unwrap();
        assert_eq!(cli.scope, "workspace");
    }
    #[test]
    fn explicit_scope_project() {
        let cli = FindReferencesCli::try_parse_from(
            s(&["find-references", "src/lib.rs:10:3", "--scope", "project"]).iter(),
        )
        .unwrap();
        assert_eq!(cli.scope, "project");
    }
    #[test]
    fn missing_position_is_rejected() {
        let result = FindReferencesCli::try_parse_from(s(&["find-references"]).iter());
        assert!(result.is_err());
    }
}
