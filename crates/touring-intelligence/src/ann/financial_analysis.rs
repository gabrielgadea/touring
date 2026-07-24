//! Financial analysis for ANTT regulatory concession documents.
//!
//! Bridges `touring_simd::financial` (NPV, IRR, stress scenarios) with
//! ANTT-specific monetary parsing from `monetary_parser`. This enables
//! end-to-end financial analysis: extract values from regulatory documents
//! → compute financial viability → run stress scenarios.
//!
//! # Domain Context
//!
//! ANTT concession analysis requires:
//! - Extracting monetary values from technical notes (Fator D, tarifas)
//! - Computing NPV/IRR for concession cash flow projections
//! - Running stress scenarios (demand variation, WACC sensitivity)
//! - Comparing computed values against regulatory thresholds

use touring_simd::financial::{StressResult, irr, npv, npv_batch, stress_scenarios};

use crate::ann::monetary_parser::{self, MonetaryValue};

/// Result of a concession financial analysis.
#[derive(Debug, Clone)]
pub struct ConcessionAnalysis {
    /// Extracted monetary values from the document.
    pub extracted_values: Vec<MonetaryValue>,
    /// Constructed cash flow series.
    pub cash_flows: Vec<f64>,
    /// Discount rate used (WACC or regulatory rate).
    pub discount_rate: f64,
    /// Computed NPV.
    pub npv: f64,
    /// Computed IRR (None if not convergent).
    pub irr: Option<f64>,
    /// Stress scenario results.
    pub stress_results: Vec<StressResult>,
    /// NPV sensitivity to discount rate variation.
    pub npv_sensitivity: Vec<(f64, f64)>,
}

/// Standard ANTT stress scenarios for concession analysis.
pub fn antt_standard_scenarios() -> Vec<(String, f64)> {
    vec![
        ("base".to_string(), 1.0),
        ("pessimista_demanda".to_string(), 0.70),
        ("pessimista_moderado".to_string(), 0.85),
        ("otimista_moderado".to_string(), 1.15),
        ("otimista_demanda".to_string(), 1.30),
    ]
}

/// Standard WACC sensitivity range for ANTT analysis.
pub fn antt_wacc_range() -> Vec<f64> {
    vec![0.06, 0.07, 0.08, 0.09, 0.10, 0.11, 0.12, 0.13, 0.14]
}

/// Analyze a concession's financial viability from extracted monetary values.
///
/// # Arguments
///
/// * `cash_flows` - Cash flow series (index 0 = initial investment, usually negative).
/// * `discount_rate` - Base discount rate (WACC).
///
/// # Returns
///
/// Complete concession analysis with NPV, IRR, stress scenarios, and sensitivity.
pub fn analyze_concession(cash_flows: &[f64], discount_rate: f64) -> ConcessionAnalysis {
    let computed_npv = npv(cash_flows, discount_rate);
    let computed_irr = irr(cash_flows, 1e-8, 200);

    let scenarios = antt_standard_scenarios();
    let stress = stress_scenarios(cash_flows, discount_rate, &scenarios);

    let wacc_range = antt_wacc_range();
    let npv_sens: Vec<(f64, f64)> = wacc_range
        .iter()
        .zip(npv_batch(cash_flows, &wacc_range))
        .map(|(r, v)| (*r, v))
        .collect();

    ConcessionAnalysis {
        extracted_values: vec![],
        cash_flows: cash_flows.to_vec(),
        discount_rate,
        npv: computed_npv,
        irr: computed_irr,
        stress_results: stress,
        npv_sensitivity: npv_sens,
    }
}

/// Extract monetary values from text and construct a cash flow series.
///
/// Parses Brazilian monetary values from document text, sorts by position,
/// and interprets the first value as investment (negated) and the rest as returns.
pub fn extract_and_analyze(text: &str, discount_rate: f64) -> ConcessionAnalysis {
    let values = monetary_parser::parse_monetary(text).unwrap_or_default();

    let cash_flows: Vec<f64> = if values.is_empty() {
        vec![]
    } else {
        let mut cfs: Vec<f64> = values.iter().map(|v| v.value).collect();
        // Negate first value as investment (convention: CF0 < 0)
        if let Some(first) = cfs.first_mut() {
            if *first > 0.0 {
                *first = -*first;
            }
        }
        cfs
    };

    let mut analysis = analyze_concession(&cash_flows, discount_rate);
    analysis.extracted_values = values;
    analysis
}

/// Determine financial viability verdict.
///
/// Returns "VIAVEL" if NPV > 0 and IRR > discount_rate, "INVIAVEL" otherwise.
pub fn viability_verdict(analysis: &ConcessionAnalysis) -> &'static str {
    let npv_ok = analysis.npv > 0.0;
    let irr_ok = analysis
        .irr
        .map(|r| r > analysis.discount_rate)
        .unwrap_or(false);
    if npv_ok && irr_ok {
        "VIAVEL"
    } else {
        "INVIAVEL"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_analyze_concession_basic() {
        let cfs = vec![-1000.0, 400.0, 400.0, 400.0];
        let analysis = analyze_concession(&cfs, 0.10);

        // NPV should be slightly negative at 10%
        assert!(analysis.npv < 10.0);
        // IRR should be around 9.7%
        assert!(analysis.irr.is_some());
        let r = analysis.irr.unwrap();
        assert!((r - 0.097).abs() < 0.01, "IRR was {r}");
        // Should have 5 standard scenarios
        assert_eq!(analysis.stress_results.len(), 5);
        // Should have 9 WACC sensitivity points
        assert_eq!(analysis.npv_sensitivity.len(), 9);
    }

    #[test]
    fn test_viability_verdict_viable() {
        let cfs = vec![-1000.0, 500.0, 500.0, 500.0];
        let analysis = analyze_concession(&cfs, 0.10);
        assert!(analysis.npv > 0.0);
        assert_eq!(viability_verdict(&analysis), "VIAVEL");
    }

    #[test]
    fn test_viability_verdict_not_viable() {
        let cfs = vec![-1000.0, 100.0, 100.0, 100.0];
        let analysis = analyze_concession(&cfs, 0.10);
        assert!(analysis.npv < 0.0);
        assert_eq!(viability_verdict(&analysis), "INVIAVEL");
    }

    #[test]
    fn test_antt_standard_scenarios() {
        let scenarios = antt_standard_scenarios();
        assert_eq!(scenarios.len(), 5);
        assert_eq!(scenarios[0].1, 1.0); // base
        assert!(scenarios[1].1 < 1.0); // pessimista
        assert!(scenarios[4].1 > 1.0); // otimista
    }

    #[test]
    fn test_stress_results_scale_linearly() {
        let cfs = vec![-1000.0, 500.0, 500.0, 500.0];
        let analysis = analyze_concession(&cfs, 0.10);
        let base_npv = analysis.stress_results[0].npv;
        let pessimistic_npv = analysis.stress_results[1].npv;
        // Pessimistic (0.7x) NPV should be less than base
        assert!(pessimistic_npv < base_npv);
    }

    #[test]
    fn test_extract_and_analyze_with_text() {
        let text = "O investimento total é de R$ 1.000.000,00 com retorno anual de R$ 400.000,00 por 3 anos.";
        let analysis = extract_and_analyze(text, 0.10);
        // Should have extracted at least the monetary values
        assert!(!analysis.extracted_values.is_empty());
    }

    #[test]
    fn test_empty_cash_flows() {
        let analysis = analyze_concession(&[], 0.10);
        assert_eq!(analysis.npv, 0.0);
        assert!(analysis.irr.is_none());
        assert_eq!(analysis.stress_results.len(), 5);
    }

    #[test]
    fn test_npv_sensitivity_range() {
        let cfs = vec![-1000.0, 400.0, 400.0, 400.0];
        let analysis = analyze_concession(&cfs, 0.10);
        // NPV should decrease as discount rate increases
        for window in analysis.npv_sensitivity.windows(2) {
            assert!(
                window[0].1 >= window[1].1,
                "NPV should decrease with higher rate: {}@{:.0}% vs {}@{:.0}%",
                window[0].1,
                window[0].0 * 100.0,
                window[1].1,
                window[1].0 * 100.0
            );
        }
    }
}
