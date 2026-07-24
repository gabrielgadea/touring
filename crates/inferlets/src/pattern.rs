//! Pattern matcher inferlet.

/// Raw evaluate — returns 1 if input is pattern-related.
pub(crate) fn evaluate_raw(input: &str) -> i32 {
    let lower = input.to_lowercase();

    let has_pattern_field =
        lower.contains("\"pattern\"") || lower.contains("'pattern'") || lower.contains("pattern:");

    let has_match_keyword = lower.contains("match")
        || lower.contains("regex")
        || lower.contains("search")
        || lower.contains("find");

    let has_specific_pattern = lower.contains("pattern_matcher") || lower.contains("regex_match");

    (has_pattern_field || has_match_keyword || has_specific_pattern) as i32
}
