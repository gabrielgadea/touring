//! Import extraction — tree-sitter-powered with regex fallback.

use std::collections::HashMap;

use streaming_iterator::StreamingIterator;

use crate::ast::languages::Lang;

/// Import information extracted from source
#[derive(Debug, Clone)]
pub struct ImportInfo {
    /// Module path being imported
    pub module_path: String,
    /// Specific symbols imported (empty for "import *" or bare imports)
    pub symbols: Vec<String>,
}

/// Extract imports from source code using tree-sitter queries.
///
/// Falls back to regex-based extraction if the tree-sitter query fails
/// (e.g., due to grammar version mismatch).
pub fn extract_imports(source: &str, lang: Lang) -> Vec<ImportInfo> {
    extract_imports_treesitter(source, lang).unwrap_or_else(|| extract_imports_regex(source, lang))
}

/// Tree-sitter-powered import extraction — precise and multi-line-safe.
fn extract_imports_treesitter(source: &str, lang: Lang) -> Option<Vec<ImportInfo>> {
    use tree_sitter::{Parser, Query, QueryCursor};

    let mut parser = Parser::new();
    parser.set_language(&lang.tree_sitter_language()).ok()?;
    let tree = parser.parse(source, None)?;

    let query_src = lang.import_query_file();
    let ts_lang = lang.tree_sitter_language();
    let query = Query::new(&ts_lang, query_src).ok()?;

    let capture_names: Vec<&str> = query.capture_names().to_vec();

    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(&query, tree.root_node(), source.as_bytes());

    // Accumulate per-module: module_path → set of symbols
    let mut module_map: HashMap<String, Vec<String>> = HashMap::new();

    while let Some(m) = matches.next() {
        let mut module_text: Option<String> = None;
        let mut symbol_text: Option<String> = None;
        let mut default_import: Option<String> = None;

        for capture in m.captures {
            let name = match capture_names.get(capture.index as usize) {
                Some(n) => *n,
                None => continue,
            };
            let text = capture
                .node
                .utf8_text(source.as_bytes())
                .unwrap_or("")
                .to_string();

            match name {
                "module" => module_text = Some(text),
                "symbol" => symbol_text = Some(text),
                "default_import" => default_import = Some(text),
                _ => {}
            }
        }

        if let Some(module) = module_text {
            let entry = module_map.entry(module).or_default();
            // Defensive: if the tree-sitter query happens to capture a brace
            // group as a single symbol (older grammar revisions, or queries
            // that match `list:` as a node), expand it into individual
            // names rather than persisting the malformed literal.
            if let Some(sym) = symbol_text {
                if sym.starts_with('{') && sym.ends_with('}') && sym.len() >= 2 {
                    let inner = &sym[1..sym.len() - 1];
                    for expanded in expand_brace_inner(inner) {
                        if !entry.contains(&expanded) {
                            entry.push(expanded);
                        }
                    }
                } else if !entry.contains(&sym) {
                    entry.push(sym);
                }
            }
            if let Some(def) = default_import {
                if !entry.contains(&def) {
                    entry.push(def);
                }
            }
        }
    }

    let imports: Vec<ImportInfo> = module_map
        .into_iter()
        .map(|(module_path, symbols)| ImportInfo {
            module_path,
            symbols,
        })
        .collect();

    Some(imports)
}

/// Regex-based import extraction — fallback when tree-sitter query fails.
fn extract_imports_regex(source: &str, lang: Lang) -> Vec<ImportInfo> {
    match lang {
        Lang::Python => extract_python_imports_regex(source),
        Lang::Rust => extract_rust_imports_regex(source),
        Lang::TypeScript | Lang::JavaScript => extract_ts_imports_regex(source),
        // Data/markup languages don't have import semantics
        _ => Vec::new(),
    }
}

/// Extract Python imports (regex fallback)
fn extract_python_imports_regex(source: &str) -> Vec<ImportInfo> {
    let mut imports = Vec::new();

    for line in source.lines() {
        let line = line.trim();

        if line.starts_with("from ") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if let Some(import_idx) = parts.iter().position(|&p| p == "import") {
                let module = parts.get(1).map(|s| s.to_string()).unwrap_or_default();
                let symbols: Vec<String> = parts
                    .get(import_idx + 1..)
                    .unwrap_or_default()
                    .iter()
                    .map(|s| s.trim_end_matches(',').to_string())
                    .filter(|s| !s.is_empty() && s != "*")
                    .collect();
                imports.push(ImportInfo {
                    module_path: module,
                    symbols,
                });
            }
        } else if line.starts_with("import ") && !line.starts_with("import(") {
            let module = line[7..].split_whitespace().next();
            if let Some(m) = module {
                imports.push(ImportInfo {
                    module_path: m.to_string(),
                    symbols: Vec::new(),
                });
            }
        }
    }

    imports
}

/// Extract Rust imports (regex fallback)
///
/// Handles three shapes:
/// 1. `use a::b::Foo;`                — module="a::b",   symbols=["Foo"]
/// 2. `use a::b::{Foo, bar};`         — module="a::b",   symbols=["Foo", "bar"]
/// 3. `use a;`                        — module="a",      symbols=[]
///
/// Brace groups (case 2) are the historic F6 bug: the legacy code emitted
/// the entire `{Foo, bar}` literal as a single symbol name, which polluted
/// wiring_map with malformed keys like `{self, GateMetricsSnapshot}` and
/// produced 800+ `kind_unknown` rows.
fn extract_rust_imports_regex(source: &str) -> Vec<ImportInfo> {
    let mut imports = Vec::new();

    for line in source.lines() {
        let line = line.trim();

        let Some(remainder) = line.strip_prefix("use ") else {
            continue;
        };
        let path = remainder.trim_end_matches(';').trim();

        // Brace group: `a::b::{X, Y, Z}` — split into N imports under module `a::b`.
        if let Some(open) = path.rfind("::{") {
            let module = &path[..open];
            let after_open = &path[open + 3..];
            // Strip trailing '}' if present (defensive — accept malformed input).
            let inner = after_open.strip_suffix('}').unwrap_or(after_open);
            let symbols = expand_brace_inner(inner);
            if !symbols.is_empty() {
                imports.push(ImportInfo {
                    module_path: module.to_string(),
                    symbols,
                });
            }
            continue;
        }

        if let Some(last_dbl) = path.rfind("::") {
            let module = &path[..last_dbl];
            let symbol = &path[last_dbl + 2..];
            imports.push(ImportInfo {
                module_path: module.to_string(),
                symbols: vec![symbol.to_string()],
            });
        } else {
            imports.push(ImportInfo {
                module_path: path.to_string(),
                symbols: Vec::new(),
            });
        }
    }

    imports
}

/// Parse the inside of a Rust brace import group (the text BETWEEN the
/// outer braces, NOT including them) into individual symbol names.
///
/// Handles:
/// - Plain identifiers: `Foo, bar, baz_2` → ["Foo", "bar", "baz_2"]
/// - Aliases: `Foo as F` → keeps the origin name (`"Foo"`)
/// - Nested paths: `mod::Sym` → keeps the leaf (`"Sym"`)
/// - Nested brace groups: counted at depth-0 only (each top-level `,` is a
///   split point, commas inside nested `{...}` are preserved). The nested
///   group itself is then dropped because it produces no leaf identifier.
/// - Drops `self`, `super`, `crate`, `*`, and anything that is not a valid
///   identifier after stripping path/alias decoration.
///
/// This is a defensive helper — it never panics and never returns malformed
/// names. False negatives (missed exotic syntax) are acceptable; false
/// positives (junk symbol names) would poison wiring_map.
pub(crate) fn expand_brace_inner(inner: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth: i32 = 0;
    let mut start = 0usize;
    let bytes = inner.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'{' => depth += 1,
            b'}' => depth -= 1,
            b',' if depth == 0 => {
                push_brace_token(&inner[start..i], &mut out);
                start = i + 1;
            }
            _ => {}
        }
    }
    push_brace_token(&inner[start..], &mut out);
    out
}

fn push_brace_token(raw: &str, out: &mut Vec<String>) {
    let token = raw.trim();
    if token.is_empty() {
        return;
    }
    // Strip alias: "Foo as F" → "Foo" (origin is what we wire).
    let before_alias = match token.split_once(" as ") {
        Some((lhs, _)) => lhs.trim(),
        None => token,
    };
    // Take the leaf segment of any path: "mod::Foo" → "Foo".
    let leaf = before_alias
        .rsplit("::")
        .next()
        .unwrap_or(before_alias)
        .trim();
    if leaf.is_empty() {
        return;
    }
    if matches!(leaf, "self" | "super" | "crate" | "*") {
        return;
    }
    // Validate identifier shape: ASCII letter or `_`, then ASCII alnum/_.
    let mut chars = leaf.chars();
    let Some(first) = chars.next() else { return };
    if !(first.is_ascii_alphabetic() || first == '_') {
        return;
    }
    if !chars.all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return;
    }
    out.push(leaf.to_string());
}

/// Extract TypeScript/JavaScript imports (regex fallback)
fn extract_ts_imports_regex(source: &str) -> Vec<ImportInfo> {
    let mut imports = Vec::new();

    for line in source.lines() {
        let line = line.trim();

        if line.starts_with("import ") && line.contains(" from ") {
            if let Some(from_idx) = line.find(" from ") {
                let module_part = &line[from_idx + 6..].trim();
                let module = module_part
                    .trim_end_matches(';')
                    .trim_matches('"')
                    .trim_matches('\'');

                let import_part = &line[7..from_idx];

                let symbols: Vec<String> = if let Some(start) = import_part.find('{') {
                    if let Some(end) = import_part.find('}') {
                        import_part[start + 1..end]
                            .split(',')
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty())
                            .collect()
                    } else {
                        Vec::new()
                    }
                } else {
                    vec![import_part.trim().to_string()]
                };

                imports.push(ImportInfo {
                    module_path: module.to_string(),
                    symbols,
                });
            }
        }
    }

    imports
}

#[cfg(test)]
mod tests {
    #![allow(clippy::indexing_slicing)]
    use super::*;

    #[test]
    fn test_extract_python_imports_regex() {
        let source = r#"
import os
from pathlib import Path
from collections import HashMap, Vec
"#;

        let imports = extract_python_imports_regex(source);
        assert_eq!(imports.len(), 3);
        assert_eq!(imports[0].module_path, "os");
        assert_eq!(imports[1].module_path, "pathlib");
        assert!(imports[1].symbols.contains(&"Path".to_string()));
    }

    #[test]
    fn test_extract_rust_imports_regex() {
        let source = r#"
use std::collections::HashMap;
use crate::ast::models::User;
use serde::Deserialize;
"#;

        let imports = extract_rust_imports_regex(source);
        assert_eq!(imports.len(), 3);
        assert_eq!(imports[0].module_path, "std::collections");
        assert!(imports[0].symbols.contains(&"HashMap".to_string()));
    }

    #[test]
    fn test_extract_ts_imports_regex() {
        let source = r#"
import { useState, useEffect } from 'react';
import axios from 'axios';
import type { User } from './types';
"#;

        let imports = extract_ts_imports_regex(source);
        assert_eq!(imports.len(), 3);
        assert_eq!(imports[0].module_path, "react");
        assert!(imports[0].symbols.contains(&"useState".to_string()));
        assert_eq!(imports[1].module_path, "axios");
    }

    #[test]
    fn test_extract_imports_treesitter_python() {
        let source = "import os\nfrom pathlib import Path\n";
        let imports = extract_imports(source, Lang::Python);
        assert!(!imports.is_empty(), "Should extract Python imports");
        assert!(
            imports
                .iter()
                .any(|i| i.module_path.contains("os") || i.module_path.contains("pathlib")),
            "Should find os or pathlib import, got: {:?}",
            imports
        );
    }

    #[test]
    fn test_extract_imports_treesitter_rust() {
        let source = "use std::collections::HashMap;\nuse serde::Deserialize;\n";
        let imports = extract_imports(source, Lang::Rust);
        assert!(!imports.is_empty(), "Should extract Rust imports");
    }

    #[test]
    fn test_extract_imports_treesitter_ts() {
        let source = "import { useState } from 'react';\nimport axios from 'axios';\n";
        let imports = extract_imports(source, Lang::TypeScript);
        assert!(!imports.is_empty(), "Should extract TS imports");
    }

    #[cfg(feature = "more-languages")]
    #[test]
    fn test_extract_imports_treesitter_java() {
        // Verifies java_imports.scm node types against tree-sitter-java: a wrong
        // query would yield an empty result (Query::new fails → graceful empty).
        let source = "package com.app;\nimport com.foo.Bar;\nimport com.baz.Qux;\n";
        let imports = extract_imports(source, Lang::Java);
        assert!(
            !imports.is_empty(),
            "Should extract Java imports, got: {imports:?}"
        );
        assert!(
            imports.iter().any(|i| i.module_path.contains("com.foo.Bar")),
            "Should find com.foo.Bar, got: {imports:?}"
        );
        assert!(
            imports.iter().any(|i| i.symbols.iter().any(|s| s == "Bar")),
            "Should capture Bar as the imported symbol, got: {imports:?}"
        );
    }

    #[cfg(feature = "more-languages")]
    #[test]
    fn test_extract_imports_treesitter_go() {
        // Verifies go_imports.scm node types against tree-sitter-go.
        let source = "package main\nimport (\n\t\"fmt\"\n\t\"mymod/pkg\"\n)\n";
        let imports = extract_imports(source, Lang::Go);
        assert!(
            !imports.is_empty(),
            "Should extract Go imports, got: {imports:?}"
        );
        assert!(
            imports.iter().any(|i| i.module_path.contains("mymod/pkg")),
            "Should find mymod/pkg, got: {imports:?}"
        );
    }

    // ─── F6 brace-import regression suite (2026-05-11 audit) ─────────────────

    #[test]
    fn f6_expand_brace_inner_simple() {
        assert_eq!(expand_brace_inner("a, b, c"), vec!["a", "b", "c"]);
    }

    #[test]
    fn f6_expand_brace_inner_drops_self_super_glob() {
        // `self` re-exports the current path, not a distinct symbol; same for
        // `super`, `crate`, and `*` — none of them should become consumer rows.
        assert_eq!(
            expand_brace_inner("self, GateMetricsSnapshot, super, *"),
            vec!["GateMetricsSnapshot"]
        );
    }

    #[test]
    fn f6_expand_brace_inner_keeps_alias_origin() {
        // `use foo::{bar as baz}` — wire the producer name, not the local alias.
        assert_eq!(expand_brace_inner("bar as baz"), vec!["bar"]);
    }

    #[test]
    fn f6_expand_brace_inner_takes_leaf_of_path() {
        // `use foo::{mod::Sym}` — wire the leaf `Sym`.
        assert_eq!(expand_brace_inner("mod::Sym, other"), vec!["Sym", "other"]);
    }

    #[test]
    fn f6_expand_brace_inner_rejects_malformed_tokens() {
        // Punctuation-only fragments and empty tokens never become symbols.
        assert!(expand_brace_inner("").is_empty());
        assert!(expand_brace_inner(",,,").is_empty());
        assert!(expand_brace_inner("{nested, group}").is_empty());
    }

    #[test]
    fn f6_regex_fallback_expands_brace_group() {
        // Pre-fix: this emitted ONE symbol named "{self, GateMetricsSnapshot}".
        // Post-fix: emits ONE module with ONE symbol "GateMetricsSnapshot".
        let src = "use crate::ast::shared::{self, GateMetricsSnapshot};\n";
        let imports = extract_rust_imports_regex(src);
        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].module_path, "crate::ast::shared");
        assert_eq!(imports[0].symbols, vec!["GateMetricsSnapshot"]);
    }

    #[test]
    fn f6_regex_fallback_multi_symbol_brace() {
        let src = "use crate::ast::foo::{Bar, baz, Qux};\n";
        let imports = extract_rust_imports_regex(src);
        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].module_path, "crate::ast::foo");
        assert_eq!(imports[0].symbols, vec!["Bar", "baz", "Qux"]);
    }

    #[test]
    fn f6_treesitter_brace_import_yields_individual_symbols() {
        // Source uses brace group — driver test for the new query patterns
        // in rust_imports.scm. Pre-fix the query had no use_list pattern,
        // so this import was dropped entirely.
        let src = "use crate::ast::shared::{self, GateMetricsSnapshot, TantivyIndex};\n";
        let imports = extract_imports(src, Lang::Rust);
        // Find the entry for the brace path module
        let entry = imports
            .iter()
            .find(|i| i.module_path.ends_with("shared"))
            .expect("brace import module must be captured");
        assert!(
            entry.symbols.contains(&"GateMetricsSnapshot".to_string()),
            "expected GateMetricsSnapshot in symbols, got: {:?}",
            entry.symbols
        );
        assert!(
            entry.symbols.contains(&"TantivyIndex".to_string()),
            "expected TantivyIndex in symbols, got: {:?}",
            entry.symbols
        );
        assert!(
            !entry.symbols.iter().any(|s| s.contains('{')),
            "no symbol must contain literal brace, got: {:?}",
            entry.symbols
        );
        assert!(
            !entry.symbols.iter().any(|s| s == "self"),
            "self must not become a wired symbol, got: {:?}",
            entry.symbols
        );
    }
}
