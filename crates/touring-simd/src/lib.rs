//! # touring-simd v0.2.0 — SIMD-accelerated operations for the Touring workspace
//!
//! High-performance vector operations with automatic multi-ISA dispatch via [`pulp`].
//! Supports AVX-512, AVX2+FMA, NEON (ARM), and scalar fallback — all from a single
//! generic implementation that the compiler specializes per platform.
//!
//! ## Architecture
//!
//! ### Core SIMD Layer ([`simd_utils`])
//!
//! - **[`simd_utils::dispatch`]** — Cached CPU detection via `pulp::Arch::new()`
//! - **[`simd_utils::ops]`]** — `pulp::WithSimd` implementations for all ops (FMA-enabled)
//! - **[`simd_utils::matrix`]** — [`MatrixView`](simd_utils::matrix::MatrixView) (flat row-major),
//!   mat-vec multiply, SIMD softmax, ReLU
//! - Dispatch functions: [`simd_dot_f32`](simd_utils::simd_dot_f32),
//!   [`simd_norm_f32`](simd_utils::simd_norm_f32), [`simd_add_f32`](simd_utils::simd_add_f32),
//!   [`simd_sub_f32`](simd_utils::simd_sub_f32), [`simd_scale_f32`](simd_utils::simd_scale_f32)
//! - Reductions: [`reduce_sum_f32`](simd_utils::reduce_sum_f32),
//!   [`reduce_max_f32`](simd_utils::reduce_max_f32), [`argmax_f32`](simd_utils::argmax_f32)
//!
//! ### Similarity & Distance ([`similarity`])
//!
//! - **Cosine** — Single-pass 3-accumulator with mixed-precision f64 reduction
//! - **Jaccard** — Sorted set intersection O(n+m)
//! - **Top-K** — Partial selection for KNN search (parallel via rayon)
//! - **Distance** — Euclidean, Manhattan (SIMD), Pearson correlation, dot product
//! - Traits: [`Similarity<[T]>`](similarity::Similarity) (slice-based, preferred),
//!   [`CosineSimilarity`], [`JaccardSimilarity`]
//!
//! ### Statistics ([`statistics`])
//!
//! - [`WilsonRanker`] — Wilson score lower bound for ranking under uncertainty
//! - [`DriftDetector`] — KS statistic + JS divergence for concept drift detection
//! - [`reconciliation`](statistics::reconciliation) — Weighted mean, CV, Bayesian fusion
//!
//! ### Financial ([`financial`])
//!
//! - NPV, IRR (Newton-Raphson), stress scenario batch computation
//!
//! ### Quantization ([`quantization`]) — feature: `quantization`
//!
//! - Half-precision (f16) vectors via `half` crate — 2x memory reduction
//! - Scalar quantization (f32 → u8) — 4x memory reduction, ~97-99% accuracy
//!
//! ### Approximate Nearest Neighbor ([`ann`]) — feature: `ann`
//!
//! - HNSW index for O(log n) approximate search
//! - [`AnnIndex`](ann::AnnIndex) trait for pluggable implementations
//!
//! ### Utilities
//!
//! - [`buffer_pool`] — Thread-local buffer reuse for zero-allocation batch ops
//!
//! ## Integration Points
//!
//! - `touring-learning`: KNN search, adaptive batch sizing, dot product batches
//! - `touring-cortex`: Embedding similarity, batch comparison, top-K retrieval
//! - `touring-hooks`: SIMD-accelerated context injection scoring
//! - `touring-index`: File similarity via cosine
//! - `touring-antt`: Financial model computation (NPV, IRR, stress)
//!
//! ## ISA Support
//!
//! | ISA | Platform | Features |
//! |-----|----------|----------|
//! | AVX-512 | x86_64 (Intel 12th+, AMD Zen 4+) | 16×f32 lanes, FMA |
//! | AVX2+FMA | x86_64 (most modern CPUs) | 8×f32 lanes, FMA |
//! | NEON | aarch64 (Apple M1-M4, ARM servers) | 4×f32 lanes |
//! | Scalar | All platforms | Portable fallback |
//!
//! Detection is cached via `pulp::Arch::new()` — zero overhead after first call.

#![deny(missing_docs)]
// RBP-01 elite-lint ratchet (2026-06-16): prod-unwrap-free (3 NaN-unsafe float
// sorts fixed to unwrap_or(Ordering::Equal)) — lock against future bare unwrap in
// non-test code (`.expect("…")` stays the sanctioned escape).
#![cfg_attr(not(test), deny(clippy::unwrap_used))]

pub mod ann;
pub mod buffer_pool;
pub mod financial;
pub mod gpu;
pub mod quantization;
pub mod simd_utils;
pub mod similarity;
pub mod statistics;

// Re-exports for convenience
// ---------------------------

/// SIMD-accelerated similarity computations.
///
/// # Re-exports
/// - [`CosineComputer`] — Cosine similarity vector computer
/// - [`CosineSimilarity`] — Cosine similarity between vectors
/// - [`JaccardComputer`] — Jaccard index computer for set similarity
/// - [`JaccardSimilarity`] — Jaccard similarity score
/// - [`TopKSearcher`] — Top-K nearest neighbor search
/// - [`TopKResult`] — Result from top-K search operation
/// - [`Similarity`] — Core similarity trait
pub use similarity::{
    CosineComputer, CosineSimilarity, JaccardComputer, JaccardSimilarity, Similarity, TopKResult,
    TopKSearcher,
};

/// Statistical ranking and drift detection.
///
/// # Re-exports
/// - [`WilsonRanker`] — Wilson score confidence ranking
/// - [`DriftDetector`] — Distribution drift detection
/// - [`DriftDetection`] — Drift detection result type
pub use statistics::{DriftDetection, DriftDetector, WilsonRanker};

/// Distance/similarity metrics for vector operations.
///
/// # Re-exports
/// - [`euclidean`] — L2 Euclidean distance
/// - [`euclidean_batch`] — Batch Euclidean distance computation
/// - [`euclidean_batch_par`] — Parallel batch Euclidean distance
/// - [`manhattan`] — L1 Manhattan/city block distance
/// - [`manhattan_batch`] — Batch Manhattan distance
/// - [`manhattan_batch_par`] — Parallel batch Manhattan distance
/// - [`pearson_correlation`] — Pearson correlation coefficient
/// - [`pearson_batch`] — Batch Pearson correlation
/// - [`pearson_batch_par`] — Parallel batch Pearson correlation
/// - [`squared_euclidean`] — Squared Euclidean distance
/// - [`dot_product`] — Dot product of vectors
pub use similarity::distance::{
    chebyshev, chebyshev_batch, chebyshev_batch_par, dot_product, euclidean, euclidean_batch,
    euclidean_batch_par, manhattan, manhattan_batch, manhattan_batch_par, pearson_batch,
    pearson_batch_par, pearson_correlation, squared_euclidean,
};

#[cfg(feature = "learning-integration")]
pub mod learning;

#[cfg(feature = "cortex-integration")]
pub mod cortex;
