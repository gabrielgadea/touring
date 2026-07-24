//! Adapter to query the GranularityBandit via daemon hook (Wave C2-D2, 2026-04-20).
//!
//! Queries the `cli-granularity-hint` daemon hook without linking
//! `touring-hooks` into `touring-server` (avoids a crate dependency cycle).
//! On any failure (daemon unreachable, parse error, timeout) the adapter
//! falls back to `GranularityHint::default()` (Monolithic, subtask_count=1)
//! so callers always receive a usable result.

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::time::Duration;

// ─── Public types ────────────────────────────────────────────────────────────

/// Result from a granularity hint query to the daemon.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GranularityHint {
    /// Variant name of `SplitFactor` (e.g. `"Monolithic"`, `"Split3"`).
    pub split_factor: String,
    /// How many subtasks the bandit recommends (mirrors `SplitFactor::subtask_count()`).
    pub subtask_count: usize,
    /// Echoes the input `size_loc` for traceability.
    pub size_loc: usize,
    /// Echoes the input `language` for traceability.
    pub language: String,
    /// Echoes the input `cila_level` for traceability.
    pub cila_level: u8,
}

impl Default for GranularityHint {
    fn default() -> Self {
        Self {
            split_factor: "Monolithic".to_string(),
            subtask_count: 1,
            size_loc: 100,
            language: "rust".to_string(),
            cila_level: 1,
        }
    }
}

// ─── Public API ──────────────────────────────────────────────────────────────

/// Query the daemon for a granularity split hint.
///
/// Sends a `cli-granularity-hint` hook request to the touring daemon and
/// returns the parsed result. Falls back to [`GranularityHint::default()`]
/// (Monolithic, subtask_count = 1) on any error so callers are never blocked
/// by daemon availability.
///
/// # Arguments
///
/// * `size_loc` — estimated lines-of-code for the task being split.
/// * `language` — source language (e.g. `"rust"`, `"python"`).
/// * `cila_level` — CILA complexity level (0–4).
pub fn query_granularity_hint(size_loc: usize, language: &str, cila_level: u8) -> GranularityHint {
    let payload = serde_json::json!({
        "size_loc": size_loc,
        "language": language,
        "cila_level": cila_level,
    });

    match query_daemon_hook("cli-granularity-hint", &payload.to_string()) {
        Ok(response) => parse_hint_response(&response, size_loc, language, cila_level),
        Err(_) => GranularityHint {
            split_factor: "Monolithic".to_string(),
            subtask_count: 1,
            size_loc,
            language: language.to_string(),
            cila_level,
        },
    }
}

// ─── Internal helpers ────────────────────────────────────────────────────────

/// Parse the daemon hook response JSON into a [`GranularityHint`].
///
/// On any parse failure returns a fallback that preserves the caller's input
/// values (`size_loc`, `language`, `cila_level`) so round-trip identity is
/// maintained even when the daemon response is malformed.
fn parse_hint_response(
    response: &str,
    size_loc: usize,
    language: &str,
    cila_level: u8,
) -> GranularityHint {
    // Fallback preserves caller inputs rather than using Default's constants.
    let fallback = || GranularityHint {
        split_factor: "Monolithic".to_string(),
        subtask_count: 1,
        size_loc,
        language: language.to_string(),
        cila_level,
    };

    let val: serde_json::Value = match serde_json::from_str(response) {
        Ok(v) => v,
        Err(_) => return fallback(),
    };

    GranularityHint {
        split_factor: val
            .get("split_factor")
            .and_then(|v| v.as_str())
            .unwrap_or("Monolithic")
            .to_string(),
        subtask_count: val
            .get("subtask_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(1) as usize,
        size_loc,
        language: language.to_string(),
        cila_level,
    }
}

/// Resolve the daemon Unix socket path.
///
/// W12.5 unification (2026-07-24): delegates to the foundation resolver
/// (canonical env → legacy env → per-project walk-up → global fallback). The
/// old local copy inverted the env precedence (legacy `TOURING_DAEMON_SOCK`
/// before the canonical `TOURING_DAEMON_SOCKET`) and skipped the walk-up.
fn daemon_socket_path() -> String {
    touring_foundation::config::TouringConfig::resolve_daemon_socket_path()
        .to_string_lossy()
        .into_owned()
}

/// Build the newline-delimited JSON request envelope for the daemon.
fn build_daemon_request(hook_name: &str, payload: &str, project_root: &str) -> String {
    let payload_val: serde_json::Value =
        serde_json::from_str(payload).unwrap_or(serde_json::Value::Null);
    let request = serde_json::json!({
        "hook": hook_name,
        "payload": payload_val,
        "project_root": project_root,
    });
    request.to_string() + "\n"
}

/// Open and configure a `UnixStream` connected to the daemon socket.
fn connect_daemon(socket_path: &str) -> Result<UnixStream, String> {
    let stream =
        UnixStream::connect(socket_path).map_err(|e| format!("connect {socket_path}: {e}"))?;
    stream
        .set_read_timeout(Some(Duration::from_millis(500)))
        .map_err(|e| format!("set_read_timeout: {e}"))?;
    stream
        .set_write_timeout(Some(Duration::from_secs(5)))
        .map_err(|e| format!("set_write_timeout: {e}"))?;
    Ok(stream)
}

/// Extract the `output` field from a `{"success": bool, "output": "..."}` wrapper.
fn extract_output(response: &str) -> Result<String, String> {
    let wrapper: serde_json::Value =
        serde_json::from_str(response).map_err(|e| format!("parse wrapper: {e}"))?;
    wrapper
        .get("output")
        .and_then(|v| v.as_str())
        .map(str::to_owned)
        .ok_or_else(|| "no output field in daemon response".to_string())
}

/// Send a hook query to the daemon and return the inner `output` string.
///
/// Wire format: newline-delimited JSON — identical to the format used by
/// `crates/touring-server/src/cli/mod.rs::send_daemon_request`.
///
/// The 500 ms read timeout (set in [`connect_daemon`]) prevents blocking the
/// calling thread when the daemon is slow or unavailable.
fn query_daemon_hook(hook_name: &str, payload: &str) -> Result<String, String> {
    let socket_path = daemon_socket_path();
    let project_root = std::env::current_dir()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    let request_str = build_daemon_request(hook_name, payload, &project_root);
    let mut stream = connect_daemon(&socket_path)?;

    stream
        .write_all(request_str.as_bytes())
        .map_err(|e| format!("write request: {e}"))?;

    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .map_err(|e| format!("read response: {e}"))?;

    extract_output(&response)
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn granularity_hint_default_is_monolithic() {
        let hint = GranularityHint::default();
        assert_eq!(hint.split_factor, "Monolithic");
        assert_eq!(hint.subtask_count, 1);
        assert_eq!(hint.cila_level, 1);
    }

    #[test]
    fn parse_hint_response_valid_split3() {
        let response = r#"{"split_factor":"Split3","subtask_count":3}"#;
        let hint = parse_hint_response(response, 500, "rust", 3);
        assert_eq!(hint.split_factor, "Split3");
        assert_eq!(hint.subtask_count, 3);
        assert_eq!(hint.size_loc, 500);
        assert_eq!(hint.language, "rust");
        assert_eq!(hint.cila_level, 3);
    }

    #[test]
    fn parse_hint_response_monolithic() {
        let response = r#"{"split_factor":"Monolithic","subtask_count":1}"#;
        let hint = parse_hint_response(response, 100, "python", 1);
        assert_eq!(hint.split_factor, "Monolithic");
        assert_eq!(hint.subtask_count, 1);
    }

    #[test]
    fn parse_hint_response_invalid_json_falls_back() {
        let hint = parse_hint_response("not json at all", 100, "rust", 1);
        assert_eq!(hint.split_factor, "Monolithic");
        assert_eq!(hint.subtask_count, 1);
    }

    #[test]
    fn parse_hint_response_missing_fields_falls_back_gracefully() {
        // Valid JSON but missing expected keys — should use defaults.
        let response = r#"{"unexpected_key": 42}"#;
        let hint = parse_hint_response(response, 200, "typescript", 2);
        assert_eq!(hint.split_factor, "Monolithic");
        assert_eq!(hint.subtask_count, 1);
        assert_eq!(hint.language, "typescript");
        assert_eq!(hint.size_loc, 200);
    }

    #[test]
    fn query_granularity_hint_returns_hint_or_fallback_without_panic() {
        // Without a running daemon the call must return a Monolithic fallback,
        // never panic or block indefinitely (500 ms timeout enforced).
        let hint = query_granularity_hint(200, "rust", 2);
        assert!(
            !hint.split_factor.is_empty(),
            "split_factor must not be empty"
        );
        assert!(hint.subtask_count >= 1, "subtask_count must be at least 1");
        assert_eq!(hint.size_loc, 200);
        assert_eq!(hint.language, "rust");
        assert_eq!(hint.cila_level, 2);
    }

    #[test]
    fn query_granularity_hint_various_languages() {
        // Ensure the function accepts various language strings without panic.
        for lang in &["python", "typescript", "go", "java"] {
            let hint = query_granularity_hint(300, lang, 1);
            assert_eq!(hint.language, *lang);
        }
    }
}
