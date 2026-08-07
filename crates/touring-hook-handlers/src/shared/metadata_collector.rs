//! Fast metadata collection for file knowledge.
//!
//! Collects essential file metadata without full parse overhead.

use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

/// Fast metadata for a single file.
/// Fields match Pln2 spec: 11 type-specified fields.
#[derive(Debug, Clone, PartialEq)]
pub struct FastMetadata {
    /// Filesystem path of the collected file.
    pub file_path: PathBuf,
    /// File size in bytes.
    pub file_size_bytes: u64,
    /// Last-modified time as a Unix epoch timestamp in seconds.
    pub mtime_epoch: i64,
    /// BLAKE3 content hash, when computed (`None` if skipped).
    pub blake3: Option<[u8; 32]>,
    /// Detected source language, when known.
    pub language: Option<String>,
    /// Total lines of code (including blanks and comments).
    pub loc: u32,
    /// Comment lines of code.
    pub cloc: u32,
    /// Count of `TODO`/`FIXME`-style markers found in the file.
    pub todo_count: u32,
    /// Cargo/build feature flags referenced by the file.
    pub feature_flags: Vec<String>,
    /// Agent that owns this file, when attributed.
    pub owner_agent: Option<String>,
}

impl FastMetadata {
    /// Create from file path with basic stats.
    pub fn from_path(path: &Path) -> std::io::Result<Self> {
        let meta = std::fs::metadata(path)?;
        let mtime = meta.modified()?;
        let mtime_epoch = mtime
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);

        Ok(Self {
            file_path: path.to_path_buf(),
            file_size_bytes: meta.len(),
            mtime_epoch,
            blake3: None,
            language: None,
            loc: 0,
            cloc: 0,
            todo_count: 0,
            feature_flags: Vec::new(),
            owner_agent: None,
        })
    }

    /// Check if file was modified since last collection.
    #[inline]
    pub fn is_modified_since(&self, mtime_epoch: i64) -> bool {
        self.mtime_epoch != mtime_epoch
    }

    /// Enrich language from file extension.
    ///
    /// Uses the same extension-to-language mapping as the Tantivy indexer,
    /// so language is consistent across the enrichment pipeline.
    #[inline]
    #[cfg(feature = "tantivy-fts")]
    pub fn with_language(mut self) -> Self {
        use crate::tantivy_index::extension_to_language;
        let ext = self
            .file_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or_default();
        self.language = Some(extension_to_language(ext));
        self
    }

    /// Enrich feature flags by parsing Cargo.toml of the containing crate.
    ///
    /// Looks for `crates/<crate_name>/Cargo.toml` and extracts the `features`
    /// section. Falls back to empty vec if the file cannot be read or parsed.
    pub fn with_feature_flags(mut self) -> Self {
        self.feature_flags = parse_cargo_features(&self.file_path);
        self
    }

    /// Enrich cognitive score by querying the Tantivy index for all symbols in this file
    /// and computing the average cognitive_score across symbols.
    #[cfg(feature = "tantivy-fts")]
    pub fn with_cognitive_from_index(mut self) -> Self {
        // A raiz é DERIVADA do próprio `file_path` do coletor: ele é um builder
        // sem acesso ao runtime, mas conhece o arquivo que está descrevendo.
        // `normalize_project_root` faz walk-up por marcador real quando o caminho
        // é absoluto; num relativo devolve `$HOME`, que resolve para o índice
        // legado — degradação explícita, não silenciosa.
        let root = self
            .file_path
            .is_absolute()
            .then(|| touring_foundation::TouringConfig::normalize_project_root(&self.file_path));
        let idx = match crate::tantivy_index::tantivy_for(root.as_deref()) {
            Some(idx) => idx,
            None => return self,
        };

        // Query all symbols in this file using path as query
        let file_str = self.file_path.to_string_lossy();
        let query_str = format!(r#"file_path:"{file_str}""#);

        let hits = match idx.search(&query_str, 50) {
            Ok(hits) if !hits.is_empty() => hits,
            _ => return self,
        };

        // Average cognitive_score across all symbols in file
        let sum: f64 = hits.iter().filter_map(|h| h.cognitive_score).sum();
        let count = hits.len() as f64;

        if sum > 0.0 && count > 0.0 {
            let avg = sum / count;
            self.language = Some(format!("cognitive:{:.2}", avg));
        }
        self
    }

    /// No-op fallback when the `tantivy-fts` feature is disabled — returns
    /// `self` unchanged (cognitive enrichment needs the Tantivy index).
    #[cfg(not(feature = "tantivy-fts"))]
    pub fn with_cognitive_from_index(self) -> Self {
        self
    }
}

// ─── Cargo Feature Extraction ───────────────────────────────────────────────

/// Parse the `features` array from a Cargo.toml file given the file path.
///
/// Walks up to find `crates/<name>/Cargo.toml`, reads it, and extracts feature names.
/// Returns empty vec on any failure.
fn parse_cargo_features(file_path: &Path) -> Vec<String> {
    let Some(crate_toml) = file_path
        .ancestors()
        .find(|p| p.file_name().is_some_and(|n| n == "crates"))
        .and_then(|p| p.parent())
        .map(|p| p.join("Cargo.toml"))
    else {
        return Vec::new();
    };

    let content = std::fs::read_to_string(crate_toml).unwrap_or_default();
    extract_features_from_toml(&content)
}

/// Extract feature names from Cargo.toml content lines.
fn extract_features_from_toml(content: &str) -> Vec<String> {
    let mut features = Vec::new();
    let mut in_features = false;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("features =") {
            in_features = true;
        } else if in_features && trimmed.starts_with('"') {
            if let Some(end) = trimmed.find('"') {
                let rest = &trimmed[end + 1..];
                if let Some(end2) = rest.find('"') {
                    features.push(rest[..end2].to_string());
                }
            }
        } else if in_features && !trimmed.is_empty() && !trimmed.starts_with('"') {
            in_features = false;
        }
    }
    features
}

/// Token budget for metadata enrichment.
#[derive(Debug, Clone)]
pub struct TokenBudget {
    /// Maximum tokens to use.
    pub max_tokens: usize,
    /// Current usage tracking.
    used_tokens: usize,
}

impl TokenBudget {
    /// Create new budget with max token limit.
    #[inline]
    pub fn new(max_tokens: usize) -> Self {
        Self {
            max_tokens,
            used_tokens: 0,
        }
    }

    /// Default pre_read budget (2000 tokens).
    #[inline]
    pub fn pre_read() -> Self {
        Self::new(2000)
    }

    /// Default pre_edit budget (500 tokens).
    #[inline]
    pub fn pre_edit() -> Self {
        Self::new(500)
    }

    /// Check if budget has remaining capacity.
    #[inline]
    pub fn has_remaining(&self) -> bool {
        self.used_tokens < self.max_tokens
    }

    /// Allocate tokens from budget.
    /// Returns actual tokens allocated (may be less if budget exhausted).
    pub fn allocate(&mut self, tokens: usize) -> usize {
        let remaining = self.max_tokens - self.used_tokens;
        let allocated = tokens.min(remaining);
        self.used_tokens += allocated;
        allocated
    }

    /// Estimate tokens from character count (heuristic: chars/4).
    #[inline]
    pub fn estimate_from_chars(chars: usize) -> usize {
        chars / 4
    }

    /// Get remaining budget.
    #[inline]
    pub fn remaining(&self) -> usize {
        self.max_tokens - self.used_tokens
    }

    /// Waterfall: drop with rationale.
    pub fn drop(&mut self, _reason: &str) {
        // Waterfall strategy would be implemented here
        // For now just mark as exhausted
        self.used_tokens = self.max_tokens;
    }
}

impl Default for TokenBudget {
    fn default() -> Self {
        Self::pre_read()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_budget_basic() {
        let mut budget = TokenBudget::new(1000);
        assert!(budget.has_remaining());
        assert_eq!(budget.allocate(300), 300);
        assert_eq!(budget.remaining(), 700);
    }

    #[test]
    fn token_budget_exhausted() {
        let mut budget = TokenBudget::new(100);
        assert_eq!(budget.allocate(150), 100);
        assert!(!budget.has_remaining());
    }
}
