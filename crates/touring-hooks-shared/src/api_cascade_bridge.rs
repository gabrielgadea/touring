//! Wave C2-wiring (2026-04-20) — bridge between Rust file edits and the
//! `touring-ast::api_cascade` planner.
//!
//! This module is the **library-level consumer** of
//! [`plan_api_cascade`]. It owns
//! a per-path cache of the last-known public API surface so that two
//! consecutive edits on the same file can be diffed without re-reading the
//! file from disk. The function [`analyze_rust_edit`] is the single entry
//! point: hooks (post_edit, post_write, …) call it with the new source and
//! receive an `Option<CascadePlan>` describing which callers need
//! follow-up.
//!
//! # Why a dedicated module
//!
//! `post_edit.rs` is already large (>2800 LOC). Rather than thread the
//! cascade logic through its existing signal pipeline, we isolate it here:
//!
//! - the cache stays owned by a single struct, not sprayed across context
//!   runtime fields;
//! - tests can exercise the full bridge without booting a `HookRuntime`;
//! - wiring into `post_edit` becomes a one-line call site.
//!
//! # Thread-safety
//!
//! `ApiSurfaceCache` wraps a `HashMap` in a `Mutex` so it can live behind a
//! shared reference. Contention is negligible in practice — the cache is
//! touched once per edit — and a `Mutex` keeps the surface `Send + Sync`
//! without adding a `DashMap` dependency.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use touring_code::ast::api_cascade::{CascadePlan, Severity, plan_api_cascade};
use touring_code::ast::call_graph::{CallGraph, build_call_graph};
use touring_code::ast::languages::Lang;
use touring_code::ast::rust_semantic::RustSemanticReport;

/// Per-path cache of the most recently observed public API surface.
///
/// The cache is keyed by the edited file's absolute path so that the bridge
/// can diff edits across process lifetimes (though the cache itself lives
/// in memory — persistence is out of scope for this wave).
#[derive(Debug, Default)]
pub struct ApiSurfaceCache {
    inner: Mutex<HashMap<PathBuf, Vec<String>>>,
}

impl ApiSurfaceCache {
    /// Create an empty cache.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Read the cached API surface for `path`, if any.
    #[must_use]
    pub fn get(&self, path: &Path) -> Option<Vec<String>> {
        self.inner.lock().ok().and_then(|g| g.get(path).cloned())
    }

    /// Replace the cached API surface for `path`.
    pub fn set(&self, path: &Path, surface: Vec<String>) {
        if let Ok(mut g) = self.inner.lock() {
            g.insert(path.to_path_buf(), surface);
        }
    }

    /// Drop the entry for `path`. Useful when a file is deleted or renamed.
    pub fn invalidate(&self, path: &Path) {
        if let Ok(mut g) = self.inner.lock() {
            g.remove(path);
        }
    }

    /// Number of cached entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.inner.lock().map(|g| g.len()).unwrap_or(0)
    }

    /// `true` when the cache is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Outcome of a single analysis pass.
///
/// Distinguishes the three terminal states a bridge call can reach so
/// callers can branch on them without inspecting the plan shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnalysisOutcome {
    /// The file is not Rust — nothing was analyzed. Cache is untouched.
    NotRust,
    /// First observation of this path — cache has been seeded but no diff
    /// was produced (there is no "before" to compare against).
    FirstObservation,
    /// The prior surface was compared against the new one; the contained
    /// [`CascadePlan`] may be empty (no-op edit) or carry proposals.
    Diffed(CascadePlan),
    /// The `new_source` failed to parse. Cache is untouched to avoid
    /// poisoning future diffs with a partial surface.
    ParseFailed(String),
}

impl AnalysisOutcome {
    /// Extract the inner [`CascadePlan`], when the outcome is
    /// [`AnalysisOutcome::Diffed`].
    #[must_use]
    pub fn plan(&self) -> Option<&CascadePlan> {
        match self {
            Self::Diffed(plan) => Some(plan),
            _ => None,
        }
    }

    /// `true` when the outcome reflects a genuine comparison against prior
    /// state (i.e. not the first observation for this path).
    #[must_use]
    pub fn is_diff(&self) -> bool {
        matches!(self, Self::Diffed(_))
    }
}

/// Analyze a Rust-file edit and return the resulting cascade plan.
///
/// Behavior:
///
/// 1. When `path` does not have the `.rs` extension, returns
///    [`AnalysisOutcome::NotRust`] without touching the cache.
/// 2. When `new_source` fails to parse, returns
///    [`AnalysisOutcome::ParseFailed`] and preserves the cache entry.
/// 3. On the first observation of a path, seeds the cache and returns
///    [`AnalysisOutcome::FirstObservation`].
/// 4. On subsequent observations, diffs the new surface against the cached
///    prior, builds a call graph over `new_source` itself (so proposals
///    reference the new call sites), updates the cache with the new
///    surface, and returns [`AnalysisOutcome::Diffed`].
///
/// The call graph is intentionally restricted to `new_source`: we want to
/// know which *remaining* call sites will need updating, not the ones that
/// were already deleted along with the API they referenced.
#[must_use]
pub fn analyze_rust_edit(
    path: &Path,
    new_source: &str,
    cache: &ApiSurfaceCache,
) -> AnalysisOutcome {
    if !is_rust_path(path) {
        return AnalysisOutcome::NotRust;
    }

    let new_surface = match RustSemanticReport::public_api_surface(new_source) {
        Ok(surface) => surface,
        Err(e) => return AnalysisOutcome::ParseFailed(e.to_string()),
    };

    let Some(prior_surface) = cache.get(path) else {
        cache.set(path, new_surface);
        return AnalysisOutcome::FirstObservation;
    };

    let changes = touring_code::ast::rust_semantic::diff_api_surfaces(&prior_surface, &new_surface);
    let graph: CallGraph = build_call_graph(new_source, Lang::Rust);
    let plan = plan_api_cascade(&changes, &graph);
    cache.set(path, new_surface);
    AnalysisOutcome::Diffed(plan)
}

/// Emit a tracing summary for a cascade plan, if it contains actionable
/// proposals. Designed to be called from hooks that want a fire-and-forget
/// observability signal.
///
/// Emits `warn` when any proposal has [`Severity::High`]; `debug` otherwise.
/// A completely empty plan produces no log record.
pub fn log_cascade_plan(path: &Path, plan: &CascadePlan) {
    if plan.is_empty() {
        return;
    }
    let high = plan.count_by_severity(Severity::High);
    let medium = plan.count_by_severity(Severity::Medium);
    let low = plan.count_by_severity(Severity::Low);
    if high > 0 {
        tracing::warn!(
            path = %path.display(),
            high, medium, low,
            "api_cascade: high-severity API changes — callers may be broken"
        );
    } else {
        tracing::debug!(
            path = %path.display(),
            high, medium, low,
            "api_cascade: API surface changed"
        );
    }
}

/// `true` when `path` has the `.rs` extension (case-insensitive).
fn is_rust_path(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("rs"))
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    const RUST_PATH: &str = "/tmp/test_cascade/foo.rs";

    fn p() -> PathBuf {
        PathBuf::from(RUST_PATH)
    }

    #[test]
    fn cache_defaults_empty() {
        let c = ApiSurfaceCache::new();
        assert!(c.is_empty());
        assert_eq!(c.len(), 0);
        assert!(c.get(&p()).is_none());
    }

    #[test]
    fn cache_set_get_invalidate_roundtrip() {
        let c = ApiSurfaceCache::new();
        let surface = vec!["pub fn greet() -> String".to_string()];
        c.set(&p(), surface.clone());
        assert_eq!(c.len(), 1);
        assert_eq!(c.get(&p()), Some(surface));
        c.invalidate(&p());
        assert!(c.get(&p()).is_none());
        assert_eq!(c.len(), 0);
    }

    #[test]
    fn non_rust_path_short_circuits() {
        let c = ApiSurfaceCache::new();
        let outcome = analyze_rust_edit(Path::new("/tmp/test.py"), "def foo(): pass", &c);
        assert_eq!(outcome, AnalysisOutcome::NotRust);
        assert!(c.is_empty(), "cache must not be touched for non-rust files");
    }

    #[test]
    fn first_observation_seeds_cache_and_skips_diff() {
        let c = ApiSurfaceCache::new();
        let src = "pub fn greet(name: &str) -> String { String::new() }";
        let outcome = analyze_rust_edit(&p(), src, &c);
        assert_eq!(outcome, AnalysisOutcome::FirstObservation);
        assert!(outcome.plan().is_none());
        assert!(!c.is_empty(), "cache must hold the first observation");
    }

    #[test]
    fn second_observation_diffs_against_prior() {
        let c = ApiSurfaceCache::new();
        let before = "pub fn greet(name: &str) -> String { String::new() }\n\
                      pub fn farewell(name: &str) -> String { String::new() }";
        let after = "pub fn greet(name: &str) -> String { String::new() }";
        // Seed
        let _ = analyze_rust_edit(&p(), before, &c);
        // Diff
        let outcome = analyze_rust_edit(&p(), after, &c);
        let plan = outcome.plan().expect("should diff on second call");
        let removed = plan
            .proposals
            .iter()
            .find(|proposal| proposal.symbol == "farewell")
            .expect("farewell removal must surface");
        assert_eq!(
            removed.kind,
            touring_code::ast::rust_semantic::ApiChangeKind::Removed
        );
    }

    #[test]
    fn identical_source_produces_empty_plan() {
        let c = ApiSurfaceCache::new();
        let src = "pub fn stable() -> u32 { 0 }";
        let _ = analyze_rust_edit(&p(), src, &c);
        let outcome = analyze_rust_edit(&p(), src, &c);
        let plan = outcome.plan().expect("diff outcome");
        assert!(plan.is_empty(), "identical source must emit zero proposals");
    }

    #[test]
    fn parse_failure_preserves_cache() {
        let c = ApiSurfaceCache::new();
        let before = "pub fn ok() -> () { () }";
        let _ = analyze_rust_edit(&p(), before, &c);
        let cached_before = c.get(&p());

        let outcome = analyze_rust_edit(&p(), "this is not rust {{{", &c);
        matches!(outcome, AnalysisOutcome::ParseFailed(_))
            .then_some(())
            .expect("parse failure must be reported");

        assert_eq!(
            c.get(&p()),
            cached_before,
            "cache must survive a parse failure"
        );
    }

    #[test]
    fn call_graph_is_built_over_new_source_not_old() {
        // The call graph should reflect CURRENT call sites in the new code,
        // so proposals list who still needs to adapt.
        let c = ApiSurfaceCache::new();
        let before = "pub fn greet() -> () {}\n\
                      pub fn farewell() -> () { greet() }";
        let after = "pub fn farewell() -> () { /* greet removed */ }";

        let _ = analyze_rust_edit(&p(), before, &c);
        let outcome = analyze_rust_edit(&p(), after, &c);
        let plan = outcome.plan().expect("diff");
        // `greet` was removed; its old callers (farewell) should NOT appear
        // because farewell no longer references greet in the new source.
        if let Some(greet_removal) = plan.proposals.iter().find(|p| p.symbol == "greet") {
            assert!(
                greet_removal.callers.is_empty(),
                "callers built from NEW source must not include the removed reference"
            );
        }
    }

    #[test]
    fn is_rust_path_handles_case_insensitivity() {
        assert!(is_rust_path(Path::new("/tmp/foo.rs")));
        assert!(is_rust_path(Path::new("/tmp/foo.RS")));
        assert!(is_rust_path(Path::new("/tmp/foo.Rs")));
        assert!(!is_rust_path(Path::new("/tmp/foo.rsx")));
        assert!(!is_rust_path(Path::new("/tmp/foo")));
        assert!(!is_rust_path(Path::new("/tmp/rs.py")));
    }

    #[test]
    fn analysis_outcome_helpers_behave() {
        let plan = CascadePlan::default();
        let diffed = AnalysisOutcome::Diffed(plan.clone());
        assert!(diffed.is_diff());
        assert_eq!(diffed.plan(), Some(&plan));

        assert!(!AnalysisOutcome::FirstObservation.is_diff());
        assert!(AnalysisOutcome::FirstObservation.plan().is_none());

        assert!(!AnalysisOutcome::NotRust.is_diff());
        assert!(!AnalysisOutcome::ParseFailed("x".into()).is_diff());
    }

    #[test]
    fn log_cascade_plan_does_nothing_on_empty_plan() {
        // Just smoke-test: must not panic.
        log_cascade_plan(&p(), &CascadePlan::default());
    }

    #[test]
    fn cache_is_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<ApiSurfaceCache>();
    }
}
