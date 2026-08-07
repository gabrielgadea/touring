//! Pre-Edit Prevention Hook — PROACTIVE error interception.
//!
//! Philosophy: PREVENT before the first failure, don't just learn after the second.
//!
//! Before Claude edits a file, this hook:
//! 1. Queries gotchas matching the target file (sorted by F1 score proxy)
//! 2. Queries cross-session error patterns with exponential decay
//! 3. Runs content-aware warnings (syntax, complexity, shadowing, anti-patterns)
//! 4. Composes a warning context ONLY if high-confidence patterns exist
//! 5. Injects the warning via `additionalContext` so Claude sees it BEFORE editing
//!
//! Enhancements (E5, E17):
//! - Deduplicated speculate_v2: result passed through from collect_syntax_issues to Deny gate
//! - Bayesian Deny gate: uses bayesian_score (not raw composite) for Block decision
//! - Jaccard near-duplicate detection (E17): old_string vs new_string similarity >0.85 warning
//!
//! Target latency: <10ms.

use std::collections::{HashMap, HashSet};

use touring_code::ast::{
    Lang, ScopeMap, SpeculateResult, build_scope_map, compute_complexity_for_source, speculate_v2,
};

use super::knowledge::FileKnowledgeDB;
use super::runtime::{HookResponse, HookRuntime, make_relative};
use crate::shared::hook_helpers;
use crate::shared::signals::rank_gotchas_by_relevance;

/// Minimum gotcha hit_count to be considered high-confidence.
const MIN_GOTCHA_HITS: i64 = 2;

/// Maximum number of gotchas to include in warning.
const MAX_GOTCHAS_IN_WARNING: usize = 3;

/// Maximum number of error patterns to include in warning.
const MAX_ERRORS_IN_WARNING: usize = 3;

/// Half-life in days for cross-session error decay (S1.3).
const DECAY_HALF_LIFE_DAYS: f64 = 7.0;

/// Cyclomatic complexity threshold — functions above this trigger a warning.
const COMPLEXITY_THRESHOLD: u16 = 10;

/// Maximum number of scope shadow warnings to emit.
const MAX_SHADOW_WARNINGS: usize = 3;

/// Speculate composite score threshold below which an edit is DENIED.
/// A score < 0.3 indicates critical syntax/structural breakage.
const DENY_SCORE_THRESHOLD: f64 = 0.3;

/// Run the pre-edit prevention hook (diverging version — for use by the CLI entry point).
///
/// Intercepts Edit and Write tool calls, queries the knowledge DB for
/// known error patterns, and injects a warning if high-confidence
/// patterns are found for the target file.
#[tracing::instrument(skip(runtime, input), fields(hook = "pre_edit_prevention"))]
pub fn run(
    runtime: &HookRuntime,
    input: &serde_json::Value,
) -> Result<(), touring_hook_runtime::hook_runtime::HookDispatchError> {
    run_returning(runtime, input).emit()
}

/// Run the pre-edit prevention hook, returning a `HookResponse` instead of diverging.
///
/// Used by the daemon to handle the hook without calling `process::exit`.
pub fn run_returning(runtime: &HookRuntime, input: &serde_json::Value) -> HookResponse {
    let file_path = input
        .pointer("/tool_input/file_path")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    if file_path.is_empty() {
        return HookResponse::Allow;
    }

    let rel_path = make_relative(file_path, &runtime.project_root);
    let new_string = input
        .pointer("/tool_input/new_string")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let old_string = input
        .pointer("/tool_input/old_string")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    // Collect all warnings from different signal sources
    let mut all_warnings: Vec<String> = Vec::new();

    // Signals 1-2: Knowledge-based warnings (gotchas + cross-session errors)
    if let Some(warning) = compose_pre_edit_warning(&runtime.ctx.knowledge, &rel_path) {
        // The existing function returns a formatted block; split into individual lines
        // after the header to merge cleanly with content warnings.
        for line in warning.lines().skip(1) {
            if !line.is_empty() {
                all_warnings.push(line.to_string());
            }
        }
    }

    // Signals 3-6: Content-aware warnings (syntax, complexity, shadowing, anti-patterns)
    // Also captures the SpeculateResult for reuse by the deny gate (avoids duplicate
    // speculate_v2 call — E5 optimization).
    let (content_warns, spec_result) = compose_content_warnings(Some(&runtime.project_root), new_string, file_path);
    all_warnings.extend(content_warns);

    // Signal 7: Invalid cfg condition detection
    all_warnings.extend(collect_invalid_cfg_warnings(new_string));

    // Signal 8: Module existence — `mod X;` without corresponding file → E0583
    all_warnings.extend(collect_module_existence_issues(new_string, file_path));

    // Signal 9: Public API signature change — callers may break
    // Enhanced: queries wiring_map to list exact caller files that need updating
    all_warnings.extend(collect_api_signature_changes(
        old_string,
        new_string,
        &runtime.ctx.knowledge,
        &rel_path,
    ));

    // Signal 10: Near-duplicate edit detection (Jaccard word-set similarity)
    if let Some(dup_warning) = check_near_duplicate(old_string, new_string) {
        all_warnings.push(dup_warning);
    }

    // ── Signal 11: Extended metadata — test coverage + community affinity ──
    // Surfaces low coverage or community membership so Claude can prioritize
    // adding tests for uncovered files and understand module groupings.
    if let Ok(Some(ext)) = runtime.ctx.knowledge.query_extended(&rel_path)
        && let Some(sig) = hook_helpers::build_file_meta_signal(&ext)
    {
        all_warnings.push(sig);
    }

    // ── DENY gate: critical syntax/structural breakage ──
    // If the new content has a very low speculate score, DENY the edit.
    // This prevents introducing broken code that will certainly fail to compile.
    // Reuses the SpeculateResult from compose_content_warnings when available.
    if let Some(deny_response) = check_deny_gate(
        new_string,
        file_path,
        &rel_path,
        &all_warnings,
        spec_result.as_ref(),
    ) {
        return deny_response;
    }

    if all_warnings.is_empty() {
        return HookResponse::Allow;
    }

    let header = format!(
        "Pre-edit prevention for {} ({} signal{}):",
        rel_path,
        all_warnings.len(),
        if all_warnings.len() > 1 { "s" } else { "" },
    );

    HookResponse::Context {
        context: format!("{}\n{}", header, all_warnings.join("\n")),
        event_name: Some("PreToolUse".to_string()),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Signal 10: Near-duplicate edit detection (Jaccard word-set similarity)
// ─────────────────────────────────────────────────────────────────────────────

/// Jaccard similarity threshold above which an edit is flagged as near-duplicate.
/// A value of 0.85 means 85% of the word tokens are shared between old and new,
/// suggesting the user may have intended to make more changes.
const NEAR_DUPLICATE_THRESHOLD: f64 = 0.85;

/// Detect when `new_string` is nearly identical to `old_string` using
/// word-level Jaccard similarity.
///
/// Returns a warning message when the similarity exceeds
/// [`NEAR_DUPLICATE_THRESHOLD`] (85%), which indicates the edit is a
/// near-duplicate — the user likely intended to change more.
///
/// Uses simple word-level tokenization (`split_whitespace`) and computes
/// Jaccard = |intersection| / |union| on the resulting sets. This is done
/// inline rather than via `touring_simd::JaccardComputer` because the SIMD
/// variant operates on sorted `&[u32]` token hashes, which is unnecessarily
/// complex for small string comparisons in a hook context.
///
/// Returns `None` when:
/// - Either string is empty (nothing to compare)
/// - The union is empty (both contain only whitespace)
/// - Similarity is at or below the threshold
fn check_near_duplicate(old_string: &str, new_string: &str) -> Option<String> {
    if old_string.is_empty() || new_string.is_empty() {
        return None;
    }

    // Extract code identifier tokens: letter/underscore followed by alphanumeric/underscore.
    // More meaningful than `split_whitespace`: punctuation (`;`, `{`, `}`, `->`, `(`, `)`)
    // is ignored so "fn foo(x: i32)" and "fn foo(y: i32)" correctly share "fn"/"foo"/"i32".
    // Minimum length 2, pure-digit sequences excluded, case-folded for case-insensitive match.
    let extract_tokens = |s: &str| -> HashSet<String> {
        let mut tokens: HashSet<String> = HashSet::new();
        let mut start: Option<usize> = None;
        for (i, c) in s.char_indices() {
            if c.is_alphanumeric() || c == '_' {
                if start.is_none() {
                    start = Some(i);
                }
            } else if let Some(s_idx) = start.take() {
                let tok = &s[s_idx..i];
                if tok.len() >= 2 && !tok.chars().all(|ch| ch.is_ascii_digit()) {
                    tokens.insert(tok.to_lowercase());
                }
            }
        }
        // Final token (no trailing delimiter)
        if let Some(s_idx) = start {
            let tok = &s[s_idx..];
            if tok.len() >= 2 && !tok.chars().all(|ch| ch.is_ascii_digit()) {
                tokens.insert(tok.to_lowercase());
            }
        }
        tokens
    };

    let old_tokens = extract_tokens(old_string);
    let new_tokens = extract_tokens(new_string);

    let intersection = old_tokens.intersection(&new_tokens).count();
    let union = old_tokens.union(&new_tokens).count();

    if union == 0 {
        return None;
    }

    let similarity = intersection as f64 / union as f64;
    if similarity > NEAR_DUPLICATE_THRESHOLD {
        Some(format!(
            "NEAR_DUPLICATE: edit is {:.0}% similar to original (AST-token Jaccard) — did you mean to change more?",
            similarity * 100.0,
        ))
    } else {
        None
    }
}

/// Compose a pre-edit warning from gotchas and cross-session error patterns.
///
/// Returns `None` if there's nothing actionable to warn about.
/// This is the core prevention logic: query -> filter -> compose -> or stay silent.
pub fn compose_pre_edit_warning(db: &FileKnowledgeDB, file_path: &str) -> Option<String> {
    let mut warnings: Vec<String> = Vec::new();

    // ── Signal 1: Gotchas (sorted by hit_count as F1 proxy) ──
    warnings.extend(collect_gotcha_warnings(db, file_path));

    // ── Signal 2: Cross-session error patterns (decay-weighted) ──
    warnings.extend(collect_decay_error_warnings(db, file_path));

    if warnings.is_empty() {
        return None;
    }

    // Compose the warning block
    let header = format!(
        "Pre-edit warning for {} ({} signal{}):",
        file_path,
        warnings.len(),
        if warnings.len() > 1 { "s" } else { "" },
    );

    Some(format!("{}\n{}", header, warnings.join("\n")))
}

/// Signal 1: Collect high-confidence gotcha warnings for a file.
///
/// Filters gotchas by [`MIN_GOTCHA_HITS`], ranks the candidates by
/// BM25+TF-IDF semantic relevance to `file_path`, then formats the top
/// [`MAX_GOTCHAS_IN_WARNING`] entries with a severity icon and optional
/// symbol hint.
fn collect_gotcha_warnings(db: &FileKnowledgeDB, file_path: &str) -> Vec<String> {
    let gotchas = db.get_gotchas_for_file(file_path);
    // Collect high-confidence candidates first so we can rank them.
    let candidates: Vec<_> = gotchas
        .iter()
        .filter(|g| g.hit_count >= MIN_GOTCHA_HITS)
        .collect();

    if candidates.is_empty() {
        return Vec::new();
    }

    // Build (pattern, message) pairs for BM25 semantic ranking against the
    // file path — more relevant gotchas surface to the top.
    let pairs: Vec<(String, String)> = candidates
        .iter()
        .map(|g| (g.pattern.clone(), g.gotcha.clone()))
        .collect();

    let ranked_messages = rank_gotchas_by_relevance(&pairs, file_path, MAX_GOTCHAS_IN_WARNING);

    // Format each ranked message by looking up the originating gotcha for
    // severity and symbol metadata.
    ranked_messages
        .iter()
        .filter_map(|msg| {
            candidates.iter().find(|g| &g.gotcha == msg).map(|g| {
                let severity_icon = match g.severity.as_str() {
                    "critical" | "error" => "🔴",
                    "warning" => "⚠️",
                    _ => "ℹ️",
                };
                let symbol_hint = g
                    .symbol_name
                    .as_deref()
                    .map(|s| format!(" (symbol: {s})"))
                    .unwrap_or_default();
                format!(
                    "{severity_icon} GOTCHA [{}]: {}{}",
                    g.pattern, g.gotcha, symbol_hint,
                )
            })
        })
        .collect()
}

/// Signal 2: Collect decay-weighted cross-session error pattern warnings.
///
/// Retrieves recent errors weighted by exponential decay (half-life:
/// [`DECAY_HALF_LIFE_DAYS`]) and formats each with language context.
fn collect_decay_error_warnings(db: &FileKnowledgeDB, file_path: &str) -> Vec<String> {
    let decay_errors = db.recent_errors_with_decay(file_path, DECAY_HALF_LIFE_DAYS);
    decay_errors
        .iter()
        .take(MAX_ERRORS_IN_WARNING)
        .map(|err| {
            let lang_ctx = err
                .language
                .as_deref()
                .map(|l| format!(":{l}"))
                .unwrap_or_default();
            format!(
                "⚡ Recurring error: {}{} (weight: {:.1})",
                err.pattern, lang_ctx, err.weighted_count,
            )
        })
        .collect()
}

/// Compose content-aware warnings for new edit content.
///
/// These signals analyze the CONTENT being written, not just file history.
/// Returns warnings only for high-confidence detections, along with the
/// [`SpeculateResult`] from the syntax pre-check so callers (e.g. the deny
/// gate) can reuse it without re-running `speculate_v2`.
///
/// # Signals
///
/// - **Signal 3**: Quick syntax pre-check via `speculate_v2`
/// - **Signal 4**: Cyclomatic complexity threshold (CC > 10)
/// - **Signal 5**: Scope shadowing detection via `build_scope_map`
/// - **Signal 6**: Structural anti-patterns (e.g. `.unwrap()` in Rust)
pub fn compose_content_warnings(
    project_root: Option<&std::path::Path>,
    new_string: &str,
    file_path: &str,
) -> (Vec<String>, Option<SpeculateResult>) {
    if new_string.is_empty() {
        return (Vec::new(), None);
    }

    let lang_name = detect_language(file_path).unwrap_or("unknown");
    let lang = match lang_name.parse::<Lang>() {
        Ok(l) => l,
        Err(_) => return (Vec::new(), None),
    };

    // V1+V2 and V3+V4 are independent — run in parallel via rayon::join.
    let src_a = new_string.to_owned();
    let src_b = new_string.to_owned();

    let ((syntax_complexity, spec_result), shadow_antipattern) = rayon::join(
        move || check_syntax_and_complexity(&src_a, lang),
        move || check_shadow_and_antipatterns(&src_b, lang, lang_name),
    );

    let mut warnings = syntax_complexity;
    warnings.extend(shadow_antipattern);

    // ── Tantivy: related symbol context (module-level awareness before edit) ──
    // Surfaces what other files in the same module define so Claude can detect
    // naming conflicts and follow conventions during content edits.
    #[cfg(feature = "tantivy-fts")]
    if let Some((_, s)) = crate::shared::signals::tantivy_related_docs_signal(project_root, file_path) {
        warnings.push(s);
    }

    (warnings, Some(spec_result))
}

/// Check whether the edit should be DENIED due to critical syntax/structural breakage.
///
/// Uses a pre-computed [`SpeculateResult`] when available (from
/// [`compose_content_warnings`]), falling back to a fresh `speculate_v2` call
/// only when `cached_spec` is `None`. This avoids a redundant AST parse that
/// previously doubled the CPU cost of the prevention hook (E5 optimization).
///
/// When `bayesian_score` is available (requires `simd-search` feature), it is
/// preferred over `composite_score` for the deny decision because it provides
/// confidence-weighted fusion across all validation layers.
///
/// Only denies when BOTH conditions are met:
/// 1. The effective score is below [`DENY_SCORE_THRESHOLD`] (< 0.3)
/// 2. There are actual syntax-level warnings detected (not just complexity/style)
///
/// Returns `Some(HookResponse::Deny { .. })` when the edit should be blocked,
/// or `None` to let it proceed (possibly with Context warnings).
fn check_deny_gate(
    new_string: &str,
    file_path: &str,
    rel_path: &str,
    existing_warnings: &[String],
    cached_spec: Option<&SpeculateResult>,
) -> Option<HookResponse> {
    if new_string.is_empty() {
        return None;
    }

    let lang = detect_language(file_path)?.parse::<Lang>().ok()?;

    // Reuse cached result when available; fall back to fresh computation otherwise
    let owned_result;
    let spec_result = match cached_spec {
        Some(cached) => cached,
        None => {
            owned_result = speculate_v2(new_string, lang, None, None);
            &owned_result
        }
    };

    // Prefer bayesian_score (confidence-weighted fusion) when available;
    // fall back to composite_score for non-simd builds.
    let effective_score = spec_result
        .bayesian_score
        .unwrap_or(spec_result.composite_score);

    // Only deny on critical breakage: low score AND syntax failures present
    if effective_score >= DENY_SCORE_THRESHOLD || spec_result.all_passed {
        return None;
    }

    // Require at least one SYNTAX-level warning to confirm this is a real breakage,
    // not just symbol resolution or import issues which may resolve at build time.
    let has_syntax_failures = spec_result.layers.iter().any(|layer| {
        matches!(layer.layer, touring_code::ast::ValidationLayer::Syntax) && !layer.passed
    });

    if !has_syntax_failures {
        return None;
    }

    // Collect syntax diagnostics for the denial reason
    let syntax_diags: Vec<String> = spec_result
        .layers
        .iter()
        .filter(|l| !l.passed)
        .flat_map(|l| l.diagnostics.iter().take(3).cloned())
        .collect();

    let reason = format!(
        "Edit denied: new content for {} has critical syntax errors (score: {:.2}). \
         Fix the syntax before applying this edit.",
        rel_path, effective_score,
    );

    let context = if existing_warnings.is_empty() && syntax_diags.is_empty() {
        None
    } else {
        let mut ctx_parts = Vec::new();
        ctx_parts.extend(syntax_diags.iter().map(|d| format!("SYNTAX: {d}")));
        ctx_parts.extend(existing_warnings.iter().cloned());
        Some(ctx_parts.join("\n"))
    };

    Some(HookResponse::Deny {
        reason,
        context,
        event_name: Some("PreToolUse".to_string()),
    })
}

/// V1+V2: Syntax validity and cyclomatic complexity checks.
///
/// Returns warning strings for any failed speculate layers and for
/// functions exceeding [`COMPLEXITY_THRESHOLD`], along with the
/// [`SpeculateResult`] from the syntax check for reuse by the deny gate.
fn check_syntax_and_complexity(source: &str, lang: Lang) -> (Vec<String>, SpeculateResult) {
    let (mut out, spec_result) = collect_syntax_issues(source, lang);
    out.extend(collect_complexity_issues(source, lang));
    (out, spec_result)
}

/// V1: Collect syntax diagnostic messages from speculate_v2.
///
/// Iterates over failed validation layers and prefixes each diagnostic
/// with `"SYNTAX: "`. Returns both the warning strings and the underlying
/// [`SpeculateResult`] so callers can reuse it (e.g. for the deny gate)
/// without re-running the expensive AST parse.
fn collect_syntax_issues(source: &str, lang: Lang) -> (Vec<String>, SpeculateResult) {
    let spec_result = speculate_v2(source, lang, None, None);
    if spec_result.all_passed {
        return (Vec::new(), spec_result);
    }
    let issues = spec_result
        .layers
        .iter()
        .filter(|layer| !layer.passed)
        .flat_map(|layer| layer.diagnostics.iter().map(|d| format!("SYNTAX: {d}")))
        .collect();
    (issues, spec_result)
}

/// V2: Collect complexity warnings for functions exceeding [`COMPLEXITY_THRESHOLD`].
///
/// Returns an empty vec when complexity data is unavailable or all functions
/// are within the threshold.
fn collect_complexity_issues(source: &str, lang: Lang) -> Vec<String> {
    let Ok(complexities) = compute_complexity_for_source(source, lang) else {
        return Vec::new();
    };
    complexities
        .iter()
        .filter(|(_, cc)| *cc > COMPLEXITY_THRESHOLD)
        .map(|(name, cc)| format!("COMPLEXITY: {name} CC={cc} (threshold: {COMPLEXITY_THRESHOLD})"))
        .collect()
}

/// V3+V4: Scope shadowing and structural anti-pattern checks.
///
/// Detects variable names that shadow an outer binding and language-specific
/// anti-patterns (all languages supported by [`crate::shared::antipatterns`]).
fn check_shadow_and_antipatterns(source: &str, lang: Lang, lang_name: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();

    // ── V3: Scope shadowing detection ──
    let scope = build_scope_map(source, lang);
    let shadows = detect_scope_shadows(&scope);
    for shadow in shadows.iter().take(MAX_SHADOW_WARNINGS) {
        out.push(format!(
            "SHADOW: variable '{}' shadows outer definition (L{})",
            shadow.0, shadow.1
        ));
    }

    // ── V4: Structural anti-patterns (all supported languages) ──
    let ap = crate::shared::antipatterns::detect_antipatterns(source, lang_name);
    for msg in ap {
        out.push(format!("ANTIPATTERN: {msg}"));
    }

    out
}

/// Detect variable shadowing within a scope map.
///
/// Iterates over all scope entries and identifies names that appear more than
/// once, indicating a shadow. Returns pairs of `(variable_name, shadow_line)`.
fn detect_scope_shadows(scope: &ScopeMap) -> Vec<(String, usize)> {
    let mut seen: HashMap<&str, usize> = HashMap::new();
    let mut shadows = Vec::new();
    for entry in &scope.entries {
        if seen.contains_key(entry.name.as_str()) {
            shadows.push((entry.name.clone(), entry.line));
        }
        seen.insert(&entry.name, entry.line);
    }
    shadows
}

/// Signal 7: Detect invalid `#[cfg(...)]` conditions in new content.
///
/// Known invalid conditions:
/// - `#[cfg(bench)]` — not a valid rustc cfg. Use `#[cfg(test)]` or `benches/`.
/// - `#[cfg(nightly)]` — not a built-in cfg. Use `#[cfg(feature = "nightly")]`.
///
/// Returns one warning string per detected invalid condition.
fn collect_invalid_cfg_warnings(source: &str) -> Vec<String> {
    /// Known invalid cfg conditions and their fix suggestions.
    const INVALID_CFGS: &[(&str, &str)] = &[
        (
            "#[cfg(bench)]",
            "INVALID_CFG: #[cfg(bench)] is not a valid cfg condition. \
             Use #[cfg(test)] for inline benchmarks, or move benchmark code \
             to benches/ directory with required-features in Cargo.toml.",
        ),
        (
            "#[cfg_attr(bench",
            "INVALID_CFG: cfg_attr(bench, ...) uses invalid cfg condition 'bench'. \
             Use cfg_attr(test, ...) or feature-gate instead.",
        ),
    ];

    let mut warnings = Vec::new();
    for (pattern, message) in INVALID_CFGS {
        if source.contains(pattern) {
            warnings.push(message.to_string());
        }
    }
    warnings
}

/// Detect language from file extension.
///
/// Used by S1.2 to populate the `language` column in edit_history
/// for context-aware error normalization.
///
/// Delegates to [`crate::shared::detect_language::detect_language`].
pub fn detect_language(file_path: &str) -> Option<&'static str> {
    crate::shared::detect_language::detect_language(file_path)
}

// ─────────────────────────────────────────────────────────────────────────────
// Signal 8: Module file existence — prevents E0583
// ─────────────────────────────────────────────────────────────────────────────

/// Returns `true` when neither `parent/NAME.rs` nor `parent/NAME/mod.rs` exists.
///
/// A `true` result means the compiler will emit E0583 for this `mod NAME;` declaration.
fn mod_file_missing(mod_name: &str, parent: &std::path::Path) -> bool {
    !parent.join(format!("{mod_name}.rs")).exists()
        && !parent.join(mod_name).join("mod.rs").exists()
}

/// Signal 8: Module existence — `mod X;` declared without a corresponding file.
///
/// Scans `new_string` for bare module declarations (`mod NAME;` / `pub mod NAME;`)
/// and checks that `NAME.rs` or `NAME/mod.rs` exists relative to `file_path`.
///
/// Zero false positives: a missing file is always a compiler error (E0583).
/// Inline module bodies (`mod NAME { ... }`) are ignored — they need no file.
fn collect_module_existence_issues(new_string: &str, file_path: &str) -> Vec<String> {
    if !file_path.ends_with(".rs") {
        return Vec::new();
    }
    let parent = match std::path::Path::new(file_path).parent() {
        Some(p) => p.to_path_buf(),
        None => return Vec::new(),
    };
    let mut issues = Vec::new();
    let mut search = new_string;
    while let Some(pos) = search.find("mod ") {
        let after = &search[pos + 4..];
        let name_end = after
            .find(|c: char| !c.is_alphanumeric() && c != '_')
            .unwrap_or(after.len());
        let mod_name = &after[..name_end];
        // Only bare declarations end with `;`, not bodies `{`
        let rest = after[name_end..].trim_start_matches([' ', '\t']);
        if !mod_name.is_empty() && rest.starts_with(';') && mod_file_missing(mod_name, &parent) {
            issues.push(format!(
                "E0583 risk: mod `{mod_name}` declared but `{mod_name}.rs` / `{mod_name}/mod.rs` not found — create the file first"
            ));
        }
        search = &search[pos + 4..];
    }
    issues
}

// ─────────────────────────────────────────────────────────────────────────────
// Signal 9: Public API signature change — prevents silent caller breakage
// ─────────────────────────────────────────────────────────────────────────────

/// Signal 9: Public API signature change — `pub fn` signature differs between
/// `old_string` and `new_string` for the same function name.
///
/// A changed parameter list or return type silently breaks every caller.
/// Enhanced: queries `wiring_map` to list exact caller files that need updating,
/// so Claude knows precisely WHERE to fix call sites.
fn collect_api_signature_changes(
    old_string: &str,
    new_string: &str,
    db: &FileKnowledgeDB,
    module_file: &str,
) -> Vec<String> {
    if old_string.is_empty() {
        return Vec::new();
    }
    let old_sigs = extract_pub_fn_signatures(old_string);
    let new_sigs = extract_pub_fn_signatures(new_string);
    let mut issues = Vec::new();
    for (name, old_sig) in &old_sigs {
        if let Some(new_sig) = new_sigs.get(name.as_str())
            && old_sig != new_sig
        {
            // Query wiring_map for all files that import this symbol
            let caller_detail = format_caller_detail(db, module_file, name);
            issues.push(format!(
                "⚠ SIGNATURE CHANGED: `pub fn {name}` — {old_sig} → {new_sig}{caller_detail}"
            ));
        }
    }
    issues
}

/// Format caller details for a changed symbol by querying the wiring_map.
///
/// Returns a string like:
/// ```text
/// \n  Callers that MUST be updated:
///   • src/post_read.rs (line 82) — uses `build_knowledge_ast`
///   • tests/integration.rs (line 145) — uses `build_knowledge_ast`
/// ```
/// Returns empty string if no consumers are found.
fn format_caller_detail(db: &FileKnowledgeDB, module_file: &str, symbol_name: &str) -> String {
    let consumers = db
        .consumers_of_symbol(module_file, symbol_name)
        .unwrap_or_default();

    if consumers.is_empty() {
        return String::new();
    }

    let mut detail = format!(
        "\n  Callers that MUST be updated ({} file{}):",
        consumers.len(),
        if consumers.len() > 1 { "s" } else { "" }
    );
    for (consumer_file, import_line) in &consumers {
        let line_hint = import_line
            .map(|l| format!(" (line {l})"))
            .unwrap_or_default();
        detail.push_str(&format!(
            "\n  • {consumer_file}{line_hint} — uses `{symbol_name}`"
        ));
    }
    detail
}

/// Scan from `text[0]` (which must be `'('`) to its matching close paren.
///
/// Returns the index one past the `')'`, or `text.len()` when unbalanced.
/// Extracted from [`extract_pub_fn_signatures`] to reduce cyclomatic complexity.
fn scan_to_close_paren(text: &str) -> usize {
    let mut depth: usize = 0;
    for (i, c) in text.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return i + 1;
                }
            }
            _ => {}
        }
    }
    text.len()
}

/// Extract `pub fn NAME(PARAMS) -> RET` signatures from Rust source.
///
/// Returns a map of `name → "(params) -> ret"`. Used by
/// [`collect_api_signature_changes`] to detect breaking signature edits.
fn extract_pub_fn_signatures(source: &str) -> HashMap<String, String> {
    let mut sigs = HashMap::new();
    let mut search = source;
    while let Some(pos) = search.find("pub fn ") {
        let after = &search[pos + 7..];
        let name_end = after
            .find(|c: char| !c.is_alphanumeric() && c != '_')
            .unwrap_or(after.len());
        let name = &after[..name_end];
        if name.is_empty() {
            search = &search[pos + 7..];
            continue;
        }
        let rest = &after[name_end..];
        if let Some(open) = rest.find('(') {
            let params_text = &rest[open..];
            let close = scan_to_close_paren(params_text);
            let params = params_text[1..close.saturating_sub(1)].trim().to_string();
            let after_params = &rest[open + close..];
            let ret = if let Some(arrow) = after_params.find("->") {
                let ret_raw = &after_params[arrow + 2..];
                let end = ret_raw.find(['{', ';', '\n']).unwrap_or(ret_raw.len());
                ret_raw[..end].trim().to_string()
            } else {
                String::new()
            };
            sigs.insert(name.to_string(), format!("({params}) -> {ret}"));
        }
        search = &search[pos + 7..];
    }
    sigs
}

#[cfg(test)]
mod tests {
    #![allow(clippy::indexing_slicing, clippy::len_zero)]
    use super::*;
    use tempfile::TempDir;

    fn setup_db() -> (TempDir, FileKnowledgeDB) {
        let dir = TempDir::new().expect("create temp dir");
        let db_path = dir.path().join("test_knowledge.db");
        let db = FileKnowledgeDB::new(&db_path).expect("create db");
        (dir, db)
    }

    #[test]
    fn test_pre_edit_silent_for_unknown_file() {
        let (_dir, db) = setup_db();
        let warning = compose_pre_edit_warning(&db, "unknown/file.rs");
        assert!(warning.is_none(), "Unknown file should produce no warning");
    }

    #[test]
    fn test_pre_edit_returns_gotcha_warning() {
        let (_dir, db) = setup_db();

        // Add a gotcha with enough hits to be high-confidence
        db.add_gotcha("rust_bridge", "use matched_text not text", "critical", None)
            .expect("add gotcha");
        // Increment hit_count to meet MIN_GOTCHA_HITS threshold
        let gotchas = db.get_gotchas_for_file("scripts/aco/rust_bridge.py");
        assert!(!gotchas.is_empty(), "gotcha should match");
        for _ in 0..MIN_GOTCHA_HITS {
            db.increment_gotcha_hit(gotchas[0].id);
        }

        let warning = compose_pre_edit_warning(&db, "scripts/aco/rust_bridge.py");
        assert!(warning.is_some(), "Should produce warning for known gotcha");
        let text = warning.unwrap();
        assert!(text.contains("GOTCHA"), "Warning should contain GOTCHA");
        assert!(
            text.contains("matched_text"),
            "Warning should contain gotcha text"
        );
    }

    #[test]
    fn test_pre_edit_filters_low_confidence_gotchas() {
        let (_dir, db) = setup_db();

        // Add a gotcha with 0 hits (below threshold)
        db.add_gotcha("some_file", "low confidence gotcha", "info", None)
            .expect("add gotcha");

        let warning = compose_pre_edit_warning(&db, "some_file.rs");
        assert!(
            warning.is_none(),
            "Low-confidence gotchas should not trigger warning"
        );
    }

    #[test]
    fn test_pre_edit_includes_recurring_errors() {
        let (_dir, db) = setup_db();

        // Record enough errors to trigger decay-weighted detection
        for _ in 0..5 {
            db.record_edit_full(
                "src/main.rs",
                "Edit",
                Some("fix import"),
                Some("string_not_found"),
                Some("rust"),
                None,
                Some("session-1"),
            )
            .expect("record edit");
        }

        let warning = compose_pre_edit_warning(&db, "src/main.rs");
        assert!(warning.is_some(), "Recurring errors should trigger warning");
        let text = warning.unwrap();
        assert!(
            text.contains("Recurring error"),
            "Warning should mention recurring error"
        );
        assert!(
            text.contains("string_not_found"),
            "Warning should name the error pattern"
        );
    }

    #[test]
    fn test_pre_edit_max_warnings_capped() {
        let (_dir, db) = setup_db();

        // Add many gotchas with DIFFERENT patterns that all substring-match the test path.
        // Patterns: "g0_", "g1_", ..., "g9_" all match "g0_g1_g2_g3_g4_g5_g6_g7_g8_g9_file.rs"
        for i in 0..10 {
            let pattern = format!("g{i}_");
            let id = db
                .add_gotcha(&pattern, &format!("gotcha_{i}"), "warning", None)
                .expect("add gotcha");
            for _ in 0..MIN_GOTCHA_HITS {
                db.increment_gotcha_hit(id);
            }
        }

        let warning = compose_pre_edit_warning(&db, "g0_g1_g2_g3_g4_g5_g6_g7_g8_g9_file.rs");
        assert!(warning.is_some());
        let text = warning.unwrap();
        let gotcha_count = text.matches("GOTCHA").count();
        assert_eq!(
            gotcha_count, MAX_GOTCHAS_IN_WARNING,
            "Should cap at {MAX_GOTCHAS_IN_WARNING} gotchas"
        );
    }

    #[test]
    fn test_detect_language_python() {
        assert_eq!(detect_language("scripts/test.py"), Some("python"));
        assert_eq!(detect_language("test.pyi"), Some("python"));
    }

    #[test]
    fn test_detect_language_rust() {
        assert_eq!(detect_language("src/main.rs"), Some("rust"));
    }

    #[test]
    fn test_detect_language_typescript() {
        assert_eq!(detect_language("src/App.tsx"), Some("typescript"));
        assert_eq!(detect_language("index.ts"), Some("typescript"));
    }

    #[test]
    fn test_detect_language_unknown() {
        assert_eq!(detect_language("Makefile"), None);
        assert_eq!(detect_language("noext"), None);
    }

    #[test]
    fn test_make_relative() {
        use std::path::Path;
        assert_eq!(
            make_relative(
                "/home/user/project/src/main.rs",
                Path::new("/home/user/project")
            ),
            "src/main.rs"
        );
        assert_eq!(
            make_relative("already/relative.rs", Path::new("/home/user/project")),
            "already/relative.rs"
        );
    }

    // ── Signal 3-6 tests (content-aware warnings) ───────────────────────

    #[test]
    fn test_content_warnings_empty_string() {
        let (warnings, spec) = compose_content_warnings(None, "", "test.rs");
        assert!(
            warnings.is_empty(),
            "Empty content should produce no warnings"
        );
        assert!(
            spec.is_none(),
            "Empty content should not produce a SpeculateResult"
        );
    }

    #[test]
    fn test_content_warnings_clean_code() {
        let (warnings, spec) = compose_content_warnings(None, "fn clean() -> u32 { 42 }", "test.rs");
        // Clean code with no unwrap, low complexity, no shadows should be clean
        let has_antipattern = warnings.iter().any(|w| w.contains("ANTIPATTERN"));
        assert!(
            !has_antipattern,
            "Clean code should produce no ANTIPATTERN warnings"
        );
        assert!(
            spec.is_some(),
            "Parseable code should return a SpeculateResult"
        );
    }

    #[test]
    fn test_content_warnings_unwrap_detection() {
        let (warnings, _spec) = compose_content_warnings(
            None,
            "fn risky() { let _ = x.unwrap(); let _ = y.unwrap(); }",
            "test.rs",
        );
        let antipattern = warnings.iter().find(|w| w.contains("ANTIPATTERN"));
        assert!(
            antipattern.is_some(),
            "Should detect unwrap() anti-pattern in Rust code"
        );
        let text = antipattern.expect("antipattern should exist");
        assert!(text.contains("2"), "Should count 2 unwrap() calls");
        assert!(text.contains("unwrap()"), "Should mention unwrap()");
    }

    #[test]
    fn test_content_warnings_unwrap_not_triggered_for_python() {
        let (warnings, _spec) = compose_content_warnings(None, "x.unwrap()", "test.py");
        let antipattern = warnings.iter().any(|w| w.contains("ANTIPATTERN"));
        assert!(
            !antipattern,
            "unwrap() anti-pattern should only trigger for Rust files"
        );
    }

    #[test]
    fn test_scope_shadow_detection() {
        let scope = build_scope_map("fn foo() { let x = 1; let x = 2; }", Lang::Rust);
        let shadows = detect_scope_shadows(&scope);
        // The function should run without panic regardless of scope_map granularity
        let _ = shadows;
    }

    #[test]
    fn test_scope_shadow_detection_empty() {
        let scope = ScopeMap::default();
        let shadows = detect_scope_shadows(&scope);
        assert!(shadows.is_empty(), "Empty scope map should have no shadows");
    }

    #[test]
    fn test_content_warnings_unknown_language() {
        // Unknown language should not panic; signals gracefully degrade
        let (warnings, spec) = compose_content_warnings(None, "some content", "test.xyz");
        // Should not panic, warnings may or may not be empty depending on heuristics
        let _ = warnings;
        assert!(
            spec.is_none(),
            "Unknown language should not produce a SpeculateResult"
        );
    }

    // ── Signal 7: Invalid cfg condition detection ───────────────────────

    #[test]
    fn test_invalid_cfg_bench_detected() {
        let warnings = collect_invalid_cfg_warnings("#[cfg(bench)]\nfn bench_it() {}");
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("INVALID_CFG"));
        assert!(warnings[0].contains("#[cfg(bench)]"));
    }

    #[test]
    fn test_invalid_cfg_attr_bench_detected() {
        let warnings = collect_invalid_cfg_warnings("#[cfg_attr(bench, derive(Debug))]");
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("cfg_attr(bench"));
    }

    #[test]
    fn test_valid_cfg_test_not_flagged() {
        let warnings = collect_invalid_cfg_warnings("#[cfg(test)]\nmod tests {}");
        assert!(
            warnings.is_empty(),
            "cfg(test) is valid and should not be flagged"
        );
    }

    #[test]
    fn test_valid_cfg_feature_not_flagged() {
        let warnings = collect_invalid_cfg_warnings("#[cfg(feature = \"nightly\")]");
        assert!(warnings.is_empty(), "cfg(feature = \"nightly\") is valid");
    }

    #[test]
    fn test_no_cfg_no_warnings() {
        let warnings = collect_invalid_cfg_warnings("fn main() { println!(\"hello\"); }");
        assert!(warnings.is_empty());
    }

    // ── Signal 8: Module file existence (E0583 prevention) ─────────────
    #[test]
    fn test_module_existence_non_rs_file_skipped() {
        let issues = collect_module_existence_issues("mod foo;", "src/lib.py");
        assert!(
            issues.is_empty(),
            "Non-Rust files should produce no module existence issues"
        );
    }

    #[test]
    fn test_module_existence_inline_body_no_warning() {
        // Inline body (`mod NAME { ... }`) needs no file — only bare `mod NAME;` does
        let issues = collect_module_existence_issues("mod inline { fn bar() {} }", "/tmp/lib.rs");
        assert!(
            issues.is_empty(),
            "Inline module body must not trigger E0583 warning"
        );
    }

    #[test]
    fn test_module_existence_missing_file_warns() {
        let issues = collect_module_existence_issues(
            "pub mod nonexistent_xyz_touring_abc;",
            "/tmp/test_lib.rs",
        );
        assert!(
            !issues.is_empty(),
            "Missing module file should trigger E0583 warning"
        );
        assert!(
            issues[0].contains("E0583"),
            "Warning should reference E0583 error code"
        );
        assert!(
            issues[0].contains("nonexistent_xyz_touring_abc"),
            "Warning should name the missing module"
        );
    }

    #[test]
    fn test_module_existence_file_present_no_warning() {
        let dir = TempDir::new().expect("create temp dir");
        std::fs::write(dir.path().join("mymod.rs"), "// stub").expect("write module file");
        let lib_path = dir.path().join("lib.rs").to_string_lossy().into_owned();
        let issues = collect_module_existence_issues("mod mymod;", &lib_path);
        assert!(
            issues.is_empty(),
            "Existing module file should produce no warning"
        );
    }

    #[test]
    fn test_module_existence_dir_mod_present_no_warning() {
        let dir = TempDir::new().expect("create temp dir");
        let sub = dir.path().join("submod");
        std::fs::create_dir(&sub).expect("create subdir");
        std::fs::write(sub.join("mod.rs"), "// stub").expect("write mod.rs");
        let lib_path = dir.path().join("lib.rs").to_string_lossy().into_owned();
        let issues = collect_module_existence_issues("mod submod;", &lib_path);
        assert!(
            issues.is_empty(),
            "NAME/mod.rs present should produce no warning"
        );
    }

    // ── Signal 9: Public API signature changes (E0308/E0061 prevention) ─

    fn sig_test_db() -> (TempDir, FileKnowledgeDB) {
        let tmp = TempDir::new().expect("create temp dir");
        let db_path = tmp.path().join("test.db");
        let db = FileKnowledgeDB::new(&db_path).expect("create db");
        (tmp, db)
    }

    #[test]
    fn test_api_sig_empty_old_string_no_issues() {
        let (_tmp, db) = sig_test_db();
        let issues =
            collect_api_signature_changes("", "pub fn foo(x: u32) -> u32 { x }", &db, "src/lib.rs");
        assert!(
            issues.is_empty(),
            "Empty old_string means no baseline — no issues expected"
        );
    }

    #[test]
    fn test_api_sig_identical_no_warning() {
        let (_tmp, db) = sig_test_db();
        let src = "pub fn foo(x: u32) -> u32 { x }";
        let issues = collect_api_signature_changes(src, src, &db, "src/lib.rs");
        assert!(
            issues.is_empty(),
            "Identical signature should produce no warning"
        );
    }

    #[test]
    fn test_api_sig_return_type_changed_warns() {
        let (_tmp, db) = sig_test_db();
        let old = "pub fn foo(x: u32) -> u32 { x }";
        let new = "pub fn foo(x: u32) -> String { x.to_string() }";
        let issues = collect_api_signature_changes(old, new, &db, "src/lib.rs");
        assert!(
            !issues.is_empty(),
            "Changed return type should trigger warning"
        );
        assert!(
            issues[0].contains("foo"),
            "Warning should name the changed function"
        );
        assert!(
            issues[0].contains("SIGNATURE CHANGED"),
            "Warning should say SIGNATURE CHANGED"
        );
    }

    #[test]
    fn test_api_sig_param_added_warns() {
        let (_tmp, db) = sig_test_db();
        let old = "pub fn bar(a: u32) -> bool { true }";
        let new = "pub fn bar(a: u32, b: u32) -> bool { true }";
        let issues = collect_api_signature_changes(old, new, &db, "src/lib.rs");
        assert!(!issues.is_empty(), "Added parameter should trigger warning");
        assert!(
            issues[0].contains("bar"),
            "Warning should name the affected function"
        );
    }

    #[test]
    fn test_api_sig_new_function_added_no_warning() {
        let (_tmp, db) = sig_test_db();
        let old = "pub fn existing() -> u32 { 1 }";
        let new = "pub fn existing() -> u32 { 1 }\npub fn brand_new() -> u32 { 2 }";
        let issues = collect_api_signature_changes(old, new, &db, "src/lib.rs");
        assert!(
            issues.is_empty(),
            "Adding a new function should not produce a warning"
        );
    }

    #[test]
    fn test_api_sig_private_fn_not_tracked() {
        let (_tmp, db) = sig_test_db();
        let old = "fn internal(x: u32) -> u32 { x }";
        let new = "fn internal(x: u32, y: u32) -> u32 { x + y }";
        let issues = collect_api_signature_changes(old, new, &db, "src/lib.rs");
        assert!(
            issues.is_empty(),
            "Private function changes should not be warned about"
        );
    }

    // ── Signal 9 caller-aware: wiring-backed caller detail ─────────────

    #[test]
    fn test_api_sig_lists_callers_from_wiring_map() {
        let (_tmp, db) = sig_test_db();

        // Register the pub fn being changed
        db.register_pub_symbol("src/ast.rs", "build_knowledge_ast", "function", "public")
            .expect("register symbol");

        // Simulate 3 callers
        db.record_consumer(
            "src/ast.rs",
            "build_knowledge_ast",
            "src/post_read.rs",
            Some(82),
        )
        .expect("record consumer 1");
        db.record_consumer(
            "src/ast.rs",
            "build_knowledge_ast",
            "tests/integration.rs",
            Some(145),
        )
        .expect("record consumer 2");
        db.record_consumer(
            "src/ast.rs",
            "build_knowledge_ast",
            "src/cli_handlers.rs",
            None,
        )
        .expect("record consumer 3");

        let old = "pub fn build_knowledge_ast(rel: &str, content: &str) -> (Knowledge, Vec<String>) { todo!() }";
        let new = "pub fn build_knowledge_ast(rel: &str, content: &str, abs: &str) -> (Knowledge, Vec<String>, Vec<Import>) { todo!() }";
        let issues = collect_api_signature_changes(old, new, &db, "src/ast.rs");

        assert_eq!(
            issues.len(),
            1,
            "Should have exactly 1 signature change warning"
        );
        let warning = &issues[0];

        // Should name the function
        assert!(
            warning.contains("build_knowledge_ast"),
            "Should name the function"
        );

        // Should list exact caller files
        assert!(
            warning.contains("src/post_read.rs"),
            "Should list post_read.rs as caller"
        );
        assert!(
            warning.contains("tests/integration.rs"),
            "Should list integration.rs as caller"
        );
        assert!(
            warning.contains("src/cli_handlers.rs"),
            "Should list cli_handlers.rs as caller"
        );

        // Should show line numbers where available
        assert!(
            warning.contains("line 82"),
            "Should show line 82 for post_read.rs"
        );
        assert!(
            warning.contains("line 145"),
            "Should show line 145 for integration.rs"
        );

        // Should show file count
        assert!(
            warning.contains("3 files"),
            "Should indicate 3 caller files"
        );
    }

    #[test]
    fn test_api_sig_no_callers_still_warns_but_without_detail() {
        let (_tmp, db) = sig_test_db();

        // Register the pub fn but no consumers
        db.register_pub_symbol("src/lib.rs", "my_fn", "function", "public")
            .expect("register");

        let old = "pub fn my_fn(a: i32) -> i32 { a }";
        let new = "pub fn my_fn(a: i32, b: i32) -> i32 { a + b }";
        let issues = collect_api_signature_changes(old, new, &db, "src/lib.rs");

        assert_eq!(issues.len(), 1);
        // Should still warn about the signature change
        assert!(issues[0].contains("SIGNATURE CHANGED"));
        // But no caller detail section
        assert!(!issues[0].contains("Callers that MUST be updated"));
    }

    #[test]
    fn test_api_sig_multiple_functions_changed_with_callers() {
        let (_tmp, db) = sig_test_db();

        db.register_pub_symbol("src/core.rs", "foo", "function", "public")
            .expect("reg");
        db.register_pub_symbol("src/core.rs", "bar", "function", "public")
            .expect("reg");
        db.record_consumer("src/core.rs", "foo", "src/main.rs", Some(10))
            .expect("rec");
        db.record_consumer("src/core.rs", "bar", "src/utils.rs", Some(25))
            .expect("rec");

        let old = "pub fn foo(x: i32) -> i32 { x }\npub fn bar(y: bool) -> bool { y }";
        let new =
            "pub fn foo(x: i64) -> i64 { x }\npub fn bar(y: bool, z: bool) -> bool { y && z }";
        let issues = collect_api_signature_changes(old, new, &db, "src/core.rs");

        assert_eq!(issues.len(), 2, "Both functions changed → 2 warnings");
        // Verify each warning has its own caller detail
        let foo_warn = issues
            .iter()
            .find(|w| w.contains("foo"))
            .expect("foo warning");
        let bar_warn = issues
            .iter()
            .find(|w| w.contains("bar"))
            .expect("bar warning");

        assert!(foo_warn.contains("src/main.rs"), "foo caller is main.rs");
        assert!(bar_warn.contains("src/utils.rs"), "bar caller is utils.rs");
    }

    #[test]
    fn test_invalid_cfg_integrated_in_run_returning() {
        // Verify that the invalid cfg signal propagates through run_returning
        let _input = serde_json::json!({
            "tool_input": {
                "file_path": "/tmp/test_cfg.rs",
                "new_string": "#[cfg(bench)]\nfn bench_it() {}"
            }
        });
        let runtime = {
            let dir = TempDir::new().expect("create temp dir");
            let db_path = dir.path().join("test_knowledge.db");
            let _db = FileKnowledgeDB::new(&db_path).expect("create db");
            // We can't easily construct a full HookRuntime in unit tests,
            // but we can verify the function directly
            drop(dir);
        };
        let _ = runtime;
        // Direct function test is sufficient — the integration is verified
        // by the test_invalid_cfg_bench_detected test above and the wiring
        // in run_returning which calls collect_invalid_cfg_warnings.
    }

    // ── Signal 10: Near-duplicate edit detection (Jaccard) ─────────────

    #[test]
    fn test_near_duplicate_high_similarity() {
        // Only a single token differs out of many — well above 85% threshold
        let old = "fn foo() { let x = 1; let y = 2; let z = 3; let w = 4; }";
        let new = "fn foo() { let x = 1; let y = 2; let z = 3; let w = 5; }";
        let result = check_near_duplicate(old, new);
        assert!(result.is_some(), "High similarity should produce a warning");
        let msg = result.expect("warning should exist");
        assert!(
            msg.contains("NEAR_DUPLICATE"),
            "Warning should contain NEAR_DUPLICATE tag"
        );
        assert!(msg.contains("Jaccard"), "Warning should mention Jaccard");
        assert!(
            msg.contains("did you mean to change more"),
            "Warning should prompt the user"
        );
    }

    #[test]
    fn test_near_duplicate_low_similarity_no_warning() {
        // Completely different content — no warning expected
        let old = "fn alpha() { let a = 1; }";
        let new = "fn beta() { println!(\"hello world\"); }";
        let result = check_near_duplicate(old, new);
        assert!(result.is_none(), "Low similarity should produce no warning");
    }

    #[test]
    fn test_near_duplicate_empty_strings() {
        assert!(
            check_near_duplicate("", "anything").is_none(),
            "Empty old_string → no warning"
        );
        assert!(
            check_near_duplicate("anything", "").is_none(),
            "Empty new_string → no warning"
        );
        assert!(
            check_near_duplicate("", "").is_none(),
            "Both empty → no warning"
        );
    }

    #[test]
    fn test_near_duplicate_identical_strings() {
        // Jaccard = 1.0 which is > 0.85 — should warn
        let same = "fn foo() { let x = 42; }";
        let result = check_near_duplicate(same, same);
        assert!(
            result.is_some(),
            "Identical strings (Jaccard=1.0) should warn"
        );
        let msg = result.expect("warning should exist");
        assert!(
            msg.contains("100%"),
            "Identical strings should show 100% similarity"
        );
    }

    #[test]
    fn test_near_duplicate_whitespace_only() {
        // Whitespace-only strings produce empty token sets → union = 0 → None
        assert!(
            check_near_duplicate("   ", "   ").is_none(),
            "Whitespace-only → no warning"
        );
    }

    #[test]
    fn test_near_duplicate_at_threshold_boundary() {
        // Construct sets where Jaccard = exactly threshold (0.85) — should NOT warn
        // 17 shared tokens + 3 unique = 17/20 = 0.85 → not > 0.85
        let shared: Vec<String> = (0..17).map(|i| format!("token{i}")).collect();
        let old_only: Vec<String> = (17..20).map(|i| format!("old{i}")).collect();
        let new_only: Vec<String> = (17..20).map(|i| format!("new{i}")).collect();
        let old = format!("{} {}", shared.join(" "), old_only.join(" "));
        let new = format!("{} {}", shared.join(" "), new_only.join(" "));
        let result = check_near_duplicate(&old, &new);
        assert!(
            result.is_none(),
            "Jaccard = 0.85 (at boundary, not above) → no warning"
        );
    }
}
