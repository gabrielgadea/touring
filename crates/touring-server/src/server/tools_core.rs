use super::*;
use crate::daemon_client::daemon_query;

/// Resolve the canonical on-disk path for the entity-registry SQLite DB.
///
/// Precedence: `TOURING_IDENTITY_DIR` → `TOURING_DATA_DIR` →
/// `/tmp/touring-identity`. The parent directory is created on demand so
/// callers can immediately open/connect. Shared with peer modules
/// (`tools_analysis::current_uid`, identity inspectors) — keeping a single
/// resolution helper avoids per-call divergence.
pub(crate) fn default_entity_db_path() -> anyhow::Result<std::path::PathBuf> {
    let base = std::env::var("TOURING_IDENTITY_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| {
            std::env::var("TOURING_DATA_DIR")
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|_| std::path::PathBuf::from("/tmp/touring-identity"))
        });
    std::fs::create_dir_all(&base)?;
    Ok(base.join("registry.db"))
}

#[tool_router(router = router_core, vis = "pub(crate)")]
impl TouringServer {
    // ── AST Tools ────────────────────────────────────────────────────────

    /// Extract symbols from source code with TOON format output
    #[tool(
        annotations(read_only_hint = true, title = "File structure overview"),
        name = "touring_ast_overview",
        description = "List a file's symbols, structure and imports (TOON output). Use to grasp a file's shape without reading it whole."
    )]
    async fn ast_overview(
        &self,
        params: Parameters<AstOverviewParams>,
    ) -> Result<CallToolResult, McpError> {
        let p = params.0;
        let dl = p.detail_level.unwrap_or_default();

        // Tier A: update focus + resolve graph context
        if let Some(ref path) = p.file_path {
            self.graph_svc.update_focus(path).await;
        }
        let gctx = self.graph_svc.resolve_ctx(p.file_path.as_deref()).await;

        // W2.4 ULTRATHINK reversal 2026-05-14 — restored the indirection through
        // `touring_ast_overview` per REGRA #0 (sempre potencializar — wire orphan
        // symbols rather than delete them). The helper is the public library API
        // surface for AST extraction; this method is one of several consumers.
        let args = crate::tools::ast_tools::AstOverviewArgs {
            content: p.content,
            file_path: p.file_path,
            language: p.language,
            format: p.format,
            show_savings: p.show_savings,
        };
        let result = crate::tools::ast_tools::touring_ast_overview(args)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        // Inject graph_ctx into the JSON output
        // TOON format is plain text, not valid JSON - handle both cases gracefully
        let text = result
            .content
            .first()
            .and_then(|c| match &c.raw {
                rmcp::model::RawContent::Text(t) => Some(t.text.clone()),
                _ => None,
            })
            .unwrap_or_default();
        // Parse JSON if valid; otherwise wrap plain text (TOON format) as JSON
        let mut output: serde_json::Value = match serde_json::from_str::<serde_json::Value>(&text) {
            Ok(v) => v,
            Err(_) => serde_json::json!({"format": "toon", "raw": text}),
        };
        self.graph_svc.inject(&mut output, &gctx);

        // Apply detail_level truncation + append suggestions
        params::apply_detail_level(&mut output, dl);
        crate::tools::suggestions::append_to_response(&mut output, "touring_ast_overview", 2);

        let injected = serde_json::to_string_pretty(&output)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(injected)]))
    }

    /// Find symbols by name across project files
    #[tool(
        annotations(read_only_hint = true, title = "Find symbol definition"),
        name = "touring_ast_find",
        description = "Find a symbol's definition, signature and module path by name across the project."
    )]
    async fn ast_find(
        &self,
        params: Parameters<AstFindParams>,
    ) -> Result<CallToolResult, McpError> {
        let p = params.0;
        let dl = p.detail_level.unwrap_or_default();

        // Tier A: update focus + resolve graph context
        let focus_path = p.file_path.clone().or_else(|| p.path.clone());
        if let Some(ref path) = focus_path {
            self.graph_svc.update_focus(path).await;
        }
        let gctx = self.graph_svc.resolve_ctx(focus_path.as_deref()).await;

        // Save hint for CognitiveNexus before symbol_name is moved
        let cognitive_hint = p.symbol_name.clone();

        // Support both structured {files: [...]} and flat {file_path, content, language}
        let files = if let Some(files) = p.files {
            files
                .into_iter()
                .map(|f| crate::tools::ast_tools::FileContent {
                    path: f.path,
                    content: f.content,
                    language: f.language,
                })
                .collect()
        } else {
            let path = p
                .file_path
                .or(p.path)
                .unwrap_or_else(|| "unknown.py".to_string());
            let content = p.content.or(p.source).unwrap_or_default();
            let language = p.language.unwrap_or_else(|| "python".to_string());
            vec![crate::tools::ast_tools::FileContent {
                path,
                content,
                language,
            }]
        };

        // Detect if files have real content (non-empty) for inline parsing
        let has_real_content = files.iter().any(|f| !f.content.is_empty());

        let result = if has_real_content {
            // W2.4 ULTRATHINK reversal 2026-05-14 — restored helper call per REGRA #0.
            // `touring_ast_find` is the library API for symbol lookup; this method
            // is one of several consumers (cli handlers, tests, etc.).
            let args = crate::tools::ast_tools::AstFindArgs {
                symbol_name: p.symbol_name,
                files,
                definitions_only: p.definitions_only,
            };
            crate::tools::ast_tools::touring_ast_find(args)
                .map_err(|e| McpError::internal_error(e.to_string(), None))?
        } else {
            // DB fallback: query persisted SymbolStore when no file content provided
            let symbol_name = p.symbol_name;
            let definitions_only = p.definitions_only;
            let db_results = if let Some(ref store_arc) = self.symbol_store {
                let store = store_arc.lock().await;
                store.find_symbol(&symbol_name).unwrap_or_default()
            } else {
                Vec::new()
            };

            // Filter to definitions only if requested
            let filtered: Vec<_> = if definitions_only {
                db_results
                    .into_iter()
                    .filter(|loc| loc.is_definition)
                    .collect()
            } else {
                db_results
            };

            // Format output matching the standard ast_find output format
            let mut output = format!("# Symbol: {}\n\n", symbol_name);
            if filtered.is_empty() {
                output.push_str("No definitions found.\n");
            } else {
                output.push_str(&format!(
                    "Found {} location(s) (from persisted DB):\n\n",
                    filtered.len()
                ));
                for loc in &filtered {
                    output.push_str(&format!(
                        "## {}:{}:{}\n",
                        loc.file_path, loc.line, loc.column
                    ));
                    output.push_str(&format!("- **File**: {}\n", loc.file_path));
                    output.push_str(&format!("- **Line**: {}\n", loc.line));
                    output.push_str(&format!(
                        "- **Type**: {}\n\n",
                        if loc.is_definition {
                            "definition"
                        } else {
                            "reference"
                        }
                    ));
                }
            }
            output.push_str(
                "\n---\nSource: persisted SymbolStore (DB fallback, no file content provided)\n",
            );

            CallToolResult::success(vec![Content::text(output)])
        };

        // Inject graph_ctx into the JSON output
        // TOON format is plain text, not valid JSON - handle both cases gracefully
        let text = result
            .content
            .first()
            .and_then(|c| match &c.raw {
                rmcp::model::RawContent::Text(t) => Some(t.text.clone()),
                _ => None,
            })
            .unwrap_or_default();
        // Parse JSON if valid; otherwise wrap plain text (TOON format) as JSON
        let mut output: serde_json::Value = match serde_json::from_str::<serde_json::Value>(&text) {
            Ok(v) => v,
            Err(_) => serde_json::json!({"format": "toon", "raw": text}),
        };
        self.graph_svc.inject(&mut output, &gctx);

        // CognitiveNexus: inject predictive context
        let cctx = self
            .nexus
            .resolve("touring_ast_find", &cognitive_hint)
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

        params::apply_detail_level(&mut output, dl);
        crate::tools::suggestions::append_to_response(&mut output, "touring_ast_find", 2);

        let injected = serde_json::to_string_pretty(&output)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        // F2: Record symbol lookup metric for every ast_find call.
        AnalysisServerMetrics::global().inc_symbol_lookup();

        Ok(CallToolResult::success(vec![Content::text(injected)]))
    }

    // ── CILA Intent Classification (48-pattern RegexSet) ─────────────────

    /// CILA L0-L6 intent classification using 48-pattern RegexSet
    #[tool(
        annotations(read_only_hint = true, title = "Classify task complexity"),
        name = "touring_classify_intent",
        description = "Classify a task into a CILA complexity level (L0-L6) to choose the orchestration depth."
    )]
    async fn classify_intent(
        &self,
        params: Parameters<ClassifyIntentParams>,
    ) -> Result<CallToolResult, McpError> {
        let text = &params.0.text;
        let gctx = self.graph_svc.resolve_ctx(None).await;

        let result = self.classifier.classify(text);
        let techniques: Vec<serde_json::Value> = CognitiveTechnique::for_level(result.level)
            .iter()
            .map(|t| {
                serde_json::json!({
                    "technique": format!("{:?}", t),
                    "hint": t.hint(),
                })
            })
            .collect();

        let mut output = serde_json::json!({
            "level": result.level,
            "level_name": result.level_name,
            "routing_strategy": result.routing_strategy,
            "requires_pipeline": result.requires_pipeline,
            "requires_code_first": result.requires_code_first,
            "matched_pattern": result.matched_pattern,
            "techniques": techniques,
            "pattern_count": self.classifier.pattern_count(),
        });
        self.graph_svc.inject(&mut output, &gctx);
        let dl = params.0.detail_level.unwrap_or_default();
        params::apply_detail_level(&mut output, dl);
        crate::tools::suggestions::append_to_response(&mut output, "touring_classify_intent", 2);

        let text = serde_json::to_string_pretty(&output)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    // ── PII Scanner (5 patterns + 13 whitelist, RegexSet) ────────────────

    /// Detect Brazilian PII (CPF, CNPJ, SEI, email, phone) with whitelist filtering
    #[tool(
        name = "touring_scan_pii",
        description = "Detect Brazilian PII (CPF, CNPJ, SEI, email, phone) with whitelist filtering"
    )]
    async fn scan_pii(
        &self,
        params: Parameters<ScanPiiParams>,
    ) -> Result<CallToolResult, McpError> {
        let text = &params.0.text;
        let gctx = self.graph_svc.resolve_ctx(None).await;

        let findings = self.pii_scanner.scan_text(text);
        let has_pii = !findings.is_empty();
        let content_hash = self.pii_scanner.content_hash(text);

        let findings_json: Vec<serde_json::Value> = findings
            .iter()
            .map(|f| {
                serde_json::json!({
                    "pattern_name": f.pattern_name,
                    "line_number": f.line_number,
                    "matched_text": f.matched_text,
                    "column": f.column,
                    "severity": f.severity,
                })
            })
            .collect();

        let mut output = serde_json::json!({
            "has_pii": has_pii,
            "finding_count": findings.len(),
            "findings": findings_json,
            "content_hash": content_hash,
            "pattern_count": self.pii_scanner.pii_pattern_count(),
        });
        self.graph_svc.inject(&mut output, &gctx);
        let dl = params.0.detail_level.unwrap_or_default();
        params::apply_detail_level(&mut output, dl);
        crate::tools::suggestions::append_to_response(&mut output, "touring_scan_pii", 2);

        let text = serde_json::to_string_pretty(&output)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    // ── Memory Store (RLM + SemanticRecall via MemoryStore) ──────────────

    /// Store a memory entry in RLM + SemanticRecall
    #[tool(
        annotations(
            read_only_hint = false,
            idempotent_hint = false,
            title = "Store a lesson in memory"
        ),
        name = "touring_memory_store",
        description = "Store a lesson, decision or state snapshot in memory (recallable later via touring_memory_recall)."
    )]
    async fn memory_store(
        &self,
        params: Parameters<MemoryStoreParams>,
    ) -> Result<CallToolResult, McpError> {
        let p = params.0;
        let gctx = self.graph_svc.resolve_ctx(None).await;
        let memory = self.memory.as_ref().ok_or_else(|| {
            McpError::internal_error("MemoryStore not initialized".to_string(), None)
        })?;

        // Accept aliases: title->key, memory_type->tier, content->value
        let key = p
            .key
            .or(p.title)
            .ok_or_else(|| McpError::invalid_params("'key' (or 'title') is required", None))?;
        let tier = p
            .tier
            .or(p.memory_type)
            .unwrap_or_else(|| "working".to_string());
        let value = p
            .value
            .or(p.content)
            .ok_or_else(|| McpError::invalid_params("'value' (or 'content') is required", None))?;

        let mut entry = MemoryEntry::new(&key, &tier, &value);
        if let Some(ref et) = p.entry_type {
            entry = entry.with_entry_type(et);
        }

        // Auto-embed via GPU service before storing (graceful degradation)
        let mut embedded = false;
        if entry.embedding.is_none() {
            if let Some(ref client) = self.embedder {
                if let Some(emb) = client.embed_single(&value).await {
                    entry = entry.with_embedding(emb);
                    embedded = true;
                }
            }
        }

        let mem = memory.lock().await;
        mem.store(entry)
            .map_err(|e| McpError::internal_error(format!("Memory store failed: {}", e), None))?;

        let mut output = serde_json::json!({
            "status": "stored",
            "key": key,
            "tier": tier,
            "value_length": value.len(),
            "entry_type": p.entry_type,
            "embedded": embedded,
        });
        self.graph_svc.inject(&mut output, &gctx);

        let dl = p.detail_level.unwrap_or_default();
        params::apply_detail_level(&mut output, dl);
        crate::tools::suggestions::append_to_response(&mut output, "touring_memory_store", 2);
        let text = serde_json::to_string_pretty(&output)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    // ── Memory Recall (FTS5 + RLM search) ────────────────────────────────

    /// Search RLM + SemanticRecall (FTS5 + cosine similarity)
    #[tool(
        annotations(read_only_hint = true, title = "Recall lessons by query"),
        name = "touring_memory_recall",
        description = "Recall past lessons, decisions and outcomes by query (substitutes a commit log). Use to find how something was solved before."
    )]
    async fn memory_recall(
        &self,
        params: Parameters<MemoryRecallParams>,
    ) -> Result<CallToolResult, McpError> {
        let p = params.0;
        let dl = p.detail_level.unwrap_or_default();
        let memory = self.memory.as_ref().ok_or_else(|| {
            McpError::internal_error("MemoryStore not initialized".to_string(), None)
        })?;

        // Tier A: update focus + resolve graph context
        if let Some(ref fp) = p.file_path {
            self.graph_svc.update_focus(fp).await;
        }
        let gctx = self.graph_svc.resolve_ctx(p.file_path.as_deref()).await;

        let limit = p.limit.or(p.top_k).unwrap_or(10) as usize;

        let mut query = MemoryQuery::new(&p.query).with_top_k(limit);
        if let Some(t) = p.tier {
            query = query.with_tier(&t);
        }

        // Auto-embed query for hybrid search (cosine + FTS5)
        if let Some(ref client) = self.embedder {
            if let Some(emb) = client.embed_single(&p.query).await {
                query.embedding = Some(emb);
            }
        }

        let mem = memory.lock().await;
        let result = mem
            .query(query)
            .map_err(|e| McpError::internal_error(format!("Memory query failed: {}", e), None))?;

        let rlm_json: Vec<serde_json::Value> = result
            .rlm_matches
            .iter()
            .map(|m| {
                serde_json::json!({
                    "key": m.key,
                    "tier": m.tier,
                    "value": m.value,
                    "entry_type": m.entry_type,
                    "score": m.score,
                    "access_count": m.access_count,
                })
            })
            .collect();

        let semantic_json: Vec<serde_json::Value> = result
            .semantic_matches
            .iter()
            .map(|m| {
                serde_json::json!({
                    "id": m.id,
                    "content": m.content,
                    "metadata": m.metadata,
                    "score": m.score,
                })
            })
            .collect();

        // Graph expansion: search memory entries tagged with neighbor file paths
        // NOTE(P5.2): wire search_by_file_paths when RlmMemory gains it
        // For now, graph_expansion results are empty.

        drop(mem); // release memory lock

        // Graph neighborhood metadata
        let graph_neighbors = if gctx.focused_file.is_some() {
            Some(serde_json::json!({
                "file": gctx.focused_file,
                "imports": gctx.imports.iter().take(10).collect::<Vec<_>>(),
                "imported_by": gctx.imported_by.iter().take(10).collect::<Vec<_>>(),
                "total_imports": gctx.imports.len(),
                "total_imported_by": gctx.imported_by.len(),
            }))
        } else {
            None
        };

        let mut output = serde_json::json!({
            "total_matches": result.total_matches() + rlm_json.len().saturating_sub(result.rlm_matches.len()),
            "rlm_matches": rlm_json,
            "semantic_matches": semantic_json,
            "graph_neighbors": graph_neighbors,
        });

        self.graph_svc.inject(&mut output, &gctx);

        // CognitiveNexus: inject predictive context
        let cctx = self.nexus.resolve("touring_memory_recall", &p.query).await;
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

        // Apply detail_level truncation + append suggestions
        params::apply_detail_level(&mut output, dl);
        crate::tools::suggestions::append_to_response(&mut output, "touring_memory_recall", 2);

        let text = serde_json::to_string_pretty(&output)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    // ── Pattern Clustering (HNSW-based lazy clustering) ─────────────────

    /// Pattern clustering via HNSW-based lazy clustering
    #[cfg(feature = "async-memory")]
    #[tool(
        name = "touring_memory_clusters",
        description = "Pattern clustering via HNSW-based lazy clustering. Actions: list (all clusters), stats (clustering statistics), members (cluster members), similar (find similar clusters)"
    )]
    async fn memory_clusters(
        &self,
        params: Parameters<MemoryClustersParams>,
    ) -> Result<CallToolResult, McpError> {
        use crate::tools::cluster_tools::ClusterTools;

        let p = params.0;
        let cluster_tools = ClusterTools::new(&self.config).map_err(|e| {
            McpError::internal_error(format!("ClusterTools init failed: {}", e), None)
        })?;

        let result = cluster_tools
            .clusters(crate::tools::cluster_tools::MemoryClustersInput {
                action: p.action,
                cluster_id: p.cluster_id,
                query_embedding: p.query_embedding,
                top_k: p.top_k.unwrap_or(10),
            })
            .await
            .map_err(|e| {
                McpError::internal_error(format!("Cluster operation failed: {}", e), None)
            })?;

        let text = serde_json::to_string_pretty(&result)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    #[cfg(not(feature = "async-memory"))]
    #[tool(
        name = "touring_memory_clusters",
        description = "Pattern clustering via HNSW-based lazy clustering"
    )]
    async fn memory_clusters(
        &self,
        params: Parameters<MemoryClustersParams>,
    ) -> Result<CallToolResult, McpError> {
        // Validate input contract: action must be one of the valid actions
        let _action = params.0.action;
        Err(McpError::invalid_params(
            "Clustering requires async-memory feature (not enabled in this build)",
            None,
        ))
    }

    // ── Q-Learning (TD(lambda) with eligibility traces) ──────────────────

    /// TD(lambda) Q-learning: update Q-value or query best action
    #[tool(
        name = "touring_learn_pattern",
        description = "TD(lambda) Q-learning: update Q-value or query best action"
    )]
    async fn learn_pattern(
        &self,
        params: Parameters<LearnPatternParams>,
    ) -> Result<CallToolResult, McpError> {
        let p = params.0;
        let gctx = self.graph_svc.resolve_ctx(None).await;

        // Accept both "action" and "operation" for backwards compat; map "query" -> "get_q"
        let action = p
            .action
            .or(p.operation)
            .unwrap_or_else(|| "get_q".to_string());
        let action = if action == "query" {
            "get_q".to_string()
        } else {
            action
        };
        let state = p.state.unwrap_or(0);

        let action_id_from_field = p.action_id;

        let mut output = match action.as_str() {
            "update" => {
                let action_id = action_id_from_field.unwrap_or(0);
                let reward = p.reward.unwrap_or(0.0);
                let next_state = p.next_state.unwrap_or(0);
                let terminal = p.terminal.unwrap_or(false);

                let mut qt = self.qtable.lock().await;
                let td_error = qt.update(state, action_id, reward, next_state, None, terminal);

                serde_json::json!({
                    "action": "update",
                    "state": state,
                    "action_id": action_id,
                    "reward": reward,
                    "td_error": td_error,
                    "q_value": qt.get_q(state, action_id),
                    "table_size": qt.len(),
                })
            }
            "get_q" => {
                let action_id = action_id_from_field.unwrap_or(0);
                let qt = self.qtable.lock().await;

                serde_json::json!({
                    "action": "get_q",
                    "state": state,
                    "action_id": action_id,
                    "q_value": qt.get_q(state, action_id),
                    "all_q_values": qt.get_state_q_values(state)
                        .iter()
                        .map(|(a, q)| serde_json::json!({"action": a, "q_value": q}))
                        .collect::<Vec<_>>(),
                })
            }
            "best_action" => {
                let qt = self.qtable.lock().await;
                serde_json::json!({
                    "action": "best_action",
                    "state": state,
                    "best_action": qt.best_action(state),
                    "table_size": qt.len(),
                })
            }
            "reset_traces" => {
                let mut qt = self.qtable.lock().await;
                qt.reset_traces();
                serde_json::json!({
                    "action": "reset_traces",
                    "status": "traces_reset",
                    "table_size": qt.len(),
                })
            }
            _ => {
                return Err(McpError::invalid_params(
                    format!(
                        "Unknown learn_pattern action: '{}'. Valid: update, get_q, best_action, reset_traces",
                        action
                    ),
                    None,
                ));
            }
        };
        self.graph_svc.inject(&mut output, &gctx);

        let dl = p.detail_level.unwrap_or_default();
        params::apply_detail_level(&mut output, dl);
        crate::tools::suggestions::append_to_response(&mut output, "touring_learn_pattern", 2);
        let text = serde_json::to_string_pretty(&output)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    // ── Skill Clustering (cosine similarity clustering) ──────────────────

    /// Skill clustering: record usage, compute clusters, find similar
    #[tool(
        name = "touring_cluster_skills",
        description = "Skill clustering: record usage, compute clusters, find similar"
    )]
    async fn cluster_skills(
        &self,
        params: Parameters<ClusterSkillsParams>,
    ) -> Result<CallToolResult, McpError> {
        let p = params.0;
        let gctx = self.graph_svc.resolve_ctx(None).await;

        // Accept both "action" and "operation" for backwards compat
        let action = p
            .action
            .or(p.operation)
            .unwrap_or_else(|| "get_clusters".to_string());

        let mut output = match action.as_str() {
            "record" => {
                let skill_id = p.skill_id.ok_or_else(|| {
                    McpError::invalid_params("'skill_id' required for record", None)
                })?;
                let context = p.context.unwrap_or_else(|| "default".to_string());
                let success = p.success.unwrap_or(true);

                let mut cl = self.clusterer.lock().await;
                cl.record_usage(&skill_id, &context, success);

                serde_json::json!({
                    "action": "record",
                    "skill_id": skill_id,
                    "context": context,
                    "success": success,
                    "status": "recorded",
                })
            }
            "cluster" => {
                let mut cl = self.clusterer.lock().await;
                let clusters = cl.cluster();

                let clusters_json: Vec<serde_json::Value> = clusters
                    .iter()
                    .map(|c| {
                        serde_json::json!({
                            "id": c.id,
                            "skills": c.skills,
                            "cohesion": c.cohesion,
                            "size": c.skills.len(),
                        })
                    })
                    .collect();

                serde_json::json!({
                    "action": "cluster",
                    "cluster_count": clusters.len(),
                    "clusters": clusters_json,
                })
            }
            "find_similar" => {
                let skill_id = p.skill_id.ok_or_else(|| {
                    McpError::invalid_params("'skill_id' required for find_similar", None)
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
                    "action": "find_similar",
                    "skill_id": skill_id,
                    "results": similar_json,
                })
            }
            "get_clusters" => {
                let cl = self.clusterer.lock().await;
                let clusters = cl.get_clusters();

                let clusters_json: Vec<serde_json::Value> = clusters
                    .iter()
                    .map(|c| {
                        serde_json::json!({
                            "id": c.id,
                            "skills": c.skills,
                            "cohesion": c.cohesion,
                            "size": c.skills.len(),
                        })
                    })
                    .collect();

                serde_json::json!({
                    "action": "get_clusters",
                    "cluster_count": clusters.len(),
                    "clusters": clusters_json,
                })
            }
            _ => {
                return Err(McpError::invalid_params(
                    format!(
                        "Unknown cluster_skills action: '{}'. Valid: record, cluster, find_similar, get_clusters",
                        action
                    ),
                    None,
                ));
            }
        };
        self.graph_svc.inject(&mut output, &gctx);

        let dl = p.detail_level.unwrap_or_default();
        params::apply_detail_level(&mut output, dl);
        crate::tools::suggestions::append_to_response(&mut output, "touring_cluster_skills", 2);
        let text = serde_json::to_string_pretty(&output)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    // ── AST Surgery (tree-sitter byte-exact editing) ─────────────────────

    /// AST surgery: replace symbol body or validate syntax
    #[tool(
        annotations(
            read_only_hint = false,
            idempotent_hint = false,
            title = "Edit source via AST"
        ),
        name = "touring_ast_edit",
        description = "Edit code by AST surgery — replace a symbol's body or validate syntax."
    )]
    async fn ast_edit(
        &self,
        params: Parameters<AstEditParams>,
    ) -> Result<CallToolResult, McpError> {
        let p = params.0;

        // Tier A: update focus + resolve graph context
        if let Some(ref path) = p.file_path {
            self.graph_svc.update_focus(path).await;
        }
        let gctx = self.graph_svc.resolve_ctx(p.file_path.as_deref()).await;

        let action = p.action.unwrap_or_else(|| "validate_syntax".to_string());
        let content = &p.content;

        let mut output = match action.as_str() {
            "replace_body" => {
                let symbol_name = p.symbol_name.ok_or_else(|| {
                    McpError::invalid_params("'symbol_name' required for replace_body", None)
                })?;
                let new_body = p.new_body.ok_or_else(|| {
                    McpError::invalid_params("'new_body' required for replace_body", None)
                })?;

                match touring_code::ast::surgery::replace_symbol_body(
                    content,
                    &symbol_name,
                    &new_body,
                ) {
                    Ok(edited) => {
                        serde_json::json!({
                            "action": "replace_body",
                            "symbol_name": symbol_name,
                            "status": "success",
                            "edited_content": edited,
                            "original_length": content.len(),
                            "edited_length": edited.len(),
                        })
                    }
                    Err(e) => {
                        serde_json::json!({
                            "action": "replace_body",
                            "symbol_name": symbol_name,
                            "status": "error",
                            "error": e.to_string(),
                        })
                    }
                }
            }
            "validate_syntax" | "validate" => {
                let language = p.language.unwrap_or_else(|| "python".to_string());
                let lang = language.parse::<touring_code::ast::Lang>().map_err(|_| {
                    McpError::invalid_params(format!("Unknown language: {language}"), None)
                })?;

                match touring_code::ast::surgery::validate_syntax(content, lang) {
                    Ok(valid) => {
                        serde_json::json!({
                            "action": "validate_syntax",
                            "language": language,
                            "valid": valid,
                            "content_length": content.len(),
                        })
                    }
                    Err(e) => {
                        serde_json::json!({
                            "action": "validate_syntax",
                            "language": language,
                            "valid": false,
                            "error": e.to_string(),
                        })
                    }
                }
            }
            _ => {
                return Err(McpError::invalid_params(
                    format!(
                        "Unknown ast_edit action: '{}'. Valid: replace_body, validate_syntax",
                        action
                    ),
                    None,
                ));
            }
        };

        // Warn when editing a hub file (many importers)
        if gctx.confidence_modifier < 0.85 {
            // SAFETY: serde_json::Value string indexing never panics — returns Null for missing keys.
            #[allow(clippy::indexing_slicing)]
            {
                output["graph_warning"] = serde_json::json!(format!(
                    "Editing a hub file ({} direct importers, confidence={:.2}). Review impact before applying.",
                    gctx.blast_radius_count, gctx.confidence_modifier
                ));
            }
        }
        self.graph_svc.inject(&mut output, &gctx);

        let dl = p.detail_level.unwrap_or_default();
        params::apply_detail_level(&mut output, dl);
        crate::tools::suggestions::append_to_response(&mut output, "touring_ast_edit", 2);
        let text = serde_json::to_string_pretty(&output)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    // ── Index Status (config + filesystem inspection) ────────────────────

    /// Get project index status and configuration
    #[tool(
        annotations(read_only_hint = true, title = "Index status"),
        name = "touring_index_status",
        description = "Report the project index status (symbol/file counts) and configuration."
    )]
    async fn index_status(
        &self,
        params: Parameters<IndexStatusParams>,
    ) -> Result<CallToolResult, McpError> {
        let dl = params.0.detail_level.unwrap_or_default();
        let gctx = self.graph_svc.resolve_ctx(None).await;
        let project_path = params.0.project_path.unwrap_or_else(|| ".".to_string());
        let project_root = std::path::Path::new(&project_path);
        let db_path = &self.config.symbols_db_path;

        let db_exists = db_path.exists();
        let db_size = if db_exists {
            std::fs::metadata(db_path).map(|m| m.len()).unwrap_or(0)
        } else {
            0
        };

        let rlm_exists = self.config.rlm_db_path.exists();
        let rlm_size = if rlm_exists {
            std::fs::metadata(&self.config.rlm_db_path)
                .map(|m| m.len())
                .unwrap_or(0)
        } else {
            0
        };

        let graph_stats = {
            let idx = self.graph_svc.index().lock().await;
            let s = idx.stats();
            serde_json::json!({
                "total_symbols": s.total_symbols,
                "total_locations": s.total_locations,
                "total_files": s.total_files,
                "total_dependencies": s.total_dependencies,
                "symbol_store_active": self.symbol_store.is_some(),
            })
        };

        let mut output = serde_json::json!({
            "project_path": project_root.canonicalize().unwrap_or_else(|_| project_root.to_path_buf()),
            "symbols_db": {
                "path": db_path,
                "exists": db_exists,
                "size_bytes": db_size,
            },
            "rlm_db": {
                "path": self.config.rlm_db_path,
                "exists": rlm_exists,
                "size_bytes": rlm_size,
            },
            "semantic_db": {
                "path": self.config.semantic_db_path,
                "exists": self.config.semantic_db_path.exists(),
            },
            "memory_store_active": self.memory.is_some(),
            "graph_index": graph_stats,
            "config": {
                "cache_size": self.config.cache_size,
                "max_file_size": self.config.max_file_size,
                "watcher_debounce_ms": self.config.watcher_debounce_ms,
                "debug": self.config.debug,
            },
        });
        self.graph_svc.inject(&mut output, &gctx);

        params::apply_detail_level(&mut output, dl);
        crate::tools::suggestions::append_to_response(&mut output, "touring_index_status", 2);
        let text = serde_json::to_string_pretty(&output)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    // ── Checkpoint (file-based) ──────────────────────────────────────────

    /// Create a checkpoint file
    #[tool(name = "touring_checkpoint", description = "Create a checkpoint file")]
    async fn checkpoint(
        &self,
        params: Parameters<CheckpointParams>,
    ) -> Result<CallToolResult, McpError> {
        let p = params.0;
        let gctx = self.graph_svc.resolve_ctx(None).await;
        let description = p.description.unwrap_or_default();
        let tags = p.tags.unwrap_or_default();

        let checkpoint_dir = std::path::PathBuf::from(".claude/checkpoints");
        std::fs::create_dir_all(&checkpoint_dir).map_err(|e| {
            McpError::internal_error(format!("Failed to create checkpoint dir: {}", e), None)
        })?;

        let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
        let filename = format!("checkpoint_{}.json", timestamp);
        let path = checkpoint_dir.join(&filename);

        let checkpoint = serde_json::json!({
            "timestamp": timestamp.to_string(),
            "description": description,
            "tags": tags,
            "version": env!("CARGO_PKG_VERSION"),
            "memory_active": self.memory.is_some(),
        });

        let checkpoint_json = serde_json::to_string_pretty(&checkpoint)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        tokio::fs::write(&path, checkpoint_json)
            .await
            .map_err(|e| {
                McpError::internal_error(format!("Failed to write checkpoint: {}", e), None)
            })?;

        let mut output = serde_json::json!({
            "status": "created",
            "path": path.display().to_string(),
            "focused_file": gctx.focused_file,
        });
        self.graph_svc.inject(&mut output, &gctx);

        let dl = p.detail_level.unwrap_or_default();
        params::apply_detail_level(&mut output, dl);
        crate::tools::suggestions::append_to_response(&mut output, "touring_checkpoint", 2);
        let text = serde_json::to_string_pretty(&output)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    // ── File Operations (filesystem) ─────────────────────────────────────

    /// SEC-01 guard: resolve a user-supplied `touring_file_ops` path and confirm it stays
    /// within an allowed root. The tool is always-on and reachable via prompt-injection;
    /// without this it is an arbitrary filesystem read/write/delete primitive.
    ///
    /// Allowed roots = the configured project root plus any colon-separated extra roots in
    /// `TOURING_FILE_OPS_ALLOW_ROOTS` (deny-by-default, mirroring the CEG capability model).
    /// `must_exist = false` lets create/write/mkdir target a not-yet-existing in-root leaf.
    fn guard_fs_path(&self, raw: &str, must_exist: bool) -> Result<std::path::PathBuf, McpError> {
        let mut roots: Vec<std::path::PathBuf> = Vec::new();
        match self.config.project_root.canonicalize() {
            Ok(p) => roots.push(p),
            Err(_) => roots.push(self.config.project_root.clone()),
        }
        if let Ok(extra) = std::env::var("TOURING_FILE_OPS_ALLOW_ROOTS") {
            for r in extra.split(':').filter(|s| !s.is_empty()) {
                if let Ok(c) = std::path::Path::new(r).canonicalize() {
                    roots.push(c);
                }
            }
        }
        crate::tools::file_tools::enforce_path_within_roots(
            std::path::Path::new(raw),
            &roots,
            must_exist,
        )
        .map_err(|denied| {
            McpError::invalid_params(
                format!(
                    "touring_file_ops denied: '{}' is outside the allowed root(s). The tool is \
                     jailed to the project root; set TOURING_FILE_OPS_ALLOW_ROOTS (colon-separated) \
                     to extend.",
                    denied.display()
                ),
                None,
            )
        })
    }

    /// File operations: read, write, append, delete, find/search, stat, exists, mkdir, copy, move/rename, glob, tree
    #[tool(
        name = "touring_file_ops",
        description = "File operations: read, write, append, delete, find/search, stat, exists, mkdir, copy, move/rename, glob, tree"
    )]
    async fn file_ops(
        &self,
        params: Parameters<FileOpsParams>,
    ) -> Result<CallToolResult, McpError> {
        let p = params.0;

        // Tier A: update focus + resolve graph context (path is non-optional)
        self.graph_svc.update_focus(&p.path).await;
        let gctx = self.graph_svc.resolve_ctx(Some(p.path.as_str())).await;

        // Accept aliases: action->operation
        let operation = p.operation.or(p.action).unwrap_or_default();
        let path = &p.path;

        // SEC-01: jail every filesystem operation to an allowed root before touching disk.
        // write/append/mkdir/exists validate the parent (the leaf may not exist yet); all
        // other operations require the target itself to resolve inside a root.
        let needs_existing = !matches!(operation.as_str(), "write" | "append" | "mkdir" | "exists");
        let safe_path = self.guard_fs_path(path, needs_existing)?;

        let mut output = match operation.as_str() {
            "read" => {
                let file_content = tokio::fs::read_to_string(&safe_path)
                    .await
                    .map_err(|e| McpError::internal_error(format!("Read failed: {}", e), None))?;
                serde_json::json!({
                    "operation": "read",
                    "path": path,
                    "size": file_content.len(),
                    "content": file_content,
                })
            }
            "write" => {
                let content = p.content.ok_or_else(|| {
                    McpError::invalid_params("'content' required for write", None)
                })?;
                tokio::fs::write(&safe_path, &content)
                    .await
                    .map_err(|e| McpError::internal_error(format!("Write failed: {}", e), None))?;
                serde_json::json!({
                    "operation": "write",
                    "path": path,
                    "bytes_written": content.len(),
                    "status": "success",
                })
            }
            "append" => {
                let content = p.content.ok_or_else(|| {
                    McpError::invalid_params("'content' required for append", None)
                })?;
                use tokio::io::AsyncWriteExt;
                let mut file = tokio::fs::OpenOptions::new()
                    .append(true)
                    .create(true)
                    .open(&safe_path)
                    .await
                    .map_err(|e| McpError::internal_error(format!("Append failed: {}", e), None))?;
                file.write_all(content.as_bytes()).await.map_err(|e| {
                    McpError::internal_error(format!("Append write failed: {}", e), None)
                })?;
                serde_json::json!({
                    "operation": "append",
                    "path": path,
                    "bytes_appended": content.len(),
                    "status": "success",
                })
            }
            "delete" => {
                tokio::fs::remove_file(&safe_path)
                    .await
                    .map_err(|e| McpError::internal_error(format!("Delete failed: {}", e), None))?;
                serde_json::json!({
                    "operation": "delete",
                    "path": path,
                    "status": "success",
                })
            }
            "find" | "search" => {
                let root = safe_path.as_path();
                let pat = p.pattern.as_deref().unwrap_or("**/*");
                let max_depth = p.max_depth.unwrap_or(10);
                let content_pat = p.content_pattern.as_deref();
                let include_hidden = p.include_hidden.unwrap_or(false);
                let use_regex = p.use_regex.unwrap_or(false);
                let files = crate::tools::file_tools::find_workspace_files(
                    root,
                    pat,
                    use_regex,
                    content_pat,
                    max_depth,
                    include_hidden,
                );
                serde_json::json!({
                    "operation": "find",
                    "root": path,
                    "pattern": pat,
                    "max_depth": max_depth,
                    "count": files.len(),
                    "files": files,
                })
            }
            "stat" => {
                let meta = tokio::fs::metadata(&safe_path)
                    .await
                    .map_err(|e| McpError::internal_error(format!("Stat failed: {}", e), None))?;
                let modified = meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs());
                let created = meta
                    .created()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs());
                serde_json::json!({
                    "operation": "stat",
                    "path": path,
                    "is_file": meta.is_file(),
                    "is_dir": meta.is_dir(),
                    "is_symlink": meta.is_symlink(),
                    "size_bytes": meta.len(),
                    "modified_unix": modified,
                    "created_unix": created,
                    "readonly": meta.permissions().readonly(),
                })
            }
            "exists" => {
                let meta = tokio::fs::metadata(&safe_path).await;
                let exists = meta.is_ok();
                let is_file = meta.as_ref().map(|m| m.is_file()).unwrap_or(false);
                let is_dir = meta.as_ref().map(|m| m.is_dir()).unwrap_or(false);
                serde_json::json!({
                    "operation": "exists",
                    "path": path,
                    "exists": exists,
                    "is_file": is_file,
                    "is_dir": is_dir,
                })
            }
            "mkdir" => {
                tokio::fs::create_dir_all(&safe_path)
                    .await
                    .map_err(|e| McpError::internal_error(format!("Mkdir failed: {}", e), None))?;
                serde_json::json!({
                    "operation": "mkdir",
                    "path": path,
                    "status": "success",
                })
            }
            "copy" => {
                let dest = p
                    .dest
                    .as_ref()
                    .ok_or_else(|| McpError::invalid_params("'dest' required for copy", None))?;
                let safe_dest = self.guard_fs_path(dest, false)?;
                // Ensure parent of dest exists
                if let Some(parent) = safe_dest.parent() {
                    tokio::fs::create_dir_all(parent).await.map_err(|e| {
                        McpError::internal_error(format!("Copy mkdir failed: {}", e), None)
                    })?;
                }
                let bytes = tokio::fs::copy(&safe_path, &safe_dest)
                    .await
                    .map_err(|e| McpError::internal_error(format!("Copy failed: {}", e), None))?;
                serde_json::json!({
                    "operation": "copy",
                    "src": path,
                    "dest": dest,
                    "bytes_copied": bytes,
                    "status": "success",
                })
            }
            "move" | "rename" => {
                let dest = p.dest.as_ref().ok_or_else(|| {
                    McpError::invalid_params("'dest' required for move/rename", None)
                })?;
                let safe_dest = self.guard_fs_path(dest, false)?;
                // Ensure parent of dest exists
                if let Some(parent) = safe_dest.parent() {
                    tokio::fs::create_dir_all(parent).await.map_err(|e| {
                        McpError::internal_error(format!("Move mkdir failed: {}", e), None)
                    })?;
                }
                tokio::fs::rename(&safe_path, &safe_dest)
                    .await
                    .map_err(|e| McpError::internal_error(format!("Move failed: {}", e), None))?;
                serde_json::json!({
                    "operation": &operation,
                    "src": path,
                    "dest": dest,
                    "status": "success",
                })
            }
            "glob" => {
                // List directory contents matching optional pattern (non-recursive by default)
                let dir_path = safe_path.as_path();
                let pat = p.pattern.as_deref().unwrap_or("*");
                let include_hidden = p.include_hidden.unwrap_or(false);
                let files = crate::tools::file_tools::find_workspace_files(
                    dir_path,
                    pat,
                    false,
                    None,
                    1, // depth=1 for glob (non-recursive)
                    include_hidden,
                );
                serde_json::json!({
                    "operation": "glob",
                    "path": path,
                    "pattern": pat,
                    "count": files.len(),
                    "entries": files,
                })
            }
            "tree" => {
                // Recursive directory listing as hierarchical structure
                let root = safe_path.as_path();
                let max_depth = p.max_depth.unwrap_or(5);
                let include_hidden = p.include_hidden.unwrap_or(false);
                fn build_tree(
                    dir: &std::path::Path,
                    depth: usize,
                    max_depth: usize,
                    include_hidden: bool,
                ) -> serde_json::Value {
                    if depth > max_depth {
                        return serde_json::json!({"truncated": true});
                    }
                    let name = dir
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or(".")
                        .to_string();
                    let node_type = if dir.is_dir() { "dir" } else { "file" };
                    if dir.is_dir() {
                        let mut children = Vec::new();
                        if let Ok(mut entries) = std::fs::read_dir(dir) {
                            let mut items: Vec<_> = entries
                                .by_ref()
                                .flatten()
                                .filter(|e| {
                                    if !include_hidden {
                                        e.file_name()
                                            .to_str()
                                            .map(|n| !n.starts_with('.'))
                                            .unwrap_or(true)
                                    } else {
                                        true
                                    }
                                })
                                .collect();
                            items.sort_by_key(|e| {
                                let is_file = e.file_type().map(|t| t.is_file()).unwrap_or(false);
                                (is_file, e.file_name())
                            });
                            for entry in items {
                                children.push(build_tree(
                                    &entry.path(),
                                    depth + 1,
                                    max_depth,
                                    include_hidden,
                                ));
                            }
                        }
                        let size = std::fs::metadata(dir).map(|m| m.len()).ok();
                        serde_json::json!({
                            "name": name,
                            "path": dir.to_string_lossy(),
                            "type": node_type,
                            "size_bytes": size,
                            "children": children,
                        })
                    } else {
                        let size = std::fs::metadata(dir).map(|m| m.len()).ok();
                        serde_json::json!({
                            "name": name,
                            "path": dir.to_string_lossy(),
                            "type": node_type,
                            "size_bytes": size,
                        })
                    }
                }
                let tree = build_tree(root, 0, max_depth, include_hidden);
                serde_json::json!({
                    "operation": "tree",
                    "root": path,
                    "max_depth": max_depth,
                    "tree": tree,
                })
            }
            "delete_dir" => {
                let force = p.force.unwrap_or(false);
                if force {
                    tokio::fs::remove_dir_all(&safe_path).await.map_err(|e| {
                        McpError::internal_error(format!("Delete_dir failed: {}", e), None)
                    })?;
                } else {
                    tokio::fs::remove_dir(&safe_path).await.map_err(|e| {
                        McpError::internal_error(
                            format!(
                                "Delete_dir failed (use force=true to delete non-empty): {}",
                                e
                            ),
                            None,
                        )
                    })?;
                }
                serde_json::json!({
                    "operation": "delete_dir",
                    "path": path,
                    "force": force,
                    "status": "success",
                })
            }
            "list" => {
                // List immediate directory children (flat, no recursion)
                let dir_path = safe_path.as_path();
                let include_hidden = p.include_hidden.unwrap_or(false);
                let mut entries = Vec::new();
                let mut read_dir = tokio::fs::read_dir(dir_path)
                    .await
                    .map_err(|e| McpError::internal_error(format!("List failed: {}", e), None))?;
                while let Ok(Some(entry)) = read_dir.next_entry().await {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if !include_hidden && name.starts_with('.') {
                        continue;
                    }
                    let meta = entry.metadata().await.ok();
                    entries.push(serde_json::json!({
                        "name": name,
                        "path": entry.path().to_string_lossy(),
                        "type": if meta.as_ref().map(|m| m.is_dir()).unwrap_or(false) { "dir" }
                                else if meta.as_ref().map(|m| m.is_symlink()).unwrap_or(false) { "symlink" }
                                else { "file" },
                        "size_bytes": meta.as_ref().map(|m| m.len()),
                    }));
                }
                entries.sort_by(|a, b| {
                    let ta = a["type"].as_str().unwrap_or("");
                    let tb = b["type"].as_str().unwrap_or("");
                    ta.cmp(tb).then(
                        a["name"]
                            .as_str()
                            .unwrap_or("")
                            .cmp(b["name"].as_str().unwrap_or("")),
                    )
                });
                serde_json::json!({
                    "operation": "list",
                    "path": path,
                    "count": entries.len(),
                    "entries": entries,
                })
            }
            _ => {
                return Err(McpError::invalid_params(
                    format!(
                        "Unknown file operation: '{}'. Valid: read, write, append, delete, delete_dir, \
                         find, search, stat, exists, mkdir, copy, move, rename, glob, tree, list",
                        operation
                    ),
                    None,
                ));
            }
        };

        self.graph_svc.inject(&mut output, &gctx);

        let dl = p.detail_level.unwrap_or_default();
        params::apply_detail_level(&mut output, dl);
        crate::tools::suggestions::append_to_response(&mut output, "touring_file_ops", 2);
        let text = serde_json::to_string_pretty(&output)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    // ── Project Info (config + system info) ──────────────────────────────

    /// Get project configuration and status
    #[tool(
        name = "touring_project",
        description = "Get project configuration and status"
    )]
    async fn project(&self, params: Parameters<ProjectParams>) -> Result<CallToolResult, McpError> {
        let dl = params.0.detail_level.unwrap_or_default();
        let gctx = self.graph_svc.resolve_ctx(None).await;
        let project_path = params
            .0
            .project_path
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| self.config.project_root.clone());

        let qtable_size = self.qtable.lock().await.len();
        let online_rl_stats = {
            let rl = self.online_rl.lock().await;
            (rl.update_count(), rl.ema_reward())
        };
        let linucb_pulls = self.linucb.lock().await.total_pulls();

        let mut output = serde_json::json!({
            "project_root": project_path.canonicalize().unwrap_or(project_path),
            "version": env!("CARGO_PKG_VERSION"),
            "config": {
                "symbols_db": self.config.symbols_db_path,
                "rlm_db": self.config.rlm_db_path,
                "semantic_db": self.config.semantic_db_path,
                "cache_size": self.config.cache_size,
                "max_file_size": self.config.max_file_size,
            },
            "modules": {
                "classifier": {
                    "active": true,
                    "pattern_count": self.classifier.pattern_count(),
                },
                "pii_scanner": {
                    "active": true,
                    "pattern_count": self.pii_scanner.pii_pattern_count(),
                },
                "memory": {
                    "active": self.memory.is_some(),
                },
                "qtable": {
                    "active": true,
                    "size": qtable_size,
                    "online_rl_updates": online_rl_stats.0,
                    "online_rl_ema_reward": online_rl_stats.1,
                    "linucb_pulls": linucb_pulls,
                },
                "clusterer": {
                    "active": true,
                },
            },
        });
        self.graph_svc.inject(&mut output, &gctx);

        params::apply_detail_level(&mut output, dl);
        crate::tools::suggestions::append_to_response(&mut output, "touring_project", 2);
        let text = serde_json::to_string_pretty(&output)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    /// Resolve a file path to its corresponding project root using longest-prefix match.
    #[tool(
        name = "touring_resolve_project",
        description = "Resolves a file path to its corresponding project root using longest-prefix match across indexed projects. Returns the project directory that owns the file."
    )]
    async fn resolve_project_for_file(
        &self,
        params: Parameters<ProjectResolverParams>,
    ) -> Result<CallToolResult, McpError> {
        let file_path = params.0.file_path.clone();
        let project_path = self.graph_svc.resolve_project_for_file(&file_path);
        Ok(CallToolResult::success(vec![Content::text(
            project_path.to_string_lossy(),
        )]))
    }

    // ====================================================================
    // NEW TOOLS (Sprint 1-3) — 6 tools
    // ====================================================================

    // ── D.2 Semantic primitives ─────────────────────────────────────────

    /// Resolve a file:line:col position to its definition.
    #[tool(
        name = "touring_resolve_def",
        description = "Resolves a source position (file:line:col) to its symbol definition. Returns kind, name, source range, and definition ID."
    )]
    async fn resolve_def(
        &self,
        params: Parameters<ResolveDefParams>,
    ) -> Result<CallToolResult, McpError> {
        let p = params.0;
        let source = if let Some(s) = p.source {
            s
        } else {
            std::fs::read_to_string(&p.file_path).map_err(|e| {
                McpError::internal_error(format!("cannot read '{}': {}", p.file_path, e), None)
            })?
        };

        let payload = serde_json::json!({
            "file": p.file_path,
            "line": p.line,
            "column": p.column,
            "source": source
        });

        let output = daemon_query("cli-resolve-def", payload)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        let dl = p.detail_level.unwrap_or_default();
        let mut json: serde_json::Value =
            serde_json::from_str(&output).unwrap_or_else(|_| serde_json::json!({"raw": output}));

        params::apply_detail_level(&mut json, dl);
        crate::tools::suggestions::append_to_response(&mut json, "touring_resolve_def", 2);

        let text = serde_json::to_string_pretty(&json)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    /// Find all references to the symbol at a file:line:col position.
    #[tool(
        annotations(read_only_hint = true, title = "Find symbol references"),
        name = "touring_find_references",
        description = "Finds all references to the symbol at a given position. Supports workspace and project scope."
    )]
    async fn find_references(
        &self,
        params: Parameters<FindReferencesParams>,
    ) -> Result<CallToolResult, McpError> {
        let p = params.0;
        let source = if let Some(s) = p.source {
            s
        } else {
            std::fs::read_to_string(&p.file_path).map_err(|e| {
                McpError::internal_error(format!("cannot read '{}': {}", p.file_path, e), None)
            })?
        };

        let payload = serde_json::json!({
            "file": p.file_path,
            "line": p.line,
            "column": p.column,
            "scope": p.scope,
            "source": source
        });

        let output = daemon_query("cli-find-references", payload)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        let dl = p.detail_level.unwrap_or_default();
        let mut json: serde_json::Value =
            serde_json::from_str(&output).unwrap_or_else(|_| serde_json::json!({"raw": output}));

        params::apply_detail_level(&mut json, dl);
        crate::tools::suggestions::append_to_response(&mut json, "touring_find_references", 2);

        let text = serde_json::to_string_pretty(&json)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    /// Rename a symbol across all its usages in scope.
    #[tool(
        name = "touring_rename",
        description = "Renames a symbol across all references. Use apply=true to commit changes, or false for a dry run preview."
    )]
    async fn rename(&self, params: Parameters<RenameParams>) -> Result<CallToolResult, McpError> {
        let p = params.0;
        let source = if let Some(s) = p.source {
            s
        } else {
            std::fs::read_to_string(&p.file_path).map_err(|e| {
                McpError::internal_error(format!("cannot read '{}': {}", p.file_path, e), None)
            })?
        };

        let payload = serde_json::json!({
            "file": p.file_path,
            "line": p.line,
            "column": p.column,
            "new_name": p.new_name,
            "apply": p.apply,
            "source": source
        });

        let output = daemon_query("cli-rename", payload)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;

        let dl = p.detail_level.unwrap_or_default();
        let mut json: serde_json::Value =
            serde_json::from_str(&output).unwrap_or_else(|_| serde_json::json!({"raw": output}));

        params::apply_detail_level(&mut json, dl);
        crate::tools::suggestions::append_to_response(&mut json, "touring_rename", 2);

        let text = serde_json::to_string_pretty(&json)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    // ── Entity Identity Registry Tools (D5.4) ───────────────────────────────

    /// Define a new entity in the identity registry
    #[tool(
        name = "touring_entity_define",
        description = "Define a new entity in the identity registry"
    )]
    async fn entity_define(
        &self,
        params: Parameters<EntityDefineParams>,
    ) -> Result<CallToolResult, McpError> {
        let p = params.0;
        use touring_identity::{Entity, EntityId, EntityKind, IdentityRegistry};

        let kind = match p.kind.to_lowercase().as_str() {
            "function" => EntityKind::Function,
            "type" => EntityKind::Type,
            "module" => EntityKind::Module,
            "constant" => EntityKind::Constant,
            "trait" => EntityKind::Trait,
            "macro" => EntityKind::Macro,
            "file" => EntityKind::File,
            "config" => EntityKind::Config,
            _ => {
                return Err(McpError::invalid_params(
                    format!(
                        "Unknown entity kind: {}. Valid: function, type, module, constant, trait, macro, file, config",
                        p.kind
                    ),
                    None,
                ));
            }
        };

        let mut entity = Entity::new(EntityId::from_str(&p.id), &p.name, kind, &p.crate_name);

        if let Some(ref sp) = p.source_path {
            let line = p.definition_line.unwrap_or(0);
            entity = entity.with_source(sp, line);
        }
        if let Some(ref doc) = p.doc_summary {
            entity = entity.with_doc(doc);
        }

        let db_path = default_entity_db_path()
            .map_err(|e| McpError::internal_error(format!("DB path error: {}", e), None))?;
        let mut reg = IdentityRegistry::open_or_create(&db_path).map_err(|e| {
            McpError::internal_error(format!("Failed to open registry: {}", e), None)
        })?;

        let id_out = reg
            .define(&entity)
            .map_err(|e| McpError::internal_error(format!("Define failed: {}", e), None))?;

        let mut output = serde_json::json!({
            "status": "defined",
            "id": id_out.as_str(),
            "canonical_name": p.name,
            "kind": p.kind,
            "crate": p.crate_name,
        });

        let dl = p.detail_level.unwrap_or_default();
        params::apply_detail_level(&mut output, dl);
        crate::tools::suggestions::append_to_response(&mut output, "touring_entity_define", 2);

        let text = serde_json::to_string_pretty(&output)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    /// Resolve an entity name with exact or fuzzy matching
    #[tool(
        name = "touring_entity_resolve",
        description = "Resolve an entity name with exact or fuzzy matching"
    )]
    async fn entity_resolve(
        &self,
        params: Parameters<EntityResolveParams>,
    ) -> Result<CallToolResult, McpError> {
        let p = params.0;
        use touring_identity::{IdentityRegistry, MatchKind};

        let db_path = default_entity_db_path()
            .map_err(|e| McpError::internal_error(format!("DB path error: {}", e), None))?;
        let mut reg = IdentityRegistry::open_or_create(&db_path).map_err(|e| {
            McpError::internal_error(format!("Failed to open registry: {}", e), None)
        })?;

        let max_edit = p.max_edit_distance.unwrap_or(2);
        let mut candidates = reg
            .resolve(&p.name, max_edit)
            .map_err(|e| McpError::internal_error(format!("Resolve failed: {}", e), None))?;

        if p.exact_only.unwrap_or(false) {
            candidates.retain(|c| matches!(c.match_kind, MatchKind::Exact));
        }

        let output: serde_json::Value = if candidates.is_empty() {
            serde_json::json!({
                "status": "not_found",
                "name": p.name,
                "candidates": [],
            })
        } else {
            serde_json::json!({
                "status": "found",
                "name": p.name,
                "candidates": candidates.iter().map(|c| {
                    serde_json::json!({
                        "id": c.entity.id.as_str(),
                        "canonical_name": c.entity.canonical_name.as_str(),
                        "kind": format!("{:?}", c.entity.kind).to_lowercase(),
                        "crate_name": c.entity.crate_name.as_str(),
                        "source_path": c.entity.source_path.as_ref().map(|s| s.as_str()),
                        "definition_line": c.entity.definition_line,
                        "doc_summary": c.entity.doc_summary.as_ref().map(|s| s.as_str()),
                        "match_kind": format!("{:?}", c.match_kind).to_lowercase(),
                        "confidence": c.confidence,
                    })
                }).collect::<Vec<_>>(),
            })
        };

        let dl = p.detail_level.unwrap_or_default();
        let mut json = output;
        params::apply_detail_level(&mut json, dl);
        crate::tools::suggestions::append_to_response(&mut json, "touring_entity_resolve", 2);

        let text = serde_json::to_string_pretty(&json)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    /// Create a relation between two entities
    #[tool(
        name = "touring_entity_relate",
        description = "Create a relation between two entities"
    )]
    async fn entity_relate(
        &self,
        params: Parameters<EntityRelateParams>,
    ) -> Result<CallToolResult, McpError> {
        let p = params.0;
        use touring_identity::{EntityId, IdentityRegistry, RelationKind};

        let kind = match p.kind.to_lowercase().as_str() {
            "derived_from" => RelationKind::DerivedFrom,
            "refines" => RelationKind::Refines,
            "supersedes" => RelationKind::Supersedes,
            "equivalent" => RelationKind::Equivalent,
            "see_also" => RelationKind::SeeAlso,
            "wraps" => RelationKind::Wraps,
            _ => {
                return Err(McpError::invalid_params(
                    format!(
                        "Unknown relation kind: {}. Valid: derived_from, refines, supersedes, equivalent, see_also, wraps",
                        p.kind
                    ),
                    None,
                ));
            }
        };

        let db_path = default_entity_db_path()
            .map_err(|e| McpError::internal_error(format!("DB path error: {}", e), None))?;
        let mut reg = IdentityRegistry::open_or_create(&db_path).map_err(|e| {
            McpError::internal_error(format!("Failed to open registry: {}", e), None)
        })?;

        let rel_id = reg
            .relate(
                &EntityId::from_str(&p.from),
                kind,
                &EntityId::from_str(&p.to),
            )
            .map_err(|e| McpError::internal_error(format!("Relate failed: {}", e), None))?;

        let mut output = serde_json::json!({
            "status": "related",
            "from": p.from,
            "relation": p.kind,
            "to": p.to,
            "relation_id": rel_id,
        });

        let dl = p.detail_level.unwrap_or_default();
        params::apply_detail_level(&mut output, dl);
        crate::tools::suggestions::append_to_response(&mut output, "touring_entity_relate", 2);

        let text = serde_json::to_string_pretty(&output)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    /// List entities with optional filters
    #[tool(
        name = "touring_entity_list",
        description = "List entities with optional crate and kind filters"
    )]
    async fn entity_list(
        &self,
        params: Parameters<EntityListParams>,
    ) -> Result<CallToolResult, McpError> {
        let p = params.0;
        use touring_identity::{EntityKind, IdentityRegistry};

        let kind_filter = p
            .kind
            .as_ref()
            .and_then(|k| match k.to_lowercase().as_str() {
                "function" => Some(EntityKind::Function),
                "type" => Some(EntityKind::Type),
                "module" => Some(EntityKind::Module),
                "constant" => Some(EntityKind::Constant),
                "trait" => Some(EntityKind::Trait),
                "macro" => Some(EntityKind::Macro),
                "file" => Some(EntityKind::File),
                "config" => Some(EntityKind::Config),
                _ => None,
            });

        let db_path = default_entity_db_path()
            .map_err(|e| McpError::internal_error(format!("DB path error: {}", e), None))?;
        let mut reg = IdentityRegistry::open_or_create(&db_path).map_err(|e| {
            McpError::internal_error(format!("Failed to open registry: {}", e), None)
        })?;

        let entities = reg
            .list(p.crate_name.as_deref(), kind_filter)
            .map_err(|e| McpError::internal_error(format!("List failed: {}", e), None))?;

        let limit = p.limit.unwrap_or(50) as usize;
        let entities: Vec<_> = entities.into_iter().take(limit).collect();

        let mut output = serde_json::json!({
            "status": "ok",
            "count": entities.len(),
            "entities": entities.iter().map(|e| {
                serde_json::json!({
                    "id": e.id.as_str(),
                    "canonical_name": e.canonical_name.as_str(),
                    "kind": format!("{:?}", e.kind).to_lowercase(),
                    "crate_name": e.crate_name.as_str(),
                    "source_path": e.source_path.as_ref().map(|s| s.as_str()),
                    "definition_line": e.definition_line,
                    "doc_summary": e.doc_summary.as_ref().map(|s| s.as_str()),
                })
            }).collect::<Vec<_>>(),
        });

        let dl = p.detail_level.unwrap_or_default();
        params::apply_detail_level(&mut output, dl);
        crate::tools::suggestions::append_to_response(&mut output, "touring_entity_list", 2);

        let text = serde_json::to_string_pretty(&output)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    /// Delete an entity from the registry
    #[tool(
        name = "touring_entity_delete",
        description = "Delete an entity from the registry"
    )]
    async fn entity_delete(
        &self,
        params: Parameters<EntityDeleteParams>,
    ) -> Result<CallToolResult, McpError> {
        let p = params.0;
        use touring_identity::{EntityId, IdentityRegistry};

        let db_path = default_entity_db_path()
            .map_err(|e| McpError::internal_error(format!("DB path error: {}", e), None))?;
        let mut reg = IdentityRegistry::open_or_create(&db_path).map_err(|e| {
            McpError::internal_error(format!("Failed to open registry: {}", e), None)
        })?;

        let reason = p.reason.unwrap_or_else(|| "no reason given".to_string());
        reg.delete(&EntityId::from_str(&p.id), &reason)
            .map_err(|e| McpError::internal_error(format!("Delete failed: {}", e), None))?;

        let mut output = serde_json::json!({
            "status": "deleted",
            "id": p.id,
            "reason": reason,
        });

        let dl = p.detail_level.unwrap_or_default();
        params::apply_detail_level(&mut output, dl);
        crate::tools::suggestions::append_to_response(&mut output, "touring_entity_delete", 2);

        let text = serde_json::to_string_pretty(&output)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }
}

#[cfg(test)]
#[path = "tools_core_tests.rs"]
mod entity_db_tests;
