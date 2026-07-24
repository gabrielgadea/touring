//! **S-13 / R12 — CEG-gated MCTS node scoring.**
//!
//! The generic MCTS ([`super::mcts`]) expands candidate actions through an opaque
//! `expand_fn` and scores them by UCT alone — it cannot tell a *verified-safe*
//! action from one the Code Execution Gateway would deny. R12 closes that: every
//! candidate carries a **CEG credibility** in `0.0..=1.0` (derived from a
//! `GateDecision` verdict + composite by the bridge `verdict_credibility` on the
//! touring-hooks side), and the gated score scales the base UCT by it.
//!
//! - **Deny** candidate → credibility `0.0` → gated score `0.0` → never selected
//!   (the deny-wins discipline, now inside planning).
//! - **Warn** → `~0.5` → explored half as eagerly as an **Allow** (`~1.0`).
//!
//! This module is intentionally **generic over the credibility scalar** — it does
//! NOT depend on `touring-hooks`. The `Verdict → credibility` mapping lives in
//! touring-hooks (`gateway::decision::verdict_credibility`), and `touring-server`
//! (which depends on both crates) joins them at runtime. Keeping the gate scalar
//! abstract here avoids a cross-crate dependency edge entirely (cycle-free by
//! construction).

/// A candidate MCTS expansion annotated with its CEG credibility.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GatedCandidate {
    /// The action id (matches the generic MCTS `u64` action space).
    pub action: u64,
    /// The base UCT/UCB1 score the un-gated MCTS would assign.
    pub base_uct: f64,
    /// The CEG verdict-credibility in `0.0..=1.0` (Deny → 0.0, Allow → ~1.0).
    pub credibility: f64,
}

impl GatedCandidate {
    /// Construct a gated candidate.
    #[must_use]
    pub fn new(action: u64, base_uct: f64, credibility: f64) -> Self {
        Self {
            action,
            base_uct,
            credibility,
        }
    }

    /// The CEG-gated score: the (non-negative) base UCT scaled by credibility.
    /// A Deny candidate (`credibility == 0.0`) always scores `0.0` — it can never
    /// be selected, regardless of how attractive its raw UCT looked.
    #[must_use]
    pub fn gated_score(&self) -> f64 {
        self.base_uct.max(0.0) * self.credibility.clamp(0.0, 1.0)
    }
}

/// Select the best candidate by gated score. Returns `None` when **every**
/// candidate is gated to `0.0` (e.g. all Deny) — the planner must then refuse to
/// expand rather than pick an unsafe action.
#[must_use]
pub fn select_best_gated(candidates: &[GatedCandidate]) -> Option<GatedCandidate> {
    candidates
        .iter()
        .copied()
        .filter(|c| c.gated_score() > 0.0)
        .max_by(|a, b| {
            a.gated_score()
                .partial_cmp(&b.gated_score())
                .unwrap_or(std::cmp::Ordering::Equal)
        })
}

/// The **verified-action-depth** of a planned path: the length of the longest
/// leading run of actions whose credibility meets `threshold`. This is the
/// MCTS analogue of the speculative loop's accepted-prefix length (S-12) and the
/// EAGLE acceptance length — the deeper the verified prefix, the more of the plan
/// the CEG has already proven safe.
#[must_use]
pub fn verified_action_depth(path_credibilities: &[f64], threshold: f64) -> usize {
    path_credibilities
        .iter()
        .take_while(|&&c| c >= threshold)
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deny_candidate_scores_zero_and_is_never_selected() {
        // action 0 = Allow (cred 1.0); actions 1,2 = Deny (cred 0.0), even with
        // higher raw UCT. The gate must pick action 0.
        let candidates = vec![
            GatedCandidate::new(0, 0.5, 1.0),
            GatedCandidate::new(1, 0.9, 0.0),
            GatedCandidate::new(2, 0.95, 0.0),
        ];
        let best = select_best_gated(&candidates).expect("one Allow candidate exists");
        assert_eq!(
            best.action, 0,
            "must pick the Allow action over higher-UCT Deny actions"
        );
        assert_eq!(GatedCandidate::new(1, 0.9, 0.0).gated_score(), 0.0);
    }

    #[test]
    fn all_deny_yields_no_selection() {
        let candidates = vec![
            GatedCandidate::new(0, 0.9, 0.0),
            GatedCandidate::new(1, 0.8, 0.0),
        ];
        assert!(
            select_best_gated(&candidates).is_none(),
            "all-Deny must refuse to expand"
        );
    }

    #[test]
    fn warn_is_explored_less_eagerly_than_allow() {
        // Equal base UCT; Allow (1.0) must outscore Warn (0.5).
        let allow = GatedCandidate::new(0, 0.6, 1.0);
        let warn = GatedCandidate::new(1, 0.6, 0.5);
        assert!(allow.gated_score() > warn.gated_score());
        let best = select_best_gated(&[warn, allow]).unwrap();
        assert_eq!(best.action, 0);
    }

    #[test]
    fn verified_action_depth_counts_leading_credible_run() {
        // Allow, Allow, Deny, Allow → verified depth 2 (truncates at the Deny).
        let creds = [1.0, 0.9, 0.0, 1.0];
        assert_eq!(verified_action_depth(&creds, 0.5), 2);
        assert_eq!(verified_action_depth(&[0.0, 1.0], 0.5), 0);
        assert_eq!(verified_action_depth(&[1.0, 1.0, 1.0], 0.5), 3);
    }
}
