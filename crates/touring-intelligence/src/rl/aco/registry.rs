//! PhaseRegistry — dynamic plugin/phase registry with insertion-order preservation.
//!
//! INS-3 (ROI=1.50): Decouples phases from core graph. New phases registered at runtime
//! without modifying graph.rs. Uses IndexMap for insertion-order-stable iteration.
//! INS-L7: pheromone_priority — dynamic reordering of phases by accumulated pheromone score.

use std::collections::HashMap;

use indexmap::IndexMap;

use super::graph::{GraphError, MutableGeneratorGraph};

/// Trait implemented by any phase handler that can be registered.
pub trait PhaseHandler: Send + Sync {
    /// Unique identifier for this phase.
    fn phase_id(&self) -> &str;

    /// Execute this phase against the graph.
    fn execute(&self, graph: &mut MutableGeneratorGraph) -> Result<(), GraphError>;

    /// Roll back the effects of this phase.
    fn rollback(&self, graph: &mut MutableGeneratorGraph) -> Result<(), GraphError>;
}

/// Dynamic registry of phase handlers, preserving insertion order.
/// INS-L7: Supports pheromone-guided dynamic reordering via `get_ordered_phases`.
pub struct PhaseRegistry {
    phases: IndexMap<String, Box<dyn PhaseHandler>>,
    /// INS-L7: accumulated pheromone priority per phase_id.
    pheromone_priority: HashMap<String, f64>,
}

impl Default for PhaseRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl PhaseRegistry {
    /// Create a new empty registry.
    pub fn new() -> Self {
        Self {
            phases: IndexMap::new(),
            pheromone_priority: HashMap::new(),
        }
    }

    /// INS-L7: Set an explicit priority score for a phase.
    pub fn set_priority(&mut self, phase_id: &str, priority: f64) {
        self.pheromone_priority
            .insert(phase_id.to_string(), priority);
    }

    /// INS-L7: Accumulate additional pheromone for a phase (positive reinforcement).
    pub fn deposit_priority(&mut self, phase_id: &str, delta: f64) {
        *self
            .pheromone_priority
            .entry(phase_id.to_string())
            .or_insert(0.0) += delta;
    }

    /// INS-L7: Return phase IDs sorted by pheromone priority descending.
    ///
    /// Phases without an explicit priority are treated as 0.0 and appear last
    /// (ordered among themselves by insertion order).
    pub fn get_ordered_phases(&self) -> Vec<&str> {
        let mut ids: Vec<&str> = self.phases.keys().map(String::as_str).collect();
        ids.sort_by(|a, b| {
            let pa = self.pheromone_priority.get(*a).copied().unwrap_or(0.0);
            let pb = self.pheromone_priority.get(*b).copied().unwrap_or(0.0);
            pb.partial_cmp(&pa).unwrap_or(std::cmp::Ordering::Equal)
        });
        ids
    }

    /// Register a phase handler. Returns Err if phase_id already registered.
    pub fn register<H: PhaseHandler + 'static>(&mut self, handler: H) -> Result<(), GraphError> {
        let id = handler.phase_id().to_string();
        if self.phases.contains_key(&id) {
            return Err(GraphError::DuplicateNode(id));
        }
        self.phases.insert(id, Box::new(handler));
        Ok(())
    }

    /// Execute the named phase against the graph.
    pub fn execute_phase(
        &self,
        id: &str,
        graph: &mut MutableGeneratorGraph,
    ) -> Result<(), GraphError> {
        let handler = self
            .phases
            .get(id)
            .ok_or_else(|| GraphError::NodeNotFound(id.to_string()))?;
        handler.execute(graph)
    }

    /// Roll back the named phase against the graph.
    pub fn rollback_phase(
        &self,
        id: &str,
        graph: &mut MutableGeneratorGraph,
    ) -> Result<(), GraphError> {
        let handler = self
            .phases
            .get(id)
            .ok_or_else(|| GraphError::NodeNotFound(id.to_string()))?;
        handler.rollback(graph)
    }

    /// Phase IDs in insertion order.
    pub fn phase_ids(&self) -> Vec<&str> {
        self.phases.keys().map(|s| s.as_str()).collect()
    }

    /// Number of registered phases.
    pub fn len(&self) -> usize {
        self.phases.len()
    }

    /// True if no phases registered.
    pub fn is_empty(&self) -> bool {
        self.phases.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rl::aco::models::{
        ExecutionStatus, GeneratorContract, GeneratorNode, GeneratorOutputs, GeneratorType,
    };

    // --- Test PhaseHandler implementations ---

    struct NoopPhase {
        id: String,
    }

    impl NoopPhase {
        fn new(id: &str) -> Self {
            Self { id: id.to_string() }
        }
    }

    impl PhaseHandler for NoopPhase {
        fn phase_id(&self) -> &str {
            &self.id
        }
        fn execute(&self, _graph: &mut MutableGeneratorGraph) -> Result<(), GraphError> {
            Ok(())
        }
        fn rollback(&self, _graph: &mut MutableGeneratorGraph) -> Result<(), GraphError> {
            Ok(())
        }
    }

    /// Phase that transitions a node Pending → Running on execute,
    /// and Running → ExecutionFailed on rollback (compensating action).
    struct StartPhase {
        id: String,
        node_id: String,
    }

    impl PhaseHandler for StartPhase {
        fn phase_id(&self) -> &str {
            &self.id
        }
        fn execute(&self, graph: &mut MutableGeneratorGraph) -> Result<(), GraphError> {
            // Valid transition: Pending → Running
            graph.transition_status(&self.node_id, ExecutionStatus::Running)
        }
        fn rollback(&self, graph: &mut MutableGeneratorGraph) -> Result<(), GraphError> {
            // Compensate: Running → ExecutionFailed (undo the start)
            graph.transition_status(&self.node_id, ExecutionStatus::ExecutionFailed)
        }
    }

    fn make_node(id: &str) -> GeneratorNode {
        GeneratorNode {
            id: id.to_string(),
            description: format!("test node {id}"),
            generator_type: GeneratorType::Template,
            inputs_data: vec![],
            inputs_templates: vec![],
            inputs_constraints: vec![],
            outputs: GeneratorOutputs {
                execution_script: String::new(),
                validation_script: String::new(),
                rollback_script: String::new(),
            },
            contract: GeneratorContract {
                precondition: String::new(),
                postcondition: String::new(),
                invariant: String::new(),
            },
            acceptance_criteria: String::new(),
            depends_on: vec![],
        }
    }

    #[test]
    fn test_registry_register_and_execute() {
        let mut registry = PhaseRegistry::new();
        registry
            .register(NoopPhase::new("phase-a"))
            .expect("register ok");
        assert_eq!(registry.len(), 1);
        let mut graph = MutableGeneratorGraph::new();
        assert!(registry.execute_phase("phase-a", &mut graph).is_ok());
    }

    #[test]
    fn test_registry_duplicate_registration_fails() {
        let mut registry = PhaseRegistry::new();
        registry
            .register(NoopPhase::new("phase-a"))
            .expect("first ok");
        let result = registry.register(NoopPhase::new("phase-a"));
        assert!(result.is_err(), "duplicate must fail");
    }

    #[test]
    fn test_registry_execute_unknown_phase_fails() {
        let registry = PhaseRegistry::new();
        let mut graph = MutableGeneratorGraph::new();
        let result = registry.execute_phase("unknown", &mut graph);
        assert!(result.is_err());
    }

    #[test]
    fn test_registry_rollback_phase() {
        let mut registry = PhaseRegistry::new();
        let mut graph = MutableGeneratorGraph::new();
        graph.add_node(make_node("n1")).expect("add ok");

        registry
            .register(StartPhase {
                id: "start".to_string(),
                node_id: "n1".to_string(),
            })
            .expect("register ok");

        // Execute: Pending → Running
        registry
            .execute_phase("start", &mut graph)
            .expect("execute ok");
        assert_eq!(
            graph.execution_status("n1"),
            Some(&ExecutionStatus::Running)
        );

        // Rollback: Running → ExecutionFailed (compensating transition)
        registry
            .rollback_phase("start", &mut graph)
            .expect("rollback ok");
        assert_eq!(
            graph.execution_status("n1"),
            Some(&ExecutionStatus::ExecutionFailed)
        );
    }

    #[test]
    fn test_registry_phase_ids_in_insertion_order() {
        let mut registry = PhaseRegistry::new();
        for id in &["alpha", "beta", "gamma", "delta"] {
            registry.register(NoopPhase::new(id)).expect("register ok");
        }
        let ids = registry.phase_ids();
        assert_eq!(ids, vec!["alpha", "beta", "gamma", "delta"]);
    }

    #[test]
    fn test_registry_is_empty() {
        let registry = PhaseRegistry::new();
        assert!(registry.is_empty());
    }

    #[test]
    fn test_registry_rollback_unknown_phase_fails() {
        let registry = PhaseRegistry::new();
        let mut graph = MutableGeneratorGraph::new();
        let result = registry.rollback_phase("unknown", &mut graph);
        assert!(result.is_err());
    }

    #[test]
    fn test_registry_multiple_phases_execute_in_order() {
        let mut registry = PhaseRegistry::new();
        let mut graph = MutableGeneratorGraph::new();
        graph.add_node(make_node("node-a")).expect("add ok");
        graph.add_node(make_node("node-b")).expect("add ok");

        registry
            .register(StartPhase {
                id: "start-a".to_string(),
                node_id: "node-a".to_string(),
            })
            .expect("register a");
        registry
            .register(StartPhase {
                id: "start-b".to_string(),
                node_id: "node-b".to_string(),
            })
            .expect("register b");

        registry
            .execute_phase("start-a", &mut graph)
            .expect("exec a");
        registry
            .execute_phase("start-b", &mut graph)
            .expect("exec b");

        assert_eq!(
            graph.execution_status("node-a"),
            Some(&ExecutionStatus::Running)
        );
        assert_eq!(
            graph.execution_status("node-b"),
            Some(&ExecutionStatus::Running)
        );
    }

    #[test]
    fn test_registry_len_after_multiple_registrations() {
        let mut registry = PhaseRegistry::new();
        assert_eq!(registry.len(), 0);
        registry.register(NoopPhase::new("p1")).expect("ok");
        assert_eq!(registry.len(), 1);
        registry.register(NoopPhase::new("p2")).expect("ok");
        assert_eq!(registry.len(), 2);
    }
}
