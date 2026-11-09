//! What a hook ANSWER decided, read once from the JSON it prints.
//!
//! A hook answers Claude Code with a JSON object on stdout. The answer is produced typed
//! (`touring_hooks_core::hook_response::HookResponse`), but several readers only ever see
//! the text: the trace of the hook runtime (the daemon's stdout crosses a process), the
//! tool-channel ledger and the injection policy (crates below `touring-hooks-core`), the
//! exit-code lowering of a block. Each used to re-parse the JSON and spell the field names
//! on its own, and the copies disagreed (`allow` + `updatedInput` was an `allow` in the
//! trace and a rewrite everywhere else; a PermissionRequest denial was `silent`).
//!
//! [`HookWire::parse`] is the one reading, in Claude Code's precedence, and the field names
//! are the constants below. Malformed or non-object text is `None`: no reader ever turns an
//! unreadable answer into a denial (fail-open, as every reader already was).

use serde_json::Value;

/// `hookSpecificOutput` — the event-specific envelope.
pub const HOOK_SPECIFIC_OUTPUT: &str = "hookSpecificOutput";
/// `hookSpecificOutput.permissionDecision` — `allow` | `deny` | `ask` | `defer`.
pub const PERMISSION_DECISION: &str = "permissionDecision";
/// `hookSpecificOutput.permissionDecisionReason`.
pub const PERMISSION_DECISION_REASON: &str = "permissionDecisionReason";
/// Top-level `decision` (`block`) — and, for a PermissionRequest, the object
/// `hookSpecificOutput.decision` carrying `behavior`.
pub const DECISION: &str = "decision";
/// Top-level `reason` of a `decision: "block"`.
pub const REASON: &str = "reason";
/// Top-level `continue: false` halts the session.
pub const CONTINUE: &str = "continue";
/// Top-level `stopReason` of a halt.
pub const STOP_REASON: &str = "stopReason";
/// `hookSpecificOutput.additionalContext`.
pub const ADDITIONAL_CONTEXT: &str = "additionalContext";
/// `hookSpecificOutput.updatedInput` — the tool input rewritten.
pub const UPDATED_INPUT: &str = "updatedInput";
/// Top-level `systemMessage`.
pub const SYSTEM_MESSAGE: &str = "systemMessage";
/// Top-level `suppressOutput`.
pub const SUPPRESS_OUTPUT: &str = "suppressOutput";

/// What an answer decided, in Claude Code's precedence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// `permissionDecision: "deny"`, or a PermissionRequest `behavior: "deny"`.
    Deny,
    /// `permissionDecision: "allow"` (or `behavior: "allow"`) without a rewritten input.
    Allow,
    /// `permissionDecision: "ask"`.
    Ask,
    /// `permissionDecision: "defer"`.
    Defer,
    /// Top-level `decision: "block"`.
    Block,
    /// Top-level `continue: false`.
    Halt,
    /// `updatedInput` — the tool runs with a rewritten input (with or without an explicit
    /// `permissionDecision: "allow"`).
    Rewrite,
    /// Only `additionalContext` (not blank).
    Context,
    /// Nothing decided (`{}`, or only blank context).
    Silent,
}

impl Decision {
    /// The label replays and KPIs group by.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Deny => "deny",
            Self::Allow => "allow",
            Self::Ask => "ask",
            Self::Defer => "defer",
            Self::Block => "block",
            Self::Halt => "halt",
            Self::Rewrite => "rewrite",
            Self::Context => "context",
            Self::Silent => "silent",
        }
    }
}

/// A hook answer, read once.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HookWire {
    permission_decision: Option<String>,
    permission_reason: Option<String>,
    request_behavior: Option<String>,
    top_decision: Option<String>,
    top_reason: Option<String>,
    halts: bool,
    additional_context: Option<String>,
    has_updated_input: bool,
}

impl HookWire {
    /// Read `text` (an answer as printed). `None` when it is not a JSON object — never a
    /// denial.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let value: Value = serde_json::from_str(text.trim()).ok()?;
        value.as_object()?;
        let hso = value.get(HOOK_SPECIFIC_OUTPUT);
        let text_at = |obj: Option<&Value>, key: &str| {
            obj.and_then(|o| o.get(key))
                .and_then(Value::as_str)
                .map(str::to_owned)
        };
        Some(Self {
            permission_decision: text_at(hso, PERMISSION_DECISION),
            permission_reason: text_at(hso, PERMISSION_DECISION_REASON),
            request_behavior: text_at(hso.and_then(|h| h.get(DECISION)), "behavior"),
            top_decision: text_at(Some(&value), DECISION),
            top_reason: text_at(Some(&value), REASON),
            halts: value.get(CONTINUE).and_then(Value::as_bool) == Some(false),
            additional_context: text_at(hso, ADDITIONAL_CONTEXT),
            has_updated_input: hso.and_then(|h| h.get(UPDATED_INPUT)).is_some(),
        })
    }

    /// The decision, in Claude Code's precedence: a permission decision (an `allow` that
    /// rewrites the input is a rewrite), a PermissionRequest behavior, a top-level block, a
    /// halt, a rewrite, plain context — else silent.
    #[must_use]
    pub fn decision(&self) -> Decision {
        self.permission()
            .or_else(|| self.request())
            .unwrap_or_else(|| self.top_level())
    }

    /// `permissionDecision`, when it names one of the four.
    fn permission(&self) -> Option<Decision> {
        Some(match self.permission_decision.as_deref()? {
            "deny" => Decision::Deny,
            "allow" if self.has_updated_input => Decision::Rewrite,
            "allow" => Decision::Allow,
            "ask" => Decision::Ask,
            "defer" => Decision::Defer,
            _ => return None,
        })
    }

    /// A PermissionRequest's `hookSpecificOutput.decision.behavior`.
    fn request(&self) -> Option<Decision> {
        Some(match self.request_behavior.as_deref()? {
            "deny" => Decision::Deny,
            "allow" => Decision::Allow,
            _ => return None,
        })
    }

    /// Without a permission decision: a block, a halt, a rewrite, context, or nothing.
    fn top_level(&self) -> Decision {
        if self.top_decision.as_deref() == Some("block") {
            return Decision::Block;
        }
        if self.halts {
            return Decision::Halt;
        }
        if self.has_updated_input {
            return Decision::Rewrite;
        }
        match self.additional_context.as_deref() {
            Some(context) if !context.trim().is_empty() => Decision::Context,
            _ => Decision::Silent,
        }
    }

    /// The reason a denial gives: `permissionDecisionReason` of a deny, `reason` of a
    /// top-level block; `None` when the answer denies nothing.
    #[must_use]
    pub fn denial_reason(&self) -> Option<&str> {
        match self.decision() {
            Decision::Deny => Some(self.permission_reason.as_deref().unwrap_or_default()),
            Decision::Block => Some(self.top_reason.as_deref().unwrap_or_default()),
            _ => None,
        }
    }

    /// The top-level `decision: "block"` and its `reason`, whatever else the answer holds —
    /// what an exit-code-only event (Stop, SubagentStop…) can carry.
    #[must_use]
    pub fn top_level_block(&self) -> Option<Option<&str>> {
        (self.top_decision.as_deref() == Some("block")).then_some(self.top_reason.as_deref())
    }

    /// The text the answer speaks with: the permission reason, else the top-level reason,
    /// else the context (where a gate tag leads).
    #[must_use]
    pub fn said(&self) -> &str {
        self.permission_reason
            .as_deref()
            .or(self.top_reason.as_deref())
            .or(self.additional_context.as_deref())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decided(text: &str) -> Option<&'static str> {
        HookWire::parse(text).map(|wire| wire.decision().as_str())
    }

    #[test]
    fn every_shape_claude_code_reads_has_one_decision() {
        let cases = [
            (r#"{}"#, "silent"),
            (
                r#"{"hookSpecificOutput":{"additionalContext":"  "}}"#,
                "silent",
            ),
            (
                r#"{"hookSpecificOutput":{"additionalContext":"[G7] x"}}"#,
                "context",
            ),
            (
                r#"{"hookSpecificOutput":{"permissionDecision":"deny","permissionDecisionReason":"r"}}"#,
                "deny",
            ),
            (
                r#"{"hookSpecificOutput":{"permissionDecision":"ask"}}"#,
                "ask",
            ),
            (
                r#"{"hookSpecificOutput":{"permissionDecision":"defer"}}"#,
                "defer",
            ),
            (
                r#"{"hookSpecificOutput":{"permissionDecision":"allow"}}"#,
                "allow",
            ),
            (r#"{"decision":"block","reason":"r"}"#, "block"),
            (r#"{"continue":false,"stopReason":"s"}"#, "halt"),
            (
                r#"{"hookSpecificOutput":{"updatedInput":{"command":"ls"}}}"#,
                "rewrite",
            ),
            // The precedence: a deny wins over a top-level block on the same answer.
            (
                r#"{"decision":"block","hookSpecificOutput":{"permissionDecision":"deny"}}"#,
                "deny",
            ),
        ];
        for (text, want) in cases {
            assert_eq!(decided(text), Some(want), "{text}");
        }
    }

    /// The readers disagreed here before there was one reading: the trace called an
    /// explicit `allow` with a rewritten input an `allow`, and read a PermissionRequest
    /// denial as silent.
    #[test]
    fn an_allow_that_rewrites_is_a_rewrite_and_a_request_denial_is_a_denial() {
        assert_eq!(
            decided(
                r#"{"hookSpecificOutput":{"permissionDecision":"allow","updatedInput":{"command":"touring run"}}}"#
            ),
            Some("rewrite")
        );
        assert_eq!(
            decided(
                r#"{"hookSpecificOutput":{"hookEventName":"PermissionRequest","decision":{"behavior":"deny","message":"m"}}}"#
            ),
            Some("deny")
        );
    }

    #[test]
    fn unreadable_text_is_none_never_a_denial() {
        for text in ["", "not json", "[1,2]", "\"deny\"", "{\"decision\":"] {
            assert_eq!(HookWire::parse(text), None, "{text:?}");
        }
    }

    #[test]
    fn denial_reason_and_top_level_block() {
        let deny = HookWire::parse(
            r#"{"hookSpecificOutput":{"permissionDecision":"deny","permissionDecisionReason":"[G9 x] y"}}"#,
        )
        .expect("object");
        assert_eq!(deny.denial_reason(), Some("[G9 x] y"));
        assert_eq!(deny.top_level_block(), None);
        let block = HookWire::parse(r#"{"decision":"block","reason":"r"}"#).expect("object");
        assert_eq!(block.denial_reason(), Some("r"));
        assert_eq!(block.top_level_block(), Some(Some("r")));
        let ask = HookWire::parse(r#"{"hookSpecificOutput":{"permissionDecision":"ask"}}"#)
            .expect("object");
        assert_eq!(ask.denial_reason(), None);
    }
}
