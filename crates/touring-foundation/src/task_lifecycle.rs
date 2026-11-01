//! The lifecycle vocabulary of a DAG task: the stages a mirrored task is
//! scaffolded with, and the statuses that mean the task is over.
//!
//! Both lists live here because each was written twice and the copies drifted,
//! which a census of the live DAG measured on 19/09/2026:
//!
//! * The scaffolder writes three stages (`scout` → `implement` → `validate`);
//!   the closer in the `task-completed` hook named two of them as literals.
//!   Result: 74 of 78 open subtasks under mirrored tasks were the same
//!   orphaned `::scout` slot — not pending work, a completion never emitted.
//! * `finalize` stamps `status = 'finalized'`, while the retention routine
//!   matched `status = 'completed'` — a value `finalize` never produces. So
//!   `archived_at` was NULL on all 381 tasks and every query that filters on
//!   the archive saw the entire history as live.
//!
//! A fourth stage, or a fourth terminal status, is added HERE and every
//! consumer follows. That is the whole point of the module.

/// One stage of the scaffold a mirrored task is born with.
///
/// The description travels with the name so the scaffolder and the closer
/// cannot disagree about what a stage *is*.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MirrorStage {
    /// Slot name, used as the `<task>::<name>` subtask suffix.
    pub name: &'static str,
    /// Human-readable description written when the stage is scaffolded.
    pub description: &'static str,
    /// Stage this one waits for, if any — the DAG edge.
    pub depends_on: Option<&'static str>,
}

/// The stages every mirrored task is scaffolded with, in dependency order.
///
/// Consumers: the scaffolder (`bridge_task_created`) and the closer
/// (`close_scaffold_stages`). Neither may name a stage literally.
pub const MIRROR_SCAFFOLD_STAGES: [MirrorStage; 3] = [
    MirrorStage {
        name: "scout",
        description: "scout: research context, blast radius, and wiring before changes",
        depends_on: None,
    },
    MirrorStage {
        name: "implement",
        description: "implement: apply changes with VGP verification and speculative validation",
        depends_on: Some("scout"),
    },
    MirrorStage {
        name: "validate",
        description: "validate: cargo test + wiring orphans + memory store lesson",
        depends_on: Some("implement"),
    },
];

/// Statuses that mean a task container is over and may be archived.
///
/// `finalized` is what `cli_decompose_finalize` writes; `completed` and `done`
/// are what the older paths wrote. All three are terminal, and leaving any of
/// them out is what kept the retention routine from ever matching a row.
pub const TERMINAL_TASK_STATUSES: [&str; 3] = ["completed", "done", "finalized"];

/// True when a task carrying this status has finished, however it got there.
#[must_use]
pub fn is_terminal_task_status(status: &str) -> bool {
    TERMINAL_TASK_STATUSES.contains(&status)
}

/// The terminal statuses as a SQL `IN (...)` list, so a query cannot spell a
/// subset of them by hand.
///
/// The values are compile-time constants with no quotes or backslashes, so the
/// interpolation carries nothing a caller controls.
#[must_use]
pub fn terminal_status_sql_list() -> String {
    TERMINAL_TASK_STATUSES
        .iter()
        .map(|s| format!("'{s}'"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// The subtask id of one stage under a task.
#[must_use]
pub fn scaffold_subtask_id(task_id: &str, stage: &MirrorStage) -> String {
    format!("{task_id}::{}", stage.name)
}

/// The opening words of a ticket the perpetual scout files for itself.
///
/// Written by `scout_perpetuo.py::ticket_desc`; read here so the retrospective
/// corpus can tell the scout's own OUTPUT from the knowledge it explores.
pub const SCOUT_TICKET_PREFIX: &str = "scout-ticket:";

/// True when this task description is the scout's own bookkeeping.
///
/// A scout ticket records that a cycle yielded something; it carries no
/// knowledge of its own. Leaving it in the corpus closed a loop measured on
/// 19/09/2026: cycle N filed a ticket, the corpus indexed it, cycle N+1 found
/// it and counted it as a NEW finding, which filed another ticket. The second
/// live ticket's sample was literally `decomp:<the first ticket>` — the scout
/// reading its own handwriting and calling it discovery, with the panel
/// promoting both as demand for the factory.
#[must_use]
pub fn is_scout_ticket(description: &str) -> bool {
    description.trim_start().starts_with(SCOUT_TICKET_PREFIX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_stage_after_the_first_depends_on_the_previous_one() {
        let mut previous: Option<&str> = None;
        for stage in MIRROR_SCAFFOLD_STAGES {
            assert_eq!(
                stage.depends_on, previous,
                "stage `{}` must wait for the stage before it",
                stage.name
            );
            previous = Some(stage.name);
        }
    }

    #[test]
    fn stage_names_are_unique() {
        let mut names: Vec<&str> = MIRROR_SCAFFOLD_STAGES.iter().map(|s| s.name).collect();
        let total = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), total, "two stages share a name");
    }

    #[test]
    fn the_status_finalize_writes_is_terminal() {
        // The bug this module exists for: `finalize` writes `finalized`, and the
        // retention routine asked for `completed`. If this ever fails again, the
        // archive silently stops working.
        assert!(is_terminal_task_status("finalized"));
        assert!(is_terminal_task_status("completed"));
        assert!(is_terminal_task_status("done"));
    }

    #[test]
    fn work_in_flight_is_not_terminal() {
        assert!(!is_terminal_task_status("created"));
        assert!(!is_terminal_task_status("in_progress"));
        assert!(!is_terminal_task_status(""));
    }

    #[test]
    fn the_sql_list_carries_every_terminal_status() {
        let sql = terminal_status_sql_list();
        for status in TERMINAL_TASK_STATUSES {
            assert!(
                sql.contains(&format!("'{status}'")),
                "`{status}` missing from {sql}"
            );
        }
        assert_eq!(sql.matches('\'').count(), TERMINAL_TASK_STATUSES.len() * 2);
    }

    #[test]
    fn subtask_ids_use_the_double_colon_convention() {
        assert_eq!(
            scaffold_subtask_id("cc_task_7", &MIRROR_SCAFFOLD_STAGES[0]),
            "cc_task_7::scout"
        );
    }
}
