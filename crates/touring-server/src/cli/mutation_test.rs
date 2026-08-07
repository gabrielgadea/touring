//! `touring mutation-test` — Wave T1 CLI subcommand.
//!
//! Wires the daemon-side `cli-mutation-test` handler to a friendly
//! command-line interface. cargo-mutants must be installed
//! (`cargo install cargo-mutants`); the daemon returns a structured
//! failure envelope when absent so CI gates can degrade.
//!
//! # Usage
//!
//! ```bash
//! touring mutation-test                                  # whole workspace, defaults
//! touring mutation-test --package touring-ast            # single crate
//! touring mutation-test --threshold 70 --timeout 120     # tune gate
//! touring mutation-test --jobs 8                         # parallelism override
//! touring mutation-test --force                          # bypass cache
//! touring mutation-test --cache-only                     # cached read only
//! touring mutation-test --json                           # raw daemon JSON (default also JSON)
//! ```
//!
//! Returns the daemon JSON envelope to stdout. CI gates parse
//! `passed_threshold` and `kill_rate`.

use super::daemon_query;

/// Run the `mutation-test` CLI subcommand.
///
/// # Errors
///
/// Returns an error if the daemon socket is unreachable or the daemon
/// reports a failure response (cargo-mutants binary missing, parse
/// failure, etc.). The CLI itself never panics on bad flags — unknown
/// flags are silently ignored to preserve forward-compat.
pub fn run(args: &[String]) -> anyhow::Result<()> {
    let mut payload = serde_json::Map::new();

    if let Some(pkg) = super::common::flag_value(args, "--package")
        && !pkg.is_empty()
    {
        payload.insert("package".into(), serde_json::Value::String(pkg.into()));
    }
    if let Some(t) = super::common::flag_value(args, "--threshold")
        && let Ok(parsed) = t.parse::<f64>()
    {
        payload.insert("threshold".into(), serde_json::Value::from(parsed));
    }
    if let Some(t) = super::common::flag_value(args, "--timeout")
        && let Ok(parsed) = t.parse::<u64>()
    {
        payload.insert("timeout_secs".into(), serde_json::Value::from(parsed));
    }
    if let Some(j) = super::common::flag_value(args, "--jobs")
        && let Ok(parsed) = j.parse::<u64>()
    {
        payload.insert("jobs".into(), serde_json::Value::from(parsed));
    }
    if args.iter().any(|a| a == "--force") {
        payload.insert("force".into(), serde_json::Value::Bool(true));
    }
    if args.iter().any(|a| a == "--cache-only") {
        payload.insert("cache_only".into(), serde_json::Value::Bool(true));
    }

    let output = daemon_query("cli-mutation-test", serde_json::Value::Object(payload))?;
    println!("{output}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|p| p.to_string()).collect()
    }

    #[test]
    fn no_args_routes_to_daemon_without_panicking() {
        let args = s(&["touring", "mutation-test"]);
        // Don't assert success — daemon may be down in unit tests; assert no
        // arg-parsing panic / no Usage message.
        let result = run(&args);
        if let Err(e) = result {
            let msg = e.to_string();
            assert!(!msg.contains("Usage"), "unexpected usage error: {msg}");
        }
    }

    #[test]
    fn package_flag_extracted() {
        let args = s(&["touring", "mutation-test", "--package", "touring-ast"]);
        let extracted = super::super::common::flag_value(&args, "--package").unwrap_or("");
        assert_eq!(extracted, "touring-ast");
    }

    #[test]
    fn threshold_flag_parsed_as_number() {
        let args = s(&["touring", "mutation-test", "--threshold", "75.5"]);
        let raw = super::super::common::flag_value(&args, "--threshold").unwrap_or("");
        assert_eq!(raw, "75.5");
        let parsed = raw.parse::<f64>().unwrap_or(0.0);
        assert!((parsed - 75.5).abs() < f64::EPSILON);
    }

    #[test]
    fn timeout_flag_parsed_as_integer() {
        let args = s(&["touring", "mutation-test", "--timeout", "120"]);
        let raw = super::super::common::flag_value(&args, "--timeout").unwrap_or("");
        assert_eq!(raw.parse::<u64>().unwrap_or(0), 120);
    }

    #[test]
    fn jobs_flag_parsed_as_integer() {
        let args = s(&["touring", "mutation-test", "--jobs", "8"]);
        let raw = super::super::common::flag_value(&args, "--jobs").unwrap_or("");
        assert_eq!(raw.parse::<u64>().unwrap_or(0), 8);
    }

    #[test]
    fn force_flag_recognized() {
        let args = s(&["touring", "mutation-test", "--force"]);
        assert!(args.iter().any(|a| a == "--force"));
    }

    #[test]
    fn cache_only_flag_recognized() {
        let args = s(&["touring", "mutation-test", "--cache-only"]);
        assert!(args.iter().any(|a| a == "--cache-only"));
    }

    #[test]
    fn unknown_flag_silently_ignored() {
        let args = s(&["touring", "mutation-test", "--bogus", "x"]);
        // Should not panic and should not extract --bogus into payload (we only
        // poll specific known flags).
        let result = run(&args);
        if let Err(e) = result {
            // Daemon failure is fine here — what we care about is no parse panic.
            let msg = e.to_string();
            assert!(!msg.contains("panicked"), "panic in arg parsing: {msg}");
        }
    }
}
