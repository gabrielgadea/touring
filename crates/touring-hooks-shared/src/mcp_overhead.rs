//! MCP overhead self-report telemetry.
//!
//! Estimates per-tool token cost for Claude Code MCP tools using a simple
//! `string_len / 4` approximation (no new dependencies required).
//!
//! ## Token Estimation Model
//!
//! Input and output token counts are approximated as `input.len() / 4`
//! (a rough approximation suitable for relative ranking, not billing-grade accuracy).
//! This is deliberately cheap — no tiktoken or similar dependency.

use std::collections::HashMap;
use std::sync::{OnceLock, RwLock};

/// Estimated per-tool cost breakdown.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct McpOverheadReport {
    /// All tools with at least one call, sorted by total_tokens descending.
    pub tools: Vec<ToolCost>,
    /// Total tokens across all tracked tools.
    pub total_tokens: u64,
}

/// Per-tool cost summary.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ToolCost {
    /// Tool name (e.g. `"mcp__touring__index_find"`).
    pub tool_name: String,
    /// Estimated input tokens (len / 4).
    pub token_estimate: u64,
    /// Number of times this tool was called.
    pub call_count: u64,
    /// Total tokens (token_estimate * call_count).
    pub total_tokens: u64,
}

/// Global per-tool call counters — updated by hook runtime.
///
/// Uses `RwLock` for interior mutability.  The MCP dispatch path records
/// invocations exclusively; reads are taken via a read guard in each
/// query function.  Tests can call `reset_counters_for_test()` safely.
static MCP_TOOL_COUNTERS: OnceLock<RwLock<HashMap<String, ToolCost>>> = OnceLock::new();

fn get_counters() -> &'static RwLock<HashMap<String, ToolCost>> {
    MCP_TOOL_COUNTERS.get_or_init(|| RwLock::new(HashMap::new()))
}

/// Reset the global counters to empty (test use only).
pub fn reset_counters_for_test() {
    let cell = get_counters();
    if let Ok(mut guard) = cell.write() {
        guard.clear();
    }
}

/// Record a single MCP tool invocation with the given input payload size.
pub fn record_tool_invocation(tool_name: &str, input_len: usize) {
    let cell = get_counters();
    let mut counters = cell.write().expect("MCP counters poisoned");
    let entry = counters.entry(tool_name.to_string()).or_default();
    if entry.call_count == 0 {
        entry.tool_name = tool_name.to_string();
    }
    entry.call_count += 1;
    if entry.call_count == 1 {
        entry.token_estimate = (input_len / 4) as u64;
    }
    entry.total_tokens += (input_len / 4) as u64;
}

/// Snapshot the current MCP overhead state as a JSON string.
pub fn snapshot_json() -> String {
    let cell = get_counters();
    let counters = cell.read().expect("MCP counters poisoned");
    let report = snapshot_from_map(&counters);
    serde_json::to_string(&report).expect("MCP overhead report serialisation failed")
}

/// Build a `McpOverheadReport` from a reference to the counters map.
fn snapshot_from_map(counters: &HashMap<String, ToolCost>) -> McpOverheadReport {
    let mut tools: Vec<ToolCost> = counters.values().cloned().collect();
    tools.sort_by_key(|b| std::cmp::Reverse(b.total_tokens));
    let total_tokens: u64 = tools.iter().map(|t| t.total_tokens).sum();
    McpOverheadReport {
        tools,
        total_tokens,
    }
}

/// Return the top-N costliest tools from the current snapshot.
pub fn top_n_tools_json(top_n: Option<usize>) -> String {
    let cell = get_counters();
    let counters = cell.read().expect("MCP counters poisoned");
    let mut tools: Vec<ToolCost> = counters.values().cloned().collect();
    tools.sort_by_key(|b| std::cmp::Reverse(b.total_tokens));
    if let Some(n) = top_n {
        tools.truncate(n);
    }
    let total_tokens: u64 = tools.iter().map(|t| t.total_tokens).sum();
    let report = McpOverheadReport {
        tools,
        total_tokens,
    };
    serde_json::to_string(&report).expect("MCP overhead report serialisation failed")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    #[test]
    #[serial]
    fn test_record_and_snapshot() {
        let cell = get_counters();
        {
            let mut guard = cell.write().expect("MCP counters poisoned");
            guard.clear();
        }
        record_tool_invocation("mcp__touring__index_find", 1000);
        record_tool_invocation("mcp__touring__index_find", 4000);
        record_tool_invocation("mcp__touring__ast_blast", 800);

        let json = snapshot_json();
        let report: McpOverheadReport = serde_json::from_str(&json).expect("valid JSON");
        assert_eq!(report.tools.len(), 2);

        let index_find = report
            .tools
            .iter()
            .find(|t| t.tool_name == "mcp__touring__index_find")
            .expect("index_find should be present");
        assert_eq!(index_find.call_count, 2);
        assert_eq!(index_find.total_tokens, 1250);

        let ast_blast = report
            .tools
            .iter()
            .find(|t| t.tool_name == "mcp__touring__ast_blast")
            .expect("ast_blast should be present");
        assert_eq!(ast_blast.call_count, 1);
        assert_eq!(ast_blast.total_tokens, 200);
        assert_eq!(report.total_tokens, 1450);
    }

    #[test]
    #[serial]
    fn test_top_n_limit() {
        let cell = get_counters();
        {
            let mut guard = cell.write().expect("MCP counters poisoned");
            guard.clear();
        }
        record_tool_invocation("tool_a", 4000);
        record_tool_invocation("tool_b", 8000);
        record_tool_invocation("tool_c", 400);

        let json = top_n_tools_json(Some(2));
        let report: McpOverheadReport = serde_json::from_str(&json).expect("valid JSON");
        assert_eq!(report.tools.len(), 2);
        assert_eq!(report.tools[0].tool_name, "tool_b");
        assert_eq!(report.tools[1].tool_name, "tool_a");
    }

    #[test]
    #[serial]
    fn test_empty_snapshot() {
        let cell = get_counters();
        {
            let mut guard = cell.write().expect("MCP counters poisoned");
            guard.clear();
        }
        let json = snapshot_json();
        let report: McpOverheadReport = serde_json::from_str(&json).expect("valid JSON");
        assert!(report.tools.is_empty());
        assert_eq!(report.total_tokens, 0);
    }
}
