//! Integration test harness demonstrating the `#[traced_test]` pattern.
//!
//! Tier A (2026-04-19): Production code across `touring-hooks` (138 hook
//! registry entries) emits `tracing::info!` / `tracing::debug!` events at
//! decision points — concept drift detection, BLAKE3 cache hits, tantivy
//! upsert failures. Asserting against those events inside unit tests used
//! to require a bespoke `tracing_subscriber` harness per test file.
//!
//! The `tracing-test` crate (v0.2) supplies an attribute macro
//! `#[traced_test]` that installs a capturing subscriber scoped to the
//! test function, exposing a `logs_contain(substring) -> bool` helper.
//! Pattern for hook authors:
//!
//! ```ignore
//! #[traced_test]
//! #[test]
//! fn post_edit_reports_concept_drift_when_detected() {
//!     run_post_edit_with_drift_fixture();
//!     assert!(logs_contain("concept drift detected"));
//! }
//! ```
//!
//! This file provides two demonstration tests against a local helper that
//! mirrors the shape of `post_edit.rs:444`:
//!
//! ```ignore
//! tracing::info!("post_edit: concept drift detected ks={:.3}", ks);
//! ```
//!
//! The tests are self-contained (no daemon, no DB, no fixtures) so they
//! serve purely as a living template — copy, adapt, and point at real
//! instrumented code paths.
//!
//! See also: `crates/touring-hooks/src/test_util.rs` for the
//! `pretty_assertions` shadow-macro prelude that the lib tests use.

use tracing_test::traced_test;

/// Mirrors the shape of `post_edit.rs:444` — emits an `info` event whose
/// message and structured field carry the KS statistic of a concept-drift
/// report. Real call-sites elsewhere in `touring-hooks` follow the same
/// pattern, so tests written against this helper transfer verbatim.
fn emit_concept_drift_event(ks: f64) {
    tracing::info!(
        ks = %format!("{:.3}", ks),
        "post_edit: concept drift detected"
    );
}

/// Mirrors `post_edit.rs:509` — emits a `debug` event when the BLAKE3
/// content hash has not changed since the last indexing pass.
fn emit_blake3_unchanged_event(rel_path: &str) {
    tracing::debug!(rel_path, "post_edit: BLAKE3 unchanged — skipping reindex");
}

#[traced_test]
#[test]
fn concept_drift_event_message_and_field_are_captured() {
    emit_concept_drift_event(0.731);
    // Message substring is present.
    assert!(logs_contain("concept drift detected"));
    // Formatted numeric field is present (precision-sensitive).
    assert!(logs_contain("0.731"));
}

#[traced_test]
#[test]
fn unrelated_event_does_not_produce_false_positive() {
    // A distinct event must not satisfy a matcher targeting the
    // concept-drift message — guards against over-broad substring matches.
    emit_blake3_unchanged_event("src/foo.rs");
    assert!(!logs_contain("concept drift detected"));
    assert!(logs_contain("BLAKE3 unchanged"));
    assert!(logs_contain("src/foo.rs"));
}

#[traced_test]
#[test]
fn multiple_events_accumulate_in_capture_buffer() {
    // Each test gets a fresh capturing subscriber, so within a single
    // test every emitted event is observable regardless of ordering.
    emit_concept_drift_event(0.42);
    emit_blake3_unchanged_event("src/bar.rs");
    emit_concept_drift_event(0.88);

    assert!(logs_contain("0.420"));
    assert!(logs_contain("0.880"));
    assert!(logs_contain("src/bar.rs"));
}
