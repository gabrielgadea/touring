//! Circuit breaker state machine — types and validation logic.
//!
//! Extracted from circuit_breaker.rs as part of Track A2 decomposition.
//! Contains the hierarchical circuit state types and state transition logic.
//!
//! # Architecture
//!
//! ```text
//! Request
//!   ├── Global Fast-Fail? ──→ [circuit tripped globally] ──→ FAIL FAST
//!   ├── Project Breaker ──→ [project degraded] ──→ SKIP PROJECT
//!   ├── Session Breaker ──→ [session degraded] ──→ SKIP SESSION
//!   └── Operation Class ──→ [class threshold] ──→ WEIGHTED BACKOFF
//!                          ──→ ALL OK ──→ PROCEED
//! ```

use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

// ── Operation Class ────────────────────────────────────────────────────────────

/// Operation class — determines threshold and cooldown based on cost.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OpClass {
    /// Light read-only: index find, ast overview, wiring status
    Light,
    /// Medium: memory recall, suggest, decompose
    Medium,
    /// Heavy: index rebuild, mcts search, blast radius on large files
    Heavy,
    /// Critical: daemon health check, session start
    Critical,
}

impl OpClass {
    /// Returns operation class based on hook name patterns.
    pub fn from_hook_name(name: &str) -> Self {
        // Critical — almost never blocked
        if name.contains("session-start") || name.contains("daemon-health") {
            return OpClass::Critical;
        }
        // Heavy — higher threshold, longer cooldown
        if name.contains("index-rebuild")
            || name.contains("index rebuild")
            || name.contains("mcts-search")
            || name.contains("mcts search")
            || name.contains("decompose-create")
            || name.contains("decompose create")
            || name.contains("evolution")
            || name.contains("blast")
        {
            return OpClass::Heavy;
        }
        // Light — strict threshold
        if name.contains("index-find")
            || name.contains("index find")
            || name.contains("ast-find")
            || name.contains("ast find")
            || name.contains("ast-overview")
            || name.contains("ast overview")
            || name.contains("wiring")
            || name.contains("memory-recall")
            || name.contains("memory recall")
            || name.contains("suggest")
            || name.contains("classify")
            || name.contains("scan-pii")
            || name.contains("cognitive")
        {
            return OpClass::Light;
        }
        // Default: Medium
        OpClass::Medium
    }

    /// How many failures before tripping this class.
    pub fn threshold(self) -> u32 {
        match self {
            OpClass::Critical => 10,
            OpClass::Heavy => 8,
            OpClass::Medium => 6,
            OpClass::Light => 10,
        }
    }

    /// Cooldown in seconds after tripping.
    pub fn cooldown_secs(self) -> u64 {
        match self {
            OpClass::Critical => 30,
            OpClass::Heavy => 120,
            OpClass::Medium => 60,
            OpClass::Light => 45,
        }
    }

    /// Window in seconds for failure accumulation.
    pub fn window_secs(self) -> u64 {
        match self {
            OpClass::Critical => 120,
            OpClass::Heavy => 90,
            OpClass::Medium => 60,
            OpClass::Light => 45,
        }
    }

    /// Cost weight for global aggregation.
    pub fn cost_weight(self) -> f64 {
        match self {
            OpClass::Critical => 0.1,
            OpClass::Heavy => 2.0,
            OpClass::Medium => 1.0,
            OpClass::Light => 0.5,
        }
    }
}

// ── State Types ────────────────────────────────────────────────────────────────

/// Global circuit state — fast-fail for catastrophic issues.
#[derive(Debug, Serialize, Deserialize, Default)]
pub struct GlobalState {
    /// Number of catastrophic failures recorded globally.
    pub catastrophic_count: u32,
    /// Unix timestamp until which the global breaker stays open (0 = closed).
    pub open_until_ts: u64,
}

/// Per-operation-class breaker state.
#[derive(Debug, Serialize, Deserialize, Default)]
pub struct ClassBreaker {
    /// Failures counted within the current sliding window.
    pub failure_count: u32,
    /// Unix timestamp of the most recent failure.
    pub last_failure_ts: u64,
    /// Unix timestamp until which this class breaker stays open (0 = closed).
    pub open_until_ts: u64,
}

/// Per-project breaker state.
#[derive(Debug, Serialize, Deserialize, Default)]
pub struct ProjectBreaker {
    /// Failures counted within the current sliding window.
    pub failure_count: u32,
    /// Unix timestamp of the most recent failure.
    pub last_failure_ts: u64,
    /// Unix timestamp until which this project breaker stays open (0 = closed).
    pub open_until_ts: u64,
    /// Cost-weighted failure score used to trip heavier projects sooner.
    pub weighted_score: f64,
}

/// Per-session breaker state.
#[derive(Debug, Serialize, Deserialize, Default)]
pub struct SessionBreaker {
    /// Failures counted within the current sliding window.
    pub failure_count: u32,
    /// Unix timestamp of the most recent failure.
    pub last_failure_ts: u64,
    /// Unix timestamp until which this session breaker stays open (0 = closed).
    pub open_until_ts: u64,
}

/// Per-operation-class breakers keyed by class.
pub type ClassBreakers = HashMap<OpClass, ClassBreaker>;

/// Per-project breakers keyed by project path.
pub type ProjectBreakers = HashMap<String, ProjectBreaker>;

/// Per-session breakers keyed by session id.
pub type SessionBreakers = HashMap<String, SessionBreaker>;

/// Full hierarchical circuit state.
#[derive(Debug, Serialize, Deserialize, Default)]
pub struct CircuitState {
    /// Global fast-fail breaker state.
    pub global: GlobalState,
    /// Breaker state per operation class.
    pub by_class: ClassBreakers,
    /// Breaker state per project path.
    pub by_project: ProjectBreakers,
    /// Breaker state per session id.
    pub by_session: SessionBreakers,
}

impl CircuitState {
    /// Create a new empty circuit state.
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if the global circuit is open.
    pub fn is_global_open(&self, now_secs: u64) -> bool {
        const GLOBAL_CATASTROPHIC_THRESHOLD: u32 = 3;
        self.global.catastrophic_count >= GLOBAL_CATASTROPHIC_THRESHOLD
            && now_secs < self.global.open_until_ts
    }

    /// Check if any class circuit is open.
    pub fn is_class_open(&self, now_secs: u64) -> bool {
        self.by_class.values().any(|b| b.open_until_ts > now_secs)
    }

    /// Check if any project circuit is open.
    pub fn is_project_open(&self, now_secs: u64) -> bool {
        self.by_project.values().any(|b| b.open_until_ts > now_secs)
    }

    /// Check if any session circuit is open.
    pub fn is_session_open(&self, now_secs: u64) -> bool {
        self.by_session.values().any(|b| b.open_until_ts > now_secs)
    }

    /// Check if any circuit is open (any dimension).
    pub fn is_any_open(&self, now_secs: u64) -> bool {
        self.is_global_open(now_secs)
            || self.is_class_open(now_secs)
            || self.is_project_open(now_secs)
            || self.is_session_open(now_secs)
    }

    /// Get weighted failure score across all projects.
    pub fn total_weighted_score(&self) -> f64 {
        self.by_project.values().map(|b| b.weighted_score).sum()
    }
}

/// Result of a circuit check — describes why/how the circuit affects this call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitCheck {
    /// Whether to skip daemon call.
    pub skip: bool,
    /// Human-readable reason.
    pub reason: String,
    /// Which circuit is affected.
    pub circuit: &'static str,
    /// Operation class of this call.
    pub op_class: OpClass,
    /// Time to wait before retry (0 if not open).
    pub retry_after_secs: u64,
}

impl CircuitCheck {
    /// Create a skip check.
    pub fn skip(
        reason: &'static str,
        circuit: &'static str,
        op_class: OpClass,
        retry_after: u64,
    ) -> Self {
        Self {
            skip: true,
            reason: reason.into(),
            circuit,
            op_class,
            retry_after_secs: retry_after,
        }
    }

    /// Create a proceed check.
    pub fn proceed(op_class: OpClass) -> Self {
        Self {
            skip: false,
            reason: "all circuits closed".into(),
            circuit: "none",
            op_class,
            retry_after_secs: 0,
        }
    }

    /// Returns true if circuit should be skipped.
    pub fn should_skip(&self) -> bool {
        self.skip
    }
}

// ── Time Utilities ─────────────────────────────────────────────────────────────

/// Get current time in seconds since UNIX_EPOCH.
pub fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Circuit breaker constants.
pub mod constants {
    // Global
    /// Catastrophic-failure count that trips the global breaker.
    pub const GLOBAL_CATASTROPHIC_THRESHOLD: u32 = 3;
    /// Seconds the global breaker stays open once tripped.
    pub const GLOBAL_COOLDOWN_SECS: u64 = 30;
    /// Sliding window (seconds) for counting global failures.
    pub const GLOBAL_WINDOW_SECS: u64 = 60;

    // Project
    /// Failure count that trips a per-project breaker.
    pub const PROJECT_THRESHOLD: u32 = 4;
    /// Seconds a project breaker stays open once tripped.
    pub const PROJECT_COOLDOWN_SECS: u64 = 90;
    /// Sliding window (seconds) for counting per-project failures.
    pub const PROJECT_WINDOW_SECS: u64 = 60;
    /// Weighted-score threshold that trips a project breaker independently of count.
    pub const PROJECT_WEIGHT_THRESHOLD: f64 = 5.0;

    // Session
    /// Failure count that trips a per-session breaker.
    pub const SESSION_THRESHOLD: u32 = 6;
    /// Seconds a session breaker stays open once tripped.
    pub const SESSION_COOLDOWN_SECS: u64 = 60;
    /// Sliding window (seconds) for counting per-session failures.
    pub const SESSION_WINDOW_SECS: u64 = 120;

    // eBPF memory pressure thresholds (Suggestion 4 — 2026-04-20)
    /// Page fault count above which eBPF signals an anomalous spike.
    pub const EBPF_PAGE_FAULT_THRESHOLD: u64 = 1_000;
    /// Cache-miss count above which L3 pressure is considered critical.
    pub const EBPF_CACHE_MISS_THRESHOLD: u64 = 10_000;
    /// Cooldown applied when eBPF trips the global circuit.
    pub const EBPF_COOLDOWN_SECS: u64 = 30;
}

/// Kernel-level memory pressure signal derived from eBPF telemetry.
///
/// Mirrors `touring_foundation::telemetry::MemorySample` without creating a direct crate
/// dependency from `touring-hooks` to `touring-foundation` (telemetry submodule, W3.6 2026-05-12). Callers that hold a
/// `MemorySample` can construct this from its `page_faults` and `cache_misses`
/// fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemoryPressureSignal {
    /// Minor + major page faults observed in the sampling window.
    pub page_faults: u64,
    /// L3 cache-miss events observed in the sampling window.
    pub cache_misses: u64,
}

impl CircuitState {
    /// Record an eBPF-derived memory pressure signal and trip the global
    /// circuit when kernel metrics exceed anomaly thresholds.
    ///
    /// Returns `true` if the circuit was tripped by this signal.
    ///
    /// # Why
    ///
    /// If code modified by Claude Code starts generating anomalous page-fault
    /// or L3 cache-miss spikes at the kernel level, the circuit opens before a
    /// user-space crash or quality regression occurs, giving the daemon time to
    /// archive the failing topology in its Pensieve memory and alert the LLM.
    pub fn record_memory_pressure(&mut self, signal: MemoryPressureSignal, now_secs: u64) -> bool {
        use constants::{EBPF_CACHE_MISS_THRESHOLD, EBPF_COOLDOWN_SECS, EBPF_PAGE_FAULT_THRESHOLD};

        if signal.page_faults > EBPF_PAGE_FAULT_THRESHOLD
            || signal.cache_misses > EBPF_CACHE_MISS_THRESHOLD
        {
            self.global.catastrophic_count = self.global.catastrophic_count.saturating_add(1);
            self.global.open_until_ts = now_secs + EBPF_COOLDOWN_SECS;
            tracing::warn!(
                page_faults = signal.page_faults,
                cache_misses = signal.cache_misses,
                open_until_ts = self.global.open_until_ts,
                catastrophic_count = self.global.catastrophic_count,
                "eBPF memory pressure anomaly — global circuit tripped",
            );
            return true;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_op_class_from_hook_name_critical() {
        assert_eq!(OpClass::from_hook_name("session-start"), OpClass::Critical);
        assert_eq!(OpClass::from_hook_name("daemon-health"), OpClass::Critical);
    }

    #[test]
    fn test_op_class_from_hook_name_heavy() {
        assert_eq!(OpClass::from_hook_name("index-rebuild"), OpClass::Heavy);
        assert_eq!(OpClass::from_hook_name("mcts-search"), OpClass::Heavy);
    }

    #[test]
    fn test_op_class_from_hook_name_light() {
        assert_eq!(OpClass::from_hook_name("index-find"), OpClass::Light);
        assert_eq!(OpClass::from_hook_name("wiring status"), OpClass::Light);
    }

    #[test]
    fn test_op_class_from_hook_name_medium() {
        assert_eq!(OpClass::from_hook_name("unknown-hook"), OpClass::Medium);
    }

    #[test]
    fn test_circuit_state_new() {
        let state = CircuitState::new();
        assert!(!state.is_any_open(0));
    }

    #[test]
    fn test_circuit_check_proceed() {
        let check = CircuitCheck::proceed(OpClass::Medium);
        assert!(!check.should_skip());
        assert_eq!(check.reason, "all circuits closed");
    }

    #[test]
    fn test_circuit_check_skip() {
        let check = CircuitCheck::skip("global", "global", OpClass::Heavy, 30);
        assert!(check.should_skip());
        assert_eq!(check.reason, "global");
        assert_eq!(check.retry_after_secs, 30);
    }

    // ── eBPF memory pressure tests (Suggestion 4 — 2026-04-20) ──────────────────

    #[test]
    fn test_memory_pressure_below_thresholds_does_not_trip_circuit() {
        let mut state = CircuitState::new();
        let signal = MemoryPressureSignal {
            page_faults: 10,
            cache_misses: 100,
        };
        let tripped = state.record_memory_pressure(signal, 1_000);
        assert!(!tripped, "normal memory signal must not trip the circuit");
        assert!(
            !state.is_global_open(1_000),
            "global circuit must stay closed"
        );
    }

    #[test]
    fn test_memory_pressure_page_faults_above_threshold_trips_circuit() {
        let mut state = CircuitState::new();
        let signal = MemoryPressureSignal {
            page_faults: constants::EBPF_PAGE_FAULT_THRESHOLD + 1,
            cache_misses: 0,
        };
        let tripped = state.record_memory_pressure(signal, 1_000);
        assert!(tripped, "page fault spike must trip the circuit");
        assert!(state.global.catastrophic_count >= 1);
        assert!(state.global.open_until_ts > 1_000);
    }

    #[test]
    fn test_memory_pressure_cache_misses_above_threshold_trips_circuit() {
        let mut state = CircuitState::new();
        let signal = MemoryPressureSignal {
            page_faults: 0,
            cache_misses: constants::EBPF_CACHE_MISS_THRESHOLD + 1,
        };
        let tripped = state.record_memory_pressure(signal, 2_000);
        assert!(tripped, "L3 cache-miss spike must trip the circuit");
        // catastrophic_count incremented and open_until_ts set — circuit state updated
        assert!(
            state.global.catastrophic_count >= 1,
            "catastrophic_count must be incremented"
        );
        assert!(
            state.global.open_until_ts > 2_000,
            "open_until_ts must be set beyond now_secs"
        );
    }

    #[test]
    fn test_memory_pressure_accumulates_catastrophic_count() {
        let mut state = CircuitState::new();
        let signal = MemoryPressureSignal {
            page_faults: constants::EBPF_PAGE_FAULT_THRESHOLD + 100,
            cache_misses: 0,
        };
        state.record_memory_pressure(signal, 1_000);
        state.record_memory_pressure(signal, 1_001);
        assert_eq!(
            state.global.catastrophic_count, 2,
            "each trip increments catastrophic_count"
        );
    }
}
