//! Persistence for the mined portfolio.
//!
//! The index is **global**, not per-project: a PDF strategy written inside
//! `detran-document-elaborator` must surface when the intent appears in
//! `analise`. It therefore lives in the L2 toolchain home (`~/.touring/`)
//! rather than in any project's `symbols.db`, whose contract (wiring, blast
//! radius) is per-project and whose size is already ~187 MB.
//!
//! Format is plain JSON: the corpus is ~4k records and loads in tens of
//! milliseconds, which is well inside the budget for a CLI call or a hook.

use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use super::CapabilityEntry;

/// Schema version — bumped when [`PortfolioIndex`] changes shape OR when the
/// corpus definition widens, so a stale on-disk index is rebuilt rather than
/// silently serving an incomplete one. v2 (2026-08-08) added `Symbol` entries.
pub const INDEX_VERSION: u32 = 2;

/// The materialized portfolio.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortfolioIndex {
    /// Schema version of this file.
    pub version: u32,
    /// RFC3339-ish build timestamp, for staleness reporting.
    pub built_at: String,
    /// Roots that were mined, echoed so a reader knows the coverage.
    pub roots: Vec<String>,
    /// The records.
    pub entries: Vec<CapabilityEntry>,
}

impl PortfolioIndex {
    /// An empty index — what a query gets before the first refresh.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            version: INDEX_VERSION,
            built_at: String::new(),
            roots: Vec::new(),
            entries: Vec::new(),
        }
    }

    /// True when nothing has been mined yet.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Directory holding the global portfolio index.
///
/// Honours `TOURING_PORTFOLIO_DIR` so tests and per-project experiments never
/// write to the real index.
#[must_use]
pub fn index_dir() -> PathBuf {
    if let Ok(custom) = std::env::var("TOURING_PORTFOLIO_DIR") {
        return PathBuf::from(custom);
    }
    home::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".touring/portfolio")
}

/// Full path of the index file.
#[must_use]
pub fn index_path() -> PathBuf {
    index_dir().join("index.json")
}

/// Write the index atomically into `dir` (temp file + rename) so a concurrent
/// reader never observes a half-written file.
///
/// Takes the directory explicitly rather than reading global state, so tests
/// and per-project experiments target a scratch path without mutating the
/// process environment.
///
/// # Errors
/// Returns an error if the directory cannot be created or the file cannot be
/// written or renamed.
pub fn save_to(dir: &std::path::Path, index: &PortfolioIndex) -> Result<PathBuf> {
    std::fs::create_dir_all(dir)
        .with_context(|| format!("creating portfolio dir {}", dir.display()))?;
    let path = dir.join("index.json");
    let tmp = dir.join(format!("index.json.tmp{}", std::process::id()));
    let json = serde_json::to_vec_pretty(index).context("serializing portfolio index")?;
    std::fs::write(&tmp, &json).with_context(|| format!("writing {}", tmp.display()))?;
    std::fs::rename(&tmp, &path).with_context(|| format!("renaming into {}", path.display()))?;
    Ok(path)
}

/// Write the index to the default location ([`index_dir`]).
///
/// # Errors
/// See [`save_to`].
pub fn save(index: &PortfolioIndex) -> Result<PathBuf> {
    save_to(&index_dir(), index)
}

/// Load the index from `dir`, or [`PortfolioIndex::empty`] when absent.
///
/// A version mismatch or a corrupt file is treated as absent: better an honest
/// empty answer than a misparsed one.
///
/// # Errors
/// Returns an error only when the file exists but cannot be read.
pub fn load_from(dir: &std::path::Path) -> Result<PortfolioIndex> {
    let path = dir.join("index.json");
    if !path.exists() {
        return Ok(PortfolioIndex::empty());
    }
    let bytes =
        std::fs::read(&path).with_context(|| format!("reading portfolio index {}", path.display()))?;
    match serde_json::from_slice::<PortfolioIndex>(&bytes) {
        Ok(idx) if idx.version == INDEX_VERSION => Ok(idx),
        _ => Ok(PortfolioIndex::empty()),
    }
}

/// Load the index from the default location ([`index_dir`]).
///
/// # Errors
/// See [`load_from`].
pub fn load() -> Result<PortfolioIndex> {
    load_from(&index_dir())
}

/// Current timestamp in the format stored in `built_at`.
#[must_use]
pub fn now_stamp() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or_else(|_| "unknown".to_string(), |d| format!("epoch:{}", d.as_secs()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::portfolio::{CapabilityKind, Evidence};

    /// A scratch directory that cleans itself up. No environment mutation — the
    /// store takes its directory as a parameter precisely so tests stay pure.
    struct ScopedDir(PathBuf);

    impl ScopedDir {
        fn new(tag: &str) -> Self {
            let dir = std::env::temp_dir()
                .join(format!("portfolio-store-{tag}-{}", std::process::id()));
            std::fs::create_dir_all(&dir).expect("mkdir");
            Self(dir)
        }

        fn path(&self) -> &std::path::Path {
            &self.0
        }
    }

    impl Drop for ScopedDir {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.0).ok();
        }
    }

    fn sample() -> CapabilityEntry {
        CapabilityEntry {
            id: CapabilityEntry::make_id(CapabilityKind::Script, "~/a/b.py"),
            display_path: "~/a/b.py".to_string(),
            kind: CapabilityKind::Script,
            name: "b".to_string(),
            purpose: "Generate a professional PDF from an HTML template".to_string(),
            language: "python".to_string(),
            entry_point: Some("python3 ~/a/b.py".to_string()),
            provenance: "skill:pdf".to_string(),
            keywords: vec!["pdf".to_string()],
            evidence: Evidence::default(),
            purpose_inherited: false,
        }
    }

    #[test]
    fn save_then_load_roundtrips() {
        let scope = ScopedDir::new("roundtrip");
        let idx = PortfolioIndex {
            version: INDEX_VERSION,
            built_at: now_stamp(),
            roots: vec!["~/x".to_string()],
            entries: vec![sample()],
        };
        save_to(scope.path(), &idx).expect("save");
        let back = load_from(scope.path()).expect("load");
        assert_eq!(back.entries, idx.entries);
        assert_eq!(back.roots, idx.roots);
    }

    #[test]
    fn missing_index_loads_empty_rather_than_failing() {
        let scope = ScopedDir::new("missing");
        let idx = load_from(scope.path()).expect("load must not error on absence");
        assert!(idx.is_empty());
    }

    #[test]
    fn version_mismatch_is_treated_as_absent() {
        let scope = ScopedDir::new("version");
        std::fs::write(
            scope.path().join("index.json"),
            serde_json::json!({"version": 999, "built_at": "", "roots": [], "entries": []})
                .to_string(),
        )
        .expect("write");
        assert!(
            load_from(scope.path()).expect("load").is_empty(),
            "future schema must not be misread"
        );
    }

    #[test]
    fn corrupt_index_loads_empty_rather_than_panicking() {
        let scope = ScopedDir::new("corrupt");
        std::fs::write(scope.path().join("index.json"), b"{not json").expect("write");
        assert!(load_from(scope.path()).expect("load").is_empty());
    }

    #[test]
    fn save_leaves_no_temp_file_behind() {
        let scope = ScopedDir::new("atomic");
        let idx = PortfolioIndex {
            version: INDEX_VERSION,
            built_at: now_stamp(),
            roots: vec![],
            entries: vec![sample()],
        };
        save_to(scope.path(), &idx).expect("save");
        let leftovers: Vec<_> = std::fs::read_dir(scope.path())
            .expect("readdir")
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().contains(".tmp"))
            .collect();
        assert!(leftovers.is_empty(), "temp file leaked: {leftovers:?}");
    }

    #[test]
    fn default_dir_honours_the_env_override() {
        // Read-only check: the override is consulted, so a per-project run can
        // redirect the index without code changes.
        match std::env::var("TOURING_PORTFOLIO_DIR") {
            Ok(custom) => assert_eq!(index_dir(), PathBuf::from(custom)),
            Err(_) => assert!(
                index_path().ends_with("portfolio/index.json"),
                "unexpected default: {}",
                index_path().display()
            ),
        }
    }
}
