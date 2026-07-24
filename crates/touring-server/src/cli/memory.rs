//! `touring memory stats|recall|store|list|reindex` — Memory database queries and storage.
//!
//! Provides CLI access to the Touring knowledge memory system:
//! - `stats`   — Summary statistics of the memory database (default)
//! - `recall`  — Query entries by semantic similarity
//! - `store`   — Persist a new key-value entry with optional tier and type
//! - `list`    — List entries with optional limit and sort control
//! - `reindex` — Backfill the ANN corpus from existing memory_entries (S-04 2026-05-29)
//!
//! Wave P3-1.3 W3 (2026-06-11): migrated from manual positional / flag-loop
//! parsing to clap derive. Dispatch contract and all payload keys unchanged (G6).
//! `collect_value_args` is preserved for store's --tier/--type stripping logic.

use super::daemon_query;
use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "touring memory",
    bin_name = "touring memory",
    about = "Memory database: stats, recall, store, list, reindex",
    disable_help_subcommand = true
)]
struct MemoryCli {
    #[command(subcommand)]
    cmd: Option<MemoryCmd>,
}

#[derive(Subcommand, Debug)]
enum MemoryCmd {
    /// Show memory database summary statistics (default).
    Stats,
    /// Query entries by semantic similarity.
    Recall {
        /// Query words (remaining positional args, joined).
        query: Vec<String>,
    },
    /// Persist a new key-value entry.
    Store {
        /// Memory key.
        key: String,
        /// Value words (remaining positional args, stripped of --tier/--type pairs).
        value: Vec<String>,
        /// Memory tier (default: semantic).
        #[arg(long, default_value = "semantic")]
        tier: String,
        /// Entry type (default: lesson).
        #[arg(long = "type", default_value = "lesson")]
        entry_type: String,
    },
    /// List entries with optional pagination and sort control.
    List {
        /// Maximum number of results (default: 20).
        #[arg(long, default_value_t = 20u64)]
        limit: u64,
        /// Sort field (default: access_count).
        #[arg(long, default_value = "access_count")]
        sort: String,
    },
    /// Backfill the ANN corpus from existing memory_entries (S-04 2026-05-29).
    Reindex {
        /// Batch size for upsert operations (default: 256).
        #[arg(long = "batch-size", default_value_t = 256u64)]
        batch_size: u64,
    },
}

/// Entry point for the `touring memory` CLI handler — dispatches the
/// `stats` (default), `recall`, `store`, `list`, and `reindex` subcommands to
/// their respective `cli-memory-*` daemon queries and prints each response.
/// `store` requires a non-empty key and value; clap strips `--tier`/`--type`
/// from the value words before joining.
pub fn run(args: &[String]) -> anyhow::Result<()> {
    let cli = match MemoryCli::try_parse_from(args.iter().skip(1)) {
        Ok(cli) => cli,
        Err(e) => e.exit(),
    };

    match cli.cmd.unwrap_or(MemoryCmd::Stats) {
        MemoryCmd::Stats => {
            let output = daemon_query("cli-memory-stats", serde_json::json!({}))?;
            println!("{output}");
        }
        MemoryCmd::Recall { query } => {
            let output = daemon_query(
                "cli-memory-recall",
                serde_json::json!({ "query": query.join(" ") }),
            )?;
            println!("{output}");
        }
        MemoryCmd::Store {
            key,
            value,
            tier,
            entry_type,
        } => {
            if key.is_empty() {
                anyhow::bail!(
                    "Usage: touring memory store <key> <value...> [--tier <tier>] [--type <type>]"
                );
            }
            // clap has already stripped --tier/--type from `value` (they are named args).
            // Replicate the original collect_value_args join for G6 byte-compat.
            let value_text = value.join(" ");
            if value_text.is_empty() {
                anyhow::bail!(
                    "Usage: touring memory store <key> <value...> [--tier <tier>] [--type <type>]"
                );
            }
            let payload = serde_json::json!({
                "key": key,
                "value": value_text,
                "tier": tier,
                "entry_type": entry_type,
            });
            let output = daemon_query("cli-memory-store", payload)?;
            println!("{output}");
        }
        MemoryCmd::List { limit, sort } => {
            let payload = serde_json::json!({
                "limit": limit,
                "sort_by": sort,
            });
            let output = daemon_query("cli-memory-list", payload)?;
            println!("{output}");
        }
        MemoryCmd::Reindex { batch_size } => {
            let output = daemon_query(
                "cli-memory-reindex",
                serde_json::json!({ "batch_size": batch_size }),
            )?;
            println!("{output}");
        }
    }
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Internal helpers (preserved for backwards-compat with existing tests)
// ─────────────────────────────────────────────────────────────────────────────

/// Known flags that take a value argument (used to skip flag pairs in legacy tests).
/// Expose this handler's clap Command for the completions aggregator (W7).
pub(super) fn command() -> clap::Command {
    use clap::CommandFactory;
    MemoryCli::command()
}

#[cfg(test)]
const KNOWN_FLAGS: &[&str] = &["--tier", "--type", "--limit", "--sort"];

/// Collect positional value args from `start`, stripping out known `--flag value` pairs.
/// Preserved for backwards-compat with existing unit tests.
#[cfg(test)]
fn collect_value_args(args: &[String], start: usize) -> String {
    let tail = match args.get(start..) {
        Some(slice) => slice,
        None => return String::new(),
    };

    let mut parts: Vec<&str> = Vec::new();
    let mut skip_next = false;

    for (i, arg) in tail.iter().enumerate() {
        if skip_next {
            skip_next = false;
            continue;
        }
        if KNOWN_FLAGS.contains(&arg.as_str()) {
            if i + 1 < tail.len() {
                skip_next = true;
            }
            continue;
        }
        parts.push(arg.as_str());
    }

    parts.join(" ")
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn s(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|p| p.to_string()).collect()
    }

    fn parse(args: &[&str]) -> MemoryCli {
        MemoryCli::try_parse_from(args).expect("args should parse")
    }

    // ── Subcommand routing ──────────────────────────────────────────────

    #[test]
    fn bare_memory_defaults_to_stats() {
        let cli = parse(&["memory"]);
        assert!(cli.cmd.is_none()); // run() maps None -> Stats
    }

    #[test]
    fn stats_subcommand_parses() {
        let cli = parse(&["memory", "stats"]);
        assert!(matches!(cli.cmd, Some(MemoryCmd::Stats)));
    }

    #[test]
    fn unknown_subcommand_is_parse_error() {
        assert!(MemoryCli::try_parse_from(["memory", "frobnicate"]).is_err());
    }

    // ── recall ─────────────────────────────────────────────────────────

    #[test]
    fn recall_with_query() {
        let cli = parse(&["memory", "recall", "error", "handling", "pattern"]);
        let Some(MemoryCmd::Recall { query }) = cli.cmd else {
            panic!("expected Recall");
        };
        assert_eq!(query.join(" "), "error handling pattern");
    }

    #[test]
    fn recall_empty_query() {
        let cli = parse(&["memory", "recall"]);
        let Some(MemoryCmd::Recall { query }) = cli.cmd else {
            panic!("expected Recall");
        };
        assert!(query.is_empty());
    }

    // ── store ──────────────────────────────────────────────────────────

    #[test]
    fn store_parses_key_and_value() {
        let cli = parse(&["memory", "store", "mykey", "hello", "world"]);
        let Some(MemoryCmd::Store {
            key,
            value,
            tier,
            entry_type,
        }) = cli.cmd
        else {
            panic!("expected Store");
        };
        assert_eq!(key, "mykey");
        assert_eq!(value.join(" "), "hello world");
        assert_eq!(tier, "semantic");
        assert_eq!(entry_type, "lesson");
    }

    #[test]
    fn store_tier_flag() {
        let cli = parse(&["memory", "store", "k", "v", "--tier", "episodic"]);
        let Some(MemoryCmd::Store { tier, .. }) = cli.cmd else {
            panic!("expected Store");
        };
        assert_eq!(tier, "episodic");
    }

    #[test]
    fn store_type_flag() {
        let cli = parse(&["memory", "store", "k", "v", "--type", "pattern"]);
        let Some(MemoryCmd::Store { entry_type, .. }) = cli.cmd else {
            panic!("expected Store");
        };
        assert_eq!(entry_type, "pattern");
    }

    #[test]
    fn store_missing_key_is_parse_error() {
        assert!(MemoryCli::try_parse_from(["memory", "store"]).is_err());
    }

    // ── list ───────────────────────────────────────────────────────────

    #[test]
    fn list_defaults() {
        let cli = parse(&["memory", "list"]);
        let Some(MemoryCmd::List { limit, sort }) = cli.cmd else {
            panic!("expected List");
        };
        assert_eq!(limit, 20);
        assert_eq!(sort, "access_count");
    }

    #[test]
    fn list_with_limit() {
        let cli = parse(&["memory", "list", "--limit", "50"]);
        let Some(MemoryCmd::List { limit, .. }) = cli.cmd else {
            panic!("expected List");
        };
        assert_eq!(limit, 50);
    }

    #[test]
    fn list_with_sort() {
        let cli = parse(&["memory", "list", "--sort", "created_at"]);
        let Some(MemoryCmd::List { sort, .. }) = cli.cmd else {
            panic!("expected List");
        };
        assert_eq!(sort, "created_at");
    }

    #[test]
    fn list_invalid_limit_is_parse_error() {
        assert!(MemoryCli::try_parse_from(["memory", "list", "--limit", "abc"]).is_err());
    }

    // ── reindex ────────────────────────────────────────────────────────

    #[test]
    fn reindex_default_batch_size() {
        let cli = parse(&["memory", "reindex"]);
        let Some(MemoryCmd::Reindex { batch_size }) = cli.cmd else {
            panic!("expected Reindex");
        };
        assert_eq!(batch_size, 256);
    }

    #[test]
    fn reindex_custom_batch_size() {
        let cli = parse(&["memory", "reindex", "--batch-size", "512"]);
        let Some(MemoryCmd::Reindex { batch_size }) = cli.cmd else {
            panic!("expected Reindex");
        };
        assert_eq!(batch_size, 512);
    }

    // ── collect_value_args (legacy helper) ─────────────────────────────

    #[test]
    fn collect_value_args_plain_text() {
        let args = s(&["touring", "memory", "store", "mykey", "hello", "world"]);
        assert_eq!(collect_value_args(&args, 4), "hello world");
    }

    #[test]
    fn collect_value_args_strips_tier_flag() {
        let args = s(&[
            "touring", "memory", "store", "mykey", "hello", "--tier", "episodic", "world",
        ]);
        assert_eq!(collect_value_args(&args, 4), "hello world");
    }

    #[test]
    fn collect_value_args_strips_type_flag() {
        let args = s(&[
            "touring", "memory", "store", "mykey", "value", "--type", "pattern",
        ]);
        assert_eq!(collect_value_args(&args, 4), "value");
    }

    #[test]
    fn collect_value_args_strips_multiple_flags() {
        let args = s(&[
            "touring", "memory", "store", "k", "some", "text", "--tier", "semantic", "--type",
            "lesson",
        ]);
        assert_eq!(collect_value_args(&args, 4), "some text");
    }

    #[test]
    fn collect_value_args_empty_when_out_of_range() {
        let args = s(&["touring", "memory", "store", "mykey"]);
        assert_eq!(collect_value_args(&args, 4), "");
    }

    #[test]
    fn collect_value_args_flag_at_end_without_value() {
        let args = s(&["touring", "memory", "store", "k", "val", "--tier"]);
        assert_eq!(collect_value_args(&args, 4), "val");
    }
}
