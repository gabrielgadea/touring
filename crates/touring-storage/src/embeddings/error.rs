//! Error types for the embedding provider abstraction.

use thiserror::Error;

/// Errors that can occur during embedding operations.
#[derive(Debug, Error)]
pub enum EmbeddingError {
    /// The requested model is not supported by this provider.
    #[error("unsupported model: {0}")]
    UnsupportedModel(String),

    /// Failed to load the model (file not found, invalid format, etc.).
    #[error("model load failed: {0}")]
    ModelLoadFailed(String),

    /// Inference failed during embedding generation.
    #[error("inference failed: {0}")]
    InferenceFailed(String),

    /// Required API key is missing from environment.
    #[error("API key missing: {0}")]
    ApiKeyMissing(String),

    /// Rate limit exceeded from remote API.
    #[error("rate limited: {0}")]
    RateLimited(String),

    /// Input text is invalid (empty, too long, etc.).
    #[error("invalid input: {0}")]
    InvalidInput(String),

    /// Internal error (serialization, I/O, etc.).
    #[error("internal error: {0}")]
    Internal(String),
}
