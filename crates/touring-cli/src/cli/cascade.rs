//! CLI cascade-queue handlers (`cli_cascade_*`) — extracted from cli_handlers.rs (A-W2.P3).

use crate::runtime::HookRuntime;

/// Return the current size of the cascade queue without draining.
pub fn cli_cascade_queue_status(rt: &mut HookRuntime, _payload: &serde_json::Value) -> String {
    let len = rt.ctx.cascade_queue.len();
    let body = serde_json::json!({ "len" : len, "is_empty" : len == 0, });
    serde_json::to_string(&body)
        .unwrap_or_else(|_| r#"{"error":"serialization failed"}"#.to_string())
}
/// Drain fresh cascade proposals, returning them as JSON for the decomposer.
///
/// Returns `{drained_count, proposals: [{path, proposals: [SubtaskProposal], queued_at_secs}]}`.
/// Stale items (age > TTL) are discarded and counted separately.
pub fn cli_cascade_queue_drain(rt: &mut HookRuntime, _payload: &serde_json::Value) -> String {
    let stale_before = rt.ctx.cascade_queue.len();
    rt.ctx.cascade_queue.evict_stale();
    let stale_evicted = stale_before - rt.ctx.cascade_queue.len();
    let fresh = rt.ctx.cascade_queue.drain_fresh();
    let drained_count = fresh.len();
    let proposals_json: Vec<serde_json::Value> = fresh
        .into_iter()
        .map(|item| {
            let since_epoch = item
                .queued_at
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            serde_json::json!(
                { "path" : item.path.to_string_lossy(), "proposals" : item.proposals
                .iter().map(| p | serde_json::json!({ "api_item" : p.api_item, "symbol" :
                p.symbol, "reason" : p.reason, "severity" : format!("{:?}", p.severity),
                "callers" : p.callers.len(), })).collect::< Vec < _ >> (),
                "queued_at_secs" : since_epoch, }
            )
        })
        .collect();
    let body = serde_json::json!(
        { "drained_count" : drained_count, "stale_evicted" : stale_evicted, "proposals" :
        proposals_json, }
    );
    serde_json::to_string(&body)
        .unwrap_or_else(|_| r#"{"error":"serialization failed"}"#.to_string())
}
