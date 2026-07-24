//! `ReplanRequest` and `RejectedPlan` — typestate circuit breaker for plan iteration.

use crate::plan::failure::FailureReason;
use crate::plan::result::CommitReport;
use crate::plan::schema::GeneratorPlan;
use either::Either;
use std::fmt;
use std::sync::Arc;
use uuid::Uuid;

/// A plan that failed verification or validation and needs replanning.
///
/// Returned from `PlanExecutor<Draft>::verify()` when VGP fails.
/// Callers use `into_draft_or_reject()` to either retry or escalate.
#[derive(Debug)]
pub struct ReplanRequest {
    pub(crate) plan: GeneratorPlan,
    pub(crate) iteration: u8,
    pub(crate) reason: FailureReason,
    pub(crate) failure_history: Vec<FailureReason>,
}

impl fmt::Display for ReplanRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "ReplanRequest(plan_id={}, iter={}, reason={:?})",
            self.plan.plan_id, self.iteration, self.reason
        )
    }
}

impl ReplanRequest {
    /// Construct a `ReplanRequest` for testing — bypasses the private field restriction.
    ///
    /// Production code should never call this; it is used by integration tests
    /// that need to exercise `into_draft_or_reject` without running the full VGP pipeline.
    #[must_use]
    pub fn test_new(plan: GeneratorPlan, iteration: u8, reason: FailureReason) -> Self {
        Self {
            plan,
            iteration,
            reason,
            failure_history: Vec::new(),
        }
    }

    /// Plan ID for logging.
    #[must_use]
    pub fn plan_id(&self) -> Uuid {
        self.plan.plan_id
    }

    /// The reason this replan was triggered.
    #[must_use]
    pub fn reason(&self) -> &FailureReason {
        &self.reason
    }

    /// Current iteration count.
    #[must_use]
    pub fn iteration(&self) -> u8 {
        self.iteration
    }

    /// Converts to either a new Draft executor (retry) or a [`RejectedPlan`] (max iterations hit).
    ///
    /// Forces explicit handling of the circuit breaker — callers cannot ignore exhaustion.
    #[must_use]
    pub fn into_draft_or_reject(
        self,
        max_iterations: u8,
        ctx: Arc<crate::core::context::GeneratorContext>,
    ) -> Either<
        crate::executor::typestate::PlanExecutor<crate::executor::typestate::Draft>,
        RejectedPlan,
    > {
        if self.iteration >= max_iterations {
            Either::Right(RejectedPlan {
                plan_id: self.plan.plan_id,
                iteration: self.iteration,
                failure_history: self.failure_history,
                escalate_to_human: true,
            })
        } else {
            Either::Left(crate::executor::typestate::PlanExecutor::new(
                self.plan,
                ctx,
                self.iteration,
            ))
        }
    }
}

/// A plan that was permanently rejected after exhausting max iterations.
#[derive(Debug, Clone)]
pub struct RejectedPlan {
    /// Unique identifier of the rejected plan.
    pub plan_id: Uuid,
    /// Iteration count at which the circuit breaker tripped.
    pub iteration: u8,
    /// Ordered list of failure reasons accumulated across all retry attempts.
    pub failure_history: Vec<FailureReason>,
    /// Whether the rejection requires human intervention (always `true` here).
    pub escalate_to_human: bool,
}

/// A plan that was successfully committed (terminal success state).
#[derive(Debug, Clone)]
pub struct CompletedPlan {
    /// Unique identifier of the successfully committed plan.
    pub plan_id: Uuid,
    /// Report of the atomic commit: files written and elapsed time.
    pub commit_report: Arc<CommitReport>,
}
