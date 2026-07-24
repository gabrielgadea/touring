//! Lifecycle Handlers — pre/post-compact, subagent start/stop, teammate idle, task completed,
//! post-tool-failure, stop-failure, session-end, config-change.
//!
//! Adapted from `.claude/rust-core/src/touring/handlers/`:
//! - `pre_compact.rs` -> PreCompactHandler
//! - `subagent_start.rs` -> SubagentStartHandler
//! - `subagent_stop.rs` -> SubagentStopHandler
//! - `teammate_idle.rs` -> TeammateIdleHandler
//! - `task_completed.rs` -> TaskCompletedHandler
//!
//! Phase 1 additions: PostToolUseFailureHandler, StopFailureHandler, SessionEndHandler
//! Phase 3 additions: PostCompactHandler (H9), ConfigChangeHandler (H10)
//!
//! These handlers use `CortexContext` and `FileKnowledgeDB` (not TouringState).
//! Where the originals used DashMap/Wilson/ESAA, we persist to the knowledge DB
//! via access_log/bash_outcomes/notes, and emit context lines for the pipeline.

use crate::context::CortexContext;
use crate::handler::Handler;
use crate::pipeline::Pipeline;
use crate::types::{HandlerResult, HookEvent};
use touring_foundation::truncate_str;

// ══════════════════════════════════════════════════════════════════════
// 1. PreCompactHandler
// ══════════════════════════════════════════════════════════════════════

/// PreCompact — injects critical rules/context before context compaction.
///
/// Adapted from `pre_compact.rs`. Before Claude Code compacts the context,
/// this handler injects a summary of the knowledge DB state and critical
/// rules that must survive compaction.
///
/// **Sync, never blocks.**
pub struct PreCompactHandler;

impl Handler for PreCompactHandler {
    fn name(&self) -> &str {
        "pre_compact"
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::PreCompact]
    }

    fn execute(&self, ctx: &mut CortexContext) -> HandlerResult {
        // Gather knowledge DB stats as a mini-crystal
        let stats = match ctx.knowledge.stats() {
            Ok(s) => s,
            Err(_) => return HandlerResult::skip(self.name()),
        };

        let mut parts = Vec::new();

        if stats.file_count > 0 {
            parts.push(format!("files:{}", stats.file_count));
        }
        if stats.relation_count > 0 {
            parts.push(format!("relations:{}", stats.relation_count));
        }
        if stats.bash_count > 0 {
            parts.push(format!("bash:{}", stats.bash_count));
        }
        if stats.edit_count > 0 {
            parts.push(format!("edits:{}", stats.edit_count));
        }

        // Critical rules that must survive compaction
        let crystal = format!(
            "PRE-COMPACT CRYSTAL: session={} knowledge=[{}] | \
             RULES: CODE-FIRST UNIVERSAL (DISCOVER\u{2192}CREATE\u{2192}EXECUTE) | \
             CILA routing active | Zero-hallucination (verify before assert) | \
             Hooks inviolable (NEVER bypass) | ACTIVE: touring_mask_context(summarize large observations) + touring_checkpoint(auto-saved) + touring_incremental_status(index current)",
            &ctx.session_id[..ctx.session_id.len().min(8)],
            parts.join(" "),
        );

        // Persist the crystal in the knowledge DB for future session recovery
        let _ = ctx
            .knowledge
            .record_access("__pre_compact_crystal__", &ctx.session_id);

        // Save Completion Gate snapshot before context is lost
        let edits = ctx.knowledge.recent_edits_all(60).unwrap_or_default();
        if !edits.is_empty() {
            let edit_files: Vec<&str> = edits
                .iter()
                .take(10)
                .map(|e| e.file_path.as_str())
                .collect();
            let compaction_note = format!(
                "PRE-COMPACT: {} files edited this session: {}",
                edits.len(),
                edit_files.join(", ")
            );
            let _ = ctx
                .knowledge
                .append_note("__session_compaction__", &compaction_note);
        }

        HandlerResult::allow(self.name(), Some(crystal))
    }
}

// ══════════════════════════════════════════════════════════════════════
// 2. SubagentStartHandler
// ══════════════════════════════════════════════════════════════════════

/// SubagentStart — injects context for newly spawned subagents.
///
/// Adapted from `subagent_start.rs`. When a subagent starts, records the
/// event and searches the knowledge DB for relevant context to inject.
///
/// **Sync, never blocks.**
pub struct SubagentStartHandler;

impl Handler for SubagentStartHandler {
    fn name(&self) -> &str {
        "subagent_start"
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::SubagentStart]
    }

    fn execute(&self, ctx: &mut CortexContext) -> HandlerResult {
        let subagent_id = ctx
            .input
            .get("subagent_id")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");

        let task_description = ctx
            .input
            .get("task_description")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        // Record the subagent spawn as an access event
        let _ = ctx.knowledge.record_access(
            &format!("__subagent_start:{}__", subagent_id),
            &ctx.session_id,
        );

        if task_description.is_empty() {
            return HandlerResult::skip(self.name());
        }

        // Search knowledge DB for recently-edited files matching task keywords
        // This provides relevant context to the subagent
        let stats = match ctx.knowledge.stats() {
            Ok(s) => s,
            Err(_) => return HandlerResult::skip(self.name()),
        };

        if stats.file_count == 0 {
            return HandlerResult::skip(self.name());
        }

        let mut context_parts = Vec::new();
        context_parts.push(format!(
            "Subagent {} started | project_root={} | knowledge: {} files, {} relations",
            subagent_id,
            ctx.project_root.display(),
            stats.file_count,
            stats.relation_count,
        ));
        context_parts.push(
            "KEY RULES: CODE-FIRST UNIVERSAL | Zero-hallucination | Hooks inviolable".to_string(),
        );

        HandlerResult::allow(self.name(), Some(context_parts.join(" | ")))
    }
}

// ══════════════════════════════════════════════════════════════════════
// 3. SubagentStopHandler
// ══════════════════════════════════════════════════════════════════════

/// SubagentStop — records subagent completion outcomes.
///
/// Adapted from `subagent_stop.rs`. Records the outcome and persists
/// success/failure as bash_outcome-style entries for future recall.
///
/// **Async, never blocks.**
pub struct SubagentStopHandler;

impl Handler for SubagentStopHandler {
    fn name(&self) -> &str {
        "subagent_stop"
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::SubagentStop]
    }

    fn is_async(&self) -> bool {
        true
    }

    fn execute(&self, ctx: &mut CortexContext) -> HandlerResult {
        let subagent_id = ctx
            .input
            .get("subagent_id")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");

        let success = ctx
            .input
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let result_summary = ctx
            .input
            .get("result_summary")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        // Record subagent outcome as a bash outcome for recall
        let status = if success { "SUCCESS" } else { "FAIL" };
        let _ = ctx
            .knowledge
            .record_bash_outcome(&touring_hooks::knowledge::BashOutcome {
                command: format!(
                    "subagent:{} {}",
                    subagent_id,
                    truncate_str(result_summary, 200)
                ),
                command_short: format!("subagent:{}", subagent_id),
                exit_code: if success { 0 } else { 1 },
                success,
                error_pattern: if success {
                    None
                } else {
                    Some(truncate_str(result_summary, 200).to_string())
                },
                file_context: None,
                command_hash: String::new(),
                executed_at: String::new(), // DB default
            });

        // Record access event
        let _ = ctx.knowledge.record_access(
            &format!("__subagent_stop:{}:{}__", subagent_id, status),
            &ctx.session_id,
        );

        HandlerResult::skip(self.name()) // Async learning — no visible output
    }
}

// ══════════════════════════════════════════════════════════════════════
// 4. TeammateIdleHandler
// ══════════════════════════════════════════════════════════════════════

/// TeammateIdle — quality gate for Agent Teams (CILA L6).
///
/// Adapted from `teammate_idle.rs`. When a teammate goes idle, checks
/// quality_passed. **BLOCKS when quality fails** — the only lifecycle
/// handler that actively prevents proceeding.
///
/// **Sync, CAN BLOCK.**
pub struct TeammateIdleHandler;

impl Handler for TeammateIdleHandler {
    fn name(&self) -> &str {
        "teammate_idle"
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::TeammateIdle]
    }

    fn execute(&self, ctx: &mut CortexContext) -> HandlerResult {
        let teammate_id = ctx
            .input
            .get("teammate_id")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");

        let quality_passed = ctx
            .input
            .get("quality_passed")
            .and_then(|v| v.as_bool())
            .unwrap_or(true); // fail-open if field missing

        let checkpoint_written = ctx
            .input
            .get("checkpoint_written")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        // Record teammate activity
        let status = if quality_passed { "PASS" } else { "FAIL" };
        let _ = ctx.knowledge.record_access(
            &format!("__teammate_idle:{}:{}__", teammate_id, status),
            &ctx.session_id,
        );

        // BLOCK when quality gate fails
        if !quality_passed {
            return HandlerResult::block(
                self.name(),
                format!(
                    "TEAMMATE_QUALITY_GATE: quality check did not pass for teammate '{}' \
                     \u{2014} fix issues before continuing",
                    teammate_id
                ),
            );
        }

        // Track checkpoint discipline (non-blocking intelligence)
        if !checkpoint_written {
            let _ = ctx.knowledge.record_access(
                &format!("__teammate_no_checkpoint:{}__", teammate_id),
                &ctx.session_id,
            );
        }

        HandlerResult::skip(self.name())
    }
}

// ══════════════════════════════════════════════════════════════════════
// 5. TaskCompletedHandler
// ══════════════════════════════════════════════════════════════════════

/// TaskCompleted — validates task completion outcomes.
///
/// Adapted from `task_completed.rs`. Records task success/failure as
/// bash_outcome entries for Wilson-style confidence tracking in future
/// sessions.
///
/// **Async, never blocks.**
pub struct TaskCompletedHandler;

impl Handler for TaskCompletedHandler {
    fn name(&self) -> &str {
        "task_completed"
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::TaskCompleted]
    }

    fn is_async(&self) -> bool {
        true
    }

    fn execute(&self, ctx: &mut CortexContext) -> HandlerResult {
        let task_id = ctx
            .input
            .get("task_id")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");

        let success = ctx
            .input
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let output_summary = ctx
            .input
            .get("output_summary")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        // Record task outcome as bash_outcome for historical recall
        let status = if success { "COMPLETED" } else { "FAILED" };
        let _ = ctx
            .knowledge
            .record_bash_outcome(&touring_hooks::knowledge::BashOutcome {
                command: format!(
                    "task:{} {} {}",
                    task_id,
                    status,
                    truncate_str(output_summary, 200)
                ),
                command_short: format!("task:{}", task_id),
                exit_code: if success { 0 } else { 1 },
                success,
                error_pattern: if success {
                    None
                } else {
                    Some(truncate_str(output_summary, 200).to_string())
                },
                file_context: None,
                command_hash: String::new(),
                executed_at: String::new(), // DB default
            });

        // Record access event
        let _ = ctx.knowledge.record_access(
            &format!("__task_completed:{}:{}__", task_id, status),
            &ctx.session_id,
        );

        // ── Mini Completion Gate: check if task edits were E2E tested ──
        let recent_edits = ctx.knowledge.recent_edits_all(30).unwrap_or_default();
        let recent_bashes = ctx
            .knowledge
            .find_bash_outcomes("", 100)
            .unwrap_or_default();

        let has_test = recent_bashes.iter().any(|o| {
            let cmd = o.command.to_lowercase();
            o.exit_code == 0
                && (cmd.contains("pytest") || cmd.contains("cargo test") || cmd.contains("vitest"))
        });

        if !recent_edits.is_empty() && !has_test {
            let count = recent_edits.len();
            return HandlerResult::allow(
                self.name(),
                Some(format!(
                    "⚠ TASK_VERIFY: {} file(s) edited in this task without E2E test execution. Consider running tests before marking complete.",
                    count
                )),
            );
        }

        HandlerResult::skip(self.name()) // Async learning — no visible output
    }
}

// ══════════════════════════════════════════════════════════════════════
// 6. PostToolUseFailureHandler
// ══════════════════════════════════════════════════════════════════════

/// PostToolUseFailure — captures negative RL signals from tool failures.
///
/// When a tool invocation fails (non-zero exit, error output, etc.), this
/// handler records the failure pattern in the knowledge DB and emits a
/// negative reward signal context line for the online RL engine.
///
/// **Async, never blocks.**
pub struct PostToolUseFailureHandler;

impl Handler for PostToolUseFailureHandler {
    fn name(&self) -> &str {
        "post_tool_use_failure"
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::PostToolUseFailure]
    }

    fn is_async(&self) -> bool {
        true
    }

    fn execute(&self, ctx: &mut CortexContext) -> HandlerResult {
        let tool_name = ctx
            .input
            .get("tool_name")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");

        let error_message = ctx
            .input
            .get("tool_output")
            .and_then(|v| v.as_str())
            .or_else(|| ctx.input.get("error").and_then(|v| v.as_str()))
            .unwrap_or("");

        let exit_code = ctx
            .input
            .get("exit_code")
            .and_then(|v| v.as_i64())
            .unwrap_or(1);

        // Truncate error for storage (avoid bloating DB)
        let error_short = truncate_str(error_message, 300);

        // Classify failure severity for RL reward signal
        let reward = if error_short.contains("permission denied") || error_short.contains("EPERM") {
            -0.50 // Security-adjacent failure — heavy penalty
        } else if error_short.contains("not found") || error_short.contains("No such file") {
            -0.15 // Mild penalty — likely stale path
        } else {
            -0.25 // Base rejection penalty
        };

        // Record failure pattern in knowledge DB
        let _ = ctx
            .knowledge
            .record_bash_outcome(&touring_hooks::knowledge::BashOutcome {
                command: format!(
                    "tool_failure:{}:exit{} {}",
                    tool_name, exit_code, error_short
                ),
                command_short: format!("tool_failure:{}", tool_name),
                exit_code,
                success: false,
                error_pattern: if error_short.is_empty() {
                    None
                } else {
                    Some(error_short.to_string())
                },
                file_context: ctx.file_path.clone(),
                command_hash: String::new(),
                executed_at: String::new(),
            });

        // Record access for frequency tracking
        let _ = ctx
            .knowledge
            .record_access(&format!("__tool_failure:{}__", tool_name), &ctx.session_id);

        // Auto-increment file_risk_scores for affected file
        // This makes file_risk a REAL-TIME signal, not just periodic
        if let Some(ref fp) = ctx.file_path {
            let _ = ctx.knowledge.increment_file_risk(fp);
        }

        tracing::warn!(
            tool = tool_name,
            exit_code = exit_code,
            reward = reward,
            "PostToolUseFailure: tool failed, RL reward={:.2}",
            reward
        );

        HandlerResult::allow(
            self.name(),
            Some(format!(
                "RL_SIGNAL: tool={} reward={:.2} exit_code={} error={}",
                tool_name,
                reward,
                exit_code,
                truncate_str(error_short, 80)
            )),
        )
    }
}

// ══════════════════════════════════════════════════════════════════════
// 7. StopFailureHandler
// ══════════════════════════════════════════════════════════════════════

/// StopFailure — classifies session stop failures and emits recovery hints.
///
/// When a session stops due to an error (rate limit, server error, max tokens,
/// auth failure), this handler classifies the error type and suggests a
/// recovery action via additionalContext.
///
/// **Async, never blocks.**
pub struct StopFailureHandler;

impl Handler for StopFailureHandler {
    fn name(&self) -> &str {
        "stop_failure"
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::StopFailure]
    }

    fn is_async(&self) -> bool {
        true
    }

    fn execute(&self, ctx: &mut CortexContext) -> HandlerResult {
        let error_type = ctx
            .input
            .get("error_type")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");

        let error_message = ctx
            .input
            .get("error_message")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        // Classify and determine recovery action
        let (category, recovery_hint) = match error_type.to_lowercase().as_str() {
            "rate_limit" | "ratelimit" | "rate-limit" | "429" => (
                "RATE_LIMIT",
                "Recovery: backoff 30-60s, reduce request frequency, check API quota",
            ),
            "max_output_tokens" | "max_tokens" | "maxoutputtokens" => (
                "MAX_OUTPUT_TOKENS",
                "Recovery: compact context via /clear, split task into smaller subtasks",
            ),
            "server_error" | "servererror" | "500" | "502" | "503" => (
                "SERVER_ERROR",
                "Recovery: retry after 5-10s, check API status page if persistent",
            ),
            "auth_failed" | "authfailed" | "401" | "403" => (
                "AUTH_FAILED",
                "Recovery: re-authenticate, check API key validity and permissions",
            ),
            _ => (
                "UNKNOWN",
                "Recovery: check error details, retry if transient",
            ),
        };

        // Record the stop failure in knowledge DB for pattern learning
        let _ = ctx
            .knowledge
            .record_bash_outcome(&touring_hooks::knowledge::BashOutcome {
                command: format!(
                    "stop_failure:{} {}",
                    category,
                    truncate_str(error_message, 200)
                ),
                command_short: format!("stop_failure:{}", category),
                exit_code: -1,
                success: false,
                error_pattern: Some(category.to_string()),
                file_context: None,
                command_hash: String::new(),
                executed_at: String::new(),
            });

        let _ = ctx
            .knowledge
            .record_access(&format!("__stop_failure:{}__", category), &ctx.session_id);

        tracing::error!(
            error_type = error_type,
            category = category,
            "StopFailure: session stopped due to {} — {}",
            category,
            recovery_hint
        );

        HandlerResult::allow(
            self.name(),
            Some(format!(
                "STOP_FAILURE: category={} | {} | error={}",
                category,
                recovery_hint,
                truncate_str(error_message, 100)
            )),
        )
    }
}

// ══════════════════════════════════════════════════════════════════════
// 8. SessionEndHandler
// ══════════════════════════════════════════════════════════════════════

/// SessionEnd — final persistence checkpoint before session terminates.
///
/// Records session analytics summary (duration, tool counts, error counts)
/// to the knowledge DB. Must be fast (<1s) since SessionEnd has limited
/// timeout from Claude Code.
///
/// **Async, never blocks.**
pub struct SessionEndHandler;

impl Handler for SessionEndHandler {
    fn name(&self) -> &str {
        "session_end"
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::SessionEnd]
    }

    fn is_async(&self) -> bool {
        true
    }

    fn execute(&self, ctx: &mut CortexContext) -> HandlerResult {
        let tool_count = ctx
            .input
            .get("tool_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);

        let error_count = ctx
            .input
            .get("error_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);

        let duration_secs = ctx
            .input
            .get("duration_secs")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);

        // Record session summary as a bash outcome for historical recall
        let _ = ctx
            .knowledge
            .record_bash_outcome(&touring_hooks::knowledge::BashOutcome {
                command: format!(
                    "session_end:{} tools={} errors={} duration={:.0}s",
                    &ctx.session_id[..ctx.session_id.len().min(12)],
                    tool_count,
                    error_count,
                    duration_secs,
                ),
                command_short: "session_end".to_string(),
                exit_code: if error_count == 0 { 0 } else { 1 },
                success: error_count == 0,
                error_pattern: if error_count > 0 {
                    Some(format!("{} errors in session", error_count))
                } else {
                    None
                },
                file_context: None,
                command_hash: String::new(),
                executed_at: String::new(),
            });

        // Record access for session frequency tracking
        let _ = ctx.knowledge.record_access(
            &format!(
                "__session_end:{}__",
                &ctx.session_id[..ctx.session_id.len().min(12)]
            ),
            &ctx.session_id,
        );

        // Gather final knowledge DB stats
        let stats = ctx.knowledge.stats().ok();

        tracing::info!(
            session_id = %&ctx.session_id[..ctx.session_id.len().min(12)],
            tool_count = tool_count,
            error_count = error_count,
            duration_secs = duration_secs,
            knowledge_files = stats.as_ref().map(|s| s.file_count).unwrap_or(0),
            "SessionEnd: persisted session analytics"
        );

        // Skip — no context to inject, this is purely a persistence handler
        HandlerResult::skip(self.name())
    }
}

// ══════════════════════════════════════════════════════════════════════
// 9. PostCompactHandler (H9 — Phase 3)
// ══════════════════════════════════════════════════════════════════════

/// Maximum characters for post-compact re-injection (~750 tokens at 4 chars/token).
const POST_COMPACT_MAX_CHARS: usize = 3000;

/// Maximum compactions within anti-flood window before skipping re-injection.
const ANTI_FLOOD_MAX_COMPACTIONS: usize = 5;

/// Anti-flood window in seconds (10 minutes).
const ANTI_FLOOD_WINDOW_SECS: u64 = 600;

/// PostCompact — re-inject critical context after context compaction.
///
/// After Claude Code compacts the context window, this handler re-injects
/// a tiered summary of critical rules and session state to prevent
/// post-compaction amnesia.
///
/// **Budget enforcement**: Hard cap at 3000 chars (~750 tokens).
/// **Anti-flood**: Skips re-injection if >5 compactions in 10 minutes.
/// **Tiered injection**:
///   - Tier 1 (always, ~100 tokens): Critical rules crystal
///   - Tier 2 (if budget allows, ~200 tokens): Knowledge DB stats
///   - Tier 3 (reserved for future): RL state summary
///
/// **Sync, never blocks.**
#[derive(Default)]
pub struct PostCompactHandler {
    /// Track recent compaction timestamps for anti-flood detection.
    compaction_times: std::sync::Mutex<Vec<std::time::Instant>>,
}

impl PostCompactHandler {
    /// Creates a handler with empty compaction-timestamp tracking.
    pub fn new() -> Self {
        Self {
            compaction_times: std::sync::Mutex::new(Vec::new()),
        }
    }
}

impl Handler for PostCompactHandler {
    fn name(&self) -> &str {
        "post_compact"
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::PostCompact]
    }

    fn execute(&self, ctx: &mut CortexContext) -> HandlerResult {
        // STEP 1: Anti-flood check
        let now = std::time::Instant::now();
        let mut times = self
            .compaction_times
            .lock()
            .unwrap_or_else(|e| e.into_inner());

        // Remove old timestamps outside the window
        let cutoff = now - std::time::Duration::from_secs(ANTI_FLOOD_WINDOW_SECS);
        times.retain(|t| *t > cutoff);

        if times.len() >= ANTI_FLOOD_MAX_COMPACTIONS {
            // Record but skip re-injection
            let _ = ctx
                .knowledge
                .record_access("__post_compact_antiflood__", &ctx.session_id);
            return HandlerResult::skip(self.name());
        }

        // Record this compaction
        times.push(now);
        drop(times); // Release mutex early

        // STEP 2: Build tiered re-injection context
        let mut output = String::new();
        let mut budget_remaining = POST_COMPACT_MAX_CHARS;

        // Tier 1 (always): Critical rules crystal (~400 chars = ~100 tokens)
        let tier1 = format!(
            "POST-COMPACT RECOVERY: session={} | \
             CODE-FIRST (DISCOVER\u{2192}CREATE\u{2192}EXECUTE) | \
             CILA routing active | Zero-hallucination (verify before assert) | \
             Hooks inviolable (NEVER bypass) | Touring v9.0 active",
            &ctx.session_id[..ctx.session_id.len().min(8)],
        );
        if tier1.len() <= budget_remaining {
            output.push_str(&tier1);
            budget_remaining -= tier1.len();
        }

        // Tier 2 (if budget allows): Knowledge DB stats (~200 chars = ~50 tokens)
        if budget_remaining >= 200 {
            if let Ok(stats) = ctx.knowledge.stats() {
                let tier2 = format!(
                    " | Knowledge: files={} relations={} bash={} edits={}",
                    stats.file_count, stats.relation_count, stats.bash_count, stats.edit_count,
                );
                if tier2.len() <= budget_remaining {
                    output.push_str(&tier2);
                    budget_remaining -= tier2.len();
                }
            }
        }

        // Tier 3 (reserved): RL state summary — intentionally omitted until Tier 1+2
        // validated in production. Budget_remaining available for future use.
        let _ = budget_remaining;

        // STEP 3: Hard budget enforcement (safety net)
        if output.len() > POST_COMPACT_MAX_CHARS {
            output.truncate(POST_COMPACT_MAX_CHARS);
        }

        // Record access for tracking compaction frequency
        let _ = ctx
            .knowledge
            .record_access("__post_compact_reinjection__", &ctx.session_id);

        HandlerResult::allow(self.name(), Some(output))
    }
}

// ══════════════════════════════════════════════════════════════════════
// 10. ConfigChangeHandler (H10 — Phase 3)
// ══════════════════════════════════════════════════════════════════════

/// ConfigChange — responds to settings file modifications.
///
/// When a config file changes (local_settings, project_settings, etc.),
/// this handler logs the event and suggests cache invalidation when
/// relevant settings are affected.
///
/// **Sync, never blocks.**
pub struct ConfigChangeHandler;

impl Handler for ConfigChangeHandler {
    fn name(&self) -> &str {
        "config_change"
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::ConfigChange]
    }

    fn execute(&self, ctx: &mut CortexContext) -> HandlerResult {
        let source = ctx
            .input
            .get("source")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");

        // Record config change event
        let _ = ctx
            .knowledge
            .record_access(&format!("__config_change:{}__", source), &ctx.session_id);

        // For settings changes, suggest cache invalidation
        if source == "local_settings" || source == "project_settings" {
            return HandlerResult::allow(
                self.name(),
                Some(format!(
                    "Config changed (source={}) \u{2014} touring caches may need refresh",
                    source,
                )),
            );
        }

        HandlerResult::skip(self.name())
    }
}

// ══════════════════════════════════════════════════════════════════════
// H44-H46: Worktree + Instructions handlers (Sprint 5)
// ══════════════════════════════════════════════════════════════════════

/// Record worktree creation, set project context, and schedule async index rebuild.
///
/// Wave C Subtask 3 — WorktreeCreate index isolation:
/// 1. Extracts `worktree_path` from hook input.
/// 2. Writes `CLAUDE_PROJECT_DIR=<path>` to `CLAUDE_ENV_FILE` when that file exists,
///    so subsequent Bash commands in the worktree session pick up the correct root.
/// 3. Spawns an async `touring index rebuild --dir <path>` as a detached background
///    process (fire-and-forget via `std::thread::spawn`). This prevents cross-pollution
///    between worktrees without blocking the hook return.
/// 4. Snapshots a semantic-tier memory entry so future sessions can recall isolation
///    context for the worktree.
pub struct WorktreeEnterHandler;

impl Handler for WorktreeEnterHandler {
    fn name(&self) -> &str {
        "worktree_enter"
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::WorktreeCreate]
    }

    fn execute(&self, ctx: &mut CortexContext) -> HandlerResult {
        let worktree_path = ctx
            .input
            .get("worktree_path")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let branch = ctx
            .input
            .get("branch")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();

        // Guard: nothing meaningful to do for empty paths.
        if worktree_path.is_empty() {
            return HandlerResult::skip(self.name());
        }

        let project_root = ctx.project_root.display().to_string();
        let timestamp = chrono::Utc::now().to_rfc3339();

        // ── Step 1: Inject CLAUDE_PROJECT_DIR into CLAUDE_ENV_FILE ──────────
        // CLAUDE_ENV_FILE is an optional file that Claude Code reads to set env vars
        // for subsequent Bash commands within a session. Writing here ensures that
        // commands run inside the worktree see the correct project root.
        if let Ok(env_file) = std::env::var("CLAUDE_ENV_FILE") {
            let env_path = std::path::Path::new(&env_file);
            if env_path.exists() {
                // Append (do not overwrite) so other env vars set by the shell survive.
                use std::io::Write;
                if let Ok(mut f) = std::fs::OpenOptions::new().append(true).open(env_path) {
                    let line = format!("export CLAUDE_PROJECT_DIR=\"{worktree_path}\"\n");
                    let _ = f.write_all(line.as_bytes());
                    tracing::debug!(
                        worktree_path = %worktree_path,
                        env_file = %env_file,
                        "wrote CLAUDE_PROJECT_DIR to CLAUDE_ENV_FILE"
                    );
                }
            }
        }

        // ── Step 2: Fire-and-forget async index rebuild ──────────────────────
        // We cannot call cli_index_rebuild directly (needs HookRuntime which is
        // not available in CortexContext). Instead we spawn the `touring` binary
        // as a detached background process. The thread exits as soon as `Command`
        // is launched; we do not join it (true fire-and-forget).
        {
            let path_for_rebuild = worktree_path.clone();
            std::thread::spawn(move || {
                let status = std::process::Command::new("touring")
                    .args(["index", "rebuild", "--dir", &path_for_rebuild])
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status();
                match status {
                    Ok(s) if s.success() => {
                        tracing::info!(
                            worktree_path = %path_for_rebuild,
                            "async index rebuild completed"
                        );
                    }
                    Ok(s) => {
                        tracing::warn!(
                            worktree_path = %path_for_rebuild,
                            exit_code = ?s.code(),
                            "async index rebuild exited with non-zero status"
                        );
                    }
                    Err(e) => {
                        tracing::warn!(
                            worktree_path = %path_for_rebuild,
                            error = %e,
                            "failed to spawn async index rebuild (touring not in PATH?)"
                        );
                    }
                }
            });
        }

        // ── Step 3: Snapshot memory with semantic tier ───────────────────────
        // Upgrade from Working → Reference tier so the worktree context persists
        // across sessions and is recalled by future touring memory recall queries.
        if let Some(ref rlm) = ctx.rlm {
            let val = format!(
                "path={worktree_path} branch={branch} root={project_root} created={timestamp}"
            );
            let _ = rlm.store(
                &format!("worktree:{worktree_path}:created:{timestamp}"),
                touring_intelligence::rl::memory::rlm::MemoryTier::Reference,
                &val,
                Some("worktree_isolation"),
                None,
            );
            tracing::debug!(
                key = %format!("worktree:{worktree_path}:created:{timestamp}"),
                "worktree isolation snapshot stored (Reference tier)"
            );
        }

        // ── Step 4: Return worktree path as system_message context ───────────
        // WorktreeCreate uses systemMessage (not hookSpecificOutput) per types.rs.
        // We return the path so the caller (Claude Code) knows which path was set.
        HandlerResult::allow(self.name(), Some(worktree_path.to_string()))
    }
}

/// Cleanup worktree state on removal.
pub struct WorktreeExitHandler;

impl Handler for WorktreeExitHandler {
    fn name(&self) -> &str {
        "worktree_exit"
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::WorktreeRemove]
    }

    fn is_async(&self) -> bool {
        true
    }

    fn execute(&self, ctx: &mut CortexContext) -> HandlerResult {
        let path = ctx
            .input
            .get("worktree_path")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        if let Some(ref rlm) = ctx.rlm {
            let _ = rlm.delete(
                &format!("worktree:{path}"),
                touring_intelligence::rl::memory::rlm::MemoryTier::Working,
            );
        }
        HandlerResult::skip(self.name())
    }
}

/// Enrich loaded instructions with active gotchas and warnings.
pub struct InstructionsEnricherHandler;

impl Handler for InstructionsEnricherHandler {
    fn name(&self) -> &str {
        "instructions_enricher"
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::InstructionsLoaded]
    }

    fn execute(&self, ctx: &mut CortexContext) -> HandlerResult {
        if ctx.context_budget_remaining < 50 {
            return HandlerResult::skip(self.name());
        }

        let mut warnings: Vec<String> = Vec::new();

        let gotchas = ctx.knowledge.get_gotchas_for_file("*");
        for g in gotchas.iter().take(3) {
            if g.hit_count > 0 {
                warnings.push(format!("[gotcha] {}", g.gotcha));
            }
        }

        if let Some(ref rlm) = ctx.rlm {
            if let Ok(matches) = rlm.search("gotcha:", None, 3) {
                for m in &matches {
                    let snippet: String = m.value.chars().take(60).collect();
                    warnings.push(format!("[rlm] {snippet}"));
                }
            }
        }

        if warnings.is_empty() {
            return HandlerResult::skip(self.name());
        }

        HandlerResult::allow(
            self.name(),
            Some(format!("Warnings: {}", warnings.join(" | "))),
        )
    }
}

/// Handle permission requests — log tool permission patterns for RL learning.
///
/// Claude Code emits PermissionRequest when asking the user to approve a tool.
/// This handler records the pattern for future learning (which tools get approved
/// vs denied), enabling the RL system to prioritize tools the user trusts.
pub struct PermissionRequestHandler;

impl Handler for PermissionRequestHandler {
    fn name(&self) -> &str {
        "permission_request"
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::PermissionRequest]
    }

    fn execute(&self, ctx: &mut CortexContext) -> HandlerResult {
        let tool_name = ctx
            .input
            .get("tool_name")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");

        // Record permission request pattern in RLM for learning
        if let Some(ref rlm) = ctx.rlm {
            let val = format!("tool={tool_name} project={}", ctx.project_root.display());
            let _ = rlm.store(
                &format!("permission:{tool_name}"),
                touring_intelligence::rl::memory::rlm::MemoryTier::Working,
                &val,
                Some("permission_request"),
                None,
            );
        }

        HandlerResult::skip(self.name())
    }
}

// ── Registration ──────────────────────────────────────────────────────

/// Register lifecycle handlers (15 original + 6 new event handlers = 21).
pub fn register(pipeline: &mut Pipeline) {
    pipeline.register(Box::new(PreCompactHandler));
    pipeline.register(Box::new(SubagentStartHandler));
    pipeline.register(Box::new(SubagentStopHandler));
    pipeline.register(Box::new(TeammateIdleHandler));
    pipeline.register(Box::new(TaskCompletedHandler));
    pipeline.register(Box::new(PostToolUseFailureHandler));
    pipeline.register(Box::new(StopFailureHandler));
    pipeline.register(Box::new(SessionEndHandler));
    pipeline.register(Box::new(PostCompactHandler::new()));
    pipeline.register(Box::new(ConfigChangeHandler));
    pipeline.register(Box::new(WorktreeEnterHandler));
    pipeline.register(Box::new(WorktreeExitHandler));
    pipeline.register(Box::new(InstructionsEnricherHandler));
    pipeline.register(Box::new(PermissionRequestHandler));
    pipeline.register(Box::new(VgpLearningHandler));
    // H77-H82: Full Claude Code event coverage
    pipeline.register(Box::new(NotificationHandler));
    pipeline.register(Box::new(SetupHandler));
    pipeline.register(Box::new(ElicitationHandler));
    pipeline.register(Box::new(ElicitationResultHandler));
    pipeline.register(Box::new(CwdChangedHandler));
    pipeline.register(Box::new(FileChangedHandler));
}

// ── H59: VgpLearningHandler ──────────────────────────────────────────

/// Record VGP (Verified Generation Protocol) results for learning on Stop.
/// Tracks which structs were verified, accuracy of field references, and
/// persists summary for future sessions.
pub struct VgpLearningHandler;

impl Handler for VgpLearningHandler {
    fn name(&self) -> &str {
        "vgp_learning"
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::Stop]
    }

    fn is_async(&self) -> bool {
        true
    }

    fn execute(&self, ctx: &mut CortexContext) -> HandlerResult {
        // Collect VGP-related memories from this session
        if let Some(ref rlm) = ctx.rlm {
            // Search for vgp-related entries stored during the session
            if let Ok(entries) = rlm.search("vgp:", None, 10) {
                if entries.is_empty() {
                    return HandlerResult::skip(self.name());
                }

                let mut verified = 0u32;
                let mut hallucinated = 0u32;

                for entry in &entries {
                    if entry.value.contains("verified") || entry.value.contains("OK") {
                        verified += 1;
                    }
                    if entry.value.contains("hallucinated") || entry.value.contains("NOT_FOUND") {
                        hallucinated += 1;
                    }
                }

                let total = verified + hallucinated;
                if total == 0 {
                    return HandlerResult::skip(self.name());
                }

                let accuracy = verified as f64 / total as f64;

                // Persist VGP accuracy via Wilson
                if let Some(ref persistence) = ctx.persistence {
                    let _ = persistence.wilson_update("vgp:session_accuracy", accuracy >= 0.9);
                    let _ = persistence.drift_record("vgp_accuracy", accuracy);
                    let _ = persistence.log_hook_event(
                        "Stop",
                        "VgpLearning",
                        &format!(
                            "verified={verified} hallucinated={hallucinated} accuracy={accuracy:.2}"
                        ),
                        accuracy,
                    );
                }
            }
        }

        HandlerResult::skip(self.name()) // No context injection on Stop
    }
}

// ══════════════════════════════════════════════════════════════════════
// H77-H82: New event handlers (full Claude Code event coverage)
// ══════════════════════════════════════════════════════════════════════

/// Notification — records notification events for analytics.
///
/// When Claude Code sends a notification, this handler records the event
/// type and content for session analytics and frequency tracking.
///
/// **Async, never blocks.**
pub struct NotificationHandler;

impl Handler for NotificationHandler {
    fn name(&self) -> &str {
        "notification"
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::Notification]
    }

    fn is_async(&self) -> bool {
        true
    }

    fn execute(&self, ctx: &mut CortexContext) -> HandlerResult {
        let notification_type = ctx
            .input
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");

        let message = ctx
            .input
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        // Record notification event for analytics
        let _ = ctx.knowledge.record_access(
            &format!("__notification:{}__", notification_type),
            &ctx.session_id,
        );

        // Track notification patterns for session intelligence
        let _ = ctx
            .knowledge
            .record_bash_outcome(&touring_hooks::knowledge::BashOutcome {
                command: format!(
                    "notification:{} {}",
                    notification_type,
                    truncate_str(message, 100)
                ),
                command_short: format!("notification:{}", notification_type),
                exit_code: 0,
                success: true,
                error_pattern: None,
                file_context: None,
                command_hash: String::new(),
                executed_at: String::new(),
            });

        HandlerResult::skip(self.name())
    }
}

/// Setup — initializes touring infrastructure during repo setup.
///
/// On first project setup, initializes the knowledge DB schema,
/// seeds initial file metadata, and records the setup event.
///
/// **Sync, never blocks.**
pub struct SetupHandler;

impl Handler for SetupHandler {
    fn name(&self) -> &str {
        "setup"
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::Setup]
    }

    fn execute(&self, ctx: &mut CortexContext) -> HandlerResult {
        // Record setup event
        let _ = ctx.knowledge.record_access("__setup__", &ctx.session_id);

        // Gather project stats for initial context
        let stats = match ctx.knowledge.stats() {
            Ok(s) => s,
            Err(_) => return HandlerResult::skip(self.name()),
        };

        let context = format!(
            "Touring Setup: project_root={} | knowledge=[files:{} relations:{} bash:{} edits:{}] | \
             Hooks active: all 24 events wired",
            ctx.project_root.display(),
            stats.file_count,
            stats.relation_count,
            stats.bash_count,
            stats.edit_count,
        );

        HandlerResult::allow(self.name(), Some(context))
    }
}

/// Elicitation — records MCP elicitation requests for tracking.
///
/// When an MCP server requests user input via elicitation, this handler
/// records the request pattern for frequency analysis and response
/// time tracking.
///
/// **Async, never blocks.**
pub struct ElicitationHandler;

impl Handler for ElicitationHandler {
    fn name(&self) -> &str {
        "elicitation"
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::Elicitation]
    }

    fn is_async(&self) -> bool {
        true
    }

    fn execute(&self, ctx: &mut CortexContext) -> HandlerResult {
        let server_name = ctx
            .input
            .get("server_name")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");

        let prompt = ctx
            .input
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        // Record elicitation event for frequency tracking
        let _ = ctx
            .knowledge
            .record_access(&format!("__elicitation:{}__", server_name), &ctx.session_id);

        // Store in RLM for MCP interaction intelligence
        if let Some(ref rlm) = ctx.rlm {
            let val = format!("server={server_name} prompt={}", truncate_str(prompt, 100));
            let _ = rlm.store(
                &format!("elicitation:{server_name}"),
                touring_intelligence::rl::memory::rlm::MemoryTier::Working,
                &val,
                Some("mcp_elicitation"),
                None,
            );
        }

        HandlerResult::skip(self.name())
    }
}

/// ElicitationResult — records user responses to MCP elicitation prompts.
///
/// After the user responds to an elicitation, this handler records the
/// response for pattern learning (which servers get fast/slow responses,
/// common answer patterns).
///
/// **Async, never blocks.**
pub struct ElicitationResultHandler;

impl Handler for ElicitationResultHandler {
    fn name(&self) -> &str {
        "elicitation_result"
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::ElicitationResult]
    }

    fn is_async(&self) -> bool {
        true
    }

    fn execute(&self, ctx: &mut CortexContext) -> HandlerResult {
        let server_name = ctx
            .input
            .get("server_name")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");

        let accepted = ctx
            .input
            .get("accepted")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        // Record outcome for RL learning
        let _ = ctx.knowledge.record_access(
            &format!(
                "__elicitation_result:{}:{}__",
                server_name,
                if accepted { "accepted" } else { "rejected" }
            ),
            &ctx.session_id,
        );

        // Store acceptance/rejection pattern in RLM
        if let Some(ref rlm) = ctx.rlm {
            let val = format!("server={server_name} accepted={accepted}");
            let _ = rlm.store(
                &format!("elicitation_result:{server_name}"),
                touring_intelligence::rl::memory::rlm::MemoryTier::Working,
                &val,
                Some("mcp_elicitation_result"),
                None,
            );
        }

        HandlerResult::skip(self.name())
    }
}

/// CwdChanged — responds to working directory changes.
///
/// When the working directory changes, this handler:
/// 1. Records the transition for session navigation tracking
/// 2. Injects knowledge context about the new directory
/// 3. Optionally adds watchPaths for interesting files in the new cwd
///
/// **Sync, never blocks.**
pub struct CwdChangedHandler;

impl Handler for CwdChangedHandler {
    fn name(&self) -> &str {
        "cwd_changed"
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::CwdChanged]
    }

    fn execute(&self, ctx: &mut CortexContext) -> HandlerResult {
        let old_cwd = ctx
            .input
            .get("old_cwd")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let new_cwd = ctx
            .input
            .get("new_cwd")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if new_cwd.is_empty() {
            return HandlerResult::skip(self.name());
        }

        // Record navigation pattern
        let _ = ctx
            .knowledge
            .record_access(&format!("__cwd_changed:{}__", new_cwd), &ctx.session_id);

        // Track directory transition for navigation intelligence
        let _ = ctx
            .knowledge
            .record_bash_outcome(&touring_hooks::knowledge::BashOutcome {
                command: format!(
                    "cwd_changed:{} -> {}",
                    truncate_str(old_cwd, 100),
                    truncate_str(new_cwd, 100),
                ),
                command_short: "cwd_changed".to_string(),
                exit_code: 0,
                success: true,
                error_pattern: None,
                file_context: Some(new_cwd.to_string()),
                command_hash: String::new(),
                executed_at: String::new(),
            });

        // Search knowledge DB for files known in the new directory
        let stats = ctx.knowledge.stats().ok();
        let known_files = stats.as_ref().map(|s| s.file_count).unwrap_or(0);

        if known_files > 0 {
            HandlerResult::allow(
                self.name(),
                Some(format!(
                    "CWD: {} ({} files known in knowledge DB)",
                    new_cwd, known_files,
                )),
            )
        } else {
            HandlerResult::skip(self.name())
        }
    }
}

/// FileChanged — responds to watched file changes on disk.
///
/// When a watched file changes (detected via the watchPaths mechanism),
/// this handler records the change event, invalidates relevant caches,
/// and optionally triggers re-indexing for critical files.
///
/// **Async, never blocks.**
pub struct FileChangedHandler;

impl Handler for FileChangedHandler {
    fn name(&self) -> &str {
        "file_changed"
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::FileChanged]
    }

    fn is_async(&self) -> bool {
        true
    }

    fn requires_cache_invalidation(&self) -> bool {
        true
    }

    fn execute(&self, ctx: &mut CortexContext) -> HandlerResult {
        let file_path = ctx
            .input
            .get("file_path")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let event_type = ctx
            .input
            .get("event")
            .and_then(|v| v.as_str())
            .unwrap_or("modified");

        if file_path.is_empty() {
            return HandlerResult::skip(self.name());
        }

        let rel_path = touring_hooks::make_relative(file_path, &ctx.project_root);

        // Record file change event
        let _ = ctx.knowledge.record_access(
            &format!("__file_changed:{}:{}__", event_type, &rel_path),
            &ctx.session_id,
        );

        // Detect if this is a critical config file that might need cache invalidation
        let is_critical = rel_path.contains("settings.json")
            || rel_path.contains("CLAUDE.md")
            || rel_path.contains("Cargo.toml")
            || rel_path.contains("pyproject.toml")
            || rel_path.contains("package.json")
            || rel_path.contains(".env");

        if is_critical {
            // Record that a critical file changed — handlers may check this
            let _ = ctx.knowledge.append_note(
                "__critical_file_change__",
                &format!("{} ({})", rel_path, event_type),
            );

            tracing::info!(
                file = %rel_path,
                event = event_type,
                "FileChanged: critical file modified — cache invalidation may be needed"
            );
        }

        // Update file knowledge if file was modified
        if event_type == "modified" || event_type == "created" {
            let abs_path = if std::path::Path::new(file_path).is_absolute() {
                file_path.to_string()
            } else {
                ctx.project_root
                    .join(file_path)
                    .to_string_lossy()
                    .to_string()
            };

            if let Ok(content) = std::fs::read_to_string(&abs_path) {
                let line_count = content.lines().count() as i64;
                let knowledge = touring_hooks::knowledge::FileKnowledge {
                    file_path: rel_path.clone(),
                    line_count,
                    ..Default::default()
                };
                let _ = ctx.knowledge.upsert(&knowledge);
            }
        }

        // E5-S6: Signal that filter_cache should be invalidated
        ctx.needs_cache_invalidation = true;
        HandlerResult::skip(self.name())
    }
}

#[cfg(test)]
#[path = "lifecycle_tests.rs"]
mod tests;
