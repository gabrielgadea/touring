//! Wiring chains route — Elite W5 re-skin (SPEC 2026-06-12 §5.10, artboard
//! `hifi-elite-chain`): the Pauling-lane SVG engine from the previous wave is
//! preserved verbatim (ranking, shell classification, trigonometric layout);
//! only the chrome changed (`chn-` dialect → `.el-*` / `.pg-chains-*`) and a
//! transport bar + right rail were added on top.
//!
//! Real data only (zero placeholders):
//! - `GET /api/wiring/chains`   → `{"chain_count": N, "rebuilt": bool, "chains"?: [...]}`
//!   (shape captured live 2026-06-12: `{"chain_count":0,"rebuilt":true}` — the
//!   page renders an honest banner pointing at `touring wiring chains --rebuild`)
//! - `GET /api/wiring/modules`  → `[{"file_path", "integration_score",
//!   "orphan_count", "orphan_symbols", "total_pub_symbols"}]` — drives the
//!   lane diagram (4 Pauling shells by integration score).
//! - `GET /api/viz/workspace`   → `{nodes: [{id, crate, ...}], edges: [{from, to}]}`
//!   — drives the node/edge KPIs and the per-lane FAN-IN/FAN-OUT direction
//!   (real in-degree vs out-degree) + top wavefront in the right rail.
//!
//! Transport semantics: the existing SVG has no animated particles, so the
//! transport honestly controls a *progressive lane highlight* — an
//! `RwSignal<usize>` step (0..=3) that lights shells núcleo→externa; ▸ plays
//! the sweep on a wasm interval (0.5×/1×/2×), the scrubber seeks per lane.
//! `prefers-reduced-motion` is honored globally via tokens.css.
//!
//! Deep links (SPEC §8.2): the Atlas links here as `/chains?focus=<symbol>`;
//! the matching module gets a warn-halo + forced label and its lane is
//! auto-selected.
//!
//! CSS: `.el-*` primitives (elite.css) + `.pg-chains-*` page block
//! (/tmp/w5-css/chains.css).

use std::collections::HashMap;

use leptos::prelude::*;
use leptos_meta::Title;
use leptos_router::hooks::use_query_map;

use crate::web::components::escape_html;
use crate::web::components::{KpiCell, KpiStrip, PageHero, Panel};
use crate::web::services::{fetch_viz_workspace_json, fetch_wiring_chains, fetch_wiring_modules};

/// Shell display names, núcleo → externa (Pauling shells == lanes).
const SHELL_NAMES: [&str; 4] = ["núcleo", "shell K", "shell L", "externa"];
/// Score range caption per shell.
const SHELL_RANGES: [&str; 4] = ["score ≥ 0.9", "score ≥ 0.7", "score ≥ 0.4", "score < 0.4"];
/// Node fill per shell — design tokens only (accent / accent / violet / neutral).
const SHELL_FILL: [&str; 4] = [
    "var(--el-accent)",
    "var(--el-accent)",
    "var(--el-violet)",
    "var(--el-fg-4)",
];
/// Node fill-opacity per shell (núcleo brightest, externa dimmest).
const SHELL_OPACITY: [f64; 4] = [0.95, 0.65, 0.80, 0.55];
/// Fill-opacity applied to lanes beyond the transport step (dimmed).
const DIMMED_OPACITY: f64 = 0.18;
/// Orbit radii inside the 640×500 viewBox, núcleo → externa.
const SHELL_RADII: [f64; 4] = [54.0, 108.0, 162.0, 216.0];
/// Vertical squash applied to every orbit (subtle isometric ellipse).
const SHELL_SQUASH: f64 = 0.78;
/// Max nodes drawn in the diagram (top by integration_score) for legibility.
const MAX_DIAGRAM_NODES: usize = 80;
/// How many top modules receive a monospace label.
const MAX_LABELS: usize = 8;

/// One row of `/api/wiring/modules`, defensively parsed.
#[derive(Clone, Debug, PartialEq)]
pub struct ModuleNode {
    /// Module path as reported by the wiring DB.
    pub file_path: String,
    /// Integration score in `[0, 1]` (lane classifier input).
    pub integration_score: f64,
    /// Public symbol count (node radius input).
    pub total_pub_symbols: u64,
}

/// Aggregate stats for one lane (shell), derived from REAL data:
/// membership from `/api/wiring/modules`, degrees from `/api/viz/workspace`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LaneStats {
    /// Modules classified into this lane.
    pub members: usize,
    /// Edges arriving at lane members (real in-degree sum).
    pub in_deg: u64,
    /// Edges leaving lane members (real out-degree sum).
    pub out_deg: u64,
    /// Top wavefront — lane nodes of highest total degree `(label, degree)`.
    pub top: Vec<(String, u64)>,
}

/// Shell (lane) index for an integration score:
/// 0 = núcleo (≥0.9) · 1 = K (≥0.7) · 2 = L (≥0.4) · 3 = externa.
pub fn shell_of(score: f64) -> usize {
    if score >= 0.9 {
        0
    } else if score >= 0.7 {
        1
    } else if score >= 0.4 {
        2
    } else {
        3
    }
}

/// Parse the `/api/wiring/modules` array, skipping malformed entries.
pub fn parse_modules(json: &serde_json::Value) -> Vec<ModuleNode> {
    json.as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|m| {
                    let file_path = m.get("file_path")?.as_str()?.to_string();
                    let integration_score = m
                        .get("integration_score")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.0);
                    let total_pub_symbols = m
                        .get("total_pub_symbols")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);
                    Some(ModuleNode {
                        file_path,
                        integration_score,
                        total_pub_symbols,
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Short label for a module path — the file stem (`…/wiring/chains.rs` → `chains`).
pub fn module_label(path: &str) -> String {
    path.rsplit('/')
        .next()
        .unwrap_or(path)
        .trim_end_matches(".rs")
        .to_string()
}

/// Workspace-relative key for cross-payload path matching — wiring module
/// paths may be absolute while graph node ids are relative (or vice versa),
/// so both are normalized to the substring starting at `crates/`.
pub fn normalize_rel(path: &str) -> Option<&str> {
    path.find("crates/").map(|i| &path[i..])
}

/// Whether a module matches the `?focus=<symbol>` deep link from the Atlas
/// (case-insensitive: exact file stem OR substring of the path).
pub fn focus_matches(file_path: &str, focus: &str) -> bool {
    if focus.is_empty() {
        return false;
    }
    let f = focus.to_ascii_lowercase();
    let p = file_path.to_ascii_lowercase();
    module_label(&p) == f || p.contains(&f)
}

/// Lane flow direction from REAL aggregate degrees:
/// more edges out than in → `FAN-OUT`; the inverse → `FAN-IN`;
/// equal non-zero → `BALANCED`; no mapped edges → `ISOLATED`.
pub fn lane_direction(in_deg: u64, out_deg: u64) -> &'static str {
    if in_deg == 0 && out_deg == 0 {
        "ISOLATED"
    } else if out_deg > in_deg {
        "FAN-OUT"
    } else if in_deg > out_deg {
        "FAN-IN"
    } else {
        "BALANCED"
    }
}

/// Fold lane membership (`/api/wiring/modules`) with the real dependency
/// edges (`/api/viz/workspace`) into per-lane stats: member counts,
/// in/out-degree sums and the top-3 wavefront nodes by total degree.
///
/// Graph endpoints that cannot be mapped to a lane (externals, scripts,
/// paths outside `crates/`) are honestly excluded rather than guessed.
pub fn derive_lane_stats(modules: &[ModuleNode], graph: &serde_json::Value) -> [LaneStats; 4] {
    let mut stats: [LaneStats; 4] = std::array::from_fn(|_| LaneStats::default());

    // Lane membership + normalized-path → lane lookup.
    let mut lane_of: HashMap<&str, usize> = HashMap::new();
    for m in modules {
        let s = shell_of(m.integration_score);
        stats[s].members += 1;
        if let Some(k) = normalize_rel(&m.file_path) {
            lane_of.insert(k, s);
        }
    }

    // Degree aggregation over the real edge list.
    let mut node_deg: HashMap<String, u64> = HashMap::new();
    if let Some(edges) = graph.get("edges").and_then(|v| v.as_array()) {
        for e in edges {
            let from = e
                .get("from")
                .or_else(|| e.get("source"))
                .and_then(|v| v.as_str());
            let to = e
                .get("to")
                .or_else(|| e.get("target"))
                .and_then(|v| v.as_str());
            if let Some(f) = from
                && let Some(&s) = normalize_rel(f).and_then(|k| lane_of.get(k))
            {
                stats[s].out_deg += 1;
                *node_deg.entry(f.to_string()).or_insert(0) += 1;
            }
            if let Some(t) = to
                && let Some(&s) = normalize_rel(t).and_then(|k| lane_of.get(k))
            {
                stats[s].in_deg += 1;
                *node_deg.entry(t.to_string()).or_insert(0) += 1;
            }
        }
    }

    // Top wavefront per lane (highest total degree, deterministic tie-break).
    let mut per_lane: [Vec<(String, u64)>; 4] = std::array::from_fn(|_| Vec::new());
    for (id, deg) in node_deg {
        if let Some(&s) = normalize_rel(&id).and_then(|k| lane_of.get(k)) {
            per_lane[s].push((module_label(&id), deg));
        }
    }
    for (s, mut v) in per_lane.into_iter().enumerate() {
        v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        v.truncate(3);
        stats[s].top = v;
    }

    stats
}

/// Build the Pauling lane SVG from REAL module data — layout algorithm
/// preserved from the previous wave (rank by `integration_score`, cap at
/// `MAX_DIAGRAM_NODES`, classify into 4 concentric shells, place with
/// plain trigonometry). Re-skin additions:
///
/// - all colors flow from design tokens via `style=` (presentation
///   attributes don't resolve `var()` reliably);
/// - `active_lane` drives the transport's progressive highlight — lanes
///   `> active_lane` are dimmed to `DIMMED_OPACITY`, the active orbit
///   ring strokes accent;
/// - `focus` (the `?focus=` deep link) draws a warn halo + forced label
///   on the matching node.
pub fn build_shell_svg(modules: &[ModuleNode], active_lane: usize, focus: Option<&str>) -> String {
    use std::f64::consts::{FRAC_PI_2, TAU};

    const W: f64 = 640.0;
    const H: f64 = 500.0;
    let cx = W / 2.0;
    let cy = H / 2.0;
    let active = active_lane.min(3);

    let mut ranked: Vec<&ModuleNode> = modules.iter().collect();
    ranked.sort_by(|a, b| {
        b.integration_score
            .total_cmp(&a.integration_score)
            .then_with(|| b.total_pub_symbols.cmp(&a.total_pub_symbols))
    });
    ranked.truncate(MAX_DIAGRAM_NODES);

    let labeled: Vec<&str> = ranked
        .iter()
        .take(MAX_LABELS)
        .map(|m| m.file_path.as_str())
        .collect();

    let mut shells: [Vec<&ModuleNode>; 4] = std::array::from_fn(|_| Vec::new());
    for m in &ranked {
        shells[shell_of(m.integration_score)].push(m);
    }

    let mut svg = format!(
        "<svg class=\"pg-chains-svg\" viewBox=\"0 0 {W} {H}\" \
         xmlns=\"http://www.w3.org/2000/svg\" role=\"img\" \
         aria-label=\"Lane diagram — wiring integration shells\">"
    );

    // Dashed orbit grid (active lane ring strokes accent) + nucleus mark.
    for (ri, r) in SHELL_RADII.iter().enumerate() {
        let ry = r * SHELL_SQUASH;
        let (stroke, sop) = if ri == active {
            ("var(--el-accent)", 0.45)
        } else {
            ("var(--el-line-strong)", 0.8)
        };
        svg.push_str(&format!(
            "<ellipse cx=\"{cx}\" cy=\"{cy}\" rx=\"{r}\" ry=\"{ry:.1}\" fill=\"none\" \
             style=\"stroke:{stroke};stroke-opacity:{sop}\" stroke-dasharray=\"3 5\"/>"
        ));
    }
    svg.push_str(&format!(
        "<circle cx=\"{cx}\" cy=\"{cy}\" r=\"2\" style=\"fill:var(--el-fg-4);fill-opacity:.6\"/>"
    ));

    for (s, members) in shells.iter().enumerate() {
        let n = members.len();
        if n == 0 {
            continue;
        }
        let radius = SHELL_RADII[s];
        let fill = SHELL_FILL[s];
        // Progressive transport highlight: lanes beyond the step are dimmed.
        let opacity = if s <= active {
            SHELL_OPACITY[s]
        } else {
            DIMMED_OPACITY
        };
        // Per-shell angular offset so the rings don't align artificially.
        let offset = -FRAC_PI_2 + s as f64 * 0.55;
        for (i, m) in members.iter().enumerate() {
            let angle = offset + TAU * (i as f64) / (n as f64);
            let x = cx + radius * angle.cos();
            let y = cy + radius * SHELL_SQUASH * angle.sin();
            let is_focus = focus
                .map(|f| focus_matches(&m.file_path, f))
                .unwrap_or(false);
            // Node radius 3..7 px ∝ pub-symbol count (clamped at 24);
            // the focused node gets a slightly larger halo.
            let mut r = 3.0 + (m.total_pub_symbols.min(24) as f64 / 24.0) * 4.0;
            if is_focus {
                r += 2.5;
            }
            let style = if is_focus {
                format!("fill:{fill};fill-opacity:{opacity};stroke:var(--el-warn);stroke-width:1.5")
            } else {
                format!("fill:{fill};fill-opacity:{opacity}")
            };
            let title = escape_html(&format!(
                "{} · score {:.2} · {} pub symbols",
                m.file_path, m.integration_score, m.total_pub_symbols
            ));
            let class = if is_focus {
                "pg-chains-node pg-chains-node-focus"
            } else {
                "pg-chains-node"
            };
            svg.push_str(&format!(
                "<circle class=\"{class}\" cx=\"{x:.1}\" cy=\"{y:.1}\" r=\"{r:.1}\" \
                 style=\"{style}\"><title>{title}</title></circle>"
            ));
            if is_focus || labeled.contains(&m.file_path.as_str()) {
                let label = escape_html(&module_label(&m.file_path));
                let lfill = if is_focus {
                    "var(--el-warn)"
                } else {
                    "var(--el-fg-3)"
                };
                let lx = x + r + 5.0;
                let ly = y + 3.0;
                svg.push_str(&format!(
                    "<text x=\"{lx:.1}\" y=\"{ly:.1}\" font-size=\"9.5\" \
                     style=\"fill:{lfill};font-family:var(--el-mono)\">{label}</text>"
                ));
            }
        }
    }

    svg.push_str("</svg>");
    svg
}

/// Render one chain entry defensively — the detailed `chains` array shape is
/// only emitted by newer daemons, so unknown layouts degrade to compact JSON.
pub fn chain_flow_label(chain: &serde_json::Value) -> String {
    let source = chain.get("source").and_then(|v| v.as_str());
    let sink = chain.get("sink").and_then(|v| v.as_str());
    if let (Some(src), Some(snk)) = (source, sink) {
        return format!(
            "<code>{}</code><span class=\"pg-chains-arrow\">→</span><code>{}</code>",
            escape_html(src),
            escape_html(snk)
        );
    }
    let compact: String = chain.to_string();
    let short: String = if compact.chars().count() > 160 {
        let mut truncated: String = compact.chars().take(157).collect();
        truncated.push('…');
        truncated
    } else {
        compact
    };
    format!("<code>{}</code>", escape_html(&short))
}

/// Wiring chains page — `/chains`.
///
/// Layout (artboard `hifi-elite-chain`): compact [`PageHero`] (eyebrow
/// "CODE INTELLIGENCE · CHAIN", Atlas action) · [`KpiStrip`] (4 real KPIs:
/// nodes / edges / lanes / chains) · main column (Panel 01: lane SVG +
/// honest 0-chains banner + transport; Panel 02: functional chains) ·
/// right rail (selected lane direction + wavefront, lane census, focus card).
#[component]
pub fn WiringChains() -> impl IntoView {
    let query = use_query_map();

    // Value::Null marks "endpoint unavailable"; resource None = still loading.
    let chains = LocalResource::new(|| async move {
        fetch_wiring_chains()
            .await
            .unwrap_or(serde_json::Value::Null)
    });
    let modules = LocalResource::new(|| async move {
        fetch_wiring_modules()
            .await
            .unwrap_or(serde_json::Value::Null)
    });
    let graph = LocalResource::new(|| async move {
        fetch_viz_workspace_json()
            .await
            .unwrap_or(serde_json::Value::Null)
    });

    // Transport state — progressive lane highlight (no fake particles).
    let step = RwSignal::new(0usize);
    let playing = RwSignal::new(false);
    let speed = RwSignal::new(1.0f64);

    // ?focus=<symbol> deep link from the Atlas (SPEC §8.2).
    let focus = Memo::new(move |_| {
        query
            .with(|q| q.get("focus"))
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    });

    // Auto-select the focused module's lane once modules arrive.
    let focus_applied = RwSignal::new(false);
    Effect::new(move |_| {
        if focus_applied.get_untracked() {
            return;
        }
        let Some(f) = focus.get() else { return };
        let Some(json) = modules.get() else { return };
        if json.is_null() {
            return;
        }
        if let Some(m) = parse_modules(&json)
            .iter()
            .find(|m| focus_matches(&m.file_path, &f))
        {
            step.set(shell_of(m.integration_score));
            focus_applied.set(true);
        }
    });

    // Play loop — wasm interval advancing the lane step (0.5×/1×/2×).
    // `prefers-reduced-motion` is honored globally (tokens.css); the sweep
    // is user-initiated only (▸), never autoplay.
    #[cfg(target_arch = "wasm32")]
    {
        use std::time::Duration;
        Effect::new(move |prev: Option<Option<IntervalHandle>>| {
            if let Some(Some(handle)) = prev {
                handle.clear();
            }
            if !playing.get() {
                return None;
            }
            let mult = speed.get().max(0.1);
            let ms = ((1400.0 / mult) as u64).max(100);
            set_interval_with_handle(
                move || step.update(|s| *s = (*s + 1) % 4),
                Duration::from_millis(ms),
            )
            .ok()
        });
    }

    // ── Derived REAL values ──────────────────────────────────────────
    let lane_stats = Memo::new(move |_| {
        let mods = modules.get().map(|j| parse_modules(&j)).unwrap_or_default();
        let g = graph.get().unwrap_or(serde_json::Value::Null);
        derive_lane_stats(&mods, &g)
    });

    let svg_memo = Memo::new(move |_| {
        let json = modules.get().unwrap_or(serde_json::Value::Null);
        let mods = parse_modules(&json);
        if mods.is_empty() {
            return String::new();
        }
        build_shell_svg(&mods, step.get(), focus.get().as_deref())
    });

    let chain_count = Signal::derive(move || {
        chains
            .get()
            .and_then(|j| j.get("chain_count").and_then(|v| v.as_u64()))
            .map(|n| n.to_string())
            .unwrap_or_else(|| "—".to_string())
    });
    let kpi_nodes = Signal::derive(move || {
        graph
            .get()
            .and_then(|j| {
                j.get("nodes")
                    .and_then(|v| v.as_array())
                    .map(|a| a.len().to_string())
            })
            .unwrap_or_else(|| "—".to_string())
    });
    let kpi_edges = Signal::derive(move || {
        graph
            .get()
            .and_then(|j| {
                j.get("edges")
                    .and_then(|v| v.as_array())
                    .map(|a| a.len().to_string())
            })
            .unwrap_or_else(|| "—".to_string())
    });
    let kpi_lanes = Signal::derive(move || {
        let available = modules.get().map(|j| !j.is_null()).unwrap_or(false);
        if !available {
            return "—".to_string();
        }
        let occupied = lane_stats.get().iter().filter(|l| l.members > 0).count();
        format!("{occupied} / 4")
    });
    let graph_has_edges = Signal::derive(move || {
        graph
            .get()
            .map(|j| j.get("edges").map(|e| e.is_array()).unwrap_or(false))
            .unwrap_or(false)
    });

    view! {
        <div class="page pg-chains">
            <Title text="Touring — Chains"/>

            <PageHero
                eyebrow="CODE INTELLIGENCE · CHAIN"
                title="Chains"
                title_em="flow"
                sub="Lanes de integração derivadas do wiring DB real — núcleo ≥0.9 · K ≥0.7 · L ≥0.4 · externa abaixo — com transporte para varrer o wavefront lane a lane."
                stat=kpi_nodes
                stat_label="graph nodes"
            >
                <a class="el-btn" href="/workspace">"Atlas"</a>
                <a class="el-btn" href="/wiring">"Wiring"</a>
            </PageHero>

            <KpiStrip>
                <KpiCell label="Nodes" value=kpi_nodes sub="grafo /api/viz/workspace"/>
                <KpiCell label="Edges" value=kpi_edges sub="dependências reais"/>
                <KpiCell label="Lanes" value=kpi_lanes sub="shells ocupadas"/>
                <KpiCell label="Chains" value=chain_count sub="source→sink do daemon"/>
            </KpiStrip>

            <div class="pg-chains-layout">
                // ── Main column ──────────────────────────────────────────
                <div class="pg-chains-main">
                    <Panel num="01" eyebrow="LANE DIAGRAM · INTEGRATION ORBITS" cmd="touring wiring modules">
                        // Honest banner — wiring DB reports zero materialized chains.
                        {move || {
                            let zero = chains
                                .get()
                                .and_then(|j| j.get("chain_count").and_then(|v| v.as_u64()))
                                == Some(0);
                            zero.then(|| view! {
                                <div class="pg-chains-banner">
                                    <span>"0 chains materializadas no wiring DB — reconstrua com"</span>
                                    <code>"touring wiring chains --rebuild"</code>
                                </div>
                            })
                        }}

                        <Suspense fallback=|| view! { <div class="el-skeleton" aria-label="loading"></div> }>
                            {move || modules.get().map(|json| {
                                if json.is_null() {
                                    view! {
                                        <div class="pg-chains-empty">
                                            <p class="el-eyebrow">"daemon indisponível"</p>
                                            <p class="pg-chains-empty-note">
                                                "GET /api/wiring/modules não respondeu JSON — sem dados reais, sem diagrama."
                                            </p>
                                        </div>
                                    }
                                    .into_any()
                                } else {
                                    let mods = parse_modules(&json);
                                    if mods.is_empty() {
                                        view! {
                                            <div class="pg-chains-empty">
                                                <p class="el-eyebrow">"0 módulos no wiring DB"</p>
                                            </div>
                                        }
                                        .into_any()
                                    } else {
                                        let total = mods.len();
                                        let shown = total.min(MAX_DIAGRAM_NODES);
                                        view! {
                                            <div class="pg-chains-diagram-wrap">
                                                <div
                                                    class="pg-chains-svg-host"
                                                    inner_html=move || svg_memo.get()
                                                ></div>
                                                <p class="pg-chains-diagram-note">
                                                    {format!("{shown} de {total} módulos · top integration_score · raio ∝ pub symbols")}
                                                </p>
                                            </div>
                                        }
                                        .into_any()
                                    }
                                }
                            })}
                        </Suspense>

                        // ── Transport — progressive lane sweep ───────────
                        <div class="el-transport pg-chains-transport" role="group" aria-label="lane transport">
                            <button
                                class="el-btn el-btn-sm"
                                aria-label="rewind para o núcleo"
                                on:click=move |_| { playing.set(false); step.set(0); }
                            >"⏮"</button>
                            <button
                                class="el-btn el-btn-sm"
                                aria-label="play/pause varredura de lanes"
                                on:click=move |_| playing.update(|p| *p = !*p)
                            >
                                {move || if playing.get() { "⏸" } else { "▸" }}
                            </button>
                            <button
                                class="el-btn el-btn-sm"
                                aria-label="próxima lane"
                                on:click=move |_| { playing.set(false); step.update(|s| *s = (*s + 1) % 4); }
                            >"⏭"</button>
                            <div
                                class="pg-chains-scrub"
                                role="slider"
                                aria-label="lane scrubber"
                                aria-valuemin="0"
                                aria-valuemax="3"
                                aria-valuenow=move || step.get().min(3).to_string()
                            >
                                <div class="el-prog-track">
                                    <div
                                        class="el-prog-fill"
                                        style=move || format!("width:{}%", (step.get().min(3) + 1) * 25)
                                    ></div>
                                </div>
                                {(0..4usize).map(|s| {
                                    let left = s * 25;
                                    view! {
                                        <button
                                            class="pg-chains-seek-seg"
                                            style=format!("left:{left}%")
                                            aria-label=format!("ir para lane {}", SHELL_NAMES[s])
                                            on:click=move |_| { playing.set(false); step.set(s); }
                                        ></button>
                                    }
                                }).collect_view()}
                            </div>
                            <span class="pg-chains-lane-label">
                                {move || SHELL_NAMES[step.get().min(3)]}
                            </span>
                            <select
                                class="pg-chains-speed"
                                aria-label="velocidade da varredura"
                                on:change=move |ev| {
                                    let v = event_target_value(&ev).parse::<f64>().unwrap_or(1.0);
                                    speed.set(v);
                                }
                            >
                                <option value="0.5">"0.5×"</option>
                                <option value="1" selected=true>"1×"</option>
                                <option value="2">"2×"</option>
                            </select>
                        </div>
                    </Panel>

                    // ── Functional chains ────────────────────────────────
                    <Panel num="02" eyebrow="FUNCTIONAL CHAINS" cmd="touring wiring chains">
                        <Suspense fallback=|| view! { <div class="el-skeleton" aria-label="loading"></div> }>
                            {move || chains.get().map(|json| {
                                if json.is_null() {
                                    view! {
                                        <div class="pg-chains-empty">
                                            <p class="el-eyebrow">"daemon indisponível"</p>
                                            <p class="pg-chains-empty-note">
                                                "GET /api/wiring/chains não respondeu JSON — verifique o touring-web-server / daemon."
                                            </p>
                                        </div>
                                    }
                                    .into_any()
                                } else {
                                    let count = json
                                        .get("chain_count")
                                        .and_then(|v| v.as_u64())
                                        .unwrap_or(0);
                                    let list = json
                                        .get("chains")
                                        .and_then(|v| v.as_array())
                                        .cloned()
                                        .unwrap_or_default();
                                    if count == 0 {
                                        view! {
                                            <div class="pg-chains-empty">
                                                <p class="el-eyebrow">"0 functional chains"</p>
                                                <p class="pg-chains-empty-note">
                                                    "O wiring DB ainda não materializou cadeias source→sink. Reconstrua o grafo com:"
                                                </p>
                                                <code class="pg-chains-empty-cmd">"touring wiring chains --rebuild"</code>
                                            </div>
                                        }
                                        .into_any()
                                    } else if list.is_empty() {
                                        view! {
                                            <div class="pg-chains-empty">
                                                <p class="el-eyebrow">
                                                    {format!("{count} chains — payload sem detalhe")}
                                                </p>
                                                <p class="pg-chains-empty-note">
                                                    "O daemon reportou o total mas esta versão do endpoint não emite o detalhe das cadeias."
                                                </p>
                                            </div>
                                        }
                                        .into_any()
                                    } else {
                                        let rows = list
                                            .iter()
                                            .enumerate()
                                            .map(|(i, c)| {
                                                let flow = chain_flow_label(c);
                                                view! {
                                                    <div class="pg-chains-row">
                                                        <span class="pg-chains-idx">{format!("{:02}", i + 1)}</span>
                                                        <span class="pg-chains-flow" inner_html=flow></span>
                                                    </div>
                                                }
                                            })
                                            .collect_view();
                                        view! { <div class="pg-chains-table">{rows}</div> }.into_any()
                                    }
                                }
                            })}
                        </Suspense>
                    </Panel>
                </div>

                // ── Right rail ───────────────────────────────────────────
                <aside class="pg-chains-rail">
                    <Panel num="03" eyebrow="LANE · SELECTED" cmd="in-deg vs out-deg">
                        {move || {
                            let s = step.get().min(3);
                            let stats = lane_stats.get();
                            let lane = stats[s].clone();
                            let dir = if graph_has_edges.get() {
                                lane_direction(lane.in_deg, lane.out_deg)
                            } else {
                                "—"
                            };
                            let dir_sub = if graph_has_edges.get() {
                                format!("in {} · out {} — grau real do grafo", lane.in_deg, lane.out_deg)
                            } else {
                                "grafo indisponível — sem direção derivável".to_string()
                            };
                            let wave = if lane.top.is_empty() {
                                view! {
                                    <p class="pg-chains-dir-sub">
                                        "sem nodes com grau mapeado nesta lane"
                                    </p>
                                }
                                .into_any()
                            } else {
                                lane.top
                                    .iter()
                                    .map(|(name, deg)| view! {
                                        <div class="pg-chains-wave-row">
                                            <span class="pg-chains-wave-name">{name.clone()}</span>
                                            <span class="pg-chains-wave-deg">{format!("deg {deg}")}</span>
                                        </div>
                                    })
                                    .collect_view()
                                    .into_any()
                            };
                            view! {
                                <div class="pg-chains-sel">
                                    <p class="pg-chains-sel-name">
                                        {format!("{} · {} · {} módulos", SHELL_NAMES[s], SHELL_RANGES[s], lane.members)}
                                    </p>
                                    <div class="pg-chains-dir">{dir}</div>
                                    <p class="pg-chains-dir-sub">{dir_sub}</p>
                                    <div class="pg-chains-wavefront">
                                        <div class="el-eyebrow">"TOP WAVEFRONT"</div>
                                        {wave}
                                    </div>
                                </div>
                            }
                        }}
                    </Panel>

                    <Panel num="04" eyebrow="LANES · CENSUS">
                        {move || {
                            let stats = lane_stats.get();
                            let active = step.get().min(3);
                            (0..4usize)
                                .map(|s| {
                                    let cls = if s == active {
                                        "pg-chains-lane-row active"
                                    } else {
                                        "pg-chains-lane-row"
                                    };
                                    let swatch = format!(
                                        "background:{};opacity:{}",
                                        SHELL_FILL[s], SHELL_OPACITY[s]
                                    );
                                    view! {
                                        <button class=cls on:click=move |_| { playing.set(false); step.set(s); }>
                                            <span class="pg-chains-lane-swatch" style=swatch></span>
                                            <span class="pg-chains-lane-name">
                                                {format!("{} · {}", SHELL_NAMES[s], SHELL_RANGES[s])}
                                            </span>
                                            <span class="pg-chains-lane-count">
                                                {stats[s].members.to_string()}
                                            </span>
                                        </button>
                                    }
                                })
                                .collect_view()
                        }}
                        <p class="pg-chains-legend-note">
                            "raio do nó ∝ pub symbols · anel tracejado = fronteira de lane · clique para selecionar"
                        </p>
                    </Panel>

                    {move || focus.get().map(|f| {
                        let matched = modules
                            .get()
                            .map(|j| parse_modules(&j))
                            .unwrap_or_default()
                            .into_iter()
                            .find(|m| focus_matches(&m.file_path, &f));
                        let body = match matched {
                            Some(m) => view! {
                                <p class="pg-chains-focus-path">{m.file_path.clone()}</p>
                                <p class="pg-chains-dir-sub">
                                    {format!(
                                        "lane {} · score {:.2} · {} pub symbols",
                                        SHELL_NAMES[shell_of(m.integration_score)],
                                        m.integration_score,
                                        m.total_pub_symbols
                                    )}
                                </p>
                            }
                            .into_any(),
                            None => view! {
                                <p class="pg-chains-dir-sub">
                                    "símbolo não encontrado no wiring DB — sem highlight."
                                </p>
                            }
                            .into_any(),
                        };
                        view! {
                            <Panel num="05" eyebrow="FOCUS · FROM ATLAS">
                                <code class="pg-chains-focus-sym">{f.clone()}</code>
                                {body}
                                <a class="el-btn el-btn-sm" href="/workspace">"voltar ao Atlas"</a>
                            </Panel>
                        }
                    })}
                </aside>
            </div>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_modules() -> Vec<ModuleNode> {
        vec![
            ModuleNode {
                file_path: "/abs/crates/a/src/core.rs".to_string(),
                integration_score: 0.95,
                total_pub_symbols: 10,
            },
            ModuleNode {
                file_path: "crates/b/src/mid.rs".to_string(),
                integration_score: 0.5,
                total_pub_symbols: 3,
            },
        ]
    }

    fn sample_graph() -> serde_json::Value {
        serde_json::json!({
            "nodes": [
                {"id": "crates/a/src/core.rs"},
                {"id": "crates/b/src/mid.rs"},
                {"id": "crates/zz/src/x.rs"}
            ],
            "edges": [
                {"from": "crates/a/src/core.rs", "to": "crates/b/src/mid.rs"},
                {"from": "crates/a/src/core.rs", "to": "crates/b/src/mid.rs"},
                {"from": "crates/zz/src/x.rs",   "to": "crates/a/src/core.rs"}
            ]
        })
    }

    #[test]
    fn shell_of_lane_boundaries() {
        assert_eq!(shell_of(0.95), 0);
        assert_eq!(shell_of(0.9), 0);
        assert_eq!(shell_of(0.7), 1);
        assert_eq!(shell_of(0.4), 2);
        assert_eq!(shell_of(0.39), 3);
    }

    #[test]
    fn lane_direction_all_branches() {
        assert_eq!(lane_direction(0, 0), "ISOLATED");
        assert_eq!(lane_direction(1, 5), "FAN-OUT");
        assert_eq!(lane_direction(5, 1), "FAN-IN");
        assert_eq!(lane_direction(3, 3), "BALANCED");
    }

    #[test]
    fn normalize_rel_strips_workspace_prefix() {
        assert_eq!(
            normalize_rel("/home/x/.claude/rust/crates/a/src/core.rs"),
            Some("crates/a/src/core.rs")
        );
        assert_eq!(
            normalize_rel("crates/a/src/core.rs"),
            Some("crates/a/src/core.rs")
        );
        assert_eq!(normalize_rel("scripts/tooling.py"), None);
    }

    #[test]
    fn focus_matches_label_substring_and_case() {
        assert!(focus_matches("crates/a/src/core.rs", "core"));
        assert!(focus_matches("crates/a/src/core.rs", "CORE"));
        assert!(focus_matches("crates/a/src/core.rs", "a/src/core.rs"));
        assert!(!focus_matches("crates/a/src/core.rs", "nope"));
        assert!(!focus_matches("crates/a/src/core.rs", ""));
    }

    #[test]
    fn derive_lane_stats_real_degrees_and_wavefront() {
        let stats = derive_lane_stats(&sample_modules(), &sample_graph());
        // Lane membership from wiring modules.
        assert_eq!(stats[0].members, 1);
        assert_eq!(stats[2].members, 1);
        assert_eq!(stats[1].members, 0);
        // core.rs (lane 0): 2 edges out, 1 in → FAN-OUT.
        assert_eq!(stats[0].out_deg, 2);
        assert_eq!(stats[0].in_deg, 1);
        assert_eq!(lane_direction(stats[0].in_deg, stats[0].out_deg), "FAN-OUT");
        // mid.rs (lane 2): 2 edges in, 0 out → FAN-IN.
        assert_eq!(stats[2].in_deg, 2);
        assert_eq!(stats[2].out_deg, 0);
        assert_eq!(lane_direction(stats[2].in_deg, stats[2].out_deg), "FAN-IN");
        // Top wavefront: core has total degree 3 (2 out + 1 in).
        assert_eq!(stats[0].top.first(), Some(&("core".to_string(), 3)));
        // Unmapped endpoints (crates/zz not in wiring modules) are excluded.
        assert!(stats[1].top.is_empty());
    }

    #[test]
    fn derive_lane_stats_tolerates_missing_graph() {
        let stats = derive_lane_stats(&sample_modules(), &serde_json::Value::Null);
        assert_eq!(stats[0].members, 1);
        assert_eq!(stats[0].in_deg, 0);
        assert_eq!(stats[0].out_deg, 0);
        assert!(stats[0].top.is_empty());
    }

    #[test]
    fn parse_modules_skips_malformed_entries() {
        let json = serde_json::json!([
            {"file_path": "crates/a/src/core.rs", "integration_score": 0.9, "total_pub_symbols": 4},
            {"integration_score": 1.0},
            "not-an-object"
        ]);
        let mods = parse_modules(&json);
        assert_eq!(mods.len(), 1);
        assert_eq!(mods[0].file_path, "crates/a/src/core.rs");
    }

    #[test]
    fn build_shell_svg_uses_tokens_dims_and_focus_halo() {
        let svg = build_shell_svg(&sample_modules(), 0, Some("core"));
        assert!(svg.starts_with("<svg"));
        assert!(svg.ends_with("</svg>"));
        // Token-driven colors (no hardcoded hex node fills).
        assert!(svg.contains("var(--el-accent)"));
        assert!(!svg.contains("#5eead4"));
        // Lane 2 (mid.rs) is beyond active lane 0 → dimmed opacity.
        assert!(svg.contains(&format!("fill-opacity:{DIMMED_OPACITY}")));
        // Focused node gets the warn halo + forced label.
        assert!(svg.contains("pg-chains-node-focus"));
        assert!(svg.contains("var(--el-warn)"));
        assert!(svg.contains(">core</text>"));
    }

    #[test]
    fn chain_flow_label_source_sink_and_fallback() {
        let chain = serde_json::json!({"source": "a", "sink": "b"});
        let html = chain_flow_label(&chain);
        assert!(html.contains("<code>a</code>"));
        assert!(html.contains("pg-chains-arrow"));
        let opaque = serde_json::json!({"hops": 3});
        assert!(chain_flow_label(&opaque).starts_with("<code>"));
    }
}
