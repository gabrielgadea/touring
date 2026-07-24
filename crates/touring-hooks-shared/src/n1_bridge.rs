//! N1 Bridge — HookRuntime invokes N1 ToolSequenceGenerator.
//!
//! Bridges HookRuntime (N0) with the N1 ToolSequenceGenerator layer.
//! For CILA L4+ (complex multi-step tasks), hooks can delegate to N1
//! to generate a tool sequence guided by ACO pheromone trails.
//!
//! # Architecture
//!
//! ```text
//! HookRuntime (N0)
//!    │  invoke_n1_sequence(objective)
//!    ▼
//! N1Bridge ──► BasicGenerator + PheromoneIntegrator
//!    │                    │
//!    │                    ├──► UnifiedPheromoneBus (ACO wiring)
//!    │                    │
//!    │                    └──► ToolSequenceGenerator trait
//!    │
//!    ▼
//! GeneratedSequence (TRIAD: execute + validate + rollback)
//! ```

use std::sync::Arc;
use touring_intelligence::rl::aco::UnifiedPheromoneBus;
use touring_intelligence::rl::n1::objective_spec::{QualityGate, TargetKind};
use touring_intelligence::rl::n1::{
    BasicGenerator, ExecutionOutcome, GeneratedSequence, ObjectiveSpec, ToolSequenceGenerator,
};

/// Bridge from HookRuntime (N0) to N1 ToolSequenceGenerator.
///
/// Lazily initialized — call `init()` before first use, or let it auto-initialize
/// on first invocation.
#[derive(Clone)]
pub struct N1Bridge {
    generator: BasicGenerator,
}

impl N1Bridge {
    /// Initialize the N1 bridge with a shared pheromone bus.
    pub fn new(bus: Arc<UnifiedPheromoneBus>) -> Self {
        Self {
            generator: BasicGenerator::new(bus, 0.1),
        }
    }

    /// Generate a tool sequence for an objective (CILA L4+).
    ///
    /// Returns `None` if generation fails or if the objective is too simple
    /// (CILA L0-L3 should use direct hook logic, not N1).
    pub fn generate_sequence(&self, objective: &ObjectiveSpec) -> Option<GeneratedSequence> {
        match self.generator.generate(objective) {
            Ok(seq) => Some(seq),
            Err(e) => {
                tracing::warn!(error = %e, "N1 sequence generation failed");
                None
            }
        }
    }

    /// Generate a sequence only if CILA level is L4+.
    ///
    /// This is the main entry point for hooks — they check CILA level first,
    /// and only invoke N1 for complex (L4+) tasks.
    pub fn generate_if_complex(
        &self,
        objective: &ObjectiveSpec,
        cila_level: u8,
    ) -> Option<GeneratedSequence> {
        if cila_level >= 4 {
            self.generate_sequence(objective)
        } else {
            None
        }
    }

    /// Validate a generated sequence against the original objective.
    pub fn validate(
        &self,
        seq: &GeneratedSequence,
        objective: &ObjectiveSpec,
    ) -> Option<touring_intelligence::rl::n1::ValidationResult> {
        self.generator.validate(seq, objective).ok()
    }

    /// Learn from execution outcome, updating pheromone trails.
    ///
    /// Call this after a sequence is executed (success or failure).
    pub fn learn(&self, seq: &GeneratedSequence, outcome: &ExecutionOutcome) {
        if let Err(e) = self.generator.learn(seq, outcome) {
            tracing::warn!(error = %e, "N1 learning failed");
        }
    }
}

// ── Builder helpers for ObjectiveSpec ─────────────────────────────────────────

impl N1Bridge {
    /// Build an ObjectiveSpec from hook context (file path + intent description).
    pub fn objective_from_hook(
        description: &str,
        file_path: &str,
        is_complex: bool,
    ) -> ObjectiveSpec {
        let mut spec = ObjectiveSpec::new(description);

        // Add target file if provided
        if !file_path.is_empty() {
            spec = spec.with_target(file_path, TargetKind::SourceFile);
        }

        // Add quality gates based on complexity
        if is_complex {
            spec = spec
                .with_quality_gate(QualityGate::Correctness)
                .with_quality_gate(QualityGate::CodeQuality)
                .with_quality_gate(QualityGate::Specification);
        } else {
            spec = spec.with_quality_gate(QualityGate::Correctness);
        }

        // Set priority based on complexity
        spec = spec.with_priority(if is_complex { 4 } else { 2 });

        spec
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn test_bridge_init() {
        let bus = Arc::new(UnifiedPheromoneBus::new(0.1));
        let bridge = N1Bridge::new(bus);
        assert!(bridge.generator.pheromone_prefix().starts_with("basic_gen"));
    }

    #[test]
    fn test_generate_sequence() {
        let bus = Arc::new(UnifiedPheromoneBus::new(0.1));
        let bridge = N1Bridge::new(bus);

        let objective = ObjectiveSpec::new("Analyze the code")
            .with_target("src/lib.rs", TargetKind::SourceFile);

        let seq = bridge.generate_sequence(&objective);
        assert!(seq.is_some());
        let seq = seq.unwrap();
        assert!(!seq.tool_calls.is_empty());
    }

    #[test]
    fn test_generate_if_complex_respects_level() {
        let bus = Arc::new(UnifiedPheromoneBus::new(0.1));
        let bridge = N1Bridge::new(bus);

        let objective = ObjectiveSpec::new("Analyze the code")
            .with_target("src/lib.rs", TargetKind::SourceFile);

        // L3 should return None
        assert!(bridge.generate_if_complex(&objective, 3).is_none());

        // L4 should return Some
        assert!(bridge.generate_if_complex(&objective, 4).is_some());

        // L0 should return None
        assert!(bridge.generate_if_complex(&objective, 0).is_none());
    }

    #[test]
    fn test_objective_from_hook_simple() {
        let spec = N1Bridge::objective_from_hook("Fix the bug", "src/auth.rs", false);
        assert_eq!(spec.description, "Fix the bug");
        assert_eq!(spec.targets.len(), 1);
        assert_eq!(spec.quality_gates.len(), 1); // Correctness only
        assert_eq!(spec.priority, 2);
    }

    #[test]
    fn test_objective_from_hook_complex() {
        let spec = N1Bridge::objective_from_hook("Refactor module", "src/mod.rs", true);
        assert_eq!(spec.quality_gates.len(), 3); // Correctness + CodeQuality + Specification
        assert_eq!(spec.priority, 4);
    }
}
