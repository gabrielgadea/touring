//! `WiringFinding` — diagnostic-aware wiring analysis findings.
//!
//! Wave Q4 (RFC-100): each variant maps to a stable code in the W- range
//! (`100..199`). Findings are emitted by orphan detection, integration
//! scoring, cross-feature dependency analysis, cycle detection, and the
//! wiring index staleness checker.
//!
//! See `~/.claude/rust/docs/touring/RFC-100-diagnostic-codes.md` §5 for
//! the canonical list.

use touring_foundation::diagnostic::{Diagnostic, DiagnosticCode, Severity, codes};

/// Structured wiring finding emitted by the analysis pipeline.
///
/// Each variant carries the minimum context required to render a
/// `Diagnostic` with file/line/help attached.
///
/// Note: `Eq` is intentionally not derived because `LowIntegration`
/// carries an `f64` score (which only implements `PartialEq`).
#[derive(Debug, Clone, PartialEq)]
pub enum WiringFinding {
    /// Public symbol with zero consumers (W-100).
    OrphanSymbol {
        /// Module file declaring the symbol.
        module_file: String,
        /// Symbol name.
        symbol: String,
    },
    /// Module integration score below the 1.0 threshold (W-101).
    LowIntegration {
        /// File path.
        file: String,
        /// Score in `[0.0, 1.0]`.
        score: f64,
    },
    /// Symbol depends on another crate behind a feature gate (W-102).
    CrossFeatureDependency {
        /// Symbol referencing the gated dependency.
        symbol: String,
        /// Feature flag name.
        feature: String,
    },
    /// Private symbol that could be promoted to `pub` based on usage (W-103).
    CouldBePublic {
        /// Symbol name candidate for promotion.
        symbol: String,
        /// File where the symbol lives.
        file: String,
    },
    /// Strongly-connected component detected — dependency cycle (W-110).
    DependencyCycle {
        /// Path through the cycle (modules / crates).
        path: Vec<String>,
        /// Depth (length of the path).
        depth: usize,
    },
    /// Wiring index is stale relative to source files on disk (W-120).
    StaleIndex {
        /// File path that disagrees with the index.
        file: String,
        /// Age of the cached entry, in seconds.
        age_seconds: u64,
    },
}

impl WiringFinding {
    /// Stable code (RFC-100 §5).
    #[must_use]
    pub fn code_str(&self) -> &'static str {
        match self {
            Self::OrphanSymbol { .. } => codes::W_100_ORPHAN_SYMBOL,
            Self::LowIntegration { .. } => codes::W_101_LOW_INTEGRATION,
            Self::CrossFeatureDependency { .. } => codes::W_102_CROSS_FEATURE_DEP,
            Self::CouldBePublic { .. } => codes::W_103_COULD_BE_PUBLIC,
            Self::DependencyCycle { .. } => codes::W_110_DEPENDENCY_CYCLE,
            Self::StaleIndex { .. } => codes::W_120_STALE_INDEX,
        }
    }

    /// Severity per RFC-100 §4.
    #[must_use]
    pub fn severity_class(&self) -> Severity {
        match self {
            // Orphans + low integration are warnings — code still compiles.
            Self::OrphanSymbol { .. }
            | Self::LowIntegration { .. }
            | Self::CrossFeatureDependency { .. }
            | Self::StaleIndex { .. } => Severity::Warning,
            // Cycle is an architectural error; could-be-public is a hint.
            Self::DependencyCycle { .. } => Severity::Error,
            Self::CouldBePublic { .. } => Severity::Hint,
        }
    }

    /// Emit a `Diagnostic` for this finding, or `None` if the finding has no
    /// actionable diagnostic payload.
    ///
    /// Currently only `LowIntegration` emits W-101; other variants delegate to
    /// `to_diagnostic()` when a diagnostic is needed.
    #[must_use]
    pub fn emit(&self) -> Option<Diagnostic> {
        match self {
            Self::LowIntegration { file, score } => Some(
                Diagnostic::new(
                    codes::W_101_LOW_INTEGRATION,
                    Severity::Warning,
                    format!(
                        "Module integration score {:.2} below threshold 1.0 for: {}",
                        score, file
                    )
                    .into(),
                )
                .with_file(file)
                .with_help("review module purpose and add consumers"),
            ),
            // All other variants use the standard to_diagnostic path.
            other => Some(other.to_diagnostic()),
        }
    }
}

impl DiagnosticCode for WiringFinding {
    fn code(&self) -> &'static str {
        self.code_str()
    }

    fn severity(&self) -> Severity {
        self.severity_class()
    }

    fn message(&self) -> String {
        match self {
            Self::OrphanSymbol {
                module_file,
                symbol,
            } => {
                format!("orphan symbol `{symbol}` in `{module_file}` has zero consumers")
            }
            Self::LowIntegration { file, score } => {
                format!("integration score {score:.2} below 1.0 threshold for `{file}`")
            }
            Self::CrossFeatureDependency { symbol, feature } => {
                format!("symbol `{symbol}` depends on feature-gated `{feature}`")
            }
            Self::CouldBePublic { symbol, file } => {
                format!("private symbol `{symbol}` in `{file}` is candidate for `pub` promotion")
            }
            Self::DependencyCycle { path, depth } => {
                format!("dependency cycle of depth {depth}: {}", path.join(" -> "))
            }
            Self::StaleIndex { file, age_seconds } => {
                format!("wiring index stale for `{file}` ({age_seconds}s old)")
            }
        }
    }

    /// Override default to attach file + help context.
    fn to_diagnostic(&self) -> Diagnostic {
        let base = Diagnostic::new(self.code(), self.severity(), self.message());
        match self {
            Self::OrphanSymbol { module_file, .. } => base
                .with_file(module_file)
                .with_help("wire the symbol to a consumer or remove it (REGRA #0 POTENCIALIZAR)"),
            Self::LowIntegration { file, .. } => base
                .with_file(file)
                .with_help("review module purpose and add consumers"),
            Self::CrossFeatureDependency { .. } => {
                base.with_help("ensure consumer crate enables the feature in `Cargo.toml`")
            }
            Self::CouldBePublic { file, .. } => base
                .with_file(file)
                .with_help("promote to `pub` if used cross-module, or remove if truly private"),
            Self::DependencyCycle { .. } => base.with_help(
                "break the cycle by extracting a shared trait or moving code to a leaf crate",
            ),
            Self::StaleIndex { file, .. } => base
                .with_file(file)
                .with_help("run `touring index rebuild` to refresh"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emit_low_integration_produces_w101_diagnostic() {
        let finding = WiringFinding::LowIntegration {
            file: "src/foo.rs".to_string(),
            score: 0.42,
        };
        let diagnostic = finding.emit();
        let d = diagnostic.expect("LowIntegration must emit Some");
        assert_eq!(d.code.as_str(), "W-101");
        assert!(matches!(d.severity, Severity::Warning));
        assert!(d.file.as_ref().is_some());
    }

    #[test]
    fn emit_orphan_symbol_delegates_to_to_diagnostic() {
        let finding = WiringFinding::OrphanSymbol {
            module_file: "lib.rs".to_string(),
            symbol: "Foo".to_string(),
        };
        let diagnostic = finding.emit();
        assert!(diagnostic.is_some());
        let d = diagnostic.expect("emit returned Some");
        assert_eq!(d.code.as_str(), "W-100");
    }

    #[test]
    fn orphan_maps_to_w100() {
        let f = WiringFinding::OrphanSymbol {
            module_file: "lib.rs".to_string(),
            symbol: "Foo".to_string(),
        };
        assert_eq!(f.code(), "W-100");
        assert_eq!(f.severity(), Severity::Warning);
    }

    #[test]
    fn low_integration_maps_to_w101() {
        let f = WiringFinding::LowIntegration {
            file: "src/foo.rs".to_string(),
            score: 0.42,
        };
        assert_eq!(f.code(), "W-101");
        assert!(f.message().contains("0.42"));
    }

    #[test]
    fn cross_feature_maps_to_w102() {
        let f = WiringFinding::CrossFeatureDependency {
            symbol: "do_x".to_string(),
            feature: "simd".to_string(),
        };
        assert_eq!(f.code(), "W-102");
    }

    #[test]
    fn could_be_public_maps_to_w103_hint() {
        let f = WiringFinding::CouldBePublic {
            symbol: "internal".to_string(),
            file: "src/lib.rs".to_string(),
        };
        assert_eq!(f.code(), "W-103");
        assert_eq!(f.severity(), Severity::Hint);
    }

    #[test]
    fn cycle_maps_to_w110_error() {
        let f = WiringFinding::DependencyCycle {
            path: vec!["a".to_string(), "b".to_string(), "a".to_string()],
            depth: 3,
        };
        assert_eq!(f.code(), "W-110");
        assert_eq!(f.severity(), Severity::Error);
        assert!(f.message().contains("a -> b -> a"));
    }

    #[test]
    fn stale_index_maps_to_w120() {
        let f = WiringFinding::StaleIndex {
            file: "src/foo.rs".to_string(),
            age_seconds: 300,
        };
        assert_eq!(f.code(), "W-120");
    }

    #[test]
    fn diagnostic_attaches_file_and_help() {
        let f = WiringFinding::OrphanSymbol {
            module_file: "lib.rs".to_string(),
            symbol: "Foo".to_string(),
        };
        let d = f.to_diagnostic();
        assert_eq!(d.file.as_deref(), Some("lib.rs"));
        assert!(d.help.is_some());
    }

    #[test]
    fn json_serialisation_round_trips() {
        let f = WiringFinding::DependencyCycle {
            path: vec!["x".into(), "y".into()],
            depth: 2,
        };
        let d = f.to_diagnostic();
        let json = serde_json::to_string(&d).unwrap_or_default();
        assert!(json.contains("\"code\":\"W-110\""), "json: {json}");
        assert!(json.contains("\"severity\":\"error\""), "json: {json}");
    }

    #[test]
    fn all_variants_emit_valid_codes() {
        let variants = [
            WiringFinding::OrphanSymbol {
                module_file: "a".into(),
                symbol: "b".into(),
            },
            WiringFinding::LowIntegration {
                file: "f".into(),
                score: 0.5,
            },
            WiringFinding::CrossFeatureDependency {
                symbol: "s".into(),
                feature: "g".into(),
            },
            WiringFinding::CouldBePublic {
                symbol: "p".into(),
                file: "f".into(),
            },
            WiringFinding::DependencyCycle {
                path: vec!["a".into()],
                depth: 1,
            },
            WiringFinding::StaleIndex {
                file: "f".into(),
                age_seconds: 1,
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
