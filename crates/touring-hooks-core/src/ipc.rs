//! IPC protocol types for the Touring daemon.
//!
//! Shared between the thin client (main.rs) and the daemon server (daemon.rs).
//! Wire format: newline-delimited JSON over a Unix domain socket.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Path of the Unix domain socket for the daemon.
///
/// Resolution chain (first match wins):
/// 1. `TOURING_DAEMON_SOCKET` env var (W12.5 — new canonical name)
/// 2. `TOURING_DAEMON_SOCK` env var (legacy — kept for backward compat)
/// 3. **Per-project walk-up** (W12.5 — 2026-05-23): from CWD (or
///    `$CLAUDE_PROJECT_DIR` if set), walk up looking for
///    `<dir>/.touring/daemon.sock`. First existing wins. Until
///    `touring init-project` (W12.1) is run + a daemon binds the per-project
///    socket, this layer NEVER matches in production — so this change is
///    zero-impact for current users.
/// 4. **Global fallback**: `/tmp/touring-daemon-<uid>.sock` (matches the
///    historical convention; REGRA #2.5 keeps backward compatibility with
///    existing running daemons).
pub fn daemon_socket_path() -> PathBuf {
    // W12.5 unification (2026-07-24): delegate to the single source of truth.
    // The old inline copy predates touring-hooks-core's touring-foundation
    // dependency (its "would introduce a cycle" note was stale) and had
    // drifted from the foundation resolver's semantics.
    touring_foundation::config::TouringConfig::resolve_daemon_socket_path()
}

/// Path of the PID/lock file that guards against multiple daemon instances
/// **on the global socket** (legacy name kept so a live pre-W12.5 daemon and a
/// post-W12.5 daemon agree on the same lock file across an upgrade).
pub fn daemon_lock_path() -> PathBuf {
    let uid = crate::current_uid();
    PathBuf::from(format!("/tmp/touring-daemon-{uid}.lock"))
}

/// W12.5 — per-socket lock path: the singleton guard scopes to ONE socket, so
/// N per-project daemons coexist while two daemons racing for the SAME socket
/// still serialize (REGRA #19 idempotent resolution).
///
/// The global socket keeps the legacy uid-only lock name (see
/// [`daemon_lock_path`]); every other socket derives
/// `/tmp/touring-daemon-<uid>-<fnv1a64/8hex>.lock` from its canonicalized-ish
/// absolute path. FNV-1a is inlined (stable across rustc versions and builds —
/// `DefaultHasher` is NOT guaranteed stable, and two binaries disagreeing on
/// the lock name would let two daemons bind the same socket).
pub fn daemon_lock_path_for(socket: &std::path::Path) -> PathBuf {
    touring_foundation::config::TouringConfig::daemon_lock_path_for(socket)
}

/// A request sent from the thin client to the daemon.
///
/// `hook` is the subcommand name (e.g. `"pre-read"`, `"post-bash"`).
/// `payload` is the raw stdin JSON that Claude Code provides.
#[derive(Debug, Serialize, Deserialize)]
pub struct DaemonRequest {
    /// Hook subcommand name (e.g. `"pre-read"`, `"post-bash"`).
    pub hook: String,
    /// Raw stdin JSON payload provided by Claude Code for this hook.
    pub payload: serde_json::Value,
    /// Project root so the daemon can scope its DB correctly.
    pub project_root: String,
    /// Session ID for per-session request tracking and priority.
    /// Used to weight semaphore acquisition and enable per-session isolation.
    #[serde(default)]
    pub session_id: Option<String>,
    /// Request priority (0-255) for weighted scheduling.
    /// Higher values = higher priority. Used to prioritize light ops
    /// over heavy ops (e.g., index-find > mcts-search).
    #[serde(default)]
    pub priority: u8,
    /// C2-W0 S-5.2 — identity of an orchestrate sub-call, `<run_id>:code:<n>`.
    /// Set only by the in-sandbox SDK (`touring run --orchestrate`); `None`
    /// on every other request. Wire-compatible both ways: serde fills the
    /// default on old clients and ignores the unknown field on old daemons.
    /// The dispatch site uses it to count the counterfactual tool-part
    /// (d4: the context bytes tool calling would have cost).
    #[serde(default)]
    pub origin: Option<String>,
    /// PID of the connecting client (`SO_PEERCRED` via `UnixStream::peer_cred`),
    /// stamped by the daemon at accept time — never on the wire. It is what lets
    /// the `heavy op:` log line name its sender: until 21/08/2026 six
    /// `cli-index-rebuild` receipts in a row were unattributable, and the
    /// emitter (generator S-4) took a night of bisection to find.
    #[serde(skip)]
    pub peer_pid: Option<i32>,
}

/// A response returned by the daemon to the thin client.
///
/// `output` is the JSON string to print to stdout (empty string = no output).
/// `success` is false only for internal daemon errors — hooks never fail hard.
#[derive(Debug, Serialize, Deserialize)]
pub struct DaemonResponse {
    /// JSON string to print to stdout verbatim (may be empty).
    pub output: String,
    /// False only on internal daemon errors; hooks themselves never fail hard.
    pub success: bool,
}

// NOTE: getuid FFI consolidated in crate::current_uid()

#[cfg(test)]
mod tests {
    use super::*;

    /// C2-W0 S-5.2 — the `origin` field is wire-compatible in BOTH
    /// directions: a request without it parses (old SDK → new daemon), and
    /// one carrying it round-trips intact (new SDK → new daemon). An old
    /// daemon ignores the unknown field (serde's default), so no combination
    /// of versions breaks the socket.
    #[test]
    fn daemon_request_origin_roundtrip_and_backcompat() {
        let legacy = r#"{"hook":"cli-index-find","payload":{},"project_root":"/tmp"}"#;
        let req: DaemonRequest = serde_json::from_str(legacy).expect("legacy request parses");
        assert_eq!(req.origin, None, "absent field defaults to None");

        let stamped = r#"{"hook":"cli-index-find","payload":{"symbol_name":"X"},"project_root":"/tmp","origin":"run-1-2:code:3"}"#;
        let req: DaemonRequest = serde_json::from_str(stamped).expect("stamped request parses");
        assert_eq!(req.origin.as_deref(), Some("run-1-2:code:3"));

        let wire = serde_json::to_string(&req).expect("serialize");
        let back: DaemonRequest = serde_json::from_str(&wire).expect("round-trip");
        assert_eq!(back.origin.as_deref(), Some("run-1-2:code:3"));
    }
}
