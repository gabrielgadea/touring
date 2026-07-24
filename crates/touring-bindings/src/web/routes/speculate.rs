//! /speculate — 6-layer speculative validation radar (SPEC 2026-06-12
//! §6.3). Elite W4.
//!
//! Renders `GET /api/speculate?file=<rel_path>` — the shadow-validate
//! pipeline result: a Bayesian-fused score plus six per-layer verdicts
//! (Syntax, SymbolResolution, Structural, ImportCheck, Complexity,
//! CfgImpact). The radar is a page-specific 6-axis inline SVG (the
//! shared [`RadarChart`](crate::web::components::RadarChart) is
//! hard-bound to the 5 RootCauseScores axes); failed layers get a
//! dashed warn halo on their vertex and warn-tinted diagnostic cards
//! listing the real messages.

use crate::web::components::{KpiCell, KpiStrip, PageHero, Panel, ProgressTrack};
use crate::web::services::{fetch_json, urlencode};
use leptos::prelude::*;
use leptos_meta::Title;
use leptos_router::hooks::use_query_map;

/// Fixed radar axis order — `(api_layer_name, radar_label)`. Must stay
/// in the engine's emission order so the polygon reads left-to-right
/// like the per-layer table.
pub const SPECULATE_AXES: [(&str, &str); 6] = [
    ("Syntax", "SYNTAX"),
    ("SymbolResolution", "SYMBOLS"),
    ("Structural", "STRUCTURAL"),
    ("ImportCheck", "IMPORTS"),
    ("Complexity", "COMPLEXITY"),
    ("CfgImpact", "CFG"),
];

/// Parse the `layers` array of a `/api/speculate` payload into
/// `(layer_name, score, passed, diagnostics_count)` rows. Malformed or
/// error payloads yield an empty vec.
pub fn parse_layers(v: &serde_json::Value) -> Vec<(String, f64, bool, usize)> {
    v.get("layers")
        .and_then(|l| l.as_array())
        .map(|arr| {
            arr.iter()
                .map(|l| {
                    let name = l
                        .get("layer")
                        .and_then(|s| s.as_str())
                        .unwrap_or("")
                        .to_string();
                    let score = l.get("score").and_then(|s| s.as_f64()).unwrap_or(0.0);
                    let passed = l.get("passed").and_then(|p| p.as_bool()).unwrap_or(false);
                    let diags = l
                        .get("diagnostics")
                        .and_then(|d| d.as_array())
                        .map(|d| d.len())
                        .unwrap_or(0);
                    (name, score, passed, diags)
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Count of layers with `passed == true`.
pub fn layers_passed(layers: &[(String, f64, bool, usize)]) -> usize {
    layers.iter().filter(|l| l.2).count()
}

/// Vertex (x, y) for fixed axis `i` (of 6) at normalized value `v` —
/// axis 0 points straight up (−π/2), subsequent axes step 2π/6.
pub fn radar_vertex6(i: usize, v: f64, cx: f64, cy: f64, r: f64) -> (f64, f64) {
    let angle = -std::f64::consts::FRAC_PI_2 + (i as f64) * (2.0 * std::f64::consts::PI / 6.0);
    let radius = r * v.clamp(0.0, 1.0);
    (cx + angle.cos() * radius, cy + angle.sin() * radius)
}

/// SVG polygon `points` attribute for the 6 fixed axes — each axis
/// takes the score of the matching parsed layer (0.0 when absent).
pub fn radar_points(layers: &[(String, f64, bool, usize)], cx: f64, cy: f64, r: f64) -> String {
    SPECULATE_AXES
        .iter()
        .enumerate()
        .map(|(i, (key, _))| {
            let score = layers
                .iter()
                .find(|l| l.0 == *key)
                .map(|l| l.1)
                .unwrap_or(0.0);
            let (x, y) = radar_vertex6(i, score, cx, cy, r);
            format!("{x:.1},{y:.1}")
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Layers with non-empty diagnostics → `(layer_name, real_messages)`.
/// String diagnostics pass through verbatim; structured ones are
/// serialized so nothing is hidden.
pub fn layer_diagnostics(v: &serde_json::Value) -> Vec<(String, Vec<String>)> {
    v.get("layers")
        .and_then(|l| l.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|l| {
                    let msgs: Vec<String> = l
                        .get("diagnostics")?
                        .as_array()?
                        .iter()
                        .map(|d| {
                            d.as_str()
                                .map(|s| s.to_string())
                                .unwrap_or_else(|| d.to_string())
                        })
                        .collect();
                    if msgs.is_empty() {
                        return None;
                    }
                    let name = l
                        .get("layer")
                        .and_then(|s| s.as_str())
                        .unwrap_or("")
                        .to_string();
                    Some((name, msgs))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Display label for an API layer name ("SymbolResolution" → "Symbols").
pub fn layer_label(key: &str) -> String {
    match key {
        "SymbolResolution" => "Symbols".to_string(),
        "ImportCheck" => "Imports".to_string(),
        "CfgImpact" => "Cfg".to_string(),
        other => other.to_string(),
    }
}

/// Last two path segments for the compact File KPI cell.
pub fn path_tail(p: &str) -> String {
    let segs: Vec<&str> = p.split('/').filter(|s| !s.is_empty()).collect();
    match segs.as_slice() {
        [.., dir, file] => format!("{dir}/{file}"),
        _ => p.to_string(),
    }
}

/// "%.2f" of a top-level numeric field ("score"/"bayesian_score"), or
/// "—" when the payload is absent or the field is missing.
pub fn fmt_score_field(v: Option<&serde_json::Value>, key: &str) -> String {
    v.and_then(|v| v.get(key))
        .and_then(|s| s.as_f64())
        .map(|s| format!("{s:.2}"))
        .unwrap_or_else(|| "—".to_string())
}

/// "N / 6" layers-passed KPI text, or "—" without a payload.
pub fn kpi_layers_text(v: Option<&serde_json::Value>) -> String {
    v.map(|v| format!("{} / 6", layers_passed(&parse_layers(v))))
        .unwrap_or_else(|| "—".to_string())
}

/// Total diagnostics count across all layers, or "—" without a payload.
pub fn kpi_diags_text(v: Option<&serde_json::Value>) -> String {
    v.map(|v| {
        parse_layers(v)
            .iter()
            .map(|l| l.3)
            .sum::<usize>()
            .to_string()
    })
    .unwrap_or_else(|| "—".to_string())
}

/// Compact `file_path` tail KPI text, or "—" without a payload.
pub fn kpi_file_text(v: Option<&serde_json::Value>) -> String {
    v.and_then(|v| v.get("file_path"))
        .and_then(|p| p.as_str())
        .map(path_tail)
        .unwrap_or_else(|| "—".to_string())
}

/// Inline 6-axis radar — grid rings, spokes, uppercase labels, the
/// accent score polygon and a dashed warn halo on every failed vertex.
/// Per-axis render tuple: spoke end (x, y), label pos (x, y), label,
/// anchor, failed flag and the score vertex (x, y) for the warn halo.
type RadarAxis = (
    f64,
    f64,
    f64,
    f64,
    &'static str,
    &'static str,
    bool,
    f64,
    f64,
);

fn radar_svg(layers: &[(String, f64, bool, usize)]) -> impl IntoView + use<> {
    let size = 240.0;
    let pad = 84.0;
    let cx = size / 2.0;
    let cy = size / 2.0;
    let r = size / 2.0 - 14.0;
    let view_min_x = -pad;
    let view_min_y = -20.0;
    let view_w = size + 2.0 * pad;
    let view_h = size + 40.0;

    let polygon = radar_points(layers, cx, cy, r);

    // Per-axis: spoke end, label pos+anchor, label text, failed flag
    // and the score vertex (for the warn halo).
    let axes: Vec<RadarAxis> = SPECULATE_AXES
        .iter()
        .enumerate()
        .map(|(i, (key, label))| {
            let angle =
                -std::f64::consts::FRAC_PI_2 + (i as f64) * (2.0 * std::f64::consts::PI / 6.0);
            let sx = cx + angle.cos() * r;
            let sy = cy + angle.sin() * r;
            let lx = cx + angle.cos() * (r + 16.0);
            let mut ly = cy + angle.sin() * (r + 16.0);
            if angle.sin() < -0.6 {
                ly -= 8.0;
            } else if angle.sin() > 0.6 {
                ly += 8.0;
            }
            let dx = lx - cx;
            let anchor = if dx.abs() < 12.0 {
                "middle"
            } else if dx > 0.0 {
                "start"
            } else {
                "end"
            };
            let found = layers.iter().find(|l| l.0 == *key);
            let score = found.map(|l| l.1).unwrap_or(0.0);
            let failed = found.map(|l| !l.2).unwrap_or(false);
            let (vx, vy) = radar_vertex6(i, score, cx, cy, r);
            (sx, sy, lx, ly, anchor, *label, failed, vx, vy)
        })
        .collect();

    view! {
        <svg class="pg-speculate-radar-svg"
             width=format!("{view_w}")
             height=format!("{view_h}")
             viewBox=format!("{view_min_x} {view_min_y} {view_w} {view_h}")
             preserveAspectRatio="xMidYMid meet"
             role="img"
             aria-label="Speculative validation radar, 6 layers">
            // grid rings
            {(1..=4).map(|i| {
                let radius = r * (i as f64 / 4.0);
                view! {
                    <circle cx={cx} cy={cy} r={radius} fill="none"
                            stroke="var(--el-line-strong)" />
                }
            }).collect_view()}
            // spokes + labels (failed layer label rendered warn)
            {axes.iter().map(|(sx, sy, lx, ly, anchor, label, failed, _, _)| {
                let fill = if *failed { "var(--el-warn)" } else { "var(--el-fg-4)" };
                view! {
                    <g>
                        <line x1={cx} y1={cy} x2={*sx} y2={*sy}
                              stroke="var(--el-line-strong)" />
                        <text x={*lx} y={*ly} font-size="10"
                              text-anchor={*anchor}
                              dominant-baseline="middle"
                              fill={fill}
                              style="text-transform:uppercase;letter-spacing:0.12em;">
                            {*label}
                        </text>
                    </g>
                }
            }).collect_view()}
            // score polygon (accent)
            <polygon points={polygon}
                     fill="var(--el-accent)" fill-opacity="0.12"
                     stroke="var(--el-accent)" stroke-width="1.25" />
            // failed vertices — dashed warn halos
            {axes.iter().filter(|a| a.6).map(|(_, _, _, _, _, _, _, vx, vy)| {
                view! {
                    <circle cx=format!("{vx:.1}") cy=format!("{vy:.1}") r="9"
                            fill="none" stroke="var(--el-warn)"
                            stroke-width="1.25" stroke-dasharray="3 3" />
                }
            }).collect_view()}
        </svg>
    }
}

/// Honest endpoint/transport error banner with the real message.
fn error_banner(msg: String) -> impl IntoView {
    view! {
        <div class="el-banner el-banner-warn pg-speculate-banner" role="alert">
            {msg}
        </div>
    }
}

/// Initial empty state — three real, clickable workspace files.
fn empty_state(input: RwSignal<String>, submitted: RwSignal<String>) -> impl IntoView {
    let suggestions = [
        "crates/touring-bindings/src/web/ctx.rs",
        "crates/touring-bindings/src/web/routes/quality.rs",
        "crates/touring-bindings/src/web/services/mod.rs",
    ];
    view! {
        <div class="el-empty pg-speculate-empty">
            <p>
                "Enter a workspace-relative path and press Validate — six "
                "speculative layers run against the file and fuse into one "
                "Bayesian shadow score. Try one of these:"
            </p>
            <div class="pg-speculate-suggest">
                {suggestions.into_iter().map(|s| view! {
                    <button
                        type="button"
                        class="el-btn el-btn-sm"
                        on:click=move |_| {
                            input.set(s.to_string());
                            submitted.set(s.to_string());
                        }
                    >
                        {s}
                    </button>
                }).collect_view()}
            </div>
        </div>
    }
}

/// One fetch outcome → the matching view branch: initial empty state,
/// transport-error banner, honest API-error banner, or the result grid.
fn outcome_view(
    outcome: Option<Result<serde_json::Value, String>>,
    input: RwSignal<String>,
    submitted: RwSignal<String>,
) -> AnyView {
    match outcome {
        None => empty_state(input, submitted).into_any(),
        Some(Err(e)) => error_banner(format!("speculate fetch failed: {e}")).into_any(),
        Some(Ok(v)) => match v.get("error").and_then(|x| x.as_str()) {
            Some(err) => error_banner(err.to_string()).into_any(),
            None => render_result(&v),
        },
    }
}

/// Successful payload → radar + per-layer table + diagnostic cards.
fn render_result(v: &serde_json::Value) -> AnyView {
    let layers = parse_layers(v);
    let diags = layer_diagnostics(v);
    let radar = radar_svg(&layers);

    let rows = layers
        .iter()
        .map(|(name, score, passed, diag_count)| {
            let label = layer_label(name);
            let score_v = *score;
            let passed_v = *passed;
            let warn = !passed_v || score_v < 1.0;
            let color = if warn {
                "var(--el-warn)"
            } else {
                "var(--el-accent)"
            };
            view! {
                <tr>
                    <td class="pg-speculate-layer-name">{label}</td>
                    <td class="pg-speculate-prog-col">
                        <ProgressTrack
                            value=Signal::derive(move || score_v)
                            color=color
                            show_value=true
                        />
                    </td>
                    <td>
                        {if passed_v {
                            view! { <span class="el-tag el-tag-pos">"pass"</span> }.into_any()
                        } else {
                            view! { <span class="el-tag el-tag-warn">"fail"</span> }.into_any()
                        }}
                    </td>
                    <td class="el-mono pg-speculate-diag-count">{format!("{diag_count}")}</td>
                </tr>
            }
        })
        .collect_view();

    let cards = diags
        .into_iter()
        .map(|(name, msgs)| {
            let label = layer_label(&name);
            let count = msgs.len();
            view! {
                <div class="el-card pg-speculate-diag-card">
                    <div class="el-eyebrow" style="color:var(--el-warn);">
                        {format!("{label} · {count} diagnostics")}
                    </div>
                    <ul class="pg-speculate-diag-list">
                        {msgs.into_iter().map(|m| view! {
                            <li class="el-mono">{m}</li>
                        }).collect_view()}
                    </ul>
                </div>
            }
        })
        .collect_view();

    view! {
        <section class="pg-speculate-grid">
            <Panel num="01" eyebrow="Radar · 6 layers" cmd="touring shadow validate">
                <div class="pg-speculate-radar">{radar}</div>
            </Panel>
            <div class="pg-speculate-right">
                <Panel num="02" eyebrow="Per-layer" cmd="GET /api/speculate">
                    <table class="el-table">
                        <thead>
                            <tr>
                                <th>"Layer"</th>
                                <th>"Score"</th>
                                <th>"Status"</th>
                                <th>"Diagnostics"</th>
                            </tr>
                        </thead>
                        <tbody>{rows}</tbody>
                    </table>
                </Panel>
                {cards}
            </div>
        </section>
    }
    .into_any()
}

/// `/speculate` — 6-layer shadow-validate radar (W4).
#[component]
pub fn SpeculatePage() -> impl IntoView {
    let input = RwSignal::new(String::new());
    let submitted = RwSignal::new(String::new());
    let submit = move || submitted.set(input.get_untracked().trim().to_string());

    // ?file= query param auto-submits (deep links from other pages).
    let qm = use_query_map();
    Effect::new(move |_| {
        let f = qm.get().get("file").unwrap_or_default().trim().to_string();
        if !f.is_empty() {
            input.set(f.clone());
            submitted.set(f);
        }
    });

    let res = LocalResource::new(move || {
        let path = submitted.get();
        async move {
            let p = path.trim().to_string();
            if p.is_empty() {
                return None;
            }
            Some(fetch_json(&format!("/api/speculate?file={}", urlencode(&p))).await)
        }
    });

    // Parsed OK payload (None while empty / fetch error / API error).
    let parsed = move || {
        res.get()
            .flatten()
            .and_then(|r| r.ok())
            .filter(|v| v.get("error").is_none())
    };

    let hero_stat = Signal::derive(move || fmt_score_field(parsed().as_ref(), "bayesian_score"));
    let kpi_score = Signal::derive(move || fmt_score_field(parsed().as_ref(), "score"));
    let kpi_layers = Signal::derive(move || kpi_layers_text(parsed().as_ref()));
    let kpi_diags = Signal::derive(move || kpi_diags_text(parsed().as_ref()));
    let kpi_file = Signal::derive(move || kpi_file_text(parsed().as_ref()));

    view! {
        <div class="page pg-speculate">
            <Title text="Touring — Speculate"/>
            <div class="el-main-editorial">
                <PageHero
                    eyebrow="SENTRUX · SPECULATIVE"
                    title="Speculate"
                    title_em="shadow"
                    sub="Six validation layers — syntax, symbol resolution, structure, imports, complexity and cfg impact — run speculatively against a workspace file before any edit lands, Bayesian-fused into one shadow score."
                    stat=hero_stat
                    stat_label="BAYESIAN"
                >
                    ""
                </PageHero>

                // ── path input — Enter or the button validates ───────────
                <div class="el-card pg-speculate-box">
                    <input
                        type="text"
                        class="pg-speculate-input"
                        placeholder="crates/…/file.rs"
                        prop:value=move || input.get()
                        on:input=move |ev| input.set(event_target_value(&ev))
                        on:keydown=move |ev| {
                            if ev.key() == "Enter" {
                                submit();
                            }
                        }
                    />
                    <button
                        type="button"
                        class="el-btn el-btn-primary"
                        on:click=move |_| submit()
                    >
                        "Validate"
                    </button>
                </div>

                <KpiStrip>
                    <KpiCell label="Score" value=kpi_score sub="fused [0, 1]"/>
                    <KpiCell label="Layers passed" value=kpi_layers sub="of six validators"/>
                    <KpiCell label="Diagnostics" value=kpi_diags sub="across all layers"/>
                    <KpiCell label="File" value=kpi_file sub="workspace-relative"/>
                </KpiStrip>

                <Suspense fallback=|| view! {
                    <div class="el-skeleton" aria-label="validating"></div>
                }>
                    {move || res.get().map(|outcome| outcome_view(outcome, input, submitted))}
                </Suspense>
            </div>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Real shape captured 2026-06-12 from
    /// `GET /api/speculate?file=crates/touring-bindings/src/web/ctx.rs`.
    fn real_payload() -> serde_json::Value {
        serde_json::json!({
            "bayesian_score": 1.0,
            "file_path": "crates/touring-bindings/src/web/ctx.rs",
            "score": 1.0,
            "valid": true,
            "layers": [
                {"diagnostics": [], "layer": "Syntax", "passed": true, "score": 1.0},
                {"diagnostics": [], "layer": "SymbolResolution", "passed": true, "score": 1.0},
                {"diagnostics": [], "layer": "Structural", "passed": true, "score": 1.0},
                {"diagnostics": [], "layer": "ImportCheck", "passed": true, "score": 1.0},
                {"diagnostics": [], "layer": "Complexity", "passed": true, "score": 1.0},
                {"diagnostics": [], "layer": "CfgImpact", "passed": true, "score": 1.0}
            ]
        })
    }

    #[test]
    fn parse_layers_real_shape_yields_six() {
        let layers = parse_layers(&real_payload());
        assert_eq!(layers.len(), 6);
        let names: Vec<&str> = layers.iter().map(|l| l.0.as_str()).collect();
        assert_eq!(
            names,
            [
                "Syntax",
                "SymbolResolution",
                "Structural",
                "ImportCheck",
                "Complexity",
                "CfgImpact"
            ]
        );
        assert!(
            layers
                .iter()
                .all(|l| (l.1 - 1.0).abs() < f64::EPSILON && l.2 && l.3 == 0)
        );
    }

    #[test]
    fn parse_layers_empty_on_error_payload() {
        // Real error shape: {"error": "file not found: nope/missing.rs"}
        let err = serde_json::json!({"error": "file not found: nope/missing.rs"});
        assert!(parse_layers(&err).is_empty());
        assert!(layer_diagnostics(&err).is_empty());
    }

    #[test]
    fn layers_passed_counts_failures() {
        let mut layers = parse_layers(&real_payload());
        assert_eq!(layers_passed(&layers), 6);
        layers[4].2 = false; // Complexity fails
        assert_eq!(layers_passed(&layers), 5);
    }

    #[test]
    fn radar_points_six_vertices_axis0_points_up() {
        let layers = parse_layers(&real_payload());
        let pts = radar_points(&layers, 120.0, 120.0, 106.0);
        let verts: Vec<&str> = pts.split(' ').collect();
        assert_eq!(verts.len(), 6);
        // Axis 0 (Syntax) at −90°: x == cx, y above center.
        let first: Vec<f64> = verts[0].split(',').filter_map(|c| c.parse().ok()).collect();
        assert_eq!(first.len(), 2);
        assert!((first[0] - 120.0).abs() < 0.1);
        assert!(first[1] < 120.0);
    }

    #[test]
    fn radar_points_missing_layers_collapse_to_center() {
        let pts = radar_points(&[], 120.0, 120.0, 106.0);
        assert_eq!(pts.split(' ').count(), 6);
        assert!(pts.split(' ').all(|p| p == "120.0,120.0"));
    }

    #[test]
    fn layer_diagnostics_extracts_real_messages() {
        let mut v = real_payload();
        v["layers"][1]["diagnostics"] = serde_json::json!([
            "unresolved symbol `Foo` at line 12",
            "unresolved symbol `Bar` at line 40"
        ]);
        v["layers"][1]["passed"] = serde_json::json!(false);
        let diags = layer_diagnostics(&v);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].0, "SymbolResolution");
        assert_eq!(diags[0].1.len(), 2);
        assert!(diags[0].1[0].contains("unresolved symbol `Foo`"));
    }

    #[test]
    fn layer_label_maps_fixed_axes() {
        assert_eq!(layer_label("SymbolResolution"), "Symbols");
        assert_eq!(layer_label("ImportCheck"), "Imports");
        assert_eq!(layer_label("CfgImpact"), "Cfg");
        assert_eq!(layer_label("Syntax"), "Syntax");
    }

    #[test]
    fn kpi_texts_from_real_payload_and_absent() {
        let v = real_payload();
        assert_eq!(fmt_score_field(Some(&v), "bayesian_score"), "1.00");
        assert_eq!(fmt_score_field(Some(&v), "score"), "1.00");
        assert_eq!(fmt_score_field(None, "score"), "—");
        assert_eq!(kpi_layers_text(Some(&v)), "6 / 6");
        assert_eq!(kpi_layers_text(None), "—");
        assert_eq!(kpi_diags_text(Some(&v)), "0");
        assert_eq!(kpi_diags_text(None), "—");
        assert_eq!(kpi_file_text(Some(&v)), "web/ctx.rs");
        assert_eq!(kpi_file_text(None), "—");
    }

    #[test]
    fn path_tail_keeps_last_two_segments() {
        assert_eq!(
            path_tail("crates/touring-bindings/src/web/ctx.rs"),
            "web/ctx.rs"
        );
        assert_eq!(path_tail("ctx.rs"), "ctx.rs");
        assert_eq!(path_tail(""), "");
    }
}
