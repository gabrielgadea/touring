//! /dashboard — Elite editorial overview (SPEC W2 §5.1).
//!
//! Every value rendered here comes from a live endpoint:
//! `/api/quality/signal` (hero composite), `/api/quality/history`
//! (trend + delta) and `/api/status` (KPIs, CEG pipeline counters,
//! system gauges). Panels with no real data source are omitted by
//! policy — there is deliberately no EventRibbon on this page because
//! no endpoint provides per-event timestamps.

use leptos::prelude::*;
use leptos_meta::Title;
use serde_json::Value;

use crate::web::components::{
    AreaChart, AreaSeries, KpiCell, KpiStrip, MiniBars, PageHero, Panel, PipelineStages, Stage,
    StageState,
};
use crate::web::ctx::use_refresh_bus;
use crate::web::services::{fetch_quality_history, fetch_quality_signal, fetch_status};

// ─── pure helpers (unit-tested) ──────────────────────────────────────

/// Greeting segment for the hero title, derived from a 0–23 hour.
pub fn greeting_for_hour(h: u32) -> &'static str {
    if h < 12 {
        "morning"
    } else if h < 18 {
        "afternoon"
    } else {
        "evening"
    }
}

/// Local-time greeting. Uses the browser clock on wasm32; native builds
/// (SSR/cargo check/tests only — the app is CSR) fall back to "afternoon".
fn local_greeting() -> &'static str {
    #[cfg(target_arch = "wasm32")]
    {
        greeting_for_hour(js_sys::Date::new_0().get_hours())
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        greeting_for_hour(15)
    }
}

/// Chronological composite series from `/api/quality/history`.
/// The API returns snapshots newest-first; we reverse so the chart
/// plots oldest → newest.
pub fn history_series(history: &Value) -> Vec<f64> {
    let mut pts: Vec<f64> = history
        .get("snapshots")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|s| s.get("signal_0_10000").and_then(Value::as_i64))
                .map(|v| v as f64)
                .collect()
        })
        .unwrap_or_default();
    pts.reverse();
    pts
}

/// Delta between the two most recent snapshots (newest-first payload):
/// `Some(current − previous)` when ≥2 snapshots exist.
pub fn signal_delta(history: &Value) -> Option<i64> {
    let snaps = history.get("snapshots")?.as_array()?;
    let cur = snaps.first()?.get("signal_0_10000")?.as_i64()?;
    let prev = snaps.get(1)?.get("signal_0_10000")?.as_i64()?;
    Some(cur - prev)
}

/// Format a delta for the hero stat ("+212" / "-34" / "0" / "—").
pub fn format_delta(delta: Option<i64>) -> String {
    match delta {
        Some(d) if d > 0 => format!("+{d}"),
        Some(d) => format!("{d}"),
        None => "—".to_string(),
    }
}

/// Format a µs latency: ≥1000 µs renders as ms ("1.5 ms"), else "420 µs".
pub fn fmt_us(us: f64) -> String {
    if us >= 1000.0 {
        format!("{:.1} ms", us / 1000.0)
    } else {
        format!("{us:.0} µs")
    }
}

/// CEG pipeline stages (X0..X7 surface view) derived from real
/// `gate_metrics` counters. CAPTURE..PREDICT are Done once anything was
/// captured; SANDBOX (and the capability GATE that follows it) once
/// anything was sandboxed; DECIDE warns when blocks were recorded.
/// With zero captures the whole pipeline is honestly Pending.
pub fn ceg_stages(gate_metrics: &Value) -> Vec<Stage> {
    let count = |k: &str| gate_metrics.get(k).and_then(Value::as_i64).unwrap_or(0);
    let captured = count("ceg_captured_count");
    let sandboxed = count("ceg_sandboxed_count");
    let blocked = count("ceg_blocked_count");

    let early = if captured > 0 {
        StageState::Done
    } else {
        StageState::Pending
    };
    let sandbox = if sandboxed > 0 {
        StageState::Done
    } else {
        StageState::Pending
    };
    let decide = if blocked > 0 {
        StageState::Warn
    } else if captured > 0 {
        StageState::Done
    } else {
        StageState::Pending
    };

    vec![
        Stage {
            label: "CAPTURE",
            count: Some(captured.to_string()),
            state: early,
        },
        Stage {
            label: "CLASSIFY",
            count: None,
            state: early,
        },
        Stage {
            label: "STATIC",
            count: None,
            state: early,
        },
        Stage {
            label: "VGP",
            count: None,
            state: early,
        },
        Stage {
            label: "PREDICT",
            count: None,
            state: early,
        },
        Stage {
            label: "SANDBOX",
            count: Some(sandboxed.to_string()),
            state: sandbox,
        },
        Stage {
            label: "GATE",
            count: None,
            state: sandbox,
        },
        Stage {
            label: "DECIDE",
            count: Some(blocked.to_string()),
            state: decide,
        },
    ]
}

/// Hook-dispatch percentile bars `[p50, p90, p99, p999]` in µs.
/// Returns fewer than 4 entries (treated as "no data") when the
/// histogram is absent or partial.
pub fn latency_bars(gate_metrics: &Value) -> Vec<f64> {
    let Some(lat) = gate_metrics.get("hook_dispatch_latency") else {
        return Vec::new();
    };
    ["p50_us", "p90_us", "p99_us", "p999_us"]
        .iter()
        .filter_map(|k| lat.get(*k).and_then(Value::as_f64))
        .collect()
}

/// One actionable card in the "Next moves" panel.
#[derive(Debug, Clone, PartialEq)]
pub struct NextMove {
    /// Card headline (includes the real count).
    pub title: String,
    /// One-line supporting copy.
    pub body: String,
    /// CTA target route.
    pub href: &'static str,
    /// CTA label.
    pub cta: &'static str,
}

/// Derive up to 3 next moves from the real `/api/status` payload:
/// dependency cycles → /quality, orphan symbols → /orphans, degraded
/// daemon components → /health. Empty when everything is clear.
pub fn next_moves(status: &Value) -> Vec<NextMove> {
    let mut moves = Vec::new();

    let cycles = status
        .pointer("/quality_signal/raw/cycle_count")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    if cycles > 0 {
        moves.push(NextMove {
            title: format!("Resolve {cycles} dependency cycles"),
            body: "Tarjan SCC found circular module dependencies — breaking them lifts the Acyclicity axis of the composite signal.".to_string(),
            href: "/quality",
            cta: "Quality",
        });
    }

    let orphans = status
        .pointer("/wiring/orphan_count")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    if orphans > 0 {
        moves.push(NextMove {
            title: format!("Wire {orphans} orphan symbols"),
            body: "Public symbols with zero consumers — wire them to callers or retire them."
                .to_string(),
            href: "/orphans",
            cta: "Orphans",
        });
    }

    let healthy = status
        .pointer("/daemon_health/healthy_count")
        .and_then(Value::as_i64);
    let total = status
        .pointer("/daemon_health/total_count")
        .and_then(Value::as_i64);
    if let (Some(h), Some(t)) = (healthy, total) {
        if h < t {
            moves.push(NextMove {
                title: format!("Inspect daemon health ({h}/{t})"),
                body: "One or more daemon components report degraded — drill into the health page before stateful work.".to_string(),
                href: "/health",
                cta: "Health",
            });
        }
    }

    moves.truncate(3);
    moves
}

// ─── page ────────────────────────────────────────────────────────────

/// `/dashboard` — Elite editorial overview composed exclusively from
/// live endpoint data (hero composite, signal trend, next moves, KPI
/// strip, CEG pipeline and system gauges).
#[component]
pub fn Dashboard() -> impl IntoView {
    let bus = use_refresh_bus();

    let signal_res = LocalResource::new(move || {
        let _tick = bus.tick.get();
        async move { fetch_quality_signal(None, false).await.ok() }
    });
    let history_res = LocalResource::new(move || {
        let _tick = bus.tick.get();
        async move { fetch_quality_history(30).await.ok() }
    });
    let status_res = LocalResource::new(move || {
        let _tick = bus.tick.get();
        async move { fetch_status().await.ok() }
    });

    // Hero stat: composite from /api/quality/signal; delta vs the
    // previous history snapshot. "—" until the first fetch lands.
    let hero_stat = Signal::derive(move || {
        signal_res
            .get()
            .flatten()
            .and_then(|v| v.get("signal_0_10000").and_then(Value::as_i64))
            .map(|n| n.to_string())
            .unwrap_or_else(|| "—".to_string())
    });
    let hero_delta = Signal::derive(move || {
        format_delta(history_res.get().flatten().as_ref().and_then(signal_delta))
    });

    // KPI strip — all four cells sourced from /api/status.
    let kpi_health = Signal::derive(move || {
        status_res
            .get()
            .flatten()
            .and_then(|v| v.pointer("/composite_health_score").and_then(Value::as_f64))
            .map(|f| format!("{f:.2}"))
            .unwrap_or_else(|| "—".to_string())
    });
    let kpi_symbols = Signal::derive(move || {
        status_res
            .get()
            .flatten()
            .and_then(|v| v.pointer("/index/symbol_count").and_then(Value::as_i64))
            .map(|n| n.to_string())
            .unwrap_or_else(|| "—".to_string())
    });
    let kpi_orphans = Signal::derive(move || {
        status_res
            .get()
            .flatten()
            .and_then(|v| v.pointer("/wiring/orphan_count").and_then(Value::as_i64))
            .map(|n| n.to_string())
            .unwrap_or_else(|| "—".to_string())
    });
    let kpi_ema = Signal::derive(move || {
        status_res
            .get()
            .flatten()
            .and_then(|v| v.pointer("/learning/ema_reward").and_then(Value::as_f64))
            .map(|f| format!("{f:.3}"))
            .unwrap_or_else(|| "—".to_string())
    });

    view! {
        <div class="page pg-dashboard">
            <Title text="Touring — Overview"/>
            <div class="el-main-editorial">
                <PageHero
                    eyebrow="TOURING · OVERVIEW"
                    title="Good"
                    title_em=local_greeting()
                    sub="The workspace at a glance — composite quality signal, wiring state and the CEG execution pipeline, all read live from the running daemon."
                    stat=hero_stat
                    stat_label="COMPOSITE"
                    stat_denom="/ 10 000"
                    stat_delta=hero_delta
                >
                    <a class="el-btn" href="/quality">"Quality"</a>
                    <a class="el-btn" href="/workspace">"Atlas"</a>
                    <a class="el-btn el-btn-primary" href="/orphans">"Orphans"</a>
                </PageHero>

                <div class="el-eyebrow pg-dashboard-kpi-title">"The state of things"</div>
                <KpiStrip>
                    <KpiCell label="Composite health" value=kpi_health sub="daemon composite [0–1]"/>
                    <KpiCell label="Symbols" value=kpi_symbols sub="indexed in workspace"/>
                    <KpiCell label="Orphans" value=kpi_orphans sub="pub without consumers"/>
                    <KpiCell label="EMA reward" value=kpi_ema sub="RL learning loop"/>
                </KpiStrip>

                <div class="pg-dashboard-cols">
                    <div class="pg-dashboard-main">
                        <Panel num="01" eyebrow="Signal evolution" cmd="touring quality history">
                            <Suspense fallback=|| view! { <div class="el-skeleton" aria-label="loading"></div> }>
                                {move || status_chart(history_res.get())}
                            </Suspense>
                        </Panel>

                        <Panel num="02" eyebrow="Next moves" cmd="touring status -j">
                            <Suspense fallback=|| view! { <div class="el-skeleton" aria-label="loading"></div> }>
                                {move || moves_panel(status_res.get())}
                            </Suspense>
                        </Panel>

                        <Panel num="03" eyebrow="Pipeline" cmd="touring gate-metrics -j">
                            <Suspense fallback=|| view! { <div class="el-skeleton" aria-label="loading"></div> }>
                                {move || pipeline_panel(status_res.get())}
                            </Suspense>
                        </Panel>
                    </div>

                    <div class="pg-dashboard-side">
                        <Panel eyebrow="System" cmd="touring status -j">
                            <Suspense fallback=|| view! { <div class="el-skeleton" aria-label="loading"></div> }>
                                {move || system_panel(status_res.get())}
                            </Suspense>
                        </Panel>

                        <Panel eyebrow="Latency" cmd="hook_dispatch_latency">
                            <Suspense fallback=|| view! { <div class="el-skeleton" aria-label="loading"></div> }>
                                {move || latency_panel(status_res.get())}
                            </Suspense>
                        </Panel>
                    </div>
                </div>
            </div>
        </div>
    }
}

// ─── panel renderers (small, honest: real data or el-empty) ──────────

/// Panel 01 body — AreaChart of the chronological composite series, or
/// an honest empty state when fewer than two snapshots exist.
fn status_chart(res: Option<Option<Value>>) -> impl IntoView {
    res.map(|h| {
        let pts = h.as_ref().map(history_series).unwrap_or_default();
        if pts.len() >= 2 {
            let series = vec![AreaSeries {
                label: "Composite",
                color: "var(--el-accent)",
                points: pts,
                dashed: false,
                area: true,
            }];
            view! {
                <AreaChart series=Signal::derive(move || series.clone())/>
            }
            .into_any()
        } else {
            view! {
                <div class="el-empty">
                    "Not enough history yet — at least two snapshots are required to draw a trend."
                </div>
            }
            .into_any()
        }
    })
}

/// Panel 02 body — up to 3 cards derived from real status data, the
/// all-clear state, or an honest unavailable state when the fetch failed.
fn moves_panel(res: Option<Option<Value>>) -> impl IntoView {
    res.map(|st| match st {
        None => view! {
            <div class="el-empty">"Status endpoint unavailable — no moves can be derived."</div>
        }
        .into_any(),
        Some(v) => {
            let moves = next_moves(&v);
            if moves.is_empty() {
                view! { <div class="el-empty">"All clear — no pending moves."</div> }.into_any()
            } else {
                view! {
                    <div class="pg-dashboard-moves">
                        {moves
                            .into_iter()
                            .map(|m| view! {
                                <div class="el-card pg-dashboard-move">
                                    <div class="pg-dashboard-move-title">{m.title}</div>
                                    <p class="pg-dashboard-move-body">{m.body}</p>
                                    <div>
                                        <a class="el-btn el-btn-sm" href=m.href>{m.cta}</a>
                                    </div>
                                </div>
                            })
                            .collect_view()}
                    </div>
                }
                .into_any()
            }
        }
    })
}

/// Panel 03 body — CEG pipeline stages from real gate_metrics counters.
fn pipeline_panel(res: Option<Option<Value>>) -> impl IntoView {
    res.map(|st| match st.as_ref().and_then(|v| v.get("gate_metrics")) {
        Some(gm) => {
            let stages = ceg_stages(gm);
            view! {
                <PipelineStages stages=Signal::derive(move || stages.clone())/>
            }
            .into_any()
        }
        None => view! {
            <div class="el-empty">"Gate metrics unavailable — CEG pipeline state cannot be shown."</div>
        }
        .into_any(),
    })
}

/// System panel body — only rows whose counters are actually present.
fn system_panel(res: Option<Option<Value>>) -> impl IntoView {
    res.map(|st| match st.as_ref().and_then(|v| v.get("gate_metrics")) {
        Some(gm) => {
            let rows: Vec<(&'static str, String)> = [
                (
                    "Memory RSS",
                    gm.get("memory_rss_mb")
                        .and_then(Value::as_f64)
                        .map(|v| format!("{v:.0} MB")),
                ),
                (
                    "Hook p50",
                    gm.pointer("/hook_dispatch_latency/p50_us")
                        .and_then(Value::as_f64)
                        .map(fmt_us),
                ),
                (
                    "Hook p99",
                    gm.pointer("/hook_dispatch_latency/p99_us")
                        .and_then(Value::as_f64)
                        .map(fmt_us),
                ),
                (
                    "Query cache hit",
                    gm.get("query_cache_hit_ratio")
                        .and_then(Value::as_f64)
                        .map(|r| format!("{:.0}%", r * 100.0)),
                ),
                (
                    "Enrichment emits",
                    gm.get("enrichment_emit_count")
                        .and_then(Value::as_i64)
                        .map(|n| n.to_string()),
                ),
            ]
            .into_iter()
            .filter_map(|(label, val)| val.map(|v| (label, v)))
            .collect();

            if rows.is_empty() {
                view! { <div class="el-empty">"No system counters reported."</div> }.into_any()
            } else {
                view! {
                    <div class="pg-dashboard-sys">
                        {rows
                            .into_iter()
                            .map(|(label, val)| view! {
                                <div class="el-row pg-dashboard-sysrow">
                                    <span class="pg-dashboard-syslabel">{label}</span>
                                    <span class="el-mono">{val}</span>
                                </div>
                            })
                            .collect_view()}
                    </div>
                }
                .into_any()
            }
        }
        None => view! { <div class="el-empty">"Status endpoint unavailable."</div> }.into_any(),
    })
}

/// Latency panel body — MiniBars of the four real hook-dispatch
/// percentiles, or an honest empty state when the histogram is absent.
fn latency_panel(res: Option<Option<Value>>) -> impl IntoView {
    res.map(|st| {
        let bars = st
            .as_ref()
            .and_then(|v| v.get("gate_metrics"))
            .map(latency_bars)
            .unwrap_or_default();
        if bars.len() == 4 {
            view! {
                <div class="pg-dashboard-latency">
                    <MiniBars values=Signal::derive(move || bars.clone())/>
                    <div class="pg-dashboard-latency-label">
                        "hook dispatch percentiles (µs) — p50 · p90 · p99 · p999"
                    </div>
                </div>
            }
            .into_any()
        } else {
            view! { <div class="el-empty">"No latency histogram reported."</div> }.into_any()
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn greeting_boundaries() {
        assert_eq!(greeting_for_hour(0), "morning");
        assert_eq!(greeting_for_hour(11), "morning");
        assert_eq!(greeting_for_hour(12), "afternoon");
        assert_eq!(greeting_for_hour(17), "afternoon");
        assert_eq!(greeting_for_hour(18), "evening");
        assert_eq!(greeting_for_hour(23), "evening");
    }

    #[test]
    fn history_series_reverses_to_chronological() {
        let h = json!({"snapshots": [
            {"signal_0_10000": 7616}, {"signal_0_10000": 7404}, {"signal_0_10000": 7100}
        ]});
        assert_eq!(history_series(&h), vec![7100.0, 7404.0, 7616.0]);
    }

    #[test]
    fn history_series_empty_on_malformed_payload() {
        assert!(history_series(&json!({"error": "boom"})).is_empty());
        assert!(history_series(&json!({"snapshots": "nope"})).is_empty());
    }

    #[test]
    fn signal_delta_and_format() {
        let h = json!({"snapshots": [{"signal_0_10000": 7616}, {"signal_0_10000": 7404}]});
        assert_eq!(signal_delta(&h), Some(212));
        assert_eq!(
            signal_delta(&json!({"snapshots": [{"signal_0_10000": 7616}]})),
            None
        );
        assert_eq!(format_delta(Some(212)), "+212");
        assert_eq!(format_delta(Some(-34)), "-34");
        assert_eq!(format_delta(Some(0)), "0");
        assert_eq!(format_delta(None), "—");
    }

    #[test]
    fn fmt_us_switches_to_ms_at_1000() {
        assert_eq!(fmt_us(420.0), "420 µs");
        assert_eq!(fmt_us(999.0), "999 µs");
        assert_eq!(fmt_us(1000.0), "1.0 ms");
        assert_eq!(fmt_us(1500.0), "1.5 ms");
    }

    #[test]
    fn ceg_stages_all_pending_when_uncaptured() {
        let gm = json!({
            "ceg_captured_count": 0, "ceg_sandboxed_count": 0, "ceg_blocked_count": 0
        });
        let stages = ceg_stages(&gm);
        assert_eq!(stages.len(), 8);
        assert!(stages.iter().all(|s| s.state == StageState::Pending));
    }

    #[test]
    fn ceg_stages_done_and_warn_with_real_counters() {
        let gm = json!({
            "ceg_captured_count": 12, "ceg_sandboxed_count": 4, "ceg_blocked_count": 2
        });
        let stages = ceg_stages(&gm);
        assert_eq!(stages[0].label, "CAPTURE");
        assert_eq!(stages[0].state, StageState::Done);
        assert_eq!(stages[4].state, StageState::Done); // PREDICT
        assert_eq!(stages[5].state, StageState::Done); // SANDBOX
        assert_eq!(stages[6].state, StageState::Done); // GATE follows SANDBOX
        assert_eq!(stages[7].state, StageState::Warn); // DECIDE warns on blocks
        assert_eq!(stages[0].count.as_deref(), Some("12"));
        assert_eq!(stages[5].count.as_deref(), Some("4"));
        assert_eq!(stages[7].count.as_deref(), Some("2"));
    }

    #[test]
    fn ceg_stages_decide_done_without_blocks() {
        let gm = json!({
            "ceg_captured_count": 7, "ceg_sandboxed_count": 0, "ceg_blocked_count": 0
        });
        let stages = ceg_stages(&gm);
        assert_eq!(stages[5].state, StageState::Pending); // SANDBOX never ran
        assert_eq!(stages[7].state, StageState::Done); // DECIDE clean
    }

    #[test]
    fn latency_bars_extracts_four_percentiles() {
        let gm = json!({"hook_dispatch_latency": {
            "count": 10, "p50_us": 120.0, "p90_us": 300.0, "p99_us": 900.0,
            "p999_us": 2400.0, "max_us": 9000.0
        }});
        assert_eq!(latency_bars(&gm), vec![120.0, 300.0, 900.0, 2400.0]);
        assert!(latency_bars(&json!({})).is_empty());
    }

    #[test]
    fn next_moves_empty_when_all_clear() {
        let st = json!({
            "quality_signal": {"raw": {"cycle_count": 0}},
            "wiring": {"orphan_count": 0},
            "daemon_health": {"healthy_count": 5, "total_count": 5}
        });
        assert!(next_moves(&st).is_empty());
    }

    #[test]
    fn next_moves_derives_all_three_cards() {
        let st = json!({
            "quality_signal": {"raw": {"cycle_count": 3}},
            "wiring": {"orphan_count": 17},
            "daemon_health": {"healthy_count": 4, "total_count": 5}
        });
        let moves = next_moves(&st);
        assert_eq!(moves.len(), 3);
        assert!(moves[0].title.contains("3 dependency cycles"));
        assert_eq!(moves[0].href, "/quality");
        assert!(moves[1].title.contains("17 orphan symbols"));
        assert_eq!(moves[1].href, "/orphans");
        assert!(moves[2].title.contains("4/5"));
        assert_eq!(moves[2].href, "/health");
    }
}
