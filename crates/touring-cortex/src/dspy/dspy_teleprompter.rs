//! DSPy Teleprompter — automatic prompt optimization.

use std::collections::HashMap;

use touring_intelligence::reasoning::persistence::GraphPersistence;
use touring_intelligence::reasoning::semantic_graph::SemanticGraph;
use touring_intelligence::reasoning::{CognitiveMCTSConfig, GraphInformedMCTS};

use super::dspy_signature::DspySignature;

/// A labeled input/output example used to bootstrap few-shot prompts.
#[derive(Debug, Clone)]
pub struct Demo {
    /// Example input field values keyed by field name.
    pub inputs: HashMap<String, String>,
    /// Example output field values keyed by field name.
    pub outputs: HashMap<String, String>,
}

impl Demo {
    /// Creates a demo from the given input and output field maps.
    pub fn new(inputs: HashMap<String, String>, outputs: HashMap<String, String>) -> Self {
        Self { inputs, outputs }
    }
    /// Creates a single-field demo from one input pair and one output pair.
    pub fn simple(input_key: &str, input_val: &str, output_key: &str, output_val: &str) -> Self {
        let mut inputs = HashMap::new();
        inputs.insert(input_key.to_string(), input_val.to_string());
        let mut outputs = HashMap::new();
        outputs.insert(output_key.to_string(), output_val.to_string());
        Self { inputs, outputs }
    }
}

/// A teleprompter's optimized prompt together with its quality metadata.
#[derive(Debug, Clone)]
pub struct CompiledPrompt {
    /// The rendered, optimized prompt text.
    pub prompt: String,
    /// Confidence in the prompt's quality, in `0.0..=1.0`.
    pub confidence: f64,
    /// Number of demonstrations folded into the prompt.
    pub demo_count: usize,
    /// Name of the teleprompter that produced this prompt.
    pub teleprompter_name: String,
}

impl CompiledPrompt {
    /// Creates an empty-metadata prompt attributed to `teleprompter_name`.
    pub fn new(prompt: String, teleprompter_name: &str) -> Self {
        Self {
            prompt,
            confidence: 0.0,
            demo_count: 0,
            teleprompter_name: teleprompter_name.to_string(),
        }
    }
    /// Returns `true` when confidence meets the acceptance threshold (`>= 0.5`).
    pub fn is_acceptable(&self) -> bool {
        self.confidence >= 0.5
    }
    /// Returns the prompt length counted in Unicode scalar values.
    pub fn len_chars(&self) -> usize {
        self.prompt.chars().count()
    }
    /// Returns `true` when the prompt text is empty.
    pub fn is_empty(&self) -> bool {
        self.prompt.is_empty()
    }
}

/// Strategy for compiling a signature and demonstrations into an optimized prompt.
pub trait Teleprompter: Send + Sync {
    /// Compiles `signature` plus `demos` into a `CompiledPrompt`.
    fn compile(&self, signature: &DspySignature, demos: &[Demo]) -> CompiledPrompt;
}

/// Teleprompter that builds a few-shot prompt from the first N demonstrations.
pub struct BootstrapFewShot {
    max_demos: usize,
}

impl BootstrapFewShot {
    /// Creates a bootstrap teleprompter that includes up to `max_demos` examples.
    pub fn new(max_demos: usize) -> Self {
        Self { max_demos }
    }
}

impl Teleprompter for BootstrapFewShot {
    fn compile(&self, sig: &DspySignature, demos: &[Demo]) -> CompiledPrompt {
        let mut prompt = format!("# {}\n\n{}\n\n", sig.name, sig.instruction);
        if demos.is_empty() {
            return CompiledPrompt {
                prompt,
                confidence: 0.3,
                demo_count: 0,
                teleprompter_name: "BootstrapFewShot".to_string(),
            };
        }
        prompt.push_str("## Demonstrations\n\n");
        let cnt = demos.len().min(self.max_demos);
        for (i, demo) in demos.iter().take(cnt).enumerate() {
            prompt.push_str(&format!("### Example {}\n", i + 1));
            push_demo_io(&mut prompt, demo);
        }
        prompt.push_str("## Output Format\n");
        for f in &sig.outputs {
            prompt.push_str(&format!("- {}: <value>\n", f.name));
        }
        CompiledPrompt {
            prompt,
            confidence: 0.7f64.min(cnt as f64 * 0.1 + 0.4),
            demo_count: cnt,
            teleprompter_name: "BootstrapFewShot".to_string(),
        }
    }
}

/// Teleprompter that ranks demonstrations via graph-informed MCTS before selection.
pub struct MCTSTeleprompter {
    mcts_config: CognitiveMCTSConfig,
    max_demos: usize,
}

impl MCTSTeleprompter {
    /// Creates an MCTS teleprompter that scores and selects from up to `max_demos` examples.
    pub fn new(max_demos: usize) -> Self {
        Self {
            mcts_config: CognitiveMCTSConfig {
                mcts_config: touring_intelligence::reasoning::MCTSConfig {
                    max_depth: 4,
                    num_rollouts: 32,
                    exploration_constant: std::f64::consts::SQRT_2,
                    discount: 0.99,
                    max_rollouts: 100,
                },
                relevance_weight: 0.6,
                pheromone_alpha: 0.3,
                pheromone_evaporation_rate: 0.05,
            },
            max_demos,
        }
    }
}

impl Teleprompter for MCTSTeleprompter {
    fn compile(&self, sig: &DspySignature, demos: &[Demo]) -> CompiledPrompt {
        let persistence =
            std::sync::Arc::new(GraphPersistence::new(std::path::PathBuf::from("/dev/null")));
        let graph = SemanticGraph::new(persistence);
        let id_map: HashMap<String, u64> = HashMap::new();
        let rev_map: HashMap<u64, String> = HashMap::new();
        let mcts = GraphInformedMCTS::new(self.mcts_config.clone());

        let mut scored: Vec<(usize, f64)> = Vec::new();
        for (i, demo) in demos.iter().enumerate().take(self.max_demos) {
            let state = demo_state(demo);
            let score = mcts
                .search(state, &graph, &id_map, &rev_map)
                .map(|r| r.value)
                .unwrap_or(0.5);
            scored.push((i, score));
        }
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        let mut prompt = format!("# {}\n\n{}\n\n", sig.name, sig.instruction);
        if !scored.is_empty() {
            prompt.push_str("## Best Demonstrations\n\n");
            for (rank, &(idx, score)) in scored.iter().enumerate() {
                if rank >= 3 {
                    break;
                }
                let demo = match demos.get(idx) {
                    Some(d) => d,
                    None => continue,
                };
                prompt.push_str(&format!("### Example (score: {:.2})\n", score));
                push_demo_io(&mut prompt, demo);
            }
        }
        prompt.push_str("## Output Format\n");
        for f in &sig.outputs {
            prompt.push_str(&format!("- {}: <value>\n", f.name));
        }

        let avg = if scored.is_empty() {
            0.5
        } else {
            scored.iter().take(3).map(|(_, s)| s).sum::<f64>() / 3.0
        };
        CompiledPrompt {
            prompt,
            confidence: avg,
            demo_count: scored.len().min(3),
            teleprompter_name: "MCTSTeleprompter".to_string(),
        }
    }
}

/// Appends a demo's `**Input:**` and `**Output:**` field lists to `prompt`,
/// followed by a trailing blank line. Shared by every teleprompter that renders
/// few-shot examples so the demonstration layout stays identical across strategies.
fn push_demo_io(prompt: &mut String, demo: &Demo) {
    prompt.push_str("**Input:**\n");
    for (k, v) in &demo.inputs {
        prompt.push_str(&format!("- {}: {}\n", k, v));
    }
    prompt.push_str("**Output:**\n");
    for (k, v) in &demo.outputs {
        prompt.push_str(&format!("- {}: {}\n", k, v));
    }
    prompt.push('\n');
}

fn demo_state(demo: &Demo) -> u64 {
    let mut h = 0u64;
    for (k, v) in demo.inputs.iter().chain(demo.outputs.iter()) {
        h = h
            .wrapping_mul(31)
            .wrapping_add(k.bytes().fold(0u64, |acc, b| {
                acc.wrapping_mul(17).wrapping_add(u64::from(b))
            }));
        h = h
            .wrapping_mul(37)
            .wrapping_add(v.bytes().fold(0u64, |acc, b| {
                acc.wrapping_mul(19).wrapping_add(u64::from(b))
            }));
    }
    h
}
