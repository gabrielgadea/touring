//! CEG IoC implementations — `LearnRuntime` / `CegRuntime` for [`HookRuntime`]
//! plus the three runtime-service handlers they delegate to.
//!
//! Carve R (2026-06-10): these inherent trait impls moved here from the cli
//! dispatch hub because Rust's orphan rule requires `impl Trait for Type` to
//! live in the crate that owns the trait or the type — `HookRuntime` now
//! lives in this crate. The three handlers (`cli_learning_reward`,
//! `cli_gotcha_add`, `cli_memory_store`) are pure runtime capabilities
//! (RL reward, gotcha DB, memory store) with zero cli/ coupling; the cli
//! modules re-export them at their historical paths.

use crate::embeddings::semantic_or_hash_embedding;
use crate::hook_runtime::HookRuntime;
use rusqlite::params;

/// Schema for the `memory_entries` table — single source of truth, used for both
/// the idempotent create and the post-migration recreate. `IF NOT EXISTS` is safe
/// on the recreate path too: that path runs only after `DROP TABLE`, so the table
/// is absent and the statement creates it fresh.
const CREATE_MEMORY_ENTRIES: &str = "CREATE TABLE IF NOT EXISTS memory_entries (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL,
                tier TEXT NOT NULL DEFAULT 'local',
                entry_type TEXT NOT NULL DEFAULT 'insight',
                access_count INTEGER NOT NULL DEFAULT 0,
                last_accessed_at TEXT NOT NULL DEFAULT (datetime('now')),
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            )";

/// Injects an immediate RL reward for a tool into the runtime Q-table, clamping the value to `[-1.0, 1.0]`.
pub fn cli_learning_reward(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let tool = payload
        .get("tool_name")
        .or_else(|| payload.get("tool"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let value = payload
        .get("reward")
        .or_else(|| payload.get("value"))
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let context = payload
        .get("context")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if tool.is_empty() {
        return serde_json::json!({ "error" : "tool name is required" }).to_string();
    }
    let clamped = value.clamp(-1.0, 1.0);
    let reward = touring_intelligence::rl::ImmediateReward {
        tool_name: tool.to_string(),
        accepted: clamped > 0.0,
        latency_ms: 0,
        error_count: if clamped < 0.0 { 1 } else { 0 },
        cila_level: 2,
        file_type: 3,
        quality_score: Some(clamped),
    };
    let mut qtable = rt.learning.qtable_cache.take().unwrap_or_default();
    rt.process_immediate_reward(&reward, &mut qtable);
    rt.learning.qtable_cache = Some(qtable);
    serde_json::json!(
        { "tool" : tool, "value" : clamped, "context" : context, "status" :
        "reward_injected" }
    )
    .to_string()
}

/// Adds a pitfall pattern with description and severity to the knowledge gotcha database.
pub fn cli_gotcha_add(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let pattern = payload
        .get("pattern")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let description = payload
        .get("description")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let severity = payload
        .get("severity")
        .and_then(|v| v.as_str())
        .unwrap_or("medium");
    if pattern.is_empty() || description.is_empty() {
        return serde_json::json!({ "error" : "pattern and description are required" }).to_string();
    }
    match rt
        .ctx
        .knowledge
        .add_gotcha(pattern, description, severity, None)
    {
        Ok(id) => serde_json::json!(
            { "id" : id, "pattern" : pattern, "description" : description, "severity"
            : severity, "status" : "added" }
        )
        .to_string(),
        Err(e) => serde_json::json!({ "error" : format!("failed to add gotcha: {e}") }).to_string(),
    }
}

/// Persists a key/value memory entry into the SQLite store and mirrors it into the ANN recall corpus.
pub fn cli_memory_store(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let key = payload.get("key").and_then(|v| v.as_str()).unwrap_or("");
    let value = payload.get("value").and_then(|v| v.as_str()).unwrap_or("");
    let tier = payload
        .get("tier")
        .and_then(|v| v.as_str())
        .unwrap_or("local");
    // CLI (`touring memory store`) sends "entry_type"; in-process callers
    // (post_tool_rl) send "type". Accept both so the ANN-indexed store path is
    // reached regardless of caller. (S-04 2026-05-29.)
    let entry_type = payload
        .get("type")
        .or_else(|| payload.get("entry_type"))
        .and_then(|v| v.as_str())
        .unwrap_or("insight");
    if key.is_empty() || value.is_empty() {
        return serde_json::json!({ "error" : "key and value are required" }).to_string();
    }
    // The measured outcome of this case, if the caller knows it. Absent stays
    // NULL: an unobserved case is not a failed one, and recording 0.0 for
    // "unknown" would teach a value-ranked recall that unmeasured means bad.
    let outcome_reward = payload
        .get("reward")
        .or_else(|| payload.get("outcome_reward"))
        .and_then(serde_json::Value::as_f64)
        .map(|r| r.clamp(-1.0, 1.0));
    let outcome_context = payload
        .get("outcome_context")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty());
    // S4 (2026-08-07): weight, pinning and supersession — the three mechanisms
    // that let the ACO pheromone evaporate. Without them a corrected lesson
    // keeps guiding with the same weight as the correction, and test junk
    // competes with curated knowledge on equal footing (observed live: a
    // `recall` returned `purpose-test-key-zx9` among its top hits).
    //
    // All three stay NULL/absent unless the caller sets them: an unweighted
    // entry has not been judged, and inventing a default importance would be
    // exactly the "approximation that erases the signal" this work removes.
    let importance = payload
        .get("importance")
        .and_then(serde_json::Value::as_i64)
        .map(|i| i.clamp(1, 5));
    let pinned = payload
        .get("pinned")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let supersedes = payload
        .get("supersedes")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty());
    let memory_db_path = touring_foundation::TouringConfig::memory_db_canonical(&rt.project_root);
    if let Some(parent) = memory_db_path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let result = rusqlite::Connection::open(&memory_db_path)
        .and_then(|conn| {
            conn.execute(CREATE_MEMORY_ENTRIES, [])?;
            let has_last_accessed: bool = conn
                .query_row(
                    "SELECT COUNT(*) FROM pragma_table_info('memory_entries') WHERE name = 'last_accessed_at'",
                    [],
                    |r| r.get::<_, i32>(0),
                )
                .map(|c| c > 0)
                .unwrap_or(false);
            if !has_last_accessed {
                conn.execute("DROP TABLE IF EXISTS memory_entries", [])?;
                conn.execute(CREATE_MEMORY_ENTRIES, [])?;
            }
            // The `r` of a case `(s, a, r)` — Memento Eq. 12. Added idempotently
            // so an existing store gains the columns without a destructive
            // rebuild (`ALTER TABLE` errors when the column is already there,
            // which is exactly the "already migrated" signal, hence `.ok()`).
            conn.execute(
                "ALTER TABLE memory_entries ADD COLUMN outcome_reward REAL",
                [],
            )
            .ok();
            conn.execute(
                "ALTER TABLE memory_entries ADD COLUMN outcome_context TEXT",
                [],
            )
            .ok();
            // S4 columns — same idempotent pattern: an existing store gains them
            // without a destructive rebuild, and the `already exists` error IS
            // the "already migrated" signal.
            conn.execute("ALTER TABLE memory_entries ADD COLUMN importance INTEGER", [])
                .ok();
            conn.execute(
                "ALTER TABLE memory_entries ADD COLUMN pinned INTEGER NOT NULL DEFAULT 0",
                [],
            )
            .ok();
            conn.execute(
                "ALTER TABLE memory_entries ADD COLUMN superseded_by TEXT",
                [],
            )
            .ok();

            conn.execute(
                "INSERT OR REPLACE INTO memory_entries (key, value, tier, entry_type, access_count, last_accessed_at, outcome_reward, outcome_context, importance, pinned)
             VALUES (?1, ?2, ?3, ?4, COALESCE((SELECT access_count FROM memory_entries WHERE key = ?1), 0) + 1, datetime('now'), ?5, ?6,
                     COALESCE(?7, (SELECT importance FROM memory_entries WHERE key = ?1)),
                     ?8)",
                params![
                    key,
                    value,
                    tier,
                    entry_type,
                    outcome_reward,
                    outcome_context,
                    importance,
                    i64::from(pinned)
                ],
            )?;
            // Retire the superseded entry: it stays in the table for audit and
            // stops surfacing in recall. Pointing at the NEW key (rather than
            // deleting) keeps the correction traceable to what it corrected.
            if let Some(old_key) = supersedes {
                conn.execute(
                    "UPDATE memory_entries SET superseded_by = ?1 WHERE key = ?2 AND key != ?1",
                    params![key, old_key],
                )
                .ok();
            }
            Ok(())
        });
    match result {
        Ok(_) => {
            // S-04 (2026-05-29): mirror the stored entry into the ANN corpus so
            // `memory recall` returns ANN hits (RRF no longer degrades to FTS/
            // TF-IDF only). Embedding = 64-dim deterministic hash vector (no GPU
            // / model). `ann_recall` is initialized at daemon startup
            // (init_ann_memory, daemon.rs:1446). Fail-open: the SQL store already
            // succeeded; an ANN failure only sets ann_indexed=false.
            let ann_indexed = {
                let mut borrow = rt.ctx.ann_recall.borrow_mut();
                if let Some(ann) = borrow.as_mut() {
                    let entry = crate::ann_memory::MemoryEntry::new(
                        key,
                        value,
                        semantic_or_hash_embedding(value),
                    );
                    match ann.add_memory(entry) {
                        Ok(()) => true,
                        Err(e) => {
                            tracing::warn!("ANN upsert failed (SQL store ok): {e}");
                            false
                        }
                    }
                } else {
                    false
                }
            };
            serde_json::json!(
                { "key" : key, "tier" : tier, "type" : entry_type, "status" : "stored", "ann_indexed" : ann_indexed }
            )
                .to_string()
        }
        Err(e) => {
            serde_json::json!({ "error" : format!("failed to store memory: {}", e) }).to_string()
        }
    }
}

// =============================================================================
// S-13 (2026-06-06) — CEG X9 LEARN dependency-inversion seam.
// =============================================================================
//
// `gateway::deps::LearnRuntime` is the trait the gateway's X9 LEARN stage
// (`gateway/learn.rs`) uses to persist RL rewards, gotchas, and memory lessons
// *without* importing this `cli_handlers` module. Implementing it HERE — on the
// parent side — inverts the old `gateway → cli_handlers` edge into
// `cli_handlers → gateway::deps`, which is the correct direction for a future
// CEG crate extraction (parent depends on the leaf, never the reverse). Each
// method delegates to the canonical handler re-exported above, so there is no
// behavioural change and no logic duplication — only the dependency direction
// flips. Fail-open by construction: the handlers return a JSON string and never
// panic. See `crates/touring-hooks/src/gateway/deps.rs`.
impl crate::gateway::deps::LearnRuntime for crate::runtime::HookRuntime {
    fn learning_reward(&mut self, payload: &serde_json::Value) -> String {
        cli_learning_reward(self, payload)
    }

    fn gotcha_add(&mut self, payload: &serde_json::Value) -> String {
        cli_gotcha_add(self, payload)
    }

    fn memory_store(&mut self, payload: &serde_json::Value) -> String {
        cli_memory_store(self, payload)
    }
}

// S-13 (2026-06-06) — the full X9 LEARN runtime-service seam. Closes the last CEG
// edge: with this impl, `gateway/learn.rs` is generic over `CegRuntime` and no
// longer names `crate::runtime::HookRuntime`. Each method delegates to the
// `HookRuntime` field it abstracts; fail-open by construction.
impl crate::gateway::deps::CegRuntime for crate::runtime::HookRuntime {
    fn record_tool_outcome(&self, event_type: &str, payload: &[u8]) {
        if let (Some(actor), Some(ledger)) = (&self.actor_id, &self.cross_agent_ledger)
            && let Err(e) = ledger.write_event(actor, event_type, payload)
        {
            tracing::debug!(error = %e, "cross-agent ledger write_event failed (fail-open)");
        }
    }

    fn drift_cache_get(&self, scope: &str, key: &str) -> Option<String> {
        self.ctx.result_cache.get_result(scope, key)
    }

    fn drift_cache_put(&self, scope: &str, key: &str, value: String) {
        self.ctx.result_cache.cache_result(scope, key, value);
    }

    fn contract_attestation(&self) -> Option<&crate::gateway::harness_contract::HarnessContract> {
        self.contract_attestation.as_ref()
    }
}
