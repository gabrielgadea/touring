//! E2E Cross-Audit: DSPy API Integration Test
//!
//! Audits:
//! 1. PURPOSE: Does code fulfill documented intent?
//! 2. CONTRACTS: Interface contracts preserved?
//! 3. INVARIANTS: Exit 0? No unwrap in prod?
//! 4. EDGE CASES: Error paths handled gracefully?
//! 5. INTEGRATION: Components communicate correctly?
//! 6. E2E FLOW: Complete workflow executes?

use std::collections::HashMap;

use crate::dspy::{
    BootstrapFewShot, Demo, DspyCompiler, DspyModule, DspySignature, ModuleResult, SignatureField,
    Teleprompter, code_generation_sig, code_reflection_sig, test_generation_sig,
};

#[test]
fn audit_1_purpose_signature_creation() {
    // Verify pre-built signatures exist and have correct structure
    let sig = code_generation_sig();
    assert_eq!(sig.name, "code_generation");
    assert!(!sig.instruction.is_empty());
    assert_eq!(sig.inputs.len(), 2);
    assert_eq!(sig.outputs.len(), 2);
    assert_eq!(sig.input_names(), vec!["intent", "context"]);
    assert_eq!(sig.output_names(), vec!["code", "explanation"]);

    // Custom signature with required/optional
    let custom = DspySignature::new(
        "custom_sig",
        "Custom instruction.",
        vec![SignatureField::new("input1", "desc1")],
        vec![SignatureField::optional("output1", "desc2")],
    );
    assert_eq!(custom.name, "custom_sig");
    assert!(custom.inputs[0].required);
    assert!(!custom.outputs[0].required);

    // to_prompt format
    let prompt = sig.to_prompt();
    assert!(prompt.contains("Inputs:"));
    assert!(prompt.contains("Outputs:"));
    assert!(prompt.contains("Generate Rust code"));
}

#[test]
fn audit_2_contracts_teleprompter_compilation() {
    let sig = code_generation_sig();

    // Empty demos → low confidence but doesn't crash
    let empty = BootstrapFewShot::new(8).compile(&sig, &[]);
    assert!(!empty.is_acceptable());
    assert_eq!(empty.demo_count, 0);
    assert!(empty.prompt.contains("code_generation"));

    // With demos
    let mut demos = Vec::new();

    let mut d1_in = HashMap::new();
    d1_in.insert("intent".to_string(), "add two numbers".to_string());
    d1_in.insert("context".to_string(), "impl Add for i32".to_string());
    let mut d1_out = HashMap::new();
    d1_out.insert(
        "code".to_string(),
        "fn add(a: i32, b: i32) -> i32 { a + b }".to_string(),
    );
    d1_out.insert(
        "explanation".to_string(),
        "Uses standard addition".to_string(),
    );
    demos.push(Demo::new(d1_in, d1_out));

    let mut d2_in = HashMap::new();
    d2_in.insert(
        "intent".to_string(),
        "divide with error handling".to_string(),
    );
    d2_in.insert("context".to_string(), "impl Div for f64".to_string());
    let mut d2_out = HashMap::new();
    d2_out.insert(
        "code".to_string(),
        "fn div(a: f64, b: f64) -> Option<f64> { if b == 0.0 { None } else { Some(a / b) } }"
            .to_string(),
    );
    d2_out.insert("explanation".to_string(), "Returns Option".to_string());
    demos.push(Demo::new(d2_in, d2_out));

    let compiled = BootstrapFewShot::new(8).compile(&sig, &demos);
    assert!(compiled.prompt.contains("Demonstrations"));
    assert!(compiled.prompt.contains("Example 1"));
    assert!(compiled.prompt.contains("Example 2"));
    assert_eq!(compiled.demo_count, 2);
    assert!(compiled.is_acceptable());

    // Demo::simple factory
    let simple = Demo::simple("x", "42", "y", "43");
    assert_eq!(simple.inputs.get("x"), Some(&"42".to_string()));
    assert_eq!(simple.outputs.get("y"), Some(&"43".to_string()));
}

#[test]
fn audit_3_invariants_compiler_registry() {
    let compiler = DspyCompiler::new();

    // compile_bootstrap
    let sig = code_generation_sig();
    let boostrapped = compiler.compile_bootstrap(&sig);
    assert!(!boostrapped.is_empty());
    assert!(boostrapped.len_chars() > 0);
    assert_eq!(boostrapped.teleprompter_name, "BootstrapFewShot");

    // compile_mcts
    let mcts = compiler.compile_mcts(&sig);
    assert!(!mcts.is_empty());
    assert_eq!(mcts.teleprompter_name, "MCTSTeleprompter");

    // get_signature — returns Option<&DspySignature>
    let cg_sig = compiler.get_signature("code_generation");
    assert!(cg_sig.is_some());
    assert_eq!(
        cg_sig
            .expect("audit: code_generation signature present")
            .name,
        "code_generation"
    );

    let cr_sig = compiler.get_signature("code_reflection");
    assert!(cr_sig.is_some());
    assert_eq!(
        cr_sig
            .expect("audit: code_reflection signature present")
            .name,
        "code_reflection"
    );

    let tg_sig = compiler.get_signature("test_generation");
    assert!(tg_sig.is_some());
    assert_eq!(
        tg_sig
            .expect("audit: test_generation signature present")
            .name,
        "test_generation"
    );

    assert!(compiler.get_signature("nonexistent").is_none());
}

#[test]
fn audit_4_edge_cases_module_result_behavior() {
    // Zero confidence is not acceptable
    let result = ModuleResult {
        outputs: HashMap::new(),
        confidence: 0.0,
        engine_name: "test".to_string(),
    };
    assert!(!result.is_acceptable());
    assert_eq!(result.get("nonexistent"), None);

    // High confidence is acceptable
    let result_ok = ModuleResult {
        outputs: [("key".to_string(), "value".to_string())]
            .into_iter()
            .collect(),
        confidence: 0.8,
        engine_name: "hybrid".to_string(),
    };
    assert!(result_ok.is_acceptable());
    assert_eq!(result_ok.get("key"), Some(&"value".to_string()));

    // DspyModule: signature vs compiled_prompt
    let module = DspyModule::new(code_generation_sig());
    assert!(module.get_prompt().contains("Generate Rust code"));

    let module_with_prompt =
        DspyModule::new(code_generation_sig()).with_compiled_prompt("CUSTOM PROMPT".to_string());
    assert_eq!(module_with_prompt.get_prompt(), "CUSTOM PROMPT");

    // forward with various inputs
    let mut inputs = HashMap::new();
    inputs.insert("intent".to_string(), "implement subtract".to_string());
    let result = module.forward(&inputs);
    assert!(result.confidence >= 0.0 && result.confidence <= 1.0);
    assert_eq!(result.engine_name, "hybrid_cognitive");

    // forward with empty inputs (graceful)
    let empty_result = module.forward(&HashMap::new());
    assert_eq!(empty_result.engine_name, "hybrid_cognitive");

    // forward with code key (intent fallback)
    let mut code_input = HashMap::new();
    code_input.insert("code".to_string(), "fn x() {}".to_string());
    let code_result = module.forward(&code_input);
    assert_eq!(code_result.engine_name, "hybrid_cognitive");
}

#[test]
fn audit_5_integration_full_pipeline() {
    // Pipeline: Signature → Demos → Compile → Module → Forward
    let pipeline_sig = test_generation_sig();

    let mut compiler = DspyCompiler::new();
    let demo = Demo::new(
        [(
            "function_sig".to_string(),
            "fn add(a: i32, b: i32) -> i32".to_string(),
        )]
        .into_iter()
        .collect(),
        [
            (
                "tests".to_string(),
                "#[test] fn test_add() { assert_eq!(add(2, 2), 4); }".to_string(),
            ),
            (
                "edge_cases".to_string(),
                "negative numbers, zero, overflow".to_string(),
            ),
        ]
        .into_iter()
        .collect(),
    );
    compiler.add_demo(demo);

    // Compile
    let compiled = compiler.compile_bootstrap(&pipeline_sig);
    assert!(compiled.prompt.contains("test_generation"));
    assert!(compiled.prompt.contains("Demonstrations"));

    // Module with compiled prompt
    let module = DspyModule::new(pipeline_sig).with_compiled_prompt(compiled.prompt.clone());
    let inputs = [
        (
            "function_sig".to_string(),
            "fn sub(a: i32, b: i32) -> i32".to_string(),
        ),
        ("context".to_string(), "impl Sub for i32".to_string()),
    ]
    .into_iter()
    .collect();

    let result = module.forward(&inputs);
    assert!(result.confidence >= 0.0);
    assert_eq!(result.engine_name, "hybrid_cognitive");
}

#[test]
fn audit_6_e2e_complete_workflow() {
    // Simulate H100 DSPyIntegrationHandler actual use case:
    // Write tool → extract content → compile → inject suggestion

    // 1. Build signature from content
    let sig_inputs = vec![
        SignatureField::new("code", "The code to analyze"),
        SignatureField::new("symbol", "Primary symbol name"),
    ];
    let sig_outputs = vec![
        SignatureField::new("correctness_score", "Correctness score"),
        SignatureField::new("suggestions", "Improvement suggestions"),
    ];
    let analysis_sig = DspySignature::new(
        "code_review",
        "Analyze code quality.",
        sig_inputs,
        sig_outputs,
    );

    // 2. Compile with demos
    let mut compiler = DspyCompiler::new();
    compiler.add_demo(Demo::simple(
        "code",
        "fn add(a, b) { a + b }",
        "suggestions",
        "add return type",
    ));
    let compiled = compiler.compile_bootstrap(&analysis_sig);
    assert!(!compiled.is_empty());

    // 3. Module forward
    let module = DspyModule::new(analysis_sig).with_compiled_prompt(compiled.prompt);
    let forward_inputs = [
        (
            "code".to_string(),
            "pub fn multiply(a: i32, b: i32) -> i32 { a * b }".to_string(),
        ),
        ("symbol".to_string(), "multiply".to_string()),
    ]
    .into_iter()
    .collect();
    let result = module.forward(&forward_inputs);

    // Assertions
    assert!(result.confidence >= 0.0 && result.confidence <= 1.0);
    assert!(!result.engine_name.is_empty());
    assert_eq!(result.engine_name, "hybrid_cognitive");
}

#[test]
fn dspy_signature_all_prebuilt_work() {
    // All three pre-built signatures must work
    let cg = code_generation_sig();
    assert_eq!(cg.name, "code_generation");
    assert!(!cg.inputs.is_empty());
    assert!(!cg.outputs.is_empty());

    let cr = code_reflection_sig();
    assert_eq!(cr.name, "code_reflection");
    assert!(cr.inputs.iter().any(|f| f.name == "code"));
    assert!(cr.outputs.len() >= 4); // correctness, style, security, robustness, suggestions

    let tg = test_generation_sig();
    assert_eq!(tg.name, "test_generation");
    assert!(tg.inputs.iter().any(|f| f.name == "function_sig"));
}

#[test]
fn dspy_module_clone_is_functional() {
    // Test that Clone derive works (needed for DspyModule::new signature Clone)
    let sig = code_generation_sig();
    let sig_clone = sig.clone();
    assert_eq!(sig.name, sig_clone.name);
    assert_eq!(sig.instruction, sig_clone.instruction);

    // SignatureField clone
    let field = SignatureField::new("test", "desc");
    let field_clone = field.clone();
    assert_eq!(field.name, field_clone.name);
}

#[test]
fn dspy_compiler_forward_integration() {
    let compiler = DspyCompiler::new();
    let sig = code_generation_sig();

    let mut inputs = HashMap::new();
    inputs.insert(
        "intent".to_string(),
        "create a greeting function".to_string(),
    );
    inputs.insert("context".to_string(), "module: greet".to_string());

    let result = compiler.forward(&sig, &inputs);
    assert!(result.confidence >= 0.0);
    assert_eq!(result.engine_name, "hybrid_cognitive");
}
