//! PyO3 bindings for touring-antt NLP pipeline types.
//!
//! Wraps `touring_intelligence::ann` types with PyO3 attributes to expose monetary parsing,
//! keyword matching, and semantic chunking to Python.
//!
//! # Modules
//!
//! - **Monetary Parser**: `PyMonetaryValue`, `py_parse_monetary`, `py_parse_monetary_batch`
//! - **Keyword Matcher**: `PyKeywordMatch`, `PyMatcherConfig`, `PyKeywordMatcher`
//! - **Semantic Chunker**: `PySemanticChunk`, `PyChunkerConfig`, `py_chunk_document`, `py_chunk_documents_batch`

use pyo3::prelude::*;
use serde::{Deserialize, Serialize};

use touring_intelligence::ann::keyword_matcher as tl_kw;
use touring_intelligence::ann::monetary_parser as tl_mon;
use touring_intelligence::ann::semantic_chunker as tl_chunk;

// ═══════════════════════════════════════════════════════════════════════════
// A. Monetary Parser
// ═══════════════════════════════════════════════════════════════════════════

/// Parsed monetary value with metadata — wraps `touring_intelligence::ann::MonetaryValue`.
#[pyclass(frozen, get_all, skip_from_py_object)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PyMonetaryValue {
    /// Normalized numeric value (with multiplier applied).
    pub value: f64,
    /// Currency code (BRL, USD, EUR).
    pub currency: String,
    /// Multiplier applied (1.0, 1000.0, 1_000_000.0, 1_000_000_000.0).
    pub multiplier: f64,
    /// Original text that was matched.
    pub source_text: String,
    /// Position in source text — start byte offset.
    pub position_start: usize,
    /// Position in source text — end byte offset.
    pub position_end: usize,
    /// Confidence score [0.0, 1.0].
    pub confidence: f64,
    /// Context around the match.
    pub context: String,
}

#[pymethods]
impl PyMonetaryValue {
    fn __str__(&self) -> String {
        format!(
            "MonetaryValue({} {:.2} @ ({}, {}), conf={:.2})",
            self.currency, self.value, self.position_start, self.position_end, self.confidence
        )
    }

    fn __repr__(&self) -> String {
        self.__str__()
    }

    fn to_json(&self) -> PyResult<String> {
        serde_json::to_string_pretty(self)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
    }
}

impl PyMonetaryValue {
    fn from_tl(inner: &tl_mon::MonetaryValue) -> Self {
        Self {
            value: inner.value,
            currency: inner.currency.clone(),
            multiplier: inner.multiplier,
            source_text: inner.source_text.clone(),
            position_start: inner.position.0,
            position_end: inner.position.1,
            confidence: inner.confidence,
            context: inner.context.clone(),
        }
    }
}

/// Parse monetary values from a single text.
///
/// Returns a list of `PyMonetaryValue` for each match found.
#[pyfunction]
pub fn py_parse_monetary(text: &str) -> PyResult<Vec<PyMonetaryValue>> {
    let results = tl_mon::parse_monetary(text)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
    Ok(results.iter().map(PyMonetaryValue::from_tl).collect())
}

/// Parse monetary values from multiple texts in parallel.
///
/// Releases the GIL during Rayon-parallelized processing.
#[pyfunction]
pub fn py_parse_monetary_batch(py: Python<'_>, texts: Vec<String>) -> Vec<Vec<PyMonetaryValue>> {
    py.detach(|| {
        let batch_results = tl_mon::parse_monetary_batch(texts);
        batch_results
            .iter()
            .map(|doc_results| doc_results.iter().map(PyMonetaryValue::from_tl).collect())
            .collect()
    })
}

// ═══════════════════════════════════════════════════════════════════════════
// B. Keyword Matcher
// ═══════════════════════════════════════════════════════════════════════════

/// Result of a keyword match with context — wraps `touring_intelligence::ann::KeywordMatch`.
#[pyclass(frozen, get_all, skip_from_py_object)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PyKeywordMatch {
    /// The keyword pattern that matched.
    pub keyword: String,
    /// Index of the pattern in the original list.
    pub pattern_index: usize,
    /// Start position in bytes.
    pub start: usize,
    /// End position in bytes.
    pub end: usize,
    /// The actual matched text (may differ from keyword if fuzzy).
    pub matched_text: String,
    /// Context before the match.
    pub context_before: String,
    /// Context after the match.
    pub context_after: String,
    /// Match score [0,1] (1.0 = exact, <1.0 = fuzzy).
    pub match_score: f64,
    /// Category of the pattern (if set).
    pub category: Option<String>,
}

#[pymethods]
impl PyKeywordMatch {
    fn __str__(&self) -> String {
        format!(
            "KeywordMatch('{}' @ ({}, {}), score={:.2})",
            self.keyword, self.start, self.end, self.match_score
        )
    }

    fn __repr__(&self) -> String {
        self.__str__()
    }

    fn to_json(&self) -> PyResult<String> {
        serde_json::to_string_pretty(self)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
    }
}

impl PyKeywordMatch {
    fn from_tl(inner: &tl_kw::KeywordMatch) -> Self {
        Self {
            keyword: inner.keyword.clone(),
            pattern_index: inner.pattern_index,
            start: inner.start,
            end: inner.end,
            matched_text: inner.matched_text.clone(),
            context_before: inner.context_before.clone(),
            context_after: inner.context_after.clone(),
            match_score: inner.match_score,
            category: inner.category.clone(),
        }
    }
}

/// Configuration for the keyword matcher — wraps `touring_intelligence::ann::MatcherConfig`.
#[pyclass(frozen, get_all, skip_from_py_object)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PyMatcherConfig {
    /// Number of characters of context to extract before/after match.
    pub context_size: usize,
    /// Enable case-insensitive matching.
    pub case_insensitive: bool,
    /// Allow overlapping matches.
    pub allow_overlapping: bool,
    /// Enable fuzzy matching (Levenshtein distance).
    pub fuzzy_enabled: bool,
    /// Maximum edit distance for fuzzy matching.
    pub max_edit_distance: usize,
    /// Minimum score threshold for fuzzy matches.
    pub min_fuzzy_score: f64,
}

#[pymethods]
impl PyMatcherConfig {
    #[new]
    #[pyo3(signature = (
        context_size = 50,
        case_insensitive = true,
        allow_overlapping = false,
        fuzzy_enabled = false,
        max_edit_distance = 2,
        min_fuzzy_score = 0.8
    ))]
    fn new(
        context_size: usize,
        case_insensitive: bool,
        allow_overlapping: bool,
        fuzzy_enabled: bool,
        max_edit_distance: usize,
        min_fuzzy_score: f64,
    ) -> Self {
        Self {
            context_size,
            case_insensitive,
            allow_overlapping,
            fuzzy_enabled,
            max_edit_distance,
            min_fuzzy_score,
        }
    }

    fn __str__(&self) -> String {
        format!(
            "MatcherConfig(context_size={}, case_insensitive={}, fuzzy_enabled={})",
            self.context_size, self.case_insensitive, self.fuzzy_enabled
        )
    }

    fn __repr__(&self) -> String {
        self.__str__()
    }

    fn to_json(&self) -> PyResult<String> {
        serde_json::to_string_pretty(self)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
    }
}

impl PyMatcherConfig {
    fn to_tl(&self) -> tl_kw::MatcherConfig {
        tl_kw::MatcherConfig {
            context_size: self.context_size,
            case_insensitive: self.case_insensitive,
            allow_overlapping: self.allow_overlapping,
            fuzzy_enabled: self.fuzzy_enabled,
            max_edit_distance: self.max_edit_distance,
            min_fuzzy_score: self.min_fuzzy_score,
        }
    }
}

/// High-performance keyword matcher using Aho-Corasick algorithm.
///
/// Wraps `touring_intelligence::ann::KeywordMatcher` with PyO3 interface.
#[pyclass(name = "KeywordMatcher")]
#[derive(Debug)]
pub struct PyKeywordMatcher {
    inner: tl_kw::KeywordMatcher,
}

#[pymethods]
impl PyKeywordMatcher {
    /// Create a new keyword matcher with patterns and optional config.
    #[new]
    #[pyo3(signature = (patterns, config = None))]
    fn new(patterns: Vec<String>, config: Option<&PyMatcherConfig>) -> Self {
        let tl_config = config.map(|c| c.to_tl()).unwrap_or_default();
        Self {
            inner: tl_kw::KeywordMatcher::new(patterns, tl_config),
        }
    }

    /// Find all keyword matches in the given text.
    fn find_matches(&self, text: &str) -> Vec<PyKeywordMatch> {
        self.inner
            .find_matches(text)
            .iter()
            .map(PyKeywordMatch::from_tl)
            .collect()
    }

    /// Find matches across multiple texts in parallel (releases GIL).
    fn find_matches_batch(&self, py: Python<'_>, texts: Vec<String>) -> Vec<Vec<PyKeywordMatch>> {
        // KeywordMatcher is not Send, so we collect results text-by-text
        // but release GIL for the overall batch. The inner Aho-Corasick
        // automaton is Arc-shared so the cost is primarily the matching.
        let results: Vec<Vec<tl_kw::KeywordMatch>> = texts
            .iter()
            .map(|text| self.inner.find_matches(text))
            .collect();

        // Release GIL only for the conversion step (lighter but still useful
        // if batch is large).
        py.detach(|| {
            results
                .iter()
                .map(|doc_matches| doc_matches.iter().map(PyKeywordMatch::from_tl).collect())
                .collect()
        })
    }

    /// Number of patterns in this matcher.
    fn pattern_count(&self) -> usize {
        self.inner.pattern_count()
    }

    fn __len__(&self) -> usize {
        self.inner.pattern_count()
    }

    fn __repr__(&self) -> String {
        format!("KeywordMatcher(patterns={})", self.inner.pattern_count())
    }
}

/// Compute Levenshtein edit distance between two strings.
#[pyfunction]
pub fn py_levenshtein_distance(a: &str, b: &str) -> usize {
    tl_kw::levenshtein_distance(a, b)
}

// ═══════════════════════════════════════════════════════════════════════════
// C. Semantic Chunker
// ═══════════════════════════════════════════════════════════════════════════

/// Semantic chunk with rich metadata — wraps `touring_intelligence::ann::SemanticChunk`.
#[pyclass(frozen, get_all, skip_from_py_object)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PySemanticChunk {
    /// Text content of the chunk.
    pub content: String,
    /// Byte range start in original document.
    pub byte_range_start: usize,
    /// Byte range end in original document.
    pub byte_range_end: usize,
    /// Character range start in original document.
    pub char_range_start: usize,
    /// Character range end in original document.
    pub char_range_end: usize,
    /// Estimated token count.
    pub token_count: usize,
    /// Boundary type name at chunk start.
    pub start_boundary_name: String,
    /// Boundary type name at chunk end.
    pub end_boundary_name: String,
    /// Start boundary heading level (0 if not heading).
    pub start_boundary_level: u8,
    /// End boundary heading level (0 if not heading).
    pub end_boundary_level: u8,
    /// Context from previous chunk (overlap).
    pub context_before: String,
    /// Context for next chunk (overlap).
    pub context_after: String,
    /// Zero-based chunk index.
    pub chunk_index: usize,
    /// Total number of chunks in document.
    pub total_chunks: usize,
    /// Hierarchical section path (heading titles).
    pub section_path: Vec<String>,
}

#[pymethods]
impl PySemanticChunk {
    fn __str__(&self) -> String {
        let preview_len = self.content.len().min(60);
        let preview = &self.content[..preview_len];
        format!(
            "SemanticChunk({}/{}, tokens={}, '{}'...)",
            self.chunk_index, self.total_chunks, self.token_count, preview
        )
    }

    fn __repr__(&self) -> String {
        self.__str__()
    }

    fn to_json(&self) -> PyResult<String> {
        serde_json::to_string_pretty(self)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
    }
}

impl PySemanticChunk {
    fn from_tl(inner: &tl_chunk::SemanticChunk) -> Self {
        Self {
            content: inner.content.clone(),
            byte_range_start: inner.byte_range.0,
            byte_range_end: inner.byte_range.1,
            char_range_start: inner.char_range.0,
            char_range_end: inner.char_range.1,
            token_count: inner.token_count,
            start_boundary_name: inner.start_boundary_name.clone(),
            end_boundary_name: inner.end_boundary_name.clone(),
            start_boundary_level: inner.start_boundary_level,
            end_boundary_level: inner.end_boundary_level,
            context_before: inner.context_before.clone(),
            context_after: inner.context_after.clone(),
            chunk_index: inner.chunk_index,
            total_chunks: inner.total_chunks,
            section_path: inner.section_path.clone(),
        }
    }
}

/// Configuration for the semantic chunker — wraps `touring_intelligence::ann::ChunkerConfig`.
#[pyclass(frozen, get_all, skip_from_py_object)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PyChunkerConfig {
    /// Target chunk size in tokens (not characters).
    pub target_tokens: usize,
    /// Tolerance for exceeding target (percentage).
    pub tolerance_percent: f64,
    /// Number of sentences to overlap between chunks.
    pub overlap_sentences: usize,
    /// Whether to preserve table rows together.
    pub preserve_tables: bool,
    /// Whether to preserve code blocks together.
    pub preserve_code_blocks: bool,
}

#[pymethods]
impl PyChunkerConfig {
    #[new]
    #[pyo3(signature = (
        target_tokens = 512,
        tolerance_percent = 20.0,
        overlap_sentences = 2,
        preserve_tables = true,
        preserve_code_blocks = true
    ))]
    fn new(
        target_tokens: usize,
        tolerance_percent: f64,
        overlap_sentences: usize,
        preserve_tables: bool,
        preserve_code_blocks: bool,
    ) -> Self {
        Self {
            target_tokens,
            tolerance_percent,
            overlap_sentences,
            preserve_tables,
            preserve_code_blocks,
        }
    }

    fn __str__(&self) -> String {
        format!(
            "ChunkerConfig(target_tokens={}, overlap={}, tolerance={:.0}%)",
            self.target_tokens, self.overlap_sentences, self.tolerance_percent
        )
    }

    fn __repr__(&self) -> String {
        self.__str__()
    }

    fn to_json(&self) -> PyResult<String> {
        serde_json::to_string_pretty(self)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
    }
}

impl PyChunkerConfig {
    fn to_tl(&self) -> tl_chunk::ChunkerConfig {
        tl_chunk::ChunkerConfig {
            target_tokens: self.target_tokens,
            tolerance_percent: self.tolerance_percent,
            overlap_sentences: self.overlap_sentences,
            preserve_tables: self.preserve_tables,
            preserve_code_blocks: self.preserve_code_blocks,
        }
    }
}

/// Chunk a single document into semantic units.
///
/// Uses the touring-antt semantic chunker with ANTT-aware boundary detection.
#[pyfunction]
#[pyo3(signature = (text, config = None))]
pub fn py_chunk_document(text: &str, config: Option<&PyChunkerConfig>) -> Vec<PySemanticChunk> {
    let tl_config = config.map(|c| c.to_tl());
    let chunks = tl_chunk::chunk_document(text, tl_config);
    chunks.iter().map(PySemanticChunk::from_tl).collect()
}

/// Chunk multiple documents in parallel using Rayon (releases GIL).
#[pyfunction]
#[pyo3(signature = (texts, config = None))]
pub fn py_chunk_documents_batch(
    py: Python<'_>,
    texts: Vec<String>,
    config: Option<&PyChunkerConfig>,
) -> Vec<Vec<PySemanticChunk>> {
    let tl_config = config.map(|c| c.to_tl());

    py.detach(|| {
        let batch_results = tl_chunk::chunk_documents_batch(&texts, tl_config);
        batch_results
            .iter()
            .map(|doc_chunks| doc_chunks.iter().map(PySemanticChunk::from_tl).collect())
            .collect()
    })
}

// ═══════════════════════════════════════════════════════════════════════════
// Module registration
// ═══════════════════════════════════════════════════════════════════════════

/// Register all NLP PyO3 classes and functions in the parent module.
pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    // --- Monetary Parser ---
    m.add_class::<PyMonetaryValue>()?;
    m.add_function(wrap_pyfunction!(py_parse_monetary, m)?)?;
    m.add_function(wrap_pyfunction!(py_parse_monetary_batch, m)?)?;

    // --- Keyword Matcher ---
    m.add_class::<PyKeywordMatch>()?;
    m.add_class::<PyMatcherConfig>()?;
    m.add_class::<PyKeywordMatcher>()?;
    m.add_function(wrap_pyfunction!(py_levenshtein_distance, m)?)?;

    // --- Semantic Chunker ---
    m.add_class::<PySemanticChunk>()?;
    m.add_class::<PyChunkerConfig>()?;
    m.add_function(wrap_pyfunction!(py_chunk_document, m)?)?;
    m.add_function(wrap_pyfunction!(py_chunk_documents_batch, m)?)?;

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing
)]
mod tests {
    use super::*;

    #[test]
    fn test_monetary_value_from_tl() {
        let tl_val = tl_mon::MonetaryValue {
            value: 1000.0,
            currency: "BRL".to_string(),
            multiplier: 1.0,
            source_text: "R$ 1.000,00".to_string(),
            position: (5, 16),
            confidence: 0.95,
            context: "valor R$ 1.000,00 total".to_string(),
        };
        let py_val = PyMonetaryValue::from_tl(&tl_val);
        assert!((py_val.value - 1000.0).abs() < 0.01);
        assert_eq!(py_val.currency, "BRL");
        assert_eq!(py_val.position_start, 5);
        assert_eq!(py_val.position_end, 16);
        assert!((py_val.confidence - 0.95).abs() < 0.01);
    }

    #[test]
    fn test_keyword_match_from_tl() {
        let tl_match = tl_kw::KeywordMatch {
            keyword: "VPL".to_string(),
            pattern_index: 0,
            start: 10,
            end: 13,
            matched_text: "VPL".to_string(),
            context_before: "O ".to_string(),
            context_after: " foi".to_string(),
            match_score: 1.0,
            category: Some("Financial".to_string()),
        };
        let py_match = PyKeywordMatch::from_tl(&tl_match);
        assert_eq!(py_match.keyword, "VPL");
        assert_eq!(py_match.start, 10);
        assert_eq!(py_match.end, 13);
        assert_eq!(py_match.match_score, 1.0);
        assert_eq!(py_match.category, Some("Financial".to_string()));
    }

    #[test]
    fn test_matcher_config_to_tl() {
        let py_cfg = PyMatcherConfig {
            context_size: 80,
            case_insensitive: true,
            allow_overlapping: false,
            fuzzy_enabled: true,
            max_edit_distance: 3,
            min_fuzzy_score: 0.75,
        };
        let tl_cfg = py_cfg.to_tl();
        assert_eq!(tl_cfg.context_size, 80);
        assert!(tl_cfg.case_insensitive);
        assert!(!tl_cfg.allow_overlapping);
        assert!(tl_cfg.fuzzy_enabled);
        assert_eq!(tl_cfg.max_edit_distance, 3);
        assert!((tl_cfg.min_fuzzy_score - 0.75).abs() < 0.01);
    }

    #[test]
    fn test_chunker_config_to_tl() {
        let py_cfg = PyChunkerConfig {
            target_tokens: 256,
            tolerance_percent: 15.0,
            overlap_sentences: 1,
            preserve_tables: false,
            preserve_code_blocks: true,
        };
        let tl_cfg = py_cfg.to_tl();
        assert_eq!(tl_cfg.target_tokens, 256);
        assert!((tl_cfg.tolerance_percent - 15.0).abs() < 0.01);
        assert_eq!(tl_cfg.overlap_sentences, 1);
        assert!(!tl_cfg.preserve_tables);
        assert!(tl_cfg.preserve_code_blocks);
    }

    #[test]
    fn test_semantic_chunk_from_tl() {
        let tl_chunk = tl_chunk::SemanticChunk {
            content: "Hello world.".to_string(),
            byte_range: (0, 12),
            char_range: (0, 12),
            token_count: 4,
            start_boundary_name: "paragraph".to_string(),
            end_boundary_name: "paragraph".to_string(),
            start_boundary_level: 0,
            end_boundary_level: 0,
            context_before: String::new(),
            context_after: String::new(),
            chunk_index: 0,
            total_chunks: 1,
            section_path: vec!["Introduction".to_string()],
        };
        let py_chunk = PySemanticChunk::from_tl(&tl_chunk);
        assert_eq!(py_chunk.content, "Hello world.");
        assert_eq!(py_chunk.byte_range_start, 0);
        assert_eq!(py_chunk.byte_range_end, 12);
        assert_eq!(py_chunk.token_count, 4);
        assert_eq!(py_chunk.chunk_index, 0);
        assert_eq!(py_chunk.total_chunks, 1);
        assert_eq!(py_chunk.section_path, vec!["Introduction".to_string()]);
    }

    #[test]
    fn test_parse_monetary_integration() {
        let results = tl_mon::parse_monetary("R$ 1.000,00").unwrap();
        assert!(!results.is_empty());
        let py_results: Vec<PyMonetaryValue> =
            results.iter().map(PyMonetaryValue::from_tl).collect();
        assert!((py_results[0].value - 1000.0).abs() < 0.01);
    }

    #[test]
    fn test_chunk_document_integration() {
        let chunks = tl_chunk::chunk_document("# Title\n\nSome text here.", None);
        assert!(!chunks.is_empty());
        let py_chunks: Vec<PySemanticChunk> = chunks.iter().map(PySemanticChunk::from_tl).collect();
        assert!(!py_chunks.is_empty());
        assert!(py_chunks[0].content.contains("Title"));
    }

    #[test]
    fn test_levenshtein_via_binding() {
        assert_eq!(tl_kw::levenshtein_distance("hello", "hello"), 0);
        assert_eq!(tl_kw::levenshtein_distance("hello", "hallo"), 1);
    }
}
