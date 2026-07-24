//! Infra MCP tools (source_change / rename_symbol / detect_clones) — extracted
//! from `tools_infra.rs` (F-9) as a second `#[tool_router]` block (router_infra_ext),
//! merged into the server router set in `mod.rs` alongside `router_infra`.

use super::*;

#[tool_router(router = router_infra_ext, vis = "pub(crate)")]
impl TouringServer {
    // ── Wave B B.5.8 — touring_source_change MCP tool ─────────────────────

    #[tool(
        name = "touring_source_change",
        description = "Apply/preview a transactional multi-file SourceChange (edits + fs_edits + optional snippet). operation=preview|validate (dry-run shadow) | apply (atomic, rolls back on failure). Returns status, file counts, errors."
    )]
    async fn source_change(
        &self,
        params: Parameters<SourceChangeParams>,
    ) -> Result<CallToolResult, McpError> {
        use std::collections::BTreeMap;
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        use touring_generator::{Applier, ApplyResult, FileId, Indel, SourceChange, TextEdit};

        let p = params.0;

        // Parse source_change JSON
        let json_value: serde_json::Value = serde_json::from_str(&p.source_change_json)
            .map_err(|e| McpError::invalid_params(format!("invalid JSON: {}", e), None))?;

        // Build SourceChange + files map (mirrors cli/source_change.rs logic)
        #[allow(clippy::type_complexity)]
        fn build_source_change_and_files(
            value: &serde_json::Value,
        ) -> anyhow::Result<(
            SourceChange,
            BTreeMap<FileId, String>,
            BTreeMap<FileId, std::path::PathBuf>,
        )> {
            let mut change = SourceChange::new();
            let mut files: BTreeMap<FileId, String> = BTreeMap::new();
            let mut paths: BTreeMap<FileId, std::path::PathBuf> = BTreeMap::new();

            if let Some(edits) = value.get("edits").and_then(|v| v.as_object()) {
                for (file_path, indels_val) in edits {
                    let file_id = path_to_file_id(file_path);
                    let disk_content = std::fs::read_to_string(file_path).unwrap_or_default();
                    files.insert(file_id, disk_content.clone());
                    paths.insert(file_id, std::path::PathBuf::from(file_path));

                    let indels_array = indels_val
                        .as_array()
                        .ok_or_else(|| anyhow::anyhow!("edits[{}] must be an array", file_path))?;

                    let mut indels = Vec::new();
                    for indel_val in indels_array {
                        let delete = indel_val
                            .get("delete")
                            .and_then(|v| v.as_array())
                            .filter(|a| a.len() == 2);
                        let insert = indel_val
                            .get("insert")
                            .and_then(|v| v.as_str())
                            .unwrap_or("");

                        let (start, end) = match delete {
                            Some(arr) if arr.len() == 2 => {
                                let s = arr[0].as_u64().unwrap_or(0) as usize;
                                let e = arr[1].as_u64().unwrap_or(0) as usize;
                                (s, e)
                            }
                            _ => continue,
                        };
                        indels.push(Indel {
                            delete: start..end,
                            insert: insert.to_string(),
                        });
                    }

                    if !indels.is_empty() {
                        let text_edit = TextEdit::try_from_iter(indels)
                            .map_err(|e| anyhow::anyhow!("invalid TextEdit: {}", e))?;
                        change = change.with_edit(file_id, text_edit);
                    }
                }
            }

            if let Some(fs_edits) = value.get("fs_edits").and_then(|v| v.as_array()) {
                for fs_edit_val in fs_edits {
                    if let Some(obj) = fs_edit_val.as_object() {
                        for (variant, payload) in obj {
                            use touring_generator::FileSystemEdit;
                            match variant.as_str() {
                                "CreateFile" => {
                                    if let Some(fields) = payload.as_object() {
                                        let path = fields
                                            .get("path")
                                            .and_then(|v| v.as_str())
                                            .unwrap_or_default();
                                        let content = fields
                                            .get("content")
                                            .and_then(|v| v.as_str())
                                            .unwrap_or_default();
                                        change = change.with_fs_edit(FileSystemEdit::CreateFile {
                                            path: std::path::PathBuf::from(path),
                                            content: content.to_string(),
                                        });
                                    }
                                }
                                "OverwriteFile" => {
                                    if let Some(fields) = payload.as_object() {
                                        let path = fields
                                            .get("path")
                                            .and_then(|v| v.as_str())
                                            .unwrap_or_default();
                                        let content = fields
                                            .get("content")
                                            .and_then(|v| v.as_str())
                                            .unwrap_or_default();
                                        change =
                                            change.with_fs_edit(FileSystemEdit::OverwriteFile {
                                                path: std::path::PathBuf::from(path),
                                                content: content.to_string(),
                                            });
                                    }
                                }
                                "MoveFile" => {
                                    if let Some(fields) = payload.as_object() {
                                        let from = fields
                                            .get("from")
                                            .and_then(|v| v.as_str())
                                            .unwrap_or_default();
                                        let to = fields
                                            .get("to")
                                            .and_then(|v| v.as_str())
                                            .unwrap_or_default();
                                        change = change.with_fs_edit(FileSystemEdit::MoveFile {
                                            from: std::path::PathBuf::from(from),
                                            to: std::path::PathBuf::from(to),
                                        });
                                    }
                                }
                                "DeleteFile" => {
                                    if let Some(fields) = payload.as_object() {
                                        let path = fields
                                            .get("path")
                                            .and_then(|v| v.as_str())
                                            .unwrap_or_default();
                                        change = change.with_fs_edit(FileSystemEdit::DeleteFile {
                                            path: std::path::PathBuf::from(path),
                                        });
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }

            if let Some(snippet_val) = value.get("snippet").and_then(|v| v.as_object()) {
                let template = snippet_val
                    .get("template")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                use touring_generator::SnippetEdit;
                let tab_stops = snippet_val
                    .get("tab_stops")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| {
                                let index = v.get("index")?.as_u64()? as u8;
                                let default_text =
                                    v.get("default_text").and_then(|w| w.as_str()).unwrap_or("");
                                Some(touring_generator::TabStop {
                                    index,
                                    default_text: default_text.to_string(),
                                })
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                let snippet_edit =
                    SnippetEdit::with_tab_stops(template.to_string(), tab_stops, None);
                change = change.with_snippet(snippet_edit);
            }

            Ok((change, files, paths))
        }

        fn path_to_file_id(path: &str) -> FileId {
            let mut hasher = DefaultHasher::new();
            path.hash(&mut hasher);
            hasher.finish() as FileId
        }

        let is_json_format = p.format.as_deref().unwrap_or("json") == "json";

        let (change, mut files, paths) = match build_source_change_and_files(&json_value) {
            Ok(v) => v,
            Err(e) => {
                return Ok(CallToolResult::success(vec![Content::text(format!(
                    "{{\"status\": \"error\", \"message\": \"failed to parse source_change JSON: {}\"}}",
                    e
                ))]));
            }
        };

        let applier = Applier::new();

        match p.operation.as_str() {
            "preview" | "validate" => {
                let validation = applier.shadow_validate(&change);
                let output = match validation {
                    ApplyResult::Valid => serde_json::json!({
                        "status": "valid",
                        "files": files.len(),
                        "fs_ops": change.fs_edit_count(),
                        "snippet": change.snippet().is_some()
                    }),
                    ApplyResult::Invalid { errors } => serde_json::json!({
                        "status": "invalid",
                        "errors": errors.iter().map(|e| e.to_string()).collect::<Vec<_>>()
                    }),
                    _ => serde_json::json!({
                        "status": "unexpected",
                        "message": "shadow_validate returned commit result"
                    }),
                };
                let text = if is_json_format {
                    serde_json::to_string_pretty(&output).unwrap_or_default()
                } else {
                    serde_json::to_string(&output).unwrap_or_default()
                };
                Ok(CallToolResult::success(vec![Content::text(text)]))
            }
            "apply" => {
                let validation = applier.shadow_validate(&change);
                match validation {
                    ApplyResult::Invalid { errors } => {
                        let output = serde_json::json!({
                            "status": "invalid",
                            "errors": errors.iter().map(|e| e.to_string()).collect::<Vec<_>>()
                        });
                        let text = serde_json::to_string_pretty(&output).unwrap_or_default();
                        return Ok(CallToolResult::success(vec![Content::text(text)]));
                    }
                    ApplyResult::Valid
                    | ApplyResult::Committed { .. }
                    | ApplyResult::RolledBack { .. } => {}
                }

                let path_for = |file_id: FileId| paths.get(&file_id).cloned();
                let result = applier.commit(&change, &mut files, path_for);
                let output = match result {
                    ApplyResult::Committed {
                        files_written,
                        fs_ops,
                    } => serde_json::json!({
                        "status": "committed",
                        "files_written": files_written,
                        "fs_ops": fs_ops
                    }),
                    ApplyResult::RolledBack {
                        errors,
                        partial_writes,
                    } => serde_json::json!({
                        "status": "rolled_back",
                        "errors": errors.iter().map(|e| e.to_string()).collect::<Vec<_>>(),
                        "partial_writes": partial_writes
                    }),
                    _ => serde_json::json!({
                        "status": "unexpected",
                        "message": "commit returned validation result"
                    }),
                };
                let text = if is_json_format {
                    serde_json::to_string_pretty(&output).unwrap_or_default()
                } else {
                    serde_json::to_string(&output).unwrap_or_default()
                };
                Ok(CallToolResult::success(vec![Content::text(text)]))
            }
            _ => Err(McpError::invalid_params(
                format!(
                    "unknown operation '{}'. Use: preview, apply, or validate",
                    p.operation
                ),
                None,
            )),
        }
    }

    /// Unified code search — combines keyword (BM25) + semantic (embedding) via RRF fusion.
    /// Runs intent detection and returns ranked results with confidence tiers.
    #[tool(
        name = "find_code",
        description = "Unified code search super-tool. Pass `query` (search string), optional `intent_override` (understand/debug/lookup/refactor/explore/navigate/document), and optional `max_results` (default 20, max 100). Returns results sorted by fused RRF score with file_path, line, col, backend, rrf_score, and confidence_tier. Internally uses detect_intent (keyword heuristics) + SearchPipeline (BM25 + embedding + RRF fusion)."
    )]
    async fn find_code(
        &self,
        params: Parameters<FindCodeParams>,
    ) -> Result<CallToolResult, McpError> {
        let p = params.0;
        match crate::tools::search_tools::find_code_impl(self, p).await {
            Ok(json_str) => Ok(CallToolResult::success(vec![Content::text(json_str)])),
            Err(e) => Err(McpError::invalid_params(e, None)),
        }
    }

    // ── D7 — touring_rename_symbol MCP tool ────────────────────────────────

    #[tool(
        name = "touring_rename_symbol",
        description = "D7: Rename a symbol across a scope. Takes `symbol` (current name), \
                       `new_name` (desired name), and optional `scope` (file/dir/project). \
                       Returns a rename plan with blast_radius, risk_tier (low/medium/high), \
                       and list of edit sites (file_path, line, col, kind). Uses RenamePlan + \
                       generate_rename_plan from refactor module."
    )]
    async fn touring_rename_symbol(
        &self,
        params: Parameters<RenameSymbolParams>,
    ) -> Result<CallToolResult, McpError> {
        let p = params.0;
        match crate::tools::refactor_tools::rename_symbol_impl(self, p).await {
            Ok(json_str) => Ok(CallToolResult::success(vec![Content::text(json_str)])),
            Err(e) => Err(McpError::invalid_params(e, None)),
        }
    }

    // ── D9 — touring_detect_clones MCP tool ──────────────────────────────

    #[tool(
        name = "touring_detect_clones",
        description = "D9: Detect structural clone groups in the codebase. Uses \
                       touring_code::ast::symbols::find_clones() which groups symbols by \
                       structural_hash (kind + param_count + complexity_bucket + line_bucket). \
                       Takes optional `path` (workspace root), `min_similarity` (0.0-1.0, \
                       default 0.5), and `detail_level` (verbosity). Returns clone groups with \
                       file:line:col, similarity scores, and code snippets."
    )]
    async fn detect_clones(
        &self,
        params: Parameters<crate::server::params::DetectClonesParams>,
    ) -> Result<CallToolResult, McpError> {
        let p = params.0;
        let min_sim = p.min_similarity.unwrap_or(0.5);
        let dl = p.detail_level.unwrap_or_default();

        let clone_params = crate::tools::clone_tools::DetectClonesParams {
            path: p.path,
            min_similarity: Some(min_sim),
            detail_level: Some(dl),
        };

        match crate::tools::clone_tools::detect_clones_impl(clone_params) {
            Ok(response) => {
                let mut output = serde_json::to_value(&response)
                    .map_err(|e| McpError::internal_error(e.to_string(), None))?;
                self.graph_svc
                    .inject(&mut output, &self.graph_svc.resolve_ctx(None).await);
                crate::tools::suggestions::append_to_response(
                    &mut output,
                    "touring_detect_clones",
                    2,
                );
                let text = serde_json::to_string_pretty(&output)
                    .map_err(|e| McpError::internal_error(e.to_string(), None))?;
                Ok(CallToolResult::success(vec![Content::text(text)]))
            }
            Err(e) => Err(McpError::internal_error(e.to_string(), None)),
        }
    }
}
