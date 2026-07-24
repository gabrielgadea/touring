//! /search — fused search over the two real sources (Elite W3 ·
//! SPEC 2026-06-12 §5.11): the symbol index (`touring index find` via
//! `/api/search?q=`) and the RRF memory store (`touring memory recall`
//! via `/api/memory/recall?q=`). One query, two ranked groups, honest
//! client-measured latency per source — nothing invented.

use crate::web::components::{Icon, PageHero};
use crate::web::services::{fetch_memory_recall, fetch_symbol_search};
use leptos::prelude::*;
use leptos_meta::Title;

/// Maximum chars of a memory key shown inline before truncation.
const KEY_DISPLAY_CHARS: usize = 64;
/// Maximum chars of a memory value carried in the row `title` tooltip.
const VALUE_TITLE_CHARS: usize = 200;

/// Client-side source filter for the fused result list.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum SourceFilter {
    All,
    Symbols,
    Memory,
}

impl SourceFilter {
    fn label(self) -> &'static str {
        match self {
            SourceFilter::All => "All",
            SourceFilter::Symbols => "Symbols",
            SourceFilter::Memory => "Memory",
        }
    }
}

/// One submitted query's fused outcome — the raw payloads from the two
/// real endpoints plus the individually measured client latencies.
#[derive(Clone)]
struct FusedHits {
    query: String,
    symbols: serde_json::Value,
    memory: serde_json::Value,
    sym_ms: u64,
    mem_ms: u64,
}

/// Wall-clock milliseconds for honest client-side latency measurement.
/// Real `js_sys::Date::now()` on wasm.
#[cfg(target_arch = "wasm32")]
fn now_ms() -> f64 {
    js_sys::Date::now()
}

/// Native fallback: constant 0 — native stubs never hit the network,
/// so a zero delta is the honest value (never a fabricated number).
#[cfg(not(target_arch = "wasm32"))]
fn now_ms() -> f64 {
    0.0
}

/// Non-negative rounded delta between two [`now_ms`] readings.
fn elapsed_ms(t0: f64, t1: f64) -> u64 {
    (t1 - t0).max(0.0).round() as u64
}

/// Extract `(symbol_name, file_path, line)` rows from the real
/// `/api/search` payload: `{count, symbol_name, definitions: [...]}`.
/// Maps every array element (missing fields default) so callers can
/// rely on 1:1 alignment with the raw `definitions` array.
pub fn parse_definitions(v: &serde_json::Value) -> Vec<(String, String, u64)> {
    v.get("definitions")
        .and_then(|d| d.as_array())
        .map(|arr| {
            arr.iter()
                .map(|item| {
                    (
                        item.get("symbol_name")
                            .and_then(|s| s.as_str())
                            .unwrap_or("")
                            .to_string(),
                        item.get("file_path")
                            .and_then(|s| s.as_str())
                            .unwrap_or("")
                            .to_string(),
                        item.get("line").and_then(|n| n.as_u64()).unwrap_or(0),
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Extract `(key, tier, type)` rows from the real `/api/memory/recall`
/// payload: `{ann_results, count, entries: [{key, source_db, tier,
/// type, value}], diagnostics}`. Maps every array element (missing
/// fields default) so rows stay 1:1 aligned with `recall_values`.
pub fn parse_recall_entries(v: &serde_json::Value) -> Vec<(String, String, String)> {
    v.get("entries")
        .and_then(|d| d.as_array())
        .map(|arr| {
            arr.iter()
                .map(|e| {
                    (
                        e.get("key")
                            .and_then(|s| s.as_str())
                            .unwrap_or("")
                            .to_string(),
                        e.get("tier")
                            .and_then(|s| s.as_str())
                            .unwrap_or("")
                            .to_string(),
                        e.get("type")
                            .and_then(|s| s.as_str())
                            .unwrap_or("")
                            .to_string(),
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

/// `value` strings of every recall entry, aligned 1:1 with
/// [`parse_recall_entries`] (both map the same array with defaults).
fn recall_values(v: &serde_json::Value) -> Vec<String> {
    v.get("entries")
        .and_then(|d| d.as_array())
        .map(|arr| {
            arr.iter()
                .map(|e| {
                    e.get("value")
                        .and_then(|s| s.as_str())
                        .unwrap_or("")
                        .to_string()
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Char-boundary-safe truncation with a trailing ellipsis.
fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
        out.push('…');
        out
    }
}

/// Display form of a definition path: from `crates/` onward when the
/// path is inside the workspace, otherwise the last two segments. The
/// full path always travels in the row `title` attribute.
fn short_path(path: &str) -> String {
    if let Some(idx) = path.find("crates/") {
        return path[idx..].to_string();
    }
    let segs: Vec<&str> = path.rsplitn(3, '/').collect();
    match segs.as_slice() {
        [file, dir, _rest] => format!("{dir}/{file}"),
        _ => path.to_string(),
    }
}

/// Fused search page — one query against the symbol index and the
/// memory RRF store, rendered as two palette-style result groups.
#[component]
pub fn SymbolSearch() -> impl IntoView {
    let query = RwSignal::new(String::new());
    let submitted = RwSignal::new(String::new());
    let filter = RwSignal::new(SourceFilter::All);

    let submit = move || submitted.set(query.get_untracked().trim().to_string());

    let results = LocalResource::new(move || {
        let q = submitted.get();
        async move {
            let q = q.trim().to_string();
            if q.is_empty() {
                return None;
            }
            let t0 = now_ms();
            let symbols = fetch_symbol_search(&q)
                .await
                .unwrap_or(serde_json::Value::Null);
            let t1 = now_ms();
            let memory = fetch_memory_recall(&q)
                .await
                .unwrap_or(serde_json::Value::Null);
            let t2 = now_ms();
            Some(FusedHits {
                query: q,
                symbols,
                memory,
                sym_ms: elapsed_ms(t0, t1),
                mem_ms: elapsed_ms(t1, t2),
            })
        }
    });

    view! {
        <div class="page pg-search">
            <Title text="Touring — Search"/>
            <div class="el-main-editorial">
                <PageHero
                    eyebrow="CODE INTELLIGENCE · SEARCH"
                    title="Search"
                    title_em="fused"
                    sub="One query, two real sources — definitions from the symbol index (touring index find) and lessons from the RRF-fused memory store (touring memory recall). Latencies below are measured on this client, per request."
                >
                    ""
                </PageHero>

                // ── Big search box — Enter or the button submits ─────────
                <div class="el-card pg-search-box">
                    <Icon name="search"/>
                    <input
                        type="text"
                        class="pg-search-input"
                        placeholder="Search symbols and memory… e.g. SemanticClassifier"
                        prop:value=move || query.get()
                        on:input=move |ev| query.set(event_target_value(&ev))
                        on:keydown=move |ev| {
                            if ev.key() == "Enter" {
                                submit();
                            }
                        }
                        autofocus=true
                    />
                    <kbd class="el-kbd" title="The global palette also searches">"⌘K"</kbd>
                    <button
                        type="button"
                        class="el-btn el-btn-primary"
                        on:click=move |_| submit()
                    >
                        "Search"
                    </button>
                </div>

                // ── Source pills — client-side display filter ────────────
                <div class="pg-search-pills" role="tablist" aria-label="result source filter">
                    {[SourceFilter::All, SourceFilter::Symbols, SourceFilter::Memory]
                        .into_iter()
                        .map(|f| view! {
                            <button
                                type="button"
                                class=move || if filter.get() == f {
                                    "el-tag el-tag-on pg-search-pill"
                                } else {
                                    "el-tag pg-search-pill"
                                }
                                on:click=move |_| filter.set(f)
                            >
                                {f.label()}
                            </button>
                        })
                        .collect_view()}
                </div>

                // ── Fused results ────────────────────────────────────────
                <Suspense fallback=|| view! {
                    <div class="el-skeleton" aria-label="searching"></div>
                }>
                    {move || results.get().map(|hits| match hits {
                        None => view! {
                            <div class="el-card pg-search-group">
                                <div class="el-empty">
                                    "Type a query and press Enter — results fuse the symbol index with memory RRF. The global palette ("
                                    <kbd class="el-kbd">"⌘K"</kbd>
                                    ") searches too."
                                </div>
                            </div>
                        }.into_any(),
                        Some(out) => {
                            let q = out.query.clone();
                            let defs = parse_definitions(&out.symbols);
                            let entries = parse_recall_entries(&out.memory);
                            let values = recall_values(&out.memory);
                            let f = filter.get();
                            let show_sym = f != SourceFilter::Memory;
                            let show_mem = f != SourceFilter::Symbols;
                            let timing = format!(
                                "symbols {}ms · memory {}ms",
                                out.sym_ms, out.mem_ms
                            );

                            let sym_group = show_sym.then(|| {
                                let count = defs.len();
                                let q_label = q.clone();
                                let body = if count == 0 {
                                    view! {
                                        <div class="el-empty">
                                            {format!("no hits for \"{q_label}\" in symbols")}
                                        </div>
                                    }.into_any()
                                } else {
                                    view! {
                                        <div class="pg-search-rows">
                                            {defs.into_iter().map(|(symbol, file, line)| {
                                                let loc = format!("{}:{line}", short_path(&file));
                                                view! {
                                                    <div class="el-result-row" title=file>
                                                        <Icon name="page"/>
                                                        <span class="pg-search-symbol">{symbol}</span>
                                                        <span class="hint el-mono">{loc}</span>
                                                    </div>
                                                }
                                            }).collect_view()}
                                        </div>
                                    }.into_any()
                                };
                                view! {
                                    <div class="el-card pg-search-group">
                                        <div class="el-palette-group pg-search-group-head">
                                            <span>"SYMBOLS · touring index find"</span>
                                            <span class="pg-search-group-count el-mono">
                                                {count.to_string()}
                                            </span>
                                        </div>
                                        {body}
                                    </div>
                                }
                            });

                            let mem_group = show_mem.then(|| {
                                let count = entries.len();
                                let q_label = q.clone();
                                let body = if count == 0 {
                                    view! {
                                        <div class="el-empty">
                                            {format!("no hits for \"{q_label}\" in memory")}
                                        </div>
                                    }.into_any()
                                } else {
                                    view! {
                                        <div class="pg-search-rows">
                                            {entries.into_iter().zip(values).map(|((key, tier, kind), value)| {
                                                let shown_key = truncate_chars(&key, KEY_DISPLAY_CHARS);
                                                let tooltip = truncate_chars(&value, VALUE_TITLE_CHARS);
                                                view! {
                                                    <div class="el-result-row" title=tooltip>
                                                        <Icon name="memory"/>
                                                        <span class="pg-search-key el-mono">{shown_key}</span>
                                                        <span class="pg-search-tags">
                                                            {(!tier.is_empty()).then(|| view! {
                                                                <span class="el-tag">{tier}</span>
                                                            })}
                                                            {(!kind.is_empty()).then(|| view! {
                                                                <span class="el-tag">{kind}</span>
                                                            })}
                                                        </span>
                                                    </div>
                                                }
                                            }).collect_view()}
                                        </div>
                                    }.into_any()
                                };
                                view! {
                                    <div class="el-card pg-search-group">
                                        <div class="el-palette-group pg-search-group-head">
                                            <span>"MEMORY · touring memory recall"</span>
                                            <span class="pg-search-group-count el-mono">
                                                {count.to_string()}
                                            </span>
                                        </div>
                                        {body}
                                    </div>
                                }
                            });

                            view! {
                                <div class="pg-search-results">
                                    <div class="pg-search-timing el-mono">{timing}</div>
                                    {sym_group}
                                    {mem_group}
                                </div>
                            }.into_any()
                        }
                    })}
                </Suspense>
            </div>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn search_payload() -> serde_json::Value {
        serde_json::json!({
            "count": 2,
            "symbol_name": "SemanticClassifier",
            "definitions": [
                {"file_path": "crates/touring-hooks/src/semantic.rs", "line": 42,
                 "column": 8, "symbol_name": "SemanticClassifier", "is_definition": true},
                {"file_path": "crates/touring-server/src/lib.rs", "line": 7,
                 "symbol_name": "SemanticClassifier", "is_definition": false}
            ]
        })
    }

    #[test]
    fn parse_definitions_real_shape() {
        let rows = parse_definitions(&search_payload());
        assert_eq!(rows.len(), 2);
        assert_eq!(
            rows[0],
            (
                "SemanticClassifier".to_string(),
                "crates/touring-hooks/src/semantic.rs".to_string(),
                42
            )
        );
        assert_eq!(rows[1].2, 7);
    }

    #[test]
    fn parse_definitions_malformed_is_empty() {
        assert!(parse_definitions(&serde_json::Value::Null).is_empty());
        assert!(parse_definitions(&serde_json::json!({"definitions": "nope"})).is_empty());
        assert!(parse_definitions(&serde_json::json!({"count": 0})).is_empty());
    }

    #[test]
    fn parse_definitions_defaults_missing_fields() {
        let rows = parse_definitions(&serde_json::json!({"definitions": [{}]}));
        assert_eq!(rows, vec![(String::new(), String::new(), 0)]);
    }

    #[test]
    fn parse_recall_entries_real_shape() {
        let v = serde_json::json!({
            "ann_results": 3,
            "count": 1,
            "entries": [{"key": "outcome:bash:x", "source_db": "fts",
                          "tier": "reference", "type": "lesson", "value": "body"}],
            "diagnostics": [{"code": "M-510", "message": "rrf", "severity": "info"}]
        });
        let rows = parse_recall_entries(&v);
        assert_eq!(
            rows,
            vec![(
                "outcome:bash:x".to_string(),
                "reference".to_string(),
                "lesson".to_string()
            )]
        );
        assert_eq!(recall_values(&v), vec!["body".to_string()]);
    }

    #[test]
    fn parse_recall_entries_malformed_is_empty() {
        assert!(parse_recall_entries(&serde_json::Value::Null).is_empty());
        assert!(parse_recall_entries(&serde_json::json!({"entries": 7})).is_empty());
    }

    #[test]
    fn recall_entries_stay_aligned_with_values() {
        let v = serde_json::json!({"entries": [{"key": "a"}, {"key": "b", "value": "vb"}]});
        let rows = parse_recall_entries(&v);
        let vals = recall_values(&v);
        assert_eq!(rows.len(), vals.len());
        assert_eq!(vals, vec![String::new(), "vb".to_string()]);
    }

    #[test]
    fn truncate_chars_is_utf8_safe() {
        assert_eq!(truncate_chars("héllo wörld", 5), "héll…");
        assert_eq!(truncate_chars("short", 64), "short");
        assert_eq!(truncate_chars("", 4), "");
    }

    #[test]
    fn short_path_prefers_crates_segment() {
        assert_eq!(
            short_path("/home/u/.claude/rust/crates/touring-hooks/src/semantic.rs"),
            "crates/touring-hooks/src/semantic.rs"
        );
        assert_eq!(short_path("a/b/c/d.rs"), "c/d.rs");
        assert_eq!(short_path("lib.rs"), "lib.rs");
    }

    #[test]
    fn elapsed_ms_clamps_negative_and_rounds() {
        assert_eq!(elapsed_ms(10.0, 4.0), 0);
        assert_eq!(elapsed_ms(0.0, 12.6), 13);
        assert_eq!(elapsed_ms(5.0, 5.0), 0);
    }
}
