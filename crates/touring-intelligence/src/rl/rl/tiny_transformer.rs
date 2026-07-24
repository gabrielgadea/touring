//! Tiny Transformer Predictor — ndarray-based next-tool prediction.
//!
//! Implements a small transformer model that predicts the next tool invocation
//! based on session context. Uses ndarray for matrix operations (CPU-only).
//!
//! # Architecture
//!
//! ```text
//! Input: [tool_t-3, tool_t-2, tool_t-1] (recent tool sequence)
//!        |
//! Embedding layer (vocab_size -> d_model=64)
//!        |
//! Positional encoding (sinusoidal, max_seq_len=16)
//!        |
//! TransformerBlock x2:
//!   - Multi-head self-attention (8 heads, d_k=8)
//!   - Residual + LayerNorm
//!   - FFN (d_model -> d_ff=256 -> d_model)
//!   - Residual + LayerNorm
//!        |
//! Mean pooling over sequence
//!        |
//! Output projection (d_model -> vocab_size)
//!        |
//! Softmax -> top-k predictions
//! ```

use std::collections::HashMap;
use std::io::{BufReader, BufWriter, Read as IoRead, Write as IoWrite};
use std::path::{Path, PathBuf};

use ndarray::{Array1, Array2, Axis};

// ── Constants ────────────────────────────────────────────────────────────────

const D_MODEL: usize = 64;
const D_FF: usize = 256;
const N_HEADS: usize = 8;
const D_K: usize = D_MODEL / N_HEADS; // 8
const MAX_SEQ_LEN: usize = 16;
const N_LAYERS: usize = 2;
const LAYER_NORM_EPS: f64 = 1e-5;

/// Default tool vocabulary — covers all standard Claude Code tools.
const TOOL_VOCAB: &[&str] = &[
    "Read",
    "Write",
    "Edit",
    "Bash",
    "Grep",
    "Glob",
    "Agent",
    "Skill",
    "WebSearch",
    "WebFetch",
    "NotebookEdit",
    "TodoRead",
    "TodoWrite",
    "TaskCreate",
    "TaskUpdate",
    "SendMessage",
    "TeamCreate",
    "LSP",
    "ToolSearch",
    "Unknown", // fallback for unknown tools
];

// ── Simple PRNG (xorshift64) ────────────────────────────────────────────────

/// Minimal PRNG for weight initialization — no external dependency needed.
struct Xorshift64 {
    state: u64,
}

impl Xorshift64 {
    fn new(seed: u64) -> Self {
        Self {
            state: if seed == 0 { 1 } else { seed },
        }
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        x
    }

    /// Uniform random f64 in [-scale, scale] (Xavier-like initialization).
    fn next_f64(&mut self, scale: f64) -> f64 {
        let u = (self.next_u64() as f64) / (u64::MAX as f64); // [0, 1)
        (u * 2.0 - 1.0) * scale
    }

    /// Generate a random Array2 with Xavier initialization.
    fn random_matrix(&mut self, rows: usize, cols: usize) -> Array2<f64> {
        let scale = (2.0 / (rows + cols) as f64).sqrt();
        Array2::from_shape_fn((rows, cols), |_| self.next_f64(scale))
    }

    /// Generate a random Array1 initialized to zero (bias).
    fn zero_bias(&self, size: usize) -> Array1<f64> {
        let _ = self; // suppress unused warning
        Array1::zeros(size)
    }
}

// ── Activation Functions ────────────────────────────────────────────────────

/// ReLU activation (element-wise).
fn relu(x: &Array2<f64>) -> Array2<f64> {
    x.mapv(|v| v.max(0.0))
}

/// Softmax over the last axis of a 2D array (row-wise).
fn softmax_2d(x: &Array2<f64>) -> Array2<f64> {
    let mut result = x.clone();
    for mut row in result.rows_mut() {
        let max = row.fold(f64::NEG_INFINITY, |a, &b| a.max(b));
        row.mapv_inplace(|v| (v - max).exp());
        let sum = row.sum();
        if sum > 0.0 {
            row.mapv_inplace(|v| v / sum);
        }
    }
    result
}

/// Softmax over a 1D array.
fn softmax_1d(x: &Array1<f64>) -> Array1<f64> {
    let max = x.fold(f64::NEG_INFINITY, |a, &b| a.max(b));
    let exp = x.mapv(|v| (v - max).exp());
    let sum = exp.sum();
    if sum > 0.0 { exp / sum } else { exp }
}

/// Layer normalization over the last axis.
fn layer_norm(x: &Array2<f64>, gamma: &Array1<f64>, beta: &Array1<f64>) -> Array2<f64> {
    let mut result = Array2::zeros(x.raw_dim());
    for (i, row) in x.rows().into_iter().enumerate() {
        let mean = row.mean().unwrap_or(0.0);
        let variance = row.mapv(|v| (v - mean).powi(2)).mean().unwrap_or(0.0);
        let std = (variance + LAYER_NORM_EPS).sqrt();
        let normed = row.mapv(|v| (v - mean) / std);
        let scaled = &normed * gamma + beta;
        result.row_mut(i).assign(&scaled);
    }
    result
}

// ── Positional Encoding ─────────────────────────────────────────────────────

/// Generate sinusoidal positional encoding matrix [max_len, d_model].
fn sinusoidal_positional_encoding(max_len: usize, d_model: usize) -> Array2<f64> {
    Array2::from_shape_fn((max_len, d_model), |(pos, i)| {
        let angle = pos as f64 / (10000.0_f64).powf((2 * (i / 2)) as f64 / d_model as f64);
        if i % 2 == 0 { angle.sin() } else { angle.cos() }
    })
}

// ── Transformer Components ──────────────────────────────────────────────────

/// Multi-head self-attention weights.
#[derive(Debug, Clone)]
struct MultiHeadAttention {
    w_q: Array2<f64>, // [d_model, d_model]
    w_k: Array2<f64>, // [d_model, d_model]
    w_v: Array2<f64>, // [d_model, d_model]
    w_o: Array2<f64>, // [d_model, d_model]
}

impl MultiHeadAttention {
    fn new(rng: &mut Xorshift64) -> Self {
        Self {
            w_q: rng.random_matrix(D_MODEL, D_MODEL),
            w_k: rng.random_matrix(D_MODEL, D_MODEL),
            w_v: rng.random_matrix(D_MODEL, D_MODEL),
            w_o: rng.random_matrix(D_MODEL, D_MODEL),
        }
    }

    /// Forward pass: multi-head self-attention.
    /// Input: [seq_len, d_model] -> Output: [seq_len, d_model]
    fn forward(&self, x: &Array2<f64>) -> Array2<f64> {
        let seq_len = x.nrows();

        // Project Q, K, V: [seq_len, d_model]
        let q = x.dot(&self.w_q);
        let k = x.dot(&self.w_k);
        let v = x.dot(&self.w_v);

        // Multi-head: split into N_HEADS heads, each with D_K dims
        let mut head_outputs = Vec::with_capacity(N_HEADS);

        for h in 0..N_HEADS {
            let start = h * D_K;
            let end = start + D_K;

            // Extract head slices: [seq_len, D_K]
            let q_h = q.slice(ndarray::s![.., start..end]).to_owned();
            let k_h = k.slice(ndarray::s![.., start..end]).to_owned();
            let v_h = v.slice(ndarray::s![.., start..end]).to_owned();

            // Attention scores: Q * K^T / sqrt(D_K) -> [seq_len, seq_len]
            let scale = (D_K as f64).sqrt();
            let scores = q_h.dot(&k_h.t()) / scale;

            // Softmax over keys dimension (row-wise)
            let attn_weights = softmax_2d(&scores);

            // Weighted sum of values: [seq_len, D_K]
            let head_out = attn_weights.dot(&v_h);
            head_outputs.push(head_out);
        }

        // Concatenate heads: [seq_len, d_model]
        let mut concat = Array2::zeros((seq_len, D_MODEL));
        for (h, head_out) in head_outputs.iter().enumerate() {
            let start = h * D_K;
            concat
                .slice_mut(ndarray::s![.., start..start + D_K])
                .assign(head_out);
        }

        // Output projection
        concat.dot(&self.w_o)
    }
}

/// Feed-forward network weights.
#[derive(Debug, Clone)]
struct FeedForward {
    w1: Array2<f64>, // [d_model, d_ff]
    b1: Array1<f64>, // [d_ff]
    w2: Array2<f64>, // [d_ff, d_model]
    b2: Array1<f64>, // [d_model]
}

impl FeedForward {
    fn new(rng: &mut Xorshift64) -> Self {
        Self {
            w1: rng.random_matrix(D_MODEL, D_FF),
            b1: rng.zero_bias(D_FF),
            w2: rng.random_matrix(D_FF, D_MODEL),
            b2: rng.zero_bias(D_MODEL),
        }
    }

    /// Forward pass: Linear -> ReLU -> Linear.
    /// Input: [seq_len, d_model] -> Output: [seq_len, d_model]
    fn forward(&self, x: &Array2<f64>) -> Array2<f64> {
        let hidden = x.dot(&self.w1) + &self.b1;
        let activated = relu(&hidden);
        activated.dot(&self.w2) + &self.b2
    }
}

/// Single transformer block: self-attention + FFN with residual connections and LayerNorm.
#[derive(Debug, Clone)]
struct TransformerBlock {
    attention: MultiHeadAttention,
    ffn: FeedForward,
    ln1_gamma: Array1<f64>,
    ln1_beta: Array1<f64>,
    ln2_gamma: Array1<f64>,
    ln2_beta: Array1<f64>,
}

impl TransformerBlock {
    fn new(rng: &mut Xorshift64) -> Self {
        Self {
            attention: MultiHeadAttention::new(rng),
            ffn: FeedForward::new(rng),
            ln1_gamma: Array1::ones(D_MODEL),
            ln1_beta: Array1::zeros(D_MODEL),
            ln2_gamma: Array1::ones(D_MODEL),
            ln2_beta: Array1::zeros(D_MODEL),
        }
    }

    /// Forward pass with pre-norm architecture.
    /// Input: [seq_len, d_model] -> Output: [seq_len, d_model]
    fn forward(&self, x: &Array2<f64>) -> Array2<f64> {
        // Self-attention with residual + LayerNorm
        let normed1 = layer_norm(x, &self.ln1_gamma, &self.ln1_beta);
        let attn_out = self.attention.forward(&normed1);
        let residual1 = x + &attn_out;

        // FFN with residual + LayerNorm
        let normed2 = layer_norm(&residual1, &self.ln2_gamma, &self.ln2_beta);
        let ffn_out = self.ffn.forward(&normed2);
        &residual1 + &ffn_out
    }
}

// ── Public Types ────────────────────────────────────────────────────────────

/// Prediction result from the transformer.
#[derive(Debug, Clone)]
pub struct ToolPrediction {
    /// Predicted tool name.
    pub tool_name: String,
    /// Confidence score (0.0 - 1.0).
    pub confidence: f64,
}

/// Session context for prediction.
#[derive(Debug, Clone)]
pub struct PredictionContext {
    /// Recent tool names (most recent last).
    pub recent_tools: Vec<String>,
    /// Current CILA level.
    pub cila_level: u8,
    /// Session ID.
    pub session_id: String,
}

/// Trait for next-tool predictors — implemented by both Markov baseline and Transformer.
pub trait ToolPredictor: Send + Sync {
    /// Predict the next tool given context.
    fn predict(&self, ctx: &PredictionContext, top_k: usize) -> Vec<ToolPrediction>;
    /// Name of the predictor backend.
    fn backend_name(&self) -> &str;
    /// Return accuracy if tracking is available.
    fn accuracy(&self) -> Option<f64> {
        None
    }
}

// ── Markov Baseline ─────────────────────────────────────────────────────────

/// Markov chain baseline predictor — uses bigram transition probabilities.
///
/// This is the baseline that the Tiny Transformer must beat at recall@5.
#[derive(Debug)]
pub struct MarkovPredictor {
    /// Transition counts: (prev_tool, next_tool) -> count.
    transitions: HashMap<String, HashMap<String, u64>>,
    /// Total predictions made.
    total_predictions: u64,
    /// Predictions that were correct.
    correct_predictions: u64,
}

impl MarkovPredictor {
    /// Create an empty Markov predictor with no learned transitions.
    pub fn new() -> Self {
        Self {
            transitions: HashMap::new(),
            total_predictions: 0,
            correct_predictions: 0,
        }
    }

    /// Record a tool transition for learning.
    pub fn record_transition(&mut self, from: &str, to: &str) {
        *self
            .transitions
            .entry(from.to_string())
            .or_default()
            .entry(to.to_string())
            .or_default() += 1;
    }

    /// Load transitions from a HashMap.
    pub fn load_transitions(&mut self, data: HashMap<String, HashMap<String, u64>>) {
        self.transitions = data;
        self.total_predictions = 0;
        self.correct_predictions = 0;
    }

    /// Get total number of unique transitions.
    pub fn transition_count(&self) -> usize {
        self.transitions.values().map(|m| m.len()).sum()
    }

    /// Record a prediction outcome for accuracy tracking.
    pub fn record_prediction(&mut self, predicted: &str, actual: &str) {
        self.total_predictions += 1;
        if predicted == actual {
            self.correct_predictions += 1;
        }
    }
}

impl Default for MarkovPredictor {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolPredictor for MarkovPredictor {
    fn predict(&self, ctx: &PredictionContext, top_k: usize) -> Vec<ToolPrediction> {
        let prev_tool = ctx.recent_tools.last().map(|s| s.as_str()).unwrap_or("");
        let Some(nexts) = self.transitions.get(prev_tool) else {
            return Vec::new();
        };

        let total: u64 = nexts.values().sum();
        if total == 0 {
            return Vec::new();
        }

        let mut predictions: Vec<ToolPrediction> = nexts
            .iter()
            .map(|(tool, count)| ToolPrediction {
                tool_name: tool.clone(),
                confidence: *count as f64 / total as f64,
            })
            .collect();

        predictions.sort_by(|a, b| {
            b.confidence
                .partial_cmp(&a.confidence)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        predictions.truncate(top_k);
        predictions
    }

    fn backend_name(&self) -> &str {
        "markov_bigram"
    }

    fn accuracy(&self) -> Option<f64> {
        if self.total_predictions == 0 {
            None
        } else {
            Some(self.correct_predictions as f64 / self.total_predictions as f64)
        }
    }
}

// ── Tiny Transformer Predictor ──────────────────────────────────────────────

/// Mini transformer predictor for next-tool prediction.
///
/// Architecture: 2-layer transformer with 8 attention heads, d_model=64.
/// Uses ndarray for all matrix operations (CPU-only, no GPU dependency).
///
/// # Usage
///
/// ```rust
/// use touring_intelligence::rl::rl::tiny_transformer::*;
///
/// let predictor = TinyTransformerPredictor::new_random(42);
/// let ctx = PredictionContext {
///     recent_tools: vec!["Read".to_string(), "Grep".to_string()],
///     cila_level: 2,
///     session_id: "session-1".to_string(),
/// };
/// let predictions = predictor.predict(&ctx, 3);
/// assert!(!predictions.is_empty());
/// ```
#[derive(Debug, Clone)]
pub struct TinyTransformerPredictor {
    /// Path to safetensors weights file (for future serialization).
    pub weights_path: PathBuf,
    /// Tool name -> vocabulary index mapping.
    pub tool_vocab: HashMap<String, usize>,
    /// Reverse mapping: index -> tool name.
    index_to_tool: Vec<String>,
    /// Whether the model is loaded and ready.
    pub loaded: bool,

    // Model weights
    /// Token embedding: [vocab_size, d_model]
    embedding: Array2<f64>,
    /// Positional encoding: [max_seq_len, d_model]
    pos_encoding: Array2<f64>,
    /// Transformer blocks
    layers: Vec<TransformerBlock>,
    /// Output projection: [d_model, vocab_size]
    output_proj: Array2<f64>,
    /// Output bias: [vocab_size]
    output_bias: Array1<f64>,
    /// Validation accuracy (computed during training, if available).
    validation_accuracy: Option<f64>,
}

impl TinyTransformerPredictor {
    /// Create a new transformer predictor with random weight initialization.
    ///
    /// Uses Xavier initialization with a deterministic PRNG seeded by `seed`.
    /// This is suitable for testing and inference with pre-trained weights.
    pub fn new_random(seed: u64) -> Self {
        let mut rng = Xorshift64::new(seed);

        // Build vocabulary
        let mut tool_vocab = HashMap::new();
        let mut index_to_tool = Vec::new();
        for (i, &tool) in TOOL_VOCAB.iter().enumerate() {
            tool_vocab.insert(tool.to_string(), i);
            index_to_tool.push(tool.to_string());
        }
        let vocab_size = tool_vocab.len();

        // Initialize weights
        let embedding = rng.random_matrix(vocab_size, D_MODEL);
        let pos_encoding = sinusoidal_positional_encoding(MAX_SEQ_LEN, D_MODEL);

        let mut layers = Vec::with_capacity(N_LAYERS);
        for _ in 0..N_LAYERS {
            layers.push(TransformerBlock::new(&mut rng));
        }

        let output_proj = rng.random_matrix(D_MODEL, vocab_size);
        let output_bias = rng.zero_bias(vocab_size);

        Self {
            weights_path: PathBuf::new(),
            tool_vocab,
            index_to_tool,
            loaded: true,
            embedding,
            pos_encoding,
            layers,
            output_proj,
            output_bias,
            validation_accuracy: None,
        }
    }

    /// Create a new (unloaded) transformer predictor pointing to a weights file.
    ///
    /// The predictor will not produce predictions until weights are loaded.
    pub fn new(weights_path: &Path) -> Self {
        let mut tool_vocab = HashMap::new();
        let mut index_to_tool = Vec::new();
        for (i, &tool) in TOOL_VOCAB.iter().enumerate() {
            tool_vocab.insert(tool.to_string(), i);
            index_to_tool.push(tool.to_string());
        }
        let vocab_size = tool_vocab.len();

        Self {
            weights_path: weights_path.to_path_buf(),
            tool_vocab,
            index_to_tool,
            loaded: false,
            embedding: Array2::zeros((vocab_size, D_MODEL)),
            pos_encoding: Array2::zeros((MAX_SEQ_LEN, D_MODEL)),
            layers: Vec::new(),
            output_proj: Array2::zeros((D_MODEL, vocab_size)),
            output_bias: Array1::zeros(vocab_size),
            validation_accuracy: None,
        }
    }

    /// Check if weights file exists (model is trained).
    pub fn weights_available(&self) -> bool {
        self.weights_path.exists()
    }

    /// Get the vocabulary size.
    pub fn vocab_size(&self) -> usize {
        self.tool_vocab.len()
    }

    /// Get the model dimensionality.
    pub fn d_model(&self) -> usize {
        D_MODEL
    }

    /// Get the number of transformer layers.
    pub fn n_layers(&self) -> usize {
        self.layers.len()
    }

    /// Get the number of attention heads.
    pub fn n_heads(&self) -> usize {
        N_HEADS
    }

    /// Map a tool name to its vocabulary index, falling back to "Unknown".
    fn tool_to_index(&self, tool: &str) -> usize {
        self.tool_vocab
            .get(tool)
            .copied()
            .unwrap_or_else(|| self.tool_vocab.get("Unknown").copied().unwrap_or(0))
    }

    /// Save all model weights to a binary file.
    ///
    /// Format: header (magic + version + dimensions) followed by flattened f64 arrays
    /// in deterministic order. File size is ~800KB for the default architecture.
    pub fn save_weights(&self, path: &Path) -> Result<(), TinyTransformerError> {
        let file = std::fs::File::create(path)
            .map_err(|e| TinyTransformerError::Io(format!("create: {e}")))?;
        let mut w = BufWriter::new(file);
        let vocab_size = self.tool_vocab.len() as u32;

        // Header
        w.write_all(b"TTPW")
            .map_err(|e| TinyTransformerError::Io(format!("write: {e}")))?;
        write_u32(&mut w, 1)?; // version
        write_u32(&mut w, vocab_size)?;
        write_u32(&mut w, D_MODEL as u32)?;
        write_u32(&mut w, N_LAYERS as u32)?;
        write_u32(&mut w, D_FF as u32)?;
        write_u32(&mut w, N_HEADS as u32)?;

        // Embedding
        write_array2(&mut w, &self.embedding)?;

        // Transformer blocks
        for layer in &self.layers {
            write_array2(&mut w, &layer.attention.w_q)?;
            write_array2(&mut w, &layer.attention.w_k)?;
            write_array2(&mut w, &layer.attention.w_v)?;
            write_array2(&mut w, &layer.attention.w_o)?;
            write_array2(&mut w, &layer.ffn.w1)?;
            write_array1(&mut w, &layer.ffn.b1)?;
            write_array2(&mut w, &layer.ffn.w2)?;
            write_array1(&mut w, &layer.ffn.b2)?;
            write_array1(&mut w, &layer.ln1_gamma)?;
            write_array1(&mut w, &layer.ln1_beta)?;
            write_array1(&mut w, &layer.ln2_gamma)?;
            write_array1(&mut w, &layer.ln2_beta)?;
        }

        // Output projection
        write_array2(&mut w, &self.output_proj)?;
        write_array1(&mut w, &self.output_bias)?;

        w.flush()
            .map_err(|e| TinyTransformerError::Io(format!("flush: {e}")))?;
        Ok(())
    }

    /// Load model weights from a binary file previously saved by `save_weights`.
    ///
    /// Returns a fully loaded predictor ready for inference.
    pub fn load_weights(path: &Path) -> Result<Self, TinyTransformerError> {
        let file = std::fs::File::open(path)
            .map_err(|e| TinyTransformerError::Io(format!("open: {e}")))?;
        let mut r = BufReader::new(file);

        // Header
        let mut magic = [0u8; 4];
        r.read_exact(&mut magic)
            .map_err(|e| TinyTransformerError::Io(format!("read magic: {e}")))?;
        if &magic != b"TTPW" {
            return Err(TinyTransformerError::InvalidMagic(magic));
        }
        let version = read_u32(&mut r)?;
        if version != 1 {
            return Err(TinyTransformerError::UnsupportedVersion(version));
        }
        let vocab_size = read_u32(&mut r)? as usize;
        let d_model = read_u32(&mut r)? as usize;
        let n_layers = read_u32(&mut r)? as usize;
        let d_ff = read_u32(&mut r)? as usize;
        let _n_heads = read_u32(&mut r)?; // stored but validated implicitly by d_model/d_k

        // Validate architecture compatibility
        if d_model != D_MODEL {
            return Err(TinyTransformerError::ArchMismatch(format!(
                "d_model mismatch: file={d_model}, expected={D_MODEL}"
            )));
        }
        if vocab_size != TOOL_VOCAB.len() {
            return Err(TinyTransformerError::ArchMismatch(format!(
                "vocab_size mismatch: file={vocab_size}, expected={}",
                TOOL_VOCAB.len()
            )));
        }

        // Build vocabulary
        let mut tool_vocab = HashMap::new();
        let mut index_to_tool = Vec::new();
        for (i, &tool) in TOOL_VOCAB.iter().enumerate() {
            tool_vocab.insert(tool.to_string(), i);
            index_to_tool.push(tool.to_string());
        }

        // Embedding
        let embedding = read_array2(&mut r, vocab_size, d_model)?;

        // Positional encoding (deterministic, regenerated)
        let pos_encoding = sinusoidal_positional_encoding(MAX_SEQ_LEN, d_model);

        // Transformer blocks
        let mut layers = Vec::with_capacity(n_layers);
        for _ in 0..n_layers {
            let w_q = read_array2(&mut r, d_model, d_model)?;
            let w_k = read_array2(&mut r, d_model, d_model)?;
            let w_v = read_array2(&mut r, d_model, d_model)?;
            let w_o = read_array2(&mut r, d_model, d_model)?;
            let w1 = read_array2(&mut r, d_model, d_ff)?;
            let b1 = read_array1(&mut r, d_ff)?;
            let w2 = read_array2(&mut r, d_ff, d_model)?;
            let b2 = read_array1(&mut r, d_model)?;
            let ln1_gamma = read_array1(&mut r, d_model)?;
            let ln1_beta = read_array1(&mut r, d_model)?;
            let ln2_gamma = read_array1(&mut r, d_model)?;
            let ln2_beta = read_array1(&mut r, d_model)?;

            layers.push(TransformerBlock {
                attention: MultiHeadAttention { w_q, w_k, w_v, w_o },
                ffn: FeedForward { w1, b1, w2, b2 },
                ln1_gamma,
                ln1_beta,
                ln2_gamma,
                ln2_beta,
            });
        }

        // Output projection
        let output_proj = read_array2(&mut r, d_model, vocab_size)?;
        let output_bias = read_array1(&mut r, vocab_size)?;

        Ok(Self {
            weights_path: path.to_path_buf(),
            tool_vocab,
            index_to_tool,
            loaded: true,
            embedding,
            pos_encoding,
            layers,
            output_proj,
            output_bias,
            validation_accuracy: None,
        })
    }

    /// Run the forward pass on a sequence of tool indices.
    ///
    /// Returns softmax probability distribution over the vocabulary.
    fn forward(&self, tool_indices: &[usize]) -> Array1<f64> {
        let seq_len = tool_indices.len().min(MAX_SEQ_LEN);
        if seq_len == 0 {
            // No input: return uniform distribution
            let vocab_size = self.tool_vocab.len();
            return Array1::from_elem(vocab_size, 1.0 / vocab_size as f64);
        }

        // Embedding lookup + positional encoding: [seq_len, d_model]
        let mut x = Array2::zeros((seq_len, D_MODEL));
        for (pos, &idx) in tool_indices.iter().take(seq_len).enumerate() {
            let emb = self.embedding.row(idx);
            let pe = self.pos_encoding.row(pos);
            let combined = &emb + &pe;
            x.row_mut(pos).assign(&combined);
        }

        // Pass through transformer blocks
        for layer in &self.layers {
            x = layer.forward(&x);
        }

        // Mean pooling over sequence: [d_model]
        let pooled = x.mean_axis(Axis(0)).expect("non-empty sequence");

        // Output projection: [vocab_size]
        let logits = pooled.dot(&self.output_proj) + &self.output_bias;

        // Softmax
        softmax_1d(&logits)
    }
}

impl ToolPredictor for TinyTransformerPredictor {
    fn predict(&self, ctx: &PredictionContext, top_k: usize) -> Vec<ToolPrediction> {
        if !self.loaded {
            return Vec::new();
        }

        // Convert tool names to indices
        let indices: Vec<usize> = ctx
            .recent_tools
            .iter()
            .map(|t| self.tool_to_index(t))
            .collect();

        // Forward pass
        let probs = self.forward(&indices);

        // Collect all predictions
        let mut predictions: Vec<ToolPrediction> = self
            .index_to_tool
            .iter()
            .enumerate()
            .map(|(i, name)| ToolPrediction {
                tool_name: name.clone(),
                confidence: probs.get(i).copied().unwrap_or(0.0),
            })
            .collect();

        // Sort by confidence descending
        predictions.sort_by(|a, b| {
            b.confidence
                .partial_cmp(&a.confidence)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Truncate to top_k
        predictions.truncate(top_k);
        predictions
    }

    fn backend_name(&self) -> &str {
        "tiny_transformer"
    }

    fn accuracy(&self) -> Option<f64> {
        self.validation_accuracy
    }
}

/// Select the best available predictor.
///
/// Loads the transformer if a valid weights file exists at `weights_path`,
/// otherwise falls back to the Markov baseline.
pub fn select_predictor(weights_path: &Path, markov: MarkovPredictor) -> Box<dyn ToolPredictor> {
    if weights_path.exists() {
        match TinyTransformerPredictor::load_weights(weights_path) {
            Ok(transformer) => {
                tracing::info!(path = %weights_path.display(), "transformer weights loaded");
                return Box::new(transformer);
            }
            Err(e) => {
                tracing::warn!(error = %e, "failed to load transformer weights, falling back to Markov");
            }
        }
    }
    Box::new(markov)
}

// ── Binary I/O helpers ─────────────────────────────────────────────────────

fn write_u32<W: IoWrite>(w: &mut W, v: u32) -> Result<(), TinyTransformerError> {
    w.write_all(&v.to_le_bytes())
        .map_err(|e| TinyTransformerError::Io(format!("write u32: {e}")))
}

fn read_u32<R: IoRead>(r: &mut R) -> Result<u32, TinyTransformerError> {
    let mut buf = [0u8; 4];
    r.read_exact(&mut buf)
        .map_err(|e| TinyTransformerError::Io(format!("read u32: {e}")))?;
    Ok(u32::from_le_bytes(buf))
}

fn write_array2<W: IoWrite>(w: &mut W, arr: &Array2<f64>) -> Result<(), TinyTransformerError> {
    for &val in arr.iter() {
        w.write_all(&val.to_le_bytes())
            .map_err(|e| TinyTransformerError::Io(format!("write f64: {e}")))?;
    }
    Ok(())
}

fn write_array1<W: IoWrite>(w: &mut W, arr: &Array1<f64>) -> Result<(), TinyTransformerError> {
    for &val in arr.iter() {
        w.write_all(&val.to_le_bytes())
            .map_err(|e| TinyTransformerError::Io(format!("write f64: {e}")))?;
    }
    Ok(())
}

fn read_array2<R: IoRead>(
    r: &mut R,
    rows: usize,
    cols: usize,
) -> Result<Array2<f64>, TinyTransformerError> {
    let n = rows * cols;
    let mut data = vec![0.0f64; n];
    let mut buf = [0u8; 8];
    for val in &mut data {
        r.read_exact(&mut buf)
            .map_err(|e| TinyTransformerError::Io(format!("read f64: {e}")))?;
        *val = f64::from_le_bytes(buf);
    }
    Array2::from_shape_vec((rows, cols), data)
        .map_err(|e| TinyTransformerError::Io(format!("reshape: {e}")))
}

fn read_array1<R: IoRead>(r: &mut R, size: usize) -> Result<Array1<f64>, TinyTransformerError> {
    let mut data = vec![0.0f64; size];
    let mut buf = [0u8; 8];
    for val in &mut data {
        r.read_exact(&mut buf)
            .map_err(|e| TinyTransformerError::Io(format!("read f64: {e}")))?;
        *val = f64::from_le_bytes(buf);
    }
    Ok(Array1::from_vec(data))
}

/// Errors returned by [`TinyTransformerPredictor`] weight persistence
/// (`save_weights` / `load_weights`) and the binary I/O helpers.
///
/// Display strings are preserved verbatim from the prior `String` error
/// contract, so existing log output and substring assertions are unaffected.
#[derive(Debug, thiserror::Error)]
pub enum TinyTransformerError {
    /// Underlying binary I/O failure during read, write, flush, or reshape.
    #[error("{0}")]
    Io(String),
    /// File header magic bytes did not match the expected `TTPW` tag.
    #[error("invalid magic: {0:?}")]
    InvalidMagic([u8; 4]),
    /// Serialized format version is not supported by this build.
    #[error("unsupported version: {0}")]
    UnsupportedVersion(u32),
    /// An architecture dimension in the file disagrees with the compiled model.
    #[error("{0}")]
    ArchMismatch(String),
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    #![allow(clippy::indexing_slicing)] // test vecs asserted non-empty before indexing
    use super::*;
    use std::collections::HashSet;

    fn make_ctx(tools: &[&str]) -> PredictionContext {
        PredictionContext {
            recent_tools: tools.iter().map(|s| s.to_string()).collect(),
            cila_level: 2,
            session_id: "test-session".to_string(),
        }
    }

    // ── TDD Tests (from spec) ───────────────────────────────────────────

    #[test]
    fn test_transformer_predicts_nonempty() {
        let pred = TinyTransformerPredictor::new_random(42);
        let ctx = make_ctx(&["Read", "Grep"]);
        let preds = pred.predict(&ctx, 3);
        assert!(!preds.is_empty(), "Predictions should not be empty");
        assert!(
            preds[0].confidence > 0.0,
            "Top prediction should have positive confidence"
        );
    }

    #[test]
    fn test_transformer_top3_distinct() {
        let pred = TinyTransformerPredictor::new_random(42);
        let ctx = make_ctx(&["Read", "Grep", "Edit"]);
        let preds = pred.predict(&ctx, 3);
        let names: HashSet<&String> = preds.iter().map(|p| &p.tool_name).collect();
        assert_eq!(
            names.len(),
            preds.len(),
            "All top-3 predictions should be distinct tools"
        );
    }

    #[test]
    fn test_transformer_confidence_sums_to_one() {
        let pred = TinyTransformerPredictor::new_random(42);
        let ctx = make_ctx(&["Read", "Grep"]);
        // Request all predictions (vocab_size)
        let preds = pred.predict(&ctx, pred.vocab_size());
        let sum: f64 = preds.iter().map(|p| p.confidence).sum();
        assert!(
            (sum - 1.0).abs() < 0.01,
            "Confidence scores should sum to ~1.0, got {sum}"
        );
    }

    // ── Architecture Tests ──────────────────────────────────────────────

    #[test]
    fn test_transformer_architecture_params() {
        let pred = TinyTransformerPredictor::new_random(42);
        assert_eq!(pred.d_model(), 64);
        assert_eq!(pred.n_layers(), 2);
        assert_eq!(pred.n_heads(), 8);
        assert_eq!(pred.vocab_size(), TOOL_VOCAB.len());
    }

    #[test]
    fn test_markov_accuracy_empty() {
        // Empty predictor should return None accuracy
        let pred = MarkovPredictor::new();
        assert_eq!(pred.accuracy(), None);
    }

    #[test]
    fn test_markov_accuracy_partial() {
        // Predictor with some predictions but not complete
        let mut pred = MarkovPredictor::new();
        pred.record_prediction("Read", "Read");
        pred.record_prediction("Edit", "Read");
        pred.record_prediction("Bash", "Bash");
        // 2 correct out of 3 = 66.66%
        let acc = pred.accuracy().unwrap();
        assert!((acc - 2.0 / 3.0).abs() < 0.01, "Expected ~0.667, got {acc}");
    }

    #[test]
    fn test_markov_record_prediction() {
        let mut pred = MarkovPredictor::new();
        assert_eq!(pred.accuracy(), None);
        pred.record_prediction("A", "A"); // correct
        assert_eq!(pred.accuracy(), Some(1.0));
        pred.record_prediction("A", "B"); // wrong
        assert_eq!(pred.accuracy(), Some(0.5));
        pred.record_prediction("A", "A"); // correct
        assert_eq!(pred.accuracy(), Some(2.0 / 3.0));
    }

    #[test]
    fn test_transformer_different_seeds_different_outputs() {
        let pred1 = TinyTransformerPredictor::new_random(42);
        let pred2 = TinyTransformerPredictor::new_random(123);
        let ctx = make_ctx(&["Read"]);
        let preds1 = pred1.predict(&ctx, 3);
        let preds2 = pred2.predict(&ctx, 3);
        // Different seeds should produce different confidence distributions
        let diff: f64 = preds1
            .iter()
            .zip(preds2.iter())
            .map(|(a, b)| (a.confidence - b.confidence).abs())
            .sum();
        assert!(diff > 0.0, "Different seeds should yield different outputs");
    }

    #[test]
    fn test_transformer_deterministic_same_seed() {
        let pred1 = TinyTransformerPredictor::new_random(42);
        let pred2 = TinyTransformerPredictor::new_random(42);
        let ctx = make_ctx(&["Read", "Edit"]);
        let preds1 = pred1.predict(&ctx, 5);
        let preds2 = pred2.predict(&ctx, 5);
        for (a, b) in preds1.iter().zip(preds2.iter()) {
            assert_eq!(a.tool_name, b.tool_name);
            assert!(
                (a.confidence - b.confidence).abs() < 1e-10,
                "Same seed should produce identical results"
            );
        }
    }

    #[test]
    fn test_transformer_unknown_tool_handled() {
        let pred = TinyTransformerPredictor::new_random(42);
        let ctx = make_ctx(&["NonExistentTool", "AlsoFake"]);
        let preds = pred.predict(&ctx, 3);
        assert!(!preds.is_empty(), "Should handle unknown tools gracefully");
    }

    #[test]
    fn test_transformer_empty_context() {
        let pred = TinyTransformerPredictor::new_random(42);
        let ctx = make_ctx(&[]);
        let preds = pred.predict(&ctx, 3);
        assert!(
            !preds.is_empty(),
            "Should handle empty context (uniform distribution)"
        );
    }

    #[test]
    fn test_transformer_single_tool_context() {
        let pred = TinyTransformerPredictor::new_random(42);
        let ctx = make_ctx(&["Bash"]);
        let preds = pred.predict(&ctx, 5);
        assert_eq!(preds.len(), 5);
        // All confidences should be positive
        for p in &preds {
            assert!(p.confidence > 0.0, "All confidences should be positive");
        }
    }

    #[test]
    fn test_transformer_long_sequence() {
        let pred = TinyTransformerPredictor::new_random(42);
        // Longer than MAX_SEQ_LEN — should be truncated gracefully
        let tools: Vec<&str> = (0..20).map(|i| TOOL_VOCAB[i % TOOL_VOCAB.len()]).collect();
        let ctx = make_ctx(&tools);
        let preds = pred.predict(&ctx, 3);
        assert!(!preds.is_empty());
    }

    #[test]
    fn test_transformer_top_k_respects_limit() {
        let pred = TinyTransformerPredictor::new_random(42);
        let ctx = make_ctx(&["Read"]);
        for k in [1, 3, 5, 10] {
            let preds = pred.predict(&ctx, k);
            assert!(
                preds.len() <= k,
                "top_k={k} should return at most {k} predictions, got {}",
                preds.len()
            );
        }
    }

    #[test]
    fn test_transformer_confidences_sorted_descending() {
        let pred = TinyTransformerPredictor::new_random(42);
        let ctx = make_ctx(&["Read", "Grep"]);
        let preds = pred.predict(&ctx, 10);
        for window in preds.windows(2) {
            assert!(
                window[0].confidence >= window[1].confidence,
                "Predictions should be sorted by confidence descending"
            );
        }
    }

    #[test]
    fn test_transformer_all_confidences_valid() {
        let pred = TinyTransformerPredictor::new_random(42);
        let ctx = make_ctx(&["Read", "Edit", "Bash"]);
        let preds = pred.predict(&ctx, pred.vocab_size());
        for p in &preds {
            assert!(
                (0.0..=1.0).contains(&p.confidence),
                "Confidence {} for {} should be in [0, 1]",
                p.confidence,
                p.tool_name
            );
        }
    }

    // ── Weight Save/Load Tests (P7.1 completion) ──────────────────────

    #[test]
    fn test_save_load_roundtrip_produces_identical_predictions() {
        let original = TinyTransformerPredictor::new_random(42);
        let ctx = make_ctx(&["Read", "Grep", "Edit"]);
        let original_preds = original.predict(&ctx, 5);

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("weights.ttpw");

        // Save
        original.save_weights(&path).unwrap();
        assert!(path.exists(), "Weight file should be created");

        // Load
        let loaded = TinyTransformerPredictor::load_weights(&path).unwrap();
        assert!(loaded.loaded, "Loaded model should be marked as loaded");

        // Predictions must be bit-identical
        let loaded_preds = loaded.predict(&ctx, 5);
        assert_eq!(original_preds.len(), loaded_preds.len());
        for (orig, load) in original_preds.iter().zip(loaded_preds.iter()) {
            assert_eq!(orig.tool_name, load.tool_name);
            assert!(
                (orig.confidence - load.confidence).abs() < 1e-15,
                "Confidence mismatch: {} vs {} for {}",
                orig.confidence,
                load.confidence,
                orig.tool_name,
            );
        }
    }

    #[test]
    fn test_save_load_preserves_architecture() {
        let original = TinyTransformerPredictor::new_random(99);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("weights.ttpw");

        original.save_weights(&path).unwrap();
        let loaded = TinyTransformerPredictor::load_weights(&path).unwrap();

        assert_eq!(loaded.d_model(), original.d_model());
        assert_eq!(loaded.n_layers(), original.n_layers());
        assert_eq!(loaded.n_heads(), original.n_heads());
        assert_eq!(loaded.vocab_size(), original.vocab_size());
    }

    #[test]
    fn test_load_invalid_magic_fails() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad.ttpw");
        std::fs::write(&path, b"NOPE1234567890123456").unwrap();
        let err = TinyTransformerPredictor::load_weights(&path).unwrap_err();
        assert!(err.to_string().contains("invalid magic"), "Error: {err}");
    }

    #[test]
    fn test_load_nonexistent_fails() {
        let err = TinyTransformerPredictor::load_weights(Path::new("/tmp/no_such_file.ttpw"))
            .unwrap_err();
        assert!(err.to_string().contains("open"), "Error: {err}");
    }

    #[test]
    fn test_load_truncated_file_fails() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("truncated.ttpw");
        // Write valid header but no data
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(b"TTPW").unwrap();
        f.write_all(&1u32.to_le_bytes()).unwrap(); // version
        drop(f);
        let err = TinyTransformerPredictor::load_weights(&path).unwrap_err();
        assert!(!err.to_string().is_empty(), "Should fail on truncated file");
    }

    #[test]
    fn test_select_predictor_returns_transformer_when_weights_exist() {
        let model = TinyTransformerPredictor::new_random(42);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("weights.ttpw");
        model.save_weights(&path).unwrap();

        let markov = MarkovPredictor::new();
        let predictor = select_predictor(&path, markov);
        assert_eq!(
            predictor.backend_name(),
            "tiny_transformer",
            "Should return transformer when valid weights exist"
        );
    }

    #[test]
    fn test_select_predictor_returns_markov_when_no_weights() {
        let markov = MarkovPredictor::new();
        let predictor = select_predictor(Path::new("/nonexistent/weights.ttpw"), markov);
        assert_eq!(
            predictor.backend_name(),
            "markov_bigram",
            "Should fall back to Markov when no weights"
        );
    }

    #[test]
    fn test_select_predictor_returns_markov_on_corrupt_weights() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("corrupt.ttpw");
        std::fs::write(&path, b"garbage data not a valid weights file").unwrap();

        let markov = MarkovPredictor::new();
        let predictor = select_predictor(&path, markov);
        assert_eq!(
            predictor.backend_name(),
            "markov_bigram",
            "Should fall back to Markov on corrupt weights"
        );
    }

    #[test]
    fn test_save_load_different_seeds_produce_different_weights() {
        let dir = tempfile::tempdir().unwrap();
        let path1 = dir.path().join("seed42.ttpw");
        let path2 = dir.path().join("seed99.ttpw");

        TinyTransformerPredictor::new_random(42)
            .save_weights(&path1)
            .unwrap();
        TinyTransformerPredictor::new_random(99)
            .save_weights(&path2)
            .unwrap();

        let loaded1 = TinyTransformerPredictor::load_weights(&path1).unwrap();
        let loaded2 = TinyTransformerPredictor::load_weights(&path2).unwrap();

        let ctx = make_ctx(&["Read"]);
        let preds1 = loaded1.predict(&ctx, 3);
        let preds2 = loaded2.predict(&ctx, 3);
        let diff: f64 = preds1
            .iter()
            .zip(preds2.iter())
            .map(|(a, b)| (a.confidence - b.confidence).abs())
            .sum();
        assert!(
            diff > 0.0,
            "Different seeds should produce different loaded models"
        );
    }

    #[test]
    fn test_weight_file_size_is_reasonable() {
        let model = TinyTransformerPredictor::new_random(42);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("weights.ttpw");
        model.save_weights(&path).unwrap();

        let size = std::fs::metadata(&path).unwrap().len();
        // Header: 24 bytes
        // Embedding: 20*64*8 = 10,240
        // 2 layers: 2 * (4*64*64*8 + 64*256*8 + 256*8 + 256*64*8 + 64*8 + 4*64*8) ≈ 794,624
        // Output: 64*20*8 + 20*8 = 10,400
        // Total ≈ 815,288
        assert!(
            size > 500_000 && size < 1_500_000,
            "Weight file should be ~800KB, got {size} bytes"
        );
    }

    // ── Unloaded / stub behavior ────────────────────────────────────────

    #[test]
    fn test_transformer_stub() {
        let predictor =
            TinyTransformerPredictor::new(Path::new("/nonexistent/weights.safetensors"));
        assert!(!predictor.loaded);
        assert!(!predictor.weights_available());
        assert_eq!(predictor.backend_name(), "tiny_transformer");
    }

    #[test]
    fn test_transformer_predict_unloaded() {
        let predictor = TinyTransformerPredictor::new(Path::new("/tmp/test_weights.safetensors"));
        let ctx = make_ctx(&["Read"]);
        // Unloaded predictor returns empty
        assert!(predictor.predict(&ctx, 5).is_empty());
    }

    // ── Markov Baseline Tests (preserved from original) ─────────────────

    #[test]
    fn test_markov_prediction() {
        let mut markov = MarkovPredictor::new();
        markov.record_transition("Read", "Edit");
        markov.record_transition("Read", "Edit");
        markov.record_transition("Read", "Bash");
        markov.record_transition("Edit", "Read");

        let ctx = make_ctx(&["Read"]);
        let predictions = markov.predict(&ctx, 3);
        assert!(!predictions.is_empty());
        // Edit should be most likely (2/3 probability)
        assert_eq!(predictions[0].tool_name, "Edit");
        assert!((predictions[0].confidence - 2.0 / 3.0).abs() < 0.01);
    }

    #[test]
    fn test_markov_empty_context() {
        let markov = MarkovPredictor::new();
        let ctx = make_ctx(&[]);
        let predictions = markov.predict(&ctx, 3);
        assert!(predictions.is_empty());
    }

    #[test]
    fn test_markov_backend_name() {
        let markov = MarkovPredictor::new();
        assert_eq!(markov.backend_name(), "markov_bigram");
    }

    #[test]
    fn test_select_predictor_fallback() {
        let markov = MarkovPredictor::new();
        let predictor = select_predictor(Path::new("/nonexistent"), markov);
        assert_eq!(predictor.backend_name(), "markov_bigram");
    }

    #[test]
    fn test_markov_transition_count() {
        let mut markov = MarkovPredictor::new();
        assert_eq!(markov.transition_count(), 0);
        markov.record_transition("A", "B");
        markov.record_transition("A", "C");
        assert_eq!(markov.transition_count(), 2);
    }

    #[test]
    fn test_prediction_top_k() {
        let mut markov = MarkovPredictor::new();
        for tool in ["A", "B", "C", "D", "E"] {
            markov.record_transition("X", tool);
        }
        let ctx = make_ctx(&["X"]);
        let predictions = markov.predict(&ctx, 2);
        assert_eq!(predictions.len(), 2);
    }

    // ── Internal function tests ─────────────────────────────────────────

    #[test]
    fn test_softmax_1d_basic() {
        let x = Array1::from_vec(vec![1.0, 2.0, 3.0]);
        let result = softmax_1d(&x);
        let sum: f64 = result.sum();
        assert!((sum - 1.0).abs() < 1e-10);
        // Larger input => larger output
        assert!(result[2] > result[1]);
        assert!(result[1] > result[0]);
    }

    #[test]
    fn test_softmax_1d_numerical_stability() {
        // Large values that would overflow naive exp()
        let x = Array1::from_vec(vec![1000.0, 1001.0, 1002.0]);
        let result = softmax_1d(&x);
        let sum: f64 = result.sum();
        assert!(
            (sum - 1.0).abs() < 1e-10,
            "Softmax should be stable with large inputs"
        );
    }

    #[test]
    fn test_layer_norm_basic() {
        let x = Array2::from_shape_vec((1, 4), vec![1.0, 2.0, 3.0, 4.0]).unwrap();
        let gamma = Array1::ones(4);
        let beta = Array1::zeros(4);
        let result = layer_norm(&x, &gamma, &beta);
        // After layer norm, mean should be ~0 and std ~1
        let mean = result.row(0).mean().unwrap();
        assert!(
            mean.abs() < 1e-5,
            "Layer norm mean should be ~0, got {mean}"
        );
    }

    #[test]
    fn test_positional_encoding_shape() {
        let pe = sinusoidal_positional_encoding(MAX_SEQ_LEN, D_MODEL);
        assert_eq!(pe.shape(), &[MAX_SEQ_LEN, D_MODEL]);
    }

    #[test]
    fn test_relu_basic() {
        let x = Array2::from_shape_vec((1, 4), vec![-1.0, 0.0, 1.0, 2.0]).unwrap();
        let result = relu(&x);
        assert_eq!(result[[0, 0]], 0.0);
        assert_eq!(result[[0, 1]], 0.0);
        assert_eq!(result[[0, 2]], 1.0);
        assert_eq!(result[[0, 3]], 2.0);
    }

    #[test]
    fn test_xorshift_deterministic() {
        let mut rng1 = Xorshift64::new(42);
        let mut rng2 = Xorshift64::new(42);
        for _ in 0..100 {
            assert_eq!(rng1.next_u64(), rng2.next_u64());
        }
    }

    #[test]
    fn test_xorshift_zero_seed_handled() {
        let mut rng = Xorshift64::new(0);
        // Should not get stuck at 0
        assert_ne!(rng.next_u64(), 0);
    }
}
