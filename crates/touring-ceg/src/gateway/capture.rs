//! Stage **X0 CAPTURE** of the Code Execution Gateway. Phase **P3.2** of CEG
//! Pln2 (`docs/2026-05-17-ceg-pln2-plan.md`).
//!
//! X0 is the pipeline's admission filter. It answers one question about an
//! incoming tool call — *does this call run code?* — and, when the answer
//! is yes, hands a fresh [`Execution<Captured>`] to the rest of the gateway.
//!
//! The taxonomy of execution surfaces is [`ExecSurface`]. A call whose surface
//! is [`ExecSurface::NonExec`] (a `Read`, a `Glob`, ...) is not admitted:
//! [`capture_tool_call`] returns `None` and the gateway never sees it.

use super::typestate::{Captured, Execution, RawInvocation};
use serde::{Deserialize, Serialize};

/// The execution surface a tool call uses — the X0 taxonomy.
///
/// Only the code-bearing variants (those for which
/// [`is_code_bearing`](ExecSurface::is_code_bearing) is `true`) are admitted to
/// the gateway pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ExecSurface {
    /// A `Bash` tool call — a shell command.
    BashCommand,
    /// An MCP `ctx_execute` / `ctx_batch_execute` call — inline code run in a
    /// sandbox language.
    CtxExecute,
    /// An inferlet run or background-job spawn.
    Inferlet,
    /// Not a code-running call — the gateway does not gate it.
    NonExec,
}

impl ExecSurface {
    /// Classify a tool call by its tool name.
    ///
    /// Tool names may be bare (`"Bash"`) or MCP-namespaced
    /// (`"mcp__touring__ctx_execute"`); the trailing `__`-segment is matched.
    /// The inferlet arm is a deliberate substring heuristic — the exact MCP
    /// tool name is refined when `P1.5` wires the inferlet/jobs surface.
    #[must_use]
    pub fn detect(tool: &str) -> Self {
        let leaf = tool.rsplit("__").next().unwrap_or(tool);
        match leaf {
            "Bash" => Self::BashCommand,
            "ctx_execute" | "ctx_batch_execute" => Self::CtxExecute,
            other if other.contains("inferlet") => Self::Inferlet,
            _ => Self::NonExec,
        }
    }

    /// `true` for every surface that actually runs code — i.e. every variant
    /// except [`NonExec`](ExecSurface::NonExec).
    #[must_use]
    pub fn is_code_bearing(self) -> bool {
        !matches!(self, Self::NonExec)
    }
}

/// X0 CAPTURE — the gateway's admission filter.
///
/// Builds an [`Execution<Captured>`] for a code-bearing tool call. Returns
/// `None` when the call's [`ExecSurface`] is [`ExecSurface::NonExec`]: a
/// non-running call (a `Read`, a `Glob`) is never admitted to the pipeline.
///
/// ```
/// use touring_ceg::gateway::capture_tool_call;
///
/// // A shell command is admitted.
/// assert!(capture_tool_call("Bash", "cargo test", None).is_some());
/// // A file read is not.
/// assert!(capture_tool_call("Read", "/etc/hosts", None).is_none());
/// ```
pub fn capture_tool_call(
    tool: impl Into<String>,
    payload: impl Into<String>,
    intent: Option<String>,
) -> Option<Execution<Captured>> {
    let tool = tool.into();
    if !ExecSurface::detect(&tool).is_code_bearing() {
        return None;
    }
    let mut raw = RawInvocation::new(tool, payload);
    if let Some(intent) = intent {
        raw = raw.with_intent(intent);
    }
    Some(Execution::capture(raw))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_bash() {
        assert_eq!(ExecSurface::detect("Bash"), ExecSurface::BashCommand);
    }

    #[test]
    fn detect_ctx_run_bare_and_namespaced() {
        assert_eq!(ExecSurface::detect("ctx_execute"), ExecSurface::CtxExecute);
        assert_eq!(
            ExecSurface::detect("mcp__touring__ctx_execute"),
            ExecSurface::CtxExecute
        );
        assert_eq!(
            ExecSurface::detect("ctx_batch_execute"),
            ExecSurface::CtxExecute
        );
    }

    #[test]
    fn detect_inferlet_by_substring_heuristic() {
        assert_eq!(
            ExecSurface::detect("mcp__touring__ctx_inferlet"),
            ExecSurface::Inferlet
        );
    }

    #[test]
    fn detect_reads_and_globs_are_not_gated() {
        for tool in ["Read", "Glob", "Edit", "Write", "Grep"] {
            assert_eq!(
                ExecSurface::detect(tool),
                ExecSurface::NonExec,
                "{tool} must not be a code-running surface"
            );
        }
    }

    #[test]
    fn is_code_bearing_is_true_for_running_surfaces() {
        assert!(ExecSurface::BashCommand.is_code_bearing());
        assert!(ExecSurface::CtxExecute.is_code_bearing());
        assert!(ExecSurface::Inferlet.is_code_bearing());
    }

    #[test]
    fn non_running_surface_is_not_code_bearing() {
        assert!(!ExecSurface::NonExec.is_code_bearing());
    }

    #[test]
    fn capture_admits_a_code_bearing_call() {
        let captured = capture_tool_call("Bash", "ls -la", None).expect("Bash is code-bearing");
        assert_eq!(captured.ordinal(), 0);
        assert_eq!(captured.raw().tool, "Bash");
    }

    #[test]
    fn capture_rejects_a_non_running_call() {
        assert!(capture_tool_call("Read", "/etc/hosts", None).is_none());
        assert!(capture_tool_call("Glob", "**/*.rs", None).is_none());
    }

    #[test]
    fn capture_threads_the_stated_intent() {
        let captured = capture_tool_call("Bash", "rm tmp", Some("cleanup".to_owned()))
            .expect("Bash is code-bearing");
        assert_eq!(captured.raw().intent.as_deref(), Some("cleanup"));
    }

    #[test]
    fn exec_surface_serde_roundtrip() {
        for surface in [
            ExecSurface::BashCommand,
            ExecSurface::CtxExecute,
            ExecSurface::Inferlet,
            ExecSurface::NonExec,
        ] {
            let json = serde_json::to_string(&surface).expect("serialize");
            let back: ExecSurface = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(surface, back);
        }
    }
}
