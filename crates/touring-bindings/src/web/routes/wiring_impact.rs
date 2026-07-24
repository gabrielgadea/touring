//! /wiring/impact — BFS blast radius explorer (SPEC 2026-06-12 §6.2). Elite W4.
//!
//! A symbol input + depth select (1..5) drive `/api/wiring/impact`;
//! the payload renders as concentric [`DepthRings`] with a wavefront
//! scrubber (`.el-transport`), a nucleus card, per-depth consumer rows
//! and the top transitive paths. Deep links from /orphans and /search
//! arrive as `?symbol=X` and auto-submit. Nodes are capped at
//! [`NODE_CAP`] (SPEC §10) with an honest truncation banner.

use leptos::prelude::*;
use leptos_meta::Title;
use leptos_router::hooks::use_query_map;
use serde_json::Value;

use crate::web::components::{
    DepthRing, DepthRings, KpiCell, KpiStrip, PageHero, Panel, ProgressTrack, RingNode,
};
use crate::web::services::{fetch_symbol_search, fetch_wiring_impact};

/// SPEC §10 — hard cap on rendered ring nodes.
pub const NODE_CAP: usize = 120;

/// Group payload `paths` by BFS depth into 1-based [`DepthRing`]s,
/// sorted ascending by depth. Malformed entries default to depth 1.
pub fn rings_of(payload: &Value) -> Vec<DepthRing> {
    let mut counts: std::collections::BTreeMap<usize, usize> = std::collections::BTreeMap::new();
    if let Some(paths) = payload.get("paths").and_then(|v| v.as_array()) {
        for p in paths {
            let depth = p.get("depth").and_then(|v| v.as_u64()).unwrap_or(1).max(1) as usize;
            *counts.entry(depth).or_insert(0) += 1;
        }
    }
    counts
        .into_iter()
        .map(|(depth, count)| DepthRing { depth, count })
        .collect()
}

/// Flatten payload `paths` into [`RingNode`]s capped at `cap`
/// (SPEC §10). `angle_idx` is the node's index within its depth group.
pub fn nodes_of(payload: &Value, cap: usize) -> Vec<RingNode> {
    let mut next_idx: std::collections::HashMap<usize, usize> = std::collections::HashMap::new();
    let mut out = Vec::new();
    if let Some(paths) = payload.get("paths").and_then(|v| v.as_array()) {
        for p in paths {
            if out.len() >= cap {
                break;
            }
            let depth = p.get("depth").and_then(|v| v.as_u64()).unwrap_or(1).max(1) as usize;
            let label = p
                .get("consumer_symbol")
                .and_then(|v| v.as_str())
                .unwrap_or("?")
                .to_string();
            let slot = next_idx.entry(depth).or_insert(0);
            let angle_idx = *slot;
            *slot += 1;
            out.push(RingNode {
                label,
                depth,
                angle_idx,
                orphan: false,
            });
        }
    }
    out
}

/// Read a numeric payload field, defaulting to 0 on absence/mismatch.
pub fn count_field(payload: &Value, key: &str) -> u64 {
    payload.get(key).and_then(|v| v.as_u64()).unwrap_or(0)
}

/// `/wiring/impact` — DepthRings BFS blast radius (W4).
#[component]
pub fn WiringImpact() -> impl IntoView {
    let query = use_query_map();
    let input = RwSignal::new(String::new());
    let depth = RwSignal::new(3usize);
    let submitted = RwSignal::new(String::new());
    let active_depth = RwSignal::new(1usize);

    // Deep links from /orphans and /search arrive as ?symbol=X — auto-submit.
    Effect::new(move |_| {
        if let Some(sym) = query.get().get("symbol") {
            let s = sym.trim().to_string();
            if !s.is_empty() && s != submitted.get_untracked() {
                input.set(s.clone());
                submitted.set(s);
                active_depth.set(1);
            }
        }
    });

    // Real data only: /api/wiring/impact (BFS) + /api/search (definition
    // count — distinguishes "orphan" from "unindexed" on zero consumers).
    let res = LocalResource::new(move || {
        let sym = submitted.get();
        let d = depth.get().clamp(1, 5);
        async move {
            let sym = sym.trim().to_string();
            if sym.is_empty() {
                return None;
            }
            let impact = fetch_wiring_impact(&sym, d as u8)
                .await
                .unwrap_or(Value::Null);
            let defs = fetch_symbol_search(&sym)
                .await
                .ok()
                .and_then(|v| v.get("count").and_then(|c| c.as_u64()))
                .unwrap_or(0);
            Some((impact, defs))
        }
    });

    let hero_stat = Signal::derive(move || {
        res.get()
            .flatten()
            .map(|(p, _)| count_field(&p, "total_transitive").to_string())
            .unwrap_or_else(|| "—".to_string())
    });

    view! {
        <div class="page pg-impact">
            <Title text="Touring — Wiring Impact"/>
            <div class="el-main-editorial">
                <PageHero
                    eyebrow="CODE INTELLIGENCE · BLAST RADIUS"
                    title="Impact"
                    title_em="rings"
                    sub="BFS blast radius of a symbol — each ring is one dependency hop. Scrub the wavefront to see how a change propagates outward through real consumers."
                    stat=hero_stat
                    stat_label="TRANSITIVE"
                >
                    <a class="el-btn" href="/orphans">"Orphans"</a>
                    <a class="el-btn" href="/search">"Search"</a>
                </PageHero>

                <form
                    class="pg-impact-input-row"
                    on:submit=move |ev| {
                        ev.prevent_default();
                        let s = input.get().trim().to_string();
                        submitted.set(s);
                        active_depth.set(1);
                    }
                >
                    <input
                        type="text"
                        class="el-palette-input pg-impact-input"
                        placeholder="symbol — e.g. run_gateway"
                        prop:value=move || input.get()
                        on:input=move |ev| input.set(event_target_value(&ev))
                    />
                    <select
                        class="pg-impact-depth-select el-mono"
                        aria-label="BFS depth"
                        on:change=move |ev| {
                            let d = event_target_value(&ev).parse::<usize>().unwrap_or(3).clamp(1, 5);
                            depth.set(d);
                        }
                    >
                        {(1..=5usize).map(|d| view! {
                            <option value=d.to_string() selected=d == 3>
                                {format!("depth {d}")}
                            </option>
                        }).collect_view()}
                    </select>
                    <button type="submit" class="el-btn el-btn-primary">"Analyze"</button>
                </form>

                <Suspense fallback=|| view! { <div class="el-skeleton" aria-label="loading"></div> }>
                    {move || {
                        if submitted.get().trim().is_empty() {
                            return view! {
                                <div class="el-empty pg-impact-empty">
                                    <p>"Enter a symbol to map its blast radius. Try:"</p>
                                    <div class="el-hero-actions">
                                        {["EliteShell", "QualityDetail", "run_gateway"].into_iter().map(|s| view! {
                                            <button
                                                class="el-btn el-btn-sm"
                                                on:click=move |_| {
                                                    input.set(s.to_string());
                                                    submitted.set(s.to_string());
                                                    active_depth.set(1);
                                                }
                                            >
                                                {s}
                                            </button>
                                        }).collect_view()}
                                    </div>
                                </div>
                            }.into_any();
                        }
                        match res.get().flatten() {
                            None => view! { <div class="el-skeleton" aria-label="loading"></div> }.into_any(),
                            Some((payload, defs)) => {
                                let rings = rings_of(&payload);
                                let nodes = nodes_of(&payload, NODE_CAP);
                                let direct = count_field(&payload, "direct_consumers");
                                let transitive = count_field(&payload, "total_transitive");
                                let ring_max_depth =
                                    rings.iter().map(|r| r.depth as u64).max().unwrap_or(0);
                                let max_d =
                                    count_field(&payload, "max_depth").max(ring_max_depth).max(1) as usize;
                                let paths_total = payload
                                    .get("paths")
                                    .and_then(|v| v.as_array())
                                    .map(|a| a.len())
                                    .unwrap_or(0);
                                let truncated = paths_total > NODE_CAP;
                                let max_ring_count =
                                    rings.iter().map(|r| r.count).max().unwrap_or(1).max(1);
                                let sym = payload
                                    .get("symbol")
                                    .and_then(|v| v.as_str())
                                    .map(str::to_string)
                                    .filter(|s| !s.is_empty())
                                    .unwrap_or_else(|| submitted.get());
                                let top_paths: Vec<(String, String, u64)> = payload
                                    .get("paths")
                                    .and_then(|v| v.as_array())
                                    .map(|a| {
                                        a.iter()
                                            .take(8)
                                            .map(|p| {
                                                (
                                                    p.get("consumer_symbol")
                                                        .and_then(|v| v.as_str())
                                                        .unwrap_or("?")
                                                        .to_string(),
                                                    p.get("consumer_module")
                                                        .and_then(|v| v.as_str())
                                                        .unwrap_or("—")
                                                        .to_string(),
                                                    p.get("depth").and_then(|v| v.as_u64()).unwrap_or(1),
                                                )
                                            })
                                            .collect()
                                    })
                                    .unwrap_or_default();

                                let nucleus_sig = Signal::derive({
                                    let s = sym.clone();
                                    move || s.clone()
                                });
                                let rings_sig = Signal::derive({
                                    let r = rings.clone();
                                    move || r.clone()
                                });
                                let nodes_sig = Signal::derive({
                                    let n = nodes.clone();
                                    move || n.clone()
                                });
                                let active_sig =
                                    Signal::derive(move || active_depth.get().clamp(1, max_d));
                                let scrub_sig = Signal::derive(move || {
                                    active_depth.get().clamp(1, max_d) as f64 / max_d as f64
                                });

                                let kpi_direct = direct.to_string();
                                let kpi_transitive = transitive.to_string();
                                let kpi_depth = format!("d={max_d}");
                                let kpi_paths = paths_total.to_string();

                                view! {
                                    <KpiStrip>
                                        <KpiCell
                                            label="Direct"
                                            value=Signal::derive(move || kpi_direct.clone())
                                            sub="depth = 1"
                                        />
                                        <KpiCell
                                            label="Transitive"
                                            value=Signal::derive(move || kpi_transitive.clone())
                                            sub="all depths"
                                        />
                                        <KpiCell
                                            label="Max depth"
                                            value=Signal::derive(move || kpi_depth.clone())
                                            sub="BFS frontier"
                                        />
                                        <KpiCell
                                            label="Paths"
                                            value=Signal::derive(move || kpi_paths.clone())
                                            sub="consumer edges"
                                        />
                                    </KpiStrip>

                                    <section class="pg-impact-grid">
                                        <Panel
                                            num="01"
                                            eyebrow="Rings"
                                            cmd="touring wiring impact <s> --depth N --format json"
                                        >
                                            {if paths_total == 0 {
                                                view! {
                                                    <div class="el-empty">
                                                        <p>"0 consumers — orphan or unindexed"</p>
                                                        <p class="el-mono pg-impact-zero-hint">
                                                            {if defs > 0 {
                                                                format!(
                                                                    "{defs} definition(s) indexed — nothing consumes this symbol yet (orphan)."
                                                                )
                                                            } else {
                                                                "no definitions in the index — symbol may be unindexed or misspelled."
                                                                    .to_string()
                                                            }}
                                                        </p>
                                                    </div>
                                                }.into_any()
                                            } else {
                                                view! {
                                                    <div class="pg-impact-rings-wrap">
                                                        {truncated.then(|| view! {
                                                            <div class="el-banner el-banner-warn pg-impact-cap-banner">
                                                                {format!("showing first {NODE_CAP} of {paths_total} consumers")}
                                                            </div>
                                                        })}
                                                        <div class="pg-impact-rings">
                                                            <DepthRings
                                                                nucleus=nucleus_sig
                                                                rings=rings_sig
                                                                nodes=nodes_sig
                                                                active_depth=active_sig
                                                                width=760.0
                                                                height=480.0
                                                            />
                                                        </div>
                                                        <div class="el-transport pg-impact-transport">
                                                            <button
                                                                class="el-btn el-btn-sm"
                                                                on:click=move |_| active_depth
                                                                    .update(|a| *a = a.saturating_sub(1).max(1))
                                                            >
                                                                "d-1"
                                                            </button>
                                                            <ProgressTrack value=scrub_sig color="var(--el-violet)"/>
                                                            <button
                                                                class="el-btn el-btn-sm"
                                                                on:click=move |_| active_depth
                                                                    .update(|a| *a = (*a + 1).min(max_d))
                                                            >
                                                                "d+1"
                                                            </button>
                                                            <span class="el-mono pg-impact-transport-label">
                                                                {move || format!(
                                                                    "wavefront d={} / {max_d}",
                                                                    active_depth.get().clamp(1, max_d)
                                                                )}
                                                            </span>
                                                        </div>
                                                    </div>
                                                }.into_any()
                                            }}
                                        </Panel>

                                        <div class="pg-impact-rail">
                                            <Panel num="02" eyebrow="Nucleus" cmd="touring index find <s>">
                                                <div class="el-card pg-impact-nucleus">
                                                    <div class="el-mono pg-impact-nucleus-sym">{sym.clone()}</div>
                                                    <div class="pg-impact-nucleus-counts">
                                                        {format!("{direct} direct · {transitive} transitive · {defs} defs")}
                                                    </div>
                                                </div>
                                            </Panel>

                                            <Panel num="03" eyebrow="Consumers by depth" cmd="grouped by BFS depth">
                                                {if rings.is_empty() {
                                                    view! {
                                                        <div class="el-empty">
                                                            <p>"no depth rings — zero consumers"</p>
                                                        </div>
                                                    }.into_any()
                                                } else {
                                                    rings.iter().map(|r| {
                                                        let frac = r.count as f64 / max_ring_count as f64;
                                                        let count = r.count;
                                                        let label = format!("d={}", r.depth);
                                                        view! {
                                                            <div class="el-row pg-impact-depth-row">
                                                                <span class="el-mono pg-impact-depth-label">{label}</span>
                                                                <div class="pg-impact-depth-track">
                                                                    <ProgressTrack value=Signal::derive(move || frac)/>
                                                                </div>
                                                                <span class="el-mono pg-impact-depth-count">
                                                                    {count.to_string()}
                                                                </span>
                                                            </div>
                                                        }
                                                    }).collect_view().into_any()
                                                }}
                                            </Panel>

                                            <Panel num="04" eyebrow="Top paths" cmd="first 8 consumer edges">
                                                {if top_paths.is_empty() {
                                                    view! {
                                                        <div class="el-empty">
                                                            <p>"no paths returned"</p>
                                                        </div>
                                                    }.into_any()
                                                } else {
                                                    top_paths.into_iter().map(|(cs, cm, d)| view! {
                                                        <div class="el-row pg-impact-path-row">
                                                            <div class="pg-impact-path-main">
                                                                <span class="pg-impact-path-sym">{cs}</span>
                                                                <span class="pg-impact-path-mod">{cm}</span>
                                                            </div>
                                                            <span class="el-mono pg-impact-path-depth">
                                                                {format!("d={d}")}
                                                            </span>
                                                        </div>
                                                    }).collect_view().into_any()
                                                }}
                                            </Panel>
                                        </div>
                                    </section>
                                }.into_any()
                            }
                        }
                    }}
                </Suspense>
            </div>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a real-shaped `/api/wiring/impact` payload from depths.
    fn payload_with(depths: &[usize]) -> Value {
        let paths: Vec<Value> = depths
            .iter()
            .enumerate()
            .map(|(i, d)| {
                serde_json::json!({
                    "consumer_module": format!("crates/x/src/m{i}.rs"),
                    "consumer_symbol": format!("sym_{i}"),
                    "depth": d,
                    "fan_out": 0,
                    "path_type": if *d == 1 { "direct" } else { "transitive" }
                })
            })
            .collect();
        serde_json::json!({
            "symbol": "X",
            "direct_consumers": depths.iter().filter(|d| **d == 1).count(),
            "total_transitive": depths.len(),
            "max_depth": depths.iter().max().copied().unwrap_or(0),
            "paths": paths
        })
    }

    #[test]
    fn rings_of_groups_paths_by_depth() {
        let rings = rings_of(&payload_with(&[1, 1, 2]));
        assert_eq!(rings.len(), 2);
        assert_eq!((rings[0].depth, rings[0].count), (1, 2));
        assert_eq!((rings[1].depth, rings[1].count), (2, 1));
    }

    #[test]
    fn rings_of_empty_and_malformed_payloads() {
        assert!(rings_of(&serde_json::json!({"paths": []})).is_empty());
        assert!(rings_of(&Value::Null).is_empty());
        assert!(rings_of(&serde_json::json!({"error": "boom"})).is_empty());
    }

    #[test]
    fn nodes_of_caps_at_120() {
        let depths: Vec<usize> = (0..130).map(|i| 1 + i % 3).collect();
        let nodes = nodes_of(&payload_with(&depths), NODE_CAP);
        assert_eq!(nodes.len(), NODE_CAP);
        // Uncapped, all 130 survive.
        assert_eq!(nodes_of(&payload_with(&depths), 200).len(), 130);
    }

    #[test]
    fn nodes_of_assigns_angle_idx_within_depth_group() {
        let nodes = nodes_of(&payload_with(&[1, 2, 1, 2, 2]), NODE_CAP);
        let idx: Vec<(usize, usize)> = nodes.iter().map(|n| (n.depth, n.angle_idx)).collect();
        assert_eq!(idx, vec![(1, 0), (2, 0), (1, 1), (2, 1), (2, 2)]);
        assert_eq!(nodes[0].label, "sym_0");
        assert!(
            nodes.iter().all(|n| !n.orphan),
            "orphan flag unused on impact paths"
        );
    }

    #[test]
    fn nodes_of_empty_payload_yields_no_nodes() {
        assert!(nodes_of(&Value::Null, NODE_CAP).is_empty());
        assert!(nodes_of(&serde_json::json!({"paths": []}), NODE_CAP).is_empty());
    }

    #[test]
    fn count_field_defaults_to_zero() {
        let p = payload_with(&[1, 2]);
        assert_eq!(count_field(&p, "total_transitive"), 2);
        assert_eq!(count_field(&p, "missing_key"), 0);
        assert_eq!(count_field(&Value::Null, "anything"), 0);
    }
}
