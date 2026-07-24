//! DSPy Module — signature-aware program transformation.

use std::collections::HashMap;

use touring_intelligence::reasoning::{HybridCognitiveEngine, ReasoningQuery};

use super::dspy_signature::DspySignature;

/// Outcome of a single `DspyModule::forward` pass: the produced output fields plus confidence.
pub struct ModuleResult {
    /// Generated values keyed by the signature's output field names.
    pub outputs: HashMap<String, String>,
    /// Confidence in the result, in `0.0..=1.0`, derived from pheromone strength.
    pub confidence: f64,
    /// Identifier of the cognitive engine that produced this result.
    pub engine_name: String,
}

impl ModuleResult {
    /// Returns the output value for `key`, or `None` if the field was not produced.
    pub fn get(&self, key: &str) -> Option<&String> {
        self.outputs.get(key)
    }
    /// Returns `true` when confidence meets the acceptance threshold (`>= 0.6`).
    pub fn is_acceptable(&self) -> bool {
        self.confidence >= 0.6
    }
}

/// A DSPy-style program unit pairing a declarative signature with a cognitive engine.
pub struct DspyModule {
    /// Declarative description of the module's input and output fields.
    pub signature: DspySignature,
    engine: HybridCognitiveEngine,
    compiled_prompt: Option<String>,
}

impl DspyModule {
    /// Creates a module for `signature` backed by a fresh-pheromone cognitive engine.
    pub fn new(signature: DspySignature) -> Self {
        Self {
            signature,
            engine: HybridCognitiveEngine::with_fresh_pheromone(),
            compiled_prompt: None,
        }
    }
    /// Attaches a teleprompter-compiled prompt, overriding the signature default.
    pub fn with_compiled_prompt(mut self, prompt: String) -> Self {
        self.compiled_prompt = Some(prompt);
        self
    }
    /// Returns the active prompt: the compiled one if set, else the signature's.
    pub fn get_prompt(&self) -> String {
        self.compiled_prompt
            .clone()
            .unwrap_or_else(|| self.signature.to_prompt())
    }
    /// Runs the module over `inputs`, producing the signature's outputs with confidence.
    pub fn forward(&self, inputs: &HashMap<String, String>) -> ModuleResult {
        let prompt = self.get_prompt();
        let root_state = self.hash_inputs(inputs);
        let description = inputs
            .get("intent")
            .or(inputs.get("code"))
            .cloned()
            .unwrap_or_default();
        let query = ReasoningQuery::new(root_state, description)
            .with_context("signature".to_string(), self.signature.name.clone())
            .with_context("prompt".to_string(), prompt);
        // HybridCognitiveEngine uses pheromone-guided node selection, not direct search.
        // Use select_next_node() for pheromone-guided scoring.
        let pheromone_score = self
            .engine
            .shared_pheromone
            .lock()
            .ok()
            .map(|g| g.strength(root_state, root_state.wrapping_add(1)))
            .unwrap_or(0.0);
        let confidence = (0.5 + pheromone_score * 0.5).min(1.0);
        let mut outputs = HashMap::new();
        for name in self.signature.output_names() {
            outputs.insert(name.to_string(), query.description.clone());
        }
        ModuleResult {
            outputs,
            confidence,
            engine_name: "hybrid_cognitive".to_string(),
        }
    }
    fn hash_inputs(&self, inputs: &HashMap<String, String>) -> u64 {
        let mut h = 0u64;
        for (k, v) in inputs.iter() {
            h = h.wrapping_mul(31).wrapping_add(self.hash_str(k));
            h = h.wrapping_mul(37).wrapping_add(self.hash_str(v));
        }
        h
    }
    fn hash_str(&self, s: &str) -> u64 {
        s.bytes().fold(0u64, |acc, b| {
            acc.wrapping_mul(31).wrapping_add(u64::from(b))
        })
    }
}
