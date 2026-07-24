//! # touring-resource-monitor
//!
//! PSI memory pressure sentinel + CPU P/E core topology scheduler.
//!
//! Replaces the bash-based `smart-cores-v2` daemon (broken `WatchdogSec=60`
//! without `sd_notify` → SIGABRT crash-loop). Created 2026-05-02 after a
//! 47.5 GiB anon-RSS OOM incident + 8 reboots in 24h.
//!
//! ## Architecture
//!
//! - `MemoryGuard` — singleton ticker reading `/proc/pressure/memory` (PSI),
//!   `/proc/meminfo`, and `/proc/vmstat::pgmajfault`. Emits `Pressure`
//!   tier (Green/Yellow/Red) every 1 s.
//! - `CoreScheduler` — reads `/sys/devices/system/cpu/cpuN/topology/cluster_id`
//!   to detect P-cores (cluster size 2 = HT pairs) vs E-cores (cluster size 4).
//!   Wraps `libc::sched_setaffinity` for thread pinning.
//!
//! ## Foundational level
//!
//! Zero `touring-*` dependencies — sits at the same layer as `touring-core`
//! and `touring-telemetry`. Consumers: `touring-hooks` (pre_bash gate),
//! `touring-server` (status query), `touring-cognitive` (P-core pinning of
//! AdaptiveEngine rayon pool).

pub mod error;
pub mod memory;
pub mod metrics;

#[cfg(target_os = "linux")]
pub mod core_sched;
#[cfg(target_os = "linux")]
pub mod guard;

pub use error::{AffinityError, PressureReadError, TopologyError, TrmError};
pub use memory::{MemorySnapshot, Pressure};

#[cfg(target_os = "linux")]
pub use core_sched::{CoreScheduler, CoreType};
#[cfg(target_os = "linux")]
pub use guard::MemoryGuard;
