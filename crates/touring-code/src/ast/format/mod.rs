//! Format preservation module — comment-preserving Rust formatting.
//!
//! Provides [`PreservingFormatter`] which wraps `prettyplease` to emit
//! rustfmt-clean output while preserving gaps (whitespace, comments, doc markers)
//! between AST nodes. This mirrors rustfmt's `missed_spans.rs` cursor-based approach.

pub mod preserve;

pub use preserve::{
    Gap, PreservingFormatter, SnippetProvider, format_preserve, has_rustfmt_skip, is_idempotent,
};
