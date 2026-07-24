//! /plans — decompose orchestration board. Elite W3 page
//! (SPEC 2026-06-12 §5.14): PageHero with the live task-DAG total,
//! KPI strip, the W1-W10 workflow-template card grid (expandable to
//! the real per-step playbook), a `PipelineStages` chart derived from
//! the selected template's real steps, a task-id lookup against
//! `/api/decompose/task` and the honest ready queue.
//!
//! Data sources (all REAL, captured 2026-06-12 against the live server):
//! - `GET /api/decompose` → `{"total_subtasks":434,"total_tasks":133}`.
//! - `GET /api/decompose/templates` → `{"count":10,"templates":[{id,name,
//!   description,patterns,steps:[{order,action,tool_or_cmd,note}],pitfall}]}`
//!   — real JSON since 12/06. [`extract_template_array`] still accepts a
//!   bare array and still routes to the verbatim W1-W10 catalog whenever
//!   the endpoint regresses to text (`{"error":"parse error"}`) — honest
//!   fallback, never mock.
//! - `GET /api/decompose/ready` → `{"ready_subtasks":[],"blocked_subtasks":[],
//!   "parallel_groups":[],"task_id":""}` (empty without an active DAG).
//! - `GET /api/decompose/task?id=X` → `{"task":{...}|null,"subtask_count":N,
//!   "subtasks":[...]}` — `task: null` when the id does not exist.

use leptos::prelude::*;
use leptos_meta::Title;

use crate::web::components::pipeline_stages::{Stage, StageState};
use crate::web::components::{KpiCell, KpiStrip, PageHero, Panel, PipelineStages};
use crate::web::ctx::use_refresh_bus;
use crate::web::services::{
    fetch_decompose, fetch_decompose_ready, fetch_decompose_task, fetch_decompose_templates,
};

/// W1-W10 workflow template catalog — captured VERBATIM from the live
/// `touring decompose template <id>` CLI output (Rn2 §6, 2026-06-12).
/// The catalog is compiled into the CLI binary (static reference data,
/// not per-project state), so this mirror is real data, not placeholder.
/// Used ONLY when `/api/decompose/templates` fails to yield a JSON array.
/// Columns: (id, name, description, patterns).
const TEMPLATE_CATALOG: [(&str, &str, &str, &str); 10] = [
    (
        "W1",
        "Localizar \"onde está X?\"",
        "Locate a symbol, file, or concept in the codebase using index-first strategy.",
        "P1, P5",
    ),
    (
        "W2",
        "Compreender uma feature / fluxo",
        "Understand how a feature or code path works end-to-end without saturating context.",
        "P4, P8, P3",
    ),
    (
        "W3",
        "Bugfix pontual (1 arquivo)",
        "Apply a targeted fix to a single file with full pre/post validation.",
        "P2, P4, P7, P9",
    ),
    (
        "W4",
        "Refatoração cross-file (renomear / extrair / mover)",
        "Rename or move a symbol across multiple files without missing any callsite.",
        "P4, P6, P7, P9, P10",
    ),
    (
        "W5",
        "Criar feature / módulo novo",
        "Create a new pub symbol or module and wire it to consumers (REGRA #0).",
        "P4, P7, P9, P10",
    ),
    (
        "W6",
        "Depurar teste falhando / erro",
        "Diagnose and fix a failing test or runtime error, starting from system health.",
        "P5, P6, P1, P10",
    ),
    (
        "W7",
        "Code review",
        "Structural and semantic review of a code change using TDG grade, wiring, and semantics.",
        "P8, P3, P4",
    ),
    (
        "W8",
        "Validar (build / test / lint)",
        "Run the full build, test, and lint validation pipeline.",
        "P9, P3",
    ),
    (
        "W9",
        "Explorar codebase desconhecido",
        "Orient in an unfamiliar codebase using index-first and subagent exploration.",
        "P4, P8, P1",
    ),
    (
        "W10",
        "Versionar / commit",
        "Snapshot state, close RL loop, write diary, and commit with proper heredoc message.",
        "P7, P10",
    ),
];

/// One real workflow step from the templates endpoint
/// (`steps: [{order, action, tool_or_cmd, note}]`).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TemplateStep {
    /// 1-based step order as emitted by the CLI.
    pub order: u64,
    /// What the step does ("tantivy search", "index find", …).
    pub action: String,
    /// Concrete tool or command for the step (mono in the UI).
    pub tool_or_cmd: String,
    /// Optional CLI note ("" when absent).
    pub note: String,
}

/// Parsed view of one workflow template — endpoint shape or catalog
/// fallback (fallback entries carry empty `steps`/`pitfall`).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TemplateView {
    /// Template id ("W1".."W10").
    pub id: String,
    /// Human name.
    pub name: String,
    /// One-line description.
    pub description: String,
    /// Combination patterns referenced (P1..P10).
    pub patterns: Vec<String>,
    /// Known pitfall ("" when absent).
    pub pitfall: String,
    /// Real ordered playbook steps (empty in catalog-fallback mode).
    pub steps: Vec<TemplateStep>,
}

/// Accept either a bare JSON array or `{"templates": [...]}` from the
/// templates endpoint. Returns `None` for any other shape (e.g. the
/// historical `{"error":"parse error"}`), routing the view to the
/// verbatim catalog fallback.
pub fn extract_template_array(v: &serde_json::Value) -> Option<Vec<serde_json::Value>> {
    if let Some(arr) = v.as_array() {
        return Some(arr.clone());
    }
    v.get("templates").and_then(|t| t.as_array()).cloned()
}

/// Parse one template object from the real endpoint shape — every field
/// is defensive ("" / empty vec when absent), never panics.
pub fn parse_template(v: &serde_json::Value) -> TemplateView {
    let text = |k: &str| -> String { v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string() };
    let patterns = match v.get("patterns") {
        Some(serde_json::Value::Array(a)) => a
            .iter()
            .filter_map(|x| x.as_str().map(str::to_string))
            .collect(),
        Some(serde_json::Value::String(s)) => s
            .split(',')
            .map(|p| p.trim().to_string())
            .filter(|p| !p.is_empty())
            .collect(),
        _ => Vec::new(),
    };
    let steps = v
        .get("steps")
        .and_then(|x| x.as_array())
        .map(|a| a.iter().map(parse_step).collect())
        .unwrap_or_default();
    TemplateView {
        id: text("id"),
        name: text("name"),
        description: text("description"),
        patterns,
        pitfall: text("pitfall"),
        steps,
    }
}

/// Parse one step object (`{order, action, tool_or_cmd, note}`).
pub fn parse_step(v: &serde_json::Value) -> TemplateStep {
    let text = |k: &str| -> String { v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string() };
    TemplateStep {
        order: v.get("order").and_then(|x| x.as_u64()).unwrap_or(0),
        action: text("action"),
        tool_or_cmd: text("tool_or_cmd"),
        note: text("note"),
    }
}

/// Parse the whole templates payload — `None` when no JSON array is
/// present (caller falls back to the verbatim catalog, honestly labeled).
pub fn parse_templates(v: &serde_json::Value) -> Option<Vec<TemplateView>> {
    let arr = extract_template_array(v)?;
    Some(arr.iter().map(parse_template).collect())
}

/// The verbatim W1-W10 catalog as [`TemplateView`]s (steps empty —
/// catalog mode carries headers only, and the UI says so).
pub fn fallback_templates() -> Vec<TemplateView> {
    TEMPLATE_CATALOG
        .iter()
        .map(|(id, name, desc, patterns)| TemplateView {
            id: (*id).to_string(),
            name: (*name).to_string(),
            description: (*desc).to_string(),
            patterns: patterns
                .split(',')
                .map(|p| p.trim().to_string())
                .filter(|p| !p.is_empty())
                .collect(),
            pitfall: String::new(),
            steps: Vec::new(),
        })
        .collect()
}

/// Map a subtask `status` string to a stage-dot state
/// (SPEC §5.14: "completed"→Done, "in_progress"→Active, else Pending).
pub fn stage_state_for(status: &str) -> StageState {
    match status.trim().to_ascii_lowercase().as_str() {
        "completed" => StageState::Done,
        "in_progress" => StageState::Active,
        _ => StageState::Pending,
    }
}

/// Char-safe truncation with a trailing ellipsis when over `max`.
pub fn truncate_label(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let cut: String = s.chars().take(max.saturating_sub(1)).collect();
        format!("{cut}…")
    }
}

/// Fixed position label for pipeline stage `index` — `Stage::label`
/// is `&'static str`, so the dynamic step action goes into `count`
/// instead (no `Box::leak`). Positions cover the longest real
/// template playbook with headroom.
pub fn step_label(index: usize) -> &'static str {
    const LABELS: [&str; 12] = [
        "S1", "S2", "S3", "S4", "S5", "S6", "S7", "S8", "S9", "S10", "S11", "S12",
    ];
    LABELS.get(index).copied().unwrap_or("S+")
}

/// Derive pipeline stages from a template's REAL steps: position label
/// (S1..Sn), the real action truncated as the mono count, Pending state
/// (templates are playbooks, not live executions — honest).
pub fn stages_for_template(t: &TemplateView) -> Vec<Stage> {
    t.steps
        .iter()
        .enumerate()
        .map(|(i, s)| Stage {
            label: step_label(i),
            count: Some(truncate_label(&s.action, 18)),
            state: StageState::Pending,
        })
        .collect()
}

/// Render a `depends_on` value (array of ids or single string) as a
/// comma-joined mono string.
pub fn depends_text(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::Array(a) => a
            .iter()
            .filter_map(|x| x.as_str())
            .collect::<Vec<_>>()
            .join(", "),
        serde_json::Value::String(s) => s.clone(),
        _ => String::new(),
    }
}

/// Normalize one ready-queue entry — entries may be plain strings or
/// objects (`subtask_id`/`id` + `title`/`description` + `status`).
/// Returns `(status, id, title)`.
pub fn queue_row(v: &serde_json::Value) -> (String, String, String) {
    if let Some(s) = v.as_str() {
        return (String::new(), s.to_string(), String::new());
    }
    let id = v
        .get("subtask_id")
        .or_else(|| v.get("id"))
        .and_then(|x| x.as_str())
        .unwrap_or("—")
        .to_string();
    let title = v
        .get("title")
        .or_else(|| v.get("description"))
        .and_then(|x| x.as_str())
        .unwrap_or("—")
        .to_string();
    let status = v
        .get("status")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string();
    (status, id, title)
}

/// `/plans` — full decompose orchestration board.
#[component]
pub fn PlansBoard() -> impl IntoView {
    let bus = use_refresh_bus();

    let totals = LocalResource::new(move || {
        let _tick = bus.tick.get();
        async move {
            fetch_decompose()
                .await
                .ok()
                .unwrap_or(serde_json::Value::Null)
        }
    });
    let templates_res = LocalResource::new(move || {
        let _tick = bus.tick.get();
        async move {
            fetch_decompose_templates()
                .await
                .ok()
                .unwrap_or(serde_json::Value::Null)
        }
    });
    let ready = LocalResource::new(move || {
        let _tick = bus.tick.get();
        async move {
            fetch_decompose_ready()
                .await
                .ok()
                .unwrap_or(serde_json::Value::Null)
        }
    });

    // Task lookup — fetch only after an explicit submit (no speculative
    // queries against the DAG store).
    let (lookup_id, set_lookup_id) = signal(String::new());
    let (submitted_id, set_submitted_id) = signal(String::new());
    let task_res = LocalResource::new(move || {
        let id = submitted_id.get();
        async move {
            let id = id.trim().to_string();
            if id.is_empty() {
                serde_json::Value::Null
            } else {
                fetch_decompose_task(&id)
                    .await
                    .ok()
                    .unwrap_or(serde_json::Value::Null)
            }
        }
    });

    // Parsed templates: Some((list, from_endpoint)) once loaded; the
    // verbatim catalog (from_endpoint=false) only when the endpoint
    // yields no JSON array.
    let templates_view: Signal<Option<(Vec<TemplateView>, bool)>> = Signal::derive(move || {
        templates_res
            .get()
            .map(|json| match parse_templates(&json) {
                Some(list) if !list.is_empty() => (list, true),
                _ => (fallback_templates(), false),
            })
    });

    // Card selection — drives both the expanded playbook and the
    // pipeline panel below.
    let selected_tpl: RwSignal<Option<usize>> = RwSignal::new(None);

    let stages: Signal<Vec<Stage>> = Signal::derive(move || {
        let Some((list, _)) = templates_view.get() else {
            return Vec::new();
        };
        let idx = selected_tpl.get().filter(|i| *i < list.len()).unwrap_or(0);
        list.get(idx).map(stages_for_template).unwrap_or_default()
    });
    let pipeline_caption: Signal<String> = Signal::derive(move || {
        let Some((list, _)) = templates_view.get() else {
            return "loading templates…".to_string();
        };
        let sel = selected_tpl.get().filter(|i| *i < list.len());
        match list.get(sel.unwrap_or(0)) {
            Some(t) if sel.is_some() => format!("{} — {}", t.id, t.name),
            Some(t) => format!(
                "{} — {} (default — click a template card to switch)",
                t.id, t.name
            ),
            None => "no templates available".to_string(),
        }
    });

    // ── KPI signals — "—" until each fetch resolves ─────────────────
    let total_tasks = Signal::derive(move || {
        totals
            .get()
            .and_then(|j| j.get("total_tasks").and_then(|v| v.as_u64()))
            .map(|n| n.to_string())
            .unwrap_or_else(|| "—".to_string())
    });
    let total_subtasks = Signal::derive(move || {
        totals
            .get()
            .and_then(|j| j.get("total_subtasks").and_then(|v| v.as_u64()))
            .map(|n| n.to_string())
            .unwrap_or_else(|| "—".to_string())
    });
    let ready_count = Signal::derive(move || {
        ready
            .get()
            .and_then(|j| {
                j.get("ready_subtasks")
                    .and_then(|v| v.as_array())
                    .map(|a| a.len().to_string())
            })
            .unwrap_or_else(|| "—".to_string())
    });
    let template_count = Signal::derive(move || {
        templates_view
            .get()
            .map(|(list, _)| list.len().to_string())
            .unwrap_or_else(|| "—".to_string())
    });

    view! {
        <div class="page pg-plans">
            <Title text="Touring — Plans"/>
            <div class="el-main-editorial">
                <PageHero
                    eyebrow="ORCHESTRATION · DECOMPOSE"
                    title="Plans"
                    title_em="board"
                    sub="Native task DAGs from touring decompose — live totals, the W1-W10 workflow template catalog with real per-step playbooks, lookup by task id and the honest ready queue."
                    stat=total_tasks
                    stat_label="TASK DAGS"
                >
                    <code class="el-cmd-label">"touring decompose status"</code>
                    <code class="el-cmd-label">"touring decompose ready"</code>
                </PageHero>

                <KpiStrip>
                    <KpiCell label="Tasks" value=total_tasks sub="decompose DAGs"/>
                    <KpiCell label="Subtasks" value=total_subtasks sub="across all DAGs"/>
                    <KpiCell label="Ready" value=ready_count sub="unblocked · live queue"/>
                    <KpiCell label="Templates" value=template_count sub="W1-W10 catalog"/>
                </KpiStrip>

                // ── 01 · Workflow templates — expandable card grid ──────
                <Panel num="01" eyebrow="Workflow templates" cmd="touring decompose templates">
                    <Suspense fallback=|| view! { <div class="el-skeleton" aria-label="loading"></div> }>
                        {move || templates_view.get().map(|(list, from_endpoint)| view! {
                            {(!from_endpoint).then(|| view! {
                                <p class="pg-plans-fallback-note">
                                    "endpoint /api/decompose/templates returned no JSON array — rendering the W1-W10 catalog captured verbatim from the CLI (steps unavailable in this mode)"
                                </p>
                            })}
                            <div class="pg-plans-grid">
                                {list.into_iter().enumerate().map(|(i, t)| {
                                    let TemplateView { id, name, description, patterns, pitfall, steps } = t;
                                    let patterns_text = patterns.join(", ");
                                    let has_patterns = !patterns_text.is_empty();
                                    let has_pitfall = !pitfall.is_empty();
                                    let steps_empty = steps.is_empty();
                                    let expanded = Signal::derive(move || selected_tpl.get() == Some(i));
                                    view! {
                                        <article
                                            class=move || if expanded.get() {
                                                "el-card pg-plans-card pg-plans-card-open"
                                            } else {
                                                "el-card pg-plans-card"
                                            }
                                            on:click=move |_| selected_tpl.update(|s| {
                                                *s = if *s == Some(i) { None } else { Some(i) };
                                            })
                                        >
                                            <div class="pg-plans-card-head">
                                                <span class="el-tag el-tag-on">{id}</span>
                                                {has_patterns.then(|| view! {
                                                    <span class="el-tag pg-plans-card-patterns">{patterns_text}</span>
                                                })}
                                            </div>
                                            <h3 class="pg-plans-card-name">{name}</h3>
                                            <p class="pg-plans-card-desc">{description}</p>
                                            {has_pitfall.then(|| view! {
                                                <p class="pg-plans-card-pitfall">"⚠ "{pitfall}</p>
                                            })}
                                            {move || expanded.get().then(|| {
                                                if steps_empty {
                                                    view! {
                                                        <p class="pg-plans-steps-empty">
                                                            "steps unavailable in catalog-fallback mode — the endpoint emitted no JSON steps for this template"
                                                        </p>
                                                    }.into_any()
                                                } else {
                                                    view! {
                                                        <ol class="pg-plans-steps">
                                                            {steps.clone().into_iter().map(|s| {
                                                                let TemplateStep { order, action, tool_or_cmd, note } = s;
                                                                let has_note = !note.is_empty();
                                                                view! {
                                                                    <li class="pg-plans-step">
                                                                        <span class="el-mono pg-plans-step-order">{format!("{order:02}")}</span>
                                                                        <span class="pg-plans-step-action">{action}</span>
                                                                        <code class="el-mono pg-plans-step-cmd">{tool_or_cmd}</code>
                                                                        {has_note.then(|| view! {
                                                                            <span class="pg-plans-step-note">{note}</span>
                                                                        })}
                                                                    </li>
                                                                }
                                                            }).collect_view()}
                                                        </ol>
                                                    }.into_any()
                                                }
                                            })}
                                        </article>
                                    }
                                }).collect_view()}
                            </div>
                        })}
                    </Suspense>
                </Panel>

                // ── 02 · Pipeline — derived from the selected playbook ──
                <Panel num="02" eyebrow="Pipeline" cmd="touring decompose template <id>">
                    <p class="el-mono pg-plans-pipeline-caption">{move || pipeline_caption.get()}</p>
                    {move || {
                        if stages.get().is_empty() {
                            view! {
                                <div class="el-empty">
                                    <p>"No steps to chart — the active template carries no JSON steps (catalog-fallback mode)."</p>
                                </div>
                            }.into_any()
                        } else {
                            view! { <PipelineStages stages=stages/> }.into_any()
                        }
                    }}
                </Panel>

                // ── 03 · Task lookup — explicit fetch by id ─────────────
                <Panel num="03" eyebrow="Task lookup" cmd="touring decompose get <id>">
                    <form
                        class="pg-plans-lookup-form"
                        on:submit=move |ev| {
                            ev.prevent_default();
                            set_submitted_id.set(lookup_id.get());
                        }
                    >
                        <input
                            type="text"
                            class="el-palette-input pg-plans-input"
                            placeholder="task id — e.g. task_1780763041476850005"
                            value={lookup_id.get()}
                            on:input=move |ev| set_lookup_id.set(event_target_value(&ev))
                        />
                        <button type="submit" class="el-btn">"Fetch"</button>
                    </form>
                    {move || {
                        let submitted = submitted_id.get();
                        if submitted.trim().is_empty() {
                            return view! {
                                <div class="el-empty">
                                    <p>
                                        "Enter a task id and press Fetch — ids come from "
                                        <code class="el-mono">"touring decompose status"</code>
                                        "."
                                    </p>
                                </div>
                            }.into_any();
                        }
                        match task_res.get() {
                            None => view! { <div class="el-skeleton" aria-label="loading"></div> }.into_any(),
                            Some(json) => {
                                let task = json.get("task").cloned().unwrap_or(serde_json::Value::Null);
                                if task.is_null() {
                                    let msg = format!(
                                        "task \"{}\" not found — /api/decompose/task returned task: null",
                                        submitted.trim()
                                    );
                                    view! { <div class="el-empty"><p>{msg}</p></div> }.into_any()
                                } else {
                                    let field = |keys: &[&str]| -> Option<String> {
                                        keys.iter().find_map(|k| {
                                            task.get(*k).and_then(|v| v.as_str()).map(str::to_string)
                                        })
                                    };
                                    let subtask_count = json
                                        .get("subtask_count")
                                        .and_then(|v| v.as_u64())
                                        .unwrap_or(0);
                                    let meta: Vec<(&'static str, String)> = vec![
                                        ("Task id", field(&["task_id", "id"])
                                            .unwrap_or_else(|| submitted.trim().to_string())),
                                        ("Type", field(&["task_type", "type"])
                                            .unwrap_or_else(|| "—".to_string())),
                                        ("Status", field(&["status"])
                                            .unwrap_or_else(|| "—".to_string())),
                                        ("Description", field(&["description", "title", "name"])
                                            .unwrap_or_else(|| "—".to_string())),
                                        ("Subtasks", subtask_count.to_string()),
                                    ];
                                    let subtasks = json
                                        .get("subtasks")
                                        .and_then(|v| v.as_array())
                                        .cloned()
                                        .unwrap_or_default();
                                    view! {
                                        <div class="pg-plans-task-meta">
                                            {meta.into_iter().map(|(label, value)| view! {
                                                <div class="pg-plans-task-field">
                                                    <span class="el-eyebrow pg-plans-task-label">{label}</span>
                                                    <span class="pg-plans-task-value">{value}</span>
                                                </div>
                                            }).collect_view()}
                                        </div>
                                        {if subtasks.is_empty() {
                                            view! {
                                                <div class="el-empty">
                                                    <p>"Task exists but holds 0 subtasks in the DAG store."</p>
                                                </div>
                                            }.into_any()
                                        } else {
                                            view! {
                                                <table class="el-table">
                                                    <thead>
                                                        <tr>
                                                            <th>"Status"</th>
                                                            <th>"Id"</th>
                                                            <th>"Title"</th>
                                                            <th>"Depends"</th>
                                                        </tr>
                                                    </thead>
                                                    <tbody>
                                                        {subtasks.into_iter().map(|st| {
                                                            let status = st.get("status")
                                                                .and_then(|v| v.as_str())
                                                                .unwrap_or("")
                                                                .to_string();
                                                            let dot = stage_state_for(&status).class();
                                                            let id = st.get("subtask_id")
                                                                .or_else(|| st.get("id"))
                                                                .and_then(|v| v.as_str())
                                                                .unwrap_or("—")
                                                                .to_string();
                                                            let title = st.get("title")
                                                                .or_else(|| st.get("description"))
                                                                .and_then(|v| v.as_str())
                                                                .unwrap_or("—")
                                                                .to_string();
                                                            let depends = st.get("depends_on")
                                                                .or_else(|| st.get("dependencies"))
                                                                .map(depends_text)
                                                                .unwrap_or_default();
                                                            view! {
                                                                <tr>
                                                                    <td>
                                                                        <div class="pg-plans-status-cell">
                                                                            <span class=dot aria-hidden="true"></span>
                                                                            <span class="pg-plans-status">{status}</span>
                                                                        </div>
                                                                    </td>
                                                                    <td class="el-mono">{id}</td>
                                                                    <td>{title}</td>
                                                                    <td class="el-mono pg-plans-depends">{depends}</td>
                                                                </tr>
                                                            }
                                                        }).collect_view()}
                                                    </tbody>
                                                </table>
                                            }.into_any()
                                        }}
                                    }.into_any()
                                }
                            }
                        }
                    }}
                </Panel>

                // ── 04 · Ready queue — honest empty without an active DAG ─
                <Panel num="04" eyebrow="Ready queue" cmd="touring decompose ready">
                    <Suspense fallback=|| view! { <div class="el-skeleton" aria-label="loading"></div> }>
                        {move || ready.get().map(|json| {
                            let ready_items = json.get("ready_subtasks")
                                .and_then(|v| v.as_array())
                                .cloned()
                                .unwrap_or_default();
                            let blocked_n = json.get("blocked_subtasks")
                                .and_then(|v| v.as_array())
                                .map(|a| a.len())
                                .unwrap_or(0);
                            let groups_n = json.get("parallel_groups")
                                .and_then(|v| v.as_array())
                                .map(|a| a.len())
                                .unwrap_or(0);
                            let task_id = json.get("task_id")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string();
                            if ready_items.is_empty() {
                                view! {
                                    <div class="el-empty">
                                        <p>"No ready subtasks — no active DAG in this project."</p>
                                        <p class="el-mono pg-plans-queue-meta">
                                            {format!("blocked: {blocked_n} · parallel groups: {groups_n}")}
                                        </p>
                                    </div>
                                }.into_any()
                            } else {
                                view! {
                                    {(!task_id.is_empty()).then(|| view! {
                                        <p class="el-mono pg-plans-queue-meta">
                                            "active DAG: "{task_id.clone()}
                                        </p>
                                    })}
                                    <table class="el-table">
                                        <thead>
                                            <tr>
                                                <th>"Status"</th>
                                                <th>"Id"</th>
                                                <th>"Title"</th>
                                            </tr>
                                        </thead>
                                        <tbody>
                                            {ready_items.iter().map(|item| {
                                                let (status, id, title) = queue_row(item);
                                                let dot = stage_state_for(&status).class();
                                                view! {
                                                    <tr>
                                                        <td>
                                                            <div class="pg-plans-status-cell">
                                                                <span class=dot aria-hidden="true"></span>
                                                                <span class="pg-plans-status">{status}</span>
                                                            </div>
                                                        </td>
                                                        <td class="el-mono">{id}</td>
                                                        <td>{title}</td>
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
            </div>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn stage_state_for_maps_real_statuses() {
        assert_eq!(stage_state_for("completed"), StageState::Done);
        assert_eq!(stage_state_for("  In_Progress "), StageState::Active);
        assert_eq!(stage_state_for("pending"), StageState::Pending);
        assert_eq!(stage_state_for("blocked"), StageState::Pending);
        assert_eq!(stage_state_for(""), StageState::Pending);
    }

    #[test]
    fn parse_templates_real_endpoint_shape() {
        // Mirrors the REAL /api/decompose/templates shape (2026-06-12).
        let v = json!({
            "count": 1,
            "templates": [{
                "id": "W1",
                "name": "Localizar",
                "description": "Locate a symbol.",
                "patterns": ["P1", "P5"],
                "steps": [
                    {"order": 1, "action": "tantivy search",
                     "tool_or_cmd": "touring tantivy search \"X\"", "note": "BM25"},
                    {"order": 2, "action": "index find",
                     "tool_or_cmd": "touring index find X"}
                ],
                "pitfall": "grep -r direto satura contexto"
            }]
        });
        let list = parse_templates(&v).expect("endpoint shape yields an array");
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, "W1");
        assert_eq!(list[0].patterns, vec!["P1".to_string(), "P5".to_string()]);
        assert_eq!(list[0].steps.len(), 2);
        assert_eq!(list[0].steps[0].note, "BM25");
        assert_eq!(list[0].steps[1].order, 2);
        assert_eq!(list[0].steps[1].note, "");
        assert!(list[0].pitfall.starts_with("grep"));
    }

    #[test]
    fn parse_templates_error_shape_routes_to_fallback() {
        // Historical text-mode shape → None → verbatim catalog.
        assert!(parse_templates(&json!({"error": "parse error"})).is_none());
        let fb = fallback_templates();
        assert_eq!(fb.len(), 10);
        assert_eq!(fb[0].id, "W1");
        assert_eq!(fb[9].id, "W10");
        assert!(
            fb.iter()
                .all(|t| t.steps.is_empty() && t.pitfall.is_empty())
        );
        assert_eq!(fb[0].patterns, vec!["P1".to_string(), "P5".to_string()]);
    }

    #[test]
    fn extract_template_array_accepts_bare_and_wrapped() {
        let bare = json!([{"id": "W1"}]);
        assert_eq!(extract_template_array(&bare).map(|a| a.len()), Some(1));
        let wrapped = json!({"templates": [{"id": "W1"}, {"id": "W2"}]});
        assert_eq!(extract_template_array(&wrapped).map(|a| a.len()), Some(2));
        assert!(extract_template_array(&json!({"error": "x"})).is_none());
    }

    #[test]
    fn stages_for_template_uses_position_labels_and_real_actions() {
        let t = TemplateView {
            steps: vec![
                TemplateStep {
                    order: 1,
                    action: "a very long action name that overflows".to_string(),
                    tool_or_cmd: "cmd".to_string(),
                    note: String::new(),
                },
                TemplateStep {
                    order: 2,
                    action: "short".to_string(),
                    tool_or_cmd: "cmd2".to_string(),
                    note: String::new(),
                },
            ],
            ..Default::default()
        };
        let stages = stages_for_template(&t);
        assert_eq!(stages.len(), 2);
        assert_eq!(stages[0].label, "S1");
        assert_eq!(stages[1].label, "S2");
        assert!(stages.iter().all(|s| s.state == StageState::Pending));
        let c0 = stages[0].count.clone().unwrap_or_default();
        assert!(c0.chars().count() <= 18);
        assert!(c0.ends_with('…'));
        assert_eq!(stages[1].count.as_deref(), Some("short"));
        // Position labels never run out — clamp instead of panicking.
        assert_eq!(step_label(11), "S12");
        assert_eq!(step_label(99), "S+");
    }

    #[test]
    fn truncate_depends_and_queue_helpers() {
        assert_eq!(truncate_label("abc", 5), "abc");
        assert_eq!(truncate_label("abcdef", 5).chars().count(), 5);
        assert!(truncate_label("abcdef", 5).ends_with('…'));
        assert_eq!(depends_text(&json!(["S-01", "S-02"])), "S-01, S-02");
        assert_eq!(depends_text(&json!("S-01")), "S-01");
        assert_eq!(depends_text(&json!(42)), "");
        let (st, id, title) = queue_row(&json!({
            "subtask_id": "S-01", "title": "Do it", "status": "in_progress"
        }));
        assert_eq!(
            (st.as_str(), id.as_str(), title.as_str()),
            ("in_progress", "S-01", "Do it")
        );
        let (st2, id2, title2) = queue_row(&json!("plain-id"));
        assert_eq!(
            (st2.as_str(), id2.as_str(), title2.as_str()),
            ("", "plain-id", "")
        );
    }
}
