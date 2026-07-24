//! GracefulChunker: wraps primary + fallback chunkers with chain-of-fallback.
//!
//! The primary chunker is tried first; on error the fallback is invoked.
//! This pattern ensures graceful degradation even when primary chunker
//! encounters binary files, parse errors, or resource limits.

use super::error::{ChunkError, ChunkResult};

/// Trait for chunking strategies. Implementors must be thread-safe.
pub trait Chunker: Send + Sync {
    /// Split content into chunks up to max_chunks.
    fn chunk(&self, content: &str, max_chunks: usize) -> Result<Vec<String>, ChunkError>;
}

/// Primary chunker using AST-aware splitting for structured content.
pub struct SemanticChunker {
    /// Target maximum chunk size in characters. The chunker
    /// splits on AST node boundaries that fit this size.
    max_chunk_size: usize,
    /// Maximum AST depth before falling back to the line-based
    /// chunker. Limits the cost of pathological nesting.
    depth_limit: usize,
}

impl SemanticChunker {
    /// Construct a new `SemanticChunker` with the given size
    /// and depth ceilings.
    pub fn new(max_chunk_size: usize, depth_limit: usize) -> Self {
        Self {
            max_chunk_size,
            depth_limit,
        }
    }

    fn is_binary_content(&self, content: &str) -> bool {
        // Binary indicators: null bytes OR high ratio of non-printable characters
        // Null bytes are a strong indicator of binary content
        if content.bytes().any(|b| b == 0) {
            return true;
        }
        let non_printable = content
            .bytes()
            .filter(|&b| b < 32 && b != b'\t' && b != b'\n' && b != b'\r')
            .count();
        let ratio = non_printable as f64 / content.len().max(1) as f64;
        ratio > 0.30
    }

    fn chunk_by_structure(&self, content: &str) -> Result<Vec<String>, ChunkError> {
        let mut chunks = Vec::new();
        let mut current = String::new();
        let mut brace_depth = 0usize;

        for line in content.lines() {
            // Track brace depth as proxy for AST nesting
            brace_depth = brace_depth.saturating_add(line.matches('{').count());
            brace_depth = brace_depth.saturating_sub(line.matches('}').count());

            if current.len() + line.len() + 1 > self.max_chunk_size
                || brace_depth > self.depth_limit
            {
                if !current.is_empty() {
                    chunks.push(current.clone());
                    current.clear();
                }
                // Reset depth on chunk boundary
                brace_depth = 0;
            }
            current.push_str(line);
            current.push('\n');

            // Reset depth at statement boundaries
            if line.trim().ends_with(';')
                || line.trim().starts_with("fn ")
                || line.trim().starts_with("pub ")
            {
                brace_depth = 0;
            }
        }

        if !current.is_empty() {
            chunks.push(current);
        }

        if chunks.is_empty() {
            chunks.push(content.to_string());
        }

        Ok(chunks)
    }
}

impl Chunker for SemanticChunker {
    fn chunk(&self, content: &str, max_chunks: usize) -> Result<Vec<String>, ChunkError> {
        if self.is_binary_content(content) {
            return Err(ChunkError::BinaryFile);
        }

        let chunks = self.chunk_by_structure(content)?;

        if chunks.len() > max_chunks {
            return Err(ChunkError::ChunkLimitExceeded {
                count: chunks.len(),
                limit: max_chunks,
            });
        }

        Ok(chunks)
    }
}

/// Fallback chunker using delimiter-based splitting.
/// Always succeeds — never returns an error.
pub struct DelimiterChunker {
    delimiter: String,
    max_chunk_size: usize,
}

impl DelimiterChunker {
    /// Construct a delimiter-based chunker that splits on
    /// `delimiter` and enforces `max_chunk_size` as an upper
    /// bound per chunk (oversized segments are slab-split).
    pub fn new(delimiter: &str, max_chunk_size: usize) -> Self {
        Self {
            delimiter: delimiter.to_string(),
            max_chunk_size,
        }
    }
}

impl Chunker for DelimiterChunker {
    fn chunk(&self, content: &str, _max_chunks: usize) -> Result<Vec<String>, ChunkError> {
        if content.is_empty() {
            return Ok(vec![]);
        }

        let mut chunks = Vec::new();

        for segment in content.split(&self.delimiter) {
            if segment.len() > self.max_chunk_size {
                // Split oversized segment into slabs
                for slab in segment.as_bytes().chunks(self.max_chunk_size) {
                    if let Ok(s) = std::str::from_utf8(slab) {
                        let trimmed = s.trim_end_matches('\n');
                        if !trimmed.is_empty() {
                            chunks.push(trimmed.to_string());
                        }
                    }
                }
            } else {
                let trimmed = segment.trim_end_matches('\n');
                if !trimmed.is_empty() {
                    chunks.push(trimmed.to_string());
                }
            }
        }

        if chunks.is_empty() {
            chunks.push(content.trim().to_string());
        }

        Ok(chunks)
    }
}

/// GracefulChunker wraps primary + fallback chunkers.
/// On primary failure, falls back to delimiter-based chunking.
pub struct GracefulChunker<P: Chunker, F: Chunker> {
    primary: P,
    fallback: F,
}

impl<P: Chunker, F: Chunker> GracefulChunker<P, F> {
    /// Construct a `GracefulChunker` from a primary and a
    /// fallback chunker. The fallback is invoked only when
    /// the primary returns an error.
    pub fn new(primary: P, fallback: F) -> Self {
        Self { primary, fallback }
    }
}

impl<P: Chunker + 'static, F: Chunker + 'static> Chunker for GracefulChunker<P, F> {
    fn chunk(&self, content: &str, max_chunks: usize) -> Result<Vec<String>, ChunkError> {
        match self.primary.chunk(content, max_chunks) {
            Ok(chunks) => Ok(chunks),
            Err(e) => {
                tracing::warn!(error = ?e, "primary chunker failed, falling back");
                self.fallback.chunk(content, max_chunks)
            }
        }
    }
}

/// Wrapper that adds metadata on top of the result.
pub struct ChunkingResult {
    /// The chunked text segments.
    pub chunks: Vec<String>,
    /// Whether the source was detected as binary.
    pub is_binary: bool,
    /// Whether the fallback chunker was used in place of the
    /// primary.
    pub fallback_used: bool,
}

impl GracefulChunker<SemanticChunker, DelimiterChunker> {
    /// Convenience constructor with default chunkers.
    pub fn with_defaults() -> Self {
        Self {
            primary: SemanticChunker::new(4096, 16),
            fallback: DelimiterChunker::new("\n\n", 4096),
        }
    }

    /// Chunk a file using primary Chunker, falling back to F on error.
    /// Returns (chunk_result, used_fallback).
    pub async fn chunk_file(&self, path: &std::path::Path) -> Result<ChunkResult, ChunkError> {
        use tokio::fs;
        let content = fs::read_to_string(path)
            .await
            .map_err(|e| ChunkError::Io(e.to_string()))?;
        Ok(self.chunk_with_metadata(&content, usize::MAX))
    }

    /// Chunk content with result metadata.
    pub fn chunk_with_metadata(&self, content: &str, max_chunks: usize) -> ChunkResult {
        let is_binary = matches!(
            self.primary.chunk(content, max_chunks),
            Err(ChunkError::BinaryFile)
        );

        let fallback_used = self.primary.chunk(content, max_chunks).is_err();

        let chunks = match self.chunk(content, max_chunks) {
            Ok(c) => c,
            Err(_) => vec![content.to_string()],
        };

        ChunkResult {
            chunks,
            is_binary,
            fallback_used,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Chunker trait tests ──────────────────────────────────────────────

    #[test]
    fn test_semantic_chunker_basic() {
        let c = SemanticChunker::new(1024, 16);
        let content = "fn main() {\n    println!(\"hello\");\n}";
        let result = c.chunk(content, 10).unwrap();
        assert!(!result.is_empty());
    }

    #[test]
    fn test_semantic_chunker_binary_detection() {
        let c = SemanticChunker::new(1024, 16);
        // Binary-like content
        let binary = (0..200)
            .map(|i| (i % 256) as u8 as char)
            .collect::<String>();
        let err = c.chunk(&binary, 10).unwrap_err();
        assert!(matches!(err, ChunkError::BinaryFile));
    }

    #[test]
    fn test_semantic_chunker_limit_exceeded() {
        let c = SemanticChunker::new(10, 1); // small chunk + depth
        let content = "fn a() { }\nfn b() { }\nfn c() { }\nfn d() { }\nfn e() { }";
        let err = c.chunk(content, 2).unwrap_err();
        assert!(matches!(err, ChunkError::ChunkLimitExceeded { count, limit: 2 } if count > 2));
    }

    #[test]
    fn test_delimiter_chunker_basic() {
        let c = DelimiterChunker::new("\n\n", 4096);
        let content = "part one\n\npart two\n\npart three";
        let result = c.chunk(content, 10).unwrap();
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn test_delimiter_chunker_oversized() {
        let c = DelimiterChunker::new("\n\n", 5); // very small
        let content = "abcdefghijklmnopqrstuvwxyz";
        let result = c.chunk(content, 10).unwrap();
        // Should split into multiple chunks
        assert!(result.len() >= 3);
    }

    #[test]
    fn test_delimiter_chunker_empty() {
        let c = DelimiterChunker::new("\n\n", 4096);
        let result = c.chunk("", 10).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_delimiter_chunker_empty_segments() {
        let c = DelimiterChunker::new("\n\n", 4096);
        let content = "\n\n\n\n";
        let result = c.chunk(content, 10).unwrap();
        // Should produce at least one chunk
        assert!(!result.is_empty() || result.len() > 0);
    }

    #[test]
    fn test_graceful_chunker_primary_success() {
        let g = GracefulChunker::new(
            SemanticChunker::new(1024, 16),
            DelimiterChunker::new("\n\n", 4096),
        );
        let content = "fn main() { println!(\"hi\"); }";
        let result = g.chunk(content, 10).unwrap();
        assert!(!result.is_empty());
        // Should NOT fall back
    }

    #[test]
    fn test_graceful_chunker_fallback_on_binary() {
        let g = GracefulChunker::new(
            SemanticChunker::new(1024, 16),
            DelimiterChunker::new("\n\n", 4096),
        );
        let binary = (0..200)
            .map(|i| (i % 256) as u8 as char)
            .collect::<String>();
        let result = g.chunk(&binary, 10);
        // Fallback should succeed
        assert!(result.is_ok());
    }

    #[test]
    fn test_graceful_with_defaults() {
        let g = GracefulChunker::with_defaults();
        let content = "fn main() { }\n\nfn other() { }";
        let result = g.chunk(content, 10).unwrap();
        assert!(!result.is_empty());
    }

    #[test]
    fn test_chunk_result_metadata_binary() {
        let g = GracefulChunker::with_defaults();
        let binary = (0..200)
            .map(|i| (i % 256) as u8 as char)
            .collect::<String>();
        let meta = g.chunk_with_metadata(&binary, 10);
        assert!(meta.is_binary);
        assert!(meta.fallback_used);
        assert!(!meta.chunks.is_empty());
    }

    #[test]
    fn test_chunk_result_metadata_text() {
        let g = GracefulChunker::with_defaults();
        let content = "fn main() { println!(\"hello\"); }";
        let meta = g.chunk_with_metadata(content, 10);
        assert!(!meta.is_binary);
        assert!(!meta.fallback_used);
    }

    #[test]
    fn test_chunk_result_fallback_preserves_content() {
        let g = GracefulChunker::with_defaults();
        let content = "first part\n\nsecond part";
        let meta = g.chunk_with_metadata(content, 10);
        let combined = meta.chunks.join(" ");
        assert!(combined.contains("first") && combined.contains("second"));
    }
}
