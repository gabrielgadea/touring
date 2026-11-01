//! The "So what?" invariant as one executable predicate (I-4, 31/10/2026).
//!
//! `pre_read.rs:1-7` declares the rule every injector of the harness signs:
//! *SILENCE IS THE DEFAULT* — inject only what is actionable, non-derivable and
//! specific. It was declared in five places (`pre_read`, `pre_bash`, `pre_edit`,
//! `instructions_loaded`, `pre_edit_prevention`) and tested on one path, while
//! the measured population broke it in ~34 % of the bytes
//! (`docs/plans/2026-10-26-injecao-contexto-hooks/IMPACTO-POR-HOOK.md` §4.1).
//! A rule declared five times and executed nowhere is the "predicate written
//! twice never meets" defect; this module is the single place it runs.
//!
//! What a PAYLOAD can prove about itself, and therefore what this predicate
//! judges:
//!
//! * [`Violation::Empty`] — nothing beyond tags: `[generic]`, `[gabrielgadea]`,
//!   a bare banner. No proposition, no path, no symbol: the model cannot act.
//! * [`Violation::NoTarget`] — prose, but no NAMED target: no path, no symbol,
//!   no command, no flag. "Similar command failed recently (similarity=68%)" was
//!   1.676 blocks of this kind: a number about nothing nameable.
//!
//! What a payload CANNOT prove, and is therefore measured elsewhere: whether the
//! model acts on it (adherence — `context_budget.injection_follow_ratio` and
//! `nudge_follow_ratio`). A sermon that names its target passes here and fails
//! there; both numbers are needed and neither stands in for the other.
//!
//! The predicate is pure and does not cut anything: deciding to SILENCE what
//! violates it is the injection default (decision 3.3 of the pending list), not
//! a side effect of measuring.

/// Why a block fails the payload half of the "So what?" test.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Violation {
    /// Nothing but tags and punctuation — fewer than [`MIN_CONTENT_CHARS`]
    /// alphanumeric characters once every `[...]` tag is removed.
    Empty,
    /// Content, but no named target the model could act on.
    NoTarget,
}

/// Below this many alphanumeric characters (tags removed) a block says nothing.
pub const MIN_CONTENT_CHARS: usize = 8;

/// File extensions short enough to be told apart from prose (`e.g.` is not a
/// path; `a.rs` is).
const PATH_EXTS: &[&str] = &[
    "rs", "py", "ts", "tsx", "js", "jsx", "md", "json", "toml", "yaml", "yml", "sh", "go", "c",
    "h", "cpp", "hpp", "java", "kt", "swift", "html", "css", "sql", "txt", "lock", "jsonl", "db",
];

/// Judge one injected block. `None` = it passes the payload test.
#[must_use]
pub fn violation(text: &str) -> Option<Violation> {
    // Tags are removed only to judge EMPTINESS. The target search reads the raw
    // text: a bracket also carries lists of files (`3 files import this: [mod.rs,
    // kpi.rs]`), and stripping it threw away exactly the target.
    let content = strip_tags(text);
    if content.chars().filter(|c| c.is_alphanumeric()).count() < MIN_CONTENT_CHARS {
        return Some(Violation::Empty);
    }
    if names_a_target(text) {
        None
    } else {
        Some(Violation::NoTarget)
    }
}

/// The text with every `[...]` tag removed (one level; an unclosed `[` is kept).
fn strip_tags(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find('[') {
        let Some(close) = rest[open..].find(']') else {
            break;
        };
        out.push_str(&rest[..open]);
        rest = &rest[open + close + 1..];
    }
    out.push_str(rest);
    out
}

/// Does the content name something the model can act on: a path, a symbol, a
/// command or a flag?
fn names_a_target(content: &str) -> bool {
    if content.contains('`') || content.contains("::") {
        return true;
    }
    content
        .split(|c: char| c.is_whitespace() || matches!(c, ',' | ';' | '(' | ')' | '"' | '\''))
        .any(is_target_token)
}

fn is_target_token(tok: &str) -> bool {
    let tok = tok.trim_end_matches([':', '.', '!', '?']);
    tok.len() >= 2 && (is_flag(tok) || is_path(tok) || is_symbol(tok))
}

/// A flag (`--timeout-ms`, `-j`): something to type.
fn is_flag(tok: &str) -> bool {
    tok.strip_prefix('-')
        .is_some_and(|rest| rest.starts_with(|c: char| c.is_ascii_alphabetic() || c == '-'))
}

/// A path: a separator between two non-empty parts, or a known extension.
fn is_path(tok: &str) -> bool {
    if tok.contains('/') && tok.split('/').filter(|p| !p.is_empty()).count() >= 2 {
        return true;
    }
    tok.rsplit_once('.').is_some_and(|(stem, ext)| {
        !stem.is_empty() && PATH_EXTS.contains(&ext.to_ascii_lowercase().as_str())
    })
}

/// A symbol: snake_case, or CamelCase with an inner capital — never a plain word.
fn is_symbol(tok: &str) -> bool {
    if !tok.chars().all(|c| c.is_alphanumeric() || c == '_') {
        return false;
    }
    let snake = tok.contains('_') && tok.chars().any(char::is_alphabetic);
    let camel = tok.chars().next().is_some_and(char::is_uppercase)
        && tok.chars().skip(1).any(char::is_uppercase)
        && tok.chars().any(char::is_lowercase);
    snake || camel
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bare_tag_is_empty() {
        for t in [
            "[generic]",
            "[gabrielgadea]",
            "  [generic]\n",
            "[a] [b] ---",
            "",
        ] {
            assert_eq!(violation(t), Some(Violation::Empty), "{t:?}");
        }
    }

    #[test]
    fn a_number_about_nothing_nameable_has_no_target() {
        // The 1.676-block `similar-fail` payload of §5: a percentage, no command.
        assert_eq!(
            violation("Similar command failed recently (similarity=68%)"),
            Some(Violation::NoTarget)
        );
        assert_eq!(
            violation("[note] remember to be careful with this kind of change"),
            Some(Violation::NoTarget)
        );
    }

    #[test]
    fn a_named_target_passes() {
        for t in [
            "HOTSPOT: crates/touring-cli/src/cli/kpi.rs blast 12",
            "3 files import this: [mod.rs, kpi.rs]",
            "MUST touring ast meta a.rs --depth summary -j",
            "`level_from_env` has 2 other call sites",
            "touring_foundation::cila changed",
            "quality: CC>10: [capture(CC=18)] in GateMetricsSnapshot",
            "edit impacts: project_root -> parse [sequential]",
            "⚠ lições de erros passados: Bash `grep` failed: Exit code 143",
        ] {
            assert_eq!(violation(t), None, "{t:?}");
        }
    }

    #[test]
    fn plain_words_and_prose_punctuation_are_not_targets() {
        // `e.g.` and `etc.` are not paths; `Note` is not a symbol; `-` alone is not a flag.
        for tok in ["e.g.", "etc.", "Note", "-", "A", "Rust", "i.e", "100%"] {
            assert!(!is_target_token(tok), "{tok:?}");
        }
        for tok in [
            "a.rs",
            "--timeout-ms",
            "-j",
            "src/main.rs",
            "snake_case",
            "HookResponse",
        ] {
            assert!(is_target_token(tok), "{tok:?}");
        }
    }

    #[test]
    fn an_unclosed_bracket_keeps_its_text() {
        assert_eq!(strip_tags("[a] keep [this"), " keep [this");
    }
}
