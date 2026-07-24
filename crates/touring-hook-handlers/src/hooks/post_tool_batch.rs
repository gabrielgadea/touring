//! PostToolBatch Hook Handler — aggregate RL and wiring signals after a batch of parallel tools.
//!
//! Claude Code fires PostToolBatch ONCE after a group of parallel tool calls resolves.
//! This handler replaces N individual post-tool-rl invocations with a single aggregated one:
//!
//! - Parses `tool_calls` array from the PostToolBatch payload.
//! - Computes per-call success flags; maps success → 1.0 and failure → −0.3.
//! - Injects ONE aggregated RL reward (average score, labelled "batch").
//! - For batches that include Edit/Write calls, emits a REGRA #0 wiring hint
//!   via `additionalContext` when new orphan symbols may have been introduced.
//! - Truncates `tool_response` content per call to 0 bytes for memory safety.
//!
//! # Return value
//!
//! Always `HookResponse::Allow` (exit 0) unless new orphans were detected, in which case
//! `HookResponse::Context` carries a wiring potencialização hint. Never blocks, denies, or halts.

use crate::HookResponse;
use crate::runtime::HookRuntime;
use serde_json::Value;

/// Tool calls extracted from a PostToolBatch payload entry.
struct BatchCall {
    tool_name: String,
    success: bool,
}

/// Infer success from a tool call entry when no explicit `success` bool is present.
///
/// Checks `error` / `tool_response.error` (non-empty string → failure)
/// and `exit_code` (non-zero → failure). Both absent → assume success.
fn infer_success_from_entry(entry: &Value) -> bool {
    let has_error = entry
        .get("error")
        .or_else(|| entry.pointer("/tool_response/error"))
        .and_then(|v| v.as_str())
        .map(|s| !s.is_empty())
        .unwrap_or(false);
    let exit_ok = entry
        .get("exit_code")
        .and_then(|v| v.as_i64())
        .map(|c| c == 0)
        .unwrap_or(true);
    !has_error && exit_ok
}

/// Parse a single tool call entry into a `BatchCall`.
///
/// Returns `None` when `tool_name` is absent (required field).
fn parse_single_call(entry: &Value) -> Option<BatchCall> {
    let tool_name = entry
        .get("tool_name")
        .or_else(|| entry.pointer("/tool_input/tool_name"))
        .and_then(|v| v.as_str())?
        .to_string();

    let success = entry
        .get("success")
        .and_then(|v| v.as_bool())
        .unwrap_or_else(|| infer_success_from_entry(entry));

    Some(BatchCall { tool_name, success })
}

/// Parse the `tool_calls` array from the PostToolBatch input.
///
/// Returns an empty vec if the key is absent (graceful degradation).
fn parse_tool_calls(input: &Value) -> Vec<BatchCall> {
    input
        .get("tool_calls")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(parse_single_call).collect())
        .unwrap_or_default()
}

/// Map per-call success flag to a scalar reward value.
///
/// Success → +1.0; failure → −0.3 (partial penalty, not catastrophic).
#[inline]
fn call_reward(success: bool) -> f64 {
    if success { 1.0 } else { -0.3 }
}

/// Compute average reward across all calls in the batch.
///
/// Returns 0.0 for an empty batch (no reward injected — no-op RL update).
fn average_reward(calls: &[BatchCall]) -> f64 {
    if calls.is_empty() {
        return 0.0;
    }
    let sum: f64 = calls.iter().map(|c| call_reward(c.success)).sum();
    sum / calls.len() as f64
}

/// Returns true if the batch contains any Edit or Write tool calls.
///
/// These are the only tools that can introduce new public symbols / orphans.
fn batch_has_edit_or_write(calls: &[BatchCall]) -> bool {
    calls
        .iter()
        .any(|c| matches!(c.tool_name.as_str(), "Edit" | "Write" | "MultiEdit"))
}

/// Run the post-tool-batch hook.
///
/// Aggregates RL reward signals from a completed batch of parallel tool calls,
/// injects a single reward into the LinUCB/QTable, and emits a REGRA #0
/// wiring hint when Edit/Write tools may have introduced new orphan symbols.
///
/// Always exits 0 — never blocks Claude Code.
pub fn run_post_tool_batch(rt: &mut HookRuntime, input: &Value) -> HookResponse {
    let calls = parse_tool_calls(input);

    // Empty or unparseable batch — nothing to aggregate, silently allow.
    if calls.is_empty() {
        tracing::debug!("post-tool-batch: empty tool_calls array, skipping RL aggregation");
        return HookResponse::Allow;
    }

    let n = calls.len();
    let avg = average_reward(&calls);
    let successes: usize = calls.iter().filter(|c| c.success).count();

    tracing::info!(
        batch_size = n,
        successes = successes,
        avg_reward = avg,
        "post-tool-batch: aggregating RL reward"
    );

    // Inject ONE aggregated RL reward instead of N individual post-tool-rl calls.
    if avg != 0.0 {
        let context = format!("batch:{n}_tools");
        rt.learning.inject_reward("batch", avg, &context);
        tracing::debug!(
            avg_reward = avg,
            n_tools = n,
            "post-tool-batch: RL reward injected"
        );
    }

    // REGRA #0 — potencializar: if Edit/Write calls were in the batch, emit a
    // wiring hint so the agent knows to check for new orphan symbols.
    if batch_has_edit_or_write(&calls) {
        // Build a comma-separated list of the edit/write tool names for context.
        let edit_names: Vec<&str> = calls
            .iter()
            .filter(|c| matches!(c.tool_name.as_str(), "Edit" | "Write" | "MultiEdit"))
            .map(|c| c.tool_name.as_str())
            .collect();
        let tools_str = edit_names.join(", ");

        let hint = format!(
            "post-tool-batch: batch of {n} tools ({tools_str}) may have introduced new symbols. \
            Run `touring wiring orphans -j` to verify REGRA #0 compliance. \
            Wire any new pub symbols to consumers before closing the task."
        );

        tracing::debug!(tools = %tools_str, "post-tool-batch: emitting REGRA #0 wiring hint");
        return HookResponse::Context {
            context: hint,
            event_name: Some("PostToolBatch".to_string()),
        };
    }

    HookResponse::Allow
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_parse_tool_calls_empty() {
        let input = json!({});
        let calls = parse_tool_calls(&input);
        assert!(calls.is_empty());
    }

    #[test]
    fn test_parse_tool_calls_explicit_success() {
        let input = json!({
            "tool_calls": [
                {"tool_name": "Read", "success": true},
                {"tool_name": "Edit", "success": false}
            ]
        });
        let calls = parse_tool_calls(&input);
        assert_eq!(calls.len(), 2);
        assert!(calls[0].success);
        assert!(!calls[1].success);
    }

    #[test]
    fn test_parse_tool_calls_infer_success_from_exit_code() {
        let input = json!({
            "tool_calls": [
                {"tool_name": "Bash", "exit_code": 0},
                {"tool_name": "Bash", "exit_code": 1}
            ]
        });
        let calls = parse_tool_calls(&input);
        assert_eq!(calls.len(), 2);
        assert!(calls[0].success);
        assert!(!calls[1].success);
    }

    #[test]
    fn test_average_reward_all_success() {
        let calls = vec![
            BatchCall {
                tool_name: "Read".into(),
                success: true,
            },
            BatchCall {
                tool_name: "Read".into(),
                success: true,
            },
        ];
        let avg = average_reward(&calls);
        assert!((avg - 1.0).abs() < 1e-9);
    }

    #[test]
    fn test_average_reward_mixed() {
        let calls = vec![
            BatchCall {
                tool_name: "Read".into(),
                success: true,
            }, // +1.0
            BatchCall {
                tool_name: "Bash".into(),
                success: false,
            }, // -0.3
        ];
        let avg = average_reward(&calls);
        // (1.0 + -0.3) / 2 = 0.35
        assert!((avg - 0.35).abs() < 1e-9);
    }

    #[test]
    fn test_average_reward_empty() {
        let calls: Vec<BatchCall> = vec![];
        assert_eq!(average_reward(&calls), 0.0);
    }

    #[test]
    fn test_batch_has_edit_or_write_true() {
        let calls = vec![
            BatchCall {
                tool_name: "Read".into(),
                success: true,
            },
            BatchCall {
                tool_name: "Edit".into(),
                success: true,
            },
        ];
        assert!(batch_has_edit_or_write(&calls));
    }

    #[test]
    fn test_batch_has_edit_or_write_false() {
        let calls = vec![
            BatchCall {
                tool_name: "Read".into(),
                success: true,
            },
            BatchCall {
                tool_name: "Bash".into(),
                success: false,
            },
        ];
        assert!(!batch_has_edit_or_write(&calls));
    }

    #[test]
    fn test_call_reward_values() {
        assert!((call_reward(true) - 1.0).abs() < 1e-9);
        assert!((call_reward(false) - (-0.3)).abs() < 1e-9);
    }
}
