//! AST Graph - Dependency analysis and cross-file symbol indexing
//!
//! Provides: import extraction, dependency graph, blast radius analysis
//!
//! # Key grammar
//!
//! The index, and the SQLite store that persists it (`ast::store`), speak about two
//! kinds of key. Every map and every column below says which one it holds:
//!
//! | key | what it is | where it lives |
//! |---|---|---|
//! | **file key** | the path as the caller indexed it (`pkg/utils.py`) | `SymbolLocation::file_path`, `DependencyEdge::from`, the keys of `file_to_symbols` and `dependencies`; SQL `symbols.file_path`, `dependencies.from_file`; and, for a RESOLVED edge, `DependencyEdge::to` and the keys of `reverse_deps` |
//! | **module key** | the text an import names (`utils`, `crate::a::b`) | `DependencyEdge::to` and the keys of `reverse_deps` for an edge extraction could not resolve; SQL `dependencies.to_file` |
//!
//! Resolved edges come from the wiring (`SymbolIndex::load_file_edges`, `ast::store::load_index`):
//! it already maps `crate::foo` to `foo.rs`, which extraction cannot. `SymbolIndex::importers_of`
//! asks the file key first. Only when that bucket is missing does it fall back on
//! `module_key(file_path)`, the file's stem, and then only when the file is the ONE indexed file
//! with that stem. The stem is a heuristic, documented rather than redesigned: an import that
//! spells a module path (Rust `crate::a::b`) never matches the stem `b`.
//! `SymbolStore::get_reverse_deps` still matches the stem without the uniqueness rule; its
//! only consumer is slated for the wiring.
//!
//! A file with no importer is either isolated or never looked at, and a list cannot say
//! which: `SymbolIndex::importers_status` answers [`Importers::Known`], [`Importers::Isolated`]
//! or [`Importers::Uncovered`], the last when no edge source declared the file. "Isolated"
//! is a statement about the edge source: the wiring loads an edge only on import evidence
//! (F13), so it means "no RESOLVED import points here", not "nothing depends on this file".
//!
//! # The write path
//!
//! Indexing, reindexing and removing a file are one verb each (`index_file` /
//! `index_files` / `remove_file`), and all of them write through the same three private
//! steps — see the comment above `SymbolIndex::insert_symbol`.

pub mod blast_radius;
pub mod cycles;
pub mod enriched;
pub(crate) mod imports;
pub(crate) mod method_calls;
pub(crate) mod module_paths;
pub mod pheromone;
pub(crate) mod python_uses;
mod references;
pub(crate) mod self_refs;

pub use blast_radius::*;
pub use enriched::*;
pub use imports::*;
pub use module_paths::*;
pub use pheromone::*;
pub use references::*;
// cycles: methods on SymbolIndex, re-exported implicitly via the struct itself.

use std::collections::{BTreeMap, BinaryHeap, HashMap, HashSet, VecDeque};
use std::sync::OnceLock;

use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use tracing::instrument;

use crate::ast::error::AstResult;
use crate::ast::languages::Lang;
use crate::ast::symbols::{Symbol, extract_symbols};

/// INS-A4: serde skip helper — omit `co_edit_weight` when 0.0.
#[allow(clippy::trivially_copy_pass_by_ref)]
pub(crate) fn is_zero_f64(v: &f64) -> bool {
    *v == 0.0
}

/// A location where a symbol is defined or referenced
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SymbolLocation {
    /// File path (relative to project root)
    pub file_path: String,
    /// Symbol name
    pub symbol_name: String,
    /// Line number (1-indexed)
    pub line: usize,
    /// Column number (0-indexed)
    pub column: usize,
    /// Whether this is a definition (true) or reference (false)
    pub is_definition: bool,
    /// Canonical symbol kind (`SymbolKind::as_str()`: `function`, `class`,
    /// `const`, `variable`, `method`, …). `None` when the producing path did
    /// not classify it (e.g. legacy rows, reference-only locations, or symbols
    /// loaded from a pre-`kind` index). Additive (`#[serde(default)]`) so RPC
    /// payloads and persisted snapshots stay backward/forward compatible.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
}

impl SymbolLocation {
    /// Create a new symbol location (kind unset — use [`Self::with_kind`] to classify).
    pub fn new(
        file_path: impl Into<String>,
        symbol_name: impl Into<String>,
        line: usize,
        column: usize,
        is_definition: bool,
    ) -> Self {
        Self {
            file_path: file_path.into(),
            symbol_name: symbol_name.into(),
            line,
            column,
            is_definition,
            kind: None,
        }
    }

    /// Builder: attach a canonical symbol kind (e.g. `SymbolKind::as_str()`).
    /// Chains after [`Self::new`] so the 5-arg constructor stays source-compatible
    /// with all existing call sites.
    #[must_use]
    pub fn with_kind(mut self, kind: Option<String>) -> Self {
        self.kind = kind;
        self
    }
}

/// Dependency edge between files
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DependencyEdge {
    /// Importing file: a file key
    pub from: String,
    /// What the edge points at. For an edge extraction produced, the text the import names:
    /// a module key (`utils`, `crate::a::b`). For a RESOLVED edge (loaded from the wiring,
    /// `SymbolIndex::load_file_edges`), the file key of the imported file (`src/foo.rs`).
    pub to: String,
    /// Imported symbols
    pub symbols: Vec<String>,
    /// INS-A4: co-edit weight — higher = files are frequently co-edited.
    /// Used as edge weight in weighted blast-radius (Dijkstra-style).
    /// Range [0.0, ∞). Defaults to 0.0 (no co-edit signal).
    #[serde(default, skip_serializing_if = "crate::ast::graph::is_zero_f64")]
    pub co_edit_weight: f64,
}

/// Number of logical shards used internally to pre-group file-keyed data
/// during bulk `index_files` operations. Public fields remain flat HashMaps
/// for backward compatibility with all consumers.
pub const SHARD_COUNT: usize = 16;

/// Compute a shard bucket index from a file path.
///
/// Hashes the first path component (typically the crate/package name) so that
/// files from the same crate land in the same bucket. This improves cache
/// locality during sequential merge after parallel extraction.
#[inline]
fn shard_for(file_path: &str) -> usize {
    let key = file_path.split('/').next().unwrap_or(file_path);
    let mut h: usize = 0;
    for b in key.bytes() {
        h = h.wrapping_mul(31).wrapping_add(b as usize);
    }
    h % SHARD_COUNT
}

/// The module key under which the importers of `file_path` are filed: the stem of its
/// file name (`pkg/utils.py` → `utils`), or the whole path when the name has no
/// extension (see the key grammar in the module docs).
///
/// A heuristic, and a known one: it matches an import that names the file's stem
/// (`import utils`, `from utils import x`) and never one that spells a module path
/// (Rust `crate::a::b` is not the stem `b`). It lives here, once, so the in-memory
/// index and the SQLite store cannot disagree about it.
pub(crate) fn module_key(file_path: &str) -> &str {
    std::path::Path::new(file_path)
        .file_name()
        .and_then(|name| name.to_str())
        .and_then(|name| name.rsplit_once('.'))
        .map_or(file_path, |(stem, _extension)| stem)
}

/// What extraction says about one file: its symbol definitions and its imports.
/// Computed without touching the index, merged into it by `SymbolIndex::replace_file`.
struct FileFacts {
    symbols: Vec<Symbol>,
    imports: Vec<ImportInfo>,
}

/// Extract everything the index records about a file. The only fallible step of
/// indexing, kept apart from the merge so a failure can never leave a half-written index.
fn extract_file(source: &str, lang: Lang) -> AstResult<FileFacts> {
    Ok(FileFacts {
        symbols: extract_symbols(source, lang)?,
        imports: extract_imports(source, lang),
    })
}

/// A pair of files with no direction: the same key for `(a, b)` and `(b, a)`.
fn ordered_pair(a: &str, b: &str) -> (String, String) {
    if a <= b {
        (a.to_string(), b.to_string())
    } else {
        (b.to_string(), a.to_string())
    }
}

/// Remove from `map[key]` the entries `matches` selects, and the key itself when that
/// leaves its list empty — the one place the "no empty lists" shape rule is applied.
fn drop_from<T>(map: &mut HashMap<String, Vec<T>>, key: &str, matches: impl Fn(&T) -> bool) {
    if let Some(list) = map.get_mut(key) {
        list.retain(|item| !matches(item));
        if list.is_empty() {
            map.remove(key);
        }
    }
}

/// What an index knows about who imports a file — the three answers a bare list cannot
/// tell apart. An empty list means "nobody" only where the index looked; where it did
/// not look, the honest answer is "unknown", and a signal built on "nobody" would say
/// a file is isolated when it was simply never asked about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Importers<'a> {
    /// At least one importer is known.
    Known(&'a [String]),
    /// The index looked at this file and no RESOLVED import points at it: a real zero of
    /// the edge source, which loads an edge on import evidence only (F13) — a file that
    /// merely matched a name in this one is not an importer. It does not say nothing
    /// depends on the file.
    Isolated,
    /// The index does not speak for this file (the edge source never covered it):
    /// absence of an edge here is not evidence of absence.
    Uncovered,
}

/// What an index holds of the dependency graph, in two counts (see
/// [`SymbolIndex::edge_coverage_summary`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct EdgeCoverage {
    /// Edges declared by the indexed files, resolved or raw.
    pub edges: usize,
    /// Files an edge source declared it speaks for: `0` when none was loaded.
    pub covered_files: usize,
}

/// Cross-file symbol index for project-wide searches
///
/// The maps speak two kinds of key (the grammar is in the module docs): `symbols`'
/// locations, `file_to_symbols` and `dependencies` are keyed by **file key**;
/// `reverse_deps` is keyed by the **file key** of the imported file when the edge was
/// resolved (loaded from the wiring) and by the **module key** when it was not
/// (extracted by `index_file`).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SymbolIndex {
    /// Map: symbol name -> locations
    pub symbols: HashMap<String, Vec<SymbolLocation>>,
    /// Map: file key -> names of the symbols defined in that file
    pub file_to_symbols: HashMap<String, Vec<String>>,
    /// Dependency graph: file key -> the edges the file declares (one per import)
    pub dependencies: HashMap<String, Vec<DependencyEdge>>,
    /// Reverse dependencies: file key (resolved edge) or module key (unresolved edge)
    /// -> the file keys that import it. Read it through [`SymbolIndex::importers_of`]
    /// when you hold a file path.
    pub reverse_deps: HashMap<String, Vec<String>>,
    /// The files an edge source (the wiring, see `load_file_edges`) speaks for. Empty
    /// when no source was loaded; [`SymbolIndex::importers_status`] reads it.
    #[serde(default, skip_serializing_if = "HashSet::is_empty")]
    edge_coverage: HashSet<String>,
    /// The extensions of the files in `edge_coverage` (lowercase; `""` for a file with none),
    /// derived from it on first use by [`SymbolIndex::coverage_expected`]. A cache, never
    /// serialized: every writer of `edge_coverage` drops it (`invalidate_covered_extensions`),
    /// so it is rebuilt from the set it describes and cannot go stale.
    #[serde(skip)]
    covered_extensions: OnceLock<HashSet<String>>,
}

/// The extension of a file key, lowercased; `""` when it has none (`Makefile`, `.gitignore`).
fn extension_key(file_path: &str) -> String {
    std::path::Path::new(file_path)
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default()
}

impl SymbolIndex {
    /// Create empty symbol index
    pub fn new() -> Self {
        Self::default()
    }

    /// The files that import `file_path`. Empty when nothing imports it — or when the
    /// index cannot say; [`SymbolIndex::importers_status`] tells those apart.
    ///
    /// Two buckets can answer. The bucket under the file's own key holds the edges that
    /// were RESOLVED to this file (loaded from the wiring), and it wins. Failing that,
    /// the bucket under the file's module key (see `module_key`) holds the unresolved
    /// imports that merely name its stem, and it answers only when this file is the ONE
    /// indexed file with that stem: `import utils` with `a/utils.py` and `b/utils.py`
    /// both indexed credits neither, because crediting both would blame a file for an
    /// import it never received. Precision before recall.
    pub fn importers_of(&self, file_path: &str) -> &[String] {
        if let Some(resolved) = self.reverse_deps.get(file_path) {
            return resolved;
        }
        let key = module_key(file_path);
        match self.reverse_deps.get(key) {
            Some(by_stem) if key != file_path && self.is_sole_owner_of_key(file_path, key) => {
                by_stem
            }
            _ => &[],
        }
    }

    /// [`SymbolIndex::importers_of`] with the answer typed: a known list, a real zero, or
    /// "this index does not speak for the file". A file is covered when an edge source
    /// declared it (`load_file_edges`, `replace_edges_of`); an index with no edge source
    /// at all speaks for the files it holds, as it always did.
    pub fn importers_status(&self, file_path: &str) -> Importers<'_> {
        let known = self.importers_of(file_path);
        if !known.is_empty() {
            return Importers::Known(known);
        }
        let covered = if self.edge_coverage.is_empty() {
            self.file_to_symbols.contains_key(file_path)
                || self.dependencies.contains_key(file_path)
        } else {
            self.edge_coverage.contains(file_path)
        };
        if covered {
            Importers::Isolated
        } else {
            Importers::Uncovered
        }
    }

    /// Whether the edge source speaks for files LIKE `file_path`: an edge source was loaded
    /// AND at least one file it covers has this file's extension (case-insensitive).
    ///
    /// It separates the two reasons a file is [`Importers::Uncovered`]. Where this is `true`,
    /// the wiring reads this kind of file, so an uncovered one is a gap worth saying aloud (a
    /// new source file, a failed parse, an excluded path). Where it is `false` — no edge source
    /// at all, or a `.md`/`.toml`/`.py` in a project whose wiring only reads `.rs` — the wiring
    /// never reads such files and "uncovered" says nothing a reader can act on. It does NOT say
    /// whether `file_path` itself is covered; ask [`SymbolIndex::importers_status`] for that.
    ///
    /// The set of covered extensions is derived once from `edge_coverage` and dropped by every
    /// writer of it (`load_file_edges`, `replace_edges_of`, `remove_file`, `clear`).
    pub fn coverage_expected(&self, file_path: &str) -> bool {
        // No early return for "no coverage": an empty set has no extensions, so it answers
        // `false` by itself, and the cache stays the one authority every writer must keep honest.
        self.covered_extensions
            .get_or_init(|| {
                self.edge_coverage
                    .iter()
                    .map(|f| extension_key(f))
                    .collect()
            })
            .contains(&extension_key(file_path))
    }

    /// Forget the covered-extensions cache: the set it was derived from changed.
    fn invalidate_covered_extensions(&mut self) {
        self.covered_extensions = OnceLock::new();
    }

    /// How many files an edge source (the wiring) speaks for, and how many edges the index
    /// holds: the two numbers `touring status -j` prints under `file_edges`, so a daemon
    /// whose index loaded no edges is visible as `0`, not left to be inferred from the
    /// signals that go quiet. `edges` counts every declared edge, resolved or raw.
    pub fn edge_coverage_summary(&self) -> EdgeCoverage {
        EdgeCoverage {
            edges: self.dependencies.values().map(Vec::len).sum(),
            covered_files: self.edge_coverage.len(),
        }
    }

    /// The importers of `file_path` — exactly the set [`SymbolIndex::importers_of`] answers,
    /// never more or fewer — ordered by how much each one matters to the file: the co-edit
    /// weight of the edge toward it first (files edited together are the likeliest to break
    /// together), then how many files import the importer itself (a hub that imports you
    /// carries the change further; the file-key bucket only, so a stem bucket is not
    /// scanned), then the path, so the order is the same on every run.
    ///
    /// A signal that can name only a few importers (the hook texts show five) picks them
    /// here, so a hub is described by its most consequential dependents, not by whichever
    /// the map yielded first.
    pub fn ranked_importers(&self, file_path: &str) -> Vec<&str> {
        let mut ranked: Vec<(&str, f64, usize)> = self
            .importers_of(file_path)
            .iter()
            .map(|importer| {
                let weight = self
                    .edge_toward(importer, file_path)
                    .map_or(0.0, |edge| edge.co_edit_weight);
                let in_degree = self.reverse_deps.get(importer.as_str()).map_or(0, Vec::len);
                (importer.as_str(), weight, in_degree)
            })
            .collect();
        ranked.sort_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| b.2.cmp(&a.2))
                .then_with(|| a.0.cmp(b.0))
        });
        ranked.into_iter().map(|(path, _, _)| path).collect()
    }

    /// Clear all entries for idempotent reload — call before `load_into_index` to prevent
    /// duplicate accumulation. The files an edge source covered go too: a reload starts
    /// from an index no edge source has spoken for yet.
    pub fn clear(&mut self) {
        self.symbols.clear();
        self.file_to_symbols.clear();
        self.dependencies.clear();
        self.reverse_deps.clear();
        self.edge_coverage.clear();
        self.invalidate_covered_extensions();
    }

    // ── The write path ───────────────────────────────────────────────────
    //
    // Everything that enters or leaves the four maps goes through `insert_symbol`,
    // `insert_edge` and `withdraw`. They keep two invariants the readers rely on:
    //
    // - `file_to_symbols[f]` lists the names of the definitions in `symbols[*]` whose
    //   `file_path` is `f`;
    // - `reverse_deps[m]` lists the `from` of every edge whose `to` is `m`;
    //
    // and one shape rule: a map never holds an empty list (a file with no symbols has no
    // `file_to_symbols` key, a file with no imports has no `dependencies` key). That is
    // what makes "index, then remove" return the index to the state it started in.

    /// The only way a symbol location enters the index: files it under its name and,
    /// when it is a definition, under its file.
    pub(crate) fn insert_symbol(&mut self, location: SymbolLocation) {
        if location.is_definition {
            self.file_to_symbols
                .entry(location.file_path.clone())
                .or_default()
                .push(location.symbol_name.clone());
        }
        self.symbols
            .entry(location.symbol_name.clone())
            .or_default()
            .push(location);
    }

    /// The only way a dependency edge enters the index: files it under the importing
    /// file (`edge.from`, a file key) and its importer under the module it names
    /// (`edge.to`, a module key).
    pub(crate) fn insert_edge(&mut self, edge: DependencyEdge) {
        self.reverse_deps
            .entry(edge.to.clone())
            .or_default()
            .push(edge.from.clone());
        self.dependencies
            .entry(edge.from.clone())
            .or_default()
            .push(edge);
    }

    /// Take back the symbol definitions `file_path` contributed.
    fn withdraw_symbols(&mut self, file_path: &str) {
        for name in self.file_to_symbols.remove(file_path).unwrap_or_default() {
            drop_from(&mut self.symbols, &name, |location| {
                location.file_path == file_path
            });
        }
    }

    /// Take back the outgoing edges of `file_path`, with the importer entry each one
    /// left in `reverse_deps`. The edges OTHER files declare toward it are not its
    /// contribution and stay.
    fn withdraw_edges(&mut self, file_path: &str) {
        for edge in self.dependencies.remove(file_path).unwrap_or_default() {
            drop_from(&mut self.reverse_deps, &edge.to, |importer| {
                importer == file_path
            });
        }
    }

    /// Take back everything `file_path` contributed: symbols and outgoing edges.
    fn withdraw(&mut self, file_path: &str) {
        self.withdraw_symbols(file_path);
        self.withdraw_edges(file_path);
    }

    /// Make the index say what `facts` say about `file_path`: its previous symbols are
    /// withdrawn and the new ones inserted. Its edges follow, unless an edge source
    /// speaks for the file (`edge_coverage`): resolved edges are better than the raw
    /// imports extraction sees, and a reindex must not trade the first for the second.
    /// Cannot fail, which is what lets `index_files` extract everything first and merge
    /// after.
    fn replace_file(&mut self, file_path: &str, facts: FileFacts) {
        self.withdraw_symbols(file_path);
        for symbol in facts.symbols {
            let kind = symbol.kind.as_str().to_string();
            self.insert_symbol(
                SymbolLocation::new(file_path, symbol.name, symbol.line, symbol.column, true)
                    .with_kind(Some(kind)),
            );
        }
        if self.edge_coverage.contains(file_path) {
            return;
        }
        self.withdraw_edges(file_path);
        for import in facts.imports {
            self.insert_edge(DependencyEdge {
                from: file_path.to_string(),
                to: import.module_path,
                symbols: import.symbols,
                co_edit_weight: 0.0,
            });
        }
    }

    /// Every indexed file once: a file counts as indexed once it has a symbol or an
    /// edge, the two ways a file leaves a trace.
    fn indexed_files(&self) -> impl Iterator<Item = &String> {
        self.file_to_symbols.keys().chain(
            self.dependencies
                .keys()
                .filter(|file| !self.file_to_symbols.contains_key(*file)),
        )
    }

    /// Whether some indexed file is filed under the module key `key`.
    fn some_file_answers_to(&self, key: &str) -> bool {
        self.indexed_files().any(|file| module_key(file) == key)
    }

    /// Whether `file_path` is the only indexed file filed under the module key `key`.
    fn is_sole_owner_of_key(&self, file_path: &str, key: &str) -> bool {
        let mut owners = self.indexed_files().filter(|file| module_key(file) == key);
        owners.next().is_some_and(|only| only == file_path) && owners.next().is_none()
    }

    /// Cut the edges other files declare toward `key` (a file key or a module key): they
    /// point at something that is no longer there.
    fn sever_importers(&mut self, key: &str) {
        for importer in self.reverse_deps.remove(key).unwrap_or_default() {
            drop_from(&mut self.dependencies, &importer, |edge| edge.to == key);
        }
    }

    /// Replace the outgoing edges of `consumer` with edges to `producers`, files named
    /// by their file key — the resolved form the wiring records. Whatever `consumer`
    /// declared before, resolved or raw, is withdrawn: the edge source speaks for the
    /// file's imports from now on, so `consumer` becomes covered (an empty `producers`
    /// is a real zero, not an absence). A producer listed twice, or the consumer itself,
    /// adds no edge. The co-edit weight of a pair belongs to the PAIR, not to one
    /// declaration of it ([`SymbolIndex::apply_co_edit_weights`]: "an edge whose pair has
    /// no history keeps the weight it had"), so an edge toward a producer the consumer
    /// still imports keeps its weight: a hook that refreshes the file after an edit must
    /// not reset the ranking of its importers to zero.
    pub fn replace_edges_of(&mut self, consumer: &str, producers: &[String]) {
        let kept_weights: HashMap<String, f64> = self
            .dependencies
            .get(consumer)
            .into_iter()
            .flatten()
            .filter(|edge| edge.co_edit_weight != 0.0)
            .map(|edge| (edge.to.clone(), edge.co_edit_weight))
            .collect();
        self.withdraw_edges(consumer);
        let mut seen = HashSet::new();
        for producer in producers {
            if producer != consumer && seen.insert(producer.as_str()) {
                self.insert_edge(DependencyEdge {
                    from: consumer.to_string(),
                    to: producer.clone(),
                    symbols: Vec::new(),
                    co_edit_weight: kept_weights.get(producer).copied().unwrap_or(0.0),
                });
            }
        }
        if self.edge_coverage.insert(consumer.to_string()) {
            self.invalidate_covered_extensions();
        }
    }

    /// Load resolved file-to-file edges — `(consumer, producer)` pairs of file keys — and
    /// declare which files the source speaks for. `covered` should list every file the
    /// source looked at, including producers nothing consumes: those are the files
    /// [`SymbolIndex::importers_status`] answers `Isolated` for, while a file outside it
    /// answers `Uncovered`. Edges enter through `replace_edges_of`, grouped by consumer in
    /// path order, so loading twice replaces instead of doubling and the order is stable.
    pub fn load_file_edges(
        &mut self,
        edges: impl IntoIterator<Item = (String, String)>,
        covered: impl IntoIterator<Item = String>,
    ) {
        let mut by_consumer: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for (consumer, producer) in edges {
            by_consumer.entry(consumer).or_default().push(producer);
        }
        for (consumer, producers) in by_consumer {
            self.replace_edges_of(&consumer, &producers);
        }
        self.edge_coverage.extend(covered);
        self.invalidate_covered_extensions();
    }

    /// Put the co-edit history on the edges: `(file, file, count)` triples, a pair
    /// counting in either direction and summed when both are present. An edge whose pair
    /// has no history keeps the weight it had. The cost of an edge in
    /// `weighted_blast_radius` is `1 / (1 + weight)`.
    pub fn apply_co_edit_weights(
        &mut self,
        pairs: impl IntoIterator<Item = (String, String, i64)>,
    ) {
        let mut weights: HashMap<(String, String), f64> = HashMap::new();
        for (a, b, count) in pairs {
            if count > 0 && a != b {
                *weights.entry(ordered_pair(&a, &b)).or_insert(0.0) += count as f64;
            }
        }
        if weights.is_empty() {
            return;
        }
        for edge in self.dependencies.values_mut().flatten() {
            if let Some(weight) = weights.get(&ordered_pair(&edge.from, &edge.to)) {
                edge.co_edit_weight = *weight;
            }
        }
    }

    /// Index a single file, replacing whatever the file contributed before.
    ///
    /// Reindexing is the same verb as indexing: the previous symbols and outgoing edges
    /// of `file_path` are withdrawn first, so an edited file never keeps the
    /// definitions and imports it no longer has. Files that import it are untouched —
    /// editing a module is not removing it. When extraction fails the index is unchanged.
    #[instrument(skip(self, source), fields(file = %file_path, lang = %lang.as_str()))]
    pub fn index_file(&mut self, file_path: &str, source: &str, lang: Lang) -> AstResult<()> {
        let facts = extract_file(source, lang)?;
        self.replace_file(file_path, facts);
        Ok(())
    }

    /// Remove a file from the symbol index.
    ///
    /// Withdraws everything the file contributed (see `withdraw`) and stops covering it.
    /// The edges other files declared toward it by its own key (resolved edges) go with
    /// it. The ones that merely name its stem go only when no other indexed file is left
    /// under the same module key: a file that shares the stem (`a/utils.py` and
    /// `b/utils.py`) keeps them alive — the module still exists.
    pub fn remove_file(&mut self, file_path: &str) {
        self.withdraw(file_path);
        if self.edge_coverage.remove(file_path) {
            self.invalidate_covered_extensions();
        }
        self.sever_importers(file_path);

        let key = module_key(file_path);
        if key != file_path && !self.some_file_answers_to(key) {
            self.sever_importers(key);
        }
    }

    /// Index multiple files using rayon for parallel symbol extraction.
    ///
    /// Phase 1 is fully parallelized (CPU-bound extraction via rayon) and is the only
    /// phase that can fail: when any file fails to extract, nothing is merged.
    /// Phase 2 sorts the results by shard bucket (keyed on the first path component,
    /// i.e. crate/package name) so that consecutive HashMap insertions touch the same
    /// memory region — improving L2/L3 cache utilisation on large monorepos.
    /// Phase 3 merges sequentially through the same writer as `index_file`, so a file
    /// already in the index is replaced, never doubled.
    #[instrument(skip(self, files), fields(file_count = files.len()))]
    pub fn index_files(&mut self, files: &[(&str, &str, Lang)]) -> AstResult<()> {
        if files.is_empty() {
            return Ok(());
        }

        // Phase 1: parallel extraction (CPU-bound, no &mut self needed)
        let mut extracted = files
            .par_iter()
            .map(|(path, source, lang)| -> AstResult<_> {
                Ok((
                    path.to_string(),
                    shard_for(path),
                    extract_file(source, *lang)?,
                ))
            })
            .collect::<AstResult<Vec<_>>>()?;

        // Phase 2: sort by shard bucket before sequential merge so that files
        // from the same crate/package are processed consecutively. This keeps
        // HashMap probing warm in CPU cache across related entries.
        extracted.sort_by_key(|(_, shard, _)| *shard);

        // Pre-reserve capacity to avoid incremental rehashing during the merge.
        let file_count = files.len();
        self.file_to_symbols.reserve(file_count);
        self.dependencies.reserve(file_count);

        // Phase 3: sequential merge into the index (requires &mut self)
        for (path, _shard, facts) in extracted {
            self.replace_file(&path, facts);
        }
        Ok(())
    }

    /// Find all locations of a symbol by name
    pub fn find_symbol(&self, name: &str) -> Vec<&SymbolLocation> {
        self.symbols
            .get(name)
            .map(|v| v.iter().collect())
            .unwrap_or_default()
    }

    /// Find symbol with exact file match
    pub fn find_symbol_in_file(&self, name: &str, file_path: &str) -> Option<&SymbolLocation> {
        self.symbols
            .get(name)
            .and_then(|locations| locations.iter().find(|loc| loc.file_path == file_path))
    }

    /// Search symbols by prefix (for autocomplete)
    pub fn search_symbols(&self, prefix: &str) -> Vec<&SymbolLocation> {
        self.symbols
            .iter()
            .filter(|(name, _)| name.starts_with(prefix))
            .flat_map(|(_, locations)| locations.iter())
            .collect()
    }

    /// Get all symbols defined in a file
    pub fn get_file_symbols(&self, file_path: &str) -> Vec<&SymbolLocation> {
        self.file_to_symbols
            .get(file_path)
            .map(|names| {
                names
                    .iter()
                    .filter_map(|name| self.find_symbol_in_file(name, file_path))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Breadth-first walk of the importers of `start_file`, the start included, to at most
    /// `max_depth` hops: `visit(file, distance)` runs once per reached file, nearest first.
    /// The ONE walk behind [`SymbolIndex::blast_radius_with_depth`] and
    /// [`SymbolIndex::blast_extent`].
    ///
    /// Each hop asks `importers_of`: the importer it found is a file key, and the next hop
    /// must look it up as one, not as the module key the last hop used.
    fn walk_importers(
        &self,
        start_file: &str,
        max_depth: usize,
        mut visit: impl FnMut(&str, usize),
    ) {
        let mut seen: HashSet<&str> = HashSet::new();
        let mut queue: VecDeque<(&str, usize)> = VecDeque::new();
        seen.insert(start_file);
        queue.push_back((start_file, 0));
        while let Some((current, distance)) = queue.pop_front() {
            visit(current, distance);
            if distance < max_depth {
                for importer in self.importers_of(current) {
                    if seen.insert(importer.as_str()) {
                        queue.push_back((importer.as_str(), distance + 1));
                    }
                }
            }
        }
    }

    /// How far a change to `start_file` reaches — the file count and the farthest distance
    /// of [`SymbolIndex::blast_radius_with_depth`], without building the affected files
    /// and symbols nobody asked for. A hub of 1.920 files costs the walk plus one counter
    /// here, and the walk plus a `Vec` of every symbol of every file there (8,99 ms
    /// measured, against a count); callers that print a count and a depth ask this.
    pub fn blast_extent(&self, start_file: &str, max_depth: usize) -> BlastExtent {
        let mut extent = BlastExtent::default();
        self.walk_importers(start_file, max_depth, |_, distance| {
            extent.file_count += 1;
            extent.max_distance = extent.max_distance.max(distance);
        });
        extent
    }

    /// Calculate blast radius: all files transitively affected by a change to a file.
    ///
    /// Traverses the full reverse dependency graph with no depth limit.
    pub fn blast_radius(&self, start_file: &str) -> BlastRadius {
        self.blast_radius_with_depth(start_file, usize::MAX)
    }

    /// Calculate blast radius with a maximum traversal depth.
    ///
    /// P5.4: Limits BFS traversal to `max_depth` hops from the start file.
    /// This prevents runaway computation on deeply connected graphs.
    /// A `max_depth` of 0 returns only the start file itself.
    ///
    /// Wave 13: When `BLAST_LATENCY=1` env var is set, captures per-depth
    /// timing in `BlastRadius::blast_span` and `BlastRadius::trace_us`.
    pub fn blast_radius_with_depth(&self, start_file: &str, max_depth: usize) -> BlastRadius {
        let timing_enabled = std::env::var("BLAST_LATENCY").as_deref() == Ok("1");
        let start_time = if timing_enabled {
            Some(std::time::Instant::now())
        } else {
            None
        };

        let mut affected_files = HashSet::new();
        let mut affected_symbols = Vec::new();
        let mut distance_map = HashMap::new();

        // Wave 13: per-depth timing tracking
        let mut depth_times: Vec<u64> = vec![0; max_depth.min(64)];
        let mut depth_file_counts: Vec<usize> = vec![0; max_depth.min(64)];
        let mut current_depth = 0usize;
        let mut depth_start_time = start_time;

        self.walk_importers(start_file, max_depth, |current_file, distance| {
            affected_files.insert(current_file.to_string());
            distance_map.insert(current_file.to_string(), distance);

            // Wave 13: track time at depth boundary
            if timing_enabled {
                if distance > current_depth && current_depth < depth_times.len() {
                    if let Some(t) = depth_start_time {
                        depth_times[current_depth] = t.elapsed().as_micros() as u64;
                    }
                    current_depth = distance;
                    depth_start_time = Some(std::time::Instant::now());
                }
                if current_depth < depth_file_counts.len() {
                    depth_file_counts[current_depth] += 1;
                }
            }

            // Get symbols from this file
            if let Some(symbols) = self.file_to_symbols.get(current_file) {
                for sym in symbols {
                    affected_symbols.push((current_file.to_string(), sym.clone()));
                }
            }
        });

        // Wave 13: finalize timing
        let total_us = start_time
            .map(|t| t.elapsed().as_micros() as u64)
            .unwrap_or(0);
        let hop_count = depth_file_counts.iter().filter(|&&c| c > 0).count();
        let max_depth_time_us = depth_times.iter().copied().max().unwrap_or(0);

        // Build BlastSpan if timing enabled
        let blast_span = if timing_enabled {
            let hops: Vec<BlastHop> = depth_times
                .iter()
                .enumerate()
                .filter(|&(_, &t)| t > 0)
                .map(|(depth, &duration)| {
                    let cumulative = depth_file_counts[..=depth].iter().sum();
                    BlastHop {
                        depth,
                        file_count: depth_file_counts[depth],
                        duration_us: duration,
                        cumulative_files: cumulative,
                    }
                })
                .collect();
            Some(BlastSpan {
                trace_id: blast_radius::new_trace_id(),
                hops,
                total_duration_us: total_us,
            })
        } else {
            None
        };

        let file_count = affected_files.len();
        BlastRadius {
            start_file: start_file.to_string(),
            affected_files: affected_files.into_iter().collect(),
            affected_symbols,
            max_distance: distance_map.values().copied().max().unwrap_or(0),
            file_count,
            trace_us: total_us,
            hop_count,
            max_depth_time_us,
            blast_span,
        }
    }

    /// The edge `importer` declares toward `file`: the one that names the file by its own
    /// key (resolved), else the one that names its module key (unresolved).
    fn edge_toward(&self, importer: &str, file: &str) -> Option<&DependencyEdge> {
        let edges = self.dependencies.get(importer)?;
        edges
            .iter()
            .find(|edge| edge.to == file)
            .or_else(|| edges.iter().find(|edge| edge.to == module_key(file)))
    }

    /// INS-A4: Weighted blast radius using `co_edit_weight` as Dijkstra edge cost.
    ///
    /// Files co-edited together frequently (high `co_edit_weight`) have lower
    /// effective distance and surface earlier in the result. Returns affected
    /// files sorted by ascending weighted distance (lowest cost = highest priority).
    ///
    /// Edge weight is `1.0 / (1.0 + co_edit_weight)` — higher co-edit weight
    /// means the edge is "shorter" (more correlated change).
    pub fn weighted_blast_radius(&self, start_file: &str) -> Vec<(String, f64)> {
        // Min-heap: (neg_distance, file) — BinaryHeap is a max-heap.
        #[derive(PartialEq)]
        struct State {
            cost: f64,
            file: String,
        }
        impl Eq for State {}
        impl PartialOrd for State {
            fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
                Some(self.cmp(other))
            }
        }
        impl Ord for State {
            fn cmp(&self, other: &Self) -> std::cmp::Ordering {
                // Reverse for min-heap: smaller cost = higher priority.
                other
                    .cost
                    .partial_cmp(&self.cost)
                    .unwrap_or(std::cmp::Ordering::Equal)
            }
        }

        let mut dist: HashMap<String, f64> = HashMap::new();
        let mut heap = BinaryHeap::new();

        dist.insert(start_file.to_string(), 0.0);
        heap.push(State {
            cost: 0.0,
            file: start_file.to_string(),
        });

        while let Some(State { cost, file }) = heap.pop() {
            // Skip stale entries.
            if dist.get(&file).copied().unwrap_or(f64::INFINITY) < cost {
                continue;
            }
            // Traverse reverse deps (files that import `file`).
            for importer in self.importers_of(&file) {
                // Find the edge weight from the dependency graph.
                let edge_weight = self
                    .edge_toward(importer, &file)
                    .map(|e| 1.0 / (1.0 + e.co_edit_weight))
                    .unwrap_or(1.0);
                let next_cost = cost + edge_weight;
                let current_dist = dist.get(importer).copied().unwrap_or(f64::INFINITY);
                if next_cost < current_dist {
                    dist.insert(importer.clone(), next_cost);
                    heap.push(State {
                        cost: next_cost,
                        file: importer.clone(),
                    });
                }
            }
        }

        // Files at the same cost keep a fixed order (the path): `dist` is a HashMap, and a
        // list that reorders its ties between runs cannot be cut to its first few names.
        let mut result: Vec<(String, f64)> = dist.into_iter().collect();
        result.sort_by(|a, b| {
            a.1.partial_cmp(&b.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.0.cmp(&b.0))
        });
        result
    }

    /// Find shortest path of dependencies between two files
    pub fn dependency_path(&self, from: &str, to: &str) -> Option<Vec<String>> {
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        let mut parent: HashMap<String, String> = HashMap::new();

        queue.push_back(from.to_string());
        visited.insert(from.to_string());

        while let Some(current) = queue.pop_front() {
            if current == to {
                // Reconstruct path
                let mut path = vec![to.to_string()];
                let mut node = to.to_string();

                while let Some(p) = parent.get(&node) {
                    path.push(p.clone());
                    node = p.clone();
                }

                path.reverse();
                return Some(path);
            }

            // Get files imported by current
            if let Some(deps) = self.dependencies.get(&current) {
                for edge in deps {
                    if !visited.contains(&edge.to) {
                        visited.insert(edge.to.clone());
                        parent.insert(edge.to.clone(), current.clone());
                        queue.push_back(edge.to.clone());
                    }
                }
            }
        }

        None
    }

    /// Query symbols by name pattern and optional kind filter
    pub fn query_symbols(&self, name_pattern: &str, kind: Option<&str>) -> Vec<&SymbolLocation> {
        let pattern_lower = name_pattern.to_lowercase();
        self.symbols
            .iter()
            .filter(|(name, _)| name.to_lowercase().contains(&pattern_lower))
            .flat_map(|(_, locations)| locations.iter())
            .filter(|loc| {
                if let Some(k) = kind {
                    // Filter by definition status as a proxy for kind
                    match k {
                        "definition" => loc.is_definition,
                        "reference" => !loc.is_definition,
                        _ => true,
                    }
                } else {
                    true
                }
            })
            .collect()
    }

    /// The `limit` most imported FILES, as `(file key, importers)`, most first, ties in path
    /// order; a file nobody imports is not a hotspot.
    ///
    /// Counts through [`SymbolIndex::importers_of`], so every label is a file key the index
    /// holds (a file with a symbol or an edge, or one an edge source declared). Reading
    /// `reverse_deps` keys instead named whatever the edge pointed at: the file for a resolved
    /// edge but the bare MODULE (`utils`) for a raw import, which is not a file and cannot be
    /// opened, edited or blasted.
    pub fn hotspots(&self, limit: usize) -> Vec<(&str, usize)> {
        let files: std::collections::BTreeSet<&str> = self
            .indexed_files()
            .chain(self.edge_coverage.iter())
            .map(String::as_str)
            .collect();
        let mut counts: Vec<(&str, usize)> = files
            .into_iter()
            .map(|file| (file, self.importers_of(file).len()))
            .filter(|(_, importers)| *importers > 0)
            .collect();
        counts.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
        counts.truncate(limit);
        counts
    }

    /// Get stats about the index
    pub fn stats(&self) -> IndexStats {
        IndexStats {
            total_symbols: self.symbols.len(),
            total_locations: self.symbols.values().map(|v| v.len()).sum(),
            total_files: self.file_to_symbols.len(),
            total_dependencies: self.dependencies.values().map(|v| v.len()).sum(),
        }
    }
}

/// Index statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexStats {
    /// Number of unique symbol names
    pub total_symbols: usize,
    /// Number of symbol locations (definitions + references)
    pub total_locations: usize,
    /// Number of indexed files
    pub total_files: usize,
    /// Number of dependency edges
    pub total_dependencies: usize,
}

/// Build a `SymbolIndex` from a slice of `(file_path, source, language)` triples.
///
/// Convenience wrapper around `SymbolIndex::index_files` for callers that have
/// all file data available up-front. Returns an error if any file fails to parse.
pub fn build_index_from_files(files: &[(&str, &str, Lang)]) -> AstResult<SymbolIndex> {
    let mut index = SymbolIndex::new();
    index.index_files(files)?;
    Ok(index)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::indexing_slicing)]
    use super::*;

    #[test]
    fn test_symbol_index() {
        let mut index = SymbolIndex::new();

        let python_source = r#"
def hello():
    pass

class Foo:
    def bar(self):
        return 42
"#;

        index
            .index_file("test.py", python_source, Lang::Python)
            .unwrap();

        let locations = index.find_symbol("hello");
        assert_eq!(locations.len(), 1);
        assert_eq!(locations[0].file_path, "test.py");
        assert_eq!(locations[0].line, 2);
    }

    /// A batch in which one extraction fails merges NOTHING: extraction is the only step
    /// that can fail, and it runs to the end before the first file enters the index.
    ///
    /// The failure is forced with the parse budget (`with_budget` is thread-local and
    /// `index_files` extracts on rayon workers, so a one-thread pool keeps every parse on
    /// the thread that holds the budget). A zero budget halts a parse at tree-sitter's
    /// first progress check: the big source is cut, the tiny one finishes. The tiny file
    /// is named to land in shard 0, the bucket a merge-as-you-go loop would have reached
    /// before the failure.
    #[test]
    fn a_batch_with_one_failing_extraction_merges_nothing() {
        use crate::ast::grammar::with_budget;
        use std::time::Duration;

        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(1)
            .build()
            .unwrap();
        let tiny = (0u32..)
            .map(|n| format!("s{n}.py"))
            .find(|name| shard_for(name) == 0)
            .unwrap();
        let big = "def f():\n    pass\n".repeat(3_000);
        let mut index = SymbolIndex::new();
        index
            .index_file("keep.py", "def keep():\n    pass\n", Lang::Python)
            .unwrap();
        let before = serde_json::to_value(&index).unwrap();
        let batch = [
            (tiny.as_str(), "y = 2\n", Lang::Python),
            ("big.py", big.as_str(), Lang::Python),
        ];

        let outcome = pool.install(|| with_budget(Duration::ZERO, || index.index_files(&batch)));

        assert!(outcome.is_err(), "the big source outlives a zero budget");
        assert_eq!(
            serde_json::to_value(&index).unwrap(),
            before,
            "no file of a failed batch may reach the index"
        );
    }

    #[test]
    fn module_key_is_the_stem_of_the_file_name() {
        for (file, key) in [
            ("utils.py", "utils"),
            ("pkg/utils.py", "utils"),
            ("a/b/utils.py", "utils"),
            ("pkg/a.b.py", "a.b"),
            ("src/lib.rs", "lib"),
            // No extension: the whole path is the key, as it always was.
            ("Makefile", "Makefile"),
            ("dir/Makefile", "dir/Makefile"),
        ] {
            assert_eq!(module_key(file), key, "module key of {file}");
        }
    }

    #[test]
    fn test_blast_radius() {
        let mut index = SymbolIndex::new();

        // File A exports symbol
        index
            .index_file("a.py", "def helper(): pass", Lang::Python)
            .unwrap();

        // File B imports from A
        index
            .index_file(
                "b.py",
                "from a import helper\ndef use_helper(): helper()",
                Lang::Python,
            )
            .unwrap();

        // File C imports from B
        index
            .index_file(
                "c.py",
                "from b import use_helper\nuse_helper()",
                Lang::Python,
            )
            .unwrap();

        // Manual setup of reverse deps for test
        index
            .reverse_deps
            .insert("a.py".to_string(), vec!["b.py".to_string()]);
        index
            .reverse_deps
            .insert("b.py".to_string(), vec!["c.py".to_string()]);

        let radius = index.blast_radius("a.py");
        assert!(radius.affected_files.contains(&"b.py".to_string()));
        assert!(radius.affected_files.contains(&"c.py".to_string()));
        assert_eq!(radius.max_distance, 2);
    }

    #[test]
    fn test_dependency_path() {
        let mut index = SymbolIndex::new();

        // Setup dependency chain
        index.dependencies.insert(
            "a.rs".to_string(),
            vec![DependencyEdge {
                from: "a.rs".to_string(),
                to: "b.rs".to_string(),
                symbols: vec!["helper".to_string()],
                co_edit_weight: 0.0,
            }],
        );
        index.dependencies.insert(
            "b.rs".to_string(),
            vec![DependencyEdge {
                from: "b.rs".to_string(),
                to: "c.rs".to_string(),
                symbols: vec!["utils".to_string()],
                co_edit_weight: 0.0,
            }],
        );

        let path = index.dependency_path("a.rs", "c.rs");
        assert!(path.is_some());
        let path = path.unwrap();
        assert_eq!(path, vec!["a.rs", "b.rs", "c.rs"]);
    }

    // -- P5.4: blast_radius_with_depth tests --

    #[test]
    fn test_blast_radius_with_depth_zero() {
        let mut index = SymbolIndex::new();
        index
            .index_file("a.py", "def helper(): pass", Lang::Python)
            .unwrap();
        index
            .reverse_deps
            .insert("a.py".to_string(), vec!["b.py".to_string()]);

        // depth=0 means only the start file itself
        let radius = index.blast_radius_with_depth("a.py", 0);
        assert_eq!(radius.file_count, 1);
        assert!(radius.affected_files.contains(&"a.py".to_string()));
        assert!(!radius.affected_files.contains(&"b.py".to_string()));
        assert_eq!(radius.max_distance, 0);
    }

    #[test]
    fn test_blast_radius_with_depth_one() {
        let mut index = SymbolIndex::new();
        index
            .index_file("a.py", "def helper(): pass", Lang::Python)
            .unwrap();
        index
            .index_file("b.py", "from a import helper", Lang::Python)
            .unwrap();
        index
            .index_file("c.py", "from b import helper", Lang::Python)
            .unwrap();

        index
            .reverse_deps
            .insert("a.py".to_string(), vec!["b.py".to_string()]);
        index
            .reverse_deps
            .insert("b.py".to_string(), vec!["c.py".to_string()]);

        // depth=1: a.py + direct importers only (b.py), NOT c.py
        let radius = index.blast_radius_with_depth("a.py", 1);
        assert!(radius.affected_files.contains(&"a.py".to_string()));
        assert!(radius.affected_files.contains(&"b.py".to_string()));
        assert!(
            !radius.affected_files.contains(&"c.py".to_string()),
            "c.py is 2 hops away, should not be included at depth=1"
        );
        assert_eq!(radius.max_distance, 1);
    }

    #[test]
    fn test_blast_radius_with_depth_unlimited_matches_original() {
        let mut index = SymbolIndex::new();
        index
            .index_file("a.py", "def helper(): pass", Lang::Python)
            .unwrap();

        index
            .reverse_deps
            .insert("a.py".to_string(), vec!["b.py".to_string()]);
        index
            .reverse_deps
            .insert("b.py".to_string(), vec!["c.py".to_string()]);

        let full = index.blast_radius("a.py");
        let unlimited = index.blast_radius_with_depth("a.py", usize::MAX);

        assert_eq!(full.file_count, unlimited.file_count);
        assert_eq!(full.max_distance, unlimited.max_distance);
    }
}
