#![allow(clippy::indexing_slicing)]
use super::*;
use touring_foundation::truncate_str;

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
                truncate_str(&prompt, 60),
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
    let hso = compose_details("fix the bug");
    assert_eq!(hso["cila_level"], 2); // Debug -> L2
    assert_eq!(hso["intent"], "DEBUG");
    assert!(hso["techniques"].as_array().unwrap().len() > 0);
    assert!(hso["confidence"].as_f64().unwrap() > 0.0);
}

#[test]
fn test_compose_json_backward_compatible() {
    // The wire keeps the contract fields; the classification lives in compose_details (S27).
    let json = compose_json("implement a new endpoint");
    let hso = json.get("hookSpecificOutput").unwrap();
    assert_eq!(hso["hookEventName"], "UserPromptSubmit");
    assert!(
        hso["additionalContext"]
            .as_str()
            .unwrap()
            .contains("CODE MODE")
    );
    let details = compose_details("implement a new endpoint");
    assert_eq!(details["cila_level"], 1); // Code -> L1
    assert_eq!(details["intent"], "CODE");
}

#[test]
fn test_compose_json_general_cila_zero() {
    let hso = compose_details("hello world");
    assert_eq!(hso["cila_level"], 0);
    assert_eq!(hso["intent"], "GENERAL");
    assert_eq!(hso["confidence"], 0.0);
}

#[test]
fn test_compose_json_techniques_are_strings() {
    let hso = compose_details("explain how the event loop works");
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

// ── G0: machine payloads carry no intent to classify ──────────────────────────
//
// Evidence (session 2026-09-22): agent task-notifications and Stop-hook feedback
// reached this classifier and were enhanced as TEST, CODE and DEBUG. A payload the
// user never typed has no intent, so the classification is noise in every turn that
// a background job reports back. The predicate is deterministic on purpose: this is
// the "use code when you can" rule, not a judgement.

/// One sample per declared prefix, so the guard below can prove coverage.
const AUTOMATED_SAMPLES: &[(&str, &str)] = &[
    (
        "<task-notification>",
        "<task-notification>\n<task-id>bk00lqb8h</task-id>\n<status>completed</status>\n<summary>Agent \"Digerir cookbooks\" finished; o digest cobre os 18 cookbooks e um teste falhou</summary>\n</task-notification>",
    ),
    (
        "[SYSTEM NOTIFICATION",
        "[SYSTEM NOTIFICATION - NOT USER INPUT]\nThis is an automated background-task event, NOT a message from the user.",
    ),
    (
        "Stop hook feedback",
        "Stop hook feedback:\nLoop Engineering: not converged (3/30). Unmet clauses: ['dag_done']. Next action: execute pending subtask(s): R3",
    ),
    (
        "<system-reminder>",
        "<system-reminder>\nAs you answer, remember to refactor the test suite.\n</system-reminder>",
    ),
    (
        "<cross-session-message",
        "<cross-session-message from=\"worker\">implemente o fix e rode os testes</cross-session-message>",
    ),
    (
        "Another Claude session sent a message",
        "Another Claude session sent a message: <agent-message from=\"gate-fonte-ausente\">Defects D1 e D2 corrigidos: 28/28 testes passando, ruff limpo</agent-message>",
    ),
    (
        "<agent-message",
        "<agent-message from=\"worker\">terminei o refactor, rode os testes</agent-message>",
    ),
    (
        "[Cross-session idle notice]",
        "[Cross-session idle notice] a sessão 'analise' ficou ociosa",
    ),
    (
        "[Cross-session delivery notice]",
        "[Cross-session delivery notice] a mensagem aguarda aprovação do usuário",
    ),
    (
        "[Artifact comment sent to Claude]",
        "[Artifact comment sent to Claude] ;thread=abc123 — corrija o gráfico da página",
    ),
    (
        "<bash-input>",
        "<bash-input>cargo test -p touring-hook-runtime</bash-input>",
    ),
    (
        "<bash-stdout>",
        "<bash-stdout>test result: FAILED. 12 passed; 1 failed</bash-stdout>",
    ),
    (
        "<local-command-stdout>",
        "<local-command-stdout>error: test failed, to rerun pass `-p touring-cli`</local-command-stdout>",
    ),
    (
        "Caveat: The messages below were generated by the user while running local commands",
        "Caveat: The messages below were generated by the user while running local commands. DO NOT respond to these messages or otherwise consider them in your response unless the user explicitly asks you to.",
    ),
];

#[test]
fn test_automated_payload_is_detected_for_every_declared_prefix() {
    for (prefix, sample) in AUTOMATED_SAMPLES {
        assert!(
            is_automated_payload(sample),
            "prefix {prefix:?} must be detected as an automated payload"
        );
    }
}

#[test]
fn test_automated_payload_tolerates_leading_whitespace() {
    assert!(is_automated_payload(
        "\n  <task-notification>\nAgent finished"
    ));
}

#[test]
fn test_human_prompts_are_never_automated_payloads() {
    for prompt in [
        "fix the KeyError on line 42 of parser.py",
        "corrija o erro de conexão no módulo de banco de dados",
        "Publique",
        "explique como o event loop funciona",
        // A slash command carries the user's own words: it stays classifiable.
        "<command-name>/loop-engineering</command-name>\n<command-args>faça uma análise completa</command-args>",
    ] {
        assert!(
            !is_automated_payload(prompt),
            "human prompt must stay classifiable: {prompt:?}"
        );
    }
}

#[test]
fn test_compose_json_skips_enhancement_for_automated_payloads() {
    for (prefix, sample) in AUTOMATED_SAMPLES {
        let json = compose_json(sample);
        let out = &json["hookSpecificOutput"];
        assert_eq!(
            out["hookEventName"], "UserPromptSubmit",
            "the hook contract still names the event ({prefix})"
        );
        assert!(
            out.get("additionalContext").is_none(),
            "{prefix}: an automated payload must not receive injected context, got {:?}",
            out.get("additionalContext")
        );
        assert!(
            out.get("intent").is_none() && out.get("cila_level").is_none(),
            "{prefix}: no intent and no CILA level for a payload the user never typed"
        );
        assert!(
            compose_details(sample).is_null(),
            "{prefix}: nothing was classified (the skip reason goes to the trace, S27)"
        );
        assert_eq!(json["suppressOutput"], true, "{prefix}: nothing to show");
    }
}

#[test]
fn test_compose_json_still_enhances_human_prompts() {
    let json = compose_json("fix the KeyError on line 42 of parser.py");
    let out = &json["hookSpecificOutput"];
    let details = compose_details("fix the KeyError on line 42 of parser.py");
    assert_eq!(details["intent"], "DEBUG");
    assert_eq!(details["cila_level"], 2);
    assert!(
        out["additionalContext"]
            .as_str()
            .is_some_and(|c| c.contains("DEBUG MODE")),
        "a real prompt keeps its enhancement"
    );
    assert!(out.get("skipped").is_none());
}

/// D8 cross-guard: both entry points must consult the predicate, and every prefix in
/// the executor's list must have a sample above. A declaration the executor does not
/// apply — or a prefix no test exercises — is exactly how this class of defect returns.
#[test]
fn test_both_entry_points_consult_the_automated_payload_predicate() {
    let src = include_str!("prompt_enhance.rs");
    for entry in ["pub fn compose_json", "pub fn run_user_prompt_submit"] {
        let body_start = src
            .find(entry)
            .unwrap_or_else(|| panic!("{entry} not found"));
        let body = &src[body_start..];
        let body_end = body.find("\n}\n").unwrap_or(body.len());
        assert!(
            body[..body_end].contains("is_automated_payload("),
            "{entry} must consult is_automated_payload before classifying"
        );
    }

    let declared: Vec<&str> = AUTOMATED_PAYLOAD_PREFIXES.to_vec();
    assert!(!declared.is_empty(), "the prefix list must not be empty");
    for prefix in &declared {
        assert!(
            AUTOMATED_SAMPLES.iter().any(|(p, _)| p == prefix),
            "prefix {prefix:?} is declared in the executor but no test sample exercises it"
        );
    }
    for (prefix, _) in AUTOMATED_SAMPLES {
        assert!(
            declared.contains(prefix),
            "sample prefix {prefix:?} is not declared in AUTOMATED_PAYLOAD_PREFIXES"
        );
    }
}

// ── S27 (plan 2026-09-25): the event's contract and the density invariant ──────

const S27_PROMPTS: &[&str] = &[
    "fix the bug",
    "implement a REST endpoint",
    "write tests for the parser module",
    "plan the migration of the indexer to tantivy",
    "refactor AuthValidator in src/auth.rs",
    "explain how the event loop works",
    "hello world",
];

/// A `<name>` in a command line is a placeholder unless it is an output-structure tag,
/// recognised by its closing `</name>` on the same line.
fn s27_placeholders(ctx: &str) -> Vec<String> {
    let mut found = Vec::new();
    for line in ctx
        .lines()
        .filter(|l| l.contains("touring ") || l.contains("cargo "))
    {
        let mut rest = line;
        while let Some(i) = rest.find('<') {
            let tail = &rest[i + 1..];
            let Some(j) = tail.find('>') else { break };
            let name = &tail[..j];
            let is_word =
                !name.is_empty() && name.chars().all(|c| c.is_ascii_lowercase() || c == '_');
            if is_word && !line.contains(&format!("</{name}>")) {
                found.push(format!("<{name}> in: {}", line.trim()));
            }
            rest = &tail[j + 1..];
        }
    }
    found
}

#[test]
fn prompt_enhance_output_matches_userpromptsubmit_schema() {
    for prompt in S27_PROMPTS {
        let json = compose_json(prompt);
        let hso = json["hookSpecificOutput"]
            .as_object()
            .expect("hookSpecificOutput");
        let mut keys: Vec<&str> = hso.keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert!(
            keys == ["additionalContext", "hookEventName"] || keys == ["hookEventName"],
            "{prompt}: UserPromptSubmit accepts additionalContext/sessionTitle/suppressOriginalPrompt only, got {keys:?}"
        );
    }
    for (_, sample) in AUTOMATED_SAMPLES {
        let json = compose_json(sample);
        let hso = json["hookSpecificOutput"]
            .as_object()
            .expect("hookSpecificOutput");
        assert!(
            hso.keys().all(|k| k == "hookEventName"),
            "automated payload leaks {hso:?}"
        );
    }
}

#[test]
fn prompt_enhance_emits_no_placeholder_tokens() {
    for prompt in S27_PROMPTS {
        let json = compose_json(prompt);
        let ctx = json["hookSpecificOutput"]["additionalContext"]
            .as_str()
            .unwrap_or("");
        let found = s27_placeholders(ctx);
        assert!(
            found.is_empty(),
            "{prompt}: placeholders shipped to the model: {found:#?}"
        );
    }
}

#[test]
fn prompt_enhance_derives_values_from_the_prompt() {
    let json = compose_json("refactor AuthValidator in src/auth.rs");
    let ctx = json["hookSpecificOutput"]["additionalContext"]
        .as_str()
        .unwrap_or("");
    assert!(
        ctx.contains("src/auth.rs") || ctx.contains("AuthValidator"),
        "the derivable values must travel: {ctx}"
    );
}

// Mutex de serialização dos testes que tocam envs process-wide
// (TOURING_INTENT_JOURNAL / LAYA_STUDENT_URL / TOURING_LAYA_INTENT): a lição
// deste arquivo aplicada à família inteira de vars — o S-2 (shadow) virou o
// terceiro tocador e a guerra foi medida 3/3 na suíte (01/10).
pub(crate) static JOURNAL_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

// S-3.2 (plan 2026-09-30, W3) — decision journal: prova de emissão, skip marcado,
// corrida de writers e fail-open. UM único teste cobre tudo em sequência porque
// TOURING_INTENT_JOURNAL é process-wide e os testes rodam em threads paralelas
// (a versão em dois testes perdeu o arquivo: um removeu a var do outro — medido).
#[test]
fn journal_written_for_human_skip_for_machine_concurrent_and_never_breaks_reply() {
    let _guard = JOURNAL_ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::env::temp_dir().join(format!("intent-journal-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("tmp journal dir");
    let file = dir.join("journal.jsonl");
    let _ = std::fs::remove_file(&file);
    // SAFETY: esta var só é lida pelo emissor de journal; nenhum outro teste a toca,
    // e linhas de terceiros no arquivo não afetam as asserções (`.any()`).
    unsafe { std::env::set_var("TOURING_INTENT_JOURNAL", &file) };

    compose_json("refactor AuthValidator in src/auth.rs");
    compose_json("<task-notification>{\"status\":\"completed\"}</task-notification>");

    // Corrida: 8 threads × 50 prompts humanos — a linha nunca intercala
    // (regressão medida em produção: 4/84 linhas com dois JSONs concatenados).
    std::thread::scope(|s| {
        for t in 0..8 {
            s.spawn(move || {
                for i in 0..50 {
                    compose_json(&format!("explique a decisão {t}/{i} do daemon"));
                }
            });
        }
    });

    let text = std::fs::read_to_string(&file).expect("journal written");
    let mut parsed = 0usize;
    for (n, line) in text.lines().enumerate() {
        serde_json::from_str::<serde_json::Value>(line)
            .unwrap_or_else(|e| panic!("line {n} is not valid JSON: {e}: {line:?}"));
        parsed += 1;
    }
    // Outros testes paralelos também escrevem neste arquivo (a var é process-wide):
    // o piso exato são as 402 daqui; o que não pode existir é linha corrompida.
    assert!(
        parsed >= 402,
        "2 linhas iniciais + 400 da corrida: {parsed}"
    );
    let lines: Vec<serde_json::Value> = text
        .lines()
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect();
    assert!(
        lines.iter().any(|l| l["skipped_automated"] == false
            && l["source"] == "keyword"
            && l["cila_level"].is_u64()),
        "human prompt must journal a classification"
    );
    assert!(
        lines.iter().any(|l| l["skipped_automated"] == true),
        "machine payload must journal the skip"
    );

    // fail-open: journal impossível não pode derrubar a resposta do hook
    unsafe {
        std::env::set_var(
            "TOURING_INTENT_JOURNAL",
            "/proc/impossible/deeply/journal.jsonl",
        )
    };
    let json = compose_json("explique o daemon");
    assert!(json["hookSpecificOutput"]["additionalContext"].is_string());
    unsafe { std::env::remove_var("TOURING_INTENT_JOURNAL") };
}

// S-2/S-3 (plan r2) — shadow-matrix: o aluno SEMPRE é perguntado; claim real só
// com flag ON + ≥τ; toda resposta não-claimada vira linha `laya_shadow` com
// latência, e a decisão nunca muda. Sob o JOURNAL_ENV_LOCK (envs process-wide).
#[test]
fn student_shadow_matrix_journals_would_be_answers_without_changing_decisions() {
    let _guard = JOURNAL_ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    // Ordem fixa JOURNAL → STUDENT: bloqueia classificadores paralelos até as
    // envs serem restauradas; daqui em diante só chamadas `_inner`.
    let _sguard = STUDENT_ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let journal = std::env::temp_dir().join(format!("shadow-journal-{}", std::process::id()));
    let _ = std::fs::remove_file(&journal);
    // SAFETY: sob o JOURNAL_ENV_LOCK; restaura ao final.
    unsafe {
        std::env::set_var("TOURING_INTENT_JOURNAL", &journal);
        std::env::remove_var("TOURING_LAYA_INTENT");
    }

    // (a) abaixo do τ (0,4) com a flag ON: sem claim, linha shadow com latência
    let low = spawn_student_stub(
        r#"{"answers":{"intent":{"type":"choice","choice":"debug","probabilities":{"debug":0.4,"code":0.3}}}}"#,
    );
    unsafe {
        std::env::set_var("LAYA_STUDENT_URL", &low);
        std::env::set_var("TOURING_LAYA_INTENT", "1");
    }
    assert!(
        student_classify_inner("prompt a").is_none(),
        "0.4 < τ — nunca claim"
    );
    let lines = std::fs::read_to_string(&journal).unwrap_or_default();
    let shadow = lines
        .lines()
        .find(|l| l.contains("\"laya_shadow\"") && l.contains("\"DEBUG\""))
        .unwrap_or_else(|| panic!("linha shadow esperada, veio: {lines}"));
    assert!(
        shadow.contains("\"latency_ms\""),
        "S-3: a shadow carrega a latência: {shadow}"
    );

    // (b) flag OFF (shadow mode): mesmo com 0.9 NÃO clama — e mesmo assim grava
    let mid = spawn_student_stub(
        r#"{"answers":{"intent":{"type":"choice","choice":"plan","probabilities":{"plan":0.9}}}}"#,
    );
    unsafe {
        std::env::remove_var("TOURING_LAYA_INTENT");
        std::env::set_var("LAYA_STUDENT_URL", &mid);
    }
    assert!(
        student_classify_inner("prompt b").is_none(),
        "flag OFF — shadow, não claim"
    );
    let lines2 = std::fs::read_to_string(&journal).unwrap_or_default();
    assert!(
        lines2
            .lines()
            .any(|l| l.contains("\"laya_shadow\"") && l.contains("\"PLAN\"")),
        "shadow-mode esperado com a flag OFF"
    );

    // (c) servidor fora: fail-open, NENHUMA linha shadow nova
    unsafe { std::env::set_var("LAYA_STUDENT_URL", "http://127.0.0.1:1") };
    let before = std::fs::read_to_string(&journal)
        .unwrap_or_default()
        .lines()
        .count();
    assert!(student_classify_inner("prompt c").is_none());
    let after = std::fs::read_to_string(&journal)
        .unwrap_or_default()
        .lines()
        .count();
    assert_eq!(before, after, "server down → nenhuma linha shadow");

    // (d) flag ON + resposta ≥ τ: claim REAL, que não vira shadow
    let high = spawn_student_stub(
        r#"{"answers":{"intent":{"type":"choice","choice":"code","probabilities":{"code":0.9}}}}"#,
    );
    unsafe {
        std::env::set_var("LAYA_STUDENT_URL", &high);
        std::env::set_var("TOURING_LAYA_INTENT", "1");
    }
    let claimed = student_classify_inner("prompt d");
    assert!(matches!(claimed, Some((Intent::Code, p)) if p >= STUDENT_TAU));
    let lines3 = std::fs::read_to_string(&journal).unwrap_or_default();
    assert!(
        !lines3
            .lines()
            .any(|l| l.contains("\"laya_shadow\"") && l.contains("\"CODE\"")),
        "claim real nunca vira shadow"
    );

    // SAFETY: restauração do ambiente do processo de teste.
    unsafe {
        std::env::remove_var("TOURING_LAYA_INTENT");
        std::env::remove_var("LAYA_STUDENT_URL");
        std::env::remove_var("TOURING_INTENT_JOURNAL");
    }
}

// S-3 (plan r2) — contrato de latência do journal: TODA linha, decision ou
// shadow, carrega `latency_ms` inteiro ≥ 0 — é o que a régua KPI lê. `as_u64`
// reprova ausência e negativo de uma vez: o assert é o tipo, não comparação.
#[test]
fn journal_line_carries_latency_ms_non_negative() {
    let _guard = JOURNAL_ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    // Ordem fixa JOURNAL → STUDENT (ver student_classify_inner).
    let _sguard = STUDENT_ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let journal = std::env::temp_dir().join(format!("latency-journal-{}", std::process::id()));
    let _ = std::fs::remove_file(&journal);
    // SAFETY: sob o JOURNAL_ENV_LOCK; restaura ao final.
    unsafe {
        std::env::set_var("TOURING_INTENT_JOURNAL", &journal);
        std::env::remove_var("TOURING_LAYA_INTENT");
    }

    // Linha decision (keyword path): journal_intent_decision é o emissor da
    // linha — compose_json chamaria student_classify (locked), deadlock sob o
    // STUDENT_ENV_LOCK que este teste segura.
    let details = classify_with_details_inner("explique o daemon");
    journal_intent_decision(&details, "compose_json", 0);
    // …e linha shadow (flag OFF: o aluno responde e vira shadow, nunca claim).
    let stub = spawn_student_stub(
        r#"{"answers":{"intent":{"type":"choice","choice":"plan","probabilities":{"plan":0.9}}}}"#,
    );
    unsafe { std::env::set_var("LAYA_STUDENT_URL", &stub) };
    assert!(
        student_classify_inner("prompt latencia").is_none(),
        "flag OFF — shadow"
    );

    let text = std::fs::read_to_string(&journal).expect("journal escrito");
    let lines: Vec<serde_json::Value> = text
        .lines()
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect();
    let decision = lines
        .iter()
        .find(|v| v["skipped_automated"] == false && v["source"] != "laya_shadow")
        .expect("linha decision");
    let shadow = lines
        .iter()
        .find(|v| v["source"] == "laya_shadow")
        .expect("linha shadow");
    for (kind, line) in [("decision", decision), ("shadow", shadow)] {
        line["latency_ms"]
            .as_u64()
            .unwrap_or_else(|| panic!("{kind}: latency_ms ausente ou não-u64: {line}"));
    }

    // SAFETY: restauração do ambiente do processo de teste.
    unsafe {
        std::env::remove_var("LAYA_STUDENT_URL");
        std::env::remove_var("TOURING_INTENT_JOURNAL");
    }
}

/// One-shot stub server: accepts ONE connection, answers `response_json`, exits.
#[cfg(test)]
fn spawn_student_stub(response_json: &str) -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind stub");
    let addr = listener.local_addr().expect("addr");
    let body = response_json.to_string();
    std::thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            use std::io::{Read, Write};
            let mut chunk = [0u8; 8192];
            let _ = stream.read(&mut chunk);
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = stream.write_all(resp.as_bytes());
        }
    });
    format!("http://{addr}")
}

// S-8 (plan r2) — semantic bash gate: o DESCONHECIDO recebe Score do aluno;
// advisory sempre, None em flag OFF/falha; conhecidos nunca pagam a chamada.
#[test]
fn semantic_bash_gate_scores_unknown_and_skips_rest() {
    let _guard = JOURNAL_ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    // Ordem fixa JOURNAL → STUDENT: o stub one-shot não pode ser consumido por
    // um classify paralelo (ver student_classify_inner).
    let _sguard = STUDENT_ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    assert!(is_known_program("ls") && is_known_program("cargo") && is_known_program("rm"));
    assert!(!is_known_program("zdbt") && !is_known_program("find-de"));

    // (a) flag ON + stub score 2,0 (read-only): Some((2,0, _))
    let stub = spawn_student_stub(
        r#"{"answers":{"reversibility":{"type":"score","score":2.0,"probabilities":{"0":0.0,"1":0.1,"2":0.9},"confidence":0.9}}}"#,
    );
    unsafe {
        std::env::set_var("LAYA_STUDENT_URL", &stub);
        std::env::set_var("TOURING_CEG_SEMANTIC_GATE", "1");
    }
    let got = semantic_reversibility("zdbt --compress /var/data");
    assert!(
        matches!(got, Some((s, _)) if (s - 2.0).abs() < 1e-9),
        "score viajou: {got:?}"
    );

    // (b) flag OFF: None sem chamar
    unsafe { std::env::remove_var("TOURING_CEG_SEMANTIC_GATE") };
    assert!(semantic_reversibility("zdbt --compress /var/data").is_none());

    // (c) flag ON + servidor fora: fail-open None
    unsafe {
        std::env::set_var("TOURING_CEG_SEMANTIC_GATE", "1");
        std::env::set_var("LAYA_STUDENT_URL", "http://127.0.0.1:1");
    }
    assert!(semantic_reversibility("zdbt --compress /var/data").is_none());

    unsafe {
        std::env::remove_var("TOURING_CEG_SEMANTIC_GATE");
        std::env::remove_var("LAYA_STUDENT_URL");
    }
}

// W4.4 — student path (TOURING_LAYA_INTENT): decide acima do τ, keyword abaixo,
// nunca chama com a flag off, nunca quebra com o servidor fora. O servidor-falso
// só responde alto-τ para o prompt-marcador — qualquer outro prompt recebe
// resposta abaixo do τ, então testes paralelos seguem no caminho keyword
// esperado por eles (a lição das envs process-wide, medida neste arquivo).
#[test]
fn w44_student_decides_above_tau_keyword_everywhere_else() {
    let _guard = JOURNAL_ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    // Ordem fixa JOURNAL → STUDENT (ver student_classify_inner).
    let _sguard = STUDENT_ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    use std::io::{Read, Write};
    const MARKER: &str = "refatore o parser-xyz-w44";
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind efêmero");
    let addr = listener.local_addr().expect("addr");
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut s) = stream else { continue };
            let mut buf = vec![0u8; 16384];
            let n = s.read(&mut buf).unwrap_or(0);
            let req = String::from_utf8_lossy(&buf[..n]).to_string();
            let high = req.contains(MARKER);
            let body = if high {
                r#"{"model":"laya-w42-intent","answers":{"intent":{"type":"choice","choice":"refactor","probabilities":{"refactor":0.9,"code":0.1},"confidence":0.9}},"usage":{}}"#.to_string()
            } else {
                r#"{"model":"laya-w42-intent","answers":{"intent":{"type":"choice","choice":"refactor","probabilities":{"refactor":0.5,"code":0.5},"confidence":0.1}},"usage":{}}"#.to_string()
            };
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = s.write_all(resp.as_bytes());
        }
    });

    // SAFETY: mesma justificativa dos testes de journal — vars só deste processo.
    unsafe { std::env::set_var("LAYA_STUDENT_URL", format!("http://{addr}")) };
    unsafe { std::env::set_var("TOURING_LAYA_INTENT", "1") };

    // 1. acima do τ: o aluno decide, e a fonte é registrada
    let details = classify_with_details_inner(MARKER);
    assert_eq!(details.intent, Intent::Refactor);
    assert_eq!(details.source, "laya");
    assert!(details.confidence >= 0.6);

    // 2. abaixo do τ: keyword (o servidor respondeu, mas baixo)
    let d2 = classify_with_details_inner("explique a arquitetura do daemon");
    assert_eq!(d2.source, "keyword");

    // 3. flag off: o aluno nunca é chamado, mesmo com o servidor no ar
    unsafe { std::env::remove_var("TOURING_LAYA_INTENT") };
    let d3 = classify_with_details_inner(MARKER);
    assert_eq!(d3.source, "keyword");

    // 4. servidor fora: keyword, sem exceção (porta 9 = discard)
    unsafe { std::env::set_var("TOURING_LAYA_INTENT", "1") };
    unsafe { std::env::set_var("LAYA_STUDENT_URL", "http://127.0.0.1:9") };
    let d4 = classify_with_details_inner(MARKER);
    assert_eq!(d4.source, "keyword");

    unsafe { std::env::remove_var("TOURING_LAYA_INTENT") };
    unsafe { std::env::remove_var("LAYA_STUDENT_URL") };
}

// Hardening (2026-10-02, revisão de segurança do release 30.4.90): o endpoint do
// aluno aceita SÓ http:// + loopback — o texto do prompt nunca sai desta máquina.
#[test]
fn student_endpoint_accepts_only_loopback_http() {
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
    assert_eq!(
        student_endpoint("http://127.0.0.1:8971"),
        Some(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 8971))
    );
    assert_eq!(
        student_endpoint("http://localhost:8971"),
        Some(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 8971))
    );
    assert_eq!(
        student_endpoint("http://[::1]:8971"),
        Some(SocketAddr::new(IpAddr::V6(Ipv6Addr::LOCALHOST), 8971))
    );
    // 127.0.0.0/8 inteiro é loopback
    assert!(student_endpoint("http://127.0.1.2:8971").is_some());
    // Recusas: https (downgrade), DNS/remoto, privado-não-loopback, sem porta.
    for url in [
        "https://127.0.0.1:8971",
        "http://example.com:80",
        "http://10.0.0.5:8971",
        "http://203.0.113.1:9",
        "http://127.0.0.1",
    ] {
        assert_eq!(student_endpoint(url), None, "{url} devia ser recusada");
    }
}

// Comportamental: URL remota NEM TENTA conectar — fail-open no keyword, <100 ms.
#[test]
fn remote_student_url_never_connects_and_fails_open_fast() {
    let _guard = JOURNAL_ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    // Ordem fixa JOURNAL → STUDENT (ver student_classify_inner).
    let _sguard = STUDENT_ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    // SAFETY: sob os locks; restaura ao final. 203.0.113.1 é TEST-NET-3 (RFC
    // 5737, não roteável): sem o gate, o connect pagaria o retry do kernel —
    // o <100 ms abaixo prova que a conexão nem é tentada.
    unsafe { std::env::set_var("LAYA_STUDENT_URL", "http://203.0.113.1:9") };
    let started = std::time::Instant::now();
    let got = student_classify_inner("refatore o parser");
    let elapsed = started.elapsed();
    unsafe { std::env::remove_var("LAYA_STUDENT_URL") };
    assert!(got.is_none(), "remoto recusado → keyword path: {got:?}");
    assert!(
        elapsed < std::time::Duration::from_millis(100),
        "o gate recusa antes do connect: {elapsed:?}"
    );
}

// REGRA #0 (01/10): compose_details era órfão pub com zero consumidores —
// a suíte é a superfície de diagnóstico documentada e agora a consome.
#[test]
fn compose_details_surfaces_classification_and_null_for_machine_payload() {
    let d = compose_details("refatore o parser de hooks");
    assert!(d["intent"].is_string(), "diagnóstico carrega o intent: {d}");
    assert!(
        d["cila_level"].is_u64(),
        "diagnóstico carrega o nível CILA: {d}"
    );
    let null = compose_details("<task-notification>{\"status\":\"completed\"}</task-notification>");
    assert!(
        null.is_null(),
        "payload de máquina não tem classificação: {null}"
    );
}
