use super::*;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use touring_intelligence::rl::aco::PheroKey;

unsafe extern "C" {
    fn getuid() -> u32;
}

/// Safe wrapper around `libc::getuid` — returns the effective user id of the
/// process. Used to compose the canonical daemon socket path
/// `/tmp/touring-daemon-<uid>.sock` and to annotate audit-trail events with
/// the originating uid. Marked `pub(crate)` so peer tool modules
/// (`tools_core::default_entity_db_path`, audit hooks) can share the
/// single FFI surface.
#[must_use]
pub(crate) fn current_uid() -> u32 {
    // SAFETY: `getuid` is a no-arg POSIX syscall; always safe to call,
    // never fails, has no side effects.
    unsafe { getuid() }
}

/// Daemon socket path derived from the current user's uid — the canonical
/// location used by `touring serve` and `touring-hook --start-daemon`.
#[must_use]
pub(crate) fn daemon_socket_path() -> std::path::PathBuf {
    // W12.5 unification (2026-07-24): the old uid-only copy ignored BOTH env
    // overrides and the per-project walk-up — MCP analysis tools always hit
    // the global daemon even inside an opted-in project.
    touring_foundation::config::TouringConfig::resolve_daemon_socket_path()
}

#[tool_router(router = router_analysis, vis = "pub(crate)")]
impl TouringServer {
    // ── touring_graph — Dependency map and symbol relationships ──────────

    /// Dependency graph: index files, blast radius, dependency paths, import extraction, symbol query, reload from DB
    #[tool(
        name = "touring_graph",
        description = "Dependency graph: index files, blast radius, dependency paths, import extraction, symbol query, reload from DB, neighbors (1-hop expansion)"
    )]
    async fn graph(&self, params: Parameters<GraphParams>) -> Result<CallToolResult, McpError> {
        let p = params.0;

        // Tier A: update focus + resolve graph context
        if let Some(ref file) = p.file_path {
            self.graph_svc.update_focus(file).await;
        }
        let gctx = self.graph_svc.resolve_ctx(p.file_path.as_deref()).await;
        let cognitive_hint = p.file_path.clone().unwrap_or_else(|| p.action.clone());

        let mut output = match p.action.as_str() {
            "index" => {
                let files = p.files.ok_or_else(|| {
                    McpError::invalid_params("'files' required for index action", None)
                })?;

                let mut idx = self.graph_svc.index().lock().await;
                let mut indexed = 0u64;
                let mut errors: Vec<String> = Vec::new();

                for f in &files {
                    let lang: touring_code::ast::languages::Lang = match f.language.parse() {
                        Ok(l) => l,
                        Err(e) => {
                            errors.push(format!(
                                "{}: unsupported language '{}' — {}",
                                f.path, f.language, e
                            ));
                            continue;
                        }
                    };
                    match idx.index_file(&f.path, &f.content, lang) {
                        Ok(()) => indexed += 1,
                        Err(e) => errors.push(format!("{}: {}", f.path, e)),
                    }
                }

                // Persist to SymbolStore after indexing
                if let Some(ref store_arc) = self.symbol_store {
                    let store = store_arc.lock().await;
                    if let Err(e) = idx.persist_to(&store) {
                        warn!("SymbolStore persist failed: {}", e);
                        errors.push(format!("persist_warning: {}", e));
                    }
                }

                let stats = idx.stats();
                serde_json::json!({
                    "action": "index",
                    "files_indexed": indexed,
                    "errors": errors,
                    "stats": {
                        "total_symbols": stats.total_symbols,
                        "total_locations": stats.total_locations,
                        "total_files": stats.total_files,
                        "total_dependencies": stats.total_dependencies,
                    }
                })
            }
            "blast_radius" => {
                let symbol = p.symbol.ok_or_else(|| {
                    McpError::invalid_params("'symbol' (file path) required for blast_radius", None)
                })?;

                let idx = self.graph_svc.index().lock().await;
                let radius = idx.blast_radius(&symbol);

                serde_json::json!({
                    "action": "blast_radius",
                    "start_file": radius.start_file,
                    "affected_files": radius.affected_files,
                    "affected_symbols": radius.affected_symbols.iter()
                        .map(|(f, s)| serde_json::json!({"file": f, "symbol": s}))
                        .collect::<Vec<_>>(),
                    "max_distance": radius.max_distance,
                    "file_count": radius.file_count,
                })
            }
            "dependency_path" => {
                let from = p.from.ok_or_else(|| {
                    McpError::invalid_params("'from' required for dependency_path", None)
                })?;
                let to = p.to.ok_or_else(|| {
                    McpError::invalid_params("'to' required for dependency_path", None)
                })?;

                let idx = self.graph_svc.index().lock().await;
                let path = idx.dependency_path(&from, &to);

                serde_json::json!({
                    "action": "dependency_path",
                    "from": from,
                    "to": to,
                    "path": path,
                    "found": path.is_some(),
                })
            }
            "imports" => {
                let content = p.content.ok_or_else(|| {
                    McpError::invalid_params("'content' required for imports", None)
                })?;
                let lang_str = p.language.unwrap_or_else(|| "python".to_string());
                let lang: touring_code::ast::languages::Lang = lang_str
                    .parse()
                    .map_err(|e: String| McpError::invalid_params(e, None))?;

                let imports = touring_code::ast::graph::extract_imports(&content, lang);
                let imports_json: Vec<serde_json::Value> = imports
                    .iter()
                    .map(|imp| {
                        serde_json::json!({
                            "module_path": imp.module_path,
                            "symbols": imp.symbols,
                        })
                    })
                    .collect();

                serde_json::json!({
                    "action": "imports",
                    "language": lang_str,
                    "import_count": imports.len(),
                    "imports": imports_json,
                })
            }
            "query" => {
                let pattern = p.pattern.ok_or_else(|| {
                    McpError::invalid_params("'pattern' required for query", None)
                })?;

                let idx = self.graph_svc.index().lock().await;
                let results = idx.query_symbols(&pattern, p.kind.as_deref());

                let results_json: Vec<serde_json::Value> = results
                    .iter()
                    .map(|loc| {
                        serde_json::json!({
                            "file_path": loc.file_path,
                            "symbol_name": loc.symbol_name,
                            "line": loc.line,
                            "column": loc.column,
                            "is_definition": loc.is_definition,
                        })
                    })
                    .collect();

                serde_json::json!({
                    "action": "query",
                    "pattern": pattern,
                    "result_count": results.len(),
                    "results": results_json,
                })
            }
            "reload" => {
                // Reload symbols from the persisted SymbolStore DB into the in-memory index.
                // Use after external tools (e.g. bootstrap scripts) write directly to symbols.db.
                let mut idx = self.graph_svc.index().lock().await;
                // Clear first — prevents duplicate accumulation on repeated reloads
                idx.clear();
                let mut loaded = 0usize;
                let mut error_msg: Option<String> = None;

                if let Some(ref store_arc) = self.symbol_store {
                    let store = store_arc.lock().await;
                    match store.load_into_index(&mut idx) {
                        Ok(n) => loaded = n,
                        Err(e) => error_msg = Some(format!("load failed: {}", e)),
                    }
                } else {
                    error_msg = Some("SymbolStore not initialized".to_string());
                }

                let stats = idx.stats();
                serde_json::json!({
                    "action": "reload",
                    "symbols_loaded": loaded,
                    "error": error_msg,
                    "stats": {
                        "total_symbols": stats.total_symbols,
                        "total_locations": stats.total_locations,
                        "total_files": stats.total_files,
                        "total_dependencies": stats.total_dependencies,
                    }
                })
            }
            "neighbors" => {
                let file = p.file_path.ok_or_else(|| {
                    McpError::invalid_params("'file_path' required for neighbors action", None)
                })?;

                // EC43: First production caller of GraphService::expand_neighbors().
                // Returns imports ∪ imported_by (deduped, sorted) up to 20 files — used for
                // context/query expansion when the LLM needs the full 1-hop neighborhood.
                let neighbors = self.graph_svc.expand_neighbors(&file, 20).await;
                let neighbor_count = neighbors.len();

                serde_json::json!({
                    "action": "neighbors",
                    "file": file,
                    "neighbors": neighbors,
                    "neighbor_count": neighbor_count,
                })
            }
            _ => {
                return Err(McpError::invalid_params(
                    format!(
                        "Unknown graph action: '{}'. Valid: index, blast_radius, dependency_path, imports, query, reload, neighbors",
                        p.action
                    ),
                    None,
                ));
            }
        };

        self.graph_svc.inject(&mut output, &gctx);

        // CognitiveNexus: inject predictive context
        let cctx = self.nexus.resolve("touring_graph", &cognitive_hint).await;
        if !cctx.is_empty() {
            match serde_json::to_value(&cctx) {
                #[allow(clippy::indexing_slicing)]
                // SAFETY: serde_json::Value string indexing never panics
                Ok(v) => {
                    output["cognitive_ctx"] = v;
                }
                Err(e) => {
                    tracing::warn!("cognitive_ctx serialize failed: {e}");
                }
            }
        }

        let dl = p.detail_level.unwrap_or_default();
        params::apply_detail_level(&mut output, dl);
        crate::tools::suggestions::append_to_response(&mut output, "touring_graph", 2);
        let text = serde_json::to_string_pretty(&output)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    // ── touring_decompose — Structured task decomposition (CoT) ─────────

    /// Task decomposition: create/delete plans, add subtasks with dependencies, track status, validate DAG order
    #[tool(
        annotations(
            read_only_hint = false,
            idempotent_hint = false,
            title = "Decompose task into DAG"
        ),
        name = "touring_decompose",
        description = "Decompose a task into a validated dependency DAG (create/delete plans, add subtasks with dependencies, validate execution order). Use for multi-step work. Actions: create, delete, add_subtask, update_status, get_plan, list_tasks, get_ready_subtasks, validate_order, validate_completion, finalize"
    )]
    async fn decompose(
        &self,
        params: Parameters<DecomposeParams>,
    ) -> Result<CallToolResult, McpError> {
        let p = params.0;
        let gctx = self.graph_svc.resolve_ctx(None).await;

        // Save hint for CognitiveNexus before description is moved
        let cognitive_hint = p.description.clone().unwrap_or_else(|| p.action.clone());

        let mut output = match p.action.as_str() {
            "create" => {
                let task_type = p.task_type.unwrap_or_else(|| "feature".to_string());
                let description = p.description.unwrap_or_default();
                let cila_level = p.cila_level;

                let (task_id, profile) = {
                    let mut dec = self.decomposer.write().await;
                    // EC50: First production caller of create_task() — routes to the convenience
                    // wrapper when no explicit cila_level is provided (default L3 semantics).
                    // Wave C3-D3: when auto_decompose=true and cila_level>=3, the granularity
                    // bandit is queried and placeholder subtasks are pre-scaffolded.
                    let effective_level = cila_level.unwrap_or(3);
                    let task_id = if p.auto_decompose.unwrap_or(false) {
                        let hint = crate::reasoning::query_granularity_hint(
                            description.len(), // rough LOC proxy from description length
                            "rust",
                            effective_level,
                        );
                        dec.create_task_with_cila_and_hint(
                            &task_type,
                            &description,
                            effective_level,
                            Some(&hint),
                        )
                    } else {
                        match cila_level {
                            Some(level) => {
                                dec.create_task_with_cila(&task_type, &description, level)
                            }
                            None => dec.create_task(&task_type, &description),
                        }
                    };
                    let profile = dec.get_plan(&task_id).map(|t| {
                        serde_json::json!({
                            "cila_level": t.cila_level,
                            "routing_mode": t.profile().routing_mode,
                            "max_parallelism": t.profile().max_parallelism,
                            "pheromone_enabled": t.profile().pheromone_enabled,
                            "mcts_enabled": t.profile().mcts_enabled,
                        })
                    });
                    (task_id, profile)
                };
                // checkpoint after write (ignore errors — non-critical)
                if let Err(e) = self
                    .checkpoint_manager
                    .lock()
                    .await
                    .checkpoint(&*self.decomposer.read().await)
                {
                    tracing::debug!("checkpoint after create failed: {}", e);
                }

                serde_json::json!({
                    "action": "create",
                    "task_id": task_id,
                    "task_type": task_type,
                    "description": description,
                    "cila_level": cila_level,
                    "status": "created",
                    "profile": profile,
                })
            }
            "delete" => {
                let task_id = p.task_id.ok_or_else(|| {
                    McpError::invalid_params("'task_id' required for delete", None)
                })?;

                let deleted = {
                    let mut dec = self.decomposer.write().await;
                    dec.delete_task(&task_id)
                };
                if let Err(e) = self
                    .checkpoint_manager
                    .lock()
                    .await
                    .checkpoint(&*self.decomposer.read().await)
                {
                    tracing::debug!("checkpoint after delete failed: {}", e);
                }

                serde_json::json!({
                    "action": "delete",
                    "task_id": task_id,
                    "deleted": deleted,
                })
            }
            "add_subtask" => {
                let task_id = p.task_id.ok_or_else(|| {
                    McpError::invalid_params("'task_id' required for add_subtask", None)
                })?;
                let description = p.description.unwrap_or_default();
                let depends_on = p.depends_on.unwrap_or_default();
                let priority = p.priority.unwrap_or(0);

                let subtask_id = {
                    let mut dec = self.decomposer.write().await;
                    dec.add_subtask(&task_id, &description, depends_on.clone(), priority)
                        .map_err(|e| McpError::internal_error(e, None))?
                };
                if let Err(e) = self
                    .checkpoint_manager
                    .lock()
                    .await
                    .checkpoint(&*self.decomposer.read().await)
                {
                    tracing::debug!("checkpoint after add_subtask failed: {}", e);
                }

                serde_json::json!({
                    "action": "add_subtask",
                    "task_id": task_id,
                    "subtask_id": subtask_id,
                    "description": description,
                    "depends_on": depends_on,
                    "priority": priority,
                    "status": "added",
                })
            }
            "update_status" => {
                let task_id = p.task_id.ok_or_else(|| {
                    McpError::invalid_params("'task_id' required for update_status", None)
                })?;
                let subtask_id = p.subtask_id.ok_or_else(|| {
                    McpError::invalid_params("'subtask_id' required for update_status", None)
                })?;
                let status_str = p.status.ok_or_else(|| {
                    McpError::invalid_params("'status' required for update_status", None)
                })?;
                let status: crate::reasoning::SubTaskStatus = status_str
                    .parse()
                    .map_err(|e| McpError::invalid_params(e, None))?;

                // Drain ACO events while holding the write lock
                let aco_events = {
                    let mut dec = self.decomposer.write().await;
                    dec.update_status(&task_id, &subtask_id, status)
                        .map_err(|e| McpError::internal_error(e, None))?;
                    dec.drain_pending_aco_events()
                };
                // W4-3: Deposit ACO pheromone events to shared bus
                for event in aco_events.iter() {
                    let key = PheroKey::TaskId(event.phero_key());
                    self.aco_bus.deposit(key, event.pheromone_delta());
                }
                // checkpoint after lock released
                if let Err(e) = self
                    .checkpoint_manager
                    .lock()
                    .await
                    .checkpoint(&*self.decomposer.read().await)
                {
                    tracing::debug!("checkpoint after update_status failed: {}", e);
                }

                // Spawn ACO pheromone events (fire-and-forget, outside the lock)
                let mut aco_emitted = Vec::new();
                let mut pheromone_signals: Vec<serde_json::Value> = Vec::new();
                let hook_path = self
                    .config
                    .project_root
                    .join(".claude")
                    .join("hooks")
                    .join("touring-hook");
                for event in aco_events {
                    let event_key = match &event {
                        crate::reasoning::AcoEvent::TaskCompleted { task_id, .. } => {
                            format!("task_completion:{}:completed", task_id)
                        }
                        crate::reasoning::AcoEvent::TaskFailed { task_id, .. } => {
                            format!("task_failure:{}:failed", task_id)
                        }
                        crate::reasoning::AcoEvent::TaskBlocked { task_id, .. } => {
                            format!("task_blocked:{}:blocked", task_id)
                        }
                        crate::reasoning::AcoEvent::TaskStarted { task_id, .. } => {
                            format!("task_started:{}:started", task_id)
                        }
                    };
                    // Capture pheromone signals before moving event into spawn closure
                    let delta = event.pheromone_delta();
                    let phero_key = event.phero_key();
                    let task_id_clone = task_id.clone();
                    let event_key_clone = event_key.clone();
                    let phero_key_clone = phero_key.clone();
                    let hook_path_clone = hook_path.clone();
                    tokio::spawn(async move {
                        let success =
                            matches!(&event, crate::reasoning::AcoEvent::TaskCompleted { .. });
                        let _ = tokio::process::Command::new(&hook_path_clone)
                            .arg("task-completed")
                            .arg(&task_id_clone)
                            .arg(if success { "success" } else { "failure" })
                            .arg(&event_key_clone)
                            .stdin(std::process::Stdio::null())
                            .stdout(std::process::Stdio::null())
                            .stderr(std::process::Stdio::null())
                            .spawn();
                        // Inject ACO pheromone delta into RL reward engine
                        let delta_str = format!("{:.1}", delta);
                        let _ = tokio::process::Command::new("touring")
                            .arg("learning")
                            .arg("reward")
                            .arg("orchestrate")
                            .arg(&delta_str)
                            .arg(&phero_key_clone)
                            .stdin(std::process::Stdio::null())
                            .stdout(std::process::Stdio::null())
                            .stderr(std::process::Stdio::null())
                            .spawn();
                    });
                    pheromone_signals.push(serde_json::json!({
                        "phero_key": phero_key,
                        "delta": delta,
                        "event": event_key.clone(),
                    }));
                    aco_emitted.push(event_key);
                }

                serde_json::json!({
                    "action": "update_status",
                    "task_id": task_id,
                    "subtask_id": subtask_id,
                    "new_status": status_str,
                    "status": "updated",
                    "aco_events": aco_emitted,
                    "pheromone_signals": pheromone_signals,
                })
            }
            "get_plan" => {
                let task_id = p.task_id.ok_or_else(|| {
                    McpError::invalid_params("'task_id' required for get_plan", None)
                })?;

                let dec = self.decomposer.read().await;
                let plan = dec.get_plan(&task_id).ok_or_else(|| {
                    McpError::internal_error(format!("Task not found: {}", task_id), None)
                })?;

                let subtasks_json: Vec<serde_json::Value> = plan
                    .subtasks
                    .iter()
                    .map(|s| {
                        serde_json::json!({
                            "id": s.id,
                            "description": s.description,
                            "status": s.status.to_string(),
                            "depends_on": s.depends_on,
                            "priority": s.priority,
                            "created_at": s.created_at.to_rfc3339(),
                            "updated_at": s.updated_at.to_rfc3339(),
                        })
                    })
                    .collect();

                serde_json::json!({
                    "action": "get_plan",
                    "task_id": plan.id,
                    "task_type": plan.task_type,
                    "description": plan.description,
                    "created_at": plan.created_at.to_rfc3339(),
                    "subtask_count": plan.subtasks.len(),
                    "completion_pct": plan.completion_pct(),
                    "subtasks": subtasks_json,
                })
            }
            "list_tasks" => {
                let dec = self.decomposer.read().await;
                let plans = dec.list_plans();

                let tasks_json: Vec<serde_json::Value> = plans
                    .iter()
                    .map(|plan| {
                        serde_json::json!({
                            "task_id": plan.id,
                            "task_type": plan.task_type,
                            "description": plan.description,
                            "created_at": plan.created_at.to_rfc3339(),
                            "subtask_count": plan.subtasks.len(),
                            "completion_pct": plan.completion_pct(),
                        })
                    })
                    .collect();

                serde_json::json!({
                    "action": "list_tasks",
                    "task_count": tasks_json.len(),
                    "tasks": tasks_json,
                })
            }
            "get_ready_subtasks" => {
                let task_id = p.task_id.ok_or_else(|| {
                    McpError::invalid_params("'task_id' required for get_ready_subtasks", None)
                })?;

                let dec = self.decomposer.read().await;
                let plan = dec.get_plan(&task_id).ok_or_else(|| {
                    McpError::internal_error(format!("Task not found: {}", task_id), None)
                })?;

                let ready: Vec<serde_json::Value> = plan
                    .ready_subtasks()
                    .iter()
                    .map(|s| {
                        serde_json::json!({
                            "id": s.id,
                            "description": s.description,
                            "priority": s.priority,
                            "depends_on": s.depends_on,
                        })
                    })
                    .collect();

                serde_json::json!({
                    "action": "get_ready_subtasks",
                    "task_id": task_id,
                    "ready_count": ready.len(),
                    "ready_subtasks": ready,
                    "completion_pct": plan.completion_pct(),
                })
            }
            "validate_order" => {
                let task_id = p.task_id.ok_or_else(|| {
                    McpError::invalid_params("'task_id' required for validate_order", None)
                })?;

                let (valid, order, error) = {
                    let mut dec = self.decomposer.write().await;
                    match dec.validate_order(&task_id) {
                        Ok(ord) => (true, Some(ord), None),
                        Err(e) => (false, None, Some(e.to_string())),
                    }
                };

                // Graph hotspots: top-5 highest blast_radius files as advisory context.
                // Warns the planner which files will cascade changes through the project.
                let graph_hotspots: Vec<serde_json::Value> = {
                    let idx = self.graph_svc.index().lock().await;
                    let mut by_indegree: Vec<(String, usize)> = idx
                        .reverse_deps
                        .iter()
                        .map(|(f, importers)| (f.clone(), importers.len()))
                        .collect();
                    by_indegree.sort_unstable_by_key(|b| std::cmp::Reverse(b.1));
                    by_indegree
                        .into_iter()
                        .take(5)
                        .map(
                            |(file, count)| serde_json::json!({"file": file, "depended_by": count}),
                        )
                        .collect()
                };

                serde_json::json!({
                    "action": "validate_order",
                    "task_id": task_id,
                    "valid": valid,
                    "execution_order": order,
                    "error": error,
                    "graph_hotspots": graph_hotspots,
                })
            }
            "get_parallel_groups" => {
                let task_id = p.task_id.ok_or_else(|| {
                    McpError::invalid_params("'task_id' required for get_parallel_groups", None)
                })?;

                let dec = self.decomposer.read().await;
                let plan = dec.get_plan(&task_id).ok_or_else(|| {
                    McpError::internal_error(format!("Task not found: {}", task_id), None)
                })?;

                let (groups, profile) = plan.parallel_groups_with_profile();

                let groups_json: Vec<serde_json::Value> = groups
                    .iter()
                    .map(|g| {
                        serde_json::json!({
                            "depth": g.depth,
                            "subtask_ids": g.subtask_ids,
                            "all_done": g.all_done,
                        })
                    })
                    .collect();

                serde_json::json!({
                    "action": "get_parallel_groups",
                    "task_id": task_id,
                    "group_count": groups_json.len(),
                    "groups": groups_json,
                    "profile": {
                        "cila_level": format!("{:?}", profile.cila_level),
                        "routing_mode": format!("{:?}", profile.routing_mode),
                        "max_parallelism": profile.max_parallelism,
                        "pheromone_enabled": profile.pheromone_enabled,
                        "mcts_enabled": profile.mcts_enabled,
                        "validator_required": profile.validator_required,
                    },
                })
            }
            "check_deadlines" => {
                let task_id = p.task_id;

                let transitions: Vec<(String, String)> = {
                    let mut dec = self.decomposer.write().await;
                    if let Some(ref tid) = task_id {
                        // Single task deadline check
                        if let Some(task) = dec.tasks.get_mut(tid.as_str()) {
                            task.check_expired_deadlines()
                                .into_iter()
                                .map(|(sid, status)| (sid, status.to_string()))
                                .collect()
                        } else {
                            return Err(McpError::internal_error(
                                format!("Task not found: {}", tid),
                                None,
                            ));
                        }
                    } else {
                        // All tasks — sweep every task for expired deadlines
                        let mut all = Vec::new();
                        for task in dec.tasks.values_mut() {
                            for (sid, status) in task.check_expired_deadlines() {
                                all.push((sid, status.to_string()));
                            }
                        }
                        all
                    }
                };
                if let Err(e) = self
                    .checkpoint_manager
                    .lock()
                    .await
                    .checkpoint(&*self.decomposer.read().await)
                {
                    tracing::debug!("checkpoint after check_deadlines failed: {}", e);
                }

                let transitions_json: Vec<serde_json::Value> = transitions.iter()
                    .map(|(sid, status)| serde_json::json!({"subtask_id": sid, "new_status": status}))
                    .collect();

                serde_json::json!({
                    "action": "check_deadlines",
                    "task_id": task_id,
                    "expired_count": transitions_json.len(),
                    "transitions": transitions_json,
                })
            }
            "infer_deps" => {
                let task_id = p.task_id.ok_or_else(|| {
                    McpError::invalid_params("'task_id' required for infer_deps", None)
                })?;
                let description = p.description.unwrap_or_default();

                let inferred = {
                    let dec = self.decomposer.read().await;
                    dec.infer_dependencies(&task_id, &description)
                };

                serde_json::json!({
                    "action": "infer_deps",
                    "task_id": task_id,
                    "description": description,
                    "inferred_count": inferred.len(),
                    "inferred_dependencies": inferred,
                })
            }
            "validate_completion" => {
                let task_id = p.task_id.ok_or_else(|| {
                    McpError::invalid_params("'task_id' required for validate_completion", None)
                })?;
                let subtask_id = p.subtask_id.ok_or_else(|| {
                    McpError::invalid_params("'subtask_id' required for validate_completion", None)
                })?;
                let min_quality = p.quality_threshold;

                let gate = {
                    let dec = self.decomposer.read().await;
                    dec.validate_completion_gate(&task_id, &subtask_id, min_quality)
                        .map_err(|e| McpError::internal_error(e.to_string(), None))?
                };

                serde_json::json!({
                    "action": "validate_completion",
                    "task_id": task_id,
                    "subtask_id": subtask_id,
                    "ready_to_complete": gate.ready_to_complete,
                    "blocking_reasons": gate.blocking_reasons,
                    "pending_deps": gate.pending_deps,
                    "min_quality_threshold": min_quality,
                })
            }
            "finalize" => {
                let task_id = p.task_id.ok_or_else(|| {
                    McpError::invalid_params("'task_id' required for finalize", None)
                })?;
                let min_quality = p.quality_threshold;

                let report = {
                    let dec = self.decomposer.read().await;
                    dec.finalize_task(&task_id)
                        .map_err(|e| McpError::internal_error(e.to_string(), None))?
                };

                // Archive (delete from active pool) and inject RL reward only when ready
                if report.ready {
                    {
                        let mut dec = self.decomposer.write().await;
                        dec.delete_task(&task_id);
                    }
                    if let Err(e) = self
                        .checkpoint_manager
                        .lock()
                        .await
                        .checkpoint(&*self.decomposer.read().await)
                    {
                        tracing::debug!("checkpoint after finalize failed: {}", e);
                    }
                    let reward_ctx = format!(
                        "task_finalized:{}:completed={}/{}:pct={:.0}",
                        task_id, report.completed, report.total, report.completion_pct
                    );
                    tokio::spawn(async move {
                        let _ = tokio::process::Command::new("touring")
                            .args(["learning", "reward", "orchestrate", "1.0", &reward_ctx])
                            .stdin(std::process::Stdio::null())
                            .stdout(std::process::Stdio::null())
                            .stderr(std::process::Stdio::null())
                            .spawn();
                    });
                }

                serde_json::json!({
                    "action": "finalize",
                    "task_id": task_id,
                    "ready": report.ready,
                    "archived": report.ready,
                    "completion_pct": report.completion_pct,
                    "total_subtasks": report.total,
                    "completed": report.completed,
                    "failed": report.failed,
                    "skipped": report.skipped,
                    "cancelled": report.cancelled,
                    "pending": report.pending,
                    "in_progress": report.in_progress,
                    "blocking_reasons": report.blocking,
                    "min_quality_threshold": min_quality,
                    "rl_reward_injected": report.ready,
                })
            }
            // Wave C-D4: drain cascade queue and create subtasks from proposals
            "drain_cascades" => {
                let target_task_id = p.task_id.clone();

                // Query daemon for cascade queue drain via Unix socket.
                // W12.5 unification: the old inline copy read `$UID` (a shell
                // variable that is NOT exported to processes) with a "1000"
                // string fallback — delegate to the real resolver.
                let socket_path = daemon_socket_path();

                let request = serde_json::json!({
                    "hook": "cli-cascade-queue-drain",
                    "payload": {},
                    "project_root": std::env::current_dir()
                        .map(|p| p.to_string_lossy().to_string())
                        .unwrap_or_default(),
                });

                let drain_output = {
                    let mut stream = UnixStream::connect(&socket_path).map_err(|e| {
                        McpError::internal_error(format!("daemon connect: {e}"), None)
                    })?;
                    stream
                        .set_read_timeout(Some(std::time::Duration::from_secs(30)))
                        .ok();
                    stream
                        .set_write_timeout(Some(std::time::Duration::from_secs(10)))
                        .ok();
                    serde_json::to_writer(&stream, &request)
                        .map_err(|e| McpError::internal_error(format!("write req: {e}"), None))?;
                    stream.write_all(b"\n").map_err(|e| {
                        McpError::internal_error(format!("write newline: {e}"), None)
                    })?;
                    stream
                        .flush()
                        .map_err(|e| McpError::internal_error(format!("flush: {e}"), None))?;
                    let mut resp = Vec::new();
                    stream
                        .read_to_end(&mut resp)
                        .map_err(|e| McpError::internal_error(format!("read resp: {e}"), None))?;
                    String::from_utf8(resp)
                        .map_err(|e| McpError::internal_error(format!("utf8: {e}"), None))?
                };

                let drain_result: serde_json::Value = serde_json::from_str(&drain_output)
                    .map_err(|e| McpError::internal_error(format!("parse: {e}"), None))?;

                let drained_count = drain_result
                    .get("drained_count")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as usize;
                let stale_evicted = drain_result
                    .get("stale_evicted")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as usize;
                let proposals = drain_result
                    .get("proposals")
                    .and_then(|v| v.as_array())
                    .cloned()
                    .unwrap_or_default();

                let mut subtasks_added = 0usize;

                // If a target task was provided, add subtasks for each drained proposal
                if let Some(ref task_id) = target_task_id {
                    for proposal_group in proposals {
                        let path = proposal_group
                            .get("path")
                            .and_then(|v| v.as_str())
                            .unwrap_or("unknown");
                        let items = proposal_group
                            .get("proposals")
                            .and_then(|v| v.as_array())
                            .cloned()
                            .unwrap_or_default();

                        for item in items {
                            let symbol = item
                                .get("symbol")
                                .and_then(|v| v.as_str())
                                .unwrap_or("unknown");
                            let reason = item.get("reason").and_then(|v| v.as_str()).unwrap_or("");
                            let callers = item.get("callers").and_then(|v| v.as_u64()).unwrap_or(0);

                            let description = format!(
                                "Update caller '{}': {} (affects {} caller(s))",
                                symbol, reason, callers
                            );

                            let subtask_id = {
                                let mut dec = self.decomposer.write().await;
                                dec.add_subtask(task_id, &description, vec![], 0)
                                    .map_err(|e| McpError::internal_error(e, None))?
                            };

                            tracing::info!(
                                "cascade subtask: {} -> {} (cascade file: {})",
                                subtask_id,
                                description,
                                path
                            );
                            subtasks_added += 1;
                        }
                    }

                    if let Err(e) = self
                        .checkpoint_manager
                        .lock()
                        .await
                        .checkpoint(&*self.decomposer.read().await)
                    {
                        tracing::debug!("checkpoint after drain_cascades failed: {}", e);
                    }
                }

                serde_json::json!({
                    "action": "drain_cascades",
                    "drained_count": drained_count,
                    "subtasks_added": subtasks_added,
                    "stale_evicted": stale_evicted,
                    "target_task_id": target_task_id,
                })
            }
            _ => {
                return Err(McpError::invalid_params(
                    format!(
                        "Unknown decompose action: '{}'. Valid: create, delete, add_subtask, update_status, get_plan, list_tasks, get_ready_subtasks, validate_order, get_parallel_groups, check_deadlines, infer_deps, validate_completion, finalize, drain_cascades",
                        p.action
                    ),
                    None,
                ));
            }
        };
        self.graph_svc.inject(&mut output, &gctx);

        // CognitiveNexus: inject predictive context
        let cctx = self
            .nexus
            .resolve("touring_decompose", &cognitive_hint)
            .await;
        if !cctx.is_empty() {
            match serde_json::to_value(&cctx) {
                #[allow(clippy::indexing_slicing)]
                // SAFETY: serde_json::Value string indexing never panics
                Ok(v) => {
                    output["cognitive_ctx"] = v;
                }
                Err(e) => {
                    tracing::warn!("cognitive_ctx serialize failed: {e}");
                }
            }
        }

        let dl = p.detail_level.unwrap_or_default();
        params::apply_detail_level(&mut output, dl);
        crate::tools::suggestions::append_to_response(&mut output, "touring_decompose", 2);
        let text = serde_json::to_string_pretty(&output)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    // ── touring_session — Session lifecycle with auto-assessment ─────────

    /// Session lifecycle: start, checkpoint, assess, end, list, get
    #[tool(
        name = "touring_session",
        description = "Session lifecycle: start, checkpoint, assess, end, list, get"
    )]
    async fn session(&self, params: Parameters<SessionParams>) -> Result<CallToolResult, McpError> {
        let p = params.0;
        let gctx = self.graph_svc.resolve_ctx(None).await;

        let mut output = match p.action.as_str() {
            "start" => {
                let task_type = p.task_type.unwrap_or_else(|| "general".to_string());
                let objective = p.objective.unwrap_or_default();

                let mut mgr = self.session_manager.lock().await;
                let session_id = mgr.start_session(&task_type, &objective);

                serde_json::json!({
                    "action": "start",
                    "session_id": session_id,
                    "task_type": task_type,
                    "objective": objective,
                    "status": "active",
                })
            }
            "checkpoint" => {
                let session_id = p.session_id.ok_or_else(|| {
                    McpError::invalid_params("'session_id' required for checkpoint", None)
                })?;
                let notes = p.notes.unwrap_or_default();
                let metrics = p.metrics.unwrap_or_default();

                let mut mgr = self.session_manager.lock().await;
                let count = mgr
                    .checkpoint(&session_id, &notes, metrics.clone())
                    .map_err(|e| McpError::internal_error(e.to_string(), None))?;

                serde_json::json!({
                    "action": "checkpoint",
                    "session_id": session_id,
                    "checkpoint_count": count,
                    "notes": notes,
                    "metrics": metrics,
                    "status": "recorded",
                })
            }
            "assess" => {
                let session_id = p.session_id.ok_or_else(|| {
                    McpError::invalid_params("'session_id' required for assess", None)
                })?;

                let mgr = self.session_manager.lock().await;
                let session = mgr.get_session(&session_id).ok_or_else(|| {
                    McpError::internal_error(format!("Session not found: {}", session_id), None)
                })?;

                // Run Wilson ranker assessment on session metrics
                let ranker = self.ranker.lock().await;
                let ranked_items = ranker.top_k(10);
                let ranked_json: Vec<serde_json::Value> = ranked_items
                    .iter()
                    .map(|item| {
                        serde_json::json!({
                            "id": item.id,
                            "wilson_lower": item.score.lower,
                            "wilson_upper": item.score.upper,
                            "raw_rate": item.raw_rate,
                            "trials": item.trials,
                        })
                    })
                    .collect();

                // Run drift detection on session metrics
                let detector = self.drift_detector.lock().await;
                let drifts = detector.detect_all();
                let drift_json: Vec<serde_json::Value> = drifts
                    .iter()
                    .map(|(metric, result)| {
                        serde_json::json!({
                            "metric": metric,
                            "drift_detected": result.drift_detected,
                            "magnitude": result.magnitude,
                            "direction": result.direction,
                            "confidence": result.confidence,
                        })
                    })
                    .collect();

                serde_json::json!({
                    "action": "assess",
                    "session_id": session_id,
                    "session_metrics": session.metrics,
                    "checkpoint_count": session.checkpoints.len(),
                    "ranker_top_items": ranked_json,
                    "drift_analysis": drift_json,
                })
            }
            "end" => {
                let session_id = p.session_id.ok_or_else(|| {
                    McpError::invalid_params("'session_id' required for end", None)
                })?;
                let status_str = p.status.unwrap_or_else(|| "completed".to_string());
                let status = match status_str.as_str() {
                    "completed" => crate::session::SessionStatus::Completed,
                    "abandoned" => crate::session::SessionStatus::Abandoned,
                    _ => {
                        return Err(McpError::invalid_params(
                            format!(
                                "Invalid status: '{}'. Valid: completed, abandoned",
                                status_str
                            ),
                            None,
                        ));
                    }
                };

                let mut mgr = self.session_manager.lock().await;
                let session = mgr
                    .end_session(&session_id, status)
                    .map_err(|e| McpError::internal_error(e.to_string(), None))?;

                // Store session summary in memory if available
                if let Some(ref mem_arc) = self.memory {
                    let summary = format!(
                        "Session {}: {} - {} ({})",
                        session.id, session.task_type, session.objective, session.status
                    );
                    let entry =
                        MemoryEntry::new(format!("session_{}", session.id), "reference", &summary)
                            .with_entry_type("session_summary");

                    let mem = mem_arc.lock().await;
                    if let Err(e) = mem.store(entry) {
                        warn!("Failed to store session summary in memory: {}", e);
                    }
                }

                serde_json::json!({
                    "action": "end",
                    "session_id": session.id,
                    "status": session.status.to_string(),
                    "started_at": session.started_at.to_rfc3339(),
                    "ended_at": session.ended_at.map(|t| t.to_rfc3339()),
                    "checkpoint_count": session.checkpoints.len(),
                    "metrics": session.metrics,
                })
            }
            "list" => {
                let limit = p.limit.unwrap_or(10) as usize;

                let mgr = self.session_manager.lock().await;
                let sessions = mgr.list_sessions(limit);

                let sessions_json: Vec<serde_json::Value> = sessions
                    .iter()
                    .map(|s| {
                        serde_json::json!({
                            "id": s.id,
                            "task_type": s.task_type,
                            "objective": s.objective,
                            "status": s.status.to_string(),
                            "started_at": s.started_at.to_rfc3339(),
                            "ended_at": s.ended_at.map(|t| t.to_rfc3339()),
                            "checkpoint_count": s.checkpoints.len(),
                        })
                    })
                    .collect();

                serde_json::json!({
                    "action": "list",
                    "session_count": sessions.len(),
                    "sessions": sessions_json,
                })
            }
            "get" => {
                let session_id = p.session_id.ok_or_else(|| {
                    McpError::invalid_params("'session_id' required for get", None)
                })?;

                let mgr = self.session_manager.lock().await;
                let session = mgr.get_session(&session_id).ok_or_else(|| {
                    McpError::internal_error(format!("Session not found: {}", session_id), None)
                })?;

                let checkpoints_json: Vec<serde_json::Value> = session
                    .checkpoints
                    .iter()
                    .map(|cp| {
                        serde_json::json!({
                            "timestamp": cp.timestamp.to_rfc3339(),
                            "notes": cp.notes,
                            "metrics": cp.metrics,
                        })
                    })
                    .collect();

                serde_json::json!({
                    "action": "get",
                    "session_id": session.id,
                    "task_type": session.task_type,
                    "objective": session.objective,
                    "status": session.status.to_string(),
                    "started_at": session.started_at.to_rfc3339(),
                    "ended_at": session.ended_at.map(|t| t.to_rfc3339()),
                    "metrics": session.metrics,
                    "checkpoints": checkpoints_json,
                })
            }
            _ => {
                return Err(McpError::invalid_params(
                    format!(
                        "Unknown session action: '{}'. Valid: start, checkpoint, assess, end, list, get",
                        p.action
                    ),
                    None,
                ));
            }
        };
        self.graph_svc.inject(&mut output, &gctx);

        let dl = p.detail_level.unwrap_or_default();
        params::apply_detail_level(&mut output, dl);
        crate::tools::suggestions::append_to_response(&mut output, "touring_session", 2);
        let text = serde_json::to_string_pretty(&output)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    // ── touring_evolve — Pattern extraction + self-improvement ───────────

    /// Evolution engine: extract patterns, update Q-table, consolidate memory, drift report, recommend
    #[tool(
        name = "touring_evolve",
        description = "Self-improvement engine for Touring's RL models: extract_patterns, auto_learn, update_qtable, consolidate_memory, drift_report, recommend (see action enum + per-action params). Call after sessions to compound learning over time."
    )]
    async fn evolve(&self, params: Parameters<EvolveParams>) -> Result<CallToolResult, McpError> {
        let p = params.0;
        let gctx = self.graph_svc.resolve_ctx(None).await;

        let mut output = match p.action.as_str() {
            "extract_patterns" => {
                let session_id = p.session_id.ok_or_else(|| {
                    McpError::invalid_params("'session_id' required for extract_patterns", None)
                })?;

                // Serialize session data inside lock scope, then release before acquiring graph index
                let (session_metrics_json, checkpoint_count) = {
                    let mgr = self.session_manager.lock().await;
                    let session = mgr.get_session(&session_id).ok_or_else(|| {
                        McpError::internal_error(format!("Session not found: {}", session_id), None)
                    })?;
                    let metrics =
                        serde_json::to_value(&session.metrics).unwrap_or(serde_json::Value::Null);
                    let count = session.checkpoints.len();
                    (metrics, count)
                };

                let clusters_json: Vec<serde_json::Value> = {
                    let cl = self.clusterer.lock().await;
                    cl.get_clusters()
                        .iter()
                        .map(|c| {
                            serde_json::json!({
                                "id": c.id,
                                "skills": c.skills,
                                "cohesion": c.cohesion,
                            })
                        })
                        .collect()
                };

                // Graph hotspots: files with highest in-degree (most imported = widest blast radius).
                // Surfaces the "load-bearing" files that disproportionately affect the codebase.
                let graph_hotspots: Vec<serde_json::Value> = {
                    let idx = self.graph_svc.index().lock().await;
                    let mut by_indegree: Vec<(String, usize)> = idx
                        .reverse_deps
                        .iter()
                        .map(|(f, importers)| (f.clone(), importers.len()))
                        .collect();
                    by_indegree.sort_unstable_by_key(|b| std::cmp::Reverse(b.1));
                    by_indegree
                        .into_iter()
                        .take(5)
                        .map(
                            |(file, count)| serde_json::json!({"file": file, "depended_by": count}),
                        )
                        .collect()
                };

                serde_json::json!({
                    "action": "extract_patterns",
                    "session_id": session_id,
                    "session_metrics": session_metrics_json,
                    "checkpoint_count": checkpoint_count,
                    "skill_clusters": clusters_json,
                    "graph_hotspots": graph_hotspots,
                })
            }
            "update_qtable" => {
                let state = p.state.unwrap_or(0);
                let action_id = p.action_id.unwrap_or(0);
                let reward = p.reward.unwrap_or(0.0);

                let mut qt = self.qtable.lock().await;
                let td_error = qt.update(state, action_id, reward, state + 1, None, true);

                // Also record in ranker for Wilson scoring
                let mut ranker = self.ranker.lock().await;
                ranker.record(&format!("s{}a{}", state, action_id), reward > 0.0);

                serde_json::json!({
                    "action": "update_qtable",
                    "state": state,
                    "action_id": action_id,
                    "reward": reward,
                    "td_error": td_error,
                    "q_value": qt.get_q(state, action_id),
                })
            }
            "auto_learn" => {
                // Feed recorded hook outcomes into the QTable via Bellman updates.
                // Reads touring_hook_events (reward already computed by cortex hooks),
                // maps event_type→state and tool_name→action_id, then calls qt.update().
                let since_id = p.state.unwrap_or(0) as i64;
                let batch = p.top_k.unwrap_or(200) as usize;

                let db_path = self.config.rlm_db_path.clone();
                let persistence = LearningPersistence::new(&db_path);

                let events = persistence.load_hook_events_since(since_id, batch);

                let mut qt = self.qtable.lock().await;
                let mut ranker = self.ranker.lock().await;
                let mut processed = 0usize;
                let mut last_id: i64 = since_id;

                for (id, event_type, tool_name, reward) in &events {
                    let state = event_type_to_state(event_type);
                    let action_id = tool_name_to_action(tool_name);
                    let next_state = state.saturating_add(1) % 9;

                    let _td_error = qt.update(state, action_id, *reward, next_state, None, false);
                    ranker.record(&format!("s{}a{}", state, action_id), *reward > 0.0);

                    processed += 1;
                    if *id > last_id {
                        last_id = *id;
                    }
                }

                let qtable_size = qt.len();

                // Persist immediately so next server startup loads the learned state.
                // Save QTable first, then Wilson (confidence scoring), then drop locks.
                let saved = persistence.save_qtable(&qt).unwrap_or(0);
                drop(qt);
                let _ = persistence.save_wilson(&ranker);
                drop(ranker);

                serde_json::json!({
                    "action": "auto_learn",
                    "events_processed": processed,
                    "last_event_id": last_id,
                    "qtable_size": qtable_size,
                    "qtable_saved": saved,
                    "next_since_id": last_id,
                })
            }
            "consolidate_memory" => {
                let key = p.key.ok_or_else(|| {
                    McpError::invalid_params("'key' required for consolidate_memory", None)
                })?;
                let current_tier = p.current_tier.ok_or_else(|| {
                    McpError::invalid_params("'current_tier' required for consolidate_memory", None)
                })?;
                let new_tier = p.new_tier.ok_or_else(|| {
                    McpError::invalid_params("'new_tier' required for consolidate_memory", None)
                })?;

                let memory = self.memory.as_ref().ok_or_else(|| {
                    McpError::internal_error("MemoryStore not initialized".to_string(), None)
                })?;

                let mem = memory.lock().await;
                let promoted = mem
                    .promote_tier(&key, &current_tier, &new_tier)
                    .map_err(|e| {
                        McpError::internal_error(format!("Promote failed: {}", e), None)
                    })?;

                serde_json::json!({
                    "action": "consolidate_memory",
                    "key": key,
                    "from_tier": current_tier,
                    "to_tier": new_tier,
                    "promoted": promoted,
                })
            }
            "drift_report" => {
                let detector = self.drift_detector.lock().await;

                if let Some(metric) = p.metric {
                    let result = detector.detect(&metric);
                    serde_json::json!({
                        "action": "drift_report",
                        "metric": metric,
                        "drift_detected": result.drift_detected,
                        "magnitude": result.magnitude,
                        "direction": result.direction,
                        "confidence": result.confidence,
                    })
                } else {
                    let all = detector.detect_all();
                    let drifts_json: Vec<serde_json::Value> = all
                        .iter()
                        .map(|(m, r)| {
                            serde_json::json!({
                                "metric": m,
                                "drift_detected": r.drift_detected,
                                "magnitude": r.magnitude,
                                "direction": r.direction,
                                "confidence": r.confidence,
                            })
                        })
                        .collect();

                    serde_json::json!({
                        "action": "drift_report",
                        "metric_count": all.len(),
                        "drifts": drifts_json,
                    })
                }
            }
            "recommend" => {
                let state = p.state.unwrap_or(0);
                let top_k = p.top_k.unwrap_or(5) as usize;

                let qt = self.qtable.lock().await;
                let best = qt.best_action(state);
                let q_values = qt.get_state_q_values(state);

                let ranker = self.ranker.lock().await;
                let top_items = ranker.top_k(top_k);
                let ranked_json: Vec<serde_json::Value> = top_items
                    .iter()
                    .map(|item| {
                        serde_json::json!({
                            "id": item.id,
                            "wilson_lower": item.score.lower,
                            "raw_rate": item.raw_rate,
                            "trials": item.trials,
                        })
                    })
                    .collect();

                serde_json::json!({
                    "action": "recommend",
                    "state": state,
                    "best_action": best,
                    "q_values": q_values.iter()
                        .map(|(a, q)| serde_json::json!({"action": a, "q_value": q}))
                        .collect::<Vec<_>>(),
                    "top_ranked": ranked_json,
                })
            }
            _ => {
                return Err(McpError::invalid_params(
                    format!(
                        "Unknown evolve action: '{}'. Valid: extract_patterns, update_qtable, auto_learn, consolidate_memory, drift_report, recommend",
                        p.action
                    ),
                    None,
                ));
            }
        };
        self.graph_svc.inject(&mut output, &gctx);

        let dl = p.detail_level.unwrap_or_default();
        params::apply_detail_level(&mut output, dl);
        crate::tools::suggestions::append_to_response(&mut output, "touring_evolve", 2);
        let text = serde_json::to_string_pretty(&output)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    // ── touring_suggest — Context-aware suggestions ──────────────────────

    /// Context-aware suggestions: next action, similar patterns, skill recommendation, code pattern analysis
    #[tool(
        name = "touring_suggest",
        description = "RL-backed suggestions via Q-table + LinUCB bandit: next_action, similar_patterns, skill_recommendation, code_pattern (see action enum + per-action params). Pass file_path to scale confidence by blast_radius. top_k default 5."
    )]
    async fn suggest(&self, params: Parameters<SuggestParams>) -> Result<CallToolResult, McpError> {
        let p = params.0;

        // Tier A: update focus + resolve graph context
        if let Some(ref fp) = p.file_path {
            self.graph_svc.update_focus(fp).await;
        }
        let gctx = self.graph_svc.resolve_ctx(p.file_path.as_deref()).await;

        // Save hint for CognitiveNexus before query is moved
        let cognitive_hint = p.query.clone().unwrap_or_else(|| p.action.clone());

        let mut output = match p.action.as_str() {
            "next_action" => {
                let state = p.state.unwrap_or(0);

                let qt = self.qtable.lock().await;
                let best = qt.best_action(state);
                let q_values = qt.get_state_q_values(state);
                // Serialize q_values before dropping qt (may be borrowed from qt internals)
                let q_values_json: Vec<serde_json::Value> = q_values
                    .iter()
                    .map(|(a, q)| serde_json::json!({"action": a, "q_value": q}))
                    .collect();

                let ranker = self.ranker.lock().await;
                let confidence = if let Some(action_id) = best {
                    let key = format!("s{}a{}", state, action_id);
                    ranker
                        .get_stats(&key)
                        .and_then(|(s, t)| {
                            touring_intelligence::rl::ranking::WilsonScore::calculate(s, t, 0.95)
                                .map(|ws| ws.lower)
                        })
                        .unwrap_or(0.0)
                } else {
                    0.0
                };
                let qtable_size = qt.len() as u64;
                drop(ranker);
                drop(qt);

                // Graph context: blast_radius of the focused file scales confidence.
                // Files with many dependents carry higher change risk → lower effective confidence.
                let (final_confidence, graph_ctx) = if let Some(ref fp) = p.file_path {
                    let idx = self.graph_svc.index().lock().await;
                    let r = idx.blast_radius(fp);
                    let impact_factor: f64 = if r.file_count > 20 {
                        0.7
                    } else if r.file_count > 5 {
                        0.85
                    } else {
                        1.0
                    };
                    let top_deps: Vec<&String> = r.affected_files.iter().take(5).collect();
                    let ctx = serde_json::json!({
                        "file": fp,
                        "affected_files": r.file_count,
                        "max_distance": r.max_distance,
                        "impact_factor": impact_factor,
                        "top_dependents": top_deps,
                    });
                    (confidence * impact_factor, Some(ctx))
                } else {
                    (confidence, None)
                };

                serde_json::json!({
                    "action": "next_action",
                    "state": state,
                    "best_action": best,
                    "confidence": final_confidence,
                    "qtable_size": qtable_size,
                    "table_size": qtable_size,  // alias: test compatibility
                    "q_values": q_values_json,
                    "graph_context": graph_ctx,
                })
            }
            "similar_patterns" => {
                let query = p.query.ok_or_else(|| {
                    McpError::invalid_params("'query' required for similar_patterns", None)
                })?;
                let top_k = p.top_k.unwrap_or(5) as usize;

                let memory = self.memory.as_ref().ok_or_else(|| {
                    McpError::internal_error("MemoryStore not initialized".to_string(), None)
                })?;

                let mut mq = MemoryQuery::new(&query).with_top_k(top_k);
                if let Some(t) = p.tier {
                    mq = mq.with_tier(&t);
                }

                let mem = memory.lock().await;
                let result = mem
                    .query(mq)
                    .map_err(|e| McpError::internal_error(format!("Query failed: {}", e), None))?;

                let matches_json: Vec<serde_json::Value> = result
                    .rlm_matches
                    .iter()
                    .map(|m| {
                        serde_json::json!({
                            "key": m.key,
                            "tier": m.tier,
                            "value": m.value,
                            "score": m.score,
                        })
                    })
                    .collect();

                // Graph expansion: search memory entries for neighbor files
                // NOTE(P5.2): wire search_by_file_paths when RlmMemory gains it

                drop(mem);

                serde_json::json!({
                    "action": "similar_patterns",
                    "query": query,
                    "match_count": matches_json.len(),
                    "matches": matches_json,
                })
            }
            "skill_recommendation" => {
                let skill_id = p.skill_id.ok_or_else(|| {
                    McpError::invalid_params("'skill_id' required for skill_recommendation", None)
                })?;
                let top_k = p.top_k.unwrap_or(5) as usize;

                let cl = self.clusterer.lock().await;
                let similar = cl.find_similar(&skill_id, top_k);

                let similar_json: Vec<serde_json::Value> = similar
                    .iter()
                    .map(|(id, score)| {
                        serde_json::json!({
                            "skill_id": id,
                            "similarity": score,
                        })
                    })
                    .collect();

                serde_json::json!({
                    "action": "skill_recommendation",
                    "skill_id": skill_id,
                    "recommendations": similar_json,
                })
            }
            "code_pattern" => {
                let content = p.content.ok_or_else(|| {
                    McpError::invalid_params("'content' required for code_pattern", None)
                })?;
                let lang_str = p.language.unwrap_or_else(|| "python".to_string());

                // W2.4 ULTRATHINK reversal 2026-05-14 — re-wired to helper API per
                // REGRA #0 (sempre potencializar). Constructs `AstOverviewArgs` via
                // the builder, requests JSON output, extracts the text payload.
                let args = crate::tools::ast_tools::AstOverviewArgs {
                    content: Some(content.clone()),
                    file_path: None,
                    language: Some(lang_str.clone()),
                    format: Some("json".to_string()),
                    show_savings: Some(false),
                };
                let ast_result = crate::tools::ast_tools::touring_ast_overview(args)
                    .map_err(|e| McpError::internal_error(e.to_string(), None))?;
                let ast_text = ast_result
                    .content
                    .first()
                    .and_then(|c| match &c.raw {
                        rmcp::model::RawContent::Text(t) => Some(t.text.clone()),
                        _ => None,
                    })
                    .unwrap_or_else(|| "{}".to_string());

                // Search memory for related patterns
                let memory_patterns = if let Some(ref mem_arc) = self.memory {
                    let query =
                        MemoryQuery::new(format!("{} code pattern", lang_str)).with_top_k(3);
                    let mem = mem_arc.lock().await;
                    mem.query(query)
                        .map(|r| {
                            r.rlm_matches.iter().map(|m| {
                            serde_json::json!({"key": m.key, "value": m.value, "score": m.score})
                        }).collect::<Vec<_>>()
                        })
                        .unwrap_or_default()
                } else {
                    Vec::new()
                };

                serde_json::json!({
                    "action": "code_pattern",
                    "language": lang_str,
                    "ast_overview": serde_json::from_str::<serde_json::Value>(&ast_text).unwrap_or(serde_json::Value::Null),
                    "related_memory_patterns": memory_patterns,
                })
            }
            _ => {
                return Err(McpError::invalid_params(
                    format!(
                        "Unknown suggest action: '{}'. Valid: next_action, similar_patterns, skill_recommendation, code_pattern",
                        p.action
                    ),
                    None,
                ));
            }
        };

        self.graph_svc.inject(&mut output, &gctx);

        // CognitiveNexus: inject predictive context
        let cctx = self.nexus.resolve("touring_suggest", &cognitive_hint).await;
        if !cctx.is_empty() {
            match serde_json::to_value(&cctx) {
                #[allow(clippy::indexing_slicing)]
                // SAFETY: serde_json::Value string indexing never panics
                Ok(v) => {
                    output["cognitive_ctx"] = v;
                }
                Err(e) => {
                    tracing::warn!("cognitive_ctx serialize failed: {e}");
                }
            }
        }

        let dl = p.detail_level.unwrap_or_default();
        params::apply_detail_level(&mut output, dl);
        crate::tools::suggestions::append_to_response(&mut output, "touring_suggest", 2);
        let text = serde_json::to_string_pretty(&output)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    // ── touring_refactor — Safe refactoring with AST validation ──────────

    /// Safe refactoring: analyze symbol impact, rename with AST, validate syntax, preview changes
    #[tool(
        name = "touring_refactor",
        description = "Safe refactoring: analyze symbol impact, rename with AST, validate syntax, preview changes"
    )]
    async fn refactor(
        &self,
        params: Parameters<RefactorParams>,
    ) -> Result<CallToolResult, McpError> {
        let p = params.0;

        // Tier A: update focus + resolve graph context
        if let Some(ref path) = p.file_path {
            self.graph_svc.update_focus(path).await;
        }
        let gctx = self.graph_svc.resolve_ctx(p.file_path.as_deref()).await;

        let content = &p.content;
        let lang_str = p.language.unwrap_or_else(|| "python".to_string());

        let mut output = match p.action.as_str() {
            "analyze" => {
                let symbol_name = p.symbol_name.ok_or_else(|| {
                    McpError::invalid_params("'symbol_name' required for analyze", None)
                })?;

                // W2.4 ULTRATHINK reversal 2026-05-14 — re-wired to helper API per
                // REGRA #0. Same pattern as the code_pattern path.
                let args = crate::tools::ast_tools::AstOverviewArgs {
                    content: Some(content.clone()),
                    file_path: None,
                    language: Some(lang_str.clone()),
                    format: Some("json".to_string()),
                    show_savings: Some(false),
                };
                let ast_result = crate::tools::ast_tools::touring_ast_overview(args)
                    .map_err(|e| McpError::internal_error(e.to_string(), None))?;
                let ast_text = ast_result
                    .content
                    .first()
                    .and_then(|c| match &c.raw {
                        rmcp::model::RawContent::Text(t) => Some(t.text.clone()),
                        _ => None,
                    })
                    .unwrap_or_else(|| "{}".to_string());

                // Check blast radius if file is in the graph index
                let blast = if let Some(ref fp) = p.file_path {
                    let idx = self.graph_svc.index().lock().await;
                    let r = idx.blast_radius(fp);
                    Some(serde_json::json!({
                        "affected_files": r.affected_files,
                        "max_distance": r.max_distance,
                        "file_count": r.file_count,
                    }))
                } else {
                    None
                };

                // Count occurrences of symbol in content
                let occurrence_count = content.matches(&symbol_name).count();

                serde_json::json!({
                    "action": "analyze",
                    "symbol_name": symbol_name,
                    "language": lang_str,
                    "occurrences": occurrence_count,
                    "ast_overview": serde_json::from_str::<serde_json::Value>(&ast_text).unwrap_or(serde_json::Value::Null),
                    "blast_radius": blast,
                })
            }
            "rename" => {
                let symbol_name = p.symbol_name.ok_or_else(|| {
                    McpError::invalid_params("'symbol_name' required for rename", None)
                })?;
                let new_name = p.new_name.ok_or_else(|| {
                    McpError::invalid_params("'new_name' required for rename", None)
                })?;

                // Simple rename: replace all occurrences of the symbol name
                let renamed = content.replace(&symbol_name, &new_name);
                let replacements = content.matches(&symbol_name).count();

                // Validate the result
                let valid = lang_str
                    .parse::<touring_code::ast::Lang>()
                    .ok()
                    .and_then(|l| touring_code::ast::surgery::validate_syntax(&renamed, l).ok())
                    .unwrap_or(false);

                serde_json::json!({
                    "action": "rename",
                    "symbol_name": symbol_name,
                    "new_name": new_name,
                    "replacements": replacements,
                    "valid_syntax": valid,
                    "renamed_content": renamed,
                })
            }
            "validate" => {
                let valid = lang_str
                    .parse::<touring_code::ast::Lang>()
                    .ok()
                    .and_then(|l| touring_code::ast::surgery::validate_syntax(content, l).ok())
                    .unwrap_or(false);

                serde_json::json!({
                    "action": "validate",
                    "language": lang_str,
                    "valid": valid,
                    "content_length": content.len(),
                })
            }
            "preview" => {
                let symbol_name = p.symbol_name.ok_or_else(|| {
                    McpError::invalid_params("'symbol_name' required for preview", None)
                })?;
                let new_name = p.new_name.clone();

                let occurrence_count = content.matches(&symbol_name).count();

                // Show what would change
                let changes: Vec<serde_json::Value> = content
                    .lines()
                    .enumerate()
                    .filter(|(_, line)| line.contains(&symbol_name))
                    .map(|(i, line)| {
                        let after = if let Some(ref nn) = new_name {
                            line.replace(&symbol_name, nn)
                        } else {
                            line.to_string()
                        };
                        serde_json::json!({
                            "line_number": i + 1,
                            "before": line,
                            "after": after,
                        })
                    })
                    .collect();

                serde_json::json!({
                    "action": "preview",
                    "symbol_name": symbol_name,
                    "new_name": new_name,
                    "total_occurrences": occurrence_count,
                    "affected_lines": changes.len(),
                    "changes": changes,
                })
            }
            _ => {
                return Err(McpError::invalid_params(
                    format!(
                        "Unknown refactor action: '{}'. Valid: analyze, rename, validate, preview",
                        p.action
                    ),
                    None,
                ));
            }
        };

        self.graph_svc.inject(&mut output, &gctx);

        let dl = p.detail_level.unwrap_or_default();
        params::apply_detail_level(&mut output, dl);
        crate::tools::suggestions::append_to_response(&mut output, "touring_refactor", 2);
        let text = serde_json::to_string_pretty(&output)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }
}
