//! Wiring modules model — parsed from `touring wiring modules -j`.

use serde::{Deserialize, Serialize};

/// A crate/module with its wiring integration score.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WiringModule {
    /// Full file path to the module (e.g. "crates/touring-hooks/src/lib.rs").
    #[serde(rename = "file_path", default)]
    pub file_path: String,
    /// Integration score (0.0–1.0); 1.0 means fully wired.
    #[serde(rename = "integration_score", default)]
    pub integration_score: f64,
    /// Number of orphan pub symbols in this module.
    #[serde(rename = "orphan_count", default)]
    pub orphan_count: usize,
}

/// Wiring modules report wrapper — parsed from `touring wiring modules -j`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WiringModulesReport {
    /// List of wired modules with their integration scores.
    #[serde(default)]
    pub modules: Vec<WiringModule>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wiring_module_serde_roundtrip() {
        let module = WiringModule {
            file_path: "crates/touring-hooks/src/lib.rs".to_string(),
            integration_score: 1.0,
            orphan_count: 0,
        };
        let json = serde_json::to_string(&module).unwrap();
        let parsed: WiringModule = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.integration_score, module.integration_score);
        assert_eq!(parsed.orphan_count, module.orphan_count);
    }

    #[test]
    fn test_wiring_modules_report_default() {
        let report = WiringModulesReport::default();
        assert!(report.modules.is_empty());
    }
}
