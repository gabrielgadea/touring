//! Domain schema DDL for consolidated database architecture (SCHEMA_VERSION=8).
//!
//! Three logical domains replace 8 legacy SQLite files:
//! - `knowledge.db`: symbols, file metadata, wiring
//! - `memory.db`:    RLM, semantic recall, ANN embeddings
//! - `graph.db`:     GoT sessions, RL pipeline, hook events

pub mod entity_registry;
pub mod graph;
pub mod knowledge;
pub mod memory;

/// Apply all three domain schemas to their respective connections.
pub fn ensure_all_schemas(
    knowledge_conn: &rusqlite::Connection,
    memory_conn: &rusqlite::Connection,
    graph_conn: &rusqlite::Connection,
) -> rusqlite::Result<()> {
    knowledge_conn.execute_batch(knowledge::KNOWLEDGE_SCHEMA_V8)?;
    memory_conn.execute_batch(memory::MEMORY_SCHEMA_V8)?;
    // Pre-migrate legacy graph.db schema (Sprint 4.8 drift fix) — adds columns
    // missing from older DBs before CREATE TABLE IF NOT EXISTS runs as a no-op.
    migrate_graph_legacy_columns(graph_conn)?;
    graph_conn.execute_batch(graph::GRAPH_SCHEMA_V8)?;
    Ok(())
}

/// Idempotent ALTER TABLE migration for legacy `touring_hook_events` schema in `graph.db`.
///
/// Adds columns the source declares (`graph.rs:71`) but legacy DBs may lack. SQLite's
/// `CREATE TABLE IF NOT EXISTS` is a no-op on existing tables, so without this helper a
/// pre-existing DB with the older 7-column schema (`event_type/tool_name/outcome/reward`)
/// silently keeps that schema and any code expecting `hook_name/file_path/...` fails or
/// inserts wrong data. Catches "duplicate column name" and "no such table" as success.
///
/// Anchor: 2026-05-24 — Sprint 4.8 follow-up to schema audit (B5).
fn migrate_graph_legacy_columns(conn: &rusqlite::Connection) -> rusqlite::Result<()> {
    const HOOK_EVENTS_COLS: &[(&str, &str)] = &[
        ("hook_name", "TEXT"),
        ("file_path", "TEXT"),
        ("duration_ms", "INTEGER"),
        ("success", "INTEGER"),
        ("context_json", "TEXT"),
        ("session_id", "TEXT"),
    ];
    for (col, decl) in HOOK_EVENTS_COLS {
        let sql = format!("ALTER TABLE touring_hook_events ADD COLUMN {col} {decl}");
        match conn.execute(&sql, []) {
            Ok(_) => {}
            Err(e) => {
                let msg = e.to_string();
                if msg.contains("duplicate column name") || msg.contains("no such table") {
                    continue;
                }
                return Err(e);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_v8_knowledge_applies_cleanly() {
        let conn = rusqlite::Connection::open_in_memory().expect("in-memory");
        conn.execute_batch(knowledge::KNOWLEDGE_SCHEMA_V8)
            .expect("apply schema");
        let domain: String = conn
            .query_row(
                "SELECT value FROM schema_meta WHERE key='domain'",
                [],
                |r| r.get(0),
            )
            .expect("query domain");
        assert_eq!(domain, "knowledge");
    }

    #[test]
    fn schema_v8_knowledge_version_is_8() {
        let conn = rusqlite::Connection::open_in_memory().expect("in-memory");
        conn.execute_batch(knowledge::KNOWLEDGE_SCHEMA_V8)
            .expect("apply schema");
        let version: String = conn
            .query_row(
                "SELECT value FROM schema_meta WHERE key='schema_version'",
                [],
                |r| r.get(0),
            )
            .expect("query version");
        assert_eq!(version, "8");
    }

    #[test]
    fn schema_v8_memory_applies_cleanly() {
        let conn = rusqlite::Connection::open_in_memory().expect("in-memory");
        conn.execute_batch(memory::MEMORY_SCHEMA_V8)
            .expect("apply schema");
        let domain: String = conn
            .query_row(
                "SELECT value FROM schema_meta WHERE key='domain'",
                [],
                |r| r.get(0),
            )
            .expect("query domain");
        assert_eq!(domain, "memory");
    }

    #[test]
    fn schema_v8_memory_version_is_8() {
        let conn = rusqlite::Connection::open_in_memory().expect("in-memory");
        conn.execute_batch(memory::MEMORY_SCHEMA_V8)
            .expect("apply schema");
        let version: String = conn
            .query_row(
                "SELECT value FROM schema_meta WHERE key='schema_version'",
                [],
                |r| r.get(0),
            )
            .expect("query version");
        assert_eq!(version, "8");
    }

    #[test]
    fn schema_v8_graph_applies_cleanly() {
        let conn = rusqlite::Connection::open_in_memory().expect("in-memory");
        conn.execute_batch(graph::GRAPH_SCHEMA_V8)
            .expect("apply schema");
        let domain: String = conn
            .query_row(
                "SELECT value FROM schema_meta WHERE key='domain'",
                [],
                |r| r.get(0),
            )
            .expect("query domain");
        assert_eq!(domain, "graph");
    }

    #[test]
    fn schema_v8_graph_version_is_8() {
        let conn = rusqlite::Connection::open_in_memory().expect("in-memory");
        conn.execute_batch(graph::GRAPH_SCHEMA_V8)
            .expect("apply schema");
        let version: String = conn
            .query_row(
                "SELECT value FROM schema_meta WHERE key='schema_version'",
                [],
                |r| r.get(0),
            )
            .expect("query version");
        assert_eq!(version, "8");
    }

    #[test]
    fn schema_v8_ensure_all_applies_all_three() {
        let k = rusqlite::Connection::open_in_memory().expect("k");
        let m = rusqlite::Connection::open_in_memory().expect("m");
        let g = rusqlite::Connection::open_in_memory().expect("g");
        ensure_all_schemas(&k, &m, &g).expect("apply all");
        // Verify each domain marker
        let kd: String = k
            .query_row(
                "SELECT value FROM schema_meta WHERE key='domain'",
                [],
                |r| r.get(0),
            )
            .expect("k domain");
        let md: String = m
            .query_row(
                "SELECT value FROM schema_meta WHERE key='domain'",
                [],
                |r| r.get(0),
            )
            .expect("m domain");
        let gd: String = g
            .query_row(
                "SELECT value FROM schema_meta WHERE key='domain'",
                [],
                |r| r.get(0),
            )
            .expect("g domain");
        assert_eq!(kd, "knowledge");
        assert_eq!(md, "memory");
        assert_eq!(gd, "graph");
    }

    #[test]
    fn schema_v8_knowledge_idempotent() {
        // Applying the schema twice should not fail (IF NOT EXISTS guards)
        let conn = rusqlite::Connection::open_in_memory().expect("in-memory");
        conn.execute_batch(knowledge::KNOWLEDGE_SCHEMA_V8)
            .expect("first apply");
        conn.execute_batch(knowledge::KNOWLEDGE_SCHEMA_V8)
            .expect("second apply");
    }

    #[test]
    fn schema_v8_memory_idempotent() {
        let conn = rusqlite::Connection::open_in_memory().expect("in-memory");
        conn.execute_batch(memory::MEMORY_SCHEMA_V8)
            .expect("first apply");
        conn.execute_batch(memory::MEMORY_SCHEMA_V8)
            .expect("second apply");
    }

    #[test]
    fn schema_v8_graph_idempotent() {
        let conn = rusqlite::Connection::open_in_memory().expect("in-memory");
        conn.execute_batch(graph::GRAPH_SCHEMA_V8)
            .expect("first apply");
        conn.execute_batch(graph::GRAPH_SCHEMA_V8)
            .expect("second apply");
    }
}
