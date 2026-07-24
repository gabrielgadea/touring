//! DSPy Signature — declarative input/output field definitions.

/// A single named input or output field within a `DspySignature`.
#[derive(Clone)]
pub struct SignatureField {
    /// Field identifier referenced by inputs/outputs maps.
    pub name: String,
    /// Human-readable description of what the field holds.
    pub desc: String,
    /// Whether the field must be present for the module to run.
    pub required: bool,
}

impl SignatureField {
    /// Creates a required field with the given name and description.
    pub fn new(name: &str, desc: &str) -> Self {
        Self {
            name: name.to_string(),
            desc: desc.to_string(),
            required: true,
        }
    }
    /// Creates an optional (non-required) field with the given name and description.
    pub fn optional(name: &str, desc: &str) -> Self {
        Self {
            name: name.to_string(),
            desc: desc.to_string(),
            required: false,
        }
    }
}

/// Declarative description of a DSPy module's task: instruction plus typed I/O fields.
#[derive(Clone)]
pub struct DspySignature {
    /// Signature identifier, used as the task name in prompts and context.
    pub name: String,
    /// Natural-language task instruction prepended to the rendered prompt.
    pub instruction: String,
    /// Ordered set of input fields the module consumes.
    pub inputs: Vec<SignatureField>,
    /// Ordered set of output fields the module is expected to produce.
    pub outputs: Vec<SignatureField>,
}

impl DspySignature {
    /// Creates a signature from a name, instruction, and its input/output fields.
    pub fn new(
        name: &str,
        instruction: &str,
        inputs: Vec<SignatureField>,
        outputs: Vec<SignatureField>,
    ) -> Self {
        Self {
            name: name.to_string(),
            instruction: instruction.to_string(),
            inputs,
            outputs,
        }
    }
    /// Renders the signature into a text prompt listing instruction, inputs, and outputs.
    pub fn to_prompt(&self) -> String {
        let mut p = format!("{}\n\n", self.instruction);
        p.push_str("Inputs:\n");
        for f in &self.inputs {
            p.push_str(&format!("  - {}: {}\n", f.name, f.desc));
        }
        p.push_str("\nOutputs:\n");
        for f in &self.outputs {
            p.push_str(&format!("  - {}: {}\n", f.name, f.desc));
        }
        p
    }
    /// Returns the names of all input fields in declaration order.
    pub fn input_names(&self) -> Vec<&str> {
        self.inputs.iter().map(|f| f.name.as_str()).collect()
    }
    /// Returns the names of all output fields in declaration order.
    pub fn output_names(&self) -> Vec<&str> {
        self.outputs.iter().map(|f| f.name.as_str()).collect()
    }
}

/// Code generation signature: intent + context → code + explanation.
pub fn code_generation_sig() -> DspySignature {
    DspySignature::new(
        "code_generation",
        "Generate Rust code that implements the described functionality.",
        vec![
            SignatureField::new(
                "intent",
                "Natural language description of what the code should do",
            ),
            SignatureField::new(
                "context",
                "Existing code context relevant to the generation task",
            ),
        ],
        vec![
            SignatureField::new("code", "The generated Rust code implementation"),
            SignatureField::new("explanation", "Brief explanation of how the code works"),
        ],
    )
}

/// Code reflection signature: code + symbol → scores + suggestions.
pub fn code_reflection_sig() -> DspySignature {
    DspySignature::new(
        "code_reflection",
        "Analyze code for quality issues across multiple dimensions.",
        vec![
            SignatureField::new("code", "The code to analyze"),
            SignatureField::new("symbol", "The primary symbol being implemented"),
        ],
        vec![
            SignatureField::new("correctness_score", "Score for correctness"),
            SignatureField::new("style_score", "Score for style"),
            SignatureField::new("security_score", "Score for security"),
            SignatureField::new("robustness_score", "Score for error handling"),
            SignatureField::new("suggestions", "List of improvement suggestions"),
        ],
    )
}

/// Test generation signature: function_sig + context → tests + edge_cases.
pub fn test_generation_sig() -> DspySignature {
    DspySignature::new(
        "test_generation",
        "Generate test cases for the given function or module.",
        vec![
            SignatureField::new("function_sig", "The function signature to test"),
            SignatureField::new("context", "The module context containing the function"),
        ],
        vec![
            SignatureField::new("tests", "Generated test code"),
            SignatureField::new("edge_cases", "Identified edge cases to test"),
        ],
    )
}
