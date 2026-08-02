//! Embedding engine — pluggable embedder trait and implementations.
//!
//! Provides a `Embedder` trait for extensible embedding backends with
//! three built-in implementations:
//!
//! - `GpuEmbedder` — HTTP client for GPU service at `localhost:8200`.
//!   Gracefully degrades to FTS5-only mode when the service is offline.
//! - `Fts5Embedder` — No-op embedder for FTS5-only mode (always returns `None`).
//! - `NullEmbedder` — Always-`None` embedder for testing or when embeddings
//!   are fully disabled.
//!
//! # Plugin Architecture
//!
//! The trait enables dependency injection and mocking:
//!
//! ```ignore
//! async fn process(embedder: &dyn Embedder) {
//!     if let Some(vec) = embedder.embed_single("query").await {
//!         // use embedding
//!     }
//! }
//! ```
//!
//! # Feature Flags
//!
//! - `gpu-embeddings` — enables `GpuEmbedder`. Without it, only
//!   `Fts5Embedder` and `NullEmbedder` are available.

pub mod client;

pub use client::ConnectionState;
pub use client::EmbeddingError;
pub use client::{Connected, Degraded, Disconnected};

pub use client::Embedder;
#[cfg(feature = "gpu-embeddings")]
pub use client::GpuEmbedder;
pub use client::{Fts5Embedder, NullEmbedder};

// Re-export the trait from client for ergonomic access
#[cfg(feature = "gpu-embeddings")]
pub use client::Embedder as Embedded;

// ─────────────────────────────────────────────────────────────────────────────
// W3.2 boundary marker (Wave 2026-05-12)
//
// The factory + config below demarcate the abstract boundary between the
// `Embedder` trait (which belongs to Layer 1 / `touring-foundation`) and the
// concrete `GpuEmbedder` + `Fts5Embedder` + `NullEmbedder` implementations
// (which will move to `touring-storage` in W5).
//
// Consumers should prefer `make_embedder(&config)` over directly naming
// concrete types. After W5 ships the embed impls in `touring-storage`, the
// concrete `pub use client::{GpuEmbedder, ...}` re-exports above become
// shim-only and can be removed.
//
// See: `scripts/touring_premium_refactor_2026/staging/w3.2-extraction-roadmap.md`
// ─────────────────────────────────────────────────────────────────────────────

/// Lightweight configuration for selecting an embedder backend at runtime.
///
/// Used by `make_embedder` to instantiate the right concrete type without
/// callers having to name `GpuEmbedder<Disconnected>` (typestate) or
/// remember which feature flag gates which backend.
#[derive(Debug, Clone)]
pub struct EmbedderConfig {
    /// `true` returns `NullEmbedder` (always-`None`, useful for tests + cold
    /// paths where embeddings would only add latency).
    pub disabled: bool,
    /// HTTP endpoint of the GPU embedding service. When `Some` and the
    /// `gpu-embeddings` feature is enabled, `make_embedder` returns
    /// `GpuEmbedder` in the `Disconnected` state (will lazily probe on first
    /// embed call). When `None`, falls back to `Fts5Embedder` (FTS5-only).
    pub gpu_service_url: Option<String>,
    /// Target embedding dimension (used by `GpuEmbedder`). Ignored by
    /// `Fts5Embedder` and `NullEmbedder`.
    pub embedding_dim: usize,
}

impl Default for EmbedderConfig {
    fn default() -> Self {
        Self {
            disabled: false,
            gpu_service_url: None,
            embedding_dim: 384,
        }
    }
}

/// Build the embedder appropriate for a given runtime configuration.
///
/// Selection logic (in order):
///   1. `config.disabled == true` → `NullEmbedder`
///   2. `config.gpu_service_url == Some(url)` + `gpu-embeddings` feature → `GpuEmbedder<Disconnected>`
///   3. Default → `Fts5Embedder` (FTS5-only mode)
///
/// Returns `Box<dyn Embedder>` so consumers can store the value behind a
/// trait object and swap backends without touching call sites — exactly the
/// abstraction the W3.2 boundary asks for. The concrete impls live in
/// `client.rs` in this crate (relocated from `touring-foundation` in A4 P4).
///
/// # Example
///
/// ```ignore
/// use touring_storage::embedding::{make_embedder, EmbedderConfig};
///
/// let cfg = EmbedderConfig {
///     gpu_service_url: Some("http://localhost:8200".into()),
///     embedding_dim: 384,
///     ..Default::default()
/// };
/// let embedder = make_embedder(&cfg);
/// let _vec = embedder.embed_single("hello").await;
/// ```
#[must_use]
pub fn make_embedder(config: &EmbedderConfig) -> Box<dyn Embedder> {
    if config.disabled {
        return Box::new(NullEmbedder::new());
    }
    #[cfg(feature = "gpu-embeddings")]
    {
        if let Some(ref url) = config.gpu_service_url {
            return Box::new(GpuEmbedder::new(url.clone(), config.embedding_dim));
        }
    }
    Box::new(Fts5Embedder::new())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn make_embedder_disabled_returns_null() {
        let cfg = EmbedderConfig {
            disabled: true,
            ..Default::default()
        };
        let e = make_embedder(&cfg);
        assert!(
            e.is_healthy(),
            "NullEmbedder must report is_healthy() == true"
        );
    }
    #[test]
    fn make_embedder_no_url_returns_fts5() {
        let cfg = EmbedderConfig::default();
        let e = make_embedder(&cfg);
        assert!(
            !e.is_healthy(),
            "Fts5Embedder must report is_healthy() == false"
        );
    }
    #[test]
    fn embedder_config_default_dim_is_384() {
        let cfg = EmbedderConfig::default();
        assert_eq!(cfg.embedding_dim, 384);
        assert!(!cfg.disabled);
        assert!(cfg.gpu_service_url.is_none());
    }
    #[tokio::test]
    async fn e2e_disabled_config_embed_single_returns_none() {
        let cfg = EmbedderConfig {
            disabled: true,
            ..Default::default()
        };
        let embedder: Box<dyn Embedder> = make_embedder(&cfg);
        let result = embedder.embed_single("any query").await;
        assert!(
            result.is_none(),
            "NullEmbedder must yield None for embed_single"
        );
    }
    #[tokio::test]
    async fn e2e_fts5_fallback_batch_yields_none_vector() {
        let cfg = EmbedderConfig::default();
        let embedder: Box<dyn Embedder> = make_embedder(&cfg);
        let texts = vec!["alpha", "beta", "gamma"];
        let batch = embedder.embed_batch(&texts).await;
        assert_eq!(batch.len(), 3, "batch len must mirror input len");
        assert!(
            batch.iter().all(Option::is_none),
            "Fts5Embedder yields all-None"
        );
    }
    #[tokio::test]
    async fn e2e_health_check_propagates_through_dyn_dispatch() {
        let null_cfg = EmbedderConfig {
            disabled: true,
            ..Default::default()
        };
        let null: Box<dyn Embedder> = make_embedder(&null_cfg);
        assert!(null.health_check().await);
        let fts5_cfg = EmbedderConfig::default();
        let fts5: Box<dyn Embedder> = make_embedder(&fts5_cfg);
        assert!(!fts5.health_check().await);
    }
    #[test]
    fn e2e_config_clone_and_debug_round_trip() {
        let original = EmbedderConfig {
            disabled: false,
            gpu_service_url: Some("http://localhost:8200".into()),
            embedding_dim: 768,
        };
        let cloned = original.clone();
        assert_eq!(cloned.disabled, original.disabled);
        assert_eq!(cloned.gpu_service_url, original.gpu_service_url);
        assert_eq!(cloned.embedding_dim, original.embedding_dim);
        let debug_str = format!("{:?}", original);
        assert!(debug_str.contains("EmbedderConfig"));
        assert!(debug_str.contains("768"));
        assert!(debug_str.contains("8200"));
    }
    #[tokio::test]
    async fn e2e_dyn_embedder_is_send_sync_for_arc_usage() {
        let cfg = EmbedderConfig::default();
        let embedder: std::sync::Arc<dyn Embedder> = std::sync::Arc::from(make_embedder(&cfg));
        let handle =
            tokio::spawn(async move { embedder.embed_single("cross-task probe").await.is_none() });
        let result = handle.await.expect("task should complete");
        assert!(result, "Fts5Embedder yields None across thread boundary");
    }
}
