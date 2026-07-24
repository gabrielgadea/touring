//! `task-sync-post-create` hook handler + 32 co-located hint helpers.
//!
//! Mirrors Claude Code's TaskCreate to the Touring decompose DAG. When a new
//! task is created, this module:
//!   - Creates the parent DAG entry + scaffolded `::scout/::implement/::validate`
//!     subtasks (R14-S1)
//!   - Stores reverse mapping `task:<task_id>:session` (R163)
//!   - Emits 30+ per-kind generator scaffold hints via
//!     `maybe_<kind>_hint_on_task_create` helpers
//!   - Injects RL reward + memory lesson on task creation
//!
//! All `maybe_*_hint_on_task_create` helpers, `task_create_wiring_hint`, and
//! `task_create_gotcha_hint` are exclusive to this handler and co-located
//! here. Extracted from `lifecycle.rs` as part of FIX-3 D8 (biggest handler
//! extraction — ~960 LOC).

use serde_json::Value;

use crate::runtime::HookRuntime;

// Shared helpers from `lifecycle/shared.rs` (via super re-export).
// Note: collect_subject_generator_hints is defined in this module (D10).
use super::{persist_task_creation, plan_scaffold_for_subject, suggest_generator_for_task_subject};

/// F0-pre (2026-07-20): on PostToolUse(TaskCreate) the id is BORN in the tool
/// response ("Task #1 created successfully: …"), not in `tool_input` — reading
/// only the input yielded task_id="unknown" and a shared junk mirror. Extract
/// the "#<digits>" from the response (string or {content: …} shapes).
fn task_id_from_response(input: &Value) -> Option<String> {
    let resp = input
        .get("tool_response")
        .or_else(|| input.get("tool_result"))?;
    let text = resp
        .as_str()
        .map(String::from)
        .or_else(|| {
            resp.get("content")
                .and_then(|c| c.as_str())
                .map(String::from)
        })
        .unwrap_or_else(|| resp.to_string());
    let hash = text.find('#')?;
    let digits: String = text[hash + 1..]
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    (!digits.is_empty()).then_some(digits)
}

pub(crate) fn handle_task_sync_post_create(rt: &mut HookRuntime, input: &Value) -> String {
    let tool_input = input.get("tool_input").unwrap_or(input);
    let response_id = task_id_from_response(input);
    let task_id = tool_input
        .get("task_id")
        .or_else(|| input.get("task_id"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty() && *s != "unknown")
        .map(String::from)
        .or(response_id)
        .unwrap_or_else(|| "unknown".to_string());
    let task_id = task_id.as_str();
    let task_subject = tool_input
        .get("task_subject")
        .or_else(|| input.get("task_subject"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    // Bidirectional task flow (2026-04-13): detect adoption of Touring-originated task.
    // When CC's TaskCreate carries `external_ref` pointing to a pre-existing Touring
    // DAG entry, we mark that entry as mirrored (breaks loop) and skip mirror +
    // auto-subtasks creation — those already exist upstream.
    let external_ref = tool_input
        .get("external_ref")
        .or_else(|| input.get("external_ref"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty());

    let mut parts = Vec::new();

    // F0-pre (2026-07-20): all decompose addressing goes through the mirror id
    // (`cc_task_<id>`), the ONE convention shared with bridge_task_created and
    // task_update — scaffold subtasks previously attached to the raw CC id
    // while the container was created under a generated nanos id, so mirrors
    // never showed their subtasks.
    let mirror_id = crate::hook_decompose_bridge::cc_mirror_task_id(task_id);

    // Subtask IDs used downstream by persist_task_creation regardless of path —
    // in adoption path they reference existing Touring subtasks, in standard path
    // they're created immediately below.
    let s1 = format!("{mirror_id}::scout");
    let s2 = format!("{mirror_id}::implement");
    let s3 = format!("{mirror_id}::validate");

    if let Some(ext_id) = external_ref {
        let mark_result = crate::cli_handlers::cli_decompose_mark_mirrored(
            rt,
            &serde_json::json!({"task_id": ext_id}),
        );
        tracing::debug!(task_id = task_id, external_ref = ext_id, result = %mark_result, "task_sync_create: adoption path");
        parts.push(format!(
            "touring-sync: adopted external task {ext_id} (mirrored_to_cc=1) — cc_task_id={task_id}, skipping DAG mirror + auto-subtasks"
        ));
    } else {
        // Standard path: CC-originated task — create mirror DAG + scaffold subtasks.
        // R12-S1: Call run_task_created directly — real sync bridge.
        let merged = serde_json::json!({
            "task_id": task_id,
            "task_subject": task_subject,
            "session_id": format!("cc-{}", &task_id[..task_id.len().min(20)]),
            "teammate_name": "claude-code",
            "team_name": "claude-code-tasks",
        });
        if let Err(e) = crate::team_hooks::run_task_created(rt, &merged) {
            tracing::debug!(error = %e, task_id = task_id, "task_sync_create: run_task_created failed");
        }

        // F0-pre: the scout→implement→validate scaffold now lives in
        // bridge_task_created (the convergence point of BOTH sync paths), so the
        // run_task_created call above already scaffolded the mirror. The s1/s2/s3
        // ids remain the contract consumed by persist_task_creation below.

        parts.push(format!(
            "touring-sync: decompose task registered for task_id={mirror_id} (applied) — run `touring decompose get {mirror_id}` to see DAG entry"
        ));
        parts.push(format!(
            "scaffolded: {mirror_id}::scout → {mirror_id}::implement → {mirror_id}::validate (3 subtasks, DAG ready)"
        ));
        // Pln3 R1: real-time CILA classification — emit plan_mode suggestion immediately
        // for L4+ tasks instead of waiting for next session_start digest cycle.
        if let Some(hint) = maybe_emit_plan_mode_suggestion(rt, task_id, task_subject) {
            parts.push(hint);
        }
    }
    // R22-S1: Surface a concrete GeneratorKind hint via SUBJECT_KEYWORD_MAP (R20-S3).
    // Replaces the fake `touring_generator_suggest_plan` call with an actual CLI command.
    let gen_hint = suggest_generator_for_task_subject(task_subject);
    if !gen_hint.is_empty() {
        // gen_hint format: " | generator: `touring generate render <Kind> ...` suggested"
        // Strip leading " | " so it fits into parts array (joined with " | ").
        parts.push(gen_hint.trim_start_matches(" | ").to_string());
        // R30-S1: Also emit a ready-to-paste GeneratorPlan JSON stub for immediate submission.
        if let Some(scaffold) = plan_scaffold_for_subject(task_subject, task_id) {
            parts.push(scaffold);
        }
    } else if !task_subject.is_empty() {
        let s = &task_subject[..task_subject.len().min(80)];
        parts.push(format!(
            "generator: run `touring generate plan-suggest --intent \"{s}\"` to scaffold artifacts"
        ));
    }
    // R41-S3: Surface wiring suggest hint for task subject — exposes orphan symbols early.
    // Bridges TaskCreate → wiring analysis before implementation starts.
    let wiring_hint = task_create_wiring_hint(task_subject);
    if !wiring_hint.is_empty() {
        parts.push(wiring_hint);
    }
    // Grupo 5 §1: JDM routing — classify task subject into execution class A/B/C/D.
    let jdm_hint = jdm_routing_hint(task_subject);
    if !jdm_hint.is_empty() {
        parts.push(jdm_hint);
    }
    // Grupo 3 §1: Task sharding — detect compound goals and suggest atomic subtasks.
    let shard_hint = task_sharding_hint(task_id, task_subject);
    if !shard_hint.is_empty() {
        parts.push(shard_hint);
    }
    // R10-C: Surface ready subtasks immediately after task creation.
    parts.push(format!(
        "ready-subtasks: run `touring decompose ready {task_id}` to see immediately actionable subtasks"
    ));

    // R45-S1: Emit task_scaffold template render command — bridges TaskCreate → touring-generator.
    // Gives Claude Code a ready-to-paste command for the full YAML task plan.
    let scaffold_yaml = task_scaffold_render_hint(task_id, task_subject);
    if !scaffold_yaml.is_empty() {
        parts.push(scaffold_yaml);
    }
    // R49-R52-S1: Collect all GeneratorKind hints from task subject keyword matchers.
    // Extracted to `collect_subject_generator_hints` — reduces CC of this function by 3.
    parts.extend(collect_subject_generator_hints(task_subject));

    // R137: Plan-recall hint — surfaces reusable GeneratorPlans from past similar tasks.
    // TaskCreate(subject) → plan-recall → plan-replay → new artifact without starting from scratch.
    // Closes the cross-session plan reuse loop: new task sees past plans BEFORE coding starts.
    // Complements plan_scaffold_for_subject (generates stub) by surfacing real past plans.
    if task_subject.len() > 3 {
        let short_subject = &task_subject[..task_subject.len().min(60)];
        parts.push(format!(
            "plan-reuse: run `touring generate plan-recall --query \"{short_subject}\"` \
            to find and replay existing GeneratorPlan for this task type"
        ));
    }

    // R139: Gotcha check hint — surfaces known pitfalls for the task subject before starting.
    // TaskCreate(subject) → gotcha list → known pitfalls surfaced BEFORE implementation begins.
    // Derives a file stem from the subject and suggests `touring gotcha list` to find pitfalls.
    // Prevents repeating known mistakes: gotcha DB accumulates patterns from past failures.
    let gotcha_hint = task_create_gotcha_hint(task_subject);
    if !gotcha_hint.is_empty() {
        parts.push(gotcha_hint);
    }

    // R18-S1 + R26-S2: Delegate Tantivy upsert + memory persistence to helper (CC reduced).
    persist_task_creation(rt, task_id, task_subject, &s1, &s2, &s3);

    // R163: Store session→task reverse mapping for cross-session CC task recall.
    // `persist_task_creation` stores `task:<task_id>:created` (task→subject) but no reverse index.
    // After context compaction, `touring memory recall "task:<id>:session"` can recover which
    // CC session created the task — closing the cross-session task continuity gap.
    // Key format: task:<task_id>:session — distinct from task:<task_id>:created (R18-S1).
    {
        let session_id = format!("cc-{}", &task_id[..task_id.len().min(20)]);
        let subject_snip = if task_subject.len() > 3 {
            &task_subject[..task_subject.len().min(60)]
        } else {
            "no-subject"
        };
        let _ = crate::cli_handlers::cli_memory_store(
            rt,
            &serde_json::json!({
                "key": format!("task:{task_id}:session"),
                "value": format!(
                    "CC task {task_id} belongs to session {session_id} | subject: {subject_snip} | dag: ::scout\u{2192}::implement\u{2192}::validate"
                ),
                "tier": "semantic",
                "entry_type": "lesson",
            }),
        );
    }

    // R144: Inject RL reward on task creation — closes the task-create → RL feedback loop.
    // TaskCreate with a non-trivial subject is a positive planning signal: the engineer decomposed
    // work into a tracked task. Reward 0.15 reinforces task-decomposition behavior in the RL engine.
    // Silent for trivially short subjects (≤3 chars) — avoids rewarding noise.
    if task_subject.len() > 3 {
        let context = format!("task:create:{}", &task_id[..task_id.len().min(20)]);
        let _ = crate::cli_handlers::cli_learning_reward(
            rt,
            &serde_json::json!({
                "tool_name": "orchestrate",
                "reward_value": 0.15,
                "context": context,
            }),
        );
    }

    parts.join(" | ")
}

/// R41-S3: Surface wiring suggest hint for the task subject on TaskCreate (CC≤2).
///
/// After scaffolding the DAG, emits `touring wiring suggest <stem>` for the task subject
/// so orphan symbols related to the new task surface immediately. This bridges TaskCreate
/// → wiring analysis — Claude Code sees integration opportunities without an extra round-trip.
/// Returns empty string when task_subject is empty.
pub(crate) fn task_create_wiring_hint(task_subject: &str) -> String {
    if task_subject.is_empty() {
        return String::new();
    }
    // Derive plausible file stem: lowercase, spaces → underscores, first 30 chars.
    let stem: String = task_subject
        .chars()
        .take(30)
        .map(|c| {
            if c.is_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect();
    format!(
        "wiring-opportunities: run `touring wiring suggest {stem}` to find orphan symbols \
        related to this task before coding starts"
    )
}

/// Grupo 5 §1 — Joint Domain Mapper (JDM) routing hint on TaskCreate.
///
/// Classifies the task subject into one of four execution classes via keyword scoring
/// and emits a `[TOURING JDM]` hint that suggests the optimal tooling path:
/// - class-A: code implementation → `touring generate plan-suggest`
/// - class-B: infra/devops → `touring jobs spawn`
/// - class-C: cognitive/architecture → `touring mcts search`
/// - class-D: multi-agent/orchestration → subagent-start + `touring decompose ready`
///
/// Priority: D > C > B > A. Records the winning class in gate metrics.
/// Returns empty string for trivially short subjects (≤3 chars).
pub(crate) fn jdm_routing_hint(subject: &str) -> String {
    if subject.len() <= 3 {
        return String::new();
    }
    let lower = subject.to_lowercase();

    // Priority D — multi-agent/orchestration keywords
    const CLASS_D: &[&str] = &[
        "parallel",
        "concurrent",
        "distribute",
        "orchestrate",
        "multi-agent",
        "fan-out",
        "coordinate",
        "delegate",
        "subagent",
    ];
    // Priority C — cognitive/architecture keywords
    const CLASS_C: &[&str] = &[
        "design",
        "architect",
        "plan",
        "analyze",
        "review",
        "model",
        "evaluate",
        "assess",
        "research",
        "strategy",
        "explore",
    ];
    // Priority B — infra/devops keywords
    const CLASS_B: &[&str] = &[
        "deploy",
        "infrastructure",
        "terraform",
        "docker",
        "kubernetes",
        "k8s",
        "ci",
        "devops",
        "pipeline",
        "helm",
        "provision",
    ];
    // Priority A — code implementation keywords (lowest priority)
    const CLASS_A: &[&str] = &[
        "implement",
        "refactor",
        "write",
        "add",
        "create",
        "fix",
        "generate",
        "build",
        "update",
        "migrate",
        "code",
        "patch",
    ];

    let matches = |keywords: &[&str]| keywords.iter().any(|kw| lower.contains(kw));

    if matches(CLASS_D) {
        crate::shared::gate_metrics::record_jdm_class_d();
        return "jdm-routing: [TOURING JDM] class-D (multi-agent) \u{2014} \
            spawn parallel subagents + run `touring decompose ready` to get independent subtasks"
            .to_string();
    }
    if matches(CLASS_C) {
        crate::shared::gate_metrics::record_jdm_class_c();
        return "jdm-routing: [TOURING JDM] class-C (cognitive) \u{2014} \
            run `touring mcts search` before implementation to explore solution space"
            .to_string();
    }
    if matches(CLASS_B) {
        crate::shared::gate_metrics::record_jdm_class_b();
        return "jdm-routing: [TOURING JDM] class-B (infra) \u{2014} \
            run `touring jobs spawn` to execute infra commands as background workers"
            .to_string();
    }
    if matches(CLASS_A) {
        crate::shared::gate_metrics::record_jdm_class_a();
        return format!(
            "jdm-routing: [TOURING JDM] class-A (code) \u{2014} \
            run `touring generate plan-suggest --intent \"{s}\"` to scaffold artifacts",
            s = &subject[..subject.len().min(60)]
        );
    }
    String::new()
}

/// Grupo 3 §1: Detect high-entropy task subjects and suggest atomic shards (Task Sharding).
///
/// Scores the subject for compound-goal signals: connectives ("and", "also"), multiple
/// action verbs, and length. When entropy score >= 2, emits a shard topology hint so
/// Claude Code decomposes the work into atomic subtasks instead of attempting the
/// macro-task linearly (which collapses the context window).
///
/// Returns an empty string for simple, single-goal subjects.
pub(crate) fn task_sharding_hint(task_id: &str, subject: &str) -> String {
    if subject.len() <= 10 {
        return String::new();
    }
    let lower = subject.to_lowercase();

    // Connective signals: each detected word adds 1 entropy point.
    const CONNECTIVES: &[&str] = &[
        " and ",
        " also ",
        " then ",
        " as well",
        " plus ",
        " while also",
        " additionally",
        " furthermore",
        ", and ",
        ", plus ",
    ];
    let connective_count = CONNECTIVES.iter().filter(|c| lower.contains(*c)).count();

    // Action verb signals: count distinct verbs found.
    const ACTION_VERBS: &[&str] = &[
        "implement",
        "add",
        "create",
        "fix",
        "refactor",
        "update",
        "migrate",
        "write",
        "generate",
        "test",
        "deploy",
        "remove",
        "delete",
        "move",
        "extract",
    ];
    let verb_count = ACTION_VERBS.iter().filter(|v| lower.contains(*v)).count();

    // Length signal: subjects over 80 chars are likely compound.
    let length_signal = if subject.len() > 80 { 1 } else { 0 };

    // Entropy: connectives dominate; verb count and length are tie-breakers.
    let entropy = connective_count + if verb_count >= 2 { 1 } else { 0 } + length_signal;

    if entropy < 2 {
        return String::new();
    }

    // Build shard topology: split on primary connective or by verb boundaries.
    let shard_count = (connective_count + 1).min(4); // cap at 4 shards
    let short_subject = &subject[..subject.len().min(60)];
    format!(
        "task-sharding: [TOURING SHARD] entropy={} compound_goals≈{} — \
         split `{short_subject}…` into {} atomic subtasks: \
         run `touring decompose add {task_id} {task_id}::shard-1 \"<goal-1>\"` \
         (repeat for each shard). Atomic shards prevent context collapse on L3+ tasks.",
        entropy, shard_count, shard_count
    )
}

/// R139: Surface known pitfalls from the gotcha DB for the task subject on TaskCreate (CC≤2).
///
/// Derives a file stem from the task subject and emits a `touring gotcha list` command
/// so the engineer sees known pitfalls BEFORE starting implementation. Closes the loop:
/// TaskCreate(subject) → gotcha check → failure prevention before any code is written.
/// Complements R41-S3 (wiring) by adding failure-pattern awareness at task creation time.
/// Returns empty string when task_subject is empty or trivially short.
pub(crate) fn task_create_gotcha_hint(task_subject: &str) -> String {
    if task_subject.len() <= 3 {
        return String::new();
    }
    // Derive plausible file stem: lowercase, spaces → underscores, first 30 chars.
    let stem: String = task_subject
        .chars()
        .take(30)
        .map(|c| {
            if c.is_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect();
    format!(
        "gotcha-check: run `touring gotcha list --file {stem}` to surface known pitfalls \
        for this task type before implementation begins"
    )
}

/// R45-S1: Emit a ready-to-paste `task_scaffold` Tera template render command on TaskCreate (CC=2).
///
/// After the DAG is scaffolded (scout→implement→validate), this bridges TaskCreate
/// directly to the touring-generator `task_scaffold` template, so the full YAML
/// task plan (with CLI bootstrap commands) is one command away.
/// Returns empty string when task_id is sentinel or task_subject is empty.
pub(crate) fn task_scaffold_render_hint(task_id: &str, task_subject: &str) -> String {
    if task_id.is_empty() || task_id == "unknown" || task_subject.is_empty() {
        return String::new();
    }
    let intent = &task_subject[..task_subject.len().min(50)];
    format!(
        "scaffold-yaml: run `touring generate render task_scaffold \
        --vars '{{\"task_id\":\"{task_id}\",\"intent\":\"{intent}\",\"phase\":\"implementation\"}}' -j` \
        to export full YAML task plan via touring-generator"
    )
}

// ── Wave C2 inversion (2026-06-10): subject GeneratorKind hint matchers ──────
// The 30 `maybe_*_hint_on_task_create` matchers + `collect_subject_generator_hints`
// dispatcher moved to touring-hooks-core::generator_hints (pure &str -> Option<String>,
// consumed by cli/decompose.rs in touring-cli and hook_registry here). Glob
// re-export keeps every existing path (`super::maybe_*`, `crate::lifecycle::*`).
pub(crate) use touring_hooks_core::generator_hints::*;

// ── Pln3 R1: real-time plan_mode emission (CC=4) ─────────────────────────────

/// Emit a `plan_mode` action suggestion immediately when a newly-created task
/// scores CILA L4+ at creation time.
///
/// Returns an optional hint string to include in `additionalContext` parts.
/// Extracted from `handle_task_sync_post_create` to keep that function CC ≤ 15.
pub(crate) fn maybe_emit_plan_mode_suggestion(
    rt: &mut crate::runtime::HookRuntime,
    task_id: &str,
    task_subject: &str,
) -> Option<String> {
    let cila = crate::suggesters::plan_mode_complexity::classify_complexity_for(task_subject);
    if cila < 4 {
        return None;
    }
    let evidence = serde_json::json!({
        "trigger": "realtime_task_create",
        "cila_level": cila,
        "subject_preview": &task_subject[..task_subject.len().min(80)],
    });
    let _ = crate::cli_handlers::cli_suggest_action(
        rt,
        &serde_json::json!({
            "action_type": "plan_mode",
            "target_task_id": task_id,
            "reason": format!("CILA L{cila} detected at creation — consider plan mode before edits"),
            "evidence_json": evidence,
        }),
    );
    Some(format!(
        "plan-mode-hint: CILA L{cila} — consider EnterPlanMode before implementing this task"
    ))
}
