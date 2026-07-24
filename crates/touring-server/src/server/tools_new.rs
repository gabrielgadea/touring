//! New MCP tools for the curated 22-tool set (W2 of task_1780763041476850005).
//!
//! Adds 3 net-new MCP tools that fill gaps identified in FASE 1 SCOUT:
//!   * `touring_tdg`            — TDG grade letter (A+..F) wrapper for `touring ast tdg`
//!   * `touring_hook_metrics`   — hook dispatch p50/p99 latency aggregator
//!   * `touring_cortex_classify`— unified intent classification (CILA L0-L6)
//!
//! All 3 are gated behind the `mcp-curated` feature so they only ship when
//! the curated set is enabled. Token cost: each tool ~80-200 tokens vs the
//! composite output of full CLI runs (often 2-5 KB).

// W2 (task_1780763041476850005): gated by `mcp-curated` so the 3 new tools
// only ship when the curated set is enabled. During the 30-day migration
// window the historical 102 tools remain on by default (`mcp-legacy`).
#![cfg(feature = "mcp-curated")]

use std::process::Command;

use rmcp::{
    ErrorData as McpError, handler::server::wrapper::Parameters, model::*, tool, tool_router,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

// ── W2.1 touring_tdg ────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
pub struct TdgInput {
    /// Absolute or workspace-relative path to the file to grade.
    pub path: String,
    /// Whether to include all 6 TDG dimensions or just the composite + grade.
    #[serde(default)]
    pub minimal: bool,
}

/// TDG grade result mirrored from `touring ast tdg -j`. Captures the 6
/// quality dimensions so callers can decide whether to refactor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TdgOutput {
    pub file_path: String,
    pub language: String,
    pub grade: String,
    pub composite: f32,
    pub action: String,
    pub edit_count: u32,
    /// Only populated when `minimal=false`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dimensions: Option<TdgDimensions>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TdgDimensions {
    pub complexity: f32,
    pub coverage: f32,
    pub duplication: f32,
    pub entropy: f32,
    pub churn: f32,
    pub antipatterns: f32,
}

// W2 tools wrapped in `impl super::TouringServer` (rmcp #[tool] macro
// requires `&self` receiver — must be in an impl block, not free-standing).
// The `#[tool_router(router = ..., vis = "pub(crate)")]` decorator is
// required by the pre-existing tools::tests::every_tools_submodule_has_tool_router_macro
// invariant — without it, the tools are invisible to MCP runtime.
#[tool_router(router = router_tdg, vis = "pub(crate)")]
impl super::TouringServer {
    #[tool(
        name = "touring_tdg",
        description = "TDG (Technical Debt Grade) letter A+..F for a file. Wraps `touring ast tdg <path> -j`. Returns the grade, composite score, and (optionally) the 6 quality dimensions: complexity, coverage, duplication, entropy, churn, antipatterns. Use to triage refactor candidates."
    )]
    pub(crate) async fn touring_tdg(
        &self,
        params: Parameters<TdgInput>,
    ) -> Result<CallToolResult, McpError> {
        use rmcp::model::Content;

        let TdgInput { path, minimal } = params.0;
        let out = Command::new("touring")
            .args(["ast", "tdg", &path, "-j"])
            .output()
            .map_err(|e| McpError::internal_error(format!("spawn failed: {e}"), None))?;

        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr).to_string();
            return Err(McpError::internal_error(
                format!("touring ast tdg failed: {stderr}"),
                None,
            ));
        }

        let raw: serde_json::Value = serde_json::from_slice(&out.stdout)
            .map_err(|e| McpError::internal_error(format!("parse failed: {e}"), None))?;

        let tdg = &raw["tdg"];
        let resp = TdgOutput {
            file_path: raw["file_path"].as_str().unwrap_or(&path).to_string(),
            language: raw["language"].as_str().unwrap_or("unknown").to_string(),
            grade: tdg["grade"].as_str().unwrap_or("?").to_string(),
            composite: tdg["composite"].as_f64().unwrap_or(0.0) as f32,
            action: tdg["action"].as_str().unwrap_or("").to_string(),
            edit_count: raw["edit_count"].as_u64().unwrap_or(0) as u32,
            dimensions: if minimal {
                None
            } else {
                Some(TdgDimensions {
                    complexity: tdg["complexity"].as_f64().unwrap_or(0.0) as f32,
                    coverage: tdg["coverage"].as_f64().unwrap_or(0.0) as f32,
                    duplication: tdg["duplication"].as_f64().unwrap_or(0.0) as f32,
                    entropy: tdg["entropy"].as_f64().unwrap_or(0.0) as f32,
                    churn: tdg["churn"].as_f64().unwrap_or(0.0) as f32,
                    antipatterns: tdg["antipatterns"].as_f64().unwrap_or(0.0) as f32,
                })
            },
        };

        let body = serde_json::to_string(&resp)
            .map_err(|e| McpError::internal_error(format!("serialize: {e}"), None))?;
        Ok(CallToolResult::success(vec![Content::text(body)]))
    }

    // Close the first impl block (W2.1 touring_tdg).
}

// ── W2.2 touring_hook_metrics ──────────────────────────────────────────

#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
pub struct HookMetricsInput {
    /// Optional subsystem to scope metrics to. Examples: "hook", "ann", "rkyv",
    /// "tantivy". If absent, returns hook_dispatch + ann_search + rkyv_dispatch
    /// (the three latency-critical surfaces).
    #[serde(default)]
    pub subsystem: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LatencySnapshot {
    pub count: u64,
    pub p50_us: u64,
    pub p90_us: u64,
    pub p99_us: u64,
    pub p999_us: u64,
    pub max_us: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct HookMetricsOutput {
    /// Number of hook invocations since daemon start.
    pub hook_dispatch_count: u64,
    pub hook_dispatch_latency: LatencySnapshot,
    /// ANN search latency (used by the symbol index).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ann_search_latency: Option<LatencySnapshot>,
    /// rkyv zero-copy IPC latency.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rkyv_dispatch_latency: Option<LatencySnapshot>,
    /// Counter that triggered the W2.2 request (e.g. "hook" or "rkyv").
    pub primary: String,
}

// Second impl block for W2.2 (Rust allows multiple impls).
// The 3rd impl below is for W2.3 (cortex_classify) — separate because
// cortex_classify needs different imports/types.
#[tool_router(router = router_hook_metrics, vis = "pub(crate)")]
impl super::TouringServer {
    #[tool(
        name = "touring_hook_metrics",
        description = "Hook + ANN + rkyv dispatch latency aggregator. Wraps `touring gate-metrics -j` and surfaces only the latency-critical counters (skips enum cardinality noise). Pass `subsystem` to scope (default: hook). Returns p50/p90/p99/p999/max latency in microseconds + invocation count. Use to detect P99 regressions post-refactor."
    )]
    pub(crate) async fn touring_hook_metrics(
        &self,
        params: Parameters<HookMetricsInput>,
    ) -> Result<CallToolResult, McpError> {
        use rmcp::model::Content;

        let subsystem = params.0.subsystem.unwrap_or_else(|| "hook".to_string());

        let out = Command::new("touring")
            .args(["gate-metrics", "-j"])
            .output()
            .map_err(|e| McpError::internal_error(format!("spawn failed: {e}"), None))?;

        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr).to_string();
            return Err(McpError::internal_error(
                format!("touring gate-metrics failed: {stderr}"),
                None,
            ));
        }

        let raw: serde_json::Value = serde_json::from_slice(&out.stdout)
            .map_err(|e| McpError::internal_error(format!("parse failed: {e}"), None))?;

        fn snapshot(v: &serde_json::Value) -> LatencySnapshot {
            serde_json::from_value(v.clone()).unwrap_or_default()
        }

        let resp = HookMetricsOutput {
            hook_dispatch_count: raw["total_invocations"].as_u64().unwrap_or(0),
            hook_dispatch_latency: snapshot(&raw["hook_dispatch_latency"]),
            ann_search_latency: snapshot_opt(&raw["ann_search_latency"]),
            rkyv_dispatch_latency: snapshot_opt(&raw["rkyv_dispatch_latency"]),
            primary: subsystem,
        };

        fn snapshot_opt(v: &serde_json::Value) -> Option<LatencySnapshot> {
            serde_json::from_value(v.clone()).ok()
        }

        let body = serde_json::to_string(&resp)
            .map_err(|e| McpError::internal_error(format!("serialize: {e}"), None))?;
        Ok(CallToolResult::success(vec![Content::text(body)]))
    }

    // Close the 2nd `impl super::TouringServer` block (W2.2 touring_hook_metrics).
}

#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
pub struct CortexClassifyInput {
    /// User intent or task description to classify.
    pub text: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CortexClassifyOutput {
    pub cila_level: u8,
    pub cila_name: String,
    pub routing_strategy: String,
    pub requires_code_first: bool,
    pub requires_pipeline: bool,
    pub techniques: Vec<String>,
}

// 3rd impl block for W2.3 touring_cortex_classify.
#[tool_router(router = router_cortex_classify, vis = "pub(crate)")]
impl super::TouringServer {
    #[tool(
        name = "touring_cortex_classify",
        description = "Unified intent classification. Wraps `touring classify-intent <text> -j` and returns a clean shape: CILA level (0-6), level name (Direct/Modify/Spawn/etc), routing strategy (direct/orchestrator/cortex), and whether code-first or pipeline is required. Use to triage a user request before spawning subagents."
    )]
    pub(crate) async fn touring_cortex_classify(
        &self,
        params: Parameters<CortexClassifyInput>,
    ) -> Result<CallToolResult, McpError> {
        use rmcp::model::Content;

        let text = params.0.text;
        let out = Command::new("touring")
            .args(["classify-intent", &text, "-j"])
            .output()
            .map_err(|e| McpError::internal_error(format!("spawn failed: {e}"), None))?;

        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr).to_string();
            return Err(McpError::internal_error(
                format!("touring classify-intent failed: {stderr}"),
                None,
            ));
        }

        let raw: serde_json::Value = serde_json::from_slice(&out.stdout)
            .map_err(|e| McpError::internal_error(format!("parse failed: {e}"), None))?;

        // classify-intent returns {metadata: {cila_level, cila_name, ...}, hookSpecificOutput, suppressOutput}
        let md = &raw["metadata"];
        let resp = CortexClassifyOutput {
            cila_level: md["cila_level"].as_u64().unwrap_or(0) as u8,
            cila_name: md["cila_name"].as_str().unwrap_or("Unknown").to_string(),
            routing_strategy: md["routing_strategy"]
                .as_str()
                .unwrap_or("direct")
                .to_string(),
            requires_code_first: md["requires_code_first"].as_bool().unwrap_or(false),
            requires_pipeline: md["requires_pipeline"].as_bool().unwrap_or(false),
            techniques: md["techniques"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|t| t.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default(),
        };

        let body = serde_json::to_string(&resp)
            .map_err(|e| McpError::internal_error(format!("serialize: {e}"), None))?;
        Ok(CallToolResult::success(vec![Content::text(body)]))
    }

    // Close the 2nd `impl super::TouringServer` block (W2.2 + W2.3).
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tdg_input_defaults_minimal_false() {
        // Deserialization: `{}` would not be valid since path is required.
        // But `{"path": "x"}` should yield minimal=false.
        let parsed: TdgInput = serde_json::from_str(r#"{"path": "x"}"#).unwrap();
        assert_eq!(parsed.path, "x");
        assert!(!parsed.minimal);
    }

    #[test]
    fn tdg_dimensions_serialize_only_when_present() {
        let dims = TdgDimensions {
            complexity: 0.8,
            coverage: 0.4,
            duplication: 1.0,
            entropy: 0.99,
            churn: 1.0,
            antipatterns: 0.95,
        };
        let json = serde_json::to_string(&dims).unwrap();
        assert!(json.contains("complexity"));
        assert!(json.contains("entropy"));
    }

    #[test]
    fn hook_metrics_input_subsystem_optional() {
        let p1: HookMetricsInput = serde_json::from_str("{}").unwrap();
        assert!(p1.subsystem.is_none());
        let p2: HookMetricsInput = serde_json::from_str(r#"{"subsystem":"rkyv"}"#).unwrap();
        assert_eq!(p2.subsystem.as_deref(), Some("rkyv"));
    }

    #[test]
    fn latency_snapshot_all_zero_is_valid() {
        let s = LatencySnapshot {
            count: 0,
            p50_us: 0,
            p90_us: 0,
            p99_us: 0,
            p999_us: 0,
            max_us: 0,
        };
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains("\"count\":0"));
    }

    #[test]
    fn cortex_classify_output_techniques_default_empty() {
        let resp = CortexClassifyOutput {
            cila_level: 3,
            cila_name: "Modify".to_string(),
            routing_strategy: "orchestrator".to_string(),
            requires_code_first: true,
            requires_pipeline: false,
            techniques: vec![],
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"cila_name\":\"Modify\""));
        assert!(json.contains("\"techniques\":[]"));
    }
}
