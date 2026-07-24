//! Semantic embeddings integration for touring-cortex.

use crate::similarity::{CosineComputer, CosineSimilarity, TopKResult, TopKSearcher};
use crate::statistics::reconciliation::{ReconciliationQuality, reconcile_with_wilson};
use crate::statistics::{DriftDetection, DriftDetector};
use rayon::prelude::*;

// ============================================================================
// MetacognitivePipeline — Evidence fusion and drift detection
// ============================================================================

/// Evidence from a single source.
#[derive(Debug, Clone)]
pub struct Evidence {
    /// Numeric identifier for the source of this evidence.
    pub source_id: usize,
    /// Observed value from this source.
    pub value: f64,
    /// Confidence score in [0, 1] for this evidence.
    pub confidence: f64,
    /// Number of successful observations.
    pub successes: u32,
    /// Total number of observations.
    pub total: u32,
}

/// Result of resolving multiple evidence sources into a fused estimate.
#[derive(Debug, Clone)]
pub struct Resolved {
    /// The reconciled value after fusing all evidence.
    pub fused_value: f64,
    /// Quality metadata from the reconciliation process.
    pub quality: ReconciliationQuality,
    /// Wilson lower-bound score for the highest-confidence source.
    pub wilson_bound: f64,
}

/// Drift report between two pipeline runs.
#[derive(Debug, Clone)]
pub struct DriftReport {
    /// True when drift exceeds configured thresholds.
    pub drift_detected: bool,
    /// Kolmogorov-Smirnov statistic between previous and current value distributions.
    pub ks_statistic: f64,
    /// Jensen-Shannon divergence between confidence distributions.
    pub js_divergence: f64,
    /// Severity classification of detected drift.
    pub severity: DriftSeverity,
}

/// Severity level for concept drift.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriftSeverity {
    /// No drift detected.
    None,
    /// Minor drift — marginal threshold exceeded.
    Low,
    /// Moderate drift — warning threshold exceeded.
    Medium,
    /// Significant drift — critical threshold exceeded.
    High,
}

/// Unified metacognitive pipeline for evidence fusion and concept-drift detection.
///
/// Orchestrates [`reconcile_with_wilson`] for evidence fusion and
/// [`DriftDetector`] for drift detection into a single coherent system.
///
/// **Note:** Not to be confused with `touring_learning::metacognitive_pipeline::LatencyAdaptationPipeline`
/// which handles latency adaptation via CUSUM drift detection, ACO pheromone threshold adaptation,
/// and Actor-Critic RL for hook execution parallelism tuning. This pipeline handles evidence
/// fusion via `DriftDetector` + Wilson scoring.
#[derive(Debug, Clone)]
pub struct MetacognitivePipeline {
    /// Confidence level passed to Wilson score lower-bound computation.
    wilson_confidence: f64,
    /// Conflict threshold (coefficient of variation) for triggering Wilson weighting.
    conflict_threshold: f64,
    /// KS statistic threshold for drift detection.
    drift_ks_threshold: f64,
    /// JS divergence threshold for drift detection.
    drift_js_threshold: f64,
    /// Drift detector instance.
    drift_detector: DriftDetector,
    /// Previous resolved values for drift comparison.
    prev_resolved: Vec<Resolved>,
}

impl Default for MetacognitivePipeline {
    fn default() -> Self {
        Self::new()
    }
}

impl MetacognitivePipeline {
    /// Create a new pipeline with default thresholds.
    pub fn new() -> Self {
        Self {
            wilson_confidence: 0.95,
            conflict_threshold: 0.3,
            drift_ks_threshold: 0.15,
            drift_js_threshold: 0.1,
            drift_detector: DriftDetector::new(),
            prev_resolved: Vec::new(),
        }
    }

    /// Resolve multiple evidence sources into a single fused result.
    ///
    /// Calls [`reconcile_with_wilson`] with the evidence values and confidences.
    /// Returns a [`Resolved`] containing the fused value, quality metadata, and
    /// the maximum Wilson lower-bound across all sources.
    #[must_use]
    pub fn resolve(&self, evidence: &[Evidence]) -> Resolved {
        if evidence.is_empty() {
            return Resolved {
                fused_value: 0.0,
                quality: ReconciliationQuality {
                    conflict_detected: false,
                    max_wilson_bound: 0.0,
                    coefficient_of_variation: 0.0,
                },
                wilson_bound: 0.0,
            };
        }

        let values: Vec<f64> = evidence.iter().map(|e| e.value).collect();
        let confidences: Vec<f64> = evidence.iter().map(|e| e.confidence).collect();

        let (fused_value, _fused_conf, mut quality) =
            reconcile_with_wilson(&values, &confidences, self.wilson_confidence);

        // EC64: Apply pipeline-specific conflict_threshold to recompute conflict_detected.
        // This overrides the internal CONFLICT_CV_THRESHOLD from reconcile_with_wilson,
        // giving callers control over conflict sensitivity (field E5-S7: conflict_threshold).
        quality.conflict_detected = quality.coefficient_of_variation > self.conflict_threshold;

        let wilson_bound = quality.max_wilson_bound;

        Resolved {
            fused_value,
            quality,
            wilson_bound,
        }
    }

    /// Detect concept drift between a previous resolved batch and new evidence.
    ///
    /// Compares the fused values of the previous batch against the raw values
    /// of the current evidence using KS statistic, and compares confidence
    /// distributions using JS divergence.
    ///
    /// Returns `DriftReport` with severity classification.
    #[must_use]
    pub fn detect_drift(&self, prev: &[Resolved], curr: &[Evidence]) -> DriftReport {
        if prev.is_empty() {
            return DriftReport {
                drift_detected: false,
                ks_statistic: 0.0,
                js_divergence: 0.0,
                severity: DriftSeverity::None,
            };
        }

        let prev_values: Vec<f64> = prev.iter().map(|r| r.fused_value).collect();
        let curr_values: Vec<f64> = curr.iter().map(|e| e.value).collect();

        let ks = self.drift_detector.ks_statistic(&prev_values, &curr_values);

        // Build confidence distributions of same length for JS divergence
        let prev_conf: Vec<f64> = prev.iter().map(|r| r.wilson_bound).collect();
        let curr_conf: Vec<f64> = curr
            .iter()
            .map(|e| {
                let total = if e.total == 0 { 1 } else { e.total };
                e.successes as f64 / total as f64
            })
            .collect();

        let js = if prev_conf.len() == curr_conf.len() && !prev_conf.is_empty() {
            self.drift_detector.js_divergence(&prev_conf, &curr_conf)
        } else {
            0.0
        };

        let drift_detected = ks > self.drift_ks_threshold || js > self.drift_js_threshold;

        let severity = if ks > 0.3 || js > 0.25 {
            DriftSeverity::High
        } else if ks > 0.15 || js > 0.1 {
            DriftSeverity::Medium
        } else if ks > 0.05 || js > 0.05 {
            DriftSeverity::Low
        } else {
            DriftSeverity::None
        };

        DriftReport {
            drift_detected,
            ks_statistic: ks,
            js_divergence: js,
            severity,
        }
    }

    /// Process a batch of evidence through the full pipeline.
    ///
    /// Resolves all evidence and, if a previous batch exists, computes a drift
    /// report by comparing against the stored previous results.
    ///
    /// Returns a tuple of `(resolved results, optional drift report)`.
    #[must_use]
    pub fn process_batch(&self, batch: &[Evidence]) -> (Vec<Resolved>, Option<DriftReport>) {
        let resolved: Vec<Resolved> = batch
            .iter()
            .map(|e| self.resolve(std::slice::from_ref(e)))
            .collect();

        let drift_report = if self.prev_resolved.is_empty() {
            None
        } else {
            Some(self.detect_drift(&self.prev_resolved, batch))
        };

        (resolved, drift_report)
    }
}

/// Compute embedding similarity using single-pass SIMD cosine.
///
/// This is the primary entry point for cortex → simd similarity.
#[inline]
#[must_use]
pub fn embedding_similarity(a: &[f32], b: &[f32]) -> f64 {
    CosineComputer::new().cosine(a, b)
}

/// Batch embedding similarities for multiple candidates.
#[must_use]
pub fn embedding_batch_similarity(query: &[f32], candidates: &[Vec<f32>]) -> Vec<f64> {
    let computer = CosineComputer::new();
    candidates
        .iter()
        .map(|c| computer.cosine(query, c))
        .collect()
}

/// Parallel batch embedding similarities for large candidate sets.
#[must_use]
pub fn embedding_batch_similarity_par(query: &[f32], candidates: &[Vec<f32>]) -> Vec<f64> {
    let computer = CosineComputer::new();
    candidates
        .par_iter()
        .map(|c| computer.cosine(query, c))
        .collect()
}

/// Top-K most similar embeddings.
#[must_use]
pub fn embedding_top_k(query: &[f32], candidates: &[Vec<f32>], k: usize) -> Vec<TopKResult> {
    let searcher = TopKSearcher::new(k);
    searcher.top_k(query, candidates, k)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ------------------------------------------------------------------------
    // MetacognitivePipeline tests
    // ------------------------------------------------------------------------

    #[test]
    fn test_metacognitive_resolve_single_source() {
        let pipeline = MetacognitivePipeline::new();
        let evidence = vec![Evidence {
            source_id: 1,
            value: 0.85,
            confidence: 0.9,
            successes: 90,
            total: 100,
        }];
        let resolved = pipeline.resolve(&evidence);
        assert!((resolved.fused_value - 0.85).abs() < 1e-6);
        assert!(resolved.wilson_bound > 0.0);
    }

    #[test]
    fn test_metacognitive_resolve_no_conflict() {
        let pipeline = MetacognitivePipeline::new();
        // Two sources in close agreement — should trigger bayesian fusion
        let evidence = vec![
            Evidence {
                source_id: 1,
                value: 10.0,
                confidence: 0.8,
                successes: 80,
                total: 100,
            },
            Evidence {
                source_id: 2,
                value: 10.5,
                confidence: 0.8,
                successes: 80,
                total: 100,
            },
        ];
        let resolved = pipeline.resolve(&evidence);
        assert!(resolved.fused_value > 10.0 && resolved.fused_value < 10.5);
        assert!(!resolved.quality.conflict_detected);
    }

    #[test]
    fn test_metacognitive_resolve_conflict() {
        let pipeline = MetacognitivePipeline::new();
        // Two sources in disagreement — should trigger Wilson weighting
        let evidence = vec![
            Evidence {
                source_id: 1,
                value: 10.0,
                confidence: 0.9,
                successes: 90,
                total: 100,
            },
            Evidence {
                source_id: 2,
                value: 50.0,
                confidence: 0.5,
                successes: 50,
                total: 100,
            },
        ];
        let resolved = pipeline.resolve(&evidence);
        // Fused value should be pulled toward the higher-confidence source
        assert!(resolved.fused_value < 30.0);
        assert!(resolved.quality.conflict_detected);
    }

    #[test]
    fn test_metacognitive_detect_drift_none() {
        let pipeline = MetacognitivePipeline::new();
        let prev = vec![Resolved {
            fused_value: 10.0,
            quality: ReconciliationQuality {
                conflict_detected: false,
                max_wilson_bound: 0.8,
                coefficient_of_variation: 0.1,
            },
            wilson_bound: 0.8,
        }];
        // Identical current evidence
        let curr = vec![Evidence {
            source_id: 1,
            value: 10.0,
            confidence: 0.9,
            successes: 90,
            total: 100,
        }];
        let report = pipeline.detect_drift(&prev, &curr);
        assert!(!report.drift_detected);
        assert_eq!(report.severity, DriftSeverity::None);
    }

    #[test]
    fn test_metacognitive_detect_drift_high() {
        let pipeline = MetacognitivePipeline::new();
        let prev = vec![Resolved {
            fused_value: 10.0,
            quality: ReconciliationQuality {
                conflict_detected: false,
                max_wilson_bound: 0.8,
                coefficient_of_variation: 0.1,
            },
            wilson_bound: 0.8,
        }];
        // Very different current evidence — should trigger high drift
        let curr = vec![Evidence {
            source_id: 1,
            value: 90.0,
            confidence: 0.9,
            successes: 90,
            total: 100,
        }];
        let report = pipeline.detect_drift(&prev, &curr);
        assert!(report.drift_detected);
        assert_eq!(report.severity, DriftSeverity::High);
    }

    #[test]
    fn test_metacognitive_process_batch() {
        let pipeline = MetacognitivePipeline::new();
        let batch = vec![
            Evidence {
                source_id: 1,
                value: 0.8,
                confidence: 0.9,
                successes: 90,
                total: 100,
            },
            Evidence {
                source_id: 2,
                value: 0.7,
                confidence: 0.85,
                successes: 85,
                total: 100,
            },
        ];
        let (resolved, drift_report) = pipeline.process_batch(&batch);
        assert_eq!(resolved.len(), 2);
        assert!(drift_report.is_none()); // No previous batch stored
    }

    // ── drift detection integration tests (D8) ────────────────────────────────

    #[test]
    fn test_detect_drift_medium() {
        let pipeline = MetacognitivePipeline::new();
        let prev = vec![Resolved {
            fused_value: 10.0,
            quality: ReconciliationQuality {
                conflict_detected: false,
                max_wilson_bound: 0.8,
                coefficient_of_variation: 0.1,
            },
            wilson_bound: 0.8,
        }];
        // Moderate jump: 10 → 25, should trigger Medium or High
        let curr = vec![Evidence {
            source_id: 1,
            value: 25.0,
            confidence: 0.9,
            successes: 90,
            total: 100,
        }];
        let report = pipeline.detect_drift(&prev, &curr);
        assert!(report.drift_detected);
        assert!(
            matches!(report.severity, DriftSeverity::Medium | DriftSeverity::High),
            "expected Medium or High, got {:?}",
            report.severity
        );
    }

    #[test]
    fn test_detect_drift_low() {
        let pipeline = MetacognitivePipeline::new();
        // Identical values across prev and curr → KS = 0, JS = 0 → None
        let prev = vec![
            Resolved {
                fused_value: 10.0,
                quality: ReconciliationQuality {
                    conflict_detected: false,
                    max_wilson_bound: 0.8,
                    coefficient_of_variation: 0.05,
                },
                wilson_bound: 0.8,
            },
            Resolved {
                fused_value: 10.1,
                quality: ReconciliationQuality {
                    conflict_detected: false,
                    max_wilson_bound: 0.8,
                    coefficient_of_variation: 0.05,
                },
                wilson_bound: 0.8,
            },
        ];
        let curr = vec![
            Evidence {
                source_id: 1,
                value: 10.0,
                confidence: 0.9,
                successes: 90,
                total: 100,
            },
            Evidence {
                source_id: 2,
                value: 10.1,
                confidence: 0.9,
                successes: 90,
                total: 100,
            },
        ];
        let report = pipeline.detect_drift(&prev, &curr);
        assert_eq!(report.severity, DriftSeverity::None);
        assert!(!report.drift_detected);
    }

    #[test]
    fn test_detect_drift_empty_prev() {
        let pipeline = MetacognitivePipeline::new();
        let prev: Vec<Resolved> = vec![];
        let curr = vec![Evidence {
            source_id: 1,
            value: 10.0,
            confidence: 0.9,
            successes: 90,
            total: 100,
        }];
        let report = pipeline.detect_drift(&prev, &curr);
        assert!(!report.drift_detected);
        assert_eq!(report.severity, DriftSeverity::None);
    }

    // ------------------------------------------------------------------------
    // Embedding tests (existing)
    // ------------------------------------------------------------------------

    #[test]
    fn test_embedding_similarity_identical() {
        let a = vec![1.0, 0.0, 0.0];
        assert!((embedding_similarity(&a, &a) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_embedding_similarity_orthogonal() {
        let a = vec![1.0, 0.0];
        let b = vec![0.0, 1.0];
        assert!((embedding_similarity(&a, &b) - 0.0).abs() < 1e-6);
    }

    #[test]
    fn test_embedding_batch() {
        let query = vec![1.0, 0.0];
        let candidates = vec![vec![1.0, 0.0], vec![0.0, 1.0]];
        let results = embedding_batch_similarity(&query, &candidates);
        assert_eq!(results.len(), 2);
        assert!((results[0] - 1.0).abs() < 1e-6);
        assert!((results[1] - 0.0).abs() < 1e-6);
    }

    #[test]
    fn test_embedding_top_k() {
        let query = vec![1.0, 0.0, 0.0];
        let candidates = vec![
            vec![1.0, 0.0, 0.0],
            vec![0.0, 1.0, 0.0],
            vec![0.9, 0.1, 0.0],
        ];
        let results = embedding_top_k(&query, &candidates, 2);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].index, 0);
    }
}
