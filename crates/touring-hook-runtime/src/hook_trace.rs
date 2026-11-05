//! F0.3a (2026-09-01) — one JSON line per `touring-hook` invocation.
//!
//! The thin client is an ephemeral process: its `tracing` output dies with it
//! and Claude Code discards hook stderr on exit 0. When live PostToolUse(Bash)
//! events stopped reaching the signal mirror (42/47 lost in one session, no
//! daemon-side evidence either way), nothing on disk could say WHICH ROUTE the
//! live process took — daemon fast-path, standalone fallback, or a stdin that
//! degraded to `{}` in silence. This module is that instrument.
//!
//! Contract:
//! - **Opt-in**: nothing happens unless `TOURING_HOOK_TRACE_FILE` is set.
//! - **One line per process**, written from an `atexit` handler so every
//!   `process::exit` path (there are a dozen in `main.rs`, several inside
//!   handlers) is covered without threading a value through each of them.
//! - **Fail-open**: an unwritable path never affects the hook.
//! - **No payload**: only route/state/size/timing — never the command text
//!   (the trace file must not become an exfiltration surface).
//!
//! `route` ∈ {`daemon`, `standalone`, `stateless`, `unknown`};
//! `stdin_state` comes from [`crate::hook_runtime_ext::last_stdin_read`].

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

/// Environment variable naming the append-only JSONL sink.
const ENV_TRACE_FILE: &str = "TOURING_HOOK_TRACE_FILE";

/// The single line a `touring-hook` process leaves behind.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct HookTraceLine {
    /// Unix epoch milliseconds at exit.
    pub ts_ms: u64,
    /// Pid of the hook process.
    pub pid: u32,
    /// Parent pid — `claude` for a live hook, a shell for a manual probe.
    pub ppid: u32,
    /// Subcommand (`post-bash`, `cli-suggest`, `help`, …).
    pub hook: String,
    /// `daemon` | `standalone` | `stateless` | `unknown`.
    pub route: String,
    /// Fine-grained outcome (`daemon-json`, `daemon-empty`, `standalone-ok`,
    /// `standalone-unknown`, `runtime-init-err`, `handler-diverged`, …).
    pub exit_reason: String,
    /// `ok` | `empty` | `timeout` | `terminal` | `read-error` | `parse-error` | `unread` |
    /// `ok-recovered` | `too-large` (C4). Every value but `ok` is a degraded read; the
    /// unreadable ones (`timeout`, `read-error`, `parse-error`, `too-large`) deny a guard
    /// (`exit_reason = stdin-unreadable-deny`) and leave an advisory running on `{}`.
    pub stdin_state: String,
    /// Bytes read from stdin (0 when unread/empty/timeout).
    pub stdin_bytes: u64,
    /// Wall time from `install` to exit.
    pub elapsed_ms: u64,
    /// Claude Code session id from the payload, when present (attribution only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    /// The tool the event is about (`Bash`, `Edit`, …), when the payload names one (#6).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
    /// For Bash, the program's bare name (`cargo`, `grep`) — never an argument or a path (#6).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub class: Option<String>,
    /// What the answer decided: `deny` | `allow` | `ask` | `defer` | `block` | `halt` |
    /// `rewrite` | `context` | `silent` (#6).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verdict: Option<String>,
    /// The gate tag the answer led with (`G7 re-inspeção`, `CODE MODE · rajada`), never a path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gate: Option<String>,
}

struct TraceState {
    started: Instant,
    hook: String,
    path: PathBuf,
    route: Mutex<&'static str>,
    reason: Mutex<&'static str>,
    session_id: Mutex<Option<String>>,
    /// `(tool, class)` from the payload (#6).
    event: Mutex<(Option<String>, Option<String>)>,
    /// `(verdict, gate)` from the answer printed (#6).
    answer: Mutex<(Option<&'static str>, Option<String>)>,
}

/// The program a Bash command runs, as a bare name: leading `VAR=value` assignments are
/// skipped and a path keeps only its last component. `None` when nothing clean is left — the
/// trace never carries an argument, a path or anything outside `[A-Za-z0-9._+-]` (#6).
pub fn command_class(command: &str) -> Option<String> {
    let program = command
        .split_whitespace()
        .find(|tok| !(tok.contains('=') && !tok.starts_with('=')))?;
    let base = program.rsplit('/').next().unwrap_or(program);
    let clean = !base.is_empty()
        && base.len() <= 32
        && base
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '+' | '-'));
    clean.then(|| base.to_owned())
}

/// The first `[tag]` a reason or context leads with, when it is short and names no path. A tag
/// that opens with a canonical gate id (`G7 re-inspeção`, `E3 …`, `M3 …`) is reduced to the id,
/// so replays group by gate, not by prose. Broader than `tool_trio::deny_gate_of` on purpose:
/// that one counts only the `G<n>` gates of a denial and folds every other family into
/// `other`; the trace keeps each family (`E3`, `S3`, `M3`) because a replay asks which gate
/// spoke, whatever its family or verdict.
fn gate_tag(text: &str) -> Option<String> {
    let start = text.find('[')?;
    let rest = &text[start + 1..];
    let end = rest.find(']')?;
    let tag = rest[..end].trim();
    let clean =
        !tag.is_empty() && tag.chars().count() <= 48 && !tag.contains(['/', '\\', '~', '\n']);
    if !clean {
        return None;
    }
    let head = tag.split_whitespace().next().unwrap_or(tag);
    let letters = head.chars().take_while(char::is_ascii_uppercase).count();
    let digits = head[letters..]
        .chars()
        .take_while(char::is_ascii_digit)
        .count();
    let canonical = (1..=3).contains(&letters) && digits > 0 && letters + digits == head.len();
    Some(if canonical { head } else { tag }.to_owned())
}

/// What an answer on stdout decided, and the gate tag it led with (#6). Pure over the text.
pub fn classify_answer(stdout: &str) -> (&'static str, Option<String>) {
    let text = stdout.trim();
    if text.is_empty() || text == "{}" {
        return ("silent", None);
    }
    let Ok(v) = serde_json::from_str::<serde_json::Value>(text) else {
        return ("silent", None);
    };
    let hso = v.get("hookSpecificOutput");
    let said = str_at(hso, "permissionDecisionReason")
        .or_else(|| str_at(Some(&v), "reason"))
        .or_else(|| str_at(hso, "additionalContext"))
        .unwrap_or_default();
    (verdict_of(&v), gate_tag(said))
}

/// `obj[key]` as a string, when present.
fn str_at<'a>(obj: Option<&'a serde_json::Value>, key: &str) -> Option<&'a str> {
    obj.and_then(|o| o.get(key)).and_then(|s| s.as_str())
}

/// The decision an answer carries, in Claude Code's precedence: a permission decision, then a
/// top-level block, a halt, an input rewrite, plain context — else nothing was decided.
fn verdict_of(v: &serde_json::Value) -> &'static str {
    let hso = v.get("hookSpecificOutput");
    match str_at(hso, "permissionDecision") {
        Some("deny") => return "deny",
        Some("allow") => return "allow",
        Some("ask") => return "ask",
        Some("defer") => return "defer",
        _ => {}
    }
    if str_at(Some(v), "decision") == Some("block") {
        return "block";
    }
    if v.get("continue").and_then(|c| c.as_bool()) == Some(false) {
        return "halt";
    }
    if hso.and_then(|h| h.get("updatedInput")).is_some() {
        return "rewrite";
    }
    match str_at(hso, "additionalContext") {
        Some(c) if !c.trim().is_empty() => "context",
        _ => "silent",
    }
}

/// Record the tool and, for Bash, the program class of the invoking payload (#6).
pub fn set_event(tool: Option<&str>, command: Option<&str>) {
    if let Some(Some(st)) = STATE.get()
        && let Ok(mut e) = st.event.lock()
    {
        *e = (tool.map(str::to_owned), command.and_then(command_class));
    }
}

/// Record what the answer about to be printed decides (last writer wins) (#6).
pub fn note_answer(stdout: &str) {
    if let Some(Some(st)) = STATE.get()
        && let Ok(mut a) = st.answer.lock()
    {
        let (verdict, gate) = classify_answer(stdout);
        *a = (Some(verdict), gate);
    }
}

static STATE: OnceLock<Option<TraceState>> = OnceLock::new();

/// Arm the trace for this process. Idempotent; a no-op when the env var is
/// unset or empty. Registers the `atexit` flush on first arming.
pub fn install(hook: &str) {
    // I-3: `HookResponse::emit` lives in `touring-hooks-core`, a layer below this one,
    // and records its exit reason through this sink. `set_reason` is a no-op while the
    // trace is unarmed, so registering it here (the only way to arm) loses nothing.
    touring_hooks_core::hook_response::set_emit_reason_sink(set_reason);
    // #6: the same route for the text `emit_json` prints — the verdict and gate come from it.
    touring_hooks_core::hook_response::set_emit_answer_sink(note_answer);
    let hook = hook.to_string();
    let _ = STATE.get_or_init(|| {
        let path = std::env::var_os(ENV_TRACE_FILE)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)?;
        // SAFETY: `flush_at_exit` is an `extern "C" fn()` with no arguments, the
        // exact signature `atexit` expects; it only touches process-global state.
        let registered = unsafe { libc::atexit(flush_at_exit) } == 0;
        if !registered {
            return None;
        }
        Some(TraceState {
            started: Instant::now(),
            hook,
            path,
            route: Mutex::new("unknown"),
            reason: Mutex::new("handler-diverged"),
            session_id: Mutex::new(None),
            event: Mutex::new((None, None)),
            answer: Mutex::new((None, None)),
        })
    });
}

/// Record which route the process took (`daemon` | `standalone` | `stateless`).
pub fn set_route(route: &'static str) {
    if let Some(Some(st)) = STATE.get()
        && let Ok(mut r) = st.route.lock()
    {
        *r = route;
    }
}

/// Record the fine-grained exit reason (last writer wins — call it at each exit).
pub fn set_reason(reason: &'static str) {
    if let Some(Some(st)) = STATE.get()
        && let Ok(mut r) = st.reason.lock()
    {
        *r = reason;
    }
}

/// Record the Claude Code session id carried by the payload (attribution only).
pub fn set_session_id(session_id: Option<&str>) {
    if let Some(Some(st)) = STATE.get()
        && let Ok(mut s) = st.session_id.lock()
    {
        *s = session_id.map(str::to_string);
    }
}

/// Build the line from the current process state — pure over its inputs so
/// the shape is testable without spawning a process.
#[allow(clippy::too_many_arguments)]
pub fn build_line(
    hook: &str,
    route: &str,
    exit_reason: &str,
    stdin_state: &str,
    stdin_bytes: u64,
    elapsed_ms: u64,
    session_id: Option<&str>,
    pid: u32,
    ppid: u32,
) -> HookTraceLine {
    let ts_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    HookTraceLine {
        ts_ms,
        pid,
        ppid,
        hook: hook.to_string(),
        route: route.to_string(),
        exit_reason: exit_reason.to_string(),
        stdin_state: stdin_state.to_string(),
        stdin_bytes,
        elapsed_ms,
        session_id: session_id.map(str::to_string),
        tool: None,
        class: None,
        verdict: None,
        gate: None,
    }
}

/// Append one line to `path` (creating parents and the file as needed).
pub fn append_line(path: &Path, line: &HookTraceLine) -> std::io::Result<()> {
    let value = serde_json::to_value(line).map_err(std::io::Error::other)?;
    append_json_line(path, &value)
}

/// Append one arbitrary JSON value as one line (creating parents and the file).
///
/// The WHOLE line goes out in a single `write_all` over O_APPEND: POSIX makes
/// the offset atomic per syscall, so concurrent writers (sessions, test threads)
/// never interleave one line's bytes. This is the single append path for every
/// hook-side JSONL journal — anything else (e.g. `writeln!`, which goes through
/// `write_fmt` in several chunks) measurably produces concatenated lines
/// (4/84 on the intent journal, 2026-10-01).
pub fn append_json_line(path: &Path, value: &serde_json::Value) -> std::io::Result<()> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)?;
    }
    let mut json = serde_json::to_string(value).map_err(std::io::Error::other)?;
    json.push('\n');
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    f.write_all(json.as_bytes())
}

/// Parent pid via libc — `std` has no portable accessor.
pub fn parent_pid() -> u32 {
    // SAFETY: getppid has no preconditions and cannot fail.
    unsafe { libc::getppid() as u32 }
}

extern "C" fn flush_at_exit() {
    let Some(Some(st)) = STATE.get() else {
        return;
    };
    let route = st.route.lock().map(|r| *r).unwrap_or("unknown");
    let reason = st.reason.lock().map(|r| *r).unwrap_or("unknown");
    let session_id = st.session_id.lock().ok().and_then(|s| s.clone());
    let (stdin_state, stdin_bytes) = crate::hook_runtime_ext::last_stdin_read();
    let (tool, class) = st.event.lock().map(|e| e.clone()).unwrap_or_default();
    let (verdict, gate) = st.answer.lock().map(|a| a.clone()).unwrap_or_default();
    let mut line = build_line(
        &st.hook,
        route,
        reason,
        stdin_state.as_str(),
        stdin_bytes,
        st.started.elapsed().as_millis() as u64,
        session_id.as_deref(),
        std::process::id(),
        parent_pid(),
    );
    line.tool = tool;
    line.class = class;
    line.verdict = verdict.map(str::to_owned);
    line.gate = gate;
    // Fail-open by contract: a trace that cannot be written must never surface.
    let _ = append_line(&st.path, &line);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Run THIS test binary again as a child that installs the trace and exits through
    /// `mode`; return the `exit_reason` of the single line the child left behind.
    fn child_exit_reason(test_name: &str, mode: &str) -> String {
        let dir = tempfile::tempdir().expect("tempdir");
        let trace = dir.path().join("trace.jsonl");
        let status = std::process::Command::new(std::env::current_exe().expect("test exe"))
            .args(["--exact", test_name, "--nocapture", "--test-threads=1"])
            .env("TOURING_TEST_EMIT_CHILD", mode)
            .env(ENV_TRACE_FILE, &trace)
            .status()
            .expect("spawn child");
        assert!(status.success(), "child exited with {status:?}");
        let body = std::fs::read_to_string(&trace).expect("the child wrote its trace line");
        let line: serde_json::Value =
            serde_json::from_str(body.lines().last().expect("one line")).expect("json line");
        line["exit_reason"].as_str().unwrap_or_default().to_string()
    }

    /// S14 (25/09/2026): `emit` exits the process; the trace must record WHY first.
    /// Until S14 the label stayed at the install default `handler-diverged` (42/42
    /// permission-request lines), so exits were indistinguishable from crashes.
    #[test]
    fn emit_records_exit_reason_before_exit() {
        match std::env::var("TOURING_TEST_EMIT_CHILD").as_deref() {
            Ok("allow") => {
                install("test-emit");
                crate::runtime::HookResponse::Allow.emit();
            }
            Ok("json") => {
                install("test-emit");
                crate::runtime::HookResponse::Context {
                    context: "c".into(),
                    event_name: None,
                }
                .emit();
            }
            Ok("ctx") => {
                install("test-emit");
                crate::runtime::HookRuntime::emit_context_for_event("c", Some("PostToolUse"));
            }
            _ => {}
        }
        let me = "hook_trace::tests::emit_records_exit_reason_before_exit";
        assert_eq!(child_exit_reason(me, "allow"), "emit-allow");
        assert_eq!(child_exit_reason(me, "json"), "emit-json");
        assert_eq!(child_exit_reason(me, "ctx"), "emit-json");
    }

    #[test]
    fn line_shape_carries_route_state_and_timing() {
        let line = build_line(
            "post-bash",
            "daemon",
            "daemon-json",
            "ok",
            412,
            13,
            Some("sess-1"),
            4242,
            4141,
        );
        assert_eq!(line.hook, "post-bash");
        assert_eq!(line.route, "daemon");
        assert_eq!(line.exit_reason, "daemon-json");
        assert_eq!(line.stdin_state, "ok");
        assert_eq!(line.stdin_bytes, 412);
        assert_eq!(line.elapsed_ms, 13);
        assert_eq!(line.session_id.as_deref(), Some("sess-1"));
        assert_eq!((line.pid, line.ppid), (4242, 4141));
        assert!(line.ts_ms > 1_700_000_000_000, "epoch ms, not seconds");
    }

    #[test]
    fn append_line_writes_exactly_one_parseable_json_line_and_creates_parents() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("nested/deeper/hook_trace.jsonl");
        let a = build_line(
            "cli-suggest",
            "daemon",
            "daemon-json",
            "ok",
            9,
            1,
            None,
            1,
            0,
        );
        let b = build_line(
            "post-bash",
            "standalone",
            "standalone-unknown",
            "empty",
            0,
            2,
            None,
            2,
            0,
        );
        append_line(&path, &a).expect("append a");
        append_line(&path, &b).expect("append b");
        let text = std::fs::read_to_string(&path).expect("read back");
        let lines: Vec<HookTraceLine> = text
            .lines()
            .map(|l| serde_json::from_str(l).expect("each line is one JSON object"))
            .collect();
        assert_eq!(lines, vec![a, b]);
        assert!(!text.contains("session_id"), "None is elided, never `null`");
    }

    #[test]
    fn append_line_to_an_unwritable_path_is_an_error_not_a_panic() {
        let line = build_line("help", "stateless", "help", "unread", 0, 0, None, 1, 0);
        let err = append_line(
            Path::new("/proc/definitely/not/writable/trace.jsonl"),
            &line,
        );
        assert!(err.is_err());
    }

    #[test]
    fn setters_without_a_sink_are_no_ops() {
        // The unit-test process never calls `install`: every setter must be a
        // harmless no-op (no panic, no state created).
        set_route("daemon");
        set_reason("daemon-json");
        set_session_id(Some("x"));
        assert!(STATE.get().is_none(), "setters must never create the state");
    }

    #[test]
    fn parent_pid_is_a_live_process() {
        let ppid = parent_pid();
        assert!(ppid > 0);
        assert!(Path::new(&format!("/proc/{ppid}")).exists());
    }
}

/// #6 (04/11/2026) — what the trace may say about a decision, and what it never says.
#[cfg(test)]
mod decision_fields_tests {
    use super::{classify_answer, command_class, gate_tag};
    use touring_hooks_core::hook_response::HookResponse;

    #[test]
    fn the_class_is_the_bare_program_never_an_argument_or_a_path() {
        assert_eq!(command_class("cargo test -p x").as_deref(), Some("cargo"));
        assert_eq!(
            command_class("RUST_LOG=debug cargo t").as_deref(),
            Some("cargo")
        );
        assert_eq!(
            command_class("/usr/bin/grep -rn secret /home/u").as_deref(),
            Some("grep")
        );
        assert_eq!(command_class("~/bin/x").as_deref(), Some("x"));
        assert_eq!(command_class("   ").as_deref(), None);
        assert_eq!(
            command_class("$(evil)").as_deref(),
            None,
            "nothing outside the safe set"
        );
    }

    #[test]
    fn verdicts_follow_claude_codes_decision_shapes() {
        assert_eq!(classify_answer("").0, "silent");
        assert_eq!(classify_answer("{}").0, "silent");
        assert_eq!(classify_answer("prose").0, "silent");
        assert_eq!(
            classify_answer(&HookResponse::deny("[G7 re-inspeção] x").to_json()).0,
            "deny"
        );
        assert_eq!(
            classify_answer(&HookResponse::block("go on").to_json()).0,
            "block"
        );
        assert_eq!(
            classify_answer(&HookResponse::halt("stop").to_json()).0,
            "halt"
        );
        assert_eq!(
            classify_answer(&HookResponse::context("hint").to_json()).0,
            "context"
        );
        let rewrite = r#"{"hookSpecificOutput":{"hookEventName":"PreToolUse","updatedInput":{"command":"ls"}}}"#;
        assert_eq!(classify_answer(rewrite).0, "rewrite");
        let allow = r#"{"hookSpecificOutput":{"permissionDecision":"allow"}}"#;
        assert_eq!(classify_answer(allow).0, "allow");
    }

    #[test]
    fn the_gate_is_the_canonical_id_or_a_short_pathless_tag() {
        assert_eq!(
            gate_tag("[G7 re-inspeção] 3ª leitura").as_deref(),
            Some("G7")
        );
        assert_eq!(gate_tag("[E3 contrafactual] x").as_deref(), Some("E3"));
        assert_eq!(
            gate_tag("[CODE MODE · rajada] x").as_deref(),
            Some("CODE MODE · rajada")
        );
        assert_eq!(gate_tag("[/home/u/secret] x"), None, "a path never leaves");
        assert_eq!(gate_tag("no tag at all"), None);
        let deny = HookResponse::deny("[G10 exec-burst] 5ª execução").to_json();
        assert_eq!(classify_answer(&deny), ("deny", Some("G10".to_owned())));
    }
}
