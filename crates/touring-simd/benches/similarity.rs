//! Criterion benchmarks for touring-simd similarity operations.
//!
//! Covers cosine similarity (single + batch) and Jaccard similarity across
//! representative dimensions and set sizes.

use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use touring_simd::similarity::{
    CosineComputer, CosineSimilarity, JaccardComputer, JaccardSimilarity,
};

/// Generate a deterministic pseudo-random f32 vector of given dimension.
fn gen_f32_vec(dim: usize, seed: u64) -> Vec<f32> {
    let mut val = seed;
    (0..dim)
        .map(|_| {
            val ^= val << 13;
            val ^= val >> 7;
            val ^= val << 17;
            (val as f32 / u64::MAX as f32) * 2.0 - 1.0
        })
        .collect()
}

fn bench_cosine_similarity(c: &mut Criterion) {
    let computer = CosineComputer::new();
    let mut group = c.benchmark_group("cosine_similarity");

    for dim in [384, 768, 1536] {
        let a = gen_f32_vec(dim, 42);
        let b = gen_f32_vec(dim, 137);

        group.bench_with_input(BenchmarkId::new("single", dim), &dim, |bencher, _| {
            bencher.iter(|| computer.cosine(black_box(&a), black_box(&b)));
        });
    }

    group.finish();
}

fn bench_cosine_batch(c: &mut Criterion) {
    let dims = [64, 256, 512, 1024, 1536];
    let batch_sizes = [10, 50, 100];

    let computer = CosineComputer::new();
    let mut group = c.benchmark_group("cosine_batch");

    for &dim in &dims {
        let base: Vec<f32> = (0..dim).map(|i| (i as f32 * 0.01).sin()).collect();

        for &batch_size in &batch_sizes {
            let queries: Vec<Vec<f32>> = (0..batch_size).map(|_| base.clone()).collect();
            let candidates: Vec<Vec<f32>> = (0..batch_size).map(|_| base.clone()).collect();

            let id = format!("{}d_x{}", dim, batch_size);
            group.bench_function(BenchmarkId::from_parameter(&id), |bencher| {
                bencher.iter(|| computer.cosine_batch(black_box(&queries), black_box(&candidates)));
            });
        }
    }

    group.finish();
}

fn bench_cosine_high_dim(c: &mut Criterion) {
    let computer = CosineComputer::new();
    let a = gen_f32_vec(1536, 42);
    let b = gen_f32_vec(1536, 137);

    c.bench_function("cosine_1536d_single", |bencher| {
        bencher.iter(|| computer.cosine(black_box(&a), black_box(&b)));
    });
}

fn bench_jaccard(c: &mut Criterion) {
    let sizes = [100, 1000, 10000];

    let computer = JaccardComputer::new();
    let mut group = c.benchmark_group("jaccard");

    for &size in &sizes {
        let a: Vec<u32> = (0..size as u32).map(|i| i * 2).collect();
        let b: Vec<u32> = (0..size as u32).map(|i| i * 2 + 1).collect();

        group.bench_function(BenchmarkId::from_parameter(size), |bencher| {
            bencher.iter(|| computer.jaccard(black_box(&a), black_box(&b)));
        });
    }

    group.finish();
}

fn bench_jaccard_batch(c: &mut Criterion) {
    let computer = JaccardComputer::new();
    let mut group = c.benchmark_group("jaccard_batch");

    let queries: Vec<Vec<u32>> = (0..10)
        .map(|i| (0..1000u32).map(|j| i * 1000 + j * 2).collect())
        .collect();
    let candidates: Vec<Vec<u32>> = (0..100)
        .map(|i| (0..1000u32).map(|j| i * 1000 + j * 2 + 1).collect())
        .collect();

    group.bench_function("10q_100c_1k", |bencher| {
        bencher.iter(|| computer.jaccard_batch(black_box(&queries), black_box(&candidates)));
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_cosine_similarity,
    bench_cosine_batch,
    bench_cosine_high_dim,
    bench_jaccard,
    bench_jaccard_batch,
);
criterion_main!(benches);
