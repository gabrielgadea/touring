//! Unified RL state/action mapping for QTable operations.
//!
//! Both `server.rs` (MCP tools) and `cortex/handlers/learning.rs` (TD loop)
//! must agree on the same state/action IDs. This module is the single source
//! of truth — all RL mapping MUST go through these functions.
//!
//! **Contract**: changing these mappings invalidates existing QTable data.
//! If you change them, also clear/migrate the `learning_qtable` table in
//! `rlm_memory.db`.
//!
//! Gap 4: Constants centralized in `touring_intelligence::rl::constants`.

use touring_intelligence::rl::constants::action;
use touring_intelligence::rl::constants::state;

/// Map hook event type to a stable RL state ID.
///
/// Covers all Claude Code hook events. Unknown events map to a catch-all.
pub fn event_to_state(event_type: &str) -> u64 {
    match event_type {
        "SessionStart" => state::SESSION_START,
        "UserPromptSubmit" => state::USER_PROMPT_SUBMIT,
        "PreToolUse" => state::PRE_TOOL_USE,
        "PostToolUse" => state::POST_TOOL_USE,
        "PreCompact" => state::PRE_COMPACT,
        "PostCompact" => state::POST_COMPACT,
        "Stop" => state::STOP,
        "SubagentStart" => state::SUBAGENT_START,
        "SubagentStop" => state::SUBAGENT_STOP,
        "TeammateIdle" => state::TEAMMATE_IDLE,
        "TaskCompleted" => state::TASK_COMPLETED,
        _ => state::UNKNOWN,
    }
}

/// Map tool name to a stable RL action ID.
///
/// Uses explicit match table (NOT hashing) for deterministic, human-readable IDs.
/// Every QTable consumer must use this function — never hash tool names directly.
pub fn tool_to_action(tool_name: &str) -> u64 {
    match tool_name {
        "Read" => action::READ,
        "Write" => action::WRITE,
        "Edit" | "MultiEdit" => action::EDIT,
        "Bash" => action::BASH,
        "Grep" => action::GREP,
        "Glob" => action::GLOB,
        "Agent" | "Task" => action::AGENT,
        "ToolSearch" => action::TOOL_SEARCH,
        "TaskCreate" | "TaskUpdate" => action::TASK_WRITE,
        "SendMessage" => action::SEND_MESSAGE,
        "EnterPlanMode" => action::ENTER_PLAN_MODE,
        "ExitPlanMode" => action::EXIT_PLAN_MODE,
        "Skill" => action::SKILL,
        _ => action::UNKNOWN,
    }
}

/// E3-S1: Compute per-handler budget allocation based on RL Q-values.
///
/// Distributes the total context budget proportionally to handler Q-values.
/// Handlers with higher historical reward (Q-value) get more budget.
/// Handlers with no Q-value data get a fair share of the remaining budget.
///
/// # Arguments
/// * `handler_names` — names of active handlers for this event
/// * `q_values` — map of handler_name → Q-value (from QTable)
/// * `total_budget` — total context budget in chars
/// * `min_budget` — minimum per-handler budget (floor)
///
/// # Returns
/// Map of handler_name → allocated budget (chars)
pub fn allocate_budget_by_qvalue(
    handler_names: &[&str],
    q_values: &std::collections::HashMap<String, f64>,
    total_budget: usize,
    min_budget: usize,
) -> std::collections::HashMap<String, usize> {
    let n = handler_names.len();
    if n == 0 || total_budget == 0 {
        return std::collections::HashMap::new();
    }

    // Collect Q-values, defaulting to 0.5 for unknown handlers
    let default_q = 0.5;
    let q_scores: Vec<f64> = handler_names
        .iter()
        .map(|&name| {
            q_values.get(name).copied().unwrap_or(default_q).max(0.01) // floor to avoid zero allocation
        })
        .collect();

    let total_q: f64 = q_scores.iter().sum();
    if total_q <= 0.0 {
        // Fallback: equal distribution
        let per_handler = total_budget / n;
        return handler_names
            .iter()
            .map(|&name| (name.to_string(), per_handler.max(min_budget)))
            .collect();
    }

    // Proportional allocation
    handler_names
        .iter()
        .zip(q_scores.iter())
        .map(|(&name, &q)| {
            let share = (q / total_q * total_budget as f64) as usize;
            (name.to_string(), share.max(min_budget))
        })
        .collect()
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

    // ── Additional coverage ───────────────────────────────────────────

    #[test]
    fn event_states_are_stable() {
        // These IDs are part of the QTable contract — must never change
        assert_eq!(event_to_state("SessionStart"), 0);
        assert_eq!(event_to_state("UserPromptSubmit"), 1);
        assert_eq!(event_to_state("PreToolUse"), 2);
        assert_eq!(event_to_state("PostToolUse"), 3);
        assert_eq!(event_to_state("PreCompact"), 4);
        assert_eq!(event_to_state("PostCompact"), 5);
        assert_eq!(event_to_state("Stop"), 6);
        assert_eq!(event_to_state("SubagentStart"), 7);
        assert_eq!(event_to_state("SubagentStop"), 8);
        assert_eq!(event_to_state("TeammateIdle"), 9);
        assert_eq!(event_to_state("TaskCompleted"), 10);
    }

    #[test]
    fn tool_actions_are_stable() {
        // These IDs are part of the QTable contract — must never change
        assert_eq!(tool_to_action("Read"), 0);
        assert_eq!(tool_to_action("Write"), 1);
        assert_eq!(tool_to_action("Edit"), 2);
        assert_eq!(tool_to_action("Bash"), 3);
        assert_eq!(tool_to_action("Grep"), 4);
        assert_eq!(tool_to_action("Glob"), 5);
        assert_eq!(tool_to_action("Agent"), 6);
        assert_eq!(tool_to_action("ToolSearch"), 7);
        assert_eq!(tool_to_action("TaskCreate"), 8);
        assert_eq!(tool_to_action("SendMessage"), 9);
        assert_eq!(tool_to_action("EnterPlanMode"), 10);
        assert_eq!(tool_to_action("ExitPlanMode"), 11);
        assert_eq!(tool_to_action("Skill"), 12);
    }

    #[test]
    fn unknown_event_catch_all_is_distinct() {
        // Catch-all (11) must differ from all known event IDs (0-10)
        let catch_all = event_to_state("__unknown__");
        for id in 0..11_u64 {
            // Known IDs 0-10 are used; catch-all = 11
            assert_ne!(
                catch_all, id,
                "catch-all should not collide with known state {id}"
            );
        }
    }

    #[test]
    fn unknown_tool_catch_all_is_distinct() {
        // Catch-all (13) must differ from all known tool IDs (0-12)
        let catch_all = tool_to_action("__unknown__");
        for id in 0..13_u64 {
            assert_ne!(
                catch_all, id,
                "catch-all should not collide with known action {id}"
            );
        }
    }

    #[test]
    fn empty_string_maps_to_catch_all() {
        assert_eq!(event_to_state(""), 11);
        assert_eq!(tool_to_action(""), 13);
    }

    #[test]
    fn multi_edit_alias() {
        assert_eq!(tool_to_action("MultiEdit"), tool_to_action("Edit"));
    }

    #[test]
    fn task_alias() {
        assert_eq!(tool_to_action("Task"), tool_to_action("Agent"));
    }

    #[test]
    fn task_update_alias() {
        assert_eq!(tool_to_action("TaskUpdate"), tool_to_action("TaskCreate"));
    }

    #[test]
    fn all_known_events_map_below_catch_all() {
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
        for e in &events {
            assert!(
                event_to_state(e) < 11,
                "Event {e} should map below catch-all 11"
            );
        }
    }

    #[test]
    fn all_known_tools_map_below_catch_all() {
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
        for t in &tools {
            assert!(
                tool_to_action(t) < 13,
                "Tool {t} should map below catch-all 13"
            );
        }
    }

    #[test]
    fn event_to_state_is_deterministic() {
        // Same input always produces same output
        assert_eq!(event_to_state("PreToolUse"), event_to_state("PreToolUse"));
        assert_eq!(event_to_state("Stop"), event_to_state("Stop"));
        assert_eq!(
            event_to_state("unknown_xyz"),
            event_to_state("another_unknown")
        );
    }

    #[test]
    fn tool_to_action_is_deterministic() {
        assert_eq!(tool_to_action("Read"), tool_to_action("Read"));
        assert_eq!(tool_to_action("Bash"), tool_to_action("Bash"));
        assert_eq!(tool_to_action("xyz_unknown"), tool_to_action("abc_unknown"));
    }

    #[test]
    fn grep_and_glob_have_distinct_ids() {
        assert_ne!(tool_to_action("Grep"), tool_to_action("Glob"));
        assert_eq!(tool_to_action("Grep"), 4);
        assert_eq!(tool_to_action("Glob"), 5);
    }

    // ── E3-S1: Dynamic budget allocation tests ──────────────────────

    #[test]
    fn test_budget_allocation_proportional() {
        let mut q_values = std::collections::HashMap::new();
        q_values.insert("high_value".to_string(), 0.9);
        q_values.insert("low_value".to_string(), 0.1);

        let alloc = allocate_budget_by_qvalue(&["high_value", "low_value"], &q_values, 1000, 10);
        assert!(
            alloc["high_value"] > alloc["low_value"],
            "High Q-value handler should get more budget"
        );
    }

    #[test]
    fn test_budget_allocation_respects_minimum() {
        let mut q_values = std::collections::HashMap::new();
        q_values.insert("tiny".to_string(), 0.001);
        q_values.insert("huge".to_string(), 100.0);

        let alloc = allocate_budget_by_qvalue(&["tiny", "huge"], &q_values, 100, 20);
        assert!(alloc["tiny"] >= 20, "Must respect min_budget");
    }

    #[test]
    fn test_budget_allocation_empty() {
        let q = std::collections::HashMap::new();
        let alloc = allocate_budget_by_qvalue(&[], &q, 100, 10);
        assert!(alloc.is_empty());
    }

    #[test]
    fn test_budget_allocation_unknown_handlers_get_default() {
        let q_values = std::collections::HashMap::new(); // no Q-values known
        let alloc = allocate_budget_by_qvalue(&["unknown_a", "unknown_b"], &q_values, 200, 10);
        // Both should get roughly equal share (default Q = 0.5 each)
        let diff = (alloc["unknown_a"] as i64 - alloc["unknown_b"] as i64).unsigned_abs();
        assert!(diff <= 1, "Unknown handlers should get equal share");
    }
}
