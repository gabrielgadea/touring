//! The user's home directory and the `~/.claude` config root — one resolver.
//!
//! Production code that needs a path under the user's home asks here instead of
//! spelling `$HOME` (or, worse, a compiled-in `/home/<name>`) at the call site.
//! The answer is an [`Option`] on purpose: when no home directory can be derived
//! (a stripped container, `HOME` unset and no passwd entry) there is NO default
//! — a developer's home directory is not one — and the caller must degrade with
//! a named reason or fail loud, never point the process at a guessed path.
//!
//! Resolution is [`home::home_dir`] (the crate cargo itself uses): `$HOME` when
//! set and non-empty, otherwise the passwd entry of the current uid. A result
//! that is not an absolute path is refused, so a relative `HOME` can never turn
//! `~/.claude/...` into something relative to whatever the working directory is.

use std::path::{Path, PathBuf};

/// Directory name of the Claude Code config root under the home directory.
const CLAUDE_DIR: &str = ".claude";

/// The current user's home directory, or `None` when it cannot be derived.
///
/// See the [module docs](self) for why there is no fallback value.
#[must_use]
pub fn home_dir() -> Option<PathBuf> {
    keep_absolute(home::home_dir())
}

/// `~/.claude` — the Claude Code config root — or `None` without a home directory.
///
/// Agents, hooks, rules and the global `touring` state live under it. Join the
/// subdirectory you need (`claude_home()?.join("agents")`) rather than
/// re-deriving `$HOME` at the call site.
#[must_use]
pub fn claude_home() -> Option<PathBuf> {
    claude_home_in(home_dir().as_deref())
}

/// [`claude_home`] for an explicit home — the pure core, testable without
/// touching the process environment.
fn claude_home_in(home: Option<&Path>) -> Option<PathBuf> {
    home.map(|h| h.join(CLAUDE_DIR))
}

/// Keep `path` only when it is absolute and non-empty.
fn keep_absolute(path: Option<PathBuf>) -> Option<PathBuf> {
    path.filter(|p| p.is_absolute())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_home_is_the_dot_claude_directory_of_the_home() {
        assert_eq!(
            claude_home_in(Some(Path::new("/home/u"))),
            Some(PathBuf::from("/home/u/.claude"))
        );
    }

    #[test]
    fn no_home_means_no_claude_home_not_a_guess() {
        assert_eq!(claude_home_in(None), None);
    }

    #[test]
    fn a_relative_or_empty_home_is_refused() {
        assert_eq!(keep_absolute(Some(PathBuf::from("relative/home"))), None);
        assert_eq!(keep_absolute(Some(PathBuf::new())), None);
        assert_eq!(keep_absolute(None), None);
        assert_eq!(
            keep_absolute(Some(PathBuf::from("/home/u"))),
            Some(PathBuf::from("/home/u"))
        );
    }

    #[test]
    fn the_live_resolvers_agree_with_each_other() {
        // `HOME` is process-global on a parallel runner, so this never mutates it:
        // whatever the environment holds, the two resolvers must stay consistent.
        match home_dir() {
            Some(h) => {
                assert!(h.is_absolute());
                assert_eq!(claude_home(), Some(h.join(".claude")));
            }
            None => assert_eq!(claude_home(), None),
        }
    }
}
