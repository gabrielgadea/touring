//! S9 (2026-09-02) — the public API of the file about to EXIST, for every
//! language the tree-sitter extractor knows (Rust, Python, TS/JS, Go, …):
//! one parse shared by the API badge (this module) and the API-cascade
//! preview (S4, `api_cascade_preview.rs`).
//!
//! The surface is built from `extract_symbols` (`Symbol::signature` = the
//! header without body, `Symbol::is_public` = language-aware visibility), so
//! a changed parameter list IS a changed entry — `public_api_surface`
//! (name-level, Rust-only) could not see a re-signed function. Rust
//! additionally gets the syn `RustSemanticReport` for the semantic number.
//!
//! Cost: one tree-sitter parse of the "after" text (plus one of the "before"
//! text for an Edit) and, for Rust, one syn parse. Files above
//! [`MAX_PREVIEW_BYTES`] and languages without an extractor yield no preview.

use touring_code::ast::languages::Lang;
use touring_code::ast::rust_semantic::{ApiChange, ApiChangeKind, RustSemanticReport, diff_api_surfaces};
use touring_code::ast::symbols::extract_symbols;

use crate::shared::quality_signal::apply_edit;
use crate::shared::signal_pipeline::{ProposedChange, SignalContext, SignalLayer};

/// Weight of the API badge — informational, never a gate.
pub(crate) const API_BADGE_SCORE: f32 = 0.4;

/// Largest text the preview is willing to parse (bytes).
pub(crate) const MAX_PREVIEW_BYTES: usize = 100_000;

/// The file as it will be after the change, with its public API extracted.
pub(crate) struct ApiPreview {
    /// Language of the file.
    pub lang: Lang,
    /// The text that will exist after the change.
    pub after: String,
    /// Public API entries of `after` (`fn name(params)`, `type Name`).
    pub surface_after: Vec<String>,
    /// Public API entries before the change — `None` for a Write (nothing
    /// existed) or when the current file does not parse.
    pub surface_before: Option<Vec<String>>,
    /// syn report — Rust only (the semantic number of the badge).
    pub rust: Option<RustSemanticReport>,
}

/// Public API entries of `source`: callables as `fn name(params)` (methods
/// carry ` @Parent`), type definitions as `type Name`. Sorted, deduplicated.
/// `None` when the language has no extractor or the text does not parse.
pub(crate) fn api_surface(lang: Lang, source: &str) -> Option<Vec<String>> {
    let symbols = extract_symbols(source, lang).ok()?;
    let mut entries: Vec<String> = symbols
        .iter()
        .filter(|s| s.is_public)
        .filter_map(|s| {
            if s.kind.is_callable() {
                let parent = s
                    .parent_name
                    .as_deref()
                    .map(|p| format!(" @{p}"))
                    .unwrap_or_default();
                Some(format!("fn {}({}){parent}", s.name, params_of(&s.signature)))
            } else if s.kind.is_type_definition() {
                Some(format!("type {}", s.name))
            } else {
                None
            }
        })
        .collect();
    entries.sort();
    entries.dedup();
    Some(entries)
}

/// The parameter list of a signature header, whitespace-normalised
/// (`a: i32, b: i32`); empty when the header has no parenthesised list.
fn params_of(signature: &str) -> String {
    let Some(open) = signature.find('(') else {
        return String::new();
    };
    let mut depth = 0usize;
    let mut end = None;
    for (i, ch) in signature[open..].char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    end = Some(open + i);
                    break;
                }
            }
            _ => {}
        }
    }
    let inner = match end {
        Some(e) => &signature[open + 1..e],
        None => &signature[open + 1..],
    };
    inner.split_whitespace().collect::<Vec<_>>().join(" ")
}

impl ApiPreview {
    /// Preview of a Write: the proposed content IS the file.
    pub(crate) fn for_write(rel_path: &str, content: &str) -> Option<Self> {
        let lang = Lang::from_path(std::path::Path::new(rel_path))?;
        Self::parse(lang, content.to_string(), None)
    }

    /// Preview of an Edit: `old → new` applied in memory over `current`.
    pub(crate) fn for_edit(rel_path: &str, current: &str, old: &str, new: &str) -> Option<Self> {
        let lang = Lang::from_path(std::path::Path::new(rel_path))?;
        let after = apply_edit(current, old, new)?;
        let before = api_surface(lang, current);
        Self::parse(lang, after, before)
    }

    fn parse(lang: Lang, after: String, surface_before: Option<Vec<String>>) -> Option<Self> {
        if after.len() > MAX_PREVIEW_BYTES {
            return None;
        }
        let surface_after = api_surface(lang, &after)?;
        let rust = match lang {
            Lang::Rust => RustSemanticReport::from_source(&after).ok(),
            _ => None,
        };
        Some(Self {
            lang,
            after,
            surface_after,
            surface_before,
            rust,
        })
    }

    /// Public-API changes the edit introduces, at signature level (a
    /// re-signed function is one `Removed` + one `Added`). Empty for a Write
    /// or when the file did not parse before the edit.
    pub(crate) fn api_changes(&self) -> Vec<ApiChange> {
        match &self.surface_before {
            Some(before) => diff_api_surfaces(before, &self.surface_after),
            None => Vec::new(),
        }
    }

    /// The badge: `[rust] semantic 0.31 · pub API 7 (+1/-0) · unsafe 2` for
    /// Rust, `[python] pub API 3 (+1/-0)` elsewhere. The delta appears only
    /// for an Edit that changes the public API; the unsafe count only when
    /// non-zero.
    pub(crate) fn badge(&self) -> String {
        let mut badge = format!("[{}] ", self.lang.as_str());
        if let Some(rust) = &self.rust {
            badge.push_str(&format!("semantic {:.2} · ", rust.semantic_complexity()));
        }
        badge.push_str(&format!("pub API {}", self.surface_after.len()));
        let changes = self.api_changes();
        if !changes.is_empty() {
            let added = changes
                .iter()
                .filter(|c| c.kind == ApiChangeKind::Added)
                .count();
            let removed = changes.len() - added;
            badge.push_str(&format!(" (+{added}/-{removed})"));
        }
        if let Some(rust) = &self.rust
            && rust.unsafe_blocks > 0
        {
            badge.push_str(&format!(" · unsafe {}", rust.unsafe_blocks));
        }
        badge
    }
}

/// `pre_write` / `pre_edit` layer carrying the badge of an [`ApiPreview`].
pub(crate) struct ApiBadgeLayer {
    signal: Option<(f32, String)>,
}

impl ApiBadgeLayer {
    /// Build from an optional preview (no preview → silent layer).
    pub(crate) fn from_preview(preview: Option<&ApiPreview>) -> Self {
        Self {
            signal: preview.map(|p| (API_BADGE_SCORE, p.badge())),
        }
    }

    /// `true` when there is no badge to show.
    pub(crate) fn is_empty(&self) -> bool {
        self.signal.is_none()
    }
}

impl SignalLayer for ApiBadgeLayer {
    fn name(&self) -> &'static str {
        "api_badge"
    }

    fn enrich(&self, ctx: &SignalContext<'_>) -> Vec<(f32, String)> {
        match (&self.signal, ctx.proposed) {
            (Some(signal), Some(ProposedChange::Write { .. } | ProposedChange::Edit { .. })) => {
                vec![signal.clone()]
            }
            _ => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RUST_SRC: &str = "pub struct Holder<T: Clone> {\n    pub value: T,\n}\n\n\
impl<T: Clone> Default for Holder<T> where T: Default {\n    fn default() -> Self {\n        Self { value: T::default() }\n    }\n}\n\n\
pub fn make<T: Clone + Default>() -> Holder<T> {\n    Holder::default()\n}\n";

    #[test]
    fn params_are_normalised_and_nested_parens_survive() {
        assert_eq!(params_of("fn f(a: i32,\n    b: Vec<(u8, u8)>) -> i32"), "a: i32, b: Vec<(u8, u8)>");
        assert_eq!(params_of("def f():"), "");
        assert_eq!(params_of("struct S"), "");
    }

    #[test]
    fn rust_write_preview_has_semantic_number_and_surface() {
        let p = ApiPreview::for_write("src/holder.rs", RUST_SRC).expect("preview");
        assert!(p.rust.as_ref().map(|r| r.semantic_complexity()).unwrap_or(0.0) > 0.0);
        assert!(p.surface_after.iter().any(|e| e.starts_with("fn make(")), "{:?}", p.surface_after);
        assert!(p.surface_after.iter().any(|e| e == "type Holder"), "{:?}", p.surface_after);
        let badge = p.badge();
        assert!(badge.starts_with("[rust] semantic "), "{badge}");
        assert!(badge.contains("pub API 2"), "{badge}");
        assert!(!badge.contains('+'), "no delta for a Write: {badge}");
    }

    #[test]
    fn rust_edit_preview_sees_an_added_item_and_a_resigned_one() {
        let current = "pub fn a() {}\n";
        let p = ApiPreview::for_edit("src/lib.rs", current, "pub fn a() {}\n", "pub fn a() {}\npub fn b() {}\n")
            .expect("preview");
        assert_eq!(p.api_changes().len(), 1);
        assert!(p.badge().contains("pub API 2 (+1/-0)"), "{}", p.badge());

        let before = "pub fn total(a: i32, b: i32) -> i32 {\n    a + b\n}\n";
        let p = ApiPreview::for_edit(
            "src/calc.rs",
            before,
            "pub fn total(a: i32, b: i32) -> i32 {",
            "pub fn total(a: i32, b: i32, c: i32) -> i32 {",
        )
        .expect("preview");
        let changes = p.api_changes();
        assert_eq!(changes.len(), 2, "re-sign = removed + added: {changes:?}");
        assert!(changes.iter().any(|c| c.kind == ApiChangeKind::Removed && c.item.starts_with("fn total(")));
        assert!(p.badge().contains("(+1/-1)"), "{}", p.badge());
    }

    #[test]
    fn python_and_typescript_previews_see_a_resigned_function() {
        let py_before = "def total(a, b):\n    return a + b\n\n\ndef _hidden():\n    pass\n";
        let p = ApiPreview::for_edit("src/calc.py", py_before, "def total(a, b):", "def total(a, b, c):")
            .expect("python preview");
        assert!(p.rust.is_none());
        assert!(p.surface_after.iter().any(|e| e.starts_with("fn total(")), "{:?}", p.surface_after);
        assert!(!p.surface_after.iter().any(|e| e.contains("_hidden")), "private stays out: {:?}", p.surface_after);
        assert_eq!(p.api_changes().len(), 2, "{:?}", p.api_changes());
        assert!(p.badge().starts_with("[python] pub API "), "{}", p.badge());

        let ts_before = "export function total(a: number, b: number): number {\n  return a + b;\n}\n";
        let p = ApiPreview::for_edit(
            "src/calc.ts",
            ts_before,
            "export function total(a: number, b: number): number {",
            "export function total(a: number, b: number, c: number): number {",
        )
        .expect("typescript preview");
        assert_eq!(p.api_changes().len(), 2, "{:?}", p.api_changes());
    }

    #[test]
    fn unknown_language_and_unparsable_rust_yield_no_preview() {
        assert!(ApiPreview::for_write("notes/readme.txt", "hello").is_none());
        assert!(ApiPreview::for_edit("src/lib.rs", "pub fn a() {}\n", "missing", "x").is_none());
        // tree-sitter is error-tolerant: broken Rust still yields a surface, so
        // the syn number is simply absent from the badge.
        let p = ApiPreview::for_write("src/bad.rs", "pub fn (( {");
        assert!(p.is_none() || p.as_ref().and_then(|p| p.rust.as_ref()).is_none());
    }

    #[test]
    fn badge_layer_speaks_only_for_proposed_content() {
        let p = ApiPreview::for_write("src/holder.rs", RUST_SRC).expect("preview");
        let layer = ApiBadgeLayer::from_preview(Some(&p));
        assert!(!layer.is_empty());
        let write_ctx = crate::shared::signal_pipeline::context_for_write("src/holder.rs", RUST_SRC, 2);
        let out = layer.enrich(&write_ctx);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].0, API_BADGE_SCORE);
        let plain = SignalContext::new("src/holder.rs", RUST_SRC);
        assert!(layer.enrich(&plain).is_empty(), "no proposed change, no badge");
        assert!(ApiBadgeLayer::from_preview(None).is_empty());
    }
}
