//! `touring file-knowledge extended|stats|populate|audit` — FileKnowledgeEnriched query.
//!
//! Returns the 23-field enriched metadata for a file, combining:
//! - Base 10 fields (file_knowledge table)
//! - 13 enrichment fields from 5 specialized tables (cognitive, ecosystem, blake3, coverage, community)
//!
//! Wave P3-1.3 W6a (2026-06-11): migrated from manual `arg_or` positional
//! parsing to clap derive. The `run` signature is unchanged — receives full
//! argv slice; clap parses `args[1..]`.

use super::daemon_query;
use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "touring file-knowledge",
    bin_name = "touring file-knowledge",
    about = "FileKnowledgeEnriched query (extended, stats, populate, audit)",
    disable_help_subcommand = true
)]
struct FileKnowledgeCli {
    #[command(subcommand)]
    cmd: Option<FileKnowledgeCmd>,
}

#[derive(Subcommand, Debug)]
enum FileKnowledgeCmd {
    /// Query the 23-field enriched metadata for a file (default).
    Extended {
        /// Path of the file to query.
        file_path: String,
    },
    /// Aggregate file_knowledge statistics across the workspace.
    Stats,
    /// Upsert all workspace files into the file_knowledge table.
    Populate {
        /// Include hidden files.
        #[arg(long)]
        include_hidden: bool,
        /// Comma-separated list of extensions to include (e.g. `rs,py`).
        #[arg(long, value_delimiter = ',')]
        extensions: Option<Vec<String>>,
    },
    /// Report anomalies in the file_knowledge table.
    Audit,
}
/// Entry point for `touring file-knowledge <subcommand>`.
pub fn run(args: &[String]) -> anyhow::Result<()> {
    let cli = match FileKnowledgeCli::try_parse_from(args.iter().skip(1)) {
        Ok(cli) => cli,
        Err(e) => e.exit(),
    };
    match cli.cmd {
        None => {
            anyhow::bail!("Usage: touring file-knowledge extended <file_path>");
        }
        Some(FileKnowledgeCmd::Extended { file_path }) => {
            if file_path.is_empty() {
                anyhow::bail!("Usage: touring file-knowledge extended <file_path>");
            }
            let output = daemon_query(
                "cli-file-knowledge-extended",
                serde_json::json!({ "file_path" : file_path }),
            )?;
            println!("{output}");
        }
        Some(FileKnowledgeCmd::Stats) => {
            let output = daemon_query("cli-file-knowledge-stats", serde_json::json!({}))?;
            println!("{output}");
        }
        Some(FileKnowledgeCmd::Populate {
            include_hidden,
            extensions,
        }) => {
            let payload = serde_json::json!(
                { "include_hidden" : include_hidden, "extensions" : extensions
                .unwrap_or_default(), }
            );
            let output = daemon_query("cli-file-knowledge-populate", payload)?;
            println!("{output}");
        }
        Some(FileKnowledgeCmd::Audit) => {
            let output = daemon_query("cli-file-knowledge-audit", serde_json::json!({}))?;
            println!("{output}");
        }
    }
    Ok(())
}

/// Expose this handler's clap Command for the completions aggregator (W7).
pub(super) fn command() -> clap::Command {
    use clap::CommandFactory;
    FileKnowledgeCli::command()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn s(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|p| p.to_string()).collect()
    }
    #[test]
    fn extended_requires_file_path_positional() {
        let result = FileKnowledgeCli::try_parse_from(s(&["file-knowledge", "extended"]).iter());
        assert!(result.is_err());
    }
    #[test]
    fn extended_parses_file_path() {
        let cli = FileKnowledgeCli::try_parse_from(
            s(&["file-knowledge", "extended", "src/lib.rs"]).iter(),
        )
        .unwrap();
        assert!(
            matches!(cli.cmd.unwrap(), FileKnowledgeCmd::Extended { file_path } if
            file_path == "src/lib.rs")
        );
    }
    #[test]
    fn stats_parses() {
        let cli = FileKnowledgeCli::try_parse_from(s(&["file-knowledge", "stats"]).iter()).unwrap();
        assert!(matches!(cli.cmd.unwrap(), FileKnowledgeCmd::Stats));
    }
    #[test]
    fn populate_with_extensions() {
        let cli = FileKnowledgeCli::try_parse_from(
            s(&["file-knowledge", "populate", "--extensions=rs,py"]).iter(),
        )
        .unwrap();
        assert!(
            matches!(cli.cmd.unwrap(), FileKnowledgeCmd::Populate { extensions :
            Some(exts), .. } if exts == vec!["rs", "py"])
        );
    }
    #[test]
    fn populate_include_hidden_flag() {
        let cli = FileKnowledgeCli::try_parse_from(
            s(&["file-knowledge", "populate", "--include-hidden"]).iter(),
        )
        .unwrap();
        assert!(
            matches!(cli.cmd.unwrap(), FileKnowledgeCmd::Populate { include_hidden, .. }
            if include_hidden)
        );
    }
    #[test]
    fn audit_parses() {
        let cli = FileKnowledgeCli::try_parse_from(s(&["file-knowledge", "audit"]).iter()).unwrap();
        assert!(matches!(cli.cmd.unwrap(), FileKnowledgeCmd::Audit));
    }
    #[test]
    fn unknown_subcommand_is_rejected() {
        let result = FileKnowledgeCli::try_parse_from(s(&["file-knowledge", "bogus"]).iter());
        assert!(result.is_err());
    }
    #[test]
    fn no_subcommand_routes_to_none() {
        let cli = FileKnowledgeCli::try_parse_from(s(&["file-knowledge"]).iter()).unwrap();
        assert!(cli.cmd.is_none());
    }
}
