//! IsoPalace — isometric memory-palace blocks (SPEC 2026-06-12 §5.12).
//!
//! One isometric block per "wing" (key-namespace group), block height
//! proportional to entry count. Honest derivation: wings come from real
//! memory-key prefixes — the page labels the grouping as derived.

use leptos::prelude::*;

/// One palace wing (a key-prefix group).
#[derive(Debug, Clone, PartialEq)]
pub struct Wing {
    /// Display label (the key prefix, e.g. `outcome`, `gotcha`, `w1`).
    pub label: String,
    /// Entries under this prefix.
    pub count: usize,
}

/// Isometric block geometry for wing `i` of `n`, height fraction `hf`.
/// Returns (top, left, right) polygon point strings.
pub fn iso_block(i: usize, hf: f64, origin_x: f64, origin_y: f64) -> (String, String, String) {
    let hw = 46.0; // half diamond width
    let hh = 23.0; // half diamond height (iso 2:1)
    let col = i as f64;
    let bx = origin_x + col * (hw * 2.0 + 18.0);
    let by = origin_y;
    let lift = 24.0 + hf.clamp(0.0, 1.0) * 96.0;
    let top_y = by - lift;
    let top = format!(
        "{bx:.1},{ty:.1} {rx:.1},{tmy:.1} {bx:.1},{by2:.1} {lx:.1},{tmy:.1}",
        bx = bx,
        ty = top_y - hh,
        rx = bx + hw,
        tmy = top_y,
        by2 = top_y + hh,
        lx = bx - hw,
    );
    let left = format!(
        "{lx:.1},{tmy:.1} {bx:.1},{tb:.1} {bx:.1},{bb:.1} {lx:.1},{bmy:.1}",
        lx = bx - hw,
        tmy = top_y,
        bx = bx,
        tb = top_y + hh,
        bb = by + hh,
        bmy = by,
    );
    let right = format!(
        "{bx:.1},{tb:.1} {rx:.1},{tmy:.1} {rx:.1},{bmy:.1} {bx:.1},{bb:.1}",
        bx = bx,
        tb = top_y + hh,
        rx = bx + hw,
        tmy = top_y,
        bmy = by,
        bb = by + hh,
    );
    (top, left, right)
}

/// Normalize wing counts to height fractions `[0, 1]`.
pub fn height_fractions(wings: &[Wing]) -> Vec<f64> {
    let max = wings.iter().map(|w| w.count).max().unwrap_or(0);
    if max == 0 {
        return wings.iter().map(|_| 0.0).collect();
    }
    wings.iter().map(|w| w.count as f64 / max as f64).collect()
}

/// Isometric palace of wing blocks (pure SVG, clickable).
#[component]
pub fn IsoPalace(
    /// Reactive wing list (cap to ~6 for layout).
    #[prop(into)]
    wings: Signal<Vec<Wing>>,
    /// Selected wing index (two-way).
    selected: RwSignal<Option<usize>>,
) -> impl IntoView {
    view! {
        <svg
            class="el-isopalace"
            viewBox="0 0 700 240"
            preserveAspectRatio="xMidYMid meet"
            role="group"
            aria-label="memory palace wings"
        >
            {move || {
                let list = wings.get();
                let fracs = height_fractions(&list);
                let n = list.len().max(1);
                let total_w = n as f64 * 110.0;
                let origin_x = (700.0 - total_w) / 2.0 + 55.0;
                list.into_iter()
                    .zip(fracs)
                    .enumerate()
                    .map(|(i, (w, hf))| {
                        let (top, left, right) = iso_block(i, hf, origin_x, 170.0);
                        let is_sel = move || selected.get() == Some(i);
                        let label = w.label.clone();
                        let count = w.count;
                        let bx = origin_x + i as f64 * 110.0;
                        view! {
                            <g
                                style="cursor:pointer;"
                                on:click=move |_| selected.update(|s| {
                                    *s = if *s == Some(i) { None } else { Some(i) };
                                })
                            >
                                <polygon
                                    points=left
                                    fill="var(--el-surface-2)"
                                    stroke="var(--el-line-strong)"
                                />
                                <polygon
                                    points=right
                                    fill="var(--el-surface-3)"
                                    stroke="var(--el-line-strong)"
                                />
                                <polygon
                                    points=top
                                    fill=move || if is_sel() { "var(--el-accent-soft)" } else { "var(--el-surface)" }
                                    stroke=move || if is_sel() { "var(--el-accent)" } else { "var(--el-line-strong)" }
                                    stroke-width="1.25"
                                />
                                <text
                                    x=format!("{bx:.1}")
                                    y="218"
                                    font-size="10"
                                    text-anchor="middle"
                                    fill=move || if is_sel() { "var(--el-accent)" } else { "var(--el-fg-4)" }
                                    style="font-family:var(--el-mono);"
                                >
                                    {format!("{label} · {count}")}
                                </text>
                            </g>
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

    fn w(label: &str, count: usize) -> Wing {
        Wing {
            label: label.to_string(),
            count,
        }
    }

    #[test]
    fn height_fractions_normalize_to_max() {
        let f = height_fractions(&[w("a", 5), w("b", 10)]);
        assert_eq!(f, vec![0.5, 1.0]);
    }

    #[test]
    fn height_fractions_zero_safe() {
        assert_eq!(height_fractions(&[w("a", 0)]), vec![0.0]);
        assert!(height_fractions(&[]).is_empty());
    }

    #[test]
    fn iso_block_taller_for_bigger_fraction() {
        let (top_lo, _, _) = iso_block(0, 0.1, 100.0, 170.0);
        let (top_hi, _, _) = iso_block(0, 1.0, 100.0, 170.0);
        let y_of = |s: &str| -> f64 {
            s.split(' ')
                .next()
                .unwrap()
                .split(',')
                .nth(1)
                .unwrap()
                .parse()
                .unwrap()
        };
        assert!(
            y_of(&top_hi) < y_of(&top_lo),
            "bigger fraction lifts the top face higher (smaller y)"
        );
    }
}
