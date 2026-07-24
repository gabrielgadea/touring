//! SessionBus — Typed bidirectional inter-hook communication channel.
//!
//! Replaces ad-hoc `result_cache["__meta__"]` keys with a structured,
//! typed bus that enables fast feedback between perception (pre_read),
//! planning (MCTS/decompose), and execution (pre_edit/post_edit) hooks.
//!
//! # Architecture (Fasciculus Arcuatus Pattern)
//!
//! ```text
//! ┌──────────────┐     SessionBus      ┌──────────────┐
//! │  Perception   │◄──────────────────►│   Planning    │
//! │  (pre_read)   │   bidirectional    │  (MCTS/GoT)   │
//! └──────┬───────┘                    └──────┬───────┘
//!        │          SessionBus               │
//!        ▼     ◄─────────────────────►       ▼
//! ┌──────────────┐                    ┌──────────────┐
//! │  Execution    │                    │   Learning    │
//! │  (pre_edit)   │                    │  (RL/LinUCB)  │
//! └──────────────┘                    └──────────────┘
//! ```
//!
//! # Signals
//!
//! - **Perception → Execution**: `last_read_file`, `last_read_complexity`,
//!   `blast_radius_cache` — pre_edit knows what was just read without re-querying.
//! - **Execution → Learning**: `last_tool_outcome` — RL reward correlation.
//! - **Planning → Execution**: `active_plan_hint` — MCTS/decompose provides
//!   the current subtask context so pre_edit can prioritize relevant signals.
//! - **Learning → Perception**: `arm_effectiveness` — which LinUCB arms are
//!   currently productive, informing pre_read budget allocation.

use std::collections::HashMap;

use tokio::sync::broadcast;
use touring_simd::cortex::Evidence;

/// Typed inter-hook communication bus.
///
/// Lives in `ContextRuntime` (via `RefCell`), accessible from all hooks.
/// Mutable fields allow bidirectional signaling between hook phases.
#[derive(Debug, Clone)]
pub struct SessionBus {
    // ── Evidence broadcast (Fasciculus Arcuatus) ───────────────────
    /// Broadcast channel for Evidence events from tool executions.
    /// Subscribers: CortexDispatcher subscribes to receive Evidence for drift detection.
    evidence_tx: broadcast::Sender<Evidence>,

    // ── Perception → Execution (forward path) ──────────────────────
    /// File most recently read (set by pre_read, consumed by post_tool_rl).
    pub last_read_file: Option<String>,

    /// Max cyclomatic complexity of the last-read file (set by pre_read).
    /// Allows pre_edit to skip complexity checks if the file is known-simple.
    pub last_read_max_cc: Option<u16>,

    /// Cached blast radius count for recently-read files.
    /// Key: relative file path, Value: transitive dependency count.
    /// Populated by pre_read/post_read, consumed by pre_edit without re-querying.
    pub blast_radius_cache: HashMap<String, usize>,

    /// Cached ANN search results for recently-read files.
    /// Key: file path query, Value: top-K ANN search results.
    /// Populated by pre_read via AnnMemoryRecall, consumed by pre_edit
    /// for context pre-warming without re-querying the ANN index.
    pub ann_results_cache: HashMap<String, Vec<crate::ann_memory::SearchResult>>,

    // ── Planning → Execution (descending path) ─────────────────────
    /// Current active subtask description from decompose/MCTS.
    /// When set, pre_edit can prioritize signals relevant to this task.
    pub active_plan_hint: Option<String>,

    /// Current CILA level (replaces `__session_cila_level__` in result_cache).
    pub cila_level: u8,

    // ── Execution → Learning (ascending path) ──────────────────────
    /// Tool outcome from the most recent tool invocation.
    /// Set by post_tool_rl, consumed by LinUCB for immediate feedback.
    pub last_tool_accepted: Option<bool>,

    /// Quality score from the most recent post_edit quality gate.
    pub last_quality_score: Option<f64>,

    // ── RL Feedback State (written by post_tool_rl) ─────────────────
    /// Last EMA reward value from OnlineRLEngine after processing a reward.
    /// Used by cognitive engine for drift detection (sudden reward drops signal degradation).
    pub last_ema_reward: Option<f64>,

    /// Last LinUCB arm selected during tool planning.
    /// Used by cognitive engine for outcome correlation (which arm produced good/bad outcomes).
    pub last_arm_selected: Option<u8>,

    /// Last TD error from QTable update in OnlineRLEngine.
    /// Used by cognitive engine for learning quality assessment (high TD error = unstable).
    pub last_td_error: Option<f64>,

    // ── Learning → Perception (feedback path) ──────────────────────
    /// Per-arm effectiveness from LinUCB (arm_id → avg_reward).
    /// Updated after each RL cycle. Pre_read uses this to skip low-value arms.
    pub arm_effectiveness: HashMap<u8, f64>,

    // ── Hook Result Storage (A8 — hook chaining via ctx.last) ─────────
    /// Last hook result by hook name — enables `ctx.last` chaining.
    /// Set by each hook's post-phase, consumed by the next hook of same family.
    /// Key: hook name (e.g. "pre_read"), Value: JSON result payload.
    pub hook_results: HashMap<String, serde_json::Value>,

    /// Number of tool invocations in this session (monotonic counter).
    pub tool_invocation_count: u64,

    // ── D4 Think-in-Code: consecutive file read counter ───────────────
    /// Monotonic counter incremented by each pre_read hook invocation.
    /// Used to detect bulk-read analysis patterns (≥10 consecutive reads
    /// without intervening writes) which trigger Think-in-Code directive injection.
    pub consecutive_file_reads: u32,

    // ── MCTS Prefetch Path Queue (Suggestion 3 — 2026-04-20) ───────────────
    /// Relative file paths queued for background prefetch by the MCTS shadow rollout.
    /// Set by `handle_enter_plan_mode` after a successful rollout, consumed by
    /// `shared::file_prefetch::try_enqueue_prefetch` to warm the parser cache.
    pub prefetch_queue: Vec<String>,
}

impl SessionBus {
    /// Create a new bus with the given CILA level.
    pub fn new(cila_level: u8) -> Self {
        let (evidence_tx, _) = broadcast::channel(256);
        Self {
            evidence_tx,
            cila_level,
            last_read_file: None,
            last_read_max_cc: None,
            blast_radius_cache: HashMap::new(),
            ann_results_cache: HashMap::new(),
            active_plan_hint: None,
            last_tool_accepted: None,
            last_quality_score: None,
            last_ema_reward: None,
            last_arm_selected: None,
            last_td_error: None,
            arm_effectiveness: HashMap::new(),
            tool_invocation_count: 0,
            consecutive_file_reads: 0,
            prefetch_queue: Vec::new(),
            hook_results: HashMap::new(),
        }
    }

    /// Enqueue file paths predicted by MCTS shadow rollout for background prefetch.
    ///
    /// Paths are relative to the project root. Duplicates are deduplicated so
    /// repeated rollouts on overlapping task sets don't inflate the queue.
    pub fn signal_prefetch_files(&mut self, paths: Vec<String>) {
        for path in paths {
            if !self.prefetch_queue.contains(&path) {
                self.prefetch_queue.push(path);
            }
        }
    }

    /// Drain the prefetch queue, returning all pending paths.
    ///
    /// Callers (e.g. `handle_enter_plan_mode`) drain the queue and forward
    /// each path to `shared::file_prefetch::try_enqueue_prefetch`.
    pub fn drain_prefetch_queue(&mut self) -> Vec<String> {
        std::mem::take(&mut self.prefetch_queue)
    }

    /// Record a file read event (perception → execution path).
    pub fn signal_file_read(&mut self, rel_path: String, max_cc: Option<u16>) {
        self.last_read_file = Some(rel_path);
        self.last_read_max_cc = max_cc;
    }

    /// Cache blast radius for a file (avoids re-query in pre_edit).
    pub fn cache_blast_radius(&mut self, rel_path: &str, count: usize) {
        self.blast_radius_cache.insert(rel_path.to_string(), count);
    }

    /// Get cached blast radius for a file, if available.
    pub fn get_blast_radius(&self, rel_path: &str) -> Option<usize> {
        self.blast_radius_cache.get(rel_path).copied()
    }

    /// Cache ANN search results for a file path query.
    /// Pre-warms pre_edit context without re-querying the ANN index.
    pub fn cache_ann_results(
        &mut self,
        query: &str,
        results: Vec<crate::ann_memory::SearchResult>,
    ) {
        self.ann_results_cache.insert(query.to_string(), results);
    }

    /// Get cached ANN results for a file path query, if available.
    pub fn get_ann_results(&self, query: &str) -> Option<&Vec<crate::ann_memory::SearchResult>> {
        self.ann_results_cache.get(query)
    }

    /// Set the active plan hint from MCTS/decompose (planning → execution).
    pub fn signal_plan_active(&mut self, hint: String) {
        self.active_plan_hint = Some(hint);
    }

    /// Record a tool outcome (execution → learning path).
    pub fn signal_tool_outcome(&mut self, accepted: bool, quality: Option<f64>) {
        self.last_tool_accepted = Some(accepted);
        self.last_quality_score = quality;
        self.tool_invocation_count += 1;
    }

    /// Update arm effectiveness from LinUCB (learning → perception).
    pub fn update_arm_effectiveness(&mut self, arm_id: u8, avg_reward: f64) {
        self.arm_effectiveness.insert(arm_id, avg_reward);
    }

    /// Check if a specific LinUCB arm is currently productive.
    /// Returns `true` if the arm has avg_reward > threshold (default 0.3).
    pub fn is_arm_productive(&self, arm_id: u8) -> bool {
        self.arm_effectiveness
            .get(&arm_id)
            .map(|&r| r > 0.3)
            .unwrap_or(true) // unknown arms assumed productive (exploration)
    }

    /// Clear ephemeral per-turn signals (called between tool invocations).
    pub fn clear_turn_signals(&mut self) {
        self.last_tool_accepted = None;
        self.last_quality_score = None;
        self.last_ema_reward = None;
        self.last_arm_selected = None;
        self.last_td_error = None;
    }

    /// Subscribe to Evidence broadcast events.
    /// Returns a receiver that will receive all future Evidence events.
    pub fn subscribe_evidence(&self) -> broadcast::Receiver<Evidence> {
        self.evidence_tx.subscribe()
    }

    /// Broadcast an Evidence event to all subscribers.
    /// Returns the number of subscribers that received the event.
    pub fn broadcast_evidence(&self, evidence: Evidence) -> usize {
        // `send` returns Err only if there are no receivers; we ignore that case.
        let _ = self.evidence_tx.send(evidence);
        // Count active receivers by attempting to estimate; actual delivery is fire-and-forget.
        self.evidence_tx.receiver_count()
    }

    // ── Hook Result Storage (A8) ─────────────────────────────────────────
    /// Store a hook result for chaining via `ctx.last`.
    ///
    /// Called by each hook's post-phase handler after successful execution.
    /// The next hook of the same family (e.g. post_read after pre_read) can
    /// retrieve it via `get_last_hook_result` to enable cross-hook chaining.
    pub fn add_hook_result(&mut self, hook_name: &'static str, result: serde_json::Value) {
        self.hook_results.insert(hook_name.to_string(), result);
    }

    /// Retrieve the last result from a given hook, if available.
    ///
    /// Returns `Some(value)` if this hook was previously executed in this session,
    /// `None` if the hook hasn't run yet or was called without a result.
    pub fn get_last_hook_result(&self, hook_name: &str) -> Option<&serde_json::Value> {
        self.hook_results.get(hook_name)
    }
}

impl Default for SessionBus {
    fn default() -> Self {
        Self::new(0)
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_with_cila() {
        let bus = SessionBus::new(4);
        assert_eq!(bus.cila_level, 4);
        assert!(bus.last_read_file.is_none());
        assert!(bus.blast_radius_cache.is_empty());
        assert_eq!(bus.tool_invocation_count, 0);
    }

    #[test]
    fn test_signal_file_read() {
        let mut bus = SessionBus::default();
        bus.signal_file_read("src/main.rs".to_string(), Some(12));
        assert_eq!(bus.last_read_file.as_deref(), Some("src/main.rs"));
        assert_eq!(bus.last_read_max_cc, Some(12));
    }

    #[test]
    fn test_blast_radius_cache() {
        let mut bus = SessionBus::default();
        assert!(bus.get_blast_radius("src/lib.rs").is_none());

        bus.cache_blast_radius("src/lib.rs", 7);
        assert_eq!(bus.get_blast_radius("src/lib.rs"), Some(7));
    }

    #[test]
    fn test_signal_tool_outcome() {
        let mut bus = SessionBus::default();
        bus.signal_tool_outcome(true, Some(0.95));
        assert_eq!(bus.last_tool_accepted, Some(true));
        assert_eq!(bus.last_quality_score, Some(0.95));
        assert_eq!(bus.tool_invocation_count, 1);

        bus.signal_tool_outcome(false, None);
        assert_eq!(bus.tool_invocation_count, 2);
    }

    #[test]
    fn test_arm_effectiveness() {
        let mut bus = SessionBus::default();

        // Unknown arm = assumed productive
        assert!(bus.is_arm_productive(0));

        // Low effectiveness arm
        bus.update_arm_effectiveness(0, 0.1);
        assert!(!bus.is_arm_productive(0));

        // High effectiveness arm
        bus.update_arm_effectiveness(1, 0.8);
        assert!(bus.is_arm_productive(1));
    }

    #[test]
    fn test_hook_result_storage() {
        let mut bus = SessionBus::default();
        assert!(bus.get_last_hook_result("pre_read").is_none());

        let result = serde_json::json!({"file_path": "src/main.rs", "context": "some context"});
        bus.add_hook_result("pre_read", result.clone());
        assert_eq!(bus.get_last_hook_result("pre_read"), Some(&result));

        // Different hook name = different result
        assert!(bus.get_last_hook_result("pre_edit").is_none());

        // Overwrite: same hook replaces result
        let result2 = serde_json::json!({"file_path": "src/main.rs", "context": "updated context"});
        bus.add_hook_result("pre_read", result2.clone());
        assert_eq!(bus.get_last_hook_result("pre_read"), Some(&result2));
    }

    #[test]
    fn test_clear_turn_signals() {
        let mut bus = SessionBus::default();
        bus.signal_tool_outcome(true, Some(0.9));
        assert!(bus.last_tool_accepted.is_some());

        bus.clear_turn_signals();
        assert!(bus.last_tool_accepted.is_none());
        assert!(bus.last_quality_score.is_none());
        // S4: RL feedback fields also cleared
        assert!(bus.last_ema_reward.is_none());
        assert!(bus.last_arm_selected.is_none());
        assert!(bus.last_td_error.is_none());
        // Counter should NOT be cleared
        assert_eq!(bus.tool_invocation_count, 1);
    }

    #[test]
    fn test_plan_hint() {
        let mut bus = SessionBus::default();
        assert!(bus.active_plan_hint.is_none());

        bus.signal_plan_active("implement Tarjan SCC for CallGraph".to_string());
        assert_eq!(
            bus.active_plan_hint.as_deref(),
            Some("implement Tarjan SCC for CallGraph")
        );
    }

    #[test]
    fn test_default_is_empty() {
        let bus = SessionBus::default();
        assert!(bus.last_read_file.is_none());
        assert!(bus.active_plan_hint.is_none());
        assert!(bus.last_tool_accepted.is_none());
        assert!(bus.arm_effectiveness.is_empty());
        assert!(bus.blast_radius_cache.is_empty());
        assert!(bus.ann_results_cache.is_empty());
        assert_eq!(bus.cila_level, 0);
        assert_eq!(bus.tool_invocation_count, 0);
        // S4: RL feedback fields default to None
        assert!(bus.last_ema_reward.is_none());
        assert!(bus.last_arm_selected.is_none());
        assert!(bus.last_td_error.is_none());
    }

    #[test]
    fn test_ann_results_cache() {
        use crate::ann_memory::SearchResult;

        let mut bus = SessionBus::default();
        assert!(bus.get_ann_results("src/lib.rs").is_none());

        let results = vec![
            SearchResult::new("mem1".to_string(), 0.95),
            SearchResult::new("mem2".to_string(), 0.87),
        ];
        bus.cache_ann_results("src/lib.rs", results);
        let cached = bus.get_ann_results("src/lib.rs");
        assert!(cached.is_some());
        let cached = cached.expect("cached results exist");
        assert_eq!(cached.len(), 2);
        assert_eq!(cached[0].id, "mem1");
        assert_eq!(cached[0].score, 0.95);
        assert_eq!(cached[1].id, "mem2");
    }

    #[test]
    fn test_ann_results_cache_overwrites() {
        use crate::ann_memory::SearchResult;

        let mut bus = SessionBus::default();
        bus.cache_ann_results(
            "src/lib.rs",
            vec![SearchResult::new("old".to_string(), 0.5)],
        );
        bus.cache_ann_results(
            "src/lib.rs",
            vec![SearchResult::new("new".to_string(), 0.99)],
        );
        let cached = bus.get_ann_results("src/lib.rs").expect("cached");
        assert_eq!(cached.len(), 1);
        assert_eq!(cached[0].id, "new");
    }

    #[test]
    fn test_rl_feedback_fields() {
        // S4: Verify RL feedback fields are directly settable (pub fields)
        let mut bus = SessionBus::default();
        assert!(bus.last_ema_reward.is_none());
        assert!(bus.last_arm_selected.is_none());
        assert!(bus.last_td_error.is_none());

        bus.last_ema_reward = Some(0.75);
        bus.last_arm_selected = Some(3);
        bus.last_td_error = Some(0.12);

        assert_eq!(bus.last_ema_reward, Some(0.75));
        assert_eq!(bus.last_arm_selected, Some(3));
        assert_eq!(bus.last_td_error, Some(0.12));
    }
}
