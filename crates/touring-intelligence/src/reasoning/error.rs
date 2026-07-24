//! Error types for the touring-cognitive crate.

use thiserror::Error;

/// Errors that can occur in the cognitive engine.
#[derive(Error, Debug)]
#[non_exhaustive]
pub enum CognitiveError {
    /// A semantic graph operation failed; carries the cause.
    #[error("graph operation failed: {0}")]
    Graph(String),

    /// Persisting or loading state failed; carries the cause.
    #[error("persistence failed: {0}")]
    Persistence(String),

    /// Serialization of a value failed; carries the cause.
    #[error("serialization error: {0}")]
    Serialization(String),

    /// An underlying I/O error.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// A JSON serialization/deserialization error.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// The requested graph node was not found; carries its id.
    #[error("node not found: {0}")]
    NodeNotFound(String),

    /// The requested graph edge was not found.
    #[error("edge not found: from={from} to={to}")]
    EdgeNotFound {
        /// Id of the source node of the missing edge.
        from: String,
        /// Id of the target node of the missing edge.
        to: String,
    },

    /// A prediction operation failed; carries the cause.
    #[error("prediction failed: {0}")]
    Prediction(String),

    /// An operation exceeded its time budget.
    #[error("timeout after {duration_ms}ms: {operation}")]
    Timeout {
        /// Name of the operation that timed out.
        operation: String,
        /// Elapsed time in milliseconds before the timeout.
        duration_ms: u64,
    },

    /// A bounded resource exceeded its capacity; carries the detail.
    #[error("capacity exceeded: {0}")]
    CapacityExceeded(String),

    /// Catch-all for unexpected errors. Delegates Display and source to the
    /// underlying error, hiding implementation details from callers.
    #[error(transparent)]
    Other(Box<dyn std::error::Error + Send + Sync>),
}

impl CognitiveError {
    /// Returns `true` if this error is transient and retrying may succeed.
    #[must_use]
    pub fn is_transient(&self) -> bool {
        matches!(self, Self::Io(_) | Self::Timeout { .. })
    }

    /// Returns `true` if this is a data/logic error (non-retryable).
    #[must_use]
    pub fn is_data_error(&self) -> bool {
        matches!(
            self,
            Self::NodeNotFound(_) | Self::EdgeNotFound { .. } | Self::Serialization(_)
        )
    }
}

/// Convenience type alias.
pub type CognitiveResult<T> = std::result::Result<T, CognitiveError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transient_errors() {
        let io_err = CognitiveError::Io(std::io::Error::new(
            std::io::ErrorKind::ConnectionRefused,
            "refused",
        ));
        assert!(io_err.is_transient());
        assert!(!io_err.is_data_error());

        let timeout = CognitiveError::Timeout {
            operation: "graph resolve".to_string(),
            duration_ms: 2000,
        };
        assert!(timeout.is_transient());
        assert!(!timeout.is_data_error());
    }

    #[test]
    fn test_data_errors() {
        let not_found = CognitiveError::NodeNotFound("foo".to_string());
        assert!(not_found.is_data_error());
        assert!(!not_found.is_transient());

        let edge_err = CognitiveError::EdgeNotFound {
            from: "a".to_string(),
            to: "b".to_string(),
        };
        assert!(edge_err.is_data_error());
        assert!(edge_err.to_string().contains("from=a"));
        assert!(edge_err.to_string().contains("to=b"));
    }

    #[test]
    fn test_new_error_variants_display() {
        let timeout = CognitiveError::Timeout {
            operation: "mcts_search".to_string(),
            duration_ms: 5000,
        };
        let msg = timeout.to_string();
        assert!(msg.contains("5000ms"), "Should show duration: {msg}");
        assert!(msg.contains("mcts_search"), "Should show operation: {msg}");

        let cap = CognitiveError::CapacityExceeded("graph nodes".to_string());
        assert!(cap.to_string().contains("capacity exceeded"));
    }

    #[test]
    fn test_json_error_from() {
        // Verify #[from] serde_json::Error works
        let json_result: Result<serde_json::Value, serde_json::Error> =
            serde_json::from_str("invalid json{");
        let json_err = json_result.unwrap_err();
        let cog_err: CognitiveError = json_err.into();
        assert!(matches!(cog_err, CognitiveError::Json(_)));
        assert!(!cog_err.is_transient());
    }

    #[test]
    fn test_other_error_transparent() {
        let other = CognitiveError::Other(Box::new(std::io::Error::other("wrapped error")));
        assert!(other.to_string().contains("wrapped error"));
    }
}
