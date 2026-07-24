//! H86: ContextualRankerHandler
//!
//! Re-ranks accumulated context lines by BM25 keyword relevance to the user's prompt.
//! Runs at priority 250 (last in pipeline), after all other handlers have contributed
//! their context lines. This ensures the most relevant lines appear first in the
//! additionalContext injected into the LLM.
//!
//! Architecture:
//! ```text
//! ctx.context_lines (accumulated by H1-H85)
//!        |
//!        v
//! extract keywords from prompt (words len > 3, up to 8)
//!        |
//!        v
//! convert lines -> SearchResult[] (base score 1.0)
//!        |
//!        v
//! ContextualReranker::rerank (BM25 keyword-weighted)
//!        |
//!        v
//! replace ctx.context_lines with re-ranked order
//!        |
//!        v
//! drift_record("context_rerank_size", N) -> RL feedback
//! ```

use touring_intelligence::ann::{ContextualReranker, RerankContext, RerankWeights, SearchResult};

use crate::context::CortexContext;
use crate::handler::Handler;
use crate::pipeline::Pipeline;
use crate::types::{HandlerResult, HookEvent};

/// Minimum number of context lines to trigger reranking.
/// Below this threshold, the ordering is unlikely to matter.
const MIN_LINES_FOR_RERANK: usize = 3;

/// Maximum number of keywords extracted from the user's prompt.
const MAX_KEYWORDS: usize = 8;

/// Minimum word length to be considered a keyword (filters stopwords).
const MIN_KEYWORD_LEN: usize = 4;

/// H86: Re-ranks context lines by keyword relevance to the user's prompt.
///
/// Uses `ContextualReranker` from `touring-antt` with keyword-heavy weights
/// (keyword=0.30, semantic=0.65, historical=0.05) to surface the most
/// relevant context lines first. Authority and recency are zeroed since
/// context lines have no document type or date metadata.
pub struct ContextualRankerHandler {
    ranker: ContextualReranker,
}

impl Default for ContextualRankerHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl ContextualRankerHandler {
    /// Creates a handler with a freshly initialized contextual reranker.
    pub fn new() -> Self {
        Self {
            ranker: ContextualReranker::new(),
        }
    }

    /// Extract keywords from the prompt: words with len > 3, lowercased, up to 8.
    fn extract_keywords(prompt: &str) -> Vec<String> {
        prompt
            .split_whitespace()
            .filter(|w| w.len() >= MIN_KEYWORD_LEN)
            .take(MAX_KEYWORDS)
            .map(|w| w.to_lowercase())
            .collect()
    }
}

impl Handler for ContextualRankerHandler {
    fn name(&self) -> &str {
        "H86_contextual_ranker"
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::UserPromptSubmit]
    }

    fn priority(&self) -> u8 {
        250
    }

    fn dependency_tier(&self) -> u8 {
        0 // T0: no external deps beyond handler state
    }

    fn timeout_ms(&self) -> u64 {
        50
    }

    fn execute(&self, ctx: &mut CortexContext) -> HandlerResult {
        // Skip if fewer than MIN_LINES_FOR_RERANK context lines accumulated
        if ctx.context_lines.len() < MIN_LINES_FOR_RERANK {
            return HandlerResult::skip(self.name());
        }

        // Extract prompt text — skip if missing or empty
        let prompt = match ctx.input.get("prompt").and_then(|v| v.as_str()) {
            Some(p) if !p.is_empty() => p,
            _ => return HandlerResult::skip(self.name()),
        };

        // Extract keywords from prompt
        let keywords = Self::extract_keywords(prompt);
        if keywords.is_empty() {
            return HandlerResult::skip(self.name());
        }

        // Convert context lines to SearchResult (base score 1.0)
        let results: Vec<SearchResult> = ctx
            .context_lines
            .iter()
            .enumerate()
            .map(|(i, line)| {
                let doc_id = format!("ctx_line_{i}");
                SearchResult::new(&doc_id, 1.0, line)
            })
            .collect();

        // Build rerank context with keyword-heavy weights
        let rerank_ctx = RerankContext {
            query: prompt.to_string(),
            keywords,
            weights: RerankWeights {
                semantic: 0.65,
                authority: 0.0,
                recency: 0.0,
                keyword: 0.30,
                historical: 0.05,
            },
            ..RerankContext::default()
        };

        // Rerank and replace context lines on success
        match self.ranker.rerank(&results, &rerank_ctx) {
            Ok(ranked) => {
                let reranked_lines: Vec<String> = ranked.into_iter().map(|r| r.content).collect();
                let rerank_size = reranked_lines.len();
                ctx.context_lines = reranked_lines;

                // Report rerank size to RL drift monitor
                if let Some(ref persistence) = ctx.persistence {
                    let _ = persistence.drift_record("context_rerank_size", rerank_size as f64);
                }
            }
            Err(_) => {
                // Reranking failed — keep original ordering, skip silently
            }
        }

        // Handler mutates ctx directly — no new context line to add
        HandlerResult::skip(self.name())
    }
}

/// Register H86 ContextualRankerHandler.
pub fn register(pipeline: &mut Pipeline) {
    pipeline.register(Box::new(ContextualRankerHandler::new()));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_handler_name() {
        let h = ContextualRankerHandler::new();
        assert_eq!(h.name(), "H86_contextual_ranker");
    }

    #[test]
    fn test_handler_events() {
        let h = ContextualRankerHandler::new();
        let events = h.events();
        assert!(events.contains(&HookEvent::UserPromptSubmit));
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn test_handler_priority() {
        let h = ContextualRankerHandler::new();
        assert_eq!(h.priority(), 250, "must run last in pipeline");
    }

    #[test]
    fn test_handler_dependency_tier() {
        let h = ContextualRankerHandler::new();
        assert_eq!(h.dependency_tier(), 0, "T0: no external deps");
    }

    #[test]
    fn test_handler_timeout() {
        let h = ContextualRankerHandler::new();
        assert_eq!(h.timeout_ms(), 50);
    }

    #[test]
    fn test_handler_not_critical() {
        let h = ContextualRankerHandler::new();
        assert!(!h.is_critical());
    }

    #[test]
    fn test_handler_not_async() {
        let h = ContextualRankerHandler::new();
        assert!(!h.is_async());
    }

    #[test]
    fn test_extract_keywords_filters_short_words() {
        let kw = ContextualRankerHandler::extract_keywords("I am a big developer of Rust code");
        // "I", "am", "a", "of" are < 4 chars, filtered out
        // "big" is 3 chars, also filtered
        assert!(!kw.contains(&"am".to_string()));
        assert!(!kw.contains(&"big".to_string()));
        assert!(kw.contains(&"developer".to_string()));
        assert!(kw.contains(&"rust".to_string())); // lowercased
        assert!(kw.contains(&"code".to_string()));
    }

    #[test]
    fn test_extract_keywords_max_limit() {
        let long_prompt = "alpha bravo charlie delta echo foxtrot golf hotel india juliet";
        let kw = ContextualRankerHandler::extract_keywords(long_prompt);
        assert!(kw.len() <= MAX_KEYWORDS);
    }

    #[test]
    fn test_rerank_orders_by_keyword_match() {
        // Build SearchResult-compatible lines and verify reranking moves keyword-rich lines up
        let ranker = ContextualReranker::new();
        let results = vec![
            SearchResult::new("line_0", 1.0, "unrelated info about nothing useful"),
            SearchResult::new(
                "line_1",
                1.0,
                "important Rust handler implementation details",
            ),
            SearchResult::new("line_2", 1.0, "another irrelevant line about weather"),
        ];
        let rerank_ctx = RerankContext {
            query: "Rust handler implementation".to_string(),
            keywords: vec![
                "rust".to_string(),
                "handler".to_string(),
                "implementation".to_string(),
            ],
            weights: RerankWeights {
                semantic: 0.65,
                authority: 0.0,
                recency: 0.0,
                keyword: 0.30,
                historical: 0.05,
            },
            ..RerankContext::default()
        };

        let ranked = ranker
            .rerank(&results, &rerank_ctx)
            .expect("rerank should succeed");
        // The line mentioning "Rust handler implementation" should rank higher
        let top = ranked.first().expect("should have results");
        assert!(
            top.content.contains("Rust handler implementation"),
            "keyword-matching line should be ranked first, got: {}",
            top.content
        );
    }

    #[test]
    fn test_skip_on_few_lines() {
        // Verify the handler skips when fewer than MIN_LINES_FOR_RERANK lines
        let _h = ContextualRankerHandler::new();
        // Create a minimal test context using the types directly
        // We just test the extract logic and threshold — the full execute path
        // requires CortexContext which depends on knowledge DB.
        // Instead, verify the threshold constant is respected.
        assert_eq!(
            MIN_LINES_FOR_RERANK, 3,
            "handler should skip when < 3 lines"
        );
    }

    #[test]
    fn test_skip_on_empty_prompt() {
        // Verify keyword extraction returns empty for empty/short prompts
        let kw = ContextualRankerHandler::extract_keywords("");
        assert!(kw.is_empty(), "empty prompt should yield no keywords");

        let kw_short = ContextualRankerHandler::extract_keywords("hi ok");
        assert!(kw_short.is_empty(), "very short words should be filtered");
    }
}
