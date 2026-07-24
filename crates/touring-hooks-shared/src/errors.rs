//! Custom error types for touring-hooks — replaces String-based errors.
//!
//! Provides discriminated error types for better error handling and debugging.
//!
//! # Usage
//!
//! ```ignore
//! // Use TouringError::knowledge, ::wiring, ::hook, etc.
//! fn fallible_operation() -> Result<(), TouringError> {
//!     Err(TouringError::Knowledge("failed to read".into()))
//! }
//! ```

use std::fmt;

/// Core error type for touring-hooks operations.
#[derive(Debug, Clone)]
pub enum TouringError {
    /// Knowledge DB operation failed.
    Knowledge(String),
    /// Wiring system error.
    Wiring(String),
    /// Hook execution error.
    Hook(String),
    /// ACO/pheromone system error.
    Aco(String),
    /// File system error.
    Io(String),
    /// JSON serialization error.
    Json(String),
    /// Async runtime error.
    Async(String),
    /// Circuit breaker error.
    CircuitBreaker(String),
    /// Database lock/constraint error.
    LockError(String),
}

impl fmt::Display for TouringError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TouringError::Knowledge(msg) => write!(f, "Knowledge error: {}", msg),
            TouringError::Wiring(msg) => write!(f, "Wiring error: {}", msg),
            TouringError::Hook(msg) => write!(f, "Hook error: {}", msg),
            TouringError::Aco(msg) => write!(f, "ACO error: {}", msg),
            TouringError::Io(msg) => write!(f, "IO error: {}", msg),
            TouringError::Json(msg) => write!(f, "JSON error: {}", msg),
            TouringError::Async(msg) => write!(f, "Async error: {}", msg),
            TouringError::CircuitBreaker(msg) => write!(f, "Circuit breaker: {}", msg),
            TouringError::LockError(msg) => write!(f, "Lock error: {}", msg),
        }
    }
}

impl std::error::Error for TouringError {}

impl From<std::io::Error> for TouringError {
    fn from(e: std::io::Error) -> Self {
        TouringError::Io(e.to_string())
    }
}

impl From<serde_json::Error> for TouringError {
    fn from(e: serde_json::Error) -> Self {
        TouringError::Json(e.to_string())
    }
}

impl From<String> for TouringError {
    fn from(s: String) -> Self {
        TouringError::Hook(s)
    }
}

impl From<&str> for TouringError {
    fn from(s: &str) -> Self {
        TouringError::Hook(s.to_string())
    }
}

impl From<rusqlite::Error> for TouringError {
    fn from(e: rusqlite::Error) -> Self {
        TouringError::LockError(format!("db: {}", e))
    }
}

impl From<tokio::sync::broadcast::error::RecvError> for TouringError {
    fn from(e: tokio::sync::broadcast::error::RecvError) -> Self {
        TouringError::Async(e.to_string())
    }
}

impl From<tokio::task::JoinError> for TouringError {
    fn from(e: tokio::task::JoinError) -> Self {
        TouringError::Async(e.to_string())
    }
}

/// Result type alias using TouringError.
pub type Result<T> = std::result::Result<T, TouringError>;

/// Error context builder for chained errors.
#[derive(Debug, Clone)]
pub struct ErrorContext {
    error: TouringError,
    context: Vec<String>,
}

impl ErrorContext {
    /// Add context to an error.
    pub fn with_context(mut self, ctx: impl Into<String>) -> Self {
        self.context.push(ctx.into());
        self
    }

    /// Build the final error with all context.
    pub fn build(self) -> TouringError {
        if self.context.is_empty() {
            return self.error;
        }
        let ctx_str = self.context.join(" -> ");
        match self.error {
            TouringError::Knowledge(msg) => {
                TouringError::Knowledge(format!("{} [{}]", msg, ctx_str))
            }
            TouringError::Wiring(msg) => TouringError::Wiring(format!("{} [{}]", msg, ctx_str)),
            TouringError::Hook(msg) => TouringError::Hook(format!("{} [{}]", msg, ctx_str)),
            TouringError::Aco(msg) => TouringError::Aco(format!("{} [{}]", msg, ctx_str)),
            TouringError::Io(msg) => TouringError::Io(format!("{} [{}]", msg, ctx_str)),
            TouringError::Json(msg) => TouringError::Json(format!("{} [{}]", msg, ctx_str)),
            TouringError::Async(msg) => TouringError::Async(format!("{} [{}]", msg, ctx_str)),
            TouringError::CircuitBreaker(msg) => {
                TouringError::CircuitBreaker(format!("{} [{}]", msg, ctx_str))
            }
            TouringError::LockError(msg) => {
                TouringError::LockError(format!("{} [{}]", msg, ctx_str))
            }
        }
    }
}

impl TouringError {
    /// Start building an error context.
    pub fn context(self) -> ErrorContext {
        ErrorContext {
            error: self,
            context: Vec::new(),
        }
    }

    /// Create a knowledge error.
    pub fn knowledge(msg: impl Into<String>) -> Self {
        TouringError::Knowledge(msg.into())
    }

    /// Create a wiring error.
    pub fn wiring(msg: impl Into<String>) -> Self {
        TouringError::Wiring(msg.into())
    }

    /// Create a hook error.
    pub fn hook(msg: impl Into<String>) -> Self {
        TouringError::Hook(msg.into())
    }

    /// Create an ACO error.
    pub fn aco(msg: impl Into<String>) -> Self {
        TouringError::Aco(msg.into())
    }

    /// Create an IO error.
    pub fn io(msg: impl Into<String>) -> Self {
        TouringError::Io(msg.into())
    }

    /// Create a JSON error.
    pub fn json(msg: impl Into<String>) -> Self {
        TouringError::Json(msg.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_knowledge_error() {
        let err = TouringError::knowledge("file not found");
        assert!(err.to_string().contains("Knowledge error"));
    }

    #[test]
    fn test_from_io_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "not found");
        let touring_err: TouringError = io_err.into();
        assert!(touring_err.to_string().contains("IO error"));
    }

    #[test]
    fn test_error_context() {
        let err = TouringError::knowledge("read failed")
            .context()
            .with_context("loading config")
            .with_context("init phase")
            .build();
        let msg = err.to_string();
        assert!(msg.contains("loading config"));
        assert!(msg.contains("init phase"));
    }

    #[test]
    fn test_result_type_alias() {
        fn fallible() -> Result<i32> {
            Ok(42)
        }
        assert_eq!(fallible().unwrap(), 42);
    }
}
