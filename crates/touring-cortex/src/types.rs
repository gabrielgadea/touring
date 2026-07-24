//! Core types for the Cortex hook execution engine.
//!
//! Defines: HookEvent, Decision, HandlerResult, CortexOutput, HookSpecificOutput.

use serde::Serialize;

/// Hook event types matching Claude Code's hook system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HookEvent {
    /// Fired when a new session begins.
    SessionStart,
    /// Fired when the user submits a prompt.
    UserPromptSubmit,
    /// Fired before a tool is invoked.
    PreToolUse,
    /// Fired after a tool completes.
    PostToolUse,
    /// Fired before the conversation context is compacted.
    PreCompact,
    /// Fired when the assistant turn stops.
    Stop,
    /// Fired when a subagent begins execution.
    SubagentStart,
    /// Fired when a subagent finishes execution.
    SubagentStop,
    /// Fired when a delegated task is marked completed.
    TaskCompleted,
    /// Fired when a teammate agent goes idle.
    TeammateIdle,
    /// Tool use that resulted in a failure (exit code != 0, error output, etc.).
    PostToolUseFailure,
    /// Session stop triggered by a failure (rate limit, server error, max tokens, auth).
    StopFailure,
    /// Clean session end — final persistence opportunity.
    SessionEnd,
    /// Post-compact — re-inject critical context after compaction.
    PostCompact,
    /// Config change — settings file modified.
    ConfigChange,
    /// Worktree created — isolated working copy set up.
    WorktreeCreate,
    /// Worktree removed — cleanup after worktree deletion.
    WorktreeRemove,
    /// Instructions loaded — CLAUDE.md/rules files loaded.
    InstructionsLoaded,
    /// Permission request — Claude Code asking user for tool permission.
    /// Auto-approve known-safe patterns via handler logic.
    PermissionRequest,
    /// Notification — when Claude Code sends a notification to the user.
    Notification,
    /// Setup — repo setup hooks, triggered during initial project setup.
    Setup,
    /// Elicitation — when an MCP server requests user input.
    Elicitation,
    /// ElicitationResult — after user responds to an elicitation prompt.
    ElicitationResult,
    /// CwdChanged — when the working directory changes.
    CwdChanged,
    /// FileChanged — when a watched file changes on disk.
    FileChanged,
}

impl HookEvent {
    /// Parse event name from CLI argument string.
    ///
    /// Accepts kebab-case, snake_case, and camelCase forms.
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().replace('_', "-").as_str() {
            "session-start" | "sessionstart" => Some(Self::SessionStart),
            "user-prompt-submit" | "userpromptsubmit" | "prompt-submit" | "prompt" => {
                Some(Self::UserPromptSubmit)
            }
            "pre-tool-use" | "pretooluse" | "pre-tool" | "pre-read" | "pre-bash" | "pre-edit" => {
                Some(Self::PreToolUse)
            }
            "post-tool-use" | "posttooluse" | "post-tool" | "post-read" | "post-bash"
            | "post-edit" => Some(Self::PostToolUse),
            "pre-compact" | "precompact" => Some(Self::PreCompact),
            "stop" | "session-stop" => Some(Self::Stop),
            "subagent-start" | "subagentstart" => Some(Self::SubagentStart),
            "subagent-stop" | "subagentstop" => Some(Self::SubagentStop),
            "task-completed" | "taskcompleted" => Some(Self::TaskCompleted),
            "teammate-idle" | "teammateidle" => Some(Self::TeammateIdle),
            "post-tool-failure"
            | "posttooluse-failure"
            | "post-tool-use-failure"
            | "posttoolusefailure" => Some(Self::PostToolUseFailure),
            "stop-failure" | "stopfailure" => Some(Self::StopFailure),
            "session-end" | "sessionend" => Some(Self::SessionEnd),
            "post-compact" | "postcompact" => Some(Self::PostCompact),
            "config-change" | "configchange" => Some(Self::ConfigChange),
            "worktree-create" | "worktreecreate" => Some(Self::WorktreeCreate),
            "worktree-remove" | "worktreeremove" => Some(Self::WorktreeRemove),
            "instructions-loaded" | "instructionsloaded" => Some(Self::InstructionsLoaded),
            "permission-request" | "permissionrequest" => Some(Self::PermissionRequest),
            "notification" => Some(Self::Notification),
            "setup" => Some(Self::Setup),
            "elicitation" => Some(Self::Elicitation),
            "elicitation-result" | "elicitationresult" => Some(Self::ElicitationResult),
            "cwd-changed" | "cwdchanged" => Some(Self::CwdChanged),
            "file-changed" | "filechanged" => Some(Self::FileChanged),
            _ => None,
        }
    }

    /// Get the canonical event name string.
    // EC64: test-only helper — cargo check does not compile #[cfg(test)] modules,
    // so the callers at types.rs:490-537 are invisible to the dead_code lint.
    // Annotation is intentional: method is kept for unit test assertions.
    #[allow(dead_code)]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::SessionStart => "SessionStart",
            Self::UserPromptSubmit => "UserPromptSubmit",
            Self::PreToolUse => "PreToolUse",
            Self::PostToolUse => "PostToolUse",
            Self::PreCompact => "PreCompact",
            Self::Stop => "Stop",
            Self::SubagentStart => "SubagentStart",
            Self::SubagentStop => "SubagentStop",
            Self::TaskCompleted => "TaskCompleted",
            Self::TeammateIdle => "TeammateIdle",
            Self::PostToolUseFailure => "PostToolUseFailure",
            Self::StopFailure => "StopFailure",
            Self::SessionEnd => "SessionEnd",
            Self::PostCompact => "PostCompact",
            Self::ConfigChange => "ConfigChange",
            Self::WorktreeCreate => "WorktreeCreate",
            Self::WorktreeRemove => "WorktreeRemove",
            Self::InstructionsLoaded => "InstructionsLoaded",
            Self::PermissionRequest => "PermissionRequest",
            Self::Notification => "Notification",
            Self::Setup => "Setup",
            Self::Elicitation => "Elicitation",
            Self::ElicitationResult => "ElicitationResult",
            Self::CwdChanged => "CwdChanged",
            Self::FileChanged => "FileChanged",
        }
    }

    /// Get the hookEventName for Claude Code's hookSpecificOutput schema.
    ///
    /// Claude Code v2.1.81 ONLY accepts hookSpecificOutput for these 3 events:
    /// PreToolUse, PostToolUse, UserPromptSubmit.
    /// All other events must use top-level `systemMessage` for context injection.
    /// Previous comment was wrong — "Stop" with hookSpecificOutput causes
    /// "JSON validation failed: Invalid input" error.
    pub fn hook_output_name(&self) -> Option<&'static str> {
        match self {
            Self::PreToolUse => Some("PreToolUse"),
            Self::PostToolUse => Some("PostToolUse"),
            Self::UserPromptSubmit => Some("UserPromptSubmit"),
            _ => None, // Use systemMessage for all other events
        }
    }
}

/// Decision from a handler.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    /// Proceed normally.
    Allow,
    /// Block with reason.
    Block(String),
    /// Handler has nothing to say.
    Skip,
}

impl Decision {
    /// Returns the strictness level for escalation comparison.
    /// Block > Allow > Skip.
    fn strictness(&self) -> u8 {
        match self {
            Self::Skip => 0,
            Self::Allow => 1,
            Self::Block(_) => 2,
        }
    }

    /// Escalate: return the more restrictive of two decisions.
    pub fn escalate(self, other: Decision) -> Decision {
        if other.strictness() > self.strictness() {
            other
        } else {
            self
        }
    }
}

/// Result from a single handler execution.
#[derive(Debug, Clone)]
pub struct HandlerResult {
    /// The decision this handler made.
    pub decision: Decision,
    /// Lines to inject into additionalContext.
    pub context_lines: Vec<String>,
    /// Handler-specific metrics.
    pub metrics: serde_json::Value,
    /// Name of the handler that produced this result.
    pub handler_name: String,
    /// How long the handler took in milliseconds.
    pub duration_ms: f64,
}

impl HandlerResult {
    /// Create a Skip result (handler has nothing to say).
    pub fn skip(handler_name: &str) -> Self {
        Self {
            decision: Decision::Skip,
            context_lines: Vec::new(),
            metrics: serde_json::Value::Null,
            handler_name: handler_name.to_string(),
            duration_ms: 0.0,
        }
    }

    /// Create an Allow result with optional context.
    pub fn allow(handler_name: &str, context: Option<String>) -> Self {
        let context_lines = context.into_iter().collect();
        Self {
            decision: Decision::Allow,
            context_lines,
            metrics: serde_json::Value::Null,
            handler_name: handler_name.to_string(),
            duration_ms: 0.0,
        }
    }

    /// Create a Block result.
    pub fn block(handler_name: &str, reason: String) -> Self {
        Self {
            decision: Decision::Block(reason),
            context_lines: Vec::new(),
            metrics: serde_json::Value::Null,
            handler_name: handler_name.to_string(),
            duration_ms: 0.0,
        }
    }
}

/// The consolidated output from the entire Cortex pipeline.
#[derive(Debug, Clone, Serialize)]
pub struct CortexOutput {
    /// Hook-specific output for Claude Code.
    /// Only valid for PreToolUse, UserPromptSubmit, PostToolUse events.
    #[serde(rename = "hookSpecificOutput")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hook_specific_output: Option<HookSpecificOutput>,

    /// Whether to suppress the hook's own output.
    #[serde(rename = "suppressOutput")]
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub suppress_output: bool,

    /// Overall decision: "approve" or "block".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decision: Option<String>,

    /// Reason for blocking (if decision is "block").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,

    /// System message — used for events that don't support hookSpecificOutput
    /// (PreCompact, Stop, SessionStart, SubagentStart/Stop, TeammateIdle, TaskCompleted).
    /// Claude Code accepts this at the top level for any event type.
    #[serde(rename = "systemMessage")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_message: Option<String>,

    /// Aggregated metrics from all handlers.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metrics: Option<serde_json::Value>,
}

/// Hook-specific output matching Claude Code's expected format.
#[derive(Debug, Clone, Serialize)]
pub struct HookSpecificOutput {
    /// The hook event name.
    #[serde(rename = "hookEventName")]
    pub hook_event_name: String,

    /// Accumulated context from all handlers.
    #[serde(rename = "additionalContext")]
    pub additional_context: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hook_event_parsing() {
        assert_eq!(
            HookEvent::parse("session-start"),
            Some(HookEvent::SessionStart)
        );
        assert_eq!(HookEvent::parse("pre-tool"), Some(HookEvent::PreToolUse));
        assert_eq!(
            HookEvent::parse("pre-tool-use"),
            Some(HookEvent::PreToolUse)
        );
        assert_eq!(HookEvent::parse("post-tool"), Some(HookEvent::PostToolUse));
        assert_eq!(
            HookEvent::parse("post-tool-use"),
            Some(HookEvent::PostToolUse)
        );
        assert_eq!(
            HookEvent::parse("user-prompt-submit"),
            Some(HookEvent::UserPromptSubmit)
        );
        assert_eq!(
            HookEvent::parse("prompt-submit"),
            Some(HookEvent::UserPromptSubmit)
        );
        assert_eq!(HookEvent::parse("pre-compact"), Some(HookEvent::PreCompact));
        assert_eq!(HookEvent::parse("stop"), Some(HookEvent::Stop));
        assert_eq!(HookEvent::parse("session-stop"), Some(HookEvent::Stop));
        assert_eq!(
            HookEvent::parse("subagent-start"),
            Some(HookEvent::SubagentStart)
        );
        assert_eq!(
            HookEvent::parse("subagent-stop"),
            Some(HookEvent::SubagentStop)
        );
        assert_eq!(
            HookEvent::parse("task-completed"),
            Some(HookEvent::TaskCompleted)
        );
        assert_eq!(
            HookEvent::parse("teammate-idle"),
            Some(HookEvent::TeammateIdle)
        );
        // snake_case variants
        assert_eq!(
            HookEvent::parse("session_start"),
            Some(HookEvent::SessionStart)
        );
        assert_eq!(
            HookEvent::parse("pre_tool_use"),
            Some(HookEvent::PreToolUse)
        );
        // camelCase
        assert_eq!(
            HookEvent::parse("SessionStart"),
            Some(HookEvent::SessionStart)
        );
        assert_eq!(HookEvent::parse("PreToolUse"), Some(HookEvent::PreToolUse));
        // New RL signal events
        assert_eq!(
            HookEvent::parse("post-tool-failure"),
            Some(HookEvent::PostToolUseFailure)
        );
        assert_eq!(
            HookEvent::parse("posttooluse-failure"),
            Some(HookEvent::PostToolUseFailure)
        );
        assert_eq!(
            HookEvent::parse("post-tool-use-failure"),
            Some(HookEvent::PostToolUseFailure)
        );
        assert_eq!(
            HookEvent::parse("PostToolUseFailure"),
            Some(HookEvent::PostToolUseFailure)
        );
        assert_eq!(
            HookEvent::parse("stop-failure"),
            Some(HookEvent::StopFailure)
        );
        assert_eq!(
            HookEvent::parse("stopfailure"),
            Some(HookEvent::StopFailure)
        );
        assert_eq!(
            HookEvent::parse("StopFailure"),
            Some(HookEvent::StopFailure)
        );
        assert_eq!(HookEvent::parse("session-end"), Some(HookEvent::SessionEnd));
        assert_eq!(HookEvent::parse("sessionend"), Some(HookEvent::SessionEnd));
        assert_eq!(HookEvent::parse("SessionEnd"), Some(HookEvent::SessionEnd));
        // Tool-specific aliases (used in hooks config)
        assert_eq!(HookEvent::parse("pre-read"), Some(HookEvent::PreToolUse));
        assert_eq!(HookEvent::parse("pre-bash"), Some(HookEvent::PreToolUse));
        assert_eq!(HookEvent::parse("pre-edit"), Some(HookEvent::PreToolUse));
        assert_eq!(HookEvent::parse("post-read"), Some(HookEvent::PostToolUse));
        assert_eq!(HookEvent::parse("post-bash"), Some(HookEvent::PostToolUse));
        assert_eq!(HookEvent::parse("post-edit"), Some(HookEvent::PostToolUse));
        // PermissionRequest
        assert_eq!(
            HookEvent::parse("permission-request"),
            Some(HookEvent::PermissionRequest)
        );
        assert_eq!(
            HookEvent::parse("permissionrequest"),
            Some(HookEvent::PermissionRequest)
        );
        assert_eq!(
            HookEvent::parse("PermissionRequest"),
            Some(HookEvent::PermissionRequest)
        );
        // New events (H77-H82)
        assert_eq!(
            HookEvent::parse("notification"),
            Some(HookEvent::Notification)
        );
        assert_eq!(
            HookEvent::parse("Notification"),
            Some(HookEvent::Notification)
        );
        assert_eq!(HookEvent::parse("setup"), Some(HookEvent::Setup));
        assert_eq!(HookEvent::parse("Setup"), Some(HookEvent::Setup));
        assert_eq!(
            HookEvent::parse("elicitation"),
            Some(HookEvent::Elicitation)
        );
        assert_eq!(
            HookEvent::parse("Elicitation"),
            Some(HookEvent::Elicitation)
        );
        assert_eq!(
            HookEvent::parse("elicitation-result"),
            Some(HookEvent::ElicitationResult)
        );
        assert_eq!(
            HookEvent::parse("elicitationresult"),
            Some(HookEvent::ElicitationResult)
        );
        assert_eq!(
            HookEvent::parse("ElicitationResult"),
            Some(HookEvent::ElicitationResult)
        );
        assert_eq!(HookEvent::parse("cwd-changed"), Some(HookEvent::CwdChanged));
        assert_eq!(HookEvent::parse("cwdchanged"), Some(HookEvent::CwdChanged));
        assert_eq!(HookEvent::parse("CwdChanged"), Some(HookEvent::CwdChanged));
        assert_eq!(
            HookEvent::parse("file-changed"),
            Some(HookEvent::FileChanged)
        );
        assert_eq!(
            HookEvent::parse("filechanged"),
            Some(HookEvent::FileChanged)
        );
        assert_eq!(
            HookEvent::parse("FileChanged"),
            Some(HookEvent::FileChanged)
        );
        // Unknown
        assert_eq!(HookEvent::parse("unknown-event"), None);
        assert_eq!(HookEvent::parse(""), None);
    }

    #[test]
    fn test_hook_event_as_str() {
        assert_eq!(HookEvent::SessionStart.as_str(), "SessionStart");
        assert_eq!(HookEvent::PreToolUse.as_str(), "PreToolUse");
        assert_eq!(HookEvent::PostToolUse.as_str(), "PostToolUse");
        assert_eq!(HookEvent::Stop.as_str(), "Stop");
        assert_eq!(HookEvent::PostToolUseFailure.as_str(), "PostToolUseFailure");
        assert_eq!(HookEvent::StopFailure.as_str(), "StopFailure");
        assert_eq!(HookEvent::SessionEnd.as_str(), "SessionEnd");
        assert_eq!(HookEvent::PermissionRequest.as_str(), "PermissionRequest");
    }

    #[test]
    fn test_hook_event_as_str_all_variants() {
        // Exhaustive check: every variant must return a non-empty string
        let variants = [
            HookEvent::SessionStart,
            HookEvent::UserPromptSubmit,
            HookEvent::PreToolUse,
            HookEvent::PostToolUse,
            HookEvent::PreCompact,
            HookEvent::Stop,
            HookEvent::SubagentStart,
            HookEvent::SubagentStop,
            HookEvent::TaskCompleted,
            HookEvent::TeammateIdle,
            HookEvent::PostToolUseFailure,
            HookEvent::StopFailure,
            HookEvent::SessionEnd,
            HookEvent::PostCompact,
            HookEvent::ConfigChange,
            HookEvent::WorktreeCreate,
            HookEvent::WorktreeRemove,
            HookEvent::InstructionsLoaded,
            HookEvent::PermissionRequest,
            HookEvent::Notification,
            HookEvent::Setup,
            HookEvent::Elicitation,
            HookEvent::ElicitationResult,
            HookEvent::CwdChanged,
            HookEvent::FileChanged,
        ];
        for v in &variants {
            assert!(
                !v.as_str().is_empty(),
                "as_str() should not be empty for {v:?}"
            );
        }
    }

    #[test]
    fn test_hook_event_hook_output_name_only_three_events() {
        // Only PreToolUse, PostToolUse, UserPromptSubmit support hookSpecificOutput
        assert_eq!(HookEvent::PreToolUse.hook_output_name(), Some("PreToolUse"));
        assert_eq!(
            HookEvent::PostToolUse.hook_output_name(),
            Some("PostToolUse")
        );
        assert_eq!(
            HookEvent::UserPromptSubmit.hook_output_name(),
            Some("UserPromptSubmit")
        );

        // All others must return None
        let non_hook_events = [
            HookEvent::SessionStart,
            HookEvent::Stop,
            HookEvent::PreCompact,
            HookEvent::SubagentStart,
            HookEvent::SubagentStop,
            HookEvent::TaskCompleted,
            HookEvent::TeammateIdle,
            HookEvent::PostToolUseFailure,
            HookEvent::StopFailure,
            HookEvent::SessionEnd,
            HookEvent::PostCompact,
            HookEvent::PermissionRequest,
            HookEvent::Notification,
            HookEvent::Setup,
        ];
        for e in &non_hook_events {
            assert!(
                e.hook_output_name().is_none(),
                "Event {e:?} must NOT support hookSpecificOutput"
            );
        }
    }

    #[test]
    fn test_hook_event_parse_new_variants() {
        assert_eq!(
            HookEvent::parse("post-compact"),
            Some(HookEvent::PostCompact)
        );
        assert_eq!(
            HookEvent::parse("postcompact"),
            Some(HookEvent::PostCompact)
        );
        assert_eq!(
            HookEvent::parse("PostCompact"),
            Some(HookEvent::PostCompact)
        );
        assert_eq!(
            HookEvent::parse("config-change"),
            Some(HookEvent::ConfigChange)
        );
        assert_eq!(
            HookEvent::parse("configchange"),
            Some(HookEvent::ConfigChange)
        );
        assert_eq!(
            HookEvent::parse("worktree-create"),
            Some(HookEvent::WorktreeCreate)
        );
        assert_eq!(
            HookEvent::parse("worktreecreate"),
            Some(HookEvent::WorktreeCreate)
        );
        assert_eq!(
            HookEvent::parse("worktree-remove"),
            Some(HookEvent::WorktreeRemove)
        );
        assert_eq!(
            HookEvent::parse("worktreeremove"),
            Some(HookEvent::WorktreeRemove)
        );
        assert_eq!(
            HookEvent::parse("instructions-loaded"),
            Some(HookEvent::InstructionsLoaded)
        );
        assert_eq!(
            HookEvent::parse("instructionsloaded"),
            Some(HookEvent::InstructionsLoaded)
        );
    }

    #[test]
    fn test_decision_strictness_ordering() {
        // Skip=0, Allow=1, Block=2
        let skip = Decision::Skip;
        let allow = Decision::Allow;
        let block = Decision::Block("x".to_string());

        // Allow > Skip
        assert_eq!(skip.clone().escalate(allow.clone()), Decision::Allow);
        // Block > Allow
        assert!(matches!(
            allow.clone().escalate(block.clone()),
            Decision::Block(_)
        ));
        // Block > Skip
        assert!(matches!(
            skip.clone().escalate(block.clone()),
            Decision::Block(_)
        ));
        // Same level: first wins
        assert_eq!(skip.clone().escalate(Decision::Skip), Decision::Skip);
        assert_eq!(allow.clone().escalate(Decision::Allow), Decision::Allow);
    }

    #[test]
    fn test_handler_result_block_reason_preserved() {
        let r = HandlerResult::block("my_handler", "detailed reason".to_string());
        assert_eq!(r.handler_name, "my_handler");
        assert!(matches!(r.decision, Decision::Block(ref s) if s == "detailed reason"));
        assert!(r.context_lines.is_empty());
    }

    #[test]
    fn test_handler_result_allow_no_context() {
        let r = HandlerResult::allow("h", None);
        assert_eq!(r.decision, Decision::Allow);
        assert!(r.context_lines.is_empty());
        assert_eq!(r.handler_name, "h");
    }

    #[test]
    fn test_handler_result_allow_with_context() {
        let r = HandlerResult::allow("h", Some("my context line".to_string()));
        assert_eq!(r.decision, Decision::Allow);
        assert_eq!(r.context_lines.len(), 1);
        assert_eq!(r.context_lines[0], "my context line");
    }

    #[test]
    fn test_handler_result_skip_empty_context() {
        let r = HandlerResult::skip("h");
        assert_eq!(r.decision, Decision::Skip);
        assert!(r.context_lines.is_empty());
        assert!(r.metrics.is_null());
        assert_eq!(r.duration_ms, 0.0);
    }

    #[test]
    fn test_cortex_output_block_with_reason() {
        let output = CortexOutput {
            hook_specific_output: None,
            suppress_output: true,
            decision: Some("block".to_string()),
            reason: Some("dangerous operation".to_string()),
            system_message: None,
            metrics: None,
        };
        let json = serde_json::to_value(&output).unwrap();
        assert_eq!(json["decision"], "block");
        assert_eq!(json["reason"], "dangerous operation");
        assert_eq!(json["suppressOutput"], true);
        assert!(json.get("hookSpecificOutput").is_none());
    }

    #[test]
    fn test_cortex_output_with_metrics() {
        let output = CortexOutput {
            hook_specific_output: None,
            suppress_output: false,
            decision: None,
            reason: None,
            system_message: None,
            metrics: Some(serde_json::json!({"total_ms": "1.5", "handlers_executed": 3})),
        };
        let json = serde_json::to_value(&output).unwrap();
        assert!(json.get("metrics").is_some());
        assert_eq!(json["metrics"]["handlers_executed"], 3);
    }

    #[test]
    fn test_hook_specific_output_fields() {
        let hso = HookSpecificOutput {
            hook_event_name: "PostToolUse".to_string(),
            additional_context: "context line 1 | context line 2".to_string(),
        };
        assert_eq!(hso.hook_event_name, "PostToolUse");
        assert!(hso.additional_context.contains("context line 1"));
        let json = serde_json::to_value(&hso).unwrap();
        assert_eq!(json["hookEventName"], "PostToolUse");
        assert_eq!(json["additionalContext"], "context line 1 | context line 2");
    }

    #[test]
    fn test_decision_block_message_preserved_through_escalate() {
        let b1 = Decision::Block("first".to_string());
        let b2 = Decision::Block("second".to_string());
        // First block wins when equal strictness
        let result = b1.escalate(b2);
        assert!(matches!(result, Decision::Block(ref s) if s == "first"));
    }

    #[test]
    fn test_decision_escalation() {
        // Skip < Allow < Block
        let skip = Decision::Skip;
        let allow = Decision::Allow;
        let block = Decision::Block("reason".to_string());

        // Skip escalated to Allow = Allow
        assert_eq!(skip.clone().escalate(allow.clone()), Decision::Allow);
        // Allow escalated to Block = Block
        assert!(matches!(
            allow.clone().escalate(block.clone()),
            Decision::Block(_)
        ));
        // Block escalated to Allow = Block (already highest)
        assert!(matches!(
            block.clone().escalate(allow.clone()),
            Decision::Block(_)
        ));
        // Skip escalated to Skip = Skip
        assert_eq!(skip.clone().escalate(Decision::Skip), Decision::Skip);
        // Block escalated to another Block = first Block wins
        let block2 = Decision::Block("other".to_string());
        let result = block.escalate(block2);
        // First block stays because neither is strictly greater
        assert!(matches!(result, Decision::Block(_)));
    }

    #[test]
    fn test_handler_result_constructors() {
        let skip = HandlerResult::skip("test");
        assert_eq!(skip.decision, Decision::Skip);
        assert!(skip.context_lines.is_empty());
        assert_eq!(skip.handler_name, "test");

        let allow = HandlerResult::allow("test", Some("context line".to_string()));
        assert_eq!(allow.decision, Decision::Allow);
        assert_eq!(allow.context_lines.len(), 1);

        let allow_none = HandlerResult::allow("test", None);
        assert!(allow_none.context_lines.is_empty());

        let block = HandlerResult::block("test", "blocked".to_string());
        assert!(matches!(block.decision, Decision::Block(r) if r == "blocked"));
    }

    #[test]
    fn test_cortex_output_serialization() {
        // Empty output (no context, approve)
        let output = CortexOutput {
            hook_specific_output: None,
            suppress_output: false,
            decision: Some("approve".to_string()),
            reason: None,
            system_message: None,
            metrics: None,
        };
        let json = serde_json::to_string(&output).unwrap();
        // suppress_output = false should be skipped
        assert!(!json.contains("suppressOutput"));
        assert!(!json.contains("hookSpecificOutput"));
        assert!(json.contains("approve"));

        // With hook specific output
        let output2 = CortexOutput {
            hook_specific_output: Some(HookSpecificOutput {
                hook_event_name: "PreToolUse".to_string(),
                additional_context: "some context".to_string(),
            }),
            suppress_output: true,
            decision: None,
            reason: None,
            system_message: None,
            metrics: None,
        };
        let json2 = serde_json::to_string(&output2).unwrap();
        assert!(json2.contains("hookSpecificOutput"));
        assert!(json2.contains("additionalContext"));
        assert!(json2.contains("suppressOutput"));
        assert!(!json2.contains("decision"));

        // Events not in {PreToolUse, PostToolUse, UserPromptSubmit} should NOT use
        // hookSpecificOutput — Claude Code v2.1.81 rejects it with "Invalid input".
        // Instead, context goes in systemMessage.
        assert!(HookEvent::SessionStart.hook_output_name().is_none());
        assert!(HookEvent::Stop.hook_output_name().is_none());
        assert!(HookEvent::PreCompact.hook_output_name().is_none());
        assert_eq!(HookEvent::PreToolUse.hook_output_name(), Some("PreToolUse"));
        assert_eq!(
            HookEvent::PostToolUse.hook_output_name(),
            Some("PostToolUse")
        );
        assert_eq!(
            HookEvent::UserPromptSubmit.hook_output_name(),
            Some("UserPromptSubmit")
        );

        // For non-supported events, use systemMessage at top level
        let output3 = CortexOutput {
            hook_specific_output: None,
            suppress_output: true,
            decision: None,
            reason: None,
            system_message: Some("Touring Knowledge: 10 files known".to_string()),
            metrics: None,
        };
        let json3 = serde_json::to_string(&output3).unwrap();
        assert!(!json3.contains("hookSpecificOutput"));
        assert!(json3.contains("systemMessage"));
        assert!(json3.contains("Touring Knowledge"));
    }
}
