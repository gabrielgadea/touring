//! Wiring persistence layer for [`FileKnowledgeDB`] — pub-symbol →
//! consumer CRUD on the `wiring_map` table (orphan detection substrate).
//!
//! Carved from `touring-hooks/src/wiring.rs` in the daemon-lib-rearch
//! Phase C split (2026-06-10): these are inherent methods on
//! [`FileKnowledgeDB`], and Rust requires inherent impls to live in the
//! crate that defines the type. The graph/engine layer (impact BFS, Tarjan
//! cycles, repair scans) stays in `touring_hooks::wiring`, which re-exports
//! everything here so historical paths keep resolving.

use std::borrow::Cow;

use rusqlite::params;

use crate::knowledge::FileKnowledgeDB;

/// Workspace root marker — the substring stripped from absolute paths to
/// produce canonical relative paths in the wiring_map.
///
/// Centralizing this constant prevents path-homonimia: the same file MUST
/// have exactly one canonical representation across producer and consumer
/// rows, otherwise orphan detection produces 100% false positives for that
/// row (producer at path A, consumer at path B → no JOIN match).
///
/// Resolution: `TOURING_WORKSPACE_ROOT` env override (Productization Fase 0 —
/// lets the canonical workspace live anywhere, e.g. `~/projects/touring`) →
/// compiled default (the historical global workspace). Always ends with `/`
/// so `strip_prefix` yields a clean relative path. Cached per process.
pub(crate) fn workspace_root_marker() -> &'static str {
    static MARKER: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    MARKER.get_or_init(|| {
        let mut root = std::env::var("TOURING_WORKSPACE_ROOT")
            .unwrap_or_else(|_| "/home/gabrielgadea/projects/touring".to_string());
        if !root.ends_with('/') {
            root.push('/');
        }
        root
    })
}

/// Polyglot-wiring opt-in — reads `TOURING_POLYGLOT_WIRING` once per process.
///
/// Default **OFF** (unset or `"0"`): the wiring graph is Rust-only, byte-identical
/// to pre-polyglot behavior — `non_rust` diagnostic rows stay 0 and the 258
/// historical false-positive orphans from `docs/*.py` / `scripts/*.py` never
/// resurface. Set to `"1"` (or `"true"`) to let per-language sources (PoC scope:
/// Python) populate producer/consumer rows via the already-polyglot indexing
/// feeder (`cli_index_rebuild` → `extract_file_imports` →
/// `resolve_import_path_with_source` → `record_consumer`).
///
/// Cached in a `OnceLock` because the wiring gate is on the hot indexing path
/// (`register_pub_symbol` / `record_consumer` run once per symbol / import).
///
/// `pub` so the polyglot feeders (e.g. the Go package-aware pass in
/// `cli_index_rebuild`, P-H) can early-return when the flag is off and skip the
/// extraction work entirely — the single source of truth for the opt-in, shared
/// via the same process `OnceLock`.
///
/// See `docs/2026-07-03-polyglot-parity-plan.md` §5 (keystone P-A).
#[must_use]
pub fn polyglot_wiring_enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var("TOURING_POLYGLOT_WIRING")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false)
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

/// Non-wireable polyglot paths — the per-language analogue of the benches/tests
/// exclusion. Blocks the exact trees that produced the 258 historical
/// false-positive orphans (`docs/*.py`, `scripts/*.py`) plus vendored/generated
/// trees (venv / site-packages / node_modules / dist / `__pycache__`) and
/// per-language test files, so a polyglot opt-in never re-pollutes the orphan
/// diagnostic.
#[must_use]
fn is_non_rust_non_wireable(module_file: &str) -> bool {
    // Vendored / generated trees (Python venv/site-packages; JS/TS
    // node_modules + build output).
    let vendored = module_file.contains("/site-packages/")
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
        || module_file.contains("/coverage/");
    // Non-source subtrees — the exact source of the 258 historical false
    // positives (docs/*.py, scripts/*.py).
    let non_source = module_file.starts_with("docs/")
        || module_file.contains("/docs/")
        || module_file.starts_with("scripts/")
        || module_file.contains("/scripts/");
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
    vendored || non_source || py_test || js_test || java_test
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
/// Used as the gate for `register_pub_symbol` / `record_consumer`. Default mode
/// is Rust-only; `TOURING_POLYGLOT_WIRING=1` also admits Python (see
/// [`polyglot_wiring_enabled`]).
///
/// # Audit reference
///
/// Pre-fix evidence (2026-05-11 orphan-count audit):
/// - 43 false positives from `benches/src/*.rs` because the legacy filter
///   `LIKE '%/benches/%'` did not match leading `benches/` (no slash prefix).
/// - 258 false positives from `docs/*.py` and `scripts/*.py` because
///   `register_pub_symbol` accepted any extension. Under the polyglot opt-in
///   these stay blocked via [`is_python_non_wireable`].
#[must_use]
pub(crate) fn is_indexable_module_file(module_file: &str) -> bool {
    is_indexable_module_file_polyglot(module_file, polyglot_wiring_enabled())
}

/// Pure policy core of [`is_indexable_module_file`], split out so the gate can
/// be unit-tested in both modes without touching the process-global env flag.
#[must_use]
fn is_indexable_module_file_polyglot(module_file: &str, polyglot: bool) -> bool {
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
    if polyglot && !module_file.ends_with(".rs") && is_non_rust_non_wireable(module_file) {
        return false;
    }
    true
}

/// Canonicalize `module_file` to a workspace-relative path.
///
/// If `module_file` starts with `workspace_root_marker()`, the prefix is
/// stripped. Otherwise the path is returned unchanged (borrowed).
///
/// This is the single source of truth for path normalization in
/// wiring_map. Calling it on the producer and the consumer sides ensures
/// the JOIN matches even when one side was reported via absolute path
/// (`/home/...`) and the other via relative path (`crates/...`).
#[must_use]
pub(crate) fn canonicalize_module_path(module_file: &str) -> Cow<'_, str> {
    if let Some(stripped) = module_file.strip_prefix(workspace_root_marker()) {
        Cow::Borrowed(stripped)
    } else {
        Cow::Borrowed(module_file)
    }
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
    /// Rows referring to non-Rust source files.
    pub non_rust_rows: i64,
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
        let canonical = canonicalize_module_path(module_file);
        if !is_indexable_module_file(&canonical) {
            return Ok(());
        }
        self.conn_ref().execute(
            "INSERT OR IGNORE INTO wiring_map (module_file, symbol_name, symbol_kind, visibility, contract_source)
             VALUES (?1, ?2, ?3, ?4, 'ast_read')",
            params![canonical.as_ref(), symbol_name, symbol_kind, visibility],
        )?;
        // Invalidate aggregate cache — new pub symbol changes module totals.
        Self::invalidate_wiring_modules_cache();
        Ok(())
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
        let canonical_module = canonicalize_module_path(module_file);
        let canonical_consumer = canonicalize_module_path(consumer_file);
        if !is_indexable_module_file(&canonical_module) {
            return Ok(());
        }
        // Wave H+1 (2026-06-11): normalize the import form before keying.
        // `use m::{X as Y}` arrives as the literal "X as Y" — the producer row
        // is keyed by the ORIGINAL name X (the alias is consumer-local detail).
        let symbol_name = symbol_name
            .split_once(" as ")
            .map_or(symbol_name, |(orig, _alias)| orig.trim());
        // `use m::*` is module-level wiring, not a symbol: give it a dedicated
        // kind instead of polluting the 'unknown' (schema-degraded) bucket.
        if symbol_name == "*" {
            self.conn_ref().execute(
                "INSERT OR REPLACE INTO wiring_map
                 (module_file, symbol_name, symbol_kind, visibility, consumer_file, import_line, contract_source, resolved_at)
                 VALUES (?1, '*', 'glob_import', 'public', ?2, ?3, 'ast_read', datetime('now'))",
                params![canonical_module.as_ref(), canonical_consumer.as_ref(), import_line],
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
                ?3, ?4, 'ast_read', datetime('now'))",
            params![
                canonical_module.as_ref(),
                symbol_name,
                canonical_consumer.as_ref(),
                import_line
            ],
        )?;
        // Invalidate aggregate cache — consumer resolution changes wired_count.
        Self::invalidate_wiring_modules_cache();
        Ok(())
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
    pub fn backfill_unknown_consumer_kinds(&self) -> Result<usize, rusqlite::Error> {
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
    pub fn find_producer_modules_for_methods(
        &self,
        names: &[String],
        cap_per_name: usize,
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
        let ext_pred = wireable_ext_sql("module_file", polyglot_wiring_enabled());
        let sql = format!(
            "SELECT DISTINCT module_file, symbol_name FROM wiring_map
             WHERE consumer_file IS NULL
               AND visibility = 'public'
               AND symbol_kind IN ('method', 'function', 'async_function')
               AND {ext_pred}
               AND symbol_name IN ({placeholders})"
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
    /// producer — race condition; many `non_rust_rows` indicate a regression
    /// in the entry gate).
    ///
    /// Fields:
    /// - `total_rows`: every row in wiring_map (producer + consumer mixed).
    /// - `producer_rows`: rows with consumer_file IS NULL (one per pub symbol).
    /// - `consumer_rows`: rows with consumer_file IS NOT NULL (one per consumer edge).
    /// - `pub_producers`: producer rows with visibility='public'.
    /// - `distinct_pub_symbols`: deduped (module_file, symbol_name) producer count.
    /// - `kind_unknown_count`: rows with symbol_kind='unknown' (schema-degraded).
    /// - `non_rust_rows`: rows whose module_file does NOT end in `.rs`. Should
    ///   be 0 in the default (Rust-only) mode — a non-zero count there signals
    ///   a regression in the entry gate. Under `TOURING_POLYGLOT_WIRING=1`
    ///   (opt-in, [`polyglot_wiring_enabled`]) it is EXPECTED to be non-zero:
    ///   Python producer/consumer rows are first-party wiring, not pollution.
    pub fn wiring_db_diagnostic(&self) -> Result<WiringDbDiagnostic, rusqlite::Error> {
        let row = self.conn_ref().query_row(
            "SELECT
                COUNT(*) AS total_rows,
                SUM(CASE WHEN consumer_file IS NULL THEN 1 ELSE 0 END) AS producer_rows,
                SUM(CASE WHEN consumer_file IS NOT NULL THEN 1 ELSE 0 END) AS consumer_rows,
                SUM(CASE WHEN consumer_file IS NULL AND visibility = 'public' THEN 1 ELSE 0 END) AS pub_producers,
                COUNT(DISTINCT CASE WHEN consumer_file IS NULL AND visibility = 'public'
                                    THEN module_file || ':' || symbol_name END) AS distinct_pub_symbols,
                SUM(CASE WHEN symbol_kind = 'unknown' THEN 1 ELSE 0 END) AS kind_unknown_count,
                SUM(CASE WHEN module_file NOT LIKE '%.rs' THEN 1 ELSE 0 END) AS non_rust_rows
             FROM wiring_map",
            [],
            |row| {
                Ok(WiringDbDiagnostic {
                    total_rows: row.get::<_, i64>(0).unwrap_or(0),
                    producer_rows: row.get::<_, Option<i64>>(1)?.unwrap_or(0),
                    consumer_rows: row.get::<_, Option<i64>>(2)?.unwrap_or(0),
                    pub_producers: row.get::<_, Option<i64>>(3)?.unwrap_or(0),
                    distinct_pub_symbols: row.get::<_, i64>(4).unwrap_or(0),
                    kind_unknown_count: row.get::<_, Option<i64>>(5)?.unwrap_or(0),
                    non_rust_rows: row.get::<_, Option<i64>>(6)?.unwrap_or(0),
                })
            },
        )?;
        Ok(row)
    }

    /// One-shot migration that canonicalizes legacy absolute paths in
    /// wiring_map to workspace-relative form. Idempotent: safe to run on
    /// every daemon startup. Returns the number of rows updated.
    ///
    /// Without this migration, the same file recorded under both
    /// `crates/foo.rs` and `/home/.../crates/foo.rs` looks like two
    /// independent rows, causing every producer in the absolute form to
    /// appear orphan even when consumers under the relative form import it.
    pub fn migrate_canonicalize_paths(&self) -> Result<u64, rusqlite::Error> {
        let updated_modules = self.conn_ref().execute(
            "UPDATE OR IGNORE wiring_map
             SET module_file = SUBSTR(module_file, LENGTH(?1) + 1)
             WHERE module_file LIKE ?1 || '%'",
            params![workspace_root_marker()],
        )?;
        let updated_consumers = self.conn_ref().execute(
            "UPDATE OR IGNORE wiring_map
             SET consumer_file = SUBSTR(consumer_file, LENGTH(?1) + 1)
             WHERE consumer_file LIKE ?1 || '%'",
            params![workspace_root_marker()],
        )?;
        // Collisions (OR IGNORE above) — delete rows that could not be merged.
        let deleted = self.conn_ref().execute(
            "DELETE FROM wiring_map WHERE module_file LIKE ?1 || '%'",
            params![workspace_root_marker()],
        )?;
        if updated_modules + updated_consumers + deleted > 0 {
            Self::invalidate_wiring_modules_cache();
        }
        Ok((updated_modules + updated_consumers + deleted) as u64)
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
        let ext_pred = wireable_ext_sql("w.module_file", polyglot_wiring_enabled());
        let sql = format!(
            "SELECT w.module_file, w.symbol_name, w.symbol_kind, w.visibility,
                    w.consumer_file, w.import_line, w.contract_source
             FROM wiring_map w
             WHERE w.consumer_file IS NULL AND w.visibility = 'public'
               AND {ext_pred}
               AND w.module_file NOT LIKE 'benches/%'
               AND w.module_file NOT LIKE 'tests/%'
               AND w.module_file NOT LIKE 'docs/%'
               AND w.module_file NOT LIKE 'scripts/%'
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
        let canonical = canonicalize_module_path(module_file);
        let ext_pred = wireable_ext_sql("w.module_file", polyglot_wiring_enabled());
        let sql = format!(
            "SELECT w.module_file, w.symbol_name, w.symbol_kind, w.visibility,
                    w.consumer_file, w.import_line, w.contract_source
             FROM wiring_map w
             WHERE w.module_file = ?1 AND w.consumer_file IS NULL AND w.visibility = 'public'
               AND {ext_pred}
               AND w.module_file NOT LIKE 'benches/%'
               AND w.module_file NOT LIKE 'tests/%'
               AND w.module_file NOT LIKE 'docs/%'
               AND w.module_file NOT LIKE 'scripts/%'
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

    /// Invalidate the query cache entry for `cli_wiring_modules`.
    ///
    /// Called by mutators (`register_pub_symbol`, `record_consumer`, `clear_wiring`)
    /// so the next `cli_wiring_modules` call gets fresh data.
    pub fn invalidate_wiring_modules_cache() {
        touring_foundation::query_cache::invalidate(&touring_foundation::query_cache::make_key(
            "cli_wiring_modules",
            "v1",
        ));
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
            params![module_file],
        )?;
        // Wave 22 FASE 6 P0 fix: invalidate query cache so cli_wiring_modules
        // does not serve stale data. Doc at invalidate_wiring_modules_cache lists
        // this function as a caller, but the actual invocation was previously missing.
        Self::invalidate_wiring_modules_cache();
        Ok(())
    }

    /// Remove consumer entries for a specific file (used when file is re-scanned).
    pub fn clear_consumer_entries(&self, consumer_file: &str) -> Result<(), rusqlite::Error> {
        self.conn_ref().execute(
            "DELETE FROM wiring_map WHERE consumer_file = ?1",
            params![consumer_file],
        )?;
        // Wave 22 FASE 6 P0 fix: invalidate query cache. If caller re-scans and
        // returns early before `record_consumer` runs, without this invalidation
        // the cache would serve stale wired-count data until 60s TTL.
        Self::invalidate_wiring_modules_cache();
        Ok(())
    }
}

#[cfg(test)]
mod polyglot_gate_tests {
    use super::{
        is_go_package_wireable, is_indexable_module_file, is_indexable_module_file_polyglot,
        is_non_rust_non_wireable, wireable_ext_sql, wireable_extensions,
    };

    // ── Default mode (flag OFF) — byte-identical Rust-only behavior ─────────

    #[test]
    fn default_mode_is_rust_only() {
        // The public gate reads TOURING_POLYGLOT_WIRING, unset in this test
        // binary → OFF. Python/Markdown rejected; Rust accepted.
        assert!(is_indexable_module_file("crates/a/src/foo.rs"));
        assert!(!is_indexable_module_file("pkg/models.py"));
        assert!(!is_indexable_module_file("docs/plan.md"));
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
        assert!(is_non_rust_non_wireable("docs/x.py"));
        assert!(is_non_rust_non_wireable("scripts/x.py"));
        assert!(is_non_rust_non_wireable("a/site-packages/b.py"));
        assert!(is_non_rust_non_wireable("pkg/test_x.py"));
        // JS/TS.
        assert!(is_non_rust_non_wireable("web/node_modules/x.js"));
        assert!(is_non_rust_non_wireable("web/dist/bundle.js"));
        assert!(is_non_rust_non_wireable("web/foo.test.ts"));
        assert!(is_non_rust_non_wireable("web/foo.spec.js"));
        assert!(is_non_rust_non_wireable("web/__tests__/x.ts"));
        // Java.
        assert!(is_non_rust_non_wireable(
            "app/src/test/java/com/FooTest.java"
        ));
        assert!(is_non_rust_non_wireable("app/com/FooTests.java"));
        // First-party source is wireable.
        assert!(!is_non_rust_non_wireable("pkg/models.py"));
        assert!(!is_non_rust_non_wireable("web/src/service.ts"));
        assert!(!is_non_rust_non_wireable("src/main/java/com/foo/Bar.java"));
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
        let repaired = db.backfill_unknown_consumer_kinds().unwrap();
        assert_eq!(repaired, 1, "exactly the frozen row must be repaired");
        assert_eq!(kind_of(&db, "crates/a/src/engine.rs", "Engine"), "struct");
        // Idempotent: a second pass has nothing left to do.
        assert_eq!(db.backfill_unknown_consumer_kinds().unwrap(), 0);
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
        assert_eq!(db.backfill_unknown_consumer_kinds().unwrap(), 1);
        assert_eq!(
            kind_of(&db, "crates/a/src/test_util.rs", "assert_eq"),
            "extern"
        );
    }
}
