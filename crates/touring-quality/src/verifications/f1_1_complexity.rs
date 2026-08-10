//! F1.1 — Cyclomatic/Cognitive Complexity verifier (D01).
//!
//! **Real engine (default `workspace-integration`)**: delegates to
//! [`touring_analysis::quality::estimate_complexity`] — a language-aware engine
//! that computes cyclomatic complexity (`max_complexity`), REAL cognitive
//! complexity (a nesting-depth-penalised single pass), and the SEI/Mozilla
//! maintainability index. Scored per the D01 thresholds (CC≤5→1.0, ≤10→0.8,
//! ≤20→0.5 warn, >20→fail) with a *per-function* cognitive penalty so a long
//! file of simple functions is not punished for its length (the cognitive total
//! is normalised by function count, not summed file-wide).
//!
//! **Standalone fallback (`--no-default-features`)**: the prior flat
//! control-flow keyword count over the whole buffer, clearly labelled.
//!
//! **Scope**: per-file (`AggKind::WeightedLoc` — large files weigh more in the
//! scope roll-up).

use crate::DimId;
use crate::verifications::Verification;
use anyhow::Result;
use std::path::Path;

/// F1.1 verifier — cyclomatic/cognitive complexity.
/// Named `F1_1_Complexity` (non_camel_case) to match the dim ID convention.
#[allow(non_camel_case_types)]
pub struct F1_1_Complexity;

impl Verification for F1_1_Complexity {
    fn id(&self) -> DimId {
        DimId::F1_1
    }

    fn measure(&self, target: &Path) -> Result<(f32, String)> {
        analyze_complexity_dim(target)
    }
}

// ── Real engine: touring-analysis estimate_complexity ─────────────────────────
#[cfg(feature = "workspace-integration")]
fn analyze_complexity_dim(target: &Path) -> Result<(f32, String)> {
    let raw = crate::verifications::read_target_source(target)?;
    let lang = crate::verifications::lang_from_ext(target);
    let m = touring_analysis::quality::estimate_complexity(&raw, lang);
    let value = score_complexity(m.max_complexity, m.cognitive_complexity, m.function_count);
    let evidence = format!(
        "F1.1: CC≈{} cognitive={} fns={} MI={:.0} ({lang}) — score={value:.3} \
         (touring-analysis estimate_complexity)",
        m.max_complexity, m.cognitive_complexity, m.function_count, m.maintainability_index
    );
    Ok((value, evidence))
}

/// D01 piecewise cyclomatic score, dampened by a *per-function* cognitive
/// penalty. Cyclomatic drives the bands (SonarQube/CodeClimate convention);
/// cognitive is the secondary signal and is normalised by function count so
/// that file length alone never lowers the score.
#[cfg(feature = "workspace-integration")]
fn score_complexity(cc: usize, cognitive: usize, function_count: usize) -> f32 {
    let cc = cc as f32;
    let cc_score = if cc <= 5.0 {
        1.0
    } else if cc <= 10.0 {
        1.0 - (cc - 5.0) / 5.0 * 0.2 // 1.0 → 0.8 across [5,10]
    } else if cc <= 20.0 {
        0.8 - (cc - 10.0) / 10.0 * 0.3 // 0.8 → 0.5 across [10,20]
    } else {
        (0.5 - (cc - 20.0) / 20.0 * 0.5).max(0.0) // 0.5 → 0.0 at CC=40
    };
    // Per-function cognitive load: nesting concentrated in one monster fn is a
    // real smell; the same total spread over many small fns is not.
    // `checked_div` returns None when there are 0 functions → fall back to the
    // raw cognitive total.
    let cog_per_fn = cognitive.checked_div(function_count).unwrap_or(cognitive);
    let cog_penalty = if cog_per_fn > 15 {
        0.2
    } else if cog_per_fn > 8 {
        0.1
    } else {
        0.0
    };
    // Cognitive-flatness floor (SonarQube cognitive-complexity principle,
    // 2026-07-02 harness calibration): a file with LOW per-function cognitive
    // load whose cyclomatic is high is a flat dispatch/lookup structure — a big
    // `match` of simple arms. McCabe increments CC for every arm, but such code
    // carries little perceived difficulty (no nesting), which is exactly why
    // SonarQube/CodeClimate score maintainability on cognitive, not cyclomatic.
    // Floor a cognitively-flat file at the Silver band (moderate concern / review
    // — never Unranked), while genuinely NESTED complexity (cog_per_fn > 8, which
    // also triggers cog_penalty) keeps falling to the bottom unchanged.
    let flat_floor = if cog_per_fn <= 8 { 0.55 } else { 0.0 };
    (cc_score - cog_penalty).max(flat_floor).clamp(0.0, 1.0)
}

// ── Standalone fallback: flat control-flow keyword count ──────────────────────
#[cfg(not(feature = "workspace-integration"))]
fn analyze_complexity_dim(target: &Path) -> Result<(f32, String)> {
    let raw = crate::verifications::read_target_source(target)?;

    let mut cyclomatic = 1u32; // base
    for token in [
        "if ", "else if", "match ", "for ", "while ", "loop ", "&&", "||", "?",
    ] {
        cyclomatic += raw.matches(token).count() as u32;
    }
    let loc = raw.lines().count().max(1);
    let cc_f = cyclomatic as f32;
    let value = (1.0 - (cc_f - 1.0) / 30.0).clamp(0.0, 1.0);
    let evidence = format!(
        "CC≈{cyclomatic} (loc={loc}) — control-flow keyword count \
         (build --features workspace-integration for estimate_complexity)"
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
    fn test_simple_function_scores_high() {
        let f = write_temp_rs("fn add(a: i32, b: i32) -> i32 { a + b }\n");
        let s = F1_1_Complexity.check(f.path()).expect("check");
        assert!(
            s.value > 0.9,
            "simple fn should score > 0.9, got {}",
            s.value
        );
    }

    #[test]
    fn test_complex_function_scores_lower() {
        let code = r#"
fn process(x: i32, y: i32) -> i32 {
    if x > 0 {
        if y > 0 {
            if x > y {
                for i in 0..x {
                    if i % 2 == 0 && i > 0 {
                        match i {
                            1 => return 1,
                            2 => return 2,
                            _ => return 0,
                        }
                    }
                }
            } else if x < y {
                while y > 0 {
                    y -= 1;
                    if y == 0 || y == 1 {
                        return y;
                    }
                }
            } else {
                loop {
                    if x == 0 {
                        break;
                    }
                }
            }
        }
    }
    0
}
"#;
        let f = write_temp_rs(code);
        let s = F1_1_Complexity.check(f.path()).expect("check");
        assert!(
            s.value < 0.7,
            "complex nested fn should score < 0.7, got {}",
            s.value
        );
    }

    #[test]
    fn test_empty_file_scores_perfect() {
        let f = write_temp_rs("");
        let s = F1_1_Complexity.check(f.path()).expect("check");
        assert!((s.value - 1.0).abs() < 1e-6);
    }

    /// Faithfulness fixture (real engine): a long file of many *simple*
    /// functions must NOT be penalised for length — per-function cognitive load
    /// is low even though the file-wide cognitive total is non-trivial.
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn test_many_simple_functions_not_penalised_for_length() {
        let body: String = (0..40)
            .map(|i| format!("fn f{i}(x: i32) -> i32 {{ if x > 0 {{ x }} else {{ 0 }} }}\n"))
            .collect();
        let f = write_temp_rs(&body);
        let s = F1_1_Complexity.check(f.path()).expect("check");
        assert!(
            s.value > 0.9,
            "40 simple fns (1 branch each) must stay high, got {}",
            s.value
        );
    }

    /// Real engine: deep nesting concentrated in one function lowers the score
    /// via the per-function cognitive penalty.
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn test_single_deeply_nested_fn_is_penalised() {
        let code = "fn deep(a: bool, b: bool, c: bool, d: bool) {\n\
            if a { if b { if c { if d { for i in 0..10 { while i > 0 { if a && b { } } } } } } }\n}\n";
        let f = write_temp_rs(code);
        let s = F1_1_Complexity.check(f.path()).expect("check");
        assert!(
            s.value < 1.0,
            "deep single-fn nesting must be penalised, got {}",
            s.value
        );
    }

    #[test]
    fn test_returns_valid_score() {
        let f = write_temp_rs("fn a() {}\nfn b() { if true {} }\n");
        let s = F1_1_Complexity.check(f.path()).expect("check");
        assert!((0.0..=1.0).contains(&s.value));
    }

    /// Calibration (2026-07-02): a flat dispatcher — high cyclomatic (McCabe
    /// counts every match arm) but LOW per-function cognitive load — must be
    /// floored to the Silver band, not scored Unranked. Fixture mirrors the real
    /// `cli/ast.rs` metrics (CC≈88, cognitive≈74 over 26 fns → cog/fn≈2.8).
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn flat_dispatcher_is_floored_not_unranked() {
        let flat = score_complexity(88, 74, 26);
        assert!(
            flat >= 0.5,
            "cognitively-flat dispatcher must land ≥ Silver, got {flat}"
        );
    }

    /// Guard the guard: genuinely NESTED complexity — high cyclomatic AND high
    /// per-function cognitive load — must NOT get the flat-dispatch floor and must
    /// stay penalised to the bottom.
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn nested_monster_is_not_floored() {
        let nested = score_complexity(88, 200, 4); // cog/fn = 50 → deeply nested
        assert!(
            nested < 0.5,
            "nested high-cognitive fn must stay penalised, got {nested}"
        );
    }
}
