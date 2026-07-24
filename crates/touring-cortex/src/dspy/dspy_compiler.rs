//! DSPy Compiler — main entry point for prompt compilation.

use std::collections::HashMap;

use super::dspy_module::{DspyModule, ModuleResult};
use super::dspy_signature::{
    DspySignature, code_generation_sig, code_reflection_sig, test_generation_sig,
};
use super::dspy_teleprompter::{
    BootstrapFewShot, CompiledPrompt, Demo, MCTSTeleprompter, Teleprompter,
};

/// DSPy compiler — main entry point for prompt compilation.
///
/// Coordinates between signatures, modules, and teleprompters
/// to produce optimized prompts.
pub struct DspyCompiler {
    /// Available teleprompters.
    teleprompters: HashMap<String, Box<dyn Teleprompter>>,
    /// Demonstrations cache.
    demos: Vec<Demo>,
}

impl DspyCompiler {
    /// Create a new compiler with default teleprompters.
    pub fn new() -> Self {
        let mut teleprompters = HashMap::new();
        teleprompters.insert(
            "bootstrap_few_shot".to_string(),
            Box::new(BootstrapFewShot::new(8)) as Box<dyn Teleprompter>,
        );
        teleprompters.insert(
            "mcts".to_string(),
            Box::new(MCTSTeleprompter::new(8)) as Box<dyn Teleprompter>,
        );

        Self {
            teleprompters,
            demos: Vec::new(),
        }
    }

    /// Add a demonstration.
    pub fn add_demo(&mut self, demo: Demo) {
        self.demos.push(demo);
    }

    /// Clear all demonstrations.
    pub fn clear_demos(&mut self) {
        self.demos.clear();
    }

    /// Compile a signature using the specified teleprompter.
    pub fn compile(&self, signature: &DspySignature, teleprompter: &str) -> CompiledPrompt {
        let tp = self
            .teleprompters
            .get(teleprompter)
            .expect("Unknown teleprompter");

        tp.compile(signature, &self.demos)
    }

    /// Compile using BootstrapFewShot (default).
    pub fn compile_bootstrap(&self, signature: &DspySignature) -> CompiledPrompt {
        self.compile(signature, "bootstrap_few_shot")
    }

    /// Compile using MCTS (more sophisticated).
    pub fn compile_mcts(&self, signature: &DspySignature) -> CompiledPrompt {
        self.compile(signature, "mcts")
    }

    /// Forward pass through a module with compiled prompt.
    pub fn forward(
        &self,
        signature: &DspySignature,
        inputs: &HashMap<String, String>,
    ) -> ModuleResult {
        let module = DspyModule::new(signature.clone());
        module.forward(inputs)
    }

    /// Get a pre-built signature by name.
    pub fn get_signature(&self, name: &str) -> Option<DspySignature> {
        match name {
            "code_generation" => Some(code_generation_sig()),
            "code_reflection" => Some(code_reflection_sig()),
            "test_generation" => Some(test_generation_sig()),
            _ => None,
        }
    }
}

impl Default for DspyCompiler {
    fn default() -> Self {
        Self::new()
    }
}
