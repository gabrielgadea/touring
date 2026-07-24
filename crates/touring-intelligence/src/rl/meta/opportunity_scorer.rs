//! Evolution opportunity scorer for Touring self-improvement.
//!
//! Analyzes telemetry signals (RL convergence, wiring gaps, drift, performance)
//! and produces a ranked list of [`EvolutionCandidate`] improvements.
//!
//! # Pipeline
//!
//! ```text
//! Telemetry signals (JSON from CLI)
//!         |
//!         v
//! OpportunityScorer::analyze()
//!         |
//!         v
//! Vec<EvolutionCandidate>  sorted by composite_score DESC
//!         |
//!         v
//! /touring-evolve skill -> subagent implementation
//! ```

use std::collections::HashMap;

/// Category of evolution opportunity.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum OpportunityCategory {
    /// Pub symbol with no consumers (wiring orphan).
    IntegrationGap,
    /// QTable state/arm with persistently high TD error.
    ConvergenceIssue,
    /// Hook or code path with high latency (>5ms warm).
    PerformanceHotspot,
    /// Two modules that logically should integrate but don't.
    MissingIntegration,
    /// Drift detected in a metric stream with no response handler.
    DriftResponse,
    /// Module with fewer than MIN_TESTS_PER_MODULE tests.
    TestCoverageGap,
    /// Self-optimizer proposed Reset but was ignored.
    HyperparamDegradation,
    /// Cognitive engine degraded or operating below healthy baseline.
    CognitiveBottleneck,
    /// Parser cache hit rate below minimum efficiency threshold.
    CacheInefficiency,
    /// Gotchas that keep recurring without resolution (high decay_score).
    GotchaRecurrence,
}

impl OpportunityCategory {
    /// Human-readable label.
    pub fn label(&self) -> &'static str {
        match self {
            Self::IntegrationGap => "Integration Gap",
            Self::ConvergenceIssue => "Convergence Issue",
            Self::PerformanceHotspot => "Performance Hotspot",
            Self::MissingIntegration => "Missing Integration",
            Self::DriftResponse => "Drift Response",
            Self::TestCoverageGap => "Test Coverage Gap",
            Self::HyperparamDegradation => "Hyperparam Degradation",
            Self::CognitiveBottleneck => "Cognitive Bottleneck",
            Self::CacheInefficiency => "Cache Inefficiency",
            Self::GotchaRecurrence => "Gotcha Recurrence",
        }
    }

    /// Base impact weight for this category (0.0-1.0).
    pub fn base_impact(&self) -> f64 {
        match self {
            Self::IntegrationGap => 0.6,
            Self::ConvergenceIssue => 0.8,
            Self::PerformanceHotspot => 0.9,
            Self::MissingIntegration => 0.7,
            Self::DriftResponse => 0.85,
            Self::TestCoverageGap => 0.5,
            Self::HyperparamDegradation => 0.75,
            Self::CognitiveBottleneck => 0.75,
            Self::CacheInefficiency => 0.65,
            Self::GotchaRecurrence => 0.55,
        }
    }

    /// Base feasibility for this category (0.0-1.0).
    pub fn base_feasibility(&self) -> f64 {
        match self {
            Self::IntegrationGap => 0.9,
            Self::ConvergenceIssue => 0.7,
            Self::PerformanceHotspot => 0.6,
            Self::MissingIntegration => 0.8,
            Self::DriftResponse => 0.85,
            Self::TestCoverageGap => 0.95,
            Self::HyperparamDegradation => 0.9,
            Self::CognitiveBottleneck => 0.70,
            Self::CacheInefficiency => 0.85,
            Self::GotchaRecurrence => 0.95,
        }
    }
}

/// A single ranked improvement opportunity for the Touring workspace.
#[derive(Debug, Clone)]
pub struct EvolutionCandidate {
    /// Unique ID for this candidate (category:target).
    pub id: String,
    /// Opportunity category.
    pub category: OpportunityCategory,
    /// Short human-readable title.
    pub title: String,
    /// Detailed description of what to improve and why.
    pub description: String,
    /// Impact score (0.0-1.0): how much improvement this would deliver.
    pub impact_score: f64,
    /// Feasibility score (0.0-1.0): how easy/safe to implement.
    pub feasibility_score: f64,
    /// Composite score = impact * feasibility. Used for ranking.
    pub composite_score: f64,
    /// Telemetry evidence that identified this opportunity.
    pub signals: Vec<String>,
    /// Suggested implementation approach (free text).
    pub suggested_implementation: String,
}

impl EvolutionCandidate {
    /// Construct an `EvolutionCandidate` from its id, category, descriptive fields, and signals.
    pub fn new(
        id: impl Into<String>,
        category: OpportunityCategory,
        title: impl Into<String>,
        description: impl Into<String>,
        impact_modifier: f64,
        signals: Vec<String>,
        suggested_implementation: impl Into<String>,
    ) -> Self {
        let impact = (category.base_impact() * impact_modifier).clamp(0.0, 1.0);
        let feasibility = category.base_feasibility();
        let composite = impact * feasibility;
        Self {
            id: id.into(),
            category,
            title: title.into(),
            description: description.into(),
            impact_score: impact,
            feasibility_score: feasibility,
            composite_score: composite,
            signals,
            suggested_implementation: suggested_implementation.into(),
        }
    }
}

/// Telemetry input for the opportunity scorer.
///
/// All fields are optional -- scorer gracefully handles missing data.
#[derive(Debug, Default)]
pub struct TelemetrySnapshot {
    /// LinUCB arm stats: arm_index -> (pulls, cumulative_reward).
    pub linucb_arms: HashMap<usize, (u64, f64)>,
    /// QTable convergence: mean absolute TD error over last N updates.
    pub mean_td_error: Option<f64>,
    /// EMA reward from OnlineRLEngine.
    pub ema_reward: Option<f64>,
    /// Total RL update count.
    pub rl_update_count: u64,
    /// Wiring orphan count (pub symbols with no consumers).
    pub wiring_orphan_count: usize,
    /// Names of orphaned symbols for candidate generation.
    pub wiring_orphans: Vec<String>,
    /// Modules with integration_score < threshold.
    pub low_integration_modules: Vec<(String, f64)>,
    /// Whether drift was detected in any monitored stream.
    pub drift_detected: bool,
    /// Number of SelfOptimizer Reset events in the last eval window.
    pub optimizer_reset_count: u64,
    /// Modules with test count below minimum.
    pub undertested_modules: Vec<String>,
    /// Average hook latency in ms (warm path).
    pub avg_hook_latency_ms: Option<f64>,
    /// Number of gotchas with decay_score > 0.7 (recurring issues).
    pub recurring_gotcha_count: usize,
    /// Parser/incremental cache hit rate (0.0-1.0). None if data unavailable.
    pub parser_cache_hit_rate: Option<f64>,
    /// Whether a cognitive engine is degraded or operating below baseline.
    pub cognitive_engine_degraded: bool,
}

/// Minimum integration score below which a module is considered under-integrated.
const LOW_INTEGRATION_THRESHOLD: f64 = 0.5;
/// Minimum tests per module.
const MIN_TESTS_PER_MODULE: usize = 3;
/// TD error threshold above which convergence is considered poor.
const HIGH_TD_ERROR_THRESHOLD: f64 = 0.5;
/// Hook latency threshold in ms above which a hotspot is flagged.
const HIGH_LATENCY_THRESHOLD_MS: f64 = 10.0;
/// Parser cache hit rate below which CacheInefficiency is flagged.
const MIN_CACHE_HIT_RATE: f64 = 0.70;
/// Gotcha recurrence count above which GotchaRecurrence is flagged.
const MAX_GOTCHA_RECURRENCE: usize = 5;

/// Analyzes telemetry and produces ranked evolution candidates.
#[derive(Debug, Default)]
pub struct OpportunityScorer;

impl OpportunityScorer {
    /// Construct a stateless `OpportunityScorer`.
    pub fn new() -> Self {
        Self
    }

    /// Analyze telemetry and return candidates sorted by composite_score DESC.
    pub fn analyze(&self, snapshot: &TelemetrySnapshot) -> Vec<EvolutionCandidate> {
        let mut candidates = Vec::new();

        self.score_wiring_gaps(snapshot, &mut candidates);
        self.score_convergence(snapshot, &mut candidates);
        self.score_performance(snapshot, &mut candidates);
        self.score_drift_response(snapshot, &mut candidates);
        self.score_test_coverage(snapshot, &mut candidates);
        self.score_hyperparam_degradation(snapshot, &mut candidates);
        self.score_low_integration(snapshot, &mut candidates);
        self.score_cache_inefficiency(snapshot, &mut candidates);
        self.score_gotcha_recurrence(snapshot, &mut candidates);
        self.score_cognitive_bottleneck(snapshot, &mut candidates);

        candidates.sort_by(|a, b| {
            b.composite_score
                .partial_cmp(&a.composite_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        candidates
    }

    fn score_wiring_gaps(
        &self,
        snapshot: &TelemetrySnapshot,
        candidates: &mut Vec<EvolutionCandidate>,
    ) {
        if snapshot.wiring_orphan_count == 0 {
            return;
        }
        let impact_mod = (snapshot.wiring_orphan_count as f64 / 10.0).min(1.0) + 0.5;
        let orphan_list = snapshot.wiring_orphans.join(", ");
        candidates.push(EvolutionCandidate::new(
            "integration:wiring_orphans",
            OpportunityCategory::IntegrationGap,
            format!(
                "{} pub symbols with no consumers",
                snapshot.wiring_orphan_count
            ),
            format!(
                "These public symbols are exported but never imported: {}. \
                Either wire them into consumers or remove them to reduce dead code.",
                orphan_list
            ),
            impact_mod,
            vec![
                format!("wiring_orphan_count = {}", snapshot.wiring_orphan_count),
                format!("orphans: {orphan_list}"),
            ],
            "Use `touring wiring orphans -j` to get full list. \
             For each orphan: (a) find the natural consumer and add the import, \
             or (b) mark as internal with `pub(crate)` to remove from public API.",
        ));
    }

    fn score_convergence(
        &self,
        snapshot: &TelemetrySnapshot,
        candidates: &mut Vec<EvolutionCandidate>,
    ) {
        let td_error = match snapshot.mean_td_error {
            Some(e) if e > HIGH_TD_ERROR_THRESHOLD => e,
            _ => return,
        };
        let impact_mod = (td_error / HIGH_TD_ERROR_THRESHOLD).clamp(0.5, 1.5);
        candidates.push(EvolutionCandidate::new(
            "convergence:high_td_error",
            OpportunityCategory::ConvergenceIssue,
            format!("QTable not converging (mean TD error = {td_error:.3})"),
            format!(
                "The QTable mean TD error is {td_error:.3}, above the threshold {HIGH_TD_ERROR_THRESHOLD}. \
                 This suggests the learning rate (alpha) is too low, state encoding is too coarse, \
                 or the reward signal is too noisy."
            ),
            impact_mod,
            vec![
                format!("mean_td_error = {td_error:.3}"),
                format!("rl_update_count = {}", snapshot.rl_update_count),
            ],
            "Options: (1) Increase LearningParams.alpha (0.1->0.3), \
             (2) Refine state encoding to distinguish more contexts, \
             (3) Add eligibility trace lambda tuning via SelfOptimizer.",
        ));
    }

    fn score_performance(
        &self,
        snapshot: &TelemetrySnapshot,
        candidates: &mut Vec<EvolutionCandidate>,
    ) {
        let latency = match snapshot.avg_hook_latency_ms {
            Some(l) if l > HIGH_LATENCY_THRESHOLD_MS => l,
            _ => return,
        };
        let impact_mod = (latency / HIGH_LATENCY_THRESHOLD_MS)
            .clamp(0.5, 2.0)
            .min(1.0);
        candidates.push(EvolutionCandidate::new(
            "performance:high_latency",
            OpportunityCategory::PerformanceHotspot,
            format!("Hook latency above target ({latency:.1}ms, target <10ms warm)"),
            format!(
                "Average hook latency is {latency:.1}ms, exceeding the warm target of \
                 {HIGH_LATENCY_THRESHOLD_MS}ms. \
                 This degrades the developer experience and may cause context injection timeouts."
            ),
            impact_mod,
            vec![format!("avg_hook_latency_ms = {latency:.1}")],
            "Profile the hot path in HookRuntime. Common causes: \
             (1) SQLite queries without indexes, \
             (2) rkyv deserialization on every call (add result_cache), \
             (3) LinUCB Sherman-Morrison update without early exit.",
        ));
    }

    fn score_drift_response(
        &self,
        snapshot: &TelemetrySnapshot,
        candidates: &mut Vec<EvolutionCandidate>,
    ) {
        if !snapshot.drift_detected {
            return;
        }
        candidates.push(EvolutionCandidate::new(
            "drift:no_response_handler",
            OpportunityCategory::DriftResponse,
            "Drift detected -- no automatic response handler configured",
            "PageHinkley or DriftMonitor detected a regime change in reward/latency streams, \
             but no automatic response is configured. The system continues using stale priors.",
            1.0,
            vec!["drift_detected = true".to_string()],
            "Wire PageHinkley.update() into OnlineRLEngine: when drift returns true, \
             call qtable.reset_eligibility_traces() and linucb.reset_arm_priors(). \
             Add a DriftResponseHandler trait to touring-learning.",
        ));
    }

    fn score_test_coverage(
        &self,
        snapshot: &TelemetrySnapshot,
        candidates: &mut Vec<EvolutionCandidate>,
    ) {
        if snapshot.undertested_modules.is_empty() {
            return;
        }
        let count = snapshot.undertested_modules.len();
        let modules = snapshot.undertested_modules.join(", ");
        candidates.push(EvolutionCandidate::new(
            "quality:test_coverage",
            OpportunityCategory::TestCoverageGap,
            format!("{count} modules below minimum test coverage"),
            format!(
                "Modules with fewer than {MIN_TESTS_PER_MODULE} tests: {modules}. \
                 These are the highest-risk areas for regressions during evolution cycles."
            ),
            (count as f64 / 5.0).clamp(0.3, 1.0),
            vec![
                format!("undertested_modules = {count}"),
                format!("modules: {modules}"),
            ],
            format!(
                "Add unit tests for: {modules}. Focus on: boundary conditions, \
                 error paths, and integration with adjacent modules."
            ),
        ));
    }

    fn score_hyperparam_degradation(
        &self,
        snapshot: &TelemetrySnapshot,
        candidates: &mut Vec<EvolutionCandidate>,
    ) {
        if snapshot.optimizer_reset_count == 0 {
            return;
        }
        let impact_mod = (snapshot.optimizer_reset_count as f64 / 3.0).clamp(0.5, 1.0);
        candidates.push(EvolutionCandidate::new(
            "learning:hyperparam_degradation",
            OpportunityCategory::HyperparamDegradation,
            format!(
                "SelfOptimizer issued {} Reset(s) -- hyperparams degrading",
                snapshot.optimizer_reset_count
            ),
            format!(
                "The SelfOptimizer detected reward degradation {} time(s) and reset hyperparams \
                 to defaults. This suggests the default config is sub-optimal for the current \
                 task distribution.",
                snapshot.optimizer_reset_count
            ),
            impact_mod,
            vec![format!(
                "optimizer_reset_count = {}",
                snapshot.optimizer_reset_count
            )],
            "Analyze which hyperparams degraded (ema_alpha vs forced_explore_interval). \
             Consider persisting learned hyperparams across sessions via rkyv snapshot.",
        ));
    }

    fn score_low_integration(
        &self,
        snapshot: &TelemetrySnapshot,
        candidates: &mut Vec<EvolutionCandidate>,
    ) {
        let low: Vec<_> = snapshot
            .low_integration_modules
            .iter()
            .filter(|(_, score)| *score < LOW_INTEGRATION_THRESHOLD)
            .collect();

        if low.is_empty() {
            return;
        }

        for (module, score) in &low {
            let impact_mod = 1.0 - score;
            candidates.push(EvolutionCandidate::new(
                format!("integration:low_score:{module}"),
                OpportunityCategory::MissingIntegration,
                format!("Module `{module}` has low integration score ({score:.0}%)"),
                format!(
                    "`{module}` integration_score = {score:.2}. Its pub symbols are exported \
                     but most are not consumed by other modules. \
                     This wastes API surface and creates maintenance burden."
                ),
                impact_mod,
                vec![
                    format!("module = {module}"),
                    format!("integration_score = {score:.2}"),
                ],
                format!(
                    "Run `touring wiring score {module} -j` for details. \
                     Wire orphan symbols into consumers or reduce pub surface with `pub(crate)`."
                ),
            ));
        }
    }

    fn score_cache_inefficiency(
        &self,
        snapshot: &TelemetrySnapshot,
        candidates: &mut Vec<EvolutionCandidate>,
    ) {
        let hit_rate = match snapshot.parser_cache_hit_rate {
            Some(r) if r < MIN_CACHE_HIT_RATE => r,
            _ => return,
        };
        let impact_mod = (MIN_CACHE_HIT_RATE - hit_rate) / MIN_CACHE_HIT_RATE + 0.5;
        candidates.push(EvolutionCandidate::new(
            "performance:cache_hit_rate",
            OpportunityCategory::CacheInefficiency,
            format!(
                "Parser cache hit rate below threshold ({:.0}%, target ≥70%)",
                hit_rate * 100.0
            ),
            format!(
                "The incremental parser cache hit rate is {:.1}%, below the efficiency \
                 threshold of {:.0}%. Each cache miss triggers a full re-parse, \
                 increasing per-file latency and CPU usage.",
                hit_rate * 100.0,
                MIN_CACHE_HIT_RATE * 100.0,
            ),
            impact_mod.clamp(0.3, 1.0),
            vec![format!("parser_cache_hit_rate = {hit_rate:.2}")],
            "Options: (1) Increase cache TTL in IncrementalIndexer, \
             (2) Add pre-warming for top-15 most accessed files at session-start, \
             (3) Investigate which file types have lowest hit rates and prioritize them.",
        ));
    }

    fn score_gotcha_recurrence(
        &self,
        snapshot: &TelemetrySnapshot,
        candidates: &mut Vec<EvolutionCandidate>,
    ) {
        if snapshot.recurring_gotcha_count <= MAX_GOTCHA_RECURRENCE {
            return;
        }
        let count = snapshot.recurring_gotcha_count;
        let impact_mod = (count as f64 / (MAX_GOTCHA_RECURRENCE as f64 * 2.0)).clamp(0.5, 1.0);
        candidates.push(EvolutionCandidate::new(
            "quality:gotcha_recurrence",
            OpportunityCategory::GotchaRecurrence,
            format!("{count} gotchas recurring without resolution"),
            format!(
                "{count} gotchas have high decay_score (> 0.7), meaning they keep \
                 occurring in the same files without being resolved. These represent \
                 systemic issues that cost tokens and time on every encounter."
            ),
            impact_mod,
            vec![format!("recurring_gotcha_count = {count}")],
            "Run `touring gotcha list -j | jq '.[] | select(.decay_score > 0.7)'` \
             to find the top recurring gotchas. For each: either (a) fix the root cause \
             in the code, or (b) add automated prevention via a pre-edit hook check.",
        ));
    }

    fn score_cognitive_bottleneck(
        &self,
        snapshot: &TelemetrySnapshot,
        candidates: &mut Vec<EvolutionCandidate>,
    ) {
        if !snapshot.cognitive_engine_degraded {
            return;
        }
        candidates.push(EvolutionCandidate::new(
            "cognitive:engine_degraded",
            OpportunityCategory::CognitiveBottleneck,
            "Cognitive engine operating below healthy baseline",
            "One or more cognitive engines (AdaptiveEngine, TfIdf vectorizer, \
             SemanticGraph) are in a degraded state. This reduces the quality of \
             context injection and semantic search results.",
            1.0,
            vec!["cognitive_engine_degraded = true".to_string()],
            "Run `touring cognitive engines -j` to identify which engines are degraded. \
             Common causes: (1) AdaptiveEngine with insufficient training data — \
             trigger a warm-up cycle, (2) TfIdf index stale — rebuild via \
             `touring index status`, (3) SqliteGraphStore corrupted — inspect WAL.",
        ));
    }

    /// Filter candidates above a minimum composite score.
    pub fn filter_by_score(
        candidates: Vec<EvolutionCandidate>,
        min_score: f64,
    ) -> Vec<EvolutionCandidate> {
        candidates
            .into_iter()
            .filter(|c| c.composite_score >= min_score)
            .collect()
    }

    /// Summary statistics for a candidate list.
    pub fn summary(candidates: &[EvolutionCandidate]) -> OpportunitySummary {
        if candidates.is_empty() {
            return OpportunitySummary::default();
        }
        let total = candidates.len();
        let mean_score = candidates.iter().map(|c| c.composite_score).sum::<f64>() / total as f64;
        let top = candidates.first().cloned();
        let mut by_category: HashMap<String, usize> = HashMap::new();
        for c in candidates {
            *by_category
                .entry(c.category.label().to_string())
                .or_insert(0) += 1;
        }
        OpportunitySummary {
            total,
            mean_score,
            top_candidate: top,
            by_category,
        }
    }
}

/// Summary of opportunity analysis results.
#[derive(Debug, Default)]
pub struct OpportunitySummary {
    /// Total number of candidates analyzed.
    pub total: usize,
    /// Mean composite score across all candidates.
    pub mean_score: f64,
    /// Highest-scoring candidate, if any.
    pub top_candidate: Option<EvolutionCandidate>,
    /// Count of candidates grouped by category.
    pub by_category: HashMap<String, usize>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_snapshot() -> TelemetrySnapshot {
        TelemetrySnapshot {
            wiring_orphan_count: 3,
            wiring_orphans: vec!["FooBar".to_string(), "BazQux".to_string()],
            mean_td_error: Some(0.8),
            ema_reward: Some(0.4),
            rl_update_count: 500,
            low_integration_modules: vec![("meta".to_string(), 0.3)],
            drift_detected: true,
            optimizer_reset_count: 2,
            undertested_modules: vec!["curiosity".to_string()],
            avg_hook_latency_ms: Some(15.0),
            recurring_gotcha_count: 1,
            linucb_arms: HashMap::new(),
            parser_cache_hit_rate: None,
            cognitive_engine_degraded: false,
        }
    }

    #[test]
    fn analyze_produces_candidates() {
        let scorer = OpportunityScorer::new();
        let snapshot = make_snapshot();
        let candidates = scorer.analyze(&snapshot);
        assert!(
            !candidates.is_empty(),
            "should produce candidates from rich snapshot"
        );
    }

    #[test]
    fn candidates_sorted_by_composite_desc() {
        let scorer = OpportunityScorer::new();
        let snapshot = make_snapshot();
        let candidates = scorer.analyze(&snapshot);
        for w in candidates.windows(2) {
            assert!(
                w[0].composite_score >= w[1].composite_score,
                "candidates must be sorted DESC by composite_score"
            );
        }
    }

    #[test]
    fn empty_snapshot_produces_no_candidates() {
        let scorer = OpportunityScorer::new();
        let snapshot = TelemetrySnapshot::default();
        let candidates = scorer.analyze(&snapshot);
        assert!(candidates.is_empty(), "empty snapshot = no candidates");
    }

    #[test]
    fn filter_by_score_works() {
        let scorer = OpportunityScorer::new();
        let candidates = scorer.analyze(&make_snapshot());
        let filtered = OpportunityScorer::filter_by_score(candidates.clone(), 0.8);
        for c in &filtered {
            assert!(c.composite_score >= 0.8);
        }
        assert!(filtered.len() <= candidates.len());
    }

    #[test]
    fn summary_has_correct_total() {
        let scorer = OpportunityScorer::new();
        let candidates = scorer.analyze(&make_snapshot());
        let total = candidates.len();
        let summary = OpportunityScorer::summary(&candidates);
        assert_eq!(summary.total, total);
        assert!(summary.top_candidate.is_some());
        assert!(summary.mean_score > 0.0);
    }

    #[test]
    fn drift_response_has_max_impact() {
        let scorer = OpportunityScorer::new();
        let snapshot = TelemetrySnapshot {
            drift_detected: true,
            ..Default::default()
        };
        let candidates = scorer.analyze(&snapshot);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].category, OpportunityCategory::DriftResponse);
        assert!((candidates[0].impact_score - 0.85).abs() < 0.01);
    }

    #[test]
    fn performance_hotspot_flagged_above_threshold() {
        let scorer = OpportunityScorer::new();
        let snapshot = TelemetrySnapshot {
            avg_hook_latency_ms: Some(25.0),
            ..Default::default()
        };
        let candidates = scorer.analyze(&snapshot);
        assert!(
            candidates
                .iter()
                .any(|c| c.category == OpportunityCategory::PerformanceHotspot)
        );
    }

    #[test]
    fn no_performance_hotspot_below_threshold() {
        let scorer = OpportunityScorer::new();
        let snapshot = TelemetrySnapshot {
            avg_hook_latency_ms: Some(5.0),
            ..Default::default()
        };
        let candidates = scorer.analyze(&snapshot);
        assert!(
            !candidates
                .iter()
                .any(|c| c.category == OpportunityCategory::PerformanceHotspot)
        );
    }

    #[test]
    fn cache_inefficiency_flagged_below_threshold() {
        let scorer = OpportunityScorer::new();
        let snapshot = TelemetrySnapshot {
            parser_cache_hit_rate: Some(0.55),
            ..Default::default()
        };
        let candidates = scorer.analyze(&snapshot);
        assert!(
            candidates
                .iter()
                .any(|c| c.category == OpportunityCategory::CacheInefficiency),
            "cache hit rate 0.55 should trigger CacheInefficiency"
        );
    }

    #[test]
    fn cache_inefficiency_not_flagged_above_threshold() {
        let scorer = OpportunityScorer::new();
        let snapshot = TelemetrySnapshot {
            parser_cache_hit_rate: Some(0.85),
            ..Default::default()
        };
        let candidates = scorer.analyze(&snapshot);
        assert!(
            !candidates
                .iter()
                .any(|c| c.category == OpportunityCategory::CacheInefficiency),
            "cache hit rate 0.85 should not trigger CacheInefficiency"
        );
    }

    #[test]
    fn gotcha_recurrence_flagged_above_threshold() {
        let scorer = OpportunityScorer::new();
        let snapshot = TelemetrySnapshot {
            recurring_gotcha_count: 10,
            ..Default::default()
        };
        let candidates = scorer.analyze(&snapshot);
        assert!(
            candidates
                .iter()
                .any(|c| c.category == OpportunityCategory::GotchaRecurrence),
            "10 recurring gotchas should trigger GotchaRecurrence"
        );
    }

    #[test]
    fn gotcha_recurrence_not_flagged_at_or_below_threshold() {
        let scorer = OpportunityScorer::new();
        let snapshot = TelemetrySnapshot {
            recurring_gotcha_count: 5,
            ..Default::default()
        };
        let candidates = scorer.analyze(&snapshot);
        assert!(
            !candidates
                .iter()
                .any(|c| c.category == OpportunityCategory::GotchaRecurrence),
            "5 recurring gotchas at threshold should not trigger GotchaRecurrence"
        );
    }

    #[test]
    fn cognitive_bottleneck_flagged_when_degraded() {
        let scorer = OpportunityScorer::new();
        let snapshot = TelemetrySnapshot {
            cognitive_engine_degraded: true,
            ..Default::default()
        };
        let candidates = scorer.analyze(&snapshot);
        assert_eq!(candidates.len(), 1);
        assert_eq!(
            candidates[0].category,
            OpportunityCategory::CognitiveBottleneck
        );
        assert!((candidates[0].impact_score - 0.75).abs() < 0.01);
    }

    #[test]
    fn test_scorer_new() {
        // OpportunityScorer::new() must not panic and produce a valid instance
        let scorer = OpportunityScorer::new();
        let snapshot = TelemetrySnapshot::default();
        let candidates = scorer.analyze(&snapshot);
        // Empty snapshot → no candidates; just confirming no panic
        assert!(candidates.is_empty());
    }

    #[test]
    fn test_analyze_integration_gaps() {
        let scorer = OpportunityScorer::new();
        let snapshot = TelemetrySnapshot {
            wiring_orphan_count: 5,
            wiring_orphans: vec!["PubFoo".to_string(), "PubBar".to_string()],
            ..Default::default()
        };
        let candidates = scorer.analyze(&snapshot);
        assert!(
            candidates
                .iter()
                .any(|c| c.category == OpportunityCategory::IntegrationGap),
            "high orphan_count must trigger IntegrationGap candidate"
        );
    }

    #[test]
    fn test_analyze_test_coverage_gap() {
        let scorer = OpportunityScorer::new();
        let snapshot = TelemetrySnapshot {
            undertested_modules: vec!["crates/foo".to_string(), "crates/bar".to_string()],
            ..Default::default()
        };
        let candidates = scorer.analyze(&snapshot);
        assert!(
            candidates
                .iter()
                .any(|c| c.category == OpportunityCategory::TestCoverageGap),
            "undertested_modules > 0 must trigger TestCoverageGap candidate"
        );
    }

    #[test]
    fn test_opportunity_category_label() {
        let categories = [
            OpportunityCategory::IntegrationGap,
            OpportunityCategory::ConvergenceIssue,
            OpportunityCategory::PerformanceHotspot,
            OpportunityCategory::MissingIntegration,
            OpportunityCategory::DriftResponse,
            OpportunityCategory::TestCoverageGap,
            OpportunityCategory::HyperparamDegradation,
            OpportunityCategory::CognitiveBottleneck,
            OpportunityCategory::CacheInefficiency,
            OpportunityCategory::GotchaRecurrence,
        ];
        for cat in &categories {
            let label = cat.label();
            assert!(
                !label.is_empty(),
                "label() must return non-empty string for {:?}",
                cat
            );
        }
    }
}
