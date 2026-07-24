//! AreaChart — shared SVG diagram primitive (SPEC 2026-06-12 §4.3).
//!
//! Multi-series line/area chart with horizontal grid lines and
//! min/max labels. Pure SVG, deterministic, colors only via tokens.
//! Used by /dashboard (signal evolution), /hooks (latency), /health.

use leptos::prelude::*;

/// One plotted series. `color` must be a CSS token expression
/// (e.g. `"var(--el-accent)"`); `dashed` renders a dashed stroke
/// (used for P95/prev overlays); `area` fills under the line.
#[derive(Debug, Clone, PartialEq)]
pub struct AreaSeries {
    /// Legend label.
    pub label: &'static str,
    /// CSS color token, e.g. `var(--el-accent)`.
    pub color: &'static str,
    /// Values oldest → newest.
    pub points: Vec<f64>,
    /// Dashed stroke (overlay/percentile style).
    pub dashed: bool,
    /// Soft fill under the line.
    pub area: bool,
}

/// Build an SVG path for `points` normalized into `w`×`h` using the
/// shared `[lo, hi]` domain. Returns an empty string for <2 points.
pub fn series_path(points: &[f64], w: f64, h: f64, lo: f64, hi: f64) -> String {
    if points.len() < 2 {
        return String::new();
    }
    let range = (hi - lo).max(1e-9);
    let dx = w / (points.len() as f64 - 1.0);
    let mut d = String::with_capacity(points.len() * 16);
    for (i, v) in points.iter().enumerate() {
        let x = i as f64 * dx;
        let y = h - ((v - lo) / range) * h;
        d.push(if i == 0 { 'M' } else { 'L' });
        d.push_str(&format!("{x:.1} {y:.1} "));
    }
    d
}

/// Shared min/max across every series (the common Y domain).
pub fn domain(series: &[AreaSeries]) -> (f64, f64) {
    let mut lo = f64::INFINITY;
    let mut hi = f64::NEG_INFINITY;
    for s in series {
        for v in &s.points {
            lo = lo.min(*v);
            hi = hi.max(*v);
        }
    }
    if !lo.is_finite() || !hi.is_finite() {
        (0.0, 1.0)
    } else if (hi - lo).abs() < 1e-9 {
        (lo - 0.5, hi + 0.5)
    } else {
        (lo, hi)
    }
}

/// Multi-series area/line chart with grid lines (SPEC §4.3).
#[component]
pub fn AreaChart(
    /// Series to plot (first `area=true` series gets the soft fill).
    #[prop(into)]
    series: Signal<Vec<AreaSeries>>,
    /// ViewBox width.
    #[prop(default = 900.0)]
    width: f64,
    /// ViewBox height.
    #[prop(default = 200.0)]
    height: f64,
    /// Number of horizontal grid lines.
    #[prop(default = 4)]
    grid_lines: usize,
) -> impl IntoView {
    let geom = Memo::new(move |_| {
        let all = series.get();
        let (lo, hi) = domain(&all);
        let paths: Vec<(String, &'static str, bool, bool, String)> = all
            .iter()
            .map(|s| {
                let line = series_path(&s.points, width, height, lo, hi);
                let area_path = if s.area && !line.is_empty() {
                    format!("{line}L {width:.1} {height:.1} L 0 {height:.1} Z")
                } else {
                    String::new()
                };
                (line, s.color, s.dashed, s.area, area_path)
            })
            .collect();
        (paths, lo, hi)
    });

    view! {
        <svg
            class="el-areachart"
            viewBox=format!("0 0 {width} {height}")
            preserveAspectRatio="none"
            aria-hidden="true"
        >
            {(1..=grid_lines)
                .map(|i| height * i as f64 / (grid_lines as f64 + 1.0))
                .map(|y| view! {
                    <line
                        x1="0"
                        y1=format!("{y:.1}")
                        x2=format!("{width}")
                        y2=format!("{y:.1}")
                        stroke="var(--el-line)"
                        stroke-width="1"
                    />
                })
                .collect_view()}
            {move || {
                geom.get()
                    .0
                    .into_iter()
                    .map(|(line, color, dashed, _is_area, area_path)| {
                        view! {
                            {(!area_path.is_empty()).then(|| view! {
                                <path d=area_path fill=color opacity="0.08" stroke="none"/>
                            })}
                            <path
                                d=line
                                fill="none"
                                stroke=color
                                stroke-width="1.5"
                                stroke-linecap="round"
                                stroke-dasharray=if dashed { "5 4" } else { "" }
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

    fn s(points: Vec<f64>) -> AreaSeries {
        AreaSeries {
            label: "t",
            color: "var(--el-accent)",
            points,
            dashed: false,
            area: false,
        }
    }

    #[test]
    fn series_path_normalizes_into_box() {
        let d = series_path(&[0.0, 5.0, 10.0], 100.0, 50.0, 0.0, 10.0);
        assert!(d.starts_with("M0.0 50.0"), "first point bottom-left: {d}");
        assert!(d.contains("L50.0 25.0"), "midpoint centered: {d}");
        assert!(d.contains("L100.0 0.0"), "last point top-right: {d}");
    }

    #[test]
    fn series_path_empty_for_single_point() {
        assert!(series_path(&[1.0], 100.0, 50.0, 0.0, 1.0).is_empty());
    }

    #[test]
    fn domain_spans_all_series() {
        let (lo, hi) = domain(&[s(vec![3.0, 7.0]), s(vec![-1.0, 4.0])]);
        assert_eq!((lo, hi), (-1.0, 7.0));
    }

    #[test]
    fn domain_pads_flat_series() {
        let (lo, hi) = domain(&[s(vec![5.0, 5.0, 5.0])]);
        assert!(hi > lo, "flat series must get a padded domain");
    }

    #[test]
    fn domain_defaults_when_empty() {
        assert_eq!(domain(&[]), (0.0, 1.0));
    }
}
