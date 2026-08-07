//! Plan introspection MCP tool helpers (diff / history / critique).
//!
//! Extracted from `generator_tools.rs` (F-9) to keep every source file < 2000 LOC.
//! The three public entry points are re-exported from `generator_tools` so the
//! `generator_tools::{diff_plans,plan_history,critique_plan}` call paths are unchanged.

use serde_json::Value;
use touring_generator::GeneratorPlan;

use crate::tools::generator_tools::parse_plan;

// ── diff ─────────────────────────────────────────────────────────────────────

/// `touring_generator_diff_plans` — compare two plan JSONs for key field differences.
pub fn diff_plans(plan_a_json: &str, plan_b_json: &str) -> Value {
    let plan_a = match parse_plan(plan_a_json) {
        Ok(p) => p,
        Err(e) => return serde_json::json!({"ok": false, "error": format!("plan_a: {e}")}),
    };
    let plan_b = match parse_plan(plan_b_json) {
        Ok(p) => p,
        Err(e) => return serde_json::json!({"ok": false, "error": format!("plan_b: {e}")}),
    };

    let mut diffs: Vec<Value> = Vec::new();
    collect_plan_diffs(&plan_a, &plan_b, &mut diffs);

    serde_json::json!({
        "ok": true,
        "diff_count": diffs.len(),
        "identical": diffs.is_empty(),
        "diffs": diffs,
        "plan_a_id": plan_a.plan_id.to_string(),
        "plan_b_id": plan_b.plan_id.to_string(),
    })
}

/// Collect field-level diffs between two plans (extracted for CC reduction).
fn collect_plan_diffs(a: &GeneratorPlan, b: &GeneratorPlan, diffs: &mut Vec<Value>) {
    if a.kind != b.kind {
        diffs.push(serde_json::json!({"field": "kind", "a": format!("{:?}", a.kind), "b": format!("{:?}", b.kind)}));
    }
    if a.intent != b.intent {
        diffs.push(serde_json::json!({"field": "intent", "a": &a.intent, "b": &b.intent}));
    }
    if a.target.file_path != b.target.file_path {
        diffs.push(serde_json::json!({"field": "target.file_path", "a": &a.target.file_path, "b": &b.target.file_path}));
    }
    if a.version != b.version {
        diffs.push(serde_json::json!({"field": "version", "a": &a.version, "b": &b.version}));
    }
}

// ── history ──────────────────────────────────────────────────────────────────

/// `touring_generator_plan_history` — show execution_trace lineage from the plan.
pub fn plan_history(plan_json: &str) -> Value {
    match parse_plan(plan_json) {
        Err(e) => serde_json::json!({"ok": false, "error": e.to_string()}),
        Ok(plan) => {
            let trace: Vec<Value> = plan
                .execution_trace
                .iter()
                .map(|e| {
                    serde_json::to_value(e)
                        .unwrap_or(serde_json::json!({"entry": "unserializable"}))
                })
                .collect();
            serde_json::json!({
                "ok": true,
                "plan_id": plan.plan_id.to_string(),
                "trace_count": trace.len(),
                "trace": trace,
            })
        }
    }
}

// ── critique ─────────────────────────────────────────────────────────────────

/// `touring_generator_critique_plan` — analyze plan structure and report issues.
pub fn critique_plan(plan_json: &str) -> Value {
    let plan = match parse_plan(plan_json) {
        Err(e) => return serde_json::json!({"ok": false, "error": e.to_string()}),
        Ok(p) => p,
    };

    let mut issues: Vec<Value> = Vec::new();
    collect_critique_issues(&plan, &mut issues);

    let error_count = issues.iter().filter(|i| i["severity"] == "error").count();
    let warning_count = issues.iter().filter(|i| i["severity"] == "warning").count();

    serde_json::json!({
        "ok": true,
        "plan_id": plan.plan_id.to_string(),
        "issue_count": issues.len(),
        "error_count": error_count,
        "warning_count": warning_count,
        "issues": issues,
        "critique_passed": error_count == 0,
    })
}

/// Collect critique issues from a plan (extracted for CC reduction).
fn collect_critique_issues(plan: &GeneratorPlan, issues: &mut Vec<Value>) {
    // Intent quality
    if plan.intent.trim().is_empty() {
        issues.push(serde_json::json!({"severity": "error", "field": "intent", "message": "intent is empty"}));
    } else if plan.intent.len() < 10 {
        issues.push(serde_json::json!({"severity": "warning", "field": "intent", "message": "intent is very short (< 10 chars)"}));
    }

    // Target path
    check_target_path(&plan.target.file_path, issues);

    // Contracts
    let has_contracts = !plan.contracts.symbols_must_exist.is_empty()
        || !plan.contracts.symbols_must_not_exist.is_empty()
        || !plan.contracts.files_must_exist.is_empty();
    if !has_contracts {
        issues.push(serde_json::json!({"severity": "info", "field": "contracts", "message": "no contracts defined — VGP will have nothing to verify"}));
    }

    // Execution trace
    if plan.execution_trace.is_empty() {
        issues.push(serde_json::json!({"severity": "info", "field": "execution_trace", "message": "no execution trace — plan has not been run yet"}));
    }

    // R3-S1,S2,S3: hooks intelligence enrichment (gotcha + coverage + related docs)
    collect_intelligence_critique(plan.target.file_path.as_str(), issues);
}

/// Enrich plan critique with touring-hooks intelligence signals.
///
/// Opens `FileKnowledgeDB` and queries three signal sources for the target path:
///
/// - **R3-S1** (`compose_pre_edit_warning`): Gotcha hit-counts + decay-weighted
///   cross-session error patterns. Surfaces recurring pitfalls before the plan
///   is committed to a problem-prone file.
/// - **R3-S2** (`query_extended`): FileKnowledgeEnriched 13-field enrichment:
///   `coverage_pct` (warns if < 50%) and `community_id` (module cluster context).
/// - **R3-S3** (`tantivy_related_docs_signal`): BM25 related-docs lookup —
///   surfaces symbols in other files whose docstrings share concepts with the
///   target, helping the reviewer spot unintended scope overlap.
///
/// All failures are silent (returns without pushing issues) — critique always
/// degrades gracefully when the DB or index is unavailable.
fn collect_intelligence_critique(target_path: &str, issues: &mut Vec<Value>) {
    if target_path.is_empty() {
        return;
    }

    // Open FileKnowledgeDB using same project-root resolution as knowledge_upsert_fn.
    let project_root = std::env::var("TOURING_PROJECT_ROOT").unwrap_or_else(|_| ".".to_string());
    let db_path = touring_foundation::TouringConfig::knowledge_db_canonical(std::path::Path::new(
        &project_root,
    ));
    let Ok(db) = touring_hooks::FileKnowledgeDB::new(&db_path) else {
        return;
    };

    // R3-S1: Pre-edit warning — gotchas + decay-weighted error patterns
    if let Some(warning) =
        touring_hooks::pre_edit_prevention::compose_pre_edit_warning(&db, target_path)
    {
        issues.push(serde_json::json!({
            "severity": "warning",
            "field": "target_file",
            "message": warning,
            "source": "hooks:pre_edit_warning",
        }));
    }

    // R3-S2: FileKnowledgeEnriched — coverage_pct + community_id
    if let Ok(Some(ext)) = db.query_extended(target_path) {
        if let Some(cov) = ext.coverage_pct {
            let (sev, msg) = if cov < 50.0 {
                (
                    "warning",
                    format!(
                        "target file coverage is {cov:.0}% — generator commit may lower test coverage"
                    ),
                )
            } else {
                ("info", format!("target file coverage: {cov:.0}%"))
            };
            issues.push(serde_json::json!({
                "severity": sev, "field": "coverage", "message": msg,
                "source": "hooks:query_extended",
            }));
        }
        if let Some(cid) = ext.community_id {
            issues.push(serde_json::json!({
                "severity": "info",
                "field": "community",
                "message": format!("target belongs to community {cid}"),
                "source": "hooks:query_extended",
            }));
        }
    }

    // R3-S3: Tantivy BM25 — related docs in other files (tantivy-fts always enabled
    // for touring-hooks in touring-server, so no cfg guard needed here)
    // A raiz é DERIVADA do próprio alvo em vez de atravessar a cadeia:
    // `critique_plan` é API pública sem raiz, e enfiar um parâmetro por três
    // níveis para um consumidor advisory seria desproporcional.
    // `normalize_project_root` faz walk-up por marcador real quando o caminho é
    // absoluto; num caminho relativo ele devolve `$HOME`, que resolve para o
    // índice legado — degradação explícita, não silenciosa.
    let derived_root = target_path.starts_with('/').then(|| {
        touring_foundation::TouringConfig::normalize_project_root(std::path::Path::new(target_path))
    });
    if let Some((_, signal)) = touring_hooks::shared::signals::tantivy_related_docs_signal(
        derived_root.as_deref(),
        target_path,
    ) {
        issues.push(serde_json::json!({
            "severity": "info",
            "field": "related_docs",
            "message": signal,
            "source": "hooks:tantivy_related_docs",
        }));
    }
}

/// Check target path validity (extracted for CC reduction).
fn check_target_path(file_path: &str, issues: &mut Vec<Value>) {
    if file_path.is_empty() {
        issues.push(serde_json::json!({"severity": "error", "field": "target.file_path", "message": "target path is empty"}));
        return;
    }
    let target = std::path::Path::new(file_path);
    if target.is_absolute()
        && let Some(parent) = target.parent()
        && !parent.exists()
    {
        issues.push(serde_json::json!({
            "severity": "warning",
            "field": "target.file_path",
            "message": format!("parent directory does not exist: {}", parent.display()),
        }));
    }
}
