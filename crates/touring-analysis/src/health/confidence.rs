//! Wilson score confidence interval computation.
//!
//! Provides statistical confidence bounds for composite scores.
//! Uses the Wilson score interval (better than normal approximation
//! for small sample sizes and extreme proportions).

/// Compute Wilson score 95% confidence interval.
///
/// # Arguments
/// * `score` - Observed proportion (0.0–1.0)
/// * `n` - Number of observations (dimensions)
///
/// # Returns
/// `(lower, upper)` bounds of the 95% CI, clamped to [0.0, 1.0].
pub fn wilson_interval(score: f64, n: u32) -> (f64, f64) {
    if n == 0 {
        return (0.0, 0.0);
    }

    let n_f = n as f64;
    // z = 1.96 for 95% CI
    let z = 1.96_f64;
    let z2 = z * z;

    let denominator = 1.0 + z2 / n_f;
    let center = (score + z2 / (2.0 * n_f)) / denominator;
    let margin = z * (score * (1.0 - score) / n_f + z2 / (4.0 * n_f * n_f)).sqrt() / denominator;

    let lower = (center - margin).clamp(0.0, 1.0);
    let upper = (center + margin).clamp(0.0, 1.0);

    (lower, upper)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_perfect_score_small_n() {
        let (lower, upper) = wilson_interval(1.0, 5);
        assert!(lower > 0.5, "lower should be reasonable: {lower}");
        assert!(
            (upper - 1.0).abs() < 0.01,
            "upper should be near 1.0: {upper}"
        );
    }

    #[test]
    fn test_zero_score() {
        let (lower, upper) = wilson_interval(0.0, 5);
        assert!((lower - 0.0).abs() < 0.01);
        assert!(upper < 0.5, "upper should be reasonable: {upper}");
    }

    #[test]
    fn test_half_score_large_n() {
        let (lower, upper) = wilson_interval(0.5, 100);
        assert!(lower > 0.4, "lower: {lower}");
        assert!(upper < 0.6, "upper: {upper}");
    }

    #[test]
    fn test_zero_observations() {
        let (lower, upper) = wilson_interval(0.5, 0);
        assert_eq!(lower, 0.0);
        assert_eq!(upper, 0.0);
    }

    #[test]
    fn test_single_observation() {
        let (lower, upper) = wilson_interval(0.8, 1);
        assert!(lower >= 0.0);
        assert!(upper <= 1.0);
        // With n=1, CI should be very wide
        assert!(upper - lower > 0.3);
    }

    #[test]
    fn test_bounds_always_valid() {
        for score_pct in 0..=100 {
            for n in 1..=20 {
                let score = score_pct as f64 / 100.0;
                let (lower, upper) = wilson_interval(score, n);
                assert!(lower >= 0.0, "lower < 0: score={score}, n={n}");
                assert!(upper <= 1.0, "upper > 1: score={score}, n={n}");
                assert!(lower <= upper, "lower > upper: score={score}, n={n}");
            }
        }
    }
}
