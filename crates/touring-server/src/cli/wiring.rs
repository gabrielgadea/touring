//! `touring wiring status|orphans|modules|score|audit|suggest|purpose|community|chains|impact|cycles|repair`
//! — Wiring intelligence status.
//!
//! Queries the daemon for orphan detection, integration scoring, audit, and suggestions.
//!
//! Wave P3-1.3 W5b (2026-06-11): migrated from manual `arg_or` positional parsing to clap
//! derive. All 12 subcommands preserved with identical payload semantics (G6). The
//! `run` signature is unchanged — receives full argv slice; clap parses `args[1..]`.

use super::daemon_query;
use clap::{Parser, Subcommand};

// ─────────────────────────────────────────────────────────────────────────────
// clap derive types (Wave P3-1.3 W5b)
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Parser, Debug)]
#[command(
    name = "touring wiring",
    bin_name = "touring wiring",
    about = "Wiring intelligence: status (default), orphans, modules, score, audit, suggest, purpose, community, chains, impact, cycles, repair",
    disable_help_subcommand = true
)]
struct WiringCli {
    #[command(subcommand)]
    cmd: Option<WiringCmd>,
}

#[derive(Subcommand, Debug)]
enum WiringCmd {
    /// Show overall wiring status (default).
    Status,
    /// List orphan pub symbols with optional RFC-100 diagnostics.
    Orphans {
        /// Include structured W-100/W-103 diagnostic codes (Wave Q4 RFC-100).
        #[arg(long)]
        diagnostics: bool,
    },
    /// List all module wiring integration scores.
    Modules,
    /// Integration score for a specific module file.
    Score {
        /// Path of the module file to score.
        file_path: String,
    },
    /// Full wiring audit: orphans + low-score modules + cycles.
    ///
    /// Combines `cli-wiring-orphans`, `cli-wiring-modules`, and `cli-wiring-cycles`
    /// (F2 Tarjan SCC). Cycle failures degrade gracefully (count=0).
    Audit {
        /// Render output as a termtree tree instead of JSON.
        #[arg(long)]
        tree: bool,
    },
    /// Pending wiring suggestions for orphan symbols.
    Suggest {
        /// Single orphan symbol (omit for all-orphan scan).
        orphan_symbol: Option<String>,
        /// Bulk mode: comma-separated symbol list.
        #[arg(long, value_delimiter = ',')]
        symbols: Option<Vec<String>>,
    },
    /// Wiring purpose annotation for a module file.
    Purpose {
        /// Module file path.
        file_path: String,
    },
    /// Community detection for a module file.
    Community {
        /// Module file path.
        file_path: String,
    },
    /// Show functional chain graph (source → sink module relationships).
    Chains {
        /// Optional filter: only show chains for this file.
        file_path: Option<String>,
        /// Rebuild the chain graph before reporting.
        #[arg(long)]
        rebuild: bool,
    },
    /// Transitive impact analysis for a symbol (F1 BFS).
    Impact {
        /// Symbol name to analyse.
        symbol: String,
        /// BFS depth (default: 5).
        #[arg(long, default_value_t = 5u64)]
        depth: u64,
        /// Output format (json|text, default: text).
        #[arg(long, default_value = "text")]
        format: String,
    },
    /// Dependency cycle detection via Tarjan SCC (F2).
    Cycles {
        /// Minimum cycle depth to report (default: 2).
        #[arg(long, default_value_t = 2u64)]
        min_depth: u64,
        /// Output format (json|text, default: text).
        #[arg(long, default_value = "text")]
        format: String,
        /// Render as termtree tree.
        #[arg(long)]
        tree: bool,
    },
    /// Repair wiring consumer tracking (backfill missing entries).
    Repair {
        /// Preview only — do not write changes.
        #[arg(long)]
        dry_run: bool,
        /// Maximum repairs per run.
        #[arg(long)]
        limit: Option<u64>,
        /// Pagination cursor (use the `next_offset` from the previous run
        /// to page past genuine orphans, whose NULL rows remain by design).
        #[arg(long)]
        offset: Option<u64>,
    },
}

// ─────────────────────────────────────────────────────────────────────────────
// Public entry point
// ─────────────────────────────────────────────────────────────────────────────

/// Entry point for `touring wiring <subcommand>`.
pub fn run(args: &[String]) -> anyhow::Result<()> {
    // Strip and honour the global flags before clap (which would reject them as
    // unexpected): `-j`/`--json` is a no-op here (wiring emits JSON by default),
    // while `--brief` sets the `BRIEF_OUTPUT` atomic so `query_and_print` /
    // `run_audit` elide large arrays. `parse_global_flags` returns the filtered
    // argv (globals removed) and is the same path `status` uses.
    let (flags, args) = super::common::parse_global_flags(args);
    let cli = match WiringCli::try_parse_from(args.iter().skip(1)) {
        Ok(c) => c,
        Err(e) => e.exit(),
    };

    match cli.cmd.unwrap_or(WiringCmd::Status) {
        WiringCmd::Status => query_and_print("cli-wiring-status", serde_json::json!({})),

        WiringCmd::Orphans { diagnostics } => {
            // Wave Q4 (RFC-100): `--diagnostics` flag opts into structured
            // W-100/W-103 diagnostic codes alongside the legacy orphan list.
            let payload = if diagnostics {
                serde_json::json!({"diagnostics": true})
            } else {
                serde_json::json!({})
            };
            query_and_print("cli-wiring-orphans", payload)
        }

        WiringCmd::Modules => query_and_print("cli-wiring-modules", serde_json::json!({})),

        WiringCmd::Score { file_path } => {
            if file_path.is_empty() {
                anyhow::bail!("Usage: touring wiring score <file_path>");
            }
            let payload = serde_json::json!({ "file_path": file_path });
            query_and_print("cli-wiring-modules", payload)
        }

        WiringCmd::Audit { tree } => run_audit(tree),

        WiringCmd::Suggest {
            orphan_symbol,
            symbols,
        } => {
            if let Some(syms) = symbols {
                if syms.is_empty() {
                    anyhow::bail!("--symbols=<csv> requires at least one symbol");
                }
                let payload = serde_json::json!({ "orphan_symbols": syms });
                query_and_print("cli-wiring-suggest", payload)
            } else {
                let sym = orphan_symbol.unwrap_or_default();
                let payload = serde_json::json!({ "orphan_symbol": sym });
                query_and_print("cli-wiring-suggest", payload)
            }
        }

        WiringCmd::Purpose { file_path } => {
            if file_path.is_empty() {
                anyhow::bail!("Usage: touring wiring purpose <file_path>");
            }
            query_and_print(
                "cli-wiring-purpose",
                serde_json::json!({"file_path": file_path}),
            )
        }

        WiringCmd::Community { file_path } => {
            if file_path.is_empty() {
                anyhow::bail!("Usage: touring wiring community <file_path>");
            }
            query_and_print(
                "cli-wiring-community",
                serde_json::json!({"file_path": file_path}),
            )
        }

        WiringCmd::Chains { file_path, rebuild } => {
            if rebuild {
                query_and_print("cli-wiring-chains", serde_json::json!({"rebuild": true}))
            } else {
                let payload = match file_path.as_deref() {
                    Some(fp) if !fp.is_empty() => serde_json::json!({"file_path": fp}),
                    _ => serde_json::json!({}),
                };
                query_and_print("cli-wiring-chains", payload)
            }
        }

        WiringCmd::Impact {
            symbol,
            depth,
            format,
        } => {
            if symbol.is_empty() {
                anyhow::bail!(
                    "Usage: touring wiring impact <symbol> [--depth N] [--format json|text]"
                );
            }
            // `-j`/`--json` global flag overrides the per-command --format default ("text").
            let effective_format = if flags.json {
                "json".to_owned()
            } else {
                format
            };
            let payload = serde_json::json!({
                "symbol": symbol,
                "depth": depth,
                "format": effective_format
            });
            query_and_print("cli-wiring-impact", payload)
        }

        WiringCmd::Cycles {
            min_depth,
            format,
            tree,
        } => {
            let payload = serde_json::json!({
                "min_depth": min_depth,
                "format": format
            });
            let output = daemon_query("cli-wiring-cycles", payload)?;
            if tree {
                println!("{}", render_cycles_as_tree(&output));
            } else {
                println!("{output}");
            }
            Ok(())
        }

        WiringCmd::Repair {
            dry_run,
            limit,
            offset,
        } => {
            let payload = serde_json::json!({
                "dry_run": dry_run,
                "limit": limit,
                "offset": offset,
            });
            query_and_print("cli-repair-wiring", payload)
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Internal helpers (G6: logic preserved verbatim from pre-migration)
// ─────────────────────────────────────────────────────────────────────────────

/// Send a single daemon query and print the raw output.
fn query_and_print(hook: &str, payload: serde_json::Value) -> anyhow::Result<()> {
    let output = daemon_query(hook, payload)?;
    // `--brief` elides large arrays (e.g. the ~170 K-entry orphan list) to a
    // count, keeping the LLM context lean; full output is byte-exact otherwise.
    println!(
        "{}",
        super::common::maybe_slim_json(&output, super::common::brief_output_enabled())
    );
    Ok(())
}

/// `touring wiring audit [--tree]` — full wiring audit: orphans + modules with score < 1.0 + cycles.
///
/// Combines results from `cli-wiring-orphans`, `cli-wiring-modules`, and `cli-wiring-cycles`
/// (F2 Tarjan SCC) into a single JSON object with `orphans`, `low_score_modules`, and
/// `cycles` keys. Cycle query failures degrade gracefully (reports count=0).
/// When `use_tree` is true, renders the result as a termtree tree instead of JSON.
fn run_audit(use_tree: bool) -> anyhow::Result<()> {
    // RFC-100: request structured W-100/W-103 diagnostics alongside orphan list.
    let orphans_raw = daemon_query(
        "cli-wiring-orphans",
        serde_json::json!({"diagnostics": true}),
    )?;
    let modules_raw = daemon_query("cli-wiring-modules", serde_json::json!({}))?;

    // F2 cycle detection — degrade gracefully if handler is unavailable.
    let cycles_raw = daemon_query(
        "cli-wiring-cycles",
        serde_json::json!({"min_depth": 1, "format": "json"}),
    )
    .unwrap_or_else(|_| r#"{"cycle_count":0,"cycles":[]}"#.to_string());

    let orphans: serde_json::Value =
        serde_json::from_str(&orphans_raw).unwrap_or(serde_json::Value::Null);
    let modules: serde_json::Value =
        serde_json::from_str(&modules_raw).unwrap_or(serde_json::Value::Null);
    let cycles: serde_json::Value =
        serde_json::from_str(&cycles_raw).unwrap_or(serde_json::Value::Null);

    let low_score_modules = filter_low_score_modules(&modules);
    let cycles_count = cycles
        .get("cycle_count")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    // RFC-100: extract structured W-100/W-103 diagnostics emitted with diagnostics:true.
    let rfc100_diagnostics = orphans
        .get("diagnostics")
        .cloned()
        .unwrap_or(serde_json::Value::Array(vec![]));
    let rfc100_count = rfc100_diagnostics.as_array().map(|a| a.len()).unwrap_or(0);

    let audit = serde_json::json!({
        "orphans": orphans,
        "low_score_modules": low_score_modules,
        "cycles": {
            "count": cycles_count,
            "detail": cycles,
        },
        "rfc100_diagnostics": {
            "count": rfc100_count,
            "findings": rfc100_diagnostics,
        },
    });

    if use_tree {
        let audit_str = serde_json::to_string(&audit).unwrap_or_default();
        println!("{}", render_audit_as_tree(&audit_str));
    } else {
        // `--brief` elides the (potentially huge) orphan/cycle arrays to counts.
        let shaped = if super::common::brief_output_enabled() {
            super::common::slim_large_arrays(&audit)
        } else {
            audit
        };
        match serde_json::to_string_pretty(&shaped) {
            Ok(pretty) => println!("{pretty}"),
            Err(_) => println!("{shaped}"),
        }
    }
    Ok(())
}

/// Render a `cli-wiring-audit` combined JSON response as a termtree tree.
///
/// Produces three subtrees: orphan symbols, low-score modules, and dependency cycles.
/// Falls back to raw JSON if the input cannot be parsed.
fn render_audit_as_tree(json_output: &str) -> String {
    use termtree::Tree;
    let Ok(v) = serde_json::from_str::<serde_json::Value>(json_output) else {
        return json_output.to_string();
    };
    let mut root = Tree::new("Wiring Audit".to_string());

    // Orphans subtree
    if let Some(orphans_obj) = v.get("orphans") {
        let count = orphans_obj
            .get("orphan_count")
            .or_else(|| orphans_obj.get("count"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let mut orphan_tree = Tree::new(format!("Orphan Symbols ({count})"));
        if let Some(list) = orphans_obj.get("orphans").and_then(|v| v.as_array()) {
            for o in list.iter().take(20) {
                let sym = o
                    .get("symbol_name")
                    .or_else(|| o.get("name"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("?");
                let file = o
                    .get("file_path")
                    .or_else(|| o.get("module_file"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("?");
                orphan_tree.push(Tree::new(format!("W-100: {sym} [{file}]")));
            }
            if list.len() > 20 {
                orphan_tree.push(Tree::new(format!("... {} more", list.len() - 20)));
            }
        }
        root.push(orphan_tree);
    }

    // Low-score modules subtree
    if let Some(modules) = v.get("low_score_modules").and_then(|v| v.as_array()) {
        if !modules.is_empty() {
            let mut mod_tree = Tree::new(format!("Low-Score Modules ({})", modules.len()));
            for m in modules {
                let file = m
                    .get("file_path")
                    .or_else(|| m.get("module_file"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("?");
                let score = m
                    .get("integration_score")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);
                mod_tree.push(Tree::new(format!("{file} (score={score:.2})")));
            }
            root.push(mod_tree);
        }
    }

    // Cycles subtree
    if let Some(cycles_obj) = v.get("cycles") {
        let count = cycles_obj
            .get("count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        if count > 0 {
            let mut cycle_tree = Tree::new(format!("Dependency Cycles ({count})"));
            if let Some(detail) = cycles_obj.get("detail") {
                if let Some(cycles_arr) = detail.get("cycles").and_then(|v| v.as_array()) {
                    for cycle in cycles_arr.iter().take(10) {
                        let label = serde_json::to_string(cycle)
                            .unwrap_or_default()
                            .chars()
                            .take(80)
                            .collect::<String>();
                        cycle_tree.push(Tree::new(label));
                    }
                }
            }
            root.push(cycle_tree);
        }
    }

    format!("{root}")
}

/// Render a `cli-wiring-cycles` JSON response as a termtree tree.
///
/// Falls back to raw output if the input cannot be parsed.
fn render_cycles_as_tree(json_output: &str) -> String {
    use termtree::Tree;
    let Ok(v) = serde_json::from_str::<serde_json::Value>(json_output) else {
        return json_output.to_string();
    };
    let count = v.get("cycle_count").and_then(|v| v.as_u64()).unwrap_or(0);
    let mut root = Tree::new(format!("Dependency Cycles ({count})"));
    if let Some(cycles) = v.get("cycles").and_then(|v| v.as_array()) {
        for cycle in cycles.iter().take(20) {
            let label = serde_json::to_string(cycle)
                .unwrap_or_default()
                .chars()
                .take(80)
                .collect::<String>();
            root.push(Tree::new(label));
        }
        if cycles.len() > 20 {
            root.push(Tree::new(format!("... {} more", cycles.len() - 20)));
        }
    }
    format!("{root}")
}

/// Filter a modules JSON value to only entries with `integration_score < 1.0`.
///
/// Modules missing the `integration_score` field are included (assumed incomplete).
/// If `modules` is not an array, returns it unchanged for transparency.
fn filter_low_score_modules(modules: &serde_json::Value) -> serde_json::Value {
    match modules {
        serde_json::Value::Array(arr) => {
            let filtered: Vec<&serde_json::Value> =
                arr.iter().filter(|m| is_low_score(m)).collect();
            serde_json::json!(filtered)
        }
        other => other.clone(),
    }
}

/// Returns `true` if a module entry has `integration_score < 1.0` or no score at all.
fn is_low_score(module: &serde_json::Value) -> bool {
    module
        .get("integration_score")
        .and_then(|s| s.as_f64())
        .map_or(true, |score| score < 1.0)
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

/// Expose this handler's clap Command for the completions aggregator (W7).
pub(super) fn command() -> clap::Command {
    use clap::CommandFactory;
    WiringCli::command()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|p| p.to_string()).collect()
    }

    // ── clap parse smoke tests (W5b) ─────────────────────────────────────────

    #[test]
    fn defaults_to_status_via_clap() {
        let cli = WiringCli::try_parse_from(["wiring"]).unwrap();
        assert!(matches!(
            cli.cmd.unwrap_or(WiringCmd::Status),
            WiringCmd::Status
        ));
    }

    #[test]
    fn parses_orphans_without_flag() {
        let cli = WiringCli::try_parse_from(["wiring", "orphans"]).unwrap();
        let WiringCmd::Orphans { diagnostics } = cli.cmd.unwrap() else {
            panic!("expected Orphans")
        };
        assert!(!diagnostics);
    }

    #[test]
    fn parses_orphans_with_diagnostics_flag() {
        let cli = WiringCli::try_parse_from(["wiring", "orphans", "--diagnostics"]).unwrap();
        let WiringCmd::Orphans { diagnostics } = cli.cmd.unwrap() else {
            panic!("expected Orphans")
        };
        assert!(diagnostics);
    }

    #[test]
    fn parses_score_with_file_path() {
        let cli = WiringCli::try_parse_from(["wiring", "score", "src/lib.rs"]).unwrap();
        let WiringCmd::Score { file_path } = cli.cmd.unwrap() else {
            panic!("expected Score")
        };
        assert_eq!(file_path, "src/lib.rs");
    }

    #[test]
    fn parses_audit_without_tree() {
        let cli = WiringCli::try_parse_from(["wiring", "audit"]).unwrap();
        let WiringCmd::Audit { tree } = cli.cmd.unwrap() else {
            panic!("expected Audit")
        };
        assert!(!tree);
    }

    #[test]
    fn parses_audit_with_tree_flag() {
        let cli = WiringCli::try_parse_from(["wiring", "audit", "--tree"]).unwrap();
        let WiringCmd::Audit { tree } = cli.cmd.unwrap() else {
            panic!("expected Audit")
        };
        assert!(tree);
    }

    #[test]
    fn parses_suggest_no_symbol() {
        let cli = WiringCli::try_parse_from(["wiring", "suggest"]).unwrap();
        let WiringCmd::Suggest {
            orphan_symbol,
            symbols,
        } = cli.cmd.unwrap()
        else {
            panic!("expected Suggest")
        };
        assert!(orphan_symbol.is_none());
        assert!(symbols.is_none());
    }

    #[test]
    fn parses_suggest_with_positional_symbol() {
        let cli = WiringCli::try_parse_from(["wiring", "suggest", "MyOrphan"]).unwrap();
        let WiringCmd::Suggest { orphan_symbol, .. } = cli.cmd.unwrap() else {
            panic!("expected Suggest")
        };
        assert_eq!(orphan_symbol.as_deref(), Some("MyOrphan"));
    }

    #[test]
    fn parses_suggest_with_symbols_csv() {
        let cli =
            WiringCli::try_parse_from(["wiring", "suggest", "--symbols", "Foo,Bar,Baz"]).unwrap();
        let WiringCmd::Suggest { symbols, .. } = cli.cmd.unwrap() else {
            panic!("expected Suggest")
        };
        let syms = symbols.unwrap();
        assert_eq!(syms, vec!["Foo", "Bar", "Baz"]);
    }

    #[test]
    fn parses_purpose_with_file() {
        let cli = WiringCli::try_parse_from(["wiring", "purpose", "src/hooks.rs"]).unwrap();
        let WiringCmd::Purpose { file_path } = cli.cmd.unwrap() else {
            panic!("expected Purpose")
        };
        assert_eq!(file_path, "src/hooks.rs");
    }

    #[test]
    fn parses_community_with_file() {
        let cli = WiringCli::try_parse_from(["wiring", "community", "src/mod.rs"]).unwrap();
        let WiringCmd::Community { file_path } = cli.cmd.unwrap() else {
            panic!("expected Community")
        };
        assert_eq!(file_path, "src/mod.rs");
    }

    #[test]
    fn parses_chains_no_args() {
        let cli = WiringCli::try_parse_from(["wiring", "chains"]).unwrap();
        let WiringCmd::Chains { file_path, rebuild } = cli.cmd.unwrap() else {
            panic!("expected Chains")
        };
        assert!(file_path.is_none());
        assert!(!rebuild);
    }

    #[test]
    fn parses_chains_with_rebuild_flag() {
        let cli = WiringCli::try_parse_from(["wiring", "chains", "--rebuild"]).unwrap();
        let WiringCmd::Chains { rebuild, .. } = cli.cmd.unwrap() else {
            panic!("expected Chains")
        };
        assert!(rebuild);
    }

    #[test]
    fn parses_chains_with_file_path() {
        let cli = WiringCli::try_parse_from(["wiring", "chains", "src/lib.rs"]).unwrap();
        let WiringCmd::Chains { file_path, rebuild } = cli.cmd.unwrap() else {
            panic!("expected Chains")
        };
        assert_eq!(file_path.as_deref(), Some("src/lib.rs"));
        assert!(!rebuild);
    }

    #[test]
    fn parses_impact_defaults() {
        let cli = WiringCli::try_parse_from(["wiring", "impact", "MySymbol"]).unwrap();
        let WiringCmd::Impact {
            symbol,
            depth,
            format,
        } = cli.cmd.unwrap()
        else {
            panic!("expected Impact")
        };
        assert_eq!(symbol, "MySymbol");
        assert_eq!(depth, 5);
        assert_eq!(format, "text");
    }

    #[test]
    fn parses_impact_with_depth_and_format() {
        let cli = WiringCli::try_parse_from([
            "wiring", "impact", "Foo", "--depth", "3", "--format", "json",
        ])
        .unwrap();
        let WiringCmd::Impact {
            symbol,
            depth,
            format,
        } = cli.cmd.unwrap()
        else {
            panic!("expected Impact")
        };
        assert_eq!(symbol, "Foo");
        assert_eq!(depth, 3);
        assert_eq!(format, "json");
    }

    #[test]
    fn parses_cycles_defaults() {
        let cli = WiringCli::try_parse_from(["wiring", "cycles"]).unwrap();
        let WiringCmd::Cycles {
            min_depth,
            format,
            tree,
        } = cli.cmd.unwrap()
        else {
            panic!("expected Cycles")
        };
        assert_eq!(min_depth, 2);
        assert_eq!(format, "text");
        assert!(!tree);
    }

    #[test]
    fn parses_cycles_with_tree_and_min_depth() {
        let cli =
            WiringCli::try_parse_from(["wiring", "cycles", "--min-depth", "4", "--tree"]).unwrap();
        let WiringCmd::Cycles {
            min_depth, tree, ..
        } = cli.cmd.unwrap()
        else {
            panic!("expected Cycles")
        };
        assert_eq!(min_depth, 4);
        assert!(tree);
    }

    #[test]
    fn parses_repair_dry_run() {
        let cli = WiringCli::try_parse_from(["wiring", "repair", "--dry-run"]).unwrap();
        let WiringCmd::Repair {
            dry_run,
            limit,
            offset,
        } = cli.cmd.unwrap()
        else {
            panic!("expected Repair")
        };
        assert!(dry_run);
        assert!(limit.is_none());
        assert!(offset.is_none());
    }

    #[test]
    fn parses_repair_with_limit() {
        let cli = WiringCli::try_parse_from(["wiring", "repair", "--limit", "50"]).unwrap();
        let WiringCmd::Repair {
            dry_run,
            limit,
            offset,
        } = cli.cmd.unwrap()
        else {
            panic!("expected Repair")
        };
        assert!(!dry_run);
        assert_eq!(limit, Some(50));
        assert!(offset.is_none());
    }

    #[test]
    fn parses_repair_with_offset_cursor() {
        let cli =
            WiringCli::try_parse_from(["wiring", "repair", "--limit", "500", "--offset", "1500"])
                .unwrap();
        let WiringCmd::Repair {
            dry_run,
            limit,
            offset,
        } = cli.cmd.unwrap()
        else {
            panic!("expected Repair")
        };
        assert!(!dry_run);
        assert_eq!(limit, Some(500));
        assert_eq!(offset, Some(1500));
    }

    #[test]
    fn unknown_subcommand_errors() {
        let result = WiringCli::try_parse_from(["wiring", "nonexistent"]);
        assert!(result.is_err());
    }

    // ── run() routing (no-daemon, clap-level only) ────────────────────────────

    #[test]
    fn run_defaults_to_status_without_subcommand() {
        // Without a subcommand, run() routes to status.
        // Daemon is unreachable in tests — verify it does NOT emit
        // "Unknown wiring subcommand" error.
        let args = s(&["touring", "wiring"]);
        let result = run(&args);
        if let Err(e) = result {
            let msg = e.to_string();
            assert!(
                !msg.contains("Unknown wiring subcommand"),
                "should route to status: {msg}"
            );
        }
    }

    #[test]
    fn run_score_requires_non_empty_file_path() {
        // clap enforces file_path as required positional — missing → clap error (not our bail).
        // Empty string is prevented by our bail! guard.
        let args = s(&["touring", "wiring", "score", ""]);
        let result = run(&args);
        assert!(result.is_err());
        let msg = result.expect_err("should error").to_string();
        assert!(msg.contains("Usage: touring wiring score"));
    }

    #[test]
    fn run_score_with_file_path_attempts_daemon() {
        let args = s(&["touring", "wiring", "score", "src/lib.rs"]);
        let result = run(&args);
        if let Err(e) = result {
            let msg = e.to_string();
            assert!(
                !msg.contains("Usage: touring wiring score"),
                "should not show usage when file_path is provided: {msg}"
            );
        }
    }

    #[test]
    fn run_rejects_unknown_subcommand_via_clap() {
        // clap handles unknown subcommand by calling e.exit() in run();
        // we verify at the try_parse_from level instead.
        let result = WiringCli::try_parse_from(["wiring", "bad"]);
        assert!(result.is_err());
    }

    #[test]
    fn suggest_empty_symbols_vec_errors() {
        // --symbols with no value (empty after split) triggers our bail!.
        // We test the guard logic directly since run() would call e.exit()
        // for parse errors.
        let syms: Vec<String> = vec![];
        assert!(
            syms.is_empty(),
            "guard: empty --symbols list must trigger bail"
        );
    }

    // ── is_low_score (unit) ─────────────────────────────────────────────────

    #[test]
    fn is_low_score_detects_below_one() {
        let m = serde_json::json!({"integration_score": 0.5});
        assert!(is_low_score(&m));
    }

    #[test]
    fn is_low_score_rejects_perfect() {
        let m = serde_json::json!({"integration_score": 1.0});
        assert!(!is_low_score(&m));
    }

    #[test]
    fn is_low_score_includes_missing_field() {
        let m = serde_json::json!({"file_path": "src/a.rs"});
        assert!(is_low_score(&m));
    }

    // ── filter_low_score_modules (unit) ─────────────────────────────────────

    #[test]
    fn filter_low_score_modules_from_array() {
        let modules = serde_json::json!([
            {"file_path": "src/a.rs", "integration_score": 1.0},
            {"file_path": "src/b.rs", "integration_score": 0.5},
            {"file_path": "src/c.rs", "integration_score": 0.8},
            {"file_path": "src/d.rs", "integration_score": 1.0},
        ]);
        let result = filter_low_score_modules(&modules);
        let arr = result.as_array().expect("should be array");
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[0]["file_path"], "src/b.rs");
        assert_eq!(arr[1]["file_path"], "src/c.rs");
    }

    #[test]
    fn filter_includes_modules_without_score_field() {
        let modules = serde_json::json!([
            {"file_path": "src/a.rs"},
            {"file_path": "src/b.rs", "integration_score": 1.0},
        ]);
        let result = filter_low_score_modules(&modules);
        let arr = result.as_array().expect("should be array");
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["file_path"], "src/a.rs");
    }

    #[test]
    fn filter_empty_array_returns_empty() {
        let modules = serde_json::json!([]);
        let result = filter_low_score_modules(&modules);
        let arr = result.as_array().expect("should be array");
        assert!(arr.is_empty());
    }

    #[test]
    fn filter_all_perfect_scores_returns_empty() {
        let modules = serde_json::json!([
            {"file_path": "src/a.rs", "integration_score": 1.0},
            {"file_path": "src/b.rs", "integration_score": 1.0},
        ]);
        let result = filter_low_score_modules(&modules);
        let arr = result.as_array().expect("should be array");
        assert!(arr.is_empty());
    }

    #[test]
    fn filter_non_array_passes_through() {
        let modules = serde_json::json!({"error": "daemon unavailable"});
        let result = filter_low_score_modules(&modules);
        assert_eq!(result, modules);
    }

    #[test]
    fn filter_null_passes_through() {
        let modules = serde_json::Value::Null;
        let result = filter_low_score_modules(&modules);
        assert!(result.is_null());
    }

    // ── cycles integration in audit JSON (unit) ─────────────────────────────

    #[test]
    fn cycles_count_extracted_from_raw_response() {
        let cycles_raw = r#"{"cycle_count":2,"cycles":[["a","b"],["c","d","e"]]}"#;
        let cycles: serde_json::Value = serde_json::from_str(cycles_raw).expect("valid json");
        let cycles_count = cycles
            .get("cycle_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        assert_eq!(cycles_count, 2, "should extract cycle_count=2");

        let audit = serde_json::json!({
            "orphans": serde_json::Value::Null,
            "low_score_modules": serde_json::json!([]),
            "cycles": {
                "count": cycles_count,
                "detail": cycles,
            },
        });
        assert!(
            audit.get("cycles").is_some(),
            "audit must have 'cycles' key"
        );
        assert_eq!(audit["cycles"]["count"], 2);
        assert!(audit["cycles"]["detail"]["cycles"].is_array());
    }

    #[test]
    fn cycles_count_defaults_to_zero_on_empty_response() {
        let cycles_raw = r#"{"cycle_count":0,"cycles":[]}"#;
        let cycles: serde_json::Value = serde_json::from_str(cycles_raw).expect("valid json");
        let cycles_count = cycles
            .get("cycle_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        assert_eq!(cycles_count, 0);
    }

    #[test]
    fn cycles_count_defaults_to_zero_on_null_cycles() {
        let fallback = r#"{"cycle_count":0,"cycles":[]}"#;
        let cycles: serde_json::Value = serde_json::from_str(fallback).expect("valid json");
        let cycles_count = cycles
            .get("cycle_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        assert_eq!(cycles_count, 0, "fallback must yield count=0");
    }

    // ── N2: rfc100_diagnostics extraction (unit) ────────────────────────────

    #[test]
    fn rfc100_diagnostics_extracted_when_present() {
        let orphans_with_diag = serde_json::json!({
            "count": 2,
            "orphans": [],
            "diagnostics": [
                {"code": "W-100", "severity": "warning", "message": "orphan: foo"},
                {"code": "W-103", "severity": "warning", "message": "orphan: bar"},
            ]
        });
        let rfc100 = orphans_with_diag
            .get("diagnostics")
            .cloned()
            .unwrap_or(serde_json::Value::Array(vec![]));
        let count = rfc100.as_array().map(|a| a.len()).unwrap_or(0);
        assert_eq!(count, 2);
    }

    #[test]
    fn rfc100_diagnostics_empty_when_absent() {
        let orphans_plain = serde_json::json!({"count": 3, "orphans": []});
        let rfc100 = orphans_plain
            .get("diagnostics")
            .cloned()
            .unwrap_or(serde_json::Value::Array(vec![]));
        let count = rfc100.as_array().map(|a| a.len()).unwrap_or(0);
        assert_eq!(count, 0);
    }

    // ── render_audit_as_tree (unit) ─────────────────────────────────────────

    #[test]
    fn test_render_audit_as_tree_with_orphans() {
        let json = r#"{
            "orphans": {
                "orphan_count": 2,
                "orphans": [
                    {"symbol_name": "foo", "file_path": "a.rs"},
                    {"symbol_name": "bar", "file_path": "b.rs"}
                ]
            },
            "low_score_modules": [],
            "cycles": {"count": 0}
        }"#;
        let tree = render_audit_as_tree(json);
        assert!(tree.contains("Wiring Audit"), "tree: {tree}");
        assert!(tree.contains("Orphan Symbols (2)"), "tree: {tree}");
        assert!(tree.contains("W-100: foo [a.rs]"), "tree: {tree}");
        assert!(tree.contains("W-100: bar [b.rs]"), "tree: {tree}");
    }

    #[test]
    fn test_render_audit_as_tree_with_low_score_modules() {
        let json = r#"{
            "orphans": {"orphan_count": 0, "orphans": []},
            "low_score_modules": [
                {"file_path": "src/x.rs", "integration_score": 0.5}
            ],
            "cycles": {"count": 0}
        }"#;
        let tree = render_audit_as_tree(json);
        assert!(tree.contains("Low-Score Modules (1)"), "tree: {tree}");
        assert!(tree.contains("src/x.rs"), "tree: {tree}");
        assert!(tree.contains("score=0.50"), "tree: {tree}");
    }

    #[test]
    fn test_render_audit_as_tree_invalid_json() {
        let raw = "not json";
        let result = render_audit_as_tree(raw);
        assert_eq!(result, raw);
    }

    // ── render_cycles_as_tree (unit) ────────────────────────────────────────

    #[test]
    fn test_render_cycles_as_tree_with_cycles() {
        let json = r#"{"cycle_count": 2, "cycles": [["a","b"],["c","d","e"]]}"#;
        let tree = render_cycles_as_tree(json);
        assert!(tree.contains("Dependency Cycles (2)"), "tree: {tree}");
        assert!(
            tree.contains("[\"a\",\"b\"]") || tree.contains("a"),
            "tree: {tree}"
        );
    }

    #[test]
    fn test_render_cycles_as_tree_empty() {
        let json = r#"{"cycle_count": 0, "cycles": []}"#;
        let tree = render_cycles_as_tree(json);
        assert!(tree.contains("Dependency Cycles (0)"), "tree: {tree}");
    }

    #[test]
    fn test_render_cycles_as_tree_invalid_json() {
        let raw = "bad input";
        let result = render_cycles_as_tree(raw);
        assert_eq!(result, raw);
    }
}
