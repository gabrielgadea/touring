//! WASM HTTP client functions for the dashboard endpoints.
//!
//! Aligns with `services/mod.rs` canonical pattern: `Result<_, String>` errors
//! and shared `super::fetch_json` (works on both wasm32 and native via cfg stubs).

use super::fetch_json;

/// GET /api/quality/signal
pub async fn fetch_quality_signal(
    root: Option<&str>,
    no_diagnostics: bool,
) -> Result<serde_json::Value, String> {
    let mut url = String::from("/api/quality/signal");
    let mut q: Vec<String> = Vec::new();
    if let Some(r) = root {
        q.push(format!("root={}", encode_uri(r)));
    }
    if no_diagnostics {
        q.push("no_diagnostics=1".into());
    }
    if !q.is_empty() {
        url.push('?');
        url.push_str(&q.join("&"));
    }
    fetch_json(&url).await
}

/// GET /api/quality/diff?prev=&curr=
pub async fn fetch_quality_diff(prev: &str, curr: &str) -> Result<serde_json::Value, String> {
    let url = format!(
        "/api/quality/diff?prev={}&curr={}",
        encode_uri(prev),
        encode_uri(curr),
    );
    fetch_json(&url).await
}

/// GET /api/quality/federation?workspaces=
pub async fn fetch_federation(spec: &str) -> Result<serde_json::Value, String> {
    let url = format!("/api/quality/federation?workspaces={}", encode_uri(spec));
    fetch_json(&url).await
}

/// GET /api/quality/history?limit=N — last N persisted snapshots, newest first.
pub async fn fetch_quality_history(limit: usize) -> Result<serde_json::Value, String> {
    let url = format!("/api/quality/history?limit={}", limit.min(1000));
    fetch_json(&url).await
}

/// Minimal portable URI percent-encoder (works on wasm32 and native).
fn encode_uri(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.as_bytes() {
        match b {
            b'A'..=b'Z'
            | b'a'..=b'z'
            | b'0'..=b'9'
            | b'-'
            | b'_'
            | b'.'
            | b'~'
            | b'/'
            | b':'
            | b',' => out.push(*b as char),
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}
