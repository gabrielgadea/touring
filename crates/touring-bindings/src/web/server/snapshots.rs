//! Persistent quality-snapshot ring buffer (Wave 4 P10.9).
//!
//! Stores up to `capacity` snapshots in memory and mirrors every append
//! to a JSONL file on disk so the dashboard can recover history across
//! restarts. Append is synchronous + best-effort: file errors do not
//! poison the in-memory ring (logged via `tracing::warn!`).
//!
//! Public surface:
//! - [`SnapshotStore::new`] — bootstrap from disk.
//! - [`SnapshotStore::append`] — add a snapshot (in-memory + disk).
//! - [`SnapshotStore::recent`] — last `limit` snapshots, newest first.

use std::collections::VecDeque;
use std::io::Write as _;
use std::path::PathBuf;
use std::sync::Mutex;

use serde_json::Value;

/// Default ring capacity. Roughly 1.6 hours at one snapshot per 6s.
pub const DEFAULT_CAPACITY: usize = 1000;

/// In-memory ring buffer + JSONL file mirror for dashboard snapshots.
pub struct SnapshotStore {
    ring: Mutex<VecDeque<Value>>,
    path: PathBuf,
    capacity: usize,
}

impl SnapshotStore {
    /// Create a new store. Reads existing JSONL into the ring (last
    /// `capacity` lines) so dashboard reload preserves history.
    pub fn new(path: PathBuf, capacity: usize) -> Self {
        let mut ring = VecDeque::with_capacity(capacity);
        if let Ok(text) = std::fs::read_to_string(&path) {
            for line in text.lines().rev().take(capacity) {
                if let Ok(v) = serde_json::from_str::<Value>(line) {
                    ring.push_front(v);
                }
            }
        }
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        Self {
            ring: Mutex::new(ring),
            path,
            capacity,
        }
    }

    /// Append a snapshot. Best-effort disk mirror.
    pub fn append(&self, snap: Value) {
        if let Ok(mut g) = self.ring.lock() {
            if g.len() >= self.capacity {
                g.pop_front();
            }
            g.push_back(snap.clone());
        }
        match std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
        {
            Ok(mut f) => {
                if let Ok(line) = serde_json::to_string(&snap) {
                    let _ = writeln!(f, "{}", line);
                }
            }
            _ => {
                tracing::warn!("snapshot disk mirror unavailable: {}", self.path.display());
            }
        }
    }

    /// Last `limit` snapshots, newest first.
    pub fn recent(&self, limit: usize) -> Vec<Value> {
        let g = match self.ring.lock() {
            Ok(g) => g,
            Err(_) => return Vec::new(),
        };
        g.iter().rev().take(limit).cloned().collect()
    }

    /// Current ring size.
    pub fn len(&self) -> usize {
        self.ring.lock().map(|g| g.len()).unwrap_or(0)
    }

    /// True when ring is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn append_and_recent_round_trip() {
        let dir = std::env::temp_dir().join("touring-test-snap-rt");
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("snap.jsonl");
        let store = SnapshotStore::new(path.clone(), 4);
        for i in 0..6_u64 {
            store.append(json!({"i": i}));
        }
        let recent = store.recent(10);
        assert_eq!(recent.len(), 4);
        assert_eq!(recent[0]["i"], 5);
        assert_eq!(recent[3]["i"], 2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn restores_from_disk() {
        let dir = std::env::temp_dir().join("touring-test-snap-restore");
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("snap.jsonl");
        {
            let s = SnapshotStore::new(path.clone(), 3);
            s.append(json!({"a": 1}));
            s.append(json!({"a": 2}));
            s.append(json!({"a": 3}));
        }
        let s2 = SnapshotStore::new(path.clone(), 3);
        let recent = s2.recent(3);
        assert_eq!(recent.len(), 3);
        assert_eq!(recent[0]["a"], 3);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
