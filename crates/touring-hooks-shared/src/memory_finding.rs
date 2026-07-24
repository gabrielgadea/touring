//! `MemoryFinding` — diagnostic-aware memory recall classification.
//!
//! Wave Q4 (RFC-100): each variant maps to a code in the M- range
//! (`500..599`). Emitted by `cli_memory_recall` and the planned TF-IDF /
//! RRF fusion pipeline (Wave M1+M2).
//!
//! Memory recall is a query, not a fallible operation, so these findings
//! classify the *outcome* of a recall (empty / activated / fused / stale)
//! rather than wrapping a Result error.

use touring_foundation::diagnostic::{Diagnostic, DiagnosticCode, Severity, codes};

/// Outcome classification for a memory recall query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MemoryFinding {
    /// Recall returned zero rows for the query (M-500).
    RecallEmpty {
        /// The recall query string.
        query: String,
    },
    /// TF-IDF retriever activated as supplementary signal (M-510, Wave M1).
    TfidfActivated {
        /// Number of TF-IDF candidates surfaced.
        candidate_count: usize,
        /// Number of git log entries searched.
        corpus_size: usize,
    },
    /// RRF (Reciprocal Rank Fusion) merged multiple result sets (M-520, Wave M2).
    RrfFusion {
        /// Number of underlying result sets fused.
        source_count: usize,
        /// Number of merged hits in the final ranking.
        merged_count: usize,
    },
    /// Recall hit a stale entry beyond the freshness threshold (M-530).
    StaleThreshold {
        /// Memory key affected.
        key: String,
        /// Age of the entry in days.
        age_days: u64,
    },
}

impl MemoryFinding {
    /// Stable code (RFC-100 §5).
    #[must_use]
    pub fn code_str(&self) -> &'static str {
        match self {
            Self::RecallEmpty { .. } => codes::M_500_RECALL_EMPTY,
            Self::TfidfActivated { .. } => codes::M_510_TFIDF_ACTIVATED,
            Self::RrfFusion { .. } => codes::M_520_RRF_FUSION,
            Self::StaleThreshold { .. } => codes::M_530_STALE_THRESHOLD,
        }
    }

    /// Severity per RFC-100 §4.
    #[must_use]
    pub fn severity_class(&self) -> Severity {
        match self {
            // Empty recall is informational — caller proceeds without context.
            Self::RecallEmpty { .. } => Severity::Info,
            // Activations / fusions are hints — we surface that the path improved.
            Self::TfidfActivated { .. } | Self::RrfFusion { .. } => Severity::Hint,
            // Stale entries are warnings — caller should refresh.
            Self::StaleThreshold { .. } => Severity::Warning,
        }
    }
}

impl DiagnosticCode for MemoryFinding {
    fn code(&self) -> &'static str {
        self.code_str()
    }

    fn severity(&self) -> Severity {
        self.severity_class()
    }

    fn message(&self) -> String {
        match self {
            Self::RecallEmpty { query } => {
                format!("memory recall returned zero rows for query `{query}`")
            }
            Self::TfidfActivated {
                candidate_count,
                corpus_size,
            } => format!(
                "TF-IDF activated: {candidate_count} candidates surfaced from corpus of {corpus_size} entries"
            ),
            Self::RrfFusion {
                source_count,
                merged_count,
            } => format!(
                "RRF fusion merged {source_count} result sets into {merged_count} ranked hits"
            ),
            Self::StaleThreshold { key, age_days } => {
                format!("memory entry `{key}` is {age_days} day(s) old — refresh recommended")
            }
        }
    }

    fn to_diagnostic(&self) -> Diagnostic {
        let base = Diagnostic::new(self.code(), self.severity(), self.message());
        match self {
            Self::RecallEmpty { .. } => base
                .with_help("consider broader query terms or run `touring memory list --limit 20`"),
            Self::TfidfActivated { .. } => {
                base.with_help("Wave M1 retriever — see RFC-100 §5 for activation criteria")
            }
            Self::RrfFusion { .. } => {
                base.with_help("Wave M2 fusion — combines SQL + ANN + TF-IDF rankings")
            }
            Self::StaleThreshold { .. } => base
                .with_help("update via `touring memory store <key> <new_value> --tier semantic`"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_recall_maps_to_m500_info() {
        let f = MemoryFinding::RecallEmpty {
            query: "nonexistent".to_string(),
        };
        assert_eq!(f.code(), "M-500");
        assert_eq!(f.severity(), Severity::Info);
        assert!(f.message().contains("nonexistent"));
    }

    #[test]
    fn tfidf_activated_maps_to_m510_hint() {
        let f = MemoryFinding::TfidfActivated {
            candidate_count: 7,
            corpus_size: 300,
        };
        assert_eq!(f.code(), "M-510");
        assert_eq!(f.severity(), Severity::Hint);
        assert!(f.message().contains("7 candidates"));
    }

    #[test]
    fn rrf_fusion_maps_to_m520() {
        let f = MemoryFinding::RrfFusion {
            source_count: 3,
            merged_count: 25,
        };
        assert_eq!(f.code(), "M-520");
        assert!(f.message().contains("3 result sets"));
        assert!(f.message().contains("25 ranked"));
    }

    #[test]
    fn stale_threshold_maps_to_m530_warning() {
        let f = MemoryFinding::StaleThreshold {
            key: "lesson:foo".to_string(),
            age_days: 90,
        };
        assert_eq!(f.code(), "M-530");
        assert_eq!(f.severity(), Severity::Warning);
        assert!(f.message().contains("90 day"));
    }

    #[test]
    fn diagnostic_carries_help_text() {
        let f = MemoryFinding::RecallEmpty {
            query: "x".to_string(),
        };
        let d = f.to_diagnostic();
        assert!(d.help.is_some());
        assert!(
            d.help.as_deref().unwrap_or("").contains("touring memory"),
            "help should reference touring memory"
        );
    }

    #[test]
    fn json_round_trip_preserves_code_and_severity() {
        let f = MemoryFinding::RrfFusion {
            source_count: 2,
            merged_count: 10,
        };
        let d = f.to_diagnostic();
        let json = serde_json::to_string(&d).unwrap_or_default();
        assert!(json.contains("\"code\":\"M-520\""), "json: {json}");
        assert!(json.contains("\"severity\":\"hint\""), "json: {json}");
    }

    #[test]
    fn all_variants_emit_valid_codes() {
        let variants = [
            MemoryFinding::RecallEmpty {
                query: "q".to_string(),
            },
            MemoryFinding::TfidfActivated {
                candidate_count: 0,
                corpus_size: 0,
            },
            MemoryFinding::RrfFusion {
                source_count: 0,
                merged_count: 0,
            },
            MemoryFinding::StaleThreshold {
                key: "k".to_string(),
                age_days: 0,
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
