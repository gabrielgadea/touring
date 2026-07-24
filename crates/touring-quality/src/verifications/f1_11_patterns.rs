//! F1.11 — Design Patterns verifier (D11).
//!
//! **Real engine (default `workspace-integration`)**: delegates to
//! [`touring_analysis::quality::analyze_design_patterns`] — a polyglot detector
//! of design **anti-patterns** across **7 languages**: GoF transplants
//! (Rust `static mut` Singleton; `getInstance`/`FactoryFactory`/`Cloneable`/
//! `extends Thread` in Java; `getInstance` in TS/C++), ownership smells
//! (`Rc<RefCell<`, `unsafe impl Send/Sync`), type-erasure escape hatches
//! (`.downcast`/`dyn Any`, `dynamic_cast`, `as unknown as`), reflection
//! (`reflect.`), and hidden global state (`func init()`, Python `global`,
//! `__new__`). It is disjoint from F4.1 idioms (local style), F1.9 api-design
//! (public contract), and F4.4 modernization (version adoption) — F1.11 scores
//! the *structural pattern* choice. `transmute` (owned by the antipattern
//! detector) and `impl Deref for` (high overlap with legitimate smart-pointer
//! `Deref`) are intentionally not flagged here.
//!
//! This replaces a stub that scored the `impl`/`trait` ratio — unrelated to
//! pattern quality. Comments and `#[cfg(test)]`/test regions are excluded via
//! `code_regions`.
//!
//! **Standalone fallback (`--no-default-features`)**: the prior `impl`/`trait`
//! ratio heuristic, labelled.
//!
//! **Scope**: per-file (`AggKind::WeightedLoc`). A per-file scanner cannot judge
//! "missing abstraction" / "over-abstraction", so it is honest about catching a
//! high-confidence subset.

use crate::verifications::Verification;
use crate::{DimId, DimScore};
use anyhow::Result;
use std::path::Path;

/// F1.11 verifier — Design Patterns.
#[allow(non_camel_case_types)]
pub struct F1_11_Patterns;

impl Verification for F1_11_Patterns {
    fn id(&self) -> DimId {
        DimId::F1_11
    }

    fn check(&self, target: &Path) -> Result<DimScore> {
        let (value, evidence) = analyze_patterns_dim(target)?;
        Ok(crate::verifications::finish(
            self.id(),
            value,
            evidence,
            target,
        ))
    }
}

// ── Real engine: polyglot design-pattern anti-pattern detection ───────────────
#[cfg(feature = "workspace-integration")]
fn analyze_patterns_dim(target: &Path) -> Result<(f32, String)> {
    use touring_analysis::quality::{analyze_design_patterns, score_design_patterns};

    // The engine (`design_patterns.rs`) embeds every needle (`b"static mut "`,
    // `b"getInstance("`, `b"dynamic_cast"`, …) as data, so scanning its own
    // source is a self-match false positive.
    if is_detector_own_source(target) {
        return Ok((
            1.0,
            "F1.11: detector own source (anti-pattern needle vocabulary embedded as data) — score=1.000"
                .to_string(),
        ));
    }

    let raw = crate::verifications::read_target_source(target)?;
    let lang = crate::verifications::lang_from_ext(target);
    let r = analyze_design_patterns(&raw, lang);

    let value = score_design_patterns(&r);
    let top = r
        .findings
        .first()
        .map(|(m, c)| format!("; top: {m} ({c}x)"))
        .unwrap_or_default();
    let evidence = format!(
        "F1.11: {} design anti-pattern(s) over {} lines ({lang}) — score={value:.3} \
         (touring-analysis analyze_design_patterns: GoF transplants / ownership / type-erasure){top}",
        r.violations, r.total_lines
    );
    Ok((value, evidence))
}

/// The engine and this verifier embed the anti-pattern needle vocabulary as
/// detection data, so scoring their own source is a self-match false positive.
/// Mirrors `f1_5_tech_debt::is_detector_own_source` (test/bench dirs + the
/// quality engine + verifier dirs). Pure path logic.
#[cfg(feature = "workspace-integration")]
fn is_detector_own_source(target: &Path) -> bool {
    crate::verifications::is_detector_own_source(target)
}

// ── Standalone fallback: impl/trait ratio heuristic (no pattern awareness) ─────
#[cfg(not(feature = "workspace-integration"))]
fn analyze_patterns_dim(target: &Path) -> Result<(f32, String)> {
    let raw = crate::verifications::read_target_source(target)?;
    let trait_defs = raw.matches("trait ").count();
    let trait_impls = raw.matches("impl ").count();
    let value = if trait_defs == 0 {
        1.0
    } else {
        (trait_impls as f32 / trait_defs as f32).min(1.0)
    };
    let evidence = format!(
        "{trait_defs} trait defs / {trait_impls} impls (heuristic; build --features \
         workspace-integration for polyglot design-pattern analysis)"
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
    fn test_patterns_returns_valid_score() {
        let f = write_temp_ext("enum State { A, B }\n", ".rs");
        let s = F1_11_Patterns.check(f.path()).expect("check");
        assert!(
            (0.0..=1.0).contains(&s.value),
            "score out of range: {}",
            s.value
        );
    }

    #[test]
    fn test_patterns_empty_file() {
        let f = write_temp("");
        let s = F1_11_Patterns.check(f.path()).expect("check");
        assert!((0.0..=1.0).contains(&s.value));
    }

    /// Idiomatic patterns → high score.
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn test_idiomatic_patterns_high() {
        let f = write_temp_ext(
            "use std::sync::OnceLock;\nstruct Miles(f64);\nenum State { A, B }\n",
            ".rs",
        );
        let s = F1_11_Patterns.check(f.path()).expect("check");
        assert!(
            s.value > 0.95,
            "idiomatic patterns should be high, got {}",
            s.value
        );
    }

    /// **End-to-end vs stub**: an anti-pattern (`static mut` Singleton) scores
    /// below idiomatic — the stub (impl/trait ratio) was blind to it.
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn test_antipattern_scores_lower() {
        let bad = write_temp_ext(
            "static mut GLOBAL: u32 = 0;\nlet x: Rc<RefCell<T>> = make();\n",
            ".rs",
        );
        let good = write_temp_ext(
            "use std::sync::OnceLock;\nstatic G: OnceLock<u32> = OnceLock::new();\n",
            ".rs",
        );
        let sb = F1_11_Patterns.check(bad.path()).expect("check");
        let sg = F1_11_Patterns.check(good.path()).expect("check");
        assert!(
            sb.value < sg.value,
            "anti-pattern ({}) < idiomatic ({})",
            sb.value,
            sg.value
        );
    }

    /// Polyglot: Java GoF smells flagged via `.java`.
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn test_java_gof_polyglot() {
        let bad = write_temp_ext(
            "class T extends Thread implements Cloneable {\n    static T getInstance() { return null; }\n}\n",
            ".java",
        );
        let good = write_temp_ext(
            "class T implements Runnable {\n    public void run() {}\n}\n",
            ".java",
        );
        let sb = F1_11_Patterns.check(bad.path()).expect("check");
        let sg = F1_11_Patterns.check(good.path()).expect("check");
        assert!(
            sb.value < sg.value,
            "GoF-heavy java ({}) must score below idiomatic ({})",
            sb.value,
            sg.value
        );
        assert!(
            (sg.value - 1.0).abs() < 1e-6,
            "Runnable impl is clean, got {}",
            sg.value
        );
    }

    /// The engine's own source (which embeds every needle as data) must be
    /// allowlisted, not self-matched.
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn test_detector_own_source_allowlisted() {
        use std::path::Path;
        assert!(is_detector_own_source(Path::new(
            "/x/touring-analysis/src/quality/design_patterns.rs"
        )));
        assert!(is_detector_own_source(Path::new(
            "/x/crates/foo/tests/bar.rs"
        )));
        assert!(!is_detector_own_source(Path::new(
            "/x/crates/touring-server/src/main.rs"
        )));
    }
}
