//! Divan microbenchmark for `gate_metrics` atomic counters (Wave 4 A2, 2026-04-14).
//!
//! Validates the assumption stated in `gate_metrics.rs:18-19` that
//! `AtomicU64::fetch_add(Ordering::Relaxed)` costs ~1ns per increment.
//! If divan reports significantly above 5ns per call, something on the
//! hot path regressed — re-check the `#[inline]` attribute and any
//! intervening cache-line contention.
//!
//! Why divan over criterion: iteration speed (~1-3s) over criterion's
//! ~10s warm-up. Same accuracy at the ns scale per divan's design.
//!
//! Run: `cargo bench -p touring-hooks --bench gate_metrics_divan`

use divan::{Bencher, black_box};
use touring_hooks::shared::gate_metrics::{
    record_pre_edit_fast_path, record_pre_edit_full, record_rkyv_dispatch, record_rkyv_parse_error,
    record_rkyv_response,
};

/// Baseline: pre_edit fast-path counter — the hottest counter in production.
/// Hit on every Edit/Write tool call when CILA gating skips enrichment.
#[divan::bench]
fn pre_edit_fast_path(b: Bencher) {
    b.bench(|| {
        record_pre_edit_fast_path();
        black_box(());
    });
}

/// pre_edit full enrichment — hit when CILA gate opens. Same code path
/// as fast_path, paired here so a regression on EITHER becomes visible.
#[divan::bench]
fn pre_edit_full(b: Bencher) {
    b.bench(|| {
        record_pre_edit_full();
        black_box(());
    });
}

/// rkyv inbound dispatch counter — Wave 3 D1. Two atomic ops:
/// fetch_add on `rkyv_dispatch_count` AND on `rkyv_dispatch_bytes`.
/// Expect roughly 2× the cost of `pre_edit_fast_path`.
#[divan::bench]
fn rkyv_dispatch(b: Bencher) {
    b.bench(|| {
        record_rkyv_dispatch(black_box(101));
        black_box(());
    });
}

/// rkyv parse error counter — single atomic. Should match the cost of
/// `pre_edit_fast_path` within noise (both are 1× fetch_add Relaxed).
#[divan::bench]
fn rkyv_parse_error(b: Bencher) {
    b.bench(|| {
        record_rkyv_parse_error();
        black_box(());
    });
}

/// rkyv outbound response counter.
#[divan::bench]
fn rkyv_response(b: Bencher) {
    b.bench(|| {
        record_rkyv_response();
        black_box(());
    });
}

fn main() {
    divan::main();
}
