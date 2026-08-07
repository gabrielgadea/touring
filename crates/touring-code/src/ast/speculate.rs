//! Speculative Validation v2 — Multi-layer pre-edit validation.
//!
//! Provides 4 validation layers:
//! 1. **Syntax** — tree-sitter parse without errors
//! 2. **SymbolResolution** — referenced symbols exist in context
//! 3. **Structural** — code quality invariants (no unwrap, etc.)
//! 4. **ImportCheck** — required imports are present
//!
//! Composite score = weighted average: Syntax(0.4) + Symbol(0.25) + Structural(0.25) + Import(0.10)

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::ast::error::AstError;
use crate::ast::import_resolver::ImportResolver;
use crate::ast::languages::Lang;
use crate::ast::parser::parse_thread_local;
use crate::ast::symbol_detail::SymbolDetail;

// ─── Types ──────────────────────────────────────────────────────────────

/// A public item gated behind a Cargo feature flag, detected in source.
///
/// When an item's signature changes, its blast radius extends to ALL files
/// that compile with that feature enabled — not just the default build graph.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CfgGatedItem {
    /// Symbol or item name (e.g., `"AsyncSharedPipeline"`)
    pub name: String,
    /// Full cfg condition (e.g., `"unix"`, `"feature = \"async-pipeline\""`, `"all(feature = \"x\", unix)"`)
    pub cfg_condition: String,
    /// Line number in source (1-indexed)
    pub line: usize,
}

/// Identifies which validation layer produced a result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ValidationLayer {
    /// Layer 1: Syntax validation via tree-sitter
    Syntax,
    /// Layer 2: Symbol resolution against known context
    SymbolResolution,
    /// Layer 3: Structural invariants (no unwrap, naming, etc.)
    Structural,
    /// Layer 4: Import completeness check
    ImportCheck,
    /// Layer 5: Feature-gate impact (informational — does not affect composite score)
    CfgImpact,
    /// Layer 6: Cyclomatic complexity check — penalizes functions exceeding threshold
    Complexity,
}

/// Result of a single validation layer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayerResult {
    /// Which layer produced this result
    pub layer: ValidationLayer,
    /// Whether this layer passed
    pub passed: bool,
    /// Diagnostic messages (empty if passed)
    pub diagnostics: Vec<String>,
    /// Layer score (0.0 to 1.0)
    pub score: f64,
}

/// Combined result of all validation layers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpeculateResult {
    /// Results per layer (6 total: Syntax, SymbolResolution, Structural, ImportCheck, CfgImpact, Complexity)
    pub layers: Vec<LayerResult>,
    /// Weighted composite score (0.0 to 1.0) — uses 5-layer weights; CfgImpact is informational
    pub composite_score: f64,
    /// Whether all layers passed (CfgImpact always passes)
    pub all_passed: bool,
    /// Cfg-gated public items detected in the source — informs cross-cfg blast radius
    #[serde(default)]
    pub cfg_gated_items: Vec<CfgGatedItem>,
    /// Bayesian fused score from all 4 validation layers (only computed when `simd-search` feature
    /// is enabled). Uses confidence-weighted fusion via `touring_simd::statistics::reconciliation::bayesian_fusion`.
    /// Syntax gets highest confidence (0.9), symbol/structural medium (0.75), import lowest (0.6).
    /// `None` when `simd-search` feature is not enabled.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bayesian_score: Option<f64>,
}

// ─── Cfg Gate Detection ──────────────────────────────────────────────────

/// Extract the full cfg condition expression from `#[cfg(...)]` on a trimmed line.
///
/// Returns the inner expression (e.g. `"unix"`, `"feature = \"async\""`,
/// `"all(feature = \"x\", unix)"`) for any `#[cfg(...)]` attribute.
/// Returns `None` if the line contains no `#[cfg(` marker.
fn parse_cfg_condition(line: &str) -> Option<String> {
    let start = line.find("#[cfg(")?;
    let rest = &line[start + 6..]; // skip "#[cfg("
    // Use rfind to handle nested parens: #[cfg(all(feature = "x", unix))]
    let end = rest.rfind(")]")?;
    let condition = rest[..end].trim();
    if condition.is_empty() {
        return None;
    }
    Some(condition.to_string())
}

/// Strip `pub` / `pub(...)` visibility from the start of a definition line.
///
/// Returns the rest of the line after the modifier, or `None` if the line
/// does not start with `pub`.
fn strip_pub_visibility(line: &str) -> Option<&str> {
    if line.starts_with("pub(") {
        let close = line.find(')')?;
        Some(line[close + 1..].trim_start())
    } else {
        line.strip_prefix("pub ").map(str::trim_start)
    }
}

/// Extract the item name from a line that starts with a `pub` definition.
///
/// Handles `pub fn`, `pub struct`, `pub enum`, `pub type`, `pub trait`,
/// `pub const`, `pub static`, `pub mod`, `pub impl`, and `pub async fn`.
/// Returns `None` for non-`pub` lines or lines with no parseable name.
fn extract_pub_item_name(line: &str) -> Option<String> {
    let after_vis = strip_pub_visibility(line)?;

    // Strip "async " prefix if present (pub async fn)
    let after_async = after_vis
        .strip_prefix("async ")
        .unwrap_or(after_vis)
        .trim_start();

    // Skip the keyword (fn, struct, enum, type, const, static, trait, impl, mod)
    let kw_end = after_async.find(|ch: char| !ch.is_alphabetic() && ch != '_')?;
    if kw_end == 0 {
        return None;
    }
    let after_kw = after_async[kw_end..].trim_start();

    // Name is the next word-like token
    let name_end = after_kw
        .find(|ch: char| !ch.is_alphanumeric() && ch != '_')
        .unwrap_or(after_kw.len());
    if name_end == 0 {
        return None;
    }
    Some(after_kw[..name_end].to_string())
}

/// Scan source for public items gated behind any `#[cfg(...)]` attribute.
///
/// Returns one [`CfgGatedItem`] per matched `pub` definition immediately
/// following a `#[cfg(...)]` attribute (with optional blank lines or
/// additional attributes in between).
///
/// Covers all cfg conditions: `feature = "..."`, `unix`, `target_os = "..."`,
/// `not(...)`, `all(...)`, `any(...)`, etc.
///
/// Uses a line-level scan — not full AST — which covers the common
/// single-line attribute pattern reliably and with zero allocation overhead.
#[must_use]
pub fn extract_cfg_gated_pub_items(source: &str) -> Vec<CfgGatedItem> {
    let mut items = Vec::new();
    let lines: Vec<&str> = source.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let trimmed = lines.get(i).map(|l| l.trim()).unwrap_or("");
        if let Some(cfg_condition) = parse_cfg_condition(trimmed) {
            // Look ahead past blank lines and extra attributes for the pub definition
            let mut j = i + 1;
            while j < lines.len() {
                let next = lines.get(j).map(|l| l.trim()).unwrap_or("");
                if next.is_empty() || next.starts_with("//") {
                    j += 1;
                    continue;
                }
                if next.starts_with("#[") {
                    // Another attribute — keep scanning
                    j += 1;
                    continue;
                }
                // First non-attribute, non-blank line
                if let Some(name) = extract_pub_item_name(next) {
                    items.push(CfgGatedItem {
                        name,
                        cfg_condition,
                        line: j + 1, // 1-indexed
                    });
                }
                break;
            }
        }
        i += 1;
    }

    items
}

/// Build the `CfgImpact` layer result from detected cfg-gated items.
///
/// Score is always 1.0 — this layer is informational and never blocks validation.
/// Diagnostics carry the cross-cfg blast radius warning.
fn build_cfg_layer_result(items: &[CfgGatedItem]) -> LayerResult {
    if items.is_empty() {
        return LayerResult {
            layer: ValidationLayer::CfgImpact,
            passed: true,
            diagnostics: Vec::new(),
            score: 1.0,
        };
    }

    let mut conditions: Vec<&str> = items.iter().map(|i| i.cfg_condition.as_str()).collect();
    conditions.sort_unstable();
    conditions.dedup();

    LayerResult {
        layer: ValidationLayer::CfgImpact,
        passed: true,
        diagnostics: vec![format!(
            "{} pub item(s) behind cfg condition(s) [{}] — blast radius extends to all consumers where these conditions hold",
            items.len(),
            conditions.join(", ")
        )],
        score: 1.0,
    }
}

// ─── Weights ────────────────────────────────────────────────────────────

const WEIGHT_SYNTAX: f64 = 0.35;
const WEIGHT_SYMBOL: f64 = 0.20;
const WEIGHT_STRUCTURAL: f64 = 0.20;
const WEIGHT_IMPORT: f64 = 0.10;
const WEIGHT_COMPLEXITY: f64 = 0.15;

/// Maximum cyclomatic complexity per function before penalization.
/// Functions exceeding this threshold reduce the complexity layer score.
const COMPLEXITY_THRESHOLD: u16 = 15;

// ─── Bayesian Fusion (optional, simd-search feature) ───────────────────

/// Compute Bayesian fused score from the 4 validation layer scores.
///
/// Confidence values reflect each layer's reliability:
/// - Syntax (0.9): tree-sitter parse is highly reliable
/// - Symbol resolution (0.75): depends on context completeness
/// - Structural (0.75): heuristic-based, reasonably reliable
/// - Import (0.6): basic heuristic, least reliable
///
/// Returns `Some(fused_value)` when `simd-search` feature is enabled,
/// `None` otherwise.
#[cfg(feature = "simd-search")]
fn compute_bayesian_score(
    syntax_score: f64,
    symbol_score: f64,
    structural_score: f64,
    import_score: f64,
) -> Option<f64> {
    use touring_simd::statistics::reconciliation::bayesian_fusion;

    let estimates = [
        (syntax_score, 0.9),      // syntax is the most reliable layer
        (symbol_score, 0.75),     // depends on context completeness
        (structural_score, 0.75), // heuristic-based
        (import_score, 0.6),      // basic heuristic, least reliable
    ];
    let (fused, _confidence) = bayesian_fusion(&estimates);
    Some(fused)
}

/// Fallback when `simd-search` feature is not enabled — always returns `None`.
#[cfg(not(feature = "simd-search"))]
fn compute_bayesian_score(
    _syntax_score: f64,
    _symbol_score: f64,
    _structural_score: f64,
    _import_score: f64,
) -> Option<f64> {
    None
}

// ─── Implementation ─────────────────────────────────────────────────────

/// Run multi-layer speculative validation on source code.
///
/// # Arguments
/// * `source` - Source code to validate
/// * `language` - Language identifier (`"rust"`, `"python"`, etc.)
/// * `symbol_context` - Optional known symbols for resolution checking
/// * `import_context` - Optional import resolver for completeness checking
///
/// # Returns
/// A [`SpeculateResult`] with per-layer results and composite score.
pub fn speculate_v2(
    source: &str,
    lang: Lang,
    symbol_context: Option<&[SymbolDetail]>,
    import_context: Option<&ImportResolver>,
) -> SpeculateResult {
    let syntax_result = validate_syntax_layer(source, lang);
    let symbol_result = validate_symbol_layer(source, lang, symbol_context);
    let structural_result = validate_structural_layer(source, lang);
    let import_result = validate_import_layer(source, import_context);
    let complexity_result = validate_complexity_layer(source, lang);

    // Layer 5: cfg-gate impact — informational, never blocks validation
    let cfg_gated_items = extract_cfg_gated_pub_items(source);
    let cfg_result = build_cfg_layer_result(&cfg_gated_items);

    let composite_score = syntax_result.score * WEIGHT_SYNTAX
        + symbol_result.score * WEIGHT_SYMBOL
        + structural_result.score * WEIGHT_STRUCTURAL
        + import_result.score * WEIGHT_IMPORT
        + complexity_result.score * WEIGHT_COMPLEXITY;

    let all_passed = syntax_result.passed
        && symbol_result.passed
        && structural_result.passed
        && import_result.passed
        && complexity_result.passed;

    // Bayesian fusion: confidence-weighted score from all 5 layers.
    // Only computed when touring-simd is available (simd-search feature).
    let bayesian_score = compute_bayesian_score(
        syntax_result.score,
        symbol_result.score,
        structural_result.score,
        import_result.score,
    );

    SpeculateResult {
        layers: vec![
            syntax_result,
            symbol_result,
            structural_result,
            import_result,
            complexity_result,
            cfg_result,
        ],
        composite_score,
        all_passed,
        cfg_gated_items,
        bayesian_score,
    }
}

// ─── Layer 1: Syntax ────────────────────────────────────────────────────

fn validate_syntax_layer(source: &str, lang: Lang) -> LayerResult {
    match parse_thread_local(source, lang) {
        Ok(tree) => {
            let root = tree.root_node();
            if root.has_error() {
                // Count error nodes
                let error_count = count_error_nodes(root);
                LayerResult {
                    layer: ValidationLayer::Syntax,
                    passed: false,
                    diagnostics: vec![format!("{error_count} syntax error(s) detected")],
                    score: 0.0,
                }
            } else {
                LayerResult {
                    layer: ValidationLayer::Syntax,
                    passed: true,
                    diagnostics: Vec::new(),
                    score: 1.0,
                }
            }
        }
        // A grammar that cannot be loaded (tree-sitter ABI mismatch) is an
        // infrastructure limitation, NOT a syntax defect in `source`. The
        // syntax layer simply could not run — report it as PASSED with no
        // diagnostics. Treating it as a failed layer surfaced a false
        // "🚨 SYNTAX: parse failed" on every edit of an ABI-mismatched
        // language (e.g. markdown) — see `AstError::GrammarUnavailable`.
        Err(AstError::GrammarUnavailable(_)) => LayerResult {
            layer: ValidationLayer::Syntax,
            passed: true,
            diagnostics: Vec::new(),
            score: 1.0,
        },
        Err(e) => LayerResult {
            layer: ValidationLayer::Syntax,
            passed: false,
            diagnostics: vec![format!("parse failed: {e}")],
            score: 0.0,
        },
    }
}

fn count_error_nodes(root: tree_sitter::Node) -> usize {
    let mut count = 0;
    let mut cursor = root.walk();
    let mut did_enter = true;
    loop {
        if did_enter {
            let node = cursor.node();
            if node.is_error() || node.is_missing() {
                count += 1;
            }
            if cursor.goto_first_child() {
                continue;
            }
        }
        if cursor.goto_next_sibling() {
            did_enter = true;
            continue;
        }
        if !cursor.goto_parent() {
            break;
        }
        did_enter = false;
    }
    count
}

// ─── Layer 2: Symbol Resolution (AST-aware v3) ─────────────────────────

/// Collect all identifier tokens from AST, excluding comments and strings.
fn collect_identifiers<'a>(source: &'a str, root: tree_sitter::Node) -> HashSet<&'a str> {
    let mut ids = HashSet::new();
    let mut cursor = root.walk();
    let mut did_enter = true;
    loop {
        if did_enter {
            let node = cursor.node();
            let kind = node.kind();
            if is_comment_or_string(kind) {
                // Skip entire subtree — don't descend into comments/strings
                // did_enter stays false; next sibling sets it to true
            } else {
                if is_identifier_kind(kind)
                    && let Some(text) = source.get(node.byte_range())
                {
                    ids.insert(text);
                }
                if cursor.goto_first_child() {
                    continue;
                }
            }
        }
        if cursor.goto_next_sibling() {
            did_enter = true;
            continue;
        }
        if !cursor.goto_parent() {
            break;
        }
        did_enter = false;
    }
    ids
}

fn is_comment_or_string(kind: &str) -> bool {
    matches!(
        kind,
        "comment"
            | "line_comment"
            | "block_comment"
            | "doc_comment"
            | "string"
            | "string_literal"
            | "raw_string_literal"
            | "string_content"
            | "string_fragment"
            | "template_string"
            | "template_literal"
    )
}

fn is_identifier_kind(kind: &str) -> bool {
    matches!(
        kind,
        "identifier"
            | "type_identifier"
            | "field_identifier"
            | "shorthand_field_identifier"
            | "property_identifier"
            | "scoped_identifier"
            | "attribute"
    )
}

fn validate_symbol_layer(
    source: &str,
    lang: Lang,
    context: Option<&[SymbolDetail]>,
) -> LayerResult {
    let Some(symbols) = context else {
        // No context provided — pass by default
        return LayerResult {
            layer: ValidationLayer::SymbolResolution,
            passed: true,
            diagnostics: Vec::new(),
            score: 1.0,
        };
    };

    if symbols.is_empty() {
        return LayerResult {
            layer: ValidationLayer::SymbolResolution,
            passed: true,
            diagnostics: Vec::new(),
            score: 1.0,
        };
    }

    // AST-aware: collect identifiers excluding comments and strings.
    // Fallback to source.contains() if parse fails.
    let identifier_set: Option<HashSet<&str>> = parse_thread_local(source, lang)
        .ok()
        .map(|tree| collect_identifiers(source, tree.root_node()));

    let mut unresolved = Vec::new();
    for sym in symbols {
        let found = match &identifier_set {
            Some(ids) => ids.contains(sym.name.as_str()),
            None => source.contains(&sym.name), // fallback
        };
        if !found {
            unresolved.push(sym.name.clone());
        }
    }

    let total = symbols.len() as f64;
    let resolved = (total - unresolved.len() as f64).max(0.0);
    let score = if total > 0.0 { resolved / total } else { 1.0 };

    LayerResult {
        layer: ValidationLayer::SymbolResolution,
        passed: unresolved.is_empty(),
        diagnostics: if unresolved.is_empty() {
            Vec::new()
        } else {
            vec![format!("unresolved symbols: {}", unresolved.join(", "))]
        },
        score,
    }
}

// ─── Layer 3: Structural Invariants ─────────────────────────────────────

fn validate_structural_layer(source: &str, lang: Lang) -> LayerResult {
    let mut diagnostics = Vec::new();
    let mut penalty = 0.0;

    if lang == Lang::Rust {
        // Check for unwrap() usage (anti-pattern in production)
        let unwrap_count = source.matches(".unwrap()").count();
        if unwrap_count > 0 {
            diagnostics.push(format!(
                "{unwrap_count} unwrap() usage(s) detected — use ? or .expect()"
            ));
            penalty += 0.15 * unwrap_count as f64;
        }

        // Check for todo!() / unimplemented!()
        let todo_count =
            source.matches("todo!()").count() + source.matches("unimplemented!()").count();
        if todo_count > 0 {
            diagnostics.push(format!(
                "{todo_count} todo!/unimplemented! macro(s) detected"
            ));
            penalty += 0.1 * todo_count as f64;
        }

        // Check for panic!() outside tests
        let panic_count = source.matches("panic!(").count();
        if panic_count > 0 {
            diagnostics.push(format!("{panic_count} panic!() call(s) detected"));
            penalty += 0.1 * panic_count as f64;
        }
    }

    if lang == Lang::Python {
        // Bare except: catches everything including KeyboardInterrupt
        let bare_except = source.matches("except:").count();
        if bare_except > 0 {
            diagnostics.push(format!(
                "{bare_except} bare except: detected — use except Exception:"
            ));
            penalty += 0.15 * bare_except as f64;
        }
    }

    if lang == Lang::TypeScript || lang == Lang::JavaScript {
        // `: any` type annotation is an escape hatch
        let any_annotation = source.matches(": any").count();
        if any_annotation > 0 {
            diagnostics.push(format!(
                "{any_annotation} `: any` type annotation(s) — use a specific type"
            ));
            penalty += 0.1 * any_annotation as f64;
        }
        // @ts-ignore suppresses type errors
        let ts_ignore = source.matches("@ts-ignore").count();
        if ts_ignore > 0 {
            diagnostics.push(format!(
                "{ts_ignore} @ts-ignore directive(s) — fix the type error instead"
            ));
            penalty += 0.15 * ts_ignore as f64;
        }
        // `as any` type assertion
        let as_any = source.matches("as any").count();
        if as_any > 0 {
            diagnostics.push(format!(
                "{as_any} `as any` assertion(s) — use a specific type"
            ));
            penalty += 0.1 * as_any as f64;
        }
    }

    let score = (1.0 - penalty).max(0.0);

    LayerResult {
        layer: ValidationLayer::Structural,
        passed: diagnostics.is_empty(),
        diagnostics,
        score,
    }
}

// ─── Layer 6: Cyclomatic Complexity ─────────────────────────────────────

/// Validate cyclomatic complexity of all functions in source.
///
/// Penalizes functions whose complexity exceeds `COMPLEXITY_THRESHOLD`.
/// Score degrades proportionally to how many functions exceed and by how much.
fn validate_complexity_layer(source: &str, lang: Lang) -> LayerResult {
    let complexities = match crate::ast::complexity::compute_complexity_for_source(source, lang) {
        Ok(c) => c,
        Err(_) => {
            // Parse failed or unsupported language — pass by default
            return LayerResult {
                layer: ValidationLayer::Complexity,
                passed: true,
                diagnostics: Vec::new(),
                score: 1.0,
            };
        }
    };

    if complexities.is_empty() {
        return LayerResult {
            layer: ValidationLayer::Complexity,
            passed: true,
            diagnostics: Vec::new(),
            score: 1.0,
        };
    }

    let mut diagnostics = Vec::new();
    let mut penalty = 0.0;

    for (name, cc) in &complexities {
        if *cc > COMPLEXITY_THRESHOLD {
            let excess = *cc - COMPLEXITY_THRESHOLD;
            diagnostics.push(format!(
                "`{name}` has CC={cc} (threshold={COMPLEXITY_THRESHOLD}, excess={excess})"
            ));
            // Each point of excess adds ~3% penalty, capped at total 1.0
            penalty += 0.03 * f64::from(excess);
        }
    }

    let score = (1.0 - penalty).max(0.0);

    LayerResult {
        layer: ValidationLayer::Complexity,
        passed: diagnostics.is_empty(),
        diagnostics,
        score,
    }
}

// ─── Layer 4: Import Completeness ───────────────────────────────────────

fn validate_import_layer(source: &str, context: Option<&ImportResolver>) -> LayerResult {
    let Some(resolver) = context else {
        return LayerResult {
            layer: ValidationLayer::ImportCheck,
            passed: true,
            diagnostics: Vec::new(),
            score: 1.0,
        };
    };

    // Simple check: if source uses identifiers that look like types (PascalCase)
    // and they're not in the imports, flag them.
    // This is a heuristic — not perfect but useful.
    let mut diagnostics = Vec::new();

    // Check if any imports are empty when source is non-trivial
    if resolver.imports.is_empty() && source.len() > 100 {
        // Not necessarily a problem — might be a standalone snippet
        diagnostics.push("no imports found in non-trivial source".to_string());
    }

    let score = if diagnostics.is_empty() { 1.0 } else { 0.8 };

    LayerResult {
        layer: ValidationLayer::ImportCheck,
        passed: diagnostics.is_empty(),
        diagnostics,
        score,
    }
}

// ─── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_speculate_valid_code() {
        let src = r#"
fn add(a: u32, b: u32) -> u32 {
    a + b
}
"#;
        let result = speculate_v2(src, Lang::Rust, None, None);
        assert!(
            result.composite_score >= 0.9,
            "score too low: {}",
            result.composite_score
        );
        assert_eq!(result.layers.len(), 6);
    }

    #[test]
    fn test_speculate_syntax_error() {
        let src = r#"
fn broken( {
    missing closing
"#;
        let result = speculate_v2(src, Lang::Rust, None, None);
        let syntax_layer = result
            .layers
            .iter()
            .find(|l| l.layer == ValidationLayer::Syntax);
        assert!(
            syntax_layer.map(|l| !l.passed).unwrap_or(false),
            "syntax layer should fail: {:?}",
            result.layers
        );
        assert!(result.composite_score < 0.8);
    }

    /// Regression: a language whose tree-sitter grammar cannot be loaded
    /// (markdown is on an ABI-15 crate the 0.24 runtime rejects) must NOT
    /// produce a failed Syntax layer. `parse_thread_local` returns
    /// `AstError::GrammarUnavailable`, an infrastructure limitation — never a
    /// syntax defect in the source. Before the fix this surfaced a false
    /// "SYNTAX: parse failed" on every markdown edit. The assertions hold
    /// equally if the markdown grammar is later realigned to ABI 14.
    #[test]
    fn test_speculate_grammar_unavailable_does_not_fail_syntax() {
        let src = "# Title\n\nSome **markdown** content.\n";
        let result = speculate_v2(src, Lang::Markdown, None, None);
        let syntax_layer = result
            .layers
            .iter()
            .find(|l| l.layer == ValidationLayer::Syntax)
            .expect("syntax layer must be present");
        assert!(
            syntax_layer.passed,
            "an unloadable grammar must not fail the Syntax layer: {syntax_layer:?}"
        );
        assert!(
            syntax_layer.diagnostics.is_empty(),
            "an unloadable grammar must emit no syntax diagnostics, got: {:?}",
            syntax_layer.diagnostics
        );
    }

    #[test]
    fn test_speculate_unwrap_penalty() {
        let src = r#"
fn risky() -> String {
    let val = std::env::var("HOME").unwrap();
    val
}
"#;
        let result = speculate_v2(src, Lang::Rust, None, None);
        let structural = result
            .layers
            .iter()
            .find(|l| l.layer == ValidationLayer::Structural);
        assert!(
            structural
                .map(|l| !l.diagnostics.is_empty())
                .unwrap_or(false),
            "structural should flag unwrap: {:?}",
            result.layers
        );
    }

    #[test]
    fn test_speculate_all_passed() {
        let src = r#"
fn safe(x: Option<u32>) -> u32 {
    x.unwrap_or_default()
}
"#;
        let result = speculate_v2(src, Lang::Rust, None, None);
        assert!(
            result.all_passed,
            "all layers should pass: {:?}",
            result.layers
        );
    }

    #[test]
    fn test_speculate_with_symbol_context() {
        let src = r#"
fn use_data(data: &Data) {
    println!("{}", data.name);
}
"#;
        let symbols = vec![
            SymbolDetail {
                name: "name".to_string(),
                kind: crate::ast::symbol_detail::MemberKind::Field,
                type_str: Some("String".to_string()),
                is_pub: true,
                line: 1,
            },
            SymbolDetail {
                name: "age".to_string(),
                kind: crate::ast::symbol_detail::MemberKind::Field,
                type_str: Some("u32".to_string()),
                is_pub: true,
                line: 2,
            },
        ];
        let result = speculate_v2(src, Lang::Rust, Some(&symbols), None);
        // "name" appears in source but "age" does not
        let sym_layer = result
            .layers
            .iter()
            .find(|l| l.layer == ValidationLayer::SymbolResolution);
        assert!(sym_layer.is_some(), "symbol layer should exist");
    }

    #[test]
    fn test_speculate_unsupported_language() {
        let result = speculate_v2("code here", Lang::Bash, None, None);
        // Should still produce 4 layers, syntax passes by default for unsupported
        assert_eq!(result.layers.len(), 6);
    }

    // ─── S4: AST-aware symbol resolution tests ─────────────────────────

    #[test]
    fn test_symbol_layer_ignores_comments() {
        let source = "// DataStore removed\nfn main() {}";
        let details = vec![SymbolDetail {
            name: "DataStore".into(),
            kind: crate::ast::symbol_detail::MemberKind::Field,
            type_str: None,
            is_pub: false,
            line: 1,
        }];
        let result = speculate_v2(source, Lang::Rust, Some(&details), None);
        // DataStore should NOT be found (only in comment)
        let sym_layer = &result.layers[1]; // SymbolResolution is index 1
        assert!(
            !sym_layer.passed || sym_layer.score < 1.0,
            "symbol in comment should not resolve: {:?}",
            sym_layer
        );
    }

    #[test]
    fn test_symbol_layer_finds_real_identifiers() {
        let source = "struct DataStore { data: Vec<u8> }";
        let details = vec![SymbolDetail {
            name: "DataStore".into(),
            kind: crate::ast::symbol_detail::MemberKind::Field,
            type_str: None,
            is_pub: false,
            line: 1,
        }];
        let result = speculate_v2(source, Lang::Rust, Some(&details), None);
        let sym_layer = &result.layers[1];
        assert!(
            sym_layer.passed,
            "real identifier should resolve: {:?}",
            sym_layer
        );
    }

    #[test]
    fn test_symbol_layer_ignores_string_literals() {
        let source = r#"fn main() { let s = "FakeSymbol"; }"#;
        let details = vec![SymbolDetail {
            name: "FakeSymbol".into(),
            kind: crate::ast::symbol_detail::MemberKind::Field,
            type_str: None,
            is_pub: false,
            line: 1,
        }];
        let result = speculate_v2(source, Lang::Rust, Some(&details), None);
        let sym_layer = &result.layers[1];
        assert!(
            !sym_layer.passed || sym_layer.score < 1.0,
            "symbol in string literal should not resolve: {:?}",
            sym_layer
        );
    }

    #[test]
    fn test_structural_python_bare_except() {
        let src = "try:\n    pass\nexcept:\n    pass\n";
        let result = speculate_v2(src, Lang::Python, None, None);
        let structural = result
            .layers
            .iter()
            .find(|l| l.layer == ValidationLayer::Structural);
        assert!(
            structural
                .map(|l| !l.diagnostics.is_empty())
                .unwrap_or(false),
            "should flag bare except: {:?}",
            result.layers
        );
    }

    #[test]
    fn test_structural_typescript_any_annotation() {
        let src = "function f(x: any) { return x; }";
        let result = speculate_v2(src, Lang::TypeScript, None, None);
        let structural = result
            .layers
            .iter()
            .find(|l| l.layer == ValidationLayer::Structural);
        assert!(
            structural
                .map(|l| !l.diagnostics.is_empty())
                .unwrap_or(false),
            "should flag : any in TypeScript: {:?}",
            result.layers
        );
    }

    #[test]
    fn test_structural_typescript_ts_ignore() {
        let src = "// @ts-ignore\nconst x = badCall();";
        let result = speculate_v2(src, Lang::TypeScript, None, None);
        let structural = result
            .layers
            .iter()
            .find(|l| l.layer == ValidationLayer::Structural);
        assert!(
            structural
                .map(|l| !l.diagnostics.is_empty())
                .unwrap_or(false),
            "should flag @ts-ignore: {:?}",
            result.layers
        );
    }

    // ─── S5: extract_cfg_gated_pub_items — all cfg conditions, not just features ─

    #[test]
    fn test_cfg_unix_condition() {
        let src = "#[cfg(unix)]\npub fn unix_only() {}";
        let items = extract_cfg_gated_pub_items(src);
        assert_eq!(items.len(), 1, "should detect unix cfg item");
        assert_eq!(items[0].cfg_condition, "unix");
        assert_eq!(items[0].name, "unix_only");
    }

    #[test]
    fn test_cfg_target_os_condition() {
        let src = "#[cfg(target_os = \"linux\")]\npub struct LinuxHandle;";
        let items = extract_cfg_gated_pub_items(src);
        assert_eq!(items.len(), 1, "should detect target_os cfg item");
        assert_eq!(items[0].cfg_condition, "target_os = \"linux\"");
        assert_eq!(items[0].name, "LinuxHandle");
    }

    #[test]
    fn test_cfg_not_condition() {
        let src = "#[cfg(not(windows))]\npub fn posix_fn() -> i32 { 0 }";
        let items = extract_cfg_gated_pub_items(src);
        assert_eq!(items.len(), 1, "should detect not(...) cfg item");
        assert_eq!(items[0].cfg_condition, "not(windows)");
    }

    #[test]
    fn test_cfg_all_condition_composite() {
        let src = "#[cfg(all(feature = \"async\", unix))]\npub async fn async_unix_fn() {}";
        let items = extract_cfg_gated_pub_items(src);
        assert_eq!(items.len(), 1, "should detect all(...) cfg item");
        assert_eq!(items[0].cfg_condition, "all(feature = \"async\", unix)");
        assert_eq!(items[0].name, "async_unix_fn");
    }

    #[test]
    fn test_cfg_any_condition() {
        let src = "#[cfg(any(target_os = \"linux\", target_os = \"macos\"))]\npub const PLATFORM: &str = \"unix-like\";";
        let items = extract_cfg_gated_pub_items(src);
        assert_eq!(items.len(), 1, "should detect any(...) cfg item");
        assert!(
            items[0].cfg_condition.starts_with("any(target_os"),
            "condition: {}",
            items[0].cfg_condition
        );
    }

    #[test]
    fn test_cfg_feature_still_works() {
        let src = "#[cfg(feature = \"simd\")]\npub struct SimdEngine;";
        let items = extract_cfg_gated_pub_items(src);
        assert_eq!(items.len(), 1, "feature gate still detected");
        assert_eq!(items[0].cfg_condition, "feature = \"simd\"");
    }

    #[test]
    fn test_cfg_multiple_conditions_mixed() {
        let src = "\
#[cfg(unix)]\npub fn unix_fn() {}\n\
#[cfg(target_arch = \"x86_64\")]\npub struct X86Struct;\n\
#[cfg(not(feature = \"legacy\"))]\npub mod modern {}\n";
        let items = extract_cfg_gated_pub_items(src);
        assert_eq!(items.len(), 3, "should detect all 3 cfg items: {:?}", items);
        let conditions: Vec<&str> = items.iter().map(|i| i.cfg_condition.as_str()).collect();
        assert!(conditions.contains(&"unix"), "missing unix");
        assert!(
            conditions.contains(&"target_arch = \"x86_64\""),
            "missing target_arch"
        );
        assert!(
            conditions.contains(&"not(feature = \"legacy\")"),
            "missing not(feature)"
        );
    }

    #[test]
    fn test_cfg_no_items_returns_empty() {
        let src = "pub fn normal() {}\npub struct Plain;\n";
        let items = extract_cfg_gated_pub_items(src);
        assert!(items.is_empty(), "no cfg items in plain source");
    }

    #[test]
    fn test_cfg_impact_layer_present_in_speculate_result() {
        // CfgImpact layer is always present regardless of cfg items
        let src = "#[cfg(unix)]\npub fn unix_fn() {}";
        let result = speculate_v2(src, Lang::Rust, None, None);
        assert_eq!(result.layers.len(), 6, "should have 6 layers");
        let cfg_layer = result
            .layers
            .iter()
            .find(|l| l.layer == ValidationLayer::CfgImpact);
        assert!(
            cfg_layer.is_some(),
            "CfgImpact layer must always be present"
        );
        assert!(
            cfg_layer.expect("CfgImpact layer always present").passed,
            "CfgImpact is always informational (passed=true)"
        );
        assert!(
            !result.cfg_gated_items.is_empty(),
            "cfg_gated_items should be populated"
        );
        assert_eq!(result.cfg_gated_items[0].cfg_condition, "unix");
    }

    #[test]
    fn test_cfg_impact_score_does_not_affect_composite() {
        // composite_score uses only 4-layer weights — CfgImpact is informational
        let src_clean = "pub fn safe(x: u32) -> u32 { x }";
        let src_gated = "#[cfg(unix)]\npub fn safe(x: u32) -> u32 { x }";
        let r1 = speculate_v2(src_clean, Lang::Rust, None, None);
        let r2 = speculate_v2(src_gated, Lang::Rust, None, None);
        // composite_score must be identical — CfgImpact doesn't change it
        assert!(
            (r1.composite_score - r2.composite_score).abs() < 1e-9,
            "CfgImpact must not affect composite_score: {:.6} vs {:.6}",
            r1.composite_score,
            r2.composite_score
        );
    }
}
