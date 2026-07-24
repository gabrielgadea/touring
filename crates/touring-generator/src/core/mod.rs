//! Core types: `NormalizedScore`, `CapacityLimits`, `GeneratorContext`, and provider traits.

pub mod adapters;
pub mod capacity;
pub mod context;
pub mod context_exec;
pub mod context_fuzzy;
pub mod context_gates;
pub mod context_quality;
pub mod context_telemetry;
pub mod context_wiring;
pub mod score;

pub use capacity::{CapacityLimits, PlanPriority};
pub use context::{
    AuditLog, GeneratorContext, LlmError, LlmProvider, MemoryEntry, MemoryError, MemoryKind,
    MemoryProvider, MemoryStats, MemoryTier, RlRewardSink, TelemetrySink,
};
pub use score::NormalizedScore;
