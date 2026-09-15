//! S4 (2026-09-02) — API-cascade preview: "these N call sites break" BEFORE
//! the edit lands (the ★ of the TIER-2 strategy).
//!
//! `diff_api_surfaces` + `plan_api_cascade` existed since Wave C2 and ran only
//! AFTER an edit (`api_cascade_bridge::analyze_rust_edit`, consumed by
//! `post_edit`). Here they run on the `ApiPreview` of the edit — the file as
//! it will be — so a public item that this edit removes or re-signs is named
//! together with its call sites: the in-file ones from the call graph of the
//! "after" source (same move as the post path) and the cross-file ones from
//! the symbol index (`find_references`, the A2 route).
//!
//! What A2 (`CrossCallerLayer`) cannot see: an edit that touches ONLY the
//! definition (`pub fn total(a, b)` → `pub fn total(a, b, c)`) changes no call
//! expression, so A2 stays silent while every caller is about to break. This
//! layer speaks exactly there.
//!
//! Pure additions have no callers to break and produce no signal. Runs at
//! CILA ≥ `MIN_CILA`; the parse is already paid by the preview (any tree-sitter language).

use std::collections::BTreeMap;

use touring_code::ast::api_cascade::{extract_symbol_name, plan_api_cascade};
use touring_code::ast::call_graph::{build_call_graph, supports_call_graph};
use touring_code::ast::rust_semantic::ApiChangeKind;

use crate::shared::api_preview::ApiPreview;
use crate::shared::related_symbols::same_file;
use crate::shared::signal_pipeline::{ProposedChange, SignalContext, SignalLayer};

/// Weight of a cascade signal — high: the edit is about to break callers.
pub(crate) const CASCADE_SCORE: f32 = 0.9;

/// CILA level below which the preview stays silent (strategy: CILA ≥ 2).
pub(crate) const MIN_CILA: usize = 2;

/// Most public items named per edit.
const MAX_ITEMS: usize = 4;

/// Most call sites listed per item.
const MAX_SITES_SHOWN: usize = 4;

/// One public item this edit removes or re-signs, with everything that calls it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CascadeItem {
    /// Symbol name (`total`).
    pub symbol: String,
    /// `true` when a new signature for the same symbol exists after the edit.
    pub resigned: bool,
    /// Call sites as `file:line`, in-file first, then cross-file (deduplicated).
    pub sites: Vec<String>,
}

/// Compute the cascade of an edit from its preview. `lookup(symbol)` returns
/// the cross-file references of a symbol as `(file, line)`; `rel_path` is the
/// edited file, whose own sites come from the call graph instead.
pub(crate) fn cascade_items<F>(
    rel_path: &str,
    preview: &ApiPreview,
    mut lookup: F,
) -> Vec<CascadeItem>
where
    F: FnMut(&str) -> Vec<(String, usize)>,
{
    let changes = preview.api_changes();
    if changes.is_empty() {
        return Vec::new();
    }
    let mut removed: BTreeMap<String, bool> = BTreeMap::new();
    let mut added: Vec<String> = Vec::new();
    for change in &changes {
        let symbol = extract_symbol_name(&change.item);
        match change.kind {
            ApiChangeKind::Removed => {
                removed.entry(symbol).or_insert(false);
            }
            ApiChangeKind::Added => added.push(symbol),
        }
    }
    if removed.is_empty() {
        return Vec::new();
    }
    for symbol in &added {
        if let Some(resigned) = removed.get_mut(symbol) {
            *resigned = true;
        }
    }

    // In-file call sites: the call graph of the "after" source (Rust, Python,
    // TS/JS), through the same planner the post path uses; languages without
    // a call graph keep the cross-file sites only.
    let plan = if supports_call_graph(preview.lang) {
        let graph = build_call_graph(&preview.after, preview.lang);
        plan_api_cascade(&changes, &graph)
    } else {
        plan_api_cascade(&changes, &Default::default())
    };

    removed
        .into_iter()
        .take(MAX_ITEMS)
        .filter_map(|(symbol, resigned)| {
            // The planner emits one proposal per change (a re-sign is Removed +
            // Added), each carrying the same in-file callers — keep each once.
            let mut sites: Vec<String> = Vec::new();
            for caller in plan
                .proposals
                .iter()
                .filter(|p| p.symbol == symbol)
                .flat_map(|p| p.callers.iter())
            {
                let site = format!("{rel_path}:{} ({})", caller.line, caller.caller);
                if !sites.contains(&site) {
                    sites.push(site);
                }
            }
            for (file, line) in lookup(&symbol) {
                if same_file(&file, rel_path) {
                    continue;
                }
                let site = format!("{file}:{line}");
                if !sites.contains(&site) {
                    sites.push(site);
                }
            }
            if sites.is_empty() {
                None
            } else {
                Some(CascadeItem {
                    symbol,
                    resigned,
                    sites,
                })
            }
        })
        .collect()
}

/// Render one item as the hook signal text.
pub(crate) fn cascade_signal(item: &CascadeItem) -> (f32, String) {
    let verb = if item.resigned {
        "signature changes"
    } else {
        "is removed from the public API"
    };
    let n = item.sites.len();
    let shown: Vec<&str> = item
        .sites
        .iter()
        .take(MAX_SITES_SHOWN)
        .map(String::as_str)
        .collect();
    let more = if n > MAX_SITES_SHOWN {
        format!(" (+{} more)", n - MAX_SITES_SHOWN)
    } else {
        String::new()
    };
    (
        CASCADE_SCORE,
        format!(
            "[cascade] `{}` {verb} — {n} call site{} break: {}{more} — update them with this edit",
            item.symbol,
            if n == 1 { "" } else { "s" },
            shown.join(", ")
        ),
    )
}

/// `pre_edit` layer carrying the cascade of the edit's [`ApiPreview`].
pub(crate) struct ApiCascadePreviewLayer {
    signals: Vec<(f32, String)>,
}

impl ApiCascadePreviewLayer {
    /// Build from the edit's preview (`None` → silent layer).
    pub(crate) fn for_edit<F>(rel_path: &str, preview: Option<&ApiPreview>, lookup: F) -> Self
    where
        F: FnMut(&str) -> Vec<(String, usize)>,
    {
        let signals = match preview {
            Some(p) => cascade_items(rel_path, p, lookup)
                .iter()
                .map(cascade_signal)
                .collect(),
            None => Vec::new(),
        };
        Self { signals }
    }

    /// `true` when nothing this edit changes has callers to break.
    pub(crate) fn is_empty(&self) -> bool {
        self.signals.is_empty()
    }
}

impl SignalLayer for ApiCascadePreviewLayer {
    fn name(&self) -> &'static str {
        "api_cascade_preview"
    }

    fn enrich(&self, ctx: &SignalContext<'_>) -> Vec<(f32, String)> {
        match ctx.proposed {
            Some(ProposedChange::Edit { .. }) => self.signals.clone(),
            _ => Vec::new(),
        }
    }

    fn should_run(&self, cila_level: usize) -> bool {
        cila_level >= MIN_CILA
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BEFORE: &str = "pub fn total(a: i32, b: i32) -> i32 {\n    a + b\n}\n\npub fn twice(x: i32) -> i32 {\n    total(x, x)\n}\n";

    fn preview(old: &str, new: &str) -> ApiPreview {
        ApiPreview::for_edit("src/calc.rs", BEFORE, old, new).expect("preview")
    }

    #[test]
    fn python_resigned_function_cascades_to_its_call_sites() {
        let before =
            "def total(a, b):\n    return a + b\n\n\ndef twice(x):\n    return total(x, x)\n";
        let p = ApiPreview::for_edit(
            "src/calc.py",
            before,
            "def total(a, b):",
            "def total(a, b, c):",
        )
        .expect("python preview");
        let items = cascade_items("src/calc.py", &p, |_| {
            vec![("src/report.py".to_string(), 3)]
        });
        assert_eq!(items.len(), 1, "{items:?}");
        assert!(items[0].resigned);
        assert!(
            items[0].sites.contains(&"src/report.py:3".to_string()),
            "{:?}",
            items[0].sites
        );
    }

    #[test]
    fn resigned_item_lists_in_file_and_cross_file_call_sites() {
        let p = preview(
            "pub fn total(a: i32, b: i32) -> i32 {",
            "pub fn total(a: i32, b: i32, c: i32) -> i32 {",
        );
        let items = cascade_items("src/calc.rs", &p, |name| {
            assert_eq!(name, "total");
            vec![
                ("src/report.rs".to_string(), 12),
                ("src/calc.rs".to_string(), 6),
            ]
        });
        assert_eq!(items.len(), 1, "{items:?}");
        let item = &items[0];
        assert_eq!(item.symbol, "total");
        assert!(item.resigned);
        assert!(
            item.sites
                .iter()
                .any(|s| s.starts_with("src/calc.rs:") && s.contains("twice")),
            "{:?}",
            item.sites
        );
        assert!(
            item.sites.contains(&"src/report.rs:12".to_string()),
            "{:?}",
            item.sites
        );
        assert_eq!(
            item.sites
                .iter()
                .filter(|s| s.starts_with("src/calc.rs:"))
                .count(),
            1,
            "own file only via the graph: {:?}",
            item.sites
        );
        let (score, text) = cascade_signal(item);
        assert_eq!(score, CASCADE_SCORE);
        assert!(
            text.contains("[cascade] `total` signature changes"),
            "{text}"
        );
        assert!(text.contains("2 call sites break"), "{text}");
    }

    #[test]
    fn pure_addition_has_nothing_to_break() {
        let p = preview(
            "pub fn twice(x: i32) -> i32 {",
            "pub fn thrice(x: i32) -> i32 {\n    total(x, total(x, x))\n}\n\npub fn twice(x: i32) -> i32 {",
        );
        // `thrice` is added; `twice` unchanged → only `twice`'s signature survives; nothing removed.
        let items = cascade_items("src/calc.rs", &p, |_| vec![("src/other.rs".to_string(), 1)]);
        assert!(items.iter().all(|i| i.symbol != "thrice"), "{items:?}");
    }

    #[test]
    fn removed_item_without_callers_is_silent() {
        let p = preview("pub fn twice(x: i32) -> i32 {\n    total(x, x)\n}\n", "");
        let items = cascade_items("src/calc.rs", &p, |_| Vec::new());
        assert!(
            items.is_empty(),
            "no caller anywhere → nothing breaks: {items:?}"
        );
    }

    #[test]
    fn layer_speaks_only_for_edits_and_from_cila_two() {
        let p = preview(
            "pub fn total(a: i32, b: i32) -> i32 {",
            "pub fn total(a: i32, b: i32, c: i32) -> i32 {",
        );
        let layer = ApiCascadePreviewLayer::for_edit("src/calc.rs", Some(&p), |_| {
            vec![("src/r.rs".to_string(), 3)]
        });
        assert!(!layer.is_empty());
        assert!(!layer.should_run(1) && layer.should_run(2));
        let ctx = crate::shared::signal_pipeline::context_for_edit("src/calc.rs", "a", "b", 2);
        assert_eq!(layer.enrich(&ctx).len(), 1);
        let write_ctx = crate::shared::signal_pipeline::context_for_write("src/calc.rs", BEFORE, 2);
        assert!(layer.enrich(&write_ctx).is_empty());
        assert!(ApiCascadePreviewLayer::for_edit("src/calc.rs", None, |_| Vec::new()).is_empty());
    }
}
