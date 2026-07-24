//! Wave 2 P5 (Sentrux master plan, 2026-05-09) — MCP tools that
//! expose the workspace quality signal pipeline:
//!
//! * `touring_quality_signal_compute` — walk a path, build a
//!   [`Workspace`], and return the
//!   [`WorkspaceQualitySignal`].
//! * `touring_quality_rules_evaluate` — compute a signal and run a
//!   [`MetricRuleSet`] against it, returning categorised
//!   [`MetricViolation`]s.
//! * `touring_quality_signal_diff` — compute signals at two roots and
//!   return a [`SignalDiff`] (trend, per-root-cause delta, bottleneck
//!   rotation).

use std::path::PathBuf;

use rmcp::{
    ErrorData as McpError, handler::server::wrapper::Parameters, model::*, tool, tool_router,
};

use touring_analysis::quality::signal::{
    DEFAULT_TREND_EPSILON, build_workspace_from_path, compute_quality_signal,
    diff_signals_with_epsilon,
};
use touring_analysis::rules::{count_by_severity, evaluate, parse_path, parse_str};
use touring_hooks::shared::federation::{FederationEntry, aggregate};

use super::TouringServer;
use super::params::{
    self, QualityFederationAggregateParams, QualityRulesEvaluateParams, QualitySignalComputeParams,
    QualitySignalDiffParams,
};

#[tool_router(router = router_quality_signal, vis = "pub(crate)")]
impl TouringServer {
    // ── touring_quality_signal_compute ─────────────────────────────────────

    /// Workspace quality signal — Sentrux 0..=10000 + bottleneck.
    #[tool(
        name = "touring_quality_signal_compute",
        description = "Compute the workspace quality signal (Sentrux 0..=10000 geometric mean of 5 root causes) for a path. Returns: signal_0_10000, signal_normalized, bottleneck, root_causes, raw, diagnostics."
    )]
    async fn quality_signal_compute(
        &self,
        params: Parameters<QualitySignalComputeParams>,
    ) -> Result<CallToolResult, McpError> {
        let p = params.0;
        let dl = p.detail_level.unwrap_or_default();
        let no_diagnostics = p.no_diagnostics.unwrap_or(false);

        let root = match p.root.as_deref() {
            Some(s) if !s.is_empty() => PathBuf::from(s),
            _ => self.config.project_root.clone(),
        };
        let gctx = self
            .graph_svc
            .resolve_ctx(Some(&root.display().to_string()))
            .await;

        let mut output = match build_workspace_from_path(&root) {
            Ok(ws) => {
                let signal = compute_quality_signal(&ws);
                let mut value = serde_json::to_value(&signal)
                    .unwrap_or_else(|_| serde_json::json!({"error": "failed to serialize signal"}));
                if no_diagnostics {
                    if let Some(obj) = value.as_object_mut() {
                        obj.remove("diagnostics");
                    }
                }
                if let Some(obj) = value.as_object_mut() {
                    obj.insert(
                        "root".to_string(),
                        serde_json::json!(root.display().to_string()),
                    );
                }
                value
            }
            Err(err) => serde_json::json!({
                "root": root.display().to_string(),
                "signal_0_10000": 0,
                "signal_normalized": 0.0,
                "bottleneck": "Tied",
                "error": err.to_string(),
            }),
        };

        self.graph_svc.inject(&mut output, &gctx);
        params::apply_detail_level(&mut output, dl);
        crate::tools::suggestions::append_to_response(
            &mut output,
            "touring_quality_signal_compute",
            2,
        );
        let text = serde_json::to_string_pretty(&output)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    // ── touring_quality_rules_evaluate ─────────────────────────────────────

    /// Evaluate a TOML metric rules file against a workspace.
    #[tool(
        name = "touring_quality_rules_evaluate",
        description = "Evaluate TOML metric budget rules against a workspace, returning MetricViolations categorised by severity (deny|warn|info). Accepts either rules_path (file) or rules_toml (inline string)."
    )]
    async fn quality_rules_evaluate(
        &self,
        params: Parameters<QualityRulesEvaluateParams>,
    ) -> Result<CallToolResult, McpError> {
        let p = params.0;
        let dl = p.detail_level.unwrap_or_default();
        let root = match p.root.as_deref() {
            Some(s) if !s.is_empty() => PathBuf::from(s),
            _ => self.config.project_root.clone(),
        };
        let gctx = self
            .graph_svc
            .resolve_ctx(Some(&root.display().to_string()))
            .await;

        let ruleset = match (p.rules_path.as_deref(), p.rules_toml.as_deref()) {
            (Some(path), None) => parse_path(std::path::Path::new(path)).map_err(|e| {
                McpError::invalid_params(format!("rules_path parse failed: {e}"), None)
            })?,
            (None, Some(content)) => parse_str(content).map_err(|e| {
                McpError::invalid_params(format!("rules_toml parse failed: {e}"), None)
            })?,
            (Some(_), Some(_)) => {
                return Err(McpError::invalid_params(
                    "rules_path and rules_toml are mutually exclusive".to_string(),
                    None,
                ));
            }
            (None, None) => {
                return Err(McpError::invalid_params(
                    "either rules_path or rules_toml is required".to_string(),
                    None,
                ));
            }
        };

        let ws = match build_workspace_from_path(&root) {
            Ok(w) => w,
            Err(err) => {
                let mut out = serde_json::json!({
                    "root": root.display().to_string(),
                    "error": err.to_string(),
                    "violations": [],
                    "counts": { "deny": 0, "warn": 0, "info": 0 },
                });
                self.graph_svc.inject(&mut out, &gctx);
                params::apply_detail_level(&mut out, dl);
                let text = serde_json::to_string_pretty(&out)
                    .map_err(|e| McpError::internal_error(e.to_string(), None))?;
                return Ok(CallToolResult::success(vec![Content::text(text)]));
            }
        };
        let signal = compute_quality_signal(&ws);
        let violations = evaluate(&ruleset, &ws, &signal)
            .map_err(|e| McpError::internal_error(format!("evaluate failed: {e}"), None))?;
        let (deny, warn, info) = count_by_severity(&violations);

        let mut output = serde_json::json!({
            "root": root.display().to_string(),
            "signal_0_10000": signal.signal_0_10000,
            "bottleneck": signal.bottleneck,
            "violations": violations,
            "counts": { "deny": deny, "warn": warn, "info": info },
        });

        self.graph_svc.inject(&mut output, &gctx);
        params::apply_detail_level(&mut output, dl);
        crate::tools::suggestions::append_to_response(
            &mut output,
            "touring_quality_rules_evaluate",
            2,
        );
        let text = serde_json::to_string_pretty(&output)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    // ── touring_quality_signal_diff ────────────────────────────────────────

    /// Diff two quality signals — previous vs current snapshots.
    #[tool(
        name = "touring_quality_signal_diff",
        description = "Compute quality signals at two paths and return the structural diff: signal delta, per-root-cause delta, bottleneck rotation, trend (improving|regressing|stable)."
    )]
    async fn quality_signal_diff(
        &self,
        params: Parameters<QualitySignalDiffParams>,
    ) -> Result<CallToolResult, McpError> {
        let p = params.0;
        let dl = p.detail_level.unwrap_or_default();
        let epsilon = p.trend_epsilon.unwrap_or(DEFAULT_TREND_EPSILON);
        let prev_root = PathBuf::from(&p.previous_root);
        let curr_root = PathBuf::from(&p.current_root);

        let gctx = self
            .graph_svc
            .resolve_ctx(Some(&curr_root.display().to_string()))
            .await;

        let prev_signal = match build_workspace_from_path(&prev_root) {
            Ok(ws) => compute_quality_signal(&ws),
            Err(err) => {
                return make_diff_error(&prev_root, &curr_root, "previous_root", &err.to_string());
            }
        };
        let curr_signal = match build_workspace_from_path(&curr_root) {
            Ok(ws) => compute_quality_signal(&ws),
            Err(err) => {
                return make_diff_error(&prev_root, &curr_root, "current_root", &err.to_string());
            }
        };
        let diff = diff_signals_with_epsilon(&prev_signal, &curr_signal, epsilon);

        let mut output = serde_json::json!({
            "previous_root": prev_root.display().to_string(),
            "current_root": curr_root.display().to_string(),
            "diff": diff,
        });

        self.graph_svc.inject(&mut output, &gctx);
        params::apply_detail_level(&mut output, dl);
        crate::tools::suggestions::append_to_response(
            &mut output,
            "touring_quality_signal_diff",
            2,
        );
        let text = serde_json::to_string_pretty(&output)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    // ── touring_quality_federation_aggregate ─────────────────────────────────

    /// Aggregate Sentrux quality signals across N workspaces.
    #[tool(
        name = "touring_quality_federation_aggregate",
        description = "Compute Sentrux quality signals for N workspaces (1..=64) and aggregate them into a single FederationSummary: arithmetic mean signal, min/max, stddev, bottleneck histogram, worst/best workspace, per-axis avg root causes."
    )]
    async fn quality_federation_aggregate(
        &self,
        params: Parameters<QualityFederationAggregateParams>,
    ) -> Result<CallToolResult, McpError> {
        let p = params.0;
        let dl = p.detail_level.unwrap_or_default();

        if p.workspaces.is_empty() {
            return Err(McpError::invalid_params(
                "workspaces must contain at least 1 entry".to_string(),
                None,
            ));
        }
        if p.workspaces.len() > 64 {
            return Err(McpError::invalid_params(
                format!(
                    "workspaces capped at 64 entries (got {})",
                    p.workspaces.len()
                ),
                None,
            ));
        }

        let now_ms = unix_millis_now();
        let mut entries: Vec<FederationEntry> = Vec::with_capacity(p.workspaces.len());
        let mut errors: Vec<serde_json::Value> = Vec::new();

        for ws in &p.workspaces {
            let root = PathBuf::from(&ws.root);
            match build_workspace_from_path(&root) {
                Ok(workspace) => {
                    let signal = compute_quality_signal(&workspace);
                    entries.push(FederationEntry {
                        workspace_id: ws.workspace_id.clone(),
                        workspace_root: root,
                        signal,
                        timestamp_ms: now_ms,
                    });
                }
                Err(err) => {
                    errors.push(serde_json::json!({
                        "workspace_id": ws.workspace_id,
                        "root": ws.root,
                        "error": err.to_string(),
                    }));
                }
            }
        }

        let summary = aggregate(&entries);
        let gctx = self
            .graph_svc
            .resolve_ctx(p.workspaces.first().map(|w| w.root.as_str()))
            .await;

        let mut output = serde_json::json!({
            "entries_input": p.workspaces.len(),
            "entries_aggregated": entries.len(),
            "errors": errors,
            "summary": summary,
        });

        self.graph_svc.inject(&mut output, &gctx);
        params::apply_detail_level(&mut output, dl);
        crate::tools::suggestions::append_to_response(
            &mut output,
            "touring_quality_federation_aggregate",
            2,
        );
        let text = serde_json::to_string_pretty(&output)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }
}

/// Best-effort `SystemTime::now()` → unix milliseconds. Falls back to 0
/// if the system clock is before the unix epoch (impossible in practice).
/// Exposed `pub(crate)` so peer quality/audit helpers can stamp events
/// with a consistent monotonic-ish timestamp without re-implementing the
/// clamp logic.
pub(crate) fn unix_millis_now() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}

/// Build a diff-error response that callers emit when `quality_signal_diff`
/// cannot compute a clean delta for a specific field (e.g. parser fault on
/// `prev_root` or `curr_root`). Includes a `timestamp_ms` stamp via
/// `unix_millis_now` so downstream consumers (telemetry, replay) can order
/// events without relying on JSONL line order. Marked `pub(crate)` so the
/// `quality_signal_diff` tool body can call it on every parse failure path.
pub(crate) fn make_diff_error(
    prev_root: &std::path::Path,
    curr_root: &std::path::Path,
    failed_field: &str,
    error: &str,
) -> Result<CallToolResult, McpError> {
    let output = serde_json::json!({
        "previous_root": prev_root.display().to_string(),
        "current_root": curr_root.display().to_string(),
        "failed_field": failed_field,
        "error": error,
        "diff": null,
        "timestamp_ms": unix_millis_now(),
    });
    let text = serde_json::to_string_pretty(&output)
        .map_err(|e| McpError::internal_error(e.to_string(), None))?;
    Ok(CallToolResult::success(vec![Content::text(text)]))
}

#[cfg(test)]
mod quality_signal_helper_tests {
    use super::*;

    #[test]
    fn unix_millis_now_is_positive() {
        let t = unix_millis_now();
        assert!(
            t > 1_700_000_000_000,
            "expected a post-2023 timestamp, got {t}"
        );
    }

    #[test]
    fn unix_millis_now_is_monotonic_within_test() {
        let a = unix_millis_now();
        std::thread::sleep(std::time::Duration::from_millis(2));
        let b = unix_millis_now();
        assert!(b >= a, "{b} must be >= {a}");
    }

    #[test]
    fn make_diff_error_serializes_all_fields() {
        let prev = std::path::Path::new("/prev");
        let curr = std::path::Path::new("/curr");
        let result = make_diff_error(prev, curr, "complexity", "parse error");
        assert!(
            result.is_ok(),
            "make_diff_error should always serialize successfully"
        );
    }
}
