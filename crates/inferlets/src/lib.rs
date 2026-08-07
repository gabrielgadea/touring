//! `inferlets` — WASM inferlets for touring-wasm.
//!
//! This crate provides sandboxed WebAssembly modules for evaluation in the
//! touring neural hooks system. All inferlets are compiled into a SINGLE WASM
//! module with a single `evaluate()` entry point that dispatches to the
//! appropriate inferlet based on the `__inferlet__` key in the input JSON.
//!
//! # Architecture
//!
//! All 4 inferlets (always_success, memory, pattern, classifier) are
//! compiled into ONE WASM module with a single `evaluate()` entry point.
//! Internal dispatch is based on `__inferlet__` discriminator in the input.
//!
//! # WASM ABI
//!
//! The exported interface is zero-argument for compatibility with the
//! AsyncInferletPool runner. Input is passed via a thread_local buffer:
//! 1. Host writes JSON to WASM memory via `set_input(ptr, len)`
//! 2. Host calls `evaluate()` (no args) — reads from thread_local buffer
//!
//! # Available Inferlets
//!
//! - `always_success`: Always returns success — for testing
//! - `memory`: Analyzes memory access patterns
//! - `pattern`: Performs pattern matching
//! - `classifier`: Semantic classification using keyword scoring

#![deny(missing_docs)]
// RBP-01 elite-lint ratchet (2026-06-16): prod-unwrap-free — lock against future
// bare unwrap in non-test code (`.expect("…")` stays the sanctioned escape).
#![cfg_attr(not(test), deny(clippy::unwrap_used))]

pub mod always_success;
pub mod classifier;
pub mod composite_health_trend;
pub mod count_files_via_cli_wrapper;
pub mod dependency_diff;
pub mod find_circular_imports;
pub mod flaky_test_pattern_detector;
pub mod manifest;
pub mod memory;
pub mod pattern;
pub mod serialized;
pub mod synergy_health_check;
pub mod tantivy_query_builder;
pub mod tdg_grade_distribution;
pub mod top_n_complex_files;
pub mod unused_pub_symbols;

pub use manifest::{InferletDep, InferletManifest, InferletManifestLoadError};
/// Serialized inferlet for persistent caching.
pub use serialized::SerializedInferlet;

use std::cell::RefCell;
use std::thread_local;

// ── Thread-local input buffer ─────────────────────────────────────────────────

thread_local! {
    static INPUT: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// Set the input buffer — called by host before `evaluate()`.
///
/// Returns 0 on success, -1 if len is 0.
///
/// Note: in WASM32, address 0 is a perfectly valid pointer — we only
/// reject zero-length inputs. The caller writes to WASM memory and passes
/// the pointer (typically 0) and length.
///
/// # Safety
///
/// `ptr` must be valid for `len` bytes. The caller (wasmtime host) guarantees
/// this for the duration of the call. The function immediately copies the
/// data into a Rust `String`, after which the WASM memory pointer need not
/// remain valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn set_input(ptr: *const u8, len: usize) -> i32 {
    if len == 0 {
        INPUT.with(|cell| cell.borrow_mut().take());
        return -1;
    }
    // SAFETY: caller (wasmtime host) guarantees ptr is valid for len bytes
    // for the duration of this call. We immediately copy to a Rust String,
    // after which the WASM memory pointer no longer needs to be valid.
    let slice = unsafe { std::slice::from_raw_parts(ptr, len) };
    match String::from_utf8(slice.to_vec()) {
        Ok(s) => {
            INPUT.with(|cell| *cell.borrow_mut() = Some(s));
            0
        }
        Err(_) => -1,
    }
}

/// Get the current input from the thread_local buffer.
fn get_input() -> Option<String> {
    INPUT.with(|cell| cell.borrow_mut().take())
}

/// Dispatching evaluate — the single WASM export entry point with ZERO args.
///
/// Reads the input from the thread_local buffer set by `set_input()`.
/// Dispatches based on `__inferlet__` discriminator; falls back to trying
/// all inferlets if the discriminator is missing.
#[unsafe(no_mangle)]
pub extern "C" fn evaluate() -> i32 {
    let input = match get_input() {
        Some(s) => s,
        None => return 0,
    };

    // Check for inferlet discriminator
    if let Some(kind) = extract_inferlet_kind(&input) {
        // Strip the __inferlet__ key from the JSON so the inferlet only
        // sees the actual data, not the dispatcher metadata.
        let data = strip_inferlet_key(&input);
        return dispatch(kind, &data);
    }

    // No discriminator — try all inferlets; return 1 if any matched
    let any_matched = always_success::evaluate_raw(&input) != 0
        || memory::evaluate_raw(&input) != 0
        || pattern::evaluate_raw(&input) != 0
        || classifier::evaluate_raw(&input) != 0;

    any_matched as i32
}

/// Dispatch to a specific inferlet by name.
fn dispatch(kind: &str, input: &str) -> i32 {
    match kind {
        "always_success" => always_success::evaluate_raw(input),
        "memory" => memory::evaluate_raw(input),
        "pattern" => pattern::evaluate_raw(input),
        "classifier" => classifier::evaluate_raw(input),
        "tantivy_query_builder" => tantivy_query_builder::evaluate_raw(input),
        "synergy_health_check" => synergy_health_check::evaluate_raw(input),
        "unused_pub_symbols" => unused_pub_symbols::evaluate_raw(input),
        "dependency_diff" => dependency_diff::evaluate_raw(input),
        "tdg_grade_distribution" => tdg_grade_distribution::evaluate_raw(input),
        "composite_health_trend" => composite_health_trend::evaluate_raw(input),
        "flaky_test_pattern_detector" => flaky_test_pattern_detector::evaluate_raw(input),
        "count_files_via_cli_wrapper" => count_files_via_cli_wrapper::evaluate_raw(input),
        "top_n_complex_files" => top_n_complex_files::evaluate_raw(input),
        "find_circular_imports" => find_circular_imports::evaluate_raw(input),
        _ => 0,
    }
}

/// Find the closing quote position for a JSON string value.
/// Scans forward from `after_open_quote` looking for a quote not preceded by backslash.
/// Returns the absolute position of the closing quote in the original string, or None.
fn find_closing_quote(_original: &str, after_open_quote: &str) -> Option<usize> {
    for (i, c) in after_open_quote.char_indices() {
        if c == '"' {
            let backslash_count = after_open_quote[..i]
                .chars()
                .rev()
                .take_while(|&ch| ch == '\\')
                .count();
            if backslash_count % 2 == 0 {
                return Some(i);
            }
        }
    }
    None
}

/// Parse the value from a JSON key-value pair after the colon.
/// Returns (value_end_absolute_in_trimmed, value_content_without_quotes) or None.
fn parse_json_value<'a>(trimmed: &'a str, after_key: &'a str) -> Option<(usize, &'a str)> {
    let colon_pos_relative = after_key.find(':')?; // relative to after_key
    let value_search = &after_key[colon_pos_relative + 1..];
    let first_quote = value_search.find('"')?;
    let value_content = &value_search[first_quote + 1..];
    let close_quote = find_closing_quote(after_key, value_content)?;

    // Calculate absolute position of closing quote in trimmed string
    // after_key starts at key_start in trimmed
    // key_start + colon_pos_relative + 1 + first_quote + 1 + close_quote
    let key_start = trimmed.len() - after_key.len();
    let close_quote_abs = key_start + colon_pos_relative + 1 + first_quote + 1 + close_quote;

    Some((close_quote_abs, &value_content[..close_quote]))
}

/// Strip the `__inferlet__` key-value pair from a JSON object.
///
/// Input:  `{"__inferlet__":"memory","cooking":"pizza"}`
/// Output: `{"cooking":"pizza"}`
///
/// Uses direct string manipulation to avoid serde_json dependency issues
/// in the WASM environment. Targets the specific format produced by
/// `wrap_for_dispatcher`.
fn strip_inferlet_key(input: &str) -> String {
    let trimmed = input.trim();
    if !trimmed.starts_with('{') || !trimmed.contains("\"__inferlet__\"") {
        return input.to_string();
    }

    let key_start = match trimmed.find("\"__inferlet__\"") {
        Some(pos) => pos,
        None => return input.to_string(),
    };

    let after_key = &trimmed[key_start..];
    let (close_quote_abs, _value) = match parse_json_value(trimmed, after_key) {
        Some(v) => v,
        None => return input.to_string(),
    };

    let after_pair = &trimmed[close_quote_abs + 1..].trim_start();
    let after_comma = after_pair.strip_prefix(',').unwrap_or(after_pair);

    let prefix_trimmed = trimmed[..key_start].trim_end();
    let suffix_trimmed = after_comma.trim_start();

    if !prefix_trimmed.ends_with('{') {
        return input.to_string();
    }

    if suffix_trimmed.starts_with('}') {
        return "{}".to_string();
    }

    format!("{{{}}}", suffix_trimmed)
}

/// Extract the inferlet kind from a JSON input string.
///
/// Looks for `"__inferlet__": "kind"` and returns the kind if found.
fn extract_inferlet_kind(input: &str) -> Option<&str> {
    let trimmed = input.trim();
    if !trimmed.starts_with('{') {
        return None;
    }

    if let Some(start) = trimmed.find("\"__inferlet__\"") {
        let after_key = &trimmed[start..];
        if let Some(colon) = after_key.find(':') {
            let rest = &after_key[colon + 1..];
            let val_start = rest.find(|c: char| !c.is_ascii_whitespace())?;
            let rest = &rest[val_start..];
            if let Some(stripped) = rest.strip_prefix('"')
                && let Some(end) = stripped.find('"')
            {
                return Some(&stripped[..end]);
            }
        }
    }
    None
}

/// Run a touring CLI subcommand and parse its JSON stdout.
///
/// Returns `None` if the process fails to start, exits non-zero, or the
/// output is not valid JSON.
///
/// Used by inferlets that shell out to the `touring` CLI to collect live
/// metrics and parse the result — avoids repeating the spawn/status/parse
/// boilerplate across call-sites in `composite_health_trend` and
/// `tdg_grade_distribution`.
pub(crate) fn run_touring_json(args: &[&str]) -> Option<serde_json::Value> {
    let output = std::process::Command::new("touring")
        .args(args)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str(&text).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_inferlet_key_removes_memory() {
        let input = r#"{"__inferlet__":"memory","cooking":"pizza"}"#;
        let stripped = strip_inferlet_key(input);
        assert!(!stripped.contains("__inferlet__"), "stripped={}", stripped);
        assert!(stripped.contains("cooking"), "stripped={}", stripped);
    }

    #[test]
    fn test_strip_inferlet_key_removes_pattern() {
        let input = r#"{"__inferlet__":"pattern","foo":"bar"}"#;
        let stripped = strip_inferlet_key(input);
        assert!(!stripped.contains("__inferlet__"), "stripped={}", stripped);
    }

    #[test]
    fn test_memory_does_not_match_cooking() {
        // This is the failing test case: {"cooking":"pizza"} should NOT match memory
        let stripped = r#"{"cooking":"pizza"}"#;
        let result = memory::evaluate_raw(stripped);
        assert_eq!(result, 0, "cooking should not match memory");
    }

    #[test]
    fn test_memory_does_not_match_foo_bar() {
        let stripped = r#"{"foo":"bar"}"#;
        let result = memory::evaluate_raw(stripped);
        assert_eq!(result, 0, "foo/bar should not match memory");
    }

    #[test]
    fn test_pattern_does_not_match_foo_bar() {
        let stripped = r#"{"foo":"bar"}"#;
        let result = pattern::evaluate_raw(stripped);
        assert_eq!(result, 0, "foo/bar should not match pattern");
    }
}
