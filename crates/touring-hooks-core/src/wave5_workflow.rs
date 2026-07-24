//! Wave 5 (2026-04-18) — public Rust workflow advisory helpers.
//!
//! # Purpose
//!
//! Extracts the V6 hint + reward logic from `post_edit.rs` into a
//! reusable public API. Callers include:
//!
//! - `post_edit::run_returning` — emits hint + injects reward after Edit.
//! - `touring-integration-tests` — end-to-end validation of the chain.
//! - Future `pre_edit`, `pre_write` consumers — advisory BEFORE the
//!   edit takes effect.
//!
//! # Invariants
//!
//! 1. Both functions accept `Option<&str>` for preloaded source — when
//!    `None`, the function reads from disk. This mirrors the pattern
//!    used by `verify_multiconfig_hint`.
//! 2. Neither function panics. Parse failure, non-Rust file, empty
//!    source, or trivial source → returns `None` (no signal).
//! 3. Reward is bounded in `[-0.10, +0.10]` so it modulates but does
//!    not dominate the `+1.0` phase-1 reward.
//!
//! # Why split this out now
//!
//! `post_edit.rs` has CC=66 in `run_returning` (pre-existing hotspot);
//! adding more logic inline would make the file harder to navigate.
//! Extracting cohesive Wave 5 logic into its own module isolates
//! future refinements (e.g. a `pre_edit` companion) behind a single
//! entry point and lets the integration test exercise the exact same
//! function `post_edit` calls — no drift.

use touring_analysis::quality::RustQualitySignals;
use touring_code::ast::{CodeGenWorkflow, Lang, analyze_quality, extract_symbols};

/// Threshold below which the positive reward is dampened.
///
/// Mirrors the Wave 7 `QualityGateAdapter::with_semantic_threshold`
/// "strict" recommendation (0.6) but slightly stricter here because the
/// RL reward modulator must give CC actionable feedback — below 0.75,
/// the semantic signals (unsafe density, trait-bound complexity,
/// lifetime abstraction) are already concerning even if tree-sitter
/// metrics look clean.
const REWARD_HEALTH_DAMPER: f32 = 0.75;

/// Classify a `complexity` score in `[0, 1]` into a stable text band.
/// Shared by Rust and non-Rust paths so the advisory wording is
/// consistent across languages.
fn complexity_band(complexity: f32) -> &'static str {
    match complexity {
        c if c < 0.15 => "simple",
        c if c < 0.35 => "moderate",
        c if c < 0.60 => "complex",
        _ => "very_complex",
    }
}

/// Emit a compact advisory line summarizing the semantic shape of a
/// Rust source file. Used by `post_edit::verify_post_edit_quality` as
/// the V6 check and by integration tests.
///
/// Returns `None` when:
/// - `file_path` does not end in `.rs`
/// - the source fails to parse (rustc will surface it)
/// - the source is whitespace-only
/// - the source has no public surface AND trivial complexity
///
/// Returns `Some(hint)` otherwise. Hint format:
/// ```text
/// ⚙ rust-workflow: pub_surface=N complexity=0.XX (band) unsafe=N async_fns=N
/// ```
#[must_use]
pub fn rust_workflow_hint(file_path: &str, preloaded: Option<&str>) -> Option<String> {
    if !file_path.ends_with(".rs") {
        return None;
    }
    let owned_source;
    let source: &str = match preloaded {
        Some(s) => s,
        None => {
            owned_source = std::fs::read_to_string(file_path).ok()?;
            &owned_source
        }
    };
    if source.trim().is_empty() {
        return None;
    }

    let report = CodeGenWorkflow::analyze_no_format(source).ok()?;

    if report.public_api.is_empty() && report.semantic_complexity < 0.05 {
        return None;
    }

    // Wave 8 symmetric fusion: health_score is the same metric the
    // Wave 7 generator gate (`QualityGateAdapter::with_semantic_threshold`)
    // evaluates. Surface it in the advisory so CC's edit-path verdict
    // and the generator's commit-path verdict align.
    let health = RustQualitySignals::from_report(&report.semantic).health_score();

    Some(format!(
        "⚙ rust-workflow: pub_surface={} complexity={:.2} ({}) unsafe={} async_fns={} health={:.2}",
        report.public_api.len(),
        report.semantic_complexity,
        report.complexity_band(),
        report.semantic.unsafe_blocks,
        report.semantic.async_fns,
        health,
    ))
}

/// Compute a bounded RL reward from the same workflow report.
///
/// Mapping:
///
/// | Condition                           | Reward |
/// |-------------------------------------|-------:|
/// | simple \| moderate, unsafe == 0     | +0.10  |
/// | complex band                        |  0.00  |
/// | very_complex OR unsafe > 0          | -0.10  |
/// | parse fail / non-Rust / trivial     |  None  |
///
/// The `[-0.10, +0.10]` envelope is intentionally ~10% of the `+1.0`
/// first-tier reward injected by `phase1_tracking`. V6 acts as a
/// modulator, not a replacement.
#[must_use]
pub fn rust_workflow_reward(file_path: &str, preloaded: Option<&str>) -> Option<f64> {
    if !file_path.ends_with(".rs") {
        return None;
    }
    let owned_source;
    let source: &str = match preloaded {
        Some(s) => s,
        None => {
            owned_source = std::fs::read_to_string(file_path).ok()?;
            &owned_source
        }
    };
    if source.trim().is_empty() {
        return None;
    }

    let report = CodeGenWorkflow::analyze_no_format(source).ok()?;

    if report.public_api.is_empty() && report.semantic_complexity < 0.05 {
        return None;
    }

    let band = report.complexity_band();
    let unsafe_penalty = report.semantic.unsafe_blocks > 0;
    let health = RustQualitySignals::from_report(&report.semantic).health_score();

    let base = match (band, unsafe_penalty) {
        ("very_complex", _) | (_, true) => -0.10,
        ("complex", false) => 0.00,
        ("simple" | "moderate", false) => 0.10,
        (_, false) => 0.00,
    };
    // Wave 8 symmetric damper: if syn-semantic health is below the
    // 0.75 threshold, halve the positive reward. Keeps envelope in
    // `[-0.10, +0.10]` and leaves the negative/zero paths untouched so
    // existing reward-bounded tests stay green.
    let reward = if base > 0.0 && health < REWARD_HEALTH_DAMPER {
        base / 2.0
    } else {
        base
    };
    Some(reward)
}

/// Aggregate helper: run both advisories in a single analyzer pass.
///
/// Internally parses the source ONCE via `CodeGenWorkflow::analyze_no_format`
/// (the expensive step) and derives both outputs. Prefer this when both
/// signals are needed — it halves the `syn::parse_file` cost.
///
/// Returns `(hint, reward)`. Either field can be `None` independently.
#[must_use]
pub fn rust_workflow_advisory(
    file_path: &str,
    preloaded: Option<&str>,
) -> (Option<String>, Option<f64>) {
    if !file_path.ends_with(".rs") {
        return (None, None);
    }
    let owned_source;
    let source: &str = match preloaded {
        Some(s) => s,
        None => match std::fs::read_to_string(file_path).ok() {
            Some(s) => {
                owned_source = s;
                &owned_source
            }
            None => return (None, None),
        },
    };
    if source.trim().is_empty() {
        return (None, None);
    }

    let Ok(report) = CodeGenWorkflow::analyze_no_format(source) else {
        return (None, None);
    };

    if report.public_api.is_empty() && report.semantic_complexity < 0.05 {
        return (None, None);
    }

    let health = RustQualitySignals::from_report(&report.semantic).health_score();

    let hint = format!(
        "⚙ rust-workflow: pub_surface={} complexity={:.2} ({}) unsafe={} async_fns={} health={:.2}",
        report.public_api.len(),
        report.semantic_complexity,
        report.complexity_band(),
        report.semantic.unsafe_blocks,
        report.semantic.async_fns,
        health,
    );

    let band = report.complexity_band();
    let unsafe_penalty = report.semantic.unsafe_blocks > 0;
    let base = match (band, unsafe_penalty) {
        ("very_complex", _) | (_, true) => -0.10,
        ("complex", false) => 0.00,
        ("simple" | "moderate", false) => 0.10,
        (_, false) => 0.00,
    };
    // Same Wave 8 damper as `rust_workflow_reward` — bundled helper
    // must stay byte-identical to the split helpers (invariant pinned
    // by `split_and_aggregate_produce_same_signals` test).
    let reward = if base > 0.0 && health < REWARD_HEALTH_DAMPER {
        base / 2.0
    } else {
        base
    };

    (Some(hint), Some(reward))
}

// ─── Wave 5.1 (2026-04-18) — Multi-language advisory ───────────────
//
// Generalizes the Rust-only advisory to every language `touring-ast`
// can parse: Python, TypeScript, TSX, JavaScript, Bash, HTML, CSS, etc.
//
// Rust keeps its richer path (syn-backed — generics/lifetimes/unsafe).
// Other languages fall through to tree-sitter `extract_symbols` +
// `analyze_quality` (both multi-lang) to surface pub surface count,
// max cyclomatic complexity, and antipattern count.

/// Multi-language workflow hint. Succeeds for every language
/// `touring-ast::Lang::from_path` can detect. Returns `None` for
/// unknown extensions, empty/missing files, or trivial content.
///
/// Rust uses the richer Wave 4/5 `CodeGenWorkflow` path (same as
/// [`rust_workflow_hint`]). All other languages use the shared
/// multi-lang path based on `extract_symbols` + `analyze_quality`.
///
/// Hint format (non-Rust):
/// ```text
/// ⚙ code-workflow [python]: pub_surface=3 complexity=0.22 (moderate) max_cc=9 antipatterns=0
/// ```
#[must_use]
pub fn code_workflow_hint(file_path: &str, preloaded: Option<&str>) -> Option<String> {
    let lang = Lang::from_path(std::path::Path::new(file_path))?;

    // Rust keeps its richer syn-backed advisory — format is already
    // `⚙ rust-workflow: ...` and downstream tests depend on it.
    if lang == Lang::Rust {
        return rust_workflow_hint(file_path, preloaded);
    }

    let owned_source;
    let source: &str = match preloaded {
        Some(s) => s,
        None => match std::fs::read_to_string(file_path).ok() {
            Some(s) => {
                owned_source = s;
                &owned_source
            }
            None => return None,
        },
    };
    if source.trim().is_empty() {
        return None;
    }

    let symbols = extract_symbols(source, lang).ok()?;
    let pub_count = symbols.iter().filter(|s| s.is_public).count();
    let total_symbols = symbols.len();
    let quality = analyze_quality(source, lang);

    // Trivial skip — NO symbols AND near-flat complexity. We use total
    // symbols (not pub-only) because non-Rust visibility detection is
    // language-specific (Python has no `export`; TS checks literal
    // "export " prefix in node_text which does not catch every pattern).
    if total_symbols == 0 && quality.max_complexity <= 1 {
        return None;
    }

    // `complexity_score` is 1.0 when perfectly clean; invert so the
    // band semantics match the Rust path ("simple" = low score).
    let complexity = 1.0 - quality.complexity_score;
    let band = complexity_band(complexity);

    // Dual-surface labelling: always show total symbols; also show
    // `exports=` when the language-specific detector found any.
    let surface_label = if pub_count > 0 {
        format!("symbols={total_symbols} exports={pub_count}")
    } else {
        format!("symbols={total_symbols}")
    };

    Some(format!(
        "⚙ code-workflow [{}]: {} complexity={:.2} ({}) max_cc={} antipatterns={}",
        lang.as_str(),
        surface_label,
        complexity,
        band,
        quality.max_complexity,
        quality.antipatterns.len(),
    ))
}

/// Multi-language reward derivation. Mirrors the Rust mapping but uses
/// the tree-sitter quality signals for non-Rust languages.
///
/// | Condition                                     | Reward |
/// |-----------------------------------------------|-------:|
/// | Rust                                          | delegates to [`rust_workflow_reward`] |
/// | simple/moderate band, no antipatterns         | +0.10  |
/// | complex band                                  |  0.00  |
/// | very_complex OR antipatterns > 0              | -0.10  |
/// | unknown lang / empty / trivial / parse fail   |  None  |
///
/// The `[-0.10, +0.10]` envelope is preserved across languages so the
/// RL engine sees a uniform signal scale.
#[must_use]
pub fn code_workflow_reward(file_path: &str, preloaded: Option<&str>) -> Option<f64> {
    let lang = Lang::from_path(std::path::Path::new(file_path))?;

    if lang == Lang::Rust {
        return rust_workflow_reward(file_path, preloaded);
    }

    let owned_source;
    let source: &str = match preloaded {
        Some(s) => s,
        None => match std::fs::read_to_string(file_path).ok() {
            Some(s) => {
                owned_source = s;
                &owned_source
            }
            None => return None,
        },
    };
    if source.trim().is_empty() {
        return None;
    }

    let symbols = extract_symbols(source, lang).ok()?;
    let quality = analyze_quality(source, lang);

    // Trivial skip — match the hint-path logic. Parallel definition
    // to keep both call sites aligned; any drift would mean hint and
    // reward disagree on when to skip.
    if symbols.is_empty() && quality.max_complexity <= 1 {
        return None;
    }

    let complexity = 1.0 - quality.complexity_score;
    let band = complexity_band(complexity);
    let has_antipatterns = !quality.antipatterns.is_empty();

    let reward = match (band, has_antipatterns) {
        ("very_complex", _) | (_, true) => -0.10,
        ("complex", false) => 0.00,
        ("simple" | "moderate", false) => 0.10,
        (_, false) => 0.00,
    };
    Some(reward)
}

/// Aggregate variant for multi-language — returns `(hint, reward)` in
/// a single call, mirroring [`rust_workflow_advisory`] for the Rust path.
#[must_use]
pub fn code_workflow_advisory(
    file_path: &str,
    preloaded: Option<&str>,
) -> (Option<String>, Option<f64>) {
    let hint = code_workflow_hint(file_path, preloaded);
    let reward = code_workflow_reward(file_path, preloaded);
    (hint, reward)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hint_and_reward_agree_on_clean_source() {
        let src = "pub fn hi() -> u32 { 1 }";
        let (hint, reward) = rust_workflow_advisory("src/lib.rs", Some(src));
        assert!(hint.is_some());
        assert_eq!(reward, Some(0.10));
    }

    #[test]
    fn hint_and_reward_agree_on_unsafe_source() {
        let src = "pub unsafe fn raw() -> *const u8 { std::ptr::null() }";
        let (hint, reward) = rust_workflow_advisory("src/lib.rs", Some(src));
        assert!(hint.is_some());
        assert_eq!(reward, Some(-0.10));
    }

    #[test]
    fn aggregate_skips_non_rust() {
        let (hint, reward) = rust_workflow_advisory("src/lib.py", Some("print('hi')"));
        assert_eq!(hint, None);
        assert_eq!(reward, None);
    }

    #[test]
    fn split_and_aggregate_produce_same_signals() {
        // Cross-check: the two single-purpose fns must agree with the
        // aggregate helper on every fixture. If they drift, callers
        // using split fns and callers using the aggregate will see
        // inconsistent V6 outputs.
        let fixtures = [
            "pub fn clean() {}",
            "pub unsafe fn raw() {}",
            "fn _private_only() {}", // triggers trivial skip
            "pub async fn fetch() -> u32 { 0 }",
        ];
        for src in fixtures {
            let path = "src/fixture.rs";
            let h1 = rust_workflow_hint(path, Some(src));
            let r1 = rust_workflow_reward(path, Some(src));
            let (h2, r2) = rust_workflow_advisory(path, Some(src));
            assert_eq!(h1, h2, "hint divergence for {src:?}");
            assert_eq!(r1, r2, "reward divergence for {src:?}");
        }
    }

    #[test]
    fn reward_is_bounded() {
        let fixtures = [
            "pub fn simple() {}",
            "pub unsafe fn raw() {}",
            "pub async fn a() {}",
            r#"pub fn big<T: Clone, U: Send + 'static>(_: T, _: U) -> u32 where T: std::fmt::Debug { 0 }"#,
        ];
        for src in fixtures {
            if let Some(r) = rust_workflow_reward("src/x.rs", Some(src)) {
                assert!(
                    (-0.10..=0.10).contains(&r),
                    "reward {r} out of bounds for {src:?}"
                );
            }
        }
    }

    // ── Wave 5.1 multi-language tests (2026-04-18) ───────────────

    #[test]
    fn code_workflow_hint_handles_python_with_public_api() {
        // Python source with a meaningful function surface — the
        // non-Rust path must emit a `code-workflow` hint.
        let src = "\
def compute(x, y):\n\
    return x + y\n\
\n\
def helper(n):\n\
    if n > 0:\n\
        return n * 2\n\
    elif n < 0:\n\
        return -n\n\
    else:\n\
        return 0\n";
        let hint = code_workflow_hint("src/compute.py", Some(src));
        assert!(
            hint.as_deref()
                .map(|h| h.starts_with("⚙ code-workflow ["))
                .unwrap_or(false),
            "python source must produce a code-workflow hint; got: {hint:?}"
        );
        let h = hint.expect("hint present");
        assert!(h.contains("[python]"), "language tag missing: {h:?}");
        assert!(h.contains("max_cc="));
        assert!(h.contains("antipatterns="));
    }

    #[test]
    fn code_workflow_hint_handles_typescript() {
        let src = "\
export function greet(name: string): string {\n\
    return `hello ${name}`;\n\
}\n\
\n\
export class Counter {\n\
    private n: number = 0;\n\
    public increment(): void { this.n += 1; }\n\
    public value(): number { return this.n; }\n\
}\n";
        let hint = code_workflow_hint("src/greet.ts", Some(src));
        assert!(
            hint.as_deref()
                .map(|h| h.contains("[typescript]"))
                .unwrap_or(false),
            "TypeScript must emit advisory with [typescript] tag; got: {hint:?}"
        );
    }

    #[test]
    fn code_workflow_hint_handles_tsx() {
        // .tsx is routed to Lang::TypeScript (tree-sitter-typescript
        // uses LANGUAGE_TSX for both). The helper must recognise it.
        let src = "\
export function Button(props: { label: string }) {\n\
    return <button>{props.label}</button>;\n\
}\n";
        let hint = code_workflow_hint("src/Button.tsx", Some(src));
        assert!(
            hint.is_some(),
            ".tsx must be recognised by Lang::from_path; got None"
        );
    }

    #[test]
    fn code_workflow_hint_handles_javascript() {
        let src = "\
export function sum(a, b) { return a + b; }\n\
export const VERSION = \"1.0.0\";\n";
        let hint = code_workflow_hint("src/util.js", Some(src));
        assert!(
            hint.as_deref()
                .map(|h| h.contains("[javascript]"))
                .unwrap_or(false),
            "JavaScript must emit [javascript] tag; got: {hint:?}"
        );
    }

    #[test]
    fn code_workflow_hint_none_for_unknown_extension() {
        // Lang::from_path returns None for `.xyz` → helper skips.
        assert!(code_workflow_hint("file.xyz", Some("whatever")).is_none());
    }

    #[test]
    fn code_workflow_reward_bounded_for_python() {
        let src = "def a():\n    return 1\n\ndef b():\n    return 2\n";
        if let Some(r) = code_workflow_reward("src/mod.py", Some(src)) {
            assert!(
                (-0.10..=0.10).contains(&r),
                "python reward {r} out of envelope"
            );
        }
    }

    #[test]
    fn code_workflow_reward_bounded_for_typescript() {
        let src = "export function hi(): void {}\n";
        if let Some(r) = code_workflow_reward("src/hi.ts", Some(src)) {
            assert!(
                (-0.10..=0.10).contains(&r),
                "typescript reward {r} out of envelope"
            );
        }
    }

    #[test]
    fn code_workflow_rust_still_uses_rust_format() {
        // Regression: Rust path must still produce the `rust-workflow`
        // (not `code-workflow`) prefix so existing consumers that pattern-match on
        // the tag continue to work.
        let src = "pub fn a() -> u32 { 1 }";
        let hint =
            code_workflow_hint("src/lib.rs", Some(src)).expect("rust source must produce hint");
        assert!(
            hint.starts_with("⚙ rust-workflow:"),
            "rust path must preserve legacy tag; got: {hint:?}"
        );
    }

    #[test]
    fn code_workflow_advisory_matches_split_apis_for_non_rust() {
        let src = "def compute(x): return x * 2\n";
        let h_split = code_workflow_hint("x.py", Some(src));
        let r_split = code_workflow_reward("x.py", Some(src));
        let (h_agg, r_agg) = code_workflow_advisory("x.py", Some(src));
        assert_eq!(h_split, h_agg, "hint drift on python");
        assert_eq!(r_split, r_agg, "reward drift on python");
    }

    // ── Wave 8 symmetric semantic fusion (2026-04-18) ────────────────

    #[test]
    fn hint_includes_health_score_field() {
        // Health field is the Wave 8 addition — every Rust hint must
        // include it so CC sees the same dual-engine verdict the
        // generator QualityGateAdapter uses.
        let src = "pub fn add(a: i32, b: i32) -> i32 { a + b }";
        let hint =
            rust_workflow_hint("src/lib.rs", Some(src)).expect("clean rust must produce hint");
        assert!(
            hint.contains("health="),
            "Wave 8 hint must surface health=; got: {hint:?}"
        );
    }

    #[test]
    fn hint_health_is_high_for_trivial_safe_rust() {
        let src = "pub fn ok() -> i32 { 42 }";
        let hint = rust_workflow_hint("src/lib.rs", Some(src)).expect("hint present");
        // Extract the "health=X.XX" substring and parse the value.
        let marker = "health=";
        let idx = hint.find(marker).expect("health field present");
        let start = idx + marker.len();
        let chunk = &hint[start..];
        let end = chunk
            .find(|c: char| !c.is_ascii_digit() && c != '.')
            .unwrap_or(chunk.len());
        let value: f64 = chunk[..end].parse().expect("health is numeric");
        assert!(
            value >= 0.9,
            "trivial safe rust must score >= 0.9, got {value} from hint: {hint:?}"
        );
    }

    #[test]
    fn hint_health_drops_for_unsafe_rust() {
        let src = "pub unsafe fn raw() -> *const u8 { std::ptr::null() }";
        let hint =
            rust_workflow_hint("src/lib.rs", Some(src)).expect("hint present for unsafe rust");
        let marker = "health=";
        let idx = hint.find(marker).expect("health field present");
        let start = idx + marker.len();
        let chunk = &hint[start..];
        let end = chunk
            .find(|c: char| !c.is_ascii_digit() && c != '.')
            .unwrap_or(chunk.len());
        let value: f64 = chunk[..end].parse().expect("health numeric");
        assert!(
            value < 1.0,
            "unsafe rust must score < 1.0, got {value} from hint: {hint:?}"
        );
    }

    #[test]
    fn reward_damper_preserves_envelope_on_all_paths() {
        // Exhaustive fixtures covering each reward branch — damper must
        // keep [-0.10, +0.10] invariant pinned by `reward_is_bounded`.
        let fixtures = [
            "pub fn simple() {}",                                  // +0.10 (clean)
            "pub unsafe fn raw() {}",                              // -0.10 (unsafe)
            "pub async fn f() {}",                                 // +0.10 (clean async)
            "pub fn g<T: Send + Sync + Clone + 'static>(_: T) {}", // higher complexity
            r#"pub fn h<A, B, C, D>(_: A, _: B, _: C, _: D)
               where A: Send + Sync + Clone + 'static,
                     B: Default + Copy + PartialEq + Eq + std::hash::Hash,
                     C: Iterator<Item = A> + ExactSizeIterator + DoubleEndedIterator,
                     D: From<A> + Into<B> {}"#, // very_complex
        ];
        for src in fixtures {
            if let Some(r) = rust_workflow_reward("src/f.rs", Some(src)) {
                assert!(
                    (-0.10..=0.10).contains(&r),
                    "damper broke envelope: reward {r} for fixture {src:?}",
                );
            }
        }
    }

    #[test]
    fn damper_does_not_amplify_negative_reward() {
        // Unsafe produces -0.10 which is INDEPENDENT of health damper.
        // The damper only divides POSITIVE rewards — if it touched
        // negatives, unsafe source would return -0.05 (incorrect).
        let src = "pub unsafe fn raw() -> *const u8 { std::ptr::null() }";
        let reward = rust_workflow_reward("src/lib.rs", Some(src));
        assert_eq!(
            reward,
            Some(-0.10),
            "damper must not touch negative reward; got {reward:?}",
        );
    }

    #[test]
    fn aggregate_surfaces_health_field_in_hint() {
        // Aggregate helper (`rust_workflow_advisory`) must include
        // the same health= field as the split helper. Pinned by the
        // existing `split_and_aggregate_produce_same_signals` test too.
        let src = "pub async fn fetch() -> u32 { 0 }";
        let (hint, _reward) = rust_workflow_advisory("src/lib.rs", Some(src));
        let h = hint.expect("hint present");
        assert!(h.contains("health="), "aggregate must emit health=: {h:?}");
    }

    #[test]
    fn code_workflow_hint_rust_includes_health() {
        // Rust path dispatches through `code_workflow_hint` → goes to
        // `rust_workflow_hint`. Health field must propagate.
        let src = "pub fn inc(x: i32) -> i32 { x + 1 }";
        let hint =
            code_workflow_hint("src/lib.rs", Some(src)).expect("rust hint via multi-lang entry");
        assert!(
            hint.starts_with("⚙ rust-workflow:"),
            "rust tag preserved; got: {hint:?}"
        );
        assert!(
            hint.contains("health="),
            "rust health propagates through multi-lang entry: {hint:?}"
        );
    }
}
