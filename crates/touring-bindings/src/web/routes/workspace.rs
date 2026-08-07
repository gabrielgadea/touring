//! Workspace graph route — "Atlas" elite page (SPEC W5 §8) over the
//! Pauling-shell visualization engine.
//!
//! Elite W5 shell: full-bleed `.page.pg-atlas` (no `el-main-editorial`) —
//! compact hero with the REAL node count in the title, `KpiStrip` with
//! live aggregates, then a `232px 1fr 320px` grid: Layers rail (cluster
//! legend + stats + filters) · graph canvas with thin toolbar · 320px
//! right rail (node dossier + top crates + chains link). The legacy
//! `ws-*`/`nd-*` Rust-side class dialect died with this migration; the
//! visual shell speaks `.el-*`/`.pg-atlas-*` only.
//!
//! Leptos owns UI state (filters, lens, view-mode, search, target crate,
//! selected workflow chain). All 3D rendering is delegated to the JS
//! runtime via `window.*` hooks defined in `index.html`. Communication is
//! bidirectional and STRICTLY TYPED via `wasm-bindgen extern "C"` —
//! no `eval`, no string-based code paths.
//!   • Leptos → JS: typed extern fns (`initForceGraph`, `setAspectLens`, …).
//!   • JS → Leptos: `CustomEvent` dispatch on `window`
//!     (`workspace-crates-loaded`, `workspace-chains-loaded`).
//!
//! ENGINE CONTRACT (do not break): the JS side mounts into
//! `#workspace-graph-container` (sized by a `ResizeObserver`), reveals the
//! node dossier by toggling `.open` on `#node-details-panel`, and writes
//! selected-node fields into the `nd-*` element ids (and injects `.ndflag`
//! / `.nd-edge` / `.nd-empty` children styled globally). All of those ids
//! and the dossier's inner markup are preserved verbatim below.
//!
//! FALLBACK 2D (SPEC §8.2/§8.3): WebGL is probed on mount (webgl2 → webgl
//! on a scratch canvas). Without WebGL the 3D engine is never booted —
//! the canvas cell renders a deterministic SVG of concentric Pauling
//! shells (`DepthRings`, shell = degree bucket, top-120 nodes by degree)
//! under a discreet `.el-banner`.

use std::collections::HashMap;

use leptos::prelude::*;
use leptos_meta::Title;
use serde_json::Value;

use crate::web::components::{DepthRing, DepthRings, KpiCell, KpiStrip, Panel, RingNode};
use crate::web::services::fetch_viz_workspace_json;

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

/* ── Typed JS bindings (no eval) ─────────────────────────────────────── */
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = window, js_name = initForceGraph)]
    fn js_init_force_graph(data: &str);

    #[wasm_bindgen(js_namespace = window, js_name = updateWorkspaceGraphFilters)]
    fn js_update_filters(orphans: bool, tests: bool, particles: bool, target: &str);

    #[wasm_bindgen(js_namespace = window, js_name = setAspectLens)]
    fn js_set_aspect_lens(lens: &str);

    #[wasm_bindgen(js_namespace = window, js_name = setViewMode)]
    fn js_set_view_mode(mode: &str);

    #[wasm_bindgen(js_namespace = window, js_name = setWorkflowChain)]
    fn js_set_workflow_chain(chain_id: &str);

    #[wasm_bindgen(js_namespace = window, js_name = searchWorkspaceGraph)]
    fn js_search_graph(query: &str);

    #[wasm_bindgen(js_namespace = window, js_name = recenterWorkspace)]
    fn js_recenter();

    #[wasm_bindgen(js_namespace = window, js_name = closeNodeDetailsPanel)]
    fn js_close_panel();

    #[wasm_bindgen(js_namespace = window, js_name = setShellEnabled)]
    fn js_set_shell_enabled(shell_level: i32, enabled: bool);
}

/// WebGL availability probe (SPEC §8.2): scratch canvas, try `webgl2`
/// then `webgl`. Any miss along the chain reads as "unavailable" — the
/// page then renders the deterministic 2D SVG fallback instead of
/// booting the 3D engine.
#[cfg(target_arch = "wasm32")]
fn detect_webgl() -> bool {
    use wasm_bindgen::JsCast;
    let probe = || -> Option<bool> {
        let doc = web_sys::window()?.document()?;
        let canvas = doc
            .create_element("canvas")
            .ok()?
            .dyn_into::<web_sys::HtmlCanvasElement>()
            .ok()?;
        let gl2 = canvas.get_context("webgl2").ok().flatten().is_some();
        let gl1 = canvas.get_context("webgl").ok().flatten().is_some();
        Some(gl2 || gl1)
    };
    probe().unwrap_or(false)
}

/// Available aspect lenses for the workspace graph.
const LENSES: &[(&str, &str, &str)] = &[
    (
        "pauling",
        "Pauling",
        "Concentric shells: nucleus → external pearl",
    ),
    (
        "quality",
        "Quality",
        "Color by quality_score: green → amber → rose",
    ),
    (
        "cognitive",
        "Cognitive",
        "Color by cognitive_score: green → cyan → violet → red",
    ),
    (
        "safety",
        "Safety",
        "Highlight unsafe + orphans; dim everything else",
    ),
    (
        "centrality",
        "Centrality",
        "Hubs (high fan-in) vs dispatchers (high fan-out)",
    ),
];

/// Pauling shell layers exposed as toggleable filters.
/// `(shell_level, label, swatch_color, hint)`.
const SHELLS: &[(i32, &str, &str, &str)] = &[
    (
        0,
        "Nucleus",
        "#00e5c0",
        "Target crate's own symbols (innermost)",
    ),
    (1, "K shell", "#26c6da", "Direct dependencies (1-hop)"),
    (2, "L shell", "#00e5ff", "Second-order dependencies (2-hop)"),
    (3, "M shell", "#7c4dff", "Third-order dependencies (3-hop)"),
    (4, "N shell", "#5e35b1", "Fourth-order dependencies (4-hop)"),
    (
        99,
        "External",
        "#ffffff",
        "External crates (outermost pearl)",
    ),
];

/// Crate-family clusters for the Layers legend. Counts are derived from the
/// REAL `/api/viz/workspace` payload (node.crate prefix matching), never
/// hardcoded. `(label, color)` — order is the bucket index of `cluster_idx`.
const CLUSTERS: &[(&str, &str)] = &[
    ("Foundation", "#67e8f9"),
    ("Hooks · hot path", "#5eead4"),
    ("Learning · cortex", "#f59e0b"),
    ("Quality · generator", "#a78bfa"),
    ("UI · web", "#84cc16"),
    ("Misc · external", "#94a3b8"),
];

/// Map a crate name onto a `CLUSTERS` bucket index by prefix family.
fn cluster_idx(krate: &str) -> usize {
    if krate.contains("foundation") || krate.contains("storage") || krate.contains("identity") {
        0
    } else if krate.contains("hooks") {
        1
    } else if krate.contains("learning")
        || krate.contains("cortex")
        || krate.contains("intelligence")
    {
        2
    } else if krate.contains("generator") || krate.contains("quality") || krate.contains("analysis")
    {
        3
    } else if krate.contains("bindings") || krate.contains("web") || krate.contains("cli") {
        4
    } else {
        5
    }
}

/// Aggregates derived from the live `/api/viz/workspace` JSON — one pass,
/// memoized, so panels render REAL numbers (no placeholders).
#[derive(Clone, PartialEq)]
struct AtlasStats {
    nodes: usize,
    links: usize,
    orphans: usize,
    unsafe_nodes: usize,
    externals: usize,
    /// Node count per `CLUSTERS` bucket (same indexing).
    clusters: [usize; 6],
    /// Distinct workspace crates (externals excluded).
    unique_crates: usize,
}

/// Single-pass fold over the graph payload. Returns `None` when the payload
/// has no `nodes` array (daemon unreachable / malformed).
fn compute_atlas_stats(data: &Value) -> Option<AtlasStats> {
    let nodes = data.get("nodes")?.as_array()?;
    let links = data
        .get("edges")
        .and_then(Value::as_array)
        .map(|a| a.len())
        .unwrap_or(0);

    let mut clusters = [0usize; 6];
    let mut crates_seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
    let (mut orphans, mut unsafe_nodes, mut externals) = (0usize, 0usize, 0usize);

    for n in nodes {
        let krate = n.get("crate").and_then(Value::as_str).unwrap_or("unknown");
        clusters[cluster_idx(krate)] += 1;
        if n.get("is_orphan").and_then(Value::as_bool).unwrap_or(false) {
            orphans += 1;
        }
        if n.get("has_unsafe")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            unsafe_nodes += 1;
        }
        let is_ext = krate == "external"
            || n.get("is_external")
                .and_then(Value::as_bool)
                .unwrap_or(false);
        if is_ext {
            externals += 1;
            continue; // externals stay out of the workspace crate census
        }
        crates_seen.insert(krate);
    }

    Some(AtlasStats {
        nodes: nodes.len(),
        links,
        orphans,
        unsafe_nodes,
        externals,
        clusters,
        unique_crates: crates_seen.len(),
    })
}

/// Thousands separator (pt-BR style: `2.169`).
fn fmt_n(n: u64) -> String {
    let s = n.to_string();
    let len = s.len();
    let mut out = String::with_capacity(len + len / 3);
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (len - i).is_multiple_of(3) {
            out.push('.');
        }
        out.push(ch);
    }
    out
}

/// Render mode from the WebGL availability probe (SPEC §8.2 fallback
/// gate). Pure + testable — the `web_sys` probe lives in the mount
/// Effect and only feeds the boolean in.
pub fn render_mode(webgl_available: bool) -> &'static str {
    if webgl_available { "3d" } else { "2d" }
}

/// Pauling shell (1-based) for a node's total degree — hubs innermost
/// (SPEC §8.3: shell = degree bucket). Always returns 1..=5.
pub fn shell_of_degree(deg: usize) -> usize {
    match deg {
        d if d >= 64 => 1,
        d if d >= 16 => 2,
        d if d >= 4 => 3,
        d if d >= 1 => 4,
        _ => 5,
    }
}

/// Top `cap` nodes by total degree (`computed_fan_in/out` with
/// `fan_in/out` fallback): `(label, degree, is_orphan)`, degree desc,
/// label asc as a deterministic tie-break.
pub fn top_nodes_by_degree(data: &Value, cap: usize) -> Vec<(String, usize, bool)> {
    let Some(nodes) = data.get("nodes").and_then(Value::as_array) else {
        return Vec::new();
    };
    let mut out: Vec<(String, usize, bool)> = nodes
        .iter()
        .map(|n| {
            let label = n
                .get("label")
                .and_then(Value::as_str)
                .or_else(|| n.get("id").and_then(Value::as_str))
                .unwrap_or("?")
                .to_string();
            let fan_in = n
                .get("computed_fan_in")
                .and_then(Value::as_u64)
                .or_else(|| n.get("fan_in").and_then(Value::as_u64))
                .unwrap_or(0);
            let fan_out = n
                .get("computed_fan_out")
                .and_then(Value::as_u64)
                .or_else(|| n.get("fan_out").and_then(Value::as_u64))
                .unwrap_or(0);
            let orphan = n.get("is_orphan").and_then(Value::as_bool).unwrap_or(false);
            (label, (fan_in + fan_out) as usize, orphan)
        })
        .collect();
    out.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    out.truncate(cap);
    out
}

/// Workspace crates ranked by node count (externals excluded), top
/// `top` entries: `(crate, node_count)` desc, name asc tie-break.
pub fn crate_counts(data: &Value, top: usize) -> Vec<(String, usize)> {
    let Some(nodes) = data.get("nodes").and_then(Value::as_array) else {
        return Vec::new();
    };
    let mut per: HashMap<&str, usize> = HashMap::new();
    for n in nodes {
        let krate = n.get("crate").and_then(Value::as_str).unwrap_or("unknown");
        let is_ext = krate == "external"
            || n.get("is_external")
                .and_then(Value::as_bool)
                .unwrap_or(false);
        if is_ext {
            continue;
        }
        *per.entry(krate).or_insert(0) += 1;
    }
    let mut out: Vec<(String, usize)> = per.into_iter().map(|(k, c)| (k.to_string(), c)).collect();
    out.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    out.truncate(top);
    out
}

/// Deterministic 2D fallback geometry (SPEC §8.3): top-`cap` nodes by
/// degree, each placed on its degree-bucket shell — same Pauling-shell
/// ellipse geometry as [`DepthRings`] (ry = 0.78·rx).
pub fn fallback_rings(data: &Value, cap: usize) -> (Vec<DepthRing>, Vec<RingNode>) {
    let top = top_nodes_by_degree(data, cap);
    let mut counts = [0usize; 5];
    for (_, deg, _) in &top {
        counts[shell_of_degree(*deg) - 1] += 1;
    }
    let mut next_idx = [0usize; 5];
    let mut ring_nodes = Vec::with_capacity(top.len());
    for (label, deg, orphan) in top {
        let depth = shell_of_degree(deg);
        let angle_idx = next_idx[depth - 1];
        next_idx[depth - 1] += 1;
        ring_nodes.push(RingNode {
            label,
            depth,
            angle_idx,
            orphan,
        });
    }
    let rings = (1..=5)
        .filter(|d| counts[d - 1] > 0)
        .map(|d| DepthRing {
            depth: d,
            count: counts[d - 1],
        })
        .collect();
    (rings, ring_nodes)
}

/// Workspace graph view — atlas-grade frame around the Pauling-shell engine.
#[component]
pub fn WorkspaceGraph() -> impl IntoView {
    let graph_data =
        LocalResource::new(
            || async move { fetch_viz_workspace_json().await.unwrap_or(Value::Null) },
        );

    /* ── Derived aggregates (REAL numbers from the live payload) ────── */
    let atlas = Memo::new(move |_| {
        graph_data.get().and_then(|d| {
            if d.is_null() {
                None
            } else {
                compute_atlas_stats(&d)
            }
        })
    });
    let hero_stat = Signal::derive(move || {
        atlas
            .get()
            .map(|a| fmt_n(a.nodes as u64))
            .unwrap_or_else(|| "—".to_string())
    });
    let links_kpi = Signal::derive(move || {
        atlas
            .get()
            .map(|a| fmt_n(a.links as u64))
            .unwrap_or_else(|| "—".to_string())
    });
    let crates_kpi = Signal::derive(move || {
        atlas
            .get()
            .map(|a| fmt_n(a.unique_crates as u64))
            .unwrap_or_else(|| "—".to_string())
    });
    let orphans_kpi = Signal::derive(move || {
        atlas
            .get()
            .map(|a| fmt_n(a.orphans as u64))
            .unwrap_or_else(|| "—".to_string())
    });

    /* Top workspace crates by node count (right rail, SPEC §8.2.5). */
    let crates_top = Memo::new(move |_| {
        graph_data
            .get()
            .filter(|d| !d.is_null())
            .map(|d| crate_counts(&d, 10))
            .unwrap_or_default()
    });

    /* 2D fallback geometry — only evaluated when the fallback renders. */
    let fallback_viz = Memo::new(move |_| {
        graph_data
            .get()
            .filter(|d| !d.is_null())
            .map(|d| fallback_rings(&d, 120))
            .unwrap_or_default()
    });
    let fb_rings = Signal::derive(move || fallback_viz.get().0);
    let fb_nodes = Signal::derive(move || fallback_viz.get().1);

    /* ── WebGL probe (SPEC §8.2 fallback gate) ──────────────────────── */
    /* Optimistic default (3D); the synchronous mount Effect settles the
    real value before the async payload can trigger engine init. */
    let (webgl_ok, set_webgl_ok) = signal(true);
    Effect::new(move |_| {
        #[cfg(target_arch = "wasm32")]
        set_webgl_ok.set(detect_webgl());
        #[cfg(not(target_arch = "wasm32"))]
        let _ = &set_webgl_ok;
    });

    /* ── UI state signals ───────────────────────────────────────────── */
    let (show_orphans, set_show_orphans) = signal(true);
    let (show_tests, set_show_tests) = signal(true);
    let (show_particles, set_show_particles) = signal(true);
    let (search_query, set_search_query) = signal(String::new());
    let (selected_crate, set_selected_crate) = signal(String::from("all"));
    let (aspect_lens, set_aspect_lens) = signal(String::from("pauling"));
    let (view_mode, set_view_mode) = signal(String::from("3d"));
    let (selected_chain, set_selected_chain) = signal(String::new());

    /* One signal per Pauling shell layer (default = enabled).
    Indexes line up with SHELLS array order: 0=Nucleus, 1=K, 2=L, 3=M, 4=N, 5=External(99). */
    let shell_signals: [(ReadSignal<bool>, WriteSignal<bool>); 6] = [
        signal(true),
        signal(true),
        signal(true),
        signal(true),
        signal(true),
        signal(true),
    ];

    /* Bridges populated by JS via custom events. Native build binds
    the setters to `_` so cargo check on the host target is warning-free. */
    #[cfg(target_arch = "wasm32")]
    let (available_crates, set_available_crates) = signal(Vec::<String>::new());
    #[cfg(not(target_arch = "wasm32"))]
    let (available_crates, _) = signal(Vec::<String>::new());
    #[cfg(target_arch = "wasm32")]
    let (available_chains, set_available_chains) = signal(Vec::<(String, String, usize)>::new());
    #[cfg(not(target_arch = "wasm32"))]
    let (available_chains, _) = signal(Vec::<(String, String, usize)>::new());

    /* ── Custom event listeners (JS → Leptos) ───────────────────────── */
    Effect::new(move |_| {
        #[cfg(target_arch = "wasm32")]
        {
            use wasm_bindgen::JsCast;

            let crates_cb = Closure::wrap(Box::new(move |e: web_sys::CustomEvent| {
                if let Some(arr) = e.detail().dyn_ref::<js_sys::Array>() {
                    let mut crates: Vec<String> = (0..arr.length())
                        .filter_map(|i| arr.get(i).as_string())
                        .collect();
                    crates.sort();
                    set_available_crates.set(crates);
                }
            }) as Box<dyn FnMut(_)>);

            let window = web_sys::window().expect("WASM must have a window object");
            let _ = window.add_event_listener_with_callback(
                "workspace-crates-loaded",
                crates_cb.as_ref().unchecked_ref(),
            );
            crates_cb.forget();

            let chains_cb = Closure::wrap(Box::new(move |e: web_sys::CustomEvent| {
                let detail = e.detail();
                let mut out: Vec<(String, String, usize)> = Vec::new();
                if let Some(arr) = detail.dyn_ref::<js_sys::Array>() {
                    for i in 0..arr.length() {
                        let item = arr.get(i);
                        if let Some(o) = item.dyn_ref::<js_sys::Object>() {
                            let id = js_sys::Reflect::get(o, &JsValue::from_str("id"))
                                .ok()
                                .and_then(|v| v.as_string())
                                .unwrap_or_default();
                            let label = js_sys::Reflect::get(o, &JsValue::from_str("label"))
                                .ok()
                                .and_then(|v| v.as_string())
                                .unwrap_or_default();
                            let length = js_sys::Reflect::get(o, &JsValue::from_str("length"))
                                .ok()
                                .and_then(|v| v.as_f64())
                                .unwrap_or(0.0) as usize;
                            if !id.is_empty() {
                                out.push((id, label, length));
                            }
                        }
                    }
                }
                set_available_chains.set(out);
            }) as Box<dyn FnMut(_)>);

            let _ = window.add_event_listener_with_callback(
                "workspace-chains-loaded",
                chains_cb.as_ref().unchecked_ref(),
            );
            chains_cb.forget();
        }
    });

    /* ── JS bridge: init graph when data arrives ─────────────────────── */
    Effect::new(move |_| {
        /* FALLBACK 2D gate (SPEC §8.2): without WebGL the 3D engine is
        never booted — the canvas cell renders the SVG shells instead. */
        if render_mode(webgl_ok.get()) != "3d" {
            return;
        }
        if let Some(data) = graph_data.get()
            && !data.is_null()
            && let Ok(json_str) = serde_json::to_string(&data)
        {
            #[cfg(target_arch = "wasm32")]
            {
                js_init_force_graph(&json_str);
            }
            #[cfg(not(target_arch = "wasm32"))]
            let _ = json_str;
        }
    });

    /* ── JS bridge: filter changes (orphans/tests/particles/target) ──── */
    Effect::new(move |_| {
        let orphans = show_orphans.get();
        let tests = show_tests.get();
        let particles = show_particles.get();
        let target = selected_crate.get();
        #[cfg(target_arch = "wasm32")]
        {
            js_update_filters(orphans, tests, particles, &target);
        }
        #[cfg(not(target_arch = "wasm32"))]
        let _ = (orphans, tests, particles, target);
    });

    /* ── JS bridge: aspect lens change ──────────────────────────────── */
    Effect::new(move |_| {
        let lens = aspect_lens.get();
        #[cfg(target_arch = "wasm32")]
        {
            js_set_aspect_lens(&lens);
        }
        #[cfg(not(target_arch = "wasm32"))]
        let _ = lens;
    });

    /* ── JS bridge: view mode change ────────────────────────────────── */
    Effect::new(move |_| {
        let mode = view_mode.get();
        #[cfg(target_arch = "wasm32")]
        {
            js_set_view_mode(&mode);
        }
        #[cfg(not(target_arch = "wasm32"))]
        let _ = mode;
    });

    /* ── JS bridge: workflow chain selection ─────────────────────────── */
    Effect::new(move |_| {
        let cid = selected_chain.get();
        /* Always forward — empty string clears the chain scope on JS side. */
        #[cfg(target_arch = "wasm32")]
        {
            js_set_workflow_chain(&cid);
        }
        #[cfg(not(target_arch = "wasm32"))]
        let _ = cid;
    });

    /* ── JS bridge: per-shell visibility toggles ─────────────────────── */
    for (idx, &(shell_level, _, _, _)) in SHELLS.iter().enumerate() {
        let read_sig = shell_signals[idx].0;
        Effect::new(move |_| {
            let enabled = read_sig.get();
            #[cfg(target_arch = "wasm32")]
            {
                js_set_shell_enabled(shell_level, enabled);
            }
            #[cfg(not(target_arch = "wasm32"))]
            let _ = (shell_level, enabled);
        });
    }

    /* ── Search submit handler ──────────────────────────────────────── */
    let on_search = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let q = search_query.get_untracked();
        if q.is_empty() {
            return;
        }
        #[cfg(target_arch = "wasm32")]
        {
            js_search_graph(&q);
        }
        #[cfg(not(target_arch = "wasm32"))]
        let _ = q;
    };

    let recenter = move |_| {
        #[cfg(target_arch = "wasm32")]
        {
            js_recenter();
        }
    };

    let close_detail_panel = move |_| {
        #[cfg(target_arch = "wasm32")]
        {
            js_close_panel();
        }
    };

    view! {
        <div class="page pg-atlas">
            <Title text="Touring — Workspace Atlas"/>

            // ── Hero (compact, full-bleed) — REAL node count in the title ────
            <header class="el-hero pg-atlas-hero">
                <div class="el-hero-main">
                    <div class="el-eyebrow">"CODE INTELLIGENCE · ATLAS"</div>
                    <h1 class="el-hero-title">
                        {move || format!("{} nodes,", hero_stat.get())}
                        <em class="el-hero-em">" one sphere."</em>
                    </h1>
                    <p class="el-hero-sub">
                        "Symbol-level knowledge graph rendered as concentric Pauling shells. External crates form a luminous outer pearl; switch the aspect lens to recolor the field by quality, cognition, safety or centrality."
                    </p>
                    <div class="el-hero-actions">
                        <a class="el-btn el-btn-sm" href="/chains">"Chains"</a>
                        <a class="el-btn el-btn-sm" href="/wiring">"Wiring"</a>
                    </div>
                </div>
                <div class="el-hero-stat">
                    <div class="el-hero-stat-value">{move || links_kpi.get()}</div>
                    <div class="el-eyebrow">"GRAPH EDGES"</div>
                </div>
            </header>

            // ── KPI strip — live aggregates from /api/viz/workspace ──────────
            <KpiStrip>
                <KpiCell label="Nodes" value=hero_stat sub="indexed symbols"/>
                <KpiCell label="Edges" value=links_kpi sub="import links"/>
                <KpiCell label="Crates" value=crates_kpi sub="workspace · unique"/>
                <KpiCell label="Orphans" value=orphans_kpi sub="dashed ring"/>
            </KpiStrip>

            // ── Atlas grid: 232px Layers · 1fr canvas · 320px rail ───────────
            <div class="pg-atlas-body">

                // ════ LEFT — Layers rail ═════════════════════════════════════
                <aside class="pg-atlas-side">

                    // Crate-family clusters — counts derived from live payload.
                    <Panel eyebrow="LAYERS" cmd="crate families · live counts">
                        {move || atlas.get().map(|a| view! {
                            <div class="pg-atlas-clusters">
                                {CLUSTERS.iter().enumerate().map(|(i, &(label, color))| {
                                    let count = a.clusters[i];
                                    view! {
                                        <div class="pg-atlas-cluster-row">
                                            <span
                                                class="pg-atlas-cluster-dot"
                                                style=format!("background: {color}; box-shadow: 0 0 8px {color};")
                                            ></span>
                                            <span class="pg-atlas-cluster-name">{label}</span>
                                            <span class="pg-atlas-cluster-count">{fmt_n(count as u64)}</span>
                                        </div>
                                    }
                                }).collect_view()}
                            </div>
                        })}

                        // Graph stats — real totals from the JSON.
                        {move || atlas.get().map(|a| view! {
                            <div class="pg-atlas-stats">
                                <div class="pg-atlas-stat">
                                    <span class="pg-atlas-stat-num">{fmt_n(a.nodes as u64)}</span>
                                    <span class="pg-atlas-stat-lbl">"nodes"</span>
                                </div>
                                <div class="pg-atlas-stat">
                                    <span class="pg-atlas-stat-num">{fmt_n(a.links as u64)}</span>
                                    <span class="pg-atlas-stat-lbl">"links"</span>
                                </div>
                                <div class="pg-atlas-stat">
                                    <span class="pg-atlas-stat-num">{fmt_n(a.orphans as u64)}</span>
                                    <span class="pg-atlas-stat-lbl">"orphans"</span>
                                </div>
                                <div class="pg-atlas-stat">
                                    <span class="pg-atlas-stat-num">{fmt_n(a.unsafe_nodes as u64)}</span>
                                    <span class="pg-atlas-stat-lbl">"unsafe"</span>
                                </div>
                                <div class="pg-atlas-stat">
                                    <span class="pg-atlas-stat-num">{fmt_n(a.externals as u64)}</span>
                                    <span class="pg-atlas-stat-lbl">"external"</span>
                                </div>
                            </div>
                        })}
                    </Panel>

                    // Optics — aspect lens, target nucleus, workflow chain.
                    <Panel eyebrow="OPTICS" cmd="lens · nucleus · chain">
                        <div class="pg-atlas-field">
                            <div class="pg-atlas-field-label">
                                "Aspect lens"
                                <span class="pg-atlas-field-hint">{move || aspect_lens.get()}</span>
                            </div>
                            // Aspect lens (SPEC §8.2.4) — all five lenses are
                            // LIVE: the JS engine already recolors per lens
                            // (window.setAspectLens → AspectLens.nodeColor).
                            <div class="pg-atlas-lens pg-atlas-seg">
                                {LENSES.iter().map(|(id, label, hint)| {
                                    let id_val = id.to_string();
                                    let id_for_click = id_val.clone();
                                    let id_for_class = id_val.clone();
                                    view! {
                                        <button
                                            type="button"
                                            class="el-btn el-btn-sm"
                                            title=hint.to_string()
                                            class:active=move || aspect_lens.get() == id_for_class
                                            on:click=move |_| set_aspect_lens.set(id_for_click.clone())
                                        >
                                            {label.to_string()}
                                        </button>
                                    }
                                }).collect_view()}
                            </div>
                        </div>

                        <div class="pg-atlas-field">
                            <div class="pg-atlas-field-label">"Target crate · nucleus"</div>
                            <select
                                class="pg-atlas-select"
                                on:change=move |ev| set_selected_crate.set(event_target_value(&ev))
                            >
                                <option value="all">"All workspace · multi-nucleus"</option>
                                {move || available_crates.get().into_iter().map(|c| {
                                    let c_val = c.clone();
                                    let c_text = c.clone();
                                    view! {
                                        <option value=c_val prop:selected=move || selected_crate.get() == c>{c_text}</option>
                                    }
                                }).collect_view()}
                            </select>
                        </div>

                        // Workflow chain picker (visible whenever chains were detected)
                        {move || {
                            let chains = available_chains.get();
                            if chains.is_empty() { return view! { <div></div> }.into_any(); }
                            view! {
                                <div class="pg-atlas-field">
                                    <div class="pg-atlas-field-label">
                                        "Workflow chain"
                                        <span class="pg-atlas-field-hint">{format!("{} detected", chains.len())}</span>
                                    </div>
                                    <select
                                        class="pg-atlas-select"
                                        on:change=move |ev| set_selected_chain.set(event_target_value(&ev))
                                    >
                                        <option value="">"— pick a chain —"</option>
                                        {chains.into_iter().map(|(cid, label, length)| {
                                            let id_for_select = cid.clone();
                                            let id_for_eq = cid.clone();
                                            view! {
                                                <option
                                                    value=id_for_select
                                                    prop:selected=move || selected_chain.get() == id_for_eq
                                                >
                                                    {format!("[{:>2}] {}", length, label)}
                                                </option>
                                            }
                                        }).collect_view()}
                                    </select>
                                </div>
                            }.into_any()
                        }}
                    </Panel>

                    // Pauling shells + node-class filters (preserved engine toggles).
                    <Panel eyebrow="SHELLS" cmd="pauling layers · filters">
                        <div class="pg-atlas-toggles">
                            {SHELLS.iter().enumerate().map(|(idx, &(_lvl, label, swatch, hint))| {
                                let read_sig = shell_signals[idx].0;
                                let write_sig = shell_signals[idx].1;
                                view! {
                                    <label class="pg-atlas-toggle" title=hint.to_string()>
                                        <input type="checkbox"
                                            prop:checked=move || read_sig.get()
                                            on:change=move |_| write_sig.update(|v| *v = !*v)
                                        />
                                        <span class="pg-atlas-toggle-dot" style=format!("--swatch: {swatch};")></span>
                                        <span class="pg-atlas-toggle-name">{label.to_string()}</span>
                                    </label>
                                }
                            }).collect_view()}
                        </div>

                        <div class="pg-atlas-toggles">
                            <label class="pg-atlas-toggle" title="Show orphan symbols (dashed ring)">
                                <input type="checkbox" prop:checked=move || show_orphans.get()
                                    on:change=move |_| set_show_orphans.update(|v| *v = !*v) />
                                <span class="pg-atlas-toggle-dot" style="--swatch: #ff5c7a;"></span>
                                <span class="pg-atlas-toggle-name">"Orphans"</span>
                            </label>
                            <label class="pg-atlas-toggle" title="Show test files (dotted aura)">
                                <input type="checkbox" prop:checked=move || show_tests.get()
                                    on:change=move |_| set_show_tests.update(|v| *v = !*v) />
                                <span class="pg-atlas-toggle-dot" style="--swatch: #a78bfa;"></span>
                                <span class="pg-atlas-toggle-name">"Test files"</span>
                            </label>
                            <label class="pg-atlas-toggle" title="Animate data-flow particles along links">
                                <input type="checkbox" prop:checked=move || show_particles.get()
                                    on:change=move |_| set_show_particles.update(|v| *v = !*v) />
                                <span class="pg-atlas-toggle-dot" style="--swatch: #5eead4;"></span>
                                <span class="pg-atlas-toggle-name">"Flow particles"</span>
                            </label>
                        </div>
                    </Panel>

                    // Visual encoding legend — mirrors what the engine renders.
                    <Panel eyebrow="ENCODING" cmd="shape · ring · pulse">
                        <div class="pg-atlas-legend">
                            <div class="pg-atlas-legend-row">
                                <span class="pg-atlas-legend-swatch" style="background: #00e5c0;"></span>
                                <span>"Nucleus · K-shell"</span>
                            </div>
                            <div class="pg-atlas-legend-row">
                                <span class="pg-atlas-legend-swatch" style="background: #26c6da;"></span>
                                <span>"L-shell · direct deps"</span>
                            </div>
                            <div class="pg-atlas-legend-row">
                                <span class="pg-atlas-legend-swatch" style="background: #7c4dff;"></span>
                                <span>"M/N-shell · indirect"</span>
                            </div>
                            <div class="pg-atlas-legend-row">
                                <span class="pg-atlas-legend-swatch pg-atlas-glow" style="background: #ffffff;"></span>
                                <span>"External pearl"</span>
                            </div>
                            <div class="pg-atlas-legend-row">
                                <span class="pg-atlas-legend-swatch pg-atlas-diamond" style="background: #00e5c0;"></span>
                                <span>"Fan-in heavy ◇"</span>
                            </div>
                            <div class="pg-atlas-legend-row">
                                <span class="pg-atlas-legend-swatch pg-atlas-square" style="background: #00e5c0;"></span>
                                <span>"Fan-out heavy ▢"</span>
                            </div>
                            <div class="pg-atlas-legend-row">
                                <span class="pg-atlas-legend-swatch pg-atlas-dashed" style="color: #ff5c7a;"></span>
                                <span>"Orphan ring"</span>
                            </div>
                            <div class="pg-atlas-legend-row">
                                <span class="pg-atlas-legend-swatch pg-atlas-glow" style="background: #ff1744;"></span>
                                <span>"Unsafe core (pulses)"</span>
                            </div>
                        </div>
                    </Panel>
                </aside>

                // ════ CENTER — toolbar + graph canvas (engine, untouched) ════
                <section class="pg-atlas-center">

                    <div class="pg-atlas-toolbar">
                        <span class="pg-atlas-toolbar-title">"atlas://workspace"</span>
                        <span class="pg-atlas-toolbar-counts">
                            {move || atlas.get()
                                .map(|a| format!("{} nodes · {} links",
                                    fmt_n(a.nodes as u64), fmt_n(a.links as u64)))
                                .unwrap_or_else(|| "charting…".to_string())}
                        </span>
                        <span class="pg-atlas-toolbar-spacer"></span>

                        <form class="pg-atlas-search" on:submit=on_search>
                            <input
                                type="text"
                                placeholder="symbol or path…"
                                prop:value=move || search_query.get()
                                on:input=move |ev| set_search_query.set(event_target_value(&ev))
                            />
                            <button type="submit" title="Fly the camera to the best match">"Fly"</button>
                        </form>

                        <div class="pg-atlas-toolbar-group">
                            <div class="pg-atlas-seg">
                                <button
                                    type="button"
                                    class="el-btn el-btn-sm"
                                    class:active=move || view_mode.get() == "3d"
                                    on:click=move |_| set_view_mode.set("3d".to_string())
                                >
                                    "Sphere"
                                </button>
                                <button
                                    type="button"
                                    class="el-btn el-btn-sm"
                                    class:active=move || view_mode.get() == "chain"
                                    on:click=move |_| set_view_mode.set("chain".to_string())
                                >
                                    "Chain"
                                </button>
                            </div>
                            <button class="el-btn el-btn-sm" on:click=recenter>"Recenter"</button>
                        </div>
                    </div>

                    // Canvas mount — id + Suspense/null handling preserved verbatim;
                    // the JS ResizeObserver adapts the renderer to this cell. When
                    // WebGL is unavailable the deterministic SVG fallback renders
                    // instead and the 3D engine is never booted (SPEC §8.2/§8.3).
                    <div class="pg-atlas-canvas">
                        <Suspense fallback=|| view! {
                            <div class="pg-atlas-msg">
                                "Initializing the atlas…"
                            </div>
                        }>
                            {move || {
                                graph_data.get().map(|data: serde_json::Value| {
                                    if data.is_null() {
                                        view! {
                                            <div class="pg-atlas-msg pg-atlas-msg-error">
                                                "Workspace graph data is unavailable. Confirm the daemon is reachable at /api/viz/workspace."
                                            </div>
                                        }.into_any()
                                    } else if render_mode(webgl_ok.get()) == "3d" {
                                        view! {
                                            <div id="workspace-graph-container"></div>
                                        }.into_any()
                                    } else {
                                        view! {
                                            <div class="pg-atlas-fallback">
                                                <div class="el-banner el-banner-warn pg-atlas-banner">
                                                    "2D fallback · WebGL unavailable"
                                                </div>
                                                <DepthRings
                                                    nucleus=Signal::derive(|| "workspace".to_string())
                                                    rings=fb_rings
                                                    nodes=fb_nodes
                                                    active_depth=Signal::derive(|| 1usize)
                                                />
                                            </div>
                                        }.into_any()
                                    }
                                })
                            }}
                        </Suspense>
                    </div>
                </section>

                // ════ RIGHT — 320px rail: dossier · top crates · chains ═════
                <aside class="pg-atlas-rail">

                    // Node dossier — ENGINE CONTRACT: id `node-details-panel` is
                    // revealed by JS via `.open`; every `nd-*` id below is written
                    // by JS on node click/hover (it also injects `.ndflag` /
                    // `.nd-edge` / `.nd-empty` children, styled globally). Markup
                    // preserved verbatim; only the Rust-side class dialect moved
                    // to `.pg-atlas-*`.
                    <aside id="node-details-panel" class="pg-atlas-dossier">
                        <div class="pg-atlas-nd-head">
                            <div>
                                <div class="el-eyebrow pg-atlas-nd-eyebrow">"node dossier"</div>
                                <h3 id="nd-title" class="pg-atlas-nd-title">"Node Name"</h3>
                                <div id="nd-id" class="pg-atlas-nd-id">"-"</div>
                            </div>
                            <button class="pg-atlas-nd-close" on:click=close_detail_panel title="Close">"×"</button>
                        </div>

                        <div id="nd-flags" class="pg-atlas-nd-flags"></div>

                        <div class="pg-atlas-nd-stats">
                            <div class="pg-atlas-nd-stat">
                                <span class="pg-atlas-nd-stat-label">"Crate"</span>
                                <span id="nd-crate" class="pg-atlas-nd-stat-value">"-"</span>
                            </div>
                            <div class="pg-atlas-nd-stat">
                                <span class="pg-atlas-nd-stat-label">"Shell"</span>
                                <span id="nd-shell" class="pg-atlas-nd-stat-value">"-"</span>
                            </div>
                            <div class="pg-atlas-nd-stat">
                                <span class="pg-atlas-nd-stat-label">"Quality"</span>
                                <span id="nd-score" class="pg-atlas-nd-stat-value">"-"</span>
                            </div>
                            <div class="pg-atlas-nd-stat">
                                <span class="pg-atlas-nd-stat-label">"Cognitive"</span>
                                <span id="nd-cog" class="pg-atlas-nd-stat-value">"-"</span>
                            </div>
                            <div class="pg-atlas-nd-stat">
                                <span class="pg-atlas-nd-stat-label">"Fan-in"</span>
                                <span id="nd-fanin" class="pg-atlas-nd-stat-value">"-"</span>
                            </div>
                            <div class="pg-atlas-nd-stat">
                                <span class="pg-atlas-nd-stat-label">"Fan-out"</span>
                                <span id="nd-fanout" class="pg-atlas-nd-stat-value">"-"</span>
                            </div>
                            <div class="pg-atlas-nd-stat">
                                <span class="pg-atlas-nd-stat-label">"Shape"</span>
                                <span id="nd-shape" class="pg-atlas-nd-stat-value">"-"</span>
                            </div>
                        </div>

                        <div class="pg-atlas-nd-radar-wrap">
                            <canvas id="nd-radar" width="240" height="220"></canvas>
                        </div>

                        <div class="pg-atlas-nd-section-title">"Connecting symbols"</div>
                        <div id="nd-edges" class="pg-atlas-nd-edges"></div>
                    </aside>

                    // Hidden automatically when the dossier opens (pure CSS).
                    <div class="pg-atlas-hint">
                        "Click any node in the field to open its dossier — crate, shell, quality, cognition, fan-in/out and connecting symbols."
                    </div>

                    // Top workspace crates by node count — client-side Memo
                    // over the live payload (SPEC §8.2.5).
                    <Panel eyebrow="INSPECTOR" cmd="top crates · node count">
                        {move || {
                            let rows = crates_top.get();
                            let max = rows.first().map(|t| t.1.max(1)).unwrap_or(1);
                            view! {
                                <table class="el-table pg-atlas-table">
                                    <thead>
                                        <tr>
                                            <th>"crate"</th>
                                            <th>"nodes"</th>
                                        </tr>
                                    </thead>
                                    <tbody>
                                        {rows.into_iter().map(|(name, count)| {
                                            let pct = (count as f64 / max as f64 * 100.0).round();
                                            view! {
                                                <tr>
                                                    <td class="pg-atlas-table-crate">
                                                        <span class="pg-atlas-bar" style=format!("--w: {pct}%;")></span>
                                                        <span>{name}</span>
                                                    </td>
                                                    <td class="el-mono pg-atlas-table-num">{fmt_n(count as u64)}</td>
                                                </tr>
                                            }
                                        }).collect_view()}
                                    </tbody>
                                </table>
                            }
                        }}
                    </Panel>

                    <a class="el-btn el-btn-sm pg-atlas-chain-link" href="/chains">"WORKFLOW CHAIN →"</a>
                </aside>

            </div>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Payload mirroring the REAL /api/viz/workspace node shape
    /// (captured 12/06/2026: computed_fan_in/out, crate, is_external,
    /// is_orphan, label, id).
    fn sample() -> Value {
        json!({
            "nodes": [
                {"id": "a.rs", "label": "hub.rs", "crate": "touring-hooks",
                 "computed_fan_in": 80, "computed_fan_out": 10, "is_orphan": false},
                {"id": "b.rs", "label": "mid.rs", "crate": "touring-hooks",
                 "computed_fan_in": 12, "computed_fan_out": 8, "is_orphan": true},
                {"id": "c.rs", "label": "leaf.rs", "crate": "touring-web",
                 "fan_in": 2, "fan_out": 1, "is_orphan": false},
                {"id": "d.rs", "label": "ext.rs", "crate": "external",
                 "computed_fan_in": 63, "computed_fan_out": 0, "is_external": true},
                {"id": "e.rs", "label": "lone.rs", "crate": "touring-web",
                 "computed_fan_in": 0, "computed_fan_out": 0, "is_orphan": true}
            ],
            "edges": [
                {"from": "a.rs", "to": "b.rs", "kind": "imports"},
                {"from": "b.rs", "to": "c.rs", "kind": "imports"}
            ]
        })
    }

    #[test]
    fn render_mode_gates_fallback() {
        assert_eq!(render_mode(true), "3d");
        assert_eq!(render_mode(false), "2d");
    }

    #[test]
    fn shell_of_degree_buckets_hubs_innermost() {
        assert_eq!(shell_of_degree(200), 1);
        assert_eq!(shell_of_degree(64), 1);
        assert_eq!(shell_of_degree(63), 2);
        assert_eq!(shell_of_degree(16), 2);
        assert_eq!(shell_of_degree(15), 3);
        assert_eq!(shell_of_degree(4), 3);
        assert_eq!(shell_of_degree(3), 4);
        assert_eq!(shell_of_degree(1), 4);
        assert_eq!(shell_of_degree(0), 5);
    }

    #[test]
    fn top_nodes_by_degree_sorts_desc_and_caps() {
        let top = top_nodes_by_degree(&sample(), 3);
        assert_eq!(top.len(), 3);
        assert_eq!(top[0], ("hub.rs".to_string(), 90, false));
        assert_eq!(top[1], ("ext.rs".to_string(), 63, false));
        assert_eq!(top[2], ("mid.rs".to_string(), 20, true));
        // Malformed payload degrades to empty, never panics.
        assert!(top_nodes_by_degree(&json!({"error": "boom"}), 5).is_empty());
    }

    #[test]
    fn crate_counts_excludes_externals_and_ranks() {
        let counts = crate_counts(&sample(), 10);
        assert_eq!(
            counts,
            vec![
                ("touring-hooks".to_string(), 2),
                ("touring-web".to_string(), 2)
            ]
        );
        // `top` cap respected.
        assert_eq!(crate_counts(&sample(), 1).len(), 1);
        assert!(crate_counts(&Value::Null, 5).is_empty());
    }

    #[test]
    fn fallback_rings_geometry_is_consistent() {
        let (rings, nodes) = fallback_rings(&sample(), 120);
        assert_eq!(nodes.len(), 5);
        // Every node sits on a declared ring and angle_idx < ring count.
        for n in &nodes {
            let ring = rings
                .iter()
                .find(|r| r.depth == n.depth)
                .expect("every node sits on a declared ring");
            assert!(n.angle_idx < ring.count, "angle_idx within ring population");
        }
        // Ring populations sum to the node total; orphan flag survives.
        assert_eq!(rings.iter().map(|r| r.count).sum::<usize>(), nodes.len());
        assert!(nodes.iter().any(|n| n.orphan), "orphan flag propagated");
        // Cap is honored.
        let (_, capped) = fallback_rings(&sample(), 2);
        assert_eq!(capped.len(), 2);
    }

    #[test]
    fn atlas_stats_counts_unique_workspace_crates() {
        let stats = compute_atlas_stats(&sample()).expect("sample payload has nodes");
        assert_eq!(stats.nodes, 5);
        assert_eq!(stats.links, 2);
        assert_eq!(stats.unique_crates, 2, "external excluded from census");
        assert_eq!(stats.externals, 1);
        assert_eq!(stats.orphans, 2);
        assert!(compute_atlas_stats(&Value::Null).is_none());
    }

    #[test]
    fn fmt_n_thousands_separator() {
        assert_eq!(fmt_n(2169), "2.169");
        assert_eq!(fmt_n(170_731), "170.731");
        assert_eq!(fmt_n(999), "999");
    }
}
