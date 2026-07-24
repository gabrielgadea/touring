//! Schema migration engine for Touring SQLite databases.
//!
//! Uses `PRAGMA user_version` for version tracking.
//! Migrations are idempotent and run in a transaction.

use rusqlite::Connection;
use tracing_attributes::instrument;

/// Current knowledge-graph schema version.
///
/// Defined here (in `touring-foundation`) so that any crate that depends on
/// `touring-foundation` can import a single authoritative value instead of each
/// maintaining its own copy.  `touring-hooks` imports this via
/// `use touring_foundation::migration::SCHEMA_VERSION;`.
///
/// Bump only when adding a new migration block to `touring-hooks::FileKnowledgeDB`.
pub const SCHEMA_VERSION: u32 = 8;

/// A single schema migration step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Migration {
    /// Version this migration upgrades TO (1-based).
    pub version: u32,
    /// Human-readable description.
    pub description: &'static str,
    /// SQL to execute (can be multi-statement separated by `;`).
    pub sql: &'static str,
}

impl std::fmt::Display for Migration {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "v{}: {}", self.version, self.description)
    }
}

/// Read the current `PRAGMA user_version` from the database.
///
/// # Errors
///
/// Returns `TouringError::Sqlite` if the query fails.
#[must_use = "migration version should be checked"]
#[instrument(skip(conn), level = tracing::Level::DEBUG)]
pub fn current_version(conn: &Connection) -> crate::Result<u32> {
    conn.query_row("PRAGMA user_version", [], |row| row.get::<_, u32>(0))
        .map_err(crate::TouringError::from)
}

/// Run all pending migrations on a database connection.
///
/// Reads `PRAGMA user_version` to determine current version,
/// then applies each migration with `version > current` in order.
/// Each migration runs inside an EXCLUSIVE transaction. On success,
/// `user_version` is bumped atomically within the same transaction.
///
/// # Errors
///
/// Returns an error if any migration fails. Already-applied migrations
/// are skipped (idempotent by version check). On failure the offending
/// transaction is rolled back; earlier successful migrations persist.
#[instrument(skip(conn, migrations), level = tracing::Level::INFO, fields(migration_count = migrations.len()))]
pub fn run_migrations(conn: &Connection, migrations: &[Migration]) -> crate::Result<u32> {
    let mut version = current_version(conn)?;
    let mut applied = 0u32;

    for m in migrations {
        if m.version <= version {
            continue;
        }

        tracing::info!("Running migration v{}: {}", m.version, m.description);

        conn.execute_batch("BEGIN EXCLUSIVE").map_err(|e| {
            crate::TouringError::Internal(format!("Failed to begin migration v{}: {e}", m.version))
        })?;

        if let Err(e) = conn.execute_batch(m.sql) {
            let _ = conn.execute_batch("ROLLBACK");
            return Err(crate::TouringError::Internal(format!(
                "Migration v{} ({}) failed: {e}",
                m.version, m.description
            )));
        }

        // Bump user_version inside the same transaction.
        let pragma_sql = format!("PRAGMA user_version = {}", m.version);
        if let Err(e) = conn.execute_batch(&pragma_sql) {
            let _ = conn.execute_batch("ROLLBACK");
            return Err(crate::TouringError::Internal(format!(
                "Failed to update user_version to {}: {e}",
                m.version
            )));
        }

        conn.execute_batch("COMMIT").map_err(|e| {
            crate::TouringError::Internal(format!("Failed to commit migration v{}: {e}", m.version))
        })?;

        version = m.version;
        applied += 1;
    }

    if applied > 0 {
        tracing::info!("Applied {applied} migration(s). Database now at v{version}");
    }

    Ok(version)
}

// ── Consolidation module (full implementation) ────────────────────────────
pub mod consolidation;

// Re-exported items from consolidation (full implementation)
pub use consolidation::ConsolidationMigration;
pub use consolidation::insert_from_attached;
pub use consolidation::mig_src_table_exists;

// Stub functions for binary compatibility (consolidation.rs may not have these)
/// Rebuild the FTS5 index for the given table. No-op stub used
/// for binary compatibility with downstream consumers that
/// expect the symbol to exist even when consolidation is the
/// real implementation. Always returns `Ok(())`.
pub fn rebuild_fts5(_conn: &rusqlite::Connection, _tbl: &str) -> anyhow::Result<()> {
    Ok(())
}
/// Convenience wrapper that opens an attached database for the
/// duration of the closure. No-op stub used for binary
/// compatibility; the real implementation lives in
/// `consolidation::with_attached`.
pub fn with_attached<T>(
    _conn: &rusqlite::Connection,
    _path: &std::path::Path,
    _f: impl FnOnce(&rusqlite::Connection) -> anyhow::Result<T>,
) -> anyhow::Result<T> {
    anyhow::bail!("with_attached not available in this build")
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn memory_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA journal_mode=WAL;").unwrap();
        conn
    }

    #[test]
    fn test_initial_version_is_zero() {
        let conn = memory_conn();
        assert_eq!(current_version(&conn).unwrap(), 0);
    }

    #[test]
    fn test_run_single_migration() {
        let conn = memory_conn();
        let migrations = [Migration {
            version: 1,
            description: "create test table",
            sql: "CREATE TABLE test (id INTEGER PRIMARY KEY, name TEXT);",
        }];
        let ver = run_migrations(&conn, &migrations).unwrap();
        assert_eq!(ver, 1);
        assert_eq!(current_version(&conn).unwrap(), 1);
        // Table should exist and be empty.
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM test", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn test_run_multiple_migrations() {
        let conn = memory_conn();
        let migrations = [
            Migration {
                version: 1,
                description: "create users",
                sql: "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT);",
            },
            Migration {
                version: 2,
                description: "add email column",
                sql: "ALTER TABLE users ADD COLUMN email TEXT;",
            },
            Migration {
                version: 3,
                description: "create index",
                sql: "CREATE INDEX idx_users_email ON users(email);",
            },
        ];
        let ver = run_migrations(&conn, &migrations).unwrap();
        assert_eq!(ver, 3);
        assert_eq!(current_version(&conn).unwrap(), 3);
    }

    #[test]
    fn test_skip_already_applied() {
        let conn = memory_conn();
        let migrations = [
            Migration {
                version: 1,
                description: "create table",
                sql: "CREATE TABLE t1 (id INTEGER);",
            },
            Migration {
                version: 2,
                description: "create table 2",
                sql: "CREATE TABLE t2 (id INTEGER);",
            },
        ];
        // First run applies both.
        run_migrations(&conn, &migrations).unwrap();
        // Second run should skip both and still report v2.
        let ver = run_migrations(&conn, &migrations).unwrap();
        assert_eq!(ver, 2);
    }

    #[test]
    fn test_incremental_application() {
        let conn = memory_conn();

        // Apply only migration 1.
        let first = [Migration {
            version: 1,
            description: "create table",
            sql: "CREATE TABLE items (id INTEGER);",
        }];
        let ver = run_migrations(&conn, &first).unwrap();
        assert_eq!(ver, 1);

        // Now apply both 1 and 2 — only 2 should run.
        let both = [
            Migration {
                version: 1,
                description: "create table",
                sql: "CREATE TABLE items (id INTEGER);",
            },
            Migration {
                version: 2,
                description: "add name column",
                sql: "ALTER TABLE items ADD COLUMN name TEXT;",
            },
        ];
        let ver = run_migrations(&conn, &both).unwrap();
        assert_eq!(ver, 2);
    }

    #[test]
    fn test_failed_migration_rolls_back() {
        let conn = memory_conn();
        let migrations = [
            Migration {
                version: 1,
                description: "create table",
                sql: "CREATE TABLE good (id INTEGER);",
            },
            Migration {
                version: 2,
                description: "bad SQL",
                sql: "THIS IS NOT VALID SQL;",
            },
        ];
        let result = run_migrations(&conn, &migrations);
        assert!(result.is_err());
        // Version should be 1 — first migration succeeded, second rolled back.
        assert_eq!(current_version(&conn).unwrap(), 1);
        // The good table exists.
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM good", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn test_empty_migrations() {
        let conn = memory_conn();
        let ver = run_migrations(&conn, &[]).unwrap();
        assert_eq!(ver, 0);
    }

    #[test]
    fn test_schema_version_8_is_current() {
        assert_eq!(SCHEMA_VERSION, 8);
    }

    #[test]
    fn test_multi_statement_migration() {
        let conn = memory_conn();
        let migrations = [Migration {
            version: 1,
            description: "create two tables in one migration",
            sql: "CREATE TABLE a (id INTEGER);
                  CREATE TABLE b (id INTEGER);
                  CREATE INDEX idx_a ON a(id);",
        }];
        let ver = run_migrations(&conn, &migrations).unwrap();
        assert_eq!(ver, 1);
        // Both tables should exist.
        conn.query_row("SELECT COUNT(*) FROM a", [], |r| r.get::<_, i64>(0))
            .unwrap();
        conn.query_row("SELECT COUNT(*) FROM b", [], |r| r.get::<_, i64>(0))
            .unwrap();
    }
}
