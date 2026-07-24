//! FF4_6 -- Build Configuration verifier.
//!
//! **Real engine (default `workspace-integration`)**: delegates to
//! `touring_analysis::quality::analyze_build_config` -- a polyglot detector of
//! the canonical Build Configuration smells: build-config smells (release profile, debug-in-release, LTO, strip, sccache, default-features=false, lockfile) -- polyglot Rust+Python+JS/TS+Go.
//!
//! It is **disjoint** from neighbouring dims (each detector keys on a
//! different smell -- see `touring-analysis/src/quality/f4_6_build_config.rs`
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

/// FF4_6 verifier -- Build Configuration.
#[allow(non_camel_case_types)]
pub struct F4_6_BuildConfig;

impl Verification for F4_6_BuildConfig {
    fn id(&self) -> DimId {
        DimId::F4_6
    }
    fn check(&self, target: &Path) -> Result<DimScore> {
        let (value, evidence) = analyze_f4_6_build_config_dim(target)?;
        Ok(crate::verifications::finish(
            self.id(),
            value,
            evidence,
            target,
        ))
    }
}

// -- Real engine: Build Configuration detection -----------------------------------------
#[cfg(feature = "workspace-integration")]
fn analyze_f4_6_build_config_dim(target: &Path) -> Result<(f32, String)> {
    use touring_analysis::quality::{analyze_build_config, score_build_config};
    if is_detector_own_source(target) {
        return Ok((
            1.0,
            "FF4_6: detector own source (Build Configuration needle vocabulary embedded as data) -- score=1.000"
                .to_string(),
        ));
    }
    let (raw, present) = crate::verifications::read_artifact_source(
        target,
        crate::verifications::ArtifactClass::Manifest,
    );
    if !present {
        return Ok(crate::verifications::absent_artifact_score(
            "F4.6",
            crate::verifications::ArtifactClass::Manifest,
        ));
    }
    let lang = crate::verifications::lang_from_ext(target);
    let r = analyze_build_config(&raw, lang);
    let value = score_build_config(&r);
    let top = r
        .findings
        .first()
        .map(|(m, c)| format!("; top: {} ({}x)", m, c))
        .unwrap_or_default();
    let evidence = format!(
        "FF4_6: {} Build Configuration smell(s) over {} lines ({lang}) -- score={value:.3}          (touring-analysis analyze_build_config: see header for detector catalog){top}",
        r.violations, r.total_lines
    );
    Ok((value, evidence))
}

/// The engine and this verifier embed the Build Configuration needle vocabulary as
/// detection data, so scoring their own source is a self-match false
/// positive. Mirrors `f2_13_scalability::is_detector_own_source`.
#[cfg(feature = "workspace-integration")]
fn is_detector_own_source(target: &Path) -> bool {
    crate::verifications::is_detector_own_source(target)
}

// -- Standalone fallback: Build Configuration substring-density heuristic ---------------
#[cfg(not(feature = "workspace-integration"))]
fn analyze_f4_6_build_config_dim(target: &Path) -> Result<(f32, String)> {
    let raw = crate::verifications::read_target_source(target)?;
    let lines = raw.lines().count().max(1) as f32;
    let smells = raw.matches("[profile.").count()
        + raw.matches("lto").count()
        + raw.matches("strip").count();
    let value = (1.0 - (smells as f32 / lines) * 6.0).clamp(0.0, 1.0);
    let evidence = format!(
        "{} Build Configuration smell(s) over {} lines (heuristic; build --features workspace-integration for full Build Configuration analysis)",
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
        let s = F4_6_BuildConfig.check(f.path()).expect("check");
        assert!(
            (0.0..=1.0).contains(&s.value),
            "score out of range: {}",
            s.value
        );
    }

    /// Build Configuration: the wrapper invokes the real engine and returns a valid score for
    /// a fixture file with the expected extension. The engine's own dirty-vs-clean
    /// semantic assertions live in `touring-analysis/src/quality/f4_6_build_config.rs::tests`.
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn test_engine_invoked_returns_valid_score() {
        let f = write_temp_ext(
            r#"[package]
name = "x"
version = "0.1.0""#,
            ".toml",
        );
        let s = F4_6_BuildConfig.check(f.path()).expect("check");
        assert!(
            (0.0..=1.0).contains(&s.value),
            "wrapper score out of [0,1]: {}",
            s.value
        );
        // Engine is invoked: evidence mentions the touring-analysis path
        assert!(
            s.evidence.contains("touring-analysis analyze_build_config"),
            "evidence should reference real engine, got: {}",
            s.evidence
        );
    }
}
