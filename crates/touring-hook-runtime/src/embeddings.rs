//! Semantic / hash text embeddings for the ANN memory path (S-04).
//!
//! Carve R (2026-06-10): moved from `touring-dispatch/src/cli/shared.rs` —
//! embedding is a runtime capability consumed by `ceg_impls::cli_memory_store`
//! and the cli memory/recall handlers (which re-import from here at their
//! historical `cli::shared::*` paths).

/// Process-wide, lazily-initialized semantic embedder (S-04, 2026-05-29).
///
/// Held for the daemon's lifetime: the ONNX model (~440MB for arctic-embed-m)
/// loads once on first use, then every embed is CPU inference from cached
/// weights (offline). `Some` = model loaded; `None` = load failed (weights
/// missing / no network on first download) → callers fall back to the 64-dim
/// hash embedder. The choice is sticky for the daemon lifetime, so `store` and
/// `recall` always agree on vector width within a single run.
#[cfg(feature = "semantic-embeddings")]
static SEMANTIC_EMBEDDER: once_cell::sync::OnceCell<
    Option<touring_storage::embeddings::FastEmbedProvider>,
> = once_cell::sync::OnceCell::new();

/// Returns the shared semantic embedder, loading it on first call.
///
/// Model: Snowflake Arctic-Embed-M (768d, retrieval-tuned) — chosen 2026-05-29
/// for best-in-class open retrieval quality. Loaded via the lean fastembed
/// provider (no qdrant/candle/voyage). Fail-open: a load error yields `None`.
#[cfg(feature = "semantic-embeddings")]
fn semantic_embedder() -> Option<&'static touring_storage::embeddings::FastEmbedProvider> {
    SEMANTIC_EMBEDDER
        .get_or_init(|| {
            use touring_storage::embeddings::{FastEmbedModel, FastEmbedProvider};
            match FastEmbedProvider::try_with_model(FastEmbedModel::ArcticEmbedM) {
                Ok(p) => {
                    tracing::info!("S-04 semantic embedder loaded: arctic-embed-m (768d)");
                    Some(p)
                }
                Err(e) => {
                    tracing::warn!(
                        "S-04 semantic embedder unavailable ({e}); ANN recall \
                         falls back to the 64-dim hash embedder"
                    );
                    None
                }
            }
        })
        .as_ref()
}

/// Genuine semantic embedding for `text`, or `None` when the feature is off or
/// the model is unavailable (the caller then falls back to the hash embedder).
#[cfg(feature = "semantic-embeddings")]
pub fn semantic_text_embedding(text: &str) -> Option<Vec<f32>> {
    semantic_embedder()?.embed_one_sync(text).ok()
}

/// Feature-off shim: no semantic model, always fall back to the hash embedder.
#[cfg(not(feature = "semantic-embeddings"))]
pub fn semantic_text_embedding(_text: &str) -> Option<Vec<f32>> {
    None
}

/// Document-side embedding for the ANN memory path: a genuine semantic vector
/// when available (S-04, arctic-embed-m 768d), otherwise the deterministic
/// 64-dim hash (graceful fallback). `store` and `reindex` embed documents
/// through here verbatim; the recall path embeds the *query* through
/// `memory_recall_query_embedding`, which adds the arctic query prefix. Both
/// use the same model and 768-dim space (cosine-comparable) — only the query
/// carries the instruction prefix, by design. In fail-open mode both sides use
/// the raw-text 64-dim hash, so they stay consistent there too.
///
/// Migration note: switching the model changes the vector width, so the ANN
/// corpus must be regenerated once via `touring memory reindex`. The search
/// path skips any entry whose width differs from the query (see
/// `AnnMemoryRecall::search`), so a mixed corpus degrades gracefully rather
/// than panicking during the transition.
pub fn semantic_or_hash_embedding(text: &str) -> Vec<f32> {
    semantic_text_embedding(text).unwrap_or_else(|| crate::ann_memory::query_hash_embedding(text))
}
