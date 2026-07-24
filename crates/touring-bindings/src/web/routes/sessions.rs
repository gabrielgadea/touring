//! /sessions — orchestration history with traceable detail (Elite W3,
//! SPEC 2026-06-12 §5.15): selectable session list (left), on-demand
//! `touring session assess <id>` detail (right) and the global
//! reinforcement learner state below.
//!
//! Data sources (REAL shapes, captured 2026-06-12):
//! - `GET /api/sessions` → `{"count":20,"sessions":[{"session_id":
//!   "gen-3b5c4696-…","objective":"Python Script","task_type":
//!   "generator","created_at":"2026-06-06T03:57:02.904+00:00",
//!   "updated_at":ISO}]}`
//! - `GET /api/sessions/{id}` (shells `touring session assess`) →
//!   `{"assessed_at":ISO,"health_summary":"DEGRADED 0.53 [wiring:0.53]
//!   36ms","metrics":{"bash_commands_last_hour":0,"edits_last_hour":0,
//!   "gotcha_hits_total":82426},"quality_score":0.0,"session_id":…}`
//! - `GET /api/learning/status` → `{update_count, ema_reward,
//!   mean_td_error, linucb_loaded, bandit_type, arm_count}`
//!
//! Honesty policy (SPEC §5.15): the assess payload exposes no
//! per-event timestamps, so this page renders NO hook EventRibbon —
//! only the fields the CLI actually returns. The learner panel is
//! labelled "global learner state" because RL is workspace-global,
//! never per-session. Timestamps are formatted by fixed-position
//! slicing of the RFC 3339 prefix — no chrono dependency.

use leptos::prelude::*;
use leptos_meta::Title;

use crate::web::components::{PageHero, Panel, ProgressTrack};
use crate::web::ctx::use_refresh_bus;
use crate::web::services::{fetch_learning_status, fetch_session_detail, fetch_sessions};

/// "2026-06-06T03:57:02.904179368+00:00" → "06/06 03:57".
///
/// Fixed-position slice of the RFC 3339 prefix (`YYYY-MM-DDTHH:MM`).
/// Returns "—" for anything that does not match that shape, so a
/// malformed timestamp never panics or renders garbage.
pub fn fmt_iso_short(iso: &str) -> String {
    let b = iso.as_bytes();
    if b.len() < 16
        || !b[..16].is_ascii()
        || b[4] != b'-'
        || b[7] != b'-'
        || b[10] != b'T'
        || b[13] != b':'
    {
        return "\u{2014}".to_string();
    }
    format!(
        "{}/{} {}:{}",
        &iso[8..10],
        &iso[5..7],
        &iso[11..13],
        &iso[14..16]
    )
}

/// Color token for a `health_summary` string ("DEGRADED 0.53 …").
///
/// DEGRADED wins over everything (it is the loud state); HEALTHY/OK
/// map to the positive token; anything else stays foreground-neutral.
pub fn health_color(summary: &str) -> &'static str {
    if summary.contains("DEGRADED") {
        "var(--el-warn)"
    } else if summary.contains("HEALTHY") || summary.contains("OK") {
        "var(--el-pos)"
    } else {
        "var(--el-fg)"
    }
}

/// Extract `(session_id, objective, task_type, created_at)` tuples from
/// the real `/api/sessions` payload. Missing string fields become empty
/// strings; a payload without a `sessions` array yields an empty vec
/// (rendered as an honest empty state, never mocked rows).
pub fn parse_sessions(v: &serde_json::Value) -> Vec<(String, String, String, String)> {
    v.get("sessions")
        .and_then(|s| s.as_array())
        .map(|arr| {
            arr.iter()
                .map(|s| {
                    let g = |k: &str| s.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();
                    (
                        g("session_id"),
                        g("objective"),
                        g("task_type"),
                        g("created_at"),
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Group an integer with thousands separators ("82426" → "82,426") —
/// matches the "of 10,000" comma convention used across elite pages.
pub fn fmt_thousands(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// Truncate to at most `max` chars, appending an ellipsis when cut.
/// Char-based (not byte-based) so multi-byte content never panics.
fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let cut: String = s.chars().take(max).collect();
        format!("{cut}…")
    }
}

/// Compact mono rendering for the TD error — scientific notation once
/// the magnitude stops being readable as a plain decimal.
fn fmt_td_error(t: f64) -> String {
    if t.abs() >= 1000.0 {
        format!("{t:.2e}")
    } else {
        format!("{t:.2}")
    }
}

/// One assessment detail block — only the fields the assess payload
/// actually carries (no event ribbon: the CLI exposes no per-event
/// timestamps, per the SPEC §5.15 honesty clause).
fn render_assessment(v: &serde_json::Value) -> AnyView {
    let summary = v
        .pointer("/health_summary")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string();
    let color = health_color(&summary);
    let bash = v
        .pointer("/metrics/bash_commands_last_hour")
        .and_then(|x| x.as_u64());
    let edits = v
        .pointer("/metrics/edits_last_hour")
        .and_then(|x| x.as_u64());
    let gotchas = v
        .pointer("/metrics/gotcha_hits_total")
        .and_then(|x| x.as_u64());
    let quality = v.pointer("/quality_score").and_then(|x| x.as_f64());
    let assessed = v
        .pointer("/assessed_at")
        .and_then(|x| x.as_str())
        .map(fmt_iso_short);

    let dash = || "\u{2014}".to_string();

    view! {
        <div class="pg-sessions-assess">
            {(!summary.is_empty()).then(|| view! {
                <div
                    class="el-stat pg-sessions-health"
                    style=format!("color:{color}")
                >
                    {summary.clone()}
                </div>
            })}
            <div class="pg-sessions-rows">
                <div class="el-row pg-sessions-row">
                    <span class="pg-sessions-row-label">"Bash commands (last hour)"</span>
                    <span class="el-mono">{bash.map(fmt_thousands).unwrap_or_else(dash)}</span>
                </div>
                <div class="el-row pg-sessions-row">
                    <span class="pg-sessions-row-label">"Edits (last hour)"</span>
                    <span class="el-mono">{edits.map(fmt_thousands).unwrap_or_else(dash)}</span>
                </div>
                <div class="el-row pg-sessions-row">
                    <span class="pg-sessions-row-label">"Gotcha hits (total)"</span>
                    <span class="el-mono">{gotchas.map(fmt_thousands).unwrap_or_else(dash)}</span>
                </div>
                <div class="el-row pg-sessions-row">
                    <span class="pg-sessions-row-label">"Quality score"</span>
                    <div class="pg-sessions-row-track">
                        {match quality {
                            Some(q) => {
                                let qc = q.clamp(0.0, 1.0);
                                view! {
                                    <ProgressTrack
                                        value=Signal::derive(move || qc)
                                        show_value=true
                                    />
                                }.into_any()
                            }
                            None => view! {
                                <span class="el-mono">"\u{2014}"</span>
                            }.into_any(),
                        }}
                    </div>
                </div>
                <div class="el-row pg-sessions-row">
                    <span class="pg-sessions-row-label">"Assessed at"</span>
                    <span class="el-mono">{assessed.unwrap_or_else(dash)}</span>
                </div>
            </div>
        </div>
    }
    .into_any()
}

/// `/sessions` — traceable orchestration history with on-demand assess
/// detail and the global learner state (SPEC §5.15).
#[component]
pub fn SessionsList() -> impl IntoView {
    let bus = use_refresh_bus();
    let selected_id: RwSignal<Option<String>> = RwSignal::new(None);

    // Session list — refreshes with the global bus cadence.
    let sessions = LocalResource::new(move || {
        let _tick = bus.tick.get();
        async move { fetch_sessions().await.ok() }
    });

    // Assess detail — re-runs whenever the selection changes (the
    // signal is read OUTSIDE the async block so it is tracked).
    let detail = LocalResource::new(move || {
        let id = selected_id.get();
        async move {
            match id {
                Some(i) => fetch_session_detail(&i).await.ok(),
                None => None,
            }
        }
    });

    // Global RL learner state — refreshes with the bus cadence.
    let learning = LocalResource::new(move || {
        let _tick = bus.tick.get();
        async move { fetch_learning_status().await.ok() }
    });

    let count = Signal::derive(move || {
        sessions
            .get()
            .flatten()
            .as_ref()
            .and_then(|j| {
                j.get("count")
                    .and_then(|v| v.as_u64())
                    .map(|n| n.to_string())
                    .or_else(|| {
                        j.get("sessions")
                            .and_then(|v| v.as_array())
                            .map(|a| a.len().to_string())
                    })
            })
            .unwrap_or_else(|| "\u{2014}".to_string())
    });

    view! {
        <div class="page pg-sessions">
            <Title text="Touring — Sessions"/>
            <div class="el-main-editorial">
                <PageHero
                    eyebrow="ORCHESTRATION · SESSIONS"
                    title="Sessions"
                    title_em="history"
                    sub="Every agent run the daemon registered. Pick a session on the left to run its live assessment — health summary, activity metrics and quality score straight from touring session assess."
                    stat=count
                    stat_label="TRACKED"
                >
                    <code class="el-cmd-label">"touring session"</code>
                </PageHero>

                <section class="pg-sessions-grid">
                    // ── 01 · session list ────────────────────────────
                    <Panel eyebrow="Sessions" num="01" cmd="touring session">
                        <Suspense fallback=|| view! {
                            <div class="el-skeleton" aria-label="loading"></div>
                        }>
                            {move || sessions.get().map(|payload| match payload {
                                None => view! {
                                    <div class="el-empty">
                                        "Sessions endpoint unavailable — history cannot be shown."
                                    </div>
                                }.into_any(),
                                Some(json) => {
                                    let items = parse_sessions(&json);
                                    if items.is_empty() {
                                        view! {
                                            <div class="el-empty">
                                                "No sessions registered — "
                                                <code>"touring session start <id> <type> \"<objective>\""</code>
                                            </div>
                                        }.into_any()
                                    } else {
                                        view! {
                                            <div class="pg-sessions-list">
                                                {items.into_iter().map(|(sid, objective, task_type, created)| {
                                                    let sid_short = truncate_chars(&sid, 18);
                                                    let created_short = fmt_iso_short(&created);
                                                    let objective_disp = if objective.trim().is_empty() {
                                                        "\u{2014}".to_string()
                                                    } else {
                                                        objective
                                                    };
                                                    let sid_click = sid.clone();
                                                    let sid_active = sid;
                                                    view! {
                                                        <div
                                                            class="el-card pg-sessions-item"
                                                            class:active=move || {
                                                                selected_id.get().as_deref()
                                                                    == Some(sid_active.as_str())
                                                            }
                                                            on:click=move |_| {
                                                                selected_id.set(Some(sid_click.clone()))
                                                            }
                                                        >
                                                            <div class="pg-sessions-item-top">
                                                                <span class="pg-sessions-item-objective">
                                                                    {objective_disp}
                                                                </span>
                                                                <span class="el-tag">{task_type}</span>
                                                            </div>
                                                            <div class="pg-sessions-item-meta">
                                                                <span class="el-mono pg-sessions-item-sid">
                                                                    {sid_short}
                                                                </span>
                                                                <span class="pg-sessions-item-ts">
                                                                    {created_short}
                                                                </span>
                                                            </div>
                                                        </div>
                                                    }
                                                }).collect_view()}
                                            </div>
                                        }.into_any()
                                    }
                                }
                            })}
                        </Suspense>
                    </Panel>

                    // ── 02 · assess detail ───────────────────────────
                    <Panel eyebrow="Assessment" num="02" cmd="touring session assess <id>">
                        {move || match selected_id.get() {
                            None => view! {
                                <div class="el-empty">"Select a session to assess it."</div>
                            }.into_any(),
                            Some(_) => view! {
                                <Suspense fallback=|| view! {
                                    <div class="el-skeleton" aria-label="loading"></div>
                                }>
                                    {move || detail.get().map(|d| match d {
                                        Some(v) => render_assessment(&v),
                                        None => view! {
                                            <div class="el-empty">
                                                "Assessment unavailable — touring session assess returned no data for this session."
                                            </div>
                                        }.into_any(),
                                    })}
                                </Suspense>
                            }.into_any(),
                        }}
                    </Panel>
                </section>

                // ── 03 · global learner state ────────────────────────
                <Panel eyebrow="Reinforcement" num="03" cmd="touring learning status">
                    <Suspense fallback=|| view! {
                        <div class="el-skeleton" aria-label="loading"></div>
                    }>
                        {move || learning.get().map(|l| match l {
                            Some(v) => {
                                let ema = v.pointer("/ema_reward")
                                    .and_then(|x| x.as_f64())
                                    .unwrap_or(0.0);
                                let ema_clamped = ema.clamp(0.0, 1.0);
                                let updates = v.pointer("/update_count").and_then(|x| x.as_u64());
                                let td = v.pointer("/mean_td_error").and_then(|x| x.as_f64());
                                let bandit = v.pointer("/bandit_type")
                                    .and_then(|x| x.as_str())
                                    .unwrap_or("unknown")
                                    .to_string();
                                let arms = v.pointer("/arm_count")
                                    .and_then(|x| x.as_u64())
                                    .unwrap_or(0);
                                view! {
                                    <div class="pg-sessions-rl">
                                        <div class="el-eyebrow pg-sessions-rl-note">
                                            "global learner state — RL is workspace-wide, not per-session"
                                        </div>
                                        <div class="pg-sessions-rows">
                                            <div class="el-row pg-sessions-row">
                                                <span class="pg-sessions-row-label">"EMA reward"</span>
                                                <div class="pg-sessions-row-track">
                                                    <ProgressTrack
                                                        value=Signal::derive(move || ema_clamped)
                                                        show_value=true
                                                    />
                                                </div>
                                            </div>
                                            <div class="el-row pg-sessions-row">
                                                <span class="pg-sessions-row-label">"Updates"</span>
                                                <span class="el-mono">
                                                    {updates.map(fmt_thousands)
                                                        .unwrap_or_else(|| "\u{2014}".to_string())}
                                                </span>
                                            </div>
                                            <div class="el-row pg-sessions-row">
                                                <span class="pg-sessions-row-label">"TD error"</span>
                                                <span class="el-mono">
                                                    {td.map(fmt_td_error)
                                                        .unwrap_or_else(|| "\u{2014}".to_string())}
                                                </span>
                                            </div>
                                            <div class="el-row pg-sessions-row">
                                                <span class="pg-sessions-row-label">"Bandit"</span>
                                                <span class="el-tag el-tag-on">
                                                    {format!("{bandit} · {arms} arms")}
                                                </span>
                                            </div>
                                        </div>
                                    </div>
                                }.into_any()
                            }
                            None => view! {
                                <div class="el-empty">
                                    "Learning endpoint unavailable — learner state cannot be shown."
                                </div>
                            }.into_any(),
                        })}
                    </Suspense>
                </Panel>
            </div>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fmt_iso_short_real_shape() {
        assert_eq!(
            fmt_iso_short("2026-06-06T03:57:02.904179368+00:00"),
            "06/06 03:57"
        );
        assert_eq!(fmt_iso_short("2026-12-31T23:59:00+00:00"), "31/12 23:59");
    }

    #[test]
    fn fmt_iso_short_rejects_malformed() {
        assert_eq!(fmt_iso_short(""), "\u{2014}");
        assert_eq!(fmt_iso_short("not-a-date"), "\u{2014}");
        assert_eq!(fmt_iso_short("2026-06-06"), "\u{2014}");
        // Multi-byte garbage in the prefix must not panic.
        assert_eq!(fmt_iso_short("2026-06-06Tżż:żż:00+00:00"), "\u{2014}");
    }

    #[test]
    fn health_color_variants() {
        assert_eq!(
            health_color("DEGRADED 0.53 [wiring:0.53] 36ms"),
            "var(--el-warn)"
        );
        assert_eq!(health_color("HEALTHY 0.91 [none] 12ms"), "var(--el-pos)");
        assert_eq!(health_color("OK 1.00"), "var(--el-pos)");
        assert_eq!(health_color("unknown"), "var(--el-fg)");
        assert_eq!(health_color(""), "var(--el-fg)");
    }

    #[test]
    fn parse_sessions_real_payload() {
        let v = serde_json::json!({
            "count": 1,
            "sessions": [{
                "session_id": "gen-3b5c4696-6918-469b-9",
                "objective": "Python Script",
                "task_type": "generator",
                "created_at": "2026-06-06T03:57:02.904179368+00:00",
                "updated_at": "2026-06-06T03:57:02.991890344+00:00"
            }]
        });
        let s = parse_sessions(&v);
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].0, "gen-3b5c4696-6918-469b-9");
        assert_eq!(s[0].1, "Python Script");
        assert_eq!(s[0].2, "generator");
        assert_eq!(s[0].3, "2026-06-06T03:57:02.904179368+00:00");
    }

    #[test]
    fn parse_sessions_empty_on_malformed() {
        assert!(parse_sessions(&serde_json::json!({"error": "boom"})).is_empty());
        assert!(parse_sessions(&serde_json::json!(null)).is_empty());
        assert!(parse_sessions(&serde_json::json!({"sessions": "nope"})).is_empty());
    }

    #[test]
    fn fmt_thousands_groups() {
        assert_eq!(fmt_thousands(0), "0");
        assert_eq!(fmt_thousands(999), "999");
        assert_eq!(fmt_thousands(82426), "82,426");
        assert_eq!(fmt_thousands(1_234_567), "1,234,567");
    }
}
