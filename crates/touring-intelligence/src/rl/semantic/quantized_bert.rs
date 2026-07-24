//! Quantized BERT (GGUF) — port of `candle_transformers::models::bert`
//! to `candle_core::quantized::QMatMul` primitives for sentence embeddings.
//!
//! # Why this exists
//!
//! `candle-transformers` 0.8 provides quantized variants for causal LLMs
//! (llama, phi, qwen2, …) but NOT for BERT-family encoders. Edge embedding
//! deployment with Q4_K_M/Q8_0 GGUF weights therefore requires a hand-ported
//! encoder — this module.
//!
//! # GGUF tensor convention
//!
//! Follows the convention emitted by llama.cpp's
//! `convert-hf-to-gguf.py --model-type bert` script:
//!
//! | GGUF name | BERT equivalent |
//! |-----------|-----------------|
//! | `token_embd.weight` | `embeddings.word_embeddings` |
//! | `position_embd.weight` | `embeddings.position_embeddings` |
//! | `token_types.weight` | `embeddings.token_type_embeddings` |
//! | `token_embd_norm.{weight,bias}` | `embeddings.LayerNorm` |
//! | `blk.N.attn_{q,k,v,output}.{weight,bias}` | attention Q/K/V/O projections |
//! | `blk.N.attn_output_norm.{weight,bias}` | post-attention LayerNorm |
//! | `blk.N.ffn_{up,down}.{weight,bias}` | FFN projections |
//! | `blk.N.layer_output_norm.{weight,bias}` | layer LayerNorm |
//!
//! # Compatibility scope
//!
//! Target models: BGE (`bge-small/base/large-en-v1.5`), Nomic embed,
//! all-MiniLM-L6-v2. All use standard BERT architecture with absolute
//! position embeddings + GELU activation + post-LayerNorm. Nomic-specific
//! RoPE variants require a separate implementation.

#![cfg(feature = "semantic-embeddings")]

use candle_core::quantized::{QMatMul, gguf_file};
use candle_core::{D, DType, Device, Module, Result, Tensor};
use candle_nn::{Embedding, LayerNorm};
use std::io::{Read, Seek};

/// Configuration derived from GGUF metadata.
///
/// Mirrors the subset of `candle_transformers::models::bert::Config`
/// relevant for embedding inference (no classifier head fields).
#[derive(Clone, Debug)]
pub struct QBertConfig {
    /// Vocabulary size — derived from `token_embd.weight` shape.
    pub vocab_size: usize,
    /// Hidden dimension (embedding length). GGUF key: `{arch}.embedding_length`.
    pub hidden_size: usize,
    /// Number of encoder layers. GGUF key: `{arch}.block_count`.
    pub num_hidden_layers: usize,
    /// Number of attention heads. GGUF key: `{arch}.attention.head_count`.
    pub num_attention_heads: usize,
    /// FFN intermediate dimension. GGUF key: `{arch}.feed_forward_length`.
    pub intermediate_size: usize,
    /// Maximum sequence length. GGUF key: `{arch}.context_length`.
    pub max_position_embeddings: usize,
    /// Type vocabulary size (usually 2 for BERT). Derived from tensor shape.
    pub type_vocab_size: usize,
    /// LayerNorm epsilon. GGUF key: `{arch}.attention.layer_norm_epsilon`
    /// with fallback to the BERT default of 1e-12.
    pub layer_norm_eps: f64,
    /// Architecture tag from GGUF metadata (e.g. "bert", "nomic-bert").
    pub architecture: String,
}

impl QBertConfig {
    /// Derive config from GGUF metadata using BERT conventions.
    pub fn from_gguf(ct: &gguf_file::Content) -> Result<Self> {
        let architecture = get_string(ct, "general.architecture")?;
        let arch = architecture.as_str();

        let hidden_size = get_u64(ct, &format!("{arch}.embedding_length"))? as usize;
        let num_hidden_layers = get_u64(ct, &format!("{arch}.block_count"))? as usize;
        let num_attention_heads = get_u64(ct, &format!("{arch}.attention.head_count"))? as usize;
        let intermediate_size = get_u64(ct, &format!("{arch}.feed_forward_length"))? as usize;
        let max_position_embeddings = get_u64(ct, &format!("{arch}.context_length"))? as usize;
        let layer_norm_eps =
            get_f32(ct, &format!("{arch}.attention.layer_norm_epsilon")).unwrap_or(1e-12) as f64;

        // Derive vocab_size from tensor shape — more reliable than metadata.
        let vocab_size = ct
            .tensor_infos
            .get("token_embd.weight")
            .ok_or_else(|| candle_core::Error::Msg("GGUF missing token_embd.weight".to_string()))?
            .shape
            .dims()
            .first()
            .copied()
            .ok_or_else(|| {
                candle_core::Error::Msg("token_embd.weight has zero-rank shape".to_string())
            })?;

        // type_vocab_size: shape of token_types tensor, default 2 if absent.
        let type_vocab_size = ct
            .tensor_infos
            .get("token_types.weight")
            .and_then(|info| info.shape.dims().first().copied())
            .unwrap_or(2);

        Ok(Self {
            vocab_size,
            hidden_size,
            num_hidden_layers,
            num_attention_heads,
            intermediate_size,
            max_position_embeddings,
            type_vocab_size,
            layer_norm_eps,
            architecture,
        })
    }
}

// ── Metadata accessors ─────────────────────────────────────────────────────

fn get_string(ct: &gguf_file::Content, k: &str) -> Result<String> {
    match ct.metadata.get(k) {
        Some(gguf_file::Value::String(s)) => Ok(s.clone()),
        Some(_) => candle_core::bail!("GGUF key {k} has wrong type (expected String)"),
        None => candle_core::bail!("GGUF missing metadata key {k}"),
    }
}

fn get_u64(ct: &gguf_file::Content, k: &str) -> Result<u64> {
    let v = ct
        .metadata
        .get(k)
        .ok_or_else(|| candle_core::Error::Msg(format!("GGUF missing metadata key {k}")))?;
    Ok(match v {
        gguf_file::Value::U8(x) => *x as u64,
        gguf_file::Value::U16(x) => *x as u64,
        gguf_file::Value::U32(x) => *x as u64,
        gguf_file::Value::U64(x) => *x,
        gguf_file::Value::I8(x) => (*x).max(0) as u64,
        gguf_file::Value::I16(x) => (*x).max(0) as u64,
        gguf_file::Value::I32(x) => (*x).max(0) as u64,
        gguf_file::Value::I64(x) => (*x).max(0) as u64,
        _ => candle_core::bail!("GGUF key {k} is not an integer"),
    })
}

fn get_f32(ct: &gguf_file::Content, k: &str) -> Result<f32> {
    match ct.metadata.get(k) {
        Some(gguf_file::Value::F32(v)) => Ok(*v),
        Some(gguf_file::Value::F64(v)) => Ok(*v as f32),
        Some(_) => candle_core::bail!("GGUF key {k} has wrong type (expected float)"),
        None => candle_core::bail!("GGUF missing metadata key {k}"),
    }
}

// ── QBertLinear: QMatMul + optional dequantized bias ───────────────────────

/// Quantized linear layer supporting optional bias.
///
/// BERT linear layers carry bias (unlike causal LLM decoders), stored as an
/// unquantized f32/f16 tensor in the GGUF. Bias is dequantized once at load.
#[derive(Clone, Debug)]
struct QBertLinear {
    inner: QMatMul,
    bias: Option<Tensor>,
}

impl QBertLinear {
    fn new<R: Read + Seek>(
        ct: &gguf_file::Content,
        r: &mut R,
        name: &str,
        device: &Device,
    ) -> Result<Self> {
        let w = ct.tensor(r, &format!("{name}.weight"), device)?;
        let inner = QMatMul::from_qtensor(w)?;
        let bias = match ct.tensor(r, &format!("{name}.bias"), device) {
            Ok(b) => Some(b.dequantize(device)?),
            Err(_) => None,
        };
        Ok(Self { inner, bias })
    }
}

impl Module for QBertLinear {
    fn forward(&self, xs: &Tensor) -> Result<Tensor> {
        let y = self.inner.forward(xs)?;
        match &self.bias {
            Some(b) => y.broadcast_add(b),
            None => Ok(y),
        }
    }
}

// ── LayerNorm loader ───────────────────────────────────────────────────────

fn load_layer_norm<R: Read + Seek>(
    ct: &gguf_file::Content,
    r: &mut R,
    name: &str,
    eps: f64,
    device: &Device,
) -> Result<LayerNorm> {
    let w = ct
        .tensor(r, &format!("{name}.weight"), device)?
        .dequantize(device)?;
    let b = ct
        .tensor(r, &format!("{name}.bias"), device)?
        .dequantize(device)?;
    Ok(LayerNorm::new(w, b, eps))
}

// ── QBertEmbeddings ────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
struct QBertEmbeddings {
    word_embeddings: Embedding,
    position_embeddings: Embedding,
    token_type_embeddings: Embedding,
    layer_norm: LayerNorm,
}

impl QBertEmbeddings {
    fn load<R: Read + Seek>(
        ct: &gguf_file::Content,
        r: &mut R,
        config: &QBertConfig,
        device: &Device,
    ) -> Result<Self> {
        let word = ct
            .tensor(r, "token_embd.weight", device)?
            .dequantize(device)?;
        let position = ct
            .tensor(r, "position_embd.weight", device)?
            .dequantize(device)?;
        let types = ct
            .tensor(r, "token_types.weight", device)?
            .dequantize(device)?;

        let word_embeddings = Embedding::new(word, config.hidden_size);
        let position_embeddings = Embedding::new(position, config.hidden_size);
        let token_type_embeddings = Embedding::new(types, config.hidden_size);
        let layer_norm = load_layer_norm(ct, r, "token_embd_norm", config.layer_norm_eps, device)?;

        Ok(Self {
            word_embeddings,
            position_embeddings,
            token_type_embeddings,
            layer_norm,
        })
    }

    fn forward(&self, input_ids: &Tensor, token_type_ids: &Tensor) -> Result<Tensor> {
        let (_bsz, seq_len) = input_ids.dims2()?;
        let word = self.word_embeddings.forward(input_ids)?;
        let tt = self.token_type_embeddings.forward(token_type_ids)?;
        let mut out = (&word + tt)?;

        // Absolute position ids [0, 1, …, seq_len-1] broadcast over batch.
        let pos_ids: Vec<u32> = (0..seq_len as u32).collect();
        let pos_ids = Tensor::new(pos_ids.as_slice(), input_ids.device())?;
        let pos = self.position_embeddings.forward(&pos_ids)?;
        out = out.broadcast_add(&pos)?;

        self.layer_norm.forward(&out)
    }
}

// ── QBertSelfAttention (includes output projection + residual + LayerNorm) ──

#[derive(Clone, Debug)]
struct QBertSelfAttention {
    query: QBertLinear,
    key: QBertLinear,
    value: QBertLinear,
    output: QBertLinear,
    layer_norm: LayerNorm,
    num_heads: usize,
    head_size: usize,
}

impl QBertSelfAttention {
    fn load<R: Read + Seek>(
        ct: &gguf_file::Content,
        r: &mut R,
        prefix: &str,
        config: &QBertConfig,
        device: &Device,
    ) -> Result<Self> {
        let query = QBertLinear::new(ct, r, &format!("{prefix}.attn_q"), device)?;
        let key = QBertLinear::new(ct, r, &format!("{prefix}.attn_k"), device)?;
        let value = QBertLinear::new(ct, r, &format!("{prefix}.attn_v"), device)?;
        let output = QBertLinear::new(ct, r, &format!("{prefix}.attn_output"), device)?;
        let layer_norm = load_layer_norm(
            ct,
            r,
            &format!("{prefix}.attn_output_norm"),
            config.layer_norm_eps,
            device,
        )?;

        let head_size = config.hidden_size / config.num_attention_heads;
        Ok(Self {
            query,
            key,
            value,
            output,
            layer_norm,
            num_heads: config.num_attention_heads,
            head_size,
        })
    }

    fn transpose_for_scores(&self, xs: &Tensor) -> Result<Tensor> {
        let mut new_shape = xs.dims().to_vec();
        new_shape.pop();
        new_shape.push(self.num_heads);
        new_shape.push(self.head_size);
        xs.reshape(new_shape.as_slice())?
            .transpose(1, 2)?
            .contiguous()
    }

    fn forward(&self, hidden_states: &Tensor, attention_mask: &Tensor) -> Result<Tensor> {
        let q = self.query.forward(hidden_states)?;
        let k = self.key.forward(hidden_states)?;
        let v = self.value.forward(hidden_states)?;

        let q = self.transpose_for_scores(&q)?;
        let k = self.transpose_for_scores(&k)?;
        let v = self.transpose_for_scores(&v)?;

        let scores = q.matmul(&k.t()?)?;
        let scores = (scores / (self.head_size as f64).sqrt())?;
        let scores = scores.broadcast_add(attention_mask)?;
        let probs = candle_nn::ops::softmax(&scores, D::Minus1)?;

        let context = probs.matmul(&v)?;
        let context = context.transpose(1, 2)?.contiguous()?;
        let context = context.flatten_from(D::Minus2)?;

        // Output projection + residual + LayerNorm (BERT post-norm).
        let proj = self.output.forward(&context)?;
        let residual = (proj + hidden_states)?;
        self.layer_norm.forward(&residual)
    }
}

// ── QBertLayer: attention + FFN with GELU ──────────────────────────────────

#[derive(Clone, Debug)]
struct QBertLayer {
    attention: QBertSelfAttention,
    ffn_up: QBertLinear,
    ffn_down: QBertLinear,
    layer_norm: LayerNorm,
}

impl QBertLayer {
    fn load<R: Read + Seek>(
        ct: &gguf_file::Content,
        r: &mut R,
        prefix: &str,
        config: &QBertConfig,
        device: &Device,
    ) -> Result<Self> {
        let attention = QBertSelfAttention::load(ct, r, prefix, config, device)?;
        let ffn_up = QBertLinear::new(ct, r, &format!("{prefix}.ffn_up"), device)?;
        let ffn_down = QBertLinear::new(ct, r, &format!("{prefix}.ffn_down"), device)?;
        let layer_norm = load_layer_norm(
            ct,
            r,
            &format!("{prefix}.layer_output_norm"),
            config.layer_norm_eps,
            device,
        )?;
        Ok(Self {
            attention,
            ffn_up,
            ffn_down,
            layer_norm,
        })
    }

    fn forward(&self, hidden_states: &Tensor, attention_mask: &Tensor) -> Result<Tensor> {
        let attn_out = self.attention.forward(hidden_states, attention_mask)?;
        let intermediate = self.ffn_up.forward(&attn_out)?.gelu_erf()?;
        let out = self.ffn_down.forward(&intermediate)?;
        let residual = (out + &attn_out)?;
        self.layer_norm.forward(&residual)
    }
}

// ── QuantizedBertModel: full encoder ───────────────────────────────────────

/// Quantized BERT encoder for sentence embedding inference.
///
/// Load via [`Self::from_gguf`], then call [`Self::forward`] to get raw
/// hidden states. Use [`mean_pool`] + [`l2_normalize`] to get a sentence
/// embedding.
#[derive(Clone, Debug)]
pub struct QuantizedBertModel {
    embeddings: QBertEmbeddings,
    layers: Vec<QBertLayer>,
    config: QBertConfig,
    device: Device,
}

impl QuantizedBertModel {
    /// Build a [`QuantizedBertModel`] from a parsed GGUF container.
    pub fn from_gguf<R: Read + Seek>(
        ct: gguf_file::Content,
        reader: &mut R,
        device: &Device,
    ) -> Result<Self> {
        let config = QBertConfig::from_gguf(&ct)?;
        let embeddings = QBertEmbeddings::load(&ct, reader, &config, device)?;

        let layers = (0..config.num_hidden_layers)
            .map(|i| QBertLayer::load(&ct, reader, &format!("blk.{i}"), &config, device))
            .collect::<Result<Vec<_>>>()?;

        Ok(Self {
            embeddings,
            layers,
            config,
            device: device.clone(),
        })
    }

    /// Read-only view of the parsed config.
    #[must_use]
    pub fn config(&self) -> &QBertConfig {
        &self.config
    }

    /// The device weights were loaded onto.
    #[must_use]
    pub fn device(&self) -> &Device {
        &self.device
    }

    /// Run the encoder forward pass.
    ///
    /// Shapes:
    /// - `input_ids`: `[batch, seq_len]`, `u32`
    /// - `token_type_ids`: `[batch, seq_len]`, `u32` (all zeros for single-sequence tasks)
    /// - `attention_mask`: optional `[batch, seq_len]`, `1.0` for valid tokens, `0.0` for padding
    ///
    /// Returns the last hidden states of shape `[batch, seq_len, hidden_size]`.
    pub fn forward(
        &self,
        input_ids: &Tensor,
        token_type_ids: &Tensor,
        attention_mask: Option<&Tensor>,
    ) -> Result<Tensor> {
        let embedding_output = self.embeddings.forward(input_ids, token_type_ids)?;
        let attention_mask = match attention_mask {
            Some(m) => m.clone(),
            None => input_ids.ones_like()?.to_dtype(DType::F32)?,
        };
        let attention_mask = get_extended_attention_mask(&attention_mask, DType::F32)?;

        let mut hidden = embedding_output;
        for layer in &self.layers {
            hidden = layer.forward(&hidden, &attention_mask)?;
        }
        Ok(hidden)
    }
}

// ── Helpers ────────────────────────────────────────────────────────────────

/// Convert a `[batch, seq]` attention mask into the additive bias form that
/// BERT self-attention consumes (`-inf` for padded positions, `0` for valid).
fn get_extended_attention_mask(mask: &Tensor, dtype: DType) -> Result<Tensor> {
    let mask = match mask.rank() {
        3 => mask.unsqueeze(1)?,
        2 => mask.unsqueeze(1)?.unsqueeze(1)?,
        _ => candle_core::bail!("attention_mask must have rank 2 or 3"),
    };
    let mask = mask.to_dtype(dtype)?;
    (mask.ones_like()? - &mask)?
        .broadcast_mul(&Tensor::try_from(f32::MIN)?.to_device(mask.device())?)
}

/// Mean-pool hidden states along the sequence dimension, masked by attention.
///
/// Shapes:
/// - `hidden_states`: `[batch, seq_len, hidden]`
/// - `attention_mask`: `[batch, seq_len]` (`1.0` valid, `0.0` padding)
///
/// Returns `[batch, hidden]`. Padded positions contribute zero, and the
/// divisor is the sum of valid tokens per batch (clamped away from zero to
/// keep all-padding batches from producing NaN).
pub fn mean_pool(hidden_states: &Tensor, attention_mask: &Tensor) -> Result<Tensor> {
    let mask = attention_mask
        .unsqueeze(D::Minus1)?
        .to_dtype(hidden_states.dtype())?;
    let masked = hidden_states.broadcast_mul(&mask)?;
    let summed = masked.sum(1)?;
    let mask_sum = mask.sum(1)?;
    let safe = mask_sum.clamp(1e-9f64, f64::INFINITY)?;
    summed.broadcast_div(&safe)
}

/// L2-normalize each row along the last dimension.
///
/// For shape `[batch, hidden]`, divides each row by its L2 norm. The norm
/// is clamped below at `1e-12` to avoid division by zero for all-zero
/// embeddings (which cannot happen for a correctly trained model but is
/// defended against for robustness).
pub fn l2_normalize(t: &Tensor) -> Result<Tensor> {
    let sq = t.sqr()?;
    let sum_sq = sq.sum_keepdim(D::Minus1)?;
    let norm = sum_sq.sqrt()?;
    let safe = norm.clamp(1e-12f64, f64::INFINITY)?;
    t.broadcast_div(&safe)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn l2_normalize_classic_3_4() {
        // The [3, 4] vector has L2 norm 5. Normalized → [0.6, 0.8].
        let t = Tensor::new(&[[3.0f32, 4.0]], &Device::Cpu).expect("build tensor");
        let n = l2_normalize(&t).expect("normalize");
        let v: Vec<Vec<f32>> = n.to_vec2().expect("materialize");
        assert!((v[0][0] - 0.6).abs() < 1e-6, "got {}", v[0][0]);
        assert!((v[0][1] - 0.8).abs() < 1e-6, "got {}", v[0][1]);
    }

    #[test]
    fn l2_normalize_preserves_shape() {
        // [batch=2, hidden=3] → same shape
        let t = Tensor::new(&[[1.0f32, 2.0, 2.0], [2.0, 3.0, 6.0]], &Device::Cpu).expect("build");
        let n = l2_normalize(&t).expect("norm");
        assert_eq!(n.dims(), &[2, 3]);
        // Row 0: [1, 2, 2] norm = 3 → [1/3, 2/3, 2/3]
        let v: Vec<Vec<f32>> = n.to_vec2().expect("vec");
        assert!((v[0][0] - 1.0 / 3.0).abs() < 1e-6);
        // Row 1: [2, 3, 6] norm = 7 → [2/7, 3/7, 6/7]
        assert!((v[1][2] - 6.0 / 7.0).abs() < 1e-6);
    }

    #[test]
    fn l2_normalize_zero_vector_stays_finite() {
        // INVARIANT: zero input must not produce NaN (clamp guards division).
        let t = Tensor::new(&[[0.0f32, 0.0, 0.0]], &Device::Cpu).expect("build");
        let n = l2_normalize(&t).expect("norm");
        let v: Vec<Vec<f32>> = n.to_vec2().expect("vec");
        assert!(v[0].iter().all(|x| x.is_finite()));
    }

    #[test]
    fn mean_pool_full_mask_averages_all_tokens() {
        // hidden [1, 2, 2]: [[[1, 2], [3, 4]]]; mask [1, 2]: [[1, 1]]
        // expected: [[2, 3]]
        let hidden = Tensor::new(&[[[1.0f32, 2.0], [3.0, 4.0]]], &Device::Cpu).expect("hidden");
        let mask = Tensor::new(&[[1.0f32, 1.0]], &Device::Cpu).expect("mask");
        let pooled = mean_pool(&hidden, &mask).expect("pool");
        assert_eq!(pooled.dims(), &[1, 2]);
        let v: Vec<Vec<f32>> = pooled.to_vec2().expect("vec");
        assert!((v[0][0] - 2.0).abs() < 1e-6);
        assert!((v[0][1] - 3.0).abs() < 1e-6);
    }

    #[test]
    fn mean_pool_ignores_padded_positions() {
        // hidden [[[1, 2], [3, 4]]]; mask [[1, 0]] — only first token counts
        // expected: [[1, 2]]
        let hidden = Tensor::new(&[[[1.0f32, 2.0], [3.0, 4.0]]], &Device::Cpu).expect("hidden");
        let mask = Tensor::new(&[[1.0f32, 0.0]], &Device::Cpu).expect("mask");
        let pooled = mean_pool(&hidden, &mask).expect("pool");
        let v: Vec<Vec<f32>> = pooled.to_vec2().expect("vec");
        assert!((v[0][0] - 1.0).abs() < 1e-6);
        assert!((v[0][1] - 2.0).abs() < 1e-6);
    }

    #[test]
    fn mean_pool_all_padding_stays_finite() {
        // INVARIANT: all-zero mask produces finite zero (clamp guards divide).
        let hidden = Tensor::new(&[[[1.0f32, 2.0], [3.0, 4.0]]], &Device::Cpu).expect("hidden");
        let mask = Tensor::new(&[[0.0f32, 0.0]], &Device::Cpu).expect("mask");
        let pooled = mean_pool(&hidden, &mask).expect("pool");
        let v: Vec<Vec<f32>> = pooled.to_vec2().expect("vec");
        assert!(v[0].iter().all(|x| x.is_finite()));
    }

    #[test]
    fn extended_attention_mask_penalizes_padding() {
        // [1, 1] positions should yield 0 bias; [0] positions should yield -inf.
        let mask = Tensor::new(&[[1.0f32, 0.0, 1.0]], &Device::Cpu).expect("mask");
        let ext = get_extended_attention_mask(&mask, DType::F32).expect("extended");
        // Shape should be [batch=1, 1, 1, seq=3]
        assert_eq!(ext.dims(), &[1, 1, 1, 3]);
        let v: Vec<f32> = ext.flatten_all().expect("flatten").to_vec1().expect("vec");
        assert!(
            v[0].abs() < 1e-6,
            "valid token must have zero bias, got {}",
            v[0]
        );
        assert!(v[1] < -1e30, "padding token must have large negative bias");
        assert!(v[2].abs() < 1e-6);
    }
}
