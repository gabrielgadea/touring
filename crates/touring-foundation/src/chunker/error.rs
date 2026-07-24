//! Typed errors for chunking operations.

use thiserror::Error;

/// Result of a chunk operation with metadata.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ChunkResult {
    /// Ordered list of chunked text segments. Empty when
    /// `is_binary` is `true`.
    pub chunks: Vec<String>,
    /// Whether the source file was detected as binary and
    /// therefore NOT chunked. Callers should treat the chunks
    /// as meaningless when this is `true`.
    pub is_binary: bool,
    /// Whether a fallback chunker (e.g. line-based) was used in
    /// place of the primary chunker (typically AST-based). Useful
    /// for telemetry and quality dashboards.
    pub fallback_used: bool,
}

/// Errors produced by the chunking pipeline. Serialisable so they
/// can be persisted in the activity store alongside the source
/// path that triggered the failure.
#[derive(Debug, Error, Clone, serde::Serialize, serde::Deserialize)]
pub enum ChunkError {
    /// The source file was detected as binary and skipped before
    /// chunking. Not a hard failure — the caller's contract is
    /// "best-effort text chunks".
    #[error("file is binary")]
    BinaryFile,

    /// The chunker parser could not understand the source. The
    /// string carries the parser-specific diagnostic.
    #[error("parse error: {0}")]
    ParseError(String),

    /// Chunking exceeded the per-file time budget. Caller should
    /// retry with a smaller scope or escalate to FTS5-only mode.
    #[error("chunking timeout after {elapsed_ms}ms")]
    ChunkingTimeout {
        /// Wall-clock milliseconds elapsed before the chunker
        /// gave up.
        elapsed_ms: u64,
    },

    /// The number of produced chunks exceeds the configured
    /// ceiling. Used to bound memory and downstream token costs.
    #[error("chunk limit exceeded: {count} > {limit}")]
    ChunkLimitExceeded {
        /// Number of chunks that would have been produced.
        count: usize,
        /// Configured ceiling that was exceeded.
        limit: usize,
    },

    /// The source AST had a depth greater than the configured
    /// ceiling. Indicates deeply nested code (e.g. generated
    /// match arms) that should be split before chunking.
    #[error("AST depth exceeded: depth {depth} > limit {limit}")]
    ASTDepthExceeded {
        /// Maximum AST depth observed in the input.
        depth: usize,
        /// Configured ceiling that was exceeded.
        limit: usize,
    },

    /// Filesystem or transport I/O failure during chunking. The
    /// string is the underlying `std::io::Error` message.
    #[error("I/O error: {0}")]
    Io(String),
}
