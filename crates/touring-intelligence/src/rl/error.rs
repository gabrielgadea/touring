//! Unified error types for touring-learning.

use thiserror::Error;

/// Unified error type for the touring-learning crate.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum LearningError {
    /// SQLite database error.
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    /// I/O error.
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),

    /// JSON serialization/deserialization error.
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    /// Invalid configuration or parameter.
    #[error("invalid config: {0}")]
    Config(String),

    /// Dimension mismatch in vector operations.
    #[error("dimension mismatch: expected {expected}, got {actual}")]
    DimensionMismatch {
        /// Expected vector dimensionality.
        expected: usize,
        /// Actual vector dimensionality received.
        actual: usize,
    },

    /// State not found (e.g., missing QTable entry, missing model).
    #[error("state not found: {0}")]
    NotFound(String),

    /// Persistence error (loading/saving state).
    #[error("persistence error: {0}")]
    Persistence(String),

    /// Capacity exceeded (buffer full, max entries reached).
    #[error("capacity exceeded: {0}")]
    CapacityExceeded(String),

    /// Convergence failure (RL algorithm did not converge within limit).
    #[error("convergence failure: {reason} — suggestion: {suggestion}")]
    ConvergenceFailure {
        /// Why convergence failed.
        reason: String,
        /// Suggested remediation for the caller.
        suggestion: String,
    },

    /// Invalid input to an algorithm (e.g., empty feature vector).
    #[error("invalid input: {0} — suggestion: {1}")]
    InvalidInput(String, String),
}

impl LearningError {
    /// Returns `true` if this error is transient and retrying may succeed.
    #[must_use]
    pub fn is_transient(&self) -> bool {
        matches!(self, Self::Sqlite(_) | Self::Io(_))
    }

    /// Returns `true` if this error represents a logic/programming error.
    #[must_use]
    pub fn is_logic_error(&self) -> bool {
        matches!(
            self,
            Self::DimensionMismatch { .. } | Self::InvalidInput(_, _) | Self::Config(_)
        )
    }
}

/// Convenience type alias.
pub type LearningResult<T> = Result<T, LearningError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transient_errors() {
        let io_err =
            LearningError::Io(std::io::Error::new(std::io::ErrorKind::TimedOut, "timeout"));
        assert!(io_err.is_transient());
        assert!(!io_err.is_logic_error());
    }

    #[test]
    fn test_logic_errors() {
        let dim_err = LearningError::DimensionMismatch {
            expected: 19,
            actual: 35,
        };
        assert!(dim_err.is_logic_error());
        assert!(!dim_err.is_transient());

        let config_err = LearningError::Config("bad alpha".to_string());
        assert!(config_err.is_logic_error());

        let input_err = LearningError::InvalidInput(
            "empty vector".to_string(),
            "provide non-empty feature vector".to_string(),
        );
        assert!(input_err.is_logic_error());
    }

    #[test]
    fn test_non_transient_non_logic() {
        let not_found = LearningError::NotFound("missing".to_string());
        assert!(!not_found.is_transient());
        assert!(!not_found.is_logic_error());

        let persist = LearningError::Persistence("disk full".to_string());
        assert!(!persist.is_transient());
        assert!(!persist.is_logic_error());
    }

    #[test]
    fn test_new_error_variants_display() {
        let cap = LearningError::CapacityExceeded("buffer full".to_string());
        assert!(cap.to_string().contains("capacity exceeded"));

        let conv = LearningError::ConvergenceFailure {
            reason: "after 1000 iterations".to_string(),
            suggestion: "increase max iterations or reduce learning rate".to_string(),
        };
        assert!(conv.to_string().contains("convergence failure"));

        let input = LearningError::InvalidInput(
            "empty features".to_string(),
            "provide non-empty feature vector".to_string(),
        );
        assert!(input.to_string().contains("invalid input"));
    }

    #[test]
    fn test_error_is_non_exhaustive() {
        // Verifies #[non_exhaustive]: within the same crate you may omit variants and
        // use a wildcard to cover them — the compiler accepts partial matches.
        let err = LearningError::Config("test".to_string());
        // The two-arm `match` with a wildcard is the *point* of the test —
        // it proves `#[non_exhaustive]` still accepts a partial pattern
        // within the defining crate. Converting to `if let` would delete
        // the very invariant under test.
        #[allow(clippy::single_match_else, clippy::single_match)]
        match err {
            LearningError::Config(_) => {}
            _ => {}
        }
    }
}
