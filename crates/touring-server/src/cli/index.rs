//! `touring index search|status|find|files|rebuild|ingest` — Symbol index queries + bulk rebuild.
//!
//! Queries the incremental symbol index for file/symbol discovery.
//! `rebuild` walks the project directory and indexes all supported code files.
//!
//! Wave P3-1.3 W4 (2026-06-11): migrated from manual `arg_or` + `parse_global_flags` to clap derive.
//! The `-j`/`--json` global flag is now a top-level field in `IndexCli` so it is consumed
//! by clap before positional args reach subcommands (preserves G6 behaviour where `-j`
//! must NOT bleed into search query / file pattern). Payload keys and hook names unchanged.

use super::daemon_query;
use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "touring index",
    bin_name = "touring index",
    about = "Symbol index: status (default), search, find, files, rebuild, ingest",
    disable_help_subcommand = true
)]
struct IndexCli {
    /// Emit JSON output (consumed here so it does not leak into subcommand positional args).
    #[arg(short = 'j', long = "json", global = true)]
    json: bool,

    #[command(subcommand)]
    cmd: Option<IndexCmd>,
}

#[derive(Subcommand, Debug)]
enum IndexCmd {
    /// Show index statistics (default).
    Status,
    /// BM25 full-text search across indexed symbols.
    Search {
        /// Query string (remaining args joined).
        query: Vec<String>,
    },
    /// Look up a symbol by name.
    Find {
        /// Symbol name to look up.
        symbol_name: String,
        /// Return only definition sites (pass "true").
        #[arg(default_value = "false")]
        definitions_only: String,
    },
    /// List indexed files matching an optional pattern.
    Files {
        /// Maximum number of results (default: 100, max: 10000).
        #[arg(long, default_value_t = 100u64)]
        limit: u64,
        /// Pattern to filter file paths (remaining args joined).
        pattern: Vec<String>,
    },
    /// Rebuild the symbol index from source.
    Rebuild {
        /// Directory to index (optional; daemon uses workspace root if absent).
        #[arg(long)]
        dir: Option<String>,
    },
    /// On-demand single-file reindex (B3, 2026-05-10).
    Ingest {
        /// Path of the file to reindex.
        path: String,
    },
}

/// Run the `index` CLI subcommand dispatcher.
///
/// # Errors
///
/// Returns an error if the daemon socket is unreachable or the daemon
/// reports a failure response.
pub fn run(args: &[String]) -> anyhow::Result<()> {
    let cli = match IndexCli::try_parse_from(args.iter().skip(1)) {
        Ok(cli) => cli,
        Err(e) => e.exit(),
    };

    match cli.cmd.unwrap_or(IndexCmd::Status) {
        IndexCmd::Status => {
            let output = daemon_query("cli-index-status", serde_json::json!({}))?;
            println!("{output}");
        }
        IndexCmd::Search { query } => {
            let q = query.join(" ");
            let payload = serde_json::json!({ "query": q });
            let output = daemon_query("cli-index-search", payload)?;
            println!("{output}");
        }
        IndexCmd::Find {
            symbol_name,
            definitions_only,
        } => {
            let defs_only = definitions_only == "true";
            let payload = serde_json::json!({
                "symbol_name": symbol_name,
                "definitions_only": defs_only,
            });
            let output = daemon_query("cli-index-find", payload)?;
            println!("{output}");
        }
        IndexCmd::Files { limit, pattern } => {
            let limit = limit.min(10_000);
            let pat = pattern.join(" ");
            let payload = serde_json::json!({ "pattern": pat, "limit": limit });
            let output = daemon_query("cli-index-files", payload)?;
            println!("{output}");
        }
        IndexCmd::Rebuild { dir } => {
            let payload = match dir {
                Some(d) => serde_json::json!({ "dir": d }),
                None => serde_json::json!({}),
            };
            let output = daemon_query("cli-index-rebuild", payload)?;
            println!("{output}");
        }
        IndexCmd::Ingest { path } => {
            if path.is_empty() {
                anyhow::bail!("index ingest requires <file>: usage `touring index ingest <path>`");
            }
            let payload = serde_json::json!({ "path": path });
            let output = daemon_query("cli-index-ingest", payload)?;
            println!("{output}");
        }
    }
    Ok(())
}

/// Expose this handler's clap Command for the completions aggregator (W7).
pub(super) fn command() -> clap::Command {
    use clap::CommandFactory;
    IndexCli::command()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|p| p.to_string()).collect()
    }

    fn parse(args: &[&str]) -> IndexCli {
        IndexCli::try_parse_from(args).expect("args should parse")
    }

    #[test]
    fn bare_index_defaults_to_status() {
        let cli = parse(&["index"]);
        assert!(cli.cmd.is_none()); // run() maps None -> Status
    }

    #[test]
    fn explicit_status_parses() {
        let cli = parse(&["index", "status"]);
        assert!(matches!(cli.cmd, Some(IndexCmd::Status)));
    }

    #[test]
    fn search_parses_multi_word_query() {
        let cli = parse(&["index", "search", "HookRuntime", "handler"]);
        let Some(IndexCmd::Search { query }) = cli.cmd else {
            panic!("expected Search");
        };
        assert_eq!(query.join(" "), "HookRuntime handler");
    }

    #[test]
    fn search_with_json_flag_does_not_bleed_into_query() {
        // -j must be consumed by IndexCli.json, NOT land in search query.
        let cli = parse(&["index", "-j", "search", "foo"]);
        assert!(cli.json);
        let Some(IndexCmd::Search { query }) = cli.cmd else {
            panic!("expected Search");
        };
        assert_eq!(query, &["foo"]);
    }

    #[test]
    fn find_parses_symbol_and_definitions_only() {
        let cli = parse(&["index", "find", "MyStruct", "true"]);
        let Some(IndexCmd::Find {
            symbol_name,
            definitions_only,
        }) = cli.cmd
        else {
            panic!("expected Find");
        };
        assert_eq!(symbol_name, "MyStruct");
        assert_eq!(definitions_only, "true");
    }

    #[test]
    fn find_defaults_definitions_only_to_false() {
        let cli = parse(&["index", "find", "foo"]);
        let Some(IndexCmd::Find {
            definitions_only, ..
        }) = cli.cmd
        else {
            panic!("expected Find");
        };
        assert_eq!(definitions_only, "false");
    }

    #[test]
    fn files_parses_limit_and_pattern() {
        let cli = parse(&["index", "files", "--limit", "50", "src/"]);
        let Some(IndexCmd::Files { limit, pattern }) = cli.cmd else {
            panic!("expected Files");
        };
        assert_eq!(limit, 50);
        assert_eq!(pattern.join(" "), "src/");
    }

    #[test]
    fn files_uses_default_limit_100() {
        let cli = parse(&["index", "files"]);
        let Some(IndexCmd::Files { limit, .. }) = cli.cmd else {
            panic!("expected Files");
        };
        assert_eq!(limit, 100);
    }

    #[test]
    fn rebuild_parses_dir() {
        let cli = parse(&["index", "rebuild", "--dir", "/tmp/proj"]);
        let Some(IndexCmd::Rebuild { dir }) = cli.cmd else {
            panic!("expected Rebuild");
        };
        assert_eq!(dir.as_deref(), Some("/tmp/proj"));
    }

    #[test]
    fn rebuild_no_dir_is_none() {
        let cli = parse(&["index", "rebuild"]);
        let Some(IndexCmd::Rebuild { dir }) = cli.cmd else {
            panic!("expected Rebuild");
        };
        assert!(dir.is_none());
    }

    #[test]
    fn ingest_parses_path() {
        let cli = parse(&["index", "ingest", "/home/user/foo.rs"]);
        let Some(IndexCmd::Ingest { path }) = cli.cmd else {
            panic!("expected Ingest");
        };
        assert_eq!(path, "/home/user/foo.rs");
    }

    #[test]
    fn ingest_missing_path_is_parse_error() {
        assert!(IndexCli::try_parse_from(["index", "ingest"]).is_err());
    }

    #[test]
    fn unknown_subcommand_is_parse_error() {
        assert!(IndexCli::try_parse_from(["index", "frobnicate"]).is_err());
    }

    #[test]
    fn ingest_empty_path_errors_at_runtime() {
        // clap requires path positional so empty string only comes from weird input;
        // test the run() guard separately.
        let args = s(&["touring", "index", "ingest", ""]);
        let result = run(&args);
        assert!(result.is_err());
        let msg = result.expect_err("").to_string();
        assert!(msg.contains("ingest requires"));
    }
}
