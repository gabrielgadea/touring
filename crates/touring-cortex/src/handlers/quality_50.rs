//! H54 — `TouringQualityScoreHandler` (added 2026-06-25, harness-consolidation W6).
//!
//! Bridges `touring-quality`'s 50-dim scoring engine into the cortex hook
//! pipeline. Runs AFTER the existing H51/H52/H53 quality handlers so it
//! can fuse their signals with the 50-dim composite. Feeds the X7.5
//! QUALITY-SIGNAL in CEG (`touring-ceg::gateway::quality_signal`).
//!
//! ## Design
//!
//! - **PreToolUse[Write|Edit]**: advisory only — when composite < 0.80,
//!   push a `QGate50[...]` line into `context_lines` so the LLM sees
//!   it in the additionalContext block. **Does NOT block** — H51 already
//!   blocks on lint regressions; H54 augments with 50-dim severity but
//!   doesn't double-block (would be too noisy).
//! - **PostToolUse[Write|Edit]**: `Skip` (no post-write advisory needed;
//!   the cache is updated for the next pre-write comparison).
//!
//! ## Fail-open
//!
//! If `touring-quality::score_target` errors (file unreadable, verifier
//! crash), the handler returns `skip` — never breaks the tool call. The
//! existing H51/H52 ruff pipeline still runs.

use std::path::PathBuf;

use moka::sync::Cache;
use serde_json::json;

use crate::context::CortexContext;
use crate::handler::Handler;
use crate::types::{HandlerResult, HookEvent};

/// Gold-tier threshold (matches touring-quality's `tier_from_composite`
/// boundary). Below this → warn.
const GOLD_THRESHOLD: f32 = 0.80;

/// One record per scored file (path → composite score).
type ScoreCache = Cache<PathBuf, f32>;

/// **H54** — 50-dim quality scoring bridge. Pairs with H51/H52/H53 by
/// emitting a score each time a Write/Edit touches a file, so the cortex
/// has the **same 50-dim signal** that CEG X7.5 consumes for its
/// quality-penalty modulation.
pub struct TouringQualityScoreHandler {
    cache: ScoreCache,
}

impl Default for TouringQualityScoreHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl TouringQualityScoreHandler {
    /// New handler with a 10K-entry moka cache (matches H51 sizing).
    #[must_use]
    pub fn new() -> Self {
        Self {
            cache: Cache::new(10_000),
        }
    }

    /// Score a file and persist the latest composite in the cache.
    fn score_and_cache(&self, path: &std::path::Path) -> Option<f32> {
        let report =
            touring_quality::score_target(path, &[], touring_quality::OutputFormat::Json).ok()?;
        let composite = report.composite.clamp(0.0, 1.0);
        self.cache.insert(path.to_path_buf(), composite);
        Some(composite)
    }

    /// Compose a `QGate50[...]` advisory line (advisory only — does not
    /// block the call; the existing H51/H52 ruff pipeline retains BLOCK).
    fn advisory_line(path_str: &str, composite: f32) -> String {
        let deficit = GOLD_THRESHOLD - composite;
        format!(
            "QGate50[{path_str}]: 50-dim composite {composite:.3} is {deficit:.3} below \
             Gold tier ({GOLD_THRESHOLD:.2}); run `touring-quality score {path_str}` \
             for the per-dim breakdown and `~/.claude/rules/elite-50-quality.md` \
             for the canonical D-rule remediations."
        )
    }
}

impl Handler for TouringQualityScoreHandler {
    fn name(&self) -> &str {
        "TouringQualityScore"
    }

    fn priority(&self) -> u8 {
        // Run AFTER H51 (CodeStandards) which uses priority ~16-20 in the
        // central hook registry; H54 runs at 22 so its score augments
        // (never preempts) the ruff-lint verdict.
        22
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::PreToolUse, HookEvent::PostToolUse]
    }

    fn tool_matcher(&self) -> Option<&str> {
        Some("Write|Edit|MultiEdit")
    }

    fn execute(&self, ctx: &mut CortexContext) -> HandlerResult {
        // Use `file_path` when the framework extracted it; fall back to a
        // hand-parse of `tool_input.file_path` / `path` / `filepath`.
        let path_str: Option<String> = ctx.file_path.clone().or_else(|| {
            ctx.tool_input
                .get("file_path")
                .or_else(|| ctx.tool_input.get("path"))
                .or_else(|| ctx.tool_input.get("filepath"))
                .and_then(|p| p.as_str())
                .map(String::from)
        });

        let Some(path_str) = path_str else {
            return HandlerResult::skip(self.name());
        };
        let path = std::path::PathBuf::from(&path_str);
        if !path.exists() {
            // Create-style operation (new file) — score when post-write fires.
            return HandlerResult::skip(self.name());
        }

        match ctx.event {
            HookEvent::PreToolUse => {
                let Some(composite) = self.score_and_cache(&path) else {
                    return HandlerResult::skip(self.name());
                };
                if composite < GOLD_THRESHOLD {
                    // Advisory only — push context line, do NOT block.
                    // H51 already blocks on lint regressions; H54 augments.
                    let line = Self::advisory_line(&path_str, composite);
                    let mut result = HandlerResult::skip(self.name());
                    result.context_lines.push(line);
                    result
                        .metrics
                        .as_object_mut()
                        .expect("metrics is Value::Object")
                        .insert(
                            "qgate50".to_string(),
                            json!({
                                "composite": composite,
                                "threshold": GOLD_THRESHOLD,
                                "tier": "below-gold",
                            }),
                        );
                    result
                } else {
                    HandlerResult::skip(self.name())
                }
            }
            HookEvent::PostToolUse => {
                // Score after the write/edit; cache for next pre-write comparison.
                // No advisory needed on post-write — H51/H52 already emit QGate.
                let _ = self.score_and_cache(&path);
                HandlerResult::skip(self.name())
            }
            _ => HandlerResult::skip(self.name()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::HookEvent;

    #[test]
    fn name_is_canonical() {
        let h = TouringQualityScoreHandler::new();
        assert_eq!(h.name(), "TouringQualityScore");
    }

    #[test]
    fn priority_in_post_ruff_band() {
        let h = TouringQualityScoreHandler::new();
        // H51 is in the ruff-lint priority band (typically 16-20).
        // H54 must run AFTER H51 so it augments, never preempts.
        assert!(h.priority() > 16, "H54 must run AFTER H51");
    }

    #[test]
    fn events_cover_pre_and_post_write() {
        let h = TouringQualityScoreHandler::new();
        let ev = h.events();
        assert!(ev.contains(&HookEvent::PreToolUse));
        assert!(ev.contains(&HookEvent::PostToolUse));
    }

    #[test]
    fn tool_matcher_filters_to_writes_and_edits() {
        let h = TouringQualityScoreHandler::new();
        let m = h.tool_matcher().unwrap_or("");
        assert!(m.contains("Write"));
        assert!(m.contains("Edit"));
    }

    #[test]
    fn score_cache_empty_for_unknown_path() {
        let h = TouringQualityScoreHandler::new();
        assert!(h.cache.get(&PathBuf::from("/nonexistent")).is_none());
    }

    #[test]
    fn gold_threshold_matches_touring_quality_default() {
        // touring_quality::tier_from_composite uses >= 0.80 for Gold;
        // H54 must align exactly.
        assert!((GOLD_THRESHOLD - 0.80).abs() < 1e-6);
    }

    #[test]
    fn advisory_line_includes_composite_and_threshold() {
        let line = TouringQualityScoreHandler::advisory_line("/tmp/x.rs", 0.62);
        assert!(line.contains("0.620"));
        assert!(line.contains("Gold"));
        assert!(line.contains("0.180")); // deficit = 0.80 - 0.62 = 0.18
        assert!(line.contains("/tmp/x.rs"));
    }
}
