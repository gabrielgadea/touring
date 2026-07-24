//! **S-08 — Conformal calibration for grounded skill selection (A-A1).**
//!
//! Realizes the "conformal-threshold routing" that [`crate::approval_store`]'s
//! module docstring anticipates. The `cli_suggester` classifier assigns a *raw*
//! confidence to each `(tool, intent)` routing decision (heuristic priors in the
//! 0.5–0.99 band). Historically the firing gate used a single hardcoded cut of
//! `0.7` — a magic constant with no statistical meaning.
//!
//! This module replaces that fixed cut with a **split conformal prediction**
//! threshold — the machinery behind KnowNo (Ren et al. 2023) and SayCan-style
//! grounded action selection. Given a calibration set of
//! `(raw_confidence, was_valid)` observations drawn from the real outcome
//! substrate (`bash_outcomes`), it computes a data-derived threshold
//! `τ = 1 − q̂` carrying a finite-sample **coverage guarantee**:
//!
//! > `P(c ≥ τ | the routing was apt) ≥ 1 − α`
//!
//! i.e. an apt routing is mistakenly deferred to a human at most an `α` fraction
//! of the time. When a live confidence falls below `τ` the action is *not* in
//! the conformal prediction set → the calibrator advises deferring to HITL (the
//! [`crate::approval_store`] pending-approval path, keyed by the same
//! `ActionSignature`, so the two compose: a human who already approved an action
//! class overrides the defer).
//!
//! ## Method (split / inductive conformal prediction)
//!
//! For the calibration set of *valid* (apt) examples, the nonconformity score of
//! example `i` is `s_i = 1 − c_i` (one minus the confidence the classifier
//! placed on the action it fired). The conformal quantile `q̂` is the
//! `⌈(n+1)(1−α)⌉`-th smallest score; if that rank exceeds `n` the quantile
//! saturates at `1.0` (admit everything — too little data to be selective). The
//! calibrated threshold is `τ = 1 − q̂`. A test confidence `c` is in the
//! prediction set iff `s(c) = 1 − c ≤ q̂`, equivalently `c ≥ τ`. The `+1` in the
//! rank is what buys the finite-sample (not merely asymptotic) coverage bound.
//!
//! The calibrator is pure (no I/O; the only allocation is the score buffer) so
//! it is trivially testable and cheap enough to run on the PreToolUse hot path
//! behind a TTL cache.

/// Target miscoverage rate α (KnowNo uses ε ≈ 0.1 → 90 % coverage).
pub const DEFAULT_ALPHA: f64 = 0.1;

/// Minimum calibration examples before the conformal threshold is trusted.
/// Below this the calibrator reports `calibrated = false` and callers fall back
/// to the legacy fixed cut.
pub const MIN_CALIBRATION: usize = 10;

/// Legacy hardcoded gate the conformal threshold supersedes; the fallback used
/// when calibration data is insufficient (`n < MIN_CALIBRATION`).
pub const LEGACY_THRESHOLD: f64 = 0.7;

/// Floor on the calibrated threshold. Guards against a degenerate substrate
/// (e.g. everything succeeded → `q̂ ≈ 1` → `τ ≈ 0`) admitting every
/// low-confidence routing. The conformal cut is never permitted below this.
pub const THRESHOLD_FLOOR: f64 = 0.5;

/// A split-conformal calibrator over routing-decision confidences.
///
/// Built incrementally with [`observe`](Self::observe) or in bulk with
/// [`from_examples`](Self::from_examples). The calibration target is *apt*
/// routings — only `was_valid = true` examples contribute a nonconformity
/// score, because the coverage guarantee is "apt actions pass the gate", not
/// "every action passes".
#[derive(Debug, Clone)]
pub struct ConformalCalibrator {
    alpha: f64,
    /// Nonconformity scores `1 − c_i` of the *valid* calibration examples.
    scores: Vec<f64>,
}

/// The calibrated decision for a single live confidence.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CalibratedDecision {
    /// The (clamped) raw confidence that was calibrated.
    pub raw_confidence: f64,
    /// The conformal firing threshold `τ` in force.
    pub calibrated_threshold: f64,
    /// The coverage target `1 − α` the threshold guarantees.
    pub coverage_target: f64,
    /// `true` iff the confidence is in the conformal prediction set (`c ≥ τ`).
    pub in_prediction_set: bool,
    /// `true` iff the calibrator advises deferring to a human (`c < τ`).
    pub defer_hitl: bool,
    /// How many calibration examples backed the threshold.
    pub n_calibration: usize,
    /// `true` iff `n ≥ MIN_CALIBRATION` (else `calibrated_threshold` is the
    /// legacy fallback, not a conformal quantile).
    pub calibrated: bool,
}

impl ConformalCalibrator {
    /// Create an empty calibrator at miscoverage rate `alpha` (clamped to the
    /// open interval `(0, 1)`).
    pub fn new(alpha: f64) -> Self {
        Self {
            alpha: alpha.clamp(f64::EPSILON, 1.0 - f64::EPSILON),
            scores: Vec::new(),
        }
    }

    /// Create an empty calibrator at [`DEFAULT_ALPHA`].
    pub fn with_default_alpha() -> Self {
        Self::new(DEFAULT_ALPHA)
    }

    /// Ingest one calibration observation. Only *valid* (apt) examples
    /// contribute a nonconformity score; invalid routings are outside the
    /// coverage target. Confidence is clamped to `[0, 1]`.
    pub fn observe(&mut self, raw_confidence: f64, was_valid: bool) {
        if was_valid {
            let s = (1.0 - raw_confidence).clamp(0.0, 1.0);
            self.scores.push(s);
        }
    }

    /// Bulk constructor from an iterator of `(raw_confidence, was_valid)`.
    pub fn from_examples<I: IntoIterator<Item = (f64, bool)>>(alpha: f64, examples: I) -> Self {
        let mut c = Self::new(alpha);
        for (conf, valid) in examples {
            c.observe(conf, valid);
        }
        c
    }

    /// The configured miscoverage rate `α`.
    pub fn alpha(&self) -> f64 {
        self.alpha
    }

    /// The number of calibration examples (valid observations) accumulated.
    pub fn n(&self) -> usize {
        self.scores.len()
    }

    /// `true` iff enough calibration data exists to trust the conformal cut.
    pub fn is_calibrated(&self) -> bool {
        self.scores.len() >= MIN_CALIBRATION
    }

    /// The conformal quantile `q̂` — the `⌈(n+1)(1−α)⌉`-th smallest
    /// nonconformity score. Saturates at `1.0` when the rank exceeds `n`
    /// (admit-all). Returns `None` only when there is no calibration data.
    pub fn quantile(&self) -> Option<f64> {
        let n = self.scores.len();
        if n == 0 {
            return None;
        }
        let mut sorted = self.scores.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        // 1-indexed rank into the sorted scores.
        let rank = (((n + 1) as f64) * (1.0 - self.alpha)).ceil() as usize;
        if rank == 0 {
            return Some(0.0);
        }
        if rank > n {
            // Not enough data to exclude anything at this α.
            return Some(1.0);
        }
        Some(sorted[rank - 1])
    }

    /// The calibrated firing threshold `τ = 1 − q̂`, floored at
    /// [`THRESHOLD_FLOOR`]. Falls back to [`LEGACY_THRESHOLD`] when
    /// `n < MIN_CALIBRATION`.
    pub fn threshold(&self) -> f64 {
        if !self.is_calibrated() {
            return LEGACY_THRESHOLD;
        }
        match self.quantile() {
            Some(q) => (1.0 - q).clamp(THRESHOLD_FLOOR, 1.0),
            None => LEGACY_THRESHOLD,
        }
    }

    /// Calibrate a single live confidence into a HITL-gating decision.
    pub fn calibrate(&self, raw_confidence: f64) -> CalibratedDecision {
        let tau = self.threshold();
        let c = raw_confidence.clamp(0.0, 1.0);
        let in_set = c >= tau;
        CalibratedDecision {
            raw_confidence: c,
            calibrated_threshold: tau,
            coverage_target: 1.0 - self.alpha,
            in_prediction_set: in_set,
            defer_hitl: !in_set,
            n_calibration: self.scores.len(),
            calibrated: self.is_calibrated(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Deterministic LCG → reproducible pseudo-confidences (no `rand` dep, so
    /// the coverage test is byte-stable across runs).
    struct Lcg(u64);
    impl Lcg {
        fn next_unit(&mut self) -> f64 {
            self.0 = self
                .0
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            ((self.0 >> 33) as f64) / ((1u64 << 31) as f64)
        }
    }

    #[test]
    fn no_data_quantile_is_none_threshold_is_legacy() {
        let c = ConformalCalibrator::with_default_alpha();
        assert_eq!(c.quantile(), None);
        assert!(!c.is_calibrated());
        assert!((c.threshold() - LEGACY_THRESHOLD).abs() < 1e-9);
    }

    #[test]
    fn invalid_examples_do_not_calibrate() {
        let mut c = ConformalCalibrator::new(0.1);
        for _ in 0..50 {
            c.observe(0.9, false); // failures contribute no score
        }
        assert_eq!(c.n(), 0);
        assert!(!c.is_calibrated());
    }

    #[test]
    fn insufficient_data_falls_back_to_legacy() {
        let mut c = ConformalCalibrator::new(0.1);
        for _ in 0..(MIN_CALIBRATION - 1) {
            c.observe(0.8, true);
        }
        let d = c.calibrate(0.65);
        assert!(!d.calibrated);
        assert!((d.calibrated_threshold - LEGACY_THRESHOLD).abs() < 1e-9);
        assert!(d.defer_hitl, "0.65 < 0.7 legacy → defer");
    }

    #[test]
    fn all_high_confidence_floors_threshold() {
        // Apt actions all fired at ~0.99 → scores ≈ 0.01 → q̂ ≈ 0.01 → τ ≈ 0.99.
        let mut c = ConformalCalibrator::new(0.1);
        for _ in 0..200 {
            c.observe(0.99, true);
        }
        assert!(c.is_calibrated());
        let tau = c.threshold();
        assert!(tau >= THRESHOLD_FLOOR && tau <= 1.0);
        assert!(
            tau > 0.9,
            "strict threshold when apt actions are high-confidence: {tau}"
        );
    }

    #[test]
    fn degenerate_low_scores_respect_floor() {
        // Apt actions all fired at LOW confidence → scores ≈ 1.0 → q̂ ≈ 1 → τ ≈ 0
        // → must be floored, never admit-everything.
        let mut c = ConformalCalibrator::new(0.1);
        for _ in 0..200 {
            c.observe(0.05, true);
        }
        let tau = c.threshold();
        assert!(
            (tau - THRESHOLD_FLOOR).abs() < 1e-9,
            "floored at {THRESHOLD_FLOOR}, got {tau}"
        );
    }

    #[test]
    fn defer_hitl_matches_prediction_set_membership() {
        let mut c = ConformalCalibrator::new(0.1);
        for _ in 0..100 {
            c.observe(0.8, true);
        }
        let tau = c.threshold();
        let below = c.calibrate(tau - 0.01);
        let above = c.calibrate(tau + 0.01);
        assert!(below.defer_hitl && !below.in_prediction_set);
        assert!(!above.defer_hitl && above.in_prediction_set);
        assert!((below.coverage_target - 0.9).abs() < 1e-9);
    }

    #[test]
    fn quantile_rank_formula_is_split_conformal() {
        // n = 9, α = 0.1 → rank = ceil(10 * 0.9) = 9 → the 9th (largest) of 9.
        let mut c = ConformalCalibrator::new(0.1);
        // scores 0.0..0.8 in steps of 0.1 (9 valid examples → confidences 1.0..0.2)
        for i in 0..9 {
            let conf = 1.0 - (i as f64) * 0.1;
            c.observe(conf, true);
        }
        // n=9 < MIN_CALIBRATION(10) → threshold falls back, but the quantile math
        // itself must still be the largest score (0.8).
        let q = c.quantile().unwrap();
        assert!((q - 0.8).abs() < 1e-9, "expected 0.8, got {q}");
    }

    #[test]
    fn coverage_guarantee_holds_empirically() {
        // Calibration and test draw from the SAME distribution (exchangeable),
        // so empirical test coverage must meet the 1−α target (split conformal
        // tends to slightly over-cover thanks to the +1 in the rank).
        let alpha = 0.1;
        let mut rng = Lcg(0x2545_F491_4F6C_DD1D);
        // Apt-action confidences concentrated in [0.55, 1.0].
        let sample = |r: &mut Lcg| 0.55 + 0.45 * r.next_unit();

        let mut cal = ConformalCalibrator::new(alpha);
        for _ in 0..1000 {
            let c = sample(&mut rng);
            cal.observe(c, true);
        }
        let tau = cal.threshold();

        let total = 5000;
        let mut covered = 0;
        for _ in 0..total {
            let c = sample(&mut rng);
            if c >= tau {
                covered += 1;
            }
        }
        let coverage = covered as f64 / total as f64;
        assert!(
            coverage >= (1.0 - alpha) - 0.04,
            "empirical coverage {coverage} below target {}",
            1.0 - alpha
        );
    }

    #[test]
    fn from_examples_matches_incremental() {
        let examples = [(0.9, true), (0.3, false), (0.7, true), (0.8, true)];
        let bulk = ConformalCalibrator::from_examples(0.1, examples);
        let mut inc = ConformalCalibrator::new(0.1);
        for (conf, valid) in examples {
            inc.observe(conf, valid);
        }
        assert_eq!(bulk.n(), inc.n());
        assert_eq!(bulk.n(), 3); // the false example contributes nothing
    }
}
