//! Result extension traits for robust error handling in production code.
//!
//! Replaces direct `.unwrap()` and `.expect()` calls in I/O paths with
//! recoverable error patterns that log context without panicking.
//!
//! # Usage
//!
//! ```ignore
//! // In a module that imports ResultExt:
//! let content = std::fs::read_to_string("config.txt")
//!     .log_err("failed to read config file")
//!     .unwrap_or_default();
//! ```

use std::fmt::Display;
use tracing::debug;

/// Extension trait for Result types to provide contextual error handling.
pub trait ResultExt<T, E: std::fmt::Display> {
    /// Log the error with DEBUG level and return the default value.
    ///
    /// Use for expected failures that shouldn't clutter logs.
    fn unwrap_or_debug(self, default: T, context: &str) -> T;
}

impl<T, E: Display> ResultExt<T, E> for Result<T, E> {
    fn unwrap_or_debug(self, default: T, context: &str) -> T {
        match self {
            Ok(value) => value,
            Err(e) => {
                debug!(error = %e, "{}", context);
                default
            }
        }
    }
}

/// Extension trait for Option types with contextual logging.
pub trait OptionExt<T> {
    /// Log if None with DEBUG level and return the default value.
    fn unwrap_or_debug(self, default: T, context: &str) -> T;
}

impl<T> OptionExt<T> for Option<T> {
    fn unwrap_or_debug(self, default: T, context: &str) -> T {
        match self {
            Some(value) => value,
            None => {
                debug!("{}", context);
                default
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // Tier A (2026-04-19): named import of the pretty_assertions
    // shadow-macros from the crate-wide test prelude.
    use pretty_assertions::{assert_eq, assert_ne};

    #[test]
    fn test_result_ok() {
        let result: Result<i32, &str> = Ok(42);
        assert_eq!(result.unwrap_or_debug(0, "context"), 42);
        // Negative control — distinct return paths must not collapse.
        assert_ne!(result.unwrap_or_debug(0, "context"), 0);
    }

    #[test]
    fn test_result_err() {
        let result: Result<i32, &str> = Err("test error");
        assert_eq!(result.unwrap_or_debug(0, "context"), 0);
    }

    #[test]
    fn test_option_some() {
        let option: Option<i32> = Some(42);
        assert_eq!(option.unwrap_or_debug(0, "context"), 42);
    }

    #[test]
    fn test_option_none() {
        let option: Option<i32> = None;
        assert_eq!(option.unwrap_or_debug(0, "context"), 0);
    }
}
