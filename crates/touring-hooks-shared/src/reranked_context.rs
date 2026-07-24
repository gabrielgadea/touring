//! Reranked context injection using touring-antt's ContextualReranker.
//!
//! Enhances the basic scored-signals approach in `pre_read` with
//! Reciprocal Rank Fusion (RRF) and authority-weighted reranking.
//! Active only when the `nlp-enrichment` feature is enabled.

#[cfg(feature = "nlp-enrichment")]
use touring_intelligence::ann::reranker::{ContextualReranker, RerankContext, SearchResult};

/// Rerank pre-read context signals using RRF authority weights.
///
/// Takes the raw scored signals from `compose_high_signal_context_budgeted`
/// and reranks them using the ContextualReranker for better signal ordering.
#[cfg(feature = "nlp-enrichment")]
pub fn rerank_context_signals(
    signals: &[(f32, String)],
    file_path: &str,
    max_results: usize,
) -> Vec<(f32, String)> {
    if signals.is_empty() {
        return vec![];
    }

    let reranker = ContextualReranker::default();

    let search_results: Vec<SearchResult> = signals
        .iter()
        .enumerate()
        .map(|(i, (score, text))| {
            SearchResult::new(&format!("signal_{i}"), f64::from(*score), text)
        })
        .collect();

    let context = RerankContext {
        query: file_path.to_string(),
        ..RerankContext::default()
    };

    match reranker.rerank(&search_results, &context) {
        Ok(ranked) => ranked
            .iter()
            .take(max_results)
            .map(|r| (r.reranked_score as f32, r.content.clone()))
            .collect(),
        Err(_) => signals.iter().take(max_results).cloned().collect(),
    }
}

/// No-op fallback when nlp-enrichment feature is disabled.
#[cfg(not(feature = "nlp-enrichment"))]
pub fn rerank_context_signals(
    signals: &[(f32, String)],
    _file_path: &str,
    max_results: usize,
) -> Vec<(f32, String)> {
    signals.iter().take(max_results).cloned().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rerank_empty_signals() {
        let result = rerank_context_signals(&[], "test.rs", 5);
        assert!(result.is_empty());
    }

    #[test]
    fn test_rerank_preserves_signals() {
        let signals = vec![
            (2.0, "gotcha: use ? instead of unwrap".to_string()),
            (1.5, "bash failure: cargo test panicked".to_string()),
            (1.0, "3 dependents: main.rs, lib.rs, mod.rs".to_string()),
        ];
        let result = rerank_context_signals(&signals, "src/handler.rs", 5);
        assert!(!result.is_empty());
        assert!(result.len() <= 3);
    }

    #[test]
    fn test_rerank_respects_max_results() {
        let signals: Vec<(f32, String)> =
            (0..10).map(|i| (i as f32, format!("signal {i}"))).collect();
        let result = rerank_context_signals(&signals, "test.py", 3);
        assert!(result.len() <= 3);
    }
}
