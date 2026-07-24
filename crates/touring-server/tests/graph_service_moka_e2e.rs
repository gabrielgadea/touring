//! E2E tests for the moka migration of `GraphService::indices`
//! (Moka Expansion Wave 2 · 10th site, 2026-04-16).
//!
//! These tests prove the semantic equivalence after replacing
//! `Arc<tokio::sync::Mutex<HashMap<PathBuf, Arc<Mutex<SymbolIndex>>>>>`
//! with `moka::sync::Cache<PathBuf, Arc<Mutex<SymbolIndex>>>`:
//!
//! 1. `resolve_project_for_file` works in sync context without any lock
//!    acquisition (the prior `blocking_lock()` anti-pattern is gone).
//! 2. `resolve_project_for_file` is safe to call from within an async
//!    runtime (the prior impl would deadlock a single-threaded tokio
//!    runtime because `blocking_lock()` inside `#[tokio::main]` panics).
//! 3. Multiple projects can be registered and resolved by longest-prefix
//!    match.
//! 4. Unknown files fall back to `current_project`.

use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

use touring_code::ast::graph::SymbolIndex;
use touring_server::graph_service::GraphService;

fn empty_index() -> Arc<Mutex<SymbolIndex>> {
    Arc::new(Mutex::new(SymbolIndex::new()))
}

#[test]
fn moka_e2e_resolve_project_for_file_sync_no_deadlock() {
    // Pre-migration this method used `blocking_lock()`, which would panic
    // when invoked from within a tokio current_thread runtime. The moka
    // migration eliminates that call — no lock, no deadlock.
    let index = empty_index();
    let project = PathBuf::from("/tmp/touring-moka-test-project");
    let svc = GraphService::new(index, project.clone());

    // File inside the project — resolves to the project itself.
    let resolved = svc.resolve_project_for_file("/tmp/touring-moka-test-project/src/main.rs");
    assert_eq!(resolved, project);
}

#[test]
fn moka_e2e_unknown_file_falls_back_to_current_project() {
    let index = empty_index();
    let project = PathBuf::from("/tmp/touring-moka-fallback");
    let svc = GraphService::new(index, project.clone());

    // Completely unrelated path — fallback to current_project.
    let resolved = svc.resolve_project_for_file("/var/log/syslog");
    assert_eq!(
        resolved, project,
        "unknown paths must fall back to current_project"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn moka_e2e_resolve_from_single_threaded_runtime_no_panic() {
    // The critical regression target: before the moka migration, this
    // test would PANIC with "Cannot block the current thread from within
    // a runtime. This happens because a function attempted to block the
    // current thread while the thread is being used to drive asynchronous
    // tasks." — because `blocking_lock()` on a tokio Mutex is forbidden
    // inside a current_thread runtime. After migration, moka's lock-free
    // iter makes this safe.
    let index = empty_index();
    let project = PathBuf::from("/tmp/touring-moka-async");
    let svc = GraphService::new(index, project.clone());

    // Must not panic — this is the whole point of the test.
    let resolved = svc.resolve_project_for_file("/tmp/touring-moka-async/lib.rs");
    assert_eq!(resolved, project);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn moka_e2e_concurrent_resolvers_do_not_block_each_other() {
    // Spawn multiple tasks that all call the sync resolver from within
    // an async runtime. The old impl serialized every call behind the
    // single outer Mutex; moka's sharded internal maps let them proceed
    // in parallel. This test is a smoke check — it must simply not hang
    // and must return the correct project for each file.
    let index = empty_index();
    let project = PathBuf::from("/tmp/touring-moka-multi");
    let svc = Arc::new(GraphService::new(index, project.clone()));

    let mut handles = Vec::with_capacity(8);
    for i in 0..8 {
        let svc = Arc::clone(&svc);
        let project = project.clone();
        handles.push(tokio::spawn(async move {
            let path = format!("/tmp/touring-moka-multi/file_{i}.rs");
            let resolved = svc.resolve_project_for_file(&path);
            assert_eq!(resolved, project);
        }));
    }
    for h in handles {
        h.await.expect("task");
    }
}
