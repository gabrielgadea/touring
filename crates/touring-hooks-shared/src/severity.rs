//! `StaticSeverity` — the shared severity vocabulary for static / workflow analysis.
//!
//! S-13 (2026-06-06). Relocated here from `touring-hooks::gateway::static_stage`
//! to this leaf crate so it is a shared vocabulary between the CEG's X2 STATIC
//! stage (`gateway/static_stage.rs`) and the workflow antipattern detector
//! (`workflow::antipattern`), neither depending on the other. Re-exported by
//! `gateway::static_stage` as `StaticSeverity` so existing call sites are
//! unchanged. Leaf-safe: depends only on `serde`.

use serde::{Deserialize, Serialize};

/// The worst-case severity of an X2 STATIC analysis.
///
/// Ordered `Clear < Warn < Block` — `StaticReport::analyze` keeps the maximum
/// across every check (the derived `Ord` provides `.max()`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum StaticSeverity {
    /// No structural or risk concern found.
    Clear,
    /// A concern worth surfacing — not blocking on its own.
    Warn,
    /// A destructive or dangerous pattern — the strongest static signal.
    Block,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The ordering invariant the X2 stage relies on: `Clear < Warn < Block`.
    #[test]
    fn ordering_is_clear_lt_warn_lt_block() {
        assert!(StaticSeverity::Clear < StaticSeverity::Warn);
        assert!(StaticSeverity::Warn < StaticSeverity::Block);
        assert_eq!(
            StaticSeverity::Clear.max(StaticSeverity::Block),
            StaticSeverity::Block
        );
    }
}
