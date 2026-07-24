//! /hooks — live hook-pipeline diagnostics. Elite W3 page
//! (SPEC 2026-06-12 §5.13, artboard `hifi-hooks-live`): hero with the
//! real `total_invocations` stat, four gate KPIs, the top-24 non-zero
//! counter table, per-histogram percentile MiniBars and the lifetime
//! activity strip — all from `GET /api/gate-metrics`.
//!
//! Real data only (zero placeholders). The endpoint returns a flat dict
//! of ~142 keys: numeric counters plus latency histograms under
//! `*_latency` keys shaped `{count, p50_us, p90_us, p99_us, p999_us,
//! max_us}` (captured live 2026-06-12: `hook_dispatch_latency.p50_us:
//! 205`, `rkyv_dispatch_count: 3624`, `total_invocations: 100`).
//!
//! Honesty constraints (SPEC §12):
//! - counters at zero (or absent) are omitted from the top-24 table;
//! - histograms with `count == 0` are omitted from the latency panel;
//! - the activity strip is labeled "lifetime counters by hook family" —
//!   gate-metrics exposes no timestamps, so no EventRibbon / time series
//!   is rendered;
//! - fetch failure renders an explicit "daemon indisponível" state.
//!
//! CSS prefix: `.pg-hooks-*` (block staged at `/tmp/w3-css/hooks.css`).

use leptos::prelude::*;
use leptos_meta::Title;
use serde_json::Value;

use crate::web::components::{KpiCell, KpiStrip, MiniBars, PageHero, Panel, ProgressTrack};
use crate::web::ctx::use_refresh_bus;
use crate::web::services::fetch_gate_metrics;

/// The four latency histograms exposed by gate-metrics, fixed order.
pub const LATENCY_KEYS: [(&str, &str); 4] = [
    ("hook_dispatch_latency", "hook dispatch"),
    ("rkyv_dispatch_latency", "rkyv dispatch"),
    ("ann_search_latency", "ANN search"),
    ("tantivy_query_latency", "tantivy query"),
];

/// Activity counters by hook family, fixed display order.
pub const ACTIVITY_KEYS: [(&str, &str); 5] = [
    ("activity_post_edit_count", "post_edit"),
    ("activity_post_write_count", "post_write"),
    ("activity_pre_compact_count", "pre_compact"),
    ("activity_pre_edit_count", "pre_edit"),
    ("activity_instructions_loaded_count", "instructions_loaded"),
];

/// Top `n` strictly-positive numeric counters, sorted descending by
/// value (key ascending on ties). Histogram objects and zero/absent
/// counters are excluded — only counters that actually fired appear.
pub fn top_counters(json: &Value, n: usize) -> Vec<(String, f64)> {
    let mut counters: Vec<(String, f64)> = json
        .as_object()
        .map(|o| {
            o.iter()
                .filter_map(|(k, v)| v.as_f64().map(|x| (k.clone(), x)))
                .filter(|(_, x)| *x > 0.0)
                .collect()
        })
        .unwrap_or_default();
    counters.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    counters.truncate(n);
    counters
}

/// `[p50, p90, p99, p999]` (µs) for the histogram at `key`, or `None`
/// when the histogram is absent, malformed or has `count == 0`.
pub fn histogram_bars(json: &Value, key: &str) -> Option<Vec<f64>> {
    let h = json.get(key)?;
    let count = h.get("count").and_then(Value::as_u64).unwrap_or(0);
    if count == 0 {
        return None;
    }
    let p = |k: &str| h.get(k).and_then(Value::as_f64).unwrap_or(0.0);
    Some(vec![p("p50_us"), p("p90_us"), p("p99_us"), p("p999_us")])
}

/// Format microseconds — `< 1000` rendered as µs, otherwise ms (1 decimal).
pub fn fmt_us(us: f64) -> String {
    if us < 1000.0 {
        format!("{us:.0} µs")
    } else {
        format!("{:.1} ms", us / 1000.0)
    }
}

/// Format a counter value: integers verbatim, fractions with 2 decimals.
pub fn fmt_count(v: f64) -> String {
    if v.fract().abs() < f64::EPSILON && v.abs() < 9.0e15 {
        (v as i64).to_string()
    } else {
        format!("{v:.2}")
    }
}

/// Fast-path share in percent over the pre_edit + pre_write gates:
/// `fast / (fast + full) * 100`, `None` when no gate has fired yet.
pub fn fast_path_pct(json: &Value) -> Option<f64> {
    let g = |k: &str| json.get(k).and_then(Value::as_f64).unwrap_or(0.0);
    let fast = g("pre_edit_fast_path") + g("pre_write_fast_path");
    let total = fast + g("pre_edit_full") + g("pre_write_full");
    if total <= 0.0 {
        None
    } else {
        Some(fast / total * 100.0)
    }
}

/// `(label, value)` pairs for the activity strip in [`ACTIVITY_KEYS`]
/// order — absent keys read as 0 (the daemon always reports them).
pub fn activity_series(json: &Value) -> Vec<(&'static str, f64)> {
    ACTIVITY_KEYS
        .iter()
        .map(|(key, label)| {
            (
                *label,
                json.get(*key).and_then(Value::as_f64).unwrap_or(0.0),
            )
        })
        .collect()
}

/// `/hooks` — Hooks Live page.
///
/// Layout: [`PageHero`] (total_invocations) · [`KpiStrip`] (dispatches,
/// hook p50, enrichment emits, fast-path %) · Panel 01 top-24 counters
/// table · Panel 02 latency histograms ([`MiniBars`] over the four
/// percentiles) · Panel 03 lifetime activity strip with legend chips.
/// The gate-metrics resource re-fetches on every refresh-bus tick.
#[component]
pub fn HooksLive() -> impl IntoView {
    let bus = use_refresh_bus();

    // None = fetch/parse failure → honest "daemon indisponível" state.
    let metrics = LocalResource::new(move || {
        let _tick = bus.tick.get();
        async move { fetch_gate_metrics().await.ok() }
    });

    // SPEC §4.4 — the top-24 reshape (filter + sort over ~142 keys) is
    // memoized so downstream rows only re-render on real value changes.
    let top = Memo::new(move |_| {
        metrics
            .get()
            .flatten()
            .as_ref()
            .map(|j| top_counters(j, 24))
            .unwrap_or_default()
    });

    let metric_signal = move |key: &'static str| {
        Signal::derive(move || {
            metrics
                .get()
                .flatten()
                .and_then(|j| j.get(key).and_then(Value::as_f64))
                .map(fmt_count)
                .unwrap_or_else(|| "—".to_string())
        })
    };

    let kpi_invocations = metric_signal("total_invocations");
    let kpi_dispatches = metric_signal("rkyv_dispatch_count");
    let kpi_enrichment = metric_signal("enrichment_emit_count");
    let kpi_p50 = Signal::derive(move || {
        metrics
            .get()
            .flatten()
            .and_then(|j| histogram_bars(&j, "hook_dispatch_latency"))
            .and_then(|bars| bars.first().copied())
            .map(fmt_us)
            .unwrap_or_else(|| "—".to_string())
    });
    let kpi_fast_path = Signal::derive(move || {
        metrics
            .get()
            .flatten()
            .as_ref()
            .and_then(fast_path_pct)
            .map(|p| format!("{p:.0}%"))
            .unwrap_or_else(|| "—".to_string())
    });

    view! {
        <div class="page pg-hooks">
            <Title text="Touring — Hooks"/>
            <div class="el-main-editorial">
                <PageHero
                    eyebrow="DIAGNOSTICS · HOOKS"
                    title="Hooks"
                    title_em="live"
                    sub="The live gate-metrics pipeline: dispatch volume, latency percentiles per hook surface, enrichment output and lifetime activity per hook family — measured by the daemon, never estimated."
                    stat=kpi_invocations
                    stat_label="INVOCATIONS"
                >
                    <a class="el-btn" href="/health">"Health"</a>
                </PageHero>

                <KpiStrip>
                    <KpiCell label="Dispatches" value=kpi_dispatches sub="rkyv dispatch count"/>
                    <KpiCell label="Hook P50" value=kpi_p50 sub="dispatch latency, median"/>
                    <KpiCell label="Enrichment" value=kpi_enrichment sub="context emits"/>
                    <KpiCell label="Fast-path" value=kpi_fast_path sub="pre_edit + pre_write share"/>
                </KpiStrip>

                <Suspense fallback=|| view! { <div class="el-skeleton" aria-label="loading"></div> }>
                    {move || metrics.get().map(|payload| match payload {
                        None => view! {
                            <section class="el-panel pg-hooks-empty">
                                <p class="el-eyebrow">"daemon indisponível"</p>
                                <p class="pg-hooks-empty-note">
                                    "GET /api/gate-metrics não respondeu JSON — sem counters reais para exibir."
                                </p>
                            </section>
                        }
                        .into_any(),
                        Some(json) => {
                            // ── Panel 01 · top-24 non-zero counters ──────
                            let rows = top.get();
                            let nonzero_total = top_counters(&json, usize::MAX).len();
                            let max = rows.first().map(|r| r.1).unwrap_or(1.0).max(1.0);
                            let counters_body = if rows.is_empty() {
                                view! {
                                    <div class="el-empty">"No counter above zero yet — the daemon has not dispatched any hook."</div>
                                }
                                .into_any()
                            } else {
                                view! {
                                    <div class="pg-hooks-counters-cap el-mono">
                                        {format!("top {} of {} non-zero counters", rows.len(), nonzero_total)}
                                    </div>
                                    <table class="el-table pg-hooks-counters">
                                        <thead>
                                            <tr>
                                                <th>"Counter"</th>
                                                <th>"Value"</th>
                                                <th>"Share of max"</th>
                                            </tr>
                                        </thead>
                                        <tbody>
                                            {rows
                                                .into_iter()
                                                .map(|(key, v)| {
                                                    let frac = (v / max).clamp(0.0, 1.0);
                                                    view! {
                                                        <tr>
                                                            <td class="el-mono pg-hooks-counter-key">{key}</td>
                                                            <td class="pg-hooks-counter-val">{fmt_count(v)}</td>
                                                            <td class="pg-hooks-prog-col">
                                                                <ProgressTrack value=Signal::derive(move || frac)/>
                                                            </td>
                                                        </tr>
                                                    }
                                                })
                                                .collect_view()}
                                        </tbody>
                                    </table>
                                }
                                .into_any()
                            };

                            // ── Panel 02 · latency histograms (count > 0) ─
                            let lat_rows: Vec<_> = LATENCY_KEYS
                                .iter()
                                .filter_map(|(key, label)| {
                                    let bars = histogram_bars(&json, key)?;
                                    let h = json.get(*key)?;
                                    let count = h.get("count").and_then(Value::as_u64).unwrap_or(0);
                                    let max_us = h.get("max_us").and_then(Value::as_f64).unwrap_or(0.0);
                                    Some(view! {
                                        <div class="pg-hooks-lat-row">
                                            <div class="pg-hooks-lat-name">
                                                <span class="el-mono">{*label}</span>
                                                <span class="pg-hooks-lat-count">{format!("{count} samples")}</span>
                                            </div>
                                            <div class="pg-hooks-lat-bars">
                                                <MiniBars
                                                    values=Signal::derive(move || bars.clone())
                                                    highlight_from=2
                                                />
                                            </div>
                                            <div class="el-mono pg-hooks-lat-max">{format!("max {}", fmt_us(max_us))}</div>
                                        </div>
                                    })
                                })
                                .collect();
                            let latency_body = if lat_rows.is_empty() {
                                view! {
                                    <div class="el-empty">"All latency histograms report count = 0 — nothing measured yet."</div>
                                }
                                .into_any()
                            } else {
                                view! {
                                    <div class="pg-hooks-lat-rows">{lat_rows.into_iter().collect_view()}</div>
                                    <div class="pg-hooks-lat-caption">
                                        "p50 · p90 · p99 · p999 per row, normalized to the row max — tail percentiles (p99+) in accent"
                                    </div>
                                }
                                .into_any()
                            };

                            // ── Panel 03 · lifetime activity by family ───
                            let series = activity_series(&json);
                            let values: Vec<f64> = series.iter().map(|(_, v)| *v).collect();
                            let activity_body = view! {
                                <div class="pg-hooks-activity">
                                    <MiniBars values=Signal::derive(move || values.clone())/>
                                    <div class="pg-hooks-activity-note">
                                        "lifetime counters by hook family — not a time series (gate-metrics exposes no timestamps)"
                                    </div>
                                    <div class="el-chart-legend pg-hooks-legend">
                                        {series
                                            .into_iter()
                                            .map(|(name, v)| view! {
                                                <span class="pg-hooks-legend-chip">
                                                    <span class="el-mono">{name}</span>
                                                    <span class="pg-hooks-legend-val">{fmt_count(v)}</span>
                                                </span>
                                            })
                                            .collect_view()}
                                    </div>
                                </div>
                            };

                            view! {
                                <Panel num="01" eyebrow="Counters" cmd="touring gate-metrics -j">
                                    {counters_body}
                                </Panel>
                                <Panel num="02" eyebrow="Latency histograms" cmd="*_latency · {count, p50…p999, max}">
                                    {latency_body}
                                </Panel>
                                <Panel num="03" eyebrow="Activity" cmd="activity_*_count">
                                    {activity_body}
                                </Panel>
                            }
                            .into_any()
                        }
                    })}
                </Suspense>
            </div>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn top_counters_filters_zeros_objects_and_sorts_desc() {
        let j = json!({
            "b_mid": 5.0,
            "a_big": 100,
            "zero": 0,
            "hist_latency": {"count": 3, "p50_us": 1},
            "label": "not-a-number",
            "c_small": 1.5
        });
        let top = top_counters(&j, 24);
        assert_eq!(
            top,
            vec![
                ("a_big".to_string(), 100.0),
                ("b_mid".to_string(), 5.0),
                ("c_small".to_string(), 1.5)
            ]
        );
    }

    #[test]
    fn top_counters_truncates_and_tie_breaks_alphabetically() {
        let j = json!({"z": 7, "a": 7, "m": 9});
        assert_eq!(
            top_counters(&j, 2),
            vec![("m".to_string(), 9.0), ("a".to_string(), 7.0)]
        );
        assert!(top_counters(&json!("not-an-object"), 5).is_empty());
    }

    #[test]
    fn histogram_bars_extracts_percentiles_in_order() {
        let j = json!({"hook_dispatch_latency": {
            "count": 6416, "p50_us": 205, "p90_us": 85183,
            "p99_us": 629759, "p999_us": 5357567, "max_us": 5849087
        }});
        assert_eq!(
            histogram_bars(&j, "hook_dispatch_latency"),
            Some(vec![205.0, 85183.0, 629759.0, 5357567.0])
        );
    }

    #[test]
    fn histogram_bars_none_when_empty_or_missing() {
        let j = json!({"tantivy_query_latency": {
            "count": 0, "p50_us": 0, "p90_us": 0, "p99_us": 0, "p999_us": 0, "max_us": 0
        }});
        assert_eq!(histogram_bars(&j, "tantivy_query_latency"), None);
        assert_eq!(histogram_bars(&j, "absent_latency"), None);
        assert_eq!(histogram_bars(&json!({"k": 3}), "k"), None);
    }

    #[test]
    fn fmt_us_switches_to_ms_at_1000() {
        assert_eq!(fmt_us(0.0), "0 µs");
        assert_eq!(fmt_us(999.0), "999 µs");
        assert_eq!(fmt_us(1000.0), "1.0 ms");
        assert_eq!(fmt_us(85183.0), "85.2 ms");
    }

    #[test]
    fn fmt_count_integers_verbatim_fractions_two_decimals() {
        assert_eq!(fmt_count(3624.0), "3624");
        assert_eq!(fmt_count(1655.1217105263158), "1655.12");
        assert_eq!(fmt_count(0.0), "0");
    }

    #[test]
    fn fast_path_pct_from_real_shape_and_none_when_idle() {
        let j = json!({
            "pre_edit_fast_path": 40, "pre_edit_full": 10,
            "pre_write_fast_path": 40, "pre_write_full": 10
        });
        let pct = fast_path_pct(&j).unwrap_or(-1.0);
        assert!((pct - 80.0).abs() < 1e-9);
        assert_eq!(fast_path_pct(&json!({})), None);
        assert_eq!(
            fast_path_pct(&json!({"pre_edit_fast_path": 0, "pre_edit_full": 0})),
            None
        );
    }

    #[test]
    fn activity_series_fixed_order_with_absent_keys_as_zero() {
        let j = json!({
            "activity_post_edit_count": 74,
            "activity_post_write_count": 32,
            "activity_pre_compact_count": 0,
            "activity_pre_edit_count": 0
            // activity_instructions_loaded_count intentionally absent
        });
        assert_eq!(
            activity_series(&j),
            vec![
                ("post_edit", 74.0),
                ("post_write", 32.0),
                ("pre_compact", 0.0),
                ("pre_edit", 0.0),
                ("instructions_loaded", 0.0)
            ]
        );
    }
}
