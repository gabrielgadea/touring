//! A4 — PreToolUse `rewrite`: do the right thing FOR the agent, not to it.
//!
//! # Why this exists
//!
//! `~/.claude/rules/touring-4-pillars.md` records a measured lesson:
//! *"adoption does not emerge from availability; it must be actively
//! induced"*, and thesis ①: **affordance changes `U(a)=P·V−C(tokens)`;
//! persuasion does not** — with the observation that MUST nudges at confidence
//! 0.95 were ignored **in the very session that emitted them**.
//!
//! [`crate::cli_suggester`] is that persuasion: better text, injected closer,
//! still text the model can ignore. `deny` is the opposite extreme and costs a
//! governance decision (it removes capability). `rewrite` is the middle the
//! gortex `docs/agents.md` documents and this module implements: when — and
//! only when — a command has an **exact mirror** in the touring surface, the
//! hook swaps it via `hookSpecificOutput.updatedInput`. Nothing is persuaded
//! and nothing is blocked; the better call simply happens.
//!
//! # The equivalence bar (non-negotiable)
//!
//! A rewrite may only fire when the replacement produces **byte-identical
//! output**. Anything less is worse than persuasion: silently returning
//! something *different* from what was asked corrupts the agent's premises,
//! and it would do so invisibly.
//!
//! This bar is why the obvious candidate is NOT here. `cat <file>` →
//! `touring read <file>` looks like the flagship conversion, but `touring read`
//! returns an aggregated metadata report, not the file — a strict improvement
//! in tokens and a strict violation of equivalence. It stays advisory
//! (`enrich`) until a human decides that trade, because changing *what the
//! caller receives* is a policy decision, not a hook's call to make.
//!
//! Verified before shipping (2026-08-07):
//! `cmp <(NO_COLOR=1 touring ast highlight F) <(cat F)` → identical.
//!
//! # Modes
//!
//! | `TOURING_HOOK_MODE` | Behaviour |
//! |---|---|
//! | unset / `rewrite` | mirror rewrites fire; everything else stays advisory |
//! | `enrich` | never rewrites — the pre-2026-08-07 behaviour |
//! | anything else | treated as `enrich` (fail-safe: an unknown mode must not act) |
//!
//! `deny` is deliberately **not** implemented here: blocking the operator's own
//! tools is a governance decision, and the [`Mode`] enum leaves room for it
//! without pretending it exists.

use serde_json::{Value, json};

/// How the PreToolUse hook is allowed to intervene.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Context injection only — never alters the call.
    Enrich,
    /// Swap a command for its exact mirror; everything else stays advisory.
    Rewrite,
}

impl Mode {
    /// Resolve from `TOURING_HOOK_MODE`.
    ///
    /// Unknown values degrade to [`Mode::Enrich`]: a mode nobody recognises
    /// must not be granted the power to change commands.
    #[must_use]
    pub fn from_env() -> Self {
        match std::env::var("TOURING_HOOK_MODE").ok().as_deref() {
            None | Some("") | Some("rewrite") => Self::Rewrite,
            Some(_) => Self::Enrich,
        }
    }
}

/// A rewrite the hook is prepared to perform.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rewrite {
    /// The command that will actually run.
    pub command: String,
    /// One line explaining the swap, surfaced as `permissionDecisionReason`.
    pub reason: String,
}

/// Characters that make a shell command more than a single simple invocation.
///
/// Any of these means the command composes, redirects or expands, and the
/// mirror can no longer be proven equivalent by inspection. Conservative on
/// purpose: a missed rewrite costs nothing, a wrong one corrupts the answer.
const SHELL_METACHARACTERS: &[char] = &[
    '|', '>', '<', '&', ';', '$', '`', '(', ')', '{', '}', '*', '?', '[', ']', '\n', '\\', '~',
    '!', '#',
];

fn is_simple_command(cmd: &str) -> bool {
    !cmd.chars().any(|c| SHELL_METACHARACTERS.contains(&c))
}

/// Does this path look like a plain file argument (no quoting games)?
fn is_plain_path(arg: &str) -> bool {
    !arg.is_empty()
        && !arg.starts_with('-')
        && !arg.contains('\'')
        && !arg.contains('"')
        && !arg.contains(' ')
}

/// Compute the rewrite for a tool call, if one is provably safe.
///
/// Returns `None` — the overwhelmingly common answer — unless every condition
/// holds: the mode allows it, the tool is `Bash`, the command is a single
/// simple invocation, and the mirror is exact.
#[must_use]
pub fn rewrite_for(mode: Mode, tool_name: &str, tool_input: &Value) -> Option<Rewrite> {
    if mode != Mode::Rewrite || tool_name != "Bash" {
        return None;
    }
    let command = tool_input.get("command")?.as_str()?.trim();
    if command.is_empty() || !is_simple_command(command) {
        return None;
    }
    let parts: Vec<&str> = command.split_whitespace().collect();
    match parts.as_slice() {
        // `cat <file>` → `touring ast highlight <file>`.
        //
        // Byte-identical under NO_COLOR (verified), and it routes the read
        // through the daemon, so the access lands in the knowledge DB instead
        // of being invisible to the index. That observability IS the gain —
        // the same one gortex describes as *"observe calls like graph_stats,
        // not file reads"*.
        ["cat", path] if is_plain_path(path) => Some(Rewrite {
            command: format!("NO_COLOR=1 touring ast highlight {path}"),
            reason: format!(
                "mirror rewrite: byte-identical to `cat {path}`, and routes the \
                 read through the index instead of past it"
            ),
        }),
        _ => None,
    }
}

/// Build the PreToolUse response body for a rewrite.
///
/// `permissionDecision` stays `allow`: the call is not being questioned, only
/// spelled better. The reason travels so the swap is never silent — an
/// invisible rewrite would be the very opacity this whole line of work exists
/// to remove.
#[must_use]
pub fn rewrite_response(rw: &Rewrite, context: &str) -> Value {
    let mut out = json!({
        "hookEventName": "PreToolUse",
        "permissionDecision": "allow",
        "permissionDecisionReason": rw.reason,
        "updatedInput": { "command": rw.command },
    });
    if !context.is_empty()
        && let Some(obj) = out.as_object_mut()
    {
        obj.insert("additionalContext".into(), json!(context));
    }
    json!({ "hookSpecificOutput": out })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn bash(cmd: &str) -> Value {
        json!({ "command": cmd })
    }

    #[test]
    fn a_bare_cat_of_a_plain_path_is_mirrored() {
        let rw = rewrite_for(Mode::Rewrite, "Bash", &bash("cat src/lib.rs")).expect("rewrite");
        assert_eq!(rw.command, "NO_COLOR=1 touring ast highlight src/lib.rs");
        assert!(rw.reason.contains("byte-identical"));
    }

    #[test]
    fn anything_composed_stays_advisory() {
        // Each of these could change meaning under rewrite, so none may fire.
        for cmd in [
            "cat a.rs | head -20",
            "cat a.rs > b.rs",
            "cat a.rs && echo done",
            "cat $FILE",
            "cat *.rs",
            "cat a.rs b.rs",
            "cat -n a.rs",
            "cat ~/notes.txt",
            "cat 'my file.rs'",
        ] {
            assert!(
                rewrite_for(Mode::Rewrite, "Bash", &bash(cmd)).is_none(),
                "must not rewrite: {cmd}"
            );
        }
    }

    #[test]
    fn enrich_mode_never_rewrites() {
        assert!(rewrite_for(Mode::Enrich, "Bash", &bash("cat src/lib.rs")).is_none());
    }

    #[test]
    fn only_bash_is_eligible() {
        assert!(
            rewrite_for(Mode::Rewrite, "Read", &json!({ "file_path": "src/lib.rs" })).is_none()
        );
        assert!(rewrite_for(Mode::Rewrite, "Grep", &bash("cat src/lib.rs")).is_none());
    }

    #[test]
    fn an_unknown_mode_degrades_to_enrich_never_to_rewrite() {
        // A mode nobody recognises must not be handed the power to change
        // commands — fail-safe, not fail-open.
        for raw in ["deny", "suppress", "REWRITE", "yes", "1"] {
            unsafe { std::env::set_var("TOURING_HOOK_MODE", raw) };
            assert_eq!(Mode::from_env(), Mode::Enrich, "mode {raw:?}");
        }
        unsafe { std::env::set_var("TOURING_HOOK_MODE", "rewrite") };
        assert_eq!(Mode::from_env(), Mode::Rewrite);
        unsafe { std::env::remove_var("TOURING_HOOK_MODE") };
        assert_eq!(Mode::from_env(), Mode::Rewrite, "default is rewrite");
    }

    #[test]
    fn the_response_carries_the_swap_and_says_why() {
        let rw = rewrite_for(Mode::Rewrite, "Bash", &bash("cat src/lib.rs")).expect("rewrite");
        let body = rewrite_response(&rw, "some context");
        let hso = &body["hookSpecificOutput"];
        assert_eq!(hso["hookEventName"], "PreToolUse");
        assert_eq!(hso["permissionDecision"], "allow");
        assert_eq!(
            hso["updatedInput"]["command"],
            "NO_COLOR=1 touring ast highlight src/lib.rs"
        );
        assert!(
            hso["permissionDecisionReason"]
                .as_str()
                .unwrap_or_default()
                .contains("mirror rewrite"),
            "a rewrite must never be silent"
        );
        assert_eq!(hso["additionalContext"], "some context");
    }

    #[test]
    fn an_empty_context_is_omitted_not_emitted_blank() {
        let rw = rewrite_for(Mode::Rewrite, "Bash", &bash("cat src/lib.rs")).expect("rewrite");
        let body = rewrite_response(&rw, "");
        assert!(body["hookSpecificOutput"].get("additionalContext").is_none());
    }

    #[test]
    fn metadata_first_conversions_are_deliberately_absent() {
        // `touring read` returns a report, not the file. It is the higher-value
        // conversion AND an equivalence violation, so it must not appear here.
        let rw = rewrite_for(Mode::Rewrite, "Bash", &bash("cat src/lib.rs")).expect("rewrite");
        assert!(
            !rw.command.contains("touring read"),
            "a non-mirror must never be rewritten silently"
        );
    }
}
