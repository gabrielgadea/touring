//! ACP — Agent Client Protocol shim layer for touring-daemon socket.
//!
//! ACP (Agent Client Protocol) is the wire protocol defined by Zed Industries
//! for editor↔agent communication (similar to LSP but optimized for AI agents).
//! See: https://agentic-coding.com
//!
//! This module provides a **opt-in** protocol layer over the existing touring
//! daemon socket. When the `acp-protocol` feature is enabled, the daemon
//! peeks incoming bytes and routes them through this ACP shim:
//! - ACP messages: parsed as ACP Message envelope
//! - Legacy JSON: passed through unchanged
//!
//! ACP brings: capability negotiation, structured error taxonomy,
//! and correlation IDs for request/response matching.

/// ACP protocol version constant.
pub const PROTOCOL_VERSION: &str = "acp-1.0";

/// JSON-RPC 2.0 envelope for ACP messages.
///
/// Example:
/// ```json
/// {
///   "jsonrpc": "2.0",
///   "id": "req-001",
///   "method": "wiring.impact",
///   "params": { "symbol": "HookRuntime", "depth": 3 },
///   "correlation_id": null
/// }
/// ```
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Message {
    /// JSON-RPC version — always "2.0"
    #[serde(default = "jsonrpc_version")]
    pub jsonrpc: String,
    /// Unique request identifier (used in response matching)
    pub id: String,
    /// Method name (e.g., "wiring.impact", "wiring.cycles")
    pub method: String,
    /// Method parameters (arbitrary JSON)
    #[serde(default)]
    pub params: serde_json::Value,
    /// Optional correlation ID for chained requests
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub correlation_id: Option<String>,
}

fn jsonrpc_version() -> String {
    "2.0".to_string()
}

/// ACP response envelope.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Response {
    /// JSON-RPC version — always "2.0"
    #[serde(default = "jsonrpc_version")]
    pub jsonrpc: String,
    /// Correlates to the request `id`
    pub id: String,
    /// Result payload (method-specific)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub result: Option<serde_json::Value>,
    /// Error payload (when applicable)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub error: Option<ResponseError>,
}

/// ACP error object following JSON-RPC 2.0 spec.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ResponseError {
    /// Numeric error code (ACP taxonomy)
    pub code: i32,
    /// Human-readable error message
    pub message: String,
    /// Optional additional error context
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub data: Option<serde_json::Value>,
}

/// Server-reported capabilities for ACP negotiation.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Capabilities {
    /// ACP protocol version
    pub version: String,
    /// Supports streaming/chunked responses
    pub streaming: bool,
    /// Supports wiring.impact analysis
    pub impact_analysis: bool,
    /// Supports wiring.cycles detection
    pub cycle_detection: bool,
    /// Supports wiring.modules scoring
    pub modules: bool,
    /// Supports wiring.orphans detection
    pub orphans: bool,
    /// Supports wiring.chains functional chains
    pub chains: bool,
}

impl Default for Capabilities {
    fn default() -> Self {
        Self {
            version: PROTOCOL_VERSION.to_string(),
            streaming: false,
            impact_analysis: true,
            cycle_detection: true,
            modules: true,
            orphans: true,
            chains: true,
        }
    }
}

/// ACP error codes taxonomy.
pub mod errors {
    /// Invalid or malformed ACP message
    pub const E_INVALID_MESSAGE: i32 = -32700;
    /// Requested method not found / not supported
    pub const E_METHOD_NOT_FOUND: i32 = -32601;
    /// Invalid parameters for the method
    pub const E_INVALID_PARAMS: i32 = -32602;
    /// Internal server error during ACP processing
    pub const E_INTERNAL_ERROR: i32 = -32603;
    /// Server is busy (backpressure)
    pub const E_SERVER_BUSY: i32 = -32000;
    /// Capability not negotiated
    pub const E_CAPABILITY_NOT_NEGOTIATED: i32 = -32001;
    /// Streaming not supported for this method
    pub const E_STREAMING_NOT_SUPPORTED: i32 = -32002;
}

/// Detects if a byte slice starts with an ACP message envelope.
///
/// ACP messages start with `{"jsonrpc":` — no space after `{`.
/// Legacy touring JSON starts with `{"hook":` or `{"success":`.
pub fn detect_acp_payload(bytes: &[u8]) -> bool {
    // ACP msg: {"jsonrpc":  (no space after {)
    // touring legacy: {"hook": or {"success":
    let prefix = b"{\"jsonrpc\":";
    bytes.len() >= prefix.len() && &bytes[..prefix.len()] == prefix
}

/// Parse an ACP Message from a JSON string.
pub fn parse_message(json: &str) -> Option<Message> {
    serde_json::from_str(json).ok()
}

/// Serialize an ACP Response to JSON string.
pub fn serialize_response(resp: &Response) -> Result<String, serde_json::Error> {
    serde_json::to_string(resp)
}

/// Build a successful ACP response.
pub fn success_response(id: String, result: serde_json::Value) -> Response {
    Response {
        jsonrpc: "2.0".to_string(),
        id,
        result: Some(result),
        error: None,
    }
}

/// Build an error ACP response.
pub fn error_response(id: String, code: i32, message: &str) -> Response {
    Response {
        jsonrpc: "2.0".to_string(),
        id,
        result: None,
        error: Some(ResponseError {
            code,
            message: message.to_string(),
            data: None,
        }),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_acp_payload_valid() {
        let msg = r#"{"jsonrpc": "2.0", "id": "1", "method": "test"}"#;
        assert!(detect_acp_payload(msg.as_bytes()));
    }

    #[test]
    fn detect_acp_payload_rejects_legacy() {
        let legacy = r#"{"hook": "cli-wiring-orphans", "payload": {}}"#;
        assert!(!detect_acp_payload(legacy.as_bytes()));
    }

    #[test]
    fn detect_acp_payload_rejects_short() {
        assert!(!detect_acp_payload(b"{"));
        assert!(!detect_acp_payload(b"{ \"j"));
    }

    #[test]
    fn message_roundtrip() {
        let msg = Message {
            jsonrpc: "2.0".to_string(),
            id: "req-001".to_string(),
            method: "wiring.impact".to_string(),
            params: serde_json::json!({"symbol": "HookRuntime", "depth": 3}),
            correlation_id: None,
        };
        let json = serde_json::to_string(&msg).unwrap();
        let parsed = parse_message(&json).unwrap();
        assert_eq!(parsed.id, "req-001");
        assert_eq!(parsed.method, "wiring.impact");
    }

    #[test]
    fn response_success_roundtrip() {
        let resp = success_response(
            "req-001".to_string(),
            serde_json::json!({"direct_consumers": 12, "total_transitive": 47}),
        );
        let json = serialize_response(&resp).unwrap();
        let parsed: Response = serde_json::from_str(&json).unwrap();
        assert!(parsed.result.is_some());
        assert!(parsed.error.is_none());
    }

    #[test]
    fn response_error_roundtrip() {
        let resp = error_response(
            "req-001".to_string(),
            errors::E_METHOD_NOT_FOUND,
            "Method not found",
        );
        let json = serialize_response(&resp).unwrap();
        let parsed: Response = serde_json::from_str(&json).unwrap();
        assert!(parsed.result.is_none());
        let err = parsed.error.unwrap();
        assert_eq!(err.code, errors::E_METHOD_NOT_FOUND);
    }

    #[test]
    fn capabilities_default() {
        let caps = Capabilities::default();
        assert_eq!(caps.version, "acp-1.0");
        assert!(caps.impact_analysis);
        assert!(caps.cycle_detection);
    }
}
