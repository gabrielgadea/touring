//! `touring daemon-ctl` — canonical daemon lifecycle helper (REGRA #19).
//!
//! Replaces the catastrophic anti-pattern `pkill -f touring`, which would
//! kill the daemon singleton AND every MCP bridge (`touring serve`) AND
//! every hook handler (`touring-hook <event>`) AND every CLI client of
//! every concurrent Claude Code session — cascading degradation across
//! unrelated work.
//!
//! Subcommands:
//!   * `status`  — JSON-capable status: socket alive? daemon PID? exe deleted?
//!     plus sibling process census (mcp_bridges / hook_handlers / cli_clients)
//!     so the operator sees collateral risk before acting.
//!   * `stop`    — SIGTERM the singleton daemon only. Idempotent (no-op if
//!     already down). Emits WARN when sibling MCP bridges exist because their
//!     queries will fail until respawn.
//!   * `restart` — `stop` + spawn the dedicated `touring-daemon` binary in its
//!     own session (`setsid`) with stderr appended to
//!     `~/.claude/touring/daemon.stderr.log`. Falls back to SIGKILL only on the
//!     singleton PID if SIGTERM doesn't drain the socket within 10s — siblings
//!     untouched.
//!   * `reset`   — Nuclear: SIGKILL daemon + remove stale socket + lock file.
//!     Requires explicit `--yes-i-know-cascading-kill` flag; the LLM cannot
//!     set this flag autonomously (REGRA #19 bypass protocol).
//!
//! Process identification (transition state, pre-PC):
//!   The daemon is identified by walking `/proc/*/cmdline` and matching
//!   `argv[0].ends_with("/touring-hook") && argv[1] == "--start-daemon"`.
//!   This avoids `pgrep`'s cmdname truncation (15 chars) AND distinguishes
//!   the daemon from hook handlers that share the same exe path.
//!
//! Symbol verification (REGRA #15):
//!   * `daemon_socket_path` — created_this_subtask, mirrors the one in
//!     `crate::daemon_client` intentionally to avoid coupling. Same env
//!     override (`TOURING_DAEMON_SOCK`).
//!   * `super::libc_getuid` — imported_existing (re-exported from
//!     `crate::daemon_client`, extern "C").
//!   * `kill` (libc) — declared via extern "C" locally; libc 0.2 is in the
//!     workspace but not a direct dep of touring-server, mirroring the
//!     getuid pattern in cli/mod.rs.
//!   * `setsid` (libc) — same local-extern pattern (daemon lifetime fix
//!     2026-07-01): gives the spawned daemon its own session so it survives
//!     the invoking Claude Code session's cleanup (killpg/SIGHUP).

use clap::{Parser, Subcommand};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use super::common::{human_to_stderr, json_to_stdout};

// POSIX signal constants. We declare the extern locally to avoid coupling
// touring-server to libc as a direct dependency — the same pattern used by
// cli/mod.rs for getuid().
unsafe extern "C" {
    fn kill(pid: i32, sig: i32) -> i32;
    fn setsid() -> i32;
}
const SIGTERM: i32 = 15;
const SIGKILL: i32 = 9;
const ESRCH: i32 = 3; // No such process — idempotent kill outcome.

// ── CLI types (Wave P3-1.3 W6b clap-derive migration) ────────────────────────

/// `touring daemon-ctl` — canonical daemon lifecycle (REGRA #19, never pkill).
#[derive(Parser, Debug)]
#[command(
    name = "touring daemon-ctl",
    bin_name = "touring daemon-ctl",
    about = "Canonical daemon lifecycle (REGRA #19): status | restart | stop | reset",
    disable_help_subcommand = true
)]
struct DaemonCtlCli {
    #[command(subcommand)]
    cmd: Option<DaemonCtlCmd>,
    /// JSON output to stdout.
    #[arg(short = 'j', long, global = true)]
    json: bool,
    /// W12.5: target an explicit daemon socket (multi-daemon aware).
    #[arg(long, global = true, value_name = "PATH")]
    socket: Option<PathBuf>,
    /// W12.5: target the per-project daemon of this project directory
    /// (resolves `<dir>/.touring/daemon.sock`; overrides the walk-up).
    #[arg(long, global = true, value_name = "DIR", conflicts_with = "socket")]
    project: Option<PathBuf>,
}

#[derive(Subcommand, Debug)]
enum DaemonCtlCmd {
    /// Show daemon status: socket alive, PID, exe deleted?, sibling census.
    Status,
    /// Graceful restart: SIGTERM singleton + respawn (siblings untouched).
    Restart,
    /// Graceful stop: SIGTERM singleton only (no respawn).
    Stop,
    /// Nuclear reset: SIGKILL + cleanup socket/lock (requires explicit flag).
    Reset {
        /// Bypass protection: confirm intent to cascade-kill all touring processes.
        ///
        /// The LLM must NOT set this flag autonomously (REGRA #19 bypass protocol).
        #[arg(long)]
        yes_i_know_cascading_kill: bool,
    },
    /// W12.5: list every known daemon (global + per-project) with liveness.
    ListAll,
}

/// W12.5 — resolve the socket this invocation targets: explicit `--socket`
/// wins, then `--project <dir>` (its `.touring/daemon.sock`), then the
/// standard resolver (env → walk-up → global).
fn resolve_target_socket(cli: &DaemonCtlCli) -> PathBuf {
    if let Some(s) = &cli.socket {
        return s.clone();
    }
    if let Some(dir) = &cli.project {
        return dir.join(".touring").join("daemon.sock");
    }
    daemon_socket_path()
}

/// W12.5 — the PID of the daemon holding `socket`. Registry-first (F-NEW-4,
/// cross-audit 2026-07-25): the per-socket REGISTRY entry is rewritten on
/// every bind and is therefore fresh across respawns; the lock-file CONTENT
/// can go stale (observed: a day-old PID in the global lock made this fn
/// return None → the all-daemons fallback SIGTERMed every per-project daemon
/// — a cascading kill, the exact REGRA #19 failure daemon-ctl exists to
/// prevent). Both sources are comm-validated against `/proc`.
fn pid_for_socket(socket: &Path) -> Option<u32> {
    use touring_foundation::config::TouringConfig;
    // 1. Registry entry (written fresh on every bind — W12.5).
    let reg = TouringConfig::daemon_registry_entry_for(socket);
    if let Some(pid) = fs::read_to_string(reg)
        .ok()
        .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
        .and_then(|v| v.get("pid").and_then(serde_json::Value::as_u64))
        .map(|p| p as u32)
        .filter(|&p| read_proc_comm(p).as_deref() == Some("touring-daemon"))
    {
        return Some(pid);
    }
    // 2. Legacy fallback: lock-file content (upgrade-compat; may be stale).
    let lock = TouringConfig::daemon_lock_path_for(socket);
    let pid: u32 = fs::read_to_string(lock).ok()?.trim().parse().ok()?;
    (read_proc_comm(pid).as_deref() == Some("touring-daemon")).then_some(pid)
}

/// Fallback reap-set when the target socket's owner cannot be identified:
/// every daemon EXCEPT registered owners of OTHER sockets. The old
/// "reap them all" (`all_daemon_pids`) predates W12.5 multi-daemon — in the
/// per-project era it cascade-killed unrelated projects' daemons (F-NEW-4:
/// konverter's daemon died on every `update-touring` restart of the global).
fn orphan_daemon_pids(target: &Path) -> Vec<u32> {
    use touring_foundation::config::TouringConfig;
    let mut owned_elsewhere = std::collections::HashSet::new();
    if let Ok(entries) = fs::read_dir(TouringConfig::daemon_registry_dir()) {
        for e in entries.flatten() {
            if let Some(v) = fs::read_to_string(e.path())
                .ok()
                .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
            {
                let sock = v.get("socket").and_then(serde_json::Value::as_str);
                let pid = v
                    .get("pid")
                    .and_then(serde_json::Value::as_u64)
                    .map(|p| p as u32);
                if let (Some(sock), Some(pid)) = (sock, pid) {
                    if Path::new(sock) != target {
                        owned_elsewhere.insert(pid);
                    }
                }
            }
        }
    }
    all_daemon_pids()
        .into_iter()
        .filter(|p| !owned_elsewhere.contains(p))
        .collect()
}

// ── Public entry ─────────────────────────────────────────────────────

/// Entry point for the `touring daemon-ctl` CLI handler — parses the argv
/// slice into a `DaemonCtlCmd` and dispatches to `status` (default),
/// `restart`, `stop`, or `reset`. Implements the canonical daemon lifecycle
/// (REGRA #19), never `pkill`.
pub fn run(args: &[String]) -> anyhow::Result<()> {
    let cli = match DaemonCtlCli::try_parse_from(args.iter().skip(1)) {
        Ok(c) => c,
        Err(e) => e.exit(),
    };
    let target = resolve_target_socket(&cli);
    match cli.cmd.unwrap_or(DaemonCtlCmd::Status) {
        DaemonCtlCmd::Status => cmd_status(cli.json, &target),
        DaemonCtlCmd::Restart => cmd_restart(cli.json, &target),
        DaemonCtlCmd::Stop => cmd_stop(cli.json, &target),
        DaemonCtlCmd::Reset {
            yes_i_know_cascading_kill,
        } => cmd_reset(cli.json, yes_i_know_cascading_kill, &target),
        DaemonCtlCmd::ListAll => cmd_list_all(cli.json),
    }
}

// ── Status ───────────────────────────────────────────────────────────

#[derive(serde::Serialize, Default)]
struct SiblingCounts {
    mcp_bridges: usize,
    hook_handlers: usize,
    cli_clients: usize,
}

#[derive(serde::Serialize)]
struct DaemonStatus {
    socket_path: String,
    socket_alive: bool,
    daemon_pid: Option<u32>,
    daemon_exe: Option<String>,
    daemon_exe_deleted: bool,
    health: Option<serde_json::Value>,
    sibling_processes: SiblingCounts,
}

fn cmd_status(json: bool, target: &Path) -> anyhow::Result<()> {
    let socket_path = target.to_path_buf();
    let socket_alive = std::os::unix::net::UnixStream::connect(&socket_path).is_ok();
    // W12.5: targeted PID via the per-socket lock; the comm-scan stays as a
    // back-compat fallback for a pre-upgrade global daemon without a lock PID.
    let daemon_pid = pid_for_socket(&socket_path).or_else(find_daemon_pid);
    let daemon_exe = daemon_pid.and_then(read_proc_exe);
    let daemon_exe_deleted = daemon_exe
        .as_deref()
        .is_some_and(|e| e.contains("(deleted)"));

    let health = if socket_alive {
        super::daemon_query("__health__", serde_json::json!({}))
            .ok()
            .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
    } else {
        None
    };

    let sibling_processes = count_sibling_processes(daemon_pid);

    let status = DaemonStatus {
        socket_path: socket_path.display().to_string(),
        socket_alive,
        daemon_pid,
        daemon_exe,
        daemon_exe_deleted,
        health,
        sibling_processes,
    };

    if json {
        json_to_stdout(&serde_json::to_string_pretty(&status)?);
    } else {
        let pid_str = status
            .daemon_pid
            .map(|p| p.to_string())
            .unwrap_or_else(|| "(none)".to_string());
        let exe_str = status.daemon_exe.as_deref().unwrap_or("(none)");
        let socket = if status.socket_alive { "alive" } else { "dead" };
        let deleted = if status.daemon_exe_deleted {
            " (deleted — rebuild ran without restart)"
        } else {
            ""
        };
        human_to_stderr(&format!(
            "Touring daemon status\n  \
               socket:     {} ({socket})\n  \
               daemon PID: {pid_str}\n  \
               exe:        {exe_str}{deleted}\n  \
               siblings:   mcp_bridges={}, hook_handlers={}, cli_clients={}",
            status.socket_path,
            status.sibling_processes.mcp_bridges,
            status.sibling_processes.hook_handlers,
            status.sibling_processes.cli_clients,
        ));
    }
    Ok(())
}

// ── Stop / Restart / Reset ───────────────────────────────────────────

fn cmd_stop(json: bool, target: &Path) -> anyhow::Result<()> {
    // W12.5: stop ONLY the daemon holding the target socket; the comm-scan
    // fallback covers a pre-upgrade global daemon without a lock PID.
    let pids = pid_for_socket(target)
        .map(|p| vec![p])
        .unwrap_or_else(|| orphan_daemon_pids(target));
    if pids.is_empty() {
        if json {
            json_to_stdout(r#"{"stopped":false,"reason":"daemon_not_running"}"#);
        } else {
            human_to_stderr("Touring daemon is not running.");
        }
        return Ok(());
    }

    let siblings = count_sibling_processes(pids.first().copied());
    if siblings.mcp_bridges > 0 && !json {
        human_to_stderr(&format!(
            "WARN: {} MCP bridge(s) active (touring serve). Their current queries \
             will fail until the daemon is restarted. (Bridges themselves are NOT killed.)",
            siblings.mcp_bridges
        ));
    }

    // SIGTERM every daemon — the singleton invariant means >1 is an orphan set
    // to reap (each does a graceful flush before exit).
    for &pid in &pids {
        send_signal(pid, SIGTERM)?;
    }
    let socket = daemon_socket_path();
    let drained = wait_socket_gone(&socket, Duration::from_secs(10));

    if json {
        let out = serde_json::json!({
            "stopped": true,
            "pids": pids,
            "socket_drained": drained,
            "mcp_bridges_affected": siblings.mcp_bridges,
        });
        json_to_stdout(&serde_json::to_string(&out)?);
    } else {
        human_to_stderr(&format!(
            "Stopped {} daemon process(es) {:?} (SIGTERM). socket drained: {drained}",
            pids.len(),
            pids
        ));
    }
    Ok(())
}

fn cmd_restart(json: bool, target: &Path) -> anyhow::Result<()> {
    restart_socket_with_bin(json, target, None)
}

/// Restart the daemon on `target`, optionally forcing a specific daemon
/// binary for the respawn (F3: `touring update` restarts a per-project daemon
/// on the project's freshly-linked `.touring/bin/touring-daemon`, not the dev
/// channel). `None` keeps the standard resolution (env > dev > PATH).
pub(crate) fn restart_socket_with_bin(
    json: bool,
    target: &Path,
    daemon_bin: Option<&Path>,
) -> anyhow::Result<()> {
    let pids = pid_for_socket(target)
        .map(|p| vec![p])
        .unwrap_or_else(|| orphan_daemon_pids(target));
    // Loud no-op guard (cross-audit 2026-07-24): a live socket whose owner the
    // scan cannot identify means the SIGTERM below would be skipped, the fresh
    // spawn would lose the flock race to the invisible owner, and this command
    // would report a "successful restart" that restarted nothing.
    if pids.is_empty() && std::os::unix::net::UnixStream::connect(target).is_ok() {
        anyhow::bail!(
            "a daemon holds {} but its PID could not be identified (no lock PID, \
             no comm match) — refusing a fake restart. Inspect with `daemon-ctl \
             list-all` / lsof.",
            target.display()
        );
    }
    if !pids.is_empty() {
        // SIGTERM every daemon (each flushes WAL/KPI gracefully). >1 means a
        // split-brain orphan set — reap them all so the respawn is a clean
        // singleton, not yet another layer on the pile.
        for &pid in &pids {
            send_signal(pid, SIGTERM)?;
        }
        let socket = target.to_path_buf();
        let drained = wait_socket_gone(&socket, Duration::from_secs(10));
        if !drained {
            // Force-kill anything still holding the socket (re-scan: graceful
            // exits may have already cleared some PIDs).
            for pid in all_daemon_pids() {
                send_signal(pid, SIGKILL)?;
            }
            wait_socket_gone(&socket, Duration::from_secs(3));
        }
    }

    spawn_daemon_with_bin(target, daemon_bin)?;

    let booted = wait_socket_alive(target, Duration::from_secs(15));
    let socket = target.to_path_buf();

    if json {
        let out = serde_json::json!({
            "restarted": booted,
            "socket": socket.display().to_string(),
        });
        json_to_stdout(&serde_json::to_string(&out)?);
    } else if booted {
        human_to_stderr("Touring daemon restarted successfully.");
    } else {
        anyhow::bail!("daemon respawned but socket did not become available within 15s");
    }

    if !booted && json {
        // JSON branch already emitted; keep exit code 0 to preserve the
        // machine-readable contract — operators inspect `restarted` field.
    }
    Ok(())
}

fn cmd_reset(json: bool, yes_i_know_cascading_kill: bool, target: &Path) -> anyhow::Result<()> {
    // SECURITY: this check is the sole enforcement of REGRA #19 bypass protocol.
    // The --yes-i-know-cascading-kill flag must be supplied explicitly.
    // The LLM cannot supply this flag autonomously — it requires human intent.
    if !yes_i_know_cascading_kill {
        anyhow::bail!(
            "`reset` is nuclear: it SIGKILLs the daemon and removes stale lock/sock files. \
             Other CC sessions may experience transient hook degradation until respawn. \
             Pass --yes-i-know-cascading-kill to confirm intent."
        );
    }

    // SIGKILL every daemon — reset is nuclear, so reap the entire orphan set
    // (split-brain socket-owner + lock-holder), not just the first /proc match.
    for pid in all_daemon_pids() {
        send_signal(pid, SIGKILL)?;
    }
    let socket = target.to_path_buf();
    let _ = fs::remove_file(&socket);
    // W12.5: remove the lock DERIVED from the target socket (per-socket locks)
    // plus the legacy uid-global lock — reset is nuclear cleanup, both names
    // must go or a stale one blocks the respawn.
    let derived = touring_foundation::config::TouringConfig::daemon_lock_path_for(&socket);
    let _ = fs::remove_file(&derived);
    // SAFETY: `getuid(2)` is a thread-safe POSIX syscall with no memory effects,
    // mirroring the convention in cli/mod.rs::libc_getuid (same extern decl).
    let uid = unsafe { super::libc_getuid() };
    let lock = PathBuf::from(format!("/tmp/touring-daemon-{uid}.lock"));
    let _ = fs::remove_file(&lock);

    if json {
        json_to_stdout(r#"{"reset":true}"#);
    } else {
        human_to_stderr("Daemon reset complete (SIGKILL + socket/lock cleanup).");
    }
    Ok(())
}

// ── W12.5 list-all ───────────────────────────────────────────────────

#[derive(serde::Serialize)]
struct DaemonListEntry {
    socket: String,
    pid: Option<u32>,
    alive: bool,
    /// false = discovered by direct probe (pre-registry daemon), not registry.
    registered: bool,
}

/// List every known daemon: registry entries (validated live, stale pruned)
/// plus a direct probe of the global socket for pre-upgrade daemons that
/// never registered. Read-only except for pruning provably-dead entries.
fn cmd_list_all(json: bool) -> anyhow::Result<()> {
    use touring_foundation::config::TouringConfig;
    let mut entries: Vec<DaemonListEntry> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

    if let Ok(read) = fs::read_dir(TouringConfig::daemon_registry_dir()) {
        for e in read.flatten() {
            let path = e.path();
            let Some(socket) = fs::read_to_string(&path)
                .ok()
                .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
                .and_then(|r| r.get("socket").and_then(|v| v.as_str()).map(String::from))
            else {
                continue;
            };
            let sock_path = PathBuf::from(&socket);
            let alive = std::os::unix::net::UnixStream::connect(&sock_path).is_ok();
            let pid = pid_for_socket(&sock_path);
            if !alive && pid.is_none() {
                // Provably dead (no socket, no live lock holder) — prune the
                // stale entry a SIGKILLed daemon left behind.
                let _ = fs::remove_file(&path);
                continue;
            }
            seen.insert(socket.clone());
            entries.push(DaemonListEntry {
                socket,
                pid,
                alive,
                registered: true,
            });
        }
    }

    // The global daemon may predate the registry — probe it directly.
    // SAFETY: `getuid(2)` is infallible and thread-safe (POSIX guarantee).
    let uid = unsafe { super::libc_getuid() };
    let global = PathBuf::from(format!("/tmp/touring-daemon-{uid}.sock"));
    if !seen.contains(&global.display().to_string())
        && std::os::unix::net::UnixStream::connect(&global).is_ok()
    {
        entries.push(DaemonListEntry {
            socket: global.display().to_string(),
            pid: pid_for_socket(&global).or_else(find_daemon_pid),
            alive: true,
            registered: false,
        });
    }

    if json {
        json_to_stdout(&serde_json::to_string_pretty(&serde_json::json!({
            "count": entries.len(),
            "daemons": entries,
        }))?);
    } else if entries.is_empty() {
        human_to_stderr("No touring daemons found (registry empty, global socket dead).");
    } else {
        human_to_stderr(&format!("Touring daemons ({}):", entries.len()));
        for d in &entries {
            human_to_stderr(&format!(
                "  {} pid={} alive={}{}",
                d.socket,
                d.pid.map_or_else(|| "?".to_string(), |p| p.to_string()),
                d.alive,
                if d.registered {
                    ""
                } else {
                    " (unregistered probe)"
                },
            ));
        }
    }
    Ok(())
}

// ── Helpers ──────────────────────────────────────────────────────────

fn daemon_socket_path() -> PathBuf {
    // W12.5 unification (2026-07-24): delegate to the single source of truth
    // (canonical env → legacy env → per-project walk-up → global fallback).
    // The old local copy only honored the legacy env var, so daemon-ctl and
    // the daemon itself could disagree about WHICH socket "the daemon" was.
    touring_foundation::config::TouringConfig::resolve_daemon_socket_path()
}

/// Walk `/proc/*/comm` to find the touring-daemon singleton.
///
/// Sprint 4 PD-2: comm-based detection replaces the prior cmdline parse.
/// PC-1 (`set_process_name`) normalised the comm string to `"touring-daemon"`
/// at every daemon entrypoint (both the dedicated `touring-daemon` binary AND
/// the legacy `touring-hook --start-daemon` polymorphic path, until S-9
/// deletes the latter). Reading comm is:
///   * Robust against cmdname truncation (kernel hard-caps at 15 chars).
///   * Unified across legacy + dedicated daemon paths (single source of truth).
///   * Distinct from sibling kinds (touring-mcp, touring-hook, touring-cli)
///     by construction — each entrypoint sets its own comm.
fn find_daemon_pid() -> Option<u32> {
    let entries = fs::read_dir("/proc").ok()?;
    for entry in entries.flatten() {
        let pid_str = entry.file_name().into_string().ok()?;
        let Ok(pid) = pid_str.parse::<u32>() else {
            continue;
        };
        if read_proc_comm(pid).as_deref() == Some("touring-daemon") {
            return Some(pid);
        }
    }
    None
}

/// All live `touring-daemon` PIDs. The daemon is a singleton (normally 0 or 1),
/// but a crash/suspend race during flock acquisition can leave orphans (a
/// socket-owner ≠ lock-holder split-brain). `stop`/`restart`/`reset` operate over
/// ALL of them so orphans are reaped instead of accumulating across restarts.
/// The `touring-daemon` comm is distinct from `touring-mcp`/`touring-hook`/
/// `touring-cli` (PC-1 `set_process_name`), so this never touches MCP bridges or
/// hook handlers — REGRA #19 safe.
fn all_daemon_pids() -> Vec<u32> {
    let mut pids = Vec::new();
    let Ok(entries) = fs::read_dir("/proc") else {
        return pids;
    };
    for entry in entries.flatten() {
        let Some(name) = entry.file_name().into_string().ok() else {
            continue;
        };
        let Ok(pid) = name.parse::<u32>() else {
            continue;
        };
        if read_proc_comm(pid).as_deref() == Some("touring-daemon") {
            pids.push(pid);
        }
    }
    pids
}

/// Read `/proc/<pid>/comm` (trimmed). Returns `None` on any I/O error.
/// Used by [`find_daemon_pid`] and [`count_sibling_processes`] for the
/// comm-based detection introduced by Sprint 4 PD-2.
fn read_proc_comm(pid: u32) -> Option<String> {
    let path = format!("/proc/{pid}/comm");
    fs::read_to_string(&path).ok().map(|s| s.trim().to_string())
}

fn read_proc_exe(pid: u32) -> Option<String> {
    let exe_path = format!("/proc/{pid}/exe");
    fs::read_link(&exe_path)
        .ok()
        .map(|p| p.display().to_string())
}

/// Classify every other touring process by comm string so the operator
/// can see the collateral footprint before acting. REGRA #19 contract.
///
/// Sprint 4 PD-2: comm-based classification, matching the canonical strings
/// set by PC-1's `set_process_name`:
///   - `touring-mcp`  → MCP bridge (`touring serve`, 1 per CC session)
///   - `touring-hook` → ephemeral hook handler
///   - `touring-cli`  → ephemeral CLI client
///
/// The daemon itself (`touring-daemon`) is excluded via the `daemon_pid` arg.
fn count_sibling_processes(daemon_pid: Option<u32>) -> SiblingCounts {
    let mut counts = SiblingCounts::default();
    let Ok(entries) = fs::read_dir("/proc") else {
        return counts;
    };
    for entry in entries.flatten() {
        let Some(pid_str) = entry.file_name().into_string().ok() else {
            continue;
        };
        let Ok(pid) = pid_str.parse::<u32>() else {
            continue;
        };
        if Some(pid) == daemon_pid {
            continue;
        }
        let Some(comm) = read_proc_comm(pid) else {
            continue;
        };
        match comm.as_str() {
            "touring-mcp" => counts.mcp_bridges += 1,
            "touring-hook" => counts.hook_handlers += 1,
            "touring-cli" => counts.cli_clients += 1,
            // "touring-daemon" with a different PID would be a runaway/zombie —
            // not a sibling category; skip silently.
            _ => {}
        }
    }
    counts
}

fn send_signal(pid: u32, sig: i32) -> anyhow::Result<()> {
    // SAFETY: `kill(2)` is a pure POSIX syscall — no memory effects on the caller.
    // `pid` is upstream-validated to belong to the touring-daemon singleton
    // via `find_daemon_pid` (REGRA #19 comm match — set by PC-1) OR is the
    // synthetic `4_194_303` of the idempotency test. ESRCH is handled below
    // as the documented "process gone" outcome.
    let rc = unsafe { kill(pid as i32, sig) };
    if rc == 0 {
        return Ok(());
    }
    let err = std::io::Error::last_os_error();
    // ESRCH: process already gone — idempotent semantics.
    if err.raw_os_error() == Some(ESRCH) {
        return Ok(());
    }
    anyhow::bail!("kill({pid}, {sig}) failed: {err}");
}

fn wait_socket_gone(socket: &Path, timeout: Duration) -> bool {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if !socket.exists() {
            return true;
        }
        if std::os::unix::net::UnixStream::connect(socket).is_err() {
            // Socket file may linger briefly after daemon exits — accept dead-but-present.
            std::thread::sleep(Duration::from_millis(100));
            if !socket.exists() {
                return true;
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    !socket.exists()
}

fn wait_socket_alive(socket: &Path, timeout: Duration) -> bool {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if std::os::unix::net::UnixStream::connect(socket).is_ok() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    false
}

/// Spawn the daemon on `target`, optionally with an explicit binary (F3
/// per-project respawn). Standard preference order when `bin_override` is
/// `None`: TOURING_DAEMON_BIN env > ~/.local/bin/touring-daemon > PATH.
fn spawn_daemon_with_bin(target: &Path, bin_override: Option<&Path>) -> anyhow::Result<()> {
    use std::process::{Command, Stdio};

    // Sprint 4 PD-2: spawn the DEDICATED `touring-daemon` binary, not the
    // legacy `touring-hook --start-daemon` polymorphic mode (deprecated by
    // S-9). Preference order: explicit override (F3 `touring update`) >
    // TOURING_DAEMON_BIN env > ~/.local/bin/touring-daemon > PATH lookup.
    let binary = bin_override
        .map(|p| p.display().to_string())
        .or_else(|| std::env::var("TOURING_DAEMON_BIN").ok())
        .or_else(|| {
            let home = std::env::var("HOME").ok()?;
            let candidate = PathBuf::from(format!("{home}/.local/bin/touring-daemon"));
            candidate.exists().then(|| candidate.display().to_string())
        });

    let mut cmd = match binary {
        Some(p) => Command::new(p),
        None => Command::new("touring-daemon"),
    };

    // Daemon lifetime fix (2026-07-01) — mirrors touring-hooks
    // `main.rs::try_autostart_daemon`; keep both call-sites in sync (C08).
    // Without its own session the daemon stays in the invoking CLI's process
    // group (a descendant of the Claude Code session) and dies with that
    // session's cleanup killpg/SIGHUP —
    // observed as "daemon alive at session end, Connection refused when the
    // next session starts". setsid(2) gives it a fresh session, a fresh
    // process group, and no controlling terminal.
    // SAFETY: pre_exec runs in the forked child before exec. setsid(2) is
    // async-signal-safe and cannot fail with EPERM here: the freshly forked
    // child has a brand-new PID and is therefore never a process-group leader.
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

    // stdout/stderr go to an append-mode logfile instead of /dev/null so
    // daemon crashes stop being invisible (post-mortem gotcha 2026-06-29:
    // "daemon-ctl spawn stderr=null não loga").
    let (stdout_io, stderr_io) = daemon_log_stdio();

    // W12.5 (1.3): pin the RESOLVED socket into the child's env so the daemon
    // binds exactly where this ctl (and every unified client) will look — the
    // child re-running the walk-up from a different cwd must never diverge.
    // Mirrors `try_autostart_daemon` (C08: keep both spawn sites in sync).
    cmd.env("TOURING_DAEMON_SOCKET", target);

    // PILOT finding (2026-07-24): a per-project daemon must resolve ITS OWN
    // project root, never inherit the invoker's. Without this, `touring
    // update`/`daemon-ctl restart` run from another workspace respawned the
    // konverter daemon with the INVOKER's TOURING_PROJECT_ROOT/cwd — its DBs
    // would land in the wrong project (cross-contamination, the exact failure
    // per-project daemons exist to prevent). The root is derived from the
    // socket itself (`<root>/.touring/daemon.sock`), so every caller is
    // correct by construction; the global socket derives nothing and keeps
    // the standard resolution.
    if let Some(root) = project_root_for_socket(target) {
        cmd.env("CLAUDE_PROJECT_DIR", &root);
        cmd.env("TOURING_PROJECT_ROOT", &root);
        cmd.current_dir(&root);
    }

    cmd.stdin(Stdio::null())
        .stdout(stdout_io)
        .stderr(stderr_io)
        .spawn()
        .map_err(|e| anyhow::anyhow!("spawn touring-daemon: {e}"))?;
    Ok(())
}

/// Derive the project root a per-project socket belongs to:
/// `<root>/.touring/daemon.sock` → `Some(<root>)`; anything else (the global
/// `/tmp` socket, ad-hoc test sockets) → `None`. Deriving from the socket —
/// not the invoker's env — is what makes every spawn caller correct by
/// construction (PILOT finding 2026-07-24).
pub(crate) fn project_root_for_socket(socket: &Path) -> Option<PathBuf> {
    let dot_touring = socket.parent()?;
    if socket.file_name()? == "daemon.sock" && dot_touring.file_name()? == ".touring" {
        dot_touring.parent().map(Path::to_path_buf)
    } else {
        None
    }
}

/// Resolve the spawned daemon's stdout/stderr: an append-mode logfile with a
/// spawn-epoch header, degrading to `Stdio::null()` when the logfile is
/// unavailable — logging must never block the spawn itself (fail-open,
/// REGRA #19).
fn daemon_log_stdio() -> (std::process::Stdio, std::process::Stdio) {
    use std::io::Write;
    use std::process::Stdio;
    let Some(mut f) = daemon_log_file() else {
        return (Stdio::null(), Stdio::null());
    };
    let epoch = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let _ = writeln!(f, "=== daemon-ctl spawn epoch={epoch} ===");
    match f.try_clone() {
        Ok(f2) => (Stdio::from(f2), Stdio::from(f)),
        Err(_) => (Stdio::null(), Stdio::from(f)),
    }
}

/// Append-mode logfile for the spawned daemon's stdout/stderr:
/// `$HOME/.claude/touring/daemon.stderr.log`. Returns `None` when HOME is
/// unset or the directory cannot be created — the caller degrades to
/// `Stdio::null()` (fail-open, REGRA #19).
fn daemon_log_file() -> Option<fs::File> {
    daemon_log_file_at(&std::env::var("HOME").ok()?)
}

/// Testable core of [`daemon_log_file`]: same behaviour, explicit base dir.
fn daemon_log_file_at(home: &str) -> Option<fs::File> {
    let dir = PathBuf::from(home).join(".claude").join("touring");
    fs::create_dir_all(&dir).ok()?;
    fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("daemon.stderr.log"))
        .ok()
}

// ── Tests ────────────────────────────────────────────────────────────

/// Expose this handler's clap Command for the completions aggregator (W7).
pub(super) fn command() -> clap::Command {
    use clap::CommandFactory;
    DaemonCtlCli::command()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|p| p.to_string()).collect()
    }

    #[test]
    fn daemon_log_file_at_creates_dir_and_file() {
        let base = std::env::temp_dir().join(format!("touring-dlf-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let f = daemon_log_file_at(&base.display().to_string());
        assert!(
            f.is_some(),
            "daemon_log_file_at must create <home>/.claude/touring and the logfile"
        );
        assert!(base.join(".claude/touring/daemon.stderr.log").exists());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn daemon_socket_path_respects_env_override() {
        // W12.5 unification: the CANONICAL var wins over the legacy one, and
        // the legacy one still works when the canonical is absent. The session
        // environment may carry TOURING_DAEMON_SOCKET (CC sessions export it),
        // so BOTH vars are saved/cleared — the old single-var version of this
        // test failed exactly because of that leak (2026-07-24).
        let canonical = "TOURING_DAEMON_SOCKET";
        let legacy = "TOURING_DAEMON_SOCK";
        let saved_c = std::env::var(canonical).ok();
        let saved_l = std::env::var(legacy).ok();
        // SAFETY (all set_var/remove_var below): test-local env mutation,
        // restored before return; concurrent readers see valid UTF-8 values.
        unsafe {
            std::env::remove_var(canonical);
            std::env::set_var(legacy, "/tmp/test-touring-daemon-ctl-legacy.sock");
        }
        assert_eq!(
            daemon_socket_path().display().to_string(),
            "/tmp/test-touring-daemon-ctl-legacy.sock",
            "legacy var must be honored when the canonical is absent"
        );
        unsafe {
            std::env::set_var(canonical, "/tmp/test-touring-daemon-ctl-canonical.sock");
        }
        assert_eq!(
            daemon_socket_path().display().to_string(),
            "/tmp/test-touring-daemon-ctl-canonical.sock",
            "canonical var must take precedence over the legacy one"
        );
        unsafe {
            match saved_c {
                Some(v) => std::env::set_var(canonical, v),
                None => std::env::remove_var(canonical),
            }
            match saved_l {
                Some(v) => std::env::set_var(legacy, v),
                None => std::env::remove_var(legacy),
            }
        }
    }

    #[test]
    fn count_sibling_processes_walks_proc_without_panic() {
        // The /proc walk must never panic, even when entries vanish mid-iteration.
        let counts = count_sibling_processes(None);
        // Tautology — exercises field access path; counts may be zero on minimal runners.
        let _total = counts.mcp_bridges + counts.hook_handlers + counts.cli_clients;
    }

    #[test]
    fn all_daemon_pids_walks_proc_without_panic() {
        // The /proc walk must never panic. Every returned pid MUST be a real
        // touring-daemon comm match — the singleton-reaping invariant of
        // stop/restart/reset depends on this never returning unrelated PIDs
        // (e.g. touring-mcp/touring-hook/touring-cli — REGRA #19).
        let pids = all_daemon_pids();
        for &pid in &pids {
            assert_eq!(
                read_proc_comm(pid).as_deref(),
                Some("touring-daemon"),
                "all_daemon_pids must only return touring-daemon comm matches"
            );
        }
    }

    #[test]
    fn send_signal_to_unreachable_pid_is_idempotent() {
        // Linux max default pid_max is 4_194_304; one less is virtually guaranteed absent.
        let r = send_signal(4_194_303, SIGTERM);
        assert!(r.is_ok(), "ESRCH must be treated as success");
    }

    /// CRITICAL SECURITY TEST — REGRA #19 bypass protocol enforcement.
    ///
    /// `reset` without --yes-i-know-cascading-kill MUST refuse with a message
    /// that names the bypass flag. This is the primary guard preventing the LLM
    /// from autonomously issuing nuclear daemon operations.
    #[test]
    fn reset_without_flag_errors_with_explicit_guidance() {
        let result = cmd_reset(false, false, Path::new("/tmp/never-touched.sock"));
        let err = result.expect_err("reset without flag must error");
        let msg = format!("{err}");
        assert!(
            msg.contains("--yes-i-know-cascading-kill"),
            "error message must name the bypass flag: {msg}"
        );
    }

    /// CRITICAL SECURITY TEST — clap parse path: `reset` without the flag
    /// must still result in an error when dispatched through run().
    ///
    /// This verifies the clap-derive wiring preserves the safety semantic:
    /// `yes_i_know_cascading_kill` defaults to false, and cmd_reset refuses.
    #[test]
    fn reset_via_clap_without_flag_errors() {
        let args = s(&["touring", "daemon-ctl", "reset"]);
        let cli = DaemonCtlCli::try_parse_from(args.iter().skip(1))
            .expect("clap should parse reset without flag (bool default=false)");
        match cli.cmd {
            Some(DaemonCtlCmd::Reset {
                yes_i_know_cascading_kill,
            }) => {
                assert!(
                    !yes_i_know_cascading_kill,
                    "flag must default to false when not supplied"
                );
                let result = cmd_reset(
                    false,
                    yes_i_know_cascading_kill,
                    Path::new("/tmp/never-touched.sock"),
                );
                let err = result.expect_err("cmd_reset must refuse when flag is false");
                assert!(
                    err.to_string().contains("--yes-i-know-cascading-kill"),
                    "error must name the bypass flag: {err}"
                );
            }
            other => panic!("expected Reset variant, got {other:?}"),
        }
    }

    /// CRITICAL SECURITY TEST — clap parse path: `reset --yes-i-know-cascading-kill`
    /// must parse the flag as true (the flag is present but we stop before actual
    /// SIGKILL by not calling the function in this test — we only assert the parse).
    #[test]
    fn reset_via_clap_with_flag_parses_true() {
        let args = s(&[
            "touring",
            "daemon-ctl",
            "reset",
            "--yes-i-know-cascading-kill",
        ]);
        let cli = DaemonCtlCli::try_parse_from(args.iter().skip(1))
            .expect("clap should parse reset with flag");
        match cli.cmd {
            Some(DaemonCtlCmd::Reset {
                yes_i_know_cascading_kill,
            }) => {
                assert!(
                    yes_i_know_cascading_kill,
                    "flag must be true when --yes-i-know-cascading-kill is supplied"
                );
            }
            other => panic!("expected Reset variant, got {other:?}"),
        }
    }

    #[test]
    fn read_proc_comm_pid_one_is_not_touring_daemon() {
        // PID 1 is init/systemd — comm is "systemd" / "init", never "touring-daemon".
        let comm = read_proc_comm(1);
        assert!(
            comm.as_deref() != Some("touring-daemon"),
            "PID 1 comm should not be touring-daemon; got {comm:?}"
        );
    }

    #[test]
    fn read_proc_comm_returns_none_for_missing_pid() {
        // High PID that should not exist on any sane host
        assert!(read_proc_comm(4_194_303).is_none());
    }

    #[test]
    fn wait_socket_alive_returns_false_on_nonexistent_path() {
        let bogus = PathBuf::from("/tmp/test-touring-daemon-ctl-never-exists.sock");
        let _ = fs::remove_file(&bogus);
        assert!(!wait_socket_alive(&bogus, Duration::from_millis(200)));
    }

    #[test]
    fn unknown_subcommand_errors() {
        // clap will call process::exit on an unrecognized subcommand when using
        // try_parse_from, but returns Err. We verify the error path exists.
        let args = s(&["touring", "daemon-ctl", "tango"]);
        let result = DaemonCtlCli::try_parse_from(args.iter().skip(1));
        assert!(result.is_err(), "unknown subcommand must return Err");
    }

    #[test]
    fn clap_status_is_default_when_no_subcommand() {
        let args = s(&["touring", "daemon-ctl"]);
        let cli = DaemonCtlCli::try_parse_from(args.iter().skip(1))
            .expect("bare daemon-ctl should parse");
        assert!(
            cli.cmd.is_none(),
            "no subcommand should yield cmd=None (dispatches to Status)"
        );
    }

    #[test]
    fn clap_json_flag_short() {
        let args = s(&["touring", "daemon-ctl", "status", "-j"]);
        let cli = DaemonCtlCli::try_parse_from(args.iter().skip(1)).expect("should parse -j");
        assert!(cli.json, "-j must set json=true");
    }

    #[test]
    fn clap_json_flag_long() {
        let args = s(&["touring", "daemon-ctl", "status", "--json"]);
        let cli = DaemonCtlCli::try_parse_from(args.iter().skip(1)).expect("should parse --json");
        assert!(cli.json, "--json must set json=true");
    }

    #[test]
    fn clap_json_flag_global_on_reset() {
        // --json is global=true so it can appear before or after the subcommand.
        let args = s(&["touring", "daemon-ctl", "--json", "reset"]);
        let cli = DaemonCtlCli::try_parse_from(args.iter().skip(1))
            .expect("--json before subcommand should parse");
        assert!(cli.json);
    }

    #[test]
    fn project_root_derives_only_from_per_project_sockets() {
        // PILOT finding 2026-07-24: the spawn pins the PROJECT's root, derived
        // from the socket itself — never the invoker's env.
        assert_eq!(
            project_root_for_socket(Path::new("/home/u/projects/konverter/.touring/daemon.sock")),
            Some(PathBuf::from("/home/u/projects/konverter"))
        );
        // Global socket → no derivation (standard resolution untouched).
        assert_eq!(
            project_root_for_socket(Path::new("/tmp/touring-daemon-1000.sock")),
            None
        );
        // Ad-hoc test sockets outside a .touring/ dir → no derivation.
        assert_eq!(
            project_root_for_socket(Path::new("/tmp/some-dir/daemon.sock")),
            None
        );
        assert_eq!(
            project_root_for_socket(Path::new("/home/u/proj/.touring/other.sock")),
            None
        );
    }
}
