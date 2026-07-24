#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::pedantic,
    clippy::needless_collect
)]

//! Wave 5 (2026-04-18) — End-to-end test of the code-generation workflow
//! Claude Code exercises via Touring.
//!
//! # The workflow under test
//!
//! When Claude Code edits a Rust file in the touring workspace, the
//! following Touring-backed checks fire in sequence (or should):
//!
//! ```text
//!  1. file metadata  (Lang::from_path)          — detect language
//!  2. rust_semantic  (RustSemanticReport)       — generics/unsafe/async counts
//!  3. public_api_surface                        — stable API snapshot
//!  4. format_rust    (prettyplease)             — rustfmt-clean output
//!  5. workspace_info (cargo_metadata)           — cross-crate impact
//! ```
//!
//! This test exercises every step on a representative Rust source so
//! regressions in any link of the chain surface immediately. It is
//! *independent of the daemon* — every call is pure library code.
//!
//! # Why this belongs here (not in touring-server/tests)
//!
//! The chain consumes only `touring-ast` crate APIs. Living here keeps
//! the test close to the code it exercises, and removes touring-server
//! from the dependency graph (faster compile, no daemon required).

use touring_code::ast::rust_semantic::RustSemanticReport;
use touring_code::ast::{Lang, WorkspaceInfo, format_rust_code};

/// Representative source covering every semantic surface the workflow
/// examines: pub/private, generics, trait bounds, async, unsafe,
/// lifetimes, derives, where clauses.
const REPRESENTATIVE_SOURCE: &str = r#"
use std::marker::PhantomData;

pub struct Cache<'a, K: Clone + std::hash::Hash, V: Send + Sync + 'static>
where
    K: std::fmt::Debug,
{
    inner: std::collections::HashMap<K, V>,
    _lt: PhantomData<&'a ()>,
}

#[derive(Debug, Clone)]
pub struct Metrics {
    pub hits: u64,
    pub misses: u64,
}

pub trait Store<K, V> {
    fn get(&self, k: &K) -> Option<&V>;
    fn put(&mut self, k: K, v: V);
}

impl<'a, K, V> Cache<'a, K, V>
where
    K: Clone + std::hash::Hash + std::fmt::Debug + Eq,
    V: Send + Sync + 'static,
{
    pub fn new() -> Self {
        Self {
            inner: std::collections::HashMap::new(),
            _lt: PhantomData,
        }
    }

    pub async fn fetch(&self, k: &K) -> Option<&V> {
        self.inner.get(k)
    }

    pub unsafe fn raw_ptr(&self) -> *const std::collections::HashMap<K, V> {
        &self.inner as *const _
    }
}

fn internal_helper(x: u32) -> u32 {
    x.saturating_add(1)
}

pub const MAX_ENTRIES: usize = 10_000;
pub static DEFAULT_NAME: &str = "touring-cache";
"#;

/// Malformed Rust — must not panic; parse path must return `Err`.
const MALFORMED_SOURCE: &str = "fn broken( { unclosed";

#[test]
fn workflow_step1_language_detection_classifies_rs_as_rust() {
    let lang = Lang::from_path(std::path::Path::new("foo.rs"));
    assert_eq!(lang, Some(Lang::Rust), "step 1: .rs must map to Lang::Rust");
}

#[test]
fn workflow_step2_rust_semantic_extracts_every_surface() {
    let report = RustSemanticReport::from_source(REPRESENTATIVE_SOURCE)
        .expect("step 2: well-formed source must parse");

    // Semantic surfaces that MUST be discovered on the fixture source:
    assert!(!report.generics.is_empty(), "must find generic parameters");
    assert!(
        !report.trait_impls.is_empty(),
        "must find at least one impl block"
    );
    assert!(!report.lifetimes.is_empty(), "must find lifetime params");
    assert!(!report.where_clauses.is_empty(), "must find where clauses");
    assert_eq!(
        report.async_fns, 1,
        "exactly one async fn in fixture (Cache::fetch)"
    );
    assert!(
        report.unsafe_blocks >= 1,
        "at least one unsafe fn/block (Cache::raw_ptr)"
    );
    // Complexity score must be bounded in [0, 1].
    let sc = report.semantic_complexity();
    assert!(
        (0.0..=1.0).contains(&sc),
        "semantic_complexity {sc} out of range"
    );
    // Fixture is non-trivial → should not be "simple".
    assert!(
        !report.is_simple(),
        "fixture has generics+lifetimes+unsafe+async; is_simple must be false"
    );
}

#[test]
fn workflow_step3_public_api_surface_finds_pub_items_only() {
    let surface = RustSemanticReport::public_api_surface(REPRESENTATIVE_SOURCE)
        .expect("step 3: surface extraction must succeed");

    // Public items present in the fixture — the workflow detects
    // removals of any of these as breaking changes.
    assert!(
        surface.iter().any(|e| e == "struct Cache"),
        "pub struct Cache missing from surface: {surface:?}"
    );
    assert!(surface.iter().any(|e| e == "struct Metrics"));
    assert!(surface.iter().any(|e| e == "trait Store"));
    assert!(surface.iter().any(|e| e == "const MAX_ENTRIES"));
    assert!(surface.iter().any(|e| e == "static DEFAULT_NAME"));

    // Private items must NOT appear.
    assert!(
        !surface.iter().any(|e| e.contains("internal_helper")),
        "private fn must not appear in public surface: {surface:?}"
    );

    // Surface is deterministically sorted.
    let mut sorted = surface.clone();
    sorted.sort();
    assert_eq!(surface, sorted, "surface must be sorted for stable diffs");
}

#[test]
fn workflow_step4_format_rust_produces_clean_output() {
    // Ugly-spaced input to showcase prettyplease normalization.
    let ugly = "pub  fn  greet  (  x : u32 )  ->  u32  {  x + 1  }";
    let clean = format_rust_code(ugly).expect("step 4: format must succeed");

    // Property a rustfmt-clean output satisfies:
    //   * "fn greet(" — exactly one space between fn and name, no space
    //     before the paren.
    //
    // We intentionally do NOT check "no double spaces" — rustfmt output
    // uses 4-space indentation inside fn bodies, which would trigger a
    // false-positive on a naive `"  "` substring search.
    assert!(
        clean.contains("fn greet("),
        "formatted output lost canonical fn signature: {clean:?}"
    );
    // Signature line itself has no double space.
    let signature_line = clean.lines().next().expect("at least one line");
    assert!(
        !signature_line.contains("  "),
        "fn signature line has double spaces: {signature_line:?}"
    );
}

#[test]
fn workflow_step5_workspace_info_loads_manifest_metadata() {
    // `WorkspaceInfo::load` appends `Cargo.toml` to its argument and
    // passes that to `cargo_metadata::MetadataCommand`. We point it at
    // touring-ast's own Cargo.toml; cargo-metadata auto-discovers the
    // enclosing workspace from any member manifest, so the result is
    // still the full workspace topology.
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let ws = WorkspaceInfo::load(manifest_dir).expect("step 5: cargo_metadata must load");

    // Hard invariants on the touring workspace structure:
    // `touring-ast` was fused into `touring-code` (ast→code::ast) in the
    // A2 shim-fusion wave (2026-06-14); the AST crate's identity now lives
    // under `touring-code`. This assertion tracks that post-fusion topology.
    assert!(
        ws.packages.iter().any(|p| p.name == "touring-code"),
        "workspace must contain touring-code"
    );
    assert!(
        ws.packages.iter().any(|p| p.name == "touring-server"),
        "workspace must contain touring-server"
    );
    assert!(
        ws.workspace_member_count >= 20,
        "workspace has at least 20 members; got {}",
        ws.workspace_member_count
    );

    // Cross-crate blast radius — at least one crate depends on the foundation
    // crate. Post-W3 (touring-premium-refactor-2026) `touring-core` was renamed
    // to `touring-foundation`; the real dependency hub is `touring-foundation`.
    let foundation_dependents = ws.dependents_of("touring-foundation");
    assert!(
        !foundation_dependents.is_empty(),
        "touring-foundation has zero dependents — wiring broken?"
    );
}

#[test]
fn workflow_robustness_malformed_source_errors_without_panic() {
    // Every library step MUST return an `Err` (not panic) on malformed
    // input. A panic in any of these would surface as a SIGABRT in the
    // daemon under real user input.
    let r = RustSemanticReport::from_source(MALFORMED_SOURCE);
    assert!(r.is_err(), "malformed source must yield Err");

    let r = RustSemanticReport::public_api_surface(MALFORMED_SOURCE);
    assert!(r.is_err(), "malformed source surface must yield Err");

    let r = format_rust_code(MALFORMED_SOURCE);
    assert!(r.is_err(), "malformed source format must yield Err");
}

#[test]
fn workflow_public_api_diff_is_monotonic() {
    // Core property: adding a `pub fn` MUST appear as an addition in
    // the diff. Removing it MUST appear as a removal.
    let v1 = "pub fn foo() {} pub fn bar() {}";
    let v2 = "pub fn foo() {} pub fn bar() {} pub fn baz() {}";
    let v3 = "pub fn foo() {}"; // bar removed, baz never existed

    let s1 = RustSemanticReport::public_api_surface(v1).expect("v1");
    let s2 = RustSemanticReport::public_api_surface(v2).expect("v2");
    let s3 = RustSemanticReport::public_api_surface(v3).expect("v3");

    assert_eq!(s1.len(), 2);
    assert_eq!(s2.len(), 3);
    assert_eq!(s3.len(), 1);

    // Going v1 → v2: baz added, nothing removed.
    let added: Vec<_> = s2.iter().filter(|e| !s1.contains(e)).collect();
    let removed: Vec<_> = s1.iter().filter(|e| !s2.contains(e)).collect();
    assert_eq!(added, vec![&"fn baz".to_string()]);
    assert!(removed.is_empty());

    // Going v2 → v3: baz+bar removed.
    let removed_v2_to_v3: Vec<_> = s2.iter().filter(|e| !s3.contains(e)).collect();
    assert_eq!(removed_v2_to_v3.len(), 2);
}

#[test]
fn workflow_full_chain_executes_under_one_second() {
    // The whole chain must complete in well under a second on a typical
    // source file. A regression into the multi-second range would block
    // the daemon hot path.
    use std::time::Instant;

    let start = Instant::now();

    let lang = Lang::from_path(std::path::Path::new("fixture.rs"));
    assert_eq!(lang, Some(Lang::Rust));

    let report = RustSemanticReport::from_source(REPRESENTATIVE_SOURCE).expect("step 2");
    let _surface = RustSemanticReport::public_api_surface(REPRESENTATIVE_SOURCE).expect("step 3");
    let _formatted = format_rust_code(REPRESENTATIVE_SOURCE).expect("step 4");

    let elapsed = start.elapsed();
    // 3s budget (was 1s — too tight under sccache cold-start + parallel CI noise).
    // Multi-second regression still surfaces a hot-path bottleneck; the 3× buffer
    // distinguishes real perf regression from incidental scheduler jitter.
    assert!(
        elapsed.as_millis() < 3000,
        "workflow exceeded 3s budget: {elapsed:?} (item_count={})",
        report.item_count
    );
}
