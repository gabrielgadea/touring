//! DDL for graph.db (SCHEMA_VERSION=8).
//!
//! Consolidates: got_snapshots.db, touring_pipeline.db (learning tables), hook_events.
//! Rule: ALWAYS LOCAL — sessions and RL pipeline are per-project.

/// Schema DDL for graph.db.
pub const GRAPH_SCHEMA_V8: &str = r#"
PRAGMA user_version = 8;
PRAGMA journal_mode=WAL;
PRAGMA foreign_keys=ON;

-- === GoT Snapshots (from got_snapshots.db) ===
CREATE TABLE IF NOT EXISTS got_snapshots (
    id INTEGER PRIMARY KEY,
    session_id TEXT NOT NULL UNIQUE,
    snapshot_data BLOB NOT NULL,
    node_count INTEGER DEFAULT 0,
    created_at TEXT,
    updated_at TEXT
);
CREATE INDEX IF NOT EXISTS idx_got_session ON got_snapshots(session_id);

-- === RL Pipeline (from touring_pipeline.db learning tables) ===
CREATE TABLE IF NOT EXISTS learning_wilson (
    id INTEGER PRIMARY KEY,
    tool_name TEXT NOT NULL UNIQUE,
    successes INTEGER DEFAULT 0,
    trials INTEGER DEFAULT 0,
    wilson_score REAL DEFAULT 0.0,
    updated_at TEXT
);

-- state_action is the composite key "ev{state}:{action}" built by TD handlers.
-- trace is the eligibility trace used in TD(λ) updates.
CREATE TABLE IF NOT EXISTS learning_qtable (
    state_action TEXT PRIMARY KEY,
    q_value REAL DEFAULT 0.0,
    trace REAL DEFAULT 0.0
);

CREATE TABLE IF NOT EXISTS learning_drift (
    id INTEGER PRIMARY KEY,
    metric_name TEXT NOT NULL,
    value REAL NOT NULL,
    window_id INTEGER DEFAULT 0,
    recorded_at TEXT
);
CREATE INDEX IF NOT EXISTS idx_drift_metric ON learning_drift(metric_name);

CREATE TABLE IF NOT EXISTS learning_tool_outcomes (
    id INTEGER PRIMARY KEY,
    tool_name TEXT NOT NULL,
    file_path TEXT,
    success INTEGER NOT NULL,
    latency_ms INTEGER,
    context_json TEXT,
    recorded_at TEXT
);
CREATE INDEX IF NOT EXISTS idx_outcomes_tool ON learning_tool_outcomes(tool_name);

CREATE TABLE IF NOT EXISTS learning_linucb (
    arm_id TEXT PRIMARY KEY,
    pulls INTEGER DEFAULT 0,
    cumulative_reward REAL DEFAULT 0.0,
    a_inv_flat BLOB,
    b_vec BLOB,
    total_pulls INTEGER DEFAULT 0
);

-- === Hook Events (from LearningPersistence touring_hook_events) ===
CREATE TABLE IF NOT EXISTS touring_hook_events (
    id INTEGER PRIMARY KEY,
    event_type TEXT NOT NULL,
    hook_name TEXT,
    file_path TEXT,
    duration_ms INTEGER,
    success INTEGER,
    context_json TEXT,
    session_id TEXT,
    created_at TEXT DEFAULT (datetime('now')),
    accessed_at TEXT DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_hook_events_type ON touring_hook_events(event_type);
CREATE INDEX IF NOT EXISTS idx_hook_events_session ON touring_hook_events(session_id);
CREATE INDEX IF NOT EXISTS idx_hook_events_accessed ON touring_hook_events(accessed_at);

-- === Session Registry ===
CREATE TABLE IF NOT EXISTS sessions (
    id INTEGER PRIMARY KEY,
    session_id TEXT NOT NULL UNIQUE,
    task_type TEXT,
    objective TEXT,
    cila_level TEXT,
    started_at TEXT,
    ended_at TEXT,
    quality_score REAL,
    context_json TEXT
);

CREATE TABLE IF NOT EXISTS session_checkpoints (
    id INTEGER PRIMARY KEY,
    session_id TEXT NOT NULL,
    checkpoint_id TEXT NOT NULL,
    data TEXT,
    created_at TEXT,
    UNIQUE(session_id, checkpoint_id)
);

-- Migration metadata
CREATE TABLE IF NOT EXISTS schema_meta (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at TEXT DEFAULT (datetime('now'))
);
INSERT OR IGNORE INTO schema_meta(key, value) VALUES('schema_version', '8');
INSERT OR IGNORE INTO schema_meta(key, value) VALUES('domain', 'graph');

CREATE TABLE IF NOT EXISTS _migration_state (
    phase        TEXT PRIMARY KEY,
    status       TEXT NOT NULL DEFAULT 'pending',
    rows_migrated INTEGER NOT NULL DEFAULT 0,
    started_at   TEXT,
    completed_at TEXT,
    error_message TEXT
);
"#;
