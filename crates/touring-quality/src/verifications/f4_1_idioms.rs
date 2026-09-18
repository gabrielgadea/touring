//! F4.1 — Language Idioms verifier (D40).
//!
//! **Real engine (default `workspace-integration`)**: delegates to
//! [`touring_analysis::quality::analyze_idioms`] — a polyglot detector of
//! non-idiomatic constructs across **7 languages**, approximating a
//! high-confidence subset of each language's lint oracle: clippy (Rust —
//! `len_zero`, `bool_comparison`, `ptr_arg`, …), ruff (Python — E711/E712/E721/
//! E731/E722), ESLint (TS/JS — `eqeqeq` char-aware, `no-var`, `no-explicit-any`,
//! …), go vet (`interface{}`→`any`), clang-tidy (C++), and Java legacy APIs.
//! Comments and `#[cfg(test)]`/test regions are excluded via `code_regions`.
//!
//! This replaces a stub that counted `let ` + `match ` occurrences and returned
//! `1.0` above five — a metric with no relationship to idiomaticity.
//!
//! **Standalone fallback (`--no-default-features`)**: the prior `let`/`match`
//! count heuristic, labelled.
//!
//! **Scope**: per-file (`AggKind::WeightedLoc`). A per-file scanner cannot
//! replace a type-aware linter, so it is honest about catching a subset.

use crate::DimId;
use crate::verifications::Verification;
use anyhow::Result;
use std::path::Path;

/// F4.1 verifier — Language Idioms.
#[allow(non_camel_case_types)]
pub struct F4_1_Idioms;

impl Verification for F4_1_Idioms {
    fn id(&self) -> DimId {
        DimId::F4_1
    }

    fn measure(&self, target: &Path) -> Result<(f32, String)> {
        analyze_idioms_dim(target)
    }
}

// ── Real engine: polyglot idiom detection ─────────────────────────────────────
#[cfg(feature = "workspace-integration")]
fn analyze_idioms_dim(target: &Path) -> Result<(f32, String)> {
    use touring_analysis::quality::{analyze_idioms, score_idioms};

    // The idiom engine (`idioms.rs`) embeds every needle (`b".len() == 0"`,
    // `b"== None"`, …) as data, so scanning its own source is a self-match FP.
    if is_detector_own_source(target) {
        return Ok((
            1.0,
            "F4.1: detector own source (idiom needle vocabulary embedded as data) — score=1.000"
                .to_string(),
        ));
    }

    let raw = crate::verifications::read_target_source(target)?;
    let lang = crate::verifications::lang_of(target);
    if lang == "shell" {
        return Ok(shellcheck_idioms(SHELLCHECK, target, &raw));
    }
    let r = analyze_idioms(&raw, lang);

    let value = score_idioms(&r);
    let top = crate::verifications::top_finding(&r.findings);
    let evidence = format!(
        "F4.1: {} non-idiomatic construct(s) over {} lines ({lang}) — score={value:.3} \
         (touring-analysis analyze_idioms: clippy/ruff/ESLint/go-vet/clang-tidy idioms){top}",
        r.violations, r.total_lines
    );
    Ok((value, evidence))
}

/// The shell lint oracle, looked up on `PATH`.
#[cfg(feature = "workspace-integration")]
const SHELLCHECK: &str = "shellcheck";

/// Shell idioms, from ShellCheck — the lint oracle for shell, as clippy is for
/// Rust.
///
/// Canvas D (15/09/2026): a shell script had no idiom reading at all; with Rust
/// rules it passed vacuously. Findings are weighed by level (error 1.0, warning
/// 0.5, info 0.1, style 0.05) on the same density curve as the other languages.
/// `--norc` keeps a user's `~/.shellcheckrc` out of the score.
///
/// ShellCheck absent or failing is UNVERIFIED (0.5, Warn), never a pass: no
/// idiom was checked.
#[cfg(feature = "workspace-integration")]
fn shellcheck_idioms(bin: &str, target: &Path, raw: &str) -> (f32, String) {
    use touring_analysis::quality::density_score;

    let unverified = |why: String| {
        (
            0.5,
            format!(
                "F4.1: UNVERIFIED — {why}; the shell idioms of this script were not checked \
                 (install shellcheck) — score=0.500"
            ),
        )
    };
    let output = match std::process::Command::new(bin)
        .args(["--format=json1", "--norc", "--"])
        .arg(target)
        .output()
    {
        Ok(output) => output,
        Err(e) => return unverified(format!("shellcheck could not run ({e})")),
    };
    // Exit 0 = clean, 1 = findings; anything else is ShellCheck failing.
    if !matches!(output.status.code(), Some(0 | 1)) {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return unverified(format!(
            "shellcheck exited {:?}: {}",
            output.status.code(),
            stderr.trim()
        ));
    }
    let Ok(report) = serde_json::from_slice::<serde_json::Value>(&output.stdout) else {
        return unverified("shellcheck printed no JSON".to_string());
    };
    let comments = report["comments"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default();
    let mut levels = [0usize; 4];
    let mut codes: std::collections::BTreeMap<u64, usize> = std::collections::BTreeMap::new();
    for comment in comments {
        let slot = match comment["level"].as_str() {
            Some("error") => 0,
            Some("warning") => 1,
            Some("info") => 2,
            _ => 3,
        };
        levels[slot] += 1;
        *codes
            .entry(comment["code"].as_u64().unwrap_or(0))
            .or_default() += 1;
    }
    let weighted = levels[0] as f32
        + 0.5 * levels[1] as f32
        + 0.1 * levels[2] as f32
        + 0.05 * levels[3] as f32;
    let total_lines = raw.lines().count();
    let value = density_score(weighted, total_lines, 8.0);
    let top = codes
        .iter()
        .max_by_key(|(code, count)| (**count, std::cmp::Reverse(**code)))
        .map(|(code, count)| format!("; top: SC{code} ({count}x)"))
        .unwrap_or_default();
    (
        value,
        format!(
            "F4.1: shellcheck {} error(s), {} warning(s), {} info, {} style over {total_lines} lines \
             (shell) — score={value:.3}{top}",
            levels[0], levels[1], levels[2], levels[3]
        ),
    )
}

/// The idiom engine and this verifier embed the needle vocabulary
/// (`b".len() == 0"`, `b"== None"`, …) as detection data, so scoring their own
/// source is a self-match false positive. Mirrors
/// `f1_5_tech_debt::is_detector_own_source` (test/bench dirs + the quality
/// engine + verifier dirs). Pure path logic.
#[cfg(feature = "workspace-integration")]
fn is_detector_own_source(target: &Path) -> bool {
    crate::verifications::is_detector_own_source(target)
}

// ── Standalone fallback: let/match count heuristic (no idiom awareness) ────────
#[cfg(not(feature = "workspace-integration"))]
fn analyze_idioms_dim(target: &Path) -> Result<(f32, String)> {
    let raw = crate::verifications::read_target_source(target)?;
    let modern = raw.matches("let ").count() + raw.matches("match ").count();
    let value = if modern > 5 { 1.0 } else { 0.7 };
    let evidence = format!(
        "{modern} let/match constructs (heuristic; build --features workspace-integration \
         for polyglot idiom analysis)"
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
    fn test_idioms_returns_valid_score() {
        let f = write_temp_ext("fn f(v: &[u32]) -> bool { v.is_empty() }\n", ".rs");
        let s = F4_1_Idioms.check(f.path()).expect("check");
        assert!((0.0..=1.0).contains(&s.value), "out of range: {}", s.value);
    }

    /// Canvas D (15/09/2026): shell idioms come from ShellCheck; without it the
    /// dimension is UNVERIFIED, never a pass.
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn shell_idioms_come_from_shellcheck_and_its_absence_is_unverified() {
        let clean = write_temp_ext(
            "#!/bin/bash\nset -eu\nname=\"$1\"\nprintf '%s\\n' \"$name\"\n",
            ".sh",
        );
        let sloppy = write_temp_ext(
            "#!/bin/bash\nfor f in $(ls *.txt); do\n  rm $f\ndone\ncd $1\n",
            ".sh",
        );
        let (value, evidence) = shellcheck_idioms("/nonexistent/shellcheck", sloppy.path(), "x\n");
        assert_eq!(value, 0.5, "{evidence}");
        assert!(evidence.contains("UNVERIFIED"), "{evidence}");

        let installed = std::process::Command::new(SHELLCHECK)
            .arg("--version")
            .output()
            .is_ok_and(|o| o.status.success());
        if !installed {
            // An image without ShellCheck: the production path says so too.
            let s = F4_1_Idioms.check(sloppy.path()).expect("check");
            assert!(s.evidence.contains("UNVERIFIED"), "{}", s.evidence);
            return;
        }
        // An unreadable script: ShellCheck prints `{"comments":[]}` and exits 2,
        // which read as JSON alone would be a clean pass.
        let (value, evidence) =
            shellcheck_idioms(SHELLCHECK, Path::new("/nonexistent/canvas-d.sh"), "x\n");
        assert_eq!(value, 0.5, "{evidence}");
        assert!(evidence.contains("exited Some(2)"), "{evidence}");
        let good = F4_1_Idioms.check(clean.path()).expect("check");
        let bad = F4_1_Idioms.check(sloppy.path()).expect("check");
        assert_eq!(good.value, 1.0, "{}", good.evidence);
        assert!(
            bad.value < good.value,
            "{} vs {}",
            bad.evidence,
            good.evidence
        );
        assert!(bad.evidence.contains("(shell)"), "{}", bad.evidence);
        assert!(bad.evidence.contains("; top: SC"), "{}", bad.evidence);
    }

    #[test]
    fn test_idioms_empty_file() {
        let f = write_temp("");
        let s = F4_1_Idioms.check(f.path()).expect("check");
        assert!((0.0..=1.0).contains(&s.value));
    }

    /// Idiomatic rust → high score.
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn test_idiomatic_rust_high() {
        let f = write_temp_ext(
            "fn f(v: &[u32]) -> bool {\n    if v.is_empty() {\n        return true;\n    }\n    v.first().is_some()\n}\n",
            ".rs",
        );
        let s = F4_1_Idioms.check(f.path()).expect("check");
        assert!(
            s.value > 0.95,
            "idiomatic rust should be high, got {}",
            s.value
        );
    }

    /// **End-to-end vs stub**: non-idiomatic rust (`.len()==0`, `&Vec`, `==true`)
    /// scores below idiomatic — the stub (counting `let`/`match`) could not tell.
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn test_non_idiomatic_rust_lower() {
        let bad = write_temp_ext(
            "fn f(v: &Vec<u32>) -> bool {\n    if v.len() == 0 {\n        return false;\n    }\n    v.is_empty() == true\n}\n",
            ".rs",
        );
        let good = write_temp_ext("fn f(v: &[u32]) -> bool { v.is_empty() }\n", ".rs");
        let sb = F4_1_Idioms.check(bad.path()).expect("check");
        let sg = F4_1_Idioms.check(good.path()).expect("check");
        assert!(
            sb.value < sg.value,
            "non-idiomatic ({}) < idiomatic ({})",
            sb.value,
            sg.value
        );
    }

    /// Polyglot: Python `== None`/`== True` are flagged via the `.py` extension.
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn test_python_idioms_flagged() {
        let f = write_temp_ext(
            "def f(x):\n    if x == None:\n        return\n    if x == True:\n        pass\n",
            ".py",
        );
        let s = F4_1_Idioms.check(f.path()).expect("check");
        assert!(
            s.value < 1.0,
            "python E711/E712 should lower the score, got {}",
            s.value
        );
    }

    /// Polyglot: TS `eqeqeq` flags loose `==` but not strict `===`.
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn test_ts_eqeqeq_polyglot() {
        let loose = write_temp_ext(
            "function f(a: number, b: number) { return a == b; }\n",
            ".ts",
        );
        let strict = write_temp_ext(
            "function f(a: number, b: number) { return a === b; }\n",
            ".ts",
        );
        let sl = F4_1_Idioms.check(loose.path()).expect("check");
        let ss = F4_1_Idioms.check(strict.path()).expect("check");
        assert!(
            sl.value < ss.value,
            "loose == ({}) must score below strict === ({})",
            sl.value,
            ss.value
        );
        assert!(
            (ss.value - 1.0).abs() < 1e-6,
            "strict TS is idiomatic, got {}",
            ss.value
        );
    }

    /// The idiom engine's own source (which embeds every needle as data) must be
    /// allowlisted, not self-matched.
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn test_detector_own_source_allowlisted() {
        use std::path::Path;
        assert!(is_detector_own_source(Path::new(
            "/x/touring-analysis/src/quality/idioms.rs"
        )));
        assert!(is_detector_own_source(Path::new(
            "/x/crates/foo/tests/bar.rs"
        )));
        assert!(!is_detector_own_source(Path::new(
            "/x/crates/touring-server/src/main.rs"
        )));
    }
}
