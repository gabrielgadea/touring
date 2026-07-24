//! Hook execution metrics — aggregation and observability for the Cortex pipeline.
//!
//! Tracks per-handler execution time, decision counts, and failure rates.
//! Injected into CortexOutput.metrics for downstream observability.

use serde::Serialize;

/// Metrics for a single handler execution.
#[derive(Debug, Clone, Serialize)]
pub struct HandlerMetric {
    /// Handler name.
    pub name: String,
    /// Execution time in milliseconds.
    pub duration_ms: f64,
    /// Decision: "allow", "block", or "skip".
    pub decision: String,
}

/// Aggregated metrics for a complete pipeline execution.
#[derive(Debug, Clone, Default, Serialize)]
pub struct PipelineMetrics {
    /// Individual handler metrics.
    pub handlers: Vec<HandlerMetric>,
    /// Total pipeline execution time in milliseconds.
    pub total_duration_ms: f64,
    /// Number of handlers that executed.
    pub handler_count: usize,
    /// Number of handlers that produced context.
    pub context_producing: usize,
    /// Slowest handler name and duration.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slowest: Option<(String, f64)>,
    /// Whether any handler blocked.
    pub had_block: bool,
}

impl PipelineMetrics {
    /// Create a new empty metrics collector.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a handler execution.
    pub fn record(&mut self, name: &str, duration_ms: f64, decision: &str, produced_context: bool) {
        self.handlers.push(HandlerMetric {
            name: name.to_string(),
            duration_ms,
            decision: decision.to_string(),
        });
        self.total_duration_ms += duration_ms;
        self.handler_count += 1;
        if produced_context {
            self.context_producing += 1;
        }
        if decision == "block" {
            self.had_block = true;
        }

        // Track slowest handler
        match &self.slowest {
            Some((_, prev_ms)) if duration_ms > *prev_ms => {
                self.slowest = Some((name.to_string(), duration_ms));
            }
            None => {
                self.slowest = Some((name.to_string(), duration_ms));
            }
            _ => {}
        }
    }

    /// Convert to a JSON value for CortexOutput.metrics.
    pub fn to_json(&self) -> serde_json::Value {
        // Only include per-handler detail if there are slow handlers (>5ms)
        let slow_handlers: Vec<&HandlerMetric> = self
            .handlers
            .iter()
            .filter(|h| h.duration_ms > 5.0)
            .collect();

        let mut output = serde_json::json!({
            "total_ms": format!("{:.1}", self.total_duration_ms),
            "handlers_executed": self.handler_count,
            "context_producing": self.context_producing,
            "had_block": self.had_block,
        });

        if let Some((ref name, ms)) = self.slowest {
            if let Some(slot) = output.get_mut("slowest_handler") {
                *slot = serde_json::json!({
                    "name": name,
                    "ms": format!("{:.1}", ms),
                });
            } else if let Some(obj) = output.as_object_mut() {
                obj.insert(
                    "slowest_handler".to_string(),
                    serde_json::json!({
                        "name": name,
                        "ms": format!("{:.1}", ms),
                    }),
                );
            }
        }

        if !slow_handlers.is_empty() {
            let handlers_val = serde_json::json!(
                slow_handlers
                    .iter()
                    .map(|h| serde_json::json!({
                        "name": h.name,
                        "ms": format!("{:.1}", h.duration_ms),
                        "decision": h.decision,
                    }))
                    .collect::<Vec<_>>()
            );
            if let Some(obj) = output.as_object_mut() {
                obj.insert("slow_handlers".to_string(), handlers_val);
            }
        }

        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_metrics() {
        let m = PipelineMetrics::new();
        assert_eq!(m.handler_count, 0);
        assert!(!m.had_block);
        assert!(m.slowest.is_none());
    }

    #[test]
    fn record_and_aggregate() {
        let mut m = PipelineMetrics::new();
        m.record("h1", 2.5, "allow", true);
        m.record("h2", 8.0, "skip", false);
        m.record("h3", 1.0, "allow", true);

        assert_eq!(m.handler_count, 3);
        assert_eq!(m.context_producing, 2);
        assert!(!m.had_block);
        assert_eq!(m.slowest.as_ref().unwrap().0, "h2");
        assert!((m.total_duration_ms - 11.5).abs() < 0.01);
    }

    #[test]
    fn block_detection() {
        let mut m = PipelineMetrics::new();
        m.record("enforcer", 0.5, "block", false);
        assert!(m.had_block);
    }

    #[test]
    fn to_json_structure() {
        let mut m = PipelineMetrics::new();
        m.record("fast", 1.0, "allow", true);
        m.record("slow", 10.0, "allow", false);

        let json = m.to_json();
        assert_eq!(json["handlers_executed"], 2);
        assert!(json["slow_handlers"].is_array());
        // Only "slow" should appear (>5ms threshold)
        let slow = json["slow_handlers"].as_array().unwrap();
        assert_eq!(slow.len(), 1);
        assert_eq!(slow[0]["name"], "slow");
    }

    // ── Additional coverage ───────────────────────────────────────────

    #[test]
    fn default_is_same_as_new() {
        let m1 = PipelineMetrics::new();
        let m2 = PipelineMetrics::default();
        assert_eq!(m1.handler_count, m2.handler_count);
        assert_eq!(m1.had_block, m2.had_block);
        assert_eq!(m1.total_duration_ms, m2.total_duration_ms);
        assert_eq!(m1.context_producing, m2.context_producing);
    }

    #[test]
    fn slowest_updates_correctly() {
        let mut m = PipelineMetrics::new();
        m.record("first", 5.0, "allow", false);
        assert_eq!(m.slowest.as_ref().unwrap().0, "first");

        m.record("faster", 2.0, "allow", false);
        // Still "first"
        assert_eq!(m.slowest.as_ref().unwrap().0, "first");

        m.record("slowest", 20.0, "skip", false);
        assert_eq!(m.slowest.as_ref().unwrap().0, "slowest");
        assert!((m.slowest.as_ref().unwrap().1 - 20.0).abs() < 0.001);
    }

    #[test]
    fn context_producing_count() {
        let mut m = PipelineMetrics::new();
        m.record("a", 1.0, "allow", true);
        m.record("b", 1.0, "skip", false);
        m.record("c", 1.0, "allow", true);
        m.record("d", 1.0, "block", false);
        assert_eq!(m.context_producing, 2);
        assert_eq!(m.handler_count, 4);
    }

    #[test]
    fn had_block_is_false_with_no_blocks() {
        let mut m = PipelineMetrics::new();
        m.record("a", 1.0, "allow", true);
        m.record("b", 1.0, "skip", false);
        assert!(!m.had_block);
    }

    #[test]
    fn had_block_is_true_after_block() {
        let mut m = PipelineMetrics::new();
        m.record("a", 1.0, "allow", false);
        assert!(!m.had_block);
        m.record("guard", 0.2, "block", false);
        assert!(m.had_block);
    }

    #[test]
    fn total_duration_accumulates() {
        let mut m = PipelineMetrics::new();
        m.record("h1", 1.1, "allow", false);
        m.record("h2", 2.2, "skip", false);
        m.record("h3", 3.3, "allow", false);
        assert!((m.total_duration_ms - 6.6).abs() < 0.001);
    }

    #[test]
    fn to_json_has_block_flag() {
        let mut m = PipelineMetrics::new();
        m.record("blocker", 0.5, "block", false);
        let json = m.to_json();
        assert_eq!(json["had_block"], true);
    }

    #[test]
    fn to_json_no_slow_handlers_below_threshold() {
        let mut m = PipelineMetrics::new();
        m.record("quick1", 1.0, "allow", true);
        m.record("quick2", 3.0, "skip", false);
        let json = m.to_json();
        // None of the handlers exceed 5ms threshold
        assert!(
            json["slow_handlers"].is_null()
                || !json["slow_handlers"].is_array()
                || json["slow_handlers"]
                    .as_array()
                    .map(|a| a.is_empty())
                    .unwrap_or(true)
        );
    }

    #[test]
    fn to_json_includes_context_producing_count() {
        let mut m = PipelineMetrics::new();
        m.record("h1", 1.0, "allow", true);
        m.record("h2", 1.0, "skip", false);
        let json = m.to_json();
        assert_eq!(json["context_producing"], 1);
    }

    #[test]
    fn to_json_total_ms_format() {
        let mut m = PipelineMetrics::new();
        m.record("h", 7.5, "allow", false);
        let json = m.to_json();
        let total = json["total_ms"].as_str().unwrap_or("");
        assert!(
            total.contains('.'),
            "total_ms should be formatted with decimal: {total}"
        );
    }

    #[test]
    fn handler_metric_fields() {
        let hm = HandlerMetric {
            name: "test_handler".to_string(),
            duration_ms: 2.5,
            decision: "allow".to_string(),
        };
        assert_eq!(hm.name, "test_handler");
        assert!((hm.duration_ms - 2.5).abs() < 0.001);
        assert_eq!(hm.decision, "allow");
    }

    #[test]
    fn handler_metric_serializable() {
        let hm = HandlerMetric {
            name: "my_handler".to_string(),
            duration_ms: 1.5,
            decision: "skip".to_string(),
        };
        let json = serde_json::to_string(&hm).unwrap();
        assert!(json.contains("my_handler"));
        assert!(json.contains("skip"));
    }

    #[test]
    fn pipeline_metrics_serializable() {
        let mut m = PipelineMetrics::new();
        m.record("h1", 2.0, "allow", true);
        let json = serde_json::to_string(&m).unwrap();
        assert!(json.contains("handlers"));
        assert!(json.contains("h1"));
    }

    #[test]
    fn single_handler_is_both_first_and_slowest() {
        let mut m = PipelineMetrics::new();
        m.record("only", 9.9, "allow", true);
        assert_eq!(m.slowest.as_ref().unwrap().0, "only");
        assert!((m.slowest.as_ref().unwrap().1 - 9.9).abs() < 0.001);
        assert_eq!(m.handler_count, 1);
        assert_eq!(m.context_producing, 1);
    }
}
