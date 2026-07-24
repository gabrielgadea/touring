//! Candle-backed embedder (feature-gated).
//!
//! # Phase status (Wave 1 Cognitive)
//!
//! | Phase | Scope | Status |
//! |-------|-------|--------|
//! | **1** | GGUF header parse → `dims`, `architecture`, `tensor_count` | ✅ done |
//! | **2a** | Tokenizer wiring: `load_with_tokenizer` + `tokenize_to_ids` | ✅ done |
//! | **2b** | Quantized BERT forward + mean-pool + L2 norm → `Vec<f32>` | ✅ done (2026-04-17) |
//! | **3** | Daemon-actor pool keyed by model id | ⏳ next session |
//!
//! Phase 2b lands via a hand-ported `crate::semantic::quantized_bert`
//! module — `candle-transformers` 0.8 ships quantized variants for causal
//! LLMs but not for BERT-family encoders. The new module implements the
//! full encoder (embeddings + attention + FFN) against
//! `candle_core::quantized::QMatMul`, so BGE/Nomic/all-MiniLM GGUF weights
//! converted via llama.cpp's `convert-hf-to-gguf.py --model-type bert`
//! load directly via [`CandleEmbedder::load_quantized_bert`].
//!
//! Without a staged GGUF, [`CandleEmbedder::stub`] and
//! [`CandleEmbedder::load_gguf`] still produce headers-only instances for
//! tokenizer-only flows; [`CandleEmbedder::forward_pass`] returns
//! `CandleEmbedderError::ForwardPassNotImplemented` in that state so
//! consumers can branch cleanly to [`super::MockEmbedder`].

#[cfg(feature = "semantic-embeddings")]
mod real {
    use crate::rl::semantic::Embedder;
    use crate::rl::semantic::quantized_bert::{QuantizedBertModel, l2_normalize, mean_pool};
    use candle_core::quantized::gguf_file::{Content, Value};
    use candle_core::{DType, Device, Tensor};
    use std::fs::File;
    use std::io::BufReader;
    use std::path::Path;
    use tokenizers::Tokenizer;

    /// Errors produced by [`CandleEmbedder::load_gguf`] and tokenizer ops.
    #[derive(Debug, thiserror::Error)]
    pub enum CandleEmbedderError {
        #[error("failed to open GGUF file: {0}")]
        OpenFile(#[from] std::io::Error),

        #[error("failed to parse GGUF content: {0}")]
        ParseGguf(#[source] candle_core::Error),

        #[error("GGUF metadata key '{0}' is missing — model is not a BERT-family embedder")]
        MissingMetadataKey(&'static str),

        #[error("GGUF metadata key '{0}' has unexpected type: {1}")]
        UnexpectedMetadataType(&'static str, &'static str),

        #[error("failed to load tokenizer.json: {0}")]
        LoadTokenizer(String),

        #[error("tokenizer failed to encode input text: {0}")]
        EncodeFailed(String),

        #[error("forward pass not yet available — load a quantized BERT via load_quantized_bert()")]
        ForwardPassNotImplemented,

        #[error("quantized BERT load failed: {0}")]
        LoadQuantizedBert(String),

        #[error("forward pass failed: {0}")]
        Forward(String),
    }

    /// GGUF-backed embedder.
    ///
    /// Owns a parsed [`Content`] header (metadata + tensor descriptors) loaded
    /// once at construction; the actual tensor blobs stay mmap'd via the
    /// underlying file handle that `Content::read` advances over. Forward-pass
    /// inference is a Phase-2 deliverable (see ADR Wave 1) — this loader
    /// implements ADR Phase 1: discover, validate, expose dims.
    pub struct CandleEmbedder {
        dims: usize,
        device: Device,
        /// Architecture string from the GGUF metadata (e.g. "bert", "nomic-bert").
        architecture: String,
        /// Tensor count — exposed for diagnostics + integration tests.
        tensor_count: usize,
        /// Optional HuggingFace tokenizer loaded from a `tokenizer.json`
        /// sidecar. Populated by [`CandleEmbedder::load_with_tokenizer`];
        /// [`CandleEmbedder::load_gguf`] leaves it `None` so callers that
        /// only need the header info do not pay the parse cost.
        tokenizer: Option<Tokenizer>,
        /// Optional quantized BERT encoder populated by
        /// [`CandleEmbedder::load_quantized_bert`]. When `None`,
        /// [`CandleEmbedder::forward_pass`] surfaces
        /// [`CandleEmbedderError::ForwardPassNotImplemented`].
        bert_model: Option<QuantizedBertModel>,
    }

    impl CandleEmbedder {
        /// Stub constructor — kept for the no-model test path.
        ///
        /// Equivalent to having loaded a model with `dims` hidden size but
        /// no real tensors. Forward pass still panics. Useful when
        /// downstream code needs an `Embedder` trait object before the GGUF
        /// file is staged on disk.
        #[must_use]
        pub fn stub(dims: usize) -> Self {
            Self {
                dims,
                device: Device::Cpu,
                architecture: "stub".to_string(),
                tensor_count: 0,
                tokenizer: None,
                bert_model: None,
            }
        }

        /// Load a GGUF model from disk (ADR Wave 1, Phase 1).
        ///
        /// Parses the GGUF header via `candle-core::quantized::gguf_file`
        /// and extracts:
        /// - `<arch>.embedding_length` → `self.dims` (BERT/Nomic convention)
        /// - `general.architecture` → `self.architecture`
        /// - tensor count → `self.tensor_count`
        ///
        /// Does NOT execute a forward pass. Tensor weights remain unloaded
        /// until the Phase-2 follow-up wires `BertModel::forward`.
        pub fn load_gguf(path: impl AsRef<Path>) -> Result<Self, CandleEmbedderError> {
            let file = File::open(path.as_ref())?;
            let mut reader = BufReader::new(file);
            let content = Content::read(&mut reader).map_err(CandleEmbedderError::ParseGguf)?;

            let architecture = match content.metadata.get("general.architecture") {
                Some(Value::String(s)) => s.clone(),
                Some(_) => {
                    return Err(CandleEmbedderError::UnexpectedMetadataType(
                        "general.architecture",
                        "expected String",
                    ));
                }
                None => {
                    return Err(CandleEmbedderError::MissingMetadataKey(
                        "general.architecture",
                    ));
                }
            };

            // BERT-family models name the hidden size `<arch>.embedding_length`.
            // We probe both the architecture-specific key and the legacy
            // fallback `embedding_length` for compatibility with hand-converted
            // models.
            let dims_key = format!("{architecture}.embedding_length");
            let dims = content
                .metadata
                .get(&dims_key)
                .or_else(|| content.metadata.get("embedding_length"))
                .ok_or(CandleEmbedderError::MissingMetadataKey(
                    "<arch>.embedding_length",
                ))?;
            let dims = match dims {
                Value::U32(v) => *v as usize,
                Value::U64(v) => *v as usize,
                Value::I32(v) => (*v).max(0) as usize,
                Value::I64(v) => (*v).max(0) as usize,
                _ => {
                    return Err(CandleEmbedderError::UnexpectedMetadataType(
                        "embedding_length",
                        "expected unsigned integer",
                    ));
                }
            };

            Ok(Self {
                dims,
                device: Device::Cpu,
                architecture,
                tensor_count: content.tensor_infos.len(),
                tokenizer: None,
                bert_model: None,
            })
        }

        /// Load a GGUF model AND attach its tokenizer in one call (Phase 2a).
        ///
        /// BGE / Nomic model distributions ship a `tokenizer.json`
        /// alongside the GGUF weights. This convenience constructor parses
        /// both, so downstream code can go straight from text to
        /// `input_ids` without juggling two loader calls.
        ///
        /// # Errors
        /// - `CandleEmbedderError::OpenFile` / `CandleEmbedderError::ParseGguf`
        ///   from the GGUF path (see [`Self::load_gguf`]).
        /// - `CandleEmbedderError::LoadTokenizer` if `tokenizer.json` is
        ///   missing, malformed, or references an unsupported tokenizer
        ///   component.
        pub fn load_with_tokenizer(
            gguf_path: impl AsRef<Path>,
            tokenizer_path: impl AsRef<Path>,
        ) -> Result<Self, CandleEmbedderError> {
            let mut emb = Self::load_gguf(gguf_path)?;
            let tok = Tokenizer::from_file(tokenizer_path.as_ref())
                .map_err(|e| CandleEmbedderError::LoadTokenizer(e.to_string()))?;
            emb.tokenizer = Some(tok);
            Ok(emb)
        }

        /// Attach a tokenizer to an already-loaded embedder.
        ///
        /// Useful for the stub path in tests and for hot-swapping the
        /// tokenizer without reparsing the GGUF header. Replaces any
        /// previously attached tokenizer.
        pub fn attach_tokenizer(
            &mut self,
            tokenizer_path: impl AsRef<Path>,
        ) -> Result<(), CandleEmbedderError> {
            let tok = Tokenizer::from_file(tokenizer_path.as_ref())
                .map_err(|e| CandleEmbedderError::LoadTokenizer(e.to_string()))?;
            self.tokenizer = Some(tok);
            Ok(())
        }

        /// Tokenize `text` into the `input_ids` that a BERT-family model
        /// expects on its first input tensor (Phase 2a deliverable).
        ///
        /// Returns the raw id sequence without attention mask or type ids —
        /// those are trivially derived (`mask = vec![1; ids.len()]`,
        /// `type_ids = vec![0; ids.len()]`) and will live in Phase 2b
        /// where the forward pass consumes them.
        ///
        /// # Errors
        /// - `CandleEmbedderError::LoadTokenizer` if no tokenizer has
        ///   been attached (call [`Self::load_with_tokenizer`] first).
        /// - `CandleEmbedderError::EncodeFailed` if the tokenizer
        ///   rejects the input (e.g., invalid UTF-8 after normalization).
        pub fn tokenize_to_ids(&self, text: &str) -> Result<Vec<u32>, CandleEmbedderError> {
            let tok = self.tokenizer.as_ref().ok_or_else(|| {
                CandleEmbedderError::LoadTokenizer(
                    "no tokenizer attached — call load_with_tokenizer()".to_string(),
                )
            })?;
            let enc = tok
                .encode(text, true /* add special tokens */)
                .map_err(|e| CandleEmbedderError::EncodeFailed(e.to_string()))?;
            Ok(enc.get_ids().to_vec())
        }

        /// Report whether a tokenizer has been attached.
        ///
        /// Callers that want to gracefully fall back to the mock path
        /// when the real tokenizer is missing should branch on this flag.
        #[must_use]
        pub fn has_tokenizer(&self) -> bool {
            self.tokenizer.is_some()
        }

        /// Architecture name from GGUF metadata (e.g. "bert").
        #[must_use]
        pub fn architecture(&self) -> &str {
            &self.architecture
        }

        /// Number of tensors declared in the GGUF header.
        #[must_use]
        pub fn tensor_count(&self) -> usize {
            self.tensor_count
        }

        /// Compute device — fixed to `Cpu` until Phase-2b wires CUDA/Metal.
        #[must_use]
        pub fn device(&self) -> &Device {
            &self.device
        }

        /// Load a quantized BERT encoder from GGUF + tokenizer (Phase 2b).
        ///
        /// Unlike [`Self::load_gguf`], which only parses the header, this
        /// path materialises every attention + FFN layer via
        /// `crate::semantic::quantized_bert::QuantizedBertModel::from_gguf`
        /// so [`Self::forward_pass`] can produce real embeddings.
        ///
        /// # Errors
        /// - `CandleEmbedderError::OpenFile` / `CandleEmbedderError::ParseGguf`
        ///   — unreadable or malformed GGUF.
        /// - `CandleEmbedderError::LoadQuantizedBert` — GGUF lacks required
        ///   BERT tensors or metadata keys.
        /// - `CandleEmbedderError::LoadTokenizer` — missing or invalid
        ///   `tokenizer.json`.
        pub fn load_quantized_bert(
            gguf_path: impl AsRef<Path>,
            tokenizer_path: impl AsRef<Path>,
        ) -> Result<Self, CandleEmbedderError> {
            let file = File::open(gguf_path.as_ref())?;
            let mut reader = BufReader::new(file);
            let content = Content::read(&mut reader).map_err(CandleEmbedderError::ParseGguf)?;

            let tensor_count = content.tensor_infos.len();
            let device = Device::Cpu;
            let model = QuantizedBertModel::from_gguf(content, &mut reader, &device)
                .map_err(|e| CandleEmbedderError::LoadQuantizedBert(e.to_string()))?;

            let dims = model.config().hidden_size;
            let architecture = model.config().architecture.clone();

            let tokenizer = Tokenizer::from_file(tokenizer_path.as_ref())
                .map_err(|e| CandleEmbedderError::LoadTokenizer(e.to_string()))?;

            Ok(Self {
                dims,
                device,
                architecture,
                tensor_count,
                tokenizer: Some(tokenizer),
                bert_model: Some(model),
            })
        }

        /// Run the full embedding pipeline: tokenize → encoder forward →
        /// mean-pool (attention-mask aware) → L2-normalize.
        ///
        /// Returns a `Vec<f32>` of length [`Self::dimension`].
        ///
        /// # Errors
        /// - `CandleEmbedderError::ForwardPassNotImplemented` if the
        ///   embedder was built via [`Self::stub`] or [`Self::load_gguf`]
        ///   (no encoder wired). Callers should branch to
        ///   `super::MockEmbedder` in that case.
        /// - `CandleEmbedderError::EncodeFailed` / `LoadTokenizer`
        ///   if tokenisation fails.
        /// - `CandleEmbedderError::Forward` if any tensor op fails.
        pub fn forward_pass(&self, text: &str) -> Result<Vec<f32>, CandleEmbedderError> {
            let model = self
                .bert_model
                .as_ref()
                .ok_or(CandleEmbedderError::ForwardPassNotImplemented)?;

            let ids = self.tokenize_to_ids(text)?;
            let seq_len = ids.len();
            if seq_len == 0 {
                return Err(CandleEmbedderError::Forward(
                    "empty token sequence after encode".to_string(),
                ));
            }

            let input_ids = Tensor::from_vec(ids, (1, seq_len), &self.device)
                .map_err(|e| CandleEmbedderError::Forward(e.to_string()))?;
            let token_type_ids = input_ids
                .zeros_like()
                .map_err(|e| CandleEmbedderError::Forward(e.to_string()))?;
            let attention_mask = Tensor::ones((1, seq_len), DType::F32, &self.device)
                .map_err(|e| CandleEmbedderError::Forward(e.to_string()))?;

            let hidden = model
                .forward(&input_ids, &token_type_ids, Some(&attention_mask))
                .map_err(|e| CandleEmbedderError::Forward(e.to_string()))?;

            let pooled = mean_pool(&hidden, &attention_mask)
                .map_err(|e| CandleEmbedderError::Forward(e.to_string()))?;
            let normalized =
                l2_normalize(&pooled).map_err(|e| CandleEmbedderError::Forward(e.to_string()))?;

            normalized
                .squeeze(0)
                .and_then(|t| t.to_vec1::<f32>())
                .map_err(|e| CandleEmbedderError::Forward(e.to_string()))
        }
    }

    impl Embedder for CandleEmbedder {
        fn embed(&self, text: &str) -> Vec<f32> {
            // The trait signature is `-> Vec<f32>` (no Result), so we either
            // return a real embedding or panic with an actionable message.
            // Panicking preserves Operating Principle #5 ("Falhe loud"):
            // consumers that need graceful fallback should check
            // `has_forward_pass()` first and branch on MockEmbedder.
            match self.forward_pass(text) {
                Ok(v) => v,
                Err(e) => panic!(
                    "CandleEmbedder::embed failed: {e}. \
                     Ensure load_quantized_bert() was called with a staged GGUF."
                ),
            }
        }

        fn dimension(&self) -> usize {
            self.dims
        }
    }

    impl CandleEmbedder {
        /// Whether this embedder has a loaded encoder ready for
        /// [`Self::forward_pass`]. Callers that need graceful fallback
        /// check this before [`Embedder::embed`] to avoid the panic path.
        #[must_use]
        pub fn has_forward_pass(&self) -> bool {
            self.bert_model.is_some()
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn stub_exposes_configured_dimension() {
            let e = CandleEmbedder::stub(768);
            assert_eq!(e.dimension(), 768);
            assert_eq!(e.architecture(), "stub");
            assert_eq!(e.tensor_count(), 0);
            assert!(matches!(e.device(), Device::Cpu));
        }

        #[test]
        #[should_panic(expected = "load_quantized_bert()")]
        fn stub_embed_panics_with_actionable_message() {
            // INVARIANT: a stub (no bert_model loaded) must panic when
            // `embed()` is called, and the panic message must point
            // callers at `load_quantized_bert()` for the fix.
            let e = CandleEmbedder::stub(384);
            let _ = e.embed("this must panic");
        }

        #[test]
        fn load_gguf_rejects_missing_file() {
            // BOUNDARY: missing path must surface OpenFile error, not panic.
            let result = CandleEmbedder::load_gguf("/nonexistent/path/model.gguf");
            assert!(matches!(result, Err(CandleEmbedderError::OpenFile(_))));
        }

        #[test]
        fn load_gguf_rejects_invalid_magic() {
            // BOUNDARY: file exists but content is not GGUF — must surface
            // ParseGguf error from candle-core (magic bytes check fails).
            let dir = tempfile::tempdir().expect("tempdir");
            let path = dir.path().join("not_a_model.gguf");
            std::fs::write(&path, b"not a gguf file at all").expect("write");
            let result = CandleEmbedder::load_gguf(&path);
            assert!(matches!(result, Err(CandleEmbedderError::ParseGguf(_))));
        }

        #[test]
        fn stub_reports_no_tokenizer_attached() {
            // INVARIANT: fresh stub has no tokenizer until one is attached.
            let e = CandleEmbedder::stub(384);
            assert!(!e.has_tokenizer(), "stub must not preattach tokenizer");
        }

        #[test]
        fn tokenize_without_tokenizer_surfaces_error() {
            // BOUNDARY: calling tokenize_to_ids before attach_tokenizer
            // must return a structured error (not panic). Consumers rely
            // on this to branch to MockEmbedder fallback cleanly.
            let e = CandleEmbedder::stub(384);
            let result = e.tokenize_to_ids("hello");
            assert!(matches!(result, Err(CandleEmbedderError::LoadTokenizer(_))));
        }

        #[test]
        fn attach_tokenizer_rejects_missing_file() {
            // BOUNDARY: missing tokenizer.json surfaces LoadTokenizer, not
            // OpenFile — because tokenizers::Tokenizer::from_file wraps
            // its own I/O errors into a single Error type we re-stringify.
            let mut e = CandleEmbedder::stub(384);
            let result = e.attach_tokenizer("/nonexistent/tokenizer.json");
            assert!(matches!(result, Err(CandleEmbedderError::LoadTokenizer(_))));
        }

        #[test]
        fn attach_tokenizer_rejects_malformed_json() {
            // BOUNDARY: an existing-but-corrupt tokenizer.json must fail
            // LoadTokenizer rather than silently succeed with a broken
            // tokenizer that would poison tokenize_to_ids downstream.
            let dir = tempfile::tempdir().expect("tempdir");
            let path = dir.path().join("tokenizer.json");
            std::fs::write(&path, b"{ this is not valid tokenizer JSON }").expect("write");
            let mut e = CandleEmbedder::stub(384);
            let result = e.attach_tokenizer(&path);
            assert!(matches!(result, Err(CandleEmbedderError::LoadTokenizer(_))));
        }

        #[test]
        fn tokenize_roundtrip_with_real_whitespace_tokenizer() {
            // HAPPY PATH: a minimal HF tokenizer.json that splits on
            // whitespace and emits token ids from a tiny vocab. Proves the
            // full pipeline text → Encoding → Vec<u32> works without
            // staging a 100MB BERT model.
            //
            // Vocab: [PAD]=0, [UNK]=1, [CLS]=2, [SEP]=3, hello=4, world=5.
            // Pre-tokenizer: Whitespace split.
            // Model: WordLevel (lookup only, no subword).
            // Post-processor: BERT-style [CLS] … [SEP].
            let dir = tempfile::tempdir().expect("tempdir");
            let path = dir.path().join("tokenizer.json");
            let spec = r#"{
                "version": "1.0",
                "truncation": null,
                "padding": null,
                "added_tokens": [],
                "normalizer": null,
                "pre_tokenizer": { "type": "Whitespace" },
                "post_processor": {
                    "type": "TemplateProcessing",
                    "single": [
                        { "SpecialToken": { "id": "[CLS]", "type_id": 0 } },
                        { "Sequence":     { "id": "A",     "type_id": 0 } },
                        { "SpecialToken": { "id": "[SEP]", "type_id": 0 } }
                    ],
                    "pair": [
                        { "SpecialToken": { "id": "[CLS]", "type_id": 0 } },
                        { "Sequence":     { "id": "A",     "type_id": 0 } },
                        { "SpecialToken": { "id": "[SEP]", "type_id": 0 } },
                        { "Sequence":     { "id": "B",     "type_id": 1 } },
                        { "SpecialToken": { "id": "[SEP]", "type_id": 1 } }
                    ],
                    "special_tokens": {
                        "[CLS]": { "id": "[CLS]", "ids": [2], "tokens": ["[CLS]"] },
                        "[SEP]": { "id": "[SEP]", "ids": [3], "tokens": ["[SEP]"] }
                    }
                },
                "decoder": null,
                "model": {
                    "type": "WordLevel",
                    "vocab": {
                        "[PAD]": 0,
                        "[UNK]": 1,
                        "[CLS]": 2,
                        "[SEP]": 3,
                        "hello": 4,
                        "world": 5
                    },
                    "unk_token": "[UNK]"
                }
            }"#;
            std::fs::write(&path, spec).expect("write tokenizer.json");

            let mut e = CandleEmbedder::stub(384);
            e.attach_tokenizer(&path).expect("attach ok");
            assert!(e.has_tokenizer(), "tokenizer must be attached");

            let ids = e.tokenize_to_ids("hello world").expect("encode ok");
            // Expected shape: [CLS] hello world [SEP] = [2, 4, 5, 3].
            assert_eq!(ids, vec![2, 4, 5, 3], "tokenizer ids drifted");

            // Unknown token falls back to [UNK] = 1.
            let ids_unk = e.tokenize_to_ids("hello stranger").expect("encode ok");
            assert_eq!(ids_unk, vec![2, 4, 1, 3], "unknown-token fallback drifted");
        }

        #[test]
        #[ignore = "requires HF_HUB_CACHE — set TOURING_TEST_GGUF=/path/to/bge-micro.gguf"]
        fn load_gguf_parses_real_bert_model() {
            // Integration test — only runs when an actual GGUF model is
            // present. The CI lane stages bge-micro-v2-q4_k_m.gguf into
            // a known cache dir; local devs opt in via the env var.
            let path = std::env::var("TOURING_TEST_GGUF").expect("TOURING_TEST_GGUF env var");
            let e = CandleEmbedder::load_gguf(&path).expect("load real model");
            assert!(e.dimension() > 0, "dims must be discovered from metadata");
            assert!(
                e.architecture() == "bert" || e.architecture() == "nomic-bert",
                "expected BERT-family arch, got {}",
                e.architecture()
            );
            assert!(e.tensor_count() > 0, "BERT models have many tensors");
        }
    }
}

#[cfg(feature = "semantic-embeddings")]
pub use real::CandleEmbedder;

// When the feature is OFF, expose a never-type shim so downstream `cfg(..)`
// branches don't need to duplicate imports. This compiles to nothing.
#[cfg(not(feature = "semantic-embeddings"))]
pub mod disabled {
    //! Placeholder when `semantic-embeddings` is disabled.
    //!
    //! Build with `--features semantic-embeddings` to pull candle-core and
    //! expose the real [`CandleEmbedder`] symbol.
}
