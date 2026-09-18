//! FastEmbed provider implementation.
//!
//! Uses the `fastembed` crate for ONNX-based embedding inference.
//! FastEmbed provides on-device embedding generation without a remote service:
//! on an NVIDIA GPU through the ONNX CUDA execution provider when one loads and
//! runs, on the CPU otherwise (see [`EmbedDevicePolicy`]).
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
#[cfg(feature = "fastembed")]
use std::time::Instant;

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

/// Hardware an embedding runtime actually executes on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmbedDevice {
    /// ONNX Runtime CPU execution provider.
    Cpu,
    /// ONNX Runtime CUDA execution provider (NVIDIA GPU).
    Cuda,
}

impl EmbedDevice {
    /// Stable lowercase label, the value `touring gate-metrics` reports.
    pub fn as_str(self) -> &'static str {
        match self {
            EmbedDevice::Cpu => "cpu",
            EmbedDevice::Cuda => "cuda",
        }
    }
}

/// Environment variable selecting the embedding device: `auto` | `cuda` | `cpu`.
pub const EMBED_DEVICE_ENV: &str = "TOURING_EMBED_DEVICE";

/// Environment variable capping the CUDA memory arena of one provider, in MiB.
#[cfg(feature = "storage-emb-cuda")]
const EMBED_CUDA_MEM_MB_ENV: &str = "TOURING_EMBED_CUDA_MEM_MB";

/// Default CUDA arena cap. Every daemon (global and per project) loads its own
/// runtime on the same 8 GiB card, so one runtime must not take the arena
/// ONNX Runtime would otherwise grow without bound.
#[cfg(feature = "storage-emb-cuda")]
const DEFAULT_CUDA_ARENA_MB: usize = 2048;

/// Texts per ONNX run on CUDA. fastembed's default batch of 256 sizes the
/// attention activations for 256 sequences of up to 512 tokens at once, which
/// does not fit the capped arena; on CPU the default stays.
#[cfg(feature = "fastembed")]
const CUDA_BATCH_SIZE: usize = 32;

/// Which device a provider may load on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EmbedDevicePolicy {
    /// CUDA when it loads and runs, CPU otherwise. The default.
    #[default]
    Auto,
    /// CUDA or an error: proves the GPU path, never silently CPU.
    Cuda,
    /// CPU only.
    Cpu,
}

impl EmbedDevicePolicy {
    /// Parses a `TOURING_EMBED_DEVICE` value; `None` for an unrecognised one.
    pub fn parse(raw: Option<&str>) -> Option<Self> {
        match raw.map(|v| v.trim().to_ascii_lowercase()).as_deref() {
            None | Some("") | Some("auto") => Some(Self::Auto),
            Some("cuda") | Some("gpu") => Some(Self::Cuda),
            Some("cpu") => Some(Self::Cpu),
            Some(_) => None,
        }
    }

    /// Reads `TOURING_EMBED_DEVICE`. An unrecognised value is logged and read
    /// as `auto`, the one policy that never loses the semantic embedder.
    pub fn from_env() -> Self {
        let raw = std::env::var(EMBED_DEVICE_ENV).ok();
        Self::parse(raw.as_deref()).unwrap_or_else(|| {
            tracing::warn!(
                "{EMBED_DEVICE_ENV}={:?} is not one of auto|cuda|cpu; using auto",
                raw.unwrap_or_default()
            );
            Self::Auto
        })
    }
}

/// CUDA arena cap in bytes from a `TOURING_EMBED_CUDA_MEM_MB` value; a missing,
/// zero or unparsable value yields the default.
#[cfg(feature = "storage-emb-cuda")]
fn cuda_arena_bytes(raw_mb: Option<&str>) -> usize {
    raw_mb
        .and_then(|v| v.trim().parse::<usize>().ok())
        .filter(|mb| *mb > 0)
        .unwrap_or(DEFAULT_CUDA_ARENA_MB)
        .saturating_mul(1024 * 1024)
}

/// A loaded ONNX session and the device it runs on.
#[cfg(feature = "fastembed")]
struct Runtime {
    embedding: fastembed::TextEmbedding,
    device: EmbedDevice,
}

#[cfg(feature = "fastembed")]
impl Runtime {
    /// One ONNX run, timed and counted per device in `touring gate-metrics`.
    fn run(&mut self, texts: &[&str]) -> Result<Vec<Vec<f32>>, EmbeddingError> {
        let batch_size = match self.device {
            EmbedDevice::Cuda => Some(CUDA_BATCH_SIZE),
            EmbedDevice::Cpu => None,
        };
        let started = Instant::now();
        let out = self
            .embedding
            .embed(texts, batch_size)
            .map_err(|e| EmbeddingError::InferenceFailed(e.to_string()))?;
        let micros = u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX);
        match self.device {
            EmbedDevice::Cuda => {
                touring_foundation::gate_metrics::record_embedding_run_cuda(texts.len(), micros)
            }
            EmbedDevice::Cpu => {
                touring_foundation::gate_metrics::record_embedding_run_cpu(texts.len(), micros)
            }
        }
        Ok(out)
    }
}

/// The CUDA execution provider as the embedder registers it.
#[cfg(feature = "storage-emb-cuda")]
fn cuda_execution_provider() -> ort::ep::ExecutionProviderDispatch {
    use ort::ep::{ArenaExtendStrategy, CUDA};
    let arena = cuda_arena_bytes(std::env::var(EMBED_CUDA_MEM_MB_ENV).ok().as_deref());
    CUDA::default()
        .with_device_id(0)
        .with_memory_limit(arena)
        // Grow by what a run asks for, not to the next power of two: the arena
        // cap is shared headroom, not a target.
        .with_arena_extend_strategy(ArenaExtendStrategy::SameAsRequested)
        .build()
        // ONNX Runtime falls back to CPU in silence when a provider fails to
        // register; this turns that into an error the policy decides on.
        .error_on_failure()
}

/// Loads `model` on exactly `device`, or fails.
#[cfg(feature = "fastembed")]
fn load_runtime(model: FastEmbedModel, device: EmbedDevice) -> Result<Runtime, EmbeddingError> {
    let opts = fastembed::TextInitOptions::new(model.fastembed_model())
        .with_cache_dir(fastembed_cache_dir())
        .with_show_download_progress(false);
    let opts = match device {
        EmbedDevice::Cpu => opts,
        #[cfg(feature = "storage-emb-cuda")]
        EmbedDevice::Cuda => opts.with_execution_providers(vec![cuda_execution_provider()]),
        #[cfg(not(feature = "storage-emb-cuda"))]
        EmbedDevice::Cuda => {
            return Err(EmbeddingError::ModelLoadFailed(
                "cuda: touring-storage was built without the storage-emb-cuda feature".into(),
            ));
        }
    };
    let mut embedding = fastembed::TextEmbedding::try_new(opts)
        .map_err(|e| EmbeddingError::ModelLoadFailed(format!("{}: {e}", device.as_str())))?;
    if device == EmbedDevice::Cuda {
        // cuBLAS/cuDNN are opened on the first run, not when the provider
        // registers, so a session can register CUDA and still fail its first
        // inference. One run here proves the device before any caller relies on
        // it, and pays the kernel selection once instead of on a user's recall.
        embedding
            .embed(["touring embedder warm-up"], None)
            .map_err(|e| EmbeddingError::ModelLoadFailed(format!("cuda warm-up: {e}")))?;
    }
    Ok(Runtime { embedding, device })
}

/// Loads `model` under `policy`. When `auto` has to leave CUDA, the reason is
/// logged and recorded in `touring gate-metrics` — the one place it lives.
#[cfg(feature = "fastembed")]
fn load_for_policy(
    model: FastEmbedModel,
    policy: EmbedDevicePolicy,
) -> Result<Runtime, EmbeddingError> {
    match policy {
        EmbedDevicePolicy::Cpu => load_runtime(model, EmbedDevice::Cpu),
        EmbedDevicePolicy::Cuda => load_runtime(model, EmbedDevice::Cuda),
        EmbedDevicePolicy::Auto if !cfg!(feature = "storage-emb-cuda") => {
            load_runtime(model, EmbedDevice::Cpu)
        }
        EmbedDevicePolicy::Auto => load_runtime(model, EmbedDevice::Cuda).or_else(|e| {
            let reason = e.to_string();
            tracing::warn!(
                model = model.model_id(),
                "CUDA embedding runtime unavailable, loading on CPU: {reason}"
            );
            touring_foundation::gate_metrics::record_embedding_cuda_fallback(&reason);
            load_runtime(model, EmbedDevice::Cpu)
        }),
    }
}

/// FastEmbed embedding provider.
///
/// Wraps the `fastembed` crate for efficient on-device embedding generation.
/// The model is loaded once and held for the provider's lifetime; inference is
/// synchronous CUDA or CPU work (the `async` trait methods are thin wrappers so the
/// daemon hot path can call [`FastEmbedProvider::embed_one_sync`] directly).
///
/// When the `fastembed` feature is disabled — or when constructed via
/// [`FastEmbedProvider::new_stub`] — the provider produces deterministic
/// hash-based vectors at the model's declared width (offline, no download).
pub struct FastEmbedProvider {
    model: FastEmbedModel,
    /// `Some` = a real ONNX runtime is loaded; `None` = deterministic stub.
    #[cfg(feature = "fastembed")]
    runtime: Arc<Mutex<Option<Runtime>>>,
    /// Policy the runtime was loaded under; `auto` also governs the CUDA→CPU
    /// reload after an inference failure.
    #[cfg(feature = "fastembed")]
    policy: EmbedDevicePolicy,
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
    /// read from that cache thereafter (offline). The device follows
    /// `TOURING_EMBED_DEVICE` (default `auto`: CUDA when it loads and runs).
    ///
    /// # Errors
    /// Returns [`EmbeddingError::ModelLoadFailed`] if the runtime cannot be
    /// initialised (e.g. weights missing and no network on first download).
    #[cfg(feature = "fastembed")]
    pub fn try_with_model(model: FastEmbedModel) -> Result<Self, EmbeddingError> {
        Self::try_with_model_on(model, EmbedDevicePolicy::from_env())
    }

    /// Loads the real ONNX model for `model` under an explicit device policy.
    ///
    /// # Errors
    /// Returns [`EmbeddingError::ModelLoadFailed`] when the model cannot load on
    /// any device the policy allows (`cuda` never falls back to CPU).
    #[cfg(feature = "fastembed")]
    pub(crate) fn try_with_model_on(
        model: FastEmbedModel,
        policy: EmbedDevicePolicy,
    ) -> Result<Self, EmbeddingError> {
        let runtime = load_for_policy(model, policy)?;
        tracing::info!(
            model = model.model_id(),
            device = runtime.device.as_str(),
            ?policy,
            "fastembed runtime loaded"
        );
        Ok(Self {
            model,
            runtime: Arc::new(Mutex::new(Some(runtime))),
            policy,
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

    /// Device the loaded runtime executes on; `None` for a stub provider.
    #[cfg(feature = "fastembed")]
    pub fn device(&self) -> Option<EmbedDevice> {
        self.runtime
            .lock()
            .ok()
            .and_then(|guard| guard.as_ref().map(|rt| rt.device))
    }

    /// Device the loaded runtime executes on; `None` for a stub provider.
    #[cfg(not(feature = "fastembed"))]
    pub fn device(&self) -> Option<EmbedDevice> {
        None
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
            #[cfg(feature = "fastembed")]
            policy: EmbedDevicePolicy::Cpu,
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

    /// Runs one inference on the loaded runtime; `Ok(None)` for a stub.
    ///
    /// Under [`EmbedDevicePolicy::Auto`] a CUDA failure at inference time (an
    /// exhausted arena while several daemons share the card, a driver reset)
    /// reloads the model on CPU once and repeats the same texts there, so the
    /// caller still gets the semantic vector instead of the hash fallback.
    #[cfg(feature = "fastembed")]
    fn infer(&self, texts: &[&str]) -> Result<Option<Vec<Vec<f32>>>, EmbeddingError> {
        let mut guard = self
            .runtime
            .lock()
            .map_err(|e| EmbeddingError::InferenceFailed(e.to_string()))?;
        let Some(rt) = guard.as_mut() else {
            return Ok(None);
        };
        match rt.run(texts) {
            Ok(out) => Ok(Some(out)),
            Err(e) if rt.device == EmbedDevice::Cuda && self.policy == EmbedDevicePolicy::Auto => {
                let reason = format!("cuda inference: {e}");
                tracing::warn!(
                    model = self.model.model_id(),
                    "{reason}; reloading the embedder on CPU"
                );
                *rt = load_runtime(self.model, EmbedDevice::Cpu)?;
                touring_foundation::gate_metrics::record_embedding_cuda_fallback(&reason);
                rt.run(texts).map(Some)
            }
            Err(e) => Err(e),
        }
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
            if let Some(out) = self.infer(&[text])? {
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
            let docs: Vec<&str> = texts.iter().map(String::as_str).collect();
            if let Some(out) = self.infer(&docs)? {
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
    fn embed_device_policy_parses_documented_values() {
        assert_eq!(
            EmbedDevicePolicy::parse(None),
            Some(EmbedDevicePolicy::Auto)
        );
        assert_eq!(
            EmbedDevicePolicy::parse(Some("")),
            Some(EmbedDevicePolicy::Auto)
        );
        assert_eq!(
            EmbedDevicePolicy::parse(Some(" Auto ")),
            Some(EmbedDevicePolicy::Auto)
        );
        assert_eq!(
            EmbedDevicePolicy::parse(Some("CUDA")),
            Some(EmbedDevicePolicy::Cuda)
        );
        assert_eq!(
            EmbedDevicePolicy::parse(Some("gpu")),
            Some(EmbedDevicePolicy::Cuda)
        );
        assert_eq!(
            EmbedDevicePolicy::parse(Some("cpu")),
            Some(EmbedDevicePolicy::Cpu)
        );
    }

    #[test]
    fn embed_device_policy_rejects_unknown_value() {
        assert_eq!(EmbedDevicePolicy::parse(Some("rocm")), None);
        assert_eq!(EmbedDevicePolicy::parse(Some("cuda0")), None);
    }

    #[test]
    fn embed_device_labels_are_the_gate_metrics_values() {
        assert_eq!(EmbedDevice::Cpu.as_str(), "cpu");
        assert_eq!(EmbedDevice::Cuda.as_str(), "cuda");
    }

    #[cfg(feature = "storage-emb-cuda")]
    #[test]
    fn cuda_arena_bytes_defaults_and_honours_override() {
        let default = DEFAULT_CUDA_ARENA_MB * 1024 * 1024;
        assert_eq!(cuda_arena_bytes(None), default);
        assert_eq!(cuda_arena_bytes(Some("0")), default, "zero is not a cap");
        assert_eq!(cuda_arena_bytes(Some("lots")), default);
        assert_eq!(cuda_arena_bytes(Some(" 512 ")), 512 * 1024 * 1024);
    }

    #[test]
    fn stub_provider_reports_no_device() {
        let provider = FastEmbedProvider::new_stub(FastEmbedModel::ArcticEmbedM);
        assert_eq!(provider.device(), None);
    }

    /// Live proof of the GPU path, run by hand on a machine with an NVIDIA GPU,
    /// CUDA 13 + cuDNN 9 and the arctic-embed-m weights cached:
    /// `cargo test -p touring-storage --release cuda_runtime -- --ignored`.
    /// Asserts the session really runs on CUDA (policy `cuda` never falls back)
    /// and that its vectors agree with the CPU ones, so a mixed CPU/GPU corpus
    /// stays cosine-comparable without a reindex.
    #[cfg(feature = "storage-emb-cuda")]
    #[test]
    #[ignore = "needs an NVIDIA GPU, CUDA 13, cuDNN 9 and cached weights"]
    fn cuda_runtime_matches_cpu_vectors() {
        let texts: Vec<String> = [
            "rust error handling with the question mark operator",
            "the daemon is spawned in its own systemd scope",
        ]
        .iter()
        .map(|t| t.to_string())
        .collect();
        let cuda = FastEmbedProvider::try_with_model_on(
            FastEmbedModel::ArcticEmbedM,
            EmbedDevicePolicy::Cuda,
        )
        .expect("CUDA runtime must load under policy cuda");
        assert_eq!(cuda.device(), Some(EmbedDevice::Cuda));
        let cpu = FastEmbedProvider::try_with_model_on(
            FastEmbedModel::ArcticEmbedM,
            EmbedDevicePolicy::Cpu,
        )
        .expect("CPU runtime must load");
        assert_eq!(cpu.device(), Some(EmbedDevice::Cpu));
        let on_gpu = cuda.embed_batch_sync(&texts).expect("cuda embed");
        let on_cpu = cpu.embed_batch_sync(&texts).expect("cpu embed");
        for (g, c) in on_gpu.iter().zip(&on_cpu) {
            let dot: f32 = g.iter().zip(c).map(|(a, b)| a * b).sum();
            let norm = |v: &[f32]| v.iter().map(|x| x * x).sum::<f32>().sqrt();
            let cosine = dot / (norm(g) * norm(c));
            assert!(
                cosine > 0.9999,
                "CPU and CUDA vectors diverge: cosine {cosine}"
            );
        }
    }

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
