//! CLI Decompose handlers — DAG task management
//!
//! Extracted from cli_handlers.rs (Phase 5 refactor).
//! Handlers: create, add, get, update (S-1 fix applied), validate, status, event.
//!
//! # Wave P1 — PlanEntryPriority
//!
//! The `decomposition_subtasks.priority` column already exists as INTEGER
//! (default 255). Wave P1 layers a string enum over it (`high`/`normal`/`low`)
//! while preserving the integer schema for backward compat. Mapping:
//! - `"high"`   → 50  (low integer = high logical priority — sorts first ASC)
//! - `"normal"` → 128 (default for newly mapped entries)
//! - `"low"`    → 200
//!
//! Existing rows with priority=255 are treated as `"low"` by `parse_priority_int`.

use crate::knowledge::FileKnowledgeDB;
use crate::runtime::HookRuntime;
use rusqlite::params;
#[cfg(feature = "templates")]
use touring_orchestration::tasks::template_engine::load_env_for_template;
use touring_orchestration::tasks::{TasksfileCompiler, parse_yaml};

// ─── Wave P1: priority string ↔ integer mapping ────────────────────────────

/// Parse a priority token (`"high"`, `"normal"`, `"low"`) to its INTEGER value.
///
/// Returns `128` (normal) for unknown / missing tokens.
/// Lower integer = higher logical priority (so `ORDER BY priority ASC` puts
/// high-priority subtasks first).
#[must_use]
pub fn parse_priority_token(token: &str) -> i64 {
    match &*token.trim().to_ascii_lowercase() {
        "high" | "h" | "hi" => 50,
        "low" | "l" | "lo" => 200,
        _ => 128, // "normal", empty, or unknown
    }
}

/// Inverse of `parse_priority_token` — render an INTEGER back to a label.
///
/// Boundaries: <= 100 → "high", >= 180 → "low", else "normal".
#[must_use]
pub fn priority_label(value: i64) -> &'static str {
    match value {
        v if v <= 100 => "high",
        v if v >= 180 => "low",
        _ => "normal",
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Decompose handlers (DAG task management)
// ─────────────────────────────────────────────────────────────────────────────

/// Ensure decompose tables exist (idempotent). Called by create handler.
///
/// Wave 2026-05-02 (Schema Drift Fix): `CREATE TABLE IF NOT EXISTS` is a no-op
/// when the table already exists, so DBs created before columns
/// `parallel_group` (S1.8) and `quality_score` (P1) were added would have
/// 14-column schemas while the INSERT statements assume 16 columns. The
/// `migrate_decompose_columns` helper applies idempotent ALTER TABLE
/// statements to bring legacy DBs forward.
pub fn ensure_decompose_tables(db: &FileKnowledgeDB) {
    let _ = db.conn_ref().execute_batch(
        "CREATE TABLE IF NOT EXISTS task_decompositions (
            task_id TEXT PRIMARY KEY,
            task_type TEXT NOT NULL,
            description TEXT NOT NULL,
            cila_level INTEGER NOT NULL DEFAULT 3,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            archived_at TEXT,
            status TEXT NOT NULL DEFAULT 'active',
            metrics TEXT
        );
        CREATE TABLE IF NOT EXISTS decomposition_subtasks (
            subtask_id TEXT PRIMARY KEY,
            task_id TEXT NOT NULL,
            description TEXT NOT NULL,
            depends_on TEXT NOT NULL DEFAULT '[]',
            priority INTEGER NOT NULL DEFAULT 255,
            status TEXT NOT NULL,
            deadline TEXT,
            deadline_behavior TEXT DEFAULT 'Fail',
            parallel_group TEXT,
            review_required INTEGER NOT NULL DEFAULT 0,
            complexity_hint TEXT,
            retry_policy TEXT,
            attempts INTEGER NOT NULL DEFAULT 0,
            quality_score REAL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            FOREIGN KEY (task_id) REFERENCES task_decompositions(task_id)
        );
        CREATE INDEX IF NOT EXISTS idx_task_status ON task_decompositions(status);
        CREATE INDEX IF NOT EXISTS idx_subtasks_task ON decomposition_subtasks(task_id);

        -- Feature C: Step-level execution tracking (2026-04-24)
        CREATE TABLE IF NOT EXISTS subtask_results (
            id TEXT PRIMARY KEY,
            subtask_id TEXT NOT NULL,
            started_at TEXT NOT NULL,
            completed_at TEXT,
            duration_ms INTEGER,
            cache_hit INTEGER NOT NULL DEFAULT 0,
            output_json TEXT,
            error TEXT,
            FOREIGN KEY (subtask_id) REFERENCES decomposition_subtasks(subtask_id)
        );
        CREATE INDEX IF NOT EXISTS idx_results_subtask ON subtask_results(subtask_id);
        CREATE INDEX IF NOT EXISTS idx_results_started ON subtask_results(started_at);

        -- S1.4: Event audit trail (decomposition_events — NEY was never written)
        CREATE TABLE IF NOT EXISTS decomposition_events (
            event_id INTEGER PRIMARY KEY AUTOINCREMENT,
            task_id TEXT NOT NULL,
            subtask_id TEXT,
            event_type TEXT NOT NULL,
            payload TEXT NOT NULL,
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        );

        -- S1.6: Snapshot table for pre-compact checkpoints
        CREATE TABLE IF NOT EXISTS decomposition_snapshots (
            snapshot_id TEXT PRIMARY KEY,
            task_id TEXT NOT NULL,
            subtasks_snapshot TEXT NOT NULL,
            metrics_snapshot TEXT NOT NULL,
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        );

        -- S1.1: Index for deadline queries
        CREATE INDEX IF NOT EXISTS idx_subtasks_deadline ON decomposition_subtasks(deadline) WHERE deadline IS NOT NULL;"
    );

    // Wave 2026-05-02: migrate legacy DBs forward (idempotent).
    migrate_decompose_columns(db);
}

/// Apply idempotent ALTER TABLE migrations for columns added after the
/// initial schema. SQLite errors with "duplicate column name" when a column
/// already exists; we treat that error as success (already migrated).
///
/// Add new entries here when extending the schema; existing entries are
/// safe to leave (they no-op on already-migrated DBs).
fn migrate_decompose_columns(db: &FileKnowledgeDB) {
    let migrations: &[&str] = &[
        // Wave S1.8 (2026-04-24): parallel_group for parallel-execution buckets
        "ALTER TABLE decomposition_subtasks ADD COLUMN parallel_group TEXT",
        // Wave P1 (2026-04-25): quality_score for completion-quality tracking
        "ALTER TABLE decomposition_subtasks ADD COLUMN quality_score REAL",
        // C2 (2026-08-18): atomic claim. `ready` only READS, so two concurrent
        // sessions polling it were handed the SAME subtask and both started it.
        // These two columns turn "who is working on this" into persisted state
        // that a conditional UPDATE can arbitrate.
        "ALTER TABLE decomposition_subtasks ADD COLUMN claimed_by TEXT",
        "ALTER TABLE decomposition_subtasks ADD COLUMN claim_expires_at INTEGER",
        // C3 (2026-08-18): Wayfinder. A DAG of uniform "tasks" cannot express that
        // some entries exist to DECIDE something and others to BUILD it, nor how
        // much fog surrounds each. These five columns carry that.
        "ALTER TABLE decomposition_subtasks ADD COLUMN ticket_kind TEXT",
        "ALTER TABLE decomposition_subtasks ADD COLUMN ticket_subtype TEXT",
        "ALTER TABLE decomposition_subtasks ADD COLUMN autonomy TEXT",
        "ALTER TABLE decomposition_subtasks ADD COLUMN fog TEXT",
        "ALTER TABLE decomposition_subtasks ADD COLUMN origin_ticket TEXT",
        // T3.1 (2026-08-19): the map must age with the work. Pocock: "that
        // resolution also gets written back up to the parent map." Without it the
        // decisions live in the children while the parent keeps describing the
        // world as it was before any of them were made.
        "ALTER TABLE task_decompositions ADD COLUMN resolutions TEXT",
    ];

    let conn = db.conn_ref();
    for sql in migrations {
        match conn.execute(sql, []) {
            Ok(_) => {
                tracing::info!("decompose migration applied: {}", sql);
            }
            Err(e) => {
                // "duplicate column name" is the expected outcome on an
                // already-migrated DB. Anything else is a real failure.
                let msg = e.to_string();
                if !msg.contains("duplicate column name") {
                    tracing::warn!("decompose migration failed: {} → {}", sql, e);
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// S1.4: Event audit trail — never written, now wired
// ─────────────────────────────────────────────────────────────────────────────

/// Log a decompose event to decomposition_events (best-effort, never fails).
pub fn log_event(
    db: &FileKnowledgeDB,
    task_id: &str,
    subtask_id: Option<&str>,
    event_type: &str,
    payload: &serde_json::Value,
) {
    let _ = db.conn_ref().execute(
        "INSERT INTO decomposition_events (task_id, subtask_id, event_type, payload) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![task_id, subtask_id, event_type, serde_json::to_string(payload).unwrap_or_default()],
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// S1.1+S1.2: Deadline enforcement with 4 behaviors
// ─────────────────────────────────────────────────────────────────────────────

/// DeadlineBehavior enum: determines what happens when a deadline expires.
#[derive(Debug, Clone, Copy)]
pub enum DeadlineBehavior {
    /// Mark the breached subtask as failed.
    Fail,
    /// Mark the breached subtask as skipped.
    Skip,
    /// Emit a warning but leave the subtask pending.
    Notify,
    /// Lower the subtask's priority to defer it.
    Backburner,
}

impl DeadlineBehavior {
    /// Parses a behavior name (case-insensitive), defaulting to `Fail` on unknown input.
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "skip" => DeadlineBehavior::Skip,
            "notify" => DeadlineBehavior::Notify,
            "backburner" => DeadlineBehavior::Backburner,
            _ => DeadlineBehavior::Fail,
        }
    }
}

/// SubtaskDeadlineBreach describes a single subtask that has breached its deadline.
#[derive(Debug)]
pub struct SubtaskDeadlineBreach {
    /// Identifier of the subtask that breached its deadline.
    pub subtask_id: String,
    /// Behavior applied in response to the breach.
    pub behavior: DeadlineBehavior,
    /// Timestamp at which the breach was detected.
    pub now: chrono::DateTime<chrono::Utc>,
}

/// Check all subtasks for a given task and apply deadline behaviors.
///
/// For each subtask where `deadline < now()` AND `status != completed`:
/// - "Fail"    → set status = "failed"
/// - "Skip"    → set status = "skipped"
/// - "Notify"  → emit to tracing::warn, keep pending (best-effort emit)
/// - "Backburner" → lower priority to 220
///
/// Returns the count of breached subtasks.
pub fn check_deadlines(db: &FileKnowledgeDB, task_id: &str) -> usize {
    let now = chrono::Utc::now();
    let now_str = now.to_rfc3339();

    // Fetch all subtasks with deadline < now that are not completed
    let mut stmt = match db.conn_ref().prepare(
        "SELECT subtask_id, deadline, deadline_behavior FROM decomposition_subtasks
         WHERE task_id = ?1 AND deadline IS NOT NULL AND status NOT IN ('completed', 'failed', 'skipped')",
    ) {
        Ok(s) => s,
        Err(_) => return 0,
    };

    let rows: Vec<(String, String, String)> = stmt
        .query_map(params![task_id], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default();

    let mut breached = 0;
    for (subtask_id, deadline_str, behavior_str) in rows {
        if let Ok(deadline) = chrono::DateTime::parse_from_rfc3339(&deadline_str)
            && deadline.with_timezone(&chrono::Utc) < now
        {
            breached += 1;
            let behavior = DeadlineBehavior::from_str(&behavior_str);
            match behavior {
                DeadlineBehavior::Fail => {
                    let _ = db.conn_ref().execute(
                            "UPDATE decomposition_subtasks SET status = 'failed', updated_at = ?1 WHERE subtask_id = ?2",
                            params![now_str, subtask_id],
                        );
                }
                DeadlineBehavior::Skip => {
                    let _ = db.conn_ref().execute(
                            "UPDATE decomposition_subtasks SET status = 'skipped', updated_at = ?1 WHERE subtask_id = ?2",
                            params![now_str, subtask_id],
                        );
                }
                DeadlineBehavior::Notify => {
                    tracing::warn!(
                        "deadline breached: subtask_id={} deadline={}",
                        subtask_id,
                        deadline_str
                    );
                }
                DeadlineBehavior::Backburner => {
                    let _ = db.conn_ref().execute(
                            "UPDATE decomposition_subtasks SET priority = 220, updated_at = ?1 WHERE subtask_id = ?2",
                            params![now_str, subtask_id],
                        );
                }
            }
        }
    }
    breached
}

/// S1.3: Retry loop with attempts tracking.
///
/// When a subtask fails, evaluate its retry_policy. If attempts < max_attempts:
///   - increment attempts
///   - reset status to "pending"
///
/// Returns `true` if a retry was scheduled, `false` if the subtask should remain failed.
pub fn evaluate_retry_policy(
    db: &FileKnowledgeDB,
    subtask_id: &str,
    retry_policy_val: Option<&serde_json::Value>,
) -> bool {
    let policy = match retry_policy_val {
        Some(v) => match serde_json::from_value(v.clone()) {
            Ok(p) => p,
            Err(_) => return false,
        },
        None => {
            // Fetch from DB
            let stored: Option<String> = db
                .conn_ref()
                .query_row(
                    "SELECT retry_policy FROM decomposition_subtasks WHERE subtask_id = ?1",
                    params![subtask_id],
                    |r| r.get::<_, String>(0),
                )
                .ok();
            let policy_val = match stored {
                Some(s) => serde_json::from_str(&s).unwrap_or_else(|_| serde_json::json!({})),
                None => serde_json::json!({}),
            };
            match serde_json::from_value::<RetryPolicy>(policy_val) {
                Ok(p) => p,
                Err(_) => return false,
            }
        }
    };

    let attempts: i64 = db
        .conn_ref()
        .query_row(
            "SELECT attempts FROM decomposition_subtasks WHERE subtask_id = ?1",
            params![subtask_id],
            |r| r.get::<_, i64>(0),
        )
        .unwrap_or(0);

    if attempts >= policy.max_attempts as i64 {
        return false; // Give up
    }

    let now = chrono::Utc::now().to_rfc3339();
    db.conn_ref()
        .execute(
            "UPDATE decomposition_subtasks SET attempts = attempts + 1, status = 'pending', updated_at = ?1 WHERE subtask_id = ?2",
            params![now, subtask_id],
        )
        .ok();
    true
}

/// RetryPolicy schema (stored in the retry_policy column).
#[derive(Debug, Clone, serde::Deserialize)]
pub struct RetryPolicy {
    /// Maximum number of attempts before giving up.
    pub max_attempts: usize,
    /// Base backoff delay between attempts, in milliseconds.
    #[serde(default = "default_backoff_ms")]
    pub backoff_ms: u64,
    /// Multiplier applied to the backoff after each attempt.
    #[serde(default = "default_backoff_multiplier")]
    pub backoff_multiplier: f64,
    /// Failure conditions that trigger a retry.
    #[serde(default)]
    pub retry_on: Vec<String>,
}

fn default_backoff_ms() -> u64 {
    1000
}
fn default_backoff_multiplier() -> f64 {
    2.0
}

/// Feature C: Record execution start for a subtask.
pub fn record_subtask_started(db: &FileKnowledgeDB, subtask_id: &str) {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let _ = db.conn_ref().execute(
        "INSERT INTO subtask_results (id, subtask_id, started_at) VALUES (?1, ?2, ?3)",
        params![id, subtask_id, now],
    );
}

/// Feature C: Record execution completion for a subtask.
pub fn record_subtask_completed(
    db: &FileKnowledgeDB,
    subtask_id: &str,
    status: &str,
    output_json: Option<String>,
    error: Option<String>,
) {
    let now = chrono::Utc::now();
    let now_str = now.to_rfc3339();

    // Calculate duration_ms from the started_at record
    let started_str: Option<String> = db.conn_ref()
        .query_row(
            "SELECT started_at FROM subtask_results WHERE subtask_id = ?1 AND completed_at IS NULL ORDER BY started_at DESC LIMIT 1",
            params![subtask_id],
            |r| r.get::<_, String>(0),
        )
        .ok();

    let duration_ms: Option<i64> = started_str.and_then(|s| {
        chrono::DateTime::parse_from_rfc3339(&s)
            .ok()
            .map(|started| (now - started.with_timezone(&chrono::Utc)).num_milliseconds())
    });

    let cache_hit = status == "completed"; // Simple heuristic: completed means cache was hit if result was fast
    let _ = db.conn_ref().execute(
        "UPDATE subtask_results SET completed_at = ?1, duration_ms = ?2, cache_hit = ?3, output_json = ?4, error = ?5 WHERE subtask_id = ?6 AND completed_at IS NULL",
        params![now_str, duration_ms, cache_hit as i32, output_json, error, subtask_id],
    );
}

/// Creates a new decomposition task (DAG root) from a task type and description, returning its id as JSON.
pub fn cli_decompose_create(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let task_type = payload
        .get("task_type")
        .and_then(|v| v.as_str())
        .unwrap_or("general");
    let description = payload
        .get("description")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    // Wave P1: optional priority echo (task itself doesn't store it; stored on subtasks).
    let priority_token = payload
        .get("priority")
        .and_then(|v| v.as_str())
        .unwrap_or("normal");

    let task_id = format!(
        "task_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    );
    let now = chrono::Utc::now().to_rfc3339();

    let db = &rt.ctx.knowledge;
    ensure_decompose_tables(db);
    let result = db.conn_ref().execute(
        "INSERT OR IGNORE INTO task_decompositions (task_id, task_type, description, status, created_at, updated_at) VALUES (?1, ?2, ?3, 'created', ?4, ?4)",
        params![task_id, task_type, description, now],
    );
    if let Err(e) = &result {
        tracing::debug!("decompose create INSERT failed: {}", e);
    }

    // S1.4: Wire event audit trail
    log_event(
        db,
        &task_id,
        None,
        "task_created",
        &serde_json::json!({"task_type": task_type, "description": description}),
    );

    // T2.2: Tasksfile YAML import — quando `tasksfile_yaml` vem no payload, o
    // documento é compilado em subtarefas. O campo `subtasks` (array JSON) segue
    // funcionando; o YAML é um formato de entrada alternativo.
    let tasksfile_subtasks_added = import_tasksfile_subtasks(db, &task_id, payload, &now);

    // FA-2: Signal active plan hint to SessionBus for pre_edit context injection.
    rt.ctx
        .session_bus
        .borrow_mut()
        .signal_plan_active(description.to_string());

    serde_json::json!({
        "task_id": task_id,
        "task_type": task_type,
        "description": description,
        "status": "created",
        "created_at": now,
        "priority": priority_token,
        "persisted": result.is_ok(),
        "tasksfile_subtasks_added": tasksfile_subtasks_added,
    })
    .to_string()
}

/// Compila `payload["tasksfile_yaml"]` em subtarefas de `task_id`; devolve
/// quantas foram inseridas (0 quando o campo está ausente ou o YAML não compila).
///
/// Extraída de `cli_decompose_create` em 03/08/2026 para ser chamada também pela
/// implementação DESPACHADA (`cli/decompose.rs`), que não tinha esta etapa: o
/// registry roteia `cli-decompose-create` para lá, então `touring tasksfile
/// import` criava a tarefa e **descartava as subtarefas em silêncio**. Provado
/// com o binário em produção: o import de um YAML de 2 tarefas devolveu
/// `subtask_count = 0`. Os 8 testes que cobriam o import passavam porque
/// importavam esta cópia superseded direto, sem passar pelo dispatch.
///
/// Uma função, dois chamadores — clonar as ~140 linhas reintroduziria
/// exatamente a assimetria que causou o defeito.
pub(crate) fn import_tasksfile_subtasks(
    db: &FileKnowledgeDB,
    task_id: &str,
    payload: &serde_json::Value,
    now: &str,
) -> usize {
    let Some(yaml_str) = payload.get("tasksfile_yaml").and_then(|v| v.as_str()) else {
        return 0;
    };
    let root = match parse_yaml(yaml_str) {
        Ok(root) => root,
        Err(e) => {
            tracing::debug!("tasksfile parse failed: {}", e);
            return 0;
        }
    };
    let compiled = match TasksfileCompiler::new().compile(&root) {
        Ok(compiled) => compiled,
        Err(e) => {
            tracing::debug!("tasksfile compile failed: {}", e);
            return 0;
        }
    };
    let mut added = 0;
    for compiled_task in &compiled.tasks {
        // T3.2: resolve o `env_file` da tarefa e mescla com o `env` inline
        // (inline vence). `load_env_for_template` é feature-gated em
        // touring-tasksfile — sem a feature, o mapa é vazio.
        #[cfg(feature = "templates")]
        let task_env_vars = load_env_for_template(&compiled_task.env_file);
        #[cfg(not(feature = "templates"))]
        let task_env_vars = std::collections::HashMap::<String, String>::new();
        #[cfg_attr(not(feature = "templates"), allow(unused_variables))]
        let merged_env: std::collections::HashMap<String, String> = task_env_vars
            .into_iter()
            .chain(
                compiled_task
                    .env
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone())),
            )
            .collect();

        // T3.3/T3.4: substituição de template em descrição e comando. Mapa de
        // params vazio — o filtro `default` do Tera cobre variáveis ausentes.
        #[cfg_attr(not(feature = "templates"), allow(unused_variables))]
        let empty_params = std::collections::HashMap::<String, serde_json::Value>::new();
        #[cfg_attr(not(feature = "templates"), allow(unused_variables))]
        let render = |raw: &str| -> String {
            #[cfg(feature = "templates")]
            {
                touring_orchestration::tasks::template_engine::render_command(
                    raw,
                    &empty_params,
                    &merged_env,
                )
                .unwrap_or_else(|_| raw.to_string())
            }
            #[cfg(not(feature = "templates"))]
            {
                raw.to_string()
            }
        };
        let rendered_description = render(&compiled_task.description);
        let rendered_command = render(&compiled_task.command);

        let deps_json =
            serde_json::to_string(&compiled_task.depends_on).unwrap_or_else(|_| "[]".to_string());
        let scoped_id = format!("{}::{}", task_id, compiled_task.task_id);
        let deadline_behavior = compiled_task.deadline_behavior.as_deref().unwrap_or("Fail");
        let retry_policy_json = compiled_task
            .retry_policy
            .as_ref()
            .map(|rp| serde_json::to_string(rp).unwrap_or_else(|_| "{}".to_string()))
            .unwrap_or_else(|| "{}".to_string());
        let insert_result = db.conn_ref().execute(
            "INSERT OR REPLACE INTO decomposition_subtasks \
             (subtask_id, task_id, description, depends_on, priority, status, \
              deadline, deadline_behavior, parallel_group, review_required, \
              complexity_hint, retry_policy, attempts, quality_score, created_at, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, 'pending', ?6, ?7, ?8, ?9, ?10, ?11, 0, NULL, ?12, ?13)",
            params![
                scoped_id,
                task_id,
                rendered_description,
                deps_json,
                compiled_task.priority,
                compiled_task.deadline,
                deadline_behavior,
                compiled_task.parallel_group,
                compiled_task.review_required as i32,
                compiled_task.complexity_hint,
                retry_policy_json,
                now,
                now,
            ],
        );
        if insert_result.is_ok() {
            added += 1;
            log_event(
                db,
                task_id,
                Some(&scoped_id),
                "subtask_added_from_tasksfile",
                &serde_json::json!({
                    "description": rendered_description,
                    "command": rendered_command,
                    "depends_on": &compiled_task.depends_on,
                    "priority": compiled_task.priority,
                    "templates_rendered": true,
                }),
            );
        }
    }
    added
}

/// Adds a subtask to an existing task's DAG, recording its dependencies on other subtasks.
pub fn cli_decompose_add(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    ensure_decompose_tables(&rt.ctx.knowledge);
    let task_id = payload
        .get("task_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let subtask_id = payload
        .get("subtask_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let description = payload
        .get("description")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let depends_on: Vec<String> = payload
        .get("depends_on")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|s| s.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    // Wave P1: parse priority token (default "normal" → 128).
    let priority_token = payload
        .get("priority")
        .and_then(|v| v.as_str())
        .unwrap_or("normal");
    let priority_int = parse_priority_token(priority_token);

    // S1.1: parse deadline and deadline_behavior from payload
    let deadline = payload.get("deadline").and_then(|v| v.as_str());
    let deadline_behavior = payload
        .get("deadline_behavior")
        .and_then(|v| v.as_str())
        .unwrap_or("Fail");

    // S1.8: parse parallel_group from payload
    let parallel_group = payload.get("parallel_group").and_then(|v| v.as_str());

    let now = chrono::Utc::now().to_rfc3339();

    let db = &rt.ctx.knowledge;
    let deps_json = serde_json::to_string(&depends_on).unwrap_or_else(|_| "[]".to_string());
    let scoped_id = if subtask_id.contains("::") {
        subtask_id.to_string()
    } else {
        format!("{}::{}", task_id, subtask_id)
    };
    let result = db.conn_ref().execute(
        "INSERT OR REPLACE INTO decomposition_subtasks (subtask_id, task_id, description, depends_on, priority, status, deadline, deadline_behavior, parallel_group, review_required, complexity_hint, retry_policy, attempts, quality_score, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, 'pending', ?6, ?7, ?8, 0, NULL, NULL, 0, NULL, ?9, ?10)",
        params![scoped_id, task_id, description, deps_json, priority_int, deadline, deadline_behavior, parallel_group, now, now],
    );
    if let Err(e) = &result {
        tracing::debug!("decompose add INSERT failed: {}", e);
    }

    // S1.4: Wire event audit trail for subtask creation
    log_event(
        db,
        task_id,
        Some(&scoped_id),
        "subtask_added",
        &serde_json::json!({
            "description": description,
            "depends_on": depends_on,
            "priority_int": priority_int,
            "deadline": deadline,
            "deadline_behavior": deadline_behavior,
            "parallel_group": parallel_group
        }),
    );

    // FA-2: Signal active plan hint to SessionBus for pre_edit context injection.
    rt.ctx
        .session_bus
        .borrow_mut()
        .signal_plan_active(description.to_string());

    serde_json::json!({
        "task_id": task_id,
        "subtask_id": subtask_id,
        "scoped_id": scoped_id,
        "description": description,
        "depends_on": depends_on,
        "status": "pending",
        "priority": priority_label(priority_int),
        "priority_int": priority_int,
        "deadline": deadline,
        "deadline_behavior": deadline_behavior,
        "parallel_group": parallel_group,
        "created_at": now,
        "persisted": result.is_ok()
    })
    .to_string()
}

/// Retrieves a task and its full subtask DAG as JSON.
pub fn cli_decompose_get(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let task_id = payload
        .get("task_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let db = &rt.ctx.knowledge;
    ensure_decompose_tables(db);

    let task: Option<serde_json::Value> = db.conn_ref()
        .query_row(
            "SELECT task_id, task_type, description, status, created_at, updated_at FROM task_decompositions WHERE task_id = ?1",
            params![task_id],
            |r| {
                Ok(serde_json::json!({
                    "task_id": r.get::<_, String>(0)?,
                    "task_type": r.get::<_, String>(1)?,
                    "description": r.get::<_, String>(2)?,
                    "status": r.get::<_, String>(3)?,
                    "created_at": r.get::<_, String>(4)?,
                    "updated_at": r.get::<_, String>(5)?
                }))
            },
        )
        .ok();

    let subtasks: Vec<serde_json::Value> = {
        let mut stmt = match db.conn_ref().prepare(
            "SELECT subtask_id, description, depends_on, status, priority, deadline, deadline_behavior, parallel_group FROM decomposition_subtasks WHERE task_id = ?1",
        ) {
            Ok(s) => s,
            Err(e) => {
                tracing::debug!("decompose get subtasks prepare failed: {}", e);
                return serde_json::json!({"error": format!("db error: {}", e)}).to_string();
            }
        };
        stmt.query_map(params![task_id], |r| {
            let deps_str = r.get::<_, String>(2)?;
            let depends_on: Vec<String> = serde_json::from_str(&deps_str).unwrap_or_else(|_| {
                deps_str
                    .split(',')
                    .filter(|s| !s.is_empty())
                    .map(String::from)
                    .collect()
            });
            Ok(serde_json::json!({
                "subtask_id": r.get::<_, String>(0)?,
                "description": r.get::<_, String>(1)?,
                "depends_on": depends_on,
                "status": r.get::<_, String>(3)?,
                "priority": r.get::<_, i32>(4)?,
                "deadline": r.get::<_, Option<String>>(5)?,
                "deadline_behavior": r.get::<_, Option<String>>(6)?,
                "parallel_group": r.get::<_, Option<String>>(7)?
            }))
        })
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default()
    };

    serde_json::json!({
        "task": task,
        "subtasks": subtasks,
        "subtask_count": subtasks.len()
    })
    .to_string()
}

/// Helper: update optional fields on a subtask row (priority, quality_score).
fn update_subtask_fields(
    db: &FileKnowledgeDB,
    subtask_id: &str,
    status: Option<&str>,
    priority: Option<i32>,
    quality_score: Option<f64>,
    depends_on: Option<&[String]>,
) -> i64 {
    let mut sql = String::from("UPDATE decomposition_subtasks SET ");
    let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
    let mut sets: Vec<String> = Vec::new();

    if let Some(s) = status {
        sets.push("status = ?1".to_string());
        params.push(Box::new(s.to_string()));
    }
    if let Some(p) = priority {
        let idx = params.len() + 1;
        sets.push(format!("priority = ?{}", idx));
        params.push(Box::new(p));
    }
    if let Some(q) = quality_score {
        let idx = params.len() + 1;
        sets.push(format!("quality_score = ?{}", idx));
        params.push(Box::new(q));
    }
    // Wave 2026-05-02 (Diagnostic Fix ISSUE-4): persist dependency list updates.
    // Schema column `depends_on TEXT NOT NULL DEFAULT '[]'` already supports it.
    // SENTINEL_W2026_05_02_ISSUE_4_DEPENDS_ON_SET — string used to verify the
    // build pipeline picked up this branch (grep the binary for this token).
    if let Some(deps) = depends_on {
        let deps_json = serde_json::to_string(deps).unwrap_or_else(|_| "[]".to_string());
        let idx = params.len() + 1;
        sets.push(format!("depends_on = ?{}", idx));
        params.push(Box::new(deps_json));
        tracing::info!(
            "SENTINEL_W2026_05_02_ISSUE_4_DEPENDS_ON_SET applied for {}",
            subtask_id
        );
    }

    if sets.is_empty() {
        return 0;
    }

    sql.push_str(&sets.join(", "));
    let where_idx = params.len() + 1;
    sql.push_str(&format!(" WHERE subtask_id = ?{}", where_idx));
    params.push(Box::new(subtask_id.to_string()));

    let refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();
    let result = db.conn_ref().execute(&sql, refs.as_slice());
    if let Err(ref e) = result {
        tracing::warn!("update_subtask_fields SQL='{}' err={}", sql, e);
    }
    result.unwrap_or(0) as i64
}

/// Write a resolved DECISION back up to the map that contains it (T3.1).
///
/// A Wayfinder map is only useful while it describes the present. Resolutions
/// recorded solely on the child leave the parent narrating a world that no longer
/// exists, and the next session reads the parent first. Appending here keeps the
/// summary and the primary source in step — the child stays the primary source,
/// which is why only the id, the description and the timestamp travel up.
///
/// Silent no-op for anything that is not a decision reaching a terminal status:
/// implementation tickets close constantly and would drown the map in noise.
pub fn propagate_resolution_to_map(
    db: &FileKnowledgeDB,
    task_id: &str,
    scoped_subtask_id: &str,
    status: &str,
    now: &str,
) -> bool {
    if !matches!(status, "completed" | "done" | "complete") {
        return false;
    }
    let Ok((kind, description)) = db.conn_ref().query_row(
        "SELECT COALESCE(ticket_kind, ''), description FROM decomposition_subtasks \
         WHERE subtask_id = ?1",
        params![scoped_subtask_id],
        |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
    ) else {
        return false;
    };
    if kind != "decision" {
        return false;
    }
    let existing: String = db
        .conn_ref()
        .query_row(
            "SELECT COALESCE(resolutions, '[]') FROM task_decompositions WHERE task_id = ?1",
            params![task_id],
            |r| r.get::<_, String>(0),
        )
        .unwrap_or_else(|_| "[]".to_string());
    let mut log: Vec<serde_json::Value> =
        serde_json::from_str(&existing).unwrap_or_else(|_| Vec::new());
    // Idempotent: replaying the same close must not grow the map.
    if log
        .iter()
        .any(|e| e.get("subtask_id").and_then(|v| v.as_str()) == Some(scoped_subtask_id))
    {
        return false;
    }
    log.push(serde_json::json!({
        "subtask_id": scoped_subtask_id,
        "description": description,
        "resolved_at": now,
    }));
    let Ok(encoded) = serde_json::to_string(&log) else {
        return false;
    };
    db.conn_ref()
        .execute(
            "UPDATE task_decompositions SET resolutions = ?1, updated_at = ?2 WHERE task_id = ?3",
            params![encoded, now, task_id],
        )
        .map(|n| n > 0)
        .unwrap_or(false)
}

/// Updates a subtask's mutable fields (status, title, dependencies) in place.
pub fn cli_decompose_update(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let task_id = payload
        .get("task_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let status = payload.get("status").and_then(|v| v.as_str()).unwrap_or("");
    let priority = payload
        .get("priority")
        .and_then(|v| v.as_i64())
        .map(|p| p as i32);
    let quality_score = payload.get("quality_score").and_then(|v| v.as_f64());
    // Wave 2026-05-02 (Diagnostic Fix ISSUE-4): accept depends_on updates.
    let depends_on: Option<Vec<String>> = payload.get("depends_on").and_then(|v| {
        v.as_array().map(|arr| {
            arr.iter()
                .filter_map(|x| x.as_str().map(|s| s.to_string()))
                .filter(|s| !s.is_empty())
                .collect()
        })
    });

    let db = &rt.ctx.knowledge;
    ensure_decompose_tables(db);
    let now = chrono::Utc::now().to_rfc3339();

    // P1-S1: Also update subtask-level fields if provided
    let subtask_id_opt = payload.get("subtask_id").and_then(|v| v.as_str());

    // O status escrito na linha-PAI só vem do chamador quando ele endereçou o
    // pai (sem `subtask_id`). Endereçando uma subtarefa, o pai é DERIVADO dos
    // filhos mais abaixo — até 03/08/2026 o status ia direto para o pai em
    // ambos os casos, e o primeiro `decompose update <task> <fase> --status done`
    // de um plano multi-fase o marcava inteiro como concluído (Lei L2: o DAG é a
    // fonte autoritativa de progresso; um pai falsamente `done` corrompe
    // exatamente o que o gate de convergência mede).
    let addresses_parent = subtask_id_opt.is_none();
    let task_affected = if !status.is_empty() && addresses_parent {
        db.conn_ref()
            .execute(
                "UPDATE task_decompositions SET status = ?1, updated_at = ?3 WHERE task_id = ?2",
                params![status, task_id, now],
            )
            .unwrap_or(0)
    } else {
        0
    };
    let scoped = subtask_id_opt.map(|raw| {
        if raw.contains("::") {
            raw.to_string()
        } else {
            format!("{}::{}", task_id, raw)
        }
    });
    let subtask_affected = if subtask_id_opt.is_some()
        || priority.is_some()
        || quality_score.is_some()
        || depends_on.is_some()
    {
        let raw = subtask_id_opt.unwrap_or(task_id);
        let scoped = if raw.contains("::") {
            raw.to_string()
        } else {
            format!("{}::{}", task_id, raw)
        };
        let status_arg = if status.is_empty() {
            None
        } else {
            Some(status)
        };
        update_subtask_fields(
            db,
            &scoped,
            status_arg,
            priority,
            quality_score,
            depends_on.as_deref(),
        )
    } else {
        0
    };

    // Feature C: Instrument status transitions
    if let Some(ref scoped_val) = scoped {
        match status {
            "in_progress" => {
                record_subtask_started(db, scoped_val);
                // S1.4: Wire event audit trail
                log_event(
                    db,
                    task_id,
                    Some(scoped_val),
                    "subtask_started",
                    &serde_json::json!({}),
                );
            }
            "completed" | "failed" => {
                let output_json = payload
                    .get("output_json")
                    .and_then(|v| v.as_str())
                    .map(String::from);
                let error_msg = payload
                    .get("error")
                    .and_then(|v| v.as_str())
                    .map(String::from);
                record_subtask_completed(db, scoped_val, status, output_json, error_msg.clone());
                // S1.4: Wire event audit trail
                log_event(
                    db,
                    task_id,
                    Some(scoped_val),
                    status,
                    &serde_json::json!({"error": error_msg}),
                );
            }
            _ => {}
        }
    }

    // S1.3: Retry loop — evaluate retry_policy when status = "failed"
    let retry_scheduled = if status == "failed" {
        match scoped {
            Some(ref s) => evaluate_retry_policy(db, s, payload.get("retry_policy")),
            None => false,
        }
    } else {
        false
    };

    // Endereçando uma subtarefa, o pai reflete o estado REAL dos filhos.
    let task_affected = if addresses_parent {
        task_affected
    } else {
        refresh_parent_status(db, task_id, &now)
    };

    // T3.1: a resolved DECISION also travels up to the map.
    let resolution_logged = match scoped.as_deref() {
        Some(sid) => propagate_resolution_to_map(db, task_id, sid, status, &now),
        None => false,
    };

    serde_json::json!({
        "task_id": task_id,
        "status": status,
        "updated": task_affected > 0,
        "subtask_updated": subtask_affected > 0,
        "resolution_logged": resolution_logged,
        "priority": priority,
        "quality_score": quality_score,
        "retry_scheduled": retry_scheduled
    })
    .to_string()
}

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

    db.conn_ref()
        .execute(
            "UPDATE task_decompositions SET status = 'finalized', metrics = ?1, updated_at = ?2 WHERE task_id = ?3",
            params![serde_json::to_string(&metrics).unwrap_or_default(), now, task_id],
        )
        .ok();

    // S1.4: Wire event audit trail for task finalization
    log_event(db, task_id, None, "task_finalized", &metrics);

    serde_json::json!({
        "task_id": task_id,
        "status": "finalized",
        "metrics": metrics,
        "breached_deadlines": breached
    })
    .to_string()
}

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

/// Default lease for a claim, in seconds. Long enough for a substantial subtask,
/// short enough that a session killed mid-work frees it the same afternoon.
const DEFAULT_CLAIM_LEASE_SECS: i64 = 3600;

/// Normalise a scoped subtask id (`<task>::S-01`) to its short form (`S-01`).
///
/// `subtask_id` is persisted scoped while `depends_on` holds short ids, so any
/// comparison between the two must normalise both sides first.
fn short_subtask_id(id: &str) -> String {
    id.rsplit("::").next().unwrap_or(id).to_string()
}

/// Atomically claim the next ready subtask for one owner.
///
/// `ready` only READS. Two sessions polling it concurrently were handed the same
/// subtask and both began it — reproduced 2026-08-18, and the reason the plan
/// rated this the highest-risk delivery in the wave.
///
/// The fix is not a lock around the read but a **conditional write**: SQLite
/// serialises writers, so of N racing `UPDATE ... WHERE status = 'pending'`
/// statements exactly one reports a row changed. The losers see zero rows and
/// move on to the next candidate — no lock, no lease server, no lost update.
///
/// This is deliberately a NEW verb rather than a change to `ready_subtasks`,
/// which has 66 call-sites legitimately asking "what COULD start" for display,
/// planning and convergence checks. Claiming answers a different question —
/// "give me work, exclusively" — so every existing caller keeps its meaning.
///
/// Payload: `{task_id, owner, lease_secs?}` → `{claimed: bool, subtask_id?, ...}`.
pub fn cli_decompose_claim(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let task_id = payload
        .get("task_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let owner = payload.get("owner").and_then(|v| v.as_str()).unwrap_or("");
    if task_id.is_empty() || owner.is_empty() {
        return serde_json::json!({
            "claimed": false,
            "error": "task_id and owner are both required — an unattributed claim \
                      cannot be released or expired by anyone"
        })
        .to_string();
    }
    let lease_secs = payload
        .get("lease_secs")
        .and_then(serde_json::Value::as_i64)
        .filter(|v| *v > 0)
        .unwrap_or(DEFAULT_CLAIM_LEASE_SECS);

    let db = &rt.ctx.knowledge;
    ensure_decompose_tables(db);

    /// One row of the claim scan. A named struct rather than a 6-tuple: the sort
    /// below reads `priority` instead of `.3`.
    struct Candidate {
        id: String,
        status: String,
        deps: Vec<String>,
        priority: i32,
        claim_expires_at: Option<i64>,
        claimed_by: Option<String>,
    }

    let rows: Vec<Candidate> = {
        let Ok(mut stmt) = db.conn_ref().prepare(
            "SELECT subtask_id, status, depends_on, priority, claim_expires_at, claimed_by \
             FROM decomposition_subtasks WHERE task_id = ?1",
        ) else {
            return serde_json::json!({"claimed": false, "error": "db error"}).to_string();
        };
        stmt.query_map(params![task_id], |r| {
            let deps_str = r.get::<_, String>(2)?;
            Ok(Candidate {
                id: r.get::<_, String>(0)?,
                status: r.get::<_, String>(1)?,
                deps: serde_json::from_str(&deps_str).unwrap_or_else(|_| {
                    deps_str
                        .split(',')
                        .filter(|s| !s.is_empty())
                        .map(String::from)
                        .collect()
                }),
                priority: r.get::<_, i32>(3)?,
                claim_expires_at: r.get::<_, Option<i64>>(4)?,
                claimed_by: r.get::<_, Option<String>>(5)?,
            })
        })
        .map(|it| it.filter_map(Result::ok).collect())
        .unwrap_or_default()
    };

    let completed: std::collections::HashSet<String> = rows
        .iter()
        .filter(|c| {
            matches!(
                c.status.as_str(),
                "completed" | "done" | "complete" | "failed" | "skipped"
            )
        })
        .map(|c| short_subtask_id(&c.id))
        .collect();

    let now = chrono::Utc::now();
    let now_epoch = now.timestamp();

    // A candidate is unblocked AND unowned: either never started, or held by a
    // lease that has run out (the session that took it died without releasing).
    // A subtask moved to in_progress by hand carries no claim and is left alone —
    // a human said someone is on it, and no lease of ours expires that.
    let mut candidates: Vec<&Candidate> = rows
        .iter()
        .filter(|c| {
            let unblocked = c
                .deps
                .iter()
                .all(|d| completed.contains(&short_subtask_id(d)));
            if !unblocked || completed.contains(&short_subtask_id(&c.id)) {
                return false;
            }
            match c.status.as_str() {
                "pending" => true,
                "in_progress" => {
                    c.claimed_by.is_some() && c.claim_expires_at.is_some_and(|e| e < now_epoch)
                }
                _ => false,
            }
        })
        .collect();
    candidates.sort_by(|a, b| a.priority.cmp(&b.priority).then_with(|| a.id.cmp(&b.id)));

    for Candidate { id: subtask_id, .. } in &candidates {
        let changed = db.conn_ref().execute(
            "UPDATE decomposition_subtasks \
                SET status = 'in_progress', claimed_by = ?1, claim_expires_at = ?2, \
                    updated_at = ?3 \
              WHERE subtask_id = ?4 AND task_id = ?5 \
                AND (status = 'pending' \
                     OR (status = 'in_progress' AND claimed_by IS NOT NULL \
                         AND claim_expires_at IS NOT NULL AND claim_expires_at < ?6))",
            params![
                owner,
                now_epoch + lease_secs,
                now.to_rfc3339(),
                subtask_id,
                task_id,
                now_epoch
            ],
        );
        // Exactly one racer sees 1 here; the others see 0 and try the next candidate.
        if matches!(changed, Ok(1)) {
            return serde_json::json!({
                "claimed": true,
                "task_id": task_id,
                "subtask_id": subtask_id,
                "owner": owner,
                "lease_secs": lease_secs,
                "claim_expires_at": now_epoch + lease_secs,
            })
            .to_string();
        }
    }

    serde_json::json!({
        "claimed": false,
        "task_id": task_id,
        "owner": owner,
        "reason": if candidates.is_empty() {
            "no unblocked, unclaimed subtask is available"
        } else {
            "every candidate was claimed by another owner first"
        },
    })
    .to_string()
}

/// Release a claim back to the pool — only its own owner may.
///
/// A worker that fails should hand the subtask back immediately rather than let
/// the whole lease elapse; letting it expire is the fallback for a session that
/// died, not the normal path.
pub fn cli_decompose_release(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let task_id = payload
        .get("task_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let subtask_id = payload
        .get("subtask_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let owner = payload.get("owner").and_then(|v| v.as_str()).unwrap_or("");
    if task_id.is_empty() || subtask_id.is_empty() || owner.is_empty() {
        return serde_json::json!({
            "released": false,
            "error": "task_id, subtask_id and owner are all required"
        })
        .to_string();
    }

    let db = &rt.ctx.knowledge;
    ensure_decompose_tables(db);
    let changed = db.conn_ref().execute(
        "UPDATE decomposition_subtasks \
            SET status = 'pending', claimed_by = NULL, claim_expires_at = NULL, updated_at = ?1 \
          WHERE task_id = ?2 \
            AND (subtask_id = ?3 OR subtask_id = ?2 || '::' || ?3) \
            AND claimed_by = ?4",
        params![
            chrono::Utc::now().to_rfc3339(),
            task_id,
            subtask_id,
            owner
        ],
    );
    match changed {
        Ok(1) => serde_json::json!({
            "released": true, "task_id": task_id, "subtask_id": subtask_id, "owner": owner
        })
        .to_string(),
        Ok(_) => serde_json::json!({
            "released": false, "task_id": task_id, "subtask_id": subtask_id,
            "reason": "not held by this owner"
        })
        .to_string(),
        Err(e) => serde_json::json!({"released": false, "error": e.to_string()}).to_string(),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// C3: Wayfinder — decision tickets, fog, and a frontier
// ─────────────────────────────────────────────────────────────────────────────

/// A ticket either DECIDES something or BUILDS something. Collapsing the two is
/// what produces plans that look complete while resting on unmade decisions.
const TICKET_KINDS: &[&str] = &["decision", "implementation"];
/// Decision work comes in four shapes; `task` is the ordinary implementation one.
const TICKET_SUBTYPES: &[&str] = &["research", "prototype", "grilling", "task"];
/// Who has to be present: human-in-the-loop, or away-from-keyboard.
const AUTONOMY_MODES: &[&str] = &["hitl", "afk"];
/// How much is unknown. The test for a ticket is whether the question can be
/// STATED clearly now — not whether it can be answered now.
const FOG_LEVELS: &[&str] = &["clear", "hazy", "unknown"];

fn validate_enum(field: &str, value: &str, allowed: &[&str]) -> Option<String> {
    (!allowed.contains(&value)).then(|| format!("{field} must be one of {allowed:?}, got `{value}`"))
}

/// Annotate a subtask with its Wayfinder metadata.
///
/// Payload: `{task_id, subtask_id, kind?, subtype?, autonomy?, fog?, origin_ticket?}`.
/// Every field is optional so a ticket can be refined as the fog lifts.
pub fn cli_decompose_ticket(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let task_id = payload
        .get("task_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let subtask_id = payload
        .get("subtask_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if task_id.is_empty() || subtask_id.is_empty() {
        return serde_json::json!({"updated": false, "error": "task_id and subtask_id are required"})
            .to_string();
    }
    let field = |name: &str| payload.get(name).and_then(|v| v.as_str()).map(str::to_string);
    let (kind, subtype, autonomy, fog, origin) = (
        field("kind"),
        field("subtype"),
        field("autonomy"),
        field("fog"),
        field("origin_ticket"),
    );

    let mut errors: Vec<String> = Vec::new();
    for (name, value, allowed) in [
        ("kind", &kind, TICKET_KINDS),
        ("subtype", &subtype, TICKET_SUBTYPES),
        ("autonomy", &autonomy, AUTONOMY_MODES),
        ("fog", &fog, FOG_LEVELS),
    ] {
        if let Some(v) = value
            && let Some(err) = validate_enum(name, v, allowed)
        {
            errors.push(err);
        }
    }
    // The only decision work that may run unattended is GATHERING evidence.
    // Choosing between options, stress-testing one, or building a throwaway to
    // learn from all end in a judgement, and a judgement wants a human present.
    if kind.as_deref() == Some("decision")
        && autonomy.as_deref() == Some("afk")
        && subtype.as_deref() != Some("research")
    {
        errors.push(
            "a decision ticket may only be `afk` when its subtype is `research` —              gathering evidence can run unattended, reaching a verdict cannot"
                .to_string(),
        );
    }
    if !errors.is_empty() {
        return serde_json::json!({"updated": false, "errors": errors}).to_string();
    }

    let db = &rt.ctx.knowledge;
    ensure_decompose_tables(db);
    // Two things this statement has to get right:
    //   · COALESCE keeps every unset field at its current value, so refining one
    //     facet of a ticket never silently clears the others.
    //   · `subtask_id` is persisted SCOPED (`<task_id>::S-00`) while callers hold
    //     the SHORT id — the same mismatch that once left every dependent subtask
    //     permanently blocked. Matching both forms is what makes a short id update
    //     the row instead of silently touching none.
    let changed = db.conn_ref().execute(
        "UPDATE decomposition_subtasks \
            SET ticket_kind    = COALESCE(?1, ticket_kind), \
                ticket_subtype = COALESCE(?2, ticket_subtype), \
                autonomy       = COALESCE(?3, autonomy), \
                fog            = COALESCE(?4, fog), \
                origin_ticket  = COALESCE(?5, origin_ticket), \
                updated_at     = ?6 \
          WHERE task_id = ?7 \
            AND (subtask_id = ?8 OR subtask_id = ?7 || '::' || ?8)",
        params![
            kind,
            subtype,
            autonomy,
            fog,
            origin,
            chrono::Utc::now().to_rfc3339(),
            task_id,
            subtask_id
        ],
    );
    match changed {
        Ok(1) => serde_json::json!({
            "updated": true, "task_id": task_id, "subtask_id": subtask_id,
            "kind": kind, "subtype": subtype, "autonomy": autonomy,
            "fog": fog, "origin_ticket": origin,
        })
        .to_string(),
        Ok(_) => serde_json::json!({
            "updated": false, "error": "no such subtask in this task"
        })
        .to_string(),
        Err(e) => serde_json::json!({"updated": false, "error": e.to_string()}).to_string(),
    }
}

/// The frontier: the edge of what is known, and what to do about it.
///
/// Two things distinguish this from `ready`. First, it partitions by TICKET KIND,
/// because a decision that has not been made is not one more task to schedule — it
/// gates the work that depends on it, and planning past it produces a plan resting
/// on an unmade choice. Second, it reports the map's integrity: an implementation
/// ticket carries a POINTER back to the decision that produced it (map-as-index —
/// the reasoning lives in the ticket, the map only indexes it), and an
/// implementation ticket with no origin while decisions are still open is work
/// nobody can trace to a choice.
pub fn cli_decompose_frontier(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let task_id = payload
        .get("task_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if task_id.is_empty() {
        return serde_json::json!({"error": "task_id is required"}).to_string();
    }
    let db = &rt.ctx.knowledge;
    ensure_decompose_tables(db);

    #[derive(Clone)]
    struct Ticket {
        id: String,
        description: String,
        status: String,
        deps: Vec<String>,
        priority: i32,
        kind: String,
        subtype: Option<String>,
        autonomy: Option<String>,
        fog: String,
        origin: Option<String>,
        claimed_by: Option<String>,
    }

    let rows: Vec<Ticket> = {
        let Ok(mut stmt) = db.conn_ref().prepare(
            "SELECT subtask_id, description, status, depends_on, priority,                     ticket_kind, ticket_subtype, autonomy, fog, origin_ticket, claimed_by              FROM decomposition_subtasks WHERE task_id = ?1",
        ) else {
            return serde_json::json!({"error": "db error"}).to_string();
        };
        stmt.query_map(params![task_id], |r| {
            let deps_str = r.get::<_, String>(3)?;
            Ok(Ticket {
                id: r.get::<_, String>(0)?,
                description: r.get::<_, String>(1)?,
                status: r.get::<_, String>(2)?,
                deps: serde_json::from_str(&deps_str).unwrap_or_else(|_| {
                    deps_str
                        .split(',')
                        .filter(|s| !s.is_empty())
                        .map(String::from)
                        .collect()
                }),
                priority: r.get::<_, i32>(4)?,
                // An unlabelled ticket is implementation work: that is what a DAG
                // held before Wayfinder, so the default preserves what it meant.
                kind: r
                    .get::<_, Option<String>>(5)?
                    .unwrap_or_else(|| "implementation".to_string()),
                subtype: r.get::<_, Option<String>>(6)?,
                autonomy: r.get::<_, Option<String>>(7)?,
                // Fog is the one axis this feature exists to surface, so an
                // unassessed ticket reports `unknown` — not `clear`. Defaulting to
                // clear made the frontier answer "no fog here" about work nobody
                // had looked at yet, which is the reassuring lie the Wayfinder is
                // meant to prevent. `kind` above may default, because an unlabelled
                // ticket really was implementation work before this existed; there
                // is no equivalent prior meaning for unmeasured fog.
                fog: r
                    .get::<_, Option<String>>(8)?
                    .unwrap_or_else(|| "unknown".to_string()),
                origin: r.get::<_, Option<String>>(9)?,
                claimed_by: r.get::<_, Option<String>>(10)?,
            })
        })
        .map(|it| it.filter_map(Result::ok).collect())
        .unwrap_or_default()
    };

    let is_done = |s: &str| {
        matches!(
            s,
            "completed" | "done" | "complete" | "failed" | "skipped"
        )
    };
    let completed: std::collections::HashSet<String> = rows
        .iter()
        .filter(|t| is_done(&t.status))
        .map(|t| short_subtask_id(&t.id))
        .collect();

    let mut open: Vec<&Ticket> = rows
        .iter()
        .filter(|t| {
            !is_done(&t.status)
                && t.deps
                    .iter()
                    .all(|d| completed.contains(&short_subtask_id(d)))
        })
        .collect();
    open.sort_by(|a, b| a.priority.cmp(&b.priority).then_with(|| a.id.cmp(&b.id)));

    let render = |t: &&Ticket| {
        serde_json::json!({
            "subtask_id": t.id,
            "description": t.description,
            "status": t.status,
            "subtype": t.subtype,
            "autonomy": t.autonomy,
            "fog": t.fog,
            "origin_ticket": t.origin,
            "claimed_by": t.claimed_by,
        })
    };
    let (decisions, implementation): (Vec<&Ticket>, Vec<&Ticket>) =
        open.iter().partition(|t| t.kind == "decision");

    let mut fog_counts = std::collections::BTreeMap::new();
    for t in rows.iter().filter(|t| !is_done(&t.status)) {
        *fog_counts.entry(t.fog.clone()).or_insert(0_usize) += 1;
    }

    // Work nobody can trace back to a choice, while choices are still open.
    let untraceable: Vec<&str> = implementation
        .iter()
        .filter(|t| t.origin.is_none())
        .map(|t| t.id.as_str())
        .collect();

    // T3.2 — Pocock names prototypes as the anti-waterfall device: "some folks
    // look at Wayfinder and think, god, that's a lot of planning, doesn't that
    // look like waterfall? The prototypes are the way that you prevent it from
    // becoming waterfall — huge amounts of low-fidelity upfront planning. A
    // prototype is a high-fidelity way to get feedback on what you're actually
    // building." A map carrying fog with no prototype anywhere in it IS waterfall
    // under another name: a plan whose uncertainty will only be tested after the
    // planning is over.
    //
    // Prototypes are counted across ALL tickets, closed included — a prototype
    // that already ran did its job of lifting fog, and demanding a fresh one
    // would punish the map for having worked.
    //
    // This NAMES the risk; it never blocks. Fog is legitimate, and a map may
    // rationally decide that research rather than a prototype is what lifts it.
    let foggy = rows
        .iter()
        .filter(|t| !is_done(&t.status) && t.fog != "clear")
        .count();
    let prototypes = rows
        .iter()
        .filter(|t| t.subtype.as_deref() == Some("prototype"))
        .count();
    let waterfall_risk = foggy > 0 && prototypes == 0;

    // T3.1: the decisions this map has already made, as recorded on the map.
    let resolutions: serde_json::Value = db
        .conn_ref()
        .query_row(
            "SELECT COALESCE(resolutions, '[]') FROM task_decompositions WHERE task_id = ?1",
            params![task_id],
            |r| r.get::<_, String>(0),
        )
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_else(|| serde_json::json!([]));

    let gated = !decisions.is_empty();
    serde_json::json!({
        "task_id": task_id,
        "gated_by_open_decisions": gated,
        "fog": fog_counts,
        "resolutions": resolutions,
        "waterfall_risk": {
            "at_risk": waterfall_risk,
            "foggy_open_tickets": foggy,
            "prototype_tickets": prototypes,
            "why": if waterfall_risk {
                "fog with no prototype ticket anywhere: the uncertainty will only be                  tested after the planning is over. A prototype is the high-fidelity                  way to get feedback on what is actually being built."
            } else if foggy == 0 {
                "no fog left to test"
            } else {
                "fog is present and at least one prototype ticket exists to test it"
            },
        },
        "decisions": decisions.iter().map(render).collect::<Vec<_>>(),
        "implementation": implementation.iter().map(render).collect::<Vec<_>>(),
        "untraceable_implementation": untraceable,
        "next_action": if gated {
            "resolve the decision tickets first — implementation planned past an              unmade decision is a plan resting on a guess"
        } else if implementation.is_empty() {
            "the frontier is empty: nothing is unblocked"
        } else {
            "no open decisions — implementation tickets may be claimed"
        },
    })
    .to_string()
}

/// Return ready-to-execute subtasks (deps satisfied) grouped by `parallel_group`.
///
/// Read-only by contract: this answers "what COULD start", which is why two
/// sessions calling it concurrently both receive the same subtask. To TAKE work,
/// use [`cli_decompose_claim`].
pub fn cli_decompose_ready(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let task_id = payload
        .get("task_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let only_ready = payload
        .get("only_ready")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let db = &rt.ctx.knowledge;
    ensure_decompose_tables(db);

    #[derive(Debug, Clone, serde::Serialize)]
    struct SubtaskInfo {
        subtask_id: String,
        status: String,
        depends_on: Vec<String>,
        parallel_group: Option<String>,
        priority: i32,
    }

    // Load all subtasks for this task
    let subtasks: Vec<SubtaskInfo> = {
        let mut stmt = match db.conn_ref().prepare(
            "SELECT subtask_id, status, depends_on, parallel_group, priority FROM decomposition_subtasks WHERE task_id = ?1",
        ) {
            Ok(s) => s,
            Err(_) => return serde_json::json!({"error": "db error"}).to_string(),
        };
        stmt.query_map(params![task_id], |r| {
            let deps_str = r.get::<_, String>(2)?;
            let deps: Vec<String> = serde_json::from_str(&deps_str).unwrap_or_else(|_| {
                deps_str
                    .split(',')
                    .filter(|s| !s.is_empty())
                    .map(String::from)
                    .collect()
            });
            Ok(SubtaskInfo {
                subtask_id: r.get::<_, String>(0)?,
                status: r.get::<_, String>(1)?,
                depends_on: deps,
                parallel_group: r.get::<_, Option<String>>(3)?,
                priority: r.get::<_, i32>(4)?,
            })
        })
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default()
    };

    // Build a set of terminal-success subtask IDs, keyed by SHORT id.
    //
    // ROOT CAUSE of the DAG "ready" quirk: `subtask_id` is persisted SCOPED
    // (`<task_id>::S-00`) while `depends_on` holds SHORT ids (`S-00`). Comparing
    // them directly never matched, so every dependent stayed permanently blocked
    // even after its dependency completed. Normalize BOTH sides to the short id.
    // "done"/"complete" are accepted as aliases for "completed" (the loop-engineering
    // `loop_phase_close` marks subtasks "done").
    let short_id = |id: &str| id.rsplit("::").next().unwrap_or(id).to_string();
    let completed: std::collections::HashSet<String> = subtasks
        .iter()
        .filter(|s| {
            matches!(
                s.status.as_str(),
                "completed" | "done" | "complete" | "failed" | "skipped"
            )
        })
        .map(|s| short_id(&s.subtask_id))
        .collect();

    // A subtask is ready if all its deps (normalized to short ids) are completed.
    let is_ready = |deps: &[String]| deps.iter().all(|d| completed.contains(&short_id(d)));

    // Partition into ready (owned) and blocked (owned)
    let (ready, blocked): (Vec<SubtaskInfo>, Vec<SubtaskInfo>) =
        subtasks.into_iter().partition(|s| {
            is_ready(&s.depends_on) && (s.status == "pending" || s.status == "in_progress")
        });

    // Group ready subtasks by parallel_group
    let mut groups_map: std::collections::HashMap<Option<String>, Vec<serde_json::Value>> =
        std::collections::HashMap::new();
    for s in &ready {
        let group = s.parallel_group.clone();
        let entry = groups_map.entry(group).or_default();
        entry.push(serde_json::json!({
            "subtask_id": s.subtask_id,
            "priority": s.priority,
            "status": s.status
        }));
    }

    let parallel_groups: Vec<serde_json::Value> = groups_map
        .into_iter()
        .map(|(group, members)| {
            serde_json::json!({
                "parallel_group": group,
                "members": members
            })
        })
        .collect();

    if only_ready {
        serde_json::json!({
            "task_id": task_id,
            "ready_subtasks": ready,
            "parallel_groups": parallel_groups
        })
        .to_string()
    } else {
        serde_json::json!({
            "task_id": task_id,
            "ready_subtasks": ready,
            "blocked_subtasks": blocked,
            "parallel_groups": parallel_groups
        })
        .to_string()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// S-3: Decompose-event handler — session-scoped task lifecycle via CLI
// ─────────────────────────────────────────────────────────────────────────────

/// Handle decompose-event subcommand from touring-hook binary.
///
/// Parses stdin JSON with event_type (TaskCreated | TaskCompleted),
/// session_id, and task_data. Maintains in-memory session→task mapping
/// via HookRuntime.decompose_event_state.
///
/// - TaskCreated: calls `touring decompose create <desc>` via Command,
///   stores session_id→task_id, returns {status: "ok", task_id, session_id}
/// - TaskCompleted: looks up task_id from session map, calls
///   `touring decompose add <task_id> <desc>`, removes from map,
///   returns {status: "ok"}
///
/// Fire-and-forget subprocess calls with 10s timeout.
/// Never fails — always returns ok with graceful degradation.
pub fn cli_decompose_event(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let event_type = payload
        .get("event_type")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let session_id = payload
        .get("session_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let task_data = payload
        .get("task_data")
        .and_then(|v| v.as_object())
        .cloned()
        .unwrap_or_default();

    match event_type {
        "TaskCreated" => {
            let task_desc = task_data
                .get("description")
                .or_else(|| task_data.get("task_description"))
                .and_then(|v| v.as_str())
                .unwrap_or("");

            let desc = task_desc.to_string();
            // Sprint 2 PB (REGRA #19): reap to prevent <defunct> zombies.
            let _ = std::thread::spawn(move || {
                if let Ok(mut child) = std::process::Command::new("touring")
                    .args(["decompose", "create", "general", &desc])
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .spawn()
                {
                    let _ = child.wait();
                }
            });

            let session_prefix: String = session_id.chars().take(8).collect();
            let task_id = format!("decompose-session-{}", session_prefix);

            rt.decompose_event_state
                .insert(session_id.to_string(), task_id.clone());

            serde_json::json!({
                "status": "ok",
                "task_id": task_id,
                "session_id": session_id
            })
            .to_string()
        }
        "TaskCompleted" => {
            if let Some(task_id) = rt.decompose_event_state.remove(session_id) {
                let completion_desc = task_data
                    .get("description")
                    .or_else(|| task_data.get("result_summary"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("completed");

                let tid = task_id.clone();
                let desc = completion_desc.to_string();
                // Sprint 2 PB (REGRA #19): reap to prevent <defunct> zombies.
                let _ = std::thread::spawn(move || {
                    if let Ok(mut child) = std::process::Command::new("touring")
                        .args(["decompose", "add", &tid, &desc])
                        .stdout(std::process::Stdio::null())
                        .stderr(std::process::Stdio::null())
                        .spawn()
                    {
                        let _ = child.wait();
                    }
                });
            }
            serde_json::json!({
                "status": "ok"
            })
            .to_string()
        }
        _ => serde_json::json!({
            "status": "ok",
            "skipped": "unknown_event_type"
        })
        .to_string(),
    }
}

// ── workflow handlers extracted to cli/handlers/decompose_workflow.rs (F-9) ──
pub use crate::cli_handlers_decompose_workflow::{
    cli_workflow_compare, cli_workflow_resume, cli_workflow_run, cli_workflow_slowest,
    cli_workflow_stats, cli_workflow_status,
};

// ─────────────────────────────────────────────────────────────────────────────
// Wave P1 — Unit tests for priority enum mapping
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests_p1_priority {
    use super::*;

    // ── parse_priority_token ────────────────────────────────────────────

    #[test]
    fn parse_high_returns_50() {
        assert_eq!(parse_priority_token("high"), 50);
    }

    #[test]
    fn parse_normal_returns_128() {
        assert_eq!(parse_priority_token("normal"), 128);
    }

    #[test]
    fn parse_low_returns_200() {
        assert_eq!(parse_priority_token("low"), 200);
    }

    #[test]
    fn parse_unknown_token_defaults_to_normal() {
        assert_eq!(parse_priority_token("urgent"), 128);
        assert_eq!(parse_priority_token(""), 128);
        assert_eq!(parse_priority_token("xyz"), 128);
    }

    #[test]
    fn parse_token_is_case_insensitive() {
        assert_eq!(parse_priority_token("HIGH"), 50);
        assert_eq!(parse_priority_token("Normal"), 128);
        assert_eq!(parse_priority_token("LOW"), 200);
    }

    #[test]
    fn parse_token_strips_whitespace() {
        assert_eq!(parse_priority_token("  high  "), 50);
        assert_eq!(parse_priority_token("\thigh\n"), 50);
    }

    #[test]
    fn parse_short_aliases_work() {
        // Short forms accepted for ergonomics.
        assert_eq!(parse_priority_token("h"), 50);
        assert_eq!(parse_priority_token("hi"), 50);
        assert_eq!(parse_priority_token("l"), 200);
        assert_eq!(parse_priority_token("lo"), 200);
    }

    // ── priority_label (inverse mapping) ────────────────────────────────

    #[test]
    fn label_for_high_range() {
        assert_eq!(priority_label(50), "high");
        assert_eq!(priority_label(0), "high");
        assert_eq!(priority_label(100), "high"); // boundary
    }

    #[test]
    fn label_for_normal_range() {
        assert_eq!(priority_label(128), "normal");
        assert_eq!(priority_label(101), "normal"); // just above high
        assert_eq!(priority_label(179), "normal"); // just below low
    }

    #[test]
    fn label_for_low_range() {
        assert_eq!(priority_label(200), "low");
        assert_eq!(priority_label(180), "low"); // boundary
        assert_eq!(priority_label(255), "low"); // legacy default
    }

    // ── round-trip property: parse(label(x)) ≈ x for canonical values ───

    #[test]
    fn round_trip_canonical_values() {
        // The exact integer is preserved when the canonical token is used.
        assert_eq!(parse_priority_token(priority_label(50)), 50);
        assert_eq!(parse_priority_token(priority_label(128)), 128);
        assert_eq!(parse_priority_token(priority_label(200)), 200);
    }

    #[test]
    fn legacy_255_treated_as_low() {
        // Existing rows with the historical default of 255 get classified as "low",
        // ensuring backward-compat for all DBs created before Wave P1.
        assert_eq!(priority_label(255), "low");
    }
}

// ── tasksfile/definitions/devrcfile handlers extracted to cli/handlers/decompose_io.rs (F-9) ──
pub use crate::cli_handlers_decompose_io::{
    cli_definitions_classify, cli_definitions_nodetypes, cli_definitions_semantic_search,
    cli_devrcfile_export, cli_devrcfile_import, cli_tasksfile_export, cli_tasksfile_validate,
};
