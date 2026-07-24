//! `touring tantivy search|fuzzy|stats|suggest|reindex|gain|discover` — Tantivy BM25 full-text search CLI.
//!
//! Delegates all operations to the daemon via the `cli-tantivy-*` hook handlers,
//! which are feature-gated under `tantivy-fts` in `touring-hooks`.
//!
//! Wave P3-1.3 W4 (2026-06-11): migrated from manual `arg_or` to clap derive.
//! The `reindex` batched loop is preserved verbatim inside the match arm (G6).
//! `gain` and `discover` delegate to `super::gain::run(args)` / `super::discover::run(args)`
//! with the original full `args` slice (unchanged from W3 pattern).

use super::daemon_query;
use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "touring tantivy",
    bin_name = "touring tantivy",
    about = "Tantivy BM25 full-text search: stats (default), search, fuzzy, suggest, reindex, gain, discover",
    disable_help_subcommand = true
)]
struct TantivyCli {
    #[command(subcommand)]
    cmd: Option<TantivyCmd>,
}

#[derive(Subcommand, Debug)]
enum TantivyCmd {
    /// Show Tantivy index statistics (default).
    Stats,
    /// BM25 full-text search.
    Search {
        /// Query string (single token; for multi-word use quotes at shell).
        query: String,
        /// Number of results to return (default: 10).
        #[arg(default_value_t = 10usize)]
        top: usize,
    },
    /// Fuzzy BM25 search with edit-distance tolerance.
    Fuzzy {
        /// Query string.
        query: String,
        /// Maximum edit distance (default: 2).
        #[arg(default_value_t = 2u8)]
        distance: u8,
        /// Number of results to return (default: 10).
        #[arg(default_value_t = 10usize)]
        top: usize,
    },
    /// Prefix-based autocomplete suggestions.
    Suggest {
        /// Prefix string.
        prefix: String,
        /// Number of suggestions (default: 10).
        #[arg(default_value_t = 10usize)]
        top: usize,
    },
    /// Reindex all documents (batched by default; use --full for single call).
    Reindex {
        /// Use legacy single-call mode (may time out on large indexes).
        #[arg(long)]
        full: bool,
        /// Batch size per daemon call (default: 25000).
        #[arg(long, default_value_t = 25_000u64)]
        batch_size: u64,
        /// Start from offset N (skip clear).
        #[arg(long, default_value_t = 0u64)]
        resume: u64,
        /// Alias for --resume.
        #[arg(long, default_value_t = 0u64)]
        offset: u64,
    },
    /// RTK parity analytics (delegates to gain submodule).
    Gain {
        /// Remaining args passed through to gain::run.
        args: Vec<String>,
    },
    /// RTK discovery analytics (delegates to discover submodule).
    Discover {
        /// Remaining args passed through to discover::run.
        args: Vec<String>,
    },
}

/// Run the `tantivy` CLI subcommand dispatcher.
///
/// # Errors
///
/// Returns an error if the daemon socket is unreachable, the daemon reports a
/// failure, or required arguments are missing.
pub fn run(args: &[String]) -> anyhow::Result<()> {
    let cli = match TantivyCli::try_parse_from(args.iter().skip(1)) {
        Ok(cli) => cli,
        Err(e) => e.exit(),
    };

    match cli.cmd.unwrap_or(TantivyCmd::Stats) {
        TantivyCmd::Stats => {
            let output = daemon_query("cli-tantivy-stats", serde_json::json!({}))?;
            println!("{output}");
            if serde_json::from_str::<serde_json::Value>(&output)
                .ok()
                .and_then(|v| v.get("error").cloned())
                .is_some()
            {
                anyhow::bail!("tantivy stats error (see output above)");
            }
        }
        TantivyCmd::Search { query, top } => {
            if query.is_empty() {
                anyhow::bail!("Usage: touring tantivy search <query> [top_k]");
            }
            let output = daemon_query(
                "cli-tantivy-search",
                serde_json::json!({"query": query, "top": top}),
            )?;
            println!("{output}");
            if serde_json::from_str::<serde_json::Value>(&output)
                .ok()
                .and_then(|v| v.get("error").cloned())
                .is_some()
            {
                anyhow::bail!("tantivy search error (see output above)");
            }
        }
        TantivyCmd::Fuzzy {
            query,
            distance,
            top,
        } => {
            if query.is_empty() {
                anyhow::bail!("Usage: touring tantivy fuzzy <query> [distance] [top_k]");
            }
            let output = daemon_query(
                "cli-tantivy-fuzzy",
                serde_json::json!({"query": query, "distance": distance, "top": top}),
            )?;
            println!("{output}");
            if serde_json::from_str::<serde_json::Value>(&output)
                .ok()
                .and_then(|v| v.get("error").cloned())
                .is_some()
            {
                anyhow::bail!("tantivy fuzzy error (see output above)");
            }
        }
        TantivyCmd::Suggest { prefix, top } => {
            if prefix.is_empty() {
                anyhow::bail!("Usage: touring tantivy suggest <prefix> [top_k]");
            }
            let output = daemon_query(
                "cli-tantivy-suggest",
                serde_json::json!({"prefix": prefix, "top": top}),
            )?;
            println!("{output}");
            if serde_json::from_str::<serde_json::Value>(&output)
                .ok()
                .and_then(|v| v.get("error").cloned())
                .is_some()
            {
                anyhow::bail!("tantivy suggest error (see output above)");
            }
        }
        TantivyCmd::Reindex {
            full,
            batch_size,
            resume,
            offset,
        } => {
            // Effective resume offset: --offset is alias for --resume; take max.
            let resume_offset = resume.max(offset);

            if full {
                let output =
                    daemon_query("cli-tantivy-reindex", serde_json::json!({"mode": "full"}))?;
                println!("{output}");
                return Ok(());
            }

            // Batched mode: loop client-side.
            // Each batch is a bounded daemon call (fits in light-hook budget, no socket timeout).
            let mut off = resume_offset;
            let mut total_upserted: u64 = 0;
            let mut batch_count: u64 = 0;
            let start_instant = std::time::Instant::now();
            loop {
                let clear = off == 0 && resume_offset == 0 && batch_count == 0;
                let output = daemon_query(
                    "cli-tantivy-reindex",
                    serde_json::json!({
                        "mode": "batch",
                        "offset": off,
                        "limit": batch_size,
                        "clear": clear,
                    }),
                )?;
                let parsed: serde_json::Value = serde_json::from_str(&output)?;
                let reindexed = parsed
                    .get("reindexed")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                if !reindexed {
                    println!("{output}");
                    anyhow::bail!("batch reindex failed at offset {off}");
                }
                let upserted = parsed.get("upserted").and_then(|v| v.as_u64()).unwrap_or(0);
                let next_offset = parsed
                    .get("next_offset")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(off);
                let done = parsed
                    .get("done")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                total_upserted += upserted;
                batch_count += 1;
                eprintln!(
                    "  batch {} offset={} upserted={} total_upserted={} elapsed={:?}",
                    batch_count,
                    off,
                    upserted,
                    total_upserted,
                    start_instant.elapsed()
                );
                if done || next_offset == off {
                    let summary = serde_json::json!({
                        "reindexed": true,
                        "mode": "batch",
                        "batches": batch_count,
                        "total_upserted": total_upserted,
                        "final_offset": next_offset,
                        "elapsed_secs": start_instant.elapsed().as_secs_f64(),
                        "final_stats": parsed.get("stats").cloned().unwrap_or(serde_json::Value::Null),
                    });
                    println!("{}", summary);
                    return Ok(());
                }
                off = next_offset;
            }
        }
        // Delegate to sibling submodules with original args (G6: no dispatch change).
        TantivyCmd::Gain { .. } => super::gain::run(args)?,
        TantivyCmd::Discover { .. } => super::discover::run(args)?,
    }
    Ok(())
}

/// Expose this handler's clap Command for the completions aggregator (W7).
pub(super) fn command() -> clap::Command {
    use clap::CommandFactory;
    TantivyCli::command()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|p| p.to_string()).collect()
    }

    fn parse(args: &[&str]) -> TantivyCli {
        TantivyCli::try_parse_from(args).expect("args should parse")
    }

    #[test]
    fn defaults_to_stats_when_no_subcommand() {
        // stats → daemon_query: if daemon unavailable this errors but NOT with "Unknown".
        let args = s(&["touring", "tantivy"]);
        let result = run(&args);
        if let Err(e) = result {
            let msg = e.to_string();
            assert!(
                !msg.contains("Unknown tantivy subcommand"),
                "should route to stats, not reject: {msg}"
            );
        }
    }

    #[test]
    fn rejects_unknown_subcommand() {
        // clap handles unknown subcommands as parse errors (exits, can't easily unit-test run()).
        assert!(TantivyCli::try_parse_from(["tantivy", "nonexistent"]).is_err());
    }

    #[test]
    fn search_requires_non_empty_query_at_runtime() {
        // clap requires the positional, so empty string must be passed explicitly.
        let args = s(&["touring", "tantivy", "search", ""]);
        let result = run(&args);
        assert!(result.is_err());
        let msg = result.expect_err("should error").to_string();
        assert!(msg.contains("Usage"), "should show usage: {msg}");
    }

    #[test]
    fn search_missing_query_is_parse_error() {
        assert!(TantivyCli::try_parse_from(["tantivy", "search"]).is_err());
    }

    #[test]
    fn fuzzy_missing_query_is_parse_error() {
        assert!(TantivyCli::try_parse_from(["tantivy", "fuzzy"]).is_err());
    }

    #[test]
    fn suggest_missing_prefix_is_parse_error() {
        assert!(TantivyCli::try_parse_from(["tantivy", "suggest"]).is_err());
    }

    #[test]
    fn search_parses_query_and_default_top() {
        let cli = parse(&["tantivy", "search", "HookRuntime"]);
        let Some(TantivyCmd::Search { query, top }) = cli.cmd else {
            panic!("expected Search");
        };
        assert_eq!(query, "HookRuntime");
        assert_eq!(top, 10);
    }

    #[test]
    fn search_parses_explicit_top() {
        let cli = parse(&["tantivy", "search", "foo", "5"]);
        let Some(TantivyCmd::Search { top, .. }) = cli.cmd else {
            panic!("expected Search");
        };
        assert_eq!(top, 5);
    }

    #[test]
    fn fuzzy_parses_defaults() {
        let cli = parse(&["tantivy", "fuzzy", "HokRuntime"]);
        let Some(TantivyCmd::Fuzzy {
            query,
            distance,
            top,
        }) = cli.cmd
        else {
            panic!("expected Fuzzy");
        };
        assert_eq!(query, "HokRuntime");
        assert_eq!(distance, 2);
        assert_eq!(top, 10);
    }

    #[test]
    fn fuzzy_parses_explicit_distance_and_top() {
        let cli = parse(&["tantivy", "fuzzy", "foo", "1", "20"]);
        let Some(TantivyCmd::Fuzzy { distance, top, .. }) = cli.cmd else {
            panic!("expected Fuzzy");
        };
        assert_eq!(distance, 1);
        assert_eq!(top, 20);
    }

    #[test]
    fn suggest_parses_prefix_and_default_top() {
        let cli = parse(&["tantivy", "suggest", "Hook"]);
        let Some(TantivyCmd::Suggest { prefix, top }) = cli.cmd else {
            panic!("expected Suggest");
        };
        assert_eq!(prefix, "Hook");
        assert_eq!(top, 10);
    }

    #[test]
    fn reindex_defaults() {
        let cli = parse(&["tantivy", "reindex"]);
        let Some(TantivyCmd::Reindex {
            full,
            batch_size,
            resume,
            offset,
        }) = cli.cmd
        else {
            panic!("expected Reindex");
        };
        assert!(!full);
        assert_eq!(batch_size, 25_000);
        assert_eq!(resume, 0);
        assert_eq!(offset, 0);
    }

    #[test]
    fn reindex_full_flag() {
        let cli = parse(&["tantivy", "reindex", "--full"]);
        let Some(TantivyCmd::Reindex { full, .. }) = cli.cmd else {
            panic!("expected Reindex");
        };
        assert!(full);
    }

    #[test]
    fn reindex_batch_size_and_resume() {
        let cli = parse(&[
            "tantivy",
            "reindex",
            "--batch-size",
            "1000",
            "--resume",
            "5000",
        ]);
        let Some(TantivyCmd::Reindex {
            batch_size, resume, ..
        }) = cli.cmd
        else {
            panic!("expected Reindex");
        };
        assert_eq!(batch_size, 1000);
        assert_eq!(resume, 5000);
    }

    #[test]
    fn search_routes_to_daemon_when_query_given() {
        let args = s(&["touring", "tantivy", "search", "HookRuntime"]);
        let result = run(&args);
        if let Err(e) = result {
            let msg = e.to_string();
            assert!(
                !msg.contains("Usage"),
                "should not show usage when query provided: {msg}"
            );
        }
    }

    #[test]
    fn fuzzy_uses_default_distance_two() {
        let args = s(&["touring", "tantivy", "fuzzy", "HokRuntime"]);
        let result = run(&args);
        if let Err(e) = result {
            let msg = e.to_string();
            assert!(!msg.contains("Usage"), "should not show usage: {msg}");
        }
    }
}
