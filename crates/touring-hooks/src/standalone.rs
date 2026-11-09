//! The standalone route of `touring-hook`: what answers a subcommand when the daemon does not
//! (it is down, it timed out, or `TOURING_NO_DAEMON` is set).
//!
//! Until 02/11/2026 this was a hand-written `match` of ~47 arms in `main.rs`, beside the
//! daemon's dispatch table (`touring_hooks::hook_registry`). Two lists of "which hooks exist"
//! drifted: 29 names were in both, 18 hook events the daemon served (`pre-tool-use`,
//! `post-tool-use`, `stop`, `pre-compact`, `user_prompt_submit`, …) had no arm and answered
//! `unknown subcommand` with the daemon down, and nothing could say whether an arm and its
//! table handler still did the same thing.
//!
//! [`route`] now decides, in this order, and the table is the default:
//!
//! 1. [`STATELESS`] — answered before any runtime exists; not daemon hooks.
//! 2. [`OVERRIDES`] — table keys whose standalone behavior is deliberately NOT the table
//!    handler's. Each entry says why, in one line, and a test pins every key to the table.
//! 3. [`STANDALONE_ONLY`] — names the table does not hold: utilities, and hook events the
//!    daemon never serves. A test pins them OUT of the table, so promoting one is a visible edit.
//! 4. The table — every other key runs the daemon's own handler in this process. Adding a hook
//!    to `dispatch_table()` therefore makes it reachable in both modes with no edit here.
//!    The exception is the table's CLI query handlers (`is_rpc_endpoint`): their caller is the
//!    daemon socket, so they answer an explicit error naming that reason.
//! 5. Anything else is unknown (fail open, as always).

use serde_json::Value;
use std::process;
use touring_foundation::truncate_str;
use touring_hook_runtime::hook_response::{self, HookResponse};
use touring_hook_runtime::hook_runtime::HookDispatchError;
use touring_hooks::hook_registry::{dispatch_table, is_daemon_hook, is_rpc_endpoint};
use touring_hooks::runtime::HookRuntime;

/// What a standalone body returns; `main` turns it into the exit reason.
pub(crate) type Standalone = Result<(), HookDispatchError>;

/// How an [`Entry`] runs.
pub(crate) enum Runner {
    /// A function of the runtime and the payload.
    Fn(fn(&mut HookRuntime, &Value) -> Standalone),
    /// The generic lifecycle recorder ([`run_lifecycle_event`]); the event it records is the
    /// entry's name with `-` turned into `_`.
    Lifecycle,
}

/// A subcommand with a body of its own in the standalone route.
pub(crate) struct Entry {
    pub(crate) name: &'static str,
    /// One line: for an override, how it differs from the table handler; for a
    /// standalone-only name, why the table does not hold it.
    pub(crate) why: &'static str,
    pub(crate) run: Runner,
}

/// A standalone-only subcommand answered before any `HookRuntime` is built.
pub(crate) struct Stateless {
    pub(crate) name: &'static str,
    pub(crate) why: &'static str,
    pub(crate) run: fn(&Value) -> !,
}

const UTILITY: &str = "CLI-ish utility: not a hook event, never in the daemon table";
const EVENT_NOT_IN_TABLE: &str =
    "hook event the daemon table does not hold, so the daemon never answers it";

/// Table keys whose standalone behavior is deliberately not the table handler's.
///
/// EMPTY since 04/11/2026 (aprofundamento #4, approved by Gabriel): the 13 entries measured on
/// 02/11 were divergences, not designs — `cli-suggest` threw the suggestion away (`let _ =`),
/// `post-tool-rl`, `task-created`, `task-validation` and seven lifecycle recorders did less
/// than the daemon's handler, `session-stop` and `subagent-stop` spoke protocols the table now
/// speaks itself (the table names session-stop's error on stderr; subagent-stop answers the
/// contract's `decision: "block"`). A key here must say, in its `why`, what the table cannot do
/// standalone; `tests/standalone_table_parity.rs` shows the output delta of every entry.
pub(crate) static OVERRIDES: &[Entry] = &[];

/// Names the daemon table does not hold: utilities and hook events the daemon never serves.
/// A test pins every one OUT of the table — a name promoted into it must leave this list.
pub(crate) static STANDALONE_ONLY: &[Entry] = &[
    Entry {
        name: "classify",
        why: UTILITY,
        run: Runner::Fn(|rt, input| run_classify(rt, input)),
    },
    Entry {
        name: "pii-scan",
        why: UTILITY,
        run: Runner::Fn(|rt, input| run_pii_scan(rt, input)),
    },
    Entry {
        name: "metrics",
        why: UTILITY,
        run: Runner::Fn(|rt, _| run_metrics(rt)),
    },
    Entry {
        name: "decompose-event",
        why: UTILITY,
        run: Runner::Fn(|rt, input| run_decompose_event(rt, input)),
    },
    #[cfg(feature = "utilities")]
    Entry {
        name: "session-audit",
        why: UTILITY,
        run: Runner::Fn(|_, input| run_session_audit(input)),
    },
    #[cfg(feature = "session-hooks")]
    Entry {
        name: "session-insights",
        why: UTILITY,
        run: Runner::Fn(|rt, input| run_session_insights(rt, input)),
    },
    Entry {
        name: "permission-request",
        why: EVENT_NOT_IN_TABLE,
        run: Runner::Fn(|rt, input| touring_hooks::permission_request::run(rt, input)),
    },
    // CEG Pln2 P3.7 — the Code Execution Gateway pre-exec hook. Advisory by default (observe
    // mode); `CEG_ENFORCE=1` opts into hard deny.
    #[cfg(feature = "pre-hooks")]
    Entry {
        name: "pre-exec",
        why: EVENT_NOT_IN_TABLE,
        run: Runner::Fn(|rt, input| touring_hooks::ceg_adapter::run(rt, input)),
    },
    Entry {
        name: "session-end",
        why: EVENT_NOT_IN_TABLE,
        run: Runner::Lifecycle,
    },
    Entry {
        name: "config-change",
        why: EVENT_NOT_IN_TABLE,
        run: Runner::Lifecycle,
    },
    Entry {
        name: "worktree-remove",
        why: EVENT_NOT_IN_TABLE,
        run: Runner::Lifecycle,
    },
    Entry {
        name: "notification",
        why: EVENT_NOT_IN_TABLE,
        run: Runner::Lifecycle,
    },
    Entry {
        name: "setup",
        why: EVENT_NOT_IN_TABLE,
        run: Runner::Lifecycle,
    },
    Entry {
        name: "elicitation",
        why: EVENT_NOT_IN_TABLE,
        run: Runner::Lifecycle,
    },
    Entry {
        name: "elicitation-result",
        why: EVENT_NOT_IN_TABLE,
        run: Runner::Lifecycle,
    },
    Entry {
        name: "stop-failure",
        why: EVENT_NOT_IN_TABLE,
        run: Runner::Lifecycle,
    },
];

/// Standalone-only subcommands that need no runtime at all.
pub(crate) static STATELESS: &[Stateless] = &[
    Stateless {
        name: "prompt-enhance",
        why: "stateless prompt classifier: no runtime, never in the daemon table (the daemon's UserPromptSubmit is `user_prompt_submit`)",
        run: run_prompt_enhance,
    },
    #[cfg(feature = "utilities")]
    Stateless {
        name: "qa-syntax",
        why: "stateless Python syntax check: no runtime, never in the daemon table",
        run: run_qa_syntax,
    },
];

/// Where a subcommand is answered.
pub(crate) enum Route {
    /// Before the runtime exists.
    Stateless(&'static Stateless),
    /// A table key with a deliberately different standalone body.
    Override(&'static Entry),
    /// A name outside the table, with a body of its own.
    StandaloneOnly(&'static Entry),
    /// The daemon's own handler, run in this process.
    Table,
    /// A table key that answers the `touring` CLI over the socket, not a hook event.
    RpcEndpoint,
    /// Nobody's.
    Unknown,
}

/// The route of `name`, given whether the dispatch table holds it. Split from [`route`] so the
/// rule is testable for a name the real table does not have.
pub(crate) fn route_with(name: &str, in_table: bool) -> Route {
    if let Some(s) = STATELESS.iter().find(|s| s.name == name) {
        return Route::Stateless(s);
    }
    if let Some(e) = OVERRIDES.iter().find(|e| e.name == name) {
        return Route::Override(e);
    }
    if let Some(e) = STANDALONE_ONLY.iter().find(|e| e.name == name) {
        return Route::StandaloneOnly(e);
    }
    match (in_table, is_rpc_endpoint(name)) {
        (false, _) => Route::Unknown,
        (true, true) => Route::RpcEndpoint,
        (true, false) => Route::Table,
    }
}

/// The route of `name` against the real dispatch table.
pub(crate) fn route(name: &str) -> Route {
    route_with(name, is_daemon_hook(name))
}

/// Answer `subcommand` with a `HookRuntime` built. Never returns normally for a refusal or an
/// unknown name (both exit 0, fail open).
pub(crate) fn dispatch(subcommand: &str, runtime: &mut HookRuntime, input: &Value) -> Standalone {
    fail_open_on_panic();
    match route(subcommand) {
        Route::Stateless(s) => run_stateless(s, input),
        Route::Override(e) | Route::StandaloneOnly(e) => {
            // `TOURING_LOG=debug` says why this name does not run the table handler.
            tracing::debug!(hook = subcommand, why = e.why, "standalone body of its own");
            match e.run {
                Runner::Fn(run) => run(runtime, input),
                Runner::Lifecycle => {
                    run_lifecycle_event(runtime, input, &subcommand.replace('-', "_"))
                }
            }
        }
        Route::Table => serve_from_table(subcommand, runtime, input),
        Route::RpcEndpoint => refuse_rpc_endpoint(subcommand),
        Route::Unknown => refuse_unknown(subcommand),
    }
}

/// Run a stateless body: no runtime, it answers and exits.
pub(crate) fn run_stateless(stateless: &Stateless, input: &Value) -> ! {
    tracing::debug!(
        hook = stateless.name,
        why = stateless.why,
        "stateless standalone body"
    );
    (stateless.run)(input)
}

/// A panicking handler must not turn a hook into a failure Claude Code shows: answer nothing and
/// exit 0, in a debug build (unwinding) and in the release one (`panic = "abort"`) alike — the
/// hook runs before either, so this is the one place both are covered.
fn fail_open_on_panic() {
    std::panic::set_hook(Box::new(|info| {
        touring_hook_runtime::hook_trace::set_reason("standalone-panic");
        eprintln!(
            "[touring-hook] a standalone handler panicked ({info}); answering nothing — \
             hooks never block Claude Code"
        );
        process::exit(0);
    }));
}

fn refuse_unknown(subcommand: &str) -> ! {
    touring_hook_runtime::hook_trace::set_reason("standalone-unknown");
    eprintln!("[touring-hook] unknown subcommand: {subcommand}");
    eprintln!("Run `touring-hook help` for usage.");
    // Fail open — never block Claude Code, even for unknown hooks.
    // Claude Code may register new hook events before we add handlers.
    process::exit(0);
}

fn refuse_rpc_endpoint(subcommand: &str) -> ! {
    touring_hook_runtime::hook_trace::set_reason("standalone-daemon-only");
    eprintln!(
        "[touring-hook] {subcommand} is a daemon query endpoint (the `touring` CLI's RPC handler), \
         not a hook event: its caller is the daemon socket, so it has no standalone mode. \
         Run it through `touring`, with the daemon up."
    );
    process::exit(0);
}

/// Run the daemon's own handler for `subcommand` in this process and answer with what it returns.
fn serve_from_table(subcommand: &str, runtime: &mut HookRuntime, input: &Value) -> Standalone {
    match dispatch_table().get(subcommand) {
        Some(handler) => answer_table_output(&handler(runtime, input)),
        None => refuse_unknown(subcommand),
    }
}

/// How the string a table handler returns leaves this process.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum TableAnswer {
    /// `""` or `{}`: no answer, which is what `HookResponse::Allow.emit()` always printed.
    Silent,
    /// A JSON object that already conforms to Claude Code's output contract for the invoking
    /// event: it leaves through the same funnel as `HookResponse::emit`.
    Conforming,
    /// Not a JSON object, or JSON the contract does not admit (the `task-completed` receipt's 14
    /// keys): it leaves as the daemon-served answer does — prose becomes the canonical `{}`, the
    /// rest is deduped and conformed by `forward_daemon_output` (stray keys go to stderr).
    NeedsContractFunnel,
}

/// Classify a table handler's answer. `conform_daemon_output` at level `Full` is the oracle: it
/// changes nothing but what the contract (or the invoking event's stamp) requires, so an answer it
/// leaves byte-identical conforms already.
pub(crate) fn classify_table_answer(out: &str) -> TableAnswer {
    let text = out.trim();
    if text.is_empty() || text == "{}" {
        return TableAnswer::Silent;
    }
    // `conform_daemon_output` answers `Some` only for valid JSON, so a leading `{` is an object.
    match crate::conform_daemon_output(
        text,
        touring_foundation::injection_policy::InjectionLevel::Full,
        None,
    ) {
        Some((conformed, None)) if conformed == text && text.starts_with('{') => {
            TableAnswer::Conforming
        }
        _ => TableAnswer::NeedsContractFunnel,
    }
}

/// Print a table handler's answer the way the daemon-served route would have delivered it, and
/// the way the arms this replaced did: an empty answer prints nothing, a conforming one leaves
/// through `emit`'s funnel (policy, then epoch dedupe — byte-identical to what `run_returning`
/// followed by `emit` printed), the rest through the daemon-forward funnel (the standalone
/// `task-completed` of S219).
fn answer_table_output(out: &str) -> Standalone {
    match classify_table_answer(out) {
        TableAnswer::Silent => HookResponse::allow().emit(),
        TableAnswer::Conforming => {
            crate::exit_if_exit_code_block(out.trim());
            hook_response::emit_json(out.trim())
        }
        TableAnswer::NeedsContractFunnel => {
            if serde_json::from_str::<Value>(out.trim()).is_err() {
                // Prose, not JSON: Claude Code's strict hook-output validator would reject it.
                // The daemon route answers the canonical `{}` and says so; so does this one —
                // naming the handler, not a daemon that is not in this picture.
                eprintln!(
                    "[touring-hook] the handler answered {} bytes of text that is not JSON; \
                     emitting canonical {{}} Allow instead",
                    out.len()
                );
                println!("{{}}");
                return Ok(());
            }
            let deduped = match touring_hook_runtime::hook_runtime::dedup_scope() {
                Some((root, session)) => {
                    touring_hooks_shared::injection_dedup::dedup_hook_output(root, session, out)
                }
                None => out.to_string(),
            };
            crate::forward_daemon_output(&deduped, true);
            Ok(())
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

    // P3/S-3.3 (2026-09-04) — the turn boundary. This is the ONLY event that
    // means "the human spoke again", so this is where the turn's injection
    // budget restarts. It works across processes because the
    // state lives on disk (see `turn_budget::turn_file`): this binary is
    // ephemeral and the suggester lives in the daemon.
    if let Some(session) = input.get("session_id").and_then(|v| v.as_str()) {
        let root = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
        touring_hook_runtime::turn_budget::reset_turn(&root, session);
    }

    let output = touring_hooks::prompt_enhance::compose_json(prompt);
    crate::print_standalone(&serde_json::to_string(&output).unwrap_or_default());
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
fn run_classify(runtime: &HookRuntime, input: &Value) -> Standalone {
    // One source for the field name (`prompt_from_input`) — this entry point used to
    // accept only "text"/"input" while every other consumer sent "userMessage"/"prompt",
    // so it answered L0 with an empty pattern for every realistic payload.
    let text = touring_hooks::classifier::prompt_from_input(input);

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
fn run_pii_scan(runtime: &HookRuntime, input: &Value) -> Standalone {
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

/// Session audit — hash CLAUDE.md and .claude/rules/ and report drift. Gated
/// like `touring_hooks_core::audit`, the module it drives.
#[cfg(feature = "utilities")]
fn run_session_audit(input: &Value) -> Standalone {
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
fn run_decompose_event(runtime: &mut HookRuntime, input: &Value) -> Standalone {
    let output = touring_hooks::cli_handlers::cli_decompose_event(runtime, input);
    println!("{}", output);
    Ok(())
}

/// R19: Export consolidated runtime metrics as JSON.
fn run_metrics(runtime: &HookRuntime) -> Standalone {
    let metrics = runtime.export_metrics(None);
    println!(
        "{}",
        serde_json::to_string_pretty(&metrics).unwrap_or_default()
    );
    Ok(())
}

/// Session insights utility.
#[cfg(feature = "session-hooks")]
fn run_session_insights(runtime: &HookRuntime, input: &Value) -> Standalone {
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
fn run_lifecycle_event(runtime: &HookRuntime, input: &Value, event_name: &str) -> Standalone {
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
                command: format!("{}:{}", event_name, truncate_str(summary, 200)),
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

#[cfg(test)]
#[path = "standalone_tests.rs"]
mod tests;
