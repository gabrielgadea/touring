//! InsightEngine — generates actionable insights from analysis results.
//!
//! Unified from touring/src/evolution/insights.rs (398 LOC)

use super::analyzer::{AnalysisResult, Trend};
use serde::{Deserialize, Serialize};

/// Intelligence axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Axis {
    /// Insights about the system improving its own behavior.
    SelfImprovement,
    /// Insights about the evolution of the analyzed project.
    ProjectEvolution,
}

impl std::fmt::Display for Axis {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SelfImprovement => write!(f, "self_improvement"),
            Self::ProjectEvolution => write!(f, "project_evolution"),
        }
    }
}

/// Insight severity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Severity {
    /// Informational insight requiring no action.
    Info,
    /// Insight flagging a concern worth attention.
    Warning,
    /// Insight flagging a critical issue.
    Critical,
}

impl std::fmt::Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Info => write!(f, "info"),
            Self::Warning => write!(f, "warning"),
            Self::Critical => write!(f, "critical"),
        }
    }
}

/// A generated insight.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Insight {
    /// Intelligence axis this insight belongs to.
    pub axis: Axis,
    /// Category label for grouping insights.
    pub category: String,
    /// Severity level of the insight.
    pub severity: Severity,
    /// Human-readable description of the insight.
    pub message: String,
    /// Supporting evidence strings.
    pub evidence: Vec<String>,
    /// Optional recommended action.
    pub recommendation: Option<String>,
    /// Unix timestamp (seconds) when the insight was created.
    pub created_at: i64,
}

impl Insight {
    /// Construct an `Insight` with the given axis, category, severity, and message (timestamped now).
    pub fn new(
        axis: Axis,
        category: impl Into<String>,
        severity: Severity,
        message: impl Into<String>,
    ) -> Self {
        Self {
            axis,
            category: category.into(),
            severity,
            message: message.into(),
            evidence: Vec::new(),
            recommendation: None,
            created_at: chrono::Utc::now().timestamp(),
        }
    }

    pub(crate) fn with_evidence(mut self, evidence: Vec<String>) -> Self {
        self.evidence = evidence;
        self
    }

    pub(crate) fn with_recommendation(mut self, rec: impl Into<String>) -> Self {
        self.recommendation = Some(rec.into());
        self
    }
}

/// Engine that converts analysis results into insights.
#[derive(Debug)]
pub struct InsightEngine;

impl InsightEngine {
    /// Convert analysis results into a severity-sorted list of insights.
    pub fn generate(results: &[AnalysisResult]) -> Vec<Insight> {
        let mut insights = Vec::new();
        for result in results {
            if let Some(insight) = Self::result_to_insight(result) {
                insights.push(insight);
            }
        }
        insights.sort_by_key(|b| std::cmp::Reverse(b.severity));
        insights
    }

    fn result_to_insight(result: &AnalysisResult) -> Option<Insight> {
        let axis = Self::categorize_axis(&result.category);
        match result.category.as_str() {
            "tool_effectiveness" => Self::tool_effectiveness_insight(result, axis),
            "cila_progression" => Self::cila_insight(result, axis),
            "drift_detection" => Self::drift_insight(result, axis),
            _ => None,
        }
    }

    fn categorize_axis(category: &str) -> Axis {
        match category {
            "tool_effectiveness" | "cila_progression" | "cost_efficiency" => Axis::SelfImprovement,
            _ => Axis::ProjectEvolution,
        }
    }

    fn tool_effectiveness_insight(result: &AnalysisResult, axis: Axis) -> Option<Insight> {
        match result.trend {
            Trend::Degrading => Some(
                Insight::new(
                    axis,
                    &result.category,
                    Severity::Warning,
                    format!(
                        "Tool '{}' has low effectiveness (Wilson score: {:.4})",
                        result.metric, result.value
                    ),
                )
                .with_evidence(result.evidence.clone())
                .with_recommendation(format!(
                    "Investigate why '{}' has below-average success rate.",
                    result.metric
                )),
            ),
            Trend::Improving if result.value > 0.95 => Some(
                Insight::new(
                    axis,
                    &result.category,
                    Severity::Info,
                    format!(
                        "Tool '{}' is highly reliable (Wilson score: {:.4})",
                        result.metric, result.value
                    ),
                )
                .with_evidence(result.evidence.clone()),
            ),
            _ => None,
        }
    }

    fn cila_insight(result: &AnalysisResult, axis: Axis) -> Option<Insight> {
        match result.trend {
            Trend::Insufficient => Some(
                Insight::new(
                    axis,
                    &result.category,
                    Severity::Info,
                    "Insufficient compliance data for CILA progression analysis",
                )
                .with_evidence(result.evidence.clone()),
            ),
            Trend::Degrading => Some(
                Insight::new(
                    axis,
                    &result.category,
                    Severity::Warning,
                    format!("CILA adoption is low (weighted avg: {:.2})", result.value),
                )
                .with_evidence(result.evidence.clone()),
            ),
            _ => None,
        }
    }

    fn drift_insight(result: &AnalysisResult, axis: Axis) -> Option<Insight> {
        match result.trend {
            Trend::Degrading => {
                let severity = if result.value > 3.0 {
                    Severity::Critical
                } else {
                    Severity::Warning
                };
                Some(
                    Insight::new(
                        axis,
                        &result.category,
                        severity,
                        format!(
                            "Drift detected in '{}' (magnitude: {:.2})",
                            result.metric, result.value
                        ),
                    )
                    .with_evidence(result.evidence.clone()),
                )
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_insight_creation() {
        let insight = Insight::new(
            Axis::SelfImprovement,
            "test",
            Severity::Info,
            "test message",
        )
        .with_evidence(vec!["ev1".to_string()])
        .with_recommendation("do something");

        assert_eq!(insight.axis, Axis::SelfImprovement);
        assert!(insight.recommendation.is_some());
    }

    #[test]
    fn test_severity_ordering() {
        assert!(Severity::Critical > Severity::Warning);
        assert!(Severity::Warning > Severity::Info);
    }
}
