//! DepthRings — BFS blast-radius visual (SPEC 2026-06-12 §4.3, §6.2).
//!
//! Concentric ellipses (ry = rx · 0.78) around a nucleus symbol; nodes
//! sit on their depth ring. Depths below the active scrubber render as
//! "past" (accent), the active depth is the wavefront (violet glow),
//! deeper rings are "future" (muted). Orphan nodes render dashed.
//! Used by /wiring/impact (W4) and the dashboard mini variant.

use leptos::prelude::*;

/// One ring of the BFS expansion.
#[derive(Debug, Clone, PartialEq)]
pub struct DepthRing {
    /// BFS depth (1-based).
    pub depth: usize,
    /// Consumers at this depth.
    pub count: usize,
}

/// One node placed on a ring.
#[derive(Debug, Clone, PartialEq)]
pub struct RingNode {
    /// Short symbol label.
    pub label: String,
    /// BFS depth (1-based — matches a [`DepthRing`]).
    pub depth: usize,
    /// Position index within the ring (0..count).
    pub angle_idx: usize,
    /// Render dashed (orphan consumer).
    pub orphan: bool,
}

/// Node (x, y) on ring `depth` of `max_depth` rings inside `w`×`h`.
pub fn ring_position(
    depth: usize,
    max_depth: usize,
    angle_idx: usize,
    ring_count: usize,
    w: f64,
    h: f64,
) -> (f64, f64) {
    let cx = w / 2.0;
    let cy = h / 2.0;
    let max_rx = (w.min(h / 0.78)) / 2.0 - 30.0;
    let rx = max_rx * depth as f64 / max_depth.max(1) as f64;
    let ry = rx * 0.78;
    // Stagger ring start angles so nodes on adjacent rings don't align.
    let offset = depth as f64 * 0.5;
    let angle = offset + angle_idx as f64 * std::f64::consts::TAU / ring_count.max(1) as f64;
    (cx + angle.cos() * rx, cy + angle.sin() * ry)
}

/// Node state vs. the active scrubber depth.
pub fn node_state(node_depth: usize, active: usize) -> &'static str {
    if node_depth < active {
        "past"
    } else if node_depth == active {
        "wavefront"
    } else {
        "future"
    }
}

/// Color token for a node state.
pub fn state_color(state: &str) -> &'static str {
    match state {
        "past" => "var(--el-accent)",
        "wavefront" => "var(--el-violet)",
        _ => "var(--el-fg-5)",
    }
}

/// Concentric BFS depth rings (pure SVG).
#[component]
pub fn DepthRings(
    /// Nucleus symbol name (center).
    #[prop(into)]
    nucleus: Signal<String>,
    /// Ring sizes per depth.
    #[prop(into)]
    rings: Signal<Vec<DepthRing>>,
    /// Nodes to place (cap before passing — perf §10).
    #[prop(into)]
    nodes: Signal<Vec<RingNode>>,
    /// Active scrubber depth (wavefront).
    #[prop(into)]
    active_depth: Signal<usize>,
    /// ViewBox width.
    #[prop(default = 760.0)]
    width: f64,
    /// ViewBox height.
    #[prop(default = 480.0)]
    height: f64,
) -> impl IntoView {
    let cx = width / 2.0;
    let cy = height / 2.0;

    view! {
        <svg
            class="el-depthrings"
            viewBox=format!("0 0 {width} {height}")
            preserveAspectRatio="xMidYMid meet"
            aria-hidden="true"
        >
            {move || {
                let ring_list = rings.get();
                let max_depth = ring_list.iter().map(|r| r.depth).max().unwrap_or(1);
                let active = active_depth.get();
                let max_rx = (width.min(height / 0.78)) / 2.0 - 30.0;
                ring_list
                    .iter()
                    .map(|r| {
                        let rx = max_rx * r.depth as f64 / max_depth.max(1) as f64;
                        let ry = rx * 0.78;
                        let is_active = r.depth == active;
                        view! {
                            <ellipse
                                cx=format!("{cx}")
                                cy=format!("{cy}")
                                rx=format!("{rx:.1}")
                                ry=format!("{ry:.1}")
                                fill="none"
                                stroke=if is_active { "var(--el-violet)" } else { "var(--el-line-strong)" }
                                stroke-width=if is_active { "1.5" } else { "1" }
                                stroke-dasharray=if is_active { "" } else { "4 5" }
                            />
                            <text
                                x=format!("{:.1}", cx + rx + 6.0)
                                y=format!("{cy}")
                                font-size="9"
                                fill="var(--el-fg-5)"
                                dominant-baseline="middle"
                                style="font-family:var(--el-mono);"
                            >
                                {format!("d={} · {}", r.depth, r.count)}
                            </text>
                        }
                    })
                    .collect_view()
            }}
            {move || {
                let ring_list = rings.get();
                let max_depth = ring_list.iter().map(|r| r.depth).max().unwrap_or(1);
                let counts: std::collections::HashMap<usize, usize> =
                    ring_list.iter().map(|r| (r.depth, r.count)).collect();
                let active = active_depth.get();
                nodes
                    .get()
                    .into_iter()
                    .map(|n| {
                        let ring_count = counts.get(&n.depth).copied().unwrap_or(1);
                        let (x, y) = ring_position(n.depth, max_depth, n.angle_idx, ring_count, width, height);
                        let state = node_state(n.depth, active);
                        let color = state_color(state);
                        let r = if state == "wavefront" { 5.0 } else { 3.5 };
                        view! {
                            <g>
                                {(state == "wavefront").then(|| view! {
                                    <circle
                                        cx=format!("{x:.1}") cy=format!("{y:.1}") r="9"
                                        fill="var(--el-violet-soft)" stroke="none"
                                    />
                                })}
                                <circle
                                    cx=format!("{x:.1}")
                                    cy=format!("{y:.1}")
                                    r=format!("{r}")
                                    fill=color
                                    stroke=if n.orphan { "var(--el-warn)" } else { "none" }
                                    stroke-dasharray=if n.orphan { "2 2" } else { "" }
                                    stroke-width="1.25"
                                >
                                    <title>{n.label.clone()}</title>
                                </circle>
                            </g>
                        }
                    })
                    .collect_view()
            }}
            // Nucleus
            <circle
                cx=format!("{cx}") cy=format!("{cy}") r="17"
                fill="var(--el-surface-3)" stroke="var(--el-accent)" stroke-width="1.5"
            />
            <text
                x=format!("{cx}") y=format!("{:.1}", cy + height * 0.5 - 12.0)
                font-size="11" text-anchor="middle" fill="var(--el-fg-2)"
                style="font-family:var(--el-mono);"
            >
                {move || nucleus.get()}
            </text>
        </svg>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ring_position_scales_with_depth() {
        // Elliptical radius (y squashed by 0.78) must grow with depth —
        // the per-ring angle stagger makes raw x/y projections unordered.
        let radius = |depth: usize| -> f64 {
            let (x, y) = ring_position(depth, 3, 0, 1, 760.0, 480.0);
            let dx = x - 380.0;
            let dy = (y - 240.0) / 0.78;
            (dx * dx + dy * dy).sqrt()
        };
        assert!(
            radius(3) > radius(2) && radius(2) > radius(1),
            "deeper ring sits further out"
        );
    }

    #[test]
    fn node_state_partitions_by_active_depth() {
        assert_eq!(node_state(1, 2), "past");
        assert_eq!(node_state(2, 2), "wavefront");
        assert_eq!(node_state(3, 2), "future");
    }

    #[test]
    fn state_colors_use_tokens() {
        for s in ["past", "wavefront", "future"] {
            assert!(state_color(s).starts_with("var(--el-"));
        }
    }
}
