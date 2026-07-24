//! Call graph enrichment for pre_edit blast radius analysis.
//!
//! Uses touring-ast's `CallGraph` to enrich blast radius warnings with
//! caller/callee information. This helps Claude understand the impact
//! of editing a symbol: who calls it, and what it calls.

use touring_code::ast::Lang;
use touring_code::ast::call_graph::{CallGraph, build_call_graph};

/// Enriched blast radius information with call graph context.
#[derive(Debug, Clone)]
pub struct EnrichedBlastInfo {
    /// Symbols that call the target (callers).
    pub callers: Vec<String>,
    /// Symbols that the target calls (callees).
    pub callees: Vec<String>,
    /// Total transitive impact count.
    pub transitive_impact: usize,
    /// Whether the symbol is a hotspot (>5 callers).
    pub is_hotspot: bool,
}

/// Build call graph context for a file being edited.
///
/// # Arguments
///
/// * `source` - File source code.
/// * `language` - Language string (e.g. "rust", "python").
/// * `target_symbol` - Symbol being edited (if known).
///
/// # Returns
///
/// `Some(EnrichedBlastInfo)` if call graph has relevant data, `None` otherwise.
pub fn enrich_with_callgraph(
    source: &str,
    language: &str,
    target_symbol: Option<&str>,
) -> Option<EnrichedBlastInfo> {
    let lang = language.parse::<Lang>().ok()?;
    let graph: CallGraph = build_call_graph(source, lang);
    let target = target_symbol?;

    let callers: Vec<String> = graph
        .callers_of(target)
        .iter()
        .map(|s| s.caller.clone())
        .collect();

    let callees: Vec<String> = graph
        .callees_of(target)
        .iter()
        .map(|s| s.callee.clone())
        .collect();

    if callers.is_empty() && callees.is_empty() {
        return None;
    }

    let is_hotspot = callers.len() > 5;
    let transitive_impact = callers.len() + callees.len();

    Some(EnrichedBlastInfo {
        callers,
        callees,
        transitive_impact,
        is_hotspot,
    })
}

/// Format enriched blast info as a context string for injection.
pub fn format_callgraph_context(info: &EnrichedBlastInfo, symbol: &str) -> String {
    let mut parts = Vec::new();

    if info.is_hotspot {
        parts.push(format!(
            "HOTSPOT: `{symbol}` has {} callers",
            info.callers.len()
        ));
    }

    if !info.callers.is_empty() {
        let caller_list: String = info
            .callers
            .iter()
            .take(5)
            .cloned()
            .collect::<Vec<_>>()
            .join(", ");
        let suffix = if info.callers.len() > 5 {
            format!(" (+{} more)", info.callers.len() - 5)
        } else {
            String::new()
        };
        parts.push(format!("callers: [{caller_list}{suffix}]"));
    }

    if !info.callees.is_empty() {
        let callee_list: String = info
            .callees
            .iter()
            .take(5)
            .cloned()
            .collect::<Vec<_>>()
            .join(", ");
        parts.push(format!("calls: [{callee_list}]"));
    }

    parts.join(" | ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_enrich_python_code() {
        let source = r#"
def helper():
    pass

def main():
    helper()
    helper()
"#;
        let info = enrich_with_callgraph(source, "python", Some("helper"));
        assert!(info.is_some());
        let info = info.unwrap();
        assert!(info.callers.contains(&"main".to_string()));
    }

    #[test]
    fn test_enrich_unknown_symbol() {
        let source = "def foo(): pass";
        let info = enrich_with_callgraph(source, "python", Some("nonexistent"));
        // Should return None since no callers/callees
        assert!(info.is_none());
    }

    #[test]
    fn test_enrich_no_target() {
        let source = "def foo(): pass";
        let info = enrich_with_callgraph(source, "python", None);
        assert!(info.is_none());
    }

    #[test]
    fn test_format_hotspot() {
        let info = EnrichedBlastInfo {
            callers: (0..7).map(|i| format!("caller_{i}")).collect(),
            callees: vec!["helper".to_string()],
            transitive_impact: 8,
            is_hotspot: true,
        };
        let ctx = format_callgraph_context(&info, "process");
        assert!(ctx.contains("HOTSPOT"));
        assert!(ctx.contains("7 callers"));
        assert!(ctx.contains("+2 more"));
    }

    #[test]
    fn test_format_simple() {
        let info = EnrichedBlastInfo {
            callers: vec!["main".to_string()],
            callees: vec!["helper".to_string(), "validate".to_string()],
            transitive_impact: 3,
            is_hotspot: false,
        };
        let ctx = format_callgraph_context(&info, "run");
        assert!(!ctx.contains("HOTSPOT"));
        assert!(ctx.contains("callers: [main]"));
        assert!(ctx.contains("calls: [helper, validate]"));
    }

    #[test]
    fn test_format_empty() {
        let info = EnrichedBlastInfo {
            callers: vec![],
            callees: vec![],
            transitive_impact: 0,
            is_hotspot: false,
        };
        let ctx = format_callgraph_context(&info, "unused");
        assert!(ctx.is_empty());
    }
}
