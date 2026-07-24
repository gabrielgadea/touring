//! WASM HTTP service layer — calls touring-web-server API endpoints.

use wasm_bindgen::prelude::*;

#[wasm_bindgen(js_name = fetchJSON)]
pub async fn fetch_json(url: &str) -> Result<serde_value::Value, JsValue> {
    let window = web_sys::window().expect("no global window in browser context");
    let resp = window.fetch_with_str(url).await.map_err(|e| JsValue::from_str(&format!("fetch error: {:?}", e)))?;
    let json: serde_value::Value = resp.json().map_err(|e| JsValue::from_str(&format!("json error: {:?}", e)))?;
    Ok(json)
}

/// Fetch daemon health from `/api/health`.
pub async fn fetch_health() -> Result<serde_json::Value, JsValue> {
    let val: serde_value::Value = fetch_json("/api/health").await?;
    serde_json::to_value(val).map_err(|e| JsValue::from_str(&e.to_string()))
}

/// Fetch daemon status from `/api/status`.
pub async fn fetch_status() -> Result<serde_json::Value, JsValue> {
    let val: serde_value::Value = fetch_json("/api/status").await?;
    serde_json::to_value(val).map_err(|e| JsValue::from_str(&e.to_string()))
}

/// Fetch orphan symbols from `/api/orphans`.
pub async fn fetch_orphans() -> Result<serde_json::Value, JsValue> {
    let val: serde_value::Value = fetch_json("/api/orphans").await?;
    serde_json::to_value(val).map_err(|e| JsValue::from_str(&e.to_string()))
}

/// Search symbols from `/api/search`.
pub async fn fetch_symbol_search(query: &str) -> Result<serde_json::Value, JsValue> {
    let val: serde_value::Value = fetch_json(&format!("/api/search?q={}", query)).await?;
    serde_json::to_value(val).map_err(|e| JsValue::from_str(&e.to_string()))
}

/// Fetch wiring modules from `/api/wiring/modules`.
pub async fn fetch_wiring_modules() -> Result<serde_json::Value, JsValue> {
    let val: serde_value::Value = fetch_json("/api/wiring/modules").await?;
    serde_json::to_value(val).map_err(|e| JsValue::from_str(&e.to_string()))
}

/// Fetch memory recall from `/api/memory`.
pub async fn fetch_memory(query: &str) -> Result<serde_json::Value, JsValue> {
    let val: serde_value::Value = fetch_json(&format!("/api/memory?q={}", query)).await?;
    serde_json::to_value(val).map_err(|e| JsValue::from_str(&e.to_string()))
}

/// Fetch workspace graph from `/api/viz/workspace`.
pub async fn fetch_viz_workspace_json() -> Result<serde_json::Value, JsValue> {
    let val: serde_value::Value = fetch_json("/api/viz/workspace").await?;
    serde_json::to_value(val).map_err(|e| JsValue::from_str(&e.to_string()))
}
