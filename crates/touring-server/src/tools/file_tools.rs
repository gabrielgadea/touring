//! File Operations Tool - touring_file_ops
//!
//! Implements file operations:
//! - read: Read file content
//! - write: Atomic file write (tmp + rename)
//! - find: Search files by pattern
//! - list: Directory listing

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use thiserror::Error;
use touring_foundation::config::TouringConfig;
use tracing::{debug, info};

/// Errors specific to file operations
#[derive(Error, Debug)]
pub(crate) enum FileToolsError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("File not found: {0}")]
    NotFound(PathBuf),

    #[error("Path is not a file: {0}")]
    NotAFile(PathBuf),

    #[error("Path is not a directory: {0}")]
    NotADirectory(PathBuf),

    #[error("Invalid operation: {0}")]
    InvalidOperation(String),

    #[error("Regex error: {0}")]
    Regex(#[from] regex::Error),

    #[error("Path outside project root: {0}")]
    PathOutsideRoot(PathBuf),
}

/// Result type for file tool operations
pub(crate) type Result<T> = std::result::Result<T, FileToolsError>;

/// Resolve `requested` and confirm it stays within one of `roots` — the single source of
/// truth for filesystem path-jailing (SEC-01 containment core).
///
/// Used by both the CLI [`FileTools::validate_path`] guard and the always-on
/// `touring_file_ops` MCP tool. Without this, that tool is an arbitrary read/write/delete
/// primitive reachable via prompt-injection.
///
/// - `must_exist == true` requires the target itself to resolve (read/delete/stat/copy-src…).
/// - `must_exist == false` validates the canonical *parent* so create/write/mkdir can
///   target a not-yet-existing leaf.
///
/// Canonicalization resolves `..` and symlinks, defeating traversal escapes. A bare leaf
/// name with no parent is resolved against the current working directory. Returns the
/// canonical in-root path on success, or the offending canonical path on denial.
pub(crate) fn enforce_path_within_roots(
    requested: &Path,
    roots: &[PathBuf],
    must_exist: bool,
) -> std::result::Result<PathBuf, PathBuf> {
    let canonical =
        resolve_canonical_target(requested, must_exist).ok_or_else(|| requested.to_path_buf())?;
    if roots.iter().any(|root| canonical.starts_with(root)) {
        Ok(canonical)
    } else {
        Err(canonical)
    }
}

/// Canonicalize `requested` for containment checking, resolving `..`/symlinks.
/// Returns `None` when the path cannot be resolved (missing target with
/// `must_exist`, unresolvable parent, or no leaf name).
fn resolve_canonical_target(requested: &Path, must_exist: bool) -> Option<PathBuf> {
    if requested.exists() {
        return requested.canonicalize().ok();
    }
    if must_exist {
        return None;
    }
    if let Some(parent) = requested.parent().filter(|p| !p.as_os_str().is_empty()) {
        let cparent = parent.canonicalize().ok()?;
        return Some(match requested.file_name() {
            Some(name) => cparent.join(name),
            None => cparent,
        });
    }
    // Bare leaf name (no parent component) → resolve against the current directory.
    let cwd = std::env::current_dir().ok()?;
    requested.file_name().map(|name| cwd.join(name))
}

/// File operation types
#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum FileOperation {
    /// Read file content
    Read,
    /// Write file content
    Write,
    /// Find files by pattern
    Find,
    /// List directory contents
    List,
}

impl std::fmt::Display for FileOperation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FileOperation::Read => write!(f, "read"),
            FileOperation::Write => write!(f, "write"),
            FileOperation::Find => write!(f, "find"),
            FileOperation::List => write!(f, "list"),
        }
    }
}

/// Input for touring_file_ops tool
#[derive(Debug, Clone, Deserialize)]
pub struct FileOpsInput {
    /// Operation to perform
    pub operation: FileOperation,
    /// Path to file or directory
    pub path: String,
    /// Content for write operations
    pub content: Option<String>,
    /// Pattern for find operations
    pub pattern: Option<String>,
    /// Maximum depth for recursive operations
    #[serde(default = "default_max_depth")]
    pub max_depth: usize,
    /// Include hidden files
    #[serde(default = "default_include_hidden")]
    pub include_hidden: bool,
}

fn default_max_depth() -> usize {
    10
}

fn default_include_hidden() -> bool {
    false
}

/// File information
#[derive(Debug, Clone, Serialize)]
pub struct FileInfo {
    /// File name
    pub name: String,
    /// Full path
    pub path: String,
    /// File size in bytes
    pub size: u64,
    /// Is directory
    pub is_dir: bool,
    /// Is file
    pub is_file: bool,
    /// Modification time (Unix timestamp)
    pub modified: Option<u64>,
}

/// Output from touring_file_ops tool
#[derive(Debug, Clone, Serialize)]
pub struct FileOpsOutput {
    /// Success status
    pub success: bool,
    /// Operation performed
    pub operation: String,
    /// File content (for read operations)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    /// List of files (for find/list operations)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub files: Option<Vec<FileInfo>>,
    /// Error message if operation failed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Number of results (for find operations)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub count: Option<usize>,
}

/// File tools handler
#[derive(Debug)]
pub struct FileTools {
    project_root: PathBuf,
}

impl FileTools {
    /// Create new file tools handler
    pub fn new(config: &TouringConfig) -> Self {
        Self {
            project_root: config.project_root.clone(),
        }
    }

    /// Validate that a path is within the project root.
    ///
    /// Delegates to [`enforce_path_within_roots`] (the shared containment core) so the CLI
    /// guard and the `touring_file_ops` MCP guard cannot diverge. `must_exist = false`
    /// preserves the original behaviour of validating the parent for not-yet-existing leaves.
    fn validate_path(&self, path: &Path) -> Result<PathBuf> {
        enforce_path_within_roots(path, std::slice::from_ref(&self.project_root), false)
            .map_err(FileToolsError::PathOutsideRoot)
    }

    /// Handle file read operation
    fn read_file(&self, path: &Path) -> Result<FileOpsOutput> {
        let path = self.validate_path(path)?;

        if !path.exists() {
            return Err(FileToolsError::NotFound(path));
        }

        if !path.is_file() {
            return Err(FileToolsError::NotAFile(path));
        }

        let content = std::fs::read_to_string(&path)?;

        Ok(FileOpsOutput {
            success: true,
            operation: "read".to_string(),
            content: Some(content),
            files: None,
            error: None,
            count: None,
        })
    }

    /// Handle file write operation (atomic)
    fn write_file(&self, path: &Path, content: &str) -> Result<FileOpsOutput> {
        let path = self.validate_path(path)?;

        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Atomic write: write to temp file, then rename
        let temp_path = path.with_extension("tmp");
        std::fs::write(&temp_path, content)?;
        std::fs::rename(&temp_path, &path)?;

        info!("Wrote file atomically: {:?}", path);

        Ok(FileOpsOutput {
            success: true,
            operation: "write".to_string(),
            content: None,
            files: None,
            error: None,
            count: None,
        })
    }

    /// Handle file find operation
    fn find_files(
        &self,
        path: &Path,
        pattern: Option<&str>,
        max_depth: usize,
        include_hidden: bool,
    ) -> Result<FileOpsOutput> {
        let path = self.validate_path(path)?;

        if !path.exists() {
            return Err(FileToolsError::NotFound(path));
        }

        if !path.is_dir() {
            return Err(FileToolsError::NotADirectory(path));
        }

        let regex = pattern.map(Regex::new).transpose()?.map(|r| r.to_string());

        let mut files = Vec::new();
        self.find_recursive(
            &path,
            &path,
            regex.as_deref(),
            max_depth,
            0,
            include_hidden,
            &mut files,
        )?;

        let count = files.len();

        Ok(FileOpsOutput {
            success: true,
            operation: "find".to_string(),
            content: None,
            files: Some(files),
            error: None,
            count: Some(count),
        })
    }

    /// Recursive file finder
    #[allow(clippy::too_many_arguments)]
    fn find_recursive(
        &self,
        root: &Path,
        current: &Path,
        pattern: Option<&str>,
        max_depth: usize,
        current_depth: usize,
        include_hidden: bool,
        results: &mut Vec<FileInfo>,
    ) -> Result<()> {
        if current_depth >= max_depth {
            return Ok(());
        }

        let entries = std::fs::read_dir(current)?;

        for entry in entries {
            let entry = entry?;
            let path = entry.path();

            // Skip symlinks to prevent traversal attacks
            if path
                .symlink_metadata()
                .is_ok_and(|m| m.file_type().is_symlink())
            {
                continue;
            }

            let name = entry.file_name().to_string_lossy().to_string();

            if !self.should_include_entry(&name, path.is_dir(), include_hidden) {
                continue;
            }

            if let Some(file_info) = self.process_entry(&path, &name, root, pattern)? {
                results.push(file_info);
            }

            // Recurse into directories
            if path.is_dir() && current_depth < max_depth {
                self.find_recursive(
                    root,
                    &path,
                    pattern,
                    max_depth,
                    current_depth + 1,
                    include_hidden,
                    results,
                )?;
            }
        }

        Ok(())
    }

    /// Check if entry should be included based on name and type.
    fn should_include_entry(&self, name: &str, is_dir: bool, include_hidden: bool) -> bool {
        if !include_hidden && name.starts_with('.') {
            return false;
        }
        if is_dir && Self::is_ignored_dir(name) {
            return false;
        }
        true
    }

    /// Check if directory should be ignored.
    fn is_ignored_dir(name: &str) -> bool {
        matches!(
            name.to_lowercase().as_str(),
            ".git" | "target" | "node_modules" | "__pycache__" | ".venv" | "venv"
        )
    }

    /// Process a single entry and return FileInfo if it matches the pattern.
    fn process_entry(
        &self,
        path: &Path,
        name: &str,
        root: &Path,
        pattern: Option<&str>,
    ) -> Result<Option<FileInfo>> {
        let metadata = std::fs::metadata(path)?;
        let modified = metadata
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs());

        let file_info = FileInfo {
            name: name.to_string(),
            path: path
                .strip_prefix(root)
                .unwrap_or(path)
                .to_string_lossy()
                .to_string(),
            size: metadata.len(),
            is_dir: metadata.is_dir(),
            is_file: metadata.is_file(),
            modified,
        };

        // Check pattern match
        let matches = if let Some(pat) = pattern {
            match Regex::new(pat) {
                Ok(regex) => regex.is_match(name),
                _ => name.contains(pat),
            }
        } else {
            true
        };

        Ok(if matches { Some(file_info) } else { None })
    }

    /// Handle directory list operation
    fn list_directory(&self, path: &Path, include_hidden: bool) -> Result<FileOpsOutput> {
        let path = self.validate_path(path)?;

        if !path.exists() {
            return Err(FileToolsError::NotFound(path));
        }

        if !path.is_dir() {
            return Err(FileToolsError::NotADirectory(path));
        }

        let mut files = Vec::new();
        let entries = std::fs::read_dir(&path)?;

        for entry in entries {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().to_string();

            // Skip hidden files unless requested
            if !include_hidden && name.starts_with('.') {
                continue;
            }

            let metadata = entry.metadata()?;
            let modified = metadata
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs());

            files.push(FileInfo {
                name,
                path: entry.path().to_string_lossy().to_string(),
                size: metadata.len(),
                is_dir: metadata.is_dir(),
                is_file: metadata.is_file(),
                modified,
            });
        }

        // Sort: directories first, then alphabetically
        files.sort_by(|a, b| match (a.is_dir, b.is_dir) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.name.cmp(&b.name),
        });

        let count = files.len();

        Ok(FileOpsOutput {
            success: true,
            operation: "list".to_string(),
            content: None,
            files: Some(files),
            error: None,
            count: Some(count),
        })
    }

    /// Handle touring_file_ops tool
    pub fn handle(&self, input: FileOpsInput) -> FileOpsOutput {
        debug!("FileOps: {:?} on {}", input.operation, input.path);

        let path = PathBuf::from(&input.path);

        let result = match input.operation {
            FileOperation::Read => self.read_file(&path),
            FileOperation::Write => {
                if let Some(content) = input.content {
                    self.write_file(&path, &content)
                } else {
                    Err(FileToolsError::InvalidOperation(
                        "Write operation requires content".to_string(),
                    ))
                }
            }
            FileOperation::Find => self.find_files(
                &path,
                input.pattern.as_deref(),
                input.max_depth,
                input.include_hidden,
            ),
            FileOperation::List => self.list_directory(&path, input.include_hidden),
        };

        match result {
            Ok(output) => output,
            Err(e) => FileOpsOutput {
                success: false,
                operation: input.operation.to_string(),
                content: None,
                files: None,
                error: Some(e.to_string()),
                count: None,
            },
        }
    }

    /// Handle async version (for compatibility)
    pub async fn handle_async(&self, input: FileOpsInput) -> FileOpsOutput {
        self.handle(input)
    }
}

/// Convert a glob pattern to a case-insensitive regex string.
/// Supports: `*` (non-separator), `**` (any path), `?` (one char), literal chars.
pub(crate) fn glob_to_regex(glob: &str) -> String {
    let mut out = String::from("(?i)");
    let chars: Vec<char> = glob.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        // Safe: loop guard ensures i < chars.len()
        let c = chars.get(i).copied().unwrap_or('\0');
        let next = chars.get(i + 1).copied();
        match c {
            '*' if next == Some('*') => {
                out.push_str(".*");
                i += 2;
                // skip leading slash after **
                if chars.get(i).copied() == Some('/') {
                    i += 1;
                }
            }
            '*' => {
                out.push_str("[^/]*");
                i += 1;
            }
            '?' => {
                out.push_str("[^/]");
                i += 1;
            }
            c if ".[+^$|()".contains(c) || c == ']' || c == '\\' => {
                out.push('\\');
                out.push(c);
                i += 1;
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    out
}

/// Find files matching a glob or regex pattern, optionally filtered by content.
/// NOT restricted to project_root — works with any absolute or relative path.
pub(crate) fn find_workspace_files(
    root: &std::path::Path,
    pattern: &str,
    use_regex: bool,
    content_pattern: Option<&str>,
    max_depth: usize,
    include_hidden: bool,
) -> Vec<String> {
    let pat_str = if use_regex {
        format!("(?i){pattern}")
    } else {
        glob_to_regex(pattern)
    };
    let Ok(pat_re) = Regex::new(&pat_str) else {
        return Vec::new();
    };
    let content_re = content_pattern.and_then(|p| Regex::new(&format!("(?i){p}")).ok());

    let mut results = Vec::new();
    find_ws_recursive(
        root,
        &pat_re,
        content_re.as_ref(),
        max_depth,
        0,
        include_hidden,
        &mut results,
    );
    results.sort();
    results
}

fn find_ws_recursive(
    dir: &std::path::Path,
    pattern: &Regex,
    content_re: Option<&Regex>,
    max_depth: usize,
    depth: usize,
    include_hidden: bool,
    results: &mut Vec<String>,
) {
    if depth > max_depth {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();

        if !include_hidden && name.starts_with('.') {
            continue;
        }
        // SEC-04: classify via the entry's own type (does NOT follow symlinks).
        // A symlink inside the jailed root must not be followed, or a target
        // like `/etc` reachable through an in-jail symlink would escape the root.
        // Mirrors the guard in `FileFinder::find_recursive`.
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            if matches!(
                name.as_str(),
                "target" | "node_modules" | "__pycache__" | ".venv" | "venv" | ".git"
            ) {
                continue;
            }
            find_ws_recursive(
                &path,
                pattern,
                content_re,
                max_depth,
                depth + 1,
                include_hidden,
                results,
            );
        } else if file_type.is_file() {
            let path_str = path.to_string_lossy();
            if pattern.is_match(&path_str) {
                if let Some(cre) = content_re {
                    if let Ok(content) = std::fs::read_to_string(&path)
                        && cre.is_match(&content)
                    {
                        results.push(path_str.into_owned());
                    }
                } else {
                    results.push(path_str.into_owned());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use tempfile::TempDir;

    fn create_test_config(temp_dir: &TempDir) -> TouringConfig {
        TouringConfig {
            project_root: temp_dir.path().to_path_buf(),
            symbols_db_path: temp_dir.path().join("symbols.db"),
            rlm_db_path: temp_dir.path().join("rlm.db"),
            semantic_db_path: temp_dir.path().join("semantic.db"),
            cache_size: 1000,
            watcher_debounce_ms: 100,
            max_file_size: 1024 * 1024,
            debug: false,
            gpu_service_url: "http://localhost:8200".to_string(),
            embedding_dim: 384,
            auto_embed: false,
            ..Default::default()
        }
    }

    #[test]
    fn test_read_file() {
        let temp_dir = TempDir::new().unwrap();
        let config = create_test_config(&temp_dir);
        let tools = FileTools::new(&config);

        // Create test file
        let test_path = temp_dir.path().join("test.txt");
        std::fs::write(&test_path, "Hello, World!").unwrap();

        let input = FileOpsInput {
            operation: FileOperation::Read,
            path: test_path.to_string_lossy().to_string(),
            content: None,
            pattern: None,
            max_depth: 10,
            include_hidden: false,
        };

        let output = tools.handle(input);
        assert!(output.success);
        assert_eq!(output.content, Some("Hello, World!".to_string()));
    }

    #[test]
    fn test_write_file() {
        let temp_dir = TempDir::new().unwrap();
        let config = create_test_config(&temp_dir);
        let tools = FileTools::new(&config);

        let test_path = temp_dir.path().join("write_test.txt");

        let input = FileOpsInput {
            operation: FileOperation::Write,
            path: test_path.to_string_lossy().to_string(),
            content: Some("Written content".to_string()),
            pattern: None,
            max_depth: 10,
            include_hidden: false,
        };

        let output = tools.handle(input);
        assert!(output.success);

        // Verify file was written
        let content = std::fs::read_to_string(&test_path).unwrap();
        assert_eq!(content, "Written content");
    }

    #[test]
    fn test_list_directory() {
        let temp_dir = TempDir::new().unwrap();
        let config = create_test_config(&temp_dir);
        let tools = FileTools::new(&config);

        // Create some files
        std::fs::write(temp_dir.path().join("file1.txt"), "content1").unwrap();
        std::fs::write(temp_dir.path().join("file2.txt"), "content2").unwrap();
        std::fs::create_dir(temp_dir.path().join("subdir")).unwrap();

        let input = FileOpsInput {
            operation: FileOperation::List,
            path: temp_dir.path().to_string_lossy().to_string(),
            content: None,
            pattern: None,
            max_depth: 10,
            include_hidden: false,
        };

        let output = tools.handle(input);
        assert!(output.success);
        assert!(output.files.is_some());

        let files = output.files.unwrap();
        assert_eq!(files.len(), 3);
        assert!(files.iter().any(|f| f.name == "file1.txt"));
        assert!(files.iter().any(|f| f.name == "file2.txt"));
        assert!(files.iter().any(|f| f.name == "subdir" && f.is_dir));
    }

    #[test]
    fn test_find_files() {
        let temp_dir = TempDir::new().unwrap();
        let config = create_test_config(&temp_dir);
        let tools = FileTools::new(&config);

        // Create files with different extensions
        std::fs::write(temp_dir.path().join("test.rs"), "rust code").unwrap();
        std::fs::write(temp_dir.path().join("test.py"), "python code").unwrap();
        std::fs::write(temp_dir.path().join("main.rs"), "main code").unwrap();

        let input = FileOpsInput {
            operation: FileOperation::Find,
            path: temp_dir.path().to_string_lossy().to_string(),
            content: None,
            pattern: Some(r"\.rs$".to_string()),
            max_depth: 10,
            include_hidden: false,
        };

        let output = tools.handle(input);
        assert!(output.success);
        assert_eq!(output.count, Some(2));

        let files = output.files.unwrap();
        assert!(files.iter().any(|f| f.name == "test.rs"));
        assert!(files.iter().any(|f| f.name == "main.rs"));
    }

    #[test]
    fn test_path_outside_root() {
        let temp_dir = TempDir::new().unwrap();
        let config = create_test_config(&temp_dir);
        let tools = FileTools::new(&config);

        let input = FileOpsInput {
            operation: FileOperation::Read,
            path: "/etc/passwd".to_string(),
            content: None,
            pattern: None,
            max_depth: 10,
            include_hidden: false,
        };

        let output = tools.handle(input);
        assert!(!output.success);
        assert!(output.error.is_some());
    }

    // ── SEC-01 containment core (enforce_path_within_roots) ────────────────
    // These prove the invariant that `touring_file_ops` relies on: the MCP guard
    // and the CLI guard share this exact logic, so a regression here fails the build.

    #[test]
    fn enforce_allows_existing_path_inside_root() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().canonicalize().unwrap();
        let file = root.join("inside.txt");
        std::fs::write(&file, b"x").unwrap();
        let got = enforce_path_within_roots(&file, std::slice::from_ref(&root), true);
        assert!(got.is_ok(), "in-root existing file must be allowed");
        assert!(got.unwrap().starts_with(&root));
    }

    #[test]
    fn enforce_denies_path_outside_root() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().canonicalize().unwrap();
        // The canonical attack: an absolute path well outside the jail.
        let got = enforce_path_within_roots(Path::new("/etc/passwd"), &[root], true);
        assert!(got.is_err(), "/etc/passwd must be denied — this is SEC-01");
    }

    #[test]
    fn enforce_allows_nonexistent_leaf_in_root_for_write() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().canonicalize().unwrap();
        let new_file = root.join("does_not_exist_yet.txt");
        // must_exist = false (write/mkdir): validates the in-root parent.
        let got = enforce_path_within_roots(&new_file, std::slice::from_ref(&root), false);
        assert!(
            got.is_ok(),
            "a not-yet-existing leaf inside the root must be writable"
        );
        assert!(got.unwrap().starts_with(&root));
    }

    #[test]
    fn enforce_denies_missing_target_when_must_exist() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().canonicalize().unwrap();
        let missing = root.join("missing.txt");
        let got = enforce_path_within_roots(&missing, &[root], true);
        assert!(
            got.is_err(),
            "read/delete of a missing target must be rejected"
        );
    }

    #[test]
    fn enforce_defeats_dotdot_traversal_escape() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().join("project");
        std::fs::create_dir_all(&root).unwrap();
        let root = root.canonicalize().unwrap();
        // Escape attempt: project/../<sibling> resolves outside the jail.
        let escape = root.join("..").join("escape.txt");
        std::fs::write(tmp.path().join("escape.txt"), b"x").unwrap();
        let got = enforce_path_within_roots(&escape, &[root], true);
        assert!(got.is_err(), "canonicalization must defeat ../ traversal");
    }

    // ── Workspace-wide helpers (free functions, not bound to project_root) ──
    //
    // These wire `glob_to_regex` + `find_workspace_files` + `find_ws_recursive`
    // into the test surface so the helpers are exercised in CI and remain
    // available for ad-hoc tooling (e.g. `touring viz` queries that need to
    // discover candidate files outside the validated project root).

    #[test]
    fn glob_to_regex_translates_star() {
        let re = glob_to_regex("*.rs");
        assert!(
            re.contains("[^/]*"),
            "single-star should map to [^/]*: {re}"
        );
        assert!(
            re.starts_with("(?i)"),
            "expected case-insensitive flag: {re}"
        );
        assert!(re.contains("\\."), "literal dot must be escaped: {re}");
    }

    #[test]
    fn glob_to_regex_translates_double_star() {
        let re = glob_to_regex("**/main.rs");
        assert!(re.contains(".*"), "double-star should map to .*: {re}");
        assert!(re.contains("main"), "literal segment lost: {re}");
    }

    #[test]
    fn glob_to_regex_escapes_special_chars() {
        let re = glob_to_regex("foo[bar].rs");
        // brackets, dots and similar metacharacters must be escaped to avoid
        // accidental regex semantics.
        assert!(re.contains("\\."), "dot escape missing");
    }

    #[test]
    fn find_workspace_files_discovers_glob_matches() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();
        std::fs::write(root.join("alpha.rs"), "fn a() {}").unwrap();
        std::fs::write(root.join("beta.py"), "def b(): pass").unwrap();
        std::fs::create_dir(root.join("sub")).unwrap();
        std::fs::write(root.join("sub").join("gamma.rs"), "fn c() {}").unwrap();

        let matches = find_workspace_files(root, "*.rs", false, None, 4, false);
        // Should match alpha.rs at root and gamma.rs in sub/ via recursion.
        assert!(
            matches.iter().any(|p| p.ends_with("alpha.rs")),
            "missing alpha.rs in {matches:?}"
        );
        assert!(
            matches.iter().any(|p| p.ends_with("gamma.rs")),
            "missing gamma.rs in {matches:?}"
        );
        assert!(
            !matches.iter().any(|p| p.ends_with("beta.py")),
            "beta.py should NOT match *.rs"
        );
    }

    #[test]
    fn find_workspace_files_with_content_filter() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();
        std::fs::write(root.join("hit.rs"), "fn target() {}").unwrap();
        std::fs::write(root.join("miss.rs"), "fn other() {}").unwrap();

        let matches = find_workspace_files(root, "*.rs", false, Some("target"), 4, false);
        assert_eq!(matches.len(), 1, "content filter should keep only hit.rs");
        assert!(matches[0].ends_with("hit.rs"));
    }

    #[test]
    fn find_workspace_files_regex_mode() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();
        std::fs::write(root.join("file1.txt"), "x").unwrap();
        std::fs::write(root.join("FILE2.TXT"), "y").unwrap();

        // use_regex=true with case-insensitive flag should match both
        let matches = find_workspace_files(root, r"file\d\.txt", true, None, 4, false);
        assert_eq!(
            matches.len(),
            2,
            "regex should match both casings: {matches:?}"
        );
    }

    #[test]
    fn find_workspace_files_skips_blocklisted_dirs() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();
        std::fs::create_dir(root.join("target")).unwrap();
        std::fs::write(root.join("target").join("artifact.rs"), "noise").unwrap();
        std::fs::write(root.join("keep.rs"), "kept").unwrap();

        let matches = find_workspace_files(root, "*.rs", false, None, 4, false);
        assert_eq!(matches.len(), 1, "target/ must be skipped: {matches:?}");
        assert!(matches[0].ends_with("keep.rs"));
    }

    #[test]
    fn find_ws_recursive_respects_max_depth() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();
        std::fs::create_dir_all(root.join("a").join("b").join("c")).unwrap();
        std::fs::write(root.join("a").join("b").join("c").join("deep.rs"), "x").unwrap();

        // max_depth=1 should NOT reach c/deep.rs
        let mut results = Vec::new();
        let pattern = Regex::new(r"(?i)\.rs$").unwrap();
        find_ws_recursive(root, &pattern, None, 1, 0, false, &mut results);
        assert!(
            results.iter().all(|p| !p.contains("deep.rs")),
            "depth=1 should not descend that far: {results:?}"
        );
    }
}
