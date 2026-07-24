//! Definition enum — unified semantic representation across all languages
//!
//! Inspired by rust-analyzer's `hir::Definition`. Provides a single enum
//! covering Function, Struct, Trait, Module, Variant, Macro, Field,
//! Variable, Lifetime, and Generic — with language-specific variants.

use serde::{Deserialize, Serialize};
use std::fmt;
use tree_sitter::Node;

/// A unified definition representation.
///
/// Covers Rust (10 variants) and maps from other languages (JS/TS/Python/Go)
/// to a subset (Function/Struct/Variable/Module).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Definition {
    // ── Rust-rich variants ────────────────────────────────────────────
    /// Function definition (free fn or method)
    Function(DefinitionId),
    /// Struct definition
    Struct(DefinitionId),
    /// Trait definition
    Trait(DefinitionId),
    /// Module definition
    Module(DefinitionId),
    /// Enum variant (e.g., `Some`, `None`, `Variant(i32)`)
    Variant(DefinitionId),
    /// Macro definition (`macro_rules!` or `macro!`)
    Macro(DefinitionId),
    /// Field definition (struct or enum field)
    Field(DefinitionId),
    /// Local variable or constant
    Variable(DefinitionId),
    /// Lifetime parameter (`'a`, `'static`)
    Lifetime(DefinitionId),
    /// Generic parameter (`<T>`, `<T: Trait>`)
    Generic(DefinitionId),

    // ── Multi-language subset (lowered representation) ─────────────────
    /// Class (JS/TS/Go/Python) — maps to Struct variant
    Class(DefinitionId),
    /// Interface (TS) — maps to Trait variant
    Interface(DefinitionId),
    /// Enum (Rust/Go/Python) — maps to Struct + Variant pair
    Enum(DefinitionId),
    /// Type alias — maps to Trait variant
    TypeAlias(DefinitionId),
    /// Namespace/module (JS/TS namespace, Python module)
    Namespace(DefinitionId),
    /// Parameter in a function signature
    Parameter(DefinitionId),
    /// Property (JS/TS object property)
    Property(DefinitionId),
}

impl Definition {
    /// Returns the kind of this definition (for dispatch/filtering).
    pub fn kind(&self) -> DefinitionKind {
        match self {
            Definition::Function(_) => DefinitionKind::Function,
            Definition::Struct(_) => DefinitionKind::Struct,
            Definition::Trait(_) => DefinitionKind::Trait,
            Definition::Module(_) => DefinitionKind::Module,
            Definition::Variant(_) => DefinitionKind::Variant,
            Definition::Macro(_) => DefinitionKind::Macro,
            Definition::Field(_) => DefinitionKind::Field,
            Definition::Variable(_) => DefinitionKind::Variable,
            Definition::Lifetime(_) => DefinitionKind::Lifetime,
            Definition::Generic(_) => DefinitionKind::Generic,
            Definition::Class(_) => DefinitionKind::Class,
            Definition::Interface(_) => DefinitionKind::Interface,
            Definition::Enum(_) => DefinitionKind::Enum,
            Definition::TypeAlias(_) => DefinitionKind::TypeAlias,
            Definition::Namespace(_) => DefinitionKind::Namespace,
            Definition::Parameter(_) => DefinitionKind::Parameter,
            Definition::Property(_) => DefinitionKind::Property,
        }
    }

    /// Returns true if this is a Rust-rich variant (10-variant set).
    pub fn is_rust_rich(&self) -> bool {
        matches!(
            self,
            Definition::Function(_)
                | Definition::Struct(_)
                | Definition::Trait(_)
                | Definition::Module(_)
                | Definition::Variant(_)
                | Definition::Macro(_)
                | Definition::Field(_)
                | Definition::Variable(_)
                | Definition::Lifetime(_)
                | Definition::Generic(_)
        )
    }

    /// Returns true if this is a multi-language lowered variant.
    pub fn is_multi_lang(&self) -> bool {
        matches!(
            self,
            Definition::Class(_)
                | Definition::Interface(_)
                | Definition::Enum(_)
                | Definition::TypeAlias(_)
                | Definition::Namespace(_)
                | Definition::Parameter(_)
                | Definition::Property(_)
        )
    }

    /// Returns a display-friendly name for the definition.
    pub fn display_name(&self) -> &'static str {
        match self {
            Definition::Function(_) => "function",
            Definition::Struct(_) => "struct",
            Definition::Trait(_) => "trait",
            Definition::Module(_) => "module",
            Definition::Variant(_) => "variant",
            Definition::Macro(_) => "macro",
            Definition::Field(_) => "field",
            Definition::Variable(_) => "variable",
            Definition::Lifetime(_) => "lifetime",
            Definition::Generic(_) => "generic",
            Definition::Class(_) => "class",
            Definition::Interface(_) => "interface",
            Definition::Enum(_) => "enum",
            Definition::TypeAlias(_) => "type alias",
            Definition::Namespace(_) => "namespace",
            Definition::Parameter(_) => "parameter",
            Definition::Property(_) => "property",
        }
    }
}

/// Category/kind of definition — used for filtering and dispatch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DefinitionKind {
    /// A function definition.
    Function,
    /// A struct / record type definition.
    Struct,
    /// A trait definition.
    Trait,
    /// A module definition.
    Module,
    /// An enum variant.
    Variant,
    /// A macro definition.
    Macro,
    /// A struct or class field.
    Field,
    /// A local or instance variable binding.
    Variable,
    /// A lifetime parameter.
    Lifetime,
    /// A generic type parameter.
    Generic,
    /// A class definition.
    Class,
    /// An interface definition.
    Interface,
    /// An enum type definition.
    Enum,
    /// A type alias (`type X = ...`).
    TypeAlias,
    /// A namespace or package grouping.
    Namespace,
    /// A function or method parameter.
    Parameter,
    /// An object property (e.g. in TypeScript/JavaScript).
    Property,
}

impl fmt::Display for DefinitionKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            DefinitionKind::Function => "function",
            DefinitionKind::Struct => "struct",
            DefinitionKind::Trait => "trait",
            DefinitionKind::Module => "module",
            DefinitionKind::Variant => "variant",
            DefinitionKind::Macro => "macro",
            DefinitionKind::Field => "field",
            DefinitionKind::Variable => "variable",
            DefinitionKind::Lifetime => "lifetime",
            DefinitionKind::Generic => "generic",
            DefinitionKind::Class => "class",
            DefinitionKind::Interface => "interface",
            DefinitionKind::Enum => "enum",
            DefinitionKind::TypeAlias => "type_alias",
            DefinitionKind::Namespace => "namespace",
            DefinitionKind::Parameter => "parameter",
            DefinitionKind::Property => "property",
        };
        write!(f, "{s}")
    }
}

/// Opaque identifier for a definition — stable across parses.
///
/// Internally backed by `(file_id, symbol_index)` allowing lookups
/// into the touring-index symbol store without holding source in memory.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DefinitionId {
    /// File identifier (stable across renames via touring-vfs FileId)
    pub file_id: u32,
    /// Symbol index within the file (matches touring-index ordering)
    pub symbol_index: u32,
    /// Optional: symbol name for display/debugging
    pub name: Option<String>,
}

impl DefinitionId {
    /// Construct a new DefinitionId from file and symbol indices.
    pub fn new(file_id: u32, symbol_index: u32) -> Self {
        Self {
            file_id,
            symbol_index,
            name: None,
        }
    }

    /// Construct with an associated name.
    pub fn with_name(mut self, name: String) -> Self {
        self.name = Some(name);
        self
    }
}

impl fmt::Display for DefinitionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(ref n) = self.name {
            write!(f, "{}:{}:{}", self.file_id, self.symbol_index, n)
        } else {
            write!(f, "{}:{}", self.file_id, self.symbol_index)
        }
    }
}

/// A file location range (start + end byte offset).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileRange {
    /// File identifier (touring-vfs FileId)
    pub file_id: u32,
    /// Start byte offset (0-indexed)
    pub start_byte: usize,
    /// End byte offset (0-indexed)
    pub end_byte: usize,
    /// Start line (1-indexed)
    pub start_line: u32,
    /// Start column (0-indexed byte offset within line)
    pub start_column: u32,
    /// End line (1-indexed)
    pub end_line: u32,
    /// End column (0-indexed byte offset within line)
    pub end_column: u32,
}

impl FileRange {
    /// Construct from tree-sitter node + file id.
    pub fn from_node(node: Node, file_id: u32, source: &str) -> Self {
        let start_byte = node.start_byte();
        let end_byte = node.end_byte();
        let _start_point = node.start_position();
        let _end_point = node.end_position();

        // Convert byte offset to line/column (naive: scan from start — cached by caller)
        let (start_line, start_column) = byte_offset_to_line_col(source, start_byte);
        let (end_line, end_column) = byte_offset_to_line_col(source, end_byte);

        Self {
            file_id,
            start_byte,
            end_byte,
            start_line,
            start_column,
            end_line,
            end_column,
        }
    }

    /// Create from raw byte offsets.
    pub fn from_offsets(file_id: u32, start_byte: usize, end_byte: usize, source: &str) -> Self {
        let (start_line, start_column) = byte_offset_to_line_col(source, start_byte);
        let (end_line, end_column) = byte_offset_to_line_col(source, end_byte);
        Self {
            file_id,
            start_byte,
            end_byte,
            start_line,
            start_column,
            end_line,
            end_column,
        }
    }
}

/// Convert a byte offset to (line, column) — 1-indexed line, 0-indexed column.
fn byte_offset_to_line_col(source: &str, byte_offset: usize) -> (u32, u32) {
    let mut line: u32 = 1;
    let mut col: u32 = 0;
    for (i, b) in source.bytes().enumerate() {
        if i == byte_offset {
            return (line, col);
        }
        if b == b'\n' {
            line += 1;
            col = 0;
        } else {
            col += 1;
        }
    }
    // If offset is at end of file
    if byte_offset == source.len() {
        return (line, col);
    }
    (line, col)
}

/// A usage of a definition (call site, reference, import, etc.).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Usage {
    /// Where the usage occurs
    pub range: FileRange,
    /// Kind of usage
    pub kind: UsageKind,
}

impl Usage {
    /// Create a new usage.
    pub fn new(range: FileRange, kind: UsageKind) -> Self {
        Self { range, kind }
    }
}

/// Kind of usage (for filtering/deduplication).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum UsageKind {
    /// Direct call site: `foo()`
    Call,
    /// Reference (read): `x`
    Read,
    /// Assignment (write): `x = ...`
    Write,
    /// Type reference: `Foo`
    Type,
    /// Import: `use crate::foo`
    Import,
    /// Export: `pub use crate::foo`
    Export,
    /// Doc reference: `foo` in doc comment
    Doc,
    /// Other/unknown
    Other,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_definition_id_display() {
        let id = DefinitionId::new(1, 42).with_name("foo".into());
        assert_eq!(id.to_string(), "1:42:foo");
    }

    #[test]
    fn test_definition_id_without_name() {
        let id = DefinitionId::new(5, 10);
        assert_eq!(id.to_string(), "5:10");
    }

    #[test]
    fn test_definition_kind_display() {
        assert_eq!(DefinitionKind::Function.to_string(), "function");
        assert_eq!(DefinitionKind::TypeAlias.to_string(), "type_alias");
    }

    #[test]
    fn test_byte_offset_to_line_col() {
        let source = "fn foo() {\n    bar();\n}";
        assert_eq!(byte_offset_to_line_col(source, 0), (1, 0)); // 'f'
        assert_eq!(byte_offset_to_line_col(source, 3), (1, 3)); // 'foo'
        assert_eq!(byte_offset_to_line_col(source, 11), (2, 0)); // '    '
        assert_eq!(byte_offset_to_line_col(source, 15), (2, 4)); // 'bar'
    }

    #[test]
    fn test_is_rust_rich() {
        assert!(Definition::Function(DefinitionId::new(0, 0)).is_rust_rich());
        assert!(!Definition::Class(DefinitionId::new(0, 0)).is_rust_rich());
        assert!(!Definition::Parameter(DefinitionId::new(0, 0)).is_rust_rich());
    }

    #[test]
    fn test_file_range_from_offsets() {
        let source = "fn foo() {}";
        let range = FileRange::from_offsets(1, 3, 6, source);
        assert_eq!(range.file_id, 1);
        assert_eq!(range.start_byte, 3);
        assert_eq!(range.end_byte, 6);
        assert_eq!(range.start_line, 1);
        assert_eq!(range.end_line, 1);
    }
}
