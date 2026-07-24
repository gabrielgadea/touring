//! Unified RL state/action mapping for QTable operations.
//!
//! Both `server.rs` (MCP tools) and `cortex/handlers/learning.rs` (TD loop)
//! must agree on the same state/action IDs. This module is the single source
//! of truth — all RL mapping MUST go through these functions.
//!
//! **Contract**: changing these mappings invalidates existing QTable data.
//! If you change them, also clear/migrate the `learning_qtable` table in
//! `rlm_memory.db`.

/// Map hook event type to a stable RL state ID.
///
/// Covers all Claude Code hook events. Unknown events map to a catch-all.
pub fn event_to_state(event_type: &str) -> u64 {
    match event_type {
        "SessionStart" => 0,
        "UserPromptSubmit" => 1,
        "PreToolUse" => 2,
        "PostToolUse" => 3,
        "PreCompact" => 4,
        "PostCompact" => 5,
        "Stop" => 6,
        "SubagentStart" => 7,
        "SubagentStop" => 8,
        "TeammateIdle" => 9,
        "TaskCompleted" => 10,
        _ => 11,
    }
}

/// Map tool name to a stable RL action ID.
///
/// Uses explicit match table (NOT hashing) for deterministic, human-readable IDs.
/// Every QTable consumer must use this function — never hash tool names directly.
pub fn tool_to_action(tool_name: &str) -> u64 {
    match tool_name {
        "Read" => 0,
        "Write" => 1,
        "Edit" | "MultiEdit" => 2,
        "Bash" => 3,
        "Grep" => 4,
        "Glob" => 5,
        "Agent" | "Task" => 6,
        "ToolSearch" => 7,
        "TaskCreate" | "TaskUpdate" => 8,
        "SendMessage" => 9,
        "EnterPlanMode" => 10,
        "ExitPlanMode" => 11,
        "Skill" => 12,
        _ => 13,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_events_have_unique_ids() {
        let events = [
            "SessionStart",
            "UserPromptSubmit",
            "PreToolUse",
            "PostToolUse",
            "PreCompact",
            "PostCompact",
            "Stop",
            "SubagentStart",
            "SubagentStop",
            "TeammateIdle",
            "TaskCompleted",
        ];
        let mut ids: Vec<u64> = events.iter().map(|e| event_to_state(e)).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), events.len(), "State IDs must be unique");
    }

    #[test]
    fn known_tools_have_unique_ids() {
        let tools = [
            "Read",
            "Write",
            "Edit",
            "Bash",
            "Grep",
            "Glob",
            "Agent",
            "ToolSearch",
            "TaskCreate",
            "SendMessage",
            "EnterPlanMode",
            "ExitPlanMode",
            "Skill",
        ];
        let mut ids: Vec<u64> = tools.iter().map(|t| tool_to_action(t)).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), tools.len(), "Action IDs must be unique");
    }

    #[test]
    fn aliases_map_to_same_id() {
        assert_eq!(tool_to_action("Edit"), tool_to_action("MultiEdit"));
        assert_eq!(tool_to_action("Agent"), tool_to_action("Task"));
        assert_eq!(tool_to_action("TaskCreate"), tool_to_action("TaskUpdate"));
    }

    #[test]
    fn unknown_maps_to_catch_all() {
        assert_eq!(event_to_state("SomeFutureEvent"), 11);
        assert_eq!(tool_to_action("SomeFutureTool"), 13);
    }
}
