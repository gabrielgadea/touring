//! Intent classification for natural language queries.
//!
//! Classifies user queries into intent types to boost semantic search relevance.
//! v1 uses keyword heuristics; future versions may use inferlets/WASM.

use serde::{Deserialize, Serialize};

/// Classified intent of a natural-language search query.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum QueryIntent {
    /// "how does X work", "why is Y", "what is Z" — understanding-oriented
    Understand = 0,
    /// "fix bug", "error in X", "why does Y fail" — debugging-oriented
    Debug = 1,
    /// "add feature", "implement X", "create Y" — implementation-oriented
    Implement = 2,
    /// "rename X to Y", "extract function", "inline X" — refactoring-oriented
    Refactor = 3,
    /// "document X", "describe Y" — documentation-oriented
    Document = 4,
    /// Default fallback — exploration/overview
    Explore = 5,
}

impl std::fmt::Display for QueryIntent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            QueryIntent::Understand => write!(f, "Understand"),
            QueryIntent::Debug => write!(f, "Debug"),
            QueryIntent::Implement => write!(f, "Implement"),
            QueryIntent::Refactor => write!(f, "Refactor"),
            QueryIntent::Document => write!(f, "Document"),
            QueryIntent::Explore => write!(f, "Explore"),
        }
    }
}

/// Outcome of classifying a query, with the chosen intent and its rationale.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct IntentResult {
    /// The intent class assigned to the query.
    pub intent: QueryIntent,
    /// Classification confidence in the range `0.0..=1.0`.
    pub confidence: f32,
    /// Human-readable explanation of why this intent was chosen.
    pub reasoning: String,
}

const UNDERSTAND_KEYWORDS: &[&str] = &[
    "how",
    "why",
    "what",
    "explain",
    "describe",
    "understand",
    "works",
];
const DEBUG_KEYWORDS: &[&str] = &[
    "fix",
    "bug",
    "error",
    "fail",
    "broken",
    "panic",
    "exception",
    "issue",
    "wrong",
];
const IMPLEMENT_KEYWORDS: &[&str] = &[
    "add",
    "create",
    "build",
    "make",
    "implement",
    "new",
    "write",
    "code",
];
const REFACTOR_KEYWORDS: &[&str] = &[
    "rename",
    "extract",
    "inline",
    "refactor",
    "move",
    "change",
    "simplify",
    "cleanup",
    "restructure",
];
const DOCUMENT_KEYWORDS: &[&str] = &[
    "document", "describe", "annotate", "comment", "explain", "spec",
];

/// Detect query intent using keyword heuristics.
/// v1: simple keyword matching with confidence scoring.
pub fn detect_intent(query: &str) -> IntentResult {
    let query_lower = query.to_lowercase();
    let words: Vec<&str> = query_lower.split_whitespace().collect();

    fn count_matches(words: &[&str], keywords: &[&str]) -> usize {
        words
            .iter()
            .filter(|w| keywords.iter().any(|k| w.contains(k)))
            .count()
    }

    let understand_count = count_matches(&words, UNDERSTAND_KEYWORDS);
    let debug_count = count_matches(&words, DEBUG_KEYWORDS);
    let implement_count = count_matches(&words, IMPLEMENT_KEYWORDS);
    let refactor_count = count_matches(&words, REFACTOR_KEYWORDS);
    let document_count = count_matches(&words, DOCUMENT_KEYWORDS);

    let mut scores = [
        (QueryIntent::Understand, understand_count),
        (QueryIntent::Debug, debug_count),
        (QueryIntent::Implement, implement_count),
        (QueryIntent::Refactor, refactor_count),
        (QueryIntent::Document, document_count),
        (QueryIntent::Explore, 0),
    ];

    // Explore should only win when ALL other intents have zero matches.
    // Use wrapping_sub so Explore's effective score is usize::MAX when all others are 0,
    // making it lose all ties (since it has the highest discriminant 5).
    scores[5].1 = scores[..5]
        .iter()
        .map(|(_, c)| *c)
        .sum::<usize>()
        .wrapping_sub(1);

    scores.sort_by(|a, b| {
        let count_cmp = b.1.cmp(&a.1);
        if count_cmp == std::cmp::Ordering::Equal {
            // Lower discriminant wins ties -> Explore (5) loses all ties
            (a.0 as u8).cmp(&(b.0 as u8))
        } else {
            count_cmp
        }
    });
    let (intent, count) = scores[0];

    let total: usize = scores.iter().map(|(_, c)| c).sum();
    let confidence = if total == 0 {
        0.3
    } else {
        (count as f32 / total as f32).max(0.3)
    };

    let reasoning = format!("matched {} keywords for {:?}", count, intent);

    IntentResult {
        intent,
        confidence,
        reasoning,
    }
}

/// Apply semantic weighting boost based on intent.
///
/// # Arguments
/// * `base_score` — the original relevance score
/// * `intent` — detected query intent
/// * `chunk_has_semantic_match` — whether the chunk has semantic signal
/// * `boost_factor` — maximum boost (default 0.2 = 20%)
pub fn apply_semantic_weighting(
    base_score: f32,
    intent: QueryIntent,
    chunk_has_semantic_match: bool,
    boost_factor: f32,
) -> f32 {
    if !chunk_has_semantic_match {
        return base_score;
    }

    // Different intents get different boost behavior
    let intent_boost = match intent {
        QueryIntent::Debug => boost_factor * 1.5, // Debug content gets extra boost
        QueryIntent::Understand => boost_factor * 1.2,
        QueryIntent::Implement => boost_factor,
        QueryIntent::Refactor => boost_factor * 0.8,
        QueryIntent::Document => boost_factor * 0.8,
        QueryIntent::Explore => boost_factor * 0.5,
    };

    base_score * (1.0 + intent_boost)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_understand() {
        let result = detect_intent("how does HookRuntime work");
        assert_eq!(result.intent, QueryIntent::Understand);
        assert!(result.confidence >= 0.5);
    }

    #[test]
    fn test_detect_debug() {
        let result = detect_intent("fix login bug in auth module");
        assert_eq!(result.intent, QueryIntent::Debug);
    }

    #[test]
    fn test_detect_implement() {
        let result = detect_intent("add new endpoint to API");
        assert_eq!(result.intent, QueryIntent::Implement);
    }

    #[test]
    fn test_detect_refactor() {
        let result = detect_intent("rename function to snake_case");
        assert_eq!(result.intent, QueryIntent::Refactor);
    }

    #[test]
    fn test_detect_document() {
        let result = detect_intent("document the auth module");
        assert_eq!(result.intent, QueryIntent::Document);
    }

    #[test]
    fn test_detect_explore() {
        // "show me the overview" - no keywords match any intent, so Explore is the fallback
        let result = detect_intent("random gibberish xyz");
        assert_eq!(result.intent, QueryIntent::Explore);
    }

    #[test]
    fn test_semantic_weighting_debug() {
        let boosted = apply_semantic_weighting(1.0, QueryIntent::Debug, true, 0.2);
        assert!(boosted > 1.0);
        // Debug gets 1.5x boost: 1.0 * (1.0 + 0.2*1.5) = 1.3
        assert_eq!(boosted, 1.3);
    }

    #[test]
    fn test_semantic_weighting_understand() {
        let boosted = apply_semantic_weighting(1.0, QueryIntent::Understand, true, 0.2);
        // Understand gets 1.2x boost: 1.0 * (1.0 + 0.2*1.2) = 1.24
        assert_eq!(boosted, 1.24);
    }

    #[test]
    fn test_semantic_weighting_implement() {
        let boosted = apply_semantic_weighting(1.0, QueryIntent::Implement, true, 0.2);
        // Implement gets 1.0x boost: 1.0 * (1.0 + 0.2) = 1.2
        assert_eq!(boosted, 1.2);
    }

    #[test]
    fn test_semantic_weighting_refactor() {
        let boosted = apply_semantic_weighting(1.0, QueryIntent::Refactor, true, 0.2);
        // Refactor gets 0.8x boost: 1.0 * (1.0 + 0.2*0.8) = 1.16
        assert_eq!(boosted, 1.16);
    }

    #[test]
    fn test_semantic_weighting_no_match() {
        let base = apply_semantic_weighting(1.0, QueryIntent::Understand, false, 0.2);
        assert_eq!(base, 1.0); // No boost without semantic match
    }

    #[test]
    fn test_confidence_low_total() {
        let result = detect_intent("xyz abc");
        // Low total should still give 0.3 minimum confidence
        assert!(result.confidence >= 0.3);
    }
}
