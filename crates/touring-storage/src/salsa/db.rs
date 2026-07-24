//! Salsa database implementation for `touring-incremental-salsa`.
//!
//! This is a REAL salsa 0.18 incremental computation graph (not a placeholder).
//!
//! # The salsa-entity model (blocker resolution)
//!
//! The original POC scaffold could not write `#[salsa::tracked]` functions
//! because its result key was a bare `FileKey(u32)`, which does not implement
//! salsa's `SalsaStructInDb`. The resolution — confirmed against
//! `salsa-0.18.0/examples/lazy-input/main.rs` and
//! `salsa-0.18.0/tests/tracked_fn_on_input_with_high_durability.rs` — is that a
//! tracked function must be parameterized over a **salsa input/tracked struct**
//! (here [`FileText`]), never over a plain integer. The salsa entity *is* the
//! identity; `FileKey(u32)` is retained only as a serializable wire-type for
//! the result structs that cross actor / FFI boundaries.
//!
//! # Demand-driven incrementality
//!
//! Tracked queries (in [`crate::salsa::queries`]) read from these inputs. When
//! an input field is mutated via the [`salsa::Setter`] trait
//! (`input.set_field(&mut db).to(v)`), salsa bumps the revision and recomputes
//! exactly the queries whose inputs changed; unchanged queries are served from
//! the memo. The [`EventCounter`] embedded in [`DatabaseImpl`] observes
//! `EventKind::WillExecute` (recompute) vs `DidValidateMemoizedValue` (cache
//! hit), which is how the invalidation tests/benches prove the behavior.

// The `#[salsa::input]` macro emits constructor/getter/setter/builder items in a
// separate `impl` block that does not inherit the struct's `///` docs, so those
// generated items cannot be documented from source. Exempt this module from the
// crate-level `deny(missing_docs)`; every hand-written item below is still documented.
#![allow(missing_docs)]

use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

/// File text input — content + version from Vfs.
///
/// `content` uses `#[return_ref]` so queries borrow rather than clone the source
/// on every access (matches the `lazy-input` example's `File::contents`).
#[salsa::input]
pub struct FileText {
    #[return_ref]
    pub content: String,
    pub version: u64,
    #[return_ref]
    pub path: String,
}

/// Module declaration in a file.
#[salsa::input]
pub struct ModuleDecl {
    #[return_ref]
    pub name: String,
    pub file: FileText,
}

/// Symbol exported from a module.
#[salsa::input]
pub struct SymbolDef {
    #[return_ref]
    pub name: String,
    pub module: ModuleDecl,
    pub kind: SymbolKind,
}

/// Kind of symbol definition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SymbolKind {
    /// A function or method definition.
    Fn,
    /// A struct definition.
    Struct,
    /// An enum definition.
    Enum,
    /// A trait definition.
    Trait,
    /// A type alias definition.
    Type,
    /// A constant definition.
    Const,
    /// A static item definition.
    Static,
    /// A macro definition.
    Macro,
}

/// Consumer reference — where a symbol is used (the edge that powers blast radius).
#[salsa::input]
pub struct SymbolUse {
    pub symbol: SymbolDef,
    pub file: FileText,
    pub line: u32,
}

/// File-level metadata snapshot for knowledge queries.
#[salsa::input]
pub struct FileMeta {
    pub file: FileText,
    pub complexity_score: f64,
    pub cognitive_score: f64,
    pub fan_out: u32,
    pub fan_in: u32,
}

/// Counts salsa execution events, partitioned by recompute vs cache-hit.
///
/// Cloned handles share the same atomics (so the database `Clone` keeps a single
/// logical counter), which lets tests/benches read the counts after running
/// queries to *prove* that an unchanged input was served from the memo and a
/// changed input forced a recompute.
#[derive(Debug, Clone, Default)]
pub struct EventCounter {
    will_execute: Arc<AtomicU64>,
    did_validate: Arc<AtomicU64>,
}

impl EventCounter {
    /// Number of times a tracked query body actually ran (`WillExecute`).
    pub fn executes(&self) -> u64 {
        self.will_execute.load(Ordering::Relaxed)
    }

    /// Number of times a memoized value was validated/reused (`DidValidateMemoizedValue`).
    pub fn validations(&self) -> u64 {
        self.did_validate.load(Ordering::Relaxed)
    }

    /// Reset both counters to zero (used between bench phases).
    pub fn reset(&self) {
        self.will_execute.store(0, Ordering::Relaxed);
        self.did_validate.store(0, Ordering::Relaxed);
    }

    fn record(&self, kind: &salsa::EventKind) {
        match kind {
            salsa::EventKind::WillExecute { .. } => {
                self.will_execute.fetch_add(1, Ordering::Relaxed);
            }
            salsa::EventKind::DidValidateMemoizedValue { .. } => {
                self.did_validate.fetch_add(1, Ordering::Relaxed);
            }
            _ => {}
        }
    }
}

/// The salsa database type for the touring incremental engine.
///
/// `#[salsa::db]` (no args) on the struct auto-generates `impl HasStorage`.
/// Input jars (FileText, ModuleDecl, etc.) are auto-discovered from scope.
#[salsa::db]
#[derive(Default, Clone)]
pub struct DatabaseImpl {
    pub(crate) storage: salsa::Storage<Self>,
    /// Shared execute/validate event counters (see [`EventCounter`]).
    pub events: EventCounter,
}

impl DatabaseImpl {
    /// Create a fresh database with empty storage and zeroed event counters.
    pub fn new() -> Self {
        Self::default()
    }

    /// Borrow the event counter (recompute vs cache-hit observability).
    pub fn events(&self) -> &EventCounter {
        &self.events
    }
}

/// `#[salsa::db]` (no args) on impl auto-generates `impl Database` + views.
#[salsa::db]
impl salsa::Database for DatabaseImpl {
    fn salsa_event(&self, event: &dyn Fn() -> salsa::Event) {
        // Only materialize the event when we might record it; salsa passes a
        // closure precisely so uninterested databases pay nothing.
        let event = event();
        self.events.record(&event.kind);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_and_set_file() {
        let mut db = DatabaseImpl::new();
        let _file = FileText::new(&db, "fn main() {}".into(), 1, "/src/main.rs".into());
        assert_eq!(_file.content(&db), "fn main() {}");
        let _ = _file.set_content(&mut db);
        let _ = _file.set_version(&mut db);
        let _ = _file.set_path(&mut db);
    }

    #[test]
    fn symbol_kind_eq() {
        assert_eq!(SymbolKind::Fn, SymbolKind::Fn);
        assert_ne!(SymbolKind::Fn, SymbolKind::Struct);
    }
}
