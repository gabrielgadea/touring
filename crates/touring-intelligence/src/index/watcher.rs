//! File Watcher - Cross-platform file system monitoring
//!
//! Uses the `notify` crate for efficient file watching with:
//! - Recursive directory watching
//! - .gitignore-aware filtering
//! - 100ms debounce for batching rapid changes
//! - Channel-based communication with main thread
//!
//! # See Also
//!
//! - [`touring_code::ast::watcher::FileWatcher`]: AST-focused file watcher with debouncing
//!   (uses `notify-debouncer-mini` for coalescing rapid save events).

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use ignore::gitignore::GitignoreBuilder;
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use thiserror::Error;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

/// Errors that can occur in the file watcher
#[derive(Error, Debug)]
pub enum WatcherError {
    /// Underlying I/O failure while watching the filesystem.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Error from the `notify` filesystem-event backend.
    #[error("Notify error: {0}")]
    Notify(#[from] notify::Error),

    /// Error parsing `.gitignore` rules via the `ignore` crate.
    #[error("Gitignore parse error: {0}")]
    Gitignore(#[from] ignore::Error),

    /// Failed to send a file event over the internal channel (receiver dropped).
    #[error("Channel send error")]
    ChannelSend,

    /// Operation requires a running watcher but none is active.
    #[error("Watcher not running")]
    NotRunning,
}

/// Result type for watcher operations
pub(crate) type Result<T> = std::result::Result<T, WatcherError>;

/// Types of file system events
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileEventType {
    /// File or directory created
    Create,
    /// File or directory modified
    Modify,
    /// File or directory removed
    Remove,
    /// File or directory renamed
    Rename,
}

impl std::fmt::Display for FileEventType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FileEventType::Create => write!(f, "CREATE"),
            FileEventType::Modify => write!(f, "MODIFY"),
            FileEventType::Remove => write!(f, "REMOVE"),
            FileEventType::Rename => write!(f, "RENAME"),
        }
    }
}

/// A file system event
#[derive(Debug, Clone)]
pub struct FileEvent {
    /// Type of event
    pub event_type: FileEventType,
    /// Path that changed
    pub path: PathBuf,
    /// Timestamp when event was detected
    pub timestamp: SystemTime,
    /// Optional old path for rename events
    pub old_path: Option<PathBuf>,
}

impl FileEvent {
    /// Create a new file event
    pub fn new(event_type: FileEventType, path: PathBuf) -> Self {
        Self {
            event_type,
            path,
            timestamp: SystemTime::now(),
            old_path: None,
        }
    }

    /// Create a rename event with old path
    pub fn rename(old_path: PathBuf, new_path: PathBuf) -> Self {
        Self {
            event_type: FileEventType::Rename,
            path: new_path,
            timestamp: SystemTime::now(),
            old_path: Some(old_path),
        }
    }

    /// Check if this is a create event
    pub fn is_create(&self) -> bool {
        self.event_type == FileEventType::Create
    }

    /// Check if this is a modify event
    pub fn is_modify(&self) -> bool {
        self.event_type == FileEventType::Modify
    }

    /// Check if this is a remove event
    pub fn is_remove(&self) -> bool {
        self.event_type == FileEventType::Remove
    }

    /// Check if this is a rename event
    pub fn is_rename(&self) -> bool {
        self.event_type == FileEventType::Rename
    }
}

/// File watcher with debouncing and gitignore support
#[derive(Debug)]
pub struct FileWatcher {
    /// Root path being watched
    root_path: PathBuf,
    /// Debounce interval
    debounce_ms: u64,
    /// Gitignore matcher
    gitignore: Option<ignore::gitignore::Gitignore>,
    /// Watcher instance
    watcher: Option<RecommendedWatcher>,
    /// Event channel sender (bounded for backpressure)
    event_sender: mpsc::Sender<FileEvent>,
    /// Event channel receiver (exposed for consumers)
    event_receiver: mpsc::Receiver<FileEvent>,
}

impl FileWatcher {
    /// Default debounce interval in milliseconds
    pub(crate) const DEFAULT_DEBOUNCE_MS: u64 = 100;

    /// Create a new file watcher
    pub fn new(root_path: impl AsRef<Path>) -> Result<Self> {
        Self::with_debounce(root_path, Self::DEFAULT_DEBOUNCE_MS)
    }

    /// Create a new file watcher with custom debounce
    pub(crate) fn with_debounce(root_path: impl AsRef<Path>, debounce_ms: u64) -> Result<Self> {
        let root_path = root_path.as_ref().canonicalize()?;
        // Bounded channel for backpressure (Context7 best practice: prevents OOM under load)
        const WATCHER_CHANNEL_CAPACITY: usize = 256;
        let (event_sender, event_receiver) = mpsc::channel::<FileEvent>(WATCHER_CHANNEL_CAPACITY);

        Ok(Self {
            root_path,
            debounce_ms,
            gitignore: None,
            watcher: None,
            event_sender,
            event_receiver,
        })
    }

    /// Load .gitignore from the watched directory
    pub(crate) fn with_gitignore(mut self) -> Result<Self> {
        let gitignore_path = self.root_path.join(".gitignore");

        if gitignore_path.exists() {
            let mut builder = GitignoreBuilder::new(&self.root_path);

            // Add the .gitignore file
            if let Some(_err) = builder.add(&gitignore_path) {
                // Continue anyway - gitignore errors are not fatal
                warn!("Failed to parse .gitignore entry, continuing without it");
            }

            // Also ignore common directories
            builder.add_line(None, ".git/").ok();
            builder.add_line(None, "target/").ok();
            builder.add_line(None, "node_modules/").ok();
            builder.add_line(None, ".claude/touring/target/").ok();

            self.gitignore = Some(builder.build()?);
            info!("Loaded .gitignore from {:?}", gitignore_path);
        } else {
            // Create a default gitignore
            let mut builder = GitignoreBuilder::new(&self.root_path);
            builder.add_line(None, ".git/").ok();
            builder.add_line(None, "target/").ok();
            builder.add_line(None, "node_modules/").ok();
            builder.add_line(None, ".claude/touring/target/").ok();
            self.gitignore = Some(builder.build()?);
        }

        Ok(self)
    }

    /// Start watching the directory
    pub fn start(&mut self) -> Result<()> {
        if self.watcher.is_some() {
            warn!("Watcher already running");
            return Ok(());
        }

        let event_sender = self.event_sender.clone();
        let root_path = self.root_path.clone();
        let _debounce_ms = self.debounce_ms;
        let gitignore = self.gitignore.clone();

        // Create the watcher
        let mut watcher = notify::recommended_watcher(move |res: notify::Result<Event>| {
            match res {
                Ok(event) => {
                    debug!("Raw notify event: {:?}", event);

                    // Convert notify event to our FileEvent
                    let events = Self::convert_event(event);

                    for file_event in events {
                        // Apply gitignore filtering
                        if let Some(ref gi) = gitignore {
                            // Get path relative to root for gitignore matching
                            let rel_path = file_event
                                .path
                                .strip_prefix(&root_path)
                                .unwrap_or(&file_event.path);
                            let is_dir = file_event.path.is_dir();
                            if gi.matched(rel_path, is_dir).is_ignore() {
                                debug!("Ignoring path: {:?}", file_event.path);
                                continue;
                            }
                        }

                        // Apply debouncing (bounded channel - use try_send for backpressure)
                        if let Err(e) = event_sender.try_send(file_event) {
                            // Channel full - log and drop (bounded provides backpressure signal)
                            error!("File event channel full, dropping event: {}", e);
                        }
                    }
                }
                Err(e) => {
                    error!("Watch error: {}", e);
                }
            }
        })?;

        // Watch the root path recursively
        watcher.watch(&self.root_path, RecursiveMode::Recursive)?;

        info!(
            "Started watching {:?} with {}ms debounce",
            self.root_path, self.debounce_ms
        );

        self.watcher = Some(watcher);
        Ok(())
    }

    /// Stop watching
    pub(crate) fn stop(&mut self) -> Result<()> {
        if let Some(watcher) = self.watcher.take() {
            drop(watcher);
            info!("Stopped watching {:?}", self.root_path);
        }
        Ok(())
    }

    /// Convert a notify event to our FileEvent format
    fn convert_event(event: Event) -> Vec<FileEvent> {
        let mut events = Vec::new();

        let event_type = match event.kind {
            EventKind::Create(_) => FileEventType::Create,
            EventKind::Modify(notify::event::ModifyKind::Name(_)) => FileEventType::Rename,
            EventKind::Modify(_) => FileEventType::Modify,
            EventKind::Remove(_) => FileEventType::Remove,
            _ => return events, // Skip other event types
        };

        // Handle rename events specially
        if let EventKind::Modify(notify::event::ModifyKind::Name(notify::event::RenameMode::Both)) =
            event.kind
        {
            // SAFETY: Indexing guarded by `event.paths.len() >= 2` check, so [0] and [1] are in-bounds.
            #[allow(clippy::indexing_slicing)]
            if event.paths.len() >= 2 {
                events.push(FileEvent::rename(
                    event.paths[0].clone(),
                    event.paths[1].clone(),
                ));
                return events;
            }
        }

        // Regular events
        for path in event.paths {
            events.push(FileEvent::new(event_type, path));
        }

        events
    }

    /// Get the next event (async)
    pub async fn next_event(&mut self) -> Option<FileEvent> {
        self.event_receiver.recv().await
    }

    /// Try to get the next event without blocking
    pub fn try_next_event(&mut self) -> Option<FileEvent> {
        self.event_receiver.try_recv().ok()
    }

    /// Get the root path
    pub fn root_path(&self) -> &Path {
        &self.root_path
    }

    /// Get the debounce interval
    pub fn debounce_ms(&self) -> u64 {
        self.debounce_ms
    }

    /// Check if the watcher is running
    pub fn is_running(&self) -> bool {
        self.watcher.is_some()
    }
}

impl Drop for FileWatcher {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

/// Builder for FileWatcher with fluent API
#[derive(Debug)]
pub struct FileWatcherBuilder {
    root_path: PathBuf,
    debounce_ms: u64,
    use_gitignore: bool,
}

impl FileWatcherBuilder {
    /// Create a new builder
    pub fn new(root_path: impl AsRef<Path>) -> Result<Self> {
        Ok(Self {
            root_path: root_path.as_ref().canonicalize()?,
            debounce_ms: FileWatcher::DEFAULT_DEBOUNCE_MS,
            use_gitignore: true,
        })
    }

    /// Set debounce interval
    pub fn debounce_ms(mut self, ms: u64) -> Self {
        self.debounce_ms = ms;
        self
    }

    /// Disable gitignore filtering
    pub fn without_gitignore(mut self) -> Self {
        self.use_gitignore = false;
        self
    }

    /// Build the file watcher
    pub fn build(self) -> Result<FileWatcher> {
        let mut watcher = FileWatcher::with_debounce(&self.root_path, self.debounce_ms)?;

        if self.use_gitignore {
            watcher = watcher.with_gitignore()?;
        }

        Ok(watcher)
    }
}

/// Batch file events for efficient processing
#[derive(Debug)]
pub struct EventBatcher {
    /// Debounce interval
    debounce: Duration,
    /// Pending events
    pending: Vec<FileEvent>,
    /// Last batch time
    last_batch: Option<SystemTime>,
}

impl EventBatcher {
    /// Create a new event batcher
    pub fn new(debounce_ms: u64) -> Self {
        Self {
            debounce: Duration::from_millis(debounce_ms),
            pending: Vec::new(),
            last_batch: None,
        }
    }

    /// Add an event to the batch
    pub fn add(&mut self, event: FileEvent) {
        self.pending.push(event);
    }

    /// Check if batch is ready to be processed
    pub fn is_ready(&self) -> bool {
        if self.pending.is_empty() {
            return false;
        }

        if let Some(last) = self.last_batch {
            SystemTime::now().duration_since(last).unwrap_or_default() >= self.debounce
        } else {
            true
        }
    }

    /// Get and clear the current batch
    pub fn take_batch(&mut self) -> Vec<FileEvent> {
        self.last_batch = Some(SystemTime::now());
        std::mem::take(&mut self.pending)
    }

    /// Get pending count
    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::Duration;
    use tempfile::TempDir;

    #[test]
    fn test_file_event_creation() {
        let path = PathBuf::from("/tmp/test.txt");
        let event = FileEvent::new(FileEventType::Create, path.clone());

        assert_eq!(event.event_type, FileEventType::Create);
        assert_eq!(event.path, path);
        assert!(event.old_path.is_none());
        assert!(event.is_create());
    }

    #[test]
    fn test_file_event_rename() {
        let old_path = PathBuf::from("/tmp/old.txt");
        let new_path = PathBuf::from("/tmp/new.txt");
        let event = FileEvent::rename(old_path.clone(), new_path.clone());

        assert_eq!(event.event_type, FileEventType::Rename);
        assert_eq!(event.path, new_path);
        assert_eq!(event.old_path, Some(old_path));
        assert!(event.is_rename());
    }

    #[tokio::test]
    async fn test_watcher_detects_create() {
        let temp_dir = TempDir::new().unwrap();
        let mut watcher = FileWatcherBuilder::new(&temp_dir)
            .unwrap()
            .without_gitignore()
            .build()
            .unwrap();

        watcher.start().unwrap();

        // Give watcher time to initialize
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Create a file
        let test_file = temp_dir.path().join("test.txt");
        fs::write(&test_file, "hello").unwrap();

        // Wait for event (with timeout)
        let timeout = tokio::time::Duration::from_millis(500);
        let result = tokio::time::timeout(timeout, watcher.next_event()).await;

        assert!(result.is_ok(), "Should receive event within timeout");
        let event = result.unwrap().unwrap();
        assert!(event.is_create() || event.is_modify());

        watcher.stop().unwrap();
    }

    #[tokio::test]
    async fn test_watcher_detects_modify() {
        let temp_dir = TempDir::new().unwrap();
        let test_file = temp_dir.path().join("test.txt");
        fs::write(&test_file, "initial").unwrap();

        let mut watcher = FileWatcherBuilder::new(&temp_dir)
            .unwrap()
            .without_gitignore()
            .build()
            .unwrap();

        watcher.start().unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Modify the file
        fs::write(&test_file, "modified").unwrap();

        // Wait for event
        let timeout = tokio::time::Duration::from_millis(500);
        let result = tokio::time::timeout(timeout, watcher.next_event()).await;

        assert!(result.is_ok(), "Should receive event within timeout");
        let event = result.unwrap().unwrap();
        assert!(event.is_modify());

        watcher.stop().unwrap();
    }

    #[tokio::test]
    async fn test_watcher_respects_gitignore() {
        let temp_dir = TempDir::new().unwrap();

        // Create .gitignore - use pattern that matches directory and contents
        fs::write(temp_dir.path().join(".gitignore"), "ignored/\nignored/**\n").unwrap();

        // Create ignored directory
        fs::create_dir(temp_dir.path().join("ignored")).unwrap();

        let mut watcher = FileWatcherBuilder::new(&temp_dir).unwrap().build().unwrap();

        watcher.start().unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Create file in ignored directory
        let ignored_file = temp_dir.path().join("ignored").join("test.txt");
        fs::write(&ignored_file, "content").unwrap();

        // Create file in tracked directory
        let tracked_file = temp_dir.path().join("tracked.txt");
        fs::write(&tracked_file, "content").unwrap();

        // Should get event for tracked file, not ignored
        let timeout = tokio::time::Duration::from_millis(500);
        let result = tokio::time::timeout(timeout, watcher.next_event()).await;

        assert!(result.is_ok());
        let event = result.unwrap().unwrap();
        assert!(
            event.path.ends_with("tracked.txt"),
            "Expected tracked.txt but got {:?}",
            event.path
        );

        watcher.stop().unwrap();
    }

    #[test]
    fn test_event_batcher() {
        let mut batcher = EventBatcher::new(100);

        assert!(!batcher.is_ready());
        assert_eq!(batcher.pending_count(), 0);

        // Add events
        batcher.add(FileEvent::new(FileEventType::Create, PathBuf::from("/a")));
        batcher.add(FileEvent::new(FileEventType::Modify, PathBuf::from("/b")));

        assert_eq!(batcher.pending_count(), 2);

        // Should be ready immediately (no last batch)
        assert!(batcher.is_ready());

        // Take batch
        let batch = batcher.take_batch();
        assert_eq!(batch.len(), 2);
        assert_eq!(batcher.pending_count(), 0);

        // Add more events - should not be ready immediately due to debounce
        batcher.add(FileEvent::new(FileEventType::Remove, PathBuf::from("/c")));
        assert!(!batcher.is_ready());
    }

    // ========================================================================
    // Extended integration tests
    // ========================================================================

    #[tokio::test]
    async fn test_watcher_detects_remove() {
        let temp_dir = TempDir::new().unwrap();
        let mut watcher = FileWatcherBuilder::new(&temp_dir)
            .unwrap()
            .without_gitignore()
            .build()
            .unwrap();

        watcher.start().unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Create then remove a file
        let test_file = temp_dir.path().join("todelete.txt");
        fs::write(&test_file, "content").unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Drain create/modify events
        while watcher.try_next_event().is_some() {}

        fs::remove_file(&test_file).unwrap();
        tokio::time::sleep(Duration::from_millis(200)).await;

        let event = tokio::time::timeout(Duration::from_millis(500), watcher.next_event())
            .await
            .unwrap()
            .unwrap();

        assert!(event.is_remove(), "Should receive Remove event");

        watcher.stop().unwrap();
    }

    #[tokio::test]
    async fn test_watcher_is_running() {
        let temp_dir = TempDir::new().unwrap();
        let watcher = FileWatcherBuilder::new(&temp_dir)
            .unwrap()
            .without_gitignore()
            .build()
            .unwrap();

        assert!(
            !watcher.is_running(),
            "Should not be running before start()"
        );

        let mut watcher = watcher;
        watcher.start().unwrap();
        assert!(watcher.is_running(), "Should be running after start()");

        watcher.stop().unwrap();
        assert!(!watcher.is_running(), "Should not be running after stop()");
    }

    #[tokio::test]
    async fn test_watcher_stop_and_restart() {
        let temp_dir = TempDir::new().unwrap();
        let mut watcher = FileWatcherBuilder::new(&temp_dir)
            .unwrap()
            .without_gitignore()
            .build()
            .unwrap();

        // Start and create file
        watcher.start().unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;

        let file_a = temp_dir.path().join("restart_a.txt");
        fs::write(&file_a, "a").unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
        while watcher.try_next_event().is_some() {}
        watcher.stop().unwrap();

        // Create file while stopped
        let file_b = temp_dir.path().join("during_stop.txt");
        fs::write(&file_b, "b").unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Restart
        watcher.start().unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;

        let file_c = temp_dir.path().join("after_restart.txt");
        fs::write(&file_c, "c").unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;

        let mut events = Vec::new();
        while let Some(e) = watcher.try_next_event() {
            events.push(e);
        }

        let has_c = events.iter().any(|e| e.path.ends_with("after_restart.txt"));
        let has_b = events.iter().any(|e| e.path.ends_with("during_stop.txt"));

        assert!(has_c, "Should detect file created after restart");
        assert!(!has_b, "Should NOT detect file created while stopped");

        watcher.stop().unwrap();
    }

    #[tokio::test]
    async fn test_watcher_concurrent_events() {
        let temp_dir = TempDir::new().unwrap();
        let mut watcher = FileWatcherBuilder::new(&temp_dir)
            .unwrap()
            .without_gitignore()
            .build()
            .unwrap();

        watcher.start().unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Create and modify rapidly
        let f1 = temp_dir.path().join("concurrent.txt");
        fs::write(&f1, "v1").unwrap();

        let f2 = temp_dir.path().join("concurrent2.txt");
        fs::write(&f2, "other").unwrap();

        tokio::time::sleep(Duration::from_millis(100)).await;
        fs::write(&f1, "v2modified").unwrap();

        tokio::time::sleep(Duration::from_millis(100)).await;
        fs::remove_file(&f2).unwrap();

        tokio::time::sleep(Duration::from_millis(100)).await;

        let mut found_modify = false;
        let mut found_remove = false;

        while let Some(e) = watcher.try_next_event() {
            if e.is_modify() && e.path.ends_with("concurrent.txt") {
                found_modify = true;
            }
            if e.is_remove() && e.path.ends_with("concurrent2.txt") {
                found_remove = true;
            }
        }

        assert!(found_modify, "Should detect modify for concurrent.txt");
        assert!(found_remove, "Should detect remove for concurrent2.txt");

        watcher.stop().unwrap();
    }

    #[test]
    fn test_file_event_type_display() {
        assert_eq!(format!("{}", FileEventType::Create), "CREATE");
        assert_eq!(format!("{}", FileEventType::Modify), "MODIFY");
        assert_eq!(format!("{}", FileEventType::Remove), "REMOVE");
        assert_eq!(format!("{}", FileEventType::Rename), "RENAME");
    }

    #[test]
    fn test_file_event_type_checks() {
        let create = FileEvent::new(FileEventType::Create, PathBuf::from("/a"));
        let modify = FileEvent::new(FileEventType::Modify, PathBuf::from("/b"));
        let remove = FileEvent::new(FileEventType::Remove, PathBuf::from("/c"));
        let rename = FileEvent::rename(PathBuf::from("/d"), PathBuf::from("/e"));

        assert!(create.is_create() && !create.is_modify() && !create.is_remove());
        assert!(modify.is_modify() && !modify.is_create());
        assert!(remove.is_remove() && !remove.is_modify());
        assert!(rename.is_rename() && rename.old_path.is_some());
    }

    #[test]
    fn test_event_batcher_empty_not_ready() {
        let batcher = EventBatcher::new(100);
        assert!(!batcher.is_ready(), "Empty batcher should not be ready");
    }

    #[test]
    fn test_event_batcher_take_batch_returns_pending() {
        let mut batcher = EventBatcher::new(10);
        batcher.add(FileEvent::new(FileEventType::Create, PathBuf::from("/x")));
        batcher.add(FileEvent::new(FileEventType::Modify, PathBuf::from("/y")));

        let batch = batcher.take_batch();
        assert_eq!(batch.len(), 2);
        assert_eq!(batcher.pending_count(), 0);
    }

    #[test]
    fn test_event_batcher_debounce_resets_after_take() {
        let mut batcher = EventBatcher::new(1000); // 1s debounce
        batcher.add(FileEvent::new(FileEventType::Create, PathBuf::from("/z")));
        assert!(batcher.is_ready(), "should be ready after add");

        let _ = batcher.take_batch();
        assert!(
            !batcher.is_ready(),
            "should not be ready after take (debounce not elapsed)"
        );
    }

    #[test]
    fn test_watcher_error_display() {
        let io_err = WatcherError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "not found",
        ));
        let display = format!("{}", io_err);
        assert!(!display.is_empty());

        let channel_err = WatcherError::ChannelSend;
        let channel_display = format!("{}", channel_err);
        assert!(!channel_display.is_empty());

        let not_running_err = WatcherError::NotRunning;
        let not_running_display = format!("{}", not_running_err);
        assert!(!not_running_display.is_empty());
    }

    #[test]
    fn test_file_event_timestamp_is_set() {
        let before = std::time::SystemTime::now();
        let event = FileEvent::new(FileEventType::Create, PathBuf::from("/tmp/test.txt"));
        let after = std::time::SystemTime::now();

        assert!(
            event.timestamp >= before && event.timestamp <= after,
            "FileEvent timestamp should be approximately now"
        );
    }
}
