//! VGP v2 — Detailed symbol extraction for verified generation.
//!
//! Extracts struct fields, enum variants, impl methods (Rust), class fields,
//! class methods, enum members (Python), and interface/class/enum members
//! (TypeScript/JavaScript) using tree-sitter queries. This enables the
//! Verified Generation Protocol to verify that generated code references
//! only types and fields that **actually exist** in the codebase.

use serde::{Deserialize, Serialize};
use streaming_iterator::StreamingIterator;
use tree_sitter::{Query, QueryCursor};

use crate::ast::languages::Lang;
use crate::ast::node_text;
use crate::ast::parser::parse_thread_local;

// ─── Types ──────────────────────────────────────────────────────────────

/// Kind of a symbol member (field, variant, method, etc.).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MemberKind {
    /// Struct field
    Field,
    /// Enum variant
    Variant,
    /// Method (in impl block)
    Method,
    /// Associated type or constant
    Associated,
}

/// Detailed information about a symbol's member for VGP verification.
///
/// Each `SymbolDetail` represents a single field, variant, or method
/// belonging to a parent struct, enum, or impl block.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolDetail {
    /// Member name (field name, variant name, or method name)
    pub name: String,
    /// What kind of member this is
    pub kind: MemberKind,
    /// Type annotation as a string (e.g., `"HashMap<String, Vec<u8>>"`)
    /// None for enum variants without explicit types.
    pub type_str: Option<String>,
    /// Whether the member has `pub` visibility
    pub is_pub: bool,
    /// Line number (1-indexed)
    pub line: usize,
}

// ─── Queries ─────────────────────────────────────────────────────────────

/// Tree-sitter query source for Rust field/variant/method extraction.
const RUST_FIELDS_QUERY: &str = include_str!("queries/rust_fields.scm");

/// Tree-sitter query source for Python class/method/field extraction.
const PYTHON_FIELDS_QUERY: &str = include_str!("queries/python_fields.scm");

/// Tree-sitter query source for TypeScript interface/class/enum extraction.
const TS_FIELDS_QUERY: &str = include_str!("queries/typescript_fields.scm");

// ─── Extraction ─────────────────────────────────────────────────────────

/// Extract detailed members of a named symbol with auto-detected language.
///
/// Tries Rust first, then Python, then TypeScript. For explicit language
/// control, use [`extract_symbol_details_for_lang`].
///
/// # Arguments
/// * `source` - Source code
/// * `symbol_name` - Name of the struct/class/enum/interface to inspect
///
/// # Returns
/// A vector of [`SymbolDetail`] for each member found. Empty if the symbol
/// is not found or has no members.
pub fn extract_symbol_details(source: &str, symbol_name: &str) -> Vec<SymbolDetail> {
    // Try Rust first (backward-compatible default)
    let result = extract_symbol_details_for_lang(source, symbol_name, Lang::Rust);
    if !result.is_empty() {
        return result;
    }
    // Try Python
    let result = extract_symbol_details_for_lang(source, symbol_name, Lang::Python);
    if !result.is_empty() {
        return result;
    }
    // Try TypeScript
    extract_symbol_details_for_lang(source, symbol_name, Lang::TypeScript)
}

/// Extract detailed members of a named symbol for a specific language.
///
/// Supports Rust (struct fields, enum variants, impl methods), Python
/// (class fields, methods, enum members), and TypeScript/JavaScript
/// (interface members, class properties/methods, enum members).
///
/// # Arguments
/// * `source` - Source code
/// * `symbol_name` - Name of the type to inspect
/// * `lang` - Language of the source code
pub fn extract_symbol_details_for_lang(
    source: &str,
    symbol_name: &str,
    lang: Lang,
) -> Vec<SymbolDetail> {
    match lang {
        Lang::Rust => extract_rust_details(source, symbol_name),
        Lang::Python => extract_python_details(source, symbol_name),
        Lang::TypeScript | Lang::JavaScript => extract_ts_details(source, symbol_name, lang),
        _ => Vec::new(),
    }
}

// ─── Rust extraction ────────────────────────────────────────────────────

fn extract_rust_details(source: &str, symbol_name: &str) -> Vec<SymbolDetail> {
    let tree = match parse_thread_local(source, Lang::Rust) {
        Ok(t) => t,
        Err(_) => return Vec::new(),
    };

    let ts_lang = Lang::Rust.tree_sitter_language();
    let query = match Query::new(&ts_lang, RUST_FIELDS_QUERY) {
        Ok(q) => q,
        Err(_) => return Vec::new(),
    };

    let mut cursor = QueryCursor::new();
    let root = tree.root_node();
    let mut matches = cursor.matches(&query, root, source.as_bytes());
    let mut details = Vec::new();

    let struct_name_idx = query.capture_index_for_name("struct_name");
    let field_name_idx = query.capture_index_for_name("field_name");
    let field_type_idx = query.capture_index_for_name("field_type");
    let enum_name_idx = query.capture_index_for_name("enum_name");
    let variant_name_idx = query.capture_index_for_name("variant_name");
    let impl_type_idx = query.capture_index_for_name("impl_type");
    let method_name_idx = query.capture_index_for_name("method_name");

    while let Some(m) = matches.next() {
        // Try struct fields
        if let (Some(sn_idx), Some(fn_idx)) = (struct_name_idx, field_name_idx) {
            let parent_cap = m.captures.iter().find(|c| c.index == sn_idx);
            let field_cap = m.captures.iter().find(|c| c.index == fn_idx);
            let type_cap = field_type_idx.and_then(|ti| m.captures.iter().find(|c| c.index == ti));

            if let (Some(parent), Some(field)) = (parent_cap, field_cap) {
                if node_text(source, parent.node) == symbol_name {
                    let field_node = field.node;
                    let is_pub = check_field_pub(source, field_node);
                    let type_text = type_cap.map(|c| node_text(source, c.node).to_string());
                    details.push(SymbolDetail {
                        name: node_text(source, field_node).to_string(),
                        kind: MemberKind::Field,
                        type_str: type_text,
                        is_pub,
                        line: field_node.start_position().row + 1,
                    });
                    continue;
                }
            }
        }

        // Try enum variants
        if let (Some(en_idx), Some(vn_idx)) = (enum_name_idx, variant_name_idx) {
            let parent_cap = m.captures.iter().find(|c| c.index == en_idx);
            let variant_cap = m.captures.iter().find(|c| c.index == vn_idx);

            if let (Some(parent), Some(variant)) = (parent_cap, variant_cap) {
                if node_text(source, parent.node) == symbol_name {
                    let variant_node = variant.node;
                    details.push(SymbolDetail {
                        name: node_text(source, variant_node).to_string(),
                        kind: MemberKind::Variant,
                        type_str: None,
                        is_pub: false,
                        line: variant_node.start_position().row + 1,
                    });
                    continue;
                }
            }
        }

        // Try impl methods
        if let (Some(it_idx), Some(mn_idx)) = (impl_type_idx, method_name_idx) {
            let parent_cap = m.captures.iter().find(|c| c.index == it_idx);
            let method_cap = m.captures.iter().find(|c| c.index == mn_idx);

            if let (Some(parent), Some(method)) = (parent_cap, method_cap) {
                if node_text(source, parent.node) == symbol_name {
                    let method_node = method.node;
                    let is_pub = check_fn_pub(source, method_node);
                    details.push(SymbolDetail {
                        name: node_text(source, method_node).to_string(),
                        kind: MemberKind::Method,
                        type_str: None,
                        is_pub,
                        line: method_node.start_position().row + 1,
                    });
                    continue;
                }
            }
        }
    }

    details
}

// ─── Python extraction ──────────────────────────────────────────────────

fn extract_python_details(source: &str, symbol_name: &str) -> Vec<SymbolDetail> {
    let tree = match parse_thread_local(source, Lang::Python) {
        Ok(t) => t,
        Err(_) => return Vec::new(),
    };

    let ts_lang = Lang::Python.tree_sitter_language();
    let query = match Query::new(&ts_lang, PYTHON_FIELDS_QUERY) {
        Ok(q) => q,
        Err(_) => return Vec::new(),
    };

    let mut cursor = QueryCursor::new();
    let root = tree.root_node();
    let mut matches = cursor.matches(&query, root, source.as_bytes());
    let mut details = Vec::new();
    let mut seen = std::collections::HashSet::new();

    // Capture indices for class methods
    let class_name_idx = query.capture_index_for_name("class_name");
    let method_name_idx = query.capture_index_for_name("method_name");

    // Capture indices for dataclass fields
    let dc_class_name_idx = query.capture_index_for_name("dc_class_name");
    let dc_field_name_idx = query.capture_index_for_name("dc_field_name");
    let dc_field_type_idx = query.capture_index_for_name("dc_field_type");

    // Capture indices for enum members / simple assignments
    let enum_class_name_idx = query.capture_index_for_name("enum_class_name");
    let enum_member_name_idx = query.capture_index_for_name("enum_member_name");

    while let Some(m) = matches.next() {
        // Class methods
        if let (Some(cn_idx), Some(mn_idx)) = (class_name_idx, method_name_idx) {
            let parent_cap = m.captures.iter().find(|c| c.index == cn_idx);
            let method_cap = m.captures.iter().find(|c| c.index == mn_idx);

            if let (Some(parent), Some(method)) = (parent_cap, method_cap) {
                if node_text(source, parent.node) == symbol_name {
                    let name = node_text(source, method.node).to_string();
                    let key = (name.clone(), MemberKind::Method);
                    if seen.insert(key) {
                        details.push(SymbolDetail {
                            name,
                            kind: MemberKind::Method,
                            type_str: None,
                            is_pub: !node_text(source, method.node).starts_with('_'),
                            line: method.node.start_position().row + 1,
                        });
                    }
                    continue;
                }
            }
        }

        // Dataclass fields (typed assignments)
        if let (Some(dcn_idx), Some(dfn_idx)) = (dc_class_name_idx, dc_field_name_idx) {
            let parent_cap = m.captures.iter().find(|c| c.index == dcn_idx);
            let field_cap = m.captures.iter().find(|c| c.index == dfn_idx);
            let type_cap =
                dc_field_type_idx.and_then(|ti| m.captures.iter().find(|c| c.index == ti));

            if let (Some(parent), Some(field)) = (parent_cap, field_cap) {
                if node_text(source, parent.node) == symbol_name {
                    let name = node_text(source, field.node).to_string();
                    let type_str = type_cap.map(|c| node_text(source, c.node).to_string());
                    let key = (name.clone(), MemberKind::Field);
                    if seen.insert(key) {
                        details.push(SymbolDetail {
                            name,
                            kind: MemberKind::Field,
                            type_str,
                            is_pub: true,
                            line: field.node.start_position().row + 1,
                        });
                    }
                    continue;
                }
            }
        }

        // Enum members / simple class-level assignments
        if let (Some(ecn_idx), Some(emn_idx)) = (enum_class_name_idx, enum_member_name_idx) {
            let parent_cap = m.captures.iter().find(|c| c.index == ecn_idx);
            let member_cap = m.captures.iter().find(|c| c.index == emn_idx);

            if let (Some(parent), Some(member)) = (parent_cap, member_cap) {
                if node_text(source, parent.node) == symbol_name {
                    let name = node_text(source, member.node).to_string();
                    // Determine if this is a variant (Enum member) or field
                    // by checking if the class inherits from Enum
                    let is_enum = is_python_enum_class(source, parent.node);
                    let kind = if is_enum {
                        MemberKind::Variant
                    } else {
                        MemberKind::Field
                    };
                    let key = (name.clone(), kind.clone());
                    if seen.insert(key) {
                        details.push(SymbolDetail {
                            name,
                            kind,
                            type_str: None,
                            is_pub: true,
                            line: member.node.start_position().row + 1,
                        });
                    }
                    continue;
                }
            }
        }
    }

    // Also extract self.field assignments from __init__
    extract_python_init_fields(source, symbol_name, &mut details, &mut seen);

    details
}

/// Check if a Python class inherits from Enum.
fn is_python_enum_class(source: &str, class_name_node: tree_sitter::Node) -> bool {
    // class_name_node is the identifier; parent is class_definition
    if let Some(class_def) = class_name_node.parent() {
        if let Some(bases) = class_def.child_by_field_name("superclasses") {
            let bases_text = node_text(source, bases);
            return bases_text.contains("Enum")
                || bases_text.contains("IntEnum")
                || bases_text.contains("StrEnum");
        }
    }
    false
}

/// Extract `self.field = ...` assignments from `__init__` method.
fn extract_python_init_fields(
    source: &str,
    symbol_name: &str,
    details: &mut Vec<SymbolDetail>,
    seen: &mut std::collections::HashSet<(String, MemberKind)>,
) {
    let tree = match parse_thread_local(source, Lang::Python) {
        Ok(t) => t,
        Err(_) => return,
    };

    // Walk the tree to find class_definition with matching name
    let root = tree.root_node();
    let mut stack = vec![root];

    while let Some(node) = stack.pop() {
        if node.kind() == "class_definition" {
            if let Some(name_node) = node.child_by_field_name("name") {
                if node_text(source, name_node) == symbol_name {
                    // Find __init__ method inside this class
                    find_init_self_fields(source, node, details, seen);
                }
            }
        }
        for i in 0..node.child_count() {
            if let Some(child) = node.child(i as u32) {
                stack.push(child);
            }
        }
    }
}

/// Walk inside a class_definition to find `__init__` and extract `self.x` assignments.
fn find_init_self_fields(
    source: &str,
    class_node: tree_sitter::Node,
    details: &mut Vec<SymbolDetail>,
    seen: &mut std::collections::HashSet<(String, MemberKind)>,
) {
    let mut stack = vec![class_node];

    while let Some(node) = stack.pop() {
        if node.kind() == "function_definition" {
            if let Some(name_node) = node.child_by_field_name("name") {
                if node_text(source, name_node) == "__init__" {
                    // Walk body for `self.x = ...` (assignment with attribute LHS)
                    collect_self_assignments(source, node, details, seen);
                    return;
                }
            }
        }
        for i in 0..node.child_count() {
            if let Some(child) = node.child(i as u32) {
                stack.push(child);
            }
        }
    }
}

/// Collect `self.field = value` patterns from a function body.
fn collect_self_assignments(
    source: &str,
    fn_node: tree_sitter::Node,
    details: &mut Vec<SymbolDetail>,
    seen: &mut std::collections::HashSet<(String, MemberKind)>,
) {
    let mut stack = vec![fn_node];

    while let Some(node) = stack.pop() {
        if node.kind() == "assignment" {
            if let Some(left) = node.child_by_field_name("left") {
                if left.kind() == "attribute" {
                    if let Some(obj) = left.child_by_field_name("object") {
                        if node_text(source, obj) == "self" {
                            if let Some(attr) = left.child_by_field_name("attribute") {
                                let name = node_text(source, attr).to_string();
                                let key = (name.clone(), MemberKind::Field);
                                if seen.insert(key) {
                                    details.push(SymbolDetail {
                                        name,
                                        kind: MemberKind::Field,
                                        type_str: None,
                                        is_pub: !node_text(source, attr).starts_with('_'),
                                        line: attr.start_position().row + 1,
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }
        for i in 0..node.child_count() {
            if let Some(child) = node.child(i as u32) {
                stack.push(child);
            }
        }
    }
}

// ─── TypeScript/JavaScript extraction ───────────────────────────────────

fn extract_ts_details(source: &str, symbol_name: &str, lang: Lang) -> Vec<SymbolDetail> {
    let tree = match parse_thread_local(source, lang) {
        Ok(t) => t,
        Err(_) => return Vec::new(),
    };

    let ts_lang = lang.tree_sitter_language();
    let query = match Query::new(&ts_lang, TS_FIELDS_QUERY) {
        Ok(q) => q,
        Err(_) => return Vec::new(),
    };

    let mut cursor = QueryCursor::new();
    let root = tree.root_node();
    let mut matches = cursor.matches(&query, root, source.as_bytes());
    let mut details = Vec::new();

    // Interface properties
    let iface_name_idx = query.capture_index_for_name("iface_name");
    let iface_prop_name_idx = query.capture_index_for_name("iface_prop_name");

    // Interface methods
    let iface_method_class_idx = query.capture_index_for_name("iface_method_class");
    let iface_method_name_idx = query.capture_index_for_name("iface_method_name");

    // Class fields
    let class_name_idx = query.capture_index_for_name("class_name");
    let class_field_name_idx = query.capture_index_for_name("class_field_name");

    // Class methods
    let class_method_class_idx = query.capture_index_for_name("class_method_class");
    let class_method_name_idx = query.capture_index_for_name("class_method_name");

    // Enum members with assignment (e.g., `Up = "UP"`)
    let enum_name_idx = query.capture_index_for_name("enum_name");
    let enum_member_name_idx = query.capture_index_for_name("enum_member_name");

    // Enum members without assignment (bare names like `Red`)
    let enum_bare_name_idx = query.capture_index_for_name("enum_bare_name");
    let enum_bare_member_name_idx = query.capture_index_for_name("enum_bare_member_name");

    while let Some(m) = matches.next() {
        // Interface properties
        if let (Some(in_idx), Some(pn_idx)) = (iface_name_idx, iface_prop_name_idx) {
            let parent_cap = m.captures.iter().find(|c| c.index == in_idx);
            let prop_cap = m.captures.iter().find(|c| c.index == pn_idx);

            if let (Some(parent), Some(prop)) = (parent_cap, prop_cap) {
                if node_text(source, parent.node) == symbol_name {
                    details.push(SymbolDetail {
                        name: node_text(source, prop.node).to_string(),
                        kind: MemberKind::Field,
                        type_str: None,
                        is_pub: true,
                        line: prop.node.start_position().row + 1,
                    });
                    continue;
                }
            }
        }

        // Interface methods
        if let (Some(imc_idx), Some(imn_idx)) = (iface_method_class_idx, iface_method_name_idx) {
            let parent_cap = m.captures.iter().find(|c| c.index == imc_idx);
            let method_cap = m.captures.iter().find(|c| c.index == imn_idx);

            if let (Some(parent), Some(method)) = (parent_cap, method_cap) {
                if node_text(source, parent.node) == symbol_name {
                    details.push(SymbolDetail {
                        name: node_text(source, method.node).to_string(),
                        kind: MemberKind::Method,
                        type_str: None,
                        is_pub: true,
                        line: method.node.start_position().row + 1,
                    });
                    continue;
                }
            }
        }

        // Class fields
        if let (Some(cn_idx), Some(cfn_idx)) = (class_name_idx, class_field_name_idx) {
            let parent_cap = m.captures.iter().find(|c| c.index == cn_idx);
            let field_cap = m.captures.iter().find(|c| c.index == cfn_idx);

            if let (Some(parent), Some(field)) = (parent_cap, field_cap) {
                if node_text(source, parent.node) == symbol_name {
                    details.push(SymbolDetail {
                        name: node_text(source, field.node).to_string(),
                        kind: MemberKind::Field,
                        type_str: None,
                        is_pub: true,
                        line: field.node.start_position().row + 1,
                    });
                    continue;
                }
            }
        }

        // Class methods
        if let (Some(cmc_idx), Some(cmn_idx)) = (class_method_class_idx, class_method_name_idx) {
            let parent_cap = m.captures.iter().find(|c| c.index == cmc_idx);
            let method_cap = m.captures.iter().find(|c| c.index == cmn_idx);

            if let (Some(parent), Some(method)) = (parent_cap, method_cap) {
                if node_text(source, parent.node) == symbol_name {
                    details.push(SymbolDetail {
                        name: node_text(source, method.node).to_string(),
                        kind: MemberKind::Method,
                        type_str: None,
                        is_pub: true,
                        line: method.node.start_position().row + 1,
                    });
                    continue;
                }
            }
        }

        // Enum members with assignment (e.g., `Up = "UP"`)
        if let (Some(en_idx), Some(emn_idx)) = (enum_name_idx, enum_member_name_idx) {
            let parent_cap = m.captures.iter().find(|c| c.index == en_idx);
            let member_cap = m.captures.iter().find(|c| c.index == emn_idx);

            if let (Some(parent), Some(member)) = (parent_cap, member_cap) {
                if node_text(source, parent.node) == symbol_name {
                    details.push(SymbolDetail {
                        name: node_text(source, member.node).to_string(),
                        kind: MemberKind::Variant,
                        type_str: None,
                        is_pub: true,
                        line: member.node.start_position().row + 1,
                    });
                    continue;
                }
            }
        }

        // Enum members without assignment (bare names like `Red`)
        if let (Some(ebn_idx), Some(ebmn_idx)) = (enum_bare_name_idx, enum_bare_member_name_idx) {
            let parent_cap = m.captures.iter().find(|c| c.index == ebn_idx);
            let member_cap = m.captures.iter().find(|c| c.index == ebmn_idx);

            if let (Some(parent), Some(member)) = (parent_cap, member_cap) {
                if node_text(source, parent.node) == symbol_name {
                    details.push(SymbolDetail {
                        name: node_text(source, member.node).to_string(),
                        kind: MemberKind::Variant,
                        type_str: None,
                        is_pub: true,
                        line: member.node.start_position().row + 1,
                    });
                    continue;
                }
            }
        }
    }

    details
}

/// Check if a field_declaration has `pub` visibility.
///
/// In tree-sitter-rust, a field_declaration with pub looks like:
///   (field_declaration (visibility_modifier) name: ... type: ...)
fn check_field_pub(source: &str, field_node: tree_sitter::Node) -> bool {
    // The field_node is the field_identifier; go to its parent (field_declaration)
    if let Some(parent) = field_node.parent() {
        for i in 0..parent.child_count() {
            if let Some(child) = parent.child(i as u32) {
                if child.kind() == "visibility_modifier" {
                    let text = node_text(source, child);
                    return text.starts_with("pub");
                }
            }
        }
    }
    false
}

/// Check if a function_item has `pub` visibility.
fn check_fn_pub(source: &str, method_name_node: tree_sitter::Node) -> bool {
    // method_name_node is the identifier; go to parent function_item
    if let Some(fn_item) = method_name_node.parent() {
        for i in 0..fn_item.child_count() {
            if let Some(child) = fn_item.child(i as u32) {
                if child.kind() == "visibility_modifier" {
                    let text = node_text(source, child);
                    return text.starts_with("pub");
                }
            }
        }
    }
    false
}

// ─── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_struct_fields() {
        let src = r#"
pub struct Foo {
    pub bar: String,
    baz: u32,
    opt: Option<Vec<i64>>,
}
"#;
        let details = extract_symbol_details(src, "Foo");
        assert_eq!(details.len(), 3, "expected 3 fields, got {:?}", details);
        assert!(
            details.iter().any(|d| d.name == "bar"
                && d.type_str.as_deref() == Some("String")
                && d.kind == MemberKind::Field
                && d.is_pub),
            "bar field not found or incorrect: {:?}",
            details
        );
        assert!(
            details
                .iter()
                .any(|d| d.name == "baz" && d.type_str.as_deref() == Some("u32") && !d.is_pub),
            "baz field not found: {:?}",
            details
        );
        assert!(
            details.iter().any(|d| d.name == "opt"),
            "opt field not found: {:?}",
            details
        );
    }

    #[test]
    fn test_extract_enum_variants() {
        let src = r#"
enum State {
    Idle,
    Running { id: u64 },
    Error(String),
}
"#;
        let details = extract_symbol_details(src, "State");
        assert!(
            details
                .iter()
                .any(|d| d.name == "Idle" && d.kind == MemberKind::Variant),
            "Idle variant not found: {:?}",
            details
        );
        assert!(
            details
                .iter()
                .any(|d| d.name == "Running" && d.kind == MemberKind::Variant),
            "Running variant not found: {:?}",
            details
        );
        assert!(
            details
                .iter()
                .any(|d| d.name == "Error" && d.kind == MemberKind::Variant),
            "Error variant not found: {:?}",
            details
        );
    }

    #[test]
    fn test_extract_impl_methods() {
        let src = r#"
struct Calculator;

impl Calculator {
    pub fn add(&self, a: u32, b: u32) -> u32 {
        a + b
    }

    fn internal_reset(&mut self) {
        // private
    }
}
"#;
        let details = extract_symbol_details(src, "Calculator");
        assert!(
            details
                .iter()
                .any(|d| d.name == "add" && d.kind == MemberKind::Method && d.is_pub),
            "add method not found: {:?}",
            details
        );
        assert!(
            details
                .iter()
                .any(|d| d.name == "internal_reset" && d.kind == MemberKind::Method && !d.is_pub),
            "internal_reset method not found: {:?}",
            details
        );
    }

    #[test]
    fn test_extract_nonexistent_symbol() {
        let src = "struct Foo { bar: u32 }";
        let details = extract_symbol_details(src, "NonExistent");
        assert!(details.is_empty());
    }

    #[test]
    fn test_extract_empty_struct() {
        let src = "struct Empty;";
        let details = extract_symbol_details(src, "Empty");
        assert!(details.is_empty());
    }

    // ─── Python tests ───────────────────────────────────────────────

    #[test]
    fn test_python_class_methods() {
        let src = r#"
class MyService:
    def __init__(self, name):
        self.name = name

    def process(self, data):
        return data

    def _internal(self):
        pass
"#;
        let details = extract_symbol_details_for_lang(src, "MyService", Lang::Python);
        assert!(
            details
                .iter()
                .any(|d| d.name == "__init__" && d.kind == MemberKind::Method),
            "__init__ not found: {:?}",
            details
        );
        assert!(
            details
                .iter()
                .any(|d| d.name == "process" && d.kind == MemberKind::Method),
            "process not found: {:?}",
            details
        );
        assert!(
            details
                .iter()
                .any(|d| d.name == "_internal" && d.kind == MemberKind::Method && !d.is_pub),
            "_internal should be private: {:?}",
            details
        );
    }

    #[test]
    fn test_python_init_fields() {
        let src = r#"
class User:
    def __init__(self, name, age):
        self.name = name
        self.age = age
        self._secret = "hidden"
"#;
        let details = extract_symbol_details_for_lang(src, "User", Lang::Python);
        assert!(
            details
                .iter()
                .any(|d| d.name == "name" && d.kind == MemberKind::Field),
            "name field not found: {:?}",
            details
        );
        assert!(
            details
                .iter()
                .any(|d| d.name == "age" && d.kind == MemberKind::Field),
            "age field not found: {:?}",
            details
        );
        assert!(
            details
                .iter()
                .any(|d| d.name == "_secret" && d.kind == MemberKind::Field && !d.is_pub),
            "_secret should be private field: {:?}",
            details
        );
    }

    #[test]
    fn test_python_enum_members() {
        let src = r#"
from enum import Enum

class Color(Enum):
    RED = 1
    GREEN = 2
    BLUE = 3
"#;
        let details = extract_symbol_details_for_lang(src, "Color", Lang::Python);
        assert!(
            details
                .iter()
                .any(|d| d.name == "RED" && d.kind == MemberKind::Variant),
            "RED variant not found: {:?}",
            details
        );
        assert!(
            details
                .iter()
                .any(|d| d.name == "GREEN" && d.kind == MemberKind::Variant),
            "GREEN variant not found: {:?}",
            details
        );
        assert!(
            details
                .iter()
                .any(|d| d.name == "BLUE" && d.kind == MemberKind::Variant),
            "BLUE variant not found: {:?}",
            details
        );
    }

    #[test]
    fn test_python_nonexistent() {
        let src = "class Foo:\n    pass\n";
        let details = extract_symbol_details_for_lang(src, "Bar", Lang::Python);
        assert!(details.is_empty());
    }

    // ─── TypeScript tests ───────────────────────────────────────────

    #[test]
    fn test_ts_interface_members() {
        let src = r#"
interface Config {
    host: string;
    port: number;
    debug: boolean;
}
"#;
        let details = extract_symbol_details_for_lang(src, "Config", Lang::TypeScript);
        assert!(
            details
                .iter()
                .any(|d| d.name == "host" && d.kind == MemberKind::Field),
            "host not found: {:?}",
            details
        );
        assert!(
            details
                .iter()
                .any(|d| d.name == "port" && d.kind == MemberKind::Field),
            "port not found: {:?}",
            details
        );
        assert!(
            details
                .iter()
                .any(|d| d.name == "debug" && d.kind == MemberKind::Field),
            "debug not found: {:?}",
            details
        );
    }

    #[test]
    fn test_ts_interface_methods() {
        let src = r#"
interface Service {
    start(): void;
    stop(): Promise<void>;
}
"#;
        let details = extract_symbol_details_for_lang(src, "Service", Lang::TypeScript);
        assert!(
            details
                .iter()
                .any(|d| d.name == "start" && d.kind == MemberKind::Method),
            "start not found: {:?}",
            details
        );
        assert!(
            details
                .iter()
                .any(|d| d.name == "stop" && d.kind == MemberKind::Method),
            "stop not found: {:?}",
            details
        );
    }

    #[test]
    fn test_ts_class_members() {
        let src = r#"
class UserService {
    name: string;
    count = 0;

    constructor(name: string) {
        this.name = name;
    }

    getCount() {
        return this.count;
    }
}
"#;
        let details = extract_symbol_details_for_lang(src, "UserService", Lang::TypeScript);
        assert!(
            details
                .iter()
                .any(|d| d.name == "name" && d.kind == MemberKind::Field),
            "name field not found: {:?}",
            details
        );
        assert!(
            details
                .iter()
                .any(|d| d.name == "getCount" && d.kind == MemberKind::Method),
            "getCount method not found: {:?}",
            details
        );
    }

    #[test]
    fn test_ts_enum_members() {
        let src = r#"
enum Direction {
    Up = "UP",
    Down = "DOWN",
    Left = "LEFT",
    Right = "RIGHT",
}
"#;
        let details = extract_symbol_details_for_lang(src, "Direction", Lang::TypeScript);
        assert!(
            details
                .iter()
                .any(|d| d.name == "Up" && d.kind == MemberKind::Variant),
            "Up not found: {:?}",
            details
        );
        assert!(
            details
                .iter()
                .any(|d| d.name == "Down" && d.kind == MemberKind::Variant),
            "Down not found: {:?}",
            details
        );
        assert_eq!(details.len(), 4, "expected 4 variants: {:?}", details);
    }

    #[test]
    fn test_ts_nonexistent() {
        let src = "interface Foo { x: number; }";
        let details = extract_symbol_details_for_lang(src, "Bar", Lang::TypeScript);
        assert!(details.is_empty());
    }

    #[test]
    fn test_unsupported_lang() {
        let details = extract_symbol_details_for_lang("code", "Foo", Lang::Bash);
        assert!(details.is_empty());
    }
}
