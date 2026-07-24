//! Plugin provider adapter — bridges the plugin world (`Arc<Box<dyn ProviderPlugin>>`)
//! with the embedding world (`Arc<dyn EmbeddingProvider>`).
//!
//! This adapter wraps a `ProviderPlugin` from the global plugin registry and
//! exposes it as an `EmbeddingProvider` for use by `SearchPipeline`.
//!
//! # Architecture
//!
//! The plugin system lives in `touring-core::plugin` and stores
//! `Arc<Box<dyn ProviderPlugin>>`. The embedding system in
//! `touring-embeddings` exposes `Arc<dyn EmbeddingProvider>`.
//! `ArcSwapPluginAdapter` bridges these two worlds by:
//! 1. Holding a lock-free `ArcSwap<Box<dyn ProviderPlugin>>` reference
//! 2. Downcasting the plugin's backend to the concrete `P` type on each call
//! 3. Delegating to `P`'s `EmbeddingProvider` implementation
//!
//! # Registration Flow (at daemon startup)
//!
//! ```ignore
//! // In main.rs or wherever plugins are registered:
//! use touring_foundation::plugin::{global_registry, PluginFamily};
//! use touring_storage::embeddings::{FastEmbedProvider, FastEmbedModel};
//!
//! global_registry().register(Box::new(FastEmbedProvider::with_model(FastEmbedModel::BgeSmall)));
//! ```
//!
//! # Usage in SearchPipeline
//!
//! ```ignore
//! use touring_foundation::plugin::{global_registry, PluginFamily};
//! use touring_storage::embeddings::adapter::ArcSwapPluginAdapter;
//!
//! // Get plugin from registry as Arc<dyn EmbeddingProvider>
//! let adapter = global_registry()
//!     .get_as_embedding::<FastEmbedProvider>(PluginFamily::Embeddings, "default")
//!     .expect("default embeddings plugin must be registered");
//! let arc_provider = Arc::new(adapter) as Arc<dyn EmbeddingProvider>;
//! let pipeline = SearchPipeline::with_provider(config, arc_provider);
//! ```

use crate::embeddings::{EmbeddingError, EmbeddingProvider, EmbeddingResult, family::ModelFamily};
#[allow(unused)]
use arc_swap::ArcSwap;
use async_trait::async_trait;
use std::sync::Arc;
use touring_foundation::plugin::{PluginFamily, ProviderPlugin};

/// `PluginAdapter` wraps a `Arc<Box<dyn ProviderPlugin>>` and implements
/// `EmbeddingProvider` by downcasting the plugin's backend to concrete type `P`.
///
/// This is the non-locking variant — for lock-free hot-path swaps, use
/// `ArcSwapPluginAdapter` below.
///
/// Use this when the plugin reference is stable and will not change at runtime.
pub struct PluginAdapter<P> {
    plugin: Arc<Box<dyn ProviderPlugin>>,
    _phantom: std::marker::PhantomData<P>,
}

impl<P> PluginAdapter<P>
where
    P: EmbeddingProvider + ProviderPlugin + 'static,
{
    /// Creates a new adapter from a plugin arc retrieved from the registry.
    ///
    /// Returns `Err` if the plugin's backend cannot be accessed or is not
    /// the expected concrete type `P`.
    pub fn new(plugin: Arc<Box<dyn ProviderPlugin>>) -> Result<Self, EmbeddingError> {
        // Verify the backend is the right type by calling backend() and downcasting.
        let backend = (*plugin)
            .backend()
            .map_err(|e| EmbeddingError::Internal(e.to_string()))?;

        // Attempt to obtain a reference to the inner P from the backend Arc.
        // If the backend is not P, this will panic — but that's a registration bug.
        let _any = backend.downcast_ref::<P>().ok_or_else(|| {
            EmbeddingError::Internal(format!(
                "plugin backend is not the expected type `{}`",
                std::any::type_name::<P>()
            ))
        })?;

        Ok(Self {
            plugin,
            _phantom: std::marker::PhantomData,
        })
    }

    /// Returns a reference to the underlying plugin.
    pub fn plugin(&self) -> &Arc<Box<dyn ProviderPlugin>> {
        &self.plugin
    }

    /// Returns the adapter's ID.
    pub fn adapter_id(&self) -> &'static str {
        self.plugin.id()
    }

    /// Returns the adapter's model family.
    pub fn adapter_family(&self) -> ModelFamily {
        <P as EmbeddingProvider>::family(
            self.plugin
                .backend()
                .expect("plugin backend should be accessible")
                .downcast_ref::<P>()
                .expect("plugin backend downcast should succeed in adapter_family"),
        )
    }

    /// Returns the embedding dimensions for this adapter (delegates via downcast).
    pub fn adapter_dimensions(&self) -> usize {
        let backend = (*self.plugin)
            .backend()
            .expect("plugin backend should be accessible");
        backend
            .downcast_ref::<P>()
            .map(|p| p.dimensions())
            .unwrap_or(0)
    }
}

#[async_trait]
impl<P> EmbeddingProvider for PluginAdapter<P>
where
    P: EmbeddingProvider + ProviderPlugin + 'static,
{
    fn id(&self) -> &'static str {
        self.plugin.id()
    }

    fn family(&self) -> ModelFamily {
        self.adapter_family()
    }

    fn dimensions(&self) -> usize {
        self.adapter_dimensions()
    }

    async fn embed(&self, texts: Vec<String>) -> Result<EmbeddingResult, EmbeddingError> {
        let backend = (*self.plugin)
            .backend()
            .map_err(|e| EmbeddingError::Internal(e.to_string()))?;
        let p = backend.downcast_ref::<P>().ok_or_else(|| {
            EmbeddingError::Internal(format!(
                "PluginAdapter backend is not the expected type `{}`",
                std::any::type_name::<P>()
            ))
        })?;
        <P as EmbeddingProvider>::embed(p, texts).await
    }

    async fn embed_query(&self, text: String) -> Result<EmbeddingResult, EmbeddingError> {
        let backend = (*self.plugin)
            .backend()
            .map_err(|e| EmbeddingError::Internal(e.to_string()))?;
        let p = backend.downcast_ref::<P>().ok_or_else(|| {
            EmbeddingError::Internal(format!(
                "PluginAdapter backend is not the expected type `{}`",
                std::any::type_name::<P>()
            ))
        })?;
        <P as EmbeddingProvider>::embed_query(p, text).await
    }
}

/// `ArcSwapPluginAdapter` is the lock-free variant — uses `ArcSwap` for
/// hot-path provider swaps without locking on each access.
///
/// Use this when you need the adapter to track live plugin swaps in the registry.
pub struct ArcSwapPluginAdapter<P> {
    /// Lock-free reference to the current plugin arc. Swapped atomically.
    plugin_ref: arc_swap::ArcSwap<Box<dyn ProviderPlugin>>,
    _phantom: std::marker::PhantomData<P>,
}

impl<P> ArcSwapPluginAdapter<P>
where
    P: EmbeddingProvider + ProviderPlugin + 'static,
{
    /// Creates a new adapter wrapping the given plugin arc.
    pub fn new(plugin: Arc<Box<dyn ProviderPlugin>>) -> Self {
        Self {
            plugin_ref: arc_swap::ArcSwap::new(plugin),
            _phantom: std::marker::PhantomData,
        }
    }

    /// Loads the current plugin arc (O(1) Arc::clone).
    fn current_plugin(&self) -> Arc<Box<dyn ProviderPlugin>> {
        self.plugin_ref.load().clone()
    }

    /// Returns the adapter's ID (delegates to the current plugin).
    pub fn id(&self) -> &'static str {
        self.current_plugin().id()
    }

    /// Returns the adapter's model family (delegates to the current plugin).
    pub fn family(&self) -> ModelFamily {
        // The inner P is the EmbeddingProvider, which provides family()
        // We downcast the backend to get the concrete P.
        let backend = (*self.current_plugin())
            .backend()
            .expect("plugin backend should be accessible");
        backend
            .downcast_ref::<P>()
            .map(|p| <P as EmbeddingProvider>::family(p))
            .unwrap_or_else(|| ModelFamily::new("unknown", "unknown"))
    }

    /// Returns the embedding dimensions for this adapter.
    pub fn dimensions(&self) -> usize {
        let backend = (*self.current_plugin())
            .backend()
            .expect("plugin backend should be accessible");
        backend
            .downcast_ref::<P>()
            .map(|p| p.dimensions())
            .unwrap_or(0)
    }
}

#[async_trait]
impl<P> EmbeddingProvider for ArcSwapPluginAdapter<P>
where
    P: EmbeddingProvider + ProviderPlugin + 'static,
{
    fn id(&self) -> &'static str {
        ArcSwapPluginAdapter::<P>::id(self)
    }

    fn family(&self) -> ModelFamily {
        ArcSwapPluginAdapter::<P>::family(self)
    }

    fn dimensions(&self) -> usize {
        ArcSwapPluginAdapter::<P>::dimensions(self)
    }

    async fn embed(&self, texts: Vec<String>) -> Result<EmbeddingResult, EmbeddingError> {
        let plugin = self.current_plugin();
        let backend = plugin
            .backend()
            .map_err(|e| EmbeddingError::Internal(e.to_string()))?;
        let p = backend.downcast_ref::<P>().ok_or_else(|| {
            EmbeddingError::Internal(format!(
                "ArcSwapPluginAdapter backend is not the expected type `{}`",
                std::any::type_name::<P>()
            ))
        })?;
        <P as EmbeddingProvider>::embed(p, texts).await
    }

    async fn embed_query(&self, text: String) -> Result<EmbeddingResult, EmbeddingError> {
        let plugin = self.current_plugin();
        let backend = plugin
            .backend()
            .map_err(|e| EmbeddingError::Internal(e.to_string()))?;
        let p = backend.downcast_ref::<P>().ok_or_else(|| {
            EmbeddingError::Internal(format!(
                "ArcSwapPluginAdapter backend is not the expected type `{}`",
                std::any::type_name::<P>()
            ))
        })?;
        <P as EmbeddingProvider>::embed_query(p, text).await
    }
}

/// Extension trait for `PluginRegistry` to retrieve a plugin as an `Arc<dyn EmbeddingProvider>`.
///
/// This is a convenience for code that needs to go from `Arc<Box<dyn ProviderPlugin>>`
/// directly to `Arc<dyn EmbeddingProvider>` via the adapter.
pub trait RegistryAsEmbeddingProviderExt {
    /// Retrieves the plugin for the given family and ID, wrapped in a
    /// `PluginAdapter` that implements `EmbeddingProvider`.
    fn get_as_embedding<P>(
        &self,
        family: PluginFamily,
        id: &'static str,
    ) -> Option<PluginAdapter<P>>
    where
        P: EmbeddingProvider + ProviderPlugin + 'static;
}

impl RegistryAsEmbeddingProviderExt for touring_foundation::plugin::PluginRegistry {
    fn get_as_embedding<P>(
        &self,
        family: PluginFamily,
        id: &'static str,
    ) -> Option<PluginAdapter<P>>
    where
        P: EmbeddingProvider + ProviderPlugin + 'static,
    {
        let plugin = self.get(family, id)?;
        PluginAdapter::new(plugin).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_adapter_size() {
        // Verify PluginAdapter<P> has non-zero size (sanity check).
        let size = std::mem::size_of::<PluginAdapter<()>>();
        assert!(size > 0, "PluginAdapter should have non-zero size");
    }

    #[test]
    fn test_plugin_adapter_type_name() {
        // PluginAdapter<P> erases the concrete plugin type while
        // preserving the ability to downcast back to P via backend().
        // This test just verifies the type name is correctly captured.
        #[cfg(feature = "fastembed")]
        let name = std::any::type_name::<PluginAdapter<super::super::FastEmbedProvider>>();
        #[cfg(feature = "fastembed")]
        assert!(name.contains("PluginAdapter"));
    }
}
