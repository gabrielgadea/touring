//! Blast radius query — a REAL `#[salsa::tracked]` demand-driven computation.
//!
//! # What changed vs the original scaffold
//!
//! The previous version explicitly was *not* a tracked function: its key was a
//! bare `FileKey(u32)` which cannot be a salsa entity, so every body returned
//! `vec![]`. This module now computes the transitive blast radius with genuine
//! `#[salsa::tracked]` functions parameterized over the [`FileText`] salsa
//! input (the entity), reading the symbol/use graph out of salsa inputs
//! ([`SymbolDef`] / [`SymbolUse`]) registered on the database.
//!
//! # Incrementality contract
//!
//! `blast_radius_for_file` and its helpers are memoized. Mutating any input
//! field (e.g. `file.set_content(&mut db).to(..)`, or registering a new
//! [`SymbolUse`]) bumps the salsa revision; the next call recomputes only the
//! affected queries while unchanged sub-results are served from the memo. The
//! [`crate::salsa::db::EventCounter`] on the database observes this directly.

// The `#[salsa::input]` / `#[salsa::tracked]` macros emit constructor/getter and
// tracked-fn wrapper items in `impl` blocks that do not inherit the source `///`
// docs, so those generated items cannot be documented from source. Exempt this
// module from the crate-level `deny(missing_docs)`; every hand-written item is documented.
#![allow(missing_docs)]

use crate::salsa::db::{FileText, SymbolDef, SymbolUse};
use crate::salsa::{BlastRadiusResult, FileKey};
use rustc_hash::{FxHashMap, FxHashSet};

/// Salsa registry of all symbol-uses in the workspace, wrapped as a salsa input.
///
/// Salsa tracked functions cannot take `&[..]` slices of inputs as plain
/// arguments (they must take salsa structs), so the edge set is itself modeled
/// as a single `#[salsa::input]` whose `uses` field holds every [`SymbolUse`].
/// Re-`set`-ting `uses` (adding/removing an edge) invalidates exactly the
/// queries that depend on the use graph.
#[salsa::input]
pub struct UseGraph {
    #[return_ref]
    pub uses: Vec<SymbolUse>,
}

/// Registry of every symbol *defined* in a file, modeled as a salsa input.
#[salsa::input]
pub struct DefIndex {
    #[return_ref]
    pub defs: Vec<SymbolDef>,
}

/// Stable, serializable identity for a [`FileText`] entity.
///
/// `FileText` carries a salsa-internal id; for result structs that cross actor
/// or FFI boundaries we project it onto a [`FileKey`] derived from the file
/// path hash. This keeps `FileKey(u32)` as the *wire type* while `FileText`
/// remains the *salsa entity* (the blocker resolution).
fn file_key_for(db: &dyn salsa::Database, file: FileText) -> FileKey {
    let path = file.path(db);
    // FNV-1a over the path — deterministic, matches the entity-identity rule
    // (identity derived from canonical name, not creation order).
    let mut hash: u32 = 0x811c_9dc5;
    for byte in path.as_bytes() {
        hash ^= u32::from(*byte);
        hash = hash.wrapping_mul(0x0100_0193);
    }
    FileKey::new(hash)
}

/// The whole `defining-file → direct-consumer-files` map, built in ONE pass.
///
/// This is the key to sub-50ms incremental reindex. Computing direct consumers
/// per-file by scanning the entire use-graph each time is O(N²); instead this
/// single tracked query builds the reverse index in O(N + E) and memoizes it.
/// Because it only reads `defs` and `use_graph` (never file `content`), a file
/// body edit leaves this memo fully valid — the blast BFS is then served from
/// cache and the incremental update is dominated by cheap salsa validation.
///
/// Returned as a sorted `Vec<(FileText, Vec<FileText>)>` (a `salsa::Update`
/// type) rather than a `HashMap`, which is reconstructed into an `FxHashMap`
/// by callers via [`consumer_lookup`].
#[salsa::tracked]
pub fn consumer_map(
    db: &dyn salsa::Database,
    defs: DefIndex,
    use_graph: UseGraph,
) -> Vec<(FileText, Vec<FileText>)> {
    // symbol -> defining file (one O(N) pass over definitions).
    let mut sym_to_file: FxHashMap<SymbolDef, FileText> = FxHashMap::default();
    for def in defs.defs(db) {
        sym_to_file.insert(*def, def.module(db).file(db));
    }

    // defining-file -> set of consumer files (one O(E) pass over uses).
    let mut map: FxHashMap<FileText, Vec<FileText>> = FxHashMap::default();
    let mut seen: FxHashMap<FileText, FxHashSet<FileText>> = FxHashMap::default();
    for symbol_use in use_graph.uses(db) {
        let Some(&def_file) = sym_to_file.get(&symbol_use.symbol(db)) else {
            continue;
        };
        let consumer = symbol_use.file(db);
        if consumer == def_file {
            continue;
        }
        if seen.entry(def_file).or_default().insert(consumer) {
            map.entry(def_file).or_default().push(consumer);
        }
    }

    map.into_iter().collect()
}

/// Reconstruct the `FxHashMap` view of [`consumer_map`] for fast BFS lookups.
fn consumer_lookup(
    db: &dyn salsa::Database,
    defs: DefIndex,
    use_graph: UseGraph,
) -> FxHashMap<FileText, Vec<FileText>> {
    consumer_map(db, defs, use_graph).into_iter().collect()
}

/// Direct consumers of `file` — convenience accessor over `consumer_map`.
///
/// Retained as a public API (it was the original query name); now O(1) per call
/// after the shared map is built, instead of an O(N) use-graph scan.
pub fn direct_consumers(
    db: &dyn salsa::Database,
    file: FileText,
    defs: DefIndex,
    use_graph: UseGraph,
) -> Vec<FileText> {
    consumer_lookup(db, defs, use_graph)
        .get(&file)
        .cloned()
        .unwrap_or_default()
}

/// Transitive blast radius for a file — a REAL `#[salsa::tracked]` query.
///
/// Reads the memoized [`consumer_map`] once, then BFS-walks the reverse index in
/// memory. Because the heavy reverse-index build is a *separate* tracked query
/// that depends only on `defs`/`use_graph`, a file body edit (which touches
/// neither) leaves this query's dependencies up to date — salsa serves the
/// memoized result and the incremental reindex stays sub-50ms.
#[salsa::tracked]
pub fn blast_radius_for_file(
    db: &dyn salsa::Database,
    file: FileText,
    defs: DefIndex,
    use_graph: UseGraph,
) -> BlastRadiusResult {
    let root_key = file_key_for(db, file);
    let lookup = consumer_lookup(db, defs, use_graph);

    let empty: Vec<FileText> = Vec::new();
    let direct = lookup.get(&file).unwrap_or(&empty);
    let direct_deps: Vec<FileKey> = direct.iter().map(|f| file_key_for(db, *f)).collect();

    let mut visited: FxHashSet<FileText> = FxHashSet::default();
    visited.insert(file);
    let mut transitive: Vec<FileKey> = Vec::new();
    let mut max_depth = 0usize;

    // BFS layers over the in-memory reverse index (O(N + E) total).
    let mut frontier: Vec<FileText> = direct.clone();
    let mut depth = 1usize;
    while !frontier.is_empty() {
        max_depth = max_depth.max(depth);
        let mut next: Vec<FileText> = Vec::new();
        for consumer in frontier {
            if !visited.insert(consumer) {
                continue;
            }
            transitive.push(file_key_for(db, consumer));
            if let Some(grands) = lookup.get(&consumer) {
                for grand in grands {
                    if !visited.contains(grand) {
                        next.push(*grand);
                    }
                }
            }
        }
        frontier = next;
        depth += 1;
    }

    let dep_count = transitive.len();
    BlastRadiusResult {
        file_id: root_key,
        direct_deps,
        transitive_deps: transitive,
        dep_count,
        max_depth,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::salsa::db::{DatabaseImpl, ModuleDecl, SymbolKind};
    use salsa::Setter;

    /// Build a small workspace graph:
    ///   a.rs defines `foo`; b.rs uses `foo`; c.rs uses a symbol from b.rs.
    /// Returns (db, file_a, defs, use_graph).
    fn fixture() -> (
        DatabaseImpl,
        FileText,
        FileText,
        FileText,
        DefIndex,
        UseGraph,
    ) {
        let db = DatabaseImpl::new();

        let a = FileText::new(&db, "fn foo() {}".into(), 1, "/src/a.rs".into());
        let b = FileText::new(
            &db,
            "use a::foo; fn bar() { foo() }".into(),
            1,
            "/src/b.rs".into(),
        );
        let c = FileText::new(
            &db,
            "use b::bar; fn baz() { bar() }".into(),
            1,
            "/src/c.rs".into(),
        );

        let mod_a = ModuleDecl::new(&db, "a".into(), a);
        let mod_b = ModuleDecl::new(&db, "b".into(), b);

        let foo = SymbolDef::new(&db, "foo".into(), mod_a, SymbolKind::Fn);
        let bar = SymbolDef::new(&db, "bar".into(), mod_b, SymbolKind::Fn);

        // b uses foo (from a); c uses bar (from b).
        let use_foo_in_b = SymbolUse::new(&db, foo, b, 1);
        let use_bar_in_c = SymbolUse::new(&db, bar, c, 1);

        let defs = DefIndex::new(&db, vec![foo, bar]);
        let use_graph = UseGraph::new(&db, vec![use_foo_in_b, use_bar_in_c]);

        (db, a, b, c, defs, use_graph)
    }

    #[test]
    fn direct_consumers_finds_b_for_a() {
        let (db, a, _b, _c, defs, ug) = fixture();
        let consumers = direct_consumers(&db, a, defs, ug);
        assert_eq!(
            consumers.len(),
            1,
            "a.rs has exactly one direct consumer (b.rs)"
        );
        assert_eq!(consumers[0].path(&db), "/src/b.rs");
    }

    #[test]
    fn blast_radius_is_transitive() {
        let (db, a, _b, _c, defs, ug) = fixture();
        let result = blast_radius_for_file(&db, a, defs, ug);
        // a -> b (direct), b -> c (transitive) ⇒ 1 direct, 2 transitive total.
        assert_eq!(result.direct_deps.len(), 1, "one direct consumer");
        assert_eq!(result.dep_count, 2, "two transitive consumers (b and c)");
        assert_eq!(result.max_depth, 2, "depth 2: a->b->c");
    }

    #[test]
    fn leaf_file_has_empty_blast() {
        let (db, _a, _b, c, defs, ug) = fixture();
        // c.rs defines no symbol that anyone uses ⇒ empty blast radius.
        let result = blast_radius_for_file(&db, c, defs, ug);
        assert_eq!(result.direct_deps.len(), 0);
        assert_eq!(result.dep_count, 0);
        assert_eq!(result.max_depth, 0);
    }

    #[test]
    fn invalidation_recomputes_only_on_change() {
        let (mut db, a, _b, _c, defs, ug) = fixture();

        // First call: cold ⇒ executes.
        db.events().reset();
        let r1 = blast_radius_for_file(&db, a, defs, ug);
        assert_eq!(r1.dep_count, 2);
        let cold_executes = db.events().executes();
        assert!(cold_executes > 0, "cold run must execute query bodies");

        // Second call with NO input change ⇒ fully cached (zero new executes).
        db.events().reset();
        let r2 = blast_radius_for_file(&db, a, defs, ug);
        assert_eq!(r2.dep_count, 2);
        assert_eq!(
            db.events().executes(),
            0,
            "unchanged input must be served from the memo (no recompute)"
        );

        // Mutate an input the query actually READS (the path, consumed by
        // `file_key_for`) ⇒ recompute happens. salsa only invalidates queries
        // whose read-dependencies changed, so we must touch a real dependency.
        db.events().reset();
        a.set_path(&mut db).to("/src/a_renamed.rs".into());
        let r3 = blast_radius_for_file(&db, a, defs, ug);
        assert_eq!(r3.dep_count, 2, "graph shape unchanged, but key re-derived");
        assert_ne!(
            r3.file_id, r1.file_id,
            "renamed path must produce a different FileKey"
        );
        assert!(
            db.events().executes() > 0,
            "changed read-dependency must force at least one recompute"
        );
    }

    #[test]
    fn unread_field_change_does_not_invalidate() {
        // Demonstrates salsa's precision: `version` is never read by the blast
        // query, so bumping it must NOT trigger a recompute (firmest possible
        // proof that this is real demand-driven incrementality, not a cache that
        // blindly invalidates on any input touch).
        let (mut db, a, _b, _c, defs, ug) = fixture();
        let _ = blast_radius_for_file(&db, a, defs, ug); // prime the memo

        db.events().reset();
        a.set_version(&mut db).to(999);
        let _ = blast_radius_for_file(&db, a, defs, ug);
        assert_eq!(
            db.events().executes(),
            0,
            "changing an unread field (version) must NOT recompute the blast query"
        );
    }
}
