//! Input type for N1 tool sequence generation.
//!
//! Represents a raw user intent that needs to be translated into
//! an ordered sequence of tool calls.

use serde::{Deserialize, Serialize};

/// Specification of an objective to be achieved via tool sequence execution.
///
/// Contains the raw intent along with metadata that guides the generator.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectiveSpec {
    /// Natural language description of what to achieve.
    pub description: String,
    /// Optional target file(s) or module(s) affected.
    pub targets: Vec<TargetSpec>,
    /// Constraints that the generated sequence must respect.
    pub constraints: Vec<Constraint>,
    /// Desired quality gates (9D Pln2 dimensions).
    pub quality_gates: Vec<QualityGate>,
    /// Context from previous attempts (for learning).
    pub history: Vec<AttemptHistory>,
    /// Priority level (0=low, 5=critical).
    pub priority: u8,
}

/// A file or module target.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TargetSpec {
    /// Path to the target file or directory.
    pub path: String,
    /// Kind of target.
    pub kind: TargetKind,
}

/// Kind of target.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum TargetKind {
    /// A single Rust source file.
    SourceFile,
    /// A directory/package.
    Package,
    /// A module within a file.
    Module,
    /// A symbol (function, struct, etc.).
    Symbol,
    /// Generic file.
    #[default]
    Generic,
}

/// A constraint on the generation process.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Constraint {
    /// Constraint type.
    pub kind: ConstraintKind,
    /// Human-readable description.
    pub description: String,
}

/// Kinds of constraints.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ConstraintKind {
    /// Must not modify these files.
    DoNotModify(Vec<String>),
    /// Must complete within this time.
    MaxDuration(std::time::Duration),
    /// Must use at most N tool calls.
    MaxToolCalls(usize),
    /// Must preserve existing behavior.
    NoBehavioralChange,
    /// Custom constraint.
    Custom(String),
}

/// A quality gate dimension (9D Pln2).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum QualityGate {
    /// Code compiles and passes tests.
    Correctness,
    /// Scales to large inputs.
    Scalability,
    /// Performs within requirements.
    Performance,
    /// Applies to the problem domain.
    Applicability,
    /// Code is readable and maintainable.
    CodeQuality,
    /// Matches specification exactly.
    Specification,
    /// Integrates with existing code.
    Integration,
    /// Has minimal dependencies.
    Dependencies,
    /// Enables future improvements.
    Potentiation,
}

impl std::fmt::Display for QualityGate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            QualityGate::Correctness => write!(f, "Correctness"),
            QualityGate::Scalability => write!(f, "Scalability"),
            QualityGate::Performance => write!(f, "Performance"),
            QualityGate::Applicability => write!(f, "Applicability"),
            QualityGate::CodeQuality => write!(f, "CodeQuality"),
            QualityGate::Specification => write!(f, "Specification"),
            QualityGate::Integration => write!(f, "Integration"),
            QualityGate::Dependencies => write!(f, "Dependencies"),
            QualityGate::Potentiation => write!(f, "Potentiation"),
        }
    }
}

/// History of a previous attempt to satisfy this objective.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttemptHistory {
    /// Timestamp of the attempt.
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// The generated sequence that was used.
    pub sequence_hash: String,
    /// Outcome of the attempt.
    pub outcome: AttemptOutcome,
    /// Quality score achieved.
    pub quality_score: f64,
}

/// Outcome of an attempt.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AttemptOutcome {
    /// Succeeded fully.
    Success,
    /// Failed at a specific tool call.
    FailedAt(usize),
    /// Rolled back.
    RolledBack,
    /// Skipped.
    Skipped,
}

impl ObjectiveSpec {
    /// Create a new objective spec.
    pub fn new(description: impl Into<String>) -> Self {
        Self {
            description: description.into(),
            targets: Vec::new(),
            constraints: Vec::new(),
            quality_gates: Vec::new(),
            history: Vec::new(),
            priority: 3,
        }
    }

    /// Add a target file.
    pub fn with_target(mut self, path: impl Into<String>, kind: TargetKind) -> Self {
        self.targets.push(TargetSpec {
            path: path.into(),
            kind,
        });
        self
    }

    /// Add a constraint.
    pub fn with_constraint(mut self, kind: ConstraintKind, description: impl Into<String>) -> Self {
        self.constraints.push(Constraint {
            kind,
            description: description.into(),
        });
        self
    }

    /// Add a quality gate.
    pub fn with_quality_gate(mut self, gate: QualityGate) -> Self {
        self.quality_gates.push(gate);
        self
    }

    /// Set priority (0-5).
    pub fn with_priority(mut self, priority: u8) -> Self {
        self.priority = priority.clamp(0, 5);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_objective_spec_builder() {
        let spec = ObjectiveSpec::new("Fix authentication bug")
            .with_target("src/auth.rs", TargetKind::SourceFile)
            .with_constraint(
                ConstraintKind::NoBehavioralChange,
                "Must preserve existing API",
            )
            .with_quality_gate(QualityGate::Correctness)
            .with_priority(5);

        assert_eq!(spec.description, "Fix authentication bug");
        assert_eq!(spec.targets.len(), 1);
        assert_eq!(spec.constraints.len(), 1);
        assert_eq!(spec.quality_gates.len(), 1);
        assert_eq!(spec.priority, 5);
    }

    #[test]
    fn test_quality_gate_display() {
        assert_eq!(QualityGate::Correctness.to_string(), "Correctness");
        assert_eq!(QualityGate::Potentiation.to_string(), "Potentiation");
    }
}
