//! Semantic similarity for the portfolio — and the ACO template library's first
//! real backend.
//!
//! # Two orphans, one implementation
//!
//! `touring_intelligence::rl::aco::template_library` shipped a designed-but-dead
//! prior-art library: `EmbeddingStore` had **zero implementors** and
//! `find_similar_semantic` / `record_template` **zero external callers**
//! (verified 2026-08-08). Meanwhile the portfolio needed exactly that seam.
//!
//! [`FastEmbedSimilarity`] implements both traits from one body, so wiring the
//! portfolio's semantic leg is the same act as bringing the ACO library back to
//! life (REGRA #0 — potentiate, never leave dead).
//!
//! # Why this is opt-in
//!
//! `fastembed` downloads its model from HuggingFace on first use. A hook that
//! silently reached for the network would be a bad neighbour, so the scorer is
//! built only when `TOURING_PORTFOLIO_SEMANTIC=1`. Unset, the portfolio stays
//! purely lexical — which the 2026-08-08 measurements showed already answers
//! the common intents.

use touring_foundation::portfolio::SemanticScorer;
use touring_intelligence::rl::aco::template_library::EmbeddingStore;
use touring_storage::embeddings::{FastEmbedModel, FastEmbedProvider};

/// Environment switch that arms the semantic leg (a human decision — it may hit
/// the network on first use).
pub const SEMANTIC_ENV: &str = "TOURING_PORTFOLIO_SEMANTIC";

/// Cosine similarity mapped from `[-1,1]` to `[0,1]`.
///
/// Returns `0.0` for mismatched or zero-norm vectors rather than `NaN`, so a
/// degenerate embedding can never poison a ranking.
#[must_use]
pub fn cosine_unit(a: &[f32], b: &[f32]) -> f64 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let (mut dot, mut na, mut nb) = (0.0_f64, 0.0_f64, 0.0_f64);
    for (x, y) in a.iter().zip(b.iter()) {
        dot += f64::from(*x) * f64::from(*y);
        na += f64::from(*x) * f64::from(*x);
        nb += f64::from(*y) * f64::from(*y);
    }
    if na <= 0.0 || nb <= 0.0 {
        return 0.0;
    }
    let cos = dot / (na.sqrt() * nb.sqrt());
    ((cos + 1.0) / 2.0).clamp(0.0, 1.0)
}

/// Embedding-backed similarity, shared by the portfolio and the ACO library.
pub struct FastEmbedSimilarity {
    provider: FastEmbedProvider,
}

impl FastEmbedSimilarity {
    /// Build the scorer, or `None` when the semantic leg is not armed.
    ///
    /// Checking the env var here — rather than at the call site — keeps every
    /// caller honest by construction: there is no way to get a scorer without
    /// the human having asked for one.
    #[must_use]
    pub fn if_armed() -> Option<Self> {
        if std::env::var(SEMANTIC_ENV).ok().as_deref() != Some("1") {
            return None;
        }
        Some(Self {
            provider: FastEmbedProvider::with_model(FastEmbedModel::BgeSmall),
        })
    }

    /// Similarity of two texts; `0.0` when either cannot be embedded.
    ///
    /// Failing to `0.0` (rather than propagating) is deliberate: a scorer is an
    /// enhancement over the lexical ranking, and an embedding outage must
    /// degrade the ordering, never break the answer.
    #[must_use]
    pub fn similarity_of(&self, a: &str, b: &str) -> f64 {
        let (Ok(va), Ok(vb)) = (
            self.provider.embed_one_sync(a),
            self.provider.embed_one_sync(b),
        ) else {
            return 0.0;
        };
        cosine_unit(&va, &vb)
    }
}

impl SemanticScorer for FastEmbedSimilarity {
    fn score(&self, a: &str, b: &str) -> f64 {
        self.similarity_of(a, b)
    }
}

impl EmbeddingStore for FastEmbedSimilarity {
    fn similarity(&self, a: &str, b: &str) -> f64 {
        self.similarity_of(a, b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cosine_of_identical_vectors_is_one() {
        let v = vec![0.3_f32, -0.7, 0.1];
        assert!((cosine_unit(&v, &v) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn cosine_of_opposite_vectors_is_zero() {
        let a = vec![1.0_f32, 0.0];
        let b = vec![-1.0_f32, 0.0];
        assert!(cosine_unit(&a, &b).abs() < 1e-9);
    }

    #[test]
    fn cosine_of_orthogonal_vectors_is_the_midpoint() {
        let a = vec![1.0_f32, 0.0];
        let b = vec![0.0_f32, 1.0];
        assert!((cosine_unit(&a, &b) - 0.5).abs() < 1e-9);
    }

    #[test]
    fn degenerate_inputs_score_zero_never_nan() {
        assert_eq!(cosine_unit(&[], &[]), 0.0);
        assert_eq!(cosine_unit(&[1.0], &[1.0, 2.0]), 0.0, "length mismatch");
        assert_eq!(cosine_unit(&[0.0, 0.0], &[1.0, 1.0]), 0.0, "zero norm");
        assert!(!cosine_unit(&[0.0], &[0.0]).is_nan());
    }

    #[test]
    fn scorer_is_absent_unless_explicitly_armed() {
        // The default must never reach the network. This asserts the gate for
        // whatever the ambient environment is, in both directions.
        match std::env::var(SEMANTIC_ENV).ok().as_deref() {
            Some("1") => assert!(FastEmbedSimilarity::if_armed().is_some()),
            _ => assert!(
                FastEmbedSimilarity::if_armed().is_none(),
                "semantic leg must stay off unless {SEMANTIC_ENV}=1"
            ),
        }
    }

    #[test]
    fn one_body_serves_both_traits() {
        // The REGRA #0 claim, asserted structurally: the same type satisfies the
        // portfolio's scorer AND the previously-implementor-less ACO trait.
        fn assert_scorer<T: SemanticScorer>() {}
        fn assert_embedding_store<T: EmbeddingStore>() {}
        assert_scorer::<FastEmbedSimilarity>();
        assert_embedding_store::<FastEmbedSimilarity>();
    }
}
