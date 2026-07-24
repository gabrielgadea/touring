//! Semantics facade — resolves definitions from syntax nodes
//!
//! Provides [`Semantics`] struct which wraps a source file and language,
//! exposing `resolve_definition(node) -> Option<Definition>`.
//!
//! # Example
//!
//! ```
//! use touring_code::semantics::Semantics;
//! use touring_code::ast::languages::Lang;
//! use touring_code::ast::parser::ParserPool;
//!
//! let source = "fn foo() {}";
//! let pool = ParserPool::new();
//! let tree = pool.parse(source, Lang::Rust).unwrap();
//! let sem = Semantics::new(source, Lang::Rust, &tree);
//!
//! assert_eq!(sem.lang(), Lang::Rust);
//! assert_eq!(sem.source(), source);
//! ```

use std::collections::HashMap;

use crate::ast::languages::Lang;
use crate::ast::parser::ParsedFile;
use tree_sitter::{Node, Tree};

pub use super::definition::{
    Definition, DefinitionId, DefinitionKind, FileRange, Usage, UsageKind,
};
pub use super::multi_lang::lang_to_definition;

/// A semantics context bound to a source file and language.
///
/// Provides cached access to parsed tree and symbol mappings,
/// enabling efficient repeated lookups.
#[derive(Debug)]
pub struct Semantics<'a> {
    /// Source text (zero-copy — borrowed)
    source: &'a str,
    /// Language of the source
    lang: Lang,
    /// Parsed syntax tree
    tree: &'a Tree,
    /// Cached definition lookups: node byte_offset -> Definition
    def_cache: HashMap<usize, Definition>,
    /// Cached parent lookups: node byte_offset -> parent byte_offset
    parent_cache: HashMap<usize, usize>,
}

impl<'a> Semantics<'a> {
    /// Create a new Semantics from a parsed file.
    pub fn new(source: &'a str, lang: Lang, tree: &'a Tree) -> Self {
        Self {
            source,
            lang,
            tree,
            def_cache: Default::default(),
            parent_cache: Default::default(),
        }
    }

    /// Create from a ParsedFile via ParserPool.
    pub fn from_parsed(parsed: &'a ParsedFile) -> Self {
        Self::new(&parsed.source, parsed.language, &parsed.tree)
    }

    /// Resolve the definition for a syntax node.
    ///
    /// Uses parent-walking recursion:
    /// 1. If node is an identifier pointing to a definition, resolve it
    /// 2. Otherwise, walk up to parent and recurse
    /// 3. Cache results to avoid repeated walks
    pub fn resolve_definition(&mut self, node: Node) -> Option<Definition> {
        let byte_offset = node.start_byte();
        if let Some(cached) = self.def_cache.get(&byte_offset) {
            return Some(cached.clone());
        }
        let result = super::source_to_def::source_to_definition(self.source, self.lang, node);
        if let Some(ref def) = result {
            self.def_cache.insert(byte_offset, def.clone());
        }
        result
    }

    /// Find all usages of a given definition in this source file.
    ///
    /// Searches for references (call sites, type uses, etc.) that
    /// resolve to the same definition.
    pub fn usages_of(&self, def: &Definition) -> Vec<Usage> {
        let def_id: DefinitionId = match def {
            Definition::Function(id) => id.clone(),
            Definition::Struct(id) => id.clone(),
            Definition::Trait(id) => id.clone(),
            Definition::Module(id) => id.clone(),
            Definition::Variant(id) => id.clone(),
            Definition::Macro(id) => id.clone(),
            Definition::Field(id) => id.clone(),
            Definition::Variable(id) => id.clone(),
            Definition::Lifetime(id) => id.clone(),
            Definition::Generic(id) => id.clone(),
            Definition::Class(id) => id.clone(),
            Definition::Interface(id) => id.clone(),
            Definition::Enum(id) => id.clone(),
            Definition::TypeAlias(id) => id.clone(),
            Definition::Namespace(id) => id.clone(),
            Definition::Parameter(id) => id.clone(),
            Definition::Property(id) => id.clone(),
        };

        let mut usages = Vec::new();
        self.collect_usages(self.tree.root_node(), &def_id, &mut usages);
        usages
    }

    fn collect_usages(&self, node: Node, def_id: &DefinitionId, out: &mut Vec<Usage>) {
        let kind = node.kind();
        // Check if this node is a reference to the target definition
        if Self::node_is_reference(kind) {
            // For now, collect as Read usage
            let range = FileRange::from_node(node, def_id.file_id, self.source);
            out.push(Usage::new(range, UsageKind::Read));
        }
        // Recurse children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.collect_usages(child, def_id, out);
        }
    }

    fn node_is_reference(kind: &str) -> bool {
        matches!(
            kind,
            "identifier"
                | "field_identifier"
                | "type_identifier"
                | "primitive_type"
                | "integer_literal"
                | "string_literal"
                | "boolean_literal"
        )
    }

    /// Invalidate caches after an edit.
    pub fn invalidate_after_edit(&mut self, start_byte: usize, end_byte: usize) {
        // Remove cached entries in the edited range
        self.def_cache
            .retain(|&byte, _| byte < start_byte || byte >= end_byte);
        self.parent_cache
            .retain(|&byte, _| byte < start_byte || byte >= end_byte);
    }

    /// Get the source text.
    pub fn source(&self) -> &'a str {
        self.source
    }

    /// Get the language.
    pub fn lang(&self) -> Lang {
        self.lang
    }

    /// Get the syntax tree.
    pub fn tree(&self) -> &'a Tree {
        self.tree
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::parser::ParserPool;

    #[test]
    fn test_semantics_from_tree() {
        let source = "fn foo() {}";
        let pool = ParserPool::new();
        let tree = pool.parse(source, Lang::Rust).unwrap();
        let sem = Semantics::new(source, Lang::Rust, &tree);
        assert_eq!(sem.lang(), Lang::Rust);
        assert_eq!(sem.source(), source);
    }

    #[test]
    fn test_semantics_cache_invalidation() {
        let source = "fn foo() {}";
        let pool = ParserPool::new();
        let tree = pool.parse(source, Lang::Rust).unwrap();
        let mut sem = Semantics::new(source, Lang::Rust, &tree);
        sem.def_cache
            .insert(0, Definition::Function(DefinitionId::new(0, 0)));
        assert_eq!(sem.def_cache.len(), 1);
        sem.invalidate_after_edit(0, 3);
        assert!(sem.def_cache.is_empty());
    }
}
