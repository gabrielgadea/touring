//! Criterion benchmarks for IncrementalPipeline lazy loading.
//!
//! Measures `queue_for_lazy` + `ensure_loaded` throughput for N files.
//! Baseline is stored per CI run; regressions > 10% fail CI.

use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use touring_code::ast::file_heat::HeatMap;
use touring_code::ast::incremental_pipeline::IncrementalPipeline;

/// Generate a synthetic Rust source with `n` functions.
fn rust_source(n: usize) -> String {
    (0..n)
        .map(|i| format!("pub fn f_{i}(x: i32) -> i32 {{ x + {i} }}\n"))
        .collect()
}

/// Benchmark: queue N files then ensure_loaded for each.
fn bench_queue_and_load(c: &mut Criterion) {
    let mut group = c.benchmark_group("lazy_loading");

    for n in [4, 16, 64] {
        let sources: Vec<(String, String)> = (0..n)
            .map(|i| (format!("src/module_{i}.rs"), rust_source(4)))
            .collect();

        group.bench_with_input(
            BenchmarkId::new("queue_and_load", n),
            &sources,
            |b, srcs| {
                b.iter(|| {
                    let mut pipeline = IncrementalPipeline::new();
                    for (path, src) in srcs {
                        pipeline.queue_for_lazy(black_box(path), black_box(src));
                    }
                    for (path, _) in srcs {
                        let _ = pipeline.ensure_loaded(black_box(path));
                    }
                });
            },
        );
    }

    group.finish();
}

/// Benchmark: prefetch_from_heat with a heat map of N hot files.
fn bench_prefetch_from_heat(c: &mut Criterion) {
    let mut group = c.benchmark_group("prefetch_from_heat");

    for n in [4, 16] {
        let mut heat = HeatMap::new(n + 1);
        let sources: Vec<(String, String)> = (0..n)
            .map(|i| {
                let path = format!("src/hot_{i}.rs");
                heat.record_access(&path, i as f64);
                (path, rust_source(4))
            })
            .collect();

        group.bench_with_input(
            BenchmarkId::new("prefetch", n),
            &(sources, heat),
            |b, (srcs, hm)| {
                b.iter(|| {
                    let mut pipeline = IncrementalPipeline::new();
                    for (path, src) in srcs {
                        pipeline.queue_for_lazy(black_box(path), black_box(src));
                    }
                    let _ = pipeline.prefetch_from_heat(black_box(hm), black_box(n));
                });
            },
        );
    }

    group.finish();
}

criterion_group!(benches, bench_queue_and_load, bench_prefetch_from_heat);
criterion_main!(benches);
