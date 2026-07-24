//! Orchestration error types.

use thiserror::Error;

/// Orchestration-layer error.
#[derive(Debug, Error)]
pub enum OrchError {
    /// Decompose DAG error.
    #[error("decompose error: {0}")]
    Decompose(String),
    /// Session lifecycle error.
    #[error("session error: {0}")]
    Session(String),
    /// RL bridge error.
    #[error("rl_bridge error: {0}")]
    RlBridge(String),
    /// I/O error.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

/// Result alias for orchestration operations.
pub type OrchResult<T> = Result<T, OrchError>;
