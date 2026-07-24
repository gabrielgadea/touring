//! `ProjectEntry` — a single registered project in the touring multi-project registry.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// A registered project alias with its path and metadata.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectEntry {
    /// Human-friendly alias for the project (e.g., "touring", "konverter").
    pub alias: String,
    /// Absolute path to the project root.
    pub path: PathBuf,
    /// Optional path to the project-specific touring daemon socket.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub daemon_socket: Option<PathBuf>,
    /// Last time this project was used (any `touring` command targeting it).
    pub last_used: DateTime<Utc>,
    /// Whether this project is the default when no alias is specified.
    #[serde(default)]
    pub is_default: bool,
}

impl ProjectEntry {
    /// Create a new project entry.
    pub fn new(alias: impl Into<String>, path: impl Into<PathBuf>) -> Self {
        Self {
            alias: alias.into(),
            path: path.into(),
            daemon_socket: None,
            last_used: Utc::now(),
            is_default: false,
        }
    }

    /// Mark this entry as the default project.
    pub fn set_default(&mut self) {
        self.is_default = true;
    }

    /// Update the `last_used` timestamp to now.
    pub fn touch(&mut self) {
        self.last_used = Utc::now();
    }

    /// Set the daemon socket path.
    pub fn set_daemon_socket(&mut self, socket: impl Into<PathBuf>) {
        self.daemon_socket = Some(socket.into());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_project_entry() {
        let entry = ProjectEntry::new("touring", "/home/user/touring");
        assert_eq!(entry.alias, "touring");
        assert_eq!(entry.path, PathBuf::from("/home/user/touring"));
        assert!(entry.daemon_socket.is_none());
        assert!(!entry.is_default);
    }

    #[test]
    fn test_set_default() {
        let mut entry = ProjectEntry::new("touring", "/home/user/touring");
        entry.set_default();
        assert!(entry.is_default);
    }

    #[test]
    fn test_touch_updates_timestamp() {
        let mut entry = ProjectEntry::new("touring", "/home/user/touring");
        let original = entry.last_used;
        std::thread::sleep(std::time::Duration::from_millis(10));
        entry.touch();
        assert!(entry.last_used > original);
    }

    #[test]
    fn test_set_daemon_socket() {
        let mut entry = ProjectEntry::new("touring", "/home/user/touring");
        entry.set_daemon_socket("/tmp/myproject.sock");
        assert_eq!(
            entry.daemon_socket,
            Some(PathBuf::from("/tmp/myproject.sock"))
        );
    }

    #[test]
    fn test_serialize_deserialize() {
        let mut entry = ProjectEntry::new("touring", "/home/user/touring");
        entry.set_default();
        let json = serde_json::to_string_pretty(&entry).unwrap();
        let roundtrip: ProjectEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(roundtrip.alias, "touring");
        assert!(roundtrip.is_default);
    }
}
