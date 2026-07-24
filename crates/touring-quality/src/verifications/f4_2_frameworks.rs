//! FF4_2 -- Framework Patterns verifier.
//!
//! **Real engine (default `workspace-integration`)**: delegates to
//! `touring_analysis::quality::analyze_frameworks` -- a polyglot detector of
//! the canonical Framework Patterns smells: framework-pattern misuses (block-on-in-runtime, sync-mutex-in-async, reqwest-blocking-in-async) -- Rust + Python polyglot.
//!
//! It is **disjoint** from neighbouring dims (each detector keys on a
//! different smell -- see `touring-analysis/src/quality/f4_2_frameworks.rs`
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

use crate::verifications::Verification;
use crate::{DimId, DimScore};
use anyhow::Result;
use std::path::Path;

/// FF4_2 verifier -- Framework Patterns.
#[allow(non_camel_case_types)]
pub struct F4_2_Frameworks;

impl Verification for F4_2_Frameworks {
    fn id(&self) -> DimId {
        DimId::F4_2
    }
    fn check(&self, target: &Path) -> Result<DimScore> {
        let (value, evidence) = analyze_f4_2_frameworks_dim(target)?;
        Ok(crate::verifications::finish(
            self.id(),
            value,
            evidence,
            target,
        ))
    }
}

// -- Real engine: Framework Patterns detection -----------------------------------------
#[cfg(feature = "workspace-integration")]
fn analyze_f4_2_frameworks_dim(target: &Path) -> Result<(f32, String)> {
    use touring_analysis::quality::{analyze_frameworks, score_frameworks};
    if is_detector_own_source(target) {
        return Ok((
            1.0,
            "FF4_2: detector own source (Framework Patterns needle vocabulary embedded as data) -- score=1.000"
                .to_string(),
        ));
    }
    let raw = crate::verifications::read_target_source(target)?;
    let lang = crate::verifications::lang_from_ext(target);
    let r = analyze_frameworks(&raw, lang);
    let value = score_frameworks(&r);
    let top = r
        .findings
        .first()
        .map(|(m, c)| format!("; top: {} ({}x)", m, c))
        .unwrap_or_default();
    let evidence = format!(
        "FF4_2: {} Framework Patterns smell(s) over {} lines ({lang}) -- score={value:.3}          (touring-analysis analyze_frameworks: see header for detector catalog){top}",
        r.violations, r.total_lines
    );
    Ok((value, evidence))
}

/// The engine and this verifier embed the Framework Patterns needle vocabulary as
/// detection data, so scoring their own source is a self-match false
/// positive. Mirrors `f2_13_scalability::is_detector_own_source`.
#[cfg(feature = "workspace-integration")]
fn is_detector_own_source(target: &Path) -> bool {
    crate::verifications::is_detector_own_source(target)
}

// -- Standalone fallback: Framework Patterns substring-density heuristic ---------------
#[cfg(not(feature = "workspace-integration"))]
fn analyze_f4_2_frameworks_dim(target: &Path) -> Result<(f32, String)> {
    let raw = crate::verifications::read_target_source(target)?;
    let lines = raw.lines().count().max(1) as f32;
    let smells = raw.matches("block_on").count()
        + raw.matches("std::sync::Mutex").count()
        + raw.matches("reqwest::blocking").count();
    let value = (1.0 - (smells as f32 / lines) * 6.0).clamp(0.0, 1.0);
    let evidence = format!(
        "{} Framework Patterns smell(s) over {} lines (heuristic; build --features workspace-integration for full Framework Patterns analysis)",
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
        let s = F4_2_Frameworks.check(f.path()).expect("check");
        assert!(
            (0.0..=1.0).contains(&s.value),
            "score out of range: {}",
            s.value
        );
    }

    /// Framework Patterns: the wrapper invokes the real engine and returns a valid score for
    /// a fixture file with the expected extension. The engine's own dirty-vs-clean
    /// semantic assertions live in `touring-analysis/src/quality/f4_2_frameworks.rs::tests`.
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn test_engine_invoked_returns_valid_score() {
        let f = write_temp_ext(
            r#"fn runtime() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {});
}"#,
            ".rs",
        );
        let s = F4_2_Frameworks.check(f.path()).expect("check");
        assert!(
            (0.0..=1.0).contains(&s.value),
            "wrapper score out of [0,1]: {}",
            s.value
        );
        // Engine is invoked: evidence mentions the touring-analysis path
        assert!(
            s.evidence.contains("touring-analysis analyze_frameworks"),
            "evidence should reference real engine, got: {}",
            s.evidence
        );
    }
}
