//! `touring repo-score` — Wave R1 executive KPI dashboard.
//!
//! Aggregate 11-category 0-269 composite repository score with A+..F letter
//! grade, RFC-100 diagnostics for low-scoring categories, and optional
//! threshold-based exit codes for CI integration.
//!
//! # Examples
//!
//! ```bash
//! # Full JSON dashboard
//! touring repo-score -j
//!
//! # Inspect just one category
//! touring repo-score --category architecture -j
//!
//! # Fail CI when score drops below 200/269 (≈ B grade)
//! touring repo-score --threshold 200 -j
//! ```

use super::daemon_query;

/// Run the `repo-score` CLI subcommand.
///
/// Flags:
/// - `--category <name>`: narrow output to a single category
/// - `--threshold <N>`: exit non-zero when total_score < N (CI gate)
///
/// # Errors
///
/// Returns an error if the daemon socket is unreachable or the daemon
/// reports a failure response.
pub fn run(args: &[String]) -> anyhow::Result<()> {
    let category = super::common::flag_value(args, "--category").unwrap_or("");
    let threshold =
        super::common::flag_value(args, "--threshold").and_then(|s| s.parse::<u32>().ok());

    let payload = if category.is_empty() {
        serde_json::json!({})
    } else {
        serde_json::json!({"category": category})
    };

    let output = daemon_query("cli-repo-score", payload)?;
    println!("{output}");

    if let Some(min) = threshold {
        let parsed: serde_json::Value =
            serde_json::from_str(&output).unwrap_or(serde_json::Value::Null);
        let score = parsed
            .get("total_score")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0) as u32;
        if score < min {
            anyhow::bail!(
                "repo-score {score} below threshold {min} (grade={})",
                parsed
                    .get("grade")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("?")
            );
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|p| p.to_string()).collect()
    }

    #[test]
    fn no_args_routes_to_daemon() {
        // run() will hit daemon — we verify it doesn't bail BEFORE reaching daemon_query.
        let args = s(&["touring", "repo-score"]);
        let result = run(&args);
        if let Err(e) = result {
            let msg = e.to_string();
            // Acceptable failures: daemon down. Unacceptable: argument parsing error.
            assert!(
                !msg.contains("Usage"),
                "should not emit usage error for valid no-arg invocation: {msg}"
            );
        }
    }

    #[test]
    fn category_flag_extracted() {
        let args = s(&["touring", "repo-score", "--category", "architecture"]);
        let extracted = super::super::common::flag_value(&args, "--category").unwrap_or("");
        assert_eq!(extracted, "architecture");
    }

    #[test]
    fn threshold_flag_parses_to_number() {
        let args = s(&["touring", "repo-score", "--threshold", "200"]);
        let extracted = super::super::common::flag_value(&args, "--threshold")
            .and_then(|s| s.parse::<u32>().ok());
        assert_eq!(extracted, Some(200));
    }

    #[test]
    fn threshold_flag_invalid_value_returns_none() {
        let args = s(&["touring", "repo-score", "--threshold", "abc"]);
        let extracted = super::super::common::flag_value(&args, "--threshold")
            .and_then(|s| s.parse::<u32>().ok());
        assert_eq!(extracted, None);
    }
}
