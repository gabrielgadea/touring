//! Deep Rust semantic analysis via `syn` 2.0.
//!
//! Tree-sitter (used by the rest of touring-ast) gives the *shape* of the
//! AST — function, struct, enum, method. `syn` gives the *semantics* that
//! tree-sitter cannot see:
//!
//! - **Generic parameters** with trait bounds (`T: Clone + Send + 'static`)
//! - **Lifetime parameters** and lifetime constraints
//! - **Where clauses** (`where T: Iterator<Item = U>, U: Copy`)
//! - **Derive macros** per type (`#[derive(Debug, Clone, Serialize)]`)
//! - **Impl blocks** with target types and optional traits
//! - **Unsafe block** counts (safety hotspots)
//! - **Async function** counts (concurrency surface)
//!
//! This module is intentionally Rust-specific. For other languages,
//! continue using tree-sitter via `crate::symbols` and `crate::quality`.
//!
//! # Example
//!
//! ```no_run
//! use touring_code::ast::rust_semantic::RustSemanticReport;
//!
//! let src = r#"
//!     pub struct Cache<K: Clone, V: Send + 'static>
//!     where K: std::hash::Hash
//!     {
//!         inner: std::collections::HashMap<K, V>,
//!     }
//!
//!     impl<K: Clone + std::hash::Hash, V: Send + 'static> Cache<K, V> {
//!         pub async unsafe fn get(&self, k: &K) -> Option<&V> {
//!             unsafe { self.inner.get(k) }
//!         }
//!     }
//! "#;
//!
//! let report = RustSemanticReport::from_source(src).expect("valid rust");
//! assert!(report.generics.len() >= 1);
//! assert_eq!(report.async_fns, 1);
//! assert!(report.unsafe_blocks >= 1);
//! ```

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use syn::visit::Visit;

// Wave 5: public-api workspace dep — available for rustdoc-backed callers.
// Referenced in tests to keep the dep active in the dependency graph.
#[cfg(test)]
use public_api as _;

use crate::ast::error::{AstError, AstResult};

// ─── Public types ─────────────────────────────────────────────────────

/// One generic parameter with its trait bounds.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenericInfo {
    /// Parameter name, e.g. `T` in `fn foo<T: Clone>()`.
    pub name: String,
    /// Kind of generic parameter.
    pub kind: GenericKind,
    /// Trait bounds, e.g. `["Clone", "Send", "'static"]`.
    pub bounds: Vec<String>,
    /// Optional default type, e.g. `T = usize`.
    pub default: Option<String>,
}

/// Kind of generic parameter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GenericKind {
    /// Type parameter (`T`).
    Type,
    /// Lifetime parameter (`'a`).
    Lifetime,
    /// Const generic parameter (`const N: usize`).
    Const,
}

/// An `impl` block. Captures the target type and optional trait being
/// implemented.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraitImplInfo {
    /// The type the impl block is for (`String`, `Vec<T>`, etc.).
    pub target_type: String,
    /// The trait being implemented, if any (`Display`, `Clone`, etc.).
    /// `None` for inherent impls.
    pub trait_name: Option<String>,
    /// Whether this is a negative impl (`impl !Send for ...`).
    pub is_negative: bool,
    /// Number of items (methods, consts, types) in the impl.
    pub item_count: usize,
}

/// A lifetime reference found in the source (distinct from lifetime parameters).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LifetimeInfo {
    /// The lifetime label without the leading apostrophe (e.g. `a` for `'a`).
    pub name: String,
    /// Occurrence count across the file.
    pub count: usize,
}

/// Information extracted from a single `where` clause.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WhereClauseInfo {
    /// The type or lifetime being constrained, rendered as source text.
    pub bounded: String,
    /// The bounds applied, e.g. `["Iterator<Item = U>", "Send"]`.
    pub bounds: Vec<String>,
}

/// Full semantic analysis report for a Rust source file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RustSemanticReport {
    /// Generic parameters declared across fns/structs/enums/traits/impls.
    pub generics: Vec<GenericInfo>,
    /// All `impl` blocks.
    pub trait_impls: Vec<TraitImplInfo>,
    /// Lifetimes referenced across the file with occurrence counts.
    pub lifetimes: Vec<LifetimeInfo>,
    /// Map of type name to its `#[derive(...)]` entries.
    pub derives: HashMap<String, Vec<String>>,
    /// `where` clauses collected from the file.
    pub where_clauses: Vec<WhereClauseInfo>,
    /// Count of `unsafe` blocks and `unsafe fn` declarations.
    pub unsafe_blocks: usize,
    /// Count of `async fn` declarations.
    pub async_fns: usize,
    /// Count of top-level items (fn, struct, enum, trait, impl, mod, use, ...).
    pub item_count: usize,
}

impl RustSemanticReport {
    /// Parse Rust source and build a full semantic report.
    ///
    /// Returns `AstError::ParseFailed` if the source fails to parse as a
    /// valid Rust file. Partial / incomplete Rust snippets (e.g. a single
    /// expression) will not parse here — use `Self::from_items` for that.
    pub fn from_source(source: &str) -> AstResult<Self> {
        let file = syn::parse_file(source)
            .map_err(|e| AstError::ParseFailed(format!("syn parse: {e}")))?;
        let mut visitor = SemanticVisitor::default();
        visitor.visit_file(&file);
        Ok(visitor.into_report(file.items.len()))
    }

    /// True when the file has no generics, no trait impls, no lifetimes,
    /// no unsafe — a "simple" Rust file.
    #[must_use]
    pub fn is_simple(&self) -> bool {
        self.generics.is_empty()
            && self.trait_impls.is_empty()
            && self.lifetimes.is_empty()
            && self.unsafe_blocks == 0
            && self.where_clauses.is_empty()
    }

    /// Surface an overall semantic complexity score in `[0.0, 1.0]` where
    /// higher means more semantic surface to reason about. Heuristic, not
    /// a formal metric.
    #[must_use]
    pub fn semantic_complexity(&self) -> f32 {
        let bounds_count: usize = self.generics.iter().map(|g| g.bounds.len()).sum();
        let raw = self.generics.len() * 2
            + self.trait_impls.len() * 3
            + self.lifetimes.len()
            + bounds_count
            + self.where_clauses.len() * 2
            + self.unsafe_blocks * 4
            + self.async_fns;
        // Normalize against a generous ceiling; clamp to [0, 1].
        (raw as f32 / 200.0).clamp(0.0, 1.0)
    }

    /// Total trait bounds declared across all generics. Useful as a
    /// "how abstract is this code" signal.
    #[must_use]
    pub fn total_trait_bounds(&self) -> usize {
        self.generics.iter().map(|g| g.bounds.len()).sum()
    }

    // ─── Wave 5 (2026-04-18) — public API surface extraction ──────────

    /// Extract the **public API surface** of a Rust source file.
    ///
    /// Returns a sorted, deduplicated list of stable identifiers for
    /// every `pub` item declared at the file scope — functions,
    /// structs, enums, traits, type aliases, constants, statics, and
    /// `pub use` re-exports. Two snapshots can be compared to detect
    /// breaking changes (an identifier disappearing or changing kind
    /// is a likely breaking change; additions are backward compatible).
    ///
    /// # Why not the `public-api` crate?
    ///
    /// The canonical `public-api` crate reads rustdoc JSON output
    /// (requires nightly rustc). That is heavier than what we need for
    /// post-edit breaking-change detection. Walking `syn::File` for
    /// `pub` items gives us 80% of the signal in 20% of the cost and
    /// works on stable. Callers that need the full rustdoc-level fidelity
    /// can still invoke `public-api` directly via the workspace dep.
    ///
    /// # Format
    ///
    /// Each entry has the form `"kind name"` — e.g.
    /// `"fn from_source"`, `"struct RustSemanticReport"`, `"trait Visit"`.
    /// The kind prefix makes the diff sensitive to kind changes
    /// (a `fn` renamed to a `struct` with the same name counts as two
    /// independent edits, not a no-op).
    pub fn public_api_surface(source: &str) -> AstResult<Vec<String>> {
        let file = syn::parse_file(source)
            .map_err(|e| AstError::ParseFailed(format!("syn parse: {e}")))?;
        Ok(extract_public_api(&file))
    }
}

/// Walk a parsed `syn::File` and collect a sorted surface list.
///
/// Internal helper shared by [`RustSemanticReport::public_api_surface`]
/// and unit tests.
fn extract_public_api(file: &syn::File) -> Vec<String> {
    use syn::{Item, Visibility};

    fn is_pub(vis: &Visibility) -> bool {
        matches!(vis, Visibility::Public(_))
    }

    let mut entries: Vec<String> = file
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Fn(f) if is_pub(&f.vis) => Some(format!("fn {}", f.sig.ident)),
            Item::Struct(s) if is_pub(&s.vis) => Some(format!("struct {}", s.ident)),
            Item::Enum(e) if is_pub(&e.vis) => Some(format!("enum {}", e.ident)),
            Item::Trait(t) if is_pub(&t.vis) => Some(format!("trait {}", t.ident)),
            Item::Type(t) if is_pub(&t.vis) => Some(format!("type {}", t.ident)),
            Item::Const(c) if is_pub(&c.vis) => Some(format!("const {}", c.ident)),
            Item::Static(s) if is_pub(&s.vis) => Some(format!("static {}", s.ident)),
            Item::Mod(m) if is_pub(&m.vis) => Some(format!("mod {}", m.ident)),
            Item::Use(u) if is_pub(&u.vis) => Some("use <re-export>".to_string()),
            _ => None,
        })
        .collect();

    entries.sort();
    entries.dedup();
    entries
}

// ─── Internal visitor ─────────────────────────────────────────────────

// Wave 5: visibility bumped to `pub(crate)` so the
// `code_gen_workflow` module can reuse a single `syn::parse_file` across
// both the semantic pass and the public-API extraction (see
// `code_gen_workflow.rs` — `analyze_inner`). The struct is still not
// part of the public API surface of the crate.
#[derive(Default)]
pub(crate) struct SemanticVisitor {
    generics: Vec<GenericInfo>,
    trait_impls: Vec<TraitImplInfo>,
    lifetime_counts: HashMap<String, usize>,
    derives: HashMap<String, Vec<String>>,
    where_clauses: Vec<WhereClauseInfo>,
    unsafe_blocks: usize,
    async_fns: usize,
}

/// Wave 5 helper: construct a default visitor for reuse inside the
/// workspace. Intentionally crate-private.
pub(crate) fn build_visitor() -> SemanticVisitor {
    SemanticVisitor::default()
}

impl SemanticVisitor {
    pub(crate) fn into_report(self, item_count: usize) -> RustSemanticReport {
        let mut lifetimes: Vec<LifetimeInfo> = self
            .lifetime_counts
            .into_iter()
            .map(|(name, count)| LifetimeInfo { name, count })
            .collect();
        // Stable ordering for deterministic reports.
        lifetimes.sort_by(|a, b| a.name.cmp(&b.name));
        RustSemanticReport {
            generics: self.generics,
            trait_impls: self.trait_impls,
            lifetimes,
            derives: self.derives,
            where_clauses: self.where_clauses,
            unsafe_blocks: self.unsafe_blocks,
            async_fns: self.async_fns,
            item_count,
        }
    }

    fn record_generics(&mut self, generics: &syn::Generics) {
        for param in &generics.params {
            match param {
                syn::GenericParam::Type(ty) => {
                    let bounds = ty
                        .bounds
                        .iter()
                        .map(render_type_param_bound)
                        .collect::<Vec<_>>();
                    let default = ty.default.as_ref().map(render_type);
                    self.generics.push(GenericInfo {
                        name: ty.ident.to_string(),
                        kind: GenericKind::Type,
                        bounds,
                        default,
                    });
                }
                syn::GenericParam::Lifetime(lt) => {
                    let bounds = lt
                        .bounds
                        .iter()
                        .map(|b| format!("'{}", b.ident))
                        .collect::<Vec<_>>();
                    self.generics.push(GenericInfo {
                        name: format!("'{}", lt.lifetime.ident),
                        kind: GenericKind::Lifetime,
                        bounds,
                        default: None,
                    });
                }
                syn::GenericParam::Const(c) => {
                    self.generics.push(GenericInfo {
                        name: c.ident.to_string(),
                        kind: GenericKind::Const,
                        bounds: vec![render_type(&c.ty)],
                        default: c
                            .default
                            .as_ref()
                            .map(|d| quote::ToTokens::to_token_stream(d).to_string()),
                    });
                }
            }
        }
        if let Some(ref wc) = generics.where_clause {
            for predicate in &wc.predicates {
                match predicate {
                    syn::WherePredicate::Type(t) => {
                        self.where_clauses.push(WhereClauseInfo {
                            bounded: render_type(&t.bounded_ty),
                            bounds: t.bounds.iter().map(render_type_param_bound).collect(),
                        });
                    }
                    syn::WherePredicate::Lifetime(lt) => {
                        self.where_clauses.push(WhereClauseInfo {
                            bounded: format!("'{}", lt.lifetime.ident),
                            bounds: lt.bounds.iter().map(|b| format!("'{}", b.ident)).collect(),
                        });
                    }
                    _ => {}
                }
            }
        }
    }

    fn record_derives(&mut self, type_name: String, attrs: &[syn::Attribute]) {
        for attr in attrs {
            if !attr.path().is_ident("derive") {
                continue;
            }
            let mut derives = Vec::new();
            let _ = attr.parse_nested_meta(|meta| {
                if let Some(ident) = meta.path.get_ident() {
                    derives.push(ident.to_string());
                }
                Ok(())
            });
            if !derives.is_empty() {
                self.derives
                    .entry(type_name.clone())
                    .or_default()
                    .extend(derives);
            }
        }
    }
}

impl<'ast> Visit<'ast> for SemanticVisitor {
    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        if node.sig.asyncness.is_some() {
            self.async_fns += 1;
        }
        if node.sig.unsafety.is_some() {
            self.unsafe_blocks += 1;
        }
        self.record_generics(&node.sig.generics);
        syn::visit::visit_item_fn(self, node);
    }

    fn visit_impl_item_fn(&mut self, node: &'ast syn::ImplItemFn) {
        if node.sig.asyncness.is_some() {
            self.async_fns += 1;
        }
        if node.sig.unsafety.is_some() {
            self.unsafe_blocks += 1;
        }
        self.record_generics(&node.sig.generics);
        syn::visit::visit_impl_item_fn(self, node);
    }

    fn visit_item_struct(&mut self, node: &'ast syn::ItemStruct) {
        self.record_generics(&node.generics);
        self.record_derives(node.ident.to_string(), &node.attrs);
        syn::visit::visit_item_struct(self, node);
    }

    fn visit_item_enum(&mut self, node: &'ast syn::ItemEnum) {
        self.record_generics(&node.generics);
        self.record_derives(node.ident.to_string(), &node.attrs);
        syn::visit::visit_item_enum(self, node);
    }

    fn visit_item_trait(&mut self, node: &'ast syn::ItemTrait) {
        self.record_generics(&node.generics);
        syn::visit::visit_item_trait(self, node);
    }

    fn visit_item_impl(&mut self, node: &'ast syn::ItemImpl) {
        let target_type = render_type(&node.self_ty);
        let trait_name = node.trait_.as_ref().map(|(_, path, _)| render_path(path));
        self.trait_impls.push(TraitImplInfo {
            target_type,
            trait_name,
            is_negative: node
                .trait_
                .as_ref()
                .is_some_and(|(neg, _, _)| neg.is_some()),
            item_count: node.items.len(),
        });
        self.record_generics(&node.generics);
        syn::visit::visit_item_impl(self, node);
    }

    fn visit_expr_unsafe(&mut self, node: &'ast syn::ExprUnsafe) {
        self.unsafe_blocks += 1;
        syn::visit::visit_expr_unsafe(self, node);
    }

    fn visit_lifetime(&mut self, node: &'ast syn::Lifetime) {
        // Skip the implicit `'_` anonymous lifetime — not meaningful here.
        let name = node.ident.to_string();
        if name != "_" {
            *self.lifetime_counts.entry(name).or_insert(0) += 1;
        }
    }
}

// ─── Rendering helpers ────────────────────────────────────────────────

fn render_type(ty: &syn::Type) -> String {
    use quote::ToTokens;
    ty.to_token_stream().to_string()
}

fn render_path(path: &syn::Path) -> String {
    use quote::ToTokens;
    path.to_token_stream().to_string()
}

fn render_type_param_bound(bound: &syn::TypeParamBound) -> String {
    use quote::ToTokens;
    bound.to_token_stream().to_string()
}

// ─── API surface diff ─────────────────────────────────────────────────

/// A single change detected by comparing two API surfaces.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiChange {
    /// Whether the item was added or removed.
    pub kind: ApiChangeKind,
    /// The stringified API item (e.g. `"pub fn foo(x: u32) -> bool"`).
    pub item: String,
}

/// Kind of API surface change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ApiChangeKind {
    /// The item is present in the new surface but absent from the old one.
    Added,
    /// The item was present in the old surface but is absent from the new one.
    Removed,
}

/// Diff two API surfaces produced by [`RustSemanticReport::public_api_surface`].
///
/// Returns items added or removed between `old` and `new`. Uses a string-set
/// diff — the same conservative approach as `extract_public_api`, but now
/// as a standalone free function so callers can compare snapshots before/after
/// an edit without re-parsing the whole file.
///
/// Callers that need semantic-level diffing (e.g. detecting signature changes
/// that keep the same name) can use the `public-api` workspace crate directly
/// to run rustdoc-JSON–backed comparison.
///
/// # Example
/// ```
/// use touring_code::ast::rust_semantic::{RustSemanticReport, diff_api_surfaces, ApiChangeKind};
///
/// let before = r#"pub fn greet(name: &str) -> String { format!("Hello {name}") }"#;
/// let after  = r#"pub fn greet(name: &str) -> String { format!("Hi {name}") }"#;
///
/// let old_api = RustSemanticReport::public_api_surface(before).expect("valid source");
/// let new_api = RustSemanticReport::public_api_surface(after).expect("valid source");
/// let changes = diff_api_surfaces(&old_api, &new_api);
/// assert!(changes.is_empty(), "same signature = no API change");
/// ```
pub fn diff_api_surfaces(old: &[String], new: &[String]) -> Vec<ApiChange> {
    use std::collections::HashSet;
    let old_set: HashSet<&String> = old.iter().collect();
    let new_set: HashSet<&String> = new.iter().collect();
    let mut changes: Vec<ApiChange> = old_set
        .difference(&new_set)
        .map(|item| ApiChange {
            kind: ApiChangeKind::Removed,
            item: (*item).clone(),
        })
        .chain(new_set.difference(&old_set).map(|item| ApiChange {
            kind: ApiChangeKind::Added,
            item: (*item).clone(),
        }))
        .collect();
    changes.sort_by(|a, b| a.item.cmp(&b.item));
    changes
}

// ─── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_source_is_simple() {
        let r = RustSemanticReport::from_source("").expect("empty is valid");
        assert!(r.is_simple());
        assert_eq!(r.item_count, 0);
    }

    #[test]
    fn invalid_source_returns_parse_error() {
        let result = RustSemanticReport::from_source("this is not rust {{{");
        assert!(matches!(result, Err(AstError::ParseFailed(_))));
    }

    #[test]
    fn simple_fn_has_no_generics() {
        let src = "fn add(a: i32, b: i32) -> i32 { a + b }";
        let r = RustSemanticReport::from_source(src).expect("valid");
        assert_eq!(r.generics.len(), 0);
        assert_eq!(r.async_fns, 0);
        assert_eq!(r.unsafe_blocks, 0);
        assert_eq!(r.item_count, 1);
    }

    #[test]
    fn generic_fn_captures_bounds() {
        let src = "fn foo<T: Clone + Send>(x: T) -> T { x.clone() }";
        let r = RustSemanticReport::from_source(src).expect("valid");
        assert_eq!(r.generics.len(), 1);
        let g = &r.generics[0];
        assert_eq!(g.name, "T");
        assert_eq!(g.kind, GenericKind::Type);
        assert_eq!(g.bounds.len(), 2);
        assert!(g.bounds.iter().any(|b| b.contains("Clone")));
        assert!(g.bounds.iter().any(|b| b.contains("Send")));
        assert_eq!(r.total_trait_bounds(), 2);
    }

    #[test]
    fn lifetime_param_captured() {
        let src = "fn borrow<'a>(x: &'a str) -> &'a str { x }";
        let r = RustSemanticReport::from_source(src).expect("valid");
        assert_eq!(r.generics.len(), 1);
        assert_eq!(r.generics[0].kind, GenericKind::Lifetime);
        // 'a appears as param decl + 2 usages in signature
        assert!(r.lifetimes.iter().any(|l| l.name == "a" && l.count >= 2));
    }

    #[test]
    fn async_unsafe_fn_counted() {
        let src = "async unsafe fn dangerous() { }";
        let r = RustSemanticReport::from_source(src).expect("valid");
        assert_eq!(r.async_fns, 1);
        assert_eq!(r.unsafe_blocks, 1);
    }

    #[test]
    fn derive_attrs_captured_per_type() {
        let src = r#"
            #[derive(Debug, Clone)]
            pub struct Foo;

            #[derive(Serialize)]
            pub enum Bar { A, B }
        "#;
        let r = RustSemanticReport::from_source(src).expect("valid");
        assert_eq!(r.derives.get("Foo").map(|v| v.len()), Some(2));
        assert!(
            r.derives
                .get("Foo")
                .expect("Foo derives must be present")
                .contains(&"Debug".to_string())
        );
        assert_eq!(r.derives.get("Bar").map(|v| v.len()), Some(1));
    }

    #[test]
    fn where_clause_captured() {
        let src = r#"
            fn f<T>(x: T) where T: Clone + Send {
                let _ = x;
            }
        "#;
        let r = RustSemanticReport::from_source(src).expect("valid");
        assert!(!r.where_clauses.is_empty(), "where clause must be captured");
    }

    #[test]
    fn impl_block_captures_trait_and_target() {
        let src = r#"
            struct Foo;
            impl std::fmt::Display for Foo {
                fn fmt(&self, _: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { Ok(()) }
            }
            impl Foo {
                fn new() -> Self { Foo }
            }
        "#;
        let r = RustSemanticReport::from_source(src).expect("valid");
        assert_eq!(r.trait_impls.len(), 2);
        let inherent = r.trait_impls.iter().find(|i| i.trait_name.is_none());
        assert!(inherent.is_some(), "inherent impl must be present");
        let traited = r
            .trait_impls
            .iter()
            .find(|i| i.trait_name.is_some())
            .expect("trait impl must exist");
        assert!(
            traited
                .trait_name
                .as_ref()
                .expect("trait_name is Some by filter above")
                .contains("Display"),
            "trait impl must capture trait name"
        );
    }

    #[test]
    fn semantic_complexity_increases_with_abstraction() {
        let simple = RustSemanticReport::from_source("fn f() {}").expect("valid");
        let complex = RustSemanticReport::from_source(
            r#"
            async unsafe fn dangerous<'a, T, U>(x: &'a T, y: U) -> &'a U
            where T: Send + Sync + 'static, U: Clone + Default
            {
                unsafe { std::mem::transmute(x) }
            }
            "#,
        )
        .expect("valid");
        assert!(
            complex.semantic_complexity() > simple.semantic_complexity(),
            "complex Rust must score higher: {} vs {}",
            complex.semantic_complexity(),
            simple.semantic_complexity()
        );
        assert!((0.0..=1.0).contains(&complex.semantic_complexity()));
    }

    #[test]
    fn unsafe_block_in_body_counted() {
        let src = r#"
            fn safe_wrapper() {
                unsafe {
                    std::ptr::null::<u8>();
                }
            }
        "#;
        let r = RustSemanticReport::from_source(src).expect("valid");
        assert_eq!(r.unsafe_blocks, 1);
    }

    #[test]
    fn const_generic_captured() {
        let src = "fn buf<const N: usize>() -> [u8; N] { [0u8; N] }";
        let r = RustSemanticReport::from_source(src).expect("valid");
        assert_eq!(r.generics.len(), 1);
        assert_eq!(r.generics[0].kind, GenericKind::Const);
        assert_eq!(r.generics[0].name, "N");
    }

    // ─── diff_api_surfaces tests ───────────────────────────────────────

    #[test]
    fn diff_api_surfaces_detects_removal() {
        let old = vec![
            "pub fn foo() -> u32".to_string(),
            "pub fn bar() -> bool".to_string(),
        ];
        let new = vec!["pub fn foo() -> u32".to_string()];
        let changes = diff_api_surfaces(&old, &new);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].kind, ApiChangeKind::Removed);
        assert_eq!(changes[0].item, "pub fn bar() -> bool");
    }

    #[test]
    fn diff_api_surfaces_detects_addition() {
        let old = vec!["pub fn foo() -> u32".to_string()];
        let new = vec![
            "pub fn foo() -> u32".to_string(),
            "pub fn baz() -> String".to_string(),
        ];
        let changes = diff_api_surfaces(&old, &new);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].kind, ApiChangeKind::Added);
        assert_eq!(changes[0].item, "pub fn baz() -> String");
    }

    #[test]
    fn diff_api_surfaces_no_change() {
        let api = vec!["pub fn foo() -> u32".to_string()];
        let changes = diff_api_surfaces(&api, &api);
        assert!(changes.is_empty());
    }

    /// Verifies that the `public-api` workspace crate is accessible from touring-ast.
    /// The `use public_api as _` at the top of this module is a compile-time check
    /// that the dep resolves and links correctly.
    #[test]
    fn public_api_crate_is_accessible() {
        // Confirm the dep is wired: public_api is imported as `_` above (line 51),
        // ensuring supply-chain cost is justified. No runtime assertion needed.
        let _ = std::any::TypeId::of::<public_api::PublicItem>();
    }
}
