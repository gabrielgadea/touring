//! Risk Scoring — Criticality scoring and unified change detection.
//!
//! Combines blast radius, wiring scores, gotcha matches, and test coverage
//! into a single risk assessment. Inspired by code-review-graph's
//! `detect_changes` tool.

use serde::Serialize;
use serde_json::json;

use crate::server::params::DetailLevel;

// ── Criticality Scoring ────────────────────────────────────────────────

/// Criticality score for a single symbol or file.
#[derive(Debug, Clone, Serialize)]
pub struct CriticalityScore {
    /// Composite score 0.0-1.0 (higher = more critical)
    pub score: f64,
    /// Number of consumers (fan-in)
    pub fan_in: u32,
    /// Number of dependencies (fan-out)
    pub fan_out: u32,
    /// Edit frequency percentile (0.0-1.0)
    pub churn_rank: f64,
    /// Whether the symbol has test coverage
    pub has_tests: bool,
    /// Number of matching gotchas
    pub gotcha_count: u32,
    /// Human-readable factors that contributed to the score
    pub factors: Vec<String>,
}

/// Weights for criticality score components.
const W_FAN_IN: f64 = 0.30;
const W_FAN_OUT: f64 = 0.20;
const W_CHURN: f64 = 0.20;
const W_TEST_GAP: f64 = 0.20;
const W_GOTCHA: f64 = 0.10;

/// Compute criticality score for a symbol/file.
pub fn compute_criticality(
    fan_in: u32,
    fan_out: u32,
    churn_rank: f64,
    has_tests: bool,
    gotcha_count: u32,
) -> CriticalityScore {
    let mut factors = Vec::new();

    // Normalize fan-in: log scale, cap at 1.0
    let fan_in_score = (fan_in as f64 + 1.0).ln() / 10.0_f64.ln();
    let fan_in_norm = fan_in_score.min(1.0);
    if fan_in > 5 {
        factors.push(format!("high fan-in: {} consumers", fan_in));
    }

    // Normalize fan-out: log scale, cap at 1.0
    let fan_out_score = (fan_out as f64 + 1.0).ln() / 10.0_f64.ln();
    let fan_out_norm = fan_out_score.min(1.0);
    if fan_out > 10 {
        factors.push(format!("high fan-out: {} dependencies", fan_out));
    }

    // Churn is already 0.0-1.0
    let churn_norm = churn_rank.clamp(0.0, 1.0);
    if churn_rank > 0.7 {
        factors.push(format!(
            "frequently edited: top {}%",
            ((1.0 - churn_rank) * 100.0) as u32
        ));
    }

    // Test gap: 1.0 if no tests, 0.0 if has tests
    let test_gap = if has_tests { 0.0 } else { 1.0 };
    if !has_tests {
        factors.push("no test coverage".to_string());
    }

    // Gotcha: normalize log scale
    let gotcha_norm = if gotcha_count > 0 {
        factors.push(format!("{} known pitfalls", gotcha_count));
        ((gotcha_count as f64 + 1.0).ln() / 5.0_f64.ln()).min(1.0)
    } else {
        0.0
    };

    let score = (W_FAN_IN * fan_in_norm
        + W_FAN_OUT * fan_out_norm
        + W_CHURN * churn_norm
        + W_TEST_GAP * test_gap
        + W_GOTCHA * gotcha_norm)
        .clamp(0.0, 1.0);

    CriticalityScore {
        score,
        fan_in,
        fan_out,
        churn_rank,
        has_tests,
        gotcha_count,
        factors,
    }
}

// ── Change Risk Report ─────────────────────────────────────────────────

/// A test coverage gap — a function affected by changes without tests.
#[derive(Debug, Clone, Serialize)]
pub struct TestGap {
    /// Symbol qualified name
    pub symbol: String,
    /// File path
    pub file_path: String,
    /// Criticality score
    pub criticality: f64,
}

/// A hotspot — a high-criticality symbol affected by changes.
#[derive(Debug, Clone, Serialize)]
pub struct Hotspot {
    /// Symbol qualified name
    pub symbol: String,
    /// File path
    pub file_path: String,
    /// Criticality score
    pub criticality: f64,
    /// Contributing factors
    pub factors: Vec<String>,
}

/// Unified change risk report combining multiple subsystems.
#[derive(Debug, Clone, Serialize)]
pub struct ChangeRiskReport {
    /// Overall risk score (0.0-1.0)
    pub overall_risk: f64,
    /// Risk level label
    pub risk_level: String,
    /// Number of changed files
    pub changed_files: usize,
    /// Files affected by blast radius
    pub affected_files: Vec<String>,
    /// Total affected symbols
    pub affected_symbols: u32,
    /// Functions affected without test coverage
    pub test_gaps: Vec<TestGap>,
    /// High-criticality symbols affected
    pub hotspots: Vec<Hotspot>,
    /// Gotcha warnings for changed files
    pub gotcha_warnings: Vec<String>,
    /// Recommended actions
    pub suggested_actions: Vec<String>,
}

/// Input bundle for [`build_change_risk_report`] — groups parameters to keep
/// the function signature within the 7-argument Clippy limit.
pub struct ChangeRiskInput {
    /// Files directly modified by the change.
    pub changed_files: Vec<String>,
    /// Files transitively impacted by the change.
    pub affected_files: Vec<String>,
    /// Count of symbols touched by the change.
    pub affected_symbols: u32,
    /// Detected gaps in test coverage for the change.
    pub test_gaps: Vec<TestGap>,
    /// Code hotspots intersecting the change.
    pub hotspots: Vec<Hotspot>,
    /// Gotcha warnings relevant to the change.
    pub gotcha_warnings: Vec<String>,
    /// Wiring integration score for the affected scope (0.0-1.0).
    pub wiring_score: f64,
    /// Verbosity level controlling report detail.
    pub detail_level: DetailLevel,
}

/// Build a change risk report from subsystem data.
///
/// This is the composition function that unifies:
/// - Blast radius (affected files/symbols)
/// - Wiring scores (integration health)
/// - Gotcha matches (known pitfalls)
/// - Test coverage gaps
/// - Criticality scores for affected hotspots
pub fn build_change_risk_report(input: ChangeRiskInput) -> serde_json::Value {
    let ChangeRiskInput {
        changed_files,
        affected_files,
        affected_symbols,
        test_gaps,
        hotspots,
        gotcha_warnings,
        wiring_score,
        detail_level,
    } = input;
    let changed_files = changed_files.as_slice();
    // Compute overall risk
    let blast_ratio = if changed_files.is_empty() {
        0.0
    } else {
        (affected_files.len() as f64) / (changed_files.len() as f64)
    };
    let blast_penalty = (blast_ratio / 10.0).min(0.3);
    let test_gap_penalty = (test_gaps.len() as f64 / 10.0).min(0.3);
    let hotspot_penalty = (hotspots.len() as f64 / 5.0).min(0.2);
    let gotcha_penalty = (gotcha_warnings.len() as f64 / 10.0).min(0.1);
    let wiring_penalty = (1.0 - wiring_score) * 0.1;

    let overall_risk =
        (blast_penalty + test_gap_penalty + hotspot_penalty + gotcha_penalty + wiring_penalty)
            .clamp(0.0, 1.0);

    let risk_level = match overall_risk {
        r if r < 0.2 => "low",
        r if r < 0.5 => "medium",
        r if r < 0.8 => "high",
        _ => "critical",
    };

    // Build structured review guidance (inspired by CRG's _generate_review_guidance)
    let mut actions = Vec::new();
    if !test_gaps.is_empty() {
        let names: Vec<&str> = test_gaps
            .iter()
            .take(5)
            .map(|g| g.symbol.as_str())
            .collect();
        actions.push(format!(
            "{} changed function(s) lack test coverage: {}",
            test_gaps.len(),
            names.join(", ")
        ));
    }
    if !hotspots.is_empty() {
        let names: Vec<&str> = hotspots.iter().take(3).map(|h| h.symbol.as_str()).collect();
        actions.push(format!(
            "{} high-criticality symbol(s) affected: {}. Review callers carefully.",
            hotspots.len(),
            names.join(", ")
        ));
    }
    if !gotcha_warnings.is_empty() {
        actions.push(format!(
            "{} known pitfall(s) in changed files — address before merging",
            gotcha_warnings.len()
        ));
    }
    if blast_ratio > 5.0 {
        actions.push(format!(
            "Wide blast radius: {}x amplification ({} affected from {} changed). Consider splitting.",
            blast_ratio as u32, affected_files.len(), changed_files.len()
        ));
    }
    if wiring_score < 0.5 {
        actions.push(format!(
            "Low wiring score ({:.0}%) — many orphan pub symbols. Run touring_wiring_audit.",
            wiring_score * 100.0
        ));
    }

    let max_items = detail_level.max_items();

    let report = ChangeRiskReport {
        overall_risk,
        risk_level: risk_level.to_string(),
        changed_files: changed_files.len(),
        affected_files: affected_files.into_iter().take(max_items).collect(),
        affected_symbols,
        test_gaps: test_gaps.into_iter().take(max_items).collect(),
        hotspots: hotspots.into_iter().take(max_items).collect(),
        gotcha_warnings: gotcha_warnings.into_iter().take(max_items).collect(),
        suggested_actions: actions,
    };

    let mut value =
        serde_json::to_value(&report).unwrap_or_else(|_| json!({"error": "serialization failed"}));

    // Add estimated token cost
    let estimated_tokens = serde_json::to_string(&value)
        .map(|s| s.len() / 4)
        .unwrap_or(0);
    if let Some(obj) = value.as_object_mut() {
        obj.insert("_estimated_tokens".to_string(), json!(estimated_tokens));
    }

    value
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_criticality_zero() {
        let score = compute_criticality(0, 0, 0.0, true, 0);
        assert!(
            score.score < 0.1,
            "Zero criticality should be low: {}",
            score.score
        );
        assert!(score.factors.is_empty());
    }

    #[test]
    fn test_criticality_high_fan_in() {
        let score = compute_criticality(50, 0, 0.0, true, 0);
        assert!(
            score.score > 0.2,
            "High fan-in should raise criticality: {}",
            score.score
        );
        assert!(score.factors.iter().any(|f| f.contains("fan-in")));
    }

    #[test]
    fn test_criticality_no_tests() {
        let score = compute_criticality(0, 0, 0.0, false, 0);
        assert!(
            score.score > 0.15,
            "No tests should raise criticality: {}",
            score.score
        );
        assert!(score.factors.iter().any(|f| f.contains("no test")));
    }

    #[test]
    fn test_criticality_all_factors() {
        let score = compute_criticality(20, 15, 0.9, false, 3);
        assert!(
            score.score > 0.5,
            "All factors should produce high score: {}",
            score.score
        );
        assert!(
            score.factors.len() >= 3,
            "Should have multiple factors: {:?}",
            score.factors
        );
    }

    #[test]
    fn test_criticality_clamped() {
        let score = compute_criticality(1000, 1000, 1.0, false, 100);
        assert!(
            score.score <= 1.0,
            "Score should be clamped to 1.0: {}",
            score.score
        );
    }

    #[test]
    fn test_change_risk_report_low() {
        let result = build_change_risk_report(ChangeRiskInput {
            changed_files: vec!["file1.rs".to_string()],
            affected_files: vec!["file1.rs".to_string(), "file2.rs".to_string()],
            affected_symbols: 5,
            test_gaps: vec![],
            hotspots: vec![],
            gotcha_warnings: vec![],
            wiring_score: 0.9,
            detail_level: DetailLevel::Standard,
        });
        let risk = result
            .get("overall_risk")
            .and_then(|v| v.as_f64())
            .unwrap_or(1.0);
        assert!(risk < 0.3, "Low risk report: {}", risk);
    }

    #[test]
    fn test_change_risk_report_high() {
        let result = build_change_risk_report(ChangeRiskInput {
            changed_files: vec!["file1.rs".to_string()],
            affected_files: (0..20).map(|i| format!("affected_{}.rs", i)).collect(),
            affected_symbols: 50,
            test_gaps: (0..5)
                .map(|i| TestGap {
                    symbol: format!("func_{}", i),
                    file_path: format!("file_{}.rs", i),
                    criticality: 0.8,
                })
                .collect(),
            hotspots: vec![Hotspot {
                symbol: "critical_fn".to_string(),
                file_path: "core.rs".to_string(),
                criticality: 0.95,
                factors: vec!["high fan-in".to_string()],
            }],
            gotcha_warnings: vec!["known issue with X".to_string()],
            wiring_score: 0.3,
            detail_level: DetailLevel::Standard,
        });
        let risk = result
            .get("overall_risk")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        assert!(risk > 0.5, "High risk report: {}", risk);
        let actions = result.get("suggested_actions").and_then(|v| v.as_array());
        assert!(
            actions.is_some() && !actions.expect("checked").is_empty(),
            "Should have actions"
        );
    }

    #[test]
    fn test_change_risk_report_minimal_detail() {
        let result = build_change_risk_report(ChangeRiskInput {
            changed_files: vec!["f.rs".to_string()],
            affected_files: (0..20).map(|i| format!("a_{}.rs", i)).collect(),
            affected_symbols: 20,
            test_gaps: vec![],
            hotspots: vec![],
            gotcha_warnings: vec![],
            wiring_score: 0.5,
            detail_level: DetailLevel::Minimal,
        });
        let affected = result.get("affected_files").and_then(|v| v.as_array());
        if let Some(arr) = affected {
            assert!(
                arr.len() <= 3,
                "Minimal should limit to 3 affected files, got {}",
                arr.len()
            );
        }
    }

    #[test]
    fn test_change_risk_empty_changes() {
        let result = build_change_risk_report(ChangeRiskInput {
            changed_files: vec![],
            affected_files: vec![],
            affected_symbols: 0,
            test_gaps: vec![],
            hotspots: vec![],
            gotcha_warnings: vec![],
            wiring_score: 1.0,
            detail_level: DetailLevel::Full,
        });
        let risk = result
            .get("overall_risk")
            .and_then(|v| v.as_f64())
            .unwrap_or(1.0);
        assert!(risk < 0.1, "Empty changes should be low risk: {}", risk);
    }
}
