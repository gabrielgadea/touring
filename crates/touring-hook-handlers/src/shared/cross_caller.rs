//! A2 (2026-09-02) — `CrossCallerLayer`: analogous call sites for an edit.
//!
//! Decision-matrix C08 (cross-caller compare) is the anti-bug the 2026-05-10
//! post-mortem paid 40 minutes for: caller A gets the fix, caller B is
//! forgotten. This layer fires when an Edit CHANGES a call — `total(a, b)`
//! becomes `total(a, b, c)` — and the symbol index knows other call sites of
//! the same callee: it names them, file:line, BEFORE the edit lands.
//!
//! v1 is deterministic (exact-name references from the symbol index). The
//! ANN "similar sites" route the strategy named for TIER-3 stacks on top once
//! an embedding index of code sites exists; the trigger (a changed call) and
//! the signal shape stay the same.
//!
//! Like S3/A3, the index lookup runs at construction (`for_edit`) because
//! pipeline layers are `'static` and cannot borrow the runtime.

use std::collections::{BTreeMap, BTreeSet};

use crate::shared::related_symbols::same_file;
use crate::shared::signal_pipeline::{ProposedChange, SignalContext, SignalLayer};

/// Score of a cross-caller signal: high — a forgotten analogous site is a
/// latent bug, not a style remark.
const CROSS_CALLER_SCORE: f32 = 0.8;

/// Callee names whose call text differs between `old` and `new`.
///
/// A call is `name(` … `)` with the name NOT introduced by a declaring keyword
/// (`fn`/`def`/`function`/`class`/…) and NOT a macro (`name!(`). Keywords
/// (`if`/`while`/`for`/`match`/`return`/…) and names shorter than 4 chars are
/// never callees. A name counts as changed when the SET of its call
/// expressions (whitespace-collapsed) differs — added, removed, or re-argued.
pub fn changed_callees(old: &str, new: &str) -> Vec<String> {
    let before = call_expressions(old);
    let after = call_expressions(new);
    before
        .keys()
        .chain(after.keys())
        .filter(|name| before.get(*name) != after.get(*name))
        .cloned()
        .collect::<BTreeSet<String>>()
        .into_iter()
        .collect()
}

/// Cross-caller signals for a proposed edit.
///
/// `lookup` receives one callee name and returns the `(file, line)` of every
/// CALL SITE the index knows for it; sites inside `rel_path` itself are
/// discarded (the edit is happening there). Never called when no call changed.
pub fn cross_caller_signals<F>(rel_path: &str, old: &str, new: &str, mut lookup: F) -> Vec<(f32, String)>
where
    F: FnMut(&str) -> Vec<(String, usize)>,
{
    let mut out = Vec::new();
    for name in changed_callees(old, new).into_iter().take(MAX_CALLEES) {
        let sites: Vec<(String, usize)> = lookup(&name)
            .into_iter()
            .filter(|(file, _)| !same_file(file, rel_path))
            .collect();
        if sites.is_empty() {
            continue;
        }
        let mut files: Vec<&str> = Vec::new();
        for (file, _) in &sites {
            if !files.contains(&file.as_str()) {
                files.push(file);
            }
        }
        let shown: Vec<String> = sites
            .iter()
            .take(MAX_SITES_SHOWN)
            .map(|(file, line)| format!("{file}:{line}"))
            .collect();
        let more = if sites.len() > MAX_SITES_SHOWN {
            format!(" (+{})", sites.len() - MAX_SITES_SHOWN)
        } else {
            String::new()
        };
        out.push((
            CROSS_CALLER_SCORE,
            format!(
                "[C08] `{name}` changes here and has {n} other call site{s} in {m} file{fs}: {list}{more} \
                 — the same change may apply there (cross-caller compare)",
                n = sites.len(),
                s = plural(sites.len()),
                m = files.len(),
                fs = plural(files.len()),
                list = shown.join(", "),
            ),
        ));
    }
    out
}

/// Changed callees looked up per edit — bounds the index cost of a big edit.
const MAX_CALLEES: usize = 5;
/// Sites named in one signal; the rest is a count.
const MAX_SITES_SHOWN: usize = 4;

fn plural(n: usize) -> &'static str {
    if n == 1 { "" } else { "s" }
}

/// Words that precede `(` without being a call.
const NOT_A_CALLEE: &[&str] = &[
    "if", "while", "for", "match", "return", "loop", "switch", "catch", "elif", "else", "in",
    "as", "use", "let", "mut", "await", "yield", "typeof", "not", "and", "or", "assert",
    "raise", "except", "with", "unsafe", "where", "impl", "pub", "const", "static",
];

/// Keywords whose FOLLOWING identifier is a declaration, not a call.
const DECLARING: &[&str] = &[
    "fn", "def", "function", "class", "struct", "impl", "trait", "enum", "interface", "type",
    "macro_rules", "mod",
];

/// `callee → { call expressions }` for `text`, whitespace removed inside each
/// expression so re-formatting is not a change.
fn call_expressions(text: &str) -> BTreeMap<String, BTreeSet<String>> {
    let mut out: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let bytes = text.as_bytes();
    let mut prev_word: Option<&str> = None;
    let mut i = 0usize;
    while i < bytes.len() {
        let c = bytes[i] as char;
        if !(c.is_ascii_alphabetic() || c == '_') {
            i += 1;
            continue;
        }
        let start = i;
        while i < bytes.len() && ((bytes[i] as char).is_ascii_alphanumeric() || bytes[i] == b'_') {
            i += 1;
        }
        let word = &text[start..i];
        let after = bytes.get(i).map(|b| *b as char);
        let is_call = after == Some('(')
            && word.len() >= 4
            && !NOT_A_CALLEE.contains(&word)
            && !prev_word.is_some_and(|p| DECLARING.contains(&p));
        if is_call && let Some(end) = matching_paren(bytes, i) {
            let expr: String = text[start..=end].chars().filter(|c| !c.is_whitespace()).collect();
            out.entry(word.to_string()).or_default().insert(expr);
        }
        prev_word = Some(word);
    }
    out
}

/// Index of the `)` closing the `(` at `open`; `None` when unbalanced.
fn matching_paren(bytes: &[u8], open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (i, b) in bytes.iter().enumerate().skip(open) {
        match b {
            b'(' => depth += 1,
            b')' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

/// The pipeline layer. Built per `pre_edit` call with the sites resolved.
pub struct CrossCallerLayer {
    signals: Vec<(f32, String)>,
}

impl CrossCallerLayer {
    /// Build the layer for a proposed EDIT of `rel_path` (`old` → `new`).
    pub fn for_edit<F>(rel_path: &str, old: &str, new: &str, lookup: F) -> Self
    where
        F: FnMut(&str) -> Vec<(String, usize)>,
    {
        Self {
            signals: cross_caller_signals(rel_path, old, new, lookup),
        }
    }

    /// `true` when the layer will emit nothing.
    pub fn is_empty(&self) -> bool {
        self.signals.is_empty()
    }
}

impl SignalLayer for CrossCallerLayer {
    fn name(&self) -> &'static str {
        "cross_caller"
    }

    fn enrich(&self, ctx: &SignalContext<'_>) -> Vec<(f32, String)> {
        if !matches!(ctx.proposed, Some(ProposedChange::Edit { .. })) {
            return Vec::new();
        }
        self.signals.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    fn sites_of_total(name: &str) -> Vec<(String, usize)> {
        if name == "total" {
            vec![
                ("crates/a/src/report.rs".to_string(), 12),
                ("crates/a/src/report.rs".to_string(), 30),
                ("crates/b/src/summary.rs".to_string(), 7),
            ]
        } else {
            Vec::new()
        }
    }

    #[test]
    fn changed_callees_sees_re_argued_added_and_removed_calls_only() {
        let old = "let t = total(a, b);\nlet k = keep(x);\nlet g = gone(1);\nif ready(x) { fn inner(q: u8) {} }\nprintln!(\"{t}\");\n";
        let new = "let t = total(a, b, c);\nlet k = keep( x );\nlet n = fresh(2);\nif ready(x) { fn inner(q: u8, r: u8) {} }\nprintln!(\"{t} {n}\");\n";
        let changed = changed_callees(old, new);
        assert_eq!(changed, vec!["fresh", "gone", "total"], "{changed:?}");
    }

    #[test]
    fn flags_a_changed_call_with_its_other_call_sites_grouped_by_file() {
        let old = "let t = total(a, b);";
        let new = "let t = total(a, b, c);";
        let signals = cross_caller_signals("crates/a/src/main.rs", old, new, sites_of_total);
        assert_eq!(signals.len(), 1, "{signals:?}");
        let (score, text) = &signals[0];
        assert!((*score - CROSS_CALLER_SCORE).abs() < f32::EPSILON);
        assert!(text.starts_with("[C08] `total`"), "{text}");
        assert!(text.contains("3 other call sites in 2 files"), "{text}");
        assert!(text.contains("crates/a/src/report.rs:12") && text.contains("crates/b/src/summary.rs:7"), "{text}");
    }

    #[test]
    fn silent_for_unchanged_calls_and_for_sites_only_in_the_edited_file() {
        let never = |name: &str| -> Vec<(String, usize)> { panic!("no call changed, got lookup for {name}") };
        assert!(cross_caller_signals("src/x.rs", "total(a, b)", "total(a, b)", never).is_empty());
        assert!(cross_caller_signals("src/x.rs", "fn total(a: u8) {}", "fn total(a: u8, b: u8) {}", never).is_empty());

        let seen: RefCell<Vec<String>> = RefCell::new(Vec::new());
        let signals = cross_caller_signals("crates/a/src/report.rs", "total(a)", "total(a, b)", |name| {
            seen.borrow_mut().push(name.to_string());
            vec![("crates/a/src/report.rs".to_string(), 30)]
        });
        assert!(signals.is_empty(), "sites in the edited file are the edit itself: {signals:?}");
        assert_eq!(*seen.borrow(), vec!["total".to_string()]);
    }

    #[test]
    fn layer_emits_only_for_a_proposed_edit() {
        let layer = CrossCallerLayer::for_edit("src/x.rs", "total(a)", "total(a, b)", sites_of_total);
        assert!(!layer.is_empty());
        assert_eq!(layer.name(), "cross_caller");
        let edit_ctx = SignalContext::new("src/x.rs", "")
            .with_hook("pre_edit")
            .with_tool_name("Edit")
            .with_proposed(ProposedChange::Edit {
                old_string: "total(a)",
                new_string: "total(a, b)",
            });
        assert_eq!(layer.enrich(&edit_ctx).len(), 1);
        let write_ctx = SignalContext::new("src/x.rs", "")
            .with_hook("pre_write")
            .with_tool_name("Write")
            .with_proposed(ProposedChange::Write { content: "total(a, b)" });
        assert!(layer.enrich(&write_ctx).is_empty());
    }
}
