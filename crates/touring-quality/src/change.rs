//! `Change` and `ProposedFile` — the input to every gate.
//!
//! Canonical home (migrated 2026-06-25 from touring-harness/src/change.rs
//! per `2026-06-25-harness-consolidation-master-plan-v3.md` W1).
//! `touring-harness::change::*` is now a re-export shim for back-compat.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// What kind of change is being proposed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FileKind {
    /// Brand-new file (did not exist before).
    Create,
    /// File exists; contents are being modified.
    Modify,
    /// File is being deleted.
    Delete,
    /// File is being renamed (path change).
    Rename,
}

impl std::fmt::Display for FileKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Create => "create",
            Self::Modify => "modify",
            Self::Delete => "delete",
            Self::Rename => "rename",
        })
    }
}

/// A single file in a proposed change.
///
/// `before` is `None` for `FileKind::Create`; `after` is `None` for
/// `FileKind::Delete`. Both are `Some` for `Modify` and `Rename`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposedFile {
    /// Workspace-relative path.
    pub path: PathBuf,
    /// Old contents (`None` for Create).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub before: Option<String>,
    /// New contents (`None` for Delete).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after: Option<String>,
    /// Kind of change.
    pub kind: FileKind,
}

impl ProposedFile {
    /// Convenience constructor for a Modify.
    #[must_use]
    pub fn modify(
        path: impl Into<PathBuf>,
        before: impl Into<String>,
        after: impl Into<String>,
    ) -> Self {
        Self {
            path: path.into(),
            before: Some(before.into()),
            after: Some(after.into()),
            kind: FileKind::Modify,
        }
    }

    /// Convenience constructor for a Create.
    #[must_use]
    pub fn create(path: impl Into<PathBuf>, after: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            before: None,
            after: Some(after.into()),
            kind: FileKind::Create,
        }
    }

    /// Convenience constructor for a Delete.
    #[must_use]
    pub fn delete(path: impl Into<PathBuf>, before: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            before: Some(before.into()),
            after: None,
            kind: FileKind::Delete,
        }
    }

    /// Compute added lines (after - before).
    #[must_use]
    pub fn added_lines(&self) -> u64 {
        match (&self.before, &self.after) {
            (_, Some(after)) => after.lines().count() as u64,
            (Some(_before), None) => 0,
            (None, None) => 0,
        }
    }

    /// Compute removed lines (before - after).
    #[must_use]
    pub fn removed_lines(&self) -> u64 {
        match (&self.before, &self.after) {
            (Some(before), _) => before.lines().count() as u64,
            (None, Some(_) | None) => 0,
        }
    }
}

/// A proposed change, with one or more files and metadata about the agent
/// proposing it.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Change {
    /// Files in this change.
    pub files: Vec<ProposedFile>,
    /// Optional commit message (for `CiCdDevops` gate).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit_message: Option<String>,
    /// Identifier of the agent / LLM proposing the change.
    /// Used for audit-trail and drift detection.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    /// Model name (e.g. `claude-opus-4-7`, `gpt-5`, `gemini-2.5-pro`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Workspace root (defaults to current dir if None).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_root: Option<PathBuf>,
}

impl Change {
    /// Create an empty change.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a file (builder pattern).
    #[must_use]
    pub fn with_file(mut self, file: ProposedFile) -> Self {
        self.files.push(file);
        self
    }

    /// Add multiple files (builder pattern).
    #[must_use]
    pub fn with_files(mut self, files: impl IntoIterator<Item = ProposedFile>) -> Self {
        self.files.extend(files);
        self
    }

    /// Set the agent ID (builder pattern).
    #[must_use]
    pub fn agent(mut self, agent_id: impl Into<String>) -> Self {
        self.agent_id = Some(agent_id.into());
        self
    }

    /// Set the model (builder pattern).
    #[must_use]
    pub fn model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    /// Set the commit message (builder pattern).
    #[must_use]
    pub fn commit_message(mut self, msg: impl Into<String>) -> Self {
        self.commit_message = Some(msg.into());
        self
    }

    /// Total LOC added across all files.
    #[must_use]
    pub fn total_added(&self) -> u64 {
        self.files.iter().map(ProposedFile::added_lines).sum()
    }

    /// Total LOC removed across all files.
    #[must_use]
    pub fn total_removed(&self) -> u64 {
        self.files.iter().map(ProposedFile::removed_lines).sum()
    }

    /// Resolve the workspace root (defaults to current dir).
    #[must_use]
    pub fn workspace_root(&self) -> PathBuf {
        self.workspace_root
            .clone()
            .unwrap_or_else(|| PathBuf::from("."))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builder() {
        let c = Change::new()
            .with_file(ProposedFile::create("src/lib.rs", "// new"))
            .with_file(ProposedFile::modify("README.md", "old", "new"))
            .agent("claude-opus-4-7")
            .model("claude-opus-4-7")
            .commit_message("feat: new file");
        assert_eq!(c.files.len(), 2);
        assert_eq!(c.agent_id.as_deref(), Some("claude-opus-4-7"));
        assert_eq!(c.model.as_deref(), Some("claude-opus-4-7"));
        assert_eq!(c.commit_message.as_deref(), Some("feat: new file"));
    }

    #[test]
    fn loc_counting() {
        let c = Change::new()
            .with_file(ProposedFile::create("a.rs", "x\ny\nz"))
            .with_file(ProposedFile::modify("b.rs", "a\nb", "c\nd\ne"));
        assert_eq!(c.total_added(), 3 + 3); // 3 new + 3 in modified
        assert_eq!(c.total_removed(), 2); // 0 in create + 2 removed in modify
    }

    #[test]
    fn file_kind_display() {
        assert_eq!(FileKind::Create.to_string(), "create");
        assert_eq!(FileKind::Modify.to_string(), "modify");
        assert_eq!(FileKind::Delete.to_string(), "delete");
        assert_eq!(FileKind::Rename.to_string(), "rename");
    }
}
