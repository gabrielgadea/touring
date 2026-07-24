//! CLI ACP-protocol handlers (`cli_acp_*`) — extracted from cli_handlers.rs (A-W2.P3).
//!
//! Feature-gated behind `acp-protocol`. Dispatches to wiring handlers, which
//! live in `cli/wiring.rs` (re-exported from cli_handlers).

#[cfg(feature = "acp-protocol")]
use crate::cli_handlers::{
    cli_wiring_cycles, cli_wiring_impact, cli_wiring_modules, cli_wiring_orphans,
    cli_wiring_status, cli_wiring_suggest,
};
// Gated: only the `acp-protocol` fns reference HookRuntime; ungated it is
// "unused" and `cargo clippy --fix` (default features) strips it — keep the cfg.
#[cfg(feature = "acp-protocol")]
use crate::runtime::HookRuntime;

/// Dispatch an ACP (Agent Client Protocol) message and return the JSON response.
///
/// Routes the payload's `method` to the matching `crate::protocol::acp` handler;
/// returns an error JSON string when the method is unknown or handling fails.
#[cfg(feature = "acp-protocol")]
pub fn cli_acp_message(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    use crate::protocol::acp;
    let method = payload.get("method").and_then(|v| v.as_str()).unwrap_or("");
    let params = payload.get("params").unwrap_or(&serde_json::Value::Null);
    let id = payload
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();
    let result = match method {
        "wiring.impact" => cli_wiring_impact(rt, params),
        "wiring.cycles" => cli_wiring_cycles(rt, params),
        "wiring.status" => cli_wiring_status(rt, params),
        "wiring.orphans" => cli_wiring_orphans(rt, params),
        "wiring.modules" => cli_wiring_modules(rt, params),
        "wiring.suggest" => cli_wiring_suggest(rt, params),
        _ => {
            return acp::serialize_response(&acp::error_response(
                id,
                acp::errors::E_METHOD_NOT_FOUND,
                &format!("Unknown method: {method}"),
            ))
            .unwrap_or_default();
        }
    };
    let result_value: serde_json::Value =
        serde_json::from_str(&result).unwrap_or_else(|_| serde_json::json!({ "raw" : result }));
    let resp = acp::success_response(id, result_value);
    acp::serialize_response(&resp).unwrap_or_default()
}
/// Handle `cli-acp-discover` — return ACP protocol capabilities.
///
/// This allows ACP clients to query what methods and features the server
/// supports without having to hardcode capability assumptions.
///
/// Response format:
/// ```json
/// {
///   "jsonrpc": "2.0",
///   "id": "discover",
///   "result": {
///     "version": "acp-1.0",
///     "streaming": false,
///     "impact_analysis": true,
///     "cycle_detection": true,
///     "modules": true,
///     "orphans": true,
///     "chains": true
///   }
/// }
/// ```
#[cfg(feature = "acp-protocol")]
pub fn cli_acp_discover(_rt: &mut HookRuntime, _payload: &serde_json::Value) -> String {
    use crate::protocol::acp::{self, Capabilities};
    let caps = Capabilities::default();
    let resp = acp::success_response(
        "discover".to_string(),
        serde_json::to_value(caps).expect("Capabilities serializes to JSON value"),
    );
    acp::serialize_response(&resp).unwrap_or_default()
}
