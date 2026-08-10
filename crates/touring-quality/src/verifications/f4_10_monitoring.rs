//! FF4_10 -- Monitoring & Observability verifier.
//!
//! **Real engine (default `workspace-integration`)**: delegates to
//! `touring_analysis::quality::analyze_monitoring` -- a polyglot detector of
//! the canonical Monitoring & Observability smells: observability smells (println-debug, no-tracing, no-instrument, no-metrics, no-otel) -- Rust + Python polyglot.
//!
//! It is **disjoint** from neighbouring dims (each detector keys on a
//! different smell -- see `touring-analysis/src/quality/f4_10_monitoring.rs`
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

/// FF4_10 verifier -- Monitoring & Observability.
#[allow(non_camel_case_types)]
pub struct F4_10_Monitoring;

impl Verification for F4_10_Monitoring {
    fn id(&self) -> DimId {
        DimId::F4_10
    }
    fn measure(&self, target: &Path) -> Result<(f32, String)> {
        analyze_f4_10_monitoring_dim(target)
    }
}

// -- Real engine: Monitoring & Observability detection -----------------------------------------
#[cfg(feature = "workspace-integration")]
fn analyze_f4_10_monitoring_dim(target: &Path) -> Result<(f32, String)> {
    use touring_analysis::quality::{analyze_monitoring, score_monitoring};
    if is_detector_own_source(target) {
        return Ok((
            1.0,
            "FF4_10: detector own source (Monitoring & Observability needle vocabulary embedded as data) -- score=1.000"
                .to_string(),
        ));
    }
    let raw = crate::verifications::read_target_source(target)?;
    let lang = crate::verifications::lang_from_ext(target);
    let r = analyze_monitoring(&raw, lang);
    let value = score_monitoring(&r);
    let top = crate::verifications::top_finding(&r.findings);
    let evidence = format!(
        "FF4_10: {} Monitoring & Observability smell(s) over {} lines ({lang}) -- score={value:.3}          (touring-analysis analyze_monitoring: see header for detector catalog){top}",
        r.violations, r.total_lines
    );
    Ok((value, evidence))
}

/// The engine and this verifier embed the Monitoring & Observability needle vocabulary as
/// detection data, so scoring their own source is a self-match false
/// positive. Mirrors `f2_13_scalability::is_detector_own_source`.
#[cfg(feature = "workspace-integration")]
fn is_detector_own_source(target: &Path) -> bool {
    crate::verifications::is_detector_own_source(target)
}

// -- Standalone fallback: Monitoring & Observability substring-density heuristic ---------------
#[cfg(not(feature = "workspace-integration"))]
fn analyze_f4_10_monitoring_dim(target: &Path) -> Result<(f32, String)> {
    let raw = crate::verifications::read_target_source(target)?;
    let lines = raw.lines().count().max(1) as f32;
    let smells = raw.matches("println!").count()
        + raw.matches("eprintln!").count()
        + raw.matches("print(").count();
    let value = (1.0 - (smells as f32 / lines) * 6.0).clamp(0.0, 1.0);
    let evidence = format!(
        "{} Monitoring & Observability smell(s) over {} lines (heuristic; build --features workspace-integration for full Monitoring & Observability analysis)",
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
        let s = F4_10_Monitoring.check(f.path()).expect("check");
        assert!(
            (0.0..=1.0).contains(&s.value),
            "score out of range: {}",
            s.value
        );
    }

    /// Monitoring & Observability: the wrapper invokes the real engine and returns a valid score for
    /// a fixture file with the expected extension. The engine's own dirty-vs-clean
    /// semantic assertions live in `touring-analysis/src/quality/f4_10_monitoring.rs::tests`.
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn test_engine_invoked_returns_valid_score() {
        let f = write_temp_ext(
            r#"fn main() {
    println!("debug");
    eprintln!("err");
}"#,
            ".rs",
        );
        let s = F4_10_Monitoring.check(f.path()).expect("check");
        assert!(
            (0.0..=1.0).contains(&s.value),
            "wrapper score out of [0,1]: {}",
            s.value
        );
        // Engine is invoked: evidence mentions the touring-analysis path
        assert!(
            s.evidence.contains("touring-analysis analyze_monitoring"),
            "evidence should reference real engine, got: {}",
            s.evidence
        );
    }
}
