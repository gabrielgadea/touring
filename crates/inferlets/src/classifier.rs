//! Semantic classifier inferlet.

/// Minimum weighted match ratio to classify input as classification-related.
/// A score below this threshold returns 0 (not matched).
const THRESHOLD: f64 = 0.30;

/// Raw evaluate — returns 1 if the input's classification signal strength
/// meets or exceeds [`THRESHOLD`], 0 otherwise.
///
/// Signal strength = matched indicators / total indicators checked (0.0–1.0).
pub(crate) fn evaluate_raw(input: &str) -> i32 {
    let lower = input.to_lowercase();

    let indicators: &[bool] = &[
        lower.contains("classify") || lower.contains("classific"),
        lower.contains("semantic"),
        lower.contains("similar"),
        lower.contains("tfidf"),
        lower.contains("embedding"),
        lower.contains("categorize") || lower.contains("categoriz"),
        lower.contains("candidate") || lower.contains("candidates"),
        lower.contains("\"text\"") || lower.contains("'text'") || lower.contains("text:"),
        lower.contains("intent") || lower.contains("cila") || lower.contains("level"),
    ];

    let matched = indicators.iter().filter(|&&b| b).count() as f64;
    let score = matched / indicators.len() as f64;
    (score >= THRESHOLD) as i32
}
