//! Wire mockall to the `SymbolChangeObserver` trait.
//!
//! Fills the gap flagged in `skills/Touring/SKILL.md` ("mockall — not yet
//! wired; available for unit isolation"). The `mock!{}` macro builds a mock
//! implementation of `SymbolChangeObserver` without touching the trait
//! definition itself — mockall stays a dev-dep, production code is
//! unaffected.
//!
//! These tests isolate the observer-notification contract of
//! `SymbolStore::apply_change_set` from the underlying SQLite write path:
//! the store still writes to a real tempdir DB, but the observer side is
//! fully controlled, letting us assert call count, argument predicates,
//! and multi-observer fan-out deterministically.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::pedantic,
    clippy::indexing_slicing
)]

use std::sync::Arc;

use mockall::mock;
use mockall::predicate::always;
use tempfile::TempDir;

use touring_code::ast::graph::SymbolLocation;
use touring_code::ast::store::{SymbolChangeObserver, SymbolChangeSet, SymbolStore};

mock! {
    pub Observer {}
    impl SymbolChangeObserver for Observer {
        fn on_symbol_change(&self, changes: &SymbolChangeSet);
    }
}

// ── Fixtures ────────────────────────────────────────────────────────────

fn temp_store() -> (TempDir, SymbolStore) {
    let dir = TempDir::new().expect("tempdir");
    let store = SymbolStore::new(&dir.path().join("symbols.db")).expect("open store");
    (dir, store)
}

fn sample_location(name: &str, file: &str) -> SymbolLocation {
    SymbolLocation {
        file_path: file.to_string(),
        symbol_name: name.to_string(),
        line: 1,
        column: 0,
        is_definition: true,
        kind: None,
    }
}

fn upsert_changes(name: &str, file: &str) -> SymbolChangeSet {
    SymbolChangeSet {
        upsert: vec![sample_location(name, file)],
        remove: vec![],
        renames: vec![],
    }
}

fn remove_changes(name: &str, file: &str) -> SymbolChangeSet {
    SymbolChangeSet {
        upsert: vec![],
        remove: vec![(name.to_string(), file.to_string(), 1)],
        renames: vec![],
    }
}

// ── Tests ───────────────────────────────────────────────────────────────

#[test]
fn upsert_triggers_observer_exactly_once() {
    let (_guard, mut store) = temp_store();

    let mut mock = MockObserver::new();
    mock.expect_on_symbol_change()
        .with(always())
        .times(1)
        .returning(|_changes| ());

    store.subscribe(Arc::new(mock));
    store
        .apply_change_set(&upsert_changes("Foo", "/tmp/foo.rs"))
        .expect("apply");
    // `MockObserver::drop` validates expect_on_symbol_change.times(1).
}

#[test]
fn remove_triggers_observer_exactly_once() {
    let (_guard, mut store) = temp_store();

    // Seed BEFORE subscribing so the observer only sees the remove.
    // `subscribe` takes effect for subsequent `apply_change_set` calls only.
    store
        .apply_change_set(&upsert_changes("Bar", "/tmp/bar.rs"))
        .expect("seed upsert");

    let mut mock = MockObserver::new();
    mock.expect_on_symbol_change().times(1).returning(|_c| ());

    store.subscribe(Arc::new(mock));
    store
        .apply_change_set(&remove_changes("Bar", "/tmp/bar.rs"))
        .expect("remove");
}

#[test]
fn empty_changeset_does_not_notify() {
    let (_guard, mut store) = temp_store();

    let mut mock = MockObserver::new();
    // never() is the anti-upsert contract for is_empty() short-circuit.
    mock.expect_on_symbol_change().never();

    store.subscribe(Arc::new(mock));
    let empty = SymbolChangeSet {
        upsert: vec![],
        remove: vec![],
        renames: vec![],
    };
    store.apply_change_set(&empty).expect("apply empty");
}

#[test]
fn observer_receives_upserted_symbol_name() {
    let (_guard, mut store) = temp_store();

    let mut mock = MockObserver::new();
    mock.expect_on_symbol_change()
        .withf(|changes: &SymbolChangeSet| {
            changes.upsert.len() == 1 && changes.upsert[0].symbol_name == "Widget"
        })
        .times(1)
        .returning(|_c| ());

    store.subscribe(Arc::new(mock));
    store
        .apply_change_set(&upsert_changes("Widget", "/tmp/widget.rs"))
        .expect("apply");
}

#[test]
fn multiple_observers_fan_out() {
    let (_guard, mut store) = temp_store();

    let mut a = MockObserver::new();
    a.expect_on_symbol_change().times(1).returning(|_c| ());

    let mut b = MockObserver::new();
    b.expect_on_symbol_change().times(1).returning(|_c| ());

    store.subscribe(Arc::new(a));
    store.subscribe(Arc::new(b));
    store
        .apply_change_set(&upsert_changes("Gadget", "/tmp/gadget.rs"))
        .expect("apply");
    // Both observers validated on Drop.
}

#[test]
fn observer_chain_over_multiple_apply_calls() {
    let (_guard, mut store) = temp_store();

    let mut mock = MockObserver::new();
    mock.expect_on_symbol_change().times(3).returning(|_c| ());

    store.subscribe(Arc::new(mock));
    store
        .apply_change_set(&upsert_changes("A", "/tmp/a.rs"))
        .unwrap();
    store
        .apply_change_set(&upsert_changes("B", "/tmp/b.rs"))
        .unwrap();
    store
        .apply_change_set(&upsert_changes("C", "/tmp/c.rs"))
        .unwrap();
}
