//! Multi-scope resolution for the 50-dimension quality harness.
//!
//! The harness historically scored a single target (`crate::score_target`). A
//! directory was handled by *concatenating* all source into one capped blob and
//! scoring it once — which sums complexity, truncates at 2 MiB (a secret past the
//! cap silently passes a BLOCK dim), and destroys per-file coverage ratios.
//!
//! This module introduces the correct model: a [`Scope`] resolves a target +
//! [`ScopeKind`] into a concrete **file-set**, which `crate::scope_report`
//! scores per file and rolls up per dimension by its
//! `crate::aggregate::AggKind`. The 50 dimensions remain present at **every**
//! granularity — file, module, path, feature, crate, repo, project, system,
//! workspace.

use crate::verifications::enumerate_source_files;
use anyhow::Result;
use std::fmt;
use std::path::{Path, PathBuf};
use std::str::FromStr;

/// The nine granularities at which the 50 dimensions can be scored.
///
/// `Project` and `System` reuse the same machinery as `Repo` and a glob-bounded
/// `Path` respectively (honest aliases) — the five distinct resolution
/// strategies underneath are: single file, Rust module-root (root file +
/// submodule directory), directory subtree, manifest-bounded crate/workspace,
/// and glob-set feature.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ScopeKind {
    /// A single source file (back-compat with `score_target`).
    File,
    /// A single Rust module: its root file (`foo.rs` or `foo/mod.rs`) together
    /// with its submodule directory (`foo/`). Finer than `Path` (it follows
    /// Rust module semantics, not a raw subtree) and richer than `File` (it
    /// pulls in the module's children). Explicit-only (`--scope module`) so
    /// bare-file/bare-dir auto-detection is unchanged.
    Module,
    /// An arbitrary directory subtree.
    Path,
    /// A changeset / feature slice = root + include/exclude globs (the git-free
    /// analog of SonarQube "new code" / Clean as You Code).
    Feature,
    /// A single Cargo crate (`[package]`): its file-set + its `Cargo.toml`.
    Crate,
    /// A repository root (`.git`/`README`): repo-level ScopeNative artifacts active.
    Repo,
    /// A project (≈ repo, or a named member set).
    Project,
    /// A coherent set of crates/modules (glob-bounded multi-crate).
    System,
    /// A Cargo `[workspace]` root: every member, workspace-wide graph dims.
    Workspace,
}

impl ScopeKind {
    /// Canonical lowercase string id.
    pub fn as_str(&self) -> &'static str {
        match self {
            ScopeKind::File => "file",
            ScopeKind::Module => "module",
            ScopeKind::Path => "path",
            ScopeKind::Feature => "feature",
            ScopeKind::Crate => "crate",
            ScopeKind::Repo => "repo",
            ScopeKind::Project => "project",
            ScopeKind::System => "system",
            ScopeKind::Workspace => "workspace",
        }
    }

    /// Repository-or-larger granularity, where repo-level ScopeNative artifacts
    /// (README / CI / CHANGELOG / Cargo.toml / IaC) are *directly* applicable.
    /// At finer scopes those dimensions are *inherited* from the nearest
    /// enclosing repo (never dropped — all 50 dims appear at every scope).
    pub fn is_repo_or_larger(&self) -> bool {
        matches!(
            self,
            ScopeKind::Crate
                | ScopeKind::Repo
                | ScopeKind::Project
                | ScopeKind::System
                | ScopeKind::Workspace
        )
    }

    /// All nine kinds in canonical granularity order.
    pub const ALL: &'static [ScopeKind] = &[
        ScopeKind::File,
        ScopeKind::Module,
        ScopeKind::Path,
        ScopeKind::Feature,
        ScopeKind::Crate,
        ScopeKind::Repo,
        ScopeKind::Project,
        ScopeKind::System,
        ScopeKind::Workspace,
    ];
}

impl fmt::Display for ScopeKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for ScopeKind {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        for k in ScopeKind::ALL {
            if k.as_str().eq_ignore_ascii_case(s.trim()) {
                return Ok(*k);
            }
        }
        Err(format!(
            "unknown scope kind: '{}'. Use one of: file|module|path|feature|crate|repo|project|system|workspace",
            s
        ))
    }
}

/// A resolved scope: its kind, root, and enumerated source file-set.
#[derive(Debug, Clone)]
pub struct Scope {
    /// The granularity.
    pub kind: ScopeKind,
    /// The root path the scope was resolved from.
    pub root: PathBuf,
    /// The concrete source files in this scope (deterministically sorted).
    pub files: Vec<PathBuf>,
}

impl Scope {
    /// Resolve a target + optional kind + include/exclude globs into a [`Scope`].
    ///
    /// * `kind = None` → auto-detect from the target (see [`detect_kind`]).
    /// * `include`/`exclude` → glob filters (relative to `root`, `/`-separated;
    ///   `*` matches within a path segment, `**` across segments). A non-empty
    ///   `include` makes the scope a `Feature` slice.
    pub fn resolve(
        target: &Path,
        kind: Option<ScopeKind>,
        include: &[String],
        exclude: &[String],
    ) -> Result<Scope> {
        let mut kind = kind.unwrap_or_else(|| detect_kind(target));
        if !include.is_empty() && kind != ScopeKind::Feature {
            // an explicit include-glob narrows any scope into a feature slice
            kind = ScopeKind::Feature;
        }
        let (root, mut files) = if kind == ScopeKind::Module {
            resolve_module_files(target)
        } else {
            (target.to_path_buf(), enumerate_source_files(target))
        };

        if !include.is_empty() || !exclude.is_empty() {
            files.retain(|f| {
                let rel = f
                    .strip_prefix(&root)
                    .unwrap_or(f)
                    .to_string_lossy()
                    .replace('\\', "/");
                let included = include.is_empty() || include.iter().any(|g| glob_match(g, &rel));
                let excluded = exclude.iter().any(|g| glob_match(g, &rel));
                included && !excluded
            });
        }
        Ok(Scope { kind, root, files })
    }

    /// Total physical lines of code across the scope's files.
    pub fn total_loc(&self) -> usize {
        self.files.iter().map(|f| file_loc(f)).sum()
    }

    /// Number of files in the scope.
    pub fn file_count(&self) -> usize {
        self.files.len()
    }
}

/// Resolve a Rust *module* target into its `(root, file-set)`.
///
/// A Rust module `foo` is rooted at either `foo.rs` or `foo/mod.rs`, and its
/// children live under `foo/`. Scoring "module foo" therefore means scoring the
/// module-root file **together with** its submodule directory — a set that
/// neither `File` (root file only) nor `Path` (directory only) captures. The
/// target may be pointed at in any of the three natural shapes:
///
/// * `P/foo.rs`     → `P/foo.rs` + `P/foo/**` (when the submodule dir exists),
/// * `P/foo/mod.rs` → `P/foo/**` (`mod.rs` roots its own directory),
/// * `P/foo/`       → `P/foo/**` + `P/foo.rs` (the root file, if present).
///
/// The returned `root` is the nearest common ancestor of the file-set so that
/// glob include/exclude stripping in [`Scope::resolve`] stays well-defined.
fn resolve_module_files(target: &Path) -> (PathBuf, Vec<PathBuf>) {
    // Directory target: the module directory subtree, plus the sibling root
    // file `P/foo.rs` when the module uses the flat (non-`mod.rs`) layout.
    if target.is_dir() {
        let mut files = enumerate_source_files(target);
        let root = match (target.parent(), target.file_name().and_then(|n| n.to_str())) {
            (Some(parent), Some(name)) => {
                let root_file = parent.join(format!("{name}.rs"));
                if root_file.is_file() {
                    files.push(root_file);
                    parent.to_path_buf() // both `foo.rs` and `foo/` live under parent
                } else {
                    target.to_path_buf()
                }
            }
            _ => target.to_path_buf(),
        };
        files.sort();
        files.dedup();
        return (root, files);
    }

    // File target.
    let parent = target.parent().unwrap_or_else(|| Path::new("."));
    let stem = target.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    if stem == "mod" {
        // `foo/mod.rs` roots the whole `foo/` directory (parent of `mod.rs`).
        let mut files = enumerate_source_files(parent);
        files.sort();
        files.dedup();
        return (parent.to_path_buf(), files);
    }
    // `P/foo.rs` (+ `P/foo/**` when the submodule directory exists).
    let mut files = vec![target.to_path_buf()];
    if !stem.is_empty() {
        let sibling = parent.join(stem);
        if sibling.is_dir() {
            files.extend(enumerate_source_files(&sibling));
        }
    }
    files.sort();
    files.dedup();
    // `root = parent` so both `foo.rs` and `foo/**` strip cleanly.
    (parent.to_path_buf(), files)
}

/// Auto-detect the scope kind for a target path.
///
/// file → `File`; dir with `[workspace]` → `Workspace`; `[package]` → `Crate`;
/// `.git`/`README` → `Repo`; otherwise `Path`. `Module` is **never**
/// auto-detected — it is opt-in via `--scope module` so bare-file/bare-dir
/// behaviour is unchanged.
pub fn detect_kind(target: &Path) -> ScopeKind {
    if !target.is_dir() {
        return ScopeKind::File;
    }
    if let Ok(s) = std::fs::read_to_string(target.join("Cargo.toml")) {
        if s.contains("[workspace]") {
            return ScopeKind::Workspace;
        }
        if s.contains("[package]") {
            return ScopeKind::Crate;
        }
    }
    if target.join(".git").exists() || target.join("README.md").exists() {
        return ScopeKind::Repo;
    }
    ScopeKind::Path
}

/// Physical line count of a single file (0 if unreadable).
pub fn file_loc(path: &Path) -> usize {
    std::fs::read_to_string(path)
        .map(|s| s.lines().count())
        .unwrap_or(0)
}

/// Minimal, dependency-free glob matcher (avoids adding `glob`/`globset` — D44).
///
/// Anchored full match against a `/`-separated path. Within a segment, `*`
/// matches any run of non-`/` chars and `?` a single non-`/` char; `**` as a
/// whole segment matches zero or more segments. Examples:
/// `crates/*/src/lib.rs`, `crates/touring-server/**`, `**/*.rs`.
pub fn glob_match(pattern: &str, text: &str) -> bool {
    let p: Vec<&str> = pattern.split('/').collect();
    let t: Vec<&str> = text.split('/').collect();
    match_segments(&p, &t)
}

fn match_segments(p: &[&str], t: &[&str]) -> bool {
    match p.first() {
        None => t.is_empty(),
        Some(&"**") => {
            // `**` matches zero or more whole segments.
            (0..=t.len()).any(|k| match_segments(&p[1..], &t[k..]))
        }
        Some(seg) => match t.first() {
            Some(first) if segment_match(seg, first) => match_segments(&p[1..], &t[1..]),
            _ => false,
        },
    }
}

/// Wildcard match within a single path segment (`*` = any run, `?` = one char).
fn segment_match(pat: &str, s: &str) -> bool {
    let pat: Vec<char> = pat.chars().collect();
    let s: Vec<char> = s.chars().collect();
    let (mut pi, mut si) = (0usize, 0usize);
    let (mut star, mut star_s): (Option<usize>, usize) = (None, 0);
    while si < s.len() {
        if pi < pat.len() && (pat[pi] == '?' || pat[pi] == s[si]) {
            pi += 1;
            si += 1;
        } else if pi < pat.len() && pat[pi] == '*' {
            star = Some(pi);
            star_s = si;
            pi += 1;
        } else if let Some(sp) = star {
            pi = sp + 1;
            star_s += 1;
            si = star_s;
        } else {
            return false;
        }
    }
    while pi < pat.len() && pat[pi] == '*' {
        pi += 1;
    }
    pi == pat.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scope_kind_roundtrip() {
        for k in ScopeKind::ALL {
            assert_eq!(ScopeKind::from_str(k.as_str()).unwrap(), *k);
        }
        assert!(ScopeKind::from_str("bogus").is_err());
    }

    #[test]
    fn repo_or_larger_classification() {
        assert!(ScopeKind::Workspace.is_repo_or_larger());
        assert!(ScopeKind::Crate.is_repo_or_larger());
        assert!(!ScopeKind::File.is_repo_or_larger());
        assert!(!ScopeKind::Path.is_repo_or_larger());
        assert!(!ScopeKind::Feature.is_repo_or_larger());
    }

    #[test]
    fn glob_within_segment() {
        assert!(glob_match("*.rs", "lib.rs"));
        assert!(glob_match("f?_*.rs", "f1_complexity.rs"));
        assert!(!glob_match("*.rs", "src/lib.rs")); // * does not cross '/'
        assert!(glob_match("src/lib.rs", "src/lib.rs"));
        assert!(!glob_match("src/lib.rs", "src/main.rs"));
    }

    #[test]
    fn glob_double_star_crosses_segments() {
        assert!(glob_match("crates/**", "crates/touring-server/src/lib.rs"));
        assert!(glob_match("**/*.rs", "crates/touring-quality/src/scope.rs"));
        assert!(glob_match("crates/**/src/*.rs", "crates/a/src/lib.rs"));
        assert!(glob_match("crates/**/src/*.rs", "crates/a/b/src/lib.rs"));
        assert!(!glob_match("crates/**/src/*.rs", "crates/a/src/sub/lib.rs"));
        assert!(glob_match("**", "anything/at/all.rs"));
    }

    #[test]
    fn detect_kind_on_file() {
        // A path that does not exist as a dir is treated as a File scope.
        assert_eq!(
            detect_kind(Path::new("nonexistent_file.rs")),
            ScopeKind::File
        );
    }

    #[test]
    fn resolve_single_file() {
        let tmp = std::env::temp_dir().join("tq_scope_test_one.rs");
        std::fs::write(&tmp, "fn main() {}\n").unwrap();
        let scope = Scope::resolve(&tmp, None, &[], &[]).unwrap();
        assert_eq!(scope.kind, ScopeKind::File);
        assert_eq!(scope.file_count(), 1);
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn resolve_dir_with_include_makes_feature() {
        let dir = std::env::temp_dir().join("tq_scope_feat");
        let _ = std::fs::create_dir_all(dir.join("a"));
        let _ = std::fs::create_dir_all(dir.join("b"));
        std::fs::write(dir.join("a/x.rs"), "fn a() {}\n").unwrap();
        std::fs::write(dir.join("b/y.rs"), "fn b() {}\n").unwrap();
        let scope = Scope::resolve(&dir, None, &["a/**".to_string()], &[]).unwrap();
        assert_eq!(scope.kind, ScopeKind::Feature);
        assert_eq!(scope.file_count(), 1, "only a/x.rs matches a/**");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn module_scope_string_and_classification() {
        assert_eq!(ScopeKind::from_str("module").unwrap(), ScopeKind::Module);
        assert_eq!(ScopeKind::Module.as_str(), "module");
        // A module is a *fine* granularity — it does not own repo-level artifacts
        // (README/CI/CHANGELOG); those are inherited from the enclosing repo.
        assert!(!ScopeKind::Module.is_repo_or_larger());
        assert!(ScopeKind::ALL.contains(&ScopeKind::Module));
    }

    #[test]
    fn module_file_target_pulls_in_submodule_dir() {
        // `P/foo.rs` + `P/foo/bar.rs` + `P/foo/baz.rs` — the flat module layout.
        let p = std::env::temp_dir().join("tq_mod_flat");
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(p.join("foo")).unwrap();
        std::fs::write(p.join("foo.rs"), "pub mod bar;\npub mod baz;\n").unwrap();
        std::fs::write(p.join("foo/bar.rs"), "pub fn bar() {}\n").unwrap();
        std::fs::write(p.join("foo/baz.rs"), "pub fn baz() {}\n").unwrap();

        let scope = Scope::resolve(&p.join("foo.rs"), Some(ScopeKind::Module), &[], &[]).unwrap();
        assert_eq!(scope.kind, ScopeKind::Module);
        assert_eq!(
            scope.file_count(),
            3,
            "module foo = foo.rs + foo/bar.rs + foo/baz.rs, got {:?}",
            scope.files
        );
        let _ = std::fs::remove_dir_all(&p);
    }

    #[test]
    fn module_file_target_without_sibling_is_just_the_file() {
        let p = std::env::temp_dir().join("tq_mod_solo");
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        std::fs::write(p.join("solo.rs"), "pub fn solo() {}\n").unwrap();

        let scope = Scope::resolve(&p.join("solo.rs"), Some(ScopeKind::Module), &[], &[]).unwrap();
        assert_eq!(scope.kind, ScopeKind::Module);
        assert_eq!(scope.file_count(), 1, "no sibling dir → just the file");
        let _ = std::fs::remove_dir_all(&p);
    }

    #[test]
    fn module_mod_rs_roots_its_directory() {
        // `P/foo/mod.rs` + `P/foo/bar.rs` — the `mod.rs` module layout.
        let p = std::env::temp_dir().join("tq_mod_modrs");
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(p.join("foo")).unwrap();
        std::fs::write(p.join("foo/mod.rs"), "pub mod bar;\n").unwrap();
        std::fs::write(p.join("foo/bar.rs"), "pub fn bar() {}\n").unwrap();

        let scope =
            Scope::resolve(&p.join("foo/mod.rs"), Some(ScopeKind::Module), &[], &[]).unwrap();
        assert_eq!(scope.kind, ScopeKind::Module);
        assert_eq!(
            scope.file_count(),
            2,
            "mod.rs roots foo/ → mod.rs + bar.rs, got {:?}",
            scope.files
        );
        let _ = std::fs::remove_dir_all(&p);
    }

    #[test]
    fn module_dir_target_includes_sibling_root_file() {
        // Pointing at the `P/foo/` directory should still pull the flat root
        // file `P/foo.rs` into the module scope.
        let p = std::env::temp_dir().join("tq_mod_dir");
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(p.join("foo")).unwrap();
        std::fs::write(p.join("foo.rs"), "pub mod bar;\n").unwrap();
        std::fs::write(p.join("foo/bar.rs"), "pub fn bar() {}\n").unwrap();

        let scope = Scope::resolve(&p.join("foo"), Some(ScopeKind::Module), &[], &[]).unwrap();
        assert_eq!(scope.kind, ScopeKind::Module);
        assert_eq!(
            scope.file_count(),
            2,
            "dir target foo/ + sibling foo.rs → 2 files, got {:?}",
            scope.files
        );
        let _ = std::fs::remove_dir_all(&p);
    }
}
