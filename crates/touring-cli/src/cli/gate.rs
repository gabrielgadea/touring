//! CLI gate-metrics handlers (`cli_gate_*`) — extracted from cli_handlers.rs (A-W2.P3).

use crate::runtime::HookRuntime;

/// Snapshot the global `GateMetrics` and serialize as JSON.
///
/// Exposed via CLI `touring gate-metrics` for observability of the CILA-gated
/// enrichment policy. Reports fast-path vs full-enrichment counts and ratios.
pub fn cli_gate_metrics(_rt: &mut HookRuntime, _payload: &serde_json::Value) -> String {
    let snapshot = crate::shared::gate_metrics::GateMetricsSnapshot::capture();
    serde_json::to_string(&snapshot)
        .unwrap_or_else(|e| format!("{{\"error\":\"serialize failed: {e}\"}}"))
}
/// Mirror CEG counter increments from a CLI process into the daemon's
/// process-local atomics. Closes the **observability boundary** so that
/// `touring exec` invocations are visible in `touring gate-metrics -j`.
///
/// # Why this exists
///
/// `touring exec` runs the X0..X9 gateway in its own CLI process. The
/// per-stage `record_ceg_*` calls increment static atomics that live and die
/// with that process — the daemon never sees them. This handler is the IPC
/// bridge: after `run_gateway` returns in the CLI, the CLI fires a
/// `cli-gate-event {events: [...]}` IPC call and the daemon replays the
/// increments against ITS own counters, so the snapshot in
/// `touring gate-metrics` reflects production CEG activity.
///
/// # Payload
///
/// `{"events": ["captured", "fast_path", "sandboxed", "blocked",
///              "workflow_advice", "antipattern", "tee_persisted",
///              "timeout_fallback", "antipattern_converted"]}`
///
/// Each entry triggers exactly one increment on the corresponding daemon
/// counter. Unknown event names are skipped silently — fail-open IPC.
///
/// # Returns
///
/// JSON `{"recorded": N, "skipped": M}` so the CLI can log what landed.
pub fn cli_gate_event(_rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let events = payload
        .get("events")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let mut recorded = 0u64;
    let mut skipped = 0u64;
    for ev in &events {
        let Some(name) = ev.as_str() else {
            skipped += 1;
            continue;
        };
        match name {
            "captured" => crate::shared::gate_metrics::record_ceg_captured(),
            "fast_path" => crate::shared::gate_metrics::record_ceg_fast_path(),
            "sandboxed" => crate::shared::gate_metrics::record_ceg_sandboxed(),
            "blocked" => crate::shared::gate_metrics::record_ceg_blocked(),
            "workflow_advice" => crate::shared::gate_metrics::record_workflow_advice_emitted(),
            "antipattern" => crate::shared::gate_metrics::record_workflow_antipattern_detected(),
            "antipattern_converted" => crate::shared::gate_metrics::record_antipattern_converted(),
            // Wave 7 cross-audit (2026-05-23) — POTENCIALIZA (REGRA #0):
            // docstring lists these 9 kinds; the match arms were missing 2.
            // Added so CLI binaries can mirror sandbox tee persistence
            // and timeout-fallback events into the daemon snapshot too.
            "tee_persisted" => crate::shared::gate_metrics::record_sandbox_tee_persisted(),
            "timeout_fallback" => crate::shared::gate_metrics::record_sandbox_timeout_fallback(),
            _ => {
                skipped += 1;
                continue;
            }
        }
        recorded += 1;
    }
    format!("{{\"recorded\":{recorded},\"skipped\":{skipped}}}")
}
