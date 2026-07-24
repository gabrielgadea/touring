//! Query DSL — Recursive descent parser for structured symbol queries.
//!
//! Supports:
//! - Simple symbol name: `MyStruct`
//! - Wildcard: `insert_*`
//! - Kind filter: `kind:fn insert_*`
//! - File filter: `file:knowledge.rs insert_*`
//! - Module path: `touring_hooks::shared::MetadataDedup`
//! - Logical AND: `kind:fn AND file:shared/*`
//! - Logical OR: `kind:fn OR kind:impl`

use std::fmt;

/// Parsed query expression.
#[derive(Debug, Clone, PartialEq)]
pub enum QueryExpr {
    /// Match a symbol by name (supports glob `*`).
    Symbol(String),
    /// Match by kind (fn, struct, impl, trait, enum, mod, const, type, macro).
    Kind(String, Box<QueryExpr>),
    /// Match by file path (supports glob `*`).
    File(String, Box<QueryExpr>),
    /// Logical AND of two expressions.
    And(Box<QueryExpr>, Box<QueryExpr>),
    /// Logical OR of two expressions.
    Or(Box<QueryExpr>, Box<QueryExpr>),
}

/// Parse error with location context.
#[derive(Debug, Clone)]
pub struct QueryDslError {
    /// Human-readable description of what went wrong while parsing.
    pub message: String,
    /// Byte offset into the query string where the error was detected.
    pub position: usize,
}

impl fmt::Display for QueryDslError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "query parse error at {}: {}",
            self.position, self.message
        )
    }
}

impl std::error::Error for QueryDslError {}

impl QueryExpr {
    /// Parse a query string into a `QueryExpr`.
    ///
    /// Grammar (informal):
    ///   expr     = or_expr
    ///   or_expr  = and_expr ("OR" and_expr)*
    ///   and_expr = atom ("AND" atom)*
    ///   atom     = "kind:" IDENT expr
    ///            | "file:" PATTERN expr
    ///            | SYMBOL_PATTERN
    pub fn parse(input: &str) -> Result<Self, QueryDslError> {
        let input = input.trim();
        if input.is_empty() {
            return Err(QueryDslError {
                message: "empty query".into(),
                position: 0,
            });
        }
        let tokens = tokenize(input)?;
        let (expr, rest) = parse_or(&tokens)?;
        if !rest.is_empty() {
            return Err(QueryDslError {
                message: format!("unexpected trailing tokens: {:?}", rest),
                position: 0,
            });
        }
        Ok(expr)
    }

    /// Check if this expression matches a given symbol.
    pub fn matches(&self, symbol_name: &str, file_path: &str, kind: &str) -> bool {
        match self {
            QueryExpr::Symbol(pattern) => glob_match(pattern, symbol_name),
            QueryExpr::Kind(k, inner) => {
                k.eq_ignore_ascii_case(kind) && inner.matches(symbol_name, file_path, kind)
            }
            QueryExpr::File(pattern, inner) => {
                glob_match(pattern, file_path) && inner.matches(symbol_name, file_path, kind)
            }
            QueryExpr::And(a, b) => {
                a.matches(symbol_name, file_path, kind) && b.matches(symbol_name, file_path, kind)
            }
            QueryExpr::Or(a, b) => {
                a.matches(symbol_name, file_path, kind) || b.matches(symbol_name, file_path, kind)
            }
        }
    }
}

// ── Tokenizer ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Word(String),
    And,
    Or,
    KindPrefix(String), // "kind:fn" → KindPrefix("fn")
    FilePrefix(String), // "file:foo.rs" → FilePrefix("foo.rs")
}

fn tokenize(input: &str) -> Result<Vec<Token>, QueryDslError> {
    let mut tokens = Vec::new();
    let mut chars = input.char_indices().peekable();

    while let Some(&(pos, ch)) = chars.peek() {
        if ch.is_whitespace() {
            chars.next();
            continue;
        }

        // Collect a word
        let start = pos;
        let mut word = String::new();
        while let Some(&(_, c)) = chars.peek() {
            if c.is_whitespace() {
                break;
            }
            word.push(c);
            chars.next();
        }

        // Classify token
        if word == "AND" {
            tokens.push(Token::And);
        } else if word == "OR" {
            tokens.push(Token::Or);
        } else if let Some(kind) = word.strip_prefix("kind:") {
            if kind.is_empty() {
                return Err(QueryDslError {
                    message: "kind: prefix requires a value".into(),
                    position: start,
                });
            }
            tokens.push(Token::KindPrefix(kind.to_string()));
        } else if let Some(file) = word.strip_prefix("file:") {
            if file.is_empty() {
                return Err(QueryDslError {
                    message: "file: prefix requires a value".into(),
                    position: start,
                });
            }
            tokens.push(Token::FilePrefix(file.to_string()));
        } else {
            tokens.push(Token::Word(word));
        }
    }

    Ok(tokens)
}

// ── Recursive descent parser ─────────────────────────────────────────────

fn parse_or(tokens: &[Token]) -> Result<(QueryExpr, &[Token]), QueryDslError> {
    let (mut left, mut rest) = parse_and(tokens)?;
    while rest.first() == Some(&Token::Or) {
        let (right, r) = parse_and(rest.get(1..).unwrap_or(&[]))?;
        left = QueryExpr::Or(Box::new(left), Box::new(right));
        rest = r;
    }
    Ok((left, rest))
}

fn parse_and(tokens: &[Token]) -> Result<(QueryExpr, &[Token]), QueryDslError> {
    let (mut left, mut rest) = parse_atom(tokens)?;
    while rest.first() == Some(&Token::And) {
        let (right, r) = parse_atom(rest.get(1..).unwrap_or(&[]))?;
        left = QueryExpr::And(Box::new(left), Box::new(right));
        rest = r;
    }
    Ok((left, rest))
}

fn parse_atom(tokens: &[Token]) -> Result<(QueryExpr, &[Token]), QueryDslError> {
    match tokens.first() {
        Some(Token::KindPrefix(kind)) => {
            let rest = tokens.get(1..).unwrap_or(&[]);
            // Next token is the symbol pattern (or a wildcard `*` match-all)
            if rest.is_empty() {
                // kind filter with implicit match-all
                Ok((
                    QueryExpr::Kind(kind.clone(), Box::new(QueryExpr::Symbol("*".into()))),
                    rest,
                ))
            } else if matches!(rest.first(), Some(Token::And | Token::Or)) {
                // kind filter alone (before AND/OR)
                Ok((
                    QueryExpr::Kind(kind.clone(), Box::new(QueryExpr::Symbol("*".into()))),
                    rest,
                ))
            } else {
                let (inner, rest2) = parse_atom(rest)?;
                Ok((QueryExpr::Kind(kind.clone(), Box::new(inner)), rest2))
            }
        }
        Some(Token::FilePrefix(file)) => {
            let rest = tokens.get(1..).unwrap_or(&[]);
            if rest.is_empty() || matches!(rest.first(), Some(Token::And | Token::Or)) {
                Ok((
                    QueryExpr::File(file.clone(), Box::new(QueryExpr::Symbol("*".into()))),
                    rest,
                ))
            } else {
                let (inner, rest2) = parse_atom(rest)?;
                Ok((QueryExpr::File(file.clone(), Box::new(inner)), rest2))
            }
        }
        Some(Token::Word(w)) => Ok((QueryExpr::Symbol(w.clone()), tokens.get(1..).unwrap_or(&[]))),
        Some(Token::And | Token::Or) => Err(QueryDslError {
            message: "unexpected logical operator without left operand".into(),
            position: 0,
        }),
        None => Err(QueryDslError {
            message: "unexpected end of query".into(),
            position: 0,
        }),
    }
}

// ── Glob matching ────────────────────────────────────────────────────────

/// Simple glob matching: `*` matches any sequence of characters.
fn glob_match(pattern: &str, text: &str) -> bool {
    if pattern == "*" {
        return true;
    }

    let parts: Vec<&str> = pattern.split('*').collect();
    if parts.len() == 1 {
        // No wildcard — exact or substring match
        return text == pattern || text.contains(pattern);
    }

    let mut pos = 0;
    for (i, part) in parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }
        match text[pos..].find(part) {
            Some(idx) => {
                if i == 0 && idx != 0 {
                    // First part must match at start
                    return false;
                }
                pos += idx + part.len();
            }
            None => return false,
        }
    }

    // If pattern ends with *, any trailing text is OK
    if !pattern.ends_with('*') {
        pos == text.len()
    } else {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_symbol() {
        let expr = QueryExpr::parse("MyStruct").unwrap();
        assert!(expr.matches("MyStruct", "any.rs", "struct"));
        assert!(!expr.matches("Other", "any.rs", "struct"));
    }

    #[test]
    fn test_wildcard() {
        let expr = QueryExpr::parse("insert_*").unwrap();
        assert!(expr.matches("insert_symbol_event", "knowledge.rs", "fn"));
        assert!(!expr.matches("delete_symbol", "knowledge.rs", "fn"));
    }

    #[test]
    fn test_kind_filter() {
        let expr = QueryExpr::parse("kind:fn insert_*").unwrap();
        assert!(expr.matches("insert_symbol_event", "knowledge.rs", "fn"));
        assert!(!expr.matches("insert_symbol_event", "knowledge.rs", "struct"));
    }

    #[test]
    fn test_file_filter() {
        let expr = QueryExpr::parse("file:knowledge.rs insert_*").unwrap();
        assert!(expr.matches("insert_symbol_event", "knowledge.rs", "fn"));
        assert!(!expr.matches("insert_symbol_event", "other.rs", "fn"));
    }

    #[test]
    fn test_and() {
        let expr = QueryExpr::parse("kind:fn AND file:shared/*").unwrap();
        assert!(expr.matches("anything", "shared/foo.rs", "fn"));
        assert!(!expr.matches("anything", "shared/foo.rs", "struct"));
        assert!(!expr.matches("anything", "other.rs", "fn"));
    }

    #[test]
    fn test_or() {
        let expr = QueryExpr::parse("kind:fn OR kind:struct").unwrap();
        assert!(expr.matches("X", "a.rs", "fn"));
        assert!(expr.matches("X", "a.rs", "struct"));
        assert!(!expr.matches("X", "a.rs", "impl"));
    }

    #[test]
    fn test_empty_query() {
        assert!(QueryExpr::parse("").is_err());
    }

    #[test]
    fn test_module_path() {
        let expr = QueryExpr::parse("touring_hooks::shared::MetadataDedup").unwrap();
        assert!(expr.matches("touring_hooks::shared::MetadataDedup", "lib.rs", "struct"));
    }
}
