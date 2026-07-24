//! VfsWatcher — file system change notifications.
//!
//! Optional feature `watcher` enables file system event subscriptions.

#[cfg(feature = "watcher")]
pub use notify_watcher::NopWatcher;

#[cfg(feature = "watcher")]
mod notify_watcher {
    use thiserror::Error;

    /// Errors that can occur while setting up or running the file watcher.
    #[derive(Error, Debug)]
    pub enum WatcherError {
        /// The watcher backend is not available in this build configuration.
        #[error("watcher not available — enable `watcher` feature")]
        NotAvailable,
    }

    /// Placeholder file watcher kept until the `notify`-backed watcher is wired in.
    pub struct NopWatcher;

    impl NopWatcher {
        /// Always fails with `WatcherError::NotAvailable` until watching is implemented.
        pub fn new(_path: &str) -> Result<Self, WatcherError> {
            Err(WatcherError::NotAvailable)
        }

        /// No-op shutdown — there is nothing to release.
        pub fn close(self) {}
    }
}

#[cfg(not(feature = "watcher"))]
pub use noop_watcher::NopWatcher;

#[cfg(not(feature = "watcher"))]
mod noop_watcher {
    use thiserror::Error;

    /// Errors returned by the no-op watcher when the `watcher` feature is off.
    #[derive(Error, Debug)]
    pub enum WatcherError {
        /// Watching is unavailable because the `watcher` feature is disabled.
        #[error("watcher feature not enabled")]
        NotEnabled,
    }

    /// No-op file watcher used when the `watcher` feature is disabled.
    pub struct NopWatcher;

    impl NopWatcher {
        /// Always fails with `WatcherError::NotEnabled` since watching is disabled.
        pub fn new(_path: &str) -> Result<Self, WatcherError> {
            Err(WatcherError::NotEnabled)
        }

        /// No-op shutdown — there is nothing to release.
        pub fn close(self) {}
    }
}
