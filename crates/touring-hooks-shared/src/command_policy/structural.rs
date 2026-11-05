//! The STRUCTURAL reading: token signatures over the quote-stripped line.
//!
//! First the line is cleaned — quoted strings and `#` comments replaced by
//! blanks ([`strip_string_literals_and_comments`]) — so `echo "rm -rf"` and
//! `# rm -rf in a comment` never fire; then every `Matcher::Signature` row of
//! the vocabulary is tried over the cleaned token stream.
//!
//! Wave 5 (2026-05-23) historical note: this tokenizer-based design originally
//! worked around the `ast-grep-language` 0.36 / tree-sitter v15 ABI mismatch
//! that prevented direct AST parsing of bash. With ast-grep 0.42 +
//! tree-sitter-bash 0.25 true AST parsing IS available, and the tokenizer was
//! kept as the chosen design: (a) the structural tests have zero regressions and
//! zero flakes, (b) the fail-open contract is simpler to reason about over a flat
//! token stream, (c) shell-quoting edge cases (heredocs, ANSI-C `$'…'`,
//! parameter expansion `${…}`) are still ahead of any AST grammar. The AST path
//! stays available as a complement via `touring-code`'s polyglot search for
//! callers that need parse-tree access (decision of
//! `lesson:wave-5-mossy-crunching-owl-S9-S13-S14-closure:2026-05-23`).

use std::time::{Duration, Instant};

use super::vocabulary::{Matcher, RULES, Rule, Severity};
use super::{Verdict, with_route};

/// Hard budget of this reading (kept far below pre-bash's <10 ms target). When it
/// runs out the remaining rows are skipped — the fail-open mode of the policy.
pub(super) const BUDGET: Duration = Duration::from_millis(5);

/// Judge `command` structurally. `Block` on the first blocking signature, else
/// `Warn` on the first warning one, else `Allow`.
pub(super) fn judge(command: &str) -> Verdict {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return Verdict::Allow;
    }

    let cleaned = strip_string_literals_and_comments(trimmed);
    if cleaned.trim().is_empty() {
        return Verdict::Allow;
    }
    // Non-empty after the trim above, so there is at least one token.
    let tokens: Vec<&str> = cleaned.split_whitespace().collect();

    let started = Instant::now();
    let mut warn: Option<&'static Rule> = None;

    for rule in RULES.iter() {
        let Matcher::Signature {
            tokens: signature,
            tail_token,
            severity,
            bypass_words,
        } = &rule.matcher
        else {
            continue;
        };
        if started.elapsed() >= BUDGET {
            break;
        }
        if !fires(&tokens, signature, *tail_token, bypass_words) {
            continue;
        }
        match severity {
            Severity::Block => {
                return Verdict::Block {
                    reason: with_route(rule.reason, rule.route),
                };
            }
            Severity::Warn => {
                warn.get_or_insert(rule);
            }
        }
    }

    match warn {
        Some(rule) => Verdict::Warn {
            reason: with_route(rule.reason, rule.route),
        },
        None => Verdict::Allow,
    }
}

/// `true` when the signature is on the token stream and no intent word
/// disables it. The tail, when there is one, may sit anywhere after — `find`
/// rows use it to span over user-supplied paths.
fn fires(
    tokens: &[&str],
    signature: &[&str],
    tail_token: Option<&str>,
    bypass_words: &[&str],
) -> bool {
    !discloses_intent(tokens, bypass_words)
        && contains_subsequence(tokens, signature)
        && tail_token.is_none_or(|tail| tokens.contains(&tail))
}

/// `true` when one of `words` is a word of the cleaned command, so quoted
/// text and `#` comments never count: `--dry-run` inside `# --dry-run` is
/// not a flag the command passes. `--x` also matches `--x=value`.
fn discloses_intent(tokens: &[&str], words: &[&str]) -> bool {
    tokens.iter().any(|token| {
        words.iter().any(|word| {
            *token == *word
                || token
                    .strip_prefix(word)
                    .is_some_and(|rest| rest.starts_with('='))
        })
    })
}

/// Replace single- and double-quoted string contents with spaces, and drop
/// everything from `#` (when not inside a string) to end of line.
///
/// This is a deliberately small state machine — it does NOT model `$()`
/// command substitution or `$VAR` expansion. The output is suitable for the
/// coarse-grained token matching but should not be treated as a real
/// shell-parsed command.
pub(super) fn strip_string_literals_and_comments(input: &str) -> String {
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
