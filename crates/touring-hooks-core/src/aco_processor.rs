//! AcoEventProcessor — Drains decomposer ACO events and processes via ACO pheromone system.
//!
//! Integrates the TaskDecomposer (touring-server) ACO events with the hook runtime's
//! ACO pheromone pipeline. Events are drained from the decomposer and fed to the
//! ACO bus for pheromone tracking and reinforcement.

use std::sync::Arc;
use touring_intelligence::rl::aco::UnifiedPheromoneBus;
use touring_intelligence::rl::aco::pheromone_bus::PheroKey;

/// ACO event from the TaskDecomposer (touring-server).
///
/// Mirrors `touring_server::reasoning::decomposer::AcoEvent` to avoid
/// a cross-crate dependency from touring-hooks → touring-server.
/// The delta and key methods produce values compatible with `UnifiedPheromoneBus::deposit`.
#[derive(Debug, Clone)]
pub enum AcoEvent {
    /// A subtask finished successfully (deposits positive pheromone).
    TaskCompleted {
        /// Parent task identifier.
        task_id: String,
        /// Subtask identifier within the parent task.
        subtask_id: String,
        /// Wall-clock execution time of the subtask in milliseconds.
        duration_ms: u64,
    },
    /// A subtask failed (evaporates pheromone to discourage the path).
    TaskFailed {
        /// Parent task identifier.
        task_id: String,
        /// Subtask identifier within the parent task.
        subtask_id: String,
        /// Human-readable failure reason.
        reason: String,
    },
    /// A subtask is blocked on unmet dependencies (mild pheromone evaporation).
    TaskBlocked {
        /// Parent task identifier.
        task_id: String,
        /// Subtask identifier within the parent task.
        subtask_id: String,
    },
    /// A subtask began executing (no pheromone change, tracks activity).
    TaskStarted {
        /// Parent task identifier.
        task_id: String,
        /// Subtask identifier within the parent task.
        subtask_id: String,
    },
}

impl AcoEvent {
    /// Returns the pheromone delta for this event.
    /// Positive = deposit, Negative = evaporate.
    pub fn pheromone_delta(&self) -> f64 {
        match self {
            Self::TaskCompleted { .. } => 1.0,
            Self::TaskFailed { .. } => -0.5,
            Self::TaskBlocked { .. } => -0.3,
            Self::TaskStarted { .. } => 0.1,
        }
    }

    /// Returns the pheromone key for this event.
    pub fn phero_key(&self) -> PheroKey {
        match self {
            Self::TaskCompleted { task_id, .. } => {
                PheroKey::TaskId(format!("completion:{}", task_id))
            }
            Self::TaskFailed { task_id, .. } => PheroKey::TaskId(format!("failure:{}", task_id)),
            Self::TaskBlocked { task_id, .. } => PheroKey::TaskId(format!("blocked:{}", task_id)),
            Self::TaskStarted { task_id, .. } => PheroKey::TaskId(format!("started:{}", task_id)),
        }
    }

    /// Returns a debug name for tracing.
    pub fn event_name(&self) -> &'static str {
        match self {
            Self::TaskCompleted { .. } => "task_completed",
            Self::TaskFailed { .. } => "task_failed",
            Self::TaskBlocked { .. } => "task_blocked",
            Self::TaskStarted { .. } => "task_started",
        }
    }
}

/// ACO event processor for decomposer events.
///
/// Drains pending ACO events from the TaskDecomposer and injects them
/// into the hook runtime's ACO pheromone pipeline for downstream consumers.
#[derive(Clone)]
pub struct AcoEventProcessor {
    /// Reference to the ACO bus for pheromone injection.
    bus: Arc<UnifiedPheromoneBus>,
}

impl AcoEventProcessor {
    /// Create a new processor backed by the given ACO bus.
    pub fn new(bus: Arc<UnifiedPheromoneBus>) -> Self {
        Self { bus }
    }

    /// Process a batch of ACO events by depositing them into the pheromone bus.
    ///
    /// Each event is dispatched to the bus for async processing by downstream
    /// ACO consumers (e.g., AcoRewardPropagator, PredictiveFocusCache).
    /// This method is infallible — events that cannot be dispatched are logged
    /// and dropped, never blocking the caller.
    pub fn process_events(&self, events: Vec<AcoEvent>) {
        for event in events {
            let delta = event.pheromone_delta();
            let key = event.phero_key();
            let event_name = event.event_name();
            tracing::debug!("ACO event: {} key={:?} delta={}", event_name, key, delta);
            // Deposit to bus — fire-and-forget, errors are logged inside the bus.
            self.bus.deposit(key, delta);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_process_events_deposits_to_bus() {
        let bus = Arc::new(UnifiedPheromoneBus::new(0.05));
        let processor = AcoEventProcessor::new(Arc::clone(&bus));
        let events = vec![
            AcoEvent::TaskStarted {
                task_id: "t1".into(),
                subtask_id: "s1".into(),
            },
            AcoEvent::TaskCompleted {
                task_id: "t1".into(),
                subtask_id: "s1".into(),
                duration_ms: 100,
            },
        ];
        // Should not panic — process_events is infallible
        processor.process_events(events);
        // Verify pheromone was deposited
        let started_key = PheroKey::TaskId("started:t1".into());
        let completed_key = PheroKey::TaskId("completion:t1".into());
        assert!(bus.get(&started_key) > 0.0);
        assert!(bus.get(&completed_key) > 0.0);
    }

    #[test]
    fn test_pheromone_delta() {
        assert_eq!(
            AcoEvent::TaskCompleted {
                task_id: "t1".into(),
                subtask_id: "s1".into(),
                duration_ms: 100
            }
            .pheromone_delta(),
            1.0
        );
        assert_eq!(
            AcoEvent::TaskFailed {
                task_id: "t1".into(),
                subtask_id: "s1".into(),
                reason: "oops".into()
            }
            .pheromone_delta(),
            -0.5
        );
        assert_eq!(
            AcoEvent::TaskBlocked {
                task_id: "t1".into(),
                subtask_id: "s1".into()
            }
            .pheromone_delta(),
            -0.3
        );
        assert_eq!(
            AcoEvent::TaskStarted {
                task_id: "t1".into(),
                subtask_id: "s1".into()
            }
            .pheromone_delta(),
            0.1
        );
    }
}
