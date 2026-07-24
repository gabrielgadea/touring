//! `touring classify-intent` — CILA intent classification hook.

use super::read_stdin_safe;
use crate::hooks::IntentClassifier;
use crate::hooks::classifier::CILAResult;

/// Map routing_strategy to a human-readable action verb.
pub(crate) fn action_for_strategy(strategy: &str) -> &'static str {
    match strategy {
        "pipeline" => "analyze",
        "tool_use" => "search",
        "compute" => "compute",
        "agent_loop" | "aco_orchestration" => "orchestrate",
        "team_spawn" => "coordinate",
        "self_evolution" => "evolve",
        _ => "modify",
    }
}

/// Map CILA level to architecture name.
pub(crate) fn arch_for_level(level: u8) -> &'static str {
    match level {
        0 => "direct",
        1 => "PAL",
        2 => "ReAct",
        3 => "pipeline",
        4 => "Reflexion",
        5 => "ESAA",
        _ => "multi-agent",
    }
}

/// Build the constraints list from a classification result.
pub(crate) fn build_constraints(result: &CILAResult) -> Vec<&'static str> {
    let mut constraints = Vec::new();
    if result.requires_code_first {
        constraints.push("GLOBAL: CODE-FIRST UNIVERSAL — DISCOVER → CREATE → EXECUTE");
    }
    if result.requires_pipeline {
        constraints.push("ANTT: pipeline_state required before analysis skills");
    }
    constraints
}

/// Map routing_strategy to a success-criteria description.
pub(crate) fn success_for_strategy(strategy: &str) -> &'static str {
    match strategy {
        "pipeline" => "Pipeline executado + state salvo + output verificado",
        "tool_use" => "Informação encontrada + fonte verificada",
        "compute" => "Resultado calculado + validado",
        "agent_loop" => "Análise completa + validada + checkpointed",
        "aco_orchestration" => "Ciclo ACO completo + GoalTracker PASS",
        "team_spawn" => "Team criado + tarefas delegadas + resultados consolidados",
        "self_evolution" => "Mutação aplicada + validada + persistida",
        "direct" => "Resposta direta entregue",
        _ => "Mudança aplicada + ruff 0 + comportamento esperado verificado",
    }
}

/// Build the full JSON output from a classification result.
pub(crate) fn build_output(result: &CILAResult) -> serde_json::Value {
    let techniques = IntentClassifier::get_techniques(result.level);
    let technique_hints: Vec<&str> = techniques.iter().map(|t| t.hint()).collect();

    let action = action_for_strategy(&result.routing_strategy);
    let arch = arch_for_level(result.level);
    let constraints = build_constraints(result);
    let success = success_for_strategy(&result.routing_strategy);

    let context = format!(
        "↳ [perception] action={action} | arch={arch} | compiled={strategy}\n\
         ↳ [constraints] {constraints}\n\
         ↳ [success] {success}",
        strategy = if result.level > 0 {
            &result.routing_strategy
        } else {
            "fallback"
        },
        constraints = if constraints.is_empty() {
            "none".to_string()
        } else {
            constraints.join(" | ")
        },
    );

    serde_json::json!({
        "hookSpecificOutput": {
            "hookEventName": "UserPromptSubmit",
            "additionalContext": context,
        },
        "suppressOutput": true,
        "metadata": {
            "cila_level": result.level,
            "cila_name": result.level_name,
            "routing_strategy": result.routing_strategy,
            "requires_pipeline": result.requires_pipeline,
            "requires_code_first": result.requires_code_first,
            "techniques": technique_hints,
        }
    })
}

/// Entry point for the `touring classify-intent` hook — reads the user prompt
/// from the stdin JSON `prompt` field, runs the CILA `IntentClassifier`, and
/// prints the `UserPromptSubmit` hook output (perception context + metadata) as JSON.
pub fn run() -> anyhow::Result<()> {
    let data = read_stdin_safe();

    let prompt = data.get("prompt").and_then(|v| v.as_str()).unwrap_or("");

    let classifier = IntentClassifier::new();
    let result = classifier.classify(prompt);
    let output = build_output(&result);

    println!("{}", serde_json::to_string(&output)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_mapping_covers_all_strategies() {
        assert_eq!(action_for_strategy("pipeline"), "analyze");
        assert_eq!(action_for_strategy("tool_use"), "search");
        assert_eq!(action_for_strategy("compute"), "compute");
        assert_eq!(action_for_strategy("agent_loop"), "orchestrate");
        assert_eq!(action_for_strategy("aco_orchestration"), "orchestrate");
        assert_eq!(action_for_strategy("team_spawn"), "coordinate");
        assert_eq!(action_for_strategy("self_evolution"), "evolve");
        assert_eq!(action_for_strategy("direct"), "modify");
        assert_eq!(action_for_strategy("unknown_xyz"), "modify");
    }

    #[test]
    fn arch_mapping_covers_all_levels() {
        assert_eq!(arch_for_level(0), "direct");
        assert_eq!(arch_for_level(1), "PAL");
        assert_eq!(arch_for_level(2), "ReAct");
        assert_eq!(arch_for_level(3), "pipeline");
        assert_eq!(arch_for_level(4), "Reflexion");
        assert_eq!(arch_for_level(5), "ESAA");
        assert_eq!(arch_for_level(6), "multi-agent");
        assert_eq!(arch_for_level(255), "multi-agent");
    }

    #[test]
    fn success_mapping_covers_all_strategies() {
        assert!(success_for_strategy("pipeline").contains("Pipeline"));
        assert!(success_for_strategy("tool_use").contains("encontrada"));
        assert!(success_for_strategy("compute").contains("calculado"));
        assert!(success_for_strategy("agent_loop").contains("checkpointed"));
        assert!(success_for_strategy("aco_orchestration").contains("GoalTracker"));
        assert!(success_for_strategy("team_spawn").contains("delegadas"));
        assert!(success_for_strategy("self_evolution").contains("persistida"));
        assert!(success_for_strategy("direct").contains("direta"));
        assert!(success_for_strategy("anything_else").contains("ruff"));
    }

    #[test]
    fn build_output_has_correct_json_structure() {
        let classifier = IntentClassifier::new();
        let result = classifier.classify("hello");
        let output = build_output(&result);

        // Top-level keys
        assert!(output.get("hookSpecificOutput").is_some());
        assert!(output.get("suppressOutput").is_some());
        assert!(output.get("metadata").is_some());

        // hookSpecificOutput structure
        let hook = &output["hookSpecificOutput"];
        assert_eq!(hook["hookEventName"], "UserPromptSubmit");
        assert!(
            hook["additionalContext"]
                .as_str()
                .unwrap()
                .contains("[perception]")
        );

        // metadata structure
        let meta = &output["metadata"];
        assert!(meta.get("cila_level").is_some());
        assert!(meta.get("cila_name").is_some());
        assert!(meta.get("routing_strategy").is_some());
        assert!(meta.get("requires_pipeline").is_some());
        assert!(meta.get("requires_code_first").is_some());
        assert!(meta.get("techniques").is_some());
        assert!(meta["techniques"].as_array().is_some());
    }

    #[test]
    fn build_output_l0_uses_fallback_strategy() {
        let classifier = IntentClassifier::new();
        // Empty prompt should classify as L0 direct
        let result = classifier.classify("");
        assert_eq!(result.level, 0);
        let output = build_output(&result);
        let ctx = output["hookSpecificOutput"]["additionalContext"]
            .as_str()
            .unwrap();
        assert!(
            ctx.contains("compiled=fallback"),
            "L0 should show compiled=fallback, got: {ctx}"
        );
    }

    #[test]
    fn build_constraints_empty_when_not_required() {
        let result = CILAResult {
            level: 0,
            level_name: "L0-Direct".into(),
            routing_strategy: "direct".into(),
            requires_pipeline: false,
            requires_code_first: false,
            matched_pattern: String::new(),
        };
        assert!(build_constraints(&result).is_empty());
    }

    #[test]
    fn build_constraints_includes_both_when_required() {
        let result = CILAResult {
            level: 3,
            level_name: "L3-Pipeline".into(),
            routing_strategy: "pipeline".into(),
            requires_pipeline: true,
            requires_code_first: true,
            matched_pattern: "test".into(),
        };
        let constraints = build_constraints(&result);
        assert_eq!(constraints.len(), 2);
        assert!(constraints[0].contains("CODE-FIRST"));
        assert!(constraints[1].contains("pipeline_state"));
    }
}
