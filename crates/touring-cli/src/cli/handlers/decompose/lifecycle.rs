//! Ciclo de vida do DAG — finalizacao, retencao e reconciliacao
//!
//! Extraido de `handlers/decompose.rs` em 21/09/2026: o arquivo tinha 2450
//! linhas e reprovava F1.2 (manutenibilidade) desde antes deste trabalho. O
//! corte e por COESAO — este modulo guarda o que FECHA uma task: finalize, o carimbo de arquivo e a
//! reconciliacao dos estagios do espelho.

use super::*;
/// Recalcula o status do plano a partir das subtarefas — `done` só quando TODAS
/// são terminais.
///
/// O vocabulário é misto por herança: `completed` (closes legados) e `done`
/// (`loop_phase_close`) são ambos terminais, junto de `finalized`; tratar um
/// deles como pendente tornaria a conclusão inalcançável (o mesmo cuidado que
/// `loop_converged.py::clause_dag` já documenta). Um plano sem subtarefas não é
/// tocado: não há filhos de onde derivar, e sobrescrevê-lo apagaria o status que
/// o chamador definiu explicitamente.
pub(crate) fn refresh_parent_status(db: &FileKnowledgeDB, task_id: &str, now: &str) -> usize {
    const TERMINAL: [&str; 3] = ["done", "completed", "finalized"];
    let statuses: Vec<String> = {
        let Ok(mut stmt) = db
            .conn_ref()
            .prepare("SELECT status FROM decomposition_subtasks WHERE task_id = ?1")
        else {
            return 0;
        };
        let Ok(rows) = stmt.query_map(params![task_id], |row| row.get::<_, String>(0)) else {
            return 0;
        };
        rows.filter_map(Result::ok).collect()
    };
    if statuses.is_empty() {
        return 0;
    }
    let terminal = |s: &String| TERMINAL.contains(&s.as_str());
    let derived = if statuses.iter().all(terminal) {
        "done"
    } else if statuses.iter().any(|s| terminal(s) || s == "in_progress") {
        "in_progress"
    } else {
        "pending"
    };
    db.conn_ref()
        .execute(
            "UPDATE task_decompositions SET status = ?1, updated_at = ?3 WHERE task_id = ?2",
            params![derived, task_id, now],
        )
        .unwrap_or(0)
}

/// S1.6: Take a checkpoint snapshot of the task state before finalizing.
/// Writes to `decomposition_snapshots` table for post-mortem/replay.
fn take_snapshot(db: &FileKnowledgeDB, task_id: &str, metrics: &serde_json::Value) {
    let snapshot_id = format!("snap_{}", uuid::Uuid::new_v4());

    // Capture full subtasks snapshot
    let subtasks_snapshot: Vec<serde_json::Value> = {
        let mut stmt = match db.conn_ref().prepare(
            "SELECT subtask_id, description, depends_on, priority, status, deadline,
                    deadline_behavior, review_required, quality_score, attempts,
                    parallel_group, created_at, updated_at
             FROM decomposition_subtasks WHERE task_id = ?1",
        ) {
            Ok(s) => s,
            Err(_) => return,
        };
        stmt.query_map(params![task_id], |r| {
            Ok(serde_json::json!({
                "subtask_id": r.get::<_, String>(0)?,
                "description": r.get::<_, String>(1)?,
                "depends_on": r.get::<_, String>(2)?,
                "priority": r.get::<_, i32>(3)?,
                "status": r.get::<_, String>(4)?,
                "deadline": r.get::<_, Option<String>>(5)?,
                "deadline_behavior": r.get::<_, Option<String>>(6)?,
                "review_required": r.get::<_, i32>(7)?,
                "quality_score": r.get::<_, Option<f64>>(8)?,
                "attempts": r.get::<_, i32>(9)?,
                "parallel_group": r.get::<_, Option<String>>(10)?,
                "created_at": r.get::<_, String>(11)?,
                "updated_at": r.get::<_, String>(12)?
            }))
        })
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default()
    };

    let _ = db.conn_ref().execute(
        "INSERT INTO decomposition_snapshots (snapshot_id, task_id, subtasks_snapshot, metrics_snapshot) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![
            snapshot_id,
            task_id,
            serde_json::to_string(&subtasks_snapshot).unwrap_or_default(),
            serde_json::to_string(metrics).unwrap_or_default()
        ],
    );
}

/// Finalizes a task's DAG, locking it for execution once its structure is validated, returning the result as JSON.
pub fn cli_decompose_finalize(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    // S1.5: review_required gate + S1.7: metrics population on finalize
    let task_id = payload
        .get("task_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let quality_threshold = payload.get("quality_threshold").and_then(|v| v.as_f64());

    let db = &rt.ctx.knowledge;
    ensure_decompose_tables(db);
    let now = chrono::Utc::now().to_rfc3339();

    // S1.6: Run deadline check before finalizing (apply Fail/Skip/Notify/Backburner)
    let breached = check_deadlines(db, task_id);

    // Fetch all subtasks to check review_required gate and compute metrics
    let subtasks: Vec<serde_json::Value> = {
        let mut stmt = match db.conn_ref().prepare(
            "SELECT subtask_id, review_required, quality_score, status FROM decomposition_subtasks WHERE task_id = ?1",
        ) {
            Ok(s) => s,
            Err(e) => return serde_json::json!({"error": format!("db error: {}", e)}).to_string(),
        };
        stmt.query_map(params![task_id], |r| {
            Ok(serde_json::json!({
                "subtask_id": r.get::<_, String>(0)?,
                "review_required": r.get::<_, i32>(1)?,
                "quality_score": r.get::<_, Option<f64>>(2)?,
                "status": r.get::<_, String>(3)?
            }))
        })
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default()
    };

    // S1.5: Gate — reject if any review_required subtask lacks quality_score
    // or if quality_score is below the threshold when one is provided
    for st in &subtasks {
        let review_required = st
            .get("review_required")
            .and_then(|v| v.as_i64())
            .unwrap_or(0)
            == 1;
        let quality_score = st.get("quality_score").and_then(|v| v.as_f64());
        if review_required {
            if quality_score.is_none() {
                return serde_json::json!({
                    "error": "Subtask requires review before completion",
                    "blocking_subtask": st["subtask_id"]
                })
                .to_string();
            }
            if let Some(qt) = quality_threshold
                && let Some(qs) = quality_score
                && qs < qt
            {
                return serde_json::json!({
                    "error": "Subtask quality_score below threshold",
                    "blocking_subtask": st["subtask_id"],
                    "quality_score": qs,
                    "quality_threshold": qt
                })
                .to_string();
            }
        }
    }

    // S1.7: Compute and persist task metrics
    let total_subtasks = subtasks.len() as i64;
    let completed = subtasks
        .iter()
        .filter(|s| s.get("status").and_then(|v| v.as_str()) == Some("completed"))
        .count() as i64;
    let failed = subtasks
        .iter()
        .filter(|s| s.get("status").and_then(|v| v.as_str()) == Some("failed"))
        .count() as i64;
    let avg_quality: Option<f64> = {
        let sum: f64 = subtasks
            .iter()
            .filter_map(|s| s.get("quality_score").and_then(|v| v.as_f64()))
            .sum();
        let count = subtasks
            .iter()
            .filter(|s| s.get("quality_score").and_then(|v| v.as_f64()).is_some())
            .count() as f64;
        if count > 0.0 { Some(sum / count) } else { None }
    };
    let completion_pct = if total_subtasks > 0 {
        (completed as f64 / total_subtasks as f64) * 100.0
    } else {
        0.0
    };

    let metrics = serde_json::json!({
        "total_subtasks": total_subtasks,
        "completed": completed,
        "failed": failed,
        "pending": total_subtasks - completed - failed,
        "avg_quality": avg_quality,
        "completion_pct": completion_pct,
        "breached_deadlines": breached
    });

    // S1.6: Take checkpoint snapshot before archiving
    take_snapshot(db, task_id, &metrics);

    // Finalizing IS archiving: the same write stamps both, because the two were
    // split before and never met — `finalize` wrote `status = 'finalized'` while
    // the retention routine looked for `status = 'completed'`, so `archived_at`
    // stayed NULL on every one of the 381 tasks (measured 19/09/2026) and every
    // query that filters on the archive read the whole history as live.
    let archived = db
        .conn_ref()
        .execute(
            "UPDATE task_decompositions SET status = 'finalized', metrics = ?1, updated_at = ?2, \
             archived_at = COALESCE(archived_at, ?2) WHERE task_id = ?3",
            params![serde_json::to_string(&metrics).unwrap_or_default(), now, task_id],
        )
        .is_ok_and(|rows| rows > 0);

    // S1.4: Wire event audit trail for task finalization
    log_event(db, task_id, None, "task_finalized", &metrics);

    serde_json::json!({
        "task_id": task_id,
        "status": "finalized",
        // The `task-completed` hook gates a log line on `archived:true`. The field
        // was never emitted, so that branch reported "not archived" forever — a
        // consumer waiting on a field the producer never wrote.
        "archived": archived,
        "archived_at": if archived { Some(now.clone()) } else { None },
        "metrics": metrics,
        "breached_deadlines": breached
    })
    .to_string()
}

/// Stamp `archived_at` on terminal tasks that never got one — the retention
/// pass, finally reachable.
///
/// `CheckpointStore::archive_completed_tasks` has done this since forever and
/// had **zero production callers**: only its own tests ever ran it (verified
/// 19/09/2026). So even after its predicate was corrected, nothing would have
/// invoked it, and the 110 terminal tasks carrying a NULL `archived_at` would
/// have stayed that way. This is the route (REGRA #0: an existing capability
/// with no caller gets wired, not deleted).
///
/// Payload: `{older_than_secs?: u64, dry_run?: bool}`. `older_than_secs`
/// defaults to 0, meaning every terminal task regardless of age; `dry_run`
/// reports the count without writing.
pub fn cli_decompose_archive(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let older_than_secs = payload
        .get("older_than_secs")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    let dry_run = payload
        .get("dry_run")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);

    let db = &rt.ctx.knowledge;
    ensure_decompose_tables(db);

    let cutoff = chrono::Utc::now() - chrono::Duration::seconds(older_than_secs as i64);
    let cutoff_str = cutoff.to_rfc3339();
    // The SAME terminal-status list `finalize` and the retention routine use.
    let terminal = touring_foundation::task_lifecycle::terminal_status_sql_list();

    let where_clause = format!(
        "status IN ({terminal}) AND archived_at IS NULL AND updated_at < ?1"
    );
    let candidates: i64 = db
        .conn_ref()
        .query_row(
            &format!("SELECT COUNT(*) FROM task_decompositions WHERE {where_clause}"),
            params![cutoff_str],
            |r| r.get(0),
        )
        .unwrap_or(0);

    if dry_run {
        return serde_json::json!({
            "archived": 0,
            "candidates": candidates,
            "dry_run": true,
            "older_than_secs": older_than_secs,
        })
        .to_string();
    }

    let now = chrono::Utc::now().to_rfc3339();
    // `?1` is the cutoff (it appears inside `where_clause`); the stamp is `?2`.
    // Reusing `?1` for both bound the wrong value AND passed an extra
    // parameter, which rusqlite rejects — and an `unwrap_or(0)` then reported
    // "archived 0" instead of the error. The count and the write must agree.
    let outcome = db.conn_ref().execute(
        &format!("UPDATE task_decompositions SET archived_at = ?2 WHERE {where_clause}"),
        params![cutoff_str, now],
    );
    let archived = match outcome {
        Ok(rows) => rows,
        Err(e) => {
            // Never answer "0 archived" for a write that did not run: that
            // reads as "nothing to do" and hides a broken retention pass.
            return serde_json::json!({
                "error": format!("archive failed: {e}"),
                "candidates": candidates,
                "archived": 0,
            })
            .to_string();
        }
    };

    serde_json::json!({
        "archived": archived,
        "candidates": candidates,
        "dry_run": false,
        "older_than_secs": older_than_secs,
    })
    .to_string()
}

/// Close scaffold stages whose every sibling already finished.
///
/// The fixed closer ([`crate::hook_decompose_bridge::close_scaffold_stages`])
/// only runs when a task emits a completion event. 74 `::scout` rows predate it
/// and sit under mirrors that never emitted one, so nothing will ever call the
/// closer for them.
///
/// The deduction is the DAG's own edge: `implement` depends on `scout`, and
/// `validate` on `implement`. A stage with NO open sibling is therefore a stage
/// whose work the others already outlived, and is closed.
///
/// Cross-audit 20/09/2026 — "sem irmão aberto" sozinho é o INVERSO do que o
/// doc dizia. A cadeia é LINEAR, então o estágio sem irmão aberto é o ÚLTIMO
/// da fila: numa task que progride normalmente, é exatamente o que está
/// EXECUTANDO agora. Quatro guardas fecham isso, e nenhuma delas é opinião:
///
/// 1. **Só estágio do espelho.** O nome do comando promete scaffold; o
///    predicado antigo alcançava qualquer subtask, e 4 dos 5 `in_progress`
///    vivos eram fases de plano (`::A1`, `::D-01`, `::F1`).
/// 2. **Nunca `in_progress`.** Um estado que declara execução não é resíduo.
/// 3. **Nunca sob reivindicação ativa.** `claimed_by` + `claim_expires_at`
///    existem desde o claim atômico da v30.4.1 e o predicado os ignorava:
///    fechar por dedução sobre uma lease viva sabota a sessão que a detém.
/// 4. **Exige um irmão TERMINAL.** É a prova de que a task PROGREDIU. Sem
///    ela, `NOT EXISTS` é trivialmente verdadeiro numa task de subtask único
///    (3 existem no banco), que seria fechada recém-criada.
///
/// Payload: `{dry_run?: bool}`.
pub fn cli_decompose_reconcile_stages(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let dry_run = payload
        .get("dry_run")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);

    let db = &rt.ctx.knowledge;
    ensure_decompose_tables(db);
    let terminal = terminal_subtask_status_sql_list();

    // "Open" is the complement of terminal, so the predicate is written
    // negatively against the enumerable side.
    //
    // Os sufixos saem de `MIRROR_SCAFFOLD_STAGES` — interpolar é seguro porque
    // são constantes de compilação, e derivá-los da lista é o que faz um
    // estágio novo ser reconciliado sem tocar neste SQL.
    let stage_filter = touring_foundation::task_lifecycle::MIRROR_SCAFFOLD_STAGES
        .iter()
        .map(|stage| format!("s.subtask_id LIKE '%::{}'", stage.name))
        .collect::<Vec<_>>()
        .join(" OR ");
    let now_epoch = chrono::Utc::now().timestamp();
    let select = format!(
        "SELECT s.subtask_id FROM decomposition_subtasks s \
         WHERE s.status NOT IN ({terminal}) \
           AND s.status <> 'in_progress' \
           AND ({stage_filter}) \
           AND (s.claimed_by IS NULL OR s.claimed_by = '' \
                OR COALESCE(s.claim_expires_at, 0) < {now_epoch}) \
           AND NOT EXISTS ( \
             SELECT 1 FROM decomposition_subtasks o \
             WHERE o.task_id = s.task_id AND o.subtask_id != s.subtask_id \
               AND o.status NOT IN ({terminal}) ) \
           AND EXISTS ( \
             SELECT 1 FROM decomposition_subtasks t \
             WHERE t.task_id = s.task_id AND t.subtask_id != s.subtask_id \
               AND t.status IN ({terminal}) )"
    );
    let candidates: Vec<String> = {
        let mut stmt = match db.conn_ref().prepare(&select) {
            Ok(s) => s,
            Err(e) => return serde_json::json!({"error": format!("db error: {e}")}).to_string(),
        };
        stmt.query_map([], |r| r.get::<_, String>(0))
            .map(|rows| rows.filter_map(Result::ok).collect())
            .unwrap_or_default()
    };
    let sample: Vec<&String> = candidates.iter().take(5).collect();

    if dry_run {
        return serde_json::json!({
            "closed": 0,
            "candidates": candidates.len(),
            "dry_run": true,
            "sample": sample,
        })
        .to_string();
    }

    let now = chrono::Utc::now().to_rfc3339();
    let mut closed = 0_usize;
    for subtask_id in &candidates {
        match db.conn_ref().execute(
            "UPDATE decomposition_subtasks SET status = 'completed', updated_at = ?1 \
             WHERE subtask_id = ?2",
            params![now, subtask_id],
        ) {
            Ok(rows) => closed += rows,
            // Never answer "closed 0" for a write that failed — that reads as
            // "nothing to do" (the `unwrap_or(0)` lesson of 19/09/2026).
            Err(e) => {
                return serde_json::json!({
                    "error": format!("reconcile failed at {subtask_id}: {e}"),
                    "closed": closed,
                    "candidates": candidates.len(),
                })
                .to_string();
            }
        }
    }

    serde_json::json!({
        "closed": closed,
        "candidates": candidates.len(),
        "dry_run": false,
        "sample": sample,
    })
    .to_string()
}

/// The statuses that mean a SUBTASK is over, as a SQL `IN (...)` list.
///
/// Deliberately NOT `task_lifecycle::terminal_status_sql_list`: subtasks carry
/// their own vocabulary — `failed`, `skipped` and `cancelled` exist here and
/// not on task containers. Values are compile-time constants, so the
/// interpolation carries nothing a caller controls.
pub(crate) fn terminal_subtask_status_sql_list() -> String {
    TERMINAL_SUBTASK_STATUSES
        .iter()
        .map(|s| format!("'{s}'"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Every status a subtask can end in.
pub(crate) const TERMINAL_SUBTASK_STATUSES: [&str; 5] =
    ["completed", "done", "failed", "skipped", "cancelled"];

/// Validates a task's DAG for structural integrity, detecting dependency cycles and dangling references.
pub fn cli_decompose_validate(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let task_id = payload
        .get("task_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let db = &rt.ctx.knowledge;
    ensure_decompose_tables(db);

    let subtasks: Vec<(String, String)> = {
        let mut stmt = match db
            .conn_ref()
            .prepare("SELECT subtask_id, depends_on FROM decomposition_subtasks WHERE task_id = ?1")
        {
            Ok(s) => s,
            Err(e) => {
                tracing::debug!("decompose validate prepare failed: {}", e);
                return serde_json::json!({"valid": false, "error": format!("db error: {}", e)})
                    .to_string();
            }
        };
        stmt.query_map(params![task_id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default()
    };

    let mut graph: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    for (subtask_id, deps) in &subtasks {
        let dep_list: Vec<String> = serde_json::from_str(deps).unwrap_or_else(|_| {
            deps.split(',')
                .filter(|s| !s.is_empty())
                .map(String::from)
                .collect()
        });
        graph.insert(subtask_id.clone(), dep_list);
    }

    let mut visited: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut rec_stack: std::collections::HashSet<String> = std::collections::HashSet::new();

    fn has_cycle(
        node: &str,
        graph: &std::collections::HashMap<String, Vec<String>>,
        visited: &mut std::collections::HashSet<String>,
        rec_stack: &mut std::collections::HashSet<String>,
    ) -> bool {
        visited.insert(node.to_string());
        rec_stack.insert(node.to_string());
        if let Some(deps) = graph.get(node) {
            for dep in deps {
                if !visited.contains(dep) {
                    if has_cycle(dep, graph, visited, rec_stack) {
                        return true;
                    }
                } else if rec_stack.contains(dep) {
                    return true;
                }
            }
        }
        rec_stack.remove(node);
        false
    }

    let mut has_cycles = false;
    for node in graph.keys() {
        if !visited.contains(node) && has_cycle(node, &graph, &mut visited, &mut rec_stack) {
            has_cycles = true;
            break;
        }
    }

    serde_json::json!({
        "task_id": task_id,
        "valid": !has_cycles,
        "has_cycles": has_cycles,
        "subtask_count": subtasks.len()
    })
    .to_string()
}

/// Summarizes progress across all decomposition tasks (subtask counts by status) as JSON.
pub fn cli_decompose_status(rt: &mut HookRuntime, _payload: &serde_json::Value) -> String {
    let db = &rt.ctx.knowledge;
    ensure_decompose_tables(db);

    let total_tasks: i64 = db
        .conn_ref()
        .query_row("SELECT COUNT(*) FROM task_decompositions", [], |r| r.get(0))
        .unwrap_or(0);

    let total_subtasks: i64 = db
        .conn_ref()
        .query_row("SELECT COUNT(*) FROM decomposition_subtasks", [], |r| {
            r.get(0)
        })
        .unwrap_or(0);

    serde_json::json!({
        "total_tasks": total_tasks,
        "total_subtasks": total_subtasks
    })
    .to_string()
}

// ─────────────────────────────────────────────────────────────────────────────
// S1.8: parallel_groups — ready subtasks grouped by parallel_group
// ─────────────────────────────────────────────────────────────────────────────



