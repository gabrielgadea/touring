//! DDL for knowledge.db (SCHEMA_VERSION=8).
//!
//! Consolidates: symbols.db, touring_knowledge.db, wiring tables from touring_pipeline.db.
//! Rule: ALWAYS LOCAL — knowledge is per-project, never shared globally.
//!
//! IMPORTANT: This DDL must match the operational schema in touring-hooks/src/knowledge.rs
//! exactly. The operational schema (ensure_schema + migrate_schema) is the ground truth.
//! Any divergence will cause query failures on fresh databases.

/// Schema DDL for knowledge.db.
///
/// This schema is the canonical DDL used when creating new project databases.
/// It must be kept in sync with the operational schema in `touring-hooks::knowledge`.
pub const KNOWLEDGE_SCHEMA_V8: &str = r#"
PRAGMA journal_mode=WAL;
PRAGMA foreign_keys=ON;

-- === Symbols AST (from symbols.db + touring_pipeline.db) ===
CREATE TABLE IF NOT EXISTS symbols (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    file_path TEXT NOT NULL,
    line INTEGER NOT NULL,
    column_offset INTEGER NOT NULL DEFAULT 0,
    is_definition INTEGER NOT NULL DEFAULT 1,
    updated_at TEXT,
    access_count INTEGER NOT NULL DEFAULT 0,
    last_accessed TEXT,
    UNIQUE(name, file_path, line)
);
CREATE INDEX IF NOT EXISTS idx_symbols_name ON symbols(name);
CREATE INDEX IF NOT EXISTS idx_symbols_file ON symbols(file_path);

CREATE TABLE IF NOT EXISTS dependencies (
    id INTEGER PRIMARY KEY,
    from_file TEXT NOT NULL,
    to_file TEXT NOT NULL,
    symbols_json TEXT,
    co_edit_weight REAL DEFAULT 0.0,
    updated_at TEXT,
    UNIQUE(from_file, to_file)
);
CREATE INDEX IF NOT EXISTS idx_deps_from ON dependencies(from_file);
CREATE INDEX IF NOT EXISTS idx_deps_to ON dependencies(to_file);

-- FTS5 for fast symbol search (must be rebuilt after data migration)
CREATE VIRTUAL TABLE IF NOT EXISTS symbols_fts
    USING fts5(name, file_path, content='symbols', content_rowid='id');

-- === File Knowledge (from touring_knowledge.db) ===
CREATE TABLE IF NOT EXISTS file_knowledge (
    file_path TEXT PRIMARY KEY,
    language TEXT,
    line_count INTEGER DEFAULT 0,
    symbol_count INTEGER DEFAULT 0,
    read_count INTEGER DEFAULT 0,
    last_read_at TEXT,
    content_hash TEXT,
    imports_json TEXT DEFAULT '[]',
    symbols_json TEXT DEFAULT '[]',
    notes TEXT,
    created_at TEXT DEFAULT (datetime('now')),
    updated_at TEXT DEFAULT (datetime('now')),
    access_count INTEGER DEFAULT 0,
    last_accessed TEXT
);

CREATE TABLE IF NOT EXISTS file_relations (
    source_path TEXT NOT NULL,
    target_path TEXT NOT NULL,
    relation_type TEXT NOT NULL DEFAULT 'imports',
    imported_symbols TEXT DEFAULT '[]',
    created_at TEXT DEFAULT (datetime('now')),
    PRIMARY KEY (source_path, target_path, relation_type)
);
CREATE INDEX IF NOT EXISTS idx_file_relations_source ON file_relations(source_path);
CREATE INDEX IF NOT EXISTS idx_file_relations_target ON file_relations(target_path);

CREATE TABLE IF NOT EXISTS file_access_log (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    file_path TEXT NOT NULL,
    session_id TEXT,
    accessed_at TEXT DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_file_access_path ON file_access_log(file_path);

CREATE TABLE IF NOT EXISTS bash_outcomes (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    command TEXT NOT NULL,
    command_short TEXT NOT NULL,
    exit_code INTEGER DEFAULT 0,
    success INTEGER DEFAULT 1,
    error_pattern TEXT,
    file_context TEXT,
    executed_at TEXT DEFAULT (datetime('now')),
    command_hash TEXT
);
CREATE INDEX IF NOT EXISTS idx_bash_outcomes_short ON bash_outcomes(command_short);
CREATE INDEX IF NOT EXISTS idx_bash_outcomes_hash ON bash_outcomes(command_hash);

CREATE TABLE IF NOT EXISTS edit_history (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    file_path TEXT NOT NULL,
    edit_type TEXT NOT NULL DEFAULT 'edit',
    summary TEXT,
    error_pattern TEXT,
    language TEXT,
    symbol_context TEXT,
    session_id TEXT,
    edited_at TEXT DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_edit_history_path ON edit_history(file_path);
CREATE INDEX IF NOT EXISTS idx_edit_error_ctx ON edit_history(error_pattern, language);
CREATE INDEX IF NOT EXISTS idx_edit_session ON edit_history(session_id);

CREATE TABLE IF NOT EXISTS gotchas (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    pattern TEXT NOT NULL,
    gotcha TEXT NOT NULL,
    severity TEXT NOT NULL DEFAULT 'warning',
    symbol_name TEXT,
    language TEXT,
    hit_count INTEGER NOT NULL DEFAULT 0,
    prevented_errors INTEGER NOT NULL DEFAULT 0,
    decay_score REAL NOT NULL DEFAULT 1.0,
    last_occurrence TEXT,
    resolved_at TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_gotchas_pattern ON gotchas(pattern);
CREATE UNIQUE INDEX IF NOT EXISTS idx_gotchas_pattern_language
    ON gotchas(pattern, COALESCE(language, ''));
CREATE INDEX IF NOT EXISTS idx_gotchas_decay
    ON gotchas(decay_score DESC) WHERE resolved_at IS NULL;

CREATE TABLE IF NOT EXISTS file_coedits (
    source_path TEXT NOT NULL,
    target_path TEXT NOT NULL,
    coedit_count INTEGER NOT NULL DEFAULT 0,
    last_coedit_at TEXT DEFAULT (datetime('now')),
    PRIMARY KEY (source_path, target_path)
);
CREATE INDEX IF NOT EXISTS idx_coedits_source ON file_coedits(source_path);
CREATE INDEX IF NOT EXISTS idx_coedits_target ON file_coedits(target_path);

CREATE TABLE IF NOT EXISTS file_risk_scores (
    file_path TEXT PRIMARY KEY,
    risk_score REAL DEFAULT 0.0,
    updated_at TEXT DEFAULT (datetime('now'))
);

-- === Wiring Intelligence (from touring_pipeline.db wiring tables) ===
CREATE TABLE IF NOT EXISTS wiring_map (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    module_file TEXT NOT NULL,
    symbol_name TEXT NOT NULL,
    symbol_kind TEXT NOT NULL DEFAULT 'unknown',
    visibility TEXT NOT NULL DEFAULT 'public',
    consumer_file TEXT,
    import_line INTEGER,
    contract_source TEXT DEFAULT 'ast_read',
    resolved_at TEXT,
    -- PLT-2026-06-02: workspace_root bounds each edge to the workspace that
    -- produced it. NULL = legacy row (pre-migration, kept for back-compat).
    -- Filters in `find_all_cycles` and friends treat NULL as "matches the
    -- current workspace OR no filter" — so old rows don't disappear.
    workspace_root TEXT
);
-- PLT-2026-06-02: unique constraint must now include workspace_root so the
-- same `(module, symbol, consumer)` tuple can coexist across multiple
-- workspaces (e.g., a vendored crate + a fork).
CREATE UNIQUE INDEX IF NOT EXISTS idx_wiring_unique
    ON wiring_map(module_file, symbol_name, COALESCE(consumer_file, ''), COALESCE(workspace_root, ''));
CREATE INDEX IF NOT EXISTS idx_wiring_orphans
    ON wiring_map(consumer_file) WHERE consumer_file IS NULL;
CREATE INDEX IF NOT EXISTS idx_wiring_module
    ON wiring_map(module_file);

-- S1 (2026-08-07): imports the resolver could NOT map to a producer file.
-- A separate table, not a synthetic wiring_map row: an unresolved import is
-- the ABSENCE of an edge, and storing it beside real edges risks it being read
-- as a consumer — which would erase true orphans and invert the metric it
-- exists to explain. Separation makes that contamination structurally
-- impossible rather than merely unlikely.
CREATE TABLE IF NOT EXISTS wiring_unresolved (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    module_path TEXT NOT NULL,
    symbol_name TEXT NOT NULL,
    consumer_file TEXT NOT NULL,
    import_line INTEGER,
    language TEXT NOT NULL DEFAULT 'rust',
    class TEXT NOT NULL DEFAULT 'workspace_unresolved',
    observed_at TEXT DEFAULT (datetime('now'))
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_wiring_unresolved_unique
    ON wiring_unresolved(module_path, symbol_name, consumer_file);
CREATE INDEX IF NOT EXISTS idx_wiring_unresolved_consumer
    ON wiring_unresolved(consumer_file);
-- PLT-2026-06-02: dedicated index for the `WHERE workspace_root = ?` filter
-- used by every cycle-detection / orphan / chain query post-migration.
CREATE INDEX IF NOT EXISTS idx_wiring_workspace_root
    ON wiring_map(workspace_root) WHERE workspace_root IS NOT NULL;

CREATE TABLE IF NOT EXISTS module_ecosystem (
    file_path TEXT PRIMARY KEY,
    module_role TEXT NOT NULL DEFAULT 'internal',
    parent_module TEXT,
    pub_symbol_count INTEGER DEFAULT 0,
    import_count INTEGER DEFAULT 0,
    re_export_count INTEGER DEFAULT 0,
    integration_score REAL DEFAULT 0.0,
    last_scanned_at TEXT
);
CREATE INDEX IF NOT EXISTS idx_ecosystem_score
    ON module_ecosystem(integration_score);

-- === Functional Wiring (v7) ===
CREATE TABLE IF NOT EXISTS functional_signatures (
    file_path TEXT PRIMARY KEY,
    module_purpose TEXT,
    domain TEXT,
    symbols_json TEXT DEFAULT '[]',
    content_hash TEXT,
    updated_at TEXT DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_functional_sig_domain
    ON functional_signatures(domain);

CREATE TABLE IF NOT EXISTS functional_chains (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    source_module TEXT NOT NULL,
    source_symbol TEXT NOT NULL,
    source_output TEXT,
    sink_module TEXT NOT NULL,
    sink_symbol TEXT NOT NULL,
    sink_input TEXT,
    chain_type TEXT NOT NULL DEFAULT 'Sequential',
    confidence REAL DEFAULT 0.0,
    created_at TEXT DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_functional_chains_source
    ON functional_chains(source_module);
CREATE INDEX IF NOT EXISTS idx_functional_chains_sink
    ON functional_chains(sink_module);

-- Migration metadata
CREATE TABLE IF NOT EXISTS schema_meta (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at TEXT DEFAULT (datetime('now'))
);
INSERT OR IGNORE INTO schema_meta(key, value) VALUES('schema_version', '8');
INSERT OR IGNORE INTO schema_meta(key, value) VALUES('domain', 'knowledge');

-- Per-phase migration progress (resumable migrations)
CREATE TABLE IF NOT EXISTS _migration_state (
    phase        TEXT PRIMARY KEY,
    status       TEXT NOT NULL DEFAULT 'pending',  -- pending | in_progress | completed | failed
    rows_migrated INTEGER NOT NULL DEFAULT 0,
    started_at   TEXT,
    completed_at TEXT,
    error_message TEXT
);
"#;
