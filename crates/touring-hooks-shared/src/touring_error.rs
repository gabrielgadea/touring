//! Central error type for touring-hooks runtime errors.
//!
//! Provides a unified error enum for all hook operations, replacing ad-hoc
//! `String`-based errors in production paths.
//!
//! # Variants
//!
//! - `NotFound` — Resource or symbol not found in index or DB
//! - `ParseError` — Failed to parse input (JSON, file content, command output)
//! - `WiringError` — Wiring subsystem error (orphans, integration score violations)
//! - `HookError` — General hook runtime error (pre/post hook failures)
//! - `LockError` — Synchronization error (poisoned locks, timeout acquiring locks)
//!
//! # Implementation
//!
//! Implements `std::error::Error` for ergonomic `?` propagation.
//! Provides `From` implementations for `std::io::Error`, `serde_json::Error`,
//! and `std::time::Duration` for seamless error conversion.

use std::error::Error;
use std::fmt;
use std::io;
use std::time::Duration;

/// Central error type for touring-hooks operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TouringError {
    /// Resource or symbol not found.
    NotFound(String),
    /// Parse error (JSON, file content, command output).
    ParseError(String),
    /// Wiring subsystem error.
    WiringError(String),
    /// General hook runtime error.
    HookError(String),
    /// Synchronization error (lock acquisition failure, timeout).
    LockError(String),
}

impl fmt::Display for TouringError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound(msg) => write!(f, "NotFound: {msg}"),
            Self::ParseError(msg) => write!(f, "ParseError: {msg}"),
            Self::WiringError(msg) => write!(f, "WiringError: {msg}"),
            Self::HookError(msg) => write!(f, "HookError: {msg}"),
            Self::LockError(msg) => write!(f, "LockError: {msg}"),
        }
    }
}

impl Error for TouringError {}

// ---------------------------------------------------------------------------
// From implementations for seamless ? propagation
// ---------------------------------------------------------------------------

impl From<io::Error> for TouringError {
    fn from(e: io::Error) -> Self {
        Self::LockError(format!("io::Error: {}", e))
    }
}

impl From<serde_json::Error> for TouringError {
    fn from(e: serde_json::Error) -> Self {
        Self::ParseError(format!("serde_json: {}", e))
    }
}

// Note: std::time::Elapsed is tokio-specific. Use Duration for timeout errors instead.

impl From<Duration> for TouringError {
    fn from(d: Duration) -> Self {
        Self::LockError(format!("lock timeout after {}ms", d.as_millis()))
    }
}

impl TouringError {
    /// Creates a `NotFound` error with a formatted message.
    #[inline]
    pub fn not_found(msg: impl Into<String>) -> Self {
        Self::NotFound(msg.into())
    }

    /// Creates a `ParseError` with a formatted message.
    #[inline]
    pub fn parse(msg: impl Into<String>) -> Self {
        Self::ParseError(msg.into())
    }

    /// Creates a `WiringError` with a formatted message.
    #[inline]
    pub fn wiring(msg: impl Into<String>) -> Self {
        Self::WiringError(msg.into())
    }

    /// Creates a `HookError` with a formatted message.
    #[inline]
    pub fn hook(msg: impl Into<String>) -> Self {
        Self::HookError(msg.into())
    }

    /// Creates a `LockError` with a formatted message.
    #[inline]
    pub fn lock(msg: impl Into<String>) -> Self {
        Self::LockError(msg.into())
    }

    /// Returns the error kind as a static string slice.
    #[inline]
    pub fn kind(&self) -> &'static str {
        match self {
            Self::NotFound(_) => "NotFound",
            Self::ParseError(_) => "ParseError",
            Self::WiringError(_) => "WiringError",
            Self::HookError(_) => "HookError",
            Self::LockError(_) => "LockError",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_display_not_found() {
        let err = TouringError::not_found("symbol 'Foo' not found");
        assert!(err.to_string().contains("NotFound"));
        assert!(err.to_string().contains("symbol 'Foo' not found"));
    }

    #[test]
    fn test_display_parse_error() {
        let err = TouringError::parse("invalid JSON at line 42");
        assert!(err.to_string().contains("ParseError"));
    }

    #[test]
    fn test_display_wiring_error() {
        let err = TouringError::wiring("orphan count exceeded threshold");
        assert!(err.to_string().contains("WiringError"));
    }

    #[test]
    fn test_display_hook_error() {
        let err = TouringError::hook("pre_edit hook returned Deny");
        assert!(err.to_string().contains("HookError"));
    }

    #[test]
    fn test_display_lock_error() {
        let err = TouringError::lock("mutex poisoned");
        assert!(err.to_string().contains("LockError"));
    }

    #[test]
    fn test_from_io_error() {
        let io_err = io::Error::new(io::ErrorKind::NotFound, "file not found");
        let tour_err: TouringError = io_err.into();
        assert!(matches!(tour_err, TouringError::LockError(_)));
    }

    #[test]
    fn test_from_serde_json_error() {
        let json_err = serde_json::from_str::<()>("invalid").expect_err("invalid JSON should fail");
        let tour_err: TouringError = json_err.into();
        assert!(matches!(tour_err, TouringError::ParseError(_)));
    }

    #[test]
    fn test_from_duration() {
        let dur = Duration::from_millis(500);
        let tour_err: TouringError = dur.into();
        assert!(tour_err.to_string().contains("500ms"));
    }

    #[test]
    fn test_kind() {
        assert_eq!(TouringError::not_found("x").kind(), "NotFound");
        assert_eq!(TouringError::parse("x").kind(), "ParseError");
        assert_eq!(TouringError::wiring("x").kind(), "WiringError");
        assert_eq!(TouringError::hook("x").kind(), "HookError");
        assert_eq!(TouringError::lock("x").kind(), "LockError");
    }

    #[test]
    fn test_error_source() {
        // TouringError has no source error
        let err = TouringError::not_found("test");
        assert!(err.source().is_none());
    }

    #[test]
    fn test_clone() {
        let err = TouringError::hook("original");
        let cloned = err.clone();
        assert_eq!(err, cloned);
    }
}
