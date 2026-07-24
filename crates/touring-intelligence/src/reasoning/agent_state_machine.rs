//! AgentStateMachine — complete finite state machine for agent lifecycle.
//!
//! INS-10 (ROI=1.20): Models the full agent lifecycle with typed transitions,
//! guard conditions, and transition history for observability.
//! INS-C4: Pheromone-guided transitions accumulate reward on (from, to) pairs.

use std::collections::HashMap;
use std::time::Instant;

/// Possible states of an agent in its lifecycle.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum AgentState {
    /// Agent is idle, awaiting a task.
    Idle,
    /// Agent is gathering context/perception for the task.
    Perceiving,
    /// Agent is forming a plan of action.
    Planning,
    /// Agent is executing the plan.
    Executing,
    /// Agent is validating the result of execution.
    Validating,
    /// Agent is learning from the validated outcome.
    Learning,
    /// Agent is paused and can be resumed.
    Paused,
    /// Agent has failed; carries the failure reason.
    Failed(String),
    /// Agent has terminated and cannot transition further.
    Terminated,
}

/// Events that can trigger state transitions.
#[derive(Debug, Clone, PartialEq)]
pub enum AgentEvent {
    /// A new task arrived for the agent.
    TaskReceived,
    /// Perception/context gathering finished.
    PerceptionComplete,
    /// A plan is ready to execute.
    PlanReady,
    /// Plan execution has begun.
    ExecutionStarted,
    /// Plan execution finished successfully.
    ExecutionComplete,
    /// Validation of the result passed.
    ValidationPassed,
    /// Validation failed; carries the failure reason.
    ValidationFailed(String),
    /// Learning from the outcome completed.
    LearningComplete,
    /// A pause was requested.
    PauseRequested,
    /// A resume from pause was requested.
    ResumeRequested,
    /// Termination of the agent was requested.
    TerminateRequested,
    /// A runtime error occurred; carries the error message.
    ErrorOccurred(String),
    /// Reset the agent back to `Idle`.
    Reset,
}

/// A recorded state transition with timing information.
#[derive(Debug, Clone)]
pub struct StateTransition {
    /// State the agent transitioned from.
    pub from: AgentState,
    /// Event that triggered the transition.
    pub event: AgentEvent,
    /// State the agent transitioned to.
    pub to: AgentState,
    /// Monotonic instant the transition occurred.
    pub timestamp: Instant,
}

/// Finite state machine governing an agent's lifecycle transitions.
///
/// Enforces valid transitions according to a predefined transition table
/// and records full history for observability and debugging.
/// INS-C4: `transition_pheromones` accumulates reward on (from_state, to_state) pairs.
pub struct AgentStateMachine {
    state: AgentState,
    history: Vec<StateTransition>,
    agent_id: String,
    /// INS-C4: pheromone strength per (from_state_label, to_state_label) pair.
    transition_pheromones: HashMap<(String, String), f64>,
}

/// Error from [`AgentStateMachine::transition`] (F-8 / RBP-03: typed in place of `String`).
#[derive(Debug, thiserror::Error)]
pub enum AgentTransitionError {
    /// The requested event is not a valid transition from the current state.
    #[error("{0}")]
    InvalidTransition(String),
}

impl AgentStateMachine {
    /// Create a new state machine in the `Idle` state.
    pub fn new(agent_id: impl Into<String>) -> Self {
        Self {
            state: AgentState::Idle,
            history: Vec::new(),
            agent_id: agent_id.into(),
            transition_pheromones: HashMap::new(),
        }
    }

    /// INS-C4: Accumulate pheromone reward on the `(from, to)` transition pair.
    pub fn deposit_transition(&mut self, from: &str, to: &str, reward: f64) {
        *self
            .transition_pheromones
            .entry((from.to_string(), to.to_string()))
            .or_insert(0.0) += reward;
    }

    /// INS-C4: Return the candidate state with the highest pheromone from `current_state`.
    ///
    /// Falls back to the first candidate if none have a recorded pheromone trail.
    pub fn best_transition_from<'a>(
        &self,
        current_state: &str,
        candidates: &[&'a str],
    ) -> Option<&'a str> {
        if candidates.is_empty() {
            return None;
        }
        candidates.iter().copied().max_by(|&a, &b| {
            let pa = self
                .transition_pheromones
                .get(&(current_state.to_string(), a.to_string()))
                .copied()
                .unwrap_or(0.0);
            let pb = self
                .transition_pheromones
                .get(&(current_state.to_string(), b.to_string()))
                .copied()
                .unwrap_or(0.0);
            pa.partial_cmp(&pb).unwrap_or(std::cmp::Ordering::Equal)
        })
    }

    /// INS-C4: Pheromone strength for a specific (from, to) pair (0.0 if unknown).
    pub fn transition_strength(&self, from: &str, to: &str) -> f64 {
        self.transition_pheromones
            .get(&(from.to_string(), to.to_string()))
            .copied()
            .unwrap_or(0.0)
    }

    /// Returns a reference to the current state.
    pub fn state(&self) -> &AgentState {
        &self.state
    }

    /// Returns the agent identifier.
    pub fn agent_id(&self) -> &str {
        &self.agent_id
    }

    /// Attempt a state transition given an event.
    ///
    /// Returns `Ok(&AgentState)` with the new state on success,
    /// or `Err(String)` if the transition is not valid from the current state.
    pub fn transition(&mut self, event: AgentEvent) -> Result<&AgentState, AgentTransitionError> {
        let next = Self::next_state(&self.state, &event)
            .map_err(AgentTransitionError::InvalidTransition)?;

        let record = StateTransition {
            from: self.state.clone(),
            event,
            to: next.clone(),
            timestamp: Instant::now(),
        };

        self.state = next;
        self.history.push(record);

        Ok(&self.state)
    }

    /// Returns the full transition history.
    pub fn history(&self) -> &[StateTransition] {
        &self.history
    }

    /// Returns the number of transitions that have occurred.
    pub fn transition_count(&self) -> usize {
        self.history.len()
    }

    /// Check whether a transition is valid without modifying state.
    pub fn can_transition(&self, event: &AgentEvent) -> bool {
        Self::next_state(&self.state, event).is_ok()
    }

    /// Pure transition function: given (state, event) compute the next state.
    fn next_state(current: &AgentState, event: &AgentEvent) -> Result<AgentState, String> {
        match (current, event) {
            // Idle transitions
            (AgentState::Idle, AgentEvent::TaskReceived) => Ok(AgentState::Perceiving),
            (AgentState::Idle, AgentEvent::TerminateRequested) => Ok(AgentState::Terminated),

            // Perceiving transitions
            (AgentState::Perceiving, AgentEvent::PerceptionComplete) => Ok(AgentState::Planning),
            (AgentState::Perceiving, AgentEvent::ErrorOccurred(msg)) => {
                Ok(AgentState::Failed(msg.clone()))
            }

            // Planning transitions
            (AgentState::Planning, AgentEvent::PlanReady) => Ok(AgentState::Executing),
            // ExecutionStarted is an alternative trigger: external orchestrator may
            // signal execution directly (e.g. pre-approved plan), bypassing PlanReady.
            (AgentState::Planning, AgentEvent::ExecutionStarted) => Ok(AgentState::Executing),
            (AgentState::Planning, AgentEvent::ErrorOccurred(msg)) => {
                Ok(AgentState::Failed(msg.clone()))
            }

            // Executing transitions
            (AgentState::Executing, AgentEvent::ExecutionComplete) => Ok(AgentState::Validating),
            (AgentState::Executing, AgentEvent::PauseRequested) => Ok(AgentState::Paused),
            (AgentState::Executing, AgentEvent::ErrorOccurred(msg)) => {
                Ok(AgentState::Failed(msg.clone()))
            }

            // Validating transitions
            (AgentState::Validating, AgentEvent::ValidationPassed) => Ok(AgentState::Learning),
            (AgentState::Validating, AgentEvent::ValidationFailed(msg)) => {
                Ok(AgentState::Failed(msg.clone()))
            }

            // Learning transitions
            (AgentState::Learning, AgentEvent::LearningComplete) => Ok(AgentState::Idle),
            (AgentState::Learning, AgentEvent::ErrorOccurred(msg)) => {
                Ok(AgentState::Failed(msg.clone()))
            }

            // Paused transitions
            (AgentState::Paused, AgentEvent::ResumeRequested) => Ok(AgentState::Executing),
            (AgentState::Paused, AgentEvent::TerminateRequested) => Ok(AgentState::Terminated),

            // Failed transitions
            (AgentState::Failed(_), AgentEvent::Reset) => Ok(AgentState::Idle),
            (AgentState::Failed(_), AgentEvent::TerminateRequested) => Ok(AgentState::Terminated),

            // Everything else is invalid
            _ => Err(format!("invalid transition: {:?} + {:?}", current, event)),
        }
    }
}

impl std::fmt::Display for AgentState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Idle => write!(f, "Idle"),
            Self::Perceiving => write!(f, "Perceiving"),
            Self::Planning => write!(f, "Planning"),
            Self::Executing => write!(f, "Executing"),
            Self::Validating => write!(f, "Validating"),
            Self::Learning => write!(f, "Learning"),
            Self::Paused => write!(f, "Paused"),
            Self::Failed(reason) => write!(f, "Failed({reason})"),
            Self::Terminated => write!(f, "Terminated"),
        }
    }
}

impl std::fmt::Display for AgentEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TaskReceived => write!(f, "TaskReceived"),
            Self::PerceptionComplete => write!(f, "PerceptionComplete"),
            Self::PlanReady => write!(f, "PlanReady"),
            Self::ExecutionStarted => write!(f, "ExecutionStarted"),
            Self::ExecutionComplete => write!(f, "ExecutionComplete"),
            Self::ValidationPassed => write!(f, "ValidationPassed"),
            Self::ValidationFailed(msg) => write!(f, "ValidationFailed({msg})"),
            Self::LearningComplete => write!(f, "LearningComplete"),
            Self::PauseRequested => write!(f, "PauseRequested"),
            Self::ResumeRequested => write!(f, "ResumeRequested"),
            Self::TerminateRequested => write!(f, "TerminateRequested"),
            Self::ErrorOccurred(msg) => write!(f, "ErrorOccurred({msg})"),
            Self::Reset => write!(f, "Reset"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_asm_initial_state_is_idle() {
        let asm = AgentStateMachine::new("agent-1");
        assert_eq!(asm.state(), &AgentState::Idle);
        assert_eq!(asm.agent_id(), "agent-1");
        assert_eq!(asm.transition_count(), 0);
    }

    #[test]
    fn test_asm_task_received_transitions_to_perceiving() {
        let mut asm = AgentStateMachine::new("agent-2");
        let result = asm.transition(AgentEvent::TaskReceived);
        assert!(result.is_ok());
        assert_eq!(
            result.expect("transition should succeed"),
            &AgentState::Perceiving
        );
    }

    #[test]
    fn test_asm_full_happy_path() {
        let mut asm = AgentStateMachine::new("agent-happy");

        // Idle -> Perceiving
        assert!(asm.transition(AgentEvent::TaskReceived).is_ok());
        assert_eq!(asm.state(), &AgentState::Perceiving);

        // Perceiving -> Planning
        assert!(asm.transition(AgentEvent::PerceptionComplete).is_ok());
        assert_eq!(asm.state(), &AgentState::Planning);

        // Planning -> Executing
        assert!(asm.transition(AgentEvent::PlanReady).is_ok());
        assert_eq!(asm.state(), &AgentState::Executing);

        // Executing -> Validating
        assert!(asm.transition(AgentEvent::ExecutionComplete).is_ok());
        assert_eq!(asm.state(), &AgentState::Validating);

        // Validating -> Learning
        assert!(asm.transition(AgentEvent::ValidationPassed).is_ok());
        assert_eq!(asm.state(), &AgentState::Learning);

        // Learning -> Idle
        assert!(asm.transition(AgentEvent::LearningComplete).is_ok());
        assert_eq!(asm.state(), &AgentState::Idle);

        assert_eq!(asm.transition_count(), 6);
    }

    #[test]
    fn test_asm_invalid_transition_returns_err() {
        let mut asm = AgentStateMachine::new("agent-invalid");
        let result = asm.transition(AgentEvent::LearningComplete);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("invalid transition"));
        assert!(err.contains("Idle"));
        assert!(err.contains("LearningComplete"));
        // State should remain unchanged
        assert_eq!(asm.state(), &AgentState::Idle);
    }

    #[test]
    fn test_asm_error_in_planning_goes_to_failed() {
        let mut asm = AgentStateMachine::new("agent-plan-err");
        asm.transition(AgentEvent::TaskReceived)
            .expect("to perceiving");
        asm.transition(AgentEvent::PerceptionComplete)
            .expect("to planning");

        let result = asm.transition(AgentEvent::ErrorOccurred("plan failed".into()));
        assert!(result.is_ok());
        assert_eq!(asm.state(), &AgentState::Failed("plan failed".into()));
    }

    #[test]
    fn test_asm_failed_reset_returns_to_idle() {
        let mut asm = AgentStateMachine::new("agent-reset");
        asm.transition(AgentEvent::TaskReceived)
            .expect("to perceiving");
        asm.transition(AgentEvent::ErrorOccurred("boom".into()))
            .expect("to failed");
        assert_eq!(asm.state(), &AgentState::Failed("boom".into()));

        let result = asm.transition(AgentEvent::Reset);
        assert!(result.is_ok());
        assert_eq!(asm.state(), &AgentState::Idle);
    }

    #[test]
    fn test_asm_pause_resume_cycle() {
        let mut asm = AgentStateMachine::new("agent-pause");
        asm.transition(AgentEvent::TaskReceived)
            .expect("to perceiving");
        asm.transition(AgentEvent::PerceptionComplete)
            .expect("to planning");
        asm.transition(AgentEvent::PlanReady).expect("to executing");

        // Pause
        assert!(asm.transition(AgentEvent::PauseRequested).is_ok());
        assert_eq!(asm.state(), &AgentState::Paused);

        // Resume
        assert!(asm.transition(AgentEvent::ResumeRequested).is_ok());
        assert_eq!(asm.state(), &AgentState::Executing);
    }

    #[test]
    fn test_asm_terminate_from_idle() {
        let mut asm = AgentStateMachine::new("agent-term-idle");
        let result = asm.transition(AgentEvent::TerminateRequested);
        assert!(result.is_ok());
        assert_eq!(asm.state(), &AgentState::Terminated);
    }

    #[test]
    fn test_asm_terminate_from_paused() {
        let mut asm = AgentStateMachine::new("agent-term-paused");
        asm.transition(AgentEvent::TaskReceived)
            .expect("to perceiving");
        asm.transition(AgentEvent::PerceptionComplete)
            .expect("to planning");
        asm.transition(AgentEvent::PlanReady).expect("to executing");
        asm.transition(AgentEvent::PauseRequested)
            .expect("to paused");

        let result = asm.transition(AgentEvent::TerminateRequested);
        assert!(result.is_ok());
        assert_eq!(asm.state(), &AgentState::Terminated);
    }

    #[test]
    fn test_asm_history_records_all_transitions() {
        let mut asm = AgentStateMachine::new("agent-history");
        asm.transition(AgentEvent::TaskReceived).expect("ok");
        asm.transition(AgentEvent::PerceptionComplete).expect("ok");
        asm.transition(AgentEvent::PlanReady).expect("ok");

        let history = asm.history();
        assert_eq!(history.len(), 3);

        // First transition: Idle -> Perceiving
        assert_eq!(history[0].from, AgentState::Idle);
        assert_eq!(history[0].event, AgentEvent::TaskReceived);
        assert_eq!(history[0].to, AgentState::Perceiving);

        // Second transition: Perceiving -> Planning
        assert_eq!(history[1].from, AgentState::Perceiving);
        assert_eq!(history[1].event, AgentEvent::PerceptionComplete);
        assert_eq!(history[1].to, AgentState::Planning);

        // Third transition: Planning -> Executing
        assert_eq!(history[2].from, AgentState::Planning);
        assert_eq!(history[2].event, AgentEvent::PlanReady);
        assert_eq!(history[2].to, AgentState::Executing);

        // Timestamps are monotonically increasing
        assert!(history[0].timestamp <= history[1].timestamp);
        assert!(history[1].timestamp <= history[2].timestamp);
    }

    #[test]
    fn test_asm_transition_count() {
        let mut asm = AgentStateMachine::new("agent-count");
        assert_eq!(asm.transition_count(), 0);

        asm.transition(AgentEvent::TaskReceived).expect("ok");
        assert_eq!(asm.transition_count(), 1);

        asm.transition(AgentEvent::PerceptionComplete).expect("ok");
        assert_eq!(asm.transition_count(), 2);

        asm.transition(AgentEvent::PlanReady).expect("ok");
        assert_eq!(asm.transition_count(), 3);
    }

    #[test]
    fn test_asm_can_transition_check() {
        let asm = AgentStateMachine::new("agent-can");

        // From Idle, TaskReceived is valid
        assert!(asm.can_transition(&AgentEvent::TaskReceived));
        // From Idle, TerminateRequested is valid
        assert!(asm.can_transition(&AgentEvent::TerminateRequested));
        // From Idle, LearningComplete is NOT valid
        assert!(!asm.can_transition(&AgentEvent::LearningComplete));
        // From Idle, ResumeRequested is NOT valid
        assert!(!asm.can_transition(&AgentEvent::ResumeRequested));

        // State must remain Idle (no mutation)
        assert_eq!(asm.state(), &AgentState::Idle);
        assert_eq!(asm.transition_count(), 0);
    }
}
