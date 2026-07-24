//! `touring gotcha list|add|match|stats|sync|init` — Gotcha pitfall database management.
//!
//! Subcommands:
//!   list              List all known gotchas (default)
//!   add `<pat> <desc>`  Add a new gotcha (--severity low|medium|high|critical)
//!   match `<file>`      Find gotchas matching a file path
//!   stats             Gotcha statistics (total, resolved, unresolved)
//!   sync              Sync gotchas from YAML rule library to SQLite cache (Wave Q3)
//!   init              Bootstrap YAML files from existing SQLite gotcha cache (Wave Q3)
//!
//! Wave P3-1.3 W3 (2026-06-11): migrated from manual positional parsing
//! (`args.get()` + `flag_value`) to clap derive. All payload keys and
//! conditions preserved (G6). Internal helper functions `cmd_*` are preserved
//! so existing tests in `mod tests` continue to compile without change.

use super::daemon_query;

/// Expose this handler's clap Command for the completions aggregator (W7).
pub(super) fn command() -> clap::Command {
    use clap::CommandFactory;
    GotchaCli::command()
}

#[cfg(test)]
use super::common::{args_joined, flag_value};
use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "touring gotcha",
    bin_name = "touring gotcha",
    about = "Gotcha pitfall database: list, add, match, stats, sync, init",
    disable_help_subcommand = true
)]
struct GotchaCli {
    #[command(subcommand)]
    cmd: Option<GotchaCmd>,
}

#[derive(Subcommand, Debug)]
enum GotchaCmd {
    /// List all known gotchas (default).
    List,
    /// Add a new gotcha to the pitfall database.
    Add {
        /// Glob or regex pattern that triggers the gotcha.
        pattern: String,
        /// Description words (remaining positional args, joined).
        /// Words are collected up to (but not including) `--severity`.
        description: Vec<String>,
        /// Severity level: low, medium (default), high, or critical.
        #[arg(long, default_value = "medium")]
        severity: String,
    },
    /// Find gotchas matching a file path.
    #[command(name = "match")]
    Match {
        /// File path to match gotchas against.
        file_path: String,
    },
    /// Show gotcha statistics (total, resolved, unresolved).
    Stats,
    /// Sync gotchas from YAML rule library to SQLite cache (Wave Q3).
    Sync {
        /// Directory containing YAML gotcha files (default: ~/.claude/rust/docs/gotchas).
        #[arg(long)]
        dir: Option<String>,
    },
    /// Bootstrap YAML files from existing SQLite gotcha cache (Wave Q3, one-shot).
    Init {
        /// Output directory for generated YAML files.
        #[arg(long)]
        output: String,
    },
}

/// Dispatch to the appropriate gotcha subcommand.
pub fn run(args: &[String]) -> anyhow::Result<()> {
    let cli = match GotchaCli::try_parse_from(args.iter().skip(1)) {
        Ok(cli) => cli,
        Err(e) => e.exit(),
    };

    match cli.cmd.unwrap_or(GotchaCmd::List) {
        GotchaCmd::List => cmd_list(),
        GotchaCmd::Add {
            pattern,
            description,
            severity,
        } => {
            // Reproduce original logic: description words are collected until
            // `--severity`; clap already strips the flag so `description`
            // contains only the positional words.
            let desc_text = description.join(" ");
            if desc_text.is_empty() {
                anyhow::bail!(
                    "Usage: touring gotcha add <pattern> <description> [--severity <sev>]"
                );
            }
            let payload = serde_json::json!({
                "pattern": pattern,
                "description": desc_text,
                "severity": severity,
            });
            let output = daemon_query("cli-gotcha-add", payload)?;
            println!("{output}");
            Ok(())
        }
        GotchaCmd::Match { file_path } => {
            let payload = serde_json::json!({ "file_path": file_path });
            let output = daemon_query("cli-gotcha-match", payload)?;
            println!("{output}");
            Ok(())
        }
        GotchaCmd::Stats => cmd_stats(),
        GotchaCmd::Sync { dir } => {
            let payload = match dir {
                Some(d) => serde_json::json!({ "dir": d }),
                None => serde_json::json!({}),
            };
            let output = daemon_query("cli-gotcha-sync", payload)?;
            println!("{output}");
            Ok(())
        }
        GotchaCmd::Init { output } => {
            let payload = serde_json::json!({ "output_dir": output });
            let out = daemon_query("cli-gotcha-init", payload)?;
            println!("{out}");
            Ok(())
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Internal helpers (preserved for backwards-compat with existing tests)
// ─────────────────────────────────────────────────────────────────────────────

fn cmd_list() -> anyhow::Result<()> {
    let output = daemon_query("cli-gotcha-list", serde_json::json!({}))?;
    println!("{output}");
    Ok(())
}

fn cmd_stats() -> anyhow::Result<()> {
    let output = daemon_query("cli-gotcha-stats", serde_json::json!({}))?;
    println!("{output}");
    Ok(())
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

    fn parse(args: &[&str]) -> GotchaCli {
        GotchaCli::try_parse_from(args).expect("args should parse")
    }

    // ── Subcommand routing ──────────────────────────────────────────────

    #[test]
    fn default_subcommand_is_list() {
        let cli = parse(&["gotcha"]);
        assert!(cli.cmd.is_none()); // run() maps None -> List
    }

    #[test]
    fn explicit_list_subcommand() {
        let cli = parse(&["gotcha", "list"]);
        assert!(matches!(cli.cmd, Some(GotchaCmd::List)));
    }

    #[test]
    fn stats_subcommand_recognized() {
        let cli = parse(&["gotcha", "stats"]);
        assert!(matches!(cli.cmd, Some(GotchaCmd::Stats)));
    }

    #[test]
    fn match_subcommand_recognized() {
        let cli = parse(&["gotcha", "match", "src/main.rs"]);
        let Some(GotchaCmd::Match { file_path }) = cli.cmd else {
            panic!("expected Match");
        };
        assert_eq!(file_path, "src/main.rs");
    }

    // ── Unknown subcommand error ────────────────────────────────────────

    #[test]
    fn unknown_subcommand_produces_error() {
        assert!(GotchaCli::try_parse_from(["gotcha", "delete"]).is_err());
    }

    // ── `add` argument parsing ──────────────────────────────────────────

    #[test]
    fn add_missing_pattern_errors() {
        // clap rejects missing required positional
        assert!(GotchaCli::try_parse_from(["gotcha", "add"]).is_err());
    }

    #[test]
    fn add_parses_severity_flag() {
        let cli = parse(&[
            "gotcha",
            "add",
            "*.rs",
            "watch",
            "out",
            "--severity",
            "high",
        ]);
        let Some(GotchaCmd::Add { severity, .. }) = cli.cmd else {
            panic!("expected Add");
        };
        assert_eq!(severity, "high");
    }

    #[test]
    fn add_severity_defaults_to_medium() {
        let cli = parse(&["gotcha", "add", "*.rs", "be", "careful"]);
        let Some(GotchaCmd::Add { severity, .. }) = cli.cmd else {
            panic!("expected Add");
        };
        assert_eq!(severity, "medium");
    }

    #[test]
    fn add_description_collected() {
        let cli = parse(&["gotcha", "add", "*.rs", "watch", "out"]);
        let Some(GotchaCmd::Add { description, .. }) = cli.cmd else {
            panic!("expected Add");
        };
        assert_eq!(description.join(" "), "watch out");
    }

    #[test]
    fn add_description_stops_before_severity_flag() {
        let cli = parse(&[
            "gotcha",
            "add",
            "*.rs",
            "watch",
            "out",
            "--severity",
            "high",
        ]);
        let Some(GotchaCmd::Add { description, .. }) = cli.cmd else {
            panic!("expected Add");
        };
        // clap strips --severity and its value; remaining positionals = description
        assert_eq!(description.join(" "), "watch out");
    }

    // ── `match` argument parsing ────────────────────────────────────────

    #[test]
    fn match_missing_file_path_errors() {
        assert!(GotchaCli::try_parse_from(["gotcha", "match"]).is_err());
    }

    #[test]
    fn match_extracts_file_path() {
        let cli = parse(&["gotcha", "match", "src/main.rs"]);
        let Some(GotchaCmd::Match { file_path }) = cli.cmd else {
            panic!("expected Match");
        };
        assert_eq!(file_path, "src/main.rs");
    }

    // ── Legacy helpers still usable in tests ───────────────────────────

    #[test]
    fn add_description_includes_all_words_without_flag() {
        let args = s(&["touring", "gotcha", "add", "*.rs", "be", "very", "careful"]);
        let desc = args_joined(&args, 4);
        assert_eq!(desc, "be very careful");
    }

    #[test]
    fn add_parses_severity_flag_via_common() {
        let args = s(&[
            "touring",
            "gotcha",
            "add",
            "*.rs",
            "watch",
            "out",
            "--severity",
            "high",
        ]);
        let severity = flag_value(&args, "--severity").unwrap_or("medium");
        assert_eq!(severity, "high");
    }

    // ── Sync / Init ─────────────────────────────────────────────────────

    #[test]
    fn sync_no_dir() {
        let cli = parse(&["gotcha", "sync"]);
        let Some(GotchaCmd::Sync { dir }) = cli.cmd else {
            panic!("expected Sync");
        };
        assert!(dir.is_none());
    }

    #[test]
    fn sync_with_dir() {
        let cli = parse(&["gotcha", "sync", "--dir", "/tmp/yaml"]);
        let Some(GotchaCmd::Sync { dir }) = cli.cmd else {
            panic!("expected Sync");
        };
        assert_eq!(dir.as_deref(), Some("/tmp/yaml"));
    }

    #[test]
    fn init_missing_output_errors() {
        assert!(GotchaCli::try_parse_from(["gotcha", "init"]).is_err());
    }

    #[test]
    fn init_with_output() {
        let cli = parse(&["gotcha", "init", "--output", "/tmp/out"]);
        let Some(GotchaCmd::Init { output }) = cli.cmd else {
            panic!("expected Init");
        };
        assert_eq!(output, "/tmp/out");
    }
}
