//! F1.2 — Maintainability verifier (D02).
//!
//! **Real engine (default `workspace-integration`)**: delegates to
//! [`touring_analysis::quality::estimate_complexity`] and scores the
//! **Maintainability Index** (MI) it computes — the SEI/Mozilla variant
//! `max(0, (171 − 5.2·ln(V) − 0.23·CC − 16.2·ln(LLOC))·100/171)`, an
//! industry-standard maintainability metric (Visual Studio / `radon` /
//! `rust-code-analysis`) that folds Halstead volume (a naming/magic-number
//! proxy), cyclomatic complexity, and logical lines into one score. The MI→score
//! mapping uses **radon's empirically-calibrated bands** (A≥20, B 10–20, C<10)
//! for this exact normalized formula — *not* the engine doc-comment's "≥85"
//! bands, which describe the pre-normalization SEI scale and would fail every
//! real file. For Rust, the MI score is blended (0.6/0.4) with
//! [`touring_analysis::quality::RustQualitySignals`]`::health_score()` — a
//! **syn-AST** signal (unsafe density + semantic complexity) — so abstract or
//! unsafe Rust, which is genuinely harder to maintain, is reflected.
//!
//! **Standalone fallback (`--no-default-features`)**: the prior function-length
//! + short-identifier heuristic over the whole buffer, clearly labelled.
//!
//! **Scope**: per-file (`AggKind::WeightedLoc`).

use crate::DimId;
use crate::verifications::Verification;
use anyhow::Result;
use std::path::Path;

/// F1.2 verifier — maintainability index + Rust semantic health.
#[allow(non_camel_case_types)]
pub struct F1_2_Maintainability;

impl Verification for F1_2_Maintainability {
    fn id(&self) -> DimId {
        DimId::F1_2
    }

    fn measure(&self, target: &Path) -> Result<(f32, String)> {
        analyze_maintainability_dim(target)
    }
}

// ── Real engine: SEI/Mozilla Maintainability Index + Rust AST health ──────────
#[cfg(feature = "workspace-integration")]
fn analyze_maintainability_dim(target: &Path) -> Result<(f32, String)> {
    use touring_analysis::quality::{RustQualitySignals, estimate_complexity};

    let raw = crate::verifications::read_target_source(target)?;
    let lang = crate::verifications::lang_from_ext(target);
    let m = estimate_complexity(&raw, lang);

    // Degenerate input (no logical lines → MI is 0.0 by construction): a file
    // with nothing to maintain is vacuously maintainable, not unmaintainable.
    if m.lloc == 0 {
        return Ok((
            1.0,
            format!("F1.2: no logical code (lloc=0) — vacuously maintainable ({lang})"),
        ));
    }

    let mi_score = score_maintainability(m.maintainability_index);

    // For Rust, blend with the syn-AST health signal (unsafe + semantic
    // complexity). `from_source` returns None on parse failure (e.g. a partial
    // snippet) → MI alone, which is the correct graceful degradation.
    let (value, rust_note) = match RustQualitySignals::from_source(&raw) {
        Some(sig) => {
            // 0.6 MI / 0.4 AST-health: the MI band drives, but the size-robust
            // syn-AST health gives a floor so a large yet semantically-clean file
            // (low MI purely from `ln(LLOC)`) is a "consider splitting" Warn, not
            // a Fail.
            let blended = (mi_score * 0.6 + sig.health_score() * 0.4).clamp(0.0, 1.0);
            (
                blended,
                format!(
                    " rust_health={:.2} sem_cx={:.2}",
                    sig.health_score(),
                    sig.semantic_complexity
                ),
            )
        }
        None => (mi_score, String::new()),
    };

    let evidence = format!(
        "F1.2: MI={:.0} (cc={} lloc={}) mi_score={mi_score:.2}{rust_note} ({lang}) — \
         score={value:.3} (touring-analysis estimate_complexity, SEI/Mozilla MI)",
        m.maintainability_index, m.max_complexity, m.lloc
    );
    Ok((value, evidence))
}

/// Map the SEI/Mozilla Maintainability Index to a `[0,1]` score using **radon's
/// empirically-calibrated bands** for this exact formula: rank **A (very high)
/// MI≥20**, **B (moderate) 10–20**, **C (poor) <10** (radon: A=20–100, B=10–19,
/// C=0–9). The `estimate_maintainability_index` doc-comment's "≥85" bands
/// describe the *pre-normalization* 0–171 SEI scale and do **not** match the
/// normalized (`×100/171`) output the engine actually returns — empirically a
/// clean ~150-line file scores MI≈20 and a clean ~800-line file MI≈2. Scoring
/// those against the 85 threshold (the bug this calibration fixes) would fail
/// every real file. radon is the canonical tool using this identical formula.
#[cfg(feature = "workspace-integration")]
fn score_maintainability(mi: f64) -> f32 {
    let mi = mi as f32;
    if mi >= 25.0 {
        1.0
    } else if mi >= 20.0 {
        0.9 + (mi - 20.0) / 5.0 * 0.1 // 0.9 → 1.0 across [20,25] (radon A)
    } else if mi >= 10.0 {
        0.6 + (mi - 10.0) / 10.0 * 0.3 // 0.6 → 0.9 across [10,20] (radon B)
    } else if mi >= 5.0 {
        0.4 + (mi - 5.0) / 5.0 * 0.2 // 0.4 → 0.6 across [5,10]
    } else {
        (mi / 5.0 * 0.4).clamp(0.0, 0.4) // 0.0 → 0.4 across [0,5] (radon C)
    }
}

// ── Standalone fallback: function length + identifier-naming heuristic ─────────
#[cfg(not(feature = "workspace-integration"))]
fn analyze_maintainability_dim(target: &Path) -> Result<(f32, String)> {
    let raw = crate::verifications::read_target_source(target)?;

    let mut long_function_penalty = 0.0_f32;
    let mut current_len = 0u32;
    let mut in_fn = false;
    for line in raw.lines() {
        if line.trim_start().starts_with("fn ") || line.trim_start().starts_with("pub fn ") {
            if in_fn && current_len > 50 {
                long_function_penalty += 0.1;
            }
            in_fn = true;
            current_len = 0;
        } else if in_fn {
            current_len += 1;
            if line == "}" && current_len > 50 {
                long_function_penalty += 0.1;
            }
        }
    }
    long_function_penalty = long_function_penalty.min(1.0);

    let short_id_count = raw
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|w| w.len() == 1 || w.len() == 2)
        .filter(|w| w.chars().next().is_some_and(|c| c.is_ascii_lowercase()))
        .count();
    let short_id_penalty = (short_id_count as f32 * 0.05).min(1.0);

    let total_penalty = (long_function_penalty + short_id_penalty).min(1.0);
    let value = (1.0 - total_penalty).clamp(0.0, 1.0);
    let evidence = format!(
        "long_fn_penalty={long_function_penalty:.2}, short_id_penalty={short_id_penalty:.2} \
         (heuristic; build --features workspace-integration for SEI/Mozilla MI) — score={value:.3}"
    );
    Ok((value, evidence))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn write_temp_rs(content: &str) -> NamedTempFile {
        let mut f = tempfile::Builder::new()
            .suffix(".rs")
            .tempfile()
            .expect("create temp");
        f.write_all(content.as_bytes()).expect("write");
        f
    }

    #[test]
    fn test_clean_function_scores_high() {
        let code = "fn well_named_function() -> i32 {\n    let result = 42;\n    result\n}\n";
        let f = write_temp_rs(code);
        let s = F1_2_Maintainability.check(f.path()).expect("check");
        assert!(
            s.value > 0.8,
            "small clean fn should be highly maintainable, got {}",
            s.value
        );
    }

    #[test]
    fn test_empty_file_is_vacuously_maintainable() {
        let f = write_temp_rs("");
        let s = F1_2_Maintainability.check(f.path()).expect("check");
        assert!(
            s.value > 0.9,
            "empty file has nothing to maintain → high, got {}",
            s.value
        );
    }

    #[test]
    fn test_returns_valid_score() {
        let f = write_temp_rs("fn a() {}\nfn b() { let x = 1; }\n");
        let s = F1_2_Maintainability.check(f.path()).expect("check");
        assert!((0.0..=1.0).contains(&s.value));
    }

    /// MI band mapping is monotone: a higher MI never yields a lower score.
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn test_score_maintainability_monotone() {
        let samples = [10.0, 40.0, 50.0, 65.0, 75.0, 85.0, 95.0];
        let mut prev = -1.0_f32;
        for mi in samples {
            let s = score_maintainability(mi);
            assert!((0.0..=1.0).contains(&s));
            assert!(s >= prev, "MI={mi} gave {s} < prev {prev}");
            prev = s;
        }
    }

    /// Lock the radon calibration: MI≈20 (a clean ~150-line file, radon rank A)
    /// must read as maintainable, NOT fail — the bug this band fixes. The old
    /// "≥85" bands gave MI=20 a 0.0 score.
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn test_radon_calibration_not_miscalibrated() {
        assert!(
            score_maintainability(20.0) >= 0.9,
            "MI=20 is radon rank A (very high) — must be ≥0.9, got {}",
            score_maintainability(20.0)
        );
        assert!(
            score_maintainability(12.0) >= 0.6,
            "MI=12 is radon rank B (moderate), got {}",
            score_maintainability(12.0)
        );
        assert!(
            score_maintainability(2.0) < 0.4,
            "MI=2 is radon rank C (poor), got {}",
            score_maintainability(2.0)
        );
    }

    /// Real engine: a large file (low MI via the `ln(LLOC)` term) is less
    /// maintainable than a tiny clean one — F1.2 captures *size*, the
    /// distinguishing maintainability factor (raw control-flow branching is
    /// F1.1's concern, not F1.2's; this test deliberately varies size, not
    /// nesting, so it exercises the dimension's own signal).
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn test_large_file_scores_below_tiny_clean() {
        let clean = write_temp_rs("fn add(a: i32, b: i32) -> i32 { a + b }\n");
        let mut big = String::from("fn big() -> i64 {\n    let mut acc = 0i64;\n");
        for i in 0..200 {
            big.push_str(&format!(
                "    let v{i} = {i} as i64; acc += v{i} * {i} as i64;\n"
            ));
        }
        big.push_str("    acc\n}\n");
        let messy = write_temp_rs(&big);
        let sc = F1_2_Maintainability.check(clean.path()).expect("check");
        let sm = F1_2_Maintainability.check(messy.path()).expect("check");
        assert!(
            sm.value < sc.value,
            "large file ({}) must score below tiny clean ({})",
            sm.value,
            sc.value
        );
    }
}
