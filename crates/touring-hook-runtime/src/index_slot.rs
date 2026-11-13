//! The daemon's in-memory dependency index, held once and shared by every read.
//!
//! Before C8 `pre_read` cloned the whole `SymbolIndex` for each graph read (31-38 ms in a
//! release build over this workspace's index, twice per read) and rebuilt the HNSW over it
//! each time. The index now lives here as one [`IndexGeneration`]: the `Arc<SymbolIndex>`
//! and the `Arc<OnceLock<HnswStrategy>>` built over it, REPLACED TOGETHER, so a read takes
//! two reference-count bumps and the HNSW is built once per generation.
//!
//! The slot is a `RefCell`, like the runtime's other mutable state, because hooks hold a
//! `&HookRuntime` and a hook that has just refreshed one file's wiring must be able to put
//! the new edges into the index (`touring-hook-runtime`'s `shared/reindex.rs`). A read that
//! overlaps a write keeps its snapshot: [`IndexSlot::edit`] copies the index only when a
//! reader still holds it.

use std::cell::{Cell, RefCell};
use std::sync::{Arc, OnceLock};

use touring_analysis::HnswStrategy;
use touring_code::ast::SymbolIndex;

/// One generation of the in-memory index and the approximate-neighbour index built over it.
///
/// Cheap to clone (two `Arc`s). Whoever holds a generation sees one index for as long as it
/// holds it, however many files change meanwhile.
#[derive(Clone)]
pub struct IndexGeneration {
    index: Arc<SymbolIndex>,
    hnsw: Arc<OnceLock<HnswStrategy>>,
}

impl IndexGeneration {
    fn new(index: SymbolIndex) -> Self {
        Self {
            index: Arc::new(index),
            hnsw: Arc::new(OnceLock::new()),
        }
    }

    /// The index of this generation.
    pub fn index(&self) -> &Arc<SymbolIndex> {
        &self.index
    }

    /// The HNSW cache of this generation: empty until the first deep read builds it, then
    /// shared by every later read of the same generation.
    pub fn hnsw_cache(&self) -> &Arc<OnceLock<HnswStrategy>> {
        &self.hnsw
    }
}

/// The slot an [`IndexGeneration`] lives in; empty until a loader fills it.
///
/// Empty is a real state, not a zero: the hooks read an empty slot as "no index" and stay
/// silent, as they did when nothing ever loaded one, so a daemon whose file-edges switch is
/// off behaves exactly as before.
#[derive(Default)]
pub struct IndexSlot {
    cell: RefCell<Option<IndexGeneration>>,
    /// A load was tried and could not fill the slot (no symbol store, unreadable stores): the
    /// lazy loader does not retry on every read. `replace` and `retry_load` clear it.
    failed: Cell<bool>,
}

impl IndexSlot {
    /// The current generation, if one was loaded: two `Arc` clones, no copy of the index.
    pub fn get(&self) -> Option<IndexGeneration> {
        self.cell.borrow().clone()
    }

    /// Whether a generation was loaded.
    pub fn is_loaded(&self) -> bool {
        self.cell.borrow().is_some()
    }

    /// Put `index` in the slot as a NEW generation: its HNSW cache starts empty, and readers
    /// that still hold the previous generation keep it.
    pub fn replace(&self, index: SymbolIndex) {
        *self.cell.borrow_mut() = Some(IndexGeneration::new(index));
        self.failed.set(false);
    }

    /// Whether a load was tried and failed (the lazy loader does not try again on every read).
    pub fn load_failed(&self) -> bool {
        self.failed.get()
    }

    /// Record that a load was tried and could not fill the slot.
    pub fn mark_load_failed(&self) {
        self.failed.set(true);
    }

    /// Forget a failed load: the next read that needs the index tries again. An `index
    /// rebuild` rewrites the stores, so it is the moment a failure may have been cured.
    pub fn retry_load(&self) {
        self.failed.set(false);
    }

    /// Change the loaded index in place and report what `change` returned; `None` when no
    /// index is loaded (an edit has nothing to edit, and must not create the index).
    ///
    /// The index is copied only if a reader still holds it (`Arc::make_mut`): that reader
    /// keeps the snapshot it started with. The HNSW built over the old index is dropped
    /// with it: the next deep read builds a fresh one over the new index.
    pub fn edit<R>(&self, change: impl FnOnce(&mut SymbolIndex) -> R) -> Option<R> {
        let mut cell = self.cell.borrow_mut();
        let generation = cell.as_mut()?;
        let result = change(Arc::make_mut(&mut generation.index));
        generation.hnsw = Arc::new(OnceLock::new());
        Some(result)
    }

    /// A reference to the loaded index through exclusive access to the slot — the shape the
    /// `&mut self` loaders of the runtime return.
    pub fn loaded_mut(&mut self) -> Option<&SymbolIndex> {
        self.cell
            .get_mut()
            .as_ref()
            .map(|generation| generation.index.as_ref())
    }
}

/// What a load of the dependency index did, so the caller can log it and tests can assert it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IndexLoad {
    /// The switch is off: nothing was read and the slot was left as it was.
    Disabled,
    /// The runtime has no symbol store to read (it failed to open): nothing to load.
    NoStore,
    /// The index was loaded and now sits in the slot as a new generation.
    Loaded {
        /// Files that hold symbols.
        files: usize,
        /// Edges the index declares, resolved or raw.
        edges: usize,
        /// Files the wiring spoke for.
        covered_files: usize,
    },
    /// The stores could not be read; the slot was left as it was.
    Failed(String),
    /// Nothing was loaded, on purpose: no generation was held, so a reload has nothing to
    /// replace. The next read that needs the index loads it (lazy load).
    NotLoaded,
}

impl IndexLoad {
    /// One line for the daemon's log; the failing and the disabled cases say what to do.
    pub fn describe(&self) -> String {
        match self {
            Self::NotLoaded => "dependency index not loaded yet: it loads on the first read that \
                                needs it"
                .to_string(),
            Self::Disabled => format!(
                "dependency index NOT loaded: {FILE_EDGES_DISABLED_ENV} is set, the graph signals \
                 stay silent (unset it and restart the daemon to load the index)"
            ),
            Self::NoStore => "dependency index NOT loaded: the symbol store is not open \
                              (run `touring doctor -j`)"
                .to_string(),
            Self::Loaded {
                files,
                edges,
                covered_files,
            } => format!(
                "dependency index loaded: {files} files with symbols, {edges} edges, \
                 {covered_files} files covered by the wiring"
            ),
            Self::Failed(why) => format!(
                "dependency index NOT loaded: {why} (the graph signals stay silent; \
                 `touring index rebuild` rewrites the stores and reloads it)"
            ),
        }
    }
}

/// An index with nothing in it, for the one question a caller cannot leave unanswered
/// while the file-edges switch is off: [`crate::hook_runtime::HookRuntime::get_symbol_index`]
/// answers from this and stores nothing.
pub fn empty_symbol_index() -> &'static SymbolIndex {
    static EMPTY: OnceLock<SymbolIndex> = OnceLock::new();
    EMPTY.get_or_init(SymbolIndex::new)
}

/// Whether the daemon loads the dependency index (and with it the resolved file edges).
///
/// The human kill switch is `TOURING_FILE_EDGES_DISABLED=1`, read once when the runtime is
/// built. Off means the index is NOT loaded: the hooks find an empty slot and stay silent,
/// exactly what they did before the index was ever loaded in production. Loading the index
/// without the edges would not return to that state, it would switch the graph signals on
/// with an "isolado" answer for every file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileEdgesSource {
    /// The knob is unset (or off): the index is loaded with the wiring's resolved edges.
    Default,
    /// `TOURING_FILE_EDGES_DISABLED` holds a true value: the index is not loaded.
    Disabled,
}

impl FileEdgesSource {
    /// The word `touring status -j` prints under `file_edges.source`.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Disabled => "disabled",
        }
    }

    /// Whether the index is loaded under this source.
    pub const fn enabled(self) -> bool {
        matches!(self, Self::Default)
    }
}

/// The environment variable of the kill switch.
pub const FILE_EDGES_DISABLED_ENV: &str = "TOURING_FILE_EDGES_DISABLED";

/// What `TOURING_FILE_EDGES_DISABLED` asks for, and the text of the value that was refused
/// (so the caller can say so: a typo never turns the switch on or off in silence).
///
/// `1`, `true`, `yes`, `on` (any case) turn the index off; unset, empty, `0`, `false`, `no`,
/// `off` leave it on; anything else leaves it on and is returned in `.1`.
pub fn resolve_file_edges_source(raw: Option<&str>) -> (FileEdgesSource, Option<String>) {
    let Some(text) = raw.map(str::trim).filter(|text| !text.is_empty()) else {
        return (FileEdgesSource::Default, None);
    };
    match text.to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => (FileEdgesSource::Disabled, None),
        "0" | "false" | "no" | "off" => (FileEdgesSource::Default, None),
        _ => (FileEdgesSource::Default, Some(text.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn index_with_edge(consumer: &str, producer: &str) -> SymbolIndex {
        let mut index = SymbolIndex::new();
        index.load_file_edges(
            [(consumer.to_string(), producer.to_string())],
            [consumer.to_string(), producer.to_string()],
        );
        index
    }

    #[test]
    fn every_load_outcome_describes_itself_and_the_bad_ones_say_what_to_do() {
        let loaded = IndexLoad::Loaded {
            files: 12,
            edges: 34,
            covered_files: 56,
        }
        .describe();
        assert!(
            loaded.contains("12 files") && loaded.contains("34 edges") && loaded.contains("56"),
            "{loaded}"
        );
        let disabled = IndexLoad::Disabled.describe();
        assert!(disabled.contains(FILE_EDGES_DISABLED_ENV), "{disabled}");
        assert!(disabled.contains("unset"), "the fix is named: {disabled}");
        let failed = IndexLoad::Failed("disk I/O error".to_string()).describe();
        assert!(
            failed.contains("disk I/O error") && failed.contains("touring index rebuild"),
            "{failed}"
        );
        assert!(IndexLoad::NoStore.describe().contains("touring doctor"));
        let idle = IndexLoad::NotLoaded.describe();
        assert!(
            idle.contains("not loaded yet") && idle.contains("first read"),
            "{idle}"
        );
    }

    #[test]
    fn a_failed_mark_is_cleared_by_a_new_generation_and_by_a_retry() {
        let slot = IndexSlot::default();
        assert!(!slot.load_failed(), "a new slot has not failed");

        slot.mark_load_failed();
        assert!(slot.load_failed());
        slot.retry_load();
        assert!(!slot.load_failed(), "a rebuild forgets the failure");

        slot.mark_load_failed();
        slot.replace(index_with_edge("a.rs", "b.rs"));
        assert!(
            !slot.load_failed(),
            "a generation that loaded is no longer a failed load"
        );
        assert!(slot.is_loaded());
    }

    #[test]
    fn an_empty_slot_is_no_index_and_stays_that_way_under_edit() {
        let slot = IndexSlot::default();
        assert!(!slot.is_loaded());
        assert!(slot.get().is_none());
        assert!(
            slot.edit(|index| index.file_to_symbols.len()).is_none(),
            "there is nothing to edit, and an edit must not create the index"
        );
        assert!(!slot.is_loaded());
    }

    #[test]
    fn a_read_shares_the_index_it_does_not_copy_it() {
        let slot = IndexSlot::default();
        slot.replace(index_with_edge("a.rs", "b.rs"));
        let (first, second) = (slot.get().expect("loaded"), slot.get().expect("loaded"));
        assert!(
            Arc::ptr_eq(first.index(), second.index()),
            "two reads, one index: the clone is a reference-count bump"
        );
        assert!(Arc::ptr_eq(first.hnsw_cache(), second.hnsw_cache()));
        assert_eq!(first.index().importers_of("b.rs"), ["a.rs".to_string()]);
    }

    #[test]
    fn replace_starts_a_generation_with_an_empty_hnsw_cache_and_leaves_readers_theirs() {
        let slot = IndexSlot::default();
        slot.replace(index_with_edge("a.rs", "b.rs"));
        let old = slot.get().expect("loaded");
        old.hnsw_cache()
            .get_or_init(|| HnswStrategy::new(old.index(), 3));

        slot.replace(index_with_edge("c.rs", "d.rs"));
        let new = slot.get().expect("loaded");
        assert!(!Arc::ptr_eq(old.index(), new.index()));
        assert!(
            new.hnsw_cache().get().is_none(),
            "the new generation builds its own HNSW"
        );
        assert!(
            old.hnsw_cache().get().is_some(),
            "the reader that holds the old generation keeps its HNSW"
        );
        assert_eq!(old.index().importers_of("b.rs"), ["a.rs".to_string()]);
    }

    #[test]
    fn edit_changes_the_index_in_place_when_no_reader_holds_it() {
        let slot = IndexSlot::default();
        slot.replace(index_with_edge("a.rs", "b.rs"));
        let before = Arc::as_ptr(slot.get().expect("loaded").index());

        let answer = slot.edit(|index| {
            index.replace_edges_of("a.rs", &["c.rs".to_string()]);
            7
        });

        assert_eq!(answer, Some(7), "edit reports what the change returned");
        let after = slot.get().expect("loaded");
        assert_eq!(
            Arc::as_ptr(after.index()),
            before,
            "nobody held the index: it was changed where it stands, not copied"
        );
        assert!(after.index().importers_of("b.rs").is_empty());
        assert_eq!(after.index().importers_of("c.rs"), ["a.rs".to_string()]);
    }

    #[test]
    fn edit_leaves_a_snapshot_held_by_a_reader_alone_and_drops_the_hnsw() {
        let slot = IndexSlot::default();
        slot.replace(index_with_edge("a.rs", "b.rs"));
        let snapshot = slot.get().expect("loaded");
        snapshot
            .hnsw_cache()
            .get_or_init(|| HnswStrategy::new(snapshot.index(), 3));

        slot.edit(|index| index.replace_edges_of("a.rs", &[]));

        let current = slot.get().expect("loaded");
        assert_eq!(
            snapshot.index().importers_of("b.rs"),
            ["a.rs".to_string()],
            "the reader's snapshot did not move under it"
        );
        assert!(current.index().importers_of("b.rs").is_empty());
        assert!(!Arc::ptr_eq(snapshot.index(), current.index()));
        assert!(
            current.hnsw_cache().get().is_none(),
            "an HNSW built over the old edges is not carried to the new ones"
        );
    }

    #[test]
    fn loaded_mut_borrows_the_index_without_a_runtime_borrow_check() {
        let mut slot = IndexSlot::default();
        assert!(slot.loaded_mut().is_none());
        slot.replace(index_with_edge("a.rs", "b.rs"));
        let index = slot.loaded_mut().expect("loaded");
        assert_eq!(index.importers_of("b.rs"), ["a.rs".to_string()]);
    }

    #[test]
    fn the_empty_index_has_no_files_and_no_edges() {
        let empty = empty_symbol_index();
        assert!(empty.file_to_symbols.is_empty());
        assert_eq!(empty.edge_coverage_summary().edges, 0);
        assert!(
            std::ptr::eq(empty, empty_symbol_index()),
            "one shared value"
        );
    }

    #[test]
    fn the_kill_switch_turns_the_index_off_for_true_words_only() {
        for on in ["1", "true", "TRUE", " yes ", "On"] {
            assert_eq!(
                resolve_file_edges_source(Some(on)),
                (FileEdgesSource::Disabled, None),
                "{on:?}"
            );
        }
        for off in [
            None,
            Some(""),
            Some("  "),
            Some("0"),
            Some("false"),
            Some("No"),
            Some("OFF"),
        ] {
            assert_eq!(
                resolve_file_edges_source(off),
                (FileEdgesSource::Default, None),
                "{off:?}"
            );
        }
    }

    #[test]
    fn a_refused_value_leaves_the_index_on_and_is_returned_to_be_warned_about() {
        for typo in ["disable", "2", "enabled", "0x1"] {
            assert_eq!(
                resolve_file_edges_source(Some(typo)),
                (FileEdgesSource::Default, Some(typo.to_string())),
                "{typo:?} is not a switch position"
            );
        }
        assert_eq!(FileEdgesSource::Default.as_str(), "default");
        assert_eq!(FileEdgesSource::Disabled.as_str(), "disabled");
        assert!(FileEdgesSource::Default.enabled() && !FileEdgesSource::Disabled.enabled());
    }
}
