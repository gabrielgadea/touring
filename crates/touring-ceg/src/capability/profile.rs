//! [`Decision`] and [`CapabilityProfile`] — the deny-by-default resolution
//! layer of the CEG capability model (phase **P2.2** of CEG Pln2,
//! `docs/2026-05-17-ceg-pln2-plan.md`).

use serde::{Deserialize, Serialize};

use super::Capability;

/// The outcome of resolving a [`Capability`] request against a
/// [`CapabilityProfile`].
///
/// Mirrors Deno's `PermissionStatus.state ∈ {granted, denied, prompt}`
/// (`docs/2026-05-17-ceg-best-practices.md`, "Deno permission model").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Decision {
    /// The capability is granted.
    Allow,
    /// The capability is refused.
    Deny,
    /// The capability needs human approval before use. The Touring daemon runs
    /// non-interactively, so the X6 capability-gate (CEG Pln2 P3.6) treats a
    /// `Prompt` as `Deny` unless an explicit approval channel is wired.
    Prompt,
}

impl Decision {
    /// `true` only for [`Decision::Allow`].
    ///
    /// [`Decision::Prompt`] is intentionally **not** an allow: a non-interactive
    /// run with a would-prompt capability fails closed.
    pub fn is_allowed(self) -> bool {
        matches!(self, Decision::Allow)
    }
}

/// A capability profile: a default disposition plus an allow-set, a deny-set,
/// and a prompt-set.
///
/// Resolution of a requested capability is **deny-by-default** and
/// **deny-wins**, evaluated in this order:
/// 1. any deny-set entry covers the request -> [`Decision::Deny`];
/// 2. else any allow-set entry covers it -> [`Decision::Allow`];
/// 3. else any prompt-set entry covers it -> [`Decision::Prompt`];
/// 4. else the profile `default`.
///
/// Resolution is `O(grant_count)`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityProfile {
    name: String,
    default: Decision,
    allow: Vec<Capability>,
    deny: Vec<Capability>,
    prompt: Vec<Capability>,
}

impl CapabilityProfile {
    /// Build an empty profile with the given `name` and `default` disposition.
    /// An empty profile resolves every request to `default`.
    pub fn new(name: impl Into<String>, default: Decision) -> Self {
        Self {
            name: name.into(),
            default,
            allow: Vec::new(),
            deny: Vec::new(),
            prompt: Vec::new(),
        }
    }

    /// Add `cap` to the allow-set (builder style).
    pub fn allowing(mut self, cap: Capability) -> Self {
        self.allow.push(cap);
        self
    }

    /// Add `cap` to the deny-set (builder style). Deny always wins over allow.
    pub fn denying(mut self, cap: Capability) -> Self {
        self.deny.push(cap);
        self
    }

    /// Add `cap` to the prompt-set (builder style). A prompt-set match resolves
    /// to [`Decision::Prompt`] — escalate-to-human — when no deny or allow
    /// applies.
    pub fn prompting(mut self, cap: Capability) -> Self {
        self.prompt.push(cap);
        self
    }

    /// The profile name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The default disposition applied when no grant matches.
    pub fn default_decision(&self) -> Decision {
        self.default
    }

    /// Total number of allow + deny + prompt grants — the cost factor of
    /// [`resolve`](Self::resolve).
    pub fn grant_count(&self) -> usize {
        self.allow.len() + self.deny.len() + self.prompt.len()
    }

    /// Resolve `requested` to a [`Decision`]: deny-wins, then allow, then
    /// prompt, then the profile default.
    pub fn resolve(&self, requested: &Capability) -> Decision {
        if self.deny.iter().any(|c| c.covers(requested)) {
            Decision::Deny
        } else if self.allow.iter().any(|c| c.covers(requested)) {
            Decision::Allow
        } else if self.prompt.iter().any(|c| c.covers(requested)) {
            Decision::Prompt
        } else {
            self.default
        }
    }

    /// `true` when `requested` resolves to [`Decision::Allow`].
    pub fn allows(&self, requested: &Capability) -> bool {
        self.resolve(requested).is_allowed()
    }
}

#[cfg(test)]
mod tests {
    use super::super::{CmdScope, HostScope, KeyScope, PathScope};
    use super::*;

    fn fsread(p: &str) -> Capability {
        Capability::FsRead(PathScope::new(p))
    }

    #[test]
    fn empty_profile_denies_by_default() {
        let p = CapabilityProfile::new("empty", Decision::Deny);
        assert_eq!(p.resolve(&fsread("/etc")), Decision::Deny);
        assert_eq!(p.grant_count(), 0);
    }

    #[test]
    fn allow_grant_resolves_allow() {
        let p = CapabilityProfile::new("p", Decision::Deny).allowing(fsread("/ws"));
        assert_eq!(p.resolve(&fsread("/ws/src")), Decision::Allow);
    }

    #[test]
    fn deny_wins_over_allow() {
        let p = CapabilityProfile::new("p", Decision::Deny)
            .allowing(fsread("/ws"))
            .denying(fsread("/ws/secrets"));
        // The request is inside both the allow root and the deny root — deny wins.
        assert_eq!(p.resolve(&fsread("/ws/secrets/key.pem")), Decision::Deny);
    }

    #[test]
    fn deny_wins_even_with_default_allow() {
        let p = CapabilityProfile::new("p", Decision::Allow)
            .denying(Capability::Run(CmdScope::new("rm")));
        assert_eq!(
            p.resolve(&Capability::Run(CmdScope::new("rm"))),
            Decision::Deny
        );
    }

    #[test]
    fn default_allow_profile_allows_unmatched() {
        let p = CapabilityProfile::new("trusted-ish", Decision::Allow);
        assert_eq!(p.resolve(&fsread("/anywhere")), Decision::Allow);
    }

    #[test]
    fn prompt_set_resolves_prompt() {
        let p = CapabilityProfile::new("p", Decision::Deny)
            .prompting(Capability::Run(CmdScope::new("git")));
        assert_eq!(
            p.resolve(&Capability::Run(CmdScope::new("git"))),
            Decision::Prompt
        );
    }

    #[test]
    fn resolve_respects_scope_containment() {
        let p = CapabilityProfile::new("p", Decision::Deny).allowing(fsread("/ws"));
        assert_eq!(p.resolve(&fsread("/etc/passwd")), Decision::Deny);
    }

    #[test]
    fn allows_convenience_matches_resolve() {
        let p = CapabilityProfile::new("p", Decision::Deny).allowing(fsread("/ws"));
        assert!(p.allows(&fsread("/ws/x")));
        assert!(!p.allows(&fsread("/etc")));
    }

    #[test]
    fn decision_is_allowed() {
        assert!(Decision::Allow.is_allowed());
        assert!(!Decision::Deny.is_allowed());
        // Prompt is not an allow — non-interactive daemon fails closed.
        assert!(!Decision::Prompt.is_allowed());
    }

    #[test]
    fn grant_count_sums_all_sets() {
        let p = CapabilityProfile::new("p", Decision::Deny)
            .allowing(fsread("/a"))
            .denying(fsread("/b"))
            .prompting(fsread("/c"));
        assert_eq!(p.grant_count(), 3);
    }

    #[test]
    fn accessors_expose_name_and_default() {
        let p = CapabilityProfile::new("named", Decision::Allow);
        assert_eq!(p.name(), "named");
        assert_eq!(p.default_decision(), Decision::Allow);
    }

    #[test]
    fn profile_serde_roundtrip() {
        let p = CapabilityProfile::new("p", Decision::Deny)
            .allowing(Capability::Net(HostScope::any()))
            .denying(Capability::Env(KeyScope::new("AWS_*")));
        let json = serde_json::to_string(&p).expect("serialize");
        let back: CapabilityProfile = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(p, back);
    }
}
