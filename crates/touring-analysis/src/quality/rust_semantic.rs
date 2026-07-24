//! Rust-specific quality signals derived from `touring_code::ast::rust_semantic`.
//!
//! Bridges the deep syn-based analysis (`RustSemanticReport`) into
//! touring-analysis quality scores. Only activates for Rust source files;
//! other languages return `None`.
//!
//! ## Signals exposed
//!
//! - **Unsafe density** — `unsafe_blocks` per KLOC
//! - **Async surface** — `async_fns` count
//! - **Generic abstraction** — total trait bounds + lifetime parameters
//! - **Semantic complexity score** — `[0.0, 1.0]` composite
//!
//! These signals complement the tree-sitter-based quality report
//! (`touring_code::ast::quality::QualityReport`) by adding semantic depth that
//! tree-sitter cannot express.

use serde::{Deserialize, Serialize};

use touring_code::ast::rust_semantic::RustSemanticReport;

/// Rust-specific quality signals distilled from `RustSemanticReport`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RustQualitySignals {
    /// Count of unsafe blocks and `unsafe fn` declarations.
    pub unsafe_count: usize,
    /// Count of `async fn` declarations.
    pub async_count: usize,
    /// Total trait bounds across all generic parameters.
    pub trait_bound_count: usize,
    /// Number of where-clause predicates.
    pub where_clause_count: usize,
    /// Number of distinct lifetimes referenced.
    pub lifetime_count: usize,
    /// Number of `impl` blocks in the file.
    pub impl_block_count: usize,
    /// Maximum aggregated **inherent-impl** item count for a single type — a
    /// God-object / SRP signal (W4 2026-07-02): one type accreting many methods
    /// almost certainly has more than one reason to change. Trait impls are
    /// excluded (implementing many traits is composition, not an SRP smell).
    pub max_type_methods: usize,
    /// Composite semantic complexity in `[0.0, 1.0]` (higher = more complex).
    pub semantic_complexity: f32,
    /// True when the file has no advanced Rust features.
    pub is_simple: bool,
    /// Safety flag: non-zero unsafe blocks require auditor attention.
    pub has_unsafe: bool,
    /// Heuristic: "needs-review" when unsafe > 0 or semantic_complexity > 0.6.
    pub needs_review: bool,
}

impl RustQualitySignals {
    /// Build signals from a Rust source file.
    ///
    /// Returns `None` when the source fails to parse (e.g. incomplete
    /// snippet) so callers can gracefully skip without surfacing errors
    /// in the quality report.
    #[must_use]
    pub fn from_source(source: &str) -> Option<Self> {
        let report = RustSemanticReport::from_source(source).ok()?;
        Some(Self::from_report(&report))
    }

    /// Build signals from a pre-computed report. Prefer this when the
    /// report is already available to avoid re-parsing.
    #[must_use]
    pub fn from_report(report: &RustSemanticReport) -> Self {
        let unsafe_count = report.unsafe_blocks;
        let semantic_complexity = report.semantic_complexity();
        let has_unsafe = unsafe_count > 0;
        // needs_review: any unsafe OR semantic complexity ≥ 0.6.
        let needs_review = has_unsafe || semantic_complexity >= 0.6;
        // God-object / SRP signal: aggregate inherent-impl item counts per type.
        let max_type_methods = {
            let mut per_type: std::collections::HashMap<&str, usize> =
                std::collections::HashMap::new();
            for imp in &report.trait_impls {
                if imp.trait_name.is_none() {
                    *per_type.entry(imp.target_type.as_str()).or_default() += imp.item_count;
                }
            }
            per_type.values().copied().max().unwrap_or(0)
        };
        Self {
            unsafe_count,
            async_count: report.async_fns,
            trait_bound_count: report.total_trait_bounds(),
            where_clause_count: report.where_clauses.len(),
            lifetime_count: report.lifetimes.len(),
            impl_block_count: report.trait_impls.len(),
            max_type_methods,
            semantic_complexity,
            is_simple: report.is_simple(),
            has_unsafe,
            needs_review,
        }
    }

    /// A reward-like score in `[0.0, 1.0]` where **higher = healthier**.
    ///
    /// Heuristic: simple Rust files score high, unsafe or highly abstract
    /// code scores lower. Intended for RL reward feeds, not for display
    /// as a raw metric.
    #[must_use]
    pub fn health_score(&self) -> f32 {
        // Start at 1.0; subtract penalties.
        let mut score = 1.0_f32;
        // Each unsafe block penalizes 5% up to 30%.
        score -= (self.unsafe_count as f32 * 0.05).min(0.30);
        // Complexity penalty scales linearly with semantic_complexity.
        score -= self.semantic_complexity * 0.30;
        score.clamp(0.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_file_has_simple_signals() {
        let s = RustQualitySignals::from_source("").expect("empty is valid rust");
        assert!(s.is_simple);
        assert_eq!(s.unsafe_count, 0);
        assert_eq!(s.async_count, 0);
        assert!(!s.has_unsafe);
        assert!(!s.needs_review);
        assert!((s.health_score() - 1.0).abs() < 0.01);
    }

    #[test]
    fn invalid_source_returns_none() {
        let s = RustQualitySignals::from_source("this is not rust {{");
        assert!(s.is_none());
    }

    #[test]
    fn god_object_max_type_methods_counts_inherent_only() {
        // Two inherent impls on the SAME type aggregate; a trait impl does NOT.
        let src = "\
struct Big;
impl Big { fn a(&self) {} fn b(&self) {} fn c(&self) {} }
impl Big { fn d(&self) {} }
impl std::fmt::Display for Big {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { Ok(()) }
}
struct Small;
impl Small { fn only(&self) {} }
";
        let s = RustQualitySignals::from_source(src).expect("valid rust");
        assert_eq!(
            s.max_type_methods, 4,
            "Big has 4 inherent methods (trait impl excluded), got {}",
            s.max_type_methods
        );
    }

    #[test]
    fn unsafe_code_triggers_review_flag() {
        let src = r#"
            unsafe fn danger() {
                unsafe { std::ptr::null::<u8>(); }
            }
        "#;
        let s = RustQualitySignals::from_source(src).expect("valid");
        assert!(s.has_unsafe);
        assert!(s.needs_review, "unsafe must trigger review");
        assert!(s.unsafe_count >= 2);
        assert!(s.health_score() < 1.0);
    }

    #[test]
    fn abstract_generics_lower_health_score() {
        let simple_src = "fn f() {}";
        let abstract_src = r#"
            async fn complex<'a, T, U>(x: &'a T, y: U) -> &'a U
            where T: Send + Sync + Clone + 'static,
                  U: Iterator<Item = T> + Default + Copy
            {
                unimplemented!()
            }
        "#;
        let simple_signals = RustQualitySignals::from_source(simple_src).expect("valid");
        let abstract_signals = RustQualitySignals::from_source(abstract_src).expect("valid");

        assert!(
            abstract_signals.health_score() < simple_signals.health_score(),
            "abstract code must score lower than simple ({} vs {})",
            abstract_signals.health_score(),
            simple_signals.health_score(),
        );
        assert!(abstract_signals.semantic_complexity > 0.0);
    }

    #[test]
    fn async_functions_counted() {
        let src = "async fn a() {} async fn b() {}";
        let s = RustQualitySignals::from_source(src).expect("valid");
        assert_eq!(s.async_count, 2);
    }

    #[test]
    fn impl_blocks_counted() {
        let src = r#"
            struct Foo;
            impl Foo { fn new() -> Self { Foo } }
            impl std::fmt::Display for Foo {
                fn fmt(&self, _: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { Ok(()) }
            }
        "#;
        let s = RustQualitySignals::from_source(src).expect("valid");
        assert_eq!(s.impl_block_count, 2);
    }

    #[test]
    fn health_score_in_bounds() {
        // health_score must always land in [0, 1] even under extreme input.
        let extreme = r#"
            unsafe fn e1() {}
            unsafe fn e2() {}
            unsafe fn e3() {}
            unsafe fn e4() {}
            unsafe fn e5() {}
            unsafe fn e6() {}
            unsafe fn e7() {}
            unsafe fn e8() {}
            unsafe fn e9() {}
            unsafe fn e10() {}
        "#;
        let s = RustQualitySignals::from_source(extreme).expect("valid");
        assert!((0.0..=1.0).contains(&s.health_score()));
    }

    #[test]
    fn from_report_matches_from_source() {
        let src = "async fn foo<T: Clone>(x: T) -> T { x.clone() }";
        let direct = RustQualitySignals::from_source(src).expect("valid");
        let report = RustSemanticReport::from_source(src).expect("valid");
        let from_rep = RustQualitySignals::from_report(&report);
        assert_eq!(direct.async_count, from_rep.async_count);
        assert_eq!(direct.trait_bound_count, from_rep.trait_bound_count);
        assert_eq!(direct.has_unsafe, from_rep.has_unsafe);
    }
}
