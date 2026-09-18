//! `cli-repair-wiring` — Wiring DB consumer tracking repair.
//!
//! Repairs the consumer_count=null staleness issue in the wiring DB
//! by re-running wiring analysis and updating consumer tracking.
//!
//! Hardened 2026-06-11 (v2): honour the `{dry_run, limit}` payload (it was
//! silently ignored and an unbounded run timed out as `success=false`).
//!
//! Batched 2026-06-11 (v3): the daemon's REQUEST_TIMEOUT is 5s and one
//! workspace grep per symbol caps throughput at ~40 symbols/request. v3
//! greps in alternation chunks (~50 symbols per grep, `\b(s1|s2|…)\b`),
//! raising throughput ~50×, and adds an `offset` cursor so repeated calls
//! page past genuine orphans (whose NULL rows intentionally remain).
//!
//! Scoped 2026-09-18 (v4): the repair reads Rust `use` lines, so it repairs
//! Rust producers only, attributes a consumer by the WORD it imports, and
//! records the edge as `ast_inferred` — a name match, not a resolved import.
//! It had matched every language's orphans against Rust `use` lines by
//! substring: the analise (a Python project) got 18.035 edges from `.rs` files
//! to Python symbols, masking ~1.488 orphans, and `No` was "found" inside
//! `NodeId`. `--purge-cross-language` removes those edges and restores the
//! orphan rows the repair deleted.

use crate::knowledge::FileKnowledgeDB;
use crate::runtime::HookRuntime;
use crate::wiring::WiringEntry;
use rusqlite::params;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::process::Command;
use touring_hook_runtime::knowledge_wiring::WiringOrigin;

/// Default per-run repair batch when the caller does not pass `--limit`.
/// With chunked greps (~50 symbols each) this stays well under the daemon's
/// 5s request timeout.
const DEFAULT_REPAIR_LIMIT: i64 = 500;

/// Symbols per grep alternation. Keeps the regex small enough for grep -E
/// while amortising the workspace walk across many symbols.
const GREP_CHUNK: usize = 50;

/// Edges a run reports verbatim, so a dry run shows WHAT it would write.
const SAMPLE: usize = 10;

/// Public orphan rows: a producer row with no consumer row anywhere.
const ORPHANS: &str = "FROM wiring_map w
    WHERE w.consumer_file IS NULL AND w.visibility = 'public'
      AND NOT EXISTS (
          SELECT 1 FROM wiring_map w2
          WHERE w2.module_file = w.module_file
            AND w2.symbol_name = w.symbol_name
            AND w2.consumer_file IS NOT NULL
      )";

/// Rows no import or call can make: consumer and producer in different
/// language families. The predicate is the storage's single source
/// (`cross_language_edge_sql`); these rows are the signature of this repair
/// before v4 and of the by-name inference before 30.4.58.
fn cross_language() -> String {
    touring_hook_runtime::knowledge_wiring::cross_language_edge_sql()
}

/// One consumer edge a run recorded or would record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SampleEdge {
    /// Producer module.
    pub module_file: String,
    /// Symbol the consumer imports.
    pub symbol_name: String,
    /// File holding the `use` line.
    pub consumer_file: String,
}

impl SampleEdge {
    fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "module_file": self.module_file,
            "symbol": self.symbol_name,
            "consumer_file": self.consumer_file,
        })
    }
}

/// Outcome of one repair run (see [`repair_wiring_consumer_tracking`]).
pub struct RepairOutcome {
    /// Orphan rows examined in this batch.
    pub scanned: usize,
    /// Consumer edges recorded (or that would be recorded under dry-run).
    pub repaired: usize,
    /// Symbols for which at least one real consumer was found.
    pub symbols_with_consumers: usize,
    /// Whether this was a preview-only run.
    pub dry_run: bool,
    /// Cursor for the next call (`offset + scanned`); `None` when the
    /// orphan list is exhausted.
    pub next_offset: Option<i64>,
    /// Orphans of other languages, left alone: the repair reads Rust `use`
    /// lines, and the index rebuild wires the other languages.
    pub skipped_non_rust: usize,
    /// Up to [`SAMPLE`] of the edges this run recorded or would record.
    pub sample: Vec<SampleEdge>,
}

/// The identifiers on a line, as whole words: `No` is not in `NodeId`.
fn words(line: &str) -> HashSet<&str> {
    line.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .filter(|word| !word.is_empty())
        .collect()
}

/// Repair wiring consumer tracking by rescanning for actual consumers.
///
/// Examines up to `limit` Rust orphan symbols starting at `offset` (ordered
/// by module_file, symbol_name) and greps the workspace's `.rs` files for
/// `use` lines naming them, in alternation chunks. Unless `dry_run`, records
/// each consumer as an inferred edge and deletes the stale NULL row of the
/// symbols it wired.
pub fn repair_wiring_consumer_tracking(
    rt: &mut HookRuntime,
    dry_run: bool,
    limit: i64,
    offset: i64,
) -> Result<RepairOutcome, String> {
    let project_root = rt.project_root.clone();
    repair_in(&rt.ctx.knowledge, &project_root, dry_run, limit, offset)
}

fn repair_in(
    db: &FileKnowledgeDB,
    project_root: &Path,
    dry_run: bool,
    limit: i64,
    offset: i64,
) -> Result<RepairOutcome, String> {
    let conn = db.conn_ref();
    let orphan_entries = rust_orphans(conn, limit, offset)?;
    let scanned = orphan_entries.len();
    let mut outcome = RepairOutcome {
        scanned,
        repaired: 0,
        symbols_with_consumers: 0,
        dry_run,
        next_offset: ((scanned as i64) >= limit).then_some(offset + scanned as i64),
        skipped_non_rust: count_non_rust_orphans(conn)?,
        sample: Vec::new(),
    };
    if orphan_entries.is_empty() {
        return Ok(outcome);
    }
    let found = grep_rust_consumers(&orphan_entries, project_root);
    for entry in &orphan_entries {
        if let Some(consumers) = found
            .get(entry.symbol_name.as_str())
            .filter(|consumers| !consumers.is_empty())
        {
            record_repair(db, entry, consumers, &mut outcome);
        }
    }
    Ok(outcome)
}

/// A page of public Rust orphan rows, in (module_file, symbol_name) order.
fn rust_orphans(
    conn: &rusqlite::Connection,
    limit: i64,
    offset: i64,
) -> Result<Vec<WiringEntry>, String> {
    let sql = format!(
        "SELECT w.module_file, w.symbol_name, w.symbol_kind, w.visibility,
               w.consumer_file, w.import_line, w.contract_source
        {ORPHANS} AND w.module_file LIKE '%.rs'
        ORDER BY w.module_file, w.symbol_name
        LIMIT ?1 OFFSET ?2"
    );
    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| format!("failed to prepare orphan query: {}", e))?;
    let rows = stmt
        .query_map(params![limit, offset], |row| {
            Ok(WiringEntry {
                module_file: row.get::<_, String>(0)?,
                symbol_name: row.get::<_, String>(1)?,
                symbol_kind: row.get::<_, String>(2)?,
                visibility: row.get::<_, String>(3)?,
                consumer_file: row.get::<_, Option<String>>(4)?,
                import_line: row.get::<_, Option<i64>>(5)?,
                contract_source: row.get::<_, String>(6)?,
            })
        })
        .map_err(|e| format!("failed to query orphans: {}", e))?;
    Ok(rows.filter_map(Result::ok).collect())
}

/// Public orphans of every other language: counted, never repaired here.
fn count_non_rust_orphans(conn: &rusqlite::Connection) -> Result<usize, String> {
    conn.query_row(
        &format!("SELECT COUNT(*) {ORPHANS} AND w.module_file NOT LIKE '%.rs'"),
        [],
        |row| row.get::<_, i64>(0),
    )
    .map(|count| count as usize)
    .map_err(|e| format!("failed to count non-Rust orphans: {}", e))
}

/// Symbol → the `.rs` files whose `use` lines import it by name.
fn grep_rust_consumers<'a>(
    orphan_entries: &'a [WiringEntry],
    project_root: &Path,
) -> HashMap<&'a str, HashSet<String>> {
    let root_str = project_root.to_str().unwrap_or("");

    // Candidates: importable identifiers only ([A-Za-z0-9_] — safe to embed
    // in a grep -E alternation without escaping).
    let candidates: Vec<&WiringEntry> = orphan_entries
        .iter()
        .filter(|e| {
            !e.symbol_name.is_empty()
                && !e.symbol_name.starts_with('_')
                && e.symbol_name
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_')
        })
        .collect();

    // symbol -> set of consumer files
    let mut found: HashMap<&str, HashSet<String>> = HashMap::new();

    // One grep per chunk of symbols (alternation), instead of one per
    // symbol — the workspace walk dominates, so amortise it.
    for chunk in candidates.chunks(GREP_CHUNK) {
        let alternation = chunk
            .iter()
            .map(|e| e.symbol_name.as_str())
            .collect::<Vec<_>>()
            .join("|");
        // A plain `use` only: a `pub use` forwards the name and is not a use
        // of it (18/09/2026, Gabriel), so the repair must not wire what the
        // rebuild no longer counts.
        let pattern = format!(r"^[[:space:]]*use\b.*\b({})\b", alternation);

        let output = Command::new("grep")
            .args([
                "-rE",
                &pattern,
                "--include=*.rs",
                "--exclude-dir=target",
                "--exclude-dir=.git",
                root_str,
            ])
            .output();
        let output = match output {
            Ok(o) => o,
            Err(_) => continue,
        };
        let stdout = String::from_utf8_lossy(&output.stdout);

        for line in stdout.lines() {
            credit_line(line, chunk, project_root, &mut found);
        }
    }
    found
}

/// Credits the consumer of one `grep` line (`file:use …`) to every chunk
/// symbol the line imports.
fn credit_line<'a>(
    line: &str,
    chunk: &[&'a WiringEntry],
    project_root: &Path,
    found: &mut HashMap<&'a str, HashSet<String>>,
) {
    let Some((file, rest)) = line.split_once(':') else {
        return;
    };
    let Ok(rel) = Path::new(file).strip_prefix(project_root) else {
        return;
    };
    let rel_str = rel.to_string_lossy().to_string();
    if rel_str.is_empty() {
        return;
    }
    // Which chunk symbols does this line import? A whole word: the grep
    // matched the line for ONE of them, and a substring test then credited
    // every symbol spelled inside another (`No` in `NodeId`).
    let imported = words(rest);
    for entry in chunk {
        let sym = entry.symbol_name.as_str();
        if imported.contains(sym)
            && rel_str != entry.module_file
            && !rel_str.contains(&entry.module_file)
        {
            found.entry(sym).or_default().insert(rel_str.clone());
        }
    }
}

/// Records one symbol's consumers (or counts them, under dry-run); the first
/// [`SAMPLE`] edges of the run go to `outcome`.
///
/// The producer row (`consumer_file IS NULL`) stays: it is the symbol's
/// DECLARATION, one per public symbol, and `record_consumer` reads the kind and
/// visibility of every edge from it. Up to 30.4.58 the repair deleted it, which
/// left producers without a declaration and taught the analise to read the
/// row beside a consumer as residue (18/09/2026). The orphan scan is
/// `NOT EXISTS` a consumer, so a repaired symbol leaves it all the same.
fn record_repair(
    db: &FileKnowledgeDB,
    entry: &WiringEntry,
    consumers: &HashSet<String>,
    outcome: &mut RepairOutcome,
) {
    let sym = entry.symbol_name.as_str();
    outcome.symbols_with_consumers += 1;
    let mut consumers: Vec<&String> = consumers.iter().collect();
    consumers.sort();
    let room = SAMPLE.saturating_sub(outcome.sample.len());
    outcome
        .sample
        .extend(consumers.iter().take(room).map(|consumer_file| SampleEdge {
            module_file: entry.module_file.clone(),
            symbol_name: sym.to_string(),
            consumer_file: (*consumer_file).clone(),
        }));

    if outcome.dry_run {
        outcome.repaired += consumers.len();
        return;
    }

    for consumer_file in consumers {
        // A `use` line naming the symbol is a name match, not a resolved
        // path: it is recorded as the guess it is.
        match db.record_consumer_with_origin(
            &entry.module_file,
            sym,
            consumer_file,
            None,
            WiringOrigin::AstInferred,
        ) {
            Err(e) => {
                eprintln!(
                    "repair: failed to record consumer for {}::{} from {}: {}",
                    entry.module_file, sym, consumer_file, e
                );
            }
            _ => {
                outcome.repaired += 1;
            }
        }
    }
}

/// Outcome of [`purge_cross_language_edges`].
pub(crate) struct PurgeOutcome {
    /// Cross-language edges removed (or that would be removed).
    pub edges: usize,
    /// Distinct `(module_file, symbol)` producers those edges pointed at.
    pub symbols: usize,
    /// Producers left with no row after the purge, whose orphan row was
    /// written back (under dry-run: that would be written back).
    pub orphans_restored: usize,
    /// Whether this was a preview-only run.
    pub dry_run: bool,
    /// Up to [`SAMPLE`] of the edges removed (or that would be removed).
    pub sample: Vec<SampleEdge>,
}

/// Remove every edge from a `.rs` consumer to a producer that is not Rust, and
/// write back the orphan row of each producer left with no row at all.
///
/// The repair before v4 deleted a symbol's NULL row after recording its false
/// consumers, so deleting the edges alone would make the symbol vanish from the
/// graph instead of reading as the orphan it is. The orphan row is written
/// through the producer write path (`register_pub_symbol_counted`), with the
/// kind and visibility the false edges copied from it; one transaction, so a
/// failure leaves the graph as it was.
pub(crate) fn purge_cross_language_edges(
    db: &FileKnowledgeDB,
    dry_run: bool,
) -> Result<PurgeOutcome, String> {
    let conn = db.conn_ref();
    let producers = cross_language_producers(conn)?;
    let mut outcome = PurgeOutcome {
        edges: count_cross_language_edges(conn)?,
        symbols: producers.len(),
        orphans_restored: 0,
        dry_run,
        sample: cross_language_sample(conn)?,
    };
    if outcome.edges > 0 {
        outcome.orphans_restored = if dry_run {
            producers_left_bare(conn, &producers)?
        } else {
            apply_purge(db, &producers)?
        };
    }
    Ok(outcome)
}

/// A producer a cross-language edge points at: `(module, symbol, kind,
/// visibility)`, with the kind and visibility the edge copied from its row.
type Producer = (String, String, String, String);

/// A producer keeps a row when any row that is NOT a cross-language edge (its
/// own NULL row, or a real consumer) survives the purge.
fn keeps_a_row() -> String {
    format!(
        "SELECT EXISTS (SELECT 1 FROM wiring_map
          WHERE module_file = ?1 AND symbol_name = ?2 AND NOT {})",
        cross_language()
    )
}

fn purge_error(what: &str, e: rusqlite::Error) -> String {
    format!("purge: {what}: {e}")
}

fn count_cross_language_edges(conn: &rusqlite::Connection) -> Result<usize, String> {
    conn.query_row(
        &format!("SELECT COUNT(*) FROM wiring_map WHERE {}", cross_language()),
        [],
        |row| row.get::<_, i64>(0),
    )
    .map(|count| count as usize)
    .map_err(|e| purge_error("count", e))
}

fn cross_language_producers(conn: &rusqlite::Connection) -> Result<Vec<Producer>, String> {
    let mut stmt = conn
        .prepare(&format!(
            "SELECT module_file, symbol_name, MIN(symbol_kind), MIN(visibility)
             FROM wiring_map WHERE {}
             GROUP BY module_file, symbol_name ORDER BY module_file, symbol_name",
            cross_language()
        ))
        .map_err(|e| purge_error("producers", e))?;
    let rows = stmt
        .query_map([], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })
        .map_err(|e| purge_error("producers", e))?;
    rows.collect::<Result<_, _>>()
        .map_err(|e| purge_error("producers", e))
}

fn cross_language_sample(conn: &rusqlite::Connection) -> Result<Vec<SampleEdge>, String> {
    let mut stmt = conn
        .prepare(&format!(
            "SELECT module_file, symbol_name, consumer_file FROM wiring_map
             WHERE {}
             ORDER BY module_file, symbol_name, consumer_file LIMIT {SAMPLE}",
            cross_language()
        ))
        .map_err(|e| purge_error("sample", e))?;
    let rows = stmt
        .query_map([], |row| {
            Ok(SampleEdge {
                module_file: row.get(0)?,
                symbol_name: row.get(1)?,
                consumer_file: row.get(2)?,
            })
        })
        .map_err(|e| purge_error("sample", e))?;
    rows.collect::<Result<_, _>>()
        .map_err(|e| purge_error("sample", e))
}

/// Dry run: the producers that the purge would leave with no row at all.
fn producers_left_bare(
    conn: &rusqlite::Connection,
    producers: &[Producer],
) -> Result<usize, String> {
    let mut bare = 0;
    let keeps = keeps_a_row();
    for (module, symbol, _, _) in producers {
        let kept: bool = conn
            .query_row(&keeps, params![module, symbol], |row| row.get(0))
            .map_err(|e| purge_error("preview", e))?;
        bare += usize::from(!kept);
    }
    Ok(bare)
}

/// Deletes every cross-language edge and writes back the orphan row of each
/// producer left bare, in one transaction. Returns the rows written back.
fn apply_purge(db: &FileKnowledgeDB, producers: &[Producer]) -> Result<usize, String> {
    let tx = db
        .conn_ref()
        .unchecked_transaction()
        .map_err(|e| purge_error("begin", e))?;
    tx.execute(
        &format!("DELETE FROM wiring_map WHERE {}", cross_language()),
        [],
    )
    .map_err(|e| purge_error("delete", e))?;
    let mut restored = 0;
    let keeps = keeps_a_row();
    for (module, symbol, kind, visibility) in producers {
        let kept: bool = tx
            .query_row(&keeps, params![module, symbol], |row| row.get(0))
            .map_err(|e| purge_error("check", e))?;
        if !kept {
            let written = db
                .register_pub_symbol_counted(
                    module,
                    symbol,
                    kind,
                    visibility,
                    WiringOrigin::AstDeclared,
                )
                .map_err(|e| purge_error("restore", e))?;
            restored += usize::from(written);
        }
    }
    tx.commit().map_err(|e| purge_error("commit", e))?;
    FileKnowledgeDB::invalidate_wiring_modules_cache();
    Ok(restored)
}

/// CLI handler for wiring repair.
///
/// Payload: `{"dry_run": bool, "limit": number|null, "offset": number|null,
/// "purge_cross_language": bool}` (sent by `touring wiring repair [--dry-run]
/// [--limit N] [--offset N] [--purge-cross-language]`).
///
/// Returns:
/// ```json
/// {
///   "status": "repaired",
///   "dry_run": false,
///   "scanned": 500,
///   "symbols_with_consumers": 57,
///   "symbols_repaired": 123,
///   "skipped_non_rust": 0,
///   "sample": [{"module_file": "…", "symbol": "…", "consumer_file": "…"}],
///   "limit": 500,
///   "offset": 0,
///   "next_offset": 500
/// }
/// ```
/// `--purge-cross-language` answers `{"status": "purged"|"dry_run", "mode":
/// "purge_cross_language", "edges", "symbols", "orphans_restored", "sample"}`;
/// `{"status": "error", "message": "..."}` on failure.
pub fn cli_repair_wiring(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let dry_run = payload
        .get("dry_run")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    if payload
        .get("purge_cross_language")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        return purge_reply(&rt.ctx.knowledge, dry_run);
    }
    let limit = payload
        .get("limit")
        .and_then(|v| v.as_i64())
        .filter(|n| *n > 0)
        .unwrap_or(DEFAULT_REPAIR_LIMIT);
    let offset = payload
        .get("offset")
        .and_then(|v| v.as_i64())
        .filter(|n| *n >= 0)
        .unwrap_or(0);

    match repair_wiring_consumer_tracking(rt, dry_run, limit, offset) {
        Ok(outcome) => {
            let mut reply = serde_json::json!({
                "status": if outcome.dry_run { "dry_run" } else { "repaired" },
                "dry_run": outcome.dry_run,
                "scanned": outcome.scanned,
                "symbols_with_consumers": outcome.symbols_with_consumers,
                "symbols_repaired": outcome.repaired,
                "skipped_non_rust": outcome.skipped_non_rust,
                "sample": outcome.sample.iter().map(SampleEdge::to_json).collect::<Vec<_>>(),
                "limit": limit,
                "offset": offset,
                "next_offset": outcome.next_offset
            });
            if outcome.skipped_non_rust > 0 {
                reply["note"] = serde_json::json!(
                    "the repair reads Rust `use` lines and repairs Rust producers only; \
                     orphans of other languages are wired by `touring index rebuild`"
                );
            }
            reply.to_string()
        }
        Err(e) => serde_json::json!({
            "status": "error",
            "message": e
        })
        .to_string(),
    }
}

/// The reply of `touring wiring repair --purge-cross-language [--dry-run]`.
fn purge_reply(db: &FileKnowledgeDB, dry_run: bool) -> String {
    match purge_cross_language_edges(db, dry_run) {
        Ok(outcome) => serde_json::json!({
            "status": if outcome.dry_run { "dry_run" } else { "purged" },
            "mode": "purge_cross_language",
            "dry_run": outcome.dry_run,
            "edges": outcome.edges,
            "symbols": outcome.symbols,
            "orphans_restored": outcome.orphans_restored,
            "sample": outcome.sample.iter().map(SampleEdge::to_json).collect::<Vec<_>>(),
        })
        .to_string(),
        Err(e) => serde_json::json!({ "status": "error", "message": e }).to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A project with polyglot wiring on (the analise's mode), its runtime,
    /// and a Rust consumer file.
    fn polyglot_project(files: &[(&str, &str)]) -> (tempfile::TempDir, HookRuntime) {
        let proj = tempfile::tempdir().expect("project tmpdir");
        let root = proj.path();
        std::fs::create_dir_all(root.join(".touring")).expect(".touring");
        std::fs::write(
            root.join(".touring/touring.toml"),
            "polyglot_wiring = true\n",
        )
        .expect("touring.toml");
        for (path, content) in files {
            let full = root.join(path);
            std::fs::create_dir_all(full.parent().expect("parent")).expect("dirs");
            std::fs::write(full, content).expect("file");
        }
        let rt = HookRuntime::new(root).expect("HookRuntime::new");
        (proj, rt)
    }

    fn producer(rt: &HookRuntime, module: &str, symbol: &str) {
        assert!(
            rt.ctx
                .knowledge
                .register_pub_symbol_counted(
                    module,
                    symbol,
                    "struct",
                    "public",
                    WiringOrigin::AstDeclared
                )
                .expect("register"),
            "{module}::{symbol} refused by the write gate"
        );
    }

    fn rows(rt: &HookRuntime, symbol: &str) -> Vec<(String, Option<String>, String)> {
        rt.ctx
            .knowledge
            .conn_ref()
            .prepare(
                "SELECT module_file, consumer_file, contract_source FROM wiring_map
                 WHERE symbol_name = ?1 ORDER BY module_file, consumer_file",
            )
            .expect("prepare")
            .query_map(params![symbol], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .expect("query")
            .collect::<Result<_, _>>()
            .expect("rows")
    }

    /// 18/09/2026 (analise): the repair wired Python orphans from Rust `use`
    /// lines, and credited `No` for a line that imports `NodeId`.
    #[test]
    fn the_repair_wires_only_rust_producers_by_the_word_they_import() {
        let (_proj, mut rt) = polyglot_project(&[
            (
                "src/main.rs",
                "use crate::model::{Widget, NodeId};\nuse other::No;\nfn main() {}\n",
            ),
            // A re-export is not a use: `Node` stays an orphan.
            ("src/lib.rs", "pub use crate::model::Node;\n"),
        ]);
        for symbol in ["Widget", "Node", "NodeId"] {
            producer(&rt, "src/model.rs", symbol);
        }
        producer(&rt, "pkg/model.py", "No");

        let preview = repair_wiring_consumer_tracking(&mut rt, true, 100, 0).expect("dry run");
        assert_eq!(
            preview.skipped_non_rust, 1,
            "the Python orphan is not the repair's"
        );
        let previewed: Vec<(&str, &str)> = preview
            .sample
            .iter()
            .map(|e| (e.symbol_name.as_str(), e.consumer_file.as_str()))
            .collect();
        assert_eq!(
            previewed,
            [("NodeId", "src/main.rs"), ("Widget", "src/main.rs")]
        );
        assert_eq!(rows(&rt, "Widget")[0].1, None, "a dry run writes nothing");

        let run = repair_wiring_consumer_tracking(&mut rt, false, 100, 0).expect("repair");
        assert_eq!(run.symbols_with_consumers, 2);
        let widget = rows(&rt, "Widget");
        assert_eq!(
            widget,
            [
                (
                    "src/model.rs".to_string(),
                    None,
                    WiringOrigin::AstDeclared.as_str().to_string()
                ),
                (
                    "src/model.rs".to_string(),
                    Some("src/main.rs".to_string()),
                    WiringOrigin::AstInferred.as_str().to_string()
                ),
            ],
            "the declaration stays beside the edge, and a name match is a guess"
        );
        let again = repair_wiring_consumer_tracking(&mut rt, true, 100, 0).expect("second pass");
        assert_eq!(
            again.symbols_with_consumers, 0,
            "a wired symbol has left the orphan scan"
        );
        assert_eq!(
            rows(&rt, "Node")[0].1,
            None,
            "`Node` is not imported by `NodeId`"
        );
        assert_eq!(
            rows(&rt, "No"),
            [(
                "pkg/model.py".to_string(),
                None,
                WiringOrigin::AstDeclared.as_str().to_string()
            )],
            "a Python orphan stays an orphan"
        );
    }

    /// The pre-v4 repair left `.rs` → Python edges and deleted the Python
    /// symbol's NULL row. The purge removes the edges, writes the orphan row
    /// back where nothing else wires the symbol, and leaves real edges alone.
    #[test]
    fn purge_removes_cross_language_edges_and_restores_the_orphans() {
        let (_proj, rt) = polyglot_project(&[]);
        producer(&rt, "pkg/model.py", "Aresta");
        producer(&rt, "src/model.rs", "Widget");
        let conn = rt.ctx.knowledge.conn_ref();
        conn.execute_batch(
            "INSERT INTO wiring_map (module_file, symbol_name, symbol_kind, visibility, consumer_file, contract_source)
             VALUES ('pkg/model.py', 'No', 'class', 'public', 'benches/gpu.rs', 'ast_resolved'),
                    ('pkg/model.py', 'No', 'class', 'public', 'src/lib.rs', 'ast_resolved'),
                    ('pkg/model.py', 'Aresta', 'class', 'public', 'src/lib.rs', 'ast_resolved'),
                    ('pkg/model.py', 'Aresta', 'class', 'public', 'pkg/app.py', 'ast_resolved'),
                    ('src/model.rs', 'Widget', 'struct', 'public', 'src/main.rs', 'ast_resolved');",
        )
        .expect("legacy rows");

        let preview = purge_cross_language_edges(&rt.ctx.knowledge, true).expect("preview");
        assert_eq!(
            (preview.edges, preview.symbols, preview.orphans_restored),
            (3, 2, 1)
        );
        assert_eq!(preview.sample.len(), 3);
        assert_eq!(rows(&rt, "No").len(), 2, "a dry run writes nothing");

        let run = purge_cross_language_edges(&rt.ctx.knowledge, false).expect("purge");
        assert_eq!((run.edges, run.orphans_restored), (3, 1));
        let no = rows(&rt, "No");
        assert_eq!(no.len(), 1, "{no:?}");
        assert_eq!(no[0].1, None, "`No` reads as the orphan it is again");
        let aresta: Vec<Option<String>> = rows(&rt, "Aresta").into_iter().map(|r| r.1).collect();
        assert_eq!(aresta, [None, Some("pkg/app.py".to_string())]);
        let widget: Vec<Option<String>> = rows(&rt, "Widget").into_iter().map(|r| r.1).collect();
        assert_eq!(
            widget,
            [None, Some("src/main.rs".to_string())],
            "Rust edges stay"
        );
        assert_eq!(
            purge_cross_language_edges(&rt.ctx.knowledge, false)
                .expect("again")
                .edges,
            0,
            "idempotent"
        );
    }
}
