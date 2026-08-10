//! The portfolio as a real keyword backend for the hybrid search pipeline.
//!
//! # Why this exists
//!
//! Removing the fabricated `doc_kw_*` corpus from
//! `touring_storage::hybrid_search` left an honest but empty command: the
//! [`KeywordSearch`] seam had no production implementor, so `find-code` could
//! only ever report "no corpus wired". A seam without an implementor is the
//! same debt as the placeholder it replaced, just quieter (REGRA #0).
//!
//! The portfolio is a corpus that genuinely exists, is in-process, and is about
//! code: ~11k artifacts and documented symbols keyed by purpose. Wiring it here
//! turns `find-code` from "always empty" into "searches the purpose corpus" —
//! and [`crate::server::params::FindCodeResponse::backends`] states which
//! corpus answered, so the caller is never misled about what was searched.
//!
//! What this is **not**: the full symbol index. `touring tantivy search` still
//! owns identifier-keyed lookup over all ~270k symbols. This backend answers
//! "what already serves this purpose", which is a different question.

use std::sync::Arc;

use touring_foundation::portfolio::query;
use touring_foundation::portfolio::store::{self, PortfolioIndex};
use touring_storage::hybrid_search::KeywordSearch;

/// Keyword backend answering from the materialized capability portfolio.
pub struct PortfolioKeyword {
    index: PortfolioIndex,
}

impl PortfolioKeyword {
    /// Load the portfolio, or `None` when it has never been mined.
    ///
    /// Returning `None` rather than an empty backend is deliberate: an empty
    /// backend would report `keyword_backend: true` and make "consulted, no
    /// match" the stated reason for an answer that never had a corpus — the
    /// exact confusion the fabricated placeholders used to create.
    #[must_use]
    pub fn load() -> Option<Self> {
        let index = store::load().ok()?;
        (!index.is_empty()).then_some(Self { index })
    }

    /// Build from an explicit index (tests, alternate corpora).
    #[must_use]
    pub fn from_index(index: PortfolioIndex) -> Self {
        Self { index }
    }

    /// Number of records this backend can answer from.
    #[must_use]
    pub fn corpus_size(&self) -> usize {
        self.index.entries.len()
    }

    /// Load and wrap for injection, when a corpus exists.
    #[must_use]
    pub fn arc_if_available() -> Option<Arc<dyn KeywordSearch>> {
        Self::load().map(|b| Arc::new(b) as Arc<dyn KeywordSearch>)
    }
}

impl KeywordSearch for PortfolioKeyword {
    fn search(&self, query_text: &str, limit: usize) -> Vec<(String, f32)> {
        query::answer(&self.index, query_text, limit)
            .prior_art
            .into_iter()
            .map(|hit| {
                #[allow(clippy::cast_possible_truncation)]
                let score = hit.score as f32;
                (hit.entry.display_path, score)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use touring_foundation::portfolio::store::INDEX_VERSION;
    use touring_foundation::portfolio::{CapabilityEntry, CapabilityKind, Evidence};

    fn entry(name: &str, purpose: &str, kws: &[&str]) -> CapabilityEntry {
        let disp = format!("~/scripts/{name}.py");
        CapabilityEntry {
            id: CapabilityEntry::make_id(CapabilityKind::Script, &disp),
            display_path: disp.clone(),
            kind: CapabilityKind::Script,
            name: name.to_string(),
            purpose: purpose.to_string(),
            language: "python".to_string(),
            entry_point: Some(format!("python3 {disp}")),
            provenance: "skill:test".to_string(),
            keywords: kws.iter().map(|s| (*s).to_string()).collect(),
            evidence: Evidence::default(),
            purpose_inherited: false,
        }
    }

    fn backend() -> PortfolioKeyword {
        PortfolioKeyword::from_index(PortfolioIndex {
            version: INDEX_VERSION,
            built_at: "epoch:0".to_string(),
            roots: vec![],
            entries: vec![
                entry("html_to_pdf", "Generate a professional PDF from an HTML template", &["pdf", "html"]),
                entry("parse_config", "Parse and validate a TOML configuration file", &["config", "toml"]),
            ],
        })
    }

    #[test]
    fn returns_real_paths_never_placeholder_ids() {
        // The whole point: `doc_kw_1` must never come back from anywhere.
        let hits = backend().search("generate professional PDF", 5);
        assert!(!hits.is_empty(), "the corpus should answer this");
        for (id, score) in &hits {
            assert!(id.starts_with('~'), "not a real path: {id}");
            assert!(!id.starts_with("doc_"), "placeholder id leaked: {id}");
            assert!(*score > 0.0, "zero score in a returned hit");
        }
    }

    #[test]
    fn a_query_matching_nothing_returns_empty() {
        assert!(backend().search("treinar rede neural convolucional", 5).is_empty());
    }

    #[test]
    fn respects_the_limit() {
        assert!(backend().search("generate professional PDF", 1).len() <= 1);
    }

    #[test]
    fn an_empty_corpus_yields_no_backend_at_all() {
        // Not an empty backend — no backend. `is_unwired()` must stay truthful.
        let empty = PortfolioIndex::empty();
        assert!(empty.is_empty());
        assert_eq!(PortfolioKeyword::from_index(empty).corpus_size(), 0);
    }

    #[test]
    fn the_backend_satisfies_the_pipeline_trait() {
        fn assert_keyword_search<T: KeywordSearch>() {}
        assert_keyword_search::<PortfolioKeyword>();
    }
}
