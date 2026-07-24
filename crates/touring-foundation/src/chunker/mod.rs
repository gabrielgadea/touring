//! Chunker module — GracefulChunker fallback chain for text and binary content.
//!
//! # Architecture
//!
//! - [`error::ChunkError`] — Typed errors for chunking operations
//! - [`error::ChunkResult`] — Metadata wrapper for chunking results
//! - [`graceful::Chunker`] — Trait for chunking strategies (Send + Sync)
//! - [`graceful::SemanticChunker`] — Primary AST-aware chunker
//! - [`graceful::DelimiterChunker`] — Fallback delimiter-based chunker
//! - [`graceful::GracefulChunker`] — Wrapper with primary + fallback chain
//! - [`graceful::ChunkingResult`] — Result with binary/fallback metadata

pub mod error;
pub mod graceful;

pub use error::{ChunkError, ChunkResult};
pub use graceful::{Chunker, ChunkingResult, DelimiterChunker, GracefulChunker, SemanticChunker};
