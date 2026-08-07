//! Post-Tool-Use RL Handler — processes immediate reward signals.
//!
//! Native Rust replacement for `post_tool_use_rl.py`. Runs after every tool
//! execution to update the RL models (QTable + LinUCB) via OnlineRLEngine.
//!
//! Performance: <1ms (vs ~40ms Python subprocess + PyO3 marshalling).
//!
//! P1-1 OPTIMIZATION: QTable persistence is batched (every N updates) to reduce
//! I/O overhead on the hot path. Uses a counter file to track pending writes.

use crate::action_signature::ActionSignature;
use crate::hook_decompose_bridge::bridge_post_tool_success;
use crate::runtime::HookRuntime;
use crate::schemas::validate_payload;
use touring_intelligence::rl::{ImmediateReward, QTable};
use touring_simd::cortex::Evidence;

/// Batch size for QTable persistence — save every N updates.
/// Reduces I/O from every tool execution to ~every 10 tools.
const QTABLE_BATCH_SIZE: u32 = 10;

/// Extract tool name, file path, output text, and error flag from PostToolUse payload.
///
/// Returns `(tool_name, file_path, tool_output, has_error)`.
fn extract_tool_metadata(input: &serde_json::Value) -> (String, &str, &str, bool) {
    let tool_name = input
        .pointer("/tool_name")
        .or_else(|| input.pointer("/tool_input/tool_name"))
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();

    let file_path = input
        .pointer("/tool_input/file_path")
        .or_else(|| input.pointer("/tool_input/path"))
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let tool_output = input
        .pointer("/tool_response/output")
        .or_else(|| input.pointer("/tool_output/output"))
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let has_error = input
        .pointer("/tool_response/error")
        .or_else(|| input.pointer("/tool_output/stderr"))
        .and_then(|v| v.as_str())
        .map(|s| !s.is_empty())
        .unwrap_or(false);

    (tool_name, file_path, tool_output, has_error)
}

/// Compute the three quality bonus components from runtime state.
///
/// Returns `(context_utility_bonus, aco_quality_bonus, gotcha_bonus)`.
fn compute_context_bonuses(
    runtime: &HookRuntime,
    file_path: &str,
    has_error: bool,
) -> (f64, f64, f64) {
    // S7: Context utility bonus — reinforce context injection when tool succeeded.
    let context_utility_bonus: f64 = if !has_error && !file_path.is_empty() {
        let injected_file = runtime
            .ctx
            .result_cache
            .get_result("__meta__", "__context_injection_file__");
        if injected_file.as_deref() == Some(file_path) {
            0.1
        } else {
            0.0
        }
    } else {
        0.0
    };

    // T3.13: ACO quality bonus — tracker composite score (10% weight).
    let aco_quality_bonus: f64 = runtime
        .ctx
        .quality_assessment
        .as_ref()
        .map(|a| {
            let session_turn = runtime
                .session_turn
                .load(std::sync::atomic::Ordering::Relaxed) as u32;
            a.to_tracker_report(session_turn).as_rl_reward() * 0.1
        })
        .unwrap_or(0.0);

    // T3.13: Gotcha prevention bonus — proportional to errors prevented, capped 0.1.
    let gotcha_bonus: f64 = {
        let (_total, _hits, prevented) = runtime.ctx.knowledge.gotcha_stats();
        if prevented > 0 {
            (prevented as f64 * 0.01).min(0.1)
        } else {
            0.0
        }
    };

    (context_utility_bonus, aco_quality_bonus, gotcha_bonus)
}

/// Persist QTable to disk if the batch threshold is reached; update the counter file.
///
/// Returns the new update count (used to gate LinUCB persistence on the same cadence).
pub(crate) fn persist_qtable_batched(runtime: &HookRuntime) -> u32 {
    let qtable_path = runtime.project_root.join(".claude/data/qtable.rkyv");
    let counter_path = runtime
        .project_root
        .join(".claude/data/qtable_update_count");

    let update_count: u32 = std::fs::read_to_string(&counter_path)
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0);
    let new_count = update_count + 1;

    if new_count >= QTABLE_BATCH_SIZE {
        if let Some(ref qt) = runtime.learning.qtable_cache {
            if let Err(e) = qt.save_rkyv(&qtable_path, 0) {
                tracing::warn!("Failed to save QTable: {e}");
            } else {
                tracing::debug!(count = new_count, "QTable batch persisted from cache");
            }
            // Also persist to graph.db for SQLite observability
            let graph_db =
                touring_foundation::TouringConfig::graph_db_canonical(&runtime.project_root);
            let persistence = touring_intelligence::rl::LearningPersistence::new(&graph_db);
            if let Err(e) = persistence.save_qtable(qt) {
                tracing::debug!("Failed to persist QTable to graph.db: {e}");
            }
        }
        let _ = std::fs::write(&counter_path, "0");
    } else {
        let _ = std::fs::write(&counter_path, new_count.to_string());
    }

    new_count
}

/// Record a Markov transition and update `last_tool_name` in runtime context.
fn record_markov_transition(runtime: &mut HookRuntime, cur_tool: &str) {
    if let Some(prev) = runtime.ctx.last_tool_name.clone() {
        runtime
            .learning
            .markov_predictor
            .record_transition(&prev, cur_tool);
    }
    runtime.ctx.last_tool_name = Some(cur_tool.to_string());
}

/// Feed the cognitive engine (SessionPredictor + SemanticGraph) after a tool use.
fn update_cognitive_engine(
    runtime: &HookRuntime,
    tool_name: &str,
    file_path: &str,
    has_error: bool,
) {
    if let Some(ref cognitive) = runtime.cognitive {
        cognitive
            .predictor()
            .record(touring_intelligence::reasoning::ToolInvocation {
                tool_name: tool_name.to_string(),
                timestamp_ms: chrono::Utc::now().timestamp_millis() as u64,
                success: !has_error,
            });
        if !file_path.is_empty() {
            cognitive.graph().touch(file_path);
        }
        tracing::debug!("cognitive predictor + graph updated");
    }
}

/// Run the post-tool-rl hook. Processes the PostToolUse event for RL learning.
///
/// Always returns Ok(()) — RL updates are best-effort and never block Claude Code.
#[tracing::instrument(skip(runtime, input), fields(hook = "post_tool_rl"))]
pub fn run(
    runtime: &mut HookRuntime,
    input: &serde_json::Value,
) -> Result<(), touring_hook_runtime::hook_runtime::HookDispatchError> {
    // D9: Validate payload with typed schema — skip on failure (non-blocking RL).
    if validate_payload::<crate::schemas::PostToolRlPayload>(input).is_err() {
        return Ok(());
    }
    tracing::info!(
        tool_name = extract_tool_metadata(input).0,
        has_input = !input.is_null(),
        "post-tool-rl hook fired"
    );
    // 1–4. Extract metadata from PostToolUse payload.
    let (tool_name, file_path, tool_output, has_error) = extract_tool_metadata(input);

    // Phase 1 Slice 1 — Action-scoped outcome persistence.
    // Compute ActionSignature and write a memory entry keyed by action class.
    // Fully fail-open: any error is swallowed — the hook always returns Ok(()).
    {
        let sig = ActionSignature::from_post_tool(&tool_name, file_path, has_error);
        let key = if has_error {
            format!("{}:failure", sig.to_key())
        } else {
            format!("{}:success", sig.to_key())
        };
        // Build a compact value (≤200 chars).
        let value: String = if has_error {
            let head: String = tool_output.chars().take(150).collect();
            format!("error in {}:{} — {}", tool_name, file_path, head)
        } else if !file_path.is_empty() {
            format!("success in {}:{}", tool_name, file_path)
        } else {
            format!("success in {}", tool_name)
        };
        let value = if value.len() > 200 {
            value[..200].to_string()
        } else {
            value
        };

        let _ = crate::cli_handlers::cli_memory_store(
            runtime,
            &serde_json::json!({
                "key": key,
                "value": value,
                "tier": "semantic",
                "type": "lesson",
            }),
        );
        tracing::debug!(key, has_error, "action_signature outcome persisted");

        // S-11 — feed the same outcome into the process-global online model, the
        // X4 PREDICT data source. Counts accumulate per feature class so a
        // brand-new signature inherits its class's success rate (beats the Beta
        // 0.5 cold-start). Fail-open: observe never panics.
        crate::gateway::outcome_learner::observe_global_outcome(
            &crate::gateway::outcome_learner::ActionFeatures::from_signature(&sig),
            !has_error,
        );
    }

    // Slice 3 Part 3: RL reward for injection quality.
    // If a lesson was injected by cli_suggester (pre-tool) AND the tool succeeded,
    // award a small positive signal to reinforce the injection pathway.
    // Fail-open: any error in get_result / inject_reward is silently ignored.
    {
        let was_injected = runtime
            .ctx
            .result_cache
            .get_result("__meta__", "__action_sig_lesson_injected__")
            .map(|v| v == "1")
            .unwrap_or(false);
        if was_injected {
            if !has_error {
                runtime.learning.inject_reward(
                    "context_injection_quality",
                    0.3,
                    "lesson_injected_and_succeeded",
                );
            }
            // Clear the one-shot flag regardless of outcome so the next tool
            // invocation starts with a clean slate.
            runtime.ctx.result_cache.cache_result(
                "__meta__",
                "__action_sig_lesson_injected__",
                String::new(),
            );
        }
    }

    let latency_ms = std::env::var("HOOK_ELAPSED_MS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(0);

    // P4.4: AutoSaveHook — increment exchange counter, fire if threshold reached.
    // Block-and-reason: auto-save must complete before RL processing continues.
    // Extract result first to avoid borrow conflict (immutable borrow in condition, mutable in body).
    let should_auto_save = runtime.auto_save.increment_exchange();
    if should_auto_save && let Err(e) = runtime.auto_save.run_auto_save(runtime) {
        tracing::warn!("AutoSaveHook checkpoint failed: {e}");
    }

    let cila_level = std::env::var("CILA_LEVEL")
        .ok()
        .and_then(|v| v.parse::<u8>().ok())
        .unwrap_or(2);
    let file_type = detect_file_type(file_path);
    let error_count = count_errors(tool_output);

    // 6. Compute quality bonuses and build ImmediateReward.
    let (ctx_bonus, aco_bonus, gotcha_bonus) =
        compute_context_bonuses(runtime, file_path, has_error);
    let combined_quality = ctx_bonus + aco_bonus + gotcha_bonus;

    let reward = ImmediateReward {
        tool_name: tool_name.clone(),
        accepted: !has_error,
        latency_ms,
        error_count,
        cila_level,
        file_type,
        quality_score: if combined_quality > 0.0 {
            Some(combined_quality)
        } else {
            None
        },
    };

    // F3-2: Emit Evidence to SessionBus broadcast channel for CortexDispatcher.
    // Evidence contains tool outcome and effectiveness for drift detection.
    let source_id = runtime.infra.cortex_dispatcher.next_source_id();
    let evidence = Evidence {
        source_id,
        value: if !has_error { 1.0 } else { 0.0 },
        confidence: 0.8,
        successes: if !has_error { 1 } else { 0 },
        total: 1,
    };
    runtime
        .ctx
        .session_bus
        .borrow()
        .broadcast_evidence(evidence);

    // 7. Feed to OnlineRLEngine (QTable + LinUCB) via in-memory cache.
    let qtable_path = runtime.project_root.join(".claude/data/qtable.rkyv");
    if runtime.learning.qtable_cache.is_none() {
        let mut qt = QTable::new();
        if qtable_path.exists()
            && let Ok((loaded, _rev)) = QTable::load_rkyv(&qtable_path)
        {
            qt = loaded;
        }
        runtime.learning.qtable_cache = Some(qt);
    }
    let mut qtable = runtime
        .learning
        .qtable_cache
        .take()
        .expect("qtable_cache initialized above");
    runtime.process_immediate_reward(&reward, &mut qtable);
    runtime.learning.qtable_cache = Some(qtable);

    // FA-3: Sync LinUCB arm effectiveness to SessionBus so pre_read can
    // query is_arm_productive(arm_id) to skip low-value arms.
    runtime.sync_arm_effectiveness();

    // FA-1: Record tool outcome to SessionBus (execution → learning path).
    // Enables post_tool_rl to correlate tool success with pre_read context injection.
    runtime
        .ctx
        .session_bus
        .borrow_mut()
        .signal_tool_outcome(!has_error, Some(combined_quality));

    // S4: Write RL feedback fields to SessionBus for cognitive engine drift detection
    // and outcome correlation. Extract from OnlineRLEngine and LearningRuntime.
    {
        let mut bus = runtime.ctx.session_bus.borrow_mut();
        // Extract EMA reward and TD error from OnlineRLEngine (requires &mut borrow)
        if let Some(engine) = runtime.learning.online_rl.take() {
            bus.last_ema_reward = Some(engine.ema_reward());
            bus.last_td_error = Some(engine.last_td_error());
            runtime.learning.online_rl = Some(engine);
        }
        // S4: Write last arm selected (already stored in LearningRuntime by select_context_strategy)
        bus.last_arm_selected = runtime.learning.last_arm_selected;
    }

    // F2-2: Process arm effectiveness from SessionBus and feed to AgenticRL learning phase.
    // Use last_arm_selected as the lookup key (valid arm IDs: 0-7).
    // Falls back to max effectiveness across all arms if no specific arm was selected.
    {
        let bus = runtime.ctx.session_bus.borrow();
        let arm_eff = bus
            .last_arm_selected
            .and_then(|arm_id| bus.arm_effectiveness.get(&arm_id).copied())
            .or_else(|| bus.arm_effectiveness.values().copied().reduce(f64::max));
        drop(bus); // Release borrow before mutating agentic_rl

        if let Some(eff) = arm_eff {
            let agentic = runtime.learning.agentic_rl_mut();
            let _ = agentic.update_learning_phase(eff);
        }
    }

    // W-1: Wire agentic_rl into post_tool_rl lifecycle.
    // Process reward through AgenticRL POMDP loop — records trajectory step
    // and triggers PPO policy update when episode buffer is full.
    {
        let agentic = runtime.learning.agentic_rl_mut();
        let (action, log_prob, value_estimate) = agentic.select_action();
        agentic.process_reward(&reward, action, log_prob, value_estimate);
        // Trigger PPO update when the learning phase is active and episodes are
        // available. FIXED (S-02 2026-05-29): the previous gate
        // `update_count() > 0 && score > 0.5` was a bootstrap DEADLOCK —
        // `update_count` only ever increments INSIDE `ppo_update`, so requiring
        // `update_count() > 0` to call it meant it could never fire from a cold
        // start and `update_count` stayed pinned at 0 forever.
        if agentic.learning_phase_score() > crate::agentic_rl::ACTIVATION_THRESHOLD
            && !agentic.episodes_is_empty()
        {
            let ppo_loss = agentic.ppo_update();
            tracing::debug!(ppo_loss, "agentic_rl PPO update completed");
        }
    }

    // S-02 (elite-harness 2026-05-29): persist AgenticRL state after a PPO
    // update has fired (update_count > 0), so intra-session learning survives a
    // daemon kill that happens between session-stop saves (update-touring,
    // crash). `load_agentic_rl` restores it on the next daemon start. Fail-open.
    if runtime
        .learning
        .agentic_rl
        .as_ref()
        .map(|a| a.update_count() > 0)
        .unwrap_or(false)
        && let Err(e) = runtime.save_agentic_rl()
    {
        tracing::debug!("agentic_rl periodic save failed (non-fatal): {e}");
    }

    // 8. Batch-persist QTable + LinUCB.
    let new_count = persist_qtable_batched(runtime);
    if new_count >= QTABLE_BATCH_SIZE {
        if let Err(e) = runtime.save_linucb() {
            tracing::warn!("Failed to save LinUCB: {e}");
        }
        // Also persist LinUCB to graph.db for SQLite observability
        let graph_db = touring_foundation::TouringConfig::graph_db_canonical(&runtime.project_root);
        let persistence = touring_intelligence::rl::LearningPersistence::new(&graph_db);
        if let Some(ref bandit) = runtime.learning.linucb
            && let Err(e) = persistence.save_linucb(bandit)
        {
            tracing::debug!("Failed to persist LinUCB to graph.db: {e}");
        }
    }

    // 9. Record Markov transition.
    record_markov_transition(runtime, &tool_name);

    // 10. Update cognitive engine.
    update_cognitive_engine(runtime, &tool_name, file_path, has_error);

    // HDG-3: Wire post_tool_rl success → decompose subtask completion.
    // Fire-and-forget: bridge errors are traced at debug level, never block.
    if !has_error {
        let subtask_id = input
            .pointer("/subtask_id")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty());
        if let Err(e) = bridge_post_tool_success(runtime, subtask_id, &tool_name) {
            tracing::debug!(error = %e, "bridge_post_tool_success fire-and-forget failed");
        }
    }

    tracing::debug!(
        accepted = !has_error,
        latency_ms,
        cila_level,
        file_type,
        error_count,
        "post-tool-rl processed"
    );

    Ok(())
}

/// Detect file type index from file path extension.
/// 0=python, 1=rust, 2=typescript, 3=other
fn detect_file_type(path: &str) -> u8 {
    if path.ends_with(".py") {
        0
    } else if path.ends_with(".rs") {
        1
    } else if path.ends_with(".ts")
        || path.ends_with(".tsx")
        || path.ends_with(".js")
        || path.ends_with(".jsx")
    {
        2
    } else {
        3
    }
}

/// Count error indicators in tool output.
fn count_errors(output: &str) -> u32 {
    if output.is_empty() {
        return 0;
    }
    // Only scan first ~2000 chars to avoid scanning huge outputs.
    // Use char boundary to avoid panic on multi-byte UTF-8.
    let head = if output.len() > 2000 {
        let mut end = 2000;
        while end > 0 && !output.is_char_boundary(end) {
            end -= 1;
        }
        &output[..end]
    } else {
        output
    };
    let mut count = 0u32;
    for pattern in &[
        "error",
        "Error",
        "ERROR",
        "traceback",
        "Traceback",
        "panic",
        "FAILED",
    ] {
        count += head.matches(pattern).count() as u32;
    }
    count.min(255) // cap to avoid overflow
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_file_type() {
        assert_eq!(detect_file_type("src/main.py"), 0);
        assert_eq!(detect_file_type("lib.rs"), 1);
        assert_eq!(detect_file_type("index.ts"), 2);
        assert_eq!(detect_file_type("app.tsx"), 2);
        assert_eq!(detect_file_type("bundle.js"), 2);
        assert_eq!(detect_file_type("component.jsx"), 2);
        assert_eq!(detect_file_type("README.md"), 3);
        assert_eq!(detect_file_type(""), 3);
    }

    #[test]
    fn test_count_errors_empty() {
        assert_eq!(count_errors(""), 0);
    }

    #[test]
    fn test_count_errors_single() {
        assert_eq!(count_errors("some error happened"), 1);
    }

    #[test]
    fn test_count_errors_multiple() {
        let output = "Error: failed\nTraceback: ...\npanic at line 42";
        assert_eq!(count_errors(output), 3);
    }

    #[test]
    fn test_count_errors_capped() {
        // Generate output with many error indicators
        let output = "error ".repeat(300);
        assert_eq!(count_errors(&output), 255);
    }

    #[test]
    fn test_count_errors_truncates_long_output() {
        // Create output > 2000 chars with errors only after position 2000
        let prefix = "a".repeat(2001);
        let output = format!("{prefix}error error error");
        // Errors are beyond the 2000 char window, should not count
        assert_eq!(count_errors(&output), 0);
    }
}
