//! AutoSaveHook — Interval-based checkpointing for Touring hooks.
//!
//! Replaces shell-based `mempal_save_hook.sh` with pure-Rust implementation.
//! Tracks tool exchanges and fires checkpoint when interval threshold reached.
//!
//! ## Design
//! - Block-and-reason pattern: save blocks until complete, then reason about outcome
//! - Interval = 15 tool exchanges (configurable)
//! - Reset on session-start to begin fresh tracking
//! - Integrated with hook_memory LRU via TTL cleanup (triggered on auto-save)

use rusqlite::Connection;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

/// AutoSave configuration and state.
/// Held in HookRuntime as a Mutex-wrapped field for interior mutability.
pub struct AutoSaveConfig {
    /// Number of tool exchanges since last auto-save.
    pub exchange_count: u32,
    /// Number of exchanges required before auto-save fires.
    pub interval: u32,
    /// Unix timestamp (seconds) of last auto-save.
    pub last_save_ts: u64,
}

impl Default for AutoSaveConfig {
    fn default() -> Self {
        Self {
            exchange_count: 0,
            interval: 15,
            last_save_ts: 0,
        }
    }
}

impl AutoSaveConfig {
    /// Create a new config with the default 15-exchange interval.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a config with a custom interval.
    pub fn with_interval(interval: u32) -> Self {
        Self {
            interval,
            ..Self::default()
        }
    }

    /// Increment the exchange counter and return whether threshold is reached.
    /// Call this on every post_tool_rl invocation.
    pub fn increment(&mut self) -> bool {
        self.exchange_count += 1;
        self.exchange_count >= self.interval
    }

    /// Reset counter and update timestamp.
    /// Call this after a successful auto-save.
    pub fn mark_saved(&mut self) {
        self.exchange_count = 0;
        self.last_save_ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
    }

    /// Reset counter (session-start path, no timestamp update needed yet).
    pub fn reset(&mut self) {
        self.exchange_count = 0;
    }

    /// Return seconds since last save (0 if never saved).
    pub fn seconds_since_save(&self) -> u64 {
        if self.last_save_ts == 0 {
            return 0;
        }
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.saturating_sub(std::time::Duration::from_secs(self.last_save_ts)))
            .unwrap_or_default()
            .as_secs()
    }
}

/// AutoSaveHook — orchestrates interval-based checkpointing.
pub struct AutoSaveHook {
    config: Mutex<AutoSaveConfig>,
}

impl AutoSaveHook {
    /// Create a new AutoSaveHook with default 15-exchange interval.
    pub fn new() -> Self {
        Self {
            config: Mutex::new(AutoSaveConfig::new()),
        }
    }

    /// Create with custom interval.
    pub fn with_interval(interval: u32) -> Self {
        Self {
            config: Mutex::new(AutoSaveConfig::with_interval(interval)),
        }
    }

    /// Increment exchange counter.
    /// Returns true if threshold reached (save should fire).
    pub fn increment_exchange(&self) -> bool {
        let mut cfg = match self.config.lock() {
            Ok(c) => c,
            Err(e) => e.into_inner(),
        };
        cfg.increment()
    }

    /// Reset counter (session-start).
    pub fn reset(&self) {
        if let Ok(mut cfg) = self.config.lock() {
            cfg.reset();
        }
    }

    /// Mark save complete.
    pub fn mark_saved(&self) {
        if let Ok(mut cfg) = self.config.lock() {
            cfg.mark_saved();
        }
    }

    /// Get current exchange count.
    pub fn exchange_count(&self) -> u32 {
        self.config
            .lock()
            .ok()
            .map(|c| c.exchange_count)
            .unwrap_or(0)
    }

    /// Get configured interval.
    pub fn interval(&self) -> u32 {
        self.config.lock().ok().map(|c| c.interval).unwrap_or(15)
    }

    /// Check if auto-save should fire (called from run_auto_save).
    /// Returns (should_fire, current_count, interval).
    pub fn should_fire(&self) -> (bool, u32, u32) {
        match self.config.lock() {
            Ok(cfg) => {
                let count = cfg.exchange_count;
                let interval = cfg.interval;
                (count >= interval, count, interval)
            }
            _ => (false, 0, 15),
        }
    }

    /// Fire auto-save: call checkpoint hook and record result.
    /// Returns Ok(()) on success, Err(message) on failure.
    /// This is the block-and-reason entry point.
    ///
    /// Also triggers LRU eviction on the knowledge DB to keep memory lean.
    pub fn run_auto_save(
        &self,
        runtime: &crate::runtime::HookRuntime,
    ) -> Result<(), crate::hook_runtime::HookPersistError> {
        let (should_fire, count, interval) = self.should_fire();
        if !should_fire {
            return Ok(());
        }

        tracing::debug!(
            exchange_count = count,
            interval = interval,
            "AutoSaveHook firing checkpoint"
        );

        // Write checkpoint directly to file system (avoids &mut borrow on runtime)
        let checkpoint_dir = runtime.project_root.join(".claude/checkpoints");
        let _ = std::fs::create_dir_all(&checkpoint_dir);
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let checkpoint_path = checkpoint_dir.join(format!("auto_save_{}.json", ts));
        let payload = serde_json::json!({
            "checkpoint_type": "auto_save",
            "exchange_count": count,
            "interval": interval,
            "source": "auto_save_hook",
            "timestamp": ts,
        });
        let result = match std::fs::write(
            &checkpoint_path,
            serde_json::to_string_pretty(&payload).unwrap_or_default(),
        ) {
            Ok(_) => {
                serde_json::json!({"saved": true, "path": checkpoint_path.display().to_string()})
                    .to_string()
            }
            Err(e) => serde_json::json!({"saved": false, "error": e.to_string()}).to_string(),
        };

        // Parse result to check success
        let parsed: serde_json::Value = result
            .parse()
            .map_err(|e| format!("checkpoint parse error: {e}"))?;
        if parsed
            .get("saved")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            self.mark_saved();

            // P4.4: Trigger LRU eviction on auto-save to keep memory lean.
            // Eviction runs TTL cleanup + working→semantic promotion.
            // Use direct SQLite to avoid trait object indirection.
            let conn = runtime.ctx.knowledge.conn_ref();
            if let Err(e) = Self::trigger_lru_eviction(conn) {
                tracing::warn!("LRU eviction failed after auto-save: {e}");
            }

            tracing::info!(exchange_count = count, "AutoSaveHook checkpoint complete");
            Ok(())
        } else {
            let error = parsed
                .get("error")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            Err(crate::hook_runtime::HookPersistError(format!(
                "checkpoint failed: {error}"
            )))
        }
    }

    /// Trigger LRU eviction: TTL cleanup + working→semantic promotion.
    /// Uses direct SQLite access via knowledge connection.
    fn trigger_lru_eviction(conn: &rusqlite::Connection) -> Result<(), String> {
        use crate::hook_memory::{
            EPHEMERAL_TTL_HOURS, TABLE_HOOK_EVENTS, TABLE_INTENT_CLASS, TABLE_PATTERNS_EPHEMERAL,
            TABLE_PATTERNS_WORKING, TABLE_QUALITY_SIGNALS, TABLE_TASK_COMPLETIONS,
            WORKING_TTL_HOURS,
        };

        // Aggregate ephemeral → working before cleanup
        let _ = Self::aggregate_ephemeral_to_working(conn);

        // Clean ephemeral (1h TTL)
        let _ = conn.execute(
            &format!("DELETE FROM {TABLE_HOOK_EVENTS} WHERE created_at <= datetime('now', '-{EPHEMERAL_TTL_HOURS} hours')"),
            [],
        );
        let _ = conn.execute(
            &format!("DELETE FROM {TABLE_PATTERNS_EPHEMERAL} WHERE created_at <= datetime('now', '-{EPHEMERAL_TTL_HOURS} hours')"),
            [],
        );
        let _ = conn.execute(
            &format!("DELETE FROM {TABLE_INTENT_CLASS} WHERE created_at <= datetime('now', '-{EPHEMERAL_TTL_HOURS} hours')"),
            [],
        );
        let _ = conn.execute(
            &format!("DELETE FROM {TABLE_QUALITY_SIGNALS} WHERE created_at <= datetime('now', '-{EPHEMERAL_TTL_HOURS} hours')"),
            [],
        );
        let _ = conn.execute(
            &format!("DELETE FROM {TABLE_TASK_COMPLETIONS} WHERE created_at <= datetime('now', '-{EPHEMERAL_TTL_HOURS} hours')"),
            [],
        );

        // Clean working patterns (24h TTL)
        let _ = conn.execute(
            &format!("DELETE FROM {TABLE_PATTERNS_WORKING} WHERE created_at <= datetime('now', '-{WORKING_TTL_HOURS} hours')"),
            [],
        );

        // Promote eligible working → semantic
        let _ = Self::promote_working_to_semantic(conn);

        Ok(())
    }

    /// Aggregate ephemeral pattern events into working patterns.
    fn aggregate_ephemeral_to_working(conn: &Connection) -> Result<(), String> {
        use crate::hook_memory::TABLE_PATTERNS_EPHEMERAL;
        use crate::hook_memory::TABLE_PATTERNS_WORKING;

        conn.execute(
            &format!(
                "INSERT INTO {TABLE_PATTERNS_WORKING} (pattern_key, sample_count, avg_reward, avg_latency_ms,
                  intent_distribution, first_seen, last_seen, confidence, created_at)
                 SELECT pattern_key, sample_count, avg_reward, avg_latency_ms,
                        intent_distribution, first_seen, last_seen, confidence, datetime('now')
                 FROM {TABLE_PATTERNS_EPHEMERAL}
                 WHERE pattern_key NOT IN (SELECT pattern_key FROM {TABLE_PATTERNS_WORKING})
                 ON CONFLICT(pattern_key) DO UPDATE SET
                    sample_count = sample_count + excluded.sample_count,
                    avg_reward = (avg_reward * sample_count + excluded.avg_reward * excluded.sample_count) / (sample_count + excluded.sample_count),
                    avg_latency_ms = (avg_latency_ms * sample_count + excluded.avg_latency_ms * excluded.sample_count) / (sample_count + excluded.sample_count),
                    last_seen = excluded.last_seen,
                    confidence = MAX(confidence, excluded.confidence)",
            ),
            [],
        ).map_err(|e| format!("aggregate ephemeral→working failed: {e}"))?;

        Ok(())
    }

    /// Promote eligible working patterns to semantic tier.
    fn promote_working_to_semantic(conn: &Connection) -> Result<(), String> {
        use crate::hook_memory::PROMOTION_MIN_AVG_REWARD;
        use crate::hook_memory::PROMOTION_MIN_CONFIDENCE;
        use crate::hook_memory::PROMOTION_MIN_SAMPLES;
        use crate::hook_memory::TABLE_PATTERNS_SEMANTIC;
        use crate::hook_memory::TABLE_PATTERNS_WORKING;

        conn.execute(
            &format!(
                "INSERT INTO {TABLE_PATTERNS_SEMANTIC} (pattern_key, sample_count, avg_reward, avg_latency_ms,
                  intent_distribution, first_seen, last_seen, confidence)
                 SELECT pattern_key, sample_count, avg_reward, avg_latency_ms,
                        intent_distribution, first_seen, last_seen, confidence
                 FROM {TABLE_PATTERNS_WORKING}
                 WHERE sample_count >= {PROMOTION_MIN_SAMPLES}
                   AND avg_reward >= {PROMOTION_MIN_AVG_REWARD}
                   AND confidence >= {PROMOTION_MIN_CONFIDENCE}
                   AND pattern_key NOT IN (SELECT pattern_key FROM {TABLE_PATTERNS_SEMANTIC})
                 ON CONFLICT(pattern_key) DO UPDATE SET
                    sample_count = excluded.sample_count,
                    avg_reward = (avg_reward * sample_count + excluded.avg_reward * excluded.sample_count) / (sample_count + excluded.sample_count),
                    avg_latency_ms = (avg_latency_ms * sample_count + excluded.avg_latency_ms * excluded.sample_count) / (sample_count + excluded.sample_count),
                    last_seen = excluded.last_seen,
                    confidence = MAX(confidence, excluded.confidence)",
            ),
            [],
        ).map_err(|e| format!("promote working→semantic failed: {e}"))?;

        Ok(())
    }
}

impl Default for AutoSaveHook {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_autosave_config_default_interval() {
        let cfg = AutoSaveConfig::new();
        assert_eq!(cfg.interval, 15);
        assert_eq!(cfg.exchange_count, 0);
    }

    #[test]
    fn test_autosave_config_custom_interval() {
        let cfg = AutoSaveConfig::with_interval(30);
        assert_eq!(cfg.interval, 30);
    }

    #[test]
    fn test_increment_returns_true_at_threshold() {
        let mut cfg = AutoSaveConfig::with_interval(3);
        assert!(!cfg.increment()); // 1
        assert!(!cfg.increment()); // 2
        assert!(cfg.increment()); // 3 -> true
    }

    #[test]
    fn test_mark_saved_resets_counter() {
        let mut cfg = AutoSaveConfig::new();
        cfg.exchange_count = 14;
        cfg.mark_saved();
        assert_eq!(cfg.exchange_count, 0);
        assert!(cfg.last_save_ts > 0);
    }

    #[test]
    fn test_reset_clears_count() {
        let mut cfg = AutoSaveConfig::new();
        cfg.exchange_count = 10;
        cfg.reset();
        assert_eq!(cfg.exchange_count, 0);
    }

    #[test]
    fn test_autosave_hook_increment_exchange() {
        let hook = AutoSaveHook::with_interval(2);
        assert!(!hook.increment_exchange());
        assert!(hook.increment_exchange()); // at threshold
    }

    #[test]
    fn test_autosave_hook_reset() {
        let hook = AutoSaveHook::with_interval(5);
        hook.increment_exchange();
        hook.increment_exchange();
        hook.reset();
        assert_eq!(hook.exchange_count(), 0);
    }
}
