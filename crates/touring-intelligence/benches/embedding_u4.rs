//! Baseline benchmark — `EmbeddingU4::from_f32` quantization throughput.
//!
//! Records the time it takes to fold an `f32` embedding vector into the 4-bit
//! packed representation used by ANN recall. Landing a baseline now lets
//! future Wave 1 sessions compare candle-core ƒorward-pass + quantization
//! against this number and detect regressions in the SIMD quantizer itself.
//!
//! Run with: `cargo bench -p touring-learning --bench embedding_u4`.

use criterion::{BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main};
use touring_intelligence::rl::semantic::{Embedder, MockEmbedder};

#[cfg(feature = "u4-quantization")]
use touring_simd::quantization::EmbeddingU4;

#[cfg(feature = "u4-quantization")]
fn bench_from_f32(c: &mut Criterion) {
    let mut group = c.benchmark_group("EmbeddingU4::from_f32");

    // Common embedding sizes: MiniLM (384), BGE (768), Nomic (1024).
    for dims in [384_usize, 768, 1024] {
        // Use MockEmbedder so the bench is self-contained and deterministic
        // — no need for a real model or network fetch.
        let embedder = MockEmbedder::new(dims);
        let vec = embedder.embed("baseline benchmark input");

        group.throughput(Throughput::Elements(dims as u64));
        group.bench_with_input(BenchmarkId::from_parameter(dims), &vec, |b, v| {
            b.iter(|| {
                let e = EmbeddingU4::from_f32(black_box(v));
                black_box(e);
            });
        });
    }

    group.finish();
}

#[cfg(feature = "u4-quantization")]
fn bench_embed_then_quantize(c: &mut Criterion) {
    // End-to-end path: MockEmbedder::embed → EmbeddingU4::from_f32.
    // Measures the full "text → quantized vector" pipeline we intend to
    // preserve when swapping MockEmbedder for CandleEmbedder in Wave 1.
    let mut group = c.benchmark_group("pipeline::embed_then_quantize");

    let embedder = MockEmbedder::new(768);
    let texts = [
        "short",
        "a medium-length line representative of a single source file digest",
        &"tokens ".repeat(256),
    ];

    for (i, text) in texts.iter().enumerate() {
        group.bench_with_input(
            BenchmarkId::from_parameter(match i {
                0 => "short",
                1 => "medium",
                _ => "long",
            }),
            text,
            |b, t| {
                b.iter(|| {
                    let v = embedder.embed(black_box(t));
                    let e = EmbeddingU4::from_f32(&v);
                    black_box(e);
                });
            },
        );
    }

    group.finish();
}

// When the feature is disabled, provide a no-op main so the file still
// compiles under `cargo check --no-default-features`.
#[cfg(feature = "u4-quantization")]
criterion_group!(benches, bench_from_f32, bench_embed_then_quantize);

#[cfg(not(feature = "u4-quantization"))]
fn noop(_c: &mut Criterion) {}

#[cfg(not(feature = "u4-quantization"))]
criterion_group!(benches, noop);

criterion_main!(benches);
