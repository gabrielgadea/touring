//! FF4_7 -- CI/CD Pipeline verifier.
//!
//! **Real engine (default `workspace-integration`)**: delegates to
//! `touring_analysis::quality::analyze_cicd` -- a polyglot detector of
//! the canonical CI/CD Pipeline smells: CI/CD pipeline smells (no-clippy-gate, no-test-gate, unpinned-action, script-injection-risk, no-cache, no-quality-gate) -- GitHub Actions YAML.
//!
//! It is **disjoint** from neighbouring dims (each detector keys on a
//! different smell -- see `touring-analysis/src/quality/f4_7_cicd.rs`
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

/// FF4_7 verifier -- CI/CD Pipeline.
#[allow(non_camel_case_types)]
pub struct F4_7_Cicd;

impl Verification for F4_7_Cicd {
    fn id(&self) -> DimId {
        DimId::F4_7
    }
    fn measure(&self, target: &Path) -> Result<(f32, String)> {
        analyze_f4_7_cicd_dim(target)
    }
}

// -- Real engine: CI/CD Pipeline detection -----------------------------------------
#[cfg(feature = "workspace-integration")]
fn analyze_f4_7_cicd_dim(target: &Path) -> Result<(f32, String)> {
    use touring_analysis::quality::{analyze_cicd, score_cicd};
    if is_detector_own_source(target) {
        return Ok((
            1.0,
            "FF4_7: detector own source (CI/CD Pipeline needle vocabulary embedded as data) -- score=1.000"
                .to_string(),
        ));
    }
    let (raw, present) = crate::verifications::read_artifact_source(
        target,
        crate::verifications::ArtifactClass::CiWorkflow,
    );
    if !present {
        return Ok(crate::verifications::absent_artifact_score_at(
            target,
            "F4.7",
            crate::verifications::ArtifactClass::CiWorkflow,
        ));
    }
    let lang = crate::verifications::lang_of(target);
    let r = analyze_cicd(&raw, lang);
    let value = score_cicd(&r);
    let top = crate::verifications::top_finding(&r.findings);
    // Where the score ceiling comes from: a bare "(1x)" leaves the author reading the
    // whole workflow to find the one job that holds the token.
    let at = r
        .primary()
        .map(|f| match f.job.as_deref() {
            Some(job) => format!("; at job `{job}` line {}", f.line),
            None => format!("; at workflow level line {}", f.line),
        })
        .unwrap_or_default();
    let evidence = format!(
        "FF4_7: {} CI/CD Pipeline smell(s) over {} lines ({lang}) -- score={value:.3}          (touring-analysis analyze_cicd: see header for detector catalog){top}{at}",
        r.violations, r.total_lines
    );
    Ok((value, evidence))
}

/// The engine and this verifier embed the CI/CD Pipeline needle vocabulary as
/// detection data, so scoring their own source is a self-match false
/// positive. Mirrors `f2_13_scalability::is_detector_own_source`.
#[cfg(feature = "workspace-integration")]
fn is_detector_own_source(target: &Path) -> bool {
    crate::verifications::is_detector_own_source(target)
}

// -- Standalone fallback: CI/CD Pipeline substring-density heuristic ---------------
#[cfg(not(feature = "workspace-integration"))]
fn analyze_f4_7_cicd_dim(target: &Path) -> Result<(f32, String)> {
    let raw = crate::verifications::read_target_source(target)?;
    let lines = raw.lines().count().max(1) as f32;
    let smells = raw.matches("uses: ").count()
        + raw.matches("permissions:").count()
        + raw.matches("cache:").count();
    let value = (1.0 - (smells as f32 / lines) * 6.0).clamp(0.0, 1.0);
    let evidence = format!(
        "{} CI/CD Pipeline smell(s) over {} lines (heuristic; build --features workspace-integration for full CI/CD Pipeline analysis)",
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
        let s = F4_7_Cicd.check(f.path()).expect("check");
        assert!(
            (0.0..=1.0).contains(&s.value),
            "score out of range: {}",
            s.value
        );
    }

    /// CI/CD Pipeline: the wrapper invokes the real engine and returns a valid score for
    /// a fixture file with the expected extension. The engine's own dirty-vs-clean
    /// semantic assertions live in `touring-analysis/src/quality/f4_7_cicd.rs::tests`.
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn test_engine_invoked_returns_valid_score() {
        let f = write_temp_ext(
            r#"name: ci
on: push
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - run: cargo test"#,
            ".yml",
        );
        let s = F4_7_Cicd.check(f.path()).expect("check");
        assert!(
            (0.0..=1.0).contains(&s.value),
            "wrapper score out of [0,1]: {}",
            s.value
        );
        // Engine is invoked: evidence mentions the touring-analysis path
        assert!(
            s.evidence.contains("touring-analysis analyze_cicd"),
            "evidence should reference real engine, got: {}",
            s.evidence
        );
    }

    /// 17/10/2026: a supply-chain finding caps the score and the evidence says WHERE.
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn supply_chain_finding_caps_the_score_and_names_its_job() {
        let workflow = format!(
            "name: ci\non:\n  pull_request_target:\npermissions:\n  contents: read\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: actions/checkout@692973e3d937129bcbf40652eb9f2f61becf3332\n        with:\n          ref: ${{{{ github.event.pull_request.head.sha }}}}\n          persist-credentials: false\n      - run: cargo test\n      - run: cargo clippy\n      - run: python3 docs/elite_aggregate.py --check\n{}",
            "# padding\n".repeat(600)
        );
        let f = write_temp_ext(&workflow, ".yml");
        let s = F4_7_Cicd.check(f.path()).expect("check");
        assert!(s.value <= 0.40, "critical ceiling, got {}", s.value);
        assert!(s.evidence.contains("pull_request_target"), "{}", s.evidence);
        assert!(
            s.evidence.contains("at job `build` line 10"),
            "{}",
            s.evidence
        );
        // A workflow-level grant has no job: the evidence says so.
        let oidc = write_temp_ext(
            "name: ci\non: push\npermissions:\n  contents: read\n  id-token: write\njobs:\n  t:\n    steps:\n      - run: cargo test\n      - run: cargo clippy\n      - run: python3 docs/elite_aggregate.py --check\n",
            ".yml",
        );
        let o = F4_7_Cicd.check(oidc.path()).expect("check");
        assert!(
            o.evidence.contains("at workflow level line 3"),
            "{}",
            o.evidence
        );
        assert!(o.value <= 0.70, "high ceiling, got {}", o.value);
        let clean = write_temp_ext(
            "name: ci\non: push\npermissions:\n  contents: read\njobs:\n  t:\n    steps:\n      - run: cargo test\n      - run: cargo clippy\n      - run: python3 docs/elite_aggregate.py --check\n      - uses: actions/cache@1bd1e32a3bdc45362d1e726936510720a7c30a57\n",
            ".yml",
        );
        let c = F4_7_Cicd.check(clean.path()).expect("check");
        assert!(!c.evidence.contains("; at "), "{}", c.evidence);
    }
}
