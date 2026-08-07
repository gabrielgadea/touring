//! Credit-assignment ledger — joins a choice back to the outcome it produced.
//!
//! # The defect this exists to fix
//!
//! `OnlineRLEngine::process_reward` used to pick the arm to credit with
//! `djb2_hash(tool_name) % NUM_ARMS`, while the arm that actually *made* the
//! decision came from `LinUCBBandit::select_arm` at a completely different call
//! site and was then dropped on the floor. The reward therefore never reached
//! the arm whose choice produced it — an open loop, which is the one thing a
//! bandit cannot tolerate. Worse, distinct tools collide into the same bucket
//! of 8, so the hash also merged unrelated outcomes.
//!
//! # Two consumers, one discipline
//!
//! The same open loop exists wherever a system *chooses* and only later learns
//! whether the choice was good, so [`Ledger`] is generic over what was chosen:
//!
//! - [`DecisionLedger`] — which bandit **arm** was selected (payload
//!   [`ArmChoice`]).
//! - [`CaseLedger`] — which memory **cases** a recall served (payload
//!   `Vec<String>` of entry keys). This is the Touring form of Memento's
//!   episodic memory `D = {(s, c, Q)}` (arXiv 2508.16153, Eq. 9): the retrieval
//!   is recorded when it happens, and the task's verdict is joined back onto
//!   the exact cases that informed it.
//!
//! The case side matters because Touring has **no other usable utility signal**:
//! `access_count` is incremented both by writes (`INSERT OR REPLACE … + 1`) and
//! by exact-key reads (`RlMemory::get`), while the real recall path (FTS5 / LIKE
//! / ANN / TF-IDF) never touches it. A counter that sums writes and by-key
//! lookups cannot answer "did this case help?" — only attribution can.
//!
//! Unclaimed entries are bounded by `capacity` and evicted oldest-first, so a
//! caller that records without ever crediting cannot grow the ledger without
//! limit. `unclaimed_evictions` is the live measure of how open the loop still
//! is at that call site.
//!
//! # Determinism
//!
//! Ordering uses a monotonic sequence counter, never a clock — the ledger's
//! behaviour is a pure function of the call sequence (REGRA #17).

use ndarray::Array1;
use std::collections::HashMap;

/// What a bandit selection chose, and the context it was conditioned on.
#[derive(Debug, Clone)]
pub struct ArmChoice {
    /// The arm `select_arm` actually chose.
    pub arm: usize,
    /// The feature vector that choice was conditioned on.
    pub features: Array1<f64>,
}

/// A choice awaiting its outcome.
#[derive(Debug, Clone)]
pub struct Pending<P> {
    /// What was chosen.
    pub payload: P,
    /// Monotonic insertion order, used for oldest-first eviction.
    seq: u64,
}

impl<P> Pending<P> {
    /// Insertion order of this entry (monotonic, clock-free).
    pub fn seq(&self) -> u64 {
        self.seq
    }
}

/// A bandit selection awaiting its reward.
pub type PendingDecision = Pending<ArmChoice>;

/// Bounded map of choices that have been made but not yet credited.
#[derive(Debug)]
pub struct Ledger<P> {
    pending: HashMap<String, Pending<P>>,
    capacity: usize,
    next_seq: u64,
    credited: u64,
    unclaimed_evictions: u64,
}

/// Bandit arms awaiting their reward, keyed by tool name.
pub type DecisionLedger = Ledger<ArmChoice>;

/// Memory cases served by a recall, keyed by the query they answered.
pub type CaseLedger = Ledger<Vec<String>>;

impl<P> Ledger<P> {
    /// Default number of in-flight choices retained.
    ///
    /// Outcomes normally follow their choice within a few steps; anything older
    /// is almost certainly a choice whose outcome was never reported.
    pub const DEFAULT_CAPACITY: usize = 256;

    /// Create a ledger holding at most `capacity` unclaimed choices.
    ///
    /// A `capacity` of 0 is raised to 1 so `record` always stores something —
    /// a silently no-op ledger would reintroduce the open loop it exists to close.
    pub fn new(capacity: usize) -> Self {
        Self {
            pending: HashMap::new(),
            capacity: capacity.max(1),
            next_seq: 0,
            credited: 0,
            unclaimed_evictions: 0,
        }
    }

    /// Record that `payload` was chosen for `key`.
    ///
    /// Re-recording the same key replaces the older choice: the newer one is
    /// what an incoming outcome refers to.
    pub fn record(&mut self, key: impl Into<String>, payload: P) {
        let seq = self.next_seq;
        self.next_seq = self.next_seq.wrapping_add(1);
        self.pending.insert(key.into(), Pending { payload, seq });
        self.evict_over_capacity();
    }

    /// Claim the choice recorded for `key`, if any.
    ///
    /// Returns `None` when nothing was recorded — the caller must then fall
    /// back to its legacy attribution, never to a *different* choice's credit.
    pub fn take(&mut self, key: &str) -> Option<Pending<P>> {
        let found = self.pending.remove(key);
        if found.is_some() {
            self.credited = self.credited.saturating_add(1);
        }
        found
    }

    /// Number of outcomes successfully joined back onto their choice.
    pub fn credited_count(&self) -> u64 {
        self.credited
    }

    /// Choices dropped without ever receiving an outcome.
    ///
    /// A steadily rising count means some call site records choices but never
    /// reports what happened — the loop is open at that site.
    pub fn unclaimed_evictions(&self) -> u64 {
        self.unclaimed_evictions
    }

    /// Choices currently awaiting an outcome.
    pub fn pending_len(&self) -> usize {
        self.pending.len()
    }

    /// True when no choice is awaiting an outcome.
    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    /// Drop the oldest entries until the ledger fits `capacity`.
    fn evict_over_capacity(&mut self) {
        while self.pending.len() > self.capacity {
            let Some(oldest) = self
                .pending
                .iter()
                .min_by_key(|(_, d)| d.seq)
                .map(|(k, _)| k.clone())
            else {
                break;
            };
            self.pending.remove(&oldest);
            self.unclaimed_evictions = self.unclaimed_evictions.saturating_add(1);
        }
    }
}

impl<P> Default for Ledger<P> {
    fn default() -> Self {
        Self::new(Self::DEFAULT_CAPACITY)
    }
}

/// Blend an observed reward into a case's running value.
///
/// This is the practical form of Memento's episodic-control estimate
/// (arXiv 2508.16153, Eq. 9): the value of a case is the average of the outcomes
/// of past interactions with it, not a single verdict. An exponential moving
/// average keeps that average online and bounded in memory — one number per
/// case rather than a growing list.
///
/// `prior` is `None` for a case that was never credited, and the first
/// observation then becomes the value outright: averaging against an assumed
/// 0.0 would tar a first success as half a failure.
pub fn blend_case_value(prior: Option<f64>, observed: f64, alpha: f64) -> f64 {
    let observed = observed.clamp(-1.0, 1.0);
    let alpha = alpha.clamp(0.0, 1.0);
    match prior {
        None => observed,
        Some(p) => (alpha * observed + (1.0 - alpha) * p).clamp(-1.0, 1.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feats(v: f64) -> Array1<f64> {
        Array1::from_elem(4, v)
    }

    fn choice(arm: usize, v: f64) -> ArmChoice {
        ArmChoice {
            arm,
            features: feats(v),
        }
    }

    #[test]
    fn a_recorded_decision_is_credited_to_the_arm_that_made_it() {
        let mut ledger = DecisionLedger::new(8);
        ledger.record("Edit", choice(5, 0.25));

        let claimed = ledger.take("Edit").expect("decision was recorded");
        assert_eq!(
            claimed.payload.arm, 5,
            "credit must go to the arm that decided"
        );
        assert!((claimed.payload.features[0] - 0.25).abs() < f64::EPSILON);
        assert_eq!(ledger.credited_count(), 1);
    }

    #[test]
    fn claiming_consumes_the_decision_so_one_choice_is_never_rewarded_twice() {
        let mut ledger = DecisionLedger::new(8);
        ledger.record("Edit", choice(5, 1.0));

        assert!(ledger.take("Edit").is_some());
        assert!(
            ledger.take("Edit").is_none(),
            "a second reward must not re-credit the same selection"
        );
        assert_eq!(ledger.credited_count(), 1, "only one genuine credit");
    }

    #[test]
    fn an_unrecorded_key_yields_none_so_the_caller_falls_back() {
        let mut ledger = DecisionLedger::new(8);
        ledger.record("Edit", choice(5, 1.0));

        assert!(
            ledger.take("Bash").is_none(),
            "an unrelated tool must not claim another tool's decision"
        );
        assert_eq!(ledger.pending_len(), 1, "Edit's decision is still pending");
    }

    #[test]
    fn re_recording_a_key_supersedes_the_older_selection() {
        let mut ledger = DecisionLedger::new(8);
        ledger.record("Edit", choice(1, 0.1));
        ledger.record("Edit", choice(7, 0.9));

        let claimed = ledger.take("Edit").expect("decision present");
        assert_eq!(
            claimed.payload.arm, 7,
            "the newer selection is the live one"
        );
        assert!(ledger.is_empty());
    }

    #[test]
    fn capacity_evicts_oldest_first_and_counts_the_open_loop() {
        let mut ledger = DecisionLedger::new(2);
        ledger.record("first", choice(0, 0.0));
        ledger.record("second", choice(1, 0.0));
        ledger.record("third", choice(2, 0.0));

        assert_eq!(ledger.pending_len(), 2, "capacity is respected");
        assert!(
            ledger.take("first").is_none(),
            "the oldest unclaimed decision is the one evicted"
        );
        assert!(ledger.take("second").is_some());
        assert!(ledger.take("third").is_some());
        assert_eq!(
            ledger.unclaimed_evictions(),
            1,
            "an evicted decision is a site that never reported its outcome"
        );
    }

    #[test]
    fn zero_capacity_is_raised_so_the_ledger_never_silently_no_ops() {
        let mut ledger = DecisionLedger::new(0);
        ledger.record("Edit", choice(3, 0.5));
        assert_eq!(
            ledger.take("Edit").map(|d| d.payload.arm),
            Some(3),
            "capacity 0 must not degrade back into an open loop"
        );
    }

    #[test]
    fn ordering_is_by_call_sequence_not_by_a_clock() {
        let mut ledger = DecisionLedger::new(8);
        ledger.record("a", choice(0, 0.0));
        ledger.record("b", choice(0, 0.0));

        let a = ledger.take("a").expect("a present");
        let b = ledger.take("b").expect("b present");
        assert!(a.seq() < b.seq(), "seq must reflect insertion order");
    }

    // ── Case attribution ────────────────────────────────────────────────────

    #[test]
    fn a_recall_can_be_credited_back_to_the_exact_cases_it_served() {
        let mut ledger = CaseLedger::new(8);
        ledger.record(
            "how to page a large file",
            vec!["outcome:read:a:failure".into(), "lesson:paging".into()],
        );

        let served = ledger
            .take("how to page a large file")
            .expect("recall was recorded");
        assert_eq!(served.payload.len(), 2);
        assert!(served.payload.contains(&"lesson:paging".to_string()));
    }

    /// The two ledgers are independent instances of the same discipline.
    #[test]
    fn case_and_decision_ledgers_do_not_share_state() {
        let mut cases = CaseLedger::new(8);
        let mut decisions = DecisionLedger::new(8);
        cases.record("q", vec!["k".into()]);

        assert!(
            decisions.take("q").is_none(),
            "a case recall must never be claimable as a bandit decision"
        );
        assert_eq!(cases.pending_len(), 1);
    }

    /// A first observation IS the value — never averaged against an assumed 0.
    #[test]
    fn the_first_credit_becomes_the_value_outright() {
        assert!((blend_case_value(None, 1.0, 0.3) - 1.0).abs() < f64::EPSILON);
        assert!((blend_case_value(None, 0.0, 0.3) - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn later_credits_move_the_value_toward_the_observation() {
        let after_one = blend_case_value(Some(1.0), 0.0, 0.5);
        assert!(
            (after_one - 0.5).abs() < f64::EPSILON,
            "half-weighted blend of 1.0 and 0.0 is 0.5, got {after_one}"
        );
        // And it must MOVE — a blend that ignored the observation would still
        // pass the bounds checks below.
        assert!(after_one < 1.0, "a bad outcome must lower a case's value");
    }

    #[test]
    fn blended_values_stay_inside_the_reward_band() {
        assert!((blend_case_value(Some(1.0), 9.9, 1.0) - 1.0).abs() < f64::EPSILON);
        assert!((blend_case_value(Some(-1.0), -9.9, 1.0) + 1.0).abs() < f64::EPSILON);
    }
}
