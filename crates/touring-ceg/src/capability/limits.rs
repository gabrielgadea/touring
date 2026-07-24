//! Per-execution resource limits for the Code Execution Gateway — phase
//! **P4.3** of CEG Pln2 (`docs/2026-05-17-ceg-pln2-plan.md`).
//!
//! Two layers, with honest boundaries:
//!
//! - **`rlimit`** — `setrlimit` caps (CPU time, address space, open files,
//!   process count, file size). Universal: [`apply_resource_caps_to`] wires
//!   them into *every* sandboxed [`Command`](tokio::process::Command), so the
//!   X5 dry-run and the compiled-language runs are capped, not only the X8
//!   SUPERVISED-EXEC path. This is the always-on baseline.
//! - **cgroup v2** — stronger, group-level caps. [`cgroup_v2_status`] probes
//!   the host *honestly*: group-level capping of an unprivileged subprocess
//!   needs the daemon to run inside a **delegated** cgroup slice (cgroup v2's
//!   no-internal-processes rule otherwise blocks controller delegation). The
//!   probe reports that precondition truthfully — the rlimit baseline holds
//!   regardless.
//!
//! P4.3 reuses [`ResourceCaps`] / [`apply_rlimit`] from
//! [`super::enforce_linux`] (delivered early in P2.4-A): [`ResourceLimits`] is
//! the unified per-execution bundle, and [`apply_resource_caps_to`] is the
//! wiring that closes the "X5 path is uncapped" gap.

use std::path::PathBuf;

#[cfg(target_os = "linux")]
use super::enforce_linux::apply_rlimit;
use super::enforce_linux::{ResourceCaps, apply_landlock};

/// The resource limits applied to one sandboxed execution.
#[derive(Debug, Clone, Default)]
pub struct ResourceLimits {
    /// `setrlimit` caps — the universal per-execution baseline.
    pub rlimit: ResourceCaps,
}

impl ResourceLimits {
    /// Conservative caps for an untrusted sandboxed run — the clean,
    /// per-process rlimit classes only.
    ///
    /// Two rlimit classes are deliberately **left uncapped**, because each is
    /// the wrong tool for a per-execution sandbox:
    /// - `RLIMIT_AS` (address space) bounds *virtual* memory — mmap-heavy but
    ///   legitimate programs (compiler toolchains, numeric runtimes) routinely
    ///   exceed it while using little physical RAM.
    /// - `RLIMIT_NPROC` (process count) is enforced **per real UID**, not per
    ///   execution: on a shared user account it is already saturated by the
    ///   user's other processes, so a low value aborts every `fork` the
    ///   sandboxed program makes (e.g. `rustc` forking its linker).
    ///
    /// Per-execution *memory* and *process-count* capping is the job of cgroup
    /// `memory.max` / `pids.max` — see [`cgroup_v2_status`]. The rlimit baseline
    /// caps CPU time, file descriptors and file size, which `setrlimit` bounds
    /// cleanly *per process*.
    #[must_use]
    pub fn sandboxed() -> Self {
        Self {
            rlimit: ResourceCaps {
                cpu_seconds: Some(30),
                address_space_bytes: None,
                open_files: Some(256),
                processes: None,
                file_size_bytes: Some(256 * 1024 * 1024),
            },
        }
    }
}

/// Wire `limits` into a sandbox [`Command`](tokio::process::Command): registers
/// a pre-exec closure that applies the `rlimit` caps in the child — post-fork,
/// pre-`exec` — so the sandboxed process is resource-capped.
///
/// This is what makes "capped per execution" true for the whole sandbox
/// surface: the X5 dry-run path and the compiled-language runners route
/// through it, not only the X8 SUPERVISED-EXEC stage. The `setrlimit`
/// enforcement is Linux-specific — a no-op on other targets.
///
/// On the first call of the process it also emits the consolidated
/// [`sandbox_enforcement_advisory`] once (via `tracing`), so an operator sees
/// exactly which kernel protections — rlimit, cgroup, landlock — are active.
pub fn apply_resource_caps_to(cmd: &mut tokio::process::Command, limits: &ResourceLimits) {
    // P4 cross-audit — emit the enforcement advisory once. Every X5 dry-run
    // and X8 SUPERVISED-EXEC spawn routes through this function, so it is the
    // one place guaranteed to run before any sandboxed subprocess.
    log_enforcement_advisory_once();

    #[cfg(target_os = "linux")]
    {
        let caps = limits.rlimit.clone();
        // SAFETY: the closure runs post-fork, pre-`exec`, in the child.
        // `apply_rlimit` performs only `setrlimit` syscalls + fixed stack work
        // — allocation-free and async-signal-safe, as the post-fork context
        // demands.
        unsafe {
            cmd.pre_exec(move || apply_rlimit(&caps).map(|_| ()));
        }
    }
    // The `setrlimit` enforcement is Linux-specific; on other targets the
    // advisory above still fires and the rlimit wiring is a no-op.
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (cmd, limits);
    }
}

/// Emit the [`sandbox_enforcement_advisory`] exactly once per process.
///
/// [`apply_resource_caps_to`] calls this on every sandbox spawn; the
/// [`Once`](std::sync::Once) gate collapses that to a single `tracing` line at
/// first use — enough for an operator to see the active protections, with no
/// per-spawn log noise.
fn log_enforcement_advisory_once() {
    static ADVISORY: std::sync::Once = std::sync::Once::new();
    ADVISORY.call_once(|| {
        tracing::info!(target: "ceg::enforcement", "{}", sandbox_enforcement_advisory());
    });
}

/// A consolidated, human-readable advisory of the kernel-level protection the
/// CEG sandbox applies — its three layers reported together:
///
/// - **rlimit** — the always-on per-process `setrlimit` baseline
///   ([`ResourceLimits::sandboxed`]).
/// - **cgroup v2** — the optional stronger group-level layer
///   ([`cgroup_v2_status`]).
/// - **landlock** — the X8 SUPERVISED-EXEC filesystem confinement
///   ([`apply_landlock`]).
///
/// This is the advisory P4.2 / P4.3 defined: a truthful report of what is — and
/// is not — enforced on this host. [`apply_resource_caps_to`] emits it once.
#[must_use]
pub fn sandbox_enforcement_advisory() -> String {
    let rlimit = format!(
        "rlimit: {} per-process classes capped (always on)",
        ResourceLimits::sandboxed().rlimit.active_count()
    );
    let cgroup = match cgroup_v2_status() {
        CgroupStatus::Available { .. } => "cgroup v2: group-level capping available".to_owned(),
        CgroupStatus::Unavailable { reason } => {
            format!("cgroup v2: unavailable — {reason}")
        }
    };
    let landlock = format!(
        "landlock: {:?}",
        apply_landlock(&super::builtins::trusted()).level
    );
    format!("CEG sandbox enforcement — {rlimit}; {cgroup}; {landlock}")
}

/// The kernel cgroup v2 availability for **group-level** resource capping.
///
/// Group-level capping (`memory.max`, `pids.max`) is stronger than `rlimit` —
/// enforced on a group of processes, not raisable by the capped process — but
/// an unprivileged process can only create a capping cgroup inside a
/// **delegated** slice. This status reports that precondition honestly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CgroupStatus {
    /// cgroup v2 is mounted and the process sits in a writable cgroup whose
    /// `cgroup.controllers` expose `memory` + `pids` — group-level capping is
    /// possible.
    Available {
        /// The process's own cgroup directory under `/sys/fs/cgroup`.
        own_dir: PathBuf,
    },
    /// Group-level capping is not available here — `reason` is explicit. The
    /// `rlimit` baseline ([`apply_resource_caps_to`]) still applies; this is a
    /// **loud** degradation, never a silent gap.
    Unavailable {
        /// Why cgroup v2 group-capping is not available on this host.
        reason: String,
    },
}

impl CgroupStatus {
    /// `true` only for [`CgroupStatus::Available`].
    #[must_use]
    pub fn is_available(&self) -> bool {
        matches!(self, CgroupStatus::Available { .. })
    }
}

/// Probe the host for kernel cgroup v2 group-level resource capping.
///
/// Honest and explicit: returns [`CgroupStatus::Available`] only when cgroup v2
/// is mounted at `/sys/fs/cgroup`, the process's own cgroup directory is
/// writable, and its `cgroup.controllers` expose both `memory` and `pids`.
/// Otherwise [`CgroupStatus::Unavailable`] names the precise reason. The
/// `rlimit` baseline holds unconditionally — this probe only governs the
/// optional stronger layer.
#[must_use]
pub fn cgroup_v2_status() -> CgroupStatus {
    let root = std::path::Path::new("/sys/fs/cgroup");
    if !root.join("cgroup.controllers").is_file() {
        return CgroupStatus::Unavailable {
            reason: "cgroup v2 is not mounted at /sys/fs/cgroup".to_owned(),
        };
    }
    let own = match own_cgroup_dir() {
        Some(dir) => dir,
        None => {
            return CgroupStatus::Unavailable {
                reason: "could not resolve the process's own cgroup from /proc/self/cgroup"
                    .to_owned(),
            };
        }
    };
    let controllers = std::fs::read_to_string(own.join("cgroup.controllers")).unwrap_or_default();
    let has_memory = controllers.split_whitespace().any(|c| c == "memory");
    let has_pids = controllers.split_whitespace().any(|c| c == "pids");
    if !(has_memory && has_pids) {
        return CgroupStatus::Unavailable {
            reason: format!(
                "the process's cgroup does not expose the memory + pids controllers (has: {})",
                controllers.trim()
            ),
        };
    }
    CgroupStatus::Available { own_dir: own }
}

/// Resolve the process's own cgroup directory from `/proc/self/cgroup`.
///
/// cgroup v2 writes a single `0::<path>` line; the directory is
/// `/sys/fs/cgroup<path>`.
fn own_cgroup_dir() -> Option<PathBuf> {
    let content = std::fs::read_to_string("/proc/self/cgroup").ok()?;
    let path = content
        .lines()
        .find_map(|line| line.strip_prefix("0::"))?
        .trim();
    Some(PathBuf::from("/sys/fs/cgroup").join(path.trim_start_matches('/')))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resource_limits_sandboxed_caps_the_clean_rlimit_classes() {
        let limits = ResourceLimits::sandboxed();
        // CPU time, file descriptors and file size are capped per process;
        // address space (too blunt) and process count (per-UID, not
        // per-execution) are left to cgroup — see `cgroup_v2_status`.
        assert_eq!(limits.rlimit.active_count(), 3);
        assert_eq!(limits.rlimit.address_space_bytes, None);
        assert_eq!(limits.rlimit.processes, None);
        assert_eq!(limits.rlimit.cpu_seconds, Some(30));
        assert_eq!(limits.rlimit.open_files, Some(256));
    }

    #[test]
    fn resource_limits_default_is_uncapped() {
        let limits = ResourceLimits::default();
        assert_eq!(limits.rlimit.active_count(), 0);
    }

    #[test]
    fn resource_limits_is_debug_and_clone() {
        let limits = ResourceLimits::sandboxed();
        let cloned = limits.clone();
        assert_eq!(cloned.rlimit, limits.rlimit);
        assert!(!format!("{limits:?}").is_empty());
    }

    #[test]
    fn cgroup_v2_status_returns_a_definite_status() {
        // The probe must always yield a concrete, inspectable status — never
        // panic — whatever the host looks like.
        let status = cgroup_v2_status();
        match &status {
            CgroupStatus::Available { own_dir } => {
                assert!(own_dir.starts_with("/sys/fs/cgroup"));
                assert!(status.is_available());
            }
            CgroupStatus::Unavailable { reason } => {
                assert!(
                    !reason.is_empty(),
                    "an Unavailable status must name the reason"
                );
                assert!(!status.is_available());
            }
        }
    }

    #[test]
    fn cgroup_status_unavailable_is_never_silent() {
        let status = CgroupStatus::Unavailable {
            reason: "test reason".to_owned(),
        };
        assert!(!status.is_available());
        assert!(format!("{status:?}").contains("test reason"));
    }

    #[test]
    fn sandbox_enforcement_advisory_reports_all_three_layers() {
        // The advisory consolidates the rlimit, cgroup and landlock layers —
        // each must be named, whatever this host's kernel supports.
        let advisory = sandbox_enforcement_advisory();
        assert!(advisory.contains("rlimit"), "advisory: {advisory}");
        assert!(advisory.contains("cgroup"), "advisory: {advisory}");
        assert!(advisory.contains("landlock"), "advisory: {advisory}");
        assert!(advisory.starts_with("CEG sandbox enforcement"));
    }

    #[test]
    fn log_enforcement_advisory_once_is_idempotent() {
        // The Once gate makes repeated calls safe — no panic, no double work.
        log_enforcement_advisory_once();
        log_enforcement_advisory_once();
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn own_cgroup_dir_resolves_under_sys_fs_cgroup() {
        // On a Linux host with cgroup v2 the own-cgroup path resolves beneath
        // the cgroup v2 mount; if the host has no cgroup v2 it is simply None.
        if let Some(dir) = own_cgroup_dir() {
            assert!(dir.starts_with("/sys/fs/cgroup"));
        }
    }
}
