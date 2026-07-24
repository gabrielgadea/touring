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

use crate::runtime::HookRuntime;
use crate::wiring::WiringEntry;
use rusqlite::params;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::process::Command;

/// Default per-run repair batch when the caller does not pass `--limit`.
/// With chunked greps (~50 symbols each) this stays well under the daemon's
/// 5s request timeout.
const DEFAULT_REPAIR_LIMIT: i64 = 500;

/// Symbols per grep alternation. Keeps the regex small enough for grep -E
/// while amortising the workspace walk across many symbols.
const GREP_CHUNK: usize = 50;

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
}

/// Repair wiring consumer tracking by rescanning for actual consumers.
///
/// Examines up to `limit` orphan symbols starting at `offset` (ordered by
/// module_file, symbol_name) and greps the workspace for real consumers in
/// alternation chunks. Unless `dry_run`, records found consumers in
/// wiring_map and deletes the stale NULL row for those symbols.
pub fn repair_wiring_consumer_tracking(
    rt: &mut HookRuntime,
    dry_run: bool,
    limit: i64,
    offset: i64,
) -> Result<RepairOutcome, String> {
    let db = &rt.ctx.knowledge;
    let conn = db.conn_ref();

    // 1. Page of orphan symbols (consumer_file IS NULL).
    let orphan_entries: Vec<WiringEntry> = {
        let sql = "SELECT w.module_file, w.symbol_name, w.symbol_kind, w.visibility,
                   w.consumer_file, w.import_line, w.contract_source
            FROM wiring_map w
            WHERE w.consumer_file IS NULL AND w.visibility = 'public'
              AND NOT EXISTS (
                  SELECT 1 FROM wiring_map w2
                  WHERE w2.module_file = w.module_file
                    AND w2.symbol_name = w.symbol_name
                    AND w2.consumer_file IS NOT NULL
              )
            ORDER BY w.module_file, w.symbol_name
            LIMIT ?1 OFFSET ?2";

        let mut stmt = conn
            .prepare(sql)
            .map_err(|e| format!("failed to prepare orphan query: {}", e))?;

        stmt.query_map(params![limit, offset], |row| {
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
        .map_err(|e| format!("failed to query orphans: {}", e))?
        .filter_map(|r| r.ok())
        .collect::<Vec<_>>()
    };

    let scanned = orphan_entries.len();
    let mut outcome = RepairOutcome {
        scanned,
        repaired: 0,
        symbols_with_consumers: 0,
        dry_run,
        next_offset: if (scanned as i64) < limit {
            None
        } else {
            Some(offset + scanned as i64)
        },
    };
    if orphan_entries.is_empty() {
        return Ok(outcome);
    }

    let project_root = rt.project_root.clone();
    let root_str = project_root.to_str().unwrap_or("");

    // 2. Candidates: importable identifiers only ([A-Za-z0-9_] — safe to
    //    embed in a grep -E alternation without escaping).
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

    // 3. One grep per chunk of symbols (alternation), instead of one per
    //    symbol — the workspace walk dominates, so amortise it.
    for chunk in candidates.chunks(GREP_CHUNK) {
        let alternation = chunk
            .iter()
            .map(|e| e.symbol_name.as_str())
            .collect::<Vec<_>>()
            .join("|");
        let pattern = format!(
            r"^[[:space:]]*(pub[[:space:]]+)?use\b.*\b({})\b",
            alternation
        );

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
            let Some((file, rest)) = line.split_once(':') else {
                continue;
            };
            let consumer_path = Path::new(file);
            let Ok(rel) = consumer_path.strip_prefix(&project_root) else {
                continue;
            };
            let rel_str = rel.to_string_lossy().to_string();
            if rel_str.is_empty() {
                continue;
            }
            // Which chunk symbols appear on this line?
            for entry in chunk {
                let sym = entry.symbol_name.as_str();
                if rest.contains(sym)
                    && rel_str != entry.module_file
                    && !rel_str.contains(&entry.module_file)
                {
                    found.entry(sym).or_default().insert(rel_str.clone());
                }
            }
        }
    }

    // 4. Record consumers + delete the stale NULL row per repaired symbol.
    for entry in &orphan_entries {
        let sym = entry.symbol_name.as_str();
        let Some(consumers) = found.get(sym) else {
            continue;
        };
        if consumers.is_empty() {
            continue;
        }
        outcome.symbols_with_consumers += 1;

        if dry_run {
            outcome.repaired += consumers.len();
            continue;
        }

        for consumer_file in consumers {
            match db.record_consumer(&entry.module_file, sym, consumer_file, None) {
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

        // Delete the NULL row for THIS symbol only (v2 fixed a bug where this
        // was gated on a run-global counter, erasing untracked orphans).
        let delete_sql = "DELETE FROM wiring_map
                          WHERE module_file = ?1
                            AND symbol_name = ?2
                            AND consumer_file IS NULL";
        if let Err(e) = conn.execute(delete_sql, params![entry.module_file, sym]) {
            eprintln!(
                "repair: failed to delete orphan entry for {}::{}: {}",
                entry.module_file, sym, e
            );
        }
    }

    Ok(outcome)
}

/// CLI handler for wiring repair.
///
/// Payload: `{"dry_run": bool, "limit": number|null, "offset": number|null}`
/// (sent by `touring wiring repair [--dry-run] [--limit N] [--offset N]`).
///
/// Returns:
/// ```json
/// {
///   "status": "repaired",
///   "dry_run": false,
///   "scanned": 500,
///   "symbols_with_consumers": 57,
///   "symbols_repaired": 123,
///   "limit": 500,
///   "offset": 0,
///   "next_offset": 500
/// }
/// ```
/// or `{"status": "error", "message": "..."}` on failure.
pub fn cli_repair_wiring(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let dry_run = payload
        .get("dry_run")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
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
        Ok(outcome) => serde_json::json!({
            "status": if outcome.dry_run { "dry_run" } else { "repaired" },
            "dry_run": outcome.dry_run,
            "scanned": outcome.scanned,
            "symbols_with_consumers": outcome.symbols_with_consumers,
            "symbols_repaired": outcome.repaired,
            "limit": limit,
            "offset": offset,
            "next_offset": outcome.next_offset
        })
        .to_string(),
        Err(e) => serde_json::json!({
            "status": "error",
            "message": e
        })
        .to_string(),
    }
}
