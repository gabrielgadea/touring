//! Bash command-shape extractor + the legacy entry points of the destructive-command policy.
//!
//! Two concerns used to share this module:
//!
//! 1. "Is this command destructive?" — answered by [`crate::command_policy`], the one
//!    owner of that question. [`validate_command`], [`Verdict`] and the route constants
//!    are kept here as thin re-exports so every historical path (the CEG static stage,
//!    the e2e suites, `touring_hooks::shared::bash_ast_validator`) keeps resolving; they
//!    answer the STRUCTURAL reading only, exactly as before.
//!
//! 2. [`command_shape`] extracts a stable cluster key for the failure-recall cache:
//!    `"cargo test --release"` and `"cargo --quiet test -j 4 --release"` both reduce to
//!    `"cargo test"`. Tokenizer-based for robustness against partial / malformed bash.
//!
//! Fail-open contract: any error / ambiguity produces [`Verdict::Allow`] — pre-bash MUST
//! never block on validator infrastructure failure.

pub use crate::command_policy::{
    DD_ROUTE, DELETE_ROUTE, DELETE_ROUTE_EXAMPLES, PIPE_SHELL_ROUTE, Verdict, route_examples,
};

/// Validate a bash command STRUCTURALLY: token signatures over the quote- and
/// comment-stripped line (`echo "rm -rf"` and `# rm -rf` do not fire). Allows on empty
/// input, on a clean stream, and when a signature's intent word is also a word of the
/// command.
///
/// This is the structural reading alone ([`crate::command_policy::judge_structural`]);
/// what `pre-bash` acts on is [`crate::command_policy::judge`].
pub fn validate_command(command: &str) -> Verdict {
    crate::command_policy::judge_structural(command)
}

/// Extract a stable cluster key from a shell command. Tokenizer-based for
/// robustness against partial / malformed bash; AST-based shape extraction
/// would fail on the very inputs (typoed commands, mid-edit fragments) where
/// failure-recall is most valuable.
///
/// Rules:
/// - Skip leading env-var assignments (`FOO=1 cargo test` → starts at "cargo").
/// - Skip leading flags (`--quiet cargo test` → "cargo test"; rare but real).
/// - Take the head (first non-flag word) and the first non-flag arg, if any.
/// - Returns `None` only for entirely-empty input.
///
/// Examples:
/// `cargo test --release` → `Some("cargo test")`
/// `cargo --quiet test -j 4` → `Some("cargo test")`
/// `git push origin main` → `Some("git push")`
/// `npm run build` → `Some("npm run")`
/// `ls` → `Some("ls")`
pub fn command_shape(command: &str) -> Option<String> {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return None;
    }

    let mut tokens = trimmed.split_whitespace();
    let mut head: Option<&str> = None;
    let mut subcommand: Option<&str> = None;

    #[allow(clippy::while_let_on_iterator)]
    while let Some(tok) = tokens.next() {
        // Drop env-var assignments (FOO=bar) at the start.
        if head.is_none() && is_env_assignment(tok) {
            continue;
        }
        // Drop leading flags before the head.
        if head.is_none() && tok.starts_with('-') {
            continue;
        }
        if head.is_none() {
            head = Some(tok);
            continue;
        }
        // After the head, skip flags and their values until we find the first
        // non-flag, non-numeric token — that becomes the subcommand.
        // (Numeric tokens are almost always flag values like `-j 4`; real
        // subcommands are alphabetic.)
        if tok.starts_with('-') {
            continue;
        }
        if tok.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        // Stop on shell separators — anything past a `;`, `&&`, `||` belongs
        // to a different command and shouldn't pollute the shape.
        if matches!(tok, ";" | "&&" | "||" | "|") {
            break;
        }
        subcommand = Some(tok);
        break;
    }

    let head = head?;
    let raw = match subcommand {
        Some(sub) => format!("{head} {sub}"),
        None => head.to_string(),
    };
    Some(strip_trailing_separators(&raw))
}

/// `true` iff `tok` looks like an env-var assignment (`KEY=value`).
fn is_env_assignment(tok: &str) -> bool {
    let bytes = tok.as_bytes();
    if bytes.is_empty() {
        return false;
    }
    let first = bytes[0];
    if !(first.is_ascii_uppercase() || first == b'_') {
        return false;
    }
    let mut saw_eq = false;
    for &b in &bytes[1..] {
        if b == b'=' {
            saw_eq = true;
            break;
        }
        if !(b.is_ascii_alphanumeric() || b == b'_') {
            return false;
        }
    }
    saw_eq
}

/// Strip trailing `;`, `&&`, `||` and similar shell separators that may have
/// stuck to the last token (e.g. `cargo test;` → `cargo test`).
fn strip_trailing_separators(s: &str) -> String {
    s.trim_end_matches([';', '&', '|']).trim_end().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─── the legacy entry point is the structural reading ─────────────────

    #[test]
    fn validate_command_is_the_structural_reading() {
        for cmd in [
            "rm -rf /",
            "find /tmp -name '*.bak' -delete",
            "git push --force origin main",
            "git push --force-with-lease origin main",
            "chmod -R 777 /var/www",
            "rm -rf /tmp --dry-run",
            "echo \"rm -rf is dangerous\"",
            "ls -la",
            "",
        ] {
            assert_eq!(
                validate_command(cmd),
                crate::command_policy::judge_structural(cmd),
                "{cmd:?}"
            );
        }
        assert!(validate_command("rm -rf /").is_block());
        assert!(matches!(
            validate_command("git reset --hard HEAD~1"),
            Verdict::Warn { .. }
        ));
    }

    #[test]
    fn the_legacy_paths_re_export_the_policy_constants() {
        assert_eq!(DELETE_ROUTE, crate::command_policy::DELETE_ROUTE);
        assert_eq!(DD_ROUTE, crate::command_policy::DD_ROUTE);
        assert_eq!(PIPE_SHELL_ROUTE, crate::command_policy::PIPE_SHELL_ROUTE);
        assert!(route_examples().count() > 0);
        assert!(!DELETE_ROUTE_EXAMPLES.is_empty());
    }

    // ─── command_shape ────────────────────────────────────────────────────

    #[test]
    fn shape_strips_release_flag() {
        assert_eq!(
            command_shape("cargo test --release").as_deref(),
            Some("cargo test")
        );
    }

    #[test]
    fn shape_handles_global_flag_before_subcommand() {
        assert_eq!(
            command_shape("cargo --quiet test --no-run").as_deref(),
            Some("cargo test")
        );
    }

    #[test]
    fn shape_extracts_git_subcommand() {
        assert_eq!(
            command_shape("git push origin main --force-with-lease").as_deref(),
            Some("git push")
        );
    }

    #[test]
    fn shape_handles_npm() {
        assert_eq!(
            command_shape("npm run build -- --watch").as_deref(),
            Some("npm run")
        );
    }

    #[test]
    fn shape_returns_just_head_when_no_subcommand() {
        assert_eq!(command_shape("ls").as_deref(), Some("ls"));
        assert_eq!(command_shape("ls -la").as_deref(), Some("ls"));
    }

    #[test]
    fn shape_skips_env_assignments() {
        assert_eq!(
            command_shape("FOO=1 BAR=baz cargo test").as_deref(),
            Some("cargo test")
        );
    }

    #[test]
    fn shape_handles_jflag_with_value() {
        // `-j` is a flag, `4` is its value — neither becomes the subcommand.
        // The next non-flag token (`--release`) is also a flag, so we end up
        // with just the head — that's correct because there really is no
        // subcommand for this invocation.
        assert_eq!(
            command_shape("cargo -j 4 --release").as_deref(),
            Some("cargo")
        );
    }

    #[test]
    fn shape_stops_at_shell_separator() {
        assert_eq!(
            command_shape("cargo test ; echo done").as_deref(),
            Some("cargo test"),
            "shape must not cross ;"
        );
    }

    #[test]
    fn shape_returns_none_for_empty() {
        assert!(command_shape("").is_none());
        assert!(command_shape("   ").is_none());
    }

    #[test]
    fn shape_strips_trailing_separator_in_token() {
        assert_eq!(command_shape("cargo test;").as_deref(), Some("cargo test"));
    }

    // ─── is_env_assignment ────────────────────────────────────────────────

    #[test]
    fn env_assignment_detection() {
        assert!(is_env_assignment("FOO=1"));
        assert!(is_env_assignment("PATH=/bin"));
        assert!(is_env_assignment("MY_VAR=value"));
        assert!(is_env_assignment("_PRIVATE=1"));
        assert!(
            !is_env_assignment("foo=1"),
            "lowercase first char rules out env-assignment"
        );
        assert!(!is_env_assignment("--flag"));
        assert!(!is_env_assignment("FOO"));
        assert!(!is_env_assignment(""));
    }
}
