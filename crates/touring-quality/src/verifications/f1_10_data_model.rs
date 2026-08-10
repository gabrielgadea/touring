//! F1.10 — Data Model verifier (D10).
//!
//! **Real engine (default `workspace-integration`)**: delegates to
//! [`touring_analysis::quality::analyze_data_model`] — a polyglot detector of
//! "make illegal states unrepresentable" + primitive-obsession smells across
//! **7 languages**: stringly-typed domain fields (`status: String` instead of an
//! `enum`/newtype — C-NEWTYPE), type-erasure escapes (`any` / `interface{}` /
//! `Object<>` / `void*` / `Any`), and boolean-flag explosion (≥3 `bool` fields
//! in one struct → an `enum`/`bitflags`). It is disjoint from F1.9 api-design
//! (contract surface — `Result<_,String>`, getters, missing `Debug`), F1.11
//! design-patterns (GoF/ownership), F4.4 modernization, and F2.2 input-validation
//! by construction — F1.10 scores the *data shape*.
//!
//! This replaces a stub that scored `derive(` per `struct ` ratio. Comments and
//! `#[cfg(test)]`/test regions are excluded via `code_regions`.
//!
//! **Standalone fallback (`--no-default-features`)**: a labelled `: String`/
//! `: bool` density heuristic.
//!
//! **Scope**: per-file; rolls up as `AggKind::WeightedLoc`. ADVISORY-tier.

use crate::DimId;
use crate::verifications::Verification;
use anyhow::Result;
use std::path::Path;

/// F1.10 verifier — Data Model.
#[allow(non_camel_case_types)]
pub struct F1_10_DataModel;

impl Verification for F1_10_DataModel {
    fn id(&self) -> DimId {
        DimId::F1_10
    }

    fn measure(&self, target: &Path) -> Result<(f32, String)> {
        analyze_data_model_dim(target)
    }
}

// ── Real engine: polyglot data-model anti-pattern detection ───────────────────
#[cfg(feature = "workspace-integration")]
fn analyze_data_model_dim(target: &Path) -> Result<(f32, String)> {
    use touring_analysis::quality::{analyze_data_model, score_data_model};

    // The engine (`data_model.rs`) embeds every needle (`b"String"`, `b"bool"`,
    // `b"interface{}"`, `b"void*"`, the `DOMAIN_WORDS` table, …) as data, so
    // scanning its own source is a self-match false positive.
    if is_detector_own_source(target) {
        return Ok((
            1.0,
            "F1.10: detector own source (data-model needle vocabulary embedded as data) — score=1.000"
                .to_string(),
        ));
    }

    let raw = crate::verifications::read_target_source(target)?;
    let lang = crate::verifications::lang_from_ext(target);
    let r = analyze_data_model(&raw, lang);

    let value = score_data_model(&r);
    let top = crate::verifications::top_finding(&r.findings);
    let evidence = format!(
        "F1.10: {} data-model anti-pattern(s) over {} lines ({lang}) — score={value:.3} \
         (touring-analysis analyze_data_model: stringly-typed domain field / type-erasure / bool-flag explosion){top}",
        r.violations, r.total_lines
    );
    Ok((value, evidence))
}

/// The engine and this verifier embed the data-model needle vocabulary
/// (`String`, `bool`, `interface{}`, `void*`, the `DOMAIN_WORDS` table, …) as
/// detection data, so scoring their own source is a self-match false positive.
/// Mirrors `f1_5_tech_debt::is_detector_own_source` (test/bench dirs + the
/// quality engine + verifier dirs). Pure path logic.
#[cfg(feature = "workspace-integration")]
fn is_detector_own_source(target: &Path) -> bool {
    crate::verifications::is_detector_own_source(target)
}

// ── Standalone fallback: : String / : bool density heuristic ──────────────────
#[cfg(not(feature = "workspace-integration"))]
fn analyze_data_model_dim(target: &Path) -> Result<(f32, String)> {
    let raw = crate::verifications::read_target_source(target)?;
    let lines = raw.lines().count().max(1) as f32;
    let primitives = raw.matches(": String").count() + raw.matches(": bool").count();
    let value = (1.0 - (primitives as f32 / lines) * 3.0).clamp(0.0, 1.0);
    let evidence = format!(
        "{primitives} `: String`/`: bool` field(s) over {} lines (heuristic; build --features \
         workspace-integration for polyglot data-model analysis)",
        lines as usize
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
    fn test_data_model_returns_valid_score() {
        let f = write_temp_ext("struct S { status: Status }\n", ".rs");
        let s = F1_10_DataModel.check(f.path()).expect("check");
        assert!(
            (0.0..=1.0).contains(&s.value),
            "score out of range: {}",
            s.value
        );
    }

    #[test]
    fn test_data_model_empty_file() {
        let f = write_temp("");
        let s = F1_10_DataModel.check(f.path()).expect("check");
        assert!((0.0..=1.0).contains(&s.value));
    }

    /// Type-modelled struct → high score.
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn test_typed_struct_high() {
        let f = write_temp_ext(
            "struct S {\n    status: Status,\n    id: UserId,\n}\n",
            ".rs",
        );
        let s = F1_10_DataModel.check(f.path()).expect("check");
        assert!(
            s.value > 0.95,
            "type-modelled struct should be high, got {}",
            s.value
        );
    }

    /// **End-to-end vs stub**: a stringly-typed domain field scores below a
    /// type-modelled one — the stub (derive/struct ratio) was blind to it.
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn test_stringly_typed_scores_lower() {
        let bad = write_temp_ext(
            "struct S { status: String, kind: String, color: String }\n",
            ".rs",
        );
        let good = write_temp_ext(
            "struct S { status: Status, kind: Kind, color: Rgb }\n",
            ".rs",
        );
        let sb = F1_10_DataModel.check(bad.path()).expect("check");
        let sg = F1_10_DataModel.check(good.path()).expect("check");
        assert!(
            sb.value < sg.value,
            "stringly-typed ({}) < enum-typed ({})",
            sb.value,
            sg.value
        );
    }

    /// Polyglot: TypeScript `: any` type-erasure flagged; a union is not.
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn test_typescript_any_polyglot() {
        let bad = write_temp_ext("interface S {\n  data: any;\n}\n", ".ts");
        let good = write_temp_ext("interface S {\n  data: Payload;\n}\n", ".ts");
        let su = F1_10_DataModel.check(bad.path()).expect("check");
        let sg = F1_10_DataModel.check(good.path()).expect("check");
        assert!(
            su.value < sg.value,
            ": any ({}) must score below a concrete type ({})",
            su.value,
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
            "/x/touring-analysis/src/quality/data_model.rs"
        )));
        assert!(is_detector_own_source(Path::new(
            "/x/crates/foo/tests/bar.rs"
        )));
        assert!(!is_detector_own_source(Path::new(
            "/x/crates/touring-server/src/main.rs"
        )));
    }
}
