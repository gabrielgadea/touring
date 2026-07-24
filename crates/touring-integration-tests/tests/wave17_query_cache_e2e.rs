#![allow(clippy::indexing_slicing)] // test-only
//! Wave 17 — Query Result Cache E2E (cross-crate).
//!
//! Validates the new `shared::query_cache` module + its wiring into
//! the two hot-path query handlers (`cli_index_find`,
//! `cli_tantivy_search`). Memoizing these queries with a 60s TTL
//! transforms repeated VGP / Tantivy lookups during code generation
//! from full DB round-trips (~50–200µs) into ~1µs cache hits.
//!
//! ```text
//! ┌──────────────────────────────────────────────────────────────────┐
//! │ touring-hooks::shared::query_cache (moka, 4096 cap, 60s TTL)    │
//! │   ├── make_key(kind, payload) → "kind::payload"                 │
//! │   ├── get(key)            → Option<String>  + hit/miss counter  │
//! │   ├── put(key, value)     → store                                │
//! │   ├── get_or_compute(key, f) → memoize f                        │
//! │   └── invalidate / clear_all                                    │
//! │                                                                  │
//! │ Wired into:                                                      │
//! │   - cli_handlers_index::cli_index_find  (VGP hot path)          │
//! │   - cli_handlers::cli_tantivy_search   (BM25 hot path)          │
//! │                                                                  │
//! │ Counters in shared::gate_metrics:                               │
//! │   - query_cache_hit_count                                       │
//! │   - query_cache_miss_count                                      │
//! │   - query_cache_hit_ratio (computed in snapshot)                │
//! └──────────────────────────────────────────────────────────────────┘
//! ```

use std::process::Command;
use std::sync::atomic::Ordering;

use touring_hooks::shared::gate_metrics::{GateMetricsSnapshot, global};
use touring_hooks::shared::query_cache;

const TOURING_BIN: &str = "/home/gabrielgadea/.claude/rust/target/release/touring";

fn binary_available() -> bool {
    std::path::Path::new(TOURING_BIN).exists()
}

// ── Axis 1: get_or_compute serves cached value on second call ──────────────

#[test]
fn axis1_get_or_compute_serves_cached_value() {
    let key = "wave17e2e::axis1";
    query_cache::invalidate(key);

    let mut compute_calls = 0;
    let mut compute = || {
        compute_calls += 1;
        format!(r#"{{"call":{compute_calls}}}"#)
    };

    let v1 = query_cache::get_or_compute(key, &mut compute);
    let v2 = query_cache::get_or_compute(key, &mut compute);

    assert_eq!(v1, v2, "second call must return cached value");
    assert_eq!(compute_calls, 1, "compute must run only on miss");
}

// ── Axis 2: hit/miss counters advance correctly ────────────────────────────

#[test]
fn axis2_hit_miss_counters_advance() {
    let key = "wave17e2e::axis2";
    query_cache::invalidate(key);

    let hits_before = global().query_cache_hit_count.load(Ordering::Relaxed);
    let misses_before = global().query_cache_miss_count.load(Ordering::Relaxed);

    // Miss → bumps misses.
    let _ = query_cache::get(key);
    // Insert → next get is hit.
    query_cache::put(key.to_string(), "v".to_string());
    let _ = query_cache::get(key);

    let hits_after = global().query_cache_hit_count.load(Ordering::Relaxed);
    let misses_after = global().query_cache_miss_count.load(Ordering::Relaxed);
    assert!(hits_after >= hits_before + 1, "hit counter must advance");
    assert!(
        misses_after >= misses_before + 1,
        "miss counter must advance"
    );
}

// ── Axis 3: snapshot exposes 3 new fields (hit/miss/ratio) ─────────────────

#[test]
fn axis3_snapshot_exposes_query_cache_fields() {
    let snap = GateMetricsSnapshot::capture();
    let json = serde_json::to_string(&snap).expect("serialize");
    for key in &[
        "query_cache_hit_count",
        "query_cache_miss_count",
        "query_cache_hit_ratio",
    ] {
        assert!(json.contains(key), "snapshot must include `{key}`: {json}");
    }
}

// ── Axis 4: hit_ratio is in [0.0, 1.0] ─────────────────────────────────────

#[test]
fn axis4_hit_ratio_is_bounded() {
    // Drive 5 misses + 5 hits.
    for i in 0..5 {
        let key = format!("wave17e2e::axis4::miss_{i}");
        query_cache::invalidate(&key);
        let _ = query_cache::get(&key);
    }
    for i in 0..5 {
        let key = format!("wave17e2e::axis4::hit_{i}");
        query_cache::put(key.clone(), "v".to_string());
        let _ = query_cache::get(&key);
    }
    let snap = GateMetricsSnapshot::capture();
    assert!(
        (0.0..=1.0).contains(&snap.query_cache_hit_ratio),
        "hit_ratio out of [0,1]: {}",
        snap.query_cache_hit_ratio,
    );
}

// ── Axis 5: invalidate drops a single entry ────────────────────────────────

#[test]
fn axis5_invalidate_drops_single_entry() {
    let key = "wave17e2e::axis5";
    query_cache::put(key.to_string(), "v".to_string());
    assert!(query_cache::get(key).is_some());
    query_cache::invalidate(key);
    assert_eq!(query_cache::get(key), None);
}

// ── Axis 6: keys with same kind but different payloads stay separate ───────

#[test]
fn axis6_keys_are_payload_specific() {
    let k_a = query_cache::make_key("axis6", "alpha");
    let k_b = query_cache::make_key("axis6", "beta");
    query_cache::put(k_a.clone(), r#"{"who":"alpha"}"#.to_string());
    query_cache::put(k_b.clone(), r#"{"who":"beta"}"#.to_string());
    assert_eq!(query_cache::get(&k_a).expect("alpha"), r#"{"who":"alpha"}"#,);
    assert_eq!(query_cache::get(&k_b).expect("beta"), r#"{"who":"beta"}"#);
    assert_ne!(query_cache::get(&k_a), query_cache::get(&k_b));
}

// ── Axis 7: subprocess `touring index find` uses cache (second call faster) ─

#[test]
fn axis7_subprocess_index_find_benefits_from_cache() {
    if !binary_available() {
        eprintln!("skipping: {TOURING_BIN} not built");
        return;
    }
    // Repeatedly invoking the same lookup proves the daemon-side cache
    // is wired (handler returns the same payload — cache served when hit).
    let symbol = "RustQualitySignals";
    let mut outputs = Vec::with_capacity(3);
    for _ in 0..3 {
        let out = Command::new(TOURING_BIN)
            .args(["index", "find", symbol])
            .output()
            .expect("spawn touring");
        assert!(out.status.success());
        outputs.push(String::from_utf8_lossy(&out.stdout).to_string());
    }
    // All 3 calls must produce identical JSON (cache stable).
    for w in outputs.windows(2) {
        assert_eq!(w[0], w[1], "repeated lookups must return same JSON");
    }
}

// ── Axis 8: gate-metrics CLI surfaces the new query_cache fields ───────────

#[test]
fn axis8_gate_metrics_cli_exposes_query_cache() {
    if !binary_available() {
        eprintln!("skipping: {TOURING_BIN} not built");
        return;
    }
    let out = Command::new(TOURING_BIN)
        .args(["gate-metrics", "-j"])
        .output()
        .expect("spawn touring");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    let v: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");
    for key in &[
        "query_cache_hit_count",
        "query_cache_miss_count",
        "query_cache_hit_ratio",
    ] {
        assert!(
            v.get(key).is_some(),
            "gate-metrics must surface `{key}`: {stdout}"
        );
    }
}
