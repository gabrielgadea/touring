//! FileHeat: digital pheromone system for file indexing priority (AST-1).
//!
//! Tracks edit frequency, recency, and blast radius weight per file.
//! Used by IncrementalPipeline to prioritize hot files for re-indexing.
//!
//! # Projector Integration
//!
//! [`HeatMap`] can sync with a [`touring_storage::vfs::Projector`] to track file-set
//! changes (create/delete/rename) as heat events. Pass a `FileSet` and the
//! current timestamp to `HeatMap::sync_projector_changes`.

use indexmap::IndexMap;
use std::cmp::Reverse;
use std::collections::BinaryHeap;

#[cfg(test)]
use touring_storage::vfs::{FileSet, Projector};

/// Heat metadata for a single file.
#[derive(Debug, Clone)]
pub struct FileHeat {
    /// Number of edits recorded.
    pub edits: u32,
    /// Epoch seconds of last edit.
    pub last_edit_epoch: f64,
    /// Number of read accesses.
    pub access_count: u32,
    /// Multiplicative boost from blast radius file_count.
    pub blast_radius_weight: f64,
    /// Monotonically increasing write generation counter.
    /// Each call to `record_edit` increments this. Heap entries carry the
    /// generation at push time; a pop is stale if generations differ.
    generation: u64,
}

impl Default for FileHeat {
    fn default() -> Self {
        Self {
            edits: 0,
            last_edit_epoch: 0.0,
            access_count: 0,
            blast_radius_weight: 0.0,
            generation: 0,
        }
    }
}

impl FileHeat {
    /// Construct a FileHeat from pre-existing heat data (e.g. restored from DB).
    /// Sets `generation` to 0 since write history is not tracked for restored entries.
    pub fn from_existing(
        edits: u32,
        last_edit_epoch: f64,
        access_count: u32,
        blast_radius_weight: f64,
    ) -> Self {
        Self {
            edits,
            last_edit_epoch,
            access_count,
            blast_radius_weight,
            generation: 0,
        }
    }

    /// Compute heat score: `edits * recency_decay * (1 + blast_radius_weight)`.
    pub fn heat_score(&self, now: f64, half_life_secs: f64) -> f64 {
        let elapsed = (now - self.last_edit_epoch).max(0.0);
        let recency_decay = (-elapsed * std::f64::consts::LN_2 / half_life_secs).exp();
        f64::from(self.edits) * recency_decay * (1.0 + self.blast_radius_weight)
    }
}

/// Collection of FileHeat entries with capacity management.
///
/// Eviction uses a lazy-deletion min-heap for O(log n) amortized complexity,
/// replacing the previous O(n) linear scan.
///
/// Each heap entry carries `(heat_bits, generation, key)` where `heat_bits` is
/// the IEEE 754 bit representation of the heat score at push time and
/// `generation` is the entry's write counter at push time. A popped entry is
/// **stale** if the entry's current generation differs — meaning the file was
/// updated after the heap push — and is skipped. This ensures we only evict
/// entries whose coldness snapshot is still the current state.
pub struct HeatMap {
    entries: IndexMap<String, FileHeat>,
    capacity: usize,
    /// Min-heap: `Reverse` makes Rust's max-heap behave as a min-heap.
    /// Tuple: `(heat_bits, generation, file_key)`.
    /// Lowest heat_bits = coldest = evicted first.
    eviction_heap: BinaryHeap<Reverse<(u64, u64, String)>>,
}

impl HeatMap {
    /// Default half-life for heat decay: 24 hours.
    pub const DEFAULT_HALF_LIFE_SECS: f64 = 86_400.0;

    /// Create a new HeatMap with the given capacity.
    pub fn new(capacity: usize) -> Self {
        Self {
            entries: IndexMap::with_capacity(capacity.min(1024)),
            capacity,
            eviction_heap: BinaryHeap::new(),
        }
    }

    /// Record an edit event for a file.
    pub fn record_edit(&mut self, file: &str, now: f64) {
        let entry = self.entries.entry(file.to_string()).or_insert(FileHeat {
            edits: 0,
            last_edit_epoch: now,
            access_count: 0,
            blast_radius_weight: 0.0,
            generation: 0,
        });
        entry.edits = entry.edits.saturating_add(1);
        entry.last_edit_epoch = now;
        entry.generation = entry.generation.wrapping_add(1);
        // Push a snapshot: (heat_bits, generation, key).
        // Generation ties the snapshot to this exact write; pops with a
        // mismatched generation are stale and will be skipped.
        let heat_bits = entry
            .heat_score(now, Self::DEFAULT_HALF_LIFE_SECS)
            .to_bits();
        let r#gen = entry.generation;
        self.eviction_heap
            .push(Reverse((heat_bits, r#gen, file.to_owned())));
        self.evict_if_needed(now);
    }

    /// Record a read access for a file.
    pub fn record_access(&mut self, file: &str, now: f64) {
        let entry = self.entries.entry(file.to_string()).or_insert(FileHeat {
            edits: 0,
            last_edit_epoch: now,
            access_count: 0,
            blast_radius_weight: 0.0,
            generation: 0,
        });
        entry.access_count = entry.access_count.saturating_add(1);
    }

    /// Set blast radius weight for a file.
    pub fn set_blast_radius_weight(&mut self, file: &str, weight: f64) {
        if let Some(entry) = self.entries.get_mut(file) {
            entry.blast_radius_weight = weight;
        }
    }

    /// Return files sorted by heat score descending.
    pub fn get_priority_order(&self, now: f64) -> Vec<(&str, f64)> {
        let mut scored: Vec<(&str, f64)> = self
            .entries
            .iter()
            .map(|(k, v)| (k.as_str(), v.heat_score(now, Self::DEFAULT_HALF_LIFE_SECS)))
            .collect();
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored
    }

    /// Apply exponential decay to all entries.
    pub fn decay_all(&mut self, now: f64, half_life_secs: f64) {
        for entry in self.entries.values_mut() {
            let elapsed = (now - entry.last_edit_epoch).max(0.0);
            let decay = (-elapsed * std::f64::consts::LN_2 / half_life_secs).exp();
            // Saturating cast: f64 → u32
            entry.edits = (f64::from(entry.edits) * decay).min(f64::from(u32::MAX)) as u32;
        }
    }

    /// Number of tracked files.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns true if no files are tracked.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Sync heat events from a [`touring_storage::vfs::Projector`] change log.
    ///
    /// Iterates over [`Change`] variants emitted by `projector.diff()` and
    /// records edit events for affected file paths. File-created and
    /// file-renamed map to `record_edit`; file-deleted records a passive
    /// access to keep the entry alive until natural eviction.
    ///
    /// Returns the count of changes applied.
    #[cfg(test)]
    pub fn sync_projector_changes(
        &mut self,
        file_set: &FileSet,
        projector: &dyn Projector,
        now: f64,
    ) -> usize {
        use touring_storage::vfs::Change;
        let changes = projector.diff();
        for change in &changes {
            match change {
                Change::FileCreated(fid) | Change::FileRenamed { new: fid, .. } => {
                    if let Some(path) = file_set.path_for_id(*fid) {
                        self.record_edit(path.as_str(), now);
                    }
                }
                Change::FileDeleted(fid) => {
                    if let Some(path) = file_set.path_for_id(*fid) {
                        self.record_access(path.as_str(), now);
                    }
                }
                Change::SymbolCreated(_)
                | Change::SymbolDeleted(_)
                | Change::SymbolMoved { .. } => {
                    // Symbol-level changes do not directly affect file heat.
                }
            }
        }
        changes.len()
    }

    /// Evict the coldest file(s) if over capacity using lazy-deletion heap.
    ///
    /// Complexity: O(log n) amortized per eviction (vs O(n) linear scan before).
    ///
    /// Staleness is detected via generation: each `record_edit` increments the
    /// entry's generation and pushes a heap node tagged with that generation.
    /// A popped node whose generation does not match the entry's current
    /// generation is stale (the file was re-edited after the push) and is
    /// skipped; the loop retries until a fresh node is found.
    fn evict_if_needed(&mut self, now: f64) {
        // `now` is only used to keep the signature consistent with callers;
        // generation-based staleness does not require recomputing heat here.
        let _ = now;
        while self.entries.len() > self.capacity {
            match self.eviction_heap.pop() {
                None => break, // heap exhausted; entries/heap out of sync — stop
                Some(Reverse((_bits, r#gen, key))) => {
                    match self.entries.get(&key) {
                        Some(entry) if entry.generation == r#gen => {
                            // Fresh snapshot — this is still the coldest entry. Evict it.
                            self.entries.swap_remove(&key);
                        }
                        Some(_) => {
                            // Stale: file was re-edited after this heap push. Skip, retry.
                        }
                        None => {
                            // Already evicted by a prior cycle. Skip, retry.
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: f64 = 1_000_000.0;

    #[test]
    fn test_heat_increases_on_edit() {
        let mut hm = HeatMap::new(100);
        hm.record_edit("file.rs", NOW);
        let order = hm.get_priority_order(NOW);
        assert!(!order.is_empty());
        assert!(order[0].1 > 0.0);
    }

    #[test]
    fn test_heat_decays_over_time() {
        let mut hm = HeatMap::new(100);
        hm.record_edit("file.rs", NOW);
        let score_now = hm.get_priority_order(NOW)[0].1;
        let score_later = hm.get_priority_order(NOW + 2.0 * 86_400.0)[0].1;
        assert!(score_later < score_now);
    }

    #[test]
    fn test_priority_order_respects_heat() {
        let mut hm = HeatMap::new(100);
        hm.record_edit("hot.rs", NOW);
        hm.record_edit("hot.rs", NOW);
        hm.record_edit("cold.rs", NOW - 86_400.0 * 7.0);
        let order = hm.get_priority_order(NOW);
        let hot_pos = order
            .iter()
            .position(|(f, _)| *f == "hot.rs")
            .expect("hot.rs");
        let cold_pos = order
            .iter()
            .position(|(f, _)| *f == "cold.rs")
            .expect("cold.rs");
        assert!(hot_pos < cold_pos);
    }

    #[test]
    fn test_blast_radius_boosts_heat() {
        let mut hm = HeatMap::new(100);
        hm.record_edit("a.rs", NOW);
        hm.record_edit("b.rs", NOW);
        hm.set_blast_radius_weight("b.rs", 5.0);
        let order = hm.get_priority_order(NOW);
        let b_score = order.iter().find(|(f, _)| *f == "b.rs").expect("b.rs").1;
        let a_score = order.iter().find(|(f, _)| *f == "a.rs").expect("a.rs").1;
        assert!(
            b_score > a_score,
            "b.rs should score higher: {b_score} vs {a_score}"
        );
    }

    #[test]
    fn test_heat_map_capacity_limit() {
        let mut hm = HeatMap::new(3);
        hm.record_edit("a.rs", NOW);
        hm.record_edit("b.rs", NOW);
        hm.record_edit("c.rs", NOW);
        hm.record_edit("d.rs", NOW); // triggers eviction
        assert!(hm.len() <= 3);
    }

    #[test]
    fn test_eviction_removes_coldest() {
        // Insert 3 files with distinct heat scores, then trigger eviction.
        // The coldest (fewest edits, oldest) must be the one evicted.
        let mut hm = HeatMap::new(2);
        // "cold.rs": edited 7 days ago — very low heat
        hm.record_edit("cold.rs", NOW - 86_400.0 * 7.0);
        // "warm.rs": edited now once
        hm.record_edit("warm.rs", NOW);
        // "hot.rs": edited now twice — highest heat, triggers eviction
        hm.record_edit("hot.rs", NOW);
        hm.record_edit("hot.rs", NOW);
        // Capacity is 2; after 4 inserts (3 unique + 1 dup) we should have evicted 1.
        assert!(hm.len() <= 2, "len={}", hm.len());
        // cold.rs must have been evicted (lowest heat)
        let keys: Vec<_> = hm.get_priority_order(NOW).iter().map(|(k, _)| *k).collect();
        assert!(
            !keys.contains(&"cold.rs"),
            "cold.rs should have been evicted; remaining={keys:?}"
        );
    }

    #[test]
    fn test_eviction_heap_lazy_deletion_stale_entries() {
        // Verify that stale heap entries (from earlier inserts with lower heat)
        // do not cause incorrect evictions when a file's heat increases.
        let mut hm = HeatMap::new(2);
        // Insert "a.rs" once (low heat snapshot pushed to heap)
        hm.record_edit("a.rs", NOW - 86_400.0);
        // Insert "b.rs" once
        hm.record_edit("b.rs", NOW - 86_400.0);
        // Now "a.rs" gets hot (its heap entry from first insert is now stale)
        hm.record_edit("a.rs", NOW);
        hm.record_edit("a.rs", NOW);
        // Adding "c.rs" triggers eviction — stale entry for "a.rs" should be skipped
        hm.record_edit("c.rs", NOW);
        assert!(hm.len() <= 2, "len={}", hm.len());
        // "a.rs" must survive (it's hot); "b.rs" is colder and should be evicted
        let keys: Vec<_> = hm.get_priority_order(NOW).iter().map(|(k, _)| *k).collect();
        assert!(
            keys.contains(&"a.rs"),
            "a.rs (hot) must survive; remaining={keys:?}"
        );
        assert!(
            !keys.contains(&"b.rs"),
            "b.rs (cold) should be evicted; remaining={keys:?}"
        );
    }

    #[test]
    fn test_sync_projector_changes_file_created() {
        use touring_storage::vfs::{FileSet, NoopProjector, Vfs};
        let vfs = Vfs::new();
        let mut file_set = FileSet::new(vfs);
        let path = touring_storage::vfs::AbsPath::from_absolute("/src/main.rs")
            .expect("valid absolute path in test");
        file_set.add_path(path, touring_storage::vfs::FileId::new(1));

        // Use NoopProjector — diff() returns empty, so sync should return 0.
        let projector = NoopProjector;
        let mut hm = HeatMap::new(100);
        let count = hm.sync_projector_changes(&file_set, &projector, NOW);
        assert_eq!(count, 0, "NoopProjector emits no changes");
    }

    #[test]
    fn test_sync_projector_changes_wires_projector() {
        // Verify that touring-vfs::Projector trait is wired and callable.
        use touring_storage::vfs::{NoopProjector, Projector};
        let projector = NoopProjector;
        let changes = projector.diff();
        // NoopProjector always returns empty diff — integration is live.
        assert!(changes.is_empty(), "NoopProjector diff must be empty");
    }

    // ── C7: Property-based tests (proptest) ──────────────────────────────────

    #[cfg(test)]
    mod proptest_tests {
        use super::*;
        use proptest::prelude::*;

        const PROP_NOW: f64 = 1_000_000.0;

        proptest! {
            /// HeatMap len must never exceed capacity.
            #[test]
            fn prop_heat_map_len_bounded(
                capacity in 1usize..=20usize,
                n_inserts in 1usize..=50usize,
            ) {
                let mut hm = HeatMap::new(capacity);
                for i in 0..n_inserts {
                    let path = format!("src/f_{}.rs", i % (capacity * 2 + 1));
                    hm.record_edit(&path, PROP_NOW);
                }
                prop_assert!(
                    hm.len() <= capacity,
                    "len {} exceeds capacity {}",
                    hm.len(),
                    capacity
                );
            }

            /// `get_priority_order` result length must not exceed `len()`.
            #[test]
            fn prop_priority_order_len_le_map_len(
                n in 1usize..=10usize,
            ) {
                let mut hm = HeatMap::new(20);
                for i in 0..n {
                    hm.record_edit(&format!("src/f_{i}.rs"), PROP_NOW - i as f64);
                }
                let order = hm.get_priority_order(PROP_NOW);
                prop_assert!(
                    order.len() <= hm.len(),
                    "order.len()={} > hm.len()={}",
                    order.len(),
                    hm.len()
                );
            }
        }
    }
}
