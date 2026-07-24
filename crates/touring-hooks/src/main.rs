//! touring-hook — Native Rust binary for Claude Code hooks.
//!
//! Single binary, multiple subcommands. Replaces Python/Bash hook scripts.
//! Startup: <1ms (vs ~40ms Python). Shared HookRuntime initialization.
//! main() CC=51 (lifecycle dispatch via alternation pattern, v29.5).
//!
//! Usage: touring-hook \<subcommand\>
//!
//! Subcommands (pre/post hooks):
//!   pre-read, post-read, pre-bash, post-bash,
//!   pre-edit, post-edit, post-tool-rl, pre-edit-prevention, pre-write, post-write
//!
//! Session & stateless:
//!   session-start, session-stop, prompt-enhance, qa-syntax,
//!   classify, pii-scan, session-audit, session-insights
//!
//! Agent Teams hooks:
//!   teammate-idle, task-completed, task-created
//!
//! Lifecycle passthrough (hyphen→underscore event name, all share run_lifecycle_event):
//!   post-tool-failure, subagent-start, subagent-stop, pre-compact, post-compact,
//!   session-end, config-change, permission-request, instructions-loaded,
//!   worktree-create, worktree-remove, notification, setup, elicitation,
//!   elicitation-result, cwd-changed, file-changed, stop-failure
//!
//! Daemon management:
//!   --start-daemon  (runs daemon in-process via run_daemon_async)
//!   --daemon-health (direct socket probe, bypasses circuit breaker)

use std::process;

use serde_json::Value;
use touring_hooks::ipc::{DaemonRequest, DaemonResponse, daemon_socket_path};
use touring_hooks::runtime::HookRuntime;

fn main() {
    // Sprint 3 PC-1 (REGRA #19): set kernel-visible comm BEFORE tracing init.
    // We do an early peek at argv[1] to pick the right identity: the legacy
    // `--start-daemon` path runs the in-process daemon, every other
    // invocation is an ephemeral hook handler (pre-edit, post-bash, etc.).
    {
        let early_args: Vec<String> = std::env::args().collect();
        let early_subcmd = early_args.get(1).map(String::as_str).unwrap_or("");
        let comm = if early_subcmd == "--start-daemon" {
            "touring-daemon"
        } else {
            "touring-hook"
        };
        touring_hooks::proc_identity::set_process_name(comm);
    }

    // Initialize tracing subscriber — sink to stderr, controlled by TOURING_LOG env var.
    // Without this, all #[tracing::instrument] spans produce zero output.
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(tracing_subscriber::EnvFilter::from_env("TOURING_LOG"))
        .init();

    let args: Vec<String> = std::env::args().collect();
    let subcommand = args.get(1).map(String::as_str).unwrap_or("help");

    // Help does not need stdin
    if matches!(subcommand, "help" | "--help" | "-h") {
        print_help();
        process::exit(0);
    }

    // Sprint 4 PD-3 (2026-05-23, REGRA #19): the legacy polymorphic
    // `touring-hook --start-daemon` daemon path is DEPRECATED. The
    // dedicated `touring-daemon` binary (touring-hooks/src/daemon_main.rs)
    // is the canonical daemon since PD-1 (update-touring spawn) +
    // PD-2 (cli/daemon_ctl spawn). This branch now refuses with a
    // clear migration message rather than silently spawning a daemon
    // under the wrong identity (would defeat REGRA #19 process census).
    if subcommand == "--start-daemon" {
        eprintln!(
            "[touring-hook] error: --start-daemon is DEPRECATED (Sprint 4 PD-3, REGRA #19).\n\
             Use the dedicated `touring-daemon` binary directly:\n  \
               $HOME/.local/bin/touring-daemon\n\
             Or via the canonical helper:\n  \
               touring daemon-ctl restart"
        );
        process::exit(2);
    }

    // S9: Health check — query daemon status without reading stdin.
    // Uses try_daemon_health_direct() which bypasses the circuit breaker:
    // health checks are diagnostic tools and must not be blocked by, or
    // contribute to, circuit state (a down daemon ≠ a reason to block diagnosis).
    if matches!(subcommand, "--daemon-health" | "daemon-health") {
        if let Some(resp) = try_daemon_health_direct() {
            println!("{}", resp.output);
        } else {
            eprintln!("Daemon not available");
            process::exit(1);
        }
        process::exit(0);
    }

    // Read stdin JSON (shared across all subcommands)
    let input: serde_json::Value = match HookRuntime::read_stdin() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("[touring-hook] stdin error: {e}");
            // Fail open — don't block Claude Code
            process::exit(0);
        }
    };

    // ── Daemon fast-path ─────────────────────────────────────────────────
    // Pre-hooks and post-hooks that the daemon handles are attempted via socket
    // first. If the daemon is unavailable (not started, crashed, timeout),
    // we fall through to the standalone path below — no blocking, always exit 0.
    //
    // S10: Single source of truth — hook_registry::ALL_DAEMON_HOOK_NAMES.
    // Stateless subcommands (prompt-enhance, qa-syntax, classify, …) always run standalone.
    if touring_hooks::hook_registry::ALL_DAEMON_HOOK_NAMES.contains(&subcommand)
        && std::env::var("TOURING_NO_DAEMON").is_err()
    {
        let project_root = HookRuntime::detect_project_root()
            .to_string_lossy()
            .into_owned();

        // Extract session_id from Claude Code hook JSON input.
        // Previously hardcoded to None, making session-level circuit breaker dead code.
        let session_id = input
            .get("session_id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let req = DaemonRequest {
            hook: subcommand.to_string(),
            payload: input.clone(),
            project_root,
            session_id,
            priority: 128,
        };

        if let Some(resp) = try_daemon_request(&req) {
            // Daemon handled the request — print output (if any) and exit.
            //
            // Defense-in-depth: validate that `resp.output` is either empty
            // OR a parseable JSON object before forwarding to Claude Code's
            // strict hook-output schema validator. Anything else (partial
            // response from a crashed daemon, accidental stdout
            // contamination, mixed prose+JSON) is silently swallowed and we
            // fall back to the canonical "{}" Allow shape — preserving the
            // "hooks never block CC" invariant.
            if resp.output.is_empty() {
                process::exit(0);
            }
            let trimmed = resp.output.trim();
            if serde_json::from_str::<serde_json::Value>(trimmed).is_ok() {
                println!("{}", resp.output);
            } else {
                eprintln!(
                    "[touring-hook] daemon returned non-JSON output ({} bytes), \
                     emitting canonical {{}} Allow instead",
                    resp.output.len()
                );
                println!("{{}}");
            }
            process::exit(0);
        }
        // Daemon unavailable — fall through to standalone execution below.
        tracing::debug!(hook = subcommand, "daemon unavailable, running standalone");
    }

    // Stateless subcommands — no HookRuntime needed.
    // These diverge (call process::exit internally) so we match and never fall through.
    match subcommand {
        "prompt-enhance" => run_prompt_enhance(&input),
        #[cfg(feature = "utilities")]
        "qa-syntax" => run_qa_syntax(&input),
        _ => {}
    }

    // Detect project root
    let project_root = HookRuntime::detect_project_root();

    // Initialize HookRuntime (shared state: SQLite, classifier, PII scanner)
    #[allow(unused_mut)] // mut needed only when session-hooks feature is enabled
    let mut runtime = match HookRuntime::new(&project_root) {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("[touring-hook] runtime init error: {e}");
            // Fail open
            process::exit(0);
        }
    };

    // Dispatch to the appropriate hook module
    let result = match subcommand {
        // Pre-hooks (inject context before tool execution)
        #[cfg(feature = "pre-hooks")]
        "pre-read" => touring_hooks::pre_read::run(&runtime, &input),
        #[cfg(feature = "pre-hooks")]
        "pre-bash" => touring_hooks::pre_bash::run(&runtime, &input),
        #[cfg(feature = "pre-hooks")]
        "pre-edit" => touring_hooks::pre_edit::run(&runtime, &input),
        #[cfg(feature = "pre-hooks")]
        "pre-edit-prevention" => touring_hooks::pre_edit_prevention::run(&runtime, &input),
        #[cfg(feature = "pre-hooks")]
        "pre-grep" => touring_hooks::pre_grep::run(&runtime, &input),
        #[cfg(feature = "pre-hooks")]
        "pre-glob" => touring_hooks::pre_glob::run(&runtime, &input),
        // CEG Pln2 P3.7 — the Code Execution Gateway pre-exec hook. Advisory by
        // default (observe mode); `CEG_ENFORCE=1` opts into hard deny.
        #[cfg(feature = "pre-hooks")]
        "pre-exec" => touring_hooks::ceg_adapter::run(&mut runtime, &input),
        // S-01 (elite-harness 2026-05-29) — the universal CEG observability
        // hook. Wired in settings.json against *every* Bash; routes the action
        // through X0..X7 for `ceg_*` counter side-effects only, fail-open, and
        // never alters the response. Makes `touring gate-metrics -j` reflect
        // the whole action stream, not just `touring exec`.
        #[cfg(feature = "pre-hooks")]
        "ceg-observe" => touring_hooks::ceg_adapter::run_observe_only(&input).emit(),
        #[cfg(feature = "pre-hooks")]
        "pre-write" => touring_hooks::pre_write::run(&mut runtime, &input),
        // PreToolUse classifier: suggests the best Touring CLI command(s) for
        // (tool_name, tool_input). Runs as a `pre-*` event from settings.json
        // wired on Bash|Grep|Glob|Read|Edit|Write.
        #[cfg(feature = "pre-hooks")]
        "cli-suggest" => {
            let _ = touring_hooks::cli_suggester::run(&runtime, &input);
            Ok(())
        }

        // Post-hooks (capture outcomes after tool execution)
        #[cfg(feature = "post-hooks")]
        "post-read" => touring_hooks::post_read::run(&runtime, &input),
        #[cfg(feature = "post-hooks")]
        "post-bash" => touring_hooks::post_bash::run(&mut runtime, &input),
        #[cfg(feature = "post-hooks")]
        "post-edit" => touring_hooks::post_edit::run(&mut runtime, &input),
        #[cfg(feature = "post-hooks")]
        "post-write" => touring_hooks::post_write::run(&runtime, &input),
        #[cfg(feature = "post-hooks")]
        "post-tool-batch" => {
            touring_hooks::post_tool_batch::run_post_tool_batch(&mut runtime, &input).emit()
        }
        #[cfg(feature = "post-hooks")]
        "post-tool-rl" => touring_hooks::post_tool_rl::run(&mut runtime, &input),

        // Session lifecycle
        #[cfg(feature = "session-hooks")]
        "session-start" => touring_hooks::session_hooks::run_session_start(&mut runtime, &input)
            .map_err(touring_hook_runtime::hook_runtime::HookDispatchError::from),
        #[cfg(feature = "session-hooks")]
        "session-stop" => touring_hooks::session_hooks::run_session_stop(&mut runtime, &input)
            .map_err(touring_hook_runtime::hook_runtime::HookDispatchError::from),

        // Permission-request hook — VGP auto-approve for low-risk tool invocations.
        "permission-request" => touring_hooks::permission_request::run(&runtime, &input),

        // Lifecycle passthrough subcommands — record events in knowledge DB.
        // All share the same handler; event name is normalized (hyphens → underscores).
        // Adding a new lifecycle event = one line in this pattern, zero new branches.
        "post-tool-failure"
        | "subagent-start"
        | "post-compact"
        | "session-end"
        | "config-change"
        | "instructions-loaded"
        | "worktree-create"
        | "worktree-remove"
        | "notification"
        | "setup"
        | "elicitation"
        | "elicitation-result"
        | "cwd-changed"
        | "file-changed"
        | "stop-failure" => {
            // Normalize hyphen-separated subcommand name → underscore DB event key.
            let event_name = subcommand.replace('-', "_");
            run_lifecycle_event(&runtime, &input, &event_name)
        }

        // Agent Teams hooks — dedicated handlers (not simple lifecycle passthrough)
        "teammate-idle" => touring_hooks::team_hooks::run_teammate_idle(&mut runtime, &input)
            .map_err(touring_hook_runtime::hook_runtime::HookDispatchError::from),
        "task-completed" => touring_hooks::team_hooks::run_task_completed(&mut runtime, &input)
            .map_err(touring_hook_runtime::hook_runtime::HookDispatchError::from),
        "task-created" => touring_hooks::team_hooks::run_task_created(&mut runtime, &input)
            .map_err(touring_hook_runtime::hook_runtime::HookDispatchError::from),
        "subagent-stop" => {
            let result = touring_hooks::team_hooks::run_subagent_stop_gate(&mut runtime, &input);
            if !result.feedback.is_empty() {
                eprintln!("{}", result.feedback);
            }
            process::exit(result.exit_code as i32);
        }
        "task-validation" => {
            let resp =
                touring_hooks::hooks_task_lifecycle::run_task_validation(&mut runtime, &input);
            match resp.to_json().is_empty() {
                true => Ok(()),
                false => {
                    println!("{}", resp.to_json());
                    Ok(())
                }
            }
        }

        // Utility subcommands
        "classify" => run_classify(&runtime, &input),
        "pii-scan" => run_pii_scan(&runtime, &input),
        "session-audit" => run_session_audit(&input),
        "metrics" => run_metrics(&runtime),
        #[cfg(feature = "session-hooks")]
        "session-insights" => run_session_insights(&runtime, &input),

        // S-3: Decompose-event subcommand — session-scoped task lifecycle
        "decompose-event" => run_decompose_event(&mut runtime, &input),

        unknown => {
            eprintln!("[touring-hook] unknown subcommand: {unknown}");
            eprintln!("Run `touring-hook help` for usage.");
            // Fail open — never block Claude Code, even for unknown hooks.
            // Claude Code may register new hook events before we add handlers.
            process::exit(0);
        }
    };

    // Handle result — post-hooks return Ok(()), pre-hooks exit internally
    match result {
        Ok(()) => process::exit(0),
        Err(e) => {
            eprintln!("[touring-hook] error in {subcommand}: {e}");
            // Fail open — never block Claude Code
            process::exit(0);
        }
    }
}

/// Prompt enhancement — stateless, no HookRuntime needed.
/// Diverges: prints JSON to stdout and exits.
fn run_prompt_enhance(input: &Value) -> ! {
    // UserPromptSubmit sends {"prompt": "...", "session_id": "..."} at root level.
    // Fallback: nested format {"input": {"message": "..."}} used by some older integrations.
    let prompt = input
        .get("prompt")
        .or_else(|| input.pointer("/input/message"))
        .or_else(|| input.get("input"))
        .and_then(|v| v.as_str())
        .unwrap_or("");

    if prompt.is_empty() {
        // No prompt to enhance — pass through
        process::exit(0);
    }

    let output = touring_hooks::prompt_enhance::compose_json(prompt);
    println!("{}", serde_json::to_string(&output).unwrap_or_default());
    process::exit(0);
}

/// Python syntax validation — stateless.
/// Diverges: prints JSON to stdout and exits.
#[cfg(feature = "utilities")]
fn run_qa_syntax(input: &Value) -> ! {
    let source = input
        .pointer("/tool_input/content")
        .or_else(|| input.get("source"))
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let result = touring_hooks::qa_syntax::validate_python_syntax(source);
    let output = serde_json::json!({
        "valid": result.valid,
        "error": result.error,
    });
    println!("{}", serde_json::to_string(&output).unwrap_or_default());
    process::exit(0);
}

/// Intent classification utility.
fn run_classify(
    runtime: &HookRuntime,
    input: &Value,
) -> Result<(), touring_hook_runtime::hook_runtime::HookDispatchError> {
    let text = input
        .get("text")
        .or_else(|| input.get("input"))
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let cila = runtime.ctx.classifier.classify(text);
    let techniques: Vec<touring_hooks::classifier::CognitiveTechnique> =
        touring_hooks::classifier::IntentClassifier::get_techniques(cila.level);
    let output = serde_json::json!({
        "level": cila.level,
        "level_name": cila.level_name,
        "routing_strategy": cila.routing_strategy,
        "requires_pipeline": cila.requires_pipeline,
        "requires_code_first": cila.requires_code_first,
        "matched_pattern": cila.matched_pattern,
        "techniques": techniques.iter().map(|t| t.hint()).collect::<Vec<_>>(),
    });
    println!("{}", serde_json::to_string(&output).unwrap_or_default());
    Ok(())
}

/// PII scanning utility.
fn run_pii_scan(
    runtime: &HookRuntime,
    input: &Value,
) -> Result<(), touring_hook_runtime::hook_runtime::HookDispatchError> {
    let content = input.get("content").and_then(|v| v.as_str()).unwrap_or("");

    let findings = runtime.ctx.pii_scanner.scan_text(content);
    let output = serde_json::json!({
        "pii_detected": !findings.is_empty(),
        "count": findings.len(),
        "findings": findings.iter().map(|f| serde_json::json!({
            "pattern_name": f.pattern_name,
            "line_number": f.line_number,
            "matched_text": f.matched_text,
            "column": f.column,
            "severity": f.severity,
        })).collect::<Vec<_>>(),
    });
    println!("{}", serde_json::to_string(&output).unwrap_or_default());
    Ok(())
}

/// Session audit — hash CLAUDE.md and .claude/rules/ and report drift.
fn run_session_audit(
    input: &Value,
) -> Result<(), touring_hook_runtime::hook_runtime::HookDispatchError> {
    use std::path::PathBuf;
    use touring_hooks::audit;

    let session_id = input
        .get("session_id")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    let agent_type = input
        .get("agent_type")
        .and_then(|v| v.as_str())
        .map(str::to_string);

    // Canonical instruction files to audit
    let home = std::env::var("HOME").unwrap_or_default();
    let claude_dir = PathBuf::from(&home).join(".claude");
    let mut files: Vec<PathBuf> = vec![
        claude_dir.join("CLAUDE.md"),
        claude_dir.join("rules").join("00-core-governance.md"),
        claude_dir.join("rules").join("taco-identity.md"),
    ];

    // Include any .md files in .claude/rules/ that exist
    if let Ok(entries) = std::fs::read_dir(claude_dir.join("rules")) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.extension().and_then(|e| e.to_str()) == Some("md") && !files.contains(&p) {
                files.push(p);
            }
        }
    }
    files.sort();

    let event = audit::build_event(&files, session_id, agent_type);
    let file_hashes: Vec<serde_json::Value> = event
        .files
        .iter()
        .map(|fh| {
            serde_json::json!({
                "path": fh.path.display().to_string(),
                "hash": fh.hash,
            })
        })
        .collect();

    let output = serde_json::json!({
        "timestamp": event.timestamp,
        "session_id": event.session_id,
        "agent_type": event.agent_type,
        "composite_hash": event.composite_hash,
        "files": file_hashes,
        "files_audited": event.files.len(),
    });
    println!("{}", serde_json::to_string(&output).unwrap_or_default());
    Ok(())
}

/// S-3: Decompose-event handler for standalone binary invocation.
/// Bridges to cli_decompose_event handler via HookRuntime state.
/// Input: {event_type, session_id, task_data}
fn run_decompose_event(
    runtime: &mut HookRuntime,
    input: &Value,
) -> Result<(), touring_hook_runtime::hook_runtime::HookDispatchError> {
    let output = touring_hooks::cli_handlers::cli_decompose_event(runtime, input);
    println!("{}", output);
    Ok(())
}

/// R19: Export consolidated runtime metrics as JSON.
fn run_metrics(
    runtime: &HookRuntime,
) -> Result<(), touring_hook_runtime::hook_runtime::HookDispatchError> {
    let metrics = runtime.export_metrics(None);
    println!(
        "{}",
        serde_json::to_string_pretty(&metrics).unwrap_or_default()
    );
    Ok(())
}

/// Session insights utility.
#[cfg(feature = "session-hooks")]
fn run_session_insights(
    runtime: &HookRuntime,
    input: &Value,
) -> Result<(), touring_hook_runtime::hook_runtime::HookDispatchError> {
    let session_id = input
        .get("session_id")
        .and_then(|v| v.as_str())
        .unwrap_or("current");

    let insights = touring_hooks::session_insights::extract_session_insights(
        &runtime.ctx.knowledge,
        session_id,
    );
    let output = serde_json::json!({
        "session_id": insights.session_id,
        "timestamp": insights.timestamp,
        "total_edits": insights.total_edits,
        "total_bash_commands": insights.total_bash_commands,
        "success_rate": insights.success_rate,
        "top_gotchas": insights.top_gotchas.iter().map(|g| serde_json::json!({
            "pattern": g.pattern,
            "gotcha": g.gotcha,
            "hit_count": g.hit_count,
        })).collect::<Vec<Value>>(),
        "top_error_patterns": insights.top_error_patterns.iter().map(|e| serde_json::json!({
            "pattern": e.pattern,
            "occurrences": e.occurrences,
        })).collect::<Vec<Value>>(),
        "top_edited_files": insights.top_edited_files.iter().map(|f| serde_json::json!({
            "file_path": f.file_path,
            "edit_count": f.edit_count,
        })).collect::<Vec<Value>>(),
    });
    println!("{}", serde_json::to_string(&output).unwrap_or_default());
    Ok(())
}

/// Lifecycle event passthrough — records the event in the knowledge DB.
///
/// For events that have full Cortex handlers in touring-server, this provides
/// a lightweight persistence path via the touring-hook binary. It records the
/// event type and key metadata into the knowledge DB for session analytics.
fn run_lifecycle_event(
    runtime: &touring_hooks::runtime::HookRuntime,
    input: &Value,
    event_name: &str,
) -> Result<(), touring_hook_runtime::hook_runtime::HookDispatchError> {
    // Record the lifecycle event in knowledge DB
    let session_id = input
        .get("session_id")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");

    let _ = runtime
        .ctx
        .knowledge
        .record_access(&format!("__{}__", event_name), session_id);

    // Extract and log any summary information
    let summary = input
        .get("message")
        .or_else(|| input.get("description"))
        .or_else(|| input.get("result_summary"))
        .and_then(|v| v.as_str())
        .unwrap_or("");

    if !summary.is_empty() {
        let _ = runtime
            .ctx
            .knowledge
            .record_bash_outcome(&touring_hooks::knowledge::BashOutcome {
                command: format!("{}:{}", event_name, &summary[..summary.len().min(200)]),
                command_short: event_name.to_string(),
                exit_code: 0,
                success: true,
                error_pattern: None,
                file_context: None,
                command_hash: String::new(),
                executed_at: String::new(),
            });
    }

    tracing::debug!(event = event_name, "Lifecycle event recorded");
    Ok(())
}

/// Attempt to send a hook request to the running daemon via Unix socket.
///
/// Health-check variant that bypasses the circuit breaker entirely.
///
/// Health checks are diagnostic tools — they must report the real daemon state
/// regardless of circuit state, and must NOT contribute failures to the circuit.
/// A daemon being down is exactly the condition health checks exist to detect.
fn try_daemon_health_direct() -> Option<DaemonResponse> {
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::net::UnixStream;

    let socket_path = daemon_socket_path();

    // Ensure the daemon socket is ready; health probes wait up to 300ms for
    // initial autostart (same as the orphan-restart branch).
    if !ensure_daemon_socket(&socket_path, 300) {
        return None;
    }

    let req = DaemonRequest {
        hook: "__health__".to_string(),
        payload: serde_json::Value::Null,
        project_root: std::env::current_dir()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned(),
        session_id: None,
        priority: 0,
    };

    let stream = UnixStream::connect(&socket_path).ok()?;
    stream
        .set_read_timeout(Some(std::time::Duration::from_millis(2000)))
        .ok()?;
    stream
        .set_write_timeout(Some(std::time::Duration::from_millis(2000)))
        .ok()?;

    let mut writer = stream.try_clone().ok()?;
    let reader_stream = stream;

    let req_json = serde_json::to_string(&req).ok()?;
    if writer.write_all(req_json.as_bytes()).is_err() || writer.write_all(b"\n").is_err() {
        return None;
    }

    let mut reader = BufReader::new(reader_stream);
    let mut line = String::new();
    reader.read_line(&mut line).ok()?;
    // No record_failure / record_success — health checks must not pollute circuit state.
    serde_json::from_str::<DaemonResponse>(line.trim()).ok()
}

/// Returns `Some(DaemonResponse)` on success, `None` if the daemon is
/// unavailable or the request times out. The caller falls back to standalone
/// execution on `None` — the invariant "hooks never block Claude Code" is preserved.
fn try_daemon_request(req: &DaemonRequest) -> Option<DaemonResponse> {
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::net::UnixStream;

    // S14: Circuit breaker — hierarchical multi-dimensional check.
    // Uses check() instead of is_open() to leverage per-hook, per-project,
    // and per-session circuit dimensions (is_open() was a flat global check).
    let circuit = touring_hooks::circuit_breaker::check(
        &req.hook,
        Some(&req.project_root),
        req.session_id.as_deref(),
    );
    if circuit.skip {
        tracing::debug!(
            hook = %req.hook,
            circuit = circuit.circuit,
            reason = %circuit.reason,
            retry_after = circuit.retry_after_secs,
            "circuit breaker open — skipping daemon"
        );
        return None;
    }

    let socket_path = daemon_socket_path();

    // Ensure the daemon socket is ready; regular requests give 200ms for initial
    // autostart (tighter than health probes — fallback to standalone is fast).
    if !ensure_daemon_socket(&socket_path, 200) {
        return None;
    }

    // Connect with a 100ms timeout
    let stream = match UnixStream::connect(&socket_path) {
        Ok(s) => s,
        Err(_) => {
            touring_hooks::circuit_breaker::record_failure(
                &req.hook,
                Some(&req.project_root),
                req.session_id.as_deref(),
                false,
            );
            return None;
        }
    };
    // 3s timeout — session-start and post-tool-rl can take up to ~500ms on first
    // call; 100ms was too tight and caused spurious standalone fallbacks that
    // raced with the daemon's in-progress SQLite write (reviewer issue I3).
    stream
        .set_read_timeout(Some(std::time::Duration::from_millis(3000)))
        .ok()?;
    stream
        .set_write_timeout(Some(std::time::Duration::from_millis(3000)))
        .ok()?;

    let mut writer = stream.try_clone().ok()?;
    let reader_stream = stream;

    // Send request as newline-delimited JSON
    let req_json = serde_json::to_string(req).ok()?;
    if writer.write_all(req_json.as_bytes()).is_err() || writer.write_all(b"\n").is_err() {
        touring_hooks::circuit_breaker::record_failure(
            &req.hook,
            Some(&req.project_root),
            req.session_id.as_deref(),
            false,
        );
        return None;
    }

    // Read response line
    let mut reader = BufReader::new(reader_stream);
    let mut line = String::new();
    if reader.read_line(&mut line).is_err() {
        touring_hooks::circuit_breaker::record_failure(
            &req.hook,
            Some(&req.project_root),
            req.session_id.as_deref(),
            false,
        );
        return None;
    }

    match serde_json::from_str::<DaemonResponse>(line.trim()) {
        Ok(resp) => {
            touring_hooks::circuit_breaker::record_success(
                &req.hook,
                Some(&req.project_root),
                req.session_id.as_deref(),
            );
            Some(resp)
        }
        Err(_) => {
            touring_hooks::circuit_breaker::record_failure(
                &req.hook,
                Some(&req.project_root),
                req.session_id.as_deref(),
                false,
            );
            None
        }
    }
}

/// Check if the daemon socket is responsive (connect succeeds).
/// Returns false if socket exists but daemon is dead (orphan socket).
fn socket_is_responsive(socket_path: &std::path::Path) -> bool {
    use std::os::unix::net::UnixStream;
    // Quick connect-only probe — 50ms timeout is enough to detect a dead socket.
    match UnixStream::connect(socket_path) {
        Ok(stream) => {
            let _ = stream.set_read_timeout(Some(std::time::Duration::from_millis(50)));
            true
        }
        Err(_) => false,
    }
}

/// Ensure the daemon socket is ready to accept connections.
///
/// Two cases:
/// - Socket absent: spawns daemon and waits up to `initial_wait_ms` for it to bind.
/// - Socket present but unresponsive (orphan crash): cleans up stale state,
///   respawns, and waits up to 300 ms (the standard restart budget).
///
/// Returns `true` if the socket exists and is connectable after the above
/// remediation, `false` if the daemon failed to start in time (caller should
/// fall back to standalone execution — "hooks never block CC" invariant).
fn ensure_daemon_socket(socket_path: &std::path::Path, initial_wait_ms: u64) -> bool {
    if !socket_path.exists() {
        try_autostart_daemon();
        let deadline =
            std::time::Instant::now() + std::time::Duration::from_millis(initial_wait_ms);
        while !socket_path.exists() && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        if !socket_path.exists() {
            return false;
        }
    } else if !socket_is_responsive(socket_path) {
        // Socket exists but daemon is dead — orphan socket from a crash.
        // Clean up stale state and try to restart the daemon.
        cleanup_orphan_daemon_state();
        try_autostart_daemon();
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(300);
        while !socket_path.exists() && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        if !socket_path.exists() {
            return false;
        }
    }
    true
}

/// Clean up orphan socket and stale lock/circuit state left by a crashed daemon.
/// Called when the socket exists but the daemon process is dead.
fn cleanup_orphan_daemon_state() {
    let socket_path = daemon_socket_path();
    // W12.5 (cross-audit fix 2026-07-24): the lock must be the one DERIVED
    // from the socket being cleaned — the old uid-global name mismatched a
    // per-project socket, so this cleanup could touch the GLOBAL daemon's
    // lock while leaving the crashed project daemon's lock behind.
    let lock_path = touring_hooks::ipc::daemon_lock_path_for(&socket_path);

    tracing::debug!("cleaning up orphan daemon state");

    // Remove the orphan socket so a new daemon can bind
    let _ = std::fs::remove_file(&socket_path);
    // Remove stale lock file (if the PID inside is dead)
    if let Ok(contents) = std::fs::read_to_string(&lock_path) {
        if let Ok(pid) = contents.trim().parse::<u32>() {
            // kill(pid, 0) checks if process exists without sending a signal
            let alive = unsafe { libc_kill(pid as i32, 0) } == 0;
            if !alive {
                let _ = std::fs::remove_file(&lock_path);
            }
        }
    }
    // Reset circuit breaker — failures from the dead daemon are stale
    touring_hooks::circuit_breaker::reset();
}

unsafe extern "C" {
    fn kill(pid: i32, sig: i32) -> i32;
    fn setsid() -> i32;
}

#[inline]
unsafe fn libc_kill(pid: i32, sig: i32) -> i32 {
    // SAFETY: caller guarantees `pid`/`sig` are a valid PID + signal number; `kill(2)`
    // has no memory-safety preconditions beyond that (it merely returns -1/EINVAL on
    // a bad signal). Explicit block per `unsafe_op_in_unsafe_fn`.
    unsafe { kill(pid, sig) }
}

/// Spawn the daemon in the background if it isn't running.
///
/// # W12.5 hardening (2026-05-24) — per-project deployment + Sprint 4 PD-3
///
/// Two issues had to be solved at once:
///
/// 1. **`--start-daemon` mode deprecated**: Sprint 4 PD-3 (2026-05-23) deprecated
///    `touring-hook --start-daemon` — invoking it now only prints a deprecation
///    notice and exits without starting any daemon. The previous implementation
///    here silently failed for every autostart attempt. We now resolve the
///    sibling `touring-daemon` binary in the same `target/release/` (or
///    `~/.local/bin/`) directory as the current executable and spawn it
///    directly.
///
/// 2. **Per-project socket path consistency**: `ipc::daemon_socket_path()`
///    resolves the socket via a 4-layer chain (env → env-legacy → per-project
///    walk-up → global fallback). The client computes this path BEFORE spawn.
///    Without explicitly passing it to the daemon via `TOURING_DAEMON_SOCKET`,
///    a daemon spawned with a different inherited cwd/env could bind a
///    different path — the client would then poll forever on the wrong socket.
///    We resolve the path here and pass it via env so client and daemon bind
///    the same Unix socket regardless of inherited context.
///
/// Fire-and-forget: we don't wait for it — `try_daemon_request` polls the
/// socket until it accepts connections.
fn try_autostart_daemon() {
    let exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(_) => return,
    };

    // Sprint 4 PD: resolve the dedicated `touring-daemon` binary in the
    // sibling directory of the current executable. Falls back to the PATH
    // name `"touring-daemon"` if the resolved path doesn't exist (e.g. when
    // current_exe is a wrapper script or installed elsewhere).
    let daemon_bin = exe
        .parent()
        .map(|d| d.join("touring-daemon"))
        .filter(|p| p.exists())
        .unwrap_or_else(|| std::path::PathBuf::from("touring-daemon"));

    // W12.5: resolve socket path on the client side and pass it explicitly,
    // so the daemon binds the same path the client is about to poll.
    let socket_path = daemon_socket_path();

    // Daemon lifetime fix (2026-07-01) — mirrors touring-server
    // `cli/daemon_ctl.rs::spawn_daemon`; keep both call-sites in sync (C08).
    // The old comment here PROMISED detachment but no code implemented it:
    // the daemon inherited the spawner's process group/session and could die
    // with the invoking Claude Code session's cleanup (observed 2026-07-01 —
    // alive at session end, Connection refused when the next session began).
    // setsid(2) gives it a fresh session; stdout/stderr append to
    // ~/.claude/touring/daemon.stderr.log so daemon tracing + exit markers
    // stop being discarded.
    let (stdout_io, stderr_io) = daemon_spawn_log_stdio();
    let mut cmd = std::process::Command::new(daemon_bin);
    cmd.env("TOURING_DAEMON_SOCKET", &socket_path)
        .stdin(std::process::Stdio::null())
        .stdout(stdout_io)
        .stderr(stderr_io);
    // SAFETY: pre_exec runs in the forked child before exec; setsid(2) is
    // async-signal-safe and cannot fail with EPERM there — the freshly forked
    // child is never a process-group leader.
    unsafe {
        use std::os::unix::process::CommandExt;
        cmd.pre_exec(|| {
            // Direct syscall, no memory access; covered by the SAFETY note above.
            if setsid() == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let _ = cmd.spawn();
}

/// Append-mode logfile stdio for the spawned daemon, prefixed with a
/// spawn-epoch header identifying this spawner. Mirrors touring-server
/// `cli/daemon_ctl.rs::daemon_log_stdio`; keep both in sync (C08).
/// Fail-open: degrades to `Stdio::null()` — logging must never block the
/// spawn itself (REGRA #19).
fn daemon_spawn_log_stdio() -> (std::process::Stdio, std::process::Stdio) {
    use std::io::Write;
    use std::process::Stdio;
    let open = || -> Option<std::fs::File> {
        let home = std::env::var("HOME").ok()?;
        let dir = std::path::PathBuf::from(home)
            .join(".claude")
            .join("touring");
        std::fs::create_dir_all(&dir).ok()?;
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.join("daemon.stderr.log"))
            .ok()
    };
    let Some(mut f) = open() else {
        return (Stdio::null(), Stdio::null());
    };
    let epoch = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let _ = writeln!(f, "=== touring-hook autostart spawn epoch={epoch} ===");
    match f.try_clone() {
        Ok(f2) => (Stdio::from(f2), Stdio::from(f)),
        Err(_) => (Stdio::null(), Stdio::from(f)),
    }
}

fn print_help() {
    println!(
        "\
touring-hook — Native Rust binary for Claude Code hooks (v11.0 — full 24-event coverage)

USAGE:
    touring-hook <SUBCOMMAND>

SUBCOMMANDS:
    Tool hooks (stdin: Claude Code hook JSON):
        pre-read              Inject context before file read
        post-read             Capture knowledge after file read
        pre-bash              Recall command outcomes before bash
        post-bash             Learn command outcomes after bash
        pre-edit              Inject context before file edit
        post-edit             Track edit outcomes after edit
        post-tool-rl          RL reward processing after tool use
        pre-edit-prevention   Complexity gating before edit
        pre-grep              Symbol enrichment before Grep (D43)
        pre-glob              Symbol enrichment before Glob (D43)
        prompt-enhance        Classify and enhance user prompts

    Session lifecycle:
        session-start         Initialize session (quality tracking, knowledge)
        session-stop          Finalize session (report, persist)
        session-end           Final persistence checkpoint
        setup                 Repo setup initialization

    Agent lifecycle:
        subagent-start        Subagent context injection
        subagent-stop         Subagent outcome recording
        teammate-idle         Teammate quality gate
        task-completed        Task outcome tracking
        task-created          Task creation recording

    Context management:
        pre-compact           Pre-compaction context crystal
        post-compact          Post-compaction recovery injection
        config-change         Config file change detection
        instructions-loaded   Instructions enrichment

    File & directory:
        worktree-create       Worktree setup
        worktree-remove       Worktree cleanup
        cwd-changed           Working directory change
        file-changed          Watched file change

    Error handling:
        post-tool-failure     Tool failure RL signal
        stop-failure          API/system error recording
        notification          Notification recording

    MCP interaction:
        elicitation           MCP elicitation recording
        elicitation-result    MCP elicitation result recording
        permission-request    Permission request audit

    Utilities (stdin: JSON with relevant fields):
        classify              CILA intent classification
        pii-scan              PII detection and redaction
        qa-syntax             Python syntax validation
        metrics               Export consolidated runtime metrics
        session-insights      Session trend analysis
        session-audit         CLAUDE.md hash drift detection

    Other:
        help                  Show this help message

ENVIRONMENT:
    CLAUDE_PROJECT_DIR    Project root (default: cwd)
    TOURING_LOG           Log level (default: off)

PERFORMANCE:
    Startup: <1ms | Runtime init: <10ms | Hook execution: <5ms
    Compare: Python hooks ~40ms startup, Node.js hooks ~300ms"
    );
}
