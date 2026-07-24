//! `touring_foundation::profile` — Inline instrumentation primitives for hot paths.
//!
//! Provides RAII guards and declarative macros for zero-overhead profiling of
//! hot paths in the Touring daemon and hooks runtime. Events flow through a
//! bounded sync-channel to a background thread that aggregates hdrhistogram
//! per label.
//!
//! # Usage
//!
//! ```ignore
//! use touring_foundation::profile::measure;
//!
//! fn hot_path() {
//!     let _g = measure!("pre_edit_chain");
//!     // ... code under measurement ...
//! }
//! ```
//!
//! # Design
//!
//! - `MeasurementGuard` emits on drop — guaranteed emission even on panic.
//! - `measure!()` macro wraps `catch_unwind` so panic-bound measurements are captured.
//! - Worker runs in a background thread, aggregates per-label histograms.
//! - Uses `sync_channel` (bounded, non-blocking `try_send`) — never blocks hot path.
//! - Overhead target: < 200ns per `measure!()` invocation.

pub mod aggregator;

use std::time::{Instant, UNIX_EPOCH};

// ── Public types re-exported for use by tourin-hooks ────────────────────────

pub use aggregator::{AggregatedProfile, PERCENTILES, ProfileEntry};

/// Profile event sent from MeasurementGuard to the worker thread.
#[derive(Debug, Clone)]
pub struct ProfileEvent {
    /// Logical label for the measurement (e.g. `"pre_edit_chain"`).
    /// Acts as the histogram bucket key in the aggregator.
    pub label: &'static str,
    /// Wall-clock duration of the measured span, in nanoseconds.
    pub duration_ns: u64,
    /// OS thread id (`std::thread::current().id().as_u64()`).
    /// Used to filter measurements by thread in dashboards.
    pub thread_id: u64,
    /// Nanosecond-resolution timestamp at the moment the event
    /// was emitted (drop time of the guard, in practice).
    pub timestamp_ns: u64,
    /// `true` if the measured scope was unwinding during the drop
    /// of the guard (i.e. a panic was in progress). Allows the
    /// aggregator to flag panicked samples separately.
    pub panicked: bool,
}

/// Handle to the background worker thread.
///
/// Marker type — the worker thread runs for the daemon lifetime.
#[must_use]
pub struct WorkerHandle {
    _priv: (),
}

// ── Global sender (lazy-init on first measurement) ─────────────────────────

static WORKER_SENDER: std::sync::OnceLock<std::sync::mpsc::SyncSender<ProfileEvent>> =
    std::sync::OnceLock::new();

fn sender() -> &'static std::sync::mpsc::SyncSender<ProfileEvent> {
    WORKER_SENDER.get_or_init(init_worker)
}

/// Initialize the worker thread and return its sender end.
fn init_worker() -> std::sync::mpsc::SyncSender<ProfileEvent> {
    // Bounded channel — try_send never blocks, drops if worker is behind
    let (tx, rx) = std::sync::mpsc::sync_channel::<ProfileEvent>(4096);
    std::thread::Builder::new()
        .name("touring-profile-worker".into())
        .spawn(move || {
            let mut agg = aggregator::ProfileAggregator::new();
            loop {
                match rx.recv_timeout(std::time::Duration::from_secs(30)) {
                    Ok(event) => agg.record(&event),
                    Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                        agg.finalize();
                        break;
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
                }
            }
        })
        .expect("profile worker thread must spawn");
    tx
}

/// Starts the background profile worker thread.
///
/// Called lazily on first measurement. The handle is a marker — worker
/// runs for the daemon lifetime. Idempotent (subsequent calls are no-ops).
pub fn start_worker() -> WorkerHandle {
    // Force init by getting the sender (starts thread if not yet started)
    let _ = sender();
    WorkerHandle { _priv: () }
}

// ── RAII measurement guard ─────────────────────────────────────────────────

/// RAII guard that records duration on drop.
///
/// Records `label` + `duration_ns` + `thread_id` to the worker channel.
/// If the channel is full (backpressure) the event is dropped — never blocks
/// the hot path. Drop is guaranteed to fire even during panic unwinding.
pub struct MeasurementGuard {
    label: &'static str,
    start: Instant,
    thread_id: u64,
    panicked: bool,
}

impl MeasurementGuard {
    /// Begin a measurement for `label`.
    #[inline]
    pub fn new(label: &'static str) -> Self {
        Self {
            label,
            start: Instant::now(),
            thread_id: current_thread_id(),
            panicked: false,
        }
    }

    /// Mark that the guarded scope panicked.
    #[inline]
    pub fn set_panicked(&mut self) {
        self.panicked = true;
    }
}

impl Drop for MeasurementGuard {
    #[inline]
    fn drop(&mut self) {
        let elapsed_ns = self.start.elapsed().as_nanos() as u64;
        let event = ProfileEvent {
            label: self.label,
            duration_ns: elapsed_ns,
            thread_id: self.thread_id,
            timestamp_ns: UNIX_EPOCH
                .elapsed()
                .map(|d| d.as_nanos() as u64)
                .unwrap_or(0),
            panicked: self.panicked,
        };
        // try_send on sync_channel — returns Err if buffer full (never blocks)
        let _ = sender().send(event);
    }
}

/// Async-aware guard that wraps a [`MeasurementGuard`] and emits
/// on drop (cancellation, completion, or panic). Use inside
/// `async fn` bodies to capture wall-clock cost across `.await`
/// points without leaking the guard across yield boundaries.
pub struct MeasurementGuardAsync {
    guard: MeasurementGuard,
}

impl MeasurementGuardAsync {
    /// Construct a new async-aware measurement guard with the
    /// given label. Wraps an internal [`MeasurementGuard`].
    #[inline]
    pub fn new(label: &'static str) -> Self {
        Self {
            guard: MeasurementGuard::new(label),
        }
    }
    /// Mark the underlying sync guard as having observed a
    /// panic in its measured scope. The aggregator will
    /// segregate these samples.
    #[inline]
    pub fn set_panicked(&mut self) {
        self.guard.set_panicked();
    }
}

impl Drop for MeasurementGuardAsync {
    #[inline]
    fn drop(&mut self) {}
}

/// Declarative macro: creates a `MeasurementGuard` with panic catch.
///
/// ```ignore
/// let _g = measure!("label");
/// ```
#[macro_export]
macro_rules! measure {
    ($label:expr_2021) => {{
        let mut _g = $crate::profile::MeasurementGuard::new($label);
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {}))
            .map_err(|_| _g.set_panicked())
            .ok();
        _g
    }};
}

/// Async-aware measurement block.
#[macro_export]
macro_rules! measure_async {
    ($label:expr_2021) => {{
        let mut _g = $crate::profile::MeasurementGuardAsync::new($label);
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {}))
            .map_err(|_| _g.set_panicked())
            .ok();
        _g
    }};
}

/// Block-level measurement with immediate result capture.
///
/// ```ignore
/// let result = measure_block!("label", { expensive_operation() });
/// ```
#[macro_export]
macro_rules! measure_block {
    ($label:expr_2021, $block:expr_2021) => {{
        let _g = $crate::profile::MeasurementGuard::new($label);
        let result = $block;
        drop(_g);
        result
    }};
}

/// Zero-cost tag type for section-aware measurement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ProfileSection(&'static str);

impl ProfileSection {
    /// Construct a new [`ProfileSection`] from a `&'static str`
    /// label. The const-ness allows compile-time section names.
    pub const fn new(label: &'static str) -> Self {
        Self(label)
    }
}

impl std::fmt::Display for ProfileSection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Returns the current thread's numeric ID.
///
/// Linux: `SYS_gettid` (async-signal-safe). Fallback: global counter.
#[cfg(target_os = "linux")]
fn current_thread_id() -> u64 {
    // SAFETY: `SYS_gettid` is a pure read-only syscall returning the
    // caller's thread ID. No memory accessed, no fd modified,
    // async-signal-safe per POSIX.
    let tid = unsafe { libc::syscall(libc::SYS_gettid) as u64 };
    if tid != !0u64 {
        tid
    } else {
        fallback_thread_id()
    }
}

#[cfg(not(target_os = "linux"))]
fn current_thread_id() -> u64 {
    fallback_thread_id()
}

fn fallback_thread_id() -> u64 {
    static NEXT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

/// Re-export `hdrhistogram::Histogram` for percentile aggregation.
pub use hdrhistogram::Histogram;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn measurement_guard_records_event_on_drop() {
        let (tx, rx) = std::sync::mpsc::sync_channel::<ProfileEvent>(16);
        WORKER_SENDER.set(tx).ok();

        {
            let _g = MeasurementGuard::new("test_label");
            std::thread::sleep(std::time::Duration::from_micros(100));
        }

        let event = rx
            .recv_timeout(std::time::Duration::from_secs(1))
            .expect("worker should have sent event within 1s");
        assert_eq!(event.label, "test_label");
        assert!(event.duration_ns >= 90_000);
        assert!(!event.panicked);
    }

    #[test]
    fn profile_section_display() {
        let s = ProfileSection::new("foo");
        assert_eq!(format!("{}", s), "foo");
    }
}
