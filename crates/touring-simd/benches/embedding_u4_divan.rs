//! Divan microbenchmark for `EmbeddingU4::from_f32`.
//!
//! Why divan in addition to criterion? Two reasons:
//!
//! 1. **Iteration speed**: divan completes a typical microbench run in
//!    1-3 seconds vs criterion's ~10s warm-up + sampling. Better feedback
//!    loop while iterating on the quantization pipeline.
//! 2. **Per-input grouping**: divan's `Bencher::with_inputs` makes
//!    parametric sweeps (over embedding dims) more concise than criterion's
//!    `BenchmarkId` ceremony.
//!
//! Run: `cargo bench -p touring-simd --bench embedding_u4_divan`
//!
//! Comparable criterion bench: `crates/touring-learning/benches/embedding_u4.rs`.

#![cfg(feature = "quantization")]

use divan::{Bencher, black_box};
use touring_simd::quantization::EmbeddingU4;

/// Deterministic input vectors of common embedding dimensions.
///
/// Sized to match real production embedders so the absolute numbers are
/// directly interpretable:
/// - 384  → `bge-micro-v2`, `bge-small-en-v1.5`, `MiniLM-L6`
/// - 768  → `bge-base`, `nomic-embed-text-v1.5`
/// - 1024 → `bge-large`, `nomic-embed-text-v2`
/// - 1536 → `OpenAI text-embedding-ada-002` (legacy)
const DIMS: &[usize] = &[384, 768, 1024, 1536];

/// Produce a deterministic f32 vector of `dim` elements in `[-1, 1)`.
///
/// Uses a simple LCG so the bench is reproducible and indifferent to OS
/// RNG state — both criterion and divan runs see the same input.
fn deterministic_vec(dim: usize) -> Vec<f32> {
    let mut state: u64 = 0xDEAD_BEEF_CAFE_BABE;
    (0..dim)
        .map(|_| {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
            let bits = (state >> 33) as f32;
            bits / (u32::MAX as f32 / 2.0) - 1.0
        })
        .collect()
}

#[divan::bench(args = DIMS)]
fn from_f32(bencher: Bencher, dim: usize) {
    let v = deterministic_vec(dim);
    bencher.bench(|| {
        let e = EmbeddingU4::from_f32(black_box(&v));
        black_box(e);
    });
}

#[divan::bench(args = DIMS)]
fn round_trip(bencher: Bencher, dim: usize) {
    // Measures the full quantize → dequantize cycle. Dominant cost is
    // from_f32 (allocating Vec<u8>) + to_f32 (allocating Vec<f32>).
    let v = deterministic_vec(dim);
    bencher.bench(|| {
        let e = EmbeddingU4::from_f32(black_box(&v));
        let r = e.to_f32();
        black_box(r);
    });
}

#[divan::bench(args = DIMS)]
fn approx_dot_self(bencher: Bencher, dim: usize) {
    // Critical hot path for ANN recall — runs N times per query in
    // touring-simd::ann::ann_search_u4. Anything regressing here shows
    // up immediately in semantic search latency.
    let v = deterministic_vec(dim);
    let q = EmbeddingU4::from_f32(&v);
    bencher.bench(|| {
        let dot = q.approx_dot(black_box(&q));
        black_box(dot);
    });
}

fn main() {
    divan::main();
}
