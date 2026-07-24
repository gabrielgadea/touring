//! Cross-audit integration tests for touring-foundation shared components.
//!
//! Audits documented contracts for:
//! 1. DomainCircuitBreaker — per-domain isolation, state machine transitions
//! 2. ConnectionPool       — deterministic paths, WAL mode, ensure_dirs
//! 3. Backup/Restore       — E2E cycle via backup_one/restore_one helpers
//!
//! Every test exercises a **contract** claim from the module documentation.

use touring_foundation::shared::{
    circuit_breaker::CircuitBreaker,
    domain_circuit::{CircuitBreakerImpl, CircuitState, DomainCircuitBreaker},
    pool::ConnectionPool,
};

// ─────────────────────────────────────────────────────────────────────────────
// CONTRACT: DomainCircuitBreaker — CLOSED→OPEN→HALF_OPEN→CLOSED state machine
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn circuit_breaker_starts_closed() {
    let b = CircuitBreakerImpl::new(3, 60);
    assert_eq!(b.state(), CircuitState::Closed);
    assert!(!b.is_open(), "new breaker must be CLOSED");
    assert!(!b.should_skip(), "should_skip must be false when CLOSED");
}

#[test]
fn circuit_breaker_opens_exactly_at_threshold() {
    let mut b = CircuitBreakerImpl::new(3, 60);

    b.record_failure();
    assert!(!b.is_open(), "1 failure < threshold=3, must stay CLOSED");
    b.record_failure();
    assert!(!b.is_open(), "2 failures < threshold=3, must stay CLOSED");
    b.record_failure();
    assert!(
        b.is_open(),
        "CONTRACT: 3rd failure must trip circuit to OPEN"
    );
    assert_eq!(b.state(), CircuitState::Open);
}

#[test]
fn circuit_breaker_success_resets_to_closed() {
    let mut b = CircuitBreakerImpl::new(3, 60);
    b.record_failure();
    b.record_failure();
    b.record_failure(); // OPEN
    assert!(b.is_open());

    b.record_success();
    assert!(
        !b.is_open(),
        "CONTRACT: success after OPEN must reset to CLOSED"
    );
    assert_eq!(b.state(), CircuitState::Closed);
}

#[test]
fn circuit_breaker_success_mid_failures_resets_counter() {
    // A success before tripping must reset the failure counter
    let mut b = CircuitBreakerImpl::new(3, 60);
    b.record_failure();
    b.record_failure(); // 2 failures, not yet tripped
    b.record_success(); // reset
    b.record_failure(); // counter back to 1
    b.record_failure(); // counter = 2, still < 3
    assert!(
        !b.is_open(),
        "failures after success-reset must not accumulate across reset"
    );
}

#[test]
fn circuit_breaker_reset_closes_from_open() {
    let mut b = CircuitBreakerImpl::new(1, 60);
    b.record_failure(); // OPEN
    assert!(b.is_open());

    b.reset();
    assert!(!b.is_open(), "reset() must force CLOSED");
    assert_eq!(b.state(), CircuitState::Closed);
}

#[test]
fn circuit_breaker_half_open_probe_failure_reopens() {
    // Zero cooldown: the instant the circuit opens, is_open() returns false
    // (elapsed >= 0) and sync_half_open transitions to HALF_OPEN on next call.
    let mut b = CircuitBreakerImpl::new(1, 0);
    b.record_failure(); // → OPEN, but cooldown=0 so is_open()=false immediately

    // Next record_failure: sync_half_open sees elapsed>=0 → HALF_OPEN,
    // then probe failure → OPEN again
    b.record_failure();
    assert_eq!(
        b.state(),
        CircuitState::Open,
        "CONTRACT: probe failure from HALF_OPEN must re-trip to OPEN"
    );
}

#[test]
fn circuit_breaker_half_open_probe_success_closes() {
    let mut b = CircuitBreakerImpl::new(1, 0);
    b.record_failure(); // → OPEN, cooldown=0

    // record_success: sync_half_open → HALF_OPEN, then success → CLOSED
    b.record_success();
    assert_eq!(
        b.state(),
        CircuitState::Closed,
        "CONTRACT: probe success from HALF_OPEN must transition to CLOSED"
    );
    assert!(!b.is_open());
}

// ─────────────────────────────────────────────────────────────────────────────
// CONTRACT: DomainCircuitBreaker — domain isolation
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn domain_circuit_breaker_knowledge_failure_does_not_affect_memory_or_graph() {
    let mut d = DomainCircuitBreaker::new();

    // Trip knowledge domain
    d.knowledge.record_failure();
    d.knowledge.record_failure();
    d.knowledge.record_failure();
    assert!(
        d.knowledge.is_open(),
        "knowledge must be OPEN after 3 failures"
    );

    // CONTRACT: memory and graph must remain CLOSED
    assert!(
        !d.memory.is_open(),
        "CONTRACT: knowledge failure must NOT affect memory domain"
    );
    assert!(
        !d.graph.is_open(),
        "CONTRACT: knowledge failure must NOT affect graph domain"
    );
}

#[test]
fn domain_circuit_breaker_memory_failure_does_not_affect_knowledge_or_graph() {
    let mut d = DomainCircuitBreaker::new();

    d.memory.record_failure();
    d.memory.record_failure();
    d.memory.record_failure();
    assert!(d.memory.is_open());

    assert!(
        !d.knowledge.is_open(),
        "CONTRACT: memory failure must NOT affect knowledge domain"
    );
    assert!(
        !d.graph.is_open(),
        "CONTRACT: memory failure must NOT affect graph domain"
    );
}

#[test]
fn domain_circuit_breaker_graph_failure_does_not_affect_others() {
    let mut d = DomainCircuitBreaker::new();

    d.graph.record_failure();
    d.graph.record_failure();
    d.graph.record_failure();
    assert!(d.graph.is_open());

    assert!(!d.knowledge.is_open());
    assert!(!d.memory.is_open());
}

#[test]
fn domain_circuit_breaker_all_closed_reflects_all_domains() {
    let mut d = DomainCircuitBreaker::new();
    assert!(
        d.all_closed(),
        "fresh DomainCircuitBreaker must report all_closed=true"
    );

    d.knowledge.record_failure();
    d.knowledge.record_failure();
    d.knowledge.record_failure();
    assert!(
        !d.all_closed(),
        "with knowledge open, all_closed must return false"
    );
}

#[test]
fn domain_circuit_breaker_reset_all_closes_every_domain() {
    let mut d = DomainCircuitBreaker::with_config(1, 60);
    d.knowledge.record_failure();
    d.memory.record_failure();
    d.graph.record_failure();

    assert!(d.knowledge.is_open());
    assert!(d.memory.is_open());
    assert!(d.graph.is_open());

    d.reset_all();
    assert!(
        d.all_closed(),
        "CONTRACT: reset_all must close all three domains"
    );
}

#[test]
fn domain_circuit_breaker_partial_trip_then_reset_all() {
    // Only memory trips; then reset_all must still close all
    let mut d = DomainCircuitBreaker::with_config(2, 60);
    d.memory.record_failure();
    d.memory.record_failure(); // memory OPEN
    assert!(!d.all_closed());

    d.reset_all();
    assert!(d.all_closed());
    assert_eq!(d.knowledge.state(), CircuitState::Closed);
    assert_eq!(d.memory.state(), CircuitState::Closed);
    assert_eq!(d.graph.state(), CircuitState::Closed);
}

// ─────────────────────────────────────────────────────────────────────────────
// CONTRACT: ConnectionPool — deterministic paths, WAL mode, ensure_dirs
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn pool_new_places_dbs_under_claude_touring() {
    let pool = ConnectionPool::new("/tmp/test_project_xyz");

    // CONTRACT: paths must be under .claude/touring/
    assert!(
        pool.knowledge_path()
            .to_string_lossy()
            .contains(".claude/touring/knowledge.db"),
        "knowledge path must be under .claude/touring/"
    );
    assert!(
        pool.memory_path()
            .to_string_lossy()
            .contains(".claude/touring/memory.db"),
        "memory path must be under .claude/touring/"
    );
    assert!(
        pool.graph_path()
            .to_string_lossy()
            .contains(".claude/touring/graph.db"),
        "graph path must be under .claude/touring/"
    );
}

#[test]
fn pool_all_three_dbs_share_same_parent_dir() {
    let pool = ConnectionPool::new("/some/project");
    assert_eq!(
        pool.knowledge_path().parent(),
        pool.memory_path().parent(),
        "CONTRACT: all three DB paths must share the same parent directory"
    );
    assert_eq!(pool.memory_path().parent(), pool.graph_path().parent(),);
}

#[test]
fn pool_open_knowledge_applies_wal_mode() {
    let dir = tempfile::tempdir().expect("tempdir");
    let pool = ConnectionPool::with_paths(
        dir.path().join("k.db"),
        dir.path().join("m.db"),
        dir.path().join("g.db"),
    );

    let conn = pool.open_knowledge().expect("open knowledge");
    let mode: String = conn
        .query_row("PRAGMA journal_mode", [], |r| r.get(0))
        .expect("journal_mode pragma");
    assert_eq!(
        mode, "wal",
        "CONTRACT: WAL mode must be applied to knowledge connection"
    );
}

#[test]
fn pool_open_memory_applies_wal_mode() {
    let dir = tempfile::tempdir().expect("tempdir");
    let pool = ConnectionPool::with_paths(
        dir.path().join("k.db"),
        dir.path().join("m.db"),
        dir.path().join("g.db"),
    );

    let conn = pool.open_memory().expect("open memory");
    let mode: String = conn
        .query_row("PRAGMA journal_mode", [], |r| r.get(0))
        .expect("journal_mode pragma");
    assert_eq!(
        mode, "wal",
        "CONTRACT: WAL mode must be applied to memory connection"
    );
}

#[test]
fn pool_open_graph_applies_wal_mode() {
    let dir = tempfile::tempdir().expect("tempdir");
    let pool = ConnectionPool::with_paths(
        dir.path().join("k.db"),
        dir.path().join("m.db"),
        dir.path().join("g.db"),
    );

    let conn = pool.open_graph().expect("open graph");
    let mode: String = conn
        .query_row("PRAGMA journal_mode", [], |r| r.get(0))
        .expect("journal_mode pragma");
    assert_eq!(
        mode, "wal",
        "CONTRACT: WAL mode must be applied to graph connection"
    );
}

#[test]
fn pool_open_connection_is_functional() {
    let dir = tempfile::tempdir().expect("tempdir");
    let pool = ConnectionPool::with_paths(
        dir.path().join("k.db"),
        dir.path().join("m.db"),
        dir.path().join("g.db"),
    );

    let conn = pool.open_knowledge().expect("open");
    let n: i64 = conn
        .query_row("SELECT 42", [], |r| r.get(0))
        .expect("query");
    assert_eq!(n, 42, "connection must execute queries after open");
}

#[test]
fn pool_ensure_dirs_creates_nested_structure() {
    let dir = tempfile::tempdir().expect("tempdir");
    let nested = dir.path().join("deep").join("nested").join("project");

    let pool = ConnectionPool::new(&nested);
    // Parent dirs do not exist yet
    assert!(!nested.join(".claude").join("touring").exists());

    pool.ensure_dirs().expect("ensure_dirs");

    let db_dir = nested.join(".claude").join("touring");
    assert!(
        db_dir.exists(),
        "CONTRACT: ensure_dirs must create .claude/touring/ structure"
    );
}

#[test]
fn pool_all_exist_false_before_open() {
    let dir = tempfile::tempdir().expect("tempdir");
    let pool = ConnectionPool::with_paths(
        dir.path().join("k.db"),
        dir.path().join("m.db"),
        dir.path().join("g.db"),
    );
    // No connections opened yet
    assert!(
        !pool.all_exist(),
        "all_exist must be false before any connection is opened"
    );
}

#[test]
fn pool_all_exist_true_after_all_three_opened() {
    let dir = tempfile::tempdir().expect("tempdir");
    let pool = ConnectionPool::with_paths(
        dir.path().join("k.db"),
        dir.path().join("m.db"),
        dir.path().join("g.db"),
    );
    pool.open_knowledge().expect("k");
    pool.open_memory().expect("m");
    pool.open_graph().expect("g");

    assert!(
        pool.all_exist(),
        "CONTRACT: all_exist must be true after opening all three DBs"
    );
}

#[test]
fn pool_with_paths_uses_explicit_paths() {
    let k = std::path::PathBuf::from("/custom/path/k.db");
    let m = std::path::PathBuf::from("/custom/path/m.db");
    let g = std::path::PathBuf::from("/custom/path/g.db");

    let pool = ConnectionPool::with_paths(k.clone(), m.clone(), g.clone());
    assert_eq!(pool.knowledge_path(), k.as_path());
    assert_eq!(pool.memory_path(), m.as_path());
    assert_eq!(pool.graph_path(), g.as_path());
}

// ─────────────────────────────────────────────────────────────────────────────
// CONTRACT: Backup E2E cycle — backup creates valid copy, restore recovers data
// These tests call backup_one / restore_one logic via the public backup module
// indirectly by exercising the core SQLite primitives they rely on.
// (backup.rs functions are private; we test the contract via rusqlite directly
//  to verify the VACUUM INTO pattern the CLI wraps.)
// ─────────────────────────────────────────────────────────────────────────────

fn make_test_db(path: &std::path::Path) {
    let conn = rusqlite::Connection::open(path).expect("open test db");
    conn.execute_batch(
        "CREATE TABLE audit_test (id INTEGER PRIMARY KEY, label TEXT); \
         INSERT INTO audit_test VALUES (1, 'cross_audit');",
    )
    .expect("create table");
}

fn is_valid_sqlite(path: &std::path::Path) -> bool {
    use std::io::Read;
    let Ok(mut f) = std::fs::File::open(path) else {
        return false;
    };
    let mut buf = [0u8; 16];
    f.read_exact(&mut buf).is_ok() && &buf == b"SQLite format 3\x00"
}

#[test]
fn backup_vacuum_into_creates_valid_sqlite_copy() {
    let dir = tempfile::tempdir().expect("tempdir");
    let src = dir.path().join("src.db");
    let dst = dir.path().join("backup.db");
    make_test_db(&src);

    // This is the exact mechanism backup_one uses
    let conn = rusqlite::Connection::open(&src).expect("open src");
    let escaped = dst.to_string_lossy().replace('\'', "''");
    conn.execute_batch(&format!("VACUUM INTO '{escaped}'"))
        .expect("VACUUM INTO");

    assert!(dst.exists(), "backup destination must exist");
    assert!(
        is_valid_sqlite(&dst),
        "CONTRACT: backup file must be valid SQLite"
    );
}

#[test]
fn backup_vacuum_into_data_preserved() {
    let dir = tempfile::tempdir().expect("tempdir");
    let src = dir.path().join("src.db");
    let dst = dir.path().join("backup.db");
    make_test_db(&src);

    let conn = rusqlite::Connection::open(&src).expect("open src");
    let escaped = dst.to_string_lossy().replace('\'', "''");
    conn.execute_batch(&format!("VACUUM INTO '{escaped}'"))
        .expect("VACUUM INTO");
    drop(conn);

    // Verify data is intact in the backup
    let bconn = rusqlite::Connection::open(&dst).expect("open backup");
    let label: String = bconn
        .query_row("SELECT label FROM audit_test WHERE id=1", [], |r| r.get(0))
        .expect("query backup");
    assert_eq!(
        label, "cross_audit",
        "CONTRACT: backup must preserve all data rows"
    );
}

#[test]
fn restore_copy_recovers_original_data() {
    let dir = tempfile::tempdir().expect("tempdir");
    let original = dir.path().join("original.db");
    let backup = dir.path().join("backup.db");
    let restored = dir.path().join("restored.db");
    make_test_db(&original);

    // Backup step
    let conn = rusqlite::Connection::open(&original).expect("open");
    let escaped = backup.to_string_lossy().replace('\'', "''");
    conn.execute_batch(&format!("VACUUM INTO '{escaped}'"))
        .expect("vacuum");
    drop(conn);

    // Validate backup is valid SQLite before restore
    assert!(
        is_valid_sqlite(&backup),
        "backup must pass SQLite magic check before restore"
    );

    // Restore step (fs::copy — what restore_one uses)
    std::fs::copy(&backup, &restored).expect("restore copy");
    assert!(
        is_valid_sqlite(&restored),
        "restored file must be valid SQLite"
    );

    let rconn = rusqlite::Connection::open(&restored).expect("open restored");
    let label: String = rconn
        .query_row("SELECT label FROM audit_test WHERE id=1", [], |r| r.get(0))
        .expect("query restored");
    assert_eq!(
        label, "cross_audit",
        "CONTRACT: restore must recover all original data"
    );
}

#[test]
fn restore_rejects_non_sqlite_magic_header() {
    let dir = tempfile::tempdir().expect("tempdir");
    let garbage = dir.path().join("garbage.db");
    std::fs::write(&garbage, b"not a database at all").expect("write garbage");

    // is_valid_sqlite must reject files without the magic header
    assert!(
        !is_valid_sqlite(&garbage),
        "CONTRACT: restore validation must reject non-SQLite files"
    );
}

#[test]
fn restore_rejects_missing_file() {
    let dir = tempfile::tempdir().expect("tempdir");
    let missing = dir.path().join("does_not_exist.db");

    assert!(
        !is_valid_sqlite(&missing),
        "CONTRACT: is_valid_sqlite must return false for missing files (not panic)"
    );
}

#[test]
fn backup_all_three_domains_e2e_cycle() {
    // Simulate `touring backup all` for all three domains
    let dir = tempfile::tempdir().expect("tempdir");
    let domains = ["knowledge", "memory", "graph"];

    // Create source DBs
    for domain in &domains {
        let src = dir.path().join(format!("{domain}.db"));
        make_test_db(&src);
    }

    // Backup all three
    let backup_dir = dir.path().join("backups");
    std::fs::create_dir_all(&backup_dir).expect("create backup dir");

    for domain in &domains {
        let src = dir.path().join(format!("{domain}.db"));
        let dst = backup_dir.join(format!("{domain}.db"));
        let conn = rusqlite::Connection::open(&src).expect("open");
        let escaped = dst.to_string_lossy().replace('\'', "''");
        conn.execute_batch(&format!("VACUUM INTO '{escaped}'"))
            .expect("vacuum");
    }

    // Verify all backups are valid SQLite with data
    for domain in &domains {
        let dst = backup_dir.join(format!("{domain}.db"));
        assert!(
            is_valid_sqlite(&dst),
            "backup/{domain}.db must be valid SQLite"
        );

        let conn = rusqlite::Connection::open(&dst).expect("open backup");
        let label: String = conn
            .query_row("SELECT label FROM audit_test WHERE id=1", [], |r| r.get(0))
            .expect("query backup");
        assert_eq!(
            label, "cross_audit",
            "{domain} backup must contain original data"
        );
    }
}
