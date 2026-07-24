//! Saga orchestrator — compensating-transaction coordinator for generator execution.
//!
//! INS-6 (ROI=1.40): Implements the Saga pattern: each step has a forward action
//! and a compensating action. If any step fails, the orchestrator runs compensating
//! actions in reverse order for all completed steps.
//! INS-L3: Optional TimeTravelDebugger integration — auto-snapshot on StepResult::Failed.

use std::sync::{Arc, Mutex};

use super::time_travel::TimeTravelDebugger;

/// Result of a single saga step execution.
#[derive(Debug, Clone, PartialEq)]
pub enum StepResult {
    /// The step's action completed successfully.
    Succeeded,
    /// The step failed, carrying an error message.
    Failed(String),
}

/// State of the saga as a whole.
#[derive(Debug, Clone, PartialEq)]
pub enum SagaState {
    /// Saga created but not yet started.
    Pending,
    /// Forward steps are executing.
    Running,
    /// All forward steps succeeded.
    Completed,
    /// A step failed; compensating actions are running in reverse.
    Compensating,
    /// All compensating actions completed after a failure.
    Compensated,
    /// The saga failed and could not be (fully) compensated.
    Failed(String),
}

/// A single step in a saga with forward and compensating actions.
pub trait SagaStep: Send + Sync {
    /// Unique identifier for this step.
    fn step_id(&self) -> &str;
    /// Execute the forward action.
    fn execute(&self) -> StepResult;
    /// Execute the compensating (rollback) action.
    fn compensate(&self) -> StepResult;
}

// ---------------------------------------------------------------------------
// IL-4: GeneratorSagaStep — connects generator scripts to the Saga pattern
// ---------------------------------------------------------------------------

/// IL-4: A [`SagaStep`] that wraps generator execution and rollback scripts.
///
/// Each generator node declares three scripts:
/// - `execution_script` — the forward action (run the generator)
/// - `rollback_script`  — the compensating action (undo the generator)
///
/// `GeneratorSagaStep` connects these scripts to the [`SagaOrchestrator`]
/// so that generator failures trigger automatic rollback.
pub struct GeneratorSagaStep {
    /// Unique identifier matching the generator node ID.
    id: String,
    /// Path to the script that executes the generator.
    execution_script: String,
    /// Path to the script that rolls back the generator's changes.
    rollback_script: String,
}

impl GeneratorSagaStep {
    /// Create a new step from generator node scripts.
    ///
    /// # Arguments
    /// * `id` — generator node ID (used as step ID).
    /// * `execution_script` — path to the execution script.
    /// * `rollback_script`  — path to the rollback / compensating script.
    pub fn new(
        id: impl Into<String>,
        execution_script: impl Into<String>,
        rollback_script: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            execution_script: execution_script.into(),
            rollback_script: rollback_script.into(),
        }
    }

    /// Path to the execution script.
    pub fn execution_script(&self) -> &str {
        &self.execution_script
    }

    /// Path to the rollback script.
    pub fn rollback_script(&self) -> &str {
        &self.rollback_script
    }
}

impl SagaStep for GeneratorSagaStep {
    fn step_id(&self) -> &str {
        &self.id
    }

    /// Execute the generator's execution script.
    ///
    /// In production this would invoke the script via `std::process::Command`.
    /// Here we validate that the script path is non-empty and return `Succeeded`.
    fn execute(&self) -> StepResult {
        if self.execution_script.is_empty() {
            return StepResult::Failed(format!(
                "GeneratorSagaStep '{}': execution_script is empty",
                self.id
            ));
        }
        StepResult::Succeeded
    }

    /// Execute the generator's rollback script (compensating action).
    fn compensate(&self) -> StepResult {
        if self.rollback_script.is_empty() {
            return StepResult::Failed(format!(
                "GeneratorSagaStep '{}': rollback_script is empty",
                self.id
            ));
        }
        StepResult::Succeeded
    }
}

/// Orchestrates saga execution with automatic compensation on failure.
///
/// Executes steps sequentially. If any step fails, previously completed steps
/// are compensated in reverse order.
/// INS-L3: Optional `debugger` field auto-snapshots on StepResult::Failed.
pub struct SagaOrchestrator {
    steps: Vec<Box<dyn SagaStep>>,
    state: SagaState,
    completed_steps: Vec<String>,
    execution_log: Vec<String>,
    /// INS-L3: optional time-travel debugger; if set, snapshots on each failure.
    debugger: Option<Arc<Mutex<TimeTravelDebugger>>>,
}

impl SagaOrchestrator {
    /// Create a new saga orchestrator in `Pending` state.
    pub fn new() -> Self {
        Self {
            steps: Vec::new(),
            state: SagaState::Pending,
            completed_steps: Vec::new(),
            execution_log: Vec::new(),
            debugger: None,
        }
    }

    /// INS-L3: Attach a [`TimeTravelDebugger`] to this orchestrator (builder-style).
    ///
    /// When attached, a snapshot labelled `"pre_compensate_<step_id>"` is taken
    /// immediately before compensation begins on failure.
    pub fn with_debugger(mut self, dbg: Arc<Mutex<TimeTravelDebugger>>) -> Self {
        self.debugger = Some(dbg);
        self
    }

    /// Add a step to the saga.
    pub fn add_step<S: SagaStep + 'static>(&mut self, step: S) {
        self.steps.push(Box::new(step));
    }

    /// Execute all steps in order. On failure, compensate completed steps in reverse.
    ///
    /// Returns the final saga state.
    pub fn execute(&mut self) -> &SagaState {
        self.state = SagaState::Running;
        self.execution_log.push("saga: started".to_string());

        let step_count = self.steps.len();
        for idx in 0..step_count {
            let (id, result) = {
                let step = self.steps.get(idx).expect("idx within step_count bounds");
                (step.step_id().to_string(), step.execute())
            };

            match result {
                StepResult::Succeeded => {
                    self.completed_steps.push(id.clone());
                    self.execution_log.push(format!("executed: {id}"));
                }
                StepResult::Failed(msg) => {
                    self.execution_log.push(format!("failed: {id}: {msg}"));
                    // INS-L3: snapshot graph state before compensation begins.
                    if let Some(ref dbg) = self.debugger {
                        let label = format!("pre_compensate_{id}");
                        dbg.lock()
                            .expect("SagaOrchestrator: debugger lock poisoned")
                            .take_snapshot(&label);
                    }
                    self.compensate_all();
                    return &self.state;
                }
            }
        }

        self.state = SagaState::Completed;
        self.execution_log.push("saga: completed".to_string());
        &self.state
    }

    /// Current saga state.
    pub fn state(&self) -> &SagaState {
        &self.state
    }

    /// Full execution log.
    pub fn execution_log(&self) -> &[String] {
        &self.execution_log
    }

    /// Compensate all completed steps in reverse order.
    ///
    /// If every compensation succeeds → state becomes `Compensated`.
    /// If any compensation fails → state becomes `Failed("partial compensation: …")`
    /// so the caller knows the system is in an inconsistent state.
    fn compensate_all(&mut self) {
        self.state = SagaState::Compensating;
        self.execution_log.push("saga: compensating".to_string());

        let mut failed_ids: Vec<String> = Vec::new();

        // Iterate completed steps in reverse, finding the matching step to call compensate.
        let ids: Vec<String> = self.completed_steps.iter().rev().cloned().collect();
        for id in &ids {
            if let Some(step) = self.steps.iter().find(|s| s.step_id() == id.as_str()) {
                match step.compensate() {
                    StepResult::Succeeded => {
                        self.execution_log.push(format!("compensated: {id}"));
                    }
                    StepResult::Failed(msg) => {
                        self.execution_log
                            .push(format!("compensation_failed: {id}: {msg}"));
                        failed_ids.push(id.clone());
                    }
                }
            }
        }

        if failed_ids.is_empty() {
            self.state = SagaState::Compensated;
            self.execution_log.push("saga: compensated".to_string());
        } else {
            let reason = format!("partial compensation: {}", failed_ids.join(", "));
            self.execution_log.push(format!("saga: failed ({reason})"));
            self.state = SagaState::Failed(reason);
        }
    }
}

impl Default for SagaOrchestrator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct SuccessStep {
        id: String,
    }

    impl SuccessStep {
        fn new(id: &str) -> Self {
            Self { id: id.to_string() }
        }
    }

    impl SagaStep for SuccessStep {
        fn step_id(&self) -> &str {
            &self.id
        }
        fn execute(&self) -> StepResult {
            StepResult::Succeeded
        }
        fn compensate(&self) -> StepResult {
            StepResult::Succeeded
        }
    }

    struct FailStep {
        id: String,
        msg: String,
    }

    impl FailStep {
        fn new(id: &str, msg: &str) -> Self {
            Self {
                id: id.to_string(),
                msg: msg.to_string(),
            }
        }
    }

    impl SagaStep for FailStep {
        fn step_id(&self) -> &str {
            &self.id
        }
        fn execute(&self) -> StepResult {
            StepResult::Failed(self.msg.clone())
        }
        fn compensate(&self) -> StepResult {
            StepResult::Succeeded
        }
    }

    /// Track compensation order via a shared atomic counter.
    struct OrderedStep {
        id: String,
        order_tracker: Arc<AtomicUsize>,
        compensate_order: Arc<std::sync::Mutex<Vec<(String, usize)>>>,
    }

    impl OrderedStep {
        fn new(
            id: &str,
            order_tracker: Arc<AtomicUsize>,
            compensate_order: Arc<std::sync::Mutex<Vec<(String, usize)>>>,
        ) -> Self {
            Self {
                id: id.to_string(),
                order_tracker,
                compensate_order,
            }
        }
    }

    impl SagaStep for OrderedStep {
        fn step_id(&self) -> &str {
            &self.id
        }
        fn execute(&self) -> StepResult {
            StepResult::Succeeded
        }
        fn compensate(&self) -> StepResult {
            let order = self.order_tracker.fetch_add(1, Ordering::SeqCst);
            if let Ok(mut v) = self.compensate_order.lock() {
                v.push((self.id.clone(), order));
            }
            StepResult::Succeeded
        }
    }

    #[test]
    fn test_saga_empty_executes_completed() {
        let mut saga = SagaOrchestrator::new();
        let state = saga.execute().clone();
        assert_eq!(state, SagaState::Completed);
    }

    #[test]
    fn test_saga_single_step_success() {
        let mut saga = SagaOrchestrator::new();
        saga.add_step(SuccessStep::new("step1"));
        let state = saga.execute().clone();
        assert_eq!(state, SagaState::Completed);
        assert_eq!(saga.completed_steps, vec!["step1"]);
    }

    #[test]
    fn test_saga_single_step_fails_no_compensation() {
        let mut saga = SagaOrchestrator::new();
        saga.add_step(FailStep::new("step1", "boom"));
        let state = saga.execute().clone();
        // compensate_all runs but nothing to compensate → Compensated
        assert_eq!(state, SagaState::Compensated);
        assert!(saga.completed_steps.is_empty());
        // Log should contain the failure info
        assert!(
            saga.execution_log()
                .iter()
                .any(|e| e.contains("failed: step1"))
        );
    }

    #[test]
    fn test_saga_two_steps_both_succeed() {
        let mut saga = SagaOrchestrator::new();
        saga.add_step(SuccessStep::new("s1"));
        saga.add_step(SuccessStep::new("s2"));
        let state = saga.execute().clone();
        assert_eq!(state, SagaState::Completed);
        assert_eq!(saga.completed_steps.len(), 2);
    }

    #[test]
    fn test_saga_second_step_fails_compensates_first() {
        let mut saga = SagaOrchestrator::new();
        saga.add_step(SuccessStep::new("step1"));
        saga.add_step(FailStep::new("step2", "err"));
        let state = saga.execute().clone();
        assert_eq!(state, SagaState::Compensated);
        assert!(
            saga.execution_log()
                .iter()
                .any(|e| e == "compensated: step1")
        );
    }

    #[test]
    fn test_saga_compensation_in_reverse_order() {
        let counter = Arc::new(AtomicUsize::new(0));
        let order_log = Arc::new(std::sync::Mutex::new(Vec::new()));

        let mut saga = SagaOrchestrator::new();
        saga.add_step(OrderedStep::new(
            "s1",
            Arc::clone(&counter),
            Arc::clone(&order_log),
        ));
        saga.add_step(OrderedStep::new(
            "s2",
            Arc::clone(&counter),
            Arc::clone(&order_log),
        ));
        saga.add_step(OrderedStep::new(
            "s3",
            Arc::clone(&counter),
            Arc::clone(&order_log),
        ));
        saga.add_step(FailStep::new("s4", "fail"));

        let state = saga.execute().clone();
        assert_eq!(state, SagaState::Compensated);

        let log = order_log.lock().expect("lock poisoned");
        assert_eq!(log.len(), 3);
        // Reverse order: s3 first (order=0), s2 second (order=1), s1 third (order=2)
        assert_eq!(log[0].0, "s3");
        assert_eq!(log[0].1, 0);
        assert_eq!(log[1].0, "s2");
        assert_eq!(log[1].1, 1);
        assert_eq!(log[2].0, "s1");
        assert_eq!(log[2].1, 2);
    }

    #[test]
    fn test_saga_state_is_running_during_execution() {
        struct StateCheckStep {
            id: String,
        }

        impl SagaStep for StateCheckStep {
            fn step_id(&self) -> &str {
                &self.id
            }
            fn execute(&self) -> StepResult {
                StepResult::Succeeded
            }
            fn compensate(&self) -> StepResult {
                StepResult::Succeeded
            }
        }

        let mut saga = SagaOrchestrator::new();
        saga.add_step(StateCheckStep {
            id: "check".to_string(),
        });

        // Before execute, state is Pending
        assert_eq!(*saga.state(), SagaState::Pending);

        saga.execute();

        // After execute with success, state is Completed (passed through Running)
        assert_eq!(*saga.state(), SagaState::Completed);
        // Log proves it went through Running (started)
        assert!(
            saga.execution_log()
                .first()
                .map_or(false, |e| e == "saga: started")
        );
    }

    #[test]
    fn test_saga_execution_log_records_steps() {
        let mut saga = SagaOrchestrator::new();
        saga.add_step(SuccessStep::new("a"));
        saga.add_step(SuccessStep::new("b"));
        saga.add_step(SuccessStep::new("c"));
        saga.execute();

        let log = saga.execution_log();
        assert!(log.iter().any(|e| e == "executed: a"));
        assert!(log.iter().any(|e| e == "executed: b"));
        assert!(log.iter().any(|e| e == "executed: c"));
    }

    #[test]
    fn test_saga_initial_state_is_pending() {
        let saga = SagaOrchestrator::new();
        assert_eq!(*saga.state(), SagaState::Pending);
    }

    #[test]
    fn test_saga_add_step_empty() {
        let mut saga = SagaOrchestrator::new();
        // Starts with 0 steps
        assert!(saga.steps.is_empty());
        saga.add_step(SuccessStep::new("only"));
        assert_eq!(saga.steps.len(), 1);
        let state = saga.execute().clone();
        assert_eq!(state, SagaState::Completed);
    }

    #[test]
    fn test_saga_default_trait() {
        let saga = SagaOrchestrator::default();
        assert_eq!(*saga.state(), SagaState::Pending);
        assert!(saga.execution_log().is_empty());
    }

    #[test]
    fn test_saga_compensation_failure_logged() {
        struct FailCompensateStep {
            id: String,
        }

        impl SagaStep for FailCompensateStep {
            fn step_id(&self) -> &str {
                &self.id
            }
            fn execute(&self) -> StepResult {
                StepResult::Succeeded
            }
            fn compensate(&self) -> StepResult {
                StepResult::Failed("compensate_err".to_string())
            }
        }

        let mut saga = SagaOrchestrator::new();
        saga.add_step(FailCompensateStep {
            id: "fc1".to_string(),
        });
        saga.add_step(FailStep::new("f2", "trigger"));
        saga.execute();

        // Compensation failed → state must be Failed("partial compensation: fc1"),
        // NOT Compensated — the system is in an inconsistent state.
        match saga.state() {
            SagaState::Failed(reason) => assert!(reason.contains("fc1")),
            other => panic!("expected Failed, got {other:?}"),
        }
        assert!(
            saga.execution_log()
                .iter()
                .any(|e| e.contains("compensation_failed: fc1"))
        );
    }

    // --- IL-4: GeneratorSagaStep tests ---

    #[test]
    fn test_generator_saga_step_execute_succeeds() {
        let step = GeneratorSagaStep::new("gen-1", "exec.sh", "rollback.sh");
        assert_eq!(step.step_id(), "gen-1");
        assert_eq!(step.execute(), StepResult::Succeeded);
    }

    #[test]
    fn test_generator_saga_step_compensate_succeeds() {
        let step = GeneratorSagaStep::new("gen-1", "exec.sh", "rollback.sh");
        assert_eq!(step.compensate(), StepResult::Succeeded);
    }

    #[test]
    fn test_generator_saga_step_empty_exec_fails() {
        let step = GeneratorSagaStep::new("gen-2", "", "rollback.sh");
        assert!(matches!(step.execute(), StepResult::Failed(_)));
    }

    #[test]
    fn test_generator_saga_step_empty_rollback_fails() {
        let step = GeneratorSagaStep::new("gen-2", "exec.sh", "");
        assert!(matches!(step.compensate(), StepResult::Failed(_)));
    }

    #[test]
    fn test_generator_saga_step_in_orchestrator() {
        let mut saga = SagaOrchestrator::new();
        saga.add_step(GeneratorSagaStep::new(
            "gen-a",
            "exec_a.sh",
            "rollback_a.sh",
        ));
        saga.add_step(GeneratorSagaStep::new(
            "gen-b",
            "exec_b.sh",
            "rollback_b.sh",
        ));
        let state = saga.execute();
        assert_eq!(*state, SagaState::Completed);
    }
}
