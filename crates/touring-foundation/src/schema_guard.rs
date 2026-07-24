//! Authoritative table name constants — single source of truth.
//!
//! Prevents schema drift: all E2E phases and analysis queries use these
//! constants instead of hardcoded strings. If a table is renamed, only
//! this file needs to change.

// === knowledge.db tables ===

/// File metadata: language, line count, content hash, symbols, imports.
pub const TABLE_FILE_KNOWLEDGE: &str = "file_knowledge";

/// File-to-file import relationships.
pub const TABLE_FILE_RELATIONS: &str = "file_relations";

/// File access log for tracking read patterns.
pub const TABLE_FILE_ACCESS_LOG: &str = "file_access_log";

/// Bash command history: command, exit code, error pattern.
pub const TABLE_BASH_OUTCOMES: &str = "bash_outcomes";

/// Edit events: file, type, summary, error pattern.
/// NOTE: actual table is `edit_history` in the daemon's knowledge.db (v6 schema).
/// The `file_edit_history` rename in v8 was planned but the consolidation migration
/// did not execute against the daemon's live database.
pub const TABLE_EDIT_HISTORY: &str = "edit_history";

/// Per-file pitfalls with decay scoring (renamed from gotchas in v8).
pub const TABLE_GOTCHAS: &str = "gotchas";

/// File co-edit tracking.
pub const TABLE_FILE_COEDITS: &str = "file_coedits";

/// Aggregated risk per file.
pub const TABLE_FILE_RISK_SCORES: &str = "file_risk_scores";

/// Structural wiring: pub symbols and their consumers.
pub const TABLE_WIRING_MAP: &str = "wiring_map";

/// Per-module aggregates: integration score, role, pub count.
pub const TABLE_MODULE_ECOSYSTEM: &str = "module_ecosystem";

/// Module-level functional identity (purpose, domain, I/O types).
pub const TABLE_FUNCTIONAL_SIGNATURES: &str = "functional_signatures";

/// Detected chains between modules (sequential, complementary, hierarchical).
pub const TABLE_FUNCTIONAL_CHAINS: &str = "functional_chains";

/// AST symbol definitions.
pub const TABLE_SYMBOLS: &str = "symbols";

/// Symbol FTS5 index.
pub const TABLE_SYMBOLS_FTS: &str = "symbols_fts";

// === Pln2: Extended knowledge.db tables ===

/// Feature flags per file (from Cargo.toml, pyproject.toml, package.json, shell).
pub const TABLE_FILE_FEATURE_FLAGS: &str = "file_feature_flags";

/// TODOs/FIXMEs per file with kind classification.
pub const TABLE_FILE_TODOS: &str = "file_todos";

/// Edge confidence scores for graph relationships.
pub const TABLE_EDGE_CONFIDENCE: &str = "edge_confidence";

/// Louvain community assignments per file.
pub const TABLE_FILE_COMMUNITIES: &str = "file_communities";

/// Test coverage percentage per file.
pub const TABLE_FILE_TEST_COVERAGE: &str = "file_test_coverage";

/// BLAKE3 hash registry with symbol count and merkle parent.
pub const TABLE_FILE_BLAKE3_REGISTRY: &str = "file_blake3_registry";

/// Session-file skeleton summaries with purpose and gotchas.
pub const TABLE_SESSION_FILE_SUMMARY: &str = "session_file_summary";

/// Symbol-level event log for change tracking.
pub const TABLE_SYMBOL_EVENTS_LOG: &str = "symbol_events_log";

/// Wiring suggestion engine output.
pub const TABLE_WIRING_SUGGESTIONS: &str = "wiring_suggestions";

/// Metadata collection benchmark results.
pub const TABLE_METADATA_BENCHMARK_RUNS: &str = "metadata_benchmark_runs";

/// Cognitive enrichment scores per file.
pub const TABLE_COGNITIVE_ENRICHMENT: &str = "cognitive_enrichment";

// === memory.db tables ===

/// RLM episodic memory entries.
pub const MEMORY_TABLE_RLM_ENTRIES: &str = "rlm_entries";

/// RLM FTS5 index.
pub const MEMORY_TABLE_RLM_FTS: &str = "rlm_fts";

/// Semantic recall embeddings (f32 + u4 quantized).
pub const MEMORY_TABLE_RECALL_EMBEDDINGS: &str = "recall_embeddings";

/// ANN path-hash embeddings.
pub const MEMORY_TABLE_ANN_EMBEDDINGS: &str = "ann_embeddings";

// === graph.db tables ===

/// GoT session snapshots.
pub const GRAPH_TABLE_GOT_SNAPSHOTS: &str = "got_snapshots";

/// Wilson score per tool.
pub const GRAPH_TABLE_LEARNING_WILSON: &str = "learning_wilson";

/// Q-table entries.
pub const GRAPH_TABLE_LEARNING_QTABLE: &str = "learning_qtable";

/// LinUCB bandit state.
pub const GRAPH_TABLE_LEARNING_LINUCB: &str = "learning_linucb";

/// The subset of `expected` tables that do NOT exist in `conn`, in order.
///
/// One `sqlite_master` existence probe per table; a prepare/query error is
/// treated as "missing" (fail-closed — a guard never claims a table is present
/// on a broken connection). This is the single shared check the three
/// `validate_*_tables` guards call — extracting it removed the byte-identical
/// per-DB copies each used to carry.
fn missing_tables(conn: &rusqlite::Connection, expected: &[&'static str]) -> Vec<&'static str> {
    expected
        .iter()
        .copied()
        .filter(|&table| {
            !conn
                .prepare("SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1")
                .and_then(|mut stmt| stmt.exists([table]))
                .unwrap_or(false)
        })
        .collect()
}

/// Validate that expected tables exist in a knowledge DB connection.
///
/// Returns a list of missing table names.
pub fn validate_knowledge_tables(conn: &rusqlite::Connection) -> Vec<&'static str> {
    missing_tables(
        conn,
        &[
            TABLE_FILE_KNOWLEDGE,
            TABLE_BASH_OUTCOMES,
            TABLE_EDIT_HISTORY,
            TABLE_GOTCHAS,
            TABLE_WIRING_MAP,
            TABLE_MODULE_ECOSYSTEM,
            TABLE_FUNCTIONAL_SIGNATURES,
            TABLE_FUNCTIONAL_CHAINS,
            TABLE_FILE_RELATIONS,
            TABLE_FILE_COEDITS,
            TABLE_FILE_RISK_SCORES,
            TABLE_SYMBOLS,
            TABLE_SYMBOLS_FTS,
        ],
    )
}

/// Validate that expected tables exist in a memory DB connection.
pub fn validate_memory_tables(conn: &rusqlite::Connection) -> Vec<&'static str> {
    missing_tables(
        conn,
        &[
            MEMORY_TABLE_RLM_ENTRIES,
            MEMORY_TABLE_RECALL_EMBEDDINGS,
            MEMORY_TABLE_ANN_EMBEDDINGS,
        ],
    )
}

/// Validate that expected tables exist in a graph DB connection.
///
/// Checks GoT snapshots, Wilson scores, Q-table, and LinUCB bandit tables.
/// Returns a vec of missing table names.
pub fn validate_graph_tables(conn: &rusqlite::Connection) -> Vec<&'static str> {
    missing_tables(
        conn,
        &[
            GRAPH_TABLE_GOT_SNAPSHOTS,
            GRAPH_TABLE_LEARNING_WILSON,
            GRAPH_TABLE_LEARNING_QTABLE,
            GRAPH_TABLE_LEARNING_LINUCB,
        ],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_validate_knowledge_tables_on_empty_db() {
        let conn = rusqlite::Connection::open_in_memory().expect("open in-memory");
        let missing = validate_knowledge_tables(&conn);
        assert_eq!(
            missing.len(),
            13,
            "empty DB should miss all 13 knowledge tables"
        );
    }
    #[test]
    fn test_validate_knowledge_tables_on_v8_schema() {
        let conn = rusqlite::Connection::open_in_memory().expect("open in-memory");
        conn.execute_batch(crate::schema::knowledge::KNOWLEDGE_SCHEMA_V8)
            .expect("apply V8 schema");
        let missing = validate_knowledge_tables(&conn);
        assert!(
            missing.is_empty(),
            "V8 schema should have all tables, missing: {missing:?}"
        );
    }
    #[test]
    fn test_validate_memory_tables_on_v8_schema() {
        let conn = rusqlite::Connection::open_in_memory().expect("open in-memory");
        conn.execute_batch(crate::schema::memory::MEMORY_SCHEMA_V8)
            .expect("apply memory V8 schema");
        let missing = validate_memory_tables(&conn);
        assert!(
            missing.is_empty(),
            "memory V8 schema should have all tables, missing: {missing:?}"
        );
    }
    #[test]
    fn test_wiring_map_has_correct_columns() {
        let conn = rusqlite::Connection::open_in_memory().expect("open in-memory");
        conn.execute_batch(crate::schema::knowledge::KNOWLEDGE_SCHEMA_V8)
            .expect("apply V8 schema");
        conn.execute(
            "INSERT INTO wiring_map (module_file, symbol_name, symbol_kind, visibility) \
             VALUES ('test.rs', 'TestStruct', 'struct', 'public')",
            [],
        )
        .expect("wiring_map should have module_file column");
    }
    #[test]
    fn test_bash_outcomes_has_executed_at() {
        let conn = rusqlite::Connection::open_in_memory().expect("open in-memory");
        conn.execute_batch(crate::schema::knowledge::KNOWLEDGE_SCHEMA_V8)
            .expect("apply V8 schema");
        conn.execute(
            "INSERT INTO bash_outcomes (command, command_short, executed_at) \
             VALUES ('cargo test', 'cargo', datetime('now'))",
            [],
        )
        .expect("bash_outcomes should have executed_at column");
    }
    #[test]
    fn test_edit_history_exists() {
        let conn = rusqlite::Connection::open_in_memory().expect("open in-memory");
        conn.execute_batch(crate::schema::knowledge::KNOWLEDGE_SCHEMA_V8)
            .expect("apply V8 schema");
        conn.execute(
            "INSERT INTO edit_history (file_path, edit_type, edited_at) \
             VALUES ('test.rs', 'edit', datetime('now'))",
            [],
        )
        .expect("edit_history should exist with edited_at column");
    }
    #[test]
    fn test_gotchas_exists() {
        let conn = rusqlite::Connection::open_in_memory().expect("open in-memory");
        conn.execute_batch(crate::schema::knowledge::KNOWLEDGE_SCHEMA_V8)
            .expect("apply V8 schema");
        conn.execute(
            "INSERT INTO gotchas (pattern, gotcha, severity, decay_score) \
             VALUES ('test_pattern', 'test gotcha', 'warning', 1.0)",
            [],
        )
        .expect("gotchas should exist");
    }
    #[test]
    fn test_rlm_entries_not_memory_entries() {
        let conn = rusqlite::Connection::open_in_memory().expect("open in-memory");
        conn.execute_batch(crate::schema::memory::MEMORY_SCHEMA_V8)
            .expect("apply memory V8 schema");
        conn.execute(
            "INSERT INTO rlm_entries (key, value, tier) VALUES ('test', 'value', 'semantic')",
            [],
        )
        .expect("rlm_entries should exist (not memory_entries)");
    }
    #[test]
    fn test_functional_tables_exist() {
        let conn = rusqlite::Connection::open_in_memory().expect("open in-memory");
        conn.execute_batch(crate::schema::knowledge::KNOWLEDGE_SCHEMA_V8)
            .expect("apply V8 schema");
        conn.execute(
            "INSERT INTO functional_signatures (file_path, module_purpose, domain) \
             VALUES ('mod.rs', 'entry point', 'core')",
            [],
        )
        .expect("functional_signatures should exist");
        conn.execute(
                "INSERT INTO functional_chains (source_module, source_symbol, sink_module, sink_symbol, chain_type) \
             VALUES ('a.rs', 'fn_a', 'b.rs', 'fn_b', 'Sequential')",
                [],
            )
            .expect("functional_chains should exist");
    }
    #[test]
    fn test_validate_graph_tables_on_empty_db() {
        let conn = rusqlite::Connection::open_in_memory().expect("open in-memory");
        let missing = validate_graph_tables(&conn);
        assert_eq!(
            missing.len(),
            4,
            "empty DB should miss all 4 graph tables, got: {missing:?}"
        );
    }
    #[test]
    fn test_validate_graph_tables_partial() {
        let conn = rusqlite::Connection::open_in_memory().expect("open in-memory");
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS got_snapshots (id INTEGER PRIMARY KEY);
             CREATE TABLE IF NOT EXISTS learning_wilson (id INTEGER PRIMARY KEY);",
        )
        .expect("create partial tables");
        let missing = validate_graph_tables(&conn);
        assert_eq!(
            missing.len(),
            2,
            "should report exactly 2 missing tables, got: {missing:?}"
        );
        assert!(
            missing.contains(&"learning_qtable"),
            "learning_qtable should be missing"
        );
        assert!(
            missing.contains(&"learning_linucb"),
            "learning_linucb should be missing"
        );
    }
}
