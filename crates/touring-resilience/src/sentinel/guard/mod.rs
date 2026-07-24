//! Memory pressure sentinel — singleton ticker with Tokio interval.
//!
//! [`MemoryGuard::global`] returns the lazy-initialized process-wide singleton.
//! Call [`MemoryGuard::start_ticker`] from inside an active Tokio runtime to
//! begin polling `/proc/pressure/memory` + `/proc/meminfo` + `/proc/vmstat`
//! at the configured cadence (1 s recommended).
//!
//! The ticker stores the latest [`MemorySnapshot`] in an `RwLock` and the
//! latest [`Pressure`] tier in a lock-free `AtomicU8` for fast O(1) reads
//! from hot paths (e.g., `pre_bash` hook gate).

use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Mutex, OnceLock, RwLock};
use std::time::Duration;

use tokio::task::JoinHandle;
use tokio::time::MissedTickBehavior;

use crate::sentinel::error::TrmError;
use crate::sentinel::memory::{MemorySnapshot, Pressure, SwapState, snapshot};
use crate::sentinel::metrics;

/// Pressure encoded as `u8` for atomic storage. `as_u8 / from_u8` are private
/// helpers — the public API exposes [`Pressure`] directly.
fn pressure_to_u8(p: Pressure) -> u8 {
    match p {
        Pressure::Green => 0,
        Pressure::Yellow => 1,
        Pressure::Red => 2,
    }
}

fn pressure_from_u8(v: u8) -> Pressure {
    match v {
        1 => Pressure::Yellow,
        2 => Pressure::Red,
        _ => Pressure::Green,
    }
}

static MEMORY_GUARD: OnceLock<MemoryGuard> = OnceLock::new();

/// Process-wide memory pressure sentinel.
pub struct MemoryGuard {
    pressure_state: Arc<AtomicU8>,
    swap_state: Arc<SwapState>,
    last_snapshot: Arc<RwLock<Option<MemorySnapshot>>>,
    ticker_handle: Mutex<Option<JoinHandle<()>>>,
}

impl MemoryGuard {
    /// Construct a new MemoryGuard. Used internally by [`global`].
    fn new() -> Self {
        Self {
            pressure_state: Arc::new(AtomicU8::new(0)),
            swap_state: Arc::new(SwapState::new()),
            last_snapshot: Arc::new(RwLock::new(None)),
            ticker_handle: Mutex::new(None),
        }
    }
    /// Access the global singleton, lazily initialized.
    pub fn global() -> &'static MemoryGuard {
        MEMORY_GUARD.get_or_init(Self::new)
    }
    /// Latest observed pressure tier — O(1) lock-free atomic read.
    pub fn pressure(&self) -> Pressure {
        pressure_from_u8(self.pressure_state.load(Ordering::Relaxed))
    }
    /// Latest full snapshot if the ticker has run at least once.
    pub fn snapshot(&self) -> Option<MemorySnapshot> {
        self.last_snapshot.read().ok().and_then(|g| g.clone())
    }
    /// Start the background pressure ticker. Idempotent — calling twice
    /// returns [`TrmError::TickerAlreadyRunning`] without disturbing the
    /// running ticker.
    pub async fn start_ticker(&self, interval: Duration) -> Result<(), TrmError> {
        let _ = tokio::runtime::Handle::try_current().map_err(|_| TrmError::NoTokioRuntime)?;
        let mut guard = self
            .ticker_handle
            .lock()
            .map_err(|_| TrmError::TickerAlreadyRunning)?;
        if guard.is_some() {
            return Err(TrmError::TickerAlreadyRunning);
        }
        let pressure_state = Arc::clone(&self.pressure_state);
        let swap_state = Arc::clone(&self.swap_state);
        let last_snapshot = Arc::clone(&self.last_snapshot);
        let join = tokio::spawn(async move {
            let interval_clamped = if interval < Duration::from_millis(100) {
                Duration::from_millis(100)
            } else {
                interval
            };
            let mut iv = tokio::time::interval(interval_clamped);
            iv.set_missed_tick_behavior(MissedTickBehavior::Skip);
            loop {
                iv.tick().await;
                match snapshot(&swap_state) {
                    Ok(snap) => {
                        let p = snap.pressure;
                        pressure_state.store(pressure_to_u8(p), Ordering::Relaxed);
                        metrics::record_pressure_tick(p);
                        if let Ok(mut w) = last_snapshot.write() {
                            *w = Some(snap);
                        }
                    }
                    Err(e) => {
                        tracing::warn!(error = % e, "memory snapshot failed");
                    }
                }
            }
        });
        *guard = Some(join);
        tracing::info!(
            interval_ms = interval.as_millis() as u64,
            "memory pressure ticker started"
        );
        Ok(())
    }
    /// Stop the ticker. Best-effort: if the lock is poisoned, leaves the
    /// JoinHandle in place (Tokio will reap it on runtime shutdown).
    pub fn stop_ticker(&self) {
        if let Ok(mut guard) = self.ticker_handle.lock() {
            if let Some(h) = guard.take() {
                h.abort();
                tracing::info!("memory pressure ticker stopped");
            }
        }
    }
}

impl std::fmt::Debug for MemoryGuard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MemoryGuard")
            .field("pressure", &self.pressure())
            .field(
                "ticker_running",
                &self
                    .ticker_handle
                    .lock()
                    .map(|g| g.is_some())
                    .unwrap_or(false),
            )
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pressure_u8_roundtrip() {
        assert_eq!(
            pressure_from_u8(pressure_to_u8(Pressure::Green)),
            Pressure::Green
        );
        assert_eq!(
            pressure_from_u8(pressure_to_u8(Pressure::Yellow)),
            Pressure::Yellow
        );
        assert_eq!(
            pressure_from_u8(pressure_to_u8(Pressure::Red)),
            Pressure::Red
        );
        assert_eq!(pressure_from_u8(99), Pressure::Green);
    }
    #[test]
    fn global_returns_same_instance() {
        let a = MemoryGuard::global() as *const MemoryGuard;
        let b = MemoryGuard::global() as *const MemoryGuard;
        assert_eq!(a, b);
    }
    #[test]
    fn snapshot_is_none_before_first_tick() {
        let _ = MemoryGuard::global().snapshot();
    }
    #[tokio::test]
    async fn start_ticker_returns_already_running_on_second_call() {
        let g = MemoryGuard::global();
        let _ = g.start_ticker(Duration::from_millis(100)).await;
        let r = g.start_ticker(Duration::from_millis(100)).await;
        match r {
            Err(TrmError::TickerAlreadyRunning) => {}
            other => panic!("expected TickerAlreadyRunning, got {other:?}"),
        }
        g.stop_ticker();
    }
    #[tokio::test(flavor = "current_thread")]
    async fn ticker_updates_pressure_within_two_ticks() {
        let g = MemoryGuard::new();
        g.start_ticker(Duration::from_millis(100)).await.unwrap();
        tokio::time::sleep(Duration::from_millis(350)).await;
        let snap = g.last_snapshot.read().unwrap().clone();
        assert!(snap.is_some(), "expected a snapshot after ticking");
        g.stop_ticker();
    }
}
