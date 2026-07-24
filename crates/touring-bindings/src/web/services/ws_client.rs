//! WebSocket client for the dashboard realtime channel (Wave 4 P10.8).
//!
//! Connects to `/ws/quality` and pipes parsed `serde_json::Value`
//! payloads into a Leptos `WriteSignal`. On wasm32 uses `web_sys::WebSocket`;
//! on native targets it is a stub so server-side rendering / cargo check
//! across the workspace stays green.

#[cfg(target_arch = "wasm32")]
use leptos::prelude::*;

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;
#[cfg(target_arch = "wasm32")]
use web_sys::{ErrorEvent, MessageEvent, WebSocket};

/// Connect to `/ws/quality` and feed every JSON message into `set`.
///
/// Returned handle keeps the socket alive — drop it to disconnect.
#[cfg(target_arch = "wasm32")]
pub fn connect_quality_ws(
    set: WriteSignal<Option<serde_json::Value>>,
) -> Result<WebSocket, JsValue> {
    let proto = if web_sys::window()
        .and_then(|w| w.location().protocol().ok())
        .map(|p| p == "https:")
        .unwrap_or(false)
    {
        "wss"
    } else {
        "ws"
    };
    let host = web_sys::window()
        .and_then(|w| w.location().host().ok())
        .unwrap_or_else(|| "localhost:3000".into());
    let url = format!("{}://{}/ws/quality", proto, host);

    let ws = WebSocket::new(&url)?;
    ws.set_binary_type(web_sys::BinaryType::Arraybuffer);

    // on-message: parse JSON and push into the signal.
    let on_msg = Closure::<dyn FnMut(_)>::new(move |evt: MessageEvent| {
        if let Some(text) = evt.data().as_string() {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) {
                set.set(Some(v));
            }
        }
    });
    ws.set_onmessage(Some(on_msg.as_ref().unchecked_ref()));
    on_msg.forget();

    let on_err = Closure::<dyn FnMut(_)>::new(|e: ErrorEvent| {
        web_sys::console::warn_1(&format!("ws error: {:?}", e.message()).into());
    });
    ws.set_onerror(Some(on_err.as_ref().unchecked_ref()));
    on_err.forget();

    Ok(ws)
}

/// Best-effort refresh signal — sends `{"action":"refresh"}` to the server
/// to bypass the interval and request an immediate snapshot.
#[cfg(target_arch = "wasm32")]
pub fn ws_request_refresh(ws: &WebSocket) {
    let _ = ws.send_with_str(r#"{"action":"refresh"}"#);
}

/// Native stub. Returns Err so accidental native callers fail fast.
#[cfg(not(target_arch = "wasm32"))]
pub fn connect_quality_ws(
    _set: leptos::prelude::WriteSignal<Option<serde_json::Value>>,
) -> Result<(), &'static str> {
    Err("WebSocket only available on wasm32 target")
}

/// Native stub.
#[cfg(not(target_arch = "wasm32"))]
pub fn ws_request_refresh(_ws: &()) {}
