//! External data sources integration module.
//!
//! Loads data from Claude Code's external stores into touring-learning's
//! EvolutionAnalyzer and RlmMemory systems.

mod audit;
mod checkpoint;
mod errors;
mod stats;
mod telemetry;

pub use audit::{AuditError, AuditEvent, AuditLoader, AuditSummary, Result};
pub use checkpoint::{CheckpointError, CheckpointLoader};
pub use stats::{SessionStats, ToolStats};
pub use telemetry::{
    TelemetryError, TelemetryEvent, TelemetryLoader, TelemetrySessionStats, TelemetrySummary,
};
