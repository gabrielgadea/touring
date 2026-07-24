//! Atomic counters for memory pressure and core scheduling.
//!
//! Mirrors the `touring-hooks::shared::gate_metrics` pattern: lazy-init
//! `OnceLock<TrmMetrics>` singleton with `AtomicU64` fields and free
//! `record_*()` functions for fire-and-forget telemetry.
//!
//! Counters are observed via [`capture`] which returns a serializable
//! [`MetricsSnapshot`] for `touring status -j` integration.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;

use crate::sentinel::memory::Pressure;

static METRICS: OnceLock<TrmMetrics> = OnceLock::new();

/// Singleton metrics container with 8 atomic counters.
#[derive(Debug)]
pub struct TrmMetrics {
    /// Number of pressure ticks classified Green.
    pub memory_pressure_green_count: AtomicU64,
    /// Number of pressure ticks classified Yellow.
    pub memory_pressure_yellow_count: AtomicU64,
    /// Number of pressure ticks classified Red.
    pub memory_pressure_red_count: AtomicU64,
    /// Total pressure ticks observed.
    pub memory_pressure_total_tick_count: AtomicU64,
    /// Times swap thrashing was detected (`pgmajfault` delta > threshold).
    pub swap_thrashing_detected_count: AtomicU64,
    /// Times a `cargo test`-class command was paused via pre_bash gate.
    pub cargo_test_paused_count: AtomicU64,
    /// Times a thread was pinned to P-cores.
    pub core_pinning_p_count: AtomicU64,
    /// Times a thread was pinned to E-cores.
    pub core_pinning_e_count: AtomicU64,
}

impl TrmMetrics {
    const fn new() -> Self {
        Self {
            memory_pressure_green_count: AtomicU64::new(0),
            memory_pressure_yellow_count: AtomicU64::new(0),
            memory_pressure_red_count: AtomicU64::new(0),
            memory_pressure_total_tick_count: AtomicU64::new(0),
            swap_thrashing_detected_count: AtomicU64::new(0),
            cargo_test_paused_count: AtomicU64::new(0),
            core_pinning_p_count: AtomicU64::new(0),
            core_pinning_e_count: AtomicU64::new(0),
        }
    }
}

/// Access the global metrics singleton, lazily initialized.
pub fn global() -> &'static TrmMetrics {
    METRICS.get_or_init(TrmMetrics::new)
}

/// Record one pressure tick. Increments both the total counter and the
/// tier-specific counter atomically (best-effort — not transactional).
pub fn record_pressure_tick(pressure: Pressure) {
    let m = global();
    m.memory_pressure_total_tick_count
        .fetch_add(1, Ordering::Relaxed);
    let tier = match pressure {
        Pressure::Green => &m.memory_pressure_green_count,
        Pressure::Yellow => &m.memory_pressure_yellow_count,
        Pressure::Red => &m.memory_pressure_red_count,
    };
    tier.fetch_add(1, Ordering::Relaxed);
}

/// Record a swap thrashing detection (pgmajfault delta exceeded threshold).
pub fn record_swap_thrashing() {
    global()
        .swap_thrashing_detected_count
        .fetch_add(1, Ordering::Relaxed);
}

/// Record that a heavy command (cargo test/build, etc.) was paused.
pub fn record_cargo_test_paused() {
    global()
        .cargo_test_paused_count
        .fetch_add(1, Ordering::Relaxed);
}

/// Record a P-core pinning operation.
pub fn record_core_pin_p() {
    global()
        .core_pinning_p_count
        .fetch_add(1, Ordering::Relaxed);
}

/// Record an E-core pinning operation.
pub fn record_core_pin_e() {
    global()
        .core_pinning_e_count
        .fetch_add(1, Ordering::Relaxed);
}

/// Snapshot of all counters at a point in time. Suitable for JSON export.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MetricsSnapshot {
    /// Pressure tier = `Green` ticks observed. Counter
    /// `pressure_green_count` in `gate-metrics`.
    pub memory_pressure_green_count: u64,
    /// Pressure tier = `Yellow` ticks observed. Counter
    /// `pressure_yellow_count`.
    pub memory_pressure_yellow_count: u64,
    /// Pressure tier = `Red` ticks observed. Counter
    /// `pressure_red_count` — these are the dangerous ones.
    pub memory_pressure_red_count: u64,
    /// Total number of pressure polling ticks (sum of all
    /// tiers).
    pub memory_pressure_total_tick_count: u64,
    /// Number of times the swap-thrashing heuristic fired.
    pub swap_thrashing_detected_count: u64,
    /// Number of times a `cargo test` invocation was paused
    /// because of Red pressure.
    pub cargo_test_paused_count: u64,
    /// P-core pinning operations (libc::sched_setaffinity calls
    /// targeting performance cores).
    pub core_pinning_p_count: u64,
    /// E-core pinning operations (pinning to efficiency cores).
    pub core_pinning_e_count: u64,
}

/// Capture all counters into a serializable snapshot.
pub fn capture() -> MetricsSnapshot {
    let m = global();
    MetricsSnapshot {
        memory_pressure_green_count: m.memory_pressure_green_count.load(Ordering::Relaxed),
        memory_pressure_yellow_count: m.memory_pressure_yellow_count.load(Ordering::Relaxed),
        memory_pressure_red_count: m.memory_pressure_red_count.load(Ordering::Relaxed),
        memory_pressure_total_tick_count: m
            .memory_pressure_total_tick_count
            .load(Ordering::Relaxed),
        swap_thrashing_detected_count: m.swap_thrashing_detected_count.load(Ordering::Relaxed),
        cargo_test_paused_count: m.cargo_test_paused_count.load(Ordering::Relaxed),
        core_pinning_p_count: m.core_pinning_p_count.load(Ordering::Relaxed),
        core_pinning_e_count: m.core_pinning_e_count.load(Ordering::Relaxed),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_pressure_increments_total_and_tier() {
        // Note: global state is shared across tests. Capture deltas only.
        let before = capture();
        record_pressure_tick(Pressure::Green);
        record_pressure_tick(Pressure::Yellow);
        record_pressure_tick(Pressure::Red);
        let after = capture();
        assert_eq!(
            after.memory_pressure_total_tick_count - before.memory_pressure_total_tick_count,
            3
        );
        assert!(
            after.memory_pressure_green_count >= before.memory_pressure_green_count + 1
                && after.memory_pressure_yellow_count >= before.memory_pressure_yellow_count + 1
                && after.memory_pressure_red_count >= before.memory_pressure_red_count + 1
        );
    }

    #[test]
    fn record_helpers_increment_their_counters() {
        let before = capture();
        record_swap_thrashing();
        record_cargo_test_paused();
        record_core_pin_p();
        record_core_pin_e();
        let after = capture();
        assert!(
            after.swap_thrashing_detected_count >= before.swap_thrashing_detected_count + 1
                && after.cargo_test_paused_count >= before.cargo_test_paused_count + 1
                && after.core_pinning_p_count >= before.core_pinning_p_count + 1
                && after.core_pinning_e_count >= before.core_pinning_e_count + 1
        );
    }

    #[test]
    fn snapshot_is_serializable() {
        let snap = capture();
        let json = serde_json::to_string(&snap).expect("serialize");
        assert!(json.contains("memory_pressure_total_tick_count"));
    }
}
