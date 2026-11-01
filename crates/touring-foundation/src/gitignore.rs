//! The ignore rules of a git working tree, applied the way git applies them.
//!
//! One implementation for every walker that must agree with `git ls-files`: the
//! symbol index (`touring_hooks_shared::index_policy`) and the touring-quality
//! corpus. Until 14/09/2026 neither looked at `.gitignore`, so the MkDocs build
//! under `site/` (231 MB, ignored by git) put 47.877 stale symbols in the index
//! and failed the workspace's F2.1 gate on a copy of a file that had long been
//! fixed at its source (cross-audit 14/09/2026, R2-2).
//!
//! The rules: `.git/info/exclude`, then every `.gitignore` from the root down to
//! the directory holding the path, a deeper file overriding a shallower one and
//! a later rule overriding an earlier one. Once a directory is ignored, nothing
//! below it comes back, whatever a deeper `!rule` says (git's own rule: it never
//! lists the contents of an excluded directory). The global excludes file
//! (`core.excludesFile`) is left out on purpose: it differs per machine, and
//! the index of a project must not.

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex};

use ignore::gitignore::{Gitignore, GitignoreBuilder};

/// The rule that ignored a path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IgnoredBy {
    /// Root-relative path the rule matched: the path itself or a directory above it.
    pub matched: String,
    /// Root-relative ignore file holding the rule.
    pub source: String,
    /// The rule as written in that file.
    pub rule: String,
}

impl std::fmt::Display for IgnoredBy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "`{}` is ignored by `{}` (rule `{}`)",
            self.matched, self.source, self.rule
        )
    }
}

/// The ignore rules of one working tree, read lazily and cached per directory.
#[derive(Debug, Clone)]
pub struct GitIgnoreRules {
    root: PathBuf,
    exclude: Option<Arc<Gitignore>>,
    per_dir: Arc<Mutex<HashMap<PathBuf, Option<Arc<Gitignore>>>>>,
}

impl GitIgnoreRules {
    /// The rules of the working tree rooted at `root`.
    #[must_use]
    pub fn for_root(root: &Path) -> Self {
        let root = &absolute(root);
        // `.git` as a FILE (a linked worktree) keeps its `info/exclude` in the
        // common git dir; only the plain layout is read.
        let exclude_file = root.join(".git").join("info").join("exclude");
        let exclude = exclude_file
            .is_file()
            .then(|| build_matcher(root, &exclude_file))
            .flatten();
        Self {
            root: root.to_path_buf(),
            exclude,
            per_dir: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// The rules of the working tree holding `path`: the nearest ancestor with a
    /// `.git` entry, else `path` itself (or its directory, for a file).
    #[must_use]
    pub fn for_path(path: &Path) -> Self {
        let path = absolute(path);
        let start = if path.is_file() {
            path.parent().unwrap_or(&path)
        } else {
            &path
        };
        let root = start
            .ancestors()
            .find(|dir| dir.join(".git").exists())
            .unwrap_or(start);
        Self::for_root(root)
    }

    /// The working-tree root these rules are read from.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Why an absolute path is ignored, or `None` when it is not (or lies
    /// outside the root, where these rules say nothing).
    #[must_use]
    pub fn ignored_abs(&self, abs: &Path, is_dir: bool) -> Option<IgnoredBy> {
        let abs = absolute(abs);
        self.ignored(abs.strip_prefix(&self.root).ok()?, is_dir)
    }

    /// Why a root-relative path is ignored, or `None` when it is not.
    #[must_use]
    pub fn ignored(&self, rel: &Path, is_dir: bool) -> Option<IgnoredBy> {
        let parts: Vec<&std::ffi::OsStr> = rel
            .components()
            .filter_map(|c| match c {
                Component::Normal(part) => Some(part),
                _ => None,
            })
            .collect();
        for depth in 1..=parts.len() {
            let prefix: PathBuf = parts[..depth].iter().collect();
            let prefix_is_dir = depth < parts.len() || is_dir;
            if let Some(hit) = self.decide(&prefix, &parts[..depth - 1], prefix_is_dir) {
                return Some(hit);
            }
        }
        None
    }

    /// The verdict on one path given the files that apply to it: `exclude`,
    /// then the `.gitignore` of the root and of every directory in `parents`.
    fn decide(
        &self,
        prefix: &Path,
        parents: &[&std::ffi::OsStr],
        is_dir: bool,
    ) -> Option<IgnoredBy> {
        let mut verdict: Option<IgnoredBy> = None;
        let mut apply = |matcher: &Gitignore, base: &Path| {
            let Ok(local) = prefix.strip_prefix(base) else {
                return;
            };
            let m = matcher.matched(local, is_dir);
            if let Some(glob) = m.inner() {
                verdict = m.is_ignore().then(|| IgnoredBy {
                    matched: prefix.to_string_lossy().into_owned(),
                    source: glob
                        .from()
                        .and_then(|f| f.strip_prefix(&self.root).ok())
                        .map_or_else(String::new, |f| f.to_string_lossy().into_owned()),
                    rule: glob.original().to_string(),
                });
            }
        };
        if let Some(exclude) = &self.exclude {
            apply(exclude, Path::new(""));
        }
        let mut dir = PathBuf::new();
        for depth in 0..=parents.len() {
            if depth > 0 {
                dir.push(parents[depth - 1]);
            }
            if let Some(matcher) = self.matcher_for(&dir) {
                apply(&matcher, &dir);
            }
        }
        verdict
    }

    /// The `.gitignore` of a root-relative directory, parsed once.
    fn matcher_for(&self, dir: &Path) -> Option<Arc<Gitignore>> {
        let mut cache = self
            .per_dir
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        cache
            .entry(dir.to_path_buf())
            .or_insert_with(|| {
                let file = self.root.join(dir).join(".gitignore");
                file.is_file()
                    .then(|| build_matcher(&self.root.join(dir), &file))
                    .flatten()
            })
            .clone()
    }
}

/// `path` made absolute against the working directory, symlinks untouched: a
/// walker that starts from `crates/x` and the rules read from the working tree
/// above it must spell every path the same way.
fn absolute(path: &Path) -> PathBuf {
    std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf())
}

/// A matcher for one ignore file whose patterns are relative to `base`. A
/// malformed line is skipped, as git skips it; an unreadable file yields none.
fn build_matcher(base: &Path, file: &Path) -> Option<Arc<Gitignore>> {
    let mut builder = GitignoreBuilder::new(base);
    if let Some(err) = builder.add(file)
        && err.is_io()
    {
        return None;
    }
    builder.build().ok().map(Arc::new)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tree(files: &[(&str, &str)]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        for (rel, body) in files {
            let path = dir.path().join(rel);
            fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
            fs::write(path, body).expect("write");
        }
        dir
    }

    fn ignored(rules: &GitIgnoreRules, rel: &str) -> Option<IgnoredBy> {
        rules.ignored(Path::new(rel), false)
    }

    #[test]
    fn an_anchored_directory_rule_covers_everything_below_it_and_only_at_the_root() {
        let dir = tree(&[(".gitignore", "/site/\ntarget/\n")]);
        let rules = GitIgnoreRules::for_root(dir.path());

        let hit = ignored(&rules, "site/docs/agentic-bench/run_bench.py").expect("ignored");
        assert_eq!(hit.matched, "site");
        assert_eq!(hit.source, ".gitignore");
        assert_eq!(hit.rule, "/site/");
        assert!(
            ignored(&rules, "docs/site/page.md").is_none(),
            "anchored to the root"
        );
        assert!(
            ignored(&rules, "crates/x/target/debug/y.rs").is_some(),
            "unanchored at any depth"
        );
        assert!(ignored(&rules, "src/target_utils.rs").is_none());
    }

    #[test]
    fn a_later_negation_takes_a_file_back_and_a_deeper_file_overrides_the_root() {
        let dir = tree(&[
            (".gitignore", "*.log\n!keep.log\n"),
            ("docs/.gitignore", "draft.md\n!late.log\n"),
        ]);
        let rules = GitIgnoreRules::for_root(dir.path());

        assert!(ignored(&rules, "app.log").is_some());
        assert!(ignored(&rules, "keep.log").is_none());
        assert_eq!(
            ignored(&rules, "docs/draft.md").map(|h| h.source),
            Some("docs/.gitignore".to_string())
        );
        assert!(
            ignored(&rules, "draft.md").is_none(),
            "a nested rule stays in its directory"
        );
        assert!(
            ignored(&rules, "docs/late.log").is_none(),
            "the deeper file wins"
        );
    }

    #[test]
    fn nothing_under_an_ignored_directory_comes_back() {
        let dir = tree(&[
            (".gitignore", "/site/\n"),
            ("site/.gitignore", "!page.md\n"),
        ]);
        let rules = GitIgnoreRules::for_root(dir.path());
        assert!(ignored(&rules, "site/page.md").is_some());
    }

    #[test]
    fn info_exclude_applies_below_the_gitignore() {
        let dir = tree(&[
            (".git/info/exclude", "local/\nshared.txt\n"),
            (".gitignore", "!shared.txt\n"),
        ]);
        let rules = GitIgnoreRules::for_root(dir.path());
        assert_eq!(
            ignored(&rules, "local/notes.md").map(|h| h.source),
            Some(".git/info/exclude".to_string())
        );
        assert!(
            ignored(&rules, "shared.txt").is_none(),
            "the .gitignore overrides exclude"
        );
    }

    #[test]
    fn paths_outside_the_root_and_trees_without_rules_are_never_ignored() {
        let dir = tree(&[("src/lib.rs", "")]);
        let rules = GitIgnoreRules::for_root(dir.path());
        assert!(ignored(&rules, "src/lib.rs").is_none());
        assert!(
            rules
                .ignored_abs(Path::new("/etc/hostname"), false)
                .is_none()
        );
    }

    #[test]
    fn the_rules_of_a_subdirectory_come_from_its_working_tree() {
        let dir = tree(&[
            (".git/HEAD", "ref: refs/heads/main\n"),
            (".gitignore", "/site/\n"),
        ]);
        fs::create_dir_all(dir.path().join("crates/x")).expect("mkdir");
        let rules = GitIgnoreRules::for_path(&dir.path().join("crates/x"));
        assert_eq!(rules.root(), dir.path());
        assert!(
            rules
                .ignored_abs(&dir.path().join("site/a.py"), false)
                .is_some()
        );
    }
}
