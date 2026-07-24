//! Touring Semantics — Unified semantic abstraction over symbol kinds
//!
//! Provides:
//! - [`Definition`] — unified enum for all symbol kinds across languages
//! - [`Semantics`] — facade for resolving definitions from syntax nodes
//! - [`source_to_def`] — recursive parent-walking algorithm
//! - [`multi_lang`] — language-specific Definition mapping via tree-sitter

pub mod definition;
// W4 fusion: nested `semantics` submodule (ex-touring-semantics crate root was
// `touring_semantics`, so `touring_semantics::semantics` had no inception).
pub mod multi_lang;
#[allow(clippy::module_inception)]
pub mod semantics;
pub mod source_to_def;

pub use definition::{Definition, DefinitionId, DefinitionKind, FileRange, Usage, UsageKind};
pub use multi_lang::{LangDefinitionMapping, lang_to_definition};
pub use semantics::Semantics;
pub use source_to_def::source_to_definition;

use crate::ast::languages::Lang;

/// Resolve the definition of a symbol at the given syntax node.
///
/// This is the main entry point for definition resolution.
/// Walks up the parent chain to find the enclosing definition.
pub fn resolve_definition(source: &str, lang: Lang, node: tree_sitter::Node) -> Option<Definition> {
    source_to_definition(source, lang, node)
}
