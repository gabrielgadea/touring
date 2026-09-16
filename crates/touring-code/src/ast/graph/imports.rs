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
    let mut imports = extract_imports_treesitter(source, lang)
        .unwrap_or_else(|| extract_imports_regex(source, lang));
    // Aliases de tipo consomem seu alvo como um import consome — e nenhum dos
    // dois extratores acima os vê, porque nenhum é um `use`.
    if matches!(lang, Lang::Rust) {
        imports.extend(extract_rust_type_alias_targets(source));
    }
    imports
}

/// `type X = a::b::Target;` — um alias de tipo **consome** seu alvo exatamente
/// como um import, mas não é um `use`, então um raspador de `use` nunca registra
/// a aresta.
///
/// Medido em 07/08/2026: `SmellReport` era reportado órfão enquanto **35**
/// arquivos o aliasavam (`pub type TestPyramidReport = crate::quality::SmellReport;`).
/// O alias existe justamente porque essas 35 structs byte-idênticas foram
/// unificadas — ou seja, a dedup melhorou o código e, por esta lacuna do modelo,
/// *piorou* o número de órfãos. Um medidor que pune a correção é pior que um
/// medidor ausente.
///
/// Conservador por construção: só emite alvos que contenham `::`, então
/// `type Meters = f64;` e `type Buf = Vec<u8>;` não geram nada; e como o
/// resolvedor de imports sonda o filesystem, um alvo externo
/// (`std::collections::HashMap`) devolve `None` em vez de um caminho fantasma.
/// Só formas de UMA linha são reconhecidas — adivinhar continuação produziria
/// alvos truncados, que é exatamente como nascem os fantasmas.
fn extract_rust_type_alias_targets(source: &str) -> Vec<ImportInfo> {
    let mut out = Vec::new();
    for line in source.lines() {
        // Um alias de tipo real nunca carrega aspas. Sem este guard, um alias
        // ESCRITO DENTRO de um literal de string — como os fixtures dos testes
        // logo abaixo, ou um exemplo em prosa — é raspado como código e produz
        // aresta para um módulo que não existe (`crates/touring-code/src/a.rs`
        // apareceu no grafo exatamente assim, 07/08/2026).
        if line.contains('"') {
            continue;
        }
        let mut s = line.trim();
        // Visibilidade opcional: `pub`, `pub(crate)`, `pub(super)`, `pub(in …)`.
        if let Some(rest) = s.strip_prefix("pub") {
            let rest = rest.trim_start();
            s = match rest.strip_prefix('(') {
                Some(paren) => match paren.find(')') {
                    Some(i) => paren[i + 1..].trim_start(),
                    None => continue,
                },
                None => rest,
            };
        }
        let Some(rest) = s.strip_prefix("type ") else {
            continue;
        };
        let Some((_name, rhs)) = rest.split_once('=') else {
            continue;
        };
        let Some(rhs) = rhs.trim().strip_suffix(';') else {
            continue; // multi-linha / where-clause: fora do escopo, por segurança
        };
        // Corta genéricos: `crate::a::B<T>` → `crate::a::B`.
        let target = rhs.split('<').next().unwrap_or(rhs).trim();
        let Some((module, symbol)) = target.rsplit_once("::") else {
            continue; // sem caminho: `f64`, `Vec` — nada a consumir
        };
        if module.is_empty()
            || symbol.is_empty()
            || !symbol.chars().all(|c| c.is_alphanumeric() || c == '_')
        {
            continue;
        }
        out.push(ImportInfo {
            module_path: module.to_string(),
            symbols: vec![symbol.to_string()],
        });
    }
    out
}

/// Tree-sitter-powered import extraction — precise and multi-line-safe.
fn extract_imports_treesitter(source: &str, lang: Lang) -> Option<Vec<ImportInfo>> {
    use tree_sitter::{Parser, Query, QueryCursor};

    let mut parser = Parser::new();
    parser.set_language(&lang.tree_sitter_language()).ok()?;
    let tree = crate::ast::parser::parse_bounded(&mut parser, source, None)?;

    // Rust walks the `use` trees instead of querying them: a nested list
    // (`use a::{b::C, d::{E, F}}`, rustfmt's everyday shape) needs the prefix of
    // every enclosing level, which no flat query capture carries.
    if matches!(lang, Lang::Rust) {
        return Some(rust_use_imports(tree.root_node(), source.as_bytes()));
    }

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
            if let Some(def) = default_import
                && !entry.contains(&def)
            {
                entry.push(def);
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

/// Every `use` declaration of a Rust tree, one [`ImportInfo`] per module path.
///
/// Until 15/09/2026 Rust never reached the tree: its import query named a
/// TypeScript node (`default_import`), `Query::new` failed, and every file went
/// to the line regex — which reads only lines that START with `use `, so each
/// `pub use` / `pub(crate) use`, and each multi-line brace list, produced no
/// import at all (cross-audit R2). The walk sees the declaration whatever its
/// visibility and composes the full module path of every leaf.
fn rust_use_imports(root: tree_sitter::Node, src: &[u8]) -> Vec<ImportInfo> {
    let mut out: Vec<ImportInfo> = Vec::new();
    let mut cursor = root.walk();
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        if node.kind() == "use_declaration" {
            if let Some(argument) = node.child_by_field_name("argument") {
                collect_rust_use(argument, "", src, &mut out);
            }
            continue;
        }
        stack.extend(node.named_children(&mut cursor));
    }
    out
}

fn collect_rust_use(node: tree_sitter::Node, prefix: &str, src: &[u8], out: &mut Vec<ImportInfo>) {
    match node.kind() {
        "scoped_identifier" => {
            if let Some(name) = node.child_by_field_name("name") {
                let module = extend_prefix(prefix, node.child_by_field_name("path"), src);
                push_rust_import(out, module, Some(node_text(name, src)));
            }
        }
        "identifier" | "crate" | "super" | "self" | "metavariable" => {
            collect_rust_use_leaf(node, prefix, src, out);
        }
        "use_as_clause" => {
            if let Some(path) = node.child_by_field_name("path") {
                collect_rust_use(path, prefix, src, out);
            }
        }
        "scoped_use_list" => {
            let inner = extend_prefix(prefix, node.child_by_field_name("path"), src);
            if let Some(list) = node.child_by_field_name("list") {
                collect_rust_use(list, &inner, src, out);
            }
        }
        "use_list" => {
            let mut cursor = node.walk();
            for child in node.named_children(&mut cursor) {
                collect_rust_use(child, prefix, src, out);
            }
        }
        "use_wildcard" => {
            let mut cursor = node.walk();
            let module = extend_prefix(prefix, node.named_children(&mut cursor).next(), src);
            if !module.is_empty() {
                push_rust_import(out, module, None);
            }
        }
        _ => {}
    }
}

/// A single-segment leaf: `use foo;` at the top, `B` / `self` inside a list.
fn collect_rust_use_leaf(
    node: tree_sitter::Node,
    prefix: &str,
    src: &[u8],
    out: &mut Vec<ImportInfo>,
) {
    if prefix.is_empty() {
        // `use foo;` — a module with no symbol, as the regex reports it.
        push_rust_import(out, node_text(node, src), None);
    } else if node.kind() == "self" {
        // `use a::{self, B}` — the module itself.
        push_rust_import(out, prefix.to_string(), None);
    } else {
        push_rust_import(out, prefix.to_string(), Some(node_text(node, src)));
    }
}

/// `prefix::path`, or `prefix` alone when the level carries no path.
fn extend_prefix(prefix: &str, path: Option<tree_sitter::Node>, src: &[u8]) -> String {
    match path.map(|p| node_text(p, src)) {
        Some(tail) if prefix.is_empty() => tail,
        Some(tail) => format!("{prefix}::{tail}"),
        None => prefix.to_string(),
    }
}

fn node_text(node: tree_sitter::Node, src: &[u8]) -> String {
    node.utf8_text(src).unwrap_or("").to_string()
}

fn push_rust_import(out: &mut Vec<ImportInfo>, module_path: String, symbol: Option<String>) {
    let index = match out.iter().position(|i| i.module_path == module_path) {
        Some(index) => index,
        None => {
            out.push(ImportInfo {
                module_path,
                symbols: Vec::new(),
            });
            out.len() - 1
        }
    };
    if let Some(symbol) = symbol
        && !out[index].symbols.contains(&symbol)
    {
        out[index].symbols.push(symbol);
    }
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

        if line.starts_with("import ")
            && line.contains(" from ")
            && let Some(from_idx) = line.find(" from ")
        {
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

    /// B5 (16/09/2026): the query captured only the `dotted_name`, so the leading
    /// dots — which live in `import_prefix` — were dropped and `from .formato
    /// import X` was resolved from the source root. 136 of the 231 residual false
    /// orphans measured in the analise were exactly this.
    #[test]
    fn a_relative_python_import_keeps_its_dots() {
        let source = "from .formato import PUB\nfrom ..pacote.mod import OUTRO\nfrom . import IRMAO\nfrom absoluto.mod import LONGE\n";
        let imports = extract_imports(source, Lang::Python);
        let module_of = |symbol: &str| {
            imports
                .iter()
                .find(|i| i.symbols.iter().any(|s| s == symbol))
                .map(|i| i.module_path.clone())
                .unwrap_or_else(|| format!("{symbol} not extracted: {imports:?}"))
        };
        assert_eq!(module_of("PUB"), ".formato");
        assert_eq!(module_of("OUTRO"), "..pacote.mod");
        assert_eq!(module_of("IRMAO"), ".");
        assert_eq!(module_of("LONGE"), "absoluto.mod", "an absolute import is unchanged");
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
            imports
                .iter()
                .any(|i| i.module_path.contains("com.foo.Bar")),
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

    /// A lacuna que fazia um refactor legítimo PIORAR o número de órfãos:
    /// `SmellReport` aparecia sem consumidor enquanto 35 arquivos o aliasavam.
    #[test]
    fn type_alias_records_a_consumer_edge_on_its_target() {
        let src = "pub type TestPyramidReport = crate::quality::SmellReport;\n";
        let imports = extract_rust_type_alias_targets(src);
        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].module_path, "crate::quality");
        assert_eq!(imports[0].symbols, vec!["SmellReport"]);
    }

    #[test]
    fn type_alias_handles_visibility_and_generics() {
        let src = "pub(crate) type Cache = crate::store::Lru<String, u64>;\n\
                   pub(super) type Inner = super::detail::Node;\n\
                   type Private = crate::a::B;\n";
        let imports = extract_rust_type_alias_targets(src);
        assert_eq!(imports.len(), 3);
        assert_eq!(imports[0].symbols, vec!["Lru"]);
        assert_eq!(imports[1].module_path, "super::detail");
        assert_eq!(imports[2].symbols, vec!["B"]);
    }

    /// Um alias sem caminho não consome símbolo algum — emitir aqui fabricaria
    /// arestas para nomes que não são módulos.
    #[test]
    fn type_alias_without_a_path_emits_nothing() {
        let src = "type Meters = f64;\ntype Buf = Vec<u8>;\ntype R<T> = Result<T>;\n";
        assert!(extract_rust_type_alias_targets(src).is_empty());
    }

    /// Formas multi-linha ficam de fora de propósito: adivinhar a continuação
    /// produziria alvos truncados, que é como nascem caminhos fantasma.
    #[test]
    fn multiline_type_alias_is_skipped_rather_than_guessed() {
        let src = "type Long =\n    crate::a::B;\n";
        assert!(extract_rust_type_alias_targets(src).is_empty());
    }

    /// O alias entra pelo caminho público, não só pelo helper — se `extract_imports`
    /// não compuser o pós-passo, a correção não chega ao indexador.
    #[test]
    fn extract_imports_surfaces_alias_targets_alongside_use_statements() {
        let src = "use crate::a::Thing;\npub type Alias = crate::b::Other;\n";
        let imports = extract_imports(src, Lang::Rust);
        assert!(
            imports
                .iter()
                .any(|i| i.module_path == "crate::b" && i.symbols == vec!["Other"]),
            "alias target ausente: {imports:?}"
        );
        assert!(imports.iter().any(|i| i.module_path == "crate::a"));
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

    /// Cross-audit R2 (15/09/2026): the import queries must COMPILE. When
    /// `Query::new` fails, `extract_imports_treesitter` returns `None` for every
    /// file of the language and the regex fallback runs instead — and the Rust
    /// fallback skips every `pub use` / `pub(crate) use` line, so a symbol that a
    /// file re-exports from another crate never got its consumer edge.
    #[test]
    fn every_import_query_compiles() {
        for lang in [Lang::Rust, Lang::Python, Lang::TypeScript, Lang::JavaScript] {
            let q = tree_sitter::Query::new(&lang.tree_sitter_language(), lang.import_query_file());
            assert!(q.is_ok(), "{lang:?} import query: {:?}", q.err());
        }
    }

    #[test]
    fn the_rust_query_sees_uses_behind_a_visibility_modifier() {
        let src = "use a::b::C;\npub(crate) use touring_x::policy::MAX;\npub use crate::m::{D, e};\n";
        let imports =
            extract_imports_treesitter(src, Lang::Rust).expect("the Rust import query compiles");
        let has = |m: &str, s: &str| {
            imports
                .iter()
                .any(|i| i.module_path == m && i.symbols.iter().any(|x| x == s))
        };
        assert!(has("a::b", "C"), "{imports:?}");
        assert!(has("touring_x::policy", "MAX"), "{imports:?}");
        assert!(has("crate::m", "D") && has("crate::m", "e"), "{imports:?}");
    }

    /// The walk composes each leaf's full module path, whatever the nesting,
    /// the line breaks or the visibility in front of `use`.
    #[test]
    fn rust_use_trees_yield_every_leaf_under_its_full_module_path() {
        let src = concat!(
            "use touring_foundation::{\n",
            "    config::TouringConfig,\n",
            "    gitignore::{GitIgnoreRules, IgnoredBy},\n",
            "    self,\n",
            "};\n",
            "pub(crate) use touring_hooks_shared::index_policy::{self as policy, MAX_INDEXABLE_FILE_BYTES};\n",
            "use super::{MAX, exceeds_index_size_ceiling};\n",
            "use crate::shared::Runtime as Rt;\n",
            "use std::io::*;\n",
            "use serde;\n",
            "mod inner { pub use super::helpers::Tool; }\n",
        );
        let imports = extract_imports(src, Lang::Rust);
        let symbols = |m: &str| -> Vec<String> {
            imports
                .iter()
                .find(|i| i.module_path == m)
                .map(|i| i.symbols.clone())
                .unwrap_or_else(|| panic!("no import of {m}: {imports:?}"))
        };
        assert_eq!(symbols("touring_foundation::config"), ["TouringConfig"]);
        assert_eq!(symbols("touring_foundation::gitignore"), ["GitIgnoreRules", "IgnoredBy"]);
        assert!(symbols("touring_foundation").is_empty(), "`self` names the module");
        assert_eq!(symbols("touring_hooks_shared::index_policy"), ["MAX_INDEXABLE_FILE_BYTES"]);
        assert_eq!(symbols("super"), ["MAX", "exceeds_index_size_ceiling"]);
        assert_eq!(symbols("crate::shared"), ["Runtime"], "the origin name, not the alias");
        assert!(symbols("std::io").is_empty(), "a glob names no symbol");
        assert!(symbols("serde").is_empty());
        assert_eq!(symbols("super::helpers"), ["Tool"], "a `use` nested in a module");
    }
}
