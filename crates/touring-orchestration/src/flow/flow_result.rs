//! Flow execution results and output targets.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;

/// Outcome of a single stage execution within a pipeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageOutcome {
    /// Name of the stage that produced this outcome.
    pub stage_name: String,
    /// Human-readable label for this output.
    pub output_label: String,
    /// How long this stage took to execute.
    pub duration_ms: u64,
    /// Error message if the stage failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl StageOutcome {
    /// Construct a successful stage outcome.
    pub fn ok(
        stage_name: impl Into<String>,
        output_label: impl Into<String>,
        duration: Duration,
    ) -> Self {
        Self {
            stage_name: stage_name.into(),
            output_label: output_label.into(),
            duration_ms: duration.as_millis() as u64,
            error: None,
        }
    }

    /// Construct a failed stage outcome.
    pub fn err(stage_name: impl Into<String>, duration: Duration, message: String) -> Self {
        Self {
            stage_name: stage_name.into(),
            output_label: String::new(),
            duration_ms: duration.as_millis() as u64,
            error: Some(message),
        }
    }
}

/// Where pipeline output is directed.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum OutputTarget {
    /// Write to stdout.
    #[default]
    Stdout,
    /// Write to a file (overwrites existing content).
    File(PathBuf),
    /// Write structured JSON to a file.
    Json(PathBuf),
    /// Discard output (no output).
    #[serde(other)]
    Discard,
}

/// The complete result of running a pipeline on one input item.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FlowResult<Item = crate::flow::types::Item> {
    /// The item that was processed.
    pub item: Item,
    /// Per-stage outcomes in execution order.
    pub stage_outcomes: Vec<StageOutcome>,
    /// Total wall-clock time for the pipeline run.
    pub total_ms: u64,
    /// True if all stages succeeded without error.
    pub success: bool,
}

impl<Item> FlowResult<Item> {
    /// Construct a successful flow result.
    pub fn ok(item: Item, stage_outcomes: Vec<StageOutcome>, total: Duration) -> Self {
        let total_ms = total.as_millis() as u64;
        let success = stage_outcomes.iter().all(|o| o.error.is_none());
        Self {
            item,
            stage_outcomes,
            total_ms,
            success,
        }
    }

    /// Construct a flow result with a failed stage.
    pub fn failed(item: Item, stage_outcomes: Vec<StageOutcome>, total: Duration) -> Self {
        Self {
            item,
            stage_outcomes,
            total_ms: total.as_millis() as u64,
            success: false,
        }
    }

    /// Returns `true` if the pipeline ran successfully.
    pub fn is_ok(&self) -> bool {
        self.success
    }

    /// Returns the first error message if any stage failed.
    pub fn error_message(&self) -> Option<&str> {
        self.stage_outcomes.iter().find_map(|o| o.error.as_deref())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn stage_outcome_ok() {
        let outcome = StageOutcome::ok("filter", "passed", Duration::from_millis(5));
        assert!(outcome.error.is_none());
        assert_eq!(outcome.stage_name, "filter");
        assert_eq!(outcome.duration_ms, 5);
    }

    #[test]
    fn stage_outcome_err() {
        let outcome = StageOutcome::err(
            "filter",
            Duration::from_millis(3),
            "predicate rejected".into(),
        );
        assert!(outcome.error.is_some());
        assert_eq!(outcome.duration_ms, 3);
    }

    #[test]
    fn flow_result_success() {
        let item = crate::flow::types::Item::new("i1", "test");
        let outcomes = vec![
            StageOutcome::ok("s1", "out1", Duration::from_millis(10)),
            StageOutcome::ok("s2", "out2", Duration::from_millis(20)),
        ];
        let result = FlowResult::ok(item, outcomes, Duration::from_millis(30));
        assert!(result.is_ok());
        assert_eq!(result.total_ms, 30);
        assert!(result.error_message().is_none());
    }

    #[test]
    fn flow_result_failure() {
        let item = crate::flow::types::Item::new("i1", "test");
        let outcomes = vec![StageOutcome::err(
            "s1",
            Duration::from_millis(10),
            "oops".into(),
        )];
        let result = FlowResult::failed(item, outcomes, Duration::from_millis(10));
        assert!(!result.is_ok());
        assert_eq!(result.error_message(), Some("oops"));
    }
}
