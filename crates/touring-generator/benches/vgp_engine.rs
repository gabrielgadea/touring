#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::pedantic
)]

//! Criterion benchmarks for `VgpEngine` — `verify_batch` latency at various batch sizes.
//!
//! Wave gate: P50 < 5ms for batch=10 (moka cache warm path).
//!
//! Run: `cargo bench -p touring-generator --bench vgp_engine`

use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use std::sync::Arc;
use touring_generator::{Contracts, NoopTelemetry, SymbolRef, TelemetrySink, VgpEngine};
use uuid::Uuid;

fn build_contracts(n: usize) -> Contracts {
    let mut c = Contracts::default();
    for i in 0..n {
        c.symbols_must_exist
            .push(SymbolRef::named(format!("bench_symbol_{i}")));
    }
    c
}

fn bench_vgp_empty(c: &mut Criterion) {
    let metrics: Arc<dyn TelemetrySink> = Arc::new(NoopTelemetry);
    let engine = VgpEngine::with_subprocess(metrics);
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let contracts = Contracts::default();

    c.bench_function("vgp_verify_batch/empty", |b| {
        b.iter(|| {
            rt.block_on(async {
                black_box(
                    engine
                        .verify_batch(black_box(&contracts), Uuid::new_v4())
                        .await,
                )
            })
        });
    });
}

fn bench_vgp_batch_sizes(c: &mut Criterion) {
    let metrics: Arc<dyn TelemetrySink> = Arc::new(NoopTelemetry);
    let engine = VgpEngine::with_subprocess(metrics);
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");

    // Warm the cache first
    let warmup = build_contracts(10);
    rt.block_on(async {
        let _ = engine.verify_batch(&warmup, Uuid::new_v4()).await;
    });

    let mut group = c.benchmark_group("vgp_verify_batch");

    for batch_size in [1_usize, 5, 10, 20, 50] {
        let contracts = build_contracts(batch_size);

        group.bench_with_input(
            BenchmarkId::from_parameter(batch_size),
            &contracts,
            |b, contracts| {
                b.iter(|| {
                    rt.block_on(async {
                        black_box(
                            engine
                                .verify_batch(black_box(contracts), Uuid::new_v4())
                                .await,
                        )
                    })
                });
            },
        );
    }
    group.finish();
}

fn bench_vgp_cache_warm_vs_cold(c: &mut Criterion) {
    let metrics: Arc<dyn TelemetrySink> = Arc::new(NoopTelemetry);
    let engine = VgpEngine::with_subprocess(metrics);
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let contracts = build_contracts(10);

    // Cold path: reset counters and invalidate.
    let mut group = c.benchmark_group("vgp_cache");

    group.bench_function("cold_10", |b| {
        b.iter(|| {
            engine.reset_counters();
            engine.invalidate_all();
            rt.block_on(async {
                black_box(
                    engine
                        .verify_batch(black_box(&contracts), Uuid::new_v4())
                        .await,
                )
            })
        });
    });

    // Warm path: same contracts (already cached from previous runs).
    group.bench_function("warm_10", |b| {
        b.iter(|| {
            rt.block_on(async {
                black_box(
                    engine
                        .verify_batch(black_box(&contracts), Uuid::new_v4())
                        .await,
                )
            })
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_vgp_empty,
    bench_vgp_batch_sizes,
    bench_vgp_cache_warm_vs_cold,
);
criterion_main!(benches);
