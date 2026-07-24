//! 5-axis radar chart — modularity / acyclicity / depth / equality / redundancy.
//!
//! Elite W1 (SPEC 2026-06-12 §4.3): token colors (accent solid for the
//! current polygon), optional `overlay` polygon (violet dashed — used
//! by /quality/diff "prev" and /speculate "cur"), and an optional
//! `bottleneck` axis highlighted with a dashed warn halo on its vertex.

use crate::web::models::quality::RootCauseScores;
use leptos::prelude::*;

/// Axis display order — must match [`RootCauseScores`] field order
/// used by `polygon_points`.
pub const RADAR_AXES: [&str; 5] = [
    "modularity",
    "acyclicity",
    "depth",
    "equality",
    "redundancy",
];

/// Vertex (x, y) for axis `i` at normalized value `v`.
pub fn radar_vertex(i: usize, v: f64, cx: f64, cy: f64, r: f64) -> (f64, f64) {
    let angle = -std::f64::consts::FRAC_PI_2 + (i as f64) * (2.0 * std::f64::consts::PI / 5.0);
    let radius = r * v.clamp(0.0, 1.0);
    (cx + angle.cos() * radius, cy + angle.sin() * radius)
}

/// SVG polygon `points` attribute for a score set.
pub fn polygon_points(s: &RootCauseScores, cx: f64, cy: f64, r: f64) -> String {
    let vals = [
        s.modularity,
        s.acyclicity,
        s.depth,
        s.equality,
        s.redundancy,
    ];
    vals.iter()
        .enumerate()
        .map(|(i, v)| {
            let (x, y) = radar_vertex(i, *v, cx, cy, r);
            format!("{x:.1},{y:.1}")
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Index of an axis name in [`RADAR_AXES`], if valid.
pub fn axis_index(name: &str) -> Option<usize> {
    RADAR_AXES.iter().position(|a| *a == name)
}

/// 5-axis SVG radar chart for Sentrux root-cause scores.
#[component]
pub fn RadarChart(
    /// Reactive [`RootCauseScores`] (one normalized score per axis).
    #[prop(into)]
    scores: Signal<RootCauseScores>,
    /// Optional comparison polygon (violet dashed — "prev"/"cur" overlay).
    #[prop(optional, into)]
    overlay: Option<Signal<RootCauseScores>>,
    /// Optional bottleneck axis name — its vertex gets a warn halo.
    #[prop(optional, into)]
    bottleneck: Option<Signal<String>>,
    /// Width/height of the SVG square in pixels.
    #[prop(default = 220.0)]
    size: f64,
) -> impl IntoView {
    // Generous horizontal margin so left/right labels never clip even
    // for the longest axis name ("REDUNDANCY"/"ACYCLICITY"). The chart
    // core lives in a `size`-square box; the viewBox surrounds it with
    // `pad` pixels of bleed on every side.
    let pad = 90.0;
    let view_min_x = -pad;
    let view_min_y = -16.0;
    let view_w = size + 2.0 * pad;
    let view_h = size + 32.0;

    let cx = size / 2.0;
    let cy = size / 2.0;
    let r = size / 2.0 - 14.0;

    let polygon = Memo::new(move |_| polygon_points(&scores.get(), cx, cy, r));
    let overlay_polygon = Memo::new(move |_| overlay.map(|o| polygon_points(&o.get(), cx, cy, r)));

    // Bottleneck halo: vertex position of the weakest axis.
    let halo = Memo::new(move |_| {
        let name_sig = bottleneck?;
        let name = name_sig.get();
        let idx = axis_index(&name)?;
        let s = scores.get();
        let vals = [
            s.modularity,
            s.acyclicity,
            s.depth,
            s.equality,
            s.redundancy,
        ];
        let (x, y) = radar_vertex(idx, vals[idx], cx, cy, r);
        Some((x, y, idx))
    });

    // Each axis: spoke endpoint (x, y), label position (lx, ly), and a
    // text-anchor chosen by side (left/middle/right) so text never
    // clips at the viewBox boundary.
    let axes: Vec<(f64, f64, f64, f64, &'static str, &'static str, usize)> = (0..5)
        .map(|i| {
            let angle =
                -std::f64::consts::FRAC_PI_2 + (i as f64) * (2.0 * std::f64::consts::PI / 5.0);
            let x = cx + angle.cos() * r;
            let y = cy + angle.sin() * r;
            let label_x = cx + angle.cos() * (r + 16.0);
            let label_y = cy + angle.sin() * (r + 16.0);
            let dx = label_x - cx;
            let anchor = if dx.abs() < 12.0 {
                "middle"
            } else if dx > 0.0 {
                "start"
            } else {
                "end"
            };
            // Slight vertical nudge so axis-aligned labels (top/bottom) don't
            // overlap the spoke endpoint marker.
            let dy_nudge = if angle.sin() < -0.6 {
                -8.0
            } else if angle.sin() > 0.6 {
                8.0
            } else {
                0.0
            };
            (x, y, label_x, label_y + dy_nudge, RADAR_AXES[i], anchor, i)
        })
        .collect();

    view! {
        <svg class="radar-chart"
             width=move || format!("{}", view_w)
             height=move || format!("{}", view_h)
             viewBox=move || format!("{} {} {} {}", view_min_x, view_min_y, view_w, view_h)
             preserveAspectRatio="xMidYMid meet"
             role="img"
             aria-label="Sentrux root-cause radar">
            // grid rings
            {(1..=4).map(|i| {
                let radius = r * (i as f64 / 4.0);
                view! {
                    <circle cx={cx} cy={cy} r={radius} fill="none"
                            stroke="var(--el-line-strong)" />
                }
            }).collect_view()}
            // axes + labels (bottleneck label rendered warn)
            {axes.iter().map(|(x,y,lx,ly,name,anchor,idx)| {
                let axis_idx = *idx;
                let is_bot = move || {
                    halo.get().map(|(_, _, i)| i == axis_idx).unwrap_or(false)
                };
                view! {
                    <g>
                        <line x1={cx} y1={cy} x2={*x} y2={*y}
                              stroke="var(--el-line-strong)" />
                        <text x={*lx} y={*ly} font-size="10"
                              text-anchor={*anchor}
                              dominant-baseline="middle"
                              fill=move || if is_bot() { "var(--el-warn)" } else { "var(--el-fg-4)" }
                              style="text-transform:uppercase;letter-spacing:0.12em;">
                            {*name}
                        </text>
                    </g>
                }
            }).collect_view()}
            // overlay polygon (prev — violet dashed)
            {move || overlay_polygon.get().map(|pts| view! {
                <polygon points=pts
                         fill="var(--el-violet)" fill-opacity="0.06"
                         stroke="var(--el-violet)" stroke-width="1.25"
                         stroke-dasharray="5 4" />
            })}
            // value polygon (current — accent)
            <polygon points=move || polygon.get()
                     fill="var(--el-accent)" fill-opacity="0.12"
                     stroke="var(--el-accent)" stroke-width="1.25" />
            // bottleneck halo (dashed warn ring on the weakest vertex)
            {move || halo.get().map(|(x, y, _)| view! {
                <circle cx=format!("{x:.1}") cy=format!("{y:.1}") r="9"
                        fill="none" stroke="var(--el-warn)"
                        stroke-width="1.25" stroke-dasharray="3 3" />
            })}
        </svg>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scores(v: f64) -> RootCauseScores {
        RootCauseScores {
            modularity: v,
            acyclicity: v,
            depth: v,
            equality: v,
            redundancy: v,
        }
    }

    #[test]
    fn polygon_points_yields_five_vertices() {
        let pts = polygon_points(&scores(1.0), 110.0, 110.0, 96.0);
        assert_eq!(pts.split(' ').count(), 5);
    }

    #[test]
    fn radar_vertex_top_axis_points_up() {
        // Axis 0 (modularity) is at -90° — full score lands above center.
        let (x, y) = radar_vertex(0, 1.0, 110.0, 110.0, 96.0);
        assert!((x - 110.0).abs() < 0.001);
        assert!(y < 110.0);
    }

    #[test]
    fn radar_vertex_clamps_out_of_range() {
        let (_, y_hi) = radar_vertex(0, 7.0, 110.0, 110.0, 96.0);
        let (_, y_one) = radar_vertex(0, 1.0, 110.0, 110.0, 96.0);
        assert!((y_hi - y_one).abs() < 0.001, "values clamp to [0,1]");
    }

    #[test]
    fn axis_index_maps_known_axes() {
        assert_eq!(axis_index("modularity"), Some(0));
        assert_eq!(axis_index("redundancy"), Some(4));
        assert_eq!(axis_index("nope"), None);
    }
}
