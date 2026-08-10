//! FF4_12 -- Environment Management verifier.
//!
//! **Real engine (default `workspace-integration`)**: delegates to
//! `touring_analysis::quality::analyze_env` -- a polyglot detector of
//! the canonical Environment Management smells: environment-management smells (no-secret-manager, no-config-layer, hardcoded-url) -- Rust + Python + JS/TS polyglot.
//!
//! It is **disjoint** from neighbouring dims (each detector keys on a
//! different smell -- see `touring-analysis/src/quality/f4_12_env.rs`
//! header for the full disjoint table).
//!
//! This replaces a W3/W4 stub that scored by raw substring density
//! (more keywords = lower score; a metric that is structurally
//! inverted -- idiomatic code legitimately names the concepts).
//!
//! **Standalone fallback (`--no-default-features`)**: a labelled
//! substring-density heuristic kept for environments without the
//! `touring-analysis` dep.
//!
//! **Scope**: per-file; rolls up as `AggKind::WeightedLoc`. ADVISORY-tier.

use crate::DimId;
use crate::verifications::Verification;
use anyhow::Result;
use std::path::Path;

/// FF4_12 verifier -- Environment Management.
#[allow(non_camel_case_types)]
pub struct F4_12_Env;

impl Verification for F4_12_Env {
    fn id(&self) -> DimId {
        DimId::F4_12
    }
    fn measure(&self, target: &Path) -> Result<(f32, String)> {
        analyze_f4_12_env_dim(target)
    }
}

// -- Real engine: Environment Management detection -----------------------------------------
#[cfg(feature = "workspace-integration")]
fn analyze_f4_12_env_dim(target: &Path) -> Result<(f32, String)> {
    use touring_analysis::quality::{analyze_env, score_env};
    if is_detector_own_source(target) {
        return Ok((
            1.0,
            "FF4_12: detector own source (Environment Management needle vocabulary embedded as data) -- score=1.000"
                .to_string(),
        ));
    }
    let raw = crate::verifications::read_target_source(target)?;
    let lang = crate::verifications::lang_from_ext(target);
    let r = analyze_env(&raw, lang);
    let value = score_env(&r);
    let top = crate::verifications::top_finding(&r.findings);
    let evidence = format!(
        "FF4_12: {} Environment Management smell(s) over {} lines ({lang}) -- score={value:.3}          (touring-analysis analyze_env: see header for detector catalog){top}",
        r.violations, r.total_lines
    );
    Ok((value, evidence))
}

/// The engine and this verifier embed the Environment Management needle vocabulary as
/// detection data, so scoring their own source is a self-match false
/// positive. Mirrors `f2_13_scalability::is_detector_own_source`.
#[cfg(feature = "workspace-integration")]
fn is_detector_own_source(target: &Path) -> bool {
    crate::verifications::is_detector_own_source(target)
}

// -- Standalone fallback: Environment Management substring-density heuristic ---------------
#[cfg(not(feature = "workspace-integration"))]
fn analyze_f4_12_env_dim(target: &Path) -> Result<(f32, String)> {
    let raw = crate::verifications::read_target_source(target)?;
    let lines = raw.lines().count().max(1) as f32;
    let smells = raw.matches("std::env::var").count()
        + raw.matches("dotenv").count()
        + raw.matches("Vault").count();
    let value = (1.0 - (smells as f32 / lines) * 6.0).clamp(0.0, 1.0);
    let evidence = format!(
        "{} Environment Management smell(s) over {} lines (heuristic; build --features workspace-integration for full Environment Management analysis)",
        smells, lines as usize
    );
    Ok((value, evidence))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn write_temp_ext(content: &str, suffix: &str) -> NamedTempFile {
        let mut f = tempfile::Builder::new()
            .suffix(suffix)
            .tempfile()
            .expect("create temp");
        f.write_all(content.as_bytes()).expect("write");
        f
    }

    #[test]
    fn test_returns_valid_score() {
        let f = write_temp_ext("fn example() {}\n", ".rs");
        let s = F4_12_Env.check(f.path()).expect("check");
        assert!(
            (0.0..=1.0).contains(&s.value),
            "score out of range: {}",
            s.value
        );
    }

    /// Environment Management: the wrapper invokes the real engine and returns a valid score for
    /// a fixture file with the expected extension. The engine's own dirty-vs-clean
    /// semantic assertions live in `touring-analysis/src/quality/f4_12_env.rs::tests`.
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn test_engine_invoked_returns_valid_score() {
        let f = write_temp_ext(
            r#"fn main() {
    let url = "https://api.example.com";
    let _ = url;
}"#,
            ".rs",
        );
        let s = F4_12_Env.check(f.path()).expect("check");
        assert!(
            (0.0..=1.0).contains(&s.value),
            "wrapper score out of [0,1]: {}",
            s.value
        );
        // Engine is invoked: evidence mentions the touring-analysis path
        assert!(
            s.evidence.contains("touring-analysis analyze_env"),
            "evidence should reference real engine, got: {}",
            s.evidence
        );
    }
}
