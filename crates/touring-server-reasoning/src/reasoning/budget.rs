//! C11 — Budget conservation `B ∈ ℕ⁶` for the decompose DAG.
//!
//! Every node of a decomposition carries a 6-dimensional **budget vector** of
//! natural numbers ([`BudgetVector`]). The **conservation law** is that the sum of
//! the children's budgets never exceeds the root budget, **per dimension**:
//!
//! ```text
//! ∀ d ∈ {tokens, wall_ms, subtasks, dependencies, max_retries, attempts_used}:
//!     Σ_v B_v[d]  ≤  B_root[d]
//! ```
//!
//! A violation means the plan **over-commits** — it promises more work, time or
//! retries than the root task was allotted. The check is the pure, O(|V|·6) = O(|V|)
//! [`verify_conservation`]; the concrete per-subtask vectors are derived from real
//! signals by [`crate::reasoning::decomposer::Task::subtask_budgets`]. The `tokens`
//! dimension is the bridge to the Workflow-tool's token budget.
//!
//! The verifier is **pure** (no I/O, deterministic) and fully unit-tested here.

use serde::{Deserialize, Serialize};

/// Number of conserved budget dimensions (the `ℕ⁶` of `B ∈ ℕ⁶`).
pub const BUDGET_DIMS: usize = 6;

/// Stable per-dimension names, index-aligned with [`BudgetVector::as_array`].
pub const DIM_NAMES: [&str; BUDGET_DIMS] = [
    "tokens",
    "wall_ms",
    "subtasks",
    "dependencies",
    "max_retries",
    "attempts_used",
];

/// A 6-dimensional budget over a decomposition node — `B ∈ ℕ⁶`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BudgetVector {
    /// Estimated token cost (the bridge to the Workflow-tool token budget).
    pub tokens: u32,
    /// Estimated wall-clock cost in milliseconds.
    pub wall_ms: u32,
    /// Node count contributed (1 per subtask).
    pub subtasks: u32,
    /// Dependency edges contributed (`depends_on` fan-in).
    pub dependencies: u32,
    /// Retry allowance (`retry_policy.max_attempts`).
    pub max_retries: u32,
    /// Retry attempts already spent.
    pub attempts_used: u32,
}

impl BudgetVector {
    /// The six dimensions as an index-aligned array (matches [`DIM_NAMES`]).
    #[must_use]
    pub fn as_array(&self) -> [u32; BUDGET_DIMS] {
        [
            self.tokens,
            self.wall_ms,
            self.subtasks,
            self.dependencies,
            self.max_retries,
            self.attempts_used,
        ]
    }
}

/// A single conserved dimension whose allocated sum exceeded the root budget.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BudgetViolation {
    /// The over-committed dimension name (one of [`DIM_NAMES`]).
    pub dimension: &'static str,
    /// `Σ` of the nodes' budgets on this dimension (`u64` — overflow-safe).
    pub allocated: u64,
    /// The root budget on this dimension.
    pub root_budget: u32,
}

/// Verify the conservation law `Σ Bᵥ ≤ B_root` for every dimension over `nodes`.
///
/// Pure and `O(|V| · 6) = O(|V|)`. Sums are accumulated in `u64` (saturating) so a
/// large fan-out cannot overflow the check. Returns every violated dimension (so the
/// caller sees all over-commitments at once), or `Ok(())` when the plan is conserved.
pub fn verify_conservation(
    root: &BudgetVector,
    nodes: &[BudgetVector],
) -> Result<(), Vec<BudgetViolation>> {
    let root_arr = root.as_array();
    let mut sums = [0u64; BUDGET_DIMS];
    for node in nodes {
        let a = node.as_array();
        for (acc, &v) in sums.iter_mut().zip(a.iter()) {
            *acc = acc.saturating_add(u64::from(v));
        }
    }
    let violations: Vec<BudgetViolation> = (0..BUDGET_DIMS)
        .filter(|&i| sums[i] > u64::from(root_arr[i]))
        .map(|i| BudgetViolation {
            dimension: DIM_NAMES[i],
            allocated: sums[i],
            root_budget: root_arr[i],
        })
        .collect();
    if violations.is_empty() {
        Ok(())
    } else {
        Err(violations)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(tokens: u32, wall_ms: u32, subtasks: u32) -> BudgetVector {
        BudgetVector {
            tokens,
            wall_ms,
            subtasks,
            ..Default::default()
        }
    }

    #[test]
    fn conserved_plan_passes() {
        let root = v(1000, 5000, 10);
        let nodes = [v(300, 1000, 3), v(400, 2000, 4), v(200, 1500, 2)];
        assert!(verify_conservation(&root, &nodes).is_ok());
    }

    #[test]
    fn empty_decomposition_is_conserved() {
        assert!(verify_conservation(&v(0, 0, 0), &[]).is_ok());
    }

    #[test]
    fn exact_match_is_conserved() {
        // Σ == root on every dim is conserved (≤, not <).
        let root = v(100, 200, 2);
        let nodes = [v(60, 120, 1), v(40, 80, 1)];
        assert!(verify_conservation(&root, &nodes).is_ok());
    }

    #[test]
    fn over_commit_on_one_dimension_is_reported() {
        let root = v(1000, 5000, 5);
        // subtasks sum = 6 > 5 → violation on `subtasks` only.
        let nodes = [v(300, 1000, 3), v(400, 2000, 3)];
        let err = verify_conservation(&root, &nodes).unwrap_err();
        assert_eq!(err.len(), 1);
        assert_eq!(err[0].dimension, "subtasks");
        assert_eq!(err[0].allocated, 6);
        assert_eq!(err[0].root_budget, 5);
    }

    #[test]
    fn multiple_dimensions_can_violate() {
        let root = v(500, 1000, 2);
        let nodes = [v(400, 800, 2), v(400, 800, 1)]; // tokens 800>500, wall 1600>1000, subtasks 3>2
        let err = verify_conservation(&root, &nodes).unwrap_err();
        let dims: Vec<&str> = err.iter().map(|x| x.dimension).collect();
        assert!(dims.contains(&"tokens"));
        assert!(dims.contains(&"wall_ms"));
        assert!(dims.contains(&"subtasks"));
    }

    #[test]
    fn sums_are_overflow_safe() {
        // Two u32::MAX nodes would overflow u32; u64 accumulation must not panic and
        // must report the violation honestly.
        let root = BudgetVector {
            tokens: u32::MAX,
            ..Default::default()
        };
        let big = BudgetVector {
            tokens: u32::MAX,
            ..Default::default()
        };
        let err = verify_conservation(&root, &[big, big]).unwrap_err();
        assert_eq!(err[0].dimension, "tokens");
        assert_eq!(err[0].allocated, u64::from(u32::MAX) * 2);
    }

    #[test]
    fn dim_names_align_with_array() {
        let b = BudgetVector {
            tokens: 1,
            wall_ms: 2,
            subtasks: 3,
            dependencies: 4,
            max_retries: 5,
            attempts_used: 6,
        };
        assert_eq!(b.as_array(), [1, 2, 3, 4, 5, 6]);
        assert_eq!(DIM_NAMES.len(), BUDGET_DIMS);
        assert_eq!(b.as_array().len(), BUDGET_DIMS);
    }
}
