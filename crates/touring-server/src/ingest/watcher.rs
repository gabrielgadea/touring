//! JsonlWatcher — periodic file poller for live JSONL ingestion
//!
//! Polls watched JSONL files on a configurable interval, reads new lines
//! from the last known offset, parses them, and stores in MemoryStore
//! with auto-embedding via GPU service.

use super::parser::{JsonlParser, SourceType};
use crate::memory_store::{MemoryEntry, MemoryStore};
use touring_storage::embedding::{Embedder, GpuEmbedder};

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::{Duration, interval};

/// Persisted state for tracking file read offsets
#[derive(Debug, Default, Serialize, Deserialize)]
pub(crate) struct WatcherState {
    /// Map of file path → byte offset of last read position
    pub offsets: HashMap<String, u64>,
}

impl WatcherState {
    /// Load state from JSON file
    pub(crate) fn load(path: &Path) -> Self {
        if path.exists() {
            std::fs::read_to_string(path)
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_default()
        } else {
            Self::default()
        }
    }

    /// Save state to JSON file
    pub(crate) fn save(&self, path: &Path) -> std::io::Result<()> {
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(path, json)
    }
}

/// Configuration for the watcher
#[derive(Debug, Clone)]
pub struct WatcherConfig {
    /// Files to watch (resolved paths)
    pub watch_paths: Vec<PathBuf>,
    /// Poll interval in seconds
    pub poll_interval_s: u64,
    /// State file for offset persistence
    pub state_path: PathBuf,
    /// Batch size for GPU embedding calls
    pub embed_batch_size: usize,
}

impl Default for WatcherConfig {
    fn default() -> Self {
        Self {
            watch_paths: Vec::new(),
            poll_interval_s: 30,
            state_path: PathBuf::from(".claude/touring/watcher_state.json"),
            embed_batch_size: 32,
        }
    }
}

/// Statistics from a single poll cycle
#[derive(Debug, Default)]
pub struct PollStats {
    /// Number of files inspected during the poll.
    pub files_checked: usize,
    /// Number of new lines discovered since the last poll.
    pub new_lines: usize,
    /// Number of lines successfully parsed.
    pub parsed: usize,
    /// Number of parsed entries persisted to the store.
    pub stored: usize,
    /// Number of lines that failed to parse or store.
    pub errors: usize,
}

/// Live JSONL file watcher with periodic polling
#[derive(Debug)]
pub struct JsonlWatcher {
    config: WatcherConfig,
    store: Arc<Mutex<MemoryStore>>,
    embedder: Option<Arc<GpuEmbedder>>,
    state: WatcherState,
}

impl JsonlWatcher {
    /// Create a new watcher
    pub fn new(
        config: WatcherConfig,
        store: Arc<Mutex<MemoryStore>>,
        embedder: Option<Arc<GpuEmbedder>>,
    ) -> Self {
        let state = WatcherState::load(&config.state_path);
        Self {
            config,
            store,
            embedder,
            state,
        }
    }

    /// Run the watcher loop (blocking, call from tokio::spawn)
    pub async fn run(mut self) {
        let mut ticker = interval(Duration::from_secs(self.config.poll_interval_s));

        loop {
            ticker.tick().await;

            match self.poll_once().await {
                Ok(stats) => {
                    if stats.new_lines > 0 {
                        tracing::info!(
                            "Watcher poll: {} new lines, {} stored, {} errors",
                            stats.new_lines,
                            stats.stored,
                            stats.errors,
                        );
                    }
                }
                Err(e) => {
                    tracing::warn!("Watcher poll error: {}", e);
                }
            }
        }
    }

    /// Single poll cycle — check all watched files for new data.
    ///
    /// Returns [`crate::ServerError::Watcher`] when a fatal poll error occurs.
    pub async fn poll_once(&mut self) -> Result<PollStats, crate::ServerError> {
        let mut stats = PollStats::default();

        for path in &self.config.watch_paths.clone() {
            stats.files_checked += 1;

            let source_type = match path
                .file_name()
                .and_then(|n| n.to_str())
                .and_then(SourceType::from_filename)
            {
                Some(st) => st,
                None => {
                    tracing::debug!("Skipping unrecognized file: {:?}", path);
                    continue;
                }
            };

            match self.read_new_lines(path, source_type).await {
                Ok((lines, parsed, stored)) => {
                    stats.new_lines += lines;
                    stats.parsed += parsed;
                    stats.stored += stored;
                }
                Err(e) => {
                    tracing::warn!("Error reading {:?}: {}", path, e);
                    stats.errors += 1;
                }
            }
        }

        // Persist state after each poll
        if let Err(e) = self.state.save(&self.config.state_path) {
            tracing::warn!("Failed to save watcher state: {}", e);
        }

        Ok(stats)
    }

    /// Read new lines from a file since last known offset
    async fn read_new_lines(
        &mut self,
        path: &Path,
        source_type: SourceType,
    ) -> Result<(usize, usize, usize), String> {
        let path_str = path.to_string_lossy().to_string();
        let last_offset = self.state.offsets.get(&path_str).copied().unwrap_or(0);

        let file =
            std::fs::File::open(path).map_err(|e| format!("Cannot open {:?}: {}", path, e))?;

        let file_len = file
            .metadata()
            .map_err(|e| format!("Cannot stat {:?}: {}", path, e))?
            .len();

        // No new data
        if file_len <= last_offset {
            return Ok((0, 0, 0));
        }

        let mut reader = BufReader::new(file);
        reader
            .seek(SeekFrom::Start(last_offset))
            .map_err(|e| format!("Seek error: {}", e))?;

        let mut entries: Vec<MemoryEntry> = Vec::new();
        let mut line_buf = String::new();
        let mut line_count = 0usize;
        let mut parse_count = 0usize;

        while reader.read_line(&mut line_buf).map_err(|e| e.to_string())? > 0 {
            let trimmed = line_buf.trim();
            if !trimmed.is_empty() {
                line_count += 1;
                if let Some(parsed) = JsonlParser::parse_line(trimmed, source_type) {
                    parse_count += 1;
                    entries.push(
                        MemoryEntry::new(&parsed.key, &parsed.tier, &parsed.value)
                            .with_entry_type(&parsed.entry_type),
                    );
                }
            }
            line_buf.clear();
        }

        // Update offset to current file position
        let new_offset = reader.stream_position().map_err(|e| e.to_string())?;
        self.state.offsets.insert(path_str, new_offset);

        // Store entries in batch
        let stored = if !entries.is_empty() {
            self.store_entries(entries).await
        } else {
            0
        };

        Ok((line_count, parse_count, stored))
    }

    /// Store parsed entries with optional GPU embedding
    async fn store_entries(&self, entries: Vec<MemoryEntry>) -> usize {
        let store = self.store.lock().await;

        // If we have an embedder, generate embeddings for the batch
        let entries_with_embeddings = if let Some(ref client) = self.embedder {
            let texts: Vec<&str> = entries.iter().map(|e| e.value.as_str()).collect();
            let embeddings = client.embed_batch(&texts).await;

            entries
                .into_iter()
                .zip(embeddings.into_iter())
                .map(|(mut entry, emb)| {
                    if let Some(embedding) = emb {
                        entry.embedding = Some(embedding);
                    }
                    entry
                })
                .collect::<Vec<_>>()
        } else {
            entries
        };

        let mut stored = 0;
        for entry in entries_with_embeddings {
            match store.store(entry) {
                Ok(()) => stored += 1,
                Err(e) => tracing::warn!("Failed to store entry: {}", e),
            }
        }

        stored
    }

    /// Get current watcher statistics
    pub fn stats(&self) -> WatcherStats {
        WatcherStats {
            watched_files: self.config.watch_paths.len(),
            tracked_offsets: self.state.offsets.len(),
            total_bytes_read: self.state.offsets.values().sum(),
        }
    }
}

/// Summary statistics for the watcher
#[derive(Debug, Clone, Serialize)]
pub struct WatcherStats {
    /// Number of files currently being watched.
    pub watched_files: usize,
    /// Number of per-file read offsets being tracked.
    pub tracked_offsets: usize,
    /// Cumulative bytes read across all watched files.
    pub total_bytes_read: u64,
}

/// Discover JSONL files across data and metrics directories.
///
/// Searches `data_dir` (`.claude/data/`) and its sibling `metrics/`
/// directory (`.claude/metrics/`) since compliance/costs JSONL files
/// live in the metrics directory.
pub(crate) fn discover_jsonl_paths(data_dir: &Path) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    let candidates = [
        "compliance.jsonl",
        "costs.jsonl",
        "rlm_skill_learning.jsonl",
        "hook_activity.jsonl",
        "lna_history.jsonl",
    ];

    // Build search directories: data_dir + sibling metrics dir
    let metrics_dir = data_dir.parent().map(|p| p.join("metrics"));

    let search_dirs: Vec<&Path> = std::iter::once(data_dir)
        .chain(metrics_dir.as_deref())
        .collect();

    for name in &candidates {
        for dir in &search_dirs {
            let p = dir.join(name);
            if p.exists() {
                paths.push(p);
                break; // found in this dir, skip remaining
            }
        }
    }

    paths
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_watcher_state_roundtrip() {
        let dir = TempDir::new().unwrap();
        let state_path = dir.path().join("state.json");

        let mut state = WatcherState::default();
        state
            .offsets
            .insert("/path/to/file.jsonl".to_string(), 12345);
        state.save(&state_path).unwrap();

        let loaded = WatcherState::load(&state_path);
        assert_eq!(loaded.offsets.get("/path/to/file.jsonl"), Some(&12345));
    }

    #[test]
    fn test_discover_jsonl_paths() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("compliance.jsonl"), "{}").unwrap();
        std::fs::write(dir.path().join("costs.jsonl"), "{}").unwrap();
        std::fs::write(dir.path().join("other.txt"), "").unwrap();

        let paths = discover_jsonl_paths(dir.path());
        assert_eq!(paths.len(), 2);
    }

    #[test]
    fn test_watcher_config_default() {
        let config = WatcherConfig::default();
        assert_eq!(config.poll_interval_s, 30);
        assert_eq!(config.embed_batch_size, 32);
    }
}
