//! Bounded subprocess pool + backpressure for the Code Execution Gateway —
//! phase **P4.4** of CEG Pln2 (`docs/2026-05-17-ceg-pln2-plan.md`).
//!
//! Every X5 dry-run and every X8 SUPERVISED-EXEC ultimately spawns an OS
//! subprocess through `spawn_and_capture` in [`crate::gateway::sandbox_executor`].
//! Without a bound, a burst of code-bearing tool calls would spawn subprocesses
//! without limit — the very resource exhaustion the gateway exists to prevent,
//! now at the gateway's own level (CEG Pln2 risk R5).
//!
//! [`ExecPool`] is that bound: a `tokio` [`Semaphore`] holding `N` permits. A
//! caller [`acquire`](ExecPool::acquire)s a permit before spawning and holds it
//! until the subprocess exits, so the daemon never has more than `N` sandboxed
//! subprocesses live at once.
//!
//! # Backpressure
//!
//! When all `N` permits are taken, [`acquire`](ExecPool::acquire) does not fail
//! at once — it **queues**, waiting up to the configured acquire timeout for a
//! slot, and returns [`PoolError::QueueTimeout`] only if that whole window
//! elapses. A caller that cannot wait uses
//! [`try_acquire`](ExecPool::try_acquire), which reports
//! [`PoolError::Saturated`] immediately. Excess load is *shaped* — never
//! silently dropped and never unbounded.
//!
//! # One pool per daemon
//!
//! The "never more than `N`" guarantee is daemon-wide: every gateway entry
//! point shares the single process-global [`ExecPool::global`] instance.
//!
//! Best-practice basis: `docs/2026-05-17-ceg-best-practices.md` and the tokio
//! `Semaphore` "limit concurrent requests" pattern.

use std::env;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use std::{error, fmt};

use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio::time::timeout;

#[cfg(feature = "txn_lock_enforcement")]
use super::txn::{AccessDeclaration, AcquireResult, TxnLockManager};

/// Smallest usable concurrency cap — a pool of zero permits would deadlock
/// every execution, so one is the floor.
const MIN_MAX_CONCURRENT: usize = 1;
/// Largest concurrency cap — a guard against a mistyped env var asking the
/// daemon to run thousands of subprocesses at once.
const MAX_MAX_CONCURRENT: usize = 256;
/// Concurrency cap used when the host CPU count cannot be probed.
const FALLBACK_MAX_CONCURRENT: usize = 4;
/// Time a saturated [`ExecPool::acquire`] waits for a slot before giving up.
const DEFAULT_ACQUIRE_TIMEOUT: Duration = Duration::from_secs(60);
/// Environment variable overriding the concurrency cap: `TOURING_CEG_MAX_CONCURRENT`.
const ENV_MAX_CONCURRENT: &str = "TOURING_CEG_MAX_CONCURRENT";
/// Environment variable overriding the acquire timeout (whole seconds):
/// `TOURING_CEG_ACQUIRE_TIMEOUT_SECS`.
const ENV_ACQUIRE_TIMEOUT_SECS: &str = "TOURING_CEG_ACQUIRE_TIMEOUT_SECS";

/// The default concurrency cap: the host's logical CPU count, clamped into the
/// `[MIN_MAX_CONCURRENT, MAX_MAX_CONCURRENT]` range, or
/// [`FALLBACK_MAX_CONCURRENT`] if the count cannot be probed.
fn default_max_concurrent() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(FALLBACK_MAX_CONCURRENT)
        .clamp(MIN_MAX_CONCURRENT, MAX_MAX_CONCURRENT)
}

/// How an [`ExecPool`] is sized and how long it lets a caller wait.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PoolConfig {
    /// The maximum number of subprocesses permitted to be live at once.
    pub max_concurrent: usize,
    /// How long [`ExecPool::acquire`] waits for a slot before timing out.
    pub acquire_timeout: Duration,
}

impl PoolConfig {
    /// Build a config from explicit values, clamping `max_concurrent` into the
    /// safe `[1, 256]` range so a pool can never be constructed unusable.
    #[must_use]
    pub fn new(max_concurrent: usize, acquire_timeout: Duration) -> Self {
        Self {
            max_concurrent: max_concurrent.clamp(MIN_MAX_CONCURRENT, MAX_MAX_CONCURRENT),
            acquire_timeout,
        }
    }

    /// Resolve a config from the process environment (`TOURING_CEG_MAX_CONCURRENT`
    /// / `TOURING_CEG_ACQUIRE_TIMEOUT_SECS`), falling back to the CPU-derived
    /// default for any field unset or unparseable.
    #[must_use]
    pub fn from_env() -> Self {
        Self::resolve(
            env::var(ENV_MAX_CONCURRENT).ok(),
            env::var(ENV_ACQUIRE_TIMEOUT_SECS).ok(),
        )
    }

    /// The pure core of [`from_env`](Self::from_env): resolve a config from the
    /// *raw* env strings. Separated from the env read so the resolution logic
    /// is testable without mutating process-global state.
    ///
    /// A value that is absent, non-numeric, or zero falls back to the default
    /// for that field — a zero cap or zero timeout would be unusable.
    #[must_use]
    pub fn resolve(max_concurrent: Option<String>, acquire_timeout_secs: Option<String>) -> Self {
        let max = max_concurrent
            .and_then(|s| s.trim().parse::<usize>().ok())
            .filter(|n| *n > 0)
            .unwrap_or_else(default_max_concurrent);
        let acquire_timeout = acquire_timeout_secs
            .and_then(|s| s.trim().parse::<u64>().ok())
            .filter(|n| *n > 0)
            .map_or(DEFAULT_ACQUIRE_TIMEOUT, Duration::from_secs);
        Self::new(max, acquire_timeout)
    }
}

impl Default for PoolConfig {
    /// A CPU-derived cap and the default 60-second acquire timeout.
    fn default() -> Self {
        Self::new(default_max_concurrent(), DEFAULT_ACQUIRE_TIMEOUT)
    }
}

/// Why an [`ExecPool`] could not hand out a slot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PoolError {
    /// An [`acquire`](ExecPool::acquire) waited the full acquire timeout and no
    /// slot ever freed — the pool stayed saturated for the whole window.
    QueueTimeout {
        /// How long the caller waited before giving up.
        waited: Duration,
        /// The pool's concurrency cap, for the diagnostic.
        max_concurrent: usize,
    },
    /// A non-blocking [`try_acquire`](ExecPool::try_acquire) found every slot in
    /// use — the caller asked not to wait.
    Saturated {
        /// The pool's concurrency cap, for the diagnostic.
        max_concurrent: usize,
    },
}

impl PoolError {
    /// A short, stable kind label — for JSON output, logs and metrics.
    #[must_use]
    pub fn kind(&self) -> &'static str {
        match self {
            PoolError::QueueTimeout { .. } => "queue_timeout",
            PoolError::Saturated { .. } => "saturated",
        }
    }
}

impl fmt::Display for PoolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PoolError::QueueTimeout {
                waited,
                max_concurrent,
            } => write!(
                f,
                "exec pool saturated: waited {waited:?} for one of {max_concurrent} \
                 subprocess slots, none freed"
            ),
            PoolError::Saturated { max_concurrent } => write!(
                f,
                "exec pool saturated: all {max_concurrent} subprocess slots in use"
            ),
        }
    }
}

impl error::Error for PoolError {}

/// A snapshot of an [`ExecPool`]'s live state and lifetime counters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PoolStats {
    /// The pool's concurrency cap.
    pub max_concurrent: usize,
    /// Subprocesses currently permitted to be live (`max_concurrent - available`).
    pub in_flight: usize,
    /// Slots currently free.
    pub available: usize,
    /// Total slots ever handed out, across the pool's life.
    pub total_acquired: u64,
    /// Total [`acquire`](ExecPool::acquire) calls that ended in `QueueTimeout`.
    pub total_timed_out: u64,
    /// Total [`try_acquire`](ExecPool::try_acquire) calls rejected as `Saturated`.
    pub total_rejected: u64,
}

/// A bounded pool of subprocess-execution slots, with queueing backpressure.
///
/// See the [module documentation](self) for the rationale. Construct one with
/// [`new`](Self::new), or share the daemon-wide [`global`](Self::global)
/// instance. The pool is `Send + Sync` — wrap a custom pool in an `Arc` to
/// share it across tasks.
#[derive(Debug)]
pub struct ExecPool {
    /// The permit store — its capacity *is* the concurrency cap.
    sem: Arc<Semaphore>,
    /// Immutable sizing + timeout.
    config: PoolConfig,
    /// Lifetime count of slots handed out, by either acquire path.
    acquired: AtomicU64,
    /// Lifetime count of `acquire` calls that timed out under saturation.
    timed_out: AtomicU64,
    /// Lifetime count of `try_acquire` calls rejected under saturation.
    rejected: AtomicU64,
    /// **S-10 (OP4)** — transactional read/write-set lock manager. Guards
    /// concurrent harness-state mutations: a caller declares its access set via
    /// [`acquire_txn`](Self::acquire_txn) and the pool refuses two conflicting
    /// executions at once. Behind `txn_lock_enforcement` (default-on since S-10
    /// rollout, ES3 P1, 2026-06-01).
    #[cfg(feature = "txn_lock_enforcement")]
    txn: std::sync::Mutex<TxnLockManager>,
    /// Monotonic id source for transactional executions (S-10).
    #[cfg(feature = "txn_lock_enforcement")]
    next_txn_id: AtomicU64,
}

impl ExecPool {
    /// Build a pool sized by `config`.
    #[must_use]
    pub fn new(config: PoolConfig) -> Self {
        Self {
            sem: Arc::new(Semaphore::new(config.max_concurrent)),
            config,
            acquired: AtomicU64::new(0),
            timed_out: AtomicU64::new(0),
            rejected: AtomicU64::new(0),
            #[cfg(feature = "txn_lock_enforcement")]
            txn: std::sync::Mutex::new(TxnLockManager::new()),
            #[cfg(feature = "txn_lock_enforcement")]
            next_txn_id: AtomicU64::new(1),
        }
    }

    /// The process-global pool, configured from the environment on first use.
    ///
    /// Every gateway entry point shares this one instance, so the "never more
    /// than `N` subprocesses" guarantee holds across the whole daemon.
    #[must_use]
    pub fn global() -> &'static ExecPool {
        static GLOBAL: OnceLock<ExecPool> = OnceLock::new();
        GLOBAL.get_or_init(|| ExecPool::new(PoolConfig::from_env()))
    }

    /// The pool's concurrency cap — the most subprocesses it ever permits live.
    #[must_use]
    pub fn max_concurrent(&self) -> usize {
        self.config.max_concurrent
    }

    /// How long [`acquire`](Self::acquire) waits for a slot before timing out.
    #[must_use]
    pub fn acquire_timeout(&self) -> Duration {
        self.config.acquire_timeout
    }

    /// Slots currently free — a new execution could take one without waiting.
    #[must_use]
    pub fn available(&self) -> usize {
        self.sem.available_permits()
    }

    /// Subprocesses currently permitted to be live: `max_concurrent - available`.
    #[must_use]
    pub fn in_flight(&self) -> usize {
        self.config
            .max_concurrent
            .saturating_sub(self.sem.available_permits())
    }

    /// A snapshot of the pool's live state and lifetime counters.
    #[must_use]
    pub fn stats(&self) -> PoolStats {
        PoolStats {
            max_concurrent: self.config.max_concurrent,
            in_flight: self.in_flight(),
            available: self.available(),
            total_acquired: self.acquired.load(Ordering::Relaxed),
            total_timed_out: self.timed_out.load(Ordering::Relaxed),
            total_rejected: self.rejected.load(Ordering::Relaxed),
        }
    }

    /// Acquire one execution slot, queueing under backpressure.
    ///
    /// Hold the returned [`OwnedSemaphorePermit`] for the whole lifetime of the
    /// spawned subprocess — the slot is released when it drops, on every path
    /// including an early return or a panic. When every slot is busy this
    /// queues for up to [`acquire_timeout`](Self::acquire_timeout); if that
    /// whole window elapses with no slot freed it returns
    /// [`PoolError::QueueTimeout`].
    ///
    /// # Errors
    ///
    /// [`PoolError::QueueTimeout`] when the pool stayed saturated for the
    /// entire acquire-timeout window.
    pub async fn acquire(&self) -> Result<OwnedSemaphorePermit, PoolError> {
        match timeout(
            self.config.acquire_timeout,
            self.sem.clone().acquire_owned(),
        )
        .await
        {
            Ok(Ok(permit)) => {
                self.acquired.fetch_add(1, Ordering::Relaxed);
                Ok(permit)
            }
            // The semaphore is closed. `ExecPool` never closes it, so this arm
            // is unreachable in practice — treat a closed pool as permanently
            // saturated rather than panicking.
            Ok(Err(_)) => Err(PoolError::Saturated {
                max_concurrent: self.config.max_concurrent,
            }),
            Err(_) => {
                self.timed_out.fetch_add(1, Ordering::Relaxed);
                tracing::warn!(
                    max_concurrent = self.config.max_concurrent,
                    waited_secs = self.config.acquire_timeout.as_secs(),
                    "CEG exec pool saturated — acquire timed out, execution backpressured"
                );
                Err(PoolError::QueueTimeout {
                    waited: self.config.acquire_timeout,
                    max_concurrent: self.config.max_concurrent,
                })
            }
        }
    }

    /// Try to acquire one execution slot **without waiting**.
    ///
    /// Returns a permit immediately if a slot is free, or
    /// [`PoolError::Saturated`] at once if every slot is in use. Use this for a
    /// caller that must not block on backpressure; a caller that should wait
    /// uses [`acquire`](Self::acquire) instead.
    ///
    /// # Errors
    ///
    /// [`PoolError::Saturated`] when every slot is in use.
    pub fn try_acquire(&self) -> Result<OwnedSemaphorePermit, PoolError> {
        match self.sem.clone().try_acquire_owned() {
            Ok(permit) => {
                self.acquired.fetch_add(1, Ordering::Relaxed);
                Ok(permit)
            }
            Err(_) => {
                self.rejected.fetch_add(1, Ordering::Relaxed);
                Err(PoolError::Saturated {
                    max_concurrent: self.config.max_concurrent,
                })
            }
        }
    }
}

// ── S-10 (OP4) — transactional read/write-set conflict gate ─────────────────────

/// **S-10** — an RAII handle for a granted transactional lock on the pool.
///
/// Held for the duration of a concurrent harness-state mutation; the declared
/// access-set is released back to the pool's [`TxnLockManager`] when this drops,
/// on every path including a panic. Obtained from
/// [`ExecPool::acquire_txn`]. Behind `txn_lock_enforcement` (default-on since
/// ES3 P1 / S-10 rollout, 2026-06-01).
#[cfg(feature = "txn_lock_enforcement")]
#[derive(Debug)]
#[must_use = "the transactional lock releases when the TxnPermit drops; bind it for the mutation's lifetime"]
pub struct TxnPermit<'a> {
    pool: &'a ExecPool,
    id: u64,
}

#[cfg(feature = "txn_lock_enforcement")]
impl Drop for TxnPermit<'_> {
    fn drop(&mut self) {
        self.pool.release_txn(self.id);
    }
}

#[cfg(feature = "txn_lock_enforcement")]
impl TxnPermit<'_> {
    /// **ES3 P2 (2026-06-02)** — return the permit's execution id.
    ///
    /// Read-only accessor for the id minted by `next_txn_id` when the
    /// permit was acquired. Callers (e.g. `run_supervised_with_locks`)
    /// use it to stamp the `SupervisedOutcome` with the lock id so the
    /// X9 LEARN ledger can correlate an outcome with the transaction
    /// that guarded its I/O.
    #[must_use]
    pub fn id(&self) -> u64 {
        self.id
    }
}

#[cfg(feature = "txn_lock_enforcement")]
impl ExecPool {
    /// **S-10 (OP4)** — acquire a transactional lock for a declared access-set.
    ///
    /// Mints a fresh execution id, then asks the [`TxnLockManager`] whether the
    /// `decl` conflicts with any currently-held declaration (write-write or
    /// write-read hazard). On [`AcquireResult::Granted`] returns a [`TxnPermit`]
    /// RAII guard that releases on drop; on [`AcquireResult::Conflict`] returns
    /// the conflicting holder's id so the caller can back off and retry.
    /// Disjoint write-sets and pure read-sets never conflict — they parallelize.
    ///
    /// This is the conflict-awareness layer that complements the semaphore: the
    /// semaphore bounds *how many* run at once; this bounds *which* may run
    /// together. The two are independent — a caller coordinating a concurrent
    /// state mutation takes a [`acquire`](Self::acquire) permit (subprocess slot)
    /// **and** an `acquire_txn` permit (state-conflict lock).
    ///
    /// # Errors
    ///
    /// [`AcquireResult::Conflict`] (carrying the holder's id) when `decl`
    /// conflicts with a live declaration.
    pub fn acquire_txn(&self, decl: AccessDeclaration) -> Result<TxnPermit<'_>, AcquireResult> {
        let id = self.next_txn_id.fetch_add(1, Ordering::Relaxed);
        let mut mgr = self
            .txn
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        match mgr.try_acquire(id, decl) {
            AcquireResult::Granted => {
                drop(mgr);
                Ok(TxnPermit { pool: self, id })
            }
            conflict => Err(conflict),
        }
    }

    /// Release a transactional lock by id. Called from [`TxnPermit`]'s `Drop`;
    /// fail-open on a poisoned mutex (recovers the inner manager rather than
    /// panicking).
    fn release_txn(&self, id: u64) {
        let mut mgr = self
            .txn
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        mgr.release(id);
    }

    /// The number of transactional locks currently held — for observability and
    /// tests. Returns `0` on a poisoned mutex.
    #[must_use]
    pub fn active_txn_count(&self) -> usize {
        self.txn.lock().map(|m| m.active_count()).unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(feature = "txn_lock_enforcement")]
    use crate::gateway::txn::AccessPath;

    /// A small fixed config for deterministic tests.
    fn fixed(max: usize, timeout_ms: u64) -> PoolConfig {
        PoolConfig::new(max, Duration::from_millis(timeout_ms))
    }

    #[test]
    fn config_resolve_uses_defaults_when_env_absent() {
        let cfg = PoolConfig::resolve(None, None);
        assert!(
            (MIN_MAX_CONCURRENT..=MAX_MAX_CONCURRENT).contains(&cfg.max_concurrent),
            "a CPU-derived cap must land in the safe range"
        );
        assert_eq!(cfg.acquire_timeout, DEFAULT_ACQUIRE_TIMEOUT);
        // `resolve(None, None)` is exactly what `Default` yields.
        assert_eq!(cfg, PoolConfig::default());
    }

    #[test]
    fn config_resolve_honors_valid_env_values() {
        let cfg = PoolConfig::resolve(Some("12".to_owned()), Some("5".to_owned()));
        assert_eq!(cfg.max_concurrent, 12);
        assert_eq!(cfg.acquire_timeout, Duration::from_secs(5));
    }

    #[test]
    fn config_resolve_falls_back_on_garbage_or_zero() {
        // Non-numeric and zero both fall back to the field default — a zero
        // cap or zero timeout would make the pool unusable.
        let garbage = PoolConfig::resolve(Some("not-a-number".to_owned()), Some("0".to_owned()));
        assert!((MIN_MAX_CONCURRENT..=MAX_MAX_CONCURRENT).contains(&garbage.max_concurrent));
        assert_eq!(garbage.acquire_timeout, DEFAULT_ACQUIRE_TIMEOUT);
        let zero_cap = PoolConfig::resolve(Some("0".to_owned()), None);
        assert!(zero_cap.max_concurrent >= MIN_MAX_CONCURRENT);
    }

    #[test]
    fn config_new_clamps_into_the_safe_range() {
        assert_eq!(
            PoolConfig::new(99_999, DEFAULT_ACQUIRE_TIMEOUT).max_concurrent,
            MAX_MAX_CONCURRENT
        );
        assert_eq!(
            PoolConfig::new(0, DEFAULT_ACQUIRE_TIMEOUT).max_concurrent,
            MIN_MAX_CONCURRENT
        );
    }

    #[test]
    fn pool_error_kind_and_display_name_the_cause() {
        let queued = PoolError::QueueTimeout {
            waited: Duration::from_secs(60),
            max_concurrent: 8,
        };
        let saturated = PoolError::Saturated { max_concurrent: 8 };
        assert_eq!(queued.kind(), "queue_timeout");
        assert_eq!(saturated.kind(), "saturated");
        assert!(queued.to_string().contains('8'));
        assert!(!saturated.to_string().is_empty());
        // Usable as a boxed std error.
        let boxed: Box<dyn error::Error> = Box::new(queued);
        assert!(!boxed.to_string().is_empty());
    }

    #[test]
    fn try_acquire_bounds_concurrency_and_counts_rejections() {
        let pool = ExecPool::new(fixed(2, 1_000));
        let p1 = pool.try_acquire().expect("slot 1 is free");
        let _p2 = pool.try_acquire().expect("slot 2 is free");
        assert_eq!(pool.in_flight(), 2);
        // The third request finds no slot — rejected at once, not queued.
        let err = pool.try_acquire().expect_err("the pool is full");
        assert_eq!(err.kind(), "saturated");
        // Releasing one permit frees exactly one slot.
        drop(p1);
        assert_eq!(pool.in_flight(), 1);
        let _reopened = pool.try_acquire().expect("a slot reopened");
        assert_eq!(pool.stats().total_rejected, 1);
    }

    #[tokio::test]
    async fn acquire_hands_out_a_slot_and_drop_returns_it() {
        let pool = ExecPool::new(fixed(3, 1_000));
        assert_eq!(pool.in_flight(), 0);
        {
            let _permit = pool.acquire().await.expect("a free slot");
            assert_eq!(pool.in_flight(), 1);
            assert_eq!(pool.available(), 2);
        }
        // Permit dropped at end of scope — the slot is back.
        assert_eq!(pool.in_flight(), 0);
        assert_eq!(pool.stats().total_acquired, 1);
    }

    #[tokio::test]
    async fn acquire_times_out_under_sustained_saturation() {
        let pool = ExecPool::new(fixed(1, 40));
        assert_eq!(pool.acquire_timeout(), Duration::from_millis(40));
        let _held = pool.acquire().await.expect("the only slot");
        // The slot is never released — the second acquire must time out.
        let err = pool.acquire().await.expect_err("the pool stays saturated");
        assert!(
            matches!(
                err,
                PoolError::QueueTimeout {
                    max_concurrent: 1,
                    ..
                }
            ),
            "expected QueueTimeout for a cap of 1, got {err:?}"
        );
        assert_eq!(pool.stats().total_timed_out, 1);
    }

    #[tokio::test]
    async fn acquire_queues_then_proceeds_when_a_slot_frees() {
        // Backpressure must *queue and proceed*, not just reject: a waiter gets
        // the slot as soon as the holder releases it, within the timeout.
        let pool = Arc::new(ExecPool::new(fixed(1, 5_000)));
        let held = pool.acquire().await.expect("the only slot");
        let waiter = {
            let pool = Arc::clone(&pool);
            tokio::spawn(async move { pool.acquire().await.map(|_permit| ()) })
        };
        // Let the waiter park on the semaphore, then release the slot.
        tokio::time::sleep(Duration::from_millis(30)).await;
        drop(held);
        waiter
            .await
            .expect("the waiter task joins")
            .expect("the freed slot is handed to the queued waiter");
        assert_eq!(pool.stats().total_acquired, 2);
        assert_eq!(pool.stats().total_timed_out, 0);
    }

    #[test]
    fn global_pool_is_a_stable_singleton() {
        let a = ExecPool::global();
        let b = ExecPool::global();
        assert!(std::ptr::eq(a, b), "global() must return one shared pool");
        assert!(a.max_concurrent() >= MIN_MAX_CONCURRENT);
    }

    #[test]
    fn fresh_pool_stats_are_zeroed_and_consistent() {
        let pool = ExecPool::new(fixed(4, 1_000));
        let stats = pool.stats();
        assert_eq!(stats.max_concurrent, 4);
        assert_eq!(stats.in_flight, 0);
        assert_eq!(stats.available, 4);
        assert_eq!(stats.total_acquired, 0);
        assert_eq!(stats.total_timed_out, 0);
        assert_eq!(stats.total_rejected, 0);
    }

    // ── S-10 (OP4) — transactional conflict gate (feature-gated) ────────────

    #[cfg(feature = "txn_lock_enforcement")]
    #[test]
    fn txn_disjoint_writes_parallelize() {
        let pool = ExecPool::new(fixed(4, 1_000));
        let _a = pool
            .acquire_txn(AccessDeclaration::new().writing(AccessPath::Path("a.rs".into())))
            .expect("first write is granted");
        // A disjoint write-set must NOT conflict — both run concurrently.
        let _b = pool
            .acquire_txn(AccessDeclaration::new().writing(AccessPath::Path("b.rs".into())))
            .expect("a disjoint write parallelizes");
        assert_eq!(pool.active_txn_count(), 2);
    }

    #[cfg(feature = "txn_lock_enforcement")]
    #[test]
    fn txn_conflicting_write_is_refused() {
        let pool = ExecPool::new(fixed(4, 1_000));
        let _held = pool
            .acquire_txn(AccessDeclaration::new().writing(AccessPath::CrdtNode(7)))
            .expect("first holder granted");
        // A second write to the same resource is a write-write hazard.
        let conflict = pool.acquire_txn(AccessDeclaration::new().writing(AccessPath::CrdtNode(7)));
        assert!(
            matches!(conflict, Err(AcquireResult::Conflict(_))),
            "a write-write hazard on the same resource must be refused: {conflict:?}"
        );
        assert_eq!(
            pool.active_txn_count(),
            1,
            "the refused acquire holds nothing"
        );
    }

    #[cfg(feature = "txn_lock_enforcement")]
    #[test]
    fn txn_permit_releases_on_drop() {
        let pool = ExecPool::new(fixed(4, 1_000));
        {
            let _held = pool
                .acquire_txn(AccessDeclaration::new().writing(AccessPath::OutcomeKey("k".into())))
                .expect("granted");
            assert_eq!(pool.active_txn_count(), 1);
        } // permit drops here → RAII release.
        assert_eq!(pool.active_txn_count(), 0, "drop must release the lock");
        // The resource is now free, so the same declaration is grantable again.
        let _reacquired = pool
            .acquire_txn(AccessDeclaration::new().writing(AccessPath::OutcomeKey("k".into())))
            .expect("released resource is re-grantable");
    }
}
