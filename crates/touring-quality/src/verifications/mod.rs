//! Verification engine + 50 dim verifications (5 W1 + 45 W3/W4 stubs).
//!
//! Each verification implements the [Verification] trait. The default
//! dispatch runs all 50 dims in parallel via rayon.

use crate::{DimId, DimScore, DimStatus};
use anyhow::Result;
use std::path::{Path, PathBuf};

pub mod f1_10_data_model;
pub mod f1_11_patterns;
pub mod f1_12_arch_consistency;
pub mod f1_1_complexity;
pub mod f1_2_maintainability;
pub mod f1_3_duplication;
pub mod f1_4_solid;
pub mod f1_5_tech_debt;
pub mod f1_6_error_handling;
pub mod f1_7_boundaries;
pub mod f1_8_dep_cycles;
pub mod f1_9_api_design;
pub mod f2_10_io;
pub mod f2_11_concurrency;
pub mod f2_12_frontend;
pub mod f2_13_scalability;
pub mod f2_1_owasp;
pub mod f2_2_input_validation;
pub mod f2_3_authz;
pub mod f2_4_secrets;
pub mod f2_5_dep_cves;
pub mod f2_6_config;
pub mod f2_7_db_perf;
pub mod f2_8_memory;
pub mod f2_9_caching;
pub mod f3_10_arch_doc;
pub mod f3_11_readme;
pub mod f3_12_doc_accuracy;
pub mod f3_13_changelog;
pub mod f3_1_coverage;
pub mod f3_2_test_quality;
pub mod f3_3_test_pyramid;
pub mod f3_4_edge_cases;
pub mod f3_5_test_maint;
pub mod f3_6_sec_tests;
pub mod f3_7_perf_tests;
pub mod f3_8_inline_doc;
pub mod f3_9_api_doc;
pub mod f4_10_monitoring;
pub mod f4_11_incident;
pub mod f4_12_env;
pub mod f4_1_idioms;
pub mod f4_2_frameworks;
pub mod f4_3_deprecated;
pub mod f4_4_modernization;
pub mod f4_5_pkg_mgmt;
pub mod f4_6_build_config;
pub mod f4_7_cicd;
pub mod f4_8_deploy;
pub mod f4_9_iac;

/// Trait implemented by all 50 dim verifications.
pub trait Verification: Send + Sync {
    /// The quality dimension this verification scores (e.g. `F1.6`).
    fn id(&self) -> DimId;

    /// Measure `target`, returning the raw `(score, evidence)` pair.
    ///
    /// This is the ONLY part a verification writes. Turning that pair into a
    /// [`DimScore`] — status, NotApplicable detection, auto-remediation
    /// suggestion — is [`finish`]'s job and happens in [`Self::check`].
    fn measure(&self, target: &Path) -> Result<(f32, String)>;

    /// Run the verification against `target`, returning its 0.0–1.0 dimension score.
    ///
    /// Defaulted on purpose: all 50 implementations previously ended in a
    /// byte-identical six-line tail (`Ok(finish(self.id(), value, evidence,
    /// target))`) — 50 copies of pure glue, and 50 chances to forget it, since
    /// nothing forced a verification through `finish`. Hoisting the tail here
    /// deletes the duplication AND makes the invariant structural: a
    /// verification cannot bypass status derivation, because it never builds the
    /// `DimScore` itself.
    fn check(&self, target: &Path) -> Result<DimScore> {
        let (value, evidence) = self.measure(target)?;
        Ok(finish(self.id(), value, evidence, target))
    }
}

/// Source-file extensions the content-based verifiers understand (polyglot).
/// Covers the languages the touring workspace and downstream polyglot projects
/// (Python / TypeScript / Go / … monorepos) actually ship.
const SOURCE_EXTS: &[&str] = &[
    "rs", "py", "pyi", "ts", "tsx", "js", "jsx", "mjs", "cjs", "go", "java", "kt", "kts", "swift",
    "c", "cc", "cpp", "cxx", "h", "hpp", "rb", "php", "scala", "cs",
];

/// Directory names skipped when reading a directory target (VCS / build / vendor
/// / virtualenv / cache); dot-prefixed directories are skipped too.
const SKIP_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    "venv",
    ".venv",
    "dist",
    "build",
    "__pycache__",
    ".mypy_cache",
    ".pytest_cache",
    "htmlcov",
    ".tox",
    "vendor",
    ".idea",
    ".vs",
    "coverage",
    // Mutation-testing artifacts: a full mutated copy of the tree (mutmut /
    // cosmic-ray write `mutants/`, cargo-mutants writes `mutants.out`). Every
    // file inside is a near-identical clone of a source file by construction —
    // counting them made F1.3 report 51% duplication on a package whose real
    // src/ duplication was 3.1% (antt-process-analysis, 2026-08-02).
    "mutants",
    "mutants.out",
];

/// Upper bound on concatenated directory source scanned per verifier (keeps large
/// monorepos bounded; single-file scoring is always unbounded).
///
/// 16 MiB, raised from 2 MiB on 2026-08-02: kazuba-geo-engine's `src/` alone is
/// ~4 MiB, so the old cap truncated the scope corpus to its alphabetical prefix
/// — F1.3 scored HALF the crate, stayed byte-identical across real dedup edits
/// in `s…`/`w…` files (they fell past the cut), and the truncation was invisible
/// in the evidence line (a silent cap). Corpus verifiers are O(n) hash/scan
/// passes, so 16 MiB stays sub-second; genuinely larger monorepos still bound.
/// Teto de bytes por varredura de diretório.
///
/// ⚠ **Este teto TRUNCA a medição em escopos grandes, e isso precisa aparecer.**
/// Medido em 07/08/2026: o corpus de fontes deste workspace tem ~26,9 MB, então
/// uma varredura da raiz cobre ~62% dele e para em
/// `crates/touring-intelligence/src/index/incremental.rs` (ordem de path). Como
/// o corte é por BYTES, o efeito é pior do que perder cauda: remover duplicação
/// dentro da janela apenas **admite mais conteúdo** na borda, que traz a própria
/// duplicação — o score fica praticamente imóvel diante de remediação real. Foi
/// exatamente o que se observou ao deduplicar 372 linhas em `touring-analysis` +
/// `touring-quality`: os scores POR CRATE melhoraram (0.453→0.490 e
/// 0.171→0.265) e o score da raiz não se moveu um dígito.
///
/// Enquanto o teto existir, todo consumidor deve **anunciar a truncagem** em vez
/// de apresentar um score de prefixo como se fosse do escopo inteiro — a mesma
/// regra que já vale para os arquivos gerados excluídos ("nunca silencioso").
/// Scores por CRATE ficam abaixo do teto e são confiáveis.
///
/// **128 MiB desde 07/08/2026** (era 16 MiB, que cobria 59% deste workspace).
/// F1.3/F1.8/F4.12 saíram da truncagem por agregação per-crate, mas as 14 dims
/// `ScopeNative` continuam medindo a raiz inteira e não têm equivalente
/// per-crate — para elas, só um teto acima do corpus resolve. A objeção era
/// memória: as 50 dims rodam em `par_iter`, então 14 blobs coexistem. Medido:
/// 26,9 MB de corpus ⇒ ~378 MB de pico real, e o pior caso teórico (1,75 GB)
/// só ocorre num workspace com 128 MiB de fonte. Verificadores de corpus são
/// passes O(n), então o custo de tempo acompanha o corpus, não o teto.
///
/// Se ainda assim truncar, [`dir_scan_overflow`] deixa o fato visível na
/// evidência — teto sem anúncio foi exatamente a falha de 02/08.
const DIR_SCAN_BYTE_CAP: usize = 128 * 1024 * 1024;

/// Bytes totais do corpus de `target` quando ele ESTOURA
/// [`DIR_SCAN_BYTE_CAP`] — `None` quando cabe (o caso normal).
///
/// Só consulta metadata (`len()`), nunca lê conteúdo, então serve para anotar a
/// evidência sem duplicar o custo da varredura. Existe para que um score de
/// prefixo jamais se apresente como score do escopo inteiro.
pub fn dir_scan_overflow(target: &Path) -> Option<u64> {
    if !target.is_dir() {
        return None;
    }
    let total: u64 = enumerate_source_files(target)
        .iter()
        .filter_map(|p| std::fs::metadata(p).ok())
        .map(|m| m.len())
        .sum();
    (total > DIR_SCAN_BYTE_CAP as u64).then_some(total)
}

/// Read a verifier target into a single source string.
///
/// * **File** → its contents (any language).
/// * **Directory** → a deterministic (entries sorted), bounded, recursive
///   concatenation of every polyglot source file (`SOURCE_EXTS`) it contains,
///   skipping `SKIP_DIRS` and dot-dirs, capped at `DIR_SCAN_BYTE_CAP`. This
///   replaces the legacy "first `.rs` file" scan that silently ignored Python /
///   TypeScript / other polyglot projects (Rust-bias fix, 2026-06-20).
pub fn read_target_source(target: &Path) -> Result<String> {
    if !target.is_dir() {
        return std::fs::read_to_string(target)
            .map_err(|e| anyhow::anyhow!("failed to read {}: {}", target.display(), e));
    }
    let mut out = String::new();
    for p in enumerate_source_files(target) {
        if let Ok(s) = std::fs::read_to_string(&p) {
            out.push_str(&s);
            out.push('\n');
            if out.len() >= DIR_SCAN_BYTE_CAP {
                return Ok(out);
            }
        }
    }
    Ok(out)
}

/// True when `path` sits under a machine-generated client/SDK tree: some ancestor
/// (walking up to `root`, inclusive) carries an `.openapi-generator-ignore` file
/// or an `.openapi-generator/` dir.
///
/// Generated trees are rewritten wholesale on regeneration, so their internal
/// duplication is the GENERATOR's signature, never maintenance debt — jscpd and
/// SonarQube exclude generated code from duplication for the same reason. ONLY
/// F1.3 consults this: security/secret/CVE dims keep scanning generated code
/// (the corpus-wide `@generated` content filter was tried on 2026-08-02 and
/// reverted for exactly that blindness, plus 85 prose false-positives).
pub(crate) fn is_under_generated_tree(path: &Path, root: &Path) -> bool {
    let mut cur = path.parent();
    while let Some(dir) = cur {
        if dir.join(".openapi-generator-ignore").is_file()
            || dir.join(".openapi-generator").is_dir()
        {
            return true;
        }
        if dir == root {
            break;
        }
        cur = dir.parent();
    }
    false
}

/// `read_target_source` minus machine-generated trees — the F1.3 duplication
/// corpus. Returns the concatenated source plus HOW MANY files were excluded, so
/// the caller can surface the exclusion in its evidence line (a silent filter
/// would repeat the invisible-cap failure fixed on 2026-08-02).
pub fn read_target_source_excluding_generated(target: &Path) -> Result<(String, usize, bool)> {
    if !target.is_dir() {
        return read_target_source(target).map(|s| (s, 0, false));
    }
    let mut out = String::new();
    let mut excluded = 0usize;
    for p in enumerate_source_files(target) {
        if is_under_generated_tree(&p, target) {
            excluded += 1;
            continue;
        }
        if let Ok(s) = std::fs::read_to_string(&p) {
            out.push_str(&s);
            out.push('\n');
            if out.len() >= DIR_SCAN_BYTE_CAP {
                // Terceiro campo = TRUNCADO. Ver a nota em `DIR_SCAN_BYTE_CAP`:
                // um score de prefixo que se apresenta como score do escopo é
                // uma medição desonesta, então quem chama tem de poder dizê-lo.
                return Ok((out, excluded, true));
            }
        }
    }
    Ok((out, excluded, false))
}

/// Strip Rust line comments (`//…`) and block comments (`/* … */`) from `src`
/// so a content scanner inspects only live code. String literals (`"…"`) are
/// blanked too (escape `\"` honored), so a needle that lives inside an example
/// or doc-leaked string is not falsely matched.
///
/// This is the single shared definition that replaced the byte-identical
/// per-file copies in `f4_3_deprecated` and `f1_8_dep_cycles` (dedup, F1.3).
pub(crate) fn strip_rust_comments_and_strings(src: &str) -> String {
    let bytes = src.as_bytes();
    let mut out = String::with_capacity(bytes.len());
    let mut i = 0usize;
    while i < bytes.len() {
        // Block comment `/* … */`
        if i + 1 < bytes.len() && bytes[i] == b'/' && bytes[i + 1] == b'*' {
            i += 2;
            while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                i += 1;
            }
            i = (i + 2).min(bytes.len());
            out.push(' ');
            continue;
        }
        // Line comment `//…`
        if i + 1 < bytes.len() && bytes[i] == b'/' && bytes[i + 1] == b'/' {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        // String literal `"…"` (escape `\"` honored)
        if bytes[i] == b'"' {
            out.push('"');
            i += 1;
            while i < bytes.len() && bytes[i] != b'"' {
                if bytes[i] == b'\\' && i + 1 < bytes.len() {
                    i += 2;
                } else {
                    i += 1;
                }
            }
            i = (i + 1).min(bytes.len());
            continue;
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

/// Step-function cap applied when a repository-level artifact (README, CI,
/// CHANGELOG, IaC, …) is entirely ABSENT from a directory scope. Absence is a
/// hard defect — not a diluted density — so it caps the dim well below the Gold
/// floor (0.80) regardless of how large the surrounding codebase is. This is
/// the fix for the anti-monotonic bug where a bigger repo hid the same missing
/// artifact behind a larger denominator (W1, 2026-07-02).
pub const ARTIFACT_ABSENT_CAP: f32 = 0.30;

/// Repository artifact classes whose quality lives in dedicated files on disk,
/// NOT in the source-code blob. Dimensions that score these (CI, IaC, README,
/// CHANGELOG, architecture docs, runbooks) must resolve the real artifact via
/// [`read_artifact_source`] instead of [`read_target_source`] — otherwise they
/// scan concatenated `.rs` as if it were YAML/Markdown and pass vacuously.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtifactClass {
    /// `README*` at any depth.
    Readme,
    /// `CHANGELOG*` / `HISTORY*` / `RELEASES*`.
    Changelog,
    /// Architecture docs: `ARCHITECTURE*`, `docs/**/adr/**`, `*ADR*.md`, `*.mmd`.
    ArchDoc,
    /// CI/CD pipelines: `.github/workflows/**`, `.gitlab-ci.yml`, `.circleci/**`,
    /// `azure-pipelines.yml`, `Jenkinsfile`.
    CiWorkflow,
    /// Container build: `Dockerfile*`, `*.dockerfile`, `docker-compose*.y*ml`.
    Dockerfile,
    /// Infrastructure-as-Code: `*.tf`, `*.tfvars`, `*.bicep`, k8s `*.y*ml` under `k8s/`/`deploy/`.
    Iac,
    /// Incident runbooks: `RUNBOOK*`, `docs/runbooks/**`, `docs/**/incident*`.
    Runbook,
    /// Build/dependency manifest: `Cargo.toml`, `package.json`, `pyproject.toml`,
    /// `go.mod`, `build.gradle`, `setup.py`, `pom.xml`.
    Manifest,
    /// Runtime configuration files whose SECURITY posture matters (F2.6):
    /// `*.yaml`/`*.yml`, `.env*`, `*.ini`, `*.properties`, `*.cfg`, `*.conf`,
    /// non-manifest `*.toml`. Distinct from [`ArtifactClass::Manifest`]
    /// (dependency graph) and [`ArtifactClass::Iac`] (infra provisioning).
    Config,
}

impl ArtifactClass {
    /// Whether the ABSENCE of this artifact from a directory scope is a genuine
    /// defect (→ [`ARTIFACT_ABSENT_CAP`]) versus merely project-type-dependent
    /// (→ provisional pass, becomes `NotApplicable` in the composite after W3).
    ///
    /// Universal (every serious repo should ship one): README, CHANGELOG,
    /// architecture docs, CI pipeline. Conditional (a pure library legitimately
    /// has none): IaC, Dockerfile, deploy configs, runbooks, manifests. Capping
    /// a conditional artifact would repeat the F3.3-Playwright false positive.
    pub fn absent_is_defect(self) -> bool {
        matches!(
            self,
            ArtifactClass::Readme
                | ArtifactClass::Changelog
                | ArtifactClass::ArchDoc
                | ArtifactClass::CiWorkflow
        )
    }

    /// Whether an *absent* artifact of this class should be resolved by walking
    /// UP to the repository/workspace root and inheriting the workspace-level
    /// artifact, instead of being capped as a per-scope defect.
    ///
    /// CHANGELOG, architecture docs and CI pipelines are **workspace-wide** in a
    /// monorepo — one CHANGELOG, one CI, one ARCHITECTURE cover every member
    /// crate — so a member that lacks its own legitimately inherits the root's,
    /// and the workspace is credited for shipping them. `Readme` is deliberately
    /// excluded: each crate should carry its own (Rust convention), so its
    /// absence stays a per-crate defect. This is the crate/module/path counterpart
    /// to [`is_repo_or_larger`](crate::scope::ScopeKind::is_repo_or_larger): the
    /// artifact is *inherited* from the enclosing repo, never dropped.
    pub fn inherits_from_repo_root(self) -> bool {
        matches!(
            self,
            ArtifactClass::Changelog | ArtifactClass::ArchDoc | ArtifactClass::CiWorkflow
        )
    }

    /// Does `path` (a file) belong to this artifact class? Matches on file name
    /// and, where relevant, an ancestor directory component.
    fn matches(self, path: &Path) -> bool {
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        let full = path.to_string_lossy().to_ascii_lowercase();
        match self {
            ArtifactClass::Readme => name.starts_with("readme"),
            ArtifactClass::Changelog => {
                name.starts_with("changelog")
                    || name.starts_with("history")
                    || name.starts_with("releases")
            }
            ArtifactClass::ArchDoc => {
                name.starts_with("architecture")
                    || name.ends_with(".mmd")
                    || full.contains("/adr/")
                    || (name.contains("adr") && name.ends_with(".md"))
            }
            ArtifactClass::CiWorkflow => {
                full.contains("/.github/workflows/")
                    || name == ".gitlab-ci.yml"
                    || full.contains("/.circleci/")
                    || name == "azure-pipelines.yml"
                    || name == "jenkinsfile"
            }
            ArtifactClass::Dockerfile => {
                name.starts_with("dockerfile")
                    || name.ends_with(".dockerfile")
                    || name.starts_with("docker-compose")
            }
            ArtifactClass::Iac => {
                name.ends_with(".tf")
                    || name.ends_with(".tfvars")
                    || name.ends_with(".bicep")
                    || ((full.contains("/k8s/") || full.contains("/deploy/"))
                        && (name.ends_with(".yml") || name.ends_with(".yaml")))
            }
            ArtifactClass::Runbook => {
                name.starts_with("runbook")
                    || full.contains("/runbooks/")
                    || (full.contains("incident") && name.ends_with(".md"))
            }
            ArtifactClass::Manifest => matches!(
                name.as_str(),
                "cargo.toml"
                    | "package.json"
                    | "pyproject.toml"
                    | "go.mod"
                    | "build.gradle"
                    | "build.gradle.kts"
                    | "setup.py"
                    | "pom.xml"
            ),
            ArtifactClass::Config => {
                let is_manifest = matches!(
                    name.as_str(),
                    "cargo.toml" | "pyproject.toml" | "package.json" | "pom.xml"
                );
                !is_manifest
                    && (name.ends_with(".yaml")
                        || name.ends_with(".yml")
                        || name.ends_with(".ini")
                        || name.ends_with(".properties")
                        || name.ends_with(".cfg")
                        || name.ends_with(".conf")
                        || name.ends_with(".toml")
                        || name.starts_with(".env")
                        || name == "env"
                        || name.ends_with(".env"))
            }
        }
    }
}

/// Standard `(score, evidence)` for a dimension whose [`ArtifactClass`] is
/// entirely absent from the scope. Universal artifacts (see
/// [`ArtifactClass::absent_is_defect`]) are capped at [`ARTIFACT_ABSENT_CAP`];
/// conditional artifacts return `1.0` provisionally with honest evidence
/// (they become `NotApplicable` in the composite after W3), so a pure library
/// is not penalised for lacking IaC/Dockerfile/deploy/runbook/manifest.
pub fn absent_artifact_score(dim: &str, class: ArtifactClass) -> (f32, String) {
    if class.absent_is_defect() {
        (
            ARTIFACT_ABSENT_CAP,
            format!(
                "{dim}: no matching artifact found in scope — repository lacks it \
                 (absent-artifact cap, not diluted density)"
            ),
        )
    } else {
        (
            1.0,
            format!(
                "[N/A] {dim}: no matching artifact in scope (conditional artifact — \
                 does not apply to this project type; excluded from composite)"
            ),
        )
    }
}

/// Whether an evidence string produced by [`absent_artifact_score`] marks a
/// NotApplicable (conditional-artifact-absent) result. Producer/consumer share
/// this `[N/A]` sentinel so a verifier's `check()` can promote the status to
/// [`DimStatus::NotApplicable`] (excluded from the composite) instead of a
/// misleading `Pass`.
///
/// `pub(crate)` desde 07/08/2026: com `check()` defaultado, [`finish`] é o único
/// consumidor — nenhum arquivo fora deste módulo a nomeia (verificado por grep).
pub(crate) fn evidence_marks_not_applicable(evidence: &str) -> bool {
    evidence.starts_with("[N/A]")
}

/// Resolve the on-disk files of a given [`ArtifactClass`] under `target`.
///
/// * **File target** → `[target]` verbatim (an explicit file the caller pointed
///   at is always used, regardless of name — preserves `score README.md`).
/// * **Directory target** → every descendant matching `class.matches`, skipping
///   `SKIP_DIRS` and dot-dirs **except** those an artifact class needs
///   (`.github`, `.circleci`). Deterministically sorted.
pub fn resolve_artifacts(target: &Path, class: ArtifactClass) -> Vec<PathBuf> {
    if !target.is_dir() {
        return vec![target.to_path_buf()];
    }
    let mut files = Vec::new();
    let mut stack = vec![target.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in rd.flatten() {
            let p = entry.path();
            if p.is_dir() {
                let dname = p.file_name().and_then(|n| n.to_str()).unwrap_or_default();
                // Artifact resolution needs a few dot-dirs the source walk skips.
                let allowed_dot = matches!(dname, ".github" | ".circleci");
                let skip = (dname.starts_with('.') && !allowed_dot) || SKIP_DIRS.contains(&dname);
                if !skip {
                    stack.push(p);
                }
            } else if class.matches(&p) {
                files.push(p);
            }
        }
    }
    files.sort();
    files
}

/// Whether `p` is a source-code file (extension ∈ `SOURCE_EXTS`). Used to keep
/// a per-file artifact sweep from scoring `foo.rs` AS a changelog/CI/README/
/// arch-doc — a `.rs`/`.py`/… file is code, never a repo artifact.
fn is_source_file(p: &Path) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| SOURCE_EXTS.contains(&e))
}

/// Read the source of a repository artifact for `class` under `target`.
///
/// Returns `(concatenated_source, present)`. `present == false` means the
/// directory scope contains NO artifact of this class — the caller applies the
/// [`ARTIFACT_ABSENT_CAP`] step-function instead of a diluted density score.
/// A file target is always `present == true` (read verbatim). Concatenation is
/// bounded by `DIR_SCAN_BYTE_CAP`.
pub fn read_artifact_source(target: &Path, class: ArtifactClass) -> (String, bool) {
    // Local resolution. A **source-code** file target (`.rs`/`.py`/… ∈
    // `SOURCE_EXTS`) is NEVER a repo artifact (changelog / CI / README /
    // arch-doc / runbook) — it falls through to inheritance/absence rather than
    // being scored AS one (the file-scope fidelity fix: a per-file sweep must not
    // read `foo.rs` as a changelog). A **non-source** file target (`CHANGELOG.md`,
    // `ci.yml`, `Cargo.toml`) IS the artifact and is read verbatim. A **directory**
    // target resolves its descendants. An empty local result falls through to
    // repo-root walk-up inheritance below.
    let (out, present) = if target.is_dir() {
        concat_artifacts(&resolve_artifacts(target, class))
    } else if is_source_file(target) {
        (String::new(), false)
    } else {
        match std::fs::read_to_string(target) {
            Ok(s) => (s, true),
            Err(_) => (String::new(), false),
        }
    };
    if present || !class.inherits_from_repo_root() {
        return (out, present);
    }
    // Walk-up inheritance (see [`ArtifactClass::inherits_from_repo_root`]): a
    // member crate/module/path lacking its own workspace-level artifact inherits
    // the repository root's CHANGELOG / architecture docs / CI. Bounded to the
    // nearest repo root (`.git` / `[workspace]` Cargo.toml) and resolved
    // SHALLOWLY there (depth ≤ 2 — `R/CHANGELOG.md`, `R/.github/workflows/**`,
    // `R/docs/**/adr`) so it never deep-walks every member subtree (the F2.6
    // walk-up over-scan lesson — an existence read, never a huge-file content scan).
    match find_repo_root(target) {
        Some(root) => concat_artifacts(&resolve_artifacts_shallow(&root, class, 2)),
        None => (out, present),
    }
}

/// Concatenate the source of resolved artifact files, bounded by
/// `DIR_SCAN_BYTE_CAP`. Returns `(source, present)` — `present` is `true` iff
/// at least one file was read.
fn concat_artifacts(paths: &[PathBuf]) -> (String, bool) {
    let mut out = String::new();
    let mut present = false;
    for p in paths {
        if let Ok(s) = std::fs::read_to_string(p) {
            present = true;
            out.push_str(&s);
            out.push('\n');
            if out.len() >= DIR_SCAN_BYTE_CAP {
                break;
            }
        }
    }
    (out, present)
}

/// Walk UP from `start` to the nearest repository/workspace root — a directory
/// containing `.git` or a `Cargo.toml` with a `[workspace]` table. Bounded by the
/// filesystem root; returns `None` if neither marker is found.
fn find_repo_root(start: &Path) -> Option<PathBuf> {
    // Canonicalize first: a *relative* target (e.g. `crates/foo`) walked via
    // `.parent()` terminates at `""`, and `"".join(".git")` would then resolve
    // against the CWD — falsely matching the workspace `.git`. An absolute path
    // walks up correctly to the filesystem root.
    let start = std::fs::canonicalize(start).ok()?;
    let mut cur = if start.is_dir() {
        Some(start.clone())
    } else {
        start.parent().map(Path::to_path_buf)
    };
    while let Some(dir) = cur {
        if dir.join(".git").exists() {
            return Some(dir);
        }
        if std::fs::read_to_string(dir.join("Cargo.toml"))
            .map(|s| s.contains("[workspace]"))
            .unwrap_or(false)
        {
            return Some(dir);
        }
        cur = dir.parent().map(Path::to_path_buf);
    }
    None
}

/// Depth-bounded variant of [`resolve_artifacts`] for repo-root walk-up
/// inheritance. The workspace-level universals live at canonical shallow
/// locations (`R/CHANGELOG.md` depth 0, `R/.github/workflows/**` and
/// `R/docs/**/adr` depth ≤ 2), so a `max_depth`-bounded scan finds them without
/// deep-walking every member crate subtree (slow + would pull per-member
/// artifacts as noise — the F2.6 over-scan lesson).
fn resolve_artifacts_shallow(root: &Path, class: ArtifactClass, max_depth: usize) -> Vec<PathBuf> {
    if !root.is_dir() {
        return vec![root.to_path_buf()];
    }
    let mut files = Vec::new();
    let mut stack = vec![(root.to_path_buf(), 0usize)];
    while let Some((dir, depth)) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in rd.flatten() {
            let p = entry.path();
            if p.is_dir() {
                if depth >= max_depth {
                    continue;
                }
                let dname = p.file_name().and_then(|n| n.to_str()).unwrap_or_default();
                let allowed_dot = matches!(dname, ".github" | ".circleci");
                let skip = (dname.starts_with('.') && !allowed_dot) || SKIP_DIRS.contains(&dname);
                if !skip {
                    stack.push((p, depth + 1));
                }
            } else if class.matches(&p) {
                files.push(p);
            }
        }
    }
    files.sort();
    files
}

/// Enumerate every polyglot source file (`SOURCE_EXTS`) under `root`, skipping
/// `SKIP_DIRS` and dot-dirs. Returns a globally-sorted, deterministic list of
/// file paths (**no byte cap** — the cap only bounds *concatenation* in
/// [`read_target_source`]). A single non-directory `root` returns `[root]`
/// verbatim, so File-scope resolution is uniform with directory scopes.
///
/// This is the file-set primitive the multi-scope harness (`crate::scope`) builds
/// on: the correct model is *score-per-file → aggregate*, not the legacy
/// *concatenate-then-score-once* (which sums complexity, truncates at 2 MiB, and
/// destroys per-file coverage ratios). Every file is enumerated — nothing is
/// dropped by a byte cap, closing the fail-open hole where a secret/CVE past the
/// 2 MiB mark would silently pass a BLOCK dimension.
pub fn enumerate_source_files(root: &Path) -> Vec<std::path::PathBuf> {
    if !root.is_dir() {
        return vec![root.to_path_buf()];
    }
    let mut files = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in rd.flatten() {
            let p = entry.path();
            if p.is_dir() {
                let skip = p
                    .file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with('.') || SKIP_DIRS.contains(&n));
                if !skip {
                    stack.push(p);
                }
            } else if p
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| SOURCE_EXTS.contains(&e))
            {
                // Vendored/minified bundles (`lunr.pt.min.js`, `app.min.css`…)
                // are machine-written single-line blobs, not project source —
                // jscpd and SonarQube exclude `**/*.min.*` for the same reason.
                let minified = p
                    .file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.contains(".min."));
                // NOTE: a content-based `@generated` header filter was tried here
                // (2026-08-02) and reverted: prose mentions ("Auto-generated test
                // for…") false-positived 85 real source files in one package —
                // and this enumeration feeds EVERY dim, so a false skip would
                // also blind the secrets/CVE scans. Generated-but-committed SDKs
                // therefore stay in scope; their duplication is visible debt.
                if !minified {
                    files.push(p);
                }
            }
        }
    }
    files.sort(); // global deterministic order → reproducible scope scores
    files
}

/// Durable wildcard: auto-categorise any DimId (including future F5+) by its
/// F-prefix family. Uses [`DimId::phase`] to pick the dominant pattern for
/// the family and [`DimId::as_str`] to disambiguate within-family
/// sub-patterns. Critical dims keep explicit overrides in [`auto_remediation`]
/// for verified accuracy; this heuristic handles everything else without
/// manual match updates.
///
/// Pattern families (from `~/.claude/rust/docs/2026-06-21-quality-remediation-patterns.md`):
/// - Phase 1 (F1.*) — Code Quality & Architecture → Pattern 1 (extract) / 2 (refactor)
/// - Phase 2 (F2.*) — Security & Performance → Pattern 3 (sanitise) / 5 (manifest) / 1 (perf)
/// - Phase 3 (F3.*) — Testing & Documentation → Pattern 6 (tests) / 7 (docs)
/// - Phase 4 (F4.*) — Best Practices & CI/CD → Pattern 3 / 5 / 7
/// - Phase 5+ (future) — Pattern 2 (safe default)
///
/// Split into per-phase helpers to keep each function's cyclomatic
/// complexity under the 15 threshold (entry: CC≈6, helpers: CC≤7).
fn infer_pattern_for_dim(dim: DimId) -> &'static str {
    let sub: Option<u8> = dim.as_str().split('.').nth(1).and_then(|s| s.parse().ok());
    match dim.phase() {
        1 => pattern_phase1(sub),
        2 => pattern_phase2(sub),
        3 => pattern_phase3(sub),
        4 => pattern_phase4(sub),
        // Phase 5+ (future) — safe default = free-form refactor
        _ => "Pattern 2",
    }
}

/// Phase 1 (F1.*) — Code Quality & Architecture.
fn pattern_phase1(sub: Option<u8>) -> &'static str {
    match sub {
        // F1.1, F1.2, F1.6 (complexity/maintainability/error handling) → extract
        Some(1) | Some(2) | Some(6) => "Pattern 1",
        // F1.3-5, F1.7-12 (duplication/SOLID/patterns/architecture/boundaries) → free-form
        Some(3..=5) | Some(7..=12) => "Pattern 2",
        // Future F1.x → safe default = extract
        _ => "Pattern 1",
    }
}

/// Phase 2 (F2.*) — Security & Performance.
fn pattern_phase2(sub: Option<u8>) -> &'static str {
    match sub {
        // F2.1-4, F2.6 (OWASP/input/authz/secrets/config) → sanitise hardcoded
        Some(1..=4) | Some(6) => "Pattern 3",
        // F2.5 (CVEs) → manifest update
        Some(5) => "Pattern 5",
        // F2.7-9 (DB perf, Memory, Caching) → extract/refactor
        Some(7..=9) => "Pattern 1",
        // F2.10 (I/O) + F2.11 (Concurrency) → free-form refactor
        Some(10..=11) => "Pattern 2",
        // F2.12-13 (Frontend, Scalability) → doc/infra generation
        Some(12..=13) => "Pattern 7",
        // Future F2.x → safe default = free-form
        _ => "Pattern 2",
    }
}

/// Phase 3 (F3.*) — Testing & Documentation.
fn pattern_phase3(sub: Option<u8>) -> &'static str {
    match sub {
        // F3.1-7 (test quality/coverage/pyramid/etc.) → test stub
        Some(1..=7) => "Pattern 6",
        // F3.8-13 (inline/api/arch docs/readme/changelog) → doc generation
        // + future F3.x → safe default = docs
        _ => "Pattern 7",
    }
}

/// Phase 4 (F4.*) — Best Practices & CI/CD.
fn pattern_phase4(sub: Option<u8>) -> &'static str {
    match sub {
        // F4.1, F4.4 (idioms, modernization) → refactor
        Some(1) | Some(4) => "Pattern 1",
        // F4.2 (frameworks) → free-form refactor
        Some(2) => "Pattern 2",
        // F4.3 (deprecated APIs) → sanitise (replace with non-deprecated)
        Some(3) => "Pattern 3",
        // F4.5, F4.6 (pkg mgmt, build config) → manifest update
        Some(5..=6) => "Pattern 5",
        // F4.7-12 (CI/CD, deploy, IaC, monitoring, incident, env) → doc/infra
        Some(7..=12) => "Pattern 7",
        // Future F4.x → safe default = free-form
        _ => "Pattern 2",
    }
}

/// Build the canonical suggestion for a dim, interpolating the target path and
/// pointing to the patterns catalog. Returns a string suitable for
/// `DimScore.suggestions`.
///
/// **Always returns a suggestion** — for Warn/Fail it's a fix command; for Pass
/// it's a preventive "tighten" command that hardens the dim against future
/// regression. Guarantees the 50 verifiers report a non-empty `suggestions`
/// for every diagnostic run (2026-06-21 session).
///
/// Replaces the previous PLANNED (W7) hardcoded `Edit tool`
/// stubs. All 50 dims now route through `Edit tool` (REGRA #2
/// canonical workflows) with the dim-specific pattern from
/// `~/.claude/rust/docs/2026-06-21-quality-remediation-patterns.md`.
///
/// Pattern mapping (50 dims → 7 patterns; see central catalog):
/// - Pattern 1 (extract / refactor):    F1.1, F1.2, F1.6, F4.1, F4.4
/// - Pattern 2 (free-form refactor):    F1.3, F1.4, F1.5, F1.7, F1.8, F1.9, F1.10, F1.11, F1.12, F2.11, F4.2
/// - Pattern 3 (remove hardcoded):      F2.1, F2.2, F2.3, F2.4, F2.6, F4.3
/// - Pattern 5 (manifest update):        F2.5, F4.5, F4.6
/// - Pattern 6 (test stub):             F3.1, F3.2, F3.3, F3.4, F3.5, F3.6, F3.7
/// - Pattern 7 (doc / infra generation): F2.12, F2.13, F3.8, F3.9, F3.10, F3.11, F3.12, F3.13, F4.7, F4.8, F4.9, F4.10, F4.11, F4.12
///
/// `pub(crate)` desde 07/08/2026: com `check()` defaultado no trait, [`finish`]
/// é o ÚNICO chamador — os 5 verificadores que a invocavam montavam `DimScore` à
/// mão em guards de retorno antecipado, e passaram a devolver `(score, evidence)`
/// como todos os outros. Manter `pub` exportaria um símbolo sem consumidor
/// externo; a visibilidade agora descreve o uso real.
pub(crate) fn auto_remediation(dim: DimId, target: &Path, status: DimStatus) -> String {
    let pattern = match dim {
        // Pattern 1 — extract / refactor (single fn / idiomatic fix)
        DimId::F1_1
        | DimId::F1_2
        | DimId::F1_6
        | DimId::F2_7
        | DimId::F2_8
        | DimId::F2_9
        | DimId::F4_1
        | DimId::F4_4 => "Pattern 1",
        // Pattern 2 — free-form refactor (trait extraction, alignment)
        DimId::F1_3
        | DimId::F1_4
        | DimId::F1_5
        | DimId::F1_7
        | DimId::F1_8
        | DimId::F1_9
        | DimId::F1_10
        | DimId::F1_11
        | DimId::F1_12
        | DimId::F2_11
        | DimId::F4_2 => "Pattern 2",
        // Pattern 3 — remove hardcoded / sanitise (injection, secrets, config)
        DimId::F2_1 | DimId::F2_2 | DimId::F2_3 | DimId::F2_4 | DimId::F2_6 | DimId::F4_3 => {
            "Pattern 3"
        }
        // Pattern 5 — manifest update (Cargo.toml / package.json)
        DimId::F2_5 | DimId::F4_5 | DimId::F4_6 => "Pattern 5",
        // Pattern 6 — test stub generation
        DimId::F3_1
        | DimId::F3_2
        | DimId::F3_3
        | DimId::F3_4
        | DimId::F3_5
        | DimId::F3_6
        | DimId::F3_7 => "Pattern 6",
        // Pattern 7 — doc / infra / new-file generation
        DimId::F2_12
        | DimId::F2_13
        | DimId::F3_8
        | DimId::F3_9
        | DimId::F3_10
        | DimId::F3_11
        | DimId::F3_12
        | DimId::F3_13
        | DimId::F4_7
        | DimId::F4_8
        | DimId::F4_9
        | DimId::F4_10
        | DimId::F4_11
        | DimId::F4_12 => "Pattern 7",
        // Durable wildcard: any future DimId (e.g. F5_1) auto-categorised by
        // phase + sub heuristic (see [`infer_pattern_for_dim`]) — no manual
        // match update needed when new dims are added.
        _ => infer_pattern_for_dim(dim),
    };
    match status {
        // Pass — already good; suggest preventive tightening to keep score stable.
        // Pattern placeholders are <hardening> / <maintain> (defensive, optional).
        DimStatus::Pass => format!(
            "Edit tool --path {} --operation ssr --pattern '<hardening>' --replacement '<maintain>' (see ~/.claude/rust/docs/2026-06-21-quality-remediation-patterns.md {}) [preventive — score is Pass; tighten pattern to harden against regression]",
            target.display(),
            pattern
        ),
        // Warn / Fail — actual gap; suggest fix.
        // Pattern placeholders are <gap> / <fix> (concrete, required).
        _ => format!(
            "Edit tool --path {} --operation ssr --pattern '<gap>' --replacement '<fix>' (see ~/.claude/rust/docs/2026-06-21-quality-remediation-patterns.md {})",
            target.display(),
            pattern
        ),
    }
}

/// Fold the shared `check()` tail every verifier ends with: derive the status
/// (`NotApplicable` when `evidence` carries the conditional-artifact `[N/A]`
/// sentinel — see [`evidence_marks_not_applicable`] — otherwise Pass/Warn/Fail
/// from the score), attach the canonical remediation suggestion, and stamp a
/// zero latency. Single definition that replaced the ~7-line tail duplicated
/// across all 50 verifiers (dedup, F1.3). Behaviour-preserving: the 44 plain
/// verifiers never emit the `[N/A]` sentinel, and the 6 conditional ones already
/// derived their status by this exact rule.
pub fn finish(id: DimId, value: f32, evidence: String, target: &Path) -> DimScore {
    let status = if evidence_marks_not_applicable(&evidence) {
        DimStatus::NotApplicable
    } else {
        DimStatus::from_score(value)
    };
    let suggestions = vec![auto_remediation(id, target, status)];
    DimScore {
        value,
        status,
        evidence,
        suggestions,
        latency_ms: 0,
        truncated: false,
    }
}

/// Whether `target` is the detector's OWN source (or a test/bench file). Such
/// files embed detection needle-vocabulary as literal data, so scoring them is
/// a self-match false positive that must be allowlisted. This is the single
/// shared definition the content verifiers delegate to — it replaced 35
/// byte-identical per-file copies (dedup, F1.3). Gated to `workspace-integration`
/// (the only config whose real engines carry the needle vocabulary).
#[cfg(feature = "workspace-integration")]
pub(crate) fn is_detector_own_source(target: &Path) -> bool {
    let canonical = target.canonicalize();
    let p = canonical
        .as_deref()
        .map(|c| c.to_string_lossy())
        .unwrap_or_else(|_| target.to_string_lossy());
    const NON_PRODUCTION_DIRS: [&str; 4] = ["/tests/", "/test/", "/benches/", "/bench/"];
    if NON_PRODUCTION_DIRS.iter().any(|s| p.contains(s)) {
        return true;
    }
    const DETECTOR_SOURCES: [&str; 3] = [
        "touring-quality/src",
        "touring-quality/tests",
        "touring-analysis/src/quality",
    ];
    DETECTOR_SOURCES.iter().any(|s| p.contains(s))
}

/// Sufixo `"; top: <smell> (<n>x)"` do achado mais frequente — vazio se não há.
///
/// Callers: `crate::verifications::top_finding(&r.findings)`.
///
/// Consolida 35 cópias byte-a-byte espalhadas por `verifications/f*.rs` (F1.3
/// dedup, 2026-08-07), no mesmo espírito de [`lang_from_ext`] logo abaixo. As
/// cópias vinham em DUAS grafias do mesmo `format!` — 27 com captura inline
/// (`{m}`/`{c}`) e 8 posicionais (`{}`, m, c) —, e era justamente por isso que o
/// detector Type-1 do F1.3 enxergava 27 e não 35: idênticas em significado,
/// diferentes em bytes. Unificar remove a duplicação E a inconsistência.
///
/// Monomórfico de propósito: todos os analisadores de `touring-analysis` expõem
/// `findings: Vec<(String, usize)>`, então genéricos não pagariam o seu custo.
#[cfg(feature = "workspace-integration")]
pub(crate) fn top_finding(findings: &[(String, usize)]) -> String {
    findings
        .first()
        .map(|(m, c)| format!("; top: {m} ({c}x)"))
        .unwrap_or_default()
}

/// Map a file extension to the language string the content-based verifiers understand
/// (polyglot, 7-language superset: Python / TypeScript / JavaScript / Go / Java / C / C++).
///
/// Callers: `crate::verifications::lang_from_ext(target)`.
///
/// This consolidates 42 byte-similar per-file copies that existed across every
/// verifier in `verifications/f*.rs` (F1.3 dedup, 2026-07-02). The 7-language body
/// is the canonical superset — 23 files previously had a 4-lang variant (py/ts/js/go
/// only); unifying to 7-lang is behaviour-IMPROVING for `.java`/`.c`/`.cpp` targets
/// and behaviour-PRESERVING for `.rs` (primary workspace language).
///
/// Gated to `workspace-integration` — identical gate to the 42 callers.
#[cfg(feature = "workspace-integration")]
pub(crate) fn lang_from_ext(target: &Path) -> &'static str {
    match target.extension().and_then(|e| e.to_str()) {
        Some("py") => "python",
        Some("ts") | Some("tsx") => "typescript",
        Some("js") | Some("jsx") | Some("mjs") | Some("cjs") => "javascript",
        Some("go") => "go",
        Some("java") => "java",
        Some("c") | Some("h") => "c",
        Some("cpp") | Some("cc") | Some("cxx") | Some("hpp") | Some("hh") => "cpp",
        Some("html") | Some("htm") => "html",
        // Default to rust (the primary workspace language).
        _ => "rust",
    }
}

/// Locate a byte span in `src`, returning `(line_number, excerpt)`.
///
/// Gate messages used to report only a COUNT ("1 vulnerability match(es)"),
/// which forces the author to bisect the file by hand to find what matched —
/// three times over on 2026-08-02, each a false positive. Naming the line and
/// the text turns a blocked write into an actionable one.
///
/// `redact` replaces the matched text with its shape (`<32 chars redacted>`)
/// instead of the value. Required for secret dims: echoing the match would
/// print the very credential the gate is protecting, into logs and CI output.
///
/// The excerpt is the trimmed source line, capped at 120 chars.
#[cfg(feature = "workspace-integration")]
pub(crate) fn locate_span(src: &str, span: (usize, usize), redact: bool) -> (usize, String) {
    const MAX_EXCERPT: usize = 120;
    let (start, end) = (span.0.min(src.len()), span.1.min(src.len()));
    let line_no = src[..start].bytes().filter(|&b| b == b'\n').count() + 1;

    if redact {
        return (
            line_no,
            format!("<{} chars redacted>", end.saturating_sub(start)),
        );
    }

    // Widen to the enclosing line, then trim and cap.
    let line_start = src[..start].rfind('\n').map_or(0, |i| i + 1);
    let line_end = src[start..].find('\n').map_or(src.len(), |i| start + i);
    let line = src[line_start..line_end].trim();
    let excerpt = if line.chars().count() > MAX_EXCERPT {
        let cut: String = line.chars().take(MAX_EXCERPT).collect();
        format!("{cut}…")
    } else {
        line.to_string()
    };
    (line_no, excerpt)
}

/// Run a single verification by dim id — uses a static dispatch table to keep CC low.
pub fn run_verification(dim: DimId, target: &Path) -> Result<DimScore> {
    use std::sync::OnceLock;
    static TABLE: OnceLock<std::collections::HashMap<DimId, Box<dyn Verification>>> =
        OnceLock::new();
    let table = TABLE.get_or_init(|| {
        let mut m: std::collections::HashMap<DimId, Box<dyn Verification>> =
            std::collections::HashMap::new();
        m.insert(DimId::F1_1, Box::new(f1_1_complexity::F1_1_Complexity));
        m.insert(
            DimId::F1_2,
            Box::new(f1_2_maintainability::F1_2_Maintainability),
        );
        m.insert(DimId::F1_3, Box::new(f1_3_duplication::F1_3_Duplication));
        m.insert(DimId::F1_4, Box::new(f1_4_solid::F1_4_Solid));
        m.insert(DimId::F1_5, Box::new(f1_5_tech_debt::F1_5_TechDebt));
        m.insert(
            DimId::F1_6,
            Box::new(f1_6_error_handling::F1_6_ErrorHandling),
        );
        m.insert(DimId::F1_7, Box::new(f1_7_boundaries::F1_7_Boundaries));
        m.insert(DimId::F1_8, Box::new(f1_8_dep_cycles::F1_8_DepCycles));
        m.insert(DimId::F1_9, Box::new(f1_9_api_design::F1_9_ApiDesign));
        m.insert(DimId::F1_10, Box::new(f1_10_data_model::F1_10_DataModel));
        m.insert(DimId::F1_11, Box::new(f1_11_patterns::F1_11_Patterns));
        m.insert(
            DimId::F1_12,
            Box::new(f1_12_arch_consistency::F1_12_ArchConsistency),
        );
        m.insert(DimId::F2_1, Box::new(f2_1_owasp::F2_1_Owasp));
        m.insert(
            DimId::F2_2,
            Box::new(f2_2_input_validation::F2_2_InputValidation),
        );
        m.insert(DimId::F2_3, Box::new(f2_3_authz::F2_3_Authz));
        m.insert(DimId::F2_4, Box::new(f2_4_secrets::F2_4_Secrets));
        m.insert(DimId::F2_5, Box::new(f2_5_dep_cves::F2_5_DepCves));
        m.insert(DimId::F2_6, Box::new(f2_6_config::F2_6_Config));
        m.insert(DimId::F2_7, Box::new(f2_7_db_perf::F2_7_DbPerf));
        m.insert(DimId::F2_8, Box::new(f2_8_memory::F2_8_Memory));
        m.insert(DimId::F2_9, Box::new(f2_9_caching::F2_9_Caching));
        m.insert(DimId::F2_10, Box::new(f2_10_io::F2_10_Io));
        m.insert(DimId::F2_11, Box::new(f2_11_concurrency::F2_11_Concurrency));
        m.insert(DimId::F2_12, Box::new(f2_12_frontend::F2_12_Frontend));
        m.insert(DimId::F2_13, Box::new(f2_13_scalability::F2_13_Scalability));
        m.insert(DimId::F3_1, Box::new(f3_1_coverage::F3_1_Coverage));
        m.insert(DimId::F3_2, Box::new(f3_2_test_quality::F3_2_TestQuality));
        m.insert(DimId::F3_3, Box::new(f3_3_test_pyramid::F3_3_TestPyramid));
        m.insert(DimId::F3_4, Box::new(f3_4_edge_cases::F3_4_EdgeCases));
        m.insert(DimId::F3_5, Box::new(f3_5_test_maint::F3_5_TestMaint));
        m.insert(DimId::F3_6, Box::new(f3_6_sec_tests::F3_6_SecTests));
        m.insert(DimId::F3_7, Box::new(f3_7_perf_tests::F3_7_PerfTests));
        m.insert(DimId::F3_8, Box::new(f3_8_inline_doc::F3_8_InlineDoc));
        m.insert(DimId::F3_9, Box::new(f3_9_api_doc::F3_9_ApiDoc));
        m.insert(DimId::F3_10, Box::new(f3_10_arch_doc::F3_10_ArchDoc));
        m.insert(DimId::F3_11, Box::new(f3_11_readme::F3_11_Readme));
        m.insert(
            DimId::F3_12,
            Box::new(f3_12_doc_accuracy::F3_12_DocAccuracy),
        );
        m.insert(DimId::F3_13, Box::new(f3_13_changelog::F3_13_Changelog));
        m.insert(DimId::F4_1, Box::new(f4_1_idioms::F4_1_Idioms));
        m.insert(DimId::F4_2, Box::new(f4_2_frameworks::F4_2_Frameworks));
        m.insert(DimId::F4_3, Box::new(f4_3_deprecated::F4_3_Deprecated));
        m.insert(
            DimId::F4_4,
            Box::new(f4_4_modernization::F4_4_Modernization),
        );
        m.insert(DimId::F4_5, Box::new(f4_5_pkg_mgmt::F4_5_PkgMgmt));
        m.insert(DimId::F4_6, Box::new(f4_6_build_config::F4_6_BuildConfig));
        m.insert(DimId::F4_7, Box::new(f4_7_cicd::F4_7_Cicd));
        m.insert(DimId::F4_8, Box::new(f4_8_deploy::F4_8_Deploy));
        m.insert(DimId::F4_9, Box::new(f4_9_iac::F4_9_Iac));
        m.insert(DimId::F4_10, Box::new(f4_10_monitoring::F4_10_Monitoring));
        m.insert(DimId::F4_11, Box::new(f4_11_incident::F4_11_Incident));
        m.insert(DimId::F4_12, Box::new(f4_12_env::F4_12_Env));
        m
    });
    let v = table
        .get(&dim)
        .ok_or_else(|| anyhow::anyhow!("no verifier for dim {}", dim))?;
    v.check(target)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// Mutation-testing trees are near-identical clones of the source by
    /// construction — enumerating them made F1.3 report 51% duplication on a
    /// package whose real src/ duplication was 3.1% (2026-08-02).
    #[test]
    fn test_enumerate_skips_mutation_testing_dirs() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        std::fs::write(root.join("real.py"), "x = 1\n").expect("write");
        for skip in ["mutants", "mutants.out"] {
            let d = root.join(skip).join("src");
            std::fs::create_dir_all(&d).expect("mkdir");
            std::fs::write(d.join("clone.py"), "x = 1\n").expect("write");
        }
        let files = enumerate_source_files(root);
        assert_eq!(files, vec![root.join("real.py")]);
    }

    /// Vendored minified bundles are machine-written blobs, not project source
    /// (jscpd/SonarQube exclude `**/*.min.*` for the same reason).
    #[test]
    fn test_enumerate_skips_minified_files() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        std::fs::write(root.join("app.js"), "let a = 1;\n").expect("write");
        std::fs::write(root.join("lunr.pt.min.js"), "!function(){}();\n").expect("write");
        let files = enumerate_source_files(root);
        assert_eq!(files, vec![root.join("app.js")]);
    }

    #[test]
    fn test_artifact_class_matches_by_name() {
        assert!(ArtifactClass::Readme.matches(Path::new("/x/README.md")));
        assert!(ArtifactClass::Readme.matches(Path::new("/x/readme.rst")));
        assert!(!ArtifactClass::Readme.matches(Path::new("/x/src/main.rs")));
        assert!(ArtifactClass::Changelog.matches(Path::new("/x/CHANGELOG.md")));
        assert!(ArtifactClass::CiWorkflow.matches(Path::new("/x/.github/workflows/ci.yml")));
        assert!(!ArtifactClass::CiWorkflow.matches(Path::new("/x/src/ci.rs")));
        assert!(ArtifactClass::Iac.matches(Path::new("/x/infra/main.tf")));
        assert!(ArtifactClass::Manifest.matches(Path::new("/x/Cargo.toml")));
        assert!(ArtifactClass::Manifest.matches(Path::new("/x/pyproject.toml")));
        assert!(!ArtifactClass::Manifest.matches(Path::new("/x/config.toml")));
    }

    #[test]
    fn test_absent_universal_is_capped_conditional_is_not() {
        // Universal artifacts absent → hard cap (defect).
        let (score, _) = absent_artifact_score("F3.11", ArtifactClass::Readme);
        assert_eq!(score, ARTIFACT_ABSENT_CAP);
        assert!(score < 0.8, "absent README must fall below Gold floor");
        // Conditional artifacts absent → provisional pass (a library has none).
        let (score, _) = absent_artifact_score("F4.9", ArtifactClass::Iac);
        assert_eq!(
            score, 1.0,
            "absent IaC must not penalise a non-infra project"
        );
    }

    #[test]
    fn test_read_artifact_source_resolves_on_disk_not_code_blob() {
        // A directory with a real README but only code files otherwise must
        // resolve the README (not the concatenated .rs blob) for Readme class,
        // and report ABSENT for a class with no matching artifact.
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("lib.rs"), "fn f() {}\n").expect("write code");
        std::fs::write(
            dir.path().join("README.md"),
            "# Title\n## Usage\nx\n## License\nMIT\n",
        )
        .expect("write readme");

        let (readme_src, present) = read_artifact_source(dir.path(), ArtifactClass::Readme);
        assert!(present, "README present must be detected");
        assert!(
            readme_src.contains("## Usage"),
            "must read the real README, not the code blob"
        );
        assert!(
            !readme_src.contains("fn f()"),
            "must NOT include source code in the README artifact read"
        );

        // No CHANGELOG in the dir → absent.
        let (_, present) = read_artifact_source(dir.path(), ArtifactClass::Changelog);
        assert!(!present, "absent CHANGELOG must report present=false");
    }

    #[test]
    fn test_all_50_dims_runnable() {
        let target = PathBuf::from(".");
        for dim in DimId::ALL {
            let result = run_verification(*dim, &target);
            assert!(
                result.is_ok(),
                "dim {} should be runnable, got error: {:?}",
                dim,
                result.err()
            );
        }
    }

    #[test]
    fn test_infer_pattern_consistency() {
        // Verify that `infer_pattern_for_dim` produces the SAME pattern as
        // the explicit match in `auto_remediation` for all 50 known dims.
        // This catches drift between the explicit match (curated) and the
        // heuristic (durable for F5+).
        for dim in DimId::ALL {
            let inferred = infer_pattern_for_dim(*dim);
            // Expected from the explicit match in auto_remediation
            let expected = match dim {
                DimId::F1_1
                | DimId::F1_2
                | DimId::F1_6
                | DimId::F2_7
                | DimId::F2_8
                | DimId::F2_9
                | DimId::F4_1
                | DimId::F4_4 => "Pattern 1",
                DimId::F1_3
                | DimId::F1_4
                | DimId::F1_5
                | DimId::F1_7
                | DimId::F1_8
                | DimId::F1_9
                | DimId::F1_10
                | DimId::F1_11
                | DimId::F1_12
                | DimId::F2_11
                | DimId::F4_2 => "Pattern 2",
                DimId::F2_1
                | DimId::F2_2
                | DimId::F2_3
                | DimId::F2_4
                | DimId::F2_6
                | DimId::F4_3 => "Pattern 3",
                DimId::F2_5 | DimId::F4_5 | DimId::F4_6 => "Pattern 5",
                DimId::F3_1
                | DimId::F3_2
                | DimId::F3_3
                | DimId::F3_4
                | DimId::F3_5
                | DimId::F3_6
                | DimId::F3_7 => "Pattern 6",
                DimId::F2_12
                | DimId::F2_13
                | DimId::F3_8
                | DimId::F3_9
                | DimId::F3_10
                | DimId::F3_11
                | DimId::F3_12
                | DimId::F3_13
                | DimId::F4_7
                | DimId::F4_8
                | DimId::F4_9
                | DimId::F4_10
                | DimId::F4_11
                | DimId::F4_12 => "Pattern 7",
                // F2.10 falls through to wildcard (current behavior = Pattern 2)
                _ => "Pattern 2",
            };
            assert_eq!(
                inferred,
                expected,
                "dim {} ({}) inference mismatch: got {}, expected {}",
                dim.as_str(),
                dim.short_name(),
                inferred,
                expected
            );
        }
    }

    #[test]
    fn test_infer_pattern_returns_valid_pattern() {
        // Sanity: every known dim maps to one of the 7 canonical patterns.
        let valid = [
            "Pattern 1",
            "Pattern 2",
            "Pattern 3",
            "Pattern 5",
            "Pattern 6",
            "Pattern 7",
        ];
        for dim in DimId::ALL {
            let p = infer_pattern_for_dim(*dim);
            assert!(
                valid.contains(&p),
                "dim {} inferred non-canonical pattern: {}",
                dim.as_str(),
                p
            );
        }
    }

    // ── Repo-root walk-up inheritance (F3.10/F3.13/F4.7 fidelity) ──────────────

    #[test]
    fn inherits_from_repo_root_classification() {
        // Workspace-level universals inherit from the enclosing repo root…
        assert!(ArtifactClass::Changelog.inherits_from_repo_root());
        assert!(ArtifactClass::ArchDoc.inherits_from_repo_root());
        assert!(ArtifactClass::CiWorkflow.inherits_from_repo_root());
        // …README stays per-crate; conditional artifacts never inherit.
        assert!(!ArtifactClass::Readme.inherits_from_repo_root());
        assert!(!ArtifactClass::Iac.inherits_from_repo_root());
        assert!(!ArtifactClass::Manifest.inherits_from_repo_root());
    }

    #[test]
    fn find_repo_root_stops_at_git_marker() {
        let root = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(root.path().join(".git")).unwrap();
        let nested = root.path().join("crates/a/src");
        std::fs::create_dir_all(&nested).unwrap();
        let found = find_repo_root(&nested).expect("repo root found via .git");
        assert_eq!(
            std::fs::canonicalize(&found).unwrap(),
            std::fs::canonicalize(root.path()).unwrap()
        );
    }

    #[test]
    fn walk_up_inherits_root_changelog_but_not_readme() {
        // Fixture: a repo root (marked by `.git`) shipping a CHANGELOG, holding a
        // member crate that has NEITHER a CHANGELOG nor a README of its own.
        let root = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(root.path().join(".git")).unwrap();
        std::fs::write(
            root.path().join("CHANGELOG.md"),
            "# Changelog\n## 1.0\n- init\n",
        )
        .unwrap();
        let crate_dir = root.path().join("crates/member");
        std::fs::create_dir_all(crate_dir.join("src")).unwrap();
        std::fs::write(crate_dir.join("src/lib.rs"), "pub fn x() {}\n").unwrap();

        // CHANGELOG inherits the root's via walk-up → present.
        let (src, present) = read_artifact_source(&crate_dir, ArtifactClass::Changelog);
        assert!(present, "member must inherit the repo-root CHANGELOG");
        assert!(src.contains("Changelog"), "inherited content, got {src:?}");

        // README does NOT inherit (per-crate concern) → stays absent.
        let (_, readme_present) = read_artifact_source(&crate_dir, ArtifactClass::Readme);
        assert!(
            !readme_present,
            "README must stay per-crate, never inherited from root"
        );
    }

    #[test]
    fn walk_up_absent_when_root_also_lacks_it() {
        // A repo root that genuinely lacks the artifact → member still absent
        // (walk-up inherits truthfully; it never fabricates presence).
        let root = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(root.path().join(".git")).unwrap();
        let crate_dir = root.path().join("crates/member");
        std::fs::create_dir_all(&crate_dir).unwrap();
        let (_, present) = read_artifact_source(&crate_dir, ArtifactClass::CiWorkflow);
        assert!(!present, "no CI anywhere → honestly absent, not fabricated");
    }

    #[test]
    fn file_scope_never_scores_source_as_artifact() {
        // A `.rs` source file scored for an artifact dim it does NOT match must
        // never be read AS that artifact (the file-scope fidelity fix).
        let root = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(root.path().join(".git")).unwrap();
        std::fs::write(root.path().join("CHANGELOG.md"), "# Changelog\n## 1.0\n").unwrap();
        let src = root.path().join("crates/m/src");
        std::fs::create_dir_all(&src).unwrap();
        let rs = src.join("lib.rs");
        std::fs::write(&rs, "pub fn x() {}\n").unwrap();

        // Inheritable class → walks up from the file's parent to the root CHANGELOG,
        // never reads lib.rs as a changelog.
        let (s, present) = read_artifact_source(&rs, ArtifactClass::Changelog);
        assert!(
            present && s.contains("Changelog"),
            "inherit root CHANGELOG, got {s:?}"
        );
        assert!(
            !s.contains("pub fn"),
            "must not read the .rs body as changelog"
        );

        // Conditional class (runbook) → honestly absent, never the .rs content.
        let (s2, p2) = read_artifact_source(&rs, ArtifactClass::Runbook);
        assert!(
            !p2 && !s2.contains("pub fn"),
            "must not read .rs as a runbook"
        );

        // An EXPLICIT artifact file still reads verbatim (`score CHANGELOG.md`).
        let cl = root.path().join("CHANGELOG.md");
        let (s3, p3) = read_artifact_source(&cl, ArtifactClass::Changelog);
        assert!(
            p3 && s3.contains("Changelog"),
            "explicit CHANGELOG.md reads verbatim"
        );
    }
}
