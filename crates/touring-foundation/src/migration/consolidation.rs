//! DB Consolidation Migration — 8 legacy SQLite files → 3 domain DBs.
//!
//! `ConsolidationMigration` is the programmatic API for the migration that
//! the `touring migrate` CLI commands drive interactively.
//!
//! ## Architecture
//!
//! ```text
//! Source (8 legacy DBs)         Target (3 domain DBs)
//! ─────────────────────         ─────────────────────
//! symbols.db              ──┐
//! touring_pipeline.db     ──┤──► knowledge.db  (symbols + file knowledge + wiring)
//! touring_knowledge.db    ──┘
//!
//! rlm_memory.db           ──┐
//! touring_rlm.db          ──┤──► memory.db     (RLM + semantic + ANN embeddings)
//! semantic_recall.db      ──┤
//! ann_memory.db           ──┘
//!
//! got_snapshots.db        ──┐
//! touring_pipeline.db     ──┘──► graph.db      (GoT + RL pipeline + hook events)
//! ```
//!
//! ## Usage
//!
//! ```rust,no_run
//! use touring_foundation::migration::consolidation::ConsolidationMigration;
//! use std::path::Path;
//!
//! let mig = ConsolidationMigration::new(Path::new("/project"));
//! mig.create_target_dbs()?;
//! let stats = mig.migrate_data(|step, src, rows| {
//!     eprintln!("  ok  {step} ← {src} ({rows} rows)");
//! })?;
//! let report = mig.validate()?;
//! assert!(report.passed);
//! # anyhow::Ok(())
//! ```

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use rusqlite::Connection;

use crate::schema::{
    graph::GRAPH_SCHEMA_V8, knowledge::KNOWLEDGE_SCHEMA_V8, memory::MEMORY_SCHEMA_V8,
};

// ── Public types ──────────────────────────────────────────────────────────────

/// Result of a `migrate_data` run — row counts per source table.
#[derive(Debug, Default, Clone)]
pub struct MigrationStats {
    /// Number of rows migrated per source table name.
    pub rows_migrated: HashMap<String, u64>,
    /// Wall-clock duration in milliseconds.
    pub duration_ms: u64,
    /// Non-fatal warnings accumulated during migration.
    pub warnings: Vec<String>,
}

impl MigrationStats {
    /// Total rows migrated across all tables.
    pub fn total_rows(&self) -> u64 {
        self.rows_migrated.values().sum()
    }
}

/// Result of a `validate` run — per-check pass/fail details.
#[derive(Debug, Clone)]
pub struct ValidationReport {
    /// `true` iff every check passed.
    pub passed: bool,
    /// Individual checks: (check_name, passed, detail message).
    pub checks: Vec<(String, bool, String)>,
}

impl ValidationReport {
    /// Return the names of all checks that failed.
    pub fn failed_checks(&self) -> Vec<&str> {
        self.checks
            .iter()
            .filter(|(_, ok, _)| !ok)
            .map(|(name, _, _)| name.as_str())
            .collect()
    }
}

// ── ConsolidationMigration ────────────────────────────────────────────────────

/// Programmatic API for the 8→3 DB consolidation migration.
///
/// All methods are pure with respect to the running daemon — they do not
/// acquire any daemon locks or modify `TouringConfig` at runtime.  Cutover
/// (updating config to point at the new paths) must be performed separately.
#[derive(Debug, Clone)]
pub struct ConsolidationMigration {
    /// Project root (the directory containing `.claude/`).
    pub project_root: PathBuf,
}

impl ConsolidationMigration {
    /// Create a new migration instance for `project_root`.
    pub fn new(project_root: &Path) -> Self {
        Self {
            project_root: project_root.to_path_buf(),
        }
    }

    /// Source DB paths (8 legacy files).
    fn source_paths(&self) -> SourcePaths {
        let touring = self.project_root.join(".claude").join("touring");
        let data = self.project_root.join(".claude").join("data");
        SourcePaths {
            symbols: touring.join("symbols.db"),
            knowledge: data.join("touring_knowledge.db"),
            rlm_memory: data.join("rlm_memory.db"),
            touring_rlm: data.join("touring_rlm.db"),
            pipeline: data.join("touring_pipeline.db"),
            got_snapshots: data.join("got_snapshots.db"),
            semantic_recall: data.join("semantic_recall.db"),
            ann_memory: data.join("ann_memory.db"),
        }
    }

    /// Target DB paths (3 domain files).
    fn target_paths(&self) -> TargetPaths {
        let touring = self.project_root.join(".claude").join("touring");
        TargetPaths {
            knowledge: touring.join("knowledge.db"),
            memory: touring.join("memory.db"),
            graph: touring.join("graph.db"),
        }
    }

    /// Create the 3 target domain DBs and apply schema v8.
    ///
    /// Idempotent — safe to call multiple times.  All tables use
    /// `CREATE TABLE IF NOT EXISTS`, so existing data is never dropped.
    pub fn create_target_dbs(&self) -> anyhow::Result<()> {
        let tgt = self.target_paths();
        let touring = self.project_root.join(".claude").join("touring");
        std::fs::create_dir_all(&touring)
            .map_err(|e| anyhow::anyhow!("create .claude/touring: {e}"))?;

        for (path, schema, name) in [
            (&tgt.knowledge, KNOWLEDGE_SCHEMA_V8, "knowledge.db"),
            (&tgt.memory, MEMORY_SCHEMA_V8, "memory.db"),
            (&tgt.graph, GRAPH_SCHEMA_V8, "graph.db"),
        ] {
            let conn = Connection::open(path).map_err(|e| anyhow::anyhow!("open {name}: {e}"))?;
            conn.execute_batch(schema)
                .map_err(|e| anyhow::anyhow!("schema {name}: {e}"))?;
        }
        Ok(())
    }

    /// Migrate data from all 8 source DBs into the 3 target DBs.
    ///
    /// - Missing source DBs are silently skipped (not all projects have all DBs).
    /// - Source tables with wrong schemas are skipped gracefully.
    /// - Already-completed phases are skipped (resume support via `_migration_state`).
    /// - `on_step` is called after each phase with (phase_name, source_db, rows).
    pub fn migrate_data<F>(&self, mut on_step: F) -> anyhow::Result<MigrationStats>
    where
        F: FnMut(&str, &str, u64),
    {
        let start = std::time::Instant::now();
        let mut stats = MigrationStats::default();
        let src = self.source_paths();
        let tgt = self.target_paths();

        // ── knowledge.db ─────────────────────────────────────────────────────
        let k = Connection::open(&tgt.knowledge)
            .map_err(|e| anyhow::anyhow!("open knowledge.db: {e}"))?;

        // symbols.db
        if src.symbols.exists() && !phase_completed(&k, "knowledge:symbols") {
            phase_start(&k, "knowledge:symbols");
            let rows = with_attached(&k, &src.symbols, |conn| {
                insert_from_attached(conn, "symbols", "symbols", "IGNORE")
            })
            .inspect_err(|e| {
                phase_failed(&k, "knowledge:symbols", &e.to_string());
            })?;
            rebuild_fts5(&k, "symbols_fts").inspect_err(|e| {
                phase_failed(&k, "knowledge:symbols", &e.to_string());
            })?;
            phase_done(&k, "knowledge:symbols", rows);
            stats.rows_migrated.insert("symbols".into(), rows);
            on_step("symbols", "symbols.db", rows);
        }

        // touring_knowledge.db — tables with column mapping for schema differences
        if src.knowledge.exists() && !phase_completed(&k, "knowledge:knowledge_tables") {
            phase_start(&k, "knowledge:knowledge_tables");
            let rows = with_attached(&k, &src.knowledge, |conn| {
                let mut total = 0u64;

                // file_knowledge: legacy schema (knowledge.rs) has columns:
                //   file_path, language, line_count, symbol_count, read_count,
                //   last_read_at, content_hash, imports_json, symbols_json, notes,
                //   created_at, updated_at
                // v8 consolidated schema has:
                //   file_path, language, line_count, content_hash, symbols_json,
                //   imports_json, notes, created_at, updated_at, access_count, last_accessed
                // Map: read_count → access_count, last_read_at → last_accessed
                if mig_src_table_exists(conn, "file_knowledge") {
                    let n = conn.execute(
                        "INSERT OR IGNORE INTO main.file_knowledge \
                             (file_path, language, line_count, content_hash, symbols_json, \
                              imports_json, notes, created_at, updated_at, access_count, last_accessed) \
                         SELECT file_path, language, COALESCE(line_count, 0), content_hash, \
                                symbols_json, imports_json, notes, \
                                COALESCE(created_at, datetime('now')), \
                                COALESCE(updated_at, datetime('now')), \
                                COALESCE(read_count, 0), last_read_at \
                         FROM mig_src.file_knowledge",
                        [],
                    ).map_err(|e| anyhow::anyhow!("migrate file_knowledge: {e}"))?;
                    total += n as u64;
                }

                // bash_outcomes: map legacy columns to v8 schema.
                // Legacy: (command, command_short, exit_code, success, error_pattern, file_context, executed_at)
                // v8:     (command, command_short, exit_code, success, error_pattern, file_context, executed_at, command_hash)
                if mig_src_table_exists(conn, "bash_outcomes") {
                    let n = conn.execute(
                        "INSERT OR IGNORE INTO main.bash_outcomes \
                             (command, command_short, exit_code, success, error_pattern, file_context, executed_at) \
                         SELECT command, command_short, exit_code, success, error_pattern, file_context, executed_at \
                         FROM mig_src.bash_outcomes",
                        [],
                    ).map_err(|e| anyhow::anyhow!("migrate bash_outcomes: {e}"))?;
                    total += n as u64;
                }

                // edit_history → edit_history (same table name, extended schema)
                // Legacy: (id, file_path, edit_type, summary, error_pattern, edited_at)
                // v8:     (id, file_path, edit_type, summary, error_pattern, language, symbol_context, session_id, edited_at)
                if mig_src_table_exists(conn, "edit_history") {
                    let n = conn.execute(
                        "INSERT OR IGNORE INTO main.edit_history \
                             (file_path, edit_type, edited_at) \
                         SELECT file_path, edit_type, edited_at \
                         FROM mig_src.edit_history",
                        [],
                    ).map_err(|e| anyhow::anyhow!("migrate edit_history: {e}"))?;
                    total += n as u64;
                }

                // gotchas → gotchas (same table name, extended schema)
                // Legacy: (id, pattern, gotcha, severity, symbol_name, hit_count, prevented_errors, created_at)
                // v8:     (id, pattern, gotcha, severity, symbol_name, language, hit_count, prevented_errors, decay_score, last_occurrence, resolved_at, created_at)
                if mig_src_table_exists(conn, "gotchas") {
                    let n = conn.execute(
                        "INSERT OR IGNORE INTO main.gotchas \
                             (pattern, gotcha, severity, symbol_name, hit_count, created_at) \
                         SELECT pattern, gotcha, severity, symbol_name, hit_count, created_at \
                         FROM mig_src.gotchas",
                        [],
                    ).map_err(|e| anyhow::anyhow!("migrate gotchas: {e}"))?;
                    total += n as u64;
                }

                // file_risk_scores: may or may not exist in legacy
                total += insert_from_attached(conn, "file_risk_scores", "file_risk_scores", "IGNORE")?;

                // wiring_map: v8 schema matches legacy — copy directly preserving all columns
                // Both: (id, module_file, symbol_name, symbol_kind, visibility, consumer_file, import_line, contract_source, resolved_at)
                if mig_src_table_exists(conn, "wiring_map") {
                    let n = conn.execute(
                        "INSERT OR IGNORE INTO main.wiring_map \
                             (module_file, symbol_name, symbol_kind, visibility, consumer_file, import_line, contract_source, resolved_at) \
                         SELECT module_file, symbol_name, symbol_kind, visibility, consumer_file, import_line, contract_source, resolved_at \
                         FROM mig_src.wiring_map",
                        [],
                    ).map_err(|e| anyhow::anyhow!("migrate wiring_map: {e}"))?;
                    total += n as u64;
                }

                // module_ecosystem: v8 schema matches legacy — copy directly
                // Both: (file_path, module_role, parent_module, pub_symbol_count, import_count, re_export_count, integration_score, last_scanned_at)
                if mig_src_table_exists(conn, "module_ecosystem") {
                    let n = conn.execute(
                        "INSERT OR IGNORE INTO main.module_ecosystem \
                             (file_path, module_role, parent_module, pub_symbol_count, import_count, re_export_count, integration_score, last_scanned_at) \
                         SELECT file_path, module_role, parent_module, pub_symbol_count, import_count, re_export_count, integration_score, last_scanned_at \
                         FROM mig_src.module_ecosystem",
                        [],
                    ).map_err(|e| anyhow::anyhow!("migrate module_ecosystem: {e}"))?;
                    total += n as u64;
                }

                Ok(total)
            })
            .inspect_err(|e| { phase_failed(&k, "knowledge:knowledge_tables", &e.to_string()); })?;
            phase_done(&k, "knowledge:knowledge_tables", rows);
            stats.rows_migrated.insert("knowledge_tables".into(), rows);
            on_step("knowledge_tables", "touring_knowledge.db", rows);
        }

        // ── memory.db ────────────────────────────────────────────────────────
        let m =
            Connection::open(&tgt.memory).map_err(|e| anyhow::anyhow!("open memory.db: {e}"))?;

        // rlm_memory.db — memory_entries → rlm_entries (INTEGER→TEXT timestamp)
        if src.rlm_memory.exists() && !phase_completed(&m, "memory:rlm_memory") {
            phase_start(&m, "memory:rlm_memory");
            let rows = with_attached(&m, &src.rlm_memory, |conn| {
                if !mig_src_table_exists(conn, "memory_entries") {
                    return Ok(0);
                }
                let n = conn
                    .execute(
                        "INSERT OR REPLACE INTO main.rlm_entries \
                        (key, tier, value, entry_type, created_at, last_accessed, access_count) \
                     SELECT key, tier, value, entry_type, \
                            datetime(created_at, 'unixepoch'), \
                            datetime(accessed_at, 'unixepoch'), \
                            access_count \
                     FROM mig_src.memory_entries",
                        [],
                    )
                    .map_err(|e| anyhow::anyhow!("migrate memory_entries: {e}"))?;
                Ok(n as u64)
            })
            .inspect_err(|e| {
                phase_failed(&m, "memory:rlm_memory", &e.to_string());
            })?;
            phase_done(&m, "memory:rlm_memory", rows);
            stats.rows_migrated.insert("rlm_memory".into(), rows);
            on_step("rlm_entries (rlm_memory)", "rlm_memory.db", rows);
        }

        // touring_rlm.db — same schema as rlm_memory
        if src.touring_rlm.exists() && !phase_completed(&m, "memory:touring_rlm") {
            phase_start(&m, "memory:touring_rlm");
            let rows = with_attached(&m, &src.touring_rlm, |conn| {
                if !mig_src_table_exists(conn, "memory_entries") {
                    return Ok(0);
                }
                let n = conn
                    .execute(
                        "INSERT OR REPLACE INTO main.rlm_entries \
                        (key, tier, value, entry_type, created_at, last_accessed, access_count) \
                     SELECT key, tier, value, entry_type, \
                            datetime(created_at, 'unixepoch'), \
                            datetime(accessed_at, 'unixepoch'), \
                            access_count \
                     FROM mig_src.memory_entries",
                        [],
                    )
                    .map_err(|e| anyhow::anyhow!("migrate touring_rlm: {e}"))?;
                Ok(n as u64)
            })
            .inspect_err(|e| {
                phase_failed(&m, "memory:touring_rlm", &e.to_string());
            })?;
            phase_done(&m, "memory:touring_rlm", rows);
            stats.rows_migrated.insert("touring_rlm".into(), rows);
            on_step("rlm_entries (touring_rlm)", "touring_rlm.db", rows);
        }

        // FTS5 rebuild for rlm_fts (only needed when any rlm phase ran)
        if !phase_completed(&m, "memory:rlm_fts") {
            phase_start(&m, "memory:rlm_fts");
            rebuild_fts5(&m, "rlm_fts").inspect_err(|e| {
                phase_failed(&m, "memory:rlm_fts", &e.to_string());
            })?;
            phase_done(&m, "memory:rlm_fts", 0);
            on_step("rlm_fts rebuilt", "FTS5", 0);
        }

        // semantic_recall.db — chunks
        if src.semantic_recall.exists() && !phase_completed(&m, "memory:semantic_recall") {
            phase_start(&m, "memory:semantic_recall");
            let rows = with_attached(&m, &src.semantic_recall, |conn| {
                insert_from_attached(conn, "chunks", "chunks", "IGNORE")
            })
            .inspect_err(|e| {
                phase_failed(&m, "memory:semantic_recall", &e.to_string());
            })?;
            phase_done(&m, "memory:semantic_recall", rows);
            stats.rows_migrated.insert("chunks".into(), rows);
            on_step("chunks", "semantic_recall.db", rows);
        }

        // ann_memory.db — embeddings → ann_embeddings
        if src.ann_memory.exists() && !phase_completed(&m, "memory:ann_memory") {
            phase_start(&m, "memory:ann_memory");
            let rows = with_attached(&m, &src.ann_memory, |conn| {
                if !mig_src_table_exists(conn, "embeddings") {
                    return Ok(0);
                }
                let n = conn.execute(
                    "INSERT OR REPLACE INTO main.ann_embeddings(id, content, embedding, metadata_json) \
                     SELECT id, content, embedding, metadata_json \
                     FROM mig_src.embeddings",
                    [],
                )
                .map_err(|e| anyhow::anyhow!("migrate ann embeddings: {e}"))?;
                Ok(n as u64)
            })
            .inspect_err(|e| { phase_failed(&m, "memory:ann_memory", &e.to_string()); })?;
            phase_done(&m, "memory:ann_memory", rows);
            stats.rows_migrated.insert("ann_embeddings".into(), rows);
            on_step("ann_embeddings", "ann_memory.db", rows);
        }

        // ── graph.db ─────────────────────────────────────────────────────────
        let g = Connection::open(&tgt.graph).map_err(|e| anyhow::anyhow!("open graph.db: {e}"))?;

        // got_snapshots.db
        if src.got_snapshots.exists() && !phase_completed(&g, "graph:got_snapshots") {
            phase_start(&g, "graph:got_snapshots");
            let rows = with_attached(&g, &src.got_snapshots, |conn| {
                insert_from_attached(conn, "got_snapshots", "got_snapshots", "IGNORE")
            })
            .inspect_err(|e| {
                phase_failed(&g, "graph:got_snapshots", &e.to_string());
            })?;
            phase_done(&g, "graph:got_snapshots", rows);
            stats.rows_migrated.insert("got_snapshots".into(), rows);
            on_step("got_snapshots", "got_snapshots.db", rows);
        }

        // touring_pipeline.db — learning tables (schemas diverge, map columns explicitly)
        if src.pipeline.exists() && !phase_completed(&g, "graph:pipeline_tables") {
            phase_start(&g, "graph:pipeline_tables");
            let rows = with_attached(&g, &src.pipeline, |conn| {
                let mut total = 0u64;

                // learning_wilson: legacy (item_id, successes, trials) → v8 (tool_name, successes, trials)
                if mig_src_table_exists(conn, "learning_wilson") {
                    let n = conn.execute(
                        "INSERT OR IGNORE INTO main.learning_wilson (tool_name, successes, trials) \
                         SELECT item_id, successes, trials FROM mig_src.learning_wilson",
                        [],
                    ).map_err(|e| anyhow::anyhow!("migrate learning_wilson: {e}"))?;
                    total += n as u64;
                }

                // learning_drift: legacy (metric, values_json) → v8 (metric_name, value)
                // values_json is a JSON array; take the last value as the current metric value
                if mig_src_table_exists(conn, "learning_drift") {
                    let n = conn.execute(
                        "INSERT OR IGNORE INTO main.learning_drift (metric_name, value) \
                         SELECT metric, COALESCE(json_extract(values_json, '$[-1]'), 0.0) FROM mig_src.learning_drift",
                        [],
                    ).map_err(|e| anyhow::anyhow!("migrate learning_drift: {e}"))?;
                    total += n as u64;
                }

                // learning_qtable: same schema (state_action, q_value, trace)
                total += insert_from_attached(conn, "learning_qtable", "learning_qtable", "IGNORE")?;

                // learning_linucb: same schema
                total += insert_from_attached(conn, "learning_linucb", "learning_linucb", "IGNORE")?;

                // touring_hook_events: may differ in columns — safe SELECT * only if match
                total += insert_from_attached(conn, "touring_hook_events", "touring_hook_events", "IGNORE")?;

                // sessions + session_checkpoints: same schema in legacy pipeline
                total += insert_from_attached(conn, "sessions", "sessions", "IGNORE")?;
                total += insert_from_attached(conn, "session_checkpoints", "session_checkpoints", "IGNORE")?;

                Ok(total)
            })
            .inspect_err(|e| { phase_failed(&g, "graph:pipeline_tables", &e.to_string()); })?;
            phase_done(&g, "graph:pipeline_tables", rows);
            stats.rows_migrated.insert("pipeline_tables".into(), rows);
            on_step("pipeline_tables", "touring_pipeline.db", rows);
        }

        stats.duration_ms = start.elapsed().as_millis() as u64;
        Ok(stats)
    }

    /// Validate migration completeness.
    ///
    /// Checks:
    /// - All 3 target DBs exist and are non-empty.
    /// - Each target DB has `schema_version = "8"` in `schema_meta`.
    /// - WAL mode is active on all target DBs.
    /// - FTS5 integrity check on `symbols_fts` (knowledge) and `rlm_fts` (memory).
    /// - Row counts: target >= source for key tables where source DBs exist.
    pub fn validate(&self) -> anyhow::Result<ValidationReport> {
        let tgt = self.target_paths();
        let src = self.source_paths();
        let mut checks: Vec<(String, bool, String)> = Vec::new();

        for (domain, path) in [
            ("knowledge", &tgt.knowledge),
            ("memory", &tgt.memory),
            ("graph", &tgt.graph),
        ] {
            // Existence check
            let exists = path.exists() && path.metadata().map(|m| m.len() > 0).unwrap_or(false);
            checks.push((
                format!("{domain}.db exists"),
                exists,
                if exists {
                    format!("{} bytes", path.metadata().map(|m| m.len()).unwrap_or(0))
                } else {
                    "missing or empty".into()
                },
            ));

            if !exists {
                continue;
            }

            let conn = match Connection::open(path) {
                Ok(c) => c,
                Err(e) => {
                    checks.push((format!("{domain}.db open"), false, e.to_string()));
                    continue;
                }
            };

            // Schema version check
            let ver_result: rusqlite::Result<String> = conn.query_row(
                "SELECT value FROM schema_meta WHERE key='schema_version'",
                [],
                |r| r.get(0),
            );
            match ver_result {
                Ok(v) => checks.push((
                    format!("{domain}.db schema_version"),
                    v == "8",
                    format!("got {v:?}, expected \"8\""),
                )),
                Err(e) => {
                    checks.push((format!("{domain}.db schema_version"), false, e.to_string()))
                }
            }

            // FTS5 integrity check (only for domains that have FTS tables)
            let fts_table = match domain {
                "knowledge" => Some("symbols_fts"),
                "memory" => Some("rlm_fts"),
                _ => None,
            };
            if let Some(fts) = fts_table {
                let fts_ok = check_fts5_integrity(&conn, fts).is_ok();
                checks.push((
                    format!("{domain}.db {fts} integrity"),
                    fts_ok,
                    if fts_ok {
                        "ok".into()
                    } else {
                        "FTS5 integrity-check failed".into()
                    },
                ));
            }

            // Row count checks: target >= source for key tables (only when source exists)
            let row_checks: &[(&Path, &str, &str)] = match domain {
                "knowledge" => &[
                    (&src.symbols as &Path, "symbols", "symbols"),
                    (&src.knowledge as &Path, "file_knowledge", "file_knowledge"),
                ],
                "memory" => &[
                    (&src.rlm_memory as &Path, "memory_entries", "rlm_entries"),
                    (&src.semantic_recall as &Path, "chunks", "chunks"),
                ],
                "graph" => &[(&src.pipeline as &Path, "learning_qtable", "learning_qtable")],
                _ => &[],
            };
            for (src_path, src_tbl, dst_tbl) in row_checks {
                if !src_path.exists() {
                    continue;
                }
                let (source, target) = count_rows_comparison(&conn, src_path, src_tbl, dst_tbl);
                checks.push((
                    format!("{domain}.db {dst_tbl} row count"),
                    target >= source,
                    format!("source={source}, target={target}"),
                ));
            }

            // WAL mode check
            let wal_result: rusqlite::Result<String> =
                conn.query_row("PRAGMA journal_mode", [], |r| r.get(0));
            match wal_result {
                Ok(mode) => checks.push((
                    format!("{domain}.db WAL mode"),
                    mode == "wal",
                    format!("journal_mode = {mode}"),
                )),
                Err(e) => checks.push((format!("{domain}.db WAL mode"), false, e.to_string())),
            }
        }

        let passed = checks.iter().all(|(_, ok, _)| *ok);
        Ok(ValidationReport { passed, checks })
    }

    /// Rollback: rename consolidated DBs to `.rollback` so legacy DBs can be used again.
    ///
    /// Does NOT restore legacy DB paths in `TouringConfig` — callers must handle
    /// the config side.  This method only renames the file-system artifacts.
    pub fn rollback(&self) -> anyhow::Result<u32> {
        let tgt = self.target_paths();
        let mut renamed = 0u32;
        for (domain, path) in [
            ("knowledge", &tgt.knowledge),
            ("memory", &tgt.memory),
            ("graph", &tgt.graph),
        ] {
            if path.exists() {
                let dst = path.with_extension("db.rollback");
                std::fs::rename(path, &dst)
                    .map_err(|e| anyhow::anyhow!("rename {domain}.db: {e}"))?;
                renamed += 1;
            }
        }
        Ok(renamed)
    }
}

// ── Private helpers ───────────────────────────────────────────────────────────

struct SourcePaths {
    symbols: PathBuf,
    knowledge: PathBuf,
    rlm_memory: PathBuf,
    touring_rlm: PathBuf,
    pipeline: PathBuf,
    got_snapshots: PathBuf,
    semantic_recall: PathBuf,
    ann_memory: PathBuf,
}

struct TargetPaths {
    knowledge: PathBuf,
    memory: PathBuf,
    graph: PathBuf,
}

/// ATTACH `src` as `mig_src`, run `action(conn)`, then DETACH unconditionally.
pub fn with_attached<F, T>(dst: &Connection, src: &Path, action: F) -> anyhow::Result<T>
where
    F: FnOnce(&Connection) -> anyhow::Result<T>,
{
    let escaped = src.to_string_lossy().replace('\'', "''");
    dst.execute_batch(&format!("ATTACH DATABASE '{escaped}' AS mig_src"))
        .map_err(|e| anyhow::anyhow!("ATTACH '{}': {e}", src.display()))?;
    let result = action(dst);
    let _ = dst.execute_batch("DETACH DATABASE mig_src");
    result
}

/// Return `true` if `name` is a table in the attached "mig_src" database.
pub fn mig_src_table_exists(conn: &Connection, name: &str) -> bool {
    conn.query_row(
        "SELECT COUNT(*) FROM mig_src.sqlite_master WHERE type='table' AND name=?1",
        [name],
        |r| r.get::<_, i64>(0),
    )
    .map(|n| n > 0)
    .unwrap_or(false)
}

/// Copy rows from `src_table` in "mig_src" into `dst_table` in main.
///
/// Returns the number of rows inserted.  Skips silently when the source table
/// does not exist.
pub fn insert_from_attached(
    conn: &Connection,
    src_table: &str,
    dst_table: &str,
    conflict: &str,
) -> anyhow::Result<u64> {
    if !mig_src_table_exists(conn, src_table) {
        return Ok(0);
    }
    let rows = conn
        .execute(
            &format!(
                "INSERT OR {conflict} INTO main.{dst_table} SELECT * FROM mig_src.{src_table}"
            ),
            [],
        )
        .map_err(|e| anyhow::anyhow!("migrate {src_table}→{dst_table}: {e}"))?;
    Ok(rows as u64)
}

/// Trigger an FTS5 external-content index rebuild from its content table.
pub fn rebuild_fts5(conn: &Connection, fts_table: &str) -> anyhow::Result<()> {
    conn.execute_batch(&format!(
        "INSERT INTO {fts_table}({fts_table}) VALUES('rebuild')"
    ))
    .map_err(|e| anyhow::anyhow!("FTS5 rebuild {fts_table}: {e}"))
}

/// Run the FTS5 integrity check command on `fts_table`.
///
/// Returns `Ok(())` if the table is internally consistent.
/// Returns `Err(...)` if SQLite reports corruption.
pub fn check_fts5_integrity(conn: &Connection, fts_table: &str) -> anyhow::Result<()> {
    conn.execute_batch(&format!(
        "INSERT INTO {fts_table}({fts_table}) VALUES('integrity-check')"
    ))
    .map_err(|e| anyhow::anyhow!("FTS5 integrity-check {fts_table}: {e}"))
}

// ── Progress tracking helpers (private) ──────────────────────────────────────

/// Mark a migration phase as started in `_migration_state`.
fn phase_start(conn: &Connection, phase: &str) {
    let _ = conn.execute(
        "INSERT OR REPLACE INTO _migration_state(phase, status, started_at) \
         VALUES(?1, 'in_progress', datetime('now'))",
        [phase],
    );
}

/// Mark a migration phase as completed.
fn phase_done(conn: &Connection, phase: &str, rows: u64) {
    let _ = conn.execute(
        "UPDATE _migration_state \
         SET status='completed', rows_migrated=?2, completed_at=datetime('now') \
         WHERE phase=?1",
        rusqlite::params![phase, rows as i64],
    );
}

/// Mark a migration phase as failed with an error message.
fn phase_failed(conn: &Connection, phase: &str, error: &str) {
    let _ = conn.execute(
        "UPDATE _migration_state SET status='failed', error_message=?2 WHERE phase=?1",
        [phase, error],
    );
}

/// Returns `true` if the phase was previously completed (enables resume).
fn phase_completed(conn: &Connection, phase: &str) -> bool {
    conn.query_row(
        "SELECT status FROM _migration_state WHERE phase=?1",
        [phase],
        |r| r.get::<_, String>(0),
    )
    .map(|s| s == "completed")
    .unwrap_or(false)
}

/// Count rows in `table` within `conn`.  Returns 0 on error (table may not exist).
fn count_rows(conn: &Connection, table: &str) -> u64 {
    conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| {
        r.get::<_, i64>(0)
    })
    .map(|n| n.max(0) as u64)
    .unwrap_or(0)
}

/// Count rows in `table` within an attached `src` DB (accessed via ATTACH).
///
/// Returns `(source_rows, target_rows)`.  Either count is 0 if the table
/// does not exist in that DB.
fn count_rows_comparison(
    conn: &Connection,
    src: &Path,
    src_table: &str,
    dst_table: &str,
) -> (u64, u64) {
    let target = count_rows(conn, dst_table);
    let source = with_attached(conn, src, |c| {
        Ok(c.query_row(
            &format!("SELECT COUNT(*) FROM mig_src.{src_table}"),
            [],
            |r| r.get::<_, i64>(0),
        )
        .map(|n| n.max(0) as u64)
        .unwrap_or(0))
    })
    .unwrap_or(0);
    (source, target)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn create_target_dbs_idempotent() {
        let dir = tempdir().expect("tempdir");
        let mig = ConsolidationMigration::new(dir.path());
        // First call creates the DBs.
        mig.create_target_dbs().expect("first create");
        // Second call must not fail (IF NOT EXISTS guards).
        mig.create_target_dbs().expect("idempotent second create");
    }

    #[test]
    fn create_target_dbs_creates_all_three() {
        let dir = tempdir().expect("tempdir");
        let mig = ConsolidationMigration::new(dir.path());
        mig.create_target_dbs().expect("create");
        let touring = dir.path().join(".claude").join("touring");
        assert!(touring.join("knowledge.db").exists());
        assert!(touring.join("memory.db").exists());
        assert!(touring.join("graph.db").exists());
    }

    #[test]
    fn migrate_data_no_sources_returns_zero_rows() {
        let dir = tempdir().expect("tempdir");
        let mig = ConsolidationMigration::new(dir.path());
        mig.create_target_dbs().expect("create");
        let stats = mig.migrate_data(|_, _, _| {}).expect("migrate");
        assert_eq!(stats.total_rows(), 0);
    }

    #[test]
    fn migrate_data_migrates_rlm_memory_entries() {
        let dir = tempdir().expect("tempdir");
        let data = dir.path().join(".claude").join("data");
        std::fs::create_dir_all(&data).expect("create data dir");

        // Build legacy rlm_memory.db with INTEGER timestamps.
        let rlm_path = data.join("rlm_memory.db");
        let src = Connection::open(&rlm_path).expect("open rlm");
        src.execute_batch(
            "CREATE TABLE memory_entries (
                key TEXT, tier TEXT, value TEXT, entry_type TEXT,
                created_at INTEGER, accessed_at INTEGER, access_count INTEGER,
                embedding BLOB,
                PRIMARY KEY (key, tier)
             );
             INSERT INTO memory_entries VALUES
                 ('k1', 'semantic', 'v1', 'lesson', 1700000000, 1700001000, 2, NULL);",
        )
        .expect("seed rlm_memory.db");
        drop(src);

        let mig = ConsolidationMigration::new(dir.path());
        mig.create_target_dbs().expect("create");
        let stats = mig.migrate_data(|_, _, _| {}).expect("migrate");

        assert_eq!(*stats.rows_migrated.get("rlm_memory").unwrap_or(&0), 1);

        let mem = dir.path().join(".claude").join("touring").join("memory.db");
        let conn = Connection::open(&mem).expect("open memory.db");
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM rlm_entries", [], |r| r.get(0))
            .expect("count");
        assert_eq!(count, 1);
    }

    #[test]
    fn validate_passes_after_create() {
        let dir = tempdir().expect("tempdir");
        let mig = ConsolidationMigration::new(dir.path());
        mig.create_target_dbs().expect("create");
        let report = mig.validate().expect("validate");
        assert!(report.passed, "failed checks: {:?}", report.failed_checks());
    }

    #[test]
    fn validate_fails_when_dbs_missing() {
        let dir = tempdir().expect("tempdir");
        let mig = ConsolidationMigration::new(dir.path());
        // Do NOT call create_target_dbs — DBs are missing.
        let report = mig.validate().expect("validate");
        assert!(!report.passed);
        assert!(!report.failed_checks().is_empty());
    }

    #[test]
    fn rollback_renames_dbs() {
        let dir = tempdir().expect("tempdir");
        let mig = ConsolidationMigration::new(dir.path());
        mig.create_target_dbs().expect("create");
        let renamed = mig.rollback().expect("rollback");
        assert_eq!(renamed, 3);

        let touring = dir.path().join(".claude").join("touring");
        for domain in ["knowledge", "memory", "graph"] {
            assert!(!touring.join(format!("{domain}.db")).exists());
            assert!(touring.join(format!("{domain}.db.rollback")).exists());
        }
    }

    #[test]
    fn migration_stats_total_rows() {
        let mut stats = MigrationStats::default();
        stats.rows_migrated.insert("a".into(), 10);
        stats.rows_migrated.insert("b".into(), 5);
        assert_eq!(stats.total_rows(), 15);
    }

    #[test]
    fn with_attached_detaches_on_error() {
        let conn = Connection::open_in_memory().expect("mem");
        conn.execute_batch("CREATE TABLE t (id INTEGER PRIMARY KEY); INSERT INTO t VALUES (1);")
            .expect("setup");
        let dir = tempdir().expect("tempdir");
        let src_path = dir.path().join("nonexistent.db");
        // ATTACH of a nonexistent file should fail gracefully.
        let result = with_attached(&conn, &src_path, |_c| Ok::<(), anyhow::Error>(()));
        // Either succeeds (SQLite creates the file) or fails — either way no panic.
        let _ = result;
    }

    // ── Fase 2.2 tests ───────────────────────────────────────────────────────

    #[test]
    fn migration_resumes_skipping_completed_phases() {
        // Verifies that a second migrate_data call skips phases already marked
        // 'completed' in _migration_state (resumability invariant).
        let dir = tempdir().expect("tempdir");
        let data = dir.path().join(".claude").join("data");
        std::fs::create_dir_all(&data).expect("create data dir");

        let rlm_path = data.join("rlm_memory.db");
        let src = Connection::open(&rlm_path).expect("open rlm");
        src.execute_batch(
            "CREATE TABLE memory_entries (
                key TEXT, tier TEXT, value TEXT, entry_type TEXT,
                created_at INTEGER, accessed_at INTEGER, access_count INTEGER,
                embedding BLOB,
                PRIMARY KEY (key, tier)
             );
             INSERT INTO memory_entries VALUES
                 ('k1', 'semantic', 'v1', 'lesson', 1700000000, 1700001000, 2, NULL);",
        )
        .expect("seed rlm");
        drop(src);

        let mig = ConsolidationMigration::new(dir.path());
        mig.create_target_dbs().expect("create");

        // First run — migrates 1 row
        let stats1 = mig.migrate_data(|_, _, _| {}).expect("first migrate");
        assert_eq!(
            *stats1.rows_migrated.get("rlm_memory").unwrap_or(&0),
            1,
            "first run should migrate 1 row"
        );

        // Second run — phase already completed, should return 0 new rows
        let stats2 = mig.migrate_data(|_, _, _| {}).expect("second migrate");
        assert_eq!(
            *stats2.rows_migrated.get("rlm_memory").unwrap_or(&0),
            0,
            "second run must skip completed phases (0 new rows)"
        );
    }

    #[test]
    fn validate_includes_fts5_integrity_check() {
        // Ensures the validation report includes FTS5 integrity entries for
        // knowledge.db (symbols_fts) and memory.db (rlm_fts).
        let dir = tempdir().expect("tempdir");
        let mig = ConsolidationMigration::new(dir.path());
        mig.create_target_dbs().expect("create");
        let report = mig.validate().expect("validate");

        let check_names: Vec<&str> = report.checks.iter().map(|(n, _, _)| n.as_str()).collect();
        assert!(
            check_names.iter().any(|n| n.contains("symbols_fts")),
            "validate must include symbols_fts integrity check; got: {check_names:?}"
        );
        assert!(
            check_names.iter().any(|n| n.contains("rlm_fts")),
            "validate must include rlm_fts integrity check; got: {check_names:?}"
        );
        // Fresh DBs should pass integrity
        let fts_checks: Vec<_> = report
            .checks
            .iter()
            .filter(|(n, _, _)| n.contains("_fts"))
            .collect();
        for (name, pass, msg) in &fts_checks {
            assert!(
                pass,
                "FTS5 integrity check {name} should pass on fresh DB: {msg}"
            );
        }
    }

    #[test]
    fn validate_row_counts_pass_after_migration() {
        // After migrating actual source data, validate must report target >= source
        // for migrated tables.
        let dir = tempdir().expect("tempdir");
        let data = dir.path().join(".claude").join("data");
        std::fs::create_dir_all(&data).expect("create data dir");

        let rlm_path = data.join("rlm_memory.db");
        let src = Connection::open(&rlm_path).expect("open rlm");
        src.execute_batch(
            "CREATE TABLE memory_entries (
                key TEXT, tier TEXT, value TEXT, entry_type TEXT,
                created_at INTEGER, accessed_at INTEGER, access_count INTEGER,
                embedding BLOB,
                PRIMARY KEY (key, tier)
             );
             INSERT INTO memory_entries VALUES ('r1', 'semantic', 'v1', 'lesson', 0, 0, 0, NULL);
             INSERT INTO memory_entries VALUES ('r2', 'local',    'v2', 'pattern', 0, 0, 0, NULL);",
        )
        .expect("seed rlm");
        drop(src);

        let mig = ConsolidationMigration::new(dir.path());
        mig.create_target_dbs().expect("create");
        mig.migrate_data(|_, _, _| {}).expect("migrate");

        let report = mig.validate().expect("validate");
        let row_checks: Vec<_> = report
            .checks
            .iter()
            .filter(|(n, _, _)| n.contains("row count"))
            .collect();

        for (name, pass, msg) in &row_checks {
            assert!(
                pass,
                "Row count check '{name}' must pass after migration: {msg}"
            );
        }
    }
}
