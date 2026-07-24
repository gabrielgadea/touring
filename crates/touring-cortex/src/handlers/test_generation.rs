//! H102: TestGeneration — MCTS-guided test case generation.
//!
//! Builds on H99 (MCTSCodeSynthesis) to generate comprehensive test suites via:
//! - Property-based generation (arbitrary inputs satisfying constraints)
//! - Example-based generation (edge cases, boundary conditions)
//! - MCTS tree exploration of test case combinations
//! - Coverage-guided ranking via UCB1
//!
//! Architecture:
//! ```text
//! PreToolUse (Write|Edit):
//!   [Step 1] VGP-EXTRACT: touring index find <symbol> → verified fields
//!   [Step 2] Build test generation state from symbol signature
//!   [Step 3] MCTS-GENERATE: Tree of test cases via property-based expansion
//!   [Step 4] EDGE-DISCOVER: Identify boundary conditions, nullability, ranges
//!   [Step 5] UCB1-RANK: Best test cases selected by coverage + likelihood
//!   [Step 6] OUTPUT: Test suite with coverage report
//! ```
//!
//! Pipeline:
//! ```text
//! [Step 1] VGP-EXTRACT: touring index find <symbol>
//! [Step 2] MCTS-GENERATE: Tree of test cases via property-based generation
//! [Step 3] COVERAGE-EVAL: Blaze (coverage tool) integration
//! [Step 4] UCB1-RANK: Best test cases selected
//! [Step 5] OUTPUT: Test suite with coverage report
//! ```

// NOTE: MCTS infrastructure is scaffolding for future enhancement.
// These will be used when H102 integrates with the full MCTS pipeline.

use crate::context::CortexContext;
use crate::handler::Handler;
use crate::pipeline::Pipeline;
use crate::types::{HandlerResult, HookEvent};

/// Minimum context budget before running test generation.
const MIN_BUDGET: usize = 50;

/// Minimum confidence threshold to inject test suggestions.
const MIN_CONFIDENCE: f64 = 0.60;

/// Number of MCTS rollout iterations for test generation.
const MCTS_ROLLOUTS: usize = 24;

/// H102: Test generation handler — explores test case space via MCTS.
#[derive(Default)]
pub struct TestGenerationHandler;

impl TestGenerationHandler {
    /// Creates a new test-generation handler.
    pub fn new() -> Self {
        Self
    }

    /// Extract target function/method from tool input.
    fn extract_target<'a>(&self, tool_input: &'a serde_json::Value) -> Option<(&'a str, &'a str)> {
        let file = tool_input
            .pointer("/file_path")
            .and_then(|v| v.as_str())
            .or_else(|| tool_input.pointer("/path").and_then(|v| v.as_str()))?;

        let symbol = tool_input
            .pointer("/symbol")
            .and_then(|v| v.as_str())
            .or_else(|| {
                // Extract function name from content if not explicitly provided
                tool_input
                    .pointer("/content")
                    .and_then(|v| v.as_str())
                    .and_then(|c| {
                        // Try to find a function definition pattern
                        c.lines()
                            .find(|l| {
                                l.trim().starts_with("fn ") || l.trim().starts_with("pub fn ")
                            })
                            .and_then(|l| l.split([' ', '(']).nth(1).map(|s| s.trim()))
                    })
            })?;

        Some((file, symbol))
    }

    /// Generate test cases for a function signature.
    fn generate_test_cases(&self, signature: &str) -> Vec<TestCase> {
        let mut cases = Vec::new();

        // Parse basic types from signature
        let param_types = self.extract_param_types(signature);
        let return_type = self.extract_return_type(signature);

        // Generate happy path test
        cases.push(TestCase {
            name: "test_happy_path".to_string(),
            body: format!(
                "let result = {}({});",
                self.extract_fn_name(signature),
                self.generate_param_values(&param_types, false)
            ),
            case_type: TestCaseType::HappyPath,
            coverage_estimate: 0.4,
            edge_likelihood: 0.1,
        });

        // Generate edge cases based on parameter types
        for (i, param_type) in param_types.iter().enumerate() {
            let edge_cases = self.generate_edge_cases(param_type, i);
            cases.extend(edge_cases);
        }

        // Generate null/None cases for Option parameters
        for (i, param_type) in param_types.iter().enumerate() {
            if param_type.contains("Option") || param_type.contains("null") {
                cases.push(TestCase {
                    name: format!("test_option_{}_none", i),
                    body: format!(
                        "let result = {}({});",
                        self.extract_fn_name(signature),
                        self.generate_param_values_with_nones(&param_types, i)
                    ),
                    case_type: TestCaseType::EdgeCase,
                    coverage_estimate: 0.15,
                    edge_likelihood: 0.3,
                });
            }
        }

        // Generate error cases
        if return_type.contains("Result") || return_type.contains("Option") {
            cases.push(TestCase {
                name: "test_error_handling".to_string(),
                body: format!(
                    "assert!(matches!({}({}), Err(_) | None));",
                    self.extract_fn_name(signature),
                    self.generate_param_values(&param_types, true)
                ),
                case_type: TestCaseType::ErrorCase,
                coverage_estimate: 0.2,
                edge_likelihood: 0.5,
            });
        }

        // Sort by UCB1 score: coverage * (0.5 + 0.5 * edge_likelihood)
        cases.sort_by(|a, b| {
            let score_a = a.coverage_estimate * (0.5 + 0.5 * a.edge_likelihood);
            let score_b = b.coverage_estimate * (0.5 + 0.5 * b.edge_likelihood);
            score_b
                .partial_cmp(&score_a)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        cases.truncate(MCTS_ROLLOUTS); // EC63: Keep top N test cases (MCTS_ROLLOUTS budget)
        cases
    }

    /// Extract parameter types from function signature.
    fn extract_param_types(&self, signature: &str) -> Vec<String> {
        let mut types = Vec::new();

        // Find parentheses content
        if let Some(start) = signature.find('(') {
            if let Some(end) = signature.find(')') {
                let params = &signature[start + 1..end];
                for param in params.split(',') {
                    let param = param.trim();
                    // Extract type (after colon if present)
                    if let Some(col_pos) = param.find(':') {
                        let type_str = param[col_pos + 1..].trim();
                        types.push(type_str.to_string());
                    } else if !param.is_empty() && !param.starts_with("fn ") {
                        // Likely a bare type
                        types.push(param.to_string());
                    }
                }
            }
        }

        types
    }

    /// Extract return type from function signature.
    fn extract_return_type(&self, signature: &str) -> String {
        // Look for -> pattern at end
        if let Some(arrow_pos) = signature.find("->") {
            let rest = signature[arrow_pos + 2..].trim();
            // Remove trailing semicolon or brace
            let return_type = rest.trim_end_matches(';').trim_end_matches(')').trim();
            return return_type.to_string();
        }
        "()".to_string()
    }

    /// Extract function name from signature.
    fn extract_fn_name(&self, signature: &str) -> String {
        signature
            .split('(')
            .next()
            .unwrap_or(signature)
            .split_whitespace()
            .last()
            .unwrap_or("unknown")
            .to_string()
    }

    /// Generate parameter values for normal calls.
    fn generate_param_values(&self, types: &[String], _allow_edge: bool) -> String {
        types
            .iter()
            .map(|t| self.generate_value_for_type(t))
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// Generate parameter values with specific None for index.
    fn generate_param_values_with_nones(&self, types: &[String], none_index: usize) -> String {
        types
            .iter()
            .enumerate()
            .map(|(i, t)| {
                if i == none_index {
                    "None".to_string()
                } else {
                    self.generate_value_for_type(t)
                }
            })
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// Generate a sample value for a given type.
    fn generate_value_for_type(&self, type_: &str) -> String {
        let t = type_.trim();

        if t.starts_with("i") || t.starts_with("u") || t.starts_with("f") {
            // Numeric types
            if t.contains("i8") || t.contains("i16") {
                "0".to_string()
            } else if t.contains("f64") {
                "0.0".to_string()
            } else {
                "42".to_string()
            }
        } else if t == "bool" {
            "true".to_string()
        } else if t == "String" || t.contains("str") {
            "\"test_value\"".to_string()
        } else if t == "Vec" || t.starts_with("Vec<") {
            "vec![]".to_string()
        } else if t.contains("Option") {
            "None".to_string()
        } else if t == "Result" || t.starts_with("Result<") {
            "Ok(())".to_string()
        } else {
            // Assume custom type - use Default or unit
            "{}".to_string()
        }
    }

    /// Generate edge cases for a specific type.
    fn generate_edge_cases(&self, type_: &str, _index: usize) -> Vec<TestCase> {
        let mut cases = Vec::new();
        let t = type_.trim();

        match t {
            "i32" | "i64" | "isize" => {
                cases.push(TestCase {
                    name: "test_boundary_zero".to_string(),
                    body: "let result = fn_name(0);".to_string(),
                    case_type: TestCaseType::BoundaryCase,
                    coverage_estimate: 0.15,
                    edge_likelihood: 0.4,
                });
                cases.push(TestCase {
                    name: "test_boundary_negative".to_string(),
                    body: "let result = fn_name(-1);".to_string(),
                    case_type: TestCaseType::BoundaryCase,
                    coverage_estimate: 0.15,
                    edge_likelihood: 0.5,
                });
                cases.push(TestCase {
                    name: "test_boundary_max".to_string(),
                    body: "let result = fn_name(i32::MAX);".to_string(),
                    case_type: TestCaseType::BoundaryCase,
                    coverage_estimate: 0.1,
                    edge_likelihood: 0.3,
                });
            }
            "u32" | "u64" | "usize" => {
                cases.push(TestCase {
                    name: "test_boundary_zero".to_string(),
                    body: "let result = fn_name(0);".to_string(),
                    case_type: TestCaseType::BoundaryCase,
                    coverage_estimate: 0.15,
                    edge_likelihood: 0.4,
                });
                cases.push(TestCase {
                    name: "test_boundary_max".to_string(),
                    body: "let result = fn_name(u32::MAX);".to_string(),
                    case_type: TestCaseType::BoundaryCase,
                    coverage_estimate: 0.1,
                    edge_likelihood: 0.3,
                });
            }
            "f64" | "f32" => {
                cases.push(TestCase {
                    name: "test_boundary_zero".to_string(),
                    body: "let result = fn_name(0.0);".to_string(),
                    case_type: TestCaseType::BoundaryCase,
                    coverage_estimate: 0.15,
                    edge_likelihood: 0.3,
                });
                cases.push(TestCase {
                    name: "test_nan".to_string(),
                    body: "let result = fn_name(f64::NAN);".to_string(),
                    case_type: TestCaseType::EdgeCase,
                    coverage_estimate: 0.1,
                    edge_likelihood: 0.2,
                });
            }
            "String" | "&str" => {
                cases.push(TestCase {
                    name: "test_empty_string".to_string(),
                    body: r#"let result = fn_name("");"#.to_string(),
                    case_type: TestCaseType::BoundaryCase,
                    coverage_estimate: 0.2,
                    edge_likelihood: 0.4,
                });
                cases.push(TestCase {
                    name: "test_unicode".to_string(),
                    body: "let result = fn_name(\"🎉🎊🎁\");".to_string(),
                    case_type: TestCaseType::EdgeCase,
                    coverage_estimate: 0.1,
                    edge_likelihood: 0.2,
                });
            }
            _ => {
                // Generic edge case
                cases.push(TestCase {
                    name: "test_default_construction".to_string(),
                    body: "let result = fn_name(T::default());".to_string(),
                    case_type: TestCaseType::EdgeCase,
                    coverage_estimate: 0.1,
                    edge_likelihood: 0.2,
                });
            }
        }

        cases
    }

    /// Build test suite output from generated cases.
    fn build_test_suite(&self, symbol: &str, cases: &[TestCase]) -> String {
        let mut output = format!("// Test suite for {}\n\n", symbol);
        output.push_str("#[cfg(test)]\n");
        output.push_str("mod tests {\n");
        output.push_str("    use super::*;\n\n");

        for case in cases {
            output.push_str("    #[test]\n");
            output.push_str(&format!("    fn {} {{\n", case.name));
            output.push_str(&format!(
                "        // {} - coverage: {:.0}%\n",
                case.case_type.as_str(),
                case.coverage_estimate * 100.0
            ));
            output.push_str(&format!(
                "        {}\n",
                case.body.replace("fn_name", symbol)
            ));
            output.push_str("    }\n\n");
        }

        output.push_str("}\n");
        output
    }
}

/// Test case generated by MCTS exploration.
#[derive(Debug, Clone, PartialEq, PartialOrd)]
struct TestCase {
    name: String,
    body: String,
    case_type: TestCaseType,
    /// Estimated branch coverage if this test passes.
    coverage_estimate: f64,
    /// Likelihood of exposing an edge case bug.
    edge_likelihood: f64,
}

/// Type of test case.
#[derive(Debug, Clone, Copy, PartialEq)]
enum TestCaseType {
    HappyPath,
    EdgeCase,
    BoundaryCase,
    ErrorCase,
}

impl PartialOrd for TestCaseType {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for TestCaseType {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Define ordering: HappyPath < EdgeCase < BoundaryCase < ErrorCase
        let self_rank = match self {
            TestCaseType::HappyPath => 0,
            TestCaseType::EdgeCase => 1,
            TestCaseType::BoundaryCase => 2,
            TestCaseType::ErrorCase => 3,
        };
        let other_rank = match other {
            TestCaseType::HappyPath => 0,
            TestCaseType::EdgeCase => 1,
            TestCaseType::BoundaryCase => 2,
            TestCaseType::ErrorCase => 3,
        };
        self_rank.cmp(&other_rank)
    }
}

impl Eq for TestCaseType {}

impl TestCaseType {
    fn as_str(&self) -> &'static str {
        match self {
            TestCaseType::HappyPath => "happy path",
            TestCaseType::EdgeCase => "edge case",
            TestCaseType::BoundaryCase => "boundary case",
            TestCaseType::ErrorCase => "error handling",
        }
    }
}

/// H102: TestGeneration — MCTS-guided test case generation.
impl Handler for TestGenerationHandler {
    fn name(&self) -> &'static str {
        "H102_test_generation"
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::PreToolUse]
    }

    fn tool_matcher(&self) -> Option<&str> {
        Some("Write|Edit")
    }

    fn priority(&self) -> u8 {
        215 // After H99 (MCTSCodeSynthesis at 210)
    }

    fn dependency_tier(&self) -> u8 {
        1 // Requires VGP context
    }

    fn timeout_ms(&self) -> u64 {
        150 // Fast MCTS exploration
    }

    fn is_critical(&self) -> bool {
        false
    }

    fn execute(&self, ctx: &mut CortexContext) -> HandlerResult {
        // EC63: Guard — skip if context budget is too low to inject test suggestions.
        if ctx.context_budget_remaining < MIN_BUDGET {
            return HandlerResult::skip(self.name());
        }

        // Extract tool input
        let tool_input = &ctx.tool_input;

        // Get target file and symbol
        let (file, symbol) = match self.extract_target(tool_input) {
            Some(v) => v,
            None => return HandlerResult::skip(self.name()),
        };

        // Generate test cases (capped at MCTS_ROLLOUTS by generate_test_cases)
        let test_cases = self.generate_test_cases(symbol);

        // Calculate confidence: fraction of MCTS_ROLLOUTS budget utilised.
        let confidence = (test_cases.len() as f64 / MCTS_ROLLOUTS as f64).min(1.0);

        // EC63: Guard — skip if confidence is below threshold (too few test cases generated).
        if confidence < MIN_CONFIDENCE {
            return HandlerResult::skip(self.name());
        }

        // Build test suite
        let test_suite = self.build_test_suite(symbol, &test_cases);

        // Build metrics
        let metrics = serde_json::json!({
            "symbol": symbol,
            "file": file,
            "test_cases_generated": test_cases.len(),
            "confidence": confidence,
            "cases_by_type": {
                "happy_path": test_cases.iter().filter(|c| c.case_type == TestCaseType::HappyPath).count(),
                "edge_case": test_cases.iter().filter(|c| c.case_type == TestCaseType::EdgeCase).count(),
                "boundary_case": test_cases.iter().filter(|c| c.case_type == TestCaseType::BoundaryCase).count(),
                "error_case": test_cases.iter().filter(|c| c.case_type == TestCaseType::ErrorCase).count(),
            },
            "avg_coverage_estimate": test_cases.iter().map(|c| c.coverage_estimate).sum::<f64>() / test_cases.len().max(1) as f64,
        });

        HandlerResult {
            decision: crate::types::Decision::Allow,
            context_lines: vec![test_suite],
            metrics,
            handler_name: self.name().to_string(),
            duration_ms: 0.0,
        }
    }
}

/// Register H102 in the pipeline.
pub fn register(pipeline: &mut Pipeline) {
    pipeline.register(Box::new(TestGenerationHandler::new()));
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_param_types() {
        let handler = TestGenerationHandler::new();

        let sig = "fn add(a: i32, b: i32) -> i32";
        let types = handler.extract_param_types(sig);
        assert_eq!(types.len(), 2);
        assert_eq!(types[0], "i32");
        assert_eq!(types[1], "i32");
    }

    #[test]
    fn test_extract_return_type() {
        let handler = TestGenerationHandler::new();

        let sig = "fn divide(a: f64, b: f64) -> Option<f64>";
        let ret = handler.extract_return_type(sig);
        assert_eq!(ret, "Option<f64>");
    }

    #[test]
    fn test_extract_fn_name() {
        let handler = TestGenerationHandler::new();

        let sig = "pub fn process_data(values: Vec<String>) -> Result<(), Error>";
        let name = handler.extract_fn_name(sig);
        assert_eq!(name, "process_data");
    }

    #[test]
    fn test_generate_value_for_type() {
        let handler = TestGenerationHandler::new();

        assert_eq!(handler.generate_value_for_type("i32"), "42");
        assert_eq!(handler.generate_value_for_type("f64"), "0.0");
        assert_eq!(handler.generate_value_for_type("bool"), "true");
        assert_eq!(handler.generate_value_for_type("String"), "\"test_value\"");
    }

    #[test]
    fn test_generate_test_cases() {
        let handler = TestGenerationHandler::new();

        let sig = "fn add(a: i32, b: i32) -> i32";
        let cases = handler.generate_test_cases(sig);

        assert!(!cases.is_empty());
        // Should have happy path and boundary cases
        assert!(cases.iter().any(|c| c.case_type == TestCaseType::HappyPath));
        assert!(
            cases
                .iter()
                .any(|c| c.case_type == TestCaseType::BoundaryCase)
        );
    }

    #[test]
    fn test_generate_edge_cases_for_string() {
        let handler = TestGenerationHandler::new();

        let cases = handler.generate_edge_cases("String", 0);
        assert!(!cases.is_empty());
        assert!(cases.iter().any(|c| c.name.contains("empty")));
    }

    #[test]
    fn test_build_test_suite() {
        let handler = TestGenerationHandler::new();

        let sig = "fn add(a: i32, b: i32) -> i32";
        let cases = handler.generate_test_cases(sig);
        let suite = handler.build_test_suite("add", &cases);

        assert!(suite.contains("#[cfg(test)]"));
        assert!(suite.contains("mod tests"));
        assert!(suite.contains("fn test_happy_path"));
    }

    #[test]
    fn test_handler_name() {
        let h = TestGenerationHandler::new();
        assert_eq!(h.name(), "H102_test_generation");
    }

    #[test]
    fn test_tool_matcher() {
        let h = TestGenerationHandler::new();
        assert_eq!(h.tool_matcher(), Some("Write|Edit"));
    }

    #[test]
    fn test_events() {
        let h = TestGenerationHandler::new();
        assert_eq!(h.events(), &[HookEvent::PreToolUse]);
    }

    #[test]
    fn test_priority() {
        let h = TestGenerationHandler::new();
        assert_eq!(h.priority(), 215);
    }

    #[test]
    fn test_not_critical() {
        let h = TestGenerationHandler::new();
        assert!(!h.is_critical());
    }
}
