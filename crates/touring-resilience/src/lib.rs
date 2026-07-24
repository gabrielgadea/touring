//! touring_resilience — Resilience subsystems peeled out of the touring-foundation
//! god-kernel (A4): conflict detection, failover coordination, and the PSI
//! memory-pressure sentinel + CPU P/E core scheduler.
//!
//! Module structure:
//!   - [`conflict`]: multi-tier conflict detection (AstDiff / Semantic / GraphImpact) with SLA tracking.
//!   - [`failover`]: failover coordinator + health + persistence/tantivy/vector-store provider impls.
//!   - [`sentinel`]: PSI memory-pressure guard + CPU P/E core scheduler (+ the
//!     `touring-resource-monitor` binary under the `resource-monitor-bin` feature).
//!     Uses `unsafe` for libc CPU-affinity syscalls (SAFETY-commented at each site).
//!   - [`error`]: typed errors via `thiserror`.
//!   - [`types`]: public data types.
//!
//! All three subsystems were relocated verbatim from `touring-foundation`.

#![deny(missing_docs)]
// RBP-01 elite-lint ratchet (2026-06-16): prod-unwrap-free leaf — lock against
// future bare unwrap in non-test code (`.expect("…")` stays the sanctioned escape).
#![cfg_attr(not(test), deny(clippy::unwrap_used))]

pub mod conflict;
pub mod error;
pub mod failover;
pub mod sentinel;
pub mod types;

pub use error::{Error, Result};
pub use failover::{
    Failover, FailoverCoordinator, FailoverError, FailoverMetrics, FailoverState, Health,
};
pub use types::*;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn smoke_re_exports_compile() {
        let _err: Result<()> = Ok(());
    }
}
