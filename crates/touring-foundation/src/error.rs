//! Unified error types for the Touring workspace.
//!
//! Merged superset from touring (MCP server) and rust-core (learning kernel).
//! Uses `thiserror` for ergonomic derives and automatic `From` impls.

use thiserror::Error;

/// Unified error type for all Touring operations.
///
/// Uses `thiserror` for ergonomic derives and automatic `From` impls.
/// Marked `#[non_exhaustive]` to allow adding variants without breaking
/// downstream match exhaustiveness.
#[derive(Error, Debug)]
#[non_exhaustive]
pub enum TouringError {
    // ── IO / Persistence ──────────────────────────────────────────────
    /// Generic I/O error from the standard library. Auto-converted
    /// from `std::io::Error` via the `From` impl.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// SQLite database error. Auto-converted from `rusqlite::Error`
    /// via the `From` impl — covers query, transaction, and
    /// connection failures.
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    /// JSON serialization/deserialization error. Auto-converted from
    /// `serde_json::Error`. Common when persisting or reading
    /// `serde::Serialize` types.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    // ── Parsing / AST ─────────────────────────────────────────────────
    /// Generic parser failure — string payload carries the parser
    /// diagnostic. Used by inline parsers in the foundation crate.
    #[error("Parse error: {0}")]
    Parse(String),

    /// AST shape validation failed after parsing. The string
    /// describes which invariant was violated.
    #[error("AST validation failed: {0}")]
    AstValidation(String),

    /// A symbol was referenced by name but not present in the index.
    /// Emitted by `touring index find` consumers and by
    /// VGP-blocked code generation.
    #[error("Symbol not found: {0}")]
    SymbolNotFound(String),

    // ── Memory / Config / MCP ─────────────────────────────────────────
    /// Memory subsystem failure — persistence, recall, or schema
    /// migration error. String carries the underlying cause.
    #[error("Memory error: {0}")]
    Memory(String),

    /// Configuration file or runtime parameter invalid. Wraps
    /// `toml::de::Error` via the From impl below.
    #[error("Configuration error: {0}")]
    Config(String),

    /// MCP (Model Context Protocol) transport or framing error.
    /// Indicates a protocol-level mismatch on the client or server
    /// side of an MCP exchange.
    #[error("MCP protocol error: {0}")]
    Mcp(String),

    /// GPU or CPU embedding generation failed. Covers model load,
    /// OOM, and dimension-mismatch paths.
    #[error("Embedding error: {0}")]
    Embedding(String),

    // ── Kernel (from rust-core) ───────────────────────────────────────
    /// Vector or matrix dimension mismatch — the operation expected
    /// `expected` but received `got`. Common in embedding math.
    #[error("Invalid dimensions: expected {expected}, got {got}")]
    InvalidDimensions {
        /// Expected dimension (e.g. `"1536"` for an embedding slot).
        expected: String,
        /// Actual dimension received from the producer.
        got: String,
    },

    /// An operation was called with an empty input (empty string,
    /// empty vector, empty set) where at least one element is
    /// required. `operation` names the offending call.
    #[error("Empty input for operation: {operation}")]
    EmptyInput {
        /// Name of the operation that rejected the empty input
        /// (e.g. `"jaccard_similarity"`).
        operation: String,
    },

    /// Numerical failure — overflow, underflow, NaN result, or
    /// division-by-zero in the learning kernel.
    #[error("Numerical error: {message}")]
    NumericalError {
        /// Human-readable description of the numerical failure mode.
        message: String,
    },

    /// Clustering algorithm failure — convergence failure, empty
    /// clusters after assignment, or k > n.
    #[error("Clustering error: {message}")]
    ClusteringError {
        /// Human-readable description of the clustering failure.
        message: String,
    },

    /// Requested allocation exceeds the configured memory ceiling.
    /// The error carries both the limit and the requested size.
    #[error("Memory limit exceeded: requested {requested} bytes, limit is {limit} bytes")]
    MemoryLimitExceeded {
        /// Configured memory ceiling in bytes.
        limit: usize,
        /// Requested allocation size in bytes that exceeded the
        /// ceiling.
        requested: usize,
    },

    /// Parameter validation failed — string payload identifies the
    /// parameter, value, and reason. Emitted by `TryFrom<u8>` and
    /// other typed coercion paths.
    #[error("Invalid parameter '{param}': value '{value}' - {reason}")]
    InvalidParameter {
        /// Name of the parameter that failed validation.
        param: String,
        /// The invalid value that was supplied (as a string).
        value: String,
        /// Human-readable reason the value is invalid.
        reason: String,
    },

    /// The Q-table lookup key was not present. Indicates the
    /// state-space encoding drifted from the persisted snapshot.
    #[error("State {state} not found in Q-table")]
    StateNotFound {
        /// Numeric state identifier (hashed state tuple) that
        /// could not be located in the Q-table.
        state: u64,
    },

    /// Index access exceeded the container's bounds. Common in
    /// embedding matrix slice operations.
    #[error("Index {index} out of bounds (max: {max})")]
    IndexOutOfBounds {
        /// The index that was accessed (zero-based).
        index: usize,
        /// The maximum valid index for the container.
        max: usize,
    },

    // ── Rules engine ──────────────────────────────────────────────────
    /// Rules engine evaluation failure — pattern compilation,
    /// action execution, or rule chain cycle.
    #[error("Rules engine error: {0}")]
    Rules(String),

    // ── NLP ──────────────────────────────────────────────────────────
    /// NLP pipeline failure — tokenization, embedding lookup, or
    /// downstream consumer error.
    #[error("NLP error: {0}")]
    Nlp(String),

    // ── General ───────────────────────────────────────────────────────
    /// A function path is not yet implemented. String is the
    /// `&'static str` name of the placeholder. Tracking via this
    /// variant (rather than `unimplemented!()`) keeps the error
    /// type uniform.
    #[error("Not implemented: {0}")]
    NotImplemented(&'static str),

    /// Internal invariant violation — should never reach user code.
    /// Indicates a bug in the calling crate; please report with
    /// the message payload.
    #[error("Internal error: {0}")]
    Internal(String),
}

// ── Convenience conversions ───────────────────────────────────────────

impl From<TouringError> for std::io::Error {
    fn from(e: TouringError) -> Self {
        std::io::Error::other(e.to_string())
    }
}

impl From<toml::de::Error> for TouringError {
    fn from(e: toml::de::Error) -> Self {
        TouringError::Config(e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_io_error_from() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "missing");
        let err = TouringError::from(io_err);
        assert!(err.to_string().contains("missing"));
    }

    #[test]
    fn test_json_error_from() {
        let json_err: serde_json::Error = serde_json::from_str::<String>("not json").unwrap_err();
        let err = TouringError::from(json_err);
        assert!(!err.to_string().is_empty());
    }

    #[test]
    fn test_touring_to_io() {
        let err = TouringError::Internal("boom".into());
        let io_err: std::io::Error = err.into();
        assert!(io_err.to_string().contains("boom"));
    }

    #[test]
    fn test_invalid_dimensions() {
        let err = TouringError::InvalidDimensions {
            expected: "1536".to_string(),
            got: "768".to_string(),
        };
        assert!(err.to_string().contains("1536"));
        assert!(err.to_string().contains("768"));
    }

    #[test]
    fn test_empty_input() {
        let err = TouringError::EmptyInput {
            operation: "jaccard_similarity".to_string(),
        };
        assert!(err.to_string().contains("jaccard_similarity"));
    }

    #[test]
    fn test_memory_limit() {
        let err = TouringError::MemoryLimitExceeded {
            limit: 1000,
            requested: 2000,
        };
        assert!(err.to_string().contains("1000"));
        assert!(err.to_string().contains("2000"));
    }

    #[test]
    fn test_state_not_found() {
        let err = TouringError::StateNotFound { state: 42 };
        assert!(err.to_string().contains("42"));
    }

    #[test]
    fn test_index_out_of_bounds() {
        let err = TouringError::IndexOutOfBounds { index: 10, max: 5 };
        assert!(err.to_string().contains("10"));
        assert!(err.to_string().contains("5"));
    }

    #[test]
    fn test_not_implemented() {
        let err = TouringError::NotImplemented("fancy feature");
        assert!(err.to_string().contains("fancy feature"));
    }
}
