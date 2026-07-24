//! Similarity computation module.
//!
//! Pln3 Phase 1B - Layer 3: Domain Modules
//! Provides Jaccard and Cosine similarity with SIMD acceleration.
//!
//! # Extensions (v28 Enhancement)
//!
//! - [`topk`]: Top-K nearest neighbor search with SIMD acceleration
//! - [`distance`]: Additional distance metrics (Euclidean, Manhattan, Pearson)

mod cosine;
pub mod distance;
mod jaccard;
pub mod topk;
mod traits;

// NOTE(P2): PyO3 bindings — requires `pyo3` dependency added to touring-simd
// with a `python` feature gate. Deferred to touring-python crate integration.
// #[cfg(feature = "python")]
// mod pyo3_bindings;

pub use cosine::*;
pub use distance::*;
pub use jaccard::*;
pub use topk::*;
pub use traits::*;

// #[cfg(feature = "python")]
// pub use pyo3_bindings::*;
