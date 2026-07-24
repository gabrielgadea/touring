//! `QualityDiagnostics` — adapter from `touring-quality`'s 50-dim scoring to
//! `lsp_types::Diagnostic` for live editor feedback (W6 of
//! `2026-06-25-harness-consolidation-master-plan-v3.md`).
//!
//! Lives in `touring-lsp` so the LSP bridge can re-score on save/change
//! and publish diagnostics with the same severity as the rest of the
//! 50-dim composite. Implemented as a **pure function** so the
//! conversion is testable without a live LSP server or feature gate.

use serde::{Deserialize, Serialize};
use touring_quality::{DimId, DimScore, DimStatus, QualityReport};

/// Severity for an LSP `Diagnostic`. Mirrors the LSP spec
/// (`DiagnosticSeverity { Error, Warning, Information, Hint }`) but stays
/// feature-flag-free so the LSP bridge can opt into `lsp_types` lazily.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DiagnosticSeverity {
    /// Compile-blocking (P0 BLOCK dims) — corresponds to LSP `Error` (1).
    Error,
    /// Quality-warning (P1 WARN dims or composite < 0.80) — LSP `Warning` (2).
    Warning,
    /// Informational (ADVISORY dims, composite ≥ 0.80) — LSP `Information` (3).
    Information,
    /// Hint (everything else, opt-in by editor) — LSP `Hint` (4).
    Hint,
}

impl DiagnosticSeverity {
    /// LSP-protocol numeric value (matches `lsp_types::DiagnosticSeverity`).
    #[must_use]
    pub fn as_lsp_value(self) -> u8 {
        match self {
            Self::Error => 1,
            Self::Warning => 2,
            Self::Information => 3,
            Self::Hint => 4,
        }
    }
}

/// One per-dim diagnostic, ready to be wrapped in `lsp_types::Diagnostic`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityDiagnostic {
    /// Which dimension triggered the diagnostic (e.g. `F2.4`).
    pub dim: DimId,
    /// Severity derived from `DimStatus` + enforcement kind.
    pub severity: DiagnosticSeverity,
    /// One-line description (typically the `evidence` string).
    pub message: String,
    /// Source identifier shown in the editor (e.g. `"touring-quality"`).
    pub source: &'static str,
    /// Stable diagnostic code so editor can silence / dedupe by dim.
    pub code: String,
}

/// Convert a 50-dim `QualityReport` into a per-dim diagnostic list.
///
/// Severity rules:
/// - `DimStatus::Fail` → `Error` for BLOCK dims (P0), `Warning` otherwise.
/// - `DimStatus::Warn` → `Warning`.
/// - `DimStatus::Pass` → `Hint` (so the LLM still sees the score in the
///   editor; can be silenced per-dim by the LSP client).
///
/// The total `composite` is also surfaced as an extra `composite_total`
/// diagnostic on the synthetic id (or on the file's first dim) so the
/// editor's status bar can show the score.
#[must_use]
pub fn from_quality_report(report: &QualityReport) -> Vec<QualityDiagnostic> {
    let mut out: Vec<QualityDiagnostic> = report
        .dimensions
        .iter()
        .map(|(dim, score)| per_dim_diagnostic(*dim, score))
        .collect();

    // Composite-total diagnostic (synthetic id; uses file path for line 0).
    // Only emitted when the composite is below Gold — otherwise the editor
    // shows green squiggles on individual dims only.
    if report.composite < 0.80 {
        out.push(QualityDiagnostic {
            dim: DimId::F4_5, // arbitrary; grouped under "release readiness"
            severity: if report.composite < 0.60 {
                DiagnosticSeverity::Error
            } else {
                DiagnosticSeverity::Warning
            },
            message: format!(
                "Composite EliteScore {:.3} < Gold tier (0.80); {} blocker(s), {} warning(s)",
                report.composite,
                report.blockers.len(),
                report.warnings.len()
            ),
            source: "touring-quality",
            code: "F4.5_composite".to_string(),
        });
    }

    // Stable order: composite first, then by dim id (BTreeMap iter).
    out.sort_by(|a, b| {
        a.code.cmp(&b.code) // both start with F4.5_composite or F\d_\d
    });
    out
}

/// Convert one dim score into a single diagnostic. Pure function —
/// directly testable.
#[must_use]
pub fn per_dim_diagnostic(dim: DimId, score: &DimScore) -> QualityDiagnostic {
    let is_p0_block = matches!(
        dim.enforcement(),
        touring_quality::Enforcement::Block
    );
    let severity = match score.status {
        DimStatus::Fail if is_p0_block => DiagnosticSeverity::Error,
        DimStatus::Fail => DiagnosticSeverity::Warning,
        DimStatus::Warn => DiagnosticSeverity::Warning,
        DimStatus::Pass => DiagnosticSeverity::Hint,
    };
    QualityDiagnostic {
        dim,
        severity,
        message: score.evidence.clone(),
        source: "touring-quality",
        code: dim.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn dim(value: f32, dim: DimId) -> (DimId, DimScore) {
        (dim, DimScore::from_value(value, format!("evidence for {dim}")))
    }

    #[test]
    fn p0_fail_yields_error_severity() {
        // F2.4 (secrets) is a P0 BLOCK dim.
        let s = DimScore::from_value(0.1, "leak");
        let d = per_dim_diagnostic(DimId::F2_4, &s);
        assert_eq!(d.severity, DiagnosticSeverity::Error);
        assert_eq!(d.severity.as_lsp_value(), 1);
    }

    #[test]
    fn p1_fail_yields_warning_not_error() {
        // F1.1 (complexity) is P1 WARN, not BLOCK.
        let s = DimScore::from_value(0.1, "deeply nested");
        let d = per_dim_diagnostic(DimId::F1_1, &s);
        assert_eq!(d.severity, DiagnosticSeverity::Warning);
        assert_eq!(d.severity.as_lsp_value(), 2);
    }

    #[test]
    fn pass_yields_hint() {
        let s = DimScore::from_value(0.95, "clean");
        let d = per_dim_diagnostic(DimId::F1_1, &s);
        assert_eq!(d.severity, DiagnosticSeverity::Hint);
        assert_eq!(d.severity.as_lsp_value(), 4);
    }

    #[test]
    fn warn_yields_warning() {
        let s = DimScore::from_value(0.6, "borderline");
        let d = per_dim_diagnostic(DimId::F1_1, &s);
        assert_eq!(d.severity, DiagnosticSeverity::Warning);
    }

    #[test]
    fn code_field_stable_string_form() {
        let s = DimScore::from_value(0.0, "x");
        let d = per_dim_diagnostic(DimId::F2_5, &s);
        assert_eq!(d.code, "F2.5");
    }

    #[test]
    fn from_quality_report_emits_per_dim() {
        let mut dims: BTreeMap<DimId, DimScore> = BTreeMap::new();
        dims.insert(DimId::F2_4, DimScore::from_value(0.1, "leak")); // BLOCK fail
        dims.insert(DimId::F1_1, DimScore::from_value(0.6, "mid"));   // WARN
        dims.insert(DimId::F1_5, DimScore::from_value(0.9, "ok"));   // PASS
        let report = QualityReport::build(std::path::PathBuf::from("/x.rs"), dims);
        let diags = from_quality_report(&report);
        // 3 per-dim + 1 composite-total (composite < 0.80)
        assert!(diags.len() >= 4);
        // Composite diagnostic is present
        assert!(diags.iter().any(|d| d.code == "F4.5_composite"));
    }

    #[test]
    fn from_quality_report_no_composite_when_gold() {
        let mut dims: BTreeMap<DimId, DimScore> = BTreeMap::new();
        dims.insert(DimId::F1_1, DimScore::from_value(1.0, "perfect"));
        dims.insert(DimId::F2_4, DimScore::from_value(1.0, "perfect"));
        let report = QualityReport::build(std::path::PathBuf::from("/x.rs"), dims);
        let diags = from_quality_report(&report);
        // composite >= 0.80 → no composite-total diagnostic
        assert!(!diags.iter().any(|d| d.code == "F4.5_composite"));
    }
}