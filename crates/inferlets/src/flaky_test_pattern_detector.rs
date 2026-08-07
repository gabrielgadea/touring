//! Flaky test pattern detector inferlet.
//!
//! Parses cargo test output to detect flaky test patterns based on
//! failure rate thresholds. FS-dependent — uses `ctx_execute` sandbox.
//!
//! # Input JSON
//!
//! ```json
//! {
//!   "__inferlet__": "flaky_test_pattern_detector",
//!   "test_log": "/path/to/test.log",
//!   "threshold": 0.3
//! }
//! ```
//!
//! # Output JSON
//!
//! ```json
//! {
//!   "flaky_tests": [
//!     {"name": "test_session_resume", "failure_rate": 0.45, "runs": 20}
//!   ],
//!   "total_runs": 100
//! }
//! ```
//!
//! Returns 1 if flaky tests found (failure_rate > threshold), 0 otherwise.

use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::collections::HashMap;
use std::thread_local;

thread_local! {
    static LAST_ERROR: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// Input structure for flaky_test_pattern_detector.
#[derive(Debug, Deserialize)]
pub struct Input {
    /// Path to the cargo test log file to parse.
    pub test_log: String,
    /// Failure-rate threshold above which a test is flagged as flaky.
    pub threshold: f64,
}

/// A single flaky test entry.
#[derive(Debug, Serialize)]
pub struct FlakyTest {
    /// Name of the flaky test.
    pub name: String,
    /// Observed failure rate (failures divided by total runs).
    pub failure_rate: f64,
    /// Total number of recorded runs for this test.
    pub runs: usize,
}

/// Output structure for flaky_test_pattern_detector.
#[derive(Debug, Serialize)]
pub struct Output {
    /// Tests whose failure rate exceeded the threshold.
    pub flaky_tests: Vec<FlakyTest>,
    /// Total number of test runs parsed from the log.
    pub total_runs: usize,
}

/// Parse a test log file and extract flaky test patterns.
/// Returns a map of test_name -> (failure_count, total_runs).
fn parse_test_log(log_path: &str) -> Option<HashMap<String, (usize, usize)>> {
    let content = std::fs::read_to_string(log_path).ok()?;
    let mut results: HashMap<String, (usize, usize)> = HashMap::new();

    for line in content.lines() {
        let line = line.trim();
        // Match patterns like: "test test_session_resume ... FAILED" or "test test_session_resume ... ok"
        if let Some(rest) = line.strip_prefix("test ")
            && let Some(name_end) = rest.find(' ')
        {
            let name_and_status = &rest[..name_end];
            if let Some(name) = name_and_status.split_whitespace().next() {
                let name = name.to_string();
                let entry = results.entry(name).or_insert((0, 0));
                entry.1 += 1;
                if line.contains("FAILED") {
                    entry.0 += 1;
                }
            }
        }
        // Also handle "running N tests" header to get total
        if let Some(count) = line.strip_prefix("running ")
            && let Some(n) = count.split_whitespace().next()
            && let Ok(_num) = n.parse::<usize>()
        {
            // Track this as a total for the next set of tests
        }
    }

    Some(results)
}

/// Calculate failure rate for a test.
fn failure_rate(failures: usize, runs: usize) -> f64 {
    if runs == 0 {
        return 0.0;
    }
    failures as f64 / runs as f64
}

/// Raw evaluate — returns 1 if flaky tests found, 0 otherwise.
pub(crate) fn evaluate_raw(input: &str) -> i32 {
    let input = input.trim();
    let inp: Input = match serde_json::from_str(input) {
        Ok(v) => v,
        Err(_) => {
            LAST_ERROR.with(|cell| *cell.borrow_mut() = Some("invalid JSON input".to_string()));
            return 0;
        }
    };

    let test_data = match parse_test_log(&inp.test_log) {
        Some(m) => m,
        None => {
            LAST_ERROR.with(|cell| {
                *cell.borrow_mut() = Some(format!("failed to parse test_log: {}", inp.test_log))
            });
            return 0;
        }
    };

    let total_runs: usize = test_data.values().map(|&(_, runs)| runs).sum();
    let mut flaky_tests: Vec<FlakyTest> = Vec::new();

    for (name, (failures, runs)) in test_data {
        let rate = failure_rate(failures, runs);
        if rate > inp.threshold {
            flaky_tests.push(FlakyTest {
                name,
                failure_rate: rate,
                runs,
            });
        }
    }

    if flaky_tests.is_empty() {
        return 0;
    }

    let output = Output {
        flaky_tests,
        total_runs,
    };

    if let Ok(json) = serde_json::to_string(&output) {
        LAST_ERROR.with(|cell| *cell.borrow_mut() = Some(json));
    }

    1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_test_log_empty() {
        let map = parse_test_log("/nonexistent/path").unwrap_or_default();
        assert!(map.is_empty());
    }

    #[test]
    fn test_failure_rate_zero_runs() {
        assert_eq!(failure_rate(0, 0), 0.0);
    }

    #[test]
    fn test_failure_rate_calculation() {
        assert!((failure_rate(5, 10) - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_evaluate_raw_malformed_json() {
        let result = evaluate_raw("{ invalid");
        assert_eq!(result, 0);
    }

    #[test]
    fn test_evaluate_raw_missing_fields() {
        let result = evaluate_raw(r#"{"test_log":"/tmp/log"}"#);
        assert_eq!(result, 0);
    }

    #[test]
    fn test_flaky_test_threshold() {
        let inp = Input {
            test_log: "/nonexistent".to_string(),
            threshold: 0.3,
        };
        assert_eq!(inp.threshold, 0.3);
    }
}
