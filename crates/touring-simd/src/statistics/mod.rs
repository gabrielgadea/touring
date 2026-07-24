//! Statistical functions module.
//!
//! Pln3 Phase 1B - Layer 3: Domain Modules
//! Provides ranking (Wilson score), drift detection (KS, JS divergence),
//! and reconciliation (weighted mean, CV, Bayesian fusion).

mod drift;
mod ranking;
pub mod reconciliation;
mod traits;

// NOTE(P2): PyO3 bindings — requires `pyo3` dependency added to touring-simd
// with a `python` feature gate. Deferred to touring-python crate integration.
// #[cfg(feature = "python")]
// mod pyo3_bindings;

pub use drift::*;
pub use ranking::*;
pub use traits::*;

// #[cfg(feature = "python")]
// pub use pyo3_bindings::*;
