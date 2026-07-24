//! Wire `rusty-fork` for OS-level test isolation.
//!
//! Complements `serial_test` (which serialises in-process access) with
//! full sub-process isolation via `rusty_fork_test!{}`. A panic, abort,
//! or static-mut corruption inside a child process cannot leak into the
//! cargo-test harness or siblings.
//!
//! Use cases this file exercises:
//!
//! 1. **Sub-process smoke test** — cheap sanity that the fork path works
//!    on the host (Linux/macOS/Windows use different backends).
//! 2. **Static state isolation** — a `static AtomicUsize` mutated in the
//!    child does not survive back into the harness process.
//! 3. **SymbolStore in a fresh process** — the SQLite WAL path opens
//!    clean in a child: the file-knowledge store tolerates process-level
//!    access, a prerequisite for running multiple touring-daemon hook
//!    binaries side-by-side.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::pedantic)]

use std::sync::atomic::{AtomicUsize, Ordering};

use rusty_fork::rusty_fork_test;
use tempfile::TempDir;

use touring_code::ast::graph::SymbolLocation;
use touring_code::ast::store::{SymbolChangeSet, SymbolStore};

// A static the parent process will observe AS-IF untouched after each
// child test runs — because rusty_fork_test! runs the body in a fork.
static MUTATED_IN_CHILD: AtomicUsize = AtomicUsize::new(0);

rusty_fork_test! {
    #[test]
    fn sub_process_runs_and_exits_cleanly() {
        // Trivial: if this completes without panic, fork worked.
        let sum: u32 = (1..=10).sum();
        assert_eq!(sum, 55);
    }

    #[test]
    fn static_mutations_in_child_do_not_leak_back_to_parent() {
        // Bump the static to a value the parent must not observe.
        MUTATED_IN_CHILD.store(9999, Ordering::SeqCst);
        assert_eq!(MUTATED_IN_CHILD.load(Ordering::SeqCst), 9999);
        // When the child exits, the parent's copy of MUTATED_IN_CHILD
        // stays at its initial value (verified in
        // `parent_sees_fresh_static_between_forks` below).
    }

    #[test]
    fn symbol_store_opens_in_fresh_child_process() {
        let dir = TempDir::new().expect("tempdir in child");
        let store = SymbolStore::new(&dir.path().join("child.db"))
            .expect("open store in child");
        let changes = SymbolChangeSet {
            upsert: vec![SymbolLocation {
                file_path: "/child/foo.rs".into(),
                symbol_name: "ChildWidget".into(),
                line: 1,
                column: 0,
                is_definition: true,
                kind: None,
            }],
            remove: vec![],
            renames: vec![],
        };
        store.apply_change_set(&changes).expect("apply in child");
        // No direct read API in SymbolStore for count — the test's value
        // is proving that open + apply + drop all succeed inside a fork.
        drop(store);
    }
}

// This test runs in the HARNESS process (no rusty_fork_test wrapper) to
// verify the invariant: static mutations from prior forks never leak.
// It must stay in the same binary; cargo-test serializes binaries but
// the forks inside `rusty_fork_test!` cannot influence this observer.
#[test]
fn parent_sees_fresh_static_between_forks() {
    // Regardless of whether `static_mutations_in_child_do_not_leak_back_to_parent`
    // ran first, MUTATED_IN_CHILD must still read 0 here because that
    // mutation happened in a child process.
    let parent_view = MUTATED_IN_CHILD.load(Ordering::SeqCst);
    assert_eq!(
        parent_view, 0,
        "parent observed leaked child mutation: {parent_view}"
    );
}
