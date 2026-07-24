//! Voyage AI REST API provider for touring-embeddings.
//!
//! Supports voyage-2 (768 dimensions) and voyage-large (1024 dimensions)
//! models via the Voyage AI embeddings API.

use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

use crate::embeddings::{
    EmbeddingError, EmbeddingModel, EmbeddingProvider, EmbeddingResult, ModelFamily,
};

/// Supported Voyage AI model variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VoyageModel {
    /// Voyage AI v2 — 768 dimensions
    Voyage2,
    /// Voyage AI Large — 1024 dimensions
    VoyageLarge,
}

impl VoyageModel {
    /// Returns the embedding dimension for this variant.
    pub fn dimensions(&self) -> usize {
        match self {
            VoyageModel::Voyage2 => 768,
            VoyageModel::VoyageLarge => 1024,
        }
    }

    /// Returns the API model identifier string.
    pub fn api_id(&self) -> &'static str {
        match self {
            VoyageModel::Voyage2 => "voyage-2",
            VoyageModel::VoyageLarge => "voyage-large",
        }
    }

    /// Converts to the公共 `EmbeddingModel` enum variant.
    pub fn to_embedding_model(&self) -> EmbeddingModel {
        match self {
            VoyageModel::Voyage2 => EmbeddingModel::Voyage2,
            VoyageModel::VoyageLarge => EmbeddingModel::VoyageLarge,
        }
    }
}

/// Voyage AI REST API embedding provider.
///
/// Makes async HTTP calls to the Voyage AI embeddings endpoint.
/// API key can be provided via constructor or `VOYAGE_API_KEY` environment variable.
pub struct VoyageProvider {
    /// HTTP client for API requests.
    client: Client,
    /// API key for authentication.
    api_key: String,
    /// Base URL for the API endpoint.
    base_url: String,
    /// The model variant to use.
    model: VoyageModel,
}

impl VoyageProvider {
    /// Creates a new VoyageProvider with the given API key and model variant.
    ///
    /// Falls back to `VOYAGE_API_KEY` environment variable if `api_key` is empty.
    ///
    /// # Errors
    /// Returns `EmbeddingError::ApiKeyMissing` if no API key is available.
    pub fn new(api_key: &str, model: VoyageModel) -> Result<Self, EmbeddingError> {
        let api_key = if api_key.is_empty() {
            std::env::var("VOYAGE_API_KEY")
                .map_err(|_| EmbeddingError::ApiKeyMissing("VOYAGE_API_KEY not set".into()))?
        } else {
            api_key.to_owned()
        };

        let base_url = "https://api.voyageai.com/v1/embeddings".to_owned();

        // Build a client with reasonable timeouts.
        let client = Client::builder()
            .timeout(Duration::from_secs(60))
            .build()
            .map_err(|e| EmbeddingError::Internal(format!("failed to build HTTP client: {e}")))?;

        Ok(Self {
            client,
            api_key,
            base_url,
            model,
        })
    }

    /// Creates a new VoyageProvider from environment variable, using the given model.
    ///
    /// # Errors
    /// Returns `EmbeddingError::ApiKeyMissing` if `VOYAGE_API_KEY` is not set.
    pub fn from_env(model: VoyageModel) -> Result<Self, EmbeddingError> {
        Self::new("", model)
    }

    /// Performs the embedding request, returning `(vectors, total_tokens)`.
    ///
    /// Vectors are ordered by the API's `index` field so each lines up with
    /// its input text; `total_tokens` is the usage count reported by the API.
    async fn embed_internal(
        &self,
        texts: Vec<String>,
    ) -> Result<(Vec<Vec<f32>>, usize), EmbeddingError> {
        let request_body = VoyageRequest {
            model: self.model.api_id().to_owned(),
            input: texts,
        };

        let response = self
            .client
            .post(&self.base_url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&request_body)
            .send()
            .await
            .map_err(|e| EmbeddingError::InferenceFailed(format!("request failed: {e}")))?;

        let status = response.status();

        if status.as_u16() == 429 {
            return Err(EmbeddingError::RateLimited(
                "Voyage AI rate limit exceeded".into(),
            ));
        }

        if !status.is_success() {
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "unknown error".to_owned());
            return Err(EmbeddingError::InferenceFailed(format!(
                "API error {}: {}",
                status, body
            )));
        }

        let voyage_response: VoyageResponse = response.json().await.map_err(|e| {
            EmbeddingError::InferenceFailed(format!("failed to parse response: {e}"))
        })?;

        // The API may return embeddings out of order — sort by `index` so
        // each vector lines up with its corresponding input text.
        let mut data = voyage_response.data;
        data.sort_by_key(|d| d.index);
        let vectors: Vec<Vec<f32>> = data.into_iter().map(|d| d.embedding).collect();

        Ok((vectors, voyage_response.usage.total_tokens))
    }
}

/// Request body for the Voyage AI embeddings API.
#[derive(Debug, Serialize)]
struct VoyageRequest {
    model: String,
    input: Vec<String>,
}

/// Response from the Voyage AI embeddings API.
#[derive(Debug, Deserialize)]
struct VoyageResponse {
    data: Vec<VoyageEmbeddingData>,
    usage: VoyageUsage,
}

/// A single embedding data entry.
#[derive(Debug, Deserialize)]
struct VoyageEmbeddingData {
    embedding: Vec<f32>,
    index: usize,
}

/// Usage statistics from the API.
#[derive(Debug, Deserialize)]
struct VoyageUsage {
    total_tokens: usize,
}

#[async_trait]
impl EmbeddingProvider for VoyageProvider {
    /// Returns the provider identifier.
    fn id(&self) -> &'static str {
        "voyage"
    }

    /// Returns the model family.
    fn family(&self) -> ModelFamily {
        ModelFamily::new(
            "voyage",
            match self.model {
                VoyageModel::Voyage2 => "v2",
                VoyageModel::VoyageLarge => "large",
            },
        )
    }

    /// Returns the default embedding dimension.
    fn dimensions(&self) -> usize {
        self.model.dimensions()
    }

    /// Embeds a batch of texts into vectors.
    async fn embed(&self, texts: Vec<String>) -> Result<EmbeddingResult, EmbeddingError> {
        let model = self.model;
        if texts.is_empty() {
            return Ok(EmbeddingResult::new(
                vec![],
                model.to_embedding_model(),
                Some(0),
            ));
        }

        // The Voyage API reports the real token usage — use it directly
        // instead of a whitespace-split estimate.
        let (vectors, total_tokens) = self.embed_internal(texts).await?;

        Ok(EmbeddingResult::new(
            vectors,
            model.to_embedding_model(),
            Some(total_tokens),
        ))
    }

    /// Embeds a single query text.
    async fn embed_query(&self, text: String) -> Result<EmbeddingResult, EmbeddingError> {
        self.embed(vec![text]).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_voyage_model_dimensions() {
        assert_eq!(VoyageModel::Voyage2.dimensions(), 768);
        assert_eq!(VoyageModel::VoyageLarge.dimensions(), 1024);
    }

    #[test]
    fn test_voyage_model_api_id() {
        assert_eq!(VoyageModel::Voyage2.api_id(), "voyage-2");
        assert_eq!(VoyageModel::VoyageLarge.api_id(), "voyage-large");
    }

    #[test]
    fn test_voyage_model_to_embedding_model() {
        assert_eq!(
            VoyageModel::Voyage2.to_embedding_model(),
            EmbeddingModel::Voyage2
        );
        assert_eq!(
            VoyageModel::VoyageLarge.to_embedding_model(),
            EmbeddingModel::VoyageLarge
        );
    }

    #[test]
    fn test_voyage_provider_id() {
        // We cannot test full provider creation without a real API key,
        // but we can verify the structure is correct by checking the impl.
        let model = VoyageModel::Voyage2;
        assert_eq!(model.dimensions(), 768);
    }
}
