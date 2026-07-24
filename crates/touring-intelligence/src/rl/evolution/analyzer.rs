//! EvolutionAnalyzer — pattern analysis over accumulated data.
//!
//! Analyzes compliance, cost, skill, and hook data to detect trends.
//! Adapted from touring/src/evolution/analyzer.rs (334 LOC)
//!
//! NOTE: The original analyzer used MemoryStore (which depends on EmbeddingClient
//! from touring-core). This version operates directly on RlmMemory to avoid the
//! async/GPU dependency. MemoryStore integration belongs in touring-server.

use crate::rl::data::AuditLoader;
use crate::rl::memory::rlm::RlmMemory;
use crate::rl::ranking::wilson::{DriftDetector, WilsonRanker};
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

/// Analysis result with statistical evidence.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AnalysisResult {
    /// Category of the analyzed data (compliance, cost, skill, hook, etc.).
    pub category: String,
    /// Name of the metric being reported.
    pub metric: String,
    /// Measured value for the metric.
    pub value: f64,
    /// Detected trend direction for the metric.
    pub trend: Trend,
    /// Supporting evidence strings backing the result.
    pub evidence: Vec<String>,
}

impl std::fmt::Display for AnalysisResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "[{}] {} = {:.3} ({})",
            self.category, self.metric, self.value, self.trend
        )
    }
}

/// Trend direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Trend {
    /// Metric is moving in a favorable direction.
    Improving,
    /// Metric is holding steady.
    Stable,
    /// Metric is moving in an unfavorable direction.
    Degrading,
    /// Not enough data to determine a trend.
    Insufficient,
}

impl std::fmt::Display for Trend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Improving => write!(f, "improving"),
            Self::Stable => write!(f, "stable"),
            Self::Degrading => write!(f, "degrading"),
            Self::Insufficient => write!(f, "insufficient_data"),
        }
    }
}

impl Trend {
    /// Returns `true` if the trend indicates improvement or stability.
    #[must_use]
    pub fn is_healthy(&self) -> bool {
        matches!(self, Self::Improving | Self::Stable)
    }

    /// Returns `true` if the trend indicates a problem needing attention.
    #[must_use]
    pub fn needs_attention(&self) -> bool {
        matches!(self, Self::Degrading)
    }
}

/// Inner state for the evolution analyzer.
struct EvolutionState {
    rlm: RlmMemory,
    ranker: WilsonRanker,
    drift: DriftDetector,
}

impl std::fmt::Debug for EvolutionState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EvolutionState")
            .field("ranker", &self.ranker)
            .field("drift", &self.drift)
            .finish_non_exhaustive()
    }
}

/// Analyzes patterns from accumulated memory data.
#[derive(Debug, Clone)]
pub struct EvolutionAnalyzer {
    state: Arc<Mutex<EvolutionState>>,
    /// Optional audit loader — populated via load_audit_data()
    audit_loader: Option<AuditLoader>,
    /// MetacognitivePipeline decisions for self-optimization feedback.
    /// Wired to touring-learning metacognitive_pipeline module.
    metacognitive_decisions: Vec<crate::rl::metacognitive_pipeline::MetacognitiveDecision>,
    /// Bug bounty tracker — wired to touring-offensive BugBountyTracker.
    bug_tracker: Option<touring_offensive::BugBountyTracker>,
}

impl EvolutionAnalyzer {
    /// Construct an `EvolutionAnalyzer` over the given memory, ranker, and drift detector.
    pub fn new(rlm: RlmMemory, ranker: WilsonRanker, drift: DriftDetector) -> Self {
        Self {
            state: Arc::new(Mutex::new(EvolutionState { rlm, ranker, drift })),
            audit_loader: None,
            metacognitive_decisions: Vec::new(),
            bug_tracker: None,
        }
    }

    /// Record a MetacognitivePipeline decision for evolution analysis.
    ///
    /// This wires the touring-learning MetacognitivePipeline (CUSUM+ACO+ActorCritic)
    /// into EvolutionAnalyzer, enabling self-optimization based on hook adaptation patterns.
    pub fn record_metacognitive(
        &mut self,
        decision: crate::rl::metacognitive_pipeline::MetacognitiveDecision,
    ) {
        self.metacognitive_decisions.push(decision);
    }

    /// Set the bug bounty tracker — wires touring-offensive BugBountyTracker to evolution.
    pub fn set_bug_tracker(&mut self, tracker: touring_offensive::BugBountyTracker) {
        self.bug_tracker = Some(tracker);
    }

    /// Creates a new EvolutionAnalyzer with a BugBountyTracker already wired.
    ///
    /// This is a convenience constructor that combines [`Self::new`] and [`Self::set_bug_tracker`],
    /// ensuring the bug tracker is available immediately after construction.
    ///
    /// # Example
    ///
    /// ```rust
    /// use touring_offensive::BugBountyTracker;
    /// use touring_intelligence::rl::{EvolutionAnalyzer, RlmMemory, WilsonRanker, DriftDetector};
    ///
    /// let rlm = RlmMemory::new(std::path::Path::new("test.db")).unwrap();
    /// let ranker = WilsonRanker::new();
    /// let drift = DriftDetector::new();
    /// let tracker = BugBountyTracker::new("CVE-2024-TEST", 8.0);
    /// let tracker2 = BugBountyTracker::new("CVE-2024-TEST2", 5.0);
    /// let mut analyzer = EvolutionAnalyzer::new_with_bug_tracker(rlm, ranker, drift, tracker);
    /// analyzer.set_bug_tracker(tracker2); // verify setter is callable
    /// ```
    pub fn new_with_bug_tracker(
        rlm: RlmMemory,
        ranker: WilsonRanker,
        drift: DriftDetector,
        tracker: touring_offensive::BugBountyTracker,
    ) -> Self {
        let mut analyzer = Self::new(rlm, ranker, drift);
        analyzer.set_bug_tracker(tracker);
        analyzer
    }

    /// Load audit data from ~/.claude/compliance/audit.jsonl.
    pub fn load_audit_data(
        &mut self,
        claude_home: impl AsRef<Path>,
    ) -> crate::rl::data::Result<usize> {
        let mut loader = AuditLoader::new(claude_home);
        let count = loader.load()?;
        self.audit_loader = Some(loader);
        Ok(count)
    }

    /// Run all analyses and return results.
    pub fn analyze_all(&self) -> Vec<AnalysisResult> {
        let mut results = Vec::new();
        results.extend(self.analyze_tool_effectiveness());
        results.extend(self.analyze_cila_progression());
        results.extend(self.analyze_drift_signals());
        if let Some(ref loader) = self.audit_loader {
            results.extend(Self::analyze_audit_patterns_static(loader));
        }
        results
    }

    /// Axis E: Audit event patterns (tool denials, session health).
    fn analyze_audit_patterns_static(loader: &AuditLoader) -> Vec<AnalysisResult> {
        let summary = loader.summary();
        let mut results = Vec::new();

        // Tool denial rate
        for tool in loader.top_tools(10) {
            let rate = tool.deny_rate();
            let trend = if rate < 0.01 {
                Trend::Improving
            } else if rate < 0.05 {
                Trend::Stable
            } else {
                Trend::Degrading
            };
            results.push(AnalysisResult {
                category: "audit_tool_denial".to_string(),
                metric: format!("{}/deny_rate", tool.tool_name),
                value: rate,
                trend,
                evidence: vec![format!(
                    "total={} denies={}",
                    tool.total_uses, tool.deny_count
                )],
            });
        }

        // Overall denial rate
        if summary.total_events > 0 {
            results.push(AnalysisResult {
                category: "audit_overall".to_string(),
                metric: "deny_rate".to_string(),
                value: summary.deny_rate,
                trend: if summary.deny_rate < 0.02 {
                    Trend::Improving
                } else {
                    Trend::Stable
                },
                evidence: vec![format!(
                    "{}/{} events denied",
                    summary.deny_count, summary.total_events
                )],
            });
        }

        results
    }

    /// Axis A: Tool effectiveness via Wilson ranking.
    pub(crate) fn analyze_tool_effectiveness(&self) -> Vec<AnalysisResult> {
        let state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let ranked_items = state.ranker.rank();
        let mut results = Vec::new();

        for (rank, item) in ranked_items.iter().enumerate() {
            let score = item.score.lower;
            let trend = if score > 0.8 {
                Trend::Improving
            } else if score > 0.5 {
                Trend::Stable
            } else {
                Trend::Degrading
            };

            results.push(AnalysisResult {
                category: "tool_effectiveness".to_string(),
                metric: item.id.clone(),
                value: score,
                trend,
                evidence: vec![format!(
                    "Wilson rank #{}, score={:.4}, trials={}",
                    rank + 1,
                    score,
                    item.trials,
                )],
            });
        }

        results
    }

    /// Axis A: CILA level progression analysis.
    pub(crate) fn analyze_cila_progression(&self) -> Vec<AnalysisResult> {
        let state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let result = match state.rlm.search("cila=L", None, 1000) {
            Ok(r) => r,
            Err(_) => return Vec::new(),
        };

        let mut level_counts: HashMap<String, usize> = HashMap::new();
        for m in &result {
            if let Some(cila_start) = m.value.find("cila=") {
                let rest = &m.value[cila_start + 5..];
                let level = rest.split_whitespace().next().unwrap_or("unknown");
                *level_counts.entry(level.to_string()).or_insert(0) += 1;
            }
        }

        let total: usize = level_counts.values().sum();
        if total == 0 {
            return vec![AnalysisResult {
                category: "cila_progression".to_string(),
                metric: "total_entries".to_string(),
                value: 0.0,
                trend: Trend::Insufficient,
                evidence: vec!["No compliance entries found".to_string()],
            }];
        }

        let weights: HashMap<&str, f64> = [
            ("L0", 0.0),
            ("L1", 0.16),
            ("L2", 0.33),
            ("L3", 0.50),
            ("L4", 0.66),
            ("L5", 0.83),
            ("L6", 1.0),
        ]
        .into_iter()
        .collect();

        let mut weighted_sum = 0.0;
        let mut evidence = Vec::new();
        for (level, count) in &level_counts {
            let w = weights.get(level.as_str()).copied().unwrap_or(0.0);
            weighted_sum += w * (*count as f64);
            evidence.push(format!(
                "{}: {} ({:.1}%)",
                level,
                count,
                (*count as f64 / total as f64) * 100.0,
            ));
        }
        let avg_level = weighted_sum / total as f64;

        let trend = if avg_level > 0.5 {
            Trend::Improving
        } else if avg_level > 0.25 {
            Trend::Stable
        } else {
            Trend::Degrading
        };

        vec![AnalysisResult {
            category: "cila_progression".to_string(),
            metric: "weighted_level".to_string(),
            value: avg_level,
            trend,
            evidence,
        }]
    }

    /// Axis B: Drift detection signals.
    pub(crate) fn analyze_drift_signals(&self) -> Vec<AnalysisResult> {
        let state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let signals = state.drift.detect_all();
        let mut results = Vec::new();

        for (metric, dr) in &signals {
            let trend = if !dr.drift_detected {
                if dr.confidence == 0.0 {
                    Trend::Insufficient
                } else {
                    Trend::Stable
                }
            } else if dr.direction == "down" {
                Trend::Degrading
            } else {
                Trend::Improving
            };

            results.push(AnalysisResult {
                category: "drift_detection".to_string(),
                metric: (*metric).clone(),
                value: dr.magnitude,
                trend,
                evidence: vec![format!(
                    "drift_detected={}, magnitude={:.4}, direction={}",
                    dr.drift_detected, dr.magnitude, dr.direction,
                )],
            });
        }

        results
    }

    /// Return Wilson-ranked tool items (sorted by score descending).
    ///
    /// Public accessor for the ranker's ranked output — avoids direct
    /// access to the private `state` field from external crates.
    pub fn rank_tools(&self) -> Vec<crate::rl::ranking::wilson::RankedItem> {
        let state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        state.ranker.rank()
    }

    /// Record a tool invocation outcome into the Wilson ranker.
    pub fn record_tool_outcome(&self, tool_name: &str, success: bool) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        state.ranker.record(tool_name, success);
    }

    /// Record a metric observation into the drift detector.
    pub fn record_metric(&self, metric: &str, value: f64) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        state.drift.record(metric, value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_analyzer() -> EvolutionAnalyzer {
        let dir = TempDir::new().unwrap();
        let rlm = RlmMemory::new(&dir.path().join("rlm.db")).unwrap();
        std::mem::forget(dir);
        let ranker = WilsonRanker::new();
        let drift = DriftDetector::new();
        EvolutionAnalyzer::new(rlm, ranker, drift)
    }

    #[test]
    fn test_analyzer_creation() {
        let analyzer = make_analyzer();

        let results = analyzer.analyze_all();
        assert!(results.iter().any(|r| r.category == "cila_progression"));
    }

    #[test]
    fn test_tool_effectiveness_with_data() {
        let analyzer = make_analyzer();

        for _ in 0..10 {
            analyzer.record_tool_outcome("Write", true);
        }
        analyzer.record_tool_outcome("Write", false);

        let results = analyzer.analyze_tool_effectiveness();
        assert!(!results.is_empty());
    }

    #[test]
    fn test_new_with_bug_tracker_wires_offensive() {
        use touring_offensive::BugBountyTracker;

        let dir = TempDir::new().unwrap();
        let rlm = RlmMemory::new(&dir.path().join("rlm.db")).unwrap();
        std::mem::forget(dir);
        let ranker = WilsonRanker::new();
        let drift = DriftDetector::new();
        let tracker = BugBountyTracker::new("CVE-2024-WIRING-TEST", 7.5);

        let analyzer = EvolutionAnalyzer::new_with_bug_tracker(rlm, ranker, drift, tracker);

        assert!(analyzer.bug_tracker.is_some());
        let tracker = analyzer.bug_tracker.unwrap();
        assert_eq!(tracker.id, "CVE-2024-WIRING-TEST");
        assert_eq!(tracker.cvss, 7.5);
    }

    #[test]
    fn test_set_bug_tracker_after_construction() {
        use touring_offensive::BugBountyTracker;

        let mut analyzer = make_analyzer();
        assert!(analyzer.bug_tracker.is_none());

        let tracker = BugBountyTracker::new("CVE-2024-SETTER-TEST", 5.0);
        analyzer.set_bug_tracker(tracker);

        assert!(analyzer.bug_tracker.is_some());
        assert_eq!(analyzer.bug_tracker.unwrap().id, "CVE-2024-SETTER-TEST");
    }
}
