//! QualityFinding — RFC-100 Q-2xx diagnostic codes for code quality signals.
//!
//! Wave Preditiva (RFC-100): these variants map to Q-200..299 codes and are
//! emitted by the quality analysis pipeline when quality signals cross
//! configurable thresholds.

use touring_foundation::diagnostic::{Diagnostic, DiagnosticCode, Severity, codes};

/// Structured quality finding — maps to RFC-100 Q-2xx codes.
///
/// Unlike `BlastWarning` (which handles blast radius signals), `QualityFinding`
/// handles code quality signals: antipattern density, cyclomatic complexity, etc.
#[derive(Debug, Clone, PartialEq)]
pub enum QualityFinding {
    /// High density of antipattern statements in the file (Q-230).
    HighAntipatternDensity {
        /// File path under analysis.
        file: String,
        /// Ratio of antipattern statements to total statements (0.0–1.0).
        antipattern_rate: f64,
        /// Threshold that was exceeded (typically 0.3 = 30%).
        threshold: f64,
    },
    /// High cyclomatic complexity in one or more symbols (Q-240).
    HighCyclomatic {
        /// File path under analysis.
        file: String,
        /// Maximum cyclomatic complexity found in the file.
        cyclomatic_complexity: usize,
        /// Threshold that was exceeded (typically 20).
        threshold: usize,
    },
}

impl QualityFinding {
    /// Stable code (RFC-100 §5).
    #[must_use]
    pub fn code_str(&self) -> &'static str {
        match self {
            Self::HighAntipatternDensity { .. } => codes::Q_230_HIGH_ANTIPATTERN_DENSITY,
            Self::HighCyclomatic { .. } => codes::Q_240_HIGH_CYCLOMATIC,
        }
    }

    /// Severity per RFC-100 §4.
    #[must_use]
    pub fn severity_class(&self) -> Severity {
        match self {
            // Both quality findings are warnings — they signal code debt.
            Self::HighAntipatternDensity { .. } | Self::HighCyclomatic { .. } => Severity::Warning,
        }
    }
}

impl DiagnosticCode for QualityFinding {
    fn code(&self) -> &'static str {
        self.code_str()
    }

    fn severity(&self) -> Severity {
        self.severity_class()
    }

    fn message(&self) -> String {
        match self {
            Self::HighAntipatternDensity {
                file,
                antipattern_rate,
                threshold,
            } => format!(
                "`{file}`: antipattern rate {:.1}% exceeds threshold {:.1}%",
                antipattern_rate * 100.0,
                threshold * 100.0
            ),
            Self::HighCyclomatic {
                file,
                cyclomatic_complexity,
                threshold,
            } => format!(
                "`{file}`: cyclomatic complexity {cyclomatic_complexity} exceeds threshold {threshold}",
            ),
        }
    }

    fn to_diagnostic(&self) -> Diagnostic {
        let base = Diagnostic::new(self.code(), self.severity(), self.message());
        match self {
            Self::HighAntipatternDensity { .. } => base.with_help(
                "run `touring analyze antipatterns <file>` and address high-density regions",
            ),
            Self::HighCyclomatic { .. } => base
                .with_help("run `touring analyze complexity <file>` and decompose high-CC symbols"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn high_antipattern_density_maps_to_q230() {
        let f = QualityFinding::HighAntipatternDensity {
            file: "src/lib.rs".into(),
            antipattern_rate: 0.45,
            threshold: 0.3,
        };
        assert_eq!(f.code(), "Q-230");
        assert_eq!(f.severity(), Severity::Warning);
        assert!(f.message().contains("45"));
    }

    #[test]
    fn high_cyclomatic_maps_to_q240() {
        let f = QualityFinding::HighCyclomatic {
            file: "src/lib.rs".into(),
            cyclomatic_complexity: 25,
            threshold: 20,
        };
        assert_eq!(f.code(), "Q-240");
        assert_eq!(f.severity(), Severity::Warning);
        assert!(f.message().contains("25"));
    }

    #[test]
    fn diagnostic_attaches_help_for_both_variants() {
        let ap = QualityFinding::HighAntipatternDensity {
            file: "src/foo.rs".into(),
            antipattern_rate: 0.5,
            threshold: 0.3,
        };
        let d = ap.to_diagnostic();
        assert!(d.help.is_some());

        let cc = QualityFinding::HighCyclomatic {
            file: "src/foo.rs".into(),
            cyclomatic_complexity: 30,
            threshold: 20,
        };
        let d = cc.to_diagnostic();
        assert!(d.help.is_some());
    }

    #[test]
    fn all_variants_emit_valid_codes() {
        let variants = [
            QualityFinding::HighAntipatternDensity {
                file: "f".into(),
                antipattern_rate: 0.35,
                threshold: 0.3,
            },
            QualityFinding::HighCyclomatic {
                file: "f".into(),
                cyclomatic_complexity: 25,
                threshold: 20,
            },
        ];
        for v in variants {
            assert!(
                Diagnostic::is_valid_code(v.code()),
                "invalid code: {}",
                v.code()
            );
        }
    }
}
