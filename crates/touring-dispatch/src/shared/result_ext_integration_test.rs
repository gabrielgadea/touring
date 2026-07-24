//! Integration tests for ResultExt/OptionExt in hook-like scenarios.
//!
//! Tests Ok/Err/Some/None paths with realistic hook inputs:
//! file paths, scores, counts, and JSON tool inputs.

use crate::shared::result_ext::{OptionExt, ResultExt};

#[test]
fn test_result_ext_ok_path_file_path() {
    // Simulates: parsing a file_path from JSON tool_input
    let result: Result<&str, &str> = Ok("/home/user/project/src/main.rs");
    let fallback = "";
    assert_eq!(
        result.unwrap_or_debug(fallback, "test: file_path ok"),
        "/home/user/project/src/main.rs"
    );
}

#[test]
fn test_result_ext_err_path_file_path() {
    // Simulates: JSON pointer miss / parse failure for file_path
    let result: Result<&str, &str> = Err("json_pointer_not_found");
    let fallback = "";
    assert_eq!(result.unwrap_or_debug(fallback, "test: file_path err"), "");
}

#[test]
fn test_result_ext_ok_path_score() {
    // Simulates: blast_radius score from SymbolIndex
    let result: Result<f32, std::convert::Infallible> = Ok(0.85);
    assert_eq!(result.unwrap_or_debug(0.0, "test: score ok"), 0.85);
}

#[test]
fn test_result_ext_err_path_score() {
    // Simulates: SymbolIndex unavailable
    let result: Result<f32, &str> = Err("symbol_index_unavailable");
    assert_eq!(result.unwrap_or_debug(0.0, "test: score err"), 0.0);
}

#[test]
fn test_result_ext_ok_path_count() {
    // Simulates: blast_radius file_count
    let result: Result<usize, std::convert::Infallible> = Ok(42);
    assert_eq!(result.unwrap_or_debug(0, "test: count ok"), 42);
}

#[test]
fn test_result_ext_err_path_count() {
    // Simulates: blast_radius computation failure
    let result: Result<usize, &str> = Err("blast_radius_failed");
    assert_eq!(result.unwrap_or_debug(0, "test: count err"), 0);
}

#[test]
fn test_option_ext_some_path() {
    // Simulates: CILA level from stable_session
    let option: Option<u8> = Some(2);
    assert_eq!(option.unwrap_or_debug(0, "test: cila_level some"), 2);
}

#[test]
fn test_option_ext_none_path() {
    // Simulates: stable_session unavailable (cold start)
    let option: Option<u8> = None;
    assert_eq!(option.unwrap_or_debug(3, "test: cila_level none"), 3);
}

#[test]
fn test_option_ext_some_path_content() {
    // Simulates: file content from tool_input
    let option: Option<&str> = Some("fn main() { println!(\"hello\"); }");
    assert_eq!(
        option.unwrap_or_debug("", "test: content some"),
        "fn main() { println!(\"hello\"); }"
    );
}

#[test]
fn test_option_ext_none_path_content() {
    // Simulates: content not provided in tool_input
    let option: Option<&str> = None;
    assert_eq!(option.unwrap_or_debug("", "test: content none"), "");
}

#[test]
fn test_result_ext_with_default_string() {
    // Simulates: old_string from tool_input with fallback
    let result: Result<&str, &str> = Err("pointer_missing");
    let fallback = "default_value";
    assert_eq!(
        result.unwrap_or_debug(fallback, "test: old_string err"),
        "default_value"
    );
}

#[test]
fn test_option_ext_with_default_bool() {
    // Simulates: is_cached flag with fallback
    let option: Option<bool> = None;
    assert_eq!(option.unwrap_or_debug(false, "test: is_cached none"), false);
    let option_some: Option<bool> = Some(true);
    assert_eq!(
        option_some.unwrap_or_debug(false, "test: is_cached some"),
        true
    );
}

#[test]
fn test_result_ext_nested_result_ok() {
    // Simulates: parsing nested JSON then extracting a value
    let result: Result<Result<i32, &str>, &str> = Ok(Ok(100));
    assert_eq!(result.unwrap_or_debug(Ok(0), "test: nested ok"), Ok(100));
}

#[test]
fn test_option_ext_with_path_fallback() {
    // Simulates: extracting file_name from path with fallback
    let path = std::path::Path::new("/some/path/file.txt");
    let option: Option<&str> = path.file_name().and_then(|n| n.to_str());
    assert_eq!(
        option.unwrap_or_debug("unknown", "test: file_name some"),
        "file.txt"
    );

    let option_none: Option<&str> = None;
    assert_eq!(
        option_none.unwrap_or_debug("unknown", "test: file_name none"),
        "unknown"
    );
}

#[test]
fn test_result_ext_debug_context_logging() {
    // Verify debug context is properly formed
    let err_result: Result<i32, &str> = Err("test_error");
    let context = "post_write: project_root fallback";
    let result = err_result.unwrap_or_debug(0, context);
    assert_eq!(result, 0);
    // The debug log is emitted via tracing::debug! - verified via cargo test --nocapture
}
