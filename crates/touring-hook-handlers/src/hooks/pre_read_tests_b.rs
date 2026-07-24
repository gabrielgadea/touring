use super::*;

#[test]
fn test_collect_ts_js_exports_empty_source() {
    let (named, has_default) = collect_ts_js_exports("");
    assert!(named.is_empty());
    assert!(!has_default);
}

#[test]
fn test_collect_ts_js_exports_named_only() {
    let src = "export function foo() {}\nexport const bar = 1;\n";
    let (named, has_default) = collect_ts_js_exports(src);
    assert_eq!(named, vec!["foo", "bar"]);
    assert!(!has_default);
}

#[test]
fn test_collect_ts_js_exports_default_only() {
    let src = "export default class MyClass {}\n";
    let (named, has_default) = collect_ts_js_exports(src);
    assert!(named.is_empty());
    assert!(has_default);
}

#[test]
fn test_collect_ts_js_exports_skips_reexport_all() {
    let src = "export * from './other';\nexport function used() {}\n";
    let (named, has_default) = collect_ts_js_exports(src);
    assert_eq!(named, vec!["used"]);
    assert!(!has_default);
}

#[test]
fn test_format_exports_signal_none_when_empty() {
    let result = format_exports_signal(vec![], false);
    assert!(result.is_none());
}

#[test]
fn test_format_exports_signal_named() {
    let named = vec!["Foo".to_string(), "Bar".to_string()];
    let (weight, text) = format_exports_signal(named, false).expect("should produce signal");
    assert!((weight - 1.6_f32).abs() < f32::EPSILON);
    assert!(text.contains("exports[2]"));
    assert!(text.contains("Foo"));
    assert!(text.contains("Bar"));
}

#[test]
fn test_format_exports_signal_overflow_suffix() {
    let named: Vec<String> = (0..6).map(|i| format!("Sym{i}")).collect();
    let (_, text) = format_exports_signal(named, false).expect("should produce signal");
    assert!(
        text.contains("+2"),
        "expected +2 overflow suffix in: {text}"
    );
}

#[test]
fn test_format_exports_signal_includes_default() {
    let (_, text) = format_exports_signal(vec![], true).expect("should produce signal");
    assert!(text.contains("default"));
    assert!(text.contains("exports[1]"));
}

#[test]
fn test_ts_js_exports_signal_roundtrip() {
    let src = "export function Alpha() {}\nexport default Beta;\n";
    let (weight, text) = ts_js_exports_signal(src).expect("should produce signal");
    assert!((weight - 1.6_f32).abs() < f32::EPSILON);
    assert!(text.contains("Alpha"));
    assert!(text.contains("default"));
}

// ── detect_scope_lang ────────────────────────────────────────────

#[test]
fn test_detect_scope_lang_rust() {
    let lang = detect_scope_lang("src/lib.rs");
    assert!(matches!(lang, Some(touring_code::ast::Lang::Rust)));
}

#[test]
fn test_detect_scope_lang_python() {
    let lang = detect_scope_lang("scripts/foo.py");
    assert!(matches!(lang, Some(touring_code::ast::Lang::Python)));
}

#[test]
fn test_detect_scope_lang_unsupported_returns_none() {
    assert!(detect_scope_lang("app.ts").is_none());
    assert!(detect_scope_lang("main.js").is_none());
    assert!(detect_scope_lang("README.md").is_none());
    assert!(detect_scope_lang("Makefile").is_none());
}

// ── classify_export_line ─────────────────────────────────────────

#[test]
fn test_classify_export_line_non_export_returns_none() {
    assert!(classify_export_line("const x = 1;").is_none());
    assert!(classify_export_line("  import foo from 'bar';").is_none());
    assert!(classify_export_line("").is_none());
}

#[test]
fn test_classify_export_line_reexport_all_skipped() {
    assert!(classify_export_line("export * from './foo';").is_none());
    assert!(classify_export_line("export * as ns from './mod';").is_none());
}

#[test]
fn test_classify_export_line_comment_skipped() {
    assert!(classify_export_line("export // not real").is_none());
}

#[test]
fn test_classify_export_line_default_export() {
    let result = classify_export_line("export default MyComponent;");
    assert_eq!(result, Some(None));
}

#[test]
fn test_classify_export_line_named_function() {
    let result = classify_export_line("export function doThing() {}");
    assert!(matches!(result, Some(Some(name)) if name == "doThing"));
}

#[test]
fn test_classify_export_line_named_const() {
    let result = classify_export_line("export const MAX_SIZE = 100;");
    assert!(matches!(result, Some(Some(name)) if name == "MAX_SIZE"));
}

#[test]
fn test_classify_export_line_named_class() {
    let result = classify_export_line("export class Foo {}");
    assert!(matches!(result, Some(Some(name)) if name == "Foo"));
}

// ── remaining_budget ──────────────────────────────────────────────

#[test]
fn test_remaining_budget_none_context_no_separator() {
    // When context is None there is no separator cost.
    assert_eq!(remaining_budget(&None, 100), 100);
}

#[test]
fn test_remaining_budget_some_context_deducts_separator() {
    // 10 chars used + 3 separator = 13 deducted from 100.
    let ctx = Some("0123456789".to_string());
    assert_eq!(remaining_budget(&ctx, 100), 87);
}

#[test]
fn test_remaining_budget_saturates_at_zero() {
    let ctx = Some("a".repeat(200));
    assert_eq!(remaining_budget(&ctx, 100), 0);
}

// ── append_to_context ─────────────────────────────────────────────

#[test]
fn test_append_to_context_none_sets_value() {
    let mut ctx: Option<String> = None;
    append_to_context(&mut ctx, "hello".to_string());
    assert_eq!(ctx.as_deref(), Some("hello"));
}

#[test]
fn test_append_to_context_some_adds_separator() {
    let mut ctx: Option<String> = Some("first".to_string());
    append_to_context(&mut ctx, "second".to_string());
    assert_eq!(ctx.as_deref(), Some("first | second"));
}

// ── is_code_file ──────────────────────────────────────────────────────

#[test]
fn test_is_code_file_accepts_known_extensions() {
    for path in &[
        "src/main.rs",
        "app.py",
        "index.ts",
        "page.tsx",
        "mod.js",
        "component.jsx",
        "handler.go",
        "parser.cpp",
        "util.c",
        "header.h",
    ] {
        assert!(is_code_file(path), "expected {path} to be a code file");
    }
}

#[test]
fn test_is_code_file_rejects_non_code() {
    for path in &[
        "README.md",
        "config.toml",
        "data.json",
        "schema.sql",
        "Makefile",
        "image.png",
        "archive.tar.gz",
        "",
    ] {
        assert!(!is_code_file(path), "expected {path} NOT to be a code file");
    }
}

// ── format_caller_list ────────────────────────────────────────────────

#[test]
fn test_format_caller_list_basic() {
    let counts = vec![("Alpha".to_string(), 5usize), ("Beta".to_string(), 3)];
    let result = format_caller_list(&counts, 3);
    // Each entry: "Name(N↑)" joined by middle dot
    assert!(result.contains("Alpha(5"), "got: {result}");
    assert!(result.contains("Beta(3"), "got: {result}");
}

#[test]
fn test_format_caller_list_respects_max_show() {
    let counts: Vec<(String, usize)> = (0..5).map(|i| (format!("Sym{i}"), i + 1)).collect();
    let result = format_caller_list(&counts, 2);
    // Only Sym0 and Sym1 should appear
    assert!(result.contains("Sym0"), "got: {result}");
    assert!(result.contains("Sym1"), "got: {result}");
    assert!(!result.contains("Sym2"), "should be truncated: {result}");
}

#[test]
fn test_format_caller_list_empty_returns_empty_string() {
    let counts: Vec<(String, usize)> = vec![];
    assert_eq!(format_caller_list(&counts, 3), "");
}

// ── large_file_touring_signal_with_est ───────────────────────────────

#[test]
fn test_large_file_signal_with_est_fires_at_threshold() {
    // Exactly at threshold — should produce a signal
    let result = large_file_touring_signal_with_est("src/lib.rs", LARGE_FILE_LINE_THRESHOLD);
    assert!(result.is_some(), "should fire at threshold");
    let (score, text) = result.expect("signal present");
    assert!(
        (score - 1.2_f32).abs() < f32::EPSILON,
        "score should be 1.2"
    );
    assert!(
        text.contains("lib.rs"),
        "text should mention filename: {text}"
    );
}

#[test]
fn test_large_file_signal_with_est_silent_below_threshold() {
    let result = large_file_touring_signal_with_est("src/tiny.rs", LARGE_FILE_LINE_THRESHOLD - 1);
    assert!(result.is_none(), "should be silent below threshold");
}

#[test]
fn test_large_file_signal_with_est_500_plus_shows_85pct() {
    let (_score, text) =
        large_file_touring_signal_with_est("app.py", 501).expect("should fire for large file");
    assert!(text.contains("85%"), "should show 85% for >500L: {text}");
}

#[test]
fn test_large_file_signal_with_est_300_to_500_shows_70pct() {
    let (_score, text) =
        large_file_touring_signal_with_est("app.py", 400).expect("should fire for 400L file");
    assert!(text.contains("70%"), "should show 70% for 300-500L: {text}");
}

#[test]
fn test_large_file_signal_with_est_non_code_returns_none() {
    let result = large_file_touring_signal_with_est("docs/notes.md", 1000);
    assert!(
        result.is_none(),
        "non-code file should return None even if large"
    );
}

// ── suggest_touring_for_code_file_with_est ───────────────────────────

#[test]
fn test_suggest_touring_with_est_non_code_returns_empty() {
    let result = suggest_touring_for_code_file_with_est("schema.sql", 500);
    assert!(
        result.is_empty(),
        "non-code file should return empty string"
    );
}

#[test]
fn test_suggest_touring_with_est_code_file_contains_overview() {
    let result = suggest_touring_for_code_file_with_est("src/lib.rs", 50);
    assert!(!result.is_empty(), "code file should produce a suggestion");
    assert!(
        result.contains("touring ast overview"),
        "should mention touring ast overview: {result}"
    );
    assert!(
        result.contains("lib.rs"),
        "should mention filename: {result}"
    );
}

#[test]
fn test_suggest_touring_with_est_large_file_shows_80pct() {
    let result = suggest_touring_for_code_file_with_est("big.rs", 300);
    assert!(
        result.contains("80%"),
        "large file should show 80%: {result}"
    );
}

#[test]
fn test_suggest_touring_with_est_small_file_shows_50pct() {
    let result = suggest_touring_for_code_file_with_est("small.rs", 100);
    assert!(
        result.contains("50%"),
        "small file should show 50%: {result}"
    );
}
