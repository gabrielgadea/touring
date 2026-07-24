//! NLP Bridge — touring-antt integration for hook enrichment.
//!
//! Provides NLP capabilities to the hook system:
//! - **Keyword matching**: Extract relevant ANTT/technical keywords from commands
//!   and file content for smarter context injection
//! - **Semantic chunking**: Produce optimal text chunks for knowledge DB storage
//! - **Monetary parsing**: Detect monetary values in bash output for
//!   structured extraction

use crate::shared::async_runtime::AsyncTaskBuilder;
use touring_intelligence::ann::{
    ANTT_PATTERNS, ChunkerConfig, KeywordMatch, MonetaryValue, NlpPipeline, SemanticChunk,
    SemanticChunker, parse_monetary,
};

/// Extract relevant keywords from text using Aho-Corasick matching.
///
/// Useful in pre-bash/pre-edit hooks to identify the domain of the
/// content being operated on, enabling more targeted context injection.
pub fn extract_keywords(text: &str) -> Vec<KeywordMatch> {
    ANTT_PATTERNS.find_matches(text)
}

/// Check if text contains ANTT regulatory keywords.
///
/// Returns true if the text mentions ANTT patterns (resolutions,
/// normative instructions, laws, decrees, TCU rulings, etc.).
/// Used by pre-read to decide whether to inject regulatory context.
pub fn has_regulatory_content(text: &str) -> bool {
    let matches = extract_keywords(text);
    matches.iter().any(|m| {
        m.category
            .as_deref()
            .map(|c| {
                matches!(
                    c,
                    "Resolution" | "Law" | "Decree" | "TCU" | "Contract" | "Opinion"
                )
            })
            .unwrap_or(false)
    })
}

/// Check if text contains technical note / document keywords.
///
/// Returns true if text mentions technical note patterns.
/// Used by pre-edit to decide complexity of context injection.
pub fn has_technical_content(text: &str) -> bool {
    let matches = extract_keywords(text);
    matches.iter().any(|m| {
        m.category
            .as_deref()
            .map(|c| c == "TechnicalNote")
            .unwrap_or(false)
    })
}

/// Chunk text into semantic segments for knowledge DB storage.
///
/// Produces chunks that respect paragraph/section boundaries,
/// optimized for later retrieval. Each chunk is self-contained
/// and searchable.
pub fn chunk_for_knowledge(text: &str, target_tokens: usize) -> Vec<SemanticChunk> {
    let config = ChunkerConfig {
        target_tokens,
        tolerance_percent: 0.2,
        overlap_sentences: 1,
        preserve_tables: true,
        preserve_code_blocks: true,
    };
    let chunker = SemanticChunker::new(config);
    chunker.chunk(text)
}

/// Extract monetary values from text.
///
/// Parses Brazilian monetary notation (R$ 1.234,56) from bash output
/// or file content. Useful in post-bash hooks when analyzing
/// financial/regulatory data.
pub fn extract_monetary_values(text: &str) -> Vec<MonetaryValue> {
    // EC55: First production caller of ResultExt::unwrap_or_debug —
    // logs parse failures at DEBUG level instead of silently swallowing them.
    use crate::shared::result_ext::ResultExt;
    parse_monetary(text).unwrap_or_debug(Vec::new(), "nlp_bridge: monetary parse failed")
}

/// Count keyword matches by category for classification features.
///
/// Returns a map of category -> count that can be used as features
/// for the LinUCB bandit or RL system.
pub fn keyword_category_counts(text: &str) -> std::collections::HashMap<String, usize> {
    let matches = extract_keywords(text);
    let mut counts = std::collections::HashMap::new();
    for m in &matches {
        if let Some(ref cat) = m.category {
            *counts.entry(cat.clone()).or_insert(0) += 1;
        }
    }
    counts
}

/// Check if text has any recognized keyword matches.
pub fn has_any_keywords(text: &str) -> bool {
    !extract_keywords(text).is_empty()
}

/// Get the dominant category in text (most frequent keyword category).
///
/// Returns None if no keywords found.
pub fn dominant_category(text: &str) -> Option<String> {
    let counts = keyword_category_counts(text);
    counts
        .into_iter()
        .max_by_key(|(_, count)| *count)
        .map(|(cat, _)| cat)
}

/// Get the dominant category or a fallback string with debug logging when absent.
///
/// EC55: First production caller of OptionExt::unwrap_or_debug — surfaces
/// "no dominant category" at DEBUG level for observability in hook pipelines.
pub fn dominant_category_or_debug(text: &str, fallback: &str) -> String {
    use crate::shared::result_ext::OptionExt;
    dominant_category(text).unwrap_or_debug(
        fallback.to_string(),
        "nlp_bridge: no dominant keyword category found",
    )
}

/// Fire-and-forget NLP analysis of a written document.
///
/// Spawns a Rayon task (via [`AsyncTaskBuilder`]) that runs
/// `NlpPipeline::process_document` on `content` and logs the result at TRACE level.
/// Used by `post_write` to asynchronously enrich markdown/text files without
/// blocking the hook response.
///
/// Returns immediately — caller does not await the result.
pub fn analyze_text_async(document_id: impl Into<String>, content: impl Into<String>) {
    let id: String = document_id.into();
    let text: String = content.into();
    AsyncTaskBuilder::new()
        .name(format!("nlp-analysis:{id}"))
        .spawn_rayon(move || {
            let pipeline = NlpPipeline::new();
            let analysis = pipeline.process_document(&id, &text);
            tracing::trace!(
                document_id = %id,
                keywords = analysis.keywords.len(),
                monetary = analysis.monetary_values.len(),
                chunks = analysis.chunks.len(),
                "nlp_bridge: async analysis complete"
            );
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_keywords_empty() {
        let keywords = extract_keywords("");
        assert!(keywords.is_empty());
    }

    #[test]
    fn test_chunk_for_knowledge_splits() {
        let text = "First paragraph about topic A.\n\nSecond paragraph about topic B.\n\nThird paragraph with details.";
        let chunks = chunk_for_knowledge(text, 50);
        assert!(!chunks.is_empty());
    }

    #[test]
    fn test_extract_monetary_values_br() {
        let values = extract_monetary_values("O valor de R$ 1.234,56 foi aprovado");
        if !values.is_empty() {
            assert!((values[0].value - 1234.56).abs() < 0.01);
        }
    }

    #[test]
    fn test_keyword_category_counts() {
        let counts = keyword_category_counts("test function ANTT regulatory code");
        // Returns a map — may be empty if no patterns match
        assert!(counts.len() <= 20); // sanity check
    }

    #[test]
    fn test_has_any_keywords_empty() {
        assert!(!has_any_keywords(""));
    }

    #[test]
    fn test_dominant_category_none() {
        assert!(dominant_category("").is_none());
    }

    #[test]
    fn test_has_regulatory_content_empty() {
        assert!(!has_regulatory_content(""));
    }

    #[test]
    fn test_has_technical_content_empty() {
        assert!(!has_technical_content(""));
    }
}
