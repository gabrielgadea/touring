//! Process RSS + virtual-memory probe for the long-lived `touring-daemon`.
//!
//! Thin wrapper around the `memory-stats` crate (cross-platform: Linux via
//! `/proc/self/statm`, macOS via `mach_task_basic_info`, Windows via
//! `GetProcessMemoryInfo`). Converts raw bytes to megabytes so the values
//! fit the CILA token budget when surfaced through `touring gate-metrics -j`.
//!
//! # Why a dedicated module
//!
//! The daemon is a long-lived mpsc actor — any slow drift in RSS over hours
//! of hook traffic points at a structural leak (stuck WAL cursors, orphan
//! `tokio::task` handles, cache bloat). `GateMetricsSnapshot` already ships
//! counters for dispatch volume; this probe adds the *footprint* dimension
//! so operators can correlate "60k rkyv dispatches" with "450 MB RSS" in a
//! single JSON snapshot, without running `dhat` or `heaptrack`.
//!
//! # Failure mode
//!
//! If `memory_stats()` returns `None` (unsupported OS, permission denied on
//! `/proc`, or exotic sandboxes that block the Mach port syscall), the probe
//! degrades to zeros — matching the `AtomicU64` convention of the rest of
//! the telemetry stack. Zero is also the sentinel JSON consumers should
//! branch on to decide "memory probe unavailable on this host".

use memory_stats::memory_stats;

/// Snapshot of the calling process's memory footprint.
///
/// Fields are in megabytes (`bytes / 1_048_576`) to keep JSON payloads
/// compact and human-readable. Both fields are `f64` so fractional MB
/// (common for small processes) survive without rounding artefacts.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MemorySnapshot {
    /// Resident-set size — physical RAM currently held by the process.
    pub physical_mb: f64,
    /// Virtual-address-space size — includes mapped-but-unresident pages.
    pub virtual_mb: f64,
}

impl Default for MemorySnapshot {
    fn default() -> Self {
        Self {
            physical_mb: 0.0,
            virtual_mb: 0.0,
        }
    }
}

const BYTES_PER_MB: f64 = 1_048_576.0;

/// Capture the current process's RSS + virtual-size.
///
/// Returns `MemorySnapshot::default()` (both zeros) when the underlying
/// `memory-stats` call reports no data — see module-level docs.
#[must_use]
pub fn snapshot() -> MemorySnapshot {
    match memory_stats() {
        Some(stats) => MemorySnapshot {
            physical_mb: stats.physical_mem as f64 / BYTES_PER_MB,
            virtual_mb: stats.virtual_mem as f64 / BYTES_PER_MB,
        },
        None => MemorySnapshot::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_returns_nonzero_in_test_process() {
        // The cargo-test runner is a real process — memory-stats MUST report
        // at least a handful of MB of RSS. Zero would indicate the probe
        // failed silently on this host (which is allowed in prod but should
        // never happen under Linux CI).
        let s = snapshot();
        assert!(
            s.physical_mb > 0.0,
            "physical_mb should be > 0 in test process, got {}",
            s.physical_mb
        );
        assert!(
            s.virtual_mb >= s.physical_mb,
            "virtual ({}) >= physical ({})",
            s.virtual_mb,
            s.physical_mb
        );
    }

    #[test]
    fn snapshot_default_is_all_zeros() {
        let d = MemorySnapshot::default();
        assert_eq!(d.physical_mb, 0.0);
        assert_eq!(d.virtual_mb, 0.0);
    }

    #[test]
    fn snapshot_is_serde_roundtrip() {
        // Wire-compat check: the struct is embedded in GateMetricsSnapshot
        // so its JSON shape is part of the touring gate-metrics contract.
        let s = MemorySnapshot {
            physical_mb: 42.5,
            virtual_mb: 128.25,
        };
        let json = serde_json::to_string(&s).expect("serialize");
        let back: MemorySnapshot = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, s);
    }
}
