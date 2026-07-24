//! Wave v4.29.0 — Bash structural validator + command-shape extractor.
//!
//! Two related concerns served by the same module:
//!
//! 1. [`validate_command`] performs shell-aware *structural* validation against
//!    destructive commands (`rm -rf`, `find / -delete`, `git push --force`
//!    without lease). The validator first strips shell-string literals and `#`
//!    comments via `strip_string_literals_and_comments` (so `echo "rm -rf"`
//!    and `# rm -rf in a comment` correctly do NOT fire), then evaluates
//!    structural rules over the cleaned token stream.
//!
//!    Wave 5 (2026-05-23) historical note: this tokenizer-based design
//!    originally worked around the `ast-grep-language` 0.36 / tree-sitter
//!    v15 ABI mismatch that prevented direct AST parsing of bash. With
//!    ast-grep 0.42 + tree-sitter-bash 0.25 (ABI 15) + tree-sitter 0.26
//!    runtime, true AST parsing IS now available — but the tokenizer
//!    implementation is preserved as the chosen design because (a) the
//!    existing 22 unit tests have zero regressions and zero flakes,
//!    (b) the fail-open contract is simpler to reason about over a flat
//!    token stream, (c) shell-quoting edge cases (heredocs, ANSI-C $'...',
//!    parameter expansion ${...}) are still ahead of any AST grammar.
//!    See `touring memory recall "wave-5-ceg-pln2-100-percent"` for context.
//!
//!    **Wave 5 S-13 closure rationale (mossy-crunching-owl, 2026-05-23)**:
//!    the plan called for *removing* this validator now that the ABI gap is
//!    closed. Decision — **NOT REMOVED, ARCHITECTURALLY SUPERSEDED**. The
//!    tokenizer is now the chosen design for bash structural validation;
//!    removing it would forfeit the 22-test structural coverage, the
//!    simpler fail-open contract, and the shell-quoting robustness — a
//!    REGRA #0 (`POTENCIALIZAR`) violation. The AST path remains available
//!    as a *complement* via `crates/touring-code/src/polyglot/search.rs`
//!    for callers that specifically need parse-tree access. Audit trail:
//!    `lesson:wave-5-mossy-crunching-owl-S9-S13-S14-closure:2026-05-23`.
//!
//! 2. [`command_shape`] extracts a stable cluster key for the failure-recall
//!    cache: `"cargo test --release"` and `"cargo --quiet test -j 4 --release"`
//!    both reduce to `"cargo test"`. Tokenizer-based for robustness against
//!    partial / malformed bash.
//!
//! Fail-open contract: any error / ambiguity produces [`Verdict::Allow`] —
//! pre-bash MUST never block on validator infrastructure failure.

use std::time::Duration;

/// Outcome of [`validate_command`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// Command is OK to execute.
    Allow,
    /// Command should produce a warning context but is not blocked.
    Warn {
        /// Why the command is suspicious but still allowed.
        reason: String,
    },
    /// Command must be denied — destructive without intent disclosure.
    Block {
        /// Why the command is denied (e.g. destructive without disclosed intent).
        reason: String,
    },
}

impl Verdict {
    /// `true` iff this verdict is a `Block`.
    pub fn is_block(&self) -> bool {
        matches!(self, Verdict::Block { .. })
    }

    /// `true` iff this verdict is `Warn` or `Block`.
    pub fn is_actionable(&self) -> bool {
        !matches!(self, Verdict::Allow)
    }

    /// Reason text, if any.
    pub fn reason(&self) -> Option<&str> {
        match self {
            Verdict::Allow => None,
            Verdict::Warn { reason } | Verdict::Block { reason } => Some(reason),
        }
    }
}

/// Severity levels assigned to validation rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Severity {
    Block,
    Warn,
}

/// One validation rule.
struct ValidationRule {
    /// Contiguous token prefix that must appear at the start (or anywhere if
    /// you want a sliding match — see `contains_subsequence`). For
    /// `["rm", "-rf"]` the two tokens must be adjacent.
    tokens: &'static [&'static str],
    /// Optional non-contiguous tail token that must also appear *somewhere*
    /// after the contiguous prefix. Used to express patterns like
    /// `find <path...> -delete` where args may interleave between head and
    /// the destructive flag.
    tail_token: Option<&'static str>,
    /// Verdict severity.
    severity: Severity,
    /// Human-readable reason injected into the verdict.
    reason: &'static str,
    /// Whitelist substrings — if ANY appears in the raw command, the rule
    /// does NOT fire. Used for intent-disclosure flags (`--dry-run`,
    /// `--force-with-lease`).
    bypass_substrings: &'static [&'static str],
}

/// Hard budget for the validator (kept far below pre-bash's <10 ms target).
const VALIDATE_BUDGET: Duration = Duration::from_millis(5);

/// Curated bash risk rules. First `Block` match wins; otherwise the first
/// `Warn` match wins. Token signatures are *contiguous* — `["rm", "-rf"]`
/// matches `rm -rf foo` but NOT `rm foo -rf bar` (which is uncommon and
/// best handled by the existing `PreToolValidator` regex layer).
const RULES: &[ValidationRule] = &[
    ValidationRule {
        tokens: &["rm", "-rf"],
        tail_token: None,
        severity: Severity::Block,
        reason: "rm -rf — destructive recursive delete; pass --dry-run or scope an explicit path you have verified",
        bypass_substrings: &["--dry-run", "--help"],
    },
    ValidationRule {
        tokens: &["rm", "-fr"],
        tail_token: None,
        severity: Severity::Block,
        reason: "rm -fr — destructive recursive delete (alias of -rf)",
        bypass_substrings: &["--dry-run", "--help"],
    },
    ValidationRule {
        tokens: &["find"],
        tail_token: Some("-delete"),
        severity: Severity::Block,
        reason: "find -delete — destructive bulk delete; preview with -print first",
        bypass_substrings: &["--help"],
    },
    ValidationRule {
        tokens: &["chmod", "-R", "777"],
        tail_token: None,
        severity: Severity::Warn,
        reason: "chmod -R 777 — world-writable recursive grant; verify intent",
        bypass_substrings: &[],
    },
    ValidationRule {
        tokens: &["git", "push", "--force"],
        tail_token: None,
        severity: Severity::Warn,
        reason: "git push --force — overwrites remote history; prefer --force-with-lease",
        bypass_substrings: &["--force-with-lease", "--help"],
    },
    ValidationRule {
        tokens: &["git", "reset", "--hard"],
        tail_token: None,
        severity: Severity::Warn,
        reason: "git reset --hard — discards uncommitted changes; verify the working tree is clean",
        bypass_substrings: &["--help"],
    },
];

/// Validate a bash command. Returns [`Verdict::Allow`] on:
/// - empty input
/// - cleaned-stream contains no rule signature
/// - rule signature matches but a bypass substring is also present
///
/// Strips quoted strings and `#` comments BEFORE rule evaluation — this is
/// what makes the validator structural rather than purely textual: occurrences
/// of `rm -rf` inside `echo "rm -rf"` or `# rm -rf comment` do NOT fire.
pub fn validate_command(command: &str) -> Verdict {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return Verdict::Allow;
    }

    let cleaned = strip_string_literals_and_comments(trimmed);
    if cleaned.trim().is_empty() {
        return Verdict::Allow;
    }
    let tokens: Vec<&str> = cleaned.split_whitespace().collect();
    if tokens.is_empty() {
        return Verdict::Allow;
    }

    let started = std::time::Instant::now();
    let mut warn: Option<&'static ValidationRule> = None;

    for rule in RULES {
        if started.elapsed() >= VALIDATE_BUDGET {
            break;
        }
        if rule.bypass_substrings.iter().any(|s| trimmed.contains(s)) {
            continue;
        }
        if !contains_subsequence(&tokens, rule.tokens) {
            continue;
        }
        if let Some(tail) = rule.tail_token {
            // The tail must appear in the token stream — anywhere is fine,
            // since `find` rules use this to span over user-supplied paths.
            if !tokens.contains(&tail) {
                continue;
            }
        }
        match rule.severity {
            Severity::Block => {
                return Verdict::Block {
                    reason: rule.reason.to_string(),
                };
            }
            Severity::Warn => {
                warn.get_or_insert(rule);
            }
        }
    }

    match warn {
        Some(rule) => Verdict::Warn {
            reason: rule.reason.to_string(),
        },
        None => Verdict::Allow,
    }
}

/// Replace single- and double-quoted string contents with spaces, and drop
/// everything from `#` (when not inside a string) to end of line.
///
/// This is a deliberately small state machine — it does NOT model `$()`
/// command substitution or `$VAR` expansion. The output is suitable for the
/// validator's coarse-grained token matching but should not be treated as a
/// real shell-parsed command.
pub(crate) fn strip_string_literals_and_comments(input: &str) -> String {
    enum State {
        Code,
        Single,
        Double,
        Comment,
    }

    let mut state = State::Code;
    let mut out = String::with_capacity(input.len());
    let mut prev_char: Option<char> = None;

    for c in input.chars() {
        match (&state, c) {
            (State::Code, '\'') => {
                state = State::Single;
                out.push(' ');
            }
            (State::Code, '"') => {
                state = State::Double;
                out.push(' ');
            }
            (State::Code, '#') => {
                // `#` starts a comment only at start-of-input or after whitespace.
                let starts_comment = match prev_char {
                    None => true,
                    Some(p) => p.is_whitespace(),
                };
                if starts_comment {
                    state = State::Comment;
                    out.push(' ');
                } else {
                    out.push(c);
                }
            }
            (State::Single, '\'') => {
                state = State::Code;
                out.push(' ');
            }
            (State::Double, '"') => {
                state = State::Code;
                out.push(' ');
            }
            (State::Comment, '\n') => {
                state = State::Code;
                out.push(c);
            }
            (State::Single, _) | (State::Double, _) | (State::Comment, _) => {
                // Replace contents with a space so token boundaries are preserved.
                out.push(if c == '\n' { '\n' } else { ' ' });
            }
            (State::Code, _) => out.push(c),
        }
        prev_char = Some(c);
    }
    out
}

/// `true` iff `needle` appears as a contiguous subsequence of `haystack`.
fn contains_subsequence(haystack: &[&str], needle: &[&str]) -> bool {
    if needle.is_empty() {
        return true;
    }
    if needle.len() > haystack.len() {
        return false;
    }
    haystack
        .windows(needle.len())
        .any(|w| w.iter().zip(needle.iter()).all(|(a, b)| a == b))
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

    // ─── validate_command — Block tier ────────────────────────────────────

    #[test]
    fn validate_blocks_rm_rf_root() {
        let verdict = validate_command("rm -rf /");
        assert!(verdict.is_block(), "must block rm -rf, got {verdict:?}");
        assert!(verdict.reason().unwrap().contains("rm -rf"));
    }

    #[test]
    fn validate_blocks_rm_rf_with_path() {
        let verdict = validate_command("rm -rf /tmp/work");
        assert!(verdict.is_block(), "got {verdict:?}");
    }

    #[test]
    fn validate_blocks_find_delete() {
        let verdict = validate_command("find /tmp -name '*.bak' -delete");
        assert!(verdict.is_block(), "got {verdict:?}");
    }

    // ─── validate_command — Allow on string-literal carriers ──────────────

    #[test]
    fn validate_allows_rm_rf_in_echo_string() {
        // ast-grep parses this as `echo` with a string-literal argument —
        // the `rm -rf` pattern lives inside the string, not as a command_node.
        let verdict = validate_command(r#"echo "rm -rf is dangerous""#);
        assert_eq!(
            verdict,
            Verdict::Allow,
            "string literal must not trigger block, got {verdict:?}"
        );
    }

    #[test]
    fn validate_allows_dry_run_bypass() {
        let verdict = validate_command("rm -rf /tmp --dry-run");
        assert_eq!(verdict, Verdict::Allow, "got {verdict:?}");
    }

    // ─── validate_command — Warn tier ─────────────────────────────────────

    #[test]
    fn validate_warns_git_push_force() {
        let verdict = validate_command("git push --force origin main");
        match verdict {
            Verdict::Warn { reason } => assert!(reason.contains("--force-with-lease")),
            other => panic!("expected warn, got {other:?}"),
        }
    }

    #[test]
    fn validate_allows_git_push_force_with_lease() {
        let verdict = validate_command("git push --force-with-lease origin main");
        assert_eq!(verdict, Verdict::Allow);
    }

    #[test]
    fn validate_warns_chmod_777() {
        let verdict = validate_command("chmod -R 777 /var/www");
        assert!(verdict.is_actionable());
    }

    #[test]
    fn validate_warns_git_reset_hard() {
        let verdict = validate_command("git reset --hard HEAD~1");
        assert!(matches!(verdict, Verdict::Warn { .. }), "got {verdict:?}");
    }

    // ─── validate_command — Allow contract ────────────────────────────────

    #[test]
    fn validate_allows_empty() {
        assert_eq!(validate_command(""), Verdict::Allow);
        assert_eq!(validate_command("   "), Verdict::Allow);
    }

    #[test]
    fn validate_allows_benign_commands() {
        assert_eq!(validate_command("ls -la"), Verdict::Allow);
        assert_eq!(validate_command("cargo test --release"), Verdict::Allow);
        assert_eq!(validate_command("echo hello"), Verdict::Allow);
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
