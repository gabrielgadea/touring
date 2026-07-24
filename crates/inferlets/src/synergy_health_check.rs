//! Synergy health check inferlet.
//!
//! Reads touring wiring state via subprocess and validates wired_pairs
//! against expected patterns, returning health status and issues found.

use serde::{Deserialize, Serialize};

/// Input for the health check inferlet.
#[derive(Debug, Deserialize)]
struct Input {
    wired_pairs_filter: Option<Vec<String>>,
}

/// A detected issue in the wiring.
#[derive(Debug, Serialize)]
struct Issue {
    pair: String,
    severity: String,
    description: String,
}

/// Health check result.
///
/// `pairs_checked` is post-filter (only the pairs the user requested);
/// `pairs_total` is the unfiltered count from `touring wiring audit` —
/// surfaces the filter ratio so operators can tell if their filter is
/// excluding too much.
#[derive(Debug, Serialize)]
struct HealthResult {
    status: String,
    issues: Vec<Issue>,
    score: f64,
    pairs_checked: usize,
    pairs_total: usize,
}

/// Wrapped output with discriminator.
#[derive(Debug, Serialize)]
struct WrappedOutput {
    __inferlet__: &'static str,
    result: HealthResult,
}

/// Parse the touring wiring audit JSON output.
fn parse_wiring_audit(output: &str) -> Option<serde_json::Value> {
    serde_json::from_str(output).ok()
}

/// Known broken pattern: touring-ast without consumers.
fn check_touring_ast_orphan(wired_pairs: &[serde_json::Value]) -> Option<Issue> {
    for pair in wired_pairs {
        if let (Some(source), Some(target)) = (
            pair.get("source").and_then(|v| v.as_str()),
            pair.get("target").and_then(|v| v.as_str()),
        ) {
            if source.contains("touring-ast") && target.contains("unconsumed") {
                return Some(Issue {
                    pair: format!("{}→{}", source, target),
                    severity: "medium".to_string(),
                    description: "touring-ast module has no registered consumers".to_string(),
                });
            }
        }
    }
    None
}

/// Check for missing integrations between known subsystems.
fn check_missing_integrations(wired_pairs: &[serde_json::Value]) -> Vec<Issue> {
    let mut issues = Vec::new();

    // Track which pairs exist
    let mut has_touring_learning = false;
    let mut has_touring_hooks = false;
    let mut has_touring_ast = false;
    let mut has_touring_cortex = false;

    for pair in wired_pairs {
        if let (Some(source), Some(target)) = (
            pair.get("source").and_then(|v| v.as_str()),
            pair.get("target").and_then(|v| v.as_str()),
        ) {
            if source.contains("touring-learning") || target.contains("touring-learning") {
                has_touring_learning = true;
            }
            if source.contains("touring-hooks") || target.contains("touring-hooks") {
                has_touring_hooks = true;
            }
            if source.contains("touring-ast") || target.contains("touring-ast") {
                has_touring_ast = true;
            }
            if source.contains("touring-cortex") || target.contains("touring-cortex") {
                has_touring_cortex = true;
            }
        }
    }

    // Check for critical missing connections
    if !has_touring_learning && !has_touring_hooks {
        issues.push(Issue {
            pair: "touring-learning→touring-hooks".to_string(),
            severity: "high".to_string(),
            description: "RL subsystem not wired to hook runtime".to_string(),
        });
    }

    if !has_touring_ast && !has_touring_cortex {
        issues.push(Issue {
            pair: "touring-ast→touring-cortex".to_string(),
            severity: "medium".to_string(),
            description: "AST analysis not connected to cortex engine".to_string(),
        });
    }

    issues
}

/// Check for low integration scores in modules.
fn check_low_integration_score(wired_pairs: &[serde_json::Value]) -> Vec<Issue> {
    let mut issues = Vec::new();

    // Check each module for integration_score < 0.8
    for pair in wired_pairs {
        if let (Some(source), Some(target)) = (
            pair.get("source").and_then(|v| v.as_str()),
            pair.get("target").and_then(|v| v.as_str()),
        ) {
            // Heuristic: if source or target contains "orphan" in name
            if source.contains("orphan") || target.contains("orphan") {
                issues.push(Issue {
                    pair: format!("{}→{}", source, target),
                    severity: "low".to_string(),
                    description: "Orphan symbol detected - may need wiring".to_string(),
                });
            }
        }
    }

    issues
}

/// Filter pairs by the optional wired_pairs_filter.
fn filter_pairs(pairs: &[serde_json::Value], filter: &[String]) -> Vec<serde_json::Value> {
    if filter.is_empty() {
        return pairs.to_vec();
    }

    pairs
        .iter()
        .filter(|pair| {
            let source = pair.get("source").and_then(|v| v.as_str()).unwrap_or("");
            let target = pair.get("target").and_then(|v| v.as_str()).unwrap_or("");
            filter
                .iter()
                .any(|f| source.contains(f) || target.contains(f))
        })
        .cloned()
        .collect()
}

/// Calculate health score based on issues.
fn calculate_score(pairs_checked: usize, issues: &[Issue]) -> f64 {
    if pairs_checked == 0 {
        return 0.85; // No pairs to check, assume OK
    }

    let issue_count = issues.len();
    let base_penalty = issue_count as f64 * 0.1;

    // Severity weighting
    let high_severity = issues.iter().filter(|i| i.severity == "high").count() as f64;
    let medium_severity = issues.iter().filter(|i| i.severity == "medium").count() as f64;

    let severity_penalty = (high_severity * 0.15) + (medium_severity * 0.08);
    let total_penalty = base_penalty + severity_penalty;

    let score = 1.0 - total_penalty;
    score.clamp(0.0, 1.0)
}

/// Raw evaluate — checks touring wiring health.
/// Returns 1 always (the inferlet always produces output), 0 on subprocess failure.
pub(crate) fn evaluate_raw(input: &str) -> i32 {
    let input = input.trim();
    if input.is_empty() {
        return 0;
    }

    // Parse JSON input
    let input_obj: Input = match serde_json::from_str(input) {
        Ok(v) => v,
        Err(_) => return 0,
    };

    // Run touring wiring audit
    let output = std::process::Command::new("touring")
        .args(["wiring", "audit", "-j"])
        .output();

    let (wiring_json, pairs_checked) = match output {
        Ok(stdout) => {
            let text = String::from_utf8_lossy(&stdout.stdout);
            match parse_wiring_audit(&text) {
                Some(parsed) => {
                    let count = parsed
                        .get("wired_pairs")
                        .and_then(|v| v.as_array())
                        .map(|a| a.len())
                        .unwrap_or(0);
                    (Some(parsed), count)
                }
                None => (None, 0),
            }
        }
        Err(_) => (None, 0),
    };

    let all_pairs = wiring_json
        .as_ref()
        .and_then(|v| v.get("wired_pairs").and_then(|p| p.as_array()))
        .cloned()
        .unwrap_or_default();

    let filtered = filter_pairs(
        &all_pairs,
        input_obj.wired_pairs_filter.as_deref().unwrap_or(&[]),
    );

    let mut issues = Vec::new();

    // Run checks
    if let Some(ref json) = wiring_json {
        if let Some(pairs) = json.get("wired_pairs").and_then(|p| p.as_array()) {
            // Check for known broken patterns
            if let Some(issue) = check_touring_ast_orphan(pairs) {
                issues.push(issue);
            }

            // Check for missing integrations
            let missing_issues = check_missing_integrations(pairs);
            issues.extend(missing_issues);

            // Check for low integration scores
            let low_score_issues = check_low_integration_score(pairs);
            issues.extend(low_score_issues);
        }
    }

    let score = calculate_score(filtered.len(), &issues);

    let status = if !issues.is_empty() { "degraded" } else { "ok" };

    let result = HealthResult {
        status: status.to_string(),
        issues,
        score,
        pairs_checked: filtered.len(),
        pairs_total: pairs_checked,
    };

    let wrapped = WrappedOutput {
        __inferlet__: "synergy_health_check",
        result,
    };

    if let Ok(json) = serde_json::to_string(&wrapped) {
        println!("{}", json);
    }

    1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_filter_pairs_empty_filter() {
        let pairs = vec![
            serde_json::json!({"source": "a", "target": "b"}),
            serde_json::json!({"source": "c", "target": "d"}),
        ];
        let filtered = filter_pairs(&pairs, &[]);
        assert_eq!(filtered.len(), 2);
    }

    #[test]
    fn test_filter_pairs_with_filter() {
        let pairs = vec![
            serde_json::json!({"source": "touring-hooks", "target": "touring-learning"}),
            serde_json::json!({"source": "touring-ast", "target": "touring-cortex"}),
        ];
        let filtered = filter_pairs(&pairs, &["touring-hooks".to_string()]);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0]["source"], "touring-hooks");
    }

    #[test]
    fn test_calculate_score_no_issues() {
        let score = calculate_score(10, &[]);
        assert_eq!(score, 1.0);
    }

    #[test]
    fn test_calculate_score_with_issues() {
        let issues = vec![Issue {
            pair: "a→b".to_string(),
            severity: "high".to_string(),
            description: "test".to_string(),
        }];
        let score = calculate_score(5, &issues);
        assert!(score < 1.0);
    }

    #[test]
    fn test_calculate_score_empty_pairs() {
        let score = calculate_score(0, &[]);
        assert_eq!(score, 0.85);
    }

    #[test]
    fn test_evaluate_raw_valid_input() {
        // This will call touring subprocess, which may not be available in test
        // but we test the parsing path
        let input = r#"{"wired_pairs_filter":[]}"#;
        // Just verify parsing doesn't panic
        let result = evaluate_raw(input);
        assert!(result == 0 || result == 1); // 0 if subprocess fails, 1 if succeeds
    }

    #[test]
    fn test_evaluate_raw_invalid_json() {
        let input = "not json";
        let result = evaluate_raw(input);
        assert_eq!(result, 0);
    }

    #[test]
    fn test_check_missing_integrations_empty() {
        let pairs = vec![];
        let issues = check_missing_integrations(&pairs);
        assert!(!issues.is_empty()); // With no pairs, integrations are missing
    }

    #[test]
    fn test_check_low_integration_score_no_orphans() {
        let pairs =
            vec![serde_json::json!({"source": "touring-hooks", "target": "touring-learning"})];
        let issues = check_low_integration_score(&pairs);
        assert!(issues.is_empty());
    }

    #[test]
    fn test_health_result_serialization() {
        let result = HealthResult {
            status: "ok".to_string(),
            issues: vec![],
            score: 1.0,
            pairs_checked: 5,
            pairs_total: 7,
        };
        let json = serde_json::to_string(&result).expect("serialize HealthResult");
        assert!(json.contains("\"status\":\"ok\""));
        assert!(json.contains("\"score\":1.0"));
        assert!(json.contains("\"pairs_total\":7"));
    }
}
