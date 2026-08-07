//! Context-Tree Awareness — Module hierarchy extraction.
//!
//! Builds a tree of Rust module declarations from source code,
//! enabling correct import path resolution and re-export tracking.

use serde::{Deserialize, Serialize};
use streaming_iterator::StreamingIterator;
use tree_sitter::{Query, QueryCursor};

use crate::ast::languages::Lang;
use crate::ast::node_text;
use crate::ast::parser::parse_thread_local;

// ─── Types ──────────────────────────────────────────────────────────────

/// A node in the module hierarchy tree.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleNode {
    /// Module name (e.g., `"outer"`, `"inner"`)
    pub name: String,
    /// File path this module was parsed from
    pub path: String,
    /// Child modules
    pub children: Vec<ModuleNode>,
    /// Re-exported symbols (from `pub use` statements)
    pub re_exports: Vec<String>,
    /// Whether the module is public
    pub is_pub: bool,
}

/// Hierarchical module tree built from source code.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleTree {
    /// Root node representing the file
    pub root: ModuleNode,
}

// ─── Tree-sitter queries ────────────────────────────────────────────────

/// Query to extract mod declarations and pub use re-exports.
const MODULE_QUERY: &str = r#"
;; mod declarations
(mod_item
  name: (identifier) @mod_name
  body: (declaration_list)? @mod_body) @mod_item

;; pub use re-exports
(use_declaration
  (visibility_modifier) @vis
  argument: (_) @use_path)
"#;

// ─── Implementation ─────────────────────────────────────────────────────

impl ModuleTree {
    /// Build a module tree from source code.
    ///
    /// Parses module declarations and re-exports to construct
    /// a hierarchical view of the module structure.
    /// Supports Rust (`mod` + `pub use`), Python (imports), and
    /// TypeScript/JavaScript (`export`/`import`).
    ///
    /// # Arguments
    /// * `source` - Source code
    /// * `file_path` - Path of the file being parsed (used as root name)
    pub fn build_from_source(source: &str, file_path: &str) -> Self {
        let root = build_module_node(source, file_path);
        Self { root }
    }

    /// Build a module tree from source code with explicit language.
    ///
    /// # Arguments
    /// * `source` - Source code
    /// * `file_path` - Path of the file being parsed (used as root name)
    /// * `lang` - Language of the source code
    pub fn build_from_source_for_lang(source: &str, file_path: &str, lang: Lang) -> Self {
        let root = match lang {
            Lang::Rust => build_module_node(source, file_path),
            Lang::Python => build_python_module_node(source, file_path),
            Lang::TypeScript | Lang::JavaScript => build_ts_module_node(source, file_path, lang),
            _ => empty_node(file_path, file_path, true),
        };
        Self { root }
    }

    /// Resolve a use path to a module node name, if possible.
    ///
    /// Simple resolution: searches the tree for a module matching
    /// the last segment of the path.
    pub fn resolve_path(&self, use_path: &str) -> Option<String> {
        let segments: Vec<&str> = use_path.split("::").collect();
        let last = segments.last()?;
        find_module_by_name(&self.root, last).map(|n| n.name.clone())
    }
}

/// A childless module node — used for error/unsupported fallbacks (`name =
/// file_path`) and for leaf import dependencies (`name = imported module`).
/// Centralizes the `ModuleNode { .. children: empty .. }` construction that
/// every per-language builder would otherwise repeat verbatim.
fn empty_node(name: &str, file_path: &str, is_pub: bool) -> ModuleNode {
    ModuleNode {
        name: name.to_string(),
        path: file_path.to_string(),
        children: Vec::new(),
        re_exports: Vec::new(),
        is_pub,
    }
}

/// Assemble a file's root node from its collected `children` and `re_exports`.
/// The root always takes the file path as its name and is public — shared by
/// every per-language builder's final step.
fn root_node(file_path: &str, children: Vec<ModuleNode>, re_exports: Vec<String>) -> ModuleNode {
    ModuleNode {
        name: file_path.to_string(),
        path: file_path.to_string(),
        children,
        re_exports,
        is_pub: true,
    }
}

fn build_module_node(source: &str, file_path: &str) -> ModuleNode {
    let tree = match parse_thread_local(source, Lang::Rust) {
        Ok(t) => t,
        Err(_) => {
            return empty_node(file_path, file_path, true);
        }
    };

    let ts_lang = Lang::Rust.tree_sitter_language();
    let query = match Query::new(&ts_lang, MODULE_QUERY) {
        Ok(q) => q,
        Err(_) => {
            return empty_node(file_path, file_path, true);
        }
    };

    let mut cursor = QueryCursor::new();
    let root = tree.root_node();
    let mut matches = cursor.matches(&query, root, source.as_bytes());

    let mod_name_idx = query.capture_index_for_name("mod_name");
    let mod_item_idx = query.capture_index_for_name("mod_item");
    let vis_idx = query.capture_index_for_name("vis");
    let use_path_idx = query.capture_index_for_name("use_path");

    let mut children = Vec::new();
    let mut re_exports = Vec::new();

    while let Some(m) = matches.next() {
        // Check for mod declarations
        if let (Some(mn_idx), Some(mi_idx)) = (mod_name_idx, mod_item_idx) {
            let name_cap = m.captures.iter().find(|c| c.index == mn_idx);
            let item_cap = m.captures.iter().find(|c| c.index == mi_idx);

            if let (Some(name_c), Some(item_c)) = (name_cap, item_cap) {
                let mod_name = node_text(source, name_c.node).to_string();
                let is_pub = check_node_pub(source, item_c.node);

                // Check if has inline body (declaration_list)
                let mut sub_children = Vec::new();
                let mod_body_idx = query.capture_index_for_name("mod_body");
                if let Some(mb_idx) = mod_body_idx
                    && let Some(body_cap) = m.captures.iter().find(|c| c.index == mb_idx)
                {
                    // Parse inner modules from the body text
                    let body_text = node_text(source, body_cap.node);
                    let inner = build_module_node(body_text, &mod_name);
                    sub_children = inner.children;
                }

                children.push(ModuleNode {
                    name: mod_name,
                    path: file_path.to_string(),
                    children: sub_children,
                    re_exports: Vec::new(),
                    is_pub,
                });
                continue;
            }
        }

        // Check for pub use re-exports
        if let (Some(v_idx), Some(up_idx)) = (vis_idx, use_path_idx) {
            let vis_cap = m.captures.iter().find(|c| c.index == v_idx);
            let path_cap = m.captures.iter().find(|c| c.index == up_idx);

            if let (Some(_vis), Some(path_c)) = (vis_cap, path_cap) {
                let use_text = node_text(source, path_c.node).to_string();
                re_exports.push(use_text);
                continue;
            }
        }
    }

    root_node(file_path, children, re_exports)
}

// ─── Python module tree ─────────────────────────────────────────────────

fn build_python_module_node(source: &str, file_path: &str) -> ModuleNode {
    let tree = match parse_thread_local(source, Lang::Python) {
        Ok(t) => t,
        Err(_) => {
            return empty_node(file_path, file_path, true);
        }
    };

    let root = tree.root_node();
    let mut children = Vec::new();
    let mut re_exports = Vec::new();

    // Walk top-level statements
    for i in 0..root.named_child_count() {
        if let Some(child) = root.named_child(i as u32) {
            match child.kind() {
                // `import module` → module as a dependency child
                "import_statement" => {
                    if let Some(name_node) = child.child_by_field_name("name") {
                        let mod_name = node_text(source, name_node).to_string();
                        children.push(empty_node(&mod_name, file_path, true));
                    }
                }
                // `from module import name` → module as dependency, name as re-export
                "import_from_statement" => {
                    if let Some(mod_node) = child.child_by_field_name("module_name") {
                        let mod_name = node_text(source, mod_node).to_string();
                        // Collect imported names as re-exports
                        for j in 0..child.named_child_count() {
                            if let Some(name_child) = child.named_child(j as u32)
                                && name_child.kind() == "dotted_name"
                                && name_child != mod_node
                            {
                                re_exports.push(node_text(source, name_child).to_string());
                            }
                        }
                        // Add module as child if not already there
                        if !children.iter().any(|c: &ModuleNode| c.name == mod_name) {
                            children.push(empty_node(&mod_name, file_path, true));
                        }
                    }
                }
                _ => {}
            }
        }
    }

    root_node(file_path, children, re_exports)
}

// ─── TypeScript/JavaScript module tree ──────────────────────────────────

fn build_ts_module_node(source: &str, file_path: &str, lang: Lang) -> ModuleNode {
    let tree = match parse_thread_local(source, lang) {
        Ok(t) => t,
        Err(_) => {
            return empty_node(file_path, file_path, true);
        }
    };

    let root = tree.root_node();
    let mut children = Vec::new();
    let mut re_exports = Vec::new();

    for i in 0..root.named_child_count() {
        if let Some(child) = root.named_child(i as u32) {
            match child.kind() {
                // `import { X } from 'module'` → dependency
                "import_statement" => {
                    if let Some(src_node) = child.child_by_field_name("source") {
                        let mod_name = node_text(source, src_node)
                            .trim_matches(|c| c == '\'' || c == '"')
                            .to_string();
                        if !children.iter().any(|c: &ModuleNode| c.name == mod_name) {
                            children.push(empty_node(&mod_name, file_path, false));
                        }
                    }
                }
                // `export function/class/const ...` → public node
                "export_statement" => {
                    // Find the declaration name
                    for j in 0..child.named_child_count() {
                        if let Some(decl) = child.named_child(j as u32) {
                            let name = match decl.kind() {
                                "function_declaration" | "class_declaration" => decl
                                    .child_by_field_name("name")
                                    .map(|n| node_text(source, n).to_string()),
                                "lexical_declaration" => {
                                    // export const X = ...
                                    decl.named_child(0)
                                        .and_then(|d| d.child_by_field_name("name"))
                                        .map(|n| node_text(source, n).to_string())
                                }
                                "interface_declaration"
                                | "type_alias_declaration"
                                | "enum_declaration" => decl
                                    .child_by_field_name("name")
                                    .map(|n| node_text(source, n).to_string()),
                                _ => None,
                            };
                            if let Some(export_name) = name {
                                re_exports.push(export_name);
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }

    root_node(file_path, children, re_exports)
}

/// Recursively find a module node by name.
fn find_module_by_name<'a>(node: &'a ModuleNode, name: &str) -> Option<&'a ModuleNode> {
    if node.name == name {
        return Some(node);
    }
    for child in &node.children {
        if let Some(found) = find_module_by_name(child, name) {
            return Some(found);
        }
    }
    None
}

/// Check if a node has a visibility_modifier child indicating `pub`.
fn check_node_pub(source: &str, node: tree_sitter::Node) -> bool {
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i as u32)
            && child.kind() == "visibility_modifier"
        {
            let text = node_text(source, child);
            return text.starts_with("pub");
        }
    }
    false
}

// ─── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_module_tree_basic() {
        let src = r#"
pub mod outer {
    pub mod inner {
        pub struct InnerStruct;
    }
    pub use inner::InnerStruct;
}
"#;
        let tree = ModuleTree::build_from_source(src, "lib.rs");
        assert_eq!(tree.root.name, "lib.rs");
        let outer = tree.root.children.iter().find(|n| n.name == "outer");
        assert!(
            outer.is_some(),
            "outer module not found: {:?}",
            tree.root.children
        );
        let outer = outer.expect("outer must exist");
        assert!(outer.is_pub);
    }

    #[test]
    fn test_module_tree_empty_source() {
        let tree = ModuleTree::build_from_source("", "empty.rs");
        assert_eq!(tree.root.name, "empty.rs");
        assert!(tree.root.children.is_empty());
    }

    #[test]
    fn test_module_tree_no_modules() {
        let src = r#"
struct Foo;
fn bar() {}
"#;
        let tree = ModuleTree::build_from_source(src, "lib.rs");
        assert!(tree.root.children.is_empty());
    }

    #[test]
    fn test_resolve_path() {
        let src = r#"
pub mod parser {
    pub struct ParserPool;
}
pub mod symbols {
    pub struct Symbol;
}
"#;
        let tree = ModuleTree::build_from_source(src, "lib.rs");
        assert_eq!(
            tree.resolve_path("crate::ast::parser"),
            Some("parser".to_string())
        );
        assert_eq!(
            tree.resolve_path("crate::ast::symbols"),
            Some("symbols".to_string())
        );
        assert!(tree.resolve_path("crate::ast::nonexistent").is_none());
    }

    #[test]
    fn test_re_exports() {
        let src = r#"
pub use crate::ast::inner::Foo;
pub use crate::ast::inner::Bar;
"#;
        let tree = ModuleTree::build_from_source(src, "lib.rs");
        assert_eq!(
            tree.root.re_exports.len(),
            2,
            "re_exports: {:?}",
            tree.root.re_exports
        );
    }

    // ─── Python tests ───────────────────────────────────────────────

    #[test]
    fn test_python_module_tree_imports() {
        let src = r#"
import os
import sys
from pathlib import Path
from collections import OrderedDict, defaultdict
"#;
        let tree = ModuleTree::build_from_source_for_lang(src, "main.py", Lang::Python);
        assert_eq!(tree.root.name, "main.py");
        assert!(
            tree.root.children.iter().any(|c| c.name == "os"),
            "os module not found: {:?}",
            tree.root.children
        );
        assert!(
            tree.root.children.iter().any(|c| c.name == "sys"),
            "sys module not found: {:?}",
            tree.root.children
        );
        assert!(
            tree.root.children.iter().any(|c| c.name == "pathlib"),
            "pathlib module not found: {:?}",
            tree.root.children
        );
        assert!(
            !tree.root.re_exports.is_empty(),
            "re_exports should contain imported names: {:?}",
            tree.root.re_exports
        );
    }

    #[test]
    fn test_python_module_tree_empty() {
        let tree = ModuleTree::build_from_source_for_lang("", "empty.py", Lang::Python);
        assert_eq!(tree.root.name, "empty.py");
        assert!(tree.root.children.is_empty());
    }

    // ─── TypeScript tests ───────────────────────────────────────────

    #[test]
    fn test_ts_module_tree_exports() {
        let src = r#"
import { readFile } from 'fs/promises';

export function processData(data: string): string {
    return data.trim();
}

export class DataService {
    name: string;
}

export const VERSION = "1.0.0";
"#;
        let tree = ModuleTree::build_from_source_for_lang(src, "service.ts", Lang::TypeScript);
        assert_eq!(tree.root.name, "service.ts");
        // Should have fs/promises as import dependency
        assert!(
            tree.root.children.iter().any(|c| c.name == "fs/promises"),
            "fs/promises import not found: {:?}",
            tree.root.children
        );
        // Should have exported symbols
        assert!(
            tree.root.re_exports.iter().any(|r| r == "processData"),
            "processData export not found: {:?}",
            tree.root.re_exports
        );
        assert!(
            tree.root.re_exports.iter().any(|r| r == "DataService"),
            "DataService export not found: {:?}",
            tree.root.re_exports
        );
        assert!(
            tree.root.re_exports.iter().any(|r| r == "VERSION"),
            "VERSION export not found: {:?}",
            tree.root.re_exports
        );
    }

    #[test]
    fn test_ts_module_tree_empty() {
        let tree = ModuleTree::build_from_source_for_lang("", "empty.ts", Lang::TypeScript);
        assert_eq!(tree.root.name, "empty.ts");
        assert!(tree.root.children.is_empty());
    }

    #[test]
    fn test_unsupported_lang_module_tree() {
        let tree = ModuleTree::build_from_source_for_lang("some code", "file.sh", Lang::Bash);
        assert_eq!(tree.root.name, "file.sh");
        assert!(tree.root.children.is_empty());
    }
}
