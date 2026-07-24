//! Criterion latency bench for the CEG content-hash dry-run cache — **P4.5**.
//!
//! Pln2 plan: `docs/2026-05-17-ceg-pln2-plan.md`, deliverable P4.5.
//!
//! Measures the X5 dry-run **cache-hit** path: `dry_run_cache_key` (a BLAKE3
//! digest over tool + payload + profile) followed by a `moka` lookup. The P4.5
//! acceptance is "P99 of a cache hit < 5ms"; this bench records the regression
//! floor for that path — sub-microsecond in practice, orders below the budget.
//!
//! Run: `cargo bench -p touring-hooks --bench dry_run_cache`

use criterion::{Criterion, black_box, criterion_group, criterion_main};

use touring_hooks::capability::{CapabilityProfile, Decision};
use touring_hooks::gateway::{
    CacheConfig, DryRunCache, OutputSummary, RawInvocation, SandboxCapabilities, SandboxOutcome,
    dry_run_cache_key,
};

/// A representative cached outcome for the warm-cache bench.
fn warm_outcome() -> SandboxOutcome {
    SandboxOutcome {
        exit_code: 0,
        output_bytes: 16,
        was_truncated: false,
        timed_out: false,
        content_hash: "benchhash".to_owned(),
        capability_profile: "bench".to_owned(),
        // C5 (2026-06-29): SandboxOutcome gained an inline output summary; an empty
        // exit-0 digest is the representative warm-cache value for this bench.
        summary: OutputSummary::empty(0),
    }
}

/// The BLAKE3 content-key computation in isolation.
fn bench_cache_key(c: &mut Criterion) {
    let raw = RawInvocation::new("Bash", "echo p45-bench-probe");
    let caps = SandboxCapabilities::from_profile(&CapabilityProfile::new("bench", Decision::Allow));
    c.bench_function("dry_run_cache/key_blake3", |b| {
        b.iter(|| black_box(dry_run_cache_key(black_box(&raw), black_box(&caps))));
    });
}

/// The full cache-hit path the X5 runner pays: compute the content key, then
/// look it up in a pre-warmed cache.
fn bench_cache_hit(c: &mut Criterion) {
    let raw = RawInvocation::new("Bash", "echo p45-bench-probe");
    let caps = SandboxCapabilities::from_profile(&CapabilityProfile::new("bench", Decision::Allow));
    let cache = DryRunCache::new(CacheConfig::default());
    cache.insert(dry_run_cache_key(&raw, &caps), warm_outcome());

    c.bench_function("dry_run_cache/full_hit", |b| {
        b.iter(|| {
            let key = dry_run_cache_key(black_box(&raw), black_box(&caps));
            black_box(cache.get(black_box(&key)))
        });
    });
}

criterion_group!(benches, bench_cache_key, bench_cache_hit);
criterion_main!(benches);
