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

/// Daemon socket read timeout in seconds. Set by `--timeout` CLI flag.
/// Default: 120s (was 30s — caused EOF on heavy ops like index rebuild).
pub static DAEMON_READ_TIMEOUT_SECS: AtomicU64 = AtomicU64::new(120);

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
    let socket_path = daemon_socket_path();
    let mut last_err = None;
    for attempt in 0..5 {
        match UnixStream::connect(&socket_path) {
            Ok(stream) => {
                return send_daemon_request(stream, hook, payload);
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                last_err = Some(e);
                if attempt < 4 {
                    let delay = std::time::Duration::from_millis(200 << attempt);
                    std::thread::sleep(delay);
                    continue;
                }
            }
            Err(e) => return Err(e.into()),
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
) -> anyhow::Result<String> {
    let read_timeout = DAEMON_READ_TIMEOUT_SECS.load(Ordering::Relaxed);
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
            touring_rkyv::frame_request(&req).map_err(|e| anyhow::anyhow!("rkyv frame: {e}"))?;
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
    stream.read_to_end(&mut response_bytes)?;
    let response: DaemonResponse = parse_daemon_response(&response_bytes)
        .map_err(|e| anyhow::anyhow!("Failed to parse daemon response: {}", e))?;
    if !response.success {
        anyhow::bail!("{}", daemon_failure_message(&response.output));
    }
    Ok(response.output)
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
        anyhow::bail!("daemon closed connection without responding");
    }
    #[cfg(feature = "rkyv-ipc")]
    {
        if bytes.len() >= touring_rkyv::IPC_FRAME_HEADER_LEN
            && bytes.get(..4) == Some(&touring_rkyv::IPC_MAGIC[..])
        {
            let body = touring_rkyv::unframe(bytes)
                .map_err(|e| anyhow::anyhow!("rkyv unframe response: {e}"))?;
            let archived = touring_rkyv::check_archived_root::<touring_rkyv::IpcResponse>(body)
                .map_err(|e| anyhow::anyhow!("rkyv bytecheck response: {e:?}"))?;
            return Ok(DaemonResponse {
                output: archived.output.to_string(),
                success: archived.success,
            });
        }
    }
    let trimmed = trim_trailing_newline(bytes);
    serde_json::from_slice::<DaemonResponse>(trimmed)
        .map_err(|e| anyhow::anyhow!("json parse: {e}"))
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
