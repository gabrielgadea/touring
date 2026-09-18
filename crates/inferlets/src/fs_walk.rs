//! Directory walk shared by the inferlets that scan a workspace.
//!
//! One copy of the rule, because three inferlets carried the same loop and the
//! same defect: `Path::is_dir` follows symlinks, so a link pointing at an
//! ancestor made the walk branch at every level until the path outgrew the OS
//! limit. Two such links left in `/tmp` by a killed test fixture cost two test
//! threads 25 minutes of CPU each and hung the convergence judge (18/09/2026).
//! A directory reached through a symlink is never entered; a symlink to a file
//! is still a file.

use std::path::Path;

/// Directory names never entered: build output and VCS metadata.
const SKIPPED_DIRS: [&str; 2] = ["target", ".git"];

/// Call `on_file` for every file under `root`, depth first.
///
/// Unreadable directories and entries are skipped. A directory reached through
/// a symlink is not entered, so the walk ends on any tree, cyclic links
/// included.
pub(crate) fn for_each_file(root: &Path, on_file: &mut dyn FnMut(&Path)) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        let path = entry.path();
        if kind.is_dir() {
            let skipped = path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| SKIPPED_DIRS.contains(&n));
            if !skipped {
                for_each_file(&path, on_file);
            }
        } else if kind.is_file() || (kind.is_symlink() && path.is_file()) {
            on_file(&path);
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    fn files_under(root: &Path) -> Vec<String> {
        let mut seen = Vec::new();
        for_each_file(root, &mut |p| {
            seen.push(
                p.strip_prefix(root)
                    .expect("walk stays under its root")
                    .to_string_lossy()
                    .into_owned(),
            );
        });
        seen.sort();
        seen
    }

    /// The fixture left in `/tmp` on 18/09/2026: two links back to an ancestor.
    /// Following them made the walk exponential in the path-length limit.
    #[test]
    fn a_directory_symlink_cycle_is_never_entered() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        std::fs::create_dir_all(root.join("src/cyc")).expect("mkdir");
        std::fs::write(root.join("src/a.rs"), "fn a() {}").expect("write");
        symlink(root.join("src"), root.join("src/cyc/loop")).expect("symlink");
        symlink(root, root.join("src/cyc/alias")).expect("symlink");

        assert_eq!(files_under(root), ["src/a.rs"]);
    }

    #[test]
    fn build_and_vcs_directories_are_skipped_and_a_file_symlink_is_a_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        for d in ["target/debug", ".git", "src"] {
            std::fs::create_dir_all(root.join(d)).expect("mkdir");
        }
        std::fs::write(root.join("target/debug/gen.rs"), "").expect("write");
        std::fs::write(root.join(".git/HEAD"), "").expect("write");
        std::fs::write(root.join("src/lib.rs"), "").expect("write");
        symlink(root.join("src/lib.rs"), root.join("alias.rs")).expect("symlink");

        assert_eq!(files_under(root), ["alias.rs", "src/lib.rs"]);
    }

    #[test]
    fn a_missing_root_yields_nothing() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert!(files_under(&dir.path().join("absent")).is_empty());
    }
}
