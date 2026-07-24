//! Plugin error types.

use thiserror::Error;

/// Errors that can occur during plugin lifecycle operations.
#[derive(Debug, Error)]
pub enum PluginError {
    /// Plugin with the given ID was not found in the registry.
    #[error("plugin not found: `{0}`")]
    PluginNotFound(String),

    /// Plugin is not available (e.g., feature not enabled, dependency missing).
    #[error("plugin not available: `{0}`")]
    PluginNotAvailable(String),

    /// Failed to swap plugin at runtime.
    #[error("plugin swap failed: `{0}`")]
    SwapFailed(String),

    /// Backend is not configured (e.g., env var not set).
    #[error("backend not configured: `{0}`")]
    BackendNotConfigured(String),
}
