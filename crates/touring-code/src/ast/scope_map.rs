//! Scope-Aware Symbol Resolution — Lexical scope tracking.
//!
//! Builds a map of local variable definitions (let bindings, function
//! parameters, for-loop variables, match bindings) from source code
//! using tree-sitter. Enables detection of variable shadowing and
//! identification of unresolved references needing imports.

use serde::{Deserialize, Serialize};
use streaming_iterator::StreamingIterator;
use tree_sitter::{Query, QueryCursor};

use crate::ast::languages::Lang;
use crate::ast::node_text;
use crate::ast::parser::parse_thread_local;

// ─── Types ──────────────────────────────────────────────────────────────

/// Kind of scope entry (how the variable was introduced).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScopeKind {
    /// `let x = ...;` or `let x: T = ...;`
    Let,
    /// Function parameter
    Param,
    /// `for x in ...`
    For,
    /// Match arm binding
    Match,
}

/// A single variable definition in a scope.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScopeEntry {
    /// Variable name
    pub name: String,
    /// Type annotation if present (e.g., `"u32"`)
    pub type_str: Option<String>,
    /// How this variable was introduced
    pub kind: ScopeKind,
    /// Line number (1-indexed)
    pub line: usize,
}

/// Map of all scope entries in a source file.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ScopeMap {
    /// All scope entries found
    pub entries: Vec<ScopeEntry>,
}

// ─── Tree-sitter queries ────────────────────────────────────────────────

const RUST_SCOPE_QUERY: &str = r#"
;; let bindings with identifier pattern
(let_declaration
  pattern: (identifier) @let_name
  type: (_)? @let_type)

;; Function parameters
(parameter
  pattern: (identifier) @param_name)

;; For loop variables
(for_expression
  pattern: (identifier) @for_name)
"#;

const PYTHON_SCOPE_QUERY: &str = r#"
;; Assignment targets
(assignment
  left: (identifier) @assign_name)

;; Function parameters
(function_definition
  parameters: (parameters
    (identifier) @param_name))
"#;

/// TypeScript scope query (TSX grammar — uses `required_parameter` for typed params).
/// In the TSX grammar, `formal_parameters` wraps each param in
/// `required_parameter` (with field `pattern:`). Bare `(identifier)` inside
/// `formal_parameters` is NOT valid in TSX — it causes a Query compile error.
const TS_SCOPE_QUERY: &str = r#"
;; let/const/var declarations
(variable_declarator
  name: (identifier) @let_name)

;; Function parameters (typed — required_parameter wraps identifier)
(formal_parameters
  (required_parameter
    pattern: (identifier) @param_name))

;; For-of / for-in loop variables
(for_in_statement
  left: (identifier) @for_name)

;; Import bindings
(import_specifier
  name: (identifier) @import_name)
"#;

/// JavaScript scope query (JS grammar — no `required_parameter`; params are
/// bare identifiers directly inside `formal_parameters`).
const JS_SCOPE_QUERY: &str = r#"
;; let/const/var declarations
(variable_declarator
  name: (identifier) @let_name)

;; Function parameters (bare identifier in JS)
(formal_parameters
  (identifier) @param_name)

;; For-of / for-in loop variables
(for_in_statement
  left: (identifier) @for_name)

;; Import bindings
(import_specifier
  name: (identifier) @import_name)
"#;

// ─── Implementation ─────────────────────────────────────────────────────

/// Build a scope map from source code.
///
/// Extracts all local variable definitions (let bindings, function parameters,
/// for-loop variables) from the given source code.
///
/// # Arguments
/// * `source` - Source code to analyze
/// * `language` - Language identifier (`"rust"` or `"python"`)
///
/// # Returns
/// A [`ScopeMap`] containing all found scope entries.
pub fn build_scope_map(source: &str, lang: Lang) -> ScopeMap {
    match lang {
        Lang::Rust => build_rust_scope_map(source),
        Lang::Python => build_python_scope_map(source),
        Lang::TypeScript | Lang::JavaScript => build_ts_scope_map(source, lang),
        _ => ScopeMap::default(),
    }
}

/// If `cap_idx` names a capture present in match `m`, push a childless
/// [`ScopeEntry`] of `kind` for it and return `true`. Centralizes the
/// find-capture → push-entry step that every per-language builder repeats for
/// parameters, for-variables, imports, and simple bindings (the type-annotated
/// Rust `let` path keeps its own body because it also resolves `let_type`).
fn push_capture(
    entries: &mut Vec<ScopeEntry>,
    m: &tree_sitter::QueryMatch<'_, '_>,
    source: &str,
    cap_idx: Option<u32>,
    kind: ScopeKind,
) -> bool {
    if let Some(idx) = cap_idx {
        if let Some(cap) = m.captures.iter().find(|c| c.index == idx) {
            entries.push(ScopeEntry {
                name: node_text(source, cap.node).to_string(),
                type_str: None,
                kind,
                line: cap.node.start_position().row + 1,
            });
            return true;
        }
    }
    false
}

fn build_rust_scope_map(source: &str) -> ScopeMap {
    let tree = match parse_thread_local(source, Lang::Rust) {
        Ok(t) => t,
        Err(_) => return ScopeMap::default(),
    };

    let ts_lang = Lang::Rust.tree_sitter_language();
    let query = match Query::new(&ts_lang, RUST_SCOPE_QUERY) {
        Ok(q) => q,
        Err(_) => return ScopeMap::default(),
    };

    let mut cursor = QueryCursor::new();
    let root = tree.root_node();
    let mut matches = cursor.matches(&query, root, source.as_bytes());
    let mut entries = Vec::new();

    let let_name_idx = query.capture_index_for_name("let_name");
    let let_type_idx = query.capture_index_for_name("let_type");
    let param_name_idx = query.capture_index_for_name("param_name");
    let for_name_idx = query.capture_index_for_name("for_name");

    while let Some(m) = matches.next() {
        // Let bindings
        if let Some(ln_idx) = let_name_idx {
            if let Some(cap) = m.captures.iter().find(|c| c.index == ln_idx) {
                let name = node_text(source, cap.node).to_string();
                let type_str = let_type_idx
                    .and_then(|ti| m.captures.iter().find(|c| c.index == ti))
                    .map(|c| node_text(source, c.node).to_string());
                entries.push(ScopeEntry {
                    name,
                    type_str,
                    kind: ScopeKind::Let,
                    line: cap.node.start_position().row + 1,
                });
                continue;
            }
        }

        // Parameters
        if push_capture(&mut entries, m, source, param_name_idx, ScopeKind::Param) {
            continue;
        }

        // For variables
        if push_capture(&mut entries, m, source, for_name_idx, ScopeKind::For) {
            continue;
        }
    }

    ScopeMap { entries }
}

fn build_python_scope_map(source: &str) -> ScopeMap {
    let tree = match parse_thread_local(source, Lang::Python) {
        Ok(t) => t,
        Err(_) => return ScopeMap::default(),
    };

    let ts_lang = Lang::Python.tree_sitter_language();
    let query = match Query::new(&ts_lang, PYTHON_SCOPE_QUERY) {
        Ok(q) => q,
        Err(_) => return ScopeMap::default(),
    };

    let mut cursor = QueryCursor::new();
    let root = tree.root_node();
    let mut matches = cursor.matches(&query, root, source.as_bytes());
    let mut entries = Vec::new();

    let assign_name_idx = query.capture_index_for_name("assign_name");
    let param_name_idx = query.capture_index_for_name("param_name");

    while let Some(m) = matches.next() {
        if push_capture(&mut entries, m, source, assign_name_idx, ScopeKind::Let) {
            continue;
        }

        if push_capture(&mut entries, m, source, param_name_idx, ScopeKind::Param) {
            continue;
        }
    }

    ScopeMap { entries }
}

fn build_ts_scope_map(source: &str, lang: Lang) -> ScopeMap {
    let tree = match parse_thread_local(source, lang) {
        Ok(t) => t,
        Err(_) => return ScopeMap::default(),
    };

    let ts_lang = lang.tree_sitter_language();

    // Select the correct query based on language — TSX and JS grammars differ
    // in how function parameters are represented.
    let query_src = match lang {
        Lang::JavaScript => JS_SCOPE_QUERY,
        _ => TS_SCOPE_QUERY,
    };

    let query = match Query::new(&ts_lang, query_src) {
        Ok(q) => q,
        Err(_) => return ScopeMap::default(),
    };

    let mut cursor = QueryCursor::new();
    let root = tree.root_node();
    let mut matches = cursor.matches(&query, root, source.as_bytes());
    let mut entries = Vec::new();

    let let_name_idx = query.capture_index_for_name("let_name");
    let param_name_idx = query.capture_index_for_name("param_name");
    let for_name_idx = query.capture_index_for_name("for_name");
    let import_name_idx = query.capture_index_for_name("import_name");

    while let Some(m) = matches.next() {
        // Variable declarations (let/const/var)
        if push_capture(&mut entries, m, source, let_name_idx, ScopeKind::Let) {
            continue;
        }

        // Function parameters (required_parameter in TS, bare identifier in JS)
        if push_capture(&mut entries, m, source, param_name_idx, ScopeKind::Param) {
            continue;
        }

        // For-of / for-in variables
        if push_capture(&mut entries, m, source, for_name_idx, ScopeKind::For) {
            continue;
        }

        // Import bindings
        // imports act like const bindings
        if push_capture(&mut entries, m, source, import_name_idx, ScopeKind::Let) {
            continue;
        }
    }

    ScopeMap { entries }
}

// ─── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scope_map_let_bindings() {
        let src = r#"
fn main() {
    let x: u32 = 42;
    let y = String::from("hello");
    for item in vec![1, 2, 3] {
        let z = item + 1;
    }
}
"#;
        let scope = build_scope_map(src, Lang::Rust);
        assert!(
            scope
                .entries
                .iter()
                .any(|e| e.name == "x" && e.kind == ScopeKind::Let),
            "x not found: {:?}",
            scope.entries
        );
        assert!(
            scope
                .entries
                .iter()
                .any(|e| e.name == "y" && e.kind == ScopeKind::Let),
            "y not found: {:?}",
            scope.entries
        );
        assert!(
            scope
                .entries
                .iter()
                .any(|e| e.name == "item" && e.kind == ScopeKind::For),
            "item not found: {:?}",
            scope.entries
        );
        assert!(
            scope
                .entries
                .iter()
                .any(|e| e.name == "z" && e.kind == ScopeKind::Let),
            "z not found: {:?}",
            scope.entries
        );
    }

    #[test]
    fn test_scope_map_function_params() {
        let src = r#"
fn process(input: &str, count: usize) -> bool {
    let result = count > 0;
    result
}
"#;
        let scope = build_scope_map(src, Lang::Rust);
        assert!(
            scope
                .entries
                .iter()
                .any(|e| e.name == "input" && e.kind == ScopeKind::Param),
            "input param not found: {:?}",
            scope.entries
        );
        assert!(
            scope
                .entries
                .iter()
                .any(|e| e.name == "count" && e.kind == ScopeKind::Param),
            "count param not found: {:?}",
            scope.entries
        );
    }

    #[test]
    fn test_scope_map_empty() {
        let scope = build_scope_map("", Lang::Rust);
        assert!(scope.entries.is_empty());
    }

    #[test]
    fn test_scope_map_unsupported_language() {
        let scope = build_scope_map("some code", Lang::Bash);
        assert!(scope.entries.is_empty());
    }

    #[test]
    fn test_scope_map_python() {
        let src = r#"
def process(x, y):
    result = x + y
    return result
"#;
        let scope = build_scope_map(src, Lang::Python);
        assert!(
            scope
                .entries
                .iter()
                .any(|e| e.name == "x" && e.kind == ScopeKind::Param),
            "x param not found: {:?}",
            scope.entries
        );
        assert!(
            scope
                .entries
                .iter()
                .any(|e| e.name == "result" && e.kind == ScopeKind::Let),
            "result not found: {:?}",
            scope.entries
        );
    }

    // ─── TypeScript/JavaScript tests ────────────────────────────────

    #[test]
    fn test_scope_map_ts_let_const() {
        let src = r#"
function main() {
    const x = 42;
    let y = "hello";
    var z = true;
}
"#;
        let scope = build_scope_map(src, Lang::TypeScript);
        assert!(
            scope
                .entries
                .iter()
                .any(|e| e.name == "x" && e.kind == ScopeKind::Let),
            "x not found: {:?}",
            scope.entries
        );
        assert!(
            scope
                .entries
                .iter()
                .any(|e| e.name == "y" && e.kind == ScopeKind::Let),
            "y not found: {:?}",
            scope.entries
        );
        assert!(
            scope
                .entries
                .iter()
                .any(|e| e.name == "z" && e.kind == ScopeKind::Let),
            "z not found: {:?}",
            scope.entries
        );
    }

    #[test]
    fn test_scope_map_ts_params() {
        let src = r#"
function process(input: string, count: number): boolean {
    const result = count > 0;
    return result;
}
"#;
        let scope = build_scope_map(src, Lang::TypeScript);
        assert!(
            scope
                .entries
                .iter()
                .any(|e| e.name == "input" && e.kind == ScopeKind::Param),
            "input param not found: {:?}",
            scope.entries
        );
        assert!(
            scope
                .entries
                .iter()
                .any(|e| e.name == "count" && e.kind == ScopeKind::Param),
            "count param not found: {:?}",
            scope.entries
        );
    }

    #[test]
    fn test_scope_map_ts_for_in() {
        let src = r#"
const items = [1, 2, 3];
for (const item of items) {
    console.log(item);
}
"#;
        let scope = build_scope_map(src, Lang::TypeScript);
        // 'item' declared inside for-of via variable_declarator
        assert!(
            scope.entries.iter().any(|e| e.name == "items"),
            "items not found: {:?}",
            scope.entries
        );
    }

    #[test]
    fn test_scope_map_ts_imports() {
        let src = r#"
import { useState, useEffect } from 'react';
const count = 0;
"#;
        let scope = build_scope_map(src, Lang::TypeScript);
        assert!(
            scope.entries.iter().any(|e| e.name == "useState"),
            "useState import not found: {:?}",
            scope.entries
        );
        assert!(
            scope.entries.iter().any(|e| e.name == "useEffect"),
            "useEffect import not found: {:?}",
            scope.entries
        );
    }

    #[test]
    fn test_scope_map_js() {
        let src = r#"
const x = 1;
let y = 2;
"#;
        let scope = build_scope_map(src, Lang::JavaScript);
        assert!(
            scope
                .entries
                .iter()
                .any(|e| e.name == "x" && e.kind == ScopeKind::Let),
            "JS x not found: {:?}",
            scope.entries
        );
        assert!(
            scope
                .entries
                .iter()
                .any(|e| e.name == "y" && e.kind == ScopeKind::Let),
            "JS y not found: {:?}",
            scope.entries
        );
    }
}
