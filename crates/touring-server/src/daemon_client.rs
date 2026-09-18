//! Daemon socket RPC client — the neutral seam between `cli/` and `server/`.
//!
//! Extracted from `cli/mod.rs` (Session A / step A1 of the touring-server
//! split) so that neither the CLI subcommand handlers nor the MCP server tools
//! reach across the `cli` ↔ `server` module boundary to talk to the daemon.
//! Both sides now depend on this leaf module instead
//! (`crate::daemon_client::daemon_query`).
//!
//! Wire format: newline-delimited JSON by default; an rkyv-framed envelope when
//! built with `--features rkyv-ipc` (the daemon dispatches on the first byte so
//! both paths interoperate on the same socket). The response is always JSON.

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

/// Default daemon socket read timeout, in seconds.
///
/// Kept modest so a genuinely wedged daemon surfaces quickly. Heavy hooks
/// ([`touring_foundation::is_heavy_hook`]) wait past the server budget instead
/// ([`wait_plan`]), which never overrides an explicit `--timeout`.
pub(crate) const DEFAULT_DAEMON_READ_TIMEOUT_SECS: u64 = 120;

/// Daemon socket read timeout in seconds. Set by `--timeout` CLI flag.
/// Default: 120s (was 30s — caused EOF on heavy ops like index rebuild).
pub static DAEMON_READ_TIMEOUT_SECS: AtomicU64 = AtomicU64::new(DEFAULT_DAEMON_READ_TIMEOUT_SECS);

/// Set by `--timeout`, so [`wait_plan`] can tell an operator's choice
/// from the untouched default.
///
/// A sentinel comparison against `DEFAULT_DAEMON_READ_TIMEOUT_SECS` cannot:
/// `--timeout 120` is byte-identical to the default and would be silently
/// raised to the floor — the exact opposite of the "explicit choice always
/// wins" contract this module documents.
static TIMEOUT_SET_BY_OPERATOR: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// Record that the operator passed `--timeout` explicitly.
pub fn mark_timeout_explicit() {
    TIMEOUT_SET_BY_OPERATOR.store(true, Ordering::Relaxed);
}

/// How one daemon call waits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct WaitPlan {
    /// Socket read timeout for this call.
    read_timeout_secs: u64,
    /// The daemon keeps working after the client stops waiting, so a read
    /// timeout must say "poll", never "retry".
    heavy: bool,
}

/// The wait of one call to `hook`, from the configured timeout (`base`) and
/// whether the operator set it.
///
/// A heavy hook ([`touring_foundation::is_heavy_hook`], the same list the daemon
/// budgets by) waits past the server budget
/// ([`touring_foundation::HEAVY_OP_CLIENT_FLOOR_SECS`]) so the typed
/// `budget_exceeded` reply arrives; `index rebuild` on this workspace ran past
/// 120 s and the CLI abandoned a rebuild that was progressing. An explicit
/// `--timeout` always wins, `--timeout 120` included.
///
/// Computed per call and never stored: the floor and the heavy flag used to be
/// written into process statics that nothing reset, so in the long-lived MCP
/// server one heavy call left every later light call waiting ~31 minutes and
/// told to poll instead of retry (cross-audit 14/09/2026, R2-7).
fn wait_plan(hook: &str, base: u64, set_by_operator: bool) -> WaitPlan {
    let heavy = touring_foundation::is_heavy_hook(hook);
    let read_timeout_secs = if heavy && !set_by_operator {
        base.max(touring_foundation::HEAVY_OP_CLIENT_FLOOR_SECS)
    } else {
        base
    };
    WaitPlan {
        read_timeout_secs,
        heavy,
    }
}

// ── Socket client ───────────────────────────────────────────────────────

/// Path to the daemon Unix socket.
///
/// W12.5 unification (2026-07-24): delegates to the foundation resolver
/// (canonical env → legacy env → per-project walk-up → global fallback). The
/// old local copy only honored the legacy env var and skipped the walk-up, so
/// the CLI client could talk to the global daemon while inside an opted-in
/// per-project directory.
fn daemon_socket_path() -> PathBuf {
    touring_foundation::config::TouringConfig::resolve_daemon_socket_path()
}

unsafe extern "C" {
    fn getuid() -> u32;
}

/// Return the current process's real user ID via the libc `getuid(2)` syscall.
///
/// Crate-visible because several CLI diagnostic handlers (`daemon_ctl`,
/// `doctor`, `entity`) build their own per-user paths and need the raw uid.
///
/// # Safety
///
/// `getuid(2)` is always safe to call — it cannot fail and touches no
/// caller-provided memory; the `unsafe` marker only reflects the FFI boundary.
pub(crate) unsafe fn libc_getuid() -> u32 {
    // SAFETY: `getuid(2)` cannot fail and touches no caller-provided memory (see the
    // `# Safety` note above); the explicit block satisfies `unsafe_op_in_unsafe_fn`.
    unsafe { getuid() }
}

/// Send a request to the daemon and return the response JSON string.
///
/// The request is formatted as a newline-delimited JSON DaemonRequest:
/// `{"hook":"<hook_name>","payload":<payload>,"project_root":"<cwd>"}`
///
/// Retries with exponential backoff on E11 (socket backlog full).
pub fn daemon_query(hook: &str, payload: serde_json::Value) -> anyhow::Result<String> {
    let plan = wait_plan(
        hook,
        DAEMON_READ_TIMEOUT_SECS.load(Ordering::Relaxed),
        TIMEOUT_SET_BY_OPERATOR.load(Ordering::Relaxed),
    );
    let socket_path = daemon_socket_path();
    let mut last_err = None;
    for attempt in 0..5 {
        match UnixStream::connect(&socket_path) {
            Ok(stream) => {
                return send_daemon_request(stream, hook, payload, plan);
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                last_err = Some(e);
                if attempt < 4 {
                    let delay = std::time::Duration::from_millis(200 << attempt);
                    std::thread::sleep(delay);
                    continue;
                }
            }
            Err(e) => return Err(connect_failure(hook, &socket_path, &e)),
        }
    }
    Err(last_err
        .unwrap_or_else(|| std::io::Error::other("connect failed"))
        .into())
}

/// Send request after connection established, handle timeout and response parsing.
///
/// Wire format: newline-delimited JSON by default. When built with
/// `--features rkyv-ipc` this emits an rkyv-framed envelope instead; the
/// daemon dispatches on the first byte so both paths interoperate on the
/// same socket. The response is always JSON (the daemon writes JSON back
/// to keep the client parser unchanged).
fn send_daemon_request(
    mut stream: UnixStream,
    hook: &str,
    payload: serde_json::Value,
    plan: WaitPlan,
) -> anyhow::Result<String> {
    let read_timeout = plan.read_timeout_secs;
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(read_timeout)))
        .ok();
    stream
        .set_write_timeout(Some(std::time::Duration::from_secs(10)))
        .ok();
    // F0-pre (2026-07-20): normalize the raw cwd to a REAL project root before it
    // keys any per-project state daemon-side. Raw `current_dir()` here spawned a
    // stray `.claude/touring/` shard per working directory (the "29 stray DBs"
    // class — this session's decompose DAGs landed in 2 different strays).
    let project_root = std::env::current_dir()
        .map(|p| {
            touring_foundation::TouringConfig::normalize_project_root(&p)
                .to_string_lossy()
                .to_string()
        })
        .unwrap_or_default();
    #[cfg(feature = "rkyv-ipc")]
    let use_rkyv = std::env::var("TOURING_RKYV_IPC")
        .map(|v| v != "0" && !v.eq_ignore_ascii_case("false"))
        .unwrap_or(true);
    #[cfg(not(feature = "rkyv-ipc"))]
    let use_rkyv = false;
    #[cfg(feature = "rkyv-ipc")]
    if use_rkyv {
        let req = touring_rkyv::IpcRequest {
            hook: hook.to_string(),
            payload: serde_json::to_vec(&payload)?,
            project_root: project_root.clone(),
            session_id: String::new(),
            priority: 0,
        };
        let frame =
            touring_rkyv::frame_request(&req).map_err(|e| anyhow::anyhow!("rkyv IPC encoding failed: {e} — enable 'rkyv-ipc' feature or set TOURING_RKYV_IPC=0"))?;
        stream.write_all(&frame)?;
        stream.flush()?;
    }
    if !use_rkyv {
        let request = serde_json::json!(
            { "hook" : hook, "payload" : payload, "project_root" : project_root, }
        );
        serde_json::to_writer(&stream, &request)?;
        stream.write_all(b"\n")?;
        stream.flush()?;
    }
    let mut response_bytes = Vec::new();
    if let Err(e) = stream.read_to_end(&mut response_bytes) {
        return Err(read_failure(hook, &e, read_timeout, plan.heavy));
    }
    let response: DaemonResponse = parse_daemon_response(&response_bytes).map_err(|e| {
        anyhow::anyhow!(
            "Failed to parse daemon response: {} — run `touring doctor -j` to verify daemon health",
            e
        )
    })?;
    if !response.success {
        return Err(failure_error(&response.output));
    }
    Ok(response.output)
}

/// Exit code of a CLI command whose handler exceeded its budget: `EX_TEMPFAIL`
/// from sysexits — the daemon is busy and the work may still complete, so the
/// caller retries later instead of reading it as a semantic error.
pub(crate) const DAEMON_BUSY_EXIT_CODE: i32 = 75;

/// Exit code of a CLI command whose HEAVY handler exceeded its budget and keeps
/// running (`retryable: false`). Not 75: a retry of `index rebuild` waits for the
/// running walk and then repeats it, so "try again later" is the wrong advice.
/// The caller polls `touring index status` instead. Outside the sysexits range
/// (64-78) so no tool reads it as one of those (decision H2, 14/09/2026).
pub(crate) const DAEMON_STILL_RUNNING_EXIT_CODE: i32 = 79;

/// A handler that exceeded its execution budget (`error_kind: budget_exceeded`).
/// Kept apart from every other failure so `main` can exit with
/// [`DaemonBusy::exit_code`] and print the daemon's JSON payload on stdout.
#[derive(Debug)]
pub struct DaemonBusy {
    /// The daemon's JSON failure payload, verbatim.
    pub payload: String,
    /// The human message (`Daemon returned success=false: …`).
    pub message: String,
    /// The payload's `retryable`. A daemon that predates the field only sent it
    /// for light hooks, so its absence reads as `true`.
    pub retryable: bool,
}

impl DaemonBusy {
    /// [`DAEMON_BUSY_EXIT_CODE`] when a retry is safe, otherwise
    /// [`DAEMON_STILL_RUNNING_EXIT_CODE`].
    pub fn exit_code(&self) -> i32 {
        if self.retryable {
            DAEMON_BUSY_EXIT_CODE
        } else {
            DAEMON_STILL_RUNNING_EXIT_CODE
        }
    }
}

impl std::fmt::Display for DaemonBusy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for DaemonBusy {}

/// The error for a failed daemon response: [`DaemonBusy`] for a budget failure,
/// a plain message otherwise.
fn failure_error(output: &str) -> anyhow::Error {
    let message = daemon_failure_message(output);
    let parsed = serde_json::from_str::<serde_json::Value>(output.trim()).ok();
    let busy = parsed
        .as_ref()
        .is_some_and(|v| v.get("error_kind").and_then(|k| k.as_str()) == Some("budget_exceeded"));
    if busy {
        let retryable = parsed
            .as_ref()
            .and_then(|v| v.get("retryable"))
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(true);
        anyhow::Error::new(DaemonBusy {
            payload: output.trim().to_string(),
            message,
            retryable,
        })
    } else {
        anyhow::anyhow!("{message}")
    }
}

/// Name the cause of a failed socket read instead of forwarding the bare errno.
///
/// A read timeout arrives as `WouldBlock`, whose `Display` is "Resource
/// temporarily unavailable (os error 11)". Propagated verbatim, that sent an
/// investigation after file descriptors and memory pressure when the real story
/// was `index rebuild` running past the client's budget — the elapsed time was
/// 2m0.058s against a 120s timeout, and the daemon was working the whole time.
///
/// Same lesson as [`daemon_failure_message`] one branch over: the failure path
/// that carries no context is the one that costs the hours.
///
/// A heavy call is the exception to "retry with a larger budget": the daemon never
/// cancels, so a re-run waits for the running walk and then repeats it. There the
/// message says to poll (cross-audit 14/09/2026, A3).
fn read_failure(hook: &str, err: &std::io::Error, timeout_secs: u64, heavy: bool) -> anyhow::Error {
    use std::io::ErrorKind;
    if heavy && matches!(err.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) {
        return anyhow::anyhow!(
            "`{hook}` returned no response within {timeout_secs}s (socket read timeout). \
             The daemon never cancels a heavy operation, so it is most likely still running. \
             Do not re-run it: a second call waits for the running work and then repeats it. \
             Poll `touring index status` (or `touring daemon-ctl status`) until it finishes."
        );
    }
    if matches!(err.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) {
        return anyhow::anyhow!(
            "`{hook}` returned no response within {timeout_secs}s (socket read timeout). \
             The daemon may still be running the request — heavy operations such as a full \
             `index rebuild` outlast the default on large workspaces. Retry with a larger \
             budget: `touring --timeout <secs> …`."
        );
    }
    anyhow::anyhow!(
        "reading the daemon response for `{hook}`: {err} — verify daemon is running with `touring daemon-ctl status`"
    )
}

/// Name the cause of a failed socket CONNECT instead of forwarding the bare errno.
///
/// A per-project daemon is spawned by the hook of a session opened in that
/// project, never by the CLI — so a project nobody opened has no socket, and
/// `touring index rebuild` there failed with "No such file or directory (os error
/// 2)", naming neither the socket nor the remedy (measured 13/09/2026 on
/// konverter, right after `touring update` reported "daemon: not running").
fn connect_failure(hook: &str, socket: &std::path::Path, err: &std::io::Error) -> anyhow::Error {
    use std::io::ErrorKind;
    let socket = socket.display();
    match err.kind() {
        ErrorKind::NotFound => anyhow::anyhow!(
            "`{hook}`: no daemon is listening — the socket {socket} does not exist. \
             Start it with `touring daemon-ctl restart --socket {socket}`."
        ),
        ErrorKind::ConnectionRefused => anyhow::anyhow!(
            "`{hook}`: the socket {socket} exists but nothing accepts connections (a daemon \
             that died without cleaning up). Start a fresh one with \
             `touring daemon-ctl restart --socket {socket}`."
        ),
        _ => anyhow::anyhow!("`{hook}`: connecting to the daemon at {socket}: {err}"),
    }
}

/// Build a *diagnosable* failure message from the daemon's response payload.
///
/// The handlers already produce specific causes — `cli_memory_reindex` returns
/// `{"error":"ANN recall not initialised — daemon startup did not call
/// init_ann_memory"}` — but this call site used to discard `output` entirely and
/// surface only "Daemon returned success=false". On 2026-08-02 that turned a real
/// outage (the memory subsystem wedged behind a long-running handler) into an
/// undiagnosable one and sent the investigation to a wrong root cause. Failing
/// loud is cheap; guessing is not.
fn daemon_failure_message(output: &str) -> String {
    const PREFIX: &str = "Daemon returned success=false";
    const MAX_SNIPPET: usize = 400;
    let trimmed = output.trim();
    if trimmed.is_empty() {
        return format!("{PREFIX} (empty response payload)");
    }
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed)
        && let Some(err) = value.get("error").and_then(|e| e.as_str())
    {
        return format!("{PREFIX}: {err}");
    }
    // Not JSON, or JSON without an `error` key: carry a bounded snippet rather
    // than nothing. Truncation counts CHARS, never bytes — a byte slice can
    // split a multi-byte UTF-8 boundary and panic on an accented message.
    let mut snippet: String = trimmed.chars().take(MAX_SNIPPET).collect();
    if trimmed.chars().count() > MAX_SNIPPET {
        snippet.push('…');
    }
    format!("{PREFIX}: {snippet}")
}

#[cfg(test)]
mod read_failure_tests {
    use super::{
        DEFAULT_DAEMON_READ_TIMEOUT_SECS, WaitPlan, connect_failure, read_failure, wait_plan,
    };
    use touring_foundation::HEAVY_OP_CLIENT_FLOOR_SECS;

    /// A missing or dead socket names the socket and the exact command that starts
    /// the daemon; the bare "os error 2" is what must not reach the operator.
    #[test]
    fn a_missing_or_dead_socket_names_the_socket_and_the_remedy() {
        let socket = std::path::Path::new("/proj/k/.touring/daemon.sock");
        for kind in [
            std::io::ErrorKind::NotFound,
            std::io::ErrorKind::ConnectionRefused,
        ] {
            let err = std::io::Error::from(kind);
            let msg = connect_failure("cli-index-rebuild", socket, &err).to_string();
            assert!(msg.contains("/proj/k/.touring/daemon.sock"), "{msg}");
            assert!(
                msg.contains("touring daemon-ctl restart --socket /proj/k/.touring/daemon.sock"),
                "{msg}"
            );
            assert!(msg.contains("cli-index-rebuild"), "{msg}");
            assert!(!msg.contains("os error"), "{msg}");
        }
        let other = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied here");
        let msg = connect_failure("cli-status", socket, &other).to_string();
        assert!(
            msg.contains("denied here") && msg.contains("daemon.sock"),
            "{msg}"
        );
    }

    /// The bug: a read timeout surfaces as `WouldBlock`, whose Display is
    /// "Resource temporarily unavailable (os error 11)". Verbatim, that reads as
    /// resource exhaustion and hides both the real cause and the existing knob.
    #[test]
    fn a_read_timeout_says_timeout_and_names_the_knob() {
        let err = std::io::Error::new(std::io::ErrorKind::WouldBlock, "eagain");
        let msg = read_failure("cli-memory-recall", &err, 120, false).to_string();
        assert!(msg.contains("120s"), "{msg}");
        assert!(msg.contains("timeout"), "{msg}");
        assert!(msg.contains("--timeout"), "{msg}");
        assert!(msg.contains("cli-memory-recall"), "{msg}");
        assert!(
            !msg.contains("Resource temporarily unavailable"),
            "the raw errno is exactly what must not reach the operator: {msg}"
        );
    }

    /// A non-timeout read failure keeps its own cause — the fix must not flatten
    /// every error into "probably a timeout".
    #[test]
    fn other_read_failures_keep_their_cause() {
        let err = std::io::Error::new(std::io::ErrorKind::ConnectionReset, "peer reset");
        let msg = read_failure("cli-status", &err, 120, false).to_string();
        assert!(msg.contains("peer reset"), "{msg}");
        assert!(!msg.contains("--timeout"), "{msg}");
    }

    /// Cross-audit 14/09/2026 (A3): "retry with a larger budget" on a heavy call
    /// started a second walk behind the first. A heavy timeout says to poll.
    #[test]
    fn a_heavy_read_timeout_says_poll_never_retry() {
        let err = std::io::Error::new(std::io::ErrorKind::WouldBlock, "eagain");
        let msg = read_failure("cli-index-rebuild", &err, 1860, true).to_string();
        assert!(msg.contains("1860s"), "{msg}");
        assert!(msg.contains("touring index status"), "{msg}");
        assert!(msg.contains("Do not re-run"), "{msg}");
        assert!(
            !msg.contains("--timeout"),
            "the knob is the wrong advice here: {msg}"
        );
    }

    /// Cross-audit 14/09/2026 (A3): the client listens past the server budget for
    /// every heavy hook, not only the two call sites that remembered to raise it
    /// (`ast blast` or `pre-task-scout` used to give up at 120 s under a 1800 s
    /// budget); a light hook keeps the quick default.
    #[test]
    fn every_heavy_hook_waits_past_the_server_budget_and_light_ones_do_not() {
        let base = DEFAULT_DAEMON_READ_TIMEOUT_SECS;
        for hook in ["cli-index-rebuild", "cli-ast-blast", "cli-pre-task-scout"] {
            assert_eq!(
                wait_plan(hook, base, false),
                WaitPlan {
                    read_timeout_secs: HEAVY_OP_CLIENT_FLOOR_SECS,
                    heavy: true
                },
                "{hook}"
            );
        }
        assert_eq!(
            wait_plan("cli-memory-store", base, false),
            WaitPlan {
                read_timeout_secs: base,
                heavy: false
            }
        );
    }

    /// Cross-audit 14/09/2026 (R2-7): the plan of one call never leaks into the
    /// next. The floor and the heavy flag used to live in process statics, so a
    /// long-lived MCP server kept both after its first heavy call.
    #[test]
    fn a_heavy_call_leaves_the_next_light_call_as_it_was() {
        let base = DEFAULT_DAEMON_READ_TIMEOUT_SECS;
        assert!(wait_plan("cli-index-rebuild", base, false).heavy);
        let light = wait_plan("cli-memory-recall", base, false);
        assert_eq!(light.read_timeout_secs, base);
        assert!(!light.heavy);
    }

    /// The floor lifts the default, and an operator's explicit `--timeout` wins,
    /// including one equal to the default: the sentinel version compared against
    /// `DEFAULT_…_SECS` and overwrote `--timeout 120` with the floor.
    #[test]
    fn an_explicit_timeout_always_wins_over_the_heavy_floor() {
        assert_eq!(
            wait_plan("cli-index-rebuild", 45, true).read_timeout_secs,
            45
        );
        assert_eq!(
            wait_plan("cli-index-rebuild", DEFAULT_DAEMON_READ_TIMEOUT_SECS, true)
                .read_timeout_secs,
            DEFAULT_DAEMON_READ_TIMEOUT_SECS,
            "--timeout 120 is the operator's choice, not the untouched default"
        );
        assert!(
            wait_plan("cli-index-rebuild", 45, true).heavy,
            "an explicit timeout changes the wait, never what a timeout means"
        );
        assert_eq!(
            wait_plan("cli-index-rebuild", 4000, false).read_timeout_secs,
            4000,
            "a floor never lowers a longer configured wait"
        );
    }
}

#[cfg(test)]
mod daemon_failure_message_tests {
    use super::daemon_failure_message;

    #[test]
    fn carries_the_handler_error_field() {
        let msg = daemon_failure_message(r#"{"error":"ANN recall not initialised"}"#);
        assert!(msg.contains("ANN recall not initialised"), "{msg}");
    }

    #[test]
    fn reports_an_empty_payload_as_such() {
        assert!(daemon_failure_message("   ").contains("empty response payload"));
    }

    #[test]
    fn falls_back_to_a_snippet_without_an_error_key() {
        let msg = daemon_failure_message(r#"{"status":"partial","failed":7}"#);
        assert!(msg.contains("partial") && msg.contains("failed"), "{msg}");
    }

    #[test]
    fn truncates_on_char_boundaries_never_panicking() {
        let long = "ç".repeat(1_000); // 2 bytes each — a byte-slice would panic
        let msg = daemon_failure_message(&long);
        assert!(msg.ends_with('…'));
        assert!(msg.chars().count() < 1_000);
    }
}

#[derive(serde::Deserialize)]
struct DaemonResponse {
    output: String,
    success: bool,
}

/// Parse a raw daemon response into [`DaemonResponse`] using dual-path
/// detection: rkyv-framed if the bytes start with the `RKYV` magic header
/// (Wave 3 D4), JSON otherwise.
///
/// The rkyv branch is gated by `feature = "rkyv-ipc"`. With the feature
/// off, only JSON is accepted — keeping the legacy build path identical.
///
/// # Errors
///
/// Returns `anyhow::Error` when:
/// * Bytes start with `RKYV` magic but framing/bytecheck fails.
/// * Bytes are JSON but `serde_json::from_slice` fails.
/// * Buffer is empty (daemon hung up before sending anything).
fn parse_daemon_response(bytes: &[u8]) -> anyhow::Result<DaemonResponse> {
    if bytes.is_empty() {
        anyhow::bail!(
            "daemon closed connection without responding — check `touring daemon-ctl status` \
             (a restarting daemon auto-spawns on the next call; rerun the command)"
        );
    }
    #[cfg(feature = "rkyv-ipc")]
    {
        if bytes.len() >= touring_rkyv::IPC_FRAME_HEADER_LEN
            && bytes.get(..4) == Some(&touring_rkyv::IPC_MAGIC[..])
        {
            let body = touring_rkyv::unframe(bytes)
                .map_err(|e| anyhow::anyhow!("rkyv unframe response: {e} — daemon and client version mismatch, try `touring daemon-ctl restart`"))?;
            let archived = touring_rkyv::check_archived_root::<touring_rkyv::IpcResponse>(body)
                .map_err(|e| anyhow::anyhow!("rkyv bytecheck response: {e:?} — restart daemon: `touring daemon-ctl restart`"))?;
            return Ok(DaemonResponse {
                output: archived.output.to_string(),
                success: archived.success,
            });
        }
    }
    let trimmed = trim_trailing_newline(bytes);
    serde_json::from_slice::<DaemonResponse>(trimmed).map_err(|e| {
        anyhow::anyhow!(
            "json parse: {e} — daemon response malformed, restart: `touring daemon-ctl restart`"
        )
    })
}

/// Strip a single trailing `\n` (and optional `\r`) from a byte slice
/// without allocating. The daemon emits `<json>\n` over the JSON path; this
/// helper makes the parser tolerant of both forms.
fn trim_trailing_newline(bytes: &[u8]) -> &[u8] {
    let mut end = bytes.len();
    if bytes.get(end - 1) == Some(&b'\n') {
        end -= 1;
        if bytes.get(end - 1) == Some(&b'\r') {
            end -= 1;
        }
    }
    bytes.get(..end).unwrap_or(bytes)
}

#[cfg(test)]
mod daemon_busy_tests {
    use super::{DAEMON_BUSY_EXIT_CODE, DAEMON_STILL_RUNNING_EXIT_CODE, DaemonBusy, failure_error};

    /// A budget failure surfaces as `DaemonBusy` (exit code 75, payload kept);
    /// every other failure stays a plain error.
    #[test]
    fn only_a_budget_failure_is_daemon_busy() {
        let busy = failure_error(
            r#"{"error":"handler `cli-memory-store` exceeded its 15s budget","error_kind":"budget_exceeded","handler":"cli-memory-store","budget_secs":15,"retryable":true}"#,
        );
        let typed = busy.downcast_ref::<DaemonBusy>().expect("typed busy error");
        assert!(typed.payload.contains("\"error_kind\":\"budget_exceeded\""));
        assert!(busy.to_string().contains("exceeded its 15s budget"));
        assert_eq!(DAEMON_BUSY_EXIT_CODE, 75);
        let plain = failure_error(r#"{"error":"key not found"}"#);
        assert!(plain.downcast_ref::<DaemonBusy>().is_none());
        assert!(plain.to_string().contains("key not found"));
    }

    /// Decision H2 (14/09/2026): only a retryable budget failure exits 75. A heavy
    /// hook still running exits with its own code, because a retry repeats the walk.
    #[test]
    fn the_exit_code_follows_retryable() {
        let exit_of = |payload: &str| {
            failure_error(payload)
                .downcast_ref::<DaemonBusy>()
                .expect("typed busy error")
                .exit_code()
        };
        assert_eq!(
            exit_of(
                r#"{"error":"x","error_kind":"budget_exceeded","handler":"cli-index-rebuild","still_running":true,"retryable":false}"#
            ),
            DAEMON_STILL_RUNNING_EXIT_CODE
        );
        assert_eq!(
            exit_of(
                r#"{"error":"x","error_kind":"budget_exceeded","handler":"cli-memory-store","still_running":true,"retryable":true}"#
            ),
            DAEMON_BUSY_EXIT_CODE
        );
        assert_eq!(
            exit_of(r#"{"error":"x","error_kind":"budget_exceeded","handler":"cli-memory-store"}"#),
            DAEMON_BUSY_EXIT_CODE,
            "a daemon that predates `retryable` only reported light hooks"
        );
        assert_ne!(DAEMON_STILL_RUNNING_EXIT_CODE, DAEMON_BUSY_EXIT_CODE);
    }
}
