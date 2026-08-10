//! F3.10 — Architecture Documentation verifier (D36).
//!
//! **Real engine (default `workspace-integration`)**: delegates to
//! [`touring_analysis::quality::analyze_arch_doc`] — a detector of the canonical
//! "missing architecture documentation" smell:
//!
//! | Detector | Signal | File kind |
//! |----------|--------|-----------|
//! | `missing-madr-status` | no `## Status` heading | `.md` (ADR / arch overview) |
//! | `missing-madr-context` | no `## Context` heading | `.md` (ADR / arch overview) |
//! | `missing-madr-decision` | no `## Decision` heading | `.md` (ADR / arch overview) |
//! | `missing-madr-consequences` | no `## Consequences` heading | `.md` (ADR / arch overview) |
//! | `no-mermaid-diagram` | no ` ```mermaid ` code block in the file | any |
//!
//! Disjoint from F3.8 inline doc (which keys on `///` proximity on source
//! items — F3.10 keys on ` ```mermaid ` blocks + MADR sections in `.md`);
//! F3.11 README completeness (which keys on the README's high-level
//! sections — F3.10 keys on ADR-specific MADR sections); F3.13 changelog
//! format (which keys on `[Unreleased]` + versioned sections — F3.10
//! keys on Status/Context/Decision/Consequences MADR sections).
//!
//! **Sources (context7, `/mermaid-js/mermaid`, High reputation, bench 91.75)**:
//! Mermaid is the canonical architecture-diagram language for Markdown
//! (embedded as ` ```mermaid ` fenced code blocks). The MADR template
//! prescribes four required H2 sections — Status, Context, Decision,
//! Consequences — and ADRs without them can't be reasoned about later.
//!
//! **Standalone fallback (`--no-default-features`)**: a labelled
//! ` ```mermaid ` / `## Status` density heuristic.
//!
//! **Scope**: per-file; rolls up as `AggKind::WeightedLoc`. ADVISORY-tier.

use crate::DimId;
use crate::verifications::Verification;
use anyhow::Result;
use std::path::Path;

/// F3.10 verifier — Architecture Documentation.
#[allow(non_camel_case_types)]
pub struct F3_10_ArchDoc;

impl Verification for F3_10_ArchDoc {
    fn id(&self) -> DimId {
        DimId::F3_10
    }
    fn measure(&self, target: &Path) -> Result<(f32, String)> {
        analyze_arch_doc_dim(target)
    }
}

// ── Real engine: architecture documentation ────────────────────────────────
#[cfg(feature = "workspace-integration")]
fn analyze_arch_doc_dim(target: &Path) -> Result<(f32, String)> {
    use touring_analysis::quality::{analyze_arch_doc, score_arch_doc};
    if is_detector_own_source(target) {
        return Ok((
            1.0,
            "F3.10: detector own source (arch_doc needle vocabulary embedded as data) — score=1.000"
                .to_string(),
        ));
    }
    let (raw, present) = crate::verifications::read_artifact_source(
        target,
        crate::verifications::ArtifactClass::ArchDoc,
    );
    if !present {
        return Ok(crate::verifications::absent_artifact_score(
            "F3.10",
            crate::verifications::ArtifactClass::ArchDoc,
        ));
    }
    let lang = crate::verifications::lang_from_ext(target);
    let r = analyze_arch_doc(&raw, lang);
    let value = score_arch_doc(&r);
    let top = crate::verifications::top_finding(&r.findings);
    let evidence = format!(
        "F3.10: {} arch-doc gap(s) over {} lines ({lang}) — score={value:.3} \
         (touring-analysis analyze_arch_doc: missing-madr-status / missing-madr-context / \
         missing-madr-decision / missing-madr-consequences / no-mermaid-diagram){top}",
        r.violations, r.total_lines
    );
    Ok((value, evidence))
}

/// The engine and this verifier embed the arch-doc needle vocabulary as
/// detection data, so scoring their own source is a self-match false
/// positive. Mirrors `f2_8_memory::is_detector_own_source`.
#[cfg(feature = "workspace-integration")]
fn is_detector_own_source(target: &Path) -> bool {
    crate::verifications::is_detector_own_source(target)
}

// ── Standalone fallback: mermaid / MADR density heuristic ───────────────────
#[cfg(not(feature = "workspace-integration"))]
fn analyze_arch_doc_dim(target: &Path) -> Result<(f32, String)> {
    let raw = crate::verifications::read_target_source(target)?;
    let lines = raw.lines().count().max(1) as f32;
    let mermaid = raw.matches("```mermaid").count();
    let status = raw.contains("## Status");
    let context = raw.contains("## Context");
    let decision = raw.contains("## Decision");
    let consequences = raw.contains("## Consequences");
    let madr_count = (status as u8 + context as u8 + decision as u8 + consequences as u8) as f32;
    let value = ((mermaid as f32 / 1.0).min(1.0) + madr_count / 4.0) / 2.0;
    let evidence = format!(
        "{mermaid} ```mermaid block(s) / {madr_count:.0}/4 MADR sections present over {lines:.0} lines \
         (heuristic; build --features workspace-integration for full arch-doc analysis)"
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

    fn write_temp(content: &str) -> NamedTempFile {
        let mut f = NamedTempFile::new().expect("create temp");
        f.write_all(content.as_bytes()).expect("write");
        f
    }

    #[test]
    fn test_arch_doc_returns_valid_score() {
        let f = write_temp_ext("# ADR-001\n\n## Status\naccepted\n", ".md");
        let s = F3_10_ArchDoc.check(f.path()).expect("check");
        assert!(
            (0.0..=1.0).contains(&s.value),
            "score out of range: {}",
            s.value
        );
    }

    #[test]
    fn test_arch_doc_empty_file() {
        let f = write_temp("");
        let s = F3_10_ArchDoc.check(f.path()).expect("check");
        assert!((0.0..=1.0).contains(&s.value));
    }

    /// Canonical ADR scores higher than a single-section stub.
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn test_canonical_scores_higher_than_stub() {
        let canonical = "# ADR\n\n## Status\ns\n\n## Context\nc\n\n## Decision\nd\n\n## Consequences\ncc\n\n```mermaid\ngraph TD\nA-->B\n```\n";
        let stub = "# ADR\n\n## Status\ns\n";
        let f_canonical = write_temp_ext(canonical, ".md");
        let f_stub = write_temp_ext(stub, ".md");
        let sc = F3_10_ArchDoc.check(f_canonical.path()).expect("check");
        let ss = F3_10_ArchDoc.check(f_stub.path()).expect("check");
        assert!(
            sc.value > ss.value,
            "canonical ({}) must score above stub ({})",
            sc.value,
            ss.value
        );
    }

    /// The engine's own source (which embeds every needle as data) must be
    /// allowlisted, not self-matched.
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn test_detector_own_source_allowlisted() {
        use std::path::Path;
        assert!(is_detector_own_source(Path::new(
            "/x/touring-analysis/src/quality/arch_doc.rs"
        )));
        assert!(is_detector_own_source(Path::new(
            "/x/crates/foo/tests/bar.rs"
        )));
        assert!(!is_detector_own_source(Path::new(
            "/x/crates/touring-server/src/main.rs"
        )));
    }
}
