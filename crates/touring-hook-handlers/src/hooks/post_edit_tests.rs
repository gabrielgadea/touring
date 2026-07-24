#![allow(clippy::indexing_slicing, clippy::len_zero)]
use super::*;
#[test]
fn test_verify_multiconfig_hint_none_for_non_rust() {
    assert!(verify_multiconfig_hint("src/lib.py", None).is_none());
}
#[test]
fn test_rust_workflow_hint_none_for_non_rust_file() {
    let source = "fn main() {}";
    assert!(verify_rust_workflow_hint("foo.py", Some(source)).is_none());
    assert!(verify_rust_workflow_hint("Cargo.toml", Some(source)).is_none());
    assert!(verify_rust_workflow_hint("README.md", Some(source)).is_none());
}
#[test]
fn test_rust_workflow_hint_none_for_empty_source() {
    assert!(verify_rust_workflow_hint("src/empty.rs", Some("")).is_none());
    assert!(verify_rust_workflow_hint("src/empty.rs", Some("   \n\t\n")).is_none());
}
#[test]
fn test_rust_workflow_hint_none_for_trivial_rust() {
    let trivial = "fn helper() {}";
    assert!(
        verify_rust_workflow_hint("src/h.rs", Some(trivial)).is_none(),
        "trivial private fn must not emit a hint"
    );
}
#[test]
fn test_rust_workflow_hint_emits_for_public_surface() {
    let source = "pub fn exported() -> u32 { 42 }\npub struct Marker;";
    let hint = verify_rust_workflow_hint("src/api.rs", Some(source))
        .expect("public API must trigger a hint");
    assert!(hint.contains("rust-workflow"));
    assert!(hint.contains("pub_surface=2"));
    assert!(hint.contains("complexity="));
}
#[test]
fn test_rust_workflow_hint_reports_unsafe_and_async() {
    let source = r#"
            pub async fn fetch() -> u32 { 0 }
            pub unsafe fn raw() -> *const u8 { std::ptr::null() }
        "#;
    let hint =
        verify_rust_workflow_hint("src/low.rs", Some(source)).expect("pub surface triggers hint");
    assert!(
        hint.contains("async_fns=1"),
        "expected async_fns=1, got {hint:?}"
    );
    assert!(hint.contains("unsafe=1"), "expected unsafe=1, got {hint:?}");
}
#[test]
fn test_rust_workflow_hint_none_for_malformed_source() {
    let malformed = "pub fn broken( { unclosed";
    assert!(
        verify_rust_workflow_hint("src/bad.rs", Some(malformed)).is_none(),
        "malformed source must skip silently"
    );
}
#[test]
fn test_rust_workflow_reward_none_for_unknown_extension() {
    assert_eq!(
        compute_rust_workflow_reward("file.xyz", Some("whatever")),
        None
    );
}
#[test]
fn test_rust_workflow_reward_bounded_for_python() {
    if let Some(r) = compute_rust_workflow_reward("script.py", Some("def compute(): return 42\n")) {
        assert!(
            (-0.10..=0.10).contains(&r),
            "python reward {r} outside envelope"
        );
    }
}
#[test]
fn test_rust_workflow_reward_none_for_trivial_or_empty() {
    assert_eq!(compute_rust_workflow_reward("empty.rs", Some("")), None);
    assert_eq!(
        compute_rust_workflow_reward("triv.rs", Some("fn helper() {}")),
        None,
        "private helper with zero complexity must not inject reward"
    );
}
#[test]
fn test_rust_workflow_reward_positive_for_clean_public_api() {
    let src = "pub fn add(a: u32, b: u32) -> u32 { a + b }";
    let r = compute_rust_workflow_reward("src/clean.rs", Some(src))
        .expect("clean pub fn must produce a reward");
    assert!(
        (r - 0.10).abs() < f64::EPSILON,
        "clean code must map to +0.10, got {r}"
    );
}
#[test]
fn test_rust_workflow_reward_negative_for_unsafe_code() {
    let src = "pub unsafe fn raw() -> *const u8 { std::ptr::null() }";
    let r = compute_rust_workflow_reward("src/unsafe_mod.rs", Some(src))
        .expect("unsafe code must produce a reward");
    assert!(
        (r + 0.10).abs() < f64::EPSILON,
        "unsafe code must map to -0.10, got {r}"
    );
}
#[test]
fn test_rust_workflow_reward_bounded_in_range() {
    let samples = [
        "pub fn simple() -> u32 { 1 }",
        "pub unsafe fn danger() {}",
        "pub async fn fetch() -> u32 { 0 }",
    ];
    for s in samples {
        if let Some(r) = compute_rust_workflow_reward("src/x.rs", Some(s)) {
            assert!(
                (-0.10..=0.10).contains(&r),
                "reward {r} out of bounds for source: {s:?}"
            );
        }
    }
}
#[test]
fn test_rust_workflow_reward_none_for_malformed() {
    assert_eq!(
        compute_rust_workflow_reward("src/bad.rs", Some("pub fn broken( {")),
        None,
        "malformed source must not inject reward"
    );
}
#[test]
fn test_verify_multiconfig_hint_preloaded_with_cfg() {
    let source = r#"#[cfg(feature = "async")] fn do_async() {}"#;
    let result = verify_multiconfig_hint("src/lib.rs", Some(source));
    assert!(
        result.is_some(),
        "should detect cfg(feature) from preloaded content"
    );
    assert!(result.unwrap().contains("async"));
}
#[test]
fn test_verify_multiconfig_hint_preloaded_without_cfg() {
    let source = "fn plain() {}";
    let result = verify_multiconfig_hint("src/lib.rs", Some(source));
    assert!(result.is_none(), "no cfg(feature) in source => None");
}
#[test]
fn test_build_edit_summary_edit() {
    let input = serde_json::json!(
        { "tool_input" : { "old_string" : "old code here", "new_string" :
        "new code here" } }
    );
    let summary = build_edit_summary(&input, "Edit");
    assert!(summary.unwrap().contains("old code here"));
}
#[test]
fn test_build_edit_summary_write() {
    let input = serde_json::json!(
        { "tool_input" : { "content" : "line 1\nline 2\nline 3" } }
    );
    let summary = build_edit_summary(&input, "Write");
    assert!(summary.unwrap().contains("3 lines"));
}
#[test]
fn test_build_edit_summary_empty() {
    let input = serde_json::json!({ "tool_input" : {} });
    let summary = build_edit_summary(&input, "Edit");
    assert!(summary.is_none());
}
#[test]
fn test_extract_error_pattern_string_not_found() {
    let pattern = extract_edit_error_pattern("String to replace not found in file");
    assert_eq!(pattern.as_deref(), Some("string_not_found"));
    let pattern2 = extract_edit_error_pattern("The old_string was not found in file");
    assert_eq!(pattern2.as_deref(), Some("string_not_found"));
}
#[test]
fn test_extract_error_pattern_file_modified() {
    let pattern = extract_edit_error_pattern("File has been unexpectedly modified since last read");
    assert_eq!(pattern.as_deref(), Some("file_modified_externally"));
    let pattern2 = extract_edit_error_pattern("The file changed on disk");
    assert_eq!(pattern2.as_deref(), Some("file_modified_externally"));
}
#[test]
fn test_extract_error_pattern_permission_denied() {
    let pattern = extract_edit_error_pattern("Permission denied: /etc/passwd");
    assert_eq!(pattern.as_deref(), Some("permission_denied"));
    let pattern2 = extract_edit_error_pattern("File is read-only");
    assert_eq!(pattern2.as_deref(), Some("permission_denied"));
}
#[test]
fn test_extract_error_pattern_file_not_found() {
    let pattern = extract_edit_error_pattern("No such file or directory: /foo/bar.rs");
    assert_eq!(pattern.as_deref(), Some("file_not_found"));
}
#[test]
fn test_extract_error_pattern_edit_not_unique() {
    let pattern =
        extract_edit_error_pattern("old_string is not unique in file, found multiple matches");
    assert_eq!(pattern.as_deref(), Some("edit_not_unique"));
}
#[test]
fn test_extract_error_pattern_generic_fallback() {
    let pattern = extract_edit_error_pattern(
        "some unusual error that does not match any known category at all",
    );
    assert!(pattern.is_some());
    let p = pattern.unwrap();
    assert!(!p.contains("  "));
    assert!(!p.starts_with('_'));
    assert!(!p.ends_with('_'));
    assert!(p.len() <= 60);
}
#[test]
fn test_extract_error_pattern_short_message_returns_none() {
    let pattern = extract_edit_error_pattern("err");
    assert!(pattern.is_none(), "Messages <= 10 chars should return None");
    let pattern2 = extract_edit_error_pattern("x");
    assert!(pattern2.is_none());
}
#[test]
fn test_extract_error_pattern_exit_code() {
    let pattern = extract_edit_error_pattern("Process exited with exit code 1");
    assert_eq!(pattern.as_deref(), Some("exit_code_nonzero"));
}
#[test]
fn test_extract_error_pattern_syntax_error() {
    let pattern = extract_edit_error_pattern("Syntax error on line 42: unexpected token");
    assert_eq!(pattern.as_deref(), Some("syntax_error"));
}
#[test]
fn test_extract_error_message_is_error_true() {
    let input = serde_json::json!(
        { "tool_use_result" : { "is_error" : true, "content" :
        "String to replace not found in file" } }
    );
    let msg = extract_error_message(&input);
    assert!(msg.is_some());
    assert!(msg.unwrap().contains("String to replace not found"));
}
#[test]
fn test_extract_error_message_is_error_false_no_error() {
    let input = serde_json::json!(
        { "tool_use_result" : { "is_error" : false, "content" :
        "File edited successfully" } }
    );
    let msg = extract_error_message(&input);
    assert!(msg.is_none());
}
#[test]
fn test_extract_error_message_no_is_error_field() {
    let input = serde_json::json!(
        { "tool_input" : { "file_path" : "/foo/bar.rs" } }
    );
    let msg = extract_error_message(&input);
    assert!(msg.is_none());
}
#[test]
fn test_extract_error_message_array_content() {
    let input = serde_json::json!(
        { "tool_use_result" : { "is_error" : true, "content" : [{ "type" : "text",
        "text" : "Permission denied: /etc/shadow" }] } }
    );
    let msg = extract_error_message(&input);
    assert!(msg.is_some());
    assert!(msg.unwrap().contains("Permission denied"));
}
#[test]
fn test_extract_error_message_tool_result_fallback() {
    let input = serde_json::json!(
        { "tool_result" : "Error: file not found at path /missing.rs" }
    );
    let msg = extract_error_message(&input);
    assert!(msg.is_some());
    assert!(msg.unwrap().contains("file not found"));
}
#[test]
fn test_auto_gotcha_not_created_below_threshold() {
    let tmp = tempfile::TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let db = FileKnowledgeDB::new(&db_path).unwrap();
    db.record_edit_with_error(
        "src/main.rs",
        "Edit",
        Some("bad edit"),
        Some("string_not_found"),
    )
    .unwrap();
    maybe_auto_create_gotcha(
        &db,
        "src/main.rs",
        "string_not_found",
        "old_string not found",
    );
    let gotchas = db.list_gotchas();
    assert!(
        gotchas.is_empty(),
        "Should not create gotcha with only 1 occurrence"
    );
}
#[test]
fn test_auto_gotcha_created_at_threshold() {
    let tmp = tempfile::TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let db = FileKnowledgeDB::new(&db_path).unwrap();
    db.record_edit_with_error(
        "src/tricky_module.rs",
        "Edit",
        Some("attempt 1"),
        Some("string_not_found"),
    )
    .unwrap();
    db.record_edit_with_error(
        "src/tricky_module.rs",
        "Edit",
        Some("attempt 2"),
        Some("string_not_found"),
    )
    .unwrap();
    maybe_auto_create_gotcha(
        &db,
        "src/tricky_module.rs",
        "string_not_found",
        "String to replace not found in file",
    );
    let gotchas = db.list_gotchas();
    assert_eq!(gotchas.len(), 1, "Should create exactly 1 gotcha");
    assert!(gotchas[0].gotcha.contains("[auto:E7.1]"));
    assert!(gotchas[0].gotcha.contains("string_not_found"));
    assert_eq!(gotchas[0].severity, "warning");
    assert_eq!(gotchas[0].pattern, "tricky_module");
}
#[test]
fn test_auto_gotcha_deduplication() {
    let tmp = tempfile::TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let db = FileKnowledgeDB::new(&db_path).unwrap();
    for i in 0..3 {
        db.record_edit_with_error(
            "src/fragile.rs",
            "Edit",
            Some(&format!("attempt {i}")),
            Some("file_modified_externally"),
        )
        .unwrap();
    }
    maybe_auto_create_gotcha(
        &db,
        "src/fragile.rs",
        "file_modified_externally",
        "File has been unexpectedly modified",
    );
    maybe_auto_create_gotcha(
        &db,
        "src/fragile.rs",
        "file_modified_externally",
        "File has been unexpectedly modified",
    );
    let gotchas = db.list_gotchas();
    assert_eq!(gotchas.len(), 1, "Should NOT duplicate gotcha");
    assert_eq!(gotchas[0].hit_count, 1);
}
#[test]
fn test_auto_gotcha_different_files_different_gotchas() {
    let tmp = tempfile::TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let db = FileKnowledgeDB::new(&db_path).unwrap();
    for _ in 0..2 {
        db.record_edit_with_error("src/parser.rs", "Edit", None, Some("string_not_found"))
            .unwrap();
    }
    for _ in 0..2 {
        db.record_edit_with_error("src/config.rs", "Edit", None, Some("permission_denied"))
            .unwrap();
    }
    maybe_auto_create_gotcha(
        &db,
        "src/parser.rs",
        "string_not_found",
        "old_string not found",
    );
    maybe_auto_create_gotcha(
        &db,
        "src/config.rs",
        "permission_denied",
        "Permission denied on file",
    );
    let gotchas = db.list_gotchas();
    assert_eq!(
        gotchas.len(),
        2,
        "Different files should create separate gotchas"
    );
}
#[test]
fn test_count_edit_error_pattern() {
    let tmp = tempfile::TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let db = FileKnowledgeDB::new(&db_path).unwrap();
    assert_eq!(
        db.count_edit_error_pattern("src/main.rs", "string_not_found", 20),
        0
    );
    db.record_edit_with_error("src/main.rs", "Edit", None, None)
        .unwrap();
    db.record_edit_with_error("src/main.rs", "Edit", None, Some("string_not_found"))
        .unwrap();
    db.record_edit_with_error("src/main.rs", "Edit", None, None)
        .unwrap();
    db.record_edit_with_error("src/main.rs", "Edit", None, Some("string_not_found"))
        .unwrap();
    db.record_edit_with_error("src/main.rs", "Edit", None, Some("permission_denied"))
        .unwrap();
    assert_eq!(
        db.count_edit_error_pattern("src/main.rs", "string_not_found", 20),
        2
    );
    assert_eq!(
        db.count_edit_error_pattern("src/main.rs", "permission_denied", 20),
        1
    );
    assert_eq!(
        db.count_edit_error_pattern("src/main.rs", "nonexistent", 20),
        0
    );
    assert_eq!(
        db.count_edit_error_pattern("src/other.rs", "string_not_found", 20),
        0
    );
}
#[test]
fn test_truncate() {
    assert_eq!(truncate("short", 10), "short");
    assert_eq!(truncate("a longer string here", 10), "a longe...");
}
#[test]
fn test_byte_boundary_ascii() {
    assert_eq!(byte_boundary("hello world", 5), 5);
    assert_eq!(byte_boundary("hello world", 100), 11);
}
#[test]
fn test_byte_boundary_multibyte() {
    let s = "café";
    assert_eq!(s.len(), 5);
    assert_eq!(byte_boundary(s, 3), 3);
    assert_eq!(byte_boundary(s, 4), 3);
    assert_eq!(byte_boundary(s, 5), 5);
    assert_eq!(byte_boundary(s, 100), 5);
}
#[test]
fn test_reindex_file_updates_wiring_map() {
    let tmp = tempfile::TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let db = FileKnowledgeDB::new(&db_path).unwrap();
    let knowledge = crate::knowledge::FileKnowledge {
        file_path: "src/analyzer.rs".into(),
        language: Some("rust".into()),
        symbols_json: Some(
            r#"[{"name":"Analyzer","kind":"struct","is_public":true},
                    {"name":"helper","kind":"function","is_public":false}]"#
                .into(),
        ),
        imports_json: Some(r#"["crate::core::Config"]"#.into()),
        ..Default::default()
    };
    db.upsert(&knowledge).unwrap();
    crate::wiring::update_wiring_after_edit(&db, "src/analyzer.rs");
    let status = db.module_wiring_status("src/analyzer.rs").unwrap();
    assert_eq!(status.total_pub_symbols, 1, "only 1 pub symbol");
    assert_eq!(
        status.orphan_symbols,
        vec!["Analyzer"],
        "Analyzer should be orphan"
    );
    assert_eq!(status.integration_score, 0.0, "no consumers = score 0.0");
}
#[test]
fn test_wiring_update_after_consumer_added() {
    let tmp = tempfile::TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let db = FileKnowledgeDB::new(&db_path).unwrap();
    db.register_pub_symbol("src/core.rs", "Config", "struct", "public")
        .unwrap();
    let consumer = crate::knowledge::FileKnowledge {
        file_path: "src/app.rs".into(),
        language: Some("rust".into()),
        symbols_json: Some(r#"[]"#.into()),
        imports_json: Some(r#"["crate::core::Config"]"#.into()),
        ..Default::default()
    };
    db.upsert(&consumer).unwrap();
    crate::wiring::update_wiring_after_edit(&db, "src/app.rs");
    let score = db.integration_score("src/core.rs").unwrap();
    assert!(
        score > 0.0,
        "score should improve after consumer is added, got {score}"
    );
}
#[test]
fn test_parse_source_and_lang_empty_lang_str() {
    let result = parse_source_and_lang("/nonexistent/path.rs", "", None);
    assert!(result.is_none(), "empty lang_str should yield None");
}
#[test]
fn test_parse_source_and_lang_unrecognised_lang() {
    let result = parse_source_and_lang("/nonexistent/path.xyz", "cobol_from_1970", None);
    assert!(result.is_none(), "unknown lang tag should yield None");
}
#[test]
fn test_parse_source_and_lang_missing_file() {
    let result = parse_source_and_lang("/no/such/file/exists.rs", "rust", None);
    assert!(result.is_none(), "missing file should yield None");
}
#[test]
fn test_parse_source_and_lang_valid() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(tmp.path(), "fn main() {}").unwrap();
    let result = parse_source_and_lang(tmp.path().to_str().unwrap(), "rust", None);
    assert!(result.is_some(), "valid file + lang should succeed");
    let (source, _lang) = result.unwrap();
    assert!(source.contains("fn main()"));
}
#[test]
fn test_parse_source_and_lang_preloaded() {
    let result = parse_source_and_lang("/nonexistent.rs", "rust", Some("fn hello() {}"));
    assert!(
        result.is_some(),
        "preloaded content should bypass disk read"
    );
    let (source, _lang) = result.unwrap();
    assert!(source.contains("fn hello()"));
}
#[test]
fn test_compute_antipattern_issues_skips_test_files() {
    let issues = compute_antipattern_issues("fn helper() { panic!(\"oops\") }", "rust", true);
    assert!(
        issues.is_empty(),
        "test files should produce no antipattern issues"
    );
}
#[test]
fn test_compute_antipattern_issues_empty_source() {
    let issues = compute_antipattern_issues("", "rust", false);
    let _ = issues;
}
#[test]
fn test_compute_antipattern_issues_non_test_runs_detection() {
    let issues = compute_antipattern_issues("fn ok() {}", "rust", false);
    let _ = issues;
}
#[test]
fn test_label_for_layer_syntax() {
    assert_eq!(
        label_for_layer(&touring_code::ast::ValidationLayer::Syntax),
        "SYNTAX"
    );
}
#[test]
fn test_label_for_layer_symbol() {
    assert_eq!(
        label_for_layer(&touring_code::ast::ValidationLayer::SymbolResolution),
        "SYMBOL"
    );
}
#[test]
fn test_label_for_layer_structural() {
    assert_eq!(
        label_for_layer(&touring_code::ast::ValidationLayer::Structural),
        "STRUCTURAL"
    );
}
#[test]
fn test_label_for_layer_import() {
    assert_eq!(
        label_for_layer(&touring_code::ast::ValidationLayer::ImportCheck),
        "IMPORT"
    );
}
#[test]
fn test_collapse_underscores_no_change() {
    assert_eq!(collapse_underscores("hello_world"), "hello_world");
}
#[test]
fn test_collapse_underscores_consecutive() {
    assert_eq!(collapse_underscores("hello___world"), "hello_world");
}
#[test]
fn test_collapse_underscores_leading_trailing() {
    assert_eq!(collapse_underscores("__hello__"), "_hello_");
}
#[test]
fn test_collapse_underscores_all_underscores() {
    assert_eq!(collapse_underscores("____"), "_");
}
#[test]
fn test_collapse_underscores_empty() {
    assert_eq!(collapse_underscores(""), "");
}
fn make_gotcha(id: i64, pattern: &str, gotcha_text: &str) -> crate::knowledge::Gotcha {
    crate::knowledge::Gotcha {
        id,
        pattern: pattern.into(),
        gotcha: gotcha_text.into(),
        severity: "warning".into(),
        language: None,
        symbol_name: None,
        hit_count: 0,
        prevented_errors: 0,
        created_at: String::new(),
    }
}
#[test]
fn test_find_existing_gotcha_found() {
    let gotchas = vec![make_gotcha(1, "foo.rs", "string_not_found pattern here")];
    let result = find_existing_gotcha(&gotchas, "string_not_found");
    assert!(result.is_some());
    assert_eq!(result.unwrap().id, 1);
}
#[test]
fn test_find_existing_gotcha_not_found() {
    let gotchas = vec![make_gotcha(1, "foo.rs", "some other error")];
    let result = find_existing_gotcha(&gotchas, "string_not_found");
    assert!(result.is_none());
}
#[test]
fn test_find_existing_gotcha_empty_slice() {
    let result = find_existing_gotcha(&[], "any_pattern");
    assert!(result.is_none());
}
#[test]
fn test_normalize_error_fallback_basic() {
    let result = normalize_error_fallback("something went wrong here");
    assert!(result.is_some());
    let key = result.unwrap();
    assert!(!key.starts_with('_'));
    assert!(!key.ends_with('_'));
    assert!(!key.contains("  "));
}
#[test]
fn test_normalize_error_fallback_all_punctuation_returns_none() {
    let result = normalize_error_fallback("!!! ??? ---");
    assert!(result.is_none(), "all-punctuation should return None");
}
#[test]
fn test_normalize_error_fallback_truncates_to_50_chars() {
    let long = "a".repeat(200);
    let result = normalize_error_fallback(&long);
    assert!(result.is_some());
    assert!(
        result.unwrap().len() <= 55,
        "key should be bounded to ~50 chars"
    );
}
#[test]
fn test_normalize_error_fallback_collapses_spaces() {
    let result = normalize_error_fallback("err   msg");
    let key = result.unwrap();
    assert!(
        !key.contains("__"),
        "consecutive underscores should be collapsed"
    );
}
#[test]
fn test_compose_post_edit_feedback_no_issues() {
    let result = compose_post_edit_feedback(vec![], 0);
    assert!(result.contains("0 issue(s)"));
}
#[test]
fn test_compose_post_edit_feedback_single_issue() {
    let issues = vec!["SYNTAX: unexpected token".to_string()];
    let result = compose_post_edit_feedback(issues, 3);
    assert!(result.contains("1 issue(s)"));
    assert!(result.contains("SYNTAX: unexpected token"));
}
#[test]
fn test_compose_post_edit_feedback_budget_drops_excess() {
    let big = "x".repeat(700);
    let issues = vec![big.clone(), big.clone(), big.clone()];
    let result = compose_post_edit_feedback(issues, 0);
    let count: usize = result.split(" | ").count().saturating_sub(1);
    assert!(count < 3, "budget should have dropped at least 1 issue");
}
#[test]
fn test_compose_post_edit_feedback_format() {
    let issues = vec!["A".to_string(), "B".to_string()];
    let result = compose_post_edit_feedback(issues, 3);
    assert!(result.starts_with("post-edit verification:"));
    assert!(result.contains(" | "));
}
#[test]
fn test_issue_priority_syntax_highest() {
    assert!(issue_priority("SYNTAX: unexpected token") > issue_priority("ANTIPATTERN: unwrap"));
    assert!(issue_priority("SYMBOL: undefined Foo") > issue_priority("WIRING: orphan"));
}
#[test]
fn test_issue_priority_antipattern_above_wiring() {
    assert!(issue_priority("ANTIPATTERN [3x]: .unwrap()") > issue_priority("WIRING: 2 orphans"));
}
#[test]
fn test_issue_priority_complexity_above_wiring() {
    assert!(issue_priority("HIGH COMPLEXITY: CC_max=25") > issue_priority("WIRING: orphan"));
}
#[test]
fn test_issue_priority_feature_gated_above_default() {
    assert!(issue_priority("feature-gated [async]") > issue_priority("some other signal"));
}
#[test]
fn test_issue_priority_health_above_wiring() {
    assert!(
        issue_priority("HEALTH DEGRADED 0.62 [wiring:0.55 quality:0.71] 124ms — orphan symbols")
            > issue_priority("WIRING: 2 orphan pub symbol(s)"),
        "HEALTH (1.2) should sort above WIRING (1.0)"
    );
}
#[test]
fn test_issue_priority_health_below_complexity() {
    assert!(
        issue_priority("COMPLEXITY CC_max=25")
            > issue_priority("HEALTH DEGRADED 0.72 [wiring:0.65] 98ms — weak dim"),
        "COMPLEXITY (1.5) should sort above HEALTH (1.2)"
    );
}
#[test]
fn test_compose_post_edit_feedback_priority_sort_order() {
    let issues = vec![
        "WIRING: 2 orphan pub symbol(s)".to_string(),
        "SYNTAX: unexpected token at line 42".to_string(),
    ];
    let result = compose_post_edit_feedback(issues, 3);
    let syntax_pos = result.find("SYNTAX").expect("SYNTAX should be present");
    let wiring_pos = result.find("WIRING").expect("WIRING should be present");
    assert!(
        syntax_pos < wiring_pos,
        "SYNTAX (priority 2.5) should appear before WIRING (priority 1.0)"
    );
}
#[test]
fn test_compose_post_edit_feedback_priority_survives_budget() {
    let syntax_issue = "SYNTAX: missing semicolon".to_string();
    let wiring_big = format!("WIRING: {}", "x".repeat(1100));
    let feature_issue = "feature-gated [foo]: check --all-features".to_string();
    let issues = vec![wiring_big, feature_issue, syntax_issue];
    let result = compose_post_edit_feedback(issues, 0);
    assert!(
        result.contains("SYNTAX"),
        "high-priority SYNTAX issue should survive budget truncation"
    );
}
#[test]
fn test_collect_layer_diagnostics_passed_layer_returns_empty() {
    let layer = touring_code::ast::LayerResult {
        layer: touring_code::ast::ValidationLayer::Syntax,
        passed: true,
        diagnostics: vec!["ignored".to_string()],
        score: 1.0,
    };
    let result = collect_layer_diagnostics(&layer);
    assert!(result.is_empty(), "passed layer must return no diagnostics");
}
#[test]
fn test_collect_layer_diagnostics_failed_with_messages() {
    let layer = touring_code::ast::LayerResult {
        layer: touring_code::ast::ValidationLayer::Syntax,
        passed: false,
        diagnostics: vec!["err1".to_string(), "err2".to_string()],
        score: 0.0,
    };
    let result = collect_layer_diagnostics(&layer);
    assert_eq!(result.len(), 2);
    assert!(result[0].contains("SYNTAX"));
    assert!(result[0].contains("err1"));
}
#[test]
fn test_collect_layer_diagnostics_caps_at_3() {
    let layer = touring_code::ast::LayerResult {
        layer: touring_code::ast::ValidationLayer::ImportCheck,
        passed: false,
        diagnostics: (1..=6).map(|i| format!("diag{i}")).collect(),
        score: 0.0,
    };
    let result = collect_layer_diagnostics(&layer);
    assert_eq!(result.len(), 3, "must cap at 3 diagnostics");
    assert!(result.iter().all(|s| s.contains("IMPORT")));
    assert!(!result.iter().any(|s| s.contains("diag4")));
}
#[test]
fn test_collect_layer_diagnostics_symbol_layer_label() {
    let layer = touring_code::ast::LayerResult {
        layer: touring_code::ast::ValidationLayer::SymbolResolution,
        passed: false,
        diagnostics: vec!["undefined: Foo".to_string()],
        score: 0.0,
    };
    let result = collect_layer_diagnostics(&layer);
    assert_eq!(result.len(), 1);
    assert!(
        result[0].contains("SYMBOL"),
        "SymbolResolution -> SYMBOL label"
    );
}
#[test]
fn test_extract_first_text_block_found() {
    let arr = vec![
        serde_json::json!({ "type" : "text", "text" : "hello error" }),
        serde_json::json!({ "type" : "text", "text" : "second block" }),
    ];
    let result = extract_first_text_block(&arr);
    assert_eq!(result.as_deref(), Some("hello error"));
}
#[test]
fn test_extract_first_text_block_no_text_field() {
    let arr = vec![serde_json::json!({ "type" : "image" })];
    let result = extract_first_text_block(&arr);
    assert!(result.is_none());
}
#[test]
fn test_extract_first_text_block_empty_array() {
    let arr: Vec<serde_json::Value> = vec![];
    let result = extract_first_text_block(&arr);
    assert!(result.is_none());
}
#[test]
fn test_extract_first_text_block_truncates_long_text() {
    let long_text = "a".repeat(400);
    let arr = vec![serde_json::json!({ "type" : "text", "text" : long_text })];
    let result = extract_first_text_block(&arr).unwrap();
    assert!(result.len() <= 303, "should truncate at 300 chars + '...'");
    assert!(result.ends_with("..."));
}
#[test]
fn test_extract_implicit_error_detects_error_keyword() {
    let input = serde_json::json!(
        { "tool_result" : "An error occurred during processing" }
    );
    let result = extract_implicit_error(&input);
    assert!(result.is_some());
}
#[test]
fn test_extract_implicit_error_detects_not_found() {
    let input = serde_json::json!(
        { "tool_result" : "File not found at the given path" }
    );
    let result = extract_implicit_error(&input);
    assert!(result.is_some());
}
#[test]
fn test_extract_implicit_error_detects_permission_denied() {
    let input = serde_json::json!(
        { "tool_result" : "Permission denied accessing /etc/shadow" }
    );
    let result = extract_implicit_error(&input);
    assert!(result.is_some());
}
#[test]
fn test_extract_implicit_error_detects_failed() {
    let input = serde_json::json!(
        { "tool_result" : "Operation failed with code 42" }
    );
    let result = extract_implicit_error(&input);
    assert!(result.is_some());
}
#[test]
fn test_extract_implicit_error_clean_message_returns_none() {
    let input = serde_json::json!({ "tool_result" : "File edited successfully" });
    let result = extract_implicit_error(&input);
    assert!(result.is_none(), "success message should return None");
}
#[test]
fn test_extract_implicit_error_prefers_tool_result_key() {
    let input = serde_json::json!(
        { "tool_result" : "failed here", "tool_use_result" : { "content" : "success"
        } }
    );
    let result = extract_implicit_error(&input);
    assert!(result.is_some());
}
#[test]
fn test_extract_explicit_error_content_string() {
    let input = serde_json::json!(
        { "tool_use_result" : { "content" : "explicit error string" } }
    );
    let result = extract_explicit_error_content(&input);
    assert_eq!(result.as_deref(), Some("explicit error string"));
}
#[test]
fn test_extract_explicit_error_content_array() {
    let input = serde_json::json!(
        { "tool_use_result" : { "content" : [{ "type" : "text", "text" :
        "block error" }] } }
    );
    let result = extract_explicit_error_content(&input);
    assert_eq!(result.as_deref(), Some("block error"));
}
#[test]
fn test_extract_explicit_error_content_falls_back_to_tool_result() {
    let input = serde_json::json!(
        { "tool_use_result" : { "content" : null }, "tool_result" : "fallback error"
        }
    );
    let result = extract_explicit_error_content(&input);
    assert_eq!(result.as_deref(), Some("fallback error"));
}
#[test]
fn test_extract_explicit_error_content_missing_all_returns_none() {
    let input = serde_json::json!({ "other_key" : "value" });
    let result = extract_explicit_error_content(&input);
    assert!(result.is_none());
}
#[test]
fn test_summarize_edit_tool_formats_arrow() {
    let input = serde_json::json!(
        { "tool_input" : { "old_string" : "foo", "new_string" : "bar" } }
    );
    let result = summarize_edit_tool(&input);
    assert!(result.is_some());
    let s = result.unwrap();
    assert!(s.contains("foo"));
    assert!(s.contains("bar"));
    assert!(s.contains("→"));
}
#[test]
fn test_summarize_edit_tool_both_empty_returns_none() {
    let input = serde_json::json!(
        { "tool_input" : { "old_string" : "", "new_string" : "" } }
    );
    assert!(summarize_edit_tool(&input).is_none());
}
#[test]
fn test_summarize_write_tool_counts_lines() {
    let input = serde_json::json!(
        { "tool_input" : { "content" : "a\nb\nc\nd\ne" } }
    );
    let result = summarize_write_tool(&input);
    assert!(result.unwrap().contains("5 lines"));
}
#[test]
fn test_summarize_write_tool_empty_content_zero_lines() {
    let input = serde_json::json!({ "tool_input" : { "content" : "" } });
    let result = summarize_write_tool(&input);
    assert!(result.unwrap().contains("0 lines"));
}
#[test]
fn test_build_edit_summary_unknown_tool_returns_none() {
    let input = serde_json::json!(
        { "tool_input" : { "old_string" : "x", "new_string" : "y" } }
    );
    let result = build_edit_summary(&input, "UnknownTool");
    assert!(result.is_none(), "unknown tool name should return None");
}
#[test]
fn check_block_gate_returns_block_on_antipattern_marker() {
    let issues = vec![
        "ANTIPATTERN_BLOCK: delta=5 threshold=4".to_string(),
        "ANTIPATTERN [5x]: unwrap".to_string(),
    ];
    let result = check_block_gate(&issues, "src/lib.rs");
    assert!(
        result.is_some(),
        "ANTIPATTERN_BLOCK marker must trigger block gate"
    );
    let response = result.unwrap();
    match response {
        HookResponse::Block { reason, .. } => {
            assert!(
                reason.contains("src/lib.rs"),
                "block reason must name the file"
            );
        }
        other => panic!("expected HookResponse::Block, got {other:?}"),
    }
}
#[test]
fn check_block_gate_returns_none_without_marker() {
    let issues = vec!["ANTIPATTERN [2x]: unwrap".to_string()];
    assert!(
        check_block_gate(&issues, "src/lib.rs").is_none(),
        "normal antipattern issues without BLOCK marker must not block"
    );
}
