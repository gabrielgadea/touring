//! /inspector — tri-pane explorer (SPEC W6 §6.5). Wave 6.
//!
//! Library | canvas | inspector — the page that most REUSES existing
//! primitives: every pane is a composition of endpoints and helpers
//! that already power other routes. Left "Library" shows real counts
//! per domain (`/api/status`, `/api/memory/stats`, `/api/gate-metrics`);
//! the center "Canvas" lists the top live rows for the active domain
//! (`/api/search`, `/api/orphans`, `/api/memory/recall`,
//! `/api/gate-metrics`, `/api/sessions`); the right "Inspector" renders
//! the selected item with deep links into the dedicated pages. No
//! fabricated data — empty endpoints render honest `el-empty` states.
//!
//! Reused primitives: [`PageHero`] / [`Panel`] / [`Icon`] /
//! [`ProgressTrack`] (components), `top_counters` + `fmt_count`
//! (`routes::hooks`), `parse_sessions` + `fmt_iso_short`
//! (`routes::sessions`), and the shared service fetchers.

use crate::web::components::{Icon, PageHero, Panel, ProgressTrack};
use crate::web::ctx::use_refresh_bus;
use crate::web::routes::hooks::{fmt_count, top_counters};
use crate::web::routes::sessions::{fmt_iso_short, parse_sessions};
use crate::web::services::{
    fetch_gate_metrics, fetch_memory_recall, fetch_memory_stats, fetch_orphans, fetch_sessions,
    fetch_status, fetch_symbol_search, urlencode,
};
use leptos::prelude::*;
use leptos_meta::Title;
use serde_json::Value;

/// Canvas rows shown per domain (top-N live items).
pub const CANVAS_LIMIT: usize = 20;

/// The item currently pinned in the right inspector pane.
#[derive(Debug, Clone, PartialEq)]
pub enum SelectedItem {
    /// A symbol definition from `/api/search`.
    Symbol {
        /// Symbol name.
        name: String,
        /// Defining file path.
        file: String,
        /// Definition line.
        line: u64,
    },
    /// An orphan pub symbol from `/api/orphans`.
    Orphan {
        /// Orphan symbol name.
        symbol: String,
        /// Module file that defines it.
        module: String,
        /// Symbol kind (function, struct, …).
        kind: String,
    },
    /// A memory entry from `/api/memory/recall`.
    Memory {
        /// Memory key.
        key: String,
        /// Storage tier.
        tier: String,
        /// Full entry value (truncated at render time).
        value: String,
    },
    /// A gate-metrics counter from `/api/gate-metrics`.
    Counter {
        /// Counter key.
        key: String,
        /// Counter value.
        value: f64,
        /// Largest top-N counter value (progress-track denominator).
        max: f64,
    },
    /// An orchestration session from `/api/sessions`.
    Session {
        /// Session id.
        id: String,
        /// Session objective.
        objective: String,
    },
}

/// Human label for a library domain key. Unknown keys render as
/// `"Unknown"` so a typo never panics the UI.
pub fn domain_label(domain: &str) -> &'static str {
    match domain {
        "symbols" => "Symbols",
        "orphans" => "Orphans",
        "memory" => "Memory",
        "counters" => "Counters",
        "sessions" => "Sessions",
        _ => "Unknown",
    }
}

/// Mono CLI-equivalence label for the canvas panel header.
pub fn domain_cmd(domain: &str) -> &'static str {
    match domain {
        "symbols" => "GET /api/search?q=",
        "orphans" => "GET /api/orphans",
        "memory" => "GET /api/memory/recall?q=",
        "counters" => "GET /api/gate-metrics",
        "sessions" => "GET /api/sessions",
        _ => "",
    }
}

/// Existing icon glyph (see `icons::icon_markup`) for a domain key.
pub fn domain_icon(domain: &str) -> &'static str {
    match domain {
        "symbols" => "search",
        "orphans" => "orphans",
        "memory" => "memory",
        "counters" => "hooks",
        "sessions" => "sessions",
        _ => "inspector",
    }
}

/// Char-boundary-safe truncation with a `…` suffix — memory values can
/// be multi-KB JSON blobs; the inspector only needs a preview.
pub fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let head: String = s.chars().take(max).collect();
        format!("{head}…")
    }
}

/// Extract `(symbol_name, file_path, line)` rows from the real
/// `/api/search` payload `{count, definitions: [...]}`. Malformed
/// payloads yield an empty vec (honest empty state).
pub fn parse_symbol_rows(v: &Value) -> Vec<(String, String, u64)> {
    v.get("definitions")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .map(|item| {
                    (
                        item.get("symbol_name")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_string(),
                        item.get("file_path")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_string(),
                        item.get("line").and_then(Value::as_u64).unwrap_or(0),
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Extract the first `limit` `(symbol_name, module_file, symbol_kind)`
/// rows from the real `/api/orphans` payload `{orphan_count, orphans:
/// [...]}`.
pub fn parse_orphan_rows(v: &Value, limit: usize) -> Vec<(String, String, String)> {
    v.get("orphans")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .take(limit)
                .map(|o| {
                    let g = |k: &str| o.get(k).and_then(Value::as_str).unwrap_or("").to_string();
                    (g("symbol_name"), g("module_file"), g("symbol_kind"))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Extract `(key, tier, value)` rows from the real `/api/memory/recall`
/// payload `{ann_results, count, entries: [{key, tier, value, …}]}`.
/// Values are kept in full here; render-time truncation happens via
/// [`truncate_chars`].
pub fn parse_memory_rows(v: &Value) -> Vec<(String, String, String)> {
    v.get("entries")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .map(|e| {
                    let g = |k: &str| e.get(k).and_then(Value::as_str).unwrap_or("").to_string();
                    (g("key"), g("tier"), g("value"))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// `/inspector` — tri-pane library | canvas | inspector (SPEC W6 §6.5).
#[component]
pub fn InspectorPage() -> impl IntoView {
    let bus = use_refresh_bus();

    // ── Library count resources (refresh-bus driven) ─────────────────
    let status_res = LocalResource::new(move || {
        let _tick = bus.tick.get();
        async move { fetch_status().await.ok() }
    });
    let mem_stats_res = LocalResource::new(move || {
        let _tick = bus.tick.get();
        async move { fetch_memory_stats().await.ok() }
    });
    let metrics_res = LocalResource::new(move || {
        let _tick = bus.tick.get();
        async move { fetch_gate_metrics().await.ok() }
    });
    let orphans_res = LocalResource::new(move || {
        let _tick = bus.tick.get();
        async move { fetch_orphans().await.ok() }
    });
    let sessions_res = LocalResource::new(move || {
        let _tick = bus.tick.get();
        async move { fetch_sessions().await.ok() }
    });

    // ── Tri-pane state ────────────────────────────────────────────────
    let domain = RwSignal::new("symbols");
    let selected = RwSignal::new(Option::<SelectedItem>::None);
    let query = RwSignal::new(String::new());

    // ── Canvas query resources (only fire on a non-empty query) ──────
    let search_res = LocalResource::new(move || {
        let q = query.get();
        async move {
            let q = q.trim().to_string();
            if q.is_empty() {
                None
            } else {
                fetch_symbol_search(&q).await.ok()
            }
        }
    });
    let recall_res = LocalResource::new(move || {
        let q = query.get();
        async move {
            let q = q.trim().to_string();
            if q.is_empty() {
                None
            } else {
                fetch_memory_recall(&q).await.ok()
            }
        }
    });

    // ── Library count signals (None-tolerant: "—") ────────────────────
    let symbols_count = Signal::derive(move || {
        status_res
            .get()
            .flatten()
            .and_then(|v| v.pointer("/index/symbol_count").and_then(Value::as_u64))
            .map_or_else(|| "—".to_string(), |n| n.to_string())
    });
    let orphans_count = Signal::derive(move || {
        status_res
            .get()
            .flatten()
            .and_then(|v| v.pointer("/wiring/orphan_count").and_then(Value::as_u64))
            .map_or_else(|| "—".to_string(), |n| n.to_string())
    });
    let sessions_count = Signal::derive(move || {
        status_res
            .get()
            .flatten()
            .and_then(|v| v.pointer("/sessions/count").and_then(Value::as_u64))
            .map_or_else(|| "—".to_string(), |n| n.to_string())
    });
    let memory_count = Signal::derive(move || {
        mem_stats_res
            .get()
            .flatten()
            .and_then(|v| v.get("memory_entry_count").and_then(Value::as_u64))
            .map_or_else(|| "—".to_string(), |n| n.to_string())
    });
    let counters_count = Signal::derive(move || {
        metrics_res
            .get()
            .flatten()
            .and_then(|j| j.get("total_invocations").and_then(Value::as_f64))
            .map(fmt_count)
            .unwrap_or_else(|| "—".to_string())
    });

    // ── Canvas row memos (reshape raw JSON once per fetch) ────────────
    let symbol_rows = Memo::new(move |_| {
        search_res
            .get()
            .flatten()
            .as_ref()
            .map(parse_symbol_rows)
            .unwrap_or_default()
    });
    let memory_rows = Memo::new(move |_| {
        recall_res
            .get()
            .flatten()
            .as_ref()
            .map(parse_memory_rows)
            .unwrap_or_default()
    });
    let orphan_rows = Memo::new(move |_| {
        orphans_res
            .get()
            .flatten()
            .as_ref()
            .map(|v| parse_orphan_rows(v, CANVAS_LIMIT))
            .unwrap_or_default()
    });
    let counter_rows = Memo::new(move |_| {
        metrics_res
            .get()
            .flatten()
            .as_ref()
            .map(|j| top_counters(j, CANVAS_LIMIT))
            .unwrap_or_default()
    });
    let session_rows = Memo::new(move |_| {
        sessions_res
            .get()
            .flatten()
            .as_ref()
            .map(parse_sessions)
            .unwrap_or_default()
    });

    // ── Canvas renderer (one branch per domain, all .into_any()) ─────
    let canvas_view = move |d: &'static str| -> AnyView {
        match d {
            "symbols" => view! {
                <div class="pg-inspector-canvas">
                    <input
                        class="pg-inspector-search"
                        type="text"
                        placeholder="search symbols…"
                        prop:value=move || query.get()
                        on:input=move |ev| query.set(event_target_value(&ev))
                    />
                    {move || {
                        let rows = symbol_rows.get();
                        if query.get().trim().is_empty() {
                            view! {
                                <div class="el-empty">
                                    "Type a symbol name — results come from the live index."
                                </div>
                            }.into_any()
                        } else if rows.is_empty() {
                            view! {
                                <div class="el-empty">"No definitions matched."</div>
                            }.into_any()
                        } else {
                            view! {
                                <table class="el-table">
                                    <thead>
                                        <tr><th>"Symbol"</th><th>"Location"</th></tr>
                                    </thead>
                                    <tbody>
                                        {rows.into_iter().map(|(name, file, line)| {
                                            let sel_name = name.clone();
                                            let sel_file = file.clone();
                                            let loc = format!("{file}:{line}");
                                            view! {
                                                <tr
                                                    class="pg-inspector-row"
                                                    on:click=move |_| {
                                                        selected.set(Some(SelectedItem::Symbol {
                                                            name: sel_name.clone(),
                                                            file: sel_file.clone(),
                                                            line,
                                                        }));
                                                    }
                                                >
                                                    <td class="el-mono">{name}</td>
                                                    <td class="el-mono pg-inspector-loc">{loc}</td>
                                                </tr>
                                            }
                                        }).collect_view()}
                                    </tbody>
                                </table>
                            }.into_any()
                        }
                    }}
                </div>
            }
            .into_any(),
            "orphans" => view! {
                <div class="pg-inspector-canvas">
                    {move || {
                        let rows = orphan_rows.get();
                        if rows.is_empty() {
                            view! {
                                <div class="el-empty">
                                    "Orphans endpoint unavailable or zero orphans."
                                </div>
                            }.into_any()
                        } else {
                            view! {
                                <table class="el-table">
                                    <thead>
                                        <tr><th>"Symbol"</th><th>"Module"</th><th>"Kind"</th></tr>
                                    </thead>
                                    <tbody>
                                        {rows.into_iter().map(|(symbol, module, kind)| {
                                            let sel_symbol = symbol.clone();
                                            let sel_module = module.clone();
                                            let sel_kind = kind.clone();
                                            view! {
                                                <tr
                                                    class="pg-inspector-row"
                                                    on:click=move |_| {
                                                        selected.set(Some(SelectedItem::Orphan {
                                                            symbol: sel_symbol.clone(),
                                                            module: sel_module.clone(),
                                                            kind: sel_kind.clone(),
                                                        }));
                                                    }
                                                >
                                                    <td class="el-mono">{symbol}</td>
                                                    <td class="el-mono pg-inspector-loc">{module}</td>
                                                    <td><span class="el-tag">{kind}</span></td>
                                                </tr>
                                            }
                                        }).collect_view()}
                                    </tbody>
                                </table>
                            }.into_any()
                        }
                    }}
                </div>
            }
            .into_any(),
            "memory" => view! {
                <div class="pg-inspector-canvas">
                    <input
                        class="pg-inspector-search"
                        type="text"
                        placeholder="recall memory…"
                        prop:value=move || query.get()
                        on:input=move |ev| query.set(event_target_value(&ev))
                    />
                    {move || {
                        let rows = memory_rows.get();
                        if query.get().trim().is_empty() {
                            view! {
                                <div class="el-empty">
                                    "Type a query — entries come from the live memory store."
                                </div>
                            }.into_any()
                        } else if rows.is_empty() {
                            view! {
                                <div class="el-empty">"No memory entries matched."</div>
                            }.into_any()
                        } else {
                            view! {
                                <table class="el-table">
                                    <thead>
                                        <tr><th>"Key"</th><th>"Tier"</th><th>"Value"</th></tr>
                                    </thead>
                                    <tbody>
                                        {rows.into_iter().map(|(key, tier, value)| {
                                            let sel_key = key.clone();
                                            let sel_tier = tier.clone();
                                            let sel_value = value.clone();
                                            let preview = truncate_chars(&value, 80);
                                            view! {
                                                <tr
                                                    class="pg-inspector-row"
                                                    on:click=move |_| {
                                                        selected.set(Some(SelectedItem::Memory {
                                                            key: sel_key.clone(),
                                                            tier: sel_tier.clone(),
                                                            value: sel_value.clone(),
                                                        }));
                                                    }
                                                >
                                                    <td class="el-mono">{key}</td>
                                                    <td><span class="el-tag">{tier}</span></td>
                                                    <td class="pg-inspector-value">{preview}</td>
                                                </tr>
                                            }
                                        }).collect_view()}
                                    </tbody>
                                </table>
                            }.into_any()
                        }
                    }}
                </div>
            }
            .into_any(),
            "counters" => view! {
                <div class="pg-inspector-canvas">
                    {move || {
                        let rows = counter_rows.get();
                        let max = rows.first().map(|(_, v)| *v).unwrap_or(0.0);
                        if rows.is_empty() {
                            view! {
                                <div class="el-empty">
                                    "Gate-metrics endpoint unavailable or all counters at zero."
                                </div>
                            }.into_any()
                        } else {
                            view! {
                                <table class="el-table">
                                    <thead>
                                        <tr><th>"Counter"</th><th>"Value"</th></tr>
                                    </thead>
                                    <tbody>
                                        {rows.into_iter().map(|(key, value)| {
                                            let sel_key = key.clone();
                                            view! {
                                                <tr
                                                    class="pg-inspector-row"
                                                    on:click=move |_| {
                                                        selected.set(Some(SelectedItem::Counter {
                                                            key: sel_key.clone(),
                                                            value,
                                                            max,
                                                        }));
                                                    }
                                                >
                                                    <td class="el-mono">{key}</td>
                                                    <td class="el-mono pg-inspector-num">
                                                        {fmt_count(value)}
                                                    </td>
                                                </tr>
                                            }
                                        }).collect_view()}
                                    </tbody>
                                </table>
                            }.into_any()
                        }
                    }}
                </div>
            }
            .into_any(),
            "sessions" => view! {
                <div class="pg-inspector-canvas">
                    {move || {
                        let rows = session_rows.get();
                        if rows.is_empty() {
                            view! {
                                <div class="el-empty">
                                    "Sessions endpoint unavailable or no sessions recorded."
                                </div>
                            }.into_any()
                        } else {
                            view! {
                                <table class="el-table">
                                    <thead>
                                        <tr>
                                            <th>"Session"</th>
                                            <th>"Objective"</th>
                                            <th>"Type"</th>
                                            <th>"Created"</th>
                                        </tr>
                                    </thead>
                                    <tbody>
                                        {rows.into_iter().map(|(id, objective, task_type, created)| {
                                            let sel_id = id.clone();
                                            let sel_obj = objective.clone();
                                            let created_short = fmt_iso_short(&created);
                                            view! {
                                                <tr
                                                    class="pg-inspector-row"
                                                    on:click=move |_| {
                                                        selected.set(Some(SelectedItem::Session {
                                                            id: sel_id.clone(),
                                                            objective: sel_obj.clone(),
                                                        }));
                                                    }
                                                >
                                                    <td class="el-mono">{id}</td>
                                                    <td>{objective}</td>
                                                    <td><span class="el-tag">{task_type}</span></td>
                                                    <td class="el-mono">{created_short}</td>
                                                </tr>
                                            }
                                        }).collect_view()}
                                    </tbody>
                                </table>
                            }.into_any()
                        }
                    }}
                </div>
            }
            .into_any(),
            _ => view! { <div class="el-empty">"Unknown domain."</div> }.into_any(),
        }
    };

    // ── Library rows (left pane data) ─────────────────────────────────
    let library: [(&'static str, Signal<String>); 5] = [
        ("symbols", symbols_count),
        ("orphans", orphans_count),
        ("memory", memory_count),
        ("counters", counters_count),
        ("sessions", sessions_count),
    ];

    view! {
        <div class="page pg-inspector">
            <Title text="Touring — Inspector"/>
            <Suspense fallback=|| view! { <div class="el-skeleton" aria-label="loading"></div> }>
                {move || status_res.get().map(|_status| {
                    view! {
                        <PageHero
                            eyebrow="DIAGNOSTICS · INSPECTOR"
                            title="Inspector"
                            title_em="tri-pane"
                            sub="Library → canvas → inspector. Pick a domain on the left, drill into live rows in the middle, pin one item on the right — every value comes from the live daemon."
                            stat=symbols_count
                            stat_label="SYMBOLS"
                        >
                            <a class="el-btn" href="/search">"Search"</a>
                            <a class="el-btn" href="/health">"Health"</a>
                        </PageHero>

                        <section class="pg-inspector-grid">
                            <Panel num="01" eyebrow="Library" cmd="GET /api/status">
                                <div class="pg-inspector-lib">
                                    {library.into_iter().map(|(key, count)| {
                                        view! {
                                            <button
                                                type="button"
                                                class=move || {
                                                    if domain.get() == key {
                                                        "el-result-row selected pg-inspector-lib-row"
                                                    } else {
                                                        "el-result-row pg-inspector-lib-row"
                                                    }
                                                }
                                                on:click=move |_| {
                                                    domain.set(key);
                                                    selected.set(None);
                                                }
                                            >
                                                <Icon name=domain_icon(key)/>
                                                <span class="pg-inspector-lib-label">
                                                    {domain_label(key)}
                                                </span>
                                                <span class="el-mono pg-inspector-lib-count">
                                                    {move || count.get()}
                                                </span>
                                            </button>
                                        }
                                    }).collect_view()}
                                </div>
                            </Panel>

                            {move || {
                                let d = domain.get();
                                view! {
                                    <Panel num="02" eyebrow="Canvas" cmd=domain_cmd(d)>
                                        {canvas_view(d)}
                                    </Panel>
                                }.into_any()
                            }}

                            <Panel num="03" eyebrow="Inspector" cmd="selection">
                                {move || {
                                    match selected.get() {
                                        None => view! {
                                            <div class="el-empty">
                                                "Select a row in the canvas to inspect it here."
                                            </div>
                                        }.into_any(),
                                        Some(SelectedItem::Symbol { name, file, line }) => {
                                            let href = format!(
                                                "/wiring/impact?symbol={}",
                                                urlencode(&name),
                                            );
                                            let loc = format!("{file}:{line}");
                                            view! {
                                                <div class="pg-inspector-detail">
                                                    <div class="el-eyebrow">"SYMBOL"</div>
                                                    <div class="el-mono pg-inspector-detail-name">
                                                        {name}
                                                    </div>
                                                    <div class="el-mono pg-inspector-detail-loc">
                                                        {loc}
                                                    </div>
                                                    <a class="el-btn el-btn-sm" href=href>
                                                        "Blast radius"
                                                    </a>
                                                </div>
                                            }.into_any()
                                        }
                                        Some(SelectedItem::Orphan { symbol, module, kind }) => view! {
                                            <div class="pg-inspector-detail">
                                                <div class="el-eyebrow">"ORPHAN"</div>
                                                <div class="el-mono pg-inspector-detail-name">
                                                    {symbol}
                                                </div>
                                                <div class="el-mono pg-inspector-detail-loc">
                                                    {module}
                                                </div>
                                                <span class="el-tag">{kind}</span>
                                            </div>
                                        }.into_any(),
                                        Some(SelectedItem::Memory { key, tier, value }) => {
                                            let preview = truncate_chars(&value, 400);
                                            view! {
                                                <div class="pg-inspector-detail">
                                                    <div class="el-eyebrow">"MEMORY"</div>
                                                    <div class="el-mono pg-inspector-detail-name">
                                                        {key}
                                                    </div>
                                                    <span class="el-tag">{tier}</span>
                                                    <pre class="el-code pg-inspector-value">
                                                        {preview}
                                                    </pre>
                                                </div>
                                            }.into_any()
                                        }
                                        Some(SelectedItem::Counter { key, value, max }) => {
                                            let ratio = if max > 0.0 {
                                                (value / max).clamp(0.0, 1.0)
                                            } else {
                                                0.0
                                            };
                                            view! {
                                                <div class="pg-inspector-detail">
                                                    <div class="el-eyebrow">"COUNTER"</div>
                                                    <div class="el-mono pg-inspector-detail-name">
                                                        {key}
                                                    </div>
                                                    <div class="el-stat">{fmt_count(value)}</div>
                                                    <ProgressTrack
                                                        value=Signal::derive(move || ratio)
                                                        show_value=true
                                                    />
                                                </div>
                                            }.into_any()
                                        }
                                        Some(SelectedItem::Session { id, objective }) => view! {
                                            <div class="pg-inspector-detail">
                                                <div class="el-eyebrow">"SESSION"</div>
                                                <div class="el-mono pg-inspector-detail-name">
                                                    {id}
                                                </div>
                                                <p class="pg-inspector-detail-sub">{objective}</p>
                                                <a class="el-btn el-btn-sm" href="/sessions">
                                                    "Open sessions"
                                                </a>
                                            </div>
                                        }.into_any(),
                                    }
                                }}
                            </Panel>
                        </section>
                    }
                })}
            </Suspense>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn domain_label_known_and_unknown() {
        assert_eq!(domain_label("symbols"), "Symbols");
        assert_eq!(domain_label("orphans"), "Orphans");
        assert_eq!(domain_label("memory"), "Memory");
        assert_eq!(domain_label("counters"), "Counters");
        assert_eq!(domain_label("sessions"), "Sessions");
        assert_eq!(domain_label("nope"), "Unknown");
    }

    #[test]
    fn domain_cmd_maps_each_domain_to_its_endpoint() {
        assert_eq!(domain_cmd("symbols"), "GET /api/search?q=");
        assert_eq!(domain_cmd("orphans"), "GET /api/orphans");
        assert_eq!(domain_cmd("memory"), "GET /api/memory/recall?q=");
        assert_eq!(domain_cmd("counters"), "GET /api/gate-metrics");
        assert_eq!(domain_cmd("sessions"), "GET /api/sessions");
        assert_eq!(domain_cmd("nope"), "");
    }

    #[test]
    fn domain_icon_reuses_existing_glyphs() {
        use crate::web::components::icon_markup;
        for d in ["symbols", "orphans", "memory", "counters", "sessions"] {
            let icon = domain_icon(d);
            assert_ne!(
                icon_markup(icon),
                icon_markup("__unknown__"),
                "{d} must map to a real glyph, not the fallback dot"
            );
        }
        assert_eq!(domain_icon("nope"), "inspector");
    }

    #[test]
    fn parse_symbol_rows_extracts_definitions() {
        let v = serde_json::json!({
            "count": 2,
            "symbol_name": "fetch_status",
            "definitions": [
                {"symbol_name": "fetch_status", "file_path": "src/web/services/mod.rs", "line": 93},
                {"symbol_name": "fetch_status_kpi", "file_path": "src/web/routes/dashboard.rs", "line": 249}
            ]
        });
        let rows = parse_symbol_rows(&v);
        assert_eq!(rows.len(), 2);
        assert_eq!(
            rows[0],
            (
                "fetch_status".to_string(),
                "src/web/services/mod.rs".to_string(),
                93,
            )
        );
        assert!(parse_symbol_rows(&serde_json::json!({"error": "x"})).is_empty());
    }

    #[test]
    fn parse_orphan_rows_caps_at_limit_and_tolerates_missing_fields() {
        let orphans: Vec<Value> = (0..25)
            .map(|i| {
                serde_json::json!({
                    "symbol_name": format!("orphan_{i}"),
                    "module_file": "crates/x/src/lib.rs",
                    "symbol_kind": "function"
                })
            })
            .collect();
        let v = serde_json::json!({"orphan_count": 25, "orphans": orphans});
        let rows = parse_orphan_rows(&v, 20);
        assert_eq!(rows.len(), 20);
        assert_eq!(rows[0].0, "orphan_0");
        assert_eq!(rows[0].2, "function");

        let sparse = serde_json::json!({"orphans": [{"symbol_name": "lonely"}]});
        let rows = parse_orphan_rows(&sparse, 20);
        assert_eq!(
            rows,
            vec![("lonely".to_string(), String::new(), String::new())]
        );
    }

    #[test]
    fn parse_memory_rows_reads_entries() {
        let v = serde_json::json!({
            "ann_results": 0,
            "count": 1,
            "entries": [
                {"key": "lesson:w6", "tier": "semantic", "type": "lesson", "value": "tri-pane works"}
            ]
        });
        let rows = parse_memory_rows(&v);
        assert_eq!(
            rows,
            vec![(
                "lesson:w6".to_string(),
                "semantic".to_string(),
                "tri-pane works".to_string(),
            )]
        );
        assert!(parse_memory_rows(&Value::Null).is_empty());
    }

    #[test]
    fn truncate_chars_is_char_boundary_safe() {
        assert_eq!(truncate_chars("short", 80), "short");
        let t = truncate_chars("ação direta no índice", 4);
        assert_eq!(t, "ação…");
        let long = "x".repeat(100);
        let t = truncate_chars(&long, 80);
        assert_eq!(t.chars().count(), 81);
        assert!(t.ends_with('…'));
    }
}
