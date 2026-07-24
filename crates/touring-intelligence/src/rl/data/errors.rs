//! Error types for data loaders (audit, telemetry).

use std::path::PathBuf;
use thiserror::Error;

/// Errors raised while loading and parsing the `audit.jsonl` audit log.
#[derive(Error, Debug)]
pub enum AuditError {
    /// Filesystem read failed.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    /// A single audit line failed to deserialize.
    #[error("JSON parse error at line {line}: {source}")]
    Json {
        /// 1-based line number of the offending record.
        line: usize,
        /// Underlying `serde_json` parse error.
        #[source]
        source: serde_json::Error,
    },
    /// The audit file path does not exist.
    #[error("Audit file not found: {0}")]
    NotFound(PathBuf),
}

/// Errors raised while loading and parsing telemetry event files.
#[derive(Error, Debug)]
pub enum TelemetryError {
    /// Filesystem read failed.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    /// A single telemetry line failed to deserialize.
    #[error("JSON parse error at line {line}: {source}")]
    Json {
        /// 1-based line number of the offending record.
        line: usize,
        /// Underlying `serde_json` parse error.
        #[source]
        source: serde_json::Error,
    },
    /// The telemetry directory does not exist.
    #[error("Telemetry directory not found: {0}")]
    NotFound(PathBuf),
    /// Matching telemetry files by glob pattern failed.
    #[error("Glob pattern error: {0}")]
    Glob(PathBuf),
}

/// Convenience result type aliasing `AuditError` for audit loaders.
pub type Result<T> = std::result::Result<T, AuditError>;
