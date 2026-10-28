//! The git files whose change must re-run `build.rs` (shared by `build.rs` and
//! `tests/build_info_freshness.rs`, so the test exercises the same resolution the build uses).
//!
//! In a plain checkout `.git` is a directory. In a `git worktree` it is a FILE holding
//! `gitdir: <dir>`: the worktree's HEAD lives in `<dir>`, and the branch refs in the directory its
//! `commondir` file names. Measured 28/10/2026 in the `touring-wt-simhash` worktree: the old
//! literal `../../.git/HEAD` named nothing there, and cargo treats a missing `rerun-if-changed`
//! path as "nothing changed" — the stale-SHA bug, silently back.

use std::path::{Path, PathBuf};

/// `(head_dir, common_dir)` for the checkout whose `.git` entry is `dot_git`.
pub fn git_dirs(dot_git: &Path) -> (PathBuf, PathBuf) {
    let Ok(texto) = std::fs::read_to_string(dot_git) else {
        // a directory (plain checkout) or nothing at all: both answers are `dot_git` itself
        return (dot_git.to_path_buf(), dot_git.to_path_buf());
    };
    let Some(alvo) = texto.trim().strip_prefix("gitdir:").map(str::trim) else {
        return (dot_git.to_path_buf(), dot_git.to_path_buf());
    };
    let base = dot_git.parent().unwrap_or(Path::new("."));
    let head_dir = base.join(alvo);
    let common_dir = std::fs::read_to_string(head_dir.join("commondir"))
        .map(|c| head_dir.join(c.trim()))
        .unwrap_or_else(|_| head_dir.clone());
    (head_dir, common_dir)
}

/// The files to declare: HEAD, the `main` ref (the propagation flow targets it) and the ref
/// HEAD currently points at (a commit on a feature branch moves that ref, not HEAD's content).
pub fn git_inputs(dot_git: &Path) -> Vec<PathBuf> {
    let (head_dir, common_dir) = git_dirs(dot_git);
    let head = head_dir.join("HEAD");
    let mut inputs = vec![head.clone(), common_dir.join("refs/heads/main")];
    if let Some(r) = std::fs::read_to_string(&head)
        .ok()
        .and_then(|t| t.trim().strip_prefix("ref:").map(|r| r.trim().to_string()))
    {
        let atual = common_dir.join(r);
        if !inputs.contains(&atual) {
            inputs.push(atual);
        }
    }
    inputs
}
