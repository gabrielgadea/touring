//! Error types for the touring-resource-monitor crate.
//!
//! All errors flow through [`TrmError`] (umbrella). Sub-errors are exposed
//! for callers that want to match on a specific failure mode.

use std::io;
use thiserror::Error;

/// Umbrella error type for the resource monitor crate.
#[derive(Debug, Error)]
pub enum TrmError {
    /// Pressure reading failed (PSI / meminfo / swap).
    #[error("pressure read: {0}")]
    Pressure(#[from] PressureReadError),

    /// CPU affinity manipulation failed.
    #[error("affinity: {0}")]
    Affinity(#[from] AffinityError),

    /// CPU topology detection failed.
    #[error("topology: {0}")]
    Topology(#[from] TopologyError),

    /// Tokio runtime not available when starting the ticker.
    #[error("ticker requires an active Tokio runtime")]
    NoTokioRuntime,

    /// Ticker already running (start_ticker is idempotent — informational).
    #[error("ticker already running")]
    TickerAlreadyRunning,
}

/// Errors specific to memory pressure reading.
#[derive(Debug, Error)]
pub enum PressureReadError {
    /// `/proc/pressure/memory` unavailable (kernel < 4.20 or PSI disabled).
    #[error("/proc/pressure/memory unavailable — kernel < 4.20 or PSI disabled")]
    PsiUnavailable,

    /// `/proc/meminfo` unreadable.
    #[error("/proc/meminfo unreadable: {0}")]
    MeminfoUnreadable(#[source] io::Error),

    /// `/proc/vmstat` unreadable.
    #[error("/proc/vmstat unreadable: {0}")]
    VmstatUnreadable(#[source] io::Error),

    /// Field expected but absent in /proc file.
    #[error("missing field {field} in {path}")]
    MissingField {
        /// Name of the missing /proc field (e.g. `"MemAvailable"`).
        field: &'static str,
        /// Path to the /proc file that lacked the field
        /// (e.g. `"/proc/meminfo"`).
        path: &'static str,
    },

    /// Numeric parse failed.
    #[error("parse error in {file} for field {field}: {value:?}")]
    ParseError {
        /// Source file (e.g. `"/proc/meminfo"`).
        file: &'static str,
        /// Field name that failed to parse.
        field: &'static str,
        /// The raw, unparseable value found at that field.
        value: String,
    },
}

/// Errors from `libc::sched_setaffinity` and friends.
#[derive(Debug, Error)]
pub enum AffinityError {
    /// `sched_setaffinity` returned -1; errno included.
    #[error("sched_setaffinity failed (errno={errno}): {description}")]
    SetAffinityFailed {
        /// `errno` value returned by the kernel call (e.g.
        /// `1` for `EPERM`).
        errno: i32,
        /// Human-readable description of the failure (typically
        /// the `strerror(errno)` rendering).
        description: String,
    },

    /// Requested core list contained invalid IDs (out of range or empty).
    #[error("invalid core list: {0}")]
    InvalidCoreList(String),

    /// Operation requires Linux; called on another OS.
    #[error("affinity operations require Linux")]
    NotLinux,
}

/// Errors from `/sys/devices/system/cpu/.../topology` parsing.
#[derive(Debug, Error)]
pub enum TopologyError {
    /// `/sys/devices/system/cpu` missing or unreadable.
    #[error("/sys topology root unreadable: {0}")]
    SysfsUnreadable(#[source] io::Error),

    /// No CPUs found in topology — empty system?
    #[error("no CPUs detected via /sys/devices/system/cpu")]
    NoCpus,

    /// Field absent in topology entry.
    #[error("topology field {field} missing for cpu{cpu_id}")]
    MissingTopologyField {
        /// CPU id whose topology entry lacks the field.
        cpu_id: u32,
        /// Name of the missing field (e.g. `"core_siblings_list"`).
        field: &'static str,
    },

    /// Parse error in topology field.
    #[error("topology parse error: cpu{cpu_id}/{field} = {value:?}")]
    ParseError {
        /// CPU id whose topology entry triggered the failure.
        cpu_id: u32,
        /// Field name that failed to parse.
        field: &'static str,
        /// Raw value that could not be interpreted.
        value: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pressure_error_displays_psi_unavailable() {
        let e = PressureReadError::PsiUnavailable;
        let s = e.to_string();
        assert!(s.contains("4.20"), "got: {s}");
    }

    #[test]
    fn trm_error_wraps_pressure() {
        let inner = PressureReadError::PsiUnavailable;
        let outer: TrmError = inner.into();
        assert!(matches!(outer, TrmError::Pressure(_)));
    }

    #[test]
    fn affinity_error_displays_errno() {
        let e = AffinityError::SetAffinityFailed {
            errno: 1,
            description: "EPERM".into(),
        };
        let s = e.to_string();
        assert!(s.contains("errno=1"));
        assert!(s.contains("EPERM"));
    }

    #[test]
    fn topology_error_no_cpus() {
        let e = TopologyError::NoCpus;
        assert!(e.to_string().contains("no CPUs"));
    }
}
