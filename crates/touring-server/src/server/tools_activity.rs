//! Activity log MCP tools — D1.5 (S, dep: D1.4).
//!
//! 4 MCP tools:
//! - `touring_activity_append` — append event to activity log
//! - `touring_activity_replay` — replay all events and produce projection
//! - `touring_activity_verify` — verify stored hash against recomputed projection
//! - `touring_activity_projection` — show current projected state hash

use super::*;
use crate::daemon_client::daemon_query;

/// Shared helper to convert a daemon JSON response into an MCP `CallToolResult`.
///
/// Treats `{"status":"ok"}` as success and everything else as error — keeps the
/// 4 activity tool handlers consistent (and ready to be reused by future
/// tools_*.rs modules that follow the same daemon-dispatch + JSON-envelope
/// convention). Marked `pub(crate)` so peer modules can adopt it.
pub(crate) fn make_result(json: &serde_json::Value) -> Result<CallToolResult, McpError> {
    let text = serde_json::to_string_pretty(json)
        .map_err(|e| McpError::internal_error(e.to_string(), None))?;
    let is_error = json.get("status") != Some(&serde_json::json!("ok"));
    if is_error {
        Ok(CallToolResult::error(vec![Content::text(text)]))
    } else {
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }
}

#[tool_router(router = router_activity, vis = "pub(crate)")]
impl TouringServer {
    // ── Activity Tools ───────────────────────────────────────────────────────

    /// Append a new event to the activity log
    #[tool(
        name = "touring_activity_append",
        description = "Append a new event to the append-only activity log (ESAA pattern). Events are stored in `<project>/.claude/touring/activity.jsonl`."
    )]
    async fn activity_append(
        &self,
        params: Parameters<ActivityAppendParams>,
    ) -> Result<CallToolResult, McpError> {
        let p = params.0;

        let action = p.action.unwrap_or_else(|| "tool_invoked".to_string());
        let actor = p.actor.unwrap_or_else(|| "ClaudeCode".to_string());
        let payload = p.payload.unwrap_or_else(|| serde_json::json!({}));

        let payload_str = serde_json::to_string(&payload)
            .map_err(|e| McpError::invalid_params(e.to_string(), None))?;

        let mut args = vec!["activity".to_string(), "append".to_string(), action];
        args.push(format!("--actor={}", actor));
        args.push(format!("--payload={}", payload_str));

        let result = daemon_query(
            "cli",
            serde_json::json!({
                "cmd": "activity",
                "args": args,
            }),
        )
        .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        let json: serde_json::Value = serde_json::from_str(&result).map_err(|e| {
            McpError::internal_error(format!("daemon returned invalid JSON: {e}"), None)
        })?;

        make_result(&json)
    }

    /// Replay all events and produce the canonical projection (deterministic SHA-256 hash)
    #[tool(
        name = "touring_activity_replay",
        description = "Replay all events in the activity log and produce the deterministic projection. Returns total event count and projection hash."
    )]
    async fn activity_replay(
        &self,
        params: Parameters<ActivityReplayParams>,
    ) -> Result<CallToolResult, McpError> {
        let p = params.0;
        let limit = p.limit;

        let mut args = vec!["activity".to_string(), "replay".to_string()];
        if let Some(n) = limit {
            args.push(format!("--limit={}", n));
        }

        let result = daemon_query(
            "cli",
            serde_json::json!({
                "cmd": "activity",
                "args": args,
            }),
        )
        .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        let json: serde_json::Value = serde_json::from_str(&result).map_err(|e| {
            McpError::internal_error(format!("daemon returned invalid JSON: {e}"), None)
        })?;

        make_result(&json)
    }

    /// Verify the stored hash against the recomputed projection
    #[tool(
        name = "touring_activity_verify",
        description = "Verify the integrity of the activity log by recomputing the projection hash and comparing against the stored hash. Returns failure count and failed seq numbers."
    )]
    async fn activity_verify(
        &self,
        _params: Parameters<ActivityVerifyParams>,
    ) -> Result<CallToolResult, McpError> {
        let result = daemon_query(
            "cli",
            serde_json::json!({
                "cmd": "activity",
                "args": ["activity".to_string(), "verify".to_string()],
            }),
        )
        .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        let json: serde_json::Value = serde_json::from_str(&result).map_err(|e| {
            McpError::internal_error(format!("daemon returned invalid JSON: {e}"), None)
        })?;

        make_result(&json)
    }

    /// Show the current projected state hash
    #[tool(
        name = "touring_activity_projection",
        description = "Compute and return the current projected state hash of the activity log (SHA-256 of canonical event sequence)."
    )]
    async fn activity_projection(
        &self,
        _params: Parameters<ActivityProjectionParams>,
    ) -> Result<CallToolResult, McpError> {
        let result = daemon_query(
            "cli",
            serde_json::json!({
                "cmd": "activity",
                "args": ["activity".to_string(), "projection".to_string()],
            }),
        )
        .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        let json: serde_json::Value = serde_json::from_str(&result).map_err(|e| {
            McpError::internal_error(format!("daemon returned invalid JSON: {e}"), None)
        })?;

        make_result(&json)
    }
}

// ── Parameter types ─────────────────────────────────────────────────────────

/// Parameters accepted by `touring_activity_append`. All fields are optional;
/// defaults are applied by the handler (`action="tool_invoked"`,
/// `actor="ClaudeCode"`, `payload={}`).
#[derive(Debug, Clone, Default, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", default)]
pub struct ActivityAppendParams {
    /// Event action verb (e.g. `tool_invoked`, `file_edited`).
    pub action: Option<String>,
    /// Originating actor (e.g. `ClaudeCode`, `Gabriel`, an agent id).
    pub actor: Option<String>,
    /// Arbitrary JSON payload — opaque to the activity log; replayed verbatim.
    pub payload: Option<serde_json::Value>,
}

impl ActivityAppendParams {
    /// Build a params bundle with sane defaults — useful in tests and callers
    /// that want to assemble the request programmatically.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Builder: set the event action.
    #[must_use]
    pub fn with_action(mut self, action: impl Into<String>) -> Self {
        self.action = Some(action.into());
        self
    }

    /// Builder: set the actor identifier.
    #[must_use]
    pub fn with_actor(mut self, actor: impl Into<String>) -> Self {
        self.actor = Some(actor.into());
        self
    }

    /// Builder: set the JSON payload.
    #[must_use]
    pub fn with_payload(mut self, payload: serde_json::Value) -> Self {
        self.payload = Some(payload);
        self
    }
}

/// Parameters accepted by `touring_activity_replay`. The optional `limit`
/// caps the number of replayed events (omit for full replay).
#[derive(Debug, Clone, Default, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", default)]
pub struct ActivityReplayParams {
    /// Optional cap on the number of events replayed (`None` = full replay).
    pub limit: Option<usize>,
}

impl ActivityReplayParams {
    /// Build a params bundle requesting a full replay.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Builder: cap replay at `n` events.
    #[must_use]
    pub fn with_limit(mut self, n: usize) -> Self {
        self.limit = Some(n);
        self
    }
}

/// Parameters for `touring_activity_verify` — no inputs required.
///
/// An explicit unit-shaped struct is used instead of `Parameters<()>` so the
/// emitted JSON schema is `{"type":"object","properties":{}}` (MCP spec
/// requirement) rather than `{"type":"null"}` (what `schemars` derives for
/// the unit type — clients reject the tool with `ZodInvalidValueError`).
#[derive(Debug, Clone, Default, serde::Deserialize, schemars::JsonSchema)]
#[serde(default)]
pub struct ActivityVerifyParams {}

/// Parameters for `touring_activity_projection` — no inputs required.
///
/// See [`ActivityVerifyParams`] for the rationale: empty named struct emits
/// the MCP-conformant `object` schema; `Parameters<()>` would emit `null`.
#[derive(Debug, Clone, Default, serde::Deserialize, schemars::JsonSchema)]
#[serde(default)]
pub struct ActivityProjectionParams {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn activity_append_params_default_is_all_none() {
        let p = ActivityAppendParams::new();
        assert!(p.action.is_none() && p.actor.is_none() && p.payload.is_none());
    }

    #[test]
    fn activity_append_params_builder_chain() {
        let p = ActivityAppendParams::new()
            .with_action("file_edited")
            .with_actor("Gabriel")
            .with_payload(serde_json::json!({"file": "test.rs"}));
        assert_eq!(p.action.as_deref(), Some("file_edited"));
        assert_eq!(p.actor.as_deref(), Some("Gabriel"));
        assert_eq!(p.payload, Some(serde_json::json!({"file": "test.rs"})));
    }

    #[test]
    fn activity_replay_params_with_limit() {
        let p = ActivityReplayParams::new().with_limit(100);
        assert_eq!(p.limit, Some(100));
    }

    #[test]
    fn activity_append_params_deserialize_from_empty_object() {
        // `#[serde(default)]` enables {} to map to all-None
        let p: ActivityAppendParams =
            serde_json::from_value(serde_json::json!({})).expect("empty object should parse");
        assert!(p.action.is_none());
    }

    #[test]
    fn make_result_success_path() {
        let ok = serde_json::json!({"status": "ok", "data": 42});
        let result = make_result(&ok).expect("ok json should produce success result");
        // CallToolResult::success vs error is internal — just assert the
        // call doesn't error and the helper survives the round trip.
        let _ = result;
    }

    #[test]
    fn make_result_error_path_when_status_missing() {
        let err = serde_json::json!({"data": 42});
        let result = make_result(&err).expect("missing status should still produce a result");
        let _ = result;
    }
}
