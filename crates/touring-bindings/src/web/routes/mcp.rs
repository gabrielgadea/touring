//! /mcp — MCP Tools executor (SPEC 2026-06-12 §6.1). Elite W4.
//!
//! Curated whitelist catalog from `GET /api/mcp/tools`, a dry-run-first
//! executor over `POST /api/mcp/call` and the async job queue from
//! `GET /api/jobs`. Every value rendered comes from the live daemon —
//! no fabricated data.

use crate::web::components::{Icon, KpiCell, KpiStrip, PageHero, Panel};
use crate::web::ctx::use_refresh_bus;
use crate::web::routes::quality_diff::fmt_epoch_short;
use crate::web::services::{fetch_json, post_json};
use leptos::prelude::*;
use leptos_meta::Title;
use serde_json::Value;

/// Flatten `/api/mcp/tools` into `(name, doc, needs_arg, cli_equivalent)`
/// rows. Entries without a `name` are skipped; malformed payloads yield
/// an empty catalog (rendered as an honest `el-empty`).
pub fn parse_tools(v: &Value) -> Vec<(String, String, bool, String)> {
    v.get("tools")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|t| {
                    let name = t.get("name")?.as_str()?.to_string();
                    let doc = t
                        .get("doc")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string();
                    let needs_arg = t.get("needs_arg").and_then(Value::as_bool).unwrap_or(false);
                    let cli = t
                        .get("cli_equivalent")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string();
                    Some((name, doc, needs_arg, cli))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// `el-tag` class for a job status: `completed` → pos (green),
/// `failed` → neg (rose), anything else → neutral.
pub fn job_status_class(status: &str) -> &'static str {
    match status {
        "completed" => "el-tag el-tag-pos",
        "failed" => "el-tag el-tag-neg",
        _ => "el-tag",
    }
}

/// Split a tool name into its dual-color segments: the `"touring_"`
/// prefix (fg-5) and the remainder (accent).
pub fn split_tool_name(name: &str) -> (&str, &str) {
    match name.strip_prefix("touring_") {
        Some(rest) => ("touring_", rest),
        None => ("", name),
    }
}

/// Truncate to `max` chars with a `…` suffix (job-id column).
pub fn truncate_id(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let head: String = s.chars().take(max).collect();
        format!("{head}…")
    }
}

/// Render the last `/api/mcp/call` response: error banner (real message
/// from the endpoint), dry-run argv preview, or pretty-printed output.
fn call_result_view(v: &Value) -> AnyView {
    if let Some(err) = v.get("error").and_then(Value::as_str) {
        let msg = err.to_string();
        return view! {
            <div class="el-banner el-banner-warn" role="alert">{msg}</div>
        }
        .into_any();
    }
    let cli = v
        .get("cli")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    if v.get("dry_run").and_then(Value::as_bool).unwrap_or(false) {
        let argv = v
            .get("argv")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .unwrap_or_default();
        view! {
            <div class="pg-mcp-result">
                <div class="el-eyebrow">"DRY-RUN · ARGV"</div>
                <pre class="el-code">{argv}</pre>
                <div class="el-eyebrow">"CLI EQUIVALENT"</div>
                <pre class="el-code">{cli}</pre>
            </div>
        }
        .into_any()
    } else {
        let pretty = match v.get("output") {
            Some(out) => serde_json::to_string_pretty(out).unwrap_or_else(|_| out.to_string()),
            None => serde_json::to_string_pretty(v).unwrap_or_default(),
        };
        view! {
            <div class="pg-mcp-result">
                <div class="el-eyebrow">"OUTPUT · "{cli}</div>
                <pre class="el-code pg-mcp-output">{pretty}</pre>
            </div>
        }
        .into_any()
    }
}

/// `/mcp` — curated tool catalog + whitelisted executor (W4).
#[component]
pub fn McpTools() -> impl IntoView {
    let bus = use_refresh_bus();
    let tools_res = LocalResource::new(move || {
        let _tick = bus.tick.get();
        async move { fetch_json("/api/mcp/tools").await.ok() }
    });
    let jobs_res = LocalResource::new(move || {
        let _tick = bus.tick.get();
        async move { fetch_json("/api/jobs").await.ok() }
    });

    let selected = RwSignal::new(Option::<String>::None);
    let arg = RwSignal::new(String::new());
    let exec_result = RwSignal::new(Option::<Value>::None);
    let exec_busy = RwSignal::new(false);

    // POST /api/mcp/call — dry_run=true previews the argv, false executes
    // the real CLI equivalent against the daemon.
    let dispatch = move |tool: String, dry: bool| {
        if exec_busy.get_untracked() {
            return;
        }
        let mut body = serde_json::json!({ "tool": tool, "dry_run": dry });
        let raw_arg = arg.get_untracked();
        let trimmed = raw_arg.trim();
        if !trimmed.is_empty() {
            body["arg"] = Value::String(trimmed.to_string());
        }
        exec_busy.set(true);
        leptos::task::spawn_local(async move {
            match post_json("/api/mcp/call", &body).await {
                Ok(v) => exec_result.set(Some(v)),
                Err(e) => exec_result.set(Some(serde_json::json!({ "error": e }))),
            }
            exec_busy.set(false);
        });
    };

    let tools_count = Signal::derive(move || {
        tools_res
            .get()
            .flatten()
            .and_then(|v| v.get("count").and_then(Value::as_u64))
            .map_or_else(|| "—".to_string(), |n| n.to_string())
    });
    let jobs_count = Signal::derive(move || {
        jobs_res
            .get()
            .flatten()
            .and_then(|v| v.get("job_count").and_then(Value::as_u64))
            .map_or_else(|| "—".to_string(), |n| n.to_string())
    });
    let jobs_failed = Signal::derive(move || {
        jobs_res
            .get()
            .flatten()
            .and_then(|v| {
                v.get("jobs").and_then(Value::as_array).map(|a| {
                    a.iter()
                        .filter(|j| j.get("status").and_then(Value::as_str) == Some("failed"))
                        .count()
                })
            })
            .map_or_else(|| "—".to_string(), |n| n.to_string())
    });

    view! {
        <div class="page pg-mcp">
            <Title text="Touring — MCP Tools"/>
            <Suspense fallback=|| view! { <div class="el-skeleton" aria-label="loading"></div> }>
                {move || tools_res.get().map(|raw| {
                    let tools = parse_tools(&raw.unwrap_or_default());
                    let tools_exec = tools.clone();

                    view! {
                        <div class="el-main-editorial">
                            <PageHero
                                eyebrow="ORCHESTRATION · MCP"
                                title="MCP"
                                title_em="tools"
                                sub="Curated whitelist of read-only daemon tools surfaced over MCP. Each call maps to a vetted CLI equivalent — dry-run first to inspect the real argv, then execute against the live daemon."
                                stat=tools_count
                                stat_label="CURATED"
                            >
                                <a class="el-btn" href="/hooks">"Hooks"</a>
                                <a class="el-btn" href="/health">"Health"</a>
                            </PageHero>

                            <KpiStrip>
                                <KpiCell label="Tools" value=tools_count sub="whitelisted"/>
                                <KpiCell label="Jobs" value=jobs_count sub="async queue"/>
                                <KpiCell label="Jobs failed" value=jobs_failed sub="status == failed"/>
                                <KpiCell
                                    label="Whitelist"
                                    value=Signal::derive(|| "read-only".to_string())
                                    sub="by construction"
                                />
                            </KpiStrip>

                            <section class="pg-mcp-grid">
                                <Panel num="01" eyebrow="Catalog" cmd="GET /api/mcp/tools">
                                    <div class="pg-mcp-catalog">
                                        {if tools.is_empty() {
                                            view! {
                                                <div class="el-empty">
                                                    "Tools endpoint unavailable — no catalog to show."
                                                </div>
                                            }.into_any()
                                        } else {
                                            tools.iter().map(|(name, _doc, _needs, _cli)| {
                                                let n_click = name.clone();
                                                let n_active = name.clone();
                                                let (prefix, rest) = split_tool_name(name);
                                                let prefix = prefix.to_string();
                                                let rest = rest.to_string();
                                                view! {
                                                    <button
                                                        type="button"
                                                        class=move || {
                                                            if selected.get().as_deref()
                                                                == Some(n_active.as_str())
                                                            {
                                                                "el-result-row selected pg-mcp-row"
                                                            } else {
                                                                "el-result-row pg-mcp-row"
                                                            }
                                                        }
                                                        on:click=move |_| {
                                                            selected.set(Some(n_click.clone()));
                                                            exec_result.set(None);
                                                            arg.set(String::new());
                                                        }
                                                    >
                                                        <Icon name="mcp"/>
                                                        <span class="el-mono pg-mcp-name">
                                                            <span class="pg-mcp-name-prefix">{prefix}</span>
                                                            <span class="pg-mcp-name-rest">{rest}</span>
                                                        </span>
                                                    </button>
                                                }
                                            }).collect_view().into_any()
                                        }}
                                    </div>
                                </Panel>

                                <Panel num="02" eyebrow="Executor" cmd="POST /api/mcp/call">
                                    {move || {
                                        match selected.get() {
                                            None => view! {
                                                <div class="el-empty">
                                                    "Select a tool from the catalog to inspect its CLI equivalent and execute it."
                                                </div>
                                            }.into_any(),
                                            Some(name) => {
                                                let meta = tools_exec
                                                    .iter()
                                                    .find(|(n, _, _, _)| n == &name)
                                                    .cloned();
                                                match meta {
                                                    None => view! {
                                                        <div class="el-empty">
                                                            "Tool not in the whitelist."
                                                        </div>
                                                    }.into_any(),
                                                    Some((tool_name, doc, needs_arg, cli)) => {
                                                        let t_dry = tool_name.clone();
                                                        let t_run = tool_name.clone();
                                                        view! {
                                                            <div class="pg-mcp-exec">
                                                                <div class="el-mono pg-mcp-name pg-mcp-exec-name">
                                                                    {tool_name}
                                                                </div>
                                                                <p class="pg-mcp-doc">{doc}</p>
                                                                <pre class="el-code">{cli}</pre>
                                                                {needs_arg.then(|| view! {
                                                                    <input
                                                                        class="pg-mcp-arg"
                                                                        type="text"
                                                                        placeholder="symbol / query"
                                                                        prop:value=move || arg.get()
                                                                        on:input=move |ev| {
                                                                            arg.set(event_target_value(&ev));
                                                                        }
                                                                    />
                                                                })}
                                                                <div class="pg-mcp-actions">
                                                                    <button
                                                                        type="button"
                                                                        class="el-btn"
                                                                        disabled=move || exec_busy.get()
                                                                        on:click=move |_| dispatch(t_dry.clone(), true)
                                                                    >
                                                                        "Dry-run"
                                                                    </button>
                                                                    <button
                                                                        type="button"
                                                                        class="el-btn el-btn-primary"
                                                                        disabled=move || exec_busy.get()
                                                                        on:click=move |_| dispatch(t_run.clone(), false)
                                                                    >
                                                                        {move || if exec_busy.get() {
                                                                            "Running…"
                                                                        } else {
                                                                            "Execute"
                                                                        }}
                                                                    </button>
                                                                </div>
                                                                {move || exec_result.get().map(|v| call_result_view(&v))}
                                                            </div>
                                                        }.into_any()
                                                    }
                                                }
                                            }
                                        }
                                    }}
                                </Panel>
                            </section>

                            <Panel num="03" eyebrow="Job queue" cmd="touring jobs list">
                                {move || jobs_res.get().map(|raw| {
                                    let raw = raw.unwrap_or_default();
                                    let jobs = raw
                                        .get("jobs")
                                        .and_then(Value::as_array)
                                        .cloned()
                                        .unwrap_or_default();
                                    if jobs.is_empty() {
                                        view! {
                                            <div class="el-empty">
                                                "Jobs endpoint unavailable or queue empty."
                                            </div>
                                        }.into_any()
                                    } else {
                                        view! {
                                            <table class="el-table">
                                                <thead>
                                                    <tr>
                                                        <th>"Job id"</th>
                                                        <th>"Status"</th>
                                                        <th>"Started (UTC)"</th>
                                                    </tr>
                                                </thead>
                                                <tbody>
                                                    {jobs.iter().map(|j| {
                                                        let id_full = j
                                                            .get("job_id")
                                                            .and_then(Value::as_str)
                                                            .unwrap_or("—")
                                                            .to_string();
                                                        let id_short = truncate_id(&id_full, 40);
                                                        let status = j
                                                            .get("status")
                                                            .and_then(Value::as_str)
                                                            .unwrap_or("—")
                                                            .to_string();
                                                        let cls = job_status_class(&status);
                                                        let started = j
                                                            .get("started_at_secs")
                                                            .and_then(Value::as_f64)
                                                            .map_or_else(
                                                                || "—".to_string(),
                                                                fmt_epoch_short,
                                                            );
                                                        view! {
                                                            <tr>
                                                                <td
                                                                    class="el-mono pg-mcp-jobid"
                                                                    title=id_full
                                                                >
                                                                    {id_short}
                                                                </td>
                                                                <td><span class=cls>{status}</span></td>
                                                                <td class="el-mono">{started}</td>
                                                            </tr>
                                                        }
                                                    }).collect_view()}
                                                </tbody>
                                            </table>
                                        }.into_any()
                                    }
                                })}
                            </Panel>
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
    fn parse_tools_extracts_whitelist_rows() {
        let v = serde_json::json!({
            "count": 2,
            "tools": [
                {"name": "touring_doctor", "doc": "Health check", "needs_arg": false,
                 "cli_equivalent": "touring doctor -j"},
                {"name": "touring_index_find", "doc": "Symbol lookup", "needs_arg": true,
                 "cli_equivalent": "touring index find <symbol>"}
            ]
        });
        let rows = parse_tools(&v);
        assert_eq!(rows.len(), 2);
        assert_eq!(
            rows[0],
            (
                "touring_doctor".to_string(),
                "Health check".to_string(),
                false,
                "touring doctor -j".to_string()
            )
        );
        assert!(rows[1].2, "needs_arg should be true for index find");
    }

    #[test]
    fn parse_tools_empty_on_malformed_payload() {
        assert!(parse_tools(&serde_json::json!({"error": "boom"})).is_empty());
        assert!(parse_tools(&Value::Null).is_empty());
    }

    #[test]
    fn parse_tools_skips_entries_without_name() {
        let v = serde_json::json!({"tools": [{"doc": "nameless"}, {"name": "touring_status"}]});
        let rows = parse_tools(&v);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].0, "touring_status");
    }

    #[test]
    fn job_status_class_variants() {
        assert_eq!(job_status_class("completed"), "el-tag el-tag-pos");
        assert_eq!(job_status_class("failed"), "el-tag el-tag-neg");
        assert_eq!(job_status_class("running"), "el-tag");
        assert_eq!(job_status_class(""), "el-tag");
    }

    #[test]
    fn split_tool_name_dual_color_segments() {
        assert_eq!(split_tool_name("touring_doctor"), ("touring_", "doctor"));
        assert_eq!(split_tool_name("ctx_execute"), ("", "ctx_execute"));
    }

    #[test]
    fn truncate_id_caps_at_max_chars() {
        assert_eq!(truncate_id("short", 40), "short");
        let long = "a".repeat(50);
        let t = truncate_id(&long, 40);
        assert_eq!(t.chars().count(), 41);
        assert!(t.ends_with('…'));
    }
}
