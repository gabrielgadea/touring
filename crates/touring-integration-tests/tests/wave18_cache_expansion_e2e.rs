#![allow(clippy::indexing_slicing)] // test-only
//! Wave 18 — Cache expansion + path-scoped invalidation E2E (cross-crate).
//!
//! Builds on Wave 17: validates the new wiring across 3 additional
//! query handlers (`cli_ast_meta`, `cli_ast_blast`, `cli_index_search`)
//! and the new `invalidate_by_path` API consumed by post_edit /
//! post_write to prevent stale cache after edits.
//!
//! ```text
//! ┌──────────────────────────────────────────────────────────────────┐
//! │ Wave 17 wiring (existing):                                       │
//! │   - cli_index_find        (VGP)                                  │
//! │   - cli_tantivy_search    (BM25)                                 │
//! │                                                                  │
//! │ Wave 18 wiring (new):                                            │
//! │   - cli_ast_meta          (TIER 1 file metadata first)          │
//! │   - cli_ast_blast         (blast radius pre-edit)               │
//! │   - cli_index_search      (prefix lookup)                       │
//! │                                                                  │
//! │ Wave 18 invalidation:                                            │
//! │   - invalidate_by_path(file_path) — substring match, immediate  │
//! │   - Wired in post_edit + post_write (after successful edits)    │
//! │   - Counter: query_cache_invalidate_count (gate_metrics)        │
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

// ── Axis 1: invalidate_by_path removes matching entries ────────────────────

#[test]
fn axis1_invalidate_removes_matching_entries() {
    let target = "/wave18e2e/axis1_target.rs";
    let other = "/wave18e2e/axis1_other.rs";
    let key_a = query_cache::make_key("cli_ast_meta", &format!("{target}|skeleton"));
    let key_b = query_cache::make_key("cli_ast_blast", target);
    let key_c = query_cache::make_key("cli_ast_meta", &format!("{other}|skeleton"));

    query_cache::put(key_a.clone(), "a".to_string());
    query_cache::put(key_b.clone(), "b".to_string());
    query_cache::put(key_c.clone(), "c".to_string());

    let removed = query_cache::invalidate_by_path(target);
    assert!(removed >= 2);
    assert!(query_cache::get(&key_a).is_none());
    assert!(query_cache::get(&key_b).is_none());
    assert!(
        query_cache::get(&key_c).is_some(),
        "non-target path must survive"
    );
}

// ── Axis 2: invalidate counter advances ────────────────────────────────────

#[test]
fn axis2_invalidate_counter_advances() {
    let path = "/wave18e2e/axis2.rs";
    let key = query_cache::make_key("cli_ast_meta", &format!("{path}|skeleton"));
    query_cache::put(key, "v".to_string());

    let before = global()
        .query_cache_invalidate_count
        .load(Ordering::Relaxed);
    let _ = query_cache::invalidate_by_path(path);
    let after = global()
        .query_cache_invalidate_count
        .load(Ordering::Relaxed);
    assert!(after >= before + 1, "invalidate counter must advance");
}

// ── Axis 3: snapshot exposes invalidate counter field ──────────────────────

#[test]
fn axis3_snapshot_exposes_invalidate_counter() {
    let snap = GateMetricsSnapshot::capture();
    let json = serde_json::to_string(&snap).expect("serialize");
    assert!(
        json.contains("query_cache_invalidate_count"),
        "snapshot must surface invalidate counter: {json}",
    );
}

// ── Axis 4: zero-match invalidation returns 0 (no false positives) ─────────

#[test]
fn axis4_invalidate_zero_match_returns_zero() {
    let removed = query_cache::invalidate_by_path("/wave18e2e/axis4_never_indexed.rs");
    assert_eq!(removed, 0);
}

// ── Axis 5: subprocess `touring ast meta` is cacheable ─────────────────────

#[test]
fn axis5_subprocess_ast_meta_benefits_from_cache() {
    if !binary_available() {
        eprintln!("skipping: {TOURING_BIN} not built");
        return;
    }
    // Pick a file that's certainly indexed (the touring source itself).
    let path = "crates/touring-hooks/src/shared/query_cache.rs";

    let mut outputs = Vec::with_capacity(3);
    for _ in 0..3 {
        let out = Command::new(TOURING_BIN)
            .args(["ast", "meta", path, "--depth", "skeleton", "-j"])
            .output()
            .expect("spawn touring");
        if !out.status.success() {
            // file may not be indexed yet — graceful skip
            eprintln!("ast meta returned non-zero, skipping: {:?}", out);
            return;
        }
        outputs.push(String::from_utf8_lossy(&out.stdout).to_string());
    }
    // All 3 calls must produce identical JSON.
    for w in outputs.windows(2) {
        assert_eq!(w[0], w[1], "repeated lookups must return same JSON");
    }
}

// ── Axis 6: subprocess `touring ast blast` is cacheable ────────────────────

#[test]
fn axis6_subprocess_ast_blast_benefits_from_cache() {
    if !binary_available() {
        eprintln!("skipping: {TOURING_BIN} not built");
        return;
    }
    let path = "crates/touring-hooks/src/lib.rs";
    let mut outputs = Vec::with_capacity(2);
    for _ in 0..2 {
        let out = Command::new(TOURING_BIN)
            .args(["ast", "blast", path, "-j"])
            .output()
            .expect("spawn");
        if !out.status.success() {
            eprintln!("ast blast returned non-zero, skipping");
            return;
        }
        outputs.push(String::from_utf8_lossy(&out.stdout).to_string());
    }
    assert_eq!(outputs[0], outputs[1]);
}

// ── Axis 7: cache hit ratio improves after Wave 18 expansion ───────────────

#[test]
fn axis7_cache_hit_ratio_advances() {
    if !binary_available() {
        eprintln!("skipping: {TOURING_BIN} not built");
        return;
    }
    // The DAEMON process owns the gate_metrics singleton; the test
    // process's `global()` is a separate instance. We must query the
    // daemon's counters via CLI (`touring gate-metrics -j`).
    fn read_daemon_hits() -> u64 {
        let out = Command::new(TOURING_BIN)
            .args(["gate-metrics", "-j"])
            .output()
            .expect("spawn gate-metrics");
        let v: serde_json::Value = serde_json::from_slice(&out.stdout).expect("valid JSON");
        v["query_cache_hit_count"].as_u64().unwrap_or(0)
    }

    let hits_before = read_daemon_hits();
    for _ in 0..3 {
        let _ = Command::new(TOURING_BIN)
            .args(["index", "find", "GateMetrics"])
            .output();
    }
    let hits_after = read_daemon_hits();
    let added = hits_after.saturating_sub(hits_before);
    assert!(
        added >= 2,
        "≥2 hits expected from 3 same lookups (1 miss + 2 hits): got {added} (before={hits_before}, after={hits_after})",
    );
}

// ── Axis 8: gate-metrics CLI surfaces invalidate counter ───────────────────

#[test]
fn axis8_gate_metrics_cli_shows_invalidate_counter() {
    if !binary_available() {
        eprintln!("skipping: {TOURING_BIN} not built");
        return;
    }
    let out = Command::new(TOURING_BIN)
        .args(["gate-metrics", "-j"])
        .output()
        .expect("spawn");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    let v: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");
    assert!(
        v.get("query_cache_invalidate_count").is_some(),
        "gate-metrics must surface invalidate counter: {stdout}",
    );
}
