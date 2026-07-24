//! Drift Tools — touring_evolution_drift
//!
//! Exposes DriftDetector drift detection via a dedicated MCP tool.

use serde::{Deserialize, Serialize};
use touring_intelligence::rl::ranking::wilson::{DriftDetector, DriftResult};

/// Input for touring_evolution_drift tool
#[derive(Debug, Clone, Deserialize)]
pub struct DriftInput {
    /// Optional metric name to filter results. If None, returns all drift metrics.
    pub metric_name: Option<String>,
}

/// Individual drift metric result
#[derive(Debug, Clone, Serialize)]
pub struct DriftMetricResult {
    /// Metric name
    pub metric: String,
    /// Whether drift was detected
    pub drift_detected: bool,
    /// Magnitude of drift
    pub magnitude: f64,
    /// Direction: "up", "down", or "stable"
    pub direction: String,
    /// Confidence in the drift detection (0..1)
    pub confidence: f64,
}

/// Output from touring_evolution_drift tool
#[derive(Debug, Clone, Serialize)]
pub struct DriftOutput {
    /// Success status
    pub success: bool,
    /// Number of metrics returned
    pub metrics_returned: usize,
    /// Filter applied (if any)
    pub filter_metric: Option<String>,
    /// Drift results per metric
    pub results: Vec<DriftMetricResult>,
    /// Error message if failed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Convert a DriftResult into a DriftMetricResult.
fn to_metric_result(result: DriftResult, metric: &str) -> DriftMetricResult {
    DriftMetricResult {
        metric: metric.to_string(),
        drift_detected: result.drift_detected,
        magnitude: result.magnitude,
        direction: result.direction,
        confidence: result.confidence,
    }
}

/// Run drift analysis on a DriftDetector, optionally filtered by metric name.
pub fn run_drift_analysis(drift: &DriftDetector, filter_metric: Option<String>) -> DriftOutput {
    let all_results = drift.detect_all();

    let filtered: Vec<(&String, &DriftResult)> = if let Some(ref metric) = filter_metric {
        all_results.iter().filter(|(k, _)| *k == metric).collect()
    } else {
        all_results.iter().collect()
    };

    let results: Vec<DriftMetricResult> = filtered
        .into_iter()
        .map(|(metric, result)| to_metric_result(result.clone(), metric))
        .collect();

    DriftOutput {
        success: true,
        metrics_returned: results.len(),
        filter_metric,
        results,
        error: None,
    }
}

/// Handle the touring_evolution_drift tool synchronously.
pub fn handle_drift(drift: DriftDetector, input: DriftInput) -> DriftOutput {
    run_drift_analysis(&drift, input.metric_name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_drift_output_no_metrics() {
        let drift = DriftDetector::new();
        let output = run_drift_analysis(&drift, None);
        assert!(output.success);
        assert_eq!(output.metrics_returned, 0);
    }

    #[test]
    fn test_drift_output_with_filter() {
        let drift = DriftDetector::new();
        let output = run_drift_analysis(&drift, Some("nonexistent_metric".to_string()));
        assert!(output.success);
        assert_eq!(output.metrics_returned, 0);
        assert_eq!(output.filter_metric, Some("nonexistent_metric".to_string()));
    }

    #[test]
    fn test_drift_with_recorded_data() {
        let mut drift = DriftDetector::new();
        drift.record("latency", 10.0);
        drift.record("latency", 20.0);
        drift.record("latency", 15.0);

        let output = run_drift_analysis(&drift, Some("latency".to_string()));
        assert!(output.success);
        assert_eq!(output.metrics_returned, 1);
        assert_eq!(output.results[0].metric, "latency");
    }
}
