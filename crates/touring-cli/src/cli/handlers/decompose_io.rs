//! Tasksfile / definitions / devrcfile CLI handlers — extracted from decompose.rs (F-9).
//! Re-exported from `cli_handlers_decompose` so historic call paths resolve unchanged.

use crate::cli_handlers_decompose::ensure_decompose_tables;
use crate::runtime::HookRuntime;
use rusqlite::params;

// ── T2.4/T2.5/T2.6: Tasksfile CLI handlers ──────────────────────────────────

/// Handle `touring tasksfile validate <file>` — parse and validate a Tasksfile YAML.
pub fn cli_tasksfile_validate(_rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let yaml = payload.get("yaml").and_then(|v| v.as_str()).unwrap_or("");
    if yaml.is_empty() {
        return serde_json::json!({
            "error": "No YAML content provided",
            "success": false,
        })
        .to_string();
    }
    match touring_orchestration::tasks::parse_yaml(yaml) {
        Ok(root) => {
            // Basic schema validation
            let task_count = root.tasks.len();
            let template_count = root.templates.len();
            serde_json::json!({
                "success": true,
                "valid": true,
                "version": root.version,
                "metadata_name": root.metadata.name,
                "task_count": task_count,
                "template_count": template_count,
                "task_names": root.tasks.keys().collect::<Vec<_>>(),
                "hooks": {
                    "before_all": root.hooks.before_all.len(),
                    "after_all": root.hooks.after_all.len(),
                    "on_failure": root.hooks.on_failure.len(),
                },
            })
            .to_string()
        }
        Err(e) => serde_json::json!({
            "success": false,
            "valid": false,
            "error": e.to_string(),
        })
        .to_string(),
    }
}

/// Handle `touring tasksfile export <task_id>` — export a decompose task to Tasksfile YAML.
pub fn cli_tasksfile_export(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let task_id = payload
        .get("task_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if task_id.is_empty() {
        return serde_json::json!({
            "error": "No task_id provided",
            "success": false,
        })
        .to_string();
    }

    let db = &rt.ctx.knowledge;
    ensure_decompose_tables(db);

    // Get task info
    let task_row: Option<(String, String)> = db
        .conn_ref()
        .query_row(
            "SELECT task_id, description FROM task_decompositions WHERE task_id = ?1",
            params![task_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .ok();

    let (task_desc, _task_type) =
        task_row.unwrap_or_else(|| (task_id.to_string(), "general".to_string()));

    // Get all subtasks
    let mut stmt = match db.conn_ref().prepare(
        "SELECT subtask_id, description, depends_on, priority, status, deadline, deadline_behavior, review_required FROM decomposition_subtasks WHERE task_id = ?1",
    ) {
        Ok(s) => s,
        Err(e) => return serde_json::json!({"error": e.to_string(), "success": false}).to_string(),
    };

    let rows = match stmt.query_map(params![task_id], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, i32>(3)?,
            r.get::<_, String>(4)?,
            r.get::<_, Option<String>>(5)?,
            r.get::<_, Option<String>>(6)?,
            r.get::<_, i32>(7)?,
        ))
    }) {
        Ok(r) => r.filter_map(|x| x.ok()).collect::<Vec<_>>(),
        Err(e) => return serde_json::json!({"error": e.to_string(), "success": false}).to_string(),
    };

    // Build Tasksfile YAML structure
    let tasks: serde_json::Map<String, serde_json::Value> = rows
        .iter()
        .map(
            |(
                subtask_id,
                desc,
                deps_json,
                priority,
                _status,
                deadline,
                deadline_behavior,
                review_required,
            )| {
                let mut task_map = serde_json::Map::new();
                task_map.insert("desc".to_string(), serde_json::json!(desc));
                // Parse depends_on JSON
                let deps: Vec<String> = serde_json::from_str(deps_json).unwrap_or_default();
                if !deps.is_empty() {
                    task_map.insert("deps".to_string(), serde_json::json!(deps));
                }
                // Priority
                let priority_label = match *priority {
                    v if v <= 100 => "high",
                    v if v >= 180 => "low",
                    _ => "normal",
                };
                task_map.insert("tags".to_string(), serde_json::json!([priority_label]));
                // Deadline
                if let Some(dl) = deadline {
                    task_map.insert("deadline".to_string(), serde_json::json!(dl));
                }
                // Deadline behavior
                if let Some(dbg) = deadline_behavior {
                    task_map.insert("deadline_behavior".to_string(), serde_json::json!(dbg));
                }
                // Review required
                if *review_required != 0 {
                    task_map.insert("review_required".to_string(), serde_json::json!(true));
                }
                (subtask_id.clone(), serde_json::Value::Object(task_map))
            },
        )
        .collect();

    let root = serde_json::json!({
        "version": "1.0",
        "metadata": {
            "name": task_id,
            "description": task_desc,
        },
        "tasks": tasks,
    });

    serde_json::json!({
        "success": true,
        "tasksfile_yaml": serde_yaml::to_string(&root).unwrap_or_default(),
    })
    .to_string()
}

// ─── D31: touring definitions CLI (classify / node-types / semantic-search) ───

use crate::semantic_classifier::SemanticClassifier;
use touring_code::ast::node_types::node_types_for_language;

/// Classify a prompt into CILA complexity level using SemanticClassifier.
///
/// Input payload: `{"prompt": "..."}`
/// Output: `{"prompt": "...", "level": 4, "strategy": "aco_orchestration", "similarity": 0.87}`
pub fn cli_definitions_classify(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let _ = rt; // unused in current implementation
    let prompt = payload.get("prompt").and_then(|v| v.as_str()).unwrap_or("");
    let classifier = SemanticClassifier::new().with_standard_patterns();
    match classifier.classify(prompt) {
        Some(result) => serde_json::json!({
            "prompt": prompt,
            "level": result.level,
            "strategy": result.strategy,
            "score": result.score,
        })
        .to_string(),
        None => serde_json::json!({"prompt": prompt, "level": serde_json::Value::Null}).to_string(),
    }
}

/// List node types (syntax categories) available for a given language.
///
/// Input payload: `{"lang": "rust", "threshold": 0.5}`
/// Output: `{"lang": "rust", "node_types": [...], "threshold": 0.5}`
pub fn cli_definitions_nodetypes(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let _ = rt;
    let lang = payload
        .get("lang")
        .and_then(|v| v.as_str())
        .unwrap_or("rust");
    let threshold = payload
        .get("threshold")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.5);
    let node_types = node_types_for_language(lang);
    serde_json::json!({
        "lang": lang,
        "node_types": node_types,
        "threshold": threshold,
    })
    .to_string()
}

/// Search the semantic graph via touring-cognitive integration.
///
/// Input payload: `{"query": "search term", "top_k": 5}`
/// Output: `{"query": "...", "results": [...]}` — returns matching definitions
/// from the cognitive runtime's SemanticGraph, ranked by cosine similarity
/// of TF-IDF embeddings.
///
/// Falls back to empty results if the query is empty or if the cognitive
/// runtime is not initialized.
pub fn cli_definitions_semantic_search(
    rt: &mut HookRuntime,
    payload: &serde_json::Value,
) -> String {
    let query = payload.get("query").and_then(|v| v.as_str()).unwrap_or("");
    let top_k = payload.get("top_k").and_then(|v| v.as_u64()).unwrap_or(5) as usize;

    if query.trim().is_empty() {
        return serde_json::json!({
            "query": query,
            "results": [],
            "note": "empty query",
        })
        .to_string();
    }

    // Access the cognitive runtime's SemanticGraph via HookRuntime.
    let Some(ref cognitive) = rt.cognitive else {
        return serde_json::json!({
            "query": query,
            "results": [],
            "note": "cognitive runtime not initialized",
        })
        .to_string();
    };

    // Use the CognitiveNexus's TF-IDF vectorizer to embed the query.
    let nexus = cognitive.nexus();
    let query_emb = {
        let v = match nexus.vectorizer().read() {
            Ok(v) => v,
            Err(_) => {
                return serde_json::json!({
                    "query": query,
                    "results": [],
                    "note": "vectorizer lock poisoned",
                })
                .to_string();
            }
        };
        v.embed(query)
    };

    // Retrieve top-k matching nodes using the existing SemanticGraph method.
    let graph = cognitive.graph();
    let matches = graph.retrieve_by_embedding(&query_emb, top_k);

    let results: Vec<serde_json::Value> = matches
        .into_iter()
        .map(|n| {
            serde_json::json!({
                "id": n.id,
                "label": n.label,
                "node_type": format!("{:?}", n.node_type),
                "metadata": n.metadata,
            })
        })
        .collect();

    serde_json::json!({
        "query": query,
        "results": results,
    })
    .to_string()
}

// ── Devrcfile CLI handlers ────────────────────────────────────────────────

/// Handle `touring devrcfile import <file>` — parse Devrcfile YAML, convert to Tasksfile, create DAG.
pub fn cli_devrcfile_import(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let yaml = payload
        .get("devrcfile_yaml")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let file_path = payload
        .get("file_path")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    if yaml.is_empty() {
        return serde_json::json!({
            "error": "No Devrcfile YAML content provided",
            "success": false,
        })
        .to_string();
    }

    // Parse Devrcfile YAML
    let devrc = match touring_orchestration::devrc::parse_devrcfile(yaml) {
        Ok(d) => d,
        Err(e) => {
            return serde_json::json!({
                "success": false,
                "error": format!("Failed to parse Devrcfile: {}", e),
            })
            .to_string();
        }
    };

    // Convert to Tasksfile
    let result = match touring_orchestration::devrc::devrcfile_to_tasksfile(&devrc) {
        Ok(r) => r,
        Err(e) => {
            return serde_json::json!({
                "success": false,
                "error": format!("Failed to convert Devrcfile to Tasksfile: {}", e),
            })
            .to_string();
        }
    };

    let warnings = result.warnings;
    let tasksfile = result.tasksfile;

    // Create decompose DAG — same pattern as cli_decompose_create
    let task_id = format!(
        "devrc_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    );
    let now = chrono::Utc::now().to_rfc3339();

    let db = &rt.ctx.knowledge;
    ensure_decompose_tables(db);

    // Insert task_decompositions entry
    let task_desc = format!("Devrcfile import: {}", file_path);
    if let Err(e) = db.conn_ref().execute(
        "INSERT OR IGNORE INTO task_decompositions (task_id, task_type, description, status, created_at, updated_at) VALUES (?1, ?2, ?3, 'created', ?4, ?4)",
        params![task_id, "devrcfile-import", task_desc, now],
    ) {
        return serde_json::json!({
            "success": false,
            "error": format!("Failed to create task: {}", e),
        }).to_string();
    }

    // Insert each subtask from Tasksfile
    let mut subtask_ids = Vec::new();
    for (task_key, task_def) in &tasksfile.tasks {
        let subtask_id = format!("{}::{}", task_id, task_key);
        let deps_json = serde_json::to_string(&task_def.deps).unwrap_or_else(|_| "[]".to_string());
        let deadline_behavior = task_def.deadline_behavior.as_deref().unwrap_or("Fail");
        // Priority from tags (high <= 100, normal 101-179, low >= 180)
        let priority = task_def
            .tags
            .iter()
            .find(|t| t.starts_with("priority:"))
            .and_then(|t| {
                let name = t.trim_start_matches("priority:");
                match name {
                    "high" => Some(50_i64),
                    "normal" => Some(150_i64),
                    "low" => Some(220_i64),
                    _ => None,
                }
            })
            .unwrap_or(150_i64);

        if let Err(e) = db.conn_ref().execute(
            "INSERT OR REPLACE INTO decomposition_subtasks \
             (subtask_id, task_id, description, depends_on, priority, status, \
              deadline, deadline_behavior, parallel_group, review_required, \
              complexity_hint, retry_policy, attempts, quality_score, created_at, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, 'pending', ?6, ?7, NULL, 0, ?8, ?9, 0, NULL, ?10, ?11)",
            params![
                subtask_id,
                task_id,
                task_def.desc.as_deref().unwrap_or(""),
                deps_json,
                priority,
                task_def.deadline.as_deref(),
                deadline_behavior,
                task_def.review_required as i32,
                task_def.complexity_hint.as_deref().unwrap_or(""),
                "{}", // retry_policy
                now,
                now,
            ],
        ) {
            return serde_json::json!({
                "success": false,
                "error": format!("Failed to create subtask {}: {}", subtask_id, e),
            })
            .to_string();
        }
        subtask_ids.push(subtask_id);
    }

    // Handle templates — store as metadata JSON for reference
    let template_names: Vec<String> = tasksfile.templates.keys().cloned().collect();

    tracing::debug!(
        "devrcfile import created task {} with {} subtasks",
        task_id,
        subtask_ids.len()
    );

    serde_json::json!({
        "success": true,
        "task_id": task_id,
        "tasksfile_yaml": serde_yaml::to_string(&tasksfile).unwrap_or_default(),
        "warnings": warnings,
        "task_count": tasksfile.tasks.len(),
        "template_count": tasksfile.templates.len(),
        "subtask_ids": subtask_ids,
        "template_names": template_names,
    })
    .to_string()
}

/// Handle `touring devrcfile export <task_id>` — export a decompose task to Devrcfile YAML.
pub fn cli_devrcfile_export(rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let task_id = payload
        .get("task_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if task_id.is_empty() {
        return serde_json::json!({
            "error": "No task_id provided",
            "success": false,
        })
        .to_string();
    }

    let db = &rt.ctx.knowledge;
    ensure_decompose_tables(db);

    // Get task info
    let task_row: Option<(String, String)> = db
        .conn_ref()
        .query_row(
            "SELECT task_id, description FROM task_decompositions WHERE task_id = ?1",
            params![task_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .ok();

    let (task_desc, _task_type) =
        task_row.unwrap_or_else(|| (task_id.to_string(), "general".to_string()));

    // Get all subtasks
    let mut stmt = match db.conn_ref().prepare(
        "SELECT subtask_id, description, depends_on, priority, status, deadline, deadline_behavior, review_required FROM decomposition_subtasks WHERE task_id = ?1",
    ) {
        Ok(s) => s,
        Err(e) => return serde_json::json!({"error": e.to_string(), "success": false}).to_string(),
    };

    let rows = match stmt.query_map(params![task_id], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, i32>(3)?,
            r.get::<_, String>(4)?,
            r.get::<_, Option<String>>(5)?,
            r.get::<_, Option<String>>(6)?,
            r.get::<_, i32>(7)?,
        ))
    }) {
        Ok(r) => r.filter_map(|x| x.ok()).collect::<Vec<_>>(),
        Err(e) => return serde_json::json!({"error": e.to_string(), "success": false}).to_string(),
    };

    // Build Tasksfile YAML structure
    let tasks: serde_json::Map<String, serde_json::Value> = rows
        .iter()
        .map(
            |(
                subtask_id,
                desc,
                deps_json,
                priority,
                _status,
                deadline,
                deadline_behavior,
                review_required,
            )| {
                let mut task_map = serde_json::Map::new();
                task_map.insert("desc".to_string(), serde_json::json!(desc));
                // Parse depends_on JSON
                let deps: Vec<String> = serde_json::from_str(deps_json).unwrap_or_default();
                if !deps.is_empty() {
                    task_map.insert("deps".to_string(), serde_json::json!(deps));
                }
                // Priority
                let priority_label = match *priority {
                    v if v <= 100 => "high",
                    v if v >= 180 => "low",
                    _ => "normal",
                };
                task_map.insert("tags".to_string(), serde_json::json!([priority_label]));
                // Deadline
                if let Some(dl) = deadline {
                    task_map.insert("deadline".to_string(), serde_json::json!(dl));
                }
                // Deadline behavior
                if let Some(dbg) = deadline_behavior {
                    task_map.insert("deadline_behavior".to_string(), serde_json::json!(dbg));
                }
                // Review required
                if *review_required != 0 {
                    task_map.insert("review_required".to_string(), serde_json::json!(true));
                }
                (subtask_id.clone(), serde_json::Value::Object(task_map))
            },
        )
        .collect();

    let root = serde_json::json!({
        "version": "1.0",
        "metadata": {
            "name": task_id,
            "description": task_desc,
        },
        "tasks": tasks,
    });

    serde_json::json!({
        "success": true,
        "tasksfile_yaml": serde_yaml::to_string(&root).unwrap_or_default(),
    })
    .to_string()
}
