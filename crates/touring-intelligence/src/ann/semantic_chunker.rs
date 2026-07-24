//! Semantic Chunker - Pln2 Module 2
//!
//! High-performance semantic text chunking for ANTT regulatory documents.
//!
//! # Features
//!
//! - **Unicode sentence boundaries**: UAX #29 compliant
//! - **ANTT patterns**: Legal articles, paragraphs, incisos, technical notes
//! - **Structure preservation**: Tables, code blocks, block quotes
//! - **Token estimation**: ~4 chars/token for Portuguese
//! - **Overlap**: Configurable sentence-based overlap
//! - **Section tracking**: Hierarchical heading paths

use once_cell::sync::Lazy;
use regex::Regex;
use unicode_segmentation::UnicodeSegmentation;

// ============================================================================
// Types
// ============================================================================

/// Type of semantic boundary detected in text.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum BoundaryType {
    /// Markdown heading (# ## ###)
    Heading {
        /// Heading level (1-6)
        level: u8,
    },
    /// Regular paragraph
    #[default]
    Paragraph,
    /// Bullet or numbered list item
    ListItem,
    /// Table row (| col | col |)
    TableRow,
    /// Block quote (> text)
    BlockQuote,
    /// Code block (```)
    CodeBlock,
    /// Legal article (Art. 1º)
    LegalArticle,
    /// Legal paragraph (§ 1º)
    LegalParagraph,
    /// Legal inciso (I, II, III)
    LegalInciso,
    /// Legal alínea (a), b), c))
    LegalAlinea,
    /// Signature block
    Signature,
}

impl BoundaryType {
    /// Get the string name of this boundary type.
    pub fn name(&self) -> &'static str {
        match self {
            BoundaryType::Heading { .. } => "heading",
            BoundaryType::Paragraph => "paragraph",
            BoundaryType::ListItem => "list_item",
            BoundaryType::TableRow => "table_row",
            BoundaryType::BlockQuote => "block_quote",
            BoundaryType::CodeBlock => "code_block",
            BoundaryType::LegalArticle => "legal_article",
            BoundaryType::LegalParagraph => "legal_paragraph",
            BoundaryType::LegalInciso => "legal_inciso",
            BoundaryType::LegalAlinea => "legal_alinea",
            BoundaryType::Signature => "signature",
        }
    }

    /// Get heading level (1-6) or 0 if not a heading.
    pub(crate) fn heading_level(&self) -> u8 {
        match self {
            BoundaryType::Heading { level } => *level,
            _ => 0,
        }
    }
}

/// Semantic chunk with rich metadata.
#[derive(Debug, Clone)]
pub struct SemanticChunk {
    /// Text content of the chunk
    pub content: String,

    /// Byte range in original document (start, end)
    pub byte_range: (usize, usize),

    /// Character range in original document (start, end)
    pub char_range: (usize, usize),

    /// Estimated token count
    pub token_count: usize,

    /// Boundary type at chunk start (as string)
    pub start_boundary_name: String,

    /// Boundary type at chunk end (as string)
    pub end_boundary_name: String,

    /// Start boundary heading level (0 if not heading)
    pub start_boundary_level: u8,

    /// End boundary heading level (0 if not heading)
    pub end_boundary_level: u8,

    /// Context from previous chunk (overlap)
    pub context_before: String,

    /// Context for next chunk (overlap)
    pub context_after: String,

    /// Zero-based chunk index
    pub chunk_index: usize,

    /// Total number of chunks in document
    pub total_chunks: usize,

    /// Hierarchical section path (heading titles)
    pub section_path: Vec<String>,
}

impl SemanticChunk {
    /// Get the start boundary type.
    pub fn start_boundary(&self) -> BoundaryType {
        self.boundary_from_name(&self.start_boundary_name, self.start_boundary_level)
    }

    /// Get the end boundary type.
    pub fn end_boundary(&self) -> BoundaryType {
        self.boundary_from_name(&self.end_boundary_name, self.end_boundary_level)
    }

    fn boundary_from_name(&self, name: &str, level: u8) -> BoundaryType {
        match name {
            "heading" => BoundaryType::Heading { level },
            "paragraph" => BoundaryType::Paragraph,
            "list_item" => BoundaryType::ListItem,
            "table_row" => BoundaryType::TableRow,
            "block_quote" => BoundaryType::BlockQuote,
            "code_block" => BoundaryType::CodeBlock,
            "legal_article" => BoundaryType::LegalArticle,
            "legal_paragraph" => BoundaryType::LegalParagraph,
            "legal_inciso" => BoundaryType::LegalInciso,
            "legal_alinea" => BoundaryType::LegalAlinea,
            "signature" => BoundaryType::Signature,
            _ => BoundaryType::Paragraph,
        }
    }
}

/// Configuration for the semantic chunker.
#[derive(Debug, Clone)]
pub struct ChunkerConfig {
    /// Target chunk size in tokens (not characters)
    pub target_tokens: usize,

    /// Tolerance for exceeding target (percentage)
    pub tolerance_percent: f64,

    /// Number of sentences to overlap between chunks
    pub overlap_sentences: usize,

    /// Whether to preserve table rows together
    pub preserve_tables: bool,

    /// Whether to preserve code blocks together
    pub preserve_code_blocks: bool,
}

impl Default for ChunkerConfig {
    fn default() -> Self {
        Self {
            target_tokens: 512,
            tolerance_percent: 20.0,
            overlap_sentences: 2,
            preserve_tables: true,
            preserve_code_blocks: true,
        }
    }
}

// ============================================================================
// Boundary Patterns (Lazy static compiled regex)
// ============================================================================

/// Compiled regex patterns for boundary detection.
pub(crate) struct BoundaryPatterns {
    pub markdown_heading: Regex,
    pub numbered_heading: Regex,
    pub legal_article: Regex,
    pub legal_paragraph: Regex,
    pub legal_inciso: Regex,
    pub legal_alinea: Regex,
    pub nota_tecnica: Regex,
    pub parecer: Regex,
    pub relatorio: Regex,
    pub table_row: Regex,
    pub code_fence: Regex,
    pub list_bullet: Regex,
    pub list_numbered: Regex,
    pub block_quote: Regex,
    pub signature: Regex,
}

/// Pre-compiled boundary patterns for ANTT documents.
pub(crate) static BOUNDARY_PATTERNS: Lazy<BoundaryPatterns> = Lazy::new(|| BoundaryPatterns {
    markdown_heading: Regex::new(r"^(#{1,6})\s+(.+)$").expect("static regex is valid"),
    numbered_heading: Regex::new(r"^(\d+(?:\.\d+)*\.?)\s+(.+)$").expect("static regex is valid"),
    legal_article: Regex::new(r"(?i)^(?:Art\.|Artigo)\s*(\d+)[°º]?\.?\s*[-–]?\s*(.*)$")
        .expect("static regex is valid"),
    legal_paragraph: Regex::new(
        r"(?i)^(?:§\s*(\d+)[°º]?|Par[aá]grafo\s+[úu]nico)\.?\s*[-–]?\s*(.*)$",
    )
    .expect("static regex is valid"),
    legal_inciso: Regex::new(r"^([IVXLCDM]+)\s*[-–]\s*(.+)$").expect("static regex is valid"),
    legal_alinea: Regex::new(r"^([a-z])\)\s*(.+)$").expect("static regex is valid"),
    nota_tecnica: Regex::new(r"(?i)^NOTA\s+T[ÉE]CNICA\s+N[°º]?\s*[\d./-]+.*$")
        .expect("static regex is valid"),
    parecer: Regex::new(r"(?i)^PARECER\s+(?:PF-ANTT|PRGREG|PROCAD)\s+N[°º]?\s*[\d./-]+.*$")
        .expect("static regex is valid"),
    relatorio: Regex::new(r"(?i)^RELAT[ÓO]RIO\s+(?:[ÀA]\s+)?DIRETORIA.*$")
        .expect("static regex is valid"),
    table_row: Regex::new(r"^\|.*\|$").expect("static regex is valid"),
    code_fence: Regex::new(r"^```").expect("static regex is valid"),
    list_bullet: Regex::new(r"^[-*+]\s+(.+)$").expect("static regex is valid"),
    list_numbered: Regex::new(r"^(\d+)\.\s+(.+)$").expect("static regex is valid"),
    block_quote: Regex::new(r"^>\s*(.*)$").expect("static regex is valid"),
    signature: Regex::new(
        r"(?i)(?:Atenciosamente|Respeitosamente|Assinado\s+digitalmente|Documento\s+assinado)",
    )
    .expect("static regex is valid"),
});

// ============================================================================
// Token Estimator
// ============================================================================

/// Token count estimator (approximation for Portuguese text).
#[derive(Debug, Clone, Default)]
pub struct TokenEstimator;

impl TokenEstimator {
    /// Create a new token estimator.
    pub fn new() -> Self {
        Self
    }

    /// Estimate token count for text.
    pub fn estimate(&self, text: &str) -> usize {
        if text.is_empty() {
            return 0;
        }

        let char_count = text.chars().count();
        let word_count = text.split_whitespace().count();

        let estimate = (char_count as f64 / 4.0) + (word_count as f64 * 0.3);
        estimate.max(1.0) as usize
    }
}

// ============================================================================
// Segment (internal)
// ============================================================================

/// Internal segment representation.
#[derive(Clone, Debug)]
struct Segment {
    content: String,
    byte_range: (usize, usize),
    char_range: (usize, usize),
    boundary_type: BoundaryType,
    heading_text: Option<String>,
}

// ============================================================================
// Semantic Chunker
// ============================================================================

/// Semantic chunker for ANTT regulatory documents.
#[derive(Debug, Clone)]
pub struct SemanticChunker {
    config: ChunkerConfig,
    token_estimator: TokenEstimator,
}

impl SemanticChunker {
    /// Create a new semantic chunker with the given configuration.
    pub fn new(config: ChunkerConfig) -> Self {
        Self {
            config,
            token_estimator: TokenEstimator::new(),
        }
    }

    /// Detect the boundary type of a line.
    pub(crate) fn detect_boundary(&self, line: &str) -> BoundaryType {
        let trimmed = line.trim();

        if trimmed.is_empty() {
            return BoundaryType::Paragraph;
        }

        // Check ANTT-specific patterns first (highest priority)
        if BOUNDARY_PATTERNS.nota_tecnica.is_match(trimmed) {
            return BoundaryType::Heading { level: 1 };
        }

        if BOUNDARY_PATTERNS.parecer.is_match(trimmed) {
            return BoundaryType::Heading { level: 1 };
        }

        if BOUNDARY_PATTERNS.relatorio.is_match(trimmed) {
            return BoundaryType::Heading { level: 1 };
        }

        // Markdown heading
        if let Some(caps) = BOUNDARY_PATTERNS.markdown_heading.captures(trimmed) {
            return BoundaryType::Heading {
                level: caps[1].len() as u8,
            };
        }

        // Legal patterns
        if BOUNDARY_PATTERNS.legal_article.is_match(trimmed) {
            return BoundaryType::LegalArticle;
        }

        if BOUNDARY_PATTERNS.legal_paragraph.is_match(trimmed) {
            return BoundaryType::LegalParagraph;
        }

        if BOUNDARY_PATTERNS.legal_inciso.is_match(trimmed) {
            return BoundaryType::LegalInciso;
        }

        if BOUNDARY_PATTERNS.legal_alinea.is_match(trimmed) {
            return BoundaryType::LegalAlinea;
        }

        // Table row
        if BOUNDARY_PATTERNS.table_row.is_match(trimmed) {
            return BoundaryType::TableRow;
        }

        // Code fence
        if BOUNDARY_PATTERNS.code_fence.is_match(trimmed) {
            return BoundaryType::CodeBlock;
        }

        // Block quote
        if BOUNDARY_PATTERNS.block_quote.is_match(trimmed) {
            return BoundaryType::BlockQuote;
        }

        // List items
        if BOUNDARY_PATTERNS.list_bullet.is_match(trimmed)
            || BOUNDARY_PATTERNS.list_numbered.is_match(trimmed)
        {
            return BoundaryType::ListItem;
        }

        // Signature
        if BOUNDARY_PATTERNS.signature.is_match(trimmed) {
            return BoundaryType::Signature;
        }

        BoundaryType::Paragraph
    }

    /// Chunk document into semantic units.
    pub fn chunk(&self, text: &str) -> Vec<SemanticChunk> {
        if text.trim().is_empty() {
            return vec![];
        }

        let segments = self.segment_document(text);

        if segments.is_empty() {
            return vec![];
        }

        let mut chunks = Vec::new();
        let mut current_segments: Vec<Segment> = Vec::new();
        let mut current_tokens = 0usize;
        let mut section_path: Vec<String> = Vec::new();

        let max_tokens = (self.config.target_tokens as f64
            * (1.0 + self.config.tolerance_percent / 100.0)) as usize;

        for segment in segments {
            if let BoundaryType::Heading { level } = &segment.boundary_type {
                let level_idx = (*level as usize).saturating_sub(1);
                section_path.truncate(level_idx);
                if let Some(heading) = &segment.heading_text {
                    section_path.push(heading.clone());
                }
            }

            let segment_tokens = self.token_estimator.estimate(&segment.content);

            let would_exceed =
                current_tokens + segment_tokens > max_tokens && !current_segments.is_empty();

            if would_exceed {
                chunks.push(self.create_chunk(&current_segments, chunks.len(), &section_path));

                current_segments = self.get_overlap_segments(&current_segments);
                current_tokens = current_segments
                    .iter()
                    .map(|s| self.token_estimator.estimate(&s.content))
                    .sum();
            }

            current_segments.push(segment);
            current_tokens += segment_tokens;
        }

        if !current_segments.is_empty() {
            chunks.push(self.create_chunk(&current_segments, chunks.len(), &section_path));
        }

        let total = chunks.len();
        for i in 0..total {
            // SAFETY: i is bounded by total which equals chunks.len()
            if let Some(chunk) = chunks.get_mut(i) {
                chunk.total_chunks = total;
            }

            // Extract context_after from the next chunk's content
            if i + 1 < total {
                let next_content = chunks
                    .get(i + 1)
                    .map(|c| self.extract_context_from_start(&c.content))
                    .unwrap_or_default();
                if let Some(chunk) = chunks.get_mut(i) {
                    chunk.context_after = next_content;
                }
            }
        }

        chunks
    }

    fn segment_document(&self, text: &str) -> Vec<Segment> {
        let mut segments = Vec::new();
        let mut in_code_block = false;
        let mut code_block_lines: Vec<String> = Vec::new();
        let mut code_block_start_byte = 0usize;
        let mut code_block_start_char = 0usize;

        let mut in_table = false;
        let mut table_lines: Vec<String> = Vec::new();
        let mut table_start_byte = 0usize;
        let mut table_start_char = 0usize;

        let mut byte_offset = 0usize;
        let mut char_offset = 0usize;

        for line in text.lines() {
            let line_bytes = line.len();
            let line_chars = line.chars().count();

            let boundary = self.detect_boundary(line);

            if boundary == BoundaryType::CodeBlock {
                if in_code_block {
                    code_block_lines.push(line.to_string());
                    if self.config.preserve_code_blocks {
                        segments.push(Segment {
                            content: code_block_lines.join("\n"),
                            byte_range: (code_block_start_byte, byte_offset + line_bytes),
                            char_range: (code_block_start_char, char_offset + line_chars),
                            boundary_type: BoundaryType::CodeBlock,
                            heading_text: None,
                        });
                    }
                    code_block_lines.clear();
                    in_code_block = false;
                } else {
                    code_block_start_byte = byte_offset;
                    code_block_start_char = char_offset;
                    code_block_lines.push(line.to_string());
                    in_code_block = true;
                }
                byte_offset += line_bytes + 1;
                char_offset += line_chars + 1;
                continue;
            }

            if in_code_block {
                code_block_lines.push(line.to_string());
                byte_offset += line_bytes + 1;
                char_offset += line_chars + 1;
                continue;
            }

            if boundary == BoundaryType::TableRow {
                if !in_table {
                    table_start_byte = byte_offset;
                    table_start_char = char_offset;
                    in_table = true;
                }
                table_lines.push(line.to_string());
                byte_offset += line_bytes + 1;
                char_offset += line_chars + 1;
                continue;
            } else if in_table {
                if self.config.preserve_tables && !table_lines.is_empty() {
                    segments.push(Segment {
                        content: table_lines.join("\n"),
                        byte_range: (table_start_byte, byte_offset),
                        char_range: (table_start_char, char_offset),
                        boundary_type: BoundaryType::TableRow,
                        heading_text: None,
                    });
                }
                table_lines.clear();
                in_table = false;
            }

            let heading_text = self.extract_heading_text(line);

            segments.push(Segment {
                content: line.to_string(),
                byte_range: (byte_offset, byte_offset + line_bytes),
                char_range: (char_offset, char_offset + line_chars),
                boundary_type: boundary,
                heading_text,
            });

            byte_offset += line_bytes + 1;
            char_offset += line_chars + 1;
        }

        if in_table && self.config.preserve_tables && !table_lines.is_empty() {
            segments.push(Segment {
                content: table_lines.join("\n"),
                byte_range: (table_start_byte, byte_offset),
                char_range: (table_start_char, char_offset),
                boundary_type: BoundaryType::TableRow,
                heading_text: None,
            });
        }

        if in_code_block && self.config.preserve_code_blocks && !code_block_lines.is_empty() {
            segments.push(Segment {
                content: code_block_lines.join("\n"),
                byte_range: (code_block_start_byte, byte_offset),
                char_range: (code_block_start_char, char_offset),
                boundary_type: BoundaryType::CodeBlock,
                heading_text: None,
            });
        }

        segments
    }

    fn extract_heading_text(&self, line: &str) -> Option<String> {
        let trimmed = line.trim();

        if let Some(caps) = BOUNDARY_PATTERNS.markdown_heading.captures(trimmed) {
            return Some(caps[2].to_string());
        }

        if let Some(caps) = BOUNDARY_PATTERNS.numbered_heading.captures(trimmed) {
            return Some(caps[2].to_string());
        }

        if BOUNDARY_PATTERNS.nota_tecnica.is_match(trimmed)
            || BOUNDARY_PATTERNS.parecer.is_match(trimmed)
            || BOUNDARY_PATTERNS.relatorio.is_match(trimmed)
        {
            return Some(trimmed.to_string());
        }

        None
    }

    fn create_chunk(
        &self,
        segments: &[Segment],
        index: usize,
        section_path: &[String],
    ) -> SemanticChunk {
        let content = segments
            .iter()
            .map(|s| s.content.as_str())
            .collect::<Vec<_>>()
            .join("\n");

        let first = segments
            .first()
            .expect("create_chunk called with non-empty segments");
        let last = segments
            .last()
            .expect("create_chunk called with non-empty segments");

        SemanticChunk {
            content: content.clone(),
            byte_range: (first.byte_range.0, last.byte_range.1),
            char_range: (first.char_range.0, last.char_range.1),
            token_count: self.token_estimator.estimate(&content),
            start_boundary_name: first.boundary_type.name().to_string(),
            end_boundary_name: last.boundary_type.name().to_string(),
            start_boundary_level: first.boundary_type.heading_level(),
            end_boundary_level: last.boundary_type.heading_level(),
            context_before: if index == 0 {
                String::new()
            } else {
                self.extract_context_from_end(&content)
            },
            context_after: String::new(),
            chunk_index: index,
            total_chunks: 0,
            section_path: section_path.to_vec(),
        }
    }

    fn extract_context_from_start(&self, text: &str) -> String {
        text.unicode_sentences()
            .take(self.config.overlap_sentences)
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn extract_context_from_end(&self, text: &str) -> String {
        let sentences: Vec<_> = text.unicode_sentences().collect();
        let start = sentences
            .len()
            .saturating_sub(self.config.overlap_sentences);
        sentences.get(start..).unwrap_or_default().join(" ")
    }

    fn get_overlap_segments(&self, segments: &[Segment]) -> Vec<Segment> {
        if self.config.overlap_sentences == 0 {
            return vec![];
        }

        let mut total_sentences = 0usize;
        let mut overlap = Vec::new();

        for segment in segments.iter().rev() {
            let seg_sentences = segment.content.unicode_sentences().count();
            if total_sentences + seg_sentences <= self.config.overlap_sentences {
                overlap.insert(0, segment.clone());
                total_sentences += seg_sentences;
            } else if total_sentences < self.config.overlap_sentences {
                overlap.insert(0, segment.clone());
                break;
            } else {
                break;
            }
        }

        overlap
    }
}

// ============================================================================
// Public convenience functions (used by rlm_integration)
// ============================================================================

/// Chunk a single document (convenience function).
pub fn chunk_document(text: &str, config: Option<ChunkerConfig>) -> Vec<SemanticChunk> {
    let config = config.unwrap_or_default();
    let chunker = SemanticChunker::new(config);
    chunker.chunk(text)
}

/// Chunk multiple documents (convenience function).
pub fn chunk_documents_batch(
    texts: &[String],
    config: Option<ChunkerConfig>,
) -> Vec<Vec<SemanticChunk>> {
    use rayon::prelude::*;

    let config = config.unwrap_or_default();

    texts
        .par_iter()
        .map(|text| {
            let chunker = SemanticChunker::new(config.clone());
            chunker.chunk(text)
        })
        .collect()
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_estimator_basic() {
        let estimator = TokenEstimator::new();
        assert_eq!(estimator.estimate(""), 0);
        assert!(estimator.estimate("Hello") > 0);
    }

    #[test]
    fn test_boundary_detection_heading() {
        let chunker = SemanticChunker::new(ChunkerConfig::default());
        assert_eq!(
            chunker.detect_boundary("# Title"),
            BoundaryType::Heading { level: 1 }
        );
        assert_eq!(
            chunker.detect_boundary("### Sub"),
            BoundaryType::Heading { level: 3 }
        );
    }

    #[test]
    fn test_boundary_detection_legal() {
        let chunker = SemanticChunker::new(ChunkerConfig::default());
        assert_eq!(
            chunker.detect_boundary("Art. 1º - Test"),
            BoundaryType::LegalArticle
        );
    }

    #[test]
    fn test_chunk_basic() {
        let chunker = SemanticChunker::new(ChunkerConfig::default());
        let chunks = chunker.chunk("Hello world.");
        assert!(!chunks.is_empty());
        assert!(chunks[0].content.contains("Hello"));
    }

    #[test]
    fn test_default_config() {
        let config = ChunkerConfig::default();
        assert_eq!(config.target_tokens, 512);
        assert_eq!(config.overlap_sentences, 2);
    }

    #[test]
    fn test_boundary_type_name() {
        assert_eq!(BoundaryType::Paragraph.name(), "paragraph");
        assert_eq!(BoundaryType::Heading { level: 2 }.name(), "heading");
        assert_eq!(BoundaryType::LegalArticle.name(), "legal_article");
    }
}
