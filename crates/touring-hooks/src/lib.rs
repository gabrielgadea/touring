//! touring-hooks — compatibility façade over the split Neural Hooks stack
//! (daemon-lib-rearch Phases C+D, 2026-06-10).
//!
//! The monolith was carved into:
//! - **touring-hooks-core** — data/intelligence engines (knowledge, tantivy,
//!   health-delta, bridges) with zero HookRuntime/cli coupling;
//! - **touring-dispatch** — the daemon's nervous system (HookRuntime,
//!   hook_registry, daemon actor, every pre/post lifecycle hook, the cli/
//!   handlers) — it re-exports the core at the historical module paths;
//! - **touring-hooks** (this crate) — a thin façade that re-exports the
//!   dispatch root verbatim plus the `touring-hook` / `touring-daemon`
//!   binaries, so every `touring_hooks::X` path used by touring-server,
//!   touring-cortex, the 57 integration suites and the 6 benches keeps
//!   resolving with zero churn.

// D.W2.P3.T6 (2026-06-11): facade measured at 0 missing docs - lock it in.
#![deny(missing_docs)]
// RBP-01 elite-lint ratchet (2026-06-16): prod-unwrap-free façade — lock against
// future bare unwrap in non-test code (`.expect("…")` stays the sanctioned escape).
#![cfg_attr(not(test), deny(clippy::unwrap_used))]

// Everything public from the dispatch layer (which itself re-exports the
// core engines + leaf crates) surfaces here at the historical paths.
pub use touring_dispatch::*;

// `#[macro_export]` macros live at their defining crate's root and are not
// reliably carried by glob re-exports — re-export explicitly.
pub use touring_dispatch::with_validation;

pub mod token_meter;

// ── B.4: Dual-module feature gating ──────────────────────────────────
//
// This module provides a compile-time flag that external benchmarks and CI
// jobs can use to verify that the hooks-active / hooks-noop feature split
// is working correctly.
//
// Architecture note: Rust requires all modules to be declared at compile
// time, so we cannot conditionally declare modules based on features.
// Instead:
// - All modules are declared in this file (lib.rs) with standard feature gates
// - lib_on.rs: empty placeholder — documents that real code lives here
// - lib_off.rs: empty placeholder — documents the noop stub layer
//
// The actual hook implementations are always compiled. For benchmarking
// purposes, each hook function checks `HOOKS_MODE.is_active()` at runtime
// and returns no-op responses when `hooks-active` feature is OFF.
//
// See: cross-repo-improvements-master-plan.md section B.4

/// Global hooks mode flag controlled by the `hooks-active` feature.
///
/// When the feature is ON (default): hooks run real implementations.
/// When the feature is OFF (`--no-default-features`): hooks return no-ops.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HooksMode {
    /// Full hook implementations active (default build).
    Active,
    /// No-op stub mode (`--no-default-features` benchmark build).
    Noop,
}

impl HooksMode {
    /// Returns true if full hook implementations are active.
    pub fn is_active(&self) -> bool {
        *self == HooksMode::Active
    }
}

/// Global hooks mode — set at startup based on feature compilation.
/// B.4.4: External benchmarks can read this to measure overhead.
pub static HOOKS_MODE: HooksMode = if cfg!(feature = "hooks-active") {
    HooksMode::Active
} else {
    HooksMode::Noop
};

#[cfg(test)]
mod hooks_mode_tests {
    use super::*;
    #[test]
    fn test_hooks_mode_is_active_true_when_feature_on() {
        if cfg!(feature = "hooks-active") {
            assert!(HOOKS_MODE.is_active());
            assert_eq!(HOOKS_MODE, HooksMode::Active);
        }
    }
    #[test]
    fn test_hooks_mode_is_active_false_when_feature_off() {
        if !cfg!(feature = "hooks-active") {
            assert!(!HOOKS_MODE.is_active());
            assert_eq!(HOOKS_MODE, HooksMode::Noop);
        }
    }
    #[test]
    fn test_hooks_mode_parity() {
        let mode = HOOKS_MODE;
        assert!(matches!(mode, HooksMode::Active | HooksMode::Noop));
    }
    #[test]
    #[allow(clippy::clone_on_copy)] // intentional: this test verifies HooksMode: Clone is derived
    fn test_hooks_mode_clone() {
        let mode = HOOKS_MODE;
        let cloned = mode.clone();
        assert_eq!(mode, cloned);
    }
    #[test]
    fn test_hooks_mode_copy() {
        let mode = HOOKS_MODE;
        let copied = mode;
        assert_eq!(mode, copied);
    }
    #[test]
    fn test_hooks_mode_debug() {
        let mode = HOOKS_MODE;
        let debug_str = format!("{:?}", mode);
        assert!(!debug_str.is_empty());
    }
    #[test]
    fn test_hooks_mode_equality() {
        assert_eq!(HOOKS_MODE, HOOKS_MODE);
    }
}
