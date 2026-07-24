//! touring-foundation — Foundation crate for the Touring workspace.
//!
//! Provides shared error types, configuration, and core type definitions
//! for the Touring workspace.
//!
//! # Architecture
//!
//! This is the foundational crate of the Touring workspace. All other crates
//! depend on it. Changes here have **high blast radius** — every crate in the
//! workspace is affected.
//!
//! # Modules
//!
//! - [`alloc`] — Global memory allocator (mimalloc) - MUST be first
//! - [`error`] — Unified error type (`TouringError`) with `thiserror` derives
//! - [`config`] — Runtime configuration with tiered path resolution
//! - [`types`] — Core domain types (`CILALevel`, `MemoryTier`)
//! - [`migration`] — Schema migration engine for SQLite databases

// W13.1 (2026-05-23) → P2.7.7 (2026-06-03) — DOC COMPLETENESS WAVE
// CLOSED. 346 → 0 `missing_docs` warnings (304 docs added in W1-W6,
// final 41 in W7.5). Promoted `#![warn(missing_docs)]` →
// `#![deny(missing_docs)]` on 2026-06-03 to lock the doc-coverage
// invariant for all future edits. Re-verify with:
//   cargo build -p touring-foundation 2>&1 | grep -cE "^warning:"
#![deny(missing_docs)]
// RBP-01 elite-lint ratchet (2026-06-16): the kernel is prod-unwrap-free (the 4
// SystemTime calls now `.expect(...)`), so lock it against future bare `.unwrap()`
// in non-test code. `cfg_attr(not(test), …)` keeps test ergonomics. `.expect("…")`
// (with a message) stays the sanctioned escape. First crate of the workspace-wide
// `unwrap_used = deny` march (RBP-01 fix-first remainder).
#![cfg_attr(not(test), deny(clippy::unwrap_used))]

pub mod alloc;
pub mod char_classes;
pub mod checkpoint;
pub mod chunker;
pub mod config;
// conflict peeled to touring-resilience crate (A4 P1, 2026-06-15)
pub mod diagnostic;
pub mod drift;
pub mod error;
// failover peeled to touring-resilience crate (A4 P2, 2026-06-15)
pub mod feedback;
/// Enrichment gate-metrics counters (fast-path vs full-enrichment, CEG counters,
/// per-path health-delta, hook latency histograms). Generic observability infra
/// relocated from touring-hooks-shared (A5 Path-A step-2, 2026-06-16): its only
/// `crate::` dep is `memory_stats_probe` (also in the kernel now) — belongs in the kernel.
pub mod gate_metrics;
pub mod gate_metrics_snapshot;
pub mod governor;
pub mod hash;
pub mod health;
pub mod health_events;
/// Session-insight value types (`ErrorPatternInsight`, `EditedFileInsight`) shared by
/// the knowledge data layer and the hooks session-insights layer (relocated A5, 2026-06-15).
pub mod insights;
/// `KnowledgeSource` trait + 6 record types — the abstraction boundary between the
/// cognitive engine (touring-intelligence) and the hooks knowledge DB. Relocated
/// to the kernel so `touring-storage` can host `impl KnowledgeSource for
/// ThreadSafeKnowledgeDB` without the storage→intelligence→analysis→code→storage
/// Cargo cycle; `touring_intelligence::reasoning::bridge` re-exports it all
/// (A5 Path-A step-4, 2026-06-16).
pub mod knowledge_source;
/// Process memory-usage probe (physical/virtual MB snapshot via the `memory-stats`
/// crate). Generic infra relocated from touring-hooks-shared (A5 Path-A step-1,
/// 2026-06-16): a leaf with no `crate::` deps — its natural home is the kernel.
pub mod memory_stats_probe;
pub mod migration;
/// Generic moka cache builders + stats (relocated from touring-hooks-shared in
/// A5 step-2, 2026-06-15; generic infra belongs in the kernel).
pub mod moka_policies;
pub mod plugin;
pub mod profile;
/// Process-wide moka-backed query result cache (string-keyed memoization +
/// single-flight `get_with` + path-scoped invalidation). Generic cache infra
/// relocated from touring-hooks-shared (A5 Path-A step-3, 2026-06-16): its only
/// `crate::` dep is `gate_metrics` (now in the kernel); `SymbolEntry` is local.
pub mod query_cache;
pub mod schema;
/// DDL table-validation helpers (relocated from touring-analysis::e2e in A5, 2026-06-15;
/// validates the [`schema`] tables — its natural home is the kernel that owns the DDL).
pub mod schema_guard;
pub mod security;
pub mod shared;
/// Core domain types — [`crate::CILALevel`], [`crate::MemoryTier`],
/// `crate::TodoKind`, `crate::EdgeConfidence`, and the
/// `truncate_str` UTF-8 safe string helper.
pub mod types;

// D39: Multi-Resolution Knowledge Layer (MVKL)
pub mod mvkl;

// D41: Code Graph Model (NeurIPS 2025)
pub mod cgm;

// embedding (GPU embedding client) peeled to touring-storage::embedding (A4 P4, 2026-06-15).

/// Graceful shutdown handler for long-running services.
///
/// ## Example
///
/// ```ignore
/// let shutdown = shutdown::Shutdown::new();
/// tokio::spawn(async move {
///     tokio::select! {
///         result = server.run() => result,
///         _ = shutdown.recv() => Ok(()),
///     }
/// });
/// ```
pub mod shutdown;

/// YAML-driven autofix rule engine — absorbed from `touring-rule-engine` in
/// W3.4 (Wave 2026-05-12). Provides `Rule`, `RuleSet`, `Fix`, `RuleEngine`,
/// `Severity` and `Error`/`Result` types under `touring_foundation::rules`.
pub mod rules;

/// Semantic classification of code symbols — absorbed from `touring-definitions`
/// in W3.5 (Wave 2026-05-12). Provides `SemanticClass` (22 categories),
/// `SemanticClassifier`, `RuleEngine` (semantic, distinct from `rules::RuleEngine`
/// autofix engine), plus `validate_embedded_data` entry-point. Exposed as
/// `touring_foundation::semantic::*`. Data corpora (`universal_rules.json`,
/// `categories.json`, `scoring.json`) ship embedded under
/// `src/semantic/data/`.
pub mod semantic;

/// eBPF-backed syscall + memory + hook-latency telemetry — absorbed from
/// `touring-telemetry` in W3.6 (Wave 2026-05-12). Provides `TelemetryCollector`,
/// `TelemetryConfig`, `TelemetryError`, `LatencyHistogram`, `EbpfMonitor`,
/// `SyscallId`, `MemorySample`, `HookPhase`, plus `TelemetryPoint` /
/// `TelemetryExport` wire types. The eBPF code path is gated behind the
/// `ebpf-telemetry` feature on this crate; without it, `TelemetryCollector`
/// degrades to a polling collector when `allow_fallback = true`.
///
/// The submodule keeps the original `#[cfg(feature = "ebpf")]` gates internally;
/// the foundation-level `ebpf-telemetry` feature both pulls in `aya` / `aya-log`
/// and re-exports them inside the module via `#[cfg(feature = "ebpf-telemetry")]`
/// (see `telemetry/mod.rs`).
pub mod telemetry;

// sentinel (PSI memory-pressure guard + CPU P/E core scheduler + the
// touring-resource-monitor binary) peeled to the touring-resilience crate (A4 P3, 2026-06-15).

/// Append-only activity event store with SHA-256 projection gate — absorbed
/// from `touring-activity` in W3.8 (Wave 2026-05-12). Event-sourced agent
/// state per the ESAA pattern. Provides [`activity::event::{Event, EventAction,
/// Actor}`], [`activity::store::EventStore`] (JSONL append + projection),
/// [`activity::projection`] (projection module + content hash), and
/// [`activity::verify::Verifier`] (replay + integrity check).
///
/// Always-on lib (no feature flag). Used by `touring-hooks::activity_hook`
/// (D1.6 hook integration) and `touring-server::cli::activity`.
pub mod activity;

pub use char_classes::{CharClass, CharClasses};
pub use config::TouringConfig;
pub use error::TouringError;
// failover types peeled to touring-resilience crate (A4 P2, 2026-06-15)
pub use feedback::{
    FeedbackPattern, FeedbackRelation, FeedbackResult, FeedbackSignal, PatternFeedback,
    PatternFeedbackContext,
};
pub use health_events::{
    DeltaOutcome, HealthDeltaEvent, publish as publish_health_event,
    subscribe as subscribe_health_events, subscriber_count as health_event_subscriber_count,
};
pub use shared::circuit_breaker::CircuitBreaker;
pub use shared::domain_circuit::{
    CircuitBreakerImpl, CircuitState, Domain, DomainCircuitBreaker, GuardOutcome,
    SharedDomainCircuitBreaker,
};
pub use shared::pool::ConnectionPool;
pub use types::{CILALevel, MemoryTier, truncate_str};

/// Result type alias for Touring operations.
pub type Result<T> = std::result::Result<T, TouringError>;
