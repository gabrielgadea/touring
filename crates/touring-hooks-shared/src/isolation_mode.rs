//! `IsolationMode` — concurrent-agent file-path isolation policy.
//!
//! ES3 P5 / S-13 (2026-06-06). Relocated here from `touring-hooks::hook_runtime`
//! to this leaf crate so the CEG gateway (`gateway/txn.rs`) and other consumers
//! can name the policy enum without depending on the (cyclic) `hook_runtime`
//! module. Leaf-safe: depends only on `std`. Re-exported by `hook_runtime` as
//! `crate::hook_runtime::IsolationMode` so existing call sites are unchanged.

use std::path::PathBuf;

/// ES3 P5 — isolation mode for concurrent-agent file paths.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum IsolationMode {
    /// Default — no isolation, all paths absolute (the single-agent path).
    #[default]
    Solo,
    /// Worktree isolation — paths inside the worktree are scoped; the
    /// `AccessDeclaration` path-rewriter (P5.3) prefixes paths with the
    /// worktree root.
    Worktree(PathBuf),
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `Default` resolves to `Solo` (the single-agent path).
    #[test]
    fn default_is_solo() {
        assert_eq!(IsolationMode::default(), IsolationMode::Solo);
    }

    /// `Worktree` carries its scoped root path.
    #[test]
    fn worktree_carries_path() {
        let m = IsolationMode::Worktree(PathBuf::from("/tmp/wt"));
        match m {
            IsolationMode::Worktree(p) => assert_eq!(p, PathBuf::from("/tmp/wt")),
            IsolationMode::Solo => panic!("expected Worktree"),
        }
    }
}
