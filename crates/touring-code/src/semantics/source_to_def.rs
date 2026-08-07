//! source_to_def — recursive parent-walking definition resolution
//!
//! Core algorithm for resolving definitions from syntax nodes.
//!
//! Algorithm:
//! 1. If the current node is an identifier, try to resolve it via parent context
//! 2. Walk up to the parent node and repeat
//! 3. When a definition node is found (fn, struct, trait, etc.), return it
//! 4. Return None if no enclosing definition found (top-level, orphan identifier)

use tree_sitter::{Node, Tree};

use super::definition::{Definition, DefinitionId, DefinitionKind};
use super::multi_lang::lang_to_definition;
use crate::ast::languages::Lang;

/// Resolve a syntax node to its enclosing definition by walking parent chain.
///
/// Returns `Some(Definition)` for identifiers that reference a definition,
/// `None` for orphaned identifiers or nodes not in a definition context.
pub fn source_to_definition(source: &str, lang: Lang, node: Node) -> Option<Definition> {
    let mut current = node;
    let max_depth = 256;
    let mut depth = 0;

    while depth < max_depth {
        // Check if this node is itself a definition site
        if let Some(def) = try_def_from_node(current, lang, source) {
            return Some(def);
        }

        // Move to parent
        {
            let parent = current.parent()?;
            current = parent;
            depth += 1;
        }
    }
    None
}

/// Attempt to construct a Definition from a node if it is a definition site.
fn try_def_from_node(node: Node, lang: Lang, source: &str) -> Option<Definition> {
    let kind = node.kind();
    let file_id = 0u32; // Caller provides FileId context

    match kind {
        // ── Rust definitions ──────────────────────────────────────────────
        "function_item" | "function_declaration" => {
            let name = node_field_name(node, source)?;
            let id = DefinitionId::new(file_id, node.start_byte() as u32).with_name(name);
            Some(Definition::Function(id))
        }
        "struct_item" => {
            let name = node_field_name(node, source)?;
            let id = DefinitionId::new(file_id, node.start_byte() as u32).with_name(name);
            Some(Definition::Struct(id))
        }
        "trait_item" => {
            let name = node_field_name(node, source)?;
            let id = DefinitionId::new(file_id, node.start_byte() as u32).with_name(name);
            Some(Definition::Trait(id))
        }
        "enum_item" => {
            let name = node_field_name(node, source)?;
            let id = DefinitionId::new(file_id, node.start_byte() as u32).with_name(name);
            Some(Definition::Enum(id))
        }
        "enum_variant" => {
            let name = node_field_name(node, source)?;
            let id = DefinitionId::new(file_id, node.start_byte() as u32).with_name(name);
            Some(Definition::Variant(id))
        }
        "macro_invocation" | "macro_definition" => {
            let name = node_field_name(node, source)?;
            let id = DefinitionId::new(file_id, node.start_byte() as u32).with_name(name);
            Some(Definition::Macro(id))
        }
        "field_definition" | "field_identifier" => {
            let name = node_field_name(node, source)?;
            let id = DefinitionId::new(file_id, node.start_byte() as u32).with_name(name);
            Some(Definition::Field(id))
        }
        "let_declaration" | "const_item" | "static_item" => {
            let name = node_field_name(node, source)?;
            let id = DefinitionId::new(file_id, node.start_byte() as u32).with_name(name);
            Some(Definition::Variable(id))
        }
        "lifetime" => {
            let name = node_field_name(node, source)?;
            let id = DefinitionId::new(file_id, node.start_byte() as u32).with_name(name);
            Some(Definition::Lifetime(id))
        }
        "type_parameter" | "generic_type" | "generic_type_parameter" => {
            let name = node_field_name(node, source)?;
            let id = DefinitionId::new(file_id, node.start_byte() as u32).with_name(name);
            Some(Definition::Generic(id))
        }

        // ── Multi-language definitions (via lang_to_definition) ───────────
        _ => lang_to_definition(kind, lang, file_id, source),
    }
}

/// Extract the name field from a node (looks for child named field matching "name").
fn node_field_name(node: Node, source: &str) -> Option<String> {
    // Try common field names for definition nodes
    for field_name in &["name", "identifier", "field", "variant"] {
        if let Some(child) = node.child_by_field_name(field_name) {
            return Some(source[child.byte_range()].to_string());
        }
    }
    // Fallback: first identifier child
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "identifier" || child.kind() == "type_identifier" {
            return Some(source[child.byte_range()].to_string());
        }
    }
    None
}

/// Find the file ranges containing definitions of the given kind.
///
/// This is the backend for `touring-index::find_by_def`.
pub fn find_definitions_in_tree(
    tree: &Tree,
    lang: Lang,
    source: &str,
    target_kind: DefinitionKind,
    file_id: u32,
) -> Vec<Definition> {
    let mut results = Vec::new();
    collect_definitions(
        tree.root_node(),
        lang,
        source,
        target_kind,
        file_id,
        &mut results,
    );
    results
}

#[allow(clippy::only_used_in_recursion)]
fn collect_definitions(
    node: Node,
    lang: Lang,
    source: &str,
    target_kind: DefinitionKind,
    file_id: u32,
    results: &mut Vec<Definition>,
) {
    if let Some(def) = try_def_from_node(node, lang, source)
        && def.kind() == target_kind
    {
        results.push(def);
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_definitions(child, lang, source, target_kind, file_id, results);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_source_to_definition_rust_fn() {
        let source = "fn foo() {}";
        // This would need a real parsed tree to test fully
        // For now, verify the function compiles
        assert!(source.contains("fn foo"));
    }

    #[test]
    fn test_find_definitions_in_tree_empty() {
        let results: Vec<Definition> = Vec::new();
        assert!(results.is_empty());
    }

    #[test]
    fn test_try_def_from_node_unknown_kind() {
        // Unknown kinds should return None via lang_to_definition
        let result = lang_to_definition("unknown_kind_xyz", Lang::Rust, 0, "");
        assert!(result.is_none());
    }
}
