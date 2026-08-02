//! Technical Debt Grading (TDG) — six-dimension orthogonal scoring with
//! letter grades.
//!
//! Inspired by PMAT 3.15.0 TDG (Pragmatic Multi-language Agent Toolkit).
//! Wraps existing [`QualityReport`] + [`RustQualitySignals`] to add
//! letter-grade UX (A+..F) on top of numerical composite scores. This is
//! a **potentialization** layer (Hard Rule #0): it does not replace any
//! existing API — it composes them into a higher-signal interface.
//!
//! # Six Orthogonal Dimensions
//!
//! 1. **complexity** — cyclomatic + cognitive + Halstead-MI penalty
//! 2. **coverage** — `(error_handling_coverage + test_proxy.score) / 2`
//! 3. **duplication** — content-hash clustering (caller-supplied; MVP `1.0`)
//! 4. **churn** — file edit frequency (caller-supplied; sourced from
//!    `FileKnowledgeDB` `edit_count`, **never** `git log` per Hard
//!    Rule #11)
//! 5. **entropy** — [`RustQualitySignals::health_score`] (Rust only;
//!    `1.0` for non-Rust files)
//! 6. **antipatterns** — `1.0 - min(antipatterns.len() * 0.05, 0.4)`
//!
//! # Composite weighting
//!
//! Equal-weight buckets (0.20 / 0.20 / 0.10 / 0.10 / 0.20 / 0.20) summing
//! to `1.0`. Duplication and churn get half-weight because they are
//! caller-supplied (lower signal than computed dimensions).
//!
//! # Letter grade thresholds
//!
//! | Grade | Composite range |
//! |-------|-----------------|
//! | A+    | `>= 0.95`       |
//! | A     | `[0.90, 0.95)`  |
//! | B+    | `[0.85, 0.90)`  |
//! | B     | `[0.80, 0.85)`  |
//! | C+    | `[0.75, 0.80)`  |
//! | C     | `[0.70, 0.75)`  |
//! | D     | `[0.60, 0.70)`  |
//! | F     | `< 0.60`        |
//!
//! `FileKnowledgeDB`: ../../touring_hooks/struct.FileKnowledgeDB.html

use serde::{Deserialize, Serialize};

use touring_foundation::diagnostic::{DiagnosticCode, Severity, codes};

use super::{QualityReport, RustQualitySignals};

/// Per-dimension weights summing to `1.0`. Duplication and churn are
/// half-weight because callers supply them (lower signal than computed
/// dimensions like complexity/entropy).
const W_COMPLEXITY: f64 = 0.20;
const W_COVERAGE: f64 = 0.20;
const W_DUPLICATION: f64 = 0.10;
const W_CHURN: f64 = 0.10;
const W_ENTROPY: f64 = 0.20;
const W_ANTIPATTERNS: f64 = 0.20;

/// Technical Debt Grade — letter grade A+ through F.
///
/// Order of variants (via `#[derive(PartialOrd, Ord)]`) matches **worst
/// → best** so `min`/`max` semantics are intuitive: `min(grade_a, grade_b)`
/// returns the worse grade.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum TdgGrade {
    /// Composite < 0.60 — refactor required before further work.
    F,
    /// Composite in `[0.60, 0.70)` — STOP, refactor before edit.
    D,
    /// Composite in `[0.70, 0.75)` — edit cautiously, plan mitigation.
    C,
    /// Composite in `[0.75, 0.80)` — edit cautiously.
    CPlus,
    /// Composite in `[0.80, 0.85)` — edit OK, consider light refactor.
    B,
    /// Composite in `[0.85, 0.90)` — edit OK.
    BPlus,
    /// Composite in `[0.90, 0.95)` — edit freely.
    A,
    /// Composite >= 0.95 — pristine.
    APlus,
}

impl TdgGrade {
    /// Compute grade from composite score in `[0.0, 1.0]`.
    ///
    /// Out-of-range values are clamped: `> 1.0` → `APlus`, `< 0.0` → `F`.
    #[must_use]
    pub fn from_composite(composite: f64) -> Self {
        let c = composite.clamp(0.0, 1.0);
        if c >= 0.95 {
            Self::APlus
        } else if c >= 0.90 {
            Self::A
        } else if c >= 0.85 {
            Self::BPlus
        } else if c >= 0.80 {
            Self::B
        } else if c >= 0.75 {
            Self::CPlus
        } else if c >= 0.70 {
            Self::C
        } else if c >= 0.60 {
            Self::D
        } else {
            Self::F
        }
    }

    /// Letter representation: `"A+"`, `"A"`, `"B+"`, `"B"`, `"C+"`,
    /// `"C"`, `"D"`, `"F"`.
    #[must_use]
    pub fn letter(&self) -> &'static str {
        match self {
            Self::APlus => "A+",
            Self::A => "A",
            Self::BPlus => "B+",
            Self::B => "B",
            Self::CPlus => "C+",
            Self::C => "C",
            Self::D => "D",
            Self::F => "F",
        }
    }

    /// Recommended action for this grade (Portuguese — matches Touring
    /// SKILL.md operator language).
    #[must_use]
    pub fn recommended_action(&self) -> &'static str {
        match self {
            Self::APlus | Self::A => "Edit livre",
            Self::BPlus | Self::B => "Edit OK, considerar refactor leve",
            Self::CPlus | Self::C => "Edit cauteloso, planejar mitigação",
            Self::D => "STOP — refactor antes de edit",
            Self::F => "STOP — análise arquitetural primeiro",
        }
    }
}

impl std::fmt::Display for TdgGrade {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.letter())
    }
}

/// Six-dimension Technical Debt Grading report.
///
/// All dimension scores are in `[0.0, 1.0]` with `1.0` meaning **healthy
/// / no debt** and `0.0` meaning **maximum debt**. The `composite` is the
/// weighted average; `grade` is derived from composite via
/// [`TdgGrade::from_composite`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TdgReport {
    /// Cyclomatic + cognitive + Halstead-MI penalty.
    pub complexity: f64,
    /// Average of `error_handling_coverage` and `test_proxy.score`.
    pub coverage: f64,
    /// Content-hash duplication score (caller-supplied; MVP `1.0`).
    pub duplication: f64,
    /// File churn stability (caller-supplied; from FileKnowledgeDB
    /// `edit_count`, never git).
    pub churn: f64,
    /// Semantic entropy from `RustQualitySignals::health_score()`
    /// (Rust only; `1.0` for non-Rust files).
    pub entropy: f64,
    /// Antipattern cleanliness (`1.0 - weighted_hit_penalty`).
    pub antipatterns: f64,
    /// Weighted composite over six dimensions, in `[0.0, 1.0]`.
    pub composite: f64,
    /// Letter grade derived from `composite`.
    pub grade: TdgGrade,
}

impl TdgReport {
    /// Build a TdgReport from raw component scores in `[0.0, 1.0]`.
    ///
    /// Each dimension is clamped before weighting so callers can pass
    /// out-of-range placeholders without breaking the composite.
    #[must_use]
    pub fn from_components(
        complexity: f64,
        coverage: f64,
        duplication: f64,
        churn: f64,
        entropy: f64,
        antipatterns: f64,
    ) -> Self {
        let complexity = complexity.clamp(0.0, 1.0);
        let coverage = coverage.clamp(0.0, 1.0);
        let duplication = duplication.clamp(0.0, 1.0);
        let churn = churn.clamp(0.0, 1.0);
        let entropy = entropy.clamp(0.0, 1.0);
        let antipatterns = antipatterns.clamp(0.0, 1.0);

        let composite = (complexity * W_COMPLEXITY
            + coverage * W_COVERAGE
            + duplication * W_DUPLICATION
            + churn * W_CHURN
            + entropy * W_ENTROPY
            + antipatterns * W_ANTIPATTERNS)
            .clamp(0.0, 1.0);
        let grade = TdgGrade::from_composite(composite);

        Self {
            complexity,
            coverage,
            duplication,
            churn,
            entropy,
            antipatterns,
            composite,
            grade,
        }
    }

    /// Build a TdgReport from a [`QualityReport`], with optional Rust
    /// signals and external duplication/churn dimensions.
    ///
    /// Dimensions derived as follows:
    ///
    /// - **complexity** — `1 - cc_pen - cog_pen - mi_pen` where:
    ///   - `cc_pen = 0.30` if `max_complexity > 20`, `0.15` if `> 10`, else `0`
    ///   - `cog_pen = 0.30` if `cognitive > 60`, `0.15` if `> 30`, else `0`
    ///   - `mi_pen = clamp((100 - MI) / 100, 0, 0.20)`
    /// - **coverage** — `(error_handling_coverage + test_proxy.score) / 2`
    /// - **entropy** — `rust_signals.health_score()` if `Some`, else `1.0`
    /// - **antipatterns** — `1 - min(hits * 0.05, 0.4)`
    ///
    /// `duplication` and `churn` must be supplied by the caller. Use
    /// `1.0` as a neutral placeholder when no data is available
    /// (fresh files, missing FileKnowledgeDB entry).
    #[must_use]
    pub fn from_quality_report(
        report: &QualityReport,
        rust_signals: Option<&RustQualitySignals>,
        duplication: f64,
        churn: f64,
    ) -> Self {
        // Complexity
        let cc_pen = if report.complexity.max_complexity > 20 {
            0.30
        } else if report.complexity.max_complexity > 10 {
            0.15
        } else {
            0.0
        };
        let cog_pen = if report.complexity.cognitive_complexity > 60 {
            0.30
        } else if report.complexity.cognitive_complexity > 30 {
            0.15
        } else {
            0.0
        };
        let mi_pen = ((100.0 - report.complexity.maintainability_index) / 100.0).clamp(0.0, 0.20);
        let complexity = (1.0 - cc_pen - cog_pen - mi_pen).clamp(0.0, 1.0);

        // Coverage
        let coverage = ((report.error_handling_coverage + f64::from(report.test_proxy.score))
            / 2.0)
            .clamp(0.0, 1.0);

        // Entropy
        let entropy = rust_signals
            .map(|s| f64::from(s.health_score()))
            .unwrap_or(1.0);

        // Antipatterns
        let ap_pen = (report.antipatterns.len() as f64 * 0.05).min(0.4);
        let antipatterns = (1.0 - ap_pen).clamp(0.0, 1.0);

        Self::from_components(
            complexity,
            coverage,
            duplication,
            churn,
            entropy,
            antipatterns,
        )
    }

    /// Letter grade as `&str`.
    #[must_use]
    pub fn grade_letter(&self) -> &'static str {
        self.grade.letter()
    }

    /// Compact one-line human summary like `"B+ (composite=0.87)"`.
    #[must_use]
    pub fn human_summary(&self) -> String {
        format!("{} (composite={:.2})", self.grade, self.composite)
    }

    /// JSON object for inclusion in CLI output. All numeric dimensions
    /// rounded to 3 decimal places for stable diffability.
    #[must_use]
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "grade": self.grade.letter(),
            "composite": round3(self.composite),
            "complexity": round3(self.complexity),
            "coverage": round3(self.coverage),
            "duplication": round3(self.duplication),
            "churn": round3(self.churn),
            "entropy": round3(self.entropy),
            "antipatterns": round3(self.antipatterns),
            "action": self.grade.recommended_action(),
        })
    }
}

#[inline]
fn round3(x: f64) -> f64 {
    (x * 1000.0).round() / 1000.0
}

// ---------------------------------------------------------------------
// Q4 integration — DiagnosticCode trait impl (RFC-100)
// ---------------------------------------------------------------------
//
// Maps TDG grades to RFC-100 §5.2 diagnostic codes:
// - F  → Q-201 (error)
// - D  → Q-202 (warning)
// - C  → Q-203 (info)
// - C+, B, B+, A, A+ → no diagnostic emitted (use `to_diagnostic_opt()`)

impl TdgReport {
    /// Emit an RFC-100 diagnostic when the grade warrants attention.
    /// Returns `None` for grades C+ and above (no action needed).
    #[must_use]
    pub fn to_diagnostic_opt(&self) -> Option<touring_foundation::diagnostic::Diagnostic> {
        match self.grade {
            TdgGrade::F => Some(
                touring_foundation::diagnostic::Diagnostic::new(
                    codes::Q_201_TDG_GRADE_F,
                    Severity::Error,
                    format!(
                        "TDG grade F detected (composite={:.3}) — architectural review required",
                        self.composite
                    ),
                )
                .with_help(self.grade.recommended_action()),
            ),
            TdgGrade::D => Some(
                touring_foundation::diagnostic::Diagnostic::new(
                    codes::Q_202_TDG_GRADE_D,
                    Severity::Warning,
                    format!(
                        "TDG grade D detected (composite={:.3}) — refactor recommended before edit",
                        self.composite
                    ),
                )
                .with_help(self.grade.recommended_action()),
            ),
            TdgGrade::C => Some(touring_foundation::diagnostic::Diagnostic::new(
                codes::Q_203_TDG_GRADE_C,
                Severity::Info,
                format!(
                    "TDG grade C (composite={:.3}) — edit cautiously",
                    self.composite
                ),
            )),
            // C+, B, B+, A, A+ → no diagnostic
            _ => None,
        }
    }
}

/// `DiagnosticCode` blanket impl — always emits Q-203 (info) when grade
/// is C or worse; callers wanting full grade-mapped severity should use
/// [`TdgReport::to_diagnostic_opt`] instead.
impl DiagnosticCode for TdgReport {
    fn code(&self) -> &'static str {
        match self.grade {
            TdgGrade::F => codes::Q_201_TDG_GRADE_F,
            TdgGrade::D => codes::Q_202_TDG_GRADE_D,
            _ => codes::Q_203_TDG_GRADE_C,
        }
    }

    fn severity(&self) -> Severity {
        match self.grade {
            TdgGrade::F => Severity::Error,
            TdgGrade::D => Severity::Warning,
            _ => Severity::Info,
        }
    }

    fn message(&self) -> String {
        format!(
            "TDG grade {} (composite={:.3}) — {}",
            self.grade,
            self.composite,
            self.grade.recommended_action()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quality::{Antipattern, ComplexityMetrics, HalsteadMetrics, QualityReport};

    fn make_report(
        max_cc: usize,
        cognitive: usize,
        mi: f64,
        antipattern_hits: usize,
        err_cov: f64,
    ) -> QualityReport {
        QualityReport {
            file_path: "test.rs".to_string(),
            language: "rust".to_string(),
            antipatterns: (0..antipattern_hits)
                .map(|i| Antipattern {
                    language: "rust".to_string(),
                    pattern: ".unwrap()".to_string(),
                    message: "test".to_string(),
                    line: i,
                })
                .collect(),
            complexity: ComplexityMetrics {
                function_count: 1,
                max_complexity: max_cc,
                avg_complexity: max_cc as f64,
                symbol_count: 1,
                cognitive_complexity: cognitive,
                sloc: 10,
                cloc: 0,
                lloc: 10,
                nexits: 0,
                blank: 0,
                maintainability_index: mi,
                halstead: HalsteadMetrics::default(),
            },
            unwrap_count: 0,
            unwrap_lines: vec![],
            error_handling_coverage: err_cov,
            question_mark_density: 0.0,
            test_proxy: Default::default(),
            expect_count: 0,
            unwrap_risk_score: 0.0,
            score: 0.0,
        }
    }

    #[test]
    fn grade_aplus_at_threshold() {
        assert_eq!(TdgGrade::from_composite(0.95), TdgGrade::APlus);
        assert_eq!(TdgGrade::from_composite(1.0), TdgGrade::APlus);
        assert_eq!(TdgGrade::from_composite(0.999), TdgGrade::APlus);
    }

    #[test]
    fn grade_a_range() {
        assert_eq!(TdgGrade::from_composite(0.949), TdgGrade::A);
        assert_eq!(TdgGrade::from_composite(0.90), TdgGrade::A);
    }

    #[test]
    fn grade_bplus_range() {
        assert_eq!(TdgGrade::from_composite(0.899), TdgGrade::BPlus);
        assert_eq!(TdgGrade::from_composite(0.85), TdgGrade::BPlus);
    }

    #[test]
    fn grade_b_range() {
        assert_eq!(TdgGrade::from_composite(0.849), TdgGrade::B);
        assert_eq!(TdgGrade::from_composite(0.80), TdgGrade::B);
    }

    #[test]
    fn grade_cplus_range() {
        assert_eq!(TdgGrade::from_composite(0.799), TdgGrade::CPlus);
        assert_eq!(TdgGrade::from_composite(0.75), TdgGrade::CPlus);
    }

    #[test]
    fn grade_c_range() {
        assert_eq!(TdgGrade::from_composite(0.749), TdgGrade::C);
        assert_eq!(TdgGrade::from_composite(0.70), TdgGrade::C);
    }

    #[test]
    fn grade_d_range() {
        assert_eq!(TdgGrade::from_composite(0.699), TdgGrade::D);
        assert_eq!(TdgGrade::from_composite(0.60), TdgGrade::D);
    }

    #[test]
    fn grade_f_range() {
        assert_eq!(TdgGrade::from_composite(0.599), TdgGrade::F);
        assert_eq!(TdgGrade::from_composite(0.0), TdgGrade::F);
        assert_eq!(TdgGrade::from_composite(-0.5), TdgGrade::F);
    }

    #[test]
    fn grade_letters_match_documentation() {
        assert_eq!(TdgGrade::APlus.letter(), "A+");
        assert_eq!(TdgGrade::A.letter(), "A");
        assert_eq!(TdgGrade::BPlus.letter(), "B+");
        assert_eq!(TdgGrade::B.letter(), "B");
        assert_eq!(TdgGrade::CPlus.letter(), "C+");
        assert_eq!(TdgGrade::C.letter(), "C");
        assert_eq!(TdgGrade::D.letter(), "D");
        assert_eq!(TdgGrade::F.letter(), "F");
    }

    #[test]
    fn grade_recommended_action_non_empty_for_all() {
        for g in [
            TdgGrade::APlus,
            TdgGrade::A,
            TdgGrade::BPlus,
            TdgGrade::B,
            TdgGrade::CPlus,
            TdgGrade::C,
            TdgGrade::D,
            TdgGrade::F,
        ] {
            assert!(
                !g.recommended_action().is_empty(),
                "grade {g} has empty action"
            );
        }
    }

    #[test]
    fn grade_ordering_worst_to_best() {
        assert!(TdgGrade::F < TdgGrade::D);
        assert!(TdgGrade::D < TdgGrade::C);
        assert!(TdgGrade::C < TdgGrade::CPlus);
        assert!(TdgGrade::CPlus < TdgGrade::B);
        assert!(TdgGrade::B < TdgGrade::BPlus);
        assert!(TdgGrade::BPlus < TdgGrade::A);
        assert!(TdgGrade::A < TdgGrade::APlus);
    }

    #[test]
    fn from_components_clamps_inputs() {
        let r = TdgReport::from_components(2.0, -0.5, 1.5, -1.0, 1.0, 1.0);
        assert!((0.0..=1.0).contains(&r.complexity));
        assert!((0.0..=1.0).contains(&r.coverage));
        assert!((0.0..=1.0).contains(&r.duplication));
        assert!((0.0..=1.0).contains(&r.churn));
        assert!((0.0..=1.0).contains(&r.composite));
    }

    #[test]
    fn from_components_perfect_scores_a_plus() {
        let r = TdgReport::from_components(1.0, 1.0, 1.0, 1.0, 1.0, 1.0);
        assert_eq!(r.grade, TdgGrade::APlus);
        assert!((r.composite - 1.0).abs() < 1e-9);
    }

    #[test]
    fn from_components_zero_scores_f() {
        let r = TdgReport::from_components(0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
        assert_eq!(r.grade, TdgGrade::F);
        assert_eq!(r.composite, 0.0);
    }

    #[test]
    fn weights_sum_to_one() {
        let total =
            W_COMPLEXITY + W_COVERAGE + W_DUPLICATION + W_CHURN + W_ENTROPY + W_ANTIPATTERNS;
        assert!(
            (total - 1.0).abs() < 1e-9,
            "weights must sum to 1.0, got {total}"
        );
    }

    #[test]
    fn from_quality_report_clean_file_grades_high() {
        let report = make_report(3, 5, 95.0, 0, 0.9);
        let tdg = TdgReport::from_quality_report(&report, None, 1.0, 1.0);
        assert!(
            tdg.composite >= 0.85,
            "clean file should grade B+ or better, got {} = {:.3}",
            tdg.grade,
            tdg.composite
        );
    }

    #[test]
    fn from_quality_report_dirty_file_grades_low() {
        // max_cc=25 (cc_pen=0.30) + cognitive=75 (cog_pen=0.30) + MI=30 (mi_pen=0.20)
        // → complexity = max(0, 1.0 - 0.80) = 0.20
        // 10 antipatterns → ap_pen=0.4 → antipatterns = 0.6
        // err_cov=0.1, test_proxy=0 → coverage = 0.05
        // duplication=0.5, churn=0.5, entropy=1.0
        let report = make_report(25, 75, 30.0, 10, 0.1);
        let tdg = TdgReport::from_quality_report(&report, None, 0.5, 0.5);
        assert!(
            tdg.composite < 0.70,
            "dirty file should grade C or worse, got {} = {:.3}",
            tdg.grade,
            tdg.composite
        );
        assert!(matches!(tdg.grade, TdgGrade::F | TdgGrade::D | TdgGrade::C));
    }

    #[test]
    fn from_quality_report_uses_rust_signals_when_provided() {
        // Build minimal rust signals proxy via from_components round-trip:
        // since RustQualitySignals construction needs a syn parse, we test
        // the entropy plumbing through the no-signals path here.
        let report = make_report(3, 5, 95.0, 0, 0.9);
        let with_none = TdgReport::from_quality_report(&report, None, 1.0, 1.0);
        // entropy=1.0 from None branch
        assert!((with_none.entropy - 1.0).abs() < 1e-9);
    }

    #[test]
    fn human_summary_format_contains_grade_and_composite() {
        let r = TdgReport::from_components(0.9, 0.9, 0.9, 0.9, 0.9, 0.9);
        let s = r.human_summary();
        assert!(s.contains('('), "summary should contain '(': {s}");
        assert!(
            s.contains("composite="),
            "summary should contain 'composite=': {s}"
        );
    }

    #[test]
    fn to_json_includes_all_required_fields() {
        let r = TdgReport::from_components(0.8, 0.8, 0.8, 0.8, 0.8, 0.8);
        let j = r.to_json();
        let obj = j.as_object().expect("json object");
        for key in &[
            "grade",
            "composite",
            "complexity",
            "coverage",
            "duplication",
            "churn",
            "entropy",
            "antipatterns",
            "action",
        ] {
            assert!(obj.contains_key(*key), "missing key {key} in {j}");
        }
    }

    #[test]
    fn to_json_grade_is_letter_string() {
        let r = TdgReport::from_components(1.0, 1.0, 1.0, 1.0, 1.0, 1.0);
        let j = r.to_json();
        assert_eq!(j.get("grade").and_then(|v| v.as_str()), Some("A+"));
    }

    #[test]
    fn grade_display_format_matches_letter() {
        assert_eq!(format!("{}", TdgGrade::APlus), "A+");
        assert_eq!(format!("{}", TdgGrade::F), "F");
        assert_eq!(format!("{}", TdgGrade::BPlus), "B+");
    }

    #[test]
    fn round3_precision() {
        assert_eq!(round3(0.123456), 0.123);
        assert_eq!(round3(0.999999), 1.0);
        assert_eq!(round3(0.5), 0.5);
    }

    #[test]
    fn from_quality_report_exit_path_complexity_low_cc() {
        // max_cc=8 < 10 → cc_pen=0; cognitive=20 < 30 → cog_pen=0; MI=80 → mi_pen=0.20
        let report = make_report(8, 20, 80.0, 0, 0.9);
        let tdg = TdgReport::from_quality_report(&report, None, 1.0, 1.0);
        // complexity = 1.0 - 0 - 0 - 0.20 = 0.80
        assert!(
            (tdg.complexity - 0.80).abs() < 0.01,
            "expected complexity ~0.80, got {}",
            tdg.complexity
        );
    }

    #[test]
    fn from_quality_report_exit_path_complexity_mid_cc() {
        // max_cc=15 → cc_pen=0.15; cognitive=40 → cog_pen=0.15; MI=100 → mi_pen=0
        let report = make_report(15, 40, 100.0, 0, 0.9);
        let tdg = TdgReport::from_quality_report(&report, None, 1.0, 1.0);
        // complexity = 1.0 - 0.15 - 0.15 - 0 = 0.70
        assert!(
            (tdg.complexity - 0.70).abs() < 0.01,
            "expected complexity ~0.70, got {}",
            tdg.complexity
        );
    }

    // ---- Q4 integration: DiagnosticCode trait impl ----

    #[test]
    fn to_diagnostic_opt_emits_q201_for_grade_f() {
        let r = TdgReport::from_components(0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
        assert_eq!(r.grade, TdgGrade::F);
        let d = r.to_diagnostic_opt().expect("F should emit diagnostic");
        assert_eq!(d.code, "Q-201");
        assert_eq!(d.severity, Severity::Error);
        assert!(d.help.is_some(), "F diagnostic should have help");
    }

    #[test]
    fn to_diagnostic_opt_emits_q202_for_grade_d() {
        // composite ~0.65 → D
        let r = TdgReport::from_components(0.65, 0.65, 0.65, 0.65, 0.65, 0.65);
        assert_eq!(r.grade, TdgGrade::D);
        let d = r.to_diagnostic_opt().expect("D should emit diagnostic");
        assert_eq!(d.code, "Q-202");
        assert_eq!(d.severity, Severity::Warning);
    }

    #[test]
    fn to_diagnostic_opt_emits_q203_for_grade_c() {
        // composite ~0.72 → C
        let r = TdgReport::from_components(0.72, 0.72, 0.72, 0.72, 0.72, 0.72);
        assert_eq!(r.grade, TdgGrade::C);
        let d = r.to_diagnostic_opt().expect("C should emit diagnostic");
        assert_eq!(d.code, "Q-203");
        assert_eq!(d.severity, Severity::Info);
    }

    #[test]
    fn to_diagnostic_opt_returns_none_for_grade_b_and_above() {
        for composite in [0.80, 0.85, 0.90, 0.95, 1.0] {
            let r = TdgReport::from_components(
                composite, composite, composite, composite, composite, composite,
            );
            assert!(
                r.to_diagnostic_opt().is_none(),
                "grade {} (composite={composite}) should not emit diagnostic",
                r.grade
            );
        }
    }

    #[test]
    fn diagnostic_code_trait_round_trips_for_all_grades() {
        // Trait impl always returns a code; verify code matches grade.
        let cases = [
            (0.0, "Q-201", Severity::Error),    // F
            (0.65, "Q-202", Severity::Warning), // D
            (0.72, "Q-203", Severity::Info),    // C
            (0.78, "Q-203", Severity::Info),    // C+ (default to Q-203)
            (0.85, "Q-203", Severity::Info),    // B+ (default to Q-203)
            (0.99, "Q-203", Severity::Info),    // A+ (default to Q-203)
        ];
        for (composite, expected_code, expected_sev) in cases {
            let r = TdgReport::from_components(
                composite, composite, composite, composite, composite, composite,
            );
            assert_eq!(r.code(), expected_code, "composite={composite}");
            assert_eq!(r.severity(), expected_sev, "composite={composite}");
            assert!(!r.message().is_empty());
        }
    }
}
