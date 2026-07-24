//! ⌘K Command Palette — global overlay (SPEC 2026-06-12 §6.4).
//!
//! W1 shipped page navigation; W4 completes the live sources: symbol
//! definitions (`/api/search`, navigates to /wiring/impact) and memory
//! keys (`/api/memory/recall`, navigates to /memory). Remote sources
//! are debounced 300 ms and capped (5 symbols + 3 memory entries).

use crate::web::components::icons::Icon;
use crate::web::components::sidebar::NavItem;
use crate::web::services::{fetch_memory_recall, fetch_symbol_search};
use leptos::prelude::*;
use leptos_router::NavigateOptions;
use leptos_router::hooks::use_navigate;

/// Filter the nav catalog by a case-insensitive substring query.
/// Empty query returns every page in display order.
pub fn filter_nav(query: &str) -> Vec<NavItem> {
    let q = query.trim().to_lowercase();
    NavItem::iter()
        .filter(|item| {
            q.is_empty()
                || item.label().to_lowercase().contains(&q)
                || item.route().to_lowercase().contains(&q)
        })
        .collect()
}

/// One row in the combined result list.
#[derive(Debug, Clone, PartialEq)]
pub enum PaletteHit {
    /// A registered page route.
    Page(NavItem),
    /// A symbol definition → /wiring/impact?symbol=…
    Symbol {
        /// Symbol name.
        name: String,
        /// Defining file path.
        file: String,
        /// Definition line.
        line: u64,
    },
    /// A memory key → /memory.
    Memory {
        /// Memory entry key.
        key: String,
        /// Storage tier label.
        tier: String,
    },
}

impl PaletteHit {
    /// Route this hit navigates to on Enter/click.
    pub fn route(&self) -> String {
        match self {
            PaletteHit::Page(item) => item.route().to_string(),
            PaletteHit::Symbol { name, .. } => {
                format!(
                    "/wiring/impact?symbol={}",
                    crate::web::services::urlencode(name)
                )
            }
            PaletteHit::Memory { .. } => "/memory".to_string(),
        }
    }

    /// Icon name for the row.
    pub fn icon(&self) -> &'static str {
        match self {
            PaletteHit::Page(_) => "page",
            PaletteHit::Symbol { .. } => "impact",
            PaletteHit::Memory { .. } => "memory",
        }
    }
}

/// Parse `/api/search` definitions into symbol hits (cap 5).
pub fn symbol_hits(v: &serde_json::Value) -> Vec<PaletteHit> {
    v.get("definitions")
        .and_then(|d| d.as_array())
        .map(|defs| {
            defs.iter()
                .take(5)
                .filter_map(|d| {
                    Some(PaletteHit::Symbol {
                        name: d.get("symbol_name")?.as_str()?.to_string(),
                        file: d
                            .get("file_path")
                            .and_then(|f| f.as_str())
                            .unwrap_or("")
                            .to_string(),
                        line: d.get("line").and_then(|l| l.as_u64()).unwrap_or(0),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Parse `/api/memory/recall` entries into memory hits (cap 3).
pub fn memory_hits(v: &serde_json::Value) -> Vec<PaletteHit> {
    v.get("entries")
        .and_then(|e| e.as_array())
        .map(|entries| {
            entries
                .iter()
                .take(3)
                .filter_map(|e| {
                    Some(PaletteHit::Memory {
                        key: e.get("key")?.as_str()?.to_string(),
                        tier: e
                            .get("tier")
                            .and_then(|t| t.as_str())
                            .unwrap_or("")
                            .to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Global command palette overlay. Closed by default; opened via the
/// titlebar search box or ⌘K/Ctrl+K (listener lives in `EliteShell`).
#[component]
pub fn CommandPalette(
    /// Shared open state (provided by `EliteShell`).
    open: RwSignal<bool>,
) -> impl IntoView {
    let query = RwSignal::new(String::new());
    let debounced = RwSignal::new(String::new());
    let selected = RwSignal::new(0usize);
    let input_ref = NodeRef::<leptos::html::Input>::new();

    // Debounce remote lookups by 300 ms (wasm only — native never runs this UI).
    #[cfg(target_arch = "wasm32")]
    {
        use std::cell::RefCell;
        use std::rc::Rc;
        let pending: Rc<RefCell<Option<TimeoutHandle>>> = Rc::new(RefCell::new(None));
        Effect::new(move |_| {
            let q = query.get();
            if let Some(h) = pending.borrow_mut().take() {
                h.clear();
            }
            let handle = set_timeout_with_handle(
                move || debounced.set(q.clone()),
                std::time::Duration::from_millis(300),
            )
            .ok();
            *pending.borrow_mut() = handle;
        });
    }

    // Remote sources — only for queries with ≥ 2 chars.
    let remote = LocalResource::new(move || {
        let q = debounced.get();
        async move {
            if q.trim().len() < 2 {
                return (Vec::new(), Vec::new());
            }
            let symbols = fetch_symbol_search(&q)
                .await
                .map(|v| symbol_hits(&v))
                .unwrap_or_default();
            let memory = fetch_memory_recall(&q)
                .await
                .map(|v| memory_hits(&v))
                .unwrap_or_default();
            (symbols, memory)
        }
    });

    // Combined flat list: pages → symbols → memory.
    let combined = Memo::new(move |_| {
        let mut hits: Vec<PaletteHit> = filter_nav(&query.get())
            .into_iter()
            .map(PaletteHit::Page)
            .collect();
        if let Some((symbols, memory)) = remote.get() {
            hits.extend(symbols);
            hits.extend(memory);
        }
        hits
    });

    // Reset + focus when opening.
    Effect::new(move |_| {
        if open.get() {
            query.set(String::new());
            debounced.set(String::new());
            selected.set(0);
            if let Some(el) = input_ref.get() {
                let _ = el.focus();
            }
        }
    });

    let navigate = use_navigate();
    let go = move |hit: PaletteHit| {
        open.set(false);
        navigate(&hit.route(), NavigateOptions::default());
    };
    let go_key = go.clone();

    let on_keydown = move |e: leptos::ev::KeyboardEvent| {
        let items = combined.get();
        match e.key().as_str() {
            "ArrowDown" => {
                e.prevent_default();
                selected.update(|s| *s = (*s + 1).min(items.len().saturating_sub(1)));
            }
            "ArrowUp" => {
                e.prevent_default();
                selected.update(|s| *s = s.saturating_sub(1));
            }
            "Enter" => {
                e.prevent_default();
                if let Some(hit) = items.get(selected.get()).cloned() {
                    go_key(hit);
                }
            }
            "Escape" => open.set(false),
            _ => selected.set(selected.get().min(items.len().saturating_sub(1))),
        }
    };

    view! {
        {move || open.get().then(|| {
            let go_click = go.clone();
            view! {
                <div
                    class="el-palette-overlay"
                    on:click=move |_| open.set(false)
                >
                    <div
                        class="el-palette"
                        role="dialog"
                        aria-label="command palette"
                        on:click=move |e| e.stop_propagation()
                    >
                        <input
                            node_ref=input_ref
                            class="el-palette-input"
                            type="text"
                            placeholder="Search pages, symbols, memory…"
                            prop:value=move || query.get()
                            on:input=move |e| {
                                query.set(event_target_value(&e));
                                selected.set(0);
                            }
                            on:keydown=on_keydown.clone()
                        />
                        <div class="el-palette-results">
                            {move || {
                                let go_rows = go_click.clone();
                                let items = combined.get();
                                let mut current_group = "";
                                items
                                    .into_iter()
                                    .enumerate()
                                    .map(|(idx, hit)| {
                                        let group = match &hit {
                                            PaletteHit::Page(_) => "Pages",
                                            PaletteHit::Symbol { .. } => "Symbols · touring index find",
                                            PaletteHit::Memory { .. } => "Memory · touring memory recall",
                                        };
                                        let header = if group != current_group {
                                            current_group = group;
                                            Some(view! { <div class="el-palette-group">{group}</div> })
                                        } else {
                                            None
                                        };
                                        let (label, hint) = match &hit {
                                            PaletteHit::Page(item) => {
                                                (item.label().to_string(), item.route().to_string())
                                            }
                                            PaletteHit::Symbol { name, file, line } => {
                                                (name.clone(), format!("{file}:{line}"))
                                            }
                                            PaletteHit::Memory { key, tier } => {
                                                let k = if key.chars().count() > 48 {
                                                    let t: String = key.chars().take(48).collect();
                                                    format!("{t}…")
                                                } else {
                                                    key.clone()
                                                };
                                                (k, tier.clone())
                                            }
                                        };
                                        let icon = hit.icon();
                                        let go_row = go_rows.clone();
                                        let hit_for_click = hit.clone();
                                        view! {
                                            {header}
                                            <div
                                                class="el-result-row"
                                                class:selected=move || selected.get() == idx
                                                on:click=move |_| go_row(hit_for_click.clone())
                                                on:mouseenter=move |_| selected.set(idx)
                                            >
                                                <Icon name=icon/>
                                                <span>{label}</span>
                                                <span class="hint el-mono">{hint}</span>
                                            </div>
                                        }
                                    })
                                    .collect_view()
                            }}
                            {move || combined.get().is_empty().then(|| view! {
                                <div class="el-empty">"No hits across pages, symbols or memory."</div>
                            })}
                        </div>
                        <div class="el-palette-foot">
                            <span><span class="el-kbd">"↑↓"</span>" navigate"</span>
                            <span><span class="el-kbd">"↵"</span>" open"</span>
                            <span><span class="el-kbd">"esc"</span>" close"</span>
                        </div>
                    </div>
                </div>
            }
        })}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_query_lists_all_pages() {
        assert_eq!(filter_nav("").len(), NavItem::iter().count());
        assert_eq!(filter_nav("   ").len(), NavItem::iter().count());
    }

    #[test]
    fn query_filters_by_label_case_insensitive() {
        let hits = filter_nav("QuAl");
        assert!(
            hits.iter()
                .all(|i| i.label().to_lowercase().contains("qual"))
        );
        assert!(hits.len() >= 3, "quality family has 3 pages");
    }

    #[test]
    fn query_filters_by_route_too() {
        let hits = filter_nav("/hooks");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].route(), "/hooks");
    }

    #[test]
    fn no_match_returns_empty() {
        assert!(filter_nav("zzz-no-such-page").is_empty());
    }

    #[test]
    fn symbol_hits_parse_and_cap() {
        let v = serde_json::json!({"definitions": [
            {"symbol_name": "A", "file_path": "a.rs", "line": 1},
            {"symbol_name": "B", "file_path": "b.rs", "line": 2},
            {"symbol_name": "C", "file_path": "c.rs", "line": 3},
            {"symbol_name": "D", "file_path": "d.rs", "line": 4},
            {"symbol_name": "E", "file_path": "e.rs", "line": 5},
            {"symbol_name": "F", "file_path": "f.rs", "line": 6}
        ]});
        let hits = symbol_hits(&v);
        assert_eq!(hits.len(), 5, "capped at 5");
        assert_eq!(hits[0].route(), "/wiring/impact?symbol=A");
    }

    #[test]
    fn memory_hits_parse_and_cap() {
        let v = serde_json::json!({"entries": [
            {"key": "k1", "tier": "semantic"},
            {"key": "k2", "tier": "reference"},
            {"key": "k3", "tier": "semantic"},
            {"key": "k4", "tier": "semantic"}
        ]});
        let hits = memory_hits(&v);
        assert_eq!(hits.len(), 3, "capped at 3");
        assert_eq!(hits[0].route(), "/memory");
    }

    #[test]
    fn malformed_payloads_yield_no_hits() {
        assert!(symbol_hits(&serde_json::json!({"error": "x"})).is_empty());
        assert!(memory_hits(&serde_json::json!({"error": "x"})).is_empty());
    }
}
