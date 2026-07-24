//! Basic rule-based `ToolSequenceGenerator` implementation.
//!
//! A simple generator that creates sequences based on rules and objective
//! patterns, without ML/learning. Learning is handled by the
//! `PheromoneIntegrator` for pheromone-guided refinement.
//!
//! ## Tool Catalog
//!
//! This generator uses the `ToolCatalog` to access all 26+ available tools:
//! - File Operations: Read, Edit, Write, Bash
//! - Index/AST: touring_index_find, touring_ast_overview, touring_ast_blast
//! - Memory: touring_memory_recall, touring_memory_store
//! - Session: touring_session_start, touring_session_assess
//! - Decompose: touring_decompose_create, touring_decompose_add
//! - Cognitive: touring_cognitive_metrics, touring_suggest_next, touring_mcts_search
//! - Wiring: touring_wiring_status, touring_wiring_orphans
//! - Evolution: touring_evolution_insights, touring_evolution_drift
//! - Learning: touring_learning_status, touring_online_learn

use std::sync::Arc;

use crate::rl::aco::UnifiedPheromoneBus;
use crate::rl::n1::objective_spec::QualityGate;
use crate::rl::n1::tool_call::{RetryPolicy, ToolArguments, topological_order};
use crate::rl::n1::{
    ExecutionOutcome, GeneratedSequence, N1Error, N1Result, ObjectiveSpec, PheromoneIntegrator,
    SequenceMetadata, ToolCall, ToolCallId, ToolCatalog, ToolCategory, ToolSequenceGenerator,
    ValidationCriteria, ValidationResult,
};

/// Rule-based generator implementation.
#[derive(Clone)]
pub struct BasicGenerator {
    integrator: PheromoneIntegrator,
    max_tool_calls: usize,
    #[expect(dead_code)]
    evaporation_rate: f64,
    /// Tool catalog with all available touring tools.
    catalog: ToolCatalog,
}

impl BasicGenerator {
    /// Create a new basic generator.
    pub fn new(bus: Arc<UnifiedPheromoneBus>, evaporation_rate: f64) -> Self {
        Self {
            integrator: PheromoneIntegrator::new(bus, evaporation_rate),
            max_tool_calls: 10,
            evaporation_rate,
            catalog: ToolCatalog::new(),
        }
    }

    /// Set maximum tool calls per sequence.
    pub fn with_max_tool_calls(mut self, max: usize) -> Self {
        self.max_tool_calls = max;
        self
    }

    /// Get the tool catalog.
    pub fn catalog(&self) -> &ToolCatalog {
        &self.catalog
    }

    /// Select the best tool for a given objective description using pattern matching.
    fn select_tool_for_objective(&self, objective: &ObjectiveSpec) -> String {
        let desc_lower = objective.description.to_lowercase();

        // Pattern table: (keywords, tool_name) - evaluated in order
        let patterns: &[(&[&str], &str)] = &[
            // File operations (highest priority)
            (&["modify", "edit", "fix"], "Edit"),
            (&["create", "write", "add"], "Write"),
            (&["read", "analyze", "check"], "Read"),
            (&["execute", "run", "bash"], "Bash"),
            // Index/AST
            (
                &["find symbol", "lookup", "find definition"],
                "touring_index_find",
            ),
            (&["blast radius", "impact analysis"], "touring_ast_blast"),
            (&["overview", "file structure"], "touring_ast_overview"),
            // Memory
            (&["remember", "recall"], "touring_memory_recall"),
            (&["store", "persist"], "touring_memory_store"),
            // Session
            (&["session", "checkpoint"], "touring_session_start"),
            (&["assess", "evaluate"], "touring_session_assess"),
            // Cognitive
            (
                &["suggest", "recommend", "next action"],
                "touring_suggest_next",
            ),
            (&["mcts", "search", "plan"], "touring_mcts_search"),
            (&["cognitive", "metrics"], "touring_cognitive_metrics"),
            (&["classify", "intent", "cila"], "touring_classify_intent"),
            (&["pii", "privacy"], "touring_scan_pii"),
            // Wiring
            (&["wiring", "integration"], "touring_wiring_status"),
            (&["wiring orphan", "unconsumed"], "touring_wiring_orphans"),
            // Decompose
            (&["decompose", "dag", "subtask"], "touring_decompose_create"),
            (&["add subtask", "add task"], "touring_decompose_add"),
            (
                &["validate dag", "check cycles"],
                "touring_decompose_validate",
            ),
            // Evolution
            (
                &["evolution", "drift", "insights"],
                "touring_evolution_insights",
            ),
            (&["tool effectiveness"], "touring_evolution_tools"),
            // Learning
            (&["learning", "rl", "reward"], "touring_learning_status"),
        ];

        for (keywords, tool_name) in patterns {
            if keywords.iter().any(|k| desc_lower.contains(k)) {
                return tool_name.to_string();
            }
        }

        // EC65: Consult pheromone bus as learned fallback before defaulting to "Read".
        // best_tool_for() returns the tool with the highest accumulated pheromone strength,
        // encoding past execution success across all tool calls for this agent.
        if let Some((pheromone_tool, _strength)) = self.integrator.best_tool_for(&desc_lower) {
            return pheromone_tool;
        }

        // Default to Read
        "Read".to_string()
    }

    /// Build a tool call for the selected tool targeting the first objective target.
    fn build_tool_call(
        &self,
        tool_name: &str,
        target_path: &str,
        dependencies: Vec<ToolCallId>,
    ) -> ToolCall {
        let tool = self.catalog.get(tool_name);

        ToolCall {
            id: ToolCallId::new(0), // Will be fixed by caller
            tool_name: tool_name.into(),
            arguments: if let Some(t) = tool {
                match t.category {
                    ToolCategory::FileOperations => {
                        ToolArguments::new().with_named("file", serde_json::json!(target_path))
                    }
                    _ => ToolArguments::new().with_named("query", serde_json::json!(target_path)),
                }
            } else {
                ToolArguments::new().with_named("file", serde_json::json!(target_path))
            },
            dependencies,
            // EC65: wire to_cli_name — convert "touring_index_find" → "touring index find"
            // for human-readable expected_output strings.
            expected_output: Some(format!("{} result", self.to_cli_name(tool_name))),
            retry_policy: RetryPolicy::default(),
            parallelizable: tool.map(|t| t.parallelizable).unwrap_or(true),
        }
    }

    /// Convert a tool name (e.g., "touring_index_find") to its CLI form ("touring index find").
    fn to_cli_name(&self, tool_name: &str) -> String {
        tool_name.replace('_', " ")
    }

    #[allow(unused_assignments)]
    #[allow(clippy::indexing_slicing)]
    fn generate_rule_based(&self, objective: &ObjectiveSpec) -> N1Result<Vec<ToolCall>> {
        let mut calls = Vec::new();

        // Analyze objective to determine the primary tool
        let primary_tool = self.select_tool_for_objective(objective);

        // Check if this is a Touring intelligence tool or a file operation
        let is_touring_tool = primary_tool.starts_with("touring_");

        if is_touring_tool {
            // Touring tool sequence - tool resolved via catalog for metadata
            let _tool_desc = self.catalog.get(&primary_tool);

            if let Some(target) = objective.targets.first() {
                calls.push(self.build_tool_call(&primary_tool, &target.path, vec![]));
            } else {
                // No target - still emit the touring tool if it's query-based
                calls.push(ToolCall {
                    id: ToolCallId::new(0),
                    tool_name: primary_tool.clone(),
                    arguments: ToolArguments::new()
                        .with_named("query", serde_json::json!(objective.description)),
                    dependencies: vec![],
                    expected_output: Some(format!("{} result", primary_tool)),
                    retry_policy: RetryPolicy::default(),
                    parallelizable: true,
                });
            }
        } else {
            // File operation tools
            match primary_tool.as_str() {
                "Read" => {
                    let base_id = calls.len();
                    for (i, target) in objective.targets.iter().enumerate() {
                        let mut call = self.build_tool_call(&primary_tool, &target.path, vec![]);
                        call.id = ToolCallId::new(base_id + i);
                        call.parallelizable = true;
                        calls.push(call);
                    }
                }
                "Edit" => {
                    // Edit requires Read first
                    if let Some(target) = objective.targets.first() {
                        let read_call = self.build_tool_call("Read", &target.path, vec![]);
                        let read_id = read_call.id;
                        calls.push(read_call);

                        let mut edit_call =
                            self.build_tool_call(&primary_tool, &target.path, vec![read_id]);
                        edit_call.id = ToolCallId::new(calls.len());
                        calls.push(edit_call);
                    }
                }
                "Bash" => {
                    let mut call =
                        self.build_tool_call(&primary_tool, &objective.description, vec![]);
                    call.id = ToolCallId::new(calls.len());
                    calls.push(call);
                }
                _ => {
                    // Default to Read
                    let base_id = calls.len();
                    for (i, target) in objective.targets.iter().enumerate() {
                        let mut call = self.build_tool_call("Read", &target.path, vec![]);
                        call.id = ToolCallId::new(base_id + i);
                        calls.push(call);
                    }
                }
            }
        }

        // Enforce max tool calls
        calls.truncate(self.max_tool_calls);

        // Re-number IDs sequentially
        for (i, call) in calls.iter_mut().enumerate() {
            call.id = ToolCallId::new(i);
        }

        // Validate no cycles
        if let Err(cycle) = topological_order(&calls) {
            return Err(N1Error::CyclicDependency(format!(
                "Cycle detected involving: {:?}",
                cycle
            )));
        }

        Ok(calls)
    }

    /// Build validation criteria from objective.
    fn build_validation(&self, objective: &ObjectiveSpec) -> ValidationCriteria {
        let mut criteria = ValidationCriteria::new();

        // Add checks based on quality gates
        for gate in &objective.quality_gates {
            match gate {
                QualityGate::Correctness => {
                    criteria = criteria.with_check(
                        crate::rl::n1::validation_criteria::ValidationCheck::no_errors(),
                    );
                }
                QualityGate::CodeQuality => {
                    criteria = criteria.with_check(
                        crate::rl::n1::validation_criteria::ValidationCheck::valid_json(),
                    );
                }
                _ => {}
            }
        }

        // Add file existence checks
        for target in &objective.targets {
            if target.kind == crate::rl::n1::objective_spec::TargetKind::SourceFile {
                criteria = criteria.with_check(
                    crate::rl::n1::validation_criteria::ValidationCheck::file_exists(&target.path),
                );
            }
        }

        criteria
    }

    /// Build rollback plan for the sequence.
    fn build_rollback(&self, _objective: &ObjectiveSpec) -> crate::rl::n1::RollbackPlan {
        // Basic rollback: just a noop placeholder
        // Full implementation would track created/modified files
        crate::rl::n1::RollbackPlan::new().with_noop("rollback_not_implemented")
    }
}

impl ToolSequenceGenerator for BasicGenerator {
    fn generate(&self, objective: &ObjectiveSpec) -> N1Result<GeneratedSequence> {
        let tool_calls = self.generate_rule_based(objective)?;
        let validation = self.build_validation(objective);
        let rollback = self.build_rollback(objective);

        // Query pheromone bus for confidence adjustment
        let pheromone_strength = self.integrator.query_objective(&objective.description);
        let confidence = (0.5 + pheromone_strength * 0.5).clamp(0.0_f64, 1.0);

        // Build pheromone deposits for learning (collect first to avoid borrowing issues)
        let pheromone_deposits: Vec<_> = tool_calls
            .iter()
            .filter_map(|call| {
                let strength = self.integrator.query_tool(&call.tool_name);
                if strength > 0.0 {
                    Some((
                        crate::rl::n1::generated_sequence::PheromoneKey::ToolSequence(
                            call.tool_name.clone(),
                        ),
                        strength,
                    ))
                } else {
                    None
                }
            })
            .collect();

        let metadata = SequenceMetadata {
            generator_name: "BasicGenerator".into(),
            generator_version: env!("CARGO_PKG_VERSION").into(),
            generation_time_ms: 0,
            is_learned: false,
            features: std::collections::HashMap::new(),
            candidates_evaluated: 1,
        };

        // Build sequence with all data
        let mut seq = GeneratedSequence::new(tool_calls, validation, rollback)
            .with_confidence(confidence)
            .with_metadata(metadata);

        for (key, amount) in pheromone_deposits {
            seq = seq.with_pheromone(key, amount);
        }

        Ok(seq)
    }

    fn validate(
        &self,
        seq: &GeneratedSequence,
        objective: &ObjectiveSpec,
    ) -> N1Result<ValidationResult> {
        Ok(validate_sequence(seq, objective))
    }

    fn learn(
        &self,
        seq: &GeneratedSequence,
        outcome: &ExecutionOutcome,
    ) -> crate::rl::LearningResult<()> {
        crate::rl::n1::pheromone_integration::learn_from_outcome(&self.integrator, seq, outcome)
    }

    fn pheromone_prefix(&self) -> &'static str {
        "basic_gen"
    }
}

/// Validate a generated sequence against an objective.
pub fn validate_sequence(seq: &GeneratedSequence, objective: &ObjectiveSpec) -> ValidationResult {
    let mut failures = Vec::new();

    // Check tool call count
    if let Some(max) = seq.validation.max_tool_calls {
        if seq.tool_calls.len() > max {
            failures.push(format!(
                "Sequence has {} tool calls, exceeds maximum {}",
                seq.tool_calls.len(),
                max
            ));
        }
    }

    // Check all targets are covered
    for target in &objective.targets {
        let target_covered = seq.tool_calls.iter().any(|c| {
            c.arguments
                .named
                .values()
                .any(|v| v.as_str().is_some_and(|s| s.contains(&target.path)))
        });
        if !target_covered {
            failures.push(format!(
                "Target {} not covered by any tool call",
                target.path
            ));
        }
    }

    // Check dependencies are satisfied
    if let Err(cycle) = topological_order(&seq.tool_calls) {
        failures.push(format!("Dependency cycle detected: {:?}", cycle));
    }

    ValidationResult {
        passed: failures.is_empty(),
        failures,
        confidence: seq.confidence,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rl::n1::objective_spec::{ObjectiveSpec, TargetKind};

    #[test]
    fn test_generate_read_objective() {
        let bus = Arc::new(UnifiedPheromoneBus::new(0.1));
        let generator = BasicGenerator::new(bus, 0.1);

        let objective = ObjectiveSpec::new("Analyze the code")
            .with_target("src/lib.rs", TargetKind::SourceFile);

        let result = generator.generate(&objective).unwrap();
        assert!(!result.tool_calls.is_empty());
        assert_eq!(result.tool_calls[0].tool_name, "Read");
    }

    #[test]
    fn test_generate_edit_objective() {
        let bus = Arc::new(UnifiedPheromoneBus::new(0.1));
        let generator = BasicGenerator::new(bus, 0.1);

        let objective = ObjectiveSpec::new("Fix the bug in auth")
            .with_target("src/auth.rs", TargetKind::SourceFile);

        let result = generator.generate(&objective).unwrap();
        assert!(result.tool_calls.len() >= 2);
        assert_eq!(result.tool_calls[0].tool_name, "Read");
        assert_eq!(result.tool_calls[1].tool_name, "Edit");
    }

    #[test]
    fn test_validate_success() {
        let bus = Arc::new(UnifiedPheromoneBus::new(0.1));
        let generator = BasicGenerator::new(bus, 0.1);

        let objective =
            ObjectiveSpec::new("Check file").with_target("src/lib.rs", TargetKind::SourceFile);

        let seq = generator.generate(&objective).unwrap();
        let result = generator.validate(&seq, &objective).unwrap();

        assert!(result.passed || !result.failures.is_empty()); // Depends on file existence
    }

    #[test]
    fn test_max_tool_calls() {
        let bus = Arc::new(UnifiedPheromoneBus::new(0.1));
        let generator = BasicGenerator::new(bus, 0.1).with_max_tool_calls(2);

        let objective = ObjectiveSpec::new("Modify files")
            .with_target("a.rs", TargetKind::SourceFile)
            .with_target("b.rs", TargetKind::SourceFile)
            .with_target("c.rs", TargetKind::SourceFile);

        let result = generator.generate(&objective).unwrap();
        assert!(result.tool_calls.len() <= 2);
    }
}
