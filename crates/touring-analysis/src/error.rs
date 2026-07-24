//! Structured error types for touring-analysis.
//!
//! All fallible operations in this crate return [`AnalysisError`] wrapped in
//! [`AnalysisResult`].  Callers can match on variants for structured recovery.

/// Alias for `Result<T, AnalysisError>`.
pub type AnalysisResult<T> = std::result::Result<T, AnalysisError>;

/// Error type for all touring-analysis operations.
///
/// Variants cover the full error surface: I/O failures, database errors,
/// parse failures, and configuration problems.
#[derive(Debug, thiserror::Error)]
pub enum AnalysisError {
    /// An I/O error occurred while reading source files.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// SQLite database error (symbol store or analysis cache).
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),

    /// The requested analysis feature is not available under the current
    /// feature flags.
    #[error("feature not available: {0}")]
    FeatureNotAvailable(&'static str),

    /// A required source file was not found at the expected path.
    #[error("file not found: {path}")]
    FileNotFound {
        /// The path that was expected to exist.
        path: String,
    },

    /// The analysis configuration is invalid.
    #[error("invalid configuration: {0}")]
    InvalidConfig(String),
}
