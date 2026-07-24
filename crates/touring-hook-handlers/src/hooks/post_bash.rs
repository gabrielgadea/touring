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
use crate::schemas::{validate_payload, validation_deny};
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
        Err(errors) => return validation_deny(&errors, "post_bash"),
    };
    let command = validated.command.as_str();
    let raw_output = validated.stdout.as_deref().unwrap_or("");

    if command.is_empty() {
        return HookResponse::Allow;
    }

    let outcome = match build_bash_outcome(command, raw_output) {
        Some(o) => o,
        None => return HookResponse::Allow,
    };

    if let Err(e) = runtime.ctx.knowledge.record_bash_outcome(&outcome) {
        tracing::debug!(
            "record_bash_outcome failed for '{}': {e}",
            outcome.command_short
        );
    }
    // EC6: Fire-and-forget async record_bash_outcome to AsyncFileKnowledgeDB.
    // Runs alongside the sync call above — non-blocking, same outcome, async persistence path.
    // Uses Handle::try_current() — safe from tokio::task::spawn_blocking context.
    if let Some(adb) = runtime.ctx.async_knowledge.as_ref().cloned() {
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            let outcome_clone = outcome.clone();
            drop(handle.spawn(async move {
                let _ = adb.record_bash_outcome(&outcome_clone).await;
            }));
        }
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

    // Cross-cutting: if command failed and involves a file, append note to that file.
    if !outcome.success {
        maybe_append_failure_note(&runtime.ctx.knowledge, &outcome);
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
            let enriched = crate::shared::signals::enrich_with_cognitive(cog, path_ref, false);
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
fn build_bash_outcome(command: &str, raw_output: &str) -> Option<BashOutcome> {
    let command_short = extract_command_short(command);
    if command_short.is_empty() {
        return None;
    }

    let output_head = truncate_str(raw_output, 2000);
    let exit_code = detect_exit_code(output_head);
    // FIX8: Also detect error indicators when exit_code==0 — some tools
    // (e.g. `cargo test` capturing panics) print "FAILED" or "error[" but
    // still exit 0. Checking indicators catches these false-success cases.
    let has_error_indicators = check_error_indicators(output_head) != 0;
    let success = exit_code == 0 && !has_error_indicators;
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
        // Persist summary as file note.
        if let Err(e) = db.append_note(file_ctx, &capture.summary) {
            tracing::debug!("append_note (capture) failed for {file_ctx}: {e}");
        }

        // Persist key metrics as structured note for future pre-bash recall.
        if !capture.metrics.is_empty() {
            let metrics_note = format_metrics_note(&capture);
            if let Err(e) = db.append_note(file_ctx, &metrics_note) {
                tracing::debug!("append_note (metrics) failed for {file_ctx}: {e}");
            }
        }
    }

    Some(capture)
}

/// Format OutputCapture metrics as a compact note for knowledge persistence.
fn format_metrics_note(capture: &super::output_capture::OutputCapture) -> String {
    use std::fmt::Write;
    let mut note = format!("bash_metrics[{}]:", capture.extractor_name);
    // Deterministic ordering for stable output.
    let mut keys: Vec<&String> = capture.metrics.keys().collect();
    keys.sort();
    for key in keys {
        if let Some(val) = capture.metrics.get(key) {
            let _ = write!(note, " {key}={val}");
        }
    }
    note
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

    // Failed command: include error pattern for immediate feedback.
    if !outcome.success {
        let error_desc = outcome.error_pattern.as_deref().unwrap_or("unknown error");
        parts.push(format!(
            "`{}` failed (exit {}): {}",
            outcome.command_short, outcome.exit_code, error_desc
        ));
    }

    // Structured metrics from OutputCapture (test counts, coverage, lint errors).
    if let Some(cap) = capture {
        if !cap.metrics.is_empty() {
            parts.push(format_capture_context(cap));
        }
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

/// Append a failure note to the file context when a command fails.
///
/// Only fires when the outcome has a `file_context` so the note is
/// attributed to the right file in the knowledge graph.
fn maybe_append_failure_note(db: &super::knowledge::FileKnowledgeDB, outcome: &BashOutcome) {
    if let Some(ref file_ctx) = outcome.file_context {
        let note = format!(
            "`{}` failed (exit {})",
            truncate(&outcome.command, 60),
            outcome.exit_code
        );
        if let Err(e) = db.append_note(file_ctx, &note) {
            tracing::debug!("append_note (failure) failed for {file_ctx}: {e}");
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

    // ── E11: format_metrics_note ────────────────────────────────────────

    #[test]
    fn test_format_metrics_note_sorted_keys() {
        use std::collections::HashMap;
        let mut metrics = HashMap::new();
        metrics.insert("failed".into(), serde_json::Value::from(1));
        metrics.insert("passed".into(), serde_json::Value::from(5));
        metrics.insert("coverage_pct".into(), serde_json::Value::from(85.0));
        let capture = super::super::output_capture::OutputCapture {
            metrics,
            summary: "test summary".to_string(),
            exit_code: 0,
            output_bytes: 2048,
            was_captured: true,
            extractor_name: "cargo_test".to_string(),
        };
        let note = format_metrics_note(&capture);
        assert!(
            note.starts_with("bash_metrics[cargo_test]:"),
            "prefix: {note}"
        );
        // Keys should be sorted alphabetically
        let coverage_pos = note.find("coverage_pct").expect("has coverage");
        let failed_pos = note.find("failed").expect("has failed");
        let passed_pos = note.find("passed").expect("has passed");
        assert!(coverage_pos < failed_pos, "coverage before failed: {note}");
        assert!(failed_pos < passed_pos, "failed before passed: {note}");
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
}
