//! File watcher for automatic re-indexation when source files change.
//!
//! Uses `notify-debouncer-mini` to coalesce rapid writes (e.g., multiple editor
//! save events) into a single [`FileEvent`] per path.  The debounce duration is
//! configurable; [`FileWatcher::new`] uses a default of 100 ms.
//!
//! # See Also
//!
//! - `touring_index::watcher::FileWatcher`: Cross-platform file watcher with
//!   `.gitignore` support (uses the `notify` crate directly, no debouncer wrapper).
//!
//! ## Example
//!
//! ```no_run
//! use std::sync::mpsc;
//! use touring_code::ast::watcher::{FileWatcher, FileEvent};
//!
//! let (tx, rx) = mpsc::channel::<FileEvent>();
//! let mut w = FileWatcher::new(tx).unwrap();
//! w.watch(std::path::Path::new("./src")).unwrap();
//!
//! // Events arrive on `rx` when files are created/modified/removed
//! for event in rx.iter() {
//!     println!("{:?}", event);
//! }
//! ```

use notify::{EventKind, RecursiveMode};
use notify_debouncer_mini::{Debouncer, new_debouncer};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

/// A file-system event relevant to code indexing.
#[derive(Debug, Clone)]
pub struct FileEvent {
    /// The kind of change detected.
    pub kind: FileEventKind,
    /// Paths affected by this event.
    pub paths: Vec<PathBuf>,
}

/// Classifies the type of file-system change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileEventKind {
    /// A new file was created.
    Created,
    /// An existing file was modified.
    Modified,
    /// A file was removed.
    Removed,
    /// Some other change (rename, metadata, etc.).
    Other,
}

/// Errors from the file watcher.
#[derive(Debug, thiserror::Error)]
pub enum WatcherError {
    /// An error propagated from the underlying `notify` file-watching backend.
    #[error("notify error: {0}")]
    Notify(#[from] notify::Error),
}

/// Watches directories for file changes and emits [`FileEvent`]s.
///
/// Events are debounced: multiple rapid changes to the same file produce
/// a single event after the configured quiet period.
pub struct FileWatcher {
    debouncer: Debouncer<notify::RecommendedWatcher>,
}

impl FileWatcher {
    /// Create a new watcher with a 100 ms debounce.
    pub fn new(sender: mpsc::Sender<FileEvent>) -> Result<Self, WatcherError> {
        Self::with_debounce(sender, Duration::from_millis(100))
    }

    /// Create a new watcher with a custom debounce duration.
    ///
    /// `debounce` is the quiet period: a path's event is not forwarded until
    /// this duration has elapsed without further changes to that path.
    pub fn with_debounce(
        sender: mpsc::Sender<FileEvent>,
        debounce: Duration,
    ) -> Result<Self, WatcherError> {
        // notify-debouncer-mini coalesces events per path within `debounce`.
        // DebouncedEventKind only carries Any/AnyContinuous (not Create/Modify/Remove),
        // so all debounced events are mapped to Modified. Callers that need fine-grained
        // kinds should use notify::RecommendedWatcher directly.
        let debouncer = new_debouncer(
            debounce,
            move |result: notify_debouncer_mini::DebounceEventResult| {
                if let Ok(events) = result {
                    for ev in events {
                        let _ = sender.send(FileEvent {
                            kind: FileEventKind::Modified,
                            paths: vec![ev.path],
                        });
                    }
                }
            },
        )?;

        Ok(Self { debouncer })
    }

    /// Start watching a directory (recursively).
    pub fn watch(&mut self, path: &Path) -> Result<(), WatcherError> {
        self.debouncer
            .watcher()
            .watch(path, RecursiveMode::Recursive)?;
        Ok(())
    }

    /// Stop watching a previously-watched directory.
    pub fn unwatch(&mut self, path: &Path) -> Result<(), WatcherError> {
        self.debouncer.watcher().unwatch(path)?;
        Ok(())
    }
}

/// Map notify's event kinds to our simplified classification.
///
/// Used when integrating with raw `notify::RecommendedWatcher` events.
pub fn classify_event(kind: &EventKind) -> FileEventKind {
    match kind {
        EventKind::Create(_) => FileEventKind::Created,
        EventKind::Modify(_) => FileEventKind::Modified,
        EventKind::Remove(_) => FileEventKind::Removed,
        _ => FileEventKind::Other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::thread;
    use tempfile::TempDir;

    #[test]
    fn test_watcher_detects_create() {
        let dir = TempDir::new().unwrap();
        let (tx, rx) = mpsc::channel();
        let mut w = FileWatcher::new(tx).unwrap();
        w.watch(dir.path()).unwrap();

        // Small delay so the watcher is fully registered
        thread::sleep(Duration::from_millis(200));

        fs::write(dir.path().join("f.py"), "def foo(): pass").unwrap();

        let event = rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(
            event.paths.iter().any(|p| p.ends_with("f.py")),
            "expected path ending with f.py, got: {:?}",
            event.paths
        );
    }

    #[test]
    fn test_watcher_detects_modify() {
        let dir = TempDir::new().unwrap();
        let f = dir.path().join("f.py");
        fs::write(&f, "v1").unwrap();

        let (tx, rx) = mpsc::channel();
        let mut w = FileWatcher::new(tx).unwrap();
        w.watch(dir.path()).unwrap();

        // Wait for watcher to fully register before modifying
        thread::sleep(Duration::from_millis(200));

        fs::write(&f, "v2").unwrap();

        let event = rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(!event.paths.is_empty(), "expected non-empty paths");
    }

    #[test]
    fn test_watcher_detects_remove() {
        let dir = TempDir::new().unwrap();
        let f = dir.path().join("to_remove.py");
        fs::write(&f, "content").unwrap();

        let (tx, rx) = mpsc::channel();
        let mut w = FileWatcher::new(tx).unwrap();
        w.watch(dir.path()).unwrap();

        thread::sleep(Duration::from_millis(200));

        fs::remove_file(&f).unwrap();

        let event = rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(
            event.paths.iter().any(|p| p.ends_with("to_remove.py")),
            "expected path ending with to_remove.py, got: {:?}",
            event.paths
        );
    }

    #[test]
    fn test_watcher_unwatch() {
        let dir = TempDir::new().unwrap();
        let (tx, rx) = mpsc::channel();
        let mut w = FileWatcher::new(tx).unwrap();
        w.watch(dir.path()).unwrap();

        thread::sleep(Duration::from_millis(200));

        w.unwatch(dir.path()).unwrap();

        // After unwatch, new files should NOT produce events
        thread::sleep(Duration::from_millis(100));
        fs::write(dir.path().join("ghost.py"), "# nothing").unwrap();

        let result = rx.recv_timeout(Duration::from_secs(1));
        assert!(result.is_err(), "should not receive events after unwatch");
    }

    #[test]
    fn test_classify_event_kinds() {
        assert_eq!(
            classify_event(&EventKind::Create(notify::event::CreateKind::File)),
            FileEventKind::Created
        );
        assert_eq!(
            classify_event(&EventKind::Modify(notify::event::ModifyKind::Data(
                notify::event::DataChange::Content
            ))),
            FileEventKind::Modified
        );
        assert_eq!(
            classify_event(&EventKind::Remove(notify::event::RemoveKind::File)),
            FileEventKind::Removed
        );
        assert_eq!(
            classify_event(&EventKind::Access(notify::event::AccessKind::Read)),
            FileEventKind::Other
        );
    }

    #[test]
    fn test_watcher_recursive_subdirs() {
        let dir = TempDir::new().unwrap();
        let sub = dir.path().join("a").join("b");
        fs::create_dir_all(&sub).unwrap();

        let (tx, rx) = mpsc::channel();
        let mut w = FileWatcher::new(tx).unwrap();
        w.watch(dir.path()).unwrap();

        thread::sleep(Duration::from_millis(200));

        fs::write(sub.join("deep.rs"), "fn main() {}").unwrap();

        // May receive directory events before the file event — drain until we find it
        let deadline = Duration::from_secs(5);
        let start = std::time::Instant::now();
        let mut found = false;
        while start.elapsed() < deadline {
            match rx.recv_timeout(Duration::from_millis(500)) {
                Ok(event) if event.paths.iter().any(|p| p.ends_with("deep.rs")) => {
                    found = true;
                    break;
                }
                Ok(_) => continue, // directory creation event, skip
                Err(_) => break,
            }
        }
        assert!(found, "expected deep.rs event within 5s");
    }
}
