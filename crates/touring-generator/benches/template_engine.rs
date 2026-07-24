//! Benchmark for `TemplateEngine::render` throughput.

use criterion::{Criterion, criterion_group, criterion_main};

fn bench_template_engine_noop(c: &mut Criterion) {
    // Placeholder benchmark — validates the harness compiles and links correctly.
    // Template rendering benchmarks require a constructed TemplateEngine + TelemetrySink.
    c.bench_function("template_engine_noop", |b| {
        b.iter(|| std::hint::black_box(0u64));
    });
}

criterion_group!(benches, bench_template_engine_noop);
criterion_main!(benches);
