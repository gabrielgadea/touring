//! /cognitive — cognitive engines, Elite re-skin (SPEC 2026-06-12 §5.16,
//! predictive layer). Real data only — panels without a real source are
//! OMITTED (honesty policy).
//!
//! Shapes captured live on 2026-06-12 (`curl http://localhost:3000/...`):
//!
//! - `GET /api/cognitive` (`touring cognitive metrics`) →
//!   `{"has_graph":true,"has_predictor":true,"initialized":true}`
//! - `GET /api/cognitive/engines` (`touring cognitive engines`) →
//!   `{"cognitive_runtime":{"graph":"healthy","status":"healthy"},
//!     "crdt_graph":{"status":"healthy"},
//!     "online_rl":{"status":"healthy"},
//!     "predictor":{"status":"healthy"}}`
//!
//! OMITTED panels (no real source in either payload): MCTS tree
//! visualization, GoT expansion graph, prediction probabilities and
//! bandit arm weights — the endpoints expose no such structure, so
//! nothing is simulated. Engines are iterated generically (`as_object`)
//! so engines the daemon adds later appear without client changes.
//! CSS block: `.pg-cognitive-*` (composes with `el-*` primitives).

use leptos::prelude::*;
use leptos_meta::Title;
use serde_json::Value;

use crate::web::components::{KpiCell, KpiStrip, PageHero, Panel, ProgressTrack};
use crate::web::ctx::use_refresh_bus;
use crate::web::services::{fetch_cognitive, fetch_cognitive_engines};

/// One classified engine-field value, derived from the live JSON shape.
#[derive(Debug, Clone, PartialEq)]
pub enum RowValue {
    /// Float in `[0, 1]` → rendered as a [`ProgressTrack`].
    Ratio(f64),
    /// Integer or out-of-range number → rendered as mono text.
    Count(String),
    /// String / bool / nested value → rendered as plain text.
    Text(String),
}

/// Classify one JSON value into its row rendering kind.
///
/// Live shape today only carries strings (`"graph":"healthy"`), but the
/// daemon may add numeric fields later: floats in `[0,1]` become
/// progress tracks, other numbers become mono counts.
pub fn classify_value(v: &Value) -> RowValue {
    match v {
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                RowValue::Count(i.to_string())
            } else if let Some(f) = n.as_f64() {
                if (0.0..=1.0).contains(&f) {
                    RowValue::Ratio(f)
                } else {
                    RowValue::Count(format!("{f:.2}"))
                }
            } else {
                RowValue::Text(n.to_string())
            }
        }
        Value::String(s) => RowValue::Text(s.clone()),
        Value::Bool(b) => RowValue::Text(b.to_string()),
        other => RowValue::Text(other.to_string()),
    }
}

/// All renderable fields of one engine entry, `status` excluded (it is
/// rendered in the panel head). Bare-string engines yield no rows.
pub fn engine_rows(v: &Value) -> Vec<(String, RowValue)> {
    v.as_object()
        .map(|o| {
            o.iter()
                .filter(|(k, _)| k.as_str() != "status")
                .map(|(k, val)| (k.clone(), classify_value(val)))
                .collect()
        })
        .unwrap_or_default()
}

/// Humanize a snake_case engine key: `cognitive_runtime` → "Cognitive
/// Runtime". Known daemon acronyms are upper-cased (`crdt`, `rl`, …).
pub fn humanize_engine(key: &str) -> String {
    key.split('_')
        .filter(|w| !w.is_empty())
        .map(|w| match w {
            "crdt" => "CRDT".to_string(),
            "rl" => "RL".to_string(),
            "mcts" => "MCTS".to_string(),
            "got" => "GoT".to_string(),
            _ => {
                let mut chars = w.chars();
                match chars.next() {
                    Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
                    None => String::new(),
                }
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Status string of one engine entry — tolerates both
/// `{"status":"healthy", …}` objects and bare `"healthy"` strings.
pub fn engine_status(v: &Value) -> String {
    v.get("status")
        .and_then(Value::as_str)
        .or_else(|| v.as_str())
        .unwrap_or("unknown")
        .to_string()
}

/// Count `(healthy, total)` engines; `None` when the payload is not an
/// object (fetch failure / not yet loaded).
pub fn healthy_count(engines: &Value) -> Option<(usize, usize)> {
    engines.as_object().map(|map| {
        let total = map.len();
        let healthy = map
            .values()
            .filter(|v| engine_status(v) == "healthy")
            .count();
        (healthy, total)
    })
}

/// Color token for a status word (panel head + status-like row values).
pub fn status_color(status: &str) -> &'static str {
    match status {
        "healthy" | "ok" | "active" | "true" => "var(--el-pos)",
        "degraded" | "warning" | "stale" => "var(--el-warn)",
        "error" | "unhealthy" | "down" | "failed" => "var(--el-neg)",
        _ => "var(--el-fg-4)",
    }
}

/// Elite panel numbering for the Nth engine ("01".."16"; "—" beyond —
/// [`Panel`]'s `num` prop is `&'static str`, so dynamic `format!` is out).
pub fn panel_num(i: usize) -> &'static str {
    const NUMS: [&str; 16] = [
        "01", "02", "03", "04", "05", "06", "07", "08", "09", "10", "11", "12", "13", "14", "15",
        "16",
    ];
    NUMS.get(i).copied().unwrap_or("—")
}

/// KPI label for one `/api/cognitive` boolean: `yes_word` when true,
/// "no" when false, "—" when the field is absent (honest, no guessing).
pub fn bool_label(meta: &Value, key: &str, yes_word: &str) -> String {
    match meta.get(key).and_then(Value::as_bool) {
        Some(true) => yes_word.to_string(),
        Some(false) => "no".to_string(),
        None => "—".to_string(),
    }
}

/// Render one classified engine field as an `.pg-cognitive-row`.
fn render_row(key: String, value: RowValue) -> AnyView {
    match value {
        RowValue::Ratio(x) => view! {
            <div class="pg-cognitive-row">
                <span class="pg-cognitive-row-key">{key}</span>
                <div class="pg-cognitive-row-track">
                    <ProgressTrack value=Signal::derive(move || x) show_value=true/>
                </div>
            </div>
        }
        .into_any(),
        RowValue::Count(s) => view! {
            <div class="pg-cognitive-row">
                <span class="pg-cognitive-row-key">{key}</span>
                <span class="el-mono pg-cognitive-row-val">{s}</span>
            </div>
        }
        .into_any(),
        RowValue::Text(s) => {
            let color = status_color(&s);
            view! {
                <div class="pg-cognitive-row">
                    <span class="pg-cognitive-row-key">{key}</span>
                    <span class="pg-cognitive-row-val" style=format!("color:{color}")>{s}</span>
                </div>
            }
            .into_any()
        }
    }
}

/// `/cognitive` — cognitive engines health page (Elite re-skin).
///
/// Layout: PageHero (live healthy-engine stat) · KpiStrip (4 real
/// values) · one Panel per engine reported by the live payload · honest
/// `el-empty` when the endpoint does not answer.
#[component]
pub fn CognitiveEngines() -> impl IntoView {
    let bus = use_refresh_bus();
    let engines = LocalResource::new(move || {
        let _tick = bus.tick.get();
        async move { fetch_cognitive_engines().await.ok() }
    });
    let meta = LocalResource::new(move || {
        let _tick = bus.tick.get();
        async move { fetch_cognitive().await.ok() }
    });

    // /api/cognitive booleans → KPI labels ("—" until/unless they arrive).
    let meta_val = move || meta.get().flatten().unwrap_or(Value::Null);
    let graph_kpi = Signal::derive(move || bool_label(&meta_val(), "has_graph", "attached"));
    let predictor_kpi = Signal::derive(move || bool_label(&meta_val(), "has_predictor", "loaded"));
    let init_kpi = Signal::derive(move || bool_label(&meta_val(), "initialized", "yes"));

    view! {
        <div class="page pg-cognitive">
            <Title text="Touring — Cognitive"/>
            <Suspense fallback=|| view! { <div class="el-skeleton" aria-label="loading"></div> }>
                {move || engines.get().map(|maybe| {
                    let hc = maybe.as_ref().and_then(healthy_count);
                    let healthy_stat = Signal::derive(move || {
                        hc.map(|(h, _)| h.to_string()).unwrap_or_else(|| "—".to_string())
                    });
                    let ratio_stat = Signal::derive(move || {
                        hc.map(|(h, t)| format!("{h} / {t}")).unwrap_or_else(|| "—".to_string())
                    });

                    // One panel per REAL engine in the payload; honest
                    // el-empty when the endpoint failed or reported none.
                    let body = match maybe.as_ref().and_then(Value::as_object).filter(|m| !m.is_empty()) {
                        Some(map) => {
                            let panels = map.iter().enumerate().map(|(i, (key, val))| {
                                let name = humanize_engine(key);
                                let status = engine_status(val);
                                let color = status_color(&status);
                                let rows = engine_rows(val);
                                view! {
                                    <Panel eyebrow="ENGINE" num=panel_num(i) cmd="touring cognitive engines">
                                        <div class="pg-cognitive-engine">
                                            <div class="pg-cognitive-engine-head">
                                                <span class="pg-cognitive-engine-name">{name}</span>
                                                <span
                                                    class="pg-cognitive-status"
                                                    style=format!("color:{color}")
                                                >
                                                    {status}
                                                </span>
                                            </div>
                                            {if rows.is_empty() {
                                                view! {
                                                    <p class="pg-cognitive-note">
                                                        "Only the status field is reported for this engine."
                                                    </p>
                                                }
                                                .into_any()
                                            } else {
                                                view! {
                                                    <div class="pg-cognitive-rows">
                                                        {rows
                                                            .into_iter()
                                                            .map(|(k, rv)| render_row(k, rv))
                                                            .collect_view()}
                                                    </div>
                                                }
                                                .into_any()
                                            }}
                                        </div>
                                    </Panel>
                                }
                            }).collect_view();
                            view! { <section class="pg-cognitive-grid">{panels}</section> }.into_any()
                        }
                        None => view! {
                            <div class="el-empty">
                                "Cognitive endpoints unavailable — touring cognitive engines did not answer (or reported no engines). Nothing is simulated."
                            </div>
                        }
                        .into_any(),
                    };

                    view! {
                        <div class="el-main-editorial">
                            <PageHero
                                eyebrow="ORCHESTRATION · COGNITIVE"
                                title="Cognitive"
                                title_em="engines"
                                sub="Live health of the daemon's cognitive stack — reasoning runtime, CRDT knowledge graph, online RL bandit and session predictor — exactly as touring cognitive engines reports it. Fields the API does not expose are not drawn."
                                stat=healthy_stat
                                stat_label="ENGINES HEALTHY"
                            >
                                <a class="el-btn" href="/health">"Daemon health"</a>
                            </PageHero>

                            // All four KPI values come from the live payloads.
                            <KpiStrip>
                                <KpiCell label="Healthy" value=ratio_stat sub="engines reporting healthy"/>
                                <KpiCell label="Graph" value=graph_kpi sub="knowledge graph handle"/>
                                <KpiCell label="Predictor" value=predictor_kpi sub="session predictor handle"/>
                                <KpiCell label="Initialized" value=init_kpi sub="cognitive runtime booted"/>
                            </KpiStrip>

                            {body}
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
    use serde_json::json;

    /// The exact live shape captured on 2026-06-12.
    fn live_engines() -> Value {
        json!({
            "cognitive_runtime": {"graph": "healthy", "status": "healthy"},
            "crdt_graph": {"status": "healthy"},
            "online_rl": {"status": "healthy"},
            "predictor": {"status": "healthy"}
        })
    }

    #[test]
    fn humanize_engine_known_keys() {
        assert_eq!(humanize_engine("cognitive_runtime"), "Cognitive Runtime");
        assert_eq!(humanize_engine("crdt_graph"), "CRDT Graph");
        assert_eq!(humanize_engine("online_rl"), "Online RL");
        assert_eq!(humanize_engine("predictor"), "Predictor");
    }

    #[test]
    fn engine_status_object_string_and_missing() {
        assert_eq!(engine_status(&json!({"status": "healthy"})), "healthy");
        assert_eq!(engine_status(&json!("degraded")), "degraded");
        assert_eq!(engine_status(&json!({"graph": "healthy"})), "unknown");
    }

    #[test]
    fn healthy_count_on_live_shape() {
        assert_eq!(healthy_count(&live_engines()), Some((4, 4)));
        assert_eq!(healthy_count(&Value::Null), None);
        let mut degraded = live_engines();
        degraded["online_rl"] = json!({"status": "degraded"});
        assert_eq!(healthy_count(&degraded), Some((3, 4)));
    }

    #[test]
    fn engine_rows_excludes_status_keeps_extras() {
        let rows = engine_rows(&json!({"graph": "healthy", "status": "healthy"}));
        assert_eq!(
            rows,
            vec![("graph".to_string(), RowValue::Text("healthy".to_string()))]
        );
        // Status-only engine → no extra rows; bare string → no rows.
        assert!(engine_rows(&json!({"status": "healthy"})).is_empty());
        assert!(engine_rows(&json!("healthy")).is_empty());
    }

    #[test]
    fn classify_value_ratio_count_text() {
        assert_eq!(classify_value(&json!(0.42)), RowValue::Ratio(0.42));
        assert_eq!(classify_value(&json!(7)), RowValue::Count("7".to_string()));
        assert_eq!(
            classify_value(&json!(3.5)),
            RowValue::Count("3.50".to_string())
        );
        assert_eq!(
            classify_value(&json!("healthy")),
            RowValue::Text("healthy".to_string())
        );
        assert_eq!(
            classify_value(&json!(true)),
            RowValue::Text("true".to_string())
        );
    }

    #[test]
    fn bool_label_yes_no_missing() {
        let meta = json!({"has_graph": true, "has_predictor": false});
        assert_eq!(bool_label(&meta, "has_graph", "attached"), "attached");
        assert_eq!(bool_label(&meta, "has_predictor", "loaded"), "no");
        assert_eq!(bool_label(&meta, "initialized", "yes"), "—");
        assert_eq!(bool_label(&Value::Null, "has_graph", "attached"), "—");
    }

    #[test]
    fn panel_num_bounds() {
        assert_eq!(panel_num(0), "01");
        assert_eq!(panel_num(3), "04");
        assert_eq!(panel_num(15), "16");
        assert_eq!(panel_num(16), "—");
    }

    #[test]
    fn status_color_tokens() {
        assert_eq!(status_color("healthy"), "var(--el-pos)");
        assert_eq!(status_color("degraded"), "var(--el-warn)");
        assert_eq!(status_color("error"), "var(--el-neg)");
        assert_eq!(status_color("whatever"), "var(--el-fg-4)");
    }
}
