//! **S-09 / R8 — the formal change contract gating self-mutation.**
//!
//! Touring's `health_delta` regression streak *detects* a regression after the
//! fact. R8 asks for the stronger guarantee the meta-loop (§3.5) needs: a
//! self-mutation may **commit only if it provably does not regress** harness
//! quality. [`ChangeContract`] is that proof obligation made explicit — a
//! before/after pair of [`HarnessQuality`] snapshots plus the [`Invariant`]s the
//! change must preserve. [`ChangeContract::evaluate`] returns
//! [`ContractVerdict::committed`] `true` only when **every** invariant holds;
//! one violation blocks the commit and names the offending dimension.
//!
//! This is the no-regression discipline a code-agent harness must apply *to its
//! own evolution*: the same deny-wins rigor the CEG applies to executions, now
//! applied to mutations of the harness itself.

use super::harness_metric::HarnessQuality;
use serde::{Deserialize, Serialize};

/// Floating-point tolerance — a change within `EPSILON` of the prior value is
/// treated as "no change", never a regression. Guards against meaningless
/// sub-noise deltas blocking a commit.
const EPSILON: f64 = 1e-6;

/// A single property a self-mutation must preserve to be allowed to commit.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Invariant {
    /// The `composite` KPI must not drop (`post >= pre - EPSILON`).
    NoCompositeRegression,
    /// No individual dimension may drop below its pre-mutation value — a change
    /// may not rob Peter (one axis) to pay Paul (another) while holding the
    /// composite flat.
    NoDimensionRegression,
    /// The post-mutation `composite` must be at least this absolute floor,
    /// regardless of the pre value — a hard quality bar for the evolved state.
    MinComposite(f64),
}

/// The default invariant set: no composite regression **and** no per-dimension
/// regression. The strictest contract that still admits genuine improvements.
#[must_use]
pub fn default_invariants() -> Vec<Invariant> {
    vec![
        Invariant::NoCompositeRegression,
        Invariant::NoDimensionRegression,
    ]
}

/// The six named (pre, post) dimension pairs of a contract, for per-axis checks.
fn dimension_pairs(pre: &HarnessQuality, post: &HarnessQuality) -> [(&'static str, f64, f64); 6] {
    [
        ("executable", pre.executable, post.executable),
        ("inspectable", pre.inspectable, post.inspectable),
        ("stateful", pre.stateful, post.stateful),
        ("governed", pre.governed, post.governed),
        ("performant", pre.performant, post.performant),
        ("evolving", pre.evolving, post.evolving),
    ]
}

/// The result of evaluating a [`ChangeContract`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContractVerdict {
    /// `true` only when every invariant held — the mutation may commit.
    pub committed: bool,
    /// One human-readable line per violated invariant; empty when `committed`.
    pub violations: Vec<String>,
}

/// A formal before/after contract over [`HarnessQuality`] gating a self-mutation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChangeContract {
    /// Harness quality measured immediately before the mutation.
    pub pre: HarnessQuality,
    /// Harness quality measured immediately after the mutation.
    pub post: HarnessQuality,
    /// The invariants the mutation must preserve to commit.
    pub invariants: Vec<Invariant>,
}

impl ChangeContract {
    /// A contract with the [`default_invariants`] — no composite and no
    /// per-dimension regression.
    #[must_use]
    pub fn new(pre: HarnessQuality, post: HarnessQuality) -> Self {
        Self {
            pre,
            post,
            invariants: default_invariants(),
        }
    }

    /// A contract with an explicit invariant set.
    #[must_use]
    pub fn with_invariants(
        pre: HarnessQuality,
        post: HarnessQuality,
        invariants: Vec<Invariant>,
    ) -> Self {
        Self {
            pre,
            post,
            invariants,
        }
    }

    /// Evaluate the contract. The mutation commits **only if every invariant
    /// holds** — the no-regression proof. Each failed invariant contributes one
    /// violation line; an empty violation list means the commit is approved.
    #[must_use]
    pub fn evaluate(&self) -> ContractVerdict {
        let mut violations = Vec::new();
        for inv in &self.invariants {
            match inv {
                Invariant::NoCompositeRegression => {
                    if self.post.composite + EPSILON < self.pre.composite {
                        violations.push(format!(
                            "composite regressed {:.4} -> {:.4}",
                            self.pre.composite, self.post.composite
                        ));
                    }
                }
                Invariant::NoDimensionRegression => {
                    for (name, pre, post) in dimension_pairs(&self.pre, &self.post) {
                        if post + EPSILON < pre {
                            violations.push(format!(
                                "dimension '{name}' regressed {pre:.4} -> {post:.4}"
                            ));
                        }
                    }
                }
                Invariant::MinComposite(floor) => {
                    if self.post.composite + EPSILON < *floor {
                        violations.push(format!(
                            "post composite {:.4} below floor {floor:.4}",
                            self.post.composite
                        ));
                    }
                }
            }
        }
        ContractVerdict {
            committed: violations.is_empty(),
            violations,
        }
    }

    /// Convenience: `true` when [`evaluate`](Self::evaluate) approves the commit.
    #[must_use]
    pub fn is_committed(&self) -> bool {
        self.evaluate().committed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hq(values: [f64; 7]) -> HarnessQuality {
        HarnessQuality {
            executable: values[0],
            inspectable: values[1],
            stateful: values[2],
            governed: values[3],
            performant: values[4],
            evolving: values[5],
            composite: values[6],
        }
    }

    #[test]
    fn allows_a_strict_improvement() {
        let pre = hq([0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5]);
        let post = hq([0.6, 0.6, 0.6, 0.6, 0.6, 0.6, 0.6]);
        let verdict = ChangeContract::new(pre, post).evaluate();
        assert!(verdict.committed, "improvement must commit");
        assert!(verdict.violations.is_empty());
    }

    #[test]
    fn allows_an_unchanged_state() {
        let q = hq([0.7, 0.7, 0.7, 0.7, 0.7, 0.7, 0.7]);
        assert!(
            ChangeContract::new(q, q).is_committed(),
            "no change is no regression"
        );
    }

    #[test]
    fn change_contract_blocks_regression() {
        let pre = hq([0.8, 0.8, 0.8, 0.8, 0.8, 0.8, 0.8]);
        let post = hq([0.8, 0.8, 0.8, 0.8, 0.8, 0.8, 0.6]); // composite dropped
        let verdict = ChangeContract::new(pre, post).evaluate();
        assert!(!verdict.committed, "a composite regression must be blocked");
        assert!(
            verdict
                .violations
                .iter()
                .any(|v| v.contains("composite regressed")),
            "violation must name the composite regression: {:?}",
            verdict.violations
        );
    }

    #[test]
    fn detects_per_dimension_regression_even_when_composite_holds() {
        // governed drops, evolving rises — composite identical, but a dimension
        // regressed: NoDimensionRegression must catch it.
        let pre = hq([0.6, 0.6, 0.6, 0.6, 0.6, 0.6, 0.6]);
        let post = hq([0.6, 0.6, 0.6, 0.4, 0.6, 0.8, 0.6]);
        let verdict = ChangeContract::new(pre, post).evaluate();
        assert!(!verdict.committed);
        assert!(
            verdict.violations.iter().any(|v| v.contains("governed")),
            "must name the regressed dimension: {:?}",
            verdict.violations
        );
    }

    #[test]
    fn min_composite_floor_is_enforced() {
        let pre = hq([0.3, 0.3, 0.3, 0.3, 0.3, 0.3, 0.3]);
        let post = hq([0.35, 0.35, 0.35, 0.35, 0.35, 0.35, 0.35]); // improved but still low
        let contract = ChangeContract::with_invariants(
            pre,
            post,
            vec![
                Invariant::NoCompositeRegression,
                Invariant::MinComposite(0.5),
            ],
        );
        let verdict = contract.evaluate();
        assert!(
            !verdict.committed,
            "below the absolute floor must block even on improvement"
        );
        assert!(verdict.violations.iter().any(|v| v.contains("below floor")));
    }
}
