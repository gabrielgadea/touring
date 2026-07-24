//! Analysis configuration controlling depth and feature selection.

/// Configuration controlling analysis depth and feature selection.
///
/// Three presets: `hook_path()` for inline hook analysis (<50ms),
/// `standard()` for CLI analysis (<2s), `deep()` for full codebase analysis.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AnalysisConfig {
    /// Maximum BFS depth for blast radius (0 = unlimited).
    pub blast_depth: usize,
    /// Files to sample for quality analysis (0 = all).
    pub quality_sample: usize,
    /// Whether to run cross-crate analysis.
    pub cross_crate: bool,
    /// Whether to include temporal trend analysis.
    pub temporal: bool,
    /// Whether to run knowledge DB dimension (file stats, language distribution).
    pub knowledge: bool,
    /// Whether to run learning/RL dimension (requires graph.db connection).
    pub learning: bool,
    /// Performance budget in milliseconds (hard-abort if exceeded on hook path).
    pub budget_ms: Option<u64>,
}

impl AnalysisConfig {
    /// Hook-path config: fast, bounded, no cross-crate.
    /// Target: <50ms total.
    pub fn hook_path() -> Self {
        Self {
            blast_depth: 5,
            quality_sample: 1,
            cross_crate: false,
            temporal: false,
            knowledge: false,
            learning: false,
            budget_ms: Some(40),
        }
    }

    /// CLI standard config: medium depth, 30-file sample.
    /// Target: <2s total.
    pub fn standard() -> Self {
        Self {
            blast_depth: 0,
            quality_sample: 30,
            cross_crate: false,
            temporal: false,
            knowledge: true,
            learning: false,
            budget_ms: None,
        }
    }

    /// CLI standard config with learning/RL dimension enabled.
    ///
    /// Same as `standard()` but also queries the RL tables in graph.db.
    /// Requires a graph DB connection to be set via `AnalysisPipelineBuilder::graph_conn`.
    pub fn standard_with_learning() -> Self {
        Self {
            learning: true,
            ..Self::standard()
        }
    }

    /// CLI deep config: all files, cross-crate, temporal.
    /// Target: <30s total.
    pub fn deep() -> Self {
        Self {
            blast_depth: 0,
            quality_sample: usize::MAX,
            cross_crate: true,
            temporal: true,
            knowledge: true,
            learning: true,
            budget_ms: None,
        }
    }

    /// Override the performance budget.
    ///
    /// Sets `budget_ms` on an existing config. Useful for adapting a preset
    /// to a tighter latency requirement without changing other settings.
    ///
    /// # Example
    /// ```
    /// use touring_analysis::AnalysisConfig;
    ///
    /// let config = AnalysisConfig::standard().with_budget(500);
    /// assert_eq!(config.budget_ms, Some(500));
    /// ```
    pub fn with_budget(mut self, ms: u64) -> Self {
        self.budget_ms = Some(ms);
        self
    }

    /// Enable the temporal dimension.
    ///
    /// Convenience modifier: enable temporal analysis on any preset.
    /// Requires the `temporal` feature to have any effect at runtime.
    pub fn with_temporal(mut self) -> Self {
        self.temporal = true;
        self
    }

    /// Enable the learning/RL dimension.
    ///
    /// Requires a graph DB connection via `AnalysisPipelineBuilder::graph_conn`.
    pub fn with_learning(mut self) -> Self {
        self.learning = true;
        self
    }
}

/// Depth preset for E2E analysis.
///
/// Ordered Quick < Standard < Deep, enabling `>=` comparisons in dispatch logic.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub enum Depth {
    /// Quick: index + wiring only (~50ms).
    Quick,
    /// Standard: + AST + quality + learning (~500ms).
    Standard,
    /// Deep: all files + evolution + memory (~2s+).
    Deep,
}

impl Depth {
    /// Parse from string (case-insensitive), defaulting to Standard.
    ///
    /// Accepts short forms: "q" → Quick, "d" → Deep.
    pub fn from_str_lossy(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "quick" | "q" => Self::Quick,
            "deep" | "d" => Self::Deep,
            _ => Self::Standard,
        }
    }

    /// Return the canonical string representation.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Quick => "quick",
            Self::Standard => "standard",
            Self::Deep => "deep",
        }
    }

    /// Number of files to sample for quality/AST analysis (0 = all).
    pub fn sample_size(self) -> usize {
        match self {
            Self::Quick => 5,
            Self::Standard => 30,
            Self::Deep => usize::MAX,
        }
    }

    /// Convert to AnalysisConfig.
    pub fn to_config(self) -> AnalysisConfig {
        match self {
            Self::Quick => AnalysisConfig {
                blast_depth: 3,
                quality_sample: 0,
                cross_crate: false,
                temporal: false,
                knowledge: false,
                learning: false,
                budget_ms: Some(100),
            },
            Self::Standard => AnalysisConfig::standard(),
            Self::Deep => AnalysisConfig::deep(),
        }
    }
}
