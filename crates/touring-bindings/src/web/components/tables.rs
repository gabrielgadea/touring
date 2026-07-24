//! Reusable UI primitives: data tables, score bars, status dots.

use leptos::prelude::*;

/// Escapes HTML special characters — call this on every DYNAMIC value you
/// interpolate inside a [`CellRenderer`] (symbol names with generics like
/// `Vec<T>` would otherwise break the cell markup, and untrusted text could
/// inject elements).
pub fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Cell renderer — returns an HTML string injected via `inner_html`. The
/// renderer OWNS escaping of interpolated data: wrap dynamic values in
/// [`escape_html`]. (The old implementation double-escaped the whole string,
/// so cell markup rendered as literal `&lt;code&gt;` text — found in the
/// 2026-06-11 cross-audit the moment the tables first showed real data.)
pub type CellRenderer = Box<dyn Fn(usize, serde_json::Value) -> String>;

/// Row-based data table with alternating striped rows.
#[component]
pub fn DataTable(
    /// Column header labels.
    headers: Vec<String>,
    /// JSON array of row data objects.
    rows: serde_json::Value,
    /// Callback to render each cell as HTML string.
    render_cell: CellRenderer,
) -> impl IntoView {
    let n_cols = headers.len();
    view! {
        <div class="data-table-wrapper">
            <table class="data-table">
                <thead>
                    <tr>
                        {headers.iter().map(|h| view! { <th>{h.clone()}</th> }).collect::<Vec<_>>().into_view()}
                    </tr>
                </thead>
                <tbody>
                    {if let Some(arr) = rows.as_array() {
                        arr.iter().enumerate().map(|(idx, row)| {
                            let cells: Vec<_> = (0..n_cols).map(|col| {
                                let html = (render_cell)(col, row.clone());
                                view! { <td inner_html=html></td> }
                            }).collect();
                            view! {
                                <tr class=if idx % 2 == 0 { "row even" } else { "row odd" }>
                                    {cells.into_view()}
                                </tr>
                            }
                        }).collect::<Vec<_>>().into_view()
                    } else {
                        vec![].into_view()
                    }}
                </tbody>
            </table>
        </div>
    }
}

/// Horizontal score bar — fills left-to-right with colour encoding.
#[component]
pub fn ScoreBar(
    /// Score value between 0.0 and 1.0; colour thresholds at 0.4 and 0.7.
    score: f64,
) -> impl IntoView {
    let pct = (score.clamp(0.0, 1.0) * 100.0) as u32;
    let color = if score >= 0.7 {
        "#00e676"
    } else if score >= 0.4 {
        "#ffaa00"
    } else {
        "#ff6666"
    };

    view! {
        <div class="score-bar">
            <div class="score-bar-fill" style=format!("width:{}%; background:{}", pct, color)></div>
            <span class="score-bar-label">{format!("{:.0}%", pct)}</span>
        </div>
    }
}

/// Status dot with colour — renders as a pulsing coloured glyph.
#[component]
pub fn StatusDot(
    /// Component status string ("ok", "warn", "warning", "error", or other).
    status: String,
) -> impl IntoView {
    let cls = match status.as_str() {
        "ok" | "healthy" => "ok",
        "warn" | "warning" | "degraded" => "warn",
        "error" | "unhealthy" => "error",
        _ => "unknown",
    };
    let (glyph, color) = match status.as_str() {
        "ok" | "healthy" => ("●", "#00e676"),
        "warn" | "warning" | "degraded" => ("◐", "#f5a623"),
        "error" | "unhealthy" => ("○", "#ff5c7a"),
        _ => ("○", "#4a5268"),
    };
    view! {
        <span class=format!("status-dot {}", cls) style=format!("color:{}", color)>{glyph}</span>
    }
}
