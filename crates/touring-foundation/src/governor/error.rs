//! Typed errors for resource governor operations.

use thiserror::Error;

/// Timeout exceeded — operation took longer than the configured limit.
#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
#[error("operation timed out after {elapsed:?} (limit: {limit:?})")]
pub struct TimeoutError {
    /// How much time elapsed before the timeout fired.
    pub elapsed: std::time::Duration,
    /// The configured timeout limit.
    pub limit: std::time::Duration,
}

/// Chunk count limit exceeded — would produce more chunks than allowed.
#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
#[error("chunk limit exceeded: {count} chunks (limit: {limit})")]
pub struct LimitError {
    /// Number of chunks produced so far.
    pub count: usize,
    /// The configured chunk limit.
    pub limit: usize,
}

/// Memory pressure threshold exceeded.
#[derive(Debug, Error, Clone, Copy, PartialEq)]
#[error("memory pressure: RSS {rss_mb:.1} MB exceeds threshold {threshold_mb:.1} MB")]
pub struct MemoryError {
    /// Current resident set size in MB.
    pub rss_mb: f64,
    /// Configured memory threshold in MB.
    pub threshold_mb: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_timeout_error_display() {
        let err = TimeoutError {
            elapsed: std::time::Duration::from_secs(30),
            limit: std::time::Duration::from_secs(10),
        };
        let s = err.to_string();
        assert!(s.contains("30"));
        assert!(s.contains("10"));
    }

    #[test]
    fn test_limit_error_display() {
        let err = LimitError {
            count: 100_000,
            limit: 50_000,
        };
        let s = err.to_string();
        assert!(s.contains("100000"));
        assert!(s.contains("50000"));
    }

    #[test]
    fn test_memory_error_display() {
        let err = MemoryError {
            rss_mb: 512.0,
            threshold_mb: 256.0,
        };
        let s = err.to_string();
        assert!(s.contains("512"));
        assert!(s.contains("256"));
    }
}
