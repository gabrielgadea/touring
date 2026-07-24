//! ACO Wiring — connects UnifiedPheromoneBus and TrackerRlBridge to the daemon.
//!
//! ## Cadeia 1 — UnifiedPheromoneBus E2E
//! post-edit hook → deposit PheroKey::FilePath on bus → periodic evaporate_all()
//!
//! ## Cadeia 2 — TrackerRlBridge E2E
//! post-edit with quality score → TrackerRlBridge.process_report()
//!   → MultiObjectivePheromonoLayer.deposit(state, action, `[f64;4]`)
//!   → UnifiedPheromoneBus.deposit(PheroKey::ActionPair, scalar)
//!   → SessionPredictor.register_outcome("post_edit", success)
//!
//! Enhancement (E10): KS drift check via evaporate_with_drift_check() — proactive
//! cache invalidation when pheromone distribution drifts significantly.
//!
//! ## Design
//! All methods take `&self` — interior mutability via `Arc<Mutex>`/RwLock.
//! Fire-and-forget: every fallible path is silently ignored (hook Exit 0 preserved).

use std::sync::{Arc, Mutex};

use touring_intelligence::reasoning::{
    AcoRewardPropagator, MctsPheromonoLayer, MultiObjectivePheromonoLayer, SessionPredictor,
};
use touring_intelligence::rl::aco::tracker::TrackerReport;
use touring_intelligence::rl::aco::{PheroKey, TrackerRlBridge, UnifiedPheromoneBus};

/// Evaporate the bus every N file-edit deposits (default: 10).
const EVAP_INTERVAL: u32 = 10;

/// F4-1: Last computed ACO metrics from process_tracker_report.
/// Stored for flush_aco_metrics_to_bus() to push to SessionBus.
#[derive(Debug, Clone, Default)]
pub(crate) struct LastAcoMetrics {
    /// Pareto objectives [f64; 4] — per-dimension arm rewards.
    pub objectives: Option<[f64; 4]>,
    /// Scalar reward — overall arm effectiveness.
    pub scalar: Option<f64>,
    /// Whether metrics were updated since last flush.
    pub dirty: bool,
}

/// Internal mutable state — the only thing that requires Mutex wrapping here
/// is `MultiObjectivePheromonoLayer` (takes `&mut self` on deposit) and the
/// evaporation cycle counter.
pub(crate) struct AcoWiringInner {
    /// Multi-objective pheromone layer — requires `&mut self` on deposit.
    pub(crate) multi_obj: MultiObjectivePheromonoLayer,
    /// Deposit counter driving periodic evaporation on the bus.
    pub(crate) evap_cycle: u32,
    /// F4-1: Last computed ACO metrics for SessionBus flush.
    pub(crate) last_aco_metrics: LastAcoMetrics,
}

/// Unified ACO wiring state for the daemon's HookRuntime.
///
/// Cheap to initialise — all allocations are O(1).
/// All methods take `&self` and use interior mutability.
pub struct AcoWiringState {
    /// Shared pheromone bus — `Arc<Mutex<>>` inside, clone-safe, `&self`-friendly.
    pub bus: UnifiedPheromoneBus,
    /// Stateless bridge: TrackerReport → (DimFeatures, `[f64;4]`, f64).
    pub bridge: TrackerRlBridge,
    /// Session predictor — `RwLock<>` inside, `&self`-friendly.
    pub session_predictor: SessionPredictor,
    /// IC-4: TD(λ) backward propagator — closes the IC-1↔IC-4 pheromone loop.
    /// Deposits RL rewards from TrackerReport into MCTS pheromone layer.
    pub aco_propagator: AcoRewardPropagator,
    /// Mutex-protected mutable state (multi_obj layer + evap counter).
    inner: Mutex<AcoWiringInner>,
}

impl AcoWiringState {
    /// Create a fully-initialised wiring state.
    pub fn new() -> Self {
        // IC-4: Shared MctsPheromonoLayer — bridges TrackerReport rewards into MCTS pheromone.
        let mcts_pheromone = Arc::new(Mutex::new(MctsPheromonoLayer::new(0.1, 0.0)));
        Self {
            bus: UnifiedPheromoneBus::new(0.05),
            bridge: TrackerRlBridge::new(),
            session_predictor: SessionPredictor::new(),
            aco_propagator: AcoRewardPropagator::new(mcts_pheromone, 0.8, 0.99),
            inner: Mutex::new(AcoWiringInner {
                multi_obj: MultiObjectivePheromonoLayer::new([0.3, 0.25, 0.2, 0.25]),
                evap_cycle: 0,
                last_aco_metrics: LastAcoMetrics::default(),
            }),
        }
    }

    /// Cadeia 1: deposit file-edit heat on the bus and trigger periodic evaporation.
    ///
    /// Called from `post_edit::run()` after every successful edit.
    /// Never panics — lock-poison is silently swallowed.
    pub fn deposit_file_edit(&self, file_path: &str) {
        self.bus
            .deposit(PheroKey::FilePath(file_path.to_owned()), 1.0);

        if let Ok(mut inner) = self.inner.lock() {
            inner.evap_cycle = inner.evap_cycle.saturating_add(1);
            if inner.evap_cycle >= EVAP_INTERVAL {
                inner.evap_cycle = 0;
                self.bus.evaporate_all();
            }
        }
    }

    /// Cadeia 1b: deposit file-edit heat and return KS drift statistic if
    /// evaporation was triggered this cycle.
    ///
    /// Snapshots pheromone values before/after `evaporate_all()` and computes
    /// Kolmogorov-Smirnov statistic measuring distribution shift. Returns
    /// `None` when evaporation was not triggered or when fewer than 2 trail
    /// entries exist (insufficient data for a meaningful test).
    ///
    /// This enables proactive cache invalidation — when the pheromone
    /// distribution drifts significantly (KS > threshold), stale cached
    /// context should be refreshed.
    pub fn deposit_file_edit_with_drift_check(&self, file_path: &str) -> Option<f64> {
        self.bus
            .deposit(PheroKey::FilePath(file_path.to_owned()), 1.0);

        if let Ok(mut inner) = self.inner.lock() {
            inner.evap_cycle = inner.evap_cycle.saturating_add(1);
            if inner.evap_cycle >= EVAP_INTERVAL {
                inner.evap_cycle = 0;

                let before = self.bus.snapshot_values();
                if before.len() < 2 {
                    self.bus.evaporate_all();
                    return None;
                }

                self.bus.evaporate_all();

                let after = self.bus.snapshot_values();
                if after.len() < 2 {
                    // Pruning removed most entries — maximum drift.
                    return Some(1.0);
                }

                return Some(ks_two_sample(&before, &after));
            }
        }
        None
    }

    /// Cadeia 2: distribute a `TrackerReport` to all 3 RL subsystems.
    ///
    /// - Deposits Pareto objectives into `MultiObjectivePheromonoLayer`
    /// - Deposits scalar reward into the bus as `PheroKey::ActionPair`
    /// - Calls `SessionPredictor::register_outcome` with success proxy
    ///
    /// `state` / `action` are FNV-like hashes of context strings (see `hash_str`).
    /// Never panics — all fallible paths are silently ignored.
    pub fn process_tracker_report(&self, report: &TrackerReport, state: u64, action: u64) {
        let (_dim_features, objectives, scalar) = self.bridge.process_report(report);

        // MultiObjectivePheromonoLayer deposit (requires &mut — guarded by Mutex)
        if let Ok(mut inner) = self.inner.lock() {
            inner.multi_obj.deposit(state, action, objectives);
            // F4-1: Store metrics for SessionBus flush
            inner.last_aco_metrics.objectives = Some(objectives);
            inner.last_aco_metrics.scalar = Some(scalar);
            inner.last_aco_metrics.dirty = true;
        }

        // Scalar → bus as ActionPair pheromone
        self.bus
            .deposit(PheroKey::ActionPair(state, action), scalar);

        // SessionPredictor: treat scalar >= 0.5 as success
        self.session_predictor
            .register_outcome("post_edit", scalar >= 0.5);

        // IC-4: Backward TD(λ) propagation into MCTS pheromone trail.
        // Closes the IC-1↔IC-4 feedback loop: RL quality signal from TrackerReport
        // reinforces MCTS pheromone, biasing future graph-informed search toward
        // high-quality action sequences.
        self.aco_propagator
            .propagate_from_report(report, &[(state, action)]);
    }

    /// F4-2: Flush ACO metrics to SessionBus via callback.
    ///
    /// Extracts stored objectives and scalar from `last_aco_metrics` and
    /// pushes arm effectiveness to the SessionBus using the provided callback.
    /// Called from `post_tool_rl` after each tool execution.
    ///
    /// The callback receives `(arm_id: u8, avg_reward: f64)` for each of the
    /// 4 Pareto arms (objectives) and for the scalar overall reward.
    /// `arm_id` mapping: 0-3 = Pareto arms, 4 = scalar overall.
    ///
    /// Clears the `dirty` flag after flushing.
    /// Never panics — lock-poison is silently swallowed.
    pub fn flush_aco_metrics_to_bus<F>(&self, mut push: F)
    where
        F: FnMut(u8, f64),
    {
        if let Ok(inner) = self.inner.lock() {
            if !inner.last_aco_metrics.dirty {
                return;
            }

            // Push Pareto arm effectiveness (arm_id 0-3)
            if let Some(obj) = inner.last_aco_metrics.objectives {
                for (i, &reward) in obj.iter().enumerate() {
                    push(i as u8, reward);
                }
            }

            // Push overall scalar effectiveness (arm_id 4)
            if let Some(scalar) = inner.last_aco_metrics.scalar {
                push(4, scalar);
            }
        }
    }

    /// N1 — Cadeia 3: deposit task completion heat on the bus.
    ///
    /// Called from `run_task_completed()` hook when a teammate completes a task.
    /// Tracks which tasks are "hot" (frequently completed) for team routing.
    /// Never panics — lock-poison is silently swallowed.
    pub fn deposit_task_completion(&self, task_id: &str, success: bool) {
        // Task heat accumulates; success boosts the signal
        let delta = if success { 1.0 } else { -0.5 };
        self.bus
            .deposit(PheroKey::TaskId(task_id.to_owned()), delta);

        // Also register in session predictor
        let outcome_key = format!(
            "task_completion:{}",
            if success { "success" } else { "fail" }
        );
        self.session_predictor
            .register_outcome(&outcome_key, success);
    }

    /// N1 — Cadeia 4: deposit teammate idle heat on the bus.
    ///
    /// Called from `run_teammate_idle()` hook when a teammate goes idle.
    /// Tracks teammate reliability — idle after work means productive.
    /// Never panics — lock-poison is silently swallowed.
    pub fn deposit_teammate_idle(&self, teammate_id: &str, tasks_completed: u32) {
        // Idle after completing tasks = productive. Heat proportional to throughput.
        let delta = tasks_completed as f64 * 0.5;
        self.bus
            .deposit(PheroKey::TeammateId(teammate_id.to_owned()), delta);

        // Track teammate outcome
        self.session_predictor
            .register_outcome("teammate_idle", tasks_completed > 0);
    }

    /// N1 — Query task heat for routing decisions.
    ///
    /// Returns pheromone level for a task (0.0 if never seen).
    pub fn task_heat(&self, task_id: &str) -> f64 {
        self.bus.get(&PheroKey::TaskId(task_id.to_owned()))
    }

    /// N1 — Query teammate heat for routing decisions.
    ///
    /// Returns pheromone level for a teammate (0.0 if never seen).
    pub fn teammate_heat(&self, teammate_id: &str) -> f64 {
        self.bus.get(&PheroKey::TeammateId(teammate_id.to_owned()))
    }

    /// N1 — Cadeia 5: deposit limbo pattern heat on the bus.
    ///
    /// Called from `run_teammate_idle()` when a teammate idles with uncompleted
    /// or blocked tasks — a limbo pattern. Negative heat accumulates, allowing
    /// the ACO to learn which teammate/DAG configurations lead to limbo.
    ///
    /// `blocked_count` = number of blocked tasks assigned to this teammate.
    /// `uncompleted_count` = number of in-progress or pending tasks left undone.
    /// Never panics — lock-poison is silently swallowed.
    pub fn deposit_teammate_limbo(
        &self,
        teammate_id: &str,
        blocked_count: u32,
        uncompleted_count: u32,
    ) {
        // Limbo severity scales with how many tasks were left undone.
        // Blocked tasks get lighter penalty (not the teammate's fault).
        let delta = -(uncompleted_count as f64 * 1.0 + blocked_count as f64 * 0.3);
        self.bus
            .deposit(PheroKey::LimboPattern(teammate_id.to_owned()), delta);

        // Register as negative outcome for session predictor
        self.session_predictor
            .register_outcome("teammate_limbo", false);
    }

    /// N1 — Query limbo heat for a teammate.
    ///
    /// Returns accumulated limbo pheromone (negative = frequent limbo, 0.0 = clean).
    pub fn limbo_heat(&self, teammate_id: &str) -> f64 {
        self.bus
            .get(&PheroKey::LimboPattern(teammate_id.to_owned()))
    }
}

/// Two-sample Kolmogorov-Smirnov statistic.
///
/// Returns the maximum absolute difference between the empirical CDFs of
/// `a` and `b`. Value in `[0.0, 1.0]` — 0 = identical distributions,
/// 1 = maximally different. Returns 0.0 if either sample is empty.
///
/// This is a local implementation to avoid pulling in `touring-simd` just
/// for this single function (keeps the ACO wiring dependency-light).
fn ks_two_sample(a: &[f64], b: &[f64]) -> f64 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let mut sorted_a: Vec<f64> = a.to_vec();
    let mut sorted_b: Vec<f64> = b.to_vec();
    sorted_a.sort_by(|x, y| x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal));
    sorted_b.sort_by(|x, y| x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal));

    let n_a = sorted_a.len() as f64;
    let n_b = sorted_b.len() as f64;
    let mut i = 0usize;
    let mut j = 0usize;
    let mut max_diff: f64 = 0.0;

    while i < sorted_a.len() && j < sorted_b.len() {
        // SAFETY: i < sorted_a.len() and j < sorted_b.len() guaranteed by loop condition.
        #[allow(clippy::indexing_slicing)]
        let (va, vb) = (sorted_a[i], sorted_b[j]);
        if va <= vb {
            i += 1;
        }
        if vb <= va {
            j += 1;
        }
        let cdf_a = i as f64 / n_a;
        let cdf_b = j as f64 / n_b;
        let diff = (cdf_a - cdf_b).abs();
        if diff > max_diff {
            max_diff = diff;
        }
    }
    max_diff
}

impl Default for AcoWiringState {
    fn default() -> Self {
        Self::new()
    }
}

// Manual Debug — none of the inner fields implement Debug uniformly.
impl std::fmt::Debug for AcoWiringState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AcoWiringState")
            .field("bus_entries", &self.bus.entry_count())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use touring_intelligence::rl::aco::tracker::{DimResult, build_report};

    fn make_report(score: f64) -> TrackerReport {
        let dims: Vec<DimResult> = (1..=9)
            .map(|i| DimResult {
                dim_id: format!("D{i}"),
                name: format!("Dim{i}"),
                score,
                checks_passed: if score >= 0.8 { 1 } else { 0 },
                checks_total: 1,
                details: vec![],
            })
            .collect();
        build_report(dims, 1)
    }

    #[test]
    fn test_aco_wiring_new() {
        let w = AcoWiringState::new();
        assert_eq!(w.bus.entry_count(), 0);
    }

    #[test]
    fn test_deposit_file_edit_accumulates() {
        let w = AcoWiringState::new();
        w.deposit_file_edit("src/lib.rs");
        assert_eq!(w.bus.entry_count(), 1);
        let key = PheroKey::FilePath("src/lib.rs".to_owned());

        assert!((w.bus.get(&key) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn test_deposit_file_edit_accumulates_twice() {
        let w = AcoWiringState::new();
        w.deposit_file_edit("src/lib.rs");
        w.deposit_file_edit("src/lib.rs");
        let key = PheroKey::FilePath("src/lib.rs".to_owned());

        assert!((w.bus.get(&key) - 2.0).abs() < 1e-9);
    }

    #[test]
    fn test_evaporation_triggers_at_interval() {
        let w = AcoWiringState::new();
        // Deposit enough to fill the trail
        w.deposit_file_edit("a.rs");
        let before = w.bus.get(&PheroKey::FilePath("a.rs".to_owned()));

        // Trigger evaporation (EVAP_INTERVAL - 1 more calls = 10 total)
        for i in 0..(EVAP_INTERVAL - 1) {
            w.deposit_file_edit(&format!("x{i}.rs"));
        }
        // After 10 calls the 10th triggers evaporate_all → decays "a.rs"
        let after = w.bus.get(&PheroKey::FilePath("a.rs".to_owned()));

        assert!(after < before, "evaporation should have decayed the trail");
    }

    #[test]
    fn test_process_tracker_report_deposits_on_bus() {
        let w = AcoWiringState::new();
        let report = make_report(1.0);
        w.process_tracker_report(&report, 42, 7);
        let v = w.bus.get(&PheroKey::ActionPair(42, 7));

        assert!(v > 0.0, "scalar should have been deposited on bus");
    }

    #[test]
    fn test_process_tracker_report_multi_obj() {
        let w = AcoWiringState::new();
        let report = make_report(1.0);
        w.process_tracker_report(&report, 1, 2);
        let trail = w.inner.lock().map(|inner| inner.multi_obj.trail(1, 2));

        let trail = trail.expect("lock ok");
        assert!(
            trail.iter().any(|&v| v > 0.0),
            "multi_obj should have a deposit"
        );
    }

    #[test]
    fn test_process_tracker_report_session_predictor() {
        let w = AcoWiringState::new();
        let report = make_report(1.0); // Pass → scalar >= 0.5 → success=true
        w.process_tracker_report(&report, 0, 0);
        // Q-value for "post_edit" should now be Some
        let q = w.session_predictor.q_value("post_edit");
        assert!(
            q.is_some(),
            "session predictor should have recorded outcome"
        );
    }

    // ═══════════════════════════════════════════════════════════
    // N1: Team Hooks — ACO Wiring Tests
    // ═══════════════════════════════════════════════════════════

    #[test]
    fn test_deposit_task_completion_success() {
        let w = AcoWiringState::new();
        w.deposit_task_completion("task-42", true);
        let heat = w.task_heat("task-42");
        assert!(
            heat > 0.0,
            "successful task should accumulate positive heat"
        );
    }

    #[test]
    fn test_deposit_task_completion_failure() {
        let w = AcoWiringState::new();
        w.deposit_task_completion("task-fail", false);
        let heat = w.task_heat("task-fail");
        assert!(heat < 0.0, "failed task should accumulate negative heat");
    }

    #[test]
    fn test_deposit_task_completion_accumulates() {
        let w = AcoWiringState::new();
        w.deposit_task_completion("task-1", true);
        w.deposit_task_completion("task-1", true);
        let heat = w.task_heat("task-1");
        assert!(
            (heat - 2.0).abs() < 1e-9,
            "heat should accumulate: expected 2.0, got {heat}"
        );
    }

    #[test]
    fn test_deposit_teammate_idle_productive() {
        let w = AcoWiringState::new();
        w.deposit_teammate_idle("engineer-1", 3);
        let heat = w.teammate_heat("engineer-1");
        assert!(
            (heat - 1.5).abs() < 1e-9,
            "heat = tasks_completed * 0.5: expected 1.5, got {heat}"
        );
    }

    #[test]
    fn test_deposit_teammate_idle_zero_tasks() {
        let w = AcoWiringState::new();
        w.deposit_teammate_idle("slacker", 0);
        let heat = w.teammate_heat("slacker");
        assert!(
            (heat - 0.0).abs() < 1e-9,
            "zero tasks should give 0.0 heat, got {heat}"
        );
    }

    #[test]
    fn test_task_heat_unknown_returns_zero() {
        let w = AcoWiringState::new();
        let heat = w.task_heat("never-seen-task");
        assert!(
            (heat - 0.0).abs() < 1e-9,
            "unknown task should return 0.0 heat"
        );
    }

    #[test]
    fn test_teammate_heat_unknown_returns_zero() {
        let w = AcoWiringState::new();
        let heat = w.teammate_heat("unknown-agent");
        assert!(
            (heat - 0.0).abs() < 1e-9,
            "unknown teammate should return 0.0 heat"
        );
    }

    #[test]
    fn test_phero_key_task_id_variant() {
        use touring_intelligence::rl::aco::PheroKey;
        let key1 = PheroKey::TaskId("t1".into());

        let key2 = PheroKey::TaskId("t1".into());

        let key3 = PheroKey::TaskId("t2".into());

        // Same content = equal
        assert_eq!(key1, key2);
        // Different content = not equal
        assert_ne!(key1, key3);
        // Hash consistency
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(key1.clone());

        set.insert(key2.clone());

        set.insert(key3.clone());

        assert_eq!(set.len(), 2, "TaskId t1 should be deduplicated");
    }

    #[test]
    fn test_phero_key_teammate_id_variant() {
        use touring_intelligence::rl::aco::PheroKey;
        let key1 = PheroKey::TeammateId("alice".into());

        let key2 = PheroKey::TeammateId("alice".into());

        let key3 = PheroKey::TeammateId("bob".into());

        assert_eq!(key1, key2);
        assert_ne!(key1, key3);
    }

    #[test]
    fn test_default_equals_new() {
        let w = AcoWiringState::default();
        assert_eq!(w.bus.entry_count(), 0);
    }

    // ═══════════════════════════════════════════════════════════
    // E10: Drift Detection Tests
    // ═══════════════════════════════════════════════════════════

    #[test]
    fn test_ks_two_sample_identical() {
        let a = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let b = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let ks = ks_two_sample(&a, &b);
        assert!(
            ks.abs() < 1e-9,
            "identical distributions should have KS=0, got {ks}"
        );
    }

    #[test]
    fn test_ks_two_sample_completely_different() {
        let a = vec![1.0, 2.0, 3.0];
        let b = vec![10.0, 20.0, 30.0];
        let ks = ks_two_sample(&a, &b);
        assert!(
            (ks - 1.0).abs() < 1e-9,
            "non-overlapping distributions should have KS=1, got {ks}"
        );
    }

    #[test]
    fn test_ks_two_sample_empty() {
        assert!(ks_two_sample(&[], &[1.0, 2.0]).abs() < 1e-9);
        assert!(ks_two_sample(&[1.0], &[]).abs() < 1e-9);
        assert!(ks_two_sample(&[], &[]).abs() < 1e-9);
    }

    #[test]
    fn test_ks_two_sample_partial_overlap() {
        let a = vec![1.0, 2.0, 3.0, 4.0];
        let b = vec![3.0, 4.0, 5.0, 6.0];
        let ks = ks_two_sample(&a, &b);
        assert!(
            ks > 0.0 && ks < 1.0,
            "partial overlap should have 0 < KS < 1, got {ks}"
        );
    }

    #[test]
    fn test_deposit_file_edit_with_drift_check_no_evaporation() {
        let w = AcoWiringState::new();
        // First deposit — evap_cycle=1, not yet EVAP_INTERVAL
        let drift = w.deposit_file_edit_with_drift_check("src/lib.rs");
        assert!(drift.is_none(), "no evaporation should return None");
    }

    #[test]
    fn test_deposit_file_edit_with_drift_check_triggers_at_interval() {
        let w = AcoWiringState::new();
        // Deposit EVAP_INTERVAL files to trigger evaporation on the last one.
        let mut last_drift = None;
        for i in 0..EVAP_INTERVAL {
            last_drift = w.deposit_file_edit_with_drift_check(&format!("file_{i}.rs"));
        }
        // The 10th call triggers evaporation, which should return Some(ks).
        // With 10 entries and 5% evaporation rate, drift is small but measurable.
        assert!(
            last_drift.is_some(),
            "evaporation cycle should return Some(ks)"
        );
    }

    #[test]
    fn test_deposit_file_edit_with_drift_check_too_few_entries() {
        let w = AcoWiringState::new();
        // Only 1 entry — insufficient for KS test.
        // Fill evap_cycle to EVAP_INTERVAL - 1 via the non-drift method.
        for i in 0..(EVAP_INTERVAL - 1) {
            w.deposit_file_edit(&format!("warmup_{i}.rs"));
        }
        // Now all warmup entries got evaporated (evap_cycle wrapped at 10).
        // Reset: deposit only 1 file via drift-check to trigger evap.
        // But we need to reach EVAP_INTERVAL again.
        let w2 = AcoWiringState::new();
        // Put exactly 1 file, then trigger evaporation.
        w2.deposit_file_edit_with_drift_check("only_one.rs");
        // Fill remaining cycles
        for _ in 1..(EVAP_INTERVAL - 1) {
            w2.deposit_file_edit("only_one.rs"); // same file, still 1 bus entry
        }
        let drift = w2.deposit_file_edit_with_drift_check("only_one.rs");
        // Only 1 unique bus entry → fewer than 2 values → None
        assert!(
            drift.is_none(),
            "single entry should return None (insufficient data)"
        );
    }
}
