//! Embedding client — pluggable async embedder implementations.
//!
//! This module provides the [`Embedder`] trait and three built-in implementations.
//! The `gpu-embeddings` feature enables [`GpuEmbedder`]; without it only
//! [`Fts5Embedder`] and [`NullEmbedder`] are available.

use async_trait::async_trait;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

// ─────────────────────────────────────────────────────────────────────────────
// Connection state machine — type-state markers
// ─────────────────────────────────────────────────────────────────────────────

/// Type-state marker for unconnected embedder.
/// Legal transitions: → [`Connected`] (health_check succeeds), → [`Degraded`] (health_check fails).
#[derive(Debug)]
pub struct Disconnected;

/// Type-state marker for successfully connected embedder.
/// Legal transitions: → [`Disconnected`] (connection lost), → [`Degraded`] (health_check fails).
#[derive(Debug)]
pub struct Connected;

/// Type-state marker for FTS5-only fallback mode (terminal — no outgoing transitions).
/// Legal transitions: none (terminal state).
#[derive(Debug)]
pub struct Degraded;

/// Sealed trait implemented by every valid type-state marker
/// ([`Disconnected`], [`Connected`], [`Degraded`]). Use it in
/// generic bounds to accept "any connection state" without
/// exposing the sealed impl.
pub trait ConnectionStateMarker: private::Sealed {}
impl ConnectionStateMarker for Disconnected {}
impl ConnectionStateMarker for Connected {}
impl ConnectionStateMarker for Degraded {}

// Sealing helper — prevents downstream impl
mod private {
    pub trait Sealed {}
    impl Sealed for super::Disconnected {}
    impl Sealed for super::Connected {}
    impl Sealed for super::Degraded {}
}

/// Represents the runtime connection health state of an [`Embedder`].
///
/// This enum mirrors the type-state markers ([`Disconnected`], [`Connected`], [`Degraded`])
/// for code that needs runtime state inspection without generics.
///
/// ```text
/// ┌──────────────┐   health_check succeeds   ┌─────────────┐
/// │ Disconnected │ ─────────────────────────► │  Connected  │
/// └──────────────┘                           └─────────────┘
///        ▲                                         │
///        │         health_check fails              │
///        └────────────────────────────────────────┘
///                          │
///                          ▼
///                  ┌─────────────┐
///                  │  Degraded   │ ◄── FTS5-only fallback
///                  └─────────────┘
/// ```
///
/// | State | `is_healthy()` | `embed_single()` |
/// |---|---|---|
/// | `Disconnected` | `false` | Returns `None` (tries health_check first) |
/// | `Connected` | `true` | Calls GPU service |
/// | `Degraded` | `false` | Returns `None` immediately (FTS5-only mode) |
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum ConnectionState {
    /// Not yet connected, or connection was lost.
    #[default]
    Disconnected,
    /// Successfully connected to the backend.
    Connected,
    /// Backend is unavailable — FTS5-only mode.
    Degraded,
}

impl ConnectionState {
    /// Convert runtime state to corresponding type-state marker.
    #[inline]
    #[must_use]
    pub const fn to_marker(&self) -> &'static str {
        match self {
            Self::Disconnected => "Disconnected",
            Self::Connected => "Connected",
            Self::Degraded => "Degraded",
        }
    }
}

impl std::fmt::Display for ConnectionState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Disconnected => write!(f, "Disconnected"),
            Self::Connected => write!(f, "Connected"),
            Self::Degraded => write!(f, "Degraded"),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Embedder trait
// ─────────────────────────────────────────────────────────────────────────────

/// Pluggable embedding backend.
///
/// This trait enables runtime-polymorphic embedding backends via `dyn Embedder`.
/// All methods are async and thread-safe (`Send + Sync`).
///
/// # Implementations
///
/// | Implementation | Feature | Behavior |
/// |---|---|---|
/// | [`GpuEmbedder`] | `gpu-embeddings` | Real GPU service, graceful degradation |
/// | [`Fts5Embedder`] | default | No-op fallback (always returns `None`) |
/// | [`NullEmbedder`] | default | No-op for tests (always returns `None`) |
#[async_trait]
pub trait Embedder: Send + Sync {
    /// Embed a single text. Returns `None` if the embedder is unavailable.
    async fn embed_single(&self, text: &str) -> Option<Vec<f32>>;
    /// Embed a batch of texts. Returns `Vec<Option<Vec<f32>>>` matching input order.
    /// Failed embeddings are `None`.
    async fn embed_batch(&self, texts: &[&str]) -> Vec<Option<Vec<f32>>>;
    /// Whether the embedder is currently considered healthy.
    fn is_healthy(&self) -> bool;
    /// Probe the backend for health and update internal state.
    /// Returns `true` if the backend is reachable and responsive.
    async fn health_check(&self) -> bool;
}

// ─────────────────────────────────────────────────────────────────────────────
// GpuEmbedder (GPU-backed, requires gpu-embeddings feature)
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(feature = "gpu-embeddings")]
mod gpu {
    use super::*;
    use rkyv::{Archive, Deserialize, Serialize};
    use std::marker::PhantomData;
    /// Zero-copy serializable request for GPU embedding service via UDS.
    #[derive(Debug, Archive, Serialize, Deserialize)]
    #[archive(check_bytes)]
    #[archive_attr(derive(Debug))]
    pub struct IpcEmbedRequest {
        /// Texts to embed. The GPU service processes them in one
        /// batch (subject to `batch_size` slicing).
        pub texts: Vec<String>,
        /// Preferred batch size for the GPU service's micro-batching
        /// loop. The service MAY split larger inputs.
        pub batch_size: usize,
        /// Maximum token length per text (longer texts are truncated
        /// before tokenisation).
        pub max_length: usize,
    }
    /// Zero-copy serializable response from GPU embedding service via UDS.
    #[derive(Debug, Archive, Serialize, Deserialize)]
    #[archive(check_bytes)]
    #[archive_attr(derive(Debug))]
    pub struct IpcEmbedResponse {
        /// One embedding vector per input text. Order matches
        /// `IpcEmbedRequest::texts`.
        pub embeddings: Vec<Vec<f32>>,
        /// Name of the model that produced the vectors — useful for
        /// logging and for migrating to a different model without
        /// recomputing the entire index.
        pub model: Option<String>,
        /// Embedding dimension of the response. Cross-checked
        /// against the client's expected `embedding_dim`.
        pub dimension: Option<usize>,
    }
    /// GPU-backed embedder using rkyv IPC over Unix Domain Socket — replaces
    /// HTTP/JSON with zero-copy serialization for sub-millisecond determinism.
    ///
    /// The type-state parameter `S` tracks the connection state at compile time
    /// identically to [`GpuEmbedder`](super::GpuEmbedder).
    #[derive(Debug, Clone)]
    pub struct RkyvGpuBackend<State = Disconnected> {
        /// Path to the Unix Domain Socket the GPU service is bound
        /// to. Configured at construction time.
        socket_path: String,
        /// Expected embedding dimension. Used to validate every
        /// `IpcEmbedResponse::dimension` and to pre-allocate result
        /// vectors.
        embedding_dim: usize,
        /// Shared health flag — flipped by `health_check_impl` and
        /// read by `is_healthy`. Shared via `Arc` so trait objects
        /// can observe the state.
        healthy: Arc<AtomicBool>,
        /// Compile-time only — `PhantomData<State>` carries the
        /// type-state marker without storing a value.
        _state: PhantomData<State>,
    }
    impl<State> RkyvGpuBackend<State> {
        /// Return the configured embedding dimension (set at
        /// construction; immutable for the lifetime of the
        /// backend).
        #[inline]
        #[must_use]
        pub fn embedding_dim(&self) -> usize {
            self.embedding_dim
        }
        /// Return the Unix Domain Socket path the GPU service
        /// is bound to. Used for diagnostics and reconnect
        /// logic.
        #[inline]
        #[must_use]
        pub fn socket_path(&self) -> &str {
            &self.socket_path
        }
        fn is_healthy_impl(&self) -> bool {
            self.healthy.load(Ordering::Relaxed)
        }
        async fn health_check_impl(&self) -> bool {
            let result = tokio::net::UnixStream::connect(&self.socket_path).await;
            let is_healthy = result.is_ok();
            let was_healthy = self.healthy.swap(is_healthy, Ordering::Relaxed);
            if was_healthy && !is_healthy {
                tracing::warn!(
                    "GPU UDS service became unhealthy — transitioning to FTS5-only mode"
                );
            } else if !was_healthy && is_healthy {
                tracing::info!("GPU UDS service recovered — semantic recall re-enabled");
            }
            is_healthy
        }
        async fn embed_texts_impl(
            &self,
            texts: &[&str],
        ) -> Result<Vec<Vec<f32>>, super::EmbeddingError> {
            if texts.is_empty() {
                return Ok(Vec::new());
            }
            let request = IpcEmbedRequest {
                texts: texts.iter().map(|s| (*s).to_owned()).collect(),
                batch_size: 32,
                max_length: 512,
            };
            let bytes = rkyv::to_bytes::<_, 256>(&request).map_err(|e| {
                self.healthy.store(false, Ordering::Relaxed);
                super::EmbeddingError::ParseError(format!("rkyv serialize: {}", e))
            })?;
            let mut socket = tokio::net::UnixStream::connect(&self.socket_path)
                .await
                .map_err(|e| {
                    self.healthy.store(false, Ordering::Relaxed);
                    super::EmbeddingError::HttpError(format!("uds connect: {}", e))
                })?;
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            socket.write_all(&bytes).await.map_err(|e| {
                self.healthy.store(false, Ordering::Relaxed);
                super::EmbeddingError::HttpError(format!("uds write: {}", e))
            })?;
            let mut len_buf = [0u8; 4];
            socket.read_exact(&mut len_buf).await.map_err(|e| {
                self.healthy.store(false, Ordering::Relaxed);
                super::EmbeddingError::HttpError(format!("uds read len: {}", e))
            })?;
            let len = u32::from_le_bytes(len_buf) as usize;
            let mut resp_buf = vec![0u8; len];
            socket.read_exact(&mut resp_buf).await.map_err(|e| {
                self.healthy.store(false, Ordering::Relaxed);
                super::EmbeddingError::HttpError(format!("uds read body: {}", e))
            })?;
            let resp: IpcEmbedResponse = rkyv::from_bytes(&resp_buf).map_err(|e| {
                self.healthy.store(false, Ordering::Relaxed);
                super::EmbeddingError::ParseError(format!("rkyv deserialize: {}", e))
            })?;
            if let Some(ref model_name) = resp.model {
                tracing::debug!(
                    model = % model_name, "RkyvGpuBackend: model reported by service"
                );
            }
            if let Some(dim) = resp.dimension {
                if dim != self.embedding_dim {
                    return Err(super::EmbeddingError::DimensionMismatch {
                        expected: self.embedding_dim,
                        actual: dim,
                    });
                }
            }
            for (i, emb) in resp.embeddings.iter().enumerate() {
                if emb.len() != self.embedding_dim {
                    return Err(super::EmbeddingError::DimensionMismatch {
                        expected: self.embedding_dim,
                        actual: emb.len(),
                    });
                }
                if emb.iter().any(|v| v.is_nan() || v.is_infinite()) {
                    return Err(super::EmbeddingError::InvalidEmbedding(format!(
                        "NaN/Inf detected in embedding at index {}",
                        i
                    )));
                }
            }
            if resp.embeddings.len() != texts.len() {
                return Err(super::EmbeddingError::CountMismatch {
                    expected: texts.len(),
                    actual: resp.embeddings.len(),
                });
            }
            self.healthy.store(true, Ordering::Relaxed);
            Ok(resp.embeddings)
        }
        async fn embed_single_impl(&self, text: &str) -> Option<Vec<f32>> {
            if !self.is_healthy_impl() && !self.health_check_impl().await {
                tracing::warn!(
                    "GPU UDS service offline — semantic recall degraded to FTS5-only mode. \
                     Start the GPU service at {} to enable full semantic search.",
                    self.socket_path
                );
                return None;
            }
            match self.embed_texts_impl(&[text]).await {
                Ok(mut embeddings) => embeddings.pop(),
                Err(e) => {
                    tracing::debug!("embed_single failed (FTS5-only mode): {}", e);
                    None
                }
            }
        }
        async fn embed_batch_impl(&self, texts: &[&str]) -> Vec<Option<Vec<f32>>> {
            if !self.is_healthy_impl() && !self.health_check_impl().await {
                return texts.iter().map(|_| None).collect();
            }
            match self.embed_texts_impl(texts).await {
                Ok(embeddings) => embeddings.into_iter().map(Some).collect(),
                Err(e) => {
                    tracing::warn!("embed_batch failed for {} texts: {}", texts.len(), e);
                    texts.iter().map(|_| None).collect()
                }
            }
        }
    }
    impl RkyvGpuBackend<Disconnected> {
        /// Create a new rkyv-UDS GPU embedder targeting the given socket path.
        pub fn new(socket_path: impl Into<String>, embedding_dim: usize) -> Self {
            Self {
                socket_path: socket_path.into(),
                embedding_dim,
                healthy: Arc::new(AtomicBool::new(false)),
                _state: PhantomData,
            }
        }
        /// Create with default GPU service socket path and dimension (384).
        pub fn default_uds() -> Self {
            Self::new("/tmp/gpu-embedder.sock", 384)
        }
        /// Attempt to connect — runs health_check and returns a [`Connected`] embedder
        /// on success, or a [`Degraded`] embedder on failure.
        pub async fn connect(self) -> RkyvGpuBackend<Connected> {
            let _ = self.health_check_impl().await;
            RkyvGpuBackend {
                socket_path: self.socket_path,
                embedding_dim: self.embedding_dim,
                healthy: self.healthy,
                _state: PhantomData,
            }
        }
    }
    impl RkyvGpuBackend<Connected> {
        /// Transition from `Connected` to `Degraded`. Idempotent
        /// — safe to call repeatedly on health-check failure.
        #[inline]
        pub fn mark_degraded(self) -> RkyvGpuBackend<Degraded> {
            RkyvGpuBackend {
                socket_path: self.socket_path,
                embedding_dim: self.embedding_dim,
                healthy: self.healthy,
                _state: PhantomData,
            }
        }
        /// Disconnect and transition back to `Disconnected`.
        /// The socket is closed; the backend can be re-`connect`ed.
        #[inline]
        pub fn disconnect(self) -> RkyvGpuBackend<Disconnected> {
            self.healthy.store(false, Ordering::Relaxed);
            RkyvGpuBackend {
                socket_path: self.socket_path,
                embedding_dim: self.embedding_dim,
                healthy: self.healthy,
                _state: PhantomData,
            }
        }
    }
    impl RkyvGpuBackend<Degraded> {
        /// Reset from `Degraded` back to `Disconnected`. Used
        /// when the host decides to abandon the fallback and
        /// try a fresh primary connection.
        #[inline]
        pub fn reset(self) -> RkyvGpuBackend<Disconnected> {
            self.healthy.store(false, Ordering::Relaxed);
            RkyvGpuBackend {
                socket_path: self.socket_path,
                embedding_dim: self.embedding_dim,
                healthy: self.healthy,
                _state: PhantomData,
            }
        }
    }
    #[async_trait]
    impl<State: std::marker::Send + std::marker::Sync> super::Embedder for RkyvGpuBackend<State> {
        async fn embed_single(&self, text: &str) -> Option<Vec<f32>> {
            self.embed_single_impl(text).await
        }
        async fn embed_batch(&self, texts: &[&str]) -> Vec<Option<Vec<f32>>> {
            self.embed_batch_impl(texts).await
        }
        fn is_healthy(&self) -> bool {
            self.is_healthy_impl()
        }
        async fn health_check(&self) -> bool {
            self.health_check_impl().await
        }
    }
    /// Request payload for GPU service /embed endpoint.
    #[derive(Debug, serde::Serialize)]
    pub(super) struct EmbedRequest<'a> {
        texts: &'a [&'a str],
        batch_size: usize,
        max_length: usize,
    }
    /// Response from GPU service /embed endpoint.
    #[derive(Debug, serde::Deserialize)]
    pub(super) struct EmbedResponse {
        embeddings: Vec<Vec<f32>>,
        model: Option<String>,
        dimension: Option<usize>,
    }
    /// GPU-backed embedder — connects to a GPU embedding service (default:
    /// `http://localhost:8200`) and gracefully degrades to FTS5-only mode
    /// when the service is offline.
    ///
    /// The type-state parameter `S` tracks the connection state at compile time:
    /// - [`Disconnected`](super::Disconnected): initial state, no health check run yet.
    ///   Transition: `connect()` → `Connected`.
    /// - [`Connected`](super::Connected): health check succeeded. Embedding available.
    ///   Transitions: `mark_degraded()` → `Degraded`, `disconnect()` → `Disconnected`.
    /// - [`Degraded`](super::Degraded): FTS5-only fallback (terminal state).
    ///   No outgoing transitions — callers must create a new `GpuEmbedder`.
    ///
    /// The [`Embedder`] trait is implemented for all `GpuEmbedder<S>` via a blanket impl,
    /// preserving runtime-polymorphic use via `dyn Embedder`.
    #[derive(Debug, Clone)]
    pub struct GpuEmbedder<State = Disconnected> {
        http: reqwest::Client,
        gpu_url: String,
        embedding_dim: usize,
        healthy: Arc<AtomicBool>,
        _state: PhantomData<State>,
    }
    impl<State> GpuEmbedder<State> {
        /// Returns the configured embedding dimension.
        #[inline]
        #[must_use]
        pub fn embedding_dim(&self) -> usize {
            self.embedding_dim
        }
        /// Returns the raw gpu service URL.
        #[inline]
        #[must_use]
        pub fn gpu_url(&self) -> &str {
            &self.gpu_url
        }
        fn is_healthy_impl(&self) -> bool {
            self.healthy.load(Ordering::Relaxed)
        }
        async fn health_check_impl(&self) -> bool {
            let url = format!("{}/health", self.gpu_url);
            let result = self
                .http
                .get(&url)
                .timeout(Duration::from_secs(3))
                .send()
                .await;
            let is_healthy = matches!(result, Ok(resp) if resp.status().is_success());
            let was_healthy = self.healthy.swap(is_healthy, Ordering::Relaxed);
            if was_healthy && !is_healthy {
                tracing::warn!(
                    "GPU embedding service became unhealthy — transitioning to FTS5-only mode"
                );
            } else if !was_healthy && is_healthy {
                tracing::info!("GPU embedding service recovered — semantic recall re-enabled");
            }
            is_healthy
        }
        async fn embed_texts_impl(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, EmbeddingError> {
            if texts.is_empty() {
                return Ok(Vec::new());
            }
            let url = format!("{}/api/v1/embed", self.gpu_url);
            let request = EmbedRequest {
                texts,
                batch_size: 32,
                max_length: 512,
            };
            let resp = self
                .http
                .post(&url)
                .json(&request)
                .timeout(Duration::from_secs(30))
                .send()
                .await
                .map_err(|e| {
                    self.healthy.store(false, Ordering::Relaxed);
                    EmbeddingError::HttpError(e.to_string())
                })?;
            if !resp.status().is_success() {
                let status = resp.status().as_u16();
                let body = resp.text().await.unwrap_or_default();
                self.healthy.store(false, Ordering::Relaxed);
                return Err(EmbeddingError::ServiceError { status, body });
            }
            let embed_resp: EmbedResponse = resp
                .json()
                .await
                .map_err(|e| EmbeddingError::ParseError(e.to_string()))?;
            if let Some(ref model_name) = embed_resp.model {
                tracing::debug!(
                    model = % model_name, "GPU embedder: model reported by service"
                );
            }
            if let Some(dim) = embed_resp.dimension {
                if dim != self.embedding_dim {
                    return Err(EmbeddingError::DimensionMismatch {
                        expected: self.embedding_dim,
                        actual: dim,
                    });
                }
            }
            for (i, emb) in embed_resp.embeddings.iter().enumerate() {
                if emb.len() != self.embedding_dim {
                    return Err(EmbeddingError::DimensionMismatch {
                        expected: self.embedding_dim,
                        actual: emb.len(),
                    });
                }
                if emb.iter().any(|v| v.is_nan() || v.is_infinite()) {
                    return Err(EmbeddingError::InvalidEmbedding(format!(
                        "NaN/Inf detected in embedding at index {i}"
                    )));
                }
            }
            if embed_resp.embeddings.len() != texts.len() {
                return Err(EmbeddingError::CountMismatch {
                    expected: texts.len(),
                    actual: embed_resp.embeddings.len(),
                });
            }
            self.healthy.store(true, Ordering::Relaxed);
            Ok(embed_resp.embeddings)
        }
        async fn embed_single_impl(&self, text: &str) -> Option<Vec<f32>> {
            #[allow(clippy::collapsible_if)]
            if !self.is_healthy_impl() {
                if !self.health_check_impl().await {
                    tracing::warn!(
                        "GPU embedding service offline — semantic recall degraded to FTS5-only mode. \
                         Start the GPU service at {} to enable full semantic search.",
                        self.gpu_url
                    );
                    return None;
                }
            }
            match self.embed_texts_impl(&[text]).await {
                Ok(mut embeddings) => embeddings.pop(),
                Err(e) => {
                    tracing::debug!("embed_single failed (FTS5-only mode): {}", e);
                    None
                }
            }
        }
        async fn embed_batch_impl(&self, texts: &[&str]) -> Vec<Option<Vec<f32>>> {
            if !self.is_healthy_impl() && !self.health_check_impl().await {
                return texts.iter().map(|_| None).collect();
            }
            match self.embed_texts_impl(texts).await {
                Ok(embeddings) => embeddings.into_iter().map(Some).collect(),
                Err(e) => {
                    tracing::warn!("embed_batch failed for {} texts: {}", texts.len(), e);
                    texts.iter().map(|_| None).collect()
                }
            }
        }
    }
    impl GpuEmbedder<Disconnected> {
        /// Create a new GPU embedder targeting the given URL and embedding dimension.
        /// Starts in [`Disconnected`] state — call `connect()` to attempt connection.
        pub fn new(gpu_url: impl Into<String>, embedding_dim: usize) -> Self {
            let http = reqwest::Client::builder()
                .timeout(Duration::from_secs(10))
                .connect_timeout(Duration::from_secs(3))
                .pool_max_idle_per_host(4)
                .build()
                .unwrap_or_default();
            Self {
                http,
                gpu_url: gpu_url.into(),
                embedding_dim,
                healthy: Arc::new(AtomicBool::new(false)),
                _state: PhantomData,
            }
        }
        /// Create with default GPU service URL and dimension (384).
        pub fn default_gpu() -> Self {
            Self::new("http://localhost:8200", 384)
        }
        /// Attempt to connect — runs health_check and returns a [`Connected`] embedder
        /// on success, or a [`Degraded`] embedder on failure.
        ///
        /// This is the only legal way to transition OUT of [`Disconnected`].
        pub async fn connect(self) -> GpuEmbedder<Connected> {
            // Run the health-check probe (for its side effects); the resulting
            // `Connected` embedder is the same regardless (the `healthy` field
            // carries `self.healthy`). The original had identical if/else
            // branches here — collapsed in the A4 P4 move to satisfy
            // `clippy::if_same_then_else`; behaviour is byte-identical.
            let _ = self.health_check_impl().await;
            GpuEmbedder {
                http: self.http,
                gpu_url: self.gpu_url,
                embedding_dim: self.embedding_dim,
                healthy: self.healthy,
                _state: PhantomData,
            }
        }
    }
    impl GpuEmbedder<Connected> {
        /// Transition to [`Degraded`] state (FTS5-only fallback).
        /// Call this when the GPU service is confirmed unreachable.
        #[inline]
        pub fn mark_degraded(self) -> GpuEmbedder<Degraded> {
            GpuEmbedder {
                http: self.http,
                gpu_url: self.gpu_url,
                embedding_dim: self.embedding_dim,
                healthy: self.healthy,
                _state: PhantomData,
            }
        }
        /// Transition back to [`Disconnected`] state.
        #[inline]
        pub fn disconnect(self) -> GpuEmbedder<Disconnected> {
            self.healthy.store(false, Ordering::Relaxed);
            GpuEmbedder {
                http: self.http,
                gpu_url: self.gpu_url,
                embedding_dim: self.embedding_dim,
                healthy: self.healthy,
                _state: PhantomData,
            }
        }
    }
    impl GpuEmbedder<Degraded> {
        /// Transition from Degraded back to [`Disconnected`] to retry connection.
        #[inline]
        pub fn reset(self) -> GpuEmbedder<Disconnected> {
            self.healthy.store(false, Ordering::Relaxed);
            GpuEmbedder {
                http: self.http,
                gpu_url: self.gpu_url,
                embedding_dim: self.embedding_dim,
                healthy: self.healthy,
                _state: PhantomData,
            }
        }
    }
    #[async_trait]
    impl<State: std::marker::Send + std::marker::Sync> Embedder for GpuEmbedder<State> {
        async fn embed_single(&self, text: &str) -> Option<Vec<f32>> {
            self.embed_single_impl(text).await
        }
        async fn embed_batch(&self, texts: &[&str]) -> Vec<Option<Vec<f32>>> {
            self.embed_batch_impl(texts).await
        }
        fn is_healthy(&self) -> bool {
            self.is_healthy_impl()
        }
        async fn health_check(&self) -> bool {
            self.health_check_impl().await
        }
    }
}

#[cfg(feature = "gpu-embeddings")]
pub use gpu::GpuEmbedder;
/// rkyv-backed GPU embedder using Unix Domain Socket — zero-copy IPC.
/// Available when `gpu-embeddings` feature is enabled.
#[cfg(feature = "gpu-embeddings")]
pub use gpu::RkyvGpuBackend;

/// Stub GpuEmbedder when `gpu-embeddings` feature is disabled.
/// All operations return `None` or `false`.
#[cfg(not(feature = "gpu-embeddings"))]
#[derive(Debug, Clone)]
pub struct GpuEmbedder<State = Disconnected>(std::marker::PhantomData<State>);

#[cfg(not(feature = "gpu-embeddings"))]
impl<State> GpuEmbedder<State> {
    /// Construct a stub [`GpuEmbedder`]. Arguments are accepted for
    /// API parity with the real backend but are ignored.
    pub fn new(gpu_url: impl Into<String>, embedding_dim: usize) -> Self {
        let _ = (gpu_url, embedding_dim);
        Self(std::marker::PhantomData)
    }
    /// Construct a stub [`GpuEmbedder`] with default GPU
    /// parameters. Same shape as the real `default_gpu` constructor.
    pub fn default_gpu() -> Self {
        Self(std::marker::PhantomData)
    }
    /// Return the configured embedding dimension (fixed at 384
    /// in the stub — the real backend uses the value passed to
    /// `new`).
    pub fn embedding_dim(&self) -> usize {
        384
    }
}

#[cfg(not(feature = "gpu-embeddings"))]
impl<State> GpuEmbedder<State> {
    /// Attempt to connect — stub always returns Degraded.
    pub async fn connect(self) -> GpuEmbedder<Degraded> {
        GpuEmbedder(std::marker::PhantomData)
    }
}

#[cfg(not(feature = "gpu-embeddings"))]
impl GpuEmbedder<Degraded> {
    /// Reset from degraded — stub returns Disconnected.
    pub fn reset(self) -> GpuEmbedder<Disconnected> {
        GpuEmbedder(std::marker::PhantomData)
    }
}

#[cfg(not(feature = "gpu-embeddings"))]
#[async_trait]
impl<State> Embedder for GpuEmbedder<State> {
    async fn embed_single(&self, _text: &str) -> Option<Vec<f32>> {
        None
    }
    async fn embed_batch(&self, texts: &[&str]) -> Vec<Option<Vec<f32>>> {
        texts.iter().map(|_| None).collect()
    }
    fn is_healthy(&self) -> bool {
        false
    }
    async fn health_check(&self) -> bool {
        false
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Fts5Embedder — fallback when GPU is unavailable
// ─────────────────────────────────────────────────────────────────────────────

/// FTS5-only embedder — always returns `None`.
///
/// Use this as a fallback when the GPU service is offline. It never attempts
/// to connect and always signals "unhealthy" so callers know to fall back to
/// FTS5 search.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Fts5Embedder;

impl Fts5Embedder {
    /// Construct a new [`Fts5Embedder`]. The struct is a unit
    /// type; this exists only to match the [`Embedder`] trait
    /// shape and to give callers a uniform `::new()` entry point.
    #[inline]
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Embedder for Fts5Embedder {
    #[inline]
    async fn embed_single(&self, _text: &str) -> Option<Vec<f32>> {
        None
    }
    #[inline]
    async fn embed_batch(&self, texts: &[&str]) -> Vec<Option<Vec<f32>>> {
        texts.iter().map(|_| None).collect()
    }
    /// Always returns `false` — FTS5 mode is a degraded state.
    #[inline]
    fn is_healthy(&self) -> bool {
        false
    }
    /// Always returns `false` — no backend to check.
    #[inline]
    async fn health_check(&self) -> bool {
        false
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// NullEmbedder — always-None embedder for testing
// ─────────────────────────────────────────────────────────────────────────────

/// No-op embedder that always returns `None`.
///
/// Useful for tests and environments where embeddings should be fully disabled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct NullEmbedder;

impl NullEmbedder {
    /// Construct a new [`NullEmbedder`]. The struct is a unit
    /// type; this exists only to give callers a uniform
    /// `::new()` entry point that mirrors the other embedder
    /// constructors.
    #[inline]
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Embedder for NullEmbedder {
    #[inline]
    async fn embed_single(&self, _text: &str) -> Option<Vec<f32>> {
        None
    }
    #[inline]
    async fn embed_batch(&self, texts: &[&str]) -> Vec<Option<Vec<f32>>> {
        #[allow(clippy::iter_with_drain)]
        std::iter::repeat_with(|| None).take(texts.len()).collect()
    }
    /// Always returns `true` — null embedder is always "healthy".
    #[inline]
    fn is_healthy(&self) -> bool {
        true
    }
    /// Always returns `true` — no backend to check.
    #[inline]
    async fn health_check(&self) -> bool {
        true
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Legacy type alias for backwards compatibility
// ─────────────────────────────────────────────────────────────────────────────

/// Legacy type alias — prefer [`GpuEmbedder`] for new code.
///
/// `EmbeddingClient` is the old name for the GPU embedder. It is not a trait
/// object; use `dyn Embedder` for polymorphic embedding.
#[deprecated(
    since = "0.2.0",
    note = "use GpuEmbedder (concrete) or dyn Embedder (trait object)"
)]
pub type EmbeddingClient = GpuEmbedder;

// ─────────────────────────────────────────────────────────────────────────────
// EmbeddingError
// ─────────────────────────────────────────────────────────────────────────────

/// Errors specific to embedding operations.
#[derive(Debug, thiserror::Error)]
pub enum EmbeddingError {
    /// Transport-level HTTP failure (DNS, connection refused, TLS
    /// handshake, etc.). String carries the underlying
    /// `reqwest::Error` or `ureq::Error` message.
    #[error("HTTP error: {0}")]
    HttpError(String),
    /// GPU service returned a non-2xx HTTP status. `status` is the
    /// numeric code; `body` is the response body (truncated to
    /// ~1 KiB by the service).
    #[error("GPU service error (HTTP {status}): {body}")]
    ServiceError {
        /// Numeric HTTP status code (e.g. `500`, `503`).
        status: u16,
        /// Truncated response body for diagnostics.
        body: String,
    },
    /// Response could not be parsed into the expected shape
    /// (typically a JSON deserialisation failure).
    #[error("Failed to parse response: {0}")]
    ParseError(String),
    /// The embedding dimension returned by the service does not
    /// match the client's expected dimension. Re-validate the
    /// configured model — the index may need to be rebuilt.
    #[error("Dimension mismatch: expected {expected}, got {actual}")]
    DimensionMismatch {
        /// Expected embedding dimension.
        expected: usize,
        /// Actual dimension returned by the service.
        actual: usize,
    },
    /// An individual embedding vector failed validation (NaN,
    /// Inf, or wrong inner length).
    #[error("Invalid embedding: {0}")]
    InvalidEmbedding(String),
    /// The number of embeddings returned does not match the
    /// number of input texts. Indicates a service-level bug.
    #[error("Count mismatch: expected {expected} embeddings, got {actual}")]
    CountMismatch {
        /// Number of embeddings the caller expected.
        expected: usize,
        /// Number of embeddings actually returned.
        actual: usize,
    },
}

// `impl From<EmbeddingError> for touring_foundation::error::TouringError` was dropped in
// A4 P4: with EmbeddingError now local to touring-storage, that impl is an orphan-rule
// violation (foreign trait `From` + foreign `Self`). Consumers needing a TouringError
// construct it explicitly: `TouringError::Embedding(e.to_string())`.

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(feature = "gpu-embeddings")]
    mod gpu_tests {
        use super::*;
        #[test]
        fn test_gpu_embedder_creation() {
            let embedder = GpuEmbedder::new("http://localhost:8200", 384);
            assert_eq!(embedder.embedding_dim(), 384);
            assert!(!embedder.is_healthy());
        }
        #[test]
        fn test_gpu_embedder_default() {
            let embedder = GpuEmbedder::default_gpu();
            assert_eq!(embedder.embedding_dim(), 384);
        }
        #[tokio::test]
        async fn test_gpu_embedder_embed_single_offline() {
            let embedder = GpuEmbedder::new("http://127.0.0.1:19999", 384);
            let result = embedder.embed_single("test text").await;
            assert!(result.is_none());
        }
        #[tokio::test]
        async fn test_gpu_embedder_health_check_offline() {
            let embedder = GpuEmbedder::new("http://127.0.0.1:19999", 384);
            let healthy = embedder.health_check().await;
            assert!(!healthy);
            assert!(!embedder.is_healthy());
        }
        #[tokio::test]
        async fn test_gpu_embedder_batch_offline() {
            let embedder = GpuEmbedder::new("http://127.0.0.1:19999", 384);
            let texts = vec!["hello", "world"];
            let results = embedder.embed_batch(&texts).await;
            assert_eq!(results.len(), 2);
            assert!(results.iter().all(|r| r.is_none()));
        }
        // test_embedding_error_to_touring_error dropped with the From impl (A4 P4 orphan-rule).
    }
    mod common_tests {
        use super::*;
        #[tokio::test]
        async fn test_null_embedder_embed_single() {
            let embedder = NullEmbedder;
            assert!(embedder.embed_single("test").await.is_none());
        }
        #[tokio::test]
        async fn test_null_embedder_batch() {
            let embedder = NullEmbedder;
            let texts = vec!["a", "b", "c"];
            let results = embedder.embed_batch(&texts).await;
            assert_eq!(results.len(), 3);
            assert!(results.iter().all(|r| r.is_none()));
        }
        #[test]
        fn test_null_embedder_health() {
            let embedder = NullEmbedder;
            assert!(embedder.is_healthy());
        }
        #[tokio::test]
        async fn test_null_embedder_health_check() {
            let embedder = NullEmbedder;
            assert!(embedder.health_check().await);
        }
        #[tokio::test]
        async fn test_fts5_embedder_embed_single() {
            let embedder = Fts5Embedder;
            assert!(embedder.embed_single("test").await.is_none());
        }
        #[tokio::test]
        async fn test_fts5_embedder_batch() {
            let embedder = Fts5Embedder;
            let texts = vec!["a", "b"];
            let results = embedder.embed_batch(&texts).await;
            assert_eq!(results.len(), 2);
            assert!(results.iter().all(|r| r.is_none()));
        }
        #[test]
        fn test_fts5_embedder_health() {
            let embedder = Fts5Embedder;
            assert!(!embedder.is_healthy());
        }
        #[tokio::test]
        async fn test_fts5_embedder_health_check() {
            let embedder = Fts5Embedder;
            assert!(!embedder.health_check().await);
        }
        #[tokio::test]
        async fn test_dyn_embedder_null() {
            let embedder: Box<dyn Embedder> = Box::new(NullEmbedder);
            assert!(embedder.is_healthy());
            assert!(embedder.embed_single("test").await.is_none());
        }
        #[tokio::test]
        async fn test_dyn_embedder_fts5() {
            let embedder: Box<dyn Embedder> = Box::new(Fts5Embedder);
            assert!(!embedder.is_healthy());
            assert!(embedder.embed_single("test").await.is_none());
        }
        #[cfg(feature = "gpu-embeddings")]
        #[tokio::test]
        async fn test_dyn_embedder_gpu_offline() {
            let embedder: Box<dyn Embedder> =
                Box::new(GpuEmbedder::new("http://127.0.0.1:19999", 384));
            assert!(!embedder.is_healthy());
            assert!(embedder.embed_single("test").await.is_none());
        }
        #[test]
        fn test_connection_state_display() {
            assert_eq!(ConnectionState::Disconnected.to_string(), "Disconnected");
            assert_eq!(ConnectionState::Connected.to_string(), "Connected");
            assert_eq!(ConnectionState::Degraded.to_string(), "Degraded");
        }
        #[test]
        fn test_connection_state_default() {
            assert_eq!(ConnectionState::default(), ConnectionState::Disconnected);
        }
    }
}
