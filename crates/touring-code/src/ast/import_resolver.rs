//! Import Resolution Semantica — Extract and validate imports.
//!
//! Extracts `use` statements from Rust source and `import`/`from` statements
//! from Python source. Provides detection of missing imports by comparing
//! used identifiers against known types.

use serde::{Deserialize, Serialize};
use streaming_iterator::StreamingIterator;
use tree_sitter::{Query, QueryCursor};

use crate::ast::languages::Lang;
use crate::ast::node_text;
use crate::ast::parser::parse_thread_local;

// ─── Types ──────────────────────────────────────────────────────────────

/// A resolved import statement.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedImport {
    /// Full import path (e.g., `"std::collections::HashMap"`)
    pub path: String,
    /// Alias if renamed (e.g., `use Foo as Bar` → alias = Some("Bar"))
    pub alias: Option<String>,
    /// Whether this is a glob import (`use module::*`)
    pub is_glob: bool,
    /// Line number (1-indexed)
    pub line: usize,
}

/// Collection of imports extracted from a source file.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ImportResolver {
    /// All imports found in the source
    pub imports: Vec<ResolvedImport>,
}

// ─── Tree-sitter queries ────────────────────────────────────────────────

const RUST_IMPORT_QUERY: &str = r#"
;; All use declarations — capture the full argument
(use_declaration
  argument: (_) @use_arg) @use_decl
"#;

const PYTHON_IMPORT_QUERY: &str = r#"
;; import module
(import_statement
  name: (dotted_name) @import_name) @import_stmt

;; from module import name
(import_from_statement
  module_name: (dotted_name) @from_module
  name: (dotted_name) @from_name)
"#;

// ─── Implementation ─────────────────────────────────────────────────────

/// Extract imports from source code.
///
/// Supports Rust (`use` statements) and Python (`import`/`from ... import`).
///
/// # Arguments
/// * `source` - Source code to analyze
/// * `language` - Language identifier (`"rust"` or `"python"`)
pub fn extract_imports_resolved(source: &str, lang: Lang) -> ImportResolver {
    match lang {
        Lang::Rust => extract_rust_imports(source),
        Lang::Python => extract_python_imports(source),
        Lang::TypeScript | Lang::JavaScript => extract_ts_imports(source, lang),
        _ => ImportResolver::default(),
    }
}

impl ImportResolver {
    /// Detect identifiers used in `source` that are in `known_types` but
    /// not present in the current import list.
    ///
    /// # Arguments
    /// * `source` - Source code to scan for type usage
    /// * `known_types` - List of type names that require imports
    ///
    /// # Returns
    /// Vector of type names that are used but not imported.
    pub fn detect_missing_imports(&self, source: &str, known_types: &[&str]) -> Vec<String> {
        let mut missing = Vec::new();

        for &type_name in known_types {
            // Check if this type appears in the source
            if !source.contains(type_name) {
                continue;
            }

            // Check if it's already imported
            let is_imported = self.imports.iter().any(|imp| {
                // Check if the path ends with the type name
                imp.path.ends_with(type_name)
                    || imp.alias.as_deref() == Some(type_name)
                    || imp.is_glob // glob imports might cover it
            });

            if !is_imported {
                missing.push(type_name.to_string());
            }
        }

        missing
    }
}

fn extract_rust_imports(source: &str) -> ImportResolver {
    let tree = match parse_thread_local(source, Lang::Rust) {
        Ok(t) => t,
        Err(_) => return ImportResolver::default(),
    };

    let ts_lang = Lang::Rust.tree_sitter_language();
    let query = match Query::new(&ts_lang, RUST_IMPORT_QUERY) {
        Ok(q) => q,
        Err(_) => return ImportResolver::default(),
    };

    let mut cursor = QueryCursor::new();
    let root = tree.root_node();
    let mut matches = cursor.matches(&query, root, source.as_bytes());
    let mut imports = Vec::new();

    let use_arg_idx = query.capture_index_for_name("use_arg");

    while let Some(m) = matches.next() {
        if let Some(ua_idx) = use_arg_idx {
            if let Some(cap) = m.captures.iter().find(|c| c.index == ua_idx) {
                let text = node_text(source, cap.node).to_string();
                let line = cap.node.start_position().row + 1;
                let is_glob = text.ends_with("::*") || text.ends_with("*");

                imports.push(ResolvedImport {
                    path: text,
                    alias: None,
                    is_glob,
                    line,
                });
            }
        }
    }

    ImportResolver { imports }
}

fn extract_python_imports(source: &str) -> ImportResolver {
    let tree = match parse_thread_local(source, Lang::Python) {
        Ok(t) => t,
        Err(_) => return ImportResolver::default(),
    };

    let ts_lang = Lang::Python.tree_sitter_language();
    let query = match Query::new(&ts_lang, PYTHON_IMPORT_QUERY) {
        Ok(q) => q,
        Err(_) => return ImportResolver::default(),
    };

    let mut cursor = QueryCursor::new();
    let root = tree.root_node();
    let mut matches = cursor.matches(&query, root, source.as_bytes());
    let mut imports = Vec::new();

    let import_name_idx = query.capture_index_for_name("import_name");
    let from_module_idx = query.capture_index_for_name("from_module");
    let from_name_idx = query.capture_index_for_name("from_name");

    while let Some(m) = matches.next() {
        // Bare import
        if let Some(in_idx) = import_name_idx {
            if let Some(cap) = m.captures.iter().find(|c| c.index == in_idx) {
                let text = node_text(source, cap.node).to_string();
                imports.push(ResolvedImport {
                    path: text,
                    alias: None,
                    is_glob: false,
                    line: cap.node.start_position().row + 1,
                });
                continue;
            }
        }

        // From import
        if let (Some(fm_idx), Some(fn_idx)) = (from_module_idx, from_name_idx) {
            let mod_cap = m.captures.iter().find(|c| c.index == fm_idx);
            let name_cap = m.captures.iter().find(|c| c.index == fn_idx);

            if let (Some(mod_c), Some(name_c)) = (mod_cap, name_cap) {
                let module = node_text(source, mod_c.node);
                let name = node_text(source, name_c.node);
                imports.push(ResolvedImport {
                    path: format!("{module}.{name}"),
                    alias: None,
                    is_glob: false,
                    line: name_c.node.start_position().row + 1,
                });
                continue;
            }
        }
    }

    ImportResolver { imports }
}

fn extract_ts_imports(source: &str, lang: Lang) -> ImportResolver {
    let tree = match parse_thread_local(source, lang) {
        Ok(t) => t,
        Err(_) => return ImportResolver::default(),
    };

    // Use a tree walk approach since TS/JS import AST varies across grammars.
    let root = tree.root_node();
    let mut imports = Vec::new();
    let mut stack = vec![root];

    while let Some(node) = stack.pop() {
        match node.kind() {
            // import { X, Y } from 'module'  or  import X from 'module'
            "import_statement" => {
                let source_node = node.child_by_field_name("source");
                let module_path = source_node
                    .map(|n| {
                        node_text(source, n)
                            .trim_matches(|c| c == '\'' || c == '"')
                            .to_string()
                    })
                    .unwrap_or_default();
                let line = node.start_position().row + 1;

                // Check for named imports: import { A, B } from '...'
                let mut found_named = false;
                for i in 0..node.named_child_count() {
                    if let Some(child) = node.named_child(i as u32) {
                        if child.kind() == "import_clause" {
                            // Default import: import X from '...'
                            if let Some(id) = child.child_by_field_name("default") {
                                let name = node_text(source, id).to_string();
                                imports.push(ResolvedImport {
                                    path: format!("{module_path}.{name}"),
                                    alias: None,
                                    is_glob: false,
                                    line,
                                });
                                found_named = true;
                            }
                            // Named imports: { A, B }
                            for j in 0..child.named_child_count() {
                                if let Some(named) = child.named_child(j as u32) {
                                    if named.kind() == "named_imports" {
                                        for k in 0..named.named_child_count() {
                                            if let Some(spec) = named.named_child(k as u32) {
                                                if spec.kind() == "import_specifier" {
                                                    let imp_name = if let Some(alias_node) =
                                                        spec.child_by_field_name("alias")
                                                    {
                                                        let original = spec
                                                            .child_by_field_name("name")
                                                            .map(|n| {
                                                                node_text(source, n).to_string()
                                                            })
                                                            .unwrap_or_default();
                                                        let alias = node_text(source, alias_node)
                                                            .to_string();
                                                        imports.push(ResolvedImport {
                                                            path: format!(
                                                                "{module_path}.{original}"
                                                            ),
                                                            alias: Some(alias),
                                                            is_glob: false,
                                                            line,
                                                        });
                                                        found_named = true;
                                                        continue;
                                                    } else if let Some(name_node) =
                                                        spec.child_by_field_name("name")
                                                    {
                                                        node_text(source, name_node).to_string()
                                                    } else {
                                                        node_text(source, spec).to_string()
                                                    };
                                                    imports.push(ResolvedImport {
                                                        path: format!("{module_path}.{imp_name}"),
                                                        alias: None,
                                                        is_glob: false,
                                                        line,
                                                    });
                                                    found_named = true;
                                                }
                                            }
                                        }
                                    }
                                    // Namespace import: import * as X from '...'
                                    if named.kind() == "namespace_import" {
                                        let alias_name = named
                                            .child_by_field_name("name")
                                            .or_else(|| {
                                                // Sometimes the identifier is a direct child
                                                for c in 0..named.named_child_count() {
                                                    if let Some(ch) = named.named_child(c as u32) {
                                                        if ch.kind() == "identifier" {
                                                            return Some(ch);
                                                        }
                                                    }
                                                }
                                                None
                                            })
                                            .map(|n| node_text(source, n).to_string());
                                        imports.push(ResolvedImport {
                                            path: module_path.clone(),
                                            alias: alias_name,
                                            is_glob: true,
                                            line,
                                        });
                                        found_named = true;
                                    }
                                }
                            }
                        }
                    }
                }

                // Fallback: if we couldn't find named children, record the whole import
                if !found_named && !module_path.is_empty() {
                    imports.push(ResolvedImport {
                        path: module_path,
                        alias: None,
                        is_glob: false,
                        line,
                    });
                }
            }
            _ => {
                // Push children for traversal (only top-level)
                for i in 0..node.child_count() {
                    if let Some(child) = node.child(i as u32) {
                        stack.push(child);
                    }
                }
            }
        }
    }

    ImportResolver { imports }
}

// ─── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_rust_imports() {
        let src = r#"
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use crate::ast::types::MyType;
"#;
        let resolver = extract_imports_resolved(src, Lang::Rust);
        assert!(!resolver.imports.is_empty(), "should have found imports");
        assert!(
            resolver.imports.iter().any(|i| i.path.contains("HashMap")),
            "HashMap not found in imports: {:?}",
            resolver.imports
        );
        assert!(
            resolver.imports.iter().any(|i| i.path.contains("MyType")),
            "MyType not found in imports: {:?}",
            resolver.imports
        );
    }

    #[test]
    fn test_extract_rust_glob_import() {
        let src = "use std::prelude::*;\n";
        let resolver = extract_imports_resolved(src, Lang::Rust);
        assert!(
            resolver.imports.iter().any(|i| i.is_glob),
            "glob import not detected: {:?}",
            resolver.imports
        );
    }

    #[test]
    fn test_detect_missing_imports() {
        let src = r#"
fn main() {
    let map: HashMap<String, u32> = HashMap::new();
    let set: HashSet<u32> = HashSet::new();
}
"#;
        let resolver = extract_imports_resolved(src, Lang::Rust);
        let missing = resolver.detect_missing_imports(src, &["HashMap", "HashSet", "Vec"]);
        assert!(
            missing.contains(&"HashMap".to_string()),
            "HashMap should be missing: {:?}",
            missing
        );
        assert!(
            missing.contains(&"HashSet".to_string()),
            "HashSet should be missing: {:?}",
            missing
        );
        // Vec is not used in source, so should not be missing
        assert!(
            !missing.contains(&"Vec".to_string()),
            "Vec should not be missing: {:?}",
            missing
        );
    }

    #[test]
    fn test_detect_no_missing_when_imported() {
        let src = r#"
use std::collections::HashMap;

fn main() {
    let map: HashMap<String, u32> = HashMap::new();
}
"#;
        let resolver = extract_imports_resolved(src, Lang::Rust);
        let missing = resolver.detect_missing_imports(src, &["HashMap"]);
        assert!(
            missing.is_empty(),
            "HashMap is imported, should not be missing: {:?}",
            missing
        );
    }

    #[test]
    fn test_extract_python_imports() {
        let src = r#"
import os
from pathlib import Path
from collections import OrderedDict
"#;
        let resolver = extract_imports_resolved(src, Lang::Python);
        assert!(
            !resolver.imports.is_empty(),
            "should have found imports: {:?}",
            resolver.imports
        );
        assert!(
            resolver.imports.iter().any(|i| i.path == "os"),
            "os import not found: {:?}",
            resolver.imports
        );
    }

    #[test]
    fn test_unsupported_language() {
        let resolver = extract_imports_resolved("code", Lang::Bash);
        assert!(resolver.imports.is_empty());
    }

    // ─── TypeScript/JavaScript tests ────────────────────────────────

    #[test]
    fn test_extract_ts_named_imports() {
        let src = r#"
import { useState, useEffect } from 'react';
import { readFile } from 'fs/promises';
"#;
        let resolver = extract_imports_resolved(src, Lang::TypeScript);
        assert!(
            !resolver.imports.is_empty(),
            "should have found TS imports: {:?}",
            resolver.imports
        );
        assert!(
            resolver.imports.iter().any(|i| i.path.contains("useState")),
            "useState not found: {:?}",
            resolver.imports
        );
        assert!(
            resolver.imports.iter().any(|i| i.path.contains("readFile")),
            "readFile not found: {:?}",
            resolver.imports
        );
    }

    #[test]
    fn test_extract_ts_default_import() {
        let src = r#"import React from 'react';"#;
        let resolver = extract_imports_resolved(src, Lang::TypeScript);
        assert!(
            !resolver.imports.is_empty(),
            "default import should be found: {:?}",
            resolver.imports
        );
        assert!(
            resolver
                .imports
                .iter()
                .any(|i| i.path.contains("React") || i.path.contains("react")),
            "React default import not found: {:?}",
            resolver.imports
        );
    }

    #[test]
    fn test_extract_ts_namespace_import() {
        let src = r#"import * as fs from 'fs';"#;
        let resolver = extract_imports_resolved(src, Lang::TypeScript);
        assert!(
            resolver.imports.iter().any(|i| i.is_glob),
            "namespace import not detected as glob: {:?}",
            resolver.imports
        );
    }

    #[test]
    fn test_extract_js_imports() {
        let src = r#"
import { join } from 'path';
const x = 1;
"#;
        let resolver = extract_imports_resolved(src, Lang::JavaScript);
        assert!(
            !resolver.imports.is_empty(),
            "JS imports should be found: {:?}",
            resolver.imports
        );
        assert!(
            resolver.imports.iter().any(|i| i.path.contains("join")),
            "join not found: {:?}",
            resolver.imports
        );
    }

    #[test]
    fn test_extract_ts_empty() {
        let resolver = extract_imports_resolved("const x = 1;", Lang::TypeScript);
        assert!(
            resolver.imports.is_empty(),
            "no imports expected: {:?}",
            resolver.imports
        );
    }
}
