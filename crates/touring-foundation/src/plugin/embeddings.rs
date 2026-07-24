//! Plugin adapter for bridging embedding providers into the plugin system.
//!
//! This module provides `EmbeddingProviderPlugin` — a `ProviderPlugin` implementation
//! that wraps an `EmbeddingProvider` from `touring-embeddings`. This allows embedding
//! providers to be registered in `global_registry()` and retrieved for use with
//! `SearchPipeline::with_registry()`.
//!
//! # Usage
//!
//! ```ignore
//! use touring_foundation::plugin::{global_registry, PluginFamily};
//! use touring_storage::embeddings::FastEmbedProvider;
//! use touring_foundation::plugin::embeddings::EmbeddingProviderPlugin;
//!
//! // Wrap and register
//! let plugin = EmbeddingProviderPlugin::new(
//!     FastEmbedProvider::with_model(FastEmbedModel::BgeSmall),
//!     "fastembed",
//!     PluginFamily::Embeddings,
//! );
//! global_registry().register(Box::new(plugin));
//! ```

use std::sync::Arc;
use crate::plugin::error::PluginError;
use crate::plugin::r#trait::{Plugin, PluginFamily, ProviderPlugin};

/// Plugin wrapper that bridges `EmbeddingProvider` (from touring-embeddings) to `ProviderPlugin`
/// (from touring-foundation).
///
/// This allows embedding providers to be registered in the global plugin registry
/// and retrieved via `PluginRegistry::get()` for use with `SearchPipeline::with_registry()`.
///
/// The wrapper stores the embedding provider as a boxed `Arc<dyn Any>` backend so it can
/// be stored in a lock-free `ArcSwap` table.
///
/// # Type Parameters
/// - `P`: The concrete embedding provider type (e.g., `FastEmbedProvider`).
pub struct EmbeddingProviderPlugin<P> {
    inner: P,
    id: &'static str,
    family: PluginFamily,
}

impl<P> EmbeddingProviderPlugin<P>
where
    P: Send + Sync + 'static,
{
    /// Creates a new plugin wrapper around an embedding provider.
    pub fn new(inner: P, id: &'static str, family: PluginFamily) -> Self {
        Self { inner, id, family }
    }

    /// Returns the inner embedding provider by reference.
    pub fn inner(&self) -> &P {
        &self.inner
    }
}

impl<P> Plugin for EmbeddingProviderPlugin<P>
where
    P: Send + Sync + 'static,
{
    fn id(&self) -> &'static str {
        self.id
    }

    fn family(&self) -> PluginFamily {
        self.family
    }

    fn supports_static(&self) -> bool {
        true
    }
}

impl<P> ProviderPlugin for EmbeddingProviderPlugin<P>
where
    P: Send + Sync + 'static,
{
    fn into_backend(self: Box<Self>) -> Result<Arc<(dyn std::any::Any + Send + Sync + 'static)>, PluginError> {
        // Box the inner P and erase its type to Arc<dyn Any>
        let boxed: Box<P> = self.inner.into();
        Ok(boxed as Box<dyn std::any::Any + Send + Sync>)
    }

    fn backend(&self) -> Result<Arc<(dyn std::any::Any + Send + Sync + 'static)>, PluginError> {
        // Return the inner P as a boxed trait object
        let boxed: Box<P> = Box::new(self.inner.by_ref());
        Ok(boxed as Box<dyn std::any::Any + Send + Sync>)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_embedding_provider_plugin_size() {
        // Verify EmbeddingProviderPlugin has non-zero size
        let size = std::mem::size_of::<EmbeddingProviderPlugin<()>>();
        assert!(size > 0, "EmbeddingProviderPlugin should have non-zero size");
    }

    #[test]
    fn test_embedding_provider_plugin_id() {
        let plugin = EmbeddingProviderPlugin::new((), "test-plugin", PluginFamily::Embeddings);
        assert_eq!(plugin.id(), "test-plugin");
        assert_eq!(plugin.family(), PluginFamily::Embeddings);
    }
}