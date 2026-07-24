//! Pheromone integration for N1 sequence generation.
//!
//! Bridges the N1 `ToolSequenceGenerator` with the ACO `UnifiedPheromoneBus`
//! for learning from execution outcomes.

use std::sync::Arc;

use crate::rl::aco::{PheroKey, UnifiedPheromoneBus};
use crate::rl::n1::{ExecutionOutcome, GeneratedSequence};

/// Pheromone integrator for N1.
///
/// Reads pheromone trails to guide generation and deposits rewards
/// after successful/failed executions.
#[derive(Clone)]
pub struct PheromoneIntegrator {
    bus: Arc<UnifiedPheromoneBus>,
    #[expect(dead_code)]
    evaporation_rate: f64,
}

impl PheromoneIntegrator {
    /// Create a new integrator backed by the shared pheromone bus.
    pub fn new(bus: Arc<UnifiedPheromoneBus>, evaporation_rate: f64) -> Self {
        Self {
            bus,
            evaporation_rate,
        }
    }

    /// Query pheromone strength for a tool sequence pattern.
    pub fn query_sequence(&self, pattern: &str) -> f64 {
        self.bus.get(&PheroKey::TaskId(format!("seq:{}", pattern)))
    }

    /// Query pheromone strength for an objective pattern.
    pub fn query_objective(&self, pattern: &str) -> f64 {
        self.bus.get(&PheroKey::TaskId(format!("obj:{}", pattern)))
    }

    /// Query pheromone for a specific tool call.
    pub fn query_tool(&self, tool_name: &str) -> f64 {
        self.bus
            .get(&PheroKey::TaskId(format!("tool:{}", tool_name)))
    }

    /// Deposit pheromone reward after successful execution.
    pub fn deposit_success(&self, seq: &GeneratedSequence, quality_score: f64) {
        // Deposit for each tool in sequence (recency-weighted)
        for (idx, call) in seq.tool_calls.iter().enumerate() {
            let recency_weight = 1.0 - (idx as f64 / seq.tool_calls.len() as f64);
            let amount = quality_score * recency_weight * 0.1;
            self.bus
                .deposit(PheroKey::TaskId(format!("tool:{}", call.tool_name)), amount);
        }

        // Deposit for sequence pattern
        let sequence_hash = compute_sequence_hash(seq);
        self.bus.deposit(
            PheroKey::TaskId(format!("seq:{}", sequence_hash)),
            quality_score * 0.5,
        );

        // Evaporate old trails
        self.bus.evaporate_all();
    }

    /// Deposit pheromone penalty after failed execution.
    pub fn deposit_failure(&self, seq: &GeneratedSequence, failed_at: usize) {
        // Penalize the failing tool call heavily
        if let Some(call) = seq.tool_calls.get(failed_at) {
            self.bus
                .deposit(PheroKey::TaskId(format!("tool:{}", call.tool_name)), -0.2);
        }

        // Record failure pattern
        let sequence_hash = compute_sequence_hash(seq);
        self.bus
            .deposit(PheroKey::TaskId(format!("fail:{}", sequence_hash)), 0.1);
    }

    /// Get top-k most promising tool sequences for a given context.
    pub fn top_sequences(&self, context: &str, k: usize) -> Vec<(String, f64)> {
        self.bus
            .top_k(k)
            .into_iter()
            .filter_map(|(key, strength)| match key {
                PheroKey::TaskId(id) if id.starts_with("seq:") => Some((
                    id.strip_prefix("seq:")
                        .expect("guarded by starts_with(\"seq:\")")
                        .to_string(),
                    strength,
                )),
                _ => None,
            })
            .filter(|(seq, _)| seq.contains(context))
            .collect()
    }

    /// Get the best tool for a given task type.
    pub fn best_tool_for(&self, _task_type: &str) -> Option<(String, f64)> {
        self.bus
            .top_k(20)
            .into_iter()
            .filter_map(|(key, strength)| match key {
                PheroKey::TaskId(id) if id.starts_with("tool:") => Some((
                    id.strip_prefix("tool:")
                        .expect("guarded by starts_with(\"tool:\")")
                        .to_string(),
                    strength,
                )),
                _ => None,
            })
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
    }

    /// Evaporate all trails (call periodically).
    pub fn evaporate(&self) {
        self.bus.evaporate_all();
    }

    /// Return the current bus for sharing with other components.
    pub fn bus(&self) -> &Arc<UnifiedPheromoneBus> {
        &self.bus
    }
}

/// Compute a hash of the sequence for pheromone keying.
fn compute_sequence_hash(seq: &GeneratedSequence) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    for call in &seq.tool_calls {
        call.tool_name.hash(&mut hasher);
    }
    format!("{:x}", hasher.finish())
}

/// Apply pheromone learning from an execution outcome.
pub fn learn_from_outcome(
    integrator: &PheromoneIntegrator,
    seq: &GeneratedSequence,
    outcome: &ExecutionOutcome,
) -> crate::rl::LearningResult<()> {
    if outcome.success {
        integrator.deposit_success(seq, outcome.quality_score);
    } else if let Some(failed_at) = outcome.first_failure_index {
        integrator.deposit_failure(seq, failed_at);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rl::n1::{
        rollback_plan::RollbackPlan,
        tool_call::{RetryPolicy, ToolArguments, ToolCall, ToolCallId},
        validation_criteria::ValidationCriteria,
    };

    fn make_test_sequence() -> GeneratedSequence {
        let calls = vec![
            ToolCall {
                id: ToolCallId::new(0),
                tool_name: "read".into(),
                arguments: ToolArguments::new(),
                dependencies: vec![],
                expected_output: None,
                retry_policy: RetryPolicy::default(),
                parallelizable: true,
            },
            ToolCall {
                id: ToolCallId::new(1),
                tool_name: "edit".into(),
                arguments: ToolArguments::new(),
                dependencies: vec![ToolCallId::new(0)],
                expected_output: None,
                retry_policy: RetryPolicy::default(),
                parallelizable: false,
            },
        ];
        GeneratedSequence::new(calls, ValidationCriteria::new(), RollbackPlan::new())
    }

    #[test]
    fn test_deposit_and_query() {
        let bus = Arc::new(UnifiedPheromoneBus::new(0.1));
        let integrator = PheromoneIntegrator::new(bus.clone(), 0.1);

        let seq = make_test_sequence();
        let outcome = ExecutionOutcome {
            success: true,
            executed: vec![],
            first_failure_index: None,
            quality_score: 0.9,
            error_message: None,
        };

        learn_from_outcome(&integrator, &seq, &outcome).unwrap();

        // Should have deposited for tools and sequence
        assert!(bus.get(&PheroKey::TaskId("tool:read".into())) > 0.0);
        assert!(bus.get(&PheroKey::TaskId("tool:edit".into())) > 0.0);
    }

    #[test]
    fn test_failure_deposit() {
        let bus = Arc::new(UnifiedPheromoneBus::new(0.0));
        let integrator = PheromoneIntegrator::new(bus.clone(), 0.0);

        let seq = make_test_sequence();
        let outcome = ExecutionOutcome {
            success: false,
            executed: vec![],
            first_failure_index: Some(1),
            quality_score: 0.0,
            error_message: Some("edit failed".into()),
        };

        learn_from_outcome(&integrator, &seq, &outcome).unwrap();

        // Should have negative deposit for failed tool
        assert!(bus.get(&PheroKey::TaskId("tool:edit".into())) < 0.0);
    }
}
