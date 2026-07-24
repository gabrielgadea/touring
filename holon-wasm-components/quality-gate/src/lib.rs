//! THSF Fase 4 — `quality-gate` capability component.
//!
//! Scans a source string for common anti-patterns and returns a
//! composite quality score in `[0, 1]`. Deliberately simple — regex-
//! free, substring-based — so it is deterministic, language-agnostic
//! (mostly Rust-focused for now), and fits under ~150 KB WASM.
//!
//! Input JSON::
//!
//!     {
//!       "source": "fn foo() -> i32 { x.unwrap() }",
//!       "lang": "rust"
//!     }
//!
//! Output JSON::
//!
//!     {
//!       "score": 0.83,
//!       "lang": "rust",
//!       "antipatterns": [
//!         {"kind": "unwrap", "count": 1},
//!         {"kind": "panic",  "count": 0},
//!         ...
//!       ],
//!       "lines": 1
//!     }

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Bindings
// ---------------------------------------------------------------------------

wit_bindgen::generate!({
    path: "../../crates/touring-wasm/wit/holon-core.wit",
    world: "holon-component",
});

use exports::holon::core::capabilities::{Guest, InvokeError, InvokeRequest, InvokeResponse};

const CAPABILITY: &str = "quality-gate";

// ---------------------------------------------------------------------------
// Anti-pattern registry (Rust-focused)
// ---------------------------------------------------------------------------

/// `(kind, needle)` pairs. Each needle is a substring that, when present
/// in the source, contributes one point to the antipattern count.
/// The order establishes the reporting order; counts are independent.
const RUST_PATTERNS: &[(&str, &str)] = &[
    ("unwrap", ".unwrap()"),
    ("expect", ".expect("),
    ("panic", "panic!("),
    ("todo", "todo!("),
    ("unimplemented", "unimplemented!("),
    ("unreachable", "unreachable!("),
];

const PYTHON_PATTERNS: &[(&str, &str)] = &[
    ("bare_except", "except:"),
    ("print_debug", "print("),
    ("todo", "# TODO"),
    ("fixme", "# FIXME"),
];

// ---------------------------------------------------------------------------
// Wire types
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct QualityInput {
    source: String,
    #[serde(default = "default_lang")]
    lang: String,
}

fn default_lang() -> String {
    "rust".to_string()
}

#[derive(Serialize)]
struct Antipattern {
    kind: &'static str,
    count: usize,
}

#[derive(Serialize)]
struct QualityOutput<'a> {
    score: f32,
    lang: &'a str,
    antipatterns: Vec<Antipattern>,
    lines: usize,
    total_antipatterns: usize,
}

// ---------------------------------------------------------------------------
// Core logic
// ---------------------------------------------------------------------------

fn patterns_for(lang: &str) -> &'static [(&'static str, &'static str)] {
    match lang {
        "python" => PYTHON_PATTERNS,
        // Fall back to Rust patterns — matches our most-used surface.
        _ => RUST_PATTERNS,
    }
}

fn count_occurrences(haystack: &str, needle: &str) -> usize {
    if needle.is_empty() {
        return 0;
    }
    let mut count = 0usize;
    let mut start = 0usize;
    while start < haystack.len() {
        match haystack[start..].find(needle) {
            Some(offset) => {
                count += 1;
                start += offset + needle.len();
            }
            None => break,
        }
    }
    count
}

/// Score function: 1.0 when source has zero antipatterns, approaches 0
/// as the antipattern density climbs. Formula: `1 / (1 + density*8)`.
/// Density = antipatterns per 100 lines.
fn score(total_antipatterns: usize, lines: usize) -> f32 {
    let lines_f = lines.max(1) as f32;
    let density_per_100 = (total_antipatterns as f32) * 100.0 / lines_f;
    1.0 / (1.0 + density_per_100 * 0.08)
}

fn analyse(input: &QualityInput) -> QualityOutput<'_> {
    let patterns = patterns_for(&input.lang);
    let mut counts: Vec<Antipattern> = Vec::with_capacity(patterns.len());
    let mut total = 0usize;
    for (kind, needle) in patterns {
        let c = count_occurrences(&input.source, needle);
        total += c;
        counts.push(Antipattern { kind, count: c });
    }

    let lines = input.source.lines().count();
    let s = score(total, lines);

    QualityOutput {
        score: s,
        lang: &input.lang,
        antipatterns: counts,
        lines,
        total_antipatterns: total,
    }
}

// ---------------------------------------------------------------------------
// Guest implementation
// ---------------------------------------------------------------------------

struct Component;

impl Guest for Component {
    fn list_capabilities() -> Vec<String> {
        vec![CAPABILITY.to_string()]
    }

    fn invoke(request: InvokeRequest) -> Result<InvokeResponse, InvokeError> {
        if request.capability != CAPABILITY {
            return Err(InvokeError::UnknownCapability(request.capability));
        }

        let input: QualityInput = serde_json::from_slice(&request.args)
            .map_err(|e| InvokeError::InvalidArgs(format!("deserialise QualityInput: {e}")))?;
        let output = analyse(&input);
        let stdout = serde_json::to_vec(&output)
            .map_err(|e| InvokeError::Internal(format!("serialise QualityOutput: {e}")))?;

        Ok(InvokeResponse {
            exit_code: 0,
            stdout,
            stderr: Vec::new(),
            duration_ms: 0,
            logged: false,
        })
    }
}

export!(Component);

// ---------------------------------------------------------------------------
// Host-side unit tests
// ---------------------------------------------------------------------------

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    #[test]
    fn count_occurrences_simple() {
        assert_eq!(count_occurrences("aaa", "a"), 3);
        assert_eq!(count_occurrences("hello world", "o"), 2);
        assert_eq!(count_occurrences("no matches", "xyz"), 0);
        assert_eq!(count_occurrences("", "x"), 0);
        assert_eq!(count_occurrences("abc", ""), 0);
    }

    #[test]
    fn score_is_one_for_zero_antipatterns() {
        assert!((score(0, 100) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn score_decreases_with_density() {
        let s_low = score(1, 100);
        let s_high = score(10, 100);
        assert!(s_low > s_high);
        assert!(s_high > 0.0);
    }
}
