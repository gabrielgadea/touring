//! Runtime metrics export for observability.
//!
//! Consolidates metrics from all HookRuntime subsystems into a single
//! serializable struct. Can be exported as JSON for dashboards, logging,
//! or inter-process communication.

use serde::{Deserialize, Serialize};

/// Consolidated runtime metrics snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeMetrics {
    /// Hook execution statistics.
    pub hooks: HookMetrics,
    /// RL learning metrics (if active).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rl: Option<RlMetrics>,
    /// Bandit selection metrics (if active).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bandit: Option<BanditMetrics>,
    /// Cognitive engine metrics (if active).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cognitive: Option<CognitiveMetrics>,
    /// Cache performance metrics.
    pub cache: CacheMetrics,
    /// Session turn counter.
    pub session_turn: usize,
}

/// Cognitive engine metrics from touring-cognitive.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CognitiveMetrics {
    /// Number of nodes in the semantic graph.
    pub graph_node_count: usize,
    /// Number of edges in the semantic graph.
    pub graph_edge_count: usize,
    /// Focus cache hit rate (0.0-1.0).
    pub focus_cache_hit_rate: f64,
    /// Number of correct predictions by SessionPredictor.
    pub prediction_accuracy: f64,
    /// Whether the cognitive runtime is connected.
    pub is_connected: bool,
    /// Latest analysis quality score from touring-analysis pipeline (0.0-1.0).
    /// `None` if no analysis has completed yet in this session.
    #[serde(default)]
    pub analysis_quality: Option<f64>,
}

/// Hook execution statistics.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HookMetrics {
    /// Total number of hooks fired in this session.
    pub total_hooks_fired: u32,
    /// Number of hooks that completed successfully.
    pub success_count: u32,
    /// Number of hooks that failed.
    pub failure_count: u32,
    /// Average latency across all hooks (milliseconds).
    pub avg_latency_ms: f64,
    /// Maximum observed latency (milliseconds).
    pub max_latency_ms: u64,
    /// Success rate (0.0-1.0).
    pub success_rate: f64,
}

/// RL convergence metrics from QTable/OnlineRL.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RlMetrics {
    /// Exponential moving average of absolute TD error.
    pub td_error_ema: f64,
    /// Average reward over the sliding window.
    pub avg_reward: f64,
    /// Total number of RL updates processed.
    pub total_updates: u64,
    /// Whether the RL loop is converging (TD error low, reward positive).
    pub is_converging: bool,
    /// Whether the RL loop is diverging (TD error high, reward negative).
    pub is_diverging: bool,
}

/// Bandit arm selection metrics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BanditMetrics {
    /// Total arm pulls across all arms.
    pub total_pulls: u64,
    /// Number of available arms.
    pub num_arms: usize,
    /// Bandit implementation type ("linucb", "ast_enriched", "transfer").
    pub bandit_type: String,
}

/// Cache hit/miss metrics.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CacheMetrics {
    /// Cache hit rate (0.0-1.0).
    pub hit_rate: f64,
}

// ── S11: Per-hook event metrics (atomic, daemon-lifetime) ──────────────

use std::sync::atomic::{AtomicU64, Ordering};

/// Per-hook event metrics accumulated during daemon lifetime.
///
/// Uses atomics for lock-free concurrent updates from the daemon's
/// request handling path. These are NOT serialized — they live in-memory
/// only and reset when the daemon restarts.
#[derive(Debug, Default)]
pub struct HookEventMetrics {
    /// Total number of hook invocations recorded.
    pub invocations: AtomicU64,
    /// Cumulative latency across all invocations, in milliseconds.
    pub total_latency_ms: AtomicU64,
    /// Cumulative bytes of context injected into hook payloads.
    pub context_bytes_injected: AtomicU64,
    /// Number of invocations served from cache.
    pub cache_hits: AtomicU64,
    /// Number of fallbacks to standalone execution when the daemon was unavailable.
    pub fallback_count: AtomicU64,
}

impl HookEventMetrics {
    /// Creates a zeroed metrics instance usable in `const` contexts.
    pub const fn new() -> Self {
        Self {
            invocations: AtomicU64::new(0),
            total_latency_ms: AtomicU64::new(0),
            context_bytes_injected: AtomicU64::new(0),
            cache_hits: AtomicU64::new(0),
            fallback_count: AtomicU64::new(0),
        }
    }

    /// Average latency per invocation in milliseconds.
    pub fn avg_latency_ms(&self) -> f64 {
        let inv = self.invocations.load(Ordering::Relaxed);
        if inv == 0 {
            return 0.0;
        }
        self.total_latency_ms.load(Ordering::Relaxed) as f64 / inv as f64
    }

    /// Record a single hook invocation with its latency and context size.
    pub fn record_invocation(&self, latency_ms: u64, context_bytes: usize) {
        self.invocations.fetch_add(1, Ordering::Relaxed);
        self.total_latency_ms
            .fetch_add(latency_ms, Ordering::Relaxed);
        self.context_bytes_injected
            .fetch_add(context_bytes as u64, Ordering::Relaxed);
    }

    /// Record a cache hit.
    pub fn record_cache_hit(&self) {
        self.cache_hits.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a fallback (daemon unavailable, standalone execution).
    pub fn record_fallback(&self) {
        self.fallback_count.fetch_add(1, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_runtime_metrics_serialization() {
        let metrics = RuntimeMetrics {
            hooks: HookMetrics {
                total_hooks_fired: 42,
                success_count: 40,
                failure_count: 2,
                avg_latency_ms: 15.5,
                max_latency_ms: 120,
                success_rate: 40.0 / 42.0,
            },
            rl: Some(RlMetrics {
                td_error_ema: 0.05,
                avg_reward: 0.72,
                total_updates: 100,
                is_converging: true,
                is_diverging: false,
            }),
            bandit: Some(BanditMetrics {
                total_pulls: 200,
                num_arms: 8,
                bandit_type: "linucb".to_string(),
            }),
            cognitive: Some(CognitiveMetrics {
                graph_node_count: 150,
                graph_edge_count: 320,
                focus_cache_hit_rate: 0.78,
                prediction_accuracy: 0.65,
                is_connected: true,
                analysis_quality: None,
            }),
            cache: CacheMetrics { hit_rate: 0.85 },
            session_turn: 15,
        };

        let json = serde_json::to_string(&metrics).expect("serialize");
        let deser: RuntimeMetrics = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(deser.hooks.total_hooks_fired, 42);
        assert_eq!(deser.session_turn, 15);
        assert!(deser.rl.is_some());
        assert!(deser.bandit.is_some());
        assert!(deser.cognitive.is_some());
        let cog = deser.cognitive.unwrap();
        assert_eq!(cog.graph_node_count, 150);
        assert!(cog.is_connected);
    }

    #[test]
    fn test_runtime_metrics_optional_fields_omitted() {
        let metrics = RuntimeMetrics {
            hooks: HookMetrics::default(),
            rl: None,
            bandit: None,
            cognitive: None,
            cache: CacheMetrics::default(),
            session_turn: 0,
        };

        let json = serde_json::to_string(&metrics).expect("serialize");
        // rl, bandit, and cognitive should not appear in JSON when None
        assert!(!json.contains("\"rl\""));
        assert!(!json.contains("\"bandit\""));
        assert!(!json.contains("\"cognitive\""));
    }

    #[test]
    fn test_hook_metrics_default() {
        let hm = HookMetrics::default();
        assert_eq!(hm.total_hooks_fired, 0);
        assert_eq!(hm.success_count, 0);
        assert_eq!(hm.failure_count, 0);
        assert!((hm.avg_latency_ms - 0.0).abs() < f64::EPSILON);
        assert_eq!(hm.max_latency_ms, 0);
        assert!((hm.success_rate - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_cache_metrics_default() {
        let cm = CacheMetrics::default();
        assert!((cm.hit_rate - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_roundtrip_with_pretty_json() {
        let metrics = RuntimeMetrics {
            hooks: HookMetrics {
                total_hooks_fired: 10,
                success_count: 9,
                failure_count: 1,
                avg_latency_ms: 8.3,
                max_latency_ms: 50,
                success_rate: 0.9,
            },
            rl: None,
            bandit: None,
            cognitive: None,
            cache: CacheMetrics { hit_rate: 0.5 },
            session_turn: 3,
        };

        let pretty = serde_json::to_string_pretty(&metrics).expect("pretty serialize");
        let deser: RuntimeMetrics = serde_json::from_str(&pretty).expect("deserialize");
        assert_eq!(deser.hooks.success_count, 9);
        assert!((deser.cache.hit_rate - 0.5).abs() < f64::EPSILON);
    }

    // ── S11: HookEventMetrics tests ──────────────────────────────────

    #[test]
    fn test_hook_event_metrics_new() {
        let m = HookEventMetrics::new();
        assert_eq!(m.invocations.load(Ordering::Relaxed), 0);
        assert_eq!(m.total_latency_ms.load(Ordering::Relaxed), 0);
        assert_eq!(m.context_bytes_injected.load(Ordering::Relaxed), 0);
        assert_eq!(m.cache_hits.load(Ordering::Relaxed), 0);
        assert_eq!(m.fallback_count.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn test_hook_event_metrics_record() {
        let m = HookEventMetrics::new();
        m.record_invocation(10, 512);
        m.record_invocation(20, 256);
        assert_eq!(m.invocations.load(Ordering::Relaxed), 2);
        assert_eq!(m.total_latency_ms.load(Ordering::Relaxed), 30);
        assert_eq!(m.context_bytes_injected.load(Ordering::Relaxed), 768);
        assert!((m.avg_latency_ms() - 15.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_hook_event_metrics_avg_zero() {
        let m = HookEventMetrics::new();
        assert!((m.avg_latency_ms() - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_hook_event_metrics_cache_hit() {
        let m = HookEventMetrics::new();
        m.record_cache_hit();
        m.record_cache_hit();
        assert_eq!(m.cache_hits.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn test_hook_event_metrics_fallback() {
        let m = HookEventMetrics::new();
        m.record_fallback();
        assert_eq!(m.fallback_count.load(Ordering::Relaxed), 1);
    }
}
