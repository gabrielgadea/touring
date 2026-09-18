//! Unused pub symbols inferlet.
//!
//! Identifies orphan pub symbols (exported but not consumed by any other crate)
//! using the touring wiring subsystem. Filters by optional threshold and module.
//!
//! # Input JSON
//!
//! ```json
//! {
//!   "__inferlet__": "unused_pub_symbols",
//!   "threshold": 0.0,
//!   "module_filter": ["touring-hooks", "touring-ast"]
//! }
//! ```
//!
//! # Output JSON
//!
//! ```json
//! {
//!   "orphans": [
//!     {"symbol": "Foo", "file": "src/lib.rs:42", "fan_out": 0}
//!   ],
//!   "count": 1
//! }
//! ```

use serde::{Deserialize, Serialize};

// Thread-local error buffer for error propagation. (Doc comment kept as
// inner-line because thread_local! macro does not attach `///` to its
// generated `static`.)
thread_local! {
    static LAST_ERROR: RefCell<Option<String>> = const { RefCell::new(None) };
}

use std::cell::RefCell;

/// Input structure for unused_pub_symbols inferlet.
#[derive(Debug, Deserialize)]
pub struct Input {
    /// Minimum fan_out threshold — symbols with fan_out <= threshold are included.
    /// Default 0.0 means include all orphans regardless of fan_out.
    #[serde(default)]
    pub threshold: Option<f32>,
    /// Optional list of module prefixes to filter results.
    #[serde(default)]
    pub module_filter: Option<Vec<String>>,
}

/// Orphan symbol entry in the result.
#[derive(Debug, Serialize)]
pub struct Orphan {
    /// Name of the orphan pub symbol.
    pub symbol: String,
    /// Source location of the symbol as `path:line`.
    pub file: String,
    /// Number of consumers of the symbol (0 means a true orphan).
    pub fan_out: i32,
}

/// Output structure for unused_pub_symbols inferlet.
#[derive(Debug, Serialize)]
pub(crate) struct Output {
    /// The orphan symbols that passed the threshold and module filters.
    pub orphans: Vec<Orphan>,
    /// Number of orphan symbols reported.
    pub count: usize,
}

/// Parse the JSON input and extract threshold + module_filter.
///
/// Input that is not JSON at all is an error, never the defaults: defaulting ran the
/// query against the live project and reported its real orphans for garbage input.
/// Valid JSON whose fields have the wrong shape still falls back per field.
fn parse_input(input: &str) -> Result<(f32, Option<Vec<String>>), String> {
    let input = input.trim();
    let raw = serde_json::from_str::<serde_json::Value>(input)
        .map_err(|e| format!("invalid JSON input: {e}"))?;
    let typed = serde_json::from_value::<Input>(raw.clone()).ok();

    let threshold = match &typed {
        Some(inp) => inp.threshold.unwrap_or(0.0),
        None => raw
            .get("threshold")
            .and_then(|v| v.as_f64())
            .map(|v| v as f32)
            .unwrap_or(0.0),
    };
    let module_filter = typed.and_then(|i| i.module_filter);

    Ok((threshold, module_filter))
}

/// Run `touring wiring orphans -j` and parse the output.
fn get_wiring_orphans() -> Result<serde_json::Value, String> {
    let output = std::process::Command::new("touring")
        // `--full`: the brief default elides the orphan array to a count, and this
        // consumer iterates it — measured 16/09/2026, it was reading zero orphans.
        .args(["wiring", "orphans", "--full", "-j"])
        .output()
        .map_err(|e| format!("failed to spawn touring: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("touring wiring orphans failed: {}", stderr));
    }

    let json_str = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str(&json_str)
        .map_err(|e| format!("failed to parse wiring orphans JSON: {}", e))
}

/// Filter orphans by threshold and module_filter.
fn filter_orphans(
    orphans: &[serde_json::Value],
    threshold: f32,
    module_filter: &Option<Vec<String>>,
) -> Vec<Orphan> {
    orphans
        .iter()
        .filter_map(|o| {
            let fan_out = o.get("fan_out").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
            if fan_out as f32 > threshold {
                return None;
            }
            let file = o
                .get("file")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            let symbol = o
                .get("symbol")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();

            // module_filter: check if any prefix matches
            if let Some(filters) = module_filter
                && !filters.iter().any(|f| file.starts_with(f))
            {
                return None;
            }

            Some(Orphan {
                symbol,
                file,
                fan_out,
            })
        })
        .collect()
}

/// Raw evaluate — returns 1 if any orphan symbols match the criteria, 0 otherwise.
pub(crate) fn evaluate_raw(input: &str) -> i32 {
    let (threshold, module_filter) = match parse_input(input) {
        Ok(parsed) => parsed,
        Err(e) => {
            LAST_ERROR.with(|cell| *cell.borrow_mut() = Some(e));
            return 0;
        }
    };

    let wiring_result = match get_wiring_orphans() {
        Ok(v) => v,
        Err(e) => {
            LAST_ERROR.with(|cell| *cell.borrow_mut() = Some(e));
            return 0;
        }
    };

    let orphans = wiring_result
        .get("orphans")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let filtered = filter_orphans(&orphans, threshold, &module_filter);
    let count = filtered.len();

    if count > 0 {
        // Store result for potential inspection (beyond return code)
        let output = Output {
            orphans: filtered,
            count,
        };
        if let Ok(json) = serde_json::to_string(&output) {
            LAST_ERROR.with(|cell| *cell.borrow_mut() = Some(json));
        }
        1
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn test_parse_input_default_threshold() {
        let input = r#"{}"#;
        let (threshold, module_filter) = parse_input(input).unwrap();
        assert_eq!(threshold, 0.0);
        assert!(module_filter.is_none());
    }

    #[test]
    fn test_parse_input_with_threshold() {
        let input = r#"{"threshold": 2.0}"#;
        let (threshold, module_filter) = parse_input(input).unwrap();
        assert_eq!(threshold, 2.0);
        assert!(module_filter.is_none());
    }

    #[test]
    fn test_parse_input_rejects_malformed_json() {
        let err = parse_input("{").unwrap_err();
        assert!(err.starts_with("invalid JSON input"), "{err}");
    }

    #[test]
    fn test_parse_input_wrong_field_type_falls_back_per_field() {
        // `module_filter` of the wrong type fails the typed parse; `threshold` survives.
        let input = r#"{"threshold": 3.0, "module_filter": "touring-hooks"}"#;
        let (threshold, module_filter) = parse_input(input).unwrap();
        assert_eq!(threshold, 3.0);
        assert!(module_filter.is_none());
    }

    #[test]
    fn test_parse_input_with_module_filter() {
        let input = r#"{"module_filter": ["touring-hooks", "touring-ast"]}"#;
        let (threshold, module_filter) = parse_input(input).unwrap();
        assert_eq!(threshold, 0.0);
        assert!(module_filter.is_some());
        let filters = module_filter.unwrap();
        assert_eq!(filters.len(), 2);
    }

    #[test]
    fn test_filter_orphans_threshold() {
        let orphans: Vec<serde_json::Value> = vec![
            serde_json::json!({"symbol": "Foo", "file": "src/lib.rs:42", "fan_out": 0}),
            serde_json::json!({"symbol": "Bar", "file": "src/lib.rs:43", "fan_out": 3}),
            serde_json::json!({"symbol": "Baz", "file": "src/lib.rs:44", "fan_out": 1}),
        ];

        // threshold = 2.0 should include Foo (0), Baz (1) but NOT Bar (3)
        let filtered = filter_orphans(&orphans, 2.0, &None);
        assert_eq!(filtered.len(), 2);
        assert_eq!(filtered[0].symbol, "Foo");
        assert_eq!(filtered[1].symbol, "Baz");
    }

    #[test]
    fn test_filter_orphans_module_filter() {
        let orphans: Vec<serde_json::Value> = vec![
            serde_json::json!({"symbol": "Foo", "file": "touring-hooks/src/lib.rs:42", "fan_out": 0}),
            serde_json::json!({"symbol": "Bar", "file": "touring-ast/src/lib.rs:43", "fan_out": 0}),
            serde_json::json!({"symbol": "Baz", "file": "some-other/src/lib.rs:44", "fan_out": 0}),
        ];

        let filters = Some(vec!["touring-hooks".to_string()]);
        let filtered = filter_orphans(&orphans, 0.0, &filters);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].symbol, "Foo");
    }

    #[test]
    fn test_evaluate_raw_malformed_input_returns_zero() {
        // Malformed JSON is rejected before `touring` is spawned: the error buffer holds
        // the parse error, not the orphan list a live query would have written there.
        let result = evaluate_raw("{");
        assert_eq!(result, 0);
        let err = LAST_ERROR.with(|cell| cell.borrow().clone()).unwrap();
        assert!(err.starts_with("invalid JSON input"), "{err}");
    }
}
