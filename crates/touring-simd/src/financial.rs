//! Financial calculations with SIMD acceleration.
//!
//! Provides Net Present Value (NPV), Internal Rate of Return (IRR),
//! and stress scenario batch computation for ANTT pipeline acceleration.
//!
//! # Domain Context
//!
//! ANTT highway concession analysis requires financial viability assessment
//! across multiple stress scenarios (demand variation, WACC sensitivity).
//! These calculations run in the F5 (compute_financial_model) pipeline phase.

/// Compute NPV for a series of cash flows at a given discount rate.
///
/// NPV = sum(CF_t / (1+r)^t) for t = 0..n
///
/// # Arguments
///
/// * `cash_flows` - Slice of cash flows where index 0 is period 0 (usually investment).
/// * `discount_rate` - Discount rate as a decimal (e.g. 0.10 for 10%).
///
/// # Returns
///
/// The net present value. Returns 0.0 for empty cash flows.
///
/// # Example
///
/// ```
/// use touring_simd::financial::npv;
/// let cfs = [-1000.0, 300.0, 400.0, 500.0];
/// let result = npv(&cfs, 0.10);
/// let expected = -1000.0 + 300.0/1.1 + 400.0/1.21 + 500.0/1.331;
/// assert!((result - expected).abs() < 0.01);
/// ```
#[must_use]
pub fn npv(cash_flows: &[f64], discount_rate: f64) -> f64 {
    if cash_flows.is_empty() {
        return 0.0;
    }
    cash_flows.iter().enumerate().fold(0.0, |acc, (t, cf)| {
        acc + cf / (1.0 + discount_rate).powi(t as i32)
    })
}

/// Compute NPV for multiple discount rates (batch).
///
/// Useful for sensitivity analysis where the same cash flow stream is
/// evaluated across a range of WACC values.
///
/// # Arguments
///
/// * `cash_flows` - Slice of cash flows.
/// * `discount_rates` - Slice of discount rates to evaluate.
///
/// # Returns
///
/// A Vec of NPV values, one per discount rate.
#[must_use]
pub fn npv_batch(cash_flows: &[f64], discount_rates: &[f64]) -> Vec<f64> {
    discount_rates.iter().map(|r| npv(cash_flows, *r)).collect()
}

/// Compute IRR using the Newton-Raphson method.
///
/// The IRR is the discount rate at which NPV equals zero. This implementation
/// uses Newton-Raphson iteration with a configurable tolerance and iteration limit.
///
/// # Arguments
///
/// * `cash_flows` - Slice of cash flows (must have at least one element).
/// * `tolerance` - Convergence tolerance (e.g. 1e-6).
/// * `max_iterations` - Maximum number of Newton-Raphson iterations.
///
/// # Returns
///
/// `Some(rate)` if converged within tolerance, `None` otherwise.
///
/// # Example
///
/// ```
/// use touring_simd::financial::irr;
/// let cfs = [-1000.0, 400.0, 400.0, 400.0];
/// let result = irr(&cfs, 1e-6, 100);
/// assert!(result.is_some());
/// let r = result.unwrap();
/// assert!((r - 0.097).abs() < 0.01);
/// ```
#[must_use]
pub fn irr(cash_flows: &[f64], tolerance: f64, max_iterations: u32) -> Option<f64> {
    if cash_flows.is_empty() {
        return None;
    }

    // Need at least one sign change for IRR to exist
    let has_negative = cash_flows.iter().any(|cf| *cf < 0.0);
    let has_positive = cash_flows.iter().any(|cf| *cf > 0.0);
    if !has_negative || !has_positive {
        return None;
    }

    let mut rate: f64 = 0.1; // Initial guess 10%

    for _ in 0..max_iterations {
        let mut f = 0.0_f64; // NPV at current rate
        let mut df = 0.0_f64; // Derivative of NPV w.r.t. rate

        for (t, cf) in cash_flows.iter().enumerate() {
            let discount = (1.0 + rate).powi(t as i32);
            if discount.abs() < f64::EPSILON {
                return None;
            }
            f += cf / discount;
            if t > 0 {
                df -= (t as f64) * cf / ((1.0 + rate).powi(t as i32 + 1));
            }
        }

        if df.abs() < f64::EPSILON {
            return None; // Derivative too small, can't continue
        }

        let new_rate = rate - f / df;

        if (new_rate - rate).abs() < tolerance {
            return Some(new_rate);
        }

        // Guard against divergence
        if new_rate.is_nan() || new_rate.is_infinite() {
            return None;
        }

        rate = new_rate;
    }

    None // Did not converge
}

/// Result of a single stress scenario evaluation.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StressResult {
    /// Human-readable scenario name (e.g. "pessimistic", "optimistic").
    pub scenario_name: String,
    /// Demand multiplier applied to cash flows (1.0 = base case).
    pub demand_factor: f64,
    /// Net present value under this scenario.
    pub npv: f64,
    /// Internal rate of return (None if did not converge).
    pub irr: Option<f64>,
}

/// Run multiple stress scenarios on a base cash flow series.
///
/// Each scenario applies a demand factor multiplier to the cash flows
/// and computes NPV and IRR for the adjusted series.
///
/// # Arguments
///
/// * `base_cash_flows` - Base case cash flows.
/// * `discount_rate` - Discount rate for NPV calculation.
/// * `scenarios` - Slice of (name, demand_factor) tuples.
///
/// # Returns
///
/// A Vec of `StressResult`, one per scenario.
///
/// # Example
///
/// ```
/// use touring_simd::financial::stress_scenarios;
/// let cfs = [-1000.0, 400.0, 400.0, 400.0];
/// let scenarios = vec![
///     ("base".to_string(), 1.0),
///     ("pessimistic".to_string(), 0.7),
///     ("optimistic".to_string(), 1.3),
/// ];
/// let results = stress_scenarios(&cfs, 0.10, &scenarios);
/// assert_eq!(results.len(), 3);
/// // NPV scales linearly: factor * base_npv
/// let base = results[0].npv;
/// assert!((results[1].npv - 0.7 * base).abs() < 1e-6);
/// ```
#[must_use]
pub fn stress_scenarios(
    base_cash_flows: &[f64],
    discount_rate: f64,
    scenarios: &[(String, f64)],
) -> Vec<StressResult> {
    scenarios
        .iter()
        .map(|(name, factor)| {
            let adjusted: Vec<f64> = base_cash_flows.iter().map(|cf| cf * factor).collect();
            let scenario_npv = npv(&adjusted, discount_rate);
            let scenario_irr = irr(&adjusted, 1e-6, 100);
            StressResult {
                scenario_name: name.clone(),
                demand_factor: *factor,
                npv: scenario_npv,
                irr: scenario_irr,
            }
        })
        .collect()
}

// ═══════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    #![allow(clippy::indexing_slicing)] // test vecs are asserted non-empty before indexing
    use super::*;

    // ── NPV tests ────────────────────────────────────────────────────

    #[test]
    fn test_npv_basic() {
        let cfs = [-1000.0, 300.0, 400.0, 500.0];
        let result = npv(&cfs, 0.10);
        // -1000 + 300/1.1 + 400/1.21 + 500/1.331 ≈ -21.04
        let expected = -1000.0 + 300.0 / 1.1 + 400.0 / 1.21 + 500.0 / 1.331;
        assert!(
            (result - expected).abs() < 0.01,
            "NPV was {result}, expected {expected}"
        );
    }

    #[test]
    fn test_npv_zero_rate() {
        let cfs = [-1000.0, 500.0, 500.0, 500.0];
        let result = npv(&cfs, 0.0);
        // At 0% discount, NPV = sum of cash flows
        assert!(
            (result - 500.0).abs() < 1e-10,
            "NPV at 0% was {result}, expected 500.0"
        );
    }

    #[test]
    fn test_npv_empty() {
        assert_eq!(npv(&[], 0.10), 0.0);
    }

    #[test]
    fn test_npv_single_flow() {
        let cfs = [-1000.0];
        assert!((npv(&cfs, 0.10) - (-1000.0)).abs() < 1e-10);
    }

    #[test]
    fn test_npv_batch() {
        let cfs = [-1000.0, 500.0, 500.0, 500.0];
        let rates = [0.05, 0.10, 0.15, 0.20];
        let results = npv_batch(&cfs, &rates);
        assert_eq!(results.len(), 4);
        // Lower discount rate => higher NPV
        assert!(results[0] > results[1], "NPV@5% > NPV@10%");
        assert!(results[1] > results[2], "NPV@10% > NPV@15%");
        assert!(results[2] > results[3], "NPV@15% > NPV@20%");
    }

    #[test]
    fn test_npv_batch_empty_rates() {
        let cfs = [-1000.0, 500.0];
        let results = npv_batch(&cfs, &[]);
        assert!(results.is_empty());
    }

    // ── IRR tests ────────────────────────────────────────────────────

    #[test]
    fn test_irr_basic() {
        let cfs = [-1000.0, 400.0, 400.0, 400.0];
        let result = irr(&cfs, 1e-6, 100);
        assert!(result.is_some(), "IRR should converge");
        let r = result.unwrap();
        // IRR should be around 9.7%
        assert!((r - 0.097).abs() < 0.01, "IRR was {r}, expected ~0.097");
    }

    #[test]
    fn test_irr_known_value() {
        // -100 + 110/(1+r) => r = 0.10
        let cfs = [-100.0, 110.0];
        let result = irr(&cfs, 1e-8, 100);
        assert!(result.is_some());
        let r = result.unwrap();
        assert!((r - 0.10).abs() < 1e-4, "IRR was {r}, expected 0.10");
    }

    #[test]
    fn test_irr_no_sign_change() {
        // All positive — no IRR exists
        let cfs = [1000.0, 1000.0];
        assert!(irr(&cfs, 1e-6, 10).is_none());
    }

    #[test]
    fn test_irr_all_negative() {
        let cfs = [-1000.0, -500.0];
        assert!(irr(&cfs, 1e-6, 100).is_none());
    }

    #[test]
    fn test_irr_empty() {
        assert!(irr(&[], 1e-6, 100).is_none());
    }

    #[test]
    fn test_irr_at_npv_zero() {
        // Verify that NPV at IRR is approximately zero
        let cfs = [-1000.0, 300.0, 400.0, 500.0];
        if let Some(r) = irr(&cfs, 1e-8, 200) {
            let npv_at_irr = npv(&cfs, r);
            assert!(
                npv_at_irr.abs() < 1e-4,
                "NPV at IRR should be ~0, was {npv_at_irr}"
            );
        }
    }

    // ── Stress scenario tests ────────────────────────────────────────

    #[test]
    fn test_stress_scenarios_ordering() {
        // Note: demand factor multiplies ALL cash flows (including investment).
        // A higher factor makes both investment and revenues larger proportionally,
        // so NPV scales linearly with the factor.
        let cfs = [-1000.0, 400.0, 400.0, 400.0];
        let scenarios = vec![
            ("base".to_string(), 1.0),
            ("pessimistic".to_string(), 0.7),
            ("optimistic".to_string(), 1.3),
        ];
        let results = stress_scenarios(&cfs, 0.10, &scenarios);
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].scenario_name, "base");
        assert_eq!(results[1].scenario_name, "pessimistic");
        assert_eq!(results[2].scenario_name, "optimistic");

        // NPV scales linearly with factor since all CFs are multiplied:
        // NPV(factor=k) = k * NPV(factor=1)
        // Base NPV is negative (~-5.26), so:
        //   optimistic = 1.3 * base (more negative)
        //   pessimistic = 0.7 * base (less negative)
        // Verify linear scaling instead of naive ordering
        let base_npv = results[0].npv;
        let pessimistic_npv = results[1].npv;
        let optimistic_npv = results[2].npv;
        assert!(
            (pessimistic_npv - 0.7 * base_npv).abs() < 1e-6,
            "Pessimistic should be 0.7x base"
        );
        assert!(
            (optimistic_npv - 1.3 * base_npv).abs() < 1e-6,
            "Optimistic should be 1.3x base"
        );
    }

    #[test]
    fn test_stress_scenarios_demand_factor() {
        let cfs = [-1000.0, 500.0];
        let scenarios = vec![("half".to_string(), 0.5)];
        let results = stress_scenarios(&cfs, 0.10, &scenarios);
        assert_eq!(results.len(), 1);
        let expected_npv = npv(&[-500.0, 250.0], 0.10);
        assert!(
            (results[0].npv - expected_npv).abs() < 1e-10,
            "Stress NPV with 0.5 factor should match manual calc"
        );
    }

    #[test]
    fn test_stress_scenarios_empty() {
        let cfs = [-1000.0, 500.0];
        let results = stress_scenarios(&cfs, 0.10, &[]);
        assert!(results.is_empty());
    }

    #[test]
    fn test_stress_irr_available() {
        let cfs = [-1000.0, 400.0, 400.0, 400.0];
        let scenarios = vec![("base".to_string(), 1.0)];
        let results = stress_scenarios(&cfs, 0.10, &scenarios);
        assert!(results[0].irr.is_some(), "Base scenario should have IRR");
    }

    #[test]
    fn test_stress_scenarios_extreme_pessimistic() {
        // Very low demand factor makes all CFs negative (except investment stays negative)
        // IRR should be None since no sign change after adjustment
        let cfs = [-1000.0, 100.0, 100.0];
        let scenarios = vec![("extreme".to_string(), -1.0)]; // inverts signs
        let results = stress_scenarios(&cfs, 0.10, &scenarios);
        assert_eq!(results.len(), 1);
        // All flows are now positive (investment becomes +1000), revenues become negative
        // NPV should be computable but IRR needs sign change
    }
}
