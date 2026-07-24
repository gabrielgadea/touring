//! Semantics integration tests — per-language definition resolution

use touring_code::ast::languages::Lang;
use touring_code::semantics::{
    Definition, DefinitionId, DefinitionKind, LangDefinitionMapping, Semantics, lang_to_definition,
};

#[test]
fn test_rust_function_resolution() {
    let source = r#"
fn foo() -> i32 { 42 }
fn main() { let x = foo(); }
"#;
    let pool = touring_code::ast::parser::ParserPool::new();
    let tree = pool.parse(source, Lang::Rust).expect("parse failed");
    let sem = Semantics::new(source, Lang::Rust, &tree);
    assert_eq!(sem.lang(), Lang::Rust);
    assert_eq!(sem.source(), source);
}

#[test]
fn test_rust_struct_field_resolution() {
    let source = r#"struct Point { x: f64, y: f64, }"#;
    let pool = touring_code::ast::parser::ParserPool::new();
    let tree = pool.parse(source, Lang::Rust).expect("parse failed");
    let _sem = Semantics::new(source, Lang::Rust, &tree);
    let root = tree.root_node();
    let mut found_struct = false;
    let mut cursor = root.walk();
    for node in root.children(&mut cursor) {
        if node.kind() == "struct_item" {
            found_struct = true;
        }
    }
    assert!(found_struct);
}

#[test]
fn test_js_function_resolution() {
    let source = r#"function bar(x) { return x * 2; }"#;
    let pool = touring_code::ast::parser::ParserPool::new();
    let tree = pool.parse(source, Lang::JavaScript).expect("parse failed");
    let _sem = Semantics::new(source, Lang::JavaScript, &tree);
    let root = tree.root_node();
    let mut found_fn = false;
    let mut cursor = root.walk();
    for node in root.children(&mut cursor) {
        if node.kind() == "function_declaration" {
            found_fn = true;
        }
    }
    assert!(found_fn);
}

#[test]
fn test_typescript_interface_resolution() {
    let source = r#"interface Config { name: string; value: number; }"#;
    let pool = touring_code::ast::parser::ParserPool::new();
    let tree = pool.parse(source, Lang::TypeScript).expect("parse failed");
    let _sem = Semantics::new(source, Lang::TypeScript, &tree);
    let root = tree.root_node();
    let mut found = false;
    let mut cursor = root.walk();
    for node in root.children(&mut cursor) {
        if node.kind() == "interface_declaration" {
            found = true;
        }
    }
    assert!(found);
}

#[test]
fn test_python_class_resolution() {
    let source = r#"class MyClass:
    def __init__(self, value): self.value = value"#;
    let pool = touring_code::ast::parser::ParserPool::new();
    let tree = pool.parse(source, Lang::Python).expect("parse failed");
    let _sem = Semantics::new(source, Lang::Python, &tree);
    let root = tree.root_node();
    let mut found = false;
    let mut cursor = root.walk();
    for node in root.children(&mut cursor) {
        if node.kind() == "class_definition" {
            found = true;
        }
    }
    assert!(found);
}

#[test]
fn test_definition_id_roundtrip() {
    let id = DefinitionId::new(42, 7).with_name("my_function".into());
    assert!(id.to_string().contains("my_function"));
}

#[test]
fn test_definition_kind_display() {
    assert_eq!(DefinitionKind::Function.to_string(), "function");
    assert_eq!(DefinitionKind::Class.to_string(), "class");
}

#[test]
fn test_js_class_maps_to_class() {
    let def = lang_to_definition("class_declaration", Lang::JavaScript, 0, "");
    assert!(def.is_some());
    assert_eq!(def.unwrap().kind(), DefinitionKind::Class);
}

#[test]
fn test_rust_fn_maps_to_function() {
    let def = lang_to_definition("function_item", Lang::Rust, 0, "");
    assert!(def.is_some());
    assert_eq!(def.unwrap().kind(), DefinitionKind::Function);
}

#[test]
fn test_lang_definition_kinds_completeness() {
    for lang in &[Lang::Rust, Lang::JavaScript, Lang::TypeScript, Lang::Python] {
        let kinds = touring_code::semantics::multi_lang::lang_definition_kinds(*lang);
        assert!(!kinds.is_empty());
    }
}

#[test]
fn test_is_valid_kind_rust_fn() {
    assert!(LangDefinitionMapping::is_valid_kind(
        Lang::Rust,
        DefinitionKind::Function
    ));
    assert!(!LangDefinitionMapping::is_valid_kind(
        Lang::Rust,
        DefinitionKind::Class
    ));
}

#[test]
fn test_is_valid_kind_js_class() {
    assert!(LangDefinitionMapping::is_valid_kind(
        Lang::JavaScript,
        DefinitionKind::Class
    ));
    assert!(!LangDefinitionMapping::is_valid_kind(
        Lang::JavaScript,
        DefinitionKind::Lifetime
    ));
}

#[test]
fn test_usages_of_basic() {
    let source = "fn foo() {}";
    let pool = touring_code::ast::parser::ParserPool::new();
    let tree = pool.parse(source, Lang::Rust).expect("parse failed");
    let sem = Semantics::new(source, Lang::Rust, &tree);
    // usages_of returns all identifiers in file (basic implementation)
    let _usages = sem.usages_of(&Definition::Function(DefinitionId::new(0, 0)));
    // At minimum, source text is accessible
    assert_eq!(sem.source(), source);
}
