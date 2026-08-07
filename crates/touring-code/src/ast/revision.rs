//! Salsa-inspired incremental computation with durability tiers and hash-based early cutoff.
//!
//! # Architecture
//!
//! This module implements three concepts from the Salsa framework (used by rust-analyzer):
//!
//! 1. **Durability tiers** — Inputs are classified by how often they change:
//!    - `High` — grammars, builtins, stdlib (rarely changes)
//!    - `Medium` — project config, Cargo.toml (occasionally changes)
//!    - `Low` — user source files (changes on every keystroke)
//!
//! 2. **Revision tracking** — A per-tier monotonic counter. Bumping a tier also bumps
//!    all lower tiers. This lets us skip revalidation of high-durability caches when
//!    only low-durability inputs change.
//!
//! 3. **Hash-based early cutoff** — After recomputing derived data (e.g., symbol table),
//!    compare its hash against the previous result. If unchanged, do NOT propagate
//!    invalidation downstream. This prevents unnecessary cascading recomputation.

use std::cell::Cell;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use crate::ast::graph::SymbolLocation;

// ─── Durability ────────────────────────────────────────────────────────────

/// Durability tier for cache entries — how often the underlying data changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Durability {
    /// Frequently changes: user source files, active edits.
    Low = 0,
    /// Occasionally changes: project config, Cargo.toml, tsconfig.json.
    Medium = 1,
    /// Rarely changes: grammars, builtins, stdlib symbols, interned values.
    High = 2,
}

impl Durability {
    /// Classify a file path into a durability tier.
    pub fn for_path(path: &Path) -> Self {
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

        match name {
            // Project config files change occasionally
            "Cargo.toml" | "package.json" | "tsconfig.json" | "pyproject.toml" | "setup.py"
            | "setup.cfg" | ".eslintrc.json" | "tailwind.config.js" | "next.config.js"
            | "next.config.ts" | "vite.config.ts" => Durability::Medium,
            _ => {
                // Check if path is in a "vendor" or "stdlib" directory
                let path_str = path.to_str().unwrap_or("");
                if path_str.contains("/vendor/")
                    || path_str.contains("node_modules/")
                    || path_str.contains("/.rustup/")
                    || path_str.contains("/site-packages/")
                {
                    Durability::High
                } else {
                    Durability::Low
                }
            }
        }
    }
}

// ─── RevisionTracker ───────────────────────────────────────────────────────

/// Tracks revision numbers per durability tier.
///
/// When a tier is bumped, all lower tiers are also bumped. This means:
/// - Bumping `Low` only increments the `Low` counter.
/// - Bumping `Medium` increments both `Medium` and `Low`.
/// - Bumping `High` increments all three.
#[derive(Debug, Clone)]
pub struct RevisionTracker {
    /// Monotonically increasing revision per tier: [Low, Medium, High].
    revisions: [u64; 3],
}

impl Default for RevisionTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl RevisionTracker {
    /// Create a new tracker with all revisions at 0.
    pub fn new() -> Self {
        Self { revisions: [0; 3] }
    }

    /// Bump the revision for a durability tier (and all lower tiers).
    pub fn bump(&mut self, durability: Durability) {
        let idx = durability as usize;
        // Bump this tier and all lower tiers
        for i in 0..=idx {
            if let Some(r) = self.revisions.get_mut(i) {
                *r += 1;
            }
        }
    }

    /// Get the current revision for a tier.
    pub fn current(&self, durability: Durability) -> u64 {
        self.revisions
            .get(durability as usize)
            .copied()
            .unwrap_or(0)
    }

    /// Check if a cached entry needs revalidation.
    ///
    /// Returns `true` if the tier's revision has advanced past `cached_revision`.
    pub fn needs_revalidation(&self, cached_revision: u64, durability: Durability) -> bool {
        self.revisions
            .get(durability as usize)
            .copied()
            .unwrap_or(0)
            > cached_revision
    }
}

// ─── SymbolCache ───────────────────────────────────────────────────────────

/// Hash of a symbol set — used for early cutoff comparison.
fn hash_symbols(symbols: &[SymbolLocation]) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for sym in symbols {
        sym.symbol_name.hash(&mut hasher);
        sym.line.hash(&mut hasher);
        sym.column.hash(&mut hasher);
        sym.is_definition.hash(&mut hasher);
    }
    symbols.len().hash(&mut hasher);
    hasher.finish()
}

/// A cached symbol extraction result with revision and hash metadata.
#[derive(Debug, Clone)]
struct CachedSymbolEntry {
    /// The cached symbols.
    symbols: Vec<SymbolLocation>,
    /// Hash of the symbols for early cutoff.
    content_hash: u64,
    /// Revision when this entry was last verified.
    verified_at: u64,
    /// Durability tier of the source file.
    durability: Durability,
}

/// Cache of per-file symbol extraction results with revision-based invalidation.
#[derive(Debug, Clone)]
pub struct SymbolCache {
    entries: HashMap<PathBuf, CachedSymbolEntry>,
    revisions: RevisionTracker,
    /// Count of early cutoffs (hash match after recomputation).
    pub early_cutoffs: u64,
    /// Count of full recomputations (hash changed).
    pub recomputations: u64,
    /// Count of cache hits (revision still valid, no recomputation needed).
    pub cache_hits: Cell<u64>,
}

impl Default for SymbolCache {
    fn default() -> Self {
        Self::new()
    }
}

impl SymbolCache {
    /// Create an empty symbol cache with default capacity.
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
            revisions: RevisionTracker::new(),
            early_cutoffs: 0,
            recomputations: 0,
            cache_hits: Cell::new(0),
        }
    }

    /// Create a symbol cache pre-allocated to hold `capacity` entries.
    ///
    /// Useful when processing large workspaces where the symbol count is
    /// known upfront, avoiding early HashMap rehashing during batch indexing.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            entries: HashMap::with_capacity(capacity),
            revisions: RevisionTracker::new(),
            early_cutoffs: 0,
            recomputations: 0,
            cache_hits: Cell::new(0),
        }
    }

    /// Notify the cache that a file has been edited.
    ///
    /// Bumps the revision for the file's durability tier.
    pub fn notify_edit(&mut self, file_path: &Path) {
        let durability = Durability::for_path(file_path);
        self.revisions.bump(durability);
    }

    /// Try to get cached symbols for a file without recomputation.
    ///
    /// Returns `Some(&[SymbolLocation])` if the cache entry is still valid
    /// (revision hasn't changed for this file's durability tier).
    /// Returns `None` if the entry is stale or missing.
    pub fn get_if_valid(&self, file_path: &Path) -> Option<&[SymbolLocation]> {
        let entry = self.entries.get(file_path)?;
        if !self
            .revisions
            .needs_revalidation(entry.verified_at, entry.durability)
        {
            self.cache_hits.set(self.cache_hits.get() + 1);
            Some(&entry.symbols)
        } else {
            None
        }
    }

    /// Update the cache with recomputed symbols.
    ///
    /// Performs hash-based early cutoff: if the new symbols hash to the same
    /// value as the cached entry, marks the entry as verified without
    /// propagating invalidation.
    ///
    /// Returns `true` if symbols actually changed (hash differs),
    /// `false` if early cutoff applied (hash matches — no downstream propagation needed).
    pub fn update(&mut self, file_path: &Path, new_symbols: Vec<SymbolLocation>) -> bool {
        let durability = Durability::for_path(file_path);
        let new_hash = hash_symbols(&new_symbols);
        let current_rev = self.revisions.current(durability);

        if let Some(entry) = self.entries.get_mut(&file_path.to_path_buf())
            && entry.content_hash == new_hash
        {
            // EARLY CUTOFF: symbols unchanged — update verified_at but
            // DON'T signal downstream invalidation.
            entry.verified_at = current_rev;
            self.early_cutoffs += 1;
            return false;
        }

        // Symbols changed — update cache entry.
        self.entries.insert(
            file_path.to_path_buf(),
            CachedSymbolEntry {
                symbols: new_symbols,
                content_hash: new_hash,
                verified_at: current_rev,
                durability,
            },
        );
        self.recomputations += 1;
        true
    }

    /// Remove a file from the cache.
    pub fn remove(&mut self, file_path: &Path) {
        self.entries.remove(file_path);
    }

    /// Clear all cached entries.
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Number of cached files.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Get a reference to the revision tracker.
    pub fn revisions(&self) -> &RevisionTracker {
        &self.revisions
    }

    /// Cache statistics: (hits, early_cutoffs, recomputations).
    pub fn stats(&self) -> (u64, u64, u64) {
        (
            self.cache_hits.get(),
            self.early_cutoffs,
            self.recomputations,
        )
    }
}

// ─── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_durability_for_path() {
        assert_eq!(
            Durability::for_path(Path::new("src/main.rs")),
            Durability::Low
        );
        assert_eq!(
            Durability::for_path(Path::new("Cargo.toml")),
            Durability::Medium
        );
        assert_eq!(
            Durability::for_path(Path::new("/home/user/.rustup/toolchains/stable/lib.rs")),
            Durability::High
        );
        assert_eq!(
            Durability::for_path(Path::new("node_modules/react/index.js")),
            Durability::High
        );
    }

    #[test]
    fn test_revision_tracker_bump() {
        let mut tracker = RevisionTracker::new();
        assert_eq!(tracker.current(Durability::Low), 0);
        assert_eq!(tracker.current(Durability::Medium), 0);
        assert_eq!(tracker.current(Durability::High), 0);

        // Bump Low — only Low advances
        tracker.bump(Durability::Low);
        assert_eq!(tracker.current(Durability::Low), 1);
        assert_eq!(tracker.current(Durability::Medium), 0);
        assert_eq!(tracker.current(Durability::High), 0);

        // Bump Medium — Low and Medium advance
        tracker.bump(Durability::Medium);
        assert_eq!(tracker.current(Durability::Low), 2);
        assert_eq!(tracker.current(Durability::Medium), 1);
        assert_eq!(tracker.current(Durability::High), 0);

        // Bump High — all advance
        tracker.bump(Durability::High);
        assert_eq!(tracker.current(Durability::Low), 3);
        assert_eq!(tracker.current(Durability::Medium), 2);
        assert_eq!(tracker.current(Durability::High), 1);
    }

    #[test]
    fn test_revision_needs_revalidation() {
        let mut tracker = RevisionTracker::new();

        // Cache at revision 0 for Low
        let cached_rev = tracker.current(Durability::Low);
        assert!(!tracker.needs_revalidation(cached_rev, Durability::Low));

        // Bump Low — cached entry is stale
        tracker.bump(Durability::Low);
        assert!(tracker.needs_revalidation(cached_rev, Durability::Low));
        // Medium was NOT bumped — medium-durability caches still valid
        assert!(!tracker.needs_revalidation(0, Durability::Medium));
    }

    #[test]
    fn test_symbol_cache_early_cutoff() {
        let mut cache = SymbolCache::new();
        let path = Path::new("src/main.rs");

        let symbols = vec![
            SymbolLocation::new("src/main.rs", "main".to_string(), 1, 0, true),
            SymbolLocation::new("src/main.rs", "helper".to_string(), 10, 0, true),
        ];

        // First insert — no early cutoff
        let changed = cache.update(path, symbols.clone());
        assert!(changed, "First insert should report changed");
        assert_eq!(cache.recomputations, 1);
        assert_eq!(cache.early_cutoffs, 0);

        // Simulate an edit that doesn't change symbols
        cache.notify_edit(path);

        // Re-insert same symbols — early cutoff
        let changed = cache.update(path, symbols.clone());
        assert!(!changed, "Same symbols should trigger early cutoff");
        assert_eq!(cache.early_cutoffs, 1);
        assert_eq!(cache.recomputations, 1);

        // Insert different symbols — real change
        let new_symbols = vec![
            SymbolLocation::new("src/main.rs", "main".to_string(), 1, 0, true),
            SymbolLocation::new("src/main.rs", "new_func".to_string(), 20, 0, true),
        ];
        let changed = cache.update(path, new_symbols);
        assert!(changed, "Different symbols should report changed");
        assert_eq!(cache.recomputations, 2);
    }

    #[test]
    fn test_symbol_cache_get_if_valid() {
        let mut cache = SymbolCache::new();
        let path = Path::new("src/main.rs");

        // No entry yet
        assert!(cache.get_if_valid(path).is_none());

        let symbols = vec![SymbolLocation::new(
            "src/main.rs",
            "main".to_string(),
            1,
            0,
            true,
        )];
        cache.update(path, symbols);

        // Valid — no edits since caching
        assert!(cache.get_if_valid(path).is_some());
        assert_eq!(cache.cache_hits.get(), 1);

        // Edit the file — cache becomes stale
        cache.notify_edit(path);
        assert!(cache.get_if_valid(path).is_none());
    }

    #[test]
    fn test_high_durability_survives_low_bump() {
        let mut cache = SymbolCache::new();
        let stdlib = Path::new("/home/user/.rustup/toolchains/stable/lib.rs");
        let user_file = Path::new("src/main.rs");

        // Cache stdlib symbols (High durability)
        let stdlib_syms = vec![SymbolLocation::new("stdlib", "Vec".to_string(), 1, 0, true)];
        cache.update(stdlib, stdlib_syms);

        // Cache user file (Low durability)
        let user_syms = vec![SymbolLocation::new(
            "src/main.rs",
            "main".to_string(),
            1,
            0,
            true,
        )];
        cache.update(user_file, user_syms);

        // Edit user file — only Low tier bumped
        cache.notify_edit(user_file);

        // Stdlib cache should still be valid (High durability, Low bump doesn't affect it)
        assert!(
            cache.get_if_valid(stdlib).is_some(),
            "High-durability cache should survive Low bump"
        );

        // User file cache should be stale
        assert!(
            cache.get_if_valid(user_file).is_none(),
            "Low-durability cache should be stale"
        );
    }

    #[test]
    fn test_hash_symbols_deterministic() {
        let symbols = vec![
            SymbolLocation::new("f.rs", "a".to_string(), 1, 0, true),
            SymbolLocation::new("f.rs", "b".to_string(), 5, 0, true),
        ];
        let h1 = hash_symbols(&symbols);
        let h2 = hash_symbols(&symbols);
        assert_eq!(h1, h2, "Same input should produce same hash");

        let different = vec![
            SymbolLocation::new("f.rs", "a".to_string(), 1, 0, true),
            SymbolLocation::new("f.rs", "c".to_string(), 5, 0, true),
        ];
        let h3 = hash_symbols(&different);
        assert_ne!(h1, h3, "Different input should produce different hash");
    }
}
