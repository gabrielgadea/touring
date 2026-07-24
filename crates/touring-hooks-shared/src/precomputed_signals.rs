//! Precomputed Signals Cache — read → edit/write pipeline optimization.
//!
//! Philosophy: **compute once, reuse many**.
//!
//! ## Dual Cache Architecture
//!
//! This module provides the **structured signals cache** (O(1) lookup).
//! It's paired with `PersistedAnnMemoryRecall` (ANN similarity search) for
//! a complete dual-cache strategy:
//!
//! | Cache | Storage | Lookup | Use Case |
//! |-------|---------|--------|----------|
//! | PrecomputedSignals | HookResultCache (moka) | O(1) key-value | Wiring, ecosystem, gotchas |
//! | PersistedAnnMemoryRecall | SQLite + in-memory | ANN similarity | Similar files had similar errors |
//!
//! After `post_read` populates the `FileKnowledgeDB`, we precompute the
//! file-level signals that `pre_edit` and `pre_write` would query.
//! This turns N sequential DB queries into 1 O(1) cache lookup.
//!
//! ## Cache Strategy
//!
//! - Key: `"__precomputed:{rel_path}"` in `HookResultCache`
//! - Value: JSON array of precomputed `(score, signal_text)` tuples
//! - Invalidated by: `file-changed` lifecycle hook (same as result_cache)
//!
//! ## What is Precomputed (file-level signals)
//!
//! These are computed once in `post_read` and reused by `pre_edit`/`pre_write`:
//! - `wiring_signal` — orphan pub symbol warnings
//! - `ecosystem_signals` — module ecosystem scores
//! - `gotcha_signals` — file-specific pitfalls (BM25-ranked)
//! - `dependents_signal` — files that depend on this module
//! - `notes_signal` — accumulated notes about this file
//! - `risk_signal` — file risk scores
//! - `blast_radius_signal` — transitive dependency count (file-level, not edit-specific)
//! - `file_digest_signal` — AST symbol count + max complexity summary
//!
//! ## What is NOT Precomputed (edit-level signals)
//!
//! These require edit-specific context and must be computed per-edit:
//! - `quality_evolution_signal` — needs old_string/new_string
//! - `speculative_validation_signal` — needs new_string content
//! - `similar_symbol_signal` — needs SymbolIndex (infra)
//! - `prediction_signal` — needs ErrorPredictor (learning)
//! - `prevention_signal` — needs edit-specific gotcha matching

use serde::{Deserialize, Serialize};

/// A precomputed signal with its score, ready for budget truncation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrecomputedSignal(pub f32, pub String);

/// Precomputed signals for a file, stored in HookResultCache.
///
/// Computed once in `post_read`, consumed by `pre_edit` and `pre_write`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PrecomputedSignals {
    /// Scored signals computed once in `post_read`, ordered for budget truncation.
    pub signals: Vec<PrecomputedSignal>,
    /// SHA256 of content at time of precomputation — used for staleness check.
    pub content_hash: Option<String>,
}

impl PrecomputedSignals {
    /// Create a new precomputed signals container.
    pub fn new(signals: Vec<PrecomputedSignal>, content_hash: Option<String>) -> Self {
        Self {
            signals,
            content_hash,
        }
    }

    /// Returns true if this cache entry is stale (content hash mismatch).
    pub fn is_stale(&self, current_content_hash: Option<&str>) -> bool {
        match (&self.content_hash, current_content_hash) {
            (Some(cached), Some(current)) => cached != current,
            _ => true, // No hash available = assume stale
        }
    }

    /// Merge precomputed signals with edit-specific signals.
    ///
    /// Precomputed signals come first (file-level), then edit-specific signals.
    pub fn merge_with_edit_signals(self, edit_signals: Vec<(f32, String)>) -> Vec<(f32, String)> {
        let mut result = Vec::with_capacity(self.signals.len() + edit_signals.len());
        result.extend(self.signals.into_iter().map(|s| (s.0, s.1)));
        result.extend(edit_signals);
        result
    }
}

/// Compute a file digest signal from AST metadata.
///
/// Summarizes symbol count, max cyclomatic complexity, and language in a compact
/// string suitable for context injection. Helps the LLM decide whether to read
/// the full file or target specific sections.
///
/// Returns `None` if AST parsing fails or the file has no symbols.
pub fn file_digest_signal(
    source: &str,
    lang: touring_code::ast::languages::Lang,
) -> Option<(f32, String)> {
    let complexities =
        touring_code::ast::complexity::compute_complexity_for_source(source, lang).ok()?;
    if complexities.is_empty() {
        return None;
    }

    let symbol_count = complexities.len();
    let max_cc = complexities.iter().map(|(_, cc)| *cc).max().unwrap_or(1);
    let avg_cc = complexities.iter().map(|(_, cc)| *cc as f64).sum::<f64>() / symbol_count as f64;
    let hot_fns: Vec<&str> = complexities
        .iter()
        .filter(|(_, cc)| *cc > 10)
        .map(|(name, _)| name.as_str())
        .take(3)
        .collect();

    let hot_str = if hot_fns.is_empty() {
        String::new()
    } else {
        format!(", hot: {}", hot_fns.join("/"))
    };

    let line_count = source.lines().count();

    // Score higher for complex files (more value in the digest)
    let score = 0.5 + (max_cc as f32 / 30.0).min(0.5);

    Some((
        score,
        format!(
            "digest({line_count}L, {symbol_count} symbols, CC avg={avg_cc:.0} max={max_cc}{hot_str})"
        ),
    ))
}

/// Cache key prefix for precomputed signals.
pub const PRECOMPUTED_SIGNALS_PREFIX: &str = "__precomputed:";

/// Build the cache key for precomputed signals of a given file.
pub fn cache_key(rel_path: &str) -> String {
    format!("{}{}", PRECOMPUTED_SIGNALS_PREFIX, rel_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_key_format() {
        assert_eq!(cache_key("src/main.rs"), "__precomputed:src/main.rs");
    }

    #[test]
    fn test_precomputed_signals_merge() {
        let precomputed = PrecomputedSignals::new(
            vec![
                PrecomputedSignal(0.9, "wiring: orphan pub symbol".to_string()),
                PrecomputedSignal(0.7, "ecosystem: low integration score".to_string()),
            ],
            Some("abc123".to_string()),
        );

        let edit_signals = vec![
            (1.0, "blast_radius: 5 files".to_string()),
            (0.8, "quality_evolution: complexity increased".to_string()),
        ];

        let merged = precomputed.merge_with_edit_signals(edit_signals);

        assert_eq!(merged.len(), 4);
        assert_eq!(merged[0], (0.9, "wiring: orphan pub symbol".to_string()));
        assert_eq!(merged[2], (1.0, "blast_radius: 5 files".to_string()));
    }

    #[test]
    fn test_file_digest_simple_rust() {
        let src = r#"
fn simple() { let x = 1; }
fn complex(a: bool, b: bool) {
    if a {
        if b {
            for i in 0..10 {
                if i > 5 { break; }
            }
        }
    }
}
"#;
        let result = file_digest_signal(src, touring_code::ast::languages::Lang::Rust);
        assert!(result.is_some(), "should produce digest for valid Rust");
        let (score, text) = result.unwrap();
        assert!(score >= 0.5, "score should be >= 0.5");
        assert!(text.contains("digest("), "should contain digest prefix");
        assert!(text.contains("2 symbols"), "should have 2 symbols");
    }

    #[test]
    fn test_file_digest_empty_source() {
        let result = file_digest_signal("", touring_code::ast::languages::Lang::Rust);
        assert!(result.is_none(), "empty source should return None");
    }

    #[test]
    fn test_file_digest_no_functions() {
        let src = "let x = 42;";
        let result = file_digest_signal(src, touring_code::ast::languages::Lang::Rust);
        // No functions = no symbols = None
        assert!(result.is_none(), "no functions should return None");
    }

    #[test]
    fn test_stale_detection() {
        let fresh = PrecomputedSignals::new(vec![], Some("abc123".to_string()));
        assert!(!fresh.is_stale(Some("abc123")));
        assert!(fresh.is_stale(Some("different")));

        let no_hash = PrecomputedSignals::new(vec![], None);
        assert!(no_hash.is_stale(None));
        assert!(no_hash.is_stale(Some("anything")));
    }
}
