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
    let sql = format!(
        "SELECT DISTINCT module_file, consumer_file, workspace_root FROM wiring_map
         WHERE module_file IS NOT NULL
           AND consumer_file IS NOT NULL
           AND module_file != consumer_file{ws_predicate}",
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

/// Update wiring map after a file is edited.
///
/// Re-scans the file's knowledge to update pub symbol registrations
/// and consumer entries. Called from post_edit::reindex_file.
pub fn update_wiring_after_edit(db: &FileKnowledgeDB, file_path: &str) {
    // Capture score BEFORE update
    let previous_score = db.integration_score(file_path).unwrap_or(1.0);

    if let Ok(Some(knowledge)) = db.lookup(file_path) {
        // Re-register pub symbols (clear + re-add to catch added/removed)
        if let Some(ref symbols_json) = knowledge.symbols_json
            && let Ok(symbols) = serde_json::from_str::<Vec<serde_json::Value>>(symbols_json)
        {
            let _ = db.clear_wiring(file_path);
            for sym in &symbols {
                let is_public = sym
                    .get("is_public")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                if is_public {
                    let name = sym.get("name").and_then(|v| v.as_str()).unwrap_or("");
                    let kind = sym
                        .get("kind")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown");
                    if !name.is_empty() {
                        let _ = db.register_pub_symbol(file_path, name, kind, "public");
                    }
                }
            }
        }

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
    for path in extract_direct_path_expressions(content) {
        record_consumer_from_path(db, &path, consumer_file);
    }
    // FIX-4 (2026-04-13): also detect `pub use <submod>::<symbol>` and
    // `pub(crate) use <submod>::<symbol>` re-exports. When a parent
    // module (e.g. `lifecycle.rs`) re-exports a symbol from a co-located
    // submodule (e.g. `lifecycle/subagent.rs`), the parent IS effectively
    // a consumer of the submodule — the re-export forwards external
    // callers through. Without this, every submodule extraction causes
    // its pub symbols to be flagged orphan even though the parent
    // re-exports them. Idempotent with `extract_direct_path_expressions`:
    // that scan only catches `crate::` / `super::` absolute paths, while
    // this scan covers bare `<mod>::<sym>` re-export syntax.
    for (submod, symbol) in extract_reexport_pairs(content) {
        record_reexport_consumer(db, consumer_file, &submod, &symbol);
    }
}

/// Extract `(submodule, symbol)` pairs from re-export statements.
///
/// Matches `pub use foo::bar;`, `pub(crate) use foo::bar;`,
/// `pub(super) use foo::bar;`, etc. Ignores absolute paths (`crate::...`,
/// `super::...`) — those are handled by `extract_direct_path_expressions`.
fn extract_reexport_pairs(content: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim_start();
        // Strip optional `pub` / `pub(<vis>)` prefix.
        let after_pub = if let Some(rest) = trimmed.strip_prefix("pub(") {
            // skip until ')'
            match rest.find(')') {
                Some(close) => rest[close + 1..].trim_start(),
                None => continue,
            }
        } else if let Some(rest) = trimmed.strip_prefix("pub ") {
            rest.trim_start()
        } else {
            continue;
        };
        let after_use = match after_pub.strip_prefix("use ") {
            Some(s) => s.trim_start(),
            None => continue,
        };
        // Skip absolute paths (handled elsewhere).
        if after_use.starts_with("crate::")
            || after_use.starts_with("super::")
            || after_use.starts_with("self::")
            || after_use.starts_with("::")
        {
            continue;
        }
        // Accept `<ident>::<ident>[::<ident>...];` — minimal relative re-export.
        let path_end = after_use
            .find([';', '{', ' ', '\t', ',', '\n'])
            .unwrap_or(after_use.len());
        let path = after_use.get(..path_end).unwrap_or("");
        if !path.contains("::") {
            continue;
        }
        let mut parts: Vec<&str> = path.split("::").collect();
        let Some(symbol) = parts.pop() else { continue };
        if symbol.is_empty() || symbol == "*" || symbol == "self" {
            continue;
        }
        if parts.is_empty() {
            continue;
        }
        let submod = parts.first().copied().unwrap_or("").to_string();
        // Filter out obviously non-local modules (std, tokio, serde, etc.).
        // Heuristic: a single-segment submodule name without any built-in
        // prefix is most likely a local submodule.
        if matches!(
            submod.as_str(),
            "std"
                | "core"
                | "alloc"
                | "tokio"
                | "serde"
                | "serde_json"
                | "anyhow"
                | "thiserror"
                | "tracing"
                | "rusqlite"
                | "regex"
                | "chrono"
                | "blake3"
                | "uuid"
                | "rand"
                | "reqwest"
        ) {
            continue;
        }
        out.push((submod, symbol.to_string()));
    }
    out
}

/// Record a re-export consumer edge: `consumer_file` re-exports `symbol`
/// from its own co-located `<submod>.rs` or `<submod>/mod.rs`.
fn record_reexport_consumer(db: &FileKnowledgeDB, consumer_file: &str, submod: &str, symbol: &str) {
    if matches!(symbol, "*" | "self" | "Self") {
        return;
    }
    // Consumer is `.../foo.rs` → submodule lives at `.../foo/<submod>.rs`.
    let parent_dir = std::path::Path::new(consumer_file)
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();
    let stem = std::path::Path::new(consumer_file)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    // Try `<parent>/<stem>/<submod>.rs` first (Rust 2018+ layout where
    // lifecycle.rs with `mod subagent;` looks for lifecycle/subagent.rs).
    let nested = if parent_dir.is_empty() {
        format!("{stem}/{submod}.rs")
    } else {
        format!("{parent_dir}/{stem}/{submod}.rs")
    };
    let _ = db.record_consumer(&nested, symbol, consumer_file, None);
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

    // Resolve `crate::` and `super::` relative to the *consumer's* crate root.
    // Workspace paths look like `crates/<name>/src/<...>.rs` — we take the
    // prefix up to and including `src/` as the crate root. For non-workspace
    // files (e.g. `src/lib.rs` top-level crates), we fall back to `src/`.
    let crate_root_prefix: String = consumer_file
        .rfind("/src/")
        .map(|idx| consumer_file[..idx + 5].to_string()) // include "/src/"
        .unwrap_or_else(|| "src/".to_string());

    let module_file = if let Some(rest) = module_hint.strip_prefix("crate::") {
        format!("{crate_root_prefix}{}.rs", rest.replace("::", "/"))
    } else if let Some(rest) = module_hint.strip_prefix("super::") {
        // `super::` resolves to the parent module of the consumer. Best-effort:
        // take consumer's directory and go up one level, then join `rest`.
        let consumer_dir = std::path::Path::new(consumer_file)
            .parent()
            .and_then(|p| p.parent())
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| "src".to_string());
        format!("{consumer_dir}/{}.rs", rest.replace("::", "/"))
    } else {
        return;
    };
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
/// token (like `(`, `;`, or whitespace). Deduplicated via HashSet.
fn extract_direct_path_expressions(content: &str) -> Vec<String> {
    let mut out: std::collections::HashSet<String> = std::collections::HashSet::new();
    let bytes = content.as_bytes();
    let len = bytes.len();
    let mut i = 0usize;
    while i < len {
        // Skip over any byte that isn't a char boundary — UTF-8 multibyte
        // continuation bytes must never be treated as start positions. Using
        // `content.get(i..)` returns None on non-boundaries without panicking.
        let Some(tail) = content.get(i..) else {
            i += 1;
            continue;
        };
        // Word-boundary check: previous byte must not be alphanumeric or `_`.
        let prev_ok = i == 0 || {
            let p = bytes.get(i - 1).copied().unwrap_or(0);
            !p.is_ascii_alphanumeric() && p != b'_'
        };
        let starts_crate = prev_ok && tail.starts_with("crate::");
        let starts_super = prev_ok && tail.starts_with("super::");
        if !(starts_crate || starts_super) {
            i += 1;
            continue;
        }
        let start = i;
        // Advance through identifier chars and `::` separators (all ASCII).
        let mut j = i;
        while j < len {
            let b = bytes.get(j).copied().unwrap_or(0);
            if b.is_ascii_alphanumeric() || b == b'_' {
                j += 1;
            } else if j + 1 < len && b == b':' && bytes.get(j + 1).copied().unwrap_or(0) == b':' {
                j += 2;
            } else {
                break;
            }
        }
        // Strip any trailing `::` so `path` always ends on an identifier.
        let mut end = j;
        while end > start + 2 && bytes.get(end - 1).copied().unwrap_or(0) == b':' {
            end -= 1;
        }
        // The span [start..end] is guaranteed ASCII (only identifier chars
        // and `:`), so slicing is safe.
        if let Some(path) = content.get(start..end)
            && path.matches("::").count() >= 2
        {
            out.insert(path.to_string());
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
