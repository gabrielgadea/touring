//! /wiring — module-level integration scores. Elite W3 (SPEC 2026-06-12
//! §5.8): composed hero stat, KPI strip, filterable/sortable/paginated
//! module table and an honest chains panel. CSS scope: `.pg-wiring-*`
//! composing the shared `.el-*` primitives (the `wir-*` dialect died
//! with this migration).
//!
//! Real data only (zero placeholders):
//! - `GET /api/wiring/modules` → `[{"file_path": String, "integration_score": f64,
//!   "orphan_count": u64, "orphan_symbols": [String], "total_pub_symbols": u64}]`
//!   (array of hundreds of entries, shape captured live 2026-06-12)
//! - `GET /api/wiring/chains`  → `{"chain_count": N, "rebuilt": bool, "chains"?: [...]}`
//!   (shape captured live 2026-06-12: `{"chain_count":0,"rebuilt":true}`)
//! - `GET /api/viz/wiring/svg` → server-rendered graphviz SVG (secondary panel;
//!   honest empty state when `dot` is not installed).

use leptos::prelude::*;
use leptos_meta::Title;
use serde_json::Value;

use crate::web::components::{KpiCell, KpiStrip, PageHero, Panel, ProgressTrack};
use crate::web::ctx::use_refresh_bus;
use crate::web::services::{fetch_viz_svg, fetch_wiring_chains, fetch_wiring_modules};

/// Rows rendered per table page (SPEC §4.4 — large arrays are paginated).
const PAGE_SIZE: usize = 50;

/// Tail length (chars) shown for module paths; full path lives in `title`.
const PATH_TAIL_CHARS: usize = 48;

/// One row of `/api/wiring/modules`, defensively parsed.
#[derive(Clone, Debug, PartialEq)]
pub struct ModuleRow {
    /// Workspace-relative module path (e.g. `crates/x/src/lib.rs`).
    pub file_path: String,
    /// Integration score in `[0, 1]`; `1.0` = fully wired.
    pub integration_score: f64,
    /// Orphan pub symbols in this module.
    pub orphan_count: u64,
    /// Total pub symbols exported by this module.
    pub total_pub_symbols: u64,
}

/// Parse the `/api/wiring/modules` array, skipping malformed entries.
/// Non-array payloads (error objects, null) yield an empty vec.
pub fn parse_modules(json: &Value) -> Vec<ModuleRow> {
    json.as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|m| {
                    let file_path = m.get("file_path")?.as_str()?.to_string();
                    Some(ModuleRow {
                        file_path,
                        integration_score: m
                            .get("integration_score")
                            .and_then(Value::as_f64)
                            .unwrap_or(0.0),
                        orphan_count: m.get("orphan_count").and_then(Value::as_u64).unwrap_or(0),
                        total_pub_symbols: m
                            .get("total_pub_symbols")
                            .and_then(Value::as_u64)
                            .unwrap_or(0),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// KPI tuple for the strip: `(modules, fully_integrated, with_orphans, pub_symbols)`.
pub fn kpis_of(rows: &[ModuleRow]) -> (usize, usize, usize, u64) {
    let total = rows.len();
    let fully = rows.iter().filter(|m| m.integration_score >= 1.0).count();
    let with_orphans = rows.iter().filter(|m| m.orphan_count > 0).count();
    let pub_sum = rows.iter().map(|m| m.total_pub_symbols).sum();
    (total, fully, with_orphans, pub_sum)
}

/// Last `n` chars of a path with an `…` prefix when truncated
/// (char-boundary safe; full path is rendered via `title`).
pub fn path_tail(path: &str, n: usize) -> String {
    let count = path.chars().count();
    if count <= n {
        path.to_string()
    } else {
        let tail: String = path.chars().skip(count - n).collect();
        format!("\u{2026}{tail}")
    }
}

/// Case-insensitive substring filter + sort.
///
/// `sort` keys: `"score"` = integration ascending (worst modules first,
/// ties broken by most orphans), `"pubs"` = pub-symbol count descending.
pub fn filter_sort(rows: &[ModuleRow], query: &str, sort: &str) -> Vec<ModuleRow> {
    let q = query.trim().to_lowercase();
    let mut out: Vec<ModuleRow> = rows
        .iter()
        .filter(|m| q.is_empty() || m.file_path.to_lowercase().contains(&q))
        .cloned()
        .collect();
    match sort {
        "pubs" => out.sort_by(|a, b| {
            b.total_pub_symbols
                .cmp(&a.total_pub_symbols)
                .then_with(|| a.file_path.cmp(&b.file_path))
        }),
        _ => out.sort_by(|a, b| {
            a.integration_score
                .total_cmp(&b.integration_score)
                .then_with(|| b.orphan_count.cmp(&a.orphan_count))
                .then_with(|| a.file_path.cmp(&b.file_path))
        }),
    }
    out
}

/// Human label for one entry of the `chains` array (string, `{source, sink}`
/// object, or arbitrary JSON rendered compactly).
fn chain_label(v: &Value) -> String {
    if let Some(s) = v.as_str() {
        return s.to_string();
    }
    let source = v.get("source").and_then(Value::as_str);
    let sink = v.get("sink").and_then(Value::as_str);
    match (source, sink) {
        (Some(a), Some(b)) => format!("{a} \u{2192} {b}"),
        _ => v.to_string(),
    }
}

/// `/wiring` — module integration scores across the workspace.
#[component]
pub fn WiringGraph() -> impl IntoView {
    let bus = use_refresh_bus();
    let modules = LocalResource::new(move || {
        let _tick = bus.tick.get();
        async move {
            match fetch_wiring_modules().await {
                Ok(v) => parse_modules(&v),
                Err(_) => Vec::new(),
            }
        }
    });
    let chains = LocalResource::new(move || {
        let _tick = bus.tick.get();
        async move { fetch_wiring_chains().await.ok() }
    });
    let svg = LocalResource::new(move || {
        let _tick = bus.tick.get();
        async move { fetch_viz_svg().await.ok() }
    });

    // Re-init pan/zoom once the (optional) SVG panel resolves.
    Effect::new(move |_| {
        if svg.get().is_some() {
            #[cfg(target_arch = "wasm32")]
            {
                let _ = js_sys::eval("window.initPanZoom && window.initPanZoom()");
            }
        }
    });

    // ── KPI reshape (SPEC §4.4 — Memo, not per-cell recompute) ────────
    let kpis = Memo::new(move |_| modules.get().map(|rows| kpis_of(&rows)));
    let modules_kpi = Signal::derive(move || {
        kpis.get()
            .map(|k| k.0.to_string())
            .unwrap_or_else(|| "\u{2026}".to_string())
    });
    let fully_kpi = Signal::derive(move || {
        kpis.get()
            .map(|k| k.1.to_string())
            .unwrap_or_else(|| "\u{2026}".to_string())
    });
    let orphaned_kpi = Signal::derive(move || {
        kpis.get()
            .map(|k| k.2.to_string())
            .unwrap_or_else(|| "\u{2026}".to_string())
    });
    let pubs_kpi = Signal::derive(move || {
        kpis.get()
            .map(|k| k.3.to_string())
            .unwrap_or_else(|| "\u{2026}".to_string())
    });

    // ── filter + sort + page state (Memo chain over the big array) ────
    let filter = RwSignal::new(String::new());
    let sort_key = RwSignal::new("score".to_string());
    let page = RwSignal::new(0_usize);

    let filtered = Memo::new(move |_| {
        let rows = modules.get().unwrap_or_default();
        filter_sort(&rows, &filter.get(), &sort_key.get())
    });
    let page_count = Memo::new(move |_| filtered.get().len().div_ceil(PAGE_SIZE).max(1));
    let page_rows = Memo::new(move |_| {
        let rows = filtered.get();
        let p = page.get().min(page_count.get().saturating_sub(1));
        rows.into_iter()
            .skip(p * PAGE_SIZE)
            .take(PAGE_SIZE)
            .collect::<Vec<_>>()
    });

    view! {
        <div class="page pg-wiring">
            <Title text="Touring — Wiring"/>
            <div class="el-main-editorial">
                <PageHero
                    eyebrow="CODE INTELLIGENCE · WIRING"
                    title="Wiring"
                    title_em="modules"
                    sub="Per-module integration scores from the wiring graph. A module scores 1.00 when every pub symbol has a consumer — orphans pull the score down and mark the next wiring opportunity."
                    stat=modules_kpi
                    stat_label="MODULES"
                >
                    <a class="el-btn" href="/orphans">"Orphans"</a>
                    <a class="el-btn" href="/chains">"Chains"</a>
                </PageHero>

                <KpiStrip>
                    <KpiCell label="Modules" value=modules_kpi sub="in workspace"/>
                    <KpiCell label="Fully integrated" value=fully_kpi sub="score = 1.00"/>
                    <KpiCell label="With orphans" value=orphaned_kpi sub="orphan_count > 0"/>
                    <KpiCell label="Pub symbols" value=pubs_kpi sub="total exported"/>
                </KpiStrip>

                // ── 01 · module table (filter + sort + pagination) ────────
                <Panel num="01" eyebrow="Modules" cmd="touring wiring modules -j">
                    <div class="pg-wiring-toolbar">
                        <input
                            class="pg-wiring-filter"
                            type="text"
                            placeholder="Filter by module path\u{2026}"
                            prop:value=move || filter.get()
                            on:input=move |ev| {
                                filter.set(event_target_value(&ev));
                                page.set(0);
                            }
                        />
                        <select
                            class="pg-wiring-sort"
                            prop:value=move || sort_key.get()
                            on:change=move |ev| {
                                sort_key.set(event_target_value(&ev));
                                page.set(0);
                            }
                        >
                            <option value="score">"Integration · worst first"</option>
                            <option value="pubs">"Pub symbols · most first"</option>
                        </select>
                    </div>
                    <Suspense fallback=|| view! { <div class="el-skeleton" aria-label="loading"></div> }>
                        {move || modules.get().map(|_| {
                            let rows = page_rows.get();
                            let total_filtered = filtered.get().len();
                            let body = if rows.is_empty() {
                                view! {
                                    <div class="el-empty">"No modules match the current filter."</div>
                                }.into_any()
                            } else {
                                view! {
                                    <table class="el-table pg-wiring-table">
                                        <thead>
                                            <tr>
                                                <th>"Module"</th>
                                                <th>"Integration"</th>
                                                <th>"Orphans"</th>
                                                <th class="pg-wiring-th-right">"Pub symbols"</th>
                                            </tr>
                                        </thead>
                                        <tbody>
                                            {rows.into_iter().map(|m| {
                                                let score = m.integration_score;
                                                let color = if score < 1.0 {
                                                    "var(--el-warn)"
                                                } else {
                                                    "var(--el-accent)"
                                                };
                                                let tail = path_tail(&m.file_path, PATH_TAIL_CHARS);
                                                let full = m.file_path.clone();
                                                let orphans = m.orphan_count;
                                                view! {
                                                    <tr>
                                                        <td>
                                                            <span class="el-mono pg-wiring-path" title=full>{tail}</span>
                                                        </td>
                                                        <td class="pg-wiring-prog-col">
                                                            <ProgressTrack
                                                                value=Signal::derive(move || score)
                                                                color=color
                                                                show_value=true
                                                            />
                                                        </td>
                                                        <td>
                                                            {if orphans > 0 {
                                                                view! {
                                                                    <span class="el-tag el-tag-warn">{orphans.to_string()}</span>
                                                                }.into_any()
                                                            } else {
                                                                view! {
                                                                    <span class="pg-wiring-zero">"0"</span>
                                                                }.into_any()
                                                            }}
                                                        </td>
                                                        <td class="el-mono pg-wiring-pubs">{m.total_pub_symbols.to_string()}</td>
                                                    </tr>
                                                }
                                            }).collect_view()}
                                        </tbody>
                                    </table>
                                }.into_any()
                            };
                            view! {
                                <div>
                                    {body}
                                    <div class="pg-wiring-pager">
                                        <button
                                            class="el-btn el-btn-sm"
                                            on:click=move |_| page.update(|p| *p = p.saturating_sub(1))
                                        >
                                            "\u{2039} Prev"
                                        </button>
                                        <span class="el-mono pg-wiring-page-label">
                                            {move || format!(
                                                "page {} / {}",
                                                page.get().min(page_count.get().saturating_sub(1)) + 1,
                                                page_count.get(),
                                            )}
                                        </span>
                                        <button
                                            class="el-btn el-btn-sm"
                                            on:click=move |_| {
                                                let max = page_count.get_untracked().saturating_sub(1);
                                                page.update(|p| {
                                                    if *p < max {
                                                        *p += 1;
                                                    }
                                                });
                                            }
                                        >
                                            "Next \u{203a}"
                                        </button>
                                        <span class="pg-wiring-pager-total">
                                            {format!("{total_filtered} filtered")}
                                        </span>
                                    </div>
                                </div>
                            }.into_any()
                        })}
                    </Suspense>
                </Panel>

                <section class="pg-wiring-secondary">
                    // ── 02 · functional chains (honest empty state) ───────
                    <Panel num="02" eyebrow="Chains" cmd="touring wiring chains">
                        <Suspense fallback=|| view! { <div class="el-skeleton" aria-label="loading"></div> }>
                            {move || chains.get().map(|payload| {
                                let count = payload
                                    .as_ref()
                                    .and_then(|v| v.get("chain_count"))
                                    .and_then(Value::as_u64)
                                    .unwrap_or(0);
                                if count == 0 {
                                    view! {
                                        <div class="el-empty">
                                            "No functional chains recorded — run touring wiring chains --rebuild."
                                        </div>
                                    }.into_any()
                                } else {
                                    let items: Vec<String> = payload
                                        .as_ref()
                                        .and_then(|v| v.get("chains"))
                                        .and_then(Value::as_array)
                                        .map(|arr| arr.iter().map(chain_label).collect())
                                        .unwrap_or_default();
                                    view! {
                                        <div>
                                            <p class="el-mono pg-wiring-chains-count">
                                                {format!("{count} functional chains")}
                                            </p>
                                            <ul class="pg-wiring-chains-list">
                                                {items.into_iter().map(|s| view! {
                                                    <li class="el-mono pg-wiring-chain-item">{s}</li>
                                                }).collect_view()}
                                            </ul>
                                        </div>
                                    }.into_any()
                                }
                            })}
                        </Suspense>
                    </Panel>

                    // ── 03 · server-rendered SVG graph (secondary) ────────
                    <Panel num="03" eyebrow="Wiring graph · SVG" cmd="GET /api/viz/wiring/svg">
                        {move || match svg.get() {
                            None => view! {
                                <div class="el-skeleton" aria-label="loading"></div>
                            }.into_any(),
                            Some(opt) => {
                                let markup = opt.unwrap_or_default();
                                if markup.trim().is_empty() || !markup.contains("<svg") {
                                    view! {
                                        <div class="el-empty">
                                            "SVG unavailable — install graphviz dot for server-side rendering."
                                        </div>
                                    }.into_any()
                                } else {
                                    view! {
                                        <div class="pg-wiring-svg" inner_html=markup></div>
                                    }.into_any()
                                }
                            }
                        }}
                    </Panel>
                </section>
            </div>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sample_rows() -> Vec<ModuleRow> {
        parse_modules(&json!([
            {
                "file_path": "crates/a/src/lib.rs",
                "integration_score": 1.0,
                "orphan_count": 0,
                "orphan_symbols": [],
                "total_pub_symbols": 3
            },
            {
                "file_path": "crates/b/src/hooks.rs",
                "integration_score": 0.5,
                "orphan_count": 2,
                "orphan_symbols": ["x", "y"],
                "total_pub_symbols": 4
            },
            {
                "file_path": "crates/c/src/mod.rs",
                "integration_score": 0.0,
                "orphan_count": 1,
                "orphan_symbols": ["z"],
                "total_pub_symbols": 1
            }
        ]))
    }

    #[test]
    fn parse_modules_real_shape() {
        let rows = sample_rows();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[1].file_path, "crates/b/src/hooks.rs");
        assert_eq!(rows[1].integration_score, 0.5);
        assert_eq!(rows[1].orphan_count, 2);
        assert_eq!(rows[1].total_pub_symbols, 4);
    }

    #[test]
    fn parse_modules_skips_malformed_and_non_array() {
        assert!(parse_modules(&json!({"error": "boom"})).is_empty());
        assert!(parse_modules(&json!(null)).is_empty());
        // Entries without file_path are skipped, valid siblings survive.
        let rows = parse_modules(&json!([
            {"integration_score": 0.7},
            {"file_path": "crates/ok/src/lib.rs"}
        ]));
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].file_path, "crates/ok/src/lib.rs");
        assert_eq!(rows[0].integration_score, 0.0);
    }

    #[test]
    fn kpis_of_counts_and_sums() {
        let rows = sample_rows();
        assert_eq!(kpis_of(&rows), (3, 1, 2, 8));
        assert_eq!(kpis_of(&[]), (0, 0, 0, 0));
    }

    #[test]
    fn path_tail_short_and_truncated() {
        assert_eq!(path_tail("src/lib.rs", 48), "src/lib.rs");
        let long = "x".repeat(60);
        let tail = path_tail(&long, 48);
        assert!(tail.starts_with('\u{2026}'));
        assert_eq!(tail.chars().count(), 49);
        assert!(tail.ends_with(&"x".repeat(48)));
    }

    #[test]
    fn filter_sort_score_asc_and_pubs_desc() {
        let rows = sample_rows();
        let worst_first = filter_sort(&rows, "", "score");
        assert_eq!(worst_first[0].file_path, "crates/c/src/mod.rs");
        assert_eq!(worst_first[2].integration_score, 1.0);

        let pubs_desc = filter_sort(&rows, "", "pubs");
        assert_eq!(pubs_desc[0].total_pub_symbols, 4);

        let filtered = filter_sort(&rows, "HOOKS", "score");
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].file_path, "crates/b/src/hooks.rs");
    }

    #[test]
    fn chain_label_variants() {
        assert_eq!(chain_label(&json!("a -> b")), "a -> b");
        assert_eq!(
            chain_label(&json!({"source": "core", "sink": "cli"})),
            "core \u{2192} cli"
        );
        assert_eq!(chain_label(&json!(7)), "7");
    }
}
