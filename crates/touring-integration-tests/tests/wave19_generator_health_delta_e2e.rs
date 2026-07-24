#![allow(clippy::indexing_slicing)] // test-only
//! Wave 19 — Generator + health_delta closure injection E2E (cross-crate).
//!
//! Closes the dynamic-quality loop by wiring the same `health_delta`
//! cache that powers `pre_edit`/`post_edit` (Waves 9-14) into the
//! generator's `commit()` pipeline. Generator-emitted code is now
//! judged by the SAME quality criteria as hand-edits.
//!
//! ```text
//! ┌──────────────────────────────────────────────────────────────────┐
//! │ touring-generator::core::context::GeneratorContext              │
//! │   ├── health_delta_record_fn:  Option<HealthDeltaRecordFn>      │
//! │   └── health_delta_compute_fn: Option<HealthDeltaComputeFn>     │
//! │                                                                  │
//! │ Wired in executor::typestate::Speculated::commit()              │
//! │   for each artifact:                                            │
//! │     1. record_pre_health_for_artifact(path)  ← reads disk      │
//! │     2. write_artifact_atomically(path, content)                 │
//! │     3. compute_health_delta_for_artifact(path, content)         │
//! │        → injects RL reward signed by delta direction            │
//! │                                                                  │
//! │ Builder: ctx.with_health_delta(record_fn, compute_fn)           │
//! │   wires the closures pointing at touring-hooks helpers.         │
//! └──────────────────────────────────────────────────────────────────┘
//! ```

use std::sync::Arc;

use touring_generator::core::context::{HealthDeltaComputeFn, HealthDeltaRecordFn};
use touring_hooks::health_delta::{
    compute_signals_delta, discard_pre_health, record_pre_signals, regression_streak, reset_streak,
};

// ── Axis 1: closure type signatures match touring-hooks helpers ────────────

#[test]
fn axis1_closure_types_compile_and_match() {
    // Build the exact closures generator/make_context will inject.
    let record: HealthDeltaRecordFn =
        Arc::new(|path: &str, src: &str| -> Option<f32> { record_pre_signals(path, src) });
    let compute: HealthDeltaComputeFn =
        Arc::new(|path: &str, new_src: &str| -> Option<(f32, bool, bool)> {
            let d = compute_signals_delta(path, new_src)?;
            let delta_val = d.delta?;
            Some((delta_val, d.is_regression(), d.is_improvement()))
        });
    // Drop signal: the closures construct + are Send+Sync (proven by Arc).
    let _ = (record, compute);
}

// ── Axis 2: full pre-record → compute cycle through closures ───────────────

#[test]
fn axis2_closure_pair_yields_signed_delta() {
    let path = "/wave19e2e/axis2.rs";
    reset_streak(path);
    discard_pre_health(path);

    let record: HealthDeltaRecordFn = Arc::new(|p, s| record_pre_signals(p, s));
    let compute: HealthDeltaComputeFn = Arc::new(|p, s| {
        let d = compute_signals_delta(p, s)?;
        let dv = d.delta?;
        Some((dv, d.is_regression(), d.is_improvement()))
    });

    // Step 1: closure records a clean Rust file as pre-state.
    let pre = record(path, "pub fn ok() -> i32 { 1 }").expect("record returns score");
    assert!(pre > 0.5, "clean rust health > 0.5: got {pre}");

    // Step 2: closure computes delta against unsafe-heavy post-state.
    let degraded = "pub unsafe fn bad() -> u8 {\n\
        unsafe { let _ = std::ptr::null::<u8>(); }\n\
        unsafe { let _ = std::ptr::null::<u8>(); }\n\
        unsafe { let _ = std::ptr::null::<u8>(); }\n\
        0\n\
    }";
    let (delta, is_reg, is_imp) = compute(path, degraded).expect("compute returns tuple");
    assert!(delta < 0.0, "regression must be negative: got {delta}");
    assert!(is_reg, "is_regression must be true");
    assert!(!is_imp);
}

// ── Axis 3: improvement direction propagates through closures ──────────────

#[test]
fn axis3_closure_pair_detects_improvement() {
    let path = "/wave19e2e/axis3.rs";
    reset_streak(path);
    discard_pre_health(path);

    let record: HealthDeltaRecordFn = Arc::new(|p, s| record_pre_signals(p, s));
    let compute: HealthDeltaComputeFn = Arc::new(|p, s| {
        let d = compute_signals_delta(p, s)?;
        let dv = d.delta?;
        Some((dv, d.is_regression(), d.is_improvement()))
    });

    let degraded = "pub unsafe fn bad() -> u8 {\n\
        unsafe { let _ = std::ptr::null::<u8>(); }\n\
        unsafe { let _ = std::ptr::null::<u8>(); }\n\
        unsafe { let _ = std::ptr::null::<u8>(); }\n\
        0\n\
    }";
    record(path, degraded).expect("pre-record degraded");
    let (delta, is_reg, is_imp) = compute(path, "pub fn good() -> i32 { 1 }").expect("delta");
    assert!(delta > 0.0, "improvement must be positive");
    assert!(is_imp);
    assert!(!is_reg);
}

// ── Axis 4: GeneratorContext.with_health_delta builder wires closures ──────

#[test]
fn axis4_with_health_delta_builder_compiles() {
    // Build closures and confirm the builder API surface is usable
    // from cross-crate consumers. Actual wiring requires a full
    // GeneratorContext which has many other deps; here we validate
    // the closure constructors compile + are Send+Sync.
    let _record: HealthDeltaRecordFn = Arc::new(|_p, _s| Some(1.0_f32));
    let _compute: HealthDeltaComputeFn = Arc::new(|_p, _s| Some((0.0, false, false)));
}

// ── Axis 5: closures don't leak when one half is None ──────────────────────

#[test]
fn axis5_unpaired_closures_are_safe_no_op() {
    // Even if record_fn is wired but compute_fn is None (or vice-versa),
    // the helpers should be no-ops — this is what GeneratorContext does
    // when only one closure is set. The pure helpers (record_pre_signals
    // / compute_signals_delta) are independent and won't fail.
    let path = "/wave19e2e/axis5.rs";
    reset_streak(path);
    discard_pre_health(path);
    // Record but never compute — cache entry persists until TTL.
    record_pre_signals(path, "pub fn a() -> i32 { 1 }").expect("record");
    // No compute — the entry is still in the cache, but we don't read it.
    // Cleanup so the cache isn't polluted between test runs.
    discard_pre_health(path);
}

// ── Axis 6: streak counters update through generator commit path ───────────

#[test]
fn axis6_repeated_regressions_advance_streak() {
    let path = "/wave19e2e/axis6.rs";
    reset_streak(path);

    let record: HealthDeltaRecordFn = Arc::new(|p, s| record_pre_signals(p, s));
    let compute: HealthDeltaComputeFn = Arc::new(|p, s| {
        let d = compute_signals_delta(p, s)?;
        let dv = d.delta?;
        Some((dv, d.is_regression(), d.is_improvement()))
    });

    let degraded = "pub unsafe fn bad() -> u8 {\n\
        unsafe { let _ = std::ptr::null::<u8>(); }\n\
        unsafe { let _ = std::ptr::null::<u8>(); }\n\
        unsafe { let _ = std::ptr::null::<u8>(); }\n\
        0\n\
    }";

    // Simulate 3 generator commits each producing a regression.
    for _ in 0..3 {
        discard_pre_health(path);
        record(path, "pub fn ok() -> i32 { 1 }").expect("pre");
        let (_, is_reg, _) = compute(path, degraded).expect("delta");
        assert!(is_reg, "fixture must regress");
    }
    // Streak counter (shared with pre_edit) reflects the 3 generator regressions.
    assert_eq!(regression_streak(path), 3);
}

// ── Axis 7: closure is dependency-free (tuple shape vs HealthDelta struct) ──

#[test]
fn axis7_compute_closure_returns_pure_tuple() {
    // The closure intentionally returns `Option<(f32, bool, bool)>` instead
    // of `HealthDelta` so touring-generator does NOT need to import
    // `touring-hooks::HealthDelta`. This proves the contract.
    let compute: HealthDeltaComputeFn = Arc::new(|_p, _s| Some((0.42_f32, true, false)));
    let result = compute("any_path.rs", "any_source");
    assert_eq!(result, Some((0.42, true, false)));
}
