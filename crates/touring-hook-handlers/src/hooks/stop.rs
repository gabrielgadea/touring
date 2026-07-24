//! Stop Hook — captures session termination for Pensieve and session summary.
//!
//! Invoked when Claude Code receives a stop/suspend signal. Records:
//! - Tokens used (prompt + completion)
//! - Session duration and stop reason
//! - Session summary for future resume/replay (Pensieve)
//!
//! Target latency: <10ms.

use crate::runtime::{HookResponse, HookRuntime};
use touring_foundation::truncate_str;

/// FNV-1a hash of a byte slice into u64 — for Pensieve state hashing.
fn fnv1a_hash(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325; // FNV offset basis
    for &b in bytes {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3); // FNV prime
    }
    h
}

/// Session summary built by the stop hook for Pensieve persistence.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SessionSummary {
    /// Identifier of the session this summary describes.
    pub session_id: String,
    /// Reason the session was stopped or suspended.
    pub stop_reason: String,
    /// Number of prompt (input) tokens consumed during the session.
    pub prompt_tokens: u32,
    /// Number of completion (output) tokens consumed during the session.
    pub completion_tokens: u32,
    /// Total count of lifecycle hooks that fired during the session.
    pub total_hooks_fired: u32,
    /// Wall-clock duration of the session in milliseconds.
    pub duration_ms: u64,
}

impl SessionSummary {
    /// Build an empty summary for `session_id` with all counters zeroed.
    pub fn new(session_id: &str) -> Self {
        Self {
            session_id: session_id.to_string(),
            stop_reason: String::new(),
            prompt_tokens: 0,
            completion_tokens: 0,
            total_hooks_fired: 0,
            duration_ms: 0,
        }
    }
}

/// Run the stop hook (diverging version — for CLI entry point).
#[tracing::instrument(skip(runtime, input), fields(hook = "stop"))]
pub fn run(
    runtime: &mut HookRuntime,
    input: &serde_json::Value,
) -> Result<(), touring_hook_runtime::hook_runtime::HookDispatchError> {
    run_returning(runtime, input).emit()
}

/// Run the stop hook, returning a `HookResponse` instead of diverging.
///
/// Used by the daemon to handle the hook without calling `process::exit`.
pub fn run_returning(runtime: &mut HookRuntime, input: &serde_json::Value) -> HookResponse {
    let session_id = parse_session_id(input);
    let stop_reason = parse_stop_reason(input);
    let prompt_tokens = parse_prompt_tokens(input);
    let completion_tokens = parse_completion_tokens(input);
    let duration_ms = parse_duration_ms(input);

    // Build session summary
    let mut summary = SessionSummary::new(&session_id);
    summary.stop_reason = truncate_str(&stop_reason, 200).into();
    summary.prompt_tokens = prompt_tokens;
    summary.completion_tokens = completion_tokens;
    summary.duration_ms = duration_ms;

    // Extract hook firing count from ACO wiring state
    if let Ok(aco_guard) = runtime.aco_wiring.try_lock() {
        // Use deposit_file_edit call count as a proxy for total hooks fired
        // The bus tracks pheromone deposits; we approximate activity from that
        summary.total_hooks_fired = aco_guard
            .bus
            .get(
                &touring_intelligence::rl::aco::pheromone_bus::PheroKey::TaskId(
                    "session".to_string(),
                ),
            )
            .round() as u32;
    }

    // Persist session summary to Pensieve (ANN memory)
    persist_pensieve_summary(runtime, &summary);

    HookResponse::Allow
}

// ── Helper functions ─────────────────────────────────────────────────────────

/// Parse session ID from the stop payload.
fn parse_session_id(input: &serde_json::Value) -> String {
    input
        .pointer("/session_id")
        .and_then(|v| v.as_str())
        .or_else(|| {
            input
                .pointer("/tool_input/session_id")
                .and_then(|v| v.as_str())
        })
        .unwrap_or("unknown")
        .to_string()
}

/// Parse stop reason string.
fn parse_stop_reason(input: &serde_json::Value) -> String {
    input
        .pointer("/stop_reason")
        .and_then(|v| v.as_str())
        .or_else(|| input.pointer("/reason").and_then(|v| v.as_str()))
        .unwrap_or("unknown")
        .to_string()
}

/// Parse prompt token count.
fn parse_prompt_tokens(input: &serde_json::Value) -> u32 {
    input
        .pointer("/usage/prompt_tokens")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32
}

/// Parse completion token count.
fn parse_completion_tokens(input: &serde_json::Value) -> u32 {
    input
        .pointer("/usage/completion_tokens")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32
}

/// Parse session duration in milliseconds.
fn parse_duration_ms(input: &serde_json::Value) -> u64 {
    input
        .pointer("/duration_ms")
        .and_then(|v| v.as_u64())
        .unwrap_or(0)
}

/// Persist session summary to Pensieve ANN memory for future resume.
///
/// Records the session outcome as a failure entry keyed by session_id,
/// so future sessions can look up past performance.
fn persist_pensieve_summary(runtime: &mut HookRuntime, summary: &SessionSummary) {
    if let Ok(mut pensieve) = runtime.learning.pensieve.try_borrow_mut() {
        // Build state hashes from session metadata
        let states: Vec<u64> = vec![
            fnv1a_hash(summary.session_id.as_bytes()),
            fnv1a_hash(summary.stop_reason.as_bytes()),
        ];

        let reason = format!(
            "stop: {} tokens (p={}, c={}), {}ms, {} hooks",
            summary.session_id,
            summary.prompt_tokens,
            summary.completion_tokens,
            summary.duration_ms,
            summary.total_hooks_fired
        );

        let depth = states.len();
        pensieve.record_failure(&states, &reason, depth);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_session_id() {
        let input = serde_json::json!({
            "session_id": "sess_abc123"
        });
        assert_eq!(parse_session_id(&input), "sess_abc123");
    }

    #[test]
    fn test_parse_stop_reason() {
        let input = serde_json::json!({
            "stop_reason": "user_requested"
        });
        assert_eq!(parse_stop_reason(&input), "user_requested");
    }

    #[test]
    fn test_parse_tokens() {
        let input = serde_json::json!({
            "usage": {
                "prompt_tokens": 1500,
                "completion_tokens": 300
            }
        });
        assert_eq!(parse_prompt_tokens(&input), 1500);
        assert_eq!(parse_completion_tokens(&input), 300);
    }

    #[test]
    fn test_session_summary_new() {
        let summary = SessionSummary::new("test_session");
        assert_eq!(summary.session_id, "test_session");
        assert_eq!(summary.stop_reason, "");
        assert_eq!(summary.prompt_tokens, 0);
    }
}
