//! multi_lang — language-specific Definition mapping via tree-sitter
//!
//! Maps tree-sitter node kinds to [`Definition`] variants per language.
//! Rust uses all 10 rich variants; other languages map to a subset.

use super::definition::{Definition, DefinitionId, DefinitionKind};

use crate::ast::languages::Lang;

/// Map a tree-sitter node kind + language to a Definition variant.
///
/// Returns `None` for nodes that are not definitions (references, literals, etc.).
/// Used by `super::source_to_def::try_def_from_node` as the multi-language fallback.
pub fn lang_to_definition(
    kind: &str,
    lang: Lang,
    file_id: u32,
    _source: &str,
) -> Option<Definition> {
    match lang {
        Lang::Rust => rust_kind_to_definition(kind, file_id),
        Lang::JavaScript | Lang::TypeScript => js_kind_to_definition(kind, file_id),
        Lang::Python => python_kind_to_definition(kind, file_id),
        #[cfg(feature = "more-languages")]
        Lang::Go => go_kind_to_definition(kind, file_id),
        _ => None,
    }
}

/// Rust-specific node kind → Definition mapping.
fn rust_kind_to_definition(kind: &str, file_id: u32) -> Option<Definition> {
    // Rust uses all 10 rich variants, mapped directly
    let id = DefinitionId::new(file_id, 0); // byte offset filled by caller
    match kind {
        // Class-like
        "struct_item" => Some(Definition::Struct(id)),
        "enum_item" => Some(Definition::Enum(id)),
        "trait_item" => Some(Definition::Trait(id)),
        "impl_item" => Some(Definition::Struct(id)), // impl resolves to the type it implements
        "type_alias_item" => Some(Definition::TypeAlias(id)),

        // Module
        "module_item" => Some(Definition::Module(id)),

        // Function
        "function_item" | "function_declaration" | "method_declaration" => {
            Some(Definition::Function(id))
        }

        // Macro
        "macro_invocation" | "macro_definition" | "macro_rule" => Some(Definition::Macro(id)),

        // Variable/constant
        "let_declaration" | "const_item" | "static_item" => Some(Definition::Variable(id)),

        // Field
        "field_definition" => Some(Definition::Field(id)),

        // Enum variant
        "enum_variant" => Some(Definition::Variant(id)),

        // Lifetime
        "lifetime" => Some(Definition::Lifetime(id)),

        // Generic
        "type_parameter" | "generic_type" | "generic_type_parameter" => {
            Some(Definition::Generic(id))
        }

        _ => None,
    }
}

/// JavaScript/TypeScript node kind → Definition mapping.
fn js_kind_to_definition(kind: &str, file_id: u32) -> Option<Definition> {
    let id = DefinitionId::new(file_id, 0);
    match kind {
        // Class-like
        "class_declaration" | "class" => Some(Definition::Class(id)),
        "interface_declaration" => Some(Definition::Interface(id)),
        "ts_type_alias_declaration" => Some(Definition::TypeAlias(id)),

        // Module
        "namespace_import" | "export_clause" => Some(Definition::Namespace(id)),

        // Function
        "function_declaration" | "function" | "method_definition" | "arrow_function" => {
            Some(Definition::Function(id))
        }

        // Variable
        "variable_declarator" | "variable_declaration" => Some(Definition::Variable(id)),

        // Property
        "property" | "property_assignment" => Some(Definition::Property(id)),

        // Parameter
        "formal_parameters" | "required_parameter" => Some(Definition::Parameter(id)),

        _ => None,
    }
}

/// Python node kind → Definition mapping.
fn python_kind_to_definition(kind: &str, file_id: u32) -> Option<Definition> {
    let id = DefinitionId::new(file_id, 0);
    match kind {
        // Class-like
        "class_definition" | "class" => Some(Definition::Class(id)),
        "module" => Some(Definition::Module(id)),

        // Function
        "function_definition" | "function" | "lambda" => Some(Definition::Function(id)),

        // Variable
        "assignment" | "identifier" => Some(Definition::Variable(id)),

        // Parameter
        "parameters" | "default_parameter" => Some(Definition::Parameter(id)),

        // Import
        "import_statement" | "import_from_statement" => Some(Definition::Namespace(id)),

        _ => None,
    }
}

/// Go node kind → Definition mapping.
#[cfg(feature = "more-languages")]
fn go_kind_to_definition(kind: &str, file_id: u32) -> Option<Definition> {
    let id = DefinitionId::new(file_id, 0);
    match kind {
        // Class-like
        "type_declaration" | "type_spec" => {
            // Check if it's a struct or interface via source... for now map to Enum
            Some(Definition::Enum(id))
        }
        "interface_type" => Some(Definition::Interface(id)),
        "struct_type" => Some(Definition::Struct(id)),

        // Module
        "package_clause" => Some(Definition::Module(id)),

        // Function
        "function_declaration" | "method_declaration" => Some(Definition::Function(id)),

        // Variable
        "var_spec" | "const_spec" => Some(Definition::Variable(id)),

        // Field/Parameter
        "field_declaration" | "parameter_declaration" => Some(Definition::Field(id)),

        // Import
        "import_declaration" => Some(Definition::Namespace(id)),

        _ => None,
    }
}

/// Language-specific Definition subset mapping.
///
/// Returns the list of [`DefinitionKind`] variants that are valid for a
/// given language. Used to validate queries and filter results.
pub fn lang_definition_kinds(lang: Lang) -> &'static [DefinitionKind] {
    match lang {
        Lang::Rust => &[
            DefinitionKind::Function,
            DefinitionKind::Struct,
            DefinitionKind::Trait,
            DefinitionKind::Module,
            DefinitionKind::Variant,
            DefinitionKind::Macro,
            DefinitionKind::Field,
            DefinitionKind::Variable,
            DefinitionKind::Lifetime,
            DefinitionKind::Generic,
        ],
        Lang::JavaScript | Lang::TypeScript => &[
            DefinitionKind::Function,
            DefinitionKind::Class,
            DefinitionKind::Interface,
            DefinitionKind::Namespace,
            DefinitionKind::Variable,
            DefinitionKind::Property,
            DefinitionKind::Parameter,
        ],
        Lang::Python => &[
            DefinitionKind::Function,
            DefinitionKind::Class,
            DefinitionKind::Module,
            DefinitionKind::Variable,
            DefinitionKind::Parameter,
        ],
        #[cfg(feature = "more-languages")]
        Lang::Go => &[
            DefinitionKind::Function,
            DefinitionKind::Struct,
            DefinitionKind::Interface,
            DefinitionKind::Module,
            DefinitionKind::Variable,
            DefinitionKind::Field,
        ],
        _ => &[],
    }
}

/// LangDefinitionMapping provides lookup table from DefinitionKind → bool per language.
pub struct LangDefinitionMapping;

impl LangDefinitionMapping {
    /// Check if a DefinitionKind is valid for a given language.
    pub fn is_valid_kind(lang: Lang, kind: DefinitionKind) -> bool {
        lang_definition_kinds(lang).contains(&kind)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rust_kind_to_definition() {
        assert!(rust_kind_to_definition("function_item", 0).is_some());
        assert!(rust_kind_to_definition("struct_item", 0).is_some());
        assert!(rust_kind_to_definition("identifier", 0).is_none());
    }

    #[test]
    fn test_js_kind_to_definition() {
        assert!(js_kind_to_definition("class_declaration", 0).is_some());
        assert!(js_kind_to_definition("function_declaration", 0).is_some());
        assert!(js_kind_to_definition("identifier", 0).is_none());
    }

    #[test]
    fn test_lang_definition_kinds_rust() {
        let kinds = lang_definition_kinds(Lang::Rust);
        assert!(kinds.contains(&DefinitionKind::Function));
        assert!(kinds.contains(&DefinitionKind::Lifetime));
        assert!(!kinds.contains(&DefinitionKind::Class));
    }

    #[test]
    fn test_lang_definition_kinds_js() {
        let kinds = lang_definition_kinds(Lang::JavaScript);
        assert!(kinds.contains(&DefinitionKind::Function));
        assert!(kinds.contains(&DefinitionKind::Class));
        assert!(!kinds.contains(&DefinitionKind::Lifetime));
    }

    #[test]
    fn test_is_valid_kind() {
        assert!(LangDefinitionMapping::is_valid_kind(
            Lang::Rust,
            DefinitionKind::Function
        ));
        assert!(!LangDefinitionMapping::is_valid_kind(
            Lang::Rust,
            DefinitionKind::Class
        ));
        assert!(LangDefinitionMapping::is_valid_kind(
            Lang::JavaScript,
            DefinitionKind::Class
        ));
    }
}
