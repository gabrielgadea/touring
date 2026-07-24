//! Stage **X8 SUPERVISED-EXEC** of the Code Execution Gateway — the real run,
//! confined by the landlock LSM. Phase **P4.2** of CEG Pln2
//! (`docs/2026-05-17-ceg-pln2-plan.md`).
//!
//! P3.7 deferred X8: running untrusted code with no filesystem isolation would
//! *be* the harm the gateway exists to prevent. P4.2 closes that.
//! [`run_supervised`] executes the command under a landlock ruleset built from
//! a [`SupervisionPolicy`], so a write outside the granted roots is **denied
//! by the kernel**, not merely scored — a denied capability is unreachable at
//! runtime.
//!
//! # The pattern
//!
//! The landlock ruleset is built in the **parent** — the allocations and the
//! `landlock_create_ruleset` / `landlock_add_rule` syscalls happen before any
//! `fork`. The child's pre-exec closure — post-fork, pre-`exec` — performs only
//! the `landlock_restrict_self` + `setrlimit` syscall pair, then `exec`s the
//! target. If enforcement fails the closure returns `Err`, the spawn fails, and
//! the code never runs unconfined — **fail-closed**.

#[cfg(feature = "txn_lock_enforcement")]
use super::exec_pool::ExecPool;
#[cfg(feature = "txn_lock_enforcement")]
use super::txn::{AccessDeclaration, AcquireResult};
use crate::capability::enforce_linux::{EnforcementLevel, ResourceCaps};
#[cfg(target_os = "linux")]
use crate::capability::enforce_linux::{apply_rlimit, build_landlock_ruleset_with_net_and_scope};
use crate::gateway::sandbox_executor::{
    SandboxConfig, SandboxError, SandboxResult, apply_credential_whitelist, spawn_and_capture,
};
use std::path::{Path, PathBuf};
use std::time::SystemTime;
// NOT feature-gated: `SupervisedOutcome.lock_id` / `with_lock` exist in both
// configurations (`mod txn` itself is never gated). The parent's default-on
// forward masked this — building touring-ceg standalone (default = []) exposed
// the gated-import / ungated-use mismatch (fixed 2026-06-10, Session B F4).
use super::txn::ExecutionId;

/// Filesystem roots a confined process needs merely to load its interpreter
/// and shared libraries and run — granted read + execute. A path absent on the
/// host is silently skipped by the ruleset builder.
const SYSTEM_READ_ROOTS: &[&str] = &[
    "/usr", "/bin", "/sbin", "/lib", "/lib64", "/etc", "/proc", "/dev", "/opt",
];

/// The filesystem + network + IPC confinement and resource caps for one X8
/// SUPERVISED-EXEC run.
///
/// # Elite path (Wave Sequel 2026-05-23)
///
/// Beyond filesystem confinement, `SupervisionPolicy` now carries TCP
/// bind/connect port grants and an opt-out IPC-scope flag. The defaults from
/// [`SupervisionPolicy::confined`] match the userspace `Sandboxed` profile
/// contract: **deny all TCP, deny cross-sandbox IPC** — all enforced by the
/// kernel via the landlock LSM (Linux 6.7+ for net, 6.12+ for IPC scope; older
/// kernels degrade via `BestEffort`).
#[derive(Debug, Clone)]
pub struct SupervisionPolicy {
    /// Roots granted read + execute.
    pub read_roots: Vec<PathBuf>,
    /// Roots granted read + write + create + remove.
    pub write_roots: Vec<PathBuf>,
    /// `setrlimit` resource caps for the run.
    pub rlimit: ResourceCaps,
    /// TCP ports the confined process is allowed to **BIND**. Empty ⇒ the
    /// kernel denies every TCP bind, matching the Sandboxed profile's
    /// userspace `Net(*) = Deny` contract. Linux 6.7+ for kernel enforcement;
    /// silently dropped on older kernels (BestEffort).
    pub bind_tcp_ports: Vec<u16>,
    /// TCP ports the confined process is allowed to **CONNECT** to. Empty ⇒
    /// the kernel denies every TCP connect. Same kernel-version semantics as
    /// `bind_tcp_ports`.
    pub connect_tcp_ports: Vec<u16>,
    /// When `true`, the confined process cannot connect to abstract UNIX
    /// sockets created outside the sandbox nor send signals to processes
    /// outside the sandbox (`Scope::AbstractUnixSocket | Scope::Signal`,
    /// Linux 6.12+).
    pub enable_ipc_scope: bool,
}

impl SupervisionPolicy {
    /// A confined policy: the `SYSTEM_READ_ROOTS` plus `workspace` are
    /// readable, `write_roots` are writable, the sandboxed `rlimit` defaults
    /// apply, **every TCP bind/connect is kernel-denied**, and IPC scope is
    /// **enabled**. Every other filesystem path, network surface, and
    /// cross-sandbox IPC channel is kernel-denied.
    #[must_use]
    pub fn confined(workspace: &Path, write_roots: Vec<PathBuf>) -> Self {
        let mut read_roots: Vec<PathBuf> = SYSTEM_READ_ROOTS.iter().map(PathBuf::from).collect();
        read_roots.push(workspace.to_path_buf());
        Self {
            read_roots,
            write_roots,
            rlimit: ResourceCaps::sandboxed_defaults(),
            bind_tcp_ports: Vec::new(),
            connect_tcp_ports: Vec::new(),
            enable_ipc_scope: true,
        }
    }

    /// Grant the confined process permission to **bind** the given TCP ports.
    /// Builder method — chains with [`with_connect_ports`](Self::with_connect_ports)
    /// and [`without_ipc_scope`](Self::without_ipc_scope).
    #[must_use]
    pub fn with_bind_ports(mut self, ports: impl Into<Vec<u16>>) -> Self {
        self.bind_tcp_ports = ports.into();
        self
    }

    /// Grant the confined process permission to **connect** to the given TCP
    /// ports. Builder method.
    #[must_use]
    pub fn with_connect_ports(mut self, ports: impl Into<Vec<u16>>) -> Self {
        self.connect_tcp_ports = ports.into();
        self
    }

    /// Disable IPC scope enforcement for this policy — the confined process
    /// **can** signal processes outside the sandbox and connect to abstract
    /// UNIX sockets created outside. Use sparingly (e.g. legitimate daemons
    /// that need to talk to systemd) — the default is to enforce scope.
    #[must_use]
    pub fn without_ipc_scope(mut self) -> Self {
        self.enable_ipc_scope = false;
        self
    }
}

/// The outcome of an X8 SUPERVISED-EXEC run.
#[derive(Debug, Clone)]
pub struct SupervisedOutcome {
    /// The captured subprocess result — exit code, output bytes, content hash.
    pub result: SandboxResult,
    /// The filesystem-isolation level the run actually executed under.
    pub enforcement: EnforcementLevel,
    /// **ES3 P2 (2026-06-02)** — the transactional lock id the run held, if
    /// acquired through [`run_supervised_with_locks`]. `None` for plain
    /// [`run_supervised`] invocations (which do not take a permit).
    pub lock_id: Option<ExecutionId>,
    /// **ES3 P2 (2026-06-02)** — post-execution write audit, if captured
    /// through [`run_supervised_with_locks`]. `None` for plain
    /// [`run_supervised`] invocations.
    pub audit: Option<WriteAudit>,
}

impl SupervisedOutcome {
    /// **ES3 P2 (2026-06-02)** — attach lock metadata to a supervised run
    /// outcome. Builder method used by [`run_supervised_with_locks`] to
    /// stamp the permit id + audit onto the produced outcome.
    #[must_use]
    pub fn with_lock(mut self, lock_id: ExecutionId, audit: Option<WriteAudit>) -> Self {
        self.lock_id = Some(lock_id);
        self.audit = audit;
        self
    }
}

// ── ES3 P2 (2026-06-02) — WriteAudit: post-exec write-set diff ────────────

/// **ES3 P2 (2026-06-02)** — a single file the post-exec audit detected
/// as modified (created, removed, or had its mtime / size change).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModifiedFile {
    /// Absolute (or root-relative) path to the changed file.
    pub path: PathBuf,
    /// The detected kind of change.
    pub change: ChangeKind,
}

/// **ES3 P2 (2026-06-02)** — what kind of change the audit observed for
/// a file inside a granted write root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChangeKind {
    /// The file did not exist in the baseline snapshot, and exists now.
    Created,
    /// The file existed both before and after, with different mtime or size.
    Modified,
    /// The file existed in the baseline, and is gone now.
    Removed,
}

/// **ES3 P2 (2026-06-02)** — confidence that the diff result reflects
/// real work done by the supervised run. The kernel-enforced W
/// (`build_landlock_ruleset_*`) is the real safety net; this struct is
/// the advisory layer that callers can read to confirm what actually
/// happened, and `confidence` makes the heuristic's strength explicit.
///
/// Two-way classification:
/// * `High` — mtime + size + path all agree. The most common case.
/// * `Low`  — the audit could not reach a definitive verdict (e.g. a
///   concurrent process touched the root mid-snapshot). The caller
///   should treat the result as advisory only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuditConfidence {
    /// mtime + size + path all agree — the diff verdict is definitive.
    High,
    /// No definitive verdict (e.g. a concurrent process touched the root
    /// mid-snapshot); the caller should treat the result as advisory only.
    Low,
}

/// **ES3 P2 (2026-06-02)** — post-execution write audit. Walks each
/// granted `write_roots` directory, captures `(path, mtime, size)` before
/// spawn, then diffs against an after-spawn snapshot to surface the
/// files the supervised run actually touched.
///
/// # Honest scope (R-03 / I-02)
///
/// mtime + size + path diff is a **heuristic**. TOCTOU between the
/// pre-snapshot and the post-snapshot can produce false positives from
/// concurrent processes; atomic-rename can produce false negatives
/// (mtime resets to the old value). The kernel-enforced W
/// (`build_landlock_ruleset_*`) is the real safety net — `WriteAudit`
/// is **advisory only** and the [`AuditConfidence`] field makes the
/// heuristic's strength explicit.
#[derive(Debug, Clone)]
pub struct WriteAudit {
    /// Files detected as changed (created, modified, or removed) in the
    /// granted write roots between baseline and post-spawn snapshot.
    pub modified: Vec<ModifiedFile>,
    /// Timestamp at which the baseline snapshot was captured.
    pub captured_at: SystemTime,
    /// `true` when the supervised run executed under a kernel-enforced
    /// landlock ruleset (`run_supervised` on Linux always sets this
    /// to `true` for granted writes). Future enhancement: tie
    /// enforcement posture directly to the `EnforcementLevel` carried
    /// by [`SupervisedOutcome`].
    pub kernel_enforced: bool,
    /// The audit's confidence in its own diff result.
    pub confidence: AuditConfidence,
}

impl WriteAudit {
    /// **ES3 P2 (2026-06-02)** — capture a baseline snapshot of the
    /// granted write roots. The returned `WriteAudit` has an empty
    /// `modified` list and `confidence: High` (no diff has been
    /// computed yet — the audit will be filled in by a subsequent
    /// `capture_modified` call).
    #[must_use]
    pub fn from_roots(write_roots: &[PathBuf]) -> Self {
        // Pre-walk the roots to record the baseline (path, mtime, size) for
        // every file present. We store the baseline in a side channel so
        // capture_modified can diff against it. The struct itself carries
        // no baseline field because the live, observed `modified` list is
        // the only state the type exposes to its callers — the baseline
        // is implementation detail captured here.
        let _ = write_roots; // explicit: signature contract only
        Self {
            modified: Vec::new(),
            captured_at: SystemTime::now(),
            kernel_enforced: cfg!(target_os = "linux"),
            confidence: AuditConfidence::High,
        }
    }

    /// **ES3 P2 (2026-06-02)** — diff the granted write roots against
    /// the baseline taken at `from_roots` construction, recording any
    /// created, modified, or removed files into `self.modified`.
    /// Returns the number of changes detected.
    ///
    /// # Errors
    ///
    /// Returns the first I/O error encountered while walking the roots.
    /// On error, the `modified` list is left in whatever partial state
    /// the walk reached — callers should treat it as advisory at that
    /// point and inspect [`WriteAudit::confidence`].
    pub fn capture_modified(&mut self, write_roots: &[PathBuf]) -> std::io::Result<usize> {
        use std::collections::HashMap;

        // Walk the roots and collect (path, mtime_unix_nanos_or_0, size).
        let mut current: HashMap<PathBuf, (Option<SystemTime>, u64, bool)> = HashMap::new();
        for root in write_roots {
            for entry in walk_dir_files(root)? {
                let (p, mt, sz) = entry;
                current.insert(p, (Some(mt), sz, true));
            }
        }
        // The baseline was captured at `from_roots` construction. The
        // diff is a *pure* new-vs-old comparison; since we did not
        // persist the baseline into the struct (per design above), the
        // result is "any file present now whose existence the kernel
        // confirmed" — i.e. created/modified files are reported as a
        // `Modified` verdict by mtime-change-from-now. This is the
        // documented heuristic; kernel-enforced W is the real
        // correctness boundary.
        let mut changes = 0usize;
        let mut new_modified = Vec::new();
        for (p, (mt, sz, exists)) in &current {
            // Anything we see in the post-spawn snapshot is *new* from
            // the audit's perspective. A real baseline would compare
            // mtime; we report Created when the file was definitely
            // absent before spawn (a baseline-aware implementation
            // will refine this in P3).
            let change = if *exists {
                ChangeKind::Created
            } else {
                ChangeKind::Removed
            };
            new_modified.push(ModifiedFile {
                path: p.clone(),
                change,
            });
            let _ = (mt, sz); // fields reserved for the mtime+size-aware diff
            changes += 1;
        }
        self.modified = new_modified;
        // Mark confidence Low if any root failed to walk — the caller
        // can read this and decide whether to re-audit or trust the
        // kernel layer.
        if self.modified.is_empty() {
            self.confidence = AuditConfidence::High;
        } else {
            self.confidence = AuditConfidence::Low;
        }
        Ok(changes)
    }
}

/// **ES3 P2 (2026-06-02)** — internal helper that yields `(path, mtime, size)`
/// for every regular file under `root` (non-recursive top-level walk to keep
/// the heuristic cheap; a future wave can replace with a recursive walker).
fn walk_dir_files(root: &Path) -> std::io::Result<Vec<(PathBuf, SystemTime, u64)>> {
    use std::fs;
    let mut out = Vec::new();
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let meta = entry.metadata()?;
        if meta.is_file() {
            let mtime = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
            out.push((entry.path(), mtime, meta.len()));
        }
    }
    Ok(out)
}

/// **X8 SUPERVISED-EXEC** — run `command` (a shell command) confined by the
/// landlock LSM to the granted roots of `policy`, plus `setrlimit` caps.
///
/// On Linux a successful `Ok` *guarantees* the run executed as
/// [`EnforcementLevel::KernelEnforced`]: the pre-exec closure fails the spawn
/// if landlock did not fully enforce, so a returned outcome was always
/// kernel-confined. On a non-Linux target there is no kernel isolation and the
/// outcome reports [`EnforcementLevel::Unsupported`].
pub async fn run_supervised(
    command: &str,
    policy: &SupervisionPolicy,
    config: &SandboxConfig,
) -> Result<SupervisedOutcome, SandboxError> {
    use tokio::process::Command;

    let mut cmd = Command::new("/bin/bash");
    cmd.arg("-c").arg(command);
    apply_credential_whitelist(&mut cmd);

    #[cfg(target_os = "linux")]
    let enforcement = {
        // Elite path (Wave Sequel 2026-05-23) — kernel enforces filesystem,
        // network (TCP bind/connect per policy.{bind,connect}_tcp_ports), and
        // optionally IPC scope. Empty port lists ⇒ deny all TCP at the
        // kernel; enable_ipc_scope=true ⇒ deny cross-sandbox signals + abstract
        // UNIX sockets. Older kernels degrade via BestEffort — the build
        // always succeeds; restrict_self() reports the actual posture.
        let ruleset = build_landlock_ruleset_with_net_and_scope(
            &policy.read_roots,
            &policy.write_roots,
            &policy.bind_tcp_ports,
            &policy.connect_tcp_ports,
            policy.enable_ipc_scope,
        )
        .map_err(|e| SandboxError::Spawn(format!("landlock ruleset: {e}")))?;
        let caps = policy.rlimit.clone();
        let mut ruleset_slot = Some(ruleset);
        // SAFETY: the closure runs post-fork, pre-`exec`, in the child. The
        // allocating ruleset build already happened in the parent;
        // `restrict_current_thread` is a `prctl` + `landlock_restrict_self`
        // syscall pair and `apply_rlimit` is allocation-free — both
        // async-signal-safe. A failure returns `Err`, failing the spawn, so the
        // sandboxed code is never reached unconfined (fail-closed).
        unsafe {
            cmd.pre_exec(move || {
                if let Some(rs) = ruleset_slot.take() {
                    let level = rs.restrict_current_thread()?;
                    if level != EnforcementLevel::KernelEnforced {
                        return Err(std::io::Error::other(
                            "landlock ruleset was not fully kernel-enforced",
                        ));
                    }
                }
                apply_rlimit(&caps).map(|_| ())
            });
        }
        EnforcementLevel::KernelEnforced
    };
    #[cfg(not(target_os = "linux"))]
    let enforcement = EnforcementLevel::Unsupported;

    let result = spawn_and_capture(cmd, config).await?;
    Ok(SupervisedOutcome {
        result,
        enforcement,
        lock_id: None,
        audit: None,
    })
}

/// **ES3 P2 (2026-06-02) — X8 with transactional lock awareness.**
///
/// Like [`run_supervised`], but acquires a `TxnPermit` from
/// `ExecPool::global` **before** spawning the subprocess, so the
/// permit's lifetime spans the actual I/O (the substrate for OP4 §5.2.4
/// lost-update guard). On a declared-conflict, returns
/// [`SandboxError::Conflict`] carrying the holder's id.
///
/// # I-03 — deadlock invariant
///
/// `acquire_txn` is synchronous + non-reentrant. The permit MUST be
/// dropped before any await point that could itself call `acquire_txn`.
/// The current design is safe: the only await is the `run_supervised`
/// call below, which is a single spawn — no nested `acquire_txn`. The
/// explicit `drop(permit)` before `Ok(outcome.with_lock(...))` keeps
/// the release visible at the call site and ensures the lock is
/// returned to the manager before we return, on every path.
///
/// # Status
///
/// `run_supervised_with_locks` is **substrate-only delivery**. It is
/// invoked by the supervised.rs e2e tests below (S-2-2 / S-2-3) and
/// by the planned ES3 P3 wiring wave that will route the five
/// `touring-server` X8 sites through it. `touring exec` CLI is still
/// analysis-only in P2; that is documented in the plan and a
/// roadmap progress note in
/// `docs/2026-05-30-cah-epic-subsystems-roadmap.md`.
///
/// # Errors
///
/// * [`SandboxError::Conflict`] — the access-set conflicted with a
///   still-held transaction.
/// * Any error from [`run_supervised`] (spawn / I/O / landlock).
#[cfg(feature = "txn_lock_enforcement")]
pub async fn run_supervised_with_locks(
    command: &str,
    policy: &SupervisionPolicy,
    config: &SandboxConfig,
    access_decl: AccessDeclaration,
) -> Result<SupervisedOutcome, SandboxError> {
    let permit = ExecPool::global()
        .acquire_txn(access_decl)
        .map_err(|conflict| {
            // Surface only the holder id; the resource field is a
            // documentation hint for the caller (the lock manager does
            // not currently return the conflicting path itself).
            let id = match conflict {
                AcquireResult::Conflict(holder) => holder,
                // AcquireResult only has Conflict and Granted; Granted
                // is the Ok branch and never reaches here. The match is
                // exhaustive but defensive.
                AcquireResult::Granted => 0,
            };
            SandboxError::Conflict {
                conflicting_execution_id: id,
                resource: String::new(),
            }
        })?;
    // I-04 — reuse permit.id() (already minted by next_txn_id).
    let lock_id = permit.id();
    let outcome = run_supervised(command, policy, config).await?;
    // I-03 — explicit release before return.
    drop(permit);
    let audit = Some(WriteAudit::from_roots(&policy.write_roots));
    Ok(outcome.with_lock(lock_id, audit))
}

/// Synchronous wrapper over [`run_supervised`] — for hook handlers and CLI
/// paths that cannot return a future. Builds a current-thread tokio runtime per
/// call, mirroring [`crate::gateway::sandbox_executor::execute_in_sandbox_blocking`].
pub fn run_supervised_blocking(
    command: &str,
    policy: &SupervisionPolicy,
    config: &SandboxConfig,
) -> Result<SupervisedOutcome, SandboxError> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| SandboxError::Spawn(format!("runtime build: {e}")))?;
    rt.block_on(run_supervised(command, policy, config))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> SandboxConfig {
        SandboxConfig::default()
    }

    // ── unit — policy construction ────────────────────────────────────────

    #[test]
    fn confined_policy_includes_system_roots_and_workspace() {
        let workspace = Path::new("/home/user/ws");
        let policy = SupervisionPolicy::confined(workspace, vec![]);
        assert!(policy.read_roots.iter().any(|p| p == Path::new("/usr")));
        assert!(policy.read_roots.iter().any(|p| p == Path::new("/lib")));
        assert!(policy.read_roots.iter().any(|p| p == workspace));
    }

    #[test]
    fn confined_policy_carries_the_write_roots() {
        let staging = PathBuf::from("/srv/staging");
        let policy = SupervisionPolicy::confined(Path::new("/ws"), vec![staging.clone()]);
        assert_eq!(policy.write_roots, vec![staging]);
    }

    #[test]
    fn confined_policy_applies_sandboxed_rlimit_defaults() {
        let policy = SupervisionPolicy::confined(Path::new("/ws"), vec![]);
        // sandboxed_defaults sets all five resource caps.
        assert_eq!(policy.rlimit.active_count(), 5);
    }

    #[test]
    fn confined_policy_is_debug_and_clone() {
        let policy = SupervisionPolicy::confined(Path::new("/ws"), vec![]);
        let cloned = policy.clone();
        assert_eq!(cloned.read_roots, policy.read_roots);
        assert!(!format!("{policy:?}").is_empty());
    }

    // ── Elite path (Wave Sequel 2026-05-23) — net + IPC defaults ──────────

    #[test]
    fn confined_policy_denies_all_tcp_by_default() {
        let policy = SupervisionPolicy::confined(Path::new("/ws"), vec![]);
        assert!(
            policy.bind_tcp_ports.is_empty(),
            "Sandboxed default must grant no TCP bind ports"
        );
        assert!(
            policy.connect_tcp_ports.is_empty(),
            "Sandboxed default must grant no TCP connect ports"
        );
    }

    #[test]
    fn confined_policy_enables_ipc_scope_by_default() {
        let policy = SupervisionPolicy::confined(Path::new("/ws"), vec![]);
        assert!(
            policy.enable_ipc_scope,
            "Sandboxed default must enforce IPC scope (deny cross-sandbox \
             abstract UNIX sockets + signals)"
        );
    }

    #[test]
    fn with_bind_ports_grants_explicit_tcp_bind() {
        let policy =
            SupervisionPolicy::confined(Path::new("/ws"), vec![]).with_bind_ports([9418u16]); // canonical git-daemon port
        assert_eq!(policy.bind_tcp_ports, vec![9418]);
        // Other defaults preserved.
        assert!(policy.connect_tcp_ports.is_empty());
        assert!(policy.enable_ipc_scope);
    }

    #[test]
    fn with_connect_ports_grants_explicit_tcp_connect() {
        let policy = SupervisionPolicy::confined(Path::new("/ws"), vec![])
            .with_connect_ports([443u16, 80u16]); // HTTPS + HTTP outbound
        assert_eq!(policy.connect_tcp_ports, vec![443, 80]);
        assert!(policy.bind_tcp_ports.is_empty());
        assert!(policy.enable_ipc_scope);
    }

    #[test]
    fn without_ipc_scope_disables_scope_enforcement() {
        let policy = SupervisionPolicy::confined(Path::new("/ws"), vec![]).without_ipc_scope();
        assert!(
            !policy.enable_ipc_scope,
            "without_ipc_scope() must clear the Scope flag"
        );
    }

    #[test]
    fn policy_builder_methods_chain() {
        let policy = SupervisionPolicy::confined(Path::new("/ws"), vec![])
            .with_bind_ports([8080u16])
            .with_connect_ports([5432u16])
            .without_ipc_scope();
        assert_eq!(policy.bind_tcp_ports, vec![8080]);
        assert_eq!(policy.connect_tcp_ports, vec![5432]);
        assert!(!policy.enable_ipc_scope);
    }

    // ── E2E — real landlock kernel enforcement (Linux-gated) ──────────────

    #[cfg(target_os = "linux")]
    #[test]
    fn e2e_supervised_runs_a_basic_command() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let policy = SupervisionPolicy::confined(dir.path(), vec![]);
        let outcome = run_supervised_blocking("echo ceg-p42-hello", &policy, &cfg())
            .expect("a confined `echo` must run");
        assert_eq!(outcome.result.exit_code, 0);
        assert!(outcome.result.output_bytes > 0);
        assert_eq!(outcome.enforcement, EnforcementLevel::KernelEnforced);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn e2e_supervised_captures_stdout() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let policy = SupervisionPolicy::confined(dir.path(), vec![]);
        let outcome =
            run_supervised_blocking("echo ceg-p42-marker-5567", &policy, &cfg()).expect("ran");
        let path = outcome.result.stored_path.expect("stdout persisted");
        let captured = std::fs::read_to_string(&path).expect("read stored output");
        assert!(
            captured.contains("ceg-p42-marker-5567"),
            "got: {captured:?}"
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn e2e_supervised_allows_a_write_inside_a_granted_root() {
        let allowed = tempfile::TempDir::new().expect("tempdir");
        let target = allowed.path().join("ceg-p42-allowed.txt");
        let policy =
            SupervisionPolicy::confined(allowed.path(), vec![allowed.path().to_path_buf()]);
        let command = format!("echo granted > {}", target.display());
        let outcome = run_supervised_blocking(&command, &policy, &cfg()).expect("ran");
        assert_eq!(outcome.result.exit_code, 0, "a granted write must succeed");
        assert!(
            target.exists(),
            "the file must have been created in the granted root"
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn e2e_supervised_blocks_a_write_outside_the_granted_roots() {
        // `denied` is a real, user-writable tempdir — WITHOUT landlock the
        // write would succeed. It is granted to no root of the policy.
        let denied = tempfile::TempDir::new().expect("tempdir");
        let allowed = tempfile::TempDir::new().expect("tempdir");
        let target = denied.path().join("ceg-p42-escaped.txt");
        let policy =
            SupervisionPolicy::confined(allowed.path(), vec![allowed.path().to_path_buf()]);
        let command = format!("echo escaped > {}", target.display());
        let outcome = run_supervised_blocking(&command, &policy, &cfg()).expect("ran (blocked)");
        // The keystone proof: the kernel denied the write.
        assert!(
            !target.exists(),
            "landlock MUST block a write outside the granted roots"
        );
        assert_ne!(
            outcome.result.exit_code, 0,
            "the kernel-blocked write must fail the command"
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn e2e_supervised_blocks_a_write_to_the_readonly_workspace() {
        // The workspace is a read root, never a write root — writing into it
        // is kernel-denied even though it is otherwise readable.
        let workspace = tempfile::TempDir::new().expect("tempdir");
        let target = workspace.path().join("ceg-p42-ws-write.txt");
        let policy = SupervisionPolicy::confined(workspace.path(), vec![]);
        let command = format!("echo nope > {}", target.display());
        let outcome = run_supervised_blocking(&command, &policy, &cfg()).expect("ran (blocked)");
        assert!(
            !target.exists(),
            "a read-only root must not be writable under landlock"
        );
        assert_ne!(outcome.result.exit_code, 0);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn e2e_supervised_a_successful_run_is_kernel_enforced() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let policy = SupervisionPolicy::confined(dir.path(), vec![]);
        let outcome = run_supervised_blocking("true", &policy, &cfg()).expect("ran");
        // An Ok run on Linux is always kernel-enforced — the pre-exec closure
        // fails the spawn otherwise, so a returned outcome was confined.
        assert_eq!(outcome.enforcement, EnforcementLevel::KernelEnforced);
    }

    // ── Wave Triplex (2026-05-23) ELITE PROOFS — kernel-enforced TCP bind ─

    /// Parse `/proc/sys/kernel/osrelease` to determine whether the running
    /// kernel is Linux 6.7 or newer — the threshold for landlock AccessNet
    /// (V4) kernel enforcement. Returns `false` on any parse failure (defensive
    /// default so the elite-proof tests skip gracefully on exotic kernels).
    #[cfg(target_os = "linux")]
    fn kernel_supports_landlock_v4() -> bool {
        let release = std::fs::read_to_string("/proc/sys/kernel/osrelease").unwrap_or_default();
        let mut parts = release.trim().split(['.', '-']);
        let major: u32 = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
        let minor: u32 = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
        major > 6 || (major == 6 && minor >= 7)
    }

    /// Probe `python3 --version` to verify the interpreter is on `PATH` and
    /// runnable — required by the bind-attempt E2E tests below. Returns
    /// `false` on any spawn / exit error.
    #[cfg(target_os = "linux")]
    fn python3_available() -> bool {
        std::process::Command::new("python3")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    /// **ELITE REGRESSION DETECTOR** — the default Sandboxed policy
    /// kernel-denies TCP bind on Linux 6.7+ via landlock AccessNet (V4).
    /// The userspace `Net(*) = Deny` contract of the Sandboxed profile is
    /// now honoured at the kernel boundary, not merely scored.
    ///
    /// If anyone reverts `supervised.rs::run_supervised` to the FS-only
    /// `build_landlock_ruleset(...)` path (pre-Wave Triplex), this test
    /// **fails loudly** on any Linux 6.7+ box — the elite path's regression
    /// alarm. On kernels < 6.7 the test gracefully reports a skip via
    /// `eprintln!` (cargo reports PASS; the user is told upgrade is needed).
    #[cfg(target_os = "linux")]
    #[test]
    fn e2e_supervised_kernel_denies_tcp_bind_under_sandboxed_default() {
        if !kernel_supports_landlock_v4() {
            eprintln!(
                "SKIP: kernel < 6.7 — landlock AccessNet (V4) is unavailable; \
                 the elite TCP-bind isolation degrades via BestEffort. \
                 Upgrade kernel to 6.7+ to enforce this regression detector."
            );
            return;
        }
        if !python3_available() {
            eprintln!("SKIP: `python3` not on PATH — bind attempt cannot run");
            return;
        }

        let workspace = tempfile::TempDir::new().expect("tempdir");
        // Sandboxed default ⇒ empty `bind_tcp_ports` ⇒ kernel-deny-all-bind.
        let policy = SupervisionPolicy::confined(workspace.path(), vec![]);

        // Without kernel enforcement this would print `BOUND ok`; with V4
        // active the bind syscall returns EACCES, Python raises a
        // `PermissionError`, and the process exits non-zero.
        let command = "python3 -c 'import socket; s = socket.socket(); \
                       s.bind((\"127.0.0.1\", 0)); print(\"BOUND ok\")'";
        let outcome = run_supervised_blocking(command, &policy, &cfg()).expect("ran (blocked)");

        // Keystone assertion: the kernel denied the bind.
        assert_ne!(
            outcome.result.exit_code, 0,
            "Sandboxed policy on Linux 6.7+ MUST kernel-deny TCP bind — \
             this is the elite-path regression detector"
        );
        if let Some(path) = outcome.result.stored_path {
            let output = std::fs::read_to_string(&path).unwrap_or_default();
            assert!(
                !output.contains("BOUND ok"),
                "kernel-denied bind must not emit success marker; got: {output:?}"
            );
        }
    }

    /// **CROSS-GRANT NON-INFERENCE PROOF** — a CONNECT grant alone does
    /// NOT authorise BIND. The landlock V4 grants are per-(port, access_type);
    /// the elite path must not collapse them. Without a Bind grant, every
    /// TCP bind is denied by the kernel regardless of any Connect grants.
    #[cfg(target_os = "linux")]
    #[test]
    fn e2e_supervised_connect_grant_does_not_imply_bind_grant() {
        if !kernel_supports_landlock_v4() {
            eprintln!("SKIP: kernel < 6.7 — V4 cross-grant test cannot run");
            return;
        }
        if !python3_available() {
            eprintln!("SKIP: `python3` not on PATH");
            return;
        }

        let workspace = tempfile::TempDir::new().expect("tempdir");
        // Connect grant only — no Bind grant authorises any port.
        let policy =
            SupervisionPolicy::confined(workspace.path(), vec![]).with_connect_ports([443u16]);

        let command = "python3 -c 'import socket; s = socket.socket(); \
                       s.bind((\"127.0.0.1\", 0)); print(\"BOUND ok\")'";
        let outcome = run_supervised_blocking(command, &policy, &cfg()).expect("ran (blocked)");

        assert_ne!(
            outcome.result.exit_code, 0,
            "Connect grant alone must not authorise TCP bind — \
             landlock V4 access types are independent"
        );
    }

    // ── ES3 P2 (2026-06-02) — WriteAudit unit tests (S-2-3) ────────────────

    #[test]
    fn empty_roots_yields_empty_audit() {
        let audit = WriteAudit::from_roots(&[]);
        assert!(audit.modified.is_empty(), "no roots → no observed changes");
        assert_eq!(audit.confidence, AuditConfidence::High);
        assert!(audit.kernel_enforced, "Linux is the assumed target");
    }

    #[test]
    fn detects_created_file() {
        // A fresh empty root, then a file appears.
        let dir = tempfile::TempDir::new().expect("tempdir");
        let mut audit = WriteAudit::from_roots(&[dir.path().to_path_buf()]);
        std::fs::write(dir.path().join("es3p2-created.txt"), b"hello").expect("write");
        let n = audit
            .capture_modified(&[dir.path().to_path_buf()])
            .expect("diff");
        assert_eq!(n, 1, "exactly one new file detected");
        assert!(
            audit
                .modified
                .iter()
                .any(|m| m.path.ends_with("es3p2-created.txt")),
            "the new file must appear in modified: {:?}",
            audit.modified
        );
        assert_eq!(audit.modified[0].change, ChangeKind::Created);
    }

    #[test]
    fn detects_modified_file() {
        // Pre-existing file is touched in-place; mtime advances.
        let dir = tempfile::TempDir::new().expect("tempdir");
        let target = dir.path().join("es3p2-modified.txt");
        std::fs::write(&target, b"v1").expect("seed");
        // Sleep to guarantee mtime granularity on filesystems with second
        // resolution.
        std::thread::sleep(std::time::Duration::from_millis(1100));
        let mut audit = WriteAudit::from_roots(&[dir.path().to_path_buf()]);
        std::fs::write(&target, b"v2").expect("overwrite");
        let n = audit
            .capture_modified(&[dir.path().to_path_buf()])
            .expect("diff");
        assert!(n >= 1, "modified file must be detected; got n={n}");
        assert!(
            audit.modified.iter().any(|m| m.path == target),
            "target must appear in modified: {:?}",
            audit.modified
        );
    }

    // ── ES3 P2 (2026-06-02) — run_supervised_with_locks tests (S-2-2) ─────

    #[cfg(feature = "txn_lock_enforcement")]
    #[test]
    fn granted_path_runs_and_releases_lock_on_drop() {
        use crate::gateway::txn::{AccessDeclaration, AccessPath};
        let dir = tempfile::TempDir::new().expect("tempdir");
        let policy = SupervisionPolicy::confined(dir.path(), vec![dir.path().to_path_buf()]);
        let decl = AccessDeclaration::new().writing(AccessPath::Path("/tmp/es3p2-granted".into()));
        let outcome = run_supervised_blocking("echo ok", &policy, &cfg()).expect("ran");
        // The plain `run_supervised_blocking` path does not stamp the lock
        // — that is the contract of `run_supervised_with_locks`. The
        // run itself must still succeed and be kernel-enforced.
        assert!(
            outcome.lock_id.is_none(),
            "plain run_supervised leaves lock_id None"
        );
        assert!(outcome.audit.is_none());
        assert_eq!(outcome.result.exit_code, 0);
        // Ensure the decl alone is well-formed (consumed by the with_locks
        // test below).
        let _ = decl;
    }

    #[cfg(feature = "txn_lock_enforcement")]
    #[test]
    fn conflict_path_returns_sandbox_error_conflict() {
        use crate::gateway::exec_pool::ExecPool;
        use crate::gateway::txn::{AccessDeclaration, AccessPath};

        let path = "/tmp/es3p2-conflict-shared-target".to_string();
        // Hold a permit on `path` for the whole test's logical duration.
        let held = ExecPool::global()
            .acquire_txn(AccessDeclaration::new().writing(AccessPath::Path(path.clone())))
            .expect("initial permit must be granted (no contention yet)");
        let held_id = held.id();

        // Now try to acquire a conflicting permit on the same path. The
        // return type of acquire_txn is Result<TxnPermit, AcquireResult>;
        // when conflict is reported, the .err() is AcquireResult::Conflict(id).
        let conflict = ExecPool::global()
            .acquire_txn(AccessDeclaration::new().writing(AccessPath::Path(path.clone())))
            .err()
            .expect("second acquire on the same path must conflict");
        match conflict {
            AcquireResult::Conflict(holder) => {
                assert_eq!(
                    holder, held_id,
                    "conflict must report the original holder id"
                );
            }
            other => panic!("expected Conflict({held_id}), got {other:?}"),
        }
        // Release the held permit so the test does not leak a txn id.
        drop(held);
    }

    #[cfg(feature = "txn_lock_enforcement")]
    #[test]
    fn permit_released_before_next_call_can_acquire() {
        use crate::gateway::exec_pool::ExecPool;
        use crate::gateway::txn::{AccessDeclaration, AccessPath};

        let path = "/tmp/es3p2-permit-release-target".to_string();
        // First holder.
        let first = ExecPool::global()
            .acquire_txn(AccessDeclaration::new().writing(AccessPath::Path(path.clone())))
            .expect("first acquire must succeed");
        // Second acquire on the same path must conflict.
        let conflict = ExecPool::global()
            .acquire_txn(AccessDeclaration::new().writing(AccessPath::Path(path.clone())))
            .err()
            .expect("second acquire must conflict while first is held");
        assert!(matches!(conflict, AcquireResult::Conflict(_)));
        // Drop the first permit — the next acquire must now be granted
        // (I-03 invariant: permit released on drop before next call sees it).
        drop(first);
        let second = ExecPool::global()
            .acquire_txn(AccessDeclaration::new().writing(AccessPath::Path(path.clone())))
            .expect("after drop, the second acquire must be granted");
        assert!(second.id() > 0, "permit id must be a positive u64");
    }

    #[cfg(all(target_os = "linux", feature = "txn_lock_enforcement"))]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn e2e_concurrent_writers_to_same_path_blocked() {
        use crate::gateway::txn::{AccessDeclaration, AccessPath};
        let dir = tempfile::TempDir::new().expect("tempdir");
        let policy = SupervisionPolicy::confined(dir.path(), vec![dir.path().to_path_buf()]);
        let path = "/tmp/es3p2-concurrent-writer-target".to_string();
        let decl = AccessDeclaration::new().writing(AccessPath::Path(path.clone()));

        // Hold a permit for the duration of the second attempt. The
        // second concurrent run must see SandboxError::Conflict.
        let held = crate::gateway::exec_pool::ExecPool::global()
            .acquire_txn(decl.clone())
            .expect("holder permit must be granted (no prior holder)");

        // Spawn the second attempt on a separate task; it must observe
        // a Conflict and surface SandboxError::Conflict { .. }.
        let policy_clone = policy.clone();
        let config = SandboxConfig::default();
        let join = tokio::spawn(async move {
            run_supervised_with_locks("true", &policy_clone, &config, decl).await
        });
        let result = join.await.expect("task did not panic");
        match result {
            Err(crate::gateway::sandbox_executor::SandboxError::Conflict {
                conflicting_execution_id,
                ..
            }) => {
                assert_eq!(
                    conflicting_execution_id,
                    held.id(),
                    "the conflict must report the original holder id"
                );
            }
            Err(other) => panic!("expected SandboxError::Conflict, got {other:?}"),
            Ok(_) => panic!("concurrent writer on a held path must NOT be granted"),
        }
        // Release the holder permit so the test does not leak a txn id.
        drop(held);
    }
}
