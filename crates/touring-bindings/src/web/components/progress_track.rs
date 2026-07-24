//! ProgressTrack — componentized 2px progress bar (SPEC 2026-06-12 §4.3).
//!
//! Wraps the `.el-prog-track/.el-prog-fill` pair with an optional
//! mono value column, replacing the per-page hand-rolled markup.

use leptos::prelude::*;

/// Clamp a 0..1 value into a CSS width percent string.
pub fn fill_percent(value: f64) -> String {
    format!("{:.1}%", (value.clamp(0.0, 1.0)) * 100.0)
}

/// 2px progress bar with token-colored fill and optional value text.
#[component]
pub fn ProgressTrack(
    /// Progress in `[0, 1]`.
    #[prop(into)]
    value: Signal<f64>,
    /// Fill color token (e.g. `var(--el-warn)` for bottleneck rows).
    #[prop(default = "var(--el-accent)")]
    color: &'static str,
    /// Render the numeric value next to the bar.
    #[prop(default = false)]
    show_value: bool,
) -> impl IntoView {
    view! {
        <div class="el-prog-row" style="display:flex;align-items:center;gap:10px;">
            <div class="el-prog-track" style="flex:1;">
                <div
                    class="el-prog-fill"
                    style=move || format!("width:{};background:{}", fill_percent(value.get()), color)
                ></div>
            </div>
            {show_value.then(|| view! {
                <span class="el-mono" style=format!("color:{color}")>
                    {move || format!("{:.2}", value.get())}
                </span>
            })}
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fill_percent_clamps_and_formats() {
        assert_eq!(fill_percent(0.5), "50.0%");
        assert_eq!(fill_percent(-0.2), "0.0%");
        assert_eq!(fill_percent(1.7), "100.0%");
    }
}
