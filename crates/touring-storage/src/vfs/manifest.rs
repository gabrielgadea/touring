//! File manifest — tracks content hashes for incremental dedup and move detection.
//!
//! The manifest maintains a mapping from file path → content hash (blake3).
//! On index updates, `detect_moves()` identifies files that were renamed
//! (same hash, new path) so the indexer can skip re-chunking and just update paths.
//!
//! # Algorithm
//!
//! For each path in `new_paths`:
//! 1. Compute blake3 hash of the file content
//! 2. If hash exists in manifest with a **different** path → `MoveEvent`
//! 3. If hash exists in manifest with the **same** path → duplicate (same content)
//! 4. If hash is not in manifest → new file
//!
//! When multiple `new_paths` share the same hash:
//! - The one with the most-recent mtime is classified FIRST (move if hash tracked else new)
//! - The rest are duplicates

use hashbrown::HashMap;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::time::UNIX_EPOCH;

/// A detected file move (same content, different path).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MoveEvent {
    /// Original path (where the file WAS)
    pub from: PathBuf,
    /// New path (where the file IS now)
    pub to: PathBuf,
    /// Blake3 hash of the file content (stable across move)
    pub content_hash: [u8; 32],
}

/// The manifest tracks known files by path + content hash.
/// Used to detect moves during incremental indexing.
/// Serialization format for FileManifest (entries as flat Vec for serde compat).
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ManifestData {
    entries: Vec<(PathBuf, [u8; 32], Option<u64>)>,
}

/// Persistent record of each tracked file's content hash and modification time,
/// used to detect changes between indexing runs.
#[derive(Debug, Clone, Default)]
pub struct FileManifest {
    /// path → (content_hash, mtime_secs)
    entries: HashMap<PathBuf, (ContentHashEntry, Option<u64>)>,
}

impl serde::Serialize for FileManifest {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let data: Vec<(PathBuf, [u8; 32], Option<u64>)> = self
            .entries
            .iter()
            .map(|(p, (h, m))| (p.clone(), *h.as_bytes(), *m))
            .collect();
        ManifestData { entries: data }.serialize(serializer)
    }
}

impl<'de> serde::Deserialize<'de> for FileManifest {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let data = ManifestData::deserialize(deserializer)?;
        let entries = data
            .entries
            .into_iter()
            .map(|(p, h, m)| (p, (ContentHashEntry(h), m)))
            .collect();
        Ok(FileManifest { entries })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
struct ContentHashEntry([u8; 32]);

impl ContentHashEntry {
    fn from_hash(hash: [u8; 32]) -> Self {
        Self(hash)
    }

    fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// Result of detecting moves among a set of new paths.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MoveDetectionResult {
    /// Confirmed move events (same hash, different path)
    pub moves: Vec<MoveEvent>,
    /// Paths that are duplicates of existing files (same hash, same content, different inode)
    pub duplicates: Vec<PathBuf>,
    /// Paths that are entirely new (hash not in manifest)
    pub new_files: Vec<PathBuf>,
}

impl FileManifest {
    /// Create an empty manifest.
    pub fn new() -> Self {
        Self::default()
    }

    /// Detect moves, duplicates, and new files among `new_paths`.
    ///
    /// For each path in `new_paths`:
    /// - If hash matches an existing entry with a **different** path → `MoveEvent`
    /// - If hash matches an existing entry with the **same** path → duplicate
    /// - If hash is not in manifest → new file
    ///
    /// When multiple `new_paths` share the same hash:
    /// - First-encountered (sorted by mtime desc) checks if hash is in manifest
    ///   → If yes: MOVE | If no: NEW FILE
    /// - Subsequent ones with same hash → DUPLICATE
    pub fn detect_moves(&self, new_paths: &[PathBuf]) -> MoveDetectionResult {
        let mut result = MoveDetectionResult::default();

        // Compute (path, hash, mtime) for all new paths
        let mut path_hashes: Vec<(PathBuf, [u8; 32], u64)> = Vec::new();
        for p in new_paths {
            if let Some(hash) = compute_hash(p) {
                let mtime = mtime_of(p).unwrap_or(0);
                path_hashes.push((p.clone(), hash, mtime));
            } else {
                result.new_files.push(p.clone());
            }
        }

        // Sort by mtime descending so most-recent is processed first
        path_hashes.sort_by_key(|b| std::cmp::Reverse(b.2));

        // Group by hash to handle multi-way duplicates
        let mut groups: HashMap<[u8; 32], Vec<PathBuf>> = HashMap::new();
        for (p, h, _) in &path_hashes {
            groups.entry(*h).or_default().push(p.clone());
        }

        // Reverse index of manifest: hash → (path, mtime) sorted by mtime desc
        let mut manifest_locs: HashMap<[u8; 32], Vec<(PathBuf, u64)>> = HashMap::new();
        for (p, (h, m)) in &self.entries {
            manifest_locs
                .entry(*h.as_bytes())
                .or_default()
                .push((p.clone(), m.unwrap_or(0)));
        }

        for (hash, paths) in groups {
            let manifest_entry = manifest_locs.get(&hash);

            // First path for this hash: move if manifest has it, else new file
            let first = paths.first().expect("paths non-empty");
            if let Some(locs) = manifest_entry {
                // Manifest has this hash at a tracked path → MOVE
                // Pick the most-recent manifest location
                let (from_path, _) = locs.iter().max_by_key(|(_, m)| m).expect("non-empty locs");
                result.moves.push(MoveEvent {
                    from: from_path.clone(),
                    to: first.clone(),
                    content_hash: hash,
                });
            } else {
                // No manifest entry → first new path is the original (new file)
                result.new_files.push(first.clone());
            }

            // Remaining paths with same hash: duplicates
            for p in paths.iter().skip(1) {
                result.duplicates.push(p.clone());
            }
        }

        result
    }

    /// Apply a move event to the manifest.
    /// Removes the old path entry and adds the new path entry with the same hash.
    pub fn apply_move(&mut self, ev: &MoveEvent) {
        self.entries.remove(&ev.from);
        self.entries.insert(
            ev.to.clone(),
            (ContentHashEntry::from_hash(ev.content_hash), None),
        );
    }

    /// Insert or update a path → hash entry.
    pub fn insert(&mut self, path: PathBuf, hash: [u8; 32]) {
        let mtime = mtime_of(&path);
        self.entries
            .insert(path, (ContentHashEntry::from_hash(hash), mtime));
    }

    /// Number of tracked entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Get the hash for a path, if tracked.
    pub fn hash_of(&self, path: &PathBuf) -> Option<[u8; 32]> {
        self.entries.get(path).map(|(h, _)| *h.as_bytes())
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────────────

/// Compute blake3 hash of file content at `path`.
fn compute_hash(path: &PathBuf) -> Option<[u8; 32]> {
    let content = fs::read(path).ok()?;
    Some(content_hash(&content))
}

/// Compute blake3 hash of raw bytes.
pub fn content_hash(content: &[u8]) -> [u8; 32] {
    let hash = blake3::hash(content);
    let mut out = [0u8; 32];
    out.copy_from_slice(hash.as_bytes());
    out
}

/// Get modification time of a path in seconds since UNIX_EPOCH.
fn mtime_of(path: &PathBuf) -> Option<u64> {
    fs::metadata(path)
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn tempfile(content: &[u8]) -> (TempDir, PathBuf) {
        let dir = TempDir::new().expect("TempDir");
        let path = dir.path().join("testfile");
        std::fs::write(&path, content).expect("write");
        (dir, path)
    }

    #[test]
    fn manifest_new_is_empty() {
        let m = FileManifest::new();
        assert!(m.is_empty());
        assert_eq!(m.len(), 0);
    }

    #[test]
    fn manifest_insert_and_hash_of() {
        let mut m = FileManifest::new();
        let (_dir, path) = tempfile(b"hello world");
        let hash = compute_hash(&path).expect("hash");
        m.insert(path.clone(), hash);
        assert_eq!(m.len(), 1);
        assert_eq!(m.hash_of(&path), Some(hash));
    }

    #[test]
    fn manifest_detect_new_file() {
        let m = FileManifest::new();
        let (_dir, path) = tempfile(b"brand new content");
        let result = m.detect_moves(std::slice::from_ref(&path));
        assert_eq!(result.moves.len(), 0);
        assert_eq!(result.duplicates.len(), 0);
        assert_eq!(result.new_files.len(), 1);
        assert_eq!(result.new_files[0], path);
    }

    #[test]
    fn manifest_detect_move() {
        let mut m = FileManifest::new();
        let (_dir1, old_path) = tempfile(b"same content");
        let hash = compute_hash(&old_path).expect("hash");
        m.insert(old_path.clone(), hash);

        let tmp = TempDir::new().expect("TempDir");
        let new_path = tmp.path().join("moved_file");
        std::fs::write(&new_path, b"same content").expect("write");

        let result = m.detect_moves(std::slice::from_ref(&new_path));
        assert_eq!(result.moves.len(), 1);
        assert_eq!(result.moves[0].from, old_path);
        assert_eq!(result.moves[0].to, new_path);
        assert_eq!(result.moves[0].content_hash, hash);
    }

    #[test]
    fn manifest_detect_duplicate_vs_move() {
        // When a hash is tracked in the manifest at old_path,
        // a new file with that hash is a MOVE (not a duplicate).
        let mut m = FileManifest::new();
        let (_dir1, old_path) = tempfile(b"content");
        let hash = compute_hash(&old_path).expect("hash");
        m.insert(old_path.clone(), hash);

        let tmp = TempDir::new().expect("TempDir");
        let dup_path = tmp.path().join("duplicate");
        std::fs::write(&dup_path, b"content").expect("write");

        let result = m.detect_moves(std::slice::from_ref(&dup_path));
        // dup_path is MOVE (old_path→dup_path), NOT duplicate
        assert_eq!(result.moves.len(), 1);
        assert_eq!(result.moves[0].from, old_path);
        assert_eq!(result.moves[0].to, dup_path);
        assert_eq!(result.duplicates.len(), 0);
    }

    #[test]
    fn manifest_apply_move() {
        let mut m = FileManifest::new();
        let (_dir1, old_path) = tempfile(b"content");
        let hash = compute_hash(&old_path).expect("hash");
        m.insert(old_path.clone(), hash);

        let tmp = TempDir::new().expect("TempDir");
        let new_path = tmp.path().join("moved");
        std::fs::write(&new_path, b"content").expect("write");

        let ev = MoveEvent {
            from: old_path.clone(),
            to: new_path.clone(),
            content_hash: hash,
        };

        m.apply_move(&ev);

        assert_eq!(m.hash_of(&old_path), None);
        assert_eq!(m.hash_of(&new_path), Some(hash));
    }

    #[test]
    fn manifest_modified_file_not_a_move() {
        let mut m = FileManifest::new();
        let (_dir, path) = tempfile(b"old content");
        let old_hash = compute_hash(&path).expect("hash");
        m.insert(path.clone(), old_hash);

        std::fs::write(&path, b"new content").expect("write");
        let new_hash = compute_hash(&path).expect("hash");

        let result = m.detect_moves(std::slice::from_ref(&path));
        assert_eq!(result.moves.len(), 0);
        assert_eq!(result.duplicates.len(), 0);
        assert_eq!(result.new_files.len(), 1);
        assert_eq!(m.hash_of(&path), Some(old_hash));
        assert_ne!(old_hash, new_hash);
    }

    #[test]
    fn manifest_multiple_no_manifest() {
        // No path with this hash in manifest.
        // First (by mtime desc) → new_file. Rest → duplicates.
        let m = FileManifest::new();

        let tmp = TempDir::new().expect("TempDir");
        let path2 = tmp.path().join("dup2");
        let path3 = tmp.path().join("dup3");
        std::fs::write(&path2, b"shared").expect("write");
        std::fs::write(&path3, b"shared").expect("write");

        let result = m.detect_moves(&[path2.clone(), path3.clone()]);
        assert_eq!(result.moves.len(), 0);
        assert_eq!(result.new_files.len(), 1);
        assert_eq!(result.duplicates.len(), 1);
    }

    #[test]
    fn manifest_unhashable_file_goes_to_new_files() {
        let m = FileManifest::new();
        let result = m.detect_moves(&[PathBuf::from("/nonexistent/file.txt")]);
        assert_eq!(result.new_files.len(), 1);
    }
}
