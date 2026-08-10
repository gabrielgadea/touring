//! A2 (2026-08-08) — the context-savings ledger, end to end.
//!
//! Proves the two properties the old `ctx_roi` could not have: bytes are the
//! EXACT delta at the site, and tokens are either a real `cl100k_base` count or
//! openly absent — never a heuristic wearing a measurement's name.
//!
//! The install path is exercised by ONE test on purpose: `set_token_counter`
//! is a process-wide `OnceLock`, so "before install" and "after install" only
//! mean anything when observed in order inside a single binary — a sibling test
//! that also installed made the pair depend on the harness's thread schedule
//! (observed on the first run). Every test here reads global counters as
//! deltas, so they are `#[serial]` too: a concurrent sibling bumping
//! `savings_event_count` between two captures is the same flake class as the
//! `query_cache` one fixed on 07/08/2026.

use touring_hooks::shared::gate_metrics as gm;

/// Raw output a `cargo test` profile actually compresses: the noise lines go,
/// the failure and the summary stay.
const RAW: &str = "running 3 tests\n\
    test alpha ... ok\n\
    test beta ... ok\n\
    test gamma ... FAILED\n\
    test result: FAILED. 2 passed; 1 failed\n";

#[test]
#[serial_test::serial]
fn the_ledger_measures_bytes_exactly_and_tokens_only_when_it_can() {
    let before = gm::GateMetricsSnapshot::capture();
    assert!(!gm::has_token_counter(), "no counter may be installed yet");

    // ── Phase 1: no tokenizer — bytes still exact, tokens untouched ──────────
    gm::record_compression_savings(RAW, "test result: FAILED. 2 passed; 1 failed\n");
    let p1 = gm::GateMetricsSnapshot::capture();
    assert_eq!(
        p1.compression_bytes_in_total - before.compression_bytes_in_total,
        RAW.len() as u64,
        "bytes in must be the real length of the real text"
    );
    assert_eq!(
        p1.compression_bytes_out_total - before.compression_bytes_out_total,
        "test result: FAILED. 2 passed; 1 failed\n".len() as u64
    );
    assert_eq!(p1.savings_event_count - before.savings_event_count, 1);
    assert_eq!(
        p1.token_measured_event_count, before.token_measured_event_count,
        "without a tokenizer the event contributes NO token data — not a zero"
    );

    // ── Phase 2: install the real cl100k counter ─────────────────────────────
    assert!(touring_hooks::token_meter::install(), "install must take");
    assert!(gm::has_token_counter());

    gm::record_compression_savings(RAW, "test result: FAILED. 2 passed; 1 failed\n");
    let p2 = gm::GateMetricsSnapshot::capture();
    assert_eq!(
        p2.token_measured_event_count - p1.token_measured_event_count,
        1,
        "with a tokenizer installed the event must carry token data"
    );
    let tokens_in = p2.measured_tokens_in_total - p1.measured_tokens_in_total;
    let tokens_out = p2.measured_tokens_out_total - p1.measured_tokens_out_total;
    assert!(tokens_in > tokens_out && tokens_out > 0);
    // The point of using a real tokenizer: it disagrees with bytes/4.
    assert_ne!(
        tokens_in,
        RAW.len() as u64 / 4,
        "if cl100k agreed with the heuristic here, the measurement would be moot"
    );

    // ── Phase 3: registration is a startup decision — first writer wins ──────
    assert!(
        !touring_hooks::token_meter::install(),
        "a later caller must be TOLD its counter did not take, never silently swapped"
    );
}

#[test]
#[serial_test::serial]
fn routing_events_contribute_bytes_and_declare_no_tokens() {
    // A routed output lives on disk; only its size is in memory. The ledger
    // must take the bytes and NOT claim token coverage for them.
    let before = gm::GateMetricsSnapshot::capture();
    gm::record_routing_savings(50_000, 420);
    let after = gm::GateMetricsSnapshot::capture();
    assert_eq!(
        after.routed_bytes_in_total - before.routed_bytes_in_total,
        50_000
    );
    assert_eq!(after.routed_bytes_out_total - before.routed_bytes_out_total, 420);
    assert_eq!(after.savings_event_count - before.savings_event_count, 1);
    assert_eq!(
        after.token_measured_event_count, before.token_measured_event_count,
        "a bytes-only event must not inflate token coverage"
    );
}
