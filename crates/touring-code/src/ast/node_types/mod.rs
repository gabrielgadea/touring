//! `node_types` — Knowledge base of AST node types per language.
//!
//! Provides a structured JSON export of node-type metadata (name,
//! importance score, and category) for each supported language.
//! Importance is ported from CodeWeaver semantics:
//!
//! | Category      | Score range | Examples                                    |
//! |---------------|-------------|--------------------------------------------|
//! | definition    | 0.9 – 1.0   | functions, structs, enums, traits          |
//! | declaration   | 0.6 – 0.8   | impl blocks, modules                        |
//! | statement     | 0.3 – 0.5   | let, expr statements                        |
//! | expression    | 0.1 – 0.2   | identifiers, literals                       |
//!
//! ## CLI interface
//!
//! - `touring ast node-types <lang>` — emits full JSON for a language
//! - `touring ast importance <file.rs> --threshold 0.5` — filters a file's AST

use serde::{Deserialize, Serialize};

/// Metadata for a single AST node type.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NodeTypeInfo {
    /// tree-sitter node name, e.g. `"function_item"`.
    pub name: String,
    /// CodeWeaver importance score in [0.0, 1.0].
    pub importance: f64,
    /// Coarse category: `"definition"`, `"declaration"`, `"statement"`, `"expression"`.
    pub category: String,
}

/// Full node-type inventory for one language.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LanguageNodeTypes {
    /// Name of the language this inventory describes (e.g. `"rust"`).
    pub language: String,
    /// Total number of node types in the inventory.
    pub node_count: usize,
    /// Per-node-type details, one entry per tree-sitter node kind.
    pub node_types: Vec<NodeTypeInfo>,
}

/// Return the complete node-type inventory for `lang`.
pub fn node_types_for_language(lang: &str) -> LanguageNodeTypes {
    match lang.to_lowercase().as_str() {
        "rust" => rust_node_types(),
        "python" | "py" => python_node_types(),
        "typescript" | "ts" => typescript_node_types(),
        "javascript" | "js" => javascript_node_types(),
        "bash" | "shell" | "sh" => bash_node_types(),
        "go" => go_node_types(),
        "java" => java_node_types(),
        _ => LanguageNodeTypes {
            language: lang.to_string(),
            node_count: 0,
            node_types: Vec::new(),
        },
    }
}

/// Return only the nodes whose importance is >= `threshold`.
pub fn importance_threshold(nodes: &[NodeTypeInfo], threshold: f64) -> Vec<&NodeTypeInfo> {
    nodes.iter().filter(|n| n.importance >= threshold).collect()
}

// ---------------------------------------------------------------------------
// Rust — most important nodes; matches syn/t tree-sitter-rust node names
// ---------------------------------------------------------------------------

fn rust_node_types() -> LanguageNodeTypes {
    use NodeTypeInfo as N;
    let types = vec![
        // definitions — score 0.9-1.0
        N {
            name: "function_item".into(),
            importance: 0.90,
            category: "definition".into(),
        },
        N {
            name: "struct_item".into(),
            importance: 0.95,
            category: "definition".into(),
        },
        N {
            name: "enum_item".into(),
            importance: 0.95,
            category: "definition".into(),
        },
        N {
            name: "trait_item".into(),
            importance: 0.90,
            category: "definition".into(),
        },
        N {
            name: "type_alias_item".into(),
            importance: 0.85,
            category: "definition".into(),
        },
        N {
            name: "macro_definition".into(),
            importance: 0.85,
            category: "definition".into(),
        },
        N {
            name: "const_item".into(),
            importance: 0.80,
            category: "definition".into(),
        },
        N {
            name: "static_item".into(),
            importance: 0.75,
            category: "definition".into(),
        },
        N {
            name: "union_item".into(),
            importance: 0.90,
            category: "definition".into(),
        },
        // declarations — score 0.6-0.8
        N {
            name: "impl_item".into(),
            importance: 0.70,
            category: "declaration".into(),
        },
        N {
            name: "module_item".into(),
            importance: 0.65,
            category: "declaration".into(),
        },
        N {
            name: "use_declaration".into(),
            importance: 0.60,
            category: "declaration".into(),
        },
        N {
            name: "extern_crate_declaration".into(),
            importance: 0.60,
            category: "declaration".into(),
        },
        N {
            name: "attribute_item".into(),
            importance: 0.50,
            category: "declaration".into(),
        },
        // statements — score 0.3-0.5
        N {
            name: "let_declaration".into(),
            importance: 0.40,
            category: "statement".into(),
        },
        N {
            name: "expression_statement".into(),
            importance: 0.30,
            category: "statement".into(),
        },
        N {
            name: "assignment".into(),
            importance: 0.35,
            category: "statement".into(),
        },
        N {
            name: "if_expression".into(),
            importance: 0.45,
            category: "statement".into(),
        },
        N {
            name: "match_expression".into(),
            importance: 0.50,
            category: "statement".into(),
        },
        N {
            name: "loop_expression".into(),
            importance: 0.40,
            category: "statement".into(),
        },
        N {
            name: "for_expression".into(),
            importance: 0.40,
            category: "statement".into(),
        },
        N {
            name: "while_expression".into(),
            importance: 0.35,
            category: "statement".into(),
        },
        N {
            name: "return_expression".into(),
            importance: 0.35,
            category: "statement".into(),
        },
        N {
            name: "block".into(),
            importance: 0.30,
            category: "statement".into(),
        },
        // expressions — score 0.1-0.2
        N {
            name: "identifier".into(),
            importance: 0.15,
            category: "expression".into(),
        },
        N {
            name: "literal".into(),
            importance: 0.15,
            category: "expression".into(),
        },
        N {
            name: "field_access".into(),
            importance: 0.20,
            category: "expression".into(),
        },
        N {
            name: "method_call_expression".into(),
            importance: 0.25,
            category: "expression".into(),
        },
        N {
            name: "call_expression".into(),
            importance: 0.25,
            category: "expression".into(),
        },
        N {
            name: "binary_expression".into(),
            importance: 0.15,
            category: "expression".into(),
        },
        N {
            name: "unary_expression".into(),
            importance: 0.12,
            category: "expression".into(),
        },
        N {
            name: "closure_expression".into(),
            importance: 0.20,
            category: "expression".into(),
        },
        N {
            name: "array_expression".into(),
            importance: 0.12,
            category: "expression".into(),
        },
        N {
            name: "tuple_expression".into(),
            importance: 0.12,
            category: "expression".into(),
        },
    ];
    let node_count = types.len();
    LanguageNodeTypes {
        language: "rust".into(),
        node_count,
        node_types: types,
    }
}

// ---------------------------------------------------------------------------
// Python — tree-sitter-python node names
// ---------------------------------------------------------------------------

fn python_node_types() -> LanguageNodeTypes {
    use NodeTypeInfo as N;
    let types = vec![
        N {
            name: "function_definition".into(),
            importance: 0.90,
            category: "definition".into(),
        },
        N {
            name: "class_definition".into(),
            importance: 0.95,
            category: "definition".into(),
        },
        N {
            name: "decorated_definition".into(),
            importance: 0.85,
            category: "definition".into(),
        },
        N {
            name: "type_alias".into(),
            importance: 0.85,
            category: "definition".into(),
        },
        N {
            name: "module".into(),
            importance: 0.65,
            category: "declaration".into(),
        },
        N {
            name: "import_from_statement".into(),
            importance: 0.60,
            category: "declaration".into(),
        },
        N {
            name: "import_statement".into(),
            importance: 0.60,
            category: "declaration".into(),
        },
        N {
            name: "assignment".into(),
            importance: 0.40,
            category: "statement".into(),
        },
        N {
            name: "augmented_assignment".into(),
            importance: 0.35,
            category: "statement".into(),
        },
        N {
            name: "expression_statement".into(),
            importance: 0.30,
            category: "statement".into(),
        },
        N {
            name: "if_statement".into(),
            importance: 0.45,
            category: "statement".into(),
        },
        N {
            name: "for_statement".into(),
            importance: 0.45,
            category: "statement".into(),
        },
        N {
            name: "while_statement".into(),
            importance: 0.35,
            category: "statement".into(),
        },
        N {
            name: "try_statement".into(),
            importance: 0.40,
            category: "statement".into(),
        },
        N {
            name: "return_statement".into(),
            importance: 0.35,
            category: "statement".into(),
        },
        N {
            name: "with_statement".into(),
            importance: 0.35,
            category: "statement".into(),
        },
        N {
            name: "identifier".into(),
            importance: 0.15,
            category: "expression".into(),
        },
        N {
            name: "string".into(),
            importance: 0.15,
            category: "expression".into(),
        },
        N {
            name: "integer".into(),
            importance: 0.15,
            category: "expression".into(),
        },
        N {
            name: "float".into(),
            importance: 0.15,
            category: "expression".into(),
        },
        N {
            name: "call".into(),
            importance: 0.25,
            category: "expression".into(),
        },
        N {
            name: "attribute".into(),
            importance: 0.20,
            category: "expression".into(),
        },
        N {
            name: "binary_operator".into(),
            importance: 0.15,
            category: "expression".into(),
        },
        N {
            name: "unary_operator".into(),
            importance: 0.12,
            category: "expression".into(),
        },
        N {
            name: "lambda".into(),
            importance: 0.20,
            category: "expression".into(),
        },
        N {
            name: "list".into(),
            importance: 0.12,
            category: "expression".into(),
        },
        N {
            name: "dict".into(),
            importance: 0.12,
            category: "expression".into(),
        },
    ];
    let node_count = types.len();
    LanguageNodeTypes {
        language: "python".into(),
        node_count,
        node_types: types,
    }
}

// ---------------------------------------------------------------------------
// TypeScript — tree-sitter-typescript (TS/TSX)
// ---------------------------------------------------------------------------

fn typescript_node_types() -> LanguageNodeTypes {
    use NodeTypeInfo as N;
    let types = vec![
        N {
            name: "function_declaration".into(),
            importance: 0.90,
            category: "definition".into(),
        },
        N {
            name: "class_declaration".into(),
            importance: 0.95,
            category: "definition".into(),
        },
        N {
            name: "interface_declaration".into(),
            importance: 0.90,
            category: "definition".into(),
        },
        N {
            name: "type_alias_declaration".into(),
            importance: 0.85,
            category: "definition".into(),
        },
        N {
            name: "enum_declaration".into(),
            importance: 0.90,
            category: "definition".into(),
        },
        N {
            name: "namespace_declaration".into(),
            importance: 0.70,
            category: "definition".into(),
        },
        N {
            name: "module_declaration".into(),
            importance: 0.65,
            category: "declaration".into(),
        },
        N {
            name: "import_clause".into(),
            importance: 0.60,
            category: "declaration".into(),
        },
        N {
            name: "export_clause".into(),
            importance: 0.60,
            category: "declaration".into(),
        },
        N {
            name: "lexical_declaration".into(),
            importance: 0.40,
            category: "statement".into(),
        },
        N {
            name: "variable_declaration".into(),
            importance: 0.40,
            category: "statement".into(),
        },
        N {
            name: "expression_statement".into(),
            importance: 0.30,
            category: "statement".into(),
        },
        N {
            name: "if_statement".into(),
            importance: 0.45,
            category: "statement".into(),
        },
        N {
            name: "for_statement".into(),
            importance: 0.45,
            category: "statement".into(),
        },
        N {
            name: "while_statement".into(),
            importance: 0.35,
            category: "statement".into(),
        },
        N {
            name: "switch_statement".into(),
            importance: 0.40,
            category: "statement".into(),
        },
        N {
            name: "return_statement".into(),
            importance: 0.35,
            category: "statement".into(),
        },
        N {
            name: "try_statement".into(),
            importance: 0.40,
            category: "statement".into(),
        },
        N {
            name: "identifier".into(),
            importance: 0.15,
            category: "expression".into(),
        },
        N {
            name: "string".into(),
            importance: 0.15,
            category: "expression".into(),
        },
        N {
            name: "number".into(),
            importance: 0.15,
            category: "expression".into(),
        },
        N {
            name: "call_expression".into(),
            importance: 0.25,
            category: "expression".into(),
        },
        N {
            name: "member_expression".into(),
            importance: 0.20,
            category: "expression".into(),
        },
        N {
            name: "binary_expression".into(),
            importance: 0.15,
            category: "expression".into(),
        },
        N {
            name: "unary_expression".into(),
            importance: 0.12,
            category: "expression".into(),
        },
        N {
            name: "arrow_function".into(),
            importance: 0.20,
            category: "expression".into(),
        },
        N {
            name: "object".into(),
            importance: 0.12,
            category: "expression".into(),
        },
        N {
            name: "array".into(),
            importance: 0.12,
            category: "expression".into(),
        },
    ];
    let node_count = types.len();
    LanguageNodeTypes {
        language: "typescript".into(),
        node_count,
        node_types: types,
    }
}

// ---------------------------------------------------------------------------
// JavaScript — tree-sitter-javascript (JS/JSX)
// ---------------------------------------------------------------------------

fn javascript_node_types() -> LanguageNodeTypes {
    use NodeTypeInfo as N;
    let types = vec![
        N {
            name: "function_declaration".into(),
            importance: 0.90,
            category: "definition".into(),
        },
        N {
            name: "class_declaration".into(),
            importance: 0.95,
            category: "definition".into(),
        },
        N {
            name: "variable_declaration".into(),
            importance: 0.40,
            category: "statement".into(),
        },
        N {
            name: "expression_statement".into(),
            importance: 0.30,
            category: "statement".into(),
        },
        N {
            name: "if_statement".into(),
            importance: 0.45,
            category: "statement".into(),
        },
        N {
            name: "for_statement".into(),
            importance: 0.45,
            category: "statement".into(),
        },
        N {
            name: "while_statement".into(),
            importance: 0.35,
            category: "statement".into(),
        },
        N {
            name: "switch_statement".into(),
            importance: 0.40,
            category: "statement".into(),
        },
        N {
            name: "return_statement".into(),
            importance: 0.35,
            category: "statement".into(),
        },
        N {
            name: "try_statement".into(),
            importance: 0.40,
            category: "statement".into(),
        },
        N {
            name: "identifier".into(),
            importance: 0.15,
            category: "expression".into(),
        },
        N {
            name: "string".into(),
            importance: 0.15,
            category: "expression".into(),
        },
        N {
            name: "number".into(),
            importance: 0.15,
            category: "expression".into(),
        },
        N {
            name: "call_expression".into(),
            importance: 0.25,
            category: "expression".into(),
        },
        N {
            name: "member_expression".into(),
            importance: 0.20,
            category: "expression".into(),
        },
        N {
            name: "binary_expression".into(),
            importance: 0.15,
            category: "expression".into(),
        },
        N {
            name: "unary_expression".into(),
            importance: 0.12,
            category: "expression".into(),
        },
        N {
            name: "arrow_function".into(),
            importance: 0.20,
            category: "expression".into(),
        },
        N {
            name: "function_expression".into(),
            importance: 0.20,
            category: "expression".into(),
        },
        N {
            name: "object".into(),
            importance: 0.12,
            category: "expression".into(),
        },
        N {
            name: "array".into(),
            importance: 0.12,
            category: "expression".into(),
        },
        N {
            name: "class".into(),
            importance: 0.95,
            category: "definition".into(),
        },
        N {
            name: "import_statement".into(),
            importance: 0.60,
            category: "declaration".into(),
        },
        N {
            name: "export_statement".into(),
            importance: 0.60,
            category: "declaration".into(),
        },
    ];
    let node_count = types.len();
    LanguageNodeTypes {
        language: "javascript".into(),
        node_count,
        node_types: types,
    }
}

// ---------------------------------------------------------------------------
// Bash — tree-sitter-bash
// ---------------------------------------------------------------------------

fn bash_node_types() -> LanguageNodeTypes {
    use NodeTypeInfo as N;
    let types = vec![
        N {
            name: "function_definition".into(),
            importance: 0.90,
            category: "definition".into(),
        },
        N {
            name: "command".into(),
            importance: 0.50,
            category: "statement".into(),
        },
        N {
            name: "if_statement".into(),
            importance: 0.45,
            category: "statement".into(),
        },
        N {
            name: "case_statement".into(),
            importance: 0.45,
            category: "statement".into(),
        },
        N {
            name: "for_statement".into(),
            importance: 0.45,
            category: "statement".into(),
        },
        N {
            name: "while_statement".into(),
            importance: 0.40,
            category: "statement".into(),
        },
        N {
            name: "until_statement".into(),
            importance: 0.40,
            category: "statement".into(),
        },
        N {
            name: "variable_assignment".into(),
            importance: 0.50,
            category: "statement".into(),
        },
        N {
            name: "export_statement".into(),
            importance: 0.50,
            category: "statement".into(),
        },
        N {
            name: "return_statement".into(),
            importance: 0.30,
            category: "statement".into(),
        },
        N {
            name: "identifier".into(),
            importance: 0.15,
            category: "expression".into(),
        },
        N {
            name: "string".into(),
            importance: 0.15,
            category: "expression".into(),
        },
        N {
            name: "expandable_string".into(),
            importance: 0.15,
            category: "expression".into(),
        },
        N {
            name: "word".into(),
            importance: 0.10,
            category: "expression".into(),
        },
        N {
            name: "redirected_statement".into(),
            importance: 0.20,
            category: "statement".into(),
        },
        N {
            name: "pipeline".into(),
            importance: 0.20,
            category: "expression".into(),
        },
    ];
    let node_count = types.len();
    LanguageNodeTypes {
        language: "bash".into(),
        node_count,
        node_types: types,
    }
}

// ---------------------------------------------------------------------------
// Go — tree-sitter-go
// ---------------------------------------------------------------------------

fn go_node_types() -> LanguageNodeTypes {
    use NodeTypeInfo as N;
    let types = vec![
        N {
            name: "function_declaration".into(),
            importance: 0.90,
            category: "definition".into(),
        },
        N {
            name: "method_declaration".into(),
            importance: 0.90,
            category: "definition".into(),
        },
        N {
            name: "type_declaration".into(),
            importance: 0.90,
            category: "definition".into(),
        },
        N {
            name: "struct_type".into(),
            importance: 0.95,
            category: "definition".into(),
        },
        N {
            name: "interface_type".into(),
            importance: 0.90,
            category: "definition".into(),
        },
        N {
            name: "const_declaration".into(),
            importance: 0.80,
            category: "definition".into(),
        },
        N {
            name: "var_declaration".into(),
            importance: 0.40,
            category: "statement".into(),
        },
        N {
            name: "import_declaration".into(),
            importance: 0.60,
            category: "declaration".into(),
        },
        N {
            name: "type_specifier".into(),
            importance: 0.85,
            category: "definition".into(),
        },
        N {
            name: "assignment_statement".into(),
            importance: 0.35,
            category: "statement".into(),
        },
        N {
            name: "if_statement".into(),
            importance: 0.45,
            category: "statement".into(),
        },
        N {
            name: "for_statement".into(),
            importance: 0.45,
            category: "statement".into(),
        },
        N {
            name: "switch_statement".into(),
            importance: 0.40,
            category: "statement".into(),
        },
        N {
            name: "return_statement".into(),
            importance: 0.35,
            category: "statement".into(),
        },
        N {
            name: "defer_statement".into(),
            importance: 0.35,
            category: "statement".into(),
        },
        N {
            name: "go_statement".into(),
            importance: 0.40,
            category: "statement".into(),
        },
        N {
            name: "identifier".into(),
            importance: 0.15,
            category: "expression".into(),
        },
        N {
            name: "interpreted_string_literal".into(),
            importance: 0.15,
            category: "expression".into(),
        },
        N {
            name: "raw_string_literal".into(),
            importance: 0.15,
            category: "expression".into(),
        },
        N {
            name: "rune_literal".into(),
            importance: 0.15,
            category: "expression".into(),
        },
        N {
            name: "call_expression".into(),
            importance: 0.25,
            category: "expression".into(),
        },
        N {
            name: "selector_expression".into(),
            importance: 0.20,
            category: "expression".into(),
        },
        N {
            name: "index_expression".into(),
            importance: 0.20,
            category: "expression".into(),
        },
        N {
            name: "binary_expression".into(),
            importance: 0.15,
            category: "expression".into(),
        },
        N {
            name: "unary_expression".into(),
            importance: 0.12,
            category: "expression".into(),
        },
        N {
            name: "composite_literal".into(),
            importance: 0.15,
            category: "expression".into(),
        },
        N {
            name: "make_expression".into(),
            importance: 0.20,
            category: "expression".into(),
        },
        N {
            name: "append_expression".into(),
            importance: 0.20,
            category: "expression".into(),
        },
    ];
    let node_count = types.len();
    LanguageNodeTypes {
        language: "go".into(),
        node_count,
        node_types: types,
    }
}

// ---------------------------------------------------------------------------
// Java — tree-sitter-java
// ---------------------------------------------------------------------------

fn java_node_types() -> LanguageNodeTypes {
    use NodeTypeInfo as N;
    let types = vec![
        N {
            name: "method_declaration".into(),
            importance: 0.90,
            category: "definition".into(),
        },
        N {
            name: "class_declaration".into(),
            importance: 0.95,
            category: "definition".into(),
        },
        N {
            name: "interface_declaration".into(),
            importance: 0.90,
            category: "definition".into(),
        },
        N {
            name: "enum_declaration".into(),
            importance: 0.90,
            category: "definition".into(),
        },
        N {
            name: "record_declaration".into(),
            importance: 0.90,
            category: "definition".into(),
        },
        N {
            name: "annotation_type_declaration".into(),
            importance: 0.85,
            category: "definition".into(),
        },
        N {
            name: "field_declaration".into(),
            importance: 0.60,
            category: "definition".into(),
        },
        N {
            name: "import_declaration".into(),
            importance: 0.60,
            category: "declaration".into(),
        },
        N {
            name: "variable_declarator".into(),
            importance: 0.40,
            category: "statement".into(),
        },
        N {
            name: "local_variable_declaration".into(),
            importance: 0.40,
            category: "statement".into(),
        },
        N {
            name: "expression_statement".into(),
            importance: 0.30,
            category: "statement".into(),
        },
        N {
            name: "if_statement".into(),
            importance: 0.45,
            category: "statement".into(),
        },
        N {
            name: "for_statement".into(),
            importance: 0.45,
            category: "statement".into(),
        },
        N {
            name: "while_statement".into(),
            importance: 0.35,
            category: "statement".into(),
        },
        N {
            name: "do_statement".into(),
            importance: 0.35,
            category: "statement".into(),
        },
        N {
            name: "switch_statement".into(),
            importance: 0.40,
            category: "statement".into(),
        },
        N {
            name: "return_statement".into(),
            importance: 0.35,
            category: "statement".into(),
        },
        N {
            name: "try_statement".into(),
            importance: 0.40,
            category: "statement".into(),
        },
        N {
            name: "throw_statement".into(),
            importance: 0.35,
            category: "statement".into(),
        },
        N {
            name: "identifier".into(),
            importance: 0.15,
            category: "expression".into(),
        },
        N {
            name: "string_literal".into(),
            importance: 0.15,
            category: "expression".into(),
        },
        N {
            name: "decimal_integer_literal".into(),
            importance: 0.15,
            category: "expression".into(),
        },
        N {
            name: "decimal_floating_point_literal".into(),
            importance: 0.15,
            category: "expression".into(),
        },
        N {
            name: "method_invocation".into(),
            importance: 0.25,
            category: "expression".into(),
        },
        N {
            name: "field_access".into(),
            importance: 0.20,
            category: "expression".into(),
        },
        N {
            name: "binary_expression".into(),
            importance: 0.15,
            category: "expression".into(),
        },
        N {
            name: "unary_expression".into(),
            importance: 0.12,
            category: "expression".into(),
        },
        N {
            name: "array_creation_expression".into(),
            importance: 0.15,
            category: "expression".into(),
        },
        N {
            name: "lambda_expression".into(),
            importance: 0.20,
            category: "expression".into(),
        },
        N {
            name: "cast_expression".into(),
            importance: 0.15,
            category: "expression".into(),
        },
    ];
    let node_count = types.len();
    LanguageNodeTypes {
        language: "java".into(),
        node_count,
        node_types: types,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rust_node_count_is_34() {
        let result = rust_node_types();
        assert_eq!(
            result.node_count, 34,
            "rust should have exactly 34 node types, got {}",
            result.node_count
        );
        assert_eq!(result.language, "rust");
    }

    #[test]
    fn test_python_node_count_is_27() {
        let result = node_types_for_language("python");
        assert_eq!(
            result.node_count, 27,
            "python should have exactly 27 node types, got {}",
            result.node_count
        );
    }

    #[test]
    fn test_typescript_node_count_is_28() {
        let result = node_types_for_language("typescript");
        assert_eq!(
            result.node_count, 28,
            "typescript should have exactly 28 node types, got {}",
            result.node_count
        );
    }

    #[test]
    fn test_javascript_node_count_is_24() {
        let result = node_types_for_language("javascript");
        assert_eq!(
            result.node_count, 24,
            "javascript should have exactly 24 node types, got {}",
            result.node_count
        );
    }

    #[test]
    fn test_go_node_count_is_28() {
        let result = node_types_for_language("go");
        assert_eq!(
            result.node_count, 28,
            "go should have exactly 28 node types, got {}",
            result.node_count
        );
    }

    #[test]
    fn test_bash_node_count_is_16() {
        let result = node_types_for_language("bash");
        assert_eq!(
            result.node_count, 16,
            "bash should have exactly 16 node types, got {}",
            result.node_count
        );
    }

    #[test]
    fn test_java_node_count_is_30() {
        let result = node_types_for_language("java");
        assert_eq!(
            result.node_count, 30,
            "java should have exactly 30 node types, got {}",
            result.node_count
        );
    }

    #[test]
    fn test_importance_threshold_0_8_boundary() {
        let nodes = vec![
            NodeTypeInfo {
                name: "fn".into(),
                importance: 0.80,
                category: "definition".into(),
            },
            NodeTypeInfo {
                name: "struct".into(),
                importance: 0.95,
                category: "definition".into(),
            },
            NodeTypeInfo {
                name: "let".into(),
                importance: 0.79,
                category: "statement".into(),
            },
            NodeTypeInfo {
                name: "id".into(),
                importance: 0.10,
                category: "expression".into(),
            },
        ];
        // exactly at 0.8 should be included
        let filtered = importance_threshold(&nodes, 0.8);
        assert_eq!(filtered.len(), 2, "nodes at exactly 0.8 should be included");
        assert!(filtered.iter().any(|n| n.name == "fn"));
        assert!(filtered.iter().any(|n| n.name == "struct"));
    }

    #[test]
    fn test_all_languages_have_content() {
        for lang in &[
            "rust",
            "python",
            "typescript",
            "javascript",
            "bash",
            "go",
            "java",
        ] {
            let result = node_types_for_language(lang);
            assert!(
                result.node_count > 0,
                "language {} returned 0 node types",
                lang
            );
            assert_eq!(result.language, *lang);
        }
    }

    #[test]
    fn test_unknown_language_returns_empty() {
        let result = node_types_for_language("cobol");
        assert_eq!(result.node_count, 0);
        assert!(result.node_types.is_empty());
    }

    #[test]
    fn test_importance_threshold_filters_correctly() {
        let nodes = vec![
            NodeTypeInfo {
                name: "fn".into(),
                importance: 0.90,
                category: "definition".into(),
            },
            NodeTypeInfo {
                name: "struct".into(),
                importance: 0.95,
                category: "definition".into(),
            },
            NodeTypeInfo {
                name: "let".into(),
                importance: 0.40,
                category: "statement".into(),
            },
            NodeTypeInfo {
                name: "id".into(),
                importance: 0.15,
                category: "expression".into(),
            },
        ];
        let filtered = importance_threshold(&nodes, 0.5);
        assert_eq!(filtered.len(), 2);
        assert_eq!(filtered[0].name, "fn");
        assert_eq!(filtered[1].name, "struct");

        let high = importance_threshold(&nodes, 0.9);
        assert_eq!(high.len(), 2);
        assert!(high.iter().any(|n| n.name == "fn"));
        assert!(high.iter().any(|n| n.name == "struct"));

        let none = importance_threshold(&nodes, 1.0);
        assert!(none.is_empty());
    }

    #[test]
    fn test_threshold_boundary_cases() {
        let nodes = vec![NodeTypeInfo {
            name: "x".into(),
            importance: 0.5,
            category: "statement".into(),
        }];
        // exactly at threshold
        let r = importance_threshold(&nodes, 0.5);
        assert_eq!(r.len(), 1);
        // just below
        let r = importance_threshold(&nodes, 0.51);
        assert!(r.is_empty());
    }

    #[test]
    fn test_all_supported_languages() {
        let langs = [
            "rust",
            "python",
            "typescript",
            "javascript",
            "bash",
            "go",
            "java",
        ];
        for lang in langs {
            let result = node_types_for_language(lang);
            assert!(
                result.node_count >= 15,
                "{} has too few node types: {}",
                lang,
                result.node_count
            );
            // every entry must have a valid category
            for n in &result.node_types {
                assert!(
                    ["definition", "declaration", "statement", "expression"]
                        .contains(&n.category.as_str()),
                    "invalid category '{}' for node '{}' in {}",
                    n.category,
                    n.name,
                    lang
                );
            }
        }
    }
}
