//! Rules Handlers — Pure Rust decision logic (previously via zen-engine).
//!
//! Replaced JSON decision tables with inline Rust for zero dependencies.
//!
//! Decision logic:
//! - `context_injection`: determines what context to inject based on file type and error state
//! - `health_thresholds`: evaluates metric health against thresholds
//! - `reward_weights`: computes data-driven RL reward weights per CILA level + file type

use crate::context::CortexContext;
use crate::handler::Handler;
use crate::pipeline::Pipeline;
use crate::types::{HandlerResult, HookEvent};

// ── Pure Rust decision tables ────────────────────────────────────────

/// Context injection decision: injection_type + priority.
#[derive(Debug)]
struct ContextInjectionDecision {
    injection_type: &'static str,
    priority: u32,
}

fn decide_context_injection(
    file_type: &str,
    last_edit_failed: bool,
    has_gotchas: bool,
) -> ContextInjectionDecision {
    // context_injection.json: hitPolicy=first, rules evaluated in order
    if last_edit_failed {
        return decide_failed_edit_context(file_type);
    }
    if has_gotchas {
        return ContextInjectionDecision {
            injection_type: "gotcha",
            priority: 3,
        };
    }
    if file_type == "rs" {
        return ContextInjectionDecision {
            injection_type: "overview",
            priority: 4,
        };
    }
    ContextInjectionDecision {
        injection_type: "none",
        priority: 5,
    }
}

#[inline]
fn decide_failed_edit_context(file_type: &str) -> ContextInjectionDecision {
    match file_type {
        "rs" => ContextInjectionDecision {
            injection_type: "full_enrichment",
            priority: 1,
        },
        "py" => ContextInjectionDecision {
            injection_type: "overview_gotcha",
            priority: 2,
        },
        _ => ContextInjectionDecision {
            injection_type: "gotcha",
            priority: 3,
        },
    }
}

/// Health thresholds decision: warning / critical / fatal.
#[derive(Debug)]
struct HealthThresholdsDecision {
    #[allow(dead_code)] // part of decision table API, not all fields used in every handler
    warning: f64,
    critical: f64,
    fatal: f64,
}

fn health_thresholds_for(metric_name: &str, context: &str) -> HealthThresholdsDecision {
    if metric_name == "latency_ms" {
        return latency_thresholds(context);
    }
    match metric_name {
        "error_rate_pct" => HealthThresholdsDecision {
            warning: 5.0,
            critical: 15.0,
            fatal: 30.0,
        },
        "complexity" => HealthThresholdsDecision {
            warning: 8.0,
            critical: 12.0,
            fatal: 20.0,
        },
        "memory_mb" => HealthThresholdsDecision {
            warning: 100.0,
            critical: 500.0,
            fatal: 1000.0,
        },
        _ => HealthThresholdsDecision {
            warning: 70.0,
            critical: 50.0,
            fatal: 30.0,
        },
    }
}

#[inline]
fn latency_thresholds(context: &str) -> HealthThresholdsDecision {
    match context {
        "hook" => HealthThresholdsDecision {
            warning: 10.0,
            critical: 50.0,
            fatal: 100.0,
        },
        "mcp" => HealthThresholdsDecision {
            warning: 1000.0,
            critical: 3000.0,
            fatal: 5000.0,
        },
        _ => HealthThresholdsDecision {
            warning: 70.0,
            critical: 50.0,
            fatal: 30.0,
        },
    }
}

/// Reward weights decision: compilation / lint / type_safe / tests / coverage / simplicity.
#[derive(Debug)]
struct RewardWeightsDecision {
    compilation_pct: f64,
    lint_pct: f64,
    type_safe_pct: f64,
    tests_pct: f64,
    coverage_pct: f64,
    simplicity_alpha_pct: f64,
}

fn reward_weights_for(cila_level: u32, file_type: &str) -> RewardWeightsDecision {
    // reward_weights.json: hitPolicy=first, i1=cila_level >= 3 means high CILA
    let high_cila = cila_level >= 3;
    match (high_cila, file_type) {
        (true, "rs") => RewardWeightsDecision {
            compilation_pct: 30.0,
            lint_pct: 15.0,
            type_safe_pct: 25.0,
            tests_pct: 20.0,
            coverage_pct: 10.0,
            simplicity_alpha_pct: 15.0,
        },
        (true, "py") => RewardWeightsDecision {
            compilation_pct: 25.0,
            lint_pct: 20.0,
            type_safe_pct: 15.0,
            tests_pct: 25.0,
            coverage_pct: 15.0,
            simplicity_alpha_pct: 10.0,
        },
        (false, "rs") => RewardWeightsDecision {
            compilation_pct: 25.0,
            lint_pct: 20.0,
            type_safe_pct: 25.0,
            tests_pct: 20.0,
            coverage_pct: 10.0,
            simplicity_alpha_pct: 10.0,
        },
        (false, "py") => RewardWeightsDecision {
            compilation_pct: 25.0,
            lint_pct: 20.0,
            type_safe_pct: 20.0,
            tests_pct: 25.0,
            coverage_pct: 10.0,
            simplicity_alpha_pct: 10.0,
        },
        _ => RewardWeightsDecision {
            compilation_pct: 25.0,
            lint_pct: 20.0,
            type_safe_pct: 20.0,
            tests_pct: 25.0,
            coverage_pct: 10.0,
            simplicity_alpha_pct: 10.0,
        },
    }
}

// ── H75: RulesContextRouter ──────────────────────────────────────────

/// H75: Uses the `context_injection` decision table to determine optimal context
/// injection strategy based on file type and error state.
pub struct RulesContextRouter;

impl Handler for RulesContextRouter {
    fn name(&self) -> &str {
        "RulesContextRouter"
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::PreToolUse]
    }

    fn tool_matcher(&self) -> Option<&str> {
        Some("Read|Edit|Write|MultiEdit")
    }

    fn execute(&self, ctx: &mut CortexContext) -> HandlerResult {
        // Extract file type from tool_input.file_path
        let file_type = ctx
            .tool_input
            .get("file_path")
            .and_then(|v| v.as_str())
            .and_then(|p| p.rsplit('.').next())
            .unwrap_or("unknown")
            .to_string();

        let last_edit_failed = ctx
            .input
            .get("last_edit_failed")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let has_gotchas = ctx
            .input
            .get("has_gotchas")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let decision = decide_context_injection(&file_type, last_edit_failed, has_gotchas);

        if decision.injection_type == "none" {
            return HandlerResult::skip(self.name());
        }

        let context_msg = format!(
            "[rules] injection={} priority={} file_type={}",
            decision.injection_type, decision.priority, file_type
        );
        let mut hr = HandlerResult::allow(self.name(), Some(context_msg));
        hr.metrics = serde_json::json!({
            "injection_type": decision.injection_type,
            "priority": decision.priority,
        });
        hr
    }
}

// ── H76: RulesHealthMonitor ──────────────────────────────────────────

/// H76: Uses the `health_thresholds` decision table to check metrics against
/// configured thresholds and inject warnings when thresholds are breached.
pub struct RulesHealthMonitor;

impl Handler for RulesHealthMonitor {
    fn name(&self) -> &str {
        "RulesHealthMonitor"
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::PostToolUse]
    }

    fn is_async(&self) -> bool {
        true // non-blocking
    }

    fn execute(&self, ctx: &mut CortexContext) -> HandlerResult {
        let tool_name = ctx.tool_name.as_deref().unwrap_or("unknown");

        let thresholds = health_thresholds_for("latency_ms", "hook");

        // Sum handler latency from accumulated handler metrics
        let latency_ms: f64 = ctx
            .handler_metrics
            .iter()
            .filter_map(|(_, m)| m.get("duration_ms").and_then(|v| v.as_f64()))
            .sum();

        if latency_ms > thresholds.fatal {
            let context_msg = format!(
                "[health] CRITICAL: {} pipeline latency {:.1}ms > {:.0}ms fatal threshold",
                tool_name, latency_ms, thresholds.fatal
            );
            let mut hr = HandlerResult::allow(self.name(), Some(context_msg));
            hr.metrics = serde_json::json!({
                "health_alert": "critical",
                "metric": "latency_ms",
                "value": latency_ms,
                "threshold": thresholds.fatal,
            });
            return hr;
        } else if latency_ms > thresholds.warning {
            // WARNING tier: latency exceeds warning threshold but stays below critical
            let context_msg = format!(
                "[health] WARNING: {} pipeline latency {:.1}ms > {:.0}ms warning threshold",
                tool_name, latency_ms, thresholds.warning
            );
            let mut hr = HandlerResult::allow(self.name(), Some(context_msg));
            hr.metrics = serde_json::json!({
                "health_alert": "warning",
                "metric": "latency_ms",
                "value": latency_ms,
                "threshold": thresholds.warning,
            });
            return hr;
        } else if latency_ms > thresholds.critical {
            // ELEVATED tier: above critical but below fatal
            let context_msg = format!(
                "[health] ELEVATED: {} pipeline latency {:.1}ms > {:.0}ms critical threshold",
                tool_name, latency_ms, thresholds.critical
            );
            let mut hr = HandlerResult::allow(self.name(), Some(context_msg));
            hr.metrics = serde_json::json!({
                "health_alert": "elevated",
                "metric": "latency_ms",
                "value": latency_ms,
                "threshold": thresholds.critical,
            });
            return hr;
        }

        HandlerResult::skip(self.name())
    }
}

// ── H88: RewardWeightsRulesHandler ─────────────────────────────────

/// H88: Evaluates the `reward_weights` decision table after each tool use.
///
/// Uses the reward_weights table to determine data-driven quality weights
/// for the current file type and CILA level, then reports them to the
/// drift monitor for downstream RL reward shaping.
pub struct RewardWeightsRulesHandler;

impl Handler for RewardWeightsRulesHandler {
    fn name(&self) -> &str {
        "H88_reward_weights_rules"
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::PostToolUse]
    }

    fn priority(&self) -> u8 {
        185
    }

    fn dependency_tier(&self) -> u8 {
        0
    }

    fn timeout_ms(&self) -> u64 {
        60
    }

    fn execute(&self, ctx: &mut CortexContext) -> HandlerResult {
        let file_type = ctx
            .file_path
            .as_deref()
            .and_then(|p| p.rsplit('.').next())
            .unwrap_or("unknown");

        let cila_level = ctx
            .input
            .as_object()
            .and_then(|o| o.get("cila_level"))
            .and_then(|v| v.as_u64())
            .unwrap_or(2) as u32;

        let weights = reward_weights_for(cila_level, file_type);

        // Report each weight to drift monitor
        if let Some(ref persistence) = ctx.persistence {
            let _ = persistence
                .drift_record("reward_weight:compilation", weights.compilation_pct / 100.0);
            let _ = persistence.drift_record("reward_weight:lint", weights.lint_pct / 100.0);
            let _ =
                persistence.drift_record("reward_weight:type_safe", weights.type_safe_pct / 100.0);
            let _ = persistence.drift_record("reward_weight:tests", weights.tests_pct / 100.0);
            let _ =
                persistence.drift_record("reward_weight:coverage", weights.coverage_pct / 100.0);
            let _ = persistence.drift_record(
                "reward_weight:simplicity",
                weights.simplicity_alpha_pct / 100.0,
            );
        }

        let context_line = format!(
            "reward weights: comp={:.0}% lint={:.0}% tests={:.0}%",
            weights.compilation_pct, weights.lint_pct, weights.tests_pct
        );
        HandlerResult::allow(self.name(), Some(context_line))
    }
}

/// Register rules handlers in the pipeline.
pub fn register(pipeline: &mut Pipeline) {
    pipeline.register(Box::new(RulesContextRouter));
    pipeline.register(Box::new(RulesHealthMonitor));
    pipeline.register(Box::new(RewardWeightsRulesHandler));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_context_injection_rust_failed_edit() {
        let d = decide_context_injection("rs", true, false);
        assert_eq!(d.injection_type, "full_enrichment");
        assert_eq!(d.priority, 1);
    }

    #[test]
    fn test_context_injection_python_failed_edit() {
        let d = decide_context_injection("py", true, false);
        assert_eq!(d.injection_type, "overview_gotcha");
        assert_eq!(d.priority, 2);
    }

    #[test]
    fn test_context_injection_has_gotchas() {
        let d = decide_context_injection("ts", false, true);
        assert_eq!(d.injection_type, "gotcha");
        assert_eq!(d.priority, 3);
    }

    #[test]
    fn test_context_injection_rust_no_failure() {
        let d = decide_context_injection("rs", false, false);
        assert_eq!(d.injection_type, "overview");
        assert_eq!(d.priority, 4);
    }

    #[test]
    fn test_context_injection_default() {
        let d = decide_context_injection("md", false, false);
        assert_eq!(d.injection_type, "none");
        assert_eq!(d.priority, 5);
    }

    #[test]
    fn test_health_thresholds_latency_hook() {
        let d = health_thresholds_for("latency_ms", "hook");
        assert_eq!(d.warning, 10.0);
        assert_eq!(d.critical, 50.0);
        assert_eq!(d.fatal, 100.0);
    }

    #[test]
    fn test_health_thresholds_latency_mcp() {
        let d = health_thresholds_for("latency_ms", "mcp");
        assert_eq!(d.warning, 1000.0);
        assert_eq!(d.critical, 3000.0);
        assert_eq!(d.fatal, 5000.0);
    }

    #[test]
    fn test_health_thresholds_error_rate() {
        let d = health_thresholds_for("error_rate_pct", "");
        assert_eq!(d.warning, 5.0);
        assert_eq!(d.critical, 15.0);
        assert_eq!(d.fatal, 30.0);
    }

    #[test]
    fn test_health_thresholds_default_fallback() {
        let d = health_thresholds_for("unknown_metric", "any");
        assert_eq!(d.warning, 70.0);
        assert_eq!(d.critical, 50.0);
        assert_eq!(d.fatal, 30.0);
    }

    #[test]
    fn test_reward_weights_rust_high_cila() {
        let d = reward_weights_for(4, "rs");
        assert_eq!(d.compilation_pct, 30.0);
        assert_eq!(d.simplicity_alpha_pct, 15.0);
    }

    #[test]
    fn test_reward_weights_python_default() {
        let d = reward_weights_for(1, "py");
        assert_eq!(d.compilation_pct, 25.0);
        assert_eq!(d.tests_pct, 25.0);
        assert_eq!(d.simplicity_alpha_pct, 10.0);
    }

    #[test]
    fn test_reward_weights_rust_low_cila() {
        let d = reward_weights_for(1, "rs");
        assert_eq!(d.compilation_pct, 25.0);
        assert_eq!(d.type_safe_pct, 25.0);
    }

    #[test]
    fn test_h88_name() {
        let handler = RewardWeightsRulesHandler;
        assert_eq!(handler.name(), "H88_reward_weights_rules");
    }

    #[test]
    fn test_h88_priority() {
        let handler = RewardWeightsRulesHandler;
        assert_eq!(handler.priority(), 185);
    }

    #[test]
    fn test_rules_context_router_name() {
        let handler = RulesContextRouter;
        assert_eq!(handler.name(), "RulesContextRouter");
    }

    #[test]
    fn test_rules_health_monitor_name() {
        let handler = RulesHealthMonitor;
        assert_eq!(handler.name(), "RulesHealthMonitor");
    }
}
