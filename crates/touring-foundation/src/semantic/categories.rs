//! SemanticClass — 22 categories for classifying code symbols.

use serde::{Deserialize, Serialize};
use strum::{EnumIter, IntoStaticStr};

/// 22 semantic classification categories for code symbols.
///
/// These categories cover the full spectrum of code constructs across
/// all supported languages (Rust, Python, TypeScript, Go, Java, etc.)
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter, IntoStaticStr,
)]
#[serde(rename_all = "PascalCase")]
pub enum SemanticClass {
    /// Function definition (fn, def, function)
    FunctionDef,

    /// Struct/class definition
    StructDef,

    /// Enum definition
    EnumDef,

    /// Trait definition (Rust) / interface (other languages)
    TraitDef,

    /// Impl block (Rust) / class body (other languages)
    ImplBlock,

    /// Type alias / type definition
    TypeDef,

    /// Module / namespace / package
    Module,

    /// Use statement / import / require
    UseStatement,

    /// Constant definition
    ConstDef,

    /// Static variable
    StaticDef,

    /// Function parameter
    FnParam,

    /// Struct field / class property
    StructField,

    /// Enum variant
    EnumVariant,

    /// Attribute / annotation / decorator
    Attribute,

    /// Documentation comment (docstring)
    DocComment,

    /// Macro definition
    MacroDef,

    /// Closure / lambda / anonymous function
    Closure,

    /// Closure parameter
    ClosureParam,

    /// Type annotation (type hints in Python/TypeScript)
    TypeAnnotation,

    /// Generic parameter (e.g., `<T>` in Rust)
    GenericParam,

    /// Where clause (Rust)
    WhereClause,

    /// Import statement (Python `import X`, `from X import Y`)
    ImportStatement,

    /// Fallback when no classification rule matches
    Unclassified,
}

impl SemanticClass {
    /// Returns a human-readable description of the category.
    pub fn description(&self) -> &'static str {
        match self {
            SemanticClass::FunctionDef => "Function definition",
            SemanticClass::StructDef => "Struct or class definition",
            SemanticClass::EnumDef => "Enum definition",
            SemanticClass::TraitDef => "Trait or interface definition",
            SemanticClass::ImplBlock => "Implementation block",
            SemanticClass::TypeDef => "Type alias or type definition",
            SemanticClass::Module => "Module or namespace",
            SemanticClass::UseStatement => "Use or import statement",
            SemanticClass::ConstDef => "Constant definition",
            SemanticClass::StaticDef => "Static variable",
            SemanticClass::FnParam => "Function parameter",
            SemanticClass::StructField => "Struct field or class property",
            SemanticClass::EnumVariant => "Enum variant",
            SemanticClass::Attribute => "Attribute, annotation, or decorator",
            SemanticClass::DocComment => "Documentation comment",
            SemanticClass::MacroDef => "Macro definition",
            SemanticClass::Closure => "Closure or lambda",
            SemanticClass::ClosureParam => "Closure parameter",
            SemanticClass::TypeAnnotation => "Type annotation",
            SemanticClass::GenericParam => "Generic parameter",
            SemanticClass::WhereClause => "Where clause",
            SemanticClass::ImportStatement => "Import statement",
            SemanticClass::Unclassified => "Unclassified",
        }
    }
}

impl std::fmt::Display for SemanticClass {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s: &'static str = self.into();
        write!(f, "{s}")
    }
}
