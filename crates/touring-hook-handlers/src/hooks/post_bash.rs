//! Post-Bash Hook — Captures command outcomes for learning and context feedback.
//!
//! After Claude executes a bash command, this hook:
//! 1. Extracts command, exit code, and error patterns
//! 2. Detects file context from command arguments
//! 3. Stores outcome in bash_outcomes table
//! 4. Returns structured test/lint metrics via run_returning() (E11)
//! 5. Records failed commands in Pensieve as embeddings (E15)
//! 6. Full command hash for deduplication (E11), error indicators fallback (AF4)
//! 7. Injects temporal reliability signal on bash failure via analyze_trends (E2)
//!
//! Target latency: <10ms.

use once_cell::sync::Lazy;
use regex::Regex;

use super::knowledge::BashOutcome;
use super::pre_bash::extract_command_short;
use super::runtime::{HookResponse, HookRuntime};
use super::shared::patterns::FILE_PATH_RE;
use crate::schemas::validate_payload;
use crate::shared::hook_helpers;
use touring_foundation::truncate_str;
use touring_intelligence::rl::ImmediateReward;

/// Lazy-compiled regex for exit code detection (compiled once, reused across calls).
static RE_EXIT_CODE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)exit[_ ]?code[: ]+(\d+)").expect("static regex is valid"));

/// Run the post-bash hook (diverging version — for CLI entry point).
#[tracing::instrument(skip(runtime, input), fields(hook = "post_bash"))]
pub fn run(
    runtime: &mut HookRuntime,
    input: &serde_json::Value,
) -> Result<(), touring_hook_runtime::hook_runtime::HookDispatchError> {
    run_returning(runtime, input).emit()
}

/// Run the post-bash hook, returning a `HookResponse`.
///
/// Records bash outcome in the knowledge graph and, when OutputCapture
/// extracts structured metrics (test counts, coverage, lint errors),
/// returns them as `HookResponse::Context` so Claude can self-correct.
///
/// On bash failure, also injects a temporal reliability signal from
/// `touring_analysis::analyze_trends` — providing 7-day bash success rate
/// and edit velocity context to help Claude understand system-wide health
/// trends (E2: first touring_analysis integration in post_bash).
///
/// Feeds ImmediateReward to OnlineRL after recording the outcome
/// (similar to post_edit S-5 pattern).
///
/// Returns `HookResponse::Allow` when there is no structured feedback.
pub fn run_returning(runtime: &mut HookRuntime, input: &serde_json::Value) -> HookResponse {
    // D9: Validate payload with typed schema — fail fast on malformed input.
    let tool_input = match input.get("tool_input") {
        Some(v) => v,
        None => return HookResponse::Allow,
    };
    let validated = match validate_payload::<crate::schemas::PostBashPayload>(tool_input) {
        Ok(v) => v,
        Err(errors) => {
            // S21: a Post event cannot deny (the call already ran) — log and allow.
            tracing::warn!(%errors, "post_bash: malformed tool_input — allowing");
            return HookResponse::Allow;
        }
    };
    let command = validated.command.as_str();
    // F10 (2026-09-02): the output lives in `tool_response` (PostToolUse) or
    // in the top-level `error` (PostToolUseFailure) — `tool_input.stdout` is
    // only the legacy/test shape. A user interrupt is not a failure to learn.
    let payload = bash_payload_output(input, validated.stdout.as_deref());
    if payload.interrupted {
        return HookResponse::Allow;
    }
    let raw_output = payload.raw_output.as_str();

    if command.is_empty() {
        return HookResponse::Allow;
    }

    // F4 P3 (2026-09-01) — feed the signal mirror from REAL PostToolUse
    // traffic: a native `touring <hook>` CLI call counts exactly like an
    // in-sandbox SDK query. PostToolUse only fires on success (failures go
    // to PostToolUseFailure), duration is not in the payload (0 = unknown),
    // and a mirror error never touches the hook response — observability,
    // not correctness.
    if let Some(hook) = touring_code::sdk::classify_bash_command(command)
        && let Some(home) = std::env::var_os("HOME")
    {
        let mirror =
            touring_code::sdk_signal_mirror::default_mirror_path(std::path::Path::new(&home));
        // F0 (01/09) — LOUD on both arms (fail-soft kept): the info line is
        // the delivery-path instrument — grep the daemon log to see WHICH
        // process ran the feeder for a live session event.
        // F10: a PostToolUseFailure delivery is a failed hook call — the
        // mirror's `success` stops being unconditionally `true`.
        match touring_code::sdk::record_hook_call(
            &mirror,
            hook,
            0,
            !payload.failure_event,
            touring_code::sdk_signal_mirror::MirrorOrigin::PostBash,
        ) {
            Ok(_) => tracing::info!("post_bash feeder: {} -> signal mirror", hook.as_str()),
            Err(e) => tracing::warn!("post_bash feeder: mirror write failed: {e}"),
        }
    }

    let outcome = match build_bash_outcome_for_event(command, &payload) {
        Some(o) => o,
        None => return HookResponse::Allow,
    };

    // S22: provenance — which session, and which event (PostToolUse = rc 0 by construction).
    let origin_event = if payload.failure_event {
        "PostToolUseFailure"
    } else if payload.success_event {
        "PostToolUse"
    } else {
        "legacy_payload"
    };
    if let Err(e) = runtime.ctx.knowledge.record_bash_outcome_with_origin(
        &outcome,
        crate::shared::signals::payload_session(input),
        origin_event,
    ) {
        tracing::debug!(
            "record_bash_outcome failed for '{}': {e}",
            outcome.command_short
        );
    }
    // EC6: Fire-and-forget async record_bash_outcome to AsyncFileKnowledgeDB.
    // Runs alongside the sync call above — non-blocking, same outcome, async persistence path.
    // Uses Handle::try_current() — safe from tokio::task::spawn_blocking context.
    if let Some(adb) = runtime.ctx.async_knowledge.as_ref().cloned()
        && let Ok(handle) = tokio::runtime::Handle::try_current()
    {
        let outcome_clone = outcome.clone();
        drop(handle.spawn(async move {
            let _ = adb.record_bash_outcome(&outcome_clone).await;
        }));
    }

    // Wire to OnlineRL: feed ImmediateReward after recording bash outcome.
    // Similar pattern to post_edit S-5, but for bash tool.
    let latency_ms = input
        .pointer("/tool_input/start_time")
        .and_then(|v| v.as_i64())
        .map(|start| {
            (std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64)
                .saturating_sub(start as u64)
        })
        .unwrap_or(0);

    let cila_level = runtime
        .ctx
        .result_cache
        .get_result("__meta__", "__session_cila_level__")
        .and_then(|s| s.parse::<u8>().ok())
        .unwrap_or(2);

    // error_count: non-zero exit code = 1 error
    let error_count = if outcome.exit_code != 0 { 1 } else { 0 };

    let reward = ImmediateReward {
        tool_name: "Bash".to_string(),
        accepted: outcome.success,
        latency_ms,
        error_count,
        cila_level,
        file_type: 3, // Bash is "other" file type
        quality_score: None,
    };

    // Load QTable from cache, process reward, and return to cache.
    hook_helpers::ensure_qtable_loaded(runtime);
    if let Some(mut qtable) = runtime.learning.qtable_cache.take() {
        runtime.process_immediate_reward(&reward, &mut qtable);
        runtime.learning.qtable_cache = Some(qtable);
    }

    // WS4: Capture large output and extract structured metrics.
    let capture = maybe_capture_and_persist(
        &runtime.ctx.knowledge,
        command,
        raw_output,
        outcome.exit_code,
        &outcome,
    );

    // E15: Record failed command in Pensieve for ANN penalty lookup in pre_bash.
    // FIX6: Use full command for richer semantic differentiation —
    // `cargo test --workspace` vs `cargo build` hash to distinct states
    // instead of both collapsing to just `["cargo"]`.
    if !outcome.success {
        let states = crate::shared::command_hash::command_to_states(&outcome.command);
        if !states.is_empty() {
            let reason = outcome.error_pattern.as_deref().unwrap_or("unknown error");
            let depth = states.len();
            match runtime.learning.pensieve.try_borrow_mut() {
                Ok(mut pensieve) => {
                    pensieve.record_failure(&states, reason, depth);
                    tracing::debug!(
                        command = %outcome.command,
                        "pensieve: recorded failed command ({} states)",
                        states.len()
                    );
                }
                _ => {
                    tracing::debug!("pensieve: skipped recording (already borrowed)");
                }
            }
        }
    }

    // Cross-cutting: a failed command that involves a file records it as that file's
    // latest bash outcome — unless a capture already did, with the richer summary.
    if !outcome.success && capture.is_none() {
        maybe_record_failure_note(&runtime.ctx.knowledge, &outcome);
    }

    // E2: Temporal quality signal on bash failure — first touring_analysis integration in post_bash.
    // analyze_trends is a pure sync DB scan (~2-5ms), within post_bash <10ms SLA on failure path.
    // Pattern mirrors pre_edit.rs: `touring_analysis::analyze_trends(db.conn_ref())`.
    let temporal_ctx: Option<String> = if !outcome.success {
        let trend = touring_analysis::analyze_trends(runtime.ctx.knowledge.conn_ref());
        if trend.bash_success_rate > 0.0 || trend.edit_velocity > 0.0 {
            let drift_note = trend
                .quality_drift
                .filter(|&d| d > 0.3)
                .map(|_| " \u{2014} drift detected")
                .unwrap_or("");
            Some(format!(
                "temporal: reliability {:.0}% (7d), velocity {:.1} edits/day{drift_note}",
                trend.bash_success_rate * 100.0,
                trend.edit_velocity,
            ))
        } else {
            None
        }
    } else {
        None
    };

    // E11: Build context feedback from structured OutputCapture metrics.
    let base_response = build_capture_response(&outcome, capture.as_ref());

    // EC35: Pre-compute cognitive enrichment for the failure path.
    // Passes file_context when available for file-specific risk signals; "" otherwise
    // so that recent_bash_outcomes still fires even without a file target.
    // Mirrors post_tool_failure.rs:EC34 — cognitive signals are maximally valuable on failure.
    let cog_enriched: Option<String> = if !outcome.success {
        runtime.cognitive.as_ref().and_then(|cog| {
            let path_ref = outcome.file_context.as_deref().unwrap_or("");
            let enriched = crate::shared::signals::enrich_with_cognitive(
                cog,
                path_ref,
                false,
                crate::shared::signals::payload_session(input),
            );
            if enriched.is_empty() {
                None
            } else {
                Some(enriched)
            }
        })
    } else {
        None
    };

    if let Some(t_ctx) = temporal_ctx {
        return match base_response {
            HookResponse::Context {
                context,
                event_name,
            } => HookResponse::Context {
                context: {
                    let mut s = format!("{context}\n{t_ctx}");
                    if let Some(ref cog) = cog_enriched {
                        s.push('\n');
                        s.push_str(cog);
                    }
                    s
                },
                event_name,
            },
            HookResponse::Allow => HookResponse::Context {
                context: {
                    let mut s = t_ctx;
                    if let Some(ref cog) = cog_enriched {
                        s.push('\n');
                        s.push_str(cog);
                    }
                    s
                },
                event_name: Some("PostToolUse".to_string()),
            },
            other => other,
        };
    }

    // EC35: No temporal context — merge cognitive enrichment into base response if present.
    if let Some(enriched) = cog_enriched {
        return match base_response {
            HookResponse::Context {
                context,
                event_name,
            } => HookResponse::Context {
                context: format!("{context}\n{enriched}"),
                event_name,
            },
            HookResponse::Allow => HookResponse::Context {
                context: enriched,
                event_name: Some("PostToolUse".to_string()),
            },
            other => other,
        };
    }
    base_response
}

/// Build a `BashOutcome` from raw command and output strings.
///
/// Returns `None` when `command_short` is empty (no recognisable command).
/// Truncates `output` to the first 2 000 characters before analysis so that
/// large outputs do not inflate detection cost.
#[cfg(test)]
fn build_bash_outcome(command: &str, raw_output: &str) -> Option<BashOutcome> {
    let output = BashPayloadOutput {
        raw_output: raw_output.to_string(),
        failure_event: false,
        success_event: false,
        exit_code_hint: None,
        interrupted: false,
    };
    build_bash_outcome_for_event(command, &output)
}

/// What the hook learned about the command's output from the Claude Code
/// payload (F10, 2026-09-02).
///
/// `PostToolUse` fires only on success and carries the output in
/// `tool_response` (`stdout` + `stderr`); `PostToolUseFailure` carries the
/// interleaved output in the top-level `error` string, which for Bash starts
/// with an `Exit code N` line. `interrupted` mirrors `is_interrupt` — a user
/// Ctrl-C is not a command failure to learn from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BashPayloadOutput {
    /// Text handed to the exit-code / error-pattern detectors.
    pub raw_output: String,
    /// `true` when the event is `PostToolUseFailure` (or an `error` field
    /// arrived without a `tool_response`).
    pub failure_event: bool,
    /// S21: a `PostToolUse` delivery — Claude Code fires it only when the call
    /// SUCCEEDED (failures go to `PostToolUseFailure`). `false` for legacy payloads
    /// that name no event and carry no `tool_response` (text inference kept there).
    pub success_event: bool,
    /// Exit code the payload states explicitly, when it does.
    pub exit_code_hint: Option<i64>,
    /// `true` when the tool run was interrupted by the user.
    pub interrupted: bool,
}

/// Read the command output from the hook payload, in order of trust:
/// failure `error` text → `tool_response.{stdout,stderr}` → the legacy
/// `tool_input.stdout` shape (`legacy_stdout`, kept for older emitters/tests).
pub(crate) fn bash_payload_output(
    input: &serde_json::Value,
    legacy_stdout: Option<&str>,
) -> BashPayloadOutput {
    let event_name = input
        .get("hook_event_name")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let error_text = input.get("error").and_then(|v| v.as_str());
    let response = input.get("tool_response");
    let interrupted = input
        .get("is_interrupt")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
        || response
            .and_then(|r| r.get("interrupted"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
    let failure_event =
        event_name == "PostToolUseFailure" || (response.is_none() && error_text.is_some());

    let exit_code_hint = response
        .and_then(|r| r.get("exit_code").or_else(|| r.get("returnCode")))
        .or_else(|| input.pointer("/tool_input/exit_code"))
        .and_then(|v| v.as_i64());

    let raw_output = if failure_event {
        error_text.unwrap_or("").to_string()
    } else if let Some(resp) = response {
        match resp {
            serde_json::Value::String(s) => s.clone(),
            serde_json::Value::Object(_) => {
                let stdout = resp.get("stdout").and_then(|v| v.as_str()).unwrap_or("");
                let stderr = resp.get("stderr").and_then(|v| v.as_str()).unwrap_or("");
                match (stdout.is_empty(), stderr.is_empty()) {
                    (_, true) => stdout.to_string(),
                    (true, false) => stderr.to_string(),
                    (false, false) => format!("{stdout}\n{stderr}"),
                }
            }
            _ => String::new(),
        }
    } else {
        legacy_stdout.unwrap_or("").to_string()
    };

    let success_event = !failure_event && (event_name == "PostToolUse" || response.is_some());
    BashPayloadOutput {
        raw_output,
        failure_event,
        success_event,
        exit_code_hint,
        interrupted,
    }
}

/// Build the outcome from what the payload said (F10). On a failure event
/// the outcome can never read as success: an output with no `Exit code`
/// line collapses to exit 1 rather than the detector's default 0.
fn build_bash_outcome_for_event(command: &str, output: &BashPayloadOutput) -> Option<BashOutcome> {
    let command_short = extract_command_short(command);
    if command_short.is_empty() {
        return None;
    }

    let raw_output = output.raw_output.as_str();
    let output_head = truncate_str(raw_output, 2000);
    if output.success_event {
        return Some(success_event_outcome(
            command,
            command_short,
            output,
            output_head,
        ));
    }
    let mut exit_code = output
        .exit_code_hint
        .unwrap_or_else(|| detect_exit_code(output_head));
    if output.failure_event && exit_code == 0 {
        exit_code = 1;
    }
    // FIX8: Also detect error indicators when exit_code==0 — some tools
    // (e.g. `cargo test` capturing panics) print "FAILED" or "error[" but
    // still exit 0. Checking indicators catches these false-success cases.
    let has_error_indicators = check_error_indicators(output_head) != 0;
    let success = !output.failure_event && exit_code == 0 && !has_error_indicators;
    let error_pattern = if !success {
        extract_error_pattern(output_head)
    } else {
        None
    };

    Some(BashOutcome {
        command: truncate(command, 500),
        command_short,
        command_hash: String::new(), // auto-computed by record_bash_outcome
        exit_code,
        success,
        error_pattern,
        file_context: extract_file_context(command),
        executed_at: String::new(), // DB default
    })
}

/// Label of an rc-0 outcome whose text looks like a failure after a pipe without
/// `pipefail` (S21): stored in `error_pattern`, never as `success = false`.
pub(crate) const MASKED_FAILURE_LABEL: &str = "masked_failure_suspected";

/// The outcome of a SUCCESS event (S21). PostToolUse fires only when the call succeeded,
/// so the exit code is 0 unless the payload states one; text never flips it. The one
/// text signal kept is the masking suspicion, labelled, for pipelines without pipefail.
fn success_event_outcome(
    command: &str,
    command_short: String,
    output: &BashPayloadOutput,
    output_head: &str,
) -> BashOutcome {
    let exit_code = output.exit_code_hint.unwrap_or(0);
    let success = exit_code == 0;
    let error_pattern = if !success {
        extract_error_pattern(output_head)
    } else if pipe_without_pipefail(command) && check_error_indicators(output_head) != 0 {
        let what = extract_error_pattern(output_head).unwrap_or_else(|| "error text".to_string());
        Some(format!("{MASKED_FAILURE_LABEL}: {what}"))
    } else {
        None
    };
    BashOutcome {
        command: truncate(command, 500),
        command_short,
        command_hash: String::new(),
        exit_code,
        success,
        error_pattern,
        file_context: extract_file_context(command),
        executed_at: String::new(),
    }
}

/// A `|` the shell reads (not inside quotes/heredocs, not `||`) and no `pipefail`.
fn pipe_without_pipefail(command: &str) -> bool {
    let shell = touring_ceg::gateway::gate::mask_shell_data(command);
    let b = shell.as_bytes();
    let piped = shell
        .match_indices('|')
        .any(|(i, _)| b.get(i + 1) != Some(&b'|') && b.get(i.wrapping_sub(1)) != Some(&b'|'));
    piped && !command.contains("pipefail")
}

/// WS4: Capture structured metrics from large output, persist as a note,
/// and return the `OutputCapture` for context feedback.
///
/// Delegates to [`super::output_capture::capture_output`]. Only writes when
/// both a capture result and a file context are present.
fn maybe_capture_and_persist(
    db: &super::knowledge::FileKnowledgeDB,
    command: &str,
    raw_output: &str,
    exit_code: i64,
    outcome: &BashOutcome,
) -> Option<super::output_capture::OutputCapture> {
    let capture = super::output_capture::capture_output(command, raw_output, exit_code)?;

    if let Some(ref file_ctx) = outcome.file_context {
        let note = bash_outcome_note(&capture.extractor_name, &capture.summary, &now_minute_utc());
        if let Err(e) = db.replace_owned_note(file_ctx, &is_bash_outcome_segment, &note) {
            tracing::debug!("replace_owned_note (capture) failed for {file_ctx}: {e}");
        }
    }

    Some(capture)
}

/// The latest bash outcome for a file — ONE segment, replaced on every capture or
/// failure, at the head of `notes` (V0e5, 09/10/2026: appended, a red pytest run stayed
/// at the readers' 100–120-byte head for 17 min after newer runs). `; ` separates
/// segments, so it is escaped inside the summary (cargo's `1 passed; 2 failed`).
fn bash_outcome_note(extractor: &str, summary: &str, at: &str) -> String {
    format!("{extractor}@{at}: {}", summary.replace("; ", ", "))
}

/// UTC to the minute: the reader sees how old the outcome is, inside its 100-byte head.
fn now_minute_utc() -> String {
    chrono::Utc::now().format("%m-%dT%H:%MZ").to_string()
}

/// The segments this writer owns: its current form and every legacy form it APPENDED
/// before — the bare summaries of the four extractors, the `bash_metrics[…]:` key=value
/// note, the failure note and the fragments cargo's `; ` was split into. Positive
/// predicates only: a segment no form names belongs to another writer and stays.
fn is_bash_outcome_segment(s: &str) -> bool {
    const LEGACY_PREFIXES: [&str; 6] = [
        "bash_metrics[",
        "PASS:",
        "FAIL",
        "UNKNOWN",
        "test result:",
        "ruff:",
    ];
    LEGACY_PREFIXES.iter().any(|p| s.starts_with(p))
        || (s.starts_with("Exit ") && s.contains(" lines output ("))
        || (s.starts_with('`') && s.contains("` failed (exit "))
        || is_cargo_fragment(s)
        || is_current_bash_outcome(s)
}

/// `"0 failed"`, `"2 ignored in 1.20s"`: what cargo's summary left once split on `; `.
fn is_cargo_fragment(s: &str) -> bool {
    const COUNTS: [&str; 6] = [
        "passed", "failed", "ignored", "measured", "filtered", "errors",
    ];
    let mut words = s.split_whitespace();
    words.next().is_some_and(|n| n.parse::<u64>().is_ok())
        && words.next().is_some_and(|w| COUNTS.contains(&w))
}

/// `"<extractor>@MM-DDTHH:MMZ: …"` — the form `bash_outcome_note` writes.
fn is_current_bash_outcome(s: &str) -> bool {
    s.split_once('@').is_some_and(|(name, rest)| {
        !name.is_empty()
            && name.bytes().all(|b| b.is_ascii_lowercase() || b == b'_')
            && rest.get(5..6) == Some("T")
            && rest.contains("Z: ")
    })
}

/// Build a `HookResponse` from the bash outcome and optional OutputCapture.
///
/// Returns `HookResponse::Context` with a structured summary when:
/// - The command failed (provides error context for self-correction), OR
/// - OutputCapture extracted meaningful metrics (test counts, coverage, etc.)
///
/// Returns `HookResponse::Allow` otherwise (no actionable feedback).
fn build_capture_response(
    outcome: &BashOutcome,
    capture: Option<&super::output_capture::OutputCapture>,
) -> HookResponse {
    let mut parts: Vec<String> = Vec::new();

    // S21: rc 0 with error text after a pipe lacking pipefail — a SUSPICION, never "failed".
    if outcome.success
        && let Some(label) = outcome.error_pattern.as_deref()
        && label.starts_with(MASKED_FAILURE_LABEL)
    {
        parts.push(format!(
            "`{}` exited 0, but its output shows errors after a pipe without `set -o pipefail` \
             — the real exit code may be masked (suspected; not recorded as a failure).",
            outcome.command_short
        ));
    }
    // Failed command: include error pattern for immediate feedback.
    if !outcome.success {
        let error_desc = outcome.error_pattern.as_deref().unwrap_or("unknown error");
        parts.push(format!(
            "`{}` failed (exit {}): {}",
            outcome.command_short, outcome.exit_code, error_desc
        ));
    }

    // Structured metrics from OutputCapture (test counts, coverage, lint errors).
    if let Some(cap) = capture
        && !cap.metrics.is_empty()
    {
        parts.push(format_capture_context(cap));
    }

    if parts.is_empty() {
        return HookResponse::Allow;
    }

    let context = parts.join("\n");
    HookResponse::Context {
        context,
        event_name: Some("PostToolUse".to_string()),
    }
}

/// Format OutputCapture metrics into a concise context string for Claude.
fn format_capture_context(capture: &super::output_capture::OutputCapture) -> String {
    let mut ctx = format!("[{}]", capture.extractor_name);

    // Extract well-known metrics in a readable format.
    if let Some(passed) = capture.metrics.get("passed") {
        ctx.push_str(&format!(" passed={passed}"));
    }
    if let Some(failed) = capture.metrics.get("failed") {
        ctx.push_str(&format!(" failed={failed}"));
    }
    if let Some(errors) = capture.metrics.get("errors") {
        ctx.push_str(&format!(" errors={errors}"));
    }
    if let Some(ignored) = capture.metrics.get("ignored") {
        ctx.push_str(&format!(" ignored={ignored}"));
    }
    if let Some(coverage) = capture.metrics.get("coverage_pct") {
        ctx.push_str(&format!(" coverage={coverage}%"));
    }
    if let Some(duration) = capture.metrics.get("duration") {
        ctx.push_str(&format!(" duration={duration}"));
    }
    if let Some(fixable) = capture.metrics.get("fixable") {
        ctx.push_str(&format!(" fixable={fixable}"));
    }

    ctx
}

/// Record a failure as the file's latest bash outcome (replacing the previous one — V0e5).
///
/// Only fires when the outcome has a `file_context` so the note is
/// attributed to the right file in the knowledge graph.
fn maybe_record_failure_note(db: &super::knowledge::FileKnowledgeDB, outcome: &BashOutcome) {
    if let Some(ref file_ctx) = outcome.file_context {
        let note = format!(
            "`{}` failed (exit {})",
            truncate(&outcome.command, 60),
            outcome.exit_code
        );
        if let Err(e) = db.replace_owned_note(file_ctx, &is_bash_outcome_segment, &note) {
            tracing::debug!("replace_owned_note (failure) failed for {file_ctx}: {e}");
        }
    }
}

/// Detect exit code from output text.
///
/// I16 fix (v2): Matches explicit "exit code" patterns in an **adaptive window**
/// of output to handle both short commands and long outputs like `cargo build`
/// (which can produce 100+ lines). The window scales with output length:
/// - ≤10 lines: full output
/// - 11-100 lines: last 20 lines
/// - >100 lines: last 50 lines
///
/// Also checks that the line doesn't look like documentation (e.g.,
/// contains "usage:", "example", or "man page").
fn detect_exit_code(output: &str) -> i64 {
    // Adaptive window: scale with output size to handle both short/long commands
    let lines: Vec<&str> = output.lines().collect();
    let window_size = if lines.len() <= 10 {
        lines.len()
    } else if lines.len() <= 100 {
        20
    } else {
        50
    };
    let tail_start = lines.len().saturating_sub(window_size);
    // SAFETY: saturating_sub guarantees tail_start <= lines.len()
    #[allow(clippy::indexing_slicing)]
    let tail_lines = &lines[tail_start..];

    for line in tail_lines {
        if let Some(cap) = RE_EXIT_CODE.captures(line) {
            // Confidence check: skip if the line looks like documentation/help text
            let lower = line.to_lowercase();
            if is_help_context(&lower) {
                continue;
            }
            if let Ok(code) = cap[1].parse::<i64>() {
                return code;
            }
        }
    }

    // Fallback: scan full output for well-known error indicators.
    check_error_indicators(output)
}

/// Scan `output` for well-known error strings and return the appropriate exit
/// code, or `0` if none are found.
///
/// Separated from `detect_exit_code` so each indicator class is independently
/// readable and testable.
fn check_error_indicators(output: &str) -> i64 {
    if output.contains("error[") || output.contains("FAILED") {
        return 1;
    }
    // "Error:" only counts if NOT in a documentation/help context line
    if has_real_error_indicator(output) {
        return 1;
    }
    if output.contains("command not found") || output.contains("No such file") {
        return 127;
    }
    0
}

/// Returns true if the line looks like documentation, help text, or examples
/// rather than an actual execution result.
fn is_help_context(lower_line: &str) -> bool {
    const HELP_MARKERS: &[&str] = &[
        "usage:",
        "example",
        "man page",
        "synopsis",
        "description:",
        "options:",
        "see also",
        "documentation",
        "--help",
        "returns an exit",
        "the exit code",
        "possible exit",
        "exit status",
        "exit codes:",
    ];
    HELP_MARKERS
        .iter()
        .any(|marker| lower_line.contains(marker))
}

/// Check if output contains "Error:" in a line that is NOT help/docs context.
fn has_real_error_indicator(output: &str) -> bool {
    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.contains("Error:") {
            let lower = trimmed.to_lowercase();
            if !is_help_context(&lower) {
                return true;
            }
        }
    }
    false
}

/// Extract a short error pattern from output.
fn extract_error_pattern(output: &str) -> Option<String> {
    // Look for the first error-like line
    for line in output.lines().take(50) {
        let trimmed = line.trim();
        if trimmed.contains("Error")
            || trimmed.contains("error")
            || trimmed.contains("FAILED")
            || trimmed.contains("traceback")
            || trimmed.starts_with("E ")
        {
            return Some(truncate(trimmed, 200));
        }
    }
    // Fallback: first non-empty line
    output
        .lines()
        .find(|l| !l.trim().is_empty())
        .map(|l| truncate(l.trim(), 200))
}

/// Extract file paths mentioned in the command.
fn extract_file_context(command: &str) -> Option<String> {
    FILE_PATH_RE
        .captures(command)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
}

/// Truncate string to max length (UTF-8 safe, delegates to touring_foundation).
fn truncate(s: &str, max: usize) -> String {
    let safe = truncate_str(s, max.saturating_sub(3));
    if safe.len() < s.len() {
        format!("{safe}...")
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_exit_code_explicit() {
        // Exit code in last 5 lines should be detected
        assert_eq!(detect_exit_code("Exit code: 1"), 1);
        assert_eq!(detect_exit_code("exit_code 42"), 42);
        // Multi-line with exit code at the end
        assert_eq!(detect_exit_code("line1\nline2\nline3\nExit code: 7"), 7);
    }

    #[test]
    fn test_detect_exit_code_error_patterns() {
        assert_eq!(detect_exit_code("Error: something broke"), 1);
        assert_eq!(detect_exit_code("bash: foo: command not found"), 127);
        assert_eq!(detect_exit_code("All tests passed"), 0);
    }

    #[test]
    fn test_detect_exit_code_ignores_help_text() {
        // I16: Exit code mentioned in help/docs should NOT be matched
        let help_output = "Usage: mycommand [options]\n\
                           Options:\n\
                             --verbose  Enable verbose output\n\
                           Exit codes:\n\
                             exit code 0: success\n\
                             exit code 1: general error\n\
                             exit code 2: misuse";
        assert_eq!(
            detect_exit_code(help_output),
            0,
            "help text mentioning exit codes should not trigger detection"
        );
    }

    #[test]
    fn test_detect_exit_code_window_boundary() {
        // I16 v2: With adaptive window, short outputs (≤10 lines) scan full,
        // and medium outputs (11-100 lines) scan last 20 lines.
        // Exit code at line 1 of 12-line output IS now detected (window=20 covers all).
        // Use a 30-line output to test the boundary: line 1 should be out of window (20).
        let mut lines = Vec::new();
        lines.push("Exit code: 99".to_string()); // line 1 — outside 20-line window
        for i in 0..28 {
            lines.push(format!("normal output line {i}"));
        }
        lines.push("Process finished.".to_string()); // line 30
        let output = lines.join("\n");
        assert_eq!(
            detect_exit_code(&output),
            0,
            "exit code at line 1 of 30-line output should be outside 20-line window"
        );
    }

    #[test]
    fn test_detect_exit_code_real_result_in_tail() {
        // Real exit code at the very end of output
        let output = "Compiling project...\n\
                      Running tests...\n\
                      test result: FAILED\n\
                      3 tests passed, 1 failed\n\
                      Exit code: 1";
        assert_eq!(detect_exit_code(output), 1);
    }

    #[test]
    fn test_detect_exit_code_docs_error_colon() {
        // "Error:" in a line containing "example" (a help marker) should be
        // suppressed, since it's documentation rather than a real error.
        let help = "Description: This tool handles errors.\n\
                    Usage: tool --help\n\
                    Error: shown when invalid input is given (example)";
        assert_eq!(
            detect_exit_code(help),
            0,
            "Error: in a documentation context (with 'example') should be suppressed"
        );

        // But a real "Error:" without help markers should be detected
        let real = "Compiling project...\nError: cannot find type `Foo`";
        assert_eq!(detect_exit_code(real), 1);
    }

    #[test]
    fn test_extract_error_pattern() {
        let output = "Running tests...\nError: assertion failed at line 42\nDone";
        let pattern = extract_error_pattern(output);
        assert!(pattern.unwrap().contains("assertion failed"));
    }

    #[test]
    fn test_extract_error_pattern_none() {
        let output = "Everything is fine\nAll tests passed";
        let pattern = extract_error_pattern(output);
        // No error line → fallback to first line
        assert!(pattern.is_some());
    }

    #[test]
    fn test_extract_file_context() {
        assert_eq!(
            extract_file_context("ruff check src/main.py"),
            Some("src/main.py".to_string())
        );
        assert_eq!(
            extract_file_context("cargo test"),
            None // No file extension match
        );
        assert_eq!(
            extract_file_context("pytest tests/test_foo.py -v"),
            Some("tests/test_foo.py".to_string())
        );
    }

    #[test]
    fn test_truncate() {
        assert_eq!(truncate("short", 10), "short");
        assert_eq!(truncate("a long string here", 10), "a long ...");
    }

    // ── build_bash_outcome ────────────────────────────────────────────

    #[test]
    fn test_build_bash_outcome_success() {
        let outcome = build_bash_outcome("cargo test", "test result: ok. 42 passed").unwrap();
        assert_eq!(outcome.command_short, "cargo");
        assert!(outcome.success);
        assert_eq!(outcome.exit_code, 0);
        assert!(outcome.error_pattern.is_none());
    }

    #[test]
    fn test_build_bash_outcome_failure() {
        let outcome =
            build_bash_outcome("ruff check src/main.py", "Error: E501 line too long").unwrap();
        assert_eq!(outcome.command_short, "ruff");
        assert!(!outcome.success);
        assert_eq!(outcome.exit_code, 1);
        assert!(outcome.error_pattern.is_some());
    }

    #[test]
    fn test_build_bash_outcome_empty_command_returns_none() {
        assert!(build_bash_outcome("", "some output").is_none());
    }

    #[test]
    fn test_build_bash_outcome_captures_file_context() {
        let outcome = build_bash_outcome("pytest tests/test_foo.py -v", "FAILED").unwrap();
        assert_eq!(outcome.file_context, Some("tests/test_foo.py".to_string()));
        assert!(!outcome.success);
    }

    #[test]
    fn test_build_bash_outcome_no_file_context_for_bare_command() {
        let outcome = build_bash_outcome("cargo build", "Compiling...").unwrap();
        assert!(outcome.file_context.is_none());
    }

    #[test]
    fn test_build_bash_outcome_command_not_found() {
        let outcome = build_bash_outcome("foo_tool", "bash: foo_tool: command not found").unwrap();
        assert_eq!(outcome.exit_code, 127);
        assert!(!outcome.success);
    }

    // ── check_error_indicators ───────────────────────────────────────────────

    #[test]
    fn test_check_error_indicators_clean_output() {
        assert_eq!(check_error_indicators("all tests passed\nDone."), 0);
    }

    #[test]
    fn test_check_error_indicators_rust_error_bracket() {
        assert_eq!(check_error_indicators("error[E0308]: mismatched types"), 1);
    }

    #[test]
    fn test_check_error_indicators_failed_keyword() {
        assert_eq!(check_error_indicators("test suite FAILED"), 1);
    }

    #[test]
    fn test_check_error_indicators_error_colon() {
        assert_eq!(
            check_error_indicators("some output\nError: connection refused"),
            1
        );
    }

    #[test]
    fn test_check_error_indicators_error_colon_in_help_is_ignored() {
        // "Error:" inside a help/docs line must NOT return 1
        assert_eq!(check_error_indicators("usage: Error: example"), 0);
    }

    #[test]
    fn test_check_error_indicators_command_not_found() {
        assert_eq!(
            check_error_indicators("bash: foobar: command not found"),
            127
        );
    }

    #[test]
    fn test_check_error_indicators_no_such_file() {
        assert_eq!(check_error_indicators("No such file or directory"), 127);
    }

    // ── E11: build_capture_response ─────────────────────────────────────

    #[test]
    fn test_build_capture_response_allow_on_success_no_capture() {
        let outcome = build_bash_outcome("cargo build", "Compiling...").unwrap();
        let response = build_capture_response(&outcome, None);
        assert_eq!(response, HookResponse::Allow);
    }

    #[test]
    fn test_build_capture_response_context_on_failure() {
        let outcome =
            build_bash_outcome("ruff check src/main.py", "Error: E501 line too long").unwrap();
        let response = build_capture_response(&outcome, None);
        match response {
            HookResponse::Context { ref context, .. } => {
                assert!(
                    context.contains("ruff"),
                    "should mention command: {context}"
                );
                assert!(
                    context.contains("failed"),
                    "should mention failure: {context}"
                );
            }
            _ => panic!("expected Context, got {response:?}"),
        }
    }

    #[test]
    fn test_build_capture_response_context_with_metrics() {
        use std::collections::HashMap;
        let outcome = build_bash_outcome("cargo test", "test result: ok. 42 passed").unwrap();
        let mut metrics = HashMap::new();
        metrics.insert("passed".into(), serde_json::Value::from(42));
        metrics.insert("failed".into(), serde_json::Value::from(0));
        let capture = super::super::output_capture::OutputCapture {
            metrics,
            summary: "42 passed, 0 failed".to_string(),
            exit_code: 0,
            output_bytes: 2048,
            was_captured: true,
            extractor_name: "cargo_test".to_string(),
        };
        let response = build_capture_response(&outcome, Some(&capture));
        match response {
            HookResponse::Context { ref context, .. } => {
                assert!(
                    context.contains("passed=42"),
                    "should contain passed metric: {context}"
                );
                assert!(
                    context.contains("failed=0"),
                    "should contain failed metric: {context}"
                );
                assert!(
                    context.contains("[cargo_test]"),
                    "should contain extractor name: {context}"
                );
            }
            _ => panic!("expected Context, got {response:?}"),
        }
    }

    #[test]
    fn test_build_capture_response_empty_metrics_is_allow() {
        use std::collections::HashMap;
        let outcome = build_bash_outcome("ls -la", "file1\nfile2").unwrap();
        let capture = super::super::output_capture::OutputCapture {
            metrics: HashMap::new(),
            summary: "listed files".to_string(),
            exit_code: 0,
            output_bytes: 2048,
            was_captured: true,
            extractor_name: "generic".to_string(),
        };
        let response = build_capture_response(&outcome, Some(&capture));
        assert_eq!(response, HookResponse::Allow);
    }

    // ── V0e5: one current bash-outcome segment per file ─────────────────

    /// The segment carries the extractor, the UTC minute and the verdict, and escapes
    /// the segment separator `; ` that cargo's summary contains.
    #[test]
    fn test_bash_outcome_note_is_one_owned_segment() {
        let note = bash_outcome_note(
            "cargo_test",
            "test result: FAILED. 1 passed; 2 failed; 0 ignored",
            "10-09T15:42Z",
        );
        assert_eq!(
            note,
            "cargo_test@10-09T15:42Z: test result: FAILED. 1 passed, 2 failed, 0 ignored"
        );
        assert!(!note.contains("; "), "{note}");
        assert!(is_bash_outcome_segment(&note), "owns its own form: {note}");
    }

    /// Every form this writer ever APPENDED is owned — so the replace sweeps the
    /// legacy pile (297 files past 500 bytes on 09/10) — and nothing another writer
    /// stores is.
    #[test]
    fn test_bash_outcome_segment_owns_the_legacy_forms_only() {
        for s in [
            "PASS: 0 passed, 3 failed, 0 errors in 0.20s ===============================",
            "FAIL: 2 passed, 1 failed, 0 errors in 3.45s",
            "UNKNOWN (no pytest summary line): 0 passed, 0 failed, 0 errors",
            "test result: ok. 12 passed",
            "0 failed",
            "2 ignored in 1.20s",
            "ruff: 3 errors (1 fixable)",
            "Exit 0 (OK), 120 lines output (4096 bytes)",
            "bash_metrics[pytest]: duration=\"0.20s\" errors=0 failed=3 passed=0",
            "`pytest t.py` failed (exit 1)",
            "pytest@10-09T15:42Z: FAIL: 0 passed, 3 failed, 0 errors in 0.20s",
        ] {
            assert!(is_bash_outcome_segment(s), "legacy form not owned: {s}");
        }
        for s in [
            "quality: CC_max=4, CC_avg=2.5",
            "generator-nlp: x",
            "Bug with enum",
            "src/a.rs (modify)",
            "owner@example.com",
        ] {
            assert!(
                !is_bash_outcome_segment(s),
                "another writer's segment claimed: {s}"
            );
        }
    }

    /// The defect V0e5 measured, end to end through the real capture and the real DB:
    /// a red run, then a green one on the same file — the head of `notes` (all the
    /// readers show) is the green run, the red one is gone, another writer's stays.
    #[test]
    fn test_newer_bash_outcome_replaces_the_stale_one_at_the_head() {
        let tmp = tempfile::tempdir().unwrap();
        let db = crate::knowledge::FileKnowledgeDB::new(&tmp.path().join("k.db")).unwrap();
        let file = "tests/test_foo.py";
        db.upsert(&crate::knowledge::FileKnowledge {
            file_path: file.to_string(),
            ..Default::default()
        })
        .unwrap();
        db.append_note(file, "quality: CC_max=4").unwrap();
        let pad = "collected line\n".repeat(100);
        let red = format!("{pad}FAILED {file}::test_a\n===== 3 failed in 0.20s =====");
        let green = format!("{pad}===== 4 passed in 0.17s =====");
        let cmd = format!("pytest {file}");
        for (out, code) in [(red.as_str(), 1), (green.as_str(), 0)] {
            let outcome = build_bash_outcome(&cmd, out).unwrap();
            let captured = maybe_capture_and_persist(&db, &cmd, out, code, &outcome);
            assert!(
                captured.is_some(),
                "control: the output crosses the capture threshold"
            );
        }
        let notes = db.lookup(file).unwrap().unwrap().notes.unwrap();
        let head = notes.split("; ").next().unwrap();
        assert!(
            head.starts_with("pytest@") && head.contains("PASS: 4 passed"),
            "{notes}"
        );
        assert!(
            !notes.contains("3 failed"),
            "the stale red run survived: {notes}"
        );
        assert!(
            notes.contains("quality: CC_max=4"),
            "another writer's segment dropped: {notes}"
        );
    }

    // ── E11: format_capture_context ─────────────────────────────────────

    #[test]
    fn test_format_capture_context_test_metrics() {
        use std::collections::HashMap;
        let mut metrics = HashMap::new();
        metrics.insert("passed".into(), serde_json::Value::from(10));
        metrics.insert("failed".into(), serde_json::Value::from(2));
        metrics.insert("ignored".into(), serde_json::Value::from(1));
        metrics.insert("coverage_pct".into(), serde_json::Value::from(92.5));
        let capture = super::super::output_capture::OutputCapture {
            metrics,
            summary: String::new(),
            exit_code: 0,
            output_bytes: 2048,
            was_captured: true,
            extractor_name: "cargo_test".to_string(),
        };
        let ctx = format_capture_context(&capture);
        assert!(ctx.contains("[cargo_test]"), "extractor name: {ctx}");
        assert!(ctx.contains("passed=10"), "passed: {ctx}");
        assert!(ctx.contains("failed=2"), "failed: {ctx}");
        assert!(ctx.contains("ignored=1"), "ignored: {ctx}");
        assert!(ctx.contains("coverage=92.5%"), "coverage: {ctx}");
    }

    #[test]
    fn test_format_capture_context_lint_metrics() {
        use std::collections::HashMap;
        let mut metrics = HashMap::new();
        metrics.insert("errors".into(), serde_json::Value::from(3));
        metrics.insert("fixable".into(), serde_json::Value::from(2));
        let capture = super::super::output_capture::OutputCapture {
            metrics,
            summary: String::new(),
            exit_code: 1,
            output_bytes: 2048,
            was_captured: true,
            extractor_name: "ruff".to_string(),
        };
        let ctx = format_capture_context(&capture);
        assert!(ctx.contains("[ruff]"), "extractor name: {ctx}");
        assert!(ctx.contains("errors=3"), "errors: {ctx}");
        assert!(ctx.contains("fixable=2"), "fixable: {ctx}");
    }

    // ── F10 (2026-09-02): PostToolUse / PostToolUseFailure payload shapes ──
    //
    // Claude Code delivers the command output in `tool_response` (PostToolUse,
    // success only) or in the top-level `error` string (PostToolUseFailure).
    // Before F10 the handler read `tool_input.stdout`, a field the live
    // payload never carries — every outcome recorded as exit 0.

    #[test]
    fn f10_payload_output_reads_tool_response_on_success() {
        let input = serde_json::json!({
            "hook_event_name": "PostToolUse",
            "tool_name": "Bash",
            "tool_input": {"command": "cargo test"},
            "tool_response": {
                "stdout": "test result: ok. 3 passed",
                "stderr": "warning: unused variable",
                "interrupted": false
            }
        });
        let out = bash_payload_output(&input, None);
        assert!(!out.failure_event);
        assert!(!out.interrupted);
        assert!(out.raw_output.contains("3 passed"), "{}", out.raw_output);
        assert!(
            out.raw_output.contains("warning: unused variable"),
            "stderr must reach the analysis: {}",
            out.raw_output
        );
    }

    #[test]
    fn f10_payload_output_uses_error_text_on_failure_event() {
        let input = serde_json::json!({
            "hook_event_name": "PostToolUseFailure",
            "tool_name": "Bash",
            "tool_input": {"command": "npm test", "description": "Run test suite"},
            "tool_use_id": "toolu_01ABC123",
            "error": "Exit code 1\nError: Cannot find module 'express'",
            "is_interrupt": false,
            "duration_ms": 4187
        });
        let out = bash_payload_output(&input, None);
        assert!(out.failure_event);
        assert!(!out.interrupted);
        assert_eq!(
            out.raw_output,
            "Exit code 1\nError: Cannot find module 'express'"
        );
    }

    #[test]
    fn f10_payload_output_flags_a_user_interrupt() {
        let input = serde_json::json!({
            "hook_event_name": "PostToolUseFailure",
            "tool_name": "Bash",
            "tool_input": {"command": "sleep 100"},
            "error": "Command was interrupted",
            "is_interrupt": true
        });
        let out = bash_payload_output(&input, None);
        assert!(out.failure_event);
        assert!(
            out.interrupted,
            "an interrupt is not a command failure to learn from"
        );
    }

    #[test]
    fn f10_payload_output_falls_back_to_legacy_tool_input_stdout() {
        let input = serde_json::json!({
            "tool_input": {"command": "ruff check src/main.py", "stdout": "Error: E501 line too long"}
        });
        let out = bash_payload_output(&input, Some("Error: E501 line too long"));
        assert!(!out.failure_event);
        assert_eq!(out.raw_output, "Error: E501 line too long");
    }

    #[test]
    fn f10_failure_event_outcome_never_reads_as_success() {
        let output = BashPayloadOutput {
            raw_output: "boom".to_string(),
            failure_event: true,
            success_event: false,
            exit_code_hint: None,
            interrupted: false,
        };
        let outcome = build_bash_outcome_for_event("make deploy", &output).unwrap();
        assert!(!outcome.success);
        assert_eq!(outcome.exit_code, 1, "unknown non-zero exit collapses to 1");
    }

    #[test]
    fn f10_failure_event_outcome_parses_the_exit_code_line() {
        let output = BashPayloadOutput {
            raw_output: "Exit code 101\nthread 'main' panicked at src/lib.rs:4".to_string(),
            failure_event: true,
            success_event: false,
            exit_code_hint: None,
            interrupted: false,
        };
        let outcome = build_bash_outcome_for_event("cargo run", &output).unwrap();
        assert!(!outcome.success);
        assert_eq!(outcome.exit_code, 101);
    }

    /// S21 reverses the old F10 assumption: a SUCCESS event (PostToolUse) whose text shows
    /// errors ran with rc 0 — `cargo check` that failed would have come as PostToolUseFailure.
    #[test]
    fn f10_success_event_with_error_text_is_success_not_failure() {
        let output = BashPayloadOutput {
            raw_output: "error[E0308]: mismatched types".to_string(),
            failure_event: false,
            success_event: true,
            exit_code_hint: None,
            interrupted: false,
        };
        let outcome = build_bash_outcome_for_event("cargo check", &output).unwrap();
        assert!(outcome.success);
        assert!(
            outcome.error_pattern.is_none(),
            "no pipe → no masking suspicion"
        );
    }
}

/// S21 (plan 2026-09-25) — PostToolUse fires only when the Bash call SUCCEEDED (Claude Code
/// routes failures to PostToolUseFailure), so its text never turns it into a failure.
/// Measured: `printf 'Exit code 3'` (rc 0) was recorded and shown as "failed (exit 3)";
/// 6 of 7 attributions in one session were false.
#[cfg(test)]
mod s21_tests {
    use super::*;

    fn post(command: &str, stdout: &str) -> serde_json::Value {
        serde_json::json!({
            "hook_event_name": "PostToolUse",
            "tool_input": {"command": command},
            "tool_response": {"stdout": stdout, "stderr": "", "interrupted": false}
        })
    }

    fn outcome_of(input: &serde_json::Value) -> BashOutcome {
        let cmd = input
            .pointer("/tool_input/command")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        build_bash_outcome_for_event(cmd, &bash_payload_output(input, None)).expect("outcome")
    }

    #[test]
    fn post_tool_use_success_never_inferred_as_failure_from_text() {
        for text in [
            "Exit code 3\n",
            "error: data\n",
            "Traceback (most recent call last):\nValueError: x\n",
        ] {
            let o = outcome_of(&post("printf %s x", text));
            assert!(
                o.success,
                "rc-0 call recorded as failure from text {text:?}"
            );
            assert_eq!(o.exit_code, 0);
        }
    }

    #[test]
    fn masked_pipeline_is_labelled_suspect_not_failed() {
        let o = outcome_of(&post(
            "cargo check 2>&1 | tail -5",
            "error[E0308]: mismatched types\n",
        ));
        assert!(
            o.success,
            "a pipe's rc 0 is what ran — never recorded as a failure"
        );
        let label = o.error_pattern.expect("the suspicion is labelled");
        assert!(label.starts_with("masked_failure_suspected"), "{label}");
        // With pipefail the rc is honest: no suspicion.
        let honest = outcome_of(&post(
            "set -o pipefail; cargo check 2>&1 | tail -5",
            "error[E0308]: x\n",
        ));
        assert!(honest.error_pattern.is_none());
    }

    #[test]
    fn failure_event_still_records_exit_code() {
        let input = serde_json::json!({
            "hook_event_name": "PostToolUseFailure",
            "tool_input": {"command": "cargo test"},
            "error": "Exit code 101\nthread 'main' panicked"
        });
        let o = outcome_of(&input);
        assert!(!o.success);
        assert_eq!(o.exit_code, 101);
    }

    #[test]
    fn explicit_exit_code_hint_still_wins_on_post_event() {
        let input = serde_json::json!({
            "hook_event_name": "PostToolUse",
            "tool_input": {"command": "make"},
            "tool_response": {"stdout": "", "stderr": "", "exit_code": 2}
        });
        let o = outcome_of(&input);
        assert!(!o.success);
        assert_eq!(o.exit_code, 2);
    }

    #[test]
    fn command_short_skips_env_assignments_and_cd() {
        use super::super::pre_bash::extract_command_short as short;
        assert_eq!(short("FOO=1 cargo test"), "cargo");
        assert_eq!(short("cd /x && TOURING_CODE_MODE=native cat f"), "cat");
        assert_eq!(short("S=/tmp/a; python3 s.py"), "python3");
        assert_eq!(
            short("env -u TOURING_DAEMON_SOCKET touring doctor"),
            "touring"
        );
        assert_eq!(short("timeout -s KILL 30 cargo test"), "cargo");
        assert_eq!(short("sudo -E apt update"), "apt");
        assert_eq!(short("for f in *; do grep x $f; done"), "for");
        assert_eq!(short("cd /repo && git status"), "git");
    }
}
