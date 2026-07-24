//! `touring-contracts` — Touring's dependency-inversion contract traits (the IoC seam).
//!
//! Leaf crate with a single runtime dependency (`serde_json`). It houses the
//! runtime *contract traits* that Touring subsystems declare and host crates
//! implement, inverting outward `crate::…` edges so a subsystem can later be
//! extracted into its own crate **without rewriting call sites** (the classic
//! dependency inversion). Extracted from `touring-hooks::gateway::deps`
//! (S-13, 2026-06-06) and promoted to a leaf crate 2026-06-09 (Master Plan
//! A.W3.P1) so the seam is reusable beyond the Code Execution Gateway.
//!
//! # [`LearnRuntime`] — the X9 LEARN persistence seam
//!
//! The CEG's X9 LEARN stage persists RL rewards, gotchas, and memory lessons.
//! [`LearnRuntime`] abstracts exactly those three operations so a subsystem's
//! learn stage closes its feedback loops without naming the host runtime type.
//! Every method is **fail-open**: it returns the host handler's JSON string
//! (which may itself encode an error) and must never panic.

#![deny(missing_docs)]
#![forbid(unsafe_code)]
// Lint-ceiling ratchet (RBP-01, 2026-06-13): this leaf is unwrap-free in
// production; deny it so a panic can never be introduced into the IoC seam that
// every host runtime depends on. Mirrors the CEG (touring-ceg gateway/mod.rs).
#![cfg_attr(not(test), deny(clippy::unwrap_used))]

use serde_json::Value;

/// The X9 LEARN persistence operations a subsystem needs from its host runtime.
///
/// Implemented by the host (e.g. `touring_hooks::runtime::HookRuntime`) by
/// delegating to the canonical `cli_learning_reward` / `cli_gotcha_add` /
/// `cli_memory_store` handlers. Abstracting them here lets a subsystem (the CEG
/// `gateway/learn.rs`) close the RL / gotcha / memory feedback loops without
/// importing the host's handler module — inverting that edge so the subsystem is
/// one step closer to being a standalone crate.
///
/// Every method mirrors the underlying handler contract: it returns the
/// handler's JSON string (which may itself encode an error) and is **fail-open**
/// — an implementation must never panic.
pub trait LearnRuntime {
    /// Inject an RL reward. `payload` carries `{tool_name, reward, context}`.
    /// Mirrors `cli_learning_reward`.
    fn learning_reward(&mut self, payload: &Value) -> String;

    /// Persist a gotcha entry. `payload` carries `{pattern, description, severity}`.
    /// Mirrors `cli_gotcha_add`.
    fn gotcha_add(&mut self, payload: &Value) -> String;

    /// Persist a memory entry. `payload` carries `{key, value, tier, type}`.
    /// Mirrors `cli_memory_store`.
    fn memory_store(&mut self, payload: &Value) -> String;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A mock [`LearnRuntime`] that records calls without any runtime — proves
    /// the seam is host-agnostic and that callers compile against the trait
    /// alone (no host runtime, no handler module).
    #[derive(Default)]
    struct MockLearn {
        rewards: Vec<String>,
        gotchas: Vec<String>,
        memories: Vec<String>,
    }

    impl LearnRuntime for MockLearn {
        fn learning_reward(&mut self, payload: &Value) -> String {
            self.rewards.push(payload.to_string());
            "{\"ok\":true}".to_owned()
        }
        fn gotcha_add(&mut self, payload: &Value) -> String {
            self.gotchas.push(payload.to_string());
            "{\"ok\":true}".to_owned()
        }
        fn memory_store(&mut self, payload: &Value) -> String {
            self.memories.push(payload.to_string());
            "{\"ok\":true}".to_owned()
        }
    }

    #[test]
    fn mock_learn_records_each_op_independently() {
        let mut m = MockLearn::default();
        let _ = m.learning_reward(&serde_json::json!({"tool_name": "ceg", "reward": 1.0}));
        let _ = m.gotcha_add(&serde_json::json!({"pattern": "rm -rf /"}));
        let _ = m.memory_store(&serde_json::json!({"key": "k", "value": "v"}));
        assert_eq!(m.rewards.len(), 1, "reward recorded");
        assert_eq!(m.gotchas.len(), 1, "gotcha recorded");
        assert_eq!(m.memories.len(), 1, "memory recorded");
    }

    /// Fail-open contract: a returned JSON string (even an error one) is the
    /// only channel — the trait surface itself cannot panic the caller.
    #[test]
    fn learn_runtime_methods_return_json_string() {
        let mut m = MockLearn::default();
        let out = m.learning_reward(&serde_json::json!({}));
        assert!(out.starts_with('{'), "must return a JSON object string");
    }
}
