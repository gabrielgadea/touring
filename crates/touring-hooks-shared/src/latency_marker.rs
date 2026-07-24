//! Latency marker files for hook performance tracking.
//!
//! Records hook execution timestamps in marker files under `/tmp/touring-hooks/`
//! enabling observability of hook latency without in-process overhead.
//!
//! ## Marker file structure
//! - Base path: `/tmp/touring-hooks/latency/`
//! - File naming: `{hook_name}.lat` (e.g., `pre_edit.lat`)
//! - Format: single line — ISO 8601 timestamp of last execution
//! - TTL: markers older than 24h are considered stale

use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

/// Base directory for hook latency markers.
const LATENCY_MARKER_DIR: &str = "/tmp/touring-hooks/latency";

/// Marker file extension.
const MARKER_EXT: &str = "lat";

/// Hook latency marker — records execution timestamps for observability.
#[derive(Debug, Clone)]
pub struct LatencyMarker {
    /// Hook name (e.g., "pre_edit", "post_read").
    hook_name: String,
    /// Path to the marker file.
    path: PathBuf,
}

impl LatencyMarker {
    /// Create a new latency marker for the given hook name.
    pub fn new(hook_name: &str) -> Self {
        let path = PathBuf::from(LATENCY_MARKER_DIR).join(format!("{}.{}", hook_name, MARKER_EXT));
        Self {
            hook_name: hook_name.to_string(),
            path,
        }
    }

    /// Returns the hook name.
    pub fn hook_name(&self) -> &str {
        &self.hook_name
    }

    /// Returns the marker file path.
    pub fn path(&self) -> &PathBuf {
        &self.path
    }

    /// Record current timestamp as the hook's last execution time.
    /// Creates parent directories if needed.
    pub fn record(&self) -> std::io::Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| std::io::Error::other(e))?;
        fs::write(&self.path, timestamp.as_millis().to_string())?;
        Ok(())
    }

    /// Read the last execution timestamp in milliseconds since UNIX_EPOCH.
    /// Returns `None` if marker doesn't exist or can't be read.
    pub fn read_timestamp_ms(&self) -> Option<u64> {
        fs::read_to_string(&self.path)
            .ok()
            .and_then(|s| s.trim().parse().ok())
    }

    /// Returns `true` if the marker file exists.
    pub fn exists(&self) -> bool {
        self.path.exists()
    }

    /// Returns `true` if the marker is stale (older than `max_age_secs`).
    /// A marker older than 24h is considered stale by default.
    pub fn is_stale(&self, max_age_secs: u64) -> bool {
        if let Some(ts) = self.read_timestamp_ms() {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|e| std::io::Error::other(e))
                .unwrap_or_default();
            let now_ms = now.as_millis() as u64;
            now_ms.saturating_sub(ts) > max_age_secs * 1000
        } else {
            true
        }
    }

    /// Delete the marker file. Silently succeeds if file doesn't exist.
    pub fn delete(&self) -> std::io::Result<()> {
        if self.path.exists() {
            fs::remove_file(&self.path)?;
        }
        Ok(())
    }

    /// Elapsed time since last execution in milliseconds.
    /// Returns `None` if marker doesn't exist.
    pub fn elapsed_ms(&self) -> Option<u64> {
        let ts = self.read_timestamp_ms()?;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| std::io::Error::other("clock error"))
            .ok()?;
        Some(now.as_millis() as u64 - ts)
    }
}

/// Record latency for a hook by creating/updating its marker file.
pub fn record_hook_latency(hook_name: &str) -> std::io::Result<()> {
    LatencyMarker::new(hook_name).record()
}

/// Get elapsed time since last execution of a hook.
/// Returns `None` if no marker exists.
pub fn get_hook_elapsed_ms(hook_name: &str) -> Option<u64> {
    LatencyMarker::new(hook_name).elapsed_ms()
}

/// Delete latency marker for a hook.
pub fn delete_hook_latency(hook_name: &str) -> std::io::Result<()> {
    LatencyMarker::new(hook_name).delete()
}

/// Delete all stale latency markers (older than 24h).
pub fn cleanup_stale_markers() -> std::io::Result<usize> {
    let base = PathBuf::from(LATENCY_MARKER_DIR);
    if !base.exists() {
        return Ok(0);
    }
    let mut removed = 0;
    if let Ok(entries) = fs::read_dir(&base) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some(MARKER_EXT) {
                let hook_name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                let marker = LatencyMarker::new(hook_name);
                if marker.is_stale(86400) {
                    let _ = marker.delete();
                    removed += 1;
                }
            }
        }
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;

    use tempfile::TempDir;

    #[test]
    fn test_latency_marker_record_and_read() {
        let _tmp = TempDir::new().unwrap();
        let hook_name = format!("test_marker_{}", std::process::id());
        let marker = LatencyMarker::new(&hook_name);

        // Should not exist initially
        assert!(!marker.exists());

        // Record timestamp
        marker.record().unwrap();
        assert!(marker.exists());

        // Read back
        let ts = marker.read_timestamp_ms();
        assert!(ts.is_some());

        // Elapsed should be very small — generous 2000ms threshold for system load variance
        let elapsed = marker.elapsed_ms().unwrap();
        assert!(elapsed < 2000);

        // Cleanup
        marker.delete().unwrap();
        assert!(!marker.exists());
    }

    #[test]
    fn test_is_stale() {
        let _tmp = TempDir::new().unwrap();
        let hook_name = format!("stale_marker_{}", std::process::id());
        let marker = LatencyMarker::new(&hook_name);

        // New marker should not be stale even with 24h threshold
        marker.record().unwrap();
        assert!(!marker.is_stale(86400));

        // A marker with 0 age threshold might be stale depending on timing
        // Just verify the method works
    }
}
