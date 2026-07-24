//! AcoWiring implementation for HookRuntime.
//!
//! Provides thread-safe access to the UnifiedPheromoneBus via Mutex.
//! All operations are fire-and-forget with logging on failure.

use super::traits::AcoWiring;
use crate::runtime::HookRuntime;
use touring_intelligence::rl::aco::tracker::TrackerReport;

impl AcoWiring for HookRuntime {
    fn deposit_file_edit(&self, file_path: &str) {
        match self.aco_wiring.lock() {
            Ok(guard) => {
                guard.deposit_file_edit(file_path);
            }
            _ => {
                tracing::warn!(
                    hook = "deposit_file_edit",
                    "failed to acquire aco_wiring lock"
                );
            }
        }
    }

    fn process_tracker_report(&self, report: &TrackerReport, state: u64, action: u64) {
        match self.aco_wiring.lock() {
            Ok(guard) => {
                guard.process_tracker_report(report, state, action);
            }
            _ => {
                tracing::warn!(
                    hook = "process_tracker_report",
                    "failed to acquire aco_wiring lock"
                );
            }
        }
    }

    fn deposit_task_completion(&self, task_id: &str, success: bool) {
        match self.aco_wiring.lock() {
            Ok(guard) => {
                guard.deposit_task_completion(task_id, success);
            }
            _ => {
                tracing::warn!(
                    hook = "deposit_task_completion",
                    "failed to acquire aco_wiring lock"
                );
            }
        }
    }

    fn deposit_teammate_idle(&self, teammate_id: &str, tasks_completed: u32) {
        match self.aco_wiring.lock() {
            Ok(guard) => {
                guard.deposit_teammate_idle(teammate_id, tasks_completed);
            }
            _ => {
                tracing::warn!(
                    hook = "deposit_teammate_idle",
                    "failed to acquire aco_wiring lock"
                );
            }
        }
    }

    fn deposit_teammate_limbo(
        &self,
        teammate_id: &str,
        blocked_count: u32,
        uncompleted_count: u32,
    ) {
        match self.aco_wiring.lock() {
            Ok(guard) => {
                guard.deposit_teammate_limbo(teammate_id, blocked_count, uncompleted_count);
            }
            _ => {
                tracing::warn!(
                    hook = "deposit_teammate_limbo",
                    "failed to acquire aco_wiring lock"
                );
            }
        }
    }

    fn task_heat(&self, task_id: &str) -> f64 {
        self.aco_wiring
            .lock()
            .map(|g| g.task_heat(task_id))
            .unwrap_or_else(|_| {
                tracing::warn!(hook = "task_heat", "failed to acquire aco_wiring lock");
                0.0
            })
    }

    fn teammate_heat(&self, teammate_id: &str) -> f64 {
        self.aco_wiring
            .lock()
            .map(|g| g.teammate_heat(teammate_id))
            .unwrap_or_else(|_| {
                tracing::warn!(hook = "teammate_heat", "failed to acquire aco_wiring lock");
                0.0
            })
    }

    fn limbo_heat(&self, teammate_id: &str) -> f64 {
        self.aco_wiring
            .lock()
            .map(|g| g.limbo_heat(teammate_id))
            .unwrap_or_else(|_| {
                tracing::warn!(hook = "limbo_heat", "failed to acquire aco_wiring lock");
                0.0
            })
    }

    fn flush_aco_metrics_to_bus<F>(&self, mut push: F)
    where
        F: FnMut(u8, f64),
    {
        match self.aco_wiring.lock() {
            Ok(guard) => {
                guard.flush_aco_metrics_to_bus(|arm_id, reward| {
                    push(arm_id, reward);
                });
            }
            _ => {
                tracing::warn!(
                    hook = "flush_aco_metrics_to_bus",
                    "failed to acquire aco_wiring lock"
                );
            }
        }
    }
}
