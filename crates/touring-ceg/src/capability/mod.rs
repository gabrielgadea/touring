//! Capability model for the Code Execution Gateway (CEG) — phase **P2** of
//! CEG Pln2 (`docs/2026-05-17-ceg-pln2-plan.md`).
//!
//! This is the Deno-inspired, deny-by-default authority layer: executed code
//! never gets ambient power. A piece of code declares the [`Capability`] set it
//! requires; a [`CapabilityProfile`] resolves each request to a [`Decision`]
//! (`Allow` / `Deny` / `Prompt`), with **deny always winning** over allow and
//! an empty profile denying everything.
//!
//! Layout:
//! - [`scope`] — typed scope payloads ([`PathScope`], [`HostScope`],
//!   [`CmdScope`], [`KeyScope`]).
//! - [`profile`] — [`Decision`] and [`CapabilityProfile`] resolution.
//! - [`builtins`] — the four built-in profiles plus
//!   [`builtins::BuiltinProfile`].
//!
//! Design rationale: `docs/2026-05-17-ceg-best-practices.md`, section
//! "Deno permission model".

pub mod builtins;
pub mod enforce_linux;
pub mod limits;
pub mod profile;
pub mod resolve;
pub mod scope;

pub use enforce_linux::{
    CappedResource, EnforcementLevel, EnforcementReport, ResourceCaps, apply_landlock, apply_rlimit,
};
pub use limits::{
    CgroupStatus, ResourceLimits, apply_resource_caps_to, cgroup_v2_status,
    sandbox_enforcement_advisory,
};
pub use profile::{CapabilityProfile, Decision};
pub use resolve::{
    ProjectId, ProjectProfileRegistry, resolve_capability_profile, resolve_profile_for_project,
};
pub use scope::{CmdScope, HostScope, KeyScope, PathScope};

use serde::{Deserialize, Serialize};

/// A single unit of authority a piece of executed code may require.
///
/// Each variant carries a typed [scope] payload describing *what*
/// the authority covers. There is one variant per resource class, mirroring
/// Deno's `--allow-read` / `--allow-write` / `--allow-net` / `--allow-run` /
/// `--allow-env` split — there is deliberately no monolithic "trusted" variant.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Capability {
    /// Read a filesystem path subtree.
    FsRead(PathScope),
    /// Write to a filesystem path subtree.
    FsWrite(PathScope),
    /// Open an outbound network connection to a `host:port` endpoint.
    Net(HostScope),
    /// Spawn a subprocess by command name.
    Run(CmdScope),
    /// Read an environment variable.
    Env(KeyScope),
}

impl Capability {
    /// `true` when `self`, treated as a *granted* capability, covers the
    /// `requested` capability.
    ///
    /// Coverage holds only within the same resource class; across classes it
    /// is always `false`. Within a class, the scope's `matches` predicate
    /// decides containment.
    pub fn covers(&self, requested: &Capability) -> bool {
        match (self, requested) {
            (Capability::FsRead(g), Capability::FsRead(r)) => g.matches(r.root()),
            (Capability::FsWrite(g), Capability::FsWrite(r)) => g.matches(r.root()),
            (Capability::Net(g), Capability::Net(r)) => g.matches(r),
            (Capability::Run(g), Capability::Run(r)) => g.matches(r),
            (Capability::Env(g), Capability::Env(r)) => g.matches(r.key()),
            _ => false,
        }
    }

    /// `true` when exercising this capability **mutates external state** — a
    /// filesystem write, an outbound network connection, or a subprocess spawn — so
    /// it needs a *compensating* saga step on rollback. Reads (`FsRead`, `Env`) are
    /// side-effect-free and need no compensation. This is the selective-checkpoint
    /// predicate used by [`crate::gateway::selective_checkpoint`] (C13).
    #[must_use]
    pub fn is_side_effecting(&self) -> bool {
        matches!(
            self,
            Capability::FsWrite(_) | Capability::Net(_) | Capability::Run(_)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn covers_same_class_fsread_subtree() {
        let grant = Capability::FsRead(PathScope::new("/ws"));
        let req = Capability::FsRead(PathScope::new("/ws/src/lib.rs"));
        assert!(grant.covers(&req));
    }

    #[test]
    fn covers_rejects_out_of_subtree() {
        let grant = Capability::FsRead(PathScope::new("/ws"));
        let req = Capability::FsRead(PathScope::new("/etc/passwd"));
        assert!(!grant.covers(&req));
    }

    #[test]
    fn covers_rejects_cross_class() {
        // An FsRead grant must never cover an FsWrite request.
        let grant = Capability::FsRead(PathScope::new("/ws"));
        let req = Capability::FsWrite(PathScope::new("/ws/out"));
        assert!(!grant.covers(&req));
    }

    #[test]
    fn covers_run_wildcard() {
        let grant = Capability::Run(CmdScope::any());
        assert!(grant.covers(&Capability::Run(CmdScope::new("rm"))));
    }

    #[test]
    fn covers_net_scope() {
        let grant = Capability::Net(HostScope::new("*", Some(443)));
        assert!(grant.covers(&Capability::Net(HostScope::new("api.test", Some(443)))));
        assert!(!grant.covers(&Capability::Net(HostScope::new("api.test", Some(80)))));
    }

    #[test]
    fn covers_env_prefix() {
        let grant = Capability::Env(KeyScope::new("AWS_*"));
        assert!(grant.covers(&Capability::Env(KeyScope::new("AWS_SECRET_ACCESS_KEY"))));
        assert!(!grant.covers(&Capability::Env(KeyScope::new("PATH"))));
    }

    #[test]
    fn capability_serde_roundtrip() {
        let cap = Capability::FsWrite(PathScope::new("/staging"));
        let json = serde_json::to_string(&cap).expect("serialize");
        let back: Capability = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(cap, back);
    }
}
