//! CLI index and AST handlers — extracted from cli_handlers.rs (lines 1778-2135)
//!
//! This module contains:
//! - **Index handlers** (5): cli_index_status, cli_index_search, cli_index_find,
//!   cli_index_files, cli_index_rebuild
//! - **AST handlers** (11): cli_ast_find, cli_ast_overview, cli_ast_blast,
//!   cli_ast_semantic, cli_ast_quality, cli_ast_calls, cli_ast_heat,
//!   cli_ast_modules, cli_ast_scope, cli_ast_detail, cli_ast_imports
//!
//! The 8 new handlers (semantic/quality/calls/heat/modules/scope/detail/imports)
//! provide deep AST-level analysis using the touring-ast crate APIs.

use crate::runtime::HookRuntime;
use rusqlite::params;
use std::path::Path;
use touring_analysis::e2e::schema_guard;
use touring_code::ast::{
    CallGraph, FileHeat, HeatMap, ImportResolver, Lang, QualityReport, ScopeMap, SymbolDetail,
    SymbolLocation, analyze_quality, build_call_graph, build_scope_map, extract_imports_resolved,
    extract_symbol_details, module_tree::ModuleTree, semantic_search::SemanticSymbolIndex,
    symbols::SymbolKind,
};

// ─────────────────────────────────────────────────────────────────────────────
// Index handlers (5)
// ─────────────────────────────────────────────────────────────────────────────

/// Um consumidor pendente do passe G3/W4: arquivo, nomes de método, refs de tipo
/// e os pares `(módulo, nome)` das chamadas qualificadas (D2, 02/09/2026).
///
/// Alias porque a tupla de quatro coleções é o que o passe realmente carrega, e
/// nomeá-la é mais barato que quatro vetores paralelos.
type PendingConsumer = (String, Vec<String>, Vec<String>, Vec<(String, String)>);

/// Um uso qualificado pendente do B5 (16/09/2026): arquivo consumidor, seu
/// caminho absoluto (o resolvedor de import precisa dele) e os pares
/// `(módulo, símbolo)` de cada `alias.Nome`.
type PendingQualifiedUse = (String, String, Vec<(String, String)>);

/// `cli-index-status` — returns symbol store health and statistics.
///
/// Wave 22 (S-Q4a): wrapped in `query_cache` with a global key — the
/// dashboard (`touring status`) calls this repeatedly in hot paths.
/// TTL is 60 s (global cache default); explicit invalidation happens in
/// `cli_index_rebuild` (S-Q4b) so post-rebuild calls always return fresh data.
pub fn cli_index_status(rt: &mut HookRuntime, _payload: &serde_json::Value) -> String {
    let cache_key =
        crate::shared::query_cache::make_key(&rt.project_root, "cli_index_status", "status");
    if let Some(cached) = crate::shared::query_cache::get(&cache_key) {
        return cached;
    }

    let (symbol_count, file_count) = if let Some(ref store) = rt.infra.symbol_store {
        match store.stats() {
            Ok(stats) => (stats.symbol_count, stats.file_count),
            Err(_) => (0, 0),
        }
    } else {
        (0, 0)
    };

    let out = index_status_json(
        rt.infra.symbol_store.is_some(),
        symbol_count,
        file_count,
        index_generation_json(rt),
    );
    crate::shared::query_cache::put(cache_key, out.clone());
    out
}

/// The `cli-index-status` payload — one shape for both sources of the answer.
fn index_status_json(
    initialized: bool,
    symbol_count: usize,
    file_count: usize,
    index_generation: serde_json::Value,
) -> String {
    serde_json::json!({
        "initialized": initialized,
        "symbol_count": symbol_count,
        "file_count": file_count,
        // I16: the seal of the latest rebuild — complete / building / partial / none.
        "index_generation": index_generation
    })
    .to_string()
}

/// `cli-index-status` answered from COMMITTED state on disk, outside the project
/// actor (decision 3-A, 14/09/2026).
///
/// The daemon routes the status here before it queues anything: a status call
/// timed out at 15 s during the sealing of an analise rebuild (09:08:32,
/// 14/09/2026), because the inferred-consumer transaction and the search
/// compaction hold the actor with no point to yield — and the monitoring session
/// read the failure as "no longer building". Both databases are opened
/// READ-ONLY: SQLite's WAL gives a reader the last committed snapshot without
/// waiting for the writer, and a read-write open would ask for an exclusive lock
/// on drop (measured deadlock, memory `sqlite-so-leitura-abre-readonly`). The
/// generation is judged with this process's pid, as the actor judges it, so a
/// rebuild in progress reads `building` until its commit.
///
/// `None` when either database is missing or unreadable: the caller falls back to
/// the actor, which creates them.
#[must_use]
pub fn index_status_from_disk(project_root: &Path) -> Option<String> {
    let cache_key =
        crate::shared::query_cache::make_key(project_root, "cli_index_status", "status");
    if let Some(cached) = crate::shared::query_cache::get(&cache_key) {
        return Some(cached);
    }
    let symbols_db = touring_foundation::TouringConfig::symbols_db_canonical(project_root);
    let knowledge_db = touring_foundation::TouringConfig::knowledge_db_canonical(project_root);
    if !symbols_db.is_file() || !knowledge_db.is_file() {
        return None;
    }
    let open = |path: &Path| -> Option<rusqlite::Connection> {
        let conn = rusqlite::Connection::open_with_flags(
            path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .ok()?;
        // A WAL reset can report BUSY for an instant; a status call waits that
        // instant rather than falling back to the queue it exists to avoid.
        conn.busy_timeout(std::time::Duration::from_secs(2)).ok()?;
        Some(conn)
    };
    let stats = touring_code::ast::store::store_stats(&open(&symbols_db)?).ok()?;
    // A generation that cannot be read falls back to the actor, which reports the
    // error; answering `unknown` with success hid it, and cached it for 60 s
    // (cross-audit 14/09/2026, A7).
    let reading = touring_hook_runtime::knowledge_index_generation::read_index_generation_state(
        &open(&knowledge_db)?,
        Some(std::process::id()),
    )
    .ok()?;
    // `building` is about to change: never cached. A reader that raced the
    // rebuild's final invalidation used to pin `building` for the cache's 60 s
    // after the seal (A6).
    let building = reading.state == "building";
    let generation = generation_value(Ok(reading));
    let out = index_status_json(true, stats.symbol_count, stats.file_count, generation);
    if !building {
        crate::shared::query_cache::put(cache_key, out.clone());
    }
    Some(out)
}

/// `cli-index-search` — prefix-search symbols in the store.
pub fn cli_index_search(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let query = payload.get("query").and_then(|v| v.as_str()).unwrap_or("");

    if query.is_empty() {
        return serde_json::json!({"results": [], "count": 0}).to_string();
    }

    // Wave 18: cache prefix lookup results for 60s — CC discovery
    // (`touring index search Foo`) often re-runs the same prefix.
    let cache_key =
        crate::shared::query_cache::make_key(&rt.project_root, "cli_index_search", query);
    if let Some(cached) = crate::shared::query_cache::get(&cache_key) {
        return cached;
    }

    let results: Vec<serde_json::Value> = if let Some(ref store) = rt.infra.symbol_store {
        store
            .search_symbols(query)
            .map(|locations| {
                locations
                    .into_iter()
                    .take(20)
                    .map(|loc| {
                        serde_json::json!({
                            "file_path": loc.file_path,
                            "symbol_name": loc.symbol_name,
                            "line": loc.line,
                            "column": loc.column,
                            "is_definition": loc.is_definition,
                            "kind": loc.kind
                        })
                    })
                    .collect()
            })
            .unwrap_or_default()
    } else {
        vec![]
    };

    let out = serde_json::json!({
        "query": query,
        "results": results,
        "count": results.len()
    })
    .to_string();
    crate::shared::query_cache::put(cache_key, out.clone());
    out
}

/// `cli-index-find` — exact-match symbol lookup.
///
/// Wave 17 (2026-04-18): wrapped in `query_cache::get_or_compute` so
/// repeated VGP verifications of the same symbol within a generation
/// pass return in ~1µs instead of round-tripping the symbol store
/// (~50–200µs). 60s TTL keeps results fresh while the daemon may
/// re-index.
pub fn cli_index_find(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let symbol_name = payload
        .get("symbol_name")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let definitions_only = payload
        .get("definitions_only")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    if symbol_name.is_empty() {
        return serde_json::json!({"definitions": [], "count": 0}).to_string();
    }

    let cache_key = crate::shared::query_cache::make_key(
        &rt.project_root,
        "cli_index_find",
        &format!("{symbol_name}|defs={definitions_only}"),
    );
    if let Some(cached) = crate::shared::query_cache::get(&cache_key) {
        return cached;
    }

    // A2/B: `definitions` (`@definition.*`) comes from `find_symbol` (which is
    // definition-only by construction — see store.rs); `references`
    // (`@reference.call`, kind="call") comes from the explicit `find_references`.
    // References are suppressed under `definitions_only` (VGP / collision
    // checks). `count` / `reference_count` are the true totals before output caps.
    fn map_loc(loc: SymbolLocation) -> serde_json::Value {
        serde_json::json!({
            "file_path": loc.file_path,
            "symbol_name": loc.symbol_name,
            "line": loc.line,
            "column": loc.column,
            "is_definition": loc.is_definition,
            "kind": loc.kind
        })
    }

    let store_opt = rt.infra.symbol_store.as_ref();

    let mut definitions: Vec<serde_json::Value> = store_opt
        .and_then(|s| s.find_symbol(symbol_name).ok())
        .unwrap_or_default()
        .into_iter()
        .map(map_loc)
        .collect();
    let def_count = definitions.len();
    definitions.truncate(10);

    let (mut references, symbol_store_ref_count): (Vec<serde_json::Value>, usize) =
        if definitions_only {
            (Vec::new(), 0)
        } else {
            let refs: Vec<serde_json::Value> = store_opt
                .and_then(|s| s.find_references(symbol_name).ok())
                .unwrap_or_default()
                .into_iter()
                .map(map_loc)
                .collect();
            let n = refs.len();
            (refs, n)
        };
    // Augment reference_count with wiring_map consumer count — the symbol store
    // call graph may undercount callers that are tracked only in the wiring layer
    // (cross-module imports). Take the max so the count never goes backwards.
    let wiring_consumer_count: usize = if definitions_only {
        0
    } else {
        rt.ctx
            .knowledge
            .conn_ref()
            .query_row(
                "SELECT COUNT(DISTINCT consumer_file) FROM wiring_map \
                 WHERE symbol_name = ?1 AND consumer_file IS NOT NULL",
                params![symbol_name],
                |row| row.get::<_, i64>(0),
            )
            .map(|n| n as usize)
            .unwrap_or(0)
    };
    let ref_count = symbol_store_ref_count.max(wiring_consumer_count);
    references.truncate(50);

    let mut out = serde_json::json!({
        "symbol_name": symbol_name,
        "definitions": definitions,
        "references": references,
        "count": def_count,
        "reference_count": ref_count
    });
    // I16: a lookup over a half-rebuilt index says so (the flag rides the 60 s
    // cache like the rest of the answer).
    flag_partial_index(rt, &mut out);
    let out = out.to_string();
    crate::shared::query_cache::put(cache_key, out.clone());
    out
}

/// `cli-index-files` — list indexed files (from knowledge DB top-accessed).
pub fn cli_index_files(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let pattern = payload
        .get("pattern")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    // Bug 1 fix (2026-05-02): honor payload.limit instead of hardcoded 50.
    // Default 100, max 10_000 to bound memory. Pool oversized for pattern filtering.
    let limit = payload
        .get("limit")
        .and_then(|v| v.as_u64())
        .map(|n| n.min(10_000) as usize)
        .unwrap_or(100);
    let pool_size = std::cmp::max(limit.saturating_mul(4), 1000);

    let files: Vec<String> = rt
        .ctx
        .knowledge
        .top_accessed_files(pool_size)
        .unwrap_or_default()
        .into_iter()
        .filter(|f| pattern.is_empty() || f.contains(pattern))
        .take(limit)
        .collect();

    serde_json::json!({
        "pattern": pattern,
        "limit": limit,
        "files": files,
        "count": files.len()
    })
    .to_string()
}

/// Bug 4 fix (2026-05-02): re-entrance guard for index rebuild.
/// Concurrent rebuild attempts are rejected with a structured error instead of
/// causing daemon-level lock contention / crash on large workspaces.
/// Canonical path of `path` when it resolves **inside** `root`; `None` otherwise.
///
/// Every recursive walker in the indexing path routes its descent decision through
/// this one predicate, which closes three failure modes that `Path::is_dir()` leaves
/// open (it calls `fs::metadata`, so it *resolves* symlinks):
///
/// 1. **Escape** — `ln -s /etc project/x` would otherwise be walked and its contents
///    indexed into the project's `symbols.db`, then surfaced by search. Comparing the
///    *canonical* path against the canonical root refuses that, and also covers `..`
///    traversal, not just symlinks.
/// 2. **Cycle** — `a/link -> a` recursed until the stack overflowed. Callers feed the
///    returned canonical path to a `visited` set, so the second encounter terminates.
/// 3. **Broken link** — `canonicalize` returns `Err`, so a dangling entry is skipped
///    instead of producing a read error mid-walk.
///
/// A symlink that stays **inside** the root is still followed: this removes the escape,
/// never the capability (REGRA #0).
///
/// `root` is canonicalized by the caller once, outside the recursion — canonicalizing
/// it per entry would be a syscall per file for a value that cannot change.
fn inside_root(path: &Path, root: &Path) -> Option<std::path::PathBuf> {
    let canonical = std::fs::canonicalize(path).ok()?;
    canonical.starts_with(root).then_some(canonical)
}

/// Tests that drive `cli_index_rebuild` in-process serialize on this lock:
/// `REBUILD_IN_PROGRESS` is process-wide, so two such tests racing on it make
/// the second read "rebuild already in progress" and fail on a `null` error
/// count (observed 13/09/2026 once the `index_why` e2e grew to two rebuilds).
#[cfg(test)]
pub(crate) static REBUILD_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

static REBUILD_IN_PROGRESS: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// P-H — emit Go package-aware wiring rows for one `.go` source file during a
/// rebuild, keyed by the file's package import-path (`go:<import-path>`) rather
/// than its `.go` file path. Producers are the file's exported top-level
/// declarations; consumers are `alias.Symbol` selectors resolved to the imported
/// package's key. Returns the number of producer rows registered (for the
/// `wiring_entries` tally).
///
/// The generic per-file loop keys Go producers by the `.go` path, which the
/// storage gate rejects (only `go:` keys wire for Go), and the Go arm of
/// `resolve_import_path_with_source` returns `None` — so without this pass Go
/// contributes nothing. This is the *package-aware* half that makes Go wire.
///
/// No-op for `_test.go` files (test code is not the package's public API) and
/// files with no derivable `go.mod` import-path. Inert unless
/// `TOURING_POLYGLOT_WIRING` is set — the storage gate rejects `go:` keys
/// otherwise — so default daemon behavior is byte-identical.
fn feed_go_package_wiring(
    rt: &HookRuntime,
    rel_path: &str,
    abs_path_str: &str,
    content: &str,
    cleared_pkgs: &mut std::collections::HashSet<String>,
) -> u32 {
    // Zero-overhead when the opt-in is off: skip extraction entirely (the
    // storage gate would reject every `go:` row anyway) so both DB state AND the
    // reported `wiring_entries` tally stay byte-identical to pre-polyglot runs.
    // The flag comes from the DATABASE being written (2026-08-19) — the same
    // answer the storage gate will give, for the project actually being
    // indexed, instead of a process-global that a per-project daemon inherits.
    if !rt.ctx.knowledge.polyglot() {
        return 0;
    }
    if rel_path.ends_with("_test.go") {
        return 0;
    }
    let Some(pkg_key) = touring_code::ast::go_wiring::go_package_key_for_file(abs_path_str) else {
        return 0;
    };
    // Clear the package's producer rows exactly once per pass. A per-file clear
    // would wipe exports a sibling `.go` file already registered under the same
    // package key; tracking cleared keys keeps a partial `--dir` reindex from
    // wiping packages it never re-walks. Consumer rows are keyed by the `.go`
    // consumer file and already cleared by the generic `clear_consumer_entries`.
    if cleared_pkgs.insert(pkg_key.clone()) {
        let _ = rt.ctx.knowledge.clear_wiring(&pkg_key);
    }
    let mut registered = 0u32;
    for export in touring_code::ast::go_wiring::extract_go_exports(content) {
        let _ = rt
            .ctx
            .knowledge
            .register_pub_symbol(&pkg_key, &export.name, export.kind, "public");
        registered += 1;
    }
    for edge in touring_code::ast::go_wiring::extract_go_consumer_edges(content) {
        let _ = rt
            .ctx
            .knowledge
            .record_consumer(&edge.package_key, &edge.symbol, rel_path, None);
    }
    registered
}

// I13/I16 (2026-09-13): the walker's rule tables, the size ceiling and the
// per-path verdict live in `touring_hooks_shared::index_policy` — ONE predicate
// for the rebuild walker, the rebuild's sweep, the hook writers
// (`reindex_file_with_old`) and `cli-index-why`. Re-exported so every historical
// path in this crate keeps resolving; the rule and its calibration are documented
// there.
// The rule TABLES are referenced through the module (`index_policy::…`) at their
// production sites: a bare cross-crate `use` of a const is invisible to the
// wiring index (it resolves cross-crate usage by inferred name only), and the
// four tables read as orphans of their own crate — measured 13/09/2026.
use touring_hooks_shared::index_policy;
#[cfg(test)]
pub(crate) use touring_hooks_shared::index_policy::MAX_INDEXABLE_FILE_BYTES;
pub(crate) use touring_hooks_shared::index_policy::{
    CandidateFacts, IndexVerdict, classify_index_candidate, dir_skip_reason,
    exceeds_index_size_ceiling, facts_from_metadata,
};

/// Definitions stored for `rel`. The store holds two spellings of the same file
/// (`crates/x.rs` and `./crates/x.rs`, measured 12/09/2026), so both are asked.
fn stored_definition_count(rt: &HookRuntime, rel: &str) -> usize {
    let Some(store) = rt.infra.symbol_store.as_ref() else {
        return 0;
    };
    let dotted = format!("./{rel}");
    [rel, dotted.as_str()]
        .iter()
        .map(|p| store.find_symbols_in_file(p).map(|v| v.len()).unwrap_or(0))
        .max()
        .unwrap_or(0)
}

/// `cli-index-why` — why a path is, or is not, in the index.
///
/// Payload: `{"path": "<repo-relative or absolute>"}`. The rebuild summary already
/// counts and samples the files it refused (`oversized_skipped`, `oversized_sample`),
/// but that answer is transient and capped at ten; this one is per file, on demand,
/// and names every exclusion the walker applies — so "my file is not indexed" never
/// ends in a silent "no definitions". I13 of the Graft analysis (2026-09-13).
pub fn cli_index_why(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let raw = payload.get("path").and_then(|v| v.as_str()).unwrap_or("");
    if raw.is_empty() {
        return serde_json::json!({ "error": "path required" }).to_string();
    }
    let root = rt.project_root.clone();
    let policy = index_policy::IndexPolicy::for_root(&root);
    // A stored key (`@companion/<name>/<rel>`) is answered where it is stored;
    // a path is answered where it lives. Both spellings resolve to one absolute
    // path and one storage key — the pair the store and the walker agree on.
    let abs = if raw.starts_with(index_policy::COMPANION_KEY_PREFIX)
        || std::path::Path::new(raw).is_absolute()
    {
        policy.path_for_key(raw)
    } else {
        root.join(raw)
    };
    // The key the store holds for this path: project-relative, the companion
    // key, or (outside every root) the absolute spelling — which the store never
    // holds, so `symbols` reads 0 and the verdict says why.
    let rel = policy
        .key_for(&abs)
        .unwrap_or_else(|| abs.to_string_lossy().into_owned());
    let bytes = std::fs::metadata(&abs).map(|m| m.len()).unwrap_or(0);
    // The SAME judgement the walker, the sweep and the hook writers apply
    // (`IndexPolicy::verdict_for_key`: an absolute key is judged by its
    // components UNDER the root or companion it lives in, or is `outside_root` —
    // measured 13/09: `why ~/.claude/rules/x.md` answered `skipped_dir` for the
    // `.claude` ancestor while the sweep said `outside_root` for the same key).
    // Readability is probed only after every cheaper rule passed: the size gate
    // exists precisely so a giant file is never pulled into memory — a
    // diagnostic must not do it either.
    let verdict = match policy.verdict_for_key(&rel) {
        Some(v) => Some(v),
        None if std::fs::read_to_string(&abs).is_err() => classify_index_candidate(
            &rel,
            CandidateFacts {
                readable: false,
                ..facts_from_metadata(&root, &abs)
            },
        ),
        None => None,
    };
    // The store is asked for EVERY verdict: rows for a path the walker refuses are
    // residue (a stale walk or a pre-policy hook write), and I13 says residue is
    // named, never hidden behind `symbols: 0` — measured 13/09/2026: `.venv/`
    // files answered `skipped_dir, symbols: 0` while the store held 30 rows each.
    let symbols = stored_definition_count(rt, &rel);
    let residue = verdict.is_some() && symbols > 0;
    let (status, mut detail) = match verdict {
        Some(v) => v,
        None if symbols > 0 => (
            IndexVerdict::Indexed,
            format!("{symbols} definition(s) in the symbol store"),
        ),
        None => (
            IndexVerdict::EligibleNotIndexed,
            "passes every walker rule but has no stored symbols — run `touring index ingest <path>` \
             or `touring index rebuild`"
                .to_string(),
        ),
    };
    if residue {
        detail.push_str(&format!(
            "; {symbols} stale definition(s) remain in the symbol store from an earlier walk or a \
             pre-policy hook write — `touring index rebuild` purges them"
        ));
    }
    // Bound outside the macro: a reference inside `json!` is invisible to the
    // wiring extractor, and the ceiling read as an orphan of its own crate.
    let max_indexable_file_bytes = index_policy::MAX_INDEXABLE_FILE_BYTES;
    let mut out = serde_json::json!({
        "path": rel,
        "status": status,
        "detail": detail,
        "bytes": bytes,
        "max_indexable_file_bytes": max_indexable_file_bytes,
        "symbols": symbols,
        "residue": residue,
    });
    flag_partial_index(rt, &mut out);
    out.to_string()
}

/// The latest generation as JSON for `cli-index-status` — never fails the status
/// call: a database that cannot be read reports `state: "unknown"` with the error.
fn index_generation_json(rt: &HookRuntime) -> serde_json::Value {
    generation_value(rt.ctx.knowledge.index_generation_state(std::process::id()))
}

/// A generation reading as JSON — shared by the actor and the read-only status.
fn generation_value(
    reading: Result<
        touring_hook_runtime::knowledge_index_generation::IndexGenerationState,
        rusqlite::Error,
    >,
) -> serde_json::Value {
    match reading {
        Ok(state) => serde_json::to_value(&state).unwrap_or(serde_json::Value::Null),
        Err(e) => serde_json::json!({ "state": "unknown", "detail": e.to_string() }),
    }
}

/// I16: stamp `index_state: "partial"` (+ `index_state_detail`, `remedy`) on a
/// query response when the latest rebuild did not complete — never silent, never
/// fatal. Adds nothing when the index is complete, so complete-state responses
/// keep their exact shape. Shared by find, the searches and `why`.
pub fn flag_partial_index(rt: &HookRuntime, out: &mut serde_json::Value) {
    if let Ok(state) = rt.ctx.knowledge.index_generation_state(std::process::id())
        && state.is_partial()
        && let Some(obj) = out.as_object_mut()
    {
        let remedy = state.remedy();
        obj.insert(
            "index_state".into(),
            serde_json::Value::String("partial".into()),
        );
        obj.insert(
            "index_state_detail".into(),
            serde_json::Value::String(state.detail),
        );
        obj.insert("remedy".into(), serde_json::Value::String(remedy.into()));
    }
}

/// `cli-index-rebuild` — walk the project root and symbol-index all supported files.
pub fn cli_index_rebuild(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    use std::sync::atomic::Ordering;
    // Bug 4: prevent concurrent rebuilds (would cause daemon crash on large workspaces).
    if REBUILD_IN_PROGRESS.swap(true, Ordering::SeqCst) {
        return serde_json::json!({
            "error": "rebuild already in progress",
            "hint": "wait for current rebuild to finish before retrying",
            "files_indexed": 0
        })
        .to_string();
    }
    // RAII drop guard so panic / early-return always clears the flag.
    struct ResetGuard;
    impl Drop for ResetGuard {
        fn drop(&mut self) {
            REBUILD_IN_PROGRESS.store(false, Ordering::SeqCst);
        }
    }
    let _reset = ResetGuard;

    let root = match payload.get("dir").and_then(|v| v.as_str()) {
        // A relative `--dir` (`.`, `crates/x`) is resolved against the project
        // root so every walked path strips to ONE relative spelling.
        Some(dir) if std::path::Path::new(dir).is_relative() => rt.project_root.join(dir),
        Some(dir) => std::path::PathBuf::from(dir),
        None => rt.project_root.clone(),
    };

    // A rebuild seals the PROJECT generation, so it may only walk inside the
    // project, and only a walk of the whole project may seal it. A `--dir`
    // outside the root walked nothing and sealed "complete, 0 files" over a real
    // index — three times per test run, from the WorktreeCreate handler spawning
    // `index rebuild --dir <tmp>` with the workspace as cwd (14/09/2026).
    let canonical_project =
        std::fs::canonicalize(&rt.project_root).unwrap_or_else(|_| rt.project_root.clone());
    let canonical_root = match std::fs::canonicalize(&root) {
        Ok(path) => path,
        Err(e) => {
            return serde_json::json!({
                "error": format!("rebuild dir {} cannot be read: {e}", root.display()),
                "error_kind": "dir_not_found",
                "dir": root.display().to_string(),
                "files_indexed": 0,
            })
            .to_string();
        }
    };
    if !canonical_root.starts_with(&canonical_project) {
        return serde_json::json!({
            "error": format!(
                "rebuild dir {} is outside the project root {} — run the rebuild from the \
                 project that owns it (its directory as cwd), or omit --dir",
                canonical_root.display(),
                canonical_project.display()
            ),
            "error_kind": "dir_outside_project",
            "dir": canonical_root.display().to_string(),
            "project_root": canonical_project.display().to_string(),
            "files_indexed": 0,
        })
        .to_string();
    }
    let scoped = canonical_root != canonical_project;
    // Walk the location that was validated, spelled under the project root: a
    // `--dir` symlink (`src/link -> real`) was checked by its canonical path but
    // walked through the link, so its files were stored a second time under the
    // link's spelling (cross-audit 14/09/2026, B7).
    let root = match canonical_root.strip_prefix(&canonical_project) {
        Ok(rel) => rt.project_root.join(rel),
        Err(_) => root,
    };

    // I16 (2026-09-13): open a generation BEFORE the first write. A daemon killed
    // mid-rebuild leaves this row `building` with a dead owner, and every reader
    // — status, find, the searches, doctor — reports `partial` instead of
    // answering from a mix of two walks (measured: `kill -9` at t+3 s left
    // wiring_map 85 rows short, integrity ok, and the next query said nothing).
    // A scoped walk opens none: it cannot vouch for the files it did not visit.
    let (generation_id, generation_error) = if scoped {
        (None, None)
    } else {
        match rt.ctx.knowledge.begin_index_generation(std::process::id()) {
            Ok(id) => (Some(id), None),
            Err(e) => {
                tracing::error!(
                    error = %e,
                    "index generation could not be opened; this rebuild runs unsealed"
                );
                (None, Some(e.to_string()))
            }
        }
    };
    // The status cache still holds the previous seal; a generation just opened is
    // news, and `index status` (served off the actor since decision 3-A) must say
    // `building` from the first file, not `complete` for another 60 s.
    crate::shared::query_cache::invalidate(&crate::shared::query_cache::make_key(
        &rt.project_root,
        "cli_index_status",
        "status",
    ));

    // The walker's three rule tables live at module level (2026-09-13) so that
    // `cli-index-why` answers "why is this file not indexed" with the SAME rules
    // that excluded it — never a second copy that can drift.
    const SUPPORTED_EXTS: &[&str] = index_policy::INDEX_SUPPORTED_EXTS;

    let mut files_indexed: u32 = 0;
    let mut symbols_added: u32 = 0;
    let mut wiring_entries: u32 = 0;
    // G3: pending (consumer_file, method-call names, type/const-ref names)
    // resolved against the COMPLETE wiring_map after the walk.
    let mut pending_consumers: Vec<PendingConsumer> = Vec::new();
    // B4 (15/09/2026): per Python file, the public symbols the file itself uses.
    // Written with the inferred edges after the walk, because the clear there
    // would wipe anything the walk had already recorded.
    let mut pending_self_refs: Vec<(String, std::collections::BTreeSet<String>)> = Vec::new();
    // B5 (16/09/2026): per Python file, `(abs path, [(module path, symbol)])` for
    // every `alias.Nome` whose alias came from an `import`. Resolved after the
    // walk, for the same reason the inferred edges are.
    let mut pending_python_qualified: Vec<PendingQualifiedUse> = Vec::new();
    let mut errors: u32 = 0;
    // Wave 2026-05-14 — root-cause fix for the "rebuild is additive only"
    // gotcha that forced manual SQL purges after every `rm -rf crates/X`.
    // `walked_rel_paths` captures every file successfully processed in the
    // main loop so the post-loop sweep can compute
    // `(db_files - walked_files) = stale`. See sweep block after the loop.
    let mut walked_rel_paths: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut stale_files_purged: u32 = 0;
    // I16: wiring transactions whose commit failed (their rows rolled back).
    let mut wiring_tx_failures: u32 = 0;
    // P-H (Go package-aware wiring): Go producers key by `go:<import-path>`, so
    // many `.go` files map to one package key. Producer rows must be cleared
    // once per package (not per file — a per-file clear of the shared key would
    // wipe the exports registered by a sibling file). This set tracks the
    // package keys already cleared in THIS pass, so a partial `--dir` reindex
    // only clears the packages it actually re-walks (never a global wipe).
    let mut cleared_go_pkgs: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut stale_paths_sample: Vec<String> = Vec::new();
    let project_root = rt.project_root.clone();
    let t0 = std::time::Instant::now();

    // `parents` carries the directory's own components so the rule can see its
    // context: `.claude/touring/` is refused while `.claude/skills/` is walked
    // (`index_policy::dir_skip_reason`, the same call `why` and the sweep make).
    /// The project's own exclusions for one walk.
    #[derive(Clone, Copy)]
    struct WalkRules<'a> {
        /// The declared `[index] exclude_dirs`, relative to the walk root; a
        /// companion walk passes none.
        excluded: &'a [String],
        /// The policy whose git ignore rules apply (`IndexPolicy::gitignored`); a
        /// companion root is not the project's working tree and passes none.
        git: Option<&'a index_policy::IndexPolicy>,
    }

    fn walk(
        dir: &std::path::Path,
        root: &std::path::Path,
        visited: &mut std::collections::HashSet<std::path::PathBuf>,
        parents: &mut Vec<String>,
        rules: WalkRules<'_>,
        supported_exts: &[&str],
        acc: &mut Vec<std::path::PathBuf>,
    ) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if path.is_dir() {
                let mut components: Vec<&str> = parents.iter().map(String::as_str).collect();
                components.push(name);
                if index_policy::excluded_dir_reason(rules.excluded, &components)
                    .or_else(|| dir_skip_reason(&components))
                    .or_else(|| rules.git.and_then(|policy| policy.gitignored(&path, true)))
                    .is_none()
                {
                    // Containment gate: `is_dir()` resolves symlinks, so a link
                    // pointing outside the project would otherwise be walked and
                    // indexed into the project DB. `inside_root` refuses that, and
                    // `visited` breaks `a/link -> a` cycles that recursed until the
                    // stack blew. Links that stay inside the root still descend —
                    // the guard removes the escape, not the capability.
                    if let Some(canonical) = inside_root(&path, root)
                        && visited.insert(canonical)
                    {
                        parents.push(name.to_string());
                        walk(&path, root, visited, parents, rules, supported_exts, acc);
                        parents.pop();
                    }
                }
            } else {
                let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                // A symlinked *file* escapes just as effectively as a directory —
                // `ln -s /etc/shadow x.rs` would land foreign bytes in symbols.db.
                if supported_exts.contains(&ext)
                    && inside_root(&path, root).is_some()
                    && rules
                        .git
                        .is_none_or(|policy| policy.gitignored(&path, false).is_none())
                {
                    acc.push(path);
                }
            }
        }
    }

    let mut paths: Vec<std::path::PathBuf> = Vec::new();
    // Canonicalize once: `inside_root` compares against this on every entry, and a
    // non-canonical root would make `starts_with` reject legitimate children whenever
    // any ancestor of the project is itself a symlink (a `/home -> /mnt/home` layout).
    let canonical_root = std::fs::canonicalize(&root).unwrap_or_else(|_| root.clone());
    let mut visited: std::collections::HashSet<std::path::PathBuf> =
        std::collections::HashSet::new();
    visited.insert(canonical_root.clone());
    let mut parents: Vec<String> = Vec::new();
    let policy = index_policy::IndexPolicy::for_root(&project_root);
    // A `--dir` subtree walk starts below the root: its components are relative
    // to the subtree, so the root-relative exclusions are re-based onto it.
    let subtree_excluded: Vec<String> = match root.strip_prefix(&project_root) {
        Ok(sub) if sub.as_os_str().is_empty() => policy.excluded_dirs().to_vec(),
        Ok(sub) => {
            let prefix = format!("{}/", sub.to_string_lossy());
            policy
                .excluded_dirs()
                .iter()
                .filter_map(|e| e.strip_prefix(prefix.as_str()).map(str::to_string))
                .collect()
        }
        Err(_) => Vec::new(),
    };
    walk(
        &root,
        &canonical_root,
        &mut visited,
        &mut parents,
        WalkRules {
            excluded: &subtree_excluded,
            git: Some(&policy),
        },
        SUPPORTED_EXTS,
        &mut paths,
    );

    // Companion roots (13/09/2026, "memórias e regras devem sim ser buscáveis"):
    // the global rules, skills, agents and commands, the auto-memory of this
    // project and of `~` — walked with the SAME rules, stored under the stable
    // key `@companion/<name>/<rel>` (`IndexPolicy`). Only a full rebuild walks
    // them; a `--dir` subtree rebuild is about the subtree.
    let mut companion_files: std::collections::BTreeMap<String, u32> =
        std::collections::BTreeMap::new();
    if root == project_root {
        for companion in policy.companions() {
            let canonical =
                std::fs::canonicalize(&companion.path).unwrap_or_else(|_| companion.path.clone());
            let mut visited_c: std::collections::HashSet<std::path::PathBuf> =
                std::collections::HashSet::new();
            visited_c.insert(canonical.clone());
            let mut parents_c: Vec<String> = Vec::new();
            let before = paths.len();
            walk(
                &companion.path,
                &canonical,
                &mut visited_c,
                &mut parents_c,
                WalkRules {
                    excluded: &[],
                    git: None,
                },
                SUPPORTED_EXTS,
                &mut paths,
            );
            companion_files.insert(
                companion.name.clone(),
                u32::try_from(paths.len().saturating_sub(before)).unwrap_or(u32::MAX),
            );
        }
    }

    // OOM-prevention gate (Wave 2026-05-12).
    //
    // Previously this loop processed every file in `paths` without checking
    // process RSS, which caused daemon SIGKILL by the kernel OOM-killer on
    // large workspaces (2517+ files / 58k+ symbols) when system swap was
    // already saturated. We now snapshot RSS every CHUNK_SIZE files and:
    //   * SOFT threshold → sleep 50 ms (let the allocator/kernel reclaim)
    //   * HARD threshold → break out gracefully, return partial result,
    //     bump `memory_pressure_red_count` so operators see the signal in
    //     `touring gate-metrics -j`.
    //
    // The chunked check is `chunk_idx % CHUNK_SIZE == 0` rather than a real
    // `paths.chunks()` envelope to avoid re-indenting the existing loop body.
    const CHUNK_SIZE: usize = 100;
    // Anonymous-memory budget: a quarter of the machine's RAM, 3000..=16000 MB,
    // unless the operator sets `TOURING_REBUILD_MEMORY_HARD_MB`; the soft line
    // keeps the old 2:3 ratio. The budget counts the WHOLE daemon, so the payload
    // reports what the daemon held before the walk (`baseline_anon_mb`).
    let memory_hard_mb = crate::shared::feature_flags::rebuild_memory_hard_mb();
    let memory_soft_mb = memory_hard_mb * 2.0 / 3.0;
    let baseline_anon_mb = crate::shared::memory_stats_probe::snapshot().anon_mb;
    let mut max_rss_mb: f64 = 0.0;
    let mut max_anon_mb: f64 = 0.0;
    let mut aborted_memory_pressure: bool = false;
    let mut oversized_skipped: u32 = 0;
    let mut oversized_sample: Vec<serde_json::Value> = Vec::new();

    // The search index follows the walk (2026-09-13): every walked file's
    // documents are replaced from the symbols just stored — names for code, text
    // for markdown (`shared::tantivy_docs`). Until this the rebuild never wrote
    // to tantivy, so `search` answered from the last manual `tantivy reindex`
    // while `index status` reported a sealed, complete generation.
    #[cfg(feature = "tantivy-fts")]
    let search_idx = crate::tantivy_index::tantivy_for(Some(&rt.project_root));
    #[cfg(feature = "tantivy-fts")]
    let (mut search_documents, mut search_refresh_failures) = (0u64, 0u32);
    #[cfg(feature = "tantivy-fts")]
    let write_search_docs = crate::shared::feature_flags::rebuild_search_docs();
    #[cfg(not(feature = "tantivy-fts"))]
    let (search_documents, search_refresh_failures) = (0u64, 0u32);
    for (chunk_idx, path) in paths.iter().enumerate() {
        // RSS probe at chunk boundaries (incl. the very first iteration).
        if chunk_idx % CHUNK_SIZE == 0 {
            // The budget is ANONYMOUS memory: resident size also counts the mapped
            // search-index segments the kernel reclaims first, and a walk over
            // 25.189 files crossed 2.972 MB resident with 2.332 MB anonymous
            // (measured 13/09/2026) — one merge away from aborting a healthy run.
            let mem = crate::shared::memory_stats_probe::snapshot();
            max_rss_mb = max_rss_mb.max(mem.physical_mb);
            max_anon_mb = max_anon_mb.max(mem.anon_mb);
            if mem.pressure_mb() > memory_hard_mb {
                crate::shared::gate_metrics::record_pressure_red();
                aborted_memory_pressure = true;
                break;
            }
            if mem.pressure_mb() > memory_soft_mb {
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
        }
        // Queued memory commands get a turn between files (A1, 14/09/2026): a
        // 15-minute analise rebuild held the actor while `memory store` from
        // another session outlived its 15 s budget. Nothing is open here — the
        // previous file's wiring transaction has committed and the search writer
        // is locked per write — so a served handler sees committed state and
        // waits on nothing this loop holds. Every file, not every chunk: with no
        // queued command the yield is one `try_recv`, and a chunk of large
        // markdown can take longer than the light budget.
        touring_hook_runtime::actor_yield::yield_now(rt);

        // `./crates/x.rs` and `crates/x.rs` are one file: a walk rooted at `.`
        // (`--dir .`) produced the dotted spelling and doubled the store — 4.483
        // files / 331.997 rows on 04/09/2026, measured 13/09. One spelling, always.
        // A companion file is stored under its key (`@companion/<name>/<rel>`),
        // a project file under its relative path — the policy owns both spellings.
        let rel_path = policy.key_for(path).unwrap_or_else(|| {
            crate::runtime::make_relative(path.to_str().unwrap_or(""), &project_root)
                .trim_start_matches("./")
                .to_string()
        });
        // The two daemon SIGSEGVs of 13-14/09/2026 died inside tree-sitter mid-rebuild
        // and no record said which file: a fatal signal now names it.
        let _crash_context = touring_hooks_core::panic_log::CrashContext::enter(&rel_path);
        // Size gate BEFORE the read: the read itself is the allocation that
        // starts the spike, so checking afterwards would be checking too late.
        if let Ok(meta) = std::fs::metadata(path)
            && exceeds_index_size_ceiling(meta.len())
        {
            oversized_skipped += 1;
            // Never a silent cap: the operator sees WHICH files were refused
            // and how big they were, so "my file is not indexed" has an answer.
            if oversized_sample.len() < 10 {
                oversized_sample.push(serde_json::json!({
                    "path": rel_path.clone(),
                    "bytes": meta.len(),
                }));
            }
            continue;
        }
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => {
                errors += 1;
                continue;
            }
        };
        match rt.process_file(&rel_path, &content) {
            Ok(result) => {
                let sym_count = result.symbols_added.len() as u32;
                if let Some(store) = rt.symbol_store() {
                    // B (2026-06-21): polyglot `@reference.call`. Persists
                    // call-sites as `is_definition=false` rows (`kind="call"`)
                    // so `index find` doubles as find-references (GitHub-semantic
                    // tags model). DEFAULT ON (Gabriel 2026-06-21) — disable
                    // per-project with `TOURING_INDEX_REFERENCES=0`. Cost measured
                    // on the touring workspace: ~2.2× rows / +61% DB / +12%
                    // reindex. Safe by construction: references are invisible to
                    // every definition query (`find_symbol`/`search_symbols`/
                    // `load_into_index` filter `is_definition=1`); only the
                    // explicit `find_references` returns them. `build_call_graph`
                    // is polyglot (Rust/Python/TS/JS); other langs yield no sites.
                    if std::env::var("TOURING_INDEX_REFERENCES")
                        .map(|v| v != "0" && v != "false")
                        .unwrap_or(true)
                    {
                        let mut combined = result.symbols_added.clone();
                        if let Some(lang) = Lang::from_path(Path::new(&rel_path)) {
                            for cs in build_call_graph(&content, lang).sites {
                                combined.push(
                                    SymbolLocation::new(
                                        rel_path.as_str(),
                                        cs.callee,
                                        cs.line,
                                        0,
                                        false,
                                    )
                                    .with_kind(Some("call".to_string())),
                                );
                            }
                        }
                        let _ = store.replace_file_symbols(&rel_path, &combined);
                    } else {
                        let _ = store.replace_file_symbols(&rel_path, &result.symbols_added);
                    }
                }
                files_indexed += 1;
                symbols_added += sym_count;
                // Wave 2026-05-14: record this path so the post-loop sweep
                // can tell it apart from files that vanished from disk.
                walked_rel_paths.insert(rel_path.clone());

                #[cfg(feature = "tantivy-fts")]
                if write_search_docs && let Some(idx) = search_idx {
                    let definitions: Vec<SymbolLocation> = result
                        .symbols_added
                        .iter()
                        .filter(|s| s.is_definition)
                        .cloned()
                        .collect();
                    let docs = crate::shared::tantivy_docs::docs_for_file(
                        &rel_path,
                        &definitions,
                        Some(&content),
                    );
                    match crate::shared::tantivy_docs::refresh_file(idx, &rel_path, &docs) {
                        Ok(n) => search_documents += n as u64,
                        Err(e) => {
                            search_refresh_failures += 1;
                            tracing::debug!(file = %rel_path, error = %e, "search documents not refreshed");
                        }
                    }
                }

                let abs_path_str = path.to_str().unwrap_or("");
                if let Some(symbols) =
                    crate::ast_bridge::extract_enriched_symbols(&content, abs_path_str)
                {
                    let language =
                        crate::shared::detect_language::detect_language_or_unknown(&rel_path);
                    // A companion file is searchable, never wiring (decision 1-A,
                    // 14/09/2026): its rows are only cleared below, so the residue a
                    // walk before the decision left goes away with this one.
                    let is_code_lang =
                        touring_hook_runtime::wiring::declares_wireable_api(language)
                            && !touring_foundation::config::is_companion_key(&rel_path);

                    // I16 (2026-09-13): this file's wiring rows are replaced ATOMICALLY —
                    // clear + re-register + consumers in ONE transaction. Measured on the
                    // per-project copy: `kill -9` between the clear and the re-register
                    // left wiring_map 85 rows short with integrity `ok`. Statement-per-row
                    // autocommit was also the slower path (one fsync each). An open
                    // failure is reported and the rows fall back to autocommit.
                    let wiring_tx = rt.ctx.knowledge.conn_ref().unchecked_transaction();
                    if let Err(e) = &wiring_tx {
                        tracing::warn!(
                            file = %rel_path,
                            error = %e,
                            "wiring transaction could not open; rows fall back to autocommit"
                        );
                    }
                    // ALWAYS clear old wiring entries so we start fresh for this file.
                    // This prevents stale entries from accumulating across rebuilds.
                    // The INFERRED consumer edges are the exception (I16): they can
                    // only be re-derived after every producer row exists, so they
                    // stay until the post-walk transaction replaces them — clearing
                    // them here left 91 edges missing under a mid-walk `kill -9`.
                    let _ = rt.ctx.knowledge.clear_wiring(&rel_path);
                    let _ = rt.ctx.knowledge.clear_declared_consumer_entries(&rel_path);
                    // S1 (2026-08-07): a re-scan of this file re-derives every
                    // unresolved import below, so the previous run's failures
                    // are dropped first. Without this the resolver-debt count
                    // could only grow, and a number that cannot improve is a
                    // number nobody acts on. Outside the code branch, so a file
                    // that stopped being wiring loses its old failures too.
                    let _ = rt.ctx.knowledge.clear_unresolved_for_consumer(&rel_path);

                    if is_code_lang {
                        // Real visibility, one predicate for the rebuild and the edit
                        // path: `pub(crate)` is stored as `crate`, so the orphan queries
                        // (`visibility = 'public'`) never count crate-internal helpers
                        // as public API (verified 07/08/2026 on `auto_remediation`).
                        wiring_entries += touring_hook_runtime::wiring::register_public_symbols(
                            &rt.ctx.knowledge,
                            &rel_path,
                            &symbols,
                        );

                        // B4 (15/09/2026): Python has no inferred consumer pass —
                        // the three extractors below are Rust-only — so a constant
                        // read by its own module and a constant nobody reads were
                        // the same row: an orphan. Collected here, written with the
                        // other inferred edges after the walk.
                        // B4 (Python, 15/09) and D9 (Rust, 18/09): the file's use of
                        // its own public symbols — `internal_only`, never an orphan.
                        if language == "rust"
                            || (language == "python" && rt.ctx.knowledge.polyglot())
                        {
                            let names = touring_code::ast::graph::self_referenced_names(
                                &content, &symbols, language,
                            );
                            if !names.is_empty() {
                                pending_self_refs.push((rel_path.clone(), names));
                            }
                        }
                        if language == "python" && rt.ctx.knowledge.polyglot() {
                            // B5 (16/09/2026): `import modulo as gm` + `gm.Nome`.
                            // A bare `import` carries no symbol, so the import pass
                            // below wrote nothing and every symbol reached through
                            // the module object read as an orphan.
                            let qualified =
                                touring_code::ast::graph::python_qualified_uses(&content);
                            if !qualified.is_empty() {
                                pending_python_qualified.push((
                                    rel_path.clone(),
                                    abs_path_str.to_string(),
                                    qualified,
                                ));
                            }
                        }

                        // The import edges of this file: resolved, credited to the
                        // module that DEFINES each symbol (a façade is never
                        // credited with a consumer it only forwards — 08/08/2026,
                        // 960 rows), and every failure recorded with its class.
                        // ONE pass with the hook path (C08, 18/09/2026): the edit
                        // and ingest paths call the same function, so a Python or
                        // TypeScript file no longer loses its edges when edited.
                        touring_hook_runtime::wiring::record_import_consumers(
                            &rt.ctx.knowledge,
                            &rel_path,
                            abs_path_str,
                            language,
                            &content,
                        );

                        // G3 (2026-08-12, cross-audit): dispatch references are
                        // COLLECTED here and RESOLVED after the walk. The old F9
                        // resolved inline — a file processed before its producer
                        // (alphabetical walk: hook-runtime < intelligence) found
                        // an empty wiring_map and recorded nothing, which is why
                        // `tags::derive_tags` consumers read as false orphans.
                        // Also collects type/const refs (`tags::TAG_TABLES_DDL`,
                        // `&ParsedTag`) that no call-expression pass can see.
                        let method_names =
                            crate::ast_bridge::extract_file_method_calls(&content, abs_path_str);
                        let type_refs = touring_code::ast::Lang::from_path(Path::new(&rel_path))
                            .map(|lang| {
                                touring_code::ast::graph::extract_type_and_const_refs(
                                    &content, lang,
                                )
                            })
                            .unwrap_or_default();
                        // D2 (2026-09-02): pares (módulo, nome) das chamadas
                        // qualificadas — `super::backup::run()` guarda `backup`,
                        // e é o qualificador que torna o nome decidível.
                        let qualified_calls =
                            touring_code::ast::Lang::from_path(Path::new(&rel_path))
                                .map(|lang| {
                                    touring_code::ast::graph::extract_qualified_calls(
                                        &content, lang,
                                    )
                                })
                                .unwrap_or_default();
                        if !method_names.is_empty()
                            || !type_refs.is_empty()
                            || !qualified_calls.is_empty()
                        {
                            pending_consumers.push((
                                rel_path.clone(),
                                method_names,
                                type_refs,
                                qualified_calls,
                            ));
                        }

                        // P-H: Go package-aware wiring. Go producers key by the
                        // package import-path (`go:<path>`), not the `.go` file
                        // (which the generic loop above registered — and the
                        // storage gate rejected), and consumers key by selector
                        // (`pkg.Foo()`), not `use`-import. Inert unless
                        // `TOURING_POLYGLOT_WIRING` is set (gate rejects `go:`).
                        if language == "go" {
                            wiring_entries += feed_go_package_wiring(
                                rt,
                                &rel_path,
                                abs_path_str,
                                &content,
                                &mut cleared_go_pkgs,
                            );
                        }
                    }
                    // I16: the file's wiring rows land together or not at all.
                    if let Ok(tx) = wiring_tx
                        && let Err(e) = tx.commit()
                    {
                        wiring_tx_failures += 1;
                        tracing::warn!(
                            file = %rel_path,
                            error = %e,
                            "wiring transaction commit failed \u{2014} this file's wiring rows were rolled back"
                        );
                    }
                }
            }
            Err(_) => {
                errors += 1;
            }
        }
    }

    // ── G3 (2026-08-12): resolve pending dispatch consumers —──────────────
    // The walk is over, so every producer row now exists regardless of file
    // order. Match collected names against the complete wiring_map (cap 4 per
    // name, origin AstInferred — the same lossy-guess provenance as before).
    // W4 (2026-09-02): the SAME inference step the hook path runs after an
    // edit (`record_inferred_consumers`) — one method, two call sites, no
    // C08 asymmetry between a rebuilt file and an edited one.
    // I16: the whole inferred-consumer pass is one transaction — the same
    // atomicity the per-file wiring writes have, and far fewer fsyncs.
    // Post-walk boundary with no open transaction: queued memory commands and
    // `index status` get a turn (an analise status call timed out while this
    // phase ran, 14/09/2026). The transaction and the search compaction below
    // cannot yield inside; their boundaries can.
    touring_hook_runtime::actor_yield::yield_now(rt);
    // Scoped so the transaction's borrow of `rt` ends with it: the yield after
    // the commit needs `rt` again.
    {
        let inferred_tx = rt.ctx.knowledge.conn_ref().unchecked_transaction().ok();
        // The inferred edges of every walked file are replaced HERE, not in the walk:
        // clear (stale ones of files that no longer call anything included) and
        // re-derive inside the same transaction, so a kill leaves the old edges or
        // the new ones — never a file with none.
        for walked in &walked_rel_paths {
            let _ = rt.ctx.knowledge.clear_inferred_consumer_entries(walked);
        }
        for (consumer_file, method_names, type_refs, qualified_calls) in &pending_consumers {
            let _ = rt.ctx.knowledge.record_inferred_consumers(
                consumer_file,
                method_names,
                type_refs,
                // D2 (2026-09-02) — pares (módulo, nome) das chamadas
                // qualificadas: resolvem para UM produtor onde o nome
                // nu enfrenta 130 homônimos.
                qualified_calls,
            );
        }
        // B4/D9: a file's use of its own public symbols. The edge is
        // `(file, symbol) -> file` and its tier is the same as any bare-name
        // match: it never means the project uses the symbol — that is what
        // `internal_only_symbols` reports — only that the file does.
        for (consumer_file, names) in &pending_self_refs {
            for name in names {
                let _ = rt.ctx.knowledge.record_consumer_with_origin(
                    consumer_file,
                    name,
                    consumer_file,
                    None,
                    touring_hook_runtime::knowledge_wiring::WiringOrigin::AstInferred,
                );
            }
        }
        // B5: qualified Python use. The module path resolves exactly like an
        // import's does; the symbol is the attribute name, so the edge is a guess
        // by name and carries the same `ast_inferred` tier as the others here.
        for (consumer_file, abs_path, uses) in &pending_python_qualified {
            for (module_path, symbol) in uses {
                if let Some(module_file) =
                    touring_hooks_core::symbol_extractors::resolve_import_path_with_source(
                        module_path,
                        "python",
                        Some(abs_path),
                    )
                {
                    let _ = rt.ctx.knowledge.record_consumer_with_origin(
                        &module_file,
                        symbol,
                        consumer_file,
                        None,
                        touring_hook_runtime::knowledge_wiring::WiringOrigin::AstInferred,
                    );
                }
            }
        }
        if let Some(tx) = inferred_tx
            && let Err(e) = tx.commit()
        {
            wiring_tx_failures += 1;
            tracing::warn!(error = %e, "inferred-consumer transaction commit failed \u{2014} rolled back");
        }
    }
    touring_hook_runtime::actor_yield::yield_now(rt);

    // ── Wave 2026-05-14: sweep stale entries ───────────────────────────
    // Historically `cli_index_rebuild` was ADDITIVE-only: it called
    // `replace_file_symbols` for every walked file, but never deleted
    // entries for files that had been removed from disk (e.g. after a
    // `rm -rf crates/foo` refactor). The result was an ever-growing
    // pool of stale symbols + wiring rows that produced phantom orphans,
    // ghost-cycles, and incorrect cross-caller analyses — forcing
    // operators to issue manual `DELETE FROM symbols WHERE file_path
    // LIKE ...` against `symbols.db` after every absorption wave.
    //
    // This sweep closes the loop: for every file_path in the DB that
    // is NOT in `walked_rel_paths` AND whose absolute path is missing
    // on disk, we purge symbols + wiring + consumer + tantivy rows.
    //
    // SAFETY: skip the sweep when the walk aborted early due to memory
    // pressure (`aborted_memory_pressure`). Without a complete walk,
    // `walked_rel_paths` is a strict subset of "should exist", and we
    // would catastrophically delete legitimate entries.
    //
    // DEFENSE-IN-DEPTH: even when not aborted, we double-check the
    // absolute path with `Path::exists()` before purging — covers the
    // case where a single file failed to be read but is still on disk.
    //
    // I16 step zero (2026-09-13): the sweep retires every row the WALKER would
    // refuse, not only paths gone from disk. "Not walked AND missing" was the only
    // rule, so rows for files that exist but are excluded by policy (956 files
    // under `.venv/`, 306 under `.claude/`, 140 outside the root — measured live)
    // survived five months of rebuilds and outranked live code in BM25. Same
    // predicate as the walker and `touring index why`; a row whose file is
    // eligible but simply was not walked this pass (a read error) is kept.
    let mut policy_purged_by_reason: std::collections::BTreeMap<&'static str, u32> =
        std::collections::BTreeMap::new();
    let mut policy_purged_sample: Vec<serde_json::Value> = Vec::new();
    let mut consumers_purged: u32 = 0;
    // Owned list: the store borrow must end before the per-file yield below.
    let db_files = if aborted_memory_pressure {
        None
    } else {
        rt.symbol_store()
            .and_then(|store| store.get_indexed_files(usize::MAX).ok())
    };
    if let Some(db_files) = db_files {
        for db_path in &db_files {
            if walked_rel_paths.contains(db_path) {
                continue;
            }
            // `./crates/x.rs` is the same file as `crates/x.rs` (both spellings
            // were measured in one store); the walked spelling wins.
            let dotted = db_path.trim_start_matches("./");
            let reason: &'static str = if dotted != db_path && walked_rel_paths.contains(dotted) {
                "duplicate_spelling"
            } else {
                // The rebuild's own policy (configuration read ONCE, not per
                // row): a companion key whose root was un-configured since the
                // last walk is `companion_not_configured` and goes with the rest.
                match policy.verdict_for_key(db_path) {
                    Some((verdict, _)) => verdict.as_str(),
                    None => continue,
                }
            };
            // A queued light command gets a turn before each purge (A5,
            // 14/09/2026): the sweep retired 5.867 rows in one live rebuild, and
            // between two files nothing is open — each purge below commits alone.
            touring_hook_runtime::actor_yield::yield_now(rt);
            if let Some(store) = rt.symbol_store() {
                let _ = store.remove_file(db_path);
            }
            let _ = rt.ctx.knowledge.clear_wiring(db_path);
            let _ = rt.ctx.knowledge.clear_consumer_entries(db_path);
            #[cfg(feature = "tantivy-fts")]
            if let Some(idx) = search_idx {
                let _ = idx.delete_by_file(db_path);
            }
            if stale_paths_sample.len() < 5 {
                stale_paths_sample.push(db_path.clone());
            }
            if policy_purged_sample.len() < 10 {
                policy_purged_sample.push(serde_json::json!({ "path": db_path, "reason": reason }));
            }
            *policy_purged_by_reason.entry(reason).or_insert(0) += 1;
            stale_files_purged += 1;
        }
    }
    // Consumer rows of files that hold no symbols (cross-audit 14/09/2026,
    // R2-6). The sweep above starts from the symbol store, so a consumer the
    // walker refuses or that left the disk was never judged: 342 edges from
    // May, 17 of their 23 consumers gone, kept 72 producers wired. Same policy
    // and the same rule — only a refusal purges; an eligible file is kept.
    if !aborted_memory_pressure && let Ok(consumers) = rt.ctx.knowledge.consumer_files() {
        for consumer in consumers {
            if walked_rel_paths.contains(&consumer) {
                continue;
            }
            let Some((verdict, _)) = policy.verdict_for_key(&consumer) else {
                continue;
            };
            touring_hook_runtime::actor_yield::yield_now(rt);
            let _ = rt.ctx.knowledge.clear_consumer_entries(&consumer);
            if policy_purged_sample.len() < 10 {
                policy_purged_sample.push(serde_json::json!({
                    "path": consumer,
                    "reason": verdict.as_str(),
                    "consumer_only": true,
                }));
            }
            *policy_purged_by_reason.entry(verdict.as_str()).or_insert(0) += 1;
            consumers_purged += 1;
        }
    }
    // A tantivy write reaches the index only at COMMIT. During the walk a batch
    // commit may publish finished files (never half of one, A10); this commit
    // publishes the rest of the walk and the sweep's deletions together, whether
    // or not the sweep ran. Without it the 5.867 paths the live sweep purged on 13/09 stayed
    // searchable: BM25's top-3 was still pip's vendored `sessions.py` after the
    // rebuild that had removed its rows from symbols.db.
    // Then one merge: BM25 statistics count deleted documents until their segment
    // is merged, so a rebuild over a live index ranked differently from a fresh
    // one (measured 13/09/2026). Compacting makes the ranking depend on the
    // documents alone.
    #[cfg(feature = "tantivy-fts")]
    let (mut search_segments_merged, mut search_compact_error) = (0usize, None::<String>);
    #[cfg(not(feature = "tantivy-fts"))]
    let (search_segments_merged, search_compact_error) = (0usize, None::<String>);
    touring_hook_runtime::actor_yield::yield_now(rt);
    #[cfg(feature = "tantivy-fts")]
    if (search_documents > 0 || stale_files_purged > 0)
        && let Some(idx) = search_idx
    {
        match idx.commit().and_then(|()| idx.compact()) {
            Ok(merged) => search_segments_merged = merged,
            Err(e) => {
                search_refresh_failures += 1;
                search_compact_error = Some(e.to_string());
                tracing::warn!(
                    error = %e,
                    "tantivy commit/compact after the rebuild failed; search keeps the previous generation until the next commit"
                );
            }
        }
    }

    // 2026-08-07: the sweep above is driven by the SYMBOLS table, so it can only
    // retire a path some symbol row once carried. `wiring_map` accepts a
    // `module_file` from the IMPORT RESOLVER, which for years could name a file
    // that never existed (a facade import resolved to
    // `crates/touring-hooks/src/tantivy_index.rs`, a module that lives in
    // touring-hooks-core). Those keys are invisible to `get_indexed_files`, so
    // they accumulated: 301 of 1711 distinct values, 17.6%.
    //
    // The damage is not merely a dead row. A consumer edge parked on a phantom
    // key leaves the REAL producer at `consumer_file IS NULL`, i.e. reported as
    // an orphan while its consumers are on record — against a file that is not
    // there. Retire them on the same full-walk condition the symbol sweep uses.
    let mut phantom_modules_purged: u32 = 0;
    if !aborted_memory_pressure && let Ok(modules) = rt.ctx.knowledge.distinct_module_files() {
        for module_file in &modules {
            // Only file-keyed rows under this project: `go:<import-path>` package
            // keys and `touring-daemon://…` pseudo-consumers are not paths and
            // must survive.
            if module_file.contains("://") || module_file.starts_with("go:") {
                continue;
            }
            let abs = project_root.join(module_file);
            if abs.exists() {
                continue;
            }
            // Same boundary as the symbol sweep: one module's rows at a time.
            touring_hook_runtime::actor_yield::yield_now(rt);
            if let Ok(n) = rt.ctx.knowledge.purge_module_rows(module_file)
                && n > 0
            {
                phantom_modules_purged += 1;
            }
        }
    }

    // Wave H+1 (2026-06-11): re-resolve consumer rows frozen at
    // symbol_kind='unknown' — walk-order races (consumer indexed before its
    // producer) and facade re-export imports both leave recoverable rows;
    // this closes the doctor wiring_diagnostic pollution warning.
    // 2026-08-19: this used to be `.unwrap_or(0)`. A failure here leaves the
    // wiring_map permanently polluted with `kind_unknown` rows and NOTHING
    // records it — the rebuild still answers success. Observed on `analise`:
    // 3.923 unknown rows survived because this pass never ran, and the only
    // visible symptom was a doctor warning with no way back to the cause.
    // The repair is cheap (9.6s over 39k rows, measured) and total (pass 1
    // inherits the producer kind, pass 2 marks the rest `extern`), so a
    // non-zero `kind_unknown` after a complete rebuild means THIS failed.
    //
    // I16 (2026-09-13): ONE deterministic pass resolves every consumer kind from
    // the producers as they stand after the walk (same module first, then the
    // homonym that sorts first, then `extern` over a complete walk). It replaces
    // the unknown-only backfill whose `LIMIT 1` without ORDER BY — and the
    // record-time copy from rows of the PREVIOUS index — made a cold rebuild
    // differ from a warm one in 24 wiring lines (measured on the isolated copy).
    // The payload key keeps its name; it now counts consumer rows whose kind
    // changed in this pass, so a warm rebuild over an unchanged tree reports 0.
    let (kinds_backfilled, backfill_error) = match rt
        .ctx
        .knowledge
        .resolve_consumer_kinds_from_producers(!aborted_memory_pressure)
    {
        Ok(n) => {
            if n > 0 {
                tracing::info!(
                    kinds_resolved = n,
                    "wiring_map consumer kinds resolved from producers"
                );
            }
            (n, None)
        }
        Err(e) => {
            tracing::error!(
                error = %e,
                "wiring_map consumer-kind resolution FAILED \u{2014} kinds may mix two walks after this rebuild"
            );
            (0, Some(e.to_string()))
        }
    };

    let abort_hint = if aborted_memory_pressure {
        Some(format!(
            "daemon anonymous memory {} MB (resident {} MB, {} MB held before the walk) exceeded \
             the {} MB hard threshold; rebuild stopped at {}/{} files. Hint: raise \
             `TOURING_REBUILD_MEMORY_HARD_MB` in the daemon's environment and restart it \
             (`touring daemon-ctl restart`), free system memory, or rebuild incrementally via \
             `touring index ingest <file>`.",
            max_anon_mb as u64,
            max_rss_mb as u64,
            baseline_anon_mb as u64,
            memory_hard_mb as u64,
            files_indexed,
            paths.len()
        ))
    } else {
        None
    };

    // I16: seal the generation. `complete` only over a full walk; an aborted walk
    // is sealed `aborted`, which every reader reports as `partial`.
    let generation = match generation_id {
        Some(id) => match rt.ctx.knowledge.finish_index_generation(
            id,
            u64::from(files_indexed),
            u64::from(symbols_added),
            !aborted_memory_pressure,
            abort_hint.as_deref(),
        ) {
            Ok(()) => serde_json::json!({
                "id": id,
                "state": if aborted_memory_pressure { "aborted" } else { "complete" }
            }),
            Err(e) => serde_json::json!({ "id": id, "state": "unsealed", "error": e.to_string() }),
        },
        None if scoped => serde_json::json!({
            "state": "scoped",
            "reason": "a walk of one directory never seals the project generation",
            "dir": canonical_root.display().to_string(),
        }),
        None => serde_json::json!({ "state": "unsealed", "error": generation_error }),
    };
    // Invalidated AFTER the seal (A6, cross-audit 14/09/2026): invalidating before
    // it let a concurrent off-actor status re-cache the pre-seal state.
    crate::shared::query_cache::invalidate(&crate::shared::query_cache::make_key(
        &rt.project_root,
        "cli_index_status",
        "status",
    ));

    let max_indexable_file_bytes = index_policy::MAX_INDEXABLE_FILE_BYTES;
    serde_json::json!({
        "files_indexed": files_indexed,
        "symbols_added": symbols_added,
        "wiring_entries": wiring_entries,
        "errors": errors,
        "total_files_scanned": paths.len(),
        "duration_ms": t0.elapsed().as_millis(),
        // Wave 2026-05-12: OOM-prevention telemetry.
        "max_rss_mb": max_rss_mb as u64,
        "max_anon_mb": max_anon_mb as u64,
        "baseline_anon_mb": baseline_anon_mb as u64,
        "memory_hard_mb": memory_hard_mb as u64,
        "aborted_memory_pressure": aborted_memory_pressure,
        "abort_hint": abort_hint,
        // Wave 2026-05-14: sweep telemetry — catches `rm -rf crates/X`
        // regressions automatically. Operators can grep this field in
        // CI to assert the workspace is "clean" after a refactor.
        "stale_files_purged": stale_files_purged,
        "stale_paths_sample": stale_paths_sample,
        // 2026-08-07: wiring keys naming files that are not on disk. Separate
        // from `stale_files_purged` because the two find different things — that
        // one follows the symbols table, this one the wiring keys.
        "phantom_modules_purged": phantom_modules_purged,
        // 2026-08-19: files refused by the size ceiling. Reported, never
        // silent — a bounded scan that reads as a complete one is the failure
        // mode that makes "covered everything" a lie.
        "oversized_skipped": oversized_skipped,
        "oversized_sample": oversized_sample,
        "max_indexable_file_bytes": max_indexable_file_bytes,
        // 2026-08-19: outcome of the unknown-kind repair. `backfill_error`
        // non-null means the wiring_map is still polluted — never silent.
        "kinds_backfilled": kinds_backfilled,
        "backfill_error": backfill_error,
        // I16 (2026-09-13): the seal, the deterministic kind pass, the atomic
        // wiring writes and the policy sweep — each reported, never silent.
        "generation": generation,
        "kind_resolution": "single_pass_deterministic",
        "wiring_tx_failures": wiring_tx_failures,
        "policy_purged_by_reason": policy_purged_by_reason,
        "consumers_purged": consumers_purged,
        "policy_purged_sample": policy_purged_sample,
        // Companion roots (2026-09-13): which were walked and how many files
        // each contributed under `@companion/<name>/` — an empty map means the
        // rebuild ran on a sub-root or no companion directory exists.
        "companion_roots": companion_files,
        // The search index written in step with the walk: documents replaced and
        // commit/write failures (never silent — a failure means `search` lags).
        "search_documents": search_documents,
        "search_refresh_failures": search_refresh_failures,
        "search_segments_merged": search_segments_merged,
        "search_compact_error": search_compact_error,
    })
    .to_string()
}

// ─────────────────────────────────────────────────────────────────────────────
// AST handlers (3 existing + 8 new = 11 total)
// ─────────────────────────────────────────────────────────────────────────────

/// `cli-ast-find` — AST-level symbol lookup with optional file filter.
pub fn cli_ast_find(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let symbol_name = payload
        .get("symbol_name")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let definitions_only = payload
        .get("definitions_only")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let file_path = payload.get("file_path").and_then(|v| v.as_str());

    if symbol_name.is_empty() {
        return serde_json::json!({"definitions": [], "count": 0}).to_string();
    }

    let results: Vec<serde_json::Value> = if let Some(ref store) = rt.infra.symbol_store {
        store
            .find_symbol(symbol_name)
            .map(|locations| {
                locations
                    .into_iter()
                    .filter(|loc| {
                        let file_match =
                            file_path.map(|f| loc.file_path.contains(f)).unwrap_or(true);
                        let def_match = !definitions_only || loc.is_definition;
                        file_match && def_match
                    })
                    .take(10)
                    .map(|loc| {
                        serde_json::json!({
                            "file_path": loc.file_path,
                            "symbol_name": loc.symbol_name,
                            "line": loc.line,
                            "column": loc.column,
                            "is_definition": loc.is_definition,
                            "kind": loc.kind
                        })
                    })
                    .collect()
            })
            .unwrap_or_default()
    } else {
        vec![]
    };

    serde_json::json!({
        "symbol_name": symbol_name,
        "definitions": results,
        "count": results.len()
    })
    .to_string()
}

/// `cli-ast-overview` — list all symbols defined in a file.
///
/// Wave 22 (S-Q3): wrapped in `query_cache` — mirrors the existing pattern
/// used by `cli_ast_blast` (line ~427). Path-scoped invalidation via
/// `invalidate_by_path` in `post_edit`/`post_write` ensures freshness after edits.
///
/// Bug 3 fix (2026-05-02): now emits `language` + symbol `kind`/`name` (was null).
/// Tries to read the file and use `extract_enriched_symbols` (provides kind).
/// Falls back to symbol_store SymbolLocation (kind=null) if file unreadable.
pub fn cli_ast_overview(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let file_path = match crate::cli::shared::require_file_path(payload) {
        Ok(fp) => fp,
        Err(e) => return e,
    };

    let cache_key =
        crate::shared::query_cache::make_key(&rt.project_root, "cli_ast_overview", file_path);
    if let Some(cached) = crate::shared::query_cache::get(&cache_key) {
        return cached;
    }

    // Bug 3 fix: detect language from file extension (was null in JSON).
    let language = crate::shared::detect_language::detect_language_or_unknown(file_path);

    // Resolve absolute path to read file content for enriched extraction.
    let abs_path: std::path::PathBuf = if std::path::Path::new(file_path).is_absolute() {
        std::path::PathBuf::from(file_path)
    } else {
        rt.project_root.join(file_path)
    };

    // Primary path: read source + extract enriched symbols (have kind + is_public).
    // Fallback: bare SymbolLocation from symbol_store (kind/is_public unavailable).
    let symbols: Vec<serde_json::Value> = match std::fs::read_to_string(&abs_path) {
        Ok(content) => match crate::ast_bridge::extract_enriched_symbols(&content, file_path) {
            Some(enriched) if !enriched.is_empty() => enriched
                .into_iter()
                .map(|sym| {
                    serde_json::json!({
                        "symbol_name": sym.name.clone(),
                        "name": sym.name,
                        "kind": sym.kind.as_str(),
                        "line": sym.line,
                        "column": sym.column,
                        "is_definition": true,
                        "is_public": sym.is_public,
                    })
                })
                .collect(),
            _ => fallback_overview_from_store(rt, file_path),
        },
        Err(_) => fallback_overview_from_store(rt, file_path),
    };

    let out = serde_json::json!({
        "file_path": file_path,
        "language": language,
        "symbols": symbols,
        "symbol_count": symbols.len()
    })
    .to_string();
    crate::shared::query_cache::put(cache_key, out.clone());
    out
}

/// Fallback: query symbol_store for SymbolLocation (no kind/is_public available).
fn fallback_overview_from_store(rt: &HookRuntime, file_path: &str) -> Vec<serde_json::Value> {
    if let Some(ref store) = rt.infra.symbol_store {
        store
            .find_symbols_in_file(file_path)
            .map(|locations| {
                locations
                    .into_iter()
                    .map(|loc| {
                        serde_json::json!({
                            "symbol_name": loc.symbol_name.clone(),
                            "name": loc.symbol_name,
                            "kind": loc.kind.clone(),
                            "line": loc.line,
                            "column": loc.column,
                            "is_definition": loc.is_definition,
                            "is_public": serde_json::Value::Null,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default()
    } else {
        Vec::new()
    }
}

/// `cli-ast-blast` — compute blast radius (number of consumers) for a file.
///
/// Wave 18: cached for 60s — pre_edit calls this on every L3+ edit
/// (regra de ouro "blast radius first"). path-scoped invalidation in
/// post_edit ensures freshness after the edit completes.
pub fn cli_ast_blast(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let file_path = payload
        .get("file_path")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let cache_key =
        crate::shared::query_cache::make_key(&rt.project_root, "cli_ast_blast", file_path);
    if let Some(cached) = crate::shared::query_cache::get(&cache_key) {
        return cached;
    }

    let db = &rt.ctx.knowledge;

    let consumers: Vec<String> = {
        let mut stmt = match db.conn_ref().prepare(&format!(
            "SELECT DISTINCT consumer_file FROM {} WHERE module_file = ?1 AND consumer_file IS NOT NULL",
            schema_guard::TABLE_WIRING_MAP
        )) {
            Ok(s) => s,
            Err(e) => {
                tracing::debug!("ast blast prepare failed: {}", e);
                return serde_json::json!({
                    "file_path": file_path,
                    "blast_radius": 0,
                    "consumers": [],
                    "error": format!("{}", e)
                })
                .to_string();
            }
        };
        stmt.query_map(params![file_path], |r| r.get::<_, String>(0))
            .map(|rows| rows.filter_map(|r| r.ok()).collect())
            .unwrap_or_default()
    };

    let out = serde_json::json!({
        "file_path": file_path,
        "blast_radius": consumers.len(),
        "consumers": consumers
    })
    .to_string();
    crate::shared::query_cache::put(cache_key, out.clone());
    out
}

// ─────────────────────────────────────────────────────────────────────────────
// 8 NEW AST handlers (semantic / quality / calls / heat / modules / scope / detail / imports)
// ─────────────────────────────────────────────────────────────────────────────

/// `cli-ast-semantic` — find semantically similar symbols using cosine similarity.
/// Uses touring_code::ast::SemanticSymbolIndex with 16-dim feature vectors.
pub fn cli_ast_semantic(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let query = payload.get("query").and_then(|v| v.as_str()).unwrap_or("");
    let threshold = payload
        .get("threshold")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.5) as f32;
    let limit = payload.get("limit").and_then(|v| v.as_u64()).unwrap_or(10) as usize;

    if query.is_empty() {
        return serde_json::json!({"error": "query is required"}).to_string();
    }

    let index = SemanticSymbolIndex::new(2048);

    // Build query symbol
    let query_sym = touring_code::ast::symbols::Symbol::new(
        query,
        SymbolKind::Function,
        0,
        1,
        0,
        0,
        0,
        query,
        true,
    );

    let results = index.find_similar_symbols(&query_sym, threshold as f64, limit);
    let count = results.len();

    // S-25: RL reward, weighted by how much of the requested limit was
    // actually matched — a constant carries no signal to learn from.
    let reward = crate::cli::shared::retrieval_coverage_reward(count, limit);
    rt.learning
        .inject_reward("cli-ast-semantic", reward, "similarity_coverage");

    serde_json::json!({
        "query": query,
        "threshold": threshold,
        "results": results,
        "count": count
    })
    .to_string()
}

/// `cli-ast-quality` — analyze code quality (anti-patterns, complexity, severity).
/// Uses touring_code::ast::analyze_quality.
pub fn cli_ast_quality(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let file_path = match crate::cli::shared::require_file_path(payload) {
        Ok(fp) => fp,
        Err(e) => return e,
    };

    let source = std::fs::read_to_string(file_path).unwrap_or_default();
    let lang = Lang::from_path(Path::new(&file_path)).unwrap_or(Lang::Rust);
    let report: QualityReport = analyze_quality(&source, lang);

    // S-25: RL reward = the quality actually measured, not a constant. This
    // call site is the luckiest of the five: it already computed a real scalar
    // in [0, 1] and was throwing it away in favour of a literal `1.0`.
    let reward = f64::from(report.overall_score).clamp(0.0, 1.0);
    rt.learning
        .inject_reward("cli-ast-quality", reward, "measured_overall_score");

    serde_json::json!({
        "file_path": file_path,
        "language": lang.as_str(),
        "report": {
            "overall_score": report.overall_score,
            "complexity_score": report.complexity_score,
            "antipattern_score": report.antipattern_score,
            "max_complexity": report.max_complexity,
            "avg_complexity": report.avg_complexity,
            "complex_symbols": report.complex_symbols,
            "antipatterns": report.antipatterns,
        }
    })
    .to_string()
}

/// `cli-ast-calls` — build and query the call graph for a file.
/// Uses touring_code::ast::build_call_graph.
pub fn cli_ast_calls(_rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let symbol = payload.get("symbol").and_then(|v| v.as_str()).unwrap_or("");
    let file_path = match crate::cli::shared::require_file_path(payload) {
        Ok(fp) => fp,
        Err(e) => return e,
    };

    let source = std::fs::read_to_string(file_path).unwrap_or_default();
    let lang = Lang::from_path(Path::new(&file_path)).unwrap_or(Lang::Rust);
    let graph: CallGraph = build_call_graph(&source, lang);

    // If a symbol is specified, return callers/callees for that symbol
    let callers: Vec<String> = if !symbol.is_empty() {
        graph
            .callers_of(symbol)
            .into_iter()
            .map(|c| c.caller.clone())
            .collect()
    } else {
        vec![]
    };
    let callees: Vec<String> = if !symbol.is_empty() {
        graph
            .callees_of(symbol)
            .into_iter()
            .map(|c| c.callee.clone())
            .collect()
    } else {
        vec![]
    };

    serde_json::json!({
        "file_path": file_path,
        "symbol": symbol,
        "callers": callers,
        "callees": callees,
        "total_calls": graph.sites.len()
    })
    .to_string()
}

/// `cli-ast-heat` — compute file heat score (edits * recency_decay * blast_radius).
/// Uses touring_code::ast::{HeatMap, FileHeat}.
pub fn cli_ast_heat(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let file_path = match crate::cli::shared::require_file_path(payload) {
        Ok(fp) => fp,
        Err(e) => return e,
    };

    // Use knowledge DB to get heat data
    let db = &rt.ctx.knowledge;
    let edits: u32 = db
        .conn_ref()
        .query_row(
            &format!(
                "SELECT COUNT(*) FROM {} WHERE module_file = ?1",
                schema_guard::TABLE_WIRING_MAP
            ),
            params![file_path],
            |r| r.get(0),
        )
        .unwrap_or(0);

    let access_count: u32 = db
        .conn_ref()
        .query_row(
            &format!(
                "SELECT COUNT(*) FROM {} WHERE file_path = ?1",
                schema_guard::TABLE_FILE_KNOWLEDGE
            ),
            params![file_path],
            |r| r.get(0),
        )
        .unwrap_or(0);

    let last_edit_epoch: f64 = db
        .conn_ref()
        .query_row(
            &format!(
                "SELECT last_edit FROM {} WHERE file_path = ?1",
                schema_guard::TABLE_FILE_KNOWLEDGE
            ),
            params![file_path],
            |r| r.get(0),
        )
        .unwrap_or(0.0);

    let blast_radius_weight: f64 = {
        let consumers: i64 = db
            .conn_ref()
            .query_row(
                &format!(
                    "SELECT COUNT(DISTINCT consumer_file) FROM {} WHERE module_file = ?1",
                    schema_guard::TABLE_WIRING_MAP
                ),
                params![file_path],
                |r| r.get(0),
            )
            .unwrap_or(0);
        (consumers as f64) * 0.1
    };

    let file_heat =
        FileHeat::from_existing(edits, last_edit_epoch, access_count, blast_radius_weight);

    let now = chrono::Utc::now().timestamp() as f64;
    let heat_score = file_heat.heat_score(now, HeatMap::DEFAULT_HALF_LIFE_SECS);

    serde_json::json!({
        "file_path": file_path,
        "heat_score": heat_score,
        "edits": edits,
        "access_count": access_count,
        "last_edit_epoch": last_edit_epoch,
        "blast_radius_weight": blast_radius_weight
    })
    .to_string()
}

/// `cli-ast-modules` — build module tree for a directory.
/// Uses touring_code::ast::ModuleTree.
pub fn cli_ast_modules(_rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let dir = payload.get("dir").and_then(|v| v.as_str()).unwrap_or("");

    if dir.is_empty() {
        return serde_json::json!({"error": "dir required"}).to_string();
    }

    let root_path = Path::new(&dir);
    if !root_path.is_dir() {
        return serde_json::json!({"error": "dir does not exist or is not a directory"})
            .to_string();
    }

    let mut module_nodes: Vec<serde_json::Value> = Vec::new();

    // Same containment gate as `cli_index_rebuild::walk` — this is the second
    // recursive walker in the indexing path, and a guard applied to only one of two
    // symmetric call sites is the asymmetry-bug shape the decision matrix calls C08.
    fn walk_dir(
        dir: &Path,
        root: &Path,
        visited: &mut std::collections::HashSet<std::path::PathBuf>,
        acc: &mut Vec<serde_json::Value>,
    ) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if let Some(canonical) = inside_root(&path, root)
                    && visited.insert(canonical)
                {
                    walk_dir(&path, root, visited, acc);
                }
            } else if path.extension().map(|e| e == "rs").unwrap_or(false)
                && inside_root(&path, root).is_some()
                && let Ok(content) = std::fs::read_to_string(&path)
            {
                let filename = path.file_name().unwrap_or_default().to_string_lossy();
                let tree = ModuleTree::build_from_source(&content, &filename);
                acc.push(serde_json::json!({
                    "file": filename,
                    "path": path.to_string_lossy(),
                    "root": tree.root.name,
                    "children": tree.root.children.len()
                }));
            }
        }
    }

    let canonical_root =
        std::fs::canonicalize(root_path).unwrap_or_else(|_| root_path.to_path_buf());
    let mut visited: std::collections::HashSet<std::path::PathBuf> =
        std::collections::HashSet::new();
    visited.insert(canonical_root.clone());
    walk_dir(root_path, &canonical_root, &mut visited, &mut module_nodes);

    serde_json::json!({
        "dir": dir,
        "modules": module_nodes,
        "count": module_nodes.len()
    })
    .to_string()
}

/// `cli-ast-scope` — build scope map (local variable definitions) for a file.
/// Uses touring_code::ast::build_scope_map.
pub fn cli_ast_scope(_rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let file_path = match crate::cli::shared::require_file_path(payload) {
        Ok(fp) => fp,
        Err(e) => return e,
    };

    let source = std::fs::read_to_string(file_path).unwrap_or_default();
    let lang = Lang::from_path(Path::new(&file_path)).unwrap_or(Lang::Rust);
    let scope_map: ScopeMap = build_scope_map(&source, lang);

    let entries: Vec<serde_json::Value> = scope_map
        .entries
        .iter()
        .map(|e| {
            serde_json::json!({
                "name": e.name,
                "type": e.type_str,
                "kind": format!("{:?}", e.kind),
                "line": e.line
            })
        })
        .collect();

    serde_json::json!({
        "file_path": file_path,
        "entries": entries,
        "count": entries.len()
    })
    .to_string()
}

/// `cli-ast-detail` — extract detailed information about a symbol (fields, methods, variants).
/// Uses touring_code::ast::extract_symbol_details.
pub fn cli_ast_detail(_rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let symbol = payload.get("symbol").and_then(|v| v.as_str()).unwrap_or("");
    let file_path = payload
        .get("file_path")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    if symbol.is_empty() {
        return serde_json::json!({"error": "symbol is required"}).to_string();
    }
    if file_path.is_empty() {
        return serde_json::json!({"error": "file_path required"}).to_string();
    }

    let source = std::fs::read_to_string(file_path).unwrap_or_default();
    let details: Vec<SymbolDetail> = extract_symbol_details(&source, symbol);

    serde_json::json!({
        "symbol": symbol,
        "file_path": file_path,
        "details": details,
        "count": details.len()
    })
    .to_string()
}

/// `cli-ast-imports` — extract and resolve imports for a file.
/// Uses touring_code::ast::extract_imports_resolved.
pub fn cli_ast_imports(_rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let file_path = match crate::cli::shared::require_file_path(payload) {
        Ok(fp) => fp,
        Err(e) => return e,
    };

    let source = std::fs::read_to_string(file_path).unwrap_or_default();
    let lang = Lang::from_path(Path::new(&file_path)).unwrap_or(Lang::Rust);
    let resolver: ImportResolver = extract_imports_resolved(&source, lang);

    let imports: Vec<serde_json::Value> = resolver
        .imports
        .iter()
        .map(|i| {
            serde_json::json!({
                "path": i.path,
                "alias": i.alias,
                "is_glob": i.is_glob,
                "line": i.line
            })
        })
        .collect();

    serde_json::json!({
        "file_path": file_path,
        "language": lang.as_str(),
        "imports": imports,
        "count": imports.len()
    })
    .to_string()
}

// ─────────────────────────────────────────────────────────────────────────────
// `cli-index-ingest` — single-file incremental reindex (B3, 2026-05-10)
// ─────────────────────────────────────────────────────────────────────────────

/// `cli-index-ingest` — reindex one file on demand.
///
/// Public, on-demand counterpart to the `post_edit`/`post_write` reindex pass.
/// Use when:
///
/// - The hook reindex silently failed in a prior edit
///   (`reindex_failure_count > 0` in `touring gate-metrics`).
/// - You want to refresh a single file without paying for a 30 s full
///   `touring index rebuild`.
/// - Automation needs a deterministic, scriptable single-file reindex.
///
/// Internally delegates to [`crate::shared::reindex::reindex_file_with_old`]
/// with `old_content = None`, which forces the O(file) full-parse path
/// (the O(edit_region) fast path requires the old content captured at the
/// edit boundary, which we don't have here).
///
/// # Payload
///
/// ```json
/// { "path": "crates/touring-core/src/lib.rs" }   // relative to project_root, OR
/// { "path": "/absolute/path/to/file.rs" }        // absolute path
/// ```
///
/// # Response
///
/// ```json
/// // Success:
/// { "status": "ok", "file_path": "<rel>", "abs_path": "<abs>" }
///
/// // Missing/empty path argument:
/// { "error": "missing required field 'path'", "hint": "usage: touring index ingest <file>" }
///
/// // File does not exist on disk:
/// { "status": "error", "file_path": "<rel>", "error": "file not found" }
///
/// // Reindex pipeline failed (also bumps `reindex_failure_count`):
/// { "status": "error", "file_path": "<rel>", "error": "<reason>" }
/// ```
pub fn cli_index_ingest(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let path_arg = match payload.get("path").and_then(|v| v.as_str()) {
        Some(p) if !p.is_empty() => p.to_string(),
        _ => {
            return serde_json::json!({
                "error": "missing required field 'path'",
                "hint": "usage: touring index ingest <file>"
            })
            .to_string();
        }
    };

    // Resolve the absolute path against the runtime's project_root; a stored
    // key (`@companion/<name>/<rel>`) resolves under its companion root.
    let policy = index_policy::IndexPolicy::for_root(&rt.project_root);
    let abs_path = if path_arg.starts_with(index_policy::COMPANION_KEY_PREFIX)
        || Path::new(&path_arg).is_absolute()
    {
        policy.path_for_key(&path_arg).to_string_lossy().to_string()
    } else {
        rt.project_root
            .join(&path_arg)
            .to_string_lossy()
            .to_string()
    };

    if !Path::new(&abs_path).exists() {
        return serde_json::json!({
            "status": "error",
            "file_path": path_arg,
            "error": "file not found",
            "abs_path": abs_path,
        })
        .to_string();
    }

    // The writer's own admission check, asked HERE so a refusal is reported to
    // the caller instead of logged and swallowed: `reindex_file_with_old`
    // refuses what the walker refuses, and a manual ingest of a `.venv/` file
    // used to answer `status: ok` while writing nothing (measured 13/09/2026).
    // `Ok(key)` is the storage key — project-relative or `@companion/…`.
    let rel_path = match crate::shared::reindex::admission_refusal(rt, &abs_path) {
        Ok(key) => key,
        Err((verdict, detail)) => {
            return serde_json::json!({
                "status": "refused",
                "file_path": path_arg,
                "abs_path": abs_path,
                "verdict": verdict,
                "detail": detail,
                "hint": "the walker applies the same rule — see `touring index why <path>`",
            })
            .to_string();
        }
    };

    match crate::shared::reindex::reindex_file_with_old(rt, &abs_path, &rel_path, None) {
        Ok(()) => serde_json::json!({
            "status": "ok",
            "file_path": rel_path,
            "abs_path": abs_path,
        })
        .to_string(),
        Err(e) => {
            // Mirror post_edit's observability: bump the failure counter so
            // `touring gate-metrics` reflects manual ingest attempts that fail.
            crate::shared::gate_metrics::record_reindex_failure();
            serde_json::json!({
                "status": "error",
                "file_path": rel_path,
                "abs_path": abs_path,
                "error": e.to_string(),
            })
            .to_string()
        }
    }
}

#[cfg(test)]
mod wiring_repair_visibility_tests {
    //! The `kind_unknown` repair is the ONLY thing that clears polluted
    //! consumer rows, it runs exactly once per rebuild, and a rebuild is
    //! long. Swallowing its error (`.unwrap_or(0)`) means a project can carry
    //! a polluted wiring_map for days with no trace of why — which is exactly
    //! what happened to `analise` on 2026-08-19 (3.923 rows). Structural, not
    //! behavioural: the failure needs a running daemon and a poisoned DB to
    //! reproduce, but the shape that hides it is readable from the source.

    const SOURCE: &str = include_str!("index.rs");

    /// The repair call must not discard its `Result`.
    #[test]
    fn the_unknown_kind_repair_never_swallows_its_error() {
        let call = "resolve_consumer_kinds_from_producers(";
        let at = SOURCE
            .find(call)
            .expect("cli_index_rebuild must still run the unknown-kind repair");
        // The handling straddles the call: `match <call> {` puts the keyword
        // before it and the arms after. Read both sides.
        let before = &SOURCE[at.saturating_sub(200)..at];
        let after = {
            let tail = &SOURCE[at + call.len()..];
            // `&tail[..N]` panics the moment an accented character lands on the
            // cut — and this guard reads real source, which has them.
            touring_foundation::truncate_str(tail, 200)
        };
        for swallow in [
            ".unwrap_or(",
            ".unwrap_or_default(",
            ".ok()",
            ".unwrap_or_else(",
        ] {
            assert!(
                !after.contains(swallow),
                "the unknown-kind repair discards its error via `{swallow}` — a failed repair \
                 leaves kind_unknown rows behind and reports success anyway"
            );
        }
        assert!(
            before.contains("match") || after.contains("?") || after.contains("Err("),
            "the repair's Result must be handled explicitly (match / `?`)"
        );
    }

    /// `extern` is terminal, so it may only be concluded from a COMPLETE walk.
    #[test]
    fn the_extern_pass_is_gated_on_a_complete_walk() {
        let at = SOURCE
            .find("resolve_consumer_kinds_from_producers(")
            .expect("the repair call must still exist");
        let tail = &SOURCE[at..];
        let args = touring_foundation::truncate_str(tail, 80);
        assert!(
            args.contains("!aborted_memory_pressure"),
            "the repair must be told whether the walk completed: marking rows \
             `extern` after a partial walk brands symbols whose producers were \
             simply never read, and `extern` is never revisited"
        );
    }

    /// A failed repair has to reach the caller, not just the log.
    #[test]
    fn a_failed_repair_is_reported_in_the_rebuild_payload() {
        assert!(
            SOURCE.contains("\"backfill_error\": backfill_error"),
            "the rebuild payload must carry `backfill_error` so a caller can tell a \
             clean rebuild from one that left the wiring_map polluted"
        );
    }
}

#[cfg(test)]
mod size_ceiling_tests {
    use super::{MAX_INDEXABLE_FILE_BYTES, exceeds_index_size_ceiling};

    /// The sizes that took the daemon to 48 GB, verbatim from the 19/08/2026
    /// measurement of `analise`. Each carries an extension in `SUPPORTED_EXTS`,
    /// so every one of them was read in full and handed to the parser.
    #[test]
    fn the_files_that_exhausted_the_machine_are_refused() {
        for (bytes, what) in [
            (
                380_400_000,
                "converted engineering report .md (×2 in the tree)",
            ),
            (69_300_000, "geodata .json"),
            (42_600_000, "claims_semantica_completa.json"),
            (22_800_000, "scraped .html"),
        ] {
            assert!(
                exceeds_index_size_ceiling(bytes),
                "{what} ({bytes} B) must be refused — reading it is the allocation that starts \
                 the spike the RSS probe cannot see between two chunk boundaries"
            );
        }
    }

    /// Calibrated against the real corpus: across all four projects, every code
    /// file over 1 MB lives in a venv or `node_modules` (already skipped), and
    /// the largest found anywhere is a 2.89 MB minified bundle. A ceiling that
    /// refused real source would trade one silent failure for another.
    #[test]
    fn no_real_source_file_is_refused() {
        for (bytes, what) in [
            (
                2_890_000,
                "largest file measured anywhere (minified JS bundle)",
            ),
            (
                2_270_000,
                "largest generated Python client (kubernetes core_v1_api.py)",
            ),
            (1_150_000, "largest torch test module"),
            (250_000, "a large hand-written Rust module"),
            (0, "an empty file"),
        ] {
            assert!(
                !exceeds_index_size_ceiling(bytes),
                "{what} ({bytes} B) is legitimate source and must still be indexed"
            );
        }
    }

    #[test]
    fn the_ceiling_sits_where_the_calibration_put_it() {
        assert_eq!(
            MAX_INDEXABLE_FILE_BYTES,
            8 * 1024 * 1024,
            "8 MB is ~3x the largest file observed across the fleet; changing it \
             without re-measuring the corpus reopens the 48 GB failure"
        );
        assert!(!exceeds_index_size_ceiling(MAX_INDEXABLE_FILE_BYTES));
        assert!(exceeds_index_size_ceiling(MAX_INDEXABLE_FILE_BYTES + 1));
    }
}

#[cfg(test)]
mod containment_tests {
    use super::inside_root;
    use std::fs;

    /// A symlink pointing outside the project is refused.
    ///
    /// This is the escape that let `ln -s /etc project/x` land foreign bytes in the
    /// project's `symbols.db`, because `Path::is_dir()` resolves links.
    #[test]
    fn a_symlink_escaping_the_root_is_refused() {
        let tmp = std::env::temp_dir().join(format!("touring-esc-{}", std::process::id()));
        let root = tmp.join("project");
        let outside = tmp.join("outside");
        fs::create_dir_all(root.join("src")).unwrap();
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("secret.rs"), "fn leaked() {}").unwrap();

        let canonical_root = fs::canonicalize(&root).unwrap();
        let link = root.join("escape");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&outside, &link).unwrap();

        assert!(
            inside_root(&link, &canonical_root).is_none(),
            "a link resolving outside the root must be refused"
        );
        // and the file behind it too — a symlinked *file* escapes just as well
        #[cfg(unix)]
        {
            let file_link = root.join("leak.rs");
            std::os::unix::fs::symlink(outside.join("secret.rs"), &file_link).unwrap();
            assert!(
                inside_root(&file_link, &canonical_root).is_none(),
                "a symlinked file resolving outside the root must be refused"
            );
        }
        let _ = fs::remove_dir_all(&tmp);
    }

    /// A symlink that stays inside the root is still followed.
    ///
    /// The guard removes the escape, never the capability (REGRA #0) — a repo that
    /// symlinks one of its own directories keeps working.
    #[test]
    fn a_symlink_staying_inside_the_root_is_still_followed() {
        let tmp = std::env::temp_dir().join(format!("touring-in-{}", std::process::id()));
        let root = tmp.join("project");
        fs::create_dir_all(root.join("real")).unwrap();
        fs::write(root.join("real/lib.rs"), "pub fn kept() {}").unwrap();
        let canonical_root = fs::canonicalize(&root).unwrap();

        #[cfg(unix)]
        {
            let link = root.join("alias");
            std::os::unix::fs::symlink(root.join("real"), &link).unwrap();
            assert!(
                inside_root(&link, &canonical_root).is_some(),
                "an internal symlink must still be walkable"
            );
        }
        let _ = fs::remove_dir_all(&tmp);
    }

    /// The canonical path is what the caller feeds its `visited` set, so a cycle
    /// resolves to an already-seen entry and terminates instead of overflowing.
    #[test]
    fn a_cycle_resolves_to_one_canonical_entry() {
        let tmp = std::env::temp_dir().join(format!("touring-cyc-{}", std::process::id()));
        let root = tmp.join("project");
        fs::create_dir_all(root.join("a")).unwrap();
        let canonical_root = fs::canonicalize(&root).unwrap();

        #[cfg(unix)]
        {
            // a/loop -> a  — the shape that recursed until the stack blew
            std::os::unix::fs::symlink(root.join("a"), root.join("a/loop")).unwrap();
            let mut visited = std::collections::HashSet::new();
            let first = inside_root(&root.join("a"), &canonical_root).unwrap();
            assert!(visited.insert(first), "first visit is new");
            let through_loop = inside_root(&root.join("a/loop"), &canonical_root).unwrap();
            assert!(
                !visited.insert(through_loop),
                "the cycle must resolve to an already-visited canonical path"
            );
        }
        let _ = fs::remove_dir_all(&tmp);
    }

    /// A dangling link canonicalizes to Err and is skipped rather than erroring mid-walk.
    #[test]
    fn a_broken_link_is_skipped_not_fatal() {
        let tmp = std::env::temp_dir().join(format!("touring-brk-{}", std::process::id()));
        let root = tmp.join("project");
        fs::create_dir_all(&root).unwrap();
        let canonical_root = fs::canonicalize(&root).unwrap();
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(root.join("nowhere"), root.join("dangling")).unwrap();
            assert!(inside_root(&root.join("dangling"), &canonical_root).is_none());
        }
        let _ = fs::remove_dir_all(&tmp);
    }

    /// A real child of the root passes — the guard must not reject ordinary files.
    #[test]
    fn ordinary_children_pass() {
        let tmp = std::env::temp_dir().join(format!("touring-ok-{}", std::process::id()));
        let root = tmp.join("project");
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/main.rs"), "fn main() {}").unwrap();
        let canonical_root = fs::canonicalize(&root).unwrap();
        assert!(inside_root(&root.join("src"), &canonical_root).is_some());
        assert!(inside_root(&root.join("src/main.rs"), &canonical_root).is_some());
        let _ = fs::remove_dir_all(&tmp);
    }
}

#[cfg(test)]
mod index_why {
    use super::{
        CandidateFacts, IndexVerdict, MAX_INDEXABLE_FILE_BYTES, classify_index_candidate,
        cli_index_find, cli_index_rebuild, cli_index_status, cli_index_why,
    };
    use crate::runtime::HookRuntime;

    fn facts() -> CandidateFacts {
        CandidateFacts {
            exists: true,
            is_dir: false,
            bytes: 1_024,
            readable: true,
            inside_root: true,
        }
    }

    fn verdict(rel: &str, f: CandidateFacts) -> Option<IndexVerdict> {
        classify_index_candidate(rel, f).map(|(v, _)| v)
    }

    #[test]
    fn walker_rules_are_applied_in_the_walker_order() {
        assert_eq!(
            verdict("target/gen.rs", facts()),
            Some(IndexVerdict::SkippedDir)
        );
        assert_eq!(
            verdict(".hidden/a.rs", facts()),
            Some(IndexVerdict::SkippedDir)
        );
        assert_eq!(
            verdict("venv_x/a.py", facts()),
            Some(IndexVerdict::SkippedDir)
        );
        assert_eq!(
            verdict("./target/gen.rs", facts()),
            Some(IndexVerdict::SkippedDir),
            "a leading ./ is not a component"
        );
        assert_eq!(
            verdict("pln2/src/a.rs", facts()),
            Some(IndexVerdict::SkippedSubproject)
        );
        assert_eq!(
            verdict(
                "src/gone.rs",
                CandidateFacts {
                    exists: false,
                    ..facts()
                }
            ),
            Some(IndexVerdict::Missing)
        );
        assert_eq!(
            verdict(
                "src/lib.rs",
                CandidateFacts {
                    inside_root: false,
                    ..facts()
                }
            ),
            Some(IndexVerdict::OutsideRoot)
        );
        assert_eq!(
            verdict(
                "src",
                CandidateFacts {
                    is_dir: true,
                    ..facts()
                }
            ),
            Some(IndexVerdict::Directory)
        );
        assert_eq!(
            verdict("README.txt", facts()),
            Some(IndexVerdict::UnsupportedExtension)
        );
        assert_eq!(
            verdict("Makefile", facts()),
            Some(IndexVerdict::UnsupportedExtension),
            "no extension at all"
        );
        assert_eq!(
            verdict(
                "src/huge.rs",
                CandidateFacts {
                    bytes: MAX_INDEXABLE_FILE_BYTES + 1,
                    ..facts()
                }
            ),
            Some(IndexVerdict::Oversized)
        );
        assert_eq!(
            verdict(
                "src/latin1.rs",
                CandidateFacts {
                    readable: false,
                    ..facts()
                }
            ),
            Some(IndexVerdict::Unreadable)
        );
        assert_eq!(
            verdict("src/lib.rs", facts()),
            None,
            "eligible: only the store can say indexed"
        );
    }

    #[test]
    fn a_skipped_component_wins_over_a_missing_file() {
        // The walker never enters `target/`, so whether the file exists there is moot.
        assert_eq!(
            verdict(
                "target/gen.rs",
                CandidateFacts {
                    exists: false,
                    ..facts()
                }
            ),
            Some(IndexVerdict::SkippedDir)
        );
    }

    #[test]
    fn the_size_detail_names_the_ceiling() {
        let (_, detail) = classify_index_candidate(
            "src/huge.rs",
            CandidateFacts {
                bytes: MAX_INDEXABLE_FILE_BYTES + 1,
                ..facts()
            },
        )
        .expect("oversized");
        assert!(
            detail.contains(&MAX_INDEXABLE_FILE_BYTES.to_string()),
            "{detail}"
        );
    }

    /// End-to-end over the REAL rebuild + `cli_index_why`, in-process (the CLI verb
    /// would exercise whatever binary the daemon runs — see the containment test).
    #[test]
    fn why_answers_for_every_exclusion_class_after_a_real_rebuild() {
        let proj = tempfile::tempdir().expect("project tmpdir");
        let root = proj.path();
        std::fs::create_dir_all(root.join("src")).expect("src");
        std::fs::create_dir_all(root.join("target")).expect("target");
        std::fs::write(
            root.join("src/lib.rs"),
            "pub fn why_probe_indexed_7c1e() {}",
        )
        .expect("lib.rs");
        std::fs::write(
            root.join("target/gen.rs"),
            "pub fn why_probe_skipped_7c1e() {}",
        )
        .expect("gen.rs");
        // `.claude/` documents are walked; its runtime-state subdirectories are not.
        std::fs::create_dir_all(root.join(".claude/skills/s")).expect(".claude/skills");
        std::fs::create_dir_all(root.join(".claude/touring")).expect(".claude/touring");
        std::fs::create_dir_all(root.join(".claude/checkpoints")).expect(".claude/checkpoints");
        std::fs::write(
            root.join(".claude/CLAUDE.md"),
            "# probe\n\n## why_probe_claude_7c1e\n",
        )
        .expect("CLAUDE.md");
        std::fs::write(
            root.join(".claude/skills/s/SKILL.md"),
            "# skill\n\n## why_probe_skill_7c1e\n",
        )
        .expect("SKILL.md");
        std::fs::write(
            root.join(".claude/touring/state.json"),
            "{\"why_probe_state_7c1e\": 1}",
        )
        .expect("state.json");
        std::fs::write(
            root.join(".claude/checkpoints/c.json"),
            "{\"why_probe_ckpt_7c1e\": 1}",
        )
        .expect("c.json");
        std::fs::write(root.join("README.txt"), "not code").expect("README.txt");
        // 8 MiB + 1: one byte past the ceiling, a real file the walker must refuse.
        let mut huge = vec![b'/'; (MAX_INDEXABLE_FILE_BYTES + 1) as usize];
        huge[0] = b'/';
        huge[1] = b'/';
        std::fs::write(root.join("src/huge.rs"), &huge).expect("huge.rs");
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname=\"probe\"\nversion=\"0.0.0\"\nedition=\"2021\"\n",
        )
        .expect("Cargo.toml");

        let _serial = super::REBUILD_TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut rt = HookRuntime::new(root).expect("HookRuntime::new");
        let out = cli_index_rebuild(&mut rt, &serde_json::json!({"dir": root.to_string_lossy()}));
        let rebuilt: serde_json::Value = serde_json::from_str(&out).expect("rebuild json");
        assert_eq!(
            rebuilt["oversized_skipped"], 1,
            "the rebuild refused huge.rs: {out}"
        );

        fn why(rt: &mut crate::runtime::HookRuntime, path: &str) -> serde_json::Value {
            let raw = cli_index_why(rt, &serde_json::json!({ "path": path }));
            serde_json::from_str(&raw).expect("why json")
        }
        let v = why(&mut rt, "src/lib.rs");
        assert_eq!(v["status"], "indexed", "{v}");
        assert!(v["symbols"].as_u64().unwrap_or(0) >= 1, "{v}");
        assert_eq!(why(&mut rt, "target/gen.rs")["status"], "skipped_dir");
        assert_eq!(
            why(&mut rt, "README.txt")["status"],
            "unsupported_extension"
        );
        let h = why(&mut rt, "src/huge.rs");
        assert_eq!(h["status"], "oversized", "{h}");
        assert_eq!(h["bytes"], MAX_INDEXABLE_FILE_BYTES + 1);
        assert_eq!(why(&mut rt, "src/nope.rs")["status"], "missing");
        assert_eq!(why(&mut rt, "src")["status"], "directory");
        assert_eq!(why(&mut rt, "/etc/hostname")["status"], "outside_root");
        // An absolute path OUTSIDE the root under a hidden-named directory (every
        // tempdir is `.tmpXXXX`) is judged by containment, never by that ancestor —
        // the verdict the sweep gives the same key.
        let outside = tempfile::tempdir().expect("outside tmpdir");
        std::fs::write(outside.path().join("note.md"), "# note\n").expect("note.md");
        let o = why(&mut rt, &outside.path().join("note.md").to_string_lossy());
        assert_eq!(o["status"], "outside_root", "{o}");

        // `.claude/` documents ARE in the index (memories, rules, skills, plans
        // live there); its state subdirectories are refused, each by its own rule.
        let c = why(&mut rt, ".claude/CLAUDE.md");
        assert_eq!(c["status"], "indexed", "{c}");
        assert_eq!(
            why(&mut rt, ".claude/skills/s/SKILL.md")["status"],
            "indexed"
        );
        let st = why(&mut rt, ".claude/touring/state.json");
        assert_eq!(st["status"], "skipped_dir", "{st}");
        assert!(
            st["detail"]
                .as_str()
                .unwrap_or("")
                .contains("runtime state"),
            "{st}"
        );
        assert_eq!(
            why(&mut rt, ".claude/checkpoints/c.json")["status"],
            "skipped_dir"
        );

        // Written after the rebuild: eligible by every rule, but not yet stored.
        std::fs::write(
            root.join("src/later.rs"),
            "pub fn why_probe_later_7c1e() {}",
        )
        .expect("later.rs");
        let l = why(&mut rt, "src/later.rs");
        assert_eq!(l["status"], "eligible_not_indexed", "{l}");
        assert!(
            l["detail"].as_str().unwrap_or("").contains("index ingest"),
            "{l}"
        );
        assert_eq!(why(&mut rt, "")["error"], "path required");

        // I16: the rebuild sealed a generation, ran the deterministic kind pass
        // and committed every file's wiring rows as one transaction.
        assert_eq!(rebuilt["generation"]["state"], "complete", "{out}");
        assert_eq!(rebuilt["kind_resolution"], "single_pass_deterministic");
        assert_eq!(rebuilt["wiring_tx_failures"], 0);

        // The hook writer asks the walker's predicate: a file under `target/`
        // reaches the store through neither route.
        let gen_abs = root.join("target/gen.rs").to_string_lossy().to_string();
        crate::shared::reindex::reindex_file_with_old(&rt, &gen_abs, "target/gen.rs", None)
            .expect("a refusal is not an error");
        let g = why(&mut rt, "target/gen.rs");
        assert_eq!(
            (
                g["status"].as_str(),
                g["symbols"].as_u64(),
                g["residue"].as_bool()
            ),
            (Some("skipped_dir"), Some(0), Some(false)),
            "{g}"
        );

        // Residue: rows a pre-policy writer left for paths the walker refuses — a
        // hidden/skipped dir, an absolute path outside the root, the dotted spelling.
        let row = |file: &str| touring_code::ast::SymbolLocation {
            symbol_name: "why_probe_residue_7c1e".to_string(),
            file_path: file.to_string(),
            line: 1,
            column: 0,
            is_definition: true,
            kind: Some("function".to_string()),
        };
        {
            let store = rt.symbol_store().expect("symbol store");
            for file in ["target/gen.rs", "/etc/hostname", "./src/lib.rs"] {
                store
                    .replace_file_symbols(file, &[row(file)])
                    .expect("inject residue");
            }
        }
        let g = why(&mut rt, "target/gen.rs");
        assert_eq!(g["status"], "skipped_dir");
        assert_eq!(g["residue"], true, "{g}");
        assert!(
            g["detail"].as_str().unwrap_or("").contains("index rebuild"),
            "{g}"
        );
        assert_eq!(why(&mut rt, "/etc/hostname")["residue"], true);

        // A generation left `building` by a dead daemon is flagged by every reader
        // (the pid is one no Linux process carries) — until the next rebuild seals.
        rt.ctx
            .knowledge
            .begin_index_generation(4_000_000_000)
            .expect("stale generation");
        let p = why(&mut rt, "src/lib.rs");
        assert_eq!(p["index_state"], "partial", "{p}");
        assert_eq!(p["remedy"], "touring index rebuild");
        let f: serde_json::Value = serde_json::from_str(&super::cli_index_find(
            &mut rt,
            &serde_json::json!({ "symbol_name": "why_probe_indexed_7c1e" }),
        ))
        .expect("find json");
        assert_eq!(f["index_state"], "partial", "{f}");

        // The next rebuild purges every residue row, by reason, seals `complete`,
        // and — warm, over an unchanged tree — resolves no consumer kind anew.
        let out2 = cli_index_rebuild(&mut rt, &serde_json::json!({"dir": root.to_string_lossy()}));
        let again: serde_json::Value = serde_json::from_str(&out2).expect("rebuild json");
        let by = &again["policy_purged_by_reason"];
        assert_eq!(by["skipped_dir"], 1, "{out2}");
        assert_eq!(by["outside_root"], 1, "{out2}");
        assert_eq!(by["duplicate_spelling"], 1, "{out2}");
        assert_eq!(again["generation"]["state"], "complete", "{out2}");
        assert_eq!(again["kinds_backfilled"], 0, "{out2}");
        let g = why(&mut rt, "target/gen.rs");
        assert_eq!(
            (g["residue"].as_bool(), g.get("index_state")),
            (Some(false), None),
            "{g}"
        );
        assert_eq!(why(&mut rt, "/etc/hostname")["residue"], false);
    }

    /// One daemon serves several projects from ONE process-wide query cache, so
    /// a cached answer must be keyed by the store it read. Measured 13/09/2026:
    /// `touring index status` from a directory outside any project answered the
    /// global store (59 files), and the same key served it to whoever asked next.
    #[test]
    fn a_cached_answer_never_crosses_into_another_project() {
        let indexed = tempfile::tempdir().expect("indexed tmpdir");
        let empty = tempfile::tempdir().expect("empty tmpdir");
        for dir in [indexed.path(), empty.path()] {
            std::fs::write(
                dir.join("Cargo.toml"),
                "[package]\nname=\"probe\"\nversion=\"0.0.0\"\nedition=\"2021\"\n",
            )
            .expect("Cargo.toml");
        }
        std::fs::create_dir_all(indexed.path().join("src")).expect("src");
        std::fs::write(
            indexed.path().join("src/lib.rs"),
            "pub fn cache_scope_probe_7c1e() {}",
        )
        .expect("lib.rs");

        let _serial = super::REBUILD_TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut rt_indexed = HookRuntime::new(indexed.path()).expect("indexed runtime");
        cli_index_rebuild(
            &mut rt_indexed,
            &serde_json::json!({"dir": indexed.path().to_string_lossy()}),
        );
        let mut rt_empty = HookRuntime::new(empty.path()).expect("empty runtime");
        let find = serde_json::json!({"symbol_name": "cache_scope_probe_7c1e"});
        let status = |rt: &mut HookRuntime| -> serde_json::Value {
            serde_json::from_str(&cli_index_status(rt, &serde_json::json!({}))).expect("status")
        };
        let found = |rt: &mut HookRuntime| -> serde_json::Value {
            serde_json::from_str(&cli_index_find(rt, &find)).expect("find")
        };

        // The indexed project answers first, filling the cache.
        assert!(status(&mut rt_indexed)["file_count"].as_u64() >= Some(1));
        assert_eq!(found(&mut rt_indexed)["count"], 1);
        // The other project must answer from its OWN store.
        let other = status(&mut rt_empty);
        assert_eq!(
            other["file_count"], 0,
            "status leaked across projects: {other}"
        );
        let other = found(&mut rt_empty);
        assert_eq!(other["count"], 0, "find leaked across projects: {other}");
    }

    /// B4 (15/09/2026): Python had no consumer edge for a use inside the
    /// declaring file, so a constant its own module reads and a constant nobody
    /// reads were the same row — an orphan. Over the REAL rebuild: imported,
    /// internal-only, dead and private each land where they belong.
    #[test]
    fn python_wiring_tells_imported_internal_and_dead_symbols_apart() {
        let proj = tempfile::tempdir().expect("project tmpdir");
        let root = proj.path();
        std::fs::create_dir_all(root.join(".touring")).expect(".touring");
        // Top-level key, before any section: the per-project opt-in.
        std::fs::write(
            root.join(".touring/touring.toml"),
            "polyglot_wiring = true\n",
        )
        .expect("touring.toml");
        // The four classes, plus the two real shapes the analise asked about:
        // `_FORMAS` (private, read by its own module) and `_NUMERAL_ARABICO`
        // (private, imported by a sibling of the same package).
        std::fs::write(
            root.join("lib.py"),
            "_PRIV = 1\nPUB = 2\nPUB2 = 3\nMORTO = 4\n_SHARED = 5\nPUB_QUAL = 6\n\n\nclass Caixa:\n    def __init__(self, valor):\n        self.valor = valor\n\n\ndef helper():\n    return PUB + _PRIV\n",
        )
        .expect("lib.py");
        std::fs::write(
            root.join("app.py"),
            "from lib import PUB2, _SHARED\nimport lib as gm\n\n\ndef main():\n    return PUB2 + _SHARED + gm.PUB_QUAL\n",
        )
        .expect("app.py");
        // B5 (16/09/2026): the three families the analise measured on the live
        // repository — relative from-import (136 of 231), qualified `alias.Nome`
        // (~90) and dunder (7).
        std::fs::create_dir_all(root.join("pacote")).expect("pacote");
        std::fs::write(root.join("pacote/__init__.py"), "").expect("pacote init");
        std::fs::write(
            root.join("pacote/formato.py"),
            "PUB_REL = 1\nMORTO_REL = 2\n",
        )
        .expect("formato.py");
        std::fs::write(
            root.join("pacote/svg.py"),
            "from .formato import PUB_REL\n\n\ndef render():\n    return PUB_REL\n",
        )
        .expect("svg.py");

        let _serial = super::REBUILD_TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut rt = HookRuntime::new(root).expect("HookRuntime::new");
        cli_index_rebuild(&mut rt, &serde_json::json!({"dir": root.to_string_lossy()}));
        let raw = crate::cli::wiring::cli_wiring_orphans(&mut rt, &serde_json::json!({}));
        let report: serde_json::Value = serde_json::from_str(&raw).expect("orphans json");
        let names = |field: &str| -> Vec<String> {
            report[field]
                .as_array()
                .map(|rows| {
                    rows.iter()
                        .filter(|row| row["module_file"] == "lib.py")
                        .filter_map(|row| row["symbol_name"].as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default()
        };
        let orphans = names("orphans");
        let internal = names("internal_only");
        assert!(
            orphans.contains(&"MORTO".to_string()),
            "nothing reads MORTO: {report}"
        );
        assert!(
            !orphans.contains(&"PUB".to_string()),
            "its own module reads PUB: {report}"
        );
        assert!(
            internal.contains(&"PUB".to_string()),
            "PUB is used only inside lib.py: {report}"
        );
        assert!(
            !internal.contains(&"MORTO".to_string()),
            "internal-only means consumed by its own file, not unconsumed: {report}"
        );
        assert!(
            !orphans.contains(&"PUB2".to_string()) && !internal.contains(&"PUB2".to_string()),
            "app.py imports PUB2: {report}"
        );
        assert!(
            !orphans.contains(&"_PRIV".to_string()) && !internal.contains(&"_PRIV".to_string()),
            "a private binding is not a producer row: {report}"
        );
        assert!(
            !orphans.contains(&"_SHARED".to_string()) && !internal.contains(&"_SHARED".to_string()),
            "a private binding imported by a sibling is still not public API: {report}"
        );
        // B5: qualified use through the module object.
        assert!(
            !orphans.contains(&"PUB_QUAL".to_string())
                && !internal.contains(&"PUB_QUAL".to_string()),
            "`gm.PUB_QUAL` in app.py is a consumer: {report}"
        );
        // B5: a dunder is called by the runtime, never by name.
        assert!(
            !orphans.contains(&"__init__".to_string())
                && !internal.contains(&"__init__".to_string()),
            "a dunder is never reported as an orphan: {report}"
        );
        // B5: relative from-import, in its own package.
        let formato = |field: &str| -> Vec<String> {
            report[field]
                .as_array()
                .map(|rows| {
                    rows.iter()
                        .filter(|row| row["module_file"] == "pacote/formato.py")
                        .filter_map(|row| row["symbol_name"].as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default()
        };
        assert!(
            !formato("orphans").contains(&"PUB_REL".to_string()),
            "`from .formato import PUB_REL` in pacote/svg.py is a consumer: {report}"
        );
        assert!(
            formato("orphans").contains(&"MORTO_REL".to_string()),
            "the symbol of the same file that nobody imports is still an orphan: {report}"
        );
    }

    /// 18/09/2026 (analise): `grafo_memoria.py` was split into siblings behind a
    /// façade and the files ingested. `orphans` then reported the symbols the
    /// siblings import (the hook path never resolved a Python import), `impact`
    /// still counted the façade's edge from before the split (nothing re-reads
    /// that consumer), and a consumer importing through a façade was credited to
    /// it. Over the REAL rebuild and ingest paths, every consumer of `No` lands
    /// on the module that defines it.
    #[test]
    fn a_python_module_split_keeps_every_consumer_on_the_definer() {
        let proj = tempfile::tempdir().expect("project tmpdir");
        let root = proj.path();
        std::fs::create_dir_all(root.join(".touring")).expect(".touring");
        std::fs::write(
            root.join(".touring/touring.toml"),
            "polyglot_wiring = true\n",
        )
        .expect("touring.toml");
        std::fs::create_dir_all(root.join("memoria")).expect("memoria");
        std::fs::create_dir_all(root.join("artefato")).expect("artefato");
        // Before the split: one module defines `No`, a consumer imports it.
        std::fs::write(root.join("memoria/grafo.py"), "class No:\n    pass\n").expect("grafo.py");
        std::fs::write(
            root.join("artefato/uso.py"),
            "from memoria.grafo import No\n\n\ndef usar():\n    return No()\n",
        )
        .expect("uso.py");

        let _serial = super::REBUILD_TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut rt = HookRuntime::new(root).expect("HookRuntime::new");
        cli_index_rebuild(&mut rt, &serde_json::json!({"dir": root.to_string_lossy()}));
        let consumers_of_no = |rt: &HookRuntime| -> Vec<(String, String)> {
            rt.ctx
                .knowledge
                .conn_ref()
                .prepare(
                    "SELECT module_file, consumer_file FROM wiring_map
                     WHERE symbol_name = 'No' AND consumer_file IS NOT NULL
                     ORDER BY module_file, consumer_file",
                )
                .expect("prepare")
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
                .expect("query")
                .collect::<Result<_, _>>()
                .expect("rows")
        };
        assert_eq!(
            consumers_of_no(&rt),
            [(
                "memoria/grafo.py".to_string(),
                "artefato/uso.py".to_string()
            )],
            "the import resolves from the project root, whatever the process cwd"
        );

        // The split: `No` moves to a sibling, the old module becomes a façade,
        // and a second sibling imports `No` from its new home.
        std::fs::write(root.join("memoria/modelo.py"), "class No:\n    pass\n").expect("modelo.py");
        std::fs::write(
            root.join("memoria/grafo.py"),
            "from memoria.modelo import No\n\n__all__ = [\"No\"]\n",
        )
        .expect("façade");
        std::fs::write(
            root.join("memoria/gravacao.py"),
            "from memoria.modelo import No\n\n\ndef gravar():\n    return No()\n",
        )
        .expect("gravacao.py");
        for file in [
            "memoria/modelo.py",
            "memoria/grafo.py",
            "memoria/gravacao.py",
        ] {
            let out = super::cli_index_ingest(
                &mut rt,
                &serde_json::json!({"path": root.join(file).to_string_lossy()}),
            );
            assert!(out.contains("\"ok\""), "{file}: {out}");
        }

        let on_definer = |consumer: &str| ("memoria/modelo.py".to_string(), consumer.to_string());
        let expected = [
            on_definer("artefato/uso.py"),
            on_definer("memoria/grafo.py"),
            on_definer("memoria/gravacao.py"),
        ];
        assert_eq!(
            consumers_of_no(&rt),
            expected,
            "the pre-split edge moved to the definer, and the siblings' imports resolve"
        );
        // The rebuild reaches the same edges: `uso.py` still imports through the
        // façade, and the Python re-export is followed there too (C08).
        cli_index_rebuild(&mut rt, &serde_json::json!({"dir": root.to_string_lossy()}));
        assert_eq!(
            consumers_of_no(&rt),
            expected,
            "rebuild and hook path agree"
        );
        let raw = crate::cli::wiring::cli_wiring_orphans(&mut rt, &serde_json::json!({}));
        let report: serde_json::Value = serde_json::from_str(&raw).expect("orphans json");
        let orphan_no = report["orphans"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|row| row["symbol_name"] == "No");
        assert!(!orphan_no, "`No` has three consumers: {report}");
    }

    /// Cross-audit 14/09/2026 (R2-6): an edge whose consumer file is gone and
    /// never held a symbol survived every rebuild, keeping its producer wired.
    #[test]
    fn a_consumer_edge_of_a_vanished_file_is_swept_by_the_rebuild() {
        let proj = tempfile::tempdir().expect("project tmpdir");
        let root = proj.path();
        std::fs::create_dir_all(root.join("src")).expect("src");
        std::fs::write(root.join("src/lib.rs"), "pub fn vanished_probe_9d4a() {}").expect("lib.rs");
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname=\"probe\"\nversion=\"0.0.0\"\nedition=\"2021\"\n",
        )
        .expect("Cargo.toml");

        let _serial = super::REBUILD_TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut rt = HookRuntime::new(root).expect("HookRuntime::new");
        let rebuild = |rt: &mut HookRuntime| -> serde_json::Value {
            let out = cli_index_rebuild(rt, &serde_json::json!({"dir": root.to_string_lossy()}));
            serde_json::from_str(&out).expect("rebuild json")
        };
        let first = rebuild(&mut rt);
        assert_eq!(first["generation"]["state"], "complete", "{first}");
        rt.ctx
            .knowledge
            .record_consumer(
                "src/lib.rs",
                "vanished_probe_9d4a",
                "crates/gone/src/ghost.rs",
                None,
            )
            .expect("seed the legacy edge");
        let rows = |rt: &HookRuntime| -> i64 {
            rt.ctx
                .knowledge
                .conn_ref()
                .query_row(
                    "SELECT COUNT(*) FROM wiring_map WHERE consumer_file = 'crates/gone/src/ghost.rs'",
                    [],
                    |r| r.get(0),
                )
                .expect("count")
        };
        assert_eq!(rows(&rt), 1, "the seeded edge exists before the rebuild");

        let second = rebuild(&mut rt);
        assert_eq!(
            rows(&rt),
            0,
            "the edge of a vanished consumer is gone: {second}"
        );
        assert!(
            second["consumers_purged"].as_u64().unwrap_or(0) >= 1,
            "{second}"
        );
        assert!(
            second["policy_purged_by_reason"]["missing"]
                .as_u64()
                .unwrap_or(0)
                >= 1,
            "the reason is named: {second}"
        );
    }

    /// Cross-audit 14/09/2026 (R2-2): the MkDocs build under `site/` is ignored
    /// by git and was indexed anyway (47.877 stale symbols). Over the real
    /// rebuild: what git ignores is never walked, a row an older walk stored is
    /// swept by the same rule, and `why` names the file and the rule.
    #[test]
    fn what_git_ignores_is_never_walked_and_an_old_row_is_swept() {
        let proj = tempfile::tempdir().expect("project tmpdir");
        let root = proj.path();
        std::fs::create_dir_all(root.join("src")).expect("src");
        std::fs::create_dir_all(root.join("site/docs")).expect("site");
        std::fs::write(
            root.join("src/lib.rs"),
            "pub fn gitignore_probe_live_7c1e() {}",
        )
        .expect("lib.rs");
        std::fs::write(
            root.join("src/table.gen.rs"),
            "pub fn gitignore_probe_gen_7c1e() {}",
        )
        .expect("table.gen.rs");
        std::fs::write(
            root.join("site/docs/copy.rs"),
            "pub fn gitignore_probe_site_7c1e() {}",
        )
        .expect("copy.rs");
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname=\"probe\"\nversion=\"0.0.0\"\nedition=\"2021\"\n",
        )
        .expect("Cargo.toml");

        let _serial = super::REBUILD_TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut rt = HookRuntime::new(root).expect("HookRuntime::new");
        let rebuild = |rt: &mut HookRuntime| -> serde_json::Value {
            let out = cli_index_rebuild(rt, &serde_json::json!({"dir": root.to_string_lossy()}));
            serde_json::from_str(&out).expect("rebuild json")
        };
        let found = |rt: &HookRuntime, name: &str| -> usize {
            rt.symbol_store()
                .expect("symbol store")
                .find_symbol(name)
                .expect("find")
                .len()
        };

        // Before the rule exists, the copy is indexed like any file.
        let first = rebuild(&mut rt);
        assert_eq!(first["generation"]["state"], "complete", "{first}");
        assert_eq!(found(&rt, "gitignore_probe_site_7c1e"), 1, "{first}");

        std::fs::write(root.join(".gitignore"), "/site/\n*.gen.rs\n").expect(".gitignore");
        let second = rebuild(&mut rt);
        assert_eq!(second["generation"]["state"], "complete", "{second}");
        assert_eq!(
            found(&rt, "gitignore_probe_site_7c1e"),
            0,
            "the ignored copy is gone from the store: {second}"
        );
        assert_eq!(
            found(&rt, "gitignore_probe_live_7c1e"),
            1,
            "the source stays"
        );
        assert_eq!(
            found(&rt, "gitignore_probe_gen_7c1e"),
            0,
            "a file rule refuses a file inside a walked directory: {second}"
        );
        assert!(
            second["policy_purged_by_reason"]["gitignored"]
                .as_u64()
                .unwrap_or(0)
                >= 1,
            "the sweep names the reason: {second}"
        );

        let why = cli_index_why(&mut rt, &serde_json::json!({"path": "site/docs/copy.rs"}));
        let why: serde_json::Value = serde_json::from_str(&why).expect("why json");
        assert_eq!(why["status"], "gitignored", "{why}");
        let detail = why["detail"].as_str().unwrap_or_default();
        assert!(
            detail.contains("/site/") && detail.contains(".gitignore"),
            "{why}"
        );
    }

    /// Companion roots end to end over the REAL rebuild: a directory OUTSIDE
    /// the project, named in `.touring/touring.toml`, is walked under
    /// `@companion/<name>/`, answered by `why` under both spellings, refused by
    /// the walker's own rules, ingestible by key or by path, and purged — row by
    /// row, by reason — the moment its name leaves the configuration.
    #[test]
    fn companion_roots_are_indexed_under_stable_keys_and_swept_by_the_same_policy() {
        let proj = tempfile::tempdir().expect("project tmpdir");
        let root = proj.path();
        let comp = tempfile::tempdir().expect("companion tmpdir");
        std::fs::create_dir_all(root.join("src")).expect("src");
        std::fs::create_dir_all(root.join("target")).expect("target");
        std::fs::write(
            root.join("src/lib.rs"),
            "pub fn companion_probe_project_5b2d() {}",
        )
        .expect("lib.rs");
        std::fs::write(
            root.join("target/gen.rs"),
            "pub fn companion_probe_gen_5b2d() {}",
        )
        .expect("gen.rs");
        // A directory the PROJECT declares out of the index (`exclude_dirs`).
        std::fs::create_dir_all(root.join("vendored")).expect("vendored");
        std::fs::write(
            root.join("vendored/copy.rs"),
            "pub fn companion_probe_vendored_5b2d() {}",
        )
        .expect("copy.rs");
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname=\"probe\"\nversion=\"0.0.0\"\nedition=\"2021\"\n",
        )
        .expect("Cargo.toml");
        std::fs::create_dir_all(comp.path().join("sub")).expect("sub");
        std::fs::create_dir_all(comp.path().join(".hidden")).expect(".hidden");
        std::fs::write(
            comp.path().join("a.md"),
            "# notes\n\n## companion_probe_doc_5b2d\n",
        )
        .expect("a.md");
        std::fs::write(
            comp.path().join("b.rs"),
            "/// Measures the kingfisher dive.\npub fn companion_probe_code_5b2d() {}",
        )
        .expect("b.rs");
        std::fs::write(comp.path().join("sub/c.md"), "# deeper\n").expect("c.md");
        std::fs::write(comp.path().join(".hidden/x.md"), "# hidden\n").expect("x.md");
        std::fs::write(
            comp.path().join("README.txt"),
            "not a kind the walker admits",
        )
        .expect("README.txt");
        // An auto-memory: frontmatter and prose, no heading at all — the shape of
        // 84 of this workspace's 98 memories, which the index used to hold nothing of.
        std::fs::write(
            comp.path().join("mem.md"),
            "---\nname: probe-memory-5b2d\ndescription: \"o pipe engole o status\"\n---\n\nA zebrafinch escapou do viveiro quando o tratador abriu a porta.\n",
        )
        .expect("mem.md");
        let configure = |table: &str| {
            std::fs::create_dir_all(root.join(".touring")).expect(".touring");
            std::fs::write(
                root.join(".touring/touring.toml"),
                format!(
                    "[index]\ncompanion_defaults = false\nexclude_dirs = [\"vendored\"]\n{table}"
                ),
            )
            .expect("touring.toml");
        };
        configure(&format!(
            "[index.companion_roots]\nnotes = \"{}\"\n",
            comp.path().display()
        ));

        let _serial = super::REBUILD_TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut rt = HookRuntime::new(root).expect("HookRuntime::new");
        let out = cli_index_rebuild(&mut rt, &serde_json::json!({"dir": root.to_string_lossy()}));
        let rebuilt: serde_json::Value = serde_json::from_str(&out).expect("rebuild json");
        assert_eq!(
            rebuilt["companion_roots"]["notes"], 4,
            "a.md, b.rs, mem.md and sub/c.md walked; the hidden dir and the .txt refused: {out}"
        );
        assert_eq!(rebuilt["search_refresh_failures"], 0, "{out}");
        // Search documents exist only with the search index compiled in: under
        // `-p touring-cli` alone the feature is off and the walk writes none.
        #[cfg(feature = "tantivy-fts")]
        assert!(
            rebuilt["search_documents"].as_u64().unwrap_or(0) >= 4,
            "the walk wrote search documents: {out}"
        );
        assert_eq!(rebuilt["generation"]["state"], "complete", "{out}");

        // Searchable, never wiring (decision 1-A, 14/09/2026): the companion's
        // `pub fn` has symbols but no producer row, while the project's has one.
        // `wiring_entries` is the discriminating count: the end-of-walk sweep
        // purges `@companion/` producer rows as phantom modules, so the table
        // alone reads zero even when the walk registered them (mutant survived).
        assert_eq!(
            rebuilt["wiring_entries"], 1,
            "only the project's producer was registered by the walk: {out}"
        );
        let wiring_rows = |pattern: &str| -> i64 {
            rt.ctx
                .knowledge
                .conn_ref()
                .query_row(
                    "SELECT COUNT(*) FROM wiring_map WHERE module_file LIKE ?1 OR consumer_file LIKE ?1",
                    [pattern],
                    |r| r.get(0),
                )
                .expect("wiring count")
        };
        assert_eq!(
            wiring_rows("@companion/%"),
            0,
            "no wiring row names a companion file: {out}"
        );
        assert_eq!(
            wiring_rows("src/lib.rs"),
            1,
            "the project producer is wired"
        );
        let companion_symbols = rt
            .symbol_store()
            .expect("symbol store")
            .find_symbol("companion_probe_code_5b2d")
            .expect("find");
        assert_eq!(
            companion_symbols.len(),
            1,
            "the companion code is still indexed for find and search"
        );

        fn why(rt: &mut HookRuntime, path: &str) -> serde_json::Value {
            let raw = cli_index_why(rt, &serde_json::json!({ "path": path }));
            serde_json::from_str(&raw).expect("why json")
        }
        // By key and by absolute path: one answer, one spelling — the key.
        let by_key = why(&mut rt, "@companion/notes/a.md");
        assert_eq!(by_key["status"], "indexed", "{by_key}");
        assert_eq!(by_key["path"], "@companion/notes/a.md");
        let by_path = why(&mut rt, &comp.path().join("a.md").to_string_lossy());
        assert_eq!(by_path["status"], "indexed", "{by_path}");
        assert_eq!(
            by_path["path"], "@companion/notes/a.md",
            "an absolute companion path is answered under its key"
        );
        assert_eq!(
            why(&mut rt, "@companion/notes/sub/c.md")["status"],
            "indexed"
        );
        // The same rules as the project root, relative to the companion root.
        assert_eq!(
            why(&mut rt, "@companion/notes/.hidden/x.md")["status"],
            "skipped_dir"
        );
        assert_eq!(
            why(&mut rt, "@companion/notes/README.txt")["status"],
            "unsupported_extension"
        );
        assert_eq!(
            why(&mut rt, "@companion/notes/nope.md")["status"],
            "missing"
        );
        // The declared exclusion is honoured by the walker and named by `why`.
        let vendored = why(&mut rt, "vendored/copy.rs");
        assert_eq!(vendored["status"], "skipped_dir", "{vendored}");
        assert!(
            vendored["detail"]
                .as_str()
                .unwrap_or("")
                .contains("exclude_dirs"),
            "{vendored}"
        );

        // A heading-less memory is indexed: its document row carries the name.
        let memory = why(&mut rt, "@companion/notes/mem.md");
        assert_eq!(memory["status"], "indexed", "{memory}");

        // Its PROSE is searchable — after the rebuild, after a store-driven
        // reindex (which used to copy names only), and after an ingest of new
        // text. Each step asks a word never asked before: both the handler and the
        // index keep a query cache.
        #[cfg(feature = "tantivy-fts")]
        {
            fn search_files(rt: &mut HookRuntime, q: &str) -> Vec<String> {
                let raw = crate::cli::tantivy::cli_tantivy_search(
                    rt,
                    &serde_json::json!({ "query": q, "top": 10 }),
                );
                let v: serde_json::Value = serde_json::from_str(&raw).expect("search json");
                v.as_array()
                    .map(|a| {
                        a.iter()
                            .filter_map(|h| h["file_path"].as_str().map(str::to_string))
                            .collect()
                    })
                    .unwrap_or_default()
            }
            let hits = search_files(&mut rt, "zebrafinch");
            assert!(
                hits.contains(&"@companion/notes/mem.md".to_string()),
                "body text found after the rebuild: {hits:?}"
            );
            let quoted = search_files(&mut rt, "engole");
            assert!(
                quoted.contains(&"@companion/notes/mem.md".to_string()),
                "the quoted description is text, not a code string: {quoted:?}"
            );

            let reindexed: serde_json::Value =
                serde_json::from_str(&crate::cli::tantivy::cli_tantivy_reindex(
                    &mut rt,
                    &serde_json::json!({ "mode": "full" }),
                ))
                .expect("reindex json");
            assert_eq!(reindexed["reindexed"], true, "{reindexed}");
            let after_reindex = search_files(&mut rt, "viveiro");
            assert!(
                after_reindex.contains(&"@companion/notes/mem.md".to_string()),
                "reindex keeps the text: {after_reindex:?}"
            );
            let code_doc = search_files(&mut rt, "kingfisher");
            assert!(
                code_doc.contains(&"@companion/notes/b.rs".to_string()),
                "reindex keeps code doc comments too: {code_doc:?}"
            );

            std::fs::write(
                comp.path().join("mem.md"),
                "---\nname: probe-memory-5b2d\n---\nO ornitorrinco agora mora aqui.\n",
            )
            .expect("rewrite mem.md");
            let ingested: serde_json::Value = serde_json::from_str(&super::cli_index_ingest(
                &mut rt,
                &serde_json::json!({ "path": "@companion/notes/mem.md" }),
            ))
            .expect("ingest json");
            assert_eq!(ingested["status"], "ok", "{ingested}");
            let fresh = search_files(&mut rt, "ornitorrinco");
            assert!(
                fresh.contains(&"@companion/notes/mem.md".to_string()),
                "the writer path refreshes the text: {fresh:?}"
            );
        }

        let unconfigured = why(&mut rt, "@companion/other/a.md");
        assert_eq!(
            unconfigured["status"], "companion_not_configured",
            "{unconfigured}"
        );
        // The symbol store holds the key, so every reader returns it.
        let f: serde_json::Value = serde_json::from_str(&super::cli_index_find(
            &mut rt,
            &serde_json::json!({ "symbol_name": "companion_probe_code_5b2d" }),
        ))
        .expect("find json");
        assert_eq!(
            f["definitions"][0]["file_path"], "@companion/notes/b.rs",
            "{f}"
        );

        // Manual ingest: refused where the walker refuses (and SAID so — a
        // `.venv/` ingest used to answer ok while writing nothing), stored
        // under the key whether addressed by key or by absolute path.
        fn ingest(rt: &mut HookRuntime, path: &str) -> serde_json::Value {
            let raw = super::cli_index_ingest(rt, &serde_json::json!({ "path": path }));
            serde_json::from_str(&raw).expect("ingest json")
        }
        let refused = ingest(&mut rt, "target/gen.rs");
        assert_eq!(refused["status"], "refused", "{refused}");
        assert_eq!(refused["verdict"], "skipped_dir", "{refused}");
        assert_eq!(
            ingest(&mut rt, "@companion/notes/a.md")["file_path"],
            "@companion/notes/a.md"
        );
        let by_abs = ingest(&mut rt, &comp.path().join("b.rs").to_string_lossy());
        assert_eq!(
            (by_abs["status"].as_str(), by_abs["file_path"].as_str()),
            (Some("ok"), Some("@companion/notes/b.rs")),
            "{by_abs}"
        );
        assert_eq!(
            ingest(&mut rt, "@companion/other/a.md")["status"],
            "error",
            "an unconfigured key resolves nowhere on disk"
        );

        // Residue under a name that is NOT configured is purged by reason.
        {
            let store = rt.symbol_store().expect("symbol store");
            store
                .replace_file_symbols(
                    "@companion/other/z.md",
                    &[touring_code::ast::SymbolLocation {
                        symbol_name: "companion_probe_residue_5b2d".to_string(),
                        file_path: "@companion/other/z.md".to_string(),
                        line: 1,
                        column: 0,
                        is_definition: true,
                        kind: Some("function".to_string()),
                    }],
                )
                .expect("inject residue");
            // A pre-exclusion row under the excluded directory is residue too.
            store
                .replace_file_symbols(
                    "vendored/copy.rs",
                    &[touring_code::ast::SymbolLocation::new(
                        "vendored/copy.rs",
                        "companion_probe_vendored_5b2d".to_string(),
                        1,
                        0,
                        true,
                    )],
                )
                .expect("inject excluded residue");
        }
        let out2 = cli_index_rebuild(&mut rt, &serde_json::json!({"dir": root.to_string_lossy()}));
        let again: serde_json::Value = serde_json::from_str(&out2).expect("rebuild json");
        assert_eq!(
            again["policy_purged_by_reason"]["companion_not_configured"], 1,
            "{out2}"
        );
        assert_eq!(
            again["policy_purged_by_reason"]["skipped_dir"], 1,
            "the excluded directory's residue is purged: {out2}"
        );
        assert_eq!(again["companion_roots"]["notes"], 4, "{out2}");
        assert_eq!(why(&mut rt, "@companion/notes/b.rs")["status"], "indexed");

        // Un-configure the companion: the next rebuild walks nothing there and
        // retires every row it had stored — the store never outlives the policy.
        configure("");
        let out3 = cli_index_rebuild(&mut rt, &serde_json::json!({"dir": root.to_string_lossy()}));
        let gone: serde_json::Value = serde_json::from_str(&out3).expect("rebuild json");
        assert_eq!(gone["companion_roots"], serde_json::json!({}), "{out3}");
        assert_eq!(
            gone["policy_purged_by_reason"]["companion_not_configured"], 4,
            "{out3}"
        );
        let after = why(&mut rt, "@companion/notes/b.rs");
        assert_eq!(
            (
                after["status"].as_str(),
                after["symbols"].as_u64(),
                after["residue"].as_bool()
            ),
            (Some("companion_not_configured"), Some(0), Some(false)),
            "{after}"
        );
        assert_eq!(
            why(&mut rt, "src/lib.rs")["status"],
            "indexed",
            "the project itself is untouched"
        );
    }
}

#[cfg(test)]
mod rebuild_containment_e2e {
    use super::cli_index_rebuild;
    use crate::runtime::HookRuntime;

    /// End-to-end over the REAL `cli_index_rebuild`: a symlink escaping the project
    /// contributes nothing to the index, an internal one still works, and a cycle
    /// terminates.
    ///
    /// This runs **in-process**. The `touring index rebuild` CLI verb dispatches to
    /// the daemon, so a CLI-level test would exercise whatever binary the daemon was
    /// started from — i.e. it would silently test the *previous* build. (Observed
    /// 2026-08-07: the stale daemon hung for 120 s on the cycle below, which is the
    /// defect this guard closes.)
    #[test]
    #[cfg(unix)]
    fn rebuild_indexes_inside_the_root_only_and_terminates_on_a_cycle() {
        let outside = tempfile::tempdir().expect("outside tmpdir");
        std::fs::write(
            outside.path().join("leaked.rs"),
            "pub fn canary_must_not_be_indexed_9f3a() {}",
        )
        .expect("write leaked");

        let proj = tempfile::tempdir().expect("project tmpdir");
        let root = proj.path();
        std::fs::create_dir_all(root.join("src/cyc")).expect("mkdir src/cyc");
        std::fs::write(root.join("src/lib.rs"), "pub fn legit_inside_9f3a() {}").expect("lib.rs");
        std::fs::write(
            root.join("src/cyc/deep.rs"),
            "pub fn legit_nested_9f3a() {}",
        )
        .expect("deep.rs");
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname=\"probe\"\nversion=\"0.0.0\"\nedition=\"2021\"\n",
        )
        .expect("Cargo.toml");

        // (1) escape, (2) cycle, (3) an internal alias that must keep working
        std::os::unix::fs::symlink(outside.path(), root.join("escape_hatch")).expect("escape");
        std::os::unix::fs::symlink(root.join("src/cyc"), root.join("src/cyc/loop")).expect("cycle");
        std::os::unix::fs::symlink(root.join("src"), root.join("src/cyc/alias")).expect("alias");

        let _serial = super::REBUILD_TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut rt = HookRuntime::new(root).expect("HookRuntime::new");
        // No timeout wrapper is needed: an unguarded walker never returns from here,
        // so the test harness hanging IS the failure signal.
        let out = cli_index_rebuild(&mut rt, &serde_json::json!({"dir": root.to_string_lossy()}));
        let v: serde_json::Value = serde_json::from_str(&out).expect("rebuild returns json");
        assert_eq!(v["errors"], 0, "rebuild reported errors: {out}");

        let store = rt
            .infra
            .symbol_store
            .as_ref()
            .expect("symbol_store initialised by HookRuntime::new");

        let leaked = store
            .find_symbol("canary_must_not_be_indexed_9f3a")
            .expect("find_symbol");
        assert!(
            leaked.is_empty(),
            "a symlink escaping the root leaked {} symbol(s) into the project index",
            leaked.len()
        );

        // Capability preserved: real files inside the root are still indexed.
        for sym in ["legit_inside_9f3a", "legit_nested_9f3a"] {
            assert!(
                !store.find_symbol(sym).expect("find_symbol").is_empty(),
                "{sym} inside the root must still be indexed — the guard removes the \
                 escape, not the capability"
            );
        }
    }
}

/// A1 (14/09/2026): the rebuild gives queued light commands a turn between
/// files. Without the yield a 15-minute analise rebuild held the project actor
/// and `memory store` from another session outlived its 15 s budget.
#[cfg(test)]
mod rebuild_yield_tests {
    use super::cli_index_rebuild;
    use crate::runtime::HookRuntime;
    use std::cell::Cell;
    use std::rc::Rc;

    #[test]
    fn the_rebuild_walk_yields_between_files() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        std::fs::create_dir_all(root.join("src")).expect("src");
        for i in 0..5 {
            std::fs::write(
                root.join(format!("src/m{i}.rs")),
                format!("pub fn f{i}() {{}}\n"),
            )
            .expect("module");
        }
        let _serial = super::REBUILD_TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut rt = HookRuntime::new(root).expect("runtime");
        let yields = Rc::new(Cell::new(0u32));
        let seen = Rc::clone(&yields);
        let open_tx = Rc::new(Cell::new(0u32));
        let open_seen = Rc::clone(&open_tx);
        let guard = touring_hook_runtime::actor_yield::install(Box::new(move |rt| {
            seen.set(seen.get() + 1);
            // A served handler must see committed state (A9): no transaction of
            // the rebuild may be open on the knowledge connection at a yield.
            if !rt.ctx.knowledge.conn_ref().is_autocommit() {
                open_seen.set(open_seen.get() + 1);
            }
        }));
        let out = cli_index_rebuild(
            &mut rt,
            &serde_json::json!({ "dir": root.to_string_lossy() }),
        );
        drop(guard);
        assert_eq!(
            open_tx.get(),
            0,
            "a yield ran inside an open transaction: {out}"
        );
        let scanned: u32 = serde_json::from_str::<serde_json::Value>(&out)
            .ok()
            .and_then(|v| v["total_files_scanned"].as_u64())
            .and_then(|n| u32::try_from(n).ok())
            .unwrap_or(0);
        assert!(scanned >= 5, "the walk scanned the 5 modules: {out}");
        // One yield per file, plus the post-walk phase boundaries (before and
        // after the inferred-consumer transaction, before the search commit):
        // `index status` timed out while an analise rebuild sealed (14/09/2026).
        assert!(
            yields.get() >= scanned + 3,
            "{} yields for a walk over {scanned} files: {out}",
            yields.get()
        );
    }

    /// A5 (cross-audit 14/09/2026): the sweep that retires deleted files held the
    /// actor for its whole run. Each purge now gives queued commands a turn.
    #[test]
    fn the_sweep_yields_before_every_file_it_purges() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        std::fs::create_dir_all(root.join("src")).expect("src");
        for i in 0..5 {
            std::fs::write(
                root.join(format!("src/m{i}.rs")),
                format!("pub fn f{i}() {{}}\n"),
            )
            .expect("module");
        }
        let _serial = super::REBUILD_TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut rt = HookRuntime::new(root).expect("runtime");
        let payload = serde_json::json!({ "dir": root.to_string_lossy() });
        let _ = cli_index_rebuild(&mut rt, &payload);
        for i in 1..5 {
            std::fs::remove_file(root.join(format!("src/m{i}.rs"))).expect("rm");
        }
        let yields = Rc::new(Cell::new(0u32));
        let seen = Rc::clone(&yields);
        let open_tx = Rc::new(Cell::new(0u32));
        let open_seen = Rc::clone(&open_tx);
        let guard = touring_hook_runtime::actor_yield::install(Box::new(move |rt| {
            seen.set(seen.get() + 1);
            if !rt.ctx.knowledge.conn_ref().is_autocommit() {
                open_seen.set(open_seen.get() + 1);
            }
        }));
        let out = cli_index_rebuild(&mut rt, &payload);
        drop(guard);
        assert_eq!(
            open_tx.get(),
            0,
            "a sweep yield ran inside an open transaction: {out}"
        );
        let v: serde_json::Value = serde_json::from_str(&out).expect("json");
        let count = |key: &str| {
            v[key]
                .as_u64()
                .and_then(|n| u32::try_from(n).ok())
                .unwrap_or(0)
        };
        let (scanned, purged, phantoms) = (
            count("total_files_scanned"),
            count("stale_files_purged"),
            count("phantom_modules_purged"),
        );
        assert!(purged >= 4, "the 4 deleted modules were swept: {out}");
        assert!(
            yields.get() >= scanned + 3 + purged + phantoms,
            "{} yields for {scanned} walked, {purged} swept and {phantoms} phantom modules: {out}",
            yields.get()
        );
    }
}

/// Cross-audit 14/09/2026 (B7): a `--dir` that is a symlink inside the project is
/// walked at its canonical location, so its files keep the one key a full walk
/// gives them.
#[cfg(all(test, unix))]
mod rebuild_symlink_dir_tests {
    use super::cli_index_rebuild;
    use crate::runtime::HookRuntime;

    #[test]
    fn a_symlinked_dir_is_stored_under_its_real_path() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        std::fs::create_dir_all(root.join(".git")).expect("marker");
        std::fs::create_dir_all(root.join("src/real")).expect("real");
        std::fs::write(root.join("src/real/a.rs"), "pub fn via_real_b7() {}\n").expect("a.rs");
        std::os::unix::fs::symlink(root.join("src/real"), root.join("src/link")).expect("link");
        let _serial = super::REBUILD_TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut rt = HookRuntime::new(root).expect("runtime");
        let out = cli_index_rebuild(&mut rt, &serde_json::json!({ "dir": "src/link" }));
        let files = rt
            .symbol_store()
            .expect("store")
            .get_indexed_files(usize::MAX)
            .expect("files");
        assert!(
            files.iter().any(|f| f == "src/real/a.rs"),
            "{files:?} {out}"
        );
        assert!(
            !files.iter().any(|f| f.starts_with("src/link/")),
            "the link's spelling is a second key for the same file: {files:?}"
        );
    }
}

/// 14/09/2026: a rebuild whose `--dir` was outside the project walked nothing and
/// sealed "complete, 0 files" over the project's real index — three times per
/// test run, from the WorktreeCreate handler spawning `index rebuild --dir <tmp>`
/// with the touring workspace as cwd. A rebuild seals the PROJECT generation, so
/// only a walk of the whole project may seal it.
#[cfg(test)]
mod rebuild_scope_tests {
    use super::cli_index_rebuild;
    use crate::runtime::HookRuntime;

    fn project() -> (tempfile::TempDir, HookRuntime) {
        let tmp = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(tmp.path().join("src/inner")).expect("src");
        std::fs::write(tmp.path().join("src/lib.rs"), "pub fn top() {}\n").expect("lib");
        std::fs::write(tmp.path().join("src/inner/mod.rs"), "pub fn inner() {}\n").expect("mod");
        let rt = HookRuntime::new(tmp.path()).expect("runtime");
        (tmp, rt)
    }

    fn generation_state(rt: &HookRuntime) -> String {
        rt.ctx
            .knowledge
            .index_generation_state(std::process::id())
            .map(|g| g.state.to_string())
            .unwrap_or_else(|e| format!("error: {e}"))
    }

    #[test]
    fn a_dir_outside_the_project_is_refused_without_opening_a_generation() {
        let (_tmp, mut rt) = project();
        let elsewhere = tempfile::tempdir().expect("elsewhere");
        let _serial = super::REBUILD_TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        let out = cli_index_rebuild(&mut rt, &serde_json::json!({ "dir": elsewhere.path() }));
        let v: serde_json::Value = serde_json::from_str(&out).expect("json");

        assert_eq!(v["error_kind"], "dir_outside_project", "{out}");
        assert_eq!(
            generation_state(&rt),
            "none",
            "no generation was opened: {out}"
        );
    }

    #[test]
    fn a_missing_dir_is_refused_without_opening_a_generation() {
        let (tmp, mut rt) = project();
        let _serial = super::REBUILD_TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        let out = cli_index_rebuild(
            &mut rt,
            &serde_json::json!({ "dir": tmp.path().join("does-not-exist") }),
        );
        let v: serde_json::Value = serde_json::from_str(&out).expect("json");

        assert_eq!(v["error_kind"], "dir_not_found", "{out}");
        assert_eq!(generation_state(&rt), "none", "{out}");
    }

    #[test]
    fn a_subdirectory_walk_never_seals_the_project_generation() {
        let (tmp, mut rt) = project();
        let _serial = super::REBUILD_TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        let out = cli_index_rebuild(
            &mut rt,
            &serde_json::json!({ "dir": tmp.path().join("src/inner") }),
        );
        let v: serde_json::Value = serde_json::from_str(&out).expect("json");

        assert!(
            v["files_indexed"].as_u64().unwrap_or(0) >= 1,
            "the subdirectory was walked: {out}"
        );
        assert_eq!(v["generation"]["state"], "scoped", "{out}");
        assert_eq!(generation_state(&rt), "none", "{out}");

        let full = cli_index_rebuild(&mut rt, &serde_json::json!({ "dir": tmp.path() }));
        let v: serde_json::Value = serde_json::from_str(&full).expect("json");
        assert_eq!(
            v["generation"]["state"], "complete",
            "a whole-project walk still seals: {full}"
        );
        assert_eq!(generation_state(&rt), "complete");
    }
}

#[cfg(test)]
mod status_off_actor_tests {
    //! Decision 3-A (14/09/2026): `index status` is served from committed state
    //! through read-only connections, outside the project actor. The answer must
    //! equal the actor's, and must not wait for a writer that holds a transaction
    //! open — the sealing phase of a rebuild did exactly that and a status call
    //! timed out at 15 s.
    use super::*;
    use std::time::{Duration, Instant};

    fn status_key(root: &Path) -> String {
        crate::shared::query_cache::make_key(root, "cli_index_status", "status")
    }

    #[test]
    fn a_project_without_databases_falls_back_to_the_actor() {
        let tmp = tempfile::tempdir().expect("tmp");
        crate::shared::query_cache::invalidate(&status_key(tmp.path()));
        assert_eq!(index_status_from_disk(tmp.path()), None);
    }

    #[test]
    fn the_disk_answer_equals_the_actor_answer() {
        let tmp = tempfile::tempdir().expect("tmp");
        let root = tmp.path();
        std::fs::create_dir_all(root.join("src")).expect("src");
        std::fs::write(
            root.join("src/lib.rs"),
            "pub fn status_probe_off_actor_3a() {}\n",
        )
        .expect("lib.rs");
        let _serial = super::REBUILD_TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut rt = HookRuntime::new(root).expect("HookRuntime::new");
        let rebuilt = cli_index_rebuild(&mut rt, &serde_json::json!({ "dir": root }));
        assert!(rebuilt.contains("\"complete\""), "{rebuilt}");

        crate::shared::query_cache::invalidate(&status_key(root));
        let from_actor: serde_json::Value =
            serde_json::from_str(&cli_index_status(&mut rt, &serde_json::json!({}))).expect("json");
        crate::shared::query_cache::invalidate(&status_key(root));
        let from_disk: serde_json::Value =
            serde_json::from_str(&index_status_from_disk(root).expect("databases exist"))
                .expect("json");
        assert_eq!(from_disk, from_actor);
        assert!(
            from_disk["symbol_count"].as_u64().unwrap_or(0) >= 1,
            "{from_disk}"
        );
        assert_eq!(
            from_disk["index_generation"]["state"], "complete",
            "{from_disk}"
        );
    }

    #[test]
    fn an_open_write_transaction_never_blocks_the_disk_answer() {
        let tmp = tempfile::tempdir().expect("tmp");
        let root = tmp.path();
        let rt = HookRuntime::new(root).expect("HookRuntime::new");
        let sealed = rt
            .ctx
            .knowledge
            .begin_index_generation(std::process::id())
            .expect("generation");
        rt.ctx
            .knowledge
            .finish_index_generation(sealed, 1, 1, true, None)
            .expect("seal");

        // Another connection opens a write transaction and leaves it open — the
        // shape of the inferred-consumer transaction during a rebuild's seal.
        let writer = rusqlite::Connection::open(
            touring_foundation::TouringConfig::knowledge_db_canonical(root),
        )
        .expect("writer");
        writer.execute_batch("BEGIN IMMEDIATE").expect("begin");
        writer
            .execute(
                "INSERT INTO index_generation (state, owner_pid) VALUES ('building', ?1)",
                [i64::from(std::process::id())],
            )
            .expect("uncommitted row");

        crate::shared::query_cache::invalidate(&status_key(root));
        let started = Instant::now();
        let during: serde_json::Value =
            serde_json::from_str(&index_status_from_disk(root).expect("answer")).expect("json");
        assert!(
            started.elapsed() < Duration::from_secs(1),
            "the read waited for the writer: {:?}",
            started.elapsed()
        );
        assert_eq!(
            during["index_generation"]["state"], "complete",
            "an uncommitted generation is not visible: {during}"
        );

        writer.execute_batch("COMMIT").expect("commit");
        crate::shared::query_cache::invalidate(&status_key(root));
        let after: serde_json::Value =
            serde_json::from_str(&index_status_from_disk(root).expect("answer")).expect("json");
        assert_eq!(
            after["index_generation"]["state"], "building",
            "the committed generation, owned by this process, is building: {after}"
        );
    }
    /// Cross-audit 14/09/2026 (A6): `building` is never cached, so the first call
    /// after the seal sees the seal — a racing reader used to pin `building` 60 s.
    #[test]
    fn a_building_answer_is_never_cached() {
        let tmp = tempfile::tempdir().expect("tmp");
        let root = tmp.path();
        let rt = HookRuntime::new(root).expect("HookRuntime::new");
        let id = rt
            .ctx
            .knowledge
            .begin_index_generation(std::process::id())
            .expect("generation");
        crate::shared::query_cache::invalidate(&status_key(root));
        let during = index_status_from_disk(root).expect("answer");
        assert!(during.contains("\"building\""), "{during}");
        assert!(
            crate::shared::query_cache::get(&status_key(root)).is_none(),
            "a building answer was cached"
        );
        rt.ctx
            .knowledge
            .finish_index_generation(id, 1, 1, true, None)
            .expect("seal");
        let after = index_status_from_disk(root).expect("answer");
        assert!(after.contains("\"complete\""), "{after}");
    }

    /// Cross-audit 14/09/2026 (A7): a generation the reader cannot read falls back
    /// to the actor (which reports the error) instead of a cached `unknown`.
    #[test]
    fn an_unreadable_generation_falls_back_to_the_actor() {
        let tmp = tempfile::tempdir().expect("tmp");
        let root = tmp.path();
        let _rt = HookRuntime::new(root).expect("HookRuntime::new");
        rusqlite::Connection::open(touring_foundation::TouringConfig::knowledge_db_canonical(
            root,
        ))
        .expect("open")
        .execute_batch(
            "DROP TABLE IF EXISTS index_generation;
                 CREATE TABLE index_generation (id INTEGER PRIMARY KEY);",
        )
        .expect("break the table");
        crate::shared::query_cache::invalidate(&status_key(root));
        assert_eq!(index_status_from_disk(root), None);
        assert!(crate::shared::query_cache::get(&status_key(root)).is_none());
    }
}
