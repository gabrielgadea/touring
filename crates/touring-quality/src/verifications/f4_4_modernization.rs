//! F4.4 — Modernization verifier (D43).
//!
//! **Real engine (default `workspace-integration`)**: delegates to
//! [`touring_analysis::quality::analyze_modernization`] — a polyglot detector
//! of **superseded constructs** across **7 languages**, version-anchored to the
//! newer feature that replaces each: Rust edition 2018 / 1.80 (`try!`→`?`,
//! `extern crate`→paths, `#[macro_use]`→`use`, `lazy_static!`→`LazyLock`),
//! Python 3 (`super(Cls, self)`→`super()`, redundant `object` base), ESM/ES6+
//! (`require`→`import`, `module.exports`→`export`, `Object.assign`→spread,
//! `indexOf(..) !== -1`→`includes`), Go 1.16/1.20 (`ioutil`→`io`/`os`,
//! `rand.Seed`), Java 8/16 (anonymous functional class→lambda,
//! `Collectors.toList()`→`.toList()`), and C++ (`std::bind`→lambda, C
//! headers→C++ headers). It is **disjoint from F4.1 idioms by construction**:
//! idioms scores per-version *style*, modernization scores *version adoption*
//! (so `var`/`interface{}`/`typedef`/`NULL`, which idioms owns, are not
//! repeated here).
//!
//! This replaces a stub that counted `try!` + `extern crate` only and returned
//! a flat `0.5`/`1.0`. Comments and `#[cfg(test)]`/test regions are excluded via
//! `code_regions`.
//!
//! **Standalone fallback (`--no-default-features`)**: the prior
//! `try!`+`extern crate` count heuristic, labelled.
//!
//! **Scope**: per-file (`AggKind::WeightedLoc`). A per-file scanner cannot
//! replace `cargo fix --edition` / a codemod, so it is honest about catching a
//! subset.

use crate::DimId;
use crate::verifications::Verification;
use anyhow::Result;
use std::path::Path;

/// F4.4 verifier — Modernization.
#[allow(non_camel_case_types)]
pub struct F4_4_Modernization;

impl Verification for F4_4_Modernization {
    fn id(&self) -> DimId {
        DimId::F4_4
    }

    fn measure(&self, target: &Path) -> Result<(f32, String)> {
        analyze_modernization_dim(target)
    }
}

// ── Real engine: polyglot modernization detection ─────────────────────────────
#[cfg(feature = "workspace-integration")]
fn analyze_modernization_dim(target: &Path) -> Result<(f32, String)> {
    use touring_analysis::quality::{analyze_modernization, score_modernization};

    // The engine (`modernization.rs`) embeds every needle (`b"try!("`,
    // `b"lazy_static!"`, `b"ioutil."`, …) as data, so scanning its own source
    // is a self-match false positive.
    if is_detector_own_source(target) {
        return Ok((
            1.0,
            "F4.4: detector own source (modernization needle vocabulary embedded as data) — score=1.000"
                .to_string(),
        ));
    }

    let raw = crate::verifications::read_target_source(target)?;
    let lang = crate::verifications::lang_from_ext(target);
    let r = analyze_modernization(&raw, lang);

    let value = score_modernization(&r);
    let top = crate::verifications::top_finding(&r.findings);
    let evidence = format!(
        "F4.4: {} modernization opportunit(ies) over {} lines ({lang}) — score={value:.3} \
         (touring-analysis analyze_modernization: edition/version migrations){top}",
        r.violations, r.total_lines
    );
    Ok((value, evidence))
}

/// The engine and this verifier embed the modernization needle vocabulary as
/// detection data, so scoring their own source is a self-match false positive.
/// Mirrors `f1_5_tech_debt::is_detector_own_source` (test/bench dirs + the
/// quality engine + verifier dirs). Pure path logic.
#[cfg(feature = "workspace-integration")]
fn is_detector_own_source(target: &Path) -> bool {
    crate::verifications::is_detector_own_source(target)
}

// ── Standalone fallback: try!/extern-crate count heuristic ─────────────────────
#[cfg(not(feature = "workspace-integration"))]
fn analyze_modernization_dim(target: &Path) -> Result<(f32, String)> {
    let raw = crate::verifications::read_target_source(target)?;
    let old = raw.matches("try!").count() + raw.matches("extern crate").count();
    let value = if old == 0 { 1.0 } else { 0.5 };
    let evidence = format!(
        "{old} legacy construct(s) (heuristic; build --features workspace-integration \
         for polyglot modernization analysis)"
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
    fn test_modernization_returns_valid_score() {
        let f = write_temp_ext("fn example() {}\n", ".rs");
        let s = F4_4_Modernization.check(f.path()).expect("check");
        assert!(
            (0.0..=1.0).contains(&s.value),
            "score out of range: {}",
            s.value
        );
    }

    #[test]
    fn test_modernization_empty_file() {
        let f = write_temp("");
        let s = F4_4_Modernization.check(f.path()).expect("check");
        assert!((0.0..=1.0).contains(&s.value));
    }

    /// Modern code → high score.
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn test_modern_rust_high() {
        let f = write_temp_ext(
            "use std::sync::LazyLock;\nfn f() -> Result<(), E> {\n    let x = g()?;\n    Ok(())\n}\n",
            ".rs",
        );
        let s = F4_4_Modernization.check(f.path()).expect("check");
        assert!(
            s.value > 0.95,
            "modern rust should be high, got {}",
            s.value
        );
    }

    /// **End-to-end vs stub**: legacy rust (`try!`, `lazy_static!`, `#[macro_use]`)
    /// scores below modern — the stub only knew `try!`/`extern crate`.
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn test_legacy_rust_lower() {
        let bad = write_temp_ext(
            "#[macro_use]\nextern crate serde;\nlazy_static! { static ref X: u8 = 1; }\n",
            ".rs",
        );
        let good = write_temp_ext("use std::sync::LazyLock;\nfn f() {}\n", ".rs");
        let sb = F4_4_Modernization.check(bad.path()).expect("check");
        let sg = F4_4_Modernization.check(good.path()).expect("check");
        assert!(
            sb.value < sg.value,
            "legacy ({}) < modern ({})",
            sb.value,
            sg.value
        );
    }

    /// Polyglot: Python `super(Cls, self)` flagged via `.py`.
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn test_python_super_polyglot() {
        let legacy = write_temp_ext(
            "class C(B):\n    def __init__(self):\n        super(C, self).__init__()\n",
            ".py",
        );
        let modern = write_temp_ext(
            "class C(B):\n    def __init__(self):\n        super().__init__()\n",
            ".py",
        );
        let sl = F4_4_Modernization.check(legacy.path()).expect("check");
        let sm = F4_4_Modernization.check(modern.path()).expect("check");
        assert!(
            sl.value < sm.value,
            "super(C, self) ({}) must score below super() ({})",
            sl.value,
            sm.value
        );
        assert!(
            (sm.value - 1.0).abs() < 1e-6,
            "zero-arg super is modern, got {}",
            sm.value
        );
    }

    /// Polyglot: Go `ioutil.` flagged via `.go`.
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn test_go_ioutil_polyglot() {
        let legacy = write_temp_ext("data, _ := ioutil.ReadFile(p)\n", ".go");
        let modern = write_temp_ext("data, _ := os.ReadFile(p)\n", ".go");
        let sl = F4_4_Modernization.check(legacy.path()).expect("check");
        let sm = F4_4_Modernization.check(modern.path()).expect("check");
        assert!(
            sl.value < sm.value,
            "ioutil ({}) must score below os.ReadFile ({})",
            sl.value,
            sm.value
        );
    }

    /// The engine's own source (which embeds every needle as data) must be
    /// allowlisted, not self-matched.
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn test_detector_own_source_allowlisted() {
        use std::path::Path;
        assert!(is_detector_own_source(Path::new(
            "/x/touring-analysis/src/quality/modernization.rs"
        )));
        assert!(is_detector_own_source(Path::new(
            "/x/crates/foo/tests/bar.rs"
        )));
        assert!(!is_detector_own_source(Path::new(
            "/x/crates/touring-server/src/main.rs"
        )));
    }
}
