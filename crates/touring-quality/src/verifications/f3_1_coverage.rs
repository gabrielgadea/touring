//! F3.1 — Test Coverage verifier (D27).
//!
//! **Real engine (default `workspace-integration`)**: delegates to
//! [`touring_analysis::quality::CoverageArtifact`], which consumes the LCOV
//! artifact emitted by `cargo llvm-cov`. When an artifact exists and contains a
//! record for the target file, F3.1 reports the file's REAL line coverage
//! (`hit / found`). This composes with the `CoverageRatio` scope roll-up
//! (Σcovered / Σtotal ≈ LOC-weighted) into faithful workspace coverage.
//!
//! **Honest fallback (both modes)**: when no `cargo-llvm-cov` artifact is
//! present (or it has no record for this file), the score falls back to the
//! presence ratio — `#[test]` fns over the public API surface they must cover —
//! clearly labelled. True line coverage is never *guessed*; the proxy is only
//! ever reported as a proxy.
//!
//! W0 (2026-06-21) replaced the original `tests / (loc / 50)` density measure
//! (which penalised long files) with this presence ratio; W12 (2026-06-22) adds
//! the real LCOV-artifact path on top.
//!
//! **Scope**: `AggKind::CoverageRatio` — LOC-weighted Σcovered/Σtotal.

use crate::DimId;
use crate::verifications::Verification;
use anyhow::Result;
use std::path::Path;

/// F3.1 verifier — Test Coverage.
#[allow(non_camel_case_types)]
pub struct F3_1_Coverage;

impl Verification for F3_1_Coverage {
    fn id(&self) -> DimId {
        DimId::F3_1
    }

    fn measure(&self, target: &Path) -> Result<(f32, String)> {
        analyze_coverage(target)
    }
}

// ── Real engine: consume a cargo-llvm-cov LCOV artifact ───────────────────────
#[cfg(feature = "workspace-integration")]
fn analyze_coverage(target: &Path) -> Result<(f32, String)> {
    use touring_analysis::quality::CoverageArtifact;
    let report = CoverageArtifact::new().analyze(target);
    if let Some(ratio) = report.ratio {
        let value = ratio.clamp(0.0, 1.0);
        let artifact = report
            .artifact_path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        let evidence = format!(
            "F3.1: REAL line coverage {}/{} = {value:.3} from {artifact} (cargo-llvm-cov)",
            report.lines_hit, report.lines_found
        );
        return Ok((value, evidence));
    }
    // No artifact (or no record for this file) → honest presence proxy.
    let (v, e) = presence_proxy(target)?;
    let note = if report.artifact_path.is_some() {
        "lcov artifact found but no record for this file"
    } else {
        "no cargo-llvm-cov artifact; run `cargo llvm-cov --lcov` for real line coverage"
    };
    Ok((v, format!("{e} [{note}]")))
}

// ── Standalone fallback: presence proxy only ──────────────────────────────────
#[cfg(not(feature = "workspace-integration"))]
fn analyze_coverage(target: &Path) -> Result<(f32, String)> {
    let (v, e) = presence_proxy(target)?;
    Ok((
        v,
        format!(
            "{e} (presence proxy; build --features workspace-integration to consume cargo-llvm-cov)"
        ),
    ))
}

/// Presence ratio: `#[test]` functions over the public API surface they must
/// cover. A file with no public API has nothing to cover at this scope and is
/// not penalised (its internals' true coverage is the llvm-cov gate). Needles
/// are split via `concat!` so the verifier never counts the literals in its own
/// source; real attributes elsewhere still count.
fn presence_proxy(target: &Path) -> Result<(f32, String)> {
    let raw = crate::verifications::read_target_source(target)?;
    let lang = crate::verifications::lang_of(target);

    let mut paired_note = String::new();
    let (test_fns, pub_fns) = if lang == "python" {
        // Python: module-level `def foo(` / `async def foo(` where name does
        // not start with `_` (private by convention). Module-level = no
        // leading whitespace on the line.
        let py_pub = raw
            .lines()
            .filter(|line| {
                let s = line.trim_end();
                let Some(name) = s
                    .strip_prefix("async def ")
                    .or_else(|| s.strip_prefix("def "))
                else {
                    return false;
                };
                !name.starts_with('_')
            })
            .count();
        // The module's own tests plus those of the pytest files that pair with it
        // (26/09/2026: counting only in-file made every module under pytest's layout
        // score 0.000 by construction).
        let mut py_test = count_python_tests(&raw);
        let paired = paired_python_tests(target);
        for file in &paired {
            py_test += std::fs::read_to_string(file)
                .map(|src| count_python_tests(&src))
                .unwrap_or(0);
        }
        if !paired.is_empty() {
            let names: Vec<String> = paired
                .iter()
                .filter_map(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
                .collect();
            paired_note = format!(", paired with {}", names.join(" + "));
        }
        (py_test, py_pub)
    } else {
        let t = raw.matches(concat!("#[", "test]")).count()
            + raw.matches(concat!("#[", "tokio::test]")).count()
            + raw.matches(concat!("#[", "async_test]")).count()
            + raw.matches(concat!("#[", "rstest]")).count();
        let p = raw.matches(concat!("pub ", "fn ")).count()
            + raw.matches(concat!("pub ", "async fn ")).count();
        (t, p)
    };

    let value = if pub_fns == 0 {
        1.0
    } else {
        (test_fns as f32 / pub_fns as f32).min(1.0)
    };
    let evidence = format!(
        "test presence: test fns={test_fns}, public fns={pub_fns}, ratio={value:.3} ({lang}{paired_note})"
    );
    Ok((value, evidence))
}

/// `def test_…` / `async def test_…` at any indent — pytest's collection rule for
/// functions and methods.
fn count_python_tests(src: &str) -> usize {
    src.lines()
        .filter(|l| {
            let t = l.trim_start();
            t.starts_with("def test_") || t.starts_with("async def test_")
        })
        .count()
}

/// The pytest files that, by pytest's naming convention, test the Python module
/// `target`: `test_<m>.py` / `<m>_test.py` beside it, and `tests/test_<m>.py` in its
/// directory or in any ancestor up to the repository root (bounded). A test file or
/// `conftest.py` pairs with nothing — it is not a unit under test.
fn paired_python_tests(target: &Path) -> Vec<std::path::PathBuf> {
    let Some(stem) = target.file_stem().and_then(|s| s.to_str()) else {
        return Vec::new();
    };
    if stem.starts_with("test_") || stem.ends_with("_test") || stem == "conftest" {
        return Vec::new();
    }
    let Some(dir) = std::fs::canonicalize(target)
        .ok()
        .and_then(|abs| abs.parent().map(Path::to_path_buf))
    else {
        return Vec::new();
    };
    let test_name = format!("test_{stem}.py");
    let mut candidates = vec![dir.join(&test_name), dir.join(format!("{stem}_test.py"))];
    let root = super::find_repo_root(&dir);
    let mut cur = Some(dir);
    for _ in 0..8 {
        let Some(d) = cur else { break };
        candidates.push(d.join("tests").join(&test_name));
        if root.as_deref() == Some(d.as_path()) {
            break;
        }
        cur = d.parent().map(Path::to_path_buf);
    }
    candidates.dedup();
    candidates.into_iter().filter(|p| p.is_file()).collect()
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
    fn test_coverage_returns_valid_score() {
        let f = write_temp("fn example() {}\n");
        let s = F3_1_Coverage.check(f.path()).expect("check");
        assert!(
            (0.0..=1.0).contains(&s.value),
            "score out of range: {}",
            s.value
        );
    }

    #[test]
    fn test_coverage_empty_file() {
        let f = write_temp("");
        let s = F3_1_Coverage.check(f.path()).expect("check");
        assert!((0.0..=1.0).contains(&s.value));
    }

    /// W0 fixture: a long file is NOT penalised merely for length when its
    /// public surface is well covered (no artifact → presence proxy path).
    #[test]
    fn test_long_file_well_covered_scores_high() {
        let padding = "// comment line\n".repeat(500);
        let content = format!(
            "{padding}pub fn only_public() {{}}\n{}",
            concat!("#[", "test]\nfn covers() {{}}\n")
        );
        let f = write_temp(&content);
        let s = F3_1_Coverage.check(f.path()).expect("check");
        assert_eq!(
            s.value, 1.0,
            "one test for one pub fn must be full presence regardless of length, got {}",
            s.value
        );
    }

    /// W0 fixture: many public fns with no tests → low presence ratio.
    #[test]
    fn test_untested_public_api_scores_low() {
        let content = "pub fn a() {}\npub fn b() {}\npub fn c() {}\npub fn d() {}\n";
        let f = write_temp(content);
        let s = F3_1_Coverage.check(f.path()).expect("check");
        assert!(
            s.value < 0.5,
            "four untested pub fns must score low, got {}",
            s.value
        );
    }

    /// W12 real-engine fixture: when a cargo-llvm-cov artifact exists with a
    /// record for the target, F3.1 reports the REAL line coverage, not the
    /// presence proxy.
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn test_real_lcov_artifact_drives_score() {
        let dir = tempfile::tempdir().expect("tmpdir");
        let src_dir = dir.path().join("src");
        std::fs::create_dir_all(&src_dir).expect("mkdir");
        let target = src_dir.join("covered.rs");
        // 2 untested pub fns → presence proxy alone would score 0.0.
        std::fs::write(&target, "pub fn a() {}\npub fn b() {}\n").expect("write src");

        let mut lcov = std::fs::File::create(dir.path().join("lcov.info")).expect("lcov");
        write!(
            lcov,
            "TN:\nSF:{}\nLF:10\nLH:9\nend_of_record\n",
            target.display()
        )
        .expect("write lcov");

        let s = F3_1_Coverage.check(&target).expect("check");
        assert!(
            (s.value - 0.9).abs() < 1e-6,
            "real coverage 9/10 must win over the 0.0 presence proxy, got {}",
            s.value
        );
        assert!(s.evidence.contains("REAL line coverage"));
    }

    /// 26/09/2026 (report from the analise-af session): with no coverage artifact the
    /// Python proxy counted `def test_` INSIDE the module itself, so under pytest's
    /// layout (`tests/test_<m>.py`) every module scored 0.000 by construction —
    /// `manuela_poa_rmpa.py` read «test fns=0» beside 26 tests at 94.5 % real coverage,
    /// and `loop_converged` stuck on F3.1 in any Python scope. The proxy now pairs a
    /// module with its tests by pytest's naming convention.
    #[test]
    fn python_module_counts_the_tests_its_pytest_file_pairs_with() {
        let repo = tempfile::tempdir().expect("tmpdir");
        std::fs::create_dir_all(repo.path().join(".git")).expect(".git");
        std::fs::write(repo.path().join(".git/HEAD"), "ref: refs/heads/main\n").expect("HEAD");
        let two_public = "def a():\n    return 1\n\n\ndef b():\n    return 2\n";
        let pkg = repo.path().join("pkg");
        std::fs::create_dir_all(pkg.join("tests")).expect("mkdir");
        std::fs::write(pkg.join("mod.py"), two_public).expect("module");
        std::fs::write(
            pkg.join("tests/test_mod.py"),
            "def test_a():\n    assert True\n\n\ndef test_b():\n    assert True\n",
        )
        .expect("tests");
        // a module deep in the tree, tested from the repository-level tests/ dir
        std::fs::create_dir_all(repo.path().join("src/app")).expect("mkdir");
        std::fs::create_dir_all(repo.path().join("tests")).expect("mkdir");
        std::fs::write(repo.path().join("src/app/deep.py"), two_public).expect("deep");
        std::fs::write(
            repo.path().join("tests/test_deep.py"),
            "def test_a():\n    assert True\n",
        )
        .expect("deep tests");
        // control: no test file pairs with this module
        std::fs::write(pkg.join("lonely.py"), two_public).expect("lonely");

        let paired = F3_1_Coverage.check(&pkg.join("mod.py")).expect("check");
        assert_eq!(paired.value, 1.0, "{}", paired.evidence);
        assert!(
            paired.evidence.contains("test_mod.py"),
            "{}",
            paired.evidence
        );
        let deep = F3_1_Coverage
            .check(&repo.path().join("src/app/deep.py"))
            .expect("check");
        assert!((deep.value - 0.5).abs() < 1e-6, "{}", deep.evidence);
        let lonely = F3_1_Coverage.check(&pkg.join("lonely.py")).expect("check");
        assert_eq!(
            lonely.value, 0.0,
            "control: nothing pairs — {}",
            lonely.evidence
        );
    }
}
