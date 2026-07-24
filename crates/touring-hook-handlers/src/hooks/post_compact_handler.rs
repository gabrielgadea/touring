//! PostCompact handler — rebuilds caches after context compaction.
//!
//! After context compaction, the result cache may be stale because the LLM's
//! working context was trimmed. This handler re-warms the cache for the most
//! accessed files so that subsequent pre-read hooks have fresh context ready.
//!
//! Runs as lifecycle hook (always-on). Target latency: <50ms.

use serde_json::Value;

use crate::hook_decompose_bridge::bridge_precompact_checkpoint;
use crate::runtime::HookRuntime;
use crate::schemas::validate_payload;

/// Handle post-compact events by re-warming the result cache.
///
/// Queries the top accessed files from the knowledge graph and pre-computes
/// context for any that are not already cached. This mirrors the session-start
/// pre-warming logic but runs after compaction events.
#[tracing::instrument(skip(rt, _input), fields(hook = "post_compact"))]
pub fn run(rt: &mut HookRuntime, _input: &Value) -> String {
    // D9: Validate payload with typed schema — skip silently on malformed input.
    if validate_payload::<crate::schemas::PostCompactPayload>(_input).is_err() {
        return String::new();
    }

    const MAX_PREWARM: usize = 10;

    // HDG-5: checkpoint decompose state BEFORE cache re-warm
    if let Err(e) = bridge_precompact_checkpoint(rt) {
        tracing::debug!(error = %e, "post_compact: bridge_precompact_checkpoint failed");
    }

    // Query most accessed files
    let files = match rt.ctx.knowledge.top_accessed_files(MAX_PREWARM) {
        Ok(f) => f,
        Err(e) => {
            tracing::debug!(error = %e, "post_compact: could not query top files");
            return String::new();
        }
    };

    // `warmed` is only mutated inside the `#[cfg(feature = "pre-hooks")]` block
    // below; without that feature the binding stays 0, so `mut` is unused.
    #[cfg_attr(not(feature = "pre-hooks"), allow(unused_mut))]
    let mut warmed = 0usize;

    // HRC-WIRE: verify HookResultCache entries for top-N frequently-edited files
    // Fire-and-forget: logging does not block post_compact
    // Batch limit: 50 files max per post_compact to avoid latency
    for file_path in files.iter().take(50) {
        // Check pre_read cache entry
        if let Some(result) = rt.ctx.result_cache.get_result("pre_read", file_path) {
            tracing::debug!(file = %file_path, bytes = result.len(), "HookResultCache pre_read hit");
        }
        // Check pre_edit cache entry
        if let Some(result) = rt.ctx.result_cache.get_result("pre_edit", file_path) {
            tracing::debug!(file = %file_path, bytes = result.len(), "HookResultCache pre_edit hit");
        }
    }

    #[cfg(feature = "pre-hooks")]
    {
        for file_path in &files {
            // Skip if already cached
            if rt
                .ctx
                .result_cache
                .get_result("pre-read", file_path)
                .is_some()
            {
                continue;
            }
            if let Some(ctx) = crate::pre_read::compose_high_signal_context_budgeted(
                &rt.ctx.knowledge,
                file_path,
                crate::pre_read::DEFAULT_CONTEXT_BUDGET,
                0,
            ) {
                rt.ctx.result_cache.cache_result("pre-read", file_path, ctx);
                warmed += 1;
            }
        }
    }

    tracing::debug!(
        candidates = files.len(),
        warmed,
        "post_compact: cache re-warmed"
    );

    // S3b: Evaporate resolved GoT pheromone trails — fades stale failure patterns.
    // Reads from got_snapshot pheromone_trails and applies decay so old failures
    // become less penalising over time (evaporation rate ~0.95).
    if let Some(ref mut snapshot) = rt.got_snapshot {
        const EVAPORATION_RATE: f64 = 0.95;
        snapshot.pheromone_trails.retain(|_, strength| {
            *strength *= EVAPORATION_RATE;
            *strength > 0.01
        });
    }

    // HEB-WIRE: flush accumulated ACO streaming events after cache re-warm.
    // Fire-and-forget: drop events if quality_assessment is unavailable.
    if let Some(ref mut qa) = rt.ctx.quality_assessment {
        let events = rt.ctx.event_buffer.flush();
        for event_json in events {
            if let Ok(outcome) = serde_json::from_str::<crate::aco_bridge::HookOutcome>(&event_json)
            {
                qa.record(outcome);
            }
        }
    }

    String::new()
}
