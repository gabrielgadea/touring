//! Entity Identity Schema — D5.1 SQLite DDL + schema management.
//!
//! ## SQLite DDL
//!
//! ```sql
//! CREATE TABLE IF NOT EXISTS entities (
//!     id              TEXT PRIMARY KEY,
//!     canonical_name  TEXT NOT NULL,
//!     kind            TEXT NOT NULL,
//!     crate_name      TEXT NOT NULL,
//!     source_path     TEXT,
//!     definition_line INTEGER,
//!     doc_summary     TEXT
//! );
//!
//! CREATE TABLE IF NOT EXISTS entity_criteria (
//!     id              INTEGER PRIMARY KEY AUTOINCREMENT,
//!     entity_id       TEXT NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
//!     criterion_name TEXT NOT NULL,
//!     description     TEXT NOT NULL
//! );
//!
//! CREATE TABLE IF NOT EXISTS entity_relations (
//!     id              INTEGER PRIMARY KEY AUTOINCREMENT,
//!     from_entity_id  TEXT NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
//!     to_entity_id    TEXT NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
//!     relation_kind   TEXT NOT NULL,
//!     justification   TEXT
//! );
//!
//! CREATE INDEX IF NOT EXISTS idx_entities_crate
//!     ON entities(crate_name);
//! CREATE INDEX IF NOT EXISTS idx_entities_kind
//!     ON entities(kind);
//! CREATE INDEX IF NOT EXISTS idx_criteria_entity
//!     ON entity_criteria(entity_id);
//! CREATE INDEX IF NOT EXISTS idx_relations_from
//!     ON entity_relations(from_entity_id);
//! CREATE INDEX IF NOT EXISTS idx_relations_to
//!     ON entity_relations(to_entity_id);
//! ```

use std::path::Path;

/// SQL statement to create the `entities` table.
pub const SQL_CREATE_ENTITIES: &str = r#"
CREATE TABLE IF NOT EXISTS entities (
    id              TEXT PRIMARY KEY,
    canonical_name TEXT NOT NULL,
    kind            TEXT NOT NULL,
    crate_name      TEXT NOT NULL,
    source_path     TEXT,
    definition_line INTEGER,
    doc_summary     TEXT,
    auto_seeded     INTEGER NOT NULL DEFAULT 0,
    canonical       INTEGER NOT NULL DEFAULT 0
)"#;

/// SQL statement to add auto_seeded/canonical columns to existing tables.
/// Idempotent: uses ALTER TABLE ADD COLUMN IF NOT EXISTS (SQLite 3.39.0+).
/// For older SQLite, catches duplicate column errors and continues.
pub const SQL_MIGRATE_AUTO_SEEDED: &str = r#"
ALTER TABLE entities ADD COLUMN auto_seeded INTEGER NOT NULL DEFAULT 0
"#;

/// SQL statement to add the canonical flag column to existing tables.
/// Idempotent: uses ALTER TABLE ADD COLUMN IF NOT EXISTS (SQLite 3.39.0+).
/// For older SQLite, catches duplicate column errors and continues.
pub const SQL_MIGRATE_CANONICAL: &str = r#"
ALTER TABLE entities ADD COLUMN canonical INTEGER NOT NULL DEFAULT 0
"#;

/// SQL statement to create the `entity_criteria` table for entity quality criteria.
pub const SQL_CREATE_CRITERIA: &str = r#"
CREATE TABLE IF NOT EXISTS entity_criteria (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    entity_id       TEXT NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
    criterion_name  TEXT NOT NULL,
    description     TEXT NOT NULL
)"#;

/// SQL statement to create the `entity_relations` table for entity relationships.
pub const SQL_CREATE_RELATIONS: &str = r#"
CREATE TABLE IF NOT EXISTS entity_relations (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    from_entity_id  TEXT NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
    to_entity_id    TEXT NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
    relation_kind   TEXT NOT NULL,
    justification   TEXT
)"#;

/// SQL statements to create indexes on entity and relation tables for query performance.
pub const SQL_CREATE_INDEXES: &str = r#"
CREATE INDEX IF NOT EXISTS idx_entities_crate
    ON entities(crate_name);
CREATE INDEX IF NOT EXISTS idx_entities_kind
    ON entities(kind);
CREATE INDEX IF NOT EXISTS idx_criteria_entity
    ON entity_criteria(entity_id);
CREATE INDEX IF NOT EXISTS idx_relations_from
    ON entity_relations(from_entity_id);
CREATE INDEX IF NOT EXISTS idx_relations_to
    ON entity_relations(to_entity_id)
"#;

/// Runs all DDL statements against an open SQLite connection.
pub fn run_ddl(conn: &mut rusqlite::Connection) -> rusqlite::Result<()> {
    conn.execute_batch(SQL_CREATE_ENTITIES)?;
    conn.execute_batch(SQL_CREATE_CRITERIA)?;
    conn.execute_batch(SQL_CREATE_RELATIONS)?;
    conn.execute_batch(SQL_CREATE_INDEXES)?;
    run_migrations(conn)?;
    Ok(())
}

/// Runs idempotent schema migrations for backward-compat with older databases.
/// SQLite 3.39.0+ supports `ADD COLUMN IF NOT EXISTS` natively; for older versions
/// we catch the error and continue.
fn run_migrations(conn: &mut rusqlite::Connection) -> rusqlite::Result<()> {
    // Both migrations are ADD COLUMN on a table that CREATE TABLE may already
    // have created with the column. "duplicate column name" is the expected
    // outcome on a current database and is deliberately ignored.
    //
    // 2026-08-20: this used to branch on `r1.is_err() || r2.is_err()` and then
    // run `INSERT INTO sqlite_master DEFAULT VALUES`, discarding the result.
    // `sqlite_master` is read-only without `writable_schema`, so that insert
    // always failed and the whole branch was dead — which is why the mutant
    // `replace || with &&` survived: there was no behaviour to observe.
    let _ = conn.execute_batch(SQL_MIGRATE_AUTO_SEEDED);
    let _ = conn.execute_batch(SQL_MIGRATE_CANONICAL);
    Ok(())
}

/// Opens a database file, creating it if it does not exist, and runs DDL.
pub fn open_or_create<P: AsRef<Path>>(path: P) -> rusqlite::Result<rusqlite::Connection> {
    let mut conn = rusqlite::Connection::open(path)?;
    run_ddl(&mut conn)?;
    Ok(conn)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    /// Migrations must actually add the columns a legacy database lacks.
    ///
    /// Kills the 2026-08-20 survivor `replace run_migrations -> Ok(())`:
    /// every existing test opened a fresh database, where `run_ddl` already
    /// creates `auto_seeded` and `canonical`, so a no-op migration was
    /// indistinguishable from a real one. This builds the pre-migration shape
    /// on purpose — the only state where the function has work to do.
    #[test]
    fn run_migrations_adds_columns_missing_from_a_legacy_table() {
        let mut conn = rusqlite::Connection::open_in_memory().unwrap();
        // The `entities` table as it existed BEFORE the two columns landed.
        conn.execute_batch(
            "CREATE TABLE entities (
                 id TEXT PRIMARY KEY,
                 canonical_name TEXT NOT NULL,
                 kind TEXT NOT NULL,
                 crate_name TEXT NOT NULL,
                 source_path TEXT,
                 definition_line INTEGER,
                 doc_summary TEXT
             )",
        )
        .unwrap();

        let before = column_names(&conn);
        assert!(
            !before.contains(&"auto_seeded".to_owned()),
            "fixture must start without the column"
        );

        run_migrations(&mut conn).unwrap();

        let after = column_names(&conn);
        assert!(
            after.contains(&"auto_seeded".to_owned()),
            "auto_seeded was not added: {after:?}"
        );
        assert!(
            after.contains(&"canonical".to_owned()),
            "canonical was not added: {after:?}"
        );
    }

    /// Running the migrations twice is a no-op, not an error.
    #[test]
    fn run_migrations_is_idempotent_on_a_current_table() {
        let mut conn = rusqlite::Connection::open_in_memory().unwrap();
        run_ddl(&mut conn).unwrap();
        run_migrations(&mut conn).unwrap();
        run_migrations(&mut conn).unwrap();
        let cols = column_names(&conn);
        assert_eq!(
            cols.iter().filter(|c| *c == "canonical").count(),
            1,
            "the column must exist exactly once"
        );
    }

    /// Column names of the `entities` table, in declaration order.
    fn column_names(conn: &rusqlite::Connection) -> Vec<String> {
        let mut stmt = conn.prepare("PRAGMA table_info(entities)").unwrap();
        stmt.query_map([], |r| r.get::<_, String>(1))
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap()
    }

    #[test]
    fn run_ddl_creates_tables() {
        let tmp = NamedTempFile::new().unwrap();
        let mut conn = rusqlite::Connection::open(tmp.path()).unwrap();
        run_ddl(&mut conn).unwrap();

        conn.execute("INSERT INTO entities (id, canonical_name, kind, crate_name) VALUES ('test', 'test::Entity', 'Type', 'test-crate')", []).unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM entities", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn run_ddl_is_idempotent() {
        let tmp = NamedTempFile::new().unwrap();
        let mut conn = rusqlite::Connection::open(tmp.path()).unwrap();
        run_ddl(&mut conn).unwrap();
        run_ddl(&mut conn).unwrap(); // must not error
    }

    #[test]
    fn open_or_create_returns_connection() {
        let tmp = NamedTempFile::new().unwrap();
        let conn = open_or_create(tmp.path()).unwrap();
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(count >= 3); // entities + criteria + relations
    }
}
