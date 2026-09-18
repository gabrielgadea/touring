//! The index admission policy — ONE predicate shared by every writer and every
//! diagnostic of the symbol index.
//!
//! Until 2026-09-13 the walker's rule tables lived inside the rebuild handler and
//! the incremental writers (`post_write` / `post_edit` → `reindex_file_with_old`)
//! applied no rule at all: an edit under `.claude/`, a script in the session
//! scratchpad, a rule file under `~/.claude/` — each landed in the project's
//! `symbols.db` (140 files under absolute paths, measured in the live workspace
//! that day), and the rebuild's stale sweep only retired paths that had vanished
//! from disk, so rows for files the walker refuses (956 files under `.venv/`, 306
//! under `.claude/`) survived five months of rebuilds and outranked live code in
//! BM25. Hoisting the predicate here closes both doors: the walker, the hook
//! writers, the sweep and `touring index why` all ask the same function, so an
//! exclusion can never be silent and a refused path can never be written.
//!
//! I13 / I16 of the Graft analysis (`docs/plans/2026-09-12-graft-analysis/`).

use std::path::Path;

use touring_foundation::config::{CompanionRoot, TouringConfig};
use touring_foundation::gitignore::GitIgnoreRules;

/// A source file this large is data wearing a source extension.
///
/// Measured 19/08/2026 in `analise`: two `.md` files of **380 MB each**, a 69 MB
/// `.json` of geodata and six `claims*.json` of ~40 MB, all carrying supported
/// extensions, took the daemon to **48 GB RSS** — and the memory guard, which
/// samples every hundred files, never fired because the jump fit between two
/// probes. The ceiling is calibrated, not guessed: across four projects every code
/// file above 1 MB lives in a venv or `node_modules` (already skipped), and the
/// largest anywhere is a 2.89 MB minified bundle. 8 MB is ~3× that.
pub const MAX_INDEXABLE_FILE_BYTES: u64 = 8 * 1024 * 1024;

/// Extensions the walker indexes. Shared with `touring index why` and the hook
/// writers so "why is this file not indexed" is answered by the rule that excluded
/// it — never by a second copy that can drift.
pub const INDEX_SUPPORTED_EXTS: &[&str] = &[
    "rs", "py", "pyi", "ts", "tsx", "js", "jsx", "mjs", "cjs", "sh", "bash", "html", "css", "scss",
    "md", "mdx", "json", "toml", "yaml", "yml",
];

/// Directory names the walker never enters (exact match; [`dir_skip_reason`] layers
/// the venv-prefix, hidden-directory and `.claude/` state rules on top).
const INDEX_SKIP_DIRS: &[&str] = &[
    // Build / dependency artifacts
    "target",
    ".git",
    "node_modules",
    ".cargo",
    ".venv",
    "venv",
    "__pycache__",
    "dist",
    "build",
    ".tox",
    ".mypy_cache",
    ".pytest_cache",
    ".ruff_cache",
    ".eggs",
    ".nox",
    ".cache",
    // Data / output directories (avoid walking huge data lakes)
    "data",
    "datasets",
    "dataset",
    "raw_data",
    "processed_data",
    "downloaded_files",
    "downloads",
    "uploads",
    "attachments",
    "coverage",
    "coverage_html",
    "htmlcov",
    "lcov-report",
    "migrations",
    "generated",
    "benchmarks",
    "tmp",
    "temp",
    "logs",
    "log",
    // Touring daemon state — JSON checkpoint files are NOT code
    // (knowledge_sync_*.json would be parsed as modules with fake symbols)
    "checkpoints",
];

/// Non-touring subprojects skipped entirely (same filter as post_read + cli_e2e).
const INDEX_SKIP_SUBPROJECTS: &[&str] = &[
    "agent-harness",
    "holon-wasm-components",
    "holon-wasm-runner",
    "pln2",
];

/// The walker's directory rule: exact skip-list match, any `venv`/`.venv` prefix,
/// or a hidden directory.
/// Directories directly under `.claude/` that hold Touring RUNTIME STATE, never
/// documents: the symbol/knowledge databases and tantivy segments, session
/// summaries, the cipher queue. (`data/`, `checkpoints/` and `logs/` are already
/// refused everywhere by `INDEX_SKIP_DIRS`.) Measured 13/09/2026 across every
/// project on the machine: `analise/.claude` holds 1.320 plan files, 522 skill
/// files, 74 docs and 27 agents next to 15.081 checkpoints and 6.607 data files;
/// `konverter/.claude` holds Rust code under `rust-core/`, rules, skills and
/// commands next to 2.649 checkpoints. Documents in, state out — by name.
const CLAUDE_STATE_DIRS: &[&str] = &["touring", "session_summaries", "cipher_queue"];

/// The walker's directory rule over a directory's relative components (the last
/// one is the directory itself): `Some(verdict)` when it is never entered, `None`
/// when it is walked. Exact skip-list match, skipped subprojects, any
/// `venv`/`.venv` prefix, and hidden directories — with ONE exception: `.claude/`
/// is entered, because memories, rules, skills, plans and even code live there
/// (Gabriel, 13/09/2026: "memórias e regras devem sim ser buscáveis"), and only
/// its state subdirectories (`CLAUDE_STATE_DIRS`) are refused, by name.
#[must_use]
pub fn dir_skip_reason(components: &[&str]) -> Option<(IndexVerdict, String)> {
    let (&name, parents) = components.split_last()?;
    if INDEX_SKIP_SUBPROJECTS.contains(&name) {
        return Some((
            IndexVerdict::SkippedSubproject,
            format!("`{name}/` is a skipped subproject; the walker never enters it"),
        ));
    }
    if INDEX_SKIP_DIRS.contains(&name) {
        return Some((
            IndexVerdict::SkippedDir,
            format!("`{name}/` is a directory the walker never enters"),
        ));
    }
    // Prefix match for venv variants (.venv-test, .venv.bak, venv_cipher, etc.)
    if name.starts_with(".venv") || name.starts_with("venv") {
        return Some((
            IndexVerdict::SkippedDir,
            format!("`{name}/` is a virtualenv; the walker never enters it"),
        ));
    }
    if name == ".claude" {
        return None;
    }
    if parents.last() == Some(&".claude") && CLAUDE_STATE_DIRS.contains(&name) {
        return Some((
            IndexVerdict::SkippedDir,
            format!("`.claude/{name}/` holds Touring runtime state, not documents"),
        ));
    }
    // Hidden directories starting with `.` — `.claude/` handled above.
    if name.starts_with('.') {
        return Some((
            IndexVerdict::SkippedDir,
            format!(
                "`{name}/` is hidden; the walker never enters it (`.claude/` is the one exception)"
            ),
        ));
    }
    None
}

/// The project's own exclusion over a directory's components relative to the
/// project root: `Some(SkippedDir)` when the directory IS one of the prefixes the
/// project declared in `[index] exclude_dirs` (`TouringConfig::index_excluded_dirs_for`).
/// Walker, sweep and `why` share this one check, so a declared exclusion is
/// reported like every built-in one — never silent.
#[must_use]
pub fn excluded_dir_reason(
    excluded: &[String],
    components: &[&str],
) -> Option<(IndexVerdict, String)> {
    if excluded.is_empty() || components.is_empty() {
        return None;
    }
    let joined = components.join("/");
    excluded.iter().find(|e| **e == joined).map(|e| {
        (
            IndexVerdict::SkippedDir,
            format!("`{e}/` is excluded by `[index] exclude_dirs` in .touring/touring.toml"),
        )
    })
}

/// Why one path is, or is not, in the index — the walker's rules applied to a
/// single path, in the order the walker applies them. Serialized `snake_case` in
/// the `cli-index-why` response and in the rebuild's purge census.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IndexVerdict {
    /// Present in the symbol store.
    Indexed,
    /// Passes every walker rule but has no stored symbols — a rebuild or
    /// `index ingest` is due.
    EligibleNotIndexed,
    /// Not under the project root: the walker never leaves it.
    OutsideRoot,
    /// A path component is a directory the walker never enters.
    SkippedDir,
    /// A path component is a skipped subproject.
    SkippedSubproject,
    /// Nothing at that path.
    Missing,
    /// A directory, not a file.
    Directory,
    /// Extension outside the supported set.
    UnsupportedExtension,
    /// Larger than the size ceiling.
    Oversized,
    /// Eligible but not readable as UTF-8 text.
    Unreadable,
    /// A `@companion/<name>/…` key whose companion root is not configured for
    /// this project (the sweep retires such rows; `why` names the root).
    CompanionNotConfigured,
    /// Ignored by the project's git rules (`.gitignore`, `.git/info/exclude`):
    /// generated trees such as the MkDocs `site/` build carried stale copies of
    /// the sources into the index (cross-audit 14/09/2026, R2-2).
    Gitignored,
}

impl IndexVerdict {
    /// The `snake_case` name the JSON carries, for callers that key a census by it.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Indexed => "indexed",
            Self::EligibleNotIndexed => "eligible_not_indexed",
            Self::OutsideRoot => "outside_root",
            Self::SkippedDir => "skipped_dir",
            Self::SkippedSubproject => "skipped_subproject",
            Self::Missing => "missing",
            Self::Directory => "directory",
            Self::UnsupportedExtension => "unsupported_extension",
            Self::Oversized => "oversized",
            Self::Unreadable => "unreadable",
            Self::Gitignored => "gitignored",
            Self::CompanionNotConfigured => "companion_not_configured",
        }
    }
}

/// What the filesystem says about a candidate. Gathered by the caller, judged by
/// [`classify_index_candidate`], so the judgement is testable without a disk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CandidateFacts {
    /// `fs::metadata` succeeded.
    pub exists: bool,
    /// The path is a directory (after following links, as the walker does).
    pub is_dir: bool,
    /// Size in bytes (0 when missing).
    pub bytes: u64,
    /// `read_to_string` succeeds — only probed once every cheaper rule passed.
    pub readable: bool,
    /// The path is under the project root.
    pub inside_root: bool,
}

/// The size gate as a pure predicate — a rule that cannot be exercised in a test
/// is a rule nobody can trust to still be there.
#[must_use]
pub fn exceeds_index_size_ceiling(bytes: u64) -> bool {
    bytes > MAX_INDEXABLE_FILE_BYTES
}

/// Pure judgement over a repo-relative path and its facts, in the walker's order:
/// skipped components, existence, containment, kind, extension, size, readability.
/// `None` means "eligible" — only the symbol store can then say indexed or not.
#[must_use]
pub fn classify_index_candidate(
    rel: &str,
    facts: CandidateFacts,
) -> Option<(IndexVerdict, String)> {
    let parts: Vec<&str> = rel
        .split('/')
        .filter(|p| !p.is_empty() && *p != ".")
        .collect();
    let Some((file, dirs)) = parts.split_last() else {
        return Some((IndexVerdict::Missing, "empty path".to_string()));
    };
    // Every ancestor is judged WITH its own ancestors — `.claude/touring/` is
    // refused for the state rule while `.claude/skills/` is walked.
    for depth in 1..=dirs.len() {
        if let Some(refused) = dir_skip_reason(&dirs[..depth]) {
            return Some(refused);
        }
    }
    if !facts.exists {
        return Some((IndexVerdict::Missing, "no such file".to_string()));
    }
    if !facts.inside_root {
        return Some((
            IndexVerdict::OutsideRoot,
            "not under the project root; the walker never leaves it".to_string(),
        ));
    }
    if facts.is_dir {
        return Some((
            IndexVerdict::Directory,
            "a directory; the index holds files".to_string(),
        ));
    }
    let ext = Path::new(file)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    if !INDEX_SUPPORTED_EXTS.contains(&ext) {
        return Some((
            IndexVerdict::UnsupportedExtension,
            format!("`.{ext}` is not in the supported extension set"),
        ));
    }
    if exceeds_index_size_ceiling(facts.bytes) {
        return Some((
            IndexVerdict::Oversized,
            format!(
                "{} bytes exceed the {} byte ceiling (MAX_INDEXABLE_FILE_BYTES)",
                facts.bytes, MAX_INDEXABLE_FILE_BYTES
            ),
        ));
    }
    if !facts.readable {
        return Some((
            IndexVerdict::Unreadable,
            "not readable as UTF-8 text".to_string(),
        ));
    }
    None
}

/// The facts `fs::metadata` alone can supply — no read, so a giant file is never
/// pulled into memory by a diagnostic or a sweep. `readable` is reported `true`
/// here; a caller that must know reads only after every cheaper rule passed.
#[must_use]
pub fn facts_from_metadata(root: &Path, abs: &Path) -> CandidateFacts {
    let meta = std::fs::metadata(abs).ok();
    let (exists, is_dir, bytes) = meta
        .as_ref()
        .map(|m| (true, m.is_dir(), m.len()))
        .unwrap_or((false, false, 0));
    CandidateFacts {
        exists,
        is_dir,
        bytes,
        readable: true,
        inside_root: abs.starts_with(root),
    }
}

/// The path a stored key denotes: absolute keys stand on their own, relative keys
/// hang off the root. (`Path::join` already does this — spelled out because the
/// sweep relies on it: an absolute key never resolves under the root by accident.)
/// Companion keys (`@companion/…`) need the project's policy: see
/// [`IndexPolicy::path_for_key`].
#[must_use]
pub fn absolute_for_key(root: &Path, key: &str) -> std::path::PathBuf {
    let p = Path::new(key);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        root.join(p)
    }
}

/// Every key of a file that lives in a companion root starts with this. Defined
/// once in `touring-foundation`, where the wiring gate of `touring-storage` reads
/// it too.
pub use touring_foundation::config::COMPANION_KEY_PREFIX;

/// The admission policy of ONE project: its root plus its companion roots —
/// the directories outside the project whose files the index carries under
/// `@companion/<name>/<relative path>` (`TouringConfig::companion_roots_for`:
/// the global rules, skills, agents and commands, the project's auto-memory
/// and the memory of `~`). Built once per rebuild, per hook write and per
/// `why`, so the configuration is read once per operation, never per row.
#[derive(Debug, Clone)]
pub struct IndexPolicy {
    root: std::path::PathBuf,
    companions: Vec<CompanionRoot>,
    excluded_dirs: Vec<String>,
    gitignore: GitIgnoreRules,
}

impl IndexPolicy {
    /// The policy of `root`, companions read from its `.touring/touring.toml`
    /// (defaults when absent).
    #[must_use]
    pub fn for_root(root: &Path) -> Self {
        Self::with_companions(root, TouringConfig::companion_roots_for(root))
            .with_excluded_dirs(TouringConfig::index_excluded_dirs_for(root))
    }

    /// The same policy with the project's declared exclusions (root-relative
    /// directory prefixes, as `TouringConfig::index_excluded_dirs_for` returns).
    #[must_use]
    fn with_excluded_dirs(mut self, excluded_dirs: Vec<String>) -> Self {
        self.excluded_dirs = excluded_dirs;
        self
    }

    /// The directories of the project root the walker must not enter.
    #[must_use]
    pub fn excluded_dirs(&self) -> &[String] {
        &self.excluded_dirs
    }

    /// The first declared exclusion covering a project-relative path.
    fn excluded_reason(&self, rel: &str) -> Option<(IndexVerdict, String)> {
        let parts: Vec<&str> = rel
            .split('/')
            .filter(|p| !p.is_empty() && *p != ".")
            .collect();
        let dirs = parts.split_last().map(|(_, d)| d).unwrap_or(&[]);
        (1..=dirs.len()).find_map(|depth| excluded_dir_reason(&self.excluded_dirs, &dirs[..depth]))
    }

    /// The policy of `root` with an explicit companion list (tests, callers
    /// that already resolved the configuration). Longest companion path first,
    /// so a nested companion wins the match over its ancestor.
    #[must_use]
    fn with_companions(root: &Path, mut companions: Vec<CompanionRoot>) -> Self {
        companions.sort_by(|a, b| {
            b.path
                .as_os_str()
                .len()
                .cmp(&a.path.as_os_str().len())
                .then_with(|| a.name.cmp(&b.name))
        });
        Self {
            root: root.to_path_buf(),
            companions,
            excluded_dirs: Vec::new(),
            gitignore: GitIgnoreRules::for_root(root),
        }
    }

    /// The project's git ignore rules over an absolute path under the root:
    /// `Some(Gitignored)` naming the file and the rule, `None` when git keeps it
    /// (or the path lies outside the root, where the rules say nothing). The
    /// walkers ask it before entering a directory or taking a file, and
    /// [`Self::verdict_for_key`] reports it, so the sweep retires what git
    /// ignores and `why` names the rule. Companion roots are not the project's
    /// working tree and are never judged by it.
    #[must_use]
    pub fn gitignored(&self, abs: &Path, is_dir: bool) -> Option<(IndexVerdict, String)> {
        self.gitignore
            .ignored_abs(abs, is_dir)
            .map(|hit| (IndexVerdict::Gitignored, hit.to_string()))
    }

    /// The project root.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The companion roots, longest path first.
    #[must_use]
    pub fn companions(&self) -> &[CompanionRoot] {
        &self.companions
    }

    /// `(name, relative path)` of a companion key.
    fn companion_parts(key: &str) -> Option<(&str, &str)> {
        key.strip_prefix(COMPANION_KEY_PREFIX)?.split_once('/')
    }

    /// The storage key of an absolute path: project-relative under the root,
    /// `@companion/<name>/<rel>` under a companion, `None` outside every root.
    /// A key never carries `./` and is the one spelling the store accepts.
    #[must_use]
    pub fn key_for(&self, abs: &Path) -> Option<String> {
        if let Ok(rel) = abs.strip_prefix(&self.root) {
            return Some(rel.to_string_lossy().trim_start_matches("./").to_string());
        }
        self.companions.iter().find_map(|c| {
            abs.strip_prefix(&c.path)
                .ok()
                .map(|rel| format!("{COMPANION_KEY_PREFIX}{}/{}", c.name, rel.to_string_lossy()))
        })
    }

    /// The path a key denotes: a companion key under its companion root (an
    /// unconfigured name resolves under the project root, where it cannot exist),
    /// an absolute key as itself, a relative key under the project root.
    #[must_use]
    pub fn path_for_key(&self, key: &str) -> std::path::PathBuf {
        if let Some((name, rel)) = Self::companion_parts(key) {
            return match self.companions.iter().find(|c| c.name == name) {
                Some(c) => c.path.join(rel),
                None => self.root.join(key),
            };
        }
        absolute_for_key(&self.root, key)
    }

    /// The verdict for a stored key or a freshly written path, from metadata
    /// alone: `Some(exclusion)` when the walker would refuse it, `None` when it
    /// is eligible (readability is the one rule left to the caller). The hook
    /// writers ask this before touching the store, the rebuild sweep before
    /// keeping a row, and `touring index why` prints the same judgement.
    ///
    /// An absolute path is judged by its components UNDER the root (or the
    /// companion) it lives in — never by that root's own ancestors (a project
    /// under `/home/x/.work/` would otherwise refuse every file for the hidden
    /// `.work`). Outside every root the verdict is containment itself.
    #[must_use]
    pub fn verdict_for_key(&self, key: &str) -> Option<(IndexVerdict, String)> {
        if let Some((name, rel)) = Self::companion_parts(key) {
            return match self.companions.iter().find(|c| c.name == name) {
                Some(c) => {
                    classify_index_candidate(rel, facts_from_metadata(&c.path, &c.path.join(rel)))
                }
                None => Some((
                    IndexVerdict::CompanionNotConfigured,
                    format!("companion root `{name}` is not configured for this project"),
                )),
            };
        }
        let abs = absolute_for_key(&self.root, key);
        if Path::new(key).is_absolute() {
            if let Ok(rel) = abs.strip_prefix(&self.root) {
                let rel = rel.to_string_lossy();
                return self
                    .excluded_reason(&rel)
                    .or_else(|| {
                        classify_index_candidate(&rel, facts_from_metadata(&self.root, &abs))
                    })
                    .or_else(|| self.gitignored(&abs, abs.is_dir()));
            }
            if let Some(c) = self.companions.iter().find(|c| abs.starts_with(&c.path)) {
                let rel = abs
                    .strip_prefix(&c.path)
                    .map(|r| r.to_string_lossy().into_owned())
                    .unwrap_or_default();
                return classify_index_candidate(&rel, facts_from_metadata(&c.path, &abs));
            }
            return Some((
                IndexVerdict::OutsideRoot,
                "not under the project root nor any companion root; the walker never leaves them"
                    .to_string(),
            ));
        }
        self.excluded_reason(key)
            .or_else(|| classify_index_candidate(key, facts_from_metadata(&self.root, &abs)))
            .or_else(|| self.gitignored(&abs, abs.is_dir()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts(exists: bool, bytes: u64) -> CandidateFacts {
        CandidateFacts {
            exists,
            is_dir: false,
            bytes,
            readable: true,
            inside_root: true,
        }
    }

    #[test]
    fn the_walker_rules_apply_in_the_walker_order() {
        let ok = facts(true, 10);
        assert_eq!(
            classify_index_candidate("src/lib.rs", ok),
            None,
            "an ordinary source file is eligible"
        );
        let cases: &[(&str, CandidateFacts, IndexVerdict)] = &[
            ("target/debug/x.rs", ok, IndexVerdict::SkippedDir),
            (".github/workflows/ci.yml", ok, IndexVerdict::SkippedDir),
            ("crates/x/.venv/lib/a.py", ok, IndexVerdict::SkippedDir),
            ("pln2/src/a.rs", ok, IndexVerdict::SkippedSubproject),
            ("src/missing.rs", facts(false, 0), IndexVerdict::Missing),
            ("Cargo.lock", ok, IndexVerdict::UnsupportedExtension),
            (
                "big.json",
                facts(true, MAX_INDEXABLE_FILE_BYTES + 1),
                IndexVerdict::Oversized,
            ),
            (
                "bin.rs",
                CandidateFacts {
                    readable: false,
                    ..ok
                },
                IndexVerdict::Unreadable,
            ),
            (
                "src",
                CandidateFacts { is_dir: true, ..ok },
                IndexVerdict::Directory,
            ),
            (
                "/etc/hostname",
                CandidateFacts {
                    inside_root: false,
                    ..ok
                },
                IndexVerdict::OutsideRoot,
            ),
        ];
        for (rel, f, want) in cases {
            let got = classify_index_candidate(rel, *f).map(|(v, _)| v);
            assert_eq!(got, Some(*want), "{rel}");
        }
    }

    /// `.claude/` is the one hidden directory the walker enters; inside it, only
    /// the state subdirectories are refused, and each with its own reason.
    #[test]
    fn claude_is_walked_and_only_its_state_dirs_are_refused() {
        let skip = |c: &[&str]| dir_skip_reason(c);
        assert_eq!(skip(&[".claude"]), None);
        assert_eq!(skip(&["crates", "x", ".claude"]), None);
        assert_eq!(skip(&[".claude", "skills"]), None);
        assert_eq!(skip(&[".claude", "plans"]), None);
        assert_eq!(skip(&[".claude", "rust-core"]), None);
        for state in ["touring", "session_summaries", "cipher_queue"] {
            let (v, d) = skip(&[".claude", state]).expect("state dir refused");
            assert_eq!(v, IndexVerdict::SkippedDir);
            assert!(d.contains("runtime state"), "{d}");
        }
        // Global rules still apply under `.claude/` — data/, checkpoints/, hidden.
        assert_eq!(
            skip(&[".claude", "data"]).map(|(v, _)| v),
            Some(IndexVerdict::SkippedDir)
        );
        assert_eq!(
            skip(&[".claude", "checkpoints"]).map(|(v, _)| v),
            Some(IndexVerdict::SkippedDir)
        );
        assert_eq!(
            skip(&[".claude", ".archive"]).map(|(v, _)| v),
            Some(IndexVerdict::SkippedDir)
        );
        // A state dir NOT directly under `.claude/` keeps its ordinary meaning.
        assert_eq!(skip(&["src", "touring"]), None);
        // Other hidden dirs, venvs and subprojects are unchanged.
        assert_eq!(
            skip(&[".github"]).map(|(v, _)| v),
            Some(IndexVerdict::SkippedDir)
        );
        assert_eq!(
            skip(&["x", ".venv-test"]).map(|(v, _)| v),
            Some(IndexVerdict::SkippedDir)
        );
        assert_eq!(
            skip(&["pln2"]).map(|(v, _)| v),
            Some(IndexVerdict::SkippedSubproject)
        );
        assert_eq!(skip(&[]), None, "the root itself is walked");

        let ok = facts(true, 10);
        assert_eq!(classify_index_candidate(".claude/CLAUDE.md", ok), None);
        assert_eq!(
            classify_index_candidate("crates/x/.claude/CLAUDE.md", ok),
            None
        );
        assert_eq!(
            classify_index_candidate(".claude/skills/s/SKILL.md", ok),
            None
        );
        let (v, d) = classify_index_candidate(".claude/touring/knowledge.db", ok).expect("refused");
        assert_eq!(
            (v, d.contains("runtime state")),
            (IndexVerdict::SkippedDir, true),
            "{d}"
        );
        assert_eq!(
            classify_index_candidate(".claude/checkpoints/c.json", ok).map(|(v, _)| v),
            Some(IndexVerdict::SkippedDir)
        );
    }

    #[test]
    fn a_skipped_component_wins_over_every_later_rule() {
        // A missing file under `.venv/` is refused for the directory, not for the
        // absence — the walker never looked, so absence is not the reason.
        let got = classify_index_candidate("x/.venv/gone.py", facts(false, 0)).map(|(v, _)| v);
        assert_eq!(got, Some(IndexVerdict::SkippedDir));
    }

    #[test]
    fn the_verdict_reads_the_disk_for_relative_and_absolute_keys() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("src/lib.rs"), "pub fn f() {}\n").unwrap();
        std::fs::create_dir_all(root.join(".claude/touring")).unwrap();
        std::fs::write(root.join(".claude/CLAUDE.md"), "# x\n").unwrap();
        std::fs::write(root.join(".claude/touring/state.json"), "{}\n").unwrap();
        std::fs::create_dir_all(root.join(".hidden")).unwrap();
        std::fs::write(root.join(".hidden/note.md"), "# x\n").unwrap();
        // No companions: the project root alone, so the judgement is hermetic.
        let policy = IndexPolicy::with_companions(root, Vec::new());

        assert_eq!(
            policy.verdict_for_key("src/lib.rs"),
            None,
            "eligible source"
        );
        assert_eq!(
            policy.verdict_for_key(".claude/CLAUDE.md"),
            None,
            "`.claude/` documents are walked — memories, rules and skills live there"
        );
        assert_eq!(
            policy
                .verdict_for_key(".claude/touring/state.json")
                .map(|(v, _)| v),
            Some(IndexVerdict::SkippedDir),
            "`.claude/touring/` is runtime state, refused by name"
        );
        assert_eq!(
            policy.verdict_for_key(".hidden/note.md").map(|(v, _)| v),
            Some(IndexVerdict::SkippedDir),
            "any other hidden directory is refused even though the file exists"
        );
        assert_eq!(
            policy.verdict_for_key("src/nope.rs").map(|(v, _)| v),
            Some(IndexVerdict::Missing)
        );
        // An absolute key outside the root is exactly what the hook writers used
        // to store (a scratchpad script, a `~/.claude/rules/*.md`).
        let outside = tempfile::tempdir().expect("tempdir");
        let foreign = outside.path().join("note.md");
        std::fs::write(&foreign, "# note\n").unwrap();
        assert_eq!(
            policy
                .verdict_for_key(&foreign.to_string_lossy())
                .map(|(v, _)| v),
            Some(IndexVerdict::OutsideRoot)
        );
        // An absolute key INSIDE the root is the same file as its relative key.
        let inside_abs = root.join("src/lib.rs");
        assert_eq!(policy.verdict_for_key(&inside_abs.to_string_lossy()), None);
        assert_eq!(policy.key_for(&inside_abs).as_deref(), Some("src/lib.rs"));
        assert_eq!(
            policy.key_for(&foreign),
            None,
            "outside every root there is no key"
        );
    }

    /// A companion root is a second root with its own key alphabet: files under
    /// it get `@companion/<name>/<rel>`, are judged by the same rules relative to
    /// that root, and a key whose root is not configured is named as such.
    #[test]
    fn a_declared_exclusion_is_root_relative_and_named_in_the_verdict() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        for rel in ["client/rules/x.md", "clientele/x.md", "src/client/y.rs"] {
            let p = root.join(rel);
            std::fs::create_dir_all(p.parent().expect("parent")).unwrap();
            std::fs::write(&p, "# x\n").unwrap();
        }
        let policy = IndexPolicy::with_companions(root, Vec::new())
            .with_excluded_dirs(vec!["client".to_string()]);
        let (verdict, detail) = policy
            .verdict_for_key("client/rules/x.md")
            .expect("excluded");
        assert_eq!(verdict, IndexVerdict::SkippedDir);
        assert!(detail.contains("exclude_dirs"), "{detail}");
        assert_eq!(
            policy
                .verdict_for_key(&root.join("client/rules/x.md").to_string_lossy())
                .map(|(v, _)| v),
            Some(IndexVerdict::SkippedDir),
            "the absolute spelling is judged by the same exclusion"
        );
        assert_eq!(
            policy.verdict_for_key("clientele/x.md"),
            None,
            "a prefix is a directory, not a string prefix"
        );
        assert_eq!(
            policy.verdict_for_key("src/client/y.rs"),
            None,
            "root-relative: a nested `client/` is code"
        );
        assert_eq!(
            excluded_dir_reason(&["client".to_string()], &["client"]).map(|(v, _)| v),
            Some(IndexVerdict::SkippedDir)
        );
        assert_eq!(excluded_dir_reason(&["a/b".to_string()], &["a"]), None);
        assert!(excluded_dir_reason(&["a/b".to_string()], &["a", "b"]).is_some());
    }

    #[test]
    fn companion_roots_get_stable_keys_and_the_same_rules() {
        let proj = tempfile::tempdir().expect("project");
        let notes = tempfile::tempdir().expect("notes");
        let root = proj.path();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("src/lib.rs"), "pub fn f() {}\n").unwrap();
        std::fs::create_dir_all(notes.path().join("deep/.hidden")).unwrap();
        std::fs::create_dir_all(notes.path().join(".claude/touring")).unwrap();
        std::fs::write(notes.path().join("a.md"), "# a\n").unwrap();
        std::fs::write(notes.path().join("deep/b.md"), "# b\n").unwrap();
        std::fs::write(notes.path().join("deep/.hidden/c.md"), "# c\n").unwrap();
        std::fs::write(notes.path().join(".claude/touring/knowledge.db"), "x").unwrap();
        let policy = IndexPolicy::with_companions(
            root,
            vec![CompanionRoot {
                name: "notes".to_string(),
                path: notes.path().to_path_buf(),
            }],
        );

        // Keys: project-relative under the root, `@companion/notes/…` under the companion.
        assert_eq!(
            policy.key_for(&root.join("src/lib.rs")).as_deref(),
            Some("src/lib.rs")
        );
        assert_eq!(
            policy.key_for(&notes.path().join("deep/b.md")).as_deref(),
            Some("@companion/notes/deep/b.md")
        );
        assert_eq!(policy.key_for(Path::new("/etc/hostname")), None);
        // Keys round-trip to the file they denote.
        assert_eq!(
            policy.path_for_key("@companion/notes/deep/b.md"),
            notes.path().join("deep/b.md")
        );
        assert_eq!(policy.path_for_key("src/lib.rs"), root.join("src/lib.rs"));

        // The same rules, relative to the companion root.
        assert_eq!(policy.verdict_for_key("@companion/notes/a.md"), None);
        assert_eq!(policy.verdict_for_key("@companion/notes/deep/b.md"), None);
        assert_eq!(
            policy
                .verdict_for_key("@companion/notes/deep/.hidden/c.md")
                .map(|(v, _)| v),
            Some(IndexVerdict::SkippedDir)
        );
        assert_eq!(
            policy
                .verdict_for_key("@companion/notes/.claude/touring/knowledge.db")
                .map(|(v, _)| v),
            Some(IndexVerdict::SkippedDir),
            "runtime state under a companion is refused like anywhere else"
        );
        assert_eq!(
            policy
                .verdict_for_key("@companion/notes/gone.md")
                .map(|(v, _)| v),
            Some(IndexVerdict::Missing)
        );
        // An absolute path under the companion is judged as its key would be.
        assert_eq!(
            policy.verdict_for_key(&notes.path().join("a.md").to_string_lossy()),
            None
        );
        // A key whose companion is not configured is named, not silently kept.
        let (v, d) = policy
            .verdict_for_key("@companion/nope/a.md")
            .expect("refused");
        assert_eq!(v, IndexVerdict::CompanionNotConfigured);
        assert!(d.contains("`nope`"), "{d}");
        // Outside every root: containment itself.
        assert_eq!(
            policy.verdict_for_key("/etc/hostname").map(|(v, _)| v),
            Some(IndexVerdict::OutsideRoot)
        );
    }

    /// Cross-audit 14/09/2026 (R2-2): the project's git rules are part of the
    /// verdict, reported after the walker's own rules and naming the rule.
    #[test]
    fn a_path_git_ignores_is_refused_with_its_rule_named() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        std::fs::create_dir_all(root.join("site/docs")).expect("site");
        std::fs::create_dir_all(root.join("docs")).expect("docs");
        std::fs::write(root.join("site/docs/run.py"), "x = 1\n").expect("site file");
        std::fs::write(root.join("docs/run.py"), "x = 1\n").expect("docs file");
        std::fs::write(root.join(".gitignore"), "/site/\n").expect(".gitignore");
        let policy = IndexPolicy::with_companions(root, Vec::new());

        let (verdict, detail) = policy
            .verdict_for_key("site/docs/run.py")
            .expect("git ignores the copy");
        assert_eq!(verdict, IndexVerdict::Gitignored);
        assert!(detail.contains("`/site/`"), "{detail}");
        assert_eq!(policy.verdict_for_key("docs/run.py"), None);
        let abs = root.join("site/docs/run.py");
        assert_eq!(
            policy
                .verdict_for_key(&abs.to_string_lossy())
                .map(|(v, _)| v),
            Some(IndexVerdict::Gitignored),
            "an absolute key under the root gets the same verdict"
        );
    }

    #[test]
    fn the_verdict_names_match_their_serialization() {
        for v in [
            IndexVerdict::Indexed,
            IndexVerdict::EligibleNotIndexed,
            IndexVerdict::OutsideRoot,
            IndexVerdict::SkippedDir,
            IndexVerdict::SkippedSubproject,
            IndexVerdict::Missing,
            IndexVerdict::Directory,
            IndexVerdict::UnsupportedExtension,
            IndexVerdict::Oversized,
            IndexVerdict::Unreadable,
            IndexVerdict::CompanionNotConfigured,
            IndexVerdict::Gitignored,
        ] {
            let json = serde_json::to_string(&v).unwrap();
            assert_eq!(json, format!("\"{}\"", v.as_str()), "{v:?}");
        }
    }
}
