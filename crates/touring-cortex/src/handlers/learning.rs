//! RL Learning Handlers — Reward emission + TD learning loop.
//!
//! - `RlRewardEmitterHandler`: PostToolUse (async) — compute and record RL reward
//! - `TdLearningLoopHandler`: Stop (async) — TD(0) QTable update + tier promotion

use std::collections::HashMap;

use crate::context::CortexContext;
use crate::handler::Handler;
use crate::pipeline::Pipeline;
use crate::types::{HandlerResult, HookEvent};

// ── Reward Constants ──────────────────────────────────────────────────

const BASE_SUCCESS: f64 = 0.5;
const BASE_FAILURE: f64 = -0.3;
const REWORK_PENALTY: f64 = -0.15;
const FIRST_TRY_BONUS: f64 = 0.10;
const SEQ_FAILURE_PENALTY: f64 = -0.05;

/// Tool-specific bonus for successful operations.
fn tool_bonus(tool: &str) -> f64 {
    match tool {
        "Write" => 0.15,
        "Edit" | "MultiEdit" => 0.12,
        "Bash" => 0.10,
        "Read" => 0.05,
        "Grep" | "Glob" => 0.05,
        "Agent" | "Task" => 0.20,
        _ => 0.08,
    }
}

fn quality_tier(reward: f64) -> &'static str {
    if reward > 0.6 {
        "excellent"
    } else if reward > 0.3 {
        "good"
    } else if reward > 0.0 {
        "neutral"
    } else {
        "poor"
    }
}

// ── H32: RlRewardEmitter ──────────────────────────────────────────────

/// Compute multi-dimensional reward signal after every tool use.
///
/// Reward = base (±0.3/0.5) + tool_bonus + rework_penalty + first_try_bonus
///        + sequential_failure_penalty
///
/// Records via: log_hook_event, wilson_update, drift_record.
pub struct RlRewardEmitterHandler;

impl Handler for RlRewardEmitterHandler {
    fn name(&self) -> &str {
        "rl_reward_emitter"
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::PostToolUse]
    }

    fn is_async(&self) -> bool {
        true
    }

    fn execute(&self, ctx: &mut CortexContext) -> HandlerResult {
        let persistence = match &ctx.persistence {
            Some(p) => p,
            None => return HandlerResult::skip(self.name()),
        };

        let tool = ctx.tool_name.as_deref().unwrap_or("unknown");

        // Determine success from tool_result (non-empty = success)
        let success = ctx
            .input
            .get("tool_result")
            .and_then(|v| v.as_str())
            .map(|s| !s.is_empty() && !s.contains("error") && !s.contains("Error"))
            .unwrap_or(true);

        // Base reward
        let base = if success { BASE_SUCCESS } else { BASE_FAILURE };
        let bonus = if success { tool_bonus(tool) } else { 0.0 };

        // Rework penalty: check if file was edited recently
        let rework = if let Some(ref fp) = ctx.file_path {
            let recent = ctx.knowledge.recent_edits(fp, 5).unwrap_or_default();
            if recent.len() > 1 {
                REWORK_PENALTY
            } else {
                0.0
            }
        } else {
            0.0
        };

        // First-try bonus
        let first_try = if let Some(ref fp) = ctx.file_path {
            let recent = ctx.knowledge.recent_edits(fp, 1).unwrap_or_default();
            if recent.is_empty() {
                FIRST_TRY_BONUS
            } else {
                0.0
            }
        } else {
            0.0
        };

        // Sequential failure penalty
        let seq_penalty = if !success {
            let recent_bash = ctx.knowledge.find_bash_outcomes("", 5).unwrap_or_default();
            let consecutive_failures = recent_bash.iter().take_while(|o| !o.success).count();
            consecutive_failures as f64 * SEQ_FAILURE_PENALTY
        } else {
            0.0
        };

        let reward = (base + bonus + rework + first_try + seq_penalty).clamp(-1.0, 1.0);
        let tier = quality_tier(reward);

        // Record via persistence (all fail-silent)
        let outcome = format!("reward={reward:.2} tier={tier}");
        let _ = persistence.log_hook_event("PostToolUse", tool, &outcome, reward);
        let _ = persistence.wilson_update(&format!("tool:{tool}"), success);
        let _ = persistence.wilson_update(&format!("quality:{tier}"), true);
        let _ = persistence.drift_record("reward_distribution", reward);

        HandlerResult::skip(self.name()) // Async — no output
    }
}

// ── H33: TdLearningLoop ──────────────────────────────────────────────

/// TD(0) QTable update + tier promotion on session end.
///
/// 1. Query hook_events for session (last 2h)
/// 2. Aggregate rewards per (state, action) pair
/// 3. TD(0) update: Q(s,a) += α(r - Q(s,a))
/// 4. Tier promotion: access_count thresholds (3→working, 10→ref, 50→core)
pub struct TdLearningLoopHandler;

const ALPHA: f64 = 0.1;
const TIER_WORKING: i64 = 3;
const TIER_REFERENCE: i64 = 10;

// ── RL mapping — single source of truth in rl_mapping.rs ──
use crate::rl_mapping::{event_to_state, tool_to_action};

impl Handler for TdLearningLoopHandler {
    fn name(&self) -> &str {
        "td_learning_loop"
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::Stop]
    }

    fn is_async(&self) -> bool {
        true
    }

    fn execute(&self, ctx: &mut CortexContext) -> HandlerResult {
        let persistence = match &ctx.persistence {
            Some(p) => p,
            None => return HandlerResult::skip(self.name()),
        };

        // 1. Query recent hook events (last 2h = 7200s)
        let events = persistence.recent_hook_events(7200).unwrap_or_default();
        if events.is_empty() {
            return HandlerResult::skip(self.name());
        }

        // 2. Aggregate rewards per (state, action)
        let mut sa_rewards: HashMap<(u64, u64), Vec<f64>> = HashMap::new();
        for (_id, event_type, tool_name, reward) in &events {
            let state = event_to_state(event_type);
            let action = tool_to_action(tool_name);
            sa_rewards.entry((state, action)).or_default().push(*reward);
        }

        // 3. TD(0) update via direct SQLite (since QTable would need &mut).
        // learning_qtable lives in consolidated graph.db.
        let conn = match rusqlite::Connection::open(
            touring_foundation::TouringConfig::graph_db_canonical(&ctx.project_root),
        ) {
            Ok(c) => c,
            Err(_) => return HandlerResult::skip(self.name()),
        };

        let mut updates = 0u32;
        for ((state, action), rewards) in &sa_rewards {
            let avg_reward = rewards.iter().sum::<f64>() / rewards.len() as f64;
            let key = format!("ev{}:{}", state, action);

            // Get current Q value
            let current_q: f64 = conn
                .query_row(
                    "SELECT q_value FROM learning_qtable WHERE state_action = ?1",
                    rusqlite::params![key],
                    |row| row.get(0),
                )
                .unwrap_or(0.0);

            // TD(0): Q += α(r - Q)
            let new_q = current_q + ALPHA * (avg_reward - current_q);

            let _ = conn.execute(
                "INSERT OR REPLACE INTO learning_qtable (state_action, q_value, trace)
                 VALUES (?1, ?2, 0.0)",
                rusqlite::params![key, new_q],
            );
            updates += 1;
        }

        // 4. Tier promotion (via RlmMemory if available)
        if let Some(ref rlm) = ctx.rlm {
            use touring_intelligence::rl::memory::rlm::MemoryTier;

            // Search ephemeral entries with high access
            let entries = rlm
                .search("", Some(MemoryTier::Ephemeral), 100)
                .unwrap_or_default();
            for entry in &entries {
                if entry.access_count >= TIER_WORKING {
                    let _ = rlm.store(&entry.key, MemoryTier::Working, &entry.value, None, None);
                }
            }

            let entries = rlm
                .search("", Some(MemoryTier::Working), 100)
                .unwrap_or_default();
            for entry in &entries {
                if entry.access_count >= TIER_REFERENCE {
                    let _ = rlm.store(&entry.key, MemoryTier::Reference, &entry.value, None, None);
                }
            }
        }

        // Log the learning event
        let summary = format!("td_updates={updates} events={}", events.len());
        let _ = persistence.log_hook_event("Stop", "TdLearningLoop", &summary, 0.3);

        HandlerResult::skip(self.name()) // Async — no output
    }
}

// ── Registration ──────────────────────────────────────────────────────

/// Registers all learning-domain handlers on the given pipeline.
pub fn register(pipeline: &mut Pipeline) {
    pipeline.register(Box::new(RlRewardEmitterHandler));
    pipeline.register(Box::new(TdLearningLoopHandler));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_bonus_values() {
        assert_eq!(tool_bonus("Write"), 0.15);
        assert_eq!(tool_bonus("Read"), 0.05);
        assert_eq!(tool_bonus("Agent"), 0.20);
        assert_eq!(tool_bonus("unknown_tool"), 0.08);
    }

    #[test]
    fn test_quality_tier() {
        assert_eq!(quality_tier(0.7), "excellent");
        assert_eq!(quality_tier(0.4), "good");
        assert_eq!(quality_tier(0.1), "neutral");
        assert_eq!(quality_tier(-0.2), "poor");
    }

    #[test]
    fn test_event_to_state() {
        // Uses unified rl_mapping module — IDs match rl_mapping.rs
        assert_eq!(event_to_state("SessionStart"), 0);
        assert_eq!(event_to_state("PostToolUse"), 3);
        assert_eq!(event_to_state("Stop"), 6);
        assert_eq!(event_to_state("unknown"), 11);
    }

    #[test]
    fn test_tool_to_action() {
        // Uses unified rl_mapping module — IDs match rl_mapping.rs
        assert_eq!(tool_to_action("Read"), 0);
        assert_eq!(tool_to_action("Edit"), 2);
        assert_eq!(tool_to_action("Bash"), 3);
        assert_eq!(tool_to_action("Agent"), 6);
    }

    #[test]
    fn test_reward_calculation() {
        // Successful Write: base(0.5) + bonus(0.15) = 0.65
        let reward = BASE_SUCCESS + tool_bonus("Write");
        assert!((reward - 0.65).abs() < 0.001);

        // Failed Bash: base(-0.3) + bonus(0) = -0.3
        let reward = BASE_FAILURE;
        assert!((reward - (-0.3)).abs() < 0.001);
    }

    #[test]
    fn test_td_update() {
        // Q=0.5, reward=0.8 → Q = 0.5 + 0.1*(0.8-0.5) = 0.53
        let q = 0.5;
        let r = 0.8;
        let new_q = q + ALPHA * (r - q);
        assert!((new_q - 0.53).abs() < 0.001);
    }

    #[test]
    fn test_handler_properties() {
        let h = RlRewardEmitterHandler;
        assert_eq!(h.name(), "rl_reward_emitter");
        assert_eq!(h.events(), &[HookEvent::PostToolUse]);
        assert!(h.is_async());
        assert!(h.tool_matcher().is_none());

        let h = TdLearningLoopHandler;
        assert_eq!(h.name(), "td_learning_loop");
        assert_eq!(h.events(), &[HookEvent::Stop]);
        assert!(h.is_async());
    }
}
