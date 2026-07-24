//! NEW-4 — User-defined filter DSL via TOML.
//!
//! Power-user extensibility: filters declared in `~/.config/touring/filters.toml`
//! take priority over the built-in 15-profile registry (NEW-1). Schema is
//! intentionally minimal so users can extend Touring's compression coverage
//! without rebuilding the Rust binary.
//!
//! ## TOML schema
//!
//! ```toml
//! [filter."cargo nextest"]
//! keep_lines_matching = ["^FAILED", "^PASS", "^test result:"]
//! strip_lines_matching = ["^running ", "^---"]
//! dedupe_consecutive = true
//! max_lines = 100
//!
//! [filter."my-internal-cli"]
//! keep_lines_matching = ["^error", "^warn"]
//! max_lines = 50
//! ```
//!
//! Loading: `load_user_filters()` reads `~/.config/touring/filters.toml`
//! once via OnceLock; `reload_user_filters()` invalidates the cache. Bad
//! filters log a warn and are skipped — graceful partial loading.
//!
//! Composition with NEW-1 in `compression_profiles::compress_for`:
//!   user_filter (this module) → built-in registry (NEW-1) → raw passthrough.

use std::borrow::Cow;
use std::path::PathBuf;
use std::sync::{OnceLock, RwLock};

use serde::Deserialize;

/// Single user filter parsed from TOML.
#[derive(Debug, Clone, Deserialize)]
pub struct UserFilter {
    /// Substring matched against `tool_args.command` (Bash) or tool_name.
    /// Stored as the TOML table key. Example: "cargo nextest".
    #[serde(skip)]
    pub tool_pattern: String,
    /// Lines kept when ANY pattern matches. Empty = keep all.
    #[serde(default)]
    pub keep_lines_matching: Vec<String>,
    /// Lines dropped when ANY pattern matches. Applied after keep_lines.
    #[serde(default)]
    pub strip_lines_matching: Vec<String>,
    /// Collapse consecutive identical lines with `(×N)` suffix.
    #[serde(default)]
    pub dedupe_consecutive: bool,
    /// Truncate output to first N lines + truncation marker.
    #[serde(default)]
    pub max_lines: Option<usize>,
}

/// Internal TOML schema: top-level table maps tool_pattern → UserFilter.
#[derive(Debug, Deserialize)]
struct FiltersToml {
    #[serde(default)]
    filter: std::collections::HashMap<String, RawFilter>,
}

#[derive(Debug, Clone, Deserialize)]
struct RawFilter {
    #[serde(default)]
    keep_lines_matching: Vec<String>,
    #[serde(default)]
    strip_lines_matching: Vec<String>,
    #[serde(default)]
    dedupe_consecutive: bool,
    #[serde(default)]
    max_lines: Option<usize>,
}

/// Parse a TOML string into a list of UserFilter.
///
/// Bad filters (regex compile errors are intentionally skipped at apply
/// time — keep_lines_matching uses `contains`, not regex, for simplicity
/// and zero-deps; future enhancement can opt into regex_lite).
pub fn parse_filters_toml(toml_str: &str) -> Result<Vec<UserFilter>, UserFilterError> {
    let parsed: FiltersToml =
        toml::from_str(toml_str).map_err(|e| UserFilterError::Parse(format!("toml parse: {e}")))?;
    let mut out: Vec<UserFilter> = Vec::with_capacity(parsed.filter.len());
    for (key, raw) in parsed.filter {
        out.push(UserFilter {
            tool_pattern: key,
            keep_lines_matching: raw.keep_lines_matching,
            strip_lines_matching: raw.strip_lines_matching,
            dedupe_consecutive: raw.dedupe_consecutive,
            max_lines: raw.max_lines,
        });
    }
    Ok(out)
}

/// Apply a single user filter to raw output.
pub fn apply_user_filter<'a>(filter: &UserFilter, raw: &'a str) -> Cow<'a, str> {
    // Step 1: keep_lines_matching (if non-empty, retain only matching lines)
    let kept: String = if filter.keep_lines_matching.is_empty() {
        raw.to_string()
    } else {
        raw.lines()
            .filter(|line| {
                filter
                    .keep_lines_matching
                    .iter()
                    .any(|p| line.contains(p.as_str()))
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    // Step 2: strip_lines_matching (drop lines matching any pattern)
    let stripped: String = if filter.strip_lines_matching.is_empty() {
        kept
    } else {
        kept.lines()
            .filter(|line| {
                !filter
                    .strip_lines_matching
                    .iter()
                    .any(|p| line.contains(p.as_str()))
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    // Step 3: dedupe consecutive
    let deduped: String = if filter.dedupe_consecutive {
        dedupe_consecutive(&stripped)
    } else {
        stripped
    };
    // Step 4: max_lines truncation
    let final_str = if let Some(max) = filter.max_lines {
        truncate_lines(&deduped, max)
    } else {
        deduped
    };
    Cow::Owned(final_str)
}

/// First-match-wins lookup: returns first user filter whose `tool_pattern`
/// is contained in either `tool_name` (e.g. "Bash" pattern) or in
/// `args.command` (e.g. "cargo nextest" pattern). Returns None when no
/// user filter matches.
pub fn find_matching_filter<'a>(
    filters: &'a [UserFilter],
    tool_name: &str,
    args: &serde_json::Value,
) -> Option<&'a UserFilter> {
    let cmd = args
        .get("command")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    filters.iter().find(|f| {
        let pat = f.tool_pattern.as_str();
        tool_name.contains(pat) || cmd.contains(pat)
    })
}

// ─── Helpers (mirror compression_profiles for consistency) ──────────────────

fn dedupe_consecutive(raw: &str) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut last: Option<String> = None;
    let mut count = 0u32;
    for line in raw.lines() {
        match &last {
            Some(prev) if prev == line => count += 1,
            _ => {
                if count > 1 {
                    let last_idx = out.len() - 1;
                    out[last_idx] = format!("{} (×{count})", out[last_idx]);
                }
                out.push(line.to_string());
                last = Some(line.to_string());
                count = 1;
            }
        }
    }
    if count > 1 {
        let last_idx = out.len() - 1;
        out[last_idx] = format!("{} (×{count})", out[last_idx]);
    }
    out.join("\n")
}

fn truncate_lines(raw: &str, max_lines: usize) -> String {
    let total = raw.lines().count();
    if total <= max_lines {
        return raw.to_string();
    }
    let kept: Vec<&str> = raw.lines().take(max_lines).collect();
    format!(
        "{}\n... (+{} more lines truncated by user filter)",
        kept.join("\n"),
        total - max_lines
    )
}

// ─── Global cache + load/reload ─────────────────────────────────────────────

static USER_FILTERS: OnceLock<RwLock<Vec<UserFilter>>> = OnceLock::new();

/// Returns the canonical path for the user filter TOML.
/// Honors `TOURING_USER_FILTERS_PATH` env override (test determinism).
pub fn user_filters_path() -> PathBuf {
    if let Ok(custom) = std::env::var("TOURING_USER_FILTERS_PATH") {
        return PathBuf::from(custom);
    }
    let mut path = std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/tmp"));
    path.push(".config/touring/filters.toml");
    path
}

/// Load user filters from disk; cache via OnceLock + RwLock.
/// Empty Vec on missing file (graceful default — user has no custom filters).
pub fn load_user_filters() -> Vec<UserFilter> {
    let lock = USER_FILTERS.get_or_init(|| {
        let filters = read_filters_from_disk();
        RwLock::new(filters)
    });
    lock.read().map(|g| g.clone()).unwrap_or_default()
}

/// Error from [`reload_user_filters`] (F-8 / RBP-03: typed in place of `String`).
#[derive(Debug, thiserror::Error)]
pub enum UserFilterError {
    /// The user-filters `RwLock` was poisoned by a panicking writer.
    #[error("write lock poisoned: {0}")]
    LockPoisoned(String),
    /// The user-filters TOML failed to parse (from [`parse_filters_toml`]).
    #[error("{0}")]
    Parse(String),
}

/// Re-read TOML from disk (invalidates cache via RwLock).
/// Used by `touring filters reload` CLI in NEW-4-Sprint5 follow-up.
pub fn reload_user_filters() -> Result<usize, UserFilterError> {
    let lock = USER_FILTERS.get_or_init(|| RwLock::new(read_filters_from_disk()));
    let new_filters = read_filters_from_disk();
    let count = new_filters.len();
    let mut guard = lock
        .write()
        .map_err(|e| UserFilterError::LockPoisoned(e.to_string()))?;
    *guard = new_filters;
    Ok(count)
}

fn read_filters_from_disk() -> Vec<UserFilter> {
    let path = user_filters_path();
    if !path.exists() {
        return Vec::new();
    }
    let content = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(target: "touring::user_filters", "read failed: {e}");
            return Vec::new();
        }
    };
    match parse_filters_toml(&content) {
        Ok(filters) => filters,
        Err(e) => {
            tracing::warn!(target: "touring::user_filters", "parse failed: {e}");
            Vec::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_parse_filters_toml_basic() {
        let toml = r#"
[filter."cargo nextest"]
keep_lines_matching = ["FAILED", "PASS"]
max_lines = 100
"#;
        let filters = parse_filters_toml(toml).expect("parse");
        assert_eq!(filters.len(), 1);
        let f = &filters[0];
        assert_eq!(f.tool_pattern, "cargo nextest");
        assert_eq!(f.keep_lines_matching, vec!["FAILED", "PASS"]);
        assert_eq!(f.max_lines, Some(100));
    }

    #[test]
    fn test_parse_filters_toml_multiple_filters() {
        let toml = r#"
[filter."cargo test"]
max_lines = 50

[filter."pytest"]
dedupe_consecutive = true
"#;
        let filters = parse_filters_toml(toml).expect("parse");
        assert_eq!(filters.len(), 2);
    }

    #[test]
    fn test_parse_filters_toml_invalid() {
        let bad = "[filter[not valid TOML";
        let result = parse_filters_toml(bad);
        assert!(result.is_err());
    }

    #[test]
    fn test_apply_keep_lines_filters_correctly() {
        let f = UserFilter {
            tool_pattern: "test".into(),
            keep_lines_matching: vec!["error".into()],
            strip_lines_matching: vec![],
            dedupe_consecutive: false,
            max_lines: None,
        };
        let raw = "info: starting\nerror: bad thing\nok: done\nerror: another";
        let out = apply_user_filter(&f, raw);
        assert!(out.contains("error: bad thing"));
        assert!(out.contains("error: another"));
        assert!(!out.contains("info: starting"));
        assert!(!out.contains("ok: done"));
    }

    #[test]
    fn test_apply_strip_lines_drops_matching() {
        let f = UserFilter {
            tool_pattern: "test".into(),
            keep_lines_matching: vec![],
            strip_lines_matching: vec!["DEBUG".into()],
            dedupe_consecutive: false,
            max_lines: None,
        };
        let raw = "INFO: ok\nDEBUG: noise\nWARN: heads up";
        let out = apply_user_filter(&f, raw);
        assert!(!out.contains("DEBUG"));
        assert!(out.contains("INFO"));
        assert!(out.contains("WARN"));
    }

    #[test]
    fn test_apply_dedupe_collapses() {
        let f = UserFilter {
            tool_pattern: "test".into(),
            keep_lines_matching: vec![],
            strip_lines_matching: vec![],
            dedupe_consecutive: true,
            max_lines: None,
        };
        let raw = "X\nX\nX\nY\n";
        let out = apply_user_filter(&f, raw);
        assert!(out.contains("(×3)"));
    }

    #[test]
    fn test_apply_max_lines_truncates() {
        let f = UserFilter {
            tool_pattern: "test".into(),
            keep_lines_matching: vec![],
            strip_lines_matching: vec![],
            dedupe_consecutive: false,
            max_lines: Some(5),
        };
        let raw = (0..20)
            .map(|i| format!("line{i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let out = apply_user_filter(&f, &raw);
        assert!(out.contains("(+15 more lines"));
    }

    #[test]
    fn test_find_matching_filter_by_tool_name() {
        let filters = vec![UserFilter {
            tool_pattern: "Sandbox".into(),
            keep_lines_matching: vec![],
            strip_lines_matching: vec![],
            dedupe_consecutive: false,
            max_lines: None,
        }];
        let m = find_matching_filter(&filters, "SandboxPython", &json!({"command": "echo hi"}));
        assert!(m.is_some());
    }

    #[test]
    fn test_find_matching_filter_by_command() {
        let filters = vec![UserFilter {
            tool_pattern: "cargo nextest".into(),
            keep_lines_matching: vec![],
            strip_lines_matching: vec![],
            dedupe_consecutive: false,
            max_lines: None,
        }];
        let m = find_matching_filter(&filters, "Bash", &json!({"command": "cargo nextest run"}));
        assert!(m.is_some());
    }

    #[test]
    fn test_find_matching_filter_returns_none_no_match() {
        let filters = vec![UserFilter {
            tool_pattern: "cargo bench".into(),
            keep_lines_matching: vec![],
            strip_lines_matching: vec![],
            dedupe_consecutive: false,
            max_lines: None,
        }];
        let m = find_matching_filter(&filters, "Bash", &json!({"command": "git status"}));
        assert!(m.is_none());
    }

    #[test]
    fn test_load_user_filters_missing_file_returns_empty() {
        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::set_var("TOURING_USER_FILTERS_PATH", "/tmp/nonexistent_xyz.toml") };
        // Force fresh re-read by reloading
        let _ = reload_user_filters();
        let filters = load_user_filters();
        assert!(filters.is_empty());
        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::remove_var("TOURING_USER_FILTERS_PATH") };
    }
}
