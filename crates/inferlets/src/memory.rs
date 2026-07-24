//! Memory analyzer inferlet.

/// Fragmentation threshold: if more than 70% of indicators match, input is
/// classified as a high-fragmentation / memory-pressure signal (score → 1).
const FRAGMENTATION_THRESHOLD: f64 = 0.7;

/// Raw evaluate — returns 1 if input's memory signal strength meets or
/// exceeds [`FRAGMENTATION_THRESHOLD`], or if any keyword is highly specific.
///
/// Signal strength = matched indicators / total indicators checked (0.0–1.0).
pub(crate) fn evaluate_raw(input: &str) -> i32 {
    let lower = input.to_lowercase();

    // High-specificity keywords: any single match is enough (score ≥ threshold).
    let high_specificity = lower.contains("fragmentation")
        || lower.contains("memory leak")
        || lower.contains("heap exhausted")
        || lower.contains("out of memory");

    if high_specificity {
        return 1;
    }

    let indicators: &[bool] = &[
        lower.contains("memory"),
        lower.contains("alloc"),
        lower.contains("heap"),
        lower.contains("stack"),
        lower.contains("leak"),
        lower.contains("allocation"),
        lower.contains("access"),
        input.trim().starts_with('{'),
    ];

    let matched = indicators.iter().filter(|&&b| b).count() as f64;
    let score = matched / indicators.len() as f64;
    (score >= FRAGMENTATION_THRESHOLD) as i32
}
