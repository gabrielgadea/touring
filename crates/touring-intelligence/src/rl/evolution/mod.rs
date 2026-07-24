//! Self-evolution engine — two-axis intelligence.
//!
//! Axis A: Claude Code self-improvement (tool effectiveness, CILA progression)
//! Axis B: Project evolution (pipeline quality, architecture drift)
//!
//! Unified from touring/src/evolution/ (1178 LOC)

pub mod analyzer;
pub mod insights;
pub mod persistence;

pub use analyzer::EvolutionAnalyzer;
pub use insights::{Axis, Insight, InsightEngine, Severity};
pub use persistence::LearningPersistence;
