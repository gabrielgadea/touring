//! The destructive-command vocabulary — ONE table.
//!
//! Every row is one thing a command line can do that the policy refuses,
//! flags, or scores. A row says WHAT it matches ([`Matcher`]), WHY it refuses
//! ([`Rule::reason`]), the road back ([`Rule::route`] + [`Rule::route_examples`])
//! and, when a permission prompt scores the same thing as text, the weight of
//! its textual fragments ([`Rule::prompt`]).
//!
//! The three matcher families are three READINGS of a command, not three
//! vocabularies — which is how this table used to exist, in three crates, one
//! of which answered the same command differently from the next (the REGRA #11
//! v2 token that was unreachable because two owners disagreed). The reading
//! each family belongs to, and the order the readings run in, live in
//! `super` (the precedence), not here.
//!
//! Order matters in two ways, both pinned by the tests in `super::tests`:
//! within one family the FIRST matching row wins (its reason is the verdict's),
//! and `prompt` fragments are summed in table order.

use regex::Regex;
use std::sync::LazyLock;

use super::lexer::{flag_matches, option_words};
use super::{DD_ROUTE, DELETE_ROUTE, DELETE_ROUTE_EXAMPLES, PIPE_SHELL_ROUTE};

/// The road back from a destructive git verdict (REGRA #11 v2).
pub(super) const GIT_ROUTE: &str = "push without force; if history rewriting is intentional, follow \
     REGRA #11 (MEDIR→SNAPSHOT→CONFIRMAR) and put `GIT_DESTRUCTIVE_OK=1` in the same command";

/// Severity of a structural row: does the line stop, or only get flagged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Severity {
    Block,
    Warn,
}

/// The parameter condition of a [`Matcher::Prefix`] row.
///
/// 18/09/2026 — `rm ` used the regex `-rf|-r\s+|-f\s+` over the raw text, which
/// erred both ways: `rm -f x` was denied "Recursive force delete" (a recursion
/// that was never there), `rm some-r dir` matched `-r ` inside a file name, and
/// `rm -fr /` matched none of the three alternatives. Flags are judged as words
/// ([`flag_matches`]).
#[derive(Clone, Copy)]
pub(super) enum ParamCheck {
    /// Any invocation of the command blocks.
    Always,
    /// The command's option words (see [`option_words`]) meet the predicate.
    Words(fn(&[String]) -> bool),
}

impl ParamCheck {
    pub(super) fn holds(self, args: &[String]) -> bool {
        match self {
            Self::Always => true,
            Self::Words(predicate) => predicate(args),
        }
    }
}

/// `rm` that is both recursive and forced — the pair that deletes a tree with no
/// prompt and no error. Either flag alone is still refused by the `rm` schema
/// rows, whose reason names the flag it saw. Case-folded like the prefix it
/// guards: `RM -RF /` is the same request (and the same command on a
/// case-insensitive filesystem), and on this path folding errs toward refusing.
fn rm_recursive_and_forced(args: &[String]) -> bool {
    let words: Vec<String> = option_words(args).map(str::to_ascii_lowercase).collect();
    let has = |flags: &[&str]| {
        words
            .iter()
            .any(|word| flags.iter().any(|flag| flag_matches(flag, word)))
    };
    has(&["-r", "--recursive"]) && has(&["-f", "--force"])
}

/// `rmdir --parents` / `-p`: removes every emptied ancestor too.
fn rmdir_parents(args: &[String]) -> bool {
    option_words(args).any(|word| flag_matches("--parents", word) || flag_matches("-p", word))
}

/// How a row reads a command. Each variant belongs to one reading; the stage
/// that runs a reading only ever looks at its own variants.
pub(super) enum Matcher {
    /// STRUCTURAL: `tokens` are a contiguous run of the quote-stripped,
    /// comment-stripped token stream (`["rm", "-rf"]` matches `rm -rf foo`, not
    /// `rm foo -rf bar`); `tail_token`, when set, must also appear somewhere
    /// after (`find <path…> -delete`). Any word of `bypass_words` among the
    /// tokens disables the row — a WORD, never text anywhere in the line:
    /// matched as a substring, `rm -rf / # --dry-run` was allowed by its own
    /// comment. `--x` also matches `--x=value`.
    Signature {
        tokens: &'static [&'static str],
        tail_token: Option<&'static str>,
        severity: Severity,
        bypass_words: &'static [&'static str],
    },
    /// LEXICAL: the lowercased `"<command> <params>"` starts with `prefix`
    /// (lowercase, trailing space so `"rm "` never matches `"rmdir"`) and the
    /// command's words meet `param`. `severity` is the label `severity_of`
    /// reports.
    Prefix {
        prefix: &'static str,
        param: ParamCheck,
        severity: &'static str,
    },
    /// LEXICAL: a regex over `"<command> <params>"` (and over a whole pipeline
    /// text, which is how `curl … | sh` is seen), with an optional second regex
    /// the parameters must also match.
    Pattern {
        pattern: Regex,
        param_pattern: Option<Regex>,
        severity: &'static str,
    },
    /// LEXICAL: a command named `tool` is refused when one of its option words
    /// is the flag `flag` (a word — `-f` is a short cluster holding the
    /// letter, never a substring of a path). The row's `reason` names the flag.
    SchemaFlag {
        tool: &'static str,
        flag: &'static str,
    },
    /// LEXICAL: a command named `tool` is refused when its parameter text
    /// matches `pattern`.
    SchemaPattern { tool: &'static str, pattern: Regex },
    /// PROMPT only: never refuses; exists so a textual fragment a permission
    /// prompt scores has a home in this table.
    PromptOnly,
}

/// One row of the vocabulary.
pub(super) struct Rule {
    /// What is wrong with the command (the verdict's text, before the route).
    pub reason: &'static str,
    /// What to do instead. The verdict carries `reason; route` verbatim, and a
    /// route may only name commands the policy lets through (checked by
    /// `route_examples_are_open`).
    pub route: Option<&'static str>,
    /// Commands that follow `route`; the tests run each through [`super::judge`].
    pub route_examples: &'static [&'static str],
    /// Lowercase text fragments a permission prompt scores, with their weight.
    pub prompt: &'static [(&'static str, f32)],
    pub matcher: Matcher,
}

fn re(pattern: &str) -> Regex {
    Regex::new(pattern).expect("vocabulary pattern must compile")
}

/// A row with no route, no examples and no prompt fragments.
fn row(reason: &'static str, matcher: Matcher) -> Rule {
    Rule {
        reason,
        route: None,
        route_examples: &[],
        prompt: &[],
        matcher,
    }
}

/// The vocabulary. Order is part of the contract (see the module doc).
pub(super) static RULES: LazyLock<Vec<Rule>> = LazyLock::new(|| {
    vec![
        // ── STRUCTURAL: what a token signature over the stripped line refuses ─────
        // 18/09/2026 — the rm rules once told the agent to "pass --dry-run or scope
        // an explicit path you have verified": `rm` has no `--dry-run`, and no
        // predicate here ever looked at a path. Three sessions spent retries on that
        // road in one day before a human ran the delete. The route now names only
        // what the policy lets through.
        Rule {
            reason: "rm -rf — destructive recursive delete",
            route: Some(DELETE_ROUTE),
            route_examples: DELETE_ROUTE_EXAMPLES,
            prompt: &[("rm -rf", 0.40)],
            matcher: Matcher::Signature {
                tokens: &["rm", "-rf"],
                tail_token: None,
                severity: Severity::Block,
                bypass_words: &["--dry-run", "--help"],
            },
        },
        Rule {
            reason: "rm -fr — destructive recursive delete (alias of -rf)",
            route: Some(DELETE_ROUTE),
            route_examples: DELETE_ROUTE_EXAMPLES,
            prompt: &[],
            matcher: Matcher::Signature {
                tokens: &["rm", "-fr"],
                tail_token: None,
                severity: Severity::Block,
                bypass_words: &["--dry-run", "--help"],
            },
        },
        Rule {
            reason: "find -delete — destructive bulk delete; list the matches with `find <path> -print` first",
            route: Some(DELETE_ROUTE),
            route_examples: &["find build -name stale.log -print"],
            prompt: &[],
            matcher: Matcher::Signature {
                tokens: &["find"],
                tail_token: Some("-delete"),
                severity: Severity::Block,
                bypass_words: &["--help"],
            },
        },
        Rule {
            reason: "kill -9 force terminates a process",
            route: Some(
                "ask the process to stop first (`kill <pid>`); `kill -9` is the user's to run",
            ),
            route_examples: &["kill 2345"],
            prompt: &[],
            matcher: Matcher::Signature {
                tokens: &["kill", "-9"],
                tail_token: None,
                severity: Severity::Block,
                bypass_words: &[],
            },
        },
        Rule {
            reason: "pkill -9 force terminates processes",
            route: Some(
                "ask matching processes to stop first (`pkill <name>`); `pkill -9` is the user's to run",
            ),
            route_examples: &["pkill nginx"],
            prompt: &[],
            matcher: Matcher::Signature {
                tokens: &["pkill", "-9"],
                tail_token: None,
                severity: Severity::Block,
                bypass_words: &[],
            },
        },
        Rule {
            reason: "Killall terminates all matching processes with -9",
            route: Some(
                "ask the process to stop first (`kill <pid>`); `killall -9` is the user's to run",
            ),
            route_examples: &["kill 2345"],
            prompt: &[],
            matcher: Matcher::Signature {
                tokens: &["killall", "-9"],
                tail_token: None,
                severity: Severity::Block,
                bypass_words: &[],
            },
        },
        Rule {
            reason: "sudo rm — destructive delete with elevated privileges",
            route: Some(DELETE_ROUTE),
            route_examples: &[],
            prompt: &[],
            matcher: Matcher::Signature {
                tokens: &["sudo"],
                tail_token: Some("rm"),
                severity: Severity::Block,
                bypass_words: &[],
            },
        },
        Rule {
            reason: "chmod -R 777 — world-writable recursive grant",
            route: Some("grant the narrowest mode that works, e.g. `chmod -R u+rwX,go-w <dir>`"),
            route_examples: &["chmod -R u+rwX,go-w build"],
            prompt: &[],
            matcher: Matcher::Signature {
                tokens: &["chmod", "-R", "777"],
                tail_token: None,
                severity: Severity::Warn,
                bypass_words: &[],
            },
        },
        // ── LEXICAL, fixed command prefix (O(m) `starts_with`, no regex engine) ───
        // File destruction.
        row(
            "Recursive force delete detected — risk of data loss",
            Matcher::Prefix {
                // "rm " — the trailing space keeps "rmdir" out.
                prefix: "rm ",
                param: ParamCheck::Words(rm_recursive_and_forced),
                severity: "critical",
            },
        ),
        row(
            "Recursive directory remove detected",
            Matcher::Prefix {
                prefix: "rmdir ",
                param: ParamCheck::Words(rmdir_parents),
                severity: "high",
            },
        ),
        Rule {
            // "del " — Windows delete; any invocation is suspicious.
            reason: "Windows delete command — verify target",
            route: Some(DELETE_ROUTE),
            route_examples: DELETE_ROUTE_EXAMPLES,
            prompt: &[],
            matcher: Matcher::Prefix {
                prefix: "del ",
                param: ParamCheck::Always,
                severity: "high",
            },
        },
        // Shell dangerous.
        Rule {
            // "dd " — low-level disk write; any invocation is dangerous.
            // V0e2 (09/10/2026): file-to-file copy is `cp`, sizing is `truncate`.
            reason: "dd low-level disk operation — risk of data loss",
            route: Some(DD_ROUTE),
            route_examples: &["cp a.img b.img", "truncate -s 1M disk.img"],
            prompt: &[("dd if=", 0.75)],
            matcher: Matcher::Prefix {
                prefix: "dd ",
                param: ParamCheck::Always,
                severity: "critical",
            },
        },
        row(
            "Disk partition manipulation",
            Matcher::Prefix {
                prefix: "fdisk ",
                param: ParamCheck::Always,
                severity: "critical",
            },
        ),
        row(
            "Disk partition manipulation",
            Matcher::Prefix {
                prefix: "parted ",
                param: ParamCheck::Always,
                severity: "critical",
            },
        ),
        row(
            "LVM physical volume removal",
            Matcher::Prefix {
                prefix: "pvremove ",
                param: ParamCheck::Always,
                severity: "critical",
            },
        ),
        row(
            "LVM logical volume deletion",
            Matcher::Prefix {
                prefix: "lvremove ",
                param: ParamCheck::Always,
                severity: "critical",
            },
        ),
        // Process killing.
        row(
            "Killall terminates all matching processes",
            Matcher::Prefix {
                prefix: "killall ",
                param: ParamCheck::Always,
                severity: "high",
            },
        ),
        // ── LEXICAL, regex (alternation, wildcards, lookahead) ────────────────────
        // Git is NOT here: `block_git.sh` is the single owner of git policy (S25,
        // D2-A, 25/09/2026) — two owners made the REGRA #11 v2 token unreachable.
        Rule {
            // sudo needs a param check against multiple dangerous sub-commands.
            prompt: &[("sudo", 0.25)],
            ..row(
                "sudo with dangerous commands",
                Matcher::Pattern {
                    pattern: re(r"(?i)^sudo\s+"),
                    param_pattern: Some(re(r"(?i)-i$|rm\s+|dd\s+")),
                    severity: "high",
                },
            )
        },
        Rule {
            // mkfs has variant suffixes: mkfs, mkfs.ext4, mkfs.btrfs — regex required.
            prompt: &[("mkfs", 0.75)],
            ..row(
                "Filesystem format — irreversible data destruction",
                Matcher::Pattern {
                    pattern: re(r"(?i)^mkfs(\.[a-z][a-z0-9]*)?\s+"),
                    param_pattern: None,
                    severity: "critical",
                },
            )
        },
        row(
            "Removes all permissions from file",
            Matcher::Pattern {
                // chmod 000 — complex flag prefix before the mode number.
                pattern: re(r"(?i)^chmod\s+(-[RrXx]+\s+)*000"),
                param_pattern: None,
                severity: "high",
            },
        ),
        row(
            "Ownership transfer to root",
            Matcher::Pattern {
                // chown :root — needs a wildcard between chown and :root.
                pattern: re(r"(?i)^chown\s+.*:\s*root\s+"),
                param_pattern: None,
                severity: "medium",
            },
        ),
        row(
            "Firewall rules flush — security exposure",
            Matcher::Pattern {
                pattern: re(r"(?i)^iptables\s+"),
                param_pattern: Some(re(r"(?i)-F|-X|--flush|--delete-chain")),
                severity: "high",
            },
        ),
        row(
            "Firewall disable — security exposure",
            Matcher::Pattern {
                // "ufw disable" — two-word command, not a pure prefix on the tool alone.
                pattern: re(r"(?i)^ufw\s+disable"),
                param_pattern: None,
                severity: "critical",
            },
        ),
        // Pipe to shell, both producers. V0e2: download, read, THEN run.
        Rule {
            route: Some(PIPE_SHELL_ROUTE),
            route_examples: &["curl -O https://example.com/install.sh", "bash install.sh"],
            ..row(
                "Pipe to shell — remote code execution risk",
                Matcher::Pattern {
                    // curl.*|.*sh — wildcard in the middle, regex required.
                    pattern: re(r"(?i)^curl\s+.*\|.*sh"),
                    param_pattern: None,
                    severity: "critical",
                },
            )
        },
        Rule {
            route: Some(PIPE_SHELL_ROUTE),
            route_examples: &[],
            ..row(
                "Pipe to shell — remote code execution risk",
                Matcher::Pattern {
                    pattern: re(r"(?i)^wget\s+.*\|.*sh"),
                    param_pattern: None,
                    severity: "critical",
                },
            )
        },
        row(
            "kill -9 PID 1 is init — system will halt",
            Matcher::Pattern {
                // "kill -9 1" — specific PID, needs a numeric match after flags.
                pattern: re(r"(?i)^kill\s+-9\s+1"),
                param_pattern: None,
                severity: "critical",
            },
        ),
        row(
            "kill -9 force terminates a process",
            Matcher::Pattern {
                pattern: re(r"(?i)^kill\s+-9\s+\S+"),
                param_pattern: None,
                severity: "high",
            },
        ),
        row(
            "pkill -9 force terminates processes",
            Matcher::Pattern {
                // `.*` may be empty: the usual spelling is exactly `pkill -9 <name>`.
                pattern: re(r"(?i)^pkill\s+.*-9(?:\s|$)"),
                param_pattern: None,
                severity: "high",
            },
        ),
        row(
            "sysctl kernel parameter change",
            Matcher::Pattern {
                // sysctl -w key=0 — param check for the write flag plus value.
                pattern: re(r"(?i)^sysctl\s+"),
                param_pattern: Some(re(r"(?i)-w\s+.*=.*0")),
                severity: "high",
            },
        ),
        row(
            "Bind mount can bypass security boundaries",
            Matcher::Pattern {
                pattern: re(r"(?i)^mount\s+.*--bind"),
                param_pattern: None,
                severity: "medium",
            },
        ),
        row(
            "Force unmount can cause data loss",
            Matcher::Pattern {
                pattern: re(r"(?i)^umount\s+.*-f"),
                param_pattern: None,
                severity: "high",
            },
        ),
        // ── LEXICAL, per-tool schemas ─────────────────────────────────────────────
        // `rm`: each flag is its own row so the refusal NAMES the flag it saw.
        Rule {
            route: Some(DELETE_ROUTE),
            ..row(
                "Disables root protection",
                Matcher::SchemaFlag {
                    tool: "rm",
                    flag: "--no-preserve-root",
                },
            )
        },
        Rule {
            route: Some(DELETE_ROUTE),
            ..row(
                "Recursive deletion",
                Matcher::SchemaFlag {
                    tool: "rm",
                    flag: "--recursive",
                },
            )
        },
        Rule {
            route: Some(DELETE_ROUTE),
            ..row(
                "Recursive deletion",
                Matcher::SchemaFlag {
                    tool: "rm",
                    flag: "-r",
                },
            )
        },
        Rule {
            route: Some(DELETE_ROUTE),
            ..row(
                "Force without confirmation",
                Matcher::SchemaFlag {
                    tool: "rm",
                    flag: "-f",
                },
            )
        },
        // `bash`: its one rule is a delete, so its one route is the delete route.
        Rule {
            route: Some(DELETE_ROUTE),
            ..row(
                "Recursive delete in bash",
                Matcher::SchemaPattern {
                    tool: "bash",
                    pattern: re(r"(?i)rm\s+(sudo\s+)?-rf"),
                },
            )
        },
        // ── PROMPT only: text a permission prompt scores that no reading refuses ──
        // High — executing remote code, whoever the producer is.
        Rule {
            prompt: &[
                ("| sh", 0.75),
                ("|sh", 0.75),
                ("| bash", 0.75),
                ("|bash", 0.75),
            ],
            ..row("pipe to a shell, any producer", Matcher::PromptOnly)
        },
        Rule {
            prompt: &[(">/dev/sd", 0.75)],
            ..row("redirect onto a raw disk", Matcher::PromptOnly)
        },
        Rule {
            prompt: &[(":(){", 0.75)],
            ..row("fork bomb", Matcher::PromptOnly)
        },
        // Medium — privilege or arbitrary execution.
        Rule {
            prompt: &[("eval ", 0.75)],
            ..row("eval of constructed text", Matcher::PromptOnly)
        },
        Rule {
            prompt: &[("chmod_rrr", 0.50)],
            ..row(
                "recursive world-writable grant (token spelling)",
                Matcher::PromptOnly,
            )
        },
        // Low — suspicious but context-dependent.
        Rule {
            prompt: &[(">/dev/", 0.25)],
            ..row("redirect into /dev", Matcher::PromptOnly)
        },
    ]
});
