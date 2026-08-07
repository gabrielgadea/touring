//! /orphans — Orphan registry. Elite W3 (SPEC 2026-06-12 §5.7):
//! composed hero stat (orphan_count, thousands-separated), KPI strip
//! derived from real `symbol_kind` values, a filterable + paginated
//! registry table (1,531 rows — client-side `Memo` filter, 50 per
//! page, SPEC §4.4) and an inspect rail wired to the live
//! `/api/wiring/suggest` and `/api/wiring/impact` endpoints. For a
//! true orphan both impact counters come back 0 — that zero IS the
//! visual proof of orphan-ness, so the rail labels it instead of
//! hiding it.
//!
//! Verified wire shapes (live captures 2026-06-12):
//! `/api/orphans` → `{orphan_count, orphans: [{module_file,
//! symbol_kind, symbol_name, visibility}], dead_patterns: []}` ·
//! `/api/wiring/suggest?symbol=` → `{count, note, orphan_symbol,
//! suggestions: []}` (the note can honestly say the symbol is missing
//! from the wiring_map — render it verbatim) ·
//! `/api/wiring/impact?symbol=&depth=2` → `{symbol, direct_consumers,
//! total_transitive, max_depth, paths: [{consumer_module,
//! consumer_symbol, depth, fan_out, path_type}]}`.

use leptos::prelude::*;
use leptos_meta::Title;

use crate::web::components::{KpiCell, KpiStrip, PageHero, Panel};
use crate::web::ctx::use_refresh_bus;
use crate::web::models::orphan::{OrphanReport, OrphanSymbol};
use crate::web::services::{fetch_orphans, fetch_wiring_impact, fetch_wiring_suggest};

/// Rows per registry page — 1,531 orphans stay snappy client-side.
const PAGE_SIZE: usize = 50;

/// Module path display cap — tail characters kept (full path in `title`).
const MODULE_TAIL_CHARS: usize = 40;

/// Impact paths rendered in the rail.
const MAX_IMPACT_PATHS: usize = 5;

/// BFS depth used by the impact probe.
const IMPACT_DEPTH: u8 = 2;

/// Case-insensitive registry filter: `text` matches `symbol_name` OR
/// `module_file` (substring); `kind` is an exact `symbol_kind` match
/// (empty string = all kinds).
pub fn filter_orphans(orphans: &[OrphanSymbol], text: &str, kind: &str) -> Vec<OrphanSymbol> {
    let needle = text.trim().to_lowercase();
    orphans
        .iter()
        .filter(|o| {
            let kind_ok = kind.is_empty() || o.kind.as_deref().unwrap_or("") == kind;
            let text_ok = needle.is_empty()
                || o.symbol.to_lowercase().contains(&needle)
                || o.file_path.to_lowercase().contains(&needle);
            kind_ok && text_ok
        })
        .cloned()
        .collect()
}

/// KPI counts straight from the raw `/api/orphans` payload:
/// `(functions, methods, distinct module files)`.
pub fn kind_counts(report: &serde_json::Value) -> (u64, u64, u64) {
    let mut functions = 0u64;
    let mut methods = 0u64;
    let mut files = std::collections::BTreeSet::new();
    if let Some(arr) = report.get("orphans").and_then(|v| v.as_array()) {
        for o in arr {
            match o.get("symbol_kind").and_then(|k| k.as_str()) {
                Some("function") => functions += 1,
                Some("method") => methods += 1,
                _ => {}
            }
            if let Some(f) = o.get("module_file").and_then(|f| f.as_str()) {
                files.insert(f.to_string());
            }
        }
    }
    (functions, methods, files.len() as u64)
}

/// Client-side pagination window: `(start, end, total_pages)` with the
/// page index clamped into range (refilters can shrink the list).
pub fn page_slice(len: usize, page: usize, per: usize) -> (usize, usize, usize) {
    let per = per.max(1);
    let total_pages = if len == 0 { 1 } else { len.div_ceil(per) };
    let page = page.min(total_pages - 1);
    let start = page * per;
    let end = (start + per).min(len);
    (start, end, total_pages)
}

/// pt-BR thousands separator (`'.'`): `1531` → `"1.531"`.
pub fn fmt_thousands(n: u64) -> String {
    let digits = n.to_string();
    let len = digits.len();
    let mut out = String::with_capacity(len + len / 3);
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (len - i).is_multiple_of(3) {
            out.push('.');
        }
        out.push(ch);
    }
    out
}

/// Char-boundary-safe module tail: keeps the last `max - 1` chars with
/// a leading ellipsis; the full path belongs in the `title` attribute.
pub fn module_tail(path: &str, max: usize) -> String {
    let count = path.chars().count();
    if count <= max {
        return path.to_string();
    }
    let keep = max.saturating_sub(1);
    let tail: String = path.chars().skip(count - keep).collect();
    format!("…{tail}")
}

/// Suggest rail body — real suggestions when present, otherwise the
/// endpoint's own honest note (e.g. "orphan_symbol not found in
/// wiring_map — run touring index rebuild first").
fn render_suggest(v: &serde_json::Value) -> AnyView {
    if v.is_null() {
        return view! {
            <div class="el-empty">"Suggest endpoint unavailable — nothing to show."</div>
        }
        .into_any();
    }
    let suggestions = v
        .get("suggestions")
        .and_then(|s| s.as_array())
        .cloned()
        .unwrap_or_default();
    if !suggestions.is_empty() {
        return view! {
            <div class="pg-orphans-suggest">
                {suggestions.into_iter().map(|s| {
                    let text = match s {
                        serde_json::Value::String(t) => t,
                        other => serde_json::to_string(&other).unwrap_or_default(),
                    };
                    view! {
                        <div class="el-row pg-orphans-suggest-row">
                            <span class="el-mono">{text}</span>
                        </div>
                    }
                }).collect_view()}
            </div>
        }
        .into_any();
    }
    let note = v
        .get("note")
        .and_then(|n| n.as_str())
        .unwrap_or("")
        .to_string();
    if note.is_empty() {
        view! {
            <div class="el-empty">"No suggestions returned for this symbol."</div>
        }
        .into_any()
    } else {
        // The daemon's own diagnostic — rendered verbatim (honesty over
        // decoration; it usually points at `touring index rebuild`).
        view! { <div class="el-empty pg-orphans-suggest-note">{note}</div> }.into_any()
    }
}

/// Impact rail body — BFS counters as rows plus up to
/// [`MAX_IMPACT_PATHS`] consumer paths. All-zero counters are labelled
/// as the orphan proof, never hidden.
fn render_impact(v: &serde_json::Value) -> AnyView {
    if v.is_null() {
        return view! {
            <div class="el-empty">"Impact endpoint unavailable — counters cannot be shown."</div>
        }
        .into_any();
    }
    let direct = v
        .get("direct_consumers")
        .and_then(|x| x.as_u64())
        .unwrap_or(0);
    let total = v
        .get("total_transitive")
        .and_then(|x| x.as_u64())
        .unwrap_or(0);
    let depth = v.get("max_depth").and_then(|x| x.as_u64()).unwrap_or(0);
    let paths = v
        .get("paths")
        .and_then(|p| p.as_array())
        .cloned()
        .unwrap_or_default();
    view! {
        <div class="pg-orphans-impact">
            <div class="el-row pg-orphans-impact-row">
                <span class="pg-orphans-impact-label">"Direct consumers"</span>
                <span class="el-mono">{direct.to_string()}</span>
            </div>
            <div class="el-row pg-orphans-impact-row">
                <span class="pg-orphans-impact-label">"Total transitive"</span>
                <span class="el-mono">{total.to_string()}</span>
            </div>
            <div class="el-row pg-orphans-impact-row">
                <span class="pg-orphans-impact-label">"Max depth"</span>
                <span class="el-mono">{depth.to_string()}</span>
            </div>
            {(direct == 0 && total == 0).then(|| view! {
                <div class="pg-orphans-proof">
                    <span class="el-tag el-tag-on">"0 consumers confirms orphan status"</span>
                </div>
            })}
            {(!paths.is_empty()).then(|| view! {
                <div class="pg-orphans-paths">
                    <div class="el-eyebrow pg-orphans-paths-label">"Consumer paths"</div>
                    {paths.into_iter().take(MAX_IMPACT_PATHS).map(|p| {
                        let module = p.get("consumer_module")
                            .and_then(|m| m.as_str())
                            .unwrap_or("")
                            .to_string();
                        let symbol = p.get("consumer_symbol")
                            .and_then(|s| s.as_str())
                            .unwrap_or("")
                            .to_string();
                        let d = p.get("depth").and_then(|d| d.as_u64()).unwrap_or(0);
                        view! {
                            <div class="el-row pg-orphans-path-row">
                                <span class="el-mono pg-orphans-path-module" title=module.clone()>
                                    {module_tail(&module, MODULE_TAIL_CHARS)}
                                </span>
                                <span class="el-mono">{symbol}</span>
                                <span class="el-tag">{format!("d{d}")}</span>
                            </div>
                        }
                    }).collect_view()}
                </div>
            })}
        </div>
    }
    .into_any()
}

/// Orphan registry page — hero · KPI strip · registry/inspect grid.
#[component]
pub fn OrphanList() -> impl IntoView {
    let bus = use_refresh_bus();

    // Raw payload kept as JSON — the KPI helper reads wire fields and
    // the hero/download stay aligned with exactly what the API served.
    let res = LocalResource::new(move || {
        let _tick = bus.tick.get();
        async move {
            fetch_orphans()
                .await
                .ok()
                .unwrap_or(serde_json::Value::Null)
        }
    });

    // Parsed registry rows (serde aliases tolerate legacy field names).
    let rows: Signal<Vec<OrphanSymbol>> = Signal::derive(move || {
        res.get()
            .and_then(|v| serde_json::from_value::<OrphanReport>(v).ok())
            .map(|r| r.orphans)
            .unwrap_or_default()
    });

    // Total from the wire (`orphan_count`) — None until the fetch lands.
    let total_count: Signal<Option<u64>> = Signal::derive(move || {
        res.get()
            .as_ref()
            .and_then(|v| v.get("orphan_count"))
            .and_then(|c| c.as_u64())
    });
    let hero_stat = Signal::derive(move || {
        total_count
            .get()
            .map(fmt_thousands)
            .unwrap_or_else(|| "\u{2014}".to_string())
    });

    // (functions, methods, distinct files) — memoized over the payload.
    let kinds_memo: Memo<Option<(u64, u64, u64)>> =
        Memo::new(move |_| res.get().map(|v| kind_counts(&v)));
    let kpi_functions = Signal::derive(move || {
        kinds_memo
            .get()
            .map(|k| fmt_thousands(k.0))
            .unwrap_or_else(|| "\u{2014}".to_string())
    });
    let kpi_methods = Signal::derive(move || {
        kinds_memo
            .get()
            .map(|k| fmt_thousands(k.1))
            .unwrap_or_else(|| "\u{2014}".to_string())
    });
    let kpi_files = Signal::derive(move || {
        kinds_memo
            .get()
            .map(|k| fmt_thousands(k.2))
            .unwrap_or_else(|| "\u{2014}".to_string())
    });

    // Registry state — text + kind filters reset the page; the
    // selection stores the row itself (symbol/kind/module/visibility),
    // NOT a list index, so it survives refilters and repagination.
    let (filter_text, set_filter_text) = signal(String::new());
    let (kind_sel, set_kind_sel) = signal(String::new());
    let page = RwSignal::new(0usize);
    let selected: RwSignal<Option<(String, String, String, String)>> = RwSignal::new(None);

    // Distinct kinds actually present in the payload (real options).
    let kind_options: Memo<Vec<String>> = Memo::new(move |_| {
        let mut set = std::collections::BTreeSet::new();
        for o in rows.get() {
            if let Some(k) = o.kind
                && !k.is_empty()
            {
                set.insert(k);
            }
        }
        set.into_iter().collect()
    });

    // Memoized filter over 1,531 rows (SPEC §4.4) — String tuples so
    // the memo can diff: (symbol, kind, module, visibility).
    let filtered: Memo<Vec<(String, String, String, String)>> = Memo::new(move |_| {
        filter_orphans(&rows.get(), &filter_text.get(), &kind_sel.get())
            .into_iter()
            .map(|o| {
                (
                    o.symbol,
                    o.kind.unwrap_or_default(),
                    o.file_path,
                    o.visibility.unwrap_or_default(),
                )
            })
            .collect()
    });

    // Memoized page window: (visible rows, clamped page, total pages).
    #[allow(clippy::type_complexity)]
    let page_view: Memo<(Vec<(String, String, String, String)>, usize, usize)> =
        Memo::new(move |_| {
            let all = filtered.get();
            let (start, end, total_pages) = page_slice(all.len(), page.get(), PAGE_SIZE);
            (all[start..end].to_vec(), start / PAGE_SIZE, total_pages)
        });

    let filtered_label = Signal::derive(move || {
        let shown = filtered.get().len();
        format!("{} shown", fmt_thousands(shown as u64))
    });

    // Rail probes — keyed on the selected symbol (read OUTSIDE the
    // async block so the resource tracks selection changes).
    let suggest = LocalResource::new(move || {
        let sym = selected.get().map(|s| s.0);
        async move {
            match sym {
                Some(s) if !s.is_empty() => fetch_wiring_suggest(Some(&s))
                    .await
                    .ok()
                    .unwrap_or(serde_json::Value::Null),
                _ => serde_json::Value::Null,
            }
        }
    });
    let impact = LocalResource::new(move || {
        let sym = selected.get().map(|s| s.0);
        async move {
            match sym {
                Some(s) if !s.is_empty() => fetch_wiring_impact(&s, IMPACT_DEPTH)
                    .await
                    .ok()
                    .unwrap_or(serde_json::Value::Null),
                _ => serde_json::Value::Null,
            }
        }
    });

    view! {
        <div class="page pg-orphans">
            <Title text="Touring — Orphans"/>
            <div class="el-main-editorial">
                <PageHero
                    eyebrow="CODE INTELLIGENCE · ORPHANS"
                    title="Orphans"
                    title_em="registry"
                    sub="Pub symbols with no registered consumers. Each row is a wiring opportunity (REGRA #0: potencializar) — pick one, ask the daemon for suggestions and probe its blast radius. For a true orphan the impact counters come back 0: that zero is the proof."
                    stat=hero_stat
                    stat_label="PUB SYMBOLS"
                >
                    // Live JSON report — same payload this page renders.
                    <a class="el-btn" href="/api/orphans" download="orphans-report.json">
                        "Download JSON"
                    </a>
                    <a class="el-btn" href="/wiring">"Wiring"</a>
                </PageHero>

                <KpiStrip>
                    <KpiCell label="Total" value=hero_stat sub="orphan pub symbols"/>
                    <KpiCell label="Functions" value=kpi_functions sub="symbol_kind = function"/>
                    <KpiCell label="Methods" value=kpi_methods sub="symbol_kind = method"/>
                    <KpiCell label="Files" value=kpi_files sub="distinct module files"/>
                </KpiStrip>

                <section class="pg-orphans-grid">
                    // ── 01 · registry — filter, kind select, 50/page ──
                    <Panel num="01" eyebrow="Registry" cmd="touring wiring orphans -j">
                        <div class="pg-orphans-controls">
                            <input
                                type="text"
                                class="pg-orphans-filter"
                                placeholder="Filter by symbol or module…"
                                prop:value=move || filter_text.get()
                                on:input=move |ev| {
                                    set_filter_text.set(event_target_value(&ev));
                                    page.set(0);
                                }
                            />
                            <select
                                class="pg-orphans-kind"
                                prop:value=move || kind_sel.get()
                                on:change=move |ev| {
                                    set_kind_sel.set(event_target_value(&ev));
                                    page.set(0);
                                }
                            >
                                <option value="">"all kinds"</option>
                                {move || kind_options.get().into_iter().map(|k| {
                                    let value = k.clone();
                                    view! { <option value=value>{k}</option> }
                                }).collect_view()}
                            </select>
                            <span class="el-mono pg-orphans-shown">
                                {move || filtered_label.get()}
                            </span>
                        </div>

                        <Suspense fallback=|| view! {
                            <div class="el-skeleton" aria-label="loading orphans"></div>
                        }>
                            {move || res.get().map(|_| view! {
                                <table class="el-table pg-orphans-table">
                                    <thead>
                                        <tr>
                                            <th>"Symbol"</th>
                                            <th>"Kind"</th>
                                            <th>"Module"</th>
                                        </tr>
                                    </thead>
                                    <tbody>
                                        {move || {
                                            let (rows_page, _, _) = page_view.get();
                                            if rows_page.is_empty() {
                                                view! {
                                                    <tr>
                                                        <td colspan="3">
                                                            <div class="el-empty">
                                                                "No orphans match the current filter."
                                                            </div>
                                                        </td>
                                                    </tr>
                                                }.into_any()
                                            } else {
                                                rows_page.into_iter().map(|(sym, kind, module, vis)| {
                                                    let sym_disp = sym.clone();
                                                    let kind_disp = if kind.is_empty() {
                                                        "unknown".to_string()
                                                    } else {
                                                        kind.clone()
                                                    };
                                                    let module_full = module.clone();
                                                    let module_disp =
                                                        module_tail(&module, MODULE_TAIL_CHARS);
                                                    let sym_active = sym.clone();
                                                    let module_active = module.clone();
                                                    let row = (sym, kind, module, vis);
                                                    view! {
                                                        <tr
                                                            class="clickable"
                                                            class:active=move || {
                                                                selected.get()
                                                                    .map(|s| {
                                                                        s.0 == sym_active
                                                                            && s.2 == module_active
                                                                    })
                                                                    .unwrap_or(false)
                                                            }
                                                            on:click=move |_| {
                                                                selected.set(Some(row.clone()))
                                                            }
                                                        >
                                                            <td class="el-mono pg-orphans-sym">
                                                                {sym_disp}
                                                            </td>
                                                            <td>
                                                                <span class="el-tag">{kind_disp}</span>
                                                            </td>
                                                            <td
                                                                class="el-mono pg-orphans-module"
                                                                title=module_full
                                                            >
                                                                {module_disp}
                                                            </td>
                                                        </tr>
                                                    }
                                                }).collect_view().into_any()
                                            }
                                        }}
                                    </tbody>
                                </table>

                                <div class="pg-orphans-pager">
                                    <button
                                        class="el-btn el-btn-sm"
                                        disabled=move || page_view.get().1 == 0
                                        on:click=move |_| {
                                            page.update(|p| *p = p.saturating_sub(1))
                                        }
                                    >
                                        "Prev"
                                    </button>
                                    <span class="el-mono pg-orphans-page-label">
                                        {move || {
                                            let (_, current, total) = page_view.get();
                                            format!("page {} of {}", current + 1, total)
                                        }}
                                    </span>
                                    <button
                                        class="el-btn el-btn-sm"
                                        disabled=move || {
                                            let (_, current, total) = page_view.get();
                                            current + 1 >= total
                                        }
                                        on:click=move |_| {
                                            let (_, current, total) = page_view.get();
                                            if current + 1 < total {
                                                page.set(current + 1);
                                            }
                                        }
                                    >
                                        "Next"
                                    </button>
                                </div>
                            })}
                        </Suspense>
                    </Panel>

                    // ── inspect rail — why / suggest / impact ─────────
                    <div class="pg-orphans-rail">
                        {move || match selected.get() {
                            None => view! {
                                <div class="el-empty pg-orphans-rail-empty">
                                    "Select an orphan to inspect it."
                                </div>
                            }.into_any(),
                            Some((sym, kind, module, vis)) => {
                                let kind_disp = if kind.is_empty() {
                                    "unknown".to_string()
                                } else {
                                    kind
                                };
                                let vis_disp = if vis.is_empty() {
                                    "public".to_string()
                                } else {
                                    vis
                                };
                                view! {
                                    <Panel num="02" eyebrow="Why orphan" cmd="touring wiring orphans -j">
                                        <div class="el-card pg-orphans-card">
                                            <div class="el-mono pg-orphans-card-sym">{sym}</div>
                                            <div class="pg-orphans-card-tags">
                                                <span class="el-tag">{kind_disp}</span>
                                                <span class="el-tag">{vis_disp}</span>
                                            </div>
                                            <div class="el-mono pg-orphans-card-module">{module}</div>
                                            <p class="pg-orphans-card-note">
                                                "Pub symbol with zero consumers in the wiring map — wire it to a caller or fold it back into its module (REGRA #0)."
                                            </p>
                                        </div>
                                    </Panel>

                                    <Panel num="03" eyebrow="Suggest" cmd="touring wiring suggest <symbol>">
                                        <Suspense fallback=|| view! {
                                            <div class="el-skeleton" aria-label="loading suggestions"></div>
                                        }>
                                            {move || suggest.get().map(|v| render_suggest(&v))}
                                        </Suspense>
                                    </Panel>

                                    <Panel num="04" eyebrow="Impact" cmd="touring wiring impact <symbol> --depth 2">
                                        <Suspense fallback=|| view! {
                                            <div class="el-skeleton" aria-label="loading impact"></div>
                                        }>
                                            {move || impact.get().map(|v| render_impact(&v))}
                                        </Suspense>
                                    </Panel>
                                }.into_any()
                            }
                        }}
                    </div>
                </section>
            </div>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Vec<OrphanSymbol> {
        vec![
            OrphanSymbol {
                symbol: "record_latency".to_string(),
                file_path: "crates/touring-hooks-shared/src/latency_marker.rs".to_string(),
                kind: Some("function".to_string()),
                visibility: Some("public".to_string()),
            },
            OrphanSymbol {
                symbol: "apply_theme".to_string(),
                file_path: "crates/touring-web/src/lib.rs".to_string(),
                kind: Some("method".to_string()),
                visibility: Some("public".to_string()),
            },
            OrphanSymbol {
                symbol: "WingLayout".to_string(),
                file_path: "crates/touring-bindings/src/web/components/iso_palace.rs".to_string(),
                kind: Some("struct".to_string()),
                visibility: Some("public".to_string()),
            },
            OrphanSymbol {
                symbol: "latency_histogram".to_string(),
                file_path: "crates/touring-hooks-shared/src/latency_marker.rs".to_string(),
                kind: Some("function".to_string()),
                visibility: Some("public".to_string()),
            },
        ]
    }

    #[test]
    fn filter_orphans_text_matches_symbol_and_module() {
        let all = sample();
        // Symbol substring, case-insensitive.
        let by_symbol = filter_orphans(&all, "LATENCY", "");
        assert_eq!(by_symbol.len(), 2);
        // Module substring.
        let by_module = filter_orphans(&all, "touring-web", "");
        assert_eq!(by_module.len(), 1);
        assert_eq!(by_module[0].symbol, "apply_theme");
        // Empty text + empty kind = everything.
        assert_eq!(filter_orphans(&all, "", "").len(), 4);
    }

    #[test]
    fn filter_orphans_kind_is_exact_and_composes_with_text() {
        let all = sample();
        assert_eq!(filter_orphans(&all, "", "function").len(), 2);
        assert_eq!(filter_orphans(&all, "", "struct").len(), 1);
        assert_eq!(filter_orphans(&all, "", "trait").len(), 0);
        // Text AND kind together.
        let both = filter_orphans(&all, "record", "function");
        assert_eq!(both.len(), 1);
        assert_eq!(both[0].symbol, "record_latency");
    }

    #[test]
    fn kind_counts_from_wire_payload() {
        // Literal excerpt of the real /api/orphans shape (2026-06-12).
        let wire = serde_json::json!({
            "dead_patterns": [],
            "orphan_count": 3,
            "orphans": [
                {"module_file": "crates/a/src/x.rs", "symbol_kind": "function",
                 "symbol_name": "f1", "visibility": "public"},
                {"module_file": "crates/a/src/x.rs", "symbol_kind": "method",
                 "symbol_name": "m1", "visibility": "public"},
                {"module_file": "crates/b/src/y.rs", "symbol_kind": "function",
                 "symbol_name": "f2", "visibility": "public"}
            ]
        });
        assert_eq!(kind_counts(&wire), (2, 1, 2));
        // Malformed payload degrades to zeros, never panics.
        assert_eq!(
            kind_counts(&serde_json::json!({"error": "boom"})),
            (0, 0, 0)
        );
    }

    #[test]
    fn page_slice_windows_and_clamps() {
        // 1531 rows / 50 per page → 31 pages.
        assert_eq!(page_slice(1531, 0, 50), (0, 50, 31));
        assert_eq!(page_slice(1531, 30, 50), (1500, 1531, 31));
        // Out-of-range page (after a refilter) clamps to the last page.
        assert_eq!(page_slice(1531, 99, 50), (1500, 1531, 31));
        // Empty list still reports one page with an empty window.
        assert_eq!(page_slice(0, 5, 50), (0, 0, 1));
        // per = 0 never divides by zero.
        assert_eq!(page_slice(10, 0, 0), (0, 1, 10));
    }

    #[test]
    fn module_tail_keeps_tail_chars() {
        assert_eq!(module_tail("src/lib.rs", 40), "src/lib.rs");
        let long = "crates/touring-hooks-shared/src/latency_marker.rs";
        let tail = module_tail(long, 40);
        assert_eq!(tail.chars().count(), 40);
        assert!(tail.starts_with('…'));
        assert!(tail.ends_with("latency_marker.rs"));
        // Multibyte-safe.
        assert_eq!(module_tail("ééééé", 3), "…éé");
    }

    #[test]
    fn fmt_thousands_groups() {
        assert_eq!(fmt_thousands(0), "0");
        assert_eq!(fmt_thousands(531), "531");
        assert_eq!(fmt_thousands(1531), "1.531");
        assert_eq!(fmt_thousands(1234567), "1.234.567");
    }
}
