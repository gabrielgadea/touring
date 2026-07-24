//! Criterion latency bench for the CEG static-only fast path — **P4.6**.
//!
//! Pln2 plan: `docs/2026-05-17-ceg-pln2-plan.md`, deliverable P4.6.
//!
//! Measures `fast_path_decision` — the X1 classification + X2 static analysis
//! that decide whether a body can skip X5 SANDBOX. The P4.6 acceptance is
//! "P50 of the fast path < 8ms"; this bench records the regression floor for
//! that decision over both a provably-pure body and an impure one.
//!
//! Run: `cargo bench -p touring-hooks --bench fast_path`

use criterion::{Criterion, black_box, criterion_group, criterion_main};

use touring_hooks::gateway::{RawInvocation, fast_path_decision};

/// The fast-path decision over a provably-pure body and an impure one.
fn bench_fast_path_decision(c: &mut Criterion) {
    let pure = RawInvocation::new("SandboxPython", "def compute():\n    return 6 * 7 + 1");
    let impure = RawInvocation::new("SandboxPython", "import os\ndef f():\n    return 1");

    c.bench_function("fast_path/decision_pure", |b| {
        b.iter(|| black_box(fast_path_decision(black_box(&pure))));
    });
    c.bench_function("fast_path/decision_impure", |b| {
        b.iter(|| black_box(fast_path_decision(black_box(&impure))));
    });
}

criterion_group!(benches, bench_fast_path_decision);
criterion_main!(benches);
