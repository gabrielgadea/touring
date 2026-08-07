//! Polyglot semantic analysis via tree-sitter — the cross-language analog of
//! [`crate::ast::rust_semantic`] (`syn`).
//!
//! `rust_semantic` uses `syn` to read Rust-only semantics (generics with trait
//! bounds, lifetimes, `unsafe`). Those concepts do not exist in Python or
//! TypeScript. This module extracts the semantic surface that *is* meaningful
//! and comparable across dynamic / gradually-typed languages, using the same
//! tree-sitter grammars the rest of `touring-code` already loads:
//!
//! - **Type parameters** (generics) — Python PEP 695 `def f[T]()`, TS `<T>`.
//! - **Async functions** — concurrency surface (parity with `async_fns`).
//! - **Decorators** — Python `@decorator`, TS `@Component` (attribute analog).
//! - **Type-annotation coverage** — the gradual-typing rigor signal.
//! - **Dynamic escapes** — Python `eval`/`exec`/`getattr`, TS `any` — the
//!   type-safety escape hatch that is the cross-language analog of `unsafe`.
//!
//! Supported languages: **Python, TypeScript, JavaScript**. Other languages
//! return [`AstError::ParseFailed`] — Rust callers use `rust_semantic`, and
//! Go/Java/C++ semantic parity is future work (they currently get the
//! tree-sitter *shape* via [`crate::ast::symbols`] but no deep semantic report).
//!
//! # Example
//!
//! ```no_run
//! use touring_code::ast::languages::Lang;
//! use touring_code::ast::polyglot_semantic::PolyglotSemanticReport;
//!
//! let src = r#"
//!     import asyncio
//!
//!     @app.route("/")
//!     async def handler(req: Request) -> Response:
//!         return Response(eval(req.body))
//! "#;
//! let report = PolyglotSemanticReport::from_source(Lang::Python, src).unwrap();
//! assert_eq!(report.async_fns, 1);
//! assert_eq!(report.decorators, 1);
//! assert_eq!(report.dynamic_escapes, 1); // the `eval` call
//! ```

use serde::{Deserialize, Serialize};
use tree_sitter::{Node, Parser};

use crate::ast::error::{AstError, AstResult};
use crate::ast::languages::Lang;

/// Full semantic analysis report for a single Python / TypeScript / JavaScript
/// source file. The cross-language analog of
/// [`crate::ast::rust_semantic::RustSemanticReport`].
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PolyglotSemanticReport {
    /// `"python"`, `"typescript"`, or `"javascript"`.
    pub language: String,
    /// Generic type parameters (`<T>` in TS, PEP 695 `[T]` in Python).
    pub type_params: usize,
    /// Count of `async` functions / methods (concurrency surface).
    pub async_fns: usize,
    /// Count of decorators (`@decorator` / `@Component`).
    pub decorators: usize,
    /// Count of class definitions.
    pub classes: usize,
    /// Count of function / method definitions.
    pub functions: usize,
    /// Parameters that carry a type annotation.
    pub typed_params: usize,
    /// Total parameters seen (annotated or not; `self`/`cls` excluded).
    pub total_params: usize,
    /// Dynamic / type-safety escapes — Python `eval`/`exec`/`getattr`, TS
    /// `any`. The cross-language analog of Rust's `unsafe` count.
    pub dynamic_escapes: usize,
    /// Count of top-level items in the file.
    pub item_count: usize,
}

impl PolyglotSemanticReport {
    /// Parse `source` under `lang` and build a full semantic report.
    ///
    /// # Errors
    /// Returns [`AstError::ParseFailed`] when `lang` is not one of
    /// Python/TypeScript/JavaScript, or when the tree-sitter parser fails to
    /// produce a tree.
    pub fn from_source(lang: Lang, source: &str) -> AstResult<Self> {
        if !matches!(lang, Lang::Python | Lang::TypeScript | Lang::JavaScript) {
            return Err(AstError::ParseFailed(format!(
                "polyglot_semantic supports Python/TypeScript/JavaScript, not {lang:?} \
                 (Rust uses rust_semantic; Go/Java/C++ semantic parity is future work)"
            )));
        }
        let mut parser = Parser::new();
        parser
            .set_language(&lang.tree_sitter_language())
            .map_err(|e| AstError::ParseFailed(format!("set_language: {e}")))?;
        let tree = parser
            .parse(source, None)
            .ok_or_else(|| AstError::ParseFailed("tree-sitter parse returned None".into()))?;

        let bytes = source.as_bytes();
        let root = tree.root_node();
        let mut report = Self {
            language: lang_label(lang).to_string(),
            item_count: root.named_child_count(),
            ..Default::default()
        };
        report.analyze(root, lang, bytes);
        Ok(report)
    }

    /// Fraction of parameters that carry a type annotation, in `[0.0, 1.0]`.
    /// A file with no parameters is vacuously fully annotated (`1.0`).
    #[must_use]
    pub fn annotation_coverage(&self) -> f32 {
        if self.total_params == 0 {
            return 1.0;
        }
        self.typed_params as f32 / self.total_params as f32
    }

    /// Heuristic semantic complexity in `[0.0, 1.0]` (higher = more surface to
    /// reason about). Mirrors the shape of
    /// [`crate::ast::rust_semantic::RustSemanticReport::semantic_complexity`].
    #[must_use]
    pub fn semantic_complexity(&self) -> f32 {
        let raw = self.type_params * 2
            + self.async_fns
            + self.decorators
            + self.classes * 2
            + self.dynamic_escapes * 4
            + self.functions;
        (raw as f32 / 200.0).clamp(0.0, 1.0)
    }

    /// True when the file has no advanced semantic features (no generics,
    /// classes, decorators, async, or dynamic escapes).
    #[must_use]
    pub fn is_simple(&self) -> bool {
        self.type_params == 0
            && self.classes == 0
            && self.decorators == 0
            && self.async_fns == 0
            && self.dynamic_escapes == 0
    }

    /// Iterative (stack-based, no recursion → no stack overflow on deep trees)
    /// full-tree walk. Visits every node — named and anonymous — so `async`
    /// keyword tokens are observable.
    fn analyze(&mut self, root: Node, lang: Lang, bytes: &[u8]) {
        let mut stack = vec![root];
        while let Some(node) = stack.pop() {
            // Only named nodes are counted: anonymous keyword tokens (`class`,
            // `function`, `async`) share `kind()` with declaration nodes and
            // would double-count. `async` is still observed via `has_child_kind`
            // on the (named) function node, which inspects anonymous children.
            if node.is_named() {
                match lang {
                    Lang::Python => self.visit_python(node, bytes),
                    Lang::TypeScript | Lang::JavaScript => self.visit_tsjs(node, bytes),
                    _ => {}
                }
            }
            for i in 0..node.child_count() as u32 {
                if let Some(child) = node.child(i) {
                    stack.push(child);
                }
            }
        }
    }

    fn visit_python(&mut self, node: Node, bytes: &[u8]) {
        match node.kind() {
            "function_definition" => {
                self.functions += 1;
                if has_child_kind(node, "async") {
                    self.async_fns += 1;
                }
            }
            "class_definition" => self.classes += 1,
            "decorator" => self.decorators += 1,
            "type_parameter" => self.type_params += 1,
            "parameters" | "lambda_parameters" => {
                let (typed, total) = count_python_params(node, bytes);
                self.typed_params += typed;
                self.total_params += total;
            }
            "call" => {
                if let Some(func) = node.child_by_field_name("function")
                    && let Ok(name) = func.utf8_text(bytes)
                    && is_python_dynamic_escape(name)
                {
                    self.dynamic_escapes += 1;
                }
            }
            _ => {}
        }
    }

    fn visit_tsjs(&mut self, node: Node, bytes: &[u8]) {
        match node.kind() {
            "function_declaration"
            | "generator_function_declaration"
            | "method_definition"
            | "arrow_function"
            | "function_expression"
            | "function"
            | "generator_function" => {
                self.functions += 1;
                if has_child_kind(node, "async") {
                    self.async_fns += 1;
                }
            }
            "class_declaration" | "class" => self.classes += 1,
            "decorator" => self.decorators += 1,
            "type_parameter" => self.type_params += 1,
            "predefined_type" if node.utf8_text(bytes).map(|t| t == "any").unwrap_or(false) => {
                self.dynamic_escapes += 1;
            }
            "required_parameter" | "optional_parameter" => {
                self.total_params += 1;
                if has_child_kind(node, "type_annotation") {
                    self.typed_params += 1;
                }
            }
            _ => {}
        }
    }
}

// ─── free helpers ─────────────────────────────────────────────────────────

fn lang_label(lang: Lang) -> &'static str {
    match lang {
        Lang::Python => "python",
        Lang::TypeScript => "typescript",
        Lang::JavaScript => "javascript",
        _ => "unknown",
    }
}

/// True when `node` has any direct child (named or anonymous) of the given kind.
fn has_child_kind(node: Node, kind: &str) -> bool {
    (0..node.child_count() as u32).any(|i| node.child(i).map(|c| c.kind() == kind).unwrap_or(false))
}

fn is_python_dynamic_escape(name: &str) -> bool {
    matches!(
        name,
        "eval" | "exec" | "getattr" | "setattr" | "compile" | "__import__" | "globals" | "locals"
    )
}

/// Tally `(typed, total)` parameters from a Python `parameters` node, skipping
/// the implicit `self` / `cls` receiver so method annotation coverage is not
/// diluted.
fn count_python_params(node: Node, bytes: &[u8]) -> (usize, usize) {
    let mut typed = 0;
    let mut total = 0;
    for i in 0..node.named_child_count() as u32 {
        let Some(child) = node.named_child(i) else {
            continue;
        };
        match child.kind() {
            "identifier" => {
                let name = child.utf8_text(bytes).unwrap_or("");
                if name != "self" && name != "cls" {
                    total += 1;
                }
            }
            "default_parameter" | "list_splat_pattern" | "dictionary_splat_pattern" => {
                total += 1;
            }
            "typed_parameter" | "typed_default_parameter" => {
                total += 1;
                typed += 1;
            }
            _ => {}
        }
    }
    (typed, total)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Node-kind reality is validated empirically here (VP-Scout Chain 5):
    // each assertion pins a semantic count to a known source, so a wrong
    // tree-sitter node kind fails a specific assert rather than passing silently.

    #[test]
    fn python_extracts_async_decorator_dynamic_and_annotations() {
        let src = r#"
@app.route("/")
async def handler(req: Request, retries: int, flag) -> Response:
    return Response(eval(req.body))

class Service:
    def sync_method(self, name: str) -> None:
        pass
"#;
        let r = PolyglotSemanticReport::from_source(Lang::Python, src).unwrap();
        assert_eq!(r.language, "python");
        assert_eq!(r.async_fns, 1, "one `async def`");
        assert_eq!(r.functions, 2, "handler + sync_method");
        assert_eq!(r.decorators, 1, "@app.route");
        assert_eq!(r.classes, 1, "class Service");
        assert_eq!(r.dynamic_escapes, 1, "the eval() call");
        // handler: req(typed), retries(typed), flag(untyped) => 2/3 ; sync_method:
        // self skipped, name(typed) => 1/1. Total typed=3, total=4.
        assert_eq!(r.typed_params, 3);
        assert_eq!(r.total_params, 4);
        assert!(!r.is_simple());
    }

    #[test]
    fn python_generic_subscript_is_not_a_type_param() {
        // `Generic[T]` is a subscript, NOT a PEP 695 type parameter — must not
        // be miscounted as a generic declaration.
        let src = "from typing import Generic, TypeVar\nT = TypeVar('T')\nclass Box(Generic[T]):\n    pass\n";
        let r = PolyglotSemanticReport::from_source(Lang::Python, src).unwrap();
        assert_eq!(r.type_params, 0, "subscript is not a generic param decl");
        assert_eq!(r.classes, 1);
    }

    #[test]
    fn typescript_extracts_generics_async_any_and_typed_params() {
        let src = r#"
class Cache<K, V> {
    async get(key: K, fallback: any): Promise<V> {
        return fallback as V;
    }
}

function identity<T>(x: T): T { return x; }
"#;
        let r = PolyglotSemanticReport::from_source(Lang::TypeScript, src).unwrap();
        assert_eq!(r.language, "typescript");
        assert!(
            r.type_params >= 3,
            "K, V, T => at least 3, got {}",
            r.type_params
        );
        assert_eq!(r.async_fns, 1, "one async method");
        assert_eq!(r.classes, 1);
        assert!(r.dynamic_escapes >= 1, "the `any` type is an escape");
        // get(key: K typed, fallback: any typed) + identity(x: T typed) => 3 typed.
        assert_eq!(r.typed_params, 3);
        assert_eq!(r.total_params, 3);
        assert!(!r.is_simple());
    }

    #[test]
    fn javascript_async_function_no_types() {
        let src =
            "async function fetchAll(urls) { return await Promise.all(urls.map(u => fetch(u))); }";
        let r = PolyglotSemanticReport::from_source(Lang::JavaScript, src).unwrap();
        assert_eq!(r.language, "javascript");
        assert_eq!(r.async_fns, 1);
        // JS has no type annotations → vacuous full coverage.
        assert_eq!(r.annotation_coverage(), 1.0);
    }

    #[test]
    fn unsupported_language_is_rejected() {
        let err = PolyglotSemanticReport::from_source(Lang::Rust, "fn main() {}");
        assert!(err.is_err(), "Rust must route to rust_semantic, not here");
    }

    #[test]
    fn health_signals_penalize_dynamic_escapes() {
        // A file drenched in eval() must score a lower semantic_complexity-driven
        // health than a clean typed file.
        let dirty = "def f(x):\n    return eval(x) + exec(x) + eval(x)\n";
        let clean = "def f(x: int) -> int:\n    return x + 1\n";
        let dr = PolyglotSemanticReport::from_source(Lang::Python, dirty).unwrap();
        let cr = PolyglotSemanticReport::from_source(Lang::Python, clean).unwrap();
        assert!(dr.dynamic_escapes >= 3);
        assert_eq!(cr.dynamic_escapes, 0);
        assert!(dr.semantic_complexity() > cr.semantic_complexity());
    }
}
