//! `SpeculateBridge` — wraps `touring shadow validate` CLI for pre-commit validation.

use crate::core::context::TelemetrySink;
use crate::core::score::NormalizedScore;
use crate::error::GenerateError;
use crate::plan::result::{LayerResult, SpeculateReport};
use std::sync::Arc;
use uuid::Uuid;

/// Bridge to `touring shadow validate` speculative validation.
///
/// Invokes the touring daemon via CLI subprocess. Returns a [`SpeculateReport`]
/// summarising composite score and per-layer results.
pub struct SpeculateBridge {
    metrics: Arc<dyn TelemetrySink>,
}

impl SpeculateBridge {
    /// Construct a new bridge with a telemetry sink.
    #[must_use]
    pub fn new(metrics: Arc<dyn TelemetrySink>) -> Self {
        Self { metrics }
    }

    /// Run speculative validation on the given file content.
    ///
    /// Returns a [`SpeculateReport`]. If the daemon is unavailable, returns a
    /// conservative report with score 0.0 so the gate fails safely.
    ///
    /// # Errors
    ///
    /// Returns [`GenerateError::Internal`] if the `touring shadow validate` subprocess
    /// cannot be spawned.
    pub async fn validate(
        &self,
        plan_id: Uuid,
        _content: &str,
    ) -> Result<SpeculateReport, GenerateError> {
        let start = std::time::Instant::now();

        let output = tokio::process::Command::new("touring")
            .args(["shadow", "validate", "-j"])
            .output()
            .await
            .map_err(|e| GenerateError::Internal(format!("speculate spawn error: {e}")))?;

        let elapsed = start.elapsed();
        let elapsed_ms = u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX);
        self.metrics.record_histogram(
            "speculate.validate.latency_ms",
            elapsed.as_secs_f64() * 1_000.0,
        );

        if !output.status.success() {
            // Daemon unavailable — conservative fail-safe.
            return Ok(Self::conservative_fail(plan_id, elapsed_ms));
        }

        let text = String::from_utf8_lossy(&output.stdout);
        let report = Self::parse_output(plan_id, &text, elapsed_ms);
        Ok(report)
    }

    /// Parse the JSON output from `touring shadow validate -j`.
    fn parse_output(plan_id: Uuid, text: &str, elapsed_ms: u64) -> SpeculateReport {
        let v: serde_json::Value = match serde_json::from_str(text) {
            Ok(v) => v,
            Err(_) => return Self::conservative_fail(plan_id, elapsed_ms),
        };

        let score_raw = v
            .get("score")
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(0.0);
        let composite_score = NormalizedScore::clamped(score_raw);
        let all_passed = v
            .get("all_passed")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(composite_score.value() >= 0.8);

        let layers = Self::parse_layers(&v);

        SpeculateReport {
            plan_id,
            composite_score,
            all_passed,
            layers,
            elapsed_ms,
        }
    }

    /// Parse per-layer results from the JSON payload.
    fn parse_layers(v: &serde_json::Value) -> Vec<LayerResult> {
        let layer_names = ["syntax", "symbol", "structural", "import", "complexity"];
        layer_names
            .iter()
            .map(|name| {
                let score_raw = v
                    .get(*name)
                    .and_then(serde_json::Value::as_f64)
                    .unwrap_or(0.0);
                let score = NormalizedScore::clamped(score_raw);
                // Layer passes if score >= 0.5 (informational layers can be lower).
                let passed = score.value() >= 0.5;
                LayerResult {
                    name: (*name).to_string(),
                    score,
                    passed,
                    issues: Vec::new(),
                    elapsed_ms: 0,
                }
            })
            .collect()
    }

    /// Conservative fail-safe report used when the daemon is unavailable.
    fn conservative_fail(plan_id: Uuid, elapsed_ms: u64) -> SpeculateReport {
        SpeculateReport {
            plan_id,
            composite_score: NormalizedScore::ZERO,
            all_passed: false,
            layers: Vec::new(),
            elapsed_ms,
        }
    }
}
