//! Service layer — HTTP API calls from WASM client.
//!
//! In WASM context, these functions call the touring-web-server HTTP API endpoints
//! using the Fetch API via web-sys + js_sys::futures::JsFuture + serde_wasm_bindgen.
//!
//! On native (server), these are stub implementations that return empty/zero values.

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsValue;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen_futures::JsFuture;
#[cfg(target_arch = "wasm32")]
use web_sys::Response;

/// Async fetch JSON - works in WASM with Leptos async blocks.
#[cfg(target_arch = "wasm32")]
pub async fn fetch_json(url: &str) -> Result<serde_json::Value, String> {
    let window = web_sys::window().ok_or_else(|| "no window".to_string())?;
    let resp: Response = JsFuture::from(window.fetch_with_str(url))
        .await
        .map_err(|e| format!("fetch error: {:?}", e))?
        .unchecked_into();
    let js_val: JsValue = JsFuture::from(resp.json().map_err(|e| format!("json error: {:?}", e))?)
        .await
        .map_err(|e| format!("json await: {:?}", e))?;
    let val: serde_json::Value =
        serde_wasm_bindgen::from_value(js_val).map_err(|e| format!("parse error: {}", e))?;
    Ok(val)
}

/// Fetch text (non-JSON responses like SVG) - works in WASM with Leptos async blocks.
#[cfg(target_arch = "wasm32")]
pub async fn fetch_text(url: &str) -> Result<String, String> {
    let window = web_sys::window().ok_or_else(|| "no window".to_string())?;
    let resp: Response = JsFuture::from(window.fetch_with_str(url))
        .await
        .map_err(|e| format!("fetch error: {:?}", e))?
        .unchecked_into();
    let js_val: JsValue = JsFuture::from(resp.text().map_err(|e| format!("text error: {:?}", e))?)
        .await
        .map_err(|e| format!("text await: {:?}", e))?;
    let text: String =
        serde_wasm_bindgen::from_value(js_val).map_err(|e| format!("parse error: {}", e))?;
    Ok(text)
}

/// PUT a raw text body, returning the JSON response (Elite W3 —
/// `/api/quality/rules` editor save).
#[cfg(target_arch = "wasm32")]
pub async fn put_text(url: &str, body: &str) -> Result<serde_json::Value, String> {
    let window = web_sys::window().ok_or_else(|| "no window".to_string())?;
    let opts = web_sys::RequestInit::new();
    opts.set_method("PUT");
    opts.set_body(&JsValue::from_str(body));
    let request = web_sys::Request::new_with_str_and_init(url, &opts)
        .map_err(|e| format!("request error: {:?}", e))?;
    let resp: Response = JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|e| format!("fetch error: {:?}", e))?
        .unchecked_into();
    let js_val: JsValue = JsFuture::from(resp.json().map_err(|e| format!("json error: {:?}", e))?)
        .await
        .map_err(|e| format!("json await: {:?}", e))?;
    serde_wasm_bindgen::from_value(js_val).map_err(|e| format!("parse error: {}", e))
}

/// Non-WASM stub for [`put_text`].
#[cfg(not(target_arch = "wasm32"))]
pub async fn put_text(_url: &str, _body: &str) -> Result<serde_json::Value, String> {
    Ok(serde_json::json!({}))
}

/// Non-WASM stub: returns empty string.
#[cfg(not(target_arch = "wasm32"))]
pub async fn fetch_text(_url: &str) -> Result<String, String> {
    Ok(String::new())
}

/// Non-WASM stub: returns empty JSON object.
#[cfg(not(target_arch = "wasm32"))]
pub async fn fetch_json(_url: &str) -> Result<serde_json::Value, String> {
    Ok(serde_json::json!({}))
}

/// Fetch daemon health.
pub async fn fetch_health() -> Result<serde_json::Value, String> {
    fetch_json("/api/health").await
}

/// Fetch daemon status.
pub async fn fetch_status() -> Result<serde_json::Value, String> {
    fetch_json("/api/status").await
}

/// Fetch orphan symbols.
pub async fn fetch_orphans() -> Result<serde_json::Value, String> {
    fetch_json("/api/orphans").await
}

/// Search symbols.
pub async fn fetch_symbol_search(query: &str) -> Result<serde_json::Value, String> {
    fetch_json(&format!("/api/search?q={}", query)).await
}

/// Fetch wiring modules.
pub async fn fetch_wiring_modules() -> Result<serde_json::Value, String> {
    fetch_json("/api/wiring/modules").await
}

/// Fetch memory recall.
pub async fn fetch_memory(query: &str) -> Result<serde_json::Value, String> {
    fetch_json(&format!("/api/memory?q={}", query)).await
}

/// Fetch memory aggregate stats (`touring memory stats`) — feeds the
/// Memory page KPI strip (Wave 3 gap closure, 2026-06-11).
pub async fn fetch_memory_stats() -> Result<serde_json::Value, String> {
    fetch_json("/api/memory/stats").await
}

// ── Wave 4 (2026-06-12) — services for the new zip-artboard pages ──

/// Fetch the 142 live hook/gate counters (`touring gate-metrics`).
pub async fn fetch_gate_metrics() -> Result<serde_json::Value, String> {
    fetch_json("/api/gate-metrics").await
}

/// Fetch the session list (`touring session`).
pub async fn fetch_sessions() -> Result<serde_json::Value, String> {
    fetch_json("/api/sessions").await
}

/// Fetch global decompose totals (`touring decompose status`).
pub async fn fetch_decompose() -> Result<serde_json::Value, String> {
    fetch_json("/api/decompose").await
}

/// Fetch the W1-W10 workflow templates (`touring decompose templates`).
pub async fn fetch_decompose_templates() -> Result<serde_json::Value, String> {
    fetch_json("/api/decompose/templates").await
}

/// Fetch ready/blocked subtasks (`touring decompose ready`).
pub async fn fetch_decompose_ready() -> Result<serde_json::Value, String> {
    fetch_json("/api/decompose/ready").await
}

/// Fetch cognitive quality-delta metrics (`touring cognitive metrics`).
pub async fn fetch_cognitive() -> Result<serde_json::Value, String> {
    fetch_json("/api/cognitive").await
}

/// Fetch cognitive engine health map (`touring cognitive engines`).
pub async fn fetch_cognitive_engines() -> Result<serde_json::Value, String> {
    fetch_json("/api/cognitive/engines").await
}

/// Fetch wiring chains (`touring wiring chains`).
pub async fn fetch_wiring_chains() -> Result<serde_json::Value, String> {
    fetch_json("/api/wiring/chains").await
}

/// Fetch persisted quality snapshots (`/api/quality/history?limit=N`) —
/// polled by the Hooks Live page as its event feed (the SSE endpoint
/// `/api/events` feeds the same ring buffer server-side).
pub async fn fetch_quality_history(limit: usize) -> Result<serde_json::Value, String> {
    fetch_json(&format!("/api/quality/history?limit={limit}")).await
}

/// Fetch wiring SVG (server-rendered graphviz output).
pub async fn fetch_viz_svg() -> Result<String, String> {
    fetch_text("/api/viz/wiring/svg").await
}

/// Fetch workspace graph from `/api/viz/workspace`.
pub async fn fetch_viz_workspace_json() -> Result<serde_json::Value, String> {
    fetch_json("/api/viz/workspace").await
}

// ── Elite W2-W4 fetchers (SPEC 2026-06-12 §7.2) ──────────────────

/// Fetch RL learning state (`/api/learning/status`).
pub async fn fetch_learning_status() -> Result<serde_json::Value, String> {
    fetch_json("/api/learning/status").await
}

/// Fused memory recall (`/api/memory/recall?q=`).
pub async fn fetch_memory_recall(query: &str) -> Result<serde_json::Value, String> {
    fetch_json(&format!("/api/memory/recall?q={}", urlencode(query))).await
}

/// BFS blast radius for a symbol (`/api/wiring/impact?symbol=&depth=`).
pub async fn fetch_wiring_impact(symbol: &str, depth: u8) -> Result<serde_json::Value, String> {
    fetch_json(&format!(
        "/api/wiring/impact?symbol={}&depth={depth}",
        urlencode(symbol)
    ))
    .await
}

/// Wiring suggestions (`/api/wiring/suggest[?symbol=]`).
pub async fn fetch_wiring_suggest(symbol: Option<&str>) -> Result<serde_json::Value, String> {
    match symbol {
        Some(s) => fetch_json(&format!("/api/wiring/suggest?symbol={}", urlencode(s))).await,
        None => fetch_json("/api/wiring/suggest").await,
    }
}

/// One task DAG with subtasks (`/api/decompose/task?id=`).
pub async fn fetch_decompose_task(id: &str) -> Result<serde_json::Value, String> {
    fetch_json(&format!("/api/decompose/task?id={}", urlencode(id))).await
}

/// Session assessment detail (`/api/sessions/{id}`).
pub async fn fetch_session_detail(id: &str) -> Result<serde_json::Value, String> {
    fetch_json(&format!("/api/sessions/{}", urlencode(id))).await
}

/// Minimal percent-encoding for query/path values (space, &, ?, #, %, /).
pub fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            ' ' => out.push_str("%20"),
            '&' => out.push_str("%26"),
            '?' => out.push_str("%3F"),
            '#' => out.push_str("%23"),
            '%' => out.push_str("%25"),
            '/' => out.push_str("%2F"),
            '+' => out.push_str("%2B"),
            _ => out.push(c),
        }
    }
    out
}

/// POST a JSON body, returning the JSON response (Elite W4 —
/// `/api/mcp/call` whitelisted executor).
#[cfg(target_arch = "wasm32")]
pub async fn post_json(url: &str, body: &serde_json::Value) -> Result<serde_json::Value, String> {
    let window = web_sys::window().ok_or_else(|| "no window".to_string())?;
    let payload = serde_json::to_string(body).map_err(|e| format!("encode error: {e}"))?;
    let opts = web_sys::RequestInit::new();
    opts.set_method("POST");
    opts.set_body(&JsValue::from_str(&payload));
    let request = web_sys::Request::new_with_str_and_init(url, &opts)
        .map_err(|e| format!("request error: {:?}", e))?;
    request
        .headers()
        .set("Content-Type", "application/json")
        .map_err(|e| format!("header error: {:?}", e))?;
    let resp: Response = JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|e| format!("fetch error: {:?}", e))?
        .unchecked_into();
    let js_val: JsValue = JsFuture::from(resp.json().map_err(|e| format!("json error: {:?}", e))?)
        .await
        .map_err(|e| format!("json await: {:?}", e))?;
    serde_wasm_bindgen::from_value(js_val).map_err(|e| format!("parse error: {}", e))
}

/// Non-WASM stub for [`post_json`].
#[cfg(not(target_arch = "wasm32"))]
pub async fn post_json(_url: &str, _body: &serde_json::Value) -> Result<serde_json::Value, String> {
    Ok(serde_json::json!({}))
}

pub mod quality_client;
#[cfg(test)]
mod test_services;
pub use quality_client::*;
pub mod ws_client;
pub use ws_client::{connect_quality_ws, ws_request_refresh};
