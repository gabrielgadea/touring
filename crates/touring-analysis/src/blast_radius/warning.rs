//! `BlastWarning` — diagnostic-aware blast radius warnings.
//!
//! Wave Q4 (RFC-100): each variant maps to a code in the B- range
//! (`300..399`). Emitted by the blast radius engine when a symbol's
//! impact crosses configurable thresholds, by the predictive blast
//! injection path, and by cross-feature blast analysis.

use touring_foundation::diagnostic::{Diagnostic, DiagnosticCode, Severity, codes};

/// Structured blast radius warning.
///
/// Note: `Eq` is intentionally not derived because `RefactorRequired`
/// carries an `f64` quality score (which only implements `PartialEq`).
#[derive(Debug, Clone, PartialEq)]
pub enum BlastWarning {
    /// Symbol blast radius exceeds the warning threshold (B-300).
    HighBlast {
        /// Symbol name.
        symbol: String,
        /// Number of files transitively impacted.
        affected_files: usize,
        /// Threshold that was exceeded (typically 10).
        threshold: usize,
    },
    /// Refactor required before edit — combined high blast + low quality (B-301).
    RefactorRequired {
        /// File path under analysis.
        file: String,
        /// Quality score in `[0.0, 1.0]` (low).
        quality_score: f64,
        /// Blast radius (high).
        blast_radius: usize,
    },
    /// Mpatch fuzzy patch expanded code with low confidence (B-302).
    ///
    /// Emitted by the pre_write hook when `mpatch-fuzzy` is enabled and a
    /// dry-run patch preview shows both:
    /// 1. Code expansion (`new_complexity > old_complexity` in bytes), AND
    /// 2. Low fuzzy match confidence (typically `< 0.7`).
    ///
    /// Signals that the patch may need manual review before commit.
    PatchExpansion {
        /// File being patched.
        file: String,
        /// Bytes added by the patch (`new - old`).
        delta_bytes: f64,
        /// Mpatch apply-report confidence in `[0.0, 1.0]`.
        confidence: f32,
    },
    /// Predictive blast injection mutated the input context (B-310).
    BlastInjection {
        /// PascalCase symbols extracted from the subject.
        symbols: Vec<String>,
        /// Number of modules crossed.
        module_count: usize,
    },
    /// Blast radius crosses feature boundaries (B-320).
    CrossFeatureBlast {
        /// Feature flag(s) crossed.
        features: Vec<String>,
        /// Number of gated symbols affected.
        gated_symbol_count: usize,
    },
}

impl BlastWarning {
    /// Stable code (RFC-100 §5).
    #[must_use]
    pub fn code_str(&self) -> &'static str {
        match self {
            Self::HighBlast { .. } => codes::B_300_HIGH_BLAST,
            Self::RefactorRequired { .. } => codes::B_301_REFACTOR_REQUIRED,
            Self::PatchExpansion { .. } => codes::B_302_PATCH_EXPANSION,
            Self::BlastInjection { .. } => codes::B_310_BLAST_INJECTION,
            Self::CrossFeatureBlast { .. } => codes::B_320_CROSS_FEATURE_BLAST,
        }
    }

    /// Severity per RFC-100 §4.
    #[must_use]
    pub fn severity_class(&self) -> Severity {
        match self {
            // RefactorRequired is an error — quality + blast both bad.
            Self::RefactorRequired { .. } => Severity::Error,
            // HighBlast, CrossFeatureBlast, and PatchExpansion warn the user
            // to check carefully but do not block the operation.
            Self::HighBlast { .. }
            | Self::CrossFeatureBlast { .. }
            | Self::PatchExpansion { .. } => Severity::Warning,
            // BlastInjection is informational — predictive layer is helping.
            Self::BlastInjection { .. } => Severity::Info,
        }
    }
}

impl DiagnosticCode for BlastWarning {
    fn code(&self) -> &'static str {
        self.code_str()
    }

    fn severity(&self) -> Severity {
        self.severity_class()
    }

    fn message(&self) -> String {
        match self {
            Self::HighBlast {
                symbol,
                affected_files,
                threshold,
            } => {
                format!("symbol `{symbol}` impacts {affected_files} files (threshold {threshold})")
            }
            Self::RefactorRequired {
                file,
                quality_score,
                blast_radius,
            } => format!(
                "`{file}`: quality {quality_score:.2} + blast {blast_radius} require refactor before edit"
            ),
            Self::PatchExpansion {
                file,
                delta_bytes,
                confidence,
            } => format!(
                "`{file}`: mpatch expanded by {delta_bytes:.0} bytes with low confidence {confidence:.2} — review before commit"
            ),
            Self::BlastInjection {
                symbols,
                module_count,
            } => format!(
                "predictive blast injected {n} symbols across {module_count} modules: [{joined}]",
                n = symbols.len(),
                joined = symbols.join(", ")
            ),
            Self::CrossFeatureBlast {
                features,
                gated_symbol_count,
            } => format!(
                "blast crosses {n} feature(s) [{joined}], affecting {gated_symbol_count} gated symbol(s)",
                n = features.len(),
                joined = features.join(", ")
            ),
        }
    }

    fn to_diagnostic(&self) -> Diagnostic {
        let base = Diagnostic::new(self.code(), self.severity(), self.message());
        match self {
            Self::HighBlast { .. } => base.with_help(
                "consider splitting the symbol or routing edits via touring-architect agent",
            ),
            Self::RefactorRequired { file, .. } => base
                .with_file(file)
                .with_help("run `touring ast tdg <file>` and address grade D/F findings first"),
            Self::PatchExpansion { file, .. } => base
                .with_file(file)
                .with_help("inspect the patched file via `touring ast meta <file>` and re-run with --no-fuzzy if behavior diverges"),
            Self::BlastInjection { .. } => base
                .with_help("review the symbols injected into context before applying edits"),
            Self::CrossFeatureBlast { .. } => base
                .with_help("validate all crossed features compile via `cargo check --features <list>`"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn high_blast_maps_to_b300() {
        let w = BlastWarning::HighBlast {
            symbol: "HookRuntime".to_string(),
            affected_files: 68,
            threshold: 10,
        };
        assert_eq!(w.code(), "B-300");
        assert_eq!(w.severity(), Severity::Warning);
        assert!(w.message().contains("68"));
    }

    #[test]
    fn refactor_required_maps_to_b301_error() {
        let w = BlastWarning::RefactorRequired {
            file: "src/big.rs".to_string(),
            quality_score: 0.3,
            blast_radius: 25,
        };
        assert_eq!(w.code(), "B-301");
        assert_eq!(w.severity(), Severity::Error);
    }

    #[test]
    fn patch_expansion_maps_to_b302_warning() {
        let w = BlastWarning::PatchExpansion {
            file: "src/foo.rs".to_string(),
            delta_bytes: 320.0,
            confidence: 0.55,
        };
        assert_eq!(w.code(), "B-302", "must use RFC-100 code B-302");
        assert_eq!(
            w.severity(),
            Severity::Warning,
            "B-302 must be Warning (not Error — patch is viable)"
        );
        let msg = w.message();
        assert!(
            msg.contains("320"),
            "message must include delta bytes: {msg}"
        );
        assert!(
            msg.contains("0.55"),
            "message must include confidence: {msg}"
        );
    }

    #[test]
    fn patch_expansion_diagnostic_attaches_file_and_help() {
        let w = BlastWarning::PatchExpansion {
            file: "src/bar.rs".to_string(),
            delta_bytes: 80.0,
            confidence: 0.4,
        };
        let d = w.to_diagnostic();
        assert_eq!(d.file.as_deref(), Some("src/bar.rs"));
        assert!(d.help.is_some(), "B-302 must include actionable help");
        let help = d.help.unwrap_or_default();
        assert!(
            help.contains("touring ast meta") || help.contains("--no-fuzzy"),
            "help must guide the operator: {help}"
        );
    }

    #[test]
    fn patch_expansion_negative_delta_still_emits_b302_when_constructed() {
        // The variant itself does not enforce the gate predicate — that lives
        // in `emit_b302_if_low_confidence_expansion`. The variant only encodes
        // the data, so constructing with negative delta still maps to B-302.
        let w = BlastWarning::PatchExpansion {
            file: "src/baz.rs".to_string(),
            delta_bytes: -40.0,
            confidence: 0.9,
        };
        assert_eq!(w.code(), "B-302");
        assert_eq!(w.severity(), Severity::Warning);
    }

    #[test]
    fn blast_injection_maps_to_b310_info() {
        let w = BlastWarning::BlastInjection {
            symbols: vec!["Foo".into(), "Bar".into()],
            module_count: 4,
        };
        assert_eq!(w.code(), "B-310");
        assert_eq!(w.severity(), Severity::Info);
        assert!(w.message().contains("Foo, Bar"));
    }

    #[test]
    fn cross_feature_maps_to_b320() {
        let w = BlastWarning::CrossFeatureBlast {
            features: vec!["simd".into(), "gpu".into()],
            gated_symbol_count: 7,
        };
        assert_eq!(w.code(), "B-320");
        assert!(w.message().contains("simd, gpu"));
    }

    #[test]
    fn diagnostic_attaches_file_for_refactor_required() {
        let w = BlastWarning::RefactorRequired {
            file: "src/foo.rs".to_string(),
            quality_score: 0.2,
            blast_radius: 50,
        };
        let d = w.to_diagnostic();
        assert_eq!(d.file.as_deref(), Some("src/foo.rs"));
        assert!(d.help.is_some());
    }

    #[test]
    fn json_round_trip_preserves_code_and_severity() {
        let w = BlastWarning::CrossFeatureBlast {
            features: vec!["x".into()],
            gated_symbol_count: 1,
        };
        let d = w.to_diagnostic();
        let json = serde_json::to_string(&d).unwrap_or_default();
        assert!(json.contains("\"code\":\"B-320\""), "json: {json}");
        assert!(json.contains("\"severity\":\"warning\""), "json: {json}");
    }

    #[test]
    fn all_variants_emit_valid_codes() {
        let variants = [
            BlastWarning::HighBlast {
                symbol: "x".into(),
                affected_files: 10,
                threshold: 5,
            },
            BlastWarning::RefactorRequired {
                file: "f".into(),
                quality_score: 0.0,
                blast_radius: 1,
            },
            BlastWarning::PatchExpansion {
                file: "f".into(),
                delta_bytes: 0.0,
                confidence: 0.0,
            },
            BlastWarning::BlastInjection {
                symbols: vec![],
                module_count: 0,
            },
            BlastWarning::CrossFeatureBlast {
                features: vec![],
                gated_symbol_count: 0,
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
