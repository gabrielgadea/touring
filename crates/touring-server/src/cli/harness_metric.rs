//! `touring harness-metric` — R5/OP1 unified harness-quality KPI.
//!
//! Queries the daemon for the current `HarnessQuality` aggregation: the six
//! harness dimensions (executable, inspectable, stateful, governed, performant,
//! evolving) each in `0.0..=1.0`, plus their `composite`. This is the single
//! elite KPI a human or the evolution agent watches climb as the code-agent
//! harness becomes more elite — built from the live `GateMetricsSnapshot`
//! counters by `touring_hooks::gateway::harness_metric::HarnessQuality`.
//!
//! For the per-execution, per-axis view (the `EvidenceBundle` behind a single
//! `GateDecision`), use `touring exec -j` instead (S-05).
//!
//! # Usage
//!
//! ```text
//! touring harness-metric           — pretty-print harness KPI (human-readable JSON)
//! touring harness-metric -j        — print raw compact JSON from daemon
//! touring harness-metric --json    — same as -j
//! touring harness-metric -h        — show this help and exit 0
//! touring harness-metric --help    — same as -h
//! ```
//!
//! # Output schema
//!
//! The daemon returns a JSON envelope matching `HarnessQuality`:
//!
//! ```json
//! {
//!   "executable":  0.0..=1.0,
//!   "inspectable": 0.0..=1.0,
//!   "stateful":    0.0..=1.0,
//!   "governed":    0.0..=1.0,
//!   "performant":  0.0..=1.0,
//!   "evolving":    0.0..=1.0,
//!   "composite":   0.0..=1.0
//! }
//! ```
//!
//! Watch `composite` climb toward `1.0` (Diamond tier) as the harness
//! receives new gate integrations and RL reward signal.
//!
//! # Architecture note
//!
//! This handler is intentionally thin: it delegates all computation to the
//! daemon's `cli-harness-metric` RPC handler which aggregates the live
//! `GateMetricsSnapshot` counters in-process. The CLI layer only handles
//! argument parsing and output formatting.

use super::daemon_query;

/// Usage text printed on `-h` / `--help`.
const USAGE: &str = "\
touring harness-metric [OPTIONS]

OPTIONS:
  -j, --json   Print raw compact JSON from daemon (default: pretty-printed)
  -h, --help   Show this help and exit 0

DESCRIPTION:
  Queries the daemon for the HarnessQuality KPI — six dimensions
  (executable, inspectable, stateful, governed, performant, evolving)
  each in 0.0..=1.0 and their composite score.

  Watch 'composite' climb toward 1.0 (Diamond tier) as the code-agent
  harness receives new gate integrations and RL reward signal.

  For per-execution evidence bundles use 'touring exec -j' (S-05).
";

/// Parsed flags for the `harness-metric` subcommand.
///
/// Encapsulates flag state so that the `run` entry point stays simple and
/// flag-parsing logic can be tested independently.
struct HarnessFlags {
    /// Request compact JSON output instead of pretty-printed human form.
    json: bool,
    /// Print help text and exit (return `Ok(())`).
    help: bool,
}

impl HarnessFlags {
    /// Parse `harness-metric` flags from the raw argument slice.
    ///
    /// Recognises:
    /// - `-j` / `--json` → compact JSON mode
    /// - `-h` / `--help` → show usage
    ///
    /// Unknown arguments are silently ignored so that future flag additions
    /// remain backward-compatible.
    fn parse(args: &[String]) -> Self {
        let json = args.iter().any(|a| a == "-j" || a == "--json");
        let help = args.iter().any(|a| a == "-h" || a == "--help");
        Self { json, help }
    }
}

/// Format a raw daemon JSON string for human display.
///
/// Pretty-prints valid JSON; falls back to the raw string unchanged when the
/// daemon returns an unexpected non-JSON envelope.  This ensures the output is
/// always readable even under daemon version skew.
fn format_human(raw: &str) -> String {
    serde_json::from_str::<serde_json::Value>(raw)
        .map(|v| serde_json::to_string_pretty(&v).unwrap_or_else(|_| raw.to_owned()))
        .unwrap_or_else(|_| raw.to_owned())
}

/// Run the `harness-metric` CLI subcommand.
///
/// Dispatches on parsed `HarnessFlags`:
/// - `--help` / `-h` → print `USAGE` to stdout and return `Ok(())`.
/// - `--json` / `-j` → query daemon, emit compact JSON.
/// - (default)       → query daemon, emit pretty-printed JSON.
///
/// # Errors
///
/// Returns an error if the daemon socket is unreachable or the daemon reports
/// a failure response (propagated via `?`).
pub fn run(args: &[String]) -> anyhow::Result<()> {
    let flags = HarnessFlags::parse(args);
    if flags.help {
        print!("{USAGE}");
        return Ok(());
    }
    let output = daemon_query("cli-harness-metric", serde_json::json!({}))?;
    if flags.json {
        println!("{output}");
    } else {
        println!("{}", format_human(&output));
    }
    Ok(())
}

// ─── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn sv(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|s| s.to_string()).collect()
    }

    // ── HarnessFlags::parse ───────────────────────────────────────────────

    #[test]
    fn parse_flags_empty_has_no_flags() {
        let f = HarnessFlags::parse(&sv(&[]));
        assert!(!f.json);
        assert!(!f.help);
    }

    #[test]
    fn parse_flags_short_json() {
        let f = HarnessFlags::parse(&sv(&["-j"]));
        assert!(f.json);
        assert!(!f.help);
    }

    #[test]
    fn parse_flags_long_json() {
        let f = HarnessFlags::parse(&sv(&["--json"]));
        assert!(f.json);
    }

    #[test]
    fn parse_flags_help_short() {
        let f = HarnessFlags::parse(&sv(&["-h"]));
        assert!(f.help);
        assert!(!f.json);
    }

    #[test]
    fn parse_flags_help_long() {
        let f = HarnessFlags::parse(&sv(&["--help"]));
        assert!(f.help);
    }

    #[test]
    fn parse_flags_json_and_help_together() {
        let f = HarnessFlags::parse(&sv(&["-j", "--help"]));
        assert!(f.json);
        assert!(f.help);
    }

    #[test]
    fn parse_flags_unknown_arg_ignored() {
        let f = HarnessFlags::parse(&sv(&["--unknown-flag"]));
        assert!(!f.json);
        assert!(!f.help);
    }

    // ── format_human ─────────────────────────────────────────────────────

    #[test]
    fn format_human_valid_json_is_pretty() {
        let raw = r#"{"composite":0.95,"executable":1.0}"#;
        let out = format_human(raw);
        assert!(out.contains("composite"), "must retain key");
        assert!(out.contains("0.95"), "must retain value");
        assert!(
            out.contains('\n'),
            "pretty-printed output contains newlines"
        );
    }

    #[test]
    fn format_human_invalid_json_passes_through() {
        let raw = "daemon unavailable";
        assert_eq!(format_human(raw), raw);
    }

    #[test]
    fn format_human_empty_object() {
        let raw = "{}";
        let out = format_human(raw);
        assert!(out.contains('{'));
    }

    #[test]
    fn format_human_full_schema() {
        let raw = r#"{
            "executable":1.0,"inspectable":0.9,"stateful":0.8,
            "governed":0.7,"performant":0.6,"evolving":0.5,"composite":0.77
        }"#;
        let out = format_human(raw);
        for key in &[
            "executable",
            "inspectable",
            "stateful",
            "governed",
            "performant",
            "evolving",
            "composite",
        ] {
            assert!(out.contains(key), "missing key: {key}");
        }
    }

    // ── USAGE constant sanity ─────────────────────────────────────────────

    #[test]
    fn usage_mentions_json_flag() {
        assert!(USAGE.contains("--json") || USAGE.contains("-j"));
    }

    #[test]
    fn usage_mentions_help_flag() {
        assert!(USAGE.contains("--help") || USAGE.contains("-h"));
    }
}
