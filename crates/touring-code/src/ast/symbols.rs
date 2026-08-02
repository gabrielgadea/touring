//! Symbol extraction from AST
//!
//! Provides type-safe symbol extraction with enriched metadata:
//! - `SymbolKind` enum (type-safe, serde-transparent as string)
//! - Parent detection (method → class)
//! - Docstring extraction (first comment/docstring above symbol)
//! - Decorator/attribute extraction
//! - Async detection
//! - Cyclomatic complexity (via `complexity` module)

use std::collections::HashMap;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::str::FromStr;

use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use streaming_iterator::StreamingIterator;
use tree_sitter::{Node, Query, QueryCursor};

use touring_foundation::truncate_str;

use crate::ast::error::{AstError, AstResult};
use crate::ast::languages::Lang;

// ─── SymbolKind ──────────────────────────────────────────────────────────────

/// Type-safe symbol kind.
///
/// Serializes as a plain string (e.g., `"function"`, `"class"`) for backward
/// compatibility with existing JSON consumers.
#[derive(Debug, Clone, PartialEq, Eq, Hash, strum::EnumIter)]
pub enum SymbolKind {
    /// A free-standing function.
    Function,
    /// An asynchronous function (`async fn` / `async def`).
    AsyncFunction,
    /// A method associated with a type, trait, or class.
    Method,
    /// A class (OOP languages).
    Class,
    /// A Rust-style struct / record type.
    Struct,
    /// An enumeration type.
    Enum,
    /// A trait (Rust) or equivalent abstract interface contract.
    Trait,
    /// An `impl` block providing methods for a type.
    Impl,
    /// An interface (e.g. TypeScript `interface`).
    Interface,
    /// A type alias (`type X = ...`).
    TypeAlias,
    /// A namespace or package grouping.
    Namespace,
    /// A named compile-time constant.
    Constant,
    /// A static (global) variable.
    Static,
    /// A local or instance variable binding.
    Variable,
    /// A module (Rust `mod`, Python module, etc.).
    Module,
    /// A macro definition.
    Macro,
    /// A generator function (yields values lazily).
    Generator,
    /// Fallback for unrecognized node kinds
    Other(String),
}

impl SymbolKind {
    /// Canonical string representation (matches legacy `kind: String` values).
    pub fn as_str(&self) -> &str {
        match self {
            Self::Function => "function",
            Self::AsyncFunction => "async_function",
            Self::Method => "method",
            Self::Class => "class",
            Self::Struct => "struct",
            Self::Enum => "enum",
            Self::Trait => "trait",
            Self::Impl => "impl",
            Self::Interface => "interface",
            Self::TypeAlias => "type_alias",
            Self::Namespace => "namespace",
            Self::Constant => "const",
            Self::Static => "static",
            Self::Variable => "variable",
            Self::Module => "module",
            Self::Macro => "macro",
            Self::Generator => "generator",
            Self::Other(s) => s.as_str(),
        }
    }

    /// Whether this kind represents a callable (function, method, async fn).
    pub fn is_callable(&self) -> bool {
        matches!(
            self,
            Self::Function | Self::AsyncFunction | Self::Method | Self::Generator
        )
    }

    /// Whether this kind represents a type definition (class, struct, enum, trait, interface).
    pub fn is_type_definition(&self) -> bool {
        matches!(
            self,
            Self::Class
                | Self::Struct
                | Self::Enum
                | Self::Trait
                | Self::Interface
                | Self::TypeAlias
        )
    }

    /// Whether this kind represents a container (can have children).
    pub fn is_container(&self) -> bool {
        matches!(
            self,
            Self::Class
                | Self::Struct
                | Self::Enum
                | Self::Trait
                | Self::Impl
                | Self::Interface
                | Self::Module
                | Self::Namespace
        )
    }
}

impl fmt::Display for SymbolKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl From<&str> for SymbolKind {
    fn from(s: &str) -> Self {
        match s {
            "function" => Self::Function,
            "async_function" => Self::AsyncFunction,
            "method" => Self::Method,
            "class" => Self::Class,
            "struct" => Self::Struct,
            "enum" => Self::Enum,
            "trait" => Self::Trait,
            "impl" => Self::Impl,
            "interface" => Self::Interface,
            "type_alias" => Self::TypeAlias,
            "namespace" => Self::Namespace,
            "const" => Self::Constant,
            "static" => Self::Static,
            "variable" => Self::Variable,
            "module" => Self::Module,
            "macro" => Self::Macro,
            "generator" => Self::Generator,
            "other" => Self::Other("other".into()),
            other => Self::Other(other.into()),
        }
    }
}

impl From<String> for SymbolKind {
    fn from(s: String) -> Self {
        Self::from(s.as_str())
    }
}

/// Implements `FromStr` so that `"function".parse::<SymbolKind>()` works.
///
/// This is infallible — unrecognized strings become `SymbolKind::Other(s)`.
impl FromStr for SymbolKind {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self::from(s))
    }
}

// Backward compatibility: allow `s.kind == "function"` in existing code
impl PartialEq<&str> for SymbolKind {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

impl PartialEq<str> for SymbolKind {
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}

impl PartialEq<String> for SymbolKind {
    fn eq(&self, other: &String) -> bool {
        self.as_str() == other.as_str()
    }
}

// Transparent serde: serializes as a plain string
impl Serialize for SymbolKind {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for SymbolKind {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        Ok(Self::from(s))
    }
}

// ─── Visibility ──────────────────────────────────────────────────────────────

/// Symbol visibility level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Visibility {
    /// Visible to all external consumers (`pub`).
    Public,
    /// Visible only within its defining scope.
    Private,
    /// Visible to the defining type and its subtypes/descendants.
    Protected,
    /// Visible within the defining crate (`pub(crate)`).
    Crate,
    /// Visible within the defining module (`pub(in module)`).
    Module,
}

impl Visibility {
    /// Return the visibility level as a lowercase string slice (e.g., `"public"`, `"private"`).
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Public => "public",
            Self::Private => "private",
            Self::Protected => "protected",
            Self::Crate => "crate",
            Self::Module => "module",
        }
    }
}

impl fmt::Display for Visibility {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

// ─── Symbol ──────────────────────────────────────────────────────────────────

/// A code symbol (function, class, struct, etc.) with enriched metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Symbol {
    /// Symbol name (e.g., "my_function")
    pub name: String,
    /// Type-safe symbol kind — serializes as string for backward compat
    pub kind: SymbolKind,
    /// Start line (1-indexed)
    pub line: usize,
    /// End line (1-indexed)
    pub end_line: usize,
    /// Start column (0-indexed, byte offset within line)
    pub column: usize,
    /// Start byte in source
    pub start_byte: usize,
    /// End byte in source
    pub end_byte: usize,
    /// Signature without body (e.g., "def foo(x: int) -> str:")
    pub signature: String,
    /// Whether the symbol is public (legacy field, see `visibility` for detail)
    pub is_public: bool,

    // ── Enriched fields (all backward-compatible defaults) ──
    /// Name of the parent container (e.g., "MyClass" for a method)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_name: Option<String>,

    /// First line of the docstring/doc comment above the symbol
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub docstring: Option<String>,

    /// Decorator names (Python: `@staticmethod`, Rust: `derive(Debug)`)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub decorators: Vec<String>,

    /// Cyclomatic complexity (populated by `complexity::compute_for_node`)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub complexity: Option<u16>,

    /// Whether the function/method is async
    #[serde(default)]
    pub is_async: bool,

    /// Detailed visibility level
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visibility: Option<Visibility>,

    /// INS-A5: structural hash for clone detection.
    ///
    /// Computed from `(kind, param_count, complexity, end_line - line)`.
    /// Two symbols with the same hash are structural clones candidates.
    /// Populated lazily by `compute_structural_hash()`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub structural_hash: Option<u64>,
}

impl Default for Symbol {
    fn default() -> Self {
        Self {
            name: String::new(),
            kind: SymbolKind::Function,
            line: 0,
            end_line: 0,
            column: 0,
            start_byte: 0,
            end_byte: 0,
            signature: String::new(),
            is_public: false,
            parent_name: None,
            docstring: None,
            decorators: Vec::new(),
            complexity: None,
            is_async: false,
            visibility: None,
            structural_hash: None,
        }
    }
}

impl Symbol {
    /// Create a new symbol (backward-compatible constructor).
    ///
    /// `kind` accepts both `&str` and `SymbolKind` via `Into<SymbolKind>`.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        name: impl Into<String>,
        kind: impl Into<SymbolKind>,
        line: usize,
        end_line: usize,
        column: usize,
        start_byte: usize,
        end_byte: usize,
        signature: impl Into<String>,
        is_public: bool,
    ) -> Self {
        Self {
            name: name.into(),
            kind: kind.into(),
            line,
            end_line,
            column,
            start_byte,
            end_byte,
            signature: signature.into(),
            is_public,
            parent_name: None,
            docstring: None,
            decorators: Vec::new(),
            complexity: None,
            is_async: false,
            visibility: None,
            structural_hash: None,
        }
    }

    /// Builder: set parent name
    pub fn with_parent(mut self, parent: impl Into<String>) -> Self {
        self.parent_name = Some(parent.into());
        self
    }

    /// Builder: set docstring
    pub fn with_docstring(mut self, doc: impl Into<String>) -> Self {
        self.docstring = Some(doc.into());
        self
    }

    /// Builder: set decorators
    pub fn with_decorators(mut self, decorators: Vec<String>) -> Self {
        self.decorators = decorators;
        self
    }

    /// Builder: set complexity
    pub fn with_complexity(mut self, complexity: u16) -> Self {
        self.complexity = Some(complexity);
        self
    }

    /// Builder: set async
    pub fn with_async(mut self, is_async: bool) -> Self {
        self.is_async = is_async;
        self
    }

    /// Builder: set visibility
    pub fn with_visibility(mut self, vis: Visibility) -> Self {
        self.visibility = Some(vis);
        self
    }

    /// Number of lines spanned by this symbol
    pub fn line_count(&self) -> usize {
        if self.end_line >= self.line {
            self.end_line - self.line + 1
        } else {
            1
        }
    }

    /// INS-A5: Compute and cache the structural hash for this symbol.
    ///
    /// Hash inputs: `kind`, param count (inferred from signature commas + 1),
    /// complexity bucket (0/1/2/3+), and line count bucket (1-5/6-20/21+).
    /// Returns the hash value (also stored in `self.structural_hash`).
    pub fn compute_structural_hash(&mut self) -> u64 {
        let param_count = count_params(&self.signature);
        let complexity_bucket = match self.complexity {
            None | Some(0) | Some(1) => 0u8,
            Some(2..=5) => 1,
            Some(6..=15) => 2,
            _ => 3,
        };
        let line_bucket = match self.line_count() {
            1..=5 => 0u8,
            6..=20 => 1,
            _ => 2,
        };

        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.kind.as_str().hash(&mut hasher);
        param_count.hash(&mut hasher);
        complexity_bucket.hash(&mut hasher);
        line_bucket.hash(&mut hasher);
        let h = hasher.finish();
        self.structural_hash = Some(h);
        h
    }

    /// Extract signature from source and node range (without body)
    fn extract_signature(source: &str, node: Node) -> String {
        let start_byte = node.start_byte();
        let end_byte = node.end_byte();

        let node_text = &source[start_byte..end_byte.min(source.len())];

        // For Python-style signatures, the body colon is ALWAYS at
        // bracket depth 0. Type-hint colons (e.g. `x: int`) live inside
        // parentheses (depth >= 1) and must be skipped.
        let body_colon_pos = find_depth_zero_colon(node_text);

        let signature = if let Some(colon_pos) = body_colon_pos {
            node_text[..=colon_pos].to_string()
        } else if let Some(brace_pos) = node_text.find('{') {
            node_text[..brace_pos].trim().to_string()
        } else if let Some(equal_pos) = node_text.find('=') {
            node_text[..equal_pos].trim().to_string()
        } else {
            node_text.lines().next().unwrap_or(node_text).to_string()
        };

        if signature.len() > 200 {
            format!("{}...", truncate_str(&signature, 200))
        } else {
            signature
        }
    }

    /// Detect visibility from syntax
    fn detect_visibility(source: &str, node: Node, lang: Lang) -> (bool, Option<Visibility>) {
        let node_text = &source[node.start_byte()..node.end_byte()];

        match lang {
            Lang::Rust => {
                let trimmed = node_text.trim_start();
                if trimmed.starts_with("pub(crate)") {
                    // pub(crate): visible within crate, considered public for API purposes
                    (true, Some(Visibility::Crate))
                } else if trimmed.starts_with("pub(super)") {
                    // pub(super): visible to parent module only — NOT truly public
                    (false, Some(Visibility::Module))
                } else if trimmed.starts_with("pub ") || trimmed.starts_with("pub(") {
                    (true, Some(Visibility::Public))
                } else {
                    (false, Some(Visibility::Private))
                }
            }
            Lang::Python => {
                // Python: _prefix = private, __prefix = name-mangled/protected
                let name_start = node_text.find("def ").or_else(|| node_text.find("class "));
                if let Some(pos) = name_start {
                    let after = &node_text[pos..];
                    let name = after.split_whitespace().nth(1).unwrap_or("");
                    if name.starts_with("__") && !name.ends_with("__") {
                        (false, Some(Visibility::Protected))
                    } else if name.starts_with('_') {
                        (false, Some(Visibility::Private))
                    } else {
                        (true, Some(Visibility::Public))
                    }
                } else {
                    (true, None)
                }
            }
            Lang::TypeScript | Lang::JavaScript => {
                if node_text.contains("export ") {
                    (true, Some(Visibility::Public))
                } else {
                    (false, Some(Visibility::Module))
                }
            }
            // Data/markup languages don't have visibility semantics
            _ => (true, None),
        }
    }

    /// Detect if a function/method is async
    fn detect_async(source: &str, node: Node, lang: Lang) -> bool {
        let node_text = &source[node.start_byte()..node.end_byte().min(source.len())];
        match lang {
            Lang::Python => node_text.trim_start().starts_with("async "),
            Lang::Rust => node_text.contains("async fn"),
            Lang::TypeScript | Lang::JavaScript => {
                node_text.trim_start().starts_with("async ") || node_text.contains("async function")
            }
            // Non-code languages don't have async semantics
            _ => false,
        }
    }

    /// Extract the parent container name for a nested symbol.
    ///
    /// Walks up the tree looking for a class/struct/impl/trait/interface container.
    fn find_parent_name(source: &str, node: Node, lang: Lang) -> Option<String> {
        let mut current = node.parent()?;
        loop {
            match lang {
                Lang::Python if current.kind() == "class_definition" => {
                    return extract_child_name(source, current, "name");
                }
                Lang::Rust => {
                    if matches!(current.kind(), "impl_item" | "trait_item") {
                        // For impl: get the type identifier
                        return extract_child_name(source, current, "type")
                            .or_else(|| extract_child_name(source, current, "name"));
                    }
                }
                Lang::TypeScript | Lang::JavaScript
                    if (current.kind() == "class_declaration" || current.kind() == "class") =>
                {
                    return extract_child_name(source, current, "name");
                }
                // Non-code languages don't have container semantics
                _ => {}
            }

            // Stop at module/program/document root
            if matches!(
                current.kind(),
                "module" | "program" | "source_file" | "document" | "stream"
            ) {
                return None;
            }

            current = current.parent()?;
        }
    }

    /// Extract decorators/attributes from the node or its parent.
    fn extract_decorators(source: &str, node: Node, lang: Lang) -> Vec<String> {
        let mut decorators = Vec::new();

        match lang {
            Lang::Python => {
                // Check if parent is a decorated_definition
                if let Some(parent) = node.parent() {
                    if parent.kind() == "decorated_definition" {
                        let mut cursor = parent.walk();
                        for child in parent.children(&mut cursor) {
                            if child.kind() == "decorator" {
                                if let Ok(text) = child.utf8_text(source.as_bytes()) {
                                    // Strip the @ prefix and whitespace
                                    let name = text.trim().strip_prefix('@').unwrap_or(text.trim());
                                    decorators.push(name.to_string());
                                }
                            }
                        }
                    }
                }
                // Also check for decorators directly on the node (if node IS the decorated_definition)
                if node.kind() == "decorated_definition" {
                    let mut cursor = node.walk();
                    for child in node.children(&mut cursor) {
                        if child.kind() == "decorator" {
                            if let Ok(text) = child.utf8_text(source.as_bytes()) {
                                let name = text.trim().strip_prefix('@').unwrap_or(text.trim());
                                if !decorators.contains(&name.to_string()) {
                                    decorators.push(name.to_string());
                                }
                            }
                        }
                    }
                }
            }
            Lang::Rust => {
                // Look for attribute_item siblings above the node
                if let Some(parent) = node.parent() {
                    let node_idx = {
                        let mut idx = 0;
                        let mut cursor = parent.walk();
                        for (i, child) in parent.children(&mut cursor).enumerate() {
                            if child.id() == node.id() {
                                idx = i;
                                break;
                            }
                        }
                        idx
                    };
                    // Scan backwards from the node position for attribute_items
                    for i in (0..node_idx).rev() {
                        if let Some(sibling) = parent.child(i as u32) {
                            if sibling.kind() == "attribute_item" {
                                if let Ok(text) = sibling.utf8_text(source.as_bytes()) {
                                    let attr = text
                                        .trim()
                                        .strip_prefix("#[")
                                        .and_then(|s| s.strip_suffix(']'))
                                        .unwrap_or(text.trim());
                                    decorators.push(attr.to_string());
                                }
                            } else {
                                break; // Stop at first non-attribute
                            }
                        }
                    }
                    decorators.reverse(); // Restore top-down order
                }
            }
            Lang::TypeScript | Lang::JavaScript => {
                // TS/JS decorators (experimental): look for decorator nodes
                if let Some(parent) = node.parent() {
                    let mut cursor = parent.walk();
                    for child in parent.children(&mut cursor) {
                        if child.kind() == "decorator" && child.end_byte() <= node.start_byte() {
                            if let Ok(text) = child.utf8_text(source.as_bytes()) {
                                let name = text.trim().strip_prefix('@').unwrap_or(text.trim());
                                decorators.push(name.to_string());
                            }
                        }
                    }
                }
            }
            // Non-code languages don't have decorators
            _ => {}
        }

        decorators
    }

    /// Extract the docstring/doc comment immediately preceding a symbol.
    fn extract_docstring(source: &str, node: Node, lang: Lang) -> Option<String> {
        match lang {
            Lang::Python => {
                // Python docstrings: first expression_statement with a string
                // inside a function/class body block
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if child.kind() == "block" {
                        // Only check the FIRST statement in block (child(0))
                        if let Some(block_child) = child.child(0) {
                            if block_child.kind() == "expression_statement" {
                                let mut expr_cursor = block_child.walk();
                                for expr_child in block_child.children(&mut expr_cursor) {
                                    if expr_child.kind() == "string"
                                        || expr_child.kind() == "concatenated_string"
                                    {
                                        if let Ok(text) = expr_child.utf8_text(source.as_bytes()) {
                                            let cleaned = text
                                                .trim()
                                                .trim_start_matches("\"\"\"")
                                                .trim_start_matches("'''")
                                                .trim_end_matches("\"\"\"")
                                                .trim_end_matches("'''")
                                                .trim();
                                            let first_line =
                                                cleaned.lines().next().unwrap_or(cleaned).trim();
                                            if !first_line.is_empty() {
                                                return Some(
                                                    truncate_str(first_line, 120).to_string(),
                                                );
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                None
            }
            Lang::Rust => {
                // Rust: look for /// or //! comments immediately above
                extract_adjacent_comments(source, node, "///")
                    .or_else(|| extract_adjacent_comments(source, node, "//!"))
            }
            Lang::TypeScript | Lang::JavaScript => {
                // JSDoc: /** ... */ comment immediately above
                extract_jsdoc_comment(source, node)
            }
            Lang::Bash => {
                // Bash: # comments above functions
                extract_adjacent_comments(source, node, "#")
            }
            // Data/markup languages: no docstring semantics
            _ => None,
        }
    }

    /// Get the node kind as a SymbolKind
    pub(crate) fn node_kind_to_symbol_kind(kind: &str, lang: Lang) -> SymbolKind {
        match lang {
            Lang::Python => match kind {
                "function_definition" | "decorated_definition" => SymbolKind::Function,
                "class_definition" => SymbolKind::Class,
                "expression_statement" | "assignment" => SymbolKind::Variable,
                "type_alias_statement" | "type" => SymbolKind::TypeAlias,
                _ => SymbolKind::Other(kind.to_string()),
            },
            Lang::Rust => match kind {
                "function_item" | "function_signature_item" => SymbolKind::Function,
                "struct_item" => SymbolKind::Struct,
                "enum_item" => SymbolKind::Enum,
                "trait_item" => SymbolKind::Trait,
                "impl_item" => SymbolKind::Impl,
                "const_item" => SymbolKind::Constant,
                "static_item" => SymbolKind::Static,
                "type_item" => SymbolKind::TypeAlias,
                "macro_definition" => SymbolKind::Macro,
                "mod_item" => SymbolKind::Module,
                _ => SymbolKind::Other(kind.to_string()),
            },
            Lang::TypeScript => match kind {
                "function_declaration" | "arrow_function" | "variable_declarator" => {
                    SymbolKind::Function
                }
                "class_declaration" => SymbolKind::Class,
                "interface_declaration" => SymbolKind::Interface,
                "type_alias_declaration" => SymbolKind::TypeAlias,
                "enum_declaration" => SymbolKind::Enum,
                "internal_module" => SymbolKind::Namespace,
                "method_definition" => SymbolKind::Method,
                _ => SymbolKind::Other(kind.to_string()),
            },
            Lang::JavaScript => match kind {
                "function_declaration"
                | "arrow_function"
                | "function_expression"
                | "variable_declarator" => SymbolKind::Function,
                "class_declaration" => SymbolKind::Class,
                "method_definition" => SymbolKind::Method,
                "generator_function_declaration" => SymbolKind::Generator,
                _ => SymbolKind::Other(kind.to_string()),
            },
            Lang::Bash => match kind {
                "function_definition" => SymbolKind::Function,
                "variable_assignment" => SymbolKind::Variable,
                _ => SymbolKind::Other(kind.to_string()),
            },
            Lang::Html => match kind {
                "script_element" | "style_element" | "element" => {
                    SymbolKind::Other(kind.to_string())
                }
                _ => SymbolKind::Other(kind.to_string()),
            },
            Lang::Css => match kind {
                "rule_set" => SymbolKind::Other("selector".to_string()),
                "media_statement" => SymbolKind::Other("media".to_string()),
                "keyframes_statement" => SymbolKind::Other("keyframes".to_string()),
                _ => SymbolKind::Other(kind.to_string()),
            },
            Lang::Markdown => match kind {
                "atx_heading" => SymbolKind::Other("heading".to_string()),
                _ => SymbolKind::Other(kind.to_string()),
            },
            Lang::Json => match kind {
                "pair" => SymbolKind::Other("key".to_string()),
                _ => SymbolKind::Other(kind.to_string()),
            },
            Lang::Toml => match kind {
                "table" | "dotted_table" => SymbolKind::Other("table".to_string()),
                "pair" => SymbolKind::Other("key".to_string()),
                _ => SymbolKind::Other(kind.to_string()),
            },
            Lang::Yaml => match kind {
                "block_mapping_pair" => SymbolKind::Other("key".to_string()),
                _ => SymbolKind::Other(kind.to_string()),
            },
            #[cfg(feature = "more-languages")]
            Lang::Go => match kind {
                "function_declaration" => SymbolKind::Function,
                "method_declaration" => SymbolKind::Method,
                "type_spec" => SymbolKind::TypeAlias, // struct, interface, type alias
                "const_spec" => SymbolKind::Constant,
                _ => SymbolKind::Other(kind.to_string()),
            },
            #[cfg(feature = "more-languages")]
            Lang::Java => match kind {
                "method_declaration" => SymbolKind::Method,
                "class_declaration" => SymbolKind::Class,
                "interface_declaration" => SymbolKind::Interface,
                "constructor_declaration" => SymbolKind::Function,
                "enum_declaration" => SymbolKind::Enum,
                "record_declaration" => SymbolKind::Struct,
                _ => SymbolKind::Other(kind.to_string()),
            },
        }
    }

    /// Build a [`SymbolPath`] from this symbol's parent chain.
    pub fn to_path(&self, file_path: &str) -> SymbolPath {
        let mut segments = Vec::new();
        if let Some(ref parent) = self.parent_name {
            segments.push(parent.clone());
        }
        segments.push(self.name.clone());
        SymbolPath::new(segments, file_path.to_string())
    }
}

// ─── SymbolPath ──────────────────────────────────────────────────────────────

/// Fully-qualified symbol path with scope resolution.
///
/// Represents a symbol in its full namespace context, e.g., `MyClass::my_method`.
/// Used for disambiguating symbols with the same leaf name across different scopes.
#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct SymbolPath {
    /// Ordered segments from outermost to innermost scope.
    /// E.g., `["MyModule", "MyClass", "my_method"]`
    pub segments: Vec<String>,
    /// File containing this symbol.
    pub file_path: String,
}

impl SymbolPath {
    /// Create a new symbol path.
    pub fn new(segments: Vec<String>, file_path: String) -> Self {
        Self {
            segments,
            file_path,
        }
    }

    /// Full qualified name joined with `::`.
    /// E.g., `"MyClass::my_method"`
    pub fn qualified_name(&self) -> String {
        self.segments.join("::")
    }

    /// Leaf name (last segment).
    /// E.g., `"my_method"` for `"MyClass::my_method"`
    pub fn leaf_name(&self) -> &str {
        self.segments.last().map(|s| s.as_str()).unwrap_or("")
    }

    /// Parent path (all segments except last).
    /// Returns `None` for single-segment paths.
    pub fn parent(&self) -> Option<SymbolPath> {
        if self.segments.len() > 1 {
            Some(SymbolPath {
                segments: self
                    .segments
                    .get(..self.segments.len() - 1)
                    .unwrap_or_default()
                    .to_vec(),
                file_path: self.file_path.clone(),
            })
        } else {
            None
        }
    }

    /// Number of nesting levels (1 = top-level, 2 = one parent, etc.)
    pub fn depth(&self) -> usize {
        self.segments.len()
    }

    /// Check if this path starts with another path (ancestor check).
    pub fn starts_with(&self, prefix: &SymbolPath) -> bool {
        self.file_path == prefix.file_path
            && self.segments.len() >= prefix.segments.len()
            && (self.segments.get(..prefix.segments.len()) == Some(prefix.segments.as_slice()))
    }
}

impl fmt::Display for SymbolPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.qualified_name())
    }
}

// ─── INS-A5: Structural clone detection ─────────────────────────────────────

/// Count parameters in a signature string (comma-based heuristic).
///
/// Handles `()` (0 params), `(x)` (1 param), `(x, y)` (2 params).
/// Ignores commas inside nested brackets.
pub fn count_params(signature: &str) -> usize {
    let open = signature.find('(');
    let close = signature.rfind(')');
    match (open, close) {
        (Some(o), Some(c)) if c > o => {
            let inner = &signature[o + 1..c];
            if inner.trim().is_empty() {
                0
            } else {
                // Count top-level commas only (ignore nested angle/square brackets).
                let mut depth: usize = 0;
                let mut commas: usize = 0;
                for ch in inner.chars() {
                    match ch {
                        '<' | '[' | '(' => depth += 1,
                        '>' | ']' | ')' => depth = depth.saturating_sub(1),
                        ',' if depth == 0 => commas += 1,
                        _ => {}
                    }
                }
                commas + 1
            }
        }
        _ => 0,
    }
}

/// INS-A5: Find structural clones across a collection of symbols.
///
/// Groups symbols by `structural_hash` and returns groups with ≥ 2 members.
/// Symbols without a pre-computed hash are hashed on-the-fly.
///
/// Returns a `Vec` of clone groups, each group being a `Vec<Symbol>`.
pub fn find_clones(symbols: &mut [Symbol]) -> Vec<Vec<Symbol>> {
    // Compute missing hashes.
    for sym in symbols.iter_mut() {
        if sym.structural_hash.is_none() {
            sym.compute_structural_hash();
        }
    }

    // Group by hash.
    let mut groups: HashMap<u64, Vec<Symbol>> = HashMap::new();
    for sym in symbols.iter() {
        if let Some(h) = sym.structural_hash {
            groups.entry(h).or_default().push(sym.clone());
        }
    }

    // Return only groups with ≥ 2 members (actual clones).
    groups.into_values().filter(|g| g.len() >= 2).collect()
}

// ─── Helper functions ────────────────────────────────────────────────────────

/// Extract a named child's text from a node
fn extract_child_name(source: &str, node: Node, field_name: &str) -> Option<String> {
    let child = node.child_by_field_name(field_name)?;
    child
        .utf8_text(source.as_bytes())
        .ok()
        .map(|s| s.to_string())
}

/// Extract adjacent line comments (/// or //!) above a node
fn extract_adjacent_comments(source: &str, node: Node, prefix: &str) -> Option<String> {
    let node_start_line = node.start_position().row;
    if node_start_line == 0 {
        return None;
    }

    // Scan source lines above the node
    let lines: Vec<&str> = source.lines().collect();
    let mut doc_lines = Vec::new();

    for i in (0..node_start_line).rev() {
        let line = lines.get(i)?.trim();
        if line.starts_with(prefix) {
            let content = line.strip_prefix(prefix).unwrap_or("").trim();
            doc_lines.push(content);
        } else if line.is_empty() {
            continue; // Skip blank lines between doc comments
        } else {
            break;
        }
    }

    if doc_lines.is_empty() {
        return None;
    }

    doc_lines.reverse();
    let first_line = {
        let line = doc_lines.first()?;
        *line
    };
    if first_line.is_empty() {
        doc_lines.get(1).map(|s| truncate_str(s, 120).to_string())
    } else {
        Some(truncate_str(first_line, 120).to_string())
    }
}

/// Extract JSDoc comment (/** ... */) above a node
fn extract_jsdoc_comment(source: &str, node: Node) -> Option<String> {
    // Look at previous sibling for a comment node
    let mut prev = node.prev_sibling();
    // Skip export_statement wrapper
    if prev.is_none() {
        if let Some(parent) = node.parent() {
            if parent.kind() == "export_statement" {
                prev = parent.prev_sibling();
            }
        }
    }

    if let Some(prev_node) = prev {
        if prev_node.kind() == "comment" {
            if let Ok(text) = prev_node.utf8_text(source.as_bytes()) {
                let text = text.trim();
                if text.starts_with("/**") {
                    let cleaned = text
                        .strip_prefix("/**")
                        .and_then(|s| s.strip_suffix("*/"))
                        .unwrap_or(text)
                        .trim();
                    let first_line = cleaned
                        .lines()
                        .map(|l| l.trim().trim_start_matches('*').trim())
                        .find(|l| !l.is_empty())?;
                    return Some(truncate_str(first_line, 120).to_string());
                }
            }
        }
    }

    None
}

/// Find the first colon at bracket depth 0 in a string.
///
/// Tracks `(`, `[`, `{` as depth increments and `)`, `]`, `}` as
/// decrements. Only a `:` encountered at depth 0 is the Python body
/// colon; colons inside parentheses are type hints (e.g. `x: int`).
pub fn find_depth_zero_colon(text: &str) -> Option<usize> {
    let mut depth = 0i32;
    for (i, ch) in text.char_indices() {
        match ch {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            ':' if depth == 0 => return Some(i),
            _ => {}
        }
    }
    None
}

// ─── Core extraction ─────────────────────────────────────────────────────────

/// Extract symbols from a parsed tree with full enrichment.
///
/// This is the core extraction logic shared by both pool-aware and standalone paths.
/// Refine a module-level binding's [`SymbolKind`] from its name convention.
///
/// `SCREAMING_SNAKE_CASE` (PEP 8 / JS `const`) and dunder metadata
/// (`__all__`, `__version__`, `__author__`) are classified as
/// [`SymbolKind::Constant`]; all other [`SymbolKind::Variable`] bindings are
/// left unchanged. Non-`Variable` kinds pass through untouched. This sharpens
/// index classification without dropping any symbol (REGRA #0 — keep, refine,
/// never reduce).
pub(crate) fn refine_binding_kind(name: &str, kind: SymbolKind) -> SymbolKind {
    if kind != SymbolKind::Variable {
        return kind;
    }
    let is_dunder = name.len() > 4 && name.starts_with("__") && name.ends_with("__");
    let is_screaming = name.chars().any(|c| c.is_ascii_uppercase())
        && name
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_');
    if is_screaming || is_dunder {
        SymbolKind::Constant
    } else {
        kind
    }
}

fn extract_symbols_from_tree(
    tree: &tree_sitter::Tree,
    source: &str,
    lang: Lang,
) -> AstResult<Vec<Symbol>> {
    let root_node = tree.root_node();

    let query_text = lang.query_file();
    let query = Query::new(&lang.tree_sitter_language(), query_text)
        .map_err(|e| AstError::QueryError(format!("{:?}", e)))?;

    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(&query, root_node, source.as_bytes());

    let mut symbols = Vec::new();
    let mut seen_ranges: std::collections::HashSet<(usize, usize)> =
        std::collections::HashSet::new();

    while let Some(m) = matches.next() {
        for capture in m.captures {
            let capture_name = match query.capture_names().get(capture.index as usize) {
                Some(n) => *n,
                None => continue,
            };
            if capture_name == "name" {
                let node = capture.node;
                let parent: Option<tree_sitter::Node> = node.parent();

                if let Some(parent) = parent {
                    let range_key: (usize, usize) = (parent.start_byte(), parent.end_byte());

                    if seen_ranges.contains(&range_key) {
                        continue;
                    }
                    seen_ranges.insert(range_key);

                    let name = node.utf8_text(source.as_bytes()).unwrap_or("").to_string();
                    let mut kind = Symbol::node_kind_to_symbol_kind(parent.kind(), lang);
                    let start_pos = parent.start_position();
                    let line = start_pos.row + 1;
                    let end_line = parent.end_position().row + 1;
                    let column = start_pos.column;
                    let start_byte = parent.start_byte();
                    let end_byte = parent.end_byte();
                    let signature = Symbol::extract_signature(source, parent);
                    let (is_public, visibility) = Symbol::detect_visibility(source, parent, lang);
                    let is_async = Symbol::detect_async(source, parent, lang);
                    let parent_name = Symbol::find_parent_name(source, parent, lang);
                    let decorators = Symbol::extract_decorators(source, parent, lang);
                    let docstring = Symbol::extract_docstring(source, parent, lang);

                    // Upgrade Function to AsyncFunction or Method based on context
                    if is_async && kind == SymbolKind::Function {
                        kind = SymbolKind::AsyncFunction;
                    }
                    if parent_name.is_some() && kind == SymbolKind::Function {
                        kind = SymbolKind::Method;
                    }
                    // Refine module-level bindings (Variable) into Constant when the
                    // name follows SCREAMING_SNAKE_CASE / dunder convention — sharpens
                    // index classification without dropping the symbol (REGRA #0).
                    kind = refine_binding_kind(&name, kind);

                    let mut sym = Symbol::new(
                        name, kind, line, end_line, column, start_byte, end_byte, signature,
                        is_public,
                    );
                    sym.is_async = is_async;
                    sym.visibility = visibility;
                    sym.parent_name = parent_name;
                    sym.decorators = decorators;
                    sym.docstring = docstring;

                    symbols.push(sym);
                }
            }
        }
    }

    // Bug 7 fix (2026-05-02): Python __all__ post-process to reduce orphan FPs.
    // When a Python file declares `__all__ = [...]`, only listed names are
    // truly public. Symbols not in __all__ are demoted to Module visibility,
    // so `touring wiring orphans` doesn't flag them as orphan pub symbols.
    if lang == Lang::Python {
        if let Some(all_list) = parse_python_all(source) {
            for sym in symbols.iter_mut() {
                if sym.is_public && !all_list.contains(&sym.name) {
                    sym.is_public = false;
                    sym.visibility = Some(Visibility::Module);
                }
            }
        }
    }

    // Sort by line number
    symbols.sort_by_key(|s| s.line);

    Ok(symbols)
}

/// Bug 7 fix (2026-05-02): parse Python `__all__ = [...]` declarations.
///
/// Returns the set of names listed in `__all__`, or `None` if not declared.
/// When present, only listed names are considered truly public; other
/// non-underscore names are demoted to Module visibility to avoid orphan FPs.
///
/// Supports common forms:
///   `__all__ = ["foo", "bar"]`
///   `__all__: list[str] = ["foo", "bar"]`
///   `__all__ = ('foo', 'bar')`
fn parse_python_all(source: &str) -> Option<std::collections::HashSet<String>> {
    use regex::Regex;
    // Match __all__ = [ ... ] or __all__ = ( ... ), with optional type annotation.
    let outer = Regex::new(r#"__all__\s*(?::[^=]+)?\s*=\s*[\[\(]([^\]\)]*)[\]\)]"#).ok()?;
    let cap = outer.captures(source)?;
    let body = cap.get(1)?.as_str();

    // Extract quoted strings from the body — both single and double quotes.
    let inner = Regex::new(r#"["']([^"']+)["']"#).ok()?;
    let names: std::collections::HashSet<String> = inner
        .captures_iter(body)
        .filter_map(|c| c.get(1).map(|m| m.as_str().to_string()))
        .collect();

    if names.is_empty() { None } else { Some(names) }
}

/// Extract symbols from source code using a `ParserPool` for parser reuse.
///
/// This is the preferred path when a pool is available (e.g., in long-lived services).
pub fn extract_symbols_with_pool(
    source: &str,
    lang: Lang,
    pool: &crate::ast::parser::ParserPool,
) -> AstResult<Vec<Symbol>> {
    let tree = pool.parse(source, lang)?;
    extract_symbols_from_tree(&tree, source, lang)
}

/// Extract symbols from source code (standalone, creates its own parser).
///
/// Backward-compatible entry point. For repeated calls, prefer
/// [`extract_symbols_with_pool`] to avoid re-creating parsers.
pub fn extract_symbols(source: &str, lang: Lang) -> AstResult<Vec<Symbol>> {
    let pool = crate::ast::parser::ParserPool::new();
    extract_symbols_with_pool(source, lang, &pool)
}

/// Extract symbols from a specific file path
pub fn extract_symbols_from_file(path: &std::path::Path) -> AstResult<Vec<Symbol>> {
    let lang =
        Lang::from_path(path).ok_or_else(|| AstError::UnknownLanguage(format!("{:?}", path)))?;

    let source = std::fs::read_to_string(path)?;

    extract_symbols(&source, lang)
}

// ─── Batch extraction (parallel via rayon) ───────────────────────────────────

/// Process multiple (path, source) pairs in parallel and return results.
///
/// Each entry in the returned `Vec` is `(path, Result<Vec<Symbol>, AstError>)`.
/// Files whose language cannot be detected yield `AstError::UnsupportedLanguage`.
///
/// Uses rayon for parallelism — the thread pool is shared process-wide.
pub fn extract_symbols_batch(
    files: &[(String, String)],
) -> Vec<(String, Result<Vec<Symbol>, AstError>)> {
    files
        .par_iter()
        .map(|(path, source)| {
            let result = match Lang::from_path(std::path::Path::new(path)) {
                Some(lang) => extract_symbols(source, lang)
                    .map_err(|e: AstError| AstError::ParseFailed(e.to_string())),
                None => Err(AstError::UnsupportedLanguage(path.clone())),
            };
            (path.clone(), result)
        })
        .collect()
}

// ─── Symbol filtering utilities ──────────────────────────────────────────────

/// Filter symbols by kind, returning references to matching symbols.
pub fn filter_by_kind(symbols: &[Symbol], kind: SymbolKind) -> Vec<&Symbol> {
    symbols.iter().filter(|s| s.kind == kind).collect()
}

/// Filter symbols by minimum cyclomatic complexity.
///
/// Symbols without a complexity value are excluded.
pub fn filter_by_complexity(symbols: &[Symbol], min_cc: u16) -> Vec<&Symbol> {
    symbols
        .iter()
        .filter(|s| s.complexity.is_some_and(|cc| cc >= min_cc))
        .collect()
}

/// Find a symbol by exact name, returning the first match.
pub fn find_by_name<'a>(symbols: &'a [Symbol], name: &str) -> Option<&'a Symbol> {
    symbols.iter().find(|s| s.name == name)
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
#[path = "symbols_tests.rs"]
mod tests;
