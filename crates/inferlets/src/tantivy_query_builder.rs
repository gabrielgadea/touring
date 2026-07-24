//! Tantivy query builder inferlet.
//!
//! Parses a simple boolean query DSL (field:value AND/OR/NOT) and converts
//! to a JSON object representing the tantivy query structure.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Parsed term from the DSL.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Term {
    field: String,
    value: String,
}

/// Boolean operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
enum Op {
    And,
    Or,
    Not,
}

impl fmt::Display for Op {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Op::And => write!(f, "AND"),
            Op::Or => write!(f, "OR"),
            Op::Not => write!(f, "NOT"),
        }
    }
}

/// Simple parsed query structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ParsedQuery {
    query_type: String,
    terms: Vec<Term>,
    ops: Vec<Op>,
}

/// Input DSL for the inferlet.
#[derive(Debug, Deserialize)]
struct Input {
    query_dsl: String,
}

/// Output JSON produced by this inferlet.
#[derive(Debug, Serialize)]
struct Output {
    query: serde_json::Value,
    normalized: String,
}

/// Wrapped output with discriminator for the WASM dispatcher.
#[derive(Debug, Serialize)]
struct WrappedOutput {
    __inferlet__: &'static str,
    result: Output,
}

/// Parse a single field:value term.
fn parse_term(token: &str) -> Option<Term> {
    let token = token.trim();
    if token.is_empty() {
        return None;
    }
    // Split on first colon
    if let Some(pos) = token.find(':') {
        let field = token[..pos].trim().to_string();
        let value = token[pos + 1..].trim().to_string();
        if !field.is_empty() && !value.is_empty() {
            return Some(Term { field, value });
        }
    }
    None
}

/// Tokenize the DSL string into terms and operators.
fn tokenize(dsl: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut paren_depth = 0;

    for ch in dsl.chars() {
        match ch {
            '"' => {
                in_quotes = !in_quotes;
                current.push(ch);
            }
            '(' => {
                if !in_quotes {
                    paren_depth += 1;
                    if !current.is_empty() {
                        tokens.push(current.clone());
                        current.clear();
                    }
                }
                current.push(ch);
            }
            ')' => {
                if !in_quotes {
                    paren_depth -= 1;
                    if !current.is_empty() {
                        tokens.push(current.clone());
                        current.clear();
                    }
                }
                current.push(ch);
            }
            ' ' | '\t' | '\n' if !in_quotes && paren_depth == 0 => {
                if !current.is_empty() {
                    tokens.push(current.clone());
                    current.clear();
                }
            }
            _ => current.push(ch),
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

/// Check if a token is an operator.
fn is_operator(token: &str) -> Option<Op> {
    let upper = token.to_uppercase();
    match upper.as_str() {
        "AND" => Some(Op::And),
        "OR" => Some(Op::Or),
        "NOT" => Some(Op::Not),
        _ => None,
    }
}

/// Normalize the DSL: trim whitespace, collapse multiple spaces.
fn normalize(dsl: &str) -> String {
    let tokens = tokenize(dsl);
    let mut normalized = Vec::new();
    let mut last_was_op = false;
    for token in tokens {
        let op = is_operator(&token);
        if op.is_some() {
            normalized.push(token.to_uppercase());
            last_was_op = true;
        } else {
            if !normalized.is_empty() && !last_was_op {
                normalized.push("AND".to_string());
            }
            normalized.push(token);
            last_was_op = false;
        }
    }
    normalized.join(" ")
}

/// Build a tantivy BooleanQuery JSON from parsed structure.
fn build_tantivy_query(terms: &[Term], ops: &[Op]) -> serde_json::Value {
    let mut must_clauses = Vec::new();
    let mut should_clauses = Vec::new();
    let mut must_not = Vec::new();

    let mut negate_next = false;
    let mut prev_op = Op::And;

    for (i, term) in terms.iter().enumerate() {
        let term_query = serde_json::json!({
            "TermQuery": {
                "field": term.field,
                "value": term.value
            }
        });

        let op = ops.get(i).copied().unwrap_or(Op::And);

        if negate_next {
            must_not.push(term_query);
            negate_next = false;
        } else {
            match (prev_op, op) {
                (Op::Or, Op::Or) | (Op::Or, Op::And) => {
                    should_clauses.push(term_query);
                }
                (Op::And, Op::Or) => {
                    should_clauses.push(term_query);
                }
                _ => {
                    must_clauses.push(term_query);
                }
            }
        }
        prev_op = op;

        if op == Op::Not {
            negate_next = true;
        }
    }

    let mut bool_query = serde_json::Map::new();
    if !must_clauses.is_empty() {
        bool_query.insert("must".to_string(), serde_json::Value::Array(must_clauses));
    }
    if !should_clauses.is_empty() {
        bool_query.insert(
            "should".to_string(),
            serde_json::Value::Array(should_clauses),
        );
    }
    if !must_not.is_empty() {
        bool_query.insert("must_not".to_string(), serde_json::Value::Array(must_not));
    }

    serde_json::json!({
        "type": "BooleanQuery",
        "config": bool_query
    })
}

/// Parse the DSL into structured query.
fn parse_dsl(dsl: &str) -> Option<ParsedQuery> {
    let tokens = tokenize(dsl);
    let mut terms = Vec::new();
    let mut ops = Vec::new();

    for token in tokens {
        if let Some(op) = is_operator(&token) {
            ops.push(op);
        } else {
            // Remove surrounding parentheses if present
            let token = token.trim_start_matches('(').trim_end_matches(')');
            if let Some(term) = parse_term(token) {
                terms.push(term);
            }
        }
    }

    if terms.is_empty() {
        return None;
    }

    let query_type = "BooleanQuery".to_string();

    Some(ParsedQuery {
        query_type,
        terms,
        ops,
    })
}

/// Raw evaluate — parses query DSL and returns structured JSON.
/// Returns 1 if parsing succeeds, 0 if input is invalid or empty.
pub(crate) fn evaluate_raw(input: &str) -> i32 {
    let input = input.trim();
    if input.is_empty() {
        return 0;
    }

    // Parse JSON input
    let input_obj: Input = match serde_json::from_str(input) {
        Ok(v) => v,
        Err(_) => return 0,
    };

    let dsl = input_obj.query_dsl.trim();
    if dsl.is_empty() {
        return 0;
    }

    // Parse and build
    let parsed = match parse_dsl(dsl) {
        Some(p) => p,
        None => return 0,
    };

    let normalized = normalize(dsl);
    let tantivy_query = build_tantivy_query(&parsed.terms, &parsed.ops);

    let output = Output {
        query: tantivy_query,
        normalized,
    };

    let wrapped = WrappedOutput {
        __inferlet__: "tantivy_query_builder",
        result: output,
    };

    // Write to stdout for host to capture
    if let Ok(json) = serde_json::to_string(&wrapped) {
        println!("{}", json);
    }

    1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_term() {
        let term = parse_term("field:value").unwrap();
        assert_eq!(term.field, "field");
        assert_eq!(term.value, "value");
    }

    #[test]
    fn test_parse_term_with_spaces() {
        let term = parse_term("field:some value").unwrap();
        assert_eq!(term.field, "field");
        assert_eq!(term.value, "some value");
    }

    #[test]
    fn test_tokenize_basic() {
        let tokens = tokenize("field:value AND field2:value2");
        assert_eq!(tokens.len(), 3);
        assert_eq!(tokens[0], "field:value");
        assert_eq!(tokens[1], "AND");
        assert_eq!(tokens[2], "field2:value2");
    }

    #[test]
    fn test_tokenize_quoted() {
        let tokens = tokenize("field:\"hello world\" AND field2:value2");
        assert_eq!(tokens.len(), 3);
        assert_eq!(tokens[0], "field:\"hello world\"");
        assert_eq!(tokens[1], "AND");
        assert_eq!(tokens[2], "field2:value2");
    }

    #[test]
    fn test_normalize_basic() {
        let norm = normalize("field:value AND field2:value2");
        assert_eq!(norm, "field:value AND field2:value2");
    }

    #[test]
    fn test_parse_dsl_single_term() {
        let parsed = parse_dsl("field:value").unwrap();
        assert_eq!(parsed.terms.len(), 1);
        assert_eq!(parsed.terms[0].field, "field");
    }

    #[test]
    fn test_parse_dsl_with_and() {
        let parsed = parse_dsl("field:value AND field2:value2").unwrap();
        assert_eq!(parsed.terms.len(), 2);
        assert_eq!(parsed.ops.len(), 1);
        assert_eq!(parsed.ops[0], Op::And);
    }

    #[test]
    fn test_parse_dsl_with_or() {
        let parsed = parse_dsl("field:value OR field2:value2").unwrap();
        assert_eq!(parsed.terms.len(), 2);
        assert_eq!(parsed.ops[0], Op::Or);
    }

    #[test]
    fn test_parse_dsl_with_not() {
        let parsed = parse_dsl("field:value NOT field2:value2").unwrap();
        assert_eq!(parsed.terms.len(), 2);
        assert_eq!(parsed.ops[0], Op::Not);
    }

    #[test]
    fn test_build_tantivy_query() {
        let terms = vec![
            Term {
                field: "author".to_string(),
                value: "tolkien".to_string(),
            },
            Term {
                field: "genre".to_string(),
                value: "fantasy".to_string(),
            },
        ];
        let ops = vec![Op::And];
        let query = build_tantivy_query(&terms, &ops);
        assert_eq!(query["type"], "BooleanQuery");
        assert!(query["config"]["must"].is_array());
    }

    #[test]
    fn test_evaluate_raw_valid_input() {
        let input = r#"{"query_dsl":"author:tolkien AND genre:fantasy"}"#;
        let result = evaluate_raw(input);
        assert_eq!(result, 1);
    }

    #[test]
    fn test_evaluate_raw_empty_dsl() {
        let input = r#"{"query_dsl":""}"#;
        let result = evaluate_raw(input);
        assert_eq!(result, 0);
    }

    #[test]
    fn test_evaluate_raw_invalid_json() {
        let input = "not json";
        let result = evaluate_raw(input);
        assert_eq!(result, 0);
    }

    #[test]
    fn test_evaluate_raw_missing_dsl() {
        let input = r#"{"other":"value"}"#;
        let result = evaluate_raw(input);
        assert_eq!(result, 0);
    }

    #[test]
    fn test_nested_parentheses() {
        let parsed = parse_dsl("(field:value) AND (field2:value2)").unwrap();
        assert_eq!(parsed.terms.len(), 2);
    }

    #[test]
    fn test_multiple_and() {
        let parsed = parse_dsl("a:1 AND b:2 AND c:3").unwrap();
        assert_eq!(parsed.terms.len(), 3);
        assert_eq!(parsed.ops.len(), 2);
    }

    #[test]
    fn test_mixed_operators() {
        let parsed = parse_dsl("a:1 AND b:2 OR c:3").unwrap();
        assert_eq!(parsed.terms.len(), 3);
    }
}
