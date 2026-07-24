//! /health — daemon vital signs. Elite W2 page (SPEC W2 §5.6):
//! doctor component table, dispatch latency percentiles, RL drift and
//! runtime counters — every value read live from `/api/*`; panels
//! without a real source render an honest empty state, never mocks.

use crate::web::components::{KpiCell, KpiStrip, MiniBars, PageHero, Panel, ProgressTrack};
use crate::web::ctx::use_refresh_bus;
use crate::web::services::{fetch_gate_metrics, fetch_health, fetch_learning_status, fetch_status};
use leptos::prelude::*;
use leptos_meta::Title;

/// Percentile bars `[p50, p90, p99, p999]` (µs) for the histogram named
/// `{prefix}_latency` inside the flat gate-metrics dict.
/// Returns `None` when the histogram is absent or has `count == 0`
/// (honest omission — no samples means no row).
pub fn latency_bars(gates: &serde_json::Value, prefix: &str) -> Option<Vec<f64>> {
    let hist = gates.get(format!("{prefix}_latency"))?;
    let count = hist.get("count")?.as_f64()?;
    if count <= 0.0 {
        return None;
    }
    Some(vec![
        hist.get("p50_us")?.as_f64()?,
        hist.get("p90_us")?.as_f64()?,
        hist.get("p99_us")?.as_f64()?,
        hist.get("p999_us")?.as_f64()?,
    ])
}

/// Human latency: `< 1000 µs` renders as integer µs, above as `%.1f ms`.
pub fn fmt_latency_us(us: f64) -> String {
    if us < 1000.0 {
        format!("{} µs", us.round() as i64)
    } else {
        format!("{:.1} ms", us / 1000.0)
    }
}

/// Rows `(name, status, detail)` from the `/api/health` top-level ARRAY
/// (`touring doctor -j` shape). Malformed entries are skipped; a
/// non-array payload yields an empty vec (honest empty table).
pub fn component_rows(health: &serde_json::Value) -> Vec<(String, String, String)> {
    health
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|c| {
                    let name = c.get("name")?.as_str()?.to_string();
                    let status = c
                        .get("status")
                        .and_then(|s| s.as_str())
                        .unwrap_or("unknown")
                        .to_string();
                    let detail = c
                        .get("detail")
                        .and_then(|s| s.as_str())
                        .unwrap_or_default()
                        .to_string();
                    Some((name, status, detail))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Char-safe truncation with ellipsis (UTF-8 boundary safe).
pub fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let head: String = s.chars().take(max).collect();
        format!("{head}…")
    }
}

/// Ratio `[0, 1]` → integer percent string ("58%").
pub fn fmt_ratio_pct(ratio: f64) -> String {
    format!("{:.0}%", ratio * 100.0)
}

/// `/health` — daemon vital signs dashboard.
#[component]
pub fn HealthDashboard() -> impl IntoView {
    let bus = use_refresh_bus();

    let doctor = LocalResource::new(move || {
        let _tick = bus.tick.get();
        async move { fetch_health().await.ok() }
    });
    let status = LocalResource::new(move || {
        let _tick = bus.tick.get();
        async move { fetch_status().await.ok() }
    });
    let gates = LocalResource::new(move || {
        let _tick = bus.tick.get();
        async move { fetch_gate_metrics().await.ok() }
    });
    let learning = LocalResource::new(move || {
        let _tick = bus.tick.get();
        async move { fetch_learning_status().await.ok() }
    });

    // ── Hero stat + KPI signals (render "—" until each fetch resolves) ──
    let composite_stat = Signal::derive(move || {
        status
            .get()
            .flatten()
            .and_then(|v| {
                v.pointer("/composite_health_score")
                    .and_then(|x| x.as_f64())
            })
            .map(|f| format!("{f:.2}"))
            .unwrap_or_else(|| "—".to_string())
    });
    let daemon_kpi = Signal::derive(move || {
        status
            .get()
            .flatten()
            .and_then(|v| {
                let healthy = v.pointer("/daemon_health/healthy_count")?.as_i64()?;
                let total = v.pointer("/daemon_health/total_count")?.as_i64()?;
                Some(format!("{healthy}/{total}"))
            })
            .unwrap_or_else(|| "—".to_string())
    });
    let sessions_kpi = Signal::derive(move || {
        status
            .get()
            .flatten()
            .and_then(|v| v.pointer("/sessions/count").and_then(|x| x.as_i64()))
            .map(|n| n.to_string())
            .unwrap_or_else(|| "—".to_string())
    });
    let cache_kpi = Signal::derive(move || {
        gates
            .get()
            .flatten()
            .and_then(|v| v.pointer("/query_cache_hit_ratio").and_then(|x| x.as_f64()))
            .map(fmt_ratio_pct)
            .unwrap_or_else(|| "—".to_string())
    });
    let hook_p50_kpi = Signal::derive(move || {
        gates
            .get()
            .flatten()
            .and_then(|v| {
                v.pointer("/hook_dispatch_latency/p50_us")
                    .and_then(|x| x.as_f64())
            })
            .map(fmt_latency_us)
            .unwrap_or_else(|| "—".to_string())
    });

    view! {
        <div class="page pg-health">
            <Title text="Touring — Health"/>
            <div class="el-main-editorial">
                <PageHero
                    eyebrow="DIAGNOSTICS · DAEMON"
                    title="Health"
                    title_em="signals"
                    sub="Doctor components, dispatch latency percentiles, RL drift and runtime counters — read live from the daemon. Panels without a source stay empty."
                    stat=composite_stat
                    stat_label="COMPOSITE"
                >
                    <a class="el-btn" href="/hooks">"Hooks"</a>
                    <a class="el-btn" href="/settings">"Settings"</a>
                </PageHero>

                <KpiStrip>
                    <KpiCell label="Daemon" value=daemon_kpi sub="components healthy"/>
                    <KpiCell label="Sessions" value=sessions_kpi sub="tracked"/>
                    <KpiCell label="Cache hit" value=cache_kpi sub="query cache"/>
                    <KpiCell label="Hook P50" value=hook_p50_kpi sub="dispatch latency"/>
                </KpiStrip>

                // ── 01 · doctor component table ──────────────────────────
                <Panel num="01" eyebrow="Components" cmd="touring doctor -j">
                    <Suspense fallback=|| view! { <div class="el-skeleton" aria-label="loading"></div> }>
                        {move || doctor.get().map(|maybe| {
                            let rows = maybe.as_ref().map(component_rows).unwrap_or_default();
                            if rows.is_empty() {
                                view! {
                                    <p class="pg-health-empty">
                                        "Doctor report unavailable — daemon unreachable or empty payload."
                                    </p>
                                }.into_any()
                            } else {
                                view! {
                                    <table class="el-table pg-health-comp-table">
                                        <thead>
                                            <tr>
                                                <th>"Name"</th>
                                                <th>"Status"</th>
                                                <th>"Detail"</th>
                                            </tr>
                                        </thead>
                                        <tbody>
                                            {rows.into_iter().map(|(name, st, detail)| {
                                                let tag_cls = if st == "ok" {
                                                    "el-tag el-tag-pos"
                                                } else {
                                                    "el-tag el-tag-neg"
                                                };
                                                let short = truncate_chars(&detail, 80);
                                                view! {
                                                    <tr>
                                                        <td class="pg-health-comp-name">{name}</td>
                                                        <td><span class=tag_cls>{st}</span></td>
                                                        <td class="el-mono pg-health-comp-detail" title=detail.clone()>
                                                            {short}
                                                        </td>
                                                    </tr>
                                                }
                                            }).collect_view()}
                                        </tbody>
                                    </table>
                                }.into_any()
                            }
                        })}
                    </Suspense>
                </Panel>

                <section class="pg-health-grid">
                    // ── 02 · latency percentiles ──────────────────────────
                    <Panel num="02" eyebrow="Latency percentiles" cmd="touring gate-metrics -j">
                        <Suspense fallback=|| view! { <div class="el-skeleton" aria-label="loading"></div> }>
                            {move || gates.get().map(|maybe| {
                                let g = maybe.unwrap_or(serde_json::Value::Null);
                                let rows: Vec<(&'static str, Vec<f64>)> = [
                                    ("Hook dispatch", "hook_dispatch"),
                                    ("rkyv dispatch", "rkyv_dispatch"),
                                    ("ANN search", "ann_search"),
                                    ("Tantivy query", "tantivy_query"),
                                ]
                                .into_iter()
                                .filter_map(|(label, prefix)| {
                                    latency_bars(&g, prefix).map(|bars| (label, bars))
                                })
                                .collect();

                                if rows.is_empty() {
                                    view! {
                                        <p class="pg-health-empty">
                                            "No latency samples recorded in this daemon session."
                                        </p>
                                    }.into_any()
                                } else {
                                    view! {
                                        <div class="pg-health-lat">
                                            {rows.into_iter().map(|(label, bars)| {
                                                let p99 = bars.get(2).copied().unwrap_or(0.0);
                                                let vals = bars.clone();
                                                view! {
                                                    <div class="pg-health-lat-row">
                                                        <span class="pg-health-lat-label">{label}</span>
                                                        <MiniBars values=Signal::derive(move || vals.clone())/>
                                                        <span class="el-mono pg-health-lat-p99">
                                                            {format!("p99 {}", fmt_latency_us(p99))}
                                                        </span>
                                                    </div>
                                                }
                                            }).collect_view()}
                                            <div class="pg-health-lat-legend el-mono">"p50 · p90 · p99 · p999"</div>
                                        </div>
                                    }.into_any()
                                }
                            })}
                        </Suspense>
                    </Panel>

                    // ── 03 · learning drift ───────────────────────────────
                    <Panel num="03" eyebrow="Learning drift" cmd="touring learning status">
                        <Suspense fallback=|| view! { <div class="el-skeleton" aria-label="loading"></div> }>
                            {move || learning.get().map(|maybe| match maybe {
                                Some(v) => {
                                    let ema = v.pointer("/ema_reward").and_then(|x| x.as_f64()).unwrap_or(0.0);
                                    let ema_clamped = ema.clamp(0.0, 1.0);
                                    let td = v.pointer("/mean_td_error").and_then(|x| x.as_f64());
                                    let updates = v.pointer("/update_count").and_then(|x| x.as_i64());
                                    let bandit = v.pointer("/bandit_type").and_then(|x| x.as_str())
                                        .unwrap_or("unknown").to_string();
                                    let arms = v.pointer("/arm_count").and_then(|x| x.as_i64()).unwrap_or(0);
                                    let td_elevated = td.is_some_and(|t| t > 100.0);

                                    view! {
                                        <div class="pg-health-rows">
                                            <div class="el-row pg-health-row">
                                                <span class="pg-health-row-label">"EMA reward"</span>
                                                <div class="pg-health-row-track">
                                                    <ProgressTrack
                                                        value=Signal::derive(move || ema_clamped)
                                                        color="var(--el-accent)"
                                                        show_value=true
                                                    />
                                                </div>
                                            </div>
                                            <div class="el-row pg-health-row">
                                                <span class="pg-health-row-label">"TD error"</span>
                                                <span class="el-mono">
                                                    {td.map(|t| format!("{t:.2}")).unwrap_or_else(|| "—".to_string())}
                                                </span>
                                            </div>
                                            {td_elevated.then(|| view! {
                                                <div class="el-banner el-banner-warn">
                                                    "TD error elevated — bandit still converging"
                                                </div>
                                            })}
                                            <div class="el-row pg-health-row">
                                                <span class="pg-health-row-label">"Updates"</span>
                                                <span class="el-mono">
                                                    {updates.map(|u| u.to_string()).unwrap_or_else(|| "—".to_string())}
                                                </span>
                                            </div>
                                            <div class="el-row pg-health-row">
                                                <span class="pg-health-row-label">"Bandit"</span>
                                                <span class="el-tag el-tag-on">{format!("{bandit} · {arms} arms")}</span>
                                            </div>
                                        </div>
                                    }.into_any()
                                }
                                None => view! {
                                    <p class="pg-health-empty">"Learning status unavailable."</p>
                                }.into_any(),
                            })}
                        </Suspense>
                    </Panel>

                    // ── 04 · runtime counters ─────────────────────────────
                    <Panel num="04" eyebrow="Runtime">
                        <Suspense fallback=|| view! { <div class="el-skeleton" aria-label="loading"></div> }>
                            {move || gates.get().map(|maybe| match maybe {
                                Some(g) => {
                                    let rss = g.pointer("/memory_rss_mb").and_then(|x| x.as_f64());
                                    let captured = g.pointer("/ceg_captured_count")
                                        .and_then(|x| x.as_i64()).unwrap_or(0);
                                    let blocked = g.pointer("/ceg_blocked_count")
                                        .and_then(|x| x.as_i64()).unwrap_or(0);
                                    let fast = g.pointer("/pre_edit_fast_path")
                                        .and_then(|x| x.as_f64()).unwrap_or(0.0);
                                    let full = g.pointer("/pre_edit_full")
                                        .and_then(|x| x.as_f64()).unwrap_or(0.0);
                                    let reindex_fail = g.pointer("/reindex_failure_count")
                                        .and_then(|x| x.as_i64()).unwrap_or(0);
                                    let fast_pct = if fast + full > 0.0 {
                                        format!("{:.0}%", fast / (fast + full) * 100.0)
                                    } else {
                                        "—".to_string()
                                    };
                                    let blocked_style = if blocked > 0 { "color:var(--el-warn)" } else { "" };

                                    view! {
                                        <div class="pg-health-rows">
                                            <div class="el-row pg-health-row">
                                                <span class="pg-health-row-label">"Memory RSS"</span>
                                                <span class="el-mono">
                                                    {rss.map(|r| format!("{r:.0} MB")).unwrap_or_else(|| "—".to_string())}
                                                </span>
                                            </div>
                                            <div class="el-row pg-health-row">
                                                <span class="pg-health-row-label">"CEG captured / blocked"</span>
                                                <span class="el-mono">
                                                    {format!("{captured} captured · ")}
                                                    <span style=blocked_style>{format!("{blocked} blocked")}</span>
                                                </span>
                                            </div>
                                            <div class="el-row pg-health-row">
                                                <span class="pg-health-row-label">"Pre-edit fast-path"</span>
                                                <span class="el-mono">{fast_pct}</span>
                                            </div>
                                            <div class="el-row pg-health-row">
                                                <span class="pg-health-row-label">"Reindex failures"</span>
                                                {if reindex_fail > 0 {
                                                    view! {
                                                        <span class="el-tag el-tag-neg">{reindex_fail.to_string()}</span>
                                                    }.into_any()
                                                } else {
                                                    view! { <span class="el-tag el-tag-pos">"0"</span> }.into_any()
                                                }}
                                            </div>
                                        </div>
                                    }.into_any()
                                }
                                None => view! {
                                    <p class="pg-health-empty">"Gate metrics unavailable."</p>
                                }.into_any(),
                            })}
                        </Suspense>
                    </Panel>
                </section>
            </div>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn gates_fixture() -> serde_json::Value {
        json!({
            "hook_dispatch_latency": {
                "count": 412, "p50_us": 850.0, "p90_us": 2100.0,
                "p99_us": 5300.0, "p999_us": 9100.0, "max_us": 12000.0
            },
            "ann_search_latency": {
                "count": 0, "p50_us": 0.0, "p90_us": 0.0,
                "p99_us": 0.0, "p999_us": 0.0, "max_us": 0.0
            }
        })
    }

    #[test]
    fn latency_bars_extracts_four_percentiles() {
        let bars = latency_bars(&gates_fixture(), "hook_dispatch");
        assert_eq!(bars, Some(vec![850.0, 2100.0, 5300.0, 9100.0]));
    }

    #[test]
    fn latency_bars_none_when_count_zero() {
        assert_eq!(latency_bars(&gates_fixture(), "ann_search"), None);
    }

    #[test]
    fn latency_bars_none_when_histogram_missing() {
        assert_eq!(latency_bars(&gates_fixture(), "tantivy_query"), None);
        assert_eq!(
            latency_bars(&json!({"error": "boom"}), "hook_dispatch"),
            None
        );
    }

    #[test]
    fn fmt_latency_us_switches_unit_at_1ms() {
        assert_eq!(fmt_latency_us(850.0), "850 µs");
        assert_eq!(fmt_latency_us(0.0), "0 µs");
        assert_eq!(fmt_latency_us(5300.0), "5.3 ms");
        assert_eq!(fmt_latency_us(1000.0), "1.0 ms");
    }

    #[test]
    fn component_rows_parses_doctor_array() {
        let payload = json!([
            {"name": "daemon_socket", "status": "ok", "detail": "/tmp/touring-daemon-1000.sock"},
            {"name": "circuit_breaker", "status": "open", "detail": "tripped"},
            {"status": "ok", "detail": "missing name is skipped"}
        ]);
        let rows = component_rows(&payload);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].0, "daemon_socket");
        assert_eq!(rows[0].1, "ok");
        assert_eq!(rows[1].1, "open");
    }

    #[test]
    fn component_rows_empty_on_non_array() {
        assert!(component_rows(&json!({"name": "not-an-array"})).is_empty());
        assert!(component_rows(&json!(null)).is_empty());
    }

    #[test]
    fn truncate_chars_is_utf8_safe() {
        assert_eq!(truncate_chars("short", 80), "short");
        let long = "x".repeat(100);
        let cut = truncate_chars(&long, 80);
        assert_eq!(cut.chars().count(), 81); // 80 chars + ellipsis
        assert!(cut.ends_with('…'));
        // multi-byte boundary safety
        assert_eq!(truncate_chars("áéíóú", 3), "áéí…");
    }

    #[test]
    fn fmt_ratio_pct_scales_unit_ratio() {
        assert_eq!(fmt_ratio_pct(0.58), "58%");
        assert_eq!(fmt_ratio_pct(0.0), "0%");
        assert_eq!(fmt_ratio_pct(1.0), "100%");
    }
}
