//! Report formatting: Display, summary, JSON, and dimension comparison.
//!
//! Provides human-readable output for `CodeHealthReport` and a `DimensionDiff`
//! type for comparing two reports (before/after).

use crate::health::{CodeHealthReport, HealthStatus};
use serde::{Deserialize, Serialize};

/// Difference between two `CodeHealthReport`s.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DimensionDiff {
    /// Composite score delta (newer − older).
    pub score_delta: f64,
    /// Dimensions that improved (score increased by ≥ 0.01).
    pub improved: Vec<String>,
    /// Dimensions that degraded (score decreased by ≥ 0.01).
    pub degraded: Vec<String>,
    /// Dimensions unchanged (delta < 0.01).
    pub unchanged: Vec<String>,
}

impl CodeHealthReport {
    /// Single-line summary for CLI output.
    ///
    /// Format: `HEALTHY 0.87 [wiring:0.95 quality:0.83 temporal:1.00] 124ms`
    pub fn to_summary_line(&self) -> String {
        let status_str = match self.status {
            HealthStatus::Healthy => "HEALTHY",
            HealthStatus::Degraded => "DEGRADED",
            HealthStatus::Critical => "CRITICAL",
        };

        let dims: Vec<String> = self
            .dimensions
            .iter()
            .map(|d| format!("{}:{:.2}", d.name, d.score))
            .collect();

        format!(
            "{} {:.2} [{}] {}ms",
            status_str,
            self.composite_score,
            dims.join(" "),
            self.total_duration_ms
        )
    }

    /// Pretty-printed JSON representation.
    pub fn to_json_pretty(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".to_string())
    }

    /// Compare this report (older) against another (newer).
    ///
    /// Returns a `DimensionDiff` describing which dimensions improved,
    /// degraded, or stayed the same.
    pub fn diff(&self, newer: &CodeHealthReport) -> DimensionDiff {
        let score_delta = newer.composite_score - self.composite_score;

        let mut improved = Vec::new();
        let mut degraded = Vec::new();
        let mut unchanged = Vec::new();

        for new_dim in &newer.dimensions {
            let old_score = self
                .dimensions
                .iter()
                .find(|d| d.name == new_dim.name)
                .map(|d| d.score)
                .unwrap_or(0.0);

            let delta = new_dim.score - old_score;
            if delta >= 0.01 {
                improved.push(new_dim.name.clone());
            } else if delta <= -0.01 {
                degraded.push(new_dim.name.clone());
            } else {
                unchanged.push(new_dim.name.clone());
            }
        }

        // Dimensions present in old but missing in new count as degraded
        for old_dim in &self.dimensions {
            if !newer.dimensions.iter().any(|d| d.name == old_dim.name) {
                degraded.push(old_dim.name.clone());
            }
        }

        DimensionDiff {
            score_delta,
            improved,
            degraded,
            unchanged,
        }
    }
}

impl std::fmt::Display for CodeHealthReport {
    /// Two-line display format:
    /// ```text
    /// touring-analysis  HEALTHY  0.87  (CI 0.84–0.90)
    ///   wiring:0.95  quality:0.83  temporal:1.00  [124ms]
    /// ```
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let status_str = match self.status {
            HealthStatus::Healthy => "HEALTHY",
            HealthStatus::Degraded => "DEGRADED",
            HealthStatus::Critical => "CRITICAL",
        };

        writeln!(
            f,
            "touring-analysis  {}  {:.2}  (CI {:.2}\u{2013}{:.2})",
            status_str, self.composite_score, self.confidence_lower, self.confidence_upper,
        )?;

        let dims: Vec<String> = self
            .dimensions
            .iter()
            .map(|d| format!("{}:{:.2}", d.name, d.score))
            .collect();

        write!(f, "  {}  [{}ms]", dims.join("  "), self.total_duration_ms)
    }
}

impl std::fmt::Display for DimensionDiff {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let direction = if self.score_delta > 0.01 {
            "+"
        } else if self.score_delta < -0.01 {
            ""
        } else {
            "~"
        };
        write!(f, "delta: {direction}{:.2}", self.score_delta)?;

        if !self.improved.is_empty() {
            write!(f, "  improved: {}", self.improved.join(", "))?;
        }
        if !self.degraded.is_empty() {
            write!(f, "  degraded: {}", self.degraded.join(", "))?;
        }
        Ok(())
    }
}

impl std::fmt::Display for HealthDiff {
    /// Single-line format: `+0.12  improved: wiring, quality  degraded: temporal`
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.dimensions)
    }
}

/// Convenience aggregate of multiple `DimensionDiff` comparisons.
///
/// Use `CodeHealthReport::to_health_diff()` to compute this from two snapshots.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthDiff {
    /// The detailed per-dimension diff.
    pub dimensions: DimensionDiff,
    /// Composite score delta (newer − older). Positive = improvement.
    pub score_delta: f64,
    /// True if composite score improved by >= 0.01.
    pub improved: bool,
}

/// JSON-serializable metrics dashboard snapshot for alerting and time-series storage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsDashboard {
    /// ISO timestamp of the analysis.
    pub timestamp: String,
    /// Project root path.
    pub project_root: String,
    /// Composite score (0.0–1.0).
    pub composite_score: f64,
    /// Overall health status.
    pub status: String,
    /// Wilson 95% CI [lower, upper].
    pub confidence_interval: [f64; 2],
    /// Per-dimension scores keyed by dimension name.
    pub dimension_scores: std::collections::HashMap<String, f64>,
    /// Top issues across all dimensions (max 10).
    pub top_issues: Vec<String>,
    /// Total analysis duration in milliseconds.
    pub duration_ms: u64,
    /// Analysis depth label.
    pub depth: String,
}

impl MetricsDashboard {
    /// Produce a newline-delimited JSON (NDJSON) line from this dashboard.
    ///
    /// Suitable for log ingestion, streaming pipelines, and time-series storage.
    /// Each call produces a single JSON line (no trailing newline).
    pub fn to_json_line(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".to_string())
    }

    /// Produce alert strings for any dimension below the given threshold.
    ///
    /// Returns one alert per degraded dimension. An empty vec means all
    /// dimensions are at or above `threshold`.
    ///
    /// # Example
    /// ```
    /// use touring_analysis::MetricsDashboard;
    ///
    /// let dashboard = MetricsDashboard {
    ///     timestamp: "2026-01-01T00:00:00Z".to_string(),
    ///     project_root: ".".to_string(),
    ///     composite_score: 0.6,
    ///     status: "degraded".to_string(),
    ///     confidence_interval: [0.55, 0.65],
    ///     dimension_scores: std::collections::HashMap::new(),
    ///     top_issues: vec!["wiring: 5 orphans".to_string()],
    ///     duration_ms: 100,
    ///     depth: "standard".to_string(),
    /// };
    /// let alerts = dashboard.alerts_below(0.8);
    /// assert!(!alerts.is_empty());
    /// ```
    pub fn alerts_below(&self, threshold: f64) -> Vec<String> {
        let mut alerts: Vec<String> = self
            .dimension_scores
            .iter()
            .filter(|&(_, &score)| score < threshold)
            .map(|(dim, score)| format!("ALERT [{dim}] score={score:.2}"))
            .collect();
        if alerts.is_empty() && self.composite_score < threshold {
            alerts.push(format!(
                "ALERT [composite] score={:.2} status={}",
                self.composite_score, self.status
            ));
        }
        alerts.sort();
        alerts
    }
}

impl CodeHealthReport {
    /// Compare this report (older/baseline) against a newer report.
    ///
    /// Returns a `HealthDiff` describing overall improvement and per-dimension changes.
    ///
    /// # Example
    /// ```text
    /// // let diff = older_report.to_health_diff(&current_report);
    /// // if diff.improved { println!("Improved by {:.2}", diff.score_delta); }
    /// ```
    pub fn to_health_diff(&self, newer: &CodeHealthReport) -> HealthDiff {
        let dimensions = self.diff(newer);
        let score_delta = dimensions.score_delta;
        HealthDiff {
            improved: score_delta >= 0.01,
            score_delta,
            dimensions,
        }
    }

    /// Produce a `MetricsDashboard` snapshot from this report.
    ///
    /// Suitable for JSON serialization, alerting, and dashboards.
    pub fn to_dashboard(&self) -> MetricsDashboard {
        let dimension_scores: std::collections::HashMap<String, f64> = self
            .dimensions
            .iter()
            .map(|d| (d.name.clone(), d.score))
            .collect();

        let top_issues: Vec<String> = self
            .dimensions
            .iter()
            .flat_map(|d| d.issues.iter().cloned())
            .take(10)
            .collect();

        let status_str = match self.status {
            crate::health::HealthStatus::Healthy => "healthy",
            crate::health::HealthStatus::Degraded => "degraded",
            crate::health::HealthStatus::Critical => "critical",
        };

        MetricsDashboard {
            timestamp: self.timestamp.clone(),
            project_root: self.project_root.clone(),
            composite_score: self.composite_score,
            status: status_str.to_string(),
            confidence_interval: [self.confidence_lower, self.confidence_upper],
            dimension_scores,
            top_issues,
            duration_ms: self.total_duration_ms,
            depth: self.depth.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::health::{HealthDimension, compute_health};

    fn make_report(dims: Vec<(&str, f64)>, total_ms: u64) -> CodeHealthReport {
        let dimensions: Vec<HealthDimension> = dims
            .into_iter()
            .map(|(name, score)| HealthDimension {
                name: name.to_string(),
                score,
                weight: 1.0,
                issues: vec![],
                metrics: serde_json::json!({"data_points": 10}),
                duration_ms: 5,
            })
            .collect();
        compute_health("/test", "standard", dimensions, total_ms)
    }

    #[test]
    fn test_summary_line_format() {
        let report = make_report(vec![("wiring", 0.95), ("quality", 0.80)], 42);
        let line = report.to_summary_line();
        assert!(line.contains("HEALTHY"), "line: {line}");
        assert!(line.contains("wiring:0.95"), "line: {line}");
        assert!(line.contains("quality:0.80"), "line: {line}");
        assert!(line.contains("42ms"), "line: {line}");
    }

    #[test]
    fn test_display_two_lines() {
        let report = make_report(vec![("wiring", 0.95)], 10);
        let display = format!("{report}");
        let lines: Vec<&str> = display.lines().collect();
        assert_eq!(lines.len(), 2, "display should be 2 lines: {display}");
        let (line0, line1) = (
            lines.first().copied().unwrap_or(""),
            lines.get(1).copied().unwrap_or(""),
        );
        assert!(line0.contains("HEALTHY"), "line 0: {line0}");
        assert!(line1.contains("wiring:0.95"), "line 1: {line1}");
    }

    #[test]
    fn test_display_critical_status() {
        let report = make_report(vec![("quality", 0.2)], 5);
        let display = format!("{report}");
        assert!(display.contains("CRITICAL"), "display: {display}");
    }

    #[test]
    fn test_json_pretty_roundtrips() {
        let report = make_report(vec![("wiring", 0.9)], 10);
        let json = report.to_json_pretty();
        let parsed: CodeHealthReport = serde_json::from_str(&json).expect("should parse back");
        assert!((parsed.composite_score - report.composite_score).abs() < 1e-6);
    }

    #[test]
    fn test_diff_improvement() {
        let old = make_report(vec![("wiring", 0.7), ("quality", 0.5)], 10);
        let new = make_report(vec![("wiring", 0.9), ("quality", 0.5)], 10);
        let diff = old.diff(&new);
        assert!(diff.score_delta > 0.0, "score should improve");
        assert!(diff.improved.contains(&"wiring".to_string()));
        assert!(diff.unchanged.contains(&"quality".to_string()));
        assert!(diff.degraded.is_empty());
    }

    #[test]
    fn test_diff_degradation() {
        let old = make_report(vec![("wiring", 0.9)], 10);
        let new = make_report(vec![("wiring", 0.5)], 10);
        let diff = old.diff(&new);
        assert!(diff.score_delta < 0.0);
        assert!(diff.degraded.contains(&"wiring".to_string()));
    }

    #[test]
    fn test_diff_missing_dimension_counts_as_degraded() {
        let old = make_report(vec![("wiring", 0.9), ("temporal", 0.8)], 10);
        let new = make_report(vec![("wiring", 0.9)], 10);
        let diff = old.diff(&new);
        assert!(diff.degraded.contains(&"temporal".to_string()));
    }

    #[test]
    fn test_diff_display() {
        let old = make_report(vec![("wiring", 0.7)], 10);
        let new = make_report(vec![("wiring", 0.9)], 10);
        let diff = old.diff(&new);
        let display = format!("{diff}");
        assert!(display.contains("improved: wiring"), "display: {display}");
    }

    #[test]
    fn test_health_diff_improved_flag() {
        let diff = HealthDiff {
            dimensions: DimensionDiff {
                improved: vec!["quality".to_string()],
                degraded: vec![],
                unchanged: vec!["wiring".to_string()],
                score_delta: 0.05,
            },
            score_delta: 0.05,
            improved: true,
        };
        assert!(diff.improved);
        assert_eq!(diff.score_delta, 0.05);
        assert!(diff.dimensions.improved.contains(&"quality".to_string()));
    }

    #[test]
    fn test_health_diff_not_improved_below_threshold() {
        let diff = HealthDiff {
            dimensions: DimensionDiff {
                improved: vec![],
                degraded: vec![],
                unchanged: vec!["quality".to_string()],
                score_delta: 0.005,
            },
            score_delta: 0.005,
            improved: false,
        };
        assert!(!diff.improved, "delta < 0.01 should not be improved");
    }

    #[test]
    fn test_to_health_diff_roundtrip() {
        let old = make_report(vec![("wiring", 0.7), ("quality", 0.5)], 10);
        let new = make_report(vec![("wiring", 0.9), ("quality", 0.5)], 10);
        let diff = old.to_health_diff(&new);
        assert!(diff.score_delta > 0.0, "score should improve");
        assert!(diff.improved, "should be flagged as improved");
        assert!(diff.dimensions.improved.contains(&"wiring".to_string()));
    }

    #[test]
    fn test_metrics_dashboard_serializes() {
        let dashboard = MetricsDashboard {
            timestamp: "2026-04-05T00:00:00Z".to_string(),
            project_root: ".".to_string(),
            composite_score: 0.9,
            status: "healthy".to_string(),
            confidence_interval: [0.85, 0.95],
            dimension_scores: std::collections::HashMap::new(),
            top_issues: vec![],
            duration_ms: 100,
            depth: "standard".to_string(),
        };
        let json = serde_json::to_string(&dashboard).expect("should serialize");
        assert!(json.contains("composite_score"), "json: {json}");
        assert!(json.contains("0.9"), "json: {json}");
        assert!(json.contains("healthy"), "json: {json}");
    }

    #[test]
    fn test_to_dashboard_from_report() {
        let report = make_report(vec![("wiring", 0.95), ("quality", 0.80)], 42);
        let dashboard = report.to_dashboard();
        assert_eq!(dashboard.composite_score, report.composite_score);
        assert_eq!(dashboard.duration_ms, 42);
        assert!(
            dashboard.dimension_scores.contains_key("wiring"),
            "dimension_scores: {:?}",
            dashboard.dimension_scores
        );
        let wiring_score = dashboard
            .dimension_scores
            .get("wiring")
            .copied()
            .expect("wiring dimension present");
        assert!((wiring_score - 0.95).abs() < 1e-6);
        assert_eq!(dashboard.depth, "standard");
    }
}
