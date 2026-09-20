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
        /// Wait for the rebuild to seal its generation, polling `index status`.
        ///
        /// A rebuild past its heavy budget answers exit 79 (`still_running`)
        /// with the instruction to poll `index status` by hand. With `--wait`
        /// the client runs that poll itself and exits 0 once the generation is
        /// sealed. The default is unchanged — no script that calls `index
        /// rebuild` today behaves differently.
        #[arg(long)]
        wait: bool,
        /// Seconds between polls while waiting.
        #[arg(long, default_value_t = 5u64)]
        poll_secs: u64,
        /// Give up waiting after this many seconds (0 = wait indefinitely).
        #[arg(long, default_value_t = 3600u64)]
        wait_timeout_secs: u64,
    },
    /// On-demand single-file reindex (B3, 2026-05-10).
    Ingest {
        /// Path of the file to reindex.
        path: String,
    },
    /// Why a path is, or is not, in the index — the walker's own rules applied to
    /// one file (skipped dir, unsupported extension, over the size ceiling, …).
    Why {
        /// Repo-relative or absolute path to explain.
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
        IndexCmd::Rebuild {
            dir,
            wait,
            poll_secs,
            wait_timeout_secs,
        } => {
            // A full rebuild runs past the 120s default; `daemon_query` waits past
            // the heavy budget for every hook in `touring_foundation::is_heavy_hook`
            // (an explicit `--timeout` still wins).
            let payload = match dir {
                Some(d) => serde_json::json!({ "dir": d }),
                None => serde_json::json!({}),
            };
            match daemon_query("cli-index-rebuild", payload) {
                Ok(output) => {
                    println!("{output}");
                    if wait {
                        println!("{}", wait_for_generation_seal(poll_secs, wait_timeout_secs)?);
                    }
                }
                // The rebuild outlived its budget and is STILL WALKING. Without
                // `--wait` this is exit 79 plus a note telling the caller to poll
                // by hand; with it, we run that poll here and answer when the
                // generation is sealed.
                Err(e) if wait && rebuild_still_running(&e) => {
                    eprintln!(
                        "index rebuild exceeded its budget and keeps running — waiting for the \
                         generation seal (polling every {poll_secs}s)"
                    );
                    println!("{}", wait_for_generation_seal(poll_secs, wait_timeout_secs)?);
                }
                Err(e) => return Err(e),
            }
        }
        IndexCmd::Ingest { path } => {
            if path.is_empty() {
                anyhow::bail!(
                    "index ingest requires <file>: usage `touring index ingest <path>` — run `touring help` for details"
                );
            }
            let payload = serde_json::json!({ "path": path });
            let output = daemon_query("cli-index-ingest", payload)?;
            println!("{output}");
        }
        IndexCmd::Why { path } => {
            if path.is_empty() {
                anyhow::bail!("index why requires <path>: usage `touring index why <path>`");
            }
            let payload = serde_json::json!({ "path": path });
            let output = daemon_query("cli-index-why", payload)?;
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

/// Is this failure a rebuild that outlived its budget and is still walking?
///
/// `DaemonBusy` carries `retryable`: a LIGHT handler that ran out of budget may
/// be retried, a HEAVY one may not — re-running `index rebuild` would wait for
/// the running walk and then repeat the whole thing. `retryable == false` is
/// therefore exactly "still running, do not retry, poll instead".
fn rebuild_still_running(err: &anyhow::Error) -> bool {
    err.downcast_ref::<crate::daemon_client::DaemonBusy>()
        .is_some_and(|busy| !busy.retryable)
}

/// Does this `index status` payload say a rebuild is still walking?
///
/// Pure so the decision is testable without a daemon: an unreadable payload or
/// a missing field reads as NOT building, because the alternative is a client
/// that waits forever on a status it cannot parse.
fn generation_is_building(status_json: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(status_json)
        .ok()
        .and_then(|v| {
            v.get("index_generation")
                .and_then(|g| g.get("state"))
                .and_then(|s| s.as_str())
                .map(|s| s == "building")
        })
        .unwrap_or(false)
}

/// Poll `index status` until the rebuild's generation stops being `building`.
///
/// Client-side only: nothing in the daemon changes, and the default remains a
/// synchronous rebuild. It exists because a rebuild past its heavy budget exits
/// 79 with the instruction to poll `touring index status` by hand — a loop a
/// person should not have to run. `index status` is answered off the project
/// actor (decision 3-A, 14/09/2026), so it keeps replying during the sealing
/// phase, which is precisely when the wait matters.
fn wait_for_generation_seal(poll_secs: u64, timeout_secs: u64) -> anyhow::Result<String> {
    let started = std::time::Instant::now();
    let interval = std::time::Duration::from_secs(poll_secs.max(1));
    loop {
        let status = daemon_query("cli-index-status", serde_json::json!({}))?;
        if !generation_is_building(&status) {
            return Ok(status);
        }
        if timeout_secs > 0 && started.elapsed().as_secs() >= timeout_secs {
            anyhow::bail!(
                "index rebuild still building after {timeout_secs}s — it keeps running; \
                 raise --wait-timeout-secs (0 waits indefinitely) or poll `touring index status`"
            );
        }
        std::thread::sleep(interval);
    }
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
    fn why_takes_one_path() {
        let cli = parse(&["index", "why", "crates/x/src/big.rs"]);
        assert!(
            matches!(cli.cmd, Some(IndexCmd::Why { ref path }) if path == "crates/x/src/big.rs")
        );
        assert!(
            IndexCli::try_parse_from(s(&["index", "why"])).is_err(),
            "the path is required"
        );
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
        let Some(IndexCmd::Rebuild { dir, .. }) = cli.cmd else {
            panic!("expected Rebuild");
        };
        assert_eq!(dir.as_deref(), Some("/tmp/proj"));
    }

    #[test]
    fn rebuild_no_dir_is_none() {
        let cli = parse(&["index", "rebuild"]);
        let Some(IndexCmd::Rebuild { dir, .. }) = cli.cmd else {
            panic!("expected Rebuild");
        };
        assert!(dir.is_none());
    }

    /// The default must stay synchronous: every script that calls
    /// `index rebuild` today keeps its behaviour.
    #[test]
    fn rebuild_does_not_wait_unless_asked() {
        let cli = parse(&["index", "rebuild"]);
        let Some(IndexCmd::Rebuild {
            wait,
            poll_secs,
            wait_timeout_secs,
            ..
        }) = cli.cmd
        else {
            panic!("expected Rebuild");
        };
        assert!(!wait, "--wait is opt-in");
        assert_eq!(poll_secs, 5);
        assert_eq!(wait_timeout_secs, 3600);
    }

    #[test]
    fn rebuild_wait_accepts_its_knobs() {
        let cli = parse(&[
            "index",
            "rebuild",
            "--wait",
            "--poll-secs",
            "2",
            "--wait-timeout-secs",
            "0",
        ]);
        let Some(IndexCmd::Rebuild {
            wait,
            poll_secs,
            wait_timeout_secs,
            ..
        }) = cli.cmd
        else {
            panic!("expected Rebuild");
        };
        assert!(wait);
        assert_eq!(poll_secs, 2);
        assert_eq!(wait_timeout_secs, 0, "0 means wait indefinitely");
    }

    #[test]
    fn a_building_generation_is_what_keeps_the_wait_going() {
        assert!(generation_is_building(
            r#"{"initialized":true,"index_generation":{"state":"building"}}"#
        ));
        for sealed in ["complete", "aborted", "partial", "scoped", "none"] {
            assert!(
                !generation_is_building(&format!(
                    r#"{{"index_generation":{{"state":"{sealed}"}}}}"#
                )),
                "`{sealed}` is not a rebuild in flight"
            );
        }
    }

    /// A payload we cannot read must not trap the client in an endless poll.
    #[test]
    fn an_unreadable_status_ends_the_wait_instead_of_looping_forever() {
        assert!(!generation_is_building("not json at all"));
        assert!(!generation_is_building("{}"));
        assert!(!generation_is_building(r#"{"index_generation":{}}"#));
        assert!(!generation_is_building(""));
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
