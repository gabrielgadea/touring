//! `FuzzySearcher` — trait abstraction for fuzzy symbol suggestions in VGP.
//!
//! Decouples the VGP engine from the concrete subprocess invocation, enabling
//! future swap to SIMD-accelerated backends via the `simd-fuzzy` feature flag.
//!
//! # Design
//! - [`FuzzySearcher`] trait: thread-safe, injectable backend
//! - [`NoopFuzzySearcher`]: default subprocess-based implementation
//! - `simd-fuzzy` feature: reserved for future `touring-simd::TopKSearcher` integration

use crate::plan::contracts::SymbolRef;
use crate::plan::failure::{FuzzySuggestion, SuggestionSource};

/// Trait for fuzzy symbol search backends.
///
/// Implementations must be `Send + Sync` — [`VgpEngine`](crate::vgp::engine::VgpEngine)
/// is used across rayon thread-pool workers via `spawn_blocking`.
///
/// # Returns
/// Up to `k` fuzzy suggestions for `name`, sorted by descending confidence.
pub trait FuzzySearcher: Send + Sync {
    /// Search for the top-k symbols similar to `name`.
    ///
    /// Implementations should be **fail-open**: return `Vec::new()` on any
    /// error rather than propagating, since VGP callers treat missing
    /// suggestions as a soft fallback, not a hard failure.
    fn search(&self, name: &str, k: usize) -> Vec<FuzzySuggestion>;
}

// ── Default subprocess backend ────────────────────────────────────────────────

/// Subprocess-based fuzzy searcher — delegates to `touring index search` CLI.
///
/// This is the default implementation. It is correct but incurs a subprocess
/// fork per call. SIMD-accelerated backends can be injected via the
/// `simd-fuzzy` feature for production hot paths.
pub struct NoopFuzzySearcher;

impl FuzzySearcher for NoopFuzzySearcher {
    fn search(&self, name: &str, k: usize) -> Vec<FuzzySuggestion> {
        use std::process::Command;

        let prefix = if name.len() > 4 { &name[..4] } else { name };
        let output = Command::new("touring")
            .args(["index", "search", prefix, "-j"])
            .output();

        let candidates = match output {
            Ok(out) if out.status.success() => {
                let text = String::from_utf8_lossy(&out.stdout);
                serde_json::from_str::<serde_json::Value>(&text)
                    .ok()
                    .and_then(|v| v.as_array().cloned())
                    .unwrap_or_default()
            }
            _ => return Vec::new(),
        };

        candidates
            .iter()
            .take(k)
            .filter_map(|entry| {
                let sym_name = entry.get("name")?.as_str()?;
                // Clamp to 255 before casting to u8 — levenshtein_distance returns usize.
                #[allow(clippy::cast_possible_truncation)]
                let distance = levenshtein_distance(name, sym_name).min(255) as u8;
                // Precision loss acceptable for fuzzy confidence score (metric, not exact).
                #[allow(clippy::cast_precision_loss)]
                let name_len_f = name.len().max(1) as f64;
                let confidence = (1.0_f64 - (f64::from(distance) / name_len_f)).max(0.0);
                Some(FuzzySuggestion {
                    symbol: SymbolRef {
                        name: sym_name.to_owned(),
                        crate_name: entry
                            .get("crate_name")
                            .and_then(|c| c.as_str())
                            .map(ToOwned::to_owned),
                        module_path: None,
                        definition_hint: None,
                    },
                    distance,
                    confidence,
                    source: SuggestionSource::TrigramIndex,
                })
            })
            .collect()
    }
}

// ── Levenshtein distance ──────────────────────────────────────────────────────

/// Simple Levenshtein distance for short symbol names.
///
/// O(m·n) — acceptable for typical symbol name lengths (<64 chars).
fn levenshtein_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let (m, n) = (a.len(), b.len());
    let mut dp = vec![vec![0usize; n + 1]; m + 1];
    for (i, row) in dp.iter_mut().enumerate().take(m + 1) {
        row[0] = i;
    }
    for (j, cell) in dp[0].iter_mut().enumerate().take(n + 1) {
        *cell = j;
    }
    for i in 1..=m {
        for j in 1..=n {
            dp[i][j] = if a[i - 1] == b[j - 1] {
                dp[i - 1][j - 1]
            } else {
                1 + dp[i - 1][j].min(dp[i][j - 1]).min(dp[i - 1][j - 1])
            };
        }
    }
    dp[m][n]
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levenshtein_identical() {
        assert_eq!(levenshtein_distance("VgpEngine", "VgpEngine"), 0);
    }

    #[test]
    fn levenshtein_one_char_diff() {
        assert_eq!(levenshtein_distance("VgpEngine", "VgpEngyne"), 1);
    }

    #[test]
    fn levenshtein_empty() {
        assert_eq!(levenshtein_distance("", "abc"), 3);
        assert_eq!(levenshtein_distance("abc", ""), 3);
    }

    #[test]
    fn noop_fuzzy_searcher_fail_open() {
        // If daemon unavailable, should return empty (not panic).
        let searcher = NoopFuzzySearcher;
        // This may return empty or results depending on daemon state — must not panic.
        let _ = searcher.search("VgpEngine", 3);
    }
}
