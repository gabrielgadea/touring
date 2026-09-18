//! Wiring persistence layer for `FileKnowledgeDB` — pub-symbol →
//! consumer CRUD on the `wiring_map` table (orphan detection substrate).
//!
//! Carved from `touring-hooks/src/wiring.rs` in the daemon-lib-rearch
//! Phase C split (2026-06-10): these are inherent methods on
//! `FileKnowledgeDB`, and Rust requires inherent impls to live in the
//! crate that defines the type. The graph/engine layer (impact BFS, Tarjan
//! cycles, repair scans) stays in `touring_hooks::wiring`, which re-exports
//! everything here so historical paths keep resolving.

use std::borrow::Cow;

use rusqlite::{OptionalExtension, params};

use crate::knowledge::FileKnowledgeDB;

/// Producer kinds that can be CALLED (the F9 method-dispatch pass filter).
const CALLABLE_KINDS: &[&str] = &["method", "function", "async_function"];

/// Producer kinds usable as a type or const reference (the G3 pass filter —
/// 2026-08-12). `module` stays out: a module path match is provenance noise,
/// not a use of a named symbol.
const TYPE_KINDS: &[&str] = &[
    "struct",
    "enum",
    "const",
    "static",
    "type_alias",
    "trait",
    "union",
];

/// The root this database's paths are canonical against, **derived from the
/// database's own location** — never from the process environment.
///
/// Centralizing the root prevents path-homonimia: the same file MUST have
/// exactly one canonical representation across producer and consumer rows,
/// otherwise orphan detection produces 100% false positives for that row
/// (producer at path A, consumer at path B → no JOIN match).
///
/// # Why the location and not the environment
///
/// A per-process constant is the wrong SHAPE for this value. One daemon serves
/// the project whose socket it was spawned on, and the global daemon can hold
/// several projects at once — so the root that makes a path canonical is a
/// property of the DATABASE being written, never of whoever started the
/// process. Until 2026-08-19 it was `TOURING_WORKSPACE_ROOT` with a personal
/// path compiled in as the fallback, and both halves misfired:
///
/// - every per-project daemon **inherits** that variable from the session that
///   spawned it (`~/.claude/settings.json` exports it), so `analise`'s daemon
///   normalized `analise`'s paths against `touring`'s root. Nothing matched:
///   4.306 rows in `analise` and 7.893 in `konverter` stayed absolute, **none
///   of them under their own root** — every single one a foreign project's
///   file, which is what `doctor`'s `abs_paths` warning had been reporting;
/// - `migrate_canonicalize_paths` runs on every DB open and DELETEs rows that
///   start with the marker. Pointed at another project's root, that is a
///   deletion primitive aimed at the wrong data.
///
/// The canonical layout is `<root>/.claude/touring/<name>.db`, so the root is
/// three components up — and the two directory names are checked rather than
/// assumed, because stripping three components off an arbitrary path yields
/// `/` and would make every absolute path "relative". `$HOME` is refused:
/// `~/.claude/touring/knowledge.db` is the GLOBAL store, whose rows span
/// projects and therefore have no single root to strip.
///
/// Returns `None` when no root can be derived (in-memory DBs, non-canonical
/// layouts). `None` disables canonicalization — paths are stored as given,
/// which is exactly the previous behaviour for a non-matching prefix.
#[must_use]
pub(crate) fn derive_workspace_root(db_path: &std::path::Path) -> Option<String> {
    // Single source of truth: the inverse of `knowledge_db_canonical`, living
    // beside it so the layout cannot be changed on one side only.
    touring_foundation::config::TouringConfig::project_root_for_db(db_path)
}

/// Legacy resolution for databases with no derivable location (`:memory:`).
///
/// Kept as a fallback ONLY: it reads the environment, which is precisely what
/// cannot be trusted once more than one project is in play. There is no
/// compiled-in path — a developer's home directory is not a default.
#[must_use]
pub(crate) fn env_workspace_root() -> Option<String> {
    std::env::var("TOURING_WORKSPACE_ROOT").ok().map(|mut r| {
        if !r.ends_with('/') {
            r.push('/');
        }
        r
    })
}

/// Source-file extensions eligible for wiring, given the polyglot opt-in.
///
/// Single source of truth for BOTH the Rust write-gate
/// ([`is_indexable_module_file`]) and the SQL read-filters (orphan queries,
/// method-dispatch lookup) so the write and read sides can never drift. Default
/// (`false`) → `[".rs"]`, keeping every call site byte-identical to
/// pre-polyglot behavior.
#[must_use]
fn wireable_extensions(polyglot: bool) -> &'static [&'static str] {
    if polyglot {
        // Polyglot scope: Rust + Python (P-A) + TS/JS + Java (P-B). File-keyed
        // `.go` stays intentionally absent — a Go import denotes a package
        // (directory), not a file, so file-keyed wiring would register producers
        // with no resolvable consumers → false orphans. Go participates instead
        // via the package-aware `"go:<import-path>"` key namespace (P-G, admitted
        // by `is_go_package_wireable` + the `go:%` read-SQL clause), plan §11.
        &[
            ".rs", ".py", ".ts", ".tsx", ".js", ".jsx", ".mjs", ".cjs", ".java",
        ]
    } else {
        &[".rs"]
    }
}

/// SQL predicate matching `col` against the wireable-extension set for the
/// current polyglot mode. When polyglot is OFF this is exactly
/// `<col> LIKE '%.rs'` — byte-identical to the historical filter — so the
/// default read path is unchanged.
///
/// The extensions are compile-time constants (never user input), so the
/// interpolation carries no SQL-injection surface.
#[must_use]
fn wireable_ext_sql(col: &str, polyglot: bool) -> String {
    let mut clauses: Vec<String> = wireable_extensions(polyglot)
        .iter()
        .map(|ext| format!("{col} LIKE '%{ext}'"))
        .collect();
    if polyglot {
        // Go package-aware keys ("go:<import-path>") are synthetic identifiers,
        // not file paths — admit them on the read side too so orphan detection
        // and method-dispatch lookup see Go package producers. The prefix is a
        // compile-time constant (no injection surface). See P-G, plan §11.
        clauses.push(format!("{col} LIKE 'go:%'"));
    }
    if clauses.len() == 1 {
        clauses.into_iter().next().unwrap_or_default()
    } else {
        format!("({})", clauses.join(" OR "))
    }
}

/// Language families whose files can wire to each other. An import or a call
/// in one never names an item of another, so the by-name inference must never
/// join across them.
const LANGUAGE_FAMILIES: &[(&str, &[&str])] = &[
    ("rust", &[".rs"]),
    ("python", &[".py", ".pyi"]),
    (
        "js",
        &[".ts", ".tsx", ".mts", ".cts", ".js", ".jsx", ".mjs", ".cjs"],
    ),
    ("java", &[".java"]),
    ("go", &[".go"]),
];

/// The language family of a wiring path (a file, or a `go:<import-path>`
/// package key); `None` outside every family (docs, configs, dispatch rows).
#[must_use]
pub fn language_family(path: &str) -> Option<&'static str> {
    if path.starts_with("go:") {
        return Some("go");
    }
    LANGUAGE_FAMILIES
        .iter()
        .find(|(_, exts)| exts.iter().any(|ext| path.ends_with(ext)))
        .map(|(family, _)| *family)
}

/// SQL expression: the language family of `col`, NULL outside every family —
/// the same table as [`language_family`]. The extensions are compile-time
/// constants, so the interpolation carries no injection surface.
#[must_use]
pub fn language_family_sql(col: &str) -> String {
    let arms: String = LANGUAGE_FAMILIES
        .iter()
        .map(|(family, exts)| {
            let test = exts
                .iter()
                .map(|ext| format!("{col} LIKE '%{ext}'"))
                .collect::<Vec<_>>()
                .join(" OR ");
            format!(" WHEN {test} THEN '{family}'")
        })
        .collect();
    format!("(CASE WHEN {col} LIKE 'go:%' THEN 'go'{arms} END)")
}

/// SQL predicate over `wiring_map`: consumer and producer are both in a
/// language family, and not the same one — an edge no import or call makes.
///
/// 18/09/2026 (analise): 18.035 such rows, `.rs` consumers of Python and
/// TypeScript producers. 15.758 came from the by-name inference below (a bare
/// `cosine_similarity(…)` in a `.rs` file matched the Python function), the
/// rest from `touring wiring repair`; together they hid ~1.488 orphans.
#[must_use]
pub fn cross_language_edge_sql() -> String {
    let consumer = language_family_sql("consumer_file");
    let module = language_family_sql("module_file");
    format!("({consumer} IS NOT NULL AND {module} IS NOT NULL AND {consumer} != {module})")
}

/// ` AND <family of module_file> = '<family of the consumer>'`, or nothing
/// when the consumer is outside every family.
fn same_family_sql(consumer_hint: Option<&str>) -> String {
    consumer_hint
        .and_then(language_family)
        .map(|family| format!(" AND {} = '{family}'", language_family_sql("module_file")))
        .unwrap_or_default()
}

/// Non-wireable polyglot paths — the per-language analogue of the benches/tests
/// exclusion. Blocks the exact trees that produced the 258 historical
/// false-positive orphans (`docs/*.py`, `scripts/*.py`) plus vendored/generated
/// trees (venv / site-packages / node_modules / dist / `__pycache__`) and
/// per-language test files, so a polyglot opt-in never re-pollutes the orphan
/// diagnostic.
#[must_use]
fn is_non_rust_non_wireable(module_file: &str, source_packages: &[String]) -> bool {
    // Vendored / generated trees (Python venv/site-packages; JS/TS
    // node_modules + build output).
    is_vendored_or_generated(module_file) || is_first_party_non_source(module_file, source_packages)
}

/// The directories whose names mark non-source trees (`docs/`, `scripts/`).
const NON_SOURCE_DIRS: [&str; 2] = ["docs", "scripts"];

/// The top-level `docs/` and `scripts/` of `root` that are Python PACKAGES
/// (`__init__.py` present): source, not tooling. The analise project keeps its
/// main code in `scripts/` and imports it as `scripts.memoria`; the touring
/// workspace's `scripts/` has no `__init__.py` and stays out (14/09/2026).
#[must_use]
pub fn python_source_packages(root: Option<&std::path::Path>) -> Vec<String> {
    let Some(root) = root else {
        return Vec::new();
    };
    NON_SOURCE_DIRS
        .iter()
        .filter(|dir| root.join(dir).join("__init__.py").is_file())
        .map(|dir| (*dir).to_string())
        .collect()
}

/// The SQL form of the `docs/`/`scripts/` exclusion over `col`, leaving out the
/// directories that are source packages — the orphan queries must refuse exactly
/// what the write gate refuses.
#[must_use]
fn non_source_sql(col: &str, source_packages: &[String]) -> String {
    NON_SOURCE_DIRS
        .iter()
        .filter(|dir| !source_packages.iter().any(|p| p == *dir))
        .map(|dir| format!("AND {col} NOT LIKE '{dir}/%'"))
        .collect::<Vec<_>>()
        .join("\n               ")
}

/// The producer rows both orphan-shaped queries start from: a public symbol's
/// placeholder row, with every path and name filter already applied.
///
/// One source for [`KnowledgeGraph::orphan_symbols_with_trust`] and
/// [`KnowledgeGraph::internal_only_symbols`]: the two differ only in what they
/// say about the symbol's consumers, and a filter added to one of them by hand
/// would silently not apply to the other (B4, 15/09/2026).
#[must_use]
fn public_producer_rows_sql(ext_pred: &str, non_source: &str) -> String {
    format!(
        "SELECT w.module_file, w.symbol_name, w.symbol_kind, w.visibility,
                    w.consumer_file, w.import_line, w.contract_source
             FROM wiring_map w
             WHERE w.consumer_file IS NULL AND w.visibility = 'public'
               AND {ext_pred}
               AND w.module_file NOT LIKE 'benches/%'
               AND w.module_file NOT LIKE 'tests/%'
               {non_source}
               AND w.module_file NOT LIKE '%/benches/%'
               AND w.module_file NOT LIKE '%/tests/%'
               AND w.symbol_kind != 'module'
               AND w.symbol_name NOT IN ('fmt','hash','eq','partial_cmp','cmp','drop','clone','default')
               AND w.symbol_name NOT LIKE '\\_\\_%\\_\\_' ESCAPE '\\'"
    )
}

/// Third-party or machine-generated trees — never first-party source, in ANY
/// language.
///
/// Split out of [`is_non_rust_non_wireable`] on 2026-08-19 because the two
/// halves answer different questions and only this one is safe on the CONSUMER
/// side of an edge. A consumer inside `node_modules/` or `target/doc/` is an
/// artifact and its edge is noise; a consumer inside `tests/` is the project
/// using its own code, which is exactly what a wiring graph is for. Judging
/// both halves on the consumer removed 13.621 legitimate edges from this
/// workspace before the count gave it away.
#[must_use]
fn is_vendored_or_generated(module_file: &str) -> bool {
    module_file.contains("/site-packages/")
        || module_file.contains("/node_modules/")
        || module_file.starts_with("node_modules/")
        || module_file.contains("/__pycache__/")
        || module_file.contains("/.venv/")
        || module_file.starts_with(".venv/")
        || module_file.contains("/venv/")
        || module_file.starts_with("venv/")
        || module_file.contains("/dist/")
        || module_file.starts_with("dist/")
        || module_file.contains("/.next/")
        || module_file.contains("/coverage/")
        // Build OUTPUT (2026-08-19). The list above covered dependency trees
        // and one build dir (`dist/`) but not the compiler's own. Measured
        // before turning the polyglot opt-in on for this very workspace:
        // 1.087 of the 1.128 files that would have entered the graph were
        // `target/doc/**.js` — rustdoc's JavaScript. A generated artifact is
        // not a producer, and wiring it is how a feature earns a bad name on
        // the day it is switched on.
        || module_file.starts_with("target/")
        || module_file.contains("/target/")
        || module_file.starts_with("build/")
        || module_file.contains("/build/")
        || module_file.starts_with("out/")
        || module_file.contains("/out/")
        || module_file.contains("/.gradle/")
        || module_file.contains("/.tox/")
        || module_file.contains("/.mypy_cache/")
        || module_file.contains("/.pytest_cache/")
        || module_file.contains("/htmlcov/")
}

/// First-party paths that are not the project's own API surface: docs,
/// scripts and tests. They must not be PRODUCERS (the 258 historical false
/// positives were `docs/*.py` and `scripts/*.py` registering as public
/// symbols), but they are perfectly good CONSUMERS.
#[must_use]
fn is_first_party_non_source(module_file: &str, source_packages: &[String]) -> bool {
    // Non-source subtrees — the exact source of the 258 historical false
    // positives (docs/*.py, scripts/*.py) — unless the top-level directory is a
    // Python package (`python_source_packages`).
    let in_package = source_packages.iter().any(|p| {
        module_file
            .strip_prefix(p.as_str())
            .is_some_and(|rest| rest.starts_with('/'))
    });
    let non_source = !in_package
        && (module_file.starts_with("docs/")
            || module_file.contains("/docs/")
            || module_file.starts_with("scripts/")
            || module_file.contains("/scripts/"));
    let base = module_file.rsplit('/').next().unwrap_or(module_file);
    // pytest/unittest conventions.
    let py_test = base == "conftest.py" || base.starts_with("test_") || base.ends_with("_test.py");
    // JS/TS conventions: `foo.test.ts`, `foo.spec.tsx`, `__tests__/`, `__mocks__/`.
    let js_test = base.contains(".test.")
        || base.contains(".spec.")
        || module_file.contains("/__tests__/")
        || module_file.starts_with("__tests__/")
        || module_file.contains("/__mocks__/");
    // Java (Maven/Gradle) conventions: `FooTest.java`, `FooTests.java`, src/test/.
    let java_test = base.ends_with("Test.java")
        || base.ends_with("Tests.java")
        || module_file.contains("/src/test/");
    non_source || py_test || js_test || java_test
}

/// Whether a Go package-aware key (`"go:<import-path>"`) is eligible for wiring.
///
/// Go participates in the wiring graph via its **import-path** key rather than
/// file-keyed `.go` (a Go import denotes a package/directory carrying no symbol,
/// so file-keyed resolution registers producers with no resolvable consumers →
/// the false-orphan class Go was deferred over). Producers (exported symbols in
/// the package) and consumers (`import "<path>"` + `pkg.Sym()`) are both keyed
/// by `"go:<import-path>"`, so the `wiring_map` JOIN resolves across the
/// package's many files without a schema change.
///
/// Vendored dependencies (`/vendor/`) are excluded (third-party, not the
/// project's own surface); `_test.go` files are excluded at the feeder (only
/// non-test files register package producers). See P-G, plan §11.
#[must_use]
fn is_go_package_wireable(go_key: &str) -> bool {
    let path = go_key.strip_prefix("go:").unwrap_or(go_key);
    !path.is_empty() && !path.starts_with("vendor/") && !path.contains("/vendor/")
}

/// Returns `true` if `module_file` is a source file eligible for wiring
/// inspection: extension in [`wireable_extensions`], NOT inside a benches/ or
/// tests/ subtree, and (for polyglot files) not a vendored / docs / scripts /
/// test path.
///
/// The policy core of the write gate
/// ([`FileKnowledgeDB::is_indexable_module_file`]), taking `polyglot`
/// explicitly so both modes are unit-testable and so the caller supplies the
/// PROJECT's answer instead of a process-global one. Default mode is Rust-only;
/// a project opts in via `polyglot_wiring` in `.touring/touring.toml` (or the
/// `TOURING_POLYGLOT_WIRING` override), which also admits Python, TS/JS, Java
/// and Go package keys.
///
/// # Audit reference
///
/// Pre-fix evidence (2026-05-11 orphan-count audit):
/// - 43 false positives from `benches/src/*.rs` because the legacy filter
///   `LIKE '%/benches/%'` did not match leading `benches/` (no slash prefix).
/// - 258 false positives from `docs/*.py` and `scripts/*.py` because
///   `register_pub_symbol` accepted any extension. Under the polyglot opt-in
///   these stay blocked via [`is_non_rust_non_wireable`].
///
/// Test vocabulary only since 14/09/2026: production asks
/// [`is_indexable_module_file_with`] with the project's source packages.
#[cfg(test)]
#[must_use]
fn is_indexable_module_file_polyglot(module_file: &str, polyglot: bool) -> bool {
    is_indexable_module_file_with(module_file, polyglot, &[])
}

/// Whether a module file is admissible to the wiring map, for a project whose
/// top-level `source_packages` (see [`python_source_packages`]) are source.
fn is_indexable_module_file_with(
    module_file: &str,
    polyglot: bool,
    source_packages: &[String],
) -> bool {
    // Companion files (`@companion/<name>/…`, the ~/.claude skills, rules and
    // memories every project indexes for search) are never wiring: decision
    // 1-A, 14/09/2026 — see `touring_foundation::config::is_companion_key`.
    if touring_foundation::config::is_companion_key(module_file) {
        return false;
    }
    // Go package-aware keys ("go:<import-path>") are synthetic PACKAGE
    // identifiers, not file paths — the extension gate does not apply. Admitted
    // only under the polyglot opt-in. A Go package participates via its
    // import-path key (producers + consumers both keyed by "go:<import-path>"),
    // never via file-keyed `.go` (which would register producers with no
    // resolvable consumers → false orphans). See P-G, plan §11.
    if module_file.starts_with("go:") {
        return polyglot && is_go_package_wireable(module_file);
    }
    let ext_ok = wireable_extensions(polyglot)
        .iter()
        .any(|ext| module_file.ends_with(ext));
    if !ext_ok {
        return false;
    }
    // Reject benches/* and tests/* in any position (leading or nested) — all
    // languages. Both forms are checked because the wiring_map historically
    // stored both relative paths ("benches/src/foo.rs") and absolute paths
    // ("/home/.../crates/foo/benches/bar.rs").
    if module_file.starts_with("benches/")
        || module_file.starts_with("tests/")
        || module_file.contains("/benches/")
        || module_file.contains("/tests/")
    {
        return false;
    }
    // Polyglot (non-Rust) files carry a language-specific non-wireable set (the
    // 258-FP defense for non-Rust): vendored trees, docs/scripts, test files.
    if polyglot
        && !module_file.ends_with(".rs")
        && is_non_rust_non_wireable(module_file, source_packages)
    {
        return false;
    }
    true
}

/// The writer's admission vocabulary, exposed so a DIAGNOSTIC can CONSULT it
/// instead of approximating it.
///
/// `touring doctor`'s wiring census used to ask `module_file NOT LIKE '%.rs'`
/// — a different question from the one the writer asks, and one that erred in
/// both directions at once (measured 2026-08-19): it flagged 192.997 legitimate
/// rows in a Python project as pollution while calling 102 rows of
/// `benches/src/*.rs` clean, in the flagship workspace, because they end in
/// `.rs`. A diagnostic that disagrees with the thing it diagnoses reports on a
/// system that does not exist.
///
/// Pass `polyglot = true` to ask the mode-INDEPENDENT question — "could any
/// read admit this file?" — which is what "non-wireable" must mean: a vendored
/// tree or a `.json` is not wiring under any mode, whereas a `.py` in a Python
/// project is merely unread while the opt-in is off. `source_packages` are the
/// project's top-level Python packages that count as source (see
/// [`python_source_packages`]); the writer asks with the same list. One
/// predicate: the variant without the list lost its last caller when the
/// doctor started passing the packages (14/09/2026).
#[must_use]
pub fn is_wireable_source(module_file: &str, polyglot: bool, source_packages: &[String]) -> bool {
    is_indexable_module_file_with(module_file, polyglot, source_packages)
}

impl FileKnowledgeDB {
    /// The root this database's paths are canonical against, if one could be
    /// derived. See `derive_workspace_root` for why it comes from the DB's
    /// own location rather than the environment.
    #[must_use]
    pub fn workspace_root(&self) -> Option<&str> {
        self.workspace_root_ref()
    }

    /// Whether NON-Rust source participates in THIS database's wiring graph.
    ///
    /// Resolved once per open from the project's own config, never from a
    /// process-global: one daemon serves one project, the global daemon can
    /// serve several, and whether Python counts as wiring is a property of the
    /// project — the same argument as [`Self::workspace_root`].
    #[must_use]
    pub fn polyglot(&self) -> bool {
        self.polyglot_ref()
    }

    /// Whether `module_file` may enter this database's `wiring_map`.
    ///
    /// The write gate and the read filters must agree on the vocabulary, or a
    /// row is admitted that no query can ever see (or the reverse). Both now
    /// read the same per-database flag.
    #[must_use]
    pub(crate) fn is_indexable_module_file(&self, module_file: &str) -> bool {
        is_indexable_module_file_with(module_file, self.polyglot(), self.source_packages())
    }

    /// The top-level `docs/`/`scripts/` of this database's project that are
    /// Python packages, resolved once at open (see [`python_source_packages`]).
    #[must_use]
    pub fn source_packages(&self) -> &[String] {
        self.source_packages_ref()
    }

    /// Canonicalize `module_file` to a root-relative path.
    ///
    /// If `module_file` starts with this database's root, the prefix is
    /// stripped; otherwise the path is returned unchanged (borrowed).
    ///
    /// This is the single source of truth for path normalization in
    /// wiring_map. Calling it on the producer and the consumer sides ensures
    /// the JOIN matches even when one side was reported via absolute path
    /// (`/home/...`) and the other via relative path (`crates/...`).
    #[must_use]
    pub(crate) fn canonicalize_module_path<'a>(&self, module_file: &'a str) -> Cow<'a, str> {
        let root_relative = self
            .workspace_root_ref()
            .and_then(|root| module_file.strip_prefix(root))
            .unwrap_or(module_file);
        // W (2026-09-02): `./crates/x` and `crates/x` are ONE key. Rows written
        // before canonicalization existed carried the dot form; a rebuild that
        // writes the plain form then saw its producers with no consumers under
        // THEIR key — 78 false orphans, 115.986 edges one prefix away.
        Cow::Borrowed(root_relative.strip_prefix("./").unwrap_or(root_relative))
    }
}

/// How strong the evidence behind a wiring edge is (S1, 2026-08-07).
///
/// Before this enum, `wiring_map.contract_source` held the literal `'ast_read'`
/// in **77.679 of 77.679 rows** — a provenance column with a single value is a
/// column that does not exist. The consequence was diagnostic, not cosmetic: an
/// edge discovered by matching a bare method name (capped at 4 producers, so
/// deliberately lossy) was indistinguishable from an edge whose `use` statement
/// the resolver mapped to a real file. Both read as "wired", so a heuristic
/// hit silenced a symbol that might well be dead, and the orphan count could
/// not be partitioned into code debt versus resolver debt.
///
/// The variants are ordered weakest-to-strongest evidence; [`Self::strength`]
/// exposes that order so consumers can weigh an edge instead of merely counting
/// it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
pub enum WiringOrigin {
    /// A bare name matched a producer row — no import, no path resolution.
    /// The F9 method-dispatch pass produces these and caps fan-out at 4
    /// producers per name, so the match is explicitly best-effort.
    AstInferred,
    /// A regex-based extractor found the reference (languages without an AST
    /// front-end).
    TextMatched,
    /// An `use` statement whose module path the resolver mapped to a real file.
    AstResolved,
    /// A producer row: the symbol was read straight off the declaring AST.
    AstDeclared,
    /// A consumer edge resolved by the compiler itself (`rust-analyzer scip`
    /// ingest, H2 2026-08-12): rustc type resolution — method dispatch,
    /// generics, re-export identity. The only origin carrying the
    /// type-checker's own answer, so it ranks above every AST origin.
    ScipResolved,
}

impl WiringOrigin {
    /// Stable string written to `wiring_map.contract_source`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AstInferred => "ast_inferred",
            Self::TextMatched => "text_matched",
            Self::AstResolved => "ast_resolved",
            Self::AstDeclared => "ast_declared",
            Self::ScipResolved => "scip_resolved",
        }
    }

    /// Parse a `contract_source` value read back from the DB.
    ///
    /// The historical literal `'ast_read'` maps to [`Self::AstResolved`]: rows
    /// written before this enum came from the `use`-resolution path, which is
    /// exactly that tier. Anything unrecognised also degrades to
    /// `AstResolved` rather than to the weakest tier — inventing weakness for
    /// an unknown value would understate real edges, and this column's whole
    /// purpose is to stop guessing.
    #[must_use]
    pub fn from_contract_source(raw: &str) -> Self {
        match raw {
            "ast_inferred" => Self::AstInferred,
            "text_matched" => Self::TextMatched,
            "ast_declared" => Self::AstDeclared,
            "scip_resolved" => Self::ScipResolved,
            _ => Self::AstResolved,
        }
    }

    /// Evidence strength in `0.0..=1.0`, for provenance-attenuated ranking.
    #[must_use]
    pub const fn strength(self) -> f32 {
        match self {
            Self::AstInferred => 0.4,
            Self::TextMatched => 0.6,
            Self::AstResolved => 0.9,
            Self::AstDeclared => 1.0,
            Self::ScipResolved => 1.0,
        }
    }

    /// Whether the edge rests on a resolved path rather than a name guess.
    #[must_use]
    pub const fn is_resolved(self) -> bool {
        matches!(
            self,
            Self::AstResolved | Self::AstDeclared | Self::ScipResolved
        )
    }
}

/// An import the resolver could not map to any producer file.
///
/// The Gortex analogue is `name_only_candidates`: the count of call sites that
/// reference something the graph cannot locate, reported as its own number and
/// never folded into the resolved total.
#[derive(Debug, Clone, serde::Serialize)]
pub struct UnresolvedImport {
    /// The module path as written in the source (`touring_foo::bar`).
    pub module_path: String,
    /// The symbol requested from that module.
    pub symbol_name: String,
    /// File that contains the unresolvable import.
    pub consumer_file: String,
    /// Line of the import, when the extractor knows it.
    pub import_line: Option<i64>,
    /// Source language of the consumer file.
    pub language: String,
}

/// A pub symbol's wiring status.
#[derive(Debug, Clone)]
pub struct WiringEntry {
    /// File that declares the public symbol.
    pub module_file: String,
    /// Name of the public symbol.
    pub symbol_name: String,
    /// Kind of the symbol (e.g. `"fn"`, `"struct"`).
    pub symbol_kind: String,
    /// Visibility modifier (e.g. `"pub"`, `"pub(crate)"`).
    pub visibility: String,
    /// File that consumes the symbol, if any consumer was found.
    pub consumer_file: Option<String>,
    /// Line number of the import in the consumer file, if known.
    pub import_line: Option<i64>,
    /// Source of the wiring contract (e.g. how the link was discovered).
    pub contract_source: String,
}

/// Map a rusqlite [`Row`] to a [`WiringEntry`].
///
/// Column order matches every `SELECT` in this file:
/// `module_file(0)`, `symbol_name(1)`, `symbol_kind(2)`, `visibility(3)`,
/// `consumer_file(4)`, `import_line(5)`, `contract_source(6)`.
///
/// Centralising the projection here means all three query functions
/// (`orphan_symbols`, `orphan_symbols_for_module`, `all_pub_symbols`) share
/// a single authoritative column-index map — a change to the SELECT list
/// only needs to be made once.
fn row_to_wiring_entry(row: &rusqlite::Row<'_>) -> rusqlite::Result<WiringEntry> {
    Ok(WiringEntry {
        module_file: row.get::<_, String>(0)?,
        symbol_name: row.get::<_, String>(1)?,
        symbol_kind: row.get::<_, String>(2)?,
        visibility: row.get::<_, String>(3)?,
        consumer_file: row.get::<_, Option<String>>(4)?,
        import_line: row.get::<_, Option<i64>>(5)?,
        contract_source: row.get::<_, String>(6)?,
    })
}

/// Summary of a module's integration status.
#[derive(Debug, Clone)]
pub struct ModuleWiringStatus {
    /// Path of the module being summarized.
    pub file_path: String,
    /// Total number of public symbols declared in the module.
    pub total_pub_symbols: usize,
    /// Number of public symbols that have at least one consumer.
    pub symbols_with_consumers: usize,
    /// Ratio of wired to total public symbols (1.0 = fully wired).
    pub integration_score: f64,
    /// Names of public symbols with no consumers (orphans).
    pub orphan_symbols: Vec<String>,
}

/// Single-row result from `wiring_modules_aggregate` — one row per module.
///
/// Wave 22 (S-Q1a): returned by the O(1) aggregate query that replaces
/// the old O(N*3) per-module query loop.
#[derive(Debug, Clone)]
pub struct WiringModuleAggregateRow {
    /// Relative file path of the module.
    pub module_file: String,
    /// Total distinct public symbols registered for this module.
    pub total_pub: i64,
    /// Distinct public symbols that have at least one consumer.
    pub wired_count: i64,
}

impl WiringModuleAggregateRow {
    /// Compute integration_score from aggregate counts.
    ///
    /// Returns 1.0 when `total_pub == 0` (nothing to wire = fully integrated).
    #[must_use]
    pub fn integration_score(&self) -> f64 {
        if self.total_pub == 0 {
            1.0
        } else {
            self.wired_count as f64 / self.total_pub as f64
        }
    }
}

/// Census of the wiring_map table for the `touring doctor` diagnostic.
///
/// All fields are signed (`i64`) because they are populated from SQL
/// aggregate functions (`SUM`, `COUNT`) which return signed integers in
/// SQLite; treating them as unsigned would mask negative anomalies that
/// indicate schema corruption.
#[derive(Debug, Clone, serde::Serialize)]
pub struct WiringDbDiagnostic {
    /// Total number of rows in the wiring_map table.
    pub total_rows: i64,
    /// Rows representing producers (symbol declarations).
    pub producer_rows: i64,
    /// Rows representing consumers (symbol imports/uses).
    pub consumer_rows: i64,
    /// Number of producer rows whose symbol is public.
    pub pub_producers: i64,
    /// Count of distinct public symbols tracked.
    pub distinct_pub_symbols: i64,
    /// Rows whose symbol kind could not be determined.
    pub kind_unknown_count: i64,
    /// Rows no read admits under ANY mode — vendored trees, `docs/`,
    /// `scripts/`, `tests/`, `benches/`, test files, extensions outside the
    /// vocabulary. This is the counter that JUDGES: non-zero means the census
    /// beside it is biased.
    ///
    /// It replaced `non_rust_rows` on 2026-08-19. "Not Rust" was a different
    /// question from "not wiring", and it answered wrong in both directions:
    /// 192.997 legitimate Python rows flagged in `analise`, 102 rows of
    /// `benches/src/*.rs` absolved in `touring` itself.
    pub non_wireable_rows: i64,
    /// Supported-language sources the CURRENT mode filters out of every query.
    ///
    /// Informative, never a defect: it is the answer to "why is my Python
    /// project's wiring thin?" — turn on `polyglot_wiring` and these rows
    /// become readable.
    pub unread_rows: i64,
    /// Call sites the resolver could not map to any producer file (S1).
    ///
    /// Lives in `wiring_unresolved`, never summed into the counters above:
    /// "we could not look" is not the same fact as "we looked and found
    /// nothing", and merging them is what made the orphan number unreadable.
    ///
    /// `None` = **not measured** (the table is absent, e.g. a DB opened before
    /// the v9 migration). A reader must not read that as zero.
    pub name_only_candidates: Option<i64>,
    /// Edges wired by bare-name matching (`ast_inferred`) rather than by a
    /// resolved import path — the tier whose evidence is weakest.
    pub heuristic_edges: i64,
}

impl FileKnowledgeDB {
    /// Register a pub symbol in the wiring map.
    ///
    /// Called after post-read extracts pub symbols from a module.
    /// Sets consumer_file = NULL initially (orphan until proven otherwise).
    ///
    /// Skips non-Rust files (`.py`, `.md`, `.json`, …) and entries inside
    /// `benches/` or `tests/` subtrees — see `is_indexable_module_file` for
    /// the policy. Skipped calls return `Ok(())` (no-op) so callers do not
    /// need to gate; this centralizes the eligibility rule.
    ///
    /// Paths are canonicalized to workspace-relative form before INSERT to
    /// prevent path-homonimia (the same file represented as both
    /// `/home/.../crates/foo.rs` and `crates/foo.rs` historically caused
    /// 291 false-positive orphans).
    pub fn register_pub_symbol(
        &self,
        module_file: &str,
        symbol_name: &str,
        symbol_kind: &str,
        visibility: &str,
    ) -> Result<(), rusqlite::Error> {
        self.register_pub_symbol_with_origin(
            module_file,
            symbol_name,
            symbol_kind,
            visibility,
            WiringOrigin::AstDeclared,
        )
    }

    /// [`Self::register_pub_symbol`] with an explicit provenance tier.
    ///
    /// Producer rows are `AstDeclared` in every current caller — the symbol was
    /// read off the declaring AST. The parameter exists so a future extractor
    /// with weaker evidence (a regex pass over a language without a front-end)
    /// records that weakness instead of borrowing the AST's credibility.
    pub fn register_pub_symbol_with_origin(
        &self,
        module_file: &str,
        symbol_name: &str,
        symbol_kind: &str,
        visibility: &str,
        origin: WiringOrigin,
    ) -> Result<(), rusqlite::Error> {
        self.register_pub_symbol_counted(module_file, symbol_name, symbol_kind, visibility, origin)
            .map(|_| ())
    }

    /// [`Self::register_pub_symbol_with_origin`] that says whether a row was
    /// written: `false` when the eligibility gate refused the file or the
    /// producer row already existed. Callers that REPORT a count use this one —
    /// counting every `Ok` inflated the rebuild's `wiring_entries` with rows the
    /// gate refused (cross-audit 14/09/2026, B5).
    ///
    /// # Errors
    ///
    /// Propagates any `rusqlite` failure.
    pub fn register_pub_symbol_counted(
        &self,
        module_file: &str,
        symbol_name: &str,
        symbol_kind: &str,
        visibility: &str,
        origin: WiringOrigin,
    ) -> Result<bool, rusqlite::Error> {
        let canonical = self.canonicalize_module_path(module_file);
        if !self.is_indexable_module_file(&canonical) {
            return Ok(false);
        }
        let written = self.conn_ref().execute(
            "INSERT OR IGNORE INTO wiring_map (module_file, symbol_name, symbol_kind, visibility, contract_source)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                canonical.as_ref(),
                symbol_name,
                symbol_kind,
                visibility,
                origin.as_str()
            ],
        )?;
        // Invalidate aggregate cache — new pub symbol changes module totals.
        Self::invalidate_wiring_modules_cache();
        Ok(written > 0)
    }

    /// Record that a consumer file imports a specific symbol from a module.
    ///
    /// Resolves the orphan status for this symbol (sets consumer_file).
    /// Applies the same canonicalization and eligibility gate as
    /// `register_pub_symbol` so producer rows and consumer rows match by
    /// path.
    pub fn record_consumer(
        &self,
        module_file: &str,
        symbol_name: &str,
        consumer_file: &str,
        import_line: Option<i64>,
    ) -> Result<(), rusqlite::Error> {
        self.record_consumer_with_origin(
            module_file,
            symbol_name,
            consumer_file,
            import_line,
            WiringOrigin::AstResolved,
        )
    }

    /// [`Self::record_consumer`] with an explicit provenance tier.
    ///
    /// The default of the plain method is [`WiringOrigin::AstResolved`] because
    /// that is what its callers actually do — resolve an `use` path to a file
    /// (77.558 of 77.679 rows are `rust_import`). The one caller whose evidence
    /// is weaker is the F9 method-dispatch pass, which matches a bare name and
    /// caps fan-out at 4 producers; it passes [`WiringOrigin::AstInferred`], so
    /// a guess stops being recorded as a resolution.
    pub fn record_consumer_with_origin(
        &self,
        module_file: &str,
        symbol_name: &str,
        consumer_file: &str,
        import_line: Option<i64>,
        origin: WiringOrigin,
    ) -> Result<(), rusqlite::Error> {
        let canonical_module = self.canonicalize_module_path(module_file);
        let canonical_consumer = self.canonicalize_module_path(consumer_file);
        // A companion file consumes nothing either: its imports resolve to
        // absolute paths under ~/.claude, rows the phantom-module sweep deleted
        // on the next rebuild (three of them in the analise, 14/09/2026).
        if !self.is_indexable_module_file(&canonical_module)
            || touring_foundation::config::is_companion_key(&canonical_consumer)
        {
            return Ok(());
        }
        // Wave H+1 (2026-06-11): normalize the import form before keying.
        // `use m::{X as Y}` arrives as the literal "X as Y" — the producer row
        // is keyed by the ORIGINAL name X (the alias is consumer-local detail).
        let symbol_name = symbol_name
            .split_once(" as ")
            .map_or(symbol_name, |(orig, _alias)| orig.trim());
        // A weaker guess never overwrites stronger evidence for the same edge:
        // `INSERT OR REPLACE` let an inferred name match demote a resolved import
        // (cross-audit 14/09/2026, B8). Equal or stronger evidence replaces.
        let existing: Option<String> = self
            .conn_ref()
            .query_row(
                "SELECT contract_source FROM wiring_map
                 WHERE module_file = ?1 AND symbol_name = ?2 AND consumer_file = ?3",
                params![
                    canonical_module.as_ref(),
                    symbol_name,
                    canonical_consumer.as_ref()
                ],
                |r| r.get(0),
            )
            .optional()?;
        if existing.is_some_and(|raw| {
            WiringOrigin::from_contract_source(&raw).strength() > origin.strength()
        }) {
            return Ok(());
        }
        // `use m::*` is module-level wiring, not a symbol: give it a dedicated
        // kind instead of polluting the 'unknown' (schema-degraded) bucket.
        if symbol_name == "*" {
            self.conn_ref().execute(
                "INSERT OR REPLACE INTO wiring_map
                 (module_file, symbol_name, symbol_kind, visibility, consumer_file, import_line, contract_source, resolved_at)
                 VALUES (?1, '*', 'glob_import', 'public', ?2, ?3, ?4, datetime('now'))",
                params![
                    canonical_module.as_ref(),
                    canonical_consumer.as_ref(),
                    import_line,
                    origin.as_str()
                ],
            )?;
            Self::invalidate_wiring_modules_cache();
            return Ok(());
        }
        self.conn_ref().execute(
            "INSERT OR REPLACE INTO wiring_map
             (module_file, symbol_name, symbol_kind, visibility, consumer_file, import_line, contract_source, resolved_at)
             VALUES (?1, ?2,
                COALESCE(
                    (SELECT symbol_kind FROM wiring_map WHERE module_file = ?1 AND symbol_name = ?2 AND consumer_file IS NULL LIMIT 1),
                    -- Wave H+1 (2026-06-11): re-export fallback. Facade mod.rs files
                    -- (`pub use`) resolve imports to a module_file with no producer
                    -- row; the symbol's kind is invariant to where it is re-exported,
                    -- so any known-kind producer row for the same name beats 'unknown'
                    -- (homonym kinds may differ across crates — still strictly better).
                    (SELECT symbol_kind FROM wiring_map WHERE symbol_name = ?2 AND consumer_file IS NULL AND symbol_kind != 'unknown' LIMIT 1),
                    'unknown'),
                COALESCE((SELECT visibility FROM wiring_map WHERE module_file = ?1 AND symbol_name = ?2 AND consumer_file IS NULL LIMIT 1), 'public'),
                ?3, ?4, ?5, datetime('now'))",
            params![
                canonical_module.as_ref(),
                symbol_name,
                canonical_consumer.as_ref(),
                import_line,
                origin.as_str()
            ],
        )?;
        // Invalidate aggregate cache — consumer resolution changes wired_count.
        Self::invalidate_wiring_modules_cache();
        Ok(())
    }

    /// Record an import whose module path the resolver could not map to a file.
    ///
    /// This is the write path that did not exist (S1, 2026-08-07). When
    /// resolution failed, the caller simply skipped the record — so the call
    /// site vanished, and the producer it *would* have wired stayed
    /// indistinguishable from genuinely dead code. Counting those failures is
    /// what turns the orphan number into a partition: code debt on one side,
    /// resolver debt on the other.
    ///
    /// Rows live in their own table, never in `wiring_map`, precisely so they
    /// can never be mistaken for a consumer and silently erase a true orphan.
    /// `INSERT OR IGNORE` keyed on `(module_path, symbol_name, consumer_file)`
    /// makes a rebuild idempotent.
    pub fn record_unresolved_import(
        &self,
        module_path: &str,
        symbol_name: &str,
        consumer_file: &str,
        import_line: Option<i64>,
        language: &str,
    ) -> Result<(), rusqlite::Error> {
        self.record_unresolved_import_classified(
            module_path,
            symbol_name,
            consumer_file,
            import_line,
            language,
            "workspace_unresolved",
        )
    }

    /// [`Self::record_unresolved_import`] with the caller's classification.
    ///
    /// `class` separates the three facts the raw count merges (see
    /// `symbol_extractors::UnresolvedClass`): a `super::` import and a `serde`
    /// import are both unresolved and neither is a defect, while an unresolved
    /// path into a workspace crate is. The classification is the caller's
    /// because only it knows the crate map the resolution attempt used —
    /// deriving it here would let the verdict drift from the attempt.
    pub fn record_unresolved_import_classified(
        &self,
        module_path: &str,
        symbol_name: &str,
        consumer_file: &str,
        import_line: Option<i64>,
        language: &str,
        class: &str,
    ) -> Result<(), rusqlite::Error> {
        let canonical_consumer = self.canonicalize_module_path(consumer_file);
        // Resolver debt is project debt: a companion import is not counted.
        if touring_foundation::config::is_companion_key(&canonical_consumer) {
            return Ok(());
        }
        self.conn_ref().execute(
            "INSERT OR IGNORE INTO wiring_unresolved
             (module_path, symbol_name, consumer_file, import_line, language, class)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                module_path,
                symbol_name,
                canonical_consumer.as_ref(),
                import_line,
                language,
                class
            ],
        )?;
        Ok(())
    }

    /// Unresolved call sites grouped by class — the honest breakdown.
    ///
    /// `None` when the table is absent (not measured). Only the
    /// `workspace_unresolved` bucket is resolver debt; the others are expected
    /// non-resolution and are reported so nobody has to guess which is which.
    pub fn unresolved_by_class(&self) -> Option<Vec<(String, i64)>> {
        if !self.unresolved_table_present() {
            return None;
        }
        let mut stmt = self
            .conn_ref()
            .prepare(
                "SELECT COALESCE(class, 'workspace_unresolved') AS c, COUNT(*)
                 FROM wiring_unresolved GROUP BY c ORDER BY 2 DESC",
            )
            .ok()?;
        let rows = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })
            .ok()?
            .filter_map(Result::ok)
            .collect();
        Some(rows)
    }

    /// Whether this DB carries the `wiring_unresolved` table.
    ///
    /// Needed because "the table is missing" and "the table is empty" are
    /// different facts that a bare `COUNT(*)` collapses into the same `0`.
    /// Learned the hard way on 2026-08-07: `ensure_schema` runs only when
    /// `user_version < SCHEMA_VERSION`, so before the v9 bump the table never
    /// materialised on existing DBs, every write failed into `let _ =`, and
    /// this counter answered a confident, entirely fictional zero.
    pub fn unresolved_table_present(&self) -> bool {
        self.conn_ref()
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='wiring_unresolved'",
                [],
                |r| r.get::<_, i64>(0),
            )
            .map(|c| c > 0)
            .unwrap_or(false)
    }

    /// Count of unresolved call sites — the Gortex `name_only_candidates`.
    ///
    /// `None` means **not measured** (table absent); `Some(0)` means measured
    /// and genuinely empty. Reported as its own number and never added to the
    /// resolved total: an aggregate that mixes "we looked and found nothing"
    /// with "we could not look" is the approximation this change exists to
    /// remove — and it must not reappear inside its own implementation.
    pub fn name_only_candidates(&self) -> Option<i64> {
        if !self.unresolved_table_present() {
            return None;
        }
        self.conn_ref()
            .query_row("SELECT COUNT(*) FROM wiring_unresolved", [], |r| r.get(0))
            .ok()
    }

    /// The distinct module paths that failed to resolve, most frequent first.
    ///
    /// This is the actionable half of [`Self::name_only_candidates`]: a single
    /// unmapped crate prefix can account for thousands of phantom orphans, and
    /// the ranking says which resolver gap to close first.
    pub fn unresolved_module_paths(
        &self,
        limit: usize,
    ) -> Result<Vec<(String, i64)>, rusqlite::Error> {
        // Debt only. Ranking `super` and `serde` at the top of a "fix these"
        // list would send every reader chasing non-problems — the ranking has
        // to point at what is actually broken.
        let mut stmt = self.conn_ref().prepare(
            "SELECT module_path, COUNT(*) AS n FROM wiring_unresolved
             WHERE COALESCE(class, 'workspace_unresolved') = 'workspace_unresolved'
             GROUP BY module_path ORDER BY n DESC, module_path ASC LIMIT ?1",
        )?;
        let rows = stmt
            .query_map(params![limit as i64], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })?
            .filter_map(Result::ok)
            .collect();
        Ok(rows)
    }

    /// Drop unresolved rows recorded for a consumer file, before re-scanning it.
    ///
    /// Without this an import that later starts resolving would leave its old
    /// failure row behind forever, and the resolver-debt number could only ever
    /// grow — a metric that cannot improve is a metric nobody acts on.
    pub fn clear_unresolved_for_consumer(
        &self,
        consumer_file: &str,
    ) -> Result<usize, rusqlite::Error> {
        let canonical = self.canonicalize_module_path(consumer_file);
        self.conn_ref().execute(
            "DELETE FROM wiring_unresolved WHERE consumer_file = ?1",
            params![canonical.as_ref()],
        )
    }

    /// The unresolved call sites themselves, newest first.
    ///
    /// The counts answer "how much"; this answers "which" — the form a human
    /// needs to actually go fix a resolver gap. `class` filters to one bucket
    /// (pass `"workspace_unresolved"` for the debt-only view).
    pub fn unresolved_imports(
        &self,
        class: Option<&str>,
        limit: usize,
    ) -> Result<Vec<UnresolvedImport>, rusqlite::Error> {
        if !self.unresolved_table_present() {
            return Ok(Vec::new());
        }
        let where_clause = if class.is_some() {
            "WHERE COALESCE(class, 'workspace_unresolved') = ?1"
        } else {
            "WHERE ?1 IS NULL OR 1 = 1"
        };
        let sql = format!(
            "SELECT module_path, symbol_name, consumer_file, import_line, language
             FROM wiring_unresolved {where_clause}
             ORDER BY id DESC LIMIT ?2"
        );
        let mut stmt = self.conn_ref().prepare(&sql)?;
        let rows = stmt
            .query_map(params![class, limit as i64], |row| {
                Ok(UnresolvedImport {
                    module_path: row.get(0)?,
                    symbol_name: row.get(1)?,
                    consumer_file: row.get(2)?,
                    import_line: row.get(3)?,
                    language: row.get(4)?,
                })
            })?
            .filter_map(Result::ok)
            .collect();
        Ok(rows)
    }

    /// Row counts of `wiring_map` grouped by provenance tier.
    ///
    /// Sorted strongest-evidence-first so a caller printing it reads the
    /// resolved bulk before the heuristic tail.
    pub fn origin_breakdown(&self) -> Result<Vec<(String, i64)>, rusqlite::Error> {
        let mut stmt = self.conn_ref().prepare(
            "SELECT COALESCE(contract_source, 'unknown') AS src, COUNT(*)
             FROM wiring_map GROUP BY src",
        )?;
        let mut rows: Vec<(String, i64)> = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })?
            .filter_map(Result::ok)
            .collect();
        rows.sort_by(|a, b| {
            WiringOrigin::from_contract_source(&b.0)
                .cmp(&WiringOrigin::from_contract_source(&a.0))
                .then_with(|| a.0.cmp(&b.0))
        });
        Ok(rows)
    }

    /// Re-resolve `symbol_kind = 'unknown'` consumer rows from known-kind
    /// producer rows (Wave H+1, 2026-06-11 — closes the doctor
    /// `wiring_diagnostic` pollution warning).
    ///
    /// Two pollution sources exist: (a) imports resolved to a facade mod.rs
    /// (`pub use` re-export) where no producer row lives, and (b) consumers
    /// indexed BEFORE their producer during a rebuild walk (the INSERT-time
    /// COALESCE saw an empty table and froze 'unknown'). Both are fixable
    /// after the fact: the kind of a symbol name is recoverable from any
    /// producer row. Returns the number of rows repaired.
    /// `mark_extern` gates the SECOND pass only. Pass 1 (inherit the kind from a
    /// known producer) adds information that is correct regardless of how much of
    /// the tree was walked. Pass 2 concludes "no producer exists anywhere,
    /// therefore this symbol is defined OUTSIDE the workspace" — a conclusion that
    /// is only sound over a COMPLETE walk. Called with `false` after a partial
    /// walk, it leaves those rows `unknown` so a later complete rebuild can still
    /// resolve them; `extern` is terminal, and a wrong `extern` never gets a
    /// second look.
    ///
    /// Observed 20/08/2026 on `analise`: a rebuild that aborted at 22.994/37.295
    /// files under memory pressure marked 2.401 rows `extern` — with 38% of the
    /// producers never read. The sweep and the phantom purge already skip on
    /// `aborted_memory_pressure` for exactly this reason; this pass did not.
    pub fn backfill_unknown_consumer_kinds(
        &self,
        mark_extern: bool,
    ) -> Result<usize, rusqlite::Error> {
        let n = self.conn_ref().execute(
            "UPDATE wiring_map AS w
             SET symbol_kind = (
                 SELECT p.symbol_kind FROM wiring_map p
                 WHERE p.symbol_name = w.symbol_name
                   AND p.consumer_file IS NULL
                   AND p.symbol_kind != 'unknown'
                 LIMIT 1)
             WHERE w.symbol_kind = 'unknown'
               AND EXISTS (
                 SELECT 1 FROM wiring_map p
                 WHERE p.symbol_name = w.symbol_name
                   AND p.consumer_file IS NULL
                   AND p.symbol_kind != 'unknown')",
            [],
        )?;
        // Second pass: rows whose symbol name has NO producer row anywhere in
        // the index are wiring to symbols defined OUTSIDE the workspace (e.g.
        // `pub use pretty_assertions::assert_eq` facades) — structurally
        // unrecoverable, and not schema degradation. Mark them 'extern'.
        // Sound ONLY over a complete walk (see `mark_extern` above).
        if !mark_extern {
            if n > 0 {
                Self::invalidate_wiring_modules_cache();
            }
            return Ok(n);
        }
        let m = self.conn_ref().execute(
            "UPDATE wiring_map AS w
             SET symbol_kind = 'extern'
             WHERE w.symbol_kind = 'unknown'
               AND NOT EXISTS (
                 SELECT 1 FROM wiring_map p
                 WHERE p.symbol_name = w.symbol_name
                   AND p.consumer_file IS NULL
                   AND p.symbol_kind != 'unknown')",
            [],
        )?;
        if n + m > 0 {
            Self::invalidate_wiring_modules_cache();
        }
        Ok(n + m)
    }

    /// The single, deterministic resolution of every consumer row's kind from
    /// the FINAL producer table — the pass that makes a cold rebuild equal a warm
    /// one (I16, 2026-09-13).
    ///
    /// At record time a consumer row copies the kind of whatever producer row
    /// happens to exist: on a warm rebuild that is the OLD row of a module not yet
    /// re-walked, on a cold one it is nothing (`unknown`, later repaired by
    /// [`Self::backfill_unknown_consumer_kinds`] through a `LIMIT 1` with no
    /// order). Measured on the isolated per-project copy: `symbols` identical
    /// between a cold and a warm rebuild, `wiring_map` different in 24 lines, every
    /// one of them a consumer kind picked by that arbitrary homonym
    /// (`extract_symbols` method↔function, `rewrite` function↔module). This pass
    /// overwrites EVERY consumer kind from the producers as they stand after the
    /// walk, with a total order on the fallback, so the result is a function of
    /// the producer table alone and of nothing the walk order or the previous
    /// index left behind:
    ///
    /// 1. the producer of the same name in the same module;
    /// 2. else the homonym producer that sorts first by `(module_file, symbol_kind)`;
    /// 3. else `extern` — only over a COMPLETE walk (`mark_extern`), for the same
    ///    reason the backfill gates it: `extern` is terminal, and a producer that
    ///    was simply never read must not be branded outside the workspace.
    ///
    /// `glob_import` rows (`use m::*`) are module-level wiring, not a symbol, and
    /// keep their kind. Returns the number of rows whose kind CHANGED, so a second
    /// run over the same table reports 0 — the idempotence a test can assert.
    ///
    /// # Errors
    ///
    /// Propagates any `rusqlite` failure.
    pub fn resolve_consumer_kinds_from_producers(
        &self,
        mark_extern: bool,
    ) -> Result<usize, rusqlite::Error> {
        // One expression for the resolved kind; repeated in the WHERE so only rows
        // whose kind actually changes are counted (SQLite reports rows matched).
        // The last arm is the verdict for a name NO producer carries: `extern`
        // after a complete walk, `unknown` after a partial one — whatever kind the
        // row held before (a stale copy of a producer that no longer exists) is
        // never trusted, or a warm rebuild would keep it and a cold one would not.
        const RESOLVED: &str = "COALESCE(
                 (SELECT p.symbol_kind FROM wiring_map p
                  WHERE p.module_file = w.module_file AND p.symbol_name = w.symbol_name
                    AND p.consumer_file IS NULL AND p.symbol_kind != 'unknown'
                  ORDER BY p.symbol_kind LIMIT 1),
                 (SELECT p.symbol_kind FROM wiring_map p
                  WHERE p.symbol_name = w.symbol_name
                    AND p.consumer_file IS NULL AND p.symbol_kind != 'unknown'
                  ORDER BY p.module_file, p.symbol_kind LIMIT 1),
                 ?1)";
        let sql = format!(
            "UPDATE wiring_map AS w SET symbol_kind = {RESOLVED}
             WHERE w.consumer_file IS NOT NULL
               AND w.symbol_kind != 'glob_import'
               AND w.symbol_kind IS NOT {RESOLVED}"
        );
        let fallback = if mark_extern { "extern" } else { "unknown" };
        let n = self.conn_ref().execute(&sql, params![fallback])?;
        if n > 0 {
            Self::invalidate_wiring_modules_cache();
        }
        Ok(n)
    }

    /// Find producer rows whose `symbol_name` matches any of the supplied
    /// names AND whose `symbol_kind` is callable (method / function /
    /// async_function). Returns `(module_file, symbol_name)` pairs.
    ///
    /// Used by the F9 method-dispatch wiring pass: the caller walks the AST
    /// of a consumer file, collects every identifier appearing in a call
    /// expression, then calls this method to discover which producer rows
    /// can be wired to it.
    ///
    /// Returns at most `cap_per_name` rows per distinct symbol_name to bound
    /// blow-up when a generic method name (`clone`, `iter`) is called —
    /// without the cap, a single `.clone()` in one file would wire every
    /// `pub fn clone` in the workspace, producing thousands of
    /// fan-out consumer rows per call site. With the cap, we accept that
    /// some method orphans will stay orphan (conservative direction).
    ///
    /// Performance: single SQL roundtrip via parameterized IN clause. Cost
    /// scales with the size of `names` (typical: 5-100 unique names per
    /// file) plus the number of producer rows returned (typical: 0-200).
    /// Producers for qualified calls, one per `(qualifier, name)` pair.
    ///
    /// The qualifier is the module the call names (`super::backup::run()` →
    /// `("backup", "run")`), so the producer is the public callable of that name
    /// living in `…/backup.rs` or `…/backup/mod.rs`. That predicate is what makes
    /// the match single-valued: resolving the bare name `run` faces 130 producers
    /// in this workspace and the by-name path caps at 4 arbitrary ones (measured
    /// 2026-09-02, which is why 94 dispatch handlers read as false orphans).
    ///
    /// Ties across crates are broken toward the consumer's own crate, then by
    /// path, and only the FIRST row per pair is returned — an ambiguous qualifier
    /// must never fan out into several edges.
    ///
    /// # Errors
    ///
    /// Propagates any `rusqlite` failure from statement preparation or query.
    pub fn find_producer_modules_for_qualified(
        &self,
        pairs: &[(String, String)],
        consumer_hint: Option<&str>,
    ) -> Result<Vec<(String, String)>, rusqlite::Error> {
        if pairs.is_empty() {
            return Ok(Vec::new());
        }
        let kind_list = CALLABLE_KINDS
            .iter()
            .map(|k| format!("'{k}'"))
            .collect::<Vec<_>>()
            .join(",");
        let ext_pred = wireable_ext_sql("module_file", self.polyglot());
        let family_pred = same_family_sql(consumer_hint);
        let crate_prefix = consumer_hint
            .and_then(|f| {
                let mut it = f.split('/');
                match (it.next(), it.next()) {
                    (Some("crates"), Some(b)) => Some(format!("crates/{b}/%")),
                    _ => None,
                }
            })
            .unwrap_or_default();
        let sql = format!(
            "SELECT module_file, symbol_name FROM wiring_map
             WHERE consumer_file IS NULL
               AND visibility = 'public'
               AND symbol_kind IN ({kind_list})
               AND {ext_pred}{family_pred}
               AND symbol_name = ?1
               AND (module_file LIKE '%/' || ?2 || '.rs'
                 OR module_file LIKE '%/' || ?2 || '/mod.rs'
                 OR module_file = ?2 || '.rs')
             ORDER BY (CASE WHEN '{crate_prefix}' != '' AND module_file LIKE '{crate_prefix}'
                            THEN 0 ELSE 1 END),
                      module_file
             LIMIT 1"
        );
        let mut stmt = self.conn_ref().prepare(&sql)?;
        let mut out = Vec::new();
        for (qualifier, name) in pairs {
            let row = stmt
                .query_map(rusqlite::params![name, qualifier], |r| {
                    Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
                })?
                .filter_map(Result::ok)
                .next();
            if let Some(pair) = row
                && !out.contains(&pair)
            {
                out.push(pair);
            }
        }
        Ok(out)
    }

    /// Callable producers matched by BARE name, capped at `cap_per_name` each.
    ///
    /// The fallback path for calls that carry no qualifier. It cannot be exact:
    /// `run` matches 130 producers in this workspace, so the cap decides which
    /// four get the edge. Prefer
    /// [`Self::find_producer_modules_for_qualified`] whenever the call names
    /// its module.
    ///
    /// # Errors
    ///
    /// Propagates any `rusqlite` failure from the underlying query.
    pub fn find_producer_modules_for_methods(
        &self,
        names: &[String],
        cap_per_name: usize,
        consumer_hint: Option<&str>,
    ) -> Result<Vec<(String, String)>, rusqlite::Error> {
        self.find_producer_modules_with_kinds(names, cap_per_name, CALLABLE_KINDS, consumer_hint)
    }

    /// Type/const producers (`struct`/`enum`/`const`/`static`/`type_alias`/
    /// `trait`) — the lookup the G3 type/const-ref pass needs (2026-08-12):
    /// a symbol used AS a type or const never appears in a call expression,
    /// so the callable-only lookup above structurally cannot wire its
    /// consumers and every such producer read as a false orphan.
    pub fn find_producer_modules_for_types(
        &self,
        names: &[String],
        cap_per_name: usize,
        consumer_hint: Option<&str>,
    ) -> Result<Vec<(String, String)>, rusqlite::Error> {
        self.find_producer_modules_with_kinds(names, cap_per_name, TYPE_KINDS, consumer_hint)
    }

    /// W4 (2026-09-02) — the ONE inference step both the rebuild and the hook
    /// path run: match the call names and type/const refs a consumer file uses
    /// against the producer rows, cap 4 per name, record with
    /// [`WiringOrigin::AstInferred`]. Returns the number of edges recorded.
    ///
    /// Measured before: `index rebuild` inferred (68.641 edges) while
    /// `update_wiring_after_edit` only re-recorded `use` imports after
    /// `clear_consumer_entries` — so every file EDITED after a rebuild lost its
    /// inferred edges until the next rebuild, and its callees read as orphans
    /// (`limits.rs::apply_resource_caps_to` right after `sandbox_executor.rs`
    /// was edited). Two call sites deriving the same edge differently is the
    /// C08 asymmetry; one method is what removes it.
    pub fn record_inferred_consumers(
        &self,
        consumer_file: &str,
        method_names: &[String],
        type_refs: &[String],
        qualified_calls: &[(String, String)],
    ) -> usize {
        let mut recorded = 0usize;
        // D2 (2026-09-02) — the qualified pass runs FIRST and is exact: one
        // producer per (module, name) pair. The by-name passes below stay as the
        // fallback for calls that carry no qualifier.
        if let Ok(producers) =
            self.find_producer_modules_for_qualified(qualified_calls, Some(consumer_file))
        {
            for (module_file, symbol_name) in &producers {
                if self
                    .record_consumer_with_origin(
                        module_file,
                        symbol_name,
                        consumer_file,
                        None,
                        WiringOrigin::AstInferred,
                    )
                    .is_ok()
                {
                    recorded += 1;
                }
            }
        }
        for (names, callable) in [(method_names, true), (type_refs, false)] {
            if names.is_empty() {
                continue;
            }
            let producers = if !callable {
                self.find_producer_modules_for_types(names, 4, Some(consumer_file))
            } else if language_family(consumer_file) == Some("python") {
                self.find_python_method_producers(names, consumer_file)
            } else {
                self.find_producer_modules_for_methods(names, 4, Some(consumer_file))
            };
            let Ok(producers) = producers else {
                continue;
            };
            for (module_file, symbol_name) in &producers {
                if self
                    .record_consumer_with_origin(
                        module_file,
                        symbol_name,
                        consumer_file,
                        None,
                        WiringOrigin::AstInferred,
                    )
                    .is_ok()
                {
                    recorded += 1;
                }
            }
        }
        recorded
    }

    /// Python methods reached as `obj.metodo()` / `obj.atributo` (18/09/2026,
    /// analise: `tags_cli` and `todas_as_arestas` read as orphans).
    ///
    /// The syntax names the method, never its class, so the guess is held by
    /// two gates: only `method` producers, and only in a module the consumer
    /// already has an edge to (an import, an `alias.Nome` use). A method edge
    /// only ever lands in such a module, so one guess never opens the gate for
    /// another. The consumer's own module is out: its uses of its own methods
    /// belong to the self-reference pass.
    pub fn find_python_method_producers(
        &self,
        names: &[String],
        consumer_file: &str,
    ) -> Result<Vec<(String, String)>, rusqlite::Error> {
        if names.is_empty() {
            return Ok(Vec::new());
        }
        let placeholders = (0..names.len())
            .map(|i| format!("?{}", i + 2))
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "SELECT DISTINCT p.module_file, p.symbol_name FROM wiring_map p
             WHERE p.consumer_file IS NULL
               AND p.visibility = 'public'
               AND p.symbol_kind = 'method'
               AND p.module_file != ?1
               AND p.symbol_name IN ({placeholders})
               AND p.module_file IN (
                   SELECT i.module_file FROM wiring_map i
                   WHERE i.consumer_file = ?1 AND i.module_file != ?1)
             ORDER BY p.module_file, p.symbol_name"
        );
        let mut params: Vec<&dyn rusqlite::ToSql> = Vec::with_capacity(names.len() + 1);
        params.push(&consumer_file);
        params.extend(names.iter().map(|s| s as &dyn rusqlite::ToSql));
        let mut stmt = self.conn_ref().prepare(&sql)?;
        let rows = stmt
            .query_map(params.as_slice(), |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Shared producer lookup: public producer rows whose `symbol_kind` is in
    /// `kinds` and whose name is in `names`, capped per name.
    ///
    /// `consumer_hint` (the consuming file) ranks same-crate producers first
    /// before the per-name cap (2026-08-12, G3): bare-name matching otherwise
    /// wires a call to a same-named symbol in ANOTHER crate — the mechanism
    /// behind the homonym-fork false cycles (foundation↔resilience
    /// `meminfo.rs` linking to each other without any import) and behind
    /// generic names (`new`, `as_str`) losing their real producer to the cap.
    pub fn find_producer_modules_with_kinds(
        &self,
        names: &[String],
        cap_per_name: usize,
        kinds: &[&str],
        consumer_hint: Option<&str>,
    ) -> Result<Vec<(String, String)>, rusqlite::Error> {
        if names.is_empty() {
            return Ok(Vec::new());
        }
        // Build placeholders dynamically — there is no IN-list binding helper
        // in rusqlite for slices of arbitrary length.
        let placeholders: String = (0..names.len())
            .map(|i| format!("?{}", i + 1))
            .collect::<Vec<_>>()
            .join(",");
        let kind_list = kinds
            .iter()
            .map(|k| format!("'{k}'"))
            .collect::<Vec<_>>()
            .join(",");
        let ext_pred = wireable_ext_sql("module_file", self.polyglot());
        // A bare name is a guess, and a guess never crosses languages: a `.rs`
        // call to `cosine_similarity` is not a use of the Python function of
        // that name (18/09/2026, analise: 15.758 such edges).
        let family_pred = same_family_sql(consumer_hint);
        // Same-crate producers sort first: `crates/touring-x/…` prefix match.
        let crate_prefix = consumer_hint
            .and_then(|f| {
                let mut it = f.split('/');
                match (it.next(), it.next()) {
                    (Some("crates"), Some(b)) => Some(format!("crates/{b}/%")),
                    _ => None,
                }
            })
            .unwrap_or_default();
        let sql = format!(
            "SELECT DISTINCT module_file, symbol_name FROM wiring_map
             WHERE consumer_file IS NULL
               AND visibility = 'public'
               AND symbol_kind IN ({kind_list})
               AND {ext_pred}{family_pred}
               AND symbol_name IN ({placeholders})
             ORDER BY (CASE WHEN '{crate_prefix}' != '' AND module_file LIKE '{crate_prefix}'
                            THEN 0 ELSE 1 END),
                      module_file"
        );
        let mut stmt = self.conn_ref().prepare(&sql)?;
        let params_vec: Vec<&dyn rusqlite::ToSql> =
            names.iter().map(|s| s as &dyn rusqlite::ToSql).collect();
        let rows = stmt
            .query_map(params_vec.as_slice(), |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .filter_map(|r| r.ok())
            .collect::<Vec<_>>();
        if cap_per_name == 0 {
            return Ok(rows);
        }
        // Apply per-name cap on the client side to keep the SQL portable
        // (SQLite supports neither ROW_NUMBER OVER (PARTITION BY ...) nor
        // LIMIT inside an UNION ALL convenient enough to warrant the
        // extra complexity for the typical 0-200 row case).
        use std::collections::HashMap;
        let mut counts: HashMap<String, usize> = HashMap::new();
        let mut capped = Vec::with_capacity(rows.len().min(names.len() * cap_per_name));
        for (module_file, symbol_name) in rows {
            let entry = counts.entry(symbol_name.clone()).or_insert(0);
            if *entry < cap_per_name {
                *entry += 1;
                capped.push((module_file, symbol_name));
            }
        }
        Ok(capped)
    }

    /// Diagnostic snapshot of the wiring_map table — used by `touring doctor`.
    ///
    /// Returns a row census so operators can spot pollution (e.g., many
    /// `kind_unknown` rows indicate consumer entries inserted before their
    /// producer — race condition; many `non_wireable_rows` indicate a
    /// regression in the entry gate).
    ///
    /// Fields:
    /// - `total_rows`: every row in wiring_map (producer + consumer mixed).
    /// - `producer_rows`: rows with consumer_file IS NULL (one per pub symbol).
    /// - `consumer_rows`: rows with consumer_file IS NOT NULL (one per consumer edge).
    /// - `pub_producers`: producer rows with visibility='public'.
    /// - `distinct_pub_symbols`: deduped (module_file, symbol_name) producer count.
    /// - `kind_unknown_count`: rows with symbol_kind='unknown' (schema-degraded).
    /// - `non_wireable_rows`: rows inadmissible under the MAXIMUM vocabulary
    ///   (`polyglot = true`) — the only counter here that judges. Mode-
    ///   independent by design: a virtualenv or a `.json` is not wiring under
    ///   any read.
    /// - `unread_rows`: supported-language sources the CURRENT mode filters
    ///   out. Expected to be large in a Python project with the opt-in off,
    ///   and zero once `polyglot_wiring` is on — information, not a defect.
    ///
    /// The census fields above cover ONLY the rows the mode actually reads, so
    /// they describe the graph the answers come from. Classification calls
    /// [`is_wireable_source`], the writer's own vocabulary, so this diagnostic
    /// cannot disagree with the thing it diagnoses.
    pub fn wiring_db_diagnostic(&self) -> Result<WiringDbDiagnostic, rusqlite::Error> {
        // GROUP BY module_file, not a flat SUM: the classification is Rust
        // logic (this crate's own write gate), which SQL cannot express.
        // Grouping keeps the cost at O(distinct files) instead of O(rows), and
        // because the group key IS `module_file`, per-group DISTINCT counts sum
        // to the global DISTINCT without a second pass.
        let polyglot = self.polyglot();
        let mut stmt = self.conn_ref().prepare(
            "SELECT
                module_file,
                COUNT(*),
                SUM(CASE WHEN consumer_file IS NULL THEN 1 ELSE 0 END),
                SUM(CASE WHEN consumer_file IS NOT NULL THEN 1 ELSE 0 END),
                SUM(CASE WHEN consumer_file IS NULL AND visibility = 'public' THEN 1 ELSE 0 END),
                COUNT(DISTINCT CASE WHEN consumer_file IS NULL AND visibility = 'public'
                                    THEN symbol_name END),
                SUM(CASE WHEN symbol_kind = 'unknown' THEN 1 ELSE 0 END)
             FROM wiring_map
             GROUP BY module_file",
        )?;
        let mut row = WiringDbDiagnostic {
            total_rows: 0,
            producer_rows: 0,
            consumer_rows: 0,
            pub_producers: 0,
            distinct_pub_symbols: 0,
            kind_unknown_count: 0,
            non_wireable_rows: 0,
            unread_rows: 0,
            name_only_candidates: None,
            heuristic_edges: 0,
        };
        let groups = stmt.query_map([], |r| {
            Ok((
                r.get::<_, Option<String>>(0)?.unwrap_or_default(),
                r.get::<_, i64>(1).unwrap_or(0),
                r.get::<_, Option<i64>>(2)?.unwrap_or(0),
                r.get::<_, Option<i64>>(3)?.unwrap_or(0),
                r.get::<_, Option<i64>>(4)?.unwrap_or(0),
                r.get::<_, i64>(5).unwrap_or(0),
                r.get::<_, Option<i64>>(6)?.unwrap_or(0),
            ))
        })?;
        for group in groups {
            let (module_file, n, prod, cons, pubp, distinct_pub, unknown) = group?;
            if !is_indexable_module_file_with(&module_file, true, self.source_packages()) {
                row.non_wireable_rows += n;
                continue;
            }
            if !is_indexable_module_file_with(&module_file, polyglot, self.source_packages()) {
                row.unread_rows += n;
                continue;
            }
            row.total_rows += n;
            row.producer_rows += prod;
            row.consumer_rows += cons;
            row.pub_producers += pubp;
            row.distinct_pub_symbols += distinct_pub;
            row.kind_unknown_count += unknown;
        }
        drop(stmt);
        // Queried apart from the aggregate above, and deliberately so: these two
        // are the honesty counters. Folding them into the same SUM would put
        // "edges we are sure of" and "guesses / failures" in one number, which
        // is the exact approximation S1 exists to remove.
        let name_only_candidates = self.name_only_candidates();
        let heuristic_edges = self
            .conn_ref()
            .query_row(
                "SELECT COUNT(*) FROM wiring_map WHERE contract_source = ?1",
                params![WiringOrigin::AstInferred.as_str()],
                |r| r.get::<_, i64>(0),
            )
            .unwrap_or(0);
        Ok(WiringDbDiagnostic {
            name_only_candidates,
            heuristic_edges,
            ..row
        })
    }

    /// One-shot migration that canonicalizes legacy absolute paths in
    /// wiring_map to root-relative form **and evicts rows that describe
    /// another project's files**. Idempotent: safe to run on every daemon
    /// startup. Returns the number of rows touched.
    ///
    /// Without the canonicalization, the same file recorded under both
    /// `crates/foo.rs` and `/home/.../crates/foo.rs` looks like two
    /// independent rows, causing every producer in the absolute form to
    /// appear orphan even when consumers under the relative form import it.
    ///
    /// The eviction closes a different hole. A wiring row whose file lives
    /// outside this database's root is not this project's code, and it is not
    /// harmless: it inflates the orphan count, and it manufactures dependency
    /// cycles between modules that never met — the 136-module false-positive
    /// SCC of 2026-06-02 was `analise` rows read through `konverter`'s graph.
    /// The mitigation then was to filter cycles by `workspace_root`, a column
    /// that is NULL on every row ever written, so it filtered nothing. Rows
    /// from a foreign project are removed at the source instead. Measured on
    /// the two projects that carried them: 4.306 rows in `analise`, 7.893 in
    /// `konverter`, and **zero** absolute rows under their own root.
    ///
    /// Runs only when a root could be derived from the DB's own location. With
    /// no root there is nothing to strip and, more importantly, nothing that
    /// could tell a foreign row from a local one — so it does nothing at all
    /// rather than guess with a deletion.
    pub fn migrate_canonicalize_paths(&self) -> Result<u64, rusqlite::Error> {
        // W (2026-09-02): merge the legacy `./`-prefixed universe first. A `./`
        // prefix has exactly one reading, so it needs no root to be safe;
        // measured: 12.032 producer rows + 115.986 consumer edges under it,
        // invisible to the plain-keyed producers the rebuild writes.
        let has_unresolved: bool = self
            .conn_ref()
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'wiring_unresolved'",
                [],
                |r| r.get::<_, i64>(0),
            )
            .map(|n| n > 0)
            .unwrap_or(false);
        let mut dot_touched: u64 = 0;
        let mut dot_targets = vec![
            ("wiring_map", "module_file"),
            ("wiring_map", "consumer_file"),
        ];
        if has_unresolved {
            dot_targets.push(("wiring_unresolved", "consumer_file"));
        }
        for (table, col) in &dot_targets {
            dot_touched += self.conn_ref().execute(
                &format!(
                    "UPDATE OR IGNORE {table} SET {col} = SUBSTR({col}, 3) WHERE {col} LIKE './%'"
                ),
                [],
            )? as u64;
        }
        // Collisions (OR IGNORE): the plain row already exists — drop the dot one.
        dot_touched += self.conn_ref().execute(
            "DELETE FROM wiring_map WHERE module_file LIKE './%' OR consumer_file LIKE './%'",
            [],
        )? as u64;
        if has_unresolved {
            dot_touched += self.conn_ref().execute(
                "DELETE FROM wiring_unresolved WHERE consumer_file LIKE './%'",
                [],
            )? as u64;
        }
        let Some(root) = self.workspace_root_ref().map(str::to_owned) else {
            return Ok(dot_touched);
        };
        let updated_modules = self.conn_ref().execute(
            "UPDATE OR IGNORE wiring_map
             SET module_file = SUBSTR(module_file, LENGTH(?1) + 1)
             WHERE module_file LIKE ?1 || '%'",
            params![&root],
        )?;
        let updated_consumers = self.conn_ref().execute(
            "UPDATE OR IGNORE wiring_map
             SET consumer_file = SUBSTR(consumer_file, LENGTH(?1) + 1)
             WHERE consumer_file LIKE ?1 || '%'",
            params![&root],
        )?;
        // Collisions (OR IGNORE above) — delete rows that could not be merged.
        let deleted = self.conn_ref().execute(
            "DELETE FROM wiring_map WHERE module_file LIKE ?1 || '%'",
            params![&root],
        )?;
        // Foreign rows: an absolute path that is NOT under this root belongs to
        // some other project. Only absolute paths are judged — a relative path
        // is already root-relative by construction and has no other reading.
        let foreign = self.conn_ref().execute(
            "DELETE FROM wiring_map
             WHERE (module_file LIKE '/%' AND module_file NOT LIKE ?1 || '%')
                OR (consumer_file LIKE '/%' AND consumer_file NOT LIKE ?1 || '%')",
            params![&root],
        )?;
        let touched =
            updated_modules + updated_consumers + deleted + foreign + dot_touched as usize;
        if touched > 0 {
            Self::invalidate_wiring_modules_cache();
        }
        Ok(touched as u64)
    }

    /// Evict rows the write gate would refuse today.
    ///
    /// The read filters admit by EXTENSION; the write gate admits by extension
    /// **and path** (vendored trees, `docs/`, `scripts/`, tests, build output).
    /// The reader therefore disagrees with the writer, and any row written
    /// before a gate rule existed stays readable forever. Turning the polyglot
    /// opt-in on for `analise` made the gap unmissable: readable producers went
    /// from 8.224 to 184.343, of which **72,4% were a virtualenv**
    /// (`/site-packages/`), 13,3% tests and 4,9% docs — 4,9% first-party
    /// source. An orphan report of 142.689 entries answers no question anyone
    /// has.
    ///
    /// Running it on every open makes the gate an INVARIANT over the data
    /// rather than a rule applied at one moment: a row written by an older
    /// binary is removed at the next open, and the read filters stay a cheap
    /// extension check instead of carrying twenty `NOT LIKE` clauses per query.
    ///
    /// Judged under the MAXIMAL vocabulary (`polyglot = true`), never the
    /// project's current mode: a project that flips the opt-in later must find
    /// its Python still there. Only what NO mode could ever read is removed.
    ///
    /// Returns the number of rows removed.
    pub fn migrate_evict_ungated_rows(&self) -> Result<u64, rusqlite::Error> {
        // BOTH columns. A path that appears only as a consumer is still a
        // path the gate refuses, and reading only `module_file` let a
        // virtualenv file keep an edge into first-party code — caught by
        // `a_bogus_consumer_takes_only_its_own_edge`, which failed against the
        // first version of this query.
        // The two sides are judged by DIFFERENT rules, because the write gate
        // judges only the producer (`record_consumer_with_origin` checks
        // `canonical_module`, never `canonical_consumer`).
        //
        // Producer: the gate's own predicate — a row it would refuse to write.
        // Consumer: only third-party or generated trees. A consumer inside
        // `tests/` is the project using its own code, which is what the graph
        // exists to record; evicting on the full predicate deleted 13.621 such
        // edges from this workspace, and the orphan count moving from 4.232 to
        // 2.498 is what gave it away.
        let refused_producers: Vec<String> = {
            let mut stmt = self
                .conn_ref()
                .prepare("SELECT DISTINCT module_file FROM wiring_map")?;
            let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
            rows.filter_map(Result::ok)
                .filter(|f| !is_indexable_module_file_with(f, true, self.source_packages()))
                .collect()
        };
        let refused_consumers: Vec<String> = {
            let mut stmt = self.conn_ref().prepare(
                "SELECT DISTINCT consumer_file FROM wiring_map WHERE consumer_file IS NOT NULL",
            )?;
            let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
            // A companion consumer too (decision 1-A): the write gate refuses it,
            // and a row written before the gate — or copied in by the legacy
            // consolidation, which bypasses it — is removed here (B11).
            rows.filter_map(Result::ok)
                .filter(|f| {
                    is_vendored_or_generated(f) || touring_foundation::config::is_companion_key(f)
                })
                .collect()
        };
        let files: Vec<String> = refused_producers;
        if files.is_empty() && refused_consumers.is_empty() {
            return Ok(0);
        }
        // A temp table instead of an `IN (?,?,…)` list: the refused set is
        // thousands of paths on a real project, well past SQLite's parameter
        // ceiling, and batching would make the DELETE non-atomic.
        self.conn_ref().execute_batch(
            "CREATE TEMP TABLE IF NOT EXISTS _ungated(f TEXT PRIMARY KEY);
             DELETE FROM _ungated;",
        )?;
        self.conn_ref().execute_batch(
            "CREATE TEMP TABLE IF NOT EXISTS _ungated_c(f TEXT PRIMARY KEY);
             DELETE FROM _ungated_c;",
        )?;
        {
            let mut ins = self
                .conn_ref()
                .prepare("INSERT OR IGNORE INTO _ungated(f) VALUES (?1)")?;
            for f in &files {
                ins.execute(params![f])?;
            }
            let mut ins_c = self
                .conn_ref()
                .prepare("INSERT OR IGNORE INTO _ungated_c(f) VALUES (?1)")?;
            for f in &refused_consumers {
                ins_c.execute(params![f])?;
            }
        }
        let removed = self.conn_ref().execute(
            "DELETE FROM wiring_map
             WHERE module_file IN (SELECT f FROM _ungated)
                OR consumer_file IN (SELECT f FROM _ungated_c)",
            [],
        )?;
        self.conn_ref()
            .execute_batch("DROP TABLE IF EXISTS _ungated; DROP TABLE IF EXISTS _ungated_c;")?;
        if removed > 0 {
            Self::invalidate_wiring_modules_cache();
        }
        Ok(removed as u64)
    }

    /// Get all orphan symbols (pub symbols with no consumer anywhere).
    ///
    /// A symbol is orphaned if it has a NULL consumer entry and NO non-NULL
    /// consumer entries exist for the same (module_file, symbol_name).
    ///
    /// SQL filter rules (matches `is_indexable_module_file`):
    /// - only `.rs` files (no Python/Markdown/JSON pollution)
    /// - reject benches/tests in any position (leading or nested)
    /// - reject docs/ and scripts/ subtrees (non-source paths)
    /// - **F7 (2026-05-11)**: reject `symbol_kind = 'module'` — `pub mod foo;`
    ///   declarations are Rust's internal namespace structure, not API
    ///   consumable via `use crate::foo` (you cannot "import a module"; you
    ///   reach into it for its members). They produced 747 false-positive
    ///   orphans (19% of post-F9 count) with no actionable interpretation.
    /// - **F8 (2026-05-11)**: reject conventional trait-derive method names
    ///   (`fmt`, `hash`, `eq`, `partial_cmp`, `cmp`, `drop`, `clone`,
    ///   `default`). These are emitted by `#[derive(...)]` macros and the
    ///   tree-sitter call walker (F9) never sees a literal call site for
    ///   them — derived implementations are invoked by the standard library
    ///   (Hash::hash by HashMap, Display::fmt by `{}` formatter, Drop::drop
    ///   by the compiler). Listing them as orphans is operationally
    ///   misleading: they are *guaranteed* to be wired.
    ///
    /// These filters are belt-and-suspenders: the write-side gate at
    /// `register_pub_symbol` blocks ineligible rows from entering, and
    /// this SQL guard masks any legacy rows that pre-date the gate.
    pub fn orphan_symbols(&self) -> Result<Vec<WiringEntry>, rusqlite::Error> {
        self.orphan_symbols_with_trust(false)
    }

    /// Public symbols whose only consumers live in the file that declares them.
    ///
    /// B4 (15/09/2026): a `X -> X` row satisfies the `NOT EXISTS` of
    /// [`Self::orphan_symbols_with_trust`], so "used only by its own file" and
    /// "used across the project" read identically — 6.191 such rows are live in
    /// this workspace. This class stays **out** of the orphan list (the symbol is
    /// consumed, and the judge's baseline does not move) and is reported beside
    /// it: a public symbol nothing outside its file uses is either internal —
    /// make it private — or public API waiting for its first consumer.
    ///
    /// # Errors
    ///
    /// Propagates any `rusqlite` failure.
    pub fn internal_only_symbols(&self) -> Result<Vec<WiringEntry>, rusqlite::Error> {
        let ext_pred = wireable_ext_sql("w.module_file", self.polyglot());
        let non_source = non_source_sql("w.module_file", self.source_packages());
        let producer_rows = public_producer_rows_sql(&ext_pred, &non_source);
        let sql = format!(
            "{producer_rows}
               AND EXISTS (
                   SELECT 1 FROM wiring_map w2
                   WHERE w2.module_file = w.module_file
                     AND w2.symbol_name = w.symbol_name
                     AND w2.consumer_file IS NOT NULL
               )
               AND NOT EXISTS (
                   SELECT 1 FROM wiring_map w3
                   WHERE w3.module_file = w.module_file
                     AND w3.symbol_name = w.symbol_name
                     AND w3.consumer_file IS NOT NULL
                     AND w3.consumer_file != w.module_file
               )
             ORDER BY w.module_file, w.symbol_name"
        );
        let mut stmt = self.conn_ref().prepare(&sql)?;
        let entries = stmt
            .query_map([], row_to_wiring_entry)?
            .filter_map(|r| r.ok())
            .collect();
        Ok(entries)
    }

    /// Orphan symbols with an optional trust filter (H2, 2026-08-12).
    ///
    /// `trusted = true` counts a symbol as consumed only by a NON-heuristic
    /// edge (`contract_source != 'ast_inferred'`): a symbol whose only
    /// consumers come from bare-name matching reports as orphan in this mode,
    /// which is the honest answer to "does anything provably use this?".
    pub fn orphan_symbols_with_trust(
        &self,
        trusted: bool,
    ) -> Result<Vec<WiringEntry>, rusqlite::Error> {
        let ext_pred = wireable_ext_sql("w.module_file", self.polyglot());
        let non_source = non_source_sql("w.module_file", self.source_packages());
        let trust_pred = if trusted {
            "AND w2.contract_source != 'ast_inferred'"
        } else {
            ""
        };
        let producer_rows = public_producer_rows_sql(&ext_pred, &non_source);
        let sql = format!(
            "{producer_rows}
               AND NOT EXISTS (
                   SELECT 1 FROM wiring_map w2
                   WHERE w2.module_file = w.module_file
                     AND w2.symbol_name = w.symbol_name
                     AND w2.consumer_file IS NOT NULL
                     {trust_pred}
               )
             ORDER BY w.module_file, w.symbol_name"
        );
        let mut stmt = self.conn_ref().prepare(&sql)?;
        let entries = stmt
            .query_map([], row_to_wiring_entry)?
            .filter_map(|r| r.ok())
            .collect();
        Ok(entries)
    }

    /// Get integration score for a module.
    ///
    /// Score = symbols_with_at_least_one_consumer / total_pub_symbols.
    /// Returns 1.0 if the module has no pub symbols (nothing to wire).
    ///
    /// A symbol is considered "with consumer" if there exists at least one
    /// wiring_map entry where consumer_file IS NOT NULL for that symbol.
    pub fn integration_score(&self, module_file: &str) -> Result<f64, rusqlite::Error> {
        // Total distinct pub symbols for this module
        let total_all: i64 = self.conn_ref().query_row(
            "SELECT COUNT(DISTINCT symbol_name) FROM wiring_map
             WHERE module_file = ?1 AND visibility = 'public'",
            params![module_file],
            |r| r.get(0),
        )?;
        if total_all == 0 {
            return Ok(1.0);
        }
        // Symbols that have at least one consumer entry
        let with_consumer: i64 = self.conn_ref().query_row(
            "SELECT COUNT(DISTINCT symbol_name) FROM wiring_map
             WHERE module_file = ?1 AND visibility = 'public' AND consumer_file IS NOT NULL",
            params![module_file],
            |r| r.get(0),
        )?;
        Ok(with_consumer as f64 / total_all as f64)
    }

    /// Get orphan symbols for a specific module (parameterized query).
    ///
    /// More efficient than `orphan_symbols()` + client-side filter:
    /// uses a WHERE clause on module_file directly in SQL.
    ///
    /// Eligibility filter mirrors `orphan_symbols()` to keep per-module and
    /// aggregate views consistent: `.rs` only, no benches/tests/docs/scripts,
    /// no `pub mod` declarations (F7), no derive-trait method names (F8).
    pub fn orphan_symbols_for_module(
        &self,
        module_file: &str,
    ) -> Result<Vec<WiringEntry>, rusqlite::Error> {
        let canonical = self.canonicalize_module_path(module_file);
        let ext_pred = wireable_ext_sql("w.module_file", self.polyglot());
        let non_source = non_source_sql("w.module_file", self.source_packages());
        let sql = format!(
            "SELECT w.module_file, w.symbol_name, w.symbol_kind, w.visibility,
                    w.consumer_file, w.import_line, w.contract_source
             FROM wiring_map w
             WHERE w.module_file = ?1 AND w.consumer_file IS NULL AND w.visibility = 'public'
               AND {ext_pred}
               AND w.module_file NOT LIKE 'benches/%'
               AND w.module_file NOT LIKE 'tests/%'
               {non_source}
               AND w.module_file NOT LIKE '%/benches/%'
               AND w.module_file NOT LIKE '%/tests/%'
               AND w.symbol_kind != 'module'
               AND w.symbol_name NOT IN ('fmt','hash','eq','partial_cmp','cmp','drop','clone','default')
               AND NOT EXISTS (
                   SELECT 1 FROM wiring_map w2
                   WHERE w2.module_file = w.module_file
                     AND w2.symbol_name = w.symbol_name
                     AND w2.consumer_file IS NOT NULL
               )
             ORDER BY w.symbol_name"
        );
        let mut stmt = self.conn_ref().prepare(&sql)?;
        let entries = stmt
            .query_map(params![canonical.as_ref()], row_to_wiring_entry)?
            .filter_map(|r| r.ok())
            .collect();
        Ok(entries)
    }

    /// Producer rows for exactly these `names` — the per-name form of
    /// [`Self::all_pub_symbols`] (S3, 2026-09-02).
    ///
    /// Returns `(symbol_name, module_file)` pairs, same-crate producers first
    /// when `consumer_hint` (the file about to import) is under `crates/<x>/`.
    /// Unlike [`Self::find_producer_modules_for_types`] it is NOT limited to
    /// type kinds and applies no cap: the caller already narrowed `names` to
    /// the handful of unresolved identifiers in one file, so a full-table scan
    /// per hook call (what `all_pub_symbols` costs on a 190k-row map) is
    /// replaced by one indexed `IN` lookup.
    ///
    pub fn find_pub_symbols_by_name(
        &self,
        names: &[String],
        consumer_hint: Option<&str>,
    ) -> Result<Vec<(String, String)>, rusqlite::Error> {
        if names.is_empty() {
            return Ok(Vec::new());
        }
        let placeholders: String = (0..names.len())
            .map(|i| format!("?{}", i + 1))
            .collect::<Vec<_>>()
            .join(",");
        // Same-crate producers sort first: `crates/touring-x/…` prefix match
        // (the ranking `find_producer_modules_with_kinds` applies).
        let crate_prefix = consumer_hint
            .and_then(|f| {
                let mut it = f.split('/');
                match (it.next(), it.next()) {
                    (Some("crates"), Some(b)) => Some(format!("crates/{b}/%")),
                    _ => None,
                }
            })
            .unwrap_or_default();
        // Same predicate as `all_pub_symbols` (producer rows = NULL consumer,
        // public) so recall is identical — only the `IN` list is new.
        let sql = format!(
            "SELECT DISTINCT symbol_name, module_file FROM wiring_map
             WHERE visibility = 'public'
               AND consumer_file IS NULL
               AND symbol_name IN ({placeholders})
             ORDER BY (CASE WHEN '{crate_prefix}' != '' AND module_file LIKE '{crate_prefix}'
                            THEN 0 ELSE 1 END),
                      module_file, symbol_name"
        );
        let mut stmt = self.conn_ref().prepare(&sql)?;
        let params_vec: Vec<&dyn rusqlite::ToSql> =
            names.iter().map(|s| s as &dyn rusqlite::ToSql).collect();
        let rows = stmt
            .query_map(params_vec.as_slice(), |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(rows)
    }

    /// Get ALL pub symbols (not just orphans) for import suggestion.
    ///
    /// Returns every registered public symbol in the wiring_map (the NULL-consumer
    /// "producer" rows). Used by Signal 6b to suggest imports for symbols that
    /// may already have consumers elsewhere but are still valid import targets.
    pub fn all_pub_symbols(&self) -> Result<Vec<WiringEntry>, rusqlite::Error> {
        let mut stmt = self.conn_ref().prepare(
            "SELECT DISTINCT w.module_file, w.symbol_name, w.symbol_kind, w.visibility,
                    NULL, NULL, w.contract_source
             FROM wiring_map w
             WHERE w.visibility = 'public' AND w.consumer_file IS NULL
             ORDER BY w.module_file, w.symbol_name",
        )?;
        let entries = stmt
            .query_map([], row_to_wiring_entry)?
            .filter_map(|r| r.ok())
            .collect();
        Ok(entries)
    }

    /// Get wiring status summary for a module.
    pub fn module_wiring_status(
        &self,
        module_file: &str,
    ) -> Result<ModuleWiringStatus, rusqlite::Error> {
        let score = self.integration_score(module_file)?;
        let orphans = self
            .orphan_symbols_for_module(module_file)?
            .into_iter()
            .map(|e| e.symbol_name)
            .collect::<Vec<_>>();
        let total: i64 = self.conn_ref().query_row(
            "SELECT COUNT(DISTINCT symbol_name) FROM wiring_map WHERE module_file = ?1 AND visibility = 'public'",
            params![module_file],
            |r| r.get(0),
        )?;
        Ok(ModuleWiringStatus {
            file_path: module_file.to_string(),
            total_pub_symbols: total as usize,
            symbols_with_consumers: total as usize - orphans.len(),
            integration_score: score,
            orphan_symbols: orphans,
        })
    }

    /// Single-pass aggregate across all modules — O(1) SQL instead of O(N*3).
    ///
    /// Wave 22 (S-Q1a): replaces the old `cli_wiring_modules` pattern that issued
    /// 3 queries per module (integration_score + orphan_symbols_for_module + COUNT).
    /// A single GROUP BY query returns totals for every module at once.
    ///
    /// Callers that need per-module orphan lists for modules with `wired_count < total_pub`
    /// can issue a targeted `orphan_symbols_for_module` call for just those modules.
    pub fn wiring_modules_aggregate(
        &self,
    ) -> Result<Vec<WiringModuleAggregateRow>, rusqlite::Error> {
        // Wave 22 FASE 6 P1 fix: explicit `visibility = 'public'` inside the CASE
        // clause mirrors the semantic of legacy `integration_score()` (which filters
        // both `total_all` and `with_consumer` by visibility). Eliminates potential
        // drift if a consumer row were ever inserted with non-public visibility.
        let mut stmt = self.conn_ref().prepare(
            "SELECT module_file,
                    COUNT(DISTINCT symbol_name) AS total_pub,
                    COUNT(DISTINCT CASE WHEN consumer_file IS NOT NULL AND visibility = 'public' THEN symbol_name END) AS wired_count
             FROM wiring_map
             WHERE visibility = 'public'
             GROUP BY module_file
             ORDER BY module_file",
        )?;
        let rows = stmt
            .query_map([], |row| {
                Ok(WiringModuleAggregateRow {
                    module_file: row.get::<_, String>(0)?,
                    total_pub: row.get::<_, i64>(1)?,
                    wired_count: row.get::<_, i64>(2)?,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(rows)
    }

    /// Invalidate the query cache entries for `cli_wiring_modules`.
    ///
    /// Called by mutators (`register_pub_symbol`, `record_consumer`, `clear_wiring`)
    /// so the next `cli_wiring_modules` call gets fresh data. The store does not
    /// know its project root, so every project's entry goes: a miss, never a
    /// stale answer.
    pub fn invalidate_wiring_modules_cache() {
        touring_foundation::query_cache::invalidate_kind("cli_wiring_modules");
    }

    /// Remove all wiring entries for a module (used when module is re-scanned).
    /// Clear ONLY the producer-side rows for this module (`consumer_file IS NULL`).
    ///
    /// Previously cleared ALL rows for `module_file` — which erased consumer
    /// evidence recorded by other files before they themselves were re-scanned.
    /// This caused a race: when `hook_registry.rs` was processed before
    /// `lifecycle.rs` in a backfill pass, hook_registry's `crate::lifecycle::X`
    /// consumer edges were destroyed as soon as lifecycle.rs was re-indexed.
    ///
    /// Producer rows (pub symbol declarations) are identified by
    /// `consumer_file IS NULL`. Consumer edges survive across producer
    /// re-scans until their own consumer file is re-indexed (which calls
    /// `clear_consumer_entries`).
    pub fn clear_wiring(&self, module_file: &str) -> Result<(), rusqlite::Error> {
        self.conn_ref().execute(
            "DELETE FROM wiring_map WHERE module_file = ?1 AND consumer_file IS NULL",
            params![self.canonicalize_module_path(module_file).as_ref()],
        )?;
        // Wave 22 FASE 6 P0 fix: invalidate query cache so cli_wiring_modules
        // does not serve stale data. Doc at invalidate_wiring_modules_cache lists
        // this function as a caller, but the actual invocation was previously missing.
        Self::invalidate_wiring_modules_cache();
        Ok(())
    }

    /// Every distinct `module_file` currently keyed in `wiring_map`.
    ///
    /// The rebuild's stale sweep is driven by the `symbols` table, which cannot
    /// see a `module_file` that no symbol row ever carried — and a resolver that
    /// invents a path produces exactly that. Enumerating the wiring keys
    /// directly is the only way to find them.
    pub fn distinct_module_files(&self) -> Result<Vec<String>, rusqlite::Error> {
        let mut stmt = self
            .conn_ref()
            .prepare("SELECT DISTINCT module_file FROM wiring_map")?;
        let rows = stmt
            .query_map([], |row| row.get::<_, String>(0))?
            .filter_map(Result::ok)
            .collect();
        Ok(rows)
    }

    /// Delete **every** row keyed to `module_file` — producer rows and the
    /// consumer edges that resolved to it.
    ///
    /// Unlike [`Self::clear_wiring`] (producer rows only, for a re-scan), this
    /// retires a module wholesale. Used by the rebuild sweep for paths that do
    /// not exist on disk: measured 2026-08-07, 301 of 1711 distinct
    /// `module_file` values (17.6%) pointed at absent files — partly renamed or
    /// merged crates (`touring-ast`, `touring-antt`), partly paths a pre-2026
    /// resolver fabricated from facade imports. A consumer edge parked on such a
    /// key is worse than a missing edge: the real producer stays
    /// `consumer_file IS NULL` and is reported orphan while its consumers are on
    /// record — against a file that is not there.
    pub fn purge_module_rows(&self, module_file: &str) -> Result<usize, rusqlite::Error> {
        let n = self.conn_ref().execute(
            "DELETE FROM wiring_map WHERE module_file = ?1",
            params![self.canonicalize_module_path(module_file).as_ref()],
        )?;
        Self::invalidate_wiring_modules_cache();
        Ok(n)
    }

    /// Every distinct consumer file the map holds. The rebuild's sweep judges
    /// each one by the walker's policy: its per-file sweep starts from files
    /// holding symbols, so a consumer with none was never looked at (cross-audit
    /// 14/09/2026, R2-6: 342 edges from May whose consumers were gone from disk
    /// kept 72 producers wired).
    ///
    /// # Errors
    /// Propagates the SQLite error of the query.
    pub fn consumer_files(&self) -> Result<Vec<String>, rusqlite::Error> {
        let mut stmt = self.conn_ref().prepare(
            "SELECT DISTINCT consumer_file FROM wiring_map WHERE consumer_file IS NOT NULL",
        )?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        Ok(rows.filter_map(Result::ok).collect())
    }

    /// Remove consumer entries for a specific file (used when file is re-scanned).
    pub fn clear_consumer_entries(&self, consumer_file: &str) -> Result<(), rusqlite::Error> {
        self.conn_ref().execute(
            "DELETE FROM wiring_map WHERE consumer_file = ?1",
            params![self.canonicalize_module_path(consumer_file).as_ref()],
        )?;
        // Wave 22 FASE 6 P0 fix: invalidate query cache. If caller re-scans and
        // returns early before `record_consumer` runs, without this invalidation
        // the cache would serve stale wired-count data until 60s TTL.
        Self::invalidate_wiring_modules_cache();
        Ok(())
    }

    /// The rebuild's per-file clear (I16, 2026-09-13): every consumer row of
    /// `consumer_file` EXCEPT the [`WiringOrigin::AstInferred`] ones, which the
    /// walk cannot re-derive until every producer row exists and are therefore
    /// re-recorded only after the walk. Clearing them per file opened a window:
    /// `kill -9` three seconds into a rebuild left 91 inferred edges of the
    /// already re-walked files missing (measured on the isolated copy, per-file
    /// transactions already in place). The inferred rows are replaced by
    /// [`Self::clear_inferred_consumer_entries`] inside the post-walk transaction.
    ///
    /// # Errors
    ///
    /// Propagates any `rusqlite` failure.
    pub fn clear_declared_consumer_entries(
        &self,
        consumer_file: &str,
    ) -> Result<usize, rusqlite::Error> {
        let n = self.conn_ref().execute(
            "DELETE FROM wiring_map WHERE consumer_file = ?1 AND contract_source != ?2",
            params![
                self.canonicalize_module_path(consumer_file).as_ref(),
                WiringOrigin::AstInferred.as_str()
            ],
        )?;
        Self::invalidate_wiring_modules_cache();
        Ok(n)
    }

    /// The other half of [`Self::clear_declared_consumer_entries`]: only the
    /// inferred consumer rows of `consumer_file`, cleared right before they are
    /// re-derived, in the same transaction.
    ///
    /// # Errors
    ///
    /// Propagates any `rusqlite` failure.
    pub fn clear_inferred_consumer_entries(
        &self,
        consumer_file: &str,
    ) -> Result<usize, rusqlite::Error> {
        let n = self.conn_ref().execute(
            "DELETE FROM wiring_map WHERE consumer_file = ?1 AND contract_source = ?2",
            params![
                self.canonicalize_module_path(consumer_file).as_ref(),
                WiringOrigin::AstInferred.as_str()
            ],
        )?;
        Self::invalidate_wiring_modules_cache();
        Ok(n)
    }
}

#[cfg(test)]
mod polyglot_gate_tests {
    use super::{
        is_go_package_wireable, is_indexable_module_file_polyglot, is_non_rust_non_wireable,
        wireable_ext_sql, wireable_extensions,
    };

    // ── Default mode (flag OFF) — byte-identical Rust-only behavior ─────────

    #[test]
    fn default_mode_is_rust_only() {
        // The gate is now asked with the PROJECT's answer rather than reading a
        // process-global, so the default mode is expressed by passing `false`
        // — which is also what makes this assertion independent of whatever
        // `TOURING_POLYGLOT_WIRING` happens to hold in the runner's env.
        assert!(is_indexable_module_file_polyglot(
            "crates/a/src/foo.rs",
            false
        ));
        assert!(!is_indexable_module_file_polyglot("pkg/models.py", false));
        assert!(!is_indexable_module_file_polyglot("docs/plan.md", false));
    }

    #[test]
    fn off_policy_matches_legacy_rust_gate() {
        assert!(is_indexable_module_file_polyglot(
            "crates/a/src/foo.rs",
            false
        ));
        assert!(!is_indexable_module_file_polyglot(
            "benches/src/b.rs",
            false
        ));
        assert!(!is_indexable_module_file_polyglot(
            "crates/a/tests/it.rs",
            false
        ));
        // Non-Rust rejected regardless of path when polyglot is OFF.
        assert!(!is_indexable_module_file_polyglot("pkg/models.py", false));
        assert!(!is_indexable_module_file_polyglot("app/index.ts", false));
    }

    #[test]
    fn off_sql_predicate_is_byte_identical() {
        assert_eq!(
            wireable_ext_sql("w.module_file", false),
            "w.module_file LIKE '%.rs'"
        );
        assert_eq!(wireable_extensions(false), &[".rs"]);
    }

    // ── Polyglot mode (flag ON) — Python admitted, FP defenses hold ─────────

    #[test]
    fn on_policy_admits_first_party_source() {
        assert!(is_indexable_module_file_polyglot("pkg/models.py", true));
        assert!(is_indexable_module_file_polyglot(
            "src/app/service.py",
            true
        ));
        assert!(is_indexable_module_file_polyglot(
            "apps/web/src/models.ts",
            true
        ));
        assert!(is_indexable_module_file_polyglot(
            "apps/web/src/App.tsx",
            true
        ));
        assert!(is_indexable_module_file_polyglot("lib/util.js", true));
        assert!(is_indexable_module_file_polyglot("lib/util.jsx", true));
        assert!(is_indexable_module_file_polyglot(
            "src/main/java/com/foo/Bar.java",
            true
        ));
        assert!(is_indexable_module_file_polyglot(
            "crates/a/src/foo.rs",
            true
        ));
    }

    #[test]
    fn on_policy_defers_go() {
        // Go is extraction-ready but NOT wireable (a Go import denotes a package,
        // not a file), so `.go` files are not admitted — no false Go orphans.
        assert!(!is_indexable_module_file_polyglot("pkg/service.go", true));
        assert!(!is_indexable_module_file_polyglot("cmd/main.go", true));
    }

    #[test]
    fn on_policy_blocks_the_258_fp_sources() {
        // docs/*.py and scripts/*.py — the exact historical false positives the
        // .rs gate was created to block (2026-05-11 audit).
        assert!(!is_indexable_module_file_polyglot("docs/example.py", true));
        assert!(!is_indexable_module_file_polyglot("scripts/gen.py", true));
        assert!(!is_indexable_module_file_polyglot(
            "crates/x/scripts/tool.py",
            true
        ));
    }

    #[test]
    fn on_policy_blocks_vendored_and_test_files() {
        assert!(!is_indexable_module_file_polyglot(
            "apps/ai-service/venv/lib/python3.12/site-packages/sympy/core.py",
            true
        ));
        assert!(!is_indexable_module_file_polyglot(
            "app/node_modules/x/index.js",
            true
        ));
        assert!(!is_indexable_module_file_polyglot(
            "pkg/__pycache__/m.py",
            true
        ));
        assert!(!is_indexable_module_file_polyglot(".venv/lib/x.py", true));
        // pytest/unittest conventions.
        assert!(!is_indexable_module_file_polyglot(
            "pkg/test_models.py",
            true
        ));
        assert!(!is_indexable_module_file_polyglot(
            "pkg/models_test.py",
            true
        ));
        assert!(!is_indexable_module_file_polyglot("pkg/conftest.py", true));
        // tests/ subtree (shared with the Rust rule).
        assert!(!is_indexable_module_file_polyglot(
            "pkg/tests/test_x.py",
            true
        ));
        // JS/TS vendored / build output + test conventions.
        assert!(!is_indexable_module_file_polyglot(
            "web/dist/bundle.js",
            true
        ));
        assert!(!is_indexable_module_file_polyglot(
            "web/.next/page.js",
            true
        ));
        assert!(!is_indexable_module_file_polyglot(
            "web/src/foo.test.ts",
            true
        ));
        assert!(!is_indexable_module_file_polyglot(
            "web/src/foo.spec.tsx",
            true
        ));
        assert!(!is_indexable_module_file_polyglot(
            "web/src/__tests__/foo.ts",
            true
        ));
    }

    #[test]
    fn on_sql_predicate_includes_polyglot_extensions() {
        assert_eq!(
            wireable_ext_sql("w.module_file", true),
            "(w.module_file LIKE '%.rs' OR w.module_file LIKE '%.py' OR w.module_file LIKE '%.ts' OR w.module_file LIKE '%.tsx' OR w.module_file LIKE '%.js' OR w.module_file LIKE '%.jsx' OR w.module_file LIKE '%.mjs' OR w.module_file LIKE '%.cjs' OR w.module_file LIKE '%.java' OR w.module_file LIKE 'go:%')"
        );
        assert_eq!(
            wireable_extensions(true),
            &[
                ".rs", ".py", ".ts", ".tsx", ".js", ".jsx", ".mjs", ".cjs", ".java"
            ]
        );
    }

    #[test]
    fn non_rust_non_wireable_classifier() {
        // Python.
        assert!(is_non_rust_non_wireable("docs/x.py", &[]));
        assert!(is_non_rust_non_wireable("scripts/x.py", &[]));
        assert!(is_non_rust_non_wireable("a/site-packages/b.py", &[]));
        assert!(is_non_rust_non_wireable("pkg/test_x.py", &[]));
        // JS/TS.
        assert!(is_non_rust_non_wireable("web/node_modules/x.js", &[]));
        assert!(is_non_rust_non_wireable("web/dist/bundle.js", &[]));
        assert!(is_non_rust_non_wireable("web/foo.test.ts", &[]));
        assert!(is_non_rust_non_wireable("web/foo.spec.js", &[]));
        assert!(is_non_rust_non_wireable("web/__tests__/x.ts", &[]));
        // Java.
        assert!(is_non_rust_non_wireable(
            "app/src/test/java/com/FooTest.java",
            &[]
        ));
        assert!(is_non_rust_non_wireable("app/com/FooTests.java", &[]));
        // First-party source is wireable.
        assert!(!is_non_rust_non_wireable("pkg/models.py", &[]));
        assert!(!is_non_rust_non_wireable("web/src/service.ts", &[]));
        assert!(!is_non_rust_non_wireable(
            "src/main/java/com/foo/Bar.java",
            &[]
        ));
    }

    // ── P-G: Go package-aware key namespace ("go:<import-path>") ────────────

    #[test]
    fn go_package_key_admitted_under_flag_only() {
        // `go:<import-path>` is a synthetic package key (no file extension) —
        // admitted only when polyglot is ON, rejected when OFF.
        assert!(is_indexable_module_file_polyglot("go:mymod/pkg", true));
        assert!(is_indexable_module_file_polyglot(
            "go:mymod/internal/svc",
            true
        ));
        assert!(!is_indexable_module_file_polyglot("go:mymod/pkg", false));
    }

    #[test]
    fn go_file_keyed_stays_rejected_even_under_flag() {
        // File-keyed `.go` is NEVER wireable (the false-orphan class): a Go
        // import denotes a package, not a file. Only `go:` keys wire.
        assert!(!is_indexable_module_file_polyglot(
            "mymod/pkg/service.go",
            true
        ));
        assert!(!is_indexable_module_file_polyglot(
            "mymod/pkg/service.go",
            false
        ));
    }

    #[test]
    fn go_vendored_and_empty_packages_rejected() {
        assert!(!is_indexable_module_file_polyglot(
            "go:vendor/dep/pkg",
            true
        ));
        assert!(!is_indexable_module_file_polyglot(
            "go:mymod/vendor/dep",
            true
        ));
        assert!(
            !is_indexable_module_file_polyglot("go:", true),
            "empty import path"
        );
    }

    #[test]
    fn is_go_package_wireable_classifier() {
        assert!(is_go_package_wireable("go:mymod/pkg"));
        assert!(is_go_package_wireable("go:mymod/internal/svc")); // internal is real code
        assert!(!is_go_package_wireable("go:vendor/x"));
        assert!(!is_go_package_wireable("go:mymod/vendor/x"));
        assert!(!is_go_package_wireable("go:"));
    }

    #[test]
    fn on_sql_predicate_includes_go_keys() {
        // Read side must admit `go:%` under the flag so orphan detection sees
        // Go package producers; OFF stays byte-identical (`LIKE '%.rs'`).
        let on = wireable_ext_sql("module_file", true);
        assert!(
            on.contains("module_file LIKE 'go:%'"),
            "polyglot SQL must admit go: keys: {on}"
        );
        let off = wireable_ext_sql("module_file", false);
        assert_eq!(
            off, "module_file LIKE '%.rs'",
            "OFF byte-identical, no go: clause"
        );
    }
}

#[cfg(test)]
mod s3_find_pub_symbols_by_name_tests {
    use crate::knowledge::FileKnowledgeDB;
    use tempfile::TempDir;

    fn setup() -> (TempDir, FileKnowledgeDB) {
        let tmp = TempDir::new().expect("tempdir");
        let db = FileKnowledgeDB::new(&tmp.path().join("s3.db")).expect("open db");
        (tmp, db)
    }

    /// The per-name lookup must see a symbol that ALREADY has a consumer —
    /// the recall `all_pub_symbols` was introduced for (pre_edit "B4 fix").
    #[test]
    fn finds_wired_and_orphan_producers_by_exact_name_only() {
        let (_tmp, db) = setup();
        db.register_pub_symbol(
            "crates/a/src/tfidf.rs",
            "TfIdfVectorizer",
            "struct",
            "public",
        )
        .expect("register");
        db.record_consumer(
            "crates/a/src/tfidf.rs",
            "TfIdfVectorizer",
            "crates/a/src/other.rs",
            Some(3),
        )
        .expect("consumer");
        db.register_pub_symbol("crates/a/src/bm25.rs", "Bm25Scorer", "struct", "public")
            .expect("register");
        db.register_pub_symbol("crates/a/src/secret.rs", "Hidden", "struct", "private")
            .expect("register");

        let names = [
            "TfIdfVectorizer".to_string(),
            "Hidden".to_string(),
            "Nope".to_string(),
        ];
        let found = db.find_pub_symbols_by_name(&names, None).expect("query");
        assert_eq!(
            found,
            vec![(
                "TfIdfVectorizer".to_string(),
                "crates/a/src/tfidf.rs".to_string()
            )],
            "exact name, public only, wired symbol still visible"
        );
        assert!(
            db.find_pub_symbols_by_name(&[], None)
                .expect("query")
                .is_empty()
        );
    }

    /// `consumer_hint` ranks the same-crate producer first — the homonym in
    /// another crate is still returned, but never as the first suggestion.
    #[test]
    fn same_crate_producer_sorts_first_under_a_consumer_hint() {
        let (_tmp, db) = setup();
        db.register_pub_symbol("crates/zeta/src/cfg.rs", "Config", "struct", "public")
            .expect("register");
        db.register_pub_symbol("crates/alpha/src/cfg.rs", "Config", "struct", "public")
            .expect("register");
        let names = ["Config".to_string()];
        let hinted = db
            .find_pub_symbols_by_name(&names, Some("crates/zeta/src/main.rs"))
            .expect("query");
        assert_eq!(hinted.len(), 2);
        assert_eq!(hinted[0].1, "crates/zeta/src/cfg.rs", "{hinted:?}");
        let plain = db.find_pub_symbols_by_name(&names, None).expect("query");
        assert_eq!(
            plain[0].1, "crates/alpha/src/cfg.rs",
            "no hint → module_file order"
        );
    }
}

#[cfg(test)]
mod consumer_kind_resolution_tests {
    //! I16: the kind of a consumer row is a function of the producer table
    //! alone — never of walk order or of what the previous index held.
    use crate::knowledge::FileKnowledgeDB;
    use rusqlite::params;
    use tempfile::TempDir;

    fn setup() -> (TempDir, FileKnowledgeDB) {
        let tmp = TempDir::new().expect("tempdir");
        let db = FileKnowledgeDB::new(&tmp.path().join("i16.db")).expect("open db");
        (tmp, db)
    }

    fn kind_of(db: &FileKnowledgeDB, module: &str, name: &str, consumer: &str) -> String {
        db.conn_ref()
            .query_row(
                "SELECT symbol_kind FROM wiring_map
                 WHERE module_file = ?1 AND symbol_name = ?2 AND consumer_file = ?3",
                params![module, name, consumer],
                |r| r.get(0),
            )
            .expect("consumer row")
    }

    #[test]
    fn every_consumer_kind_is_rewritten_from_the_final_producers_and_the_pass_is_idempotent() {
        let (_tmp, db) = setup();
        // Producers as they stand AFTER the walk.
        db.register_pub_symbol(
            "crates/a/src/symbols.rs",
            "extract_symbols",
            "function",
            "public",
        )
        .expect("producer");
        db.register_pub_symbol("crates/b/src/poly.rs", "rewrite", "module", "public")
            .expect("producer");
        db.register_pub_symbol("crates/c/src/other.rs", "rewrite", "function", "public")
            .expect("producer");
        // Consumers: one resolves to its own module's producer, one to a homonym
        // (the module named by the import has no producer of that name), one
        // whose name no producer carries anywhere, and one glob import.
        db.record_consumer(
            "crates/a/src/symbols.rs",
            "extract_symbols",
            "crates/x/src/u.rs",
            Some(1),
        )
        .expect("consumer");
        db.record_consumer(
            "crates/z/src/facade.rs",
            "rewrite",
            "crates/x/src/v.rs",
            Some(2),
        )
        .expect("consumer");
        db.record_consumer(
            "crates/a/src/symbols.rs",
            "from_outside",
            "crates/x/src/w.rs",
            Some(3),
        )
        .expect("consumer");
        db.record_consumer("crates/a/src/symbols.rs", "*", "crates/x/src/g.rs", Some(4))
            .expect("glob");
        // Simulate what a warm rebuild leaves behind: kinds copied at record
        // time from rows that no longer describe the producers.
        db.conn_ref()
            .execute(
                "UPDATE wiring_map SET symbol_kind = 'method' WHERE consumer_file IS NOT NULL AND symbol_name != '*'",
                [],
            )
            .expect("corrupt");

        let changed = db
            .resolve_consumer_kinds_from_producers(true)
            .expect("pass 1");
        assert_eq!(
            changed, 3,
            "the three symbol rows are rewritten; the glob is untouched"
        );
        assert_eq!(
            kind_of(
                &db,
                "crates/a/src/symbols.rs",
                "extract_symbols",
                "crates/x/src/u.rs"
            ),
            "function"
        );
        // Homonym fallback: (module_file, symbol_kind) order → crates/b's `module`
        // beats crates/c's `function`, on every machine and every walk order.
        assert_eq!(
            kind_of(
                &db,
                "crates/z/src/facade.rs",
                "rewrite",
                "crates/x/src/v.rs"
            ),
            "module"
        );
        assert_eq!(
            kind_of(
                &db,
                "crates/a/src/symbols.rs",
                "from_outside",
                "crates/x/src/w.rs"
            ),
            "extern"
        );
        assert_eq!(
            kind_of(&db, "crates/a/src/symbols.rs", "*", "crates/x/src/g.rs"),
            "glob_import"
        );

        let again = db
            .resolve_consumer_kinds_from_producers(true)
            .expect("pass 2");
        assert_eq!(
            again, 0,
            "a second pass over the same producers changes nothing"
        );
    }

    #[test]
    fn extern_is_only_concluded_over_a_complete_walk() {
        let (_tmp, db) = setup();
        db.register_pub_symbol("crates/a/src/lib.rs", "present", "struct", "public")
            .expect("producer");
        db.record_consumer(
            "crates/a/src/lib.rs",
            "absent",
            "crates/x/src/u.rs",
            Some(1),
        )
        .expect("consumer");
        // Recorded with no producer anywhere → 'unknown' at INSERT time.
        assert_eq!(
            kind_of(&db, "crates/a/src/lib.rs", "absent", "crates/x/src/u.rs"),
            "unknown"
        );
        db.resolve_consumer_kinds_from_producers(false)
            .expect("partial walk");
        assert_eq!(
            kind_of(&db, "crates/a/src/lib.rs", "absent", "crates/x/src/u.rs"),
            "unknown",
            "after a partial walk the row waits for a producer instead of being branded extern"
        );
        db.resolve_consumer_kinds_from_producers(true)
            .expect("complete walk");
        assert_eq!(
            kind_of(&db, "crates/a/src/lib.rs", "absent", "crates/x/src/u.rs"),
            "extern"
        );
    }
}

#[cfg(test)]
mod consumer_clear_split_tests {
    //! I16: the walk clears what it re-derives per file; the inferred edges,
    //! derivable only after every producer exists, survive the walk and are
    //! replaced in the post-walk transaction.
    use super::WiringOrigin;
    use crate::knowledge::FileKnowledgeDB;
    use rusqlite::params;
    use tempfile::TempDir;

    fn consumer_rows(db: &FileKnowledgeDB, consumer: &str) -> Vec<String> {
        let mut stmt = db
            .conn_ref()
            .prepare("SELECT contract_source FROM wiring_map WHERE consumer_file = ?1 ORDER BY 1")
            .expect("prepare");
        stmt.query_map(params![consumer], |r| r.get::<_, String>(0))
            .expect("query")
            .filter_map(Result::ok)
            .collect()
    }

    #[test]
    fn the_walk_clear_keeps_inferred_edges_and_the_post_walk_clear_takes_only_them() {
        let tmp = TempDir::new().expect("tempdir");
        let db = FileKnowledgeDB::new(&tmp.path().join("split.db")).expect("open db");
        db.register_pub_symbol("crates/a/src/lib.rs", "Widget", "struct", "public")
            .expect("producer");
        db.register_pub_symbol("crates/a/src/lib.rs", "render", "function", "public")
            .expect("producer");
        // One declared (`use`) edge and one inferred (bare-name) edge, same consumer.
        db.record_consumer(
            "crates/a/src/lib.rs",
            "Widget",
            "crates/x/src/u.rs",
            Some(1),
        )
        .expect("declared");
        db.record_consumer_with_origin(
            "crates/a/src/lib.rs",
            "render",
            "crates/x/src/u.rs",
            None,
            WiringOrigin::AstInferred,
        )
        .expect("inferred");
        assert_eq!(
            consumer_rows(&db, "crates/x/src/u.rs"),
            vec!["ast_inferred", "ast_resolved"]
        );

        let n = db
            .clear_declared_consumer_entries("crates/x/src/u.rs")
            .expect("walk clear");
        assert_eq!(n, 1);
        assert_eq!(
            consumer_rows(&db, "crates/x/src/u.rs"),
            vec!["ast_inferred"],
            "the walk leaves the inferred edge in place — it is re-derived only after the walk"
        );

        let m = db
            .clear_inferred_consumer_entries("crates/x/src/u.rs")
            .expect("post-walk clear");
        assert_eq!(m, 1);
        assert!(consumer_rows(&db, "crates/x/src/u.rs").is_empty());
        // Neither clear touches producer rows.
        let producers: i64 = db
            .conn_ref()
            .query_row(
                "SELECT COUNT(*) FROM wiring_map WHERE consumer_file IS NULL",
                [],
                |r| r.get(0),
            )
            .expect("count");
        assert_eq!(producers, 2);
    }
}

#[cfg(test)]
mod phantom_module_tests {
    use crate::knowledge::FileKnowledgeDB;
    use tempfile::TempDir;

    fn setup() -> (TempDir, FileKnowledgeDB) {
        let tmp = TempDir::new().unwrap();
        let db = FileKnowledgeDB::new(&tmp.path().join("test.db")).unwrap();
        (tmp, db)
    }

    /// Both keys must be enumerable — the rebuild's sweep can only retire what
    /// it can see, and a phantom `module_file` appears in NO other table.
    #[test]
    fn distinct_module_files_lists_producer_and_consumer_keys() {
        let (_tmp, db) = setup();
        db.register_pub_symbol("crates/a/src/real.rs", "Thing", "struct", "public")
            .unwrap();
        db.record_consumer(
            "crates/facade/src/real.rs", // phantom: resolver invented this path
            "Thing",
            "crates/b/src/uses.rs",
            None,
        )
        .unwrap();

        let modules = db.distinct_module_files().unwrap();
        assert!(modules.iter().any(|m| m == "crates/a/src/real.rs"));
        assert!(modules.iter().any(|m| m == "crates/facade/src/real.rs"));
    }

    /// Retiring a module removes its consumer edges too — `clear_wiring` deletes
    /// only `consumer_file IS NULL` rows, which would leave the edge parked on a
    /// key that names no file.
    #[test]
    fn purge_module_rows_removes_producer_and_consumer_rows() {
        let (_tmp, db) = setup();
        db.register_pub_symbol("crates/ghost/src/gone.rs", "Old", "struct", "public")
            .unwrap();
        db.record_consumer(
            "crates/ghost/src/gone.rs",
            "Old",
            "crates/b/src/uses.rs",
            None,
        )
        .unwrap();

        let removed = db.purge_module_rows("crates/ghost/src/gone.rs").unwrap();
        assert!(
            removed >= 2,
            "producer + consumer rows expected, got {removed}"
        );
        assert!(
            !db.distinct_module_files()
                .unwrap()
                .iter()
                .any(|m| m == "crates/ghost/src/gone.rs")
        );
    }

    /// Purging one module must not disturb another — the sweep runs over every
    /// key in the table, so an over-broad delete would erase live wiring.
    #[test]
    fn purge_is_scoped_to_the_named_module() {
        let (_tmp, db) = setup();
        db.register_pub_symbol("crates/a/src/keep.rs", "Keep", "struct", "public")
            .unwrap();
        db.register_pub_symbol("crates/a/src/drop.rs", "Drop", "struct", "public")
            .unwrap();

        db.purge_module_rows("crates/a/src/drop.rs").unwrap();
        let modules = db.distinct_module_files().unwrap();
        assert!(modules.iter().any(|m| m == "crates/a/src/keep.rs"));
        assert!(!modules.iter().any(|m| m == "crates/a/src/drop.rs"));
    }
}

#[cfg(test)]
mod companion_key_tests {
    //! Decision 1-A (14/09/2026): companion files (`@companion/<name>/…`) are
    //! searchable and never wiring. The gate refuses them on every write path, in
    //! both wiring modes, so no caller — rebuild, edit hook, file watcher — can
    //! let one in.
    use crate::knowledge::FileKnowledgeDB;
    use tempfile::TempDir;

    fn count(db: &FileKnowledgeDB, sql: &str) -> i64 {
        db.conn_ref()
            .query_row(sql, [], |r| r.get(0))
            .expect("count")
    }

    #[test]
    fn a_companion_file_never_produces_consumes_or_owes_an_import() {
        let tmp = TempDir::new().expect("tmp");
        let db = FileKnowledgeDB::new(&tmp.path().join("test.db")).expect("db");
        for polyglot in [false, true] {
            assert!(
                !super::is_wireable_source("@companion/skills/x/scripts/y.py", polyglot, &[]),
                "polyglot={polyglot}"
            );
            assert!(
                !super::is_wireable_source("@companion/skills/x/src/lib.rs", polyglot, &[]),
                "polyglot={polyglot}"
            );
        }
        db.register_pub_symbol("@companion/skills/x/src/lib.rs", "Api", "fn", "public")
            .expect("register");
        db.register_pub_symbol("crates/a/src/lib.rs", "Api", "fn", "public")
            .expect("register");
        // A companion consuming a project producer, and a project file consuming
        // a companion producer: neither edge is recorded.
        db.record_consumer(
            "crates/a/src/lib.rs",
            "Api",
            "@companion/skills/x/src/main.rs",
            None,
        )
        .expect("consumer");
        db.record_consumer(
            "@companion/skills/x/src/lib.rs",
            "Api",
            "crates/b/src/uses.rs",
            None,
        )
        .expect("consumer");
        db.record_unresolved_import_classified(
            "crate::missing",
            "Gone",
            "@companion/skills/x/src/main.rs",
            None,
            "rust",
            "workspace_unresolved",
        )
        .expect("unresolved");
        // A project file's unresolved import, so the table exists and the
        // assertion below always runs (B10: it used to be skipped when the only
        // write was the refused one).
        db.record_unresolved_import_classified(
            "crate::missing",
            "Gone",
            "crates/b/src/uses.rs",
            None,
            "rust",
            "workspace_unresolved",
        )
        .expect("unresolved");

        assert_eq!(
            count(
                &db,
                "SELECT COUNT(*) FROM wiring_map WHERE module_file LIKE '@companion/%' OR consumer_file LIKE '@companion/%'"
            ),
            0
        );
        assert_eq!(
            count(&db, "SELECT COUNT(*) FROM wiring_map"),
            1,
            "the project producer is the only row"
        );
        assert!(db.unresolved_table_present());
        assert_eq!(
            count(&db, "SELECT COUNT(*) FROM wiring_unresolved"),
            1,
            "only the project file owes an import"
        );
    }
}

#[cfg(test)]
mod backfill_tests {
    use crate::knowledge::FileKnowledgeDB;
    use tempfile::TempDir;

    fn setup() -> (TempDir, FileKnowledgeDB) {
        let tmp = TempDir::new().unwrap();
        let db = FileKnowledgeDB::new(&tmp.path().join("test.db")).unwrap();
        (tmp, db)
    }

    fn kind_of(db: &FileKnowledgeDB, module: &str, symbol: &str) -> String {
        db.conn_ref()
            .query_row(
                "SELECT symbol_kind FROM wiring_map
                 WHERE module_file = ?1 AND symbol_name = ?2 AND consumer_file IS NOT NULL",
                rusqlite::params![module, symbol],
                |r| r.get::<_, String>(0),
            )
            .unwrap()
    }

    // Wave H+1 case (a): the import resolved to a facade mod.rs that only
    // re-exports the symbol — no producer row exists for that module_file,
    // but the cascading COALESCE recovers the kind from the defining module.
    #[test]
    fn record_consumer_resolves_kind_through_reexport_facade() {
        let (_tmp, db) = setup();
        db.register_pub_symbol(
            "crates/a/src/hook_runtime.rs",
            "HookRuntime",
            "struct",
            "public",
        )
        .unwrap();
        db.record_consumer(
            "crates/a/src/runtime/mod.rs", // facade path — no producer row here
            "HookRuntime",
            "crates/b/src/consumer.rs",
            Some(3),
        )
        .unwrap();
        assert_eq!(
            kind_of(&db, "crates/a/src/runtime/mod.rs", "HookRuntime"),
            "struct",
            "re-export consumer row must inherit the defining module's kind"
        );
    }

    // Wave H+1 case (b): the consumer was indexed BEFORE its producer during
    // a rebuild walk — the INSERT-time lookup saw an empty table and froze
    // 'unknown'; the post-rebuild backfill repairs it.
    #[test]
    fn backfill_repairs_consumer_indexed_before_producer() {
        let (_tmp, db) = setup();
        db.record_consumer(
            "crates/a/src/engine.rs",
            "Engine",
            "crates/b/src/user.rs",
            Some(7),
        )
        .unwrap();
        assert_eq!(kind_of(&db, "crates/a/src/engine.rs", "Engine"), "unknown");
        db.register_pub_symbol("crates/a/src/engine.rs", "Engine", "struct", "public")
            .unwrap();
        let repaired = db.backfill_unknown_consumer_kinds(true).unwrap();
        assert_eq!(repaired, 1, "exactly the frozen row must be repaired");
        assert_eq!(kind_of(&db, "crates/a/src/engine.rs", "Engine"), "struct");
        // Idempotent: a second pass has nothing left to do.
        assert_eq!(db.backfill_unknown_consumer_kinds(true).unwrap(), 0);
    }

    // Wave H+1 normalization: `use m::{X as Y}` keys the row by the ORIGINAL
    // name so the producer lookup matches; `use m::*` gets a dedicated kind.
    #[test]
    fn record_consumer_normalizes_alias_and_glob_imports() {
        let (_tmp, db) = setup();
        db.register_pub_symbol("crates/a/src/svc.rs", "GraphService", "struct", "public")
            .unwrap();
        db.record_consumer(
            "crates/a/src/svc.rs",
            "GraphService as GS",
            "crates/b/src/x.rs",
            None,
        )
        .unwrap();
        assert_eq!(
            kind_of(&db, "crates/a/src/svc.rs", "GraphService"),
            "struct",
            "aliased import must be keyed by the original name"
        );
        db.record_consumer("crates/a/src/svc.rs", "*", "crates/b/src/y.rs", None)
            .unwrap();
        assert_eq!(kind_of(&db, "crates/a/src/svc.rs", "*"), "glob_import");
    }

    // Wave H+1 second backfill pass: a consumer of a symbol with NO producer
    // anywhere (external-crate re-export) is 'extern', not schema degradation.
    /// The behavioural half of the partial-walk guard: after an INCOMPLETE walk
    /// the same row must stay `unknown`, because "no producer anywhere" is not a
    /// conclusion a 62%-complete index is entitled to draw. `extern` is terminal
    /// — nothing revisits it — so a wrong one is permanent.
    #[test]
    fn a_partial_walk_leaves_producerless_consumers_unknown_not_extern() {
        let (_tmp, db) = setup();
        db.record_consumer(
            "crates/a/src/uses_unseen.rs",
            "NotYetWalked", // its producer lives in the part of the tree never read
            "crates/a/src/error.rs",
            None,
        )
        .unwrap();

        // mark_extern = false → pass 1 only; nothing to inherit, so zero repairs.
        assert_eq!(db.backfill_unknown_consumer_kinds(false).unwrap(), 0);
        assert_eq!(
            kind_of(&db, "crates/a/src/uses_unseen.rs", "NotYetWalked"),
            "unknown",
            "a partial walk must not brand an unseen producer as external"
        );

        // A later COMPLETE walk is still free to conclude `extern`.
        assert_eq!(db.backfill_unknown_consumer_kinds(true).unwrap(), 1);
        assert_eq!(
            kind_of(&db, "crates/a/src/uses_unseen.rs", "NotYetWalked"),
            "extern"
        );
    }

    #[test]
    fn backfill_marks_external_reexport_consumers_as_extern() {
        let (_tmp, db) = setup();
        db.record_consumer(
            "crates/a/src/test_util.rs",
            "assert_eq", // `pub use pretty_assertions::assert_eq` — never indexed
            "crates/a/src/error.rs",
            None,
        )
        .unwrap();
        assert_eq!(
            kind_of(&db, "crates/a/src/test_util.rs", "assert_eq"),
            "unknown"
        );
        assert_eq!(db.backfill_unknown_consumer_kinds(true).unwrap(), 1);
        assert_eq!(
            kind_of(&db, "crates/a/src/test_util.rs", "assert_eq"),
            "extern"
        );
    }
}

#[cfg(test)]
mod provenance_tests {
    use super::{UnresolvedImport, WiringOrigin};
    use crate::knowledge::FileKnowledgeDB;
    use tempfile::TempDir;

    fn setup() -> (TempDir, FileKnowledgeDB) {
        let tmp = TempDir::new().expect("tempdir");
        let db = FileKnowledgeDB::new(&tmp.path().join("prov.db")).expect("open db");
        (tmp, db)
    }

    fn contract_source_of(db: &FileKnowledgeDB, module: &str, symbol: &str) -> String {
        db.conn_ref()
            .query_row(
                "SELECT contract_source FROM wiring_map
                 WHERE module_file = ?1 AND symbol_name = ?2 AND consumer_file IS NOT NULL",
                rusqlite::params![module, symbol],
                |r| r.get::<_, String>(0),
            )
            .expect("consumer row")
    }

    // THE containment test. Unresolved imports must never read as consumers —
    // if they did, S1 would erase true orphans and invert the metric it exists
    // to explain. The assertion is set equality before/after, not a count.
    #[test]
    fn unresolved_imports_never_change_the_orphan_set() {
        let (_tmp, db) = setup();
        db.register_pub_symbol("crates/a/src/lib.rs", "Alpha", "struct", "public")
            .expect("producer");
        db.register_pub_symbol("crates/b/src/lib.rs", "Beta", "struct", "public")
            .expect("producer");

        let before: Vec<String> = db
            .orphan_symbols()
            .expect("orphans")
            .into_iter()
            .map(|e| format!("{}::{}", e.module_file, e.symbol_name))
            .collect();
        assert_eq!(before.len(), 2, "both producers start orphan");

        // Unresolved rows naming the SAME symbols the producers declare: the
        // adversarial case — if the write path leaked into wiring_map, these
        // would wire both producers and the orphan set would empty out.
        for (module, symbol) in [
            ("mystery_crate::alpha", "Alpha"),
            ("mystery_crate::beta", "Beta"),
            ("crates/a/src/lib.rs", "Alpha"),
        ] {
            db.record_unresolved_import(module, symbol, "crates/c/src/main.rs", Some(7), "rust")
                .expect("unresolved");
        }

        let after: Vec<String> = db
            .orphan_symbols()
            .expect("orphans")
            .into_iter()
            .map(|e| format!("{}::{}", e.module_file, e.symbol_name))
            .collect();
        assert_eq!(before, after, "unresolved rows must not resolve any orphan");
        assert_eq!(db.name_only_candidates().expect("count"), 3);
    }

    /// W (2026-09-02): `./crates/…` and `crates/…` are ONE file. Measured after
    /// a full rebuild: 12.032 producer rows and 115.986 consumer edges under the
    /// dot form, the rebuild writing the plain form — 78 plain producers had all
    /// their consumers one prefix away and read orphan.
    #[test]
    fn canonicalize_strips_a_leading_dot_slash() {
        let (_tmp, db) = setup();
        assert_eq!(
            db.canonicalize_module_path("./crates/a/src/lib.rs")
                .as_ref(),
            "crates/a/src/lib.rs"
        );
    }

    #[test]
    fn migration_merges_the_dot_prefixed_universe_into_the_plain_one() {
        let (_tmp, db) = setup();
        db.conn_ref()
            .execute(
                "INSERT INTO wiring_map
                 (module_file, symbol_name, symbol_kind, visibility, consumer_file, contract_source)
                 VALUES ('./crates/a/src/lib.rs', 'Alpha', 'struct', 'public', NULL, 'ast_declared'),
                        ('./crates/a/src/lib.rs', 'Alpha', 'struct', 'public', './crates/b/src/use.rs', 'ast_inferred')",
                [],
            )
            .expect("legacy rows");
        db.register_pub_symbol("crates/a/src/lib.rs", "Alpha", "struct", "public")
            .expect("today's producer");

        db.migrate_canonicalize_paths().expect("migration");

        let dot: i64 = db
            .conn_ref()
            .query_row(
                "SELECT COUNT(*) FROM wiring_map
                 WHERE module_file LIKE './%' OR consumer_file LIKE './%'",
                [],
                |r| r.get(0),
            )
            .expect("count");
        assert_eq!(dot, 0, "no dot-prefixed key may survive the migration");
        let merged: i64 = db
            .conn_ref()
            .query_row(
                "SELECT COUNT(*) FROM wiring_map
                 WHERE module_file = 'crates/a/src/lib.rs' AND symbol_name = 'Alpha'
                   AND consumer_file = 'crates/b/src/use.rs'",
                [],
                |r| r.get(0),
            )
            .expect("count");
        assert_eq!(
            merged, 1,
            "the legacy consumer edge must now sit under the plain key"
        );
    }

    /// W4 (2026-09-02): a bare call name and a type reference both reach their
    /// producers through the shared inference step, recorded as `ast_inferred`
    /// — the step the hook path lacked (edited files lost every inferred edge).
    #[test]
    fn record_inferred_consumers_wires_free_calls_and_type_refs() {
        let (_tmp, db) = setup();
        db.register_pub_symbol("crates/a/src/limits.rs", "apply_caps", "function", "public")
            .expect("callable producer");
        db.register_pub_symbol("crates/a/src/limits.rs", "Limits", "struct", "public")
            .expect("type producer");
        let n = db.record_inferred_consumers(
            "crates/b/src/exec.rs",
            &["apply_caps".to_string(), "no_such_fn".to_string()],
            &["Limits".to_string()],
            &[],
        );
        assert_eq!(n, 2, "one call edge + one type edge");
        let rows: Vec<(String, String)> = db
            .conn_ref()
            .prepare(
                "SELECT symbol_name, contract_source FROM wiring_map
                 WHERE consumer_file = 'crates/b/src/exec.rs' ORDER BY symbol_name",
            )
            .and_then(|mut s| {
                s.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
                    .map(|it| it.filter_map(Result::ok).collect())
            })
            .expect("rows");
        assert_eq!(
            rows,
            vec![
                ("Limits".to_string(), "ast_inferred".to_string()),
                ("apply_caps".to_string(), "ast_inferred".to_string()),
            ]
        );
        assert_eq!(
            db.record_inferred_consumers("crates/b/src/exec.rs", &[], &[], &[]),
            0
        );
    }

    #[test]
    fn a_qualified_pair_resolves_to_the_module_that_owns_the_name() {
        let (_tmp, db) = setup();
        for module in [
            "crates/touring-server/src/cli/backup.rs",
            "crates/touring-server/src/cli/cascade.rs",
            "crates/touring-other/src/lib.rs",
        ] {
            db.register_pub_symbol(module, "run", "function", "public")
                .expect("producer");
        }
        let got = db
            .find_producer_modules_for_qualified(
                &[("backup".to_string(), "run".to_string())],
                Some("crates/touring-server/src/cli/command_table.rs"),
            )
            .expect("query");
        assert_eq!(
            got,
            vec![(
                "crates/touring-server/src/cli/backup.rs".to_string(),
                "run".to_string()
            )],
            "esperava exatamente o produtor do módulo nomeado"
        );
    }

    #[test]
    fn an_unknown_qualifier_wires_nothing() {
        let (_tmp, db) = setup();
        db.register_pub_symbol("crates/a/src/cli/backup.rs", "run", "function", "public")
            .expect("producer");
        let got = db
            .find_producer_modules_for_qualified(
                &[("nao_existe".to_string(), "run".to_string())],
                None,
            )
            .expect("query");
        assert!(got.is_empty(), "{got:?}");
    }

    #[test]
    fn each_pair_yields_at_most_one_edge() {
        let (_tmp, db) = setup();
        for module in ["crates/a/src/cli/backup.rs", "crates/a/src/cli/cascade.rs"] {
            db.register_pub_symbol(module, "run", "function", "public")
                .expect("producer");
        }
        let got = db
            .find_producer_modules_for_qualified(
                &[
                    ("backup".to_string(), "run".to_string()),
                    ("cascade".to_string(), "run".to_string()),
                ],
                None,
            )
            .expect("query");
        assert_eq!(got.len(), 2, "{got:?}");
    }

    #[test]
    fn provenance_is_written_per_tier_not_as_one_constant() {
        let (_tmp, db) = setup();
        db.register_pub_symbol("crates/a/src/lib.rs", "Alpha", "struct", "public")
            .expect("producer");
        db.record_consumer("crates/a/src/lib.rs", "Alpha", "crates/b/src/use.rs", None)
            .expect("resolved consumer");
        db.record_consumer_with_origin(
            "crates/a/src/lib.rs",
            "Alpha",
            "crates/c/src/guess.rs",
            None,
            WiringOrigin::AstInferred,
        )
        .expect("inferred consumer");

        // Producer keeps the strongest tier; the two consumer rows differ —
        // which is the whole point: one column, more than one value.
        let producer: String = db
            .conn_ref()
            .query_row(
                "SELECT contract_source FROM wiring_map
                 WHERE module_file = 'crates/a/src/lib.rs' AND consumer_file IS NULL",
                [],
                |r| r.get(0),
            )
            .expect("producer row");
        assert_eq!(producer, WiringOrigin::AstDeclared.as_str());

        let sources: Vec<String> = db
            .conn_ref()
            .prepare(
                "SELECT contract_source FROM wiring_map
                 WHERE consumer_file IS NOT NULL ORDER BY consumer_file",
            )
            .and_then(|mut s| {
                s.query_map([], |r| r.get::<_, String>(0))
                    .map(|rows| rows.filter_map(Result::ok).collect())
            })
            .expect("consumer rows");
        assert_eq!(
            sources,
            vec![
                WiringOrigin::AstResolved.as_str().to_string(),
                WiringOrigin::AstInferred.as_str().to_string()
            ],
            "b/ resolved, c/ inferred — the heuristic no longer borrows AST credibility"
        );
        assert_eq!(
            contract_source_of(&db, "crates/a/src/lib.rs", "Alpha"),
            WiringOrigin::AstResolved.as_str()
        );
    }

    #[test]
    fn diagnostic_reports_the_two_honesty_counters_apart() {
        let (_tmp, db) = setup();
        db.register_pub_symbol("crates/a/src/lib.rs", "Alpha", "function", "public")
            .expect("producer");
        db.record_consumer_with_origin(
            "crates/a/src/lib.rs",
            "Alpha",
            "crates/c/src/guess.rs",
            None,
            WiringOrigin::AstInferred,
        )
        .expect("inferred");
        db.record_unresolved_import("ghost::mod", "Ghost", "crates/c/src/guess.rs", None, "rust")
            .expect("unresolved");

        let diag = db.wiring_db_diagnostic().expect("diagnostic");
        assert_eq!(
            diag.name_only_candidates,
            Some(1),
            "Some(n) = measured; None would mean the table is missing"
        );
        assert_eq!(diag.heuristic_edges, 1);
        // The census counts only real rows — the unresolved one lives elsewhere
        // and is never folded into the totals.
        assert_eq!(diag.total_rows, 2);
        assert_eq!(
            diag.non_wireable_rows, 0,
            "unresolved must not pollute this"
        );
    }

    /// The defect this file's own author hit on 2026-08-07.
    ///
    /// `ensure_schema` runs only when `user_version < SCHEMA_VERSION`, so the
    /// new table did not materialise on the live DB; every write failed into
    /// `let _ =`, and a bare `COUNT(*)` reported `0`. A zero produced by a
    /// missing table is indistinguishable from a zero produced by a clean
    /// codebase — the exact class of lie this whole change removes. `None` now
    /// says "not measured" out loud.
    #[test]
    fn a_missing_table_reports_not_measured_never_zero() {
        let tmp = TempDir::new().expect("tempdir");
        let db = FileKnowledgeDB::new(&tmp.path().join("no_unresolved.db")).expect("open db");
        db.conn_ref()
            .execute_batch("DROP TABLE IF EXISTS wiring_unresolved")
            .expect("simulate a pre-v9 database");

        assert!(!db.unresolved_table_present());
        assert_eq!(
            db.name_only_candidates(),
            None,
            "absent table must read as unmeasured, not as zero"
        );
        let diag = db.wiring_db_diagnostic().expect("diagnostic still works");
        assert_eq!(diag.name_only_candidates, None);

        // And once the table exists, an empty one is a genuine, measured zero.
        db.conn_ref()
            .execute_batch(
                "CREATE TABLE wiring_unresolved (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    module_path TEXT NOT NULL,
                    symbol_name TEXT NOT NULL,
                    consumer_file TEXT NOT NULL,
                    import_line INTEGER,
                    language TEXT NOT NULL DEFAULT 'rust',
                    observed_at TEXT DEFAULT (datetime('now')))",
            )
            .expect("create");
        assert_eq!(db.name_only_candidates(), Some(0));
    }

    #[test]
    fn rescanning_a_consumer_drops_its_stale_unresolved_rows() {
        let (_tmp, db) = setup();
        db.record_unresolved_import("ghost::a", "A", "crates/c/src/m.rs", None, "rust")
            .expect("first scan");
        db.record_unresolved_import("ghost::b", "B", "crates/d/src/m.rs", None, "rust")
            .expect("other file");
        assert_eq!(db.name_only_candidates().expect("count"), 2);

        let removed = db
            .clear_unresolved_for_consumer("crates/c/src/m.rs")
            .expect("clear");
        assert_eq!(removed, 1);
        assert_eq!(
            db.name_only_candidates().expect("count"),
            1,
            "only the rescanned file's rows are dropped"
        );
    }

    #[test]
    fn unresolved_writes_are_idempotent_across_rebuilds() {
        let (_tmp, db) = setup();
        for _ in 0..3 {
            db.record_unresolved_import("ghost::a", "A", "crates/c/src/m.rs", Some(3), "rust")
                .expect("repeat");
        }
        assert_eq!(db.name_only_candidates().expect("count"), 1);
    }

    #[test]
    fn top_unresolved_modules_rank_by_call_site_count() {
        let (_tmp, db) = setup();
        for i in 0..5 {
            db.record_unresolved_import(
                "hot::mod",
                &format!("S{i}"),
                "crates/c/src/m.rs",
                None,
                "rust",
            )
            .expect("hot");
        }
        db.record_unresolved_import("cold::mod", "S", "crates/c/src/m.rs", None, "rust")
            .expect("cold");

        let ranked = db.unresolved_module_paths(10).expect("ranked");
        assert_eq!(ranked[0], ("hot::mod".to_string(), 5));
        assert_eq!(ranked[1], ("cold::mod".to_string(), 1));
    }

    #[test]
    fn legacy_ast_read_rows_read_back_as_resolved_not_as_weakest() {
        // 77.679 historical rows carry the literal 'ast_read'. They came from
        // the use-resolution path, so degrading them to the weakest tier would
        // invent weakness that the data does not show.
        assert_eq!(
            WiringOrigin::from_contract_source("ast_read"),
            WiringOrigin::AstResolved
        );
        assert_eq!(
            WiringOrigin::from_contract_source("something_unheard_of"),
            WiringOrigin::AstResolved
        );
    }

    #[test]
    fn origin_ordering_and_strength_agree_on_evidence() {
        assert!(WiringOrigin::AstDeclared > WiringOrigin::AstResolved);
        assert!(WiringOrigin::AstResolved > WiringOrigin::TextMatched);
        assert!(WiringOrigin::TextMatched > WiringOrigin::AstInferred);
        assert!(WiringOrigin::AstResolved.strength() > WiringOrigin::AstInferred.strength());
        assert!(WiringOrigin::AstResolved.is_resolved());
        assert!(!WiringOrigin::AstInferred.is_resolved());
        // Round-trip through the DB representation must be lossless, or the
        // breakdown would silently reclassify tiers on read.
        for origin in [
            WiringOrigin::AstDeclared,
            WiringOrigin::AstResolved,
            WiringOrigin::TextMatched,
            WiringOrigin::AstInferred,
        ] {
            assert_eq!(
                WiringOrigin::from_contract_source(origin.as_str()),
                origin,
                "{} must round-trip",
                origin.as_str()
            );
        }
    }

    #[test]
    fn origin_breakdown_is_ordered_strongest_first() {
        let (_tmp, db) = setup();
        db.register_pub_symbol("crates/a/src/lib.rs", "Alpha", "function", "public")
            .expect("declared");
        db.record_consumer("crates/a/src/lib.rs", "Alpha", "crates/b/src/u.rs", None)
            .expect("resolved");
        db.record_consumer_with_origin(
            "crates/a/src/lib.rs",
            "Alpha",
            "crates/c/src/g.rs",
            None,
            WiringOrigin::AstInferred,
        )
        .expect("inferred");

        let breakdown = db.origin_breakdown().expect("breakdown");
        let order: Vec<&str> = breakdown.iter().map(|(s, _)| s.as_str()).collect();
        assert_eq!(order, vec!["ast_declared", "ast_resolved", "ast_inferred"]);
        assert!(breakdown.iter().all(|(_, n)| *n == 1));
    }

    #[test]
    fn unresolved_import_struct_serializes_for_reporting() {
        let value = serde_json::to_value(UnresolvedImport {
            module_path: "ghost::mod".into(),
            symbol_name: "Ghost".into(),
            consumer_file: "crates/c/src/m.rs".into(),
            import_line: Some(12),
            language: "rust".into(),
        })
        .expect("serialize");
        assert_eq!(value["module_path"], "ghost::mod");
        assert_eq!(value["import_line"], 12);
    }
}

#[cfg(test)]
mod workspace_root_derivation_tests {
    use super::derive_workspace_root;
    use std::path::Path;

    #[test]
    fn derives_the_project_root_from_the_canonical_db_layout() {
        assert_eq!(
            derive_workspace_root(Path::new(
                "/home/u/projects/app/.claude/touring/knowledge.db"
            )),
            Some("/home/u/projects/app/".to_string()),
            "the root is three components up, with a trailing slash for strip_prefix"
        );
    }

    #[test]
    fn refuses_the_global_store_under_home() {
        // `$HOME/.claude/touring/knowledge.db` is the GLOBAL store: its rows
        // span every project the daemon has seen, so no prefix is canonical
        // for all of them. Deriving `$HOME` here would make every path under
        // the home directory "relative" — and hand the migration a DELETE
        // whose predicate matches almost everything.
        let home = std::env::var("HOME").expect("HOME set in test env");
        let global = format!("{home}/.claude/touring/knowledge.db");
        assert_eq!(derive_workspace_root(Path::new(&global)), None);
    }

    #[test]
    fn resolves_a_relative_db_path_against_the_current_directory() {
        // The daemon opens `.claude/touring/knowledge.db` with its cwd pinned
        // to the project root. Refusing that shape is refusing the ONLY shape
        // production actually uses: the first cut of this function rejected it
        // (three components up from a relative path is the empty path), so the
        // migration silently did nothing on every project it was meant to fix.
        let cwd = std::env::current_dir().expect("cwd");
        assert_eq!(
            derive_workspace_root(Path::new(".claude/touring/knowledge.db")),
            Some(format!("{}/", cwd.display())),
        );
    }

    #[test]
    fn refuses_a_layout_that_is_not_dot_claude_touring() {
        // Three components up from an arbitrary path is meaningless — and for
        // a shallow path it is `/`, which would strip the leading slash off
        // every absolute path in the table.
        assert_eq!(derive_workspace_root(Path::new("/tmp/scratch.db")), None);
        assert_eq!(
            derive_workspace_root(Path::new("/a/b/c/knowledge.db")),
            None
        );
        assert_eq!(derive_workspace_root(Path::new(":memory:")), None);
    }

    #[test]
    fn refuses_touring_dir_that_is_not_under_dot_claude() {
        assert_eq!(
            derive_workspace_root(Path::new("/home/u/app/config/touring/knowledge.db")),
            None,
            "the parent of `touring/` must be `.claude/` — not any directory"
        );
    }
}

#[cfg(test)]
mod per_project_polyglot_tests {
    use crate::knowledge::FileKnowledgeDB;

    /// A project at the canonical layout, optionally opting into polyglot
    /// wiring, with its knowledge DB open.
    fn project(opt_in: bool) -> (tempfile::TempDir, FileKnowledgeDB) {
        let tmp = tempfile::tempdir().expect("tempdir");
        let cfg_dir = tmp.path().join(".touring");
        std::fs::create_dir_all(&cfg_dir).expect("mkdir .touring");
        std::fs::write(
            cfg_dir.join("touring.toml"),
            format!("polyglot_wiring = {opt_in}\n"),
        )
        .expect("write toml");
        let db_dir = tmp.path().join(".claude").join("touring");
        std::fs::create_dir_all(&db_dir).expect("mkdir .claude/touring");
        let db = FileKnowledgeDB::new(&db_dir.join("knowledge.db")).expect("open db");
        (tmp, db)
    }

    /// An explicit env override outranks the project layer by design, so these
    /// assertions state what holds under BOTH states of the variable rather
    /// than mutating a process-global under a parallel runner.
    fn env_forced() -> Option<bool> {
        std::env::var("TOURING_POLYGLOT_WIRING")
            .ok()
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
    }

    #[test]
    fn two_databases_in_one_process_hold_different_modes() {
        let (_a, yes) = project(true);
        let (_b, no) = project(false);
        match env_forced() {
            Some(f) => assert_eq!((yes.polyglot(), no.polyglot()), (f, f)),
            None => assert!(
                yes.polyglot() && !no.polyglot(),
                "the whole point of the opt-in: one project says yes, its \
                 neighbour says no, same process, same instant"
            ),
        }
    }

    #[test]
    fn the_write_gate_follows_the_projects_answer() {
        // Behaviour, not configuration: a first-party Python producer is
        // admitted by the project that opted in and refused by the one that
        // did not — the same call, in the same process.
        let (_a, yes) = project(true);
        let (_b, no) = project(false);
        for db in [&yes, &no] {
            db.register_pub_symbol("pkg/models.py", "Order", "class", "public")
                .expect("call succeeds either way — the gate is silent");
        }
        let count = |db: &FileKnowledgeDB| -> i64 {
            db.conn_ref()
                .query_row(
                    "SELECT COUNT(*) FROM wiring_map WHERE module_file = 'pkg/models.py'",
                    [],
                    |r| r.get(0),
                )
                .expect("count")
        };
        match env_forced() {
            Some(true) => assert_eq!((count(&yes), count(&no)), (1, 1)),
            Some(false) => assert_eq!((count(&yes), count(&no)), (0, 0)),
            None => {
                assert_eq!(count(&yes), 1, "the opted-in project wires its Python");
                assert_eq!(count(&no), 0, "the other stays Rust-only");
            }
        }
    }

    #[test]
    fn rust_is_wired_in_both_modes() {
        // The opt-in ADDS languages; it never takes Rust away.
        let (_a, yes) = project(true);
        let (_b, no) = project(false);
        for db in [&yes, &no] {
            db.register_pub_symbol("crates/a/src/lib.rs", "Thing", "struct", "public")
                .expect("ok");
            let n: i64 = db
                .conn_ref()
                .query_row(
                    "SELECT COUNT(*) FROM wiring_map WHERE module_file = 'crates/a/src/lib.rs'",
                    [],
                    |r| r.get(0),
                )
                .expect("count");
            assert_eq!(n, 1);
        }
    }

    #[test]
    fn the_258_defence_survives_the_opt_in() {
        // Opting in must not re-open the false-positive class the Rust-only
        // default was protecting: docs/, scripts/, tests and vendored trees
        // stay out even for a project that said yes.
        let (_a, yes) = project(true);
        for path in [
            "docs/plan.py",
            "scripts/build.py",
            "pkg/test_models.py",
            "app/.venv/lib/python3.12/site-packages/x.py",
            "web/node_modules/left-pad/index.js",
        ] {
            yes.register_pub_symbol(path, "X", "class", "public")
                .expect("ok");
        }
        let n: i64 = yes
            .conn_ref()
            .query_row("SELECT COUNT(*) FROM wiring_map", [], |r| r.get(0))
            .expect("count");
        assert_eq!(n, 0, "none of the 258-FP shapes may enter, opt-in or not");
    }
}

#[cfg(test)]
mod build_output_exclusion_tests {
    use super::is_indexable_module_file_polyglot;

    #[test]
    fn rustdoc_javascript_is_not_wiring() {
        // The evidence that motivated the rule: 1.087 of the 1.128 non-Rust
        // files that would have entered THIS workspace's graph on the day the
        // opt-in was switched on were rustdoc output.
        for f in [
            "target/doc/crates.js",
            "target/doc/search.index/0036dee5f75b.js",
            "holon-wasm-components/target/doc/crates.js",
        ] {
            assert!(!is_indexable_module_file_polyglot(f, true), "{f}");
        }
    }

    #[test]
    fn other_build_and_cache_trees_are_not_wiring_either() {
        for f in [
            "build/gen/app.js",
            "web/build/bundle.js",
            "out/index.js",
            "svc/.tox/py312/lib/x.py",
            "svc/.mypy_cache/3.12/x.py",
            "svc/.pytest_cache/v/x.py",
            "cov/htmlcov/index.js",
            "app/.gradle/caches/X.java",
        ] {
            assert!(!is_indexable_module_file_polyglot(f, true), "{f}");
        }
    }

    #[test]
    fn first_party_source_still_passes() {
        // The rule must exclude OUTPUT, not anything whose path happens to
        // contain a word: a package named `outbound` or `building` is source.
        for f in [
            "packages/api/src/models.py",
            "apps/web/src/outbound/client.ts",
            "apps/web/src/building/plan.ts",
            "services/target_practice/aim.py",
        ] {
            assert!(is_indexable_module_file_polyglot(f, true), "{f}");
        }
    }

    #[test]
    fn rust_source_under_no_circumstances_regresses() {
        assert!(is_indexable_module_file_polyglot(
            "crates/a/src/lib.rs",
            false
        ));
        assert!(is_indexable_module_file_polyglot(
            "crates/a/src/lib.rs",
            true
        ));
    }
}

#[cfg(test)]
mod ungated_eviction_tests {
    use crate::knowledge::FileKnowledgeDB;

    fn db_at_root() -> (tempfile::TempDir, FileKnowledgeDB) {
        let tmp = tempfile::tempdir().expect("tempdir");
        let d = tmp.path().join(".claude").join("touring");
        std::fs::create_dir_all(&d).expect("mkdir");
        let db = FileKnowledgeDB::new(&d.join("knowledge.db")).expect("open");
        (tmp, db)
    }

    /// Rows are inserted RAW, bypassing the gate, because that is exactly how
    /// they got there: written by a binary whose gate did not yet know the rule.
    fn raw_producer(db: &FileKnowledgeDB, module: &str) {
        db.conn_ref()
            .execute(
                "INSERT INTO wiring_map (module_file, symbol_name, symbol_kind, visibility, contract_source)
                 VALUES (?1, 'X', 'class', 'public', 'legacy')",
                [module],
            )
            .expect("raw insert");
    }

    fn count(db: &FileKnowledgeDB) -> i64 {
        db.conn_ref()
            .query_row("SELECT COUNT(*) FROM wiring_map", [], |r| r.get(0))
            .expect("count")
    }

    /// Cross-audit 14/09/2026 (B11): a companion consumer written before the gate
    /// (or copied by the legacy consolidation) is evicted; the project producer it
    /// pointed at stays.
    #[test]
    fn evicts_a_companion_consumer_and_keeps_the_producer() {
        let (_t, db) = db_at_root();
        raw_producer(&db, "crates/a/src/lib.rs");
        db.conn_ref()
            .execute(
                "INSERT INTO wiring_map (module_file, symbol_name, symbol_kind, visibility, consumer_file, contract_source)
                 VALUES ('crates/a/src/lib.rs', 'X', 'class', 'public', '@companion/skills/x/main.rs', 'legacy')",
                [],
            )
            .expect("raw consumer");
        assert_eq!(db.migrate_evict_ungated_rows().expect("evict"), 1);
        assert_eq!(count(&db), 1, "the producer row stays");
    }

    #[test]
    fn evicts_what_the_write_gate_would_refuse() {
        let (_t, db) = db_at_root();
        for f in [
            "apps/api/.venv/lib/python3.12/site-packages/pytest/x.py",
            "web/node_modules/left-pad/index.js",
            "docs/plan.py",
            "scripts/build.py",
            "pkg/test_models.py",
            "target/doc/crates.js",
            "crates/a/tests/it.rs",
            "benches/src/b.rs",
            ".cipher/agent_outputs/out.json",
        ] {
            raw_producer(&db, f);
        }
        assert_eq!(count(&db), 9);
        assert_eq!(db.migrate_evict_ungated_rows().expect("evict"), 9);
        assert_eq!(count(&db), 0);
    }

    #[test]
    fn keeps_every_language_the_maximal_vocabulary_admits() {
        // Judged under polyglot = true even though THIS project has not opted
        // in: a project that flips the switch tomorrow must find its Python
        // still there. Only what no mode could ever read is removed.
        let (_t, db) = db_at_root();
        assert!(!db.polyglot(), "this fixture project did not opt in");
        for f in [
            "crates/a/src/lib.rs",
            "packages/api/models.py",
            "apps/web/src/client.ts",
            "apps/web/src/view.tsx",
            "svc/Handler.java",
        ] {
            raw_producer(&db, f);
        }
        assert_eq!(db.migrate_evict_ungated_rows().expect("evict"), 0);
        assert_eq!(count(&db), 5);
    }

    #[test]
    fn a_bogus_consumer_takes_only_its_own_edge() {
        let (_t, db) = db_at_root();
        db.conn_ref()
            .execute(
                "INSERT INTO wiring_map (module_file, symbol_name, symbol_kind, visibility, consumer_file)
                 VALUES ('crates/a/src/lib.rs', 'Thing', 'struct', 'public', ?1)",
                ["app/.venv/lib/python3.12/site-packages/x.py"],
            )
            .expect("insert");
        raw_producer(&db, "crates/a/src/lib.rs");
        assert_eq!(db.migrate_evict_ungated_rows().expect("evict"), 1);
        assert_eq!(
            count(&db),
            1,
            "the producer row survives; only the bogus edge goes"
        );
    }

    #[test]
    fn a_test_file_consuming_first_party_code_keeps_its_edge() {
        // The correction to the rule above. The write gate judges the PRODUCER
        // only; a test consuming the crate's own API is real usage and the
        // graph exists to record it. Judging the consumer by the full
        // predicate deleted 13.621 of these from the touring workspace.
        let (_t, db) = db_at_root();
        for consumer in [
            "crates/a/tests/it.rs",
            "benches/src/bench.rs",
            "docs/example.py",
            "scripts/demo.py",
        ] {
            db.conn_ref()
                .execute(
                    "INSERT INTO wiring_map (module_file, symbol_name, symbol_kind, visibility, consumer_file)
                     VALUES ('crates/a/src/lib.rs', 'Thing', 'struct', 'public', ?1)",
                    [consumer],
                )
                .expect("insert");
        }
        assert_eq!(db.migrate_evict_ungated_rows().expect("evict"), 0);
        assert_eq!(count(&db), 4, "first-party consumers all survive");
    }

    #[test]
    fn is_idempotent() {
        let (_t, db) = db_at_root();
        raw_producer(&db, "docs/plan.py");
        assert_eq!(db.migrate_evict_ungated_rows().expect("first"), 1);
        assert_eq!(db.migrate_evict_ungated_rows().expect("second"), 0);
    }
}

/// A `scripts/` or `docs/` directory that is a Python PACKAGE (it holds
/// `__init__.py`) is source: the analise project keeps its main code there and
/// imports it as `scripts.memoria`, and 0 of its 90.662 wiring rows had a producer
/// under `scripts/` (14/09/2026). A tool directory without `__init__.py` — the
/// touring workspace's own `scripts/` — stays out, as the 258 historical false
/// positives demand.
#[cfg(test)]
mod python_package_source_tests {
    use super::{is_indexable_module_file_with, non_source_sql, python_source_packages};

    #[test]
    fn a_scripts_package_is_source_and_a_tools_directory_is_not() {
        let packages = vec!["scripts".to_string()];
        assert!(is_indexable_module_file_with(
            "scripts/memoria/grafo.py",
            true,
            &packages
        ));
        assert!(!is_indexable_module_file_with(
            "scripts/memoria/grafo.py",
            true,
            &[]
        ));
        assert!(!is_indexable_module_file_with(
            "docs/gen.py",
            true,
            &packages
        ));
        assert!(
            !is_indexable_module_file_with("scripts/memoria/test_grafo.py", true, &packages),
            "test files stay out even inside a package"
        );
        assert!(!is_indexable_module_file_with(
            "crates/x/scripts/tool.py",
            true,
            &packages
        ));
    }

    #[test]
    fn the_orphan_queries_exclude_exactly_what_the_write_gate_refuses() {
        let none = non_source_sql("w.module_file", &[]);
        assert!(none.contains("NOT LIKE 'scripts/%'") && none.contains("NOT LIKE 'docs/%'"));
        let scripts = non_source_sql("w.module_file", &["scripts".to_string()]);
        assert!(!scripts.contains("'scripts/%'"), "{scripts}");
        assert!(scripts.contains("NOT LIKE 'docs/%'"), "{scripts}");
    }

    #[test]
    fn packages_are_read_from_the_project_root() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(dir.path().join("scripts")).expect("scripts");
        std::fs::create_dir_all(dir.path().join("docs")).expect("docs");
        assert!(python_source_packages(Some(dir.path())).is_empty());
        std::fs::write(dir.path().join("scripts/__init__.py"), "").expect("init");
        assert_eq!(
            python_source_packages(Some(dir.path())),
            vec!["scripts".to_string()]
        );
        assert!(python_source_packages(None).is_empty());
    }
}

/// Cross-audit 14/09/2026: B4 (deletes key like inserts), B5 (counted writes)
/// and B8 (provenance never demoted).
#[cfg(test)]
mod key_count_provenance_tests {
    use super::WiringOrigin;
    use crate::knowledge::FileKnowledgeDB;
    use tempfile::TempDir;

    fn setup() -> (TempDir, FileKnowledgeDB) {
        let tmp = TempDir::new().expect("tempdir");
        let db = FileKnowledgeDB::new(&tmp.path().join("k.db")).expect("open db");
        (tmp, db)
    }

    fn count(db: &FileKnowledgeDB, sql: &str) -> i64 {
        db.conn_ref()
            .query_row(sql, [], |r| r.get(0))
            .expect("count")
    }

    #[test]
    fn every_delete_keys_the_path_the_way_the_insert_did() {
        let (_tmp, db) = setup();
        db.register_pub_symbol("crates/a/src/lib.rs", "Alpha", "struct", "public")
            .expect("producer");
        db.record_consumer("crates/a/src/lib.rs", "Alpha", "crates/b/src/use.rs", None)
            .expect("consumer");
        db.clear_consumer_entries("./crates/b/src/use.rs")
            .expect("clear consumer");
        assert_eq!(
            count(
                &db,
                "SELECT COUNT(*) FROM wiring_map WHERE consumer_file IS NOT NULL"
            ),
            0
        );
        db.clear_wiring("./crates/a/src/lib.rs")
            .expect("clear producer");
        assert_eq!(count(&db, "SELECT COUNT(*) FROM wiring_map"), 0);
        db.register_pub_symbol("crates/a/src/lib.rs", "Alpha", "struct", "public")
            .expect("producer");
        assert_eq!(
            db.purge_module_rows("./crates/a/src/lib.rs")
                .expect("purge"),
            1
        );
    }

    #[test]
    fn the_counted_register_reports_only_rows_it_wrote() {
        let (_tmp, db) = setup();
        let reg = |file: &str| {
            db.register_pub_symbol_counted(
                file,
                "Alpha",
                "struct",
                "public",
                WiringOrigin::AstDeclared,
            )
            .expect("register")
        };
        assert!(reg("crates/a/src/lib.rs"), "a new producer row");
        assert!(
            !reg("crates/a/src/lib.rs"),
            "the same row again writes nothing"
        );
        assert!(
            !reg("@companion/rules/a.rs"),
            "a refused file writes nothing"
        );
    }

    #[test]
    fn a_weaker_guess_never_demotes_a_stronger_edge() {
        let (_tmp, db) = setup();
        db.register_pub_symbol("crates/a/src/lib.rs", "Alpha", "struct", "public")
            .expect("producer");
        let source = |db: &FileKnowledgeDB| -> String {
            db.conn_ref()
                .query_row(
                    "SELECT contract_source FROM wiring_map WHERE consumer_file = 'crates/b/src/use.rs'",
                    [],
                    |r| r.get(0),
                )
                .expect("edge")
        };
        let record = |origin| {
            db.record_consumer_with_origin(
                "crates/a/src/lib.rs",
                "Alpha",
                "crates/b/src/use.rs",
                None,
                origin,
            )
            .expect("record");
        };
        record(WiringOrigin::AstInferred);
        assert_eq!(source(&db), "ast_inferred");
        record(WiringOrigin::AstResolved);
        assert_eq!(source(&db), "ast_resolved", "stronger evidence upgrades");
        record(WiringOrigin::AstInferred);
        assert_eq!(source(&db), "ast_resolved", "a guess does not demote it");
    }
}

#[cfg(test)]
mod cross_language_inference_tests {
    use super::{WiringOrigin, cross_language_edge_sql, language_family};
    use crate::knowledge::FileKnowledgeDB;

    /// A polyglot project at the canonical layout. `None` when the env forces
    /// polyglot wiring off: it outranks the project layer, and with it off
    /// there is no Python wiring to test.
    fn polyglot_project() -> Option<(tempfile::TempDir, FileKnowledgeDB)> {
        if std::env::var("TOURING_POLYGLOT_WIRING")
            .is_ok_and(|v| v == "0" || v.eq_ignore_ascii_case("false"))
        {
            return None;
        }
        let tmp = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(tmp.path().join(".touring")).expect("mkdir .touring");
        std::fs::write(
            tmp.path().join(".touring/touring.toml"),
            "polyglot_wiring = true\n",
        )
        .expect("write toml");
        let db_dir = tmp.path().join(".claude").join("touring");
        std::fs::create_dir_all(&db_dir).expect("mkdir .claude/touring");
        let db = FileKnowledgeDB::new(&db_dir.join("knowledge.db")).expect("open db");
        Some((tmp, db))
    }

    #[test]
    fn families_follow_the_wireable_extensions() {
        assert_eq!(language_family("benches/gpu_benchmark.rs"), Some("rust"));
        assert_eq!(
            language_family("scripts/memoria/grafo_modelo.py"),
            Some("python")
        );
        assert_eq!(language_family("web/app.tsx"), Some("js"));
        assert_eq!(language_family("web/lib.mjs"), Some("js"));
        assert_eq!(language_family("go:example.com/pkg"), Some("go"));
        assert_eq!(language_family("docs/README.md"), None);
        assert_eq!(language_family("touring-daemon://dispatch"), None);
    }

    /// 18/09/2026 (analise): `no.tags_cli()` in a file that imports `No` from
    /// `modelo.py` is a use of the method. The same name in a module the file
    /// never imports from is not, and neither is a function of that name.
    #[test]
    fn a_python_method_is_credited_only_in_a_module_the_consumer_imports() {
        let Some((_tmp, db)) = polyglot_project() else {
            return;
        };
        for (module, symbol, kind) in [
            ("memoria/modelo.py", "No", "class"),
            ("memoria/modelo.py", "tags_cli", "method"),
            ("outro/servico.py", "tags_cli", "method"),
            ("memoria/modelo.py", "todas_as_arestas", "function"),
        ] {
            assert!(
                db.register_pub_symbol_counted(
                    module,
                    symbol,
                    kind,
                    "public",
                    WiringOrigin::AstDeclared
                )
                .expect("register"),
                "{module}::{symbol}"
            );
        }
        db.record_consumer("memoria/modelo.py", "No", "memoria/gravacao.py", None)
            .expect("the import edge");
        let names = vec!["tags_cli".to_string(), "todas_as_arestas".to_string()];

        assert_eq!(
            db.find_python_method_producers(&names, "memoria/gravacao.py")
                .expect("lookup"),
            [("memoria/modelo.py".to_string(), "tags_cli".to_string())],
            "only the imported module, only a method"
        );
        assert!(
            db.find_python_method_producers(&names, "artefato/solto.py")
                .expect("lookup")
                .is_empty(),
            "no import, no gate"
        );
        assert_eq!(
            db.record_inferred_consumers("memoria/gravacao.py", &names, &[], &[]),
            1,
            "the storage entry both paths call routes a Python consumer through the gate"
        );
    }

    /// 18/09/2026 (analise): a bare `cosine_similarity(…)` in a `.rs` benchmark
    /// was recorded as a use of the Python function of that name — 15.758 such
    /// edges hid Python orphans. The inference matches only producers of the
    /// consumer's own language family, in both directions.
    #[test]
    fn the_by_name_inference_never_crosses_languages() {
        let Some((_tmp, db)) = polyglot_project() else {
            return;
        };
        for module in ["src/sim.rs", "pkg/sim.py"] {
            assert!(
                db.register_pub_symbol_counted(
                    module,
                    "cosine_similarity",
                    "function",
                    "public",
                    WiringOrigin::AstDeclared,
                )
                .expect("register"),
                "{module} refused by the write gate"
            );
        }
        let names = vec!["cosine_similarity".to_string()];
        db.record_inferred_consumers("src/bench.rs", &names, &[], &[]);
        db.record_inferred_consumers("pkg/app.py", &names, &[], &[]);

        let edges: Vec<(String, String)> = db
            .conn_ref()
            .prepare(
                "SELECT consumer_file, module_file FROM wiring_map
                 WHERE consumer_file IS NOT NULL ORDER BY consumer_file",
            )
            .expect("prepare")
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .expect("query")
            .collect::<Result<_, _>>()
            .expect("rows");
        // A Python attribute name reaches only METHODS of modules the file
        // imports from (`find_python_method_producers`): a bare function of that
        // name is no use at all, in its own language or the other one.
        assert_eq!(
            edges,
            [("src/bench.rs".to_string(), "src/sim.rs".to_string())]
        );
        let crossing: i64 = db
            .conn_ref()
            .query_row(
                &format!(
                    "SELECT COUNT(*) FROM wiring_map WHERE {}",
                    cross_language_edge_sql()
                ),
                [],
                |r| r.get(0),
            )
            .expect("count");
        assert_eq!(crossing, 0);
    }
}
