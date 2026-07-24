//! Preserve sub-module — gap capture and comment-preserving formatting.

pub mod formatter;
pub mod snippet_provider;

pub use formatter::{PreservingFormatter, format_preserve, has_rustfmt_skip, is_idempotent};
pub use snippet_provider::{Gap, SnippetProvider};
