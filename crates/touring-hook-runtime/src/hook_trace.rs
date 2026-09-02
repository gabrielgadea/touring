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
pub const ENV_TRACE_FILE: &str = "TOURING_HOOK_TRACE_FILE";

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
    /// `ok` | `empty` | `timeout` | `terminal` | `read-error` | `parse-error` | `unread`.
    pub stdin_state: String,
    /// Bytes read from stdin (0 when unread/empty/timeout).
    pub stdin_bytes: u64,
    /// Wall time from `install` to exit.
    pub elapsed_ms: u64,
    /// Claude Code session id from the payload, when present (attribution only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
}

struct TraceState {
    started: Instant,
    hook: String,
    path: PathBuf,
    route: Mutex<&'static str>,
    reason: Mutex<&'static str>,
    session_id: Mutex<Option<String>>,
}

static STATE: OnceLock<Option<TraceState>> = OnceLock::new();

/// Arm the trace for this process. Idempotent; a no-op when the env var is
/// unset or empty. Registers the `atexit` flush on first arming.
pub fn install(hook: &str) {
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
    }
}

/// Append one line to `path` (creating parents and the file as needed).
pub fn append_line(path: &Path, line: &HookTraceLine) -> std::io::Result<()> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)?;
    }
    let mut json = serde_json::to_string(line).map_err(std::io::Error::other)?;
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
    let line = build_line(
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
    // Fail-open by contract: a trace that cannot be written must never surface.
    let _ = append_line(&st.path, &line);
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let a = build_line("cli-suggest", "daemon", "daemon-json", "ok", 9, 1, None, 1, 0);
        let b = build_line("post-bash", "standalone", "standalone-unknown", "empty", 0, 2, None, 2, 0);
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
        let err = append_line(Path::new("/proc/definitely/not/writable/trace.jsonl"), &line);
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
