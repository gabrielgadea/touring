//! 4-Layer Diagnostic Engine for ACO error classification (ACO-1).
//!
//! Classifies errors into 4 architectural layers:
//! - Syntax: malformed input, parse failures → auto-fix
//! - Logic: incorrect algorithm behavior → re-plan
//! - Contract: violated invariants/preconditions → escalate
//! - Architecture: fundamental structural problems → halt

/// Classification of errors into 4 architectural layers.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum DiagnosticLayer {
    /// Syntax errors: malformed input, parse failures.
    Syntax,
    /// Logic errors: incorrect algorithm behavior.
    Logic,
    /// Contract errors: violated invariants or preconditions.
    Contract,
    /// Architecture errors: fundamental structural problems.
    Architecture,
}

impl std::fmt::Display for DiagnosticLayer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Syntax => write!(f, "Syntax"),
            Self::Logic => write!(f, "Logic"),
            Self::Contract => write!(f, "Contract"),
            Self::Architecture => write!(f, "Architecture"),
        }
    }
}

/// Strategy to apply when a diagnostic fires.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum RetryStrategy {
    /// Auto-fix and retry immediately.
    AutoFix,
    /// Re-plan the current approach.
    RePlan,
    /// Escalate to the caller/user.
    EscalateToUser,
    /// Halt execution entirely.
    Halt,
}

impl std::fmt::Display for RetryStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AutoFix => write!(f, "AutoFix"),
            Self::RePlan => write!(f, "RePlan"),
            Self::EscalateToUser => write!(f, "EscalateToUser"),
            Self::Halt => write!(f, "Halt"),
        }
    }
}

/// Result of classifying an error.
#[derive(Debug, Clone)]
pub struct DiagnosticResult {
    /// The classified layer.
    pub layer: DiagnosticLayer,
    /// Confidence in the classification (0.0–1.0).
    pub confidence: f64,
    /// Original error message.
    pub message: String,
    /// Recommended retry strategy for this layer.
    pub retry_strategy: RetryStrategy,
    /// INS-L8: Optional counter-example extracted from the error context.
    pub counter_example: Option<String>,
}

impl DiagnosticResult {
    /// INS-L8: Attach a counter-example to this result (builder-style).
    pub fn with_counter_example(mut self, example: impl Into<String>) -> Self {
        self.counter_example = Some(example.into());
        self
    }
}

/// Classify an error message into a diagnostic layer.
///
/// Uses keyword matching on the combined error message and context.
/// Falls back to `Logic` if no strong match is found.
pub fn classify_error(error_msg: &str, context: &str) -> DiagnosticResult {
    let combined = format!("{error_msg} {context}").to_lowercase();

    let syntax_keywords = [
        "parse",
        "syntax",
        "unexpected token",
        "invalid format",
        "malformed",
        "utf-8",
        "encoding",
    ];
    let contract_keywords = [
        "invariant",
        "precondition",
        "postcondition",
        "violated",
        "assert",
        "panic",
        "overflow",
    ];
    let arch_keywords = [
        "circular dependency",
        "deadlock",
        "stack overflow",
        "oom",
        "out of memory",
        "cycle detected",
    ];

    let syntax_score = syntax_keywords
        .iter()
        .filter(|k| combined.contains(**k))
        .count();
    let contract_score = contract_keywords
        .iter()
        .filter(|k| combined.contains(**k))
        .count();
    let arch_score = arch_keywords
        .iter()
        .filter(|k| combined.contains(**k))
        .count();

    let max_score = syntax_score.max(contract_score).max(arch_score);

    let layer = if max_score == 0 {
        DiagnosticLayer::Logic
    } else if arch_score == max_score {
        DiagnosticLayer::Architecture
    } else if contract_score == max_score {
        DiagnosticLayer::Contract
    } else {
        DiagnosticLayer::Syntax
    };

    let confidence = if max_score == 0 {
        0.5
    } else {
        (max_score as f64 / 3.0).min(1.0)
    };

    let retry_strategy = suggest_retry_strategy(&layer);

    // INS-L8: extract a counter-example fragment for Syntax errors when the
    // error message contains a code snippet (anything after ":" on the line).
    let counter_example = if layer == DiagnosticLayer::Syntax {
        error_msg
            .find(':')
            .map(|pos| error_msg[pos + 1..].trim().to_string())
            .filter(|s| !s.is_empty())
    } else {
        None
    };

    DiagnosticResult {
        layer,
        confidence,
        message: error_msg.to_string(),
        retry_strategy,
        counter_example,
    }
}

/// Suggest a retry strategy for a given diagnostic layer.
pub fn suggest_retry_strategy(layer: &DiagnosticLayer) -> RetryStrategy {
    match layer {
        DiagnosticLayer::Syntax => RetryStrategy::AutoFix,
        DiagnosticLayer::Logic => RetryStrategy::RePlan,
        DiagnosticLayer::Contract => RetryStrategy::EscalateToUser,
        DiagnosticLayer::Architecture => RetryStrategy::Halt,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_syntax_error() {
        let result = classify_error("parse error: unexpected token at line 5", "");
        assert_eq!(result.layer, DiagnosticLayer::Syntax);
        assert_eq!(result.retry_strategy, RetryStrategy::AutoFix);
    }

    #[test]
    fn test_classify_logic_error() {
        let result = classify_error("wrong output: expected 42 got 0", "computation");
        assert_eq!(result.layer, DiagnosticLayer::Logic);
        assert_eq!(result.retry_strategy, RetryStrategy::RePlan);
    }

    #[test]
    fn test_classify_contract_error() {
        let result = classify_error("invariant violated: node count negative", "");
        assert_eq!(result.layer, DiagnosticLayer::Contract);
        assert_eq!(result.retry_strategy, RetryStrategy::EscalateToUser);
    }

    #[test]
    fn test_classify_architecture_error() {
        let result = classify_error("cycle detected in dependency graph", "deadlock");
        assert_eq!(result.layer, DiagnosticLayer::Architecture);
        assert_eq!(result.retry_strategy, RetryStrategy::Halt);
    }

    #[test]
    fn test_suggest_retry_for_each_layer() {
        assert_eq!(
            suggest_retry_strategy(&DiagnosticLayer::Syntax),
            RetryStrategy::AutoFix
        );
        assert_eq!(
            suggest_retry_strategy(&DiagnosticLayer::Logic),
            RetryStrategy::RePlan
        );
        assert_eq!(
            suggest_retry_strategy(&DiagnosticLayer::Contract),
            RetryStrategy::EscalateToUser
        );
        assert_eq!(
            suggest_retry_strategy(&DiagnosticLayer::Architecture),
            RetryStrategy::Halt
        );
    }

    #[test]
    fn test_unknown_error_defaults_to_logic() {
        let result = classify_error("something went wrong", "");
        assert_eq!(result.layer, DiagnosticLayer::Logic);
        assert!((result.confidence - 0.5).abs() < f64::EPSILON);
    }
}
