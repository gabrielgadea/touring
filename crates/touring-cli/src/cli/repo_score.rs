//! `cli_repo_score` — Wave R1: aggregate executive KPI dashboard.
//!
//! Computes a 0-289 composite repository score across 11 orthogonal categories,
//! emitting an A+..F letter grade and structured `Diagnostic` (RFC-100 / Wave Q4)
//! for sub-thresholds. Categories delegate to existing handlers when possible;
//! external categories (cargo-deny, nextest, coverage) currently report a
//! conservative placeholder score with `source: "stub"` so consumers can
//! distinguish derived from observed metrics.
//!
//! See `~/.claude/rust/docs/2026-04-24-waves-Q-R-M-A-T-P-plan.md` §R1 for the
//! original spec and the 11-category table.
//!
//! # Output (`-j` mode)
//!
//! ```json
//! {
//!   "total_score": 247,
//!   "max_score": 289,
//!   "percentage": 85.5,
//!   "grade": "B+",
//!   "categories": {
//!     "architecture": { "score": 27, "max": 30, "source": "wiring", "details": {...} },
//!     ...
//!   },
//!   "diagnostics": [
//!     { "code": "W-100", "severity": "warning", "message": "..." }
//!   ]
//! }
//! ```

use crate::runtime::HookRuntime;
use serde_json::{Value, json};
use touring_foundation::diagnostic::{Diagnostic, Severity, codes};

// ─────────────────────────────────────────────────────────────────────────────
// Score table (RFC-R1 §1) — 11 orthogonal categories totalling 289 points.
// ─────────────────────────────────────────────────────────────────────────────

const MAX_ARCHITECTURE: u32 = 30;
const MAX_TESTING: u32 = 30;
const MAX_DOCUMENTATION: u32 = 20;
const MAX_SECURITY: u32 = 30;
const MAX_PERFORMANCE: u32 = 20;
const MAX_MAINTAINABILITY: u32 = 30;
const MAX_OBSERVABILITY: u32 = 20;
const MAX_SUPPLY_CHAIN: u32 = 20;
const MAX_DEPENDENCIES: u32 = 20;
const MAX_GOTCHAS: u32 = 20;
const MAX_RL_CONVERGENCE: u32 = 29;

/// Total points across all 11 categories.
///
/// Note: the original RFC-R1 §1 table claims a max of 289, but the listed
/// per-category points sum to 269 (30+30+20+30+20+30+20+20+20+20+29 = 269).
/// The "289" target is preserved as a known doc bug; this implementation
/// uses the mathematically correct sum of the per-category constants.
pub const MAX_REPO_SCORE: u32 = MAX_ARCHITECTURE
    + MAX_TESTING
    + MAX_DOCUMENTATION
    + MAX_SECURITY
    + MAX_PERFORMANCE
    + MAX_MAINTAINABILITY
    + MAX_OBSERVABILITY
    + MAX_SUPPLY_CHAIN
    + MAX_DEPENDENCIES
    + MAX_GOTCHAS
    + MAX_RL_CONVERGENCE;

const _: () = assert!(
    MAX_REPO_SCORE == 269,
    "per-category constants sum to 269 (RFC-R1 §1 doc bug claims 289)"
);

// ─────────────────────────────────────────────────────────────────────────────
// Grade letters — same envelope as Wave Q1 (TDG) for consistency.
// ─────────────────────────────────────────────────────────────────────────────

/// Letter grade derived from the percentage of `MAX_REPO_SCORE`.
///
/// Mirrors `touring_analysis::quality::tdg::TdgGrade` thresholds so a single
/// rubric covers per-file (TDG) and per-repo (R1) scoring.
#[must_use]
pub fn grade_letter(percentage: f64) -> &'static str {
    match percentage {
        p if p >= 95.0 => "A+",
        p if p >= 90.0 => "A",
        p if p >= 85.0 => "B+",
        p if p >= 80.0 => "B",
        p if p >= 75.0 => "C+",
        p if p >= 70.0 => "C",
        p if p >= 60.0 => "D",
        _ => "F",
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Public handler
// ─────────────────────────────────────────────────────────────────────────────

/// `cli-repo-score` handler — produces the 11-category aggregate dashboard.
///
/// `payload` accepts an optional `"category"` field to narrow the response to
/// a single category (useful for piping into `jq`).
pub fn cli_repo_score(rt: &mut HookRuntime, payload: &Value) -> String {
    let filter = payload
        .get("category")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let categories = compute_all_categories(rt);
    let total: u32 = categories.iter().map(|c| c.score).sum();
    let pct = (f64::from(total) / f64::from(MAX_REPO_SCORE)) * 100.0;
    let grade = grade_letter(pct);

    let diagnostics = build_diagnostics(&categories);

    let cat_obj: serde_json::Map<String, Value> = categories
        .into_iter()
        .filter(|c| filter.is_empty() || c.name == filter)
        .map(|c| (c.name.to_string(), c.into_json()))
        .collect();

    json!({
        "total_score": total,
        "max_score": MAX_REPO_SCORE,
        "percentage": (pct * 10.0).round() / 10.0,
        "grade": grade,
        "categories": cat_obj,
        "diagnostics": diagnostics,
        "diagnostic_count": diagnostics.len(),
    })
    .to_string()
}

// ─────────────────────────────────────────────────────────────────────────────
// Per-category structures
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct CategoryReport {
    name: &'static str,
    score: u32,
    max: u32,
    source: &'static str,
    details: Value,
}

impl CategoryReport {
    fn into_json(self) -> Value {
        json!({
            "score": self.score,
            "max": self.max,
            "source": self.source,
            "details": self.details,
        })
    }
}

fn compute_all_categories(rt: &mut HookRuntime) -> Vec<CategoryReport> {
    vec![
        score_architecture(rt),
        score_testing(rt),
        score_documentation(),
        score_security(),
        score_performance(),
        score_maintainability(rt),
        score_observability(rt),
        score_supply_chain(),
        score_dependencies(rt),
        score_gotchas(rt),
        score_rl_convergence(rt),
    ]
}

// ─────────────────────────────────────────────────────────────────────────────
// Real-data categories (delegate to existing handlers)
// ─────────────────────────────────────────────────────────────────────────────

/// Category 1: Architecture — derived from wiring stats.
///
/// Score = max * (1 - orphan_ratio), clamped to `[0, MAX_ARCHITECTURE]`.
fn score_architecture(rt: &mut HookRuntime) -> CategoryReport {
    let raw = super::super::cli_handlers::cli_wiring_status(rt, &Value::Null);
    let parsed: Value = serde_json::from_str(&raw).unwrap_or(Value::Null);

    let orphan = parsed
        .get("orphan_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let total_pub = parsed
        .get("total_pub_symbols")
        .and_then(Value::as_u64)
        .unwrap_or(1)
        .max(1);
    let ratio = (orphan as f64 / total_pub as f64).clamp(0.0, 1.0);
    let score = ((1.0 - ratio) * f64::from(MAX_ARCHITECTURE)).round() as u32;
    let score = score.min(MAX_ARCHITECTURE);

    CategoryReport {
        name: "architecture",
        score,
        max: MAX_ARCHITECTURE,
        source: "wiring",
        details: json!({
            "orphan_count": orphan,
            "total_pub_symbols": total_pub,
            "orphan_ratio": (ratio * 1000.0).round() / 1000.0,
        }),
    }
}

/// Category 6: Maintainability — derived from gate_metrics health stream.
///
/// Currently uses health_delta improvement/regression ratio as a proxy.
/// Once Q1 TDG is run repo-wide, this will switch to grade distribution.
fn score_maintainability(rt: &mut HookRuntime) -> CategoryReport {
    let raw = super::super::cli_handlers::cli_gate_metrics(rt, &Value::Null);
    let parsed: Value = serde_json::from_str(&raw).unwrap_or(Value::Null);

    let improvements = parsed
        .get("health_delta_improvement_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let regressions = parsed
        .get("health_delta_regression_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let total = improvements + regressions;
    let positive_ratio = if total == 0 {
        0.7 // default to "neutral" 70% when no data yet
    } else {
        improvements as f64 / total as f64
    };
    let score = (positive_ratio * f64::from(MAX_MAINTAINABILITY)).round() as u32;

    CategoryReport {
        name: "maintainability",
        score: score.min(MAX_MAINTAINABILITY),
        max: MAX_MAINTAINABILITY,
        source: "health_delta",
        details: json!({
            "improvements": improvements,
            "regressions": regressions,
            "positive_ratio": (positive_ratio * 1000.0).round() / 1000.0,
        }),
    }
}

/// Category 7: Observability — derived from gate_metrics counter activity.
fn score_observability(rt: &mut HookRuntime) -> CategoryReport {
    let raw = super::super::cli_handlers::cli_gate_metrics(rt, &Value::Null);
    let parsed: Value = serde_json::from_str(&raw).unwrap_or(Value::Null);

    let total = parsed
        .get("total_invocations")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    // Active observability: any recorded invocation gives full credit; zero gives 60%.
    let score = if total > 0 {
        MAX_OBSERVABILITY
    } else {
        (f64::from(MAX_OBSERVABILITY) * 0.6).round() as u32
    };

    CategoryReport {
        name: "observability",
        score,
        max: MAX_OBSERVABILITY,
        source: "gate_metrics",
        details: json!({"total_invocations": total}),
    }
}

/// Category 9: Dependencies — derived from index symbol density.
fn score_dependencies(rt: &mut HookRuntime) -> CategoryReport {
    let raw = super::super::cli_handlers_index::cli_index_status(rt, &Value::Null);
    let parsed: Value = serde_json::from_str(&raw).unwrap_or(Value::Null);

    let initialized = parsed
        .get("initialized")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let symbol_count = parsed
        .get("symbol_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let score = if initialized && symbol_count > 1000 {
        MAX_DEPENDENCIES
    } else if initialized {
        (f64::from(MAX_DEPENDENCIES) * 0.7).round() as u32
    } else {
        0
    };

    CategoryReport {
        name: "dependencies",
        score,
        max: MAX_DEPENDENCIES,
        source: "index",
        details: json!({
            "initialized": initialized,
            "symbol_count": symbol_count,
        }),
    }
}

/// Category 10: Gotchas — derived from the gotcha database stats.
fn score_gotchas(rt: &mut HookRuntime) -> CategoryReport {
    let raw = super::super::cli_handlers::cli_gotcha_stats(rt, &Value::Null);
    let parsed: Value = serde_json::from_str(&raw).unwrap_or(Value::Null);

    let total = parsed.get("total").and_then(Value::as_u64).unwrap_or(0);
    let resolved = parsed.get("resolved").and_then(Value::as_u64).unwrap_or(0);
    let resolution_ratio = if total == 0 {
        1.0
    } else {
        resolved as f64 / total as f64
    };
    let score = (resolution_ratio * f64::from(MAX_GOTCHAS)).round() as u32;

    CategoryReport {
        name: "gotchas",
        score: score.min(MAX_GOTCHAS),
        max: MAX_GOTCHAS,
        source: "gotcha_db",
        details: json!({
            "total": total,
            "resolved": resolved,
            "resolution_ratio": (resolution_ratio * 1000.0).round() / 1000.0,
        }),
    }
}

/// Category 11: RL convergence — derived from learning EMA reward stability.
fn score_rl_convergence(rt: &mut HookRuntime) -> CategoryReport {
    let raw = super::super::cli_handlers::cli_learning_status(rt, &Value::Null);
    let parsed: Value = serde_json::from_str(&raw).unwrap_or(Value::Null);

    let ema = parsed
        .get("ema_reward")
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    // EMA reward in [0.0, 1.0] maps linearly to [0, MAX_RL_CONVERGENCE].
    let normalised = ema.clamp(0.0, 1.0);
    let score = (normalised * f64::from(MAX_RL_CONVERGENCE)).round() as u32;

    CategoryReport {
        name: "rl_convergence",
        score: score.min(MAX_RL_CONVERGENCE),
        max: MAX_RL_CONVERGENCE,
        source: "learning",
        details: json!({
            "ema_reward": (ema * 1000.0).round() / 1000.0,
        }),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Stub categories — emit `source: "stub"` so consumers can distinguish.
// Each stub returns `max * 0.7` (a deliberately conservative neutral score)
// rather than zero, so a healthy repo isn't punished for unimplemented
// integrations. As external gates are wired (cargo-deny, nextest, coverage),
// stubs can be replaced with real measurements.
// ─────────────────────────────────────────────────────────────────────────────

fn stub_score(name: &'static str, max: u32, hint: &str) -> CategoryReport {
    let score = (f64::from(max) * 0.7).round() as u32;
    CategoryReport {
        name,
        score,
        max,
        source: "stub",
        details: json!({
            "neutral_score_70pct": true,
            "hint": hint,
        }),
    }
}

/// Category 2: Testing — derived from cargo-mutants kill_rate when a fresh
/// cache entry exists, falling back to the conservative stub otherwise.
///
/// Reads `<workspace>/.touring-cache/mutation-test/_workspace.json` (Wave T1).
/// Score formula: `MAX_TESTING * (kill_rate / 100)`, clamped to `[0, MAX_TESTING]`.
fn score_testing(rt: &mut HookRuntime) -> CategoryReport {
    let cache_root = rt.project_root.join(".touring-cache");
    match crate::mutation_test::cache_load(&cache_root, None) {
        Ok(Some(report)) => {
            let pct = f64::from(report.kill_rate) / 100.0;
            let raw = (f64::from(MAX_TESTING) * pct).round();
            let score = raw.clamp(0.0, f64::from(MAX_TESTING)) as u32;
            CategoryReport {
                name: "testing",
                score,
                max: MAX_TESTING,
                source: "mutation_test",
                details: json!({
                    "kill_rate": report.kill_rate,
                    "mutants_total": report.mutants_total,
                    "mutants_killed": report.mutants_killed,
                    "mutants_survived": report.mutants_survived,
                    "passed_threshold": report.passed_threshold,
                    "threshold": report.threshold,
                    "package": report.package,
                    "elapsed_secs": report.elapsed_secs,
                    "cargo_mutants_version": report.cargo_mutants_version,
                }),
            }
        }
        _ => stub_score(
            "testing",
            MAX_TESTING,
            "run `touring mutation-test` to populate cache; falls back to nextest stub",
        ),
    }
}

fn score_documentation() -> CategoryReport {
    stub_score(
        "documentation",
        MAX_DOCUMENTATION,
        "wire to `cargo doc --no-deps` rustdoc coverage",
    )
}

fn score_security() -> CategoryReport {
    stub_score(
        "security",
        MAX_SECURITY,
        "wire to `cargo deny check` + unwrap_audit + security antipatterns",
    )
}

fn score_performance() -> CategoryReport {
    stub_score(
        "performance",
        MAX_PERFORMANCE,
        "wire to P99 latency guards (hdrhistogram) and bench delta",
    )
}

fn score_supply_chain() -> CategoryReport {
    stub_score(
        "supply_chain",
        MAX_SUPPLY_CHAIN,
        "wire to `cargo deny check advisories` + `cargo machete`",
    )
}

// ─────────────────────────────────────────────────────────────────────────────
// Diagnostic emission (RFC-100 / Wave Q4 — bridges per-category low scores
// into the unified diagnostic stream so consumers can correlate W/Q/B/G/M
// findings with the executive R1 dashboard).
// ─────────────────────────────────────────────────────────────────────────────

fn build_diagnostics(categories: &[CategoryReport]) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    for c in categories {
        let pct = f64::from(c.score) / f64::from(c.max);
        if pct < 0.5 {
            out.push(category_diagnostic(c, Severity::Warning));
        } else if pct < 0.3 {
            out.push(category_diagnostic(c, Severity::Error));
        }
    }
    out
}

fn category_diagnostic(c: &CategoryReport, sev: Severity) -> Diagnostic {
    // Map a category to the closest existing RFC-100 code range. R-codes
    // (600..699) are reserved for repo-score per RFC-100 §3 — we use Q-200
    // here as a conservative proxy until R-codes are formally allocated in
    // a follow-up wave.
    let code = codes::Q_200_QUALITY_BELOW_THRESHOLD;
    let pct = (f64::from(c.score) / f64::from(c.max) * 100.0).round();
    Diagnostic::new(
        code,
        sev,
        format!(
            "category `{}` scored {}/{} ({}%)",
            c.name, c.score, c.max, pct as u32
        ),
    )
    .with_help(format!(
        "inspect details via `touring repo-score --category {}`",
        c.name
    ))
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn max_repo_score_matches_per_category_sum() {
        // RFC-R1 §1 doc claims 289, but listed constants sum to 269.
        // We trust the per-category constants (single source of truth).
        assert_eq!(MAX_REPO_SCORE, 269);
    }

    #[test]
    fn grade_letter_aplus_at_95() {
        assert_eq!(grade_letter(95.0), "A+");
        assert_eq!(grade_letter(99.9), "A+");
    }

    #[test]
    fn grade_letter_b_around_82() {
        assert_eq!(grade_letter(82.0), "B");
        assert_eq!(grade_letter(80.0), "B");
        assert_eq!(grade_letter(85.0), "B+");
    }

    #[test]
    fn grade_letter_f_below_60() {
        assert_eq!(grade_letter(59.9), "F");
        assert_eq!(grade_letter(0.0), "F");
    }

    #[test]
    fn grade_letter_d_in_60_to_70() {
        assert_eq!(grade_letter(60.0), "D");
        assert_eq!(grade_letter(69.9), "D");
    }

    #[test]
    fn stub_score_returns_70_percent() {
        let s = stub_score("test_cat", 30, "hint");
        assert_eq!(s.score, 21, "30 * 0.7 rounds to 21");
        assert_eq!(s.source, "stub");
        assert_eq!(s.max, 30);
    }

    #[test]
    fn stub_score_for_max_20() {
        let s = stub_score("test_cat", 20, "");
        assert_eq!(s.score, 14, "20 * 0.7 = 14");
    }

    #[test]
    fn diagnostic_emitted_for_low_score() {
        let cat = CategoryReport {
            name: "x",
            score: 2,
            max: 30, // 6.6%
            source: "test",
            details: json!({}),
        };
        let diags = build_diagnostics(&[cat]);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("scored 2/30"));
    }

    #[test]
    fn no_diagnostic_for_healthy_score() {
        let cat = CategoryReport {
            name: "x",
            score: 28,
            max: 30, // 93%
            source: "test",
            details: json!({}),
        };
        let diags = build_diagnostics(&[cat]);
        assert!(
            diags.is_empty(),
            "high-scoring category should emit no diagnostic"
        );
    }

    #[test]
    fn category_constants_sum_to_max() {
        let sum = MAX_ARCHITECTURE
            + MAX_TESTING
            + MAX_DOCUMENTATION
            + MAX_SECURITY
            + MAX_PERFORMANCE
            + MAX_MAINTAINABILITY
            + MAX_OBSERVABILITY
            + MAX_SUPPLY_CHAIN
            + MAX_DEPENDENCIES
            + MAX_GOTCHAS
            + MAX_RL_CONVERGENCE;
        assert_eq!(sum, MAX_REPO_SCORE);
    }

    #[test]
    fn category_report_into_json_has_required_fields() {
        let c = CategoryReport {
            name: "x",
            score: 10,
            max: 20,
            source: "test",
            details: json!({"foo": "bar"}),
        };
        let j = c.into_json();
        assert_eq!(j["score"], 10);
        assert_eq!(j["max"], 20);
        assert_eq!(j["source"], "test");
        assert_eq!(j["details"]["foo"], "bar");
    }
}
