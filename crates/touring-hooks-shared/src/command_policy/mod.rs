//! `command_policy` — the ONE owner of "is this Bash command destructive: block
//! it, flag it, or let it through?"
//!
//! Until 02/11/2026 four owners answered it with different semantics: a token
//! signature validator (`bash_ast_validator`), a prefix + regex validator
//! (`PreToolValidator`, touring-hooks-core), a weighted keyword scorer
//! (`permission_request`) and a shell script (`block_git.sh`). The commit that
//! took git out of the second one records why: "two owners with divergent
//! semantics made the REGRA #11 v2 ritual unreachable" (5731f91b). This module
//! is where the Rust three now ask.
//!
//! # The interface
//!
//! | Question | Answer |
//! |---|---|
//! | what `pre-bash` acts on | [`judge`] |
//! | the structural reading alone (the CEG static stage) | [`judge_structural`] |
//! | the lexical reading alone | [`judge_lexical`] |
//! | one invocation as `(tool, params)` / JSON params | [`judge_tool`], [`judge_params`] |
//! | how risky a permission prompt's text reads | [`prompt_risk`] |
//!
//! # What this module owns
//!
//! * **The vocabulary** — one table of rows (`vocabulary::RULES`); each row is
//!   one destructive thing, read by up to three readings of a command.
//! * **The precedence** — the readings run in a fixed order, see [`judge`].
//! * **The per-command words** — what a command passes to say "I mean the safe
//!   variant" (`--dry-run`, `--force-with-lease`, a row's own `--help`), and the
//!   registry of the per-command env assignments a SHELL guard honours instead
//!   ([`SHELL_GUARD_TOKENS`]).
//! * **The fail-open mode** — below.
//!
//! # Precedence
//!
//! 1. **Structural** ([`judge_structural`]): token signatures over the quote- and
//!    comment-stripped line. A `Block` row ends the judgment; its reason is the
//!    verdict's.
//! 2. **Lexical** ([`judge_lexical`]): every simple command of the line by its
//!    words (fixed prefixes, then regexes, then per-tool schemas; pipelines as one
//!    text). A `Block` ends the judgment.
//! 3. A structural `Warn` (git `--force`, `reset --hard`, `chmod -R 777`)
//!    survives as the verdict only when nothing blocked: `pre-bash` leaves it to
//!    flow through (the shell guard owns git), the CEG static stage reports it.
//!
//! Intent words are read per reading and the grammars differ: the lexical
//! reading honours `--dry-run` and `--force-with-lease[=<ref>]` for the simple
//! command that carries them; a structural row honours its own `bypass_words`
//! (and `--word=<v>` for any of them). Kept as measured — see the tests.
//!
//! # Fail-open
//!
//! Any ambiguity is `Allow`: empty input, a line that is only quotes or
//! comments, and a structural budget ([`structural::BUDGET`], 5 ms) that lapses
//! before the rows are exhausted. The policy never blocks on its own
//! infrastructure failing. (The patterns are compiled once, and compiling them
//! is proven by every test that touches the table.)
//!
//! # What it does NOT own
//!
//! `block_git.sh` stays the single owner of git policy (S25, D2-A, 25/09/2026):
//! it honours `GIT_DESTRUCTIVE_OK=1` and `GIT_HOOKS_BYPASS_OK=1`, which no Rust
//! reading looks at. [`SHELL_GUARD_TOKENS`] lists them so the day the script
//! asks this module for the verdict (planned after the 08/11 freeze) they have a
//! home, and a test keeps the list and the script from drifting apart.

mod lexer;
mod lexical;
mod structural;
mod vocabulary;

#[cfg(test)]
mod tests;

use vocabulary::RULES;

pub use lexer::{command_of, shell_pipelines};

/// Outcome of a judgment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// Command is OK to execute.
    Allow,
    /// Command should produce a warning context but is not blocked.
    Warn {
        /// Why the command is suspicious but still allowed.
        reason: String,
    },
    /// Command must be denied.
    Block {
        /// Why the command is denied, followed by the road still open.
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

/// The deletions an agent can still run, stated once.
///
/// Every refusal of a delete carries this text. It names only what the policy
/// lets through (tests run each of [`DELETE_ROUTE_EXAMPLES`] through [`judge`])
/// and says plainly that a recursive delete belongs to the user: no flag unlocks
/// one. `rm` has no `--dry-run`, so passing it only makes `rm` exit with an error.
pub const DELETE_ROUTE: &str = "remove named files with `rm <file>` or empty directories with \
     `rmdir <dir>`; empty a file in place with `: > <file>` or remove a single entry with \
     `unlink <file>`; a recursive or forced delete is the user's to run (`! rm -r <path>`)";

/// Commands that follow [`DELETE_ROUTE`], proven open by tests.
pub const DELETE_ROUTE_EXAMPLES: &[&str] = &[
    "rm notes.txt",
    "rm a.log b.log",
    "rmdir build",
    ": > stale.log",
    "unlink stale.done",
];

/// V0e2 (09/10/2026) — the route a `dd` refusal names (measured by analise-6b:
/// 0% route-followed, because there was none). File-to-file copy is `cp`,
/// sizing is `truncate`; raw `dd` is the user's to run.
pub const DD_ROUTE: &str =
    "file-to-file copy is `cp`, sizing is `truncate` — raw `dd` is the user's to run";

/// V0e2 — the route a pipe-to-shell refusal names: download, read, THEN run.
pub const PIPE_SHELL_ROUTE: &str =
    "download, read, then run: `curl -O <url>` (or `wget -O`), read the file, `bash <file>`";

/// A refusal reason followed by what to do instead, when there is a route.
fn with_route(reason: impl Into<String>, route: Option<&str>) -> String {
    let reason = reason.into();
    match route {
        Some(route) => format!("{reason}; {route}"),
        None => reason,
    }
}

/// A per-command env assignment honoured by a SHELL guard, not by any Rust reading.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShellGuardToken {
    /// The assignment as it is written in front of a command.
    pub assignment: &'static str,
    /// What it waives.
    pub waives: &'static str,
    /// The guard that honours it.
    pub honoured_by: &'static str,
}

/// The per-command assignments `block_git.sh` honours (REGRA #11 v2). One counts
/// when it is CODE at any level of the command — never when it is data (a quoted
/// literal, a heredoc body). No Rust reading honours either: git left the Rust
/// policy at S25, and `pre-bash` neither reads nor strips them.
pub const SHELL_GUARD_TOKENS: &[ShellGuardToken] = &[
    ShellGuardToken {
        assignment: "GIT_DESTRUCTIVE_OK=1",
        waives: "forms that lose data: reset --hard, checkout --, clean, stash, rebase, push --force, branch -D…",
        honoured_by: "client/hooks/block_git.sh",
    },
    ShellGuardToken {
        assignment: "GIT_HOOKS_BYPASS_OK=1",
        waives: "forms that skip a repository's hooks: --no-verify, core.hooksPath, SKIP=, HUSKY=0",
        honoured_by: "client/hooks/block_git.sh",
    },
];

/// What `pre-bash` acts on: the structural reading, then the lexical one.
///
/// `Block` as soon as either blocks (structural first, so its reason wins on
/// overlap); otherwise the structural `Warn`, if there was one; otherwise `Allow`.
pub fn judge(command: &str) -> Verdict {
    let structural = structural::judge(command);
    if structural.is_block() {
        return structural;
    }
    let lexical = lexical::judge_line(command);
    if lexical.is_block() {
        return lexical;
    }
    structural
}

/// The structural reading alone: token signatures over the quote- and
/// comment-stripped line. `echo "rm -rf"` and `# rm -rf` do not fire.
pub fn judge_structural(command: &str) -> Verdict {
    structural::judge(command)
}

/// The lexical reading alone: each simple command of the line by its words, each
/// pipeline as one text. Never `Warn`.
pub fn judge_lexical(command: &str) -> Verdict {
    lexical::judge_line(command)
}

/// One invocation as a command name and its parameter text (the lexical reading).
pub fn judge_tool(tool_name: &str, params: &str) -> Verdict {
    lexical::judge_tool(tool_name, params)
}

/// One invocation with structured (JSON) parameters (the lexical reading).
pub fn judge_params(tool_name: &str, params: &serde_json::Value) -> Verdict {
    lexical::judge_params(tool_name, params)
}

/// Whether the command NAME alone is a dangerous tool.
pub fn is_dangerous_tool(tool_name: &str) -> bool {
    lexical::is_dangerous_tool(tool_name)
}

/// The severity label (`"critical"`, `"high"`, `"medium"`) of the row an
/// invocation trips; `"unknown"` when none.
pub fn severity_of(tool_name: &str, params: &str) -> &'static str {
    lexical::severity_of(tool_name, params)
}

/// How risky a command's TEXT reads for a permission prompt, in `0.0..=1.0`:
/// the sum of the weights of the vocabulary's fragments the lowercased text
/// contains. Deliberately coarse and textual (a fragment inside a quote still
/// counts) — it is a pre-filter for a human prompt, not a block decision, so the
/// thresholds that turn the score into allow / ask / deny belong to the caller.
pub fn prompt_risk(command: &str) -> f32 {
    let lower = command.to_ascii_lowercase();
    let total: f32 = RULES
        .iter()
        .flat_map(|rule| rule.prompt.iter())
        .filter_map(|&(fragment, weight)| lower.contains(fragment).then_some(weight))
        .sum();
    total.min(1.0_f32)
}

/// Every command a row's route names. A route is only a route when the policy
/// lets it through, and the tests run each one through [`judge`].
pub fn route_examples() -> impl Iterator<Item = &'static str> {
    RULES
        .iter()
        .flat_map(|rule| rule.route_examples.iter().copied())
}
