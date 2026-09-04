//! S6 (2026-09-02) — quality baseline over the PROPOSED content, shared by
//! `pre_write` and `pre_edit`.
//!
//! `pre_write` has measured the content it is about to write since Wave 5
//! (`quality: CC>10: [...]`, avg CC, async ratio). `pre_edit` only measured
//! the file already on disk (`compose_quality_evolution`), so an edit that
//! pushes a function past the complexity threshold was reported one hook
//! late — by `post_edit`, after the damage. This module holds the single
//! measuring function and the `pre_edit` layer that applies the edit in
//! memory and measures the result BEFORE it lands.
//!
//! Latency: one `analyze_file_quality` pass over the simulated file (the
//! same cost `pre_write` already pays). Files above [`MAX_SOURCE_BYTES`]
//! are skipped — the signal is a nudge, not a gate.

use crate::shared::signal_pipeline::{ProposedChange, SignalContext, SignalLayer};

/// Weight of the quality-baseline signal (identical in both hooks).
pub(crate) const QUALITY_BASELINE_SCORE: f32 = 0.8;

/// Largest simulated source the layer is willing to analyse (bytes).
pub(crate) const MAX_SOURCE_BYTES: usize = 200_000;

/// Collect quality-baseline signals (CC, async ratio) from AST metrics.
///
/// Returns a single scored signal at [`QUALITY_BASELINE_SCORE`], or an empty
/// vec when the content is within limits or the language is not analysable.
pub(crate) fn quality_baseline_signals(content: &str, file_path: &str) -> Vec<(f32, String)> {
    let Some(metrics) = crate::ast_bridge::analyze_file_quality(content, file_path) else {
        return Vec::new();
    };
    let mut quality_parts: Vec<String> = Vec::new();
    if !metrics.complex_symbols.is_empty() {
        let names = metrics
            .complex_symbols
            .iter()
            .take(3)
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        quality_parts.push(format!("CC>10: [{}]", names));
    }
    if metrics.avg_complexity > 8.0 {
        quality_parts.push(format!("avg_CC={:.1}", metrics.avg_complexity));
    }
    if metrics.async_count > 0 {
        quality_parts.push(format!("async_ratio={:.0}%", metrics.async_ratio * 100.0));
    }
    if quality_parts.is_empty() {
        Vec::new()
    } else {
        vec![(
            QUALITY_BASELINE_SCORE,
            format!("quality: {}", quality_parts.join(", ")),
        )]
    }
}

/// Apply an Edit in memory: the first occurrence of `old` becomes `new`.
///
/// `None` when the edit cannot be simulated (empty `old`, `old` absent from
/// `source`) — the caller then measures nothing rather than something wrong.
pub(crate) fn apply_edit(source: &str, old: &str, new: &str) -> Option<String> {
    if old.is_empty() || !source.contains(old) {
        return None;
    }
    Some(source.replacen(old, new, 1))
}

/// `pre_edit` layer: the quality baseline of the file AS IT WILL BE after
/// the edit. Computed at construction (the pipeline's layers cannot borrow
/// the runtime); `enrich` only speaks for `ProposedChange::Edit`.
pub(crate) struct QualityBaselineLayer {
    signals: Vec<(f32, String)>,
}

impl QualityBaselineLayer {
    /// Simulate `old → new` over `current_source` and measure the result.
    pub(crate) fn for_edit(rel_path: &str, current_source: &str, old: &str, new: &str) -> Self {
        let signals = match apply_edit(current_source, old, new) {
            Some(simulated) if simulated.len() <= MAX_SOURCE_BYTES => {
                quality_baseline_signals(&simulated, rel_path)
            }
            _ => Vec::new(),
        };
        Self { signals }
    }

    /// `true` when the simulated file is within limits (nothing to say).
    pub(crate) fn is_empty(&self) -> bool {
        self.signals.is_empty()
    }
}

impl SignalLayer for QualityBaselineLayer {
    fn name(&self) -> &'static str {
        "quality_baseline"
    }

    fn enrich(&self, ctx: &SignalContext<'_>) -> Vec<(f32, String)> {
        match ctx.proposed {
            Some(ProposedChange::Edit { .. }) => self.signals.clone(),
            _ => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn branchy(ifs: usize) -> String {
        let mut body = String::from("pub fn branchy(x: i32) -> i32 {\n    let mut acc = 0;\n");
        for i in 0..ifs {
            body.push_str(&format!("    if x > {i} {{ acc += {i}; }}\n"));
        }
        body.push_str("    acc\n}\n");
        body
    }

    #[test]
    fn apply_edit_replaces_the_first_occurrence_only() {
        assert_eq!(apply_edit("a b a", "a", "z").as_deref(), Some("z b a"));
        assert_eq!(apply_edit("a b", "", "z"), None, "empty old cannot be simulated");
        assert_eq!(apply_edit("a b", "q", "z"), None, "absent old cannot be simulated");
    }

    #[test]
    fn simple_function_is_within_limits() {
        assert!(quality_baseline_signals("pub fn f(x: i32) -> i32 { x }\n", "src/f.rs").is_empty());
    }

    #[test]
    fn proposed_branchy_function_is_named_past_the_threshold() {
        let current = "pub fn branchy(x: i32) -> i32 {\n    x\n}\n";
        let layer = QualityBaselineLayer::for_edit("src/b.rs", current, current, &branchy(14));
        assert!(!layer.is_empty());
        let ctx = crate::shared::signal_pipeline::context_for_edit("src/b.rs", current, "", 2);
        let out = layer.enrich(&ctx);
        assert_eq!(out.len(), 1);
        assert!(out[0].1.contains("quality: CC>10"), "{}", out[0].1);
        assert!(out[0].1.contains("branchy"), "{}", out[0].1);
        assert_eq!(out[0].0, QUALITY_BASELINE_SCORE);
    }

    #[test]
    fn layer_is_silent_for_a_write_context() {
        let current = "pub fn branchy(x: i32) -> i32 {\n    x\n}\n";
        let proposed = branchy(14);
        let layer = QualityBaselineLayer::for_edit("src/b.rs", current, current, &proposed);
        let ctx = crate::shared::signal_pipeline::context_for_write("src/b.rs", &proposed, 2);
        assert!(layer.enrich(&ctx).is_empty(), "an Edit measurement never speaks for a Write");
    }
}
