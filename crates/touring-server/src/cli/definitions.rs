//! `touring definitions classify|nodetypes|semantic-search` — Semantic definition browser.
//!
//! Provides classify, nodetypes, and semantic-search subcommands that leverage
//! the touring-semantics crate for language-agnostic AST-based code intelligence.
//!
//! Wave P3-1.3 W6a (2026-06-11): migrated from manual `arg_or` positional
//! parsing to clap derive. The `run` signature is unchanged — receives full
//! argv slice; clap parses `args[1..]`.

use crate::cli::daemon_query;
use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "touring definitions",
    bin_name = "touring definitions",
    about = "Semantic definition browser and cross-reference (classify, nodetypes, semantic-search)",
    disable_help_subcommand = true
)]
struct DefinitionsCli {
    #[command(subcommand)]
    cmd: Option<DefinitionsCmd>,
}

#[derive(Subcommand, Debug)]
enum DefinitionsCmd {
    /// Classify all definitions in a file (default).
    Classify {
        /// Source file to classify.
        #[arg(default_value = "")]
        file: String,
    },
    /// List node types present in a file.
    Nodetypes {
        /// Source file to inspect.
        #[arg(default_value = "")]
        file: String,
    },
    /// Semantic similarity search across definitions.
    SemanticSearch {
        /// Free-text query.
        #[arg(default_value = "")]
        query: String,
        /// Maximum number of results.
        #[arg(long, default_value = "20")]
        limit: usize,
    },
}
/// Run the definitions subcommand.
pub fn run(args: &[String]) -> anyhow::Result<()> {
    let cli = match DefinitionsCli::try_parse_from(args.iter().skip(1)) {
        Ok(cli) => cli,
        Err(e) => e.exit(),
    };
    match cli.cmd.unwrap_or(DefinitionsCmd::Classify {
        file: String::new(),
    }) {
        DefinitionsCmd::Classify { file } => {
            let payload = serde_json::json!({ "file" : file });
            let output = daemon_query("cli-definitions-classify", payload)?;
            println!("{}", output);
        }
        DefinitionsCmd::Nodetypes { file } => {
            let payload = serde_json::json!({ "file" : file });
            let output = daemon_query("cli-definitions-nodetypes", payload)?;
            println!("{}", output);
        }
        DefinitionsCmd::SemanticSearch { query, limit } => {
            let payload = serde_json::json!({ "query" : query, "limit" : limit });
            let output = daemon_query("cli-definitions-semantic-search", payload)?;
            println!("{}", output);
        }
    }
    Ok(())
}

/// Expose this handler's clap Command for the completions aggregator (W7).
pub(super) fn command() -> clap::Command {
    use clap::CommandFactory;
    DefinitionsCli::command()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn s(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|p| p.to_string()).collect()
    }
    #[test]
    fn defaults_to_classify() {
        let cli = DefinitionsCli::try_parse_from(s(&["definitions"]).iter()).unwrap();
        let cmd = cli.cmd.unwrap_or(DefinitionsCmd::Classify {
            file: String::new(),
        });
        assert!(matches!(cmd, DefinitionsCmd::Classify { .. }));
    }
    #[test]
    fn classify_with_file() {
        let cli =
            DefinitionsCli::try_parse_from(s(&["definitions", "classify", "src/lib.rs"]).iter())
                .unwrap();
        assert!(
            matches!(cli.cmd.unwrap(), DefinitionsCmd::Classify { file } if file ==
            "src/lib.rs")
        );
    }
    #[test]
    fn nodetypes_with_file() {
        let cli =
            DefinitionsCli::try_parse_from(s(&["definitions", "nodetypes", "src/main.rs"]).iter())
                .unwrap();
        assert!(
            matches!(cli.cmd.unwrap(), DefinitionsCmd::Nodetypes { file } if file ==
            "src/main.rs")
        );
    }
    #[test]
    fn semantic_search_default_limit_is_20() {
        let cli = DefinitionsCli::try_parse_from(
            s(&["definitions", "semantic-search", "orphan detection"]).iter(),
        )
        .unwrap();
        assert!(
            matches!(cli.cmd.unwrap(), DefinitionsCmd::SemanticSearch { limit, .. } if
            limit == 20)
        );
    }
    #[test]
    fn semantic_search_explicit_limit() {
        let cli = DefinitionsCli::try_parse_from(
            s(&["definitions", "semantic-search", "query", "--limit", "5"]).iter(),
        )
        .unwrap();
        assert!(
            matches!(cli.cmd.unwrap(), DefinitionsCmd::SemanticSearch { limit, .. } if
            limit == 5)
        );
    }
    #[test]
    fn unknown_subcommand_is_rejected() {
        let result = DefinitionsCli::try_parse_from(s(&["definitions", "bogus"]).iter());
        assert!(result.is_err());
    }
}
