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

            // Check if it's already imported. The name in scope is the LAST
            // path segment (`::` Rust, `.` Python) — an `ends_with` let
            // "…::TfIdfVectorizer" vouch for a type called "Vectorizer" — and
            // an aliased import puts only the alias in scope.
            let is_imported = self.imports.iter().any(|imp| {
                imp.is_glob // glob imports might cover it
                    || match imp.alias.as_deref() {
                        Some(alias) => alias == type_name,
                        None => last_path_segment(&imp.path) == type_name,
                    }
            });

            if !is_imported {
                missing.push(type_name.to_string());
            }
        }

        missing
    }

    /// PascalCase type references in `source` that nothing brings into scope:
    /// not imported (per this resolver — grouped lists, aliases and globs
    /// count), not declared in the file (`local_declared`, see
    /// [`declared_type_names`]), not a builtin, and used UNQUALIFIED at least
    /// once (`search::Foo::new()` alone needs no `use`).
    ///
    /// S3 (2026-09-02): the pre-write form of `pre_edit`'s detector — the
    /// imports come from the text itself, so a file that does not exist yet
    /// (no FileKnowledgeDB row) is analysable.
    pub fn unresolved_type_references(&self, source: &str, local_declared: &[String]) -> Vec<String> {
        let mut candidates: Vec<&str> = Vec::new();
        for (start, word) in identifier_tokens(source) {
            if !looks_like_type_name(word) || is_builtin_type_name(word) {
                continue;
            }
            if local_declared.iter().any(|l| l == word) {
                continue;
            }
            // A path-qualified occurrence (`search::Foo`) needs no `use`; only
            // a bare occurrence somewhere makes the name a candidate.
            if source[..start].ends_with("::") {
                continue;
            }
            if !candidates.contains(&word) {
                candidates.push(word);
            }
        }
        if candidates.is_empty() {
            return Vec::new();
        }
        self.detect_missing_imports(source, &candidates)
    }
}

/// Names declared by `struct`/`enum`/`trait`/`type`/`union` items in `source`
/// (any visibility) — the file's own types never need importing.
///
/// Token-based on purpose: it must work on a draft that may not parse yet,
/// and the only false positive shape (the keyword inside a string literal
/// followed by a capitalised word) can only SILENCE a suggestion, never
/// invent one.
pub fn declared_type_names(source: &str) -> Vec<String> {
    const DECLARING_KEYWORDS: [&str; 5] = ["struct", "enum", "trait", "type", "union"];
    let tokens = identifier_tokens(source);
    let mut out: Vec<String> = Vec::new();
    for pair in tokens.windows(2) {
        let (keyword, name) = (pair[0].1, pair[1].1);
        if DECLARING_KEYWORDS.contains(&keyword)
            && name.chars().next().is_some_and(char::is_uppercase)
            && !out.iter().any(|o| o == name)
        {
            out.push(name.to_string());
        }
    }
    out
}

/// Names that need no `use` before they appear: the Rust prelude plus the
/// std/serde names the legacy detectors always exempted (a std type is never a
/// wiring-map producer, so exempting it only removes noise), plus the JS/TS
/// globals `detect_unresolved_references` skips for polyglot sources.
///
/// ONE list for the three detectors (`wiring::detect_unresolved_references`,
/// `pre_edit::detect_unresolved_types`, [`ImportResolver::unresolved_type_references`])
/// — they had drifted into two slightly different copies.
pub fn is_builtin_type_name(name: &str) -> bool {
    matches!(
        name,
        // prelude types / variants
        "String" | "Vec" | "Option" | "Result" | "Box" | "Ok" | "Err" | "Some" | "None" | "Self"
        // prelude traits (incl. the derive macro names)
        | "Default" | "Send" | "Sync" | "Sized" | "Unpin" | "Drop" | "Clone" | "Copy" | "Debug"
        | "PartialEq" | "Eq" | "PartialOrd" | "Ord" | "Hash" | "Iterator" | "IntoIterator"
        | "DoubleEndedIterator" | "ExactSizeIterator" | "Extend" | "Into" | "From" | "TryInto"
        | "TryFrom" | "AsRef" | "AsMut" | "Fn" | "FnMut" | "FnOnce" | "ToOwned" | "ToString"
        // std names the legacy lists exempted
        | "Arc" | "Mutex" | "RwLock" | "HashMap" | "HashSet" | "BTreeMap" | "BTreeSet"
        | "VecDeque" | "Path" | "PathBuf" | "Duration" | "Instant" | "Display"
        // serde names (derive + serde_json::Value)
        | "Serialize" | "Deserialize" | "Value"
        // JS/TS globals (polyglot sources reach `detect_unresolved_references`)
        | "True" | "False" | "Array" | "Object" | "Map" | "Set" | "Promise"
    )
}

/// Does `word` have the shape of a type name: ≥2 chars, capitalised, not an
/// ALL_CAPS constant.
fn looks_like_type_name(word: &str) -> bool {
    word.len() >= 2
        && word.chars().next().is_some_and(char::is_uppercase)
        && !word
            .chars()
            .all(|c| c.is_uppercase() || c == '_' || c.is_ascii_digit())
}

/// `(byte_offset, identifier)` for every `[A-Za-z0-9_]+` run in `source`.
fn identifier_tokens(source: &str) -> Vec<(usize, &str)> {
    let mut tokens = Vec::new();
    let mut start: Option<usize> = None;
    for (i, ch) in source.char_indices() {
        let is_ident = ch.is_alphanumeric() || ch == '_';
        match (is_ident, start) {
            (true, None) => start = Some(i),
            (false, Some(s)) => {
                tokens.push((s, &source[s..i]));
                start = None;
            }
            _ => {}
        }
    }
    if let Some(s) = start {
        tokens.push((s, &source[s..]));
    }
    tokens
}

/// The name a path brings into scope: its last segment (`::` Rust, `.` Python,
/// `/` never reaches here but costs nothing).
fn last_path_segment(path: &str) -> &str {
    path.rsplit([':', '.', '/']).next().unwrap_or(path)
}

/// Flatten one `use` argument into `(path, alias, is_glob)` entries.
///
/// `crate::ast::{self, wiring::{Foo, Bar as Baz}, prelude::*}` becomes
/// `crate::ast` · `crate::ast::wiring::Foo` · `crate::ast::wiring::Bar as Baz`
/// · `crate::ast::prelude::*` (glob). Before S3 the whole argument was ONE
/// entry whose path ended in `}`, so every grouped import read as missing.
pub fn expand_use_arg(text: &str) -> Vec<(String, Option<String>, bool)> {
    let mut out = Vec::new();
    expand_use_into(text, &mut out);
    out
}

fn expand_use_into(text: &str, out: &mut Vec<(String, Option<String>, bool)>) {
    let text = text.trim();
    if text.is_empty() {
        return;
    }
    let Some(open) = text.find('{') else {
        // Leaf: `a::b`, `a::b as c`, `a::*`.
        let leaf = text.split_whitespace().collect::<Vec<_>>().join(" ");
        let (path, alias) = match leaf.split_once(" as ") {
            Some((p, a)) => (p.trim().to_string(), Some(a.trim().to_string())),
            None => (leaf, None),
        };
        let is_glob = path.ends_with('*');
        out.push((path, alias, is_glob));
        return;
    };
    let Some(close) = matching_brace(text, open) else {
        return; // unbalanced draft — nothing reliable to report
    };
    let prefix = text[..open].trim();
    for item in split_top_level(&text[open + 1..close]) {
        let item = item.trim();
        if item.is_empty() {
            continue;
        }
        if item == "self" || item.starts_with("self ") {
            // `a::{self}` brings the module `a` itself into scope.
            let alias = item
                .strip_prefix("self")
                .map(str::trim)
                .and_then(|rest| rest.strip_prefix("as"))
                .map(|a| a.trim().to_string());
            out.push((prefix.trim_end_matches("::").to_string(), alias, false));
            continue;
        }
        expand_use_into(&format!("{prefix}{item}"), out);
    }
}

fn matching_brace(text: &str, open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (i, ch) in text.char_indices().skip_while(|(i, _)| *i < open) {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

fn split_top_level(inner: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;
    for (i, ch) in inner.char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                parts.push(&inner[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(&inner[start..]);
    parts
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
        if let Some(ua_idx) = use_arg_idx
            && let Some(cap) = m.captures.iter().find(|c| c.index == ua_idx)
        {
            let text = node_text(source, cap.node);
            let line = cap.node.start_position().row + 1;
            // S3: one entry per imported NAME — a grouped `use a::{B, C}` is
            // two imports, not one whose path ends in `}`.
            for (path, alias, is_glob) in expand_use_arg(text) {
                imports.push(ResolvedImport {
                    path,
                    alias,
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
        if let Some(in_idx) = import_name_idx
            && let Some(cap) = m.captures.iter().find(|c| c.index == in_idx)
        {
            let text = node_text(source, cap.node).to_string();
            imports.push(ResolvedImport {
                path: text,
                alias: None,
                is_glob: false,
                line: cap.node.start_position().row + 1,
            });
            continue;
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
                    if let Some(child) = node.named_child(i as u32)
                        && child.kind() == "import_clause"
                    {
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
                                        if let Some(spec) = named.named_child(k as u32)
                                            && spec.kind() == "import_specifier"
                                        {
                                            let imp_name = if let Some(alias_node) =
                                                spec.child_by_field_name("alias")
                                            {
                                                let original = spec
                                                    .child_by_field_name("name")
                                                    .map(|n| node_text(source, n).to_string())
                                                    .unwrap_or_default();
                                                let alias =
                                                    node_text(source, alias_node).to_string();
                                                imports.push(ResolvedImport {
                                                    path: format!("{module_path}.{original}"),
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
                                // Namespace import: import * as X from '...'
                                if named.kind() == "namespace_import" {
                                    let alias_name = named
                                        .child_by_field_name("name")
                                        .or_else(|| {
                                            // Sometimes the identifier is a direct child
                                            for c in 0..named.named_child_count() {
                                                if let Some(ch) = named.named_child(c as u32)
                                                    && ch.kind() == "identifier"
                                                {
                                                    return Some(ch);
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

    // ── S3 (2026-09-02): grouped `use` lists are flattened, one entry per name.
    // Before: `use std::sync::{Arc, Mutex};` produced ONE import whose path was
    // the literal "std::sync::{Arc, Mutex}" — `ends_with("Mutex")` was false, so
    // `detect_missing_imports` reported every grouped import as missing.

    #[test]
    fn s3_grouped_use_list_is_flattened_one_entry_per_name() {
        let src = "use std::sync::{Arc, Mutex};\nuse crate::ast::{self, wiring::{Foo, Bar as Baz}, prelude::*};\n";
        let resolver = extract_imports_resolved(src, Lang::Rust);
        let paths: Vec<&str> = resolver.imports.iter().map(|i| i.path.as_str()).collect();
        for expected in [
            "std::sync::Arc",
            "std::sync::Mutex",
            "crate::ast",
            "crate::ast::wiring::Foo",
            "crate::ast::wiring::Bar",
        ] {
            assert!(paths.contains(&expected), "missing {expected} in {paths:?}");
        }
        let bar = resolver
            .imports
            .iter()
            .find(|i| i.path == "crate::ast::wiring::Bar")
            .expect("Bar entry");
        assert_eq!(bar.alias.as_deref(), Some("Baz"), "alias travels with the entry");
        let glob = resolver
            .imports
            .iter()
            .find(|i| i.is_glob)
            .expect("the nested `prelude::*` must be a glob entry");
        assert_eq!(glob.path, "crate::ast::prelude::*");
        assert!(
            resolver.imports.iter().all(|i| !i.path.contains('{')),
            "no braces may survive flattening: {paths:?}"
        );
    }

    #[test]
    fn s3_a_name_imported_through_a_grouped_list_is_not_missing() {
        let src = "use std::sync::{Arc, Mutex};\nfn f() -> Mutex<u8> { Mutex::new(0) }\n";
        let resolver = extract_imports_resolved(src, Lang::Rust);
        let missing = resolver.detect_missing_imports(src, &["Mutex", "Arc"]);
        assert!(missing.is_empty(), "grouped import must count: {missing:?}");
    }

    #[test]
    fn s3_alias_import_counts_for_the_alias_not_the_original_name() {
        let src = "use foo::Bar as Qux;\nfn f() -> Qux { Qux::default() }\n";
        let resolver = extract_imports_resolved(src, Lang::Rust);
        assert!(
            resolver.detect_missing_imports(src, &["Qux"]).is_empty(),
            "the alias is the name in scope"
        );
        let src_using_original = "use foo::Bar as Qux;\nfn f() -> Bar { Bar::default() }\n";
        let resolver = extract_imports_resolved(src_using_original, Lang::Rust);
        assert_eq!(
            resolver.detect_missing_imports(src_using_original, &["Bar"]),
            vec!["Bar".to_string()],
            "the original name is NOT in scope once aliased"
        );
    }

    #[test]
    fn s3_declared_type_names_lists_struct_enum_trait_type_and_union_only() {
        let src = "pub struct Foo;\nenum Bar { A }\npub(crate) trait Baz {}\ntype Qux = u8;\nunion U { a: u8 }\nfn make_thing() {}\nlet x = Other::new();\n";
        let names = declared_type_names(src);
        for n in ["Foo", "Bar", "Baz", "Qux", "U"] {
            assert!(names.iter().any(|d| d == n), "{n} missing in {names:?}");
        }
        assert!(!names.iter().any(|d| d == "make_thing" || d == "Other"), "{names:?}");
    }

    #[test]
    fn s3_unresolved_type_references_skip_qualified_declared_imported_and_builtins() {
        let src = "use crate::tfidf::{TfIdfVectorizer, Other};\nstruct Local;\nfn f() {\n    let a: TfIdfVectorizer = Other::new();\n    let b = crate::search::BM25Scorer::new();\n    let c: Vec<Local> = Vec::new();\n    let d: Missing = Missing::default();\n}\n";
        let resolver = extract_imports_resolved(src, Lang::Rust);
        let local = declared_type_names(src);
        assert_eq!(
            resolver.unresolved_type_references(src, &local),
            vec!["Missing".to_string()],
            "only the name nothing brings into scope"
        );
    }

    #[test]
    fn s3_unresolved_type_references_count_a_name_used_unqualified_at_least_once() {
        let src = "fn f() {\n    let a = search::Foo::new();\n    let b: Foo = a;\n}\n";
        let resolver = extract_imports_resolved(src, Lang::Rust);
        assert_eq!(resolver.unresolved_type_references(src, &[]), vec!["Foo".to_string()]);
        let only_qualified = "fn f() {\n    let a = search::Foo::new();\n}\n";
        let resolver = extract_imports_resolved(only_qualified, Lang::Rust);
        assert!(resolver.unresolved_type_references(only_qualified, &[]).is_empty());
    }
}
