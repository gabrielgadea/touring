//! P7.1 — CEG gate-metrics recording for the Code Execution Gateway.
//!
//! This module is the **single call-site** for all CEG-specific
//! `gate_metrics` counter increments.  Each public function wraps one
//! counter from [`touring_hooks_shared::gate_metrics`] and documents *which*
//! pipeline stage drives it.
//!
//! # Design
//!
//! Functions here are thin `#[inline]` wrappers — zero business logic.
//! Their only job is to provide a discoverable, namespaced API so callers
//! in `pre_exec.rs`, `fast_path.rs`, `static_stage.rs`, and `learn.rs` do
//! not need to import the raw `gate_metrics` module directly.
//!
//! # Fail-open invariant
//!
//! All counter increments are `Ordering::Relaxed` — they cannot fail and
//! never allocate.  The functions are always safe to call regardless of
//! gateway state.

use touring_hooks_shared::gate_metrics;

// ── X0 CAPTURE ───────────────────────────────────────────────────────────────

/// Increment `ceg_captured_count`.
///
/// Called at X0 CAPTURE when a tool call first enters the CEG pipeline.
/// Tracks total gateway entries; acts as the denominator for block-rate
/// and fast-path-rate ratios.
#[inline]
pub fn record_ceg_captured() {
    gate_metrics::record_ceg_captured();
}

// ── X7 DECISION ──────────────────────────────────────────────────────────────

/// Increment `ceg_blocked_count`.
///
/// Called at X7 DECISION when the gateway verdict is [`Verdict::Deny`].
/// A rising `ceg_blocked / ceg_captured` ratio signals tightening static
/// rules or degraded code quality in the session.
#[inline]
pub fn record_ceg_blocked() {
    gate_metrics::record_ceg_blocked();
}

// ── X5 SANDBOX ───────────────────────────────────────────────────────────────

/// Increment `ceg_sandboxed_count`.
///
/// Called at X5 SANDBOX when the dry-run path is taken (i.e. when
/// `sandbox_runner` is NOT the `deferred_dry_run` no-op).  High values
/// indicate active sandboxing; low values indicate deferred / fast-path.
#[inline]
pub fn record_ceg_sandboxed() {
    gate_metrics::record_ceg_sandboxed();
}

// ── X6 CAPABILITY-GATE ───────────────────────────────────────────────────────

/// Increment `ceg_fast_path_count`.
///
/// Called at X6 CAPABILITY-GATE (or earlier via `is_provably_pure`) when
/// the call is classified as provably-pure and skips sandboxing.
/// `ceg_fast_path + ceg_sandboxed` should approximately equal
/// `ceg_captured` (minus early X1/X2 denials).
#[inline]
pub fn record_ceg_fast_path() {
    gate_metrics::record_ceg_fast_path();
}

// ── X2 STATIC / X3 VGP ───────────────────────────────────────────────────────

/// Increment `workflow_antipattern_detected_count`.
///
/// Called by X2 STATIC or X3 VGP when an anti-pattern is detected at any
/// severity (Error *or* Warning).  Drives the anti-pattern prevalence
/// metric in `touring gate-metrics -j`.
#[inline]
pub fn record_workflow_antipattern_detected() {
    gate_metrics::record_workflow_antipattern_detected();
}

// ── X9 LEARN ─────────────────────────────────────────────────────────────────

/// Increment `workflow_advice_emitted_count`.
///
/// Called by the X9 LEARN path (`emit_gate_reward`) when the verdict is
/// [`Verdict::Warn`] and an advisory message was surfaced to the caller.
/// Tracks how frequently the gateway nudges rather than blocks.
#[inline]
pub fn record_workflow_advice_emitted() {
    gate_metrics::record_workflow_advice_emitted();
}

/// Increment `antipattern_converted_count`.
///
/// Called when a previously-blocked pattern was subsequently allowed —
/// i.e. after the caller applied the canonical fix.  Tracks the gateway's
/// effectiveness as a teaching signal: high values mean the RL loop is
/// producing actionable fixes.
#[inline]
pub fn record_antipattern_converted() {
    gate_metrics::record_antipattern_converted();
}

// ── Verdict dispatch helper (Wave 6 — observability boundary fix) ───────────

use super::decision::Verdict;

/// Mirror an X7 [`Verdict`] to its corresponding counter.
///
/// Both `pre_exec::gate_hook_input` (the daemon hook path) and
/// `cli/exec.rs::run` (the CLI path) need to increment the same counter
/// per terminal verdict. Centralising the dispatch here keeps the two
/// call-sites in lock-step and ensures every `touring exec` invocation
/// also feeds the production counters.
///
/// - [`Verdict::Allow`] → `ceg_fast_path_count`
/// - [`Verdict::Warn`]  → `workflow_advice_emitted_count`
/// - [`Verdict::Deny`]  → `ceg_blocked_count`
///
/// **Note**: this only mirrors the process-local atomic. To surface CLI
/// counters in the daemon's `touring gate-metrics -j` snapshot, callers
/// in CLI binaries should also issue an `emit_gate_event` IPC call.
#[inline]
pub fn record_verdict_counters(verdict: Verdict) {
    match verdict {
        Verdict::Allow => record_ceg_fast_path(),
        Verdict::Warn => record_workflow_advice_emitted(),
        Verdict::Deny => record_ceg_blocked(),
    }
}

// ── TR-2 — STR enrichment counter ────────────────────────────────────────────

/// Record a non-empty enrichment context emission.
///
/// Thin wrapper over [`gate_metrics::record_enrichment_emitted`].  Call this
/// from any hook path that assembles and emits a non-empty `additionalContext`
/// string (cli_suggester, pre_grep, etc.).
///
/// `bytes` should be `context.len()` — UTF-8 byte length of the assembled
/// context string, used as a token-count proxy for the STR observable.
///
/// See [`gate_metrics::record_enrichment_emitted`] for full STR documentation.
#[inline]
pub fn record_enrichment_emitted(bytes: usize) {
    gate_metrics::record_enrichment_emitted(bytes);
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::Ordering;
    use touring_hooks_shared::gate_metrics::{GateMetricsSnapshot, global};

    // ── P7.1 unit tests (9 total) ─────────────────────────────────────────────

    #[test]
    fn ceg_captured_increments_global_counter() {
        let before = global().ceg_captured_count.load(Ordering::Relaxed);
        record_ceg_captured();
        let after = global().ceg_captured_count.load(Ordering::Relaxed);
        assert_eq!(
            after - before,
            1,
            "ceg_captured_count should increment by 1"
        );
    }

    #[test]
    fn ceg_blocked_increments_global_counter() {
        let before = global().ceg_blocked_count.load(Ordering::Relaxed);
        record_ceg_blocked();
        let after = global().ceg_blocked_count.load(Ordering::Relaxed);
        assert_eq!(after - before, 1, "ceg_blocked_count should increment by 1");
    }

    #[test]
    fn ceg_sandboxed_increments_global_counter() {
        let before = global().ceg_sandboxed_count.load(Ordering::Relaxed);
        record_ceg_sandboxed();
        let after = global().ceg_sandboxed_count.load(Ordering::Relaxed);
        assert_eq!(
            after - before,
            1,
            "ceg_sandboxed_count should increment by 1"
        );
    }

    #[test]
    fn ceg_fast_path_increments_global_counter() {
        let before = global().ceg_fast_path_count.load(Ordering::Relaxed);
        record_ceg_fast_path();
        let after = global().ceg_fast_path_count.load(Ordering::Relaxed);
        assert_eq!(
            after - before,
            1,
            "ceg_fast_path_count should increment by 1"
        );
    }

    #[test]
    fn workflow_antipattern_detected_increments_global_counter() {
        let before = global()
            .workflow_antipattern_detected_count
            .load(Ordering::Relaxed);
        record_workflow_antipattern_detected();
        let after = global()
            .workflow_antipattern_detected_count
            .load(Ordering::Relaxed);
        assert_eq!(
            after - before,
            1,
            "workflow_antipattern_detected_count should increment by 1"
        );
    }

    #[test]
    fn workflow_advice_emitted_increments_global_counter() {
        let before = global()
            .workflow_advice_emitted_count
            .load(Ordering::Relaxed);
        record_workflow_advice_emitted();
        let after = global()
            .workflow_advice_emitted_count
            .load(Ordering::Relaxed);
        assert_eq!(
            after - before,
            1,
            "workflow_advice_emitted_count should increment by 1"
        );
    }

    #[test]
    fn antipattern_converted_increments_global_counter() {
        let before = global().antipattern_converted_count.load(Ordering::Relaxed);
        record_antipattern_converted();
        let after = global().antipattern_converted_count.load(Ordering::Relaxed);
        assert_eq!(
            after - before,
            1,
            "antipattern_converted_count should increment by 1"
        );
    }

    #[test]
    fn multiple_increments_are_additive() {
        let before = global().ceg_captured_count.load(Ordering::Relaxed);
        record_ceg_captured();
        record_ceg_captured();
        record_ceg_captured();
        let after = global().ceg_captured_count.load(Ordering::Relaxed);
        assert_eq!(after - before, 3, "three increments should add 3");
    }

    #[test]
    fn snapshot_contains_ceg_fields_and_serializes() {
        // Exercise every counter once so the snapshot is non-trivially populated.
        record_ceg_captured();
        record_ceg_blocked();
        record_ceg_sandboxed();
        record_ceg_fast_path();
        record_workflow_antipattern_detected();
        record_workflow_advice_emitted();
        record_antipattern_converted();

        let snap = GateMetricsSnapshot::capture();

        // All 7 CEG fields must be present in the snapshot.
        assert!(
            snap.ceg_captured_count > 0,
            "snapshot.ceg_captured_count should be > 0"
        );
        assert!(
            snap.ceg_blocked_count > 0,
            "snapshot.ceg_blocked_count should be > 0"
        );
        assert!(
            snap.ceg_sandboxed_count > 0,
            "snapshot.ceg_sandboxed_count should be > 0"
        );
        assert!(
            snap.ceg_fast_path_count > 0,
            "snapshot.ceg_fast_path_count should be > 0"
        );
        assert!(
            snap.workflow_antipattern_detected_count > 0,
            "snapshot.workflow_antipattern_detected_count should be > 0"
        );
        assert!(
            snap.workflow_advice_emitted_count > 0,
            "snapshot.workflow_advice_emitted_count should be > 0"
        );
        assert!(
            snap.antipattern_converted_count > 0,
            "snapshot.antipattern_converted_count should be > 0"
        );

        // Snapshot must serialize cleanly (verifies serde annotation coverage).
        let json = serde_json::to_string(&snap).expect("GateMetricsSnapshot must serialize");
        assert!(
            json.contains("ceg_captured_count"),
            "JSON must include ceg_captured_count"
        );
        assert!(
            json.contains("ceg_blocked_count"),
            "JSON must include ceg_blocked_count"
        );
        assert!(
            json.contains("workflow_antipattern_detected_count"),
            "JSON must include workflow_antipattern_detected_count"
        );
        assert!(
            json.contains("antipattern_converted_count"),
            "JSON must include antipattern_converted_count"
        );
    }

    /// Wave 7.A (2026-05-23) — `observe()` must drive the gateway pipeline
    /// purely for counter side-effects and return the outcome without
    /// requiring a `HookRuntime`. Verifies the helper that backs the native
    /// `pre-bash` CEG observability wire.
    #[test]
    fn observe_increments_captured_and_returns_outcome() {
        use super::super::pre_exec::observe;
        let before = global().ceg_captured_count.load(Ordering::Relaxed);
        let outcome = observe("Bash", "echo hello");
        let after = global().ceg_captured_count.load(Ordering::Relaxed);
        assert_eq!(after - before, 1, "observe must increment ceg_captured");
        assert!(
            outcome.is_some(),
            "observe must return Some(GatewayOutcome) for code-bearing payload"
        );
    }

    /// `observe()` must fail-open on a non-code-bearing tool — returns `None`
    /// and does NOT increment any counter.
    #[test]
    fn observe_returns_none_and_no_counter_for_non_code_bearing_tool() {
        use super::super::pre_exec::observe;
        let before = global().ceg_captured_count.load(Ordering::Relaxed);
        let outcome = observe("Read", "/etc/hosts");
        let after = global().ceg_captured_count.load(Ordering::Relaxed);
        assert_eq!(after, before, "non-code-bearing tool must not increment");
        assert!(
            outcome.is_none(),
            "observe must return None for non-code-bearing"
        );
    }

    /// Wave 6 (2026-05-23) — `record_verdict_counters` must route each
    /// terminal verdict to exactly one counter. Verifies the dispatch table
    /// the CEG observability boundary fix depends on.
    #[test]
    fn record_verdict_counters_dispatches_each_arm() {
        use super::super::decision::Verdict;
        // Allow → fast_path
        let before = global().ceg_fast_path_count.load(Ordering::Relaxed);
        record_verdict_counters(Verdict::Allow);
        let after = global().ceg_fast_path_count.load(Ordering::Relaxed);
        assert_eq!(
            after - before,
            1,
            "Allow must increment ceg_fast_path_count"
        );

        // Warn → workflow_advice
        let before = global()
            .workflow_advice_emitted_count
            .load(Ordering::Relaxed);
        record_verdict_counters(Verdict::Warn);
        let after = global()
            .workflow_advice_emitted_count
            .load(Ordering::Relaxed);
        assert_eq!(
            after - before,
            1,
            "Warn must increment workflow_advice_emitted_count"
        );

        // Deny → blocked
        let before = global().ceg_blocked_count.load(Ordering::Relaxed);
        record_verdict_counters(Verdict::Deny);
        let after = global().ceg_blocked_count.load(Ordering::Relaxed);
        assert_eq!(after - before, 1, "Deny must increment ceg_blocked_count");
    }
}
