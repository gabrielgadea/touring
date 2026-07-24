//! Typed parameter extraction for the `cli-*` handlers.
//!
//! Every CLI handler has the signature
//! `fn(&mut HookRuntime, &serde_json::Value) -> String` and pulls typed fields
//! out of the JSON `payload`. Before this module those pulls were open-coded as
//! `payload.get("k").and_then(|v| v.as_str()).unwrap_or("")` (40×),
//! `payload.get("k").and_then(|v| v.as_u64()).unwrap_or(n)` (21×),
//! `… .unwrap_or(n) as usize` (19×), and so on — uniform boilerplate scattered
//! across ~20 handler files. These helpers collapse each shape into one readable
//! call while preserving the exact default semantics (absent OR wrong-type → the
//! stated default), so migrating a call site is behavior-identical.
//!
//! This is the genuine REGRA #0 value behind the report's "195 `cli_*` handler
//! dedup" item — the *dispatch* it imagined a `CliHandler` trait would enable is
//! already data-driven (a `HashMap<&str, fn(&mut HookRuntime, &Value) -> String>`
//! in `hook_registry.rs`), so a trait would be net boilerplate; the real
//! duplication lives in the per-handler field extraction, which these free
//! functions deduplicate without changing any handler signature.

use serde_json::Value;

/// String field, defaulting to `""` when absent or not a string.
#[must_use]
pub fn str_or_empty<'a>(payload: &'a Value, key: &str) -> &'a str {
    payload.get(key).and_then(Value::as_str).unwrap_or("")
}

/// String field with a caller-supplied default.
#[must_use]
pub fn str_or<'a>(payload: &'a Value, key: &str, default: &'a str) -> &'a str {
    payload.get(key).and_then(Value::as_str).unwrap_or(default)
}

/// Optional string field (`None` when absent or not a string).
#[must_use]
pub fn str_opt<'a>(payload: &'a Value, key: &str) -> Option<&'a str> {
    payload.get(key).and_then(Value::as_str)
}

/// `u64` field with a caller-supplied default.
#[must_use]
pub fn u64_or(payload: &Value, key: &str, default: u64) -> u64 {
    payload.get(key).and_then(Value::as_u64).unwrap_or(default)
}

/// `usize` field (read as `u64`, cast) with a caller-supplied default.
///
/// Mirrors the common `… .as_u64().unwrap_or(n) as usize` shape used for
/// `top_k`/`limit`/`depth`-style parameters.
#[must_use]
pub fn usize_or(payload: &Value, key: &str, default: usize) -> usize {
    payload
        .get(key)
        .and_then(Value::as_u64)
        .map_or(default, |n| n as usize)
}

/// `i64` field with a caller-supplied default.
#[must_use]
pub fn i64_or(payload: &Value, key: &str, default: i64) -> i64 {
    payload.get(key).and_then(Value::as_i64).unwrap_or(default)
}

/// `bool` field with a caller-supplied default.
#[must_use]
pub fn bool_or(payload: &Value, key: &str, default: bool) -> bool {
    payload.get(key).and_then(Value::as_bool).unwrap_or(default)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn str_helpers_default_and_extract() {
        let p = json!({ "name" : "alpha", "n" : 7 });
        assert_eq!(str_or_empty(&p, "name"), "alpha");
        assert_eq!(str_or_empty(&p, "missing"), "");
        assert_eq!(str_or_empty(&p, "n"), "");
        assert_eq!(str_or(&p, "missing", "def"), "def");
        assert_eq!(str_opt(&p, "name"), Some("alpha"));
        assert_eq!(str_opt(&p, "missing"), None);
    }
    #[test]
    fn numeric_helpers_default_and_extract() {
        let p = json!({ "u" : 10, "i" : - 3, "f" : 1.5, "b" : true });
        assert_eq!(u64_or(&p, "u", 0), 10);
        assert_eq!(u64_or(&p, "missing", 99), 99);
        assert_eq!(usize_or(&p, "u", 0), 10);
        assert_eq!(usize_or(&p, "missing", 5), 5);
        assert_eq!(i64_or(&p, "i", 0), -3);
        assert_eq!(i64_or(&p, "missing", -1), -1);
        assert!(bool_or(&p, "b", false));
        assert!(!bool_or(&p, "missing", false));
    }
}
