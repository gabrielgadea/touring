//! Per-project capability-profile resolution — phase **P2.5** of CEG Pln2
//! (`docs/2026-05-17-ceg-pln2-plan.md`).
//!
//! Each daemon project (one actor per project) may declare its own default
//! [`BuiltinProfile`]. This module resolves a project to its profile
//! **deterministically**: resolution is a pure function of
//! `(registry, project)` — the same inputs always yield the same profile,
//! independent of declaration order (REGRA #17, entity-identity determinism).
//!
//! The [`ProjectProfileRegistry`] is `serde`-serializable, so a later wiring
//! wave (CEG Pln2 P3/P6) can load it from — or persist it to — the
//! per-project daemon DB or a config file. P2.5 itself performs **no I/O**:
//! the resolution layer stays pure and testable.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::CapabilityProfile;
use super::builtins::BuiltinProfile;

/// A stable, canonical identifier for a daemon project.
///
/// Derived deterministically from the project's workspace path: trailing path
/// separators are trimmed so `/ws` and `/ws/` denote the same project. The
/// path is **not** filesystem-canonicalized (no symlink resolution, no I/O) —
/// resolution must stay pure, per REGRA #17 (identity is a function of the
/// canonical name, never of process state).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ProjectId(String);

impl ProjectId {
    /// Build a `ProjectId` from a workspace path, normalizing trailing `/`.
    pub fn new(workspace: impl AsRef<Path>) -> Self {
        let raw = workspace.as_ref().to_string_lossy();
        let trimmed = raw.trim_end_matches('/');
        // The root path "/" trims to the empty string — preserve "/" alone.
        let id = if trimmed.is_empty() { "/" } else { trimmed };
        Self(id.to_string())
    }

    /// The canonical string form.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A registry mapping daemon projects to their declared capability profile.
///
/// Resolution is **deterministic**: a pure function of `(registry, project)`.
/// Overrides live in a [`BTreeMap`], so iteration and serialization are in
/// sorted key order — independent of insertion order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectProfileRegistry {
    /// The profile applied to any project without an explicit declaration.
    default_profile: BuiltinProfile,
    /// Per-project overrides, keyed by canonical [`ProjectId`].
    overrides: BTreeMap<ProjectId, BuiltinProfile>,
}

impl ProjectProfileRegistry {
    /// A registry whose unset-project fallback is `default_profile`.
    pub fn new(default_profile: BuiltinProfile) -> Self {
        Self {
            default_profile,
            overrides: BTreeMap::new(),
        }
    }

    /// A registry with the safe [`BuiltinProfile::Sandboxed`] fallback — the
    /// recommended default for a daemon that has not been told otherwise.
    pub fn sandboxed_default() -> Self {
        Self::new(BuiltinProfile::Sandboxed)
    }

    /// Declare that `project` runs under `profile` (builder style).
    ///
    /// A second declaration for the same project replaces the first
    /// (last-write-wins); the resulting registry is identical regardless of
    /// the order in which distinct projects are declared.
    pub fn declare(mut self, project: ProjectId, profile: BuiltinProfile) -> Self {
        self.overrides.insert(project, profile);
        self
    }

    /// The fallback profile applied to undeclared projects.
    pub fn default_profile(&self) -> BuiltinProfile {
        self.default_profile
    }

    /// The number of explicit per-project declarations.
    pub fn declared_count(&self) -> usize {
        self.overrides.len()
    }

    /// Resolve the [`BuiltinProfile`] for `project`: the explicit declaration
    /// if one exists, else the registry default. Deterministic, `O(log n)`.
    pub fn resolve(&self, project: &ProjectId) -> BuiltinProfile {
        self.overrides
            .get(project)
            .copied()
            .unwrap_or(self.default_profile)
    }
}

/// Resolve the [`BuiltinProfile`] a project runs under.
///
/// Free-function form of [`ProjectProfileRegistry::resolve`] — the P2.5
/// `resolve_profile_for_project` entry point. Deterministic: a pure function
/// of the registry and the project id.
pub fn resolve_profile_for_project(
    registry: &ProjectProfileRegistry,
    project: &ProjectId,
) -> BuiltinProfile {
    registry.resolve(project)
}

/// Resolve `project` to a concrete [`CapabilityProfile`].
///
/// Resolves the declared [`BuiltinProfile`] and builds it with the given
/// `workspace` / `staging_dir` roots — the bridge from "which profile" to a
/// usable capability set.
pub fn resolve_capability_profile(
    registry: &ProjectProfileRegistry,
    project: &ProjectId,
    workspace: &Path,
    staging_dir: &Path,
) -> CapabilityProfile {
    resolve_profile_for_project(registry, project).build(workspace, staging_dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_id_canonicalizes_trailing_slash() {
        assert_eq!(
            ProjectId::new("/home/user/ws/"),
            ProjectId::new("/home/user/ws")
        );
        assert_eq!(ProjectId::new("/home/user/ws").as_str(), "/home/user/ws");
    }

    #[test]
    fn project_id_root_path_preserved() {
        assert_eq!(ProjectId::new("/").as_str(), "/");
    }

    #[test]
    fn registry_default_for_undeclared_project() {
        let reg = ProjectProfileRegistry::sandboxed_default();
        assert_eq!(
            reg.resolve(&ProjectId::new("/unknown/project")),
            BuiltinProfile::Sandboxed
        );
        assert_eq!(reg.default_profile(), BuiltinProfile::Sandboxed);
    }

    #[test]
    fn registry_resolves_declared_override() {
        let ws = ProjectId::new("/home/user/trusted-ws");
        let reg = ProjectProfileRegistry::new(BuiltinProfile::Sandboxed)
            .declare(ws.clone(), BuiltinProfile::Trusted);
        assert_eq!(reg.resolve(&ws), BuiltinProfile::Trusted);
        assert_eq!(reg.declared_count(), 1);
    }

    #[test]
    fn registry_last_write_wins() {
        let ws = ProjectId::new("/ws");
        let reg = ProjectProfileRegistry::new(BuiltinProfile::Sandboxed)
            .declare(ws.clone(), BuiltinProfile::ReadOnly)
            .declare(ws.clone(), BuiltinProfile::Trusted);
        assert_eq!(reg.resolve(&ws), BuiltinProfile::Trusted);
        assert_eq!(reg.declared_count(), 1);
    }

    #[test]
    fn resolution_is_declaration_order_independent() {
        let a = ProjectId::new("/a");
        let b = ProjectId::new("/b");
        let r1 = ProjectProfileRegistry::new(BuiltinProfile::Sandboxed)
            .declare(a.clone(), BuiltinProfile::Trusted)
            .declare(b.clone(), BuiltinProfile::ReadOnly);
        let r2 = ProjectProfileRegistry::new(BuiltinProfile::Sandboxed)
            .declare(b.clone(), BuiltinProfile::ReadOnly)
            .declare(a.clone(), BuiltinProfile::Trusted);
        // Deterministic: identical registries, identical resolution.
        assert_eq!(r1, r2);
        assert_eq!(r1.resolve(&a), r2.resolve(&a));
        assert_eq!(r1.resolve(&b), r2.resolve(&b));
    }

    #[test]
    fn free_function_matches_method() {
        let ws = ProjectId::new("/ws");
        let reg = ProjectProfileRegistry::new(BuiltinProfile::ReadOnly)
            .declare(ws.clone(), BuiltinProfile::StagedWrite);
        assert_eq!(resolve_profile_for_project(&reg, &ws), reg.resolve(&ws));
        assert_eq!(
            resolve_profile_for_project(&reg, &ProjectId::new("/other")),
            BuiltinProfile::ReadOnly
        );
    }

    #[test]
    fn resolve_capability_profile_builds_concrete() {
        let ws = ProjectId::new("/home/user/ws");
        let reg = ProjectProfileRegistry::new(BuiltinProfile::Sandboxed)
            .declare(ws.clone(), BuiltinProfile::Trusted);
        let workspace = Path::new("/home/user/ws");
        let staging = Path::new("/home/user/.staging");
        let declared = resolve_capability_profile(&reg, &ws, workspace, staging);
        assert_eq!(declared.name(), "Trusted");
        // An undeclared project falls back to the registry default.
        let fallback = resolve_capability_profile(&reg, &ProjectId::new("/x"), workspace, staging);
        assert_eq!(fallback.name(), "Sandboxed");
    }

    #[test]
    fn registry_serde_roundtrip() {
        let reg = ProjectProfileRegistry::new(BuiltinProfile::Sandboxed)
            .declare(ProjectId::new("/a"), BuiltinProfile::Trusted)
            .declare(ProjectId::new("/b"), BuiltinProfile::StagedWrite);
        let json = serde_json::to_string(&reg).expect("serialize");
        let back: ProjectProfileRegistry = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(reg, back);
    }

    #[test]
    fn project_id_serde_roundtrip() {
        let id = ProjectId::new("/home/user/ws");
        let json = serde_json::to_string(&id).expect("serialize");
        let back: ProjectId = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(id, back);
    }
}
