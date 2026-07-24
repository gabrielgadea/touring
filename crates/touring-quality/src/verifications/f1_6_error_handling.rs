//! F1.6 — Error Handling verifier (D06).
//!
//! **Real engine (default `workspace-integration`)**: delegates to
//! [`touring_analysis::quality::count_prod_hazards`] — a **region-aware**
//! scanner that counts `.unwrap()` / `.expect(` / `panic!` occurrences in
//! *production* code only, excluding comments and `#[cfg(test)]`/`#[test]`
//! regions (via `code_regions`). This is the faithful D06 signal: the dimension
//! explicitly exempts test code, so counting a test's `.unwrap()` as a hazard —
//! which the raw `count_unwraps` scanner does — is a false positive. Error
//! *propagation* quality (functions returning `Result`/`Option`, via
//! [`touring_analysis::quality::error_coverage::analyze_error_coverage`]) lifts
//! the score: idiomatic `?`-propagation is healthy.
//!
//! **Standalone fallback (`--no-default-features`)**: the prior raw substring
//! count of `panic!` / `.unwrap()` / `.expect(` over the whole buffer (no
//! comment/test suppression), clearly labelled.
//!
//! **Known limitation (narrow)**: production *string literals* are not
//! suppressed (consistent with `code_regions`, where injection lives in
//! strings). A hazard *token* embedded in a production string — e.g. a detector
//! or codegen file holding the byte-string needle `b".unwrap()"` — is counted.
//! This is the detector-self-match class (cf. the F2.1 self-match fixed in the
//! Part 9 program): rare outside detector/lint code, and never a comment/test
//! false positive (those are excluded). A `.unwrap()` token inside a string can
//! never be a real call, so a future refinement could also mask string interiors.
//!
//! **Scope**: per-file (`AggKind::WeightedLoc`).

use crate::verifications::Verification;
use crate::{DimId, DimScore};
use anyhow::Result;
use std::path::Path;

/// F1.6 verifier — Error Handling.
#[allow(non_camel_case_types)]
pub struct F1_6_ErrorHandling;

impl Verification for F1_6_ErrorHandling {
    fn id(&self) -> DimId {
        DimId::F1_6
    }

    fn check(&self, target: &Path) -> Result<DimScore> {
        let (value, evidence) = analyze_error_handling_dim(target)?;
        Ok(crate::verifications::finish(
            self.id(),
            value,
            evidence,
            target,
        ))
    }
}

// ── Real engine: region-aware prod hazards + error-propagation coverage ───────
#[cfg(feature = "workspace-integration")]
fn analyze_error_handling_dim(target: &Path) -> Result<(f32, String)> {
    use touring_analysis::quality::count_prod_hazards;
    use touring_analysis::quality::error_coverage::analyze_error_coverage;

    let raw = crate::verifications::read_target_source(target)?;
    let lang = crate::verifications::lang_from_ext(target);

    let hazards = count_prod_hazards(&raw, lang);
    let cov = analyze_error_coverage(&raw, lang);

    let value = score_error_handling(
        hazards.unwraps,
        hazards.expects,
        hazards.panics,
        hazards.total_lines,
        cov.result_ratio as f32,
    );

    let evidence = format!(
        "F1.6: prod hazards unwrap={} expect={} panic={} | propagation result_ratio={:.2} \
         qmark={} ({lang}) — score={value:.3} \
         (touring-analysis count_prod_hazards, comment/#[cfg(test)] excluded)",
        hazards.unwraps, hazards.expects, hazards.panics, cov.result_ratio, cov.question_mark_count,
    );
    Ok((value, evidence))
}

/// D06 score: penalise production error-handling hazards (density-based, using
/// the same per-100-lines convention as `unwrap_audit::risk_score`), reward
/// idiomatic error propagation. `panic!` is weighted heaviest, then `.unwrap()`,
/// then `.expect(` (an `expect` carries a documented reason, the least severe).
///
/// Files with no error-returning functions sit at a neutral base — D06 must not
/// punish a pure-data module for having no errors to handle; good `Result`
/// propagation lifts the base toward `1.0`.
#[cfg(feature = "workspace-integration")]
fn score_error_handling(
    unwraps: usize,
    expects: usize,
    panics: usize,
    total_lines: usize,
    result_ratio: f32,
) -> f32 {
    let loc = total_lines.max(1) as f32;
    let per_100 = |count: usize| (count as f32 / loc) * 100.0;
    let hazard_penalty =
        (per_100(unwraps) * 0.10 + per_100(expects) * 0.04 + per_100(panics) * 0.15).min(0.9);
    let base = 0.85 + 0.15 * result_ratio.clamp(0.0, 1.0);
    (base - hazard_penalty).clamp(0.0, 1.0)
}

// ── Standalone fallback: raw substring count (no region suppression) ──────────
#[cfg(not(feature = "workspace-integration"))]
fn analyze_error_handling_dim(target: &Path) -> Result<(f32, String)> {
    let raw = crate::verifications::read_target_source(target)?;

    let panics = raw.matches("panic!").count();
    let unwraps = raw.matches(".unwrap()").count() + raw.matches(".expect(").count();
    let penalty = (panics as f32 * 0.1 + unwraps as f32 * 0.05).min(1.0);
    let value = (1.0 - penalty).clamp(0.0, 1.0);
    let evidence = format!(
        "Error Handling: panics={panics} unwrap/expect={unwraps} (raw substring count; \
         build --features workspace-integration for region-aware prod-only scan) — score={value:.3}"
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

    fn write_temp(content: &str) -> NamedTempFile {
        let mut f = NamedTempFile::new().expect("create temp");
        f.write_all(content.as_bytes()).expect("write");
        f
    }

    #[test]
    fn test_error_handling_returns_valid_score() {
        let f = write_temp_rs("fn example() {}\n");
        let s = F1_6_ErrorHandling.check(f.path()).expect("check");
        assert!(
            (0.0..=1.0).contains(&s.value),
            "score out of range: {}",
            s.value
        );
    }

    #[test]
    fn test_error_handling_empty_file() {
        let f = write_temp("");
        let s = F1_6_ErrorHandling.check(f.path()).expect("check");
        assert!((0.0..=1.0).contains(&s.value));
    }

    /// Clean error-propagating code (returns `Result`, uses `?`, no hazards)
    /// must score high.
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn test_clean_propagation_scores_high() {
        let code = "fn run() -> Result<i32, String> {\n    let x = parse()?;\n    Ok(x)\n}\n";
        let f = write_temp_rs(code);
        let s = F1_6_ErrorHandling.check(f.path()).expect("check");
        assert!(
            s.value > 0.9,
            "clean Result+? code should score > 0.9, got {}",
            s.value
        );
    }

    /// Production code peppered with unwrap/panic and no error propagation must
    /// score low.
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn test_prod_unwrap_panic_scores_low() {
        let code = "fn run() {\n    let a = foo().unwrap();\n    let b = bar().unwrap();\n    \
                    if a > b { panic!(\"bad\"); }\n    let c = baz().unwrap();\n}\n";
        let f = write_temp_rs(code);
        let s = F1_6_ErrorHandling.check(f.path()).expect("check");
        assert!(
            s.value < 0.6,
            "3 unwraps + panic in 6 lines should score low, got {}",
            s.value
        );
    }

    /// **End-to-end region-awareness proof**: a file whose only `.unwrap()`s live
    /// in a `#[cfg(test)]` module must NOT be penalised — D06 exempts test code.
    /// The raw fallback scanner would (wrongly) count them.
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn test_test_module_unwraps_do_not_penalise_production() {
        let code = "fn prod() -> Result<(), String> {\n    do_thing()?;\n    Ok(())\n}\n\
                    #[cfg(test)]\n\
                    mod tests {\n    #[test]\n    fn t() {\n        let _ = prod().unwrap();\n        \
                    let _ = other().unwrap();\n    }\n}\n";
        let f = write_temp_rs(code);
        let s = F1_6_ErrorHandling.check(f.path()).expect("check");
        assert!(
            s.value > 0.9,
            "test-only unwraps must not penalise prod error handling, got {}",
            s.value
        );
    }
}
