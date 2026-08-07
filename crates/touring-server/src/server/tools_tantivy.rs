//! Tantivy FTS MCP tools — 5 tools exposing full-text search over the symbol index.
//!
//! Tools:
//! - `touring_tantivy_search`  — BM25 ranked search
//! - `touring_tantivy_fuzzy`   — edit-distance fuzzy search
//! - `touring_tantivy_stats`   — index health metrics
//! - `touring_tantivy_suggest` — autocomplete prefix suggestions
//! - `touring_tantivy_reindex` — clear + reset the Tantivy index
//!
//! All tools are backed by the `TantivyIndex` singleton from `touring-hooks`
//! (feature `tantivy-fts`). When the singleton is unavailable (not initialized,
//! disk error), tools return a JSON error object rather than failing the MCP call.

use super::*;

#[tool_router(router = router_tantivy, vis = "pub(crate)")]
impl TouringServer {
    /// BM25 full-text search over symbols.
    ///
    /// Queries are matched against `symbol_name`, `docstring`, `module_path`, and
    /// `functional_signature`. Results are ranked by BM25 score descending.
    #[tool(
        annotations(read_only_hint = true, title = "Full-text symbol search"),
        name = "touring_tantivy_search",
        description = "BM25 full-text search over the symbol index. \
                       Returns ranked hits with symbol_name, file_path, kind, score. \
                       Faster than SQL FTS5 for large corpora."
    )]
    pub(crate) async fn tantivy_search(
        &self,
        params: Parameters<TantivySearchParams>,
    ) -> Result<CallToolResult, McpError> {
        let p = params.0;
        let query = p.query.clone();
        let limit = p.limit.unwrap_or(10);

        // A raiz vem do config do servidor; o closure e `move`, entao clonamos
        // antes. Ate 03/08/2026 estas tools liam o indice LEGADO global,
        // servindo simbolos de outros projetos a esta sessao MCP.
        let project_root = self.config.project_root.clone();
        let result = tokio::task::spawn_blocking(move || {
            let Some(idx) = touring_hooks::tantivy_index::tantivy_for(Some(&project_root)) else {
                return serde_json::json!({"error": "tantivy index unavailable"});
            };
            // F2: um índice sem documentos responde a CONDIÇÃO, não um `[]` que
            // o cliente leria como "esse símbolo não existe".
            if idx.is_empty() {
                return serde_json::json!({
                    "error": touring_hooks::tantivy_index::EMPTY_INDEX_MESSAGE,
                    "total_docs": 0,
                });
            }
            match idx.search(&query, limit) {
                Ok(hits) => serde_json::to_value(&hits)
                    .unwrap_or_else(|e| serde_json::json!({"error": format!("serialize: {e}")})),
                Err(e) => serde_json::json!({"error": e}),
            }
        })
        .await
        .unwrap_or_else(|e| serde_json::json!({"error": format!("spawn_blocking: {e}")}));

        let text = serde_json::to_string_pretty(&result)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    /// Fuzzy full-text search with Levenshtein edit-distance tolerance.
    ///
    /// Useful for typo-tolerant symbol lookup. `distance` defaults to 2.
    #[tool(
        annotations(read_only_hint = true, title = "Fuzzy symbol search"),
        name = "touring_tantivy_fuzzy",
        description = "Fuzzy search over symbols with Levenshtein edit-distance tolerance. \
                       Use when exact BM25 search misses due to typos or partial names."
    )]
    pub(crate) async fn tantivy_fuzzy(
        &self,
        params: Parameters<TantivyFuzzyParams>,
    ) -> Result<CallToolResult, McpError> {
        let p = params.0;
        let query = p.query.clone();
        let distance = p.distance.unwrap_or(2);
        let limit = p.limit.unwrap_or(10);

        // A raiz vem do config do servidor; o closure e `move`, entao clonamos
        // antes. Ate 03/08/2026 estas tools liam o indice LEGADO global,
        // servindo simbolos de outros projetos a esta sessao MCP.
        let project_root = self.config.project_root.clone();
        let result = tokio::task::spawn_blocking(move || {
            let Some(idx) = touring_hooks::tantivy_index::tantivy_for(Some(&project_root)) else {
                return serde_json::json!({"error": "tantivy index unavailable"});
            };
            if idx.is_empty() {
                return serde_json::json!({
                    "error": touring_hooks::tantivy_index::EMPTY_INDEX_MESSAGE,
                    "total_docs": 0,
                });
            }
            match idx.fuzzy_search(&query, distance, limit) {
                Ok(hits) => serde_json::to_value(&hits)
                    .unwrap_or_else(|e| serde_json::json!({"error": format!("serialize: {e}")})),
                Err(e) => serde_json::json!({"error": e}),
            }
        })
        .await
        .unwrap_or_else(|e| serde_json::json!({"error": format!("spawn_blocking: {e}")}));

        let text = serde_json::to_string_pretty(&result)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    /// Snapshot of Tantivy index health metrics.
    ///
    /// Returns `total_docs`, `index_size_bytes`, `pending_ops`, commit counters.
    #[tool(
        name = "touring_tantivy_stats",
        description = "Snapshot of Tantivy FTS index health: total_docs, index_size_bytes, \
                       pending_ops, total_commits, total_upserts."
    )]
    pub(crate) async fn tantivy_stats(
        &self,
        _params: Parameters<TantivyStatsParams>,
    ) -> Result<CallToolResult, McpError> {
        // A raiz vem do config do servidor; o closure e `move`, entao clonamos
        // antes. Ate 03/08/2026 estas tools liam o indice LEGADO global,
        // servindo simbolos de outros projetos a esta sessao MCP.
        let project_root = self.config.project_root.clone();
        let result = tokio::task::spawn_blocking(move || {
            let Some(idx) = touring_hooks::tantivy_index::tantivy_for(Some(&project_root)) else {
                return serde_json::json!({"error": "tantivy index unavailable"});
            };
            serde_json::to_value(idx.stats())
                .unwrap_or_else(|e| serde_json::json!({"error": format!("serialize: {e}")}))
        })
        .await
        .unwrap_or_else(|e| serde_json::json!({"error": format!("spawn_blocking: {e}")}));

        let text = serde_json::to_string_pretty(&result)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    /// Autocomplete prefix suggestions from the symbol index.
    ///
    /// Returns symbol names matching the given prefix, ordered by BM25 score.
    #[tool(
        name = "touring_tantivy_suggest",
        description = "Autocomplete prefix suggestions from the symbol index. \
                       Returns symbol names whose lowercased form starts with `prefix`."
    )]
    pub(crate) async fn tantivy_suggest(
        &self,
        params: Parameters<TantivySuggestParams>,
    ) -> Result<CallToolResult, McpError> {
        let p = params.0;
        let prefix = p.prefix.clone();
        let limit = p.limit.unwrap_or(10);

        // A raiz vem do config do servidor; o closure e `move`, entao clonamos
        // antes. Ate 03/08/2026 estas tools liam o indice LEGADO global,
        // servindo simbolos de outros projetos a esta sessao MCP.
        let project_root = self.config.project_root.clone();
        let result = tokio::task::spawn_blocking(move || {
            let Some(idx) = touring_hooks::tantivy_index::tantivy_for(Some(&project_root)) else {
                return serde_json::json!({"error": "tantivy index unavailable"});
            };
            match idx.suggest(&prefix, limit) {
                Ok(hits) => serde_json::to_value(&hits)
                    .unwrap_or_else(|e| serde_json::json!({"error": format!("serialize: {e}")})),
                Err(e) => serde_json::json!({"error": e}),
            }
        })
        .await
        .unwrap_or_else(|e| serde_json::json!({"error": format!("spawn_blocking: {e}")}));

        let text = serde_json::to_string_pretty(&result)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    /// Clear and reset the Tantivy FTS index.
    ///
    /// Deletes all documents and commits an empty index. The daemon will repopulate
    /// incrementally via `post-write` and `post-edit` hooks as files are processed.
    ///
    /// Use this after schema migrations or to recover from index corruption.
    /// Returns `IndexStats` after the reset completes.
    #[tool(
        name = "touring_tantivy_reindex",
        description = "Clear and reset the Tantivy FTS index. Removes all documents and commits. \
                       The daemon repopulates incrementally via hooks. \
                       Use after schema migration or index corruption."
    )]
    pub(crate) async fn tantivy_reindex(
        &self,
        _params: Parameters<TantivyReindexParams>,
    ) -> Result<CallToolResult, McpError> {
        // A raiz vem do config do servidor; o closure e `move`, entao clonamos
        // antes. Ate 03/08/2026 estas tools liam o indice LEGADO global,
        // servindo simbolos de outros projetos a esta sessao MCP.
        let project_root = self.config.project_root.clone();
        let result = tokio::task::spawn_blocking(move || {
            let Some(idx) = touring_hooks::tantivy_index::tantivy_for(Some(&project_root)) else {
                return serde_json::json!({"error": "tantivy index unavailable"});
            };
            // Reindex with an empty symbol list — clears all documents and commits.
            // The daemon hooks (post-write/post-edit) will repopulate incrementally.
            match idx.reindex(vec![]) {
                Ok(stats) => serde_json::to_value(stats)
                    .unwrap_or_else(|e| serde_json::json!({"error": format!("serialize: {e}")})),
                Err(e) => serde_json::json!({"error": e}),
            }
        })
        .await
        .unwrap_or_else(|e| serde_json::json!({"error": format!("spawn_blocking: {e}")}));

        let text = serde_json::to_string_pretty(&result)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }
}
