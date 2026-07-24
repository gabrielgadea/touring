//! CPU P/E core scheduler — topology detection and thread affinity pinning.
//!
//! Entry point: [`CoreScheduler::detect()`] reads `/sys` at construction time
//! and classifies every online CPU as [`CoreType::Performance`] or
//! [`CoreType::Efficiency`]. Thread pinning is then a single method call.
//!
//! ## Quick start
//!
//! ```no_run
//! # #[cfg(target_os = "linux")]
//! # {
//! use touring_resource_monitor::core_sched::{CoreScheduler, CoreType};
//!
//! let sched = CoreScheduler::detect().expect("topology detection failed");
//! // Pin the current thread to all P-cores (high-priority work).
//! let _ = sched.pin_self(CoreType::Performance);
//! # }
//! ```

pub mod affinity;
pub mod topology;

pub use affinity::current_tid;
pub use topology::{detect_topology, TopologyMap};

use crate::sentinel::error::{AffinityError, TopologyError};

/// Classification of a logical CPU as Performance or Efficiency.
///
/// Used to select the target CPU set for [`CoreScheduler::pin_self`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoreType {
    /// High-frequency P-core (Hyper-Threading pair or non-HT P-core).
    Performance,
    /// Power-efficient E-core (Intel Gracemont quad-cluster).
    Efficiency,
}

/// Schedules threads onto P-cores or E-cores based on detected CPU topology.
///
/// Construct via [`CoreScheduler::detect()`] (reads `/sys` once at startup)
/// or [`CoreScheduler::from_topology`] (supply a pre-built [`TopologyMap`]
/// for testing or when topology is known ahead of time).
#[derive(Debug)]
pub struct CoreScheduler {
    topology: TopologyMap,
}

impl CoreScheduler {
    /// Detect P/E core topology from `/sys/devices/system/cpu` and construct a
    /// scheduler.
    ///
    /// This performs blocking I/O. Call once at startup and share the
    /// resulting `CoreScheduler` (e.g. wrapped in `Arc`).
    ///
    /// # Errors
    ///
    /// Propagates [`TopologyError`] if sysfs is unreadable or no CPUs are
    /// found.
    pub fn detect() -> Result<Self, TopologyError> {
        let topology = topology::detect_topology()?;
        Ok(Self { topology })
    }

    /// Construct from a known [`TopologyMap`].
    ///
    /// Useful for unit tests or when the topology has been detected elsewhere
    /// and shared across components.
    pub fn from_topology(topology: TopologyMap) -> Self {
        Self { topology }
    }

    /// Slice of all logical CPU IDs classified as Performance cores.
    pub fn p_cores(&self) -> &[u32] {
        &self.topology.p_cores
    }

    /// Slice of all logical CPU IDs classified as Efficiency cores.
    pub fn e_cores(&self) -> &[u32] {
        &self.topology.e_cores
    }

    /// The underlying topology map (for introspection / serialisation).
    pub fn topology(&self) -> &TopologyMap {
        &self.topology
    }

    /// Pin `tid` to all P-cores (high-priority work).
    ///
    /// # Errors
    ///
    /// - [`AffinityError::InvalidCoreList`] — no P-cores were detected.
    /// - [`AffinityError::SetAffinityFailed`] — syscall failed (e.g. `EPERM`
    ///   in a container).
    pub fn pin_high_priority(&self, tid: libc::pid_t) -> Result<(), AffinityError> {
        if self.topology.p_cores.is_empty() {
            return Err(AffinityError::InvalidCoreList("no P-cores detected".into()));
        }
        affinity::pin_to_cpus(tid, &self.topology.p_cores)
    }

    /// Pin `tid` to all E-cores (background / low-priority work).
    ///
    /// Falls back to P-cores with a `tracing::warn!` if no E-cores were
    /// detected (e.g. on a P-core-only system or VM).
    ///
    /// # Errors
    ///
    /// - [`AffinityError::InvalidCoreList`] — neither E-cores nor P-cores are
    ///   available (degenerate topology).
    /// - [`AffinityError::SetAffinityFailed`] — syscall failed.
    pub fn pin_background(&self, tid: libc::pid_t) -> Result<(), AffinityError> {
        let cpus: &[u32] = if self.topology.e_cores.is_empty() {
            tracing::warn!("no E-cores detected, falling back to P-cores for background thread");
            &self.topology.p_cores
        } else {
            &self.topology.e_cores
        };
        affinity::pin_to_cpus(tid, cpus)
    }

    /// Pin the **current** thread to the CPU set corresponding to `core_type`.
    ///
    /// Convenience wrapper around [`CoreScheduler::pin_high_priority`] and
    /// [`CoreScheduler::pin_background`] that resolves the current thread ID
    /// automatically via [`current_tid()`].
    ///
    /// # Errors
    ///
    /// Same as [`pin_high_priority`](Self::pin_high_priority) and
    /// [`pin_background`](Self::pin_background).
    pub fn pin_self(&self, core_type: CoreType) -> Result<(), AffinityError> {
        let tid = affinity::current_tid();
        match core_type {
            CoreType::Performance => self.pin_high_priority(tid),
            CoreType::Efficiency => self.pin_background(tid),
        }
    }
}

// ─── Unit Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sentinel::core_sched::topology::classify_by_cluster_size;

    /// Build a simple 4P + 4E topology for testing.
    fn test_topology() -> TopologyMap {
        // 2 P-core clusters of size 2 (HT pairs) → cpu0-3 as P
        // 1 E-core cluster of size 4 → cpu4-7 as E
        let pairs: Vec<(u32, u32)> = vec![
            (0, 0),
            (1, 0), // P-core cluster 0
            (2, 8),
            (3, 8), // P-core cluster 1
            (4, 16),
            (5, 16),
            (6, 16),
            (7, 16), // E-core cluster
        ];
        classify_by_cluster_size(&pairs)
    }

    #[test]
    fn from_topology_accessors() {
        let topo = test_topology();
        let sched = CoreScheduler::from_topology(topo);

        assert_eq!(sched.p_cores(), &[0, 1, 2, 3]);
        assert_eq!(sched.e_cores(), &[4, 5, 6, 7]);
        assert_eq!(sched.topology().total, 8);
    }

    #[test]
    fn pin_high_priority_no_p_cores_returns_error() {
        // Topology with only E-cores (degenerate but valid test case).
        let topo = TopologyMap {
            p_cores: Vec::new(),
            e_cores: vec![0, 1, 2, 3],
            total: 4,
        };
        let sched = CoreScheduler::from_topology(topo);
        let result = sched.pin_high_priority(0);
        assert!(
            matches!(result, Err(AffinityError::InvalidCoreList(_))),
            "expected InvalidCoreList, got {result:?}"
        );
    }

    #[test]
    fn pin_background_falls_back_to_p_when_no_e_cores() {
        // P-only topology — pin_background must fall back without panicking.
        let topo = TopologyMap {
            p_cores: vec![0, 1],
            e_cores: Vec::new(),
            total: 2,
        };
        let sched = CoreScheduler::from_topology(topo);
        // Result is Ok or EPERM — either is acceptable; must not panic.
        let result = sched.pin_background(0);
        match result {
            Ok(()) => {}
            Err(AffinityError::SetAffinityFailed { errno, .. }) if errno == libc::EPERM => {}
            Err(e) => panic!("unexpected error: {e:?}"),
        }
    }

    /// Smoke test: `pin_self` dispatches correctly for both variants.
    #[test]
    #[cfg(target_os = "linux")]
    fn pin_self_smoke() {
        let topo = test_topology();
        let sched = CoreScheduler::from_topology(topo);

        // Both calls must return Ok or a known AffinityError — never panic.
        for core_type in [CoreType::Performance, CoreType::Efficiency] {
            let result = sched.pin_self(core_type);
            match result {
                Ok(()) => {}
                Err(AffinityError::SetAffinityFailed { errno, .. })
                    if errno == libc::EPERM || errno == libc::EINVAL => {}
                Err(AffinityError::InvalidCoreList(_)) => {}
                Err(e) => panic!("unexpected error for {core_type:?}: {e:?}"),
            }
        }
    }
}
