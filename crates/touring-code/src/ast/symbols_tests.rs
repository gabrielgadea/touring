#![allow(clippy::indexing_slicing)] // test vecs are asserted non-empty before indexing
use super::*;

#[test]
fn test_extract_python() {
    let source = r#"
def hello():
    pass

class Foo:
    def bar(self):
        return 42
"#;
    let symbols = extract_symbols(source, Lang::Python).unwrap();

    assert!(!symbols.is_empty());
    assert!(
        symbols
            .iter()
            .any(|s| s.name == "hello" && s.kind == "function")
    );
    assert!(symbols.iter().any(|s| s.name == "Foo" && s.kind == "class"));
    // bar is inside Foo, so it's a method
    assert!(
        symbols
            .iter()
            .any(|s| s.name == "bar" && (s.kind == "method" || s.kind == "function"))
    );
}

#[test]
fn test_extract_rust() {
    let source = r#"
pub fn hello() {}

pub(crate) struct Foo {
    bar: i32,
}

impl Foo {
    pub fn new() -> Self {
        Self { bar: 0 }
    }
}
"#;
    let symbols = extract_symbols(source, Lang::Rust).unwrap();

    assert!(!symbols.is_empty());
    assert!(
        symbols
            .iter()
            .any(|s| s.name == "hello" && s.kind == "function")
    );
    assert!(
        symbols
            .iter()
            .any(|s| s.name == "Foo" && s.kind == "struct")
    );
}

#[test]
fn test_symbol_signature() {
    let source = "def my_function(x: int) -> str:\n    pass";
    let symbols = extract_symbols(source, Lang::Python).unwrap();

    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].name, "my_function");
    assert!(symbols[0].signature.contains("def"));
    assert!(symbols[0].signature.contains("my_function"));
}

#[test]
fn test_extract_symbols_with_pool() {
    use crate::ast::parser::ParserPool;

    let pool = ParserPool::new();
    let source = "def foo():\n    pass\n\ndef bar():\n    return 1";

    let symbols = extract_symbols_with_pool(source, Lang::Python, &pool).unwrap();
    assert_eq!(symbols.len(), 2);
    assert_eq!(symbols[0].name, "foo");
    assert_eq!(symbols[1].name, "bar");

    let symbols2 = extract_symbols_with_pool(source, Lang::Python, &pool).unwrap();
    assert_eq!(symbols2.len(), 2);
}

#[test]
fn test_extract_symbols_backward_compat() {
    let source = "fn hello() {}\nstruct World { x: i32 }";
    let symbols = extract_symbols(source, Lang::Rust).unwrap();
    assert!(symbols.iter().any(|s| s.name == "hello"));
    assert!(symbols.iter().any(|s| s.name == "World"));
}

#[test]
fn test_signature_with_type_hints() {
    let source = "def foo(x: int) -> str:\n    pass";
    let symbols = extract_symbols(source, Lang::Python).unwrap();
    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].signature, "def foo(x: int) -> str:");
}

#[test]
fn test_signature_with_nested_brackets() {
    let source = "def bar(d: dict[str, int]):\n    pass";
    let symbols = extract_symbols(source, Lang::Python).unwrap();
    assert_eq!(symbols.len(), 1);
    assert_eq!(symbols[0].signature, "def bar(d: dict[str, int]):");
}

#[test]
fn test_find_depth_zero_colon_simple() {
    assert_eq!(find_depth_zero_colon("def f():"), Some(7));
}

#[test]
fn test_find_depth_zero_colon_type_hints() {
    let text = "def f(x: int):";
    assert_eq!(find_depth_zero_colon(text), Some(13));
}

#[test]
fn test_find_depth_zero_colon_nested() {
    let text = "def f(d: dict[str, int]):";
    assert_eq!(find_depth_zero_colon(text), Some(24));
}

#[test]
fn test_find_depth_zero_colon_no_colon() {
    assert_eq!(find_depth_zero_colon("fn hello()"), None);
}

#[test]
fn test_extract_python_type_alias() {
    let source = "type Point = tuple[int, int]\n\ndef foo():\n    pass";
    let symbols = extract_symbols(source, Lang::Python).unwrap();
    assert!(
        symbols
            .iter()
            .any(|s| s.name == "Point" && s.kind == "type_alias"),
        "Expected Python type alias 'Point', got: {:?}",
        symbols
            .iter()
            .map(|s| (&s.name, &s.kind))
            .collect::<Vec<_>>()
    );
    assert!(
        symbols
            .iter()
            .any(|s| s.name == "foo" && (s.kind == "function" || s.kind == "async_function"))
    );
}

#[test]
fn test_module_binding_constant_vs_variable_classification() {
    // Module-level assignments are kept (REGRA #0) but classified precisely:
    // SCREAMING_SNAKE_CASE and dunder metadata → const; ordinary lowercase
    // bindings → variable. Refines the index without dropping any symbol.
    let source = "MAX_RETRIES = 3\n\
DATABASE_URL = \"postgres://localhost/db\"\n\
__version__ = \"1.0.0\"\n\
app = make_app()\n\
router = make_router()\n";
    let symbols = extract_symbols(source, Lang::Python).unwrap();
    let kind_of = |n: &str| {
        symbols
            .iter()
            .find(|s| s.name == n)
            .map(|s| s.kind.as_str().to_string())
    };
    assert_eq!(
        kind_of("MAX_RETRIES").as_deref(),
        Some("const"),
        "SCREAMING_SNAKE → const; got {:?}",
        symbols
            .iter()
            .map(|s| (&s.name, s.kind.as_str()))
            .collect::<Vec<_>>()
    );
    assert_eq!(
        kind_of("DATABASE_URL").as_deref(),
        Some("const"),
        "SCREAMING_SNAKE → const"
    );
    assert_eq!(
        kind_of("__version__").as_deref(),
        Some("const"),
        "dunder metadata → const"
    );
    assert_eq!(
        kind_of("app").as_deref(),
        Some("variable"),
        "lowercase → variable"
    );
    assert_eq!(
        kind_of("router").as_deref(),
        Some("variable"),
        "lowercase → variable"
    );
}

#[test]
fn test_extract_typescript_namespace() {
    let source = "namespace Utils {\n  export function helper() {}\n}\n\nfunction main() {}";
    let symbols = extract_symbols(source, Lang::TypeScript).unwrap();
    assert!(
        symbols
            .iter()
            .any(|s| s.name == "Utils" && s.kind == "namespace"),
        "Expected TypeScript namespace 'Utils', got: {:?}",
        symbols
            .iter()
            .map(|s| (&s.name, &s.kind))
            .collect::<Vec<_>>()
    );
    assert!(
        symbols
            .iter()
            .any(|s| s.name == "main" && s.kind == "function")
    );
}

// ── New enrichment tests ──────────────────────────────────────────────

#[test]
fn test_symbol_kind_serde_roundtrip() {
    let kind = SymbolKind::Function;
    let json = serde_json::to_string(&kind).unwrap();
    assert_eq!(json, "\"function\"");

    let deserialized: SymbolKind = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized, SymbolKind::Function);
}

#[test]
fn test_symbol_kind_str_comparison() {
    let kind = SymbolKind::Function;
    assert!(kind == "function");
    assert!(kind != "class");

    let kind2 = SymbolKind::Class;
    assert!(kind2 == "class");
}

#[test]
fn test_symbol_kind_display() {
    assert_eq!(format!("{}", SymbolKind::Function), "function");
    assert_eq!(format!("{}", SymbolKind::AsyncFunction), "async_function");
    assert_eq!(format!("{}", SymbolKind::Other("custom".into())), "custom");
}

#[test]
fn test_symbol_kind_predicates() {
    assert!(SymbolKind::Function.is_callable());
    assert!(SymbolKind::Method.is_callable());
    assert!(!SymbolKind::Class.is_callable());

    assert!(SymbolKind::Class.is_type_definition());
    assert!(SymbolKind::Struct.is_type_definition());
    assert!(!SymbolKind::Function.is_type_definition());

    assert!(SymbolKind::Class.is_container());
    assert!(SymbolKind::Module.is_container());
    assert!(!SymbolKind::Function.is_container());
}

#[test]
fn test_async_detection_python() {
    let source = "async def fetch():\n    await something()\n\ndef sync():\n    pass";
    let symbols = extract_symbols(source, Lang::Python).unwrap();

    let fetch = symbols.iter().find(|s| s.name == "fetch").unwrap();
    assert!(fetch.is_async, "fetch should be async");
    assert_eq!(fetch.kind, SymbolKind::AsyncFunction);

    let sync = symbols.iter().find(|s| s.name == "sync").unwrap();
    assert!(!sync.is_async, "sync should not be async");
}

#[test]
fn test_async_detection_rust() {
    let source = "pub async fn fetch() {}\nfn sync() {}";
    let symbols = extract_symbols(source, Lang::Rust).unwrap();

    let fetch = symbols.iter().find(|s| s.name == "fetch").unwrap();
    assert!(fetch.is_async);
    assert_eq!(fetch.kind, SymbolKind::AsyncFunction);
}

#[test]
fn test_python_docstring_extraction() {
    let source = r#"def greet(name):
    """Say hello to the user."""
    print(f"Hello, {name}")
"#;
    let symbols = extract_symbols(source, Lang::Python).unwrap();
    let greet = symbols.iter().find(|s| s.name == "greet").unwrap();
    assert_eq!(greet.docstring.as_deref(), Some("Say hello to the user."));
}

#[test]
fn test_python_decorator_extraction() {
    let source = r#"
@staticmethod
@cache
def helper():
    pass
"#;
    let symbols = extract_symbols(source, Lang::Python).unwrap();
    let helper = symbols.iter().find(|s| s.name == "helper").unwrap();
    assert!(
        !helper.decorators.is_empty(),
        "Should extract decorators, got: {:?}",
        helper.decorators
    );
}

#[test]
fn test_python_parent_detection() {
    let source = r#"
class MyClass:
    def method(self):
        pass
"#;
    let symbols = extract_symbols(source, Lang::Python).unwrap();
    let method = symbols.iter().find(|s| s.name == "method").unwrap();
    assert_eq!(method.parent_name.as_deref(), Some("MyClass"));
    assert!(method.kind == SymbolKind::Method || method.kind == "method");
}

#[test]
fn test_rust_visibility_levels() {
    let source = r#"
pub fn public_fn() {}
pub(crate) fn crate_fn() {}
fn private_fn() {}
"#;
    let symbols = extract_symbols(source, Lang::Rust).unwrap();

    let pub_fn = symbols.iter().find(|s| s.name == "public_fn").unwrap();
    assert_eq!(pub_fn.visibility, Some(Visibility::Public));
    assert!(pub_fn.is_public);

    let crate_fn = symbols.iter().find(|s| s.name == "crate_fn").unwrap();
    assert_eq!(crate_fn.visibility, Some(Visibility::Crate));

    let priv_fn = symbols.iter().find(|s| s.name == "private_fn").unwrap();
    assert_eq!(priv_fn.visibility, Some(Visibility::Private));
    assert!(!priv_fn.is_public);
}

#[test]
fn test_symbol_json_backward_compat() {
    // Verify that serialized JSON has `kind` as a plain string
    let sym = Symbol::new(
        "test",
        SymbolKind::Function,
        1,
        1,
        0,
        0,
        10,
        "def test():",
        true,
    );
    let json = serde_json::to_value(&sym).unwrap();
    assert_eq!(json["kind"], "function");
    assert_eq!(json["name"], "test");
    assert_eq!(json["is_public"], true);

    // Deserialize back
    let deserialized: Symbol = serde_json::from_value(json).unwrap();
    assert_eq!(deserialized.kind, SymbolKind::Function);
    assert_eq!(deserialized.name, "test");
}

#[test]
fn test_symbol_builder_pattern() {
    let sym = Symbol::new("foo", "function", 1, 5, 0, 0, 50, "def foo():", true)
        .with_parent("MyClass")
        .with_docstring("Does something useful")
        .with_decorators(vec!["staticmethod".into()])
        .with_complexity(3)
        .with_async(true)
        .with_visibility(Visibility::Public);

    assert_eq!(sym.parent_name.as_deref(), Some("MyClass"));
    assert_eq!(sym.docstring.as_deref(), Some("Does something useful"));
    assert_eq!(sym.decorators, vec!["staticmethod"]);
    assert_eq!(sym.complexity, Some(3));
    assert!(sym.is_async);
    assert_eq!(sym.visibility, Some(Visibility::Public));
}

#[test]
fn test_line_count() {
    let sym = Symbol::new("foo", "function", 10, 25, 0, 0, 100, "def foo():", true);
    assert_eq!(sym.line_count(), 16);
}

#[test]
fn test_rust_doc_comment_extraction() {
    let source = r#"
/// Process incoming data
/// and return the result.
pub fn process() {}
"#;
    let symbols = extract_symbols(source, Lang::Rust).unwrap();
    let process = symbols.iter().find(|s| s.name == "process").unwrap();
    assert!(
        process.docstring.is_some(),
        "Should extract /// doc comment, got: {:?}",
        process.docstring
    );
}

// ── FromStr tests ────────────────────────────────────────────────────

#[test]
fn test_symbol_kind_from_str_known_variants() {
    assert_eq!(
        "function".parse::<SymbolKind>().unwrap(),
        SymbolKind::Function
    );
    assert_eq!(
        "async_function".parse::<SymbolKind>().unwrap(),
        SymbolKind::AsyncFunction
    );
    assert_eq!("method".parse::<SymbolKind>().unwrap(), SymbolKind::Method);
    assert_eq!("class".parse::<SymbolKind>().unwrap(), SymbolKind::Class);
    assert_eq!("struct".parse::<SymbolKind>().unwrap(), SymbolKind::Struct);
    assert_eq!("enum".parse::<SymbolKind>().unwrap(), SymbolKind::Enum);
    assert_eq!("trait".parse::<SymbolKind>().unwrap(), SymbolKind::Trait);
    assert_eq!("impl".parse::<SymbolKind>().unwrap(), SymbolKind::Impl);
    assert_eq!(
        "interface".parse::<SymbolKind>().unwrap(),
        SymbolKind::Interface
    );
    assert_eq!(
        "type_alias".parse::<SymbolKind>().unwrap(),
        SymbolKind::TypeAlias
    );
    assert_eq!(
        "namespace".parse::<SymbolKind>().unwrap(),
        SymbolKind::Namespace
    );
    assert_eq!("const".parse::<SymbolKind>().unwrap(), SymbolKind::Constant);
    assert_eq!("static".parse::<SymbolKind>().unwrap(), SymbolKind::Static);
    assert_eq!(
        "variable".parse::<SymbolKind>().unwrap(),
        SymbolKind::Variable
    );
    assert_eq!("module".parse::<SymbolKind>().unwrap(), SymbolKind::Module);
    assert_eq!("macro".parse::<SymbolKind>().unwrap(), SymbolKind::Macro);
    assert_eq!(
        "generator".parse::<SymbolKind>().unwrap(),
        SymbolKind::Generator
    );
}

#[test]
fn test_symbol_kind_from_str_unknown_becomes_other() {
    let kind: SymbolKind = "some_exotic_kind".parse().unwrap();
    assert_eq!(kind, SymbolKind::Other("some_exotic_kind".into()));
    assert_eq!(kind.as_str(), "some_exotic_kind");
}

#[test]
fn test_symbol_kind_from_str_roundtrip() {
    let variants = vec![
        SymbolKind::Function,
        SymbolKind::AsyncFunction,
        SymbolKind::Method,
        SymbolKind::Class,
        SymbolKind::Struct,
        SymbolKind::Enum,
        SymbolKind::Trait,
        SymbolKind::Impl,
        SymbolKind::Interface,
        SymbolKind::TypeAlias,
        SymbolKind::Namespace,
        SymbolKind::Constant,
        SymbolKind::Static,
        SymbolKind::Variable,
        SymbolKind::Module,
        SymbolKind::Macro,
        SymbolKind::Generator,
    ];

    for variant in variants {
        let s = variant.as_str();
        let parsed: SymbolKind = s.parse().unwrap();
        assert_eq!(parsed, variant, "Roundtrip failed for {s}");
    }
}

// ── Batch extraction tests ───────────────────────────────────────────

#[test]
fn test_extract_symbols_batch_multiple_files() {
    let files = vec![
        (
            "hello.py".to_string(),
            "def hello():\n    pass\n\ndef world():\n    pass".to_string(),
        ),
        (
            "lib.rs".to_string(),
            "pub fn add(a: i32, b: i32) -> i32 { a + b }".to_string(),
        ),
    ];

    let results = extract_symbols_batch(&files);
    assert_eq!(results.len(), 2);

    let py_result = results.iter().find(|(p, _)| p == "hello.py").unwrap();
    let syms = py_result.1.as_ref().unwrap();
    assert_eq!(syms.len(), 2);
    assert!(syms.iter().any(|s| s.name == "hello"));
    assert!(syms.iter().any(|s| s.name == "world"));

    let rs_result = results.iter().find(|(p, _)| p == "lib.rs").unwrap();
    let syms = rs_result.1.as_ref().unwrap();
    assert!(syms.iter().any(|s| s.name == "add"));
}

#[test]
fn test_extract_symbols_batch_unsupported_language() {
    let files = vec![("unknown.xyz".to_string(), "some content".to_string())];

    let results = extract_symbols_batch(&files);
    assert_eq!(results.len(), 1);
    let (path, result) = &results[0];
    assert_eq!(path, "unknown.xyz");
    assert!(result.is_err());
    let err = result.as_ref().unwrap_err();
    assert!(
        matches!(err, AstError::UnsupportedLanguage(p) if p == "unknown.xyz"),
        "Expected UnsupportedLanguage, got: {err:?}"
    );
}

#[test]
fn test_extract_symbols_batch_empty_input() {
    let files: Vec<(String, String)> = vec![];
    let results = extract_symbols_batch(&files);
    assert!(results.is_empty());
}

#[test]
fn test_extract_symbols_batch_mixed_success_and_failure() {
    let files = vec![
        ("good.py".to_string(), "def ok():\n    pass".to_string()),
        ("bad.xyz".to_string(), "unrecognized".to_string()),
        ("also_good.rs".to_string(), "fn main() {}".to_string()),
    ];

    let results = extract_symbols_batch(&files);
    assert_eq!(results.len(), 3);

    let good_py = results.iter().find(|(p, _)| p == "good.py").unwrap();
    assert!(good_py.1.is_ok());

    let bad = results.iter().find(|(p, _)| p == "bad.xyz").unwrap();
    assert!(bad.1.is_err());

    let good_rs = results.iter().find(|(p, _)| p == "also_good.rs").unwrap();
    assert!(good_rs.1.is_ok());
}

// ── Filtering utility tests ──────────────────────────────────────────

#[test]
fn test_filter_by_kind_python() {
    let source = "def hello():\n    pass\n\nclass Foo:\n    def method(self):\n        pass\n\ndef world():\n    pass\n";
    let symbols = extract_symbols(source, Lang::Python).unwrap();
    let functions = filter_by_kind(&symbols, SymbolKind::Function);
    assert!(
        functions.iter().all(|s| s.kind == SymbolKind::Function),
        "All filtered symbols should be functions"
    );

    let classes = filter_by_kind(&symbols, SymbolKind::Class);
    assert_eq!(classes.len(), 1);
    assert_eq!(classes[0].name, "Foo");
}

#[test]
fn test_filter_by_kind_rust() {
    let source = "pub fn alpha() {}\npub fn beta() {}\npub struct Gamma {}\n";
    let symbols = extract_symbols(source, Lang::Rust).unwrap();
    let functions = filter_by_kind(&symbols, SymbolKind::Function);
    assert_eq!(functions.len(), 2);
    assert!(functions.iter().any(|s| s.name == "alpha"));
    assert!(functions.iter().any(|s| s.name == "beta"));

    let structs = filter_by_kind(&symbols, SymbolKind::Struct);
    assert_eq!(structs.len(), 1);
    assert_eq!(structs[0].name, "Gamma");
}

#[test]
fn test_filter_by_kind_empty_result() {
    let source = "def hello():\n    pass";
    let symbols = extract_symbols(source, Lang::Python).unwrap();
    let structs = filter_by_kind(&symbols, SymbolKind::Struct);
    assert!(structs.is_empty());
}

#[test]
fn test_filter_by_complexity() {
    let sym_low =
        Symbol::new("low", "function", 1, 3, 0, 0, 30, "def low():", true).with_complexity(2);
    let sym_high =
        Symbol::new("high", "function", 5, 20, 0, 40, 200, "def high():", true).with_complexity(15);
    let sym_none = Symbol::new("none", "function", 22, 25, 0, 210, 280, "def none():", true);

    let symbols = vec![sym_low, sym_high, sym_none];

    let complex = filter_by_complexity(&symbols, 10);
    assert_eq!(complex.len(), 1);
    assert_eq!(complex[0].name, "high");

    let all_with_cc = filter_by_complexity(&symbols, 1);
    assert_eq!(all_with_cc.len(), 2);

    let none = filter_by_complexity(&symbols, 100);
    assert!(none.is_empty());
}

#[test]
fn test_filter_by_complexity_excludes_none() {
    let sym = Symbol::new("no_cc", "function", 1, 1, 0, 0, 10, "def no_cc():", true);
    let symbols = vec![sym];

    let result = filter_by_complexity(&symbols, 1);
    assert!(
        result.is_empty(),
        "Symbols without complexity should be excluded"
    );
}

#[test]
fn test_find_by_name() {
    let source = "def alpha():\n    pass\n\ndef beta():\n    pass\n\ndef gamma():\n    pass\n";
    let symbols = extract_symbols(source, Lang::Python).unwrap();

    let found = find_by_name(&symbols, "beta");
    assert!(found.is_some());
    assert_eq!(found.unwrap().name, "beta");

    let not_found = find_by_name(&symbols, "nonexistent");
    assert!(not_found.is_none());
}

#[test]
fn test_find_by_name_rust() {
    let source = "pub fn process() {}\nfn helper() {}";
    let symbols = extract_symbols(source, Lang::Rust).unwrap();

    assert!(find_by_name(&symbols, "process").is_some());
    assert!(find_by_name(&symbols, "helper").is_some());
    assert!(find_by_name(&symbols, "missing").is_none());
}

#[test]
fn test_find_by_name_empty_slice() {
    let symbols: Vec<Symbol> = vec![];
    assert!(find_by_name(&symbols, "anything").is_none());
}

// ── Integration: batch + filter ──────────────────────────────────────

#[test]
fn test_filter_integration_python_methods_vs_functions() {
    let source = "def standalone():\n    pass\n\nclass MyClass:\n    def method_a(self):\n        pass\n\n    def method_b(self):\n        pass\n\ndef another():\n    pass\n";
    let symbols = extract_symbols(source, Lang::Python).unwrap();

    let methods = filter_by_kind(&symbols, SymbolKind::Method);
    assert_eq!(
        methods.len(),
        2,
        "Expected 2 methods, got: {:?}",
        methods.iter().map(|s| &s.name).collect::<Vec<_>>()
    );

    let functions = filter_by_kind(&symbols, SymbolKind::Function);
    assert_eq!(
        functions.len(),
        2,
        "Expected 2 standalone functions, got: {:?}",
        functions.iter().map(|s| &s.name).collect::<Vec<_>>()
    );
}

#[test]
fn test_batch_then_filter() {
    let files = vec![
        (
            "main.py".to_string(),
            "def foo():\n    pass\n\nclass Bar:\n    def baz(self):\n        pass".to_string(),
        ),
        (
            "lib.rs".to_string(),
            "pub fn add() {}\npub struct Config {}".to_string(),
        ),
    ];

    let results = extract_symbols_batch(&files);

    let all_symbols: Vec<&Symbol> = results
        .iter()
        .filter_map(|(_, r)| r.as_ref().ok())
        .flat_map(|syms| syms.iter())
        .collect();

    assert!(all_symbols.len() >= 4, "Expected at least 4 symbols total");
    assert!(all_symbols.iter().any(|s| s.name == "foo"));
    assert!(all_symbols.iter().any(|s| s.name == "add"));
}

#[test]
fn symbol_kind_iter_works() {
    use strum::IntoEnumIterator;
    let kinds: Vec<_> = SymbolKind::iter().collect();
    // All 17 named variants (Function..Generator) + Other("") from EnumIter
    assert!(
        kinds.len() >= 10,
        "expected at least 10 SymbolKind variants, got {}",
        kinds.len()
    );
    // fmt::Display must still work correctly (manual impl preserved)
    assert_eq!(SymbolKind::Function.to_string(), "function");
    assert_eq!(SymbolKind::Constant.to_string(), "const");
    assert_eq!(SymbolKind::AsyncFunction.to_string(), "async_function");
    // iter() covers named variants — ensure Struct and Enum are present
    assert!(kinds.contains(&SymbolKind::Struct));
    assert!(kinds.contains(&SymbolKind::Enum));
}
