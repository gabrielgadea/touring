//! Risk Scoring — Criticality scoring and unified change detection.
//!
//! Combines blast radius, wiring scores, gotcha matches, and test coverage
//! into a single risk assessment. Inspired by code-review-graph's
//! `detect_changes` tool.

use serde::Serialize;
use serde_json::json;

use crate::server::params::DetailLevel;

// ── Blast radius input ─────────────────────────────────────────────────

/// The direct importers of the `changed` files, from the loaded dependency index: each one
/// once, in first-seen order, none of them a changed file itself.
///
/// `SymbolIndex::importers_of` answers from the wiring's resolved file edges (a file-key
/// bucket) and credits a bare module stem only to the ONE indexed file that owns it. The
/// SQL it replaces (`SymbolStore::get_reverse_deps`) matched the stem for every file, so
/// `a/utils.py` and `b/utils.py` shared all their importers.
pub(crate) fn direct_importers_of(
    index: &touring_code::ast::graph::SymbolIndex,
    changed: &[String],
) -> Vec<String> {
    let mut affected: Vec<String> = Vec::new();
    for file in changed {
        for importer in index.importers_of(file) {
            if !affected.contains(importer) && !changed.contains(importer) {
                affected.push(importer.clone());
            }
        }
    }
    affected
}

/// How much of the change the dependency index could actually LOOK AT: the `coverage` field of
/// the `detect_changes` payload.
///
/// `affected_files` is the answer of [`direct_importers_of`], and an empty answer has two
/// causes the list cannot tell apart: no resolved import points at the file (`Isolated`; F13: a name
/// guess is not an importer), or the wiring never
/// looked at the file (`Uncovered`: a new file, a file outside the indexed tree, a project whose
/// wiring was not built). Reading the second as the first understates the blast radius and the
/// risk derived from it, so the payload says how many changed files the index covers and names
/// the ones it does not. It never changes `affected_files` or `blast_penalty`: for an uncovered
/// file the true count is unknown, and the honest move is to say so beside the number, not to
/// invent a different one.
///
/// `{changed_files, covered, uncovered, uncovered_files}`; `uncovered_files` lists at most
/// [`COVERAGE_LISTED_FILES`] names and `caveat` (present only when `uncovered > 0`) says what
/// to do about it.
pub(crate) fn importer_coverage(
    index: &touring_code::ast::graph::SymbolIndex,
    changed: &[String],
) -> serde_json::Value {
    use touring_code::ast::graph::Importers;
    let uncovered_files: Vec<&String> = changed
        .iter()
        .filter(|file| matches!(index.importers_status(file), Importers::Uncovered))
        .collect();
    let mut coverage = json!({
        "changed_files": changed.len(),
        "covered": changed.len() - uncovered_files.len(),
        "uncovered": uncovered_files.len(),
        "uncovered_files": uncovered_files
            .iter()
            .take(COVERAGE_LISTED_FILES)
            .collect::<Vec<_>>(),
    });
    if !uncovered_files.is_empty() {
        coverage["caveat"] = json!(
            "the dependency index holds no wiring for the `uncovered_files`: an empty list of \
             importers there means 'not looked at', not 'isolated', so `affected_files` and \
             the blast penalty are a lower bound for them. Run `touring index rebuild` (or \
             check the path spelling: keys are project-relative) and ask again."
        );
    }
    coverage
}

/// The most `uncovered_files` names [`importer_coverage`] lists (the count is always exact).
const COVERAGE_LISTED_FILES: usize = 20;

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

    /// C8 before/after: two modules with the same stem, each imported by its own consumer.
    /// The SQL stem match (`get_reverse_deps`) gave BOTH consumers to either file
    /// (affected 2, blast_ratio 2, blast_penalty 0.2 for one changed file); the loaded
    /// index answers with the one that imports it (affected 1, penalty 0.1).
    #[test]
    fn detect_changes_counts_only_the_importers_of_the_changed_homonym() {
        use touring_code::ast::graph::SymbolIndex;
        let s = |text: &str| text.to_string();
        let mut index = SymbolIndex::new();
        index.load_file_edges(
            [
                (s("app_a.py"), s("a/utils.py")),
                (s("app_b.py"), s("b/utils.py")),
            ],
            [
                s("a/utils.py"),
                s("b/utils.py"),
                s("app_a.py"),
                s("app_b.py"),
            ],
        );

        let changed = vec![s("a/utils.py")];
        let affected = direct_importers_of(&index, &changed);
        assert_eq!(
            affected,
            ["app_a.py"],
            "only the consumer wired to a/utils.py"
        );

        let report = build_change_risk_report(ChangeRiskInput {
            changed_files: changed,
            affected_symbols: affected.len() as u32,
            affected_files: affected,
            test_gaps: vec![],
            hotspots: vec![],
            gotcha_warnings: vec![],
            wiring_score: 1.0,
            detail_level: DetailLevel::Standard,
        });
        assert_eq!(report["affected_files"].as_array().map(Vec::len), Some(1));
    }

    #[test]
    fn direct_importers_are_listed_once_and_never_include_a_changed_file() {
        use touring_code::ast::graph::SymbolIndex;
        let s = |text: &str| text.to_string();
        let mut index = SymbolIndex::new();
        index.load_file_edges(
            [
                (s("main.rs"), s("a.rs")),
                (s("main.rs"), s("b.rs")),
                (s("b.rs"), s("a.rs")),
            ],
            [s("a.rs"), s("b.rs"), s("main.rs")],
        );
        // `b.rs` imports `a.rs` but is itself part of the change: it is not "affected".
        let affected = direct_importers_of(&index, &[s("a.rs"), s("b.rs")]);
        assert_eq!(affected, ["main.rs"], "main.rs once, b.rs not at all");
    }

    /// An empty `affected_files` has two causes the list cannot tell apart; `coverage` can. `solo.py`
    /// was looked at and nobody imports it (isolated); `fresh.py` was never looked at (uncovered).
    /// Both give 0 affected files — only the second is "unknown", and the payload says which.
    #[test]
    fn detect_changes_coverage_separates_isolated_from_never_looked_at() {
        use touring_code::ast::graph::SymbolIndex;
        let s = |text: &str| text.to_string();
        let mut index = SymbolIndex::new();
        index.load_file_edges(
            [(s("app.py"), s("lib.py"))],
            [s("app.py"), s("lib.py"), s("solo.py")],
        );
        let changed = vec![s("lib.py"), s("solo.py"), s("fresh.py")];

        // The number is unchanged by the coverage: lib.py's one importer, nothing for the rest.
        assert_eq!(direct_importers_of(&index, &changed), ["app.py"]);
        assert!(direct_importers_of(&index, &[s("solo.py")]).is_empty());
        assert!(direct_importers_of(&index, &[s("fresh.py")]).is_empty());

        let coverage = importer_coverage(&index, &changed);
        assert_eq!(coverage["changed_files"], 3, "{coverage}");
        assert_eq!(coverage["covered"], 2, "lib.py and solo.py: {coverage}");
        assert_eq!(coverage["uncovered"], 1, "{coverage}");
        assert_eq!(coverage["uncovered_files"], serde_json::json!(["fresh.py"]));
        assert!(
            coverage["caveat"]
                .as_str()
                .is_some_and(|c| c.contains("not looked at") && c.contains("index rebuild")),
            "the caveat says what 0 means and what to do: {coverage}"
        );

        // The blast penalty is derived from the same list as before: the coverage moved nothing.
        let fresh_only = vec![s("fresh.py")];
        let report = build_change_risk_report(ChangeRiskInput {
            changed_files: fresh_only.clone(),
            affected_files: direct_importers_of(&index, &fresh_only),
            affected_symbols: 0,
            test_gaps: vec![],
            hotspots: vec![],
            gotcha_warnings: vec![],
            wiring_score: 1.0,
            detail_level: DetailLevel::Standard,
        });
        assert_eq!(report["affected_files"].as_array().map(Vec::len), Some(0));
    }

    #[test]
    fn detect_changes_coverage_is_silent_when_every_changed_file_was_looked_at() {
        use touring_code::ast::graph::SymbolIndex;
        let s = |text: &str| text.to_string();
        let mut index = SymbolIndex::new();
        index.load_file_edges([(s("app.py"), s("lib.py"))], [s("app.py"), s("lib.py")]);
        let coverage = importer_coverage(&index, &[s("lib.py"), s("app.py")]);
        assert_eq!(coverage["covered"], 2, "{coverage}");
        assert_eq!(coverage["uncovered"], 0, "{coverage}");
        assert_eq!(coverage["uncovered_files"], serde_json::json!([]));
        assert!(
            coverage.get("caveat").is_none(),
            "nothing to caveat when everything was looked at: {coverage}"
        );
    }

    #[test]
    fn detect_changes_coverage_counts_every_uncovered_file_but_lists_a_bounded_few() {
        use touring_code::ast::graph::SymbolIndex;
        let s = |text: &str| text.to_string();
        let mut index = SymbolIndex::new();
        index.load_file_edges([(s("app.py"), s("lib.py"))], [s("app.py"), s("lib.py")]);
        let changed: Vec<String> = (0..COVERAGE_LISTED_FILES + 5)
            .map(|n| format!("new_{n}.py"))
            .collect();
        let coverage = importer_coverage(&index, &changed);
        assert_eq!(
            coverage["uncovered"],
            COVERAGE_LISTED_FILES + 5,
            "{coverage}"
        );
        assert_eq!(
            coverage["uncovered_files"].as_array().map(Vec::len),
            Some(COVERAGE_LISTED_FILES),
            "the count is exact, the list is bounded: {coverage}"
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
