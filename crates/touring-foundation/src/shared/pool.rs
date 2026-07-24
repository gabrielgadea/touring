//! ConnectionPool — domain-oriented SQLite connection manager.
//!
//! ## Abstraction Boundary
//!
//! This module manages the **lifecycle of SQLite database connections** for the
//! three Touring domain databases (knowledge, memory, graph). It is NOT related
//! to WASM module lifecycle management.
//!
//! ### Relationship to touring-wasm/pool.rs
//!
//! | Layer | File | Role |
//! |-------|------|------|
//! | **WASM module lifecycle** | `touring-wasm/src/pool.rs` | When WASM modules are created/destroyed, connection pooling via semaphore+deque |
//! | **WASM KV data cache** | `touring-wasm/src/cache_manager.rs` | What data of runs is cached, independent of lifecycle |
//! | **SQLite lifecycle** | `touring-foundation/src/shared/pool.rs` | When SQLite connections are opened/closed, domain-oriented connection management |
//!
//! These are three independent abstraction layers. `ConnectionPool` does NOT
//! manage WASM modules — it only manages SQLite connections for the Touring
//! daemon's three domain databases.
//!
//! ## Rationale (Phase 2.4)
//!
//! The pre-consolidation design had 7 independent DB paths scattered across
//! multiple crates. `ConnectionPool` centralises path resolution so that the
//! three logical domains can be addressed uniformly, regardless of whether the
//! underlying files are the legacy per-crate databases or the unified DBs
//! produced by the Phase 2.1-2.3 migration.
//!
//! # Example
//!
//! ```rust,ignore
//! let pool = ConnectionPool::new(config.project_root());
//! let conn = pool.open_knowledge()?;
//! conn.execute("SELECT 1", [])?;
//! ```

use std::path::{Path, PathBuf};

use rusqlite::Connection;

/// WAL-mode PRAGMAs applied to every connection opened by this pool.
const INIT_SQL: &str = "
PRAGMA journal_mode = WAL;
PRAGMA synchronous = NORMAL;
PRAGMA temp_store = MEMORY;
PRAGMA foreign_keys = ON;
";

/// Result alias re-using `rusqlite::Error` for conciseness.
pub type PoolResult<T> = Result<T, rusqlite::Error>;

/// Domain-oriented SQLite connection manager.
///
/// Holds the file-system paths for three logical Touring domains and
/// opens on-demand connections with consistent PRAGMA tuning.
#[derive(Debug, Clone)]
pub struct ConnectionPool {
    knowledge_path: PathBuf,
    memory_path: PathBuf,
    graph_path: PathBuf,
}

impl ConnectionPool {
    /// Create a pool rooted at `project_root`.
    ///
    /// Database files are placed in `<project_root>/.claude/touring/`:
    /// - `knowledge.db` — symbols, file metadata, pipeline chunks
    /// - `memory.db`    — RLM entries, semantic graph nodes/edges
    /// - `graph.db`     — sessions, GoT snapshots
    pub fn new(project_root: impl AsRef<Path>) -> Self {
        let base = project_root.as_ref().join(".claude").join("touring");
        Self {
            knowledge_path: base.join("knowledge.db"),
            memory_path: base.join("memory.db"),
            graph_path: base.join("graph.db"),
        }
    }

    /// Create a pool with explicit paths (useful for testing and migration).
    pub fn with_paths(
        knowledge: impl Into<PathBuf>,
        memory: impl Into<PathBuf>,
        graph: impl Into<PathBuf>,
    ) -> Self {
        Self {
            knowledge_path: knowledge.into(),
            memory_path: memory.into(),
            graph_path: graph.into(),
        }
    }

    /// Open a connection to the `knowledge` domain database.
    pub fn open_knowledge(&self) -> PoolResult<Connection> {
        open_with_pragmas(&self.knowledge_path)
    }

    /// Open a connection to the `memory` domain database.
    pub fn open_memory(&self) -> PoolResult<Connection> {
        open_with_pragmas(&self.memory_path)
    }

    /// Open a connection to the `graph` domain database.
    pub fn open_graph(&self) -> PoolResult<Connection> {
        open_with_pragmas(&self.graph_path)
    }

    /// Path to the `knowledge` domain database file.
    pub fn knowledge_path(&self) -> &Path {
        &self.knowledge_path
    }

    /// Path to the `memory` domain database file.
    pub fn memory_path(&self) -> &Path {
        &self.memory_path
    }

    /// Path to the `graph` domain database file.
    pub fn graph_path(&self) -> &Path {
        &self.graph_path
    }

    /// Check whether all three database files exist on disk.
    pub fn all_exist(&self) -> bool {
        self.knowledge_path.exists() && self.memory_path.exists() && self.graph_path.exists()
    }

    /// Ensure the parent directory exists for all three paths.
    pub fn ensure_dirs(&self) -> std::io::Result<()> {
        for path in [&self.knowledge_path, &self.memory_path, &self.graph_path] {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
        }
        Ok(())
    }
}

/// Open a `rusqlite::Connection` at `path` with WAL + NORMAL sync PRAGMAs.
fn open_with_pragmas(path: &Path) -> PoolResult<Connection> {
    let conn = Connection::open(path)?;
    conn.execute_batch(INIT_SQL)?;
    Ok(conn)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn pool_paths_rooted_under_claude_touring() {
        let pool = ConnectionPool::new("/tmp/myproject");
        assert!(pool.knowledge_path().ends_with("knowledge.db"));
        assert!(pool.memory_path().ends_with("memory.db"));
        assert!(pool.graph_path().ends_with("graph.db"));
        // All paths must share the same parent
        assert_eq!(pool.knowledge_path().parent(), pool.memory_path().parent());
    }

    #[test]
    fn open_knowledge_creates_file() {
        let dir = tempdir().expect("tempdir");
        let pool = ConnectionPool::with_paths(
            dir.path().join("k.db"),
            dir.path().join("m.db"),
            dir.path().join("g.db"),
        );
        let conn = pool.open_knowledge().expect("open knowledge");
        let n: i64 = conn
            .query_row("SELECT 42", [], |r| r.get(0))
            .expect("query");
        assert_eq!(n, 42);
    }

    #[test]
    fn open_memory_creates_file() {
        let dir = tempdir().expect("tempdir");
        let pool = ConnectionPool::with_paths(
            dir.path().join("k.db"),
            dir.path().join("m.db"),
            dir.path().join("g.db"),
        );
        let conn = pool.open_memory().expect("open memory");
        // WAL mode should be active
        let mode: String = conn
            .query_row("PRAGMA journal_mode", [], |r| r.get(0))
            .expect("pragma");
        assert_eq!(mode, "wal");
    }

    #[test]
    fn open_graph_creates_file() {
        let dir = tempdir().expect("tempdir");
        let pool = ConnectionPool::with_paths(
            dir.path().join("k.db"),
            dir.path().join("m.db"),
            dir.path().join("g.db"),
        );
        pool.open_graph().expect("open graph");
        assert!(dir.path().join("g.db").exists());
    }

    #[test]
    fn all_exist_false_when_missing() {
        let pool = ConnectionPool::new("/tmp/nonexistent_project_xyz");
        assert!(!pool.all_exist());
    }

    #[test]
    fn ensure_dirs_creates_parent() {
        let dir = tempdir().expect("tempdir");
        let nested = dir.path().join("a").join("b").join("c");
        let pool = ConnectionPool::with_paths(
            nested.join("k.db"),
            nested.join("m.db"),
            nested.join("g.db"),
        );
        pool.ensure_dirs().expect("ensure_dirs");
        assert!(nested.exists());
    }
}
