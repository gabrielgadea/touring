//! `touring find-code` — CLI wrapper around the find_code MCP implementation.
//!
//! Provides unified code search via hybrid semantic + keyword fusion.
//! Calls the same SearchPipeline as the MCP tool, exposed as a CLI command.

use crate::server::params::{FindCodeResponse, FindCodeResult};
use std::sync::Arc;
use touring_storage::embeddings::{FastEmbedModel, FastEmbedProvider};
use touring_storage::hybrid_search::{
    HybridConfig, HybridQuery, HybridQueryIntent, SearchPipeline,
    SearchResult as FusionSearchResult, detect_intent,
};
use touring_storage::vec::InMemoryVectorStore;

/// Entry point for the `touring find-code search` CLI handler — runs hybrid
/// semantic + keyword fusion search over the same `SearchPipeline` the
/// `find_code` MCP tool uses. Parses `<query>` plus `--limit N` (capped at
/// 100) and `--intent understand|debug|refactor|explore`, executes the search
/// on a dedicated thread, and prints the JSON [`FindCodeResponse`].
pub fn run(args: &[String]) -> anyhow::Result<()> {
    let subcommand = args.get(2).map(|s| s.as_str()).unwrap_or("search");
    if subcommand != "search" {
        anyhow::bail!(
            "Unknown find-code subcommand: '{}'. Use: touring find-code search <query>",
            subcommand
        );
    }

    let query = args.get(3).map(|s| s.as_str()).unwrap_or("");
    if query.is_empty() {
        eprintln!(
            "Usage: touring find-code search <query> [--limit N] [--intent understand|debug|refactor|explore]"
        );
        return Ok(());
    }

    let limit = args
        .iter()
        .position(|a| a == "--limit")
        .and_then(|i| args.get(i + 1)?.parse().ok())
        .unwrap_or(20);

    let intent_override: Option<String> = args
        .iter()
        .position(|a| a == "--intent")
        .and_then(|i| args.get(i + 1).map(|s| s.as_str()).map(String::from));

    // Execute the same logic as find_code_impl
    let result = std::thread::spawn({
        let query = query.to_string();
        let intent_override = intent_override.clone();
        let limit = limit.min(100);
        move || -> Result<String, String> {
            // detect_intent (sync)
            let intent_result = detect_intent(&query);

            let hybrid_intent = if let Some(ref override_str) = intent_override {
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
                        _ => HybridQueryIntent::Explore,
                    },
                }
            } else {
                match intent_result.intent {
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
                    _ => HybridQueryIntent::Explore,
                }
            };

            let rt = tokio::runtime::Runtime::new().map_err(|e| format!("runtime: {}", e))?;
            let provider = FastEmbedProvider::with_model(FastEmbedModel::BgeSmall);
            let store = Arc::new(InMemoryVectorStore::default());
            let config = HybridConfig::default();
            let pipeline =
                SearchPipeline::with_provider_and_store(config, Arc::new(provider), store);
            let hybrid_query = HybridQuery {
                query: query.clone(),
                intent: hybrid_intent,
                top_k: limit,
                rerank: false,
            };

            let results = rt.block_on(pipeline.search(hybrid_query)).0;

            let mapped: Vec<FindCodeResult> = results
                .into_iter()
                .map(|sr: FusionSearchResult| {
                    let doc_id = &sr.doc_id;
                    let (file_path, line, col) = if doc_id.contains(':') {
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
                        symbol: None,
                        context: None,
                        backend: "hybrid-search-fusion".to_string(),
                        rrf_score: sr.score,
                        confidence_tier: format!("{:?}", sr.confidence).to_lowercase(),
                    }
                })
                .collect();

            let response = FindCodeResponse {
                results: mapped,
                detected_intent: format!("{:?}", intent_result.intent),
                confidence: 0.85,
            };
            serde_json::to_string(&response).map_err(|e| format!("json: {}", e))
        }
    })
    .join()
    .expect("find-code search panicked");

    match result {
        Ok(json) => println!("{}", json),
        Err(e) => eprintln!("find-code error: {}", e),
    }
    Ok(())
}
