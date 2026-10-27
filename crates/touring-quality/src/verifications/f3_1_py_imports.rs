//! Import-based test discovery for F3.1 (Python).
//!
//! The proximity pairing in `f3_1_coverage` finds `test_<m>.py` beside the module or
//! in a `tests/` directory on the way up. Real repositories also keep tests in a
//! sibling tree (`tests/hooks_contract/test_referencia_viva.py` for
//! `scripts/diagnostics/referencia_viva.py`), where that rule is blind and F3.1 read
//! `test fns=0` for modules with a full suite (23/10/2026, project `analise`).
//!
//! This module adds a second criterion: a test file (`test_*.py` / `*_test.py`,
//! anywhere under the repository root) counts for a module when it **imports** it.
//!
//! * The test files are parsed ONCE per repository root (cached for [`INDEX_TTL`]),
//!   not once per scored file.
//! * Strings and comments are blanked before parsing, so a module name that only
//!   appears in a docstring, a string or a comment is never an import.
//! * Matching is by the full module path, never by basename: `pkg/mod.py` and
//!   `other/mod.py` do not cover each other.
//!
//! Detected: `import a.b`, `from a.b import x`, `from a import b` (submodule),
//! relative `from . import b` / `from .b import x`, `src`-layout imports
//! (`import pkg.mod` for `src/pkg/mod.py`) when exactly one file answers to the
//! name, a bare `import mod` when the test touches `sys.path` and exactly one file
//! answers to the name, and an `importlib` / `runpy` load by a literal file path.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use touring_foundation::gitignore::GitIgnoreRules;

/// How long a repository's import index is reused. The score of a whole scope runs
/// F3.1 once per file inside one process, which is the case the cache serves; a
/// long-lived daemon re-reads after the TTL, so an edited test is never invisible
/// for long.
const INDEX_TTL: Duration = Duration::from_secs(10);

/// Directories that are never part of the project's own tests.
const SKIP_DIRS: &[&str] = &[
    ".git",
    ".venv",
    "venv",
    "node_modules",
    "target",
    "__pycache__",
    "site-packages",
    ".tox",
    ".mypy_cache",
    ".pytest_cache",
    ".ruff_cache",
];

/// A test file is not parsed past this size (generated fixtures, vendored blobs).
const MAX_TEST_FILE_BYTES: u64 = 2 * 1024 * 1024;

/// Substrings that make a file worth scanning for literal `.py` paths.
const PATH_LOADERS: &[&str] = &[
    "spec_from_file_location",
    "run_path",
    "load_source",
    "SourceFileLoader",
];

/// A test file that imports the module under score.
#[derive(Debug, Clone)]
pub(super) struct ImportingTest {
    pub path: PathBuf,
    pub test_fns: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum ImportKind {
    /// Written as an absolute import: resolves from the root, or from a `src`-style
    /// import root.
    Absolute,
    /// Already resolved to a root-relative dotted path (relative import).
    RootRelative,
}

#[derive(Debug)]
struct TestFile {
    path: PathBuf,
    test_fns: usize,
    imports: HashSet<(String, ImportKind)>,
    /// The test file touches `sys.path`, so a bare `import mod` can resolve.
    touches_sys_path: bool,
    /// Normalised literal paths ending in `.py` (only for files using a path loader).
    path_literals: Vec<String>,
}

#[derive(Debug)]
pub(super) struct PyTestIndex {
    root: PathBuf,
    tests: Vec<TestFile>,
    /// Module path (`a/b/c`, no `.py`, `/__init__` folded away) of every `.py` file.
    module_paths: Vec<String>,
}

type Cache = Mutex<HashMap<PathBuf, (Instant, Arc<PyTestIndex>)>>;

fn cache() -> &'static Cache {
    static CACHE: OnceLock<Cache> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// The test files under the repository root of `target` that import `target`.
/// Empty when `target` is itself a test or `conftest.py`, has no repository root, or
/// nothing imports it.
pub(super) fn importing_tests(target: &Path) -> Vec<ImportingTest> {
    let Ok(abs) = std::fs::canonicalize(target) else {
        return Vec::new();
    };
    let stem = abs.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    if is_test_stem(stem) || stem == "conftest" {
        return Vec::new();
    }
    let Some(root) = super::find_repo_root(&abs) else {
        return Vec::new();
    };
    let Ok(rel) = abs.strip_prefix(&root) else {
        return Vec::new();
    };
    let rel = rel_string(rel);
    let index = index_for(&root);
    index.importing(&rel)
}

fn index_for(root: &Path) -> Arc<PyTestIndex> {
    let now = Instant::now();
    if let Ok(map) = cache().lock()
        && let Some((built, idx)) = map.get(root)
        && now.duration_since(*built) < INDEX_TTL
    {
        return Arc::clone(idx);
    }
    let built = Arc::new(PyTestIndex::build(root));
    if let Ok(mut map) = cache().lock() {
        map.insert(root.to_path_buf(), (now, Arc::clone(&built)));
    }
    built
}

fn is_test_stem(stem: &str) -> bool {
    stem.starts_with("test_") || stem.ends_with("_test")
}

fn rel_string(rel: &Path) -> String {
    rel.components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

/// `a/b/c.py` → `a/b/c`; `a/b/__init__.py` → `a/b`.
fn module_path_of(rel: &str) -> Option<String> {
    let no_ext = rel.strip_suffix(".py")?;
    Some(
        no_ext
            .strip_suffix("/__init__")
            .or_else(|| (no_ext == "__init__").then_some(""))
            .unwrap_or(no_ext)
            .to_string(),
    )
}

impl PyTestIndex {
    fn build(root: &Path) -> Self {
        let git = GitIgnoreRules::for_path(root);
        let mut tests = Vec::new();
        let mut module_paths = Vec::new();
        let mut stack = vec![root.to_path_buf()];
        while let Some(dir) = stack.pop() {
            let Ok(rd) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in rd.flatten() {
                let Ok(ft) = entry.file_type() else { continue };
                let p = entry.path();
                let name = entry.file_name();
                let name = name.to_string_lossy();
                if ft.is_dir() {
                    if !SKIP_DIRS.contains(&name.as_ref()) && git.ignored_abs(&p, true).is_none() {
                        stack.push(p);
                    }
                } else if ft.is_file()
                    && name.ends_with(".py")
                    && git.ignored_abs(&p, false).is_none()
                {
                    let Ok(rel) = p.strip_prefix(root) else {
                        continue;
                    };
                    let rel = rel_string(rel);
                    if let Some(m) = module_path_of(&rel) {
                        module_paths.push(m);
                    }
                    let stem = name.trim_end_matches(".py");
                    if is_test_stem(stem)
                        && let Some(tf) = parse_test_file(&p, &rel)
                    {
                        tests.push(tf);
                    }
                }
            }
        }
        tests.sort_by(|a, b| a.path.cmp(&b.path));
        Self {
            root: root.to_path_buf(),
            tests,
            module_paths,
        }
    }

    /// Files whose module path is `p` or ends with `/p` (a `src` layout).
    fn files_answering_to(&self, p: &str) -> usize {
        let tail = format!("/{p}");
        self.module_paths
            .iter()
            .filter(|m| m.as_str() == p || m.ends_with(&tail))
            .count()
    }

    fn importing(&self, target_rel: &str) -> Vec<ImportingTest> {
        let Some(tp) = module_path_of(target_rel) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for tf in &self.tests {
            if tf.path == self.root.join(target_rel) {
                continue;
            }
            if self.imports_module(tf, target_rel, &tp) {
                out.push(ImportingTest {
                    path: tf.path.clone(),
                    test_fns: tf.test_fns,
                });
            }
        }
        out
    }

    fn imports_module(&self, tf: &TestFile, target_rel: &str, tp: &str) -> bool {
        for (dotted, kind) in &tf.imports {
            let p = dotted.replace('.', "/");
            if p == tp {
                return true;
            }
            if *kind == ImportKind::RootRelative {
                continue;
            }
            // `src`-style import root, or a bare name made importable by sys.path:
            // only when the root itself does not already answer to the name and the
            // target is the single file that does.
            let suffix = format!("/{p}");
            if !tp.ends_with(&suffix) {
                continue;
            }
            if !dotted.contains('.') && !tf.touches_sys_path {
                continue;
            }
            if self.module_paths.contains(&p) {
                continue;
            }
            if self.files_answering_to(&p) == 1 {
                return true;
            }
        }
        tf.path_literals
            .iter()
            .any(|lit| self.literal_names(lit, target_rel))
    }

    /// A literal `.../x.py` handed to an importlib/runpy loader names the target.
    fn literal_names(&self, lit: &str, target_rel: &str) -> bool {
        if target_rel == lit || target_rel.ends_with(&format!("/{lit}")) {
            // a partial path must not also name a homonym elsewhere
            let tail = format!("/{lit}");
            let n = self
                .module_paths
                .iter()
                .filter(|m| {
                    let with_ext = format!("{m}.py");
                    with_ext == lit || with_ext.ends_with(&tail)
                })
                .count();
            return n <= 1 || target_rel == lit;
        }
        // an absolute literal ends with the target's root-relative path
        lit.ends_with(&format!("/{target_rel}"))
    }
}

fn parse_test_file(path: &Path, rel: &str) -> Option<TestFile> {
    let meta = std::fs::metadata(path).ok()?;
    if meta.len() > MAX_TEST_FILE_BYTES {
        return None;
    }
    let raw = std::fs::read_to_string(path).ok()?;
    let test_fns = super::f3_1_coverage::count_python_tests(&raw);
    let (code, literals) = strip_strings_and_comments(&raw);

    // directory of the test file, root-relative, as path components
    let dir_parts: Vec<&str> = rel.split('/').collect();
    let dir_parts = &dir_parts[..dir_parts.len().saturating_sub(1)];

    let imports = parse_imports(&code, dir_parts);
    let touches_sys_path = code.contains("sys.path");
    let path_literals = if PATH_LOADERS.iter().any(|k| code.contains(k)) {
        literals
            .into_iter()
            .filter_map(|l| {
                let n = l.replace('\\', "/");
                let n = n.trim_start_matches("./").to_string();
                (n.ends_with(".py") && n.contains('/')).then_some(n)
            })
            .collect()
    } else {
        Vec::new()
    };
    Some(TestFile {
        path: path.to_path_buf(),
        test_fns,
        imports,
        touches_sys_path,
        path_literals,
    })
}

/// Byte scanner that blanks comments and string literals.
struct Scan<'a> {
    b: &'a [u8],
    i: usize,
    out: Vec<u8>,
    literals: Vec<String>,
}

impl<'a> Scan<'a> {
    /// Consume one byte, emitting a blank (a newline stays a newline so line
    /// structure survives).
    fn eat_blank(&mut self) {
        self.out
            .push(if self.b[self.i] == b'\n' { b'\n' } else { b' ' });
        self.i += 1;
    }

    fn comment(&mut self) {
        while self.i < self.b.len() && self.b[self.i] != b'\n' {
            self.eat_blank();
        }
    }

    fn closes_triple(&self, q: u8) -> bool {
        self.i + 2 < self.b.len() && self.b[self.i..self.i + 3] == [q, q, q]
    }

    fn triple(&mut self, q: u8) {
        for _ in 0..3 {
            self.eat_blank();
        }
        while self.i < self.b.len() {
            if self.b[self.i] == b'\\' {
                self.eat_blank();
                if self.i < self.b.len() {
                    self.eat_blank();
                }
            } else if self.closes_triple(q) {
                for _ in 0..3 {
                    self.eat_blank();
                }
                return;
            } else {
                self.eat_blank();
            }
        }
    }

    fn single(&mut self, q: u8) {
        self.eat_blank();
        let start = self.i;
        while self.i < self.b.len() && self.b[self.i] != b'\n' {
            if self.b[self.i] == b'\\' {
                self.eat_blank();
                if self.i < self.b.len() {
                    self.eat_blank();
                }
            } else if self.b[self.i] == q {
                if let Ok(s) = std::str::from_utf8(&self.b[start..self.i])
                    && s.len() < 512
                {
                    self.literals.push(s.to_string());
                }
                self.eat_blank();
                return;
            } else {
                self.eat_blank();
            }
        }
    }

    fn run(mut self) -> (String, Vec<String>) {
        while self.i < self.b.len() {
            match self.b[self.i] {
                b'#' => self.comment(),
                q @ (b'"' | b'\'') => {
                    if self.closes_triple(q) {
                        self.triple(q);
                    } else {
                        self.single(q);
                    }
                }
                c => {
                    self.out.push(c);
                    self.i += 1;
                }
            }
        }
        (
            String::from_utf8(self.out).unwrap_or_default(),
            self.literals,
        )
    }
}

/// Blank every comment and string literal (keeping newlines) and collect the
/// single-line string literals. What is left is code, so a line that starts with
/// `import`/`from` there is a real import.
fn strip_strings_and_comments(src: &str) -> (String, Vec<String>) {
    Scan {
        b: src.as_bytes(),
        i: 0,
        out: Vec::with_capacity(src.len()),
        literals: Vec::new(),
    }
    .run()
}

/// Logical statements of already-stripped code: physical lines joined across open
/// brackets and backslash continuations, split on `;`.
fn statements(code: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut buf = String::new();
    let mut depth: i32 = 0;
    for line in code.lines() {
        let l = line.trim_end();
        let continued = l.ends_with('\\');
        let l = l.strip_suffix('\\').unwrap_or(l);
        buf.push_str(l);
        buf.push(' ');
        for ch in l.chars() {
            match ch {
                '(' | '[' | '{' => depth += 1,
                ')' | ']' | '}' => depth -= 1,
                _ => {}
            }
        }
        if depth <= 0 && !continued {
            depth = 0;
            for part in buf.split(';') {
                let t = part.trim();
                if !t.is_empty() {
                    out.push(t.to_string());
                }
            }
            buf.clear();
        }
    }
    if !buf.trim().is_empty() {
        out.push(buf.trim().to_string());
    }
    out
}

/// Every module an import statement may bring in, as dotted names.
fn parse_imports(code: &str, test_dir: &[&str]) -> HashSet<(String, ImportKind)> {
    let mut set = HashSet::new();
    for st in statements(code) {
        if let Some(rest) = st.strip_prefix("import ") {
            plain_import(rest, &mut set);
        } else if let Some(rest) = st.strip_prefix("from ") {
            from_import(rest, test_dir, &mut set);
        }
    }
    set
}

/// `import a.b as x, c.d` → `a.b`, `c.d`.
fn plain_import(rest: &str, set: &mut HashSet<(String, ImportKind)>) {
    for part in rest.split(',') {
        let name = part.split_whitespace().next().unwrap_or("");
        if is_dotted_name(name) {
            set.insert((name.to_string(), ImportKind::Absolute));
        }
    }
}

/// The names after `import` in a `from … import (a, b as c)` statement.
fn imported_names(names: &str) -> Vec<String> {
    names
        .replace(['(', ')'], " ")
        .split(',')
        .filter_map(|n| n.split_whitespace().next())
        .filter(|n| *n != "*" && is_dotted_name(n))
        .map(str::to_string)
        .collect()
}

/// `from a.b import x` → `a.b` and `a.b.x` (x may be a submodule); a relative form
/// is resolved against the test file's own directory.
fn from_import(rest: &str, test_dir: &[&str], set: &mut HashSet<(String, ImportKind)>) {
    let Some((module, names)) = rest.split_once(" import ") else {
        return;
    };
    let module = module.trim();
    let level = module.chars().take_while(|c| *c == '.').count();
    let base = module.trim_start_matches('.');
    let names = imported_names(names);
    if level == 0 {
        if !is_dotted_name(base) {
            return;
        }
        set.insert((base.to_string(), ImportKind::Absolute));
        for n in &names {
            set.insert((format!("{base}.{n}"), ImportKind::Absolute));
        }
        return;
    }
    if level - 1 > test_dir.len() {
        return;
    }
    let mut prefix: Vec<&str> = test_dir[..test_dir.len() - (level - 1)].to_vec();
    if !base.is_empty() {
        prefix.extend(base.split('.'));
    }
    let pre = prefix.join(".");
    if !pre.is_empty() {
        set.insert((pre.clone(), ImportKind::RootRelative));
    }
    for n in &names {
        let full = if pre.is_empty() {
            n.clone()
        } else {
            format!("{pre}.{n}")
        };
        set.insert((full, ImportKind::RootRelative));
    }
}

fn is_dotted_name(s: &str) -> bool {
    !s.is_empty()
        && s.split('.').all(|seg| {
            let mut ch = seg.chars();
            ch.next().is_some_and(|c| c.is_alphabetic() || c == '_')
                && ch.all(|c| c.is_alphanumeric() || c == '_')
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_strings_and_comments_but_keeps_code() {
        let src = "import real  # import fake_c\nx = \"import fake_s\"\n'''\nfrom fake_d import y\n'''\nfrom ok import z\n";
        let (code, lits) = strip_strings_and_comments(src);
        let imps = parse_imports(&code, &[]);
        let names: HashSet<&str> = imps.iter().map(|(n, _)| n.as_str()).collect();
        assert!(names.contains("real"));
        assert!(names.contains("ok"));
        assert!(names.contains("ok.z"));
        assert!(!names.iter().any(|n| n.starts_with("fake")), "{names:?}");
        assert!(lits.iter().any(|l| l == "import fake_s"));
    }

    #[test]
    fn parses_multiline_aliases_and_semicolons() {
        let code = "from a.b import (\n    x,\n    y as w,\n)\nimport c.d as e, f; import g\n";
        let imps = parse_imports(code, &[]);
        let names: HashSet<&str> = imps.iter().map(|(n, _)| n.as_str()).collect();
        for want in ["a.b", "a.b.x", "a.b.y", "c.d", "f", "g"] {
            assert!(names.contains(want), "missing {want} in {names:?}");
        }
    }

    #[test]
    fn relative_import_resolves_against_the_test_directory() {
        let imps = parse_imports("from ..core import m\nfrom . import n\n", &["pkg", "tests"]);
        let names: HashSet<&str> = imps.iter().map(|(n, _)| n.as_str()).collect();
        assert!(names.contains("pkg.core.m"), "{names:?}");
        assert!(names.contains("pkg.tests.n"), "{names:?}");
    }

    #[test]
    fn module_path_folds_init() {
        assert_eq!(module_path_of("a/b/c.py").as_deref(), Some("a/b/c"));
        assert_eq!(module_path_of("a/b/__init__.py").as_deref(), Some("a/b"));
        assert_eq!(module_path_of("a/b/c.rs"), None);
    }
}
