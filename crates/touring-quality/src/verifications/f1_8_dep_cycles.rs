//! F1.8 — Dependency Management verifier (D08).
//!
//! **Real engine (default `workspace-integration`)**: delegates to
//! [`touring_analysis::ModuleCycleAnalyzer`] — a hermetic Kosaraju SCC over the
//! crate's own `use crate::<top-level>` module-import graph (the same algorithm
//! as `detect_import_cycles`, but built from the target's source tree rather
//! than the daemon's `wiring_map`, so it is correct for ANY target crate with
//! no cross-project staleness). A reported SCC means two top-level module
//! subtrees genuinely `use crate::` each other.
//!
//! **Honest fallback (non-crate target / `--no-default-features`)**: the local
//! dependency-hygiene smells visible in a single buffer (`extern crate`, deep
//! `super::super::super::` relative re-imports). This NEVER claims acyclicity —
//! acyclicity is a crate-scoped property and is only asserted when the target
//! resolves to a crate.
//!
//! W0 (2026-06-21) stopped the original stub from penalising healthy
//! `use crate::` imports; W12 (2026-06-22) adds the real crate-scoped engine.
//!
//! **Scope**: `AggKind::ScopeNative` — the cycle graph is computed once on the
//! crate, never folded per-file.

use crate::DimId;
use crate::verifications::{Verification, strip_rust_comments_and_strings};
use anyhow::Result;
use std::path::Path;

/// F1.8 verifier — Dependency Management (module-import acyclicity).
#[allow(non_camel_case_types)]
pub struct F1_8_DepCycles;

impl Verification for F1_8_DepCycles {
    fn id(&self) -> DimId {
        DimId::F1_8
    }

    fn measure(&self, target: &Path) -> Result<(f32, String)> {
        analyze_dep_cycles(target)
    }
}

// ── Real engine: hermetic Kosaraju SCC on the crate's module graph ────────────
#[cfg(feature = "workspace-integration")]
fn analyze_dep_cycles(target: &Path) -> Result<(f32, String)> {
    use touring_analysis::ModuleCycleAnalyzer;
    let report = ModuleCycleAnalyzer::new().analyze(target);

    // Not part of a crate → fall back to the local hygiene proxy (never claim
    // acyclicity from a buffer that has no module graph).
    if !report.is_crate_scoped() {
        let (v, e) = local_hygiene(target)?;
        return Ok((v, format!("{e} [not crate-scoped — local hygiene only]")));
    }

    let cycles = report.cycle_count();
    // 0 cycles → 1.0; each cycle is a real architectural defect (D08: 1-2
    // shallow → warn, ≥3 → fail). ADVISORY dim, so a cycle lowers but does not
    // hard-block.
    let value = if cycles == 0 {
        1.0
    } else {
        (0.8 - cycles as f32 * 0.15).max(0.0)
    };
    let evidence = if cycles == 0 {
        format!(
            "F1.8: 0 module-import cycles across {} top-level modules \
             (Kosaraju SCC, hermetic) — acyclic",
            report.modules_analyzed
        )
    } else {
        let sample = report
            .cycles
            .iter()
            .take(3)
            .map(|c| c.join("↔"))
            .collect::<Vec<_>>()
            .join("; ");
        format!(
            "F1.8: {cycles} module-import cycle(s) across {} modules [{sample}] — score={value:.3}",
            report.modules_analyzed
        )
    };
    Ok((value, evidence))
}

// ── Standalone fallback: local dependency-hygiene smells ──────────────────────
#[cfg(not(feature = "workspace-integration"))]
fn analyze_dep_cycles(target: &Path) -> Result<(f32, String)> {
    let (v, e) = local_hygiene(target)?;
    Ok((
        v,
        format!(
            "{e} (local hygiene; build --features workspace-integration for hermetic Kosaraju cycle detection)"
        ),
    ))
}

/// Locally-visible dependency-hygiene smells: legacy `extern crate` and a
/// three-level `super::super::super::` relative re-import (a coupling-direction
/// smell). Plain `use crate::…` / `mod …` are healthy and NOT penalised.
/// Floor at 0.5 — warn-level only; a real cycle FAIL comes from the crate-scoped
/// engine. Needles split via `concat!` so the verifier never matches its own
/// source. Comments and string literals are stripped before counting, so doc
/// comments that merely mention the needle are not penalised.
fn local_hygiene(target: &Path) -> Result<(f32, String)> {
    let raw = crate::verifications::read_target_source(target)?;
    let code_only = strip_rust_comments_and_strings(&raw);
    let legacy_extern = code_only.matches(concat!("extern ", "crate ")).count();
    let deep_relative = code_only
        .matches(concat!("super::", "super::", "super::"))
        .count();
    let smells = legacy_extern + deep_relative;
    let value = (1.0 - smells as f32 * 0.1).max(0.5);
    let evidence = format!(
        "dependency hygiene: legacy extern-crate={legacy_extern}, deep relative re-imports={deep_relative}; \
         acyclicity is crate-scoped; score={value:.3}"
    );
    Ok((value, evidence))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn write_temp(content: &str) -> NamedTempFile {
        let mut f = NamedTempFile::new().expect("create temp");
        f.write_all(content.as_bytes()).expect("write");
        f
    }

    #[test]
    fn test_dep_cycles_returns_valid_score() {
        let f = write_temp("fn example() {}\n");
        let s = F1_8_DepCycles.check(f.path()).expect("check");
        assert!(
            (0.0..=1.0).contains(&s.value),
            "score out of range: {}",
            s.value
        );
    }

    #[test]
    fn test_dep_cycles_empty_file() {
        let f = write_temp("");
        let s = F1_8_DepCycles.check(f.path()).expect("check");
        assert!((0.0..=1.0).contains(&s.value));
    }

    /// A loose file (no crate) with many healthy `use crate::` imports must
    /// score 1.0 via the hygiene fallback — the old stub penalised this to 0.0.
    #[test]
    fn test_healthy_imports_are_not_penalised() {
        let content = "use crate::a;\nuse crate::b;\nuse crate::c;\nuse crate::d;\n\
                       use crate::e;\nuse crate::f;\nuse crate::g;\nmod x;\nmod y;\nmod z;\n";
        let f = write_temp(content);
        let s = F1_8_DepCycles.check(f.path()).expect("check");
        assert_eq!(
            s.value, 1.0,
            "healthy intra-crate imports must NOT be penalised, got {}",
            s.value
        );
    }

    /// Real dependency-hygiene smells lower the score (loose-file fallback).
    #[test]
    fn test_legacy_extern_crate_is_penalised() {
        let content = concat!("extern ", "crate libc;\n", "fn x() {}\n");
        let f = write_temp(content);
        let s = F1_8_DepCycles.check(f.path()).expect("check");
        assert!(
            s.value < 1.0,
            "legacy extern-crate must be penalised, got {}",
            s.value
        );
    }

    // ── Real-engine (crate-scoped) fixtures ───────────────────────────────────

    /// Build a temp crate with `Cargo.toml` + `src/<name>.rs` files.
    #[cfg(feature = "workspace-integration")]
    fn make_crate(files: &[(&str, &str)]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tmpdir");
        std::fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname=\"c\"\nversion=\"0.1.0\"\n",
        )
        .expect("manifest");
        let src = dir.path().join("src");
        std::fs::create_dir_all(&src).expect("src");
        for (name, content) in files {
            std::fs::write(src.join(name), content).expect("file");
        }
        dir
    }

    #[cfg(feature = "workspace-integration")]
    #[test]
    fn test_acyclic_crate_scores_perfect() {
        let dir = make_crate(&[
            ("lib.rs", "pub mod a;\npub mod b;\n"),
            ("a.rs", "use crate::b;\npub fn fa() {}\n"),
            ("b.rs", "pub fn fb() {}\n"),
        ]);
        let s = F1_8_DepCycles.check(dir.path()).expect("check");
        assert_eq!(
            s.value, 1.0,
            "acyclic crate must score 1.0, got {}",
            s.value
        );
        assert!(s.evidence.contains("acyclic"));
    }

    #[cfg(feature = "workspace-integration")]
    #[test]
    fn test_cyclic_crate_is_penalised() {
        let dir = make_crate(&[
            ("lib.rs", "pub mod a;\npub mod b;\n"),
            ("a.rs", "use crate::b;\npub fn fa() {}\n"),
            ("b.rs", "use crate::a;\npub fn fb() {}\n"),
        ]);
        let s = F1_8_DepCycles.check(dir.path()).expect("check");
        assert!(
            s.value < 1.0,
            "a↔b module cycle must lower the score, got {}",
            s.value
        );
        assert!(s.evidence.contains("cycle"));
    }
}
