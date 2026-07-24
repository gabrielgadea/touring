//! Criterion benchmarks for PrioritizedPipeline hot paths.
//!
//! Covers three hot-path operations:
//!   1. `enqueue`               — record an edit + insert into pending set
//!   2. `enqueue_with_priority` — same + blast-radius weight update
//!   3. `next_hot` (100 files)  — priority-ordered dequeue from a realistic queue

use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use touring_code::ast::incremental_pipeline::PrioritizedPipeline;

// ── helpers ──────────────────────────────────────────────────────────────────

/// Build a [`PrioritizedPipeline`] pre-loaded with `n` files at varying
/// priorities so `next_hot` has a realistic ordered queue to drain.
fn build_pipeline_with_files(n: usize) -> PrioritizedPipeline {
    let mut pp = PrioritizedPipeline::new(n.max(16));
    for i in 0..n {
        // Alternate priorities so the heat-map has a non-trivial sort
        let weight = if i % 3 == 0 { 3.0 } else { 1.0 };
        pp.enqueue_with_priority(format!("src/file_{i}.rs"), weight);
    }
    pp
}

// ── benchmarks ───────────────────────────────────────────────────────────────

fn bench_enqueue(c: &mut Criterion) {
    c.bench_function("PrioritizedPipeline::enqueue", |b| {
        b.iter_batched(
            || PrioritizedPipeline::new(16),
            |mut pp| {
                pp.enqueue(black_box("src/hot.rs".to_string()));
            },
            criterion::BatchSize::SmallInput,
        );
    });
}

fn bench_enqueue_with_priority(c: &mut Criterion) {
    c.bench_function("PrioritizedPipeline::enqueue_with_priority", |b| {
        b.iter_batched(
            || PrioritizedPipeline::new(16),
            |mut pp| {
                pp.enqueue_with_priority(black_box("src/hot.rs".to_string()), black_box(5.0_f64));
            },
            criterion::BatchSize::SmallInput,
        );
    });
}

fn bench_next_hot(c: &mut Criterion) {
    let mut group = c.benchmark_group("PrioritizedPipeline::next_hot");

    for &file_count in &[10usize, 50, 100] {
        group.bench_with_input(
            BenchmarkId::new("files", file_count),
            &file_count,
            |b, &n| {
                b.iter_batched(
                    || build_pipeline_with_files(n),
                    |mut pp| {
                        // Drain the entire priority queue
                        while let Some(path) = pp.next_hot() {
                            black_box(path);
                        }
                    },
                    criterion::BatchSize::SmallInput,
                );
            },
        );
    }

    group.finish();
}

// ── registry ─────────────────────────────────────────────────────────────────

criterion_group!(
    benches,
    bench_enqueue,
    bench_enqueue_with_priority,
    bench_next_hot
);
criterion_main!(benches);
