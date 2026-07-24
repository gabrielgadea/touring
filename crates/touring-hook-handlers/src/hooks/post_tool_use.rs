//! Post-Tool-Use Hook — captures tool outcomes for learning and ACO pheromone.
//!
//! Invoked AFTER each tool call completes. Records:
//! - Tool name, arguments, result, and duration
//! - ACO pheromone update via UnifiedPheromoneBus
//! - RL immediate reward via OnlineRLEngine (QTable + LinUCB)
//!
//! Target latency: <5ms (hot path).

use crate::runtime::{HookResponse, HookRuntime};
use crate::shared::hook_helpers;
use touring_intelligence::rl::aco::pheromone_bus::PheroKey;
use touring_intelligence::rl::ImmediateReward;

/// Run the post-tool-use hook (diverging version — for CLI entry point).
#[tracing::instrument(skip(runtime, input), fields(hook = "post_tool_use"))]
pub fn run(
    runtime: &mut HookRuntime,
    input: &serde_json::Value,
) -> Result<(), touring_hook_runtime::hook_runtime::HookDispatchError> {
    run_returning(runtime, input).emit()
}

/// Run the post-tool-use hook, returning a `HookResponse` instead of diverging.
///
/// Used by the daemon to handle the hook without calling `process::exit`.
pub fn run_returning(runtime: &mut HookRuntime, input: &serde_json::Value) -> HookResponse {
    let tool_name = parse_tool_name(input);
    let tool_output = parse_tool_output(input);
    let duration_ms = parse_duration_ms(input);
    let exit_code = parse_exit_code(input);

    if tool_name.is_empty() {
        return HookResponse::Allow;
    }

    let success = exit_code == 0 && !tool_output.contains("error");

    // ── Record ACO pheromone update ───────────────────────────────────────────
    record_aco_pheromone(runtime, &tool_name, success);

    // ── Update circuit breaker state ────────────────────────────────────────
    update_circuit_breaker(runtime, &tool_name, success);

    // ── Inject RL reward into OnlineRLEngine ─────────────────────────────────
    inject_rl_reward(runtime, &tool_name, success, duration_ms);

    // S3b: Update GoT pheromone trails with tool outcome.
    // Reinforcement on success, weak penalty on failure (post_tool_failure
    // already handles strong failure penalisation via pheromone_trails).
    if let Some(ref mut snapshot) = runtime.got_snapshot {
        let file_path = parse_file_path_from_input(input);
        if !file_path.is_empty() {
            let path_key = format!("tool:{}:{}", tool_name, file_path);
            let delta = if success { 0.1 } else { -0.05 };
            let entry = snapshot.pheromone_trails.entry(path_key).or_insert(0.0);
            *entry = (entry.max(0.0) + delta).max(0.0);
        }
    }

    // L7-B Gamma: Auto-trigger mandatory enrichment at CILA L4+ (self-modifying /
    // multi-agent workflows). At L4+, enrichment coherence is critical even for
    // PostToolUse observations — we mark the event for observability and ensure
    // the enrichment pipeline stays hot.
    // EC33: Read cila_level here so we can gate the cognitive enrichment below.
    let cila_level: u8 = hook_helpers::cila_level_from_runtime(runtime, 3);
    trigger_mandatory_enrichment_l4plus(runtime, &tool_name);

    // EC33: Cognitive enrichment at L4+ PostToolUse.
    // Fulfills the "used for cognitive wiring" purpose of parse_file_path_from_input (line ~118).
    // Only fires at CILA L4+ to avoid noise on every routine tool call.
    if crate::shared::cila::is_enrichment_mandatory(runtime.enrichment_active, cila_level) {
        let file_path = parse_file_path_from_input(input);
        if let Some(ref cognitive) = runtime.cognitive {
            let enriched =
                crate::shared::signals::enrich_with_cognitive(cognitive, &file_path, false);
            if !enriched.is_empty() {
                return HookResponse::Context {
                    context: enriched,
                    event_name: Some("PostToolUse".to_string()),
                };
            }
        }
    }

    HookResponse::Allow
}

/// L7-B Gamma: Check if L4+ mandatory enrichment applies and record the event.
///
/// At CILA L4 (self-modifying) or L6 (multi-agent), enrichment is mandatory
/// regardless of tool name. This hook point guarantees that PostToolUse
/// invocations in those high-order workflows are registered for observability
/// and that the `gate_metrics.post_tool_l4_mandatory_count` counter reflects
/// actual usage.
///
/// Side effects: increments `gate_metrics::record_post_tool_l4_mandatory()` and
/// emits a tracing::debug event. No expensive work — the actual enrichment was
/// already done at `pre_edit`/`pre_write` time. This is the accounting hook.
fn trigger_mandatory_enrichment_l4plus(runtime: &HookRuntime, tool_name: &str) {
    // Read cila_level using the canonical pattern (stable_session → result_cache fallback).
    let cila_level: u8 = hook_helpers::cila_level_from_runtime(runtime, 3);

    if crate::shared::cila::is_enrichment_mandatory(runtime.enrichment_active, cila_level) {
        crate::shared::gate_metrics::record_post_tool_l4_mandatory();
        tracing::debug!(
            cila_level,
            tool_name,
            enrichment_active = runtime.enrichment_active,
            "L7-B Gamma: L4+ mandatory enrichment triggered"
        );
    }
}

// ── Helper functions ─────────────────────────────────────────────────────────

/// Parse tool name from the hook input payload.
fn parse_tool_name(input: &serde_json::Value) -> String {
    input
        .pointer("/tool_name")
        .and_then(|v| v.as_str())
        .or_else(|| {
            input
                .pointer("/tool_input/tool_name")
                .and_then(|v| v.as_str())
        })
        .unwrap_or("")
        .to_string()
}

/// Parse file path from the tool input payload (used for cognitive wiring).
fn parse_file_path_from_input(input: &serde_json::Value) -> String {
    input
        .pointer("/tool_input/file_path")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

/// Parse tool output string.
fn parse_tool_output(input: &serde_json::Value) -> String {
    input
        .pointer("/tool_output/output")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

/// Parse duration in milliseconds from the tool execution.
fn parse_duration_ms(input: &serde_json::Value) -> u64 {
    input
        .pointer("/tool_output/duration_ms")
        .and_then(|v| v.as_u64())
        .or_else(|| {
            input.pointer("/tool_input/start_time").and_then(|start| {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64;
                start.as_u64().map(|s| now.saturating_sub(s))
            })
        })
        .unwrap_or(0)
}

/// Parse exit code from tool output.
fn parse_exit_code(input: &serde_json::Value) -> i64 {
    input
        .pointer("/tool_output/exit_code")
        .and_then(|v| v.as_i64())
        .unwrap_or(0)
}

/// Record ACO pheromone update based on tool outcome.
///
/// Updates the ACO wiring bus with the tool quality signal.
fn record_aco_pheromone(runtime: &mut HookRuntime, tool_name: &str, success: bool) {
    let Ok(aco_guard) = runtime.aco_wiring.try_lock() else {
        return;
    };

    let pheromone_key = PheroKey::TemplateId(tool_name.to_string());
    let signal = if success { 1.0 } else { -1.0 };

    aco_guard.bus.deposit(pheromone_key, signal);
}

/// Update circuit breaker failure/success counter.
fn update_circuit_breaker(runtime: &mut HookRuntime, tool_name: &str, success: bool) {
    let failure_key = format!("{}_failure_count", tool_name);

    if success {
        // Reset failure count on success
        runtime
            .ctx
            .result_cache
            .cache_result("__circuit__", &failure_key, "0".to_string());
    } else {
        // Increment failure count
        let count: u32 = runtime
            .ctx
            .result_cache
            .get_result("__circuit__", &failure_key)
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        let new_count = count.saturating_add(1);
        runtime
            .ctx
            .result_cache
            .cache_result("__circuit__", &failure_key, new_count.to_string());
    }
}

/// Inject RL immediate reward into OnlineRLEngine via QTable.
fn inject_rl_reward(runtime: &mut HookRuntime, tool_name: &str, success: bool, latency_ms: u64) {
    let cila_level = runtime
        .ctx
        .result_cache
        .get_result("__meta__", "__session_cila_level__")
        .and_then(|s| s.parse::<u8>().ok())
        .unwrap_or(2);

    let reward = ImmediateReward {
        tool_name: tool_name.to_string(),
        accepted: success,
        latency_ms,
        error_count: if success { 0 } else { 1 },
        cila_level,
        file_type: detect_file_type(tool_name),
        quality_score: None,
    };

    // Load QTable from cache or disk, update, and return to cache
    hook_helpers::ensure_qtable_loaded(runtime);

    if let Some(mut qtable) = runtime.learning.qtable_cache.take() {
        runtime.process_immediate_reward(&reward, &mut qtable);
        runtime.learning.qtable_cache = Some(qtable);
    }
}

/// Detect file type from tool name heuristics.
fn detect_file_type(tool_name: &str) -> u8 {
    match tool_name {
        "Read" | "Edit" | "Write" => 1, // code file
        "Bash" => 3,                    // other/shell
        _ => 3,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_tool_name() {
        let input = serde_json::json!({
            "tool_name": "Read",
            "tool_input": {"file_path": "/tmp/foo"}
        });
        assert_eq!(parse_tool_name(&input), "Read");
    }

    #[test]
    fn test_parse_exit_code_success() {
        let input = serde_json::json!({
            "tool_output": {"exit_code": 0}
        });
        assert_eq!(parse_exit_code(&input), 0);
    }

    #[test]
    fn test_parse_exit_code_failure() {
        let input = serde_json::json!({
            "tool_output": {"exit_code": 1}
        });
        assert_eq!(parse_exit_code(&input), 1);
    }

    #[test]
    fn test_detect_file_type() {
        assert_eq!(detect_file_type("Read"), 1);
        assert_eq!(detect_file_type("Edit"), 1);
        assert_eq!(detect_file_type("Write"), 1);
        assert_eq!(detect_file_type("Bash"), 3);
    }
}
