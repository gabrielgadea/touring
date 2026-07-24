//! `projects` — Multi-project registry for touring-server.
//!
//! Allows registering multiple touring workspaces under human-friendly aliases,
//! switching between them, and persisting the registry to `~/.claude/touring/projects.json`.

pub mod project;
pub mod registry;

pub use project::ProjectEntry;
pub use registry::ProjectRegistry;
