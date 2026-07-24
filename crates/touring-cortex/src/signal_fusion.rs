//! Bayesian signal fusion for combining cortex handler outputs.
//!
//! When multiple handlers produce confidence scores for the same event,
//! this module fuses them into a single calibrated estimate using
//! `touring_simd::statistics::reconciliation::bayesian_fusion`.
//!
//! # Use Cases
//!
//! - Combining H91 (HybridReasoning), H92 (AdaptiveContext), H93 (HybridCognitive)
//!   scores into a single context enrichment quality score
//! - Fusing multiple reranker signals for optimal context ordering
//! - Aggregating handler-level confidence into pipeline-level confidence
//!
//! # Algorithm
//!
//! Bayesian fusion treats each handler's score as an estimate with a confidence
//! weight. The fused result is the confidence-weighted mean, with uncertainty
//! quantified via coefficient of variation.

use touring_simd::simd_utils::matrix::softmax as simd_softmax;
use touring_simd::simd_utils::{argmax_f32, reduce_max_f32, reduce_min_f32, reduce_sum_f32};
use touring_simd::statistics::reconciliation::{
    bayesian_fusion, coefficient_of_variation, reconcile_weighted,
};

/// A single handler's signal with confidence.
#[derive(Debug, Clone)]
pub struct HandlerSignal {
    /// Handler name (for diagnostics).
    pub handler_name: String,
    /// Estimated value (e.g., relevance score 0.0–1.0).
    pub estimate: f64,
    /// Confidence in the estimate (0.0–1.0). Higher = more trust.
    pub confidence: f64,
}

/// Result of fusing multiple handler signals.
#[derive(Debug, Clone)]
pub struct FusedSignal {
    /// Bayesian-fused estimate (confidence-weighted mean).
    pub fused_estimate: f64,
    /// Fused confidence (sum of input confidences, normalized).
    pub fused_confidence: f64,
    /// Coefficient of variation — measures disagreement among handlers.
    /// Low CV (<0.2) = handlers agree. High CV (>0.5) = significant disagreement.
    pub disagreement_cv: f64,
    /// Number of signals fused.
    pub signal_count: usize,
    /// Whether the result is high-confidence (confidence > 0.7 AND cv < 0.3).
    pub is_high_confidence: bool,
}

/// Fuse multiple handler signals using Bayesian fusion.
///
/// Each signal is an (estimate, confidence) pair. The fused result is the
/// confidence-weighted mean of all estimates.
///
/// # Example
///
/// ```
/// use touring_cortex::signal_fusion::{fuse_signals, HandlerSignal};
///
/// let signals = vec![
///     HandlerSignal { handler_name: "H91".into(), estimate: 0.8, confidence: 0.9 },
///     HandlerSignal { handler_name: "H92".into(), estimate: 0.7, confidence: 0.6 },
///     HandlerSignal { handler_name: "H93".into(), estimate: 0.85, confidence: 0.8 },
/// ];
/// let fused = fuse_signals(&signals);
/// assert!(fused.fused_estimate > 0.7);
/// assert!(fused.is_high_confidence);
/// ```
pub fn fuse_signals(signals: &[HandlerSignal]) -> FusedSignal {
    if signals.is_empty() {
        return FusedSignal {
            fused_estimate: 0.0,
            fused_confidence: 0.0,
            disagreement_cv: 0.0,
            signal_count: 0,
            is_high_confidence: false,
        };
    }

    // Build (estimate, confidence) pairs for bayesian_fusion
    let pairs: Vec<(f64, f64)> = signals.iter().map(|s| (s.estimate, s.confidence)).collect();

    let (fused_est, fused_conf) = bayesian_fusion(&pairs);

    // Compute disagreement using CV of raw estimates
    let estimates: Vec<f64> = signals.iter().map(|s| s.estimate).collect();
    let cv = coefficient_of_variation(&estimates);

    let is_high_confidence = fused_conf > 0.7 && cv < 0.3;

    FusedSignal {
        fused_estimate: fused_est,
        fused_confidence: fused_conf.min(1.0),
        disagreement_cv: cv,
        signal_count: signals.len(),
        is_high_confidence,
    }
}

/// Fuse signals with explicit weights per handler (for priority-based fusion).
///
/// Weights are applied as multipliers on confidence. A handler with weight 2.0
/// has twice the influence of one with weight 1.0.
pub fn fuse_signals_weighted(signals: &[HandlerSignal], weights: &[f64]) -> FusedSignal {
    if signals.is_empty() {
        return fuse_signals(&[]);
    }

    let weighted_signals: Vec<HandlerSignal> = signals
        .iter()
        .zip(weights.iter().chain(std::iter::repeat(&1.0)))
        .map(|(s, w)| HandlerSignal {
            handler_name: s.handler_name.clone(),
            estimate: s.estimate,
            confidence: s.confidence * w,
        })
        .collect();

    fuse_signals(&weighted_signals)
}

/// Reconcile handler estimates using weighted mean + CV.
///
/// Simpler than Bayesian fusion — just weighted average with disagreement metric.
/// Use when confidences are uniform and weights represent handler priority.
pub fn reconcile_handler_scores(scores: &[f64], weights: &[f64]) -> (f64, f64) {
    reconcile_weighted(scores, weights)
}

// ─────────────────────────────────────────────────────────────────────────────
// Softmax-weighted fusion & signal ranking (SIMD-accelerated, 2026-04-12)
// ─────────────────────────────────────────────────────────────────────────────

/// Softmax-weighted fusion of handler signals.
///
/// Treats each handler's `estimate` as a logit, applies SIMD-stable softmax
/// (`touring_simd::simd_utils::matrix::softmax` — max-subtracted for numerical
/// stability), and returns the probability-weighted fused estimate plus the
/// full distribution.
///
/// # When to use vs. `fuse_signals`
///
/// - Use [`fuse_signals`] (Bayesian) when handlers report independent,
///   calibrated confidence values and you want a confidence-weighted mean.
/// - Use [`fuse_signals_softmax`] when handlers report *logits* (raw scores
///   on a heterogeneous scale) and you want an attention-style weighting
///   that amplifies the dominant signal. Common in hybrid-retrieval rerank
///   where BM25 + cognitive score + access_count are heterogeneous.
#[derive(Debug, Clone, Default)]
pub struct SoftmaxFusion {
    /// Softmax probability per input signal (sums to 1.0). f64 for downstream
    /// Bayesian compat; internally computed in f32 via SIMD then upcast.
    pub probabilities: Vec<f64>,
    /// Probability-weighted fused estimate (Σ pᵢ · estimateᵢ).
    pub fused_estimate: f64,
    /// Index of the dominant signal (argmax of estimates).
    pub dominant_index: Option<usize>,
    /// Max probability (0..1). Close to 1.0 means one signal dominates; close
    /// to 1/N means signals are near-uniform.
    pub max_probability: f64,
    /// Shannon entropy of the distribution (natural log). High entropy =
    /// uniform disagreement; low entropy = one signal confidently wins.
    pub entropy: f64,
    /// Number of signals fused.
    pub signal_count: usize,
}

/// Fuse multiple handler signals using SIMD softmax over estimates.
///
/// Returns an attention-style distribution that downstream consumers (e.g.,
/// rerankers, context enrichers) can treat as probabilities for weighted
/// selection or mixing.
#[must_use]
pub fn fuse_signals_softmax(signals: &[HandlerSignal]) -> SoftmaxFusion {
    if signals.is_empty() {
        return SoftmaxFusion::default();
    }

    // Extract estimates as f32 logits for the SIMD pipeline.
    let logits: Vec<f32> = signals.iter().map(|s| s.estimate as f32).collect();
    let dominant = argmax_f32(&logits);

    let probs_f32 = simd_softmax(&logits);

    // Weighted fusion: Σ pᵢ · estimateᵢ (done in f64 to avoid precision loss).
    let mut fused = 0.0f64;
    let mut max_p = 0.0f64;
    let mut entropy = 0.0f64;
    let probabilities: Vec<f64> = probs_f32
        .iter()
        .zip(signals.iter())
        .map(|(&p, sig)| {
            let p64 = p as f64;
            fused += p64 * sig.estimate;
            if p64 > max_p {
                max_p = p64;
            }
            // Entropy: -Σ p·ln(p). Skip p=0 to avoid NaN.
            if p64 > f64::EPSILON {
                entropy -= p64 * p64.ln();
            }
            p64
        })
        .collect();

    SoftmaxFusion {
        probabilities,
        fused_estimate: fused,
        dominant_index: dominant,
        max_probability: max_p,
        entropy,
        signal_count: signals.len(),
    }
}

/// The dominant (highest-estimate) signal plus its rank metadata.
#[derive(Debug, Clone)]
pub struct DominantSignal {
    /// Index of the winning signal in the input slice.
    pub index: usize,
    /// Handler name of the winner (cloned for convenience).
    pub handler_name: String,
    /// Winning estimate value.
    pub estimate: f64,
    /// Margin over runner-up (max - second_max). Large margin = confident win.
    /// 0.0 if only one signal.
    pub margin: f64,
}

/// Identify the dominant signal via SIMD argmax + compute margin-over-runner-up.
///
/// Returns `None` on empty input. Uses `touring_simd::argmax_f32` internally,
/// which is branchless-stable under ties (returns the first occurrence).
#[must_use]
pub fn dominant_signal(signals: &[HandlerSignal]) -> Option<DominantSignal> {
    if signals.is_empty() {
        return None;
    }

    let estimates_f32: Vec<f32> = signals.iter().map(|s| s.estimate as f32).collect();
    let idx = argmax_f32(&estimates_f32)?;
    let winner = signals.get(idx)?;

    // Margin over second-best (linear scan — signal count is always small).
    let mut second_max = f64::NEG_INFINITY;
    for (i, s) in signals.iter().enumerate() {
        if i != idx && s.estimate > second_max {
            second_max = s.estimate;
        }
    }
    let margin = if second_max.is_finite() {
        winner.estimate - second_max
    } else {
        0.0
    };

    Some(DominantSignal {
        index: idx,
        handler_name: winner.handler_name.clone(),
        estimate: winner.estimate,
        margin,
    })
}

/// Min/max span of the signal distribution via SIMD horizontal reductions.
///
/// Returns `(min, max)` of the estimates, or `None` on empty input. Uses
/// `touring_simd::reduce_min_f32` / `reduce_max_f32` — both branch-free and
/// vectorised. Useful for computing signal range as an uncertainty proxy
/// (narrow span = handler agreement; wide span = disagreement).
#[must_use]
pub fn signal_span(signals: &[HandlerSignal]) -> Option<(f64, f64)> {
    if signals.is_empty() {
        return None;
    }
    let estimates: Vec<f32> = signals.iter().map(|s| s.estimate as f32).collect();
    let min = reduce_min_f32(&estimates) as f64;
    let max = reduce_max_f32(&estimates) as f64;
    Some((min, max))
}

/// Sum of all signal estimates via SIMD horizontal reduction.
///
/// Useful for normalisation checks (e.g., verifying that a distribution sums
/// to 1.0 within tolerance).
#[must_use]
pub fn signal_total(signals: &[HandlerSignal]) -> f64 {
    if signals.is_empty() {
        return 0.0;
    }
    let estimates: Vec<f32> = signals.iter().map(|s| s.estimate as f32).collect();
    reduce_sum_f32(&estimates) as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_signal(name: &str, estimate: f64, confidence: f64) -> HandlerSignal {
        HandlerSignal {
            handler_name: name.to_string(),
            estimate,
            confidence,
        }
    }

    #[test]
    fn test_fuse_empty() {
        let result = fuse_signals(&[]);
        assert_eq!(result.signal_count, 0);
        assert_eq!(result.fused_estimate, 0.0);
        assert!(!result.is_high_confidence);
    }

    #[test]
    fn test_fuse_single_signal() {
        let signals = vec![make_signal("H91", 0.85, 0.9)];
        let result = fuse_signals(&signals);
        assert_eq!(result.signal_count, 1);
        assert!((result.fused_estimate - 0.85).abs() < 0.01);
        assert!(result.fused_confidence > 0.8);
    }

    #[test]
    fn test_fuse_agreeing_signals() {
        let signals = vec![
            make_signal("H91", 0.80, 0.9),
            make_signal("H92", 0.82, 0.8),
            make_signal("H93", 0.78, 0.85),
        ];
        let result = fuse_signals(&signals);
        assert_eq!(result.signal_count, 3);
        // Fused should be around 0.80
        assert!(result.fused_estimate > 0.75 && result.fused_estimate < 0.85);
        // Low CV — handlers agree
        assert!(result.disagreement_cv < 0.1);
        assert!(result.is_high_confidence);
    }

    #[test]
    fn test_fuse_disagreeing_signals() {
        let signals = vec![make_signal("H91", 0.9, 0.5), make_signal("H92", 0.2, 0.5)];
        let result = fuse_signals(&signals);
        // High CV — handlers disagree
        assert!(result.disagreement_cv > 0.3);
        assert!(!result.is_high_confidence);
    }

    #[test]
    fn test_fuse_weighted() {
        let signals = vec![make_signal("H91", 0.9, 0.5), make_signal("H92", 0.3, 0.5)];
        // Give H91 double weight
        let result = fuse_signals_weighted(&signals, &[2.0, 1.0]);
        // Should lean toward H91's estimate
        assert!(result.fused_estimate > 0.5);
    }

    #[test]
    fn test_reconcile_scores() {
        let scores = [0.8, 0.7, 0.9];
        let weights = [1.0, 1.0, 2.0];
        let (reconciled, cv) = reconcile_handler_scores(&scores, &weights);
        // Weighted mean should lean toward 0.9 (weight 2)
        assert!(reconciled > 0.78);
        assert!(cv < 0.15);
    }

    #[test]
    fn test_high_confidence_threshold() {
        // High confidence: conf > 0.7 AND cv < 0.3
        let signals = vec![make_signal("A", 0.85, 0.95), make_signal("B", 0.83, 0.90)];
        let result = fuse_signals(&signals);
        assert!(result.is_high_confidence);

        // Low confidence due to low conf scores
        let signals = vec![make_signal("A", 0.85, 0.2), make_signal("B", 0.83, 0.1)];
        let result = fuse_signals(&signals);
        assert!(!result.is_high_confidence);
    }

    // ─────────────────────────────────────────────────────────────────────
    // SIMD-powered softmax fusion + dominant signal tests
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn test_softmax_empty() {
        let r = fuse_signals_softmax(&[]);
        assert_eq!(r.signal_count, 0);
        assert_eq!(r.fused_estimate, 0.0);
        assert!(r.dominant_index.is_none());
    }

    #[test]
    fn test_softmax_single_signal() {
        let signals = vec![make_signal("A", 0.7, 0.9)];
        let r = fuse_signals_softmax(&signals);
        assert_eq!(r.signal_count, 1);
        assert_eq!(r.probabilities.len(), 1);
        // Single-signal softmax is 1.0 by definition.
        assert!((r.probabilities[0] - 1.0).abs() < 1e-6);
        assert!((r.fused_estimate - 0.7).abs() < 1e-6);
        assert_eq!(r.dominant_index, Some(0));
        assert!((r.max_probability - 1.0).abs() < 1e-6);
        // Entropy of a degenerate distribution (p=1) is 0.
        assert!(r.entropy.abs() < 1e-6);
    }

    #[test]
    fn test_softmax_uniform_logits() {
        // All-equal logits → uniform distribution (p=1/N each).
        let signals = vec![
            make_signal("A", 0.5, 0.8),
            make_signal("B", 0.5, 0.8),
            make_signal("C", 0.5, 0.8),
            make_signal("D", 0.5, 0.8),
        ];
        let r = fuse_signals_softmax(&signals);
        assert_eq!(r.probabilities.len(), 4);
        for p in &r.probabilities {
            assert!((p - 0.25).abs() < 1e-5, "expected uniform 0.25, got {p}");
        }
        // Fused = Σ 0.25 × 0.5 = 0.5.
        assert!((r.fused_estimate - 0.5).abs() < 1e-5);
        // Max entropy for N=4 is ln(4) ≈ 1.386.
        assert!((r.entropy - 4.0f64.ln()).abs() < 1e-4);
    }

    #[test]
    fn test_softmax_dominant_signal_amplified() {
        // One clearly-larger logit → softmax concentrates mass there.
        let signals = vec![
            make_signal("A", 0.1, 0.9),
            make_signal("B", 5.0, 0.9), // dominant
            make_signal("C", 0.2, 0.9),
        ];
        let r = fuse_signals_softmax(&signals);
        assert_eq!(r.dominant_index, Some(1));
        // Dominant probability must be > 0.95 given 5× gap.
        assert!(
            r.probabilities[1] > 0.95,
            "expected dominance, got {:?}",
            r.probabilities
        );
        // Fused estimate should be close to the dominant estimate.
        assert!((r.fused_estimate - 5.0).abs() < 0.5);
        // Low entropy = confident decision.
        assert!(r.entropy < 0.3);
    }

    #[test]
    fn test_softmax_probs_sum_to_one() {
        let signals = vec![
            make_signal("A", 1.0, 0.5),
            make_signal("B", 2.0, 0.5),
            make_signal("C", 3.0, 0.5),
        ];
        let r = fuse_signals_softmax(&signals);
        let sum: f64 = r.probabilities.iter().sum();
        assert!((sum - 1.0).abs() < 1e-5, "probs must sum to 1, got {sum}");
    }

    #[test]
    fn test_dominant_signal_with_margin() {
        let signals = vec![
            make_signal("runner_up", 0.5, 0.8),
            make_signal("winner", 0.95, 0.9),
            make_signal("lowest", 0.1, 0.7),
        ];
        let d = dominant_signal(&signals).expect("non-empty");
        assert_eq!(d.index, 1);
        assert_eq!(d.handler_name, "winner");
        assert!((d.estimate - 0.95).abs() < 1e-9);
        // Margin over runner-up (0.5): 0.45.
        assert!((d.margin - 0.45).abs() < 1e-9);
    }

    #[test]
    fn test_dominant_signal_single_is_zero_margin() {
        let signals = vec![make_signal("only", 0.7, 0.9)];
        let d = dominant_signal(&signals).expect("non-empty");
        assert_eq!(d.index, 0);
        assert_eq!(d.margin, 0.0);
    }

    #[test]
    fn test_dominant_signal_empty() {
        assert!(dominant_signal(&[]).is_none());
    }

    #[test]
    fn test_signal_span_and_total() {
        let signals = vec![
            make_signal("A", 0.1, 0.8),
            make_signal("B", 0.9, 0.8),
            make_signal("C", 0.5, 0.8),
        ];
        let (min, max) = signal_span(&signals).expect("non-empty");
        assert!((min - 0.1).abs() < 1e-5);
        assert!((max - 0.9).abs() < 1e-5);

        let total = signal_total(&signals);
        assert!((total - 1.5).abs() < 1e-5);

        assert!(signal_span(&[]).is_none());
        assert_eq!(signal_total(&[]), 0.0);
    }
}
