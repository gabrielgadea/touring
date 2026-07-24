//! `ProjectRegistry` — in-memory + persistent storage for touring multi-project registry.
//!
//! Storage: `~/.claude/touring/projects.json`
//!
//! # Lifetime
//!
//! A `ProjectRegistry` is cheap to create — it only reads/writes the JSON file on
//! `load()` / `save()` calls. The in-memory `Vec` is the source of truth while the process
//! is alive.

use crate::projects::project::ProjectEntry;
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use tracing::{debug, info};

/// Default storage file name (relative to the touring config directory).
pub const PROJECTS_FILE: &str = "projects.json";

/// Default directory for touring user data.
pub fn default_touring_dir() -> PathBuf {
    // `home` (rust-lang, the crate cargo itself uses) replaces the unmaintained
    // `dirs` (RUSTSEC-2020-0053); identical `home_dir() -> Option<PathBuf>` API.
    home::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".claude")
        .join("touring")
}

/// `ProjectRegistry` — tracks all registered touring projects.
#[derive(Debug, Clone, Default)]
pub struct ProjectRegistry {
    /// All registered project entries, in insertion order.
    entries: Vec<ProjectEntry>,
    /// Alias of the currently active project (loaded from `current` field in JSON).
    current: Option<String>,
    /// Absolute path to the storage file.
    storage_path: PathBuf,
}

impl ProjectRegistry {
    // ─────────────────────────────────────────────
    //  Constructors
    // ─────────────────────────────────────────────

    /// Create a new registry backed by `storage_path`.
    pub fn new(storage_path: impl Into<PathBuf>) -> Self {
        Self {
            entries: Vec::new(),
            current: None,
            storage_path: storage_path.into(),
        }
    }

    /// Create a registry using the default storage path (`~/.claude/touring/projects.json`).
    pub fn with_default_path() -> Self {
        Self::new(default_touring_dir().join(PROJECTS_FILE))
    }

    // ─────────────────────────────────────────────
    //  Persistence
    // ─────────────────────────────────────────────

    /// Load entries from the JSON file, creating an empty file if it does not exist.
    pub fn load(&mut self) -> Result<()> {
        if !self.storage_path.exists() {
            debug!(
                "projects file not found at {}, starting fresh",
                self.storage_path.display()
            );
            self.entries.clear();
            self.current = None;
            return Ok(());
        }

        let content = std::fs::read_to_string(&self.storage_path)
            .with_context(|| format!("reading {}", self.storage_path.display()))?;

        #[derive(serde::Deserialize)]
        #[serde(try_from = "RawRegistry")]
        struct ValidRegistry {
            entries: Vec<ProjectEntry>,
            current: Option<String>,
        }

        impl TryFrom<RawRegistry> for ValidRegistry {
            type Error = serde_json::Error;
            fn try_from(raw: RawRegistry) -> std::result::Result<Self, Self::Error> {
                Ok(Self {
                    entries: raw.entries,
                    current: raw.current,
                })
            }
        }

        #[derive(serde::Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct RawRegistry {
            entries: Vec<ProjectEntry>,
            #[serde(default)]
            current: Option<String>,
        }

        let ValidRegistry { entries, current } = serde_json::from_str(&content)
            .with_context(|| format!("parsing {}", self.storage_path.display()))?;

        self.entries = entries;
        self.current = current;
        info!(
            "loaded {} project entries from {}",
            self.entries.len(),
            self.storage_path.display()
        );
        Ok(())
    }

    /// Save entries to the JSON file.  Creates the parent directory if needed.
    pub fn save(&self) -> Result<()> {
        if let Some(parent) = self.storage_path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating directory {}", parent.display()))?;
        }

        #[derive(serde::Serialize)]
        #[serde(rename_all = "camelCase")]
        struct RawRegistry<'a> {
            entries: &'a [ProjectEntry],
            current: &'a Option<String>,
        }

        let raw = RawRegistry {
            entries: &self.entries,
            current: &self.current,
        };

        let json = serde_json::to_string_pretty(&raw).context("serializing project registry")?;

        std::fs::write(&self.storage_path, json.as_bytes())
            .with_context(|| format!("writing {}", self.storage_path.display()))?;

        debug!(
            "saved {} project entries to {}",
            self.entries.len(),
            self.storage_path.display()
        );
        Ok(())
    }

    // ─────────────────────────────────────────────
    //  Queries
    // ─────────────────────────────────────────────

    /// Return an iterator over all entries.
    pub fn entries(&self) -> impl ExactSizeIterator<Item = &ProjectEntry> + '_ {
        self.entries.iter()
    }

    /// Return the number of registered projects.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// True if there are no registered projects.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Return the currently active project, if any.
    pub fn current_project(&self) -> Option<&ProjectEntry> {
        self.current
            .as_ref()
            .and_then(|alias| self.find_by_alias(alias))
    }

    /// Find an entry by alias (case-sensitive).
    pub fn find_by_alias(&self, alias: &str) -> Option<&ProjectEntry> {
        self.entries.iter().find(|e| e.alias == alias)
    }

    /// Find a mutable entry by alias.
    pub fn find_by_alias_mut(&mut self, alias: &str) -> Option<&mut ProjectEntry> {
        self.entries.iter_mut().find(|e| e.alias == alias)
    }

    // ─────────────────────────────────────────────
    //  Mutations
    // ─────────────────────────────────────────────

    /// Register a new project.  Returns an error if the alias already exists.
    pub fn add(&mut self, entry: ProjectEntry) -> Result<()> {
        let alias = entry.alias.clone();
        if self.find_by_alias(&alias).is_some() {
            anyhow::bail!("project alias '{}' is already registered", alias);
        }
        self.entries.push(entry);
        debug!("added project '{}'", alias);
        Ok(())
    }

    /// Remove a project by alias.  Returns the removed entry, or `None` if not found.
    pub fn remove(&mut self, alias: &str) -> Option<ProjectEntry> {
        let pos = self.entries.iter().position(|e| e.alias == alias)?;
        let removed = self.entries.remove(pos);

        // Clear current if it pointed to the removed project.
        if self.current.as_deref() == Some(alias) {
            self.current = None;
        }

        debug!("removed project '{}'", alias);
        Some(removed)
    }

    /// Set the currently active project.  Does NOT persist — call `save()` afterward.
    pub fn set_current(&mut self, alias: Option<String>) {
        self.current = alias;
    }

    /// Set a project as the default.  There can only be one default at a time.
    pub fn set_default(&mut self, alias: &str) -> Result<()> {
        // First check the alias exists.
        if !self.entries.iter().any(|e| e.alias == alias) {
            anyhow::bail!("project alias '{}' not found", alias);
        }

        // Unset all other defaults and set the target.
        for e in &mut self.entries {
            e.is_default = e.alias == alias;
        }

        debug!("set '{}' as the default project", alias);
        Ok(())
    }

    /// Touch (update `last_used`) the project with the given alias, or do nothing if
    /// the alias does not exist.
    pub fn touch(&mut self, alias: &str) {
        if let Some(entry) = self.find_by_alias_mut(alias) {
            entry.touch();
        }
    }

    /// Return the storage path used by this registry.
    pub fn storage_path(&self) -> &Path {
        &self.storage_path
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn temp_registry() -> (ProjectRegistry, tempfile::TempDir) {
        let dir = tempdir().unwrap();
        let path = dir.path().join("projects.json");
        (ProjectRegistry::new(path), dir)
    }

    #[test]
    fn test_add_and_find() {
        let (mut reg, _dir) = temp_registry();
        reg.add(ProjectEntry::new("touring", "/repo/touring"))
            .unwrap();
        reg.add(ProjectEntry::new("konverter", "/repo/konverter"))
            .unwrap();
        assert_eq!(reg.len(), 2);
        assert_eq!(reg.find_by_alias("touring").unwrap().alias, "touring");
        assert!(reg.find_by_alias("missing").is_none());
    }

    #[test]
    fn test_duplicate_alias_rejected() {
        let (mut reg, _dir) = temp_registry();
        reg.add(ProjectEntry::new("touring", "/repo/touring"))
            .unwrap();
        let r = reg.add(ProjectEntry::new("touring", "/other/path"));
        assert!(r.is_err());
    }

    #[test]
    fn test_remove() {
        let (mut reg, _dir) = temp_registry();
        reg.add(ProjectEntry::new("touring", "/repo/touring"))
            .unwrap();
        let removed = reg.remove("touring");
        assert!(removed.is_some());
        assert_eq!(reg.len(), 0);
    }

    #[test]
    fn test_set_default() {
        let (mut reg, _dir) = temp_registry();
        reg.add(ProjectEntry::new("a", "/a")).unwrap();
        reg.add(ProjectEntry::new("b", "/b")).unwrap();

        // Set 'b' as default.
        reg.set_default("b").unwrap();
        assert!(!reg.find_by_alias("a").unwrap().is_default);
        assert!(reg.find_by_alias("b").unwrap().is_default);

        // Only one default at a time.
        reg.set_default("a").unwrap();
        assert!(reg.find_by_alias("a").unwrap().is_default);
        assert!(!reg.find_by_alias("b").unwrap().is_default);
    }

    #[test]
    fn test_current_project() {
        let (mut reg, _dir) = temp_registry();
        reg.add(ProjectEntry::new("touring", "/repo/touring"))
            .unwrap();
        reg.set_current(Some("touring".into()));
        assert_eq!(reg.current_project().unwrap().alias, "touring");

        reg.set_current(None);
        assert!(reg.current_project().is_none());
    }

    #[test]
    fn test_save_load_roundtrip() {
        let (mut reg, _dir) = temp_registry();
        let path = reg.storage_path().to_path_buf();

        reg.add(ProjectEntry::new("touring", "/repo/touring"))
            .unwrap();
        reg.add(ProjectEntry::new("konverter", "/repo/konverter"))
            .unwrap();
        reg.set_default("touring").unwrap();
        reg.set_current(Some("touring".into()));
        reg.save().unwrap();

        let mut reg2 = ProjectRegistry::new(path);
        reg2.load().unwrap();
        assert_eq!(reg2.len(), 2);
        assert!(reg2.find_by_alias("touring").unwrap().is_default);
        assert_eq!(reg2.current_project().unwrap().alias, "touring");
    }

    #[test]
    fn test_load_nonexistent_is_empty() {
        let (mut reg, _dir) = temp_registry();
        reg.add(ProjectEntry::new("touring", "/repo/touring"))
            .unwrap();
        // Override path to a non-existent file.
        let fresh = ProjectRegistry::new("/nonexistent/path/projects.json");
        let mut empty = ProjectRegistry::new(fresh.storage_path().to_path_buf());
        empty.load().unwrap(); // Must not error.
        assert!(empty.is_empty());
    }
}
