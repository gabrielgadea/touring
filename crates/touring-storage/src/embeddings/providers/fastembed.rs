//! FastEmbed provider implementation.
//!
//! Uses the `fastembed` crate for ONNX-based embedding inference.
//! FastEmbed provides efficient on-device (CPU) embedding generation without a
//! GPU and without a remote service.
//!
//! # Features
//! - `fastembed` feature must be enabled (default-on in `touring-storage`)
//! - Models are downloaded once into a pinned cache dir, then run fully offline
//! - Supports BGE Small (384d), BGE Large (1024d) and Snowflake Arctic-Embed-M
//!   (768d, retrieval-tuned)
//!
//! # History
//! Prior to 2026-05-29 every constructor hard-coded `AllMiniLML6V2` regardless
//! of the requested variant, and `dimensions()` reported 768 for BGE-small
//! (which is genuinely 384d). Both bugs are fixed here: each variant now loads
//! its real ONNX model and reports its true width. A pinned, CWD-independent
//! cache dir makes the daemon offline-deterministic.

#[cfg(not(feature = "fastembed"))]
use std::marker;
use std::path::PathBuf;
#[cfg(feature = "fastembed")]
use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use crate::embeddings::error::EmbeddingError;
use crate::embeddings::family::ModelFamily;
use crate::embeddings::{EmbeddingModel, EmbeddingProvider, EmbeddingResult};

/// FastEmbed model variants supported by this provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FastEmbedModel {
    /// BGE Large — `BAAI/bge-large-en-v1.5`, 1024 dimensions, highest BGE quality.
    BgeLarge,
    /// BGE Small — `BAAI/bge-small-en-v1.5`, 384 dimensions, fast default.
    BgeSmall,
    /// Snowflake Arctic-Embed-M — `Snowflake/snowflake-arctic-embed-m`, 768
    /// dimensions, retrieval-tuned (best-in-class open model at its size).
    ArcticEmbedM,
}

impl FastEmbedModel {
    /// Returns the true embedding dimension produced by this model.
    pub fn dimensions(&self) -> usize {
        match self {
            FastEmbedModel::BgeLarge => 1024,
            FastEmbedModel::BgeSmall => 384,
            FastEmbedModel::ArcticEmbedM => 768,
        }
    }

    /// Returns the HuggingFace model identifier string.
    pub fn model_id(&self) -> &'static str {
        match self {
            FastEmbedModel::BgeLarge => "BAAI/bge-large-en-v1.5",
            FastEmbedModel::BgeSmall => "BAAI/bge-small-en-v1.5",
            FastEmbedModel::ArcticEmbedM => "Snowflake/snowflake-arctic-embed-m",
        }
    }

    /// Maps to the concrete `fastembed::EmbeddingModel` enum variant.
    #[cfg(feature = "fastembed")]
    fn fastembed_model(&self) -> fastembed::EmbeddingModel {
        match self {
            FastEmbedModel::BgeLarge => fastembed::EmbeddingModel::BGELargeENV15,
            FastEmbedModel::BgeSmall => fastembed::EmbeddingModel::BGESmallENV15,
            FastEmbedModel::ArcticEmbedM => fastembed::EmbeddingModel::SnowflakeArcticEmbedM,
        }
    }
}

impl From<FastEmbedModel> for EmbeddingModel {
    fn from(model: FastEmbedModel) -> Self {
        match model {
            FastEmbedModel::BgeLarge => EmbeddingModel::FastEmbedBgeLarge,
            FastEmbedModel::BgeSmall => EmbeddingModel::FastEmbedBgeSmall,
            FastEmbedModel::ArcticEmbedM => EmbeddingModel::FastEmbedArcticM,
        }
    }
}

/// Resolves the on-disk cache directory for FastEmbed model weights.
///
/// Pinned (CWD-independent) so the daemon resolves the same cache regardless of
/// where it was spawned: `$TOURING_FASTEMBED_CACHE` if set, else
/// `~/.claude/touring/models/fastembed`. Keeping it out of any git workspace
/// also honours disk-hygiene (REGRA #12).
pub fn fastembed_cache_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("TOURING_FASTEMBED_CACHE") {
        return PathBuf::from(dir);
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home)
        .join(".claude")
        .join("touring")
        .join("models")
        .join("fastembed")
}

/// FastEmbed embedding provider.
///
/// Wraps the `fastembed` crate for efficient on-device embedding generation.
/// The model is loaded once and held for the provider's lifetime; inference is
/// synchronous CPU work (the `async` trait methods are thin wrappers so the
/// daemon hot path can call [`FastEmbedProvider::embed_one_sync`] directly).
///
/// When the `fastembed` feature is disabled — or when constructed via
/// [`FastEmbedProvider::new_stub`] — the provider produces deterministic
/// hash-based vectors at the model's declared width (offline, no download).
pub struct FastEmbedProvider {
    model: FastEmbedModel,
    /// `Some` = a real ONNX runtime is loaded; `None` = deterministic stub.
    #[cfg(feature = "fastembed")]
    runtime: Arc<Mutex<Option<fastembed::TextEmbedding>>>,
    #[cfg(not(feature = "fastembed"))]
    _marker: marker::PhantomData<()>,
}

impl FastEmbedProvider {
    /// Creates a provider from a model id string, loading the real model.
    ///
    /// Accepts canonical ids (`"BAAI/bge-small-en-v1.5"`), short aliases
    /// (`"bge-small"`, `"arctic-m"`) or the variant name (`"ArcticEmbedM"`).
    ///
    /// # Errors
    /// Returns [`EmbeddingError::UnsupportedModel`] for an unknown id, or
    /// [`EmbeddingError::ModelLoadFailed`] if the weights cannot be loaded.
    #[cfg(feature = "fastembed")]
    pub fn new(model_id: &str) -> Result<Self, EmbeddingError> {
        let model = match model_id {
            "bge-large" | "BAAI/bge-large-en-v1.5" | "BgeLarge" => FastEmbedModel::BgeLarge,
            "bge-small" | "BAAI/bge-small-en-v1.5" | "BgeSmall" => FastEmbedModel::BgeSmall,
            "arctic-m"
            | "arctic-embed-m"
            | "Snowflake/snowflake-arctic-embed-m"
            | "ArcticEmbedM" => FastEmbedModel::ArcticEmbedM,
            _ => {
                return Err(EmbeddingError::UnsupportedModel(format!(
                    "unknown FastEmbed model: {model_id}"
                )));
            }
        };
        Self::try_with_model(model)
    }

    /// Loads the real ONNX model for `model`, returning an error on failure.
    ///
    /// The model is downloaded into the `fastembed_cache_dir` on first use and
    /// read from that cache thereafter (offline).
    ///
    /// # Errors
    /// Returns [`EmbeddingError::ModelLoadFailed`] if the runtime cannot be
    /// initialised (e.g. weights missing and no network on first download).
    #[cfg(feature = "fastembed")]
    pub fn try_with_model(model: FastEmbedModel) -> Result<Self, EmbeddingError> {
        let opts = fastembed::TextInitOptions::new(model.fastembed_model())
            .with_cache_dir(fastembed_cache_dir())
            .with_show_download_progress(false);
        let runtime = fastembed::TextEmbedding::try_new(opts)
            .map_err(|e| EmbeddingError::ModelLoadFailed(e.to_string()))?;
        Ok(Self {
            model,
            runtime: Arc::new(Mutex::new(Some(runtime))),
        })
    }

    /// Creates a provider with explicit model selection (infallible).
    ///
    /// Loads the real model and panics on failure — preserved for existing
    /// call sites. Prefer [`FastEmbedProvider::try_with_model`] on hot paths.
    #[cfg(feature = "fastembed")]
    pub fn with_model(model: FastEmbedModel) -> Self {
        Self::try_with_model(model)
            .unwrap_or_else(|e| panic!("FastEmbed model {model:?} should initialize: {e}"))
    }

    /// Returns the model variant in use.
    pub fn model(&self) -> FastEmbedModel {
        self.model
    }

    /// Non-loading constructor for tests/development.
    ///
    /// Produces deterministic hash vectors at the model's declared width — no
    /// download, no ONNX runtime, fully offline. Use for fast unit tests.
    pub fn new_stub(model: FastEmbedModel) -> Self {
        Self {
            model,
            #[cfg(feature = "fastembed")]
            runtime: Arc::new(Mutex::new(None)),
            #[cfg(not(feature = "fastembed"))]
            _marker: marker::PhantomData,
        }
    }

    /// Deterministic stub vector at the declared width (offline fallback).
    fn stub_vector(&self, text: &str) -> Vec<f32> {
        let dimension = self.model.dimensions();
        let mut hasher_state: u64 = 0xcbf2_9ce4_8422_2325;
        for b in text.bytes() {
            hasher_state ^= b as u64;
            hasher_state = hasher_state.wrapping_mul(0x0100_0000_01b3);
        }
        let mut vec = vec![0.0_f32; dimension];
        for (i, v) in vec.iter_mut().enumerate() {
            *v = ((hasher_state.wrapping_add(i as u64)) & 0xFF) as f32 / 255.0_f32;
        }
        vec
    }

    /// Embeds a single text synchronously into one vector.
    ///
    /// This is the hot-path entry point: it does not touch the async runtime,
    /// so daemon dispatch handlers can call it directly. The returned vector's
    /// width is [`FastEmbedModel::dimensions`].
    ///
    /// # Errors
    /// Returns [`EmbeddingError::InferenceFailed`] if locking or inference fails.
    pub fn embed_one_sync(&self, text: &str) -> Result<Vec<f32>, EmbeddingError> {
        #[cfg(feature = "fastembed")]
        {
            let mut guard = self
                .runtime
                .lock()
                .map_err(|e| EmbeddingError::InferenceFailed(e.to_string()))?;
            if let Some(rt) = guard.as_mut() {
                let out = rt
                    .embed([text], None)
                    .map_err(|e| EmbeddingError::InferenceFailed(e.to_string()))?;
                return out.into_iter().next().ok_or_else(|| {
                    EmbeddingError::InferenceFailed("empty embedding output".into())
                });
            }
        }
        Ok(self.stub_vector(text))
    }

    /// Embeds a batch of texts synchronously, preserving input order.
    ///
    /// # Errors
    /// Returns [`EmbeddingError::InferenceFailed`] if locking or inference fails.
    pub fn embed_batch_sync(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, EmbeddingError> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        #[cfg(feature = "fastembed")]
        {
            let mut guard = self
                .runtime
                .lock()
                .map_err(|e| EmbeddingError::InferenceFailed(e.to_string()))?;
            if let Some(rt) = guard.as_mut() {
                let docs: Vec<&str> = texts.iter().map(String::as_str).collect();
                let out = rt
                    .embed(docs, None)
                    .map_err(|e| EmbeddingError::InferenceFailed(e.to_string()))?;
                return Ok(out);
            }
        }
        Ok(texts.iter().map(|t| self.stub_vector(t)).collect())
    }
}

#[async_trait]
impl EmbeddingProvider for FastEmbedProvider {
    fn id(&self) -> &'static str {
        "fastembed"
    }

    fn family(&self) -> ModelFamily {
        ModelFamily::new(
            "fastembed",
            match self.model {
                FastEmbedModel::BgeLarge => "large",
                FastEmbedModel::BgeSmall => "small",
                FastEmbedModel::ArcticEmbedM => "arctic-m",
            },
        )
    }

    fn dimensions(&self) -> usize {
        self.model.dimensions()
    }

    /// Embeds a batch of texts into vectors (delegates to the sync path).
    async fn embed(&self, texts: Vec<String>) -> Result<EmbeddingResult, EmbeddingError> {
        let vectors = self.embed_batch_sync(&texts)?;
        let token_count = texts.iter().map(|t| t.split_whitespace().count()).sum();
        Ok(EmbeddingResult::new(
            vectors,
            self.model.into(),
            Some(token_count),
        ))
    }

    /// Embeds a single query text (optimized for shorter texts).
    async fn embed_query(&self, text: String) -> Result<EmbeddingResult, EmbeddingError> {
        let vector = self.embed_one_sync(&text)?;
        let token_count = text.split_whitespace().count();
        Ok(EmbeddingResult::new(
            vec![vector],
            self.model.into(),
            Some(token_count),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fastembed_model_dimensions() {
        assert_eq!(FastEmbedModel::BgeLarge.dimensions(), 1024);
        // BAAI/bge-small-en-v1.5 is genuinely 384-dim (the prior 768 was a bug).
        assert_eq!(FastEmbedModel::BgeSmall.dimensions(), 384);
        assert_eq!(FastEmbedModel::ArcticEmbedM.dimensions(), 768);
    }

    #[test]
    fn test_fastembed_model_id() {
        assert_eq!(
            FastEmbedModel::BgeLarge.model_id(),
            "BAAI/bge-large-en-v1.5"
        );
        assert_eq!(
            FastEmbedModel::BgeSmall.model_id(),
            "BAAI/bge-small-en-v1.5"
        );
        assert_eq!(
            FastEmbedModel::ArcticEmbedM.model_id(),
            "Snowflake/snowflake-arctic-embed-m"
        );
    }

    #[test]
    fn test_cache_dir_env_override() {
        // The pinned default lives under ~/.claude/touring/models/fastembed.
        let dir = fastembed_cache_dir();
        assert!(
            dir.ends_with("fastembed"),
            "cache dir should end in 'fastembed', got {dir:?}"
        );
    }

    #[test]
    fn test_provider_creation_stub() {
        let provider = FastEmbedProvider::new_stub(FastEmbedModel::BgeLarge);
        assert_eq!(provider.id(), "fastembed");
        assert_eq!(provider.dimensions(), 1024);
    }

    #[test]
    fn stub_embed_one_sync_matches_declared_width() {
        // The stub (no runtime) yields a vector at the model's declared width,
        // fully offline — the daemon's graceful fallback contract.
        let provider = FastEmbedProvider::new_stub(FastEmbedModel::ArcticEmbedM);
        let v = provider.embed_one_sync("semantic memory lesson").unwrap();
        assert_eq!(v.len(), 768);
        // Deterministic: same text → same vector.
        let v2 = provider.embed_one_sync("semantic memory lesson").unwrap();
        assert_eq!(v, v2);
    }

    #[tokio::test]
    async fn test_embed_single_text_stub() {
        let provider = FastEmbedProvider::new_stub(FastEmbedModel::BgeSmall);
        let result = provider
            .embed(vec!["hello world".to_string()])
            .await
            .unwrap();
        assert_eq!(result.len(), 1);
        let width = result.vectors.first().map(Vec::len).unwrap_or(0);
        assert_eq!(
            result.dimension, width,
            "dimension must equal the real vector width"
        );
        assert_eq!(width, 384, "bge-small stub is 384-dim");
    }

    #[tokio::test]
    async fn test_embed_empty_batch() {
        let provider = FastEmbedProvider::new_stub(FastEmbedModel::BgeLarge);
        let result = provider.embed(vec![]).await.unwrap();
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn test_embed_query_stub() {
        let provider = FastEmbedProvider::new_stub(FastEmbedModel::ArcticEmbedM);
        let result = provider
            .embed_query("search query".to_string())
            .await
            .unwrap();
        assert_eq!(result.len(), 1);
        let width = result.vectors.first().map(Vec::len).unwrap_or(0);
        assert_eq!(
            result.dimension, width,
            "dimension must equal the real vector width"
        );
        assert_eq!(width, 768, "arctic-m is 768-dim");
    }

    #[test]
    fn test_model_conversion() {
        let model: EmbeddingModel = FastEmbedModel::ArcticEmbedM.into();
        assert_eq!(model.dimensions(), 768);
        assert_eq!(model.id(), "fastembed-arctic-m");
    }

    /// Real-model integration proof (S-04, 2026-05-29). Downloads arctic-embed-m
    /// once into the pinned cache, then proves GENUINE SEMANTICS: a related
    /// query's cosine similarity exceeds an unrelated one's — which a lexical
    /// hash (zero token overlap across paraphrases) cannot reliably satisfy.
    ///
    /// Ignored by default (network + ~440MB download). Run explicitly:
    /// `cargo test -p touring-storage --no-default-features \
    ///   --features storage-emb-fastembed arctic_m_real_model -- --ignored --nocapture`
    #[cfg(feature = "fastembed")]
    #[test]
    #[ignore = "downloads ~440MB arctic-embed-m on first run; run with --ignored"]
    fn arctic_m_real_model_embeds_768_and_is_semantic() {
        let provider = FastEmbedProvider::try_with_model(FastEmbedModel::ArcticEmbedM)
            .expect("arctic-embed-m should load (network required on first download)");

        let anchor = provider
            .embed_one_sync("rust error handling with the question mark operator")
            .expect("embed anchor");
        assert_eq!(anchor.len(), 768, "arctic-embed-m produces 768-dim vectors");

        let related = provider
            .embed_one_sync("propagating errors in Rust using the ? operator")
            .expect("embed related");
        let unrelated = provider
            .embed_one_sync("the weather in Paris is sunny this afternoon")
            .expect("embed unrelated");

        let cos = |a: &[f32], b: &[f32]| -> f32 {
            let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
            let na: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
            let nb: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
            if na == 0.0 || nb == 0.0 {
                0.0
            } else {
                dot / (na * nb)
            }
        };

        let sim_related = cos(&anchor, &related);
        let sim_unrelated = cos(&anchor, &unrelated);
        println!("S-04 semantic proof: related={sim_related:.4} unrelated={sim_unrelated:.4}");
        assert!(
            sim_related > sim_unrelated,
            "semantic recall: related ({sim_related:.4}) must beat unrelated ({sim_unrelated:.4})"
        );
    }
}
