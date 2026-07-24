//! Output of N1 tool sequence generation — the TRIAD.
//!
//! Contains the complete generated sequence with its validation criteria
//! and rollback plan, forming the TRIAD (execute + validate + rollback)
//! pattern transplanted from ACO `generator_engine.py`.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::tool_call::ToolCallId;
use super::{ExecutionOutcome, RollbackPlan, ToolCall, ValidationCriteria, ValidationResult};
use crate::rl::aco::PheroKey;

/// The TRIAD output of N1 sequence generation.
///
/// Contains:
/// 1. **execute**: ordered `tool_calls` respecting dependency graph
/// 2. **validate**: `validation` criteria for determining success
/// 3. **rollback**: `rollback` plan for restoring state on failure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratedSequence {
    /// Ordered tool calls to execute (topological order).
    pub tool_calls: Vec<ToolCall>,
    /// Validation criteria for this sequence.
    pub validation: ValidationCriteria,
    /// Rollback plan if execution fails.
    pub rollback: RollbackPlan,
    /// Confidence score (0.0 to 1.0).
    pub confidence: f64,
    /// Pheromone trail for learning (deposited after successful execution).
    pub pheromone_trail: Vec<PheromoneDeposit>,
    /// Metadata about the generation process.
    pub metadata: SequenceMetadata,
}

/// A pheromone deposit to be recorded after successful execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PheromoneDeposit {
    /// The pheromone key.
    pub key: PheromoneKey,
    /// The deposit amount.
    pub amount: f64,
    /// Timestamp of deposit.
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Key for a pheromone deposit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PheromoneKey {
    /// Tool sequence pattern (serialized).
    ToolSequence(String),
    /// Objective pattern.
    ObjectivePattern(String),
    /// Success path.
    SuccessPath(String),
    /// Failure pattern.
    FailurePattern(String),
}

impl From<PheromoneKey> for PheroKey {
    fn from(key: PheromoneKey) -> Self {
        match key {
            PheromoneKey::ToolSequence(seq) => PheroKey::TaskId(format!("seq:{}", seq)),
            PheromoneKey::ObjectivePattern(pat) => PheroKey::TaskId(format!("obj:{}", pat)),
            PheromoneKey::SuccessPath(path) => PheroKey::TaskId(format!("success:{}", path)),
            PheromoneKey::FailurePattern(pat) => PheroKey::TaskId(format!("fail:{}", pat)),
        }
    }
}

/// Metadata about how the sequence was generated.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SequenceMetadata {
    /// Name of the generator that produced this sequence.
    pub generator_name: String,
    /// Version of the generator.
    pub generator_version: String,
    /// Time taken to generate (ms).
    pub generation_time_ms: u64,
    /// Whether this is a learned (vs rule-based) sequence.
    pub is_learned: bool,
    /// Features used in generation.
    pub features: HashMap<String, f64>,
    /// Number of candidate sequences evaluated.
    pub candidates_evaluated: usize,
}

impl GeneratedSequence {
    /// Create a new generated sequence.
    pub fn new(
        tool_calls: Vec<ToolCall>,
        validation: ValidationCriteria,
        rollback: RollbackPlan,
    ) -> Self {
        Self {
            tool_calls,
            validation,
            rollback,
            confidence: 0.0,
            pheromone_trail: Vec::new(),
            metadata: SequenceMetadata {
                generator_name: "unknown".into(),
                generator_version: "0.0.0".into(),
                generation_time_ms: 0,
                is_learned: false,
                features: HashMap::new(),
                candidates_evaluated: 0,
            },
        }
    }

    /// Set the confidence score.
    pub fn with_confidence(mut self, confidence: f64) -> Self {
        self.confidence = confidence.clamp(0.0, 1.0);
        self
    }

    /// Set the metadata.
    pub fn with_metadata(mut self, metadata: SequenceMetadata) -> Self {
        self.metadata = metadata;
        self
    }

    /// Add a pheromone deposit.
    pub fn with_pheromone(mut self, key: PheromoneKey, amount: f64) -> Self {
        self.pheromone_trail.push(PheromoneDeposit {
            key,
            amount,
            timestamp: chrono::Utc::now(),
        });
        self
    }

    /// Get a tool call by ID.
    pub fn get_call(&self, id: ToolCallId) -> Option<&ToolCall> {
        self.tool_calls.iter().find(|c| c.id == id)
    }

    /// Get parallel groups for execution.
    pub fn parallel_groups(&self) -> Vec<Vec<ToolCallId>> {
        super::tool_call::parallel_groups(&self.tool_calls)
    }

    /// Compute topological order.
    pub fn topological_order(&self) -> Result<Vec<ToolCallId>, Vec<ToolCallId>> {
        super::tool_call::topological_order(&self.tool_calls)
    }

    /// Validate this sequence against execution outcomes.
    pub fn validate_outcome(&self, outcome: &ExecutionOutcome) -> ValidationResult {
        let mut failures = Vec::new();

        // Check success
        if !outcome.success {
            failures.push(format!(
                "Sequence failed at tool call {}",
                outcome
                    .first_failure_index
                    .map(|i| i.to_string())
                    .unwrap_or_else(|| "unknown".into())
            ));
        }

        // Check quality score
        if outcome.quality_score < self.confidence {
            failures.push(format!(
                "Quality score {} below confidence threshold {}",
                outcome.quality_score, self.confidence
            ));
        }

        ValidationResult {
            passed: failures.is_empty(),
            failures,
            confidence: outcome.quality_score,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rl::n1::tool_call::{RetryPolicy, ToolArguments, ToolCall, ToolCallId};

    fn make_test_call(id: usize, deps: Vec<usize>) -> ToolCall {
        ToolCall {
            id: ToolCallId::new(id),
            tool_name: format!("tool_{}", id),
            arguments: ToolArguments::new(),
            dependencies: deps.into_iter().map(|d| ToolCallId::new(d)).collect(),
            expected_output: None,
            retry_policy: RetryPolicy::default(),
            parallelizable: true,
        }
    }

    #[test]
    fn test_generated_sequence_builder() {
        let calls = vec![make_test_call(0, vec![]), make_test_call(1, vec![0])];
        let validation = ValidationCriteria::new()
            .with_check(crate::rl::n1::validation_criteria::ValidationCheck::no_errors());

        let rollback = RollbackPlan::new()
            .with_restore_file("src/lib.rs", "/tmp/backup")
            .with_full_rollback_possible();

        let seq = GeneratedSequence::new(calls, validation, rollback)
            .with_confidence(0.85)
            .with_pheromone(PheromoneKey::ToolSequence("test_seq".into()), 1.0);

        assert_eq!(seq.tool_calls.len(), 2);
        assert!((seq.confidence - 0.85).abs() < 1e-9);
        assert_eq!(seq.pheromone_trail.len(), 1);
    }

    #[test]
    fn test_parallel_groups() {
        let calls = vec![
            make_test_call(0, vec![]),
            make_test_call(1, vec![]),
            make_test_call(2, vec![0, 1]),
        ];

        let seq = GeneratedSequence::new(calls, ValidationCriteria::new(), RollbackPlan::new());

        let groups = seq.parallel_groups();
        assert_eq!(groups.len(), 2); // Level 1: [0,1], Level 2: [2]
    }

    #[test]
    fn test_validate_outcome_success() {
        let calls = vec![make_test_call(0, vec![])];
        let seq = GeneratedSequence::new(calls, ValidationCriteria::new(), RollbackPlan::new())
            .with_confidence(0.8);

        let outcome = ExecutionOutcome {
            success: true,
            executed: vec![],
            first_failure_index: None,
            quality_score: 0.9,
            error_message: None,
        };

        let result = seq.validate_outcome(&outcome);
        assert!(result.passed);
        assert!((result.confidence - 0.9).abs() < 1e-9);
    }

    #[test]
    fn test_validate_outcome_failure() {
        let calls = vec![make_test_call(0, vec![]), make_test_call(1, vec![0])];
        let seq = GeneratedSequence::new(calls, ValidationCriteria::new(), RollbackPlan::new())
            .with_confidence(0.8);

        let outcome = ExecutionOutcome {
            success: false,
            executed: vec![],
            first_failure_index: Some(1),
            quality_score: 0.3,
            error_message: Some("Edit failed".into()),
        };

        let result = seq.validate_outcome(&outcome);
        assert!(!result.passed);
        assert!(!result.failures.is_empty());
    }
}
