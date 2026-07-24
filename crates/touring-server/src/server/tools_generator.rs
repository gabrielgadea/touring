use super::*;

#[tool_router(router = router_generator, vis = "pub(crate)")]
impl TouringServer {
    // ── Generator Tools (20 MCP tools) ───────────────────────────────────

    #[tool(
        annotations(
            read_only_hint = false,
            idempotent_hint = false,
            title = "Submit generation plan"
        ),
        name = "touring_generator_submit_plan",
        description = "Run the full generator pipeline (VGP verify → template render → speculate → commit). Pass a GeneratorPlan as plan_json. Set dry_run=true to stop after render without committing."
    )]
    async fn generator_submit_plan(
        &self,
        params: Parameters<GeneratorSubmitParams>,
    ) -> Result<CallToolResult, McpError> {
        let p = params.0;
        let result = crate::tools::generator_tools::submit_plan_with_registry(
            &p.plan_json,
            p.dry_run,
            &self.plan_registry,
        )
        .await;
        let text = serde_json::to_string_pretty(&result)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    #[tool(
        name = "touring_generator_validate_plan",
        description = "Validate a GeneratorPlan JSON for schema correctness without executing the pipeline. Returns valid=true/false and a list of errors."
    )]
    async fn generator_validate_plan(
        &self,
        params: Parameters<GeneratorPlanParams>,
    ) -> Result<CallToolResult, McpError> {
        let result = crate::tools::generator_tools::validate_plan(&params.0.plan_json);
        let text = serde_json::to_string_pretty(&result)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    #[tool(
        name = "touring_generator_verify_plan",
        description = "Run VGP symbol verification only on a GeneratorPlan. Checks that all must_exist symbols are present in the touring index."
    )]
    async fn generator_verify_plan(
        &self,
        params: Parameters<GeneratorPlanParams>,
    ) -> Result<CallToolResult, McpError> {
        let result = crate::tools::generator_tools::verify_plan(&params.0.plan_json).await;
        let text = serde_json::to_string_pretty(&result)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    #[tool(
        name = "touring_generator_render_plan",
        description = "Run VGP verify + template render on a GeneratorPlan. Stops before speculate and commit. Returns ok=true if rendering succeeds."
    )]
    async fn generator_render_plan(
        &self,
        params: Parameters<GeneratorPlanParams>,
    ) -> Result<CallToolResult, McpError> {
        let result = crate::tools::generator_tools::render_plan(&params.0.plan_json).await;
        let text = serde_json::to_string_pretty(&result)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    #[tool(
        name = "touring_generator_speculate_plan",
        description = "Run VGP verify + render + speculate on a GeneratorPlan. Stops before commit. Use to validate the generated artifact without writing to disk."
    )]
    async fn generator_speculate_plan(
        &self,
        params: Parameters<GeneratorPlanParams>,
    ) -> Result<CallToolResult, McpError> {
        let result = crate::tools::generator_tools::speculate_plan(&params.0.plan_json).await;
        let text = serde_json::to_string_pretty(&result)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    #[tool(
        name = "touring_generator_commit_plan",
        description = "Alias for touring_generator_submit_plan with dry_run=false. Runs the full pipeline and commits the artifact to disk."
    )]
    async fn generator_commit_plan(
        &self,
        params: Parameters<GeneratorPlanParams>,
    ) -> Result<CallToolResult, McpError> {
        let result = crate::tools::generator_tools::submit_plan(&params.0.plan_json, false).await;
        let text = serde_json::to_string_pretty(&result)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    #[tool(
        name = "touring_generator_rollback_plan",
        description = "Check rollback availability for a plan's target file. Reports whether a .bak backup exists. Does not execute the restore — reports what would happen."
    )]
    async fn generator_rollback_plan(
        &self,
        params: Parameters<GeneratorPlanParams>,
    ) -> Result<CallToolResult, McpError> {
        let result = crate::tools::generator_tools::rollback_plan(&params.0.plan_json);
        let text = serde_json::to_string_pretty(&result)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    #[tool(
        name = "touring_generator_plan_status",
        description = "Show plan metadata (id, kind, intent, target, cila_level, trace entries) and inline validation results for a GeneratorPlan JSON."
    )]
    async fn generator_plan_status(
        &self,
        params: Parameters<GeneratorPlanParams>,
    ) -> Result<CallToolResult, McpError> {
        let result = crate::tools::generator_tools::plan_status(&params.0.plan_json);
        let text = serde_json::to_string_pretty(&result)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    #[tool(
        name = "touring_generator_schema_dump",
        description = "Emit the JSON Schema for the GeneratorPlan struct. Use to validate plan files programmatically or generate editor integrations."
    )]
    async fn generator_schema_dump(
        &self,
        params: Parameters<GeneratorSchemaDumpParams>,
    ) -> Result<CallToolResult, McpError> {
        let schema_version = params.0.version.unwrap_or_else(|| "v1.0".to_string());
        let mut result = crate::tools::generator_tools::schema_dump();
        // Annotate schema dump with the requested version string
        if let Some(obj) = result.as_object_mut() {
            obj.insert(
                "schema_version".to_string(),
                serde_json::json!(schema_version),
            );
        }
        let text = serde_json::to_string_pretty(&result)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    #[tool(
        name = "touring_generator_schema_check",
        description = "Check whether a GeneratorPlan version is compatible with the current engine. Returns compatibility status and available migration paths."
    )]
    async fn generator_schema_check(
        &self,
        params: Parameters<SchemaVersionParams>,
    ) -> Result<CallToolResult, McpError> {
        let result = crate::tools::generator_tools::schema_check(&params.0.version);
        let text = serde_json::to_string_pretty(&result)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    #[tool(
        name = "touring_generator_bundle",
        description = "Execute multiple GeneratorPlans sequentially as a bundle. Pass plans_json as a Vec of plan JSON strings. Set dry_run=true to skip commits. Returns a manifest with per-plan stage results and aggregate counts."
    )]
    async fn generator_bundle(
        &self,
        params: Parameters<GeneratorBundleParams>,
    ) -> Result<CallToolResult, McpError> {
        let p = params.0;
        let result = crate::tools::generator_tools::bundle(&p.plans_json, p.dry_run).await;
        let text = serde_json::to_string_pretty(&result)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    #[tool(
        name = "touring_generator_recall_similar",
        description = "Search touring memory for past generation patterns matching the query. Returns top-N matching memory entries ordered by relevance."
    )]
    async fn generator_recall_similar(
        &self,
        params: Parameters<GeneratorRecallParams>,
    ) -> Result<CallToolResult, McpError> {
        let p = params.0;
        let result = crate::tools::generator_tools::recall_similar(&p.query, p.limit);
        let text = serde_json::to_string_pretty(&result)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    #[tool(
        name = "touring_generator_diff_plans",
        description = "Compare two GeneratorPlan JSON strings. Returns a list of field-level differences (kind, intent, target, version). identical=true if no diffs found."
    )]
    async fn generator_diff_plans(
        &self,
        params: Parameters<GeneratorDiffParams>,
    ) -> Result<CallToolResult, McpError> {
        let p = params.0;
        let result =
            crate::tools::generator_tools_introspect::diff_plans(&p.plan_a_json, &p.plan_b_json);
        let text = serde_json::to_string_pretty(&result)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    #[tool(
        name = "touring_generator_plan_history",
        description = "Show the execution_trace lineage of a GeneratorPlan. Returns all trace entries populated by previous pipeline runs."
    )]
    async fn generator_plan_history(
        &self,
        params: Parameters<GeneratorPlanParams>,
    ) -> Result<CallToolResult, McpError> {
        let result = crate::tools::generator_tools_introspect::plan_history(&params.0.plan_json);
        let text = serde_json::to_string_pretty(&result)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    #[tool(
        name = "touring_generator_critique_plan",
        description = "Analyze a GeneratorPlan structure and report issues: empty intent, missing contracts, invalid target paths. Returns error_count, warning_count, and issue list."
    )]
    async fn generator_critique_plan(
        &self,
        params: Parameters<GeneratorPlanParams>,
    ) -> Result<CallToolResult, McpError> {
        let result = crate::tools::generator_tools_introspect::critique_plan(&params.0.plan_json);
        let text = serde_json::to_string_pretty(&result)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    #[tool(
        name = "touring_generator_suggest_plan",
        description = "Generate a skeleton GeneratorPlan JSON for the given intent and optional kind. Returns a ready-to-edit plan with sensible defaults. Replace plan_id and target.file_path before submitting."
    )]
    async fn generator_suggest_plan(
        &self,
        params: Parameters<GeneratorSuggestParams>,
    ) -> Result<CallToolResult, McpError> {
        let p = params.0;
        let result = crate::tools::generator_tools::suggest_plan(&p.intent, p.kind.as_deref());
        let text = serde_json::to_string_pretty(&result)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    #[tool(
        name = "touring_generator_template_list",
        description = "List all 30 built-in Tera templates with their GeneratorKind, label, and template filename. Use template names with touring_generator_template_test."
    )]
    async fn generator_template_list(
        &self,
        _params: Parameters<GeneratorEmptyParams>,
    ) -> Result<CallToolResult, McpError> {
        let result = crate::tools::generator_tools::template_list();
        let text = serde_json::to_string_pretty(&result)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    #[tool(
        name = "touring_generator_template_validate",
        description = "Validate a Tera template file for syntax errors. Pass the absolute path to a .tera file. Returns valid=true/false and any syntax errors."
    )]
    async fn generator_template_validate(
        &self,
        params: Parameters<GeneratorTemplateValidateParams>,
    ) -> Result<CallToolResult, McpError> {
        let result = crate::tools::generator_tools::template_validate(&params.0.template_file);
        let text = serde_json::to_string_pretty(&result)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    #[tool(
        name = "touring_generator_template_test",
        description = "Render a built-in template by name with given vars. Use touring_generator_template_list to get valid template names. Pass vars as a JSON object string."
    )]
    async fn generator_template_test(
        &self,
        params: Parameters<GeneratorTemplateTestParams>,
    ) -> Result<CallToolResult, McpError> {
        let p = params.0;
        let result =
            crate::tools::generator_tools::template_test(&p.template_name, p.vars_json.as_deref());
        let text = serde_json::to_string_pretty(&result)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    #[tool(
        name = "touring_generator_kinds_list",
        description = "List all 30 GeneratorKind variants with their label and template name. Alias for touring_generator_template_list with kind-focused output."
    )]
    async fn generator_kinds_list(
        &self,
        _params: Parameters<GeneratorEmptyParams>,
    ) -> Result<CallToolResult, McpError> {
        let result = crate::tools::generator_tools::kinds_list();
        let text = serde_json::to_string_pretty(&result)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    #[tool(
        name = "touring_generator_capacity",
        description = "Show CapacityLimits defaults: max plan size, max contracts, token budget per CILA level. Use to understand pipeline constraints before submitting large plans."
    )]
    async fn generator_capacity(
        &self,
        _params: Parameters<GeneratorEmptyParams>,
    ) -> Result<CallToolResult, McpError> {
        let result = crate::tools::generator_tools::capacity();
        let text = serde_json::to_string_pretty(&result)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    #[tool(
        name = "touring_generator_replay_plan",
        description = "Replay a GeneratorPlan through the full pipeline (VGP → render → speculate → commit) with iteration counter incremented. Use to re-run a previously committed plan."
    )]
    async fn generator_replay_plan(
        &self,
        params: Parameters<GeneratorPlanParams>,
    ) -> Result<CallToolResult, McpError> {
        let result = crate::tools::generator_tools::replay_plan(&params.0.plan_json).await;
        let text = serde_json::to_string_pretty(&result)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    #[tool(
        name = "touring_generator_consumer_wiring",
        description = "Query orphan pub symbols from the wiring system and generate ConsumerGenerator plans for each wiring opportunity. Returns ok=true, count, and plans[]. Submit each plan via touring_generator_submit_plan to create wiring implementations."
    )]
    async fn generator_consumer_wiring(
        &self,
        params: Parameters<GeneratorConsumerWiringParams>,
    ) -> Result<CallToolResult, McpError> {
        let result = crate::tools::generator_tools::build_consumer_generator_plans(params.0.limit);
        let text = serde_json::to_string_pretty(&result)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    #[tool(
        name = "touring_generator_schema_registry_info",
        description = "Query the SchemaRegistry: engine version, number of migration paths, and migration keys. Use before schema_check to understand what versions are available."
    )]
    async fn generator_schema_registry_info(
        &self,
        _params: Parameters<GeneratorEmptyParams>,
    ) -> Result<CallToolResult, McpError> {
        let result = crate::tools::generator_tools::schema_registry_info();
        let text = serde_json::to_string_pretty(&result)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    #[tool(
        name = "touring_generator_registry_status",
        description = "List all in-flight generator plans from the PlanRegistry. Returns plan_id, intent_preview, and execution status for each registered plan. Use filter to restrict to a specific plan_id prefix."
    )]
    async fn generator_registry_status(
        &self,
        params: Parameters<GeneratorRegistryParams>,
    ) -> Result<CallToolResult, McpError> {
        let p = params.0;
        let filter = p.filter.as_deref().unwrap_or("");
        let dl = p.detail_level.unwrap_or_default();
        let entries = self.plan_registry.list();
        let filtered: Vec<_> = entries
            .into_iter()
            .filter(|(id, _, _)| filter.is_empty() || id.starts_with(filter))
            .map(|(id, preview, status)| {
                serde_json::json!({
                    "plan_id": id,
                    "intent_preview": preview,
                    "status": format!("{:?}", status),
                })
            })
            .collect();
        let mut result_val = serde_json::json!({
            "ok": true,
            "count": filtered.len(),
            "plans": filtered,
        });
        crate::server::params::apply_detail_level(&mut result_val, dl);
        crate::tools::suggestions::append_to_response(
            &mut result_val,
            "touring_generator_registry_status",
            2,
        );
        let text = serde_json::to_string_pretty(&result_val)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }
}
