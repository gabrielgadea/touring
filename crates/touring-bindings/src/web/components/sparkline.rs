//! Sparkline component — pure SVG, dep-free.
//!
//! Wave 4 P10 polish (2026-05-10): gradient stroke (`accent → violet`),
//! soft area fill below the line, end-of-line dot marker indicating the
//! most recent value, and a stroke-dasharray draw-in animation honoured
//! by `@keyframes sparkline-draw` in `main.css`.

use leptos::prelude::*;

/// Sparkline component — pure SVG, dep-free.
#[component]
pub fn Sparkline(
    /// Sequence of values to plot (oldest → newest).
    #[prop(into)]
    values: Signal<Vec<f64>>,
    /// Visible width in pixels.
    #[prop(default = 320.0)]
    width: f64,
    /// Visible height in pixels.
    #[prop(default = 80.0)]
    height: f64,
) -> impl IntoView {
    // Returns (line_path, area_path, last_x, last_y).
    let geom = Memo::new(move |_| {
        let v = values.get();
        if v.len() < 2 {
            return (String::new(), String::new(), 0.0_f64, 0.0_f64);
        }
        let lo = v.iter().copied().fold(f64::INFINITY, f64::min);
        let hi = v.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let range = (hi - lo).max(1e-6);
        let dx = width / (v.len() as f64 - 1.0);
        let mut line = String::with_capacity(v.len() * 16);
        let mut last = (0.0_f64, 0.0_f64);
        for (i, val) in v.iter().enumerate() {
            let x = i as f64 * dx;
            let y = height - ((val - lo) / range) * height;
            line.push(if i == 0 { 'M' } else { 'L' });
            line.push(' ');
            line.push_str(&format!("{:.1} {:.1} ", x, y));
            last = (x, y);
        }
        // Closed area beneath the line for soft fill.
        let area = format!("{}L {:.1} {:.1} L 0 {:.1} Z", line, last.0, height, height);
        (line, area, last.0, last.1)
    });

    let path_d = move || geom.get().0;
    let area_d = move || geom.get().1;
    let last_x = move || geom.get().2;
    let last_y = move || geom.get().3;
    let visible = move || !geom.get().0.is_empty();

    view! {
        <svg class="sparkline"
             viewBox=move || format!("0 0 {} {}", width, height)
             width=move || format!("{}", width)
             height=move || format!("{}", height)
             preserveAspectRatio="none"
             aria-hidden="true">
            <defs>
                <linearGradient id="sparkline-stroke" x1="0%" y1="0%" x2="100%" y2="0%">
                    <stop offset="0%"   stop-color="#00e5c0" stop-opacity="0.6"/>
                    <stop offset="50%"  stop-color="#26c6da" stop-opacity="1"/>
                    <stop offset="100%" stop-color="#7c4dff" stop-opacity="1"/>
                </linearGradient>
                <linearGradient id="sparkline-fill" x1="0%" y1="0%" x2="0%" y2="100%">
                    <stop offset="0%"   stop-color="#00e5c0" stop-opacity="0.32"/>
                    <stop offset="100%" stop-color="#00e5c0" stop-opacity="0.0"/>
                </linearGradient>
            </defs>
            <path d=area_d fill="url(#sparkline-fill)" stroke="none"
                  class="sparkline-area" />
            <path d=path_d fill="none" stroke="url(#sparkline-stroke)"
                  stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"
                  class="sparkline-line" />
            // End-of-line dot marker (only when there's data).
            {move || if visible() {
                Some(view! {
                    <circle cx=last_x cy=last_y r="3.4"
                            fill="#00e5c0" stroke="#0b0e16" stroke-width="1.2"
                            class="sparkline-dot" />
                })
            } else { None }}
        </svg>
    }
}
