//! Wiring modules model — parsed from `touring wiring modules --full -j`.
//!
//! This model was an orphan for a reason worth recording: it could never have
//! worked. `WiringModulesReport` declared `{ "modules": [...] }`, and the command
//! emits a TOP-LEVEL ARRAY. Nothing wired to it because nothing could; the
//! desktop's graph viewer carried its own private copy with the right shape,
//! and the two drifted apart unmeasured (16/09/2026).
//!
//! It is now the single model for both surfaces, shaped by the command's actual
//! output, measured: 1437 entries of
//! `{file_path, integration_score, total_pub_symbols, orphan_count, orphan_symbols}`.
//!
//! `--full` is part of the contract. The `wiring`/`viz`/`graph` family defaults
//! to `--brief`, which elides large arrays to `{"_elided_array_len": N}` so an
//! LLM's context survives; a programmatic consumer that omits the flag parses an
//! object where it expects a list. [`WiringModulesReport::parse`] says so in the
//! error instead of leaving the caller with a bare serde message.

use serde::{Deserialize, Serialize};

/// A crate/module with its wiring integration score.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WiringModule {
    /// Full file path to the module (e.g. "crates/touring-hooks/src/lib.rs").
    #[serde(default)]
    pub file_path: String,
    /// Integration score (0.0–1.0); 1.0 means fully wired.
    #[serde(default)]
    pub integration_score: f64,
    /// Public symbols the module declares.
    #[serde(default)]
    pub total_pub_symbols: usize,
    /// Number of orphan pub symbols in this module.
    #[serde(default)]
    pub orphan_count: usize,
    /// The orphan symbols themselves, when the command reports them.
    #[serde(default)]
    pub orphan_symbols: Vec<String>,
}

/// The `touring wiring modules --full -j` payload: a top-level array of modules.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct WiringModulesReport {
    /// List of wired modules with their integration scores.
    pub modules: Vec<WiringModule>,
}

impl WiringModulesReport {
    /// Parse the command's stdout, naming the elision when that is the failure.
    ///
    /// A caller that forgot `--full` gets `{"_elided_array_len": N}`, whose serde
    /// error ("invalid type: map, expected a sequence") says nothing about the
    /// cause. This turns it into the instruction: pass `--full`.
    pub fn parse(stdout: &str) -> Result<Self, String> {
        serde_json::from_str::<Self>(stdout).map_err(|e| {
            if stdout.contains("_elided_array_len") {
                "the wiring output came back elided — pass `--full`: \
                 `touring wiring modules --full -j` (the family defaults to --brief)"
                    .to_string()
            } else {
                format!("failed to parse wiring modules JSON: {e}")
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wiring_module_serde_roundtrip() {
        let module = WiringModule {
            file_path: "crates/touring-hooks/src/lib.rs".to_string(),
            integration_score: 1.0,
            total_pub_symbols: 3,
            orphan_count: 0,
            orphan_symbols: Vec::new(),
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

    #[test]
    fn the_report_parses_the_command_s_real_shape_a_top_level_array() {
        // Verbatim from `touring wiring modules --full -j` (16/09/2026).
        let raw = r#"[{"file_path":"crates/inferlets/src/composite_health_trend.rs",
                       "integration_score":1.0,"total_pub_symbols":3,
                       "orphan_count":0,"orphan_symbols":[]}]"#;
        let report = WiringModulesReport::parse(raw).expect("real payload must parse");
        assert_eq!(report.modules.len(), 1);
        assert_eq!(report.modules[0].total_pub_symbols, 3);
        assert_eq!(
            report.modules[0].file_path,
            "crates/inferlets/src/composite_health_trend.rs"
        );
    }

    #[test]
    fn the_old_object_shape_is_not_what_the_command_emits() {
        // NEGATIVE CONTROL pinning the defect this model carried: the previous
        // `{ "modules": [...] }` shape parses NOTHING the command produces.
        let old_shape = r#"{"modules":[{"file_path":"a.rs","integration_score":1.0}]}"#;
        assert!(
            WiringModulesReport::parse(old_shape).is_err(),
            "the command never emitted an object wrapper; accepting it would hide the drift"
        );
    }

    #[test]
    fn an_elided_payload_is_reported_as_the_missing_full_flag() {
        let elided = r#"{"_elided_array_len": 1437}"#;
        let err = WiringModulesReport::parse(elided).expect_err("elided input must fail");
        assert!(err.contains("--full"), "the error must teach the fix: {err}");
    }
}
