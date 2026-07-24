//! E2E audit tests for DB consolidation migration (8 → 3 domains).
//!
//! These tests verify that data ACTUALLY flows from legacy DBs to consolidated
//! DBs, catching bugs where table names diverge between source and destination.
//!
//! Each test creates real SQLite databases with sample data, runs migration,
//! and verifies row counts in the destination.

use rusqlite::Connection;
use std::path::Path;
use tempfile::TempDir;
use touring_foundation::migration::consolidation::ConsolidationMigration;

// ── Helpers ──────────────────────────────────────────────────────────────────

fn make_legacy_knowledge_db(path: &Path) {
    let conn = Connection::open(path).expect("open");
    // Schema matches the REAL legacy `FileKnowledgeDB::ensure_schema()` in knowledge.rs
    conn.execute_batch(
        "PRAGMA journal_mode=WAL;
         CREATE TABLE IF NOT EXISTS file_knowledge (
             file_path TEXT PRIMARY KEY, language TEXT,
             line_count INTEGER DEFAULT 0, symbol_count INTEGER DEFAULT 0,
             read_count INTEGER DEFAULT 0, last_read_at TEXT,
             content_hash TEXT, imports_json TEXT DEFAULT '[]',
             symbols_json TEXT DEFAULT '[]', notes TEXT,
             created_at TEXT DEFAULT (datetime('now')),
             updated_at TEXT DEFAULT (datetime('now'))
         );
         CREATE TABLE IF NOT EXISTS file_relations (
             source_path TEXT NOT NULL, target_path TEXT NOT NULL,
             relation_type TEXT NOT NULL DEFAULT 'imports', updated_at TEXT,
             PRIMARY KEY(source_path, target_path, relation_type)
         );
         CREATE TABLE IF NOT EXISTS file_access_log (
             id INTEGER PRIMARY KEY AUTOINCREMENT,
             file_path TEXT, session_id TEXT, accessed_at TEXT, tool_name TEXT
         );
         CREATE TABLE IF NOT EXISTS bash_outcomes (
             id INTEGER PRIMARY KEY AUTOINCREMENT,
             command TEXT NOT NULL, command_short TEXT NOT NULL,
             exit_code INTEGER DEFAULT 0, success INTEGER DEFAULT 1,
             error_pattern TEXT, file_context TEXT,
             executed_at TEXT DEFAULT (datetime('now'))
         );
         CREATE TABLE IF NOT EXISTS edit_history (
             id INTEGER PRIMARY KEY AUTOINCREMENT,
             file_path TEXT NOT NULL,
             edit_type TEXT NOT NULL DEFAULT 'edit',
             summary TEXT,
             error_pattern TEXT,
             edited_at TEXT DEFAULT (datetime('now'))
         );
         CREATE TABLE IF NOT EXISTS gotchas (
             id INTEGER PRIMARY KEY AUTOINCREMENT,
             pattern TEXT NOT NULL,
             gotcha TEXT NOT NULL,
             severity TEXT NOT NULL DEFAULT 'warning',
             symbol_name TEXT,
             hit_count INTEGER NOT NULL DEFAULT 0,
             prevented_errors INTEGER NOT NULL DEFAULT 0,
             created_at TEXT NOT NULL DEFAULT (datetime('now'))
         );
         CREATE TABLE IF NOT EXISTS file_risk_scores (
             file_path TEXT PRIMARY KEY, risk_score REAL NOT NULL DEFAULT 0.0, updated_at TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS wiring_map (
             id INTEGER PRIMARY KEY AUTOINCREMENT,
             module_file TEXT NOT NULL, symbol_name TEXT NOT NULL,
             symbol_kind TEXT DEFAULT 'unknown', visibility TEXT DEFAULT 'public',
             consumer_file TEXT, import_line INTEGER,
             contract_source TEXT DEFAULT 'ast_read', resolved_at TEXT
         );
         CREATE TABLE IF NOT EXISTS module_ecosystem (
             file_path TEXT PRIMARY KEY,
             module_role TEXT DEFAULT 'internal', parent_module TEXT,
             pub_symbol_count INTEGER DEFAULT 0, import_count INTEGER DEFAULT 0,
             re_export_count INTEGER DEFAULT 0, integration_score REAL DEFAULT 0.0,
             last_scanned_at TEXT
         );
        ",
    )
    .expect("create knowledge tables");

    // Insert sample data
    conn.execute(
        "INSERT INTO file_knowledge (file_path, language, line_count) VALUES ('src/main.rs', 'rust', 100)",
        [],
    ).expect("insert file_knowledge");
    conn.execute(
        "INSERT INTO edit_history (file_path, edit_type) VALUES ('src/main.rs', 'modify')",
        [],
    )
    .expect("insert edit_history");
    conn.execute(
        "INSERT INTO gotchas (pattern, gotcha, severity) VALUES ('unwrap()', 'Avoid unwrap in prod', 'high')",
        [],
    ).expect("insert gotchas");
    conn.execute(
        "INSERT INTO bash_outcomes (command, command_short, exit_code, success) VALUES ('cargo test', 'cargo test', 0, 1)",
        [],
    ).expect("insert bash_outcomes");
    conn.execute(
        "INSERT INTO file_relations (source_path, target_path) VALUES ('src/main.rs', 'src/lib.rs')",
        [],
    ).expect("insert file_relations");
    conn.execute(
        "INSERT INTO file_access_log (file_path, tool_name) VALUES ('src/main.rs', 'Read')",
        [],
    )
    .expect("insert file_access_log");
}

fn make_legacy_got_snapshots_db(path: &Path) {
    let conn = Connection::open(path).expect("open");
    conn.execute_batch(
        "PRAGMA journal_mode=WAL;
         CREATE TABLE IF NOT EXISTS got_snapshots (
             id INTEGER PRIMARY KEY,
             session_id TEXT NOT NULL UNIQUE,
             snapshot_data BLOB NOT NULL,
             node_count INTEGER DEFAULT 0,
             created_at TEXT,
             updated_at TEXT
         );",
    )
    .expect("create got table");
    conn.execute(
        "INSERT INTO got_snapshots (session_id, snapshot_data, node_count) VALUES ('sess-1', X'DEADBEEF', 5)",
        [],
    ).expect("insert got_snapshots");
}

fn make_legacy_pipeline_db(path: &Path) {
    let conn = Connection::open(path).expect("open");
    // Schema matches the REAL legacy LearningPersistence in persistence.rs
    conn.execute_batch(
        "PRAGMA journal_mode=WAL;
         CREATE TABLE IF NOT EXISTS learning_wilson (
             item_id TEXT PRIMARY KEY,
             successes INTEGER NOT NULL,
             trials INTEGER NOT NULL
         );
         CREATE TABLE IF NOT EXISTS learning_qtable (
             state_action TEXT PRIMARY KEY,
             q_value REAL NOT NULL,
             trace REAL NOT NULL DEFAULT 0.0
         );
         CREATE TABLE IF NOT EXISTS learning_linucb (
             arm_id TEXT PRIMARY KEY,
             pulls INTEGER DEFAULT 0,
             cumulative_reward REAL DEFAULT 0.0,
             a_inv_flat BLOB,
             b_vec BLOB,
             total_pulls INTEGER DEFAULT 0
         );
         CREATE TABLE IF NOT EXISTS learning_drift (
             metric TEXT PRIMARY KEY,
             values_json TEXT NOT NULL
         );",
    )
    .expect("create pipeline tables");

    conn.execute(
        "INSERT INTO learning_wilson (item_id, successes, trials) VALUES ('edit', 10, 15)",
        [],
    )
    .expect("insert wilson");
    conn.execute(
        "INSERT INTO learning_qtable (state_action, q_value) VALUES ('evREAD:edit', 0.75)",
        [],
    )
    .expect("insert qtable");
    conn.execute(
        "INSERT INTO learning_linucb (arm_id, pulls) VALUES ('arm-edit', 42)",
        [],
    )
    .expect("insert linucb");
}

fn row_count(conn: &Connection, table: &str) -> i64 {
    conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
        .unwrap_or(-1)
}

// ── E2E Audit Tests ──────────────────────────────────────────────────────────

/// BUG: migrate_data() uses source table name "edit_history" but destination
/// schema declares "file_edit_history". The INSERT should fail or silently
/// lose data.
#[test]
fn audit_migrate_data_edit_history_table_name_mismatch() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    std::fs::create_dir_all(root.join(".claude/data")).unwrap();
    std::fs::create_dir_all(root.join(".claude/touring")).unwrap();

    // Create legacy touring_knowledge.db with edit_history table
    make_legacy_knowledge_db(&root.join(".claude/data/touring_knowledge.db"));

    // Create consolidated knowledge.db with v8 schema
    let mig = ConsolidationMigration::new(root);
    mig.create_target_dbs().unwrap();

    // Run migrate_data
    let _result = mig.migrate_data(|_, _, _| {});

    // Check: did edit_history data arrive in edit_history?
    let k = Connection::open(root.join(".claude/touring/knowledge.db")).unwrap();
    let edit_count = row_count(&k, "edit_history");

    // After fix: migrate_data() uses correct table name "edit_history" (same as v8 schema)
    // The row should now migrate successfully.
    assert_eq!(
        edit_count, 1,
        "edit_history row should migrate to edit_history table"
    );
}

/// BUG: migrate_data() uses source table name "gotchas" but destination
/// schema declares "file_gotchas".
#[test]
fn audit_migrate_data_gotchas_table_name_mismatch() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    std::fs::create_dir_all(root.join(".claude/data")).unwrap();
    std::fs::create_dir_all(root.join(".claude/touring")).unwrap();

    make_legacy_knowledge_db(&root.join(".claude/data/touring_knowledge.db"));

    let mig = ConsolidationMigration::new(root);
    mig.create_target_dbs().unwrap();
    let _ = mig.migrate_data(|_, _, _| {});

    let k = Connection::open(root.join(".claude/touring/knowledge.db")).unwrap();
    let gotcha_count = row_count(&k, "gotchas");

    assert_eq!(
        gotcha_count, 1,
        "gotchas row should migrate to gotchas table"
    );
}

/// BUG: migrate_data() looks for source table "got_sessions" but the actual
/// table in got_snapshots.db is "got_snapshots".
#[test]
fn audit_migrate_data_got_snapshots_table_name_mismatch() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    std::fs::create_dir_all(root.join(".claude/data")).unwrap();
    std::fs::create_dir_all(root.join(".claude/touring")).unwrap();

    make_legacy_got_snapshots_db(&root.join(".claude/data/got_snapshots.db"));

    let mig = ConsolidationMigration::new(root);
    mig.create_target_dbs().unwrap();
    let _stats = mig.migrate_data(|_, _, _| {}).unwrap();

    let g = Connection::open(root.join(".claude/touring/graph.db")).unwrap();
    let got_count = row_count(&g, "got_snapshots");

    // migrate_data() searches for "got_sessions" in source, finds nothing, returns 0
    assert_eq!(
        got_count, 1,
        "BUG: got_snapshots row should migrate but was lost (migrate_data looks for 'got_sessions')"
    );
}

/// BUG: learning_linucb table is missing from GRAPH_SCHEMA_V8 but
/// migrate_data() tries to INSERT into it.
#[test]
fn audit_graph_schema_missing_learning_linucb() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    std::fs::create_dir_all(root.join(".claude/data")).unwrap();
    std::fs::create_dir_all(root.join(".claude/touring")).unwrap();

    make_legacy_pipeline_db(&root.join(".claude/data/touring_pipeline.db"));

    let mig = ConsolidationMigration::new(root);
    mig.create_target_dbs().unwrap();

    let _result = mig.migrate_data(|_, _, _| {});

    let g = Connection::open(root.join(".claude/touring/graph.db")).unwrap();

    // Check if learning_linucb table exists in graph.db
    let table_exists: bool = g
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='learning_linucb'",
            [],
            |r| r.get::<_, i64>(0),
        )
        .map(|c| c > 0)
        .unwrap_or(false);

    if !table_exists {
        eprintln!("AUDIT BUG CONFIRMED: learning_linucb missing from GRAPH_SCHEMA_V8");
    }

    assert!(
        table_exists,
        "BUG: learning_linucb table must exist in graph.db schema"
    );

    // If table exists, verify data migrated
    if table_exists {
        let linucb_count = row_count(&g, "learning_linucb");
        assert_eq!(linucb_count, 1, "learning_linucb data should migrate");
    }
}

/// Verify that data that IS correctly named actually migrates properly.
/// This tests the HAPPY PATH to confirm infrastructure works.
#[test]
fn audit_migrate_data_happy_path_tables() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    std::fs::create_dir_all(root.join(".claude/data")).unwrap();
    std::fs::create_dir_all(root.join(".claude/touring")).unwrap();

    make_legacy_knowledge_db(&root.join(".claude/data/touring_knowledge.db"));
    make_legacy_pipeline_db(&root.join(".claude/data/touring_pipeline.db"));
    make_legacy_got_snapshots_db(&root.join(".claude/data/got_snapshots.db"));

    let mig = ConsolidationMigration::new(root);
    mig.create_target_dbs().unwrap();
    let result = mig.migrate_data(|phase, src, rows| {
        eprintln!("  migrate: {phase} from {src} = {rows} rows");
    });

    if let Err(ref e) = result {
        eprintln!("AUDIT: migrate_data() error: {e}");
    }

    // These tables have matching names between source and schema:
    let k = Connection::open(root.join(".claude/touring/knowledge.db")).unwrap();
    assert_eq!(
        row_count(&k, "file_knowledge"),
        1,
        "file_knowledge should migrate"
    );
    assert_eq!(
        row_count(&k, "bash_outcomes"),
        1,
        "bash_outcomes should migrate"
    );

    let g = Connection::open(root.join(".claude/touring/graph.db")).unwrap();
    // learning_wilson migrates via explicit column mapping (item_id → tool_name)
    let wilson_count = row_count(&g, "learning_wilson");
    assert_eq!(
        wilson_count, 1,
        "learning_wilson should migrate (item_id→tool_name mapping)"
    );

    let qtable_count = row_count(&g, "learning_qtable");
    assert_eq!(
        qtable_count, 1,
        "learning_qtable should migrate (same schema)"
    );

    let linucb_count = row_count(&g, "learning_linucb");
    assert_eq!(
        linucb_count, 1,
        "learning_linucb should migrate (same schema)"
    );
}

/// Verify that FTS5 is rebuilt correctly after migration.
#[test]
fn audit_fts5_integrity_after_migration() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    std::fs::create_dir_all(root.join(".claude/touring")).unwrap();

    // Create symbols.db with FTS5 source
    let symbols_path = root.join(".claude/touring/symbols.db");
    let src = Connection::open(&symbols_path).unwrap();
    src.execute_batch(
        "CREATE TABLE IF NOT EXISTS symbols (
             id INTEGER PRIMARY KEY, name TEXT NOT NULL,
             file_path TEXT NOT NULL, line INTEGER NOT NULL,
             column_offset INTEGER NOT NULL DEFAULT 0,
             is_definition INTEGER NOT NULL DEFAULT 1,
             updated_at TEXT, access_count INTEGER NOT NULL DEFAULT 0,
             last_accessed TEXT, UNIQUE(name, file_path, line)
         );
         INSERT INTO symbols (name, file_path, line) VALUES ('HookRuntime', 'src/hook_runtime.rs', 42);
         INSERT INTO symbols (name, file_path, line) VALUES ('SymbolStore', 'src/ast/store.rs', 10);",
    )
    .unwrap();
    drop(src);

    // Create consolidated knowledge.db and migrate
    let mig = ConsolidationMigration::new(root);
    mig.create_target_dbs().unwrap();
    let _ = mig.migrate_data(|_, _, _| {});

    // Verify FTS5 works
    let k = Connection::open(root.join(".claude/touring/knowledge.db")).unwrap();
    let symbols_count = row_count(&k, "symbols");
    assert_eq!(symbols_count, 2, "2 symbols should migrate from symbols.db");

    // FTS5 query should work
    let fts_result: i64 = k
        .query_row(
            "SELECT COUNT(*) FROM symbols_fts WHERE symbols_fts MATCH 'HookRuntime'",
            [],
            |r| r.get(0),
        )
        .unwrap_or(-1);

    assert_eq!(
        fts_result, 1,
        "FTS5 search for 'HookRuntime' should return 1 result after rebuild"
    );
}
