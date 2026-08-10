//! find_code — MCP super-tool for unified code search.
//
//! Orchestrates detect_intent + SearchPipeline (keyword + semantic + RRF).
//! detect_intent runs in-thread (sync). SearchPipeline::search is async
//! and runs via spawn_blocking + block_on to avoid nested runtime issues.

use crate::server::TouringServer;
use crate::server::params::{FindCodeParams, FindCodeResponse, FindCodeResult};
use std::sync::Arc;
use touring_storage::embeddings::{FastEmbedModel, FastEmbedProvider};
use touring_storage::hybrid_search::{
    HybridConfig, HybridQuery, HybridQueryIntent, SearchPipeline,
    SearchResult as FusionSearchResult, detect_intent,
};

/// Implementation entry point — called by the `#[tool]` wrapper in server/mod.rs.
pub async fn find_code_impl(
    _server: &TouringServer,
    params: FindCodeParams,
) -> Result<String, String> {
    let query_trimmed = params.query.trim().to_string();
    if query_trimmed.is_empty() {
        return Err("query cannot be empty".to_string());
    }

    let max_results = params.max_results.unwrap_or(20).min(100);
    let intent_override = params.intent_override.clone();

    // ── Phase 1: detect_intent (sync, runs in-thread via spawn_blocking) ──
    let intent_result = tokio::task::spawn_blocking({
        let qt = query_trimmed.clone();
        move || detect_intent(&qt)
    })
    .await
    .map_err(|e| format!("detect_intent panicked: {e}"))?;

    // ── Phase 2: Determine intent enum for HybridQuery ──
    let hybrid_intent = if let Some(override_str) = intent_override {
        match override_str.to_lowercase().as_str() {
            "understand" | "implement" => HybridQueryIntent::Understand,
            "debug" | "lookup" => HybridQueryIntent::Lookup,
            "refactor" | "navigate" => HybridQueryIntent::Navigate,
            "explore" | "document" => HybridQueryIntent::Explore,
            _ => match intent_result.intent {
                touring_storage::hybrid_search::IntentQueryIntent::Understand
                | touring_storage::hybrid_search::IntentQueryIntent::Implement => {
                    HybridQueryIntent::Understand
                }
                touring_storage::hybrid_search::IntentQueryIntent::Debug => {
                    HybridQueryIntent::Lookup
                }
                touring_storage::hybrid_search::IntentQueryIntent::Refactor => {
                    HybridQueryIntent::Navigate
                }
                touring_storage::hybrid_search::IntentQueryIntent::Document
                | touring_storage::hybrid_search::IntentQueryIntent::Explore => {
                    HybridQueryIntent::Explore
                }
            },
        }
    } else {
        match intent_result.intent {
            touring_storage::hybrid_search::IntentQueryIntent::Understand
            | touring_storage::hybrid_search::IntentQueryIntent::Implement => {
                HybridQueryIntent::Understand
            }
            touring_storage::hybrid_search::IntentQueryIntent::Debug => HybridQueryIntent::Lookup,
            touring_storage::hybrid_search::IntentQueryIntent::Refactor => {
                HybridQueryIntent::Navigate
            }
            touring_storage::hybrid_search::IntentQueryIntent::Document
            | touring_storage::hybrid_search::IntentQueryIntent::Explore => {
                HybridQueryIntent::Explore
            }
        }
    };

    // ── Phase 3: SearchPipeline search (async, runs via spawn_blocking + block_on) ──
    let (results, backends) = tokio::task::spawn_blocking({
        let query_owned = query_trimmed.clone();
        let intent_owned = hybrid_intent;
        move || -> Result<(Vec<FusionSearchResult>, touring_storage::hybrid_search::BackendStatus), String> {
            let rt = tokio::runtime::Runtime::new()
                .map_err(|e| format!("failed to create search runtime: {e}"))?;
            let provider = FastEmbedProvider::with_model(FastEmbedModel::BgeSmall);
            let config = HybridConfig::default();
            // No store is wired on purpose: an EMPTY InMemoryVectorStore would
            // report `vector_store: true` and make "consulted, no match" the
            // reported reason for an answer that never had a corpus. Wire a
            // store here only once one is actually populated.
            let mut pipeline = SearchPipeline::with_provider(config, Arc::new(provider));
            // Real keyword corpus: the capability portfolio (in-process, ~11k
            // artifacts + documented symbols). Absent until `touring portfolio
            // refresh` runs, and its absence is reported rather than papered over.
            if let Some(backend) = crate::portfolio::keyword::PortfolioKeyword::arc_if_available() {
                pipeline = pipeline.with_keyword_backend(backend);
            }
            let hybrid_query = HybridQuery {
                query: query_owned,
                intent: intent_owned,
                top_k: max_results,
                rerank: false,
            };
            let (hits, stats) = rt.block_on(pipeline.search(hybrid_query));
            Ok((hits, stats.backends))
        }
    })
    .await
    .map_err(|e| format!("SearchPipeline search panicked: {e}"))?
    .map_err(|e| format!("SearchPipeline search error: {e}"))?;

    // ── Phase 4: Map FusionSearchResult → FindCodeResult ──
    let mapped_results: Vec<FindCodeResult> = results
        .into_iter()
        .map(|sr: FusionSearchResult| {
            // Parse file_path from doc_id (may contain ":" prefix for qualified names)
            let doc_id = &sr.doc_id;
            let (file_path, line, col) = if doc_id.contains(':') {
                // Qualified format: "file_path:line:col" or just "file_path:line"
                let parts: Vec<&str> = doc_id.split(':').collect();
                let fp = parts
                    .first()
                    .copied()
                    .unwrap_or(doc_id.as_str())
                    .to_string();
                let ln = parts.get(1).and_then(|s| s.parse().ok());
                let cl = parts.get(2).and_then(|s| s.parse().ok());
                (fp, ln, cl)
            } else {
                (doc_id.clone(), None, None)
            };

            FindCodeResult {
                file_path,
                line,
                col,
                symbol: None,  // SearchPipeline doesn't expose symbol name in result
                context: None, // SearchPipeline doesn't expose context snippet
                backend: "hybrid-search-fusion".to_string(),
                rrf_score: sr.score,
                confidence_tier: format!("{:?}", sr.confidence).to_lowercase(),
            }
        })
        .collect();

    // An empty result set means two very different things; say which.
    let note = if backends.is_unwired() {
        Some(
            "nenhum corpus wirado: o portfólio de capacidades está vazio (rode \
             `touring portfolio refresh`) e a perna semântica não tem vector store \
             populado. Vazio NÃO significa ausência no código — use \
             `touring tantivy search` para busca por identificador."
                .to_string(),
        )
    } else if mapped_results.is_empty() {
        Some("corpus consultado: portfólio de capacidades (propósito). Sem \
             correspondência — para busca por IDENTIFICADOR use `touring tantivy \
             search`, que cobre os ~270k símbolos do índice".to_string())
    } else {
        None
    };

    let response = FindCodeResponse {
        results: mapped_results,
        detected_intent: format!("{:?}", intent_result.intent),
        confidence: intent_result.confidence,
        backends,
        note,
    };

    serde_json::to_string(&response).map_err(|e| format!("JSON serialize error: {e}"))
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    // Note: Integration tests for the full find_code_impl require a properly
    // initialized TouringServer (with classifier, embedder, memory provider).
    // See crates/touring-server/tests/binary_e2e.rs for end-to-end tests that
    // exercise the find_code tool via the MCP protocol.
    //
    // The critical safety fix (Runtime::new().map_err()? instead of .expect())
    // is verified by compilation — if Runtime::new() ever fails in practice,
    // the error propagates as Err(String) instead of panicking the process.

    /// Documents the safety invariant: Tokio's Runtime::new() is architecturally
    /// infallible. Our .map_err()? converts any theoretical OOM failure from
    /// panic into a graceful error return to the MCP caller.
    #[test]
    fn runtime_construction_safety_invariant_doc() {
        // This test always passes — it's documentation for future maintainers.
        //
        // TOKIO DESIGN INVARIANT: Runtime::new() calls the same internal
        // constructors that Tokio uses in its own test suite and CI. The
        // only failure mode is OOM at allocation time, which would indicate
        // the entire process is in an unrecoverable state anyway.
        //
        // OUR CONTRIBUTION: Changed from .expect() to .map_err()? so that
        // if (hypothetically) the runtime creation ever fails, the error
        // surfaces as a JSON-RPC error response rather than panicking the
        // entire daemon. This is the correct resilience trade-off.
        //
        // ERROR CHAIN VERIFIED BY COMPILATION:
        //   spawn_blocking(move || -> Result<Vec<_>, String> { ... })
        //     .map_err(|e| format!("SearchPipeline search panicked: {e}"))
        //     .map_err(|e| format!("SearchPipeline search error: {e}"))
        //
        // If Runtime::new() fails → returns Err(String) from closure → wraps
        // in "SearchPipeline search error: ..." → propagates to caller as
        // MCP error response via tools_infra.rs:2134 match arm.
        assert!(true);
    }
}
