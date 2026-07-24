//! Parametric multi-language tests powered by `rstest`.
//!
//! These tests exercise the core touring-ast pipeline (language detection,
//! symbol extraction, quality analysis) across every supported language in
//! one fixture table per concern. Replaces N copy-pasted test functions with
//! one body that runs once per fixture row.

use rstest::rstest;
use std::path::Path;
use touring_code::ast::{analyze_quality, extract_symbols, languages::Lang};

// ─── Lang::from_path — detects language from extensions ────────────────

#[rstest]
#[case("src/lib.rs", Some(Lang::Rust))]
#[case("module.py", Some(Lang::Python))]
#[case("component.ts", Some(Lang::TypeScript))]
#[case("component.tsx", Some(Lang::TypeScript))]
#[case("app.js", Some(Lang::JavaScript))]
#[case("app.jsx", Some(Lang::JavaScript))]
#[case("app.mjs", Some(Lang::JavaScript))]
#[case("run.sh", Some(Lang::Bash))]
#[case("page.html", Some(Lang::Html))]
#[case("style.css", Some(Lang::Css))]
#[case("README.md", Some(Lang::Markdown))]
#[case("data.json", Some(Lang::Json))]
#[case("Cargo.toml", Some(Lang::Toml))]
#[case("config.yaml", Some(Lang::Yaml))]
#[case("config.yml", Some(Lang::Yaml))]
#[case("unknown.xyz", None)]
#[case("no_extension", None)]
fn lang_from_path_detects_correctly(#[case] path_str: &str, #[case] expected: Option<Lang>) {
    assert_eq!(Lang::from_path(Path::new(path_str)), expected);
}

// ─── Lang::as_str / Display / FromStr round-trip ───────────────────────

#[rstest]
#[case(Lang::Python, "python")]
#[case(Lang::Rust, "rust")]
#[case(Lang::TypeScript, "typescript")]
#[case(Lang::JavaScript, "javascript")]
#[case(Lang::Bash, "bash")]
#[case(Lang::Html, "html")]
#[case(Lang::Css, "css")]
#[case(Lang::Markdown, "markdown")]
#[case(Lang::Json, "json")]
#[case(Lang::Toml, "toml")]
#[case(Lang::Yaml, "yaml")]
fn lang_as_str_matches_display(#[case] lang: Lang, #[case] expected: &str) {
    assert_eq!(lang.as_str(), expected);
    assert_eq!(lang.to_string(), expected);
}

#[rstest]
#[case("python", Lang::Python)]
#[case("py", Lang::Python)]
#[case("rust", Lang::Rust)]
#[case("rs", Lang::Rust)]
#[case("typescript", Lang::TypeScript)]
#[case("ts", Lang::TypeScript)]
#[case("javascript", Lang::JavaScript)]
#[case("js", Lang::JavaScript)]
#[case("RUST", Lang::Rust)] // case-insensitive
#[case("Bash", Lang::Bash)]
fn lang_from_str_accepts_aliases(#[case] input: &str, #[case] expected: Lang) {
    use std::str::FromStr;
    assert_eq!(Lang::from_str(input).expect("valid alias"), expected);
}

#[rstest]
#[case("cobol")]
#[case("")]
#[case("  ")]
fn lang_from_str_rejects_unknown(#[case] input: &str) {
    use std::str::FromStr;
    assert!(Lang::from_str(input).is_err());
}

// ─── Lang classification (is_code / is_markup / is_data) ──────────────

#[rstest]
#[case(Lang::Python, true, false, false)]
#[case(Lang::Rust, true, false, false)]
#[case(Lang::TypeScript, true, false, false)]
#[case(Lang::JavaScript, true, false, false)]
#[case(Lang::Bash, true, false, false)]
#[case(Lang::Html, false, true, false)]
#[case(Lang::Css, false, true, false)]
#[case(Lang::Markdown, false, true, false)]
#[case(Lang::Json, false, false, true)]
#[case(Lang::Toml, false, false, true)]
#[case(Lang::Yaml, false, false, true)]
fn lang_classification_partitions_languages(
    #[case] lang: Lang,
    #[case] is_code: bool,
    #[case] is_markup: bool,
    #[case] is_data: bool,
) {
    assert_eq!(lang.is_code(), is_code, "is_code for {lang}");
    assert_eq!(lang.is_markup(), is_markup, "is_markup for {lang}");
    assert_eq!(lang.is_data(), is_data, "is_data for {lang}");
    // Exactly one classification must be true for code-kind languages
    // (markup/data are disjoint; Lang itself is partitioned).
    let total = is_code as u8 + is_markup as u8 + is_data as u8;
    assert_eq!(total, 1, "{lang} must belong to exactly one category");
}

// ─── Symbol extraction across code languages ──────────────────────────

#[rstest]
#[case(Lang::Python, "def hello():\n    return 42\n", "hello")]
#[case(Lang::Rust, "fn hello() -> i32 { 42 }\n", "hello")]
#[case(Lang::JavaScript, "function hello() { return 42; }\n", "hello")]
#[case(Lang::TypeScript, "function hello(): number { return 42; }\n", "hello")]
fn symbol_extraction_finds_top_level_function(
    #[case] lang: Lang,
    #[case] source: &str,
    #[case] expected_name: &str,
) {
    let syms = extract_symbols(source, lang).expect("extract_symbols must succeed on valid source");
    let names: Vec<&str> = syms.iter().map(|s| s.name.as_str()).collect();
    assert!(
        names.contains(&expected_name),
        "expected to find symbol '{expected_name}' in {lang} source; got {names:?}"
    );
}

// ─── Quality analysis: todo!() detection is language-aware ────────────

#[rstest]
#[case(Lang::Rust, "fn pending() { todo!() }\n", true)]
#[case(Lang::Rust, "fn done() -> i32 { 42 }\n", false)]
fn quality_detects_todo_macro_in_rust(
    #[case] lang: Lang,
    #[case] source: &str,
    #[case] expect_anti_pattern: bool,
) {
    let report = analyze_quality(source, lang);
    let has_todo = report
        .antipatterns
        .iter()
        .any(|h| h.name.contains("todo") || h.message.to_lowercase().contains("todo"));
    assert_eq!(
        has_todo, expect_anti_pattern,
        "todo!() anti-pattern detection mismatch for {lang}: got {has_todo}, expected {expect_anti_pattern}"
    );
}
