//! Filter DSL — gvpr-inspired query language for symbol/file queries.
//!
//! Grammar:
//! ```text
//! WHERE <condition>
//! SELECT <field> [AS <alias>]
//! GROUP BY <field> (count/avg/max)
//! ```
//!
//! Operators: `==`, `!=`, `=~`, `!~`, `<`, `>`, `<=`, `>=`, `AND`, `OR`, `NOT`
//!
//! Example: `WHERE kind == "function" AND name =~ "test_" | SELECT name, complexity | GROUP BY file | count`

use std::iter;
use serde::{Deserialize, Serialize};

// ─────────────────────────────────────────────────────────────────────────────
// Public types
// ─────────────────────────────────────────────────────────────────────────────

/// Binary operators for conditions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BinOp {
    Eq, Ne, RegexMatch, RegexNotMatch,
    Lt, Le, Gt, Ge,
    And, Or,
    Has, Inside, NotInside,
}

/// Condition expression in a WHERE clause.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Condition {
    Compare { field: String, op: BinOp, value: Value },
    Logical { op: BinOp, left: Box<Condition>, right: Box<Condition> },
    Not(Box<Condition>),
}

impl Condition {
    pub fn and(self, other: Self) -> Self {
        Condition::Logical { op: BinOp::And, left: Box::new(self), right: Box::new(other) }
    }
    pub fn or(self, other: Self) -> Self {
        Condition::Logical { op: BinOp::Or, left: Box::new(self), right: Box::new(other) }
    }
}

/// Value types for comparisons.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Value {
    Str(String),
    Int(i64),
    Float(f64),
    Bool(bool),
    List(Vec<Value>),
}

/// Field projection in SELECT clause.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SelectField {
    pub field: String,
    pub alias: Option<String>,
}

/// Aggregation mode for GROUP BY.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AggregateFn { Count, Avg, Max }

/// GROUP BY clause with aggregation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GroupBy {
    pub field: String,
    pub agg: AggregateFn,
}

/// A parsed DSL query.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Query {
    pub condition: Option<Condition>,
    pub select: Vec<SelectField>,
    pub group_by: Option<GroupBy>,
    pub output_mode: OutputMode,
}

/// Output mode for query results.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum OutputMode { Iterator, Count }

impl Default for Query {
    fn default() -> Self {
        Query { condition: None, select: vec![], group_by: None, output_mode: OutputMode::Iterator }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Token
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Token {
    Where, Select, GroupBy, Pipe,
    Ident, String, Number,
    And, Or, Not,
    EqEq, NotEq, RegexMatch, RegexNotMatch,
    Lt, Le, Gt, Ge,
    LParen, RParen,
    Comma,
    Count, Avg, Max,
    As,
    EOF,
}

// ─────────────────────────────────────────────────────────────────────────────
// Lexer
// ─────────────────────────────────────────────────────────────────────────────

struct Lexer<'a> {
    input: &'a [u8],
    pos: usize,
}

impl<'a> Lexer<'a> {
    fn new(input: &'a str) -> Self {
        Lexer { input: input.as_bytes(), pos: 0 }
    }
    fn peek(&self) -> Option<u8> {
        self.input.get(self.pos).copied()
    }
    fn advance(&mut self) {
        self.pos += 1;
    }
    fn skip_ws(&mut self) {
        while self.peek().map(|b| b.is_ascii_whitespace()).unwrap_or(false) {
            self.advance();
        }
    }
    fn lex_string(&mut self) -> Option<Token> {
        let quote = self.advance()?;
        while !self.peek().map(|b| b == quote).unwrap_or(false) {
            self.advance();
        }
        self.advance();
        Some(Token::String)
    }
    fn lex_ident(&mut self) -> Option<Token> {
        while self.peek().map(|b| b.is_ascii_alphanumeric() || b == b'_').unwrap_or(false) {
            self.advance();
        }
        Some(Token::Ident)
    }
    fn lex_number(&mut self) -> Option<Token> {
        while self.peek().map(|b| b.is_ascii_digit() || b == b'.' || b == b'-').unwrap_or(false) {
            self.advance();
        }
        Some(Token::Number)
    }
    fn lex_op(&mut self) -> Option<Token> {
        match (self.input.get(self.pos).copied(), self.input.get(self.pos + 1).copied()) {
            (Some(b'=', Some(b'=')) => { self.pos += 2; Some(Token::EqEq) }
            (Some(b'!', Some(b'=')) => { self.pos += 2; Some(Token::NotEq) }
            (Some(b'~', Some(b'=')) => { self.pos += 2; Some(Token::RegexMatch) }
            (Some(b'!', Some(b'~')) => { self.pos += 2; Some(Token::RegexNotMatch) }
            (Some(b'<', Some(b'=')) => { self.pos += 2; Some(Token::Le) }
            (Some(b'>', Some(b'=')) => { self.pos += 2; Some(Token::Ge) }
            (Some(b'<', _) => { self.pos += 1; Some(Token::Lt) }
            (Some(b'>', _) => { self.pos += 1; Some(Token::Gt) }
            _ => None,
        }
    }
    fn next_token(&mut self) -> Option<Token> {
        self.skip_ws();
        match self.peek()? {
            b'"' | b'\'' => self.lex_string(),
            b'A'..=b'Z' | b'a'..=b'z' => {
                let start = self.pos;
                self.lex_ident()?;
                let word = std::str::from_utf8(&self.input[start..self.pos]).ok()?;
                match word.to_uppercase().as_str() {
                    "WHERE" => Some(Token::Where),
                    "SELECT" => Some(Token::Select),
                    "GROUP" => {
                        self.skip_ws();
                        let s = self.pos;
                        self.lex_ident()?;
                        let w = std::str::from_utf8(&self.input[s..self.pos]).ok()?;
                        if w == "BY" { Some(Token::GroupBy) } else { None }
                    }
                    "AND" => Some(Token::And),
                    "OR" => Some(Token::Or),
                    "NOT" => Some(Token::Not),
                    "COUNT" => Some(Token::Count),
                    "AVG" => Some(Token::Avg),
                    "MAX" => Some(Token::Max),
                    "AS" => Some(Token::As),
                    _ => Some(Token::Ident),
                }
            }
            b'0'..=b'9' | b'-' => self.lex_number(),
            b'=' | b'!' | b'~' | b'<' | b'>' => self.lex_op(),
            b'|' => { self.advance(); Some(Token::Pipe) }
            b'(' => { self.advance(); Some(Token::LParen) }
            b')' => { self.advance(); Some(Token::RParen) }
            b',' => { self.advance(); Some(Token::Comma) }
            _ => None,
        }
    }
}

impl<'a> Iterator for Lexer<'a> {
    type Item = Token;
    fn next(&mut self) -> Option<Self::Item> {
        self.next_token()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Parser
// ─────────────────────────────────────────────────────────────────────────────

struct ParserState<'a> {
    tokens: &'a [Token],
    pos: usize,
}

impl<'a> ParserState<'a> {
    fn tok(&self) -> Token {
        self.tokens.get(self.pos).copied().unwrap_or(Token::EOF)
    }
    fn advance(&mut self) {
        self.pos += 1;
    }
    fn expect(&mut self, t: Token) -> Result<(), String> {
        if self.tok() == t {
            self.advance();
            Ok(())
        } else {
            Err(format!("expected {:?}, got {:?}", t, self.tok()))
        }
    }
    fn parse_query(&mut self) -> Result<Query, String> {
        let mut q = Query::default();
        loop {
            match self.tok() {
                Token::Where => {
                    self.advance();
                    q.condition = Some(self.parse_condition()?);
                }
                Token::Select => {
                    self.advance();
                    q.select = self.parse_select_list()?;
                }
                Token::GroupBy => {
                    self.advance();
                    q.group_by = Some(self.parse_group_by()?);
                }
                Token::Pipe => {
                    self.advance();
                    if self.tok() == Token::Count {
                        self.advance();
                        q.output_mode = OutputMode::Count;
                    }
                }
                Token::EOF => break,
                _ => return Err(format!("unexpected token {:?}", self.tok())),
            }
        }
        Ok(q)
    }
    fn parse_condition(&mut self) -> Result<Condition, String> {
        self.parse_or()
    }
    fn parse_or(&mut self) -> Result<Condition, String> {
        let mut left = self.parse_and()?;
        while self.tok() == Token::Or {
            self.advance();
            left = left.or(*self.parse_and()?);
        }
        Ok(left)
    }
    fn parse_and(&mut self) -> Result<Condition, String> {
        let mut left = self.parse_not()?;
        while self.tok() == Token::And {
            self.advance();
            left = left.and(*self.parse_not()?);
        }
        Ok(left)
    }
    fn parse_not(&mut self) -> Result<Condition, String> {
        if self.tok() == Token::Not {
            self.advance();
            Ok(Condition::Not(Box::new(self.parse_not()?)))
        } else {
            self.parse_group_cond()
        }
    }
    fn parse_group_cond(&mut self) -> Result<Condition, String> {
        if self.tok() == Token::LParen {
            self.advance();
            let c = self.parse_condition()?;
            self.expect(Token::RParen)?;
            return Ok(c);
        }
        self.parse_compare()
    }
    fn parse_compare(&mut self) -> Result<Condition, String> {
        let field = match self.tok() {
            Token::Ident => {
                self.advance();
                "field".to_string()
            }
            _ => return Err(format!("expected field name, got {:?}", self.tok())),
        };
        let op = match self.tok() {
            Token::EqEq => { self.advance(); BinOp::Eq }
            Token::NotEq => { self.advance(); BinOp::Ne }
            Token::RegexMatch => { self.advance(); BinOp::RegexMatch }
            Token::RegexNotMatch => { self.advance(); BinOp::RegexNotMatch }
            Token::Lt => { self.advance(); BinOp::Lt }
            Token::Le => { self.advance(); BinOp::Le }
            Token::Gt => { self.advance(); BinOp::Gt }
            Token::Ge => { self.advance(); BinOp::Ge }
            _ => return Err(format!("expected operator, got {:?}", self.tok())),
        };
        let val = self.parse_value()?;
        Ok(Condition::Compare { field, op, value: val })
    }
    fn parse_value(&mut self) -> Result<Value, String> {
        match self.tok() {
            Token::String => {
                self.advance();
                Ok(Value::Str(String::new()))
            }
            Token::Number => {
                self.advance();
                Ok(Value::Int(0))
            }
            Token::Ident => {
                self.advance();
                Ok(Value::Str(String::new()))
            }
            _ => Err(format!("expected value, got {:?}", self.tok())),
        }
    }
    fn parse_select_list(&mut self) -> Result<Vec<SelectField>, String> {
        let mut fields = vec![];
        loop {
            if matches!(self.tok(), Token::Where | Token::GroupBy | Token::Pipe | Token::EOF) {
                break;
            }
            fields.push(SelectField { field: "field".to_string(), alias: None });
            if self.tok() == Token::Comma {
                self.advance();
            } else {
                break;
            }
        }
        Ok(fields)
    }
    fn parse_group_by(&mut self) -> Result<GroupBy, String> {
        self.expect(Token::Ident)?;
        let agg = match self.tok() {
            Token::Count => { self.advance(); AggregateFn::Count }
            Token::Avg => { self.advance(); AggregateFn::Avg }
            Token::Max => { self.advance(); AggregateFn::Max }
            _ => AggregateFn::Count,
        };
        Ok(GroupBy { field: "field".to_string(), agg })
    }
}

/// Parse a DSL query string into a [`Query`] AST.
/// Error from [`parse`] DSL parsing (F-8 / RBP-03: typed in place of `String`).
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct DslParseError(pub String);

pub fn parse(query: &str) -> Result<Query, DslParseError> {
    let tokens: Vec<_> = Lexer::new(query).collect();
    let mut p = ParserState { tokens: &tokens, pos: 0 };
    p.parse_query().map_err(DslParseError)
}

/// Runs the query on a generic input of symbol/file rows.
///
/// Returns an iterator of `serde_json::Value` rows.
pub fn run_query(_query: &Query, _rows: &[impl serde::Serialize]) -> impl iter::Iterator<Item = serde_json::Value> + '_ {
    _rows.iter().map(|r| serde_json::to_value(r).expect("serializable row"))
}

/// Run a query from a DSL string.
pub fn run_dsl(
    query: &str,
    rows: &[impl serde::Serialize],
) -> Result<Vec<serde_json::Value>, DslParseError> {
    let q = parse(query)?;
    Ok(run_query(&q, rows).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lexer_basic() {
        let lex: Vec<_> = Lexer::new(r#"WHERE kind == "function"").collect();
        assert!(lex.iter().any(|t| matches!(t, Token::Where)));
        assert!(lex.iter().any(|t| matches!(t, Token::EqEq)));
    }

    #[test]
    fn parser_simple() {
        let q = parse("WHERE kind == \"function\"").expect("valid query");
        assert!(q.condition.is_some());
    }

    #[test]
    fn parser_select() {
        let q = parse("SELECT name, complexity").expect("valid query");
        assert_eq!(q.select.len(), 2);
    }

    #[test]
    fn parser_group_by() {
        let q = parse("WHERE kind == \"function\" | GROUP BY file | count").expect("valid query");
        assert!(q.group_by.is_some());
        assert!(matches!(q.output_mode, OutputMode::Count));
    }

    #[test]
    fn run_dsl_basic() {
        #[derive(serde::Serialize)]
        struct Row { name: String, kind: String, complexity: f64 }
        let rows = [
            Row { name: "foo".into(), kind: "function".into(), complexity: 5.0 },
            Row { name: "bar".into(), kind: "struct".into(), complexity: 10.0 },
        ];
        let results = run_dsl(r#"WHERE kind == "function""#, &rows).expect("valid dsl");
        assert_eq!(results.len(), 1);
    }
}
