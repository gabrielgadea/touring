//! AST Bridge — Connects touring-ast code intelligence to the hook system.
//!
//! Intelligence layer between touring-ast and hooks:
//! - Symbol extraction with enrichment (kind, parent, docstring, complexity)
//! - File quality analysis (max/avg complexity, async ratio, nesting depth)
//! - Edit impact analysis (syntax validation + blast radius + complexity delta)
//! - diff_pub_symbols: pub API surface change detection (E13)
//! - Durability classification for early-exit gating (E14)
//! - fuse_quality_evidence: MetacognitivePipeline Bayesian fusion (AF9)
//! - Import extraction for knowledge graph (tree-sitter, not regex)

use std::path::Path;

use touring_code::ast::{
    Lang, PubSymbol, QualityReport, Severity, Symbol, SymbolDiff, SymbolIndex, analyze_quality,
    compute_complexity_for_source, diff_pub_symbols, enrich_symbols_with_complexity,
    extract_cfg_gated_pub_items, extract_pub_symbols, extract_symbols, extract_symbols_from_file,
    revision::Durability, surgery::SurgeryError, validate_syntax,
};

use crate::knowledge::FileKnowledge;

// ─── Enriched types ──────────────────────────────────────────────────────────

/// Summary quality metrics for a file, computed from AST analysis.
#[derive(Debug, Clone, serde::Serialize)]
pub struct FileQualityMetrics {
    /// Total number of symbols
    pub symbol_count: usize,
    /// Number of callable symbols (functions, methods)
    pub callable_count: usize,
    /// Number of type definitions (classes, structs, enums)
    pub type_count: usize,
    /// Maximum cyclomatic complexity across all callables
    pub max_complexity: u16,
    /// Average cyclomatic complexity across all callables
    pub avg_complexity: f32,
    /// Number of callables with CC > 10 (warning threshold)
    pub high_complexity_count: usize,
    /// Number of async callables
    pub async_count: usize,
    /// Ratio of async to total callables (0.0-1.0)
    pub async_ratio: f32,
    /// Maximum nesting depth (symbols with parents)
    pub has_nesting: bool,
    /// Names of symbols with CC > 10
    pub complex_symbols: Vec<String>,
}

/// Result of validating an edit's impact on the codebase.
#[derive(Debug, Clone, serde::Serialize)]
pub struct EditImpactResult {
    /// Whether the resulting syntax is valid
    pub syntax_valid: bool,
    /// Number of files affected by this change (blast radius)
    pub affected_files: usize,
    /// Maximum dependency distance
    pub max_distance: usize,
    /// Complexity delta: (symbol_name, old_cc, new_cc)
    pub complexity_changes: Vec<(String, u16, u16)>,
    /// Whether any complexity increased above the threshold
    pub complexity_violation: bool,
    /// Human-readable summary for hook context injection
    pub summary: String,
}

// ─── Core functions ──────────────────────────────────────────────────────────

/// Extract structured symbol information from a file using tree-sitter.
///
/// Returns `None` if the language is unsupported or parsing fails.
/// This replaces the regex-based extraction in `post_read.rs`.
pub fn extract_file_symbols(file_path: &str) -> Option<Vec<Symbol>> {
    let path = Path::new(file_path);
    extract_symbols_from_file(path).ok()
}

/// Extract symbol metadata from source, for `FileKnowledge` enrichment.
///
/// Returns `(symbols_json, symbol_count)` where each symbol is a compact object
/// with `name`, `kind`, `is_public`, and `line` — the minimum fields needed by
/// the wiring system to register pub symbols and compute integration scores.
pub fn enrich_file_knowledge(source: &str, file_path: &str) -> Option<(String, usize)> {
    let lang = Lang::from_path(Path::new(file_path))?;
    let symbols = extract_symbols(source, lang).ok()?;

    let count = symbols.len();
    // Serialize compact symbol objects (not full Symbol — avoid bloating knowledge.db)
    let compact: Vec<serde_json::Value> = symbols
        .iter()
        .map(|s| {
            serde_json::json!({
                "name": s.name,
                "kind": s.kind.as_str(),
                "is_public": s.is_public,
                "line": s.line,
            })
        })
        .collect();
    let json = serde_json::to_string(&compact).unwrap_or_else(|_| "[]".to_string());

    Some((json, count))
}

/// Extract symbols with full enrichment including complexity.
///
/// This is the **primary entry point** for hooks that need complete symbol data.
/// Complexity is computed via tree-sitter AST walk (not external tools).
pub fn extract_enriched_symbols(source: &str, file_path: &str) -> Option<Vec<Symbol>> {
    let lang = Lang::from_path(Path::new(file_path))?;
    let mut symbols = extract_symbols(source, lang).ok()?;

    // Enrich with complexity (uses touring-ast internal tree_sitter access).
    // Non-fatal: enrichment failure leaves symbols without complexity scores,
    // which is acceptable — callers handle missing complexity gracefully.
    if let Err(e) = enrich_symbols_with_complexity(&mut symbols, source, lang) {
        tracing::debug!(
            file = file_path,
            error = %e,
            "complexity enrichment skipped (non-fatal)"
        );
    }

    Some(symbols)
}

/// Extract import information from source for dependency graph.
///
/// Returns a list of `(module_path, imported_symbols)` tuples.
pub fn extract_file_imports(source: &str, file_path: &str) -> Vec<(String, Vec<String>)> {
    let lang = match Lang::from_path(Path::new(file_path)) {
        Some(l) => l,
        None => return Vec::new(),
    };

    touring_code::ast::graph::extract_imports(source, lang)
        .into_iter()
        .map(|imp| (imp.module_path, imp.symbols))
        .collect()
}

/// Extract method-call / associated-function-call identifiers used in
/// the file. Captures `.method()` dispatch and `Type::assoc_fn()` syntax
/// that the import-based extractor misses.
///
/// Returns an unordered, deduplicated `Vec<String>` of identifier names.
/// Callers cross-reference the names against wiring_map producer rows to
/// record dynamic-dispatch consumer edges (F9 of the 2026-05-11 audit).
///
/// Empty vec for non-Rust files (Python / TS imports already cover both
/// types and functions, so the extra walk is not needed there).
pub fn extract_file_method_calls(source: &str, file_path: &str) -> Vec<String> {
    let lang = match Lang::from_path(Path::new(file_path)) {
        Some(l) => l,
        None => return Vec::new(),
    };
    touring_code::ast::graph::extract_method_calls(source, lang)
}

/// Compute blast radius for a file using the touring-ast symbol index.
///
/// Returns `(affected_file_count, max_distance)`.
/// Returns `None` if the index is empty or the file is not indexed.
pub fn compute_blast_radius(index: &SymbolIndex, file_path: &str) -> Option<(usize, usize)> {
    let radius = index.blast_radius(file_path);
    if radius.file_count <= 1 {
        return None;
    }
    Some((radius.file_count, radius.max_distance))
}

/// Validate syntax of a source string using tree-sitter.
///
/// Returns `Some(true)` when the grammar parsed the source cleanly, and
/// `Some(false)` when it parsed the source and found genuine error nodes.
///
/// Returns `None` when the check could not run at all — the language is
/// unsupported, or the grammar failed to load (a tree-sitter ABI mismatch).
/// A grammar that cannot be loaded is an infrastructure limitation: it is
/// reported as `None` ("undetermined"), never as `Some(false)`, so an ABI
/// mismatch never masquerades as a syntax error in the analyzed source.
pub fn validate_file_syntax(source: &str, file_path: &str) -> Option<bool> {
    let lang = Lang::from_path(Path::new(file_path))?;
    match validate_syntax(source, lang) {
        // Grammar parsed the source with no error nodes.
        Ok(_) => Some(true),
        // Grammar parsed the source and reported genuine error nodes.
        Err(SurgeryError::SyntaxError(_)) => Some(false),
        // Grammar could not be loaded (ABI mismatch) or the parser produced
        // no tree — the check could not run. Undetermined, not "invalid".
        Err(_) => None,
    }
}

// ─── Quality analysis ────────────────────────────────────────────────────────

/// Analyze file quality using AST-derived metrics.
///
/// Computes complexity, async ratio, nesting, and identifies problem symbols.
/// This powers the pre-edit hook's quality gate.
pub fn analyze_file_quality(source: &str, file_path: &str) -> Option<FileQualityMetrics> {
    let symbols = extract_enriched_symbols(source, file_path)?;

    let callable_count = symbols.iter().filter(|s| s.kind.is_callable()).count();
    let type_count = symbols
        .iter()
        .filter(|s| s.kind.is_type_definition())
        .count();
    let async_count = symbols.iter().filter(|s| s.is_async).count();
    let has_nesting = symbols.iter().any(|s| s.parent_name.is_some());

    let complexities: Vec<u16> = symbols.iter().filter_map(|s| s.complexity).collect();

    let max_complexity = complexities.iter().copied().max().unwrap_or(1);
    let avg_complexity = if complexities.is_empty() {
        1.0
    } else {
        complexities.iter().map(|&c| c as f32).sum::<f32>() / complexities.len() as f32
    };

    let complex_symbols: Vec<String> = symbols
        .iter()
        .filter(|s| s.complexity.map(|c| c > 10).unwrap_or(false))
        .map(|s| format!("{}(CC={})", s.name, s.complexity.unwrap_or(0)))
        .collect();

    let async_ratio = if callable_count > 0 {
        async_count as f32 / callable_count as f32
    } else {
        0.0
    };

    Some(FileQualityMetrics {
        symbol_count: symbols.len(),
        callable_count,
        type_count,
        max_complexity,
        avg_complexity,
        high_complexity_count: complex_symbols.len(),
        async_count,
        async_ratio,
        has_nesting,
        complex_symbols,
    })
}

/// Generate a compact quality summary for hook context injection.
///
/// Returns a short string suitable for `additionalContext`.
pub fn quality_summary(metrics: &FileQualityMetrics) -> String {
    let mut parts = Vec::new();

    parts.push(format!(
        "{} symbols ({} callable, {} types)",
        metrics.symbol_count, metrics.callable_count, metrics.type_count
    ));

    if metrics.max_complexity > 1 {
        parts.push(format!(
            "CC: max={} avg={:.1}",
            metrics.max_complexity, metrics.avg_complexity
        ));
    }

    if !metrics.complex_symbols.is_empty() {
        parts.push(format!("HIGH CC: {}", metrics.complex_symbols.join(", ")));
    }

    if metrics.async_count > 0 {
        parts.push(format!(
            "async: {}/{} ({:.0}%)",
            metrics.async_count,
            metrics.callable_count,
            metrics.async_ratio * 100.0
        ));
    }

    parts.join(" | ")
}

/// Analyze code quality using the AST-native multi-language quality module.
///
/// Bridges `file_path` → language detection → [`touring_code::ast::analyze_quality`].
/// Covers Rust, Python, TypeScript, JavaScript, Go (feature-gated), Java (feature-gated).
///
/// Returns `None` only if the file extension maps to no supported language.
#[must_use]
pub fn analyze_ast_quality(source: &str, file_path: &str) -> Option<QualityReport> {
    let lang = Lang::from_path(Path::new(file_path))?;
    Some(analyze_quality(source, lang))
}

// ─── Edit impact analysis ────────────────────────────────────────────────────

/// Validate the impact of an edit before/after application.
///
/// Compares old and new source to detect:
/// 1. Syntax validity of the new version
/// 2. Complexity changes per symbol
/// 3. Blast radius (requires a populated SymbolIndex)
///
/// The `complexity_threshold` parameter sets the CC limit (default: 10).
pub fn validate_edit_impact(
    old_source: &str,
    new_source: &str,
    file_path: &str,
    index: Option<&SymbolIndex>,
    complexity_threshold: u16,
) -> Option<EditImpactResult> {
    let lang = Lang::from_path(Path::new(file_path))?;

    // 1. Syntax validation.
    // Only a genuine `SyntaxError` (the grammar parsed `new_source` and found
    // error nodes) means the new version is broken. A grammar that fails to
    // load — a tree-sitter ABI mismatch — or a parser that yields no tree is
    // an infrastructure limitation, never a defect in `new_source`. Treating
    // those as `syntax_valid = false` produced false-positive "SYNTAX ERROR"
    // reports on every edit of an ABI-mismatched language (e.g. markdown).
    let syntax_valid = !matches!(
        validate_syntax(new_source, lang),
        Err(SurgeryError::SyntaxError(_))
    );

    // 2. Blast radius
    let (affected_files, max_distance) = index
        .and_then(|idx| compute_blast_radius(idx, file_path))
        .unwrap_or((1, 0));

    // 3. Complexity delta
    let old_cc = compute_complexity_for_source(old_source, lang).unwrap_or_default();
    let new_cc = compute_complexity_for_source(new_source, lang).unwrap_or_default();

    let mut complexity_changes = Vec::new();
    let mut complexity_violation = false;

    // Build lookup for old complexity values
    let old_map: std::collections::HashMap<&str, u16> = old_cc
        .iter()
        .map(|(name, cc)| (name.as_str(), *cc))
        .collect();

    for (name, new_val) in &new_cc {
        let old_val = old_map.get(name.as_str()).copied().unwrap_or(1);
        if *new_val != old_val {
            complexity_changes.push((name.clone(), old_val, *new_val));
            if *new_val > complexity_threshold {
                complexity_violation = true;
            }
        }
    }

    // 4. Build summary
    let mut summary_parts = Vec::new();

    if !syntax_valid {
        summary_parts.push("SYNTAX ERROR in new version".to_string());
    }

    if affected_files > 1 {
        summary_parts.push(format!(
            "Blast radius: {} files (max distance {})",
            affected_files, max_distance
        ));
    }

    if !complexity_changes.is_empty() {
        let changes: Vec<String> = complexity_changes
            .iter()
            .map(|(name, old, new)| format!("{}:{}->{}", name, old, new))
            .collect();
        summary_parts.push(format!("CC changes: {}", changes.join(", ")));
    }

    if complexity_violation {
        summary_parts.push(format!(
            "VIOLATION: CC exceeds threshold {}",
            complexity_threshold
        ));
    }

    let summary = if summary_parts.is_empty() {
        "No impact detected".to_string()
    } else {
        summary_parts.join(" | ")
    };

    Some(EditImpactResult {
        syntax_valid,
        affected_files,
        max_distance,
        complexity_changes,
        complexity_violation,
        summary,
    })
}

/// Quick complexity check for a single function after edit.
///
/// Returns `Some((cc, is_violation))` if the symbol is callable.
pub fn check_symbol_complexity(
    source: &str,
    file_path: &str,
    symbol_name: &str,
    threshold: u16,
) -> Option<(u16, bool)> {
    let lang = Lang::from_path(Path::new(file_path))?;
    let complexities = compute_complexity_for_source(source, lang).ok()?;

    for (name, cc) in &complexities {
        if name == symbol_name {
            return Some((*cc, *cc > threshold));
        }
    }

    None
}

// ─── Knowledge enrichment ────────────────────────────────────────────────────

/// Build a `FileKnowledge` struct enriched with AST data.
///
/// This is the primary integration point for `post_read` hooks.
pub fn build_enriched_knowledge(file_path: &str, source: &str) -> FileKnowledge {
    let lang = Lang::from_path(Path::new(file_path));

    let language = lang.map(|l| l.as_str().to_string()).unwrap_or_default();

    let (symbols_json, symbol_count) =
        enrich_file_knowledge(source, file_path).unwrap_or_else(|| ("[]".to_string(), 0));

    let imports: Vec<String> = extract_file_imports(source, file_path)
        .into_iter()
        .map(|(module, _syms)| module)
        .collect();
    let imports_json = serde_json::to_string(&imports).unwrap_or_else(|_| "[]".to_string());

    let line_count = source.lines().count();
    let content_hash = {
        use sha2::{Digest, Sha256};
        let hash = Sha256::digest(source.as_bytes());
        format!("{:x}", hash)[..12].to_string()
    };

    FileKnowledge {
        file_path: file_path.to_string(),
        language: Some(language),
        line_count: line_count as i64,
        symbol_count: symbol_count as i64,
        read_count: 1,
        last_read_at: None,
        content_hash: Some(content_hash),
        imports_json: Some(imports_json),
        symbols_json: Some(symbols_json),
        notes: None,
    }
}

/// Build a `FileKnowledge` struct with full enrichment including quality metrics.
///
/// Extends `build_enriched_knowledge` with notes from two quality passes:
/// 1. **Complexity** — symbols with CC > 10 from `analyze_file_quality`
/// 2. **Anti-patterns** — error-severity hits from `analyze_ast_quality`
///
/// The `notes` field is only set when actionable issues are found.
pub fn build_enriched_knowledge_with_quality(file_path: &str, source: &str) -> FileKnowledge {
    let mut knowledge = build_enriched_knowledge(file_path, source);

    let mut note_parts: Vec<String> = Vec::new();

    // Pass 1: complexity analysis via enriched symbols
    if let Some(metrics) = analyze_file_quality(source, file_path) {
        if !metrics.complex_symbols.is_empty() {
            note_parts.push(format!(
                "High complexity: {}",
                metrics.complex_symbols.join(", ")
            ));
        }
    }

    // Pass 2: anti-pattern analysis via AST-native quality module
    if let Some(report) = analyze_ast_quality(source, file_path) {
        let error_patterns: Vec<&str> = report
            .antipatterns
            .iter()
            .filter(|h| h.severity == Severity::Error)
            .map(|h| h.name)
            .collect();
        if !error_patterns.is_empty() {
            note_parts.push(format!("Anti-patterns: {}", error_patterns.join(", ")));
        }
        if report.overall_score < 0.7 {
            note_parts.push(format!("Quality score: {:.2}", report.overall_score));
        }
    }

    if !note_parts.is_empty() {
        knowledge.notes = Some(note_parts.join(" | "));
    }

    knowledge
}

// ─── Bayesian quality fusion ─────────────────────────────────────────────────

/// Fuses multiple quality signals into a single authoritative quality estimate
/// using Bayesian evidence fusion with Wilson-weighted conflict detection.
///
/// Collects evidence from two independent quality analysis paths:
/// 1. **Complexity** — `analyze_file_quality` (symbol-level cyclomatic complexity)
/// 2. **Anti-patterns** — `analyze_ast_quality` (AST-native multi-language quality)
///
/// Returns `None` if no quality data is available (unsupported language, etc.).
#[must_use]
pub fn fuse_quality_evidence(
    source: &str,
    file_path: &str,
    pipeline: &touring_simd::cortex::MetacognitivePipeline,
) -> Option<touring_simd::cortex::Resolved> {
    use touring_simd::cortex::Evidence;

    let mut evidence = Vec::new();

    // Source 1: complexity-based quality from enriched symbols
    if let Some(metrics) = analyze_file_quality(source, file_path) {
        // Normalize: low complexity -> high quality score.
        // avg_complexity of 1.0 -> score 0.95; avg_complexity of 20+ -> score 0.0
        let complexity_score = (1.0 - (metrics.avg_complexity / 20.0).min(1.0)) as f64;
        // Confidence scales with the number of callable symbols observed
        let complexity_observations = metrics.callable_count.max(1) as u32;
        let complexity_successes = ((complexity_score * f64::from(complexity_observations)) as u32)
            .min(complexity_observations);
        evidence.push(Evidence {
            source_id: evidence.len(),
            value: complexity_score,
            confidence: 0.8,
            successes: complexity_successes,
            total: complexity_observations,
        });
    }

    // Source 2: AST-native quality report (anti-patterns + complexity + Wilson CI)
    if let Some(report) = analyze_ast_quality(source, file_path) {
        let ast_confidence = report
            .confidence_interval
            .map(|(lower, _)| lower as f64)
            .unwrap_or(0.7);
        let ast_trials = report.confidence_n_trials.unwrap_or(100);
        let ast_successes = (report.overall_score * ast_trials as f32) as u32;
        evidence.push(Evidence {
            source_id: evidence.len(),
            value: report.overall_score as f64,
            confidence: ast_confidence,
            successes: ast_successes,
            total: ast_trials,
        });
    }

    if evidence.is_empty() {
        return None;
    }

    Some(pipeline.resolve(&evidence))
}

// ─── Cross-cfg blast radius ──────────────────────────────────────────────────

/// Blast radius extended by `#[cfg(...)]`-gated pub items.
///
/// Changing a cfg-gated pub item affects ALL consumers that compile with the
/// cfg condition active — not just the default build graph. This struct makes
/// that extended impact explicit for hook context injection.
#[derive(Debug, Clone, serde::Serialize)]
pub struct CrossFeatureBlastRadius {
    /// Number of cfg-gated pub items found in the source
    pub gated_item_count: usize,
    /// Distinct cfg conditions (e.g. `["unix", "feature = \"async\""]`)
    pub cfg_conditions: Vec<String>,
    /// Names of cfg-gated pub items
    pub gated_symbols: Vec<String>,
    /// Human-readable summary for hook context injection
    pub summary: String,
}

/// Compute cross-cfg blast radius for a Rust source file.
///
/// Scans `source` for public items gated behind any `#[cfg(...)]` condition
/// (features, target OS, architectures, `not`/`all`/`any` composites, etc.)
/// and builds a [`CrossFeatureBlastRadius`] describing the extended impact.
///
/// Returns `None` for non-Rust files (cfg detection is Rust-specific).
#[must_use]
pub fn compute_blast_radius_cross_feature(
    source: &str,
    file_path: &str,
    _index: Option<&SymbolIndex>,
) -> Option<CrossFeatureBlastRadius> {
    // cfg detection is meaningful for Rust only
    let lang = Lang::from_path(Path::new(file_path))?;
    if lang != Lang::Rust {
        return None;
    }

    let items = extract_cfg_gated_pub_items(source);
    if items.is_empty() {
        return Some(CrossFeatureBlastRadius {
            gated_item_count: 0,
            cfg_conditions: Vec::new(),
            gated_symbols: Vec::new(),
            summary: "No cfg-gated pub items found".to_string(),
        });
    }

    let mut conditions: Vec<String> = items.iter().map(|i| i.cfg_condition.clone()).collect();
    conditions.sort_unstable();
    conditions.dedup();

    let gated_symbols: Vec<String> = items.iter().map(|i| i.name.clone()).collect();

    let summary = format!(
        "{} pub item(s) behind cfg condition(s) [{}] — blast radius extends to all consumers where these conditions hold",
        items.len(),
        conditions.join(", ")
    );

    Some(CrossFeatureBlastRadius {
        gated_item_count: items.len(),
        cfg_conditions: conditions,
        gated_symbols,
        summary,
    })
}

// ─── Pub API surface diffing (E13) ─────────────────────────────────────────

/// Diff public API surface between old and new source code.
///
/// Extracts pub symbols from both versions using tree-sitter, then computes the
/// diff (added/removed). Returns a human-readable description of API surface
/// changes, or `None` if:
/// - The language is unsupported
/// - No pub symbols changed (identical API surface)
///
/// This is the primary integration point for `post_edit` Phase 2 — detecting
/// breaking API changes after an edit.
#[must_use]
pub fn diff_pub_api_surface(old_source: &str, new_source: &str, file_path: &str) -> Option<String> {
    let lang = Lang::from_path(Path::new(file_path))?;

    let old_syms = extract_symbols(old_source, lang).ok()?;
    let new_syms = extract_symbols(new_source, lang).ok()?;

    let old_pub = extract_pub_symbols(&old_syms);
    let new_pub = extract_pub_symbols(&new_syms);

    let diff = diff_pub_symbols(&old_pub, &new_pub);
    if diff.added.is_empty() && diff.removed.is_empty() {
        return None;
    }

    Some(format_symbol_diff(&diff))
}

/// Diff pub symbols from a stored JSON snapshot (symbols_json from knowledge DB)
/// against fresh pub symbols extracted from current source.
///
/// The `old_symbols_json` is the `symbols_json` field from `FileKnowledge`,
/// a JSON array of `{"name", "kind", "is_public", "line"}` objects.
///
/// Returns `None` if parsing fails or no pub API changes occurred.
#[must_use]
pub fn diff_pub_api_from_snapshot(
    old_symbols_json: &str,
    new_source: &str,
    file_path: &str,
) -> Option<String> {
    let lang = Lang::from_path(Path::new(file_path))?;
    let new_syms = extract_symbols(new_source, lang).ok()?;
    let new_pub = extract_pub_symbols(&new_syms);

    let old_pub = parse_pub_symbols_from_json(old_symbols_json)?;

    let diff = diff_pub_symbols(&old_pub, &new_pub);
    if diff.added.is_empty() && diff.removed.is_empty() {
        return None;
    }

    Some(format_symbol_diff(&diff))
}

/// Parse pub symbols from the compact JSON format stored in knowledge.db.
///
/// Expects: `[{"name":"foo","kind":"function","is_public":true,"line":1}, ...]`
/// Filters to `is_public == true` and maps `kind` string to `SymbolKind`.
fn parse_pub_symbols_from_json(json: &str) -> Option<Vec<PubSymbol>> {
    let arr: Vec<serde_json::Value> = serde_json::from_str(json).ok()?;
    let mut pub_syms = Vec::new();
    for item in &arr {
        let is_public = item
            .get("is_public")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if !is_public {
            continue;
        }
        let name = item.get("name").and_then(|v| v.as_str()).unwrap_or("");
        if name.is_empty() {
            continue;
        }
        let kind_str = item
            .get("kind")
            .and_then(|v| v.as_str())
            .unwrap_or("function");
        let kind: touring_code::ast::SymbolKind = kind_str
            .parse()
            .unwrap_or(touring_code::ast::SymbolKind::Function);
        let line = item.get("line").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
        pub_syms.push(PubSymbol {
            name: name.to_string(),
            kind,
            line,
        });
    }
    Some(pub_syms)
}

/// Format a `SymbolDiff` into a human-readable string for hook context injection.
fn format_symbol_diff(diff: &SymbolDiff) -> String {
    let mut parts = Vec::new();

    if !diff.added.is_empty() {
        let names: Vec<&str> = diff.added.iter().map(|s| s.name.as_str()).collect();
        parts.push(format!("+pub: {}", names.join(", ")));
    }
    if !diff.removed.is_empty() {
        let names: Vec<&str> = diff.removed.iter().map(|s| s.name.as_str()).collect();
        parts.push(format!("-pub: {}", names.join(", ")));
    }

    format!("API surface changed: {}", parts.join(" | "))
}

// ─── Durability early-exit gate (E14) ──────────────────────────────────────

/// Check if a file path indicates a high-durability (vendor/generated/stdlib)
/// target that should skip expensive hook analysis.
///
/// Uses `touring_code::ast::revision::Durability::for_path()` for classification.
/// High-durability files include vendor code, node_modules, .rustup toolchains,
/// and site-packages — files that rarely change and don't benefit from
/// pre-edit/pre-write analysis.
///
/// Returns `true` if the file should be skipped (high durability).
#[must_use]
pub fn is_high_durability_target(file_path: &str) -> bool {
    let path = Path::new(file_path);
    Durability::for_path(path) == Durability::High
}

#[cfg(test)]
mod tests {
    #![allow(clippy::indexing_slicing, clippy::len_zero)]
    use super::*;

    #[test]
    fn test_enrich_python() {
        let source =
            "def hello():\n    pass\n\nclass Foo:\n    def bar(self):\n        return 42\n";
        let result = enrich_file_knowledge(source, "test.py");
        let (json, count) = result.expect("enrich should succeed");
        assert!(
            count >= 2,
            "Should find at least hello and Foo, got {count}"
        );
        assert!(json.contains("hello"));
        assert!(json.contains("Foo"));
    }

    #[test]
    fn test_enrich_rust() {
        let source = "pub fn main() {}\nstruct Config { x: i32 }\n";
        let result = enrich_file_knowledge(source, "main.rs");
        let (json, count) = result.expect("enrich should succeed");
        assert!(count >= 2);
        assert!(json.contains("main"));
        assert!(json.contains("Config"));
    }

    #[test]
    fn test_enrich_unsupported() {
        // `.txt` has no tree-sitter grammar in `Lang::from_path`, so enrichment
        // must return None. (Previously this used `readme.md`, but Markdown was
        // added to `Lang` — `"md" | "mdx" => Some(Lang::Markdown)` in
        // touring-code/src/ast/languages.rs — so `.md` is now a *supported*
        // language and no longer exercises the unsupported path. Reconciled
        // 2026-05-29 during the elite-harness wave full-suite run.)
        let result = enrich_file_knowledge("hello world", "readme.txt");
        assert!(result.is_none());
    }

    #[test]
    fn test_extract_imports_python() {
        let source = "import os\nfrom pathlib import Path\n";
        let imports = extract_file_imports(source, "test.py");
        assert!(!imports.is_empty());
    }

    #[test]
    fn test_validate_syntax_valid() {
        let result = validate_file_syntax("def foo():\n    pass\n", "test.py");
        assert_eq!(result, Some(true));
    }

    #[test]
    fn test_validate_syntax_invalid() {
        let result = validate_file_syntax("def foo( \n", "test.py");
        assert_ne!(
            result,
            Some(true),
            "Invalid syntax should not report as valid"
        );
    }

    // ── ABI-mismatch false-positive regression (grammar-unavailable) ─────

    /// `tree-sitter-bash` is pinned to an ABI-14 crate, so a valid shell
    /// script now validates as `Some(true)`. Before the pin the ABI-15
    /// grammar failed to load and `validate_syntax` returned `Err`, which the
    /// old `.unwrap_or(false)` collapsed to `Some(false)` — a false syntax
    /// error on every `.sh` / `.bash` edit.
    #[test]
    fn test_validate_file_syntax_bash_is_valid() {
        let result =
            validate_file_syntax("echo hello\nif [ -f x ]; then echo yes; fi\n", "deploy.sh");
        assert_eq!(
            result,
            Some(true),
            "valid bash must validate as Some(true), not a false syntax error"
        );
    }

    /// A grammar that cannot be loaded (markdown is on an ABI-15 crate the
    /// 0.24 runtime rejects) must never be reported as a syntax error. The
    /// result is `None` ("undetermined") or `Some(true)` — never `Some(false)`.
    /// The assertion only forbids `Some(false)`, so it stays correct if the
    /// markdown grammar is later realigned to ABI 14.
    #[test]
    fn test_validate_file_syntax_markdown_never_false() {
        let result = validate_file_syntax("# Title\n\nSome **markdown** text.\n", "README.md");
        assert_ne!(
            result,
            Some(false),
            "an unloadable grammar must not masquerade as a syntax error"
        );
    }

    /// Editing a markdown file must NOT surface a false "SYNTAX ERROR" in the
    /// edit-impact summary just because the markdown grammar cannot be loaded.
    /// This is the regression guard for the reported false positive.
    #[test]
    fn test_validate_edit_impact_no_false_syntax_error_for_markdown() {
        let old = "# Title\n\nfirst paragraph\n";
        let new = "# Title\n\nfirst paragraph\n\nsecond paragraph\n";
        let result = validate_edit_impact(old, new, "README.md", None, 10);
        let impact = result.expect("markdown edit must return Some");
        assert!(
            impact.syntax_valid,
            "markdown (grammar unavailable) must not be flagged syntactically invalid"
        );
        assert!(
            !impact.summary.contains("SYNTAX ERROR"),
            "summary must not contain a false 'SYNTAX ERROR'; got: {}",
            impact.summary
        );
    }

    #[test]
    fn test_build_enriched_knowledge() {
        let source = "def hello():\n    pass\n";
        let knowledge = build_enriched_knowledge("test.py", source);
        assert_eq!(knowledge.language.as_deref(), Some("python"));
        assert_eq!(knowledge.line_count, 2);
        assert!(knowledge.symbol_count >= 1);
        assert!(knowledge.content_hash.is_some());
    }

    #[test]
    fn test_blast_radius_empty_index() {
        let index = SymbolIndex::new();
        let result = compute_blast_radius(&index, "nonexistent.py");
        assert!(result.is_none());
    }

    // ── New enriched tests ──────────────────────────────────────────────

    #[test]
    fn test_extract_enriched_symbols() {
        let source = r#"
def simple():
    return 1

async def fetch():
    await something()

class Handler:
    def process(self):
        if True:
            for i in range(10):
                pass
"#;
        let symbols = extract_enriched_symbols(source, "test.py");
        let symbols = symbols.expect("extract symbols should succeed");

        // Check async detection
        let fetch = symbols.iter().find(|s| s.name == "fetch");
        let fetch = fetch.expect("Should find fetch");
        assert!(fetch.is_async, "fetch should be async");

        // Check complexity is populated
        let process = symbols.iter().find(|s| s.name == "process");
        if let Some(p) = process {
            let cc = p.complexity.expect("process should have complexity");
            assert!(cc >= 2, "process has branches: CC >= 2");
        }
    }

    #[test]
    fn test_analyze_file_quality() {
        let source = r#"
def simple():
    return 1

def complex(x, y):
    if x > 0:
        for i in range(x):
            if y > i:
                while True:
                    if x and y:
                        break
    return False
"#;
        let metrics = analyze_file_quality(source, "test.py");
        let metrics = metrics.expect("quality metrics should succeed");

        assert!(metrics.callable_count >= 2);
        assert!(metrics.max_complexity >= 4, "complex() should have high CC");
        assert!(metrics.avg_complexity > 1.0);
    }

    #[test]
    fn test_quality_summary() {
        let metrics = FileQualityMetrics {
            symbol_count: 5,
            callable_count: 3,
            type_count: 1,
            max_complexity: 15,
            avg_complexity: 6.5,
            high_complexity_count: 1,
            async_count: 1,
            async_ratio: 0.333,
            has_nesting: true,
            complex_symbols: vec!["complex(CC=15)".into()],
        };

        let summary = quality_summary(&metrics);
        assert!(summary.contains("5 symbols"));
        assert!(summary.contains("CC: max=15"));
        assert!(summary.contains("HIGH CC"));
        assert!(summary.contains("async"));
    }

    #[test]
    fn test_validate_edit_impact() {
        let old = "def foo():\n    return 1\n";
        let new = "def foo():\n    if True:\n        for i in range(10):\n            return i\n    return 0\n";

        let result = validate_edit_impact(old, new, "test.py", None, 10);
        let result = result.expect("validate should succeed");

        assert!(result.syntax_valid);
        assert_eq!(result.affected_files, 1); // No index provided
        assert!(
            !result.complexity_changes.is_empty(),
            "Should detect CC change"
        );
    }

    #[test]
    fn test_validate_edit_syntax_error() {
        let old = "def foo():\n    pass\n";
        let new = "def foo(\n    pass\n"; // syntax error

        let result = validate_edit_impact(old, new, "test.py", None, 10);
        let result = result.expect("validate should succeed");

        // tree-sitter may still parse with error nodes
        // The summary should indicate an issue
        assert!(result.summary.len() > 0);
    }

    #[test]
    fn test_check_symbol_complexity() {
        let source = r#"
def simple():
    return 1

def complex(x):
    if x > 0:
        for i in range(x):
            if i > 5:
                return i
    return 0
"#;
        let result = check_symbol_complexity(source, "test.py", "simple", 10);
        let (cc, violation) = result.expect("complexity check should succeed");
        assert_eq!(cc, 1, "simple() should have CC=1");
        assert!(!violation);

        let result = check_symbol_complexity(source, "test.py", "complex", 2);
        let (cc, violation) = result.expect("complexity check should succeed");
        assert!(cc >= 3, "complex() CC >= 3, got {}", cc);
        assert!(violation, "CC {} exceeds threshold 2", cc);
    }

    #[test]
    fn test_enriched_knowledge_with_quality() {
        let source = r#"
def simple():
    return 1
"#;
        let knowledge = build_enriched_knowledge_with_quality("test.py", source);
        assert_eq!(knowledge.language.as_deref(), Some("python"));
        // Simple function — no high complexity note
        assert!(
            knowledge.notes.is_none()
                || !knowledge
                    .notes
                    .as_deref()
                    .unwrap_or("")
                    .contains("High complexity")
        );
    }

    // ── E13: diff_pub_api_surface tests ────────────────────────────────

    #[test]
    fn test_diff_pub_api_surface_detects_added() {
        let old_source = "pub fn foo() {}\n";
        let new_source = "pub fn foo() {}\npub fn bar() {}\n";
        let result = diff_pub_api_surface(old_source, new_source, "test.rs");
        let msg = result.expect("should detect added pub symbol");
        assert!(msg.contains("+pub"), "should mention added: {msg}");
        assert!(msg.contains("bar"), "should mention bar: {msg}");
    }

    #[test]
    fn test_diff_pub_api_surface_detects_removed() {
        let old_source = "pub fn foo() {}\npub fn bar() {}\n";
        let new_source = "pub fn foo() {}\n";
        let result = diff_pub_api_surface(old_source, new_source, "test.rs");
        let msg = result.expect("should detect removed pub symbol");
        assert!(msg.contains("-pub"), "should mention removed: {msg}");
        assert!(msg.contains("bar"), "should mention bar: {msg}");
    }

    #[test]
    fn test_diff_pub_api_surface_no_change() {
        let source = "pub fn foo() {}\n";
        let result = diff_pub_api_surface(source, source, "test.rs");
        assert!(result.is_none(), "no API change should return None");
    }

    #[test]
    fn test_diff_pub_api_surface_unsupported_lang() {
        let result = diff_pub_api_surface("hello", "world", "readme.md");
        assert!(result.is_none(), "unsupported language should return None");
    }

    #[test]
    fn test_diff_pub_api_from_snapshot_added() {
        let old_json = r#"[{"name":"foo","kind":"function","is_public":true,"line":1}]"#;
        let new_source = "pub fn foo() {}\npub fn bar() {}\n";
        let result = diff_pub_api_from_snapshot(old_json, new_source, "test.rs");
        let msg = result.expect("should detect added pub symbol");
        assert!(msg.contains("+pub"), "should mention added: {msg}");
        assert!(msg.contains("bar"), "should mention bar: {msg}");
    }

    #[test]
    fn test_diff_pub_api_from_snapshot_no_change() {
        let old_json = r#"[{"name":"foo","kind":"function","is_public":true,"line":1}]"#;
        let new_source = "pub fn foo() {}\n";
        let result = diff_pub_api_from_snapshot(old_json, new_source, "test.rs");
        assert!(result.is_none(), "identical API should return None");
    }

    #[test]
    fn test_diff_pub_api_from_snapshot_invalid_json() {
        let result = diff_pub_api_from_snapshot("not json", "pub fn foo() {}", "test.rs");
        assert!(result.is_none(), "invalid JSON should return None");
    }

    #[test]
    fn test_parse_pub_symbols_from_json() {
        let json = r#"[
            {"name":"foo","kind":"function","is_public":true,"line":1},
            {"name":"Bar","kind":"struct","is_public":true,"line":10},
            {"name":"priv_fn","kind":"function","is_public":false,"line":20}
        ]"#;
        let syms = parse_pub_symbols_from_json(json).expect("should parse");
        assert_eq!(syms.len(), 2, "should only include public symbols");
        assert_eq!(syms[0].name, "foo");
        assert_eq!(syms[1].name, "Bar");
    }

    #[test]
    fn test_format_symbol_diff_both() {
        let diff = SymbolDiff {
            added: vec![PubSymbol {
                name: "new_fn".into(),
                kind: touring_code::ast::SymbolKind::Function,
                line: 1,
            }],
            removed: vec![PubSymbol {
                name: "old_fn".into(),
                kind: touring_code::ast::SymbolKind::Function,
                line: 1,
            }],
            unchanged: vec![],
        };
        let msg = format_symbol_diff(&diff);
        assert!(msg.contains("+pub: new_fn"), "msg: {msg}");
        assert!(msg.contains("-pub: old_fn"), "msg: {msg}");
    }

    // ── E14: is_high_durability_target tests ───────────────────────────

    #[test]
    fn test_high_durability_vendor() {
        assert!(is_high_durability_target("node_modules/react/index.js"));
        assert!(is_high_durability_target(
            "/home/user/.rustup/toolchains/stable/lib.rs"
        ));
        assert!(is_high_durability_target("/path/to/vendor/lib.py"));
        assert!(is_high_durability_target(
            "/env/lib/python3.11/site-packages/requests/api.py"
        ));
    }

    #[test]
    fn test_low_durability_user_source() {
        assert!(!is_high_durability_target("src/main.rs"));
        assert!(!is_high_durability_target("lib/utils.py"));
        assert!(!is_high_durability_target("index.ts"));
    }

    #[test]
    fn test_medium_durability_config() {
        // Config files are Medium durability, not High — should NOT be skipped
        assert!(!is_high_durability_target("Cargo.toml"));
        assert!(!is_high_durability_target("package.json"));
        assert!(!is_high_durability_target("tsconfig.json"));
    }
}
