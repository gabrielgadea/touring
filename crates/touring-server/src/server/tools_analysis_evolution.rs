//! Evolution MCP tool methods (insights / status / drift) for `TouringServer`.
//!
//! Extracted from `tools_analysis.rs` (F-9): a second `impl TouringServer` block
//! — Rust permits inherent-impl blocks split across modules in the same crate.

use super::*;

#[tool_router(router = router_analysis_ext, vis = "pub(crate)")]
impl TouringServer {
    // ── Evolution Insights ──────────────────────────────────────────────

    /// Query evolution insights: tool effectiveness, CILA trends, drift detection, cost analysis
    #[tool(
        name = "touring_insights",
        description = "Query operational insights from Touring's learning engine: tool effectiveness, CILA progression, cost efficiency, drift detection. Filter via axis/category/min_severity (see param enums). Omit all params for full summary."
    )]
    async fn insights(
        &self,
        params: Parameters<InsightsParams>,
    ) -> Result<CallToolResult, McpError> {
        let p = params.0;
        let dl = p.detail_level.unwrap_or_default();
        let gctx = self.graph_svc.resolve_ctx(None).await;

        // EvolutionAnalyzer takes ownership of WilsonRanker + DriftDetector.
        // Clone the already-loaded instances from the server's fields to avoid
        // reopening SQLite (LearningPersistence::load_wilson + load_drift) on every call.
        // Only RlmMemory still requires a fresh connection (no persistent field on server).
        let rlm_for_analysis = RlmMemory::new(&self.config.rlm_db_path)
            .map_err(|e| McpError::internal_error(format!("RLM init failed: {}", e), None))?;
        let ranker_snapshot = self.ranker.lock().await.clone();
        let drift_snapshot = self.drift_detector.lock().await.clone();

        let analyzer = EvolutionAnalyzer::new(rlm_for_analysis, ranker_snapshot, drift_snapshot);

        let results = analyzer.analyze_all();
        let insights = InsightEngine::generate(&results);

        // Apply filters
        let filtered: Vec<_> = insights
            .into_iter()
            .filter(|i| {
                if let Some(ref axis_str) = p.axis {
                    let matches = match axis_str.as_str() {
                        "self_improvement" | "self" => matches!(i.axis, Axis::SelfImprovement),
                        "project_evolution" | "project" => matches!(i.axis, Axis::ProjectEvolution),
                        _ => true,
                    };
                    if !matches {
                        return false;
                    }
                }
                if let Some(ref cat) = p.category {
                    if !i.category.contains(cat.as_str()) {
                        return false;
                    }
                }
                if let Some(ref sev) = p.min_severity {
                    let min = match sev.as_str() {
                        "critical" => Severity::Critical,
                        "warning" => Severity::Warning,
                        _ => Severity::Info,
                    };
                    if i.severity < min {
                        return false;
                    }
                }
                true
            })
            .collect();

        let mut output = serde_json::json!({
            "total_insights": filtered.len(),
            "insights": filtered.iter().map(|i| serde_json::json!({
                "axis": i.axis.to_string(),
                "category": i.category,
                "severity": i.severity.to_string(),
                "message": i.message,
                "evidence": i.evidence,
                "recommendation": i.recommendation,
            })).collect::<Vec<_>>(),
        });
        self.graph_svc.inject(&mut output, &gctx);

        params::apply_detail_level(&mut output, dl);
        crate::tools::suggestions::append_to_response(&mut output, "touring_insights", 2);
        let text = serde_json::to_string_pretty(&output)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    // ── Evolution Status ────────────────────────────────────────────────

    /// Get status of the self-evolution engine
    #[tool(
        name = "touring_evolution_status",
        description = "Get status of the self-evolution engine: learning state, drift detection, watcher stats"
    )]
    async fn evolution_status(
        &self,
        params: Parameters<EvolutionStatusParams>,
    ) -> Result<CallToolResult, McpError> {
        let dl = params.0.detail_level.unwrap_or_default();
        let detailed = params.0.detailed.unwrap_or(false);
        let gctx = self.graph_svc.resolve_ctx(None).await;

        let ranker = self.ranker.lock().await;
        let drift = self.drift_detector.lock().await;
        let qt = self.qtable.lock().await;
        let rl_engine = self.online_rl.lock().await;
        let bandit = self.linucb.lock().await;

        let ranker_stats = ranker.all_stats();
        let drift_histories = drift.all_histories();

        let mut output = serde_json::json!({
            "learning": {
                "qtable_size": qt.len(),
                "wilson_items": ranker_stats.len(),
                "drift_metrics": drift_histories.len(),
                "online_rl": {
                    "update_count": rl_engine.update_count(),
                    "ema_reward": rl_engine.ema_reward(),
                    "linucb_total_pulls": bandit.total_pulls(),
                },
            },
            "memory": {
                "active": self.memory.is_some(),
                "embedder_active": self.embedder.is_some(),
            },
            "config": {
                "evolution_enabled": self.config.evolution_enabled,
                "evolution_interval_s": self.config.evolution_interval_s,
                "jsonl_watch_enabled": self.config.jsonl_watch_enabled,
                "jsonl_poll_interval_s": self.config.jsonl_poll_interval_s,
            },
        });

        if detailed {
            let wilson_detail: Vec<_> = ranker_stats
                .iter()
                .map(|(id, s, t)| {
                    serde_json::json!({
                        "id": id,
                        "successes": s,
                        "trials": t,
                        "score": ranker.score(id),
                    })
                })
                .collect();

            let drift_detail: Vec<_> = drift_histories
                .iter()
                .map(|(metric, values)| {
                    serde_json::json!({
                        "metric": metric,
                        "data_points": values.len(),
                        "latest": values.last(),
                    })
                })
                .collect();

            // SAFETY: serde_json::Value string indexing never panics — returns Null for missing keys.
            #[allow(clippy::indexing_slicing)]
            {
                output["wilson_detail"] = serde_json::json!(wilson_detail);
                output["drift_detail"] = serde_json::json!(drift_detail);
            }
        }
        self.graph_svc.inject(&mut output, &gctx);

        params::apply_detail_level(&mut output, dl);
        crate::tools::suggestions::append_to_response(&mut output, "touring_evolution_status", 2);
        let text = serde_json::to_string_pretty(&output)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    // ── Evolution Drift ──────────────────────────────────────────────────

    /// Query drift detection results from the self-evolution engine.
    /// Optionally filter by metric name; returns all metrics if None.
    #[tool(
        name = "touring_evolution_drift",
        description = "Query drift detection results from Touring's learning engine. Optionally filter by metric name. Returns drift status for all tracked metrics: drift_detected, magnitude, direction, confidence, and trend label. Use to identify which metrics in the system are degrading or improving over time."
    )]
    async fn evolution_drift(
        &self,
        params: Parameters<DriftParams>,
    ) -> Result<CallToolResult, McpError> {
        let p = params.0;
        let dl = p.detail_level.unwrap_or_default();
        let gctx = self.graph_svc.resolve_ctx(None).await;

        let drift_snapshot = self.drift_detector.lock().await.clone();

        let output = drift::handle_drift(
            drift_snapshot,
            drift::DriftInput {
                metric_name: p.metric_name,
            },
        );

        let mut json_output = serde_json::json!({
            "success": output.success,
            "metrics_returned": output.metrics_returned,
            "filter_metric": output.filter_metric,
            "results": output.results,
        });
        if let Some(err) = output.error {
            if let serde_json::Value::Object(ref mut map) = json_output {
                map.insert("error".to_string(), serde_json::json!(err));
            }
        }
        self.graph_svc.inject(&mut json_output, &gctx);

        params::apply_detail_level(&mut json_output, dl);
        crate::tools::suggestions::append_to_response(
            &mut json_output,
            "touring_evolution_drift",
            2,
        );
        let text = serde_json::to_string_pretty(&json_output)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }
}
