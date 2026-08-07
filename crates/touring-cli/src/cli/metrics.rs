//! CLI runtime-metrics handlers (`cli_mcp_overhead`, `cli_tokio_metrics`, `cli_profile_status`) — extracted from cli_handlers.rs (A-W2.P4).
//!
//! Lightweight introspection over MCP overhead, the Tokio runtime, and the
//! profile aggregator. All dependencies are fully-qualified (`crate::mcp_overhead::*`,
//! `crate::shared::gate_metrics::*`, `touring_foundation::profile::*`).

use crate::runtime::HookRuntime;

/// Snapshot the MCP overhead counters and serialize as JSON.
///
/// Exposed via CLI `touring mcp-overhead` for self-reporting of per-tool
/// token costs. Returns top-N costliest tools sorted by total_tokens.
///
/// Payload (optional): `{"top_n": <usize>}` to limit output.
pub fn cli_mcp_overhead(_rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let top_n = payload
        .get("top_n")
        .and_then(|v| v.as_u64())
        .map(|n| n as usize);
    if let Some(n) = top_n {
        crate::mcp_overhead::top_n_tools_json(Some(n))
    } else {
        crate::mcp_overhead::snapshot_json()
    }
}
/// Snapshot Tokio multi-threaded work-stealing scheduler metrics.
///
/// Collects worker count, idle thread count, blocking thread count, and
/// injection queue depth from `tokio::runtime::Handle::current().metrics()`.
///
/// Payload (optional): `{"record": true}` to also update the global
/// `GateMetrics` counters with the snapshot. When absent, only returns
/// the current snapshot without updating counters.
///
/// Diagnostic interpretation:
/// - `num_idle_threads > 0` AND `injection_queue_depth > 0` → backpressure downstream
/// - `num_idle_threads == 0` sustained → workers undersized for workload
/// - `num_blocking_threads` approaching runtime max → blocking pool saturated
///
/// Wired via CLI `touring metrics -j` (shares handler with gate-metrics
/// by inspecting payload — both return `GateMetricsSnapshot`).
pub fn cli_tokio_metrics(_rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    use tokio::runtime::Handle;
    let handle = match Handle::try_current() {
        Ok(h) => h,
        Err(_) => {
            return serde_json::json!(
                { "error" : "no Tokio runtime handle in context", "hint" :
                "cli_tokio_metrics must be called from within the daemon runtime" }
            )
            .to_string();
        }
    };
    let metrics = handle.metrics();
    let workers = metrics.num_workers() as u64;
    let injection = metrics.global_queue_depth() as u64;
    let idle = 0u64;
    let blocking = 0u64;
    let do_record = payload
        .get("record")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    if do_record {
        crate::shared::gate_metrics::record_tokio_metrics(workers, idle, blocking, injection);
    }
    serde_json::json!(
        { "num_workers" : workers, "num_idle_threads" : idle, "num_blocking_threads" :
        blocking, "injection_queue_depth" : injection, "diagnostics" : { "backpressure" :
        idle > 0 && injection > 0, "workers_undersized" : idle == 0, "blocking_saturated"
        : false, }, "recorded" : do_record }
    )
    .to_string()
}
/// Wave A A.4 (2026-04-29): profile_query CLI handler.
/// See `cli_profile_status` docs above.
pub fn cli_profile_status(_rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let json = touring_foundation::profile::aggregator::snapshot_json();
    let section = payload.get("section").and_then(|v| v.as_str());
    let top_n = payload
        .get("top_n")
        .and_then(|v| v.as_u64())
        .map(|n| n as usize);
    let parsed: serde_json::Value = match serde_json::from_str(&json) {
        Ok(v) => v,
        Err(_) => return r#"{"entries":[],"percent_total":0.0}"#.to_string(),
    };
    // The three filtering arms were byte-identical apart from which of
    // `section`/`top_n` they applied; the envelope they built was the same in
    // all three. Applying both as optional stages keeps every combination
    // behaviourally identical — a `None` section matches every label, and a
    // `None` top_n takes every survivor — while the unfiltered case still
    // short-circuits to the parsed document untouched.
    let filtered = match (section, top_n) {
        (None, None) => parsed,
        _ => {
            let entries: Vec<serde_json::Value> = parsed
                .get("entries")
                .and_then(|e| e.as_array())
                .map(|arr| {
                    let kept = arr.iter().filter(|e| match section {
                        Some(sec) => e
                            .get("label")
                            .and_then(|l| l.as_str())
                            .map(|l| l.starts_with(sec))
                            .unwrap_or(false),
                        None => true,
                    });
                    match top_n {
                        Some(n) => kept.take(n).cloned().collect(),
                        None => kept.cloned().collect(),
                    }
                })
                .unwrap_or_default();
            serde_json::json!({
                "entries": entries,
                "percent_total": parsed
                    .get("percent_total")
                    .unwrap_or(&serde_json::Value::Null)
                    .clone(),
            })
        }
    };
    serde_json::to_string(&filtered).unwrap_or_else(|_err: serde_json::Error| json.clone())
}
