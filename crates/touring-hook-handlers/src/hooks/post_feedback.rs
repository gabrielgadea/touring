//! Shared post-mutation feedback (S28, plan 2026-09-25): the antipattern block gate and
//! the issue/signal rendering that `post_edit` and `post_write` each carried a copy of.
//!
//! Two defects this module closes, both measured on 25/09/2026:
//! - **The block gate was dead.** Both copies parsed `ANTIPATTERN [Nx]: …` — a form only
//!   test fixtures wrote; the detector emits `[lang] message (Nx)` — so the delta was always
//!   zero and `Block` never fired. Its baseline was a per-tool cache that started EMPTY, so
//!   fixing the parse alone would have blocked a file for its PRE-EXISTING antipatterns on
//!   the first edit. The gate now compares structured counts
//!   ([`touring_analysis::quality::antipatterns::count_antipatterns`]) of the content
//!   before and after the mutation.
//! - **"N issue(s)" counted information.** A first health observation, a workflow summary
//!   or a cognitive enrichment made `post-write: 2 issue(s)` with zero issues in it. Items
//!   are typed where they are produced ([`Feedback`]); only issues are counted, and what a
//!   budget elides is declared, never silent.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::Path;

use touring_analysis::quality::antipatterns::count_antipatterns;

use super::runtime::HookResponse;

/// NEW antipattern occurrences (summed over rules) at which a mutation is blocked.
pub(crate) const BLOCK_ANTIPATTERN_THRESHOLD: usize = 4;

/// The tool that mutated the file — only the verdict's wording depends on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mutation {
    /// `Edit` / `MultiEdit`.
    Edit,
    /// `Write`.
    Write,
}

impl Mutation {
    fn noun(self) -> &'static str {
        match self {
            Self::Edit => "Edit",
            Self::Write => "Write",
        }
    }
}

/// What a post hook reports, typed where it is produced.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct Feedback {
    /// Defects of the file — counted in the header.
    pub(crate) issues: Vec<String>,
    /// Information (first observations, summaries, enrichment) — shown, never counted.
    pub(crate) signals: Vec<String>,
}

impl Feedback {
    /// Nothing to report.
    pub(crate) fn is_empty(&self) -> bool {
        self.issues.is_empty() && self.signals.is_empty()
    }
}

/// The `Block` verdict when the mutation introduced at least
/// [`BLOCK_ANTIPATTERN_THRESHOLD`] new antipattern occurrences; `None` otherwise.
pub(crate) fn check_block_gate(
    new_count: usize,
    rel_path: &str,
    mutation: Mutation,
    feedback: &Feedback,
) -> Option<HookResponse> {
    if new_count < BLOCK_ANTIPATTERN_THRESHOLD {
        return None;
    }
    Some(HookResponse::Block {
        reason: format!(
            "{} blocked: too many new anti-patterns introduced in {rel_path} ({new_count} new \
             occurrence(s), threshold {BLOCK_ANTIPATTERN_THRESHOLD}). Fix them before continuing.",
            mutation.noun()
        ),
        context: Some(feedback.issues.join("\n")),
        event_name: Some("PostToolUse".to_string()),
    })
}

/// Keep items (in order) while they fit `budget` characters (+3 per separator) and return
/// the characters used. An item that does not fit is dropped and later, smaller ones still
/// get their chance — the behaviour both hooks had.
pub(crate) fn fit_budget(items: &mut Vec<String>, budget: usize) -> usize {
    let mut used = 0usize;
    items.retain(|s| {
        let len = s.len() + 3;
        if used + len <= budget {
            used += len;
            true
        } else {
            false
        }
    });
    used
}

fn edit_triple(v: &serde_json::Value) -> Option<(&str, &str, bool)> {
    let old = v.get("old_string")?.as_str()?;
    let new = v.get("new_string")?.as_str()?;
    let all = v
        .get("replace_all")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    Some((old, new, all))
}

/// Occurrences of antipatterns in `after` beyond those already in `before`, summed over
/// rules; `before = None` means the file did not exist, so every occurrence is new. Test
/// files never count (their `unwrap`/`assert` are idiomatic).
pub(crate) fn new_antipatterns(
    before: Option<&str>,
    after: &str,
    lang: &str,
    file_path: &str,
) -> usize {
    if crate::shared::quality::is_test_file(file_path) {
        return 0;
    }
    let path = Path::new(file_path);
    let old: HashMap<&'static str, usize> = before
        .map(|b| {
            count_antipatterns(b, lang, Some(path))
                .into_iter()
                .map(|c| (c.message, c.count))
                .collect()
        })
        .unwrap_or_default();
    count_antipatterns(after, lang, Some(path))
        .into_iter()
        .map(|c| {
            c.count
                .saturating_sub(old.get(c.message).copied().unwrap_or(0))
        })
        .sum()
}

/// Render feedback as one line:
/// `"<label>: N issue(s) | i1 | … | (+K elided by the budget) | ℹ s1 | …"`.
///
/// `listed` are the issues that survived the caller's budget and `elided` how many did not;
/// N is their sum, so the header always equals the listed items plus the declared elision.
/// Signals follow the `ℹ` marker and are never counted. `None` when there is nothing.
pub(crate) fn render_feedback(
    label: &str,
    listed: &[String],
    elided: usize,
    signals: &[String],
) -> Option<String> {
    let total = listed.len() + elided;
    if total == 0 && signals.is_empty() {
        return None;
    }
    let mut out = format!("{label}:");
    if total > 0 {
        let _ = write!(out, " {total} issue(s)");
        for issue in listed {
            let _ = write!(out, " | {issue}");
        }
        if elided > 0 {
            let _ = write!(out, " | (+{elided} elided by the budget)");
        }
    }
    if !signals.is_empty() {
        if total > 0 {
            out.push_str(" |");
        }
        let _ = write!(out, " \u{2139} {}", signals.join(" | "));
    }
    Some(out)
}

/// The file content BEFORE an Edit/MultiEdit, rebuilt from the content after it and the
/// tool input (S28). A post hook runs after the tool, so reading the disk there yields the
/// NEW content — `post_edit` compared the file with itself. The edits are reversed in
/// reverse order; a replacement is located by its first occurrence (antipattern counts do
/// not depend on where it sits). `None` when the input shape is unknown or a replacement
/// cannot be located (a pure deletion): callers then abstain.
pub(crate) fn content_before_edit(after: &str, tool_input: &serde_json::Value) -> Option<String> {
    let edits: Vec<(&str, &str, bool)> = match tool_input.get("edits").and_then(|v| v.as_array()) {
        Some(list) => list.iter().map(edit_triple).collect::<Option<_>>()?,
        None => vec![edit_triple(tool_input)?],
    };
    edits
        .into_iter()
        .rev()
        .try_fold(after.to_string(), |text, (old, new, all)| {
            if new.is_empty() || !text.contains(new) {
                return None;
            }
            Some(if all {
                text.replace(new, old)
            } else {
                text.replacen(new, old, 1)
            })
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    const CLEAN: &str = "fn a() -> Option<u8> {\n    Some(1)\n}\n";
    const FOUR_NEW: &str = "fn a() -> Option<u8> {\n    x.unwrap();\n    y.unwrap();\n    z.unwrap();\n    todo!();\n    Some(1)\n}\n";

    /// S28 — ONE gate for both hooks: the constant, the gate and the old text parser live
    /// nowhere else, and both hooks call this module's gate. Behaviour: 4 new occurrences
    /// block an Edit and a Write alike; pre-existing ones never do.
    #[test]
    fn block_gate_shared_between_post_edit_and_post_write() {
        for (name, src) in [
            ("post_edit.rs", include_str!("post_edit.rs")),
            ("post_write.rs", include_str!("post_write.rs")),
        ] {
            assert!(
                !src.contains("const BLOCK_ANTIPATTERN_THRESHOLD"),
                "{name} keeps its own threshold"
            );
            assert!(
                !src.contains("fn check_block_gate"),
                "{name} keeps its own gate"
            );
            assert!(
                !src.contains("RE_ANTIPATTERN_COUNT"),
                "{name} still re-parses formatted text"
            );
            assert!(
                src.contains("post_feedback::check_block_gate("),
                "{name} does not call the shared gate"
            );
        }
        let n = new_antipatterns(Some(CLEAN), FOUR_NEW, "rust", "src/a.rs");
        assert_eq!(n, 4, "3 unwrap + 1 todo introduced");
        for m in [Mutation::Edit, Mutation::Write] {
            match check_block_gate(n, "src/a.rs", m, &Feedback::default()) {
                Some(HookResponse::Block {
                    reason, event_name, ..
                }) => {
                    assert!(
                        reason.starts_with(m.noun())
                            && reason.contains("too many new anti-patterns"),
                        "{reason}"
                    );
                    assert!(reason.contains("src/a.rs"), "{reason}");
                    assert_eq!(event_name.as_deref(), Some("PostToolUse"));
                }
                other => panic!("{m:?}: expected Block, got {other:?}"),
            }
        }
    }

    #[test]
    fn pre_existing_antipatterns_never_block() {
        assert_eq!(
            new_antipatterns(Some(FOUR_NEW), FOUR_NEW, "rust", "src/a.rs"),
            0,
            "nothing new"
        );
        let one_more = FOUR_NEW.replace("    Some(1)", "    w.unwrap();\n    Some(1)");
        assert_eq!(
            new_antipatterns(Some(FOUR_NEW), &one_more, "rust", "src/a.rs"),
            1
        );
        assert!(check_block_gate(1, "src/a.rs", Mutation::Edit, &Feedback::default()).is_none());
        assert_eq!(
            new_antipatterns(None, FOUR_NEW, "rust", "src/new.rs"),
            4,
            "a new file: all of it is new"
        );
        assert_eq!(
            new_antipatterns(Some(CLEAN), FOUR_NEW, "rust", "tests/a_test.rs"),
            0,
            "test files never count"
        );
    }

    /// S28 — the header counts ISSUES: signals are shown after `ℹ` and never counted, and
    /// what the budget elided is declared (N = listed + elided).
    #[test]
    fn issue_count_matches_listed_items() {
        let a = |s: &str| s.to_string();
        let out = render_feedback(
            "post-write",
            &[a("A"), a("B")],
            0,
            &[a("⚙ health-delta: first-observation new=1.00")],
        )
        .expect("rendered");
        assert!(out.starts_with("post-write: 2 issue(s) | A | B"), "{out}");
        let (issues_part, signals_part) =
            out.split_once(" | \u{2139} ").expect("signals are marked");
        assert_eq!(
            issues_part.split(" | ").count() - 1,
            2,
            "N equals the listed issues: {out}"
        );
        assert!(signals_part.contains("health-delta"), "{out}");
        let only = render_feedback(
            "post-write",
            &[],
            0,
            &[a("⚙ health-delta: first-observation new=1.00")],
        )
        .expect("rendered");
        assert!(
            !only.contains("issue(s)"),
            "a signal alone is not an issue: {only}"
        );
        let elided =
            render_feedback("post-edit verification", &[a("A")], 2, &[]).expect("rendered");
        assert!(
            elided.contains("3 issue(s)") && elided.contains("+2 elided"),
            "{elided}"
        );
        assert_eq!(
            render_feedback("post-write", &[], 0, &[]),
            None,
            "nothing renders nothing"
        );
    }

    #[test]
    fn content_before_edit_reverses_edit_and_multiedit() {
        let after = "fn a() {\n    y();\n}\n";
        let edit = serde_json::json!({"old_string": "    x();", "new_string": "    y();"});
        assert_eq!(
            content_before_edit(after, &edit).as_deref(),
            Some("fn a() {\n    x();\n}\n")
        );
        let multi = serde_json::json!({"edits": [
            {"old_string": "a", "new_string": "b"},
            {"old_string": "b()", "new_string": "c()"}
        ]});
        assert_eq!(
            content_before_edit("fn c() {}", &multi).as_deref(),
            Some("fn a() {}"),
            "reversed in reverse order"
        );
        let all = serde_json::json!({"old_string": "x", "new_string": "z", "replace_all": true});
        assert_eq!(content_before_edit("z z", &all).as_deref(), Some("x x"));
        let deletion = serde_json::json!({"old_string": "gone", "new_string": ""});
        assert_eq!(
            content_before_edit("rest", &deletion),
            None,
            "a pure deletion cannot be located"
        );
        assert_eq!(
            content_before_edit("abc", &serde_json::json!({})),
            None,
            "unknown shape"
        );
    }

    #[test]
    fn fit_budget_keeps_what_fits_in_order() {
        let mut items = vec!["a".repeat(10), "b".repeat(100), "c".repeat(5)];
        let used = fit_budget(&mut items, 25);
        assert_eq!(
            items,
            vec!["a".repeat(10), "c".repeat(5)],
            "the big one is dropped, the small later one kept"
        );
        assert_eq!(used, 13 + 8);
    }
}
