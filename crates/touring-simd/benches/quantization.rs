//! Criterion benchmarks for block-wise quantization.
//!
//! Run with: cargo bench -p touring-simd --bench quantization
//!
//! Measures:
//! - Round-trip error (quantize → dequantize) per block size
//! - Accuracy vs f32 dot product baseline
//! - Throughput (ops/sec) for dot_blockwise vs naive f32 dot

use criterion::{BatchSize, BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use std::time::Instant;

use touring_simd::quantization::{BlockQuantizer, ScalarQuantizer};

// ── Deterministic test data ─────────────────────────────────────────────────

fn rng_vec(dim: usize, seed: u64) -> Vec<f32> {
    // Simple LCG RNG for reproducibility
    let mut state = seed;
    (0..dim)
        .map(|_| {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
            // Map u64 → f32 in [-1, 1)
            let bits = (state >> 33) as f32;
            bits / (u32::MAX as f32 / 2.0) - 1.0
        })
        .collect()
}

// ── Round-trip error benchmark ───────────────────────────────────────────────

fn round_trip_error(data: &[f32], block_size: usize) -> f64 {
    let q = BlockQuantizer::fit_blockwise(data, block_size);
    let nibbles = q.quantize_blockwise(data);
    let recovered = q.dequantize_blockwise(&nibbles);

    let mut max_rel_err = 0.0_f64;
    for (orig, &rec) in data.iter().zip(recovered.iter()) {
        let rel_err = if orig.abs() >= 1e-6 {
            ((orig - rec).abs() / orig.abs()) as f64
        } else {
            (orig - rec).abs() as f64
        };
        if rel_err > max_rel_err {
            max_rel_err = rel_err;
        }
    }
    max_rel_err
}

fn bench_round_trip(c: &mut Criterion) {
    let mut group = c.benchmark_group("round_trip_error");

    let dims = [256, 1024, 4096];
    let block_sizes = [16, 32, 64, 128];

    for &dim in &dims {
        // Pre-generate data outside the benchmark loop
        let data = rng_vec(dim, 42);

        for &block_size in &block_sizes {
            group.bench_with_input(
                BenchmarkId::new(dim.to_string(), block_size.to_string()),
                &(&data, block_size),
                |bencher, (data, bs)| {
                    bencher.iter_batched(
                        || ((*data).clone(), *bs),
                        |(d, bs)| {
                            let err = round_trip_error(&d, bs);
                            black_box(err)
                        },
                        BatchSize::SmallInput,
                    );
                },
            );
        }
    }

    group.finish();
}

// ── Accuracy vs f32 baseline ───────────────────────────────────────────────

fn bench_accuracy_vs_f32_impl(dim: usize, n_samples: usize) -> (f64, f64) {
    // n_samples = number of vector pairs to test
    let mut max_rel_err = 0.0_f64;
    let mut sum_rel_err = 0.0_f64;

    for i in 0..n_samples {
        let seed_a = (i * 2) as u64;
        let seed_b = (i * 2 + 1) as u64;

        let a = rng_vec(dim, seed_a);
        let b = rng_vec(dim, seed_b);

        // Naive f32 dot (ground truth)
        let exact_dot: f64 = a
            .iter()
            .zip(b.iter())
            .map(|(x, y)| (*x as f64) * (*y as f64))
            .sum();

        // Block-wise quantized dot
        let q = BlockQuantizer::fit_blockwise(&a, 32);
        let qa = q.quantize_blockwise(&a);
        let qb = q.quantize_blockwise(&b);
        let approx_dot = q.dot_blockwise(&qa, &qb) as f64;

        let rel_err = if exact_dot.abs() >= 1e-6 {
            ((exact_dot - approx_dot).abs() / exact_dot.abs()).min(1.0)
        } else {
            (exact_dot - approx_dot).abs().min(1.0)
        };

        max_rel_err = max_rel_err.max(rel_err);
        sum_rel_err += rel_err;
    }

    let avg_rel_err = sum_rel_err / n_samples as f64;
    (max_rel_err * 100.0, avg_rel_err * 100.0)
}

fn bench_accuracy_vs_f32(c: &mut Criterion) {
    let mut group = c.benchmark_group("accuracy_vs_f32");

    let dims = [256, 1024, 4096];
    let n_samples = 100;

    for &dim in &dims {
        group.bench_function(
            BenchmarkId::new(dim.to_string(), n_samples.to_string()),
            |bencher| {
                bencher.iter(|| {
                    let result = bench_accuracy_vs_f32_impl(dim, n_samples);
                    black_box(result)
                });
            },
        );
    }

    group.finish();
}

// ── Throughput benchmark ─────────────────────────────────────────────────────

/// Standalone throughput helper — kept for ad-hoc manual runs outside the
/// criterion framework (e.g. `cargo bench --bench quantization -- --nocapture`
/// call-sites that print raw ops/sec). Not invoked by the criterion groups.
#[allow(dead_code)]
fn bench_throughput_impl(dim: usize, block_size: usize, n_iters: usize) -> f64 {
    let a = rng_vec(dim, 42);
    let b = rng_vec(dim, 137);

    let q = BlockQuantizer::fit_blockwise(&a, block_size);
    let qa = q.quantize_blockwise(&a);
    let qb = q.quantize_blockwise(&b);

    let start = Instant::now();
    for _ in 0..n_iters {
        let _ = black_box(q.dot_blockwise(&qa, &qb));
    }
    let elapsed = start.elapsed().as_secs_f64();

    n_iters as f64 / elapsed
}

fn naive_dot(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}

fn bench_throughput_dot(c: &mut Criterion) {
    let mut group = c.benchmark_group("throughput_dot");

    let dim = 4096;
    let block_size = 32;
    let n_iters = 1000;

    // Pre-generate data once
    let a = rng_vec(dim, 42);
    let b = rng_vec(dim, 137);
    let q = BlockQuantizer::fit_blockwise(&a, block_size);
    let qa = q.quantize_blockwise(&a);
    let qb = q.quantize_blockwise(&b);

    // Baseline: naive f32 dot
    group.bench_function("naive_f32_dot", |bencher| {
        bencher.iter_batched(
            || (a.clone(), b.clone()),
            |(aa, bb)| {
                let result = naive_dot(&aa, &bb);
                black_box(result)
            },
            BatchSize::SmallInput,
        );
    });

    // Quantized dot_blockwise
    group.bench_function("dot_blockwise", |bencher| {
        bencher.iter_batched(
            || (qa.clone(), qb.clone(), &q),
            |(qa, qb, q)| {
                let result = q.dot_blockwise(&qa, &qb);
                black_box(result)
            },
            BatchSize::SmallInput,
        );
    });

    // ops/sec for dot_blockwise
    let start = Instant::now();
    for _ in 0..n_iters {
        let _ = q.dot_blockwise(&qa, &qb);
    }
    let elapsed = start.elapsed().as_secs_f64();
    let ops_per_sec = n_iters as f64 / elapsed;

    println!(
        "\n  Throughput: dot_blockwise dim={dim} block_size={block_size}: {ops_per_sec:.0} ops/sec"
    );

    group.finish();
}

// ── Strong scaling benchmark ─────────────────────────────────────────────────

fn bench_scaling(c: &mut Criterion) {
    let mut group = c.benchmark_group("strong_scaling_dot_blockwise");

    let dims = [256, 1024, 4096, 16384];
    let block_size = 32;
    let _n_iters = 500;

    for &dim in &dims {
        let a = rng_vec(dim, 42);
        let b = rng_vec(dim, 137);
        let q = BlockQuantizer::fit_blockwise(&a, block_size);
        let qa = q.quantize_blockwise(&a);
        let qb = q.quantize_blockwise(&b);

        group.bench_with_input(
            BenchmarkId::new(dim.to_string(), block_size.to_string()),
            &dim,
            |bencher, &_d| {
                bencher.iter_batched(
                    || (qa.clone(), qb.clone(), &q),
                    |(qa, qb, q)| {
                        let _ = black_box(q.dot_blockwise(&qa, &qb));
                    },
                    BatchSize::SmallInput,
                );
            },
        );
    }

    group.finish();
}

// ── Scalar quantizer baseline ────────────────────────────────────────────────

fn bench_scalar_quantizer_accuracy(c: &mut Criterion) {
    let mut group = c.benchmark_group("scalar_quantizer_accuracy");

    let dim = 4096;
    let n_samples = 100;
    let _n_iters = 100;

    group.bench_function(
        BenchmarkId::new(dim.to_string(), n_samples.to_string()),
        |bencher| {
            let mut max_rel_err = 0.0_f64;
            let mut sum_rel_err = 0.0_f64;

            bencher.iter_batched(
                || {
                    (0..n_samples)
                        .map(|i| rng_vec(dim, i as u64))
                        .collect::<Vec<_>>()
                },
                |datasets| {
                    for (i, data) in datasets.into_iter().enumerate() {
                        let target = rng_vec(dim, (i + 1000) as u64);
                        let mut all = data.clone();
                        all.extend_from_slice(&target);
                        let q = ScalarQuantizer::fit(&all);
                        let qa = q.quantize(&data);
                        let qb = q.quantize(&target);
                        let exact_dot: f64 = data
                            .iter()
                            .zip(target.iter())
                            .map(|(x, y)| (*x as f64) * (*y as f64))
                            .sum();
                        let approx_dot = q.dot_quantized(&qa, &qb) as f64;
                        let rel_err = if exact_dot.abs() >= 1e-6 {
                            ((exact_dot - approx_dot).abs() / exact_dot.abs()).min(1.0)
                        } else {
                            (exact_dot - approx_dot).abs().min(1.0)
                        };
                        max_rel_err = max_rel_err.max(rel_err);
                        sum_rel_err += rel_err;
                    }
                    (max_rel_err * 100.0, sum_rel_err / n_samples as f64 * 100.0)
                },
                BatchSize::LargeInput,
            );
        },
    );

    group.finish();
}

// ── Main ────────────────────────────────────────────────────────────────────

criterion_group!(
    name = benches;
    config = Criterion::default().sample_size(10);
    targets =
        bench_round_trip,
        bench_accuracy_vs_f32,
        bench_throughput_dot,
        bench_scaling,
        bench_scalar_quantizer_accuracy,
);
criterion_main!(benches);
