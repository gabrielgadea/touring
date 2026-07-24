//! Capability *scope* types for the Code Execution Gateway (CEG) capability model.
//!
//! Every [`crate::capability::Capability`] variant carries a typed scope payload
//! that defines *what* the capability covers — a filesystem subtree, a network
//! endpoint, a command name, or an environment-variable key. Scopes are concrete
//! value spaces, never bare booleans, mirroring the Deno permission model
//! documented in `docs/2026-05-17-ceg-best-practices.md` ("Deno permission
//! model").
//!
//! Each scope type is total, [`serde`]-serializable, and exposes a `matches`
//! predicate. A *granted* scope is always the receiver and the *requested*
//! value the argument, so containment is directional: `granted.matches(req)`.
//!
//! Part of CEG Pln2 phase **P2.1** (`docs/2026-05-17-ceg-pln2-plan.md`).

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// A filesystem path subtree.
///
/// A granted `PathScope` covers the directory tree rooted at its `root`: the
/// root itself and every path beneath it. Matching is component-wise (via
/// [`Path::starts_with`]), so `/var/log` does **not** spuriously cover
/// `/var/logger`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PathScope {
    root: PathBuf,
}

impl PathScope {
    /// Build a scope rooted at `root`. The path is stored as given; a caller
    /// that needs symlink-resolved matching should canonicalize beforehand.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// The subtree root.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// `true` when `candidate` is the root itself or lies beneath it.
    pub fn matches(&self, candidate: impl AsRef<Path>) -> bool {
        candidate.as_ref().starts_with(&self.root)
    }
}

/// A network endpoint scope: an outbound `host:port` target.
///
/// `host` may be the literal `*` wildcard ("any host"). `port` of `None` means
/// "any port"; `Some(p)` restricts to that single port.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct HostScope {
    host: String,
    port: Option<u16>,
}

impl HostScope {
    /// Build a scope for `host` and an optional `port`.
    pub fn new(host: impl Into<String>, port: Option<u16>) -> Self {
        Self {
            host: host.into(),
            port,
        }
    }

    /// A scope covering every host and every port.
    pub fn any() -> Self {
        Self {
            host: "*".to_string(),
            port: None,
        }
    }

    /// The host pattern (`*` means any host).
    pub fn host(&self) -> &str {
        &self.host
    }

    /// The port restriction, if any.
    pub fn port(&self) -> Option<u16> {
        self.port
    }

    /// `true` when this granted scope covers `requested`. A `*` host covers any
    /// host; a `None` port covers any port.
    pub fn matches(&self, requested: &HostScope) -> bool {
        let host_ok = self.host == "*" || self.host == requested.host;
        let port_ok = self.port.is_none() || self.port == requested.port;
        host_ok && port_ok
    }
}

/// A subprocess command-name scope.
///
/// `name` is matched exactly, or `*` to mean "any command".
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CmdScope {
    name: String,
}

impl CmdScope {
    /// Build a scope for the command `name`.
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into() }
    }

    /// A scope covering every command.
    pub fn any() -> Self {
        Self {
            name: "*".to_string(),
        }
    }

    /// The command name (`*` means any command).
    pub fn name(&self) -> &str {
        &self.name
    }

    /// `true` when this granted scope covers `requested`. A `*` grant covers
    /// any command; otherwise the names must be equal.
    pub fn matches(&self, requested: &CmdScope) -> bool {
        self.name == "*" || self.name == requested.name
    }
}

/// An environment-variable key scope.
///
/// `key` is matched exactly, or — when it ends in `*` — as a prefix: `AWS_*`
/// covers `AWS_REGION`, `AWS_SECRET_ACCESS_KEY`, and so on.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct KeyScope {
    key: String,
}

impl KeyScope {
    /// Build a scope for the environment-variable `key`. A trailing `*` makes
    /// it a prefix pattern.
    pub fn new(key: impl Into<String>) -> Self {
        Self { key: key.into() }
    }

    /// The key pattern.
    pub fn key(&self) -> &str {
        &self.key
    }

    /// `true` when this scope covers `requested_key`.
    pub fn matches(&self, requested_key: &str) -> bool {
        match self.key.strip_suffix('*') {
            Some(prefix) => requested_key.starts_with(prefix),
            None => self.key == requested_key,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_scope_matches_root() {
        let s = PathScope::new("/home/user/ws");
        assert!(s.matches("/home/user/ws"));
    }

    #[test]
    fn path_scope_matches_subtree() {
        let s = PathScope::new("/home/user/ws");
        assert!(s.matches("/home/user/ws/src/main.rs"));
    }

    #[test]
    fn path_scope_rejects_sibling_with_shared_prefix() {
        // Component-wise matching: /var/log must not cover /var/logger.
        let s = PathScope::new("/var/log");
        assert!(!s.matches("/var/logger/app.log"));
    }

    #[test]
    fn path_scope_rejects_parent() {
        let s = PathScope::new("/home/user/ws");
        assert!(!s.matches("/home/user"));
    }

    #[test]
    fn path_scope_root_accessor() {
        let s = PathScope::new("/tmp/x");
        assert_eq!(s.root(), Path::new("/tmp/x"));
    }

    #[test]
    fn host_scope_exact_match() {
        let g = HostScope::new("example.com", Some(443));
        assert!(g.matches(&HostScope::new("example.com", Some(443))));
    }

    #[test]
    fn host_scope_wildcard_host() {
        let g = HostScope::new("*", Some(443));
        assert!(g.matches(&HostScope::new("anything.test", Some(443))));
    }

    #[test]
    fn host_scope_any_port() {
        let g = HostScope::new("example.com", None);
        assert!(g.matches(&HostScope::new("example.com", Some(8080))));
    }

    #[test]
    fn host_scope_rejects_wrong_port() {
        let g = HostScope::new("example.com", Some(443));
        assert!(!g.matches(&HostScope::new("example.com", Some(80))));
    }

    #[test]
    fn host_scope_rejects_wrong_host() {
        let g = HostScope::new("example.com", None);
        assert!(!g.matches(&HostScope::new("evil.test", None)));
    }

    #[test]
    fn host_scope_any_covers_everything() {
        let g = HostScope::any();
        assert!(g.matches(&HostScope::new("any.host", Some(1))));
        assert_eq!(g.host(), "*");
        assert_eq!(g.port(), None);
    }

    #[test]
    fn cmd_scope_exact() {
        let g = CmdScope::new("cargo");
        assert!(g.matches(&CmdScope::new("cargo")));
    }

    #[test]
    fn cmd_scope_wildcard() {
        let g = CmdScope::any();
        assert!(g.matches(&CmdScope::new("rm")));
        assert_eq!(g.name(), "*");
    }

    #[test]
    fn cmd_scope_rejects_other() {
        let g = CmdScope::new("cargo");
        assert!(!g.matches(&CmdScope::new("rm")));
    }

    #[test]
    fn key_scope_exact() {
        let g = KeyScope::new("PATH");
        assert!(g.matches("PATH"));
        assert!(!g.matches("HOME"));
    }

    #[test]
    fn key_scope_prefix_wildcard() {
        let g = KeyScope::new("AWS_*");
        assert!(g.matches("AWS_SECRET_ACCESS_KEY"));
        assert!(g.matches("AWS_REGION"));
    }

    #[test]
    fn key_scope_rejects_non_prefix() {
        let g = KeyScope::new("AWS_*");
        assert!(!g.matches("GCP_PROJECT"));
        assert_eq!(g.key(), "AWS_*");
    }

    #[test]
    fn scope_serde_roundtrip() {
        let s = PathScope::new("/ws");
        let json = serde_json::to_string(&s).expect("serialize");
        let back: PathScope = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(s, back);

        let h = HostScope::new("h", Some(1));
        let hj = serde_json::to_string(&h).expect("serialize");
        let hb: HostScope = serde_json::from_str(&hj).expect("deserialize");
        assert_eq!(h, hb);
    }
}
