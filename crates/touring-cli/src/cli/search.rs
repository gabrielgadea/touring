//! CLI search handlers (`cli_search_*`) — extracted from cli_handlers.rs (A-W2.P4).
//!
//! `cli_search_symbols`: LIKE over `wiring_map.symbol_name`, one clause per query
//! token, ranked by how many tokens a name satisfies — so a prose query such as
//! "run the gateway pipeline" reaches `run_gateway` instead of being matched as
//! one literal substring that no symbol name contains. A single-token query is
//! byte-identical to the previous `LIKE '%token%'` scan (the `exact` subcommand).
//!
//! `cli_search_docs`: BM25 over the project's Tantivy symbol index (name,
//! signature, docstring — the same index behind `touring tantivy search`) when the
//! project has one, falling back to the LIKE scan of `file_knowledge` otherwise.
//!
//! Measured 12/09/2026 (docs/plans/2026-09-12-graft-analysis §14): both handlers
//! ran `LIKE '%<whole query>%'`, so every multi-word query returned `[]` and
//! `touring search unified|bm25` scored 0/10 on a 10-question prose benchmark
//! while the same Tantivy index reached 7/10 through `touring tantivy search`.

use crate::cli::params;
use crate::runtime::HookRuntime;
use rusqlite::params as sql_params;
use touring_analysis::e2e::schema_guard;

/// Upper bound on LIKE clauses per query — bounds SQL size and keeps a pasted
/// paragraph from turning into a hundred scans.
const MAX_QUERY_TOKENS: usize = 8;

/// Words that carry no symbol-name signal; dropped before the LIKE scan so a
/// prose query is not diluted by them (they still reach Tantivy in `cli_search_docs`).
const STOPWORDS: &[&str] = &[
    "a", "an", "and", "are", "as", "at", "be", "by", "for", "from", "how", "in", "into", "is",
    "it", "of", "on", "or", "that", "the", "this", "to", "what", "when", "where", "which", "with",
];

/// Query tokens for the LIKE scan: maximal runs of `[[:alnum:]_]`, stopwords and
/// single characters removed, capped at [`MAX_QUERY_TOKENS`]. SQLite `LIKE` is
/// ASCII case-insensitive, so no lowercasing is needed.
pub(crate) fn query_tokens(query: &str) -> Vec<String> {
    query
        .split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .filter(|t| t.len() > 1 && !STOPWORDS.contains(&t.to_ascii_lowercase().as_str()))
        .take(MAX_QUERY_TOKENS)
        .map(str::to_string)
        .collect()
}

/// SQL for the ranked multi-token symbol scan: `hits` counts how many of the `n`
/// LIKE patterns (`?1..?n`) a name satisfies; rows with zero hits are dropped and
/// the limit is `?n+1`. Pure so the shape is testable without a database.
pub(crate) fn ranked_like_sql(n: usize, table: &str) -> String {
    let hits = (1..=n)
        .map(|i| format!("(CASE WHEN symbol_name LIKE ?{i} THEN 1 ELSE 0 END)"))
        .collect::<Vec<_>>()
        .join(" + ");
    format!(
        "SELECT symbol_name, module_file, symbol_kind, MAX({hits}) AS hits \
         FROM {table} GROUP BY symbol_name, module_file, symbol_kind \
         HAVING hits > 0 ORDER BY hits DESC, symbol_name LIMIT ?{}",
        n + 1
    )
}

/// Shared LIKE-search pipeline behind both `cli_search_*` handlers.
///
/// The two handlers differ only in the SQL projection and in how a row becomes
/// JSON; the empty-query guard, the `top` clamp, the `%…%` pattern, the
/// prepare-error envelope and the result envelope were byte-identical copies.
/// `sql` is built by the caller so each handler keeps ownership of its own
/// `schema_guard` table constant.
fn like_search<F>(
    rt: &mut HookRuntime,
    payload: &serde_json::Value,
    sql: &str,
    map_row: F,
) -> String
where
    F: Fn(&rusqlite::Row<'_>) -> rusqlite::Result<serde_json::Value>,
{
    let query = params::str_or_empty(payload, "query");
    if query.is_empty() {
        return serde_json::json!({ "error" : "query required" }).to_string();
    }
    let top = params::i64_or(payload, "top", 10).clamp(1, 100);
    let conn = rt.ctx.knowledge.conn_ref();
    let pattern = format!("%{}%", query);
    let mut stmt = match conn.prepare(sql) {
        Ok(s) => s,
        Err(e) => {
            return serde_json::json!({ "error" : format!("query failed: {e}") }).to_string();
        }
    };
    let results: Vec<serde_json::Value> = stmt
        .query_map(sql_params![pattern, top], |row| map_row(row))
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default();
    let count = results.len();
    let mut out = serde_json::json!({ "query" : query, "results" : results, "count" : count });
    crate::cli_handlers_index::flag_partial_index(rt, &mut out);
    out.to_string()
}

/// Search symbols by name using LIKE matching against wiring_map.
///
/// Payload: `{"query": "...", "top": 10}`. One token keeps the historical
/// `LIKE '%token%'` scan; several tokens are scanned one clause each and ranked
/// by the number of tokens the name satisfies (`hits`).
pub fn cli_search_symbols(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let query = params::str_or_empty(payload, "query");
    let tokens = query_tokens(query);
    if tokens.len() <= 1 {
        let sql = format!(
            "SELECT DISTINCT symbol_name, module_file, symbol_kind \
             FROM {} WHERE symbol_name LIKE ?1 ORDER BY symbol_name LIMIT ?2",
            schema_guard::TABLE_WIRING_MAP
        );
        return like_search(rt, payload, &sql, |row| {
            Ok(serde_json::json!(
                { "symbol_name" : row.get::< _, String > (0) ?, "file_path" : row
                .get::< _, String > (1) ?, "symbol_kind" : row.get::< _, String >
                (2) ? }
            ))
        });
    }
    let top = params::i64_or(payload, "top", 10).clamp(1, 100);
    let sql = ranked_like_sql(tokens.len(), schema_guard::TABLE_WIRING_MAP);
    let conn = rt.ctx.knowledge.conn_ref();
    let mut stmt = match conn.prepare(&sql) {
        Ok(s) => s,
        Err(e) => {
            return serde_json::json!({ "error" : format!("query failed: {e}") }).to_string();
        }
    };
    let mut bound: Vec<rusqlite::types::Value> = tokens
        .iter()
        .map(|t| rusqlite::types::Value::Text(format!("%{t}%")))
        .collect();
    bound.push(rusqlite::types::Value::Integer(top));
    let results: Vec<serde_json::Value> = stmt
        .query_map(rusqlite::params_from_iter(bound), |row| {
            Ok(serde_json::json!({
                "symbol_name": row.get::<_, String>(0)?,
                "file_path": row.get::<_, String>(1)?,
                "symbol_kind": row.get::<_, String>(2)?,
                "hits": row.get::<_, i64>(3)?,
            }))
        })
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default();
    let count = results.len();
    let mut out =
        serde_json::json!({ "query": query, "tokens": tokens, "results": results, "count": count });
    crate::cli_handlers_index::flag_partial_index(rt, &mut out);
    out.to_string()
}

/// One Tantivy hit in the `cli_search_docs` result shape: the three historical
/// keys (`file_path`, `language`, `context_value`) plus the symbol, kind, line
/// and BM25 score the LIKE scan could never provide.
#[cfg(feature = "tantivy-fts")]
pub(crate) fn tantivy_hit_json(hit: &crate::tantivy_index::SearchHit) -> serde_json::Value {
    serde_json::json!({
        "file_path": hit.file_path,
        "language": serde_json::Value::Null,
        "context_value": hit
            .functional_signature
            .clone()
            .unwrap_or_else(|| hit.symbol_name.clone()),
        "symbol_name": hit.symbol_name,
        "symbol_kind": hit.symbol_kind,
        "line": hit.line_number,
        "score": hit.score,
    })
}

/// Which Tantivy ranking `cli-search-docs` runs. `touring search bm25` and
/// `touring search fuzzy` sent byte-identical payloads to this handler until
/// 2026-09-13 (measured: identical output for every query; `search fuzzy
/// HokRuntime` found nothing while `tantivy fuzzy HokRuntime 2` found
/// `HookRuntime`), so the name promised a tolerance the route never had. The
/// fuzzy route now runs `search_rrf` — BM25 ⊕ edit-distance-2 ⊕ trigram, fused
/// by RRF — which is what the name says.
#[cfg(feature = "tantivy-fts")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DocsMode {
    /// Plain BM25 (`TantivyIndex::search`).
    Bm25,
    /// BM25 ⊕ fuzzy ⊕ trigram via RRF (`TantivyIndex::search_rrf`).
    Fuzzy,
    /// BM25 over document text only (`TantivyIndex::search_text`) — the lane
    /// where memories, rules and skills are found by what they say.
    Text,
    /// The ranking `touring tantivy search` runs
    /// (`TantivyIndex::search_with_community_boost`, no community): names, text,
    /// path words, proximity and file aggregation — the best single ranking
    /// measured (48 of 55 live questions against 27 for the old fusion).
    Ranked,
}

#[cfg(feature = "tantivy-fts")]
impl DocsMode {
    /// `mode` in the payload: `fuzzy` / `rrf` select the fused route; anything
    /// else (or nothing) is BM25, the historical behaviour.
    pub(crate) fn from_payload(payload: &serde_json::Value) -> Self {
        match params::str_or_empty(payload, "mode") {
            "fuzzy" | "rrf" => Self::Fuzzy,
            "text" => Self::Text,
            "ranked" => Self::Ranked,
            _ => Self::Bm25,
        }
    }

    /// The name the response carries.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Bm25 => "bm25",
            Self::Fuzzy => "fuzzy",
            Self::Text => "text",
            Self::Ranked => "ranked",
        }
    }
}

/// BM25 (or the fused fuzzy ranking) over the project's Tantivy index. `None`
/// when the project has no index, the index is empty, or the search itself fails
/// — every one of those falls back to the LIKE scan so the handler never answers
/// `[]` for a reason it could avoid.
#[cfg(feature = "tantivy-fts")]
fn tantivy_docs_search(
    rt: &HookRuntime,
    query: &str,
    top: usize,
    mode: DocsMode,
) -> Option<Vec<serde_json::Value>> {
    let idx = crate::tantivy_index::tantivy_for(Some(&rt.project_root))?;
    if idx.is_empty() {
        return None;
    }
    let searched = match mode {
        DocsMode::Bm25 => idx.search(query, top),
        DocsMode::Fuzzy => idx.search_rrf(query, top),
        DocsMode::Text => idx.search_text(query, top),
        DocsMode::Ranked => idx.search_with_community_boost(query, top, None),
    };
    let hits = match searched {
        Ok(hits) => hits,
        Err(err) => {
            tracing::debug!(query, "{}", tantivy_fallback_reason(&err));
            return None;
        }
    };
    Some(hits.iter().map(tantivy_hit_json).collect())
}

/// Why the BM25 path handed a query back to the LIKE scan. Logged at debug so a
/// `[]` that came from a broken index is diagnosable instead of silent — the
/// same ambiguity `EMPTY_INDEX_MESSAGE` exists to remove for the empty case.
#[cfg(feature = "tantivy-fts")]
pub(crate) fn tantivy_fallback_reason(err: &crate::tantivy_index::TantivyIndexError) -> String {
    format!("tantivy search failed, falling back to the LIKE scan: {err}")
}

/// Full-text search over the project's symbols and documentation.
///
/// Tantivy BM25 (symbol name, signature, docstring) when the project has an
/// index; otherwise the LIKE scan of `file_knowledge.notes` /
/// `file_knowledge.symbols_json`.
///
/// Payload: `{"query": "...", "top": 10, "mode": "bm25" | "fuzzy"}` — see
/// `DocsMode`. The response carries `backend` and `mode`, and `index_state:
/// "partial"` + `remedy` when the latest rebuild did not complete (I16).
pub fn cli_search_docs(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    #[cfg(feature = "tantivy-fts")]
    {
        let query = params::str_or_empty(payload, "query");
        if !query.is_empty() {
            let top = params::i64_or(payload, "top", 10).clamp(1, 100) as usize;
            let mode = DocsMode::from_payload(payload);
            if let Some(results) = tantivy_docs_search(rt, query, top, mode)
                && !results.is_empty()
            {
                let count = results.len();
                let mut out = serde_json::json!({
                    "query": query, "results": results, "count": count,
                    "backend": "tantivy", "mode": mode.as_str()
                });
                crate::cli_handlers_index::flag_partial_index(rt, &mut out);
                return out.to_string();
            }
        }
    }
    let sql = format!(
        "SELECT file_path, language, notes FROM {} \
         WHERE notes LIKE ?1 OR symbols_json LIKE ?1 ORDER BY file_path LIMIT ?2",
        schema_guard::TABLE_FILE_KNOWLEDGE
    );
    like_search(rt, payload, &sql, |row| {
        Ok(serde_json::json!(
            { "file_path" : row.get::< _, String > (0) ?, "language" : row
            .get::< _, Option < String >> (1) ?, "context_value" : row.get::<
            _, Option < String >> (2) ? }
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "tantivy-fts")]
    #[test]
    fn fallback_reason_names_the_index_failure_and_the_route_taken() {
        let err =
            crate::tantivy_index::TantivyIndexError::from("segment file vanished".to_string());
        let reason = tantivy_fallback_reason(&err);
        assert!(
            reason.contains("segment file vanished"),
            "the index's own message survives"
        );
        assert!(reason.contains("LIKE"), "the route the query took is named");
    }

    /// `search bm25` and `search fuzzy` used to be the same route; the payload's
    /// `mode` is what now tells them apart, and the default is the historical BM25.
    #[cfg(feature = "tantivy-fts")]
    #[test]
    fn docs_mode_defaults_to_bm25_and_only_fuzzy_selects_the_fused_route() {
        assert_eq!(
            DocsMode::from_payload(&serde_json::json!({"query": "x"})),
            DocsMode::Bm25
        );
        assert_eq!(
            DocsMode::from_payload(&serde_json::json!({"query": "x", "mode": "bm25"})),
            DocsMode::Bm25
        );
        assert_eq!(
            DocsMode::from_payload(&serde_json::json!({"query": "x", "mode": "fuzzy"})),
            DocsMode::Fuzzy
        );
        assert_eq!(
            DocsMode::from_payload(&serde_json::json!({"query": "x", "mode": "rrf"})),
            DocsMode::Fuzzy
        );
        assert_eq!(
            DocsMode::from_payload(&serde_json::json!({"query": "x", "mode": "nonsense"})),
            DocsMode::Bm25,
            "an unknown mode is the historical route, never an error"
        );
        assert_eq!(
            DocsMode::from_payload(&serde_json::json!({"query": "x", "mode": "text"})),
            DocsMode::Text
        );
        assert_eq!(DocsMode::Text.as_str(), "text");
        assert_eq!(
            DocsMode::from_payload(&serde_json::json!({"query": "x", "mode": "ranked"})),
            DocsMode::Ranked
        );
        assert_eq!(DocsMode::Ranked.as_str(), "ranked");
        assert_eq!(DocsMode::Fuzzy.as_str(), "fuzzy");
        assert_eq!(DocsMode::Bm25.as_str(), "bm25");
    }

    #[test]
    fn prose_query_splits_into_signal_tokens_only() {
        assert_eq!(
            query_tokens("run the gateway pipeline for a tool call"),
            vec!["run", "gateway", "pipeline", "tool", "call"]
        );
    }

    #[test]
    fn identifier_query_is_one_token() {
        assert_eq!(query_tokens("run_gateway"), vec!["run_gateway"]);
        assert_eq!(
            query_tokens("  CapabilityProfile "),
            vec!["CapabilityProfile"]
        );
    }

    #[test]
    fn empty_and_stopword_only_queries_have_no_tokens() {
        assert!(query_tokens("").is_empty());
        assert!(query_tokens("the of a").is_empty());
        assert!(
            query_tokens("x y z").is_empty(),
            "single characters carry no signal"
        );
    }

    #[test]
    fn token_count_is_capped() {
        let q = (0..20)
            .map(|i| format!("tok{i}"))
            .collect::<Vec<_>>()
            .join(" ");
        assert_eq!(query_tokens(&q).len(), MAX_QUERY_TOKENS);
    }

    #[test]
    fn ranked_sql_has_one_clause_per_token_and_limit_after_them() {
        let sql = ranked_like_sql(3, "wiring_map");
        assert_eq!(sql.matches("symbol_name LIKE ?").count(), 3);
        assert!(sql.contains("LIKE ?3 "), "third pattern is bound as ?3");
        assert!(
            sql.ends_with("LIMIT ?4"),
            "limit is the parameter after the patterns"
        );
        assert!(
            sql.contains("HAVING hits > 0"),
            "names matching no token are dropped"
        );
        assert!(
            sql.contains("ORDER BY hits DESC"),
            "more matched tokens rank first"
        );
    }

    #[cfg(feature = "tantivy-fts")]
    #[test]
    fn tantivy_hit_keeps_the_docs_shape_and_adds_position() {
        let hit = crate::tantivy_index::SearchHit {
            symbol_name: "run_gateway".into(),
            file_path: "crates/touring-ceg/src/gateway/pre_exec.rs".into(),
            symbol_kind: "fn".into(),
            line_number: 208,
            score: 12.5,
            crate_name: None,
            visibility: None,
            functional_signature: Some("fn(&Request) -> Decision".into()),
            cognitive_score: None,
            community_id: None,
        };
        let v = tantivy_hit_json(&hit);
        assert_eq!(v["file_path"], "crates/touring-ceg/src/gateway/pre_exec.rs");
        assert!(v["language"].is_null());
        assert_eq!(v["context_value"], "fn(&Request) -> Decision");
        assert_eq!(v["symbol_name"], "run_gateway");
        assert_eq!(v["line"], 208);
        let bare = crate::tantivy_index::SearchHit {
            functional_signature: None,
            ..hit
        };
        assert_eq!(
            tantivy_hit_json(&bare)["context_value"],
            "run_gateway",
            "without a signature the symbol name is the context"
        );
    }
}
