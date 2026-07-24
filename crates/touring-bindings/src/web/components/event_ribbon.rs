//! EventRibbon — lane-based event timeline (SPEC 2026-06-12 §4.3).
//!
//! Horizontal lanes (hooks, edits, bash, …) with rounded event marks
//! positioned by normalized time `x ∈ [0,1]`, plus an optional
//! playhead. Used by /dashboard (activity), /sessions (hook ribbon),
//! /hooks (60s ribbon) and the W6 tri-pane inspector.

use leptos::prelude::*;

/// One event mark on the ribbon.
#[derive(Debug, Clone, PartialEq)]
pub struct RibbonEvent {
    /// Normalized horizontal position `[0, 1]` (0 = oldest).
    pub x: f64,
    /// Lane index (row).
    pub lane: usize,
    /// Mark width in px (≥2 for visibility).
    pub w: f64,
    /// CSS color token (e.g. `var(--el-accent)`).
    pub color: &'static str,
}

/// Cap events to `max` (most recent last — keeps the tail) so giant
/// feeds never melt the SVG (SPEC §10 perf: ribbon cap 200).
pub fn cap_events(events: Vec<RibbonEvent>, max: usize) -> Vec<RibbonEvent> {
    if events.len() <= max {
        events
    } else {
        events[events.len() - max..].to_vec()
    }
}

/// Lane-based event ribbon (pure SVG).
#[component]
pub fn EventRibbon(
    /// Lane labels, top to bottom.
    lanes: Vec<&'static str>,
    /// Reactive event list (positions normalized to `[0, 1]`).
    #[prop(into)]
    events: Signal<Vec<RibbonEvent>>,
    /// Optional playhead position `[0, 1]`.
    #[prop(optional, into)]
    playhead: Option<Signal<f64>>,
    /// ViewBox width.
    #[prop(default = 720.0)]
    width: f64,
    /// ViewBox height.
    #[prop(default = 120.0)]
    height: f64,
) -> impl IntoView {
    let lane_count = lanes.len().max(1);
    let label_w = 92.0;
    let lane_h = height / lane_count as f64;
    let track_w = width - label_w;

    let lane_rows: Vec<(usize, &'static str, f64)> = lanes
        .iter()
        .enumerate()
        .map(|(i, l)| (i, *l, i as f64 * lane_h + lane_h / 2.0))
        .collect();

    view! {
        <svg
            class="el-ribbon"
            viewBox=format!("0 0 {width} {height}")
            preserveAspectRatio="none"
            aria-hidden="true"
        >
            {lane_rows
                .iter()
                .map(|(_, label, cy)| view! {
                    <line
                        x1=format!("{label_w}")
                        y1=format!("{cy:.1}")
                        x2=format!("{width}")
                        y2=format!("{cy:.1}")
                        stroke="var(--el-line)"
                        stroke-width="1"
                    />
                    <text
                        x="0"
                        y=format!("{cy:.1}")
                        font-size="9"
                        dominant-baseline="middle"
                        fill="var(--el-fg-5)"
                        style="text-transform:uppercase;letter-spacing:0.14em;"
                    >
                        {*label}
                    </text>
                })
                .collect_view()}
            {move || {
                events
                    .get()
                    .into_iter()
                    .filter(|e| e.lane < lane_count)
                    .map(|e| {
                        let cx = label_w + e.x.clamp(0.0, 1.0) * track_w;
                        let cy = e.lane as f64 * lane_h + lane_h / 2.0;
                        let w = e.w.max(2.0);
                        view! {
                            <rect
                                x=format!("{:.1}", cx - w / 2.0)
                                y=format!("{:.1}", cy - 3.5)
                                width=format!("{w:.1}")
                                height="7"
                                rx="2"
                                fill=e.color
                                opacity="0.85"
                            />
                        }
                    })
                    .collect_view()
            }}
            {playhead.map(|p| view! {
                <line
                    x1=move || format!("{:.1}", label_w + p.get().clamp(0.0, 1.0) * track_w)
                    y1="0"
                    x2=move || format!("{:.1}", label_w + p.get().clamp(0.0, 1.0) * track_w)
                    y2=format!("{height}")
                    stroke="var(--el-accent)"
                    stroke-width="1"
                    stroke-dasharray="2 3"
                />
            })}
        </svg>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(x: f64) -> RibbonEvent {
        RibbonEvent {
            x,
            lane: 0,
            w: 4.0,
            color: "var(--el-accent)",
        }
    }

    #[test]
    fn cap_events_keeps_tail() {
        let evs: Vec<_> = (0..10).map(|i| ev(i as f64 / 10.0)).collect();
        let capped = cap_events(evs, 3);
        assert_eq!(capped.len(), 3);
        assert!((capped[0].x - 0.7).abs() < 1e-9, "keeps most recent tail");
    }

    #[test]
    fn cap_events_noop_under_limit() {
        let evs: Vec<_> = (0..5).map(|i| ev(i as f64)).collect();
        assert_eq!(cap_events(evs.clone(), 200).len(), 5);
    }
}
