//! F1.9 — API Design verifier (D09).
//!
//! **Real engine (default `workspace-integration`)**: delegates to
//! [`touring_analysis::quality::analyze_api_design`] — a polyglot detector of
//! public-API **contract** smells across **7 languages**, approximating a
//! high-confidence subset of each language's API oracle: Rust API Guidelines
//! (`Result<_, String>` → C-GOOD-ERR, `get_` prefix → C-GETTER, `into_`/`as_`
//! → C-CONV, missing `Debug` → C-DEBUG, wide `new()` → C-BUILDER), PEP 8
//! (mutable defaults, broad `raise`), ESLint design (`throw "string"`, wide
//! params), Effective Go (`GetX()` getters, library `panic`), Java
//! encapsulation (`public` fields, broad `throws`), and Effective C++
//! (function-like macros). It is disjoint from F4.1 idioms (local *style*) by
//! construction — F1.9 scores the public *contract*.
//!
//! This replaces a stub that counted `pub fn`/`pub struct`/`pub trait` and
//! **penalised a wide public surface** — an anti-metric, since a large
//! well-designed API scored worse than a tiny badly-designed one. Comments and
//! `#[cfg(test)]`/test regions are excluded via `code_regions`.
//!
//! **Standalone fallback (`--no-default-features`)**: the prior pub-item count
//! heuristic, labelled.
//!
//! **Scope**: per-file (`AggKind::WeightedLoc`). A per-file scanner cannot
//! replace a type-aware API linter, so it is honest about catching a subset.

use crate::verifications::Verification;
use crate::{DimId, DimScore};
use anyhow::Result;
use std::path::Path;

/// F1.9 verifier — API Design.
#[allow(non_camel_case_types)]
pub struct F1_9_ApiDesign;

impl Verification for F1_9_ApiDesign {
    fn id(&self) -> DimId {
        DimId::F1_9
    }

    fn check(&self, target: &Path) -> Result<DimScore> {
        let (value, evidence) = analyze_api_design_dim(target)?;
        Ok(crate::verifications::finish(
            self.id(),
            value,
            evidence,
            target,
        ))
    }
}

// ── Real engine: polyglot public-API contract analysis ────────────────────────
#[cfg(feature = "workspace-integration")]
fn analyze_api_design_dim(target: &Path) -> Result<(f32, String)> {
    use touring_analysis::quality::{analyze_api_design, score_api_design};

    // The engine (`api_design.rs`) embeds every needle (`b"Result<"`,
    // `b"throws Exception"`, `b"#define "`, …) as data, so scanning its own
    // source is a self-match false positive.
    if is_detector_own_source(target) {
        return Ok((
            1.0,
            "F1.9: detector own source (API-smell needle vocabulary embedded as data) — score=1.000"
                .to_string(),
        ));
    }

    let raw = crate::verifications::read_target_source(target)?;
    let lang = crate::verifications::lang_from_ext(target);
    let r = analyze_api_design(&raw, lang);

    let value = score_api_design(&r);
    let top = r
        .findings
        .first()
        .map(|(m, c)| format!("; top: {m} ({c}x)"))
        .unwrap_or_default();
    let evidence = format!(
        "F1.9: {} public-API contract smell(s) over {} lines ({lang}) — score={value:.3} \
         (touring-analysis analyze_api_design: C-* / Effective Go / PEP 8 / Effective C++){top}",
        r.violations, r.total_lines
    );
    Ok((value, evidence))
}

/// The engine and this verifier embed the smell needle vocabulary as detection
/// data, so scoring their own source is a self-match false positive. Mirrors
/// `f1_5_tech_debt::is_detector_own_source` (test/bench dirs + the quality
/// engine + verifier dirs). Pure path logic.
#[cfg(feature = "workspace-integration")]
fn is_detector_own_source(target: &Path) -> bool {
    crate::verifications::is_detector_own_source(target)
}

// ── Standalone fallback: pub-item count heuristic (no API-design awareness) ────
#[cfg(not(feature = "workspace-integration"))]
fn analyze_api_design_dim(target: &Path) -> Result<(f32, String)> {
    let raw = crate::verifications::read_target_source(target)?;
    let pub_items = raw.matches("pub fn ").count()
        + raw.matches("pub struct ").count()
        + raw.matches("pub trait ").count();
    // Note: this is the legacy anti-metric (wide surface penalised); the real
    // engine replaces it under the default feature set.
    let value = if pub_items > 30 {
        0.0
    } else {
        1.0 - (pub_items as f32 / 30.0)
    };
    let evidence = format!(
        "{pub_items} public items (heuristic; build --features workspace-integration \
         for polyglot API-design analysis)"
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
    fn test_api_design_returns_valid_score() {
        let f = write_temp_ext("pub fn name(&self) -> &str { &self.name }\n", ".rs");
        let s = F1_9_ApiDesign.check(f.path()).expect("check");
        assert!(
            (0.0..=1.0).contains(&s.value),
            "score out of range: {}",
            s.value
        );
    }

    #[test]
    fn test_api_design_empty_file() {
        let f = write_temp("");
        let s = F1_9_ApiDesign.check(f.path()).expect("check");
        assert!((0.0..=1.0).contains(&s.value));
    }

    /// Idiomatic API → high score.
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn test_idiomatic_api_high() {
        let f = write_temp_ext(
            "#[derive(Debug)]\npub struct Config {\n    name: String,\n}\nimpl Config {\n    pub fn name(&self) -> &str { &self.name }\n}\n",
            ".rs",
        );
        let s = F1_9_ApiDesign.check(f.path()).expect("check");
        assert!(
            s.value > 0.95,
            "idiomatic api should be high, got {}",
            s.value
        );
    }

    /// **End-to-end vs stub**: a bad contract (`Result<_, String>`, `get_`
    /// prefix) scores below idiomatic — the stub (counting pub items) would
    /// score the *smaller* surface here as better regardless of design.
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn test_bad_contract_scores_lower() {
        let bad = write_temp_ext(
            "pub fn get_thing() -> Result<(), String> { Ok(()) }\n",
            ".rs",
        );
        let good = write_temp_ext(
            "#[derive(Debug)]\npub struct S;\nimpl S {\n    pub fn thing(&self) -> Result<(), MyError> { Ok(()) }\n}\n",
            ".rs",
        );
        let sb = F1_9_ApiDesign.check(bad.path()).expect("check");
        let sg = F1_9_ApiDesign.check(good.path()).expect("check");
        assert!(
            sb.value < sg.value,
            "bad contract ({}) < good ({})",
            sb.value,
            sg.value
        );
    }

    /// Polyglot: Python mutable-default + broad `raise` flagged via `.py`.
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn test_python_contract_flagged() {
        let f = write_temp_ext("def f(x=[]):\n    raise Exception('boom')\n", ".py");
        let s = F1_9_ApiDesign.check(f.path()).expect("check");
        assert!(
            s.value < 1.0,
            "python API smells should lower the score, got {}",
            s.value
        );
    }

    /// Polyglot: Go `GetX()` getter prefix flagged via `.go`.
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn test_go_getter_polyglot() {
        let getter = write_temp_ext("func (r *R) GetOwner() string { return r.owner }\n", ".go");
        let idiomatic = write_temp_ext("func (r *R) Owner() string { return r.owner }\n", ".go");
        let sg = F1_9_ApiDesign.check(getter.path()).expect("check");
        let si = F1_9_ApiDesign.check(idiomatic.path()).expect("check");
        assert!(
            sg.value < si.value,
            "GetOwner ({}) must score below Owner ({})",
            sg.value,
            si.value
        );
        assert!(
            (si.value - 1.0).abs() < 1e-6,
            "idiomatic Go getter is clean, got {}",
            si.value
        );
    }

    /// The engine's own source (which embeds every needle as data) must be
    /// allowlisted, not self-matched.
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn test_detector_own_source_allowlisted() {
        use std::path::Path;
        assert!(is_detector_own_source(Path::new(
            "/x/touring-analysis/src/quality/api_design.rs"
        )));
        assert!(is_detector_own_source(Path::new(
            "/x/crates/foo/tests/bar.rs"
        )));
        assert!(!is_detector_own_source(Path::new(
            "/x/crates/touring-server/src/main.rs"
        )));
    }
}
