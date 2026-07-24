//! Criterion benchmarks for CoEditPredictor RRF fusion.

use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use touring_intelligence::reasoning::coedit_predictor::CoEditPredictor;

/// Generate a ranked list of `n` files with decreasing scores.
fn make_ranked_list(prefix: &str, n: usize) -> Vec<(String, f64)> {
    (0..n)
        .map(|i| (format!("{prefix}_{i}.rs"), (n - i) as f64))
        .collect()
}

fn bench_predict_next_files(c: &mut Criterion) {
    let mut group = c.benchmark_group("coedit_predict_next");

    for &candidate_count in &[10usize, 50, 100] {
        let coedits = make_ranked_list("coedit", candidate_count);
        let imports = make_ranked_list("import", candidate_count);
        let blast = make_ranked_list("blast", candidate_count);

        group.bench_with_input(
            BenchmarkId::new("candidates", candidate_count),
            &candidate_count,
            |b, _| {
                b.iter(|| {
                    CoEditPredictor::predict_next_files(
                        black_box(&coedits),
                        black_box(&imports),
                        black_box(&blast),
                        black_box(10),
                    )
                });
            },
        );
    }

    group.finish();
}

fn bench_predict_with_overlap(c: &mut Criterion) {
    let mut group = c.benchmark_group("coedit_predict_overlap");

    // Create lists where files overlap significantly
    for &size in &[20usize, 50, 100] {
        let shared: Vec<(String, f64)> = (0..size)
            .map(|i| (format!("shared_{i}.rs"), (size - i) as f64))
            .collect();
        // All three lists have the same files (maximum overlap)
        let coedits = shared.clone();
        let imports = shared.clone();
        let blast = shared;

        group.bench_with_input(BenchmarkId::new("full_overlap", size), &size, |b, _| {
            b.iter(|| {
                CoEditPredictor::predict_next_files(
                    black_box(&coedits),
                    black_box(&imports),
                    black_box(&blast),
                    black_box(10),
                )
            });
        });
    }

    group.finish();
}

fn bench_predict_empty_inputs(c: &mut Criterion) {
    let coedits = make_ranked_list("coedit", 50);
    let empty: Vec<(String, f64)> = vec![];

    let mut group = c.benchmark_group("coedit_predict_sparse");

    group.bench_function("one_list_only", |b| {
        b.iter(|| {
            CoEditPredictor::predict_next_files(
                black_box(&coedits),
                black_box(&empty),
                black_box(&empty),
                black_box(10),
            )
        });
    });

    group.bench_function("all_empty", |b| {
        b.iter(|| {
            CoEditPredictor::predict_next_files(
                black_box(&empty),
                black_box(&empty),
                black_box(&empty),
                black_box(10),
            )
        });
    });

    group.finish();
}

fn bench_predict_top_k_variations(c: &mut Criterion) {
    let mut group = c.benchmark_group("coedit_predict_top_k");
    let coedits = make_ranked_list("coedit", 100);
    let imports = make_ranked_list("import", 100);
    let blast = make_ranked_list("blast", 100);

    for &k in &[5usize, 10, 25, 50] {
        group.bench_with_input(BenchmarkId::new("top_k", k), &k, |b, &top_k| {
            b.iter(|| {
                CoEditPredictor::predict_next_files(
                    black_box(&coedits),
                    black_box(&imports),
                    black_box(&blast),
                    black_box(top_k),
                )
            });
        });
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_predict_next_files,
    bench_predict_with_overlap,
    bench_predict_empty_inputs,
    bench_predict_top_k_variations,
);
criterion_main!(benches);
