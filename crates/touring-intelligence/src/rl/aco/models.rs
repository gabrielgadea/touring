//! ACO domain models — Rust equivalents of Python Pydantic V2 frozen models.
//!
//! All structs are immutable (no `&mut self` methods).
//! Unified from rust-core/src/aco/models.rs (897 LOC)
//! PyO3 attributes removed — they belong in touring-python.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;

// --- Enums ---

/// Operation mode determines orchestration depth.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OperationMode {
    /// Resolve the task directly with no subagent orchestration.
    Direct,
    /// Lightweight orchestration with minimal phases.
    Light,
    /// Full multi-phase orchestration pipeline.
    Complete,
    /// Deepest orchestration with all phases and audits.
    Deep,
}

impl fmt::Display for OperationMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Direct => write!(f, "Direct"),
            Self::Light => write!(f, "Light"),
            Self::Complete => write!(f, "Complete"),
            Self::Deep => write!(f, "Deep"),
        }
    }
}

/// Task complexity classification.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Complexity {
    /// Trivial task requiring minimal effort.
    Trivial,
    /// Moderate task with some intricacy.
    Moderate,
    /// High-complexity task needing careful handling.
    High,
    /// Critical-complexity task with maximal risk.
    Critical,
}

impl fmt::Display for Complexity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Trivial => write!(f, "Trivial"),
            Self::Moderate => write!(f, "Moderate"),
            Self::High => write!(f, "High"),
            Self::Critical => write!(f, "Critical"),
        }
    }
}

/// Validation check result status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ValidationStatus {
    /// Check passed.
    Pass,
    /// Check passed with a non-blocking warning.
    Warn,
    /// Check failed.
    Fail,
}

impl fmt::Display for ValidationStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pass => write!(f, "PASS"),
            Self::Warn => write!(f, "WARN"),
            Self::Fail => write!(f, "FAIL"),
        }
    }
}

/// Drift from objective.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DriftLevel {
    /// On track with the objective.
    Ok,
    /// Minor drift from the objective.
    Warning,
    /// Severe drift requiring corrective action.
    Critical,
    /// Drift so severe execution must stop.
    Halt,
}

impl fmt::Display for DriftLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ok => write!(f, "OK"),
            Self::Warning => write!(f, "WARNING"),
            Self::Critical => write!(f, "CRITICAL"),
            Self::Halt => write!(f, "HALT"),
        }
    }
}

/// Generator classification by function.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GeneratorType {
    /// Emits output from a fixed template.
    Template,
    /// Transforms existing input into output.
    Transformer,
    /// Composes multiple sub-outputs into one.
    Composer,
    /// Validates output against criteria.
    Validator,
}

/// Status of a generator execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ExecutionStatus {
    /// Node is waiting to be executed (ACO-4: state machine).
    Pending,
    /// Node is currently executing (ACO-4: state machine).
    Running,
    /// Node executed and validated successfully.
    Success,
    /// Node executed but failed post-execution validation.
    ValidationFailed,
    /// Node's execution itself failed.
    ExecutionFailed,
    /// Node's precondition was not satisfied.
    PreconditionFailed,
    /// Node failed and its rollback ran successfully.
    RollbackExecuted,
    /// Node failed and its rollback also failed.
    RollbackFailed,
}

impl fmt::Display for ExecutionStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pending => write!(f, "Pending"),
            Self::Running => write!(f, "Running"),
            Self::Success => write!(f, "Success"),
            Self::ValidationFailed => write!(f, "ValidationFailed"),
            Self::ExecutionFailed => write!(f, "ExecutionFailed"),
            Self::PreconditionFailed => write!(f, "PreconditionFailed"),
            Self::RollbackExecuted => write!(f, "RollbackExecuted"),
            Self::RollbackFailed => write!(f, "RollbackFailed"),
        }
    }
}

// --- N2: Intent & Objective Structs ---

/// Parsed user intent with gap analysis and risk assessment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentSpec {
    /// Original prompt text as supplied by the user.
    pub raw_prompt: String,
    /// Inferred underlying need behind the prompt.
    pub real_need: String,
    /// Analysis of gaps between stated and real need.
    pub gap_analysis: String,
    /// Selected orchestration depth for this intent.
    pub operation_mode: OperationMode,
    /// Risks identified up front.
    pub preliminary_risks: Vec<String>,
    /// Anti-patterns to guard against during execution.
    pub anti_patterns_to_watch: Vec<String>,
    /// Stable hash identifying the objective.
    pub objective_hash: String,
}

/// Single measurable success criterion with threshold.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuccessCriterion {
    /// Human-readable description of the criterion.
    pub criterion: String,
    /// Name of the metric being measured.
    pub metric: String,
    /// Minimum value the metric must reach to pass.
    pub threshold: f64,
    /// Current measured value of the metric.
    pub current_value: f64,
}

impl SuccessCriterion {
    /// Returns `true` when `current_value` meets or exceeds `threshold`.
    pub fn is_met(&self) -> bool {
        self.current_value >= self.threshold
    }
}

/// Impact analysis across system boundaries.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemicImpact {
    /// Components the change depends on or affects upstream.
    pub upstream: Vec<String>,
    /// Components affected downstream by the change.
    pub downstream: Vec<String>,
    /// Peer components affected laterally.
    pub lateral: Vec<String>,
    /// Future capabilities the change enables.
    pub future_enablement: Vec<String>,
}

// --- N1: Generator Contracts ---

/// Triad of scripts produced by a generator.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GeneratorOutputs {
    /// Script that performs the generator's action.
    pub execution_script: String,
    /// Script that validates the action's result.
    pub validation_script: String,
    /// Script that rolls back the action on failure.
    pub rollback_script: String,
}

impl GeneratorOutputs {
    /// Returns `true` when all three scripts are non-empty.
    pub fn is_complete(&self) -> bool {
        !self.execution_script.is_empty()
            && !self.validation_script.is_empty()
            && !self.rollback_script.is_empty()
    }
}

/// Design-by-contract specification for a generator node.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GeneratorContract {
    /// Condition that must hold before execution.
    pub precondition: String,
    /// Condition guaranteed to hold after execution.
    pub postcondition: String,
    /// Property preserved across execution.
    pub invariant: String,
}

impl GeneratorContract {
    /// Returns `true` when pre/post/invariant are all specified.
    pub fn is_complete(&self) -> bool {
        !self.precondition.is_empty()
            && !self.postcondition.is_empty()
            && !self.invariant.is_empty()
    }
}

impl Default for GeneratorNode {
    fn default() -> Self {
        Self {
            id: String::new(),
            description: String::new(),
            generator_type: GeneratorType::Template,
            inputs_data: Vec::new(),
            inputs_templates: Vec::new(),
            inputs_constraints: Vec::new(),
            outputs: GeneratorOutputs::default(),
            contract: GeneratorContract::default(),
            acceptance_criteria: String::new(),
            depends_on: Vec::new(),
        }
    }
}

/// Single generator in the DAG.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratorNode {
    /// Unique identifier of the node.
    pub id: String,
    /// Human-readable description of the node's purpose.
    pub description: String,
    /// Functional classification of the generator.
    pub generator_type: GeneratorType,
    /// Data inputs the generator consumes.
    pub inputs_data: Vec<String>,
    /// Template inputs the generator uses.
    pub inputs_templates: Vec<String>,
    /// Constraints the generator must honor.
    pub inputs_constraints: Vec<String>,
    /// Scripts produced by the generator.
    pub outputs: GeneratorOutputs,
    /// Design-by-contract specification for the node.
    pub contract: GeneratorContract,
    /// Criteria the output must satisfy to be accepted.
    pub acceptance_criteria: String,
    /// Ids of nodes this node depends on.
    pub depends_on: Vec<String>,
}

/// Immutable DAG of generator nodes with execution metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratorGraphModel {
    /// All generator nodes in the DAG.
    pub nodes: Vec<GeneratorNode>,
    /// Node ids forming the critical (longest) path.
    pub critical_path: Vec<String>,
    /// Groups of node ids that can run in parallel.
    pub parallelizable: Vec<Vec<String>>,
    /// Stable hash identifying the objective.
    pub objective_hash: String,
}

// --- N2: Goal Tracking ---

/// Score for one of the 9 quality dimensions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DimensionScore {
    /// Name of the quality dimension.
    pub name: String,
    /// Aggregate score for the dimension.
    pub score: f64,
    /// Breakdown of contributing sub-scores by name.
    pub sub_scores: HashMap<String, f64>,
    /// Minimum acceptable score for the dimension.
    pub threshold: f64,
    /// Drift classification relative to the threshold.
    pub drift_level: DriftLevel,
}

/// Snapshot of goal tracking state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoalTrackerState {
    /// Stable hash identifying the objective.
    pub objective_hash: String,
    /// Current orchestration phase.
    pub phase: String,
    /// Per-dimension quality scores.
    pub dimension_scores: Vec<DimensionScore>,
    /// Aggregate score across all dimensions.
    pub overall_score: f64,
    /// Overall drift classification.
    pub drift_level: DriftLevel,
    /// Diagnostic messages accumulated so far.
    pub messages: Vec<String>,
    /// Current iteration count.
    pub iteration: u32,
}

// --- N3: Evolution & Learning ---

/// Reusable pattern extracted from a successful orchestration cycle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearnedPattern {
    /// Unique identifier of the pattern.
    pub pattern_id: String,
    /// Human-readable description of the pattern.
    pub description: String,
    /// Reusable generator template the pattern captures.
    pub generator_template: String,
    /// Domain the pattern applies to.
    pub domain: String,
    /// Tags for retrieval and categorization.
    pub tags: Vec<String>,
}

/// Proposed system improvement.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemUpgrade {
    /// Component the upgrade targets.
    pub component: String,
    /// Proposed change to the component.
    pub change: String,
    /// Justification for the change.
    pub rationale: String,
}

/// Post-execution learning package.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvolutionPackage {
    /// Identifier of the orchestration session.
    pub session_id: String,
    /// Stable hash identifying the objective.
    pub objective_hash: String,
    /// Serialized JSON of the execution report.
    pub execution_report_json: String,
    /// Patterns learned during the session.
    pub learned_patterns: Vec<LearnedPattern>,
    /// Anti-patterns discovered during the session.
    pub anti_patterns_discovered: Vec<String>,
    /// Proposed system upgrades.
    pub system_upgrades: Vec<SystemUpgrade>,
    /// Quality metrics keyed by name.
    pub quality_metrics: HashMap<String, f64>,
    /// Persistence actions to apply post-session.
    pub persistence_actions: Vec<String>,
}

/// Result of a single validation check.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationResult {
    /// Orchestration phase the check belongs to.
    pub phase: String,
    /// Name of the validation check.
    pub check_name: String,
    /// Outcome status of the check.
    pub status: ValidationStatus,
    /// Expected value or condition.
    pub expected: String,
    /// Actual observed value or condition.
    pub actual: String,
    /// Human-readable result message.
    pub message: String,
    /// Suggested remediation when the check did not pass.
    pub remediation: Option<String>,
}

// --- Unit Tests ---

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_operation_mode_serialization() {
        let mode = OperationMode::Complete;
        let json = serde_json::to_string(&mode).unwrap();
        let back: OperationMode = serde_json::from_str(&json).unwrap();
        assert_eq!(back, mode);
    }

    #[test]
    fn test_all_enums_roundtrip() {
        for status in [
            ValidationStatus::Pass,
            ValidationStatus::Warn,
            ValidationStatus::Fail,
        ] {
            let json = serde_json::to_string(&status).unwrap();
            let back: ValidationStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(back, status);
        }
    }

    #[test]
    fn test_generator_outputs_completeness() {
        let complete = GeneratorOutputs {
            execution_script: "exec.py".into(),
            validation_script: "val.py".into(),
            rollback_script: "rb.py".into(),
        };
        assert!(complete.is_complete());

        let incomplete = GeneratorOutputs {
            execution_script: "exec.py".into(),
            validation_script: "".into(),
            rollback_script: "rb.py".into(),
        };
        assert!(!incomplete.is_complete());
    }

    #[test]
    fn test_success_criterion_is_met() {
        let met = SuccessCriterion {
            criterion: "coverage".into(),
            metric: "percent".into(),
            threshold: 0.8,
            current_value: 0.95,
        };
        assert!(met.is_met());

        let not_met = SuccessCriterion {
            criterion: "coverage".into(),
            metric: "percent".into(),
            threshold: 0.8,
            current_value: 0.5,
        };
        assert!(!not_met.is_met());
    }

    #[test]
    fn test_operation_mode_display() {
        assert_eq!(OperationMode::Direct.to_string(), "Direct");
        assert_eq!(OperationMode::Light.to_string(), "Light");
        assert_eq!(OperationMode::Complete.to_string(), "Complete");
        assert_eq!(OperationMode::Deep.to_string(), "Deep");
    }

    #[test]
    fn test_complexity_display() {
        assert_eq!(Complexity::Trivial.to_string(), "Trivial");
        assert_eq!(Complexity::Moderate.to_string(), "Moderate");
        assert_eq!(Complexity::High.to_string(), "High");
        assert_eq!(Complexity::Critical.to_string(), "Critical");
    }

    #[test]
    fn test_validation_status_display() {
        assert_eq!(ValidationStatus::Pass.to_string(), "PASS");
        assert_eq!(ValidationStatus::Warn.to_string(), "WARN");
        assert_eq!(ValidationStatus::Fail.to_string(), "FAIL");
    }

    #[test]
    fn test_drift_level_display() {
        assert_eq!(DriftLevel::Ok.to_string(), "OK");
        assert_eq!(DriftLevel::Warning.to_string(), "WARNING");
        assert_eq!(DriftLevel::Critical.to_string(), "CRITICAL");
        assert_eq!(DriftLevel::Halt.to_string(), "HALT");
    }

    #[test]
    fn test_execution_status_display() {
        assert_eq!(ExecutionStatus::Success.to_string(), "Success");
        assert_eq!(
            ExecutionStatus::ValidationFailed.to_string(),
            "ValidationFailed"
        );
        assert_eq!(
            ExecutionStatus::ExecutionFailed.to_string(),
            "ExecutionFailed"
        );
        assert_eq!(
            ExecutionStatus::PreconditionFailed.to_string(),
            "PreconditionFailed"
        );
        assert_eq!(
            ExecutionStatus::RollbackExecuted.to_string(),
            "RollbackExecuted"
        );
        assert_eq!(
            ExecutionStatus::RollbackFailed.to_string(),
            "RollbackFailed"
        );
    }

    #[test]
    fn test_display_in_format_string() {
        // Verify Display works in format! macros (the primary use case)
        let msg = format!(
            "Mode: {}, Status: {}",
            OperationMode::Deep,
            ValidationStatus::Pass
        );
        assert_eq!(msg, "Mode: Deep, Status: PASS");
    }
}
