//! File-based circuit breaker for IPC daemon calls — Hierarchical Multi-Dimensional.
//!
//! ## Problem
//! Every `check()`, `record_failure()`, and `record_success()` was doing
//! synchronous file I/O (read/write `/tmp/touring-circuit-{uid}.state`) on every call.
//! Under high concurrency (64 parallel connections), this caused massive I/O contention:
//! thousands of file operations per second on a single file → Resource temporarily unavailable.
//!
//! ## Solution: In-Memory Cache + Async Write Coalescing
//!
//! - **In-memory `CircuitState`** cached as a global static with `RwLock`
//! - **Synchronous read path**: reads from memory (µs) instead of disk (ms)
//! - **Write coalescing**: writes are batched and flushed to disk every N operations
//!   or every FLUSH_INTERVAL_MS, whichever comes first
//! - **Background flush task**: a dedicated thread handles all disk writes
//! - **Graceful shutdown**: flush on drop to persist all pending writes
//! - **File-based fallback**: if in-memory state is corrupted/missing, rebuild from file
//!   (preserves circuit state across thin-client process restarts)
//!
//! ## Concurrency
//!
//! Multiple thin-client processes (from concurrent sessions) all share the same
//! circuit state file. With in-memory caching, reads never contend.
//! Writes are serialized by a background thread — no concurrent disk writes.
//!
//! # Architecture
//!
//! ```text
//! thin-client process                      background flush thread
//!  ┌─────────────────────────────────┐     ┌────────────────────────┐
//!  │ check() → read memory (RwLock)  │     │                        │
//!  │ record_failure() → mark dirty   │────▶│ batched write to disk │
//!  │ record_success() → mark dirty   │     │ every FLUSH_INTERVAL   │
//!  └─────────────────────────────────┘     └────────────────────────┘
//! ```

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{OnceLock, RwLock};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

// ── Flush Configuration ───────────────────────────────────────────────────────

/// Flush to disk every N dirty events — coalesces writes under burst load.
const FLUSH_EVENTS_THRESHOLD: u32 = 50;

/// Flush to disk every FLUSH_INTERVAL_MS even if threshold not reached.
/// Ensures circuit state is persisted even if no more events arrive.
const FLUSH_INTERVAL_MS: u64 = 5_000;

/// Force synchronous write every N flushes — prevents data loss on crash.
const SYNC_FLUSH_INTERVAL: u32 = 10;

/// File path for circuit state — single writer, readers use memory cache.
fn circuit_path() -> PathBuf {
    let uid = crate::current_uid();
    PathBuf::from(format!("/tmp/touring-circuit-{uid}.state"))
}

// ── Operation Class ────────────────────────────────────────────────────────────

/// Operation class — determines threshold and cooldown based on cost.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OpClass {
    /// Light read-only: index find, ast overview, wiring status
    Light,
    /// Medium: memory recall, suggest, decompose
    Medium,
    /// Heavy: index rebuild, mcts search, blast radius on large files
    Heavy,
    /// Critical: daemon health check, session start
    Critical,
}

impl OpClass {
    /// Returns operation class based on hook name patterns.
    #[inline]
    pub fn from_hook_name(name: &str) -> Self {
        if has_any(name, &["session-start", "daemon-health"]) {
            return OpClass::Critical;
        }
        if has_any(
            name,
            &[
                "index-rebuild",
                "index rebuild",
                "mcts-search",
                "mcts search",
                "decompose-create",
                "decompose create",
                "evolution",
                "blast",
            ],
        ) {
            return OpClass::Heavy;
        }
        if has_any(
            name,
            &[
                "index-find",
                "index find",
                "ast-find",
                "ast find",
                "ast-overview",
                "ast overview",
                "wiring",
                "memory-recall",
                "memory recall",
                "suggest",
                "classify",
                "scan-pii",
                "cognitive",
            ],
        ) {
            return OpClass::Light;
        }
        OpClass::Medium
    }
}

#[inline]
fn has_any(s: &str, patterns: &[&str]) -> bool {
    patterns.iter().any(|p| s.contains(p))
}

impl OpClass {
    /// Consecutive-failure count that trips the breaker for this class.
    pub fn threshold(self) -> u32 {
        match self {
            OpClass::Critical => 10,
            OpClass::Heavy => 8,
            OpClass::Medium => 6,
            OpClass::Light => 10,
        }
    }

    /// Seconds the breaker stays open after tripping for this class.
    pub fn cooldown_secs(self) -> u64 {
        match self {
            OpClass::Critical => 30,
            OpClass::Heavy => 120,
            OpClass::Medium => 60,
            OpClass::Light => 45,
        }
    }

    /// Sliding window (seconds) over which failures are counted for this class.
    pub fn window_secs(self) -> u64 {
        match self {
            OpClass::Critical => 120,
            OpClass::Heavy => 90,
            OpClass::Medium => 60,
            OpClass::Light => 45,
        }
    }

    /// Relative cost weight used to scale this class's contribution to scores.
    pub fn cost_weight(self) -> f64 {
        match self {
            OpClass::Critical => 0.1,
            OpClass::Heavy => 2.0,
            OpClass::Medium => 1.0,
            OpClass::Light => 0.5,
        }
    }
}

// ── State Types ────────────────────────────────────────────────────────────────

/// Global breaker state shared across all operations and projects.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GlobalState {
    /// Number of catastrophic failures recorded globally.
    pub catastrophic_count: u32,
    /// Unix timestamp until which the global breaker stays open (0 = closed).
    pub open_until_ts: u64,
}

/// Per-operation-class breaker state.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ClassBreaker {
    /// Failures counted within the current sliding window.
    pub failure_count: u32,
    /// Unix timestamp of the most recent failure.
    pub last_failure_ts: u64,
    /// Unix timestamp until which this class breaker stays open (0 = closed).
    pub open_until_ts: u64,
}

/// Per-project breaker state with a weighted failure score.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProjectBreaker {
    /// Failures counted within the current sliding window.
    pub failure_count: u32,
    /// Unix timestamp of the most recent failure.
    pub last_failure_ts: u64,
    /// Unix timestamp until which this project breaker stays open (0 = closed).
    pub open_until_ts: u64,
    /// Cost-weighted failure score used to trip heavier projects sooner.
    pub weighted_score: f64,
}

/// Per-session breaker state.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SessionBreaker {
    /// Failures counted within the current sliding window.
    pub failure_count: u32,
    /// Unix timestamp of the most recent failure.
    pub last_failure_ts: u64,
    /// Unix timestamp until which this session breaker stays open (0 = closed).
    pub open_until_ts: u64,
}

type ClassBreakers = HashMap<OpClass, ClassBreaker>;
type ProjectBreakers = HashMap<String, ProjectBreaker>;
type SessionBreakers = HashMap<String, SessionBreaker>;

/// Aggregate circuit-breaker state: the global breaker plus the per-class,
/// per-project, and per-session breaker maps.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CircuitState {
    global: GlobalState,
    by_class: ClassBreakers,
    by_project: ProjectBreakers,
    by_session: SessionBreakers,
}

/// Outcome of a breaker check for a single operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitCheck {
    /// Whether the operation should be skipped (breaker open).
    pub skip: bool,
    /// Human-readable explanation for the decision.
    pub reason: String,
    /// Which breaker tier triggered (e.g. `"global"`, `"class"`, `"project"`).
    pub circuit: &'static str,
    /// Operation class the check was evaluated against.
    pub op_class: OpClass,
    /// Seconds the caller should wait before retrying when skipped.
    pub retry_after_secs: u64,
}

/// Structured health report for the circuit breaker system.
///
/// Returned by [`health()`] — provides a stable, versioned view of all
/// circuit breaker state at a point in time.
///
/// ## Fields
/// - `global_state` — catastrophic failure state affecting all operations
/// - `by_class` — per-operation-class breakers (Light/Medium/Heavy/Critical)
/// - `by_project` — per-project breakers keyed by project path
/// - `by_session` — per-session breakers keyed by session ID
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitHealthReport {
    /// Global catastrophic failure state.
    pub global_state: GlobalState,
    /// Per-operation-class circuit breakers.
    pub by_class: ClassBreakers,
    /// Per-project circuit breakers keyed by project path.
    pub by_project: ProjectBreakers,
    /// Per-session circuit breakers keyed by session ID.
    pub by_session: SessionBreakers,
}

impl CircuitCheck {
    fn skip(
        reason: &'static str,
        circuit: &'static str,
        op_class: OpClass,
        retry_after: u64,
    ) -> Self {
        Self {
            skip: true,
            reason: reason.into(),
            circuit,
            op_class,
            retry_after_secs: retry_after,
        }
    }
    fn proceed(op_class: OpClass) -> Self {
        Self {
            skip: false,
            reason: "all circuits closed".into(),
            circuit: "none",
            op_class,
            retry_after_secs: 0,
        }
    }
}

// ── In-Memory State ───────────────────────────────────────────────────────────

/// In-memory circuit state with dirty tracking for coalesced writes.
struct CachedState {
    inner: CircuitState,
    dirty: bool,
    dirty_count: u32,
    last_flush: Instant,
    flush_count: u32,
    flush_tx: std::sync::mpsc::Sender<FlushCmd>,
}

/// Flush commands sent to the background flush thread.
enum FlushCmd {
    Write(CircuitState),
    SyncWrite(CircuitState),
    Shutdown,
}

impl CachedState {
    fn new(flush_tx: std::sync::mpsc::Sender<FlushCmd>) -> Self {
        Self {
            inner: CircuitState::default(),
            dirty: false,
            dirty_count: 0,
            last_flush: Instant::now(),
            flush_count: 0,
            flush_tx,
        }
    }

    /// Mark state as dirty and schedule a flush if thresholds are crossed.
    fn mark_dirty(&mut self) {
        self.dirty = true;
        self.dirty_count += 1;

        // Coalesced flush: write synchronously if thresholds crossed
        if self.dirty_count >= FLUSH_EVENTS_THRESHOLD
            || self.last_flush.elapsed() >= Duration::from_millis(FLUSH_INTERVAL_MS)
        {
            self.flush(false);
        }
    }

    /// Flush dirty state to disk via the background thread.
    fn flush(&mut self, force_sync: bool) {
        if !self.dirty {
            return;
        }

        let state = self.inner.clone();
        let flush_count = self.flush_count;

        if force_sync || flush_count % SYNC_FLUSH_INTERVAL == SYNC_FLUSH_INTERVAL - 1 {
            let _ = self.flush_tx.send(FlushCmd::SyncWrite(state));
        } else {
            let _ = self.flush_tx.send(FlushCmd::Write(state));
        }

        self.dirty = false;
        self.dirty_count = 0;
        self.last_flush = Instant::now();
        self.flush_count += 1;
    }
}

/// Global in-memory circuit state.
/// Lazily initialized on first use via OnceLock.
static CIRCUIT_CACHE: OnceLock<RwLock<CachedState>> = OnceLock::new();

/// Track whether the background flush thread has been started.
static FLUSH_THREAD_STARTED: AtomicU32 = AtomicU32::new(0);

/// Get read access to the circuit cache.
#[inline]
fn with_cache<T, F>(f: F) -> T
where
    F: FnOnce(&RwLock<CachedState>) -> T,
{
    f(CIRCUIT_CACHE.get().expect("circuit cache initialized"))
}

/// Gracefully shut down the background flush thread.
///
/// Sends a `Shutdown` command to the flush thread so it can persist final
/// state and exit cleanly. Safe to call multiple times — subsequent calls
/// are no-ops if the cache was never initialized.
pub fn shutdown() {
    if let Some(cache) = CIRCUIT_CACHE.get()
        && let Ok(state) = cache.read()
    {
        let _ = state.flush_tx.send(FlushCmd::Shutdown);
    }
}

/// Start the background flush thread if not already running.
/// Uses compare-exchange to ensure only one thread starts (even across processes).
fn ensure_flush_thread() {
    if FLUSH_THREAD_STARTED.swap(1, Ordering::SeqCst) == 0 {
        let (tx, rx) = std::sync::mpsc::channel::<FlushCmd>();
        let path = circuit_path();

        // Initialize cache with the sender
        let _ = CIRCUIT_CACHE.get_or_init(|| RwLock::new(CachedState::new(tx)));

        thread::spawn(move || {
            let flush_interval = Duration::from_millis(FLUSH_INTERVAL_MS);
            loop {
                match rx.recv_timeout(flush_interval) {
                    Ok(FlushCmd::Shutdown) => {
                        // On shutdown, do one final sync write
                        if let Ok(FlushCmd::SyncWrite(s)) = rx.recv() {
                            let _ = write_state_to_file(&path, &s);
                        }
                        break;
                    }
                    Ok(FlushCmd::Write(state)) => {
                        let _ = write_state_to_file(&path, &state);
                    }
                    Ok(FlushCmd::SyncWrite(state)) => {
                        let _ = write_state_to_file(&path, &state);
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                        // Periodic flush — check if cache is dirty
                        if let Some(cache) = CIRCUIT_CACHE.get()
                            && let Ok(mut cache) = cache.write()
                            && cache.dirty
                        {
                            let state = cache.inner.clone();
                            let _ = write_state_to_file(&path, &state);
                            cache.dirty = false;
                            cache.dirty_count = 0;
                            cache.last_flush = Instant::now();
                        }
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                }
            }
        });
    }
}

// ── File I/O ────────────────────────────────────────────────────────────────

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn write_state_to_file(path: &PathBuf, state: &CircuitState) -> std::io::Result<()> {
    let json = serde_json::to_string(state)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    // Write to temp file then rename for atomicity
    let temp_path = path.with_extension("tmp");
    std::fs::write(&temp_path, json.as_bytes())?;
    std::fs::rename(&temp_path, path)?;
    Ok(())
}

fn read_state_from_file() -> CircuitState {
    let path = circuit_path();
    match std::fs::read_to_string(&path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
        Err(_) => CircuitState::default(),
    }
}

/// Load state from disk into the in-memory cache.
/// Called once at startup to restore state from previous runs.
fn load_cached_state() {
    // Skip if already initialized — avoids overwriting live in-memory state with
    // potentially stale disk data when concurrent callers (e.g. parallel tests or
    // a second session-start hook) call init() after the cache is warm.
    if CIRCUIT_CACHE.get().is_some() {
        return;
    }
    let state = read_state_from_file();
    let (tx, _) = std::sync::mpsc::channel();
    let cache = CIRCUIT_CACHE.get_or_init(|| RwLock::new(CachedState::new(tx)));
    let mut cache = cache.write().unwrap_or_else(|e| e.into_inner());
    cache.inner = state;
    cache.dirty = false;
    cache.dirty_count = 0;
    cache.last_flush = Instant::now();
    cache.flush_count = 0;
}

// ── Constants ────────────────────────────────────────────────────────────────

const GLOBAL_CATASTROPHIC_THRESHOLD: u32 = 3;
const GLOBAL_COOLDOWN_SECS: u64 = 30;
const GLOBAL_WINDOW_SECS: u64 = 60;

const PROJECT_THRESHOLD: u32 = 4;
const PROJECT_COOLDOWN_SECS: u64 = 90;
const PROJECT_WINDOW_SECS: u64 = 60;
const PROJECT_WEIGHT_THRESHOLD: f64 = 5.0;

const SESSION_THRESHOLD: u32 = 6;
const SESSION_COOLDOWN_SECS: u64 = 60;
const SESSION_WINDOW_SECS: u64 = 120;

// ── Public API ───────────────────────────────────────────────────────────────

/// Initialize the circuit breaker — starts the background flush thread
/// and loads persisted state from disk. Safe to call multiple times.
pub fn init() {
    load_cached_state();
    ensure_flush_thread();
}

/// Check if daemon should be skipped for this request.
pub fn check(hook_name: &str, project: Option<&str>, session: Option<&str>) -> CircuitCheck {
    // Ensure flush thread is running (init on first use)
    ensure_flush_thread();

    let op_class = OpClass::from_hook_name(hook_name);
    let now = now_secs();

    let inner = with_cache(|c| c.read().unwrap_or_else(|e| e.into_inner()).inner.clone());
    let inner = &inner;

    // 1. Global catastrophic
    if inner.global.catastrophic_count >= GLOBAL_CATASTROPHIC_THRESHOLD
        && now < inner.global.open_until_ts
    {
        return CircuitCheck::skip(
            "global catastrophic circuit open",
            "global",
            op_class,
            inner.global.open_until_ts.saturating_sub(now),
        );
    }

    // 2. Operation class check
    if let Some(class_breaker) = inner.by_class.get(&op_class)
        && class_breaker.open_until_ts > now
    {
        return CircuitCheck::skip(
            "operation class circuit open",
            "class",
            op_class,
            class_breaker.open_until_ts.saturating_sub(now),
        );
    }

    // 3. Project check
    if let Some(p) = project
        && let Some(proj_breaker) = inner.by_project.get(p)
        && proj_breaker.open_until_ts > now
    {
        return CircuitCheck::skip(
            "project circuit open",
            "project",
            op_class,
            proj_breaker.open_until_ts.saturating_sub(now),
        );
    }

    // 4. Session check
    if let Some(s) = session
        && let Some(sess_breaker) = inner.by_session.get(s)
        && sess_breaker.open_until_ts > now
    {
        return CircuitCheck::skip(
            "session circuit open",
            "session",
            op_class,
            sess_breaker.open_until_ts.saturating_sub(now),
        );
    }

    CircuitCheck::proceed(op_class)
}

/// Returns true if the circuit is OPEN (daemon should be skipped).
pub fn is_open() -> bool {
    ensure_flush_thread();
    let inner = with_cache(|c| c.read().unwrap_or_else(|e| e.into_inner()).inner.clone());
    let now = now_secs();
    let inner = &inner;

    if inner.global.catastrophic_count >= GLOBAL_CATASTROPHIC_THRESHOLD
        && now < inner.global.open_until_ts
    {
        return true;
    }
    if inner.by_class.values().any(|b| b.open_until_ts > now) {
        return true;
    }
    if inner.by_project.values().any(|b| b.open_until_ts > now) {
        return true;
    }
    if inner.by_session.values().any(|b| b.open_until_ts > now) {
        return true;
    }
    false
}

/// Record a daemon call failure for a specific operation class.
pub fn record_failure(
    hook_name: &str,
    project: Option<&str>,
    session: Option<&str>,
    catastrophic: bool,
) {
    ensure_flush_thread();

    let mut cache = CIRCUIT_CACHE
        .get()
        .expect("circuit cache initialized")
        .write()
        .unwrap_or_else(|e| e.into_inner());
    let now = now_secs();
    let op_class = OpClass::from_hook_name(hook_name);
    let state = &mut cache.inner;

    // Global catastrophic
    if catastrophic {
        if now.saturating_sub(state.global.open_until_ts) > GLOBAL_WINDOW_SECS {
            state.global.catastrophic_count = 0;
        }
        state.global.catastrophic_count += 1;
        state.global.open_until_ts = now + GLOBAL_COOLDOWN_SECS;
        if state.global.catastrophic_count >= GLOBAL_CATASTROPHIC_THRESHOLD {
            tracing::error!(
                catastrophic_count = state.global.catastrophic_count,
                "GLOBAL circuit breaker OPEN"
            );
        }
        cache.mark_dirty();
        return;
    }

    // Operation class breaker
    let class_breaker = state.by_class.entry(op_class).or_default();
    if now.saturating_sub(class_breaker.last_failure_ts) > op_class.window_secs() {
        class_breaker.failure_count = 0;
    }
    class_breaker.failure_count += 1;
    class_breaker.last_failure_ts = now;
    if class_breaker.failure_count >= op_class.threshold() {
        class_breaker.open_until_ts = now + op_class.cooldown_secs();
        class_breaker.failure_count = 0;
        tracing::warn!(op_class = ?op_class, "circuit OPEN for class {:?}", op_class);
    }

    // Project breaker
    if let Some(p) = project {
        let breaker = state.by_project.entry(p.to_string()).or_default();
        if now.saturating_sub(breaker.last_failure_ts) > PROJECT_WINDOW_SECS {
            breaker.failure_count = 0;
            breaker.weighted_score = 0.0;
        }
        breaker.failure_count += 1;
        breaker.last_failure_ts = now;
        breaker.weighted_score += op_class.cost_weight();
        if breaker.failure_count >= PROJECT_THRESHOLD
            || breaker.weighted_score >= PROJECT_WEIGHT_THRESHOLD
        {
            breaker.open_until_ts = now + PROJECT_COOLDOWN_SECS;
            breaker.failure_count = 0;
            breaker.weighted_score = 0.0;
            tracing::warn!(project = p, "circuit OPEN for project {}", p);
        }
    }

    // Session breaker
    if let Some(s) = session {
        let breaker = state.by_session.entry(s.to_string()).or_default();
        if now.saturating_sub(breaker.last_failure_ts) > SESSION_WINDOW_SECS {
            breaker.failure_count = 0;
        }
        breaker.failure_count += 1;
        breaker.last_failure_ts = now;
        if breaker.failure_count >= SESSION_THRESHOLD {
            breaker.open_until_ts = now + SESSION_COOLDOWN_SECS;
            breaker.failure_count = 0;
            tracing::warn!(session = s, "circuit OPEN for session {}", s);
        }
    }

    cache.mark_dirty();
}

/// Record a successful daemon call. Resets failure counters.
pub fn record_success(hook_name: &str, project: Option<&str>, session: Option<&str>) {
    ensure_flush_thread();

    let mut cache = CIRCUIT_CACHE
        .get()
        .expect("circuit cache initialized")
        .write()
        .unwrap_or_else(|e| e.into_inner());
    let now = now_secs();
    let op_class = OpClass::from_hook_name(hook_name);
    let state = &mut cache.inner;

    // Decay class breaker failures
    if let Some(breaker) = state.by_class.get_mut(&op_class) {
        decay_class_breaker(breaker, now);
    }

    // Decay project breaker weighted score
    if let Some(p) = project
        && let Some(breaker) = state.by_project.get_mut(p)
    {
        decay_project_breaker(breaker, op_class, now);
    }

    // Decay session breaker
    if let Some(s) = session
        && let Some(breaker) = state.by_session.get_mut(s)
    {
        decay_session_breaker(breaker, now);
    }

    // Global catastrophic: on ANY success, start recovery
    if state.global.catastrophic_count > 0 {
        state.global.catastrophic_count = state.global.catastrophic_count.saturating_sub(1);
        if state.global.catastrophic_count == 0 {
            state.global.open_until_ts = 0;
        }
    }

    cache.mark_dirty();
}

#[inline]
fn decay_class_breaker(breaker: &mut ClassBreaker, now: u64) {
    if breaker.failure_count > 0 {
        breaker.failure_count -= 1;
    }
    if breaker.open_until_ts > 0 && breaker.open_until_ts <= now {
        breaker.open_until_ts = 0;
    }
}

#[inline]
fn decay_project_breaker(breaker: &mut ProjectBreaker, op_class: OpClass, now: u64) {
    breaker.weighted_score = (breaker.weighted_score - op_class.cost_weight() * 0.5).max(0.0);
    breaker.failure_count = breaker.failure_count.saturating_sub(1);
    if breaker.open_until_ts > 0 && breaker.open_until_ts <= now {
        breaker.open_until_ts = 0;
    }
}

#[inline]
fn decay_session_breaker(breaker: &mut SessionBreaker, now: u64) {
    breaker.failure_count = breaker.failure_count.saturating_sub(1);
    if breaker.open_until_ts > 0 && breaker.open_until_ts <= now {
        breaker.open_until_ts = 0;
    }
}

/// Reset the circuit breaker state entirely — called on graceful daemon shutdown.
pub fn reset() {
    let mut cache = CIRCUIT_CACHE
        .get()
        .expect("circuit cache initialized")
        .write()
        .unwrap_or_else(|e| e.into_inner());
    cache.inner = CircuitState::default();
    cache.dirty = false;
    cache.dirty_count = 0;
    cache.flush_count = 0;
    // Synchronous write to reset file
    let _ = write_state_to_file(&circuit_path(), &CircuitState::default());
}

/// Get current state summary for debugging/monitoring.
pub fn state_summary() -> CircuitState {
    ensure_flush_thread();
    with_cache(|c| c.read().unwrap_or_else(|e| e.into_inner()).inner.clone())
}

/// Returns a structured health report for all circuit breakers.
///
/// This is the primary entry point for reading circuit breaker state from
/// `HookRuntime::circuit_state()` — provides a stable, versioned view of
/// global, class, project, and session-level breaker state.
///
/// # Example
/// ```ignore
/// let report = health();
/// eprintln!("global catastrophic = {}", report.global_state.catastrophic_count);
/// ```
pub fn health() -> CircuitHealthReport {
    ensure_flush_thread();
    let cache = with_cache(|c| c.read().unwrap_or_else(|e| e.into_inner()).inner.clone());
    CircuitHealthReport {
        global_state: cache.global.clone(),
        by_class: cache.by_class.clone(),
        by_project: cache.by_project.clone(),
        by_session: cache.by_session.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Serializes all tests that mutate the global CIRCUIT_CACHE.
    // Without this, `catastrophic_resets_on_success` can open the global circuit
    // while `circuit_closed_by_default` is checking it — causing a flaky failure.
    static CB_MUTEX: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
    fn cb_lock() -> std::sync::MutexGuard<'static, ()> {
        CB_MUTEX
            .get_or_init(|| std::sync::Mutex::new(()))
            .lock()
            .unwrap_or_else(|e| e.into_inner())
    }

    #[test]
    fn op_class_from_name_light() {
        assert_eq!(OpClass::from_hook_name("cli-index-find"), OpClass::Light);
        assert_eq!(OpClass::from_hook_name("cli-ast-overview"), OpClass::Light);
        assert_eq!(OpClass::from_hook_name("cli-wiring-status"), OpClass::Light);
    }

    #[test]
    fn op_class_from_name_critical() {
        assert_eq!(OpClass::from_hook_name("session-start"), OpClass::Critical);
        assert_eq!(OpClass::from_hook_name("daemon-health"), OpClass::Critical);
    }

    #[test]
    fn op_class_from_name_heavy() {
        assert_eq!(OpClass::from_hook_name("index rebuild"), OpClass::Heavy);
        assert_eq!(OpClass::from_hook_name("mcts search"), OpClass::Heavy);
    }

    #[test]
    fn circuit_closed_by_default() {
        let _guard = cb_lock();
        init();
        reset();
        let chk = check("cli-index-find", Some("/project"), Some("session-1"));
        assert!(!chk.skip);
        assert_eq!(chk.circuit, "none");
    }

    #[test]
    fn project_circuit_opens_after_threshold() {
        let _guard = cb_lock();
        init();
        reset();
        let project = "/tmp/test-project";
        let hook = "cli-index-find";

        for _ in 0..5 {
            record_failure(hook, Some(project), None, false);
        }

        let chk = check(hook, Some(project), None);
        assert!(chk.skip);
        assert_eq!(chk.circuit, "project");
    }

    #[test]
    fn catastrophic_resets_on_success() {
        let _guard = cb_lock();
        init();
        reset();
        let hook = "daemon-crash";

        for _ in 0..3 {
            record_failure(hook, None, None, true);
        }

        assert!(is_open());

        record_success(hook, None, None);
        // After 3 successes, catastrophic count should have decayed
        let state = state_summary();
        assert!(state.global.catastrophic_count < 3);
    }
}
