//! Validation criteria for generated sequences.
//!
//! Defines what "success" means for a generated sequence via
//! objective criteria and quality gates.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Criteria for validating a generated sequence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationCriteria {
    /// Individual checks that must pass.
    pub checks: Vec<ValidationCheck>,
    /// Maximum total execution time (ms).
    pub max_duration_ms: Option<u64>,
    /// Maximum number of tool calls.
    pub max_tool_calls: Option<usize>,
    /// Required quality gates.
    pub required_gates: HashSet<crate::rl::n1::objective_spec::QualityGate>,
    /// Expected output patterns (regex).
    pub output_patterns: Vec<String>,
}

impl ValidationCriteria {
    /// Create empty criteria.
    pub fn new() -> Self {
        Self {
            checks: Vec::new(),
            max_duration_ms: None,
            max_tool_calls: None,
            required_gates: HashSet::new(),
            output_patterns: Vec::new(),
        }
    }

    /// Add a validation check.
    pub fn with_check(mut self, check: ValidationCheck) -> Self {
        self.checks.push(check);
        self
    }

    /// Set max duration.
    pub fn with_max_duration_ms(mut self, ms: u64) -> Self {
        self.max_duration_ms = Some(ms);
        self
    }

    /// Set max tool calls.
    pub fn with_max_tool_calls(mut self, n: usize) -> Self {
        self.max_tool_calls = Some(n);
        self
    }

    /// Add a required quality gate.
    pub fn with_required_gate(mut self, gate: crate::rl::n1::objective_spec::QualityGate) -> Self {
        self.required_gates.insert(gate);
        self
    }

    /// Add an output pattern.
    pub fn with_output_pattern(mut self, pattern: impl Into<String>) -> Self {
        self.output_patterns.push(pattern.into());
        self
    }
}

impl Default for ValidationCriteria {
    fn default() -> Self {
        Self::new()
    }
}

/// A single validation check.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationCheck {
    /// Name of the check.
    pub name: String,
    /// The check kind.
    pub kind: ValidationCheckKind,
    /// Whether this check is critical.
    pub critical: bool,
}

/// Kind of validation check.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ValidationCheckKind {
    /// Check that a file exists.
    FileExists(String),
    /// Check that a file does not exist.
    FileNotExists(String),
    /// Check that output matches a pattern.
    OutputMatches {
        /// Pattern the output must match.
        pattern: String,
        /// Whether matching is case-sensitive.
        case_sensitive: bool,
    },
    /// Check that output does not contain error indicators.
    NoErrorIndicators,
    /// Check that all dependencies were satisfied.
    DependenciesSatisfied,
    /// Check that output is valid JSON.
    ValidJson,
    /// Check that output is non-empty.
    NonEmptyOutput,
    /// Check that a specific symbol exists in output.
    SymbolExists(String),
    /// Custom check with description.
    Custom {
        /// Description of the custom check.
        description: String,
        /// Whether the custom check passed.
        passed: bool,
    },
}

impl ValidationCheck {
    /// Create a file exists check.
    pub fn file_exists(path: impl Into<String>) -> Self {
        let path_str = path.into();
        Self {
            name: format!("file_exists:{}", path_str),
            kind: ValidationCheckKind::FileExists(path_str),
            critical: true,
        }
    }

    /// Create a no-error-indicators check.
    pub fn no_errors() -> Self {
        Self {
            name: "no_errors".into(),
            kind: ValidationCheckKind::NoErrorIndicators,
            critical: true,
        }
    }

    /// Create a valid JSON check.
    pub fn valid_json() -> Self {
        Self {
            name: "valid_json".into(),
            kind: ValidationCheckKind::ValidJson,
            critical: false,
        }
    }

    /// Create a dependencies satisfied check.
    pub fn dependencies_satisfied() -> Self {
        Self {
            name: "dependencies_satisfied".into(),
            kind: ValidationCheckKind::DependenciesSatisfied,
            critical: true,
        }
    }
}

impl std::fmt::Display for ValidationCheckKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ValidationCheckKind::FileExists(path) => write!(f, "FileExists({})", path),
            ValidationCheckKind::FileNotExists(path) => write!(f, "FileNotExists({})", path),
            ValidationCheckKind::OutputMatches { pattern, .. } => {
                write!(f, "OutputMatches({})", pattern)
            }
            ValidationCheckKind::NoErrorIndicators => write!(f, "NoErrorIndicators"),
            ValidationCheckKind::DependenciesSatisfied => write!(f, "DependenciesSatisfied"),
            ValidationCheckKind::ValidJson => write!(f, "ValidJson"),
            ValidationCheckKind::NonEmptyOutput => write!(f, "NonEmptyOutput"),
            ValidationCheckKind::SymbolExists(sym) => write!(f, "SymbolExists({})", sym),
            ValidationCheckKind::Custom { description, .. } => write!(f, "Custom({})", description),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validation_criteria_builder() {
        let criteria = ValidationCriteria::new()
            .with_check(ValidationCheck::file_exists("src/lib.rs"))
            .with_check(ValidationCheck::no_errors())
            .with_max_duration_ms(5000)
            .with_max_tool_calls(10)
            .with_output_pattern(r#"success:\s*true"#);

        assert_eq!(criteria.checks.len(), 2);
        assert_eq!(criteria.max_duration_ms, Some(5000));
        assert_eq!(criteria.max_tool_calls, Some(10));
        assert_eq!(criteria.output_patterns.len(), 1);
    }
}
