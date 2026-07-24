//! Touring Embeddings Provider Abstraction
//!
//! This crate provides a unified async interface for embedding models
//! from multiple providers: Candle BGE, FastEmbed, and Voyage AI.

pub mod adapter;
mod error;
mod family;
mod providers;

pub use adapter::{ArcSwapPluginAdapter, PluginAdapter, RegistryAsEmbeddingProviderExt};

pub use error::EmbeddingError;
pub use family::ModelFamily;

// Re-export for public API consumers
pub use async_trait::async_trait;
pub use serde::{Deserialize, Serialize};

// Re-export providers for consumers who need concrete implementations
#[cfg(feature = "fastembed")]
pub use providers::fastembed::{FastEmbedModel, FastEmbedProvider};

#[cfg(feature = "candle-bge")]
pub use providers::candle_bge::{BgeModelVariant, CandleBgeProvider};

#[cfg(feature = "voyage")]
pub use providers::voyage::{VoyageModel, VoyageProvider};

/// Supported embedding model variants.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EmbeddingModel {
    /// BGE Large (candle-bge provider)
    BgeLarge,
    /// BGE Small (candle-bge provider)
    BgeSmall,
    /// FastEmbed BGE Large
    FastEmbedBgeLarge,
    /// FastEmbed BGE Small (BAAI/bge-small-en-v1.5, 384d)
    FastEmbedBgeSmall,
    /// FastEmbed Snowflake Arctic-Embed-M (768d, retrieval-tuned, best-in-class open)
    FastEmbedArcticM,
    /// Voyage AI v2
    Voyage2,
    /// Voyage AI Large
    VoyageLarge,
}

impl EmbeddingModel {
    /// Returns the dimensions produced by this model.
    pub fn dimensions(&self) -> usize {
        match self {
            EmbeddingModel::BgeLarge
            | EmbeddingModel::FastEmbedBgeLarge
            | EmbeddingModel::VoyageLarge => 1024,
            EmbeddingModel::BgeSmall
            | EmbeddingModel::FastEmbedArcticM
            | EmbeddingModel::Voyage2 => 768,
            // BAAI/bge-small-en-v1.5 is genuinely 384-dim (the prior 768 here was a bug).
            EmbeddingModel::FastEmbedBgeSmall => 384,
        }
    }

    /// Returns the family for compatibility checking.
    pub fn family(&self) -> ModelFamily {
        match self {
            EmbeddingModel::BgeLarge | EmbeddingModel::BgeSmall => ModelFamily::new("bge", "large"),
            EmbeddingModel::FastEmbedBgeLarge | EmbeddingModel::FastEmbedBgeSmall => {
                ModelFamily::new("fastembed", "large")
            }
            EmbeddingModel::FastEmbedArcticM => ModelFamily::new("fastembed", "arctic-m"),
            EmbeddingModel::Voyage2 => ModelFamily::new("voyage", "v2"),
            EmbeddingModel::VoyageLarge => ModelFamily::new("voyage", "large"),
        }
    }

    /// Returns a human-readable identifier for this model.
    pub fn id(&self) -> &'static str {
        match self {
            EmbeddingModel::BgeLarge => "bge-large",
            EmbeddingModel::BgeSmall => "bge-small",
            EmbeddingModel::FastEmbedBgeLarge => "fastembed-bge-large",
            EmbeddingModel::FastEmbedBgeSmall => "fastembed-bge-small",
            EmbeddingModel::FastEmbedArcticM => "fastembed-arctic-m",
            EmbeddingModel::Voyage2 => "voyage-2",
            EmbeddingModel::VoyageLarge => "voyage-large",
        }
    }
}

/// Result of an embedding operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingResult {
    /// The embedding vectors, each a `Vec<f32>` of dimension `model.dimensions()`.
    pub vectors: Vec<Vec<f32>>,
    /// The model used to generate the embeddings.
    pub model: EmbeddingModel,
    /// The dimension of each vector.
    pub dimension: usize,
    /// Token count used (if available from the provider).
    pub token_count: Option<usize>,
}

impl EmbeddingResult {
    /// Creates a new embedding result.
    ///
    /// The reported `dimension` reflects the vectors actually produced — a
    /// provider may run a model whose real width differs from the enum's
    /// declared default (e.g. a 384-dim BERT checkpoint loaded through the
    /// candle provider). The model's declared dimension is used only as a
    /// fallback when there are no vectors to measure.
    pub fn new(vectors: Vec<Vec<f32>>, model: EmbeddingModel, token_count: Option<usize>) -> Self {
        let dimension = vectors
            .first()
            .map(Vec::len)
            .unwrap_or_else(|| model.dimensions());
        Self {
            vectors,
            model,
            dimension,
            token_count,
        }
    }

    /// Returns the number of embeddings in this result.
    pub fn len(&self) -> usize {
        self.vectors.len()
    }

    /// Returns true if there are no embeddings.
    pub fn is_empty(&self) -> bool {
        self.vectors.is_empty()
    }
}

/// Async trait for embedding providers.
///
/// Implementors must be Send + Sync and handle their own model loading/caching.
#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    /// Returns the provider's unique identifier.
    fn id(&self) -> &'static str;

    /// Returns the model family this provider handles.
    fn family(&self) -> ModelFamily;

    /// Returns the default dimensions for this provider.
    fn dimensions(&self) -> usize;

    /// Embeds a batch of texts into vectors.
    async fn embed(&self, texts: Vec<String>) -> Result<EmbeddingResult, EmbeddingError>;

    /// Embeds a single query text (typically shorter than document texts).
    async fn embed_query(&self, text: String) -> Result<EmbeddingResult, EmbeddingError>;
}

/// Discovers the provider to use based on the TOURING_EMBEDDING_PROVIDER env var.
///
/// Falls back to "fastembed" if not set or invalid.
pub fn provider_from_env() -> &'static str {
    match std::env::var("TOURING_EMBEDDING_PROVIDER").as_deref() {
        Ok("candle-bge") => "candle-bge",
        Ok("voyage") => "voyage",
        Ok(_) | Err(_) => "fastembed",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_embedding_model_dimensions() {
        assert_eq!(EmbeddingModel::BgeLarge.dimensions(), 1024);
        assert_eq!(EmbeddingModel::Voyage2.dimensions(), 768);
    }

    #[test]
    fn test_embedding_result_len() {
        let result = EmbeddingResult::new(
            vec![vec![0.1; 1024], vec![0.2; 1024]],
            EmbeddingModel::BgeLarge,
            Some(10),
        );
        assert_eq!(result.len(), 2);
        assert!(!result.is_empty());
    }

    #[test]
    fn test_model_family_compatibility() {
        let bge_family = EmbeddingModel::BgeLarge.family();
        assert!(bge_family.is_compatible_with(&ModelFamily::new("bge", "large")));
    }

    #[test]
    fn test_provider_from_env_default() {
        // When env var is not set, defaults to "fastembed"
        let provider = provider_from_env();
        assert_eq!(provider, "fastembed");
    }

    #[test]
    fn test_embedding_result_empty() {
        let empty = EmbeddingResult::new(vec![], EmbeddingModel::BgeSmall, None);
        assert!(empty.is_empty());
        assert_eq!(empty.len(), 0);
    }
}
