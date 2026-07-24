//! touring-nlp — NLP pipeline for regulatory document analysis.
//!
//! High-performance NLP utilities for ANTT regulatory document analysis.
//!
//! # Modules
//!
//! - `monetary_parser`: Parse Brazilian monetary values with 50+ patterns
//! - `semantic_chunker`: Semantic text chunking with ANTT patterns
//! - `cross_validator`: Cross-validation for contradictions and confidence propagation
//! - `keyword_matcher`: Aho-Corasick multi-pattern matching with fuzzy support
//! - `rlm_integration`: Unified NLP pipeline with RLM Memory caching
//! - `reranker`: Contextual reranking for RAG with authority weights
//! - `search_index`: Search preprocessing for .claude/ documents

pub mod cross_validator;
pub mod financial_analysis;
pub mod keyword_matcher;
pub mod monetary_parser;
pub mod reranker;
pub mod rlm_integration;
pub mod search_index;
pub mod semantic_chunker;
/// Shared validation status (used by cross_validator + rlm_integration).
pub mod validation_status;

// Re-exports
pub use cross_validator::{
    Assertion, AssertionType, Contradiction, ContradictionType, CrossValidator, Evidence,
    EvidenceRelationship, Gap, GapType, NormalizedValue, NormativeIndex, ValidationResult,
    ValidatorConfig,
};
pub use financial_analysis::{
    ConcessionAnalysis, analyze_concession, antt_standard_scenarios, antt_wacc_range,
    extract_and_analyze, viability_verdict,
};
pub use keyword_matcher::{
    ANTT_PATTERNS, CacheStats, KeywordMatch, KeywordMatcher, MatcherConfig, PatternCategory,
    TECHNICAL_KEYWORDS, clear_cache, get_cache_stats, levenshtein_distance,
};
pub use monetary_parser::{
    MonetaryValue, ParseError, detect_multiplier, normalize_br_number, parse_monetary,
    parse_monetary_batch,
};
pub use reranker::{
    AuthorityWeights, ContextualReranker, RankedResult, RankingFactors, RerankContext,
    RerankWeights, RerankerError, RerankerStats, SearchResult, compute_ndcg,
};
pub use rlm_integration::{
    ChunkSummary, DocumentAnalysis, MatchSummary, MonetaryValueSummary, NlpPipeline, PipelineStats,
};
pub use semantic_chunker::{
    BoundaryType, ChunkerConfig, SemanticChunk, SemanticChunker, TokenEstimator,
};
pub use semantic_chunker::{chunk_document, chunk_documents_batch};
pub use validation_status::ValidationStatus;
