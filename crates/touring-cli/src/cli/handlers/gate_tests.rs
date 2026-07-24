//! `cli_gate_event` regression tests (Master Plan A.W2.P5 extraction).
//!
//! Included from `cli_handlers.rs` via `#[path]` so `super::*` still resolves
//! to the `cli_handlers` facade (zero changes to test bodies).
//
// ── Wave 6 (2026-05-23) — cli_gate_event regression tests ────────────────────
//
// The CEG observability boundary fix depends on this handler routing each
// event-name string to the matching `record_*` counter. A typo in the match
// arms here would silently break daemon observability — these tests are the
// regression guard.
use super::*;
use crate::shared::gate_metrics::global;
use std::sync::atomic::Ordering;

fn mock_runtime() -> HookRuntime {
    let tmp = tempfile::tempdir().expect("tempdir");
    HookRuntime::new(tmp.path()).expect("HookRuntime::new")
}

#[test]
fn cli_gate_event_increments_captured_counter() {
    let mut rt = mock_runtime();
    let before = global().ceg_captured_count.load(Ordering::Relaxed);
    let out = cli_gate_event(&mut rt, &serde_json::json!({"events": ["captured"]}));
    let after = global().ceg_captured_count.load(Ordering::Relaxed);
    // `ceg_captured_count` is a process-global counter. Under `cargo test --workspace`
    // the CEG real path (X0 capture in `touring run`/ctx_execute tests) and the sibling
    // `records_each_known_event_kind` test increment it concurrently in the same process,
    // so an exact `== 1` delta is racy (was flaky under high parallelism). The routing
    // guarantee (captured → record_ceg_captured, no typo) is asserted as "advanced by
    // ≥ 1"; the EXACT count THIS handler recorded is asserted deterministically via the
    // response payload (`recorded:1`), which the handler computes alone.
    assert!(
        after >= before + 1,
        "captured event must advance the global counter (before={before}, after={after})"
    );
    assert!(
        out.contains("\"recorded\":1"),
        "response must report exactly 1 recorded; got {out}"
    );
}

#[test]
fn cli_gate_event_records_each_known_event_kind() {
    let mut rt = mock_runtime();
    // Wave 7 cross-audit (2026-05-23): 9 kinds — added tee_persisted +
    // timeout_fallback to match the docstring contract (REGRA #0 fix).
    let payload = serde_json::json!({
        "events": [
            "captured", "fast_path", "sandboxed", "blocked",
            "workflow_advice", "antipattern", "antipattern_converted",
            "tee_persisted", "timeout_fallback"
        ]
    });
    let out = cli_gate_event(&mut rt, &payload);
    assert!(
        out.contains("\"recorded\":9"),
        "all 9 known kinds must record; got {out}"
    );
    assert!(
        out.contains("\"skipped\":0"),
        "no kind should be skipped; got {out}"
    );
}

#[test]
fn cli_gate_event_skips_unknown_event_names() {
    let mut rt = mock_runtime();
    let out = cli_gate_event(
        &mut rt,
        &serde_json::json!({"events": ["captured", "totally_made_up", "fast_path"]}),
    );
    assert!(
        out.contains("\"recorded\":2"),
        "2 valid events must record; got {out}"
    );
    assert!(
        out.contains("\"skipped\":1"),
        "1 unknown must skip; got {out}"
    );
}

#[test]
fn cli_gate_event_handles_missing_events_field_gracefully() {
    let mut rt = mock_runtime();
    let out = cli_gate_event(&mut rt, &serde_json::json!({}));
    assert!(
        out.contains("\"recorded\":0"),
        "no events → 0 recorded; got {out}"
    );
    assert!(
        out.contains("\"skipped\":0"),
        "no events → 0 skipped; got {out}"
    );
}

#[test]
fn cli_gate_event_handles_non_string_event_entries() {
    let mut rt = mock_runtime();
    let out = cli_gate_event(
        &mut rt,
        &serde_json::json!({"events": ["captured", 42, null]}),
    );
    assert!(out.contains("\"recorded\":1"), "1 string valid; got {out}");
    assert!(
        out.contains("\"skipped\":2"),
        "2 non-strings skipped; got {out}"
    );
}
