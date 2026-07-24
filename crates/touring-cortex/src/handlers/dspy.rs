//! DSPy Rust Native Fallback Validator.
//!
//! Gap 5: When Python DSPy is unavailable or times out, this validator
//! provides basic structural quality checks (bracket balance, indentation,
//! keywords) so the quality gate doesn't silently skip.

/// E5-S8: Rust-native code quality validator.
///
/// Runs basic structural checks when DSPy is unavailable:
/// - Bracket balance (parentheses, braces, brackets)
/// - Keyword presence (fn, struct, impl, let, mut, pub, use, mod, const, static)
/// - Basic indentation consistency
#[derive(Debug, Default)]
pub struct RustNativeValidator;

impl RustNativeValidator {
    /// Create a new validator.
    pub fn new() -> Self {
        Self
    }

    /// Validate a file and return a score (0.0 to 1.0).
    ///
    /// Score interpretation:
    /// - 1.0: Perfect bracket balance, keywords found, good indentation
    /// - 0.7-0.9: Minor issues
    /// - 0.5-0.7: Moderate issues
    /// - 0.0-0.5: Severe issues
    ///
    /// A score < 0.5 with errors should block; 0.5-0.8 should warn.
    pub fn validate(&self, file_path: &str, code: &str) -> ValidationResult {
        let bracket_score = self.check_bracket_balance(code);
        let keyword_score = self.check_keywords(code);
        let indent_score = self.check_indentation(code);

        let overall = bracket_score * 0.4 + keyword_score * 0.3 + indent_score * 0.3;

        ValidationResult {
            score: overall,
            errors: if bracket_score < 0.5 { 1 } else { 0 },
            warnings: if overall < 0.8 { 1 } else { 0 },
            details: format!(
                "bracket={:.2}, keyword={:.2}, indent={:.2}",
                bracket_score, keyword_score, indent_score
            ),
            file_path: file_path.to_string(),
        }
    }

    fn check_bracket_balance(&self, code: &str) -> f64 {
        let mut depth = 0i32;
        let mut max_depth = 0i32;
        let mut in_string = false;
        let mut in_char = false;
        let mut prev_was_escape = false;

        for ch in code.chars() {
            if prev_was_escape {
                prev_was_escape = false;
                continue;
            }
            match ch {
                '"' if !in_char => in_string = !in_string,
                '\'' if !in_string => in_char = !in_char,
                '\\' if in_string || in_char => prev_was_escape = true,
                '(' | '[' | '{' if !in_string && !in_char => {
                    depth += 1;
                    max_depth = max_depth.max(depth);
                }
                ')' | ']' | '}' if !in_string && !in_char => {
                    depth -= 1;
                }
                _ => {}
            }
        }

        if depth == 0 {
            1.0
        } else {
            0.0_f64.max(1.0 - (depth.abs() as f64 * 0.3))
        }
    }

    fn check_keywords(&self, code: &str) -> f64 {
        let keywords = [
            "fn ", "struct", "impl", "let", "pub ", "use ", "mod ", "const", "static", "if",
            "else", "while", "for", "match", "impl", "trait", "enum", "match", "return", "async",
            "await",
        ];
        let found: f64 = keywords.iter().filter(|kw| code.contains(*kw)).count() as f64;
        (found / keywords.len() as f64).min(1.0)
    }

    fn check_indentation(&self, code: &str) -> f64 {
        let lines: Vec<&str> = code.lines().collect();
        if lines.is_empty() {
            return 1.0;
        }

        let mut consistent = 0usize;
        let mut has_inconsistent = false;

        for line in &lines {
            let trimmed = line.trim_start();
            if trimmed.is_empty() {
                continue;
            }
            let prefix = &line[..line.len() - trimmed.len()];
            let indent = prefix.len();

            // Check if indent is reasonable (multiple of 2 or 4, or 1 for first-level)
            if indent == 0 || indent == 1 || indent % 2 == 0 || indent % 4 == 0 {
                consistent += 1;
            } else {
                has_inconsistent = true;
            }
        }

        if !has_inconsistent {
            1.0
        } else {
            consistent as f64 / lines.len() as f64
        }
    }
}

/// Result of validation.
#[derive(Debug)]
pub struct ValidationResult {
    /// Overall score 0.0-1.0.
    pub score: f64,
    /// Number of errors (bracket mismatch, etc.).
    pub errors: u64,
    /// Number of warnings.
    pub warnings: u64,
    /// Human-readable details.
    pub details: String,
    /// File path that was validated.
    pub file_path: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_balanced_brackets() {
        let v = RustNativeValidator::new();
        assert!((v.check_bracket_balance("fn main() { }") - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_unbalanced_brackets() {
        let v = RustNativeValidator::new();
        let score = v.check_bracket_balance("fn main() {");
        assert!(score < 1.0);
    }

    #[test]
    fn test_keywords() {
        let v = RustNativeValidator::new();
        let code = "fn main() { let x = 1; }";
        let score = v.check_keywords(code);
        assert!(score > 0.0);
    }

    #[test]
    fn test_validation_result() {
        let v = RustNativeValidator::new();
        let result = v.validate("test.rs", "fn main() { let x = 1; }");
        assert!(result.score > 0.0);
        assert_eq!(result.errors, 0);
    }
}
