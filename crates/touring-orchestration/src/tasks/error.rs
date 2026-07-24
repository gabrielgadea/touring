//! Tasksfile error types.

use thiserror::Error;

/// Errors that can occur while parsing, validating, or compiling a Tasksfile.
#[derive(Debug, Error)]
pub enum TasksfileError {
    /// The Tasksfile YAML could not be parsed.
    #[error("YAML parse error: {0}")]
    YamlParse(#[from] serde_yaml::Error),

    /// An I/O error occurred while reading a file.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("HTTP request error: {0}")]
    #[cfg(feature = "http-client")]
    /// An HTTP request for a remote include failed.
    Http(#[from] reqwest::Error),

    /// A command template failed to render.
    #[error("Template render error: {0}")]
    Template(String),

    /// The Tasksfile failed semantic validation.
    #[error("Validation error: {0}")]
    Validation(String),

    /// A required parameter was not supplied.
    #[error("Parameter missing required value: {0}")]
    MissingRequiredParam(String),

    /// A parameter value is not among the allowed options.
    #[error("Invalid parameter option: {0}")]
    InvalidOption(String),

    /// An include could not be resolved (e.g. missing file or fetch error).
    #[error("Include resolution failed: {0}")]
    IncludeFailed(String),

    /// A circular include chain was detected.
    #[error("Circular include detected: {0}")]
    CircularInclude(String),
}

/// Convenience result type for Tasksfile operations.
pub type Result<T> = std::result::Result<T, TasksfileError>;
