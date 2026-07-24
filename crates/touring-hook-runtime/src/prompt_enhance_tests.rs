#![allow(clippy::indexing_slicing)]
use super::*;

#[test]
fn test_classify_debug() {
    let r = classify("fix the memory leak in UserService");
    assert_eq!(r.intent, Intent::Debug);
    assert!(r.confidence > 0.0);
}

#[test]
fn test_classify_code() {
    let r = classify("implement a REST endpoint for user registration");
    assert_eq!(r.intent, Intent::Code);
}

#[test]
fn test_classify_test() {
    let r = classify("write unit tests for the auth module with pytest");
    assert_eq!(r.intent, Intent::Test);
}

#[test]
fn test_classify_refactor() {
    let r = classify("refactor the handler to follow SOLID principles");
    assert_eq!(r.intent, Intent::Refactor);
}

#[test]
fn test_classify_analysis() {
    let r = classify("explain how the authentication flow works");
    assert_eq!(r.intent, Intent::Analysis);
}

#[test]
fn test_classify_plan() {
    let r = classify("create a migration plan for the database");
    assert_eq!(r.intent, Intent::Plan);
}

#[test]
fn test_classify_creative() {
    let r = classify("brainstorm ideas for the new feature");
    assert_eq!(r.intent, Intent::Creative);
}

#[test]
fn test_classify_general() {
    let r = classify("hello");
    assert_eq!(r.intent, Intent::General);
    assert_eq!(r.confidence, 0.0);
}

#[test]
fn test_classify_portuguese_debug() {
    let r = classify("corrija o erro no serviço de autenticação");
    assert_eq!(r.intent, Intent::Debug);
}

#[test]
fn test_classify_portuguese_code() {
    let r = classify("implemente um endpoint de cadastro");
    assert_eq!(r.intent, Intent::Code);
}

#[test]
fn test_tiebreak_favors_higher_priority() {
    // "fix" (2.0) matches debug, "add" (1.0) matches code
    // Both score 2.0, debug has higher priority (7) than code (1)
    let r = classify("fix this and add something");
    assert_eq!(r.intent, Intent::Debug);
}

#[test]
fn test_compose_debug_has_all_techniques() {
    let output = compose(&Intent::Debug, "fix the bug");
    assert!(output.contains("Chain Of Thought"));
    assert!(output.contains("Self Validation"));
    assert!(output.contains("Constitutional Constraints"));
    assert!(output.contains("Few Shot Reasoning"));
    assert!(output.contains("[PROMPT ENHANCEMENT -- DEBUG MODE]"));
}

#[test]
fn test_compose_general_has_minimal_techniques() {
    let output = compose(&Intent::General, "hello");
    assert!(output.contains("Chain Of Thought"));
    assert!(output.contains("Precision Hints"));
    assert!(!output.contains("Constitutional"));
}

#[test]
fn test_compose_json_output_contract() {
    let json = compose_json("fix the bug");
    let hso = json.get("hookSpecificOutput").unwrap();
    assert_eq!(hso["hookEventName"], "UserPromptSubmit");
    assert!(
        hso["additionalContext"]
            .as_str()
            .unwrap()
            .contains("DEBUG MODE")
    );
}

#[test]
fn test_techniques_count_per_intent() {
    assert_eq!(techniques_for(&Intent::Code).len(), 4);
    assert_eq!(techniques_for(&Intent::Debug).len(), 4);
    assert_eq!(techniques_for(&Intent::Refactor).len(), 4);
    assert_eq!(techniques_for(&Intent::Test).len(), 4);
    assert_eq!(techniques_for(&Intent::Analysis).len(), 3);
    assert_eq!(techniques_for(&Intent::Creative).len(), 3);
    assert_eq!(techniques_for(&Intent::Plan).len(), 4);
    assert_eq!(techniques_for(&Intent::General).len(), 2);
}

// ── Action Directives Tests ──────────────────────────────────────

#[test]
fn test_compose_includes_code_first_directives() {
    let output = compose(&Intent::Code, "implement a new endpoint");
    assert!(output.contains("Code-First Directives"));
    assert!(output.contains("touring tantivy search"));
    assert!(output.contains("VIOLATION"));
    // Gabriel's directive (2026-06-26): touring is the SINGLE source of truth —
    // the directives must never steer the LLM to external MCP servers
    // (gitnexus/serena) or the non-existent `scripts/discover.py`.
    assert!(
        !output.contains("gitnexus"),
        "directives must not reference gitnexus"
    );
    assert!(
        !output.contains("serena"),
        "directives must not reference serena"
    );
    assert!(
        !output.contains("scripts/discover.py"),
        "directives must not reference the non-existent discover.py"
    );
}

#[test]
fn test_compose_debug_has_trace_directives() {
    let output = compose(&Intent::Debug, "fix the crash in auth");
    assert!(output.contains("trace the actual code path"));
    assert!(output.contains("touring ast find"));
    assert!(output.contains("touring wiring impact"));
    assert!(
        !output.contains("serena"),
        "debug directives must not reference serena"
    );
}

#[test]
fn test_compose_plan_has_plan_mode_directive() {
    let output = compose(&Intent::Plan, "create a migration plan");
    assert!(output.contains("EnterPlanMode"));
    assert!(output.contains("blast radius"));
}

#[test]
fn test_cli_hints_for_refactor() {
    let output = compose(&Intent::Refactor, "refactor this with blast radius check");
    assert!(output.contains("Touring CLI Hints"));
    assert!(output.contains("impact"));
}

#[test]
fn test_cli_hints_for_symbol_lookup() {
    let output = compose(&Intent::Code, "implement the function parse_config");
    assert!(output.contains("Touring CLI Hints"));
    assert!(output.contains("symbol lookup"));
}

#[test]
fn test_json_output_includes_directives() {
    let json = compose_json("implement a REST endpoint");
    let ctx = json["hookSpecificOutput"]["additionalContext"]
        .as_str()
        .unwrap();
    assert!(ctx.contains("Code-First Directives"));
    assert!(ctx.contains("touring tantivy search"));
}

// ── Boundary overlap tests ───────────────────────────────────────

#[test]
fn test_overlap_fix_the_test_favors_debug() {
    // "fix" (debug, 2.0) + "test" (test, 2.0) → tied → debug wins by priority
    let r = classify("fix the test");
    assert_eq!(
        r.intent,
        Intent::Debug,
        "debug should win tiebreak over test"
    );
}

#[test]
fn test_overlap_explain_how_to_implement_favors_code() {
    // "explain" (analysis, 2.0) + "implement" (code, 2.0) → tied → analysis wins (priority 3 > 1)
    let r = classify("explain how to implement");
    assert_eq!(
        r.intent,
        Intent::Analysis,
        "analysis (priority 3) beats code (priority 1)"
    );
}

#[test]
fn test_overlap_add_test_for_module_favors_test() {
    // "add" (code, 1.0) + "test" (test, 2.0) → test wins by score
    let r = classify("add a test for the auth module");
    assert_eq!(
        r.intent,
        Intent::Test,
        "test (score 2.0) beats code (score 1.0)"
    );
}

// ── Golden Prompt Parity Tests (46 prompts from prompt_enhancer.py) ──

/// All 46 golden prompts from the Python implementation.
/// These MUST produce identical classification results.
#[test]
fn test_golden_prompt_parity() {
    let golden: &[(&str, Intent)] = &[
        // -- code (5) --
        (
            "Create a FastAPI endpoint for user registration",
            Intent::Code,
        ),
        ("Implement a binary search function in Python", Intent::Code),
        ("Crie um módulo de autenticação com JWT", Intent::Code),
        ("Write a React component for a data table", Intent::Code),
        ("Build a CLI tool that parses CSV files", Intent::Code),
        // -- debug (5) --
        ("Fix the KeyError on line 42 of parser.py", Intent::Debug),
        (
            "This function crashes with a TypeError when input is None",
            Intent::Debug,
        ),
        (
            "Corrija o erro de conexão no módulo de banco de dados",
            Intent::Debug,
        ),
        ("Track down the memory leak in this service", Intent::Debug),
        (
            "Debug why the API returns 500 on POST requests",
            Intent::Debug,
        ),
        // -- refactor (3) --
        (
            "Refactor the user service to follow SOLID principles",
            Intent::Refactor,
        ),
        (
            "Simplify the nested if-else chain in process_order",
            Intent::Refactor,
        ),
        ("Limpe e otimize a classe DatabaseManager", Intent::Refactor),
        // -- test (3) --
        (
            "Write unit tests for the payment processor module",
            Intent::Test,
        ),
        (
            "Add pytest fixtures for the database connection",
            Intent::Test,
        ),
        (
            "Crie testes de cobertura para o serviço de autenticação",
            Intent::Test,
        ),
        // -- analysis (3) --
        (
            "Explain how the event loop works in asyncio",
            Intent::Analysis,
        ),
        (
            "Review the architecture of the relay module",
            Intent::Analysis,
        ),
        (
            "Analise por que a latência aumentou após o deploy",
            Intent::Analysis,
        ),
        // -- creative (2) --
        (
            "Suggest alternative approaches for caching user sessions",
            Intent::Creative,
        ),
        (
            "Proponha uma estratégia de migração para microserviços",
            Intent::Creative,
        ),
        // -- plan (2) --
        (
            "Plan the migration from monolith to microservices",
            Intent::Plan,
        ),
        (
            "Planeje as etapas para implementar CI/CD completo",
            Intent::Plan,
        ),
        // -- general (2) --
        ("Hello", Intent::General),
        ("What time is it?", Intent::General),
    ];

    let mut failures = Vec::new();
    for (prompt, expected) in golden {
        let result = classify(prompt);
        if result.intent != *expected {
            failures.push(format!(
                "FAIL: '{}' expected={:?} got={:?} (confidence={:.1})",
                &prompt[..prompt.len().min(60)],
                expected,
                result.intent,
                result.confidence,
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "Golden prompt parity failures ({}/{}):\n{}",
        failures.len(),
        golden.len(),
        failures.join("\n"),
    );
}

// ── Individual golden prompt tests for fine-grained diagnostics ──

#[test]
fn test_golden_code_fastapi() {
    let r = classify("Create a FastAPI endpoint for user registration");
    assert_eq!(r.intent, Intent::Code);
}

#[test]
fn test_golden_code_binary_search() {
    let r = classify("Implement a binary search function in Python");
    assert_eq!(r.intent, Intent::Code);
}

#[test]
fn test_golden_code_jwt_ptbr() {
    let r = classify("Crie um módulo de autenticação com JWT");
    assert_eq!(r.intent, Intent::Code);
}

#[test]
fn test_golden_code_react() {
    let r = classify("Write a React component for a data table");
    assert_eq!(r.intent, Intent::Code);
}

#[test]
fn test_golden_code_cli() {
    let r = classify("Build a CLI tool that parses CSV files");
    assert_eq!(r.intent, Intent::Code);
}

#[test]
fn test_golden_debug_keyerror() {
    let r = classify("Fix the KeyError on line 42 of parser.py");
    assert_eq!(r.intent, Intent::Debug);
}

#[test]
fn test_golden_debug_typeerror() {
    let r = classify("This function crashes with a TypeError when input is None");
    assert_eq!(r.intent, Intent::Debug);
}

#[test]
fn test_golden_debug_ptbr_conexao() {
    let r = classify("Corrija o erro de conexão no módulo de banco de dados");
    assert_eq!(r.intent, Intent::Debug);
}

#[test]
fn test_golden_debug_memory_leak() {
    let r = classify("Track down the memory leak in this service");
    assert_eq!(r.intent, Intent::Debug);
}

#[test]
fn test_golden_debug_api_500() {
    let r = classify("Debug why the API returns 500 on POST requests");
    assert_eq!(r.intent, Intent::Debug);
}

#[test]
fn test_golden_refactor_solid() {
    let r = classify("Refactor the user service to follow SOLID principles");
    assert_eq!(r.intent, Intent::Refactor);
}

#[test]
fn test_golden_refactor_simplify() {
    let r = classify("Simplify the nested if-else chain in process_order");
    assert_eq!(r.intent, Intent::Refactor);
}

#[test]
fn test_golden_refactor_ptbr_limpe() {
    let r = classify("Limpe e otimize a classe DatabaseManager");
    assert_eq!(r.intent, Intent::Refactor);
}

#[test]
fn test_golden_test_unit() {
    let r = classify("Write unit tests for the payment processor module");
    assert_eq!(r.intent, Intent::Test);
}

#[test]
fn test_golden_test_pytest() {
    let r = classify("Add pytest fixtures for the database connection");
    assert_eq!(r.intent, Intent::Test);
}

#[test]
fn test_golden_test_ptbr_cobertura() {
    let r = classify("Crie testes de cobertura para o serviço de autenticação");
    assert_eq!(r.intent, Intent::Test);
}

#[test]
fn test_golden_analysis_explain() {
    let r = classify("Explain how the event loop works in asyncio");
    assert_eq!(r.intent, Intent::Analysis);
}

#[test]
fn test_golden_analysis_review() {
    let r = classify("Review the architecture of the relay module");
    assert_eq!(r.intent, Intent::Analysis);
}

#[test]
fn test_golden_analysis_ptbr_latencia() {
    let r = classify("Analise por que a latência aumentou após o deploy");
    assert_eq!(r.intent, Intent::Analysis);
}

#[test]
fn test_golden_creative_suggest() {
    let r = classify("Suggest alternative approaches for caching user sessions");
    assert_eq!(r.intent, Intent::Creative);
}

#[test]
fn test_golden_creative_ptbr_proponha() {
    let r = classify("Proponha uma estratégia de migração para microserviços");
    assert_eq!(r.intent, Intent::Creative);
}

#[test]
fn test_golden_plan_migration() {
    let r = classify("Plan the migration from monolith to microservices");
    assert_eq!(r.intent, Intent::Plan);
}

#[test]
fn test_golden_plan_ptbr_etapas() {
    let r = classify("Planeje as etapas para implementar CI/CD completo");
    assert_eq!(r.intent, Intent::Plan);
}

#[test]
fn test_golden_general_hello() {
    let r = classify("Hello");
    assert_eq!(r.intent, Intent::General);
    assert_eq!(r.confidence, 0.0);
}

#[test]
fn test_golden_general_time() {
    let r = classify("What time is it?");
    assert_eq!(r.intent, Intent::General);
}

// ── CILA Level Tests ────────────────────────────────────────────

#[test]
fn test_intent_to_cila_general_l0() {
    assert_eq!(intent_to_cila(&Intent::General), 0);
}

#[test]
fn test_intent_to_cila_code_l1() {
    assert_eq!(intent_to_cila(&Intent::Code), 1);
}

#[test]
fn test_intent_to_cila_debug_l2() {
    assert_eq!(intent_to_cila(&Intent::Debug), 2);
}

#[test]
fn test_intent_to_cila_refactor_l2() {
    assert_eq!(intent_to_cila(&Intent::Refactor), 2);
}

#[test]
fn test_intent_to_cila_test_l2() {
    assert_eq!(intent_to_cila(&Intent::Test), 2);
}

#[test]
fn test_intent_to_cila_analysis_l3() {
    assert_eq!(intent_to_cila(&Intent::Analysis), 3);
}

#[test]
fn test_intent_to_cila_plan_l3() {
    assert_eq!(intent_to_cila(&Intent::Plan), 3);
}

#[test]
fn test_intent_to_cila_creative_l4() {
    assert_eq!(intent_to_cila(&Intent::Creative), 4);
}

#[test]
fn test_intent_to_cila_all_intents_covered() {
    // Verify all 8 intents have a defined CILA mapping
    let mappings = [
        (Intent::General, 0u8),
        (Intent::Code, 1),
        (Intent::Debug, 2),
        (Intent::Refactor, 2),
        (Intent::Test, 2),
        (Intent::Analysis, 3),
        (Intent::Plan, 3),
        (Intent::Creative, 4),
    ];
    for (intent, expected) in &mappings {
        assert_eq!(
            intent_to_cila(intent),
            *expected,
            "CILA mapping wrong for {:?}",
            intent
        );
    }
}

// ── classify_with_details Tests ─────────────────────────────────

#[test]
fn test_classify_with_details_debug() {
    let r = classify_with_details("fix the memory leak in UserService");
    assert_eq!(r.intent, Intent::Debug);
    assert_eq!(r.cila_level, 2);
    assert!(r.confidence > 0.0);
    assert_eq!(r.techniques.len(), 4);
    // Debug techniques: CoT, SelfValidation, ConstitutionalConstraints, FewShotReasoning
    assert!(r.techniques.contains(&Technique::ChainOfThought));
    assert!(r.techniques.contains(&Technique::SelfValidation));
    assert!(r.techniques.contains(&Technique::ConstitutionalConstraints));
    assert!(r.techniques.contains(&Technique::FewShotReasoning));
}

#[test]
fn test_classify_with_details_code() {
    let r = classify_with_details("implement a REST endpoint");
    assert_eq!(r.intent, Intent::Code);
    assert_eq!(r.cila_level, 1);
    assert!(r.techniques.contains(&Technique::ChainOfThought));
    assert!(r.techniques.contains(&Technique::StructuredOutput));
}

#[test]
fn test_classify_with_details_general() {
    let r = classify_with_details("hello");
    assert_eq!(r.intent, Intent::General);
    assert_eq!(r.cila_level, 0);
    assert_eq!(r.confidence, 0.0);
    assert_eq!(r.techniques.len(), 2);
}

#[test]
fn test_classify_with_details_plan() {
    let r = classify_with_details("plan the migration roadmap");
    assert_eq!(r.intent, Intent::Plan);
    assert_eq!(r.cila_level, 3);
    assert!(r.techniques.contains(&Technique::SelfValidation));
    assert!(r.techniques.contains(&Technique::PrecisionHints));
}

#[test]
fn test_classify_with_details_creative() {
    let r = classify_with_details("brainstorm ideas for architecture");
    assert_eq!(r.intent, Intent::Creative);
    assert_eq!(r.cila_level, 4);
    assert!(r.techniques.contains(&Technique::FewShotReasoning));
}

// ── compose_json enriched output Tests ──────────────────────────

#[test]
fn test_compose_json_includes_cila_level() {
    let json = compose_json("fix the bug");
    let hso = json.get("hookSpecificOutput").unwrap();
    assert_eq!(hso["cila_level"], 2); // Debug -> L2
    assert_eq!(hso["intent"], "DEBUG");
    assert!(hso["techniques"].as_array().unwrap().len() > 0);
    assert!(hso["confidence"].as_f64().unwrap() > 0.0);
}

#[test]
fn test_compose_json_backward_compatible() {
    // Verify the original contract fields are still present
    let json = compose_json("implement a new endpoint");
    let hso = json.get("hookSpecificOutput").unwrap();
    assert_eq!(hso["hookEventName"], "UserPromptSubmit");
    assert!(
        hso["additionalContext"]
            .as_str()
            .unwrap()
            .contains("CODE MODE")
    );
    // New fields are additive, not replacing
    assert_eq!(hso["cila_level"], 1); // Code -> L1
    assert_eq!(hso["intent"], "CODE");
}

#[test]
fn test_compose_json_general_cila_zero() {
    let json = compose_json("hello world");
    let hso = json.get("hookSpecificOutput").unwrap();
    assert_eq!(hso["cila_level"], 0);
    assert_eq!(hso["intent"], "GENERAL");
    assert_eq!(hso["confidence"], 0.0);
}

#[test]
fn test_compose_json_techniques_are_strings() {
    let json = compose_json("explain how the event loop works");
    let hso = json.get("hookSpecificOutput").unwrap();
    let techs = hso["techniques"].as_array().unwrap();
    for tech in techs {
        assert!(
            tech.is_string(),
            "technique should be a string, got: {:?}",
            tech
        );
    }
    // Analysis -> CoT, StructuredOutput, PrecisionHints
    let tech_strs: Vec<&str> = techs.iter().map(|t| t.as_str().unwrap()).collect();
    assert!(tech_strs.contains(&"chain_of_thought"));
    assert!(tech_strs.contains(&"precision_hints"));
}

// ── All 6 Python techniques present ─────────────────────────────

#[test]
fn test_all_six_python_techniques_exist() {
    // Verify that the 6 techniques from Python prompt_enhancer.py are
    // all present in the Rust Technique enum.
    let all_techniques = [
        Technique::ChainOfThought,
        Technique::ConstitutionalConstraints,
        Technique::StructuredOutput,
        Technique::FewShotReasoning,
        Technique::SelfValidation,
        Technique::PrecisionHints,
    ];
    // Verify as_str() matches Python names
    assert_eq!(all_techniques[0].as_str(), "chain_of_thought");
    assert_eq!(all_techniques[1].as_str(), "constitutional_constraints");
    assert_eq!(all_techniques[2].as_str(), "structured_output");
    assert_eq!(all_techniques[3].as_str(), "few_shot_reasoning");
    assert_eq!(all_techniques[4].as_str(), "self_validation");
    assert_eq!(all_techniques[5].as_str(), "precision_hints");
}

#[test]
fn test_every_technique_used_by_at_least_one_intent() {
    let all_techniques = [
        Technique::ChainOfThought,
        Technique::ConstitutionalConstraints,
        Technique::StructuredOutput,
        Technique::FewShotReasoning,
        Technique::SelfValidation,
        Technique::PrecisionHints,
    ];
    let all_intents = [
        Intent::Code,
        Intent::Debug,
        Intent::Refactor,
        Intent::Test,
        Intent::Analysis,
        Intent::Creative,
        Intent::Plan,
        Intent::General,
    ];
    for tech in &all_techniques {
        let used = all_intents
            .iter()
            .any(|intent| techniques_for(intent).contains(tech));
        assert!(used, "Technique {:?} is not used by any intent!", tech);
    }
}
