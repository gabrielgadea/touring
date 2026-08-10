//! Composite scoring algorithm — weighted average over 50 dimensions.

use crate::DimId;
use std::collections::BTreeMap;
use std::sync::OnceLock;

/// Return the default per-dim weight (0.5..2.0).
///
/// Higher weight = more important in composite. Weights chosen by priority:
/// - P0 (BLOCK dims): 2.0
/// - P1 (WARN dims):   1.5
/// - P2 (rest):        1.0
pub fn default_weights() -> &'static BTreeMap<DimId, f32> {
    static WEIGHTS: OnceLock<BTreeMap<DimId, f32>> = OnceLock::new();
    WEIGHTS.get_or_init(|| {
        let mut m = BTreeMap::new();
        for dim in DimId::ALL {
            let w = match dim.enforcement() {
                crate::Enforcement::Block => 2.0,
                crate::Enforcement::Warn => 1.5,
                crate::Enforcement::Advisory => 1.0,
            };
            m.insert(*dim, w);
        }
        m
    })
}

/// Exponent of the weighted **power mean** used by the composite (W6
/// 2026-07-02). `p < 1` pulls the aggregate toward the WORST dimensions, so a
/// few genuinely-low dims in an otherwise-passing file are not washed out by
/// the ~44 dims that pass at ~1.0. This fixes the measured discrimination gap
/// (a file with antipatterns scored Diamond above a clean file under the old
/// arithmetic mean). `p = 1.0` would be the arithmetic mean; `p → 0` the
/// geometric mean. 0.5 is a moderate pull, aligned with SonarQube's
/// "worst-dimensions-dominate" gate philosophy. Equal-valued dims are
/// unchanged (power mean of all-`v` is `v` for any `p`).
const COMPOSITE_POWER: f32 = 0.5;

/// Compute the weighted **power-mean** composite from per-dim scores.
///
/// Returns 0.0 if all scores are missing or all weights are 0. `NotApplicable`
/// dims are excluded from both numerator and denominator (W3).
pub fn compute_composite(
    dimensions: &BTreeMap<DimId, crate::DimScore>,
    weights: &BTreeMap<DimId, f32>,
) -> f32 {
    let mut total_weight = 0.0_f32;
    let mut weighted_pow_sum = 0.0_f32;
    for (id, score) in dimensions {
        // W3 (2026-07-02): a NotApplicable dimension is excluded from BOTH the
        // numerator and the denominator — an inapplicable dim must neither
        // inflate nor deflate the composite (poliglota projects were being
        // inflated by Rust-only dims silently scoring Pass 1.0).
        if score.status == crate::DimStatus::NotApplicable {
            continue;
        }
        let w = weights.get(id).copied().unwrap_or(1.0);
        total_weight += w;
        // Accumulate wᵢ·vᵢ^p; the final mean is (Σ w·v^p / Σ w)^(1/p).
        weighted_pow_sum += score.value.clamp(0.0, 1.0).powf(COMPOSITE_POWER) * w;
    }
    if total_weight == 0.0 {
        return 0.0;
    }
    (weighted_pow_sum / total_weight)
        .powf(1.0 / COMPOSITE_POWER)
        .clamp(0.0, 1.0)
}

/// Apply the SonarQube-style **quality gate** on top of the numeric tier
/// (W5 2026-07-02). The weighted mean alone made the tier a badge: a single
/// BLOCK-dimension failure moved the composite only ~1.5%, so a hardcoded
/// secret or a live CVE could still land in Diamond. The gate makes a P0
/// failure DISQUALIFYING regardless of the mean:
///
/// * any **BLOCK** dimension (F2.1/F2.4/F2.5/F2.6/F4.3/F4.5) with status
///   `Fail` → tier is capped at [`crate::Tier::Unranked`] (rewrite);
/// * any **WARN** dimension with status `Fail` → tier capped at
///   [`crate::Tier::Silver`] (cannot be Gold+ with a failing quality dimension).
///
/// `NotApplicable` dimensions never trip the gate. `Tier`'s `Ord` runs
/// best→worst (Diamond is the smallest), so "cap" is `max` (the worse tier).
pub fn apply_quality_gate(
    base: crate::Tier,
    dimensions: &BTreeMap<DimId, crate::DimScore>,
) -> crate::Tier {
    use crate::{DimStatus, Enforcement, Tier};
    let mut tier = base;
    for (id, score) in dimensions {
        if score.status != DimStatus::Fail {
            continue;
        }
        match id.enforcement() {
            Enforcement::Block => return Tier::Unranked,
            Enforcement::Warn => tier = tier.max(Tier::Silver),
            Enforcement::Advisory => {}
        }
    }
    tier
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DimId, DimScore};

    fn score(value: f32) -> DimScore {
        DimScore::from_value(value, "test")
    }

    #[test]
    fn power_mean_pulls_below_arithmetic_but_keeps_perfect() {
        // W6 (2026-07-02): the power mean must pull a file with a few low dims
        // below the arithmetic mean (so localized badness is not washed out),
        // while an all-perfect file still scores 1.0.
        let w = default_weights();
        let mut mixed = BTreeMap::new();
        for (i, d) in DimId::ALL.iter().enumerate() {
            mixed.insert(*d, score(if i < 6 { 0.2 } else { 1.0 }));
        }
        let pm = compute_composite(&mixed, w);
        // Arithmetic reference over the same dims/weights.
        let (mut ws, mut tw) = (0.0_f32, 0.0_f32);
        for (id, s) in &mixed {
            let wt = w.get(id).copied().unwrap_or(1.0);
            ws += s.value * wt;
            tw += wt;
        }
        let arith = ws / tw;
        assert!(
            pm < arith - 0.01,
            "power-mean ({pm}) must sit below arithmetic ({arith}) — low dims not washed out"
        );
        let mut perfect = BTreeMap::new();
        for d in DimId::ALL {
            perfect.insert(*d, score(1.0));
        }
        assert!(
            (compute_composite(&perfect, w) - 1.0).abs() < 1e-4,
            "all-perfect must stay 1.0"
        );
    }

    #[test]
    fn quality_gate_block_fail_disqualifies_tier() {
        use crate::{DimStatus, Tier};
        // A near-perfect mean that would be Diamond, but with ONE failed BLOCK
        // dimension (F2.4 secrets) → gate caps at Unranked. This is the fix for
        // "Diamond illusory": a hardcoded secret cannot ride a high mean.
        let mut dims = BTreeMap::new();
        for d in DimId::ALL {
            dims.insert(*d, score(1.0));
        }
        dims.insert(
            DimId::F2_4,
            DimScore {
                value: 0.0,
                status: DimStatus::Fail,
                evidence: "secret".into(),
                suggestions: vec![],
                latency_ms: 0,
                truncated: false,
            },
        );
        let composite = compute_composite(&dims, default_weights());
        assert!(composite > 0.9, "mean is still high: {composite}");
        let tier = apply_quality_gate(Tier::Diamond, &dims);
        assert_eq!(
            tier,
            Tier::Unranked,
            "a failed BLOCK dim must disqualify the tier regardless of the mean"
        );
    }

    #[test]
    fn quality_gate_warn_fail_caps_at_silver() {
        use crate::{DimStatus, Tier};
        let mut dims = BTreeMap::new();
        // F1.1 complexity is a WARN-enforced dim; a hard fail caps at Silver.
        dims.insert(
            DimId::F1_1,
            DimScore {
                value: 0.2,
                status: DimStatus::Fail,
                evidence: "cc".into(),
                suggestions: vec![],
                latency_ms: 0,
                truncated: false,
            },
        );
        assert_eq!(apply_quality_gate(Tier::Diamond, &dims), Tier::Silver);
        // A non-failing WARN dim does not cap.
        let mut ok = BTreeMap::new();
        ok.insert(DimId::F1_1, score(1.0));
        assert_eq!(apply_quality_gate(Tier::Diamond, &ok), Tier::Diamond);
    }

    #[test]
    fn not_applicable_is_excluded_from_composite() {
        // W3 (2026-07-02): a NotApplicable dim must neither inflate nor deflate
        // the composite — it is dropped from numerator AND denominator.
        let w = default_weights();

        let mut with_na = BTreeMap::new();
        with_na.insert(DimId::F1_1, score(0.6));
        with_na.insert(DimId::F1_2, score(0.6));
        with_na.insert(DimId::F4_9, DimScore::not_applicable("no IaC"));
        let c_na = compute_composite(&with_na, w);

        // Same two applicable dims WITHOUT the N/A entry → identical composite.
        let mut without = BTreeMap::new();
        without.insert(DimId::F1_1, score(0.6));
        without.insert(DimId::F1_2, score(0.6));
        let c_plain = compute_composite(&without, w);

        assert!(
            (c_na - c_plain).abs() < 1e-6,
            "NotApplicable must not change the composite: {c_na} vs {c_plain}"
        );
        // And it must NOT be pulled toward the N/A's placeholder 1.0.
        assert!(
            c_na < 0.7,
            "two 0.6 dims must yield ~0.6, not be inflated by the N/A: {c_na}"
        );
    }

    #[test]
    fn test_composite_all_perfect() {
        let mut dims = BTreeMap::new();
        for d in DimId::ALL {
            dims.insert(*d, score(1.0));
        }
        let c = compute_composite(&dims, default_weights());
        assert!(
            (c - 1.0).abs() < 1e-6,
            "all 1.0 should yield 1.0, got {}",
            c
        );
    }

    #[test]
    fn test_composite_all_zero() {
        let mut dims = BTreeMap::new();
        for d in DimId::ALL {
            dims.insert(*d, score(0.0));
        }
        let c = compute_composite(&dims, default_weights());
        assert!((c - 0.0).abs() < 1e-6);
    }

    #[test]
    fn test_composite_empty() {
        let dims = BTreeMap::new();
        let c = compute_composite(&dims, default_weights());
        assert!((c - 0.0).abs() < 1e-6);
    }

    #[test]
    fn test_composite_weighted() {
        // Only F2.5 (BLOCK, weight 2.0) at 1.0; rest missing → should be 1.0
        let mut dims = BTreeMap::new();
        dims.insert(DimId::F2_5, score(1.0));
        let c = compute_composite(&dims, default_weights());
        assert!((c - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_composite_block_dims_dominate() {
        // F2.5 BLOCK (weight 2.0) = 0.0 ; F3.1 WARN (1.5) = 1.0 ; F4.1 (1.0) = 1.0.
        // W6 (2026-07-02): the composite is now a weighted POWER mean (p=0.5),
        // so the failed BLOCK dim dominates even more than the old arithmetic
        // mean (0.31 vs 0.56) — the intended "block dims dominate" behaviour,
        // strengthened. Expected = ((Σ w·v^0.5)/Σw)^(1/0.5).
        let mut dims = BTreeMap::new();
        dims.insert(DimId::F2_5, score(0.0));
        dims.insert(DimId::F3_1, score(1.0));
        dims.insert(DimId::F4_1, score(1.0));
        let c = compute_composite(&dims, default_weights());
        let p = 0.5_f32;
        let expected = ((0.0_f32.powf(p) * 2.0 + 1.0_f32.powf(p) * 1.5 + 1.0_f32.powf(p) * 1.0)
            / (2.0 + 1.5 + 1.0))
            .powf(1.0 / p);
        assert!((c - expected).abs() < 1e-4, "expected {expected} got {c}");
        // Sanity: the power mean pulls it below the arithmetic mean (0.556).
        assert!(
            c < 0.556,
            "power mean must dominate harder than arithmetic: {c}"
        );
    }

    #[test]
    fn test_default_weights_distribution() {
        let w = default_weights();
        let mut block_count = 0;
        let mut warn_count = 0;
        let mut adv_count = 0;
        for (id, weight) in w {
            let eff = id.enforcement();
            let expected = match eff {
                crate::Enforcement::Block => 2.0,
                crate::Enforcement::Warn => 1.5,
                crate::Enforcement::Advisory => 1.0,
            };
            assert!(
                (*weight - expected).abs() < 1e-6,
                "weight mismatch for {:?}",
                id
            );
            match eff {
                crate::Enforcement::Block => block_count += 1,
                crate::Enforcement::Warn => warn_count += 1,
                crate::Enforcement::Advisory => adv_count += 1,
            }
        }
        assert_eq!(block_count, 6);
        assert_eq!(warn_count, 13);
        assert_eq!(adv_count, 31);
    }
}
