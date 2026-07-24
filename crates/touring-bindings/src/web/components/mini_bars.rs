//! MiniBars — compact activity bars (SPEC 2026-06-12 §4.3).
//!
//! 30-ish vertical bars normalized to the max value, with an optional
//! highlight range (recent activity in accent, the rest in fg-5).
//! Used by the /dashboard right rail and the Pensieve panel.

use leptos::prelude::*;

/// Normalize values to `[0, 1]` against the max (0 → flat baseline).
pub fn normalize_bars(values: &[f64]) -> Vec<f64> {
    let max = values.iter().copied().fold(0.0_f64, f64::max);
    if max <= 0.0 {
        return values.iter().map(|_| 0.0).collect();
    }
    values.iter().map(|v| (v / max).clamp(0.0, 1.0)).collect()
}

/// Compact vertical bars (pure SVG).
#[component]
pub fn MiniBars(
    /// Reactive values, oldest → newest.
    #[prop(into)]
    values: Signal<Vec<f64>>,
    /// Bars at index ≥ `highlight_from` render accent (recent window).
    #[prop(optional)]
    highlight_from: Option<usize>,
    /// ViewBox width.
    #[prop(default = 240.0)]
    width: f64,
    /// ViewBox height.
    #[prop(default = 48.0)]
    height: f64,
) -> impl IntoView {
    view! {
        <svg
            class="el-minibars"
            viewBox=format!("0 0 {width} {height}")
            preserveAspectRatio="none"
            aria-hidden="true"
        >
            {move || {
                let vals = values.get();
                let norm = normalize_bars(&vals);
                let n = norm.len().max(1);
                let slot = width / n as f64;
                let bar_w = (slot * 0.62).max(1.0);
                norm.into_iter()
                    .enumerate()
                    .map(|(i, v)| {
                        let h = (v * (height - 2.0)).max(1.5);
                        let x = i as f64 * slot + (slot - bar_w) / 2.0;
                        let color = match highlight_from {
                            Some(from) if i >= from => "var(--el-accent)",
                            _ => "var(--el-fg-5)",
                        };
                        view! {
                            <rect
                                x=format!("{x:.1}")
                                y=format!("{:.1}", height - h)
                                width=format!("{bar_w:.1}")
                                height=format!("{h:.1}")
                                rx="1"
                                fill=color
                                opacity="0.9"
                            />
                        }
                    })
                    .collect_view()
            }}
        </svg>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_scales_to_unit_max() {
        let n = normalize_bars(&[1.0, 2.0, 4.0]);
        assert_eq!(n, vec![0.25, 0.5, 1.0]);
    }

    #[test]
    fn normalize_handles_all_zero() {
        assert_eq!(normalize_bars(&[0.0, 0.0]), vec![0.0, 0.0]);
    }

    #[test]
    fn normalize_empty_is_empty() {
        assert!(normalize_bars(&[]).is_empty());
    }
}
