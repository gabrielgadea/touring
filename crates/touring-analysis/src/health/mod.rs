//! Code health scoring with Wilson confidence intervals.

pub mod confidence;
pub mod nlp;

use serde::{Deserialize, Serialize};

pub use nlp::NlpMetrics;
/// Re-export `EricksonElement` so that `crate::health::EricksonElement` resolves
/// in `nlp.rs` when the `erickson-bridge` feature is active.
#[cfg(feature = "erickson-bridge")]
pub use touring_offensive::erickson::EricksonElement;

/// Overall health status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HealthStatus {
    /// Score >= 0.8
    Healthy,
    /// Score 0.4–0.8
    Degraded,
    /// Score < 0.4
    Critical,
}

impl HealthStatus {
    /// Derive status from a score.
    pub fn from_score(score: f64) -> Self {
        if score >= 0.8 {
            Self::Healthy
        } else if score >= 0.4 {
            Self::Degraded
        } else {
            Self::Critical
        }
    }
}

/// A single dimension of code health.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthDimension {
    /// Dimension name (e.g., "blast_radius", "quality", "wiring").
    pub name: String,
    /// Score for this dimension (0.0–1.0).
    pub score: f64,
    /// Weight for composite scoring.
    pub weight: f64,
    /// Issues found in this dimension.
    pub issues: Vec<String>,
    /// Arbitrary metrics for this dimension.
    pub metrics: serde_json::Value,
    /// Computation time in milliseconds.
    pub duration_ms: u64,
}

/// Complete code health report combining all dimensions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeHealthReport {
    /// Project root path.
    pub project_root: String,
    /// ISO timestamp of analysis.
    pub timestamp: String,
    /// Analysis depth used.
    pub depth: String,
    /// Individual dimension scores.
    pub dimensions: Vec<HealthDimension>,
    /// Weighted composite score (0.0–1.0).
    pub composite_score: f64,
    /// Wilson 95% CI lower bound.
    pub confidence_lower: f64,
    /// Wilson 95% CI upper bound.
    pub confidence_upper: f64,
    /// Overall status.
    pub status: HealthStatus,
    /// Total analysis duration in milliseconds.
    pub total_duration_ms: u64,
}

/// Compute a CodeHealthReport from individual dimensions.
pub fn compute_health(
    project_root: &str,
    depth: &str,
    dimensions: Vec<HealthDimension>,
    total_duration_ms: u64,
) -> CodeHealthReport {
    let timestamp = chrono::Utc::now().to_rfc3339();

    // Weighted average
    let total_weight: f64 = dimensions.iter().map(|d| d.weight).sum();
    let composite_score = if total_weight > 0.0 {
        let raw = dimensions.iter().map(|d| d.score * d.weight).sum::<f64>() / total_weight;
        if raw.is_finite() { raw } else { 0.0 }
    } else {
        0.0
    };

    // Wilson confidence interval — use sum of data_points across dimensions,
    // not the count of dimensions. Each dimension's "data_points" metric
    // represents the number of files/samples that contributed to its score.
    // Falling back to 1 per dimension when the metric is absent keeps the
    // interval conservative (wide) rather than degenerate.
    let n_observations: u32 = dimensions
        .iter()
        .map(|d| {
            d.metrics
                .get("data_points")
                .and_then(|v| v.as_u64())
                .unwrap_or(1) as u32
        })
        .sum::<u32>()
        .max(1);
    let (lower, upper) = confidence::wilson_interval(composite_score, n_observations);

    CodeHealthReport {
        project_root: project_root.to_string(),
        timestamp,
        depth: depth.to_string(),
        dimensions,
        composite_score,
        confidence_lower: lower,
        confidence_upper: upper,
        status: HealthStatus::from_score(composite_score),
        total_duration_ms,
    }
}

/// CodeHealthReport enriched with optional NLP metrics from EricksonExtractor.
///
/// This type is created when the `erickson-bridge` feature is enabled and
/// source text is provided for NLP argument mining.
#[derive(Debug, Clone)]
pub struct CodeHealthReportEnriched {
    /// The underlying base report.
    pub base: CodeHealthReport,
    /// Optional NLP metrics derived from text analysis.
    pub nlp: Option<NlpMetrics>,
}

impl CodeHealthReportEnriched {
    /// Creates a new enriched report from a base report and optional NLP metrics.
    pub fn new(base: CodeHealthReport, nlp: Option<NlpMetrics>) -> Self {
        Self { base, nlp }
    }

    /// Enriches the base report with NLP metrics computed from `text`.
    #[cfg(feature = "erickson-bridge")]
    pub fn enrich_with_text(mut self, text: &str) -> Self {
        use crate::health::nlp::compute_nlp_metrics;
        self.nlp = compute_nlp_metrics(text);
        self
    }
}

impl From<CodeHealthReport> for CodeHealthReportEnriched {
    fn from(report: CodeHealthReport) -> Self {
        Self::new(report, None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_health_status_from_score() {
        assert_eq!(HealthStatus::from_score(0.9), HealthStatus::Healthy);
        assert_eq!(HealthStatus::from_score(0.6), HealthStatus::Degraded);
        assert_eq!(HealthStatus::from_score(0.2), HealthStatus::Critical);
    }

    #[test]
    fn test_compute_health_empty() {
        let report = compute_health("/test", "standard", vec![], 100);
        assert_eq!(report.composite_score, 0.0);
        assert_eq!(report.status, HealthStatus::Critical);
    }

    #[test]
    fn test_wilson_ci_uses_data_points_not_dimension_count() {
        // Two dimensions: one with 100 data_points, one with 50.
        // n_observations should be 150, NOT 2 (the dimension count).
        // With n=150 the CI should be much narrower than with n=2.
        let make_dim = |name: &str, score: f64, data_points: u64| HealthDimension {
            name: name.to_string(),
            score,
            weight: 1.0,
            issues: vec![],
            metrics: serde_json::json!({ "data_points": data_points }),
            duration_ms: 0,
        };
        let report_rich = compute_health(
            "/test",
            "standard",
            vec![make_dim("quality", 0.8, 100), make_dim("wiring", 0.8, 50)],
            0,
        );
        let report_no_dp = compute_health(
            "/test",
            "standard",
            vec![make_dim("quality", 0.8, 0), make_dim("wiring", 0.8, 0)],
            0,
        );
        // With data_points=0 falls back to 1 each → n=2 → wide CI.
        let ci_width_rich = report_rich.confidence_upper - report_rich.confidence_lower;
        let ci_width_sparse = report_no_dp.confidence_upper - report_no_dp.confidence_lower;
        assert!(
            ci_width_rich < ci_width_sparse,
            "more data_points should produce narrower CI: rich={ci_width_rich:.4} sparse={ci_width_sparse:.4}"
        );
    }

    #[test]
    fn test_compute_health_weighted() {
        let dims = vec![
            HealthDimension {
                name: "quality".to_string(),
                score: 0.9,
                weight: 2.0,
                issues: vec![],
                metrics: serde_json::json!({}),
                duration_ms: 50,
            },
            HealthDimension {
                name: "wiring".to_string(),
                score: 0.5,
                weight: 1.0,
                issues: vec!["orphans found".to_string()],
                metrics: serde_json::json!({}),
                duration_ms: 30,
            },
        ];
        let report = compute_health("/test", "standard", dims, 80);
        // (0.9*2 + 0.5*1) / 3 = 2.3/3 ≈ 0.767
        assert!((report.composite_score - 0.767).abs() < 0.01);
        assert_eq!(report.status, HealthStatus::Degraded);
        assert!(report.confidence_lower <= report.composite_score);
        assert!(report.confidence_upper >= report.composite_score);
    }
}
