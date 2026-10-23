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
        // Second criterion, additive to the proximity pairing above: test files
        // anywhere under the repository root that IMPORT this module (23/10/2026 —
        // `tests/hooks_contract/test_x.py` for `scripts/diagnostics/x.py` read
        // `test fns=0` beside a full suite). A file already paired is not recounted.
        let importers: Vec<_> = super::f3_1_py_imports::importing_tests(target)
            .into_iter()
            .filter(|t| !paired.contains(&t.path))
            .collect();
        for t in &importers {
            py_test += t.test_fns;
        }
        if !importers.is_empty() {
            paired_note.push_str(&importers_note(&importers));
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

/// ", imported by a.py + b.py (+N more)" — at most three names, so the evidence line
/// stays one line however many test files import the module.
fn importers_note(importers: &[super::f3_1_py_imports::ImportingTest]) -> String {
    let names: Vec<String> = importers
        .iter()
        .take(3)
        .filter_map(|t| t.path.file_name().map(|n| n.to_string_lossy().into_owned()))
        .collect();
    let more = importers.len().saturating_sub(3);
    let tail = if more > 0 {
        format!(" (+{more} more)")
    } else {
        String::new()
    };
    format!(", imported by {}{tail}", names.join(" + "))
}

/// `def test_…` / `async def test_…` at any indent — pytest's collection rule for
/// functions and methods.
pub(super) fn count_python_tests(src: &str) -> usize {
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

    // ── import-based test discovery (23/10/2026) ─────────────────────────────

    const TWO_PUBLIC: &str = "def a():\n    return 1\n\n\ndef b():\n    return 2\n";
    const TWO_TESTS: &str = "def test_a():\n    assert True\n\n\ndef test_b():\n    assert True\n";

    /// A throwaway repository (with a `.git` marker) holding `files`. Every fixture
    /// is written BEFORE the first `check`, because the import index is cached per
    /// repository root.
    fn repo_with(files: &[(&str, &str)]) -> tempfile::TempDir {
        let repo = tempfile::tempdir().expect("tmpdir");
        std::fs::create_dir_all(repo.path().join(".git")).expect(".git");
        std::fs::write(repo.path().join(".git/HEAD"), "ref: refs/heads/main\n").expect("HEAD");
        for (rel, body) in files {
            let p = repo.path().join(rel);
            std::fs::create_dir_all(p.parent().expect("parent")).expect("mkdir");
            std::fs::write(&p, body).expect("write fixture");
        }
        repo
    }

    fn score(repo: &tempfile::TempDir, rel: &str) -> crate::DimScore {
        F3_1_Coverage.check(&repo.path().join(rel)).expect("check")
    }

    /// Positive control: a test in the repository-root `tests/` tree, named so the
    /// proximity rule cannot pair it, covers the module it imports by full path.
    #[test]
    fn import_of_the_package_path_counts_for_the_module() {
        let repo = repo_with(&[
            ("pkg/mod.py", TWO_PUBLIC),
            (
                "tests/contract/test_pkg_contract.py",
                &format!("import pkg.mod\n\n{TWO_TESTS}"),
            ),
            ("pkg/untested.py", TWO_PUBLIC),
        ]);
        let s = score(&repo, "pkg/mod.py");
        assert_eq!(s.value, 1.0, "{}", s.evidence);
        assert!(
            s.evidence.contains("imported by test_pkg_contract.py"),
            "{}",
            s.evidence
        );
        // control: nothing imports it and no test sits near it
        let none = score(&repo, "pkg/untested.py");
        assert_eq!(none.value, 0.0, "{}", none.evidence);
    }

    /// Negative control: homonymous modules in different packages do not cover each
    /// other — the match is by full module path, never by basename.
    #[test]
    fn homonymous_module_in_another_package_is_not_covered() {
        let repo = repo_with(&[
            ("pkg/mod.py", TWO_PUBLIC),
            ("other/mod.py", TWO_PUBLIC),
            (
                "tests/test_only_other.py",
                &format!("from other.mod import a\n\n{TWO_TESTS}"),
            ),
        ]);
        let other = score(&repo, "other/mod.py");
        assert_eq!(other.value, 1.0, "{}", other.evidence);
        let pkg = score(&repo, "pkg/mod.py");
        assert_eq!(
            pkg.value, 0.0,
            "homonym must not be covered — {}",
            pkg.evidence
        );
    }

    /// Negative control: the module's name in a comment, a string or a docstring is
    /// not an import.
    #[test]
    fn citing_the_module_in_a_string_or_comment_is_not_an_import() {
        let body = format!(
            "\"\"\"\nfrom pkg.cited import a\n\"\"\"\n# import pkg.cited\n\
             NAME = \"import pkg.cited\"\nOTHER = 'from pkg import cited'\n\n{TWO_TESTS}"
        );
        let repo = repo_with(&[
            ("pkg/cited.py", TWO_PUBLIC),
            ("tests/test_mentions.py", &body),
        ]);
        let s = score(&repo, "pkg/cited.py");
        assert_eq!(s.value, 0.0, "{}", s.evidence);
        assert!(!s.evidence.contains("imported by"), "{}", s.evidence);
    }

    /// Non-regression: the proximity pairing still counts, and the import criterion
    /// ADDS to it instead of replacing it.
    #[test]
    fn proximity_pairing_still_counts_and_import_adds_to_it() {
        let repo = repo_with(&[
            ("pkg/combo.py", TWO_PUBLIC),
            (
                "pkg/tests/test_combo.py",
                "def test_a():\n    assert True\n",
            ),
            ("pkg/near.py", TWO_PUBLIC),
            ("pkg/tests/test_near.py", TWO_TESTS),
            (
                "tests/test_combo_contract.py",
                "from pkg import combo\n\ndef test_c():\n    assert True\n",
            ),
        ]);
        let combo = score(&repo, "pkg/combo.py");
        assert_eq!(
            combo.value, 1.0,
            "1 paired + 1 imported = 2/2 — {}",
            combo.evidence
        );
        assert!(
            combo.evidence.contains("paired with test_combo.py"),
            "{}",
            combo.evidence
        );
        assert!(
            combo
                .evidence
                .contains("imported by test_combo_contract.py"),
            "{}",
            combo.evidence
        );
        let near = score(&repo, "pkg/near.py");
        assert_eq!(near.value, 1.0, "proximity alone — {}", near.evidence);
        assert!(!near.evidence.contains("imported by"), "{}", near.evidence);
    }

    /// `from pkg import mod` imports the submodule `pkg/mod.py`.
    #[test]
    fn from_package_import_module_counts() {
        let repo = repo_with(&[
            ("pkg/mod.py", TWO_PUBLIC),
            ("pkg/__init__.py", ""),
            (
                "tests/test_from_pkg.py",
                &format!("from pkg import mod as m\n\n{TWO_TESTS}"),
            ),
        ]);
        let s = score(&repo, "pkg/mod.py");
        assert_eq!(s.value, 1.0, "{}", s.evidence);
    }

    /// Relative imports resolve against the test file's own directory; a `src`
    /// layout resolves `import pkg.mod` for `src/pkg/mod.py` only while a single
    /// file answers to the name.
    #[test]
    fn relative_and_src_layout_imports_resolve() {
        let repo = repo_with(&[
            ("core/rel.py", TWO_PUBLIC),
            (
                "core/suite/test_rel_contract.py",
                &format!("from .. import rel\n\n{TWO_TESTS}"),
            ),
            ("src/lib/inner.py", TWO_PUBLIC),
            (
                "tests/test_inner_contract.py",
                &format!("import lib.inner\n\n{TWO_TESTS}"),
            ),
            // two src roots answer to `twin.shared`: ambiguous, so nothing is credited
            ("a/src/twin/shared.py", TWO_PUBLIC),
            ("b/src/twin/shared.py", TWO_PUBLIC),
            (
                "tests/test_twin_contract.py",
                &format!("import twin.shared\n\n{TWO_TESTS}"),
            ),
        ]);
        assert_eq!(score(&repo, "core/rel.py").value, 1.0);
        assert_eq!(score(&repo, "src/lib/inner.py").value, 1.0);
        let twin = score(&repo, "a/src/twin/shared.py");
        assert_eq!(
            twin.value, 0.0,
            "ambiguous name must not credit — {}",
            twin.evidence
        );
    }

    /// A bare `import mod` resolves only when the test edits `sys.path` and exactly
    /// one file answers to the name; a name shared by two files credits neither.
    #[test]
    fn bare_import_needs_sys_path_and_a_unique_name() {
        let repo = repo_with(&[
            ("tools/solo.py", TWO_PUBLIC),
            (
                "tests/test_solo_contract.py",
                &format!("import sys\nsys.path.insert(0, 'tools')\nimport solo\n\n{TWO_TESTS}"),
            ),
            ("tools/nopath.py", TWO_PUBLIC),
            (
                "tests/test_nopath_contract.py",
                &format!("import nopath\n\n{TWO_TESTS}"),
            ),
            ("one/config.py", TWO_PUBLIC),
            ("two/config.py", TWO_PUBLIC),
            (
                "tests/test_config_contract.py",
                &format!("import sys\nsys.path.insert(0, 'one')\nimport config\n\n{TWO_TESTS}"),
            ),
        ]);
        assert_eq!(score(&repo, "tools/solo.py").value, 1.0);
        assert_eq!(score(&repo, "tools/nopath.py").value, 0.0, "no sys.path");
        assert_eq!(score(&repo, "one/config.py").value, 0.0, "shared name");
        assert_eq!(score(&repo, "two/config.py").value, 0.0, "shared name");
    }

    /// An importlib load by a literal file path credits that exact file, not a
    /// homonym at another path.
    #[test]
    fn importlib_load_by_literal_path_credits_that_file_only() {
        let body = format!(
            "import importlib.util\n\
             spec = importlib.util.spec_from_file_location('m', 'scripts/a/loaded.py')\n\n{TWO_TESTS}"
        );
        let repo = repo_with(&[
            ("scripts/a/loaded.py", TWO_PUBLIC),
            ("scripts/b/loaded.py", TWO_PUBLIC),
            ("tests/test_loader_contract.py", &body),
        ]);
        assert_eq!(score(&repo, "scripts/a/loaded.py").value, 1.0);
        assert_eq!(score(&repo, "scripts/b/loaded.py").value, 0.0);
    }
}
