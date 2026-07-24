//! Candle BGE embedding provider.
//!
//! Local BGE (BAAI General Embedding) inference via the `candle-transformers`
//! BERT implementation. BGE models use the BERT architecture, so the encoder
//! weights load through `BertModel` from a `model.safetensors` checkpoint —
//! a pure-Rust path with no ONNX Runtime dependency.

use async_trait::async_trait;
use candle_core::{Device, Result as CandleResult, Tensor};
use candle_nn::VarBuilder;
use candle_transformers::models::bert::{BertModel, Config, DTYPE};
use serde::{Deserialize, Serialize};
use std::path::Path;
use tokenizers::Tokenizer;

use crate::embeddings::{EmbeddingError, EmbeddingProvider, EmbeddingResult, ModelFamily};

/// Supported BGE model variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BgeModelVariant {
    /// BGE Large — 1024 dimensions
    BgeLarge,
    /// BGE Small — 768 dimensions
    BgeSmall,
}

impl BgeModelVariant {
    /// Returns the embedding dimension for this variant.
    pub fn dimensions(&self) -> usize {
        match self {
            BgeModelVariant::BgeLarge => 1024,
            BgeModelVariant::BgeSmall => 768,
        }
    }

    /// Returns the provider identifier string.
    pub fn id(&self) -> &'static str {
        match self {
            BgeModelVariant::BgeLarge => "bge-large",
            BgeModelVariant::BgeSmall => "bge-small",
        }
    }
}

/// Candle BGE embedding provider.
///
/// Holds a `candle-transformers` BERT model for local, pure-Rust inference.
/// Supports BGE Large (1024 dims) and BGE Small (768 dims) variants.
pub struct CandleBgeProvider {
    /// The BERT model instance (BGE shares the BERT architecture).
    model: BertModel,
    /// The device inference runs on (CPU).
    device: Device,
    /// The model variant (Large or Small).
    variant: BgeModelVariant,
    /// Loaded tokenizer for the model.
    tokenizer: Tokenizer,
    /// Embedding width, read from the loaded model's `config.json`
    /// (`hidden_size`). This is the *real* dimension of the vectors `embed`
    /// produces — it may differ from `variant`'s nominal preset when a non-BGE
    /// BERT checkpoint (e.g. all-MiniLM, 384 dims) is loaded.
    hidden_size: usize,
}

impl CandleBgeProvider {
    /// Creates a new `CandleBgeProvider` from a model directory.
    ///
    /// The directory must contain:
    /// - `config.json` — the BERT configuration
    /// - `model.safetensors` — the encoder weights
    /// - `tokenizer.json` — the tokenizer configuration
    ///
    /// # Errors
    /// Returns `EmbeddingError::ModelLoadFailed` if the config, weights, or
    /// tokenizer cannot be read or parsed.
    pub fn new(model_path: &str, variant: BgeModelVariant) -> Result<Self, EmbeddingError> {
        let dir = Path::new(model_path);

        // BGE inference runs on CPU; CUDA would need a separate candle build.
        let device = Device::Cpu;

        // Parse the BERT configuration (config.json).
        let config_path = dir.join("config.json");
        let config_text = std::fs::read_to_string(&config_path).map_err(|e| {
            EmbeddingError::ModelLoadFailed(format!("cannot read {}: {e}", config_path.display()))
        })?;
        let config: Config = serde_json::from_str(&config_text).map_err(|e| {
            EmbeddingError::ModelLoadFailed(format!("invalid BERT config.json: {e}"))
        })?;

        // The embedding width is a property of the model itself, taken from
        // its config — not assumed from the caller-supplied variant preset.
        let hidden_size = config.hidden_size;

        // Load the safetensors weights into a BertModel.
        let model = Self::load_model(dir, &config, &device)?;

        // Load the tokenizer.
        let tokenizer = Self::load_tokenizer(dir)?;

        Ok(Self {
            model,
            device,
            variant,
            tokenizer,
            hidden_size,
        })
    }

    /// Loads the BERT encoder weights from `<dir>/model.safetensors`.
    fn load_model(
        dir: &Path,
        config: &Config,
        device: &Device,
    ) -> Result<BertModel, EmbeddingError> {
        let weights_path = dir.join("model.safetensors");
        let weights = std::fs::read(&weights_path).map_err(|e| {
            EmbeddingError::ModelLoadFailed(format!("cannot read {}: {e}", weights_path.display()))
        })?;
        let vb = VarBuilder::from_buffered_safetensors(weights, DTYPE, device)
            .map_err(|e| EmbeddingError::ModelLoadFailed(format!("safetensors load error: {e}")))?;
        BertModel::load(vb, config)
            .map_err(|e| EmbeddingError::ModelLoadFailed(format!("BERT model load error: {e}")))
    }

    /// Loads the tokenizer from `<dir>/tokenizer.json`.
    fn load_tokenizer(dir: &Path) -> Result<Tokenizer, EmbeddingError> {
        let tokenizer_path = dir.join("tokenizer.json");
        Tokenizer::from_file(&tokenizer_path)
            .map_err(|e| EmbeddingError::ModelLoadFailed(format!("tokenizer load error: {e}")))
    }

    /// Runs tokenization and BERT inference for a batch of texts.
    ///
    /// Each text is embedded in its own forward pass (batch size 1), which
    /// avoids cross-sequence padding while keeping every result exact.
    fn run_inference(&self, texts: Vec<String>) -> Result<Vec<Vec<f32>>, EmbeddingError> {
        texts.into_iter().map(|t| self.embed_single(t)).collect()
    }

    /// Embeds a single text: tokenize → BERT forward → mean-pool → vector.
    fn embed_single(&self, text: String) -> Result<Vec<f32>, EmbeddingError> {
        // Tokenize the text (special tokens included).
        let encoding = self
            .tokenizer
            .encode(text, true)
            .map_err(|e| EmbeddingError::InvalidInput(format!("tokenization failed: {e}")))?;
        let ids: Vec<u32> = encoding.get_ids().to_vec();
        let mask: Vec<u32> = encoding.get_attention_mask().to_vec();

        // input_ids / attention_mask: shape [1, seq_len].
        let input_ids = Tensor::new(ids.as_slice(), &self.device)
            .and_then(|t| t.unsqueeze(0))
            .map_err(|e| EmbeddingError::InferenceFailed(format!("input_ids tensor: {e}")))?;
        let attention_mask = Tensor::new(mask.as_slice(), &self.device)
            .and_then(|t| t.unsqueeze(0))
            .map_err(|e| EmbeddingError::InferenceFailed(format!("attention_mask tensor: {e}")))?;
        // token_type_ids: all zeros for single-sentence embedding.
        let token_type_ids = input_ids
            .zeros_like()
            .map_err(|e| EmbeddingError::InferenceFailed(format!("token_type tensor: {e}")))?;

        // Forward pass → [1, seq_len, hidden_dim].
        let hidden = self
            .model
            .forward(&input_ids, &token_type_ids, Some(&attention_mask))
            .map_err(|e| EmbeddingError::InferenceFailed(format!("forward pass failed: {e}")))?;

        // Mean-pool over the sequence dimension → [1, hidden_dim].
        let pooled = Self::mean_pool(&hidden, &attention_mask)
            .map_err(|e| EmbeddingError::InferenceFailed(format!("pooling failed: {e}")))?;

        // Flatten [1, hidden_dim] → Vec<f32>.
        pooled
            .flatten_all()
            .and_then(|t| t.to_vec1::<f32>())
            .map_err(|e| EmbeddingError::InferenceFailed(format!("vector extraction failed: {e}")))
    }

    /// Applies mean pooling over the sequence dimension.
    fn mean_pool(embeddings: &Tensor, attention_mask: &Tensor) -> CandleResult<Tensor> {
        // Expand attention mask to match hidden dimension: [batch, seq_len] -> [batch, seq_len, 1]
        let mask = attention_mask
            .unsqueeze(2)?
            .to_dtype(candle_core::DType::F32)?;

        // Masked fill: set padding tokens to 0, then sum over the sequence.
        let sum = embeddings.broadcast_mul(&mask)?.sum(1)?;

        // Count of non-padding tokens per sample, with a small epsilon to
        // avoid division by zero on an all-padding row.
        let mask_sum = (mask.squeeze(2)?.sum(1)? + 1e-9)?;

        // Mean pooling: sum / token_count.
        sum.broadcast_div(&mask_sum)
    }
}

#[async_trait]
impl EmbeddingProvider for CandleBgeProvider {
    /// Returns the provider identifier.
    fn id(&self) -> &'static str {
        "candle-bge"
    }

    /// Returns the model family.
    fn family(&self) -> ModelFamily {
        match self.variant {
            BgeModelVariant::BgeLarge => ModelFamily::new("bge", "large"),
            BgeModelVariant::BgeSmall => ModelFamily::new("bge", "small"),
        }
    }

    /// Returns the embedding dimension of the loaded model.
    ///
    /// Read from the model's `config.json` (`hidden_size`) at load time, so it
    /// is the true width of the vectors `embed` produces — correct even for a
    /// non-BGE BERT checkpoint whose width differs from the variant preset.
    fn dimensions(&self) -> usize {
        self.hidden_size
    }

    /// Embeds a batch of texts into vectors.
    async fn embed(&self, texts: Vec<String>) -> Result<EmbeddingResult, EmbeddingError> {
        if texts.is_empty() {
            return Ok(EmbeddingResult::new(
                vec![],
                crate::embeddings::EmbeddingModel::BgeLarge,
                Some(0),
            ));
        }

        let vectors = self.run_inference(texts.clone())?;
        let token_count = texts.iter().map(|t| t.split_whitespace().count()).sum();

        let model = match self.variant {
            BgeModelVariant::BgeLarge => crate::embeddings::EmbeddingModel::BgeLarge,
            BgeModelVariant::BgeSmall => crate::embeddings::EmbeddingModel::BgeSmall,
        };

        Ok(EmbeddingResult::new(vectors, model, Some(token_count)))
    }

    /// Embeds a single query text.
    async fn embed_query(&self, text: String) -> Result<EmbeddingResult, EmbeddingError> {
        self.embed(vec![text]).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bge_variant_dimensions() {
        assert_eq!(BgeModelVariant::BgeLarge.dimensions(), 1024);
        assert_eq!(BgeModelVariant::BgeSmall.dimensions(), 768);
    }

    #[test]
    fn test_bge_variant_id() {
        assert_eq!(BgeModelVariant::BgeLarge.id(), "bge-large");
        assert_eq!(BgeModelVariant::BgeSmall.id(), "bge-small");
    }

    #[test]
    fn mean_pool_averages_unmasked_tokens() {
        // 1 batch, 2 tokens, 3 hidden dims — both tokens unmasked.
        let device = Device::Cpu;
        let embeddings =
            Tensor::from_vec(vec![1.0f32, 2.0, 3.0, 3.0, 4.0, 5.0], (1, 2, 3), &device)
                .expect("embeddings tensor");
        let mask = Tensor::from_vec(vec![1u32, 1], (1, 2), &device).expect("mask tensor");

        let pooled = CandleBgeProvider::mean_pool(&embeddings, &mask).expect("mean_pool");
        let got: Vec<f32> = pooled
            .flatten_all()
            .and_then(|t| t.to_vec1::<f32>())
            .expect("flatten pooled vector");

        // mean of [1,2,3] and [3,4,5] = [2,3,4].
        for (g, w) in got.iter().zip([2.0f32, 3.0, 4.0].iter()) {
            assert!((g - w).abs() < 1e-4, "got {got:?}, want [2,3,4]");
        }
    }

    #[test]
    fn mean_pool_ignores_padding_tokens() {
        // Second token is padding (mask 0) — must not contribute to the mean.
        let device = Device::Cpu;
        let embeddings =
            Tensor::from_vec(vec![1.0f32, 2.0, 3.0, 9.0, 9.0, 9.0], (1, 2, 3), &device)
                .expect("embeddings tensor");
        let mask = Tensor::from_vec(vec![1u32, 0], (1, 2), &device).expect("mask tensor");

        let pooled = CandleBgeProvider::mean_pool(&embeddings, &mask).expect("mean_pool");
        let got: Vec<f32> = pooled
            .flatten_all()
            .and_then(|t| t.to_vec1::<f32>())
            .expect("flatten pooled vector");

        // Only the unmasked first token counts → [1,2,3].
        for (g, w) in got.iter().zip([1.0f32, 2.0, 3.0].iter()) {
            assert!((g - w).abs() < 1e-4, "got {got:?}, want [1,2,3]");
        }
    }
}
