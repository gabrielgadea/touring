//! /quality — Sentrux signal detail. Elite W1 pilot page
//! (SPEC 2026-06-12 §5.2): composed hero stat, warn-halo radar,
//! real trend from /api/quality/history, ProgressTrack rows and the
//! closed `.el-*`/`.pg-quality-*` class set (the `ql-*` dialect died
//! with this migration).

use crate::web::components::{KpiCell, KpiStrip, PageHero, Panel, ProgressTrack, RadarChart};
use crate::web::ctx::use_refresh_bus;
use crate::web::models::quality::WorkspaceQualitySignal;
use crate::web::services::{fetch_json, fetch_quality_signal};
use leptos::prelude::*;
use leptos_meta::Title;

/// Real trend from the two most recent history snapshots:
/// `Some(delta)` when ≥2 snapshots exist (current − previous).
pub fn trend_delta(history: &serde_json::Value) -> Option<i64> {
    let snaps = history.get("snapshots")?.as_array()?;
    let cur = snaps.first()?.get("signal_0_10000")?.as_i64()?;
    let prev = snaps.get(1)?.get("signal_0_10000")?.as_i64()?;
    Some(cur - prev)
}

/// Format a trend delta for the KPI cell ("+212" / "-34" / "0").
pub fn format_trend(delta: Option<i64>) -> String {
    match delta {
        Some(d) if d > 0 => format!("+{d}"),
        Some(d) => format!("{d}"),
        None => "—".to_string(),
    }
}

/// `/quality` — full Sentrux signal detail view.
#[component]
pub fn QualityDetail() -> impl IntoView {
    let bus = use_refresh_bus();
    let res = LocalResource::new(move || {
        let _tick = bus.tick.get();
        async move {
            let raw = fetch_quality_signal(None, false).await.ok();
            raw.and_then(|v| serde_json::from_value::<WorkspaceQualitySignal>(v).ok())
                .unwrap_or_default()
        }
    });
    let history = LocalResource::new(move || {
        let _tick = bus.tick.get();
        async move { fetch_json("/api/quality/history?limit=2").await.ok() }
    });

    view! {
        <div class="page pg-quality">
            <Title text="Touring — Quality"/>
            <Suspense fallback=|| view! { <div class="el-skeleton" aria-label="loading"></div> }>
                {move || res.get().map(|sig| {
                    let scores = sig.root_causes.clone();
                    let bot = sig.bottleneck.clone();
                    let bot_radar = bot.clone();
                    let bot_kpi = bot.clone();
                    let bot_hint = bot.clone();

                    let composite_str = format!("{}", sig.signal_0_10000);
                    let cycles_str = format!("{} cycles", sig.raw.cycle_count);
                    let cycle_count = sig.raw.cycle_count;

                    // Per-axis rows: (label, axis_key, description, score, raw column).
                    let rows: Vec<(&'static str, &'static str, String, f64, String)> = vec![
                        ("Modularity", "modularity",
                         format!("Newman/Leiden Q={:.3}", sig.raw.modularity_q),
                         sig.root_causes.modularity,
                         format!("{:.3}", sig.raw.modularity_q)),
                        ("Acyclicity", "acyclicity",
                         "Tarjan SCC".to_string(),
                         sig.root_causes.acyclicity,
                         format!("{} cycles", sig.raw.cycle_count)),
                        ("Depth", "depth",
                         "max call-graph".to_string(),
                         sig.root_causes.depth,
                         format!("depth {}", sig.raw.max_depth)),
                        ("Equality", "equality",
                         "CC distribution".to_string(),
                         sig.root_causes.equality,
                         format!("{:.3} Gini", sig.raw.complexity_gini)),
                        ("Redundancy", "redundancy",
                         "structural copies".to_string(),
                         sig.root_causes.redundancy,
                         format!("{:.1}%", sig.raw.redundancy_ratio * 100.0)),
                    ];

                    view! {
                        <div class="el-main-editorial">
                            <PageHero
                                eyebrow="SENTRUX · COMPOSITE SIGNAL"
                                title="Quality"
                                title_em="signal"
                                sub="Five normalized axes, weighted into a 0–10,000 score. The lowest axis is the dominant bottleneck — where the next refactor has the most leverage."
                                stat=Signal::derive(move || composite_str.clone())
                                stat_label="COMPOSITE"
                                stat_denom="/ 10 000"
                                stat_delta=Signal::derive(move || {
                                    format_trend(history.get().flatten().as_ref().and_then(trend_delta))
                                })
                            >
                                <a class="el-btn" href="/quality/rules">"Rules"</a>
                                <a class="el-btn" href="/quality/diff">"Diff"</a>
                                <a class="el-btn el-btn-primary" href="/orphans">"Orphans"</a>
                            </PageHero>

                            <KpiStrip>
                                <KpiCell
                                    label="Composite"
                                    value=Signal::derive({
                                        let v = format!("{}", sig.signal_0_10000);
                                        move || v.clone()
                                    })
                                    sub="of 10,000"
                                />
                                <KpiCell
                                    label="Bottleneck"
                                    value=Signal::derive(move || bot_kpi.clone())
                                    sub="lowest axis"
                                />
                                <KpiCell
                                    label="Trend"
                                    value=Signal::derive(move || {
                                        format_trend(history.get().flatten().as_ref().and_then(trend_delta))
                                    })
                                    sub="vs. previous snapshot"
                                />
                                <KpiCell
                                    label="Acyclicity raw"
                                    value=Signal::derive(move || cycles_str.clone())
                                    sub="Tarjan SCC"
                                />
                            </KpiStrip>

                            <section class="pg-quality-grid">
                                <Panel num="01" eyebrow="Radar · 5 axes" cmd="normalized [0, 1]">
                                    <div class="pg-quality-radar">
                                        <RadarChart
                                            scores=Signal::derive(move || scores.clone())
                                            bottleneck=Signal::derive(move || bot_radar.clone())
                                        />
                                    </div>
                                </Panel>

                                <div class="pg-quality-right">
                                    <Panel num="02" eyebrow="By axis" cmd="touring quality signal -j">
                                        <table class="el-table">
                                            <tbody>
                                                {rows.into_iter().map(|(label, key, desc, score, raw)| {
                                                    let is_bot = bot == key;
                                                    let color = if is_bot { "var(--el-warn)" } else { "var(--el-accent)" };
                                                    view! {
                                                        <tr>
                                                            <td>
                                                                <div class="pg-quality-axis-name">
                                                                    {label}
                                                                    {is_bot.then(|| view! {
                                                                        <span class="el-tag el-tag-warn">"Bottleneck"</span>
                                                                    })}
                                                                </div>
                                                                <div class="pg-quality-axis-desc">{desc}</div>
                                                            </td>
                                                            <td class="pg-quality-prog-col">
                                                                <ProgressTrack
                                                                    value=Signal::derive(move || score)
                                                                    color=color
                                                                    show_value=true
                                                                />
                                                            </td>
                                                            <td class="el-mono pg-quality-raw">{raw}</td>
                                                        </tr>
                                                    }
                                                }).collect_view()}
                                            </tbody>
                                        </table>
                                    </Panel>

                                    {if cycle_count > 0 {
                                        view! {
                                            <div class="el-card pg-quality-suggest">
                                                <div class="el-eyebrow" style="color:var(--el-accent);">"Suggested action · LinUCB"</div>
                                                <p class="pg-quality-suggest-body">
                                                    "Resolving the "
                                                    <strong style="color:var(--el-warn);">{format!("{cycle_count} cycles")}</strong>
                                                    " detected by Tarjan SCC would lift Acyclicity and add points to the composite signal."
                                                </p>
                                                <div class="el-hero-actions">
                                                    <a class="el-btn el-btn-sm" href="/wiring">"Wiring modules"</a>
                                                    <a class="el-btn el-btn-sm" href="/chains">"Chains"</a>
                                                    <a class="el-btn el-btn-primary el-btn-sm" href="/orphans">"Orphans"</a>
                                                </div>
                                            </div>
                                        }.into_any()
                                    } else {
                                        view! {
                                            <div class="el-card pg-quality-suggest">
                                                <div class="el-eyebrow" style="color:var(--el-pos);">"No critical cycles detected"</div>
                                                <p class="pg-quality-suggest-body">
                                                    "Bottleneck: "
                                                    <strong>{bot_hint.clone()}</strong>
                                                    " — review the axis table above for improvement opportunities."
                                                </p>
                                            </div>
                                        }.into_any()
                                    }}
                                </div>
                            </section>
                        </div>
                    }
                })}
            </Suspense>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trend_delta_from_two_snapshots() {
        let h = serde_json::json!({"snapshots": [
            {"signal_0_10000": 7616}, {"signal_0_10000": 7404}
        ]});
        assert_eq!(trend_delta(&h), Some(212));
    }

    #[test]
    fn trend_delta_none_with_single_snapshot() {
        let h = serde_json::json!({"snapshots": [{"signal_0_10000": 7616}]});
        assert_eq!(trend_delta(&h), None);
    }

    #[test]
    fn trend_delta_none_on_malformed_payload() {
        assert_eq!(trend_delta(&serde_json::json!({"error": "boom"})), None);
    }

    #[test]
    fn format_trend_signs() {
        assert_eq!(format_trend(Some(212)), "+212");
        assert_eq!(format_trend(Some(-34)), "-34");
        assert_eq!(format_trend(Some(0)), "0");
        assert_eq!(format_trend(None), "—");
    }
}
