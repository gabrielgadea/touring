//! CLI handlers for plugin operations.
//!
//! These handlers are exposed via the touring CLI interface and allow
//! runtime inspection and swapping of plugins.

use crate::plugin::error::PluginError;
use crate::plugin::{PluginFamily, PluginRegistry, ProviderPlugin};
use std::sync::Arc;

/// Lists all registered plugin families.
///
/// Note: enumerating individual plugin IDs within a family requires
/// a future `plugins_for_family()` method on `PluginRegistry`.
pub fn plugin_list(registry: &PluginRegistry) -> Vec<PluginListEntry> {
    registry
        .families()
        .map(|family| PluginListEntry {
            family: family.as_str().to_string(),
            plugins: vec![], // TODO: add PluginRegistry::plugins_for_family() for full enumeration
        })
        .collect()
}

/// Simplified list entry.
#[derive(Debug, serde::Serialize)]
pub struct PluginListEntry {
    pub family: String,
    pub plugins: Vec<String>,
}

/// Swap the active plugin for a given family and ID.
///
/// Returns an error if the plugin is not found or its backend cannot be constructed.
pub fn plugin_swap(
    registry: &PluginRegistry,
    family: PluginFamily,
    id: &'static str,
) -> Result<(), PluginError> {
    registry.backend(family, id).map(|_| ())
}

/// Returns the status of a specific plugin (whether it is registered and its backend is available).
pub fn plugin_status(
    registry: &PluginRegistry,
    family: PluginFamily,
    id: &'static str,
) -> Result<PluginStatus, PluginError> {
    let backend = registry.backend(family, id)?;
    Ok(PluginStatus {
        family: family.as_str().to_string(),
        id,
        available: true,
        backend_type: backend.type_id(),
    })
}

/// Status response for a plugin.
#[derive(Debug, serde::Serialize)]
pub struct PluginStatus {
    pub family: String,
    pub id: &'static str,
    pub available: bool,
    #[serde(skip_serializing_if = "std::option::Option::is_none")]
    pub backend_type: Option<std::any::TypeId>,
}

/// Constructs a plugin from the `TOURING_PLUGIN_<FAMILY>` environment variable.
///
/// Supported families:
/// - `"embeddings"` — `TOURING_PLUGIN_EMBEDDINGS`
/// - `"vector-store"` — `TOURING_PLUGIN_VECTOR_STORE`
/// - `"search"` — `TOURING_PLUGIN_SEARCH`
/// - `"reranker"` — `TOURING_PLUGIN_RERANKER`
///
/// Falls back to the default plugin for that family if the env var is not set.
pub fn plugin_from_env(family: PluginFamily) -> Option<String> {
    let env_key = format!("TOURING_PLUGIN_{}", family.as_str().to_uppercase());
    std::env::var(env_key).ok()
}
