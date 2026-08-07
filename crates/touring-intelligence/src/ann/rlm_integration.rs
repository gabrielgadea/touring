//! RLM Integration - Pln2 Module 5
//!
//! Integrates NLP modules with RLM (Recursive Language Model) Memory
//! from touring-learning.
//!
//! # Features
//!
//! - Cache of monetary values, chunks, validations, and patterns
//! - Learning from pattern usage
//! - Statistics tracking for optimization
//! - Unified NLP pipeline API

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use crate::ann::keyword_matcher::{ANTT_PATTERNS, KeywordMatch, TECHNICAL_KEYWORDS};
use crate::ann::monetary_parser::{MonetaryValue, parse_monetary};
use crate::ann::semantic_chunker::{ChunkerConfig, SemanticChunk, SemanticChunker};
use crate::ann::validation_status::ValidationStatus;

// touring-learning memory integration
use crate::rl::memory::rlm::{MemoryStats, MemoryTier, RlmMemory};
use touring_foundation::truncate_str;

// ============================================================================
// CONSTANTS
// ============================================================================

const CHUNK_PREFIX: &str = "chunk:";
const VALIDATION_PREFIX: &str = "validation:";

// ============================================================================
// CACHE STATISTICS
// ============================================================================

/// Statistics for NLP pipeline operations.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PipelineStats {
    /// Total documents run through the pipeline.
    pub documents_processed: u64,
    /// Count of monetary values extracted.
    pub monetary_extractions: u64,
    /// Number of semantic chunks created.
    pub chunks_created: u64,
    /// Cache hits when looking up chunks in RLM memory.
    pub chunk_cache_hits: u64,
    /// Cache misses when looking up chunks in RLM memory.
    pub chunk_cache_misses: u64,
    /// Number of validations confirmed correct.
    pub validation_correct: u64,
    /// Number of validations confirmed incorrect.
    pub validation_incorrect: u64,
    /// Total keyword/pattern matches recorded.
    pub patterns_matched: u64,
    /// Most-matched patterns as `(pattern, count)` pairs.
    pub top_patterns: Vec<(String, u64)>,
}

impl PipelineStats {
    /// Ratio of chunk cache hits to total chunk lookups (0.0 when none).
    pub fn chunk_cache_hit_rate(&self) -> f64 {
        let total = self.chunk_cache_hits + self.chunk_cache_misses;
        if total == 0 {
            0.0
        } else {
            self.chunk_cache_hits as f64 / total as f64
        }
    }

    /// Ratio of correct validations to total validations (0.0 when none).
    pub fn validation_accuracy(&self) -> f64 {
        let total = self.validation_correct + self.validation_incorrect;
        if total == 0 {
            0.0
        } else {
            self.validation_correct as f64 / total as f64
        }
    }
}

// ============================================================================
// DOCUMENT ANALYSIS RESULT
// ============================================================================

/// Complete analysis of a document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentAnalysis {
    /// Stable identifier of the analyzed document.
    pub document_id: String,
    /// Monetary values extracted from the document.
    pub monetary_values: Vec<MonetaryValueSummary>,
    /// Semantic chunks produced for the document.
    pub chunks: Vec<ChunkSummary>,
    /// Technical keyword matches found in the document.
    pub keywords: Vec<MatchSummary>,
    /// Reference/citation matches found in the document.
    pub references: Vec<MatchSummary>,
    /// Wall-clock processing time in milliseconds.
    pub processing_time_ms: f64,
    /// Whether the result was served from RLM cache.
    pub from_cache: bool,
}

/// Lightweight monetary value summary for serialization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonetaryValueSummary {
    /// Numeric amount of the monetary value.
    pub value: f64,
    /// Currency code or symbol of the value.
    pub currency: String,
    /// Original text the value was parsed from.
    pub source_text: String,
    /// Parser confidence in the extraction (0.0..=1.0).
    pub confidence: f64,
}

impl From<&MonetaryValue> for MonetaryValueSummary {
    fn from(mv: &MonetaryValue) -> Self {
        Self {
            value: mv.value,
            currency: mv.currency.clone(),
            source_text: mv.source_text.clone(),
            confidence: mv.confidence,
        }
    }
}

/// Lightweight chunk summary for serialization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkSummary {
    /// Text content of the chunk.
    pub content: String,
    /// Number of tokens in the chunk.
    pub token_count: usize,
    /// Byte offset where the chunk begins in the source document.
    pub start_offset: usize,
    /// Byte offset where the chunk ends in the source document.
    pub end_offset: usize,
}

impl From<&SemanticChunk> for ChunkSummary {
    fn from(chunk: &SemanticChunk) -> Self {
        Self {
            content: chunk.content.clone(),
            token_count: chunk.token_count,
            start_offset: chunk.byte_range.0,
            end_offset: chunk.byte_range.1,
        }
    }
}

/// Lightweight match summary for serialization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchSummary {
    /// Keyword/pattern that produced the match.
    pub keyword: String,
    /// Actual substring of the document that matched.
    pub matched_text: String,
    /// Byte offset where the match begins.
    pub start: usize,
    /// Byte offset where the match ends.
    pub end: usize,
    /// Relevance score of the match.
    pub score: f64,
    /// Optional category the keyword belongs to.
    pub category: Option<String>,
}

impl From<&KeywordMatch> for MatchSummary {
    fn from(m: &KeywordMatch) -> Self {
        Self {
            keyword: m.keyword.clone(),
            matched_text: m.matched_text.clone(),
            start: m.start,
            end: m.end,
            score: m.match_score,
            category: m.category.clone(),
        }
    }
}

// ============================================================================
// VALIDATION FEEDBACK
// ============================================================================

/// Feedback for validation learning.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationFeedback {
    /// Whether the validation was ultimately correct.
    pub was_correct: bool,
    /// Classification of the error when incorrect.
    pub error_type: Option<String>,
    /// Suggested correction when incorrect.
    pub correction: Option<String>,
    /// Unix timestamp when the feedback was recorded.
    pub timestamp: u64,
}

/// Validation record with result and feedback.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationRecord {
    /// The assertion text that was validated.
    pub assertion_content: String,
    /// Validation status as a string (e.g. `valid`, `invalid`).
    pub status: String,
    /// Confidence in the validation result (0.0..=1.0).
    pub confidence: f64,
    /// Optional learning feedback attached to the record.
    pub feedback: Option<ValidationFeedback>,
    /// Unix timestamp when the record was created.
    pub created_at: u64,
}

// ============================================================================
// PATTERN FREQUENCY TRACKER
// ============================================================================

#[derive(Debug, Default)]
struct PatternFrequencyTracker {
    frequencies: HashMap<String, u64>,
    contexts: HashMap<String, Vec<String>>,
}

impl PatternFrequencyTracker {
    fn new() -> Self {
        Self::default()
    }

    fn record(&mut self, keyword: &str, context: Option<&str>) {
        *self.frequencies.entry(keyword.to_string()).or_insert(0) += 1;

        if let Some(ctx) = context {
            self.contexts
                .entry(keyword.to_string())
                .or_default()
                .push(ctx.to_string());

            if let Some(ctxs) = self.contexts.get_mut(keyword)
                && ctxs.len() > 100
            {
                ctxs.drain(0..50);
            }
        }
    }

    fn top_patterns(&self, n: usize) -> Vec<(String, u64)> {
        let mut items: Vec<_> = self.frequencies.iter().collect();
        items.sort_by(|a, b| b.1.cmp(a.1));
        items
            .into_iter()
            .take(n)
            .map(|(k, v)| (k.clone(), *v))
            .collect()
    }

    fn suggest_patterns(&self, min_freq: usize, top_k: usize) -> Vec<String> {
        let mut ngram_freq: HashMap<String, usize> = HashMap::new();

        for contexts in self.contexts.values() {
            for ctx in contexts {
                for ngram in Self::extract_ngrams(ctx, 2, 4) {
                    *ngram_freq.entry(ngram).or_insert(0) += 1;
                }
            }
        }

        let mut freq_vec: Vec<_> = ngram_freq.into_iter().collect();
        freq_vec.sort_by_key(|(_, count)| std::cmp::Reverse(*count));

        freq_vec
            .into_iter()
            .filter(|(_, count)| *count >= min_freq)
            .take(top_k)
            .map(|(ngram, _)| ngram)
            .collect()
    }

    fn extract_ngrams(text: &str, min_n: usize, max_n: usize) -> Vec<String> {
        let words: Vec<&str> = text.split_whitespace().collect();
        let mut ngrams = Vec::new();

        for n in min_n..=max_n.min(words.len()) {
            for window in words.windows(n) {
                ngrams.push(window.join(" "));
            }
        }

        ngrams
    }
}

// ============================================================================
// NLP PIPELINE
// ============================================================================

/// Unified NLP Pipeline with RLM Memory integration.
///
/// Provides caching, learning, and statistics for all NLP operations.
#[derive(Debug)]
pub struct NlpPipeline {
    /// RLM Memory instance (optional — None when no DB path available).
    memory: Option<Arc<Mutex<RlmMemory>>>,
    /// Chunker configuration.
    chunker_config: ChunkerConfig,
    /// Pattern frequency tracker.
    pattern_tracker: Arc<Mutex<PatternFrequencyTracker>>,
    /// Pipeline statistics.
    stats: Arc<Mutex<PipelineStats>>,
}

impl NlpPipeline {
    /// Create new NLP Pipeline without memory persistence.
    /// Use `with_memory` to attach an RLM memory backend.
    pub fn new() -> Self {
        Self {
            memory: None,
            chunker_config: ChunkerConfig::default(),
            pattern_tracker: Arc::new(std::sync::Mutex::new(PatternFrequencyTracker::new())),
            stats: Arc::new(std::sync::Mutex::new(PipelineStats::default())),
        }
    }

    /// Create with an existing RlmMemory instance.
    pub fn with_memory(memory: RlmMemory) -> Self {
        Self {
            memory: Some(Arc::new(std::sync::Mutex::new(memory))),
            chunker_config: ChunkerConfig::default(),
            pattern_tracker: Arc::new(std::sync::Mutex::new(PatternFrequencyTracker::new())),
            stats: Arc::new(std::sync::Mutex::new(PipelineStats::default())),
        }
    }

    /// Create with custom chunker configuration.
    pub fn with_chunker_config(config: ChunkerConfig) -> Self {
        Self {
            memory: None,
            chunker_config: config,
            pattern_tracker: Arc::new(std::sync::Mutex::new(PatternFrequencyTracker::new())),
            stats: Arc::new(std::sync::Mutex::new(PipelineStats::default())),
        }
    }

    /// Generate cache key for chunker config.
    fn config_hash(&self) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.chunker_config.target_tokens.hash(&mut hasher);
        self.chunker_config.overlap_sentences.hash(&mut hasher);
        hasher.finish()
    }

    /// Process a document with full NLP pipeline.
    ///
    /// Extracts keywords, monetary values, and semantic chunks from `content`.
    /// Results are stored in the attached RLM memory backend (if any) and returned
    /// for immediate use (e.g., background NLP enrichment via `nlp_bridge`).
    pub fn process_document(&self, document_id: &str, content: &str) -> DocumentAnalysis {
        let start = std::time::Instant::now();
        let cache_key = format!("{}{}:{}", CHUNK_PREFIX, document_id, self.config_hash());

        // Check cache for existing analysis
        let cached_chunks = if let Some(ref mem_arc) = self.memory {
            let mem = mem_arc.lock().unwrap_or_else(|e| e.into_inner());
            mem.get(&cache_key, MemoryTier::Working).ok().flatten()
        } else {
            None
        };

        let (chunks, from_cache) = if let Some(cached_json) = cached_chunks {
            if let Ok(cached) = serde_json::from_str::<Vec<ChunkSummary>>(&cached_json) {
                {
                    let mut stats = self.stats.lock().unwrap_or_else(|e| e.into_inner());
                    stats.chunk_cache_hits += 1;
                }
                (cached, true)
            } else {
                (self.chunk_content(content), false)
            }
        } else {
            {
                let mut stats = self.stats.lock().unwrap_or_else(|e| e.into_inner());
                stats.chunk_cache_misses += 1;
            }
            (self.chunk_content(content), false)
        };

        // Cache chunks if not from cache
        if !from_cache
            && let Some(ref mem_arc) = self.memory
            && let Ok(json) = serde_json::to_string(&chunks)
        {
            let mem = mem_arc.lock().unwrap_or_else(|e| e.into_inner());
            let _ = mem.store(&cache_key, MemoryTier::Working, &json, Some("chunks"), None);
        }

        let monetary_values = self.extract_monetary(content);
        let keywords = self.find_keywords(content);
        let references = self.find_references(content);

        // Record pattern frequencies
        {
            let mut tracker = self
                .pattern_tracker
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            for kw in &keywords {
                tracker.record(&kw.keyword, Some(&kw.matched_text));
            }
            for ref_ in &references {
                tracker.record(&ref_.keyword, Some(&ref_.matched_text));
            }
        }

        // Update stats
        {
            let mut stats = self.stats.lock().unwrap_or_else(|e| e.into_inner());
            stats.documents_processed += 1;
            stats.monetary_extractions += monetary_values.len() as u64;
            stats.chunks_created += if from_cache { 0 } else { chunks.len() as u64 };
            stats.patterns_matched += (keywords.len() + references.len()) as u64;
        }

        DocumentAnalysis {
            document_id: document_id.to_string(),
            monetary_values,
            chunks,
            keywords,
            references,
            processing_time_ms: start.elapsed().as_secs_f64() * 1000.0,
            from_cache,
        }
    }

    fn chunk_content(&self, content: &str) -> Vec<ChunkSummary> {
        let chunker = SemanticChunker::new(self.chunker_config.clone());
        chunker
            .chunk(content)
            .iter()
            .map(ChunkSummary::from)
            .collect()
    }

    fn extract_monetary(&self, content: &str) -> Vec<MonetaryValueSummary> {
        parse_monetary(content)
            .unwrap_or_default()
            .iter()
            .map(MonetaryValueSummary::from)
            .collect()
    }

    fn find_keywords(&self, content: &str) -> Vec<MatchSummary> {
        TECHNICAL_KEYWORDS
            .find_matches(content)
            .iter()
            .map(MatchSummary::from)
            .collect()
    }

    fn find_references(&self, content: &str) -> Vec<MatchSummary> {
        ANTT_PATTERNS
            .find_matches(content)
            .iter()
            .map(MatchSummary::from)
            .collect()
    }

    /// Record validation feedback for learning.
    pub fn record_validation_feedback(
        &self,
        assertion_content: &str,
        _status: ValidationStatus,
        was_correct: bool,
        error_type: Option<String>,
    ) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let record = ValidationRecord {
            assertion_content: assertion_content.to_string(),
            status: if was_correct {
                "confirmed"
            } else {
                "contradicted"
            }
            .to_string(),
            confidence: 0.0,
            feedback: Some(ValidationFeedback {
                was_correct,
                error_type,
                correction: None,
                timestamp: now,
            }),
            created_at: now,
        };

        if let Some(ref mem_arc) = self.memory
            && let Ok(json) = serde_json::to_string(&record)
        {
            let key = format!(
                "{}{}:{}",
                VALIDATION_PREFIX,
                truncate_str(assertion_content, 32),
                now
            );
            let mem = mem_arc.lock().unwrap_or_else(|e| e.into_inner());
            let _ = mem.store(&key, MemoryTier::Reference, &json, Some("validation"), None);
        }

        // Update stats
        {
            let mut stats = self.stats.lock().unwrap_or_else(|e| e.into_inner());
            if was_correct {
                stats.validation_correct += 1;
            } else {
                stats.validation_incorrect += 1;
            }
        }
    }

    /// Get pipeline statistics.
    pub fn get_stats(&self) -> PipelineStats {
        let mut stats = self.stats.lock().unwrap_or_else(|e| e.into_inner()).clone();
        stats.top_patterns = self
            .pattern_tracker
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .top_patterns(10);
        stats
    }

    /// Get suggested new patterns based on context analysis.
    pub fn suggest_patterns(&self, min_freq: usize, top_k: usize) -> Vec<String> {
        self.pattern_tracker
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .suggest_patterns(min_freq, top_k)
    }

    /// Clear all caches and stats.
    ///
    /// Resets `PipelineStats` and `PatternFrequencyTracker` to their defaults.
    /// Intended for test teardown and session-end cleanup.
    pub fn clear_all(&self) {
        let mut stats = self.stats.lock().unwrap_or_else(|e| e.into_inner());
        *stats = PipelineStats::default();

        let mut tracker = self
            .pattern_tracker
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        *tracker = PatternFrequencyTracker::new();
    }

    /// Get RLM memory statistics (if memory is attached).
    pub fn memory_stats(&self) -> Option<MemoryStats> {
        self.memory.as_ref().and_then(|mem_arc| {
            let mem = mem_arc.lock().unwrap_or_else(|e| e.into_inner());
            mem.stats().ok()
        })
    }
}

impl Default for NlpPipeline {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pipeline_creation() {
        let pipeline = NlpPipeline::new();
        let stats = pipeline.get_stats();
        assert_eq!(stats.documents_processed, 0);
        assert_eq!(stats.chunk_cache_hits, 0);
    }

    #[test]
    fn test_process_simple_document() {
        let pipeline = NlpPipeline::new();
        let content = "O valor é R$ 100.000,00 conforme Resolução ANTT 5.950/2021.";

        let analysis = pipeline.process_document("doc1", content);

        assert_eq!(analysis.document_id, "doc1");
        assert!(
            !analysis.monetary_values.is_empty(),
            "Should find monetary value"
        );
        assert!(
            !analysis.references.is_empty(),
            "Should find ANTT reference"
        );
        assert!(!analysis.from_cache);
    }

    #[test]
    fn test_monetary_extraction() {
        let pipeline = NlpPipeline::new();
        let content = r#"
            O VPL calculado é de R$ 131.802.273,70.
            A TIR foi estimada em 10,5%.
        "#;

        let analysis = pipeline.process_document("doc1", content);

        let vpl = analysis
            .monetary_values
            .iter()
            .find(|v| v.value > 100_000_000.0);
        assert!(vpl.is_some(), "Should find VPL value");
        let vpl = vpl.unwrap();
        assert!((vpl.value - 131_802_273.70).abs() < 0.01);
    }

    #[test]
    fn test_keyword_matching() {
        let pipeline = NlpPipeline::new();
        let content = "O VPL e a TIR foram calculados para o reequilíbrio econômico-financeiro.";

        let analysis = pipeline.process_document("doc1", content);

        let keywords: Vec<_> = analysis
            .keywords
            .iter()
            .map(|k| k.keyword.as_str())
            .collect();
        assert!(
            keywords
                .iter()
                .any(|k| k.contains("VPL") || k.contains("TIR") || k.contains("reequilíbrio")),
            "Should find financial keywords"
        );
    }

    #[test]
    fn test_pattern_frequency_tracking() {
        let pipeline = NlpPipeline::new();

        for i in 0..5 {
            pipeline.process_document(
                &format!("doc{}", i),
                "O VPL calculado para o reequilíbrio é positivo.",
            );
        }

        let stats = pipeline.get_stats();
        assert!(
            !stats.top_patterns.is_empty(),
            "Should have pattern frequencies"
        );
        assert!(stats.patterns_matched >= 5, "Should have matched patterns");
    }

    #[test]
    fn test_clear_all() {
        let pipeline = NlpPipeline::new();
        pipeline.process_document("doc1", "R$ 100.000,00");

        pipeline.clear_all();

        let stats = pipeline.get_stats();
        assert_eq!(stats.documents_processed, 0, "Stats should be cleared");
    }

    #[test]
    fn test_pipeline_stats_methods() {
        let mut stats = PipelineStats::default();

        assert_eq!(stats.chunk_cache_hit_rate(), 0.0);
        assert_eq!(stats.validation_accuracy(), 0.0);

        stats.chunk_cache_hits = 80;
        stats.chunk_cache_misses = 20;
        assert!((stats.chunk_cache_hit_rate() - 0.8).abs() < 0.01);

        stats.validation_correct = 90;
        stats.validation_incorrect = 10;
        assert!((stats.validation_accuracy() - 0.9).abs() < 0.01);
    }

    #[test]
    fn test_summary_conversions() {
        let mv = MonetaryValue {
            value: 100_000.0,
            currency: "BRL".to_string(),
            multiplier: 1.0,
            source_text: "R$ 100.000,00".to_string(),
            position: (0, 13),
            confidence: 0.95,
            context: "".to_string(),
        };
        let summary = MonetaryValueSummary::from(&mv);
        assert_eq!(summary.value, 100_000.0);
        assert_eq!(summary.currency, "BRL");

        let km = KeywordMatch {
            keyword: "VPL".to_string(),
            pattern_index: 0,
            start: 0,
            end: 3,
            matched_text: "VPL".to_string(),
            context_before: "".to_string(),
            context_after: "".to_string(),
            match_score: 1.0,
            category: Some("Financial".to_string()),
        };
        let match_summary = MatchSummary::from(&km);
        assert_eq!(match_summary.keyword, "VPL");
        assert_eq!(match_summary.score, 1.0);
    }
}
