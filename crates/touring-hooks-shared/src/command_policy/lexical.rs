//! The LEXICAL reading: every simple command of the line, judged by its WORDS.
//!
//! 18/09/2026 — the hook used to pick ONE tool (the last `&&` segment) and take
//! as its parameters whatever followed that word at the START of the line, so
//! `cd x && git push -f` validated `git` with no parameters at all. And every
//! check read raw text: `-f` matched inside `touring-foundation` (a `git add` of
//! that path was denied "Force operation"), `rm my-file.txt` was denied "Force
//! without confirmation", and `git push --force … # --dry-run` was ALLOWED
//! because the bypass matched a comment. Now the line is read the way a shell
//! reads it (`super::lexer`), each simple command is judged by its own words,
//! and a flag is a word, never a substring.
//!
//! Patterns that span a pipe (`curl … | sh`) are read on the PIPELINE, never on
//! its commands one by one: split at `|`, neither half is dangerous.
//!
//! Within one simple command the order is: intent words, fixed prefixes, regex
//! patterns, per-tool schemas. The first row that fires is the verdict.

use super::lexer::{command_of, flag_matches, option_words, shell_pipelines};
use super::vocabulary::{Matcher, ParamCheck, RULES, Rule};
use super::{DELETE_ROUTE, Verdict, with_route};

/// Universal intent disclosure: a word that opts ONE simple command into the safe
/// variant of an otherwise destructive operation. `--dry-run` makes a command
/// non-mutating by definition. Git's `--force-with-lease` is deliberately NOT an
/// intent word: the shell guard treats every forced push as destructive.
/// Without `--dry-run`, `rm -rf --dry-run /tmp` was blocked even though the
/// structural reading had cleared it (Wave v4.29.0). A bypass is a WORD of the
/// command, never text anywhere in the line: matched as a substring,
/// `git push --force origin main # --dry-run` was allowed by its own comment
/// (18/09/2026).
///
/// `accepts_value`: whether `--word=<v>` is the same word. The structural
/// reading's own bypass grammar accepts `=<v>` for all of its words.
const INTENT_WORDS: &[(&str, bool)] = &[("--dry-run", false)];

fn is_intent_word(word: &str) -> bool {
    INTENT_WORDS.iter().any(|&(intent, accepts_value)| {
        word == intent
            || (accepts_value
                && word
                    .strip_prefix(intent)
                    .is_some_and(|rest| rest.starts_with('=')))
    })
}

/// The route a refusal of this tool names: every refusal of `rm`/`rmdir` is a
/// refusal of a delete, and names the one delete route still open.
fn delete_route(tool: &str) -> Option<&'static str> {
    matches!(tool.to_ascii_lowercase().as_str(), "rm" | "rmdir").then_some(DELETE_ROUTE)
}

/// A fixed-prefix row, unpacked.
struct PrefixRow {
    rule: &'static Rule,
    prefix: &'static str,
    param: ParamCheck,
    severity: &'static str,
}

/// The fixed-prefix rows, in table order.
fn prefix_rows() -> impl Iterator<Item = PrefixRow> {
    RULES.iter().filter_map(|rule| match &rule.matcher {
        Matcher::Prefix {
            prefix,
            param,
            severity,
        } => Some(PrefixRow {
            rule,
            prefix,
            param: *param,
            severity,
        }),
        _ => None,
    })
}

/// The first regex row `full_command` matches (with its parameter condition,
/// when it has one, met by `params`).
fn dangerous_match(full_command: &str, params: &str) -> Option<&'static Rule> {
    RULES.iter().find(|rule| match &rule.matcher {
        Matcher::Pattern {
            pattern,
            param_pattern,
            ..
        } => {
            pattern.is_match(full_command)
                && param_pattern.as_ref().is_none_or(|pp| pp.is_match(params))
        }
        _ => false,
    })
}

/// Judge a whole shell command line: every simple command in it
/// (`a && b; c | d`), each by its own words, then every multi-command pipeline
/// as one text.
pub(super) fn judge_line(command: &str) -> Verdict {
    let git = super::git::judge(command);
    if git.is_block() {
        return git;
    }
    for pipeline in shell_pipelines(command) {
        for words in &pipeline {
            // Judge the wrapper before `command_of` strips it: otherwise
            // `sudo rm <file>` loses the elevated-delete row entirely.
            let wrapped = words.join(" ");
            if let Some(rule) = dangerous_match(&wrapped, &wrapped) {
                return Verdict::Block {
                    reason: with_route(rule.reason, rule.route),
                };
            }
            if let Some((tool, args)) = command_of(words) {
                let verdict = judge_parts(tool, args, &args.join(" "));
                if verdict.is_block() {
                    return verdict;
                }
            }
        }
        if pipeline.len() > 1 {
            let text = pipeline
                .iter()
                .map(|words| words.join(" "))
                .collect::<Vec<_>>()
                .join(" | ");
            if let Some(rule) = dangerous_match(&text, &text) {
                // V0e2: this path never had a route — not even the delete one.
                // The reason names the safe alternative of ITS row, no more.
                return Verdict::Block {
                    reason: with_route(rule.reason, rule.route),
                };
            }
        }
    }
    Verdict::Allow
}

/// Judge one invocation given as a command name and its parameter text.
pub(super) fn judge_tool(tool_name: &str, params: &str) -> Verdict {
    // Words for the flag rules and the bypass; the raw text for the patterns,
    // exactly as this entry point always matched them.
    let args: Vec<String> = shell_pipelines(params)
        .into_iter()
        .flatten()
        .flatten()
        .collect();
    judge_parts(tool_name, &args, params)
}

/// One simple command: its command word, its argument WORDS (flag rules, intent
/// words) and the parameter TEXT the patterns are matched against.
fn judge_parts(tool_name: &str, args: &[String], params: &str) -> Verdict {
    let full_command = format!("{tool_name} {params}");
    let full_lower = full_command.to_lowercase();

    if args.iter().any(|word| is_intent_word(word)) {
        return Verdict::Allow;
    }

    // A refusal names what to do instead. 18/09/2026: `rm -r build/` was
    // refused with "Recursive deletion" and nothing else; agents retried blind.
    let route = delete_route(tool_name);

    // Fast path: O(m) starts_with for fixed-prefix rows.
    for row in prefix_rows() {
        if full_lower.starts_with(row.prefix) && row.param.holds(args) {
            return block(row.rule.reason, row.rule.route.or(route));
        }
    }

    // Slow path: regex rows for complex conditions.
    if let Some(rule) = dangerous_match(&full_command, params) {
        return block(rule.reason, rule.route.or(route));
    }

    schema_violation(tool_name, args, params, &full_command).unwrap_or(Verdict::Allow)
}

/// Per-tool schemas. The command NAME is matched case-folded, like every prefix
/// above (`Rm -r x` is `rm -r x`); its flags are not — `git commit -F msg` names a
/// file, `-f` forces. The reason names what matched, so the retry can fix it, and
/// the route says what to run instead.
fn schema_violation(
    tool_name: &str,
    args: &[String],
    params: &str,
    full_command: &str,
) -> Option<Verdict> {
    let tool = tool_name.to_ascii_lowercase();
    RULES.iter().find_map(|rule| {
        let violation = match &rule.matcher {
            Matcher::SchemaFlag { tool: t, flag } if *t == tool => option_words(args)
                .find(|word| flag_matches(flag, word))
                .map(|word| format!("{}: flag `{word}`", rule.reason)),
            Matcher::SchemaPattern { tool: t, pattern }
                if *t == tool && pattern.is_match(params) =>
            {
                Some(rule.reason.to_string())
            }
            _ => None,
        }?;
        Some(block(
            &format!("{violation} — in `{full_command}`"),
            rule.route,
        ))
    })
}

/// Judge structured parameters (JSON): one `"<tool> <key>=<value>"` text per field.
/// Prefix rows refuse without a parameter check and name only the tool's own
/// route; regex rows refuse with no route at all — as this surface always did.
pub(super) fn judge_params(tool_name: &str, params_json: &serde_json::Value) -> Verdict {
    let Some(obj) = params_json.as_object() else {
        return Verdict::Allow;
    };
    for (key, value) in obj {
        let full_cmd = format!("{tool_name} {key}={value}");
        let full_cmd_lower = full_cmd.to_lowercase();
        for row in prefix_rows() {
            if full_cmd_lower.starts_with(row.prefix) {
                return block(row.rule.reason, delete_route(tool_name));
            }
        }
        if let Some(rule) = dangerous_match_text(&full_cmd) {
            return block(rule.reason, None);
        }
    }
    Verdict::Allow
}

/// The first regex row whose pattern alone (no parameter condition) matches.
fn dangerous_match_text(text: &str) -> Option<&'static Rule> {
    RULES.iter().find(|rule| match &rule.matcher {
        Matcher::Pattern { pattern, .. } => pattern.is_match(text),
        _ => false,
    })
}

/// Whether `tool_name` alone names a dangerous tool: a fixed prefix (with a space
/// appended so prefix matching works as in [`judge_tool`]), else a regex row.
pub(super) fn is_dangerous_tool(tool_name: &str) -> bool {
    let probe = format!("{} ", tool_name.to_lowercase());
    prefix_rows().any(|row| probe.starts_with(row.prefix))
        || dangerous_match_text(tool_name).is_some()
}

/// The severity label of the row an invocation trips; `"unknown"` when none.
pub(super) fn severity_of(tool_name: &str, params: &str) -> &'static str {
    let full_command = format!("{tool_name} {params}");
    let full_lower = full_command.to_lowercase();
    if let Some(row) = prefix_rows().find(|row| full_lower.starts_with(row.prefix)) {
        return row.severity;
    }
    RULES
        .iter()
        .find_map(|rule| match &rule.matcher {
            Matcher::Pattern {
                pattern, severity, ..
            } if pattern.is_match(&full_command) => Some(*severity),
            _ => None,
        })
        .unwrap_or("unknown")
}

fn block(reason: &str, route: Option<&str>) -> Verdict {
    Verdict::Block {
        reason: with_route(reason, route),
    }
}
