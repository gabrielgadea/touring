//! Benchmark for `SpeculateBridge::validate` latency.

use criterion::{Criterion, criterion_group, criterion_main};

fn bench_speculate_bridge_noop(c: &mut Criterion) {
    // Placeholder benchmark — actual execution requires a live daemon.
    // This validates the benchmark harness compiles and links correctly.
    c.bench_function("speculate_bridge_noop", |b| {
        b.iter(|| {
            // No-op: daemon subprocess benches run in integration tests.
            std::hint::black_box(0u64)
        });
    });
}

criterion_group!(benches, bench_speculate_bridge_noop);
criterion_main!(benches);
