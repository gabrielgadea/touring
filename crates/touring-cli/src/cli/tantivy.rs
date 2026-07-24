//! CLI tantivy handlers (`cli_tantivy_*`) — extracted from cli_handlers.rs (A-W2.P3).

#[cfg(feature = "tantivy-fts")]
use crate::cli_handlers::normalize_to_relative;
use crate::runtime::HookRuntime;
#[cfg(feature = "tantivy-fts")]
use rusqlite::params;
#[cfg(feature = "tantivy-fts")]
use touring_analysis::e2e::schema_guard;

/// Handler: cli-tantivy-search
/// Payload: {query: str, top: usize, source_file?: str}
/// Returns BM25-ranked search hits from the Tantivy symbol index.
///
/// Wave 17 (2026-04-18): wrapped in `query_cache::get_or_compute` so
/// repeated tantivy lookups for the same `(query, top_k)` within 60s
/// return in ~1µs instead of paying the BM25 ranking cost. Critical
/// for code generation: VGP often re-searches the same symbol family
/// across iterations.
///
/// Wave D (2026-04-20): topological RRF boost. When `source_file` is
/// provided, the handler looks up the file's `community_id` from the
/// `file_communities` table and passes it to
/// `search_with_community_boost`. Symbols in the same community receive
/// a 1.3× score multiplier, surfacing topologically related symbols
/// for context-sensitive queries. Falls back to plain `search()` when
/// the community is unknown or the table is absent.
pub fn cli_tantivy_search(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    #[cfg(feature = "tantivy-fts")]
    {
        let query = payload.get("query").and_then(|v| v.as_str()).unwrap_or("");
        let top_k = payload.get("top").and_then(|v| v.as_u64()).unwrap_or(10) as usize;
        let source_file = payload.get("source_file").and_then(|v| v.as_str());
        let source_community_id: Option<u64> = source_file.and_then(|sf| {
            let rel = normalize_to_relative(sf, &rt.project_root);
            let conn = rt.ctx.knowledge.conn_ref();
            let sql = format!(
                "SELECT community_id FROM {} WHERE file_path = ?1",
                schema_guard::TABLE_FILE_COMMUNITIES
            );
            conn.query_row(&sql, params![rel], |row| row.get::<_, i64>(0))
                .ok()
                .map(|id| id as u64)
        });
        let cache_key = crate::shared::query_cache::make_key(
            "cli_tantivy_search",
            &format!("{query}|top={top_k}|cid={source_community_id:?}"),
        );
        if let Some(cached) = crate::shared::query_cache::get(&cache_key) {
            return cached;
        }
        if let Some(idx) = crate::tantivy_index::global_tantivy() {
            let out = match idx.search_with_community_boost(query, top_k, source_community_id) {
                Ok(hits) => {
                    rt.learning
                        .inject_reward("cli-tantivy-search", 1.0, "successful_query");
                    serde_json::to_string(&hits).unwrap_or_default()
                }
                Err(e) => serde_json::json!({ "error" : e, "hits" : [] }).to_string(),
            };
            crate::shared::query_cache::put(cache_key, out.clone());
            return out;
        }
        // Feature compiled in but global_tantivy() returned None (daemon init failed).
        serde_json::json!(
            { "error" : "tantivy index not initialized — run 'touring tantivy reindex' to rebuild; check daemon logs for cause", "hits" : [] }
        )
        .to_string()
    }
    #[cfg(not(feature = "tantivy-fts"))]
    {
        let _ = (rt, payload);
        serde_json::json!(
            { "error" : "tantivy-fts feature not compiled in", "hits" : [] }
        )
        .to_string()
    }
}
/// Handler: cli-tantivy-fuzzy
/// Payload: {query: str, distance: u8, top: usize}
/// Returns fuzzy (Levenshtein) BM25 search hits from the Tantivy symbol index.
pub fn cli_tantivy_fuzzy(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    #[cfg(feature = "tantivy-fts")]
    {
        let query = payload.get("query").and_then(|v| v.as_str()).unwrap_or("");
        let distance = payload
            .get("distance")
            .and_then(|v| v.as_u64())
            .unwrap_or(2) as u8;
        let top_k = payload.get("top").and_then(|v| v.as_u64()).unwrap_or(10) as usize;
        if let Some(idx) = crate::tantivy_index::global_tantivy() {
            return match idx.fuzzy_search(query, distance, top_k) {
                Ok(hits) => {
                    rt.learning
                        .inject_reward("cli-tantivy-fuzzy", 1.0, "successful_query");
                    serde_json::to_string(&hits).unwrap_or_default()
                }
                Err(e) => serde_json::json!({ "error" : e, "hits" : [] }).to_string(),
            };
        }
        serde_json::json!(
            { "error" : "tantivy index not initialized — run 'touring tantivy reindex' to rebuild; check daemon logs for cause", "hits" : [] }
        )
        .to_string()
    }
    #[cfg(not(feature = "tantivy-fts"))]
    {
        let _ = (rt, payload);
        serde_json::json!(
            { "error" : "tantivy-fts feature not compiled in", "hits" : [] }
        )
        .to_string()
    }
}
/// Handler: cli-tantivy-stats
/// Payload: {}
/// Returns IndexStats (total_docs, index_size_bytes, pending_ops, total_commits, total_upserts).
pub fn cli_tantivy_stats(_rt: &mut HookRuntime, _payload: &serde_json::Value) -> String {
    #[cfg(feature = "tantivy-fts")]
    {
        if let Some(idx) = crate::tantivy_index::global_tantivy() {
            let stats = idx.stats();
            return serde_json::to_string(&stats).unwrap_or_default();
        }
        serde_json::json!(
            { "error" : "tantivy index not initialized — run 'touring tantivy reindex' to rebuild; check daemon logs for cause" }
        )
        .to_string()
    }
    #[cfg(not(feature = "tantivy-fts"))]
    {
        serde_json::json!(
            { "error" : "tantivy-fts feature not compiled in" }
        )
        .to_string()
    }
}
/// Handler: cli-tantivy-suggest
/// Payload: {prefix: str, top: usize}
/// Returns prefix-based symbol suggestions from the Tantivy symbol index.
pub fn cli_tantivy_suggest(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    #[cfg(feature = "tantivy-fts")]
    {
        let prefix = payload.get("prefix").and_then(|v| v.as_str()).unwrap_or("");
        let top_k = payload.get("top").and_then(|v| v.as_u64()).unwrap_or(10) as usize;
        if let Some(idx) = crate::tantivy_index::global_tantivy() {
            return match idx.suggest(prefix, top_k) {
                Ok(hits) => {
                    rt.learning
                        .inject_reward("cli-tantivy-suggest", 1.0, "successful_query");
                    serde_json::to_string(&hits).unwrap_or_default()
                }
                Err(e) => serde_json::json!({ "error" : e, "hits" : [] }).to_string(),
            };
        }
        serde_json::json!(
            { "error" : "tantivy index not initialized — run 'touring tantivy reindex' to rebuild; check daemon logs for cause", "hits" : [] }
        )
        .to_string()
    }
    #[cfg(not(feature = "tantivy-fts"))]
    {
        let _ = (rt, payload);
        serde_json::json!(
            { "error" : "tantivy-fts feature not compiled in", "hits" : [] }
        )
        .to_string()
    }
}
/// Handler: cli-tantivy-reindex
///
/// Payload:
/// - `{}` or `{"mode": "full"}` — clear + full reindex (legacy, may exceed
///   handler budget on >500k symbols).
/// - `{"mode": "batch", "offset": u64, "limit": u64, "clear": bool}` —
///   incremental batch reindex. Client loops externally until `done: true`.
///   `clear: true` on the first batch clears the index; subsequent batches
///   should pass `clear: false` to preserve accumulated docs.
///
/// Response schema:
/// - `reindexed: bool` — operation succeeded
/// - `done: bool` — no more rows after this batch (page < limit)
/// - `upserted: u64` — rows upserted in this call
/// - `next_offset: u64` — where the next batch should start
/// - `stats: {...}` — Tantivy index stats snapshot
///
/// # Why batched?
/// A full 1M-symbol reindex takes ~90s which exceeds the CLI read timeout
/// (120s default, stream-bound) and the actor handler budget (15s light /
/// 300s heavy). Batching into ~25k-row chunks keeps each call under 10s,
/// avoids socket timeouts, and lets the client show progress.
pub fn cli_tantivy_reindex(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    #[cfg(feature = "tantivy-fts")]
    {
        if let Some(idx) = crate::tantivy_index::global_tantivy() {
            let Some(store) = rt.infra.symbol_store.as_ref() else {
                return serde_json::json!(
                    { "reindexed" : false, "error" : "symbol store unavailable", "stats"
                    : idx.stats(), "warning" :
                    "index NOT cleared — would cause data loss without repopulation" }
                )
                .to_string();
            };
            let mode = payload
                .get("mode")
                .and_then(|v| v.as_str())
                .unwrap_or("full");
            let batch_offset: usize =
                payload.get("offset").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
            let batch_limit: usize =
                payload.get("limit").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
            let should_clear = payload
                .get("clear")
                .and_then(|v| v.as_bool())
                .unwrap_or(mode == "full");
            if should_clear {
                if let Err(e) = idx.reindex(vec![]) {
                    return serde_json::json!(
                        { "reindexed" : false, "error" : format!("clear: {e}") }
                    )
                    .to_string();
                }
            }
            const PAGE_SIZE: usize = 5_000;
            let mut offset = batch_offset;
            let end_offset = if mode == "batch" && batch_limit > 0 {
                batch_offset.saturating_add(batch_limit)
            } else {
                usize::MAX
            };
            let mut total_upserted = 0u64;
            let mut done_early = false;
            while offset < end_offset {
                let page_size = PAGE_SIZE.min(end_offset.saturating_sub(offset));
                if page_size == 0 {
                    break;
                }
                let page = match store.symbols_page(page_size, offset) {
                    Ok(p) => p,
                    Err(e) => {
                        return serde_json::json!(
                            { "reindexed" : false, "error" :
                            format!("symbols_page offset={offset}: {e}"),
                            "upserted_before_error" : total_upserted, }
                        )
                        .to_string();
                    }
                };
                let fewer_than_requested = page.len() < page_size;
                for sym in &page {
                    let language = crate::tantivy_index::extension_to_language(&sym.file_path);
                    let crate_name = sym
                        .file_path
                        .split('/')
                        .collect::<Vec<_>>()
                        .windows(2)
                        .find(|w| w.first().copied() == Some("crates"))
                        .and_then(|w| w.get(1).map(|s| (*s).to_owned()));
                    let doc = crate::tantivy_index::SymbolDoc {
                        symbol_name: sym.symbol_name.clone(),
                        file_path: sym.file_path.clone(),
                        symbol_kind: if sym.is_definition {
                            "definition".to_owned()
                        } else {
                            "reference".to_owned()
                        },
                        module_path: None,
                        docstring: None,
                        line_number: sym.line as u64,
                        language,
                        visibility: None,
                        crate_name,
                        blake3_hash: None,
                        import_count: None,
                        export_count: None,
                        cognitive_score: None,
                        functional_signature: None,
                        community_id: None,
                    };
                    if let Err(e) = idx.upsert_symbol(&doc) {
                        tracing::warn!(
                            "tantivy_reindex: upsert failed for {}: {e}",
                            sym.symbol_name
                        );
                    } else {
                        total_upserted += 1;
                    }
                }
                offset = offset.saturating_add(page_size);
                if fewer_than_requested {
                    done_early = true;
                    break;
                }
            }
            if let Err(e) = idx.commit() {
                return serde_json::json!(
                    { "reindexed" : false, "error" : format!("final commit: {e}"),
                    "upserted" : total_upserted, }
                )
                .to_string();
            }
            return serde_json::json!(
                { "reindexed" : true, "done" : done_early || (mode == "full"), "mode" :
                mode, "upserted" : total_upserted, "next_offset" : offset, "stats" : idx
                .stats(), }
            )
            .to_string();
        }
        serde_json::json!(
            { "reindexed" : false, "error" : "tantivy index not initialized — run 'touring tantivy reindex' to rebuild; check daemon logs for cause" }
        )
        .to_string()
    }
    #[cfg(not(feature = "tantivy-fts"))]
    {
        let _ = (rt, payload);
        serde_json::json!(
            { "reindexed" : false, "error" : "tantivy-fts feature not compiled in" }
        )
        .to_string()
    }
}
