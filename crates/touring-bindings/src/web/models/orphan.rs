//! Orphan symbol model — parsed from `touring wiring orphans` (JSON default).
//!
//! Field names mirror the CLI wire format exactly:
//! `{orphan_count, orphans: [{symbol_name, module_file, symbol_kind,
//! visibility}]}` — the previous model expected `{count, orphans: [{symbol,
//! file_path, line, symbol_type, integration_score}]}`, so EVERY field
//! deserialized to its default and the screen showed zeros (cross-audit
//! 2026-06-11, F-02). Legacy names are kept as serde aliases for tolerance.

use serde::{Deserialize, Serialize};

/// An orphan pub symbol (no consumers found).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OrphanSymbol {
    /// Symbol name (e.g. "apply_theme").
    #[serde(rename = "symbol_name", alias = "symbol", default)]
    pub symbol: String,
    /// File that defines the symbol's module.
    #[serde(rename = "module_file", alias = "file_path", default)]
    pub file_path: String,
    /// Symbol kind (e.g. "function", "struct", "method").
    #[serde(rename = "symbol_kind", alias = "symbol_type", default)]
    pub kind: Option<String>,
    /// Visibility as reported by the wiring scan (e.g. "public").
    #[serde(default)]
    pub visibility: Option<String>,
}

/// Orphan report wrapper — parsed from `touring wiring orphans`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OrphanReport {
    /// Total count of orphan symbols.
    #[serde(rename = "orphan_count", alias = "count", default)]
    pub count: usize,
    /// List of individual orphan symbol entries.
    #[serde(rename = "orphans", default)]
    pub orphans: Vec<OrphanSymbol>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_orphan_symbol_serde_roundtrip() {
        let symbol = OrphanSymbol {
            symbol: "apply_theme".to_string(),
            file_path: "crates/touring-web/src/lib.rs".to_string(),
            kind: Some("function".to_string()),
            visibility: Some("public".to_string()),
        };
        let json = serde_json::to_string(&symbol).unwrap();
        assert!(
            json.contains("symbol_name"),
            "wire format must match CLI: {json}"
        );
        let parsed: OrphanSymbol = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.symbol, symbol.symbol);
        assert_eq!(parsed.kind, symbol.kind);
    }

    /// Contract fixture — literal excerpt of real `touring wiring orphans`
    /// output (captured 2026-06-11). If the CLI wire format drifts, this is
    /// the test that must fail.
    #[test]
    fn test_parses_real_cli_wire_format() {
        let wire = r#"{
            "dead_patterns": [],
            "orphan_count": 1531,
            "orphans": [{
                "module_file": "crates/touring-hooks-shared/src/latency_marker.rs",
                "symbol_kind": "function",
                "symbol_name": "record_latency",
                "visibility": "public"
            }]
        }"#;
        let report: OrphanReport = serde_json::from_str(wire).expect("CLI wire format");
        assert_eq!(report.count, 1531);
        assert_eq!(report.orphans.len(), 1);
        assert_eq!(report.orphans[0].symbol, "record_latency");
        assert_eq!(
            report.orphans[0].file_path,
            "crates/touring-hooks-shared/src/latency_marker.rs"
        );
        assert_eq!(report.orphans[0].kind.as_deref(), Some("function"));
        assert_eq!(report.orphans[0].visibility.as_deref(), Some("public"));
    }

    #[test]
    fn test_orphan_report_empty() {
        let report = OrphanReport::default();
        assert_eq!(report.count, 0);
        assert!(report.orphans.is_empty());
    }
}
