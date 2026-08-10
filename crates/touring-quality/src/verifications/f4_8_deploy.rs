//! FF4_8 -- Deployment Strategy verifier.
//!
//! **Real engine (default `workspace-integration`)**: delegates to
//! `touring_analysis::quality::analyze_deploy` -- a polyglot detector of
//! the canonical Deployment Strategy smells: deployment strategy smells (no-rollout, no-strategy, no-maxSurge, no-pause-step, no-rollback, no-readiness-probe) -- k8s/Argo Rollouts YAML.
//!
//! It is **disjoint** from neighbouring dims (each detector keys on a
//! different smell -- see `touring-analysis/src/quality/f4_8_deploy.rs`
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

/// FF4_8 verifier -- Deployment Strategy.
#[allow(non_camel_case_types)]
pub struct F4_8_Deploy;

impl Verification for F4_8_Deploy {
    fn id(&self) -> DimId {
        DimId::F4_8
    }
    fn measure(&self, target: &Path) -> Result<(f32, String)> {
        analyze_f4_8_deploy_dim(target)
    }
}

// -- Real engine: Deployment Strategy detection -----------------------------------------
#[cfg(feature = "workspace-integration")]
fn analyze_f4_8_deploy_dim(target: &Path) -> Result<(f32, String)> {
    use touring_analysis::quality::{analyze_deploy, score_deploy};
    if is_detector_own_source(target) {
        return Ok((
            1.0,
            "FF4_8: detector own source (Deployment Strategy needle vocabulary embedded as data) -- score=1.000"
                .to_string(),
        ));
    }
    let (raw, present) = crate::verifications::read_artifact_source(
        target,
        crate::verifications::ArtifactClass::Iac,
    );
    if !present {
        return Ok(crate::verifications::absent_artifact_score(
            "F4.8",
            crate::verifications::ArtifactClass::Iac,
        ));
    }
    let lang = crate::verifications::lang_from_ext(target);
    let r = analyze_deploy(&raw, lang);
    let value = score_deploy(&r);
    let top = crate::verifications::top_finding(&r.findings);
    let evidence = format!(
        "FF4_8: {} Deployment Strategy smell(s) over {} lines ({lang}) -- score={value:.3}          (touring-analysis analyze_deploy: see header for detector catalog){top}",
        r.violations, r.total_lines
    );
    Ok((value, evidence))
}

/// The engine and this verifier embed the Deployment Strategy needle vocabulary as
/// detection data, so scoring their own source is a self-match false
/// positive. Mirrors `f2_13_scalability::is_detector_own_source`.
#[cfg(feature = "workspace-integration")]
fn is_detector_own_source(target: &Path) -> bool {
    crate::verifications::is_detector_own_source(target)
}

// -- Standalone fallback: Deployment Strategy substring-density heuristic ---------------
#[cfg(not(feature = "workspace-integration"))]
fn analyze_f4_8_deploy_dim(target: &Path) -> Result<(f32, String)> {
    let raw = crate::verifications::read_target_source(target)?;
    let lines = raw.lines().count().max(1) as f32;
    let smells = raw.matches("strategy:").count() + raw.matches("readinessProbe").count();
    let value = (1.0 - (smells as f32 / lines) * 6.0).clamp(0.0, 1.0);
    let evidence = format!(
        "{} Deployment Strategy smell(s) over {} lines (heuristic; build --features workspace-integration for full Deployment Strategy analysis)",
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
        let s = F4_8_Deploy.check(f.path()).expect("check");
        assert!(
            (0.0..=1.0).contains(&s.value),
            "score out of range: {}",
            s.value
        );
    }

    /// Deployment Strategy: the wrapper invokes the real engine and returns a valid score for
    /// a fixture file with the expected extension. The engine's own dirty-vs-clean
    /// semantic assertions live in `touring-analysis/src/quality/f4_8_deploy.rs::tests`.
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn test_engine_invoked_returns_valid_score() {
        let f = write_temp_ext(
            r#"apiVersion: apps/v1
kind: Deployment
metadata:
  name: app
spec:
  replicas: 3
  template:
    spec:
      containers:
        - name: app
          image: myapp:latest"#,
            ".yml",
        );
        let s = F4_8_Deploy.check(f.path()).expect("check");
        assert!(
            (0.0..=1.0).contains(&s.value),
            "wrapper score out of [0,1]: {}",
            s.value
        );
        // Engine is invoked: evidence mentions the touring-analysis path
        assert!(
            s.evidence.contains("touring-analysis analyze_deploy"),
            "evidence should reference real engine, got: {}",
            s.evidence
        );
    }
}
