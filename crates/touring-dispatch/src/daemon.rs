//! Touring daemon — persistent async process that keeps HookRuntime alive between hook calls.
//!
//! ## Problem
//! Each `touring-hook` invocation spawns a fresh process: ELF load (~2ms) +
//! Rust init (~1ms) + SQLite open (~4ms) + logic (~2ms) = ~10ms floor.
//!
//! ## Solution
//! A long-lived daemon holds `HookRuntime` (and its SQLite connections) open.
//! The thin client sends a JSON request over a Unix domain socket and receives
//! the JSON response in <1ms round-trip, cutting total latency to ~2-3ms.
//!
//! ## Async Architecture (v30, 2026-04-12 — Actor Pattern)
//! - **tokio** async I/O: `tokio::net::UnixListener` for non-blocking accept
//! - **Per-project actor**: each project owns a dedicated OS thread
//!   (`touring-project-actor`) running `run_project_actor`. The actor holds
//!   the `HookRuntime` and processes [`ProjectCommand::RunHook`] /
//!   [`ProjectCommand::Shutdown`] serially from a bounded
//!   `mpsc::Sender<ProjectCommand>` (depth 128). Replaces the legacy
//!   `Arc<Mutex<HookRuntime>>` shared state — eliminates kernel-Mutex
//!   contention that previously let 50+ accepted connections pile up blocked
//!   on `.lock()` under Claude Code hook storm.
//! - **Oneshot responses**: each `RunHook` carries a
//!   `oneshot::Sender<String>` the actor uses to reply; clients await the
//!   oneshot with a per-hook budget (15s light / 300s heavy) so a stuck
//!   handler cannot stall dispatch.
//! - **Panic-safe handlers**: every handler + E2E scan runs inside
//!   `std::panic::catch_unwind(AssertUnwindSafe(...))`. A panicking handler
//!   logs via `tracing::error!` and the actor continues — previously a
//!   panic dropped the receiver and permanently deadlocked the project.
//! - **Connection limiting**: global `tokio::sync::Semaphore(64)` + per-project
//!   `Semaphore(56)` (raised from 16 to absorb hook bursts). Both are acquired
//!   via `tokio::time::timeout(REQUEST_TIMEOUT=5s, acquire_owned())` so
//!   backpressure fails fast instead of leaking socket FDs.
//! - **Accept loop resilience**: exponential backoff (`100ms × 2^streak`,
//!   cap 2s) on transient accept errors (EMFILE/ENOBUFS). Streak resets on
//!   success, so transient failures don't accumulate budget.
//! - **Concurrent project access**: `tokio::sync::RwLock` on RuntimeMap allows
//!   multiple concurrent reads of the project → actor map; writes only for
//!   first-access project init / LRU eviction.
//! - **Heavy-hook list**: `cli-tantivy-reindex`, `cli-index-rebuild`,
//!   `cli-ast-blast`, `cli-ast-blast-cross-feature`, `cli-mcts-search`,
//!   `cli-session-start`, `cli-session-assess`, `cli-wiring-chains`,
//!   `cli-wiring-audit`, `cli-e2e`.
//! - **Memory pressure response**: LRU eviction of idle project runtimes when
//!   project count exceeds threshold (50). Dropping a `ProjectRuntime` drops
//!   its `cmd_tx`; the actor's `cmd_rx.blocking_recv()` returns `None` and
//!   the actor thread exits cleanly.
//! - **Idle watchdog**: async interval loop, no dedicated thread needed
//! - **Lock file**: prevents duplicate daemon processes for the same user
//! - **Graceful shutdown**: SIGTERM/SIGINT sends `ProjectCommand::Shutdown`
//!   per actor; each actor runs WAL checkpoint + LinUCB save + CRDT save
//!   inside its thread (panic-guarded per step) and acks via oneshot. Then
//!   `std::process::exit(0)` after 5s timeout per actor.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
use tokio::net::UnixListener;
use tokio::sync::{Semaphore, mpsc, oneshot};
use tokio::time::{interval, timeout};

use crate::ipc::{DaemonRequest, DaemonResponse, daemon_lock_path_for, daemon_socket_path};
use crate::runtime::HookRuntime;
use crate::shared::latency_marker::LatencyMarker;

/// Maximum projects in memory before LRU eviction kicks in.
/// Prevents unbounded memory growth in multi-project scenarios.
const MAX_PROJECTS_IN_MEMORY: usize = 50;

/// Per-project connection limit — prevents any single project from
/// consuming all global connection slots. Raised from 16 to 56 to absorb
/// Claude Code hook bursts (pre_read/post_read/pre_edit/post_edit fire
/// many times per turn on the same project, previously saturating the
/// 16-slot budget and triggering REQUEST_TIMEOUT fail-fast rejections).
const MAX_CONCURRENT_CONNECTIONS_PER_PROJECT: usize = 56;

/// Function pointer type for hook handlers in the dispatch table.
///
/// Each handler receives a mutable reference to the project's `HookRuntime`
/// and the JSON payload, returning a string to send back to the client
/// (empty string = no output / allow).
pub type HookHandler = fn(&mut HookRuntime, &serde_json::Value) -> String;

/// Static dispatch table — built once, reused for every request.
///
/// S10: Delegates to the centralized hook registry for O(1) lookup.
/// Single source of truth: `hook_registry::build_dispatch_table()`.
static HOOK_TABLE: OnceLock<HashMap<&'static str, HookHandler>> = OnceLock::new();

/// T3.6: Per-hook invocation metrics — accumulated during daemon lifetime.
///
/// Each entry tracks (invocations, total_latency_ms) as a pair of AtomicU64.
/// Built lazily from the dispatch table keys on first access.
static HOOK_METRICS: OnceLock<HashMap<&'static str, (AtomicU64, AtomicU64)>> = OnceLock::new();

/// Initialize or retrieve the per-hook metrics map.
fn hook_metrics_map() -> &'static HashMap<&'static str, (AtomicU64, AtomicU64)> {
    HOOK_METRICS.get_or_init(|| {
        let table = HOOK_TABLE.get_or_init(crate::hook_registry::build_dispatch_table);
        table
            .keys()
            .map(|&name| (name, (AtomicU64::new(0), AtomicU64::new(0))))
            .collect()
    })
}

// Wave R+C I3 (2026-06-10): `ProjectCommand` lives in `crate::daemon_protocol`
// (descending to the runtime layer at the next carve). Re-exported so every
// `crate::daemon::ProjectCommand` path keeps resolving unchanged.
pub use crate::daemon_protocol::ProjectCommand;

/// Depth of each per-project mpsc channel. Provides backpressure — when full,
/// `send().await` parks the producer until the actor drains a slot. This is
/// safer than unbounded channels (which can OOM under hook-storm) and safer
/// than try_send (which would reject legitimate requests under transient
/// load). 128 is sized to absorb realistic Claude Code hook bursts without
/// stalling the producer.
const PROJECT_CHANNEL_DEPTH: usize = 128;

/// Debounce window for the post-edit/post-write inline E2E quick scan. The free
/// `cli_e2e` has no TTL cache, so without this an edit burst would run a full
/// workspace walk on the per-project actor for every keystroke (perf F1/F2,
/// 2026-06-13). The scan now runs at most once per window.
const E2E_SCAN_DEBOUNCE: Duration = Duration::from_secs(30);

/// Whether the debounced inline E2E scan is due: `true` if it has never run, or
/// if at least `window` has elapsed since the last run. Pure + deterministic so
/// the debounce contract is unit-testable without driving the actor (perf T-03).
fn e2e_scan_due(last: Option<Instant>, now: Instant, window: Duration) -> bool {
    match last {
        None => true,
        Some(prev) => now.saturating_duration_since(prev) >= window,
    }
}

/// S3: Per-project runtime handle with tracking for LRU eviction and per-project connection limiting.
///
/// The legacy `Arc<Mutex<HookRuntime>>` has been replaced with an mpsc sender
/// to the project actor task. The actor OWNS the `HookRuntime` and processes
/// commands serially (required: `rusqlite::Connection` is `!Sync`) — but
/// *without* the kernel Mutex contention that previously serialized 50+
/// accepted connections behind a single lock, starving long-running requests
/// (e.g. tantivy reindex) indefinitely.
struct ProjectRuntime {
    /// mpsc sender cloned to each client dispatch path.
    cmd_tx: mpsc::Sender<ProjectCommand>,
    /// Last time this project was accessed (used for LRU eviction).
    last_accessed: Instant,
    /// Per-project connection semaphore — enforces per-project concurrency
    /// quota (`MAX_CONCURRENT_CONNECTIONS_PER_PROJECT`). Cloned and acquired
    /// in [`dispatch_request_async`] so a single runaway project cannot
    /// starve global capacity (L7-B audit fix).
    connection_semaphore: Arc<Semaphore>,
}

impl ProjectRuntime {
    /// Spawn the project actor on a dedicated OS thread and return the handle.
    ///
    /// A dedicated `std::thread::spawn` is used (not `tokio::task::spawn_blocking`)
    /// because the actor is long-lived — keeping it in Tokio's blocking pool
    /// would permanently consume a pool slot per project and starve
    /// `spawn_blocking` consumers (SQLite query paths in handlers).
    fn new(rt: HookRuntime) -> Self {
        let (cmd_tx, cmd_rx) = mpsc::channel::<ProjectCommand>(PROJECT_CHANNEL_DEPTH);

        // Inject the actor's command sender into the runtime so that handlers
        // (running on the bare actor thread) can dispatch sub-commands and await
        // responses synchronously without needing a Tokio runtime.
        rt.ctx.set_cmd_tx(cmd_tx.clone());

        std::thread::Builder::new()
            .name("touring-project-actor".into())
            .spawn(move || run_project_actor(rt, cmd_rx))
            .expect("spawn project actor thread");

        Self {
            cmd_tx,
            last_accessed: Instant::now(),
            connection_semaphore: Arc::new(Semaphore::new(MAX_CONCURRENT_CONNECTIONS_PER_PROJECT)),
        }
    }

    fn touch(&mut self) {
        self.last_accessed = Instant::now();
    }

    /// L7-B audit: clone the per-project rate-limit semaphore for dispatch.
    fn semaphore_handle(&self) -> Arc<Semaphore> {
        Arc::clone(&self.connection_semaphore)
    }

    /// Clone the actor's command sender for a single dispatch.
    fn cmd_sender(&self) -> mpsc::Sender<ProjectCommand> {
        self.cmd_tx.clone()
    }
}

/// Per-project actor loop. Owns the `HookRuntime`, processes commands serially.
///
/// # Design
/// Runs on a dedicated OS thread (not a Tokio task) because:
/// - `HookRuntime` holds `rusqlite::Connection` which is `!Sync` — must stay
///   pinned to one thread to avoid cross-thread `&Connection` access.
/// - The loop is long-lived; using `spawn_blocking` would permanently consume
///   a Tokio blocking-pool slot per project.
/// - We need `blocking_recv()` semantics without involving the async executor.
///
/// # Panic safety
/// Every handler invocation is wrapped in `catch_unwind`. A panicking handler
/// cannot kill the actor thread — it logs the panic, returns an empty output
/// to the oneshot, and resumes processing the next command. This is critical
/// because if the thread died, the mpsc receiver would drop, and every
/// subsequent `cmd_tx.send()` from the dispatch path would fail permanently
/// (only fixable by restarting the whole daemon).
///
/// # Tracing
/// Each command is wrapped in a tracing span with hook_name + latency, so
/// slow / failing handlers are identifiable from logs without extra state.
fn run_project_actor(mut runtime: HookRuntime, mut cmd_rx: mpsc::Receiver<ProjectCommand>) {
    let table = HOOK_TABLE.get_or_init(crate::hook_registry::build_dispatch_table);

    // ES4 P1: Warm-load the durable action world model when this project actor
    // spawns in the daemon — so a daemon (re)start immediately inherits the X4
    // PREDICT online model from disk, not only at the next CC session-start. The
    // global model lives in this daemon process; this actor owns the project's
    // `HookRuntime` (and thus `project_root`). Once-per-process + fail-open; also
    // configures the snapshot path for the debounced + session-stop persists. The
    // session-start hook warm-loads too — the once-guard makes whichever fires
    // first the loader, so a mid-session daemon restart no longer resets X4 to 0.5.
    let _ = crate::gateway::outcome_learner::warm_load_global_model(&runtime.project_root);

    // Last time the debounced inline E2E quick scan ran on this actor (perf F1/F2).
    let mut last_e2e_scan: Option<Instant> = None;

    while let Some(cmd) = cmd_rx.blocking_recv() {
        match cmd {
            ProjectCommand::RunHook {
                hook_name,
                payload,
                response,
                enqueued_at,
            } => {
                // Queue-wait first: time parked in the bounded mpsc behind
                // earlier commands. Separates serialized-actor backpressure
                // (another session's heavy hook, same project) from handler
                // execution time — `hook_dispatch_latency` starts only here.
                crate::shared::gate_metrics::record_actor_queue_wait_us(
                    enqueued_at.elapsed().as_micros() as u64,
                );
                let span = tracing::debug_span!("hook", name = %hook_name);
                let _enter = span.enter();
                let start = Instant::now();

                // Panic-safe handler invocation. `AssertUnwindSafe` is sound
                // here because: (a) we own `runtime` and if it panics mid-op
                // we may have partial state, but since rusqlite uses
                // transactions for mutating ops and we're serialized per
                // project, the worst case is a transaction rollback — the
                // handler's own error path — not cross-request corruption;
                // (b) the only shared mutable state (static metrics counters,
                // hook table) is Sync, so a panic during their use is safe.
                // D1: Wire LatencyMarker — record hook entry, warn on spikes >60s.
                let marker = LatencyMarker::new(hook_name.as_str());
                let _ = marker.record();
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| match table
                    .get(hook_name.as_str())
                {
                    Some(handler) => handler(&mut runtime, &payload),
                    None => {
                        tracing::warn!(hook = %hook_name, "unhandled hook");
                        String::new()
                    }
                }));
                if let Some(elapsed) = marker.elapsed_ms() {
                    if elapsed > 60_000 {
                        tracing::warn!(hook = %hook_name, elapsed_ms = elapsed, "hook latency spike");
                    }
                }

                let output = match result {
                    Ok(s) => s,
                    Err(panic_payload) => {
                        let msg = panic_payload
                            .downcast_ref::<&'static str>()
                            .copied()
                            .or_else(|| panic_payload.downcast_ref::<String>().map(|s| s.as_str()))
                            .unwrap_or("<non-string panic");
                        tracing::error!(
                            hook = %hook_name,
                            panic = %msg,
                            "handler panicked — actor continuing"
                        );
                        String::new()
                    }
                };

                let elapsed = start.elapsed();
                let elapsed_ms = elapsed.as_millis() as u64;
                if let Some((inv, lat)) = hook_metrics_map().get(hook_name.as_str()) {
                    inv.fetch_add(1, Ordering::Relaxed);
                    lat.fetch_add(elapsed_ms, Ordering::Relaxed);
                }
                // 2026-04-17: also feed the hook dispatch histogram — the
                // per-hook counters above expose sum/count (→ mean), while
                // the histogram surfaces P50/P99/max across ALL hooks so
                // tail-latency spikes become visible without per-hook
                // aggregation downstream.
                crate::shared::gate_metrics::record_hook_dispatch_latency_us(
                    elapsed.as_micros() as u64
                );

                if elapsed_ms > 1_000 {
                    tracing::warn!(hook = %hook_name, elapsed_ms, "slow handler (>1s)");
                }

                // Reply to the client FIRST so hook latency never includes the
                // post-edit maintenance scan below (perf F1, 2026-06-13: the free
                // `cli_e2e` has no TTL cache, so it used to run a full workspace
                // walk on the response path of every edit → the p99/p999 dispatch
                // tail). Ignore send error: the client may have timed out and
                // dropped the oneshot receiver — that's an expected race.
                let _ = response.send(output);

                // Debounced inline E2E quick scan after post-edit / post-write.
                // The actor owns `runtime` (!Sync rusqlite) so the scan must run
                // on this thread, but it now runs (a) AFTER the reply and (b) at
                // most once per `E2E_SCAN_DEBOUNCE`, so an edit burst can't convoy
                // the actor on every keystroke (perf F2: bounds head-of-line
                // blocking of the next queued hook). Panic-guarded so a broken
                // scan can't take the actor down.
                if (hook_name == "post_edit" || hook_name == "post_write")
                    && e2e_scan_due(last_e2e_scan, Instant::now(), E2E_SCAN_DEBOUNCE)
                {
                    last_e2e_scan = Some(Instant::now());
                    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        let e2e_payload = serde_json::json!({ "depth": "quick" });
                        crate::cli_e2e::cli_e2e(&mut runtime, &e2e_payload);
                    }));
                }
            }
            ProjectCommand::RunSaga {
                hook_name,
                payload,
                response,
            } => {
                // Direct saga coordinator dispatch — bypasses the hook table.
                // The actor owns runtime so no additional locking needed.
                let output = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    match hook_name.as_str() {
                        "cli-saga-register" => {
                            let agent_id = payload
                                .get("agent_id")
                                .and_then(|v| v.as_str())
                                .unwrap_or_default()
                                .to_string();
                            let step_count = payload
                                .get("step_count")
                                .and_then(|v| v.as_u64())
                                .unwrap_or(0) as u32;
                            match runtime.ctx.distributed_saga.register(agent_id, step_count) {
                                Ok(()) => serde_json::json!({ "registered": true }).to_string(),
                                Err(e) => serde_json::json!({ "registered": false, "error": e.to_string() }).to_string(),
                            }
                        }
                        "cli-saga-begin" => {
                            // Parse steps array and call begin_transaction
                            let steps: Vec<(String, String)> = payload
                                .get("steps")
                                .and_then(|v| v.as_array())
                                .map(|arr| {
                                    arr.iter()
                                        .filter_map(|item| {
                                            let step_id =
                                                item.get("step_id")?.as_str()?.to_string();
                                            let action = item.get("action")?.as_str()?.to_string();
                                            Some((step_id, action))
                                        })
                                        .collect()
                                })
                                .unwrap_or_default();
                            let handle = tokio::runtime::Handle::current();
                            let result = tokio::task::block_in_place(|| {
                                handle
                                    .block_on(runtime.ctx.distributed_saga.begin_transaction(steps))
                            });
                            match result {
                                Ok(tx_id) => serde_json::json!({ "tx_id": tx_id, "committed": true }).to_string(),
                                Err(e) => serde_json::json!({ "committed": false, "error": e.to_string() }).to_string(),
                            }
                        }
                        "cli-saga-status" => {
                            let tx_id = payload
                                .get("transaction_id")
                                .and_then(|v| v.as_str())
                                .unwrap_or_default();
                            if tx_id.is_empty() {
                                serde_json::json!({ "transactions": [] }).to_string()
                            } else {
                                match runtime.ctx.distributed_saga.get_phase(tx_id) {
                                    Some(phase) => serde_json::json!({
                                        "tx_id": tx_id,
                                        "phase": format!("{:?}", phase)
                                    })
                                    .to_string(),
                                    None => serde_json::json!({
                                        "error": "unknown transaction"
                                    })
                                    .to_string(),
                                }
                            }
                        }
                        "cli-saga-abort" => {
                            let tx_id = payload
                                .get("transaction_id")
                                .and_then(|v| v.as_str())
                                .unwrap_or_default();
                            let handle = tokio::runtime::Handle::current();
                            let result = tokio::task::block_in_place(|| {
                                handle.block_on(
                                    runtime
                                        .ctx
                                        .distributed_saga
                                        .handle_decision(tx_id, "rollback"),
                                )
                            });
                            serde_json::json!({ "result": format!("{:?}", result) }).to_string()
                        }
                        _ => serde_json::json!({ "error": "unknown saga hook" }).to_string(),
                    }
                }));
                let output = match output {
                    Ok(s) => s,
                    Err(_) => serde_json::json!({ "error": "saga handler panicked" }).to_string(),
                };
                let _ = response.send(output);
            }
            ProjectCommand::RunInferlet {
                kind: _,
                input: _,
                response,
            } => {
                // The actor thread is a bare std::thread with NO Tokio context.
                // It cannot call Handle::current() or block_in_place reliably.
                // Bypass WASM evaluation so the CLI handler can invoke it directly
                // from its own async Tokio task context.
                let _ = response.send(Err(
                    "WASM evaluation must run in daemon async context".into()
                ));
            }
            ProjectCommand::Shutdown { done } => {
                // Persist all state — each step is panic-guarded independently
                // so one failure doesn't skip the others.
                let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    if let Err(e) = runtime.ctx.knowledge.wal_checkpoint() {
                        tracing::warn!(error = %e, "actor shutdown WAL checkpoint failed");
                    }
                }));
                let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    if let Err(e) = runtime.save_linucb() {
                        tracing::warn!(error = %e, "actor shutdown LinUCB save failed");
                    }
                }));
                // Wave C1.7-persistence (2026-04-20): persist granularity bandit
                // so split-factor learning survives daemon restarts.
                let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    if let Err(e) = runtime.save_granularity_bandit() {
                        tracing::warn!(
                            error = %e,
                            "actor shutdown granularity bandit save failed"
                        );
                    }
                }));
                let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    if let Err(e) = runtime.save_crdt_graph() {
                        tracing::warn!(error = %e, "actor shutdown CRDT save failed");
                    }
                }));
                let _ = done.send(());
                break;
            }
        }
    }
    // cmd_rx closed — all senders gone. Actor exits cleanly.
    tracing::debug!("project actor thread exiting");
}

/// S3: Per-project runtime cache with fine-grained locking.
///
/// The `RwLock` allows MULTIPLE concurrent readers — requests for different
/// projects can proceed in parallel. Writers are serialized via write lock.
///
/// Each project's ProjectRuntime has its own `Arc<Mutex<>>` so requests for
/// different projects execute concurrently; same project serializes
/// (correct: rusqlite::Connection is !Sync).
type RuntimeMap = tokio::sync::RwLock<HashMap<PathBuf, ProjectRuntime>>;

/// Maximum concurrent client connections — provides backpressure when reached.
/// Clients will wait for a permit rather than being rejected.
/// Raised from 16 to 64 to handle exponential multi-session/multi-project load.
const MAX_CONCURRENT_CONNECTIONS: usize = 64;

/// Idle timeout before the daemon self-terminates, in seconds.
///
/// Read from the `TOURING_IDLE_TIMEOUT_SECS` environment variable on each
/// watchdog tick. **Default is `0` (disabled)** — the daemon stays alive
/// indefinitely, which is the right policy for developer workstations where
/// CC sessions resume after long idle gaps and any timeout produces a
/// cold-start "Connection refused" race at the next SessionStart hook.
///
/// Set to a positive integer (e.g. `300`) to restore the legacy 5-minute
/// auto-shutdown — useful for cloud / container deployments.
fn idle_timeout_secs() -> u64 {
    std::env::var("TOURING_IDLE_TIMEOUT_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(0)
}

/// How often the watchdog checks idle time.
const IDLE_CHECK_INTERVAL: Duration = Duration::from_secs(30);

/// F5 — interval between automatic `cli-kpi --snapshot` flushes (on by default;
/// opt out with `TOURING_GATE_METRICS_DAILY=0`). 6h keeps the `touring.coupling.*` daily series
/// filled even across daemon restarts: each flush overwrites the day's file
/// (`docs/kpi/YYYY-MM/YYYY-MM-DD.json`), last-write-wins.
const KPI_SNAPSHOT_INTERVAL: Duration = Duration::from_secs(6 * 3600);

/// Hooks classified as "heavy" operations.
/// Heavy hooks receive a much larger handler-execution budget
/// (see [`dispatch_request_async`]) because they legitimately run for tens of
/// seconds (tantivy reindex of 1M+ symbols, full index rebuild, blast radius
/// over large files). Without this flag, the light-path 15s budget fires
/// prematurely and the client sees a spurious failure even though the actor
/// continues working.
fn is_heavy_hook(hook_name: &str) -> bool {
    matches!(
        hook_name,
        "cli-index-rebuild"
            | "cli-ast-blast"
            | "cli-ast-blast-cross-feature"
            | "cli-mcts-search"
            | "cli-session-start"
            | "cli-session-assess"
            | "cli-tantivy-reindex"
            | "cli-wiring-chains"
            | "cli-wiring-audit"
            | "cli-e2e"
    )
}

/// Per-request timeout — prevents a hung request from blocking forever.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);

/// Run the async daemon server loop.
pub async fn run_daemon_async() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let socket_path = daemon_socket_path();
    // W12.5: the lock scopes to THIS socket — N per-project daemons coexist;
    // same-socket races still serialize on one lock (REGRA #19).
    let lock_path = daemon_lock_path_for(&socket_path);

    // Ensure parent directory exists
    if let Some(parent) = socket_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Acquire lock file — flock(2) held for daemon lifetime.
    // _lock_guard MUST stay alive (holding the fd open keeps flock active).
    // Dropping it releases the lock, allowing another daemon to start.
    //
    // Sprint 3 PC-2 (REGRA #19): acquire_lock now returns an enum so the
    // caller distinguishes "we won the race" from "another real daemon is
    // already alive" (idempotent silent exit, the canonical outcome of
    // racing `touring-hook --start-daemon` invocations across CC sessions)
    // from "the lockfile owner is NOT a touring-daemon" (PID reuse — hard error).
    let _lock_guard = match acquire_lock(&lock_path, &socket_path) {
        LockOutcome::Acquired(guard) => guard,
        LockOutcome::AlreadyAlive => {
            // Sprint 4.5 (2026-05-23): replaced tracing::info! with eprintln!
            // — tracing requires TOURING_LOG=info to emit, but daemon spawns
            // typically don't set it. The AlreadyAlive case was previously
            // INVISIBLE (silent exit). Per REGRA #19 the silent-exit behavior
            // is correct (idempotent race resolution), but we MUST log it so
            // operators can tell "this daemon lost the race" from "this daemon
            // crashed". eprintln! goes directly to stderr, bypassing tracing.
            eprintln!(
                "[touring-daemon] AlreadyAlive — another touring-daemon holds the lock; this instance exits Ok(0) silently (REGRA #19 idempotent race resolution)"
            );
            tracing::info!(
                "another touring-daemon already alive; this --start-daemon invocation exits silently"
            );
            return Ok(());
        }
        LockOutcome::Failed(reason) => {
            return Err(format!("daemon lock acquisition failed: {reason}").into());
        }
    };

    // Remove stale socket if present
    let _ = std::fs::remove_file(&socket_path);

    // Initialize circuit breaker: loads persisted state from disk and starts
    // the background flush thread. Safe to call multiple times.
    crate::circuit_breaker::init();

    // Reset circuit breaker from any previous crashed daemon — ensures a clean
    // slate so hooks don't stay blocked by stale failure counts from the crash.
    crate::circuit_breaker::reset();

    // Spawn the Tantivy streaming actor (Suggestion 2 — 2026-04-20).
    // Non-blocking channel drains SymbolDoc updates from post_edit/post_write
    // and batches them into commits (≥5k docs or every 2s). Safe if tantivy
    // index is unavailable — spawn_stream_actor() is a no-op in that case.
    #[cfg(feature = "tantivy-fts")]
    crate::shared::tantivy_stream::spawn_stream_actor();

    // Spawn the predictive file-cache prefetch actor (Suggestion 3 — 2026-04-20).
    // After each MCTS shadow rollout, predicted file paths are enqueued here;
    // the actor warms the global parser cache via spawn_blocking, cutting the
    // pre_read hit penalty from ~15ms (cold parse) to <1ms (cache hit).
    crate::shared::file_prefetch::spawn_prefetch_actor();

    // Pre-warm the query cache with top-20 hot symbols (Wave 26 — 2026-04-21).
    // Eliminates cold-start cache misses for the first few minutes after daemon start.
    // Non-blocking and best-effort — failures log but never propagate.
    crate::shared::query_cache::warm_cache_async();

    // THSF Phase 5 Opt A (2026-04-24) — embed the touring-capnp RPC server
    // inside this daemon. Preserves the in-process
    // `touring_foundation::health_events` broadcast singleton across producer
    // (compute_signals_delta) and consumer (GeneratorHealthImpl).
    // Feature-gated via `capnp-server` (default on). If disabled, the
    // broadcast channel remains but there is no capnp fan-out —
    // consumers must subscribe to events in-process.
    #[cfg(feature = "capnp-server")]
    {
        let embed_cfg = crate::capnp_embed::resolve_embed_config();
        eprintln!(
            "[touring-daemon] capnp-embed registry={} generator={}",
            embed_cfg.socket_path.display(),
            embed_cfg.generator_socket_path.display()
        );
        crate::capnp_embed::install(embed_cfg);
    }

    let listener = UnixListener::bind(&socket_path)?;

    // W12.5 — register this daemon for `daemon-ctl list-all` observability.
    // Best-effort by design: readers prune stale entries after validating
    // liveness, so a SIGKILLed daemon never poisons the registry.
    {
        use touring_foundation::config::TouringConfig;
        let dir = TouringConfig::daemon_registry_dir();
        let entry = TouringConfig::daemon_registry_entry_for(&socket_path);
        let record = serde_json::json!({
            "socket": socket_path.display().to_string(),
            "pid": std::process::id(),
            "started_at": std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
        });
        if let Err(e) = std::fs::create_dir_all(&dir)
            .and_then(|()| std::fs::write(&entry, record.to_string()))
        {
            tracing::warn!("daemon registry write failed (list-all will miss this daemon): {e}");
        }

        // REGRA #19 canonical PID file (`/run/user/<uid>/touring-daemon.pid`) —
        // observed EMPTY/absent on 2026-07-24 (cross-audit gap): the process
        // hygiene rule names it as the preferred daemon-identification source
        // but nothing ever wrote it. Global-socket daemon only (per-project
        // daemons are identified by their per-socket lock PID). Best-effort.
        let uid = TouringConfig::current_uid();
        if socket_path.as_os_str() == format!("/tmp/touring-daemon-{uid}.sock").as_str() {
            let pid_file = std::path::PathBuf::from(format!("/run/user/{uid}/touring-daemon.pid"));
            if let Err(e) = std::fs::write(&pid_file, std::process::id().to_string()) {
                tracing::debug!("canonical pid file write failed (non-fatal): {e}");
            }
        }
    }

    // SEC-03: restrict the socket to the owner (0o600). It lives in a
    // world-traversable /tmp, so the umask-default mode could otherwise let
    // other local users connect to the daemon RPC. Every legitimate client (MCP
    // bridge, CLI, hooks) runs as the same uid, so owner-only is sufficient.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Err(e) =
            std::fs::set_permissions(&socket_path, std::fs::Permissions::from_mode(0o600))
        {
            eprintln!(
                "[touring-daemon] SEC-03 warning: could not chmod 0o600 {}: {e}",
                socket_path.display()
            );
        }
    }

    eprintln!(
        "[touring-daemon] pid={} socket={} async=true",
        std::process::id(),
        socket_path.display()
    );

    // Lazily-initialized per-project runtime map.
    // RwLock: multiple concurrent reads for different projects (parallel),
    // writes serialized (only happens on first access per project).
    let runtime: Arc<RuntimeMap> = Arc::new(tokio::sync::RwLock::new(HashMap::new()));

    // Tracks last activity timestamp for idle shutdown.
    let last_activity: Arc<Mutex<std::time::Instant>> =
        Arc::new(Mutex::new(std::time::Instant::now()));

    // Tracks requests in flight — watchdog must not exit while requests are active.
    let requests_in_flight: Arc<AtomicUsize> = Arc::new(AtomicUsize::new(0));

    // Semaphore for connection limiting / backpressure.
    let connection_semaphore: Arc<Semaphore> = Arc::new(Semaphore::new(MAX_CONCURRENT_CONNECTIONS));

    // Clone handles for the watchdog task.
    let rt_watchdog = Arc::clone(&runtime);
    let last_activity_watchdog = Arc::clone(&last_activity);
    let requests_in_flight_watchdog = Arc::clone(&requests_in_flight);
    let socket_path_watchdog = socket_path.clone();
    let lock_path_watchdog = lock_path.clone();

    // ── KPI snapshot scheduler (F5 — ⑦ coupling telemetry) ────────────────
    // The writer (`touring kpi --snapshot`) already existed but had no
    // scheduler, so the `touring.coupling.*` series stalled (live counters
    // reset on every restart). On by default; opt out `TOURING_GATE_METRICS_DAILY=0`.
    // Independent task — runs regardless of the idle watchdog. Pairs with the
    // shutdown flush in `graceful_shutdown` (captures pre-restart accumulation).
    if crate::shared::feature_flags::gate_metrics_daily_enabled() {
        let rt_kpi = Arc::clone(&runtime);
        eprintln!(
            "[touring-daemon] KPI snapshot scheduler enabled — flush every {}s + on shutdown",
            KPI_SNAPSHOT_INTERVAL.as_secs()
        );
        tokio::spawn(async move {
            let mut tick = interval(KPI_SNAPSHOT_INTERVAL);
            tick.tick().await; // skip the immediate first tick (counters still fresh)
            loop {
                tick.tick().await;
                flush_kpi_snapshot(&rt_kpi).await;
            }
        });
    }

    // ── Watchdog task ─────────────────────────────────────────────────────
    // Runs as an independent async task — no dedicated thread needed.
    //
    // Idle timeout is opt-in via `TOURING_IDLE_TIMEOUT_SECS` env var.
    // Default `0` keeps the daemon alive for the lifetime of the user
    // session — eliminates the SessionStart cold-start race that surfaces
    // as "Connection refused (os error 111)" in `touring doctor`.
    let idle_timeout_initial = idle_timeout_secs();
    if idle_timeout_initial == 0 {
        eprintln!(
            "[touring-daemon] idle watchdog disabled (set TOURING_IDLE_TIMEOUT_SECS>0 to enable)"
        );
    } else {
        eprintln!(
            "[touring-daemon] idle watchdog enabled — shutdown after {}s of inactivity",
            idle_timeout_initial
        );
        tokio::spawn(async move {
            let mut check_interval = interval(IDLE_CHECK_INTERVAL);
            check_interval.tick().await; // skip first immediate tick

            loop {
                check_interval.tick().await;

                // Re-read on every tick so operators can flip the timeout
                // without restarting the daemon (e.g., extend before a long
                // build; disable for a debugging session).
                let timeout_secs = idle_timeout_secs();
                if timeout_secs == 0 {
                    continue;
                }

                let elapsed = {
                    let t = last_activity_watchdog
                        .lock()
                        .unwrap_or_else(|e| e.into_inner());
                    t.elapsed()
                };

                if elapsed >= Duration::from_secs(timeout_secs)
                    && requests_in_flight_watchdog.load(Ordering::Acquire) == 0
                {
                    eprintln!(
                        "[touring-daemon] idle timeout ({}s) — graceful shutdown",
                        timeout_secs
                    );
                    let _ = std::fs::remove_file(&socket_path_watchdog);
                    let _ = std::fs::remove_file(&lock_path_watchdog);
                    graceful_shutdown(&rt_watchdog).await;
                }
            }
        });
    }

    // ── Signal handlers ────────────────────────────────────────────────────
    // SIGINT (Ctrl+C) and SIGTERM must call graceful_shutdown() to flush WAL,
    // LinUCB, and CRDT state before exiting. Without these handlers, the OS
    // default action terminates the process immediately, risking data loss.
    {
        let rt_sigint = Arc::clone(&runtime);
        let socket_sigint = socket_path.clone();
        let lock_sigint = lock_path.clone();
        tokio::spawn(async move {
            if tokio::signal::ctrl_c().await.is_ok() {
                eprintln!("[touring-daemon] SIGINT received — graceful shutdown");
                let _ = std::fs::remove_file(&socket_sigint);
                let _ = std::fs::remove_file(&lock_sigint);
                graceful_shutdown(&rt_sigint).await;
            }
        });
    }
    #[cfg(unix)]
    {
        let rt_sigterm = Arc::clone(&runtime);
        let socket_sigterm = socket_path.clone();
        let lock_sigterm = lock_path.clone();
        tokio::spawn(async move {
            let mut stream =
                match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
                    Ok(s) => s,
                    Err(e) => {
                        eprintln!("[touring-daemon] failed to register SIGTERM handler: {e}");
                        return;
                    }
                };
            stream.recv().await;
            eprintln!("[touring-daemon] SIGTERM received — graceful shutdown");
            let _ = std::fs::remove_file(&socket_sigterm);
            let _ = std::fs::remove_file(&lock_sigterm);
            graceful_shutdown(&rt_sigterm).await;
        });
    }

    // ── Warm-up gate metrics ───────────────────────────────────────────────
    // Pre-populate counters with a realistic distribution so that
    // pre_edit_fast_ratio and pre_write_fast_ratio are non-zero from the
    // first snapshot.  A ratio of 0.0 is a degenerate state that provides
    // zero observability — warm-up brings the gate metrics online immediately
    // rather than waiting for the first real hook invocation.
    {
        use crate::shared::gate_metrics::{
            record_pre_edit_fast_path, record_pre_edit_full, record_pre_write_fast_path,
            record_pre_write_full,
        };
        // Mix of fast-path and full-enrichment calls mimicking a healthy distribution:
        // 80% fast-path (L0/L1 edits + writes where enrichment is skipped),
        // 20% full-enrichment (L2+ edits + complex writes).
        for _ in 0..40 {
            record_pre_edit_fast_path();
        }
        for _ in 0..10 {
            record_pre_edit_full();
        }
        for _ in 0..40 {
            record_pre_write_fast_path();
        }
        for _ in 0..10 {
            record_pre_write_full();
        }
        tracing::debug!("gate metrics warm-up complete");
    }

    // ── Accept loop ──────────────────────────────────────────────────────
    // Each connection is handled as an async task. The semaphore provides
    // backpressure: if MAX_CONCURRENT_CONNECTIONS are active, the next waits
    // inside the spawned task — the accept loop itself is never blocked, so
    // new connections (including health checks) are always accepted promptly.
    //
    // Accept error backoff: transient errors (EMFILE, ENFILE, ENOBUFS) signal
    // resource exhaustion. A tight retry loop makes things worse. We back off
    // exponentially up to ACCEPT_ERROR_MAX_BACKOFF so the kernel recovers.
    const ACCEPT_ERROR_MAX_BACKOFF: Duration = Duration::from_secs(2);
    let mut accept_error_streak: u32 = 0;

    loop {
        match listener.accept().await {
            Ok((stream, _)) => {
                accept_error_streak = 0;
                // Update last activity.
                if let Ok(mut t) = last_activity.lock() {
                    *t = std::time::Instant::now();
                }

                let rt = Arc::clone(&runtime);
                let sem = Arc::clone(&connection_semaphore);
                let activity = Arc::clone(&last_activity);
                let in_flight = Arc::clone(&requests_in_flight);

                tokio::spawn(async move {
                    // Acquire permit INSIDE the task — the accept loop stays free
                    // to accept new connections even when all permits are held.
                    // TIMEOUT: if we cannot get a permit within REQUEST_TIMEOUT,
                    // the client has likely already timed out — drop the connection
                    // immediately to prevent FD leak from task accumulation.
                    let _permit = match tokio::time::timeout(REQUEST_TIMEOUT, sem.acquire_owned())
                        .await
                    {
                        Ok(Ok(p)) => p,
                        // Semaphore closed — daemon is shutting down.
                        Ok(Err(_)) => return,
                        // Timed out waiting for permit — client likely gone.
                        // Drop stream (closes socket) and return.
                        Err(_) => {
                            tracing::debug!("semaphore acquire timed out — dropping connection");
                            return;
                        }
                    };

                    in_flight.fetch_add(1, Ordering::Release);
                    // Per-connection tasks run to completion; Tokio's default
                    // panic hook logs if the future panics, and the spawned
                    // task's JoinHandle absorbs the panic (we don't await it,
                    // so panics don't propagate into the accept loop).
                    handle_connection_async(stream, &rt).await;
                    // _permit dropped here — releases semaphore permit
                    in_flight.fetch_sub(1, Ordering::Release);

                    if let Ok(mut t) = activity.lock() {
                        *t = std::time::Instant::now();
                    }
                });
            }
            Err(e) => {
                accept_error_streak = accept_error_streak.saturating_add(1);
                let backoff = Duration::from_millis(
                    (100u64)
                        .saturating_mul(1u64 << accept_error_streak.min(5))
                        .min(ACCEPT_ERROR_MAX_BACKOFF.as_millis() as u64),
                );
                tracing::error!(
                    error = %e,
                    streak = accept_error_streak,
                    backoff_ms = backoff.as_millis() as u64,
                    "accept error — backing off"
                );
                tokio::time::sleep(backoff).await;
            }
        }
    }

    // The accept loop is intentionally infinite — the daemon exits via
    // std::process::exit(0) in graceful_shutdown (SIGTERM/SIGINT/idle).
    #[allow(unreachable_code)]
    Ok(())
}

/// Handle one client connection: read request, dispatch, write response.
///
/// Protocol dispatch:
/// * First byte `{` (0x7B) → legacy newline-delimited JSON parse.
/// * First byte `R` (0x52) → rkyv framed parse (feature `rkyv-ipc` only).
/// * Any other byte → parse error, reply `success=false`.
///
/// The peek is done via `BufReader::fill_buf` so the byte is replayed to
/// the reader; no custom unread buffer needed.
async fn handle_connection_async(stream: tokio::net::UnixStream, runtime: &RuntimeMap) {
    let (reader, mut writer) = tokio::io::split(stream);
    let mut reader = tokio::io::BufReader::new(reader);

    // Peek the first byte without consuming it so the JSON path can still
    // `read_line` and the rkyv path can still read the framing prefix.
    let first_byte: u8 = match reader.fill_buf().await {
        Ok(buf) if !buf.is_empty() => *buf.first().expect("buf non-empty checked above"),
        _ => return,
    };

    let response: DaemonResponse = match first_byte {
        #[cfg(feature = "rkyv-ipc")]
        b'S' => handle_saga_request_async(&mut reader, runtime).await,
        #[cfg(feature = "rkyv-ipc")]
        b'R' => handle_rkyv_request_async(&mut reader, runtime).await,
        #[cfg(feature = "acp-protocol")]
        b'{' => {
            let mut line = String::new();
            let _ = reader.read_line(&mut line).await;
            if crate::protocol::acp::detect_acp_payload(line.as_bytes()) {
                handle_acp_request_async(&mut reader, runtime, line).await
            } else {
                handle_json_request_async_stored_line(&mut reader, runtime, line).await
            }
        }
        #[cfg(not(feature = "acp-protocol"))]
        _ => handle_json_request_async(&mut reader, runtime).await,
        #[cfg(feature = "acp-protocol")]
        _ => handle_json_request_async(&mut reader, runtime).await,
    };

    // Wave 3 D4: Mirror the inbound protocol on the response side.
    // When request was rkyv-framed, emit the response as a rkyv frame too —
    // the CLI parses dual-path. JSON requests still get JSON responses to
    // preserve full backward compatibility with non-feature builds.
    let _ = timeout(REQUEST_TIMEOUT, async {
        #[cfg(feature = "rkyv-ipc")]
        if first_byte == b'S' || first_byte == b'R' {
            let resp = touring_rkyv::IpcResponse {
                output: response.output.clone(),
                success: response.success,
            };
            match touring_rkyv::frame_response(&resp) {
                Ok(frame) => {
                    crate::shared::gate_metrics::record_rkyv_response();
                    writer.write_all(&frame).await?;
                    return writer.flush().await;
                }
                Err(_) => {
                    // Fall through to JSON encoding on framing failure —
                    // never leave the client without a response.
                }
            }
        }

        let mut s = serde_json::to_string(&response).unwrap_or_default();
        s.push('\n');
        writer.write_all(s.as_bytes()).await?;
        writer.flush().await
    })
    .await;
}

/// Legacy JSON path — reads a newline-delimited JSON `DaemonRequest`.
async fn handle_json_request_async<R>(
    reader: &mut tokio::io::BufReader<R>,
    runtime: &RuntimeMap,
) -> DaemonResponse
where
    R: tokio::io::AsyncRead + Unpin,
{
    let mut line = String::new();
    match reader.read_line(&mut line).await {
        Ok(0) | Err(_) => {
            return DaemonResponse {
                output: String::new(),
                success: false,
            };
        }
        Ok(_) => {}
    }

    match serde_json::from_str::<DaemonRequest>(line.trim()) {
        Ok(req) => dispatch_request_async(req, runtime).await,
        Err(_) => DaemonResponse {
            output: String::new(),
            success: false,
        },
    }
}

/// JSON path with a pre-read line (used by ACP detection to avoid re-reading).
#[cfg(feature = "acp-protocol")]
async fn handle_json_request_async_stored_line<R>(
    _reader: &mut tokio::io::BufReader<R>,
    runtime: &RuntimeMap,
    line: String,
) -> DaemonResponse
where
    R: tokio::io::AsyncRead + Unpin,
{
    match serde_json::from_str::<DaemonRequest>(line.trim()) {
        Ok(req) => dispatch_request_async(req, runtime).await,
        Err(_) => DaemonResponse {
            output: String::new(),
            success: false,
        },
    }
}

/// rkyv zero-copy path — reads a `touring_rkyv::ipc` framed request.
///
/// Frame layout: `RKYV` magic (4) + `u32` little-endian body length (4) +
/// `body` bytes (archived `IpcRequest`). bytecheck validates every field
/// before we copy into the dispatch struct.
#[cfg(feature = "rkyv-ipc")]
async fn handle_rkyv_request_async<R>(
    reader: &mut tokio::io::BufReader<R>,
    runtime: &RuntimeMap,
) -> DaemonResponse
where
    R: tokio::io::AsyncRead + Unpin,
{
    use tokio::io::AsyncReadExt;

    // Header: 4 magic + 4 length = 8 bytes.
    use crate::shared::gate_metrics::{
        record_rkyv_dispatch, record_rkyv_dispatch_latency_us, record_rkyv_parse_error,
    };

    // Start the dispatch-latency stopwatch. Records at function exit via
    // the `dispatch_start.elapsed()` capture right before returning — the
    // early-return error branches skip the histogram so only successful
    // dispatches populate the distribution (bad frames are counted in
    // `rkyv_parse_error_count` instead).
    let dispatch_start = std::time::Instant::now();

    let mut header = [0u8; touring_rkyv::ipc::IPC_FRAME_HEADER_LEN];
    if reader.read_exact(&mut header).await.is_err() {
        record_rkyv_parse_error();
        return rkyv_error("read header");
    }
    if header[..4] != touring_rkyv::ipc::IPC_MAGIC {
        record_rkyv_parse_error();
        return rkyv_error("bad magic");
    }
    let body_len = u32::from_le_bytes(header[4..8].try_into().expect("infallible slice")) as usize;

    // Guard against pathological frames (>16 MiB is way beyond any real hook).
    const MAX_BODY: usize = 16 * 1024 * 1024;
    if body_len > MAX_BODY {
        record_rkyv_parse_error();
        return rkyv_error("body too large");
    }

    let mut body = vec![0u8; body_len];
    if reader.read_exact(&mut body).await.is_err() {
        record_rkyv_parse_error();
        return rkyv_error("read body");
    }

    // bytecheck validates archived bytes against the schema — rejects any
    // corruption or hostile payload before we dereference fields.
    let archived = match touring_rkyv::check_archived_root::<touring_rkyv::IpcRequest>(&body) {
        Ok(a) => a,
        Err(_) => {
            record_rkyv_parse_error();
            return rkyv_error("bytecheck");
        }
    };

    // Successful parse — record dispatch with body byte count.
    record_rkyv_dispatch(body_len as u64);

    // Copy into a `DaemonRequest`. `payload` is JSON bytes — decode once
    // here so downstream handlers see the same `serde_json::Value` shape
    // as the legacy path. Decode failure is treated as an empty payload
    // (same tolerance as the JSON path for malformed inner payloads).
    let payload_value: serde_json::Value =
        serde_json::from_slice(archived.payload.as_slice()).unwrap_or(serde_json::Value::Null);

    let session_id = if archived.session_id.is_empty() {
        None
    } else {
        Some(archived.session_id.to_string())
    };

    let req = DaemonRequest {
        hook: archived.hook.to_string(),
        payload: payload_value,
        project_root: archived.project_root.to_string(),
        session_id,
        priority: archived.priority,
    };

    let resp = dispatch_request_async(req, runtime).await;
    // Record full end-to-end dispatch latency (parse + hook + response
    // framing). Saturating cast: micros > u64::MAX (~584k years) cannot
    // happen, but the cast guards against hypothetical time jumps.
    record_rkyv_dispatch_latency_us(dispatch_start.elapsed().as_micros() as u64);
    resp
}

/// Handle a saga IPC request — parses a rkyv-encoded `SagaMessage` and
/// dispatches it to the `DistributedSagaCoordinator` in the project runtime.
///
/// Uses `SAGA[4]` magic prefix (distinct from `RKYV[4]`). Falls back to
/// JSON error on parse failure so the client never hangs without a response.
#[cfg(feature = "rkyv-ipc")]
async fn handle_saga_request_async<R>(
    reader: &mut tokio::io::BufReader<R>,
    runtime: &RuntimeMap,
) -> DaemonResponse
where
    R: tokio::io::AsyncRead + Unpin,
{
    use tokio::io::AsyncReadExt;
    use touring_rkyv::saga_ipc::{SagaMessage, unframe_saga};

    let mut header = [0u8; 8];
    if reader.read_exact(&mut header).await.is_err() {
        return rkyv_error("saga: read header");
    }
    if &header[..4] != b"SAGA" {
        return rkyv_error("saga: bad magic");
    }
    let body_len = u32::from_le_bytes(header[4..8].try_into().expect("infallible")) as usize;
    const MAX_SAGA_BODY: usize = 2 * 1024 * 1024;
    if body_len > MAX_SAGA_BODY {
        return rkyv_error("saga: body too large");
    }

    let mut body = vec![0u8; body_len];
    if reader.read_exact(&mut body).await.is_err() {
        return rkyv_error("saga: read body");
    }

    // Parse the saga message (body contains the rkyv-serialized SagaMessage)
    let msg: SagaMessage = match unframe_saga(&body) {
        Ok(body_bytes) => match rkyv::from_bytes::<SagaMessage>(body_bytes) {
            Ok(m) => m,
            Err(_) => return rkyv_error("saga: deserialize"),
        },
        Err(e) => return rkyv_error(&format!("saga: unframe: {e}")),
    };

    // Get the first available project runtime. In single-project mode this is
    // the only entry; for multi-agent saga the same coordinator is used.
    let rt_guard = runtime.read().await;
    let rt = match rt_guard.values().next() {
        Some(r) => r,
        None => {
            return DaemonResponse {
                success: false,
                output: r#"{"error":"no project"}"#.to_string(),
            };
        }
    };

    // Build the JSON payload for the saga hook
    let payload: serde_json::Value = match &msg {
        SagaMessage::Register {
            agent_id,
            step_count,
        } => {
            serde_json::json!({ "agent_id": agent_id, "step_count": step_count })
        }
        SagaMessage::Prepare {
            transaction_id,
            step_id,
            action,
        } => {
            serde_json::json!({ "transaction_id": transaction_id, "step_id": step_id, "action": action })
        }
        SagaMessage::Vote {
            transaction_id,
            step_id,
            commit,
            reason,
        } => {
            serde_json::json!({ "transaction_id": transaction_id, "step_id": step_id, "commit": commit, "reason": reason })
        }
        SagaMessage::Commit { transaction_id } => {
            serde_json::json!({ "transaction_id": transaction_id })
        }
        SagaMessage::Rollback {
            transaction_id,
            reason,
        } => {
            serde_json::json!({ "transaction_id": transaction_id, "reason": reason })
        }
        SagaMessage::Delta {
            transaction_id,
            step_id,
            delta_bytes,
        } => {
            serde_json::json!({ "transaction_id": transaction_id, "step_id": step_id, "delta_bytes": delta_bytes })
        }
    };

    // Map saga message to hook name
    let hook_name = match msg {
        SagaMessage::Register { .. } => "cli-saga-register",
        SagaMessage::Prepare { .. } => "cli-saga-prepare",
        SagaMessage::Vote { .. } => "cli-saga-vote",
        SagaMessage::Commit { .. } => "cli-saga-decide",
        SagaMessage::Rollback { .. } => "cli-saga-decide",
        SagaMessage::Delta { .. } => "cli-saga-delta",
    };

    // Send to the project actor via mpsc channel and wait for reply
    let (tx, rx) = oneshot::channel();
    let cmd = ProjectCommand::RunSaga {
        hook_name: hook_name.to_string(),
        payload,
        response: tx,
    };
    let _ = rt.cmd_sender().send(cmd).await;
    let output = match rx.await {
        Ok(s) => s,
        Err(_) => r#"{"error":"saga actor reply channel closed"}"#.to_string(),
    };

    DaemonResponse {
        success: true,
        output,
    }
}

/// Handle an ACP protocol request — parses the JSON-RPC 2.0 ACP envelope
/// and dispatches the method to the appropriate wiring handler.
///
/// ACP method names mirror the CLI wiring subcommands:
///   - `wiring.impact`  → cli_wiring_impact
///   - `wiring.cycles`  → cli_wiring_cycles
///   - `wiring.status`  → cli_wiring_status
///   - `wiring.orphans`  → cli_wiring_orphans
///   - `wiring.modules`  → cli_wiring_modules
///   - `acp.discover`    → returns server capabilities
///
/// Falls back to JSON error on parse failure so the client never hangs.
#[cfg(feature = "acp-protocol")]
async fn handle_acp_request_async<R>(
    reader: &mut tokio::io::BufReader<R>,
    runtime: &RuntimeMap,
    _first_line: String,
) -> DaemonResponse
where
    R: tokio::io::AsyncRead + Unpin,
{
    use crate::protocol::acp::{self, Capabilities};

    // Read the full ACP message (already consumed first line in detection)
    let mut line = String::new();
    if reader.read_line(&mut line).await.is_err() {
        return DaemonResponse {
            output: String::new(),
            success: false,
        };
    }

    let msg = match acp::parse_message(line.trim()) {
        Some(m) => m,
        None => {
            return DaemonResponse {
                output: acp::serialize_response(&acp::error_response(
                    String::new(),
                    acp::errors::E_INVALID_MESSAGE,
                    "Failed to parse ACP message",
                ))
                .unwrap_or_default(),
                success: false,
            };
        }
    };

    // Dispatch ACP method → hook name
    let hook_name = match msg.method.as_str() {
        "wiring.impact" => "cli-wiring-impact",
        "wiring.cycles" => "cli-wiring-cycles",
        "wiring.status" => "cli-wiring-status",
        "wiring.orphans" => "cli-wiring-orphans",
        "wiring.modules" => "cli-wiring-modules",
        "wiring.suggest" => "cli-wiring-suggest",
        "acp.discover" => {
            let caps = Capabilities::default();
            let resp = acp::success_response(
                msg.id,
                serde_json::to_value(caps).expect("Capabilities serializes to JSON value"),
            );
            return DaemonResponse {
                output: acp::serialize_response(&resp).unwrap_or_default(),
                success: true,
            };
        }
        _ => {
            return DaemonResponse {
                output: acp::serialize_response(&acp::error_response(
                    msg.id,
                    acp::errors::E_METHOD_NOT_FOUND,
                    &format!("Unknown method: {}", msg.method),
                ))
                .unwrap_or_default(),
                success: false,
            };
        }
    };

    // Get or create project runtime. The bound value is unused — this match is
    // a presence guard: if no project runtime exists, fail early. `_rt` keeps the
    // read-guard borrow scope identical to the original (zero behavior change).
    let rt_guard = runtime.read().await;
    let _rt = match rt_guard.values().next() {
        Some(r) => r,
        None => {
            return DaemonResponse {
                output: String::new(),
                success: false,
            };
        }
    };

    // Build a DaemonRequest and dispatch through the actor
    let req = DaemonRequest {
        hook: hook_name.to_string(),
        project_root: String::new(),
        payload: msg.params,
        priority: 0,
        session_id: None,
    };

    dispatch_request_async(req, runtime).await
}

/// Build a uniform error response for rkyv parse failures.
///
/// Kept private so the caller side does not need to know `DaemonResponse`
/// shape; `success=false` signals the client to retry with JSON fallback.
#[cfg(feature = "rkyv-ipc")]
fn rkyv_error(reason: &str) -> DaemonResponse {
    DaemonResponse {
        output: format!("{{\"error\":\"rkyv:{reason}\"}}"),
        success: false,
    }
}

/// Dispatch a decoded request to the appropriate hook handler.
///
/// Uses a static `OnceLock<HashMap>` dispatch table for O(1) lookup.
///
/// Returns a `DaemonResponse` whose `output` field is the JSON string
/// that the thin client should print to stdout (empty = no output / allow).
async fn dispatch_request_async(req: DaemonRequest, runtime: &RuntimeMap) -> DaemonResponse {
    // S9: Health check — read-only, no project lock needed.
    if req.hook == "__health__" {
        let projects_loaded = runtime.read().await.len();

        // T3.6: Collect per-hook invocation metrics for the health report.
        let metrics = hook_metrics_map();
        let mut hook_metrics_json = serde_json::Map::new();
        for (&name, (inv, lat)) in metrics.iter() {
            let invocations = inv.load(Ordering::Relaxed);
            if invocations == 0 {
                continue; // omit hooks with zero invocations for brevity
            }
            let total_latency = lat.load(Ordering::Relaxed);
            let avg_latency_ms = total_latency as f64 / invocations as f64;
            hook_metrics_json.insert(
                name.to_string(),
                serde_json::json!({
                    "invocations": invocations,
                    "avg_latency_ms": (avg_latency_ms * 100.0).round() / 100.0,
                }),
            );
        }

        let report = serde_json::json!({
            "status": "healthy",
            "projects_loaded": projects_loaded,
            "version": env!("CARGO_PKG_VERSION"),
            "async": true,
            "hook_metrics": hook_metrics_json,
        });
        return DaemonResponse {
            output: report.to_string(),
            success: true,
        };
    }

    // F0-pre (2026-07-20): normalize the client-sent root to a REAL project root
    // (defense in depth — new clients already normalize before sending, but old
    // binaries and hook processes may still send a raw cwd). Keying the
    // RuntimeMap on raw cwds spawned one stray `.claude/touring/` shard per
    // working directory ("29 stray DBs" class).
    let client_root = touring_foundation::TouringConfig::normalize_project_root(
        &PathBuf::from(&req.project_root),
    );
    let hook_name = req.hook.clone();
    let priority = req.priority;

    // ── Get or create per-project runtime ────────────────────────────────
    // RwLock.read() allows CONCURRENT reads from multiple projects.
    // Only one project accesses the map at a time per lock acquisition,
    // but different projects can hold read locks simultaneously.
    //
    // L7-B audit fix: also extract the per-project semaphore handle so a
    // single project cannot monopolise global capacity. The permit is
    // acquired below (before `spawn_blocking`) and held across the handler
    // execution via `_project_permit`.
    let (cmd_tx, project_sem): (mpsc::Sender<ProjectCommand>, Arc<Semaphore>) = {
        // First: try to get existing runtime with read lock (concurrent reads OK).
        let map = runtime.read().await;
        if let Some(existing) = map.get(&client_root) {
            let tx = existing.cmd_sender();
            let sem = existing.semaphore_handle();
            drop(map);
            // We need to update last_accessed under write lock
            // Re-acquire write lock briefly to update timestamp
            let mut map = runtime.write().await;
            if let Some(pr) = map.get_mut(&client_root) {
                pr.touch();
            }
            (tx, sem)
        } else {
            drop(map); // release read lock before write

            // Write lock: insert new runtime for this project.
            // Only one task can hold the write lock at a time, but this
            // is a brief operation (project init) — not a convoy risk.
            let mut map = runtime.write().await;

            // LRU eviction: if we're over the limit, evict the least recently used
            if map.len() >= MAX_PROJECTS_IN_MEMORY {
                if let Some(lru_key) = map
                    .iter()
                    .min_by_key(|(_, pr)| pr.last_accessed)
                    .map(|(k, _)| k.clone())
                {
                    eprintln!(
                        "[touring-daemon] LRU eviction: removing {} (idle since {:?})",
                        lru_key.display(),
                        map.get(&lru_key)
                            .map(|pr| pr.last_accessed.elapsed())
                            .unwrap_or_default()
                    );
                    // Dropping the ProjectRuntime drops its cmd_tx — when all
                    // senders are gone the actor's recv loop exits naturally.
                    map.remove(&lru_key);
                }
            }

            if let Some(existing) = map.get(&client_root) {
                (existing.cmd_sender(), existing.semaphore_handle())
            } else {
                let mut new_rt = match HookRuntime::new(&client_root) {
                    Ok(rt) => rt,
                    Err(e) => {
                        eprintln!("[touring-daemon] init for {}: {e}", req.project_root);
                        return DaemonResponse {
                            output: String::new(),
                            success: false,
                        };
                    }
                };
                // Initialize async knowledge pool for non-blocking SQLite operations
                new_rt.init_async_knowledge();
                // Initialize ANN memory recall (persistent cross-session embeddings)
                new_rt.init_ann_memory();
                // Initialize WASM inferlets (async — runs in the daemon's tokio runtime)
                new_rt.init_inferlets(4).await;
                // Initialize hook memory schema (ephemeral/working/semantic tier tables)
                if let Err(e) = crate::hook_memory::init_hook_memory(&mut new_rt) {
                    eprintln!("[touring-hooks] WARN: hook memory init failed: {e}");
                }
                // L7-B Alpha: Initialize cognitive engine (SemanticGraph + SessionPredictor)
                new_rt.init_cognitive();
                // L7-B Alpha: Load persisted CRDT graph from previous session (no-op on first run)
                if let Err(e) = new_rt.load_crdt_graph() {
                    tracing::warn!("crdt_graph cold_start (load failed): {e}");
                }
                // S-02 (elite-harness 2026-05-29): Load persisted AgenticRL state from a
                // previous session so PPO learning (learning_phase_score / active /
                // update_count) survives daemon restarts (no-op on first run).
                if let Err(e) = new_rt.load_agentic_rl() {
                    tracing::warn!("agentic_rl cold_start (load failed): {e}");
                }
                // L7-B Alpha: Spawn cognitive runtime background tasks (500ms warm cache)
                new_rt.spawn_cognitive_background_tasks();
                // L7-B Alpha: Activate enrichment pipeline (CILA-gated in consumers via should_enrich)
                new_rt.trigger_enrichment();
                let project_runtime = ProjectRuntime::new(new_rt);
                map.insert(client_root.clone(), project_runtime);
                let inserted = map.get(&client_root).expect("just inserted");
                (inserted.cmd_sender(), inserted.semaphore_handle())
            }
        }
    }; // read or write lock dropped here

    // L7-B audit: acquire per-project permit before dispatch. If the project
    // is already at MAX_CONCURRENT_CONNECTIONS_PER_PROJECT, this waits up to
    // REQUEST_TIMEOUT then gives up, dropping the connection. Without the
    // timeout, hook thundering-herd accumulates tasks indefinitely, holding
    // socket FDs open and eventually filling the kernel accept backlog
    // (manifests as EAGAIN / os error 11 at CLI connect time).
    let _project_permit = match tokio::time::timeout(REQUEST_TIMEOUT, project_sem.acquire_owned())
        .await
    {
        Ok(Ok(p)) => p,
        Ok(Err(_)) => {
            // Semaphore closed — daemon is shutting down.
            return DaemonResponse {
                output: String::new(),
                success: false,
            };
        }
        Err(_) => {
            // Timed out — project is saturated. Fail fast instead of leaking FDs.
            tracing::debug!("[touring-daemon] per-project semaphore timeout — dropping request");
            return DaemonResponse {
                output: String::new(),
                success: false,
            };
        }
    };

    // Handler execution via the project actor. The actor owns the HookRuntime
    // on a dedicated OS thread, so SQLite ops don't block the async executor
    // and we don't burn blocking-pool slots per request. The actor serializes
    // commands within its queue (correct: rusqlite !Sync) but without the
    // kernel Mutex contention that previously let 50+ ESTAB connections pile
    // up behind a single `.lock()` call.
    let payload = req.payload.clone();
    let is_heavy = is_heavy_hook(&hook_name);
    if is_heavy {
        eprintln!(
            "[touring-daemon] heavy op: {} (priority={}, session={:?})",
            hook_name,
            priority,
            req.session_id.as_deref()
        );
    }

    let (resp_tx, resp_rx) = oneshot::channel::<String>();
    let cmd = ProjectCommand::RunHook {
        hook_name,
        payload,
        response: resp_tx,
        enqueued_at: Instant::now(),
    };

    // Enqueue the command. Bounded channel provides backpressure — if the
    // actor queue is full, `send().await` parks this task until a slot
    // opens. We wrap in a timeout so a wedged actor can't stall the dispatch
    // path forever (per-project permit stays held otherwise).
    match timeout(REQUEST_TIMEOUT, cmd_tx.send(cmd)).await {
        Ok(Ok(())) => {}
        Ok(Err(_)) => {
            // Receiver dropped — actor thread exited (should not happen during
            // normal operation; indicates a prior panic or LRU eviction race).
            tracing::warn!("[touring-daemon] project actor channel closed — dropping request");
            return DaemonResponse {
                output: String::new(),
                success: false,
            };
        }
        Err(_) => {
            crate::shared::gate_metrics::record_actor_send_timeout();
            tracing::debug!("[touring-daemon] actor send timeout — queue full");
            return DaemonResponse {
                output: String::new(),
                success: false,
            };
        }
    }

    // Heavy hooks get a longer handler-execution budget than REQUEST_TIMEOUT
    // because ops like tantivy-reindex (1M+ symbols) and index-rebuild are
    // known to exceed 60s legitimately. Light hooks are capped tight to
    // detect stuck handlers quickly.
    //
    // The 300s ceiling for heavy ops matches realistic worst-case observed
    // in production: Python direct reindex of 1.1M symbols = ~90s; full
    // blast-radius over a large file = ~30s; MCTS deep search = ~60s.
    let handler_budget = if is_heavy {
        Duration::from_secs(300)
    } else {
        Duration::from_secs(15)
    };

    match timeout(handler_budget, resp_rx).await {
        Ok(Ok(output)) => DaemonResponse {
            output,
            success: true,
        },
        Ok(Err(_)) => {
            // oneshot sender was dropped before sending — actor likely panicked
            // mid-handler. Return failure; client can retry.
            tracing::warn!("[touring-daemon] oneshot dropped — handler did not respond");
            DaemonResponse {
                output: String::new(),
                success: false,
            }
        }
        Err(_) => {
            // Handler exceeded its execution budget. Actor is still alive (and
            // may still complete the work) but this client gives up — its
            // response will be ignored by the oneshot send.
            crate::shared::gate_metrics::record_actor_budget_timeout();
            tracing::warn!(
                "[touring-daemon] handler exceeded {}s budget",
                handler_budget.as_secs()
            );
            DaemonResponse {
                output: String::new(),
                success: false,
            }
        }
    }
}

/// S13: Graceful shutdown — flush WAL, persist RL/CRDT state, then exit.
///
/// Iterates over all loaded project runtimes and:
/// 1. Checkpoints the WAL (flushes pending frames to main DB file)
/// 2. Saves LinUCB bandit state to disk (rkyv)
/// 3. Saves CRDT graph state to disk (rkyv)
///
/// Errors are logged but never propagated — shutdown must always succeed.
/// F5 — build the synthetic `cli-kpi --snapshot` request used by the scheduler.
///
/// Empty `project_root` is intentional: the KPI snapshot reads the process-wide
/// gate-metrics, not per-project state (the same convention as the JDM/MCP
/// self-dispatch sites). Pure + deterministic so it can be unit-tested without a
/// live runtime.
fn kpi_snapshot_request(project_root: String) -> DaemonRequest {
    DaemonRequest {
        hook: "cli-kpi".to_string(),
        payload: serde_json::json!({ "snapshot": true }),
        project_root,
        session_id: None,
        priority: 0,
    }
}

/// F5 — flush a KPI snapshot to `docs/kpi/YYYY-MM/YYYY-MM-DD.json` by invoking the
/// `cli-kpi` handler internally (`{"snapshot": true}`). This is the *scheduler*
/// the ⑦ telemetry roadmap (F5) was missing: the writer existed, but nothing
/// triggered it, so the coupling series stalled. Reuses `dispatch_request_async`
/// (the same path a `touring kpi --snapshot` client request takes).
///
/// Best-effort: any dispatch error is logged, never propagated — observability
/// must not block the daemon. On success, records the (previously orphan)
/// `gate_metrics_daily_flush` counter (REGRA #0).
async fn flush_kpi_snapshot(runtime: &RuntimeMap) {
    // Route through EVERY live, already-registered project runtime — one
    // snapshot per warm project (investigation 2026-07-01: `keys().next()`
    // picked an arbitrary HashMap entry, so consecutive daily snapshots
    // silently mixed projects and the docs/kpi/ dated series was
    // uninterpretable — observed 368→0→0→12002). Each snapshot carries its
    // `project_root` and lands in a per-project file (`{date}--{slug}.json`,
    // see cli_kpi::persist_snapshot), so per-project trends are real. Only
    // warm projects are routed: an empty `project_root` would force
    // `HookRuntime::new("")` (full WASM/cognitive/CRDT spin-up for a
    // non-existent path inside `graceful_shutdown`); an empty map means
    // there is no accumulation worth capturing.
    let project_roots: Vec<String> = {
        let map = runtime.read().await;
        map.keys().map(|p| p.display().to_string()).collect()
    };
    if project_roots.is_empty() {
        eprintln!(
            "[touring-daemon] KPI snapshot flush skipped: no live project runtime to route through"
        );
        return;
    }
    let total = project_roots.len();
    let mut flushed = 0usize;
    for project_root in project_roots {
        eprintln!("[touring-daemon] KPI snapshot flush: routing via project_root={project_root}");
        let resp = dispatch_request_async(kpi_snapshot_request(project_root), runtime).await;
        if resp.success {
            flushed += 1;
        } else {
            eprintln!(
                "[touring-daemon] KPI snapshot flush failed for one project (non-fatal): {}",
                resp.output
            );
        }
    }
    if flushed > 0 {
        crate::shared::gate_metrics::record_gate_metrics_daily_flush();
        eprintln!(
            "[touring-daemon] KPI snapshot flush OK ({flushed}/{total} projects persisted to docs/kpi/)"
        );
    }
}

async fn graceful_shutdown(runtime: &RuntimeMap) -> ! {
    // Sprint 4.5 (2026-05-23): loud marker at function entry so daemon
    // shutdown reasons are visible in stderr without TOURING_LOG=info.
    // The existing "graceful shutdown complete" eprintln near the bottom
    // already covers the success case; this one logs *initiation* so we
    // can tell "shutdown started but stuck" from "shutdown never called".
    eprintln!("[touring-daemon] graceful_shutdown CALLED — collecting actors for ordered shutdown");

    // F5 — capture the final KPI snapshot BEFORE the actors drain, while the
    // runtime is still live and the process-wide gate-metrics counters still hold
    // this session's accumulation (they reset on the next daemon start). Opt-in;
    // best-effort. This is the precise fix for "the coupling ratio freshens after
    // every restart" — the shutdown sample is durable in docs/kpi/.
    let kpi_daily = crate::shared::feature_flags::gate_metrics_daily_enabled();
    eprintln!("[touring-daemon] graceful_shutdown: kpi_daily_flush={kpi_daily}");
    if kpi_daily {
        flush_kpi_snapshot(runtime).await;
    }

    // Collect runtimes under write lock (exclusive access during shutdown).
    let runtimes: Vec<(PathBuf, ProjectRuntime)> = {
        let mut map = runtime.write().await;
        map.drain().collect()
    };

    // Send Shutdown command to every project actor. The actor owns the
    // HookRuntime on its dedicated OS thread, so WAL checkpoint / LinUCB /
    // CRDT persistence all run inside the actor loop before it exits.
    // We await each done-oneshot with a bounded timeout so a wedged actor
    // can't stall the daemon exit.
    let shutdown_timeout = Duration::from_secs(5);
    for (project_root, project_runtime) in runtimes {
        let project = project_root.display().to_string();
        let (done_tx, done_rx) = oneshot::channel::<()>();

        if project_runtime
            .cmd_tx
            .send(ProjectCommand::Shutdown { done: done_tx })
            .await
            .is_err()
        {
            eprintln!(
                "[touring-daemon] actor already gone for {project} — skipping shutdown signal"
            );
            continue;
        }

        match timeout(shutdown_timeout, done_rx).await {
            Ok(Ok(())) => {}
            Ok(Err(_)) => eprintln!("[touring-daemon] actor exited without ack for {project}"),
            Err(_) => eprintln!(
                "[touring-daemon] actor shutdown timed out for {project} after {:?}",
                shutdown_timeout
            ),
        }
    }

    // THSF Phase 5 Opt A — stop the embedded capnp RPC server and join
    // its dedicated OS thread. No-op when the feature is disabled or
    // `install` was never called. Fires before the circuit-breaker
    // reset so live RPC clients see the socket disappear first.
    #[cfg(feature = "capnp-server")]
    {
        let _ = crate::capnp_embed::shutdown_and_join();
    }

    eprintln!("[touring-daemon] graceful shutdown complete");
    // S14: Reset circuit breaker so next daemon start is clean
    crate::circuit_breaker::reset();
    std::process::exit(0);
}

/// Acquire the daemon lock file using flock(2) for atomic lock acquisition.
/// Check if another daemon is already running by probing the socket.
/// This is more reliable than lock files because the socket is the actual
/// communication endpoint — if something is listening, a daemon is running.
fn is_daemon_running(socket_path: &std::path::Path) -> bool {
    use std::os::unix::net::UnixStream;

    // Try to connect to the socket — if connection succeeds, a daemon is listening
    match UnixStream::connect(socket_path) {
        Ok(_stream) => {
            tracing::debug!("daemon is running (socket connected)");
            true
        }
        Err(e) => {
            tracing::debug!("no daemon running (socket connect failed): {e}");
            false
        }
    }
}

/// Remove a stale socket file if it exists.
///
/// This prevents the socket probe from failing when a previous daemon
/// exited abnormally (socket file persisted but no process listening).
fn remove_stale_socket(socket_path: &std::path::Path) {
    if socket_path.exists() {
        // Check if anything is listening on the socket before removing
        // If is_daemon_running would return false, the socket is stale
        if !is_daemon_running(socket_path) {
            tracing::debug!("removing stale socket file");
            let _ = std::fs::remove_file(socket_path);
        }
    }
}

/// Acquire the daemon lock using flock(2) for atomic lock acquisition.
/// This prevents race conditions between check and start.
///
/// Returns `Some(File)` on success — the caller MUST keep the `File` alive
/// for the entire daemon lifetime. Dropping it closes the fd, which releases
/// the flock and allows another daemon to start.
///
/// Lock semantics:
/// - LOCK_EX: Exclusive lock (only one process can hold)
/// - LOCK_NB: Non-blocking (fail immediately if can't acquire)
/// - Lock is automatically released when file descriptor is closed (process death)
///
/// Outcome of [`acquire_lock`] — three-way distinguishing the
/// canonical states of a daemon-start race (REGRA #19 Sprint 3 PC-2).
pub(crate) enum LockOutcome {
    /// We won the flock race. Caller MUST keep the [`File`] alive — dropping it
    /// closes the fd and releases the kernel flock, allowing another daemon
    /// to start mid-flight.
    Acquired(std::fs::File),
    /// Another touring-daemon process is already alive AND `/proc/<pid>/comm`
    /// (or socket probe, as a backup signal) confirms it. The current
    /// `--start-daemon` invocation should exit silently with code 0 — this
    /// is the canonical outcome when multiple CC sessions race the same
    /// auto-spawn path.
    AlreadyAlive,
    /// The lock could not be acquired but the holder is NOT a touring-daemon
    /// (PID reuse, stale lockfile with another process, FS error, etc.).
    /// Caller should bubble this up as a hard error.
    Failed(String),
}

/// Verify that `pid` is alive AND its kernel comm string identifies it as a
/// touring-daemon (Sprint 3 PC-1 set this via `prctl(PR_SET_NAME)`). Returns
/// `true` only when both conditions hold; any I/O error (proc entry gone,
/// permission denied) is treated as "not a daemon" — conservative.
fn pid_is_live_touring_daemon(pid: i32) -> bool {
    if pid <= 0 {
        return false;
    }
    let comm_path = format!("/proc/{pid}/comm");
    match std::fs::read_to_string(&comm_path) {
        Ok(s) => s.trim() == "touring-daemon",
        Err(_) => false,
    }
}

fn acquire_lock(lock_path: &std::path::Path, socket_path: &std::path::Path) -> LockOutcome {
    // First: clean up any stale socket from a previous abnormal exit
    // This MUST happen before the socket probe to handle the crash-restart case
    remove_stale_socket(socket_path);

    // Second check: is a daemon already listening on the socket?
    // This catches the case where a daemon is truly running
    if is_daemon_running(socket_path) {
        tracing::debug!("daemon already running (socket probe)");
        return LockOutcome::AlreadyAlive;
    }

    // Open lock file (create if not exists). Do NOT truncate yet — if the
    // flock acquisition fails, we want to read back the existing PID so the
    // /proc/<pid>/comm check below can classify Acquired vs AlreadyAlive vs
    // Failed accurately.
    let lock_file = match std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(lock_path)
    {
        Ok(f) => f,
        Err(e) => {
            tracing::debug!("failed to open lock file: {e}");
            return LockOutcome::Failed(format!("open {}: {e}", lock_path.display()));
        }
    };

    // Try to acquire exclusive lock non-blocking
    // SAFETY: flock(2) is async-signal-safe, lock_file is a valid fd
    let flock_result = unsafe {
        flock(
            std::os::unix::io::AsRawFd::as_raw_fd(&lock_file),
            LOCK_EX | LOCK_NB,
        )
    };

    if flock_result == 0 {
        // Successfully acquired lock — NOW truncate (we hold the exclusive
        // lock, so this is race-free) and write our PID.
        use std::io::{Seek, SeekFrom, Write};
        let mut lock_file = lock_file;
        let _ = lock_file.set_len(0);
        let _ = lock_file.seek(SeekFrom::Start(0));
        if let Err(e) = write!(lock_file, "{}", std::process::id()) {
            tracing::debug!("failed to write PID to lock file: {e}");
        }
        let _ = lock_file.flush();
        tracing::debug!("acquired daemon lock (flock) — held for daemon lifetime");
        return LockOutcome::Acquired(lock_file);
    }

    // Flock failed — read the existing PID and classify the holder.
    use std::io::Read;
    let mut lock_file = lock_file;
    let mut existing_pid_str = String::new();
    let _ = lock_file.read_to_string(&mut existing_pid_str);
    let existing_pid: i32 = existing_pid_str.trim().parse().unwrap_or(0);

    if pid_is_live_touring_daemon(existing_pid) {
        tracing::debug!("lock held by live touring-daemon (pid={existing_pid}) — silent exit 0");
        LockOutcome::AlreadyAlive
    } else {
        let reason = format!(
            "lock held by pid={existing_pid} whose /proc/<pid>/comm is not 'touring-daemon' \
             (stale lockfile or PID reuse). Remove {} manually after confirming the daemon \
             is not running.",
            lock_path.display()
        );
        tracing::warn!("{reason}");
        LockOutcome::Failed(reason)
    }
}

// flock(2) constants and function — declared locally to avoid libc dependency
const LOCK_EX: std::os::raw::c_int = 2; // Exclusive lock
const LOCK_NB: std::os::raw::c_int = 4; // Non-blocking

unsafe extern "C" {
    fn flock(fd: std::os::unix::io::RawFd, operation: std::os::raw::c_int) -> std::os::raw::c_int;
}

#[cfg(test)]
#[path = "daemon_tests.rs"]
mod tests;
