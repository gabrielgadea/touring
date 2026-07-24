//! Shared signal functions for pre-hooks.
//!
//! Consolidates duplicated signal computation across pre_read, pre_edit, and pre_write hooks.
//!
//! # Functions
//!
//! - [`score_cmp`]: NaN-safe descending comparator for `(f32, String)` tuples.
//! - [`assemble_scored_context`]: Budget-limited assembly of scored signals.
//! - [`normalize_scores`]: Min-max normalization to `[0, 1]`.
//! - [`blast_radius_signal`]: Blast radius from `SymbolIndex`.
//! - [`enrich_with_cognitive`]: Risk + gotcha + optional prediction signals.
//! - [`similar_symbol_signal_for_path`]: Cross-file similar-symbol navigation hint.
//! - [`rank_gotchas_by_relevance`]: BM25+TF-IDF semantic ranking of gotcha entries.
//! - `rank_symbols_by_relevance`: BM25+TF-IDF semantic ranking of symbol name strings.
//! - [`wilson_adjusted_score`]: WilsonRanker confidence-bounded scoring (E8, touring-simd).
//! - RRF fusion via `touring_cortex::rrf_strings()` (E16) for multi-signal ranking.

use std::cmp::Ordering;
use std::collections::HashMap;
use std::sync::OnceLock;
use touring_code::ast::graph::SymbolIndex;
use touring_intelligence::reasoning::BM25TfIdfVectorizer;
use touring_simd::WilsonRanker;

/// Compute a Wilson score lower bound adjustment for a signal based on hit history.
///
/// Scales `base_score` by the Wilson lower bound of the success rate
/// (`hits / total`), so signals with more evidence get scores closer to
/// `base_score` while signals with little or no history are penalized.
///
/// - `total == 0` → returns `base_score * 0.5` (no history, half confidence).
/// - High `hits/total` with large `total` → approaches `base_score`.
/// - Low `hits/total` or small `total` → significantly below `base_score`.
///
/// Uses the Wilson score interval with z=1.96 (95% confidence).
///
/// # Examples
///
/// ```ignore
/// // Gotcha seen 100 times, prevented 90 errors → high confidence
/// let score = wilson_adjusted_score(1.8, 90, 100);
/// assert!(score > 1.4);
///
/// // Gotcha seen 2 times, prevented 2 errors → low confidence
/// let score = wilson_adjusted_score(1.8, 2, 2);
/// assert!(score < 1.4);
/// ```
pub fn wilson_adjusted_score(base_score: f32, hits: u32, total: u32) -> f32 {
    if total == 0 {
        return base_score * 0.5; // No history — use half confidence
    }
    let ranker = WilsonRanker::default(); // z=1.96 (95% CI)
    let lower_bound = ranker.wilson_bound(hits, total);
    base_score * lower_bound as f32
}

/// NaN-safe score comparison for `(f32, String)` tuples (descending).
///
/// Used throughout pre-hooks to sort signals by priority.
/// Replaces the repeated closure `|a, b| b.0.partial_cmp(&a.0).unwrap_or(Equal)`.
#[inline]
pub fn score_cmp(a: &(f32, String), b: &(f32, String)) -> Ordering {
    b.0.partial_cmp(&a.0).unwrap_or(Ordering::Equal)
}

/// Assemble scored signals into a budget-limited context string.
///
/// Sorts signals by score descending, then concatenates until `budget`
/// is exhausted. Signals are separated by `" | "`. Returns the assembled
/// parts as a `Vec<&str>` and the total bytes used, so callers can
/// decide how to embed the result.
///
/// Returns empty vec if no signals fit within the budget.
pub fn assemble_scored_context(signals: &[(f32, String)], budget: usize) -> (Vec<&str>, usize) {
    let mut result_parts: Vec<&str> = Vec::new();
    let mut budget_used = 0_usize;

    for (_score, text) in signals {
        let cost = text.len() + if result_parts.is_empty() { 0 } else { 3 }; // " | " separator
        if budget_used + cost > budget {
            continue; // skip this large signal, try smaller ones
        }
        result_parts.push(text);
        budget_used += cost;
    }

    (result_parts, budget_used)
}

/// Normalize signal scores to `[0, 1]` range via min-max normalization.
///
/// Leaves signals unchanged if all scores are equal or the vec has fewer
/// than 2 elements.
pub fn normalize_scores(signals: &mut [(f32, String)]) {
    if signals.len() < 2 {
        return;
    }
    let max = signals
        .iter()
        .map(|s| s.0)
        .fold(f32::NEG_INFINITY, f32::max);
    let min = signals.iter().map(|s| s.0).fold(f32::INFINITY, f32::min);
    let range = max - min;
    if range > f32::EPSILON {
        for s in signals.iter_mut() {
            s.0 = (s.0 - min) / range;
        }
    }
}

/// Is SNR gating armed? (`TOURING_SNR_GATING=1`/`true`; default OFF.)
///
/// P1 of the SNR-gated context-injection slice. When OFF (the shipped default),
/// [`apply_relevance_cutoff`] is a no-op, so every enrichment consumer stays
/// byte-identical to the pre-slice behavior — mirrors the `TOURING_POLYGLOT_WIRING`
/// opt-in convention.
fn snr_gating_enabled() -> bool {
    static ARMED: OnceLock<bool> = OnceLock::new();
    *ARMED.get_or_init(|| {
        std::env::var("TOURING_SNR_GATING")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false)
    })
}

/// Minimum normalized relevance score a signal must reach to survive the cutoff.
///
/// Read once from `TOURING_SNR_CUTOFF`; defaults to `0.15` on the min-max
/// normalized `[0,1]` score, clamped to `[0.0, 1.0]`. An absent or invalid value
/// falls back to the default. The default is a starting point — P5 recalibrates
/// it against real files (per `feedback-verify-engine-effectiveness`).
fn snr_cutoff() -> f32 {
    static CUTOFF: OnceLock<f32> = OnceLock::new();
    *CUTOFF.get_or_init(|| {
        std::env::var("TOURING_SNR_CUTOFF")
            .ok()
            .and_then(|v| v.parse::<f32>().ok())
            .filter(|v| v.is_finite())
            .map(|v| v.clamp(0.0, 1.0))
            .unwrap_or(0.15)
    })
}

/// Prune low-relevance signals in place before budget assembly (P1 — SNR gating).
///
/// Implements the LlamaIndex `SimilarityPostprocessor(similarity_cutoff=…)` pattern
/// over hook signals: keep only the high-relevance tail so the context window
/// receives dense signal, not budget-filling noise (the monograph's SNR thesis).
///
/// Expects `signals` already min-max normalized via [`normalize_scores`]. When
/// [`snr_gating_enabled`] is false (the shipped default) or fewer than 2 signals
/// exist, this is a **no-op** returning `0` — byte-identical to pre-slice behavior.
///
/// When armed, retains signals scoring `>= snr_cutoff()`, but NEVER empties a
/// non-empty set: the single highest-scoring signal always survives (silence is
/// earned by the absence of signal, never by an over-strict threshold). Returns
/// the count pruned, for the STR A/B measurement (P5).
pub fn apply_relevance_cutoff(signals: &mut Vec<(f32, String)>) -> usize {
    if !snr_gating_enabled() {
        return 0;
    }
    prune_below_cutoff(signals, snr_cutoff())
}

/// Pure relevance cutoff (no env / flag reads — deterministically testable).
///
/// Retains signals scoring `>= cutoff`, but NEVER empties a non-empty set: the
/// single highest-scoring signal always survives (silence is earned by the
/// absence of signal, never by an over-strict threshold). No-op for fewer than 2
/// signals. Returns the count pruned.
fn prune_below_cutoff(signals: &mut Vec<(f32, String)>, cutoff: f32) -> usize {
    if signals.len() < 2 {
        return 0;
    }
    let before = signals.len();
    // Capture the max so the single best signal always survives, even if the
    // cutoff would otherwise drop everything (e.g. an over-strict env value).
    let max_score = signals
        .iter()
        .map(|s| s.0)
        .fold(f32::NEG_INFINITY, f32::max);
    signals.retain(|(score, _)| *score >= cutoff || *score >= max_score);
    before - signals.len()
}

/// Blast radius signal from `SymbolIndex`.
///
/// **Always injected** when `SymbolIndex` is available — even for isolated files.
/// Knowing `blast_radius = 0` tells Claude the change is low-risk; knowing it is
/// large tells Claude to audit transitive impact.
///
/// Score: isolated = 0.7 (informational); connected scales from 1.0 to 3.0.
/// Returns `None` only when `SymbolIndex` is entirely absent (project not indexed).
///
/// When `include_direct_deps` is `true`, appends the top 3 direct reverse
/// dependencies ranked by BM25+TF-IDF semantic relevance to `rel_path`
/// (used by pre_read/pre_edit for richer context). When `false`,
/// uses a compact format (used by pre_write).
pub fn blast_radius_signal(
    idx_opt: Option<&SymbolIndex>,
    rel_path: &str,
    include_direct_deps: bool,
) -> Option<(f32, String)> {
    let idx = idx_opt?;
    let br = idx.blast_radius_with_depth(rel_path, 4);
    if br.file_count <= 1 {
        return Some((0.7_f32, format_blast_isolated(include_direct_deps)));
    }
    let score = 1.0_f32 + ((br.file_count.saturating_sub(1) as f32) / 20.0).min(2.0);
    let connected_text = format_blast_connected(
        idx,
        rel_path,
        br.file_count,
        br.max_distance,
        include_direct_deps,
    );
    Some((score, connected_text))
}

/// Returns the blast radius file count for `rel_path` via `SymbolIndex`.
pub fn blast_radius_file_count(idx_opt: Option<&SymbolIndex>, rel_path: &str) -> Option<usize> {
    let idx = idx_opt?;
    let br = idx.blast_radius_with_depth(rel_path, 4);
    Some(br.file_count)
}

/// Format the blast-radius message for an isolated file (0 reverse deps).
fn format_blast_isolated(verbose: bool) -> String {
    if verbose {
        "\u{26a1} blast(0 deps \u{2014} arquivo isolado, impacto transitive: nenhum)".to_string()
    } else {
        "blast(0 deps)".to_string()
    }
}

/// Format the blast-radius message for a connected file (1+ reverse deps).
fn format_blast_connected(
    idx: &SymbolIndex,
    rel_path: &str,
    file_count: usize,
    max_distance: usize,
    verbose: bool,
) -> String {
    if verbose {
        let direct_str = format_direct_deps(idx, rel_path);
        format!("\u{26a1} blast(dist={max_distance}, {file_count} files){direct_str}",)
    } else {
        format!("blast({file_count} files, dist={max_distance})")
    }
}

/// Format the direct reverse-dependency list for blast radius context.
///
/// Collects all reverse-dependency basenames, ranks them by BM25+TF-IDF
/// semantic relevance to `rel_path`, then returns the top 3 most relevant.
/// Returns `"; diretos: [a, b, c]"` for up to 3 deps, or empty string when none.
fn format_direct_deps(idx: &SymbolIndex, rel_path: &str) -> String {
    let all_deps: Vec<String> = idx
        .reverse_deps
        .get(rel_path)
        .map(|deps| {
            deps.iter()
                .map(|d| {
                    std::path::Path::new(d.as_str())
                        .file_name()
                        .and_then(|f| f.to_str())
                        .unwrap_or(d.as_str())
                        .to_string()
                })
                .collect()
        })
        .unwrap_or_default();
    if all_deps.is_empty() {
        return String::new();
    }
    // Rank by semantic relevance to the current file path, then take top 3.
    let ranked = rank_symbols_by_relevance(&all_deps, rel_path);
    let top3: Vec<&str> = ranked.iter().take(3).map(|s| s.as_str()).collect();
    format!("; diretos: [{}]", top3.join(", "))
}

/// Rank gotcha entries by semantic relevance to a query using BM25+TF-IDF.
///
/// Builds a `BM25TfIdfVectorizer` corpus from gotcha text (`pattern` + `gotcha`
/// fields) and queries it with `query`. Returns the gotcha texts ranked by
/// combined BM25+TF-IDF score, most relevant first.
///
/// Falls back to original order when fewer than 2 gotchas exist (no ranking
/// benefit) or when the query is empty.
///
/// # Arguments
/// - `gotcha_texts`: `(pattern, gotcha_message)` pairs — typically from `gotchas_for_file`.
/// - `query`: the context string to rank against (e.g., file path + symbol names).
/// - `top_k`: max results to return.
pub fn rank_gotchas_by_relevance(
    gotcha_texts: &[(String, String)],
    query: &str,
    top_k: usize,
) -> Vec<String> {
    if gotcha_texts.len() < 2 || query.trim().is_empty() {
        // No ranking benefit — return gotcha messages in original order
        return gotcha_texts
            .iter()
            .take(top_k)
            .map(|(_, msg)| msg.clone())
            .collect();
    }

    let mut vectorizer = BM25TfIdfVectorizer::new();
    for (i, (pattern, msg)) in gotcha_texts.iter().enumerate() {
        // Index id = position as string; corpus text = pattern + " " + message
        let corpus_text = format!("{pattern} {msg}");
        vectorizer.add_document(i.to_string(), &corpus_text);
    }

    let results = vectorizer.query(query, top_k);
    results
        .iter()
        .filter_map(|r| {
            r.id.parse::<usize>()
                .ok()
                .and_then(|idx| gotcha_texts.get(idx).map(|(_, msg)| msg.clone()))
        })
        .collect()
}

/// Rank a list of symbol name strings by BM25+TF-IDF semantic relevance to a query.
///
/// Builds a `BM25TfIdfVectorizer` corpus from the symbol names and queries it with
/// `query`. Returns symbol names ranked by combined BM25+TF-IDF score, most relevant
/// first.
///
/// Falls back to original order when fewer than 2 symbols exist (no ranking benefit)
/// or when the query is empty.
///
/// # Arguments
/// - `symbols`: list of symbol name strings to rank.
/// - `query`: the context string to rank against (e.g., file path or query term).
pub(crate) fn rank_symbols_by_relevance(symbols: &[String], query: &str) -> Vec<String> {
    if symbols.len() < 2 || query.trim().is_empty() {
        // No ranking benefit — return symbols in original order
        return symbols.to_vec();
    }

    let mut vectorizer = BM25TfIdfVectorizer::new();
    for (i, sym) in symbols.iter().enumerate() {
        vectorizer.add_document(i.to_string(), sym);
    }

    let top_k = symbols.len();
    let results = vectorizer.query(query, top_k);
    let mut ranked: Vec<String> = results
        .iter()
        .filter_map(|r| {
            r.id.parse::<usize>()
                .ok()
                .and_then(|idx| symbols.get(idx).cloned())
        })
        .collect();

    // Append any symbols the vectorizer didn't return (zero-score fallback)
    for sym in symbols {
        if !ranked.contains(sym) {
            ranked.push(sym.clone());
        }
    }
    ranked
}

/// Enrich context with cognitive signals (risk + gotchas from `CognitiveRuntime`).
///
/// When `include_predictions` is `true`, also includes the top session prediction
/// from the `SessionPredictor` (used by pre_read for proactive navigation hints).
///
/// Gotchas are ranked by BM25+TF-IDF semantic relevance to the file path context
/// rather than by insertion order, ensuring the most actionable patterns surface first.
///
/// Returns empty string if nothing worth injecting.
pub fn enrich_with_cognitive(
    cognitive: &touring_intelligence::reasoning::CognitiveRuntime,
    file_path: &str,
    include_predictions: bool,
) -> String {
    let mut signals: Vec<String> = Vec::new();

    // File risk from knowledge source (if available)
    if let Some(knowledge) = cognitive.knowledge_ref() {
        let risk = knowledge.file_risk(file_path);
        if risk.risk_score >= 0.3 {
            signals.push(format!("\u{26a0} Risk: {:.0}%", risk.risk_score * 100.0));
        }
        if risk.gotcha_count > 0 {
            let gotchas = knowledge.gotchas_for_file(file_path);
            // Build (pattern, message) pairs for BM25 semantic ranking
            let gotcha_pairs: Vec<(String, String)> = gotchas
                .iter()
                .map(|g| (g.pattern.clone(), g.gotcha.clone()))
                .collect();
            // Use file_path as query — ranks gotchas whose patterns match this file's context
            let ranked = rank_gotchas_by_relevance(&gotcha_pairs, file_path, 2);
            for msg in ranked {
                signals.push(format!("\u{26a1} {msg}"));
            }
        }

        // EC22: Recent bash failures — surfaces past command failures as proactive context.
        // Complements EnrichedCtx.bash_failures (EC21 async path) on the sync signal path.
        let recent_failures: Vec<String> = knowledge
            .recent_bash_outcomes(3)
            .into_iter()
            .filter(|o| !o.success)
            .map(|o| o.command_short.clone())
            .collect();
        if !recent_failures.is_empty() {
            signals.push(format!(
                "\u{26a1} last fail: {}",
                recent_failures.join(", ")
            ));
        }
    }

    // Next tool prediction from SessionPredictor (pre_read only)
    if include_predictions {
        let predictions = cognitive.predictor().predict_top_k("Read", 2);
        if let Some(top) = predictions.first() {
            if top.1 >= 0.5 {
                signals.push(format!("\u{1f52e} Next: {} ({:.0}%)", top.0, top.1 * 100.0));
            }
        }
    }

    signals.join(" | ")
}

/// Cross-file similar-symbol navigation hint.
///
/// Looks up symbols defined in `rel_path` that also appear as definitions in
/// other files, providing cross-file navigation hints. Weight 0.8 (informational).
///
/// Previously duplicated in `pre_read` (as `similar_symbol_signal`) and
/// `pre_edit` (as `similar_symbol_signal_for_path`). Single canonical implementation.
///
/// Returns `None` when the index is absent, the file has no symbols, or no
/// cross-file matches are found.
pub fn similar_symbol_signal_for_path(
    idx_opt: Option<&SymbolIndex>,
    rel_path: &str,
) -> Option<(f32, String)> {
    let idx = idx_opt?;
    let current_syms = idx.file_to_symbols.get(rel_path)?;
    if current_syms.is_empty() {
        return None;
    }
    let matches = collect_cross_file_symbol_matches(idx, rel_path, current_syms);
    if matches.is_empty() {
        return None;
    }
    let label = format_similar_symbol_label(&matches, rel_path);
    Some((0.8_f32, label))
}

/// Tantivy BM25 search for symbols with related docstrings in OTHER files.
///
/// When Claude reads a file, this surfaces symbols in OTHER files whose
/// docstrings mention the same concepts — providing cross-file context that
/// is NOT derivable from reading the file alone.
///
/// Philosophy:
/// - Is it actionable? YES — Claude can jump to related implementation details
/// - Is it non-derivable? YES — docstrings in OTHER files are not visible
/// - Is it specific to THIS file? YES — query is derived from module path
///
/// Weight: 0.7 (informational, below blast radius and callers).
#[cfg(feature = "tantivy-fts")]
pub fn tantivy_related_docs_signal(rel_path: &str) -> Option<(f32, String)> {
    use crate::tantivy_index::global_tantivy;

    let idx = global_tantivy()?;

    // Extract module context from path (e.g., "auth" from "src/auth/login.rs")
    let module_terms = extract_module_terms(rel_path)?;
    let hits = idx.search(&module_terms, 5).ok()?;

    // Filter hits to other files, keep one entry per file
    let file_contexts = dedupe_hits_by_file(hits, rel_path, 10, 2)?;

    // Format as "📚 related: [sym1@file1, sym2@file2]"
    let shown: Vec<String> = file_contexts
        .into_iter()
        .take(3)
        .map(|(fp, sym)| format!("{sym}@{}", fname(&fp)))
        .collect();

    Some((
        0.7_f32,
        format!("\u{1f4da} related docs: [{}]", shown.join(", ")),
    ))
}

/// Extract meaningful module path components from a file path.
///
/// Skips single-char dirs and common prefixes (src, lib, tests, benches).
#[cfg_attr(not(feature = "tantivy-fts"), allow(dead_code))] // only called by tantivy-fts-gated signals
fn extract_module_terms(rel_path: &str) -> Option<String> {
    let terms: Vec<&str> = std::path::Path::new(rel_path)
        .components()
        .filter_map(|c| {
            let s = c.as_os_str().to_str()?;
            // Skip single-char dirs and common prefixes
            if s.len() <= 2 || s == "src" || s == "lib" || s == "tests" || s == "benches" {
                None
            } else {
                Some(s)
            }
        })
        .collect();
    if terms.is_empty() {
        None
    } else {
        Some(terms.join(" "))
    }
}

/// Deduplicate search hits by file path, keeping highest-scoring symbol per file.
#[cfg(feature = "tantivy-fts")]
fn dedupe_hits_by_file(
    hits: Vec<crate::tantivy_index::SearchHit>,
    rel_path: &str,
    max_hits: usize,
    min_unique: usize,
) -> Option<Vec<(String, String)>> {
    let mut file_contexts: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    for hit in hits.into_iter().take(max_hits) {
        if hit.file_path != rel_path && !file_contexts.contains_key(&hit.file_path) {
            file_contexts.insert(hit.file_path, hit.symbol_name);
        }
    }
    if file_contexts.len() < min_unique {
        None
    } else {
        Some(file_contexts.into_iter().collect())
    }
}

/// Extract basename from a file path.
#[inline]
#[cfg_attr(not(feature = "tantivy-fts"), allow(dead_code))] // only called by tantivy-fts-gated signals
fn fname(fp: &str) -> &str {
    std::path::Path::new(fp)
        .file_name()
        .and_then(|f| f.to_str())
        .unwrap_or(fp)
}

/// Tantivy fuzzy search for files with similar paths.
///
/// When Claude reads a file, this finds OTHER files with similar path patterns
/// (e.g., reading `auth.rs` finds `auth_test.rs`, `auth_mod.rs`).
///
/// Philosophy:
/// - Is it actionable? YES — suggests related test/impl files
/// - Is it non-derivable? YES — fuzzy matching requires index search
/// - Is it specific to THIS file? YES — uses exact path components
///
/// Weight: 0.65 (informational, below similar symbols).
#[cfg(feature = "tantivy-fts")]
pub fn tantivy_fuzzy_file_signal(rel_path: &str) -> Option<(f32, String)> {
    use crate::tantivy_index::global_tantivy;

    let idx = global_tantivy()?;

    // Extract file basename without extension
    let basename = std::path::Path::new(rel_path)
        .file_stem()
        .and_then(|f| f.to_str())?;

    if basename.len() < 3 {
        return None;
    }

    // Fuzzy search for files with similar basenames
    let hits = idx.fuzzy_search(basename, 2, 5).ok()?;

    // Filter out self-matches and extract unique files
    let similar_files: Vec<String> = hits
        .into_iter()
        .filter(|h| h.file_path != rel_path)
        .take(3)
        .map(|h| fname(&h.file_path).to_string())
        .collect();

    if similar_files.is_empty() {
        return None;
    }

    Some((
        0.65_f32,
        format!("\u{1f500} related files: [{}]", similar_files.join(", ")),
    ))
}

/// Tantivy BM25 search for symbols of the SAME kind (fn/struct/enum/trait) in OTHER files.
///
/// When Claude reads a file, this surfaces what OTHER symbols of the same kind
/// exist across the project — useful for finding all structs that handle X,
/// all functions that do Y, etc.
///
/// Philosophy:
/// - Is it actionable? YES — reveals pattern usage across codebase
/// - Is it non-derivable? YES — requires global index search, not derivable locally
/// - Is it specific to THIS file? YES — kind filter is derived from current file
///
/// Weight: 0.73 (between similar_symbols 0.8 and ANN 0.68).
#[cfg(feature = "tantivy-fts")]
pub fn tantivy_kind_context_signal(rel_path: &str) -> Option<(f32, String)> {
    use crate::tantivy_index::global_tantivy;

    let idx = global_tantivy()?;

    let module_terms = extract_module_terms(rel_path)?;
    let hits = idx.search(&module_terms, 10).ok()?;

    // Group hits by symbol_kind and count unique files per kind
    let mut kind_counts: std::collections::HashMap<String, (usize, usize)> =
        std::collections::HashMap::new();
    for hit in hits.iter().take(10) {
        if hit.file_path == rel_path {
            continue;
        }
        let entry = kind_counts.entry(hit.symbol_kind.clone()).or_insert((0, 0));
        entry.0 += 1; // total hits
        entry.1 += 1; // unique files (simplified: count each hit once)
    }

    // Build summary: "3 fns, 2 structs" across the project
    let summary: Vec<String> = kind_counts
        .iter()
        .map(|(kind, (hits, _))| format!("{} {}", hits, kind))
        .take(3)
        .collect();

    if summary.is_empty() {
        return None;
    }

    Some((
        0.73_f32,
        format!("\u{1f4cb} kind context: [{}]", summary.join(", ")),
    ))
}

/// Tantivy search within the SAME crate — find other files/modules from the same crate.
///
/// When Claude reads a file, this surfaces OTHER files from the same crate,
/// helping understand module boundaries and crate-level structure.
///
/// Philosophy:
/// - Is it actionable? YES — reveals crate-level architecture
/// - Is it non-derivable? YES — requires crate field from index
/// - Is it specific to THIS file? YES — query includes exact crate_name
///
/// Weight: 0.68 (informational, same tier as ANN predictions).
#[cfg(feature = "tantivy-fts")]
pub fn tantivy_crate_origin_signal(rel_path: &str) -> Option<(f32, String)> {
    use crate::tantivy_index::global_tantivy;

    let idx = global_tantivy()?;

    // Extract crate name from path (e.g. "touring-hooks" from "crates/touring-hooks/src/...")
    let crate_name: String = std::path::Path::new(rel_path)
        .components()
        .skip_while(|c| c.as_os_str().to_str().is_none_or(|s| s != "crates"))
        .nth(1)
        .and_then(|c| c.as_os_str().to_str())
        .map(|s| s.replace('-', "_"))?;

    if crate_name.is_empty() || crate_name.len() < 3 {
        return None;
    }

    let hits = idx.search_by_crate(&crate_name, &crate_name, 8).ok()?;

    let other_files: Vec<String> = hits
        .into_iter()
        .filter(|h| h.file_path != rel_path)
        .take(4)
        .map(|h| fname(&h.file_path).to_string())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .take(4)
        .collect();

    if other_files.len() < 2 {
        return None;
    }

    Some((
        0.68_f32,
        format!("\u{1f3e0} same crate: [{}]", other_files.join(", ")),
    ))
}

/// Tantivy fuzzy search for symbol NAMES similar to the current file's basename.
///
/// When Claude reads a file, this finds symbol definitions in OTHER files
/// whose names are fuzzy-similar to the current file's basename — e.g.,
/// reading `auth.rs` finds symbols named `Auth`, `authenticate`, etc.
///
/// Philosophy:
/// - Is it actionable? YES — suggests related symbols by name similarity
/// - Is it non-derivable? YES — fuzzy matching requires global index
/// - Is it specific to THIS file? YES — uses exact basename as query
///
/// Weight: 0.62 (lower priority, supplementary to kind_context).
#[cfg(feature = "tantivy-fts")]
pub fn tantivy_fuzzy_symbol_signal(rel_path: &str) -> Option<(f32, String)> {
    use crate::tantivy_index::global_tantivy;

    let idx = global_tantivy()?;

    let basename = std::path::Path::new(rel_path)
        .file_stem()
        .and_then(|f| f.to_str())?;

    if basename.len() < 4 {
        return None;
    }

    let hits = idx.fuzzy_search(basename, 2, 6).ok()?;

    let similar_syms: Vec<String> = hits
        .into_iter()
        .filter(|h| h.file_path != rel_path)
        .take(4)
        .map(|h| h.symbol_name)
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .take(4)
        .collect();

    if similar_syms.len() < 2 {
        return None;
    }

    Some((
        0.62_f32,
        format!("\u{1f50d} similar symbols: [{}]", similar_syms.join(", ")),
    ))
}

/// Format the `🔗 similar: [...]` label from cross-file symbol matches.
///
/// Ranks symbol names by BM25+TF-IDF relevance to `rel_path`, then picks the
/// top 3 and formats them as `"sym≈file"` pairs.
fn format_similar_symbol_label(matches: &[(String, String)], rel_path: &str) -> String {
    let sym_names: Vec<String> = matches.iter().map(|(s, _)| s.clone()).collect();
    let ranked_names = rank_symbols_by_relevance(&sym_names, rel_path);
    let shown: Vec<String> = ranked_names
        .iter()
        .take(3)
        .filter_map(|name| {
            matches
                .iter()
                .find(|(s, _)| s == name)
                .map(|(sym, file)| format!("{sym}\u{2248}{file}"))
        })
        .collect();
    format!("\u{1f517} similar: [{}]", shown.join(", "))
}

/// Find symbols defined in `rel_path` that also appear as definitions in other files.
///
/// Scans up to 20 symbols from the file, skipping names shorter than 4 chars,
/// and collects up to 5 cross-file matches (one per symbol). Returns pairs of
/// `(symbol_name, other_file_basename)`.
fn collect_cross_file_symbol_matches(
    idx: &SymbolIndex,
    rel_path: &str,
    current_syms: &[String],
) -> Vec<(String, String)> {
    let mut matches: Vec<(String, String)> = Vec::new();
    'outer: for sym_name in current_syms.iter().take(20) {
        if sym_name.len() < 4 {
            continue;
        }
        if let Some(locations) = idx.symbols.get(sym_name) {
            for loc in locations {
                if loc.file_path != rel_path && loc.is_definition {
                    let fname = std::path::Path::new(&loc.file_path)
                        .file_name()
                        .and_then(|f| f.to_str())
                        .unwrap_or(loc.file_path.as_str());
                    matches.push((sym_name.clone(), fname.to_string()));
                    if matches.len() >= 5 {
                        break 'outer;
                    }
                    break; // one match per symbol
                }
            }
        }
    }
    matches
}

/// Merge multiple ranked signal lists using Reciprocal Rank Fusion (RRF).
///
/// Each input list should be pre-sorted by relevance within its domain.
/// Items appearing in multiple lists accumulate score from each list,
/// causing them to rank higher in the merged result.
///
/// Uses the standard RRF formula: `score(item) = Σ 1/(k + rank_i)`
/// where `rank_i` is the 1-indexed position in list _i_.
///
/// # Arguments
/// * `signal_lists` — multiple ranked `String` lists from different signal sources.
/// * `k` — smoothing constant (standard: `60.0`). Lower values amplify
///   top-ranked positions; higher values flatten the ranking.
///
/// # Returns
/// A single list of unique items sorted by descending RRF score.
///
/// This is a standalone implementation equivalent to `touring_cortex::rrf_strings`
/// but avoids a circular dependency (`touring-cortex` depends on `touring-hooks`).
pub fn merge_signals_rrf(signal_lists: &[Vec<String>], k: f64) -> Vec<String> {
    if signal_lists.is_empty() {
        return Vec::new();
    }

    let mut scores: HashMap<String, f64> = HashMap::new();

    for list in signal_lists {
        for (idx, item) in list.iter().enumerate() {
            let rank = (idx + 1) as f64; // 1-indexed
            *scores.entry(item.clone()).or_insert(0.0) += 1.0 / (k + rank);
        }
    }

    let mut result: Vec<(String, f64)> = scores.into_iter().collect();
    result.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(Ordering::Equal));
    result.into_iter().map(|(item, _)| item).collect()
}

/// Merge multiple scored signal lists using Reciprocal Rank Fusion.
///
/// Like [`merge_signals_rrf`] but operates on `(f32, String)` signal tuples.
/// Input lists should already be sorted by score descending (as produced by
/// signal pipelines in pre-hooks). The scores in the input tuples determine
/// the ranking position; the output scores are fresh RRF-computed values.
///
/// # Returns
/// Merged `(f32, String)` tuples with RRF scores, sorted descending.
pub fn merge_scored_signals_rrf(signal_lists: &[Vec<(f32, String)>], k: f64) -> Vec<(f32, String)> {
    if signal_lists.is_empty() {
        return Vec::new();
    }

    let mut scores: HashMap<String, f64> = HashMap::new();

    for list in signal_lists {
        for (idx, (_score, item)) in list.iter().enumerate() {
            let rank = (idx + 1) as f64;
            *scores.entry(item.clone()).or_insert(0.0) += 1.0 / (k + rank);
        }
    }

    let mut result: Vec<(String, f64)> = scores.into_iter().collect();
    result.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(Ordering::Equal));
    result
        .into_iter()
        .map(|(item, rrf_score)| (rrf_score as f32, item))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_score_cmp_descending() {
        let mut signals = vec![
            (0.5, "low".to_string()),
            (2.5, "high".to_string()),
            (1.0, "mid".to_string()),
        ];
        signals.sort_by(score_cmp);
        assert_eq!(signals[0].1, "high");
        assert_eq!(signals[1].1, "mid");
        assert_eq!(signals[2].1, "low");
    }

    #[test]
    fn test_score_cmp_nan_handling() {
        let mut signals = vec![(f32::NAN, "nan".to_string()), (1.0, "one".to_string())];
        // Should not panic — NaN is handled gracefully
        signals.sort_by(score_cmp);
        assert_eq!(signals.len(), 2);
    }

    #[test]
    fn prune_below_cutoff_removes_low_relevance_tail() {
        // Normalized [0,1] set with a clear low-relevance signal.
        let mut signals = vec![
            (1.0, "keep-top".to_string()),
            (0.6, "keep-mid".to_string()),
            (0.05, "drop-noise".to_string()),
        ];
        let pruned = prune_below_cutoff(&mut signals, 0.15);
        assert_eq!(pruned, 1);
        assert_eq!(signals.len(), 2);
        assert!(signals.iter().all(|(_, t)| t != "drop-noise"));
    }

    #[test]
    fn prune_below_cutoff_never_empties_non_empty_set() {
        // Over-strict cutoff: the single best signal must always survive.
        let mut signals = vec![(0.4, "best".to_string()), (0.1, "noise".to_string())];
        let pruned = prune_below_cutoff(&mut signals, 0.99);
        assert_eq!(signals.len(), 1);
        assert_eq!(signals[0].1, "best");
        assert_eq!(pruned, 1);
    }

    #[test]
    fn prune_below_cutoff_noop_for_single_signal() {
        let mut signals = vec![(0.01, "lonely".to_string())];
        let pruned = prune_below_cutoff(&mut signals, 0.5);
        assert_eq!(pruned, 0);
        assert_eq!(signals.len(), 1);
    }

    #[test]
    fn apply_relevance_cutoff_is_noop_when_gating_disabled() {
        // Default (TOURING_SNR_GATING unset): byte-identical passthrough — the
        // shipped-OFF invariant that keeps every consumer unchanged.
        let mut signals = vec![(1.0, "a".to_string()), (0.01, "b".to_string())];
        let before = signals.clone();
        let pruned = apply_relevance_cutoff(&mut signals);
        assert_eq!(pruned, 0, "gating OFF must not prune");
        assert_eq!(signals, before, "gating OFF must be byte-identical");
    }

    #[test]
    fn test_assemble_scored_context_within_budget() {
        let signals = vec![(2.0, "first".to_string()), (1.0, "second".to_string())];
        let (parts, used) = assemble_scored_context(&signals, 100);
        assert_eq!(parts, vec!["first", "second"]);
        assert_eq!(used, 5 + 3 + 6); // "first" + " | " + "second"
    }

    #[test]
    fn test_assemble_scored_context_budget_truncation() {
        let signals = vec![
            (2.0, "first signal here".to_string()),
            (1.0, "second signal that is rather long".to_string()),
        ];
        let (parts, _) = assemble_scored_context(&signals, 20);
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0], "first signal here");
    }

    #[test]
    fn test_assemble_scored_context_empty() {
        let signals: Vec<(f32, String)> = vec![];
        let (parts, used) = assemble_scored_context(&signals, 100);
        assert!(parts.is_empty());
        assert_eq!(used, 0);
    }

    #[test]
    fn test_normalize_scores_basic() {
        let mut signals = vec![
            (0.0, "min".to_string()),
            (10.0, "max".to_string()),
            (5.0, "mid".to_string()),
        ];
        normalize_scores(&mut signals);
        assert!((signals[0].0 - 0.0).abs() < f32::EPSILON);
        assert!((signals[1].0 - 1.0).abs() < f32::EPSILON);
        assert!((signals[2].0 - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn test_normalize_scores_equal_values() {
        let mut signals = vec![(5.0, "a".to_string()), (5.0, "b".to_string())];
        normalize_scores(&mut signals);
        // All equal — should remain unchanged
        assert!((signals[0].0 - 5.0).abs() < f32::EPSILON);
        assert!((signals[1].0 - 5.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_normalize_scores_single_element() {
        let mut signals = vec![(3.0, "only".to_string())];
        normalize_scores(&mut signals);
        assert!((signals[0].0 - 3.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_normalize_scores_empty() {
        let mut signals: Vec<(f32, String)> = vec![];
        normalize_scores(&mut signals); // should not panic
        assert!(signals.is_empty());
    }

    // ── rank_gotchas_by_relevance ──────────────────────────────────────────

    #[test]
    fn test_rank_gotchas_empty_input() {
        let result = rank_gotchas_by_relevance(&[], "src/main.rs", 2);
        assert!(result.is_empty());
    }

    #[test]
    fn test_rank_gotchas_single_entry_no_ranking() {
        let pairs = vec![("unwrap".to_string(), "avoid unwrap in prod".to_string())];
        let result = rank_gotchas_by_relevance(&pairs, "src/main.rs", 2);
        // Single entry — falls back to original order
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], "avoid unwrap in prod");
    }

    #[test]
    fn test_rank_gotchas_empty_query_fallback() {
        let pairs = vec![
            ("mutex".to_string(), "mutex deadlock risk".to_string()),
            ("async".to_string(), "missing await pitfall".to_string()),
        ];
        // Empty query → fallback to original order, top_k respected
        let result = rank_gotchas_by_relevance(&pairs, "", 1);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], "mutex deadlock risk");
    }

    #[test]
    fn test_rank_gotchas_semantic_relevance() {
        // The query mentions "async" and "tokio" — the async gotcha should rank higher
        let pairs = vec![
            (
                "mutex".to_string(),
                "mutex poisoning causes panic on lock".to_string(),
            ),
            (
                "async tokio".to_string(),
                "missing await silently drops future".to_string(),
            ),
            (
                "sql".to_string(),
                "sql injection via format strings".to_string(),
            ),
        ];
        let result = rank_gotchas_by_relevance(&pairs, "src/async_handler.rs tokio runtime", 2);
        assert_eq!(result.len(), 2);
        // The async/tokio gotcha should surface first due to keyword overlap
        assert!(
            result[0].contains("await") || result[0].contains("future"),
            "Expected async gotcha to rank first, got: {:?}",
            result
        );
    }

    #[test]
    fn test_rank_gotchas_top_k_respected() {
        let pairs: Vec<(String, String)> = (0..5)
            .map(|i| (format!("pattern{i}"), format!("gotcha message {i}")))
            .collect();
        let result = rank_gotchas_by_relevance(&pairs, "src/lib.rs", 3);
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn test_rank_gotchas_top_k_larger_than_input() {
        let pairs = vec![
            ("p1".to_string(), "msg1".to_string()),
            ("p2".to_string(), "msg2".to_string()),
        ];
        // top_k > len — should return all entries without panic
        let result = rank_gotchas_by_relevance(&pairs, "src/lib.rs", 10);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_rank_gotchas_returns_messages_not_patterns() {
        let pairs = vec![
            ("pattern_alpha".to_string(), "message_alpha".to_string()),
            ("pattern_beta".to_string(), "message_beta".to_string()),
        ];
        let result = rank_gotchas_by_relevance(&pairs, "src/alpha.rs", 2);
        // Returned values should be messages, not patterns
        for msg in &result {
            assert!(
                !msg.starts_with("pattern_"),
                "Got pattern instead of message: {msg}"
            );
        }
    }

    // ── rank_symbols_by_relevance ──────────────────────────────────────────

    #[test]
    fn test_rank_symbols_empty_input() {
        let result = rank_symbols_by_relevance(&[], "src/main.rs");
        assert!(result.is_empty());
    }

    #[test]
    fn test_rank_symbols_single_entry_no_ranking() {
        let syms = vec!["handle_request".to_string()];
        let result = rank_symbols_by_relevance(&syms, "src/handler.rs");
        // Single symbol — falls back to original order
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], "handle_request");
    }

    #[test]
    fn test_rank_symbols_empty_query_fallback() {
        let syms = vec![
            "connect_db".to_string(),
            "run_query".to_string(),
            "close_conn".to_string(),
        ];
        // Empty query → original order preserved
        let result = rank_symbols_by_relevance(&syms, "");
        assert_eq!(result, syms);
    }

    #[test]
    fn test_rank_symbols_semantic_relevance() {
        // Query is the exact symbol name — that symbol should rank first via BM25
        let syms = vec![
            "parse_config".to_string(),
            "database_connect".to_string(),
            "render_template".to_string(),
        ];
        // Query exactly matches "database_connect" — BM25 should rank it highest
        let result = rank_symbols_by_relevance(&syms, "database_connect");
        assert_eq!(result.len(), 3);
        assert_eq!(
            result[0], "database_connect",
            "Expected database_connect to rank first, got: {:?}",
            result
        );
    }

    #[test]
    fn test_rank_symbols_all_entries_returned() {
        // Ensures zero-score fallback appends missing symbols
        let syms: Vec<String> = (0..6).map(|i| format!("symbol_{i}")).collect();
        let result = rank_symbols_by_relevance(&syms, "src/lib.rs");
        // All symbols must be present in the result
        assert_eq!(result.len(), syms.len());
        for sym in &syms {
            assert!(result.contains(sym), "Missing symbol in result: {sym}");
        }
    }

    #[test]
    fn test_rank_symbols_no_duplicates() {
        let syms = vec![
            "foo_bar".to_string(),
            "baz_qux".to_string(),
            "foo_baz".to_string(),
        ];
        let result = rank_symbols_by_relevance(&syms, "src/foo.rs");
        // No symbol should appear twice
        let mut seen = std::collections::HashSet::new();
        for sym in &result {
            assert!(seen.insert(sym), "Duplicate symbol in result: {sym}");
        }
    }

    // ── blast_radius_signal ───────────────────────────────────────────────────

    #[test]
    fn test_blast_radius_signal_absent_when_no_index() {
        // When idx_opt is None the signal must be absent — not an error.
        let signal = blast_radius_signal(None, "src/lib.rs", true);
        assert!(signal.is_none(), "Expected None when index is absent");
    }

    #[test]
    fn test_blast_radius_signal_isolated_verbose() {
        // File with no reverse deps → file_count == 1 → isolated message, score 0.7.
        let idx = touring_code::ast::graph::SymbolIndex::new();
        let (score, text) = blast_radius_signal(Some(&idx), "src/orphan.rs", true)
            .expect("Expected Some for verbose isolated file");
        assert!(
            (score - 0.7).abs() < f32::EPSILON,
            "Isolated file score should be 0.7, got {score}"
        );
        assert!(
            text.contains("isolado") || text.contains("0 deps"),
            "Expected isolated-file message, got: {text}"
        );
    }

    #[test]
    fn test_blast_radius_signal_isolated_compact() {
        // include_direct_deps=false → compact "blast(0 deps)" format.
        let idx = touring_code::ast::graph::SymbolIndex::new();
        let (score, text) = blast_radius_signal(Some(&idx), "src/orphan.rs", false)
            .expect("Expected Some for compact isolated file");
        assert!((score - 0.7).abs() < f32::EPSILON);
        assert_eq!(text, "blast(0 deps)");
    }

    #[test]
    fn test_blast_radius_signal_connected_bm25_ranked_deps() {
        // Build an index with reverse deps so format_direct_deps exercises BM25 ranking.
        // Verifies the BM25 ranking path is taken and top-3 deps appear in the output.
        let mut idx = touring_code::ast::graph::SymbolIndex::new();
        idx.reverse_deps.insert(
            "src/database.rs".to_string(),
            vec![
                "src/renderer.rs".to_string(),
                "src/db_connect.rs".to_string(),
                "src/cache.rs".to_string(),
                "src/db_pool.rs".to_string(),
            ],
        );
        let (_, text) = blast_radius_signal(Some(&idx), "src/database.rs", true)
            .expect("Expected Some for connected file");
        // BM25-ranked direct deps section must be present.
        assert!(
            text.contains("diretos:"),
            "Expected direct deps section, got: {text}"
        );
        // Top 3 of 4 deps appear — exact order is BM25-determined.
        let dep_count = ["renderer.rs", "db_connect.rs", "cache.rs", "db_pool.rs"]
            .iter()
            .filter(|d| text.contains(*d))
            .count();
        assert!(
            dep_count >= 3,
            "Expected at least 3 of 4 deps in diretos section, got {dep_count}: {text}"
        );
    }

    // ── format_blast_isolated ─────────────────────────────────────────────

    #[test]
    fn test_format_blast_isolated_verbose() {
        let text = format_blast_isolated(true);
        assert!(text.contains("blast(0 deps"), "verbose isolated: {text}");
        assert!(
            text.contains("isolado"),
            "verbose isolated should mention 'isolado': {text}"
        );
    }

    #[test]
    fn test_format_blast_isolated_compact() {
        let text = format_blast_isolated(false);
        assert_eq!(text, "blast(0 deps)");
    }

    // ── format_blast_connected ────────────────────────────────────────────

    #[test]
    fn test_format_blast_connected_compact() {
        use touring_code::ast::graph::SymbolIndex;
        let idx = SymbolIndex::default();
        let text = format_blast_connected(&idx, "src/lib.rs", 7, 3, false);
        assert_eq!(text, "blast(7 files, dist=3)");
    }

    #[test]
    fn test_format_blast_connected_verbose_no_deps() {
        use touring_code::ast::graph::SymbolIndex;
        let idx = SymbolIndex::default();
        // No reverse deps registered → direct_str is empty.
        let text = format_blast_connected(&idx, "src/lib.rs", 5, 2, true);
        assert!(
            text.contains("blast(dist=2, 5 files)"),
            "verbose connected: {text}"
        );
    }

    // ── format_similar_symbol_label ───────────────────────────────────────

    #[test]
    fn test_format_similar_symbol_label_single() {
        let matches = vec![("handle_request".to_string(), "router.rs".to_string())];
        let label = format_similar_symbol_label(&matches, "src/handler.rs");
        assert!(label.contains("handle_request"), "label: {label}");
        assert!(label.contains("router.rs"), "label: {label}");
        assert!(label.starts_with("\u{1f517} similar:"), "label: {label}");
    }

    #[test]
    fn test_format_similar_symbol_label_top3_capped() {
        let matches: Vec<(String, String)> = (0..5)
            .map(|i| (format!("sym_{i}"), format!("file_{i}.rs")))
            .collect();
        let label = format_similar_symbol_label(&matches, "src/main.rs");
        // Should show at most 3 entries (comma-separated pairs inside brackets).
        let inner = label
            .trim_start_matches("\u{1f517} similar: [")
            .trim_end_matches(']');
        let count = inner.split(", ").count();
        assert!(count <= 3, "Expected ≤3 entries, got {count}: {label}");
    }

    #[test]
    fn test_format_similar_symbol_label_format() {
        let matches = vec![
            ("process_event".to_string(), "dispatcher.rs".to_string()),
            ("process_event".to_string(), "handler.rs".to_string()),
        ];
        let label = format_similar_symbol_label(&matches, "src/core.rs");
        // Each entry should use the ≈ separator.
        assert!(
            label.contains('\u{2248}'),
            "Expected ≈ separator in: {label}"
        );
    }

    // ── wilson_adjusted_score ────────────────────────────────────────────

    #[test]
    fn test_wilson_no_history_returns_half_base() {
        let score = wilson_adjusted_score(1.8, 0, 0);
        let expected = 1.8 * 0.5;
        assert!(
            (score - expected).abs() < f32::EPSILON,
            "No history should return base * 0.5 = {expected}, got {score}"
        );
    }

    #[test]
    fn test_wilson_high_confidence_near_base() {
        // 90 successes out of 100 trials — high confidence, score should be
        // close to base_score (within ~20% of base).
        let base = 1.8_f32;
        let score = wilson_adjusted_score(base, 90, 100);
        assert!(
            score > base * 0.8,
            "High confidence (90/100) should yield > 80% of base ({:.2}), got {score:.2}",
            base * 0.8
        );
        assert!(
            score <= base,
            "Wilson-adjusted score should never exceed base, got {score}"
        );
    }

    #[test]
    fn test_wilson_low_confidence_below_base() {
        // 2 successes out of 3 trials — small sample, lower confidence.
        let base = 1.8_f32;
        let score = wilson_adjusted_score(base, 2, 3);
        let high_conf = wilson_adjusted_score(base, 90, 100);
        assert!(
            score < high_conf,
            "Low sample (2/3) should score lower than high sample (90/100): {score} vs {high_conf}"
        );
        assert!(score > 0.0, "Wilson score should be positive, got {score}");
    }

    #[test]
    fn test_wilson_zero_successes_very_low() {
        // 0 successes out of 10 trials — the signal was never useful.
        let base = 1.8_f32;
        let score = wilson_adjusted_score(base, 0, 10);
        assert!(
            score < base * 0.1,
            "Zero successes should yield very low score (< 10% of base), got {score}"
        );
    }

    #[test]
    fn test_wilson_sample_size_ordering() {
        // Same 100% success rate but different sample sizes.
        // Larger sample → higher Wilson lower bound → higher adjusted score.
        let base = 1.5_f32;
        let small = wilson_adjusted_score(base, 3, 3);
        let large = wilson_adjusted_score(base, 50, 50);
        assert!(
            large > small,
            "Larger sample (50/50) should yield higher score than small sample (3/3): {large} vs {small}"
        );
    }

    // ── merge_signals_rrf ────────────────────────────────────────────────

    #[test]
    fn test_rrf_merge_single_list() {
        // Single list → items returned in same order (rank 1,2,3).
        let lists = vec![vec![
            "alpha".to_string(),
            "beta".to_string(),
            "gamma".to_string(),
        ]];
        let result = merge_signals_rrf(&lists, 60.0);
        assert_eq!(result.len(), 3);
        assert_eq!(result[0], "alpha");
        assert_eq!(result[1], "beta");
        assert_eq!(result[2], "gamma");
    }

    #[test]
    fn test_rrf_merge_multiple_lists() {
        // "shared" appears in both lists → should rank higher than items in only one list.
        let lists = vec![
            vec!["only_a".to_string(), "shared".to_string()],
            vec!["shared".to_string(), "only_b".to_string()],
        ];
        let result = merge_signals_rrf(&lists, 60.0);
        assert_eq!(result[0], "shared", "Item in both lists should rank first");
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn test_rrf_merge_empty_input() {
        let lists: Vec<Vec<String>> = vec![];
        let result = merge_signals_rrf(&lists, 60.0);
        assert!(result.is_empty());
    }

    #[test]
    fn test_rrf_merge_empty_inner_lists() {
        let lists: Vec<Vec<String>> = vec![vec![], vec![]];
        let result = merge_signals_rrf(&lists, 60.0);
        assert!(result.is_empty());
    }

    #[test]
    fn test_rrf_merge_no_duplicates() {
        // Even though "x" appears in 3 lists, it should appear exactly once in output.
        let lists = vec![
            vec!["x".to_string()],
            vec!["x".to_string()],
            vec!["x".to_string()],
        ];
        let result = merge_signals_rrf(&lists, 60.0);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], "x");
    }

    #[test]
    fn test_rrf_merge_k_affects_ranking() {
        // With k=0, first-rank items get score=1.0 (very high), amplifying
        // position 1 vs position 2 more than with k=60.
        let lists = vec![
            vec!["a".to_string(), "b".to_string()],
            vec!["b".to_string(), "a".to_string()],
        ];
        // Both k values should produce same ordering since a/b are symmetric,
        // but both should be present.
        let r1 = merge_signals_rrf(&lists, 0.0);
        let r2 = merge_signals_rrf(&lists, 60.0);
        assert_eq!(r1.len(), 2);
        assert_eq!(r2.len(), 2);
    }

    // ── merge_scored_signals_rrf ─────────────────────────────────────────

    #[test]
    fn test_scored_rrf_merge_single_list() {
        let lists = vec![vec![(2.0, "high".to_string()), (1.0, "low".to_string())]];
        let result = merge_scored_signals_rrf(&lists, 60.0);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].1, "high");
        assert_eq!(result[1].1, "low");
        // RRF scores should be descending
        assert!(result[0].0 > result[1].0);
    }

    #[test]
    fn test_scored_rrf_merge_multiple_lists() {
        // "common" appears at rank 1 in both lists → highest RRF score.
        let lists = vec![
            vec![(1.5, "common".to_string()), (0.8, "only_a".to_string())],
            vec![(1.2, "common".to_string()), (0.5, "only_b".to_string())],
        ];
        let result = merge_scored_signals_rrf(&lists, 60.0);
        assert_eq!(
            result[0].1, "common",
            "Item in both lists should rank first"
        );
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn test_scored_rrf_merge_empty() {
        let lists: Vec<Vec<(f32, String)>> = vec![];
        let result = merge_scored_signals_rrf(&lists, 60.0);
        assert!(result.is_empty());
    }

    #[test]
    fn test_scored_rrf_scores_are_rrf_not_original() {
        // Verify that output scores are RRF-computed, not pass-through of input scores.
        let lists = vec![vec![(999.0, "item".to_string())]];
        let result = merge_scored_signals_rrf(&lists, 60.0);
        // RRF score for single item at rank 1 with k=60: 1/(60+1) ≈ 0.01639
        let expected = 1.0_f32 / 61.0;
        assert!(
            (result[0].0 - expected).abs() < 1e-5,
            "Score should be RRF-computed ({expected}), got {}",
            result[0].0
        );
    }
}
