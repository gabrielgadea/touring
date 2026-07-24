//! RL Constants — Centralized state and action IDs for QTable operations.
//!
//! **Contract**: changing these constants invalidates existing QTable data.
//! If you change them, also clear/migrate the `learning_qtable` table in
//! `rlm_memory.db`.
//!
//! Gap 4: These were previously hardcoded in `touring_cortex::rl_mapping`.
//! Now centralized here so `touring_cortex` and `touring_learning` both reference
//! the same constants.

/// RL state IDs for hook events (0-11).
pub mod state {
    /// SessionStart event.
    pub const SESSION_START: u64 = 0;
    /// UserPromptSubmit event.
    pub const USER_PROMPT_SUBMIT: u64 = 1;
    /// PreToolUse event.
    pub const PRE_TOOL_USE: u64 = 2;
    /// PostToolUse event.
    pub const POST_TOOL_USE: u64 = 3;
    /// PreCompact event.
    pub const PRE_COMPACT: u64 = 4;
    /// PostCompact event.
    pub const POST_COMPACT: u64 = 5;
    /// Stop event.
    pub const STOP: u64 = 6;
    /// SubagentStart event.
    pub const SUBAGENT_START: u64 = 7;
    /// SubagentStop event.
    pub const SUBAGENT_STOP: u64 = 8;
    /// TeammateIdle event.
    pub const TEAMMATE_IDLE: u64 = 9;
    /// TaskCompleted event.
    pub const TASK_COMPLETED: u64 = 10;
    /// Unknown/catch-all event.
    pub const UNKNOWN: u64 = 11;
}

/// RL action IDs for tools (0-13).
pub mod action {
    /// Read tool.
    pub const READ: u64 = 0;
    /// Write tool.
    pub const WRITE: u64 = 1;
    /// Edit or MultiEdit tool.
    pub const EDIT: u64 = 2;
    /// Bash tool.
    pub const BASH: u64 = 3;
    /// Grep tool.
    pub const GREP: u64 = 4;
    /// Glob tool.
    pub const GLOB: u64 = 5;
    /// Agent or Task tool.
    pub const AGENT: u64 = 6;
    /// ToolSearch tool.
    pub const TOOL_SEARCH: u64 = 7;
    /// TaskCreate or TaskUpdate tool.
    pub const TASK_WRITE: u64 = 8;
    /// SendMessage tool.
    pub const SEND_MESSAGE: u64 = 9;
    /// EnterPlanMode tool.
    pub const ENTER_PLAN_MODE: u64 = 10;
    /// ExitPlanMode tool.
    pub const EXIT_PLAN_MODE: u64 = 11;
    /// Skill tool.
    pub const SKILL: u64 = 12;
    /// Unknown/catch-all tool.
    pub const UNKNOWN: u64 = 13;
}
