//! FileSet — query VFS by path, FileId, or glob pattern.

use crate::vfs::abs_path::{AbsPath, AbsPathBuf};
use crate::vfs::file_id::FileId;
use crate::vfs::vfs::Vfs;
use std::collections::HashMap;

/// Queryable view over a [`Vfs`] by path, [`FileId`], or glob pattern.
pub struct FileSet {
    vfs: Vfs,
    /// Additional in-memory overlay paths (not tracked by Vfs yet)
    overlay_paths: HashMap<String, FileId>,
}

impl FileSet {
    /// Build a `FileSet` wrapping the given [`Vfs`] with an empty overlay.
    pub fn new(vfs: Vfs) -> Self {
        FileSet {
            vfs,
            overlay_paths: HashMap::new(),
        }
    }

    /// Register an overlay path-to-[`FileId`] mapping not yet tracked by the [`Vfs`].
    pub fn add_path(&mut self, path: &AbsPath, id: FileId) {
        self.overlay_paths.insert(path.as_str().to_string(), id);
    }

    /// Find all files matching a glob pattern under a root
    pub fn glob(&self, root: &AbsPath, pattern: &str) -> Vec<AbsPathBuf> {
        let root_str = root.as_str();
        let full_pattern = format!("{}/{}", root_str.trim_end_matches('/'), pattern);

        let mut results = Vec::new();

        // Helper: check if path is under root (exact prefix with / boundary)
        let is_under_root = |path: &str| -> bool {
            if root_str == "/" {
                // Root is filesystem root — all absolute paths are under it
                return path.starts_with('/');
            }
            path.starts_with(root_str)
                && (path.len() == root_str.len() || path[root_str.len()..].starts_with('/'))
        };

        // Check overlay paths
        for path in self.overlay_paths.keys() {
            if is_under_root(path) && simple_glob_match(path, &full_pattern) {
                results.push(AbsPathBuf::from_maybe_unsafe(path.clone()));
            }
        }

        // Check VFS paths
        for path in self.vfs.paths() {
            let s = path.as_str();
            if is_under_root(s)
                && simple_glob_match(s, &full_pattern)
                && !results.iter().any(|r| r.as_str() == s)
            {
                results.push(path);
            }
        }

        results
    }

    /// Find all files under a directory
    pub fn list_dir(&self, dir: &AbsPath) -> Vec<AbsPathBuf> {
        let dir_str = dir.as_str();
        let mut results = Vec::new();

        for path in self.overlay_paths.keys() {
            if path.starts_with(dir_str) && !path.ends_with('/') {
                results.push(AbsPathBuf::from_maybe_unsafe(path.clone()));
            }
        }

        for path in self.vfs.paths() {
            let s = path.as_str();
            if s.starts_with(dir_str) && !results.iter().any(|r| r.as_str() == s) {
                results.push(path);
            }
        }

        results
    }

    /// Get file ID for path
    pub fn get(&self, path: &AbsPath) -> Option<FileId> {
        self.overlay_paths
            .get(path.as_str())
            .copied()
            .or_else(|| self.vfs.file_id(path))
    }

    /// Get path for file ID
    pub fn path_for_id(&self, id: FileId) -> Option<AbsPathBuf> {
        for (path, fid) in &self.overlay_paths {
            if *fid == id {
                return Some(AbsPathBuf::from_maybe_unsafe(path.clone()));
            }
        }
        self.vfs.path(id)
    }

    /// Number of tracked paths
    pub fn len(&self) -> usize {
        self.overlay_paths.len() + self.vfs.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Simple glob matching:
/// - "*" matches any characters except "/"
/// - "**" matches zero or more path segments
fn simple_glob_match(path: &str, pattern: &str) -> bool {
    let pattern = pattern.trim_start_matches("./");

    // Handle ** — matches zero or more path segments
    if let Some(dbl) = pattern.find("**") {
        let prefix = &pattern[..dbl];
        let suffix = &pattern[dbl + 2..];
        let prefix_trimmed = prefix.trim_end_matches('/');

        // Prefix must match at path boundary
        let prefix_ok = if prefix.is_empty() {
            true
        } else {
            path.starts_with(prefix_trimmed)
                && (path.len() == prefix_trimmed.len()
                    || path[prefix_trimmed.len()..].starts_with('/'))
        };
        if !prefix_ok {
            return false;
        }

        // Remaining path after prefix
        let remaining: &str = if prefix_trimmed.is_empty() {
            path
        } else if path.starts_with(prefix_trimmed) {
            let after = path
                .strip_prefix(prefix_trimmed)
                .expect("guarded by starts_with(prefix_trimmed)");
            if after.is_empty() {
                ""
            } else if let Some(rest) = after.strip_prefix('/') {
                rest
            } else {
                return false;
            }
        } else {
            return false;
        };

        // Split remaining into segments for ** skip counting
        let segs: Vec<&str> = remaining
            .split('/')
            .filter(|s: &&str| !s.is_empty())
            .collect();
        let suffix_clean = suffix.strip_prefix('/').unwrap_or(suffix);

        // Try matching suffix at each skip point (0..=segs.len())
        for skip in 0..=segs.len() {
            let candidate = segs[skip..].join("/");
            if match_end(&candidate, suffix_clean) {
                return true;
            }
        }
        // Also try matching suffix against remaining as-is
        return match_end(remaining, suffix_clean);
    }

    // No ** — simple * matching
    match_end(path, pattern)
}

/// Match path against a pattern with * wildcards (no **).
/// * matches any characters EXCEPT "/".
///   Returns true if path matches the pattern.
fn match_end(path: &str, pattern: &str) -> bool {
    if !pattern.contains('*') {
        return path == pattern;
    }

    // Handle end-anchored: pattern starts with *
    if pattern.starts_with('*') {
        // suffix = everything after the leading *
        let suffix = pattern.strip_prefix('*').unwrap_or("");
        if !suffix.is_empty() && !path.ends_with(suffix) {
            return false;
        }
        // The part before suffix is the *-matched portion
        // For a single * at start, it must not cross / (matches one segment only)
        // For ** (which we've already handled in simple_glob_match), this branch
        // should only be reached for simple * at start patterns
        let before = if suffix.is_empty() {
            path
        } else {
            &path[..path.len() - suffix.len()]
        };
        // Single * at start: must not span across / boundary
        // ** at start: already handled in simple_glob_match, this is simple * case
        return !before.contains('/');
    }

    // Start-anchored: first segment must match start of path
    let segs: Vec<&str> = pattern.split('*').collect();
    let first = segs.first().unwrap_or(&"");
    if !first.is_empty() && !path.starts_with(first) {
        return false;
    }

    // Single-star: prefix matches start, suffix matches end, * in between
    let count = pattern.matches('*').count();
    let last = segs.last().unwrap_or(&"");

    if count == 1 {
        if last.is_empty() {
            return true; // pattern ends with *, match all after prefix
        }
        // suffix must be at end, and middle must not contain /
        let end_idx = path.len() - last.len();
        if end_idx < first.len() {
            return false;
        }
        if &path[end_idx..] != *last {
            return false;
        }
        let middle = &path[first.len()..end_idx];
        return !middle.contains('/');
    }

    // Multiple stars: non-last segments must appear in sequence
    let mut pos = first.len();
    for seg in segs.iter().skip(1) {
        if seg.is_empty() {
            continue;
        }
        if let Some(idx) = path[pos..].find(seg) {
            pos += idx + seg.len();
        } else {
            return false;
        }
    }
    pos == path.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_vfs() -> Vfs {
        Vfs::new()
    }

    #[test]
    fn file_set_new() {
        let vfs = make_vfs();
        let fs = FileSet::new(vfs);
        assert!(fs.is_empty());
    }

    #[test]
    fn file_set_add_path() {
        let vfs = make_vfs();
        let mut fs = FileSet::new(vfs);
        let path = AbsPath::from_absolute("/src/main.rs").unwrap();
        fs.add_path(path, FileId::new(1));
        assert_eq!(fs.get(path), Some(FileId::new(1)));
    }

    #[test]
    fn file_set_glob_simple() {
        let vfs = make_vfs();
        let mut fs = FileSet::new(vfs);
        fs.add_path(
            AbsPath::from_absolute("/src/foo.rs").unwrap(),
            FileId::new(1),
        );
        fs.add_path(
            AbsPath::from_absolute("/src/bar.rs").unwrap(),
            FileId::new(2),
        );
        let results = fs.glob(AbsPath::from_absolute("/src").unwrap(), "*.rs");
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn file_set_list_dir() {
        let vfs = make_vfs();
        let mut fs = FileSet::new(vfs);
        fs.add_path(
            AbsPath::from_absolute("/src/foo.rs").unwrap(),
            FileId::new(1),
        );
        fs.add_path(
            AbsPath::from_absolute("/src/bar.rs").unwrap(),
            FileId::new(2),
        );
        let dir = AbsPath::from_absolute("/src").unwrap();
        let results = fs.list_dir(dir);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn simple_glob_matches_star() {
        assert!(simple_glob_match("foo.rs", "*.rs"));
        assert!(!simple_glob_match("bar.txt", "*.rs"));
    }

    #[test]
    fn simple_glob_matches_double_star() {
        assert!(simple_glob_match("/src/foo/bar.rs", "**/*.rs"));
    }
}
