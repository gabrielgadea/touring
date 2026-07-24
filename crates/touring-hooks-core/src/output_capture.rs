//! OutputCapture — Captures command output and extracts structured metrics.
//!
//! Inspired by autoresearch pattern P6: Output Redirection + Selective Extraction.
//! Training output is redirected to file, agent receives only structured metrics.
//!
//! ```text
//! command output (potentially large)
//!     → OutputCapture { full_log, metrics, summary, exit_code }
//!         → agent receives only `summary` (≤500 chars)
//!         → full_log available on-demand for debugging
//! ```
//!
//! ## Extractors
//!
//! Each `MetricsExtractor` implementation handles a specific command type:
//! - `PytestExtractor`: pytest output → passed/failed/coverage/duration
//! - `CargoTestExtractor`: cargo test → passed/failed/ignored/duration
//! - `RuffExtractor`: ruff check → errors/warnings/fixed
//! - `GenericExtractor`: fallback → exit_code/line_count

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Threshold in bytes above which output is captured (not passed through).
const CAPTURE_THRESHOLD: usize = 1024;

/// Maximum summary length in characters.
const MAX_SUMMARY_LEN: usize = 500;

/// Captured command output with selective extraction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputCapture {
    /// Structured metrics extracted from output.
    pub metrics: HashMap<String, Value>,
    /// Human-readable summary (≤500 chars, for LLM context).
    pub summary: String,
    /// Process exit code.
    pub exit_code: i64,
    /// Output size in bytes.
    pub output_bytes: usize,
    /// Whether output was captured (exceeded threshold).
    pub was_captured: bool,
    /// Name of the extractor that processed this output.
    pub extractor_name: String,
}

/// Result from a metrics extraction.
#[derive(Debug, Clone)]
pub struct ExtractionResult {
    /// Structured metrics.
    pub metrics: HashMap<String, Value>,
    /// 1-3 line summary for LLM context.
    pub summary: String,
}

/// Trait for extracting structured metrics from command output.
pub trait MetricsExtractor: Send + Sync {
    /// Name of this extractor.
    fn name(&self) -> &str;

    /// Whether this extractor should handle the given command.
    fn matches(&self, command: &str) -> bool;

    /// Extract metrics and summary from output.
    fn extract(&self, stdout: &str, exit_code: i64) -> ExtractionResult;
}

// ── Registry ────────────────────────────────────────────────────────────

/// Process command output through the extractor pipeline.
///
/// Returns `None` if output is below threshold (pass through).
/// Returns `Some(OutputCapture)` if output was captured and summarized.
pub fn capture_output(command: &str, output: &str, exit_code: i64) -> Option<OutputCapture> {
    let output_bytes = output.len();

    // Below threshold: pass through (don't penalize short commands)
    if output_bytes <= CAPTURE_THRESHOLD {
        return None;
    }

    let extractors: Vec<Box<dyn MetricsExtractor>> = vec![
        Box::new(PytestExtractor),
        Box::new(CargoTestExtractor),
        Box::new(RuffExtractor),
        Box::new(GenericExtractor),
    ];

    // Find first matching extractor
    let extractor = extractors
        .iter()
        .find(|e| e.matches(command))
        .unwrap_or(extractors.last().expect("extractors non-empty"));

    let result = extractor.extract(output, exit_code);

    let summary = if result.summary.len() > MAX_SUMMARY_LEN {
        // Find nearest char boundary to avoid panicking on multi-byte UTF-8
        let mut end = MAX_SUMMARY_LEN - 3;
        while end > 0 && !result.summary.is_char_boundary(end) {
            end -= 1;
        }
        format!("{}...", &result.summary[..end])
    } else {
        result.summary
    };

    Some(OutputCapture {
        metrics: result.metrics,
        summary,
        exit_code,
        output_bytes,
        was_captured: true,
        extractor_name: extractor.name().to_string(),
    })
}

// ── Extractors ──────────────────────────────────────────────────────────

/// Extracts metrics from pytest output.
pub struct PytestExtractor;

impl MetricsExtractor for PytestExtractor {
    fn name(&self) -> &str {
        "pytest"
    }

    fn matches(&self, command: &str) -> bool {
        command.contains("pytest") || command.contains("py.test")
    }

    fn extract(&self, stdout: &str, exit_code: i64) -> ExtractionResult {
        let mut metrics = HashMap::new();
        let mut passed = 0u64;
        let mut failed = 0u64;
        let mut errors = 0u64;
        let mut warnings = 0u64;
        let mut duration = String::new();
        let mut coverage_pct: Option<f64> = None;

        for line in stdout.lines().rev().take(20) {
            let trimmed = line.trim();

            // "X passed, Y failed, Z error in Ns"
            if trimmed.contains("passed") || trimmed.contains("failed") {
                for part in trimmed.split(',') {
                    let part = part.trim();
                    if let Some(n) = extract_leading_number(part) {
                        if part.contains("passed") {
                            passed = n;
                        } else if part.contains("failed") {
                            failed = n;
                        } else if part.contains("error") {
                            errors = n;
                        } else if part.contains("warning") {
                            warnings = n;
                        }
                    }
                    if part.contains(" in ") {
                        if let Some(d) = part.split(" in ").last() {
                            duration = d.trim().to_string();
                        }
                    }
                }
            }

            // "TOTAL    X%"
            if trimmed.starts_with("TOTAL") || trimmed.contains("% cover") {
                if let Some(pct) = extract_percentage(trimmed) {
                    coverage_pct = Some(pct);
                }
            }
        }

        metrics.insert("passed".into(), Value::from(passed));
        metrics.insert("failed".into(), Value::from(failed));
        metrics.insert("errors".into(), Value::from(errors));
        metrics.insert("warnings".into(), Value::from(warnings));
        if !duration.is_empty() {
            metrics.insert("duration".into(), Value::from(duration.clone()));
        }
        if let Some(cov) = coverage_pct {
            metrics.insert("coverage_pct".into(), Value::from(cov));
        }

        let status = if exit_code == 0 { "PASS" } else { "FAIL" };
        let cov_str = coverage_pct
            .map(|c| format!(" ({c:.0}% coverage)"))
            .unwrap_or_default();
        let dur_str = if duration.is_empty() {
            String::new()
        } else {
            format!(" in {duration}")
        };

        let summary = format!(
            "{status}: {passed} passed, {failed} failed, {errors} errors{cov_str}{dur_str}"
        );

        ExtractionResult { metrics, summary }
    }
}

/// Extracts metrics from `cargo test` output.
pub struct CargoTestExtractor;

impl MetricsExtractor for CargoTestExtractor {
    fn name(&self) -> &str {
        "cargo_test"
    }

    fn matches(&self, command: &str) -> bool {
        command.contains("cargo test") || command.contains("cargo nextest")
    }

    fn extract(&self, stdout: &str, exit_code: i64) -> ExtractionResult {
        let mut metrics = HashMap::new();
        let mut total_passed = 0u64;
        let mut total_failed = 0u64;
        let mut total_ignored = 0u64;
        let mut duration = String::new();

        for line in stdout.lines().rev().take(30) {
            let trimmed = line.trim();

            // "test result: ok. X passed; Y failed; Z ignored; ..."
            if trimmed.starts_with("test result:") {
                for part in trimmed.split(';') {
                    let part = part.trim();
                    if let Some(n) = extract_leading_number(part) {
                        if part.contains("passed") {
                            total_passed += n;
                        } else if part.contains("failed") {
                            total_failed += n;
                        } else if part.contains("ignored") {
                            total_ignored += n;
                        }
                    }
                }
            }

            // "Finished ... in Xs"
            if trimmed.contains("Finished") && trimmed.contains(" in ") {
                if let Some(d) = trimmed.split(" in ").last() {
                    duration = d.trim().to_string();
                }
            }
        }

        metrics.insert("passed".into(), Value::from(total_passed));
        metrics.insert("failed".into(), Value::from(total_failed));
        metrics.insert("ignored".into(), Value::from(total_ignored));
        if !duration.is_empty() {
            metrics.insert("duration".into(), Value::from(duration.clone()));
        }

        let status = if exit_code == 0 { "ok" } else { "FAILED" };
        let dur_str = if duration.is_empty() {
            String::new()
        } else {
            format!(" in {duration}")
        };
        let summary = format!(
            "test result: {status}. {total_passed} passed; {total_failed} failed; {total_ignored} ignored{dur_str}"
        );

        ExtractionResult { metrics, summary }
    }
}

/// Extracts metrics from `ruff check` output.
pub struct RuffExtractor;

impl MetricsExtractor for RuffExtractor {
    fn name(&self) -> &str {
        "ruff"
    }

    fn matches(&self, command: &str) -> bool {
        command.contains("ruff check") || command.contains("ruff format")
    }

    fn extract(&self, stdout: &str, _exit_code: i64) -> ExtractionResult {
        let mut metrics = HashMap::new();
        let mut error_count = 0u64;
        let mut fixable = 0u64;

        for line in stdout.lines().rev().take(10) {
            let trimmed = line.trim();

            // "Found X errors." or "Found X errors (Y fixable ...)"
            if trimmed.starts_with("Found") && trimmed.contains("error") {
                if let Some(n) = extract_leading_number(trimmed.trim_start_matches("Found").trim())
                {
                    error_count = n;
                }
                if trimmed.contains("fixable") {
                    if let Some(fix_part) = trimmed.split('(').nth(1) {
                        if let Some(n) = extract_leading_number(fix_part.trim()) {
                            fixable = n;
                        }
                    }
                }
            }

            // "All checks passed!"
            if trimmed.contains("All checks passed") {
                error_count = 0;
            }
        }

        metrics.insert("errors".into(), Value::from(error_count));
        metrics.insert("fixable".into(), Value::from(fixable));

        let summary = if error_count == 0 {
            "ruff: All checks passed".to_string()
        } else {
            format!("ruff: {error_count} errors ({fixable} fixable)")
        };

        ExtractionResult { metrics, summary }
    }
}

/// Generic fallback extractor for any command.
pub struct GenericExtractor;

impl MetricsExtractor for GenericExtractor {
    fn name(&self) -> &str {
        "generic"
    }

    fn matches(&self, _command: &str) -> bool {
        true // Always matches as fallback
    }

    fn extract(&self, stdout: &str, exit_code: i64) -> ExtractionResult {
        let mut metrics = HashMap::new();
        let line_count = stdout.lines().count();

        metrics.insert("exit_code".into(), Value::from(exit_code));
        metrics.insert("line_count".into(), Value::from(line_count));
        metrics.insert("byte_count".into(), Value::from(stdout.len()));

        let status = if exit_code == 0 { "OK" } else { "ERROR" };
        let summary = format!(
            "Exit {exit_code} ({status}), {line_count} lines output ({} bytes)",
            stdout.len()
        );

        ExtractionResult { metrics, summary }
    }
}

// ── Helpers ─────────────────────────────────────────────────────────────

/// Extract the first integer found in a string like "=== 42 passed" → 42.
/// Extract first integer from a string like "42 passed" or "=== 42 passed".
fn extract_leading_number(s: &str) -> Option<u64> {
    s.split_whitespace().find_map(|w| w.parse::<u64>().ok())
}

/// Extract percentage from a string like "TOTAL 85%" → 85.0.
/// Prefers words ending with '%' (e.g., "85%") over bare numbers.
fn extract_percentage(s: &str) -> Option<f64> {
    // First pass: look for words ending with '%' (most specific)
    for word in s.split_whitespace() {
        if !word.ends_with('%') {
            continue;
        }
        let cleaned = word.trim_end_matches('%');
        if let Ok(pct) = cleaned.parse::<f64>() {
            if (0.0..=100.0).contains(&pct) {
                return Some(pct);
            }
        }
    }
    // Fallback: first bare number in percentage range (no % suffix)
    for word in s.split_whitespace() {
        if word.ends_with('%') {
            continue; // already handled above
        }
        if let Ok(pct) = word.parse::<f64>() {
            if (0.0..=100.0).contains(&pct) {
                return Some(pct);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    #![allow(clippy::indexing_slicing, clippy::len_zero)]
    use super::*;

    // ── capture_output threshold ────────────────────────────────────

    #[test]
    fn test_below_threshold_passes_through() {
        let result = capture_output("ls -la", "short output", 0);
        assert!(result.is_none(), "Output below 1KB should pass through");
    }

    #[test]
    fn test_above_threshold_captured() {
        let big_output = "x\n".repeat(600); // ~1200 bytes
        let result = capture_output("some command", &big_output, 0);
        assert!(result.is_some(), "Output above 1KB should be captured");
        let cap = result.unwrap();
        assert!(cap.was_captured);
        assert!(cap.summary.len() <= MAX_SUMMARY_LEN);
    }

    // ── PytestExtractor ────────────────────────────────────────────

    #[test]
    fn test_pytest_extractor_matches() {
        let e = PytestExtractor;
        assert!(e.matches("pytest tests/ -v"));
        assert!(e.matches("python -m pytest"));
        assert!(!e.matches("cargo test"));
    }

    #[test]
    fn test_pytest_extract_full() {
        let e = PytestExtractor;
        let output = "\
test_foo.py::test_a PASSED
test_foo.py::test_b PASSED
test_bar.py::test_c FAILED
========= 2 passed, 1 failed, 0 error in 3.45s =========";
        let result = e.extract(output, 1);
        assert_eq!(result.metrics["passed"], 2);
        assert_eq!(result.metrics["failed"], 1);
        assert!(result.summary.contains("FAIL"));
        assert!(result.summary.contains("2 passed"));
    }

    #[test]
    fn test_pytest_extract_with_coverage() {
        let e = PytestExtractor;
        let output = "\
test_foo.py::test_a PASSED
---------- coverage ----------
Name         Stmts   Miss  Cover
TOTAL          100     15    85% cover
========= 5 passed, 0 failed in 1.2s =========";
        let result = e.extract(output, 0);
        assert_eq!(result.metrics["passed"], 5);
        assert_eq!(result.metrics["coverage_pct"], 85.0);
        assert!(result.summary.contains("PASS"));
        assert!(result.summary.contains("85% coverage"));
    }

    // ── CargoTestExtractor ─────────────────────────────────────────

    #[test]
    fn test_cargo_test_extractor_matches() {
        let e = CargoTestExtractor;
        assert!(e.matches("cargo test --workspace"));
        assert!(e.matches("cargo nextest run"));
        assert!(!e.matches("pytest"));
    }

    #[test]
    fn test_cargo_test_extract() {
        let e = CargoTestExtractor;
        let output = "\
running 27 tests
test tests::test_a ... ok
test tests::test_b ... ok
test tests::test_c ... FAILED

test result: ok. 26 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; Finished in 2.5s";
        let result = e.extract(output, 1);
        assert_eq!(result.metrics["passed"], 26);
        assert_eq!(result.metrics["failed"], 1);
        assert!(result.summary.contains("26 passed"));
    }

    #[test]
    fn test_cargo_test_all_pass() {
        let e = CargoTestExtractor;
        let output = "\
running 15 tests
...
test result: ok. 15 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out; Finished in 0.8s";
        let result = e.extract(output, 0);
        assert_eq!(result.metrics["passed"], 15);
        assert_eq!(result.metrics["failed"], 0);
        assert_eq!(result.metrics["ignored"], 2);
        assert!(result.summary.contains("ok"));
    }

    // ── RuffExtractor ──────────────────────────────────────────────

    #[test]
    fn test_ruff_extractor_matches() {
        let e = RuffExtractor;
        assert!(e.matches("ruff check ."));
        assert!(e.matches("ruff format --check src/"));
        assert!(!e.matches("cargo clippy"));
    }

    #[test]
    fn test_ruff_extract_errors() {
        let e = RuffExtractor;
        let output = "\
src/foo.py:10:1: F401 `os` imported but unused
src/bar.py:5:1: E501 Line too long
Found 2 errors (1 fixable with --fix).";
        let result = e.extract(output, 1);
        assert_eq!(result.metrics["errors"], 2);
        assert_eq!(result.metrics["fixable"], 1);
        assert!(result.summary.contains("2 errors"));
    }

    #[test]
    fn test_ruff_extract_clean() {
        let e = RuffExtractor;
        let output = "All checks passed!";
        let result = e.extract(output, 0);
        assert_eq!(result.metrics["errors"], 0);
        assert!(result.summary.contains("passed"));
    }

    // ── GenericExtractor ───────────────────────────────────────────

    #[test]
    fn test_generic_always_matches() {
        let e = GenericExtractor;
        assert!(e.matches("anything"));
    }

    #[test]
    fn test_generic_extract() {
        let e = GenericExtractor;
        let output = "line 1\nline 2\nline 3\n";
        let result = e.extract(output, 0);
        assert_eq!(result.metrics["line_count"], 3);
        assert_eq!(result.metrics["exit_code"], 0);
        assert!(result.summary.contains("3 lines"));
    }

    #[test]
    fn test_generic_extract_error() {
        let e = GenericExtractor;
        let result = e.extract("error output", 1);
        assert!(result.summary.contains("ERROR"));
        assert_eq!(result.metrics["exit_code"], 1);
    }

    // ── Helpers ────────────────────────────────────────────────────

    #[test]
    fn test_extract_leading_number() {
        assert_eq!(extract_leading_number("42 passed"), Some(42));
        assert_eq!(extract_leading_number("no number"), None);
        assert_eq!(extract_leading_number("0 failed"), Some(0));
    }

    #[test]
    fn test_extract_percentage() {
        assert_eq!(extract_percentage("TOTAL 85%"), Some(85.0));
        assert_eq!(extract_percentage("no percentage here"), None);
        assert_eq!(extract_percentage("coverage 92.5%"), Some(92.5));
    }

    // ── Integration: capture_output with specific extractors ───────

    #[test]
    fn test_capture_selects_pytest() {
        let big = "x\n".repeat(600);
        let output = format!("{big}\n5 passed, 0 failed in 1.0s");
        let cap = capture_output("pytest tests/", &output, 0).unwrap();
        assert_eq!(cap.extractor_name, "pytest");
        assert_eq!(cap.metrics["passed"], 5);
    }

    #[test]
    fn test_capture_selects_cargo() {
        let big = "x\n".repeat(600);
        let output = format!("{big}\ntest result: ok. 10 passed; 0 failed; 0 ignored;");
        let cap = capture_output("cargo test -p foo", &output, 0).unwrap();
        assert_eq!(cap.extractor_name, "cargo_test");
        assert_eq!(cap.metrics["passed"], 10);
    }

    #[test]
    fn test_capture_selects_ruff() {
        let big = "x\n".repeat(600);
        let output = format!("{big}\nFound 3 errors (2 fixable with --fix).");
        let cap = capture_output("ruff check .", &output, 1).unwrap();
        assert_eq!(cap.extractor_name, "ruff");
        assert_eq!(cap.metrics["errors"], 3);
    }

    #[test]
    fn test_capture_fallback_generic() {
        let big = "x\n".repeat(600);
        let cap = capture_output("ls -la /tmp", &big, 0).unwrap();
        assert_eq!(cap.extractor_name, "generic");
    }

    #[test]
    fn test_summary_truncated() {
        let big = "x\n".repeat(600);
        let cap = capture_output("some_command", &big, 0).unwrap();
        assert!(
            cap.summary.len() <= MAX_SUMMARY_LEN,
            "Summary {} > max {}",
            cap.summary.len(),
            MAX_SUMMARY_LEN,
        );
    }
}
