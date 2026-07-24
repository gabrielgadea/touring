//! Shared types across all language bindings.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Language-agnostic binding error.
#[derive(Debug, Error)]
pub enum BindingError {
    /// Underlying Touring error.
    #[error("touring error: {0}")]
    Touring(String),
    /// Serialization/deserialization failure.
    #[error("serde error: {0}")]
    Serde(#[from] serde_json::Error),
    /// Invalid input from the caller language.
    #[error("invalid input: {0}")]
    InvalidInput(String),
}

/// Result alias for binding operations.
pub type BindingResult<T> = Result<T, BindingError>;

/// Public façade for the simplest workflow ("hello touring").
#[derive(Debug, Serialize, Deserialize)]
pub struct Greeting {
    /// Greeting payload.
    pub message: String,
    /// Touring version.
    pub touring_version: String,
}

impl Greeting {
    /// Construct a new greeting.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            touring_version: crate::VERSION.to_owned(),
        }
    }
}
