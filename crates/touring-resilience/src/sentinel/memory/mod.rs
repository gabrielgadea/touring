//! Memory pressure subsystem.
//!
//! Aggregates three `/proc` data sources into a single [`MemorySnapshot`]:
//!
//! | Source | File | Mandatory |
//! |--------|------|-----------|
//! | Available/swap | `/proc/meminfo` | Yes |
//! | PSI stall | `/proc/pressure/memory` | No (falls back to 0.0) |
//! | Major faults | `/proc/vmstat` | No (falls back to 0) |
//!
//! Call [`snapshot`] periodically (e.g., every 1 s) passing a shared
//! [`swap::SwapState`] to track per-tick `pgmajfault` deltas.
//!
//! # Pressure tiers
//!
//! [`classify_pressure`] maps snapshot metrics to one of three [`Pressure`]
//! tiers using the thresholds defined as public constants in this module.

pub mod meminfo;
pub mod psi;
pub mod swap;

pub use meminfo::{MeminfoReading, parse_meminfo, read_meminfo};
pub use psi::{PsiReading, parse_psi, read_psi};
pub use swap::{SwapState, VmstatReading, parse_vmstat, read_vmstat};

use std::time::{SystemTime, UNIX_EPOCH};

use crate::sentinel::error::PressureReadError;

// ── Pressure thresholds ──────────────────────────────────────────────────────

/// PSI `some avg10` (%) at or above which pressure is **Yellow**.
pub const YELLOW_PSI_AVG10_PCT: f32 = 10.0;
/// PSI `some avg10` (%) at or above which pressure is **Red**.
pub const RED_PSI_AVG10_PCT: f32 = 40.0;

/// Available memory (MB) below which pressure is **Yellow**.
pub const YELLOW_AVAILABLE_MB_BELOW: u64 = 4096;
/// Available memory (MB) below which pressure is **Red**.
pub const RED_AVAILABLE_MB_BELOW: u64 = 2048;

/// Swap used (%) above which pressure is **Yellow**.
pub const YELLOW_SWAP_USED_PCT_ABOVE: f64 = 20.0;
/// Swap used (%) above which pressure is **Red**.
pub const RED_SWAP_USED_PCT_ABOVE: f64 = 60.0;

// ── Pressure enum ────────────────────────────────────────────────────────────

/// Memory pressure tier emitted by [`classify_pressure`].
///
/// Consumers (e.g., `MemoryGuard`) map tiers to action policies:
/// - [`Green`][Pressure::Green] — normal operation
/// - [`Yellow`][Pressure::Yellow] — shed non-critical work
/// - [`Red`][Pressure::Red] — emergency mitigation (OOM imminent)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Pressure {
    /// System memory is healthy; no action required.
    Green,
    /// Memory pressure is elevated; non-critical work should be deferred.
    Yellow,
    /// Memory pressure is critical; emergency action is required.
    Red,
}

// ── MemorySnapshot ───────────────────────────────────────────────────────────

/// Point-in-time snapshot of system memory health.
///
/// Constructed by [`snapshot`]; size values are in **MB** except
/// `pgmajfault` (cumulative counter) and `swap_used_pct` (percent 0–100).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MemorySnapshot {
    /// Total physical RAM in MB.
    pub total_mb: u64,
    /// Estimated available RAM in MB (`MemAvailable` from `/proc/meminfo`).
    pub available_mb: u64,
    /// Used RAM in MB (`total - available`).
    pub used_mb: u64,
    /// Total swap space in MB.
    pub swap_total_mb: u64,
    /// Used swap space in MB.
    pub swap_used_mb: u64,
    /// Swap utilisation as a percentage (0.0–100.0).
    pub swap_used_pct: f64,
    /// PSI `some` stall average over the last 10 s (percent).
    pub psi_some_avg10: f32,
    /// PSI `some` stall average over the last 60 s (percent).
    pub psi_some_avg60: f32,
    /// PSI `full` stall average over the last 10 s (percent).
    pub psi_full_avg10: f32,
    /// Cumulative major page faults since boot (`pgmajfault` from `/proc/vmstat`).
    pub pgmajfault: u64,
    /// Derived [`Pressure`] tier for this snapshot.
    pub pressure: Pressure,
    /// Wall-clock capture time as milliseconds since the Unix epoch.
    pub captured_at_unix_ms: u64,
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Classify a snapshot into a [`Pressure`] tier.
///
/// Red checks are evaluated first (most severe). A single Red condition
/// is sufficient to return [`Pressure::Red`] regardless of other metrics.
pub fn classify_pressure(snap: &MemorySnapshot) -> Pressure {
    if is_red(snap) {
        return Pressure::Red;
    }
    if is_yellow(snap) {
        return Pressure::Yellow;
    }
    Pressure::Green
}

/// Read all sources and assemble a [`MemorySnapshot`].
///
/// `/proc/meminfo` is mandatory — its failure propagates as an error.
/// PSI and vmstat failures are non-fatal: fields fall back to `0`/`0.0`
/// with a `tracing::warn` log.
pub fn snapshot(swap_state: &swap::SwapState) -> Result<MemorySnapshot, PressureReadError> {
    let mem = read_meminfo()?;
    let (psi_some_avg10, psi_some_avg60, psi_full_avg10) = match read_psi() {
        Ok(p) => (p.some_avg10, p.some_avg60, p.full_avg10),
        Err(e) => {
            tracing::warn!(error = % e, "PSI unavailable; using 0.0 for all PSI fields");
            (0.0, 0.0, 0.0)
        }
    };
    let pgmajfault = match read_vmstat() {
        Ok(v) => v.pgmajfault,
        Err(e) => {
            tracing::warn!(error = % e, "vmstat unavailable; using 0 for pgmajfault");
            0
        }
    };
    let _ = swap_state.observe(pgmajfault);
    let total_mb = mem.total_kb / 1024;
    let available_mb = mem.available_kb / 1024;
    let used_mb = total_mb.saturating_sub(available_mb);
    let swap_total_mb = mem.swap_total_kb / 1024;
    let swap_used_mb = swap_total_mb.saturating_sub(mem.swap_free_kb / 1024);
    let swap_used_pct = compute_swap_pct(mem.swap_total_kb, mem.swap_free_kb);
    let captured_at_unix_ms = unix_ms_now();
    let mut snap = MemorySnapshot {
        total_mb,
        available_mb,
        used_mb,
        swap_total_mb,
        swap_used_mb,
        swap_used_pct,
        psi_some_avg10,
        psi_some_avg60,
        psi_full_avg10,
        pgmajfault,
        pressure: Pressure::Green,
        captured_at_unix_ms,
    };
    snap.pressure = classify_pressure(&snap);
    Ok(snap)
}

// ── Internal helpers ────────────────────────────────────────────────────────

fn is_red(snap: &MemorySnapshot) -> bool {
    snap.psi_some_avg10 >= RED_PSI_AVG10_PCT
        || snap.available_mb < RED_AVAILABLE_MB_BELOW
        || snap.swap_used_pct >= RED_SWAP_USED_PCT_ABOVE
}

fn is_yellow(snap: &MemorySnapshot) -> bool {
    snap.psi_some_avg10 >= YELLOW_PSI_AVG10_PCT
        || snap.available_mb < YELLOW_AVAILABLE_MB_BELOW
        || snap.swap_used_pct >= YELLOW_SWAP_USED_PCT_ABOVE
}

/// Compute swap utilisation percentage, guarding against division by zero.
fn compute_swap_pct(swap_total_kb: u64, swap_free_kb: u64) -> f64 {
    if swap_total_kb == 0 {
        return 0.0;
    }
    let used = swap_total_kb.saturating_sub(swap_free_kb) as f64;
    (used / swap_total_kb as f64) * 100.0
}

/// Current time as milliseconds since the Unix epoch.
fn unix_ms_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn make_snap(available_mb: u64, swap_used_pct: f64, psi_some_avg10: f32) -> MemorySnapshot {
        MemorySnapshot {
            total_mb: 64_000,
            available_mb,
            used_mb: 64_000u64.saturating_sub(available_mb),
            swap_total_mb: 20_000,
            swap_used_mb: (20_000.0 * swap_used_pct / 100.0) as u64,
            swap_used_pct,
            psi_some_avg10,
            psi_some_avg60: 0.0,
            psi_full_avg10: 0.0,
            pgmajfault: 0,
            pressure: Pressure::Green,
            captured_at_unix_ms: 0,
        }
    }
    #[test]
    fn classify_green_all_healthy() {
        let snap = make_snap(8192, 5.0, 0.5);
        assert_eq!(classify_pressure(&snap), Pressure::Green);
    }
    #[test]
    fn classify_yellow_by_available_mb() {
        let snap = make_snap(YELLOW_AVAILABLE_MB_BELOW - 1, 0.0, 0.0);
        assert_eq!(classify_pressure(&snap), Pressure::Yellow);
    }
    #[test]
    fn classify_yellow_by_swap_pct() {
        let snap = make_snap(8192, YELLOW_SWAP_USED_PCT_ABOVE + 1.0, 0.0);
        assert_eq!(classify_pressure(&snap), Pressure::Yellow);
    }
    #[test]
    fn classify_yellow_by_psi() {
        let snap = make_snap(8192, 0.0, YELLOW_PSI_AVG10_PCT + 1.0);
        assert_eq!(classify_pressure(&snap), Pressure::Yellow);
    }
    #[test]
    fn classify_red_by_available_mb() {
        let snap = make_snap(RED_AVAILABLE_MB_BELOW - 1, 0.0, 0.0);
        assert_eq!(classify_pressure(&snap), Pressure::Red);
    }
    #[test]
    fn classify_red_by_swap_pct() {
        let snap = make_snap(8192, RED_SWAP_USED_PCT_ABOVE + 1.0, 0.0);
        assert_eq!(classify_pressure(&snap), Pressure::Red);
    }
    #[test]
    fn classify_red_by_psi() {
        let snap = make_snap(8192, 0.0, RED_PSI_AVG10_PCT + 1.0);
        assert_eq!(classify_pressure(&snap), Pressure::Red);
    }
    #[test]
    fn classify_red_takes_priority_over_yellow() {
        let snap = make_snap(YELLOW_AVAILABLE_MB_BELOW - 1, 0.0, RED_PSI_AVG10_PCT + 5.0);
        assert_eq!(classify_pressure(&snap), Pressure::Red);
    }
    #[test]
    fn swap_pct_zero_when_no_swap() {
        assert!((compute_swap_pct(0, 0) - 0.0).abs() < f64::EPSILON);
    }
    #[test]
    fn swap_pct_full_when_none_free() {
        assert!((compute_swap_pct(1000, 0) - 100.0).abs() < f64::EPSILON);
    }
    #[test]
    fn swap_pct_half_used() {
        assert!((compute_swap_pct(1000, 500) - 50.0).abs() < 0.001);
    }
    #[test]
    fn unix_ms_now_is_recent() {
        let ms = unix_ms_now();
        assert!(ms > 1_577_836_800_000, "timestamp looks stale: {ms}");
    }
    /// Live smoke test: snapshot must not panic and must return a valid Pressure.
    #[test]
    #[cfg(target_os = "linux")]
    fn live_snapshot_smoke() {
        let state = SwapState::new();
        let snap = snapshot(&state).expect("live snapshot should succeed on Linux");
        assert!(
            matches!(
                snap.pressure,
                Pressure::Green | Pressure::Yellow | Pressure::Red
            ),
            "pressure must be one of the three variants"
        );
        assert!(snap.total_mb > 0);
        assert!(snap.captured_at_unix_ms > 1_577_836_800_000);
    }
}
