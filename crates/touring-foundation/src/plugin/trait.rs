//! Plugin traits for the runtime-swappable provider system.

use std::sync::Arc;

use crate::plugin::error::PluginError;

/// Family of plugins.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PluginFamily {
    /// Embeddings provider (text→vectors).
    Embeddings,
    /// Vector store (storage/retrieval).
    VectorStore,
    /// Search pipeline.
    Search,
    /// Reranker (reorders search results).
    Reranker,
    /// Custom/third-party.
    Custom(&'static str),
}

impl PluginFamily {
    /// Returns the plugin family as a string identifier.
    pub fn as_str(&self) -> &'static str {
        match self {
            PluginFamily::Embeddings => "embeddings",
            PluginFamily::VectorStore => "vector-store",
            PluginFamily::Search => "search",
            PluginFamily::Reranker => "reranker",
            PluginFamily::Custom(s) => s,
        }
    }
}

/// Core plugin trait.
///
/// All plugins must implement this trait. They are identified by unique IDs
/// and belong to a [`PluginFamily`].
pub trait Plugin: Send + Sync {
    /// Returns the plugin's unique identifier.
    fn id(&self) -> &'static str;

    /// Returns the plugin family this plugin belongs to.
    fn family(&self) -> PluginFamily;

    /// Returns true if the plugin supports static initialization
    /// (i.e., can be instantiated without async I/O).
    fn supports_static(&self) -> bool {
        false
    }
}

/// Plugin that can produce a backend suitable for `ArcSwap` lock-free storage.
///
/// This trait bridges the plugin lifecycle (dynamic discovery) with the
/// runtime swap lifecycle (lock-free `Arc` swap on the hot path).
pub trait ProviderPlugin: Plugin {
    /// Consumes the provider and returns its backend as a boxed trait object.
    ///
    /// Returns `Err` if the provider cannot produce a usable backend
    /// (e.g., missing credentials, feature not enabled).
    fn into_backend(
        self: Box<Self>,
    ) -> Result<Arc<dyn std::any::Any + Send + Sync + 'static>, PluginError>;

    /// Returns a new backend from the provider without consuming it.
    fn backend(&self) -> Result<Arc<dyn std::any::Any + Send + Sync + 'static>, PluginError>;

    /// Creates a new plugin instance with a fresh backend, suitable for
    /// hot-reload without disrupting live references to the old plugin.
    ///
    /// The default implementation calls `self.backend()` and wraps the result
    /// in a new box of the same concrete type, assuming the concrete type
    /// implements `FromBackend`.
    fn with_fresh_backend(
        self: Box<Self>,
        fresh: Arc<dyn std::any::Any + Send + Sync + 'static>,
    ) -> Result<Box<dyn ProviderPlugin>, PluginError> {
        let _ = fresh;
        Err(PluginError::SwapFailed(
            "with_fresh_backend not implemented for this plugin type".into(),
        ))
    }
}
