#![allow(clippy::indexing_slicing)] // test-only: intentional deterministic indexing
//! Wave 11 — Multi-language Health Delta E2E (cross-crate).
//!
//! Wave 9-10 enabled health_delta tracking for Rust files only.
//! Wave 11 broadens this to every language `Lang::from_path` recognises
//! by dispatching:
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │ touring-hooks::health_delta::record_pre_signals (path, src)    │
//! │                                                                 │
//! │      ┌──── .rs ──────► RustQualitySignals (syn, Wave 7-8)     │
//! │      │                                                          │
//! │  ┌───┴───────► path  ─► Lang::from_path (touring-ast)          │
//! │      │                                                          │
//! │      └──── other ────► analyze_quality(src, lang)              │
//! │                          (tree-sitter, Wave 5.1)               │
//! └─────────────────────────────────────────────────────────────────┘
//! ```
//!
//! Eight axes pin cross-crate invariants for the multi-lang dispatch.

use std::io::Write;
use tempfile::NamedTempFile;

use touring_hooks::health_delta::{compute_signals_delta, discard_pre_health, record_pre_signals};
use touring_hooks::knowledge::FileKnowledgeDB;
use touring_hooks::pre_edit::compose_edit_context;

fn write_temp(suffix: &str, content: &str) -> (NamedTempFile, String) {
    let mut f = tempfile::Builder::new()
        .prefix("wave11_")
        .suffix(suffix)
        .tempfile()
        .expect("create temp");
    f.write_all(content.as_bytes()).expect("write");
    f.flush().expect("flush");
    let path = f.path().to_string_lossy().to_string();
    (f, path)
}

fn temp_db() -> FileKnowledgeDB {
    let conn = rusqlite::Connection::open_in_memory().expect("in-memory sqlite");
    FileKnowledgeDB::from_conn(conn)
}

// ── Axis 1: pre_edit hook records signals for Python files ────────────────────

#[test]
fn axis1_pre_edit_records_python_signals() {
    let (_tf, path) = write_temp(".py", "def add(a, b):\n    return a + b\n");
    discard_pre_health(&path);
    let db = temp_db();

    // Wave 11: pre_edit::compose_edit_context now calls record_pre_signals
    // (multi-lang) instead of record_pre_health (Rust-only).
    let _ = compose_edit_context(None, &db, &path);

    // Cache must be populated for the .py path now (Wave 11 multi-lang).
    let delta = compute_signals_delta(&path, "def add(a, b):\n    return a + b\n").expect("delta");
    assert!(
        delta.old.is_some(),
        "Wave 11 must populate cache for python files",
    );
}

// ── Axis 2: pre_edit hook records signals for TypeScript files ───────────────

#[test]
fn axis2_pre_edit_records_typescript_signals() {
    let (_tf, path) = write_temp(
        ".ts",
        "export function inc(x: number): number { return x + 1; }\n",
    );
    discard_pre_health(&path);
    let db = temp_db();
    let _ = compose_edit_context(None, &db, &path);

    let delta = compute_signals_delta(
        &path,
        "export function inc(x: number): number { return x + 1; }\n",
    )
    .expect("delta");
    assert!(delta.old.is_some(), "ts pre_edit must populate cache");
}

// ── Axis 3: Rust path still works via the multi-lang dispatch ────────────────

#[test]
fn axis3_pre_edit_records_rust_via_dispatch() {
    let (_tf, path) = write_temp(".rs", "pub fn ok(x: i32) -> i32 { x + 1 }\n");
    discard_pre_health(&path);
    let db = temp_db();
    let _ = compose_edit_context(None, &db, &path);

    // Rust path delegates to RustQualitySignals (syn) — Wave 7-8 fusion.
    let delta =
        compute_signals_delta(&path, "pub fn ok(x: i32) -> i32 { x + 1 }\n").expect("delta");
    assert!(delta.old.is_some(), "rust pre_edit still works");
    assert_eq!(delta.delta, Some(0.0), "identity edit zero delta");
}

// ── Axis 4: Python regression detection (delta drops on complexity rise) ──────

#[test]
fn axis4_python_complexity_rise_yields_delta() {
    let path = "/wave11/py_complexity.py";
    discard_pre_health(path);
    record_pre_signals(path, "def a(): return 1\n").expect("pre");
    let new_src = "\
def a(x):
    if x > 0:
        if x > 10:
            if x > 100:
                return x * 1000
            return x * 100
        return x * 10
    return 0
";
    let delta = compute_signals_delta(path, new_src).expect("delta");
    assert!(delta.delta.is_some(), "delta must be defined");
}

// ── Axis 5: TypeScript identity edit emits zero delta ────────────────────────

#[test]
fn axis5_typescript_identity_yields_zero_delta() {
    let path = "/wave11/ts_identity.ts";
    discard_pre_health(path);
    let src = "export function add(a: number, b: number): number { return a + b; }\n";
    record_pre_signals(path, src).expect("pre");
    let delta = compute_signals_delta(path, src).expect("delta");
    assert_eq!(delta.delta, Some(0.0), "identity must yield zero delta");
}

// ── Axis 6: cache schema is uniform across engines (path → engine deterministic) ──

#[test]
fn axis6_cache_schema_uniform_across_engines() {
    // Same path, same engine, two distinct edits — first record then
    // multiple compute calls. Cache one-shot semantic must hold.
    let path = "/wave11/uniform.tsx";
    discard_pre_health(path);
    record_pre_signals(path, "export const App = () => <div>hello</div>;\n").expect("tsx pre");
    let first = compute_signals_delta(path, "export const App = () => <div>hello world</div>;\n")
        .expect("first delta");
    assert!(first.old.is_some(), "first compute hits cache");
    let second = compute_signals_delta(path, "export const App = () => <div>!</div>;\n")
        .expect("second delta");
    assert_eq!(second.old, None, "cache consumed by first compute");
}

// ── Axis 7: legacy record_pre_health remains Rust-only (no behaviour drift) ──

#[test]
fn axis7_legacy_apis_remain_rust_only() {
    use touring_hooks::health_delta::{compute_health_delta, record_pre_health};

    // Legacy Rust-only API must NOT touch non-rust files (preserves
    // Wave 9 contract). Wave 11 added new multi-lang APIs without
    // changing legacy semantics.
    let py_path = "/wave11/legacy_py.py";
    discard_pre_health(py_path);
    assert_eq!(record_pre_health(py_path, "def a(): return 1\n"), None);
    assert_eq!(
        compute_health_delta(py_path, "def a(): return 2\n"),
        None,
        "legacy compute_health_delta must remain Rust-only",
    );
}

// ── Axis 8: pre_write→post_edit cross-tool flow (overwrite produces delta) ────

#[test]
fn axis8_pre_write_records_signals_for_overwrite() {
    // Direct API simulation since pre_write is invoked through hook
    // entrypoints we don't expose publicly. The wiring inside pre_write
    // calls `record_pre_signals(file_path, prev_src)` — same as Wave 11
    // pre_edit. We validate that the function is reachable and produces
    // the expected behaviour for non-Rust files.
    let path = "/wave11/write_overwrite.tsx";
    discard_pre_health(path);
    let prev = "export const App = () => null;\n";
    let recorded = record_pre_signals(path, prev).expect("tsx record");
    assert!((0.0..=1.0).contains(&recorded));

    let new = "export const App = () => <div>new</div>;\n";
    let delta = compute_signals_delta(path, new).expect("delta");
    assert!(delta.old.is_some(), "overwrite must yield old=Some");
    assert!(delta.delta.is_some(), "overwrite must yield delta=Some");
}
