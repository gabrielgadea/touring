//! PyO3 bindings for touring-antt::financial_analysis and touring-simd::financial.

use pyo3::prelude::*;

/// Compute Net Present Value for cash flows at a discount rate.
///
/// Args:
///     cash_flows: List of cash flows (index 0 = period 0).
///     discount_rate: Discount rate as decimal (e.g. 0.10 for 10%).
///
/// Returns:
///     The NPV as a float.
#[pyfunction]
pub fn py_npv(cash_flows: Vec<f64>, discount_rate: f64) -> f64 {
    touring_simd::financial::npv(&cash_flows, discount_rate)
}

/// Compute NPV for multiple discount rates (batch).
///
/// Args:
///     cash_flows: List of cash flows.
///     discount_rates: List of discount rates.
///
/// Returns:
///     List of NPV values, one per rate.
#[pyfunction]
pub fn py_npv_batch(cash_flows: Vec<f64>, discount_rates: Vec<f64>) -> Vec<f64> {
    touring_simd::financial::npv_batch(&cash_flows, &discount_rates)
}

/// Compute Internal Rate of Return using Newton-Raphson.
///
/// Args:
///     cash_flows: List of cash flows.
///     tolerance: Convergence tolerance (default: 1e-8).
///     max_iterations: Maximum iterations (default: 200).
///
/// Returns:
///     The IRR as a float, or None if not convergent.
#[pyfunction]
#[pyo3(signature = (cash_flows, tolerance=1e-8, max_iterations=200))]
pub fn py_irr(cash_flows: Vec<f64>, tolerance: f64, max_iterations: u32) -> Option<f64> {
    touring_simd::financial::irr(&cash_flows, tolerance, max_iterations)
}

/// Run ANTT standard stress scenarios on cash flows.
///
/// Applies 5 demand variation scenarios (base, pessimistic, pessimistic_moderate,
/// optimistic_moderate, optimistic) and computes NPV + IRR for each.
///
/// Args:
///     cash_flows: Base case cash flows.
///     discount_rate: Discount rate for NPV.
///
/// Returns:
///     JSON string with full ConcessionAnalysis results.
#[pyfunction]
pub fn py_analyze_concession(cash_flows: Vec<f64>, discount_rate: f64) -> String {
    let analysis = touring_intelligence::ann::financial_analysis::analyze_concession(
        &cash_flows,
        discount_rate,
    );
    serde_json::json!({
        "npv": analysis.npv,
        "irr": analysis.irr,
        "discount_rate": analysis.discount_rate,
        "stress_results": analysis.stress_results,
        "npv_sensitivity": analysis.npv_sensitivity,
        "verdict": touring_intelligence::ann::financial_analysis::viability_verdict(&analysis),
    })
    .to_string()
}

/// Determine viability verdict from NPV and IRR.
///
/// Args:
///     npv: Net present value.
///     irr: Internal rate of return (or None).
///     discount_rate: The discount rate used.
///
/// Returns:
///     "VIAVEL" or "INVIAVEL".
#[pyfunction]
pub fn py_viability_verdict(npv: f64, irr: Option<f64>, discount_rate: f64) -> &'static str {
    let npv_ok = npv > 0.0;
    let irr_ok = irr.map(|r| r > discount_rate).unwrap_or(false);
    if npv_ok && irr_ok {
        "VIAVEL"
    } else {
        "INVIAVEL"
    }
}

/// Register financial bindings on the Python module.
pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(py_npv, m)?)?;
    m.add_function(wrap_pyfunction!(py_npv_batch, m)?)?;
    m.add_function(wrap_pyfunction!(py_irr, m)?)?;
    m.add_function(wrap_pyfunction!(py_analyze_concession, m)?)?;
    m.add_function(wrap_pyfunction!(py_viability_verdict, m)?)?;
    Ok(())
}
