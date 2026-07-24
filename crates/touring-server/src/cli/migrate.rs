//! `touring migrate` — DB consolidation migration CLI.
//!
//! # Usage
//!
//! ```text
//! touring migrate status   — show migration status for current project
//! touring migrate plan     — show what would be migrated (dry run)
//! touring migrate run      — execute migration (8 DBs → 3 domains)
//! touring migrate validate — validate migration completeness
//! touring migrate rollback — rename new DBs to .rollback, restore old paths
//! ```
//!
//! Migration uses a Shadow DB approach: the daemon continues using the 8 legacy
//! DBs while the consolidated DBs are built in the background.  Cutover is an
//! atomic config/path swap performed in Phase 2.3.

use std::path::{Path, PathBuf};

use touring_foundation::migration::consolidation::{
    ConsolidationMigration, insert_from_attached, mig_src_table_exists, rebuild_fts5, with_attached,
};

use super::common::parse_global_flags;

// ── Root / path helpers ──────────────────────────────────────────────────────

fn find_project_root() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("TOURING_PROJECT_ROOT") {
        return Some(PathBuf::from(p));
    }
    if let Ok(p) = std::env::var("CLAUDE_PROJECT_DIR") {
        return Some(PathBuf::from(p));
    }
    let mut dir = std::env::current_dir().ok()?;
    loop {
        if dir.join(".claude").join("touring").is_dir() {
            return Some(dir);
        }
        if !dir.pop() {
            return None;
        }
    }
}

fn db_exists(path: &Path) -> bool {
    path.exists() && path.metadata().map(|m| m.len() > 0).unwrap_or(false)
}

// Migration primitives are defined in touring_foundation::migration::consolidation
// and re-imported at the top of this file.  This CLI module focuses on
// argument parsing, user-facing output, and orchestrating the library calls.

/// Log a migration step to stdout in human or JSON format.
fn log_step(json: bool, step: &str, source: &str, rows: u64) {
    if json {
        println!(
            "{}",
            serde_json::json!({
                "step": step,
                "source": source,
                "rows": rows,
            })
        );
    } else {
        println!("  ok  {step:<42} ← {source} ({rows} rows)");
    }
}

// ── Subcommands ──────────────────────────────────────────────────────────────

fn cmd_status(args: &[String]) -> anyhow::Result<()> {
    let (flags, _) = parse_global_flags(args);

    let root = find_project_root().ok_or_else(|| {
        anyhow::anyhow!(
            "could not locate project root — run from a Touring project or set TOURING_PROJECT_ROOT"
        )
    })?;

    let data = root.join(".claude").join("data");
    let touring = root.join(".claude").join("touring");

    let sources = [
        ("symbols.db", touring.join("symbols.db")),
        ("touring_knowledge", data.join("touring_knowledge.db")),
        ("rlm_memory", data.join("rlm_memory.db")),
        ("touring_rlm", data.join("touring_rlm.db")),
        ("touring_pipeline", data.join("touring_pipeline.db")),
        ("got_snapshots", data.join("got_snapshots.db")),
        ("semantic_recall", data.join("semantic_recall.db")),
        ("ann_memory", data.join("ann_memory.db")),
    ];

    let targets = [
        ("knowledge.db", touring.join("knowledge.db")),
        ("memory.db", touring.join("memory.db")),
        ("graph.db", touring.join("graph.db")),
    ];

    if flags.json {
        let src_json: serde_json::Value = sources
            .iter()
            .map(|(name, path)| {
                serde_json::json!({
                    "name": name,
                    "exists": db_exists(path),
                    "path": path.display().to_string()
                })
            })
            .collect::<Vec<_>>()
            .into();
        let dst_json: serde_json::Value = targets
            .iter()
            .map(|(name, path)| {
                serde_json::json!({
                    "name": name,
                    "exists": db_exists(path),
                    "path": path.display().to_string()
                })
            })
            .collect::<Vec<_>>()
            .into();
        println!(
            "{}",
            serde_json::json!({ "sources": src_json, "targets": dst_json })
        );
    } else {
        println!("=== Migration Status ===");
        println!("\nSource DBs (8 legacy):");
        for (name, path) in &sources {
            let status = if db_exists(path) { "exists" } else { "missing" };
            println!("  [{status}] {name}: {}", path.display());
        }
        println!("\nTarget DBs (3 consolidated):");
        for (name, path) in &targets {
            let status = if db_exists(path) {
                "exists"
            } else {
                "not yet created"
            };
            println!("  [{status}] {name}: {}", path.display());
        }
        let all_targets_exist = targets.iter().all(|(_, p)| db_exists(p));
        if all_targets_exist {
            println!("\nStatus: MIGRATION COMPLETE");
        } else {
            println!("\nStatus: PENDING — run `touring migrate run` to execute");
        }
    }
    Ok(())
}

fn cmd_plan(args: &[String]) -> anyhow::Result<()> {
    let (flags, _) = parse_global_flags(args);
    let root =
        find_project_root().ok_or_else(|| anyhow::anyhow!("could not locate project root"))?;

    let plan = [
        (
            "symbols → knowledge.db",
            "symbols, dependencies, symbols_fts",
        ),
        (
            "touring_knowledge → knowledge.db",
            "file_knowledge, bash_outcomes, file_edit_history, file_gotchas, file_risk_scores, wiring_map, module_ecosystem",
        ),
        (
            "rlm_memory + touring_rlm → memory.db",
            "rlm_entries (merged, newest wins), rlm_fts",
        ),
        ("semantic_recall → memory.db", "recall_embeddings"),
        (
            "ann_memory → memory.db",
            "ann_embeddings (renamed from embeddings), ann_meta (renamed from meta)",
        ),
        ("got_snapshots → graph.db", "got_snapshots"),
        (
            "touring_pipeline (learning) → graph.db",
            "learning_wilson, learning_qtable, learning_drift, learning_tool_outcomes, touring_hook_events, sessions, session_checkpoints",
        ),
    ];

    if flags.json {
        let steps: Vec<serde_json::Value> = plan
            .iter()
            .enumerate()
            .map(|(i, (src, tables))| {
                serde_json::json!({ "step": i + 1, "source": src, "tables": tables })
            })
            .collect();
        println!(
            "{}",
            serde_json::json!({
                "project_root": root.display().to_string(),
                "steps": steps
            })
        );
    } else {
        println!("=== Migration Plan for {} ===\n", root.display());
        for (i, (src, tables)) in plan.iter().enumerate() {
            println!("Step {}: {src}", i + 1);
            println!("   Tables: {tables}\n");
        }
        println!("To execute: touring migrate run");
    }
    Ok(())
}

fn cmd_validate(args: &[String]) -> anyhow::Result<()> {
    let (flags, _) = parse_global_flags(args);
    let root =
        find_project_root().ok_or_else(|| anyhow::anyhow!("could not locate project root"))?;

    let report = ConsolidationMigration::new(&root).validate()?;

    if flags.json {
        let checks: Vec<serde_json::Value> = report
            .checks
            .iter()
            .map(|(name, pass, msg)| {
                serde_json::json!({ "check": name, "pass": pass, "message": msg })
            })
            .collect();
        println!(
            "{}",
            serde_json::json!({ "all_pass": report.passed, "checks": checks })
        );
    } else {
        println!("=== Migration Validation ===");
        for (name, pass, msg) in &report.checks {
            let icon = if *pass { "PASS" } else { "FAIL" };
            println!("  [{icon}] {name}: {msg}");
        }
        if report.passed {
            println!("\nResult: ALL CHECKS PASSED");
        } else {
            anyhow::bail!("validation failed — run `touring migrate run` first");
        }
    }
    Ok(())
}

fn cmd_run(args: &[String]) -> anyhow::Result<()> {
    use touring_foundation::schema::{
        graph::GRAPH_SCHEMA_V8, knowledge::KNOWLEDGE_SCHEMA_V8, memory::MEMORY_SCHEMA_V8,
    };

    let (flags, _) = parse_global_flags(args);
    let root = find_project_root()
        .ok_or_else(|| anyhow::anyhow!("project root not found — set TOURING_PROJECT_ROOT"))?;
    let touring = root.join(".claude").join("touring");
    let data = root.join(".claude").join("data");
    std::fs::create_dir_all(&touring)?;

    // Source paths (8 legacy databases).
    let symbols_src = touring.join("symbols.db");
    let knowledge_src = data.join("touring_knowledge.db");
    let rlm_memory_src = data.join("rlm_memory.db");
    let touring_rlm_src = data.join("touring_rlm.db");
    let pipeline_src = data.join("touring_pipeline.db");
    let got_src = data.join("got_snapshots.db");
    let semantic_src = data.join("semantic_recall.db");
    let ann_src = data.join("ann_memory.db");

    // Open / create the 3 target domain DBs.  Schemas are idempotent via
    // CREATE TABLE IF NOT EXISTS, so repeated runs are safe.
    let k = rusqlite::Connection::open(touring.join("knowledge.db"))
        .map_err(|e| anyhow::anyhow!("open knowledge.db: {e}"))?;
    k.execute_batch(KNOWLEDGE_SCHEMA_V8)?;

    let m = rusqlite::Connection::open(touring.join("memory.db"))
        .map_err(|e| anyhow::anyhow!("open memory.db: {e}"))?;
    m.execute_batch(MEMORY_SCHEMA_V8)?;

    let g = rusqlite::Connection::open(touring.join("graph.db"))
        .map_err(|e| anyhow::anyhow!("open graph.db: {e}"))?;
    // Pre-migrate legacy columns before CREATE TABLE IF NOT EXISTS becomes a no-op.
    // Sprint 4.8 schema drift fix — adds touring_hook_events.hook_name/file_path/etc.
    for col_decl in &[
        "hook_name TEXT",
        "file_path TEXT",
        "duration_ms INTEGER",
        "success INTEGER",
        "context_json TEXT",
        "session_id TEXT",
    ] {
        let col = col_decl.split_whitespace().next().unwrap_or("");
        let sql = format!("ALTER TABLE touring_hook_events ADD COLUMN {col_decl}");
        if let Err(e) = g.execute(&sql, []) {
            let msg = e.to_string();
            if !msg.contains("duplicate column name") && !msg.contains("no such table") {
                return Err(anyhow::anyhow!("alter touring_hook_events.{col}: {e}"));
            }
        }
    }
    g.execute_batch(GRAPH_SCHEMA_V8)?;

    if !flags.json {
        println!("=== Touring DB Migration (8 → 3 domains) ===\n");
        println!("knowledge.db:");
    }

    let mut errors: Vec<String> = Vec::new();
    let mut total: u64 = 0;

    // ── knowledge: symbols.db → symbols, dependencies ─────────────────────
    if db_exists(&symbols_src) {
        match with_attached(&k, &symbols_src, |conn| {
            let r1 = insert_from_attached(conn, "symbols", "symbols", "IGNORE")?;
            let r2 = insert_from_attached(conn, "dependencies", "dependencies", "IGNORE")?;
            Ok(r1 + r2)
        }) {
            Ok(rows) => {
                total += rows;
                log_step(flags.json, "symbols + dependencies", "symbols.db", rows);
            }
            Err(e) => {
                let msg = format!("symbols.db: {e}");
                eprintln!("  ERR {msg}");
                errors.push(msg);
            }
        }
        // FTS5 rebuild: symbols_fts is an external-content table (content='symbols').
        match rebuild_fts5(&k, "symbols_fts") {
            Err(e) => {
                eprintln!("  ERR symbols_fts rebuild: {e}");
            }
            _ => {
                if !flags.json {
                    println!("  ok  symbols_fts rebuilt");
                }
            }
        }
    }

    // ── knowledge: touring_knowledge.db → 7 tables ────────────────────────
    if db_exists(&knowledge_src) {
        match with_attached(&k, &knowledge_src, |conn| {
            let mut rows = 0u64;
            for tbl in &[
                "file_knowledge",
                "bash_outcomes",
                "file_edit_history",
                "file_gotchas",
                "file_risk_scores",
                "wiring_map",
                "module_ecosystem",
            ] {
                rows += insert_from_attached(conn, tbl, tbl, "IGNORE")?;
            }
            Ok(rows)
        }) {
            Ok(rows) => {
                total += rows;
                log_step(
                    flags.json,
                    "knowledge tables (7)",
                    "touring_knowledge.db",
                    rows,
                );
            }
            Err(e) => {
                let msg = format!("touring_knowledge.db: {e}");
                eprintln!("  ERR {msg}");
                errors.push(msg);
            }
        }
    }

    if !flags.json {
        println!("\nmemory.db:");
    }

    // ── memory: rlm_memory.db → rlm_entries (base, IGNORE on conflict) ────
    // Source table is "memory_entries" (created by RlmMemory::new()), NOT "rlm_entries".
    // Timestamps are INTEGER (Unix epoch) in source; TEXT in consolidated schema.
    if db_exists(&rlm_memory_src) {
        match with_attached(&m, &rlm_memory_src, |conn| {
            if !mig_src_table_exists(conn, "memory_entries") {
                return Ok(0);
            }
            let rows = conn
                .execute(
                    "INSERT OR IGNORE INTO main.rlm_entries \
                     (key, value, tier, entry_type, access_count, created_at, last_accessed) \
                     SELECT key, value, tier, entry_type, access_count, \
                            datetime(created_at, 'unixepoch'), \
                            datetime(accessed_at, 'unixepoch') \
                     FROM mig_src.memory_entries",
                    [],
                )
                .map_err(|e| anyhow::anyhow!("migrate memory_entries→rlm_entries: {e}"))?;
            Ok(rows as u64)
        }) {
            Ok(rows) => {
                total += rows;
                log_step(flags.json, "rlm_entries (base)", "rlm_memory.db", rows);
            }
            Err(e) => {
                let msg = format!("rlm_memory.db: {e}");
                eprintln!("  ERR {msg}");
                errors.push(msg);
            }
        }
    }

    // ── memory: touring_rlm.db → rlm_entries (merge; REPLACE = newest wins) ─
    // Same schema as rlm_memory.db: source table is "memory_entries", INTEGER timestamps.
    if db_exists(&touring_rlm_src) {
        match with_attached(&m, &touring_rlm_src, |conn| {
            if !mig_src_table_exists(conn, "memory_entries") {
                return Ok(0);
            }
            let rows = conn
                .execute(
                    "INSERT OR REPLACE INTO main.rlm_entries \
                     (key, value, tier, entry_type, access_count, created_at, last_accessed) \
                     SELECT key, value, tier, entry_type, access_count, \
                            datetime(created_at, 'unixepoch'), \
                            datetime(accessed_at, 'unixepoch') \
                     FROM mig_src.memory_entries",
                    [],
                )
                .map_err(|e| anyhow::anyhow!("migrate memory_entries→rlm_entries (merge): {e}"))?;
            Ok(rows as u64)
        }) {
            Ok(rows) => {
                total += rows;
                log_step(
                    flags.json,
                    "rlm_entries (merge/newest-wins)",
                    "touring_rlm.db",
                    rows,
                );
            }
            Err(e) => {
                let msg = format!("touring_rlm.db: {e}");
                eprintln!("  ERR {msg}");
                errors.push(msg);
            }
        }
    }

    // Rebuild rlm_fts after both rlm_entries sources are merged.
    match rebuild_fts5(&m, "rlm_fts") {
        Err(e) => {
            eprintln!("  ERR rlm_fts rebuild: {e}");
        }
        _ => {
            if !flags.json {
                println!("  ok  rlm_fts rebuilt");
            }
        }
    }

    // ── memory: semantic_recall.db → recall_embeddings ────────────────────
    // Source may use "recall_embeddings" (new layout) or legacy "chunks" table.
    if db_exists(&semantic_src) {
        match with_attached(&m, &semantic_src, |conn| {
            if mig_src_table_exists(conn, "recall_embeddings") {
                insert_from_attached(conn, "recall_embeddings", "recall_embeddings", "IGNORE")
            } else if mig_src_table_exists(conn, "chunks") {
                // Legacy schema: chunks(id, content, embedding, metadata_json)
                // → recall_embeddings(chunk_id, content, embedding, metadata_json, created_at)
                let rows = conn
                    .execute(
                        "INSERT OR IGNORE INTO main.recall_embeddings
                             (chunk_id, content, embedding, metadata_json, created_at)
                         SELECT CAST(id AS TEXT), content, embedding,
                                metadata_json, datetime('now')
                         FROM mig_src.chunks",
                        [],
                    )
                    .map_err(|e| anyhow::anyhow!("chunks→recall_embeddings: {e}"))?;
                Ok(rows as u64)
            } else {
                Ok(0) // No matching source table — skip.
            }
        }) {
            Ok(rows) => {
                total += rows;
                log_step(flags.json, "recall_embeddings", "semantic_recall.db", rows);
            }
            Err(e) => {
                let msg = format!("semantic_recall.db: {e}");
                eprintln!("  ERR {msg}");
                errors.push(msg);
            }
        }
    }

    // ── memory: ann_memory.db → ann_embeddings + ann_meta ─────────────────
    // Source table names differ: "embeddings" → "ann_embeddings", "meta" → "ann_meta".
    if db_exists(&ann_src) {
        match with_attached(&m, &ann_src, |conn| {
            let r1 = insert_from_attached(conn, "embeddings", "ann_embeddings", "IGNORE")?;
            let r2 = insert_from_attached(conn, "meta", "ann_meta", "IGNORE")?;
            Ok(r1 + r2)
        }) {
            Ok(rows) => {
                total += rows;
                log_step(
                    flags.json,
                    "ann_embeddings + ann_meta",
                    "ann_memory.db",
                    rows,
                );
            }
            Err(e) => {
                let msg = format!("ann_memory.db: {e}");
                eprintln!("  ERR {msg}");
                errors.push(msg);
            }
        }
    }

    if !flags.json {
        println!("\ngraph.db:");
    }

    // ── graph: got_snapshots.db → got_snapshots ───────────────────────────
    if db_exists(&got_src) {
        match with_attached(&g, &got_src, |conn| {
            insert_from_attached(conn, "got_snapshots", "got_snapshots", "IGNORE")
        }) {
            Ok(rows) => {
                total += rows;
                log_step(flags.json, "got_snapshots", "got_snapshots.db", rows);
            }
            Err(e) => {
                let msg = format!("got_snapshots.db: {e}");
                eprintln!("  ERR {msg}");
                errors.push(msg);
            }
        }
    }

    // ── graph: touring_pipeline.db → learning + session tables ───────────
    if db_exists(&pipeline_src) {
        match with_attached(&g, &pipeline_src, |conn| {
            let mut rows = 0u64;
            for tbl in &[
                "learning_wilson",
                "learning_drift",
                "learning_tool_outcomes",
                "touring_hook_events",
                "sessions",
                "session_checkpoints",
            ] {
                rows += insert_from_attached(conn, tbl, tbl, "IGNORE")?;
            }
            // learning_qtable: source schema has (state_action TEXT, q_value REAL).
            // Destination schema has (state_action TEXT PRIMARY KEY, q_value REAL, trace REAL).
            // Explicit column mapping avoids SELECT * mismatch on column count.
            if mig_src_table_exists(conn, "learning_qtable") {
                let r = conn
                    .execute(
                        "INSERT OR IGNORE INTO main.learning_qtable (state_action, q_value) \
                         SELECT state_action, q_value FROM mig_src.learning_qtable",
                        [],
                    )
                    .map_err(|e| anyhow::anyhow!("migrate learning_qtable: {e}"))?;
                rows += r as u64;
            }
            Ok(rows)
        }) {
            Ok(rows) => {
                total += rows;
                log_step(
                    flags.json,
                    "learning + session tables (7)",
                    "touring_pipeline.db",
                    rows,
                );
            }
            Err(e) => {
                let msg = format!("touring_pipeline.db: {e}");
                eprintln!("  ERR {msg}");
                errors.push(msg);
            }
        }
    }

    // ── Summary ───────────────────────────────────────────────────────────
    let status = if errors.is_empty() {
        "completed"
    } else {
        "partial"
    };
    if flags.json {
        println!(
            "{}",
            serde_json::json!({
                "status": status,
                "total_rows_migrated": total,
                "errors": errors,
            })
        );
    } else {
        println!("\n=== Summary ===");
        println!("Rows migrated : {total}");
        if errors.is_empty() {
            println!("Status        : COMPLETED");
            println!("Next step     : touring migrate validate");
        } else {
            println!("Status        : PARTIAL ({} error(s))", errors.len());
            for msg in &errors {
                println!("  - {msg}");
            }
        }
    }
    Ok(())
}

/// Archive legacy source DBs to `.db.migrated` after a validated migration.
///
/// This is the final cleanup step: renames the 8 legacy databases in
/// `.claude/data/` to `<name>.db.migrated` so they no longer interfere
/// with the consolidated DBs in `.claude/touring/`.
///
/// Only archives DBs that exist at the expected legacy paths.
/// Safe to re-run (already-archived DBs are skipped).
fn cmd_cleanup(args: &[String]) -> anyhow::Result<()> {
    let (flags, _) = parse_global_flags(args);
    let root = find_project_root().ok_or_else(|| anyhow::anyhow!("project root not found"))?;

    // First verify migration was completed successfully.
    let report = ConsolidationMigration::new(&root).validate()?;
    if !report.passed {
        anyhow::bail!(
            "migration validation failed — run `touring migrate run` and \
             `touring migrate validate` before cleanup"
        );
    }

    let data = root.join(".claude").join("data");
    let touring = root.join(".claude").join("touring");
    let legacy_dbs = [
        touring.join("symbols.db"),
        data.join("touring_knowledge.db"),
        data.join("rlm_memory.db"),
        data.join("touring_rlm.db"),
        data.join("touring_pipeline.db"),
        data.join("got_snapshots.db"),
        data.join("semantic_recall.db"),
        data.join("ann_memory.db"),
    ];

    let mut archived = 0u32;
    let mut skipped = 0u32;
    for src in &legacy_dbs {
        if !src.exists() {
            skipped += 1;
            continue;
        }
        let name = src.file_name().unwrap_or_default().to_string_lossy();
        let dst = src.with_extension("db.migrated");
        if dst.exists() {
            // Already archived — skip silently.
            skipped += 1;
            continue;
        }
        std::fs::rename(src, &dst).map_err(|e| anyhow::anyhow!("archive {name}: {e}"))?;
        archived += 1;
        if flags.json {
            println!(
                "{}",
                serde_json::json!({
                    "archived": name.as_ref(),
                    "to": dst.file_name().unwrap_or_default().to_string_lossy().as_ref(),
                })
            );
        } else {
            println!(
                "  archived: {} → {}",
                name,
                dst.file_name().unwrap_or_default().to_string_lossy()
            );
        }
    }

    if !flags.json {
        println!("\nCleanup complete: {archived} archived, {skipped} skipped.");
        if archived > 0 {
            println!("Legacy DBs renamed to .db.migrated — safe to delete after verification.");
        }
    }
    Ok(())
}

fn cmd_rollback(args: &[String]) -> anyhow::Result<()> {
    let (flags, _) = parse_global_flags(args);
    let root = find_project_root().ok_or_else(|| anyhow::anyhow!("project root not found"))?;
    let touring = root.join(".claude").join("touring");

    let mut renamed = 0u32;
    for domain in ["knowledge", "memory", "graph"] {
        let src = touring.join(format!("{domain}.db"));
        if src.exists() {
            let dst = touring.join(format!("{domain}.db.rollback"));
            std::fs::rename(&src, &dst).map_err(|e| anyhow::anyhow!("rename {domain}.db: {e}"))?;
            renamed += 1;
            if flags.json {
                println!(
                    "{}",
                    serde_json::json!({
                        "renamed": format!("{domain}.db"),
                        "to": format!("{domain}.db.rollback"),
                    })
                );
            } else {
                println!("  renamed: {domain}.db → {domain}.db.rollback");
            }
        }
    }

    if !flags.json {
        if renamed > 0 {
            println!("\nRollback prepared ({renamed} DB(s) renamed).");
            println!("To restore legacy databases:");
            println!("  for f in .claude/data/*.migrated; do mv \"$f\" \"${{f%.migrated}}\"; done");
            println!("  Restart touring-daemon after restoring.");
        } else {
            println!("No consolidated databases found — nothing to roll back.");
        }
    }
    Ok(())
}

// ── Entry point ──────────────────────────────────────────────────────────────

/// Dispatch `touring migrate` subcommands.
pub fn run(args: &[String]) -> anyhow::Result<()> {
    match args.get(2).map(|s| s.as_str()) {
        Some("status") => cmd_status(args),
        Some("plan") => cmd_plan(args),
        Some("validate") => cmd_validate(args),
        Some("run") => cmd_run(args),
        Some("cleanup") => cmd_cleanup(args),
        Some("rollback") => cmd_rollback(args),
        _ => {
            eprintln!("Usage:");
            eprintln!("  touring migrate status    — show migration status");
            eprintln!("  touring migrate plan      — preview migration steps");
            eprintln!("  touring migrate validate  — validate completed migration");
            eprintln!("  touring migrate run       — execute migration (8 DBs → 3 domains)");
            eprintln!("  touring migrate cleanup   — archive legacy DBs to .db.migrated");
            eprintln!("  touring migrate rollback  — rename new DBs to .rollback");
            Ok(())
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    // Serialise all tests that mutate process-level TOURING_PROJECT_ROOT.
    // std::env::set_var is not thread-safe; without a lock, parallel tests
    // can observe each other's temp paths, causing "disk I/O error" panics.
    static ENV_MUTEX: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// Set TOURING_PROJECT_ROOT for the duration of `f`, then remove it.
    fn with_project_root<F: FnOnce()>(root: &std::path::Path, f: F) {
        let _guard = ENV_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
        // SAFETY: guarded by ENV_MUTEX — no other test mutates this var concurrently.
        unsafe { std::env::set_var("TOURING_PROJECT_ROOT", root) };
        f();
        unsafe { std::env::remove_var("TOURING_PROJECT_ROOT") };
    }

    fn make_sqlite(path: &Path) -> rusqlite::Connection {
        let conn = rusqlite::Connection::open(path).expect("open");
        conn.execute_batch("PRAGMA journal_mode=WAL;").expect("wal");
        conn
    }

    // ── helpers ─────────────────────────────────────────────────────────

    #[test]
    fn find_project_root_uses_env_var() {
        let dir = tempdir().expect("tempdir");
        let touring = dir.path().join(".claude").join("touring");
        std::fs::create_dir_all(&touring).expect("create dirs");
        with_project_root(dir.path(), || {
            let found = find_project_root();
            assert_eq!(found.as_deref(), Some(dir.path()));
        });
    }

    #[test]
    fn db_exists_returns_false_for_missing() {
        let dir = tempdir().expect("tempdir");
        assert!(!db_exists(&dir.path().join("nonexistent.db")));
    }

    #[test]
    fn db_exists_returns_false_for_empty_file() {
        let dir = tempdir().expect("tempdir");
        let p = dir.path().join("empty.db");
        std::fs::write(&p, b"").expect("write");
        assert!(!db_exists(&p));
    }

    #[test]
    fn db_exists_returns_true_for_nonempty_file() {
        let dir = tempdir().expect("tempdir");
        let p = dir.path().join("nonempty.db");
        std::fs::write(&p, b"SQLite format 3\x00").expect("write");
        assert!(db_exists(&p));
    }

    // ── migration primitives ─────────────────────────────────────────────

    #[test]
    fn insert_from_attached_copies_data() {
        let dir = tempdir().expect("tempdir");

        // Source DB with test data.
        let src_path = dir.path().join("src.db");
        let src = make_sqlite(&src_path);
        src.execute_batch(
            "CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT NOT NULL);
             INSERT INTO items VALUES (1, 'alpha');
             INSERT INTO items VALUES (2, 'beta');",
        )
        .expect("seed src");
        drop(src);

        // Destination DB with matching schema.
        let dst_path = dir.path().join("dst.db");
        let dst = make_sqlite(&dst_path);
        dst.execute_batch("CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT NOT NULL);")
            .expect("create dst");

        let rows = with_attached(&dst, &src_path, |conn| {
            insert_from_attached(conn, "items", "items", "IGNORE")
        })
        .expect("migrate");

        assert_eq!(rows, 2, "both rows must be copied");
        let count: i64 = dst
            .query_row("SELECT COUNT(*) FROM items", [], |r| r.get(0))
            .expect("count");
        assert_eq!(count, 2);
    }

    #[test]
    fn insert_from_attached_skips_missing_table() {
        let dir = tempdir().expect("tempdir");
        let src_path = dir.path().join("src.db");
        make_sqlite(&src_path); // empty DB — no tables
        let dst_path = dir.path().join("dst.db");
        let dst = make_sqlite(&dst_path);
        dst.execute_batch("CREATE TABLE items (id INTEGER PRIMARY KEY);")
            .expect("create dst");

        let rows = with_attached(&dst, &src_path, |conn| {
            insert_from_attached(conn, "nonexistent_table", "items", "IGNORE")
        })
        .expect("should not error");

        assert_eq!(rows, 0, "missing source table must yield 0 rows");
    }

    #[test]
    fn rebuild_fts5_succeeds_on_rlm_schema() {
        use touring_foundation::schema::memory::MEMORY_SCHEMA_V8;
        let conn = rusqlite::Connection::open_in_memory().expect("in-memory");
        conn.execute_batch(MEMORY_SCHEMA_V8).expect("apply schema");
        // rlm_fts is an external-content FTS5 over rlm_entries.
        rebuild_fts5(&conn, "rlm_fts").expect("rebuild must succeed on empty table");
    }

    #[test]
    fn rebuild_fts5_succeeds_on_symbols_schema() {
        use touring_foundation::schema::knowledge::KNOWLEDGE_SCHEMA_V8;
        let conn = rusqlite::Connection::open_in_memory().expect("in-memory");
        conn.execute_batch(KNOWLEDGE_SCHEMA_V8)
            .expect("apply schema");
        rebuild_fts5(&conn, "symbols_fts").expect("rebuild must succeed on empty table");
    }

    #[test]
    fn cmd_run_no_sources_creates_domain_dbs_with_schema_v8() {
        let dir = tempdir().expect("tempdir");
        let touring = dir.path().join(".claude").join("touring");
        std::fs::create_dir_all(&touring).expect("create dirs");

        let args: Vec<String> = vec!["touring".into(), "migrate".into(), "run".into()];
        let mut result = Ok(());
        with_project_root(dir.path(), || {
            result = cmd_run(&args);
        });
        result.expect("cmd_run must succeed with no source DBs");

        // All 3 target DBs must exist with schema_version=8.
        for domain in ["knowledge", "memory", "graph"] {
            let path = touring.join(format!("{domain}.db"));
            assert!(path.exists(), "{domain}.db must be created");
            let conn = rusqlite::Connection::open(&path).expect("open");
            let ver: String = conn
                .query_row(
                    "SELECT value FROM schema_meta WHERE key='schema_version'",
                    [],
                    |r| r.get(0),
                )
                .expect("schema_meta query");
            assert_eq!(ver, "8", "{domain}.db must have schema_version=8");
        }
    }

    #[test]
    fn cmd_rollback_renames_consolidated_dbs() {
        let dir = tempdir().expect("tempdir");
        let touring = dir.path().join(".claude").join("touring");
        std::fs::create_dir_all(&touring).expect("create dirs");

        // Create stub target DBs.
        for domain in ["knowledge", "memory", "graph"] {
            std::fs::write(touring.join(format!("{domain}.db")), b"SQLite format 3\x00")
                .expect("write stub");
        }

        let args: Vec<String> = vec!["touring".into(), "migrate".into(), "rollback".into()];
        let mut rollback_result = Ok(());
        with_project_root(dir.path(), || {
            rollback_result = cmd_rollback(&args);
        });
        rollback_result.expect("rollback must succeed");

        for domain in ["knowledge", "memory", "graph"] {
            let orig = touring.join(format!("{domain}.db"));
            let rollback = touring.join(format!("{domain}.db.rollback"));
            assert!(!orig.exists(), "{domain}.db must not exist after rollback");
            assert!(rollback.exists(), "{domain}.db.rollback must exist");
        }
    }

    // ── schema tests (carry-over) ────────────────────────────────────────

    #[test]
    fn schema_v8_knowledge_applies_cleanly() {
        use touring_foundation::schema::knowledge::KNOWLEDGE_SCHEMA_V8;
        let conn = rusqlite::Connection::open_in_memory().expect("in-memory");
        conn.execute_batch(KNOWLEDGE_SCHEMA_V8)
            .expect("apply schema");
        let domain: String = conn
            .query_row(
                "SELECT value FROM schema_meta WHERE key='domain'",
                [],
                |r| r.get(0),
            )
            .expect("query");
        assert_eq!(domain, "knowledge");
    }

    #[test]
    fn schema_v8_memory_applies_cleanly() {
        use touring_foundation::schema::memory::MEMORY_SCHEMA_V8;
        let conn = rusqlite::Connection::open_in_memory().expect("in-memory");
        conn.execute_batch(MEMORY_SCHEMA_V8).expect("apply schema");
        let domain: String = conn
            .query_row(
                "SELECT value FROM schema_meta WHERE key='domain'",
                [],
                |r| r.get(0),
            )
            .expect("query");
        assert_eq!(domain, "memory");
    }

    #[test]
    fn schema_v8_graph_applies_cleanly() {
        use touring_foundation::schema::graph::GRAPH_SCHEMA_V8;
        let conn = rusqlite::Connection::open_in_memory().expect("in-memory");
        conn.execute_batch(GRAPH_SCHEMA_V8).expect("apply schema");
        let domain: String = conn
            .query_row(
                "SELECT value FROM schema_meta WHERE key='domain'",
                [],
                |r| r.get(0),
            )
            .expect("query");
        assert_eq!(domain, "graph");
    }

    #[test]
    fn schema_v8_ensure_all_applies_all_three() {
        use touring_foundation::schema::ensure_all_schemas;
        let k = rusqlite::Connection::open_in_memory().expect("k");
        let m = rusqlite::Connection::open_in_memory().expect("m");
        let g = rusqlite::Connection::open_in_memory().expect("g");
        ensure_all_schemas(&k, &m, &g).expect("apply all");
    }

    // ── migration correctness: rlm_memory.db → memory.db::rlm_entries ───────

    /// Proves that the migration reads from "memory_entries" (not "rlm_entries")
    /// and converts INTEGER Unix timestamps → ISO TEXT in the destination.
    #[test]
    fn rlm_memory_migrated_with_correct_table_and_datetime_conversion() {
        let dir = tempdir().expect("tempdir");
        let data = dir.path().join(".claude").join("data");
        let touring = dir.path().join(".claude").join("touring");
        std::fs::create_dir_all(&data).expect("data dir");
        std::fs::create_dir_all(&touring).expect("touring dir");

        // Build legacy rlm_memory.db with "memory_entries" table and INTEGER timestamps.
        let rlm_path = data.join("rlm_memory.db");
        {
            let src = make_sqlite(&rlm_path);
            src.execute_batch(
                "CREATE TABLE memory_entries (
                     key TEXT, tier TEXT, value TEXT, entry_type TEXT,
                     created_at INTEGER, accessed_at INTEGER, access_count INTEGER,
                     embedding BLOB,
                     PRIMARY KEY (key, tier)
                 );
                 INSERT INTO memory_entries VALUES
                     ('lesson:refactor','semantic','always use ? operator',
                      'lesson', 1700000000, 1700001000, 3, NULL),
                     ('insight:patterns','semantic','CRDT merge is commutative',
                      'insight', 1700002000, 1700003000, 1, NULL);",
            )
            .expect("build legacy rlm_memory.db");
        }

        // Run migration.
        let mut result = Ok(());
        with_project_root(dir.path(), || {
            result = cmd_run(&["touring".into(), "migrate".into(), "run".into()]);
        });
        result.expect("cmd_run must succeed");

        // Verify: memory.db must have both rows in rlm_entries with TEXT timestamps.
        let mem_conn =
            rusqlite::Connection::open(touring.join("memory.db")).expect("open memory.db");
        let count: i64 = mem_conn
            .query_row("SELECT COUNT(*) FROM rlm_entries", [], |r| r.get(0))
            .expect("count");
        assert_eq!(count, 2, "both memory_entries rows must be migrated");

        // Verify timestamp conversion: INTEGER 1700000000 → "2023-11-14 22:13:20"
        let (created_at, last_accessed): (String, String) = mem_conn
            .query_row(
                "SELECT created_at, last_accessed FROM rlm_entries WHERE key = 'lesson:refactor'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .expect("fetch row");
        // Timestamps must be non-null TEXT strings (not integers, not NULL).
        assert!(
            !created_at.is_empty(),
            "created_at must be TEXT after datetime() conversion, got: {created_at:?}"
        );
        assert!(
            !last_accessed.is_empty(),
            "last_accessed must be TEXT after datetime() conversion, got: {last_accessed:?}"
        );
        // The converted value must look like an ISO datetime, not a raw integer.
        assert!(
            created_at.contains('-') || created_at.contains(':'),
            "created_at should be an ISO datetime string, not an integer: {created_at:?}"
        );

        // Verify values round-tripped correctly.
        let (key, value, entry_type, access_count): (String, String, String, i64) = mem_conn
            .query_row(
                "SELECT key, value, entry_type, access_count \
                 FROM rlm_entries WHERE key = 'lesson:refactor'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .expect("fetch values");
        assert_eq!(key, "lesson:refactor");
        assert_eq!(value, "always use ? operator");
        assert_eq!(entry_type, "lesson");
        assert_eq!(access_count, 3);
    }

    /// Proves that if rlm_memory.db has the wrong table name (not "memory_entries"),
    /// migration skips it gracefully with 0 rows — no crash, no data loss.
    #[test]
    fn rlm_memory_skipped_gracefully_when_source_table_absent() {
        let dir = tempdir().expect("tempdir");
        let data = dir.path().join(".claude").join("data");
        let touring = dir.path().join(".claude").join("touring");
        std::fs::create_dir_all(&data).expect("data dir");
        std::fs::create_dir_all(&touring).expect("touring dir");

        // DB exists but has a completely different table — simulates wrong schema.
        let rlm_path = data.join("rlm_memory.db");
        {
            let src = make_sqlite(&rlm_path);
            src.execute_batch(
                "CREATE TABLE wrong_table (id INTEGER PRIMARY KEY, data TEXT);
                 INSERT INTO wrong_table VALUES (1, 'orphan');",
            )
            .expect("build wrong-schema DB");
        }

        let mut result = Ok(());
        with_project_root(dir.path(), || {
            result = cmd_run(&["touring".into(), "migrate".into(), "run".into()]);
        });
        result.expect("cmd_run must not crash on wrong source schema");

        // memory.db must exist but rlm_entries must be empty.
        let mem_conn =
            rusqlite::Connection::open(touring.join("memory.db")).expect("open memory.db");
        let count: i64 = mem_conn
            .query_row("SELECT COUNT(*) FROM rlm_entries", [], |r| r.get(0))
            .expect("count");
        assert_eq!(count, 0, "no rows must be migrated from wrong source table");
    }

    // ── migration correctness: learning_qtable ───────────────────────────────

    /// Proves that graph.db::learning_qtable schema has (state_action, q_value, trace)
    /// — the columns that TdLearningLoopHandler and recall_rl_top_action actually use.
    #[test]
    fn graph_db_learning_qtable_schema_has_state_action_column() {
        use touring_foundation::schema::graph::GRAPH_SCHEMA_V8;
        let conn = rusqlite::Connection::open_in_memory().expect("in-memory");
        conn.execute_batch(GRAPH_SCHEMA_V8).expect("apply schema");

        // Insert using the exact SQL that TdLearningLoopHandler uses.
        conn.execute(
            "INSERT OR REPLACE INTO learning_qtable (state_action, q_value, trace) \
             VALUES (?1, ?2, 0.0)",
            rusqlite::params!["ev3:5", 0.75_f64],
        )
        .expect("INSERT with state_action, q_value, trace must succeed");

        // SELECT using the exact SQL that recall_rl_top_action uses.
        let (sa, qv): (String, f64) = conn
            .query_row(
                "SELECT state_action, q_value FROM learning_qtable \
                 ORDER BY q_value DESC LIMIT 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .expect("SELECT state_action must succeed");
        assert_eq!(sa, "ev3:5");
        assert!((qv - 0.75).abs() < 1e-9);

        // SELECT using exact SQL from TdLearningLoopHandler WHERE clause.
        let q: f64 = conn
            .query_row(
                "SELECT q_value FROM learning_qtable WHERE state_action = ?1",
                rusqlite::params!["ev3:5"],
                |r| r.get(0),
            )
            .expect("WHERE state_action lookup must succeed");
        assert!((q - 0.75).abs() < 1e-9);
    }

    /// Proves that learning_qtable from touring_pipeline.db is migrated correctly
    /// using explicit column mapping (SELECT state_action, q_value FROM source).
    #[test]
    fn learning_qtable_migrated_from_pipeline_db() {
        let dir = tempdir().expect("tempdir");
        let data = dir.path().join(".claude").join("data");
        let touring = dir.path().join(".claude").join("touring");
        std::fs::create_dir_all(&data).expect("data dir");
        std::fs::create_dir_all(&touring).expect("touring dir");

        // Build legacy touring_pipeline.db with old learning_qtable schema.
        let pipeline_path = data.join("touring_pipeline.db");
        {
            let src = make_sqlite(&pipeline_path);
            src.execute_batch(
                "CREATE TABLE learning_qtable (state_action TEXT PRIMARY KEY, q_value REAL);
                 INSERT INTO learning_qtable VALUES ('ev2:4', 0.82);
                 INSERT INTO learning_qtable VALUES ('ev1:3', 0.45);",
            )
            .expect("build legacy pipeline db");
        }

        let mut result = Ok(());
        with_project_root(dir.path(), || {
            result = cmd_run(&["touring".into(), "migrate".into(), "run".into()]);
        });
        result.expect("cmd_run must succeed");

        // Verify both rows migrated to graph.db::learning_qtable.
        let g_conn = rusqlite::Connection::open(touring.join("graph.db")).expect("open graph.db");
        let count: i64 = g_conn
            .query_row("SELECT COUNT(*) FROM learning_qtable", [], |r| r.get(0))
            .expect("count");
        assert_eq!(count, 2, "both qtable rows must be migrated");

        let qv: f64 = g_conn
            .query_row(
                "SELECT q_value FROM learning_qtable WHERE state_action = 'ev2:4'",
                [],
                |r| r.get(0),
            )
            .expect("fetch q_value");
        assert!(
            (qv - 0.82).abs() < 1e-9,
            "q_value must round-trip correctly"
        );
    }

    // ── E2E: full migration with both legacy sources ──────────────────────────

    /// Full E2E: both rlm_memory.db and touring_pipeline.db present.
    /// Proves migration is complete, non-destructive, and idempotent.
    #[test]
    fn cmd_run_e2e_rlm_entries_and_qtable_both_migrated() {
        let dir = tempdir().expect("tempdir");
        let data = dir.path().join(".claude").join("data");
        let touring = dir.path().join(".claude").join("touring");
        std::fs::create_dir_all(&data).expect("data dir");
        std::fs::create_dir_all(&touring).expect("touring dir");

        // Seed rlm_memory.db.
        {
            let src = make_sqlite(&data.join("rlm_memory.db"));
            src.execute_batch(
                "CREATE TABLE memory_entries (
                     key TEXT, tier TEXT, value TEXT, entry_type TEXT,
                     created_at INTEGER, accessed_at INTEGER, access_count INTEGER,
                     embedding BLOB, PRIMARY KEY (key, tier)
                 );
                 INSERT INTO memory_entries VALUES
                     ('lesson:a','semantic','value a','lesson',1700000000,1700001000,1,NULL);",
            )
            .expect("seed rlm_memory");
        }
        // Seed touring_pipeline.db.
        {
            let src = make_sqlite(&data.join("touring_pipeline.db"));
            src.execute_batch(
                "CREATE TABLE learning_qtable (state_action TEXT PRIMARY KEY, q_value REAL);
                 INSERT INTO learning_qtable VALUES ('ev1:2', 0.60);",
            )
            .expect("seed pipeline");
        }

        // First run + idempotency check — both inside the same env guard.
        let mut result = Ok(());
        with_project_root(dir.path(), || {
            result = cmd_run(&["touring".into(), "migrate".into(), "run".into()]).and_then(|_| {
                // Idempotency: second run must not error or duplicate rows.
                cmd_run(&["touring".into(), "migrate".into(), "run".into()])
            });
        });
        result.expect("both runs (first + idempotent) must succeed");

        // Assert memory.db has exactly 1 rlm_entries row (not 2 after idempotent run).
        let mem_conn =
            rusqlite::Connection::open(touring.join("memory.db")).expect("open memory.db");
        let rlm_count: i64 = mem_conn
            .query_row("SELECT COUNT(*) FROM rlm_entries", [], |r| r.get(0))
            .expect("count rlm");
        assert_eq!(
            rlm_count, 1,
            "idempotent run must not duplicate rlm_entries"
        );

        // Assert graph.db has exactly 1 learning_qtable row.
        let g_conn = rusqlite::Connection::open(touring.join("graph.db")).expect("open graph.db");
        let qt_count: i64 = g_conn
            .query_row("SELECT COUNT(*) FROM learning_qtable", [], |r| r.get(0))
            .expect("count qtable");
        assert_eq!(
            qt_count, 1,
            "idempotent run must not duplicate learning_qtable rows"
        );

        // All 3 domain DBs must exist.
        for domain in ["knowledge", "memory", "graph"] {
            assert!(
                touring.join(format!("{domain}.db")).exists(),
                "{domain}.db must exist after migration"
            );
        }
    }

    // ── Fase 2.3: cleanup tests ──────────────────────────────────────────────

    #[test]
    fn cmd_cleanup_archives_legacy_dbs() {
        let dir = tempdir().expect("tempdir");
        let data = dir.path().join(".claude").join("data");
        let touring = dir.path().join(".claude").join("touring");
        std::fs::create_dir_all(&data).expect("data dir");
        std::fs::create_dir_all(&touring).expect("touring dir");

        // Seed a legacy source DB (rlm_memory.db).
        let rlm_path = data.join("rlm_memory.db");
        {
            let src = make_sqlite(&rlm_path);
            src.execute_batch(
                "CREATE TABLE memory_entries (
                     key TEXT, tier TEXT, value TEXT, entry_type TEXT,
                     created_at INTEGER, accessed_at INTEGER, access_count INTEGER,
                     embedding BLOB, PRIMARY KEY (key, tier)
                 );
                 INSERT INTO memory_entries VALUES ('k1','semantic','v1','lesson',0,0,1,NULL);",
            )
            .expect("seed rlm");
        }

        // Run migration to create the consolidated target DBs.
        with_project_root(dir.path(), || {
            cmd_run(&["touring".into(), "migrate".into(), "run".into()]).expect("migrate run");
        });

        // Run cleanup — must succeed after valid migration.
        with_project_root(dir.path(), || {
            cmd_cleanup(&["touring".into(), "migrate".into(), "cleanup".into()]).expect("cleanup");
        });

        // Legacy DB must be renamed to .db.migrated.
        assert!(
            !rlm_path.exists(),
            "rlm_memory.db must no longer exist after cleanup"
        );
        assert!(
            data.join("rlm_memory.db.migrated").exists(),
            "rlm_memory.db.migrated must exist after cleanup"
        );

        // Consolidated target DBs must still exist.
        for domain in ["knowledge", "memory", "graph"] {
            assert!(
                touring.join(format!("{domain}.db")).exists(),
                "{domain}.db must still exist after cleanup"
            );
        }
    }

    #[test]
    fn cmd_cleanup_is_idempotent() {
        // Second cleanup run must not error when DBs are already archived.
        let dir = tempdir().expect("tempdir");
        let data = dir.path().join(".claude").join("data");
        let touring = dir.path().join(".claude").join("touring");
        std::fs::create_dir_all(&data).expect("data dir");
        std::fs::create_dir_all(&touring).expect("touring dir");

        // Run migration with no sources (creates empty target DBs).
        with_project_root(dir.path(), || {
            cmd_run(&["touring".into(), "migrate".into(), "run".into()]).expect("migrate run");
            // First cleanup.
            cmd_cleanup(&["touring".into(), "migrate".into(), "cleanup".into()])
                .expect("first cleanup");
            // Second cleanup must also succeed.
            cmd_cleanup(&["touring".into(), "migrate".into(), "cleanup".into()])
                .expect("idempotent second cleanup");
        });
    }
}
