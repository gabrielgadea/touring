//! PostToolUseFailure handler — learns from tool failures.
//!
//! When a tool (Edit/Write/Bash) FAILS, records the failure in the knowledge
//! graph so pre-hooks can warn about similar situations in future sessions.
//! Auto-creates gotchas for file-specific failures to surface as pre-read warnings.
//!
//! Runs async (non-blocking). Target latency: <10ms.
//!
//! ## P4-S1: Security/PII RL Rewards
//! This handler wires PostToolUseFailure into the RL reward pipeline:
//!   - `RL_REWARD_SECURITY_PASS = +0.2` — security-sensitive operation passed
//!   - `RL_REWARD_PII_DENIAL = -1.0` — PII scan blocked sensitive content
//!     These signals flow through `process_immediate_reward` to OnlineRLEngine
//!     (QTable + LinUCB) for adaptive security policy learning.

use super::knowledge::BashOutcome;
use super::llm_judge::judge_failure;
use super::runtime::HookRuntime;
use crate::hook_runtime::HookResponse;
use crate::post_tool_rl::persist_qtable_batched;
use crate::schemas::validate_payload;
use touring_foundation::truncate_str;
use touring_intelligence::rl::ImmediateReward;

/// Number of recent failures on the same file that triggers a Halt response
/// at `Negligible` severity — the highest threshold before halting.
/// Lower severity levels use smaller thresholds (see [`halt_threshold_for_severity`]).
const HALT_FAILURE_THRESHOLD: usize = 5;

// ── P4-S1: Security/PII RL Reward Constants ───────────────────────────────

/// Reward for security-sensitive operation passing (e.g., PII scan clean).
/// Used in POMDP formulation for security policy learning.
const RL_REWARD_SECURITY_PASS: f64 = 0.2;

/// Reward for PII denial — tool attempted to expose sensitive content.
/// Large negative to strongly discourage PII leaks.
const RL_REWARD_PII_DENIAL: f64 = -1.0;

/// Handle PostToolUseFailure events by recording failure context.
///
/// Extracts tool name, error output, and file context from the hook input,
/// then stores the failure as a bash outcome and auto-creates a gotcha
/// for the affected file.
#[tracing::instrument(skip(runtime, input), fields(hook = "post_tool_failure"))]
pub fn run_returning(runtime: &mut HookRuntime, input: &serde_json::Value) -> HookResponse {
    // D9: Validate payload with typed schema — fail fast on malformed input.
    // PostToolFailure skips silently on malformed input (non-blocking, like post_read).
    let tool_input = match input.get("tool_input") {
        Some(v) => v,
        None => return HookResponse::Allow,
    };
    if validate_payload::<crate::schemas::PostToolFailurePayload>(tool_input).is_err() {
        return HookResponse::Allow;
    }
    // ── Parse failure context from hook input ──────────────────────────────────
    let tool_name = parse_tool_name(input);
    let error_text = parse_error_text(input);

    if error_text.is_empty() {
        return HookResponse::Allow;
    }

    let file_path = parse_file_path(input);

    // ── P3-S3: LLM-as-a-Judge — classify severity + get repair recommendation ──
    let judge_report = judge_failure(tool_name, error_text, file_path);
    tracing::debug!("llm_judge: {}", judge_report.summary());

    // ── Record failure in knowledge DB + create gotcha ─────────────────────────
    record_failure_outcome(runtime, tool_name, error_text, file_path);

    // ── Decompose bridge: track failure pattern, mark subtask failed, trigger MCTS replan ──
    let failure_pattern = format!("{}:{}", tool_name, truncate_str(error_text, 100));
    if let Err(e) = super::hook_decompose_bridge::bridge_post_tool_failure(
        runtime,
        &failure_pattern,
        file_path,
        tool_name,
        error_text,
    ) {
        tracing::debug!("post_tool_failure: decompose bridge failed: {}", e);
    }

    // ── P4-S1: Inject security/PII reward into RL pipeline (POMDP state) ───
    let security_state = SecurityState::from_error_text(tool_name, file_path, error_text);
    inject_security_reward(runtime, &security_state);

    // ── HALT gate: circuit breaker for repeated failures on the same file ──
    // P3-S3: Enhanced with LLM-as-a-Judge severity classification
    if let Some(halt_response) =
        check_halt_gate(&runtime.ctx.knowledge, file_path, judge_report.severity)
    {
        return halt_response;
    }

    // EC34: Cognitive enrichment on tool failure — inject file risk + bash failure signals.
    // On failure, surfacing cognitive context (risk score, recent bash failures, gotchas)
    // gives the LLM maximum signal for self-correction. Replicates post_edit.rs:675 pattern.
    if let Some(ref cognitive) = runtime.cognitive {
        let enriched = crate::shared::signals::enrich_with_cognitive(cognitive, file_path, false);
        if !enriched.is_empty() {
            return HookResponse::Context {
                context: enriched,
                event_name: Some("PostToolUse".to_string()),
            };
        }
    }

    HookRuntime::build_allow()
}

// ── Helper functions (CC <= 4 each) ─────────────────────────────────────────

fn parse_tool_name(input: &serde_json::Value) -> &str {
    input
        .get("tool_name")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
}

fn parse_error_text(input: &serde_json::Value) -> &str {
    input
        .get("tool_output")
        .and_then(|v| v.as_str())
        .or_else(|| input.get("error").and_then(|v| v.as_str()))
        .unwrap_or("")
}

fn parse_file_path(input: &serde_json::Value) -> &str {
    input
        .get("tool_input")
        .and_then(|v| v.get("file_path"))
        .and_then(|v| v.as_str())
        .or_else(|| {
            input
                .get("tool_input")
                .and_then(|v| v.get("command"))
                .and_then(|v| v.as_str())
                .and_then(extract_file_from_command)
        })
        .unwrap_or("")
}

fn record_failure_outcome(
    runtime: &mut HookRuntime,
    tool_name: &str,
    error_text: &str,
    file_path: &str,
) {
    let command_key = format!("{tool_name}:{file_path}");
    let error_head = truncate_str(error_text, 500);

    let outcome = BashOutcome {
        command: command_key.clone(),
        command_short: format!("{tool_name}_failure"),
        command_hash: String::new(),
        exit_code: 1,
        success: false,
        error_pattern: Some(truncate_str(error_head, 200).to_string()),
        file_context: if file_path.is_empty() {
            None
        } else {
            Some(file_path.to_string())
        },
        executed_at: chrono::Utc::now().to_rfc3339(),
    };

    if let Err(e) = runtime.ctx.knowledge.record_bash_outcome(&outcome) {
        tracing::debug!("post_tool_failure: failed to record outcome: {e}");
    }

    // S3b: Wire GotEngine — record failure pattern in GoT pheromone trails.
    // Records the failure in the session's GoT snapshot so future reasoning
    // can avoid re-exploring failed thought paths.
    if !file_path.is_empty() {
        if let Some(ref mut snapshot) = runtime.got_snapshot {
            let path_key = format!(
                "failure:{}:{}",
                tool_name,
                file_path.rsplit('/').next().unwrap_or(file_path)
            );
            let entry = snapshot.pheromone_trails.entry(path_key).or_insert(0.0);
            *entry = entry.max(1.0); // penalise failed paths with max strength
        }
    }

    if !file_path.is_empty() {
        let pattern = file_path.rsplit('/').next().unwrap_or(file_path);
        let description = format!("{tool_name} failure: {}", truncate_str(error_head, 200));
        if let Err(e) = runtime
            .ctx
            .knowledge
            .add_gotcha(pattern, &description, "warning", None)
        {
            tracing::debug!("post_tool_failure: failed to add gotcha: {e}");
        }
    }
}

/// Determine HALT threshold based on failure severity.
///
/// Critical/High severity failures trip the circuit breaker faster.
fn halt_threshold_for_severity(severity: super::llm_judge::Severity) -> usize {
    use super::llm_judge::Severity;
    match severity {
        Severity::Critical => 1,
        Severity::High => 2,
        Severity::Medium => 3,
        Severity::Low => 4,
        Severity::Negligible => HALT_FAILURE_THRESHOLD,
    }
}

/// Check whether the session should be HALTED due to repeated failures on the same file.
///
/// Queries the knowledge DB for recent failures targeting `file_path`. If the
/// count meets or exceeds the severity-adjusted threshold, returns a Halt response
/// to stop the session and prevent further damage.
///
/// Returns `None` when the failure count is below threshold.
fn check_halt_gate(
    knowledge: &super::knowledge::FileKnowledgeDB,
    file_path: &str,
    severity: super::llm_judge::Severity,
) -> Option<HookResponse> {
    if file_path.is_empty() {
        return None;
    }

    let threshold = halt_threshold_for_severity(severity);
    let recent_failures = knowledge
        .recent_failures_for_file(file_path, threshold)
        .unwrap_or_default();

    if recent_failures.len() < threshold {
        return None;
    }

    let reason = format!(
        "Session halted: {} consecutive failures on '{}' (severity: {:?}). \
         This file appears to be in an unrecoverable state. \
         Please investigate manually before retrying.",
        recent_failures.len(),
        file_path,
        severity,
    );

    Some(HookRuntime::build_halt(&reason))
}

/// Try to extract a file path from a bash command string.
///
/// Looks for common patterns like paths starting with `/` or `./`.
fn extract_file_from_command(command: &str) -> Option<&str> {
    command.split_whitespace().find(|token| {
        token.starts_with('/')
            || token.starts_with("./")
            || token.contains(".rs")
            || token.contains(".py")
            || token.contains(".ts")
            || token.contains(".js")
    })
}

// ── P4-S1: POMDP Security/PII State Representation ──────────────────────

/// POMDP security state derived from failure context.
///
/// Encodes whether the failure involved security-sensitive operations:
/// - PII exposure attempt: `pii_denied = true`
/// - Security-pass event: `security_sensitive = true`
///
/// Used by OnlineRLEngine to learn adaptive security policies.
#[derive(Debug, Clone)]
struct SecurityState {
    /// True if tool attempted to read/process PII and was denied.
    pub pii_denied: bool,
    /// True if operation passed security scan (e.g., clean PII scan).
    pub security_pass: bool,
    /// Tool that triggered the event.
    pub tool_name: String,
    /// File path involved (empty if no file context).
    pub file_path: String,
}

impl SecurityState {
    /// Detect PII denial from error text patterns.
    fn from_error_text(tool_name: &str, file_path: &str, error_text: &str) -> Self {
        let error_lower = error_text.to_lowercase();
        let pii_denied = error_lower.contains("pii")
            || error_lower.contains("personal data")
            || error_lower.contains("blocked")
            || error_lower.contains("forbidden")
            || error_lower.contains("unauthorized")
            || error_lower.contains("credentials")
            || error_lower.contains("secret")
            || error_lower.contains("api key");

        let security_pass = error_lower.contains("security scan")
            || error_lower.contains("pii scan")
            || error_lower.contains("passed");

        Self {
            pii_denied,
            security_pass,
            tool_name: tool_name.to_string(),
            file_path: file_path.to_string(),
        }
    }

    /// Returns the POMDP reward for this security state.
    fn pomdp_reward(&self) -> Option<f64> {
        if self.pii_denied {
            Some(RL_REWARD_PII_DENIAL)
        } else if self.security_pass {
            Some(RL_REWARD_SECURITY_PASS)
        } else {
            None
        }
    }
}

/// Inject a security/PII reward signal into the OnlineRLEngine.
///
/// Fire-and-forget: errors are traced at debug level, never block.
fn inject_security_reward(runtime: &mut HookRuntime, state: &SecurityState) {
    let Some(reward_val) = state.pomdp_reward() else {
        return;
    };

    let reward = ImmediateReward {
        tool_name: state.tool_name.clone(),
        accepted: state.security_pass,
        latency_ms: 0,
        error_count: if state.pii_denied { 1 } else { 0 },
        cila_level: 0,
        file_type: detect_file_type_from_path(&state.file_path),
        quality_score: Some(reward_val),
    };

    // Ensure qtable cache exists
    if runtime.learning.qtable_cache.is_none() {
        let qtable_path = runtime.project_root.join(".claude/data/qtable.rkyv");
        let mut qt = touring_intelligence::rl::QTable::new();
        if qtable_path.exists() {
            if let Ok((loaded, _rev)) = touring_intelligence::rl::QTable::load_rkyv(&qtable_path) {
                qt = loaded;
            }
        }
        runtime.learning.qtable_cache = Some(qt);
    }

    let mut qtable = match runtime.learning.qtable_cache.take() {
        Some(qt) => qt,
        None => return,
    };

    runtime.process_immediate_reward(&reward, &mut qtable);
    runtime.learning.qtable_cache = Some(qtable);

    // Persist qtable cache after update (cross-session learning)
    persist_qtable_batched(runtime);
    tracing::debug!(
        pii_denied = state.pii_denied,
        security_pass = state.security_pass,
        reward = reward_val,
        "P4-S1 security reward injected into RL pipeline"
    );
}

/// Detect file type from path (mirrors post_tool_rl logic).
fn detect_file_type_from_path(path: &str) -> u8 {
    if path.is_empty() {
        return 3;
    }
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
