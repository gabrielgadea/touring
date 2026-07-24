//! C13 — Selective checkpointing: only side-effecting actions get a compensating
//! saga step.
//!
//! Blind checkpointing snapshots *every* action so any of them can be rolled back —
//! but the vast majority of captured actions (reads, classification, analysis) mutate
//! nothing and have nothing to compensate. The "Crab" insight is to checkpoint
//! **selectively**: only when an action actually carries a side effect. In practice
//! that skips ~87% of actions, paying the checkpoint cost only where a rollback is
//! meaningful.
//!
//! The side-effect signal is the CEG's own [`Capability`] model
//! ([`Capability::is_side_effecting`] — `FsWrite` / `Net` / `Run`), which is always
//! on. When the `ebpf-telemetry` feature is active, the same decision is refined by
//! *runtime-observed* syscalls (a write/connect/exec the static profile under-declared)
//! — but the static capability decision already covers the common case.
//!
//! [`decide_checkpoint`] turns the required capabilities of a captured action into a
//! [`CheckpointDecision`]; a `needs_compensation` decision is what a downstream
//! consumer (the X8 supervised-exec path) hands to the
//! `DistributedSagaCoordinator::compensate` rollback. The decision is **pure** and
//! fully unit-tested here.

use crate::capability::Capability;

/// The selective-checkpoint decision for one captured action (C13).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckpointDecision {
    /// `true` when at least one required capability mutates external state, so a
    /// compensating saga step must be registered before the action executes.
    pub needs_compensation: bool,
    /// The side-effecting capabilities that would have to be rolled back (the scope
    /// the compensating saga step must undo). Empty when `needs_compensation` is false.
    pub side_effects: Vec<Capability>,
}

impl CheckpointDecision {
    /// Number of distinct side effects to compensate (the saga step count).
    #[must_use]
    pub fn compensation_steps(&self) -> usize {
        self.side_effects.len()
    }
}

/// Decide whether a captured action needs a saga compensation checkpoint, given the
/// `required` capabilities it declares. Side-effect-free actions (reads only) skip the
/// checkpoint entirely — the selective-checkpoint insight that avoids paying for the
/// ~majority of actions that have nothing to compensate.
#[must_use]
pub fn decide_checkpoint(required: &[Capability]) -> CheckpointDecision {
    let side_effects: Vec<Capability> = required
        .iter()
        .filter(|c| c.is_side_effecting())
        .cloned()
        .collect();
    CheckpointDecision {
        needs_compensation: !side_effects.is_empty(),
        side_effects,
    }
}

/// Running tally of selective-checkpoint decisions — surfaces the *skip rate* (the
/// "~87% avoided" claim made observable). Cheap, lock-free counters a caller updates
/// per captured action via [`SelectiveCheckpointStats::record`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SelectiveCheckpointStats {
    /// Total actions evaluated.
    pub evaluated: u64,
    /// Actions that needed a compensation checkpoint (side-effecting).
    pub checkpointed: u64,
    /// Actions that skipped the checkpoint (side-effect-free).
    pub skipped: u64,
}

impl SelectiveCheckpointStats {
    /// Fold one decision into the tally.
    pub fn record(&mut self, decision: &CheckpointDecision) {
        self.evaluated += 1;
        if decision.needs_compensation {
            self.checkpointed += 1;
        } else {
            self.skipped += 1;
        }
    }

    /// Fraction of actions for which the checkpoint was skipped, in `[0.0, 1.0]`.
    /// `0.0` when nothing has been evaluated yet.
    #[must_use]
    pub fn skip_rate(&self) -> f64 {
        if self.evaluated == 0 {
            return 0.0;
        }
        self.skipped as f64 / self.evaluated as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::scope::{CmdScope, HostScope, KeyScope, PathScope};

    fn read() -> Capability {
        Capability::FsRead(PathScope::new("/repo"))
    }
    fn write() -> Capability {
        Capability::FsWrite(PathScope::new("/repo/out"))
    }
    fn net() -> Capability {
        Capability::Net(HostScope::any())
    }
    fn run() -> Capability {
        Capability::Run(CmdScope::new("cargo"))
    }
    fn env() -> Capability {
        Capability::Env(KeyScope::new("PATH"))
    }

    #[test]
    fn reads_are_side_effect_free() {
        assert!(!read().is_side_effecting());
        assert!(!env().is_side_effecting());
    }

    #[test]
    fn writes_network_and_spawn_are_side_effecting() {
        assert!(write().is_side_effecting());
        assert!(net().is_side_effecting());
        assert!(run().is_side_effecting());
    }

    #[test]
    fn read_only_action_skips_checkpoint() {
        let d = decide_checkpoint(&[read(), env()]);
        assert!(!d.needs_compensation);
        assert_eq!(d.compensation_steps(), 0);
    }

    #[test]
    fn side_effecting_action_needs_compensation() {
        let d = decide_checkpoint(&[read(), write(), env(), net()]);
        assert!(d.needs_compensation);
        // Only the write + net are compensated; the read + env are not.
        assert_eq!(d.compensation_steps(), 2);
        assert!(d.side_effects.contains(&write()));
        assert!(d.side_effects.contains(&net()));
        assert!(!d.side_effects.contains(&read()));
    }

    #[test]
    fn empty_requirements_skip_checkpoint() {
        let d = decide_checkpoint(&[]);
        assert!(!d.needs_compensation);
    }

    #[test]
    fn stats_track_skip_rate() {
        let mut stats = SelectiveCheckpointStats::default();
        assert_eq!(stats.skip_rate(), 0.0);
        // 3 read-only (skipped) + 1 side-effecting (checkpointed) → 75% skipped.
        for _ in 0..3 {
            stats.record(&decide_checkpoint(&[read()]));
        }
        stats.record(&decide_checkpoint(&[write()]));
        assert_eq!(stats.evaluated, 4);
        assert_eq!(stats.skipped, 3);
        assert_eq!(stats.checkpointed, 1);
        assert!((stats.skip_rate() - 0.75).abs() < f64::EPSILON);
    }
}
