//! Equality root cause — Gini coefficient over per-function complexity.
//!
//! High Gini = complexity concentrated in a few god functions. Low Gini =
//! complexity distributed evenly. We invert (`1 - G`) so that higher score
//! always means better.

use super::types::Workspace;

/// Compute the Gini coefficient of a non-negative distribution.
///
/// `G = Σ (2i - n - 1) * x_i / (n * Σ x_i)`, with `x` sorted ascending.
///
/// Edge cases:
/// * `n <= 1` → 0 (single value cannot be unequal)
/// * total <= 0 → 0 (degenerate distribution)
#[must_use]
pub fn gini_coefficient(values: &mut [f64]) -> f64 {
    let n = values.len();
    if n <= 1 {
        return 0.0;
    }

    values.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let total: f64 = values.iter().sum();
    if total <= 0.0 {
        return 0.0;
    }

    let mut numerator = 0.0_f64;
    for (i, &v) in values.iter().enumerate() {
        let idx = (i as f64) + 1.0;
        numerator += ((2.0 * idx) - (n as f64) - 1.0) * v;
    }
    (numerator / ((n as f64) * total)).clamp(0.0, 1.0)
}

/// Compute Gini over per-function cyclomatic complexity in `ws`.
///
/// Falls back to per-file line counts when no function-level data is
/// available (e.g. in tests or for languages without a CC extractor).
#[must_use]
pub fn compute_complexity_gini(ws: &Workspace) -> f64 {
    let mut values: Vec<f64> = ws.function_cc.iter().map(|f| f64::from(f.cc)).collect();
    if values.len() <= 1 {
        values = ws.file_lines.values().map(|v| *v as f64).collect();
    }
    if values.len() <= 1 {
        return 0.0;
    }
    gini_coefficient(&mut values)
}

/// Equality score: direct invert `1 - gini`.
#[must_use]
pub fn equality_score(gini: f64) -> f64 {
    (1.0 - gini).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::super::types::FuncComplexity;
    use super::*;

    #[test]
    fn perfectly_equal_distribution_zero_gini() {
        let mut v = vec![10.0, 10.0, 10.0, 10.0];
        assert!(gini_coefficient(&mut v).abs() < 1e-9);
    }

    #[test]
    fn god_function_high_gini() {
        let mut v = vec![1.0, 1.0, 1.0, 100.0];
        let g = gini_coefficient(&mut v);
        assert!(g > 0.6, "expected high Gini, got {g}");
    }

    #[test]
    fn empty_or_single_value_zero_gini() {
        let mut empty: Vec<f64> = vec![];
        assert!(gini_coefficient(&mut empty).abs() < f64::EPSILON);
        let mut single = vec![42.0];
        assert!(gini_coefficient(&mut single).abs() < f64::EPSILON);
    }

    #[test]
    fn equality_score_invert_endpoints() {
        assert!((equality_score(0.0) - 1.0).abs() < f64::EPSILON);
        assert!((equality_score(1.0) - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn workspace_uses_function_cc_when_present() {
        let mut ws = Workspace::empty("/tmp");
        ws.function_cc = vec![
            FuncComplexity {
                file: "a".into(),
                func: "f".into(),
                cc: 1,
            },
            FuncComplexity {
                file: "a".into(),
                func: "g".into(),
                cc: 100,
            },
            FuncComplexity {
                file: "a".into(),
                func: "h".into(),
                cc: 1,
            },
        ];
        assert!(compute_complexity_gini(&ws) > 0.5);
    }

    #[test]
    fn workspace_falls_back_to_lines_when_no_fn_cc() {
        let mut ws = Workspace::empty("/tmp");
        ws.file_lines.insert("a".into(), 10);
        ws.file_lines.insert("b".into(), 10);
        ws.file_lines.insert("c".into(), 10);
        assert!(compute_complexity_gini(&ws).abs() < 1e-9);
    }
}
