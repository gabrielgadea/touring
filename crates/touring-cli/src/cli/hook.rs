//! CLI hook-memory handlers (`cli_hook_memory_*`) — extracted from cli_handlers.rs (A-W2.P4).
//!
//! Store/recall over the `hook_events` table via `SqliteHookMemoryBridge`.

use crate::cli::params::{str_or, str_or_empty, u64_or};
use crate::hook_memory::{HookEvent, HookMemoryBridge, MemoryTier, SqliteHookMemoryBridge};
use crate::runtime::HookRuntime;
use rusqlite::params;

/// Handler: hook-memory-store
/// Payload: {hook_name, session_id, event_type, data, ttl_seconds}
/// Inlines HookMemoryBridge::store_hook_event logic directly on the connection.
/// The bridge is not stored in HookRuntime, so we execute SQL directly.
pub fn cli_hook_memory_store(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let hook_name = str_or_empty(payload, "hook_name");
    let session_id = str_or_empty(payload, "session_id");
    let event_type = str_or(payload, "event_type", "execution");
    let data = payload
        .get("data")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let _ttl_seconds = u64_or(payload, "ttl_seconds", 3600);
    if hook_name.is_empty() || session_id.is_empty() {
        return serde_json::json!({ "error" : "hook_name and session_id are required" })
            .to_string();
    }
    let event = HookEvent::new(
        hook_name,
        session_id,
        rt.project_root.to_str().unwrap_or(""),
        event_type,
        data,
        String::new(), // content_hash computed by caller if needed
    );
    let conn = rt.ctx.knowledge.conn_ref();
    if let Err(e) = SqliteHookMemoryBridge::new(conn).ensure_schema() {
        return serde_json::json!({ "error" : format!("schema init failed: {}", e) }).to_string();
    }
    let corr_json = match serde_json::to_string(&event.correlation_ids) {
        Ok(j) => j,
        Err(e) => {
            return serde_json::json!({ "error" : format!("serialization error: {}", e) })
                .to_string();
        }
    };
    let sql = "INSERT INTO hook_events
         (event_id, hook_name, session_id, project_dir, event_type, timestamp, data, content_hash, correlation_ids, outcome_linked)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
         ON CONFLICT(event_id) DO UPDATE SET
            hook_name = excluded.hook_name,
            session_id = excluded.session_id,
            project_dir = excluded.project_dir,
            event_type = excluded.event_type,
            timestamp = excluded.timestamp,
            data = excluded.data,
            content_hash = excluded.content_hash,
            correlation_ids = excluded.correlation_ids,
            outcome_linked = excluded.outcome_linked";
    let result = conn.execute(
        sql,
        params![
            event.event_id,
            event.hook_name,
            event.session_id,
            event.project_dir,
            event.event_type,
            event.timestamp,
            event.data,
            event.content_hash,
            corr_json,
            event.outcome_linked as i32,
        ],
    );
    match result {
        Ok(_) => serde_json::json!(
            { "status" : "stored", "hook_name" : hook_name, "session_id" :
            session_id, "event_type" : event_type }
        )
        .to_string(),
        Err(e) => serde_json::json!({ "error" : format!("failed to store hook event: {}", e) })
            .to_string(),
    }
}
/// Handler: hook-memory-recall
/// Payload: {query, tier, limit}
/// Uses: HookMemoryBridge::recall_hook_patterns()
pub fn cli_hook_memory_recall(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let query = payload.get("query").and_then(|v| v.as_str()).unwrap_or("");
    let tier_str = payload
        .get("tier")
        .and_then(|v| v.as_str())
        .unwrap_or("ephemeral");
    let limit = payload.get("limit").and_then(|v| v.as_u64()).unwrap_or(10) as usize;
    let tier = MemoryTier::parse(tier_str).unwrap_or(MemoryTier::Ephemeral);
    if query.is_empty() {
        return serde_json::json!(
            { "patterns" : [], "count" : 0, "query" : "", "tier" : tier_str }
        )
        .to_string();
    }
    let conn = rt.ctx.knowledge.conn_ref();
    let bridge = SqliteHookMemoryBridge::new(conn);
    let patterns = bridge.recall_hook_patterns(query, tier, limit);
    let result: Vec<serde_json::Value> = patterns
        .into_iter()
        .map(|p| {
            serde_json::json!(
                { "pattern_key" : p.pattern_key, "sample_count" : p.sample_count,
                "avg_reward" : p.avg_reward, "avg_latency_ms" : p.avg_latency_ms,
                "confidence" : p.confidence, "first_seen" : p.first_seen, "last_seen" : p
                .last_seen }
            )
        })
        .collect();
    serde_json::json!(
        { "patterns" : result, "count" : result.len(), "query" : query, "tier" : tier_str
        }
    )
    .to_string()
}
