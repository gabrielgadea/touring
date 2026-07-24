//! F2.2 — Input Validation verifier (D15).
//!
//! **Real engine (default `workspace-integration`)**: delegates to
//! [`touring_analysis::quality::analyze_input_validation`] — a polyglot
//! detector of boundary-validation security anti-patterns across **7
//! languages**: blocklist sanitization (`.replace("../"`, CWE-22), insecure
//! deserialization (`pickle.loads`/`yaml.load`/`ObjectInputStream`/`readObject`,
//! CWE-502), unbounded input (`gets`/`strcpy`/`scanf("%s"`, CWE-242/120), and
//! auto-escaping bypasses (`dangerouslySetInnerHTML`, `document.write`,
//! `template.HTML`). It is disjoint from F2.1 OWASP (injection sinks via the
//! `SecurityAnalyzer`), F2.4 secrets, and F2.6 config — F2.2 scores *boundary
//! input validation*. The safe forms (`yaml.safe_load`, a width-limited
//! `scanf("%31s"`) are not matched.
//!
//! This replaces a stub that scored `validate`/`sanitize`/`.parse()` keyword
//! density. Comments and `#[cfg(test)]`/test regions are excluded via
//! `code_regions`.
//!
//! **Standalone fallback (`--no-default-features`)**: the prior keyword-density
//! heuristic, labelled.
//!
//! **Scope**: per-file; rolls up as `AggKind::WorstOf` (the worst file in scope
//! is the score — one unvalidated boundary is a vulnerability), so the engine
//! is high-precision (a false positive would drag the whole scope).

use crate::verifications::Verification;
use crate::{DimId, DimScore};
use anyhow::Result;
use std::path::Path;

/// F2.2 verifier — Input Validation.
#[allow(non_camel_case_types)]
pub struct F2_2_InputValidation;

impl Verification for F2_2_InputValidation {
    fn id(&self) -> DimId {
        DimId::F2_2
    }

    fn check(&self, target: &Path) -> Result<DimScore> {
        let (value, evidence) = analyze_input_validation_dim(target)?;
        Ok(crate::verifications::finish(
            self.id(),
            value,
            evidence,
            target,
        ))
    }
}

// ── Real engine: polyglot input-validation anti-pattern detection ─────────────
#[cfg(feature = "workspace-integration")]
fn analyze_input_validation_dim(target: &Path) -> Result<(f32, String)> {
    use touring_analysis::quality::{analyze_input_validation, score_input_validation};

    // The engine (`input_validation.rs`) embeds every needle (`b"pickle.loads("`,
    // `b"gets("`, `b"yaml.load("`, …) as data, so scanning its own source is a
    // self-match false positive.
    if is_detector_own_source(target) {
        return Ok((
            1.0,
            "F2.2: detector own source (input-validation needle vocabulary embedded as data) — score=1.000"
                .to_string(),
        ));
    }

    let raw = crate::verifications::read_target_source(target)?;
    let lang = crate::verifications::lang_from_ext(target);
    let r = analyze_input_validation(&raw, lang);

    let value = score_input_validation(&r);
    let top = r
        .findings
        .first()
        .map(|(m, c)| format!("; top: {m} ({c}x)"))
        .unwrap_or_default();
    let evidence = format!(
        "F2.2: {} input-validation anti-pattern(s) over {} lines ({lang}) — score={value:.3} \
         (touring-analysis analyze_input_validation: blocklist / CWE-502 deser / CWE-242 unbounded / XSS-bypass){top}",
        r.violations, r.total_lines
    );
    Ok((value, evidence))
}

/// The engine and this verifier embed the input-validation needle vocabulary
/// (`pickle.loads(`, `gets(`, …) as detection data, so scoring their own source
/// is a self-match false positive. Mirrors `f1_5_tech_debt::is_detector_own_source`
/// (test/bench dirs + the quality engine + verifier dirs). Pure path logic.
#[cfg(feature = "workspace-integration")]
fn is_detector_own_source(target: &Path) -> bool {
    crate::verifications::is_detector_own_source(target)
}

// ── Standalone fallback: validate/sanitize keyword-density heuristic ───────────
#[cfg(not(feature = "workspace-integration"))]
fn analyze_input_validation_dim(target: &Path) -> Result<(f32, String)> {
    let raw = crate::verifications::read_target_source(target)?;
    let validations = raw.matches("validate").count()
        + raw.matches("sanitize").count()
        + raw.matches(".parse()").count();
    let value = if validations > 0 { 1.0 } else { 0.9 };
    let evidence = format!(
        "{validations} validate/sanitize keyword(s) (heuristic; build --features \
         workspace-integration for polyglot input-validation analysis)"
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
    fn test_input_validation_returns_valid_score() {
        let f = write_temp_ext("fn f(p: &Path) -> bool { p.exists() }\n", ".rs");
        let s = F2_2_InputValidation.check(f.path()).expect("check");
        assert!(
            (0.0..=1.0).contains(&s.value),
            "score out of range: {}",
            s.value
        );
    }

    #[test]
    fn test_input_validation_empty_file() {
        let f = write_temp("");
        let s = F2_2_InputValidation.check(f.path()).expect("check");
        assert!((0.0..=1.0).contains(&s.value));
    }

    /// Validated boundary → high score.
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn test_validated_boundary_high() {
        let f = write_temp_ext(
            "fn load(p: &Path, base: &Path) -> Result<Vec<u8>> {\n    let real = p.canonicalize()?;\n    if !real.starts_with(base) { bail!(\"traversal\"); }\n    std::fs::read(real)\n}\n",
            ".rs",
        );
        let s = F2_2_InputValidation.check(f.path()).expect("check");
        assert!(
            s.value > 0.95,
            "validated boundary should be high, got {}",
            s.value
        );
    }

    /// **End-to-end vs stub**: a blocklist path defense scores below a validated
    /// boundary — the stub (keyword density) was blind to it.
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn test_blocklist_scores_lower() {
        let bad = write_temp_ext(
            "fn clean(p: &str) -> String { p.replace(\"../\", \"\") }\n",
            ".rs",
        );
        let good = write_temp_ext(
            "fn clean(p: &Path, base: &Path) -> bool { p.canonicalize().map(|r| r.starts_with(base)).unwrap_or(false) }\n",
            ".rs",
        );
        let sb = F2_2_InputValidation.check(bad.path()).expect("check");
        let sg = F2_2_InputValidation.check(good.path()).expect("check");
        assert!(
            sb.value < sg.value,
            "blocklist ({}) < canonicalize ({})",
            sb.value,
            sg.value
        );
    }

    /// Polyglot: Python insecure deserialization flagged; the safe form is not.
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn test_python_deser_polyglot() {
        let unsafe_d = write_temp_ext("import yaml\ncfg = yaml.load(open(p).read())\n", ".py");
        let safe_d = write_temp_ext("import yaml\ncfg = yaml.safe_load(open(p).read())\n", ".py");
        let su = F2_2_InputValidation.check(unsafe_d.path()).expect("check");
        let ss = F2_2_InputValidation.check(safe_d.path()).expect("check");
        assert!(
            su.value < ss.value,
            "yaml.load ({}) must score below yaml.safe_load ({})",
            su.value,
            ss.value
        );
        assert!(
            (ss.value - 1.0).abs() < 1e-6,
            "yaml.safe_load is clean, got {}",
            ss.value
        );
    }

    /// Polyglot: C unbounded input (`gets`) flagged via `.c`.
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn test_cpp_gets_polyglot() {
        let bad = write_temp_ext("int main() { char b[8]; gets(b); return 0; }\n", ".c");
        let good = write_temp_ext(
            "int main() { char b[8]; fgets(b, sizeof(b), stdin); return 0; }\n",
            ".c",
        );
        let sb = F2_2_InputValidation.check(bad.path()).expect("check");
        let sg = F2_2_InputValidation.check(good.path()).expect("check");
        assert!(
            sb.value < sg.value,
            "gets ({}) must score below fgets ({})",
            sb.value,
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
            "/x/touring-analysis/src/quality/input_validation.rs"
        )));
        assert!(is_detector_own_source(Path::new(
            "/x/crates/foo/tests/bar.rs"
        )));
        assert!(!is_detector_own_source(Path::new(
            "/x/crates/touring-server/src/main.rs"
        )));
    }
}
