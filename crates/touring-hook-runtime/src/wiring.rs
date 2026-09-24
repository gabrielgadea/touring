//! Wiring Intelligence — tracks pub symbol → consumer connections.
//!
//! Provides CRUD operations on the `wiring_map` table to detect orphan
//! modules (pub symbols exported but never imported by any consumer).

use std::collections::{HashMap, HashSet};

use rusqlite::params;
use rustc_hash::FxHashMap;

pub mod hypergraph;

use hypergraph::{FeatureGateHyperedge, HyperGraph, MultiImportHyperedge};

use crate::knowledge::FileKnowledgeDB;

// Phase C carve (2026-06-10): the wiring persistence layer (inherent
// `impl FileKnowledgeDB` + its row/diagnostic structs + path-canonicalization
// helpers) moved to `touring_hooks_core::knowledge_wiring` — inherent impls
// must live in the crate that defines the type. Re-exported so every
// historical path (`crate::wiring::WiringEntry`, …) keeps resolving.
pub use touring_hooks_core::knowledge_wiring::{
    ModuleWiringStatus, WiringDbDiagnostic, WiringEntry, WiringModuleAggregateRow,
};

// Wave R+C I1 (2026-06-10): the `repair_consumer_tracking` delegating wrapper
// was removed — zero external callers (REGRA #0); the repair entry point is
// `cli_handlers_wiring_repair::repair_wiring_consumer_tracking` directly.
// This also dissolved the only production wiring→cli edge, unblocking the
// wiring engine's descent to touring-hook-runtime.

// =============================================================================
// Impact Analysis — F1: touring wiring impact <symbol>
// =============================================================================

/// Result of transitive impact analysis for a symbol.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ImpactResult {
    /// Symbol whose transitive impact was analyzed.
    pub symbol: String,
    /// Number of symbols that consume this symbol directly.
    pub direct_consumers: usize,
    /// Total number of transitive consumers reached via BFS.
    pub total_transitive: usize,
    /// Greatest depth reached in the consumer graph.
    pub max_depth: usize,
    /// Individual consumer paths discovered during the traversal.
    pub paths: Vec<ImpactPath>,
}

/// A single consumer path in a transitive impact traversal.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ImpactPath {
    /// Module that contains the consuming symbol.
    pub consumer_module: String,
    /// Symbol that consumes the analyzed symbol along this path.
    pub consumer_symbol: String,
    /// Depth of this consumer from the analyzed symbol.
    pub depth: usize,
    /// Number of outgoing edges from the consuming symbol.
    pub fan_out: usize,
    /// Kind of dependency edge (e.g. direct call, re-export).
    pub path_type: String,
}

/// Compute transitive impact of changes to a symbol.
///
/// Uses BFS on the wiring_map consumer edges. Cycles are prevented via visited set.
pub fn compute_impact(db: &FileKnowledgeDB, symbol: &str, max_depth: usize) -> ImpactResult {
    // Direct consumers: query wiring_map without gating on a producer row.
    // Previously gated on finding a `consumer_file IS NULL` (producer) row first,
    // which caused symbols tracked only as consumers (no self-producer row) to return
    // 0 direct consumers even when real consumer rows existed. Fix: query unconditionally.
    let direct: Vec<(String, String)> = {
        let mut result = Vec::new();
        if let Ok(mut stmt) = db.conn_ref().prepare(
            "SELECT DISTINCT consumer_file, symbol_name FROM wiring_map
             WHERE symbol_name = ?1 AND consumer_file IS NOT NULL",
        ) && let Ok(rows) = stmt.query_map(params![symbol], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        }) {
            result = rows.filter_map(|r| r.ok()).collect();
        }
        result
    };

    let direct_consumers = direct.len();
    let mut visited: HashSet<String> = HashSet::new();
    let mut paths: Vec<ImpactPath> = Vec::new();

    // BFS transitively walk consumers
    for (consumer_file, consumer_symbol) in &direct {
        visited.insert(format!("{}::{}", consumer_file, consumer_symbol));
        paths.push(ImpactPath {
            consumer_module: consumer_file.clone(),
            consumer_symbol: consumer_symbol.clone(),
            depth: 1,
            fan_out: count_fan_out(db, consumer_file, consumer_symbol),
            path_type: "direct".to_string(),
        });
        walk_consumers_bfs(
            db,
            consumer_file,
            consumer_symbol,
            1,
            max_depth,
            &mut visited,
            &mut paths,
        );
    }

    let total_transitive = paths.len();
    let max_depth_reached = paths.iter().map(|p| p.depth).max().unwrap_or(0);

    ImpactResult {
        symbol: symbol.to_string(),
        direct_consumers,
        total_transitive,
        max_depth: max_depth_reached,
        paths,
    }
}

fn walk_consumers_bfs(
    db: &FileKnowledgeDB,
    module_file: &str,
    symbol_name: &str,
    depth: usize,
    max_depth: usize,
    visited: &mut HashSet<String>,
    paths: &mut Vec<ImpactPath>,
) {
    if depth >= max_depth {
        return;
    }

    // Find symbols that this (module, symbol) consumer calls
    let consumers: Vec<(String, String)> = {
        let stmt_opt = db.conn_ref().prepare(
            "SELECT DISTINCT consumer_file, symbol_name FROM wiring_map
             WHERE module_file = ?1
               AND symbol_name = ?2
               AND consumer_file IS NOT NULL",
        );
        if let Ok(mut stmt) = stmt_opt {
            stmt.query_map(params![module_file, symbol_name], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map(|rows| rows.filter_map(|r| r.ok()).collect())
            .unwrap_or_default()
        } else {
            Vec::new()
        }
    };

    for (consumer_file, consumer_symbol) in consumers {
        let key = format!("{}::{}", consumer_file, consumer_symbol);
        if visited.contains(&key) {
            continue;
        }
        visited.insert(key);
        let next_depth = depth + 1;
        paths.push(ImpactPath {
            consumer_module: consumer_file.clone(),
            consumer_symbol: consumer_symbol.clone(),
            depth: next_depth,
            fan_out: count_fan_out(db, &consumer_file, &consumer_symbol),
            path_type: "transitive".to_string(),
        });
        walk_consumers_bfs(
            db,
            &consumer_file,
            &consumer_symbol,
            next_depth,
            max_depth,
            visited,
            paths,
        );
    }
}

fn count_fan_out(db: &FileKnowledgeDB, module_file: &str, symbol_name: &str) -> usize {
    db.conn_ref()
        .query_row(
            "SELECT COUNT(*) FROM wiring_map WHERE module_file = ?1 AND symbol_name = ?2 AND consumer_file IS NOT NULL",
            params![module_file, symbol_name],
            |r| r.get::<_, i64>(0),
        )
        .unwrap_or(0) as usize
}

// =============================================================================
// Cycle Detection — F2: touring wiring cycles
// =============================================================================

/// A detected dependency cycle.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Cycle {
    /// Sequential identifier of this cycle within the detection run.
    pub id: usize,
    /// Modules forming the cycle, in traversal order.
    pub modules: Vec<String>,
    /// Number of modules in the cycle.
    pub depth: usize,
    /// Severity classification derived from the cycle depth.
    pub severity: String,
}

/// Find all dependency cycles in the module wiring graph using Tarjan's SCC.
///
/// Uses the wiring_map consumer edges to build a directed graph of module dependencies.
/// Resolve a `wiring_map` path against likely roots and report whether it
/// currently exists on disk.
///
/// Paths in `wiring_map` are heterogeneous: absolute, relative to `$HOME`
/// (e.g. `.claude/rust/crates/...`), or relative to a project root. A05 uses
/// this to prune **phantom edges** from the cycle graph — rows left behind by
/// absorbed crates (`touring-rule-engine`, `touring-definitions`) and
/// cross-project pollution (`../../../ThemeContext`) whose files no longer
/// exist. Resolution is conservative (tries every plausible base) so a real
/// file is never mistaken for a phantom.
fn path_exists_resolved(p: &str, root: Option<&str>) -> bool {
    use std::path::Path;
    let path = Path::new(p);
    if path.is_absolute() {
        return path.exists();
    }
    if let Some(r) = root
        && Path::new(r).join(p).exists()
    {
        return true;
    }
    if let Some(home) = std::env::var_os("HOME")
        && Path::new(&home).join(p).exists()
    {
        return true;
    }
    path.exists()
}

/// Detect dependency cycles in the wiring graph, optionally filtered to a
/// single workspace root.
///
/// `workspace_root_filter` semantics (PLT-2026-06-02):
/// - `Some(root)` — only return cycles where **every** node in the cycle has
///   `workspace_root = root` OR `workspace_root IS NULL` (legacy pre-migration
///   rows are treated as "matches any root", preserving back-compat).
/// - `None` — return cycles from the entire `wiring_map` (legacy behavior).
///
/// # Rationale
/// The wiring DB has 4 projects tracked by the daemon; the konverter
/// workspace was seeing a 136-module false-positive cycle caused by
/// `abs_paths` rows from a sibling project (`analise/kazuba-rust-core`)
/// leaking into konverter's view. Filtering by `workspace_root` scopes
/// the cycle report to the workspace the user is actually in.
pub fn find_all_cycles(
    db: &FileKnowledgeDB,
    workspace_root_filter: Option<&str>,
    prune_nonexistent: bool,
    trusted_only: bool,
) -> Vec<Cycle> {
    // Build adjacency list from wiring_map (module_file → consumer_file)
    let mut adjacency: HashMap<String, Vec<String>> = HashMap::new();
    let mut all_modules: HashSet<String> = HashSet::new();

    // Compose the workspace-root predicate. The legacy `None` case skips the
    // filter entirely (back-compat for callers that don't know their root).
    let ws_predicate = match workspace_root_filter {
        Some(_) => " AND (workspace_root = ?1 OR workspace_root IS NULL)",
        None => "",
    };
    // H2 (2026-08-12): trusted mode excludes the name-matching heuristic
    // (`ast_inferred`, ~66% of edges) — measured: SCCs go 7 (917-module giant)
    // → 0 when it is excluded. What remains is import-resolved + SCIP
    // type-resolved truth.
    let trust_predicate = if trusted_only {
        " AND contract_source != 'ast_inferred'"
    } else {
        ""
    };
    let sql = format!(
        "SELECT DISTINCT module_file, consumer_file, workspace_root FROM wiring_map
         WHERE module_file IS NOT NULL
           AND consumer_file IS NOT NULL
           AND module_file != consumer_file{ws_predicate}{trust_predicate}",
    );

    let rows: Vec<(String, String, Option<String>)> = {
        let stmt_opt = db.conn_ref().prepare(&sql);
        if let Ok(mut stmt) = stmt_opt {
            let row_map = |row: &rusqlite::Row<'_>| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                ))
            };
            // Bind the optional workspace_root parameter (index 4 in the ?4
            // form). Branching here keeps each rusqlite::ToSql reference's
            // lifetime tied to a stable binding — a `Vec<&dyn ToSql>` would
            // need to outlive the temporaries the match arms would create.
            let result = match workspace_root_filter {
                Some(root) => stmt
                    .query_map([root], row_map)
                    .map(|rows| rows.filter_map(|r| r.ok()).collect()),
                None => stmt
                    .query_map([], row_map)
                    .map(|rows| rows.filter_map(|r| r.ok()).collect()),
            };
            result.unwrap_or_default()
        } else {
            Vec::new()
        }
    };

    for (module, consumer, _workspace_root) in rows {
        // A05: drop phantom edges whose endpoints no longer exist on disk
        // (absorbed crates + cross-project pollution). Opt-in so unit tests
        // that use fictional paths keep their legacy behavior.
        if prune_nonexistent
            && (!path_exists_resolved(&module, workspace_root_filter)
                || !path_exists_resolved(&consumer, workspace_root_filter))
        {
            continue;
        }
        all_modules.insert(module.clone());
        all_modules.insert(consumer.clone());
        adjacency.entry(module).or_default().push(consumer);
    }

    // Tarjan's SCC
    let mut index: usize = 0;
    let mut stack: Vec<String> = Vec::new();
    let mut on_stack: HashSet<String> = HashSet::new();
    let mut indices: HashMap<String, usize> = HashMap::new();
    let mut lowlinks: HashMap<String, usize> = HashMap::new();
    let mut cycles: Vec<Vec<String>> = Vec::new();

    #[allow(clippy::too_many_arguments)]
    fn strong_connect(
        node: &str,
        adjacency: &HashMap<String, Vec<String>>,
        index: &mut usize,
        stack: &mut Vec<String>,
        on_stack: &mut HashSet<String>,
        indices: &mut HashMap<String, usize>,
        lowlinks: &mut HashMap<String, usize>,
        cycles: &mut Vec<Vec<String>>,
    ) {
        indices.insert(node.to_string(), *index);
        lowlinks.insert(node.to_string(), *index);
        *index += 1;
        stack.push(node.to_string());
        on_stack.insert(node.to_string());

        if let Some(neighbors) = adjacency.get(node) {
            for neighbor in neighbors {
                if !indices.contains_key(neighbor) {
                    strong_connect(
                        neighbor, adjacency, index, stack, on_stack, indices, lowlinks, cycles,
                    );
                    let nl = *lowlinks
                        .get(neighbor)
                        .expect("Tarjan: neighbor lowlink set by strong_connect");
                    let my_ll = *lowlinks.entry(node.to_string()).or_insert(0);
                    if nl < my_ll {
                        lowlinks.insert(node.to_string(), nl);
                    }
                } else if on_stack.contains(neighbor) {
                    let nl = *indices
                        .get(neighbor)
                        .expect("Tarjan: on-stack neighbor has an index");
                    let my_ll = *lowlinks.entry(node.to_string()).or_insert(0);
                    if nl < my_ll {
                        lowlinks.insert(node.to_string(), nl);
                    }
                }
            }
        }

        let my_lowlink = *lowlinks
            .get(node)
            .expect("Tarjan: node lowlink set at entry");
        if my_lowlink == *indices.get(node).expect("Tarjan: node index set at entry") {
            let mut scc: Vec<String> = Vec::new();
            loop {
                let w = stack.pop().expect("Tarjan: stack non-empty until SCC root");
                on_stack.remove(&w);
                scc.push(w.clone());
                if w == node {
                    break;
                }
            }
            if scc.len() > 1 {
                cycles.push(scc);
            }
        }
    }

    for module in &all_modules {
        if !indices.contains_key(module) {
            strong_connect(
                module,
                &adjacency,
                &mut index,
                &mut stack,
                &mut on_stack,
                &mut indices,
                &mut lowlinks,
                &mut cycles,
            );
        }
    }

    // Convert to Cycle structs
    cycles
        .into_iter()
        .enumerate()
        .map(|(i, mut modules)| {
            modules.reverse();
            let depth = modules.len();
            let severity = if depth >= 4 {
                "high".to_string()
            } else if depth >= 2 {
                "medium".to_string()
            } else {
                "low".to_string()
            };
            Cycle {
                id: i + 1,
                modules,
                depth,
                severity,
            }
        })
        .collect()
}

/// Whether `language` declares an API the wiring map tracks: data and markup
/// files do not. The rebuild, the edit path and the read path share this
/// predicate.
///
/// Renamed from `is_code_language` (cross-audit 14/09/2026, B9): the name was
/// also `touring_hooks_shared::detect_language::is_code_language`, an ALLOW-list
/// with the opposite default for an unknown language, and `post_read` kept a
/// third, inline copy.
#[must_use]
pub fn declares_wireable_api(language: &str) -> bool {
    !matches!(
        language,
        "toml" | "json" | "yaml" | "markdown" | "html" | "css"
    )
}

/// Register every public symbol of `symbols` as a producer row of `file_path`,
/// with its REAL visibility: `pub(crate)` is stored as `crate`, never as
/// `public`, because the orphan queries count only `visibility = 'public'`.
/// Does not clear — the caller owns the clear and the transaction. Returns the
/// rows actually written (a refused file or an existing row counts zero).
pub fn register_public_symbols(
    db: &FileKnowledgeDB,
    file_path: &str,
    symbols: &[touring_code::ast::symbols::Symbol],
) -> u32 {
    let mut registered = 0;
    for sym in symbols.iter().filter(|sym| sym.is_public) {
        let visibility = sym
            .visibility
            .as_ref()
            .map_or("public", touring_code::ast::Visibility::as_str);
        if db
            .register_pub_symbol_counted(
                file_path,
                &sym.name,
                sym.kind.as_str(),
                visibility,
                crate::knowledge_wiring::WiringOrigin::AstDeclared,
            )
            .unwrap_or(false)
        {
            registered += 1;
        }
    }
    registered
}

/// Replace the producer rows of `file_path` with the public symbols its current
/// `content` declares — the same extraction and visibility the rebuild uses.
///
/// Returns `None` and touches nothing when the language declares no API or the
/// file cannot be parsed: rows that cannot be re-derived are kept, never
/// cleared. Clearing and re-registering from the stored `symbols_json` — which
/// carries no visibility — wiped the producers of every edited file
/// (14/09/2026).
pub fn refresh_file_producers(
    db: &FileKnowledgeDB,
    file_path: &str,
    language: &str,
    content: &str,
) -> Option<u32> {
    if !declares_wireable_api(language) {
        return None;
    }
    // A companion file produces nothing (decision 1-A): the write gate would
    // refuse every row anyway, so its residue is cleared and nothing is counted.
    if touring_foundation::config::is_companion_key(file_path) {
        let _ = db.clear_wiring(file_path);
        return Some(0);
    }
    let symbols = crate::ast_bridge::extract_enriched_symbols(content, file_path)?;
    let _ = db.clear_wiring(file_path);
    let registered = register_public_symbols(db, file_path, &symbols);
    if language == "python" {
        retarget_vanished_consumers(db, file_path, content, &symbols);
    }
    Some(registered)
}

/// Consumer rows other files hold on the Python module `file_path` for a symbol
/// it no longer defines.
///
/// A module split moves `No` to a sibling, and the edges written while the old
/// module defined it keep pointing there — where `impact` finds them by name and
/// `orphans` never looks (18/09/2026, analise: `grafo_memoria.py::No` kept the
/// consumer it had on 16/09 after the split). The consumers did not change, so
/// nothing re-reads them; their edges are settled here instead. A symbol the file
/// still imports is forwarded: its edges move to the module that defines it, or
/// stay when the chain cannot be followed (no proof either way). A symbol the file
/// neither defines nor imports is gone, and so are its edges.
fn retarget_vanished_consumers(
    db: &FileKnowledgeDB,
    file_path: &str,
    content: &str,
    symbols: &[touring_code::ast::symbols::Symbol],
) {
    let Some(root) = db.workspace_root() else {
        return;
    };
    let stale = consumers_of_undefined_symbols(db, file_path, symbols);
    if stale.is_empty() {
        return;
    }
    let abs_module = std::path::Path::new(root).join(file_path);
    let abs_module = abs_module.to_string_lossy();
    let imported: HashSet<String> = crate::ast_bridge::extract_file_imports(content, &abs_module)
        .into_iter()
        .flat_map(|(_, names)| names)
        .map(|name| {
            name.split(" as ")
                .next()
                .unwrap_or_default()
                .trim()
                .to_string()
        })
        .collect();
    for (symbol, consumer, line, source) in stale {
        if imported.contains(&symbol) {
            let definer = crate::symbol_extractors::definer_module(&abs_module, &symbol, None);
            if definer == abs_module {
                continue;
            }
            let _ = db.record_consumer_with_origin(
                &definer,
                &symbol,
                &consumer,
                line,
                crate::knowledge_wiring::WiringOrigin::from_contract_source(&source),
            );
        }
        let _ = db.conn_ref().execute(
            "DELETE FROM wiring_map WHERE module_file = ?1 AND symbol_name = ?2 AND consumer_file = ?3",
            params![file_path, symbol, consumer],
        );
    }
    FileKnowledgeDB::invalidate_wiring_modules_cache();
}

/// Other files' consumer rows on `file_path` whose symbol it does not define:
/// `(symbol, consumer, import_line, contract_source)`.
fn consumers_of_undefined_symbols(
    db: &FileKnowledgeDB,
    file_path: &str,
    symbols: &[touring_code::ast::symbols::Symbol],
) -> Vec<(String, String, Option<i64>, String)> {
    let defined: HashSet<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
    let rows: Vec<(String, String, Option<i64>, String)> = db
        .conn_ref()
        .prepare(
            "SELECT symbol_name, consumer_file, import_line, contract_source FROM wiring_map
             WHERE module_file = ?1 AND consumer_file IS NOT NULL AND consumer_file != module_file
               AND symbol_name != '*'",
        )
        .and_then(|mut stmt| {
            stmt.query_map(params![file_path], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
            })?
            .collect()
        })
        .unwrap_or_default();
    rows.into_iter()
        .filter(|(symbol, ..)| !defined.contains(symbol.as_str()))
        .collect()
}

/// The consumer rows a file's imports make, resolved exactly as the rebuild
/// resolves them. Each `(module, symbols)` the file's language extractor yields
/// goes through `resolve_import_path_with_source`; the edge is credited to the
/// module that DEFINES the symbol (`definer_module`, re-exports followed); an
/// import that resolves nowhere is recorded unresolved, with its class. Returns
/// the edges recorded.
///
/// One pass for the rebuild and the hook path (C08). Until 18/09/2026 the hook
/// path read `imports_json` through a `::` splitter — Rust syntax — so a Python
/// or TypeScript file edited or ingested lost every import edge until the next
/// rebuild: the analise split `grafo_memoria.py`, ingested the five files, and 12
/// of 13 symbols its siblings import from `grafo_modelo.py` read as orphans.
pub fn record_import_consumers(
    db: &FileKnowledgeDB,
    rel_path: &str,
    abs_path: &str,
    language: &str,
    content: &str,
) -> usize {
    let _ = db.clear_unresolved_for_consumer(rel_path);
    let mut recorded = 0;
    // Only the imports that USE what they name: a re-export or a dead import
    // is not a consumer (18/09/2026, Gabriel).
    for (module_path, imported_symbols) in
        crate::ast_bridge::extract_consumed_imports(content, abs_path)
    {
        let Some(module_file) = crate::symbol_extractors::resolve_import_path_with_source(
            &module_path,
            language,
            Some(abs_path),
        ) else {
            // Classified in the workspace of the same file the attempt used, so
            // the verdict never drifts from the attempt.
            let class = crate::symbol_extractors::classify_unresolved(&module_path, Some(abs_path));
            for symbol_name in &imported_symbols {
                let _ = db.record_unresolved_import_classified(
                    &module_path,
                    symbol_name,
                    rel_path,
                    None,
                    language,
                    class.as_str(),
                );
            }
            continue;
        };
        for symbol_name in &imported_symbols {
            // 24/09/2026 (touring-36; doctor wiring_diagnostic kind_unknown=8):
            // (2) a scope keyword is never a symbol — it is born ScopeKeyword.
            if matches!(symbol_name.as_str(), "super" | "self" | "crate" | "Self") {
                let _ = db.record_unresolved_import_classified(
                    &module_path,
                    symbol_name,
                    rel_path,
                    None,
                    language,
                    crate::symbol_extractors::UnresolvedClass::ScopeKeyword.as_str(),
                );
                continue;
            }
            // The import resolved to a MODULE; the producer row lives wherever
            // the symbol is DEFINED, so a façade is never credited with a
            // consumer it only forwards.
            let definer =
                crate::symbol_extractors::definer_module(&module_file, symbol_name, Some(abs_path));
            // (3) a definer that is the consumer file itself is self-reference —
            // the self_refs pass owns that class (pub → internal_only, private
            // → nothing); this pass records no edge for it.
            if definer == rel_path {
                continue;
            }
            // (1) a fallback is not a resolution: only a GENUINE definer — a
            // producer row, or a definition the chain/on-disk facts reach —
            // earns an edge. Everything else is an unresolved import with a
            // class, never an `ast_resolved` row without a producer: the
            // phantom no repair can ever clear.
            let genuine = crate::symbol_extractors::definer_module_opt(
                &module_file,
                symbol_name,
                Some(abs_path),
            )
            .is_some()
                || db.find_producer_modules_for_qualified(
                    &[(definer.clone(), symbol_name.clone())],
                    None,
                )
                .map(|found| !found.is_empty())
                .unwrap_or(true);
            if genuine
                && db
                    .record_consumer(&definer, symbol_name, rel_path, None)
                    .is_ok()
            {
                recorded += 1;
            } else if !genuine {
                let _ = db.record_unresolved_import_classified(
                    &module_path,
                    symbol_name,
                    rel_path,
                    None,
                    language,
                    crate::symbol_extractors::UnresolvedClass::WorkspaceUnresolved.as_str(),
                );
            }
        }
    }
    recorded
}

/// Option B (19/09/2026, Gabriel): every segment of a Rust path that names a
/// module credits the file that DECLARES it (`pub mod b;`). Returns the edges
/// recorded.
///
/// Only the LEAF of a path was credited, so a module that serves as a namespace
/// was an orphan by construction: 643 of them here, 392 with a traversal one line
/// away (measured by the analise session). A path only a re-export names credits
/// nothing — the rule of 18/09 — and a child module this file declares and uses
/// itself lands on `(file, module) → file`, the `internal_only` class.
///
/// One pass for the rebuild and the hook path (C08).
pub fn record_module_path_consumers(
    db: &FileKnowledgeDB,
    rel_path: &str,
    abs_path: &str,
    content: &str,
) -> usize {
    let Some(root) = db.workspace_root() else {
        return 0;
    };
    let root = std::path::Path::new(root);
    let declared = touring_code::ast::graph::rust_declared_module_names(content);
    let mut recorded = 0;
    for path in touring_code::ast::graph::rust_module_paths(content) {
        let Some(module) = path.rsplit("::").next() else {
            continue;
        };
        // The DECLARATION comes first, because it is the fact: walking forward
        // from the crate root finds each segment as a `mod` item of the file
        // reached so far. Only then the layout resolver, which probes the
        // filesystem and follows re-exports — both inferences. Order matters:
        // touring-cli declares `pub mod shared { … }` inline AND re-exports
        // names from `touring_hook_runtime::shared`, and `reexport_origins`
        // matches any segment of that path, so the resolver credited the OTHER
        // crate's module while the local declaration sat one line away (4
        // modules, 19/09/2026).
        let declarer = crate::symbol_extractors::declarer_of_crate_path(&path, abs_path, root)
            .map(|(declarer, _)| declarer)
            .or_else(|| {
                crate::symbol_extractors::resolve_import_path_with_source(
                    &path,
                    "rust",
                    Some(abs_path),
                )
                .and_then(|module_file| {
                    crate::symbol_extractors::declaring_file_for_module(&module_file, root)
                })
            })
            // A bare child module of THIS file: the file declares it, so the
            // file is its declarer, and the edge reads `internal_only`.
            .or_else(|| declared.contains(module).then(|| rel_path.to_string()));
        if let Some(declarer) = declarer
            && db
                .record_consumer(&declarer, module, rel_path, None)
                .is_ok()
        {
            recorded += 1;
        }
    }
    recorded
}

/// B5: `import pacote.modulo as m` + `m.Nome`. The module path resolves the way
/// an import's does, the symbol is credited to the module that DEFINES it (a
/// façade only forwards), and the edge is an `ast_inferred` guess by name. Each
/// `(module_path, symbol)` comes from `python_qualified_uses`. Returns the edges
/// recorded.
///
/// One pass for the rebuild and the hook path (C08). Until 18/09/2026 only the
/// rebuild wrote these edges, and without the definer: the edit path cleared
/// them with the other inferred rows and never re-derived them, so every edited
/// Python file lost them until the next rebuild. They also feed the gate of the
/// Python method pass, so they are written before it.
pub fn record_python_qualified_uses(
    db: &FileKnowledgeDB,
    rel_path: &str,
    abs_path: &str,
    uses: &[(String, String)],
) -> usize {
    let mut recorded = 0;
    for (module_path, symbol) in uses {
        let Some(module_file) = crate::symbol_extractors::resolve_import_path_with_source(
            module_path,
            "python",
            Some(abs_path),
        ) else {
            continue;
        };
        let definer =
            crate::symbol_extractors::definer_module(&module_file, symbol, Some(abs_path));
        if db
            .record_consumer_with_origin(
                &definer,
                symbol,
                rel_path,
                None,
                crate::knowledge_wiring::WiringOrigin::AstInferred,
            )
            .is_ok()
        {
            recorded += 1;
        }
    }
    recorded
}

/// Every wiring row `file_path` owns, re-derived from its current `content`: its
/// producers (real visibility), then its consumer rows — `use` imports, direct
/// paths, and the INFERRED edges (bare calls, type positions, qualified calls).
///
/// The hook-path twin of the rebuild's per-file pass, and the ONE function the
/// edit, read, file-changed and task-output paths call. Until 14/09/2026 they
/// were four sequences: the edit path ran all three steps, the read path cleared
/// every consumer row and re-recorded only `use` imports plus an obsolete
/// name-only pass that labelled guesses `ast_resolved` (reading `dep_health.rs`
/// once took it from 79 inferred edges to 0), and file-changed/task-output ran
/// the first two steps, losing the inferred edges the same way — orphans that
/// grew with every read and vanished at the next rebuild (cross-audit B1/B2).
pub fn refresh_file_wiring(db: &FileKnowledgeDB, file_path: &str, language: &str, content: &str) {
    let _ = refresh_file_producers(db, file_path, language, content);
    update_wiring_after_edit(db, file_path);
    // Every language's imports resolve in the pass the rebuild uses (C08). Rust
    // used to keep only `update_wiring_after_edit`, whose `imports_json` is a
    // line regex: it never saw a `use a::{B, C}` list, and a re-export the file
    // also uses in its body (18/09/2026) read one way here, another there.
    if !touring_foundation::config::is_companion_key(file_path)
        && let Some(root) = db.workspace_root()
    {
        let abs = std::path::Path::new(root).join(file_path);
        let _ = db.clear_declared_consumer_entries(file_path);
        record_import_consumers(db, file_path, &abs.to_string_lossy(), language, content);
        if language == "rust" {
            record_module_path_consumers(db, file_path, &abs.to_string_lossy(), content);
        }
    }
    // `update_wiring_after_edit` clears consumer rows only for a file with stored
    // imports; the inferred edges are re-derived below either way, so the stale
    // ones of calls the file no longer makes are dropped first.
    let _ = db.clear_inferred_consumer_entries(file_path);
    // Before the method pass: its Python gate reads the modules this file has
    // edges to, and `alias.Nome` is one of the ways a module is reached.
    if language == "python"
        && db.polyglot()
        && let Some(root) = db.workspace_root()
    {
        let abs = std::path::Path::new(root).join(file_path);
        record_python_qualified_uses(
            db,
            file_path,
            &abs.to_string_lossy(),
            &touring_code::ast::graph::python_qualified_uses(content),
        );
    }
    record_direct_path_consumers(db, file_path, content);
    record_self_references(db, file_path, language, content);
}

/// The file's use of its own public symbols (`internal_only`), re-derived like
/// every other consumer row it owns, through the rebuild's own rule
/// ([`touring_code::ast::graph::self_referenced_names`]). Until 18/09/2026 only
/// the rebuild wrote these edges, so an edit cleared them with the other inferred
/// rows and the file's internal symbols read as orphans until the next rebuild.
fn record_self_references(db: &FileKnowledgeDB, file_path: &str, language: &str, content: &str) {
    if !(language == "rust" || (language == "python" && db.polyglot())) {
        return;
    }
    let Some(symbols) = crate::ast_bridge::extract_enriched_symbols(content, file_path) else {
        return;
    };
    // The producer is the file itself: `self_referenced_names` only returns
    // symbols this file declares, so it is the definer by construction.
    let declaring_file = file_path;
    for name in touring_code::ast::graph::self_referenced_names(content, &symbols, language) {
        let _ = db.record_consumer_with_origin(
            declaring_file,
            &name,
            file_path,
            None,
            crate::knowledge_wiring::WiringOrigin::AstInferred,
        );
    }
}

/// [`refresh_file_wiring`] for a caller that holds only the path (relative to
/// `project_root`, or absolute). `false`, touching nothing, when the walker would
/// refuse the file or it is missing or unreadable.
///
/// Cross-audit 14/09/2026 (B3): the admission policy used to live only in the
/// edit path, so `file_changed` and `task_output` wrote wiring under keys the
/// rebuild never produces — absolute paths, excluded directories. The file is
/// stored under the ONE key the walker gives it.
pub fn refresh_file_wiring_from_disk(
    db: &FileKnowledgeDB,
    project_root: &std::path::Path,
    file_path: &str,
) -> bool {
    let rel = crate::hook_runtime::make_relative(file_path, project_root);
    let Ok(key) = crate::shared::reindex::admission_key_for_root(project_root, &rel) else {
        return false;
    };
    let abs = if std::path::Path::new(&rel).is_absolute() {
        std::path::PathBuf::from(&rel)
    } else {
        project_root.join(&rel)
    };
    let Ok(content) = std::fs::read_to_string(abs) else {
        return false;
    };
    let language = crate::shared::detect_language::detect_language_owned(&key);
    refresh_file_wiring(db, &key, &language, &content);
    true
}

/// Update wiring map after a file is edited.
///
/// Re-scans the file's knowledge to update pub symbol registrations
/// and consumer entries. Called from post_edit::reindex_file.
pub fn update_wiring_after_edit(db: &FileKnowledgeDB, file_path: &str) {
    // Capture score BEFORE update
    let previous_score = db.integration_score(file_path).unwrap_or(1.0);

    if let Ok(Some(knowledge)) = db.lookup(file_path) {
        // Producer rows are NOT touched here. The stored `symbols_json` carries no
        // visibility, and clearing them to re-register from it wiped the producers
        // of every edited file (14/09/2026: 10 of 15 touring files edited after
        // the rebuild had none). Callers holding the content refresh them with
        // `refresh_file_producers`; the rest leave them as the last parse saw them.

        // Re-register consumer entries (this file as consumer).
        //
        // Sources of consumer evidence:
        //   1. `use X::Y` imports (imports_json) — recorded for both Types (PascalCase)
        //      AND functions (snake_case). Previous versions filtered uppercase-only,
        //      which caused handler-style consumers like `crate::lifecycle::handle_*`
        //      to be silently dropped — leaving entire modules mis-flagged as orphaned
        //      and starving the quality gate of feedback.
        //   2. Direct path expressions — `crate::module::fn()` calls that are not
        //      preceded by `use`. Detected via a conservative regex scan of the file
        //      content pulled from `file_knowledge.notes` when available (fallback:
        //      extract from imports which ast_bridge already surfaces via symbol_use
        //      edges in the enriched extractor).
        if let Some(ref imports_json) = knowledge.imports_json
            && let Ok(imports) = serde_json::from_str::<Vec<String>>(imports_json)
        {
            let _ = db.clear_consumer_entries(file_path);
            for import_path in &imports {
                record_consumer_from_path(db, import_path, file_path);
            }
        }

        // Direct-path consumer edges are recorded from the caller (e.g.
        // `reindex_file`) via `record_direct_path_consumers` — that caller
        // already has the file content in memory, so we avoid a redundant
        // disk read here. Keeping the zero-IO contract here also lets unit
        // tests exercise `update_wiring_after_edit` without a real file.

        // Log integration score change
        if let Ok(score) = db.integration_score(file_path)
            && score < 1.0
        {
            tracing::debug!(
                file = file_path,
                score,
                "wiring: integration score after edit"
            );
        }
    }

    // Inject RL reward AFTER update
    inject_wiring_reward(db, file_path, previous_score);
}

/// Scan file content for `crate::mod::fn(...)` direct-path consumer calls
/// and record each as a consumer edge.
///
/// Called from `shared::reindex::reindex_file` after the base wiring update,
/// because the caller already has the full file content in memory. This
/// function is idempotent — `clear_consumer_entries` was already invoked by
/// `update_wiring_after_edit`, so this just re-populates the consumer edges
/// with direct-path evidence in addition to `use`-import evidence.
pub fn record_direct_path_consumers(db: &FileKnowledgeDB, consumer_file: &str, content: &str) {
    // The scan reads every `crate::`/`super::` path, `use` lines included, so
    // the path a `pub use` forwards would read as a use of it (18/09/2026).
    let forwarded = reexported_paths(consumer_file, content);
    for path in extract_direct_path_expressions(content) {
        if !forwarded.contains(&path) {
            record_consumer_from_path(db, &path, consumer_file);
        }
    }
    // W4 (2026-09-02): the inference step the rebuild runs post-walk, now on
    // the edit path too. `update_wiring_after_edit` cleared this file's
    // consumer rows and re-recorded only `use` imports — every edited file
    // lost its `ast_inferred` edges until the next full rebuild, and callees
    // reached by a bare call or a type position read as orphans (measured on
    // `sandbox_executor.rs` → `limits.rs::apply_resource_caps_to`, 02/09).
    if let Some(lang) = touring_code::ast::Lang::from_path(std::path::Path::new(consumer_file)) {
        let method_names = touring_code::ast::graph::extract_method_calls(content, lang);
        let type_refs = touring_code::ast::graph::extract_type_and_const_refs(content, lang);
        let qualified_calls = touring_code::ast::graph::extract_qualified_calls(content, lang);
        let _ = db.record_inferred_consumers(
            consumer_file,
            &method_names,
            &type_refs,
            // D2 (2026-09-02) — ver `index.rs`: o par qualificado é exato.
            &qualified_calls,
        );
    }
    // FIX-4 (2026-04-13) recorded a `pub use <submod>::<symbol>` as the parent's
    // use of the symbol, on this path only. Removed 18/09/2026: a re-export is
    // not a use (Gabriel), and a caller reaching the symbol through the parent
    // is credited to the definer, which follows the chain.
}

/// Full paths (`crate::a::Foo`) a Rust file re-exports, from the one reading of
/// re-exports ([`touring_code::ast::graph::rust_reexports`]).
fn reexported_paths(consumer_file: &str, content: &str) -> HashSet<String> {
    if !consumer_file.ends_with(".rs") {
        return HashSet::new();
    }
    touring_code::ast::graph::rust_reexports(content)
        .into_iter()
        .flat_map(|imp| {
            let module = imp.module_path;
            imp.symbols
                .into_iter()
                .map(move |symbol| format!("{module}::{symbol}"))
        })
        .collect()
}

/// `consumer_file` as an absolute path under the project this database belongs to.
///
/// The resolver finds a file's Cargo workspace by walking up from the file (Cargo's
/// rule). The edit path hands it project-relative paths, and a relative path can only
/// be read against the PROCESS — whose current directory is wherever the daemon
/// happened to be spawned (18/09/2026: `~/Work`, and every `use touring_…` read as
/// external). The database knows its own project root; anchoring here removes the
/// process from the question.
fn absolute_consumer(db: &FileKnowledgeDB, consumer_file: &str) -> String {
    let path = std::path::Path::new(consumer_file);
    match db.workspace_root() {
        Some(root) if !path.is_absolute() => std::path::Path::new(root)
            .join(path)
            .to_string_lossy()
            .into_owned(),
        _ => consumer_file.to_string(),
    }
}

/// Resolve a path like `crate::module::symbol` or `super::submod::Type` into
/// a `(module_file, symbol)` pair and record a consumer edge.
///
/// Accepts both Type consumers (PascalCase) and function consumers (snake_case).
/// Previously only the former were recorded, leaving handler-style modules
/// mis-flagged as orphaned.
fn record_consumer_from_path(db: &FileKnowledgeDB, import_path: &str, consumer_file: &str) {
    let symbol_name = match import_path.rsplit("::").next() {
        Some(s) if !s.is_empty() => s,
        _ => return,
    };
    // Skip common pseudo-symbols that aren't real exports.
    if matches!(symbol_name, "*" | "self" | "Self") {
        return;
    }
    let module_hint = import_path
        .rsplit_once("::")
        .map(|(m, _)| m)
        .unwrap_or(import_path);

    // Resolve with the SAME resolver the full rebuild uses (2026-08-08).
    //
    // This used to build the path by string substitution:
    // `format!("{crate_root}/{}.rs", rest.replace("::", "/"))`. Three failures,
    // each already solved in `resolve_import_path_with_source` and each
    // re-introduced here because the two paths were written separately:
    //
    // 1. **No filesystem probe** — the path was recorded whether or not it
    //    existed, so every miss became a consumer row pointing at a phantom
    //    file. Those rows wire nothing (no producer can share a module_file
    //    that does not exist) while looking like coverage.
    // 2. **No directory layout** — `foo/mod.rs` was never tried, only `foo.rs`.
    // 3. **No re-export following** — `crate::shared::feature_flags::f()` in
    //    touring-hooks-core became `…/src/shared/feature_flags.rs`, which does
    //    not exist: `shared/mod.rs` re-exports it from touring-hooks-shared.
    //    The real producer therefore kept ZERO consumers and read as an orphan.
    //
    // Measured consequence (08/08/2026): a full rebuild wired these symbols via
    // its bare-name pass, then editing any consumer file re-ran THIS path,
    // which replaced the good edge with a phantom — so `orphans_base` reported
    // "new orphans" for symbols with obvious live callers, and a rebuild
    // "fixed" them until the next edit. Sharing one resolver is what stops the
    // two paths from disagreeing again (decision matrix C08).
    let consumer_abs = absolute_consumer(db, consumer_file);
    let Some(module_file) = crate::symbol_extractors::resolve_import_path_with_source(
        module_hint,
        "rust",
        Some(&consumer_abs),
    ) else {
        return;
    };
    // Attribute the producer to the module that DEFINES the symbol, not to one
    // that merely re-exports it. Measured 2026-08-08: `KeywordSearch` landed on
    // `hybrid_search/mod.rs`, which only carries a `pub use`; with no definition
    // there the kind extractor produced `symbol_kind='unknown'` — the single
    // such row in a 76.942-row map, and enough to degrade `touring doctor`.
    let module_file =
        crate::symbol_extractors::definer_module(&module_file, symbol_name, Some(&consumer_abs));
    let _ = db.record_consumer(&module_file, symbol_name, consumer_file, None);
}

/// Extract `crate::a::b::c` and `super::a::b::c` path expressions from Rust
/// source content.
///
/// Matches identifiers separated by `::` that start with `crate::` or
/// `super::`. Used to catch direct-call consumer evidence that `use`-based
/// extractors miss, e.g. `crate::lifecycle::handle_file_changed(rt, v)` in
/// `hook_registry.rs`.
///
/// Returns the path up to but not including the first trailing non-path
/// token (like `(`, `;`, or whitespace). Deduplicated via HashSet. Comments,
/// string and char literals are skipped, and `super::` segments consumed by
/// inline `mod x { … }` blocks of the same file are removed first.
fn extract_direct_path_expressions(content: &str) -> Vec<String> {
    let mut out: std::collections::HashSet<String> = std::collections::HashSet::new();
    let bytes = content.as_bytes();
    let len = bytes.len();
    let at = |k: usize| bytes.get(k).copied().unwrap_or(0);
    let is_ident = |b: u8| b.is_ascii_alphanumeric() || b == b'_';
    // Brace depth of the code, and the depths at which inline `mod x {` blocks
    // opened. A `super::` written inside N inline modules climbs those N modules
    // first — all within THIS file — before it reaches the file's parent
    // (cross-audit 14/09/2026: `use super::super::real_engine;` two modules deep
    // in `f2_5_dep_cves.rs` was resolved against the crate root, `lib.rs`).
    let mut depth = 0usize;
    let mut inline_mods: Vec<usize> = Vec::new();
    let mut pending_mod = false;
    let mut i = 0usize;
    while i < len {
        let b = at(i);
        // Comments, strings and char literals are not code: a path quoted in a
        // comment wired a consumer to a symbol that does not exist
        // (`crate::shared::feature_flags::f()` in a `wiring.rs` comment).
        if b == b'/' && at(i + 1) == b'/' {
            while i < len && at(i) != b'\n' {
                i += 1;
            }
            continue;
        }
        if b == b'/' && at(i + 1) == b'*' {
            let mut nest = 1usize;
            i += 2;
            while i < len && nest > 0 {
                if at(i) == b'/' && at(i + 1) == b'*' {
                    nest += 1;
                    i += 2;
                } else if at(i) == b'*' && at(i + 1) == b'/' {
                    nest -= 1;
                    i += 2;
                } else {
                    i += 1;
                }
            }
            continue;
        }
        let prev_ident = i > 0 && is_ident(at(i - 1));
        if b == b'r' && !prev_ident && (at(i + 1) == b'"' || at(i + 1) == b'#') {
            let mut k = i + 1;
            let mut hashes = 0usize;
            while at(k) == b'#' {
                hashes += 1;
                k += 1;
            }
            if at(k) == b'"' {
                k += 1;
                'raw: while k < len {
                    if at(k) == b'"' && (1..=hashes).all(|h| at(k + h) == b'#') {
                        k += 1 + hashes;
                        break 'raw;
                    }
                    k += 1;
                }
                i = k;
                continue;
            }
        }
        if b == b'"' {
            i += 1;
            while i < len && at(i) != b'"' {
                i += if at(i) == b'\\' { 2 } else { 1 };
            }
            i += 1;
            continue;
        }
        if b == b'\'' {
            // `'x'` or `'\n'` is a char literal; `'a` (a lifetime) has no close.
            let close = if at(i + 1) == b'\\' {
                (i + 2..(i + 12).min(len)).find(|&k| at(k) == b'\'')
            } else {
                content
                    .get(i + 1..)
                    .and_then(|t| t.chars().next())
                    .map(|c| i + 1 + c.len_utf8())
                    .filter(|&k| at(k) == b'\'')
            };
            i = close.map_or(i + 1, |k| k + 1);
            continue;
        }
        if b == b'{' {
            depth += 1;
            if pending_mod {
                inline_mods.push(depth);
                pending_mod = false;
            }
            i += 1;
            continue;
        }
        if b == b'}' {
            if inline_mods.last() == Some(&depth) {
                inline_mods.pop();
            }
            depth = depth.saturating_sub(1);
            i += 1;
            continue;
        }
        if b == b';' {
            // `mod x;` declares a file module, not an inline block.
            pending_mod = false;
            i += 1;
            continue;
        }
        if !prev_ident && content.get(i..).is_some_and(|t| t.starts_with("mod ")) {
            pending_mod = true;
            i += 4;
            continue;
        }
        let tail = content.get(i..).unwrap_or("");
        let starts_path =
            !prev_ident && (tail.starts_with("crate::") || tail.starts_with("super::"));
        if !starts_path {
            // Advance by a whole character so a multibyte byte is never a start.
            i += tail.chars().next().map_or(1, char::len_utf8);
            continue;
        }
        let start = i;
        let mut j = i;
        while j < len {
            if is_ident(at(j)) {
                j += 1;
            } else if at(j) == b':' && at(j + 1) == b':' {
                j += 2;
            } else {
                break;
            }
        }
        let mut end = j;
        while end > start + 2 && at(end - 1) == b':' {
            end -= 1;
        }
        if let Some(path) = content.get(start..end) {
            let supers = path.split("::").take_while(|s| *s == "super").count();
            let inside = inline_mods.len();
            let resolved = if supers == 0 {
                Some(path.to_string())
            } else if supers <= inside {
                // Every `super` climbs an inline module of this same file: the
                // path names this file's own items, which it does not consume.
                None
            } else {
                Some(path.split("::").skip(inside).collect::<Vec<_>>().join("::"))
            };
            if let Some(path) = resolved
                && path.matches("::").count() >= 2
            {
                out.insert(path);
            }
        }
        i = j.max(start + 1);
    }
    out.into_iter().collect()
}

/// Inject RL reward based on integration score change.
///
/// Positive reward when wiring improves (orphan resolved).
/// Negative reward when wiring degrades (new orphan created).
pub fn inject_wiring_reward(db: &FileKnowledgeDB, module_file: &str, previous_score: f64) {
    let current_score = db.integration_score(module_file).unwrap_or(1.0);
    let delta = current_score - previous_score;

    if delta.abs() > 0.01 {
        let reward_type = if delta > 0.0 {
            "wiring_improvement"
        } else {
            "wiring_regression"
        };
        tracing::info!(
            module = module_file,
            previous = previous_score,
            current = current_score,
            delta,
            reward_type,
            "wiring RL signal"
        );
        // The actual RL injection happens through the post-tool-rl hook
        // which reads these structured logs. No direct LinUCB call needed.
    }
}

// =============================================================================
// P4.4: HyperGraph Integration — N-ary cycle/dependency/feature-trace analysis
// =============================================================================

/// Hypergraph-based cycle detection using artificial node pattern.
///
/// Uses `HyperGraph::<String>` to detect N-ary cycles across multi-symbol imports
/// and feature-gated dependencies that the pairwise Tarjan's SCC misses.
///
/// Returns (cycle_count, hyperedge_labels) for cycles involving hyperedges.
pub fn hypergraph_cycle_detection(db: &FileKnowledgeDB) -> (usize, Vec<String>) {
    let mut hg: HyperGraph<String> = HyperGraph::new();
    let mut node_map: FxHashMap<String, petgraph::graph::NodeIndex> = FxHashMap::default();

    // Build hypergraph from wiring_map edges
    let rows: Vec<(String, String)> = {
        let stmt_opt = db.conn_ref().prepare(
            "SELECT DISTINCT module_file, consumer_file FROM wiring_map
             WHERE module_file IS NOT NULL AND consumer_file IS NOT NULL
               AND module_file != consumer_file",
        );
        if let Ok(mut stmt) = stmt_opt {
            stmt.query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map(|rows| rows.filter_map(|r| r.ok()).collect())
            .unwrap_or_default()
        } else {
            Vec::new()
        }
    };

    // Multi-import hyperedges: group consumer imports that share a source module
    let mut import_groups: FxHashMap<String, Vec<String>> = FxHashMap::default();
    for (module, consumer) in &rows {
        import_groups
            .entry(module.clone())
            .or_default()
            .push(consumer.clone());
    }

    let mut hyperedge_labels: Vec<String> = Vec::new();

    // Add pairwise edges for standard dependencies
    for (module, consumer) in &rows {
        let src = if let Some(&idx) = node_map.get(module) {
            idx
        } else {
            let idx = hg.add_node(module.clone());
            node_map.insert(module.clone(), idx);
            idx
        };
        let dst = if let Some(&idx) = node_map.get(consumer) {
            idx
        } else {
            let idx = hg.add_node(consumer.clone());
            node_map.insert(consumer.clone(), idx);
            idx
        };
        let label = format!("dep:{}->{}", module, consumer);
        hg.add_hyperedge(&[src, dst], &label);
    }

    // Detect hyperedge-level cycles using the membership index
    let mut cycles_found = 0;
    for (label, members) in &import_groups {
        if members.len() > 1 {
            let node_indices: Vec<_> = members
                .iter()
                .filter_map(|m| node_map.get(m).copied())
                .collect();
            if node_indices.len() > 1 {
                let he_label = format!("multi_import:{}", label);
                hg.add_hyperedge(&node_indices, &he_label);
                hyperedge_labels.push(he_label);
                cycles_found += 1;
            }
        }
    }

    // Count cycles involving hyperedges (depth > 2 via hyperedge traversal)
    let mut hyperedge_cycle_count = 0;
    for node_idx in node_map.values() {
        let edges = hg.hyperedges_for(*node_idx);
        if edges.len() > 1 {
            hyperedge_cycle_count += 1;
        }
    }

    // Suppress unused warning
    let _ = cycles_found;

    (hyperedge_cycle_count, hyperedge_labels)
}

/// Build a HyperGraph from all wiring_map entries for multi-import analysis.
///
/// Returns the hypergraph and a summary of multi-import hyperedges detected.
pub fn build_multi_import_hypergraph(
    db: &FileKnowledgeDB,
) -> (HyperGraph<String>, Vec<MultiImportHyperedge>) {
    let mut hg: HyperGraph<String> = HyperGraph::new();
    let mut multi_imports: Vec<MultiImportHyperedge> = Vec::new();

    // Collect all import paths from wiring_map
    let rows: Vec<(String, String, Option<i64>)> = {
        let stmt_opt = db.conn_ref().prepare(
            "SELECT DISTINCT module_file, consumer_file, import_line FROM wiring_map
             WHERE module_file IS NOT NULL AND consumer_file IS NOT NULL
               AND import_line IS NOT NULL",
        );
        if let Ok(mut stmt) = stmt_opt {
            stmt.query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<i64>>(2)?,
                ))
            })
            .map(|rows| rows.filter_map(|r| r.ok()).collect())
            .unwrap_or_default()
        } else {
            Vec::new()
        }
    };

    // Group by source module to find multi-import patterns
    let mut by_source: FxHashMap<String, Vec<String>> = FxHashMap::default();
    for (module, consumer, _line) in &rows {
        by_source
            .entry(module.clone())
            .or_default()
            .push(consumer.clone());
    }

    let mut node_indices: FxHashMap<String, petgraph::graph::NodeIndex> = FxHashMap::default();

    for (source, consumers) in by_source {
        if consumers.len() > 1 {
            let _src_idx = if let Some(&idx) = node_indices.get(&source) {
                idx
            } else {
                let idx = hg.add_node(source.clone());
                node_indices.insert(source.clone(), idx);
                idx
            };

            let mut consumer_indices: Vec<petgraph::graph::NodeIndex> = Vec::new();
            for consumer in &consumers {
                let c_idx = if let Some(&idx) = node_indices.get(consumer) {
                    idx
                } else {
                    let idx = hg.add_node(consumer.clone());
                    node_indices.insert(consumer.clone(), idx);
                    idx
                };
                consumer_indices.push(c_idx);
            }

            let import_path = format!("{{{}}}", consumers.join(", "));
            let label = format!("multi:{}", source);
            hg.add_hyperedge(&consumer_indices, &label);

            let mig = MultiImportHyperedge::new(&import_path, &source);
            multi_imports.push(mig);
        }
    }

    (hg, multi_imports)
}

/// Analyze feature gate combinations using FeatureGateHyperedge.
///
/// Scans the wiring_map for cfg-gated modules and returns FeatureGateHyperedge
/// entries for feature-trace analysis.
pub fn analyze_feature_gates(db: &FileKnowledgeDB) -> Vec<FeatureGateHyperedge> {
    let mut gates: Vec<FeatureGateHyperedge> = Vec::new();

    // Query modules with cfg-related patterns in their file paths or names
    let rows: Vec<String> = {
        let stmt_opt = db
            .conn_ref()
            .prepare("SELECT DISTINCT module_file FROM wiring_map WHERE module_file LIKE '%cfg%'");
        if let Ok(mut stmt) = stmt_opt {
            stmt.query_map([], |row| row.get::<_, String>(0))
                .map(|rows| rows.filter_map(|r| r.ok()).collect())
                .unwrap_or_default()
        } else {
            Vec::new()
        }
    };

    for module_path in rows {
        // Create a feature gate hyperedge for each cfg-gated module
        let features: Vec<String> = module_path
            .split(['_', '/'])
            .filter(|s| s.starts_with("feat") || s.starts_with("cfg"))
            .map(|s| {
                s.trim_start_matches("feat")
                    .trim_start_matches("cfg")
                    .to_string()
            })
            .filter(|s| !s.is_empty())
            .collect();

        if !features.is_empty() {
            let expression = format!(
                "all({})",
                features
                    .iter()
                    .map(|f| format!("feature = \"{}\"", f))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            let gate = FeatureGateHyperedge::new(&expression, &module_path);
            gates.push(gate);
        }
    }

    gates
}

#[cfg(test)]
#[path = "wiring_tests.rs"]
mod tests;
