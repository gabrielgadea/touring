//! Input population — feeds REAL workspace symbol/use data into the salsa engine.
//!
//! # Why this module exists (the production-wiring blocker resolution)
//!
//! The salsa incremental engine ([`crate::salsa::queries::blast`]) is a real
//! `#[salsa::tracked]` computation graph, but until now it had *no production
//! consumer* — its inputs ([`FileText`], [`DefIndex`], [`UseGraph`]) were only
//! ever populated by synthetic test fixtures. This module is the seam that lets
//! a real data source (the AST `SymbolIndex` / `SymbolStore` in
//! `touring-code`) drive the engine.
//!
//! # Dependency-direction safety (no cycle)
//!
//! `touring-storage` is a *leaf* crate (it depends only on `touring-foundation`);
//! `touring-code` depends *on* `touring-storage`. If this module imported the
//! `SymbolIndex` / `SymbolLocation` / `DependencyEdge` types from `touring-code`
//! to read them directly, it would invert the dependency edge and create a
//! Cargo cycle (`touring-code -> touring-storage -> touring-code`).
//!
//! The resolution is **inversion of control**: this module accepts *plain*
//! owned structs ([`IngestDef`], [`IngestUse`], [`IngestGraph`]) that carry no
//! salsa types and no `touring-code` types. The caller (which already holds the
//! `SymbolIndex`) projects its data onto these plain structs and passes them
//! IN; [`populate_inputs`] then materializes the salsa inputs. `touring-storage`
//! stays a leaf, the engine gets real data, and there is no cycle.
//!
//! # Incrementality preserved
//!
//! The returned [`PopulatedInputs`] hands back the per-file [`FileText`] salsa
//! entities by path, so a caller can mutate one file's content
//! (`file.set_content(&mut db).to(..)`) and re-run `blast_radius_for_file`
//! to get demand-driven recomputation — exactly the property the synthetic
//! tests prove, now reachable from production-shaped data.

use crate::salsa::BlastRadiusResult;
use crate::salsa::db::{DatabaseImpl, FileText, ModuleDecl, SymbolDef, SymbolKind, SymbolUse};
use crate::salsa::queries::blast::{DefIndex, UseGraph, blast_radius_for_file};
use rustc_hash::FxHashMap;

/// Plain (salsa-free, `touring-code`-free) description of a symbol *definition*.
///
/// The caller builds these from whatever data source it owns (an AST
/// `SymbolIndex`, the SQLite `SymbolStore`, a `FileKnowledgeDB` row, …) without
/// `touring-storage` ever needing to depend on that source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IngestDef {
    /// Fully-qualified or local symbol name (must match `IngestUse::symbol_name`).
    pub name: String,
    /// Path of the file that *defines* this symbol.
    pub file_path: String,
    /// Symbol kind (defaults to [`SymbolKind::Fn`] when the source is untyped).
    pub kind: SymbolKind,
}

impl IngestDef {
    /// Convenience constructor; defaults `kind` to [`SymbolKind::Fn`].
    pub fn new(name: impl Into<String>, file_path: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            file_path: file_path.into(),
            kind: SymbolKind::Fn,
        }
    }

    /// Builder-style override of the symbol kind.
    pub fn with_kind(mut self, kind: SymbolKind) -> Self {
        self.kind = kind;
        self
    }
}

/// Plain description of a symbol *use* (the edge that powers blast radius).
///
/// `def_file_path` names the file that defines the used symbol; `use_file_path`
/// names the file that references it. An edge is only materialized when a
/// matching [`IngestDef`] exists for `(symbol_name, def_file_path)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IngestUse {
    /// Name of the symbol being used (matched against [`IngestDef::name`]).
    pub symbol_name: String,
    /// Path of the file that *defines* the used symbol.
    pub def_file_path: String,
    /// Path of the file that *references* the symbol.
    pub use_file_path: String,
    /// 1-indexed line of the reference (0 when unknown).
    pub line: u32,
}

impl IngestUse {
    /// Construct a use edge.
    pub fn new(
        symbol_name: impl Into<String>,
        def_file_path: impl Into<String>,
        use_file_path: impl Into<String>,
        line: u32,
    ) -> Self {
        Self {
            symbol_name: symbol_name.into(),
            def_file_path: def_file_path.into(),
            use_file_path: use_file_path.into(),
            line,
        }
    }
}

/// A complete, salsa-free snapshot of a workspace's def/use graph.
///
/// This is the single value a caller assembles from its own data source and
/// hands to [`populate_inputs`]. `files` carries `(path, content, version)`
/// tuples for every file referenced by any def or use; missing files are
/// synthesized with empty content so the graph is always well-formed.
#[derive(Debug, Clone, Default)]
pub struct IngestGraph {
    /// `(file_path, content, version)` for every file in the snapshot.
    pub files: Vec<(String, String, u64)>,
    /// All symbol definitions.
    pub defs: Vec<IngestDef>,
    /// All symbol uses (def→use edges).
    pub uses: Vec<IngestUse>,
}

impl IngestGraph {
    /// Create an empty graph.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a file with explicit content and version.
    pub fn add_file(
        &mut self,
        path: impl Into<String>,
        content: impl Into<String>,
        version: u64,
    ) -> &mut Self {
        self.files.push((path.into(), content.into(), version));
        self
    }

    /// Register a symbol definition.
    pub fn add_def(&mut self, def: IngestDef) -> &mut Self {
        self.defs.push(def);
        self
    }

    /// Register a symbol use (def→use edge).
    pub fn add_use(&mut self, use_edge: IngestUse) -> &mut Self {
        self.uses.push(use_edge);
        self
    }
}

/// Result of [`populate_inputs`]: the salsa inputs plus a path→[`FileText`] map.
///
/// `files_by_path` lets callers look up the salsa entity for a given path so
/// they can (a) run [`blast_for`] / `blast_radius_for_file` on it, and
/// (b) mutate it (`set_content` / `set_version`) to drive incremental recompute.
#[derive(Debug, Clone)]
pub struct PopulatedInputs {
    /// Path → the salsa [`FileText`] entity for that file.
    pub files_by_path: FxHashMap<String, FileText>,
    /// The salsa [`DefIndex`] input (all definitions).
    pub defs: DefIndex,
    /// The salsa [`UseGraph`] input (all use edges).
    pub use_graph: UseGraph,
}

impl PopulatedInputs {
    /// Look up the [`FileText`] salsa entity for a path, if present.
    pub fn file(&self, path: &str) -> Option<FileText> {
        self.files_by_path.get(path).copied()
    }
}

/// Populate salsa inputs from a plain [`IngestGraph`].
///
/// Builds, in order:
/// 1. one [`FileText`] per distinct path (from `graph.files`, with any
///    def/use-referenced-but-undeclared file synthesized as empty content);
/// 2. one [`ModuleDecl`] per defining file (named after the file path);
/// 3. one [`SymbolDef`] per [`IngestDef`];
/// 4. one [`SymbolUse`] per [`IngestUse`] whose `(symbol_name, def_file_path)`
///    resolves to a known definition (unresolved uses are skipped, mirroring
///    the tracked query's `continue`-on-miss behavior).
///
/// Returns [`PopulatedInputs`] holding the [`DefIndex`] / [`UseGraph`] salsa
/// inputs and the path→[`FileText`] map for downstream queries and mutation.
pub fn populate_inputs(db: &DatabaseImpl, graph: &IngestGraph) -> PopulatedInputs {
    // 1. FileText per distinct path. Declared files first (carry real content),
    //    then synthesize any path that only appears in defs/uses.
    let mut files_by_path: FxHashMap<String, FileText> = FxHashMap::default();
    for (path, content, version) in &graph.files {
        files_by_path
            .entry(path.clone())
            .or_insert_with(|| FileText::new(db, content.clone(), *version, path.clone()));
    }
    let ensure_file = |path: &str, files: &mut FxHashMap<String, FileText>| -> FileText {
        *files
            .entry(path.to_string())
            .or_insert_with(|| FileText::new(db, String::new(), 1, path.to_string()))
    };

    // 2. ModuleDecl per defining file (one per distinct def file_path).
    let mut module_by_file: FxHashMap<String, ModuleDecl> = FxHashMap::default();

    // 3. SymbolDef per IngestDef, keyed by (name, def_file_path) for use lookup.
    let mut def_by_key: FxHashMap<(String, String), SymbolDef> = FxHashMap::default();
    let mut defs: Vec<SymbolDef> = Vec::with_capacity(graph.defs.len());
    for d in &graph.defs {
        let file = ensure_file(&d.file_path, &mut files_by_path);
        let module = *module_by_file
            .entry(d.file_path.clone())
            .or_insert_with(|| ModuleDecl::new(db, d.file_path.clone(), file));
        let sym = SymbolDef::new(db, d.name.clone(), module, d.kind);
        def_by_key.insert((d.name.clone(), d.file_path.clone()), sym);
        defs.push(sym);
    }

    // 4. SymbolUse per resolvable IngestUse.
    let mut uses: Vec<SymbolUse> = Vec::with_capacity(graph.uses.len());
    for u in &graph.uses {
        let Some(&sym) = def_by_key.get(&(u.symbol_name.clone(), u.def_file_path.clone())) else {
            continue; // unresolved use — skip (matches tracked-query miss policy)
        };
        let use_file = ensure_file(&u.use_file_path, &mut files_by_path);
        uses.push(SymbolUse::new(db, sym, use_file, u.line));
    }

    let def_index = DefIndex::new(db, defs);
    let use_graph = UseGraph::new(db, uses);

    PopulatedInputs {
        files_by_path,
        defs: def_index,
        use_graph,
    }
}

/// End-to-end convenience: populate inputs from `graph`, then compute the blast
/// radius for `start_file`. Returns `None` if `start_file` is not in the graph.
///
/// This is the single call a production consumer makes; the heavy reverse-index
/// build is memoized inside salsa, so repeated calls (or calls after an
/// unrelated file's body edit) are served from the memo.
pub fn blast_for(
    db: &DatabaseImpl,
    inputs: &PopulatedInputs,
    start_file: &str,
) -> Option<BlastRadiusResult> {
    let file = inputs.file(start_file)?;
    Some(blast_radius_for_file(
        db,
        file,
        inputs.defs,
        inputs.use_graph,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use salsa::Setter;

    /// Build a production-shaped graph from data that *looks like* SymbolStore
    /// rows: a.rs defines `foo`; b.rs uses `foo`; c.rs uses `bar` (defined in
    /// b.rs). This mirrors the `(name, file_path, is_definition)` symbols table
    /// plus the `(from, to, symbols)` dependency edges, but assembled as the plain
    /// inversion-of-control structs.
    fn realistic_graph() -> IngestGraph {
        let mut g = IngestGraph::new();
        g.add_file("crates/a/src/lib.rs", "pub fn foo() {}", 1)
            .add_file(
                "crates/b/src/lib.rs",
                "use a::foo; pub fn bar() { foo() }",
                1,
            )
            .add_file("crates/c/src/lib.rs", "use b::bar; fn baz() { bar() }", 1);
        g.add_def(IngestDef::new("foo", "crates/a/src/lib.rs"))
            .add_def(IngestDef::new("bar", "crates/b/src/lib.rs"));
        g.add_use(IngestUse::new(
            "foo",
            "crates/a/src/lib.rs",
            "crates/b/src/lib.rs",
            1,
        ))
        .add_use(IngestUse::new(
            "bar",
            "crates/b/src/lib.rs",
            "crates/c/src/lib.rs",
            1,
        ));
        g
    }

    #[test]
    fn populate_builds_all_inputs() {
        let db = DatabaseImpl::new();
        let inputs = populate_inputs(&db, &realistic_graph());
        assert_eq!(inputs.files_by_path.len(), 3, "three distinct files");
        assert_eq!(inputs.defs.defs(&db).len(), 2, "two definitions");
        assert_eq!(
            inputs.use_graph.uses(&db).len(),
            2,
            "two resolved use edges"
        );
    }

    #[test]
    fn unresolved_use_is_skipped() {
        let db = DatabaseImpl::new();
        let mut g = IngestGraph::new();
        g.add_file("a.rs", "fn foo() {}", 1).add_file("b.rs", "", 1);
        g.add_def(IngestDef::new("foo", "a.rs"));
        // `nonexistent` has no matching def ⇒ edge must be dropped.
        g.add_use(IngestUse::new("nonexistent", "a.rs", "b.rs", 1));
        g.add_use(IngestUse::new("foo", "a.rs", "b.rs", 2));
        let inputs = populate_inputs(&db, &g);
        assert_eq!(
            inputs.use_graph.uses(&db).len(),
            1,
            "only the resolvable use edge is materialized"
        );
    }

    #[test]
    fn blast_for_real_data_is_transitive() {
        let db = DatabaseImpl::new();
        let inputs = populate_inputs(&db, &realistic_graph());
        let result =
            blast_for(&db, &inputs, "crates/a/src/lib.rs").expect("start file is in the graph");
        // a -> b (direct), b -> c (transitive) ⇒ 1 direct, 2 total.
        assert_eq!(result.direct_deps.len(), 1, "one direct consumer (b)");
        assert_eq!(result.dep_count, 2, "two transitive consumers (b and c)");
        assert_eq!(result.max_depth, 2, "depth 2: a -> b -> c");
    }

    #[test]
    fn blast_for_unknown_file_is_none() {
        let db = DatabaseImpl::new();
        let inputs = populate_inputs(&db, &realistic_graph());
        assert!(blast_for(&db, &inputs, "does/not/exist.rs").is_none());
    }

    /// THE production-shaped incremental proof: populate from realistic data,
    /// run blast, mutate ONE file's content, and assert the dependent query
    /// recomputes while an unrelated unread field stays memoized — proving
    /// demand-driven incrementality works end-to-end on production-shaped data
    /// (not just the synthetic fixtures already covered in `queries::blast`).
    #[test]
    fn incremental_recompute_on_real_data() {
        let mut db = DatabaseImpl::new();
        let inputs = populate_inputs(&db, &realistic_graph());
        let a = inputs.file("crates/a/src/lib.rs").unwrap();

        // Cold run executes.
        db.events().reset();
        let r1 = blast_radius_for_file(&db, a, inputs.defs, inputs.use_graph);
        assert_eq!(r1.dep_count, 2);
        assert!(db.events().executes() > 0, "cold run must execute");

        // Re-run with no change ⇒ fully memoized.
        db.events().reset();
        let r2 = blast_radius_for_file(&db, a, inputs.defs, inputs.use_graph);
        assert_eq!(r2.dep_count, 2);
        assert_eq!(
            db.events().executes(),
            0,
            "unchanged inputs must be served from the memo"
        );

        // Mutate a's CONTENT (real production event: file body edited). Content
        // is not read by the blast query, so the dependent query must stay
        // memoized — the firmest proof of salsa precision on real data.
        db.events().reset();
        a.set_content(&mut db)
            .to("pub fn foo() { /* edited */ }".into());
        let r3 = blast_radius_for_file(&db, a, inputs.defs, inputs.use_graph);
        assert_eq!(r3.dep_count, 2, "graph shape unchanged after body edit");
        assert_eq!(
            db.events().executes(),
            0,
            "editing unread content must NOT recompute the blast query"
        );

        // Now mutate a READ dependency (the path, consumed by file_key_for) ⇒
        // recompute is forced, proving the dependent query DOES recompute when
        // a real read-dependency changes.
        db.events().reset();
        a.set_path(&mut db).to("crates/a/src/renamed.rs".into());
        let r4 = blast_radius_for_file(&db, a, inputs.defs, inputs.use_graph);
        assert_eq!(r4.dep_count, 2, "shape unchanged, key re-derived");
        assert_ne!(r4.file_id, r1.file_id, "renamed path ⇒ different FileKey");
        assert!(
            db.events().executes() > 0,
            "changed read-dependency must force a recompute"
        );
    }
}
