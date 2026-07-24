//! Semantic embedding layer — W1 Cognitive.
//!
//! Provides the [`Embedder`] trait abstracting embedding generation from the
//! concrete backend. Two implementations ship:
//!
//! - [`MockEmbedder`] — deterministic hash-derived vectors, zero dependencies,
//!   always available. Used for tests, CI, and warm-up paths where semantic
//!   fidelity is not required.
//! - [`candle_embedder::CandleEmbedder`] — gated behind
//!   `semantic-embeddings` feature; loads quantized GGUF models (BGE, Nomic)
//!   via candle-core for edge inference.
//!
//! Downstream consumers should prefer the trait over concrete types so the
//! feature gate does not leak into user code:
//!
//! ```no_run
//! use touring_intelligence::rl::semantic::{Embedder, MockEmbedder};
//! let e: Box<dyn Embedder> = Box::new(MockEmbedder::new(384));
//! let v = e.embed("hello world");
//! assert_eq!(v.len(), 384);
//! ```

pub mod candle_embedder;
pub mod quantized_bert;

use rustc_hash::FxHasher;
use std::hash::{Hash, Hasher};

/// Abstraction over embedding backends.
///
/// Implementations MUST be `Send + Sync` so consumers can share a single
/// embedder across the rayon thread pool that drives wiring analysis.
pub trait Embedder: Send + Sync {
    /// Produce a dense floating-point embedding for `text`.
    ///
    /// The returned vector MUST have length equal to [`Self::dimension`].
    fn embed(&self, text: &str) -> Vec<f32>;

    /// Fixed output dimensionality. Callers use this to pre-size buffers
    /// (e.g. `EmbeddingU4::from_f32` allocates based on length).
    fn dimension(&self) -> usize;
}

/// Deterministic mock embedder. Produces byte-stable vectors derived from a
/// FxHash seed + a fixed multiplicative constant (golden ratio). Useful for:
///
/// - Integration tests that must run in CI without GPU/model downloads.
/// - Warm-up paths during daemon boot before the real model is ready.
/// - Fallback when `semantic-embeddings` feature is disabled.
///
/// # Determinism
///
/// The same `text` and `dims` always produce the same output across runs.
/// Different inputs produce different outputs with very high probability
/// (FxHash collision rate is acceptable for non-cryptographic use).
#[derive(Debug, Clone)]
pub struct MockEmbedder {
    dims: usize,
}

impl MockEmbedder {
    /// Create a new mock embedder producing vectors of `dims` length.
    ///
    /// # Panics
    ///
    /// Panics if `dims == 0`. Embedders with zero dimensions are never
    /// meaningful and the invariant is cheaper to enforce than to ignore.
    #[must_use]
    pub fn new(dims: usize) -> Self {
        assert!(dims > 0, "MockEmbedder dimension must be > 0");
        Self { dims }
    }
}

impl Embedder for MockEmbedder {
    fn embed(&self, text: &str) -> Vec<f32> {
        let mut hasher = FxHasher::default();
        text.hash(&mut hasher);
        let seed = hasher.finish();
        // Golden-ratio constant — good distribution for sequential offsets.
        const PHI: u64 = 0x9E37_79B9_7F4A_7C15;
        (0..self.dims)
            .map(|i| {
                let x = seed.wrapping_add(i as u64).wrapping_mul(PHI);
                // Map to [-1.0, 1.0] so downstream cosine makes sense.
                ((x as i64) as f32) / (i64::MAX as f32)
            })
            .collect()
    }

    fn dimension(&self) -> usize {
        self.dims
    }
}

// Compile-time invariants: MockEmbedder is the CI default, must be cheap to
// pass through the rayon pool and clone into per-thread contexts.
static_assertions::assert_impl_all!(MockEmbedder: Send, Sync, Clone);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_embedder_respects_configured_dimension() {
        // REQUIREMENT: dimension() == embed().len() for any input.
        let e = MockEmbedder::new(384);
        assert_eq!(e.dimension(), 384);
        assert_eq!(e.embed("hello").len(), 384);
        assert_eq!(e.embed("").len(), 384);
        assert_eq!(e.embed(&"x".repeat(10_000)).len(), 384);
    }

    #[test]
    fn mock_embedder_is_deterministic_across_calls() {
        // REQUIREMENT: same input → bit-identical output.
        let e = MockEmbedder::new(64);
        let a = e.embed("stable input");
        let b = e.embed("stable input");
        assert_eq!(a, b, "mock embedder must be deterministic");
    }

    #[test]
    fn mock_embedder_differentiates_distinct_inputs() {
        // REQUIREMENT: different inputs → different outputs (with very high
        // probability). Checking the first element avoids flaky full-vector
        // comparisons while still detecting regression to a constant.
        let e = MockEmbedder::new(16);
        let a = e.embed("alpha");
        let b = e.embed("beta");
        assert_ne!(a, b, "distinct inputs must produce distinct vectors");
    }

    #[test]
    fn mock_embedder_outputs_stay_in_bounded_range() {
        // REQUIREMENT: outputs fit in [-1.0, 1.0] so cosine downstream is
        // well-behaved. Loose bound with 1e-6 epsilon to accommodate rounding.
        let e = MockEmbedder::new(128);
        let v = e.embed("bounds check");
        for (i, x) in v.iter().enumerate() {
            assert!(x.is_finite(), "element {i} is not finite: {x}");
            assert!(
                (-1.000_001..=1.000_001).contains(x),
                "element {i} = {x} outside [-1, 1]"
            );
        }
    }

    #[test]
    #[should_panic(expected = "dimension must be > 0")]
    fn mock_embedder_rejects_zero_dimension() {
        // BOUNDARY: zero-dimension embedder is nonsensical, must panic early.
        let _ = MockEmbedder::new(0);
    }

    #[test]
    fn mock_embedder_as_trait_object_works() {
        // REQUIREMENT: trait is dyn-safe (Box<dyn Embedder>).
        let e: Box<dyn Embedder> = Box::new(MockEmbedder::new(32));
        assert_eq!(e.dimension(), 32);
        assert_eq!(e.embed("dyn").len(), 32);
    }
}
