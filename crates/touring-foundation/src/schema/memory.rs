//! DDL for memory.db (SCHEMA_VERSION=8).
//!
//! Consolidates: rlm_memory.db, touring_rlm.db, semantic_recall.db, ann_memory.db.
//! Rule: LOCAL-FIRST with global fallback — RL episodic memory uses global path.
//! IMPORTANT: ann_embeddings uses raw f32 le_bytes format (NOT bincode).

/// Schema DDL for memory.db.
pub const MEMORY_SCHEMA_V8: &str = r#"
PRAGMA user_version = 8;
PRAGMA journal_mode=WAL;
PRAGMA foreign_keys=ON;

-- === RLM Episodic Memory (from rlm_memory.db + touring_rlm.db) ===
CREATE TABLE IF NOT EXISTS rlm_entries (
    id INTEGER PRIMARY KEY,
    session_id TEXT,
    key TEXT NOT NULL,
    value TEXT,
    tier TEXT DEFAULT 'semantic',
    entry_type TEXT DEFAULT 'lesson',
    access_count INTEGER DEFAULT 0,
    last_accessed TEXT,
    created_at TEXT,
    updated_at TEXT,
    UNIQUE(key)
);
CREATE INDEX IF NOT EXISTS idx_rlm_key ON rlm_entries(key);
CREATE INDEX IF NOT EXISTS idx_rlm_type ON rlm_entries(entry_type);
CREATE INDEX IF NOT EXISTS idx_rlm_session ON rlm_entries(session_id);

CREATE VIRTUAL TABLE IF NOT EXISTS rlm_fts
    USING fts5(key, value, content='rlm_entries', content_rowid='id');

-- === Semantic Recall (from semantic_recall.db) ===
CREATE TABLE IF NOT EXISTS recall_embeddings (
    id INTEGER PRIMARY KEY,
    chunk_id TEXT NOT NULL UNIQUE,
    content TEXT NOT NULL,
    -- Raw f32 le_bytes: 384 dims = 1536 bytes
    embedding BLOB NOT NULL,
    -- u4 quantized: 384 dims = 192 bytes (when quantization feature active)
    -- BLOB format: scale(4) + zero(4) + dims(4) + nibbles(ceil(dims/2))
    embedding_u4 BLOB,
    -- Separate scale/zero for fast SQL filtering without BLOB deserialization
    quant_scale REAL,
    quant_zero REAL,
    metadata_json TEXT,
    access_count INTEGER DEFAULT 0,
    created_at TEXT,
    updated_at TEXT
);
CREATE INDEX IF NOT EXISTS idx_recall_chunk ON recall_embeddings(chunk_id);

-- === ANN Memory (from ann_memory.db) ===
-- Note: renamed from 'embeddings'/'meta' to avoid collision with recall_embeddings
CREATE TABLE IF NOT EXISTS ann_embeddings (
    rowid INTEGER PRIMARY KEY,
    id TEXT NOT NULL UNIQUE,
    content TEXT NOT NULL,
    -- Raw f32 le_bytes format (standardized from bincode in Phase 3.1)
    embedding BLOB NOT NULL,
    metadata_json TEXT
);
CREATE INDEX IF NOT EXISTS idx_ann_id ON ann_embeddings(id);

CREATE TABLE IF NOT EXISTS ann_meta (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

-- Migration metadata
CREATE TABLE IF NOT EXISTS schema_meta (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at TEXT DEFAULT (datetime('now'))
);
INSERT OR IGNORE INTO schema_meta(key, value) VALUES('schema_version', '8');
INSERT OR IGNORE INTO schema_meta(key, value) VALUES('domain', 'memory');

CREATE TABLE IF NOT EXISTS _migration_state (
    phase        TEXT PRIMARY KEY,
    status       TEXT NOT NULL DEFAULT 'pending',
    rows_migrated INTEGER NOT NULL DEFAULT 0,
    started_at   TEXT,
    completed_at TEXT,
    error_message TEXT
);
"#;
