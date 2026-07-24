//! CLI memory handlers (`cli_memory_*`) — extracted from cli_handlers.rs (A-W2.P3).
//!
//! Recall (RRF-fused federated), store, reindex, stats, list. Shared helpers
//! (`semantic_or_hash_embedding`, `discover_canonical_dbs`,
//! `memory_recall_sql_federated`, `touring_claude_dir`) stay in cli_handlers.rs
//! and are referenced via `crate::cli_handlers::`.

use crate::cli_handlers::{
    ARCTIC_QUERY_PREFIX, GotchaStats, KnowledgeStats, discover_canonical_dbs,
    memory_recall_sql_federated, semantic_or_hash_embedding, semantic_text_embedding,
    touring_claude_dir,
};
use crate::runtime::HookRuntime;
use rusqlite::params;
use touring_analysis::e2e::schema_guard;

/// Reports aggregate memory store statistics (entry counts across knowledge tables) as JSON.
pub fn cli_memory_stats(rt: &mut HookRuntime, _payload: &serde_json::Value) -> String {
    let db = &rt.ctx.knowledge;
    let (gotcha_total, gotcha_unresolved, gotcha_resolved) = db.gotcha_stats();
    let file_count: usize = db
        .conn_ref()
        .query_row(
            &format!(
                "SELECT COUNT(*) FROM {}",
                schema_guard::TABLE_FILE_KNOWLEDGE
            ),
            [],
            |r| r.get(0),
        )
        .unwrap_or(0) as usize;
    let relation_count: usize = db.all_file_relations().len();
    let bash_count: i64 = db
        .conn_ref()
        .query_row(
            &format!("SELECT COUNT(*) FROM {}", schema_guard::TABLE_BASH_OUTCOMES),
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);
    let edit_count: i64 = db
        .conn_ref()
        .query_row(
            &format!("SELECT COUNT(*) FROM {}", schema_guard::TABLE_EDIT_HISTORY),
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);
    let status = KnowledgeStats {
        file_count,
        relation_count,
        bash_outcome_count: bash_count,
        edit_event_count: edit_count,
        gotcha_stats: GotchaStats {
            total_count: gotcha_total,
            unresolved_count: gotcha_unresolved as usize,
            resolved_count: gotcha_resolved as usize,
        },
        memory_entry_count: {
            let memory_db_path =
                touring_foundation::TouringConfig::memory_db_canonical(&rt.project_root);
            rusqlite::Connection::open(&memory_db_path)
                .and_then(|conn| {
                    conn.query_row("SELECT COUNT(*) FROM memory_entries", [], |r| {
                        r.get::<_, i64>(0)
                    })
                })
                .unwrap_or(0) as usize
        },
    };
    serde_json::to_string(&status)
        .unwrap_or_else(|_| r#"{"error":"serialization failed"}"#.to_string())
}
/// Recalls memory entries for a query via RRF-fused federated search across canonical databases as JSON.
pub fn cli_memory_recall(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let query = payload.get("query").and_then(|v| v.as_str()).unwrap_or("");
    if query.is_empty() {
        return serde_json::json!({ "entries" : [], "count" : 0, "query" : "" }).to_string();
    }
    let memory_db_path = touring_foundation::TouringConfig::memory_db_canonical(&rt.project_root);
    // Federated recall — search the current project's memory.db AND every
    // other project's, so a lesson stored under one project is recallable
    // from any other (the `where` half of "always know where to look").
    let memory_dbs = discover_canonical_dbs(&memory_db_path, &touring_claude_dir(), "memory.db");
    let entries = memory_recall_sql_federated(&memory_dbs, query);
    let ann_results: Vec<serde_json::Value> = {
        let borrow = rt.ctx.ann_recall.borrow();
        if let Some(recall) = borrow.as_ref() {
            let embedding = memory_recall_query_embedding(query);
            let start = std::time::Instant::now();
            let neighbors = recall.search(&embedding, 20);
            let elapsed_us = start.elapsed().as_micros() as u64;
            crate::shared::gate_metrics::record_ann_search_latency_us(elapsed_us);
            neighbors
                .into_iter()
                .map(|r| {
                    serde_json::json!(
                        { "key" : r.id, "value" : r.content, "score" : r.score, "source"
                        : "ann", }
                    )
                })
                .collect()
        } else {
            vec![]
        }
    };
    let tfidf_results: Vec<serde_json::Value> = memory_recall_tfidf(rt, query, 20);
    let entries_len = entries.len();
    let merged_entries: Vec<serde_json::Value> =
        if ann_results.is_empty() && tfidf_results.is_empty() {
            entries
        } else {
            memory_recall_rrf_merge_n(&[&entries[..], &ann_results[..], &tfidf_results[..]], 20)
        };
    #[cfg(feature = "tantivy-fts")]
    let symbol_context: Vec<serde_json::Value> = {
        crate::tantivy_index::global_tantivy()
            .and_then(|idx| idx.search(query, 5).ok())
            .unwrap_or_default()
            .into_iter()
            .map(|hit| {
                serde_json::json!(
                    { "symbol_name" : hit.symbol_name, "file_path" : hit.file_path,
                    "symbol_kind" : hit.symbol_kind, "line_number" : hit.line_number,
                    "score" : hit.score, }
                )
            })
            .collect()
    };
    #[cfg(not(feature = "tantivy-fts"))]
    let symbol_context: Vec<serde_json::Value> = vec![];
    let memory_diagnostics: Vec<serde_json::Value> = {
        use crate::memory_finding::MemoryFinding;
        let mut diags = vec![];
        if merged_entries.is_empty() {
            let f = MemoryFinding::RecallEmpty {
                query: query.to_string(),
            };
            tracing::info!(code = f.code_str(), % query, "recall empty for query");
            diags.push(serde_json::json!(
                { "code" : f.code_str(), "severity" : "info", "message" :
                format!("No memory entries found for query: {query}") }
            ));
        }
        if !tfidf_results.is_empty() {
            let f = MemoryFinding::TfidfActivated {
                candidate_count: tfidf_results.len(),
                corpus_size: entries_len,
            };
            tracing::debug!(
                code = f.code_str(),
                candidate_count = tfidf_results.len(),
                "tfidf activated"
            );
            diags.push(serde_json::json!(
                { "code" : f.code_str(), "severity" : "debug", "message" :
                format!("TF-IDF activated: {} candidates from corpus of {entries_len}",
                tfidf_results.len()) }
            ));
        }
        if !ann_results.is_empty() || !tfidf_results.is_empty() {
            let source_count = usize::from(entries_len > 0)
                + usize::from(!ann_results.is_empty())
                + usize::from(!tfidf_results.is_empty());
            let f = MemoryFinding::RrfFusion {
                source_count,
                merged_count: merged_entries.len(),
            };
            tracing::debug!(code = f.code_str(), source_count, "rrf fusion");
            diags.push(serde_json::json!(
                { "code" : f.code_str(), "severity" : "debug", "message" :
                format!("RRF fusion from {source_count} sources → {} results",
                merged_entries.len()) }
            ));
        }
        diags
    };
    let entry_count = merged_entries.len();
    let ann_count = ann_results.len();
    serde_json::json!(
        { "entries" : merged_entries, "count" : entry_count, "query" : query,
        "ann_results" : ann_count, "symbol_context" : symbol_context, "diagnostics" :
        memory_diagnostics, }
    )
    .to_string()
}
/// Query embedding for the ANN recall path. Semantic (with the arctic query
/// prefix) when available, else the raw-query 64-dim hash. The prefix is applied
/// only here, never to stored documents — that asymmetry is what makes
/// retrieval ranking discriminative.
fn memory_recall_query_embedding(query: &str) -> Vec<f32> {
    semantic_text_embedding(&format!("{ARCTIC_QUERY_PREFIX}{query}"))
        .unwrap_or_else(|| crate::ann_memory::query_hash_embedding(query))
}
/// Merge N ranked result lists via Reciprocal Rank Fusion (k=60).
///
/// Each list contributes `1 / (rank + 1 + k)` per entry. Entries with the
/// same `key` accumulate scores across lists. Returns up to `limit` entries
/// sorted by descending combined RRF score.
///
/// **List ordering matters for tie-breaks** (the canonical value taken on
/// duplicate keys is the first occurrence — typically SQL > ANN > TF-IDF).
/// SQL entries carry `tier` / `type` metadata that other sources lack;
/// putting SQL first preserves that fidelity.
///
/// History:
/// - Wave 22: original 2-list signature `(sql, ann)`.
/// - Wave M2 (2026-04-25, Hard Rule #11 reescopo): generalised to N lists so
///   TF-IDF (Wave M1) can plug in as a third orthogonal source.
fn memory_recall_rrf_merge_n(
    lists: &[&[serde_json::Value]],
    limit: usize,
) -> Vec<serde_json::Value> {
    use std::collections::HashMap;
    const RRF_K: f64 = 60.0;
    let mut rrf_map: HashMap<String, (f64, serde_json::Value)> = HashMap::new();
    for list in lists {
        for (rank, entry) in list.iter().enumerate() {
            let key = entry
                .get("key")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if key.is_empty() {
                continue;
            }
            let score = 1.0 / (rank as f64 + 1.0 + RRF_K);
            let e = rrf_map.entry(key).or_insert((0.0, entry.clone()));
            e.0 += score;
        }
    }
    let mut merged: Vec<(f64, serde_json::Value)> = rrf_map.into_values().collect();
    merged.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    merged.into_iter().take(limit).map(|(_, v)| v).collect()
}
/// Build (or load from cache) a TF-IDF index over the touring memory corpus
/// and return up to `top_k` hits as RRF-ready JSON entries.
///
/// Cold path: rebuilds the index when the cache is absent or older than
/// `tfidf_retriever::CACHE_TTL_SECS` (1 hour). Hot path: reuses the cached
/// index. Failures (missing dbs, empty corpus) degrade silently to an empty
/// vec — the consumer (cli_memory_recall) already runs SQL + ANN paths in
/// parallel so a missing third source is non-fatal.
fn memory_recall_tfidf(rt: &mut HookRuntime, query: &str, top_k: usize) -> Vec<serde_json::Value> {
    use crate::tfidf_retriever::{CACHE_TTL_SECS, TfidfIndex, default_cache_path};
    let cache_path = default_cache_path(&rt.project_root);
    let index = match TfidfIndex::load_cache(&cache_path, CACHE_TTL_SECS) {
        Ok(Some(idx)) => idx,
        _ => {
            let memory_db =
                touring_foundation::TouringConfig::memory_db_canonical(&rt.project_root);
            let knowledge_db =
                touring_foundation::TouringConfig::knowledge_db_canonical(&rt.project_root);
            match TfidfIndex::build_from_db(&memory_db, &knowledge_db) {
                Ok(idx) => {
                    if let Err(e) = idx.save_cache(&cache_path) {
                        tracing::debug!(
                            target : "touring::tfidf", "tfidf cache persist failed: {e}"
                        );
                    }
                    idx
                }
                Err(_) => return Vec::new(),
            }
        }
    };
    index
        .query(query, top_k)
        .into_iter()
        .map(|hit| {
            let normalized_key = hit
                .key
                .strip_prefix("memory:")
                .map(str::to_string)
                .unwrap_or(hit.key);
            serde_json::json!(
                { "key" : normalized_key, "value" : hit.snippet, "score" : hit.score,
                "source" : format!("tfidf:{}", hit.source), }
            )
        })
        .collect()
}
// Carve R (2026-06-10): runtime-service handler moved to touring-hook-runtime::ceg_impls
// (it is a pure HookRuntime capability); re-exported at the historical path.
pub use touring_hook_runtime::ceg_impls::cli_memory_store;
/// Backfill the ANN corpus from all existing `memory_entries` rows. S-04 (2026-05-29).
///
/// Walks every row in `memory_entries`, generates a 64-dim hash embedding for
/// each value, and upserts in batches into the ANN corpus (idempotent via
/// `add_batch`'s INSERT OR REPLACE). This is what populates an empty ANN corpus
/// so `memory recall` returns ANN hits instead of degrading to FTS/TF-IDF only.
/// Payload: `{"batch_size": N}` (default 256).
pub fn cli_memory_reindex(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let batch_size = payload
        .get("batch_size")
        .and_then(|v| v.as_u64())
        .unwrap_or(256) as usize;

    if !rt.ctx.ann_recall.borrow().is_some() {
        return serde_json::json!({
            "error": "ANN recall not initialised — daemon startup did not call init_ann_memory"
        })
        .to_string();
    }

    let memory_db_path = touring_foundation::TouringConfig::memory_db_canonical(&rt.project_root);
    let rows: Vec<(String, String)> = match rusqlite::Connection::open(&memory_db_path) {
        Ok(conn) => {
            let Ok(mut stmt) = conn.prepare("SELECT key, value FROM memory_entries ORDER BY rowid")
            else {
                return serde_json::json!({ "error": "failed to prepare SELECT" }).to_string();
            };
            stmt.query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map(|rows| rows.filter_map(|r| r.ok()).collect())
            .unwrap_or_default()
        }
        Err(e) => {
            return serde_json::json!({ "error": format!("cannot open memory.db: {e}") })
                .to_string();
        }
    };

    let total = rows.len();
    let mut indexed = 0usize;
    let mut failed = 0usize;
    for chunk in rows.chunks(batch_size) {
        let entries: Vec<crate::ann_memory::MemoryEntry> = chunk
            .iter()
            .map(|(key, value)| {
                let emb = semantic_or_hash_embedding(value);
                crate::ann_memory::MemoryEntry::new(key.as_str(), value.as_str(), emb)
            })
            .collect();
        let mut borrow = rt.ctx.ann_recall.borrow_mut();
        if let Some(ann) = borrow.as_mut() {
            match ann.add_batch(entries) {
                Ok(()) => indexed += chunk.len(),
                Err(e) => {
                    tracing::warn!("ANN reindex batch failed: {e}");
                    failed += chunk.len();
                }
            }
        }
    }

    serde_json::json!({
        "total_entries": total,
        "indexed": indexed,
        "failed": failed,
        "batch_size": batch_size,
        "status": if failed == 0 { "ok" } else { "partial" },
    })
    .to_string()
}
/// Lists stored memory entries (optionally filtered by tier or type) as JSON.
pub fn cli_memory_list(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let limit = payload.get("limit").and_then(|v| v.as_u64()).unwrap_or(20) as i64;
    let sort_field = payload
        .get("sort")
        .and_then(|v| v.as_str())
        .unwrap_or("access_count");
    let order = memory_list_order_clause(sort_field);
    let query = format!(
        "SELECT file_path, notes, read_count, COALESCE(last_read_at, '')
         FROM {} WHERE file_path LIKE '__memory__:%' {order} LIMIT ?1",
        schema_guard::TABLE_FILE_KNOWLEDGE
    );
    let db = &rt.ctx.knowledge;
    let mut stmt = match db.conn_ref().prepare(&query) {
        Ok(s) => s,
        Err(e) => {
            return serde_json::json!(
                { "error" : format!("query failed: {e}"), "entries" : [], "count" : 0 }
            )
            .to_string();
        }
    };
    let entries: Vec<serde_json::Value> = stmt
        .query_map(params![limit], |row| {
            Ok(parse_memory_row(
                &row.get::<_, String>(0)?,
                &row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                &row.get::<_, String>(3)?,
            ))
        })
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default();
    let count = entries.len();
    serde_json::json!({ "entries" : entries, "count" : count }).to_string()
}
/// Helper: determine ORDER BY clause for memory list queries.
fn memory_list_order_clause(sort_field: &str) -> &'static str {
    match sort_field {
        "last_read_at" | "last_accessed" => "ORDER BY last_read_at DESC",
        _ => "ORDER BY read_count DESC",
    }
}
/// Helper: parse a raw __memory__:tier:type:key into structured JSON.
fn parse_memory_row(
    raw_key: &str,
    notes: &str,
    read_count: i64,
    last_read_at: &str,
) -> serde_json::Value {
    let parts: Vec<&str> = raw_key.splitn(4, ':').collect();
    let (tier, entry_type, key) = match (parts.get(1), parts.get(2), parts.get(3)) {
        (Some(t), Some(et), Some(k)) => (*t, *et, *k),
        _ => ("unknown", "unknown", raw_key),
    };
    serde_json::json!(
        { "key" : key, "value" : notes, "tier" : tier, "type" : entry_type,
        "access_count" : read_count, "last_accessed" : last_read_at }
    )
}
