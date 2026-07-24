//! /quality/diff — snapshot comparison with radar overlay (Elite W3 §5.4,
//! artboard hifi-quality-diff).
//!
//! DATA DECISION (honest, 100% real): the server endpoint
//! `/api/quality/diff?prev=&curr=` requires two **filesystem root paths**
//! (it shells `touring quality-signal --root <path>` per side — see
//! `api_quality_diff` in `web/server/mod.rs`). It cannot compare two
//! points in time of the *same* workspace. This page therefore performs a
//! **client-side diff of two history snapshots** picked from
//! `/api/quality/history?limit=N` — each snapshot is a complete real
//! payload (`signal_0_10000`, `computed_at`, `bottleneck`, `root_causes`,
//! `raw`), so no value on screen is mocked. The picker panel is labeled
//! accordingly. `fetch_quality_diff` remains available in
//! `services::quality_client` for cross-root comparisons.

use leptos::prelude::*;
use leptos_meta::Title;
use serde_json::Value;

use crate::web::components::{KpiCell, KpiStrip, PageHero, Panel, RadarChart};
use crate::web::ctx::use_refresh_bus;
use crate::web::models::quality::RootCauseScores;
use crate::web::services::fetch_quality_history;

/// How many history snapshots the picker offers (newest first).
const HISTORY_LIMIT: usize = 24;

// ─── pure helpers (unit-tested below) ────────────────────────────────

/// Number of snapshots in a `/api/quality/history` payload.
pub fn snapshot_count(history: &Value) -> usize {
    history
        .get("snapshots")
        .and_then(Value::as_array)
        .map_or(0, Vec::len)
}

/// Snapshot object at `idx` (0 = most recent).
fn snapshot_at(history: &Value, idx: usize) -> Option<&Value> {
    history.get("snapshots")?.as_array()?.get(idx)
}

/// Parse the five root-cause axis scores of snapshot `idx`.
pub fn snapshot_scores(history: &Value, idx: usize) -> Option<RootCauseScores> {
    let rc = snapshot_at(history, idx)?.get("root_causes")?.clone();
    serde_json::from_value(rc).ok()
}

/// Composite signal (`signal_0_10000`) of snapshot `idx`.
pub fn snapshot_signal(history: &Value, idx: usize) -> Option<i64> {
    snapshot_at(history, idx)?.get("signal_0_10000")?.as_i64()
}

/// Bottleneck axis name of snapshot `idx`.
pub fn snapshot_bottleneck(history: &Value, idx: usize) -> Option<String> {
    Some(
        snapshot_at(history, idx)?
            .get("bottleneck")?
            .as_str()?
            .to_string(),
    )
}

/// Picker label `"#N · 7616 · 2026-06-12"` — index, composite signal and
/// the UTC date of `computed_at` (date omitted when the field is absent).
pub fn snapshot_label(history: &Value, idx: usize) -> String {
    let sig = snapshot_signal(history, idx).map_or_else(|| "—".to_string(), |s| s.to_string());
    let date = snapshot_at(history, idx)
        .and_then(|s| s.get("computed_at"))
        .and_then(Value::as_f64)
        .map(fmt_epoch_short);
    match date {
        Some(d) if d != "—" => format!("#{idx} · {sig} · {d}"),
        _ => format!("#{idx} · {sig}"),
    }
}

/// `YYYY-MM-DD` (UTC) from a Unix epoch in seconds — exact proleptic
/// Gregorian via Howard Hinnant's `civil_from_days`, no chrono needed.
/// Millisecond payloads (> 1e11) are normalized; invalid input → `"—"`.
pub fn fmt_epoch_short(epoch: f64) -> String {
    if !epoch.is_finite() || epoch < 0.0 {
        return "—".to_string();
    }
    // Anything past year ~5138 in seconds is really milliseconds.
    let secs = if epoch > 1.0e11 {
        epoch / 1000.0
    } else {
        epoch
    };
    let days = (secs as i64).div_euclid(86_400);
    let (y, m, d) = civil_from_days(days);
    format!("{y:04}-{m:02}-{d:02}")
}

/// Days-since-epoch → (year, month, day) — Hinnant's civil_from_days.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097); // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32; // [1, 12]
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// Per-axis `(label, prev, curr, delta)` rows — delta = curr − prev.
/// Order matches [`RootCauseScores`] / the radar axis order.
pub fn axis_delta(
    prev: &RootCauseScores,
    curr: &RootCauseScores,
) -> [(&'static str, f64, f64, f64); 5] {
    [
        (
            "Modularity",
            prev.modularity,
            curr.modularity,
            curr.modularity - prev.modularity,
        ),
        (
            "Acyclicity",
            prev.acyclicity,
            curr.acyclicity,
            curr.acyclicity - prev.acyclicity,
        ),
        ("Depth", prev.depth, curr.depth, curr.depth - prev.depth),
        (
            "Equality",
            prev.equality,
            curr.equality,
            curr.equality - prev.equality,
        ),
        (
            "Redundancy",
            prev.redundancy,
            curr.redundancy,
            curr.redundancy - prev.redundancy,
        ),
    ]
}

/// Raw counters surfaced in panel 04 — `(key, integer-valued)`.
const RAW_KEYS: [(&str, bool); 8] = [
    ("cycle_count", true),
    ("max_depth", true),
    ("complexity_gini", false),
    ("redundancy_ratio", false),
    ("modularity_q", false),
    ("total_functions", true),
    ("total_nodes", true),
    ("total_edges", true),
];

/// `(key, prev, curr, delta)` formatted rows for the raw counters of two
/// snapshot objects (each carrying a `raw` map). Missing values render
/// `"—"`; integers print without decimals, ratios with 3 decimals.
pub fn raw_delta_rows(prev: &Value, curr: &Value) -> Vec<(String, String, String, String)> {
    let p_raw = prev.get("raw");
    let c_raw = curr.get("raw");
    RAW_KEYS
        .iter()
        .map(|(key, integer)| {
            let pv = p_raw.and_then(|r| r.get(key)).and_then(Value::as_f64);
            let cv = c_raw.and_then(|r| r.get(key)).and_then(Value::as_f64);
            let fmt = |v: Option<f64>| match v {
                Some(x) if *integer => format!("{}", x as i64),
                Some(x) => format!("{x:.3}"),
                None => "—".to_string(),
            };
            let delta = match (pv, cv) {
                (Some(p), Some(c)) => fmt_signed(c - p, if *integer { 0 } else { 3 }),
                _ => "—".to_string(),
            };
            ((*key).to_string(), fmt(pv), fmt(cv), delta)
        })
        .collect()
}

/// Sign-prefixed float with `prec` decimals — `"+0.034"` / `"-2"` / `"0.00"`.
pub fn fmt_signed(d: f64, prec: usize) -> String {
    if d == 0.0 {
        format!("{:.prec$}", 0.0, prec = prec)
    } else {
        format!("{d:+.prec$}", prec = prec)
    }
}

/// Sign-prefixed integer — `"+212"` / `"-34"` / `"0"`.
pub fn fmt_signed_i64(d: i64) -> String {
    if d > 0 {
        format!("+{d}")
    } else {
        format!("{d}")
    }
}

// ─── component ───────────────────────────────────────────────────────

/// `/quality/diff` — pick any two persisted snapshots and compare them:
/// radar overlay (prev violet dashed vs curr accent), per-axis delta
/// table with a dual-marker knob, and the raw structural counters diff.
#[component]
pub fn QualityDiff() -> impl IntoView {
    let bus = use_refresh_bus();
    // 0 = most recent snapshot. Defaults: PREV = #1, CURR = #0.
    let prev_idx = RwSignal::new(1usize);
    let curr_idx = RwSignal::new(0usize);

    let history = LocalResource::new(move || {
        let _tick = bus.tick.get();
        async move { fetch_quality_history(HISTORY_LIMIT).await.ok() }
    });

    view! {
        <div class="page pg-qdiff">
            <Title text="Touring — Quality Diff"/>
            <Suspense fallback=|| view! { <div class="el-skeleton" aria-label="loading"></div> }>
                {move || history.get().map(|maybe| match maybe {
                    Some(h) if snapshot_count(&h) >= 2 => {
                        render_diff(h, prev_idx, curr_idx).into_any()
                    }
                    Some(_) => view! {
                        <div class="el-empty">"Need at least 2 snapshots to diff."</div>
                    }.into_any(),
                    None => view! {
                        <div class="el-empty">
                            "History endpoint unavailable — nothing to diff."
                        </div>
                    }.into_any(),
                })}
            </Suspense>
        </div>
    }
}

/// Full diff layout for a history payload with ≥ 2 snapshots.
fn render_diff(h: Value, prev_idx: RwSignal<usize>, curr_idx: RwSignal<usize>) -> impl IntoView {
    let n = snapshot_count(&h);

    // Reactive slices over the (immutable) history payload. Out-of-range
    // indices fail soft to defaults — helpers return Option.
    let h_prev = h.clone();
    let prev_scores =
        Signal::derive(move || snapshot_scores(&h_prev, prev_idx.get()).unwrap_or_default());
    let h_curr = h.clone();
    let curr_scores =
        Signal::derive(move || snapshot_scores(&h_curr, curr_idx.get()).unwrap_or_default());
    let h_bot = h.clone();
    let curr_bottleneck =
        Signal::derive(move || snapshot_bottleneck(&h_bot, curr_idx.get()).unwrap_or_default());

    let h_stat = h.clone();
    let delta_stat = Signal::derive(move || {
        match (
            snapshot_signal(&h_stat, prev_idx.get()),
            snapshot_signal(&h_stat, curr_idx.get()),
        ) {
            (Some(p), Some(c)) => fmt_signed_i64(c - p),
            _ => "—".to_string(),
        }
    });
    let h_kpi_prev = h.clone();
    let prev_sig_text = Signal::derive(move || {
        snapshot_signal(&h_kpi_prev, prev_idx.get())
            .map_or_else(|| "—".to_string(), |s| s.to_string())
    });
    let h_kpi_curr = h.clone();
    let curr_sig_text = Signal::derive(move || {
        snapshot_signal(&h_kpi_curr, curr_idx.get())
            .map_or_else(|| "—".to_string(), |s| s.to_string())
    });
    let count_text = Signal::derive(move || n.to_string());

    let prev_options = (0..n)
        .map(|i| {
            let label = snapshot_label(&h, i);
            view! {
                <option value=i.to_string() prop:selected=move || prev_idx.get() == i>
                    {label}
                </option>
            }
        })
        .collect_view();
    let curr_options = (0..n)
        .map(|i| {
            let label = snapshot_label(&h, i);
            view! {
                <option value=i.to_string() prop:selected=move || curr_idx.get() == i>
                    {label}
                </option>
            }
        })
        .collect_view();

    let on_swap = move |_| {
        let p = prev_idx.get_untracked();
        let c = curr_idx.get_untracked();
        prev_idx.set(c);
        curr_idx.set(p);
    };

    let h_raw = h.clone();

    view! {
        <div class="el-main-editorial">
            <PageHero
                eyebrow="SENTRUX · SNAPSHOT DIFF"
                title="Diff"
                title_em="compare"
                sub="Pick any two persisted Sentrux snapshots and compare them — overlay radar, per-axis deltas and raw structural counters. The diff is computed client-side from real history payloads."
                stat=delta_stat
                stat_label="DELTA"
            >
                <a class="el-btn" href="/quality">"Signal"</a>
                <a class="el-btn" href="/quality/rules">"Rules"</a>
            </PageHero>

            <KpiStrip>
                <KpiCell label="Prev composite" value=prev_sig_text sub="of 10,000"/>
                <KpiCell label="Curr composite" value=curr_sig_text sub="of 10,000"/>
                <KpiCell label="Delta" value=delta_stat sub="curr − prev"/>
                <KpiCell label="Snapshots" value=count_text sub="in history window"/>
            </KpiStrip>

            // ── 01 · Picker ───────────────────────────────────────────
            <Panel num="01" eyebrow="Picker" cmd="touring quality history">
                <div class="pg-qdiff-picker">
                    <label class="pg-qdiff-picker-field">
                        <span class="el-eyebrow pg-qdiff-picker-tag-prev">"Prev"</span>
                        <select
                            class="pg-qdiff-select"
                            on:change=move |e| {
                                prev_idx.set(event_target_value(&e).parse().unwrap_or(0))
                            }
                        >
                            {prev_options}
                        </select>
                    </label>
                    <button class="el-btn el-btn-sm" on:click=on_swap>"Swap"</button>
                    <label class="pg-qdiff-picker-field">
                        <span class="el-eyebrow pg-qdiff-picker-tag-curr">"Curr"</span>
                        <select
                            class="pg-qdiff-select"
                            on:change=move |e| {
                                curr_idx.set(event_target_value(&e).parse().unwrap_or(0))
                            }
                        >
                            {curr_options}
                        </select>
                    </label>
                    <span class="pg-qdiff-picker-note el-mono">
                        "client-side diff of two history snapshots"
                    </span>
                </div>
            </Panel>

            <section class="pg-qdiff-grid">
                // ── 02 · Radar overlay ────────────────────────────────
                <Panel num="02" eyebrow="Radar overlay" cmd="prev (violet) vs curr (accent)">
                    <div class="el-chart-legend pg-qdiff-legend">
                        <span class="pg-qdiff-chip">
                            <span class="pg-qdiff-swatch pg-qdiff-swatch-prev"></span>
                            "prev"
                        </span>
                        <span class="pg-qdiff-chip">
                            <span class="pg-qdiff-swatch pg-qdiff-swatch-curr"></span>
                            "curr"
                        </span>
                    </div>
                    <div class="pg-qdiff-radar">
                        <RadarChart
                            scores=curr_scores
                            overlay=prev_scores
                            bottleneck=curr_bottleneck
                        />
                    </div>
                </Panel>

                // ── 03 · Delta table ──────────────────────────────────
                <Panel num="03" eyebrow="Delta table" cmd="Δ = curr − prev">
                    <table class="el-table pg-qdiff-table">
                        <thead>
                            <tr>
                                <th>"Axis"</th>
                                <th class="pg-qdiff-th-num">"Prev"</th>
                                <th class="pg-qdiff-th-num">"Curr"</th>
                                <th class="pg-qdiff-th-num">"Δ"</th>
                                <th></th>
                            </tr>
                        </thead>
                        <tbody>
                            {move || {
                                let p = prev_scores.get();
                                let c = curr_scores.get();
                                axis_delta(&p, &c)
                                    .into_iter()
                                    .map(|(label, pv, cv, d)| {
                                        let delta_style = if d > 0.0 {
                                            "color:var(--el-pos)"
                                        } else if d < 0.0 {
                                            "color:var(--el-neg)"
                                        } else {
                                            "color:var(--el-fg-4)"
                                        };
                                        view! {
                                            <tr>
                                                <td class="pg-qdiff-axis">{label}</td>
                                                <td class="el-mono pg-qdiff-val-prev">
                                                    {format!("{pv:.2}")}
                                                </td>
                                                <td class="el-mono pg-qdiff-val-curr">
                                                    {format!("{cv:.2}")}
                                                </td>
                                                <td class="el-mono pg-qdiff-val-delta" style=delta_style>
                                                    {fmt_signed(d, 2)}
                                                </td>
                                                <td class="pg-qdiff-knob-col">
                                                    <div class="pg-qdiff-knob">
                                                        <span
                                                            class="pg-qdiff-knob-marker pg-qdiff-knob-prev"
                                                            style=format!(
                                                                "left:{:.1}%",
                                                                pv.clamp(0.0, 1.0) * 100.0,
                                                            )
                                                        ></span>
                                                        <span
                                                            class="pg-qdiff-knob-marker pg-qdiff-knob-curr"
                                                            style=format!(
                                                                "left:{:.1}%",
                                                                cv.clamp(0.0, 1.0) * 100.0,
                                                            )
                                                        ></span>
                                                    </div>
                                                </td>
                                            </tr>
                                        }
                                    })
                                    .collect_view()
                            }}
                        </tbody>
                    </table>
                </Panel>
            </section>

            // ── 04 · Raw counters diff ────────────────────────────────
            <Panel num="04" eyebrow="Raw counters diff" cmd="raw structural counters">
                <table class="el-table pg-qdiff-table">
                    <thead>
                        <tr>
                            <th>"Counter"</th>
                            <th class="pg-qdiff-th-num">"Prev"</th>
                            <th class="pg-qdiff-th-num">"Curr"</th>
                            <th class="pg-qdiff-th-num">"Δ"</th>
                        </tr>
                    </thead>
                    <tbody>
                        {move || {
                            let null = Value::Null;
                            let p = snapshot_at(&h_raw, prev_idx.get()).unwrap_or(&null);
                            let c = snapshot_at(&h_raw, curr_idx.get()).unwrap_or(&null);
                            raw_delta_rows(p, c)
                                .into_iter()
                                .map(|(key, pv, cv, dv)| view! {
                                    <tr>
                                        <td class="el-mono pg-qdiff-raw-key">{key}</td>
                                        <td class="el-mono pg-qdiff-val-prev">{pv}</td>
                                        <td class="el-mono pg-qdiff-val-curr">{cv}</td>
                                        <td class="el-mono pg-qdiff-val-delta">{dv}</td>
                                    </tr>
                                })
                                .collect_view()
                        }}
                    </tbody>
                </table>
            </Panel>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_history() -> Value {
        serde_json::json!({
            "count": 2,
            "snapshots": [
                {
                    "signal_0_10000": 7616,
                    "computed_at": 1_718_150_400.0,
                    "bottleneck": "modularity",
                    "raw": {
                        "cycle_count": 3, "max_depth": 12, "complexity_gini": 0.41,
                        "redundancy_ratio": 0.07, "modularity_q": 0.55,
                        "total_functions": 1000, "total_nodes": 1500, "total_edges": 4200
                    },
                    "root_causes": {
                        "modularity": 0.55, "acyclicity": 0.9, "depth": 0.8,
                        "equality": 0.7, "redundancy": 0.93
                    }
                },
                {
                    "signal_0_10000": 7404,
                    "computed_at": 1_718_064_000.0,
                    "bottleneck": "depth",
                    "raw": {
                        "cycle_count": 5, "max_depth": 14, "complexity_gini": 0.44,
                        "redundancy_ratio": 0.08, "modularity_q": 0.52,
                        "total_functions": 980, "total_nodes": 1450, "total_edges": 4100
                    },
                    "root_causes": {
                        "modularity": 0.52, "acyclicity": 0.85, "depth": 0.7,
                        "equality": 0.68, "redundancy": 0.92
                    }
                }
            ]
        })
    }

    #[test]
    fn snapshot_scores_parses_real_shape() {
        let h = sample_history();
        let s0 = snapshot_scores(&h, 0).expect("idx 0 parses");
        assert!((s0.modularity - 0.55).abs() < 1e-9);
        assert!((s0.redundancy - 0.93).abs() < 1e-9);
        assert!(snapshot_scores(&h, 5).is_none(), "out of range → None");
        assert!(snapshot_scores(&serde_json::json!({"error": "x"}), 0).is_none());
    }

    #[test]
    fn snapshot_metadata_accessors() {
        let h = sample_history();
        assert_eq!(snapshot_count(&h), 2);
        assert_eq!(snapshot_count(&serde_json::json!({"error": "boom"})), 0);
        assert_eq!(snapshot_signal(&h, 0), Some(7616));
        assert_eq!(snapshot_signal(&h, 1), Some(7404));
        assert_eq!(snapshot_bottleneck(&h, 0).as_deref(), Some("modularity"));
        assert_eq!(snapshot_bottleneck(&h, 9), None);
    }

    #[test]
    fn axis_delta_computes_curr_minus_prev() {
        let h = sample_history();
        let prev = snapshot_scores(&h, 1).expect("prev");
        let curr = snapshot_scores(&h, 0).expect("curr");
        let rows = axis_delta(&prev, &curr);
        assert_eq!(rows[0].0, "Modularity");
        assert!((rows[0].3 - 0.03).abs() < 1e-9);
        assert_eq!(rows[2].0, "Depth");
        assert!((rows[2].3 - 0.10).abs() < 1e-9);
        assert_eq!(rows.len(), 5);
    }

    #[test]
    fn raw_delta_rows_formats_ints_floats_and_missing() {
        let h = sample_history();
        let snaps = h.get("snapshots").and_then(Value::as_array).expect("arr");
        let rows = raw_delta_rows(&snaps[1], &snaps[0]);
        assert_eq!(rows.len(), 8);
        // cycle_count: 5 → 3 (integer, signed delta)
        assert_eq!(
            rows[0],
            (
                "cycle_count".to_string(),
                "5".to_string(),
                "3".to_string(),
                "-2".to_string(),
            )
        );
        // complexity_gini: 0.44 → 0.41 (3 decimals)
        assert_eq!(rows[2].1, "0.440");
        assert_eq!(rows[2].2, "0.410");
        assert_eq!(rows[2].3, "-0.030");
        // missing raw maps → all "—"
        let empty = raw_delta_rows(&serde_json::json!({}), &serde_json::json!({}));
        assert!(
            empty
                .iter()
                .all(|(_, p, c, d)| p == "—" && c == "—" && d == "—")
        );
    }

    #[test]
    fn fmt_epoch_short_known_dates() {
        assert_eq!(fmt_epoch_short(0.0), "1970-01-01");
        assert_eq!(fmt_epoch_short(86_400.0), "1970-01-02");
        assert_eq!(fmt_epoch_short(1_718_150_400.0), "2024-06-12");
        // millisecond payloads are normalized
        assert_eq!(fmt_epoch_short(1_718_150_400_000.0), "2024-06-12");
        assert_eq!(fmt_epoch_short(-1.0), "—");
        assert_eq!(fmt_epoch_short(f64::NAN), "—");
    }

    #[test]
    fn snapshot_label_includes_index_signal_and_date() {
        let h = sample_history();
        assert_eq!(snapshot_label(&h, 0), "#0 · 7616 · 2024-06-12");
        // missing snapshot falls back to em-dash signal, no date
        assert_eq!(snapshot_label(&h, 7), "#7 · —");
    }

    #[test]
    fn fmt_signed_signs() {
        assert_eq!(fmt_signed(0.034, 3), "+0.034");
        assert_eq!(fmt_signed(-0.5, 2), "-0.50");
        assert_eq!(fmt_signed(0.0, 2), "0.00");
        assert_eq!(fmt_signed(2.0, 0), "+2");
        assert_eq!(fmt_signed_i64(212), "+212");
        assert_eq!(fmt_signed_i64(-34), "-34");
        assert_eq!(fmt_signed_i64(0), "0");
    }
}
