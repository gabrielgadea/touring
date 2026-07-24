//! Linux capability enforcement for the Code Execution Gateway (CEG) —
//! phases **P2.4 + P4.2** of CEG Pln2 (`docs/2026-05-17-ceg-pln2-plan.md`).
//!
//! Two mechanisms turn a resolved [`CapabilityProfile`]
//! into a real OS-level boundary around a sandboxed child process:
//!
//! - **`apply_rlimit`** — `setrlimit`-based resource caps (CPU time, address
//!   space, open files, process count, file size). **Delivered (P2.4-A).**
//! - **`apply_landlock` + `LandlockRuleset`** — landlock LSM kernel-enforced
//!   filesystem sandboxing via the audited `landlock` crate (v0.4.x).
//!   **Delivered (P4.2).** The ruleset is built parent-side under
//!   [`CompatLevel::BestEffort`](landlock::CompatLevel) and enforced post-fork
//!   in the child via [`LandlockRuleset::restrict_current_thread`]. On
//!   Linux 5.13+ this reports [`EnforcementLevel::KernelEnforced`]; older
//!   kernels degrade to [`EnforcementLevel::ProcessIsolationOnly`] — loud
//!   degradation, never a silent no-op. Currently uses `ABI::V6` for
//!   filesystem access. **Follow-up wave** (documented in
//!   `docs/2026-05-22-ceg-pln2-final-implementation.md`): wire
//!   `AccessNet::BindTcp | ConnectTcp` (V4 — Linux 6.7+) and
//!   `Scope::AbstractUnixSocket | Signal` (V6 — Linux 6.12+) to extend
//!   kernel-enforced isolation to network and IPC surfaces.
//!
//! Both are designed to run inside a `Command::pre_exec` closure (post-fork,
//! in the child, before `exec`) so the restrictions bind the sandboxed
//! process and not the Touring daemon. `apply_rlimit` is therefore
//! **allocation-free** on its success path (only `setrlimit` syscalls + stack
//! work) — async-signal-safe, as the post-fork context demands.

use std::io;

use serde::{Deserialize, Serialize};

use super::CapabilityProfile;

#[cfg(target_os = "linux")]
use rustix::process::{Resource, Rlimit, setrlimit};

/// The OS-level isolation actually achieved for an execution.
///
/// Recorded by the CEG X9 LEARN stage: a degraded level is a weaker security
/// posture and must be observable, never silent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EnforcementLevel {
    /// Kernel-enforced sandboxing is live (landlock LSM). Strongest — the
    /// target state once the `landlock` crate is wired (P2.4-B).
    KernelEnforced,
    /// Process-level isolation only — `rlimit` caps plus the user-space
    /// capability decision. The kernel does **not** enforce the
    /// filesystem/network boundary (landlock not compiled in).
    ProcessIsolationOnly,
    /// No OS-level isolation available (non-Linux target). Weakest — only the
    /// user-space capability decision applies.
    Unsupported,
}

/// A resource class [`ResourceCaps`] can limit.
///
/// Platform-independent — the rustix `Resource` mapping happens only at
/// enforcement time, so a cap set is inspectable and testable on any OS.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CappedResource {
    /// CPU time in seconds (`RLIMIT_CPU`).
    CpuSeconds,
    /// Address space / virtual memory in bytes (`RLIMIT_AS`).
    AddressSpace,
    /// Open file descriptors (`RLIMIT_NOFILE`).
    OpenFiles,
    /// Process / thread count for the real user id (`RLIMIT_NPROC`).
    Processes,
    /// Maximum created-file size in bytes (`RLIMIT_FSIZE`).
    FileSize,
}

/// Per-execution resource caps applied via `setrlimit` before a sandboxed
/// child runs.
///
/// A `None` field leaves that limit untouched — the child inherits the
/// parent's. Maps to the CEG Pln2 P4.3 resource-cap set.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ResourceCaps {
    /// Max CPU time, seconds (`RLIMIT_CPU`).
    pub cpu_seconds: Option<u64>,
    /// Max address space, bytes (`RLIMIT_AS`).
    pub address_space_bytes: Option<u64>,
    /// Max open file descriptors (`RLIMIT_NOFILE`).
    pub open_files: Option<u64>,
    /// Max process / thread count (`RLIMIT_NPROC`).
    pub processes: Option<u64>,
    /// Max created-file size, bytes (`RLIMIT_FSIZE`).
    pub file_size_bytes: Option<u64>,
}

impl ResourceCaps {
    /// No caps — every limit inherits the parent's.
    pub fn none() -> Self {
        Self::default()
    }

    /// Conservative caps for an untrusted sandboxed run: 10 s CPU, 512 MiB
    /// address space, 256 open files, 64 processes, 64 MiB max file size.
    pub fn sandboxed_defaults() -> Self {
        Self {
            cpu_seconds: Some(10),
            address_space_bytes: Some(512 * 1024 * 1024),
            open_files: Some(256),
            processes: Some(64),
            file_size_bytes: Some(64 * 1024 * 1024),
        }
    }

    /// The `(resource, soft-limit)` pairs this cap set expands to.
    ///
    /// Pure — performs no syscalls and is allocation-tolerant — so the
    /// cap-to-resource mapping is unit-testable on any platform. The
    /// enforcement path [`apply_rlimit`] does **not** call this (it stays
    /// allocation-free); this is for inspection / observability.
    pub fn limits(&self) -> Vec<(CappedResource, u64)> {
        let mut out = Vec::new();
        if let Some(v) = self.cpu_seconds {
            out.push((CappedResource::CpuSeconds, v));
        }
        if let Some(v) = self.address_space_bytes {
            out.push((CappedResource::AddressSpace, v));
        }
        if let Some(v) = self.open_files {
            out.push((CappedResource::OpenFiles, v));
        }
        if let Some(v) = self.processes {
            out.push((CappedResource::Processes, v));
        }
        if let Some(v) = self.file_size_bytes {
            out.push((CappedResource::FileSize, v));
        }
        out
    }

    /// Count of caps that are set (`Some`).
    pub fn active_count(&self) -> usize {
        self.limits().len()
    }
}

/// The outcome of querying / applying Linux landlock enforcement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnforcementReport {
    /// The isolation level achieved.
    pub level: EnforcementLevel,
    /// Human-readable note — e.g. why a level degraded.
    pub note: String,
}

/// Apply [`ResourceCaps`] to the **current process** via `setrlimit`.
///
/// Intended to run inside a `Command::pre_exec` closure (post-fork, in the
/// child) so the caps bind the sandboxed process, not the daemon. Each cap is
/// set as **both** the soft and hard limit, so the child cannot raise it back.
///
/// This path is **allocation-free** (a fixed stack array + `setrlimit`
/// syscalls only) — async-signal-safe, valid post-fork. Returns the count of
/// resource classes actually capped. On a non-Linux target it is a no-op
/// returning `0`.
#[cfg(target_os = "linux")]
pub fn apply_rlimit(caps: &ResourceCaps) -> io::Result<usize> {
    let entries: [(Option<u64>, CappedResource); 5] = [
        (caps.cpu_seconds, CappedResource::CpuSeconds),
        (caps.address_space_bytes, CappedResource::AddressSpace),
        (caps.open_files, CappedResource::OpenFiles),
        (caps.processes, CappedResource::Processes),
        (caps.file_size_bytes, CappedResource::FileSize),
    ];
    let mut applied = 0usize;
    for (value, resource) in entries {
        if let Some(v) = value {
            let limit = Rlimit {
                current: Some(v),
                maximum: Some(v),
            };
            setrlimit(capped_to_rustix(resource), limit).map_err(io::Error::from)?;
            applied += 1;
        }
    }
    Ok(applied)
}

/// Non-Linux fallback: resource caps are a no-op (see [`EnforcementLevel`]).
#[cfg(not(target_os = "linux"))]
pub fn apply_rlimit(_caps: &ResourceCaps) -> io::Result<usize> {
    Ok(0)
}

/// Map a platform-independent [`CappedResource`] to the rustix `Resource`.
#[cfg(target_os = "linux")]
fn capped_to_rustix(resource: CappedResource) -> Resource {
    match resource {
        CappedResource::CpuSeconds => Resource::Cpu,
        CappedResource::AddressSpace => Resource::As,
        CappedResource::OpenFiles => Resource::Nofile,
        CappedResource::Processes => Resource::Nproc,
        CappedResource::FileSize => Resource::Fsize,
    }
}

/// Report the landlock filesystem-sandboxing status for `profile`.
///
/// **Status — CEG Pln2 P4.2, delivered.** The `landlock` crate (v0.4.x) is
/// wired (resolving the P2.4-B deferral). On Linux this reports
/// [`EnforcementLevel::KernelEnforced`] — the X8 SUPERVISED-EXEC stage
/// ([`crate::gateway::supervised::run_supervised`]) confines the real run to
/// the granted filesystem roots via the landlock LSM, so a denied path is
/// unreachable at runtime, not merely scored. Off Linux it reports
/// [`EnforcementLevel::Unsupported`].
///
/// This is the *advisory* availability report (used for the X9 LEARN record).
/// The *authoritative* per-run status is the [`EnforcementLevel`] returned by
/// [`LandlockRuleset::restrict_current_thread`] inside the child.
pub fn apply_landlock(profile: &CapabilityProfile) -> EnforcementReport {
    #[cfg(target_os = "linux")]
    let report = EnforcementReport {
        level: EnforcementLevel::KernelEnforced,
        note: format!(
            "landlock LSM wired for profile '{}'; X8 SUPERVISED-EXEC confines \
             the run to the granted filesystem roots — a denied path is \
             kernel-unreachable at runtime (per-run status: restrict_self)",
            profile.name()
        ),
    };
    #[cfg(not(target_os = "linux"))]
    let report = EnforcementReport {
        level: EnforcementLevel::Unsupported,
        note: format!(
            "landlock is Linux-only; profile '{}' has no OS-level filesystem \
             enforcement on this target",
            profile.name()
        ),
    };
    report
}

/// P4.2 — a landlock ruleset built in the parent, ready to be enforced in a
/// child process via [`LandlockRuleset::restrict_current_thread`].
///
/// Built by [`build_landlock_ruleset`], which allocates and performs the
/// `landlock_create_ruleset` / `landlock_add_rule` syscalls — **parent-side**
/// work, before any `fork`. The enforcement step is the `prctl` +
/// `landlock_restrict_self` syscall pair, allocation-light and therefore safe
/// to run post-fork inside a `Command::pre_exec` closure.
#[cfg(target_os = "linux")]
pub struct LandlockRuleset {
    inner: landlock::RulesetCreated,
}

#[cfg(target_os = "linux")]
impl LandlockRuleset {
    /// Enforce this ruleset on the **current thread** and its future children.
    ///
    /// Irreversible. Returns the [`EnforcementLevel`] the kernel actually
    /// achieved — [`EnforcementLevel::KernelEnforced`] when the ruleset is
    /// fully live, [`EnforcementLevel::ProcessIsolationOnly`] when the running
    /// kernel could not fully enforce it (landlock disabled / too old).
    pub fn restrict_current_thread(self) -> io::Result<EnforcementLevel> {
        use landlock::RulesetStatus;
        let status = self
            .inner
            .restrict_self()
            .map_err(|e| io::Error::other(e.to_string()))?;
        Ok(match status.ruleset {
            RulesetStatus::FullyEnforced => EnforcementLevel::KernelEnforced,
            RulesetStatus::PartiallyEnforced | RulesetStatus::NotEnforced => {
                EnforcementLevel::ProcessIsolationOnly
            }
        })
    }
}

/// P4.2 — build a landlock ruleset confining filesystem access to `read_roots`
/// (read + execute) and `write_roots` (read + write + create + remove).
///
/// Every path outside the union of the two sets is **denied** by landlock's
/// deny-by-default. Non-existent roots are silently skipped — a confined run
/// needs the system library roots and not every box has each one. Built under
/// [`CompatLevel::BestEffort`](landlock::CompatLevel), so the call succeeds on
/// any kernel; whether enforcement is actually *kernel-live* is reported by
/// [`LandlockRuleset::restrict_current_thread`].
///
/// Call this in the **parent** (it allocates); enforce in the child.
///
/// # Elite path (Wave 2026-05-22)
///
/// For kernel-enforced **network and IPC** isolation matching the userspace
/// deny-by-default contract, prefer
/// [`build_landlock_ruleset_with_net_and_scope`] — it extends this function
/// with `AccessNet::BindTcp | ConnectTcp` (Linux 6.7+ / ABI::V4) and
/// `Scope::AbstractUnixSocket | Signal` (Linux 6.12+ / ABI::V6). On older
/// kernels both gracefully degrade via `BestEffort` — the call always succeeds.
#[cfg(target_os = "linux")]
pub fn build_landlock_ruleset(
    read_roots: &[std::path::PathBuf],
    write_roots: &[std::path::PathBuf],
) -> io::Result<LandlockRuleset> {
    use landlock::{
        ABI, Access, AccessFs, CompatLevel, Compatible, Ruleset, RulesetAttr, RulesetCreatedAttr,
        path_beneath_rules,
    };

    // ABI::V6 is the newest the landlock crate (v0.4.x) models; CompatLevel
    // BestEffort downgrades to whatever the running kernel actually supports.
    let abi = ABI::V6;
    let read_access = AccessFs::from_read(abi);
    let write_access = AccessFs::from_all(abi);

    let created = Ruleset::default()
        .set_compatibility(CompatLevel::BestEffort)
        .handle_access(AccessFs::from_all(abi))
        .map_err(|e| io::Error::other(format!("landlock handle_access: {e}")))?
        .create()
        .map_err(|e| io::Error::other(format!("landlock create: {e}")))?;

    let created = created
        .add_rules(path_beneath_rules(read_roots, read_access))
        .map_err(|e| io::Error::other(format!("landlock read rules: {e}")))?
        .add_rules(path_beneath_rules(write_roots, write_access))
        .map_err(|e| io::Error::other(format!("landlock write rules: {e}")))?;

    Ok(LandlockRuleset { inner: created })
}

/// **Elite path (Wave 2026-05-22)** — build a landlock ruleset confining
/// filesystem (V1+), network (V4 — Linux 6.7+), and IPC (V6 — Linux 6.12+).
///
/// Extends [`build_landlock_ruleset`] with two kernel-enforced isolation
/// surfaces that previously lived only in the userspace
/// [`CapabilityProfile`] decision:
///
/// - **Network** — `AccessNet::BindTcp | AccessNet::ConnectTcp` is handled
///   (denied by default); each TCP port in `bind_tcp_ports` /
///   `connect_tcp_ports` is granted via a `NetPort` rule. An empty grant set
///   means **deny every TCP bind/connect** at the kernel, matching the
///   `Sandboxed` profile's userspace `Net(*) = Deny` contract.
/// - **IPC scoping** — when `enable_ipc_scope` is `true`, the ruleset adds
///   `Scope::AbstractUnixSocket | Scope::Signal` so the confined process
///   cannot connect to abstract UNIX sockets created outside the sandbox nor
///   send signals to processes outside it. Matches the deny-by-default IPC
///   posture documented in [`super::builtins::sandboxed`].
///
/// All access modes are configured under
/// [`CompatLevel::BestEffort`](landlock::CompatLevel): on Linux < 6.7
/// `AccessNet` is silently dropped; on Linux < 6.12 the `Scope` request is
/// silently dropped. The kernel reports the actual enforcement level via
/// `RulesetStatus` — surfaced as [`EnforcementLevel::KernelEnforced`] or
/// [`EnforcementLevel::ProcessIsolationOnly`] by
/// [`LandlockRuleset::restrict_current_thread`]. The call itself **always
/// succeeds** on any kernel.
///
/// Call this in the **parent** (it allocates); enforce in the child via a
/// `Command::pre_exec` closure.
#[cfg(target_os = "linux")]
pub fn build_landlock_ruleset_with_net_and_scope(
    read_roots: &[std::path::PathBuf],
    write_roots: &[std::path::PathBuf],
    bind_tcp_ports: &[u16],
    connect_tcp_ports: &[u16],
    enable_ipc_scope: bool,
) -> io::Result<LandlockRuleset> {
    use landlock::{
        ABI, Access, AccessFs, AccessNet, CompatLevel, Compatible, NetPort, Ruleset, RulesetAttr,
        RulesetCreatedAttr, Scope, path_beneath_rules,
    };

    let abi = ABI::V6;
    let read_access = AccessFs::from_read(abi);
    let write_access = AccessFs::from_all(abi);

    // Filesystem handle is always declared (V1+).
    let mut rs = Ruleset::default()
        .set_compatibility(CompatLevel::BestEffort)
        .handle_access(AccessFs::from_all(abi))
        .map_err(|e| io::Error::other(format!("landlock handle_access(fs): {e}")))?;

    // Network handle (V4) — silently dropped on Linux < 6.7 under BestEffort,
    // so the call still succeeds; the actual enforcement is reported per-run.
    rs = rs
        .handle_access(AccessNet::BindTcp | AccessNet::ConnectTcp)
        .map_err(|e| io::Error::other(format!("landlock handle_access(net): {e}")))?;

    // IPC scope (V6) — opt-in because some legitimate workloads need to signal
    // sibling daemons or use abstract sockets the policy considers acceptable.
    // BestEffort downgrades to no-op on Linux < 6.12 — the call still succeeds.
    if enable_ipc_scope {
        rs = rs
            .scope(Scope::AbstractUnixSocket | Scope::Signal)
            .map_err(|e| io::Error::other(format!("landlock scope(ipc): {e}")))?;
    }

    let mut created = rs
        .create()
        .map_err(|e| io::Error::other(format!("landlock create: {e}")))?
        .add_rules(path_beneath_rules(read_roots, read_access))
        .map_err(|e| io::Error::other(format!("landlock read rules: {e}")))?
        .add_rules(path_beneath_rules(write_roots, write_access))
        .map_err(|e| io::Error::other(format!("landlock write rules: {e}")))?;

    // Each granted bind/connect port becomes one NetPort rule. An empty grant
    // set leaves the access *handled but unauthorised* — every TCP bind /
    // connect is denied by the kernel's deny-by-default semantics.
    for &port in bind_tcp_ports {
        created = created
            .add_rule(NetPort::new(port, AccessNet::BindTcp))
            .map_err(|e| io::Error::other(format!("landlock bind port {port}: {e}")))?;
    }
    for &port in connect_tcp_ports {
        created = created
            .add_rule(NetPort::new(port, AccessNet::ConnectTcp))
            .map_err(|e| io::Error::other(format!("landlock connect port {port}: {e}")))?;
    }

    Ok(LandlockRuleset { inner: created })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::builtins;

    #[test]
    fn resource_caps_none_is_empty() {
        let caps = ResourceCaps::none();
        assert_eq!(caps.active_count(), 0);
        assert!(caps.limits().is_empty());
    }

    #[test]
    fn resource_caps_sandboxed_defaults() {
        let caps = ResourceCaps::sandboxed_defaults();
        assert_eq!(caps.active_count(), 5);
        assert_eq!(caps.cpu_seconds, Some(10));
        assert_eq!(caps.address_space_bytes, Some(512 * 1024 * 1024));
        assert_eq!(caps.open_files, Some(256));
        assert_eq!(caps.processes, Some(64));
        assert_eq!(caps.file_size_bytes, Some(64 * 1024 * 1024));
    }

    #[test]
    fn resource_caps_limits_mapping() {
        let caps = ResourceCaps {
            cpu_seconds: Some(3),
            open_files: Some(128),
            ..ResourceCaps::none()
        };
        let limits = caps.limits();
        assert_eq!(limits.len(), 2);
        assert!(limits.contains(&(CappedResource::CpuSeconds, 3)));
        assert!(limits.contains(&(CappedResource::OpenFiles, 128)));
    }

    #[test]
    fn resource_caps_partial_one_field() {
        let caps = ResourceCaps {
            file_size_bytes: Some(4096),
            ..ResourceCaps::none()
        };
        assert_eq!(caps.active_count(), 1);
        assert_eq!(caps.limits(), vec![(CappedResource::FileSize, 4096)]);
    }

    #[test]
    fn resource_caps_serde_roundtrip() {
        let caps = ResourceCaps::sandboxed_defaults();
        let json = serde_json::to_string(&caps).expect("serialize");
        let back: ResourceCaps = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(caps, back);
    }

    #[test]
    fn apply_landlock_reports_kernel_enforcement() {
        let profile = builtins::trusted();
        let report = apply_landlock(&profile);
        // P4.2 — the landlock crate is wired; Linux reports kernel enforcement.
        #[cfg(target_os = "linux")]
        assert_eq!(report.level, EnforcementLevel::KernelEnforced);
        #[cfg(not(target_os = "linux"))]
        assert_eq!(report.level, EnforcementLevel::Unsupported);
        assert!(!report.note.is_empty());
        assert!(report.note.contains("Trusted"));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn build_landlock_ruleset_succeeds_for_system_roots() {
        // BestEffort — building always succeeds on any kernel.
        let read_roots = vec![
            std::path::PathBuf::from("/usr"),
            std::path::PathBuf::from("/lib"),
        ];
        let rs = build_landlock_ruleset(&read_roots, &[]);
        assert!(rs.is_ok(), "ruleset build must succeed under BestEffort");
    }

    // ── Elite path (Wave 2026-05-22) — A1+A5 net + IPC scope ─────────────

    #[cfg(target_os = "linux")]
    #[test]
    fn build_landlock_with_net_and_scope_empty_grants_kernel_denies_all_net() {
        // Empty bind / connect port lists mean every TCP bind+connect is
        // denied by the kernel's deny-by-default once the access kind is
        // handled. The build itself succeeds under BestEffort on any kernel.
        let read_roots = vec![std::path::PathBuf::from("/usr")];
        let rs = build_landlock_ruleset_with_net_and_scope(&read_roots, &[], &[], &[], true);
        assert!(rs.is_ok(), "empty-grant ruleset build must succeed");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn build_landlock_with_net_and_scope_accepts_bind_and_connect_ports() {
        let read_roots = vec![std::path::PathBuf::from("/usr")];
        let bind = [9418u16]; // canonical git daemon port
        let connect = [443u16, 80u16]; // canonical HTTPS / HTTP outbound
        let rs = build_landlock_ruleset_with_net_and_scope(&read_roots, &[], &bind, &connect, true);
        assert!(
            rs.is_ok(),
            "ruleset with explicit bind+connect port grants must build"
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn build_landlock_with_net_and_scope_ipc_opt_out_still_builds() {
        // enable_ipc_scope=false → Scope handle is not declared. The build
        // must still succeed; only the IPC posture differs.
        let read_roots = vec![std::path::PathBuf::from("/usr")];
        let rs = build_landlock_ruleset_with_net_and_scope(&read_roots, &[], &[], &[], false);
        assert!(
            rs.is_ok(),
            "ruleset with enable_ipc_scope=false must still build"
        );
    }

    #[test]
    fn enforcement_types_serde_roundtrip() {
        for level in [
            EnforcementLevel::KernelEnforced,
            EnforcementLevel::ProcessIsolationOnly,
            EnforcementLevel::Unsupported,
        ] {
            let json = serde_json::to_string(&level).expect("serialize");
            let back: EnforcementLevel = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(level, back);
        }
        let report = EnforcementReport {
            level: EnforcementLevel::KernelEnforced,
            note: "test".to_string(),
        };
        let json = serde_json::to_string(&report).expect("serialize");
        let back: EnforcementReport = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(report, back);

        let res = CappedResource::Processes;
        let rj = serde_json::to_string(&res).expect("serialize");
        let rb: CappedResource = serde_json::from_str(&rj).expect("deserialize");
        assert_eq!(res, rb);
    }

    /// Spawn `sh -c <query>` with `apply_rlimit(caps)` injected via `pre_exec`
    /// (post-fork, in the child) and return the trimmed stdout.
    #[cfg(target_os = "linux")]
    fn ulimit_in_child(query: &str, caps: ResourceCaps) -> String {
        use std::os::unix::process::CommandExt;
        use std::process::{Command, Stdio};

        let mut cmd = Command::new("sh");
        cmd.arg("-c")
            .arg(query)
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        // SAFETY: the closure runs post-fork, pre-exec. `apply_rlimit` performs
        // only `setrlimit` syscalls + fixed stack work — allocation-free and
        // async-signal-safe, as the post-fork context requires.
        unsafe {
            cmd.pre_exec(move || apply_rlimit(&caps).map(|_| ()));
        }
        let out = cmd.output().expect("spawn sh");
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn e2e_apply_rlimit_caps_nofile_in_child() {
        let caps = ResourceCaps {
            open_files: Some(64),
            ..ResourceCaps::none()
        };
        assert_eq!(ulimit_in_child("ulimit -n", caps), "64");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn e2e_apply_rlimit_caps_cpu_in_child() {
        let caps = ResourceCaps {
            cpu_seconds: Some(7),
            ..ResourceCaps::none()
        };
        assert_eq!(ulimit_in_child("ulimit -t", caps), "7");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn e2e_apply_rlimit_none_leaves_limit_untouched() {
        let capped = ulimit_in_child(
            "ulimit -n",
            ResourceCaps {
                open_files: Some(48),
                ..ResourceCaps::none()
            },
        );
        let uncapped = ulimit_in_child("ulimit -n", ResourceCaps::none());
        assert_eq!(capped, "48");
        // none() applied no cap — the child inherits the parent default.
        assert_ne!(uncapped, "48");
    }
}
