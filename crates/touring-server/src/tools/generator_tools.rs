//! Generator Tools — 20 MCP tools for the touring-generator pipeline.
//!
//! Mirrors the `touring generate` CLI subcommands as MCP tools for use by
//! LLM clients. All tools delegate to `touring_generator` crate functions
//! and/or CLI subprocess calls for plan lifecycle management.
//!
//! ## Tool Groups
//!
//! | Group | Tools |
//! |-------|-------|
//! | Plan lifecycle | submit, validate, verify, render, speculate, commit, rollback, status, replay |
//! | Plan analysis  | diff, history, critique, suggest, recall |
//! | Template ops   | list, validate, test |
//! | Info           | kinds_list, schema_dump, capacity |

use std::collections::HashMap;
use std::sync::Arc;

use serde_json::Value;
use touring_generator::{
    CapacityLimits, ExecutionStatus, GeneratorContext, GeneratorKind, GeneratorPlan, PlanExecutor,
    PlanExecutorHandle, PlanRegistry, RenderShape, Rendered, ReplanRequest,
};
use touring_intelligence::rl::{PheroKey, UnifiedPheromoneBus};

// ── Helper: parse GeneratorPlan from a JSON string ─────────────────────────

/// Parse a `GeneratorPlan` from a raw JSON string.
///
/// Returns a [`crate::ServerError::PlanParse`] variant on failure so callers
/// can match on the typed error rather than inspecting string contents.
pub fn parse_plan(plan_json: &str) -> Result<GeneratorPlan, crate::ServerError> {
    serde_json::from_str(plan_json)
        .map_err(|e| crate::ServerError::PlanParse(format!("invalid plan JSON: {e}")))
}

/// Detect language string from a file path extension.
///
/// Used by the knowledge upsert closure to enrich `FileKnowledge.language`.
/// Returns `None` for unrecognised extensions.
fn language_from_path(path: &str) -> Option<String> {
    /// Static extension → language mapping. CC=1 (no branching).
    static EXT_MAP: &[(&str, &str)] = &[
        ("rs", "rust"),
        ("py", "python"),
        ("ts", "typescript"),
        ("tsx", "typescript"),
        ("js", "javascript"),
        ("jsx", "javascript"),
        ("go", "go"),
        ("c", "c"),
        ("h", "c"),
        ("cpp", "cpp"),
        ("hpp", "cpp"),
        ("cc", "cpp"),
        ("java", "java"),
        ("sh", "bash"),
        ("toml", "toml"),
        ("json", "json"),
        ("md", "markdown"),
    ];
    let ext = std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())?;
    EXT_MAP
        .iter()
        .find(|(k, _)| *k == ext)
        .map(|(_, v)| (*v).to_string())
}

/// Build the `KnowledgeUpsertFn` closure that registers committed artifacts into
/// `FileKnowledgeDB` after each successful atomic write in `PlanExecutor::commit()`.
///
/// Resolves the DB path via [`touring_foundation::TouringConfig::knowledge_db_canonical`]
/// using the `TOURING_PROJECT_ROOT` env var (falls back to `"."`). Returns `None`
/// when the DB cannot be opened — failure is logged as a warning and generation
/// continues unaffected.
fn build_knowledge_upsert_fn() -> Option<touring_generator::KnowledgeUpsertFn> {
    let project_root_str =
        std::env::var("TOURING_PROJECT_ROOT").unwrap_or_else(|_| ".".to_string());
    let db_path = touring_foundation::TouringConfig::knowledge_db_canonical(std::path::Path::new(
        &project_root_str,
    ));
    match touring_hooks::FileKnowledgeDB::new(&db_path) {
        Ok(db) => {
            let db = std::sync::Arc::new(std::sync::Mutex::new(db));
            Some(std::sync::Arc::new(move |path: &str, content: &[u8]| {
                let guard = db.lock().map_err(|e| format!("db lock poisoned: {e}"))?;
                use sha2::Digest as _;
                let content_hash = format!("{:x}", sha2::Sha256::digest(content));
                let line_count = content.iter().filter(|&&b| b == b'\n').count() as i64;
                let fk = touring_hooks::FileKnowledge {
                    file_path: path.to_string(),
                    content_hash: Some(content_hash),
                    line_count,
                    language: language_from_path(path),
                    ..Default::default()
                };
                guard
                    .upsert(&fk)
                    .map_err(|e| format!("knowledge upsert: {e}"))?;

                // R3-S4: Record generator commit in edit history so post_edit hook
                // analytics and decay-weighted error patterns include generated files.
                let _ = guard.record_edit(path, "Generated", Some("touring-generator plan commit"));

                // S-7: NLP enrichment for text/markdown artifacts — mirrors post_write.rs
                // analyze_text_async. Generator commits trigger the same NLP pipeline so
                // generated docs are immediately searchable via tantivy_related_docs_signal.
                let ext = std::path::Path::new(path)
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("");
                if matches!(ext, "md" | "txt" | "rst" | "adoc") {
                    if let Ok(content_str) = std::str::from_utf8(content) {
                        touring_hooks::nlp_bridge::analyze_text_async(path, content_str);

                        // R3-S5: Extract ANTT semantic keywords and append as knowledge note.
                        // Enables future memory recall to surface generator-produced docs by
                        // regulatory/technical category (Resolution, Law, TechnicalNote, etc.).
                        let kw = touring_hooks::extract_keywords(content_str);
                        if !kw.is_empty() {
                            let mut seen = std::collections::HashSet::new();
                            let cats: Vec<String> = kw
                                .iter()
                                .filter_map(|m| m.category.clone())
                                .filter(|c| seen.insert(c.clone()))
                                .collect();
                            if !cats.is_empty() {
                                let _ = guard.append_note(
                                    path,
                                    &format!("generator-nlp: {}", cats.join(",")),
                                );
                            }
                        }
                    }
                }

                Ok(())
            }))
        }
        Err(e) => {
            tracing::warn!(
                db_path = %db_path.display(),
                error = %e,
                "failed to open FileKnowledgeDB — knowledge_upsert_fn disabled"
            );
            None
        }
    }
}

/// Build a pheromone update closure that deposits template RL signals into a
/// per-context [`UnifiedPheromoneBus`].
///
/// Each generator typestate transition (verify, render, speculate, commit) calls
/// `ctx.pheromone_update(tool, score)` — previously a no-op when `pheromone_fn`
/// was `None`. This closure activates those 4 call sites by depositing the score
/// into a `PheroKey::TemplateId`-keyed trail for MCTS template selection feedback.
fn build_pheromone_fn() -> Option<touring_generator::PheromoneUpdateFn> {
    let bus = std::sync::Arc::new(std::sync::Mutex::new(UnifiedPheromoneBus::new(0.05)));
    Some(std::sync::Arc::new(
        move |tool: &str, score: touring_generator::NormalizedScore| {
            if let Ok(guard) = bus.lock() {
                guard.deposit(PheroKey::TemplateId(tool.to_owned()), score.value());
            }
        },
    ))
}

/// Build the production [`GeneratorContext`] with real fuzzy and RL providers.
///
/// Under `simd-fuzzy` feature: injects `BkTreeFuzzyAdapter` (Levenshtein search).
/// Under `rl-integration` feature: injects `LinUCBRewardSink` (OnlineRLEngine).
/// Under `mcts-synthesis` feature: injects `McctsEvalAdapter` (graph-informed MCTS).
/// Falls back to no-op providers when features are not active.
fn make_context() -> Arc<GeneratorContext> {
    #[cfg(feature = "simd-fuzzy")]
    let fuzzy: std::sync::Arc<dyn touring_generator::FuzzyMatcher> =
        std::sync::Arc::new(touring_generator::BkTreeFuzzyAdapter::new());
    #[cfg(not(feature = "simd-fuzzy"))]
    let fuzzy: std::sync::Arc<dyn touring_generator::FuzzyMatcher> =
        std::sync::Arc::new(touring_generator::NoopFuzzyMatcher);

    #[cfg(feature = "rl-integration")]
    let rl: std::sync::Arc<dyn touring_generator::RlRewardSink> =
        std::sync::Arc::new(touring_generator::LinUCBRewardSink::new());
    #[cfg(not(feature = "rl-integration"))]
    let rl: std::sync::Arc<dyn touring_generator::RlRewardSink> =
        std::sync::Arc::new(touring_generator::NoopRlSink);

    // Wiring gate: prefer CompositeWiringGate (syn + analysis) when both features
    // are active; fall back to SynWiringGateAdapter alone; then None.
    #[cfg(all(feature = "syn-quote", feature = "analysis-gate"))]
    let wiring_gate: Option<touring_generator::WiringGateFn> = {
        let project_root =
            std::env::var("TOURING_PROJECT_ROOT").unwrap_or_else(|_| ".".to_string());
        let db_path = touring_foundation::TouringConfig::knowledge_db_canonical(
            std::path::Path::new(&project_root),
        );
        // open_with_env honors TOURING_WIRING_GATE_{MIN_SCORE,MAX_DELTA,DISABLED}
        // env vars; defaults preserved when env vars unset.
        match touring_generator::CompositeWiringGate::open_with_env(&db_path) {
            Ok(gate) => Some(gate.into_closure()),
            Err(e) => {
                tracing::warn!(error = %e, "CompositeWiringGate failed — falling back to SynWiringGateAdapter");
                Some(touring_generator::SynWiringGateAdapter::new().into_closure())
            }
        }
    };
    #[cfg(all(feature = "syn-quote", not(feature = "analysis-gate")))]
    let wiring_gate: Option<touring_generator::WiringGateFn> =
        { Some(touring_generator::SynWiringGateAdapter::new().into_closure()) };
    #[cfg(not(feature = "syn-quote"))]
    let wiring_gate: Option<touring_generator::WiringGateFn> = None;

    // `ctx` is a GeneratorContext (NOT Arc) — inject all fields directly.
    // Previously used Arc::get_mut which only succeeds ONCE per Arc lifetime,
    // silently skipping all subsequent injections. Direct field assignment
    // ensures every closure is properly wired.
    // Note: mcts-synthesis feature requires Arc<GeneratorContext>, so we wrap
    // after initial field injection for that feature's inject_mcts_closure.
    #[allow(unused_mut)]
    let mut ctx = GeneratorContext::with_closures(
        fuzzy,
        rl,
        build_pheromone_fn(),
        wiring_gate,
        build_knowledge_upsert_fn(),
    );

    // Inject MCTS scoring closure when mcts-synthesis feature is active.
    #[cfg(feature = "mcts-synthesis")]
    {
        let ctx_arc = std::sync::Arc::new(ctx);
        ctx = std::sync::Arc::try_unwrap(inject_mcts_closure(ctx_arc))
            .unwrap_or_else(|arc| (*arc).clone());
    }

    // Inject SemanticGraphAdapter closures when cognitive-nexus feature is active.
    #[cfg(feature = "cognitive-nexus")]
    {
        let adapter = std::sync::Arc::new(touring_generator::SemanticGraphAdapter::new(
            std::path::PathBuf::from(
                std::env::var("TOURING_PROJECT_ROOT").unwrap_or_else(|_| "/tmp".to_string()),
            )
            .join(".claude/touring/cognitive_graph.json"),
        ));
        ctx.semantic_graph_fn = Some(std::sync::Arc::clone(&adapter).into_semantic_graph_fn());
        ctx.cognitive_nexus_fn = Some(std::sync::Arc::clone(&adapter).into_cognitive_nexus_fn());
        ctx.dspy_sig_fn = Some(build_dspy_closure());
    }

    // S-6: Inject NlpPlanRankerAdapter as cognitive_nexus_fn for keyword-based plan reranking.
    // When cognitive-nexus is also active, NlpPlanRanker overrides SemanticGraphAdapter for
    // cognitive_nexus_fn, providing complementary keyword-overlap scoring. Uses empty candidates
    // at init — populated lazily by plan recall pipeline.
    #[cfg(feature = "nlp-reranking")]
    {
        let ranker = touring_generator::NlpPlanRankerAdapter::new();
        ctx.cognitive_nexus_fn = Some(ranker.into_cognitive_nexus_fn(vec![]));
    }

    // Inject ConcolicPreToolAdapter when security-gate feature is active.
    // Wraps ConcolicExecutor into 3 focused closures for pre-tool hook integration.
    #[cfg(feature = "security-gate")]
    {
        use touring_offensive::concolic::ConcolicExecutor;
        let executor = std::sync::Arc::new(std::sync::Mutex::new(ConcolicExecutor::new()));
        let adapter = touring_generator::ConcolicPreToolAdapter::new(executor);
        ctx.concolic_analyze_fn = Some(adapter.analyze_fn());
        // S-5: Wire concolic analyzer via learn_reward for RL feedback
        let concolic_fn = adapter.analyze_fn();
        ctx.rl.inject(
            "concolic_analyze",
            touring_generator::NormalizedScore::clamped(0.1),
            "wiring_gate_validation",
        );
    }

    // Inject WasmSandboxAdapter when generator-wasm-sandbox feature is active.
    #[cfg(feature = "generator-wasm-sandbox")]
    {
        if let Ok(wasm) = touring_generator::WasmSandboxAdapter::with_default_wat() {
            ctx.wasm_sandbox_fn = Some(wasm.into_closure());
        }
    }

    // Inject TracingTelemetrySink + TracingAuditLog when observability feature is active.
    #[cfg(feature = "observability")]
    {
        ctx.telemetry = std::sync::Arc::new(touring_generator::TracingTelemetrySink::new());
        ctx.audit_log = std::sync::Arc::new(touring_generator::TracingAuditLog);
    }

    // Wire TouringMemoryProvider when memory-integration feature is active.
    // Enables real lesson/pattern persistence via `touring memory store/recall` subprocess.
    #[cfg(feature = "memory-integration")]
    {
        let project_root =
            std::env::var("TOURING_PROJECT_ROOT").unwrap_or_else(|_| ".".to_string());
        ctx.memory =
            std::sync::Arc::new(touring_generator::TouringMemoryProvider::new(project_root));
    }

    // Inject session lifecycle closures (P2) — wraps touring CLI subprocess calls.
    ctx.session_start_fn = Some(std::sync::Arc::new(|plan_id: &str, objective: &str| {
        let session_id = format!("gen-{}", plan_id.chars().take(20).collect::<String>());
        let output = std::process::Command::new("touring")
            .args(["session", "start", &session_id, "generator", objective])
            .output()
            .map_err(|e| format!("touring session start: {e}"))?;
        if output.status.success() {
            Ok(session_id)
        } else {
            Err(String::from_utf8_lossy(&output.stderr).to_string())
        }
    }));
    ctx.session_checkpoint_fn = Some(std::sync::Arc::new(|session_id: &str, data: &str| {
        let output = std::process::Command::new("touring")
            .args(["session", "checkpoint", session_id, data])
            .output()
            .map_err(|e| format!("touring session checkpoint: {e}"))?;
        if output.status.success() {
            Ok(())
        } else {
            Err(String::from_utf8_lossy(&output.stderr).to_string())
        }
    }));
    ctx.session_assess_fn = Some(std::sync::Arc::new(|session_id: &str| {
        let output = std::process::Command::new("touring")
            .args(["session", "assess", session_id])
            .output()
            .map_err(|e| format!("touring session assess: {e}"))?;
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            // Parse score from JSON output ({"score": 0.85, ...})
            serde_json::from_str::<serde_json::Value>(&stdout)
                .ok()
                .and_then(|v| v.get("score").and_then(|s| s.as_f64()))
                .ok_or_else(|| "failed to parse session assess score".to_string())
        } else {
            Err(String::from_utf8_lossy(&output.stderr).to_string())
        }
    }));

    // Inject decompose bridge closures (P3) — wraps touring CLI subprocess calls.
    // S-4: After successful decompose create, also emit task-created hook for
    // ACO pheromone deposit + knowledge DB recording via run_task_created path.
    ctx.decompose_create_fn = Some(std::sync::Arc::new(|task_type: &str, description: &str| {
        let output = std::process::Command::new("touring")
            .args(["decompose", "create", task_type, description])
            .output()
            .map_err(|e| format!("touring decompose create: {e}"))?;
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let task_id = serde_json::from_str::<serde_json::Value>(&stdout)
                .ok()
                .and_then(|v| v.get("task_id").and_then(|s| s.as_str().map(String::from)))
                .ok_or_else(|| "failed to parse task_id".to_string())?;
            // Emit task-created hook: routes to ACO pheromone + knowledge DB analytics.
            inject_task_created_hook(&task_id, &format!("{task_type}:{description}"));
            Ok(task_id)
        } else {
            Err(String::from_utf8_lossy(&output.stderr).to_string())
        }
    }));
    ctx.decompose_update_fn = Some(std::sync::Arc::new(
        |task_id: &str, subtask_id: &str, status: &str| {
            let output = std::process::Command::new("touring")
                .args([
                    "decompose",
                    "update",
                    &format!("{}/{}", task_id, subtask_id),
                    status,
                ])
                .output()
                .map_err(|e| format!("touring decompose update: {e}"))?;
            if output.status.success() {
                Ok(())
            } else {
                Err(String::from_utf8_lossy(&output.stderr).to_string())
            }
        },
    ));
    // Inject quality gate + health gate adapters (PLN2 — feature quality-gate / health-gate).
    #[cfg(feature = "quality-gate")]
    {
        let config = touring_analysis::engine::AnalysisConfig {
            quality_sample: usize::MAX,
            ..Default::default()
        };
        let adapter = touring_generator::QualityGateAdapter::new(config);
        ctx.quality_gate_adapter = Some(adapter.clone());
        ctx.quality_gate_fn = Some(adapter.into_closure());
    }
    #[cfg(feature = "health-gate")]
    {
        let adapter = touring_generator::HealthGateAdapter::with_thresholds(0.7, 0.3);
        ctx.health_gate_fn = Some(adapter.into_closure());
    }
    // Inject enrichment trigger (PLN3 — feature enrichment-gate).
    // Fires `touring post-write` for each artifact, triggering the full daemon
    // enrichment pipeline (Tantivy FTS, gotcha, wiring, knowledge).
    #[cfg(feature = "enrichment-gate")]
    {
        let trigger: touring_generator::EnrichmentTriggerFn =
            std::sync::Arc::new(move |paths: &[String], _project_root: &str| {
                for path in paths {
                    let path = path.to_string();
                    // Fire-and-forget: spawn each post-write call in the background.
                    tokio::spawn(async move {
                        let payload = serde_json::json!({
                            "tool_input": { "file_path": path },
                            "tool_use_result": { "is_error": false },
                        });
                        let mut child = tokio::process::Command::new("touring")
                            .arg("post-write")
                            .stdin(std::process::Stdio::piped())
                            .stdout(std::process::Stdio::null())
                            .stderr(std::process::Stdio::null())
                            .spawn();
                        if let Ok(ref mut c) = child {
                            if let Some(mut stdin) = c.stdin.take() {
                                use tokio::io::AsyncWriteExt;
                                let _ = stdin.write_all(payload.to_string().as_bytes()).await;
                            }
                            let _ = c.wait().await;
                        }
                    });
                }
            });
        ctx.enrichment_trigger_fn = Some(trigger);
    }

    std::sync::Arc::new(ctx)
}

/// Build a DSPy signature evaluation closure bridging `touring_cortex::DspyModule`
/// to the generator's `DspySigFn` interface.
///
/// Adapts between the type systems:
/// - Input: `HashMap<String, serde_json::Value>` → `HashMap<String, String>`
/// - Output: `ModuleResult.outputs: HashMap<String, String>` → `HashMap<String, serde_json::Value>`
#[cfg(feature = "cognitive-nexus")]
fn build_dspy_closure() -> touring_generator::DspySigFn {
    use touring_cortex::{DspyModule, code_generation_sig};
    let module = std::sync::Arc::new(DspyModule::new(code_generation_sig()));
    std::sync::Arc::new(
        move |_sig_name: &touring_generator::DspySignatureName,
              inputs: &touring_generator::DspyInputs|
              -> touring_generator::DspyOutputs {
            let str_inputs: HashMap<String, String> = inputs
                .iter()
                .map(|(k, v)| {
                    let s = v
                        .as_str()
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| v.to_string());
                    (k.clone(), s)
                })
                .collect();
            let result = module.forward(&str_inputs);
            result
                .outputs
                .into_iter()
                .map(|(k, v)| (k, Value::String(v)))
                .collect()
        },
    )
}

/// Inject a `McctsEvalAdapter` closure into `ctx` (only compiled under `mcts-synthesis`).
///
/// Builds a `SemanticGraph` backed by `mcts_graph.json` alongside the knowledge DB.
/// No disk I/O occurs until `graph.save()` is called; construction is infallible.
#[cfg(feature = "mcts-synthesis")]
fn inject_mcts_closure(ctx: Arc<GeneratorContext>) -> Arc<GeneratorContext> {
    let project_root_str =
        std::env::var("TOURING_PROJECT_ROOT").unwrap_or_else(|_| ".".to_string());
    let graph_path = touring_foundation::TouringConfig::knowledge_db_canonical(
        std::path::Path::new(&project_root_str),
    )
    .with_file_name("mcts_graph.json");
    let persistence = std::sync::Arc::new(touring_intelligence::reasoning::GraphPersistence::new(
        graph_path,
    ));
    let graph = std::sync::Arc::new(
        touring_intelligence::reasoning::semantic_graph::SemanticGraph::new(persistence),
    );
    let adapter = touring_generator::McctsEvalAdapter::with_graph(graph);
    ctx.with_mcts_eval(adapter.into_closure())
}

// ── submit / commit ─────────────────────────────────────────────────────────

/// `touring_generator_submit_plan` — run the full generator pipeline.
///
/// Deserializes the plan JSON, runs VGP → render → speculate → commit.
/// If `dry_run` is true the commit step is skipped (pipeline stops at Rendered).
pub async fn submit_plan(plan_json: &str, dry_run: bool) -> Value {
    let mut plan = match parse_plan(plan_json) {
        Ok(p) => p,
        Err(e) => return serde_json::json!({"ok": false, "error": e.to_string()}),
    };
    // R5-S2: Auto-populate contracts when empty using Tantivy symbol discovery.
    // If the LLM submits a plan with no symbols_must_exist, derive candidates
    // from the target file path (e.g. "generator_tools.rs" → query "generator tools")
    // and fill contracts so VGP has real symbols to verify. This removes the friction
    // of manual contract authorship for routine generation tasks.
    auto_populate_contracts(&mut plan);
    // R6-S2: Pre-flight structural gate — fail fast before VGP if plan has fatal errors.
    // Uses `collect_plan_errors` (same engine as validate_plan) to catch: empty intent,
    // over-length intent, empty target path, empty version. Returns actionable errors
    // immediately rather than letting the pipeline discover them mid-VGP. Only blocks
    // on structural errors — warnings and info-level critique proceed normally.
    if let Err(fatal) = preflight_gate(&plan) {
        return serde_json::json!({
            "ok": false,
            "stage": "pre_flight",
            "errors": fatal,
            "note": "Fix structural errors before submitting. Run validate_plan for full diagnostics.",
        });
    }
    run_pipeline(plan, dry_run, 0).await
}

/// Structural pre-flight check extracted for CC reduction.
///
/// Returns `Ok(())` when the plan is safe to enter the pipeline; `Err(errors)` with
/// one entry per fatal structural violation detected by `collect_plan_errors`.
fn preflight_gate(plan: &GeneratorPlan) -> Result<(), Vec<String>> {
    let mut errors: Vec<String> = Vec::new();
    collect_plan_errors(plan, &mut errors);
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// Auto-populate `contracts.symbols_must_exist` when the plan has none.
///
/// Extracts module keywords from the target file path (stem segments split on `_`),
/// runs a Tantivy BM25 search, and fills up to 5 `SymbolRef` entries. Silent on
/// failure — if Tantivy is unavailable the plan proceeds with empty contracts.
fn auto_populate_contracts(plan: &mut GeneratorPlan) {
    if !plan.contracts.symbols_must_exist.is_empty() {
        return; // already populated — respect LLM's explicit contracts
    }
    // Derive a search query from the target file stem.
    // "src/tools/generator_tools.rs" → "generator tools"
    let stem = std::path::Path::new(&plan.target.file_path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .replace('_', " ");
    if stem.trim().is_empty() || stem.len() < 3 {
        return;
    }
    let hits = tantivy_search_symbols(&stem, 5);
    let syms: Vec<touring_generator::plan::contracts::SymbolRef> = hits
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|h| {
            let name = h.get("symbol_name")?.as_str()?.to_string();
            if name.is_empty() {
                return None;
            }
            Some(touring_generator::plan::contracts::SymbolRef::named(name))
        })
        .collect();
    if !syms.is_empty() {
        plan.contracts.symbols_must_exist = syms;
    }
}

/// Internal pipeline runner — shared by submit and replay.
async fn run_pipeline(plan: GeneratorPlan, dry_run: bool, iteration: u8) -> Value {
    let ctx = make_context();
    let executor = if iteration == 0 {
        PlanExecutor::first(plan, Arc::clone(&ctx))
    } else {
        PlanExecutor::new(plan, Arc::clone(&ctx), iteration)
    };

    // Stage 1: VGP verify
    let verified = match executor.verify(ctx.vgp_engine.as_ref()).await {
        Ok(v) => v,
        Err(replan) => return replan_json("verify", &replan),
    };

    // Stage 2: Template render (plan carries its own extra_vars via template.extra_vars)
    let rendered = match verified.render(
        ctx.template_engine.as_ref(),
        &HashMap::new(),
        None,
        RenderShape::default_width(),
    ) {
        Ok(Some(r)) => r,
        Ok(None) => return stage_error("render", "shape overflow (content too wide)".to_string()),
        Err(e) => return stage_error("render", e.to_string()),
    };

    if dry_run {
        return serde_json::json!({
            "ok": true, "stage": "rendered", "dry_run": true,
            "note": "commit skipped (dry_run=true)",
        });
    }

    speculate_and_commit(rendered, ctx).await
}

/// Stage 3+4: speculate then commit (extracted to reduce CC of `run_pipeline`).
async fn speculate_and_commit(
    rendered: PlanExecutor<Rendered>,
    ctx: Arc<GeneratorContext>,
) -> Value {
    let speculated = match rendered.speculate(ctx.speculate_bridge.as_ref()).await {
        Ok(s) => s,
        Err(replan) => {
            // R4-S1: Speculate failure → negative RL signal. Closes the feedback loop
            // that previously only rewarded success. Penalty -0.5 (shadow-validate
            // rejection is a stronger learning signal than a commit gate failure).
            inject_daemon_rl_reward(
                "generate",
                -0.5,
                &format!("speculate_failed:{}", replan.plan_id()),
            );
            return replan_json("speculate", &replan);
        }
    };
    match speculated.commit().await {
        Ok(completed) => {
            // S-2: Propagate commit success to daemon RL engine (fire-and-forget).
            inject_daemon_rl_reward(
                "generate",
                1.0,
                &format!("commit_success:{}", completed.plan_id),
            );

            // S-3: Emit task-metrics hook for generator commit analytics (fire-and-forget).
            inject_task_metrics_hook(
                &completed.plan_id.to_string(),
                completed.commit_report.elapsed_ms,
                completed.commit_report.files_written.len(),
                1.0_f64,
            );

            // S-5: Emit task-validation hook for DAG integrity visibility (fire-and-forget).
            inject_task_validation_hook(
                &completed.plan_id.to_string(),
                completed.commit_report.files_written.len(),
            );

            // R11-B: Fire task-completed cortex event to close the generator lifecycle loop.
            // generator creates plan  -> inject_task_created_hook  (TaskCreated event)
            // generator commits plan  -> inject_task_completed_hook (TaskCompleted event)
            // TaskCompleted handler mirrors the completed status to the decompose DAG
            // and triggers decompose finalize + RL reward 1.0 automatically.
            inject_task_completed_hook(
                &completed.plan_id.to_string(),
                &format!(
                    "{} file(s) committed",
                    completed.commit_report.files_written.len()
                ),
            );

            // S-4: Trigger symbol index rebuild for committed files (fire-and-forget).
            {
                let dirs: std::collections::HashSet<String> = completed
                    .commit_report
                    .files_written
                    .iter()
                    .filter_map(|a| {
                        std::path::Path::new(&a.path)
                            .parent()
                            .map(|p| p.to_string_lossy().into_owned())
                    })
                    .collect();
                for dir in dirs {
                    tokio::spawn(async move {
                        let _ = std::process::Command::new("touring")
                            .args(["index", "rebuild", "--dir", &dir])
                            .output();
                    });
                }
            }

            // R5-S3: Post-commit consumer plan suggestions — surface orphan wiring
            // opportunities after generation so Claude Code knows what to wire next.
            // Only invoked when at least one Rust file was committed (avoids
            // subprocess overhead for template-only / config commits). Limit=3 to
            // keep response size bounded. Failures are silent (returns null).
            let consumer_plans_suggested = {
                let has_rust = completed
                    .commit_report
                    .files_written
                    .iter()
                    .any(|a| a.path.ends_with(".rs"));
                if has_rust {
                    build_consumer_generator_plans(3)
                } else {
                    Value::Null
                }
            };

            // R9-S3: Emit task-complete hint so Claude Code can close its task loop.
            // When a generator plan commits successfully, the Claude Code task that
            // triggered the generation should be marked complete in both Claude Code
            // and the Touring decompose DAG. Surface the CLI hint in the response.
            let task_complete_hint = format!(
                "plan committed — if this completes a task, run `touring decompose finalize <task_id>` \
                and `touring learning reward orchestrate 1.0 \"plan:{}:committed\"`",
                completed.plan_id
            );

            serde_json::json!({
                "ok": true,
                "stage": "committed",
                "plan_id": completed.plan_id.to_string(),
                "files_written_count": completed.commit_report.files_written.len(),
                "files_written": completed.commit_report.files_written.iter().map(|a| serde_json::json!({
                    "path": a.path,
                    "sha256": a.sha256,
                    "bytes_written": a.bytes_written,
                    "action": format!("{:?}", a.action),
                    "backup_path": a.backup_path,
                })).collect::<Vec<_>>(),
                "elapsed_ms": completed.commit_report.elapsed_ms,
                "consumer_plans_suggested": consumer_plans_suggested,
                "task_complete_hint": task_complete_hint,
            })
        }
        Err(e) => {
            // R4-S2: Commit failure → negative RL signal. Wiring/quality gate rejections
            // and atomic-write failures now produce a learning signal. Penalty -0.3
            // (lighter than speculate since commit failures may be transient I/O issues).
            inject_daemon_rl_reward("generate", -0.3, "commit_failed");
            stage_error("commit", e.to_string())
        }
    }
}

/// Fire-and-forget task-created hook after generator plan decompose creation.
///
/// Routes generator task creation to the touring-hooks lifecycle system via
/// `touring cortex task-completed` (HookSilent — always exits 0). Payload is
/// piped via stdin in the `{"task_id", "status": "CREATED", ...}` format that
/// TaskCompletedHandler records to the knowledge DB.
/// Mirrors the team-hooks `run_task_created` path without a direct Rust dependency.
fn inject_task_created_hook(task_id: &str, subject: &str) {
    let task_id = task_id.to_owned();
    let subject = subject.to_owned();
    tokio::spawn(async move {
        let payload = serde_json::json!({
            "task_id": task_id,
            "task_subject": subject,
            "session_id": format!("gen-{}", &task_id.chars().take(20).collect::<String>()),
            "teammate_name": "touring-generator",
            "team_name": "taco-generator",
            "hook_event_name": "TaskCreated",
            "status": "CREATED",
        })
        .to_string();
        emit_cortex_event(&task_id, "task-created", &payload, "task-created hook");
    });
}

/// R11-B: Fire-and-forget task-completed cortex event after generator plan commit.
///
/// Closes the generator lifecycle loop: generator creates plan (inject_task_created_hook)
/// → generator commits plan (inject_task_completed_hook). The task-completed handler
/// in touring-hooks mirrors the status to the decompose DAG and fires RL reward 1.0.
fn inject_task_completed_hook(task_id: &str, outcome: &str) {
    let task_id = task_id.to_owned();
    let outcome = outcome.to_owned();
    tokio::spawn(async move {
        let payload = serde_json::json!({
            "task_id": task_id,
            "outcome": outcome,
            "session_id": format!("gen-{}", &task_id.chars().take(20).collect::<String>()),
            "teammate_name": "touring-generator",
            "team_name": "taco-generator",
            "hook_event_name": "TaskCompleted",
            "status": "COMPLETED",
        })
        .to_string();
        emit_cortex_event(&task_id, "task-completed", &payload, "task-completed hook");
    });
}

/// Fire-and-forget task-metrics hook emission after successful generator commit.
///
/// Routes commit analytics to the touring-hooks task lifecycle system via
/// `touring cortex task-completed` (HookSilent — always exits 0). Payload
/// carries elapsed_ms, files_count, and quality_score for knowledge DB analytics.
fn inject_task_metrics_hook(
    plan_id: &str,
    elapsed_ms: u64,
    files_count: usize,
    quality_score: f64,
) {
    let plan_id = plan_id.to_owned();
    tokio::spawn(async move {
        let payload = serde_json::json!({
            "task_id": plan_id,
            "completion_time": elapsed_ms as f64 / 1000.0,
            "subtask_count": files_count as i64,
            "success_rate": 1.0_f64,
            "quality_score": quality_score,
            "status": "METRICS",
        })
        .to_string();
        emit_cortex_event(&plan_id, "task-metrics", &payload, "task-metrics hook");
    });
}

/// Fire-and-forget task-validation hook for generator plan DAG integrity visibility.
///
/// Routes generator plan validation events to the touring-hooks lifecycle system via
/// `touring cortex task-completed` (HookSilent — always exits 0). The `status: VALIDATED`
/// field distinguishes validation events from commit events in the knowledge DB.
fn inject_task_validation_hook(plan_id: &str, contracts_count: usize) {
    let plan_id = plan_id.to_owned();
    tokio::spawn(async move {
        let payload = serde_json::json!({
            "task_id": plan_id,
            "subtask_count": contracts_count as i64,
            "status": "VALIDATED",
        })
        .to_string();
        emit_cortex_event(
            &plan_id,
            "task-validation",
            &payload,
            "task-validation hook",
        );
    });
}

/// Emit a `touring cortex task-completed` event by piping `payload` JSON to stdin.
///
/// `touring cortex` uses `ErrorPolicy::HookSilent` — it ALWAYS exits 0 even when
/// the daemon is unavailable or the payload is malformed. This makes it safe for all
/// fire-and-forget hooks in the generator pipeline. `event_label` is used only for
/// tracing; `id` is the task/plan identifier logged on success or failure.
fn emit_cortex_event(id: &str, event_label: &str, payload: &str, log_tag: &str) {
    use std::io::Write as _;
    use std::process::Stdio;

    let mut child = match std::process::Command::new("touring")
        .args(["cortex", "task-completed"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            tracing::debug!(id = %id, event = %event_label, error = %e, "{log_tag} spawn failed");
            return;
        }
    };

    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(payload.as_bytes());
    }

    match child.wait() {
        Ok(status) if status.success() => {
            tracing::debug!(id = %id, event = %event_label, "{log_tag} emitted");
        }
        Ok(_) => {
            // HookSilent policy means cortex always exits 0; log at debug if not.
            tracing::debug!(id = %id, event = %event_label, "{log_tag} non-zero exit (daemon unavailable)");
        }
        Err(e) => {
            tracing::debug!(id = %id, event = %event_label, error = %e, "{log_tag} wait failed");
        }
    }
}

/// Fire-and-forget RL reward injection to the touring daemon.
///
/// Propagates generator commit success signals to the global daemon RL engine,
/// complementing the per-context `LinUCBRewardSink` rewards in typestate.rs.
/// Spawned via `tokio::spawn` — never blocks the MCP response.
fn inject_daemon_rl_reward(tool: &str, reward: f64, context: &str) {
    let tool = tool.to_owned();
    let context = context.to_owned();
    tokio::spawn(async move {
        let reward_str = format!("{reward:.1}");
        match std::process::Command::new("touring")
            .args(["learning", "reward", &tool, &reward_str, &context])
            .output()
        {
            Ok(output) if output.status.success() => {
                tracing::debug!(tool = %tool, reward = %reward_str, "daemon RL reward injected");
            }
            Ok(output) => {
                tracing::debug!(
                    tool = %tool,
                    stderr = %String::from_utf8_lossy(&output.stderr),
                    "daemon RL reward injection failed (non-zero exit)"
                );
            }
            Err(e) => {
                tracing::debug!(tool = %tool, error = %e, "daemon RL reward injection failed");
            }
        }
    });
}

/// Format a pipeline stage error as JSON.
fn stage_error(stage: &str, error: String) -> Value {
    serde_json::json!({"ok": false, "stage": stage, "error": error})
}

/// Build a structured JSON value from a [`ReplanRequest`], exposing all accessible fields.
///
/// Replaces opaque `format!("{:?}", replan.reason())` strings with a machine-readable
/// payload that MCP callers can inspect to drive retry/escalation logic.
fn replan_json(stage: &str, r: &ReplanRequest) -> Value {
    serde_json::json!({
        "ok": false,
        "stage": stage,
        "type": "replan_request",
        "plan_id": r.plan_id().to_string(),
        "iteration": r.iteration(),
        "reason": format!("{:?}", r.reason()),
    })
}

// ── validate ────────────────────────────────────────────────────────────────

/// `touring_generator_validate_plan` — validate JSON schema without executing.
///
/// Deserializes the plan and checks required field invariants. Returns a list
/// of validation errors (empty = valid).
pub fn validate_plan(plan_json: &str) -> Value {
    let plan: GeneratorPlan = match parse_plan(plan_json) {
        Ok(p) => p,
        Err(e) => return serde_json::json!({"valid": false, "errors": [e.to_string()]}),
    };

    let mut errors: Vec<String> = Vec::new();
    collect_plan_errors(&plan, &mut errors);

    // R5-S1: Tantivy pre-check for symbols_must_exist — confirms existence before VGP.
    let contract_hints = check_contracts_in_tantivy(&plan.contracts.symbols_must_exist);

    // R6-S4: Tantivy collision check for symbols_must_not_exist — warns when a symbol
    // the plan asserts must NOT exist is actually FOUND in the live index. A hit means
    // the generation may collide with an existing symbol, causing a VGP hard-block.
    // Returns [{name, collision_risk, top_file}] — empty array when list is empty.
    let collision_check: Vec<Value> = plan
        .contracts
        .symbols_must_not_exist
        .iter()
        .map(|sym| {
            let hits = tantivy_search_symbols(&sym.name, 1);
            let found = hits.as_array().map(|a| !a.is_empty()).unwrap_or(false);
            let top_file = hits
                .as_array()
                .and_then(|a| a.first())
                .and_then(|h| h.get("file_path"))
                .cloned()
                .unwrap_or(Value::Null);
            serde_json::json!({
                "name": sym.name,
                "collision_risk": found,
                "top_file": top_file,
            })
        })
        .collect();

    serde_json::json!({
        "valid": errors.is_empty(),
        "errors": errors,
        "plan_id": plan.plan_id.to_string(),
        "kind": format!("{:?}", plan.kind),
        "intent_len": plan.intent.len(),
        "contract_hints": contract_hints,
        "collision_check": collision_check,
    })
}

/// Check each SymbolRef against the Tantivy BM25 index for pre-validation.
///
/// Returns an array of `{name, found_in_index, top_file, functional_signature}` entries —
/// one per `symbols_must_exist` entry. Symbols not found in Tantivy are likely invented
/// and should be removed from contracts before VGP verification.
///
/// Suggestion 4 (2026-04-20): also surfaces `functional_signature` from the Tantivy index
/// so callers can validate type contracts (not just symbol existence) before submitting a plan.
/// When the plan's expected signature and the indexed signature diverge, the LLM should
/// update its contracts to match the actual API before proceeding to VGP.
///
/// Uses `tantivy_search_symbols` with limit=1 per symbol for minimal latency.
fn check_contracts_in_tantivy(symbols: &[touring_generator::plan::contracts::SymbolRef]) -> Value {
    if symbols.is_empty() {
        return Value::Array(Vec::new());
    }
    let hints: Vec<Value> = symbols
        .iter()
        .map(|sym| {
            let hits = tantivy_search_symbols(&sym.name, 1);
            let top_hit = hits.as_array().and_then(|arr| arr.first()).cloned();
            let found = top_hit.is_some();
            let top_file = top_hit
                .as_ref()
                .and_then(|h| h.get("file_path"))
                .cloned()
                .unwrap_or(Value::Null);
            // Suggestion 4: expose the indexed functional_signature so the caller can
            // detect type-contract mismatches before VGP hard-blocks on wrong signatures.
            let functional_signature = top_hit
                .as_ref()
                .and_then(|h| h.get("functional_signature"))
                .cloned()
                .unwrap_or(Value::Null);
            serde_json::json!({
                "name": sym.name,
                "found_in_index": found,
                "top_file": top_file,
                "functional_signature": functional_signature,
            })
        })
        .collect();
    Value::Array(hints)
}

/// Collect all validation errors into `errors` (extracted for CC reduction).
fn collect_plan_errors(plan: &GeneratorPlan, errors: &mut Vec<String>) {
    if plan.intent.trim().is_empty() {
        errors.push("intent is empty".into());
    }
    if plan.intent.len() > 4096 {
        errors.push(format!(
            "intent too long ({} chars, max 4096)",
            plan.intent.len()
        ));
    }
    if plan.target.file_path.is_empty() {
        errors.push("target.file_path is empty".into());
    }
    if plan.version.is_empty() {
        errors.push("version is empty".into());
    }
}

// ── verify ──────────────────────────────────────────────────────────────────

/// `touring_generator_verify_plan` — run VGP symbol verification only.
pub async fn verify_plan(plan_json: &str) -> Value {
    let plan = match parse_plan(plan_json) {
        Ok(p) => p,
        Err(e) => return serde_json::json!({"ok": false, "error": e.to_string()}),
    };

    let ctx = make_context();
    let executor = PlanExecutor::first(plan, Arc::clone(&ctx));

    match executor.verify(ctx.vgp_engine.as_ref()).await {
        Ok(_) => serde_json::json!({"ok": true, "stage": "verified"}),
        Err(replan) => replan_json("verify", &replan),
    }
}

// ── render ──────────────────────────────────────────────────────────────────

/// `touring_generator_render_plan` — VGP verify + template render (no speculate/commit).
pub async fn render_plan(plan_json: &str) -> Value {
    let plan = match parse_plan(plan_json) {
        Ok(p) => p,
        Err(e) => return serde_json::json!({"ok": false, "error": e.to_string()}),
    };

    let ctx = make_context();
    let executor = PlanExecutor::first(plan, Arc::clone(&ctx));

    let verified = match executor.verify(ctx.vgp_engine.as_ref()).await {
        Ok(v) => v,
        Err(replan) => return replan_json("verify", &replan),
    };

    match verified.render(
        ctx.template_engine.as_ref(),
        &HashMap::new(),
        None,
        RenderShape::default_width(),
    ) {
        Ok(Some(_)) => serde_json::json!({"ok": true, "stage": "rendered"}),
        Ok(None) => stage_error("render", "shape overflow (content too wide)".to_string()),
        Err(e) => stage_error("render", e.to_string()),
    }
}

// ── speculate ───────────────────────────────────────────────────────────────

/// `touring_generator_speculate_plan` — verify + render + speculate (no commit).
pub async fn speculate_plan(plan_json: &str) -> Value {
    let plan = match parse_plan(plan_json) {
        Ok(p) => p,
        Err(e) => return serde_json::json!({"ok": false, "error": e.to_string()}),
    };

    let ctx = make_context();
    let executor = PlanExecutor::first(plan, Arc::clone(&ctx));

    let verified = match executor.verify(ctx.vgp_engine.as_ref()).await {
        Ok(v) => v,
        Err(replan) => return replan_json("verify", &replan),
    };

    let rendered = match verified.render(
        ctx.template_engine.as_ref(),
        &HashMap::new(),
        None,
        RenderShape::default_width(),
    ) {
        Ok(Some(r)) => r,
        Ok(None) => return stage_error("render", "shape overflow (content too wide)".to_string()),
        Err(e) => return stage_error("render", e.to_string()),
    };

    match rendered.speculate(ctx.speculate_bridge.as_ref()).await {
        // R10-B: Enrich speculate response with next_step_hint so Claude Code
        // knows to call commit immediately after successful speculate, completing
        // the verify->render->speculate->commit pipeline without interruption.
        Ok(_) => serde_json::json!({
            "ok": true,
            "stage": "speculated",
            "next_step_hint": "call touring_generator_commit_plan(plan_json) to commit artifacts | run `touring generate plan-commit --plan-file <path>` from CLI"
        }),
        Err(replan) => replan_json("speculate", &replan),
    }
}

// ── rollback ────────────────────────────────────────────────────────────────

/// `touring_generator_rollback_plan` — report rollback availability.
///
/// Checks whether the plan's target file has a `.bak` backup on disk.
/// Reports what would happen — does not execute the restore.
pub fn rollback_plan(plan_json: &str) -> Value {
    let plan = match parse_plan(plan_json) {
        Ok(p) => p,
        Err(e) => return serde_json::json!({"ok": false, "error": e.to_string()}),
    };

    let target = &plan.target.file_path;
    let backup_path = format!("{target}.bak");
    let target_exists = std::path::Path::new(target).exists();
    let backup_exists = std::path::Path::new(&backup_path).exists();

    if backup_exists {
        serde_json::json!({
            "ok": true, "rollback_available": true,
            "target": target, "backup": backup_path,
            "note": "Run `cp <backup> <target>` to restore. This tool only reports.",
        })
    } else {
        let reason = if target_exists {
            "no backup file found at <target>.bak"
        } else {
            "target file does not exist"
        };
        serde_json::json!({
            "ok": false, "rollback_available": false,
            "target": target, "reason": reason,
        })
    }
}

// ── query_target_knowledge (shared helper) ────────────────────────────────────

/// Query `FileKnowledgeEnriched` for the plan target path.
///
/// Returns a JSON object with `coverage_pct`, `community_id`, `integration_score`,
/// `fan_in`, `fan_out`, and `cognitive_score` extracted from the enrichment tables.
/// Returns [`Value::Null`] when the DB is unavailable or the target has no record.
/// Used by `plan_status` (R4-S5) to surface file-level health signals before pipeline
/// execution, complementing the pre-commit critique used by `collect_intelligence_critique`.
fn query_target_knowledge(target_path: &str) -> Value {
    if target_path.is_empty() {
        return Value::Null;
    }
    let project_root = std::env::var("TOURING_PROJECT_ROOT").unwrap_or_else(|_| ".".to_string());
    let db_path = touring_foundation::TouringConfig::knowledge_db_canonical(std::path::Path::new(
        &project_root,
    ));
    let Ok(db) = touring_hooks::FileKnowledgeDB::new(&db_path) else {
        return Value::Null;
    };
    match db.query_extended(target_path) {
        Ok(Some(ext)) => serde_json::json!({
            "coverage_pct": ext.coverage_pct,
            "community_id": ext.community_id,
            "integration_score": ext.integration_score,
            "fan_in": ext.fan_in_signal,
            "fan_out": ext.fan_out_signal,
            "cognitive_score": ext.cognitive_score,
        }),
        _ => Value::Null,
    }
}

// ── status ──────────────────────────────────────────────────────────────────

/// `touring_generator_plan_status` — show plan metadata and validation status.
pub fn plan_status(plan_json: &str) -> Value {
    match parse_plan(plan_json) {
        Ok(plan) => {
            let validation = validate_plan(plan_json);
            // R4-S5: FileKnowledgeEnriched enrichment (coverage_pct, community_id, etc.)
            let file_knowledge = query_target_knowledge(plan.target.file_path.as_str());
            // R5-S5: Tantivy contract verification — check each symbols_must_exist entry
            // against the live BM25 index. Returns [{name, found_in_index, top_file}]
            // so the caller sees which contracts are backed by real indexed symbols vs.
            // potentially invented names. Complements R5-S1 (validate_plan pre-check)
            // by surfacing live contract state at status-query time (not only on validate).
            let contract_verification =
                check_contracts_in_tantivy(&plan.contracts.symbols_must_exist);
            serde_json::json!({
                "ok": true,
                "plan_id": plan.plan_id.to_string(),
                "version": plan.version,
                "kind": format!("{:?}", plan.kind),
                "intent": plan.intent,
                "target": plan.target.file_path,
                "cila_level": format!("{:?}", plan.cila_level),
                "trace_entries": plan.execution_trace.len(),
                "validation": validation,
                "file_knowledge": file_knowledge,
                "contract_verification": contract_verification,
            })
        }
        Err(e) => serde_json::json!({"ok": false, "error": e.to_string()}),
    }
}

// ── schema_dump ──────────────────────────────────────────────────────────────

/// `touring_generator_schema_dump` — emit JSON Schema for `GeneratorPlan`.
pub fn schema_dump() -> Value {
    let schema = schemars::schema_for!(GeneratorPlan);
    match serde_json::to_value(&schema) {
        Ok(v) => serde_json::json!({"ok": true, "version": "v1.0", "schema": v}),
        Err(e) => serde_json::json!({"ok": false, "error": e.to_string()}),
    }
}

/// `touring_generator_schema_check` — verify a plan version is compatible with the engine.
///
/// Uses the live `SchemaRegistry` from a freshly built `GeneratorContext` to check
/// whether plans of the requested `version` can be deserialized and migrated.
/// Returns `{ok, compatible, engine_version, migrations_available}` so callers can
/// decide whether to upgrade or run a migration before submitting.
pub fn schema_check(version: &str) -> Value {
    let ctx = make_context();
    let compatible = ctx.schema_registry.is_compatible(version);
    let engine_version = ctx.schema_registry.engine_version.clone();
    let migrations_available: Vec<String> =
        ctx.schema_registry.migrations.keys().cloned().collect();
    serde_json::json!({
        "ok": true,
        "compatible": compatible,
        "engine_version": engine_version,
        "requested_version": version,
        "migrations_available": migrations_available,
    })
}

/// `touring_generator_bundle` — execute multiple plans sequentially as a bundle.
///
/// Each plan is executed independently via `submit_plan`. Stops at the first failure
/// when `dry_run=false`; in `dry_run=true` mode runs all plans through `render` only.
/// Returns a manifest with per-plan stage outcomes for full traceability.
pub async fn bundle(plans_json: &[String], dry_run: bool) -> Value {
    let mut results: Vec<Value> = Vec::with_capacity(plans_json.len());
    let mut succeeded: usize = 0;
    let mut failed: usize = 0;

    for (idx, plan_json) in plans_json.iter().enumerate() {
        let result = submit_plan(plan_json, dry_run).await;
        let ok = result.get("ok").and_then(Value::as_bool).unwrap_or(false);
        if ok {
            succeeded += 1;
        } else {
            failed += 1;
        }
        results.push(serde_json::json!({
            "index": idx,
            "result": result,
        }));
        // Fail-fast in non-dry-run mode: a real commit failure halts the bundle
        // to avoid leaving the workspace in a partially-committed state.
        if !ok && !dry_run {
            break;
        }
    }

    serde_json::json!({
        "ok": failed == 0,
        "total": plans_json.len(),
        "succeeded": succeeded,
        "failed": failed,
        "dry_run": dry_run,
        "results": results,
    })
}

// ── tantivy_search_symbols (shared helper) ────────────────────────────────────

/// Query Tantivy full-text search for symbols matching the given query string.
///
/// Returns a JSON array of `{symbol_name, file_path, crate_name}` objects
/// (top `limit` hits, sorted by BM25 relevance) via `touring tantivy search`.
/// Returns [`Value::Null`] when Tantivy is unavailable or the subprocess fails.
/// Used by `recall_similar` (R4-S3) and `suggest_plan` (R4-S4).
fn tantivy_search_symbols(query: &str, limit: usize) -> Value {
    let limit_str = limit.to_string();
    let out = match std::process::Command::new("touring")
        .args(["tantivy", "search", query, &limit_str, "-j"])
        .output()
    {
        Ok(o) => o,
        Err(_) => return Value::Null,
    };
    match serde_json::from_slice::<Value>(&out.stdout) {
        Ok(Value::Array(hits)) => {
            let filtered: Vec<Value> = hits
                .into_iter()
                .take(limit)
                .filter_map(|h| {
                    let sym = h.get("symbol_name")?.as_str()?.to_string();
                    // Suggestion 4 (2026-04-20): surface functional_signature so
                    // check_contracts_in_tantivy can validate type contracts, not
                    // just symbol existence.
                    let functional_signature = h.get("functional_signature").cloned();
                    Some(serde_json::json!({
                        "symbol_name": sym,
                        "file_path": h.get("file_path"),
                        "crate_name": h.get("crate_name"),
                        "functional_signature": functional_signature,
                    }))
                })
                .collect();
            Value::Array(filtered)
        }
        _ => Value::Null,
    }
}

// ── recall ──────────────────────────────────────────────────────────────────

/// `touring_generator_recall_similar` — call `touring memory recall <query> -j`.
pub fn recall_similar(query: &str, limit: i64) -> Value {
    let out = std::process::Command::new("touring")
        .args(["memory", "recall", query, "-j"])
        .output();

    match out {
        Err(e) => serde_json::json!({
            "ok": false, "query": query,
            "error": format!("touring subprocess failed: {e}"),
        }),
        Ok(out) => {
            let text = String::from_utf8_lossy(&out.stdout);
            match serde_json::from_str::<Value>(&text) {
                Err(e) => serde_json::json!({
                    "ok": false, "query": query,
                    "error": format!("failed to parse touring output: {e}"),
                    "raw": text.chars().take(500).collect::<String>(),
                }),
                Ok(mut v) => {
                    if let Some(arr) = v.get_mut("matches").and_then(|a| a.as_array_mut()) {
                        arr.truncate(limit.max(1) as usize);
                    }
                    // R4-S3: Enrich memory recall with Tantivy symbol-level hits.
                    // Memory recall finds semantic patterns; Tantivy finds exact BM25 matches
                    // in the live symbol index — complementary signal sources for the LLM.
                    let tantivy_hits = tantivy_search_symbols(query, 5);
                    serde_json::json!({"ok": true, "query": query, "results": v, "tantivy_symbol_hits": tantivy_hits})
                }
            }
        }
    }
}

// ── plan introspection (diff/history/critique) — extracted to
// generator_tools_introspect.rs (F-9). Callers reach these via the
// `generator_tools_introspect::` path directly; no re-export here — a re-export
// would form a module import cycle, since introspect already depends on
// `generator_tools::parse_plan` (F1.8 decoupling, 2026-07-02).

// ── suggest ──────────────────────────────────────────────────────────────────

/// `touring_generator_suggest_plan` — generate a skeleton `GeneratorPlan` JSON.
pub fn suggest_plan(intent: &str, kind_str: Option<&str>) -> Value {
    let kind = kind_str
        .and_then(parse_generator_kind)
        .unwrap_or(GeneratorKind::RustModule);

    // Use a deterministic placeholder ID — callers should replace it
    let skeleton = build_skeleton_plan(intent, &kind);

    // R4-S4: Enrich suggestion with Tantivy BM25 symbol hits for the intent text.
    // Surfaces existing symbols semantically related to the intent, giving the
    // planner a starting point for `contracts.symbols_to_verify` without requiring
    // prior knowledge of the codebase symbol index.
    let intent_query = intent
        .split_whitespace()
        .take(8)
        .collect::<Vec<_>>()
        .join(" ");
    let symbol_hints = if intent_query.len() >= 3 {
        tantivy_search_symbols(&intent_query, 5)
    } else {
        Value::Null
    };

    // R6-S3: Pre-populate a ready-to-use contracts block from Tantivy symbol hits.
    // `symbol_hints` (R4-S4) gave informational hints; `suggested_contracts` gives
    // a copy-pasteable `symbols_must_exist` array that Claude Code can drop directly
    // into the skeleton without reformatting, removing one manual authorship step.
    let suggested_contracts = if let Some(arr) = symbol_hints.as_array() {
        let syms: Vec<Value> = arr
            .iter()
            .filter_map(|h| {
                let name = h.get("symbol_name")?.as_str()?;
                Some(serde_json::json!({"name": name, "crate_name": h.get("crate_name")}))
            })
            .collect();
        serde_json::json!({"symbols_must_exist": syms, "symbols_must_not_exist": []})
    } else {
        Value::Null
    };

    serde_json::json!({
        "ok": true,
        "suggestion": skeleton,
        "symbol_hints": symbol_hints,
        "suggested_contracts": suggested_contracts,
        "note": "Edit plan_id and target.file_path. Copy suggested_contracts into the plan's contracts field to seed VGP verification.",
    })
}

/// Build the skeleton plan JSON (extracted for CC reduction).
fn build_skeleton_plan(intent: &str, kind: &GeneratorKind) -> Value {
    serde_json::json!({
        "version": "2.0",
        "plan_id": "00000000-0000-0000-0000-000000000000",
        "intent": intent,
        "cila_level": "L2",
        "target": {"file_path": "src/generated.rs"},
        "kind": format!("{kind:?}"),
        "contracts": {"must_exist": [], "must_not_exist": []},
        "template": {"override_name": null},
        "assembly": {"merge_strategy": "overwrite"},
        "validation": {"run_clippy": true, "run_tests": false},
        "commit_policy": {"write_to_disk": true, "store_memory": true, "inject_rl_reward": true},
        "rollback": {"keep_backup": true},
        "learning": {"reward_on_success": 1.0, "reward_on_failure": -0.3},
        "spec_inputs": null,
        "capacity_hints": {"estimated_tokens": 500},
        "execution_trace": [],
        "metadata": {"tags": []},
    })
}

// ── template_list ────────────────────────────────────────────────────────────

/// `touring_generator_template_list` — list all built-in templates.
pub fn template_list() -> Value {
    let templates: Vec<Value> = all_kinds()
        .iter()
        .map(|k| {
            serde_json::json!({
                "kind": format!("{k:?}"),
                "label": k.label(),
                "template": k.template_name(),
            })
        })
        .collect();

    serde_json::json!({"ok": true, "count": templates.len(), "templates": templates})
}

// ── template_validate ─────────────────────────────────────────────────────────

/// `touring_generator_template_validate` — validate a Tera template file for syntax errors.
///
/// Delegates to `touring generate template-validate` subprocess to avoid
/// a direct tera dependency in touring-server.
pub fn template_validate(template_file: &str) -> Value {
    let out = std::process::Command::new("touring")
        .args([
            "generate",
            "template-validate",
            "--template-file",
            template_file,
            "-j",
        ])
        .output();

    match out {
        Err(e) => {
            serde_json::json!({"ok": false, "file": template_file, "error": format!("subprocess failed: {e}")})
        }
        Ok(out) => {
            let text = String::from_utf8_lossy(&out.stdout);
            serde_json::from_str::<Value>(&text).unwrap_or_else(|_| {
                serde_json::json!({
                    "ok": out.status.success(),
                    "file": template_file,
                    "raw": text.chars().take(500).collect::<String>(),
                })
            })
        }
    }
}

// ── template_test ─────────────────────────────────────────────────────────────

/// `touring_generator_template_test` — render a built-in template with given vars.
pub fn template_test(template_name: &str, vars_json: Option<&str>) -> Value {
    let kind = match all_kinds()
        .into_iter()
        .find(|k| k.template_name() == template_name)
    {
        None => {
            return serde_json::json!({
                "ok": false,
                "error": format!("no built-in template named '{template_name}'. Use touring_generator_template_list."),
            });
        }
        Some(k) => k,
    };

    let vars = match parse_vars(vars_json) {
        Err(e) => return serde_json::json!({"ok": false, "error": e}),
        Ok(v) => v,
    };

    let ctx = make_context();
    match ctx.template_engine.render_for_kind(&kind, &vars) {
        Ok(output) => serde_json::json!({
            "ok": true, "template": template_name, "kind": format!("{kind:?}"), "output": output,
        }),
        Err(e) => serde_json::json!({
            "ok": false, "template": template_name, "error": format!("{e}"),
            "hint": "Provide vars via vars_json: '{\"name\":\"MyModule\"}'",
        }),
    }
}

/// Parse optional `vars_json` string into a HashMap.
fn parse_vars(vars_json: Option<&str>) -> Result<HashMap<String, Value>, String> {
    match vars_json {
        None => Ok(HashMap::new()),
        Some(raw) => match serde_json::from_str(raw) {
            Err(e) => Err(format!("invalid vars JSON: {e}")),
            Ok(Value::Object(m)) => Ok(m.into_iter().collect()),
            Ok(_) => Err("vars_json must be a JSON object".into()),
        },
    }
}

// ── kinds_list ────────────────────────────────────────────────────────────────

/// `touring_generator_kinds_list` — list all GeneratorKind variants.
pub fn kinds_list() -> Value {
    let entries: Vec<Value> = all_kinds()
        .iter()
        .map(|k| {
            serde_json::json!({
                "kind": format!("{k:?}"),
                "label": k.label(),
                "template": k.template_name(),
            })
        })
        .collect();

    serde_json::json!({"ok": true, "count": entries.len(), "kinds": entries})
}

// ── capacity ──────────────────────────────────────────────────────────────────

/// `touring_generator_capacity` — show CapacityLimits defaults.
pub fn capacity() -> Value {
    let limits = CapacityLimits::default();
    match serde_json::to_value(&limits) {
        Ok(v) => serde_json::json!({"ok": true, "capacity": v}),
        Err(e) => serde_json::json!({"ok": false, "error": e.to_string()}),
    }
}

// ── submit_plan_with_registry ─────────────────────────────────────────────────

/// `touring_generator_submit_plan_with_registry` — run pipeline + register in PlanRegistry.
///
/// Same as `submit_plan` but registers the plan in the provided `PlanRegistry` before
/// execution and updates its status to `Completed` or `Failed` after. The registry
/// entry uses `intent_preview` derived from the plan's intent string.
pub async fn submit_plan_with_registry(
    plan_json: &str,
    dry_run: bool,
    registry: &PlanRegistry,
) -> Value {
    let plan = match parse_plan(plan_json) {
        Err(e) => return serde_json::json!({"ok": false, "error": e.to_string()}),
        Ok(p) => p,
    };
    let plan_id = plan.plan_id;
    let intent_preview = plan.intent.chars().take(80).collect::<String>();
    // Register before execution so callers can observe in-progress status.
    registry.register(PlanExecutorHandle {
        plan_id,
        status: ExecutionStatus::Rendered,
        intent_preview,
    });
    let result = run_pipeline(plan, dry_run, 0).await;
    let success = result.get("ok").and_then(Value::as_bool).unwrap_or(false);
    let final_status = if success {
        ExecutionStatus::Committed
    } else {
        ExecutionStatus::Failed
    };
    registry.update_status(plan_id, final_status);
    result
}

// ── bundle_plans ──────────────────────────────────────────────────────────────

/// `touring_generator_bundle_plans` — execute multiple plans as a sequential bundle.
///
/// Accepts an owned `Vec<String>` (vs. `bundle` which takes `&[String]`) for
/// ergonomic use from MCP tool dispatch.  Each plan runs through the full
/// pipeline; partial success is reported with per-plan results.
pub async fn bundle_plans(plans_json: Vec<String>, dry_run: bool) -> Value {
    bundle(&plans_json, dry_run).await
}

// ── schema_registry_info ──────────────────────────────────────────────────────

/// `touring_generator_schema_registry_info` — query schema registry version and migrations.
///
/// Returns the engine version, number of registered migration paths, and their
/// keys so callers can decide whether a migration is needed before submitting.
pub fn schema_registry_info() -> Value {
    let ctx = make_context();
    let reg = &ctx.schema_registry;
    let migration_keys: Vec<&str> = reg.migrations.keys().map(String::as_str).collect();
    serde_json::json!({
        "ok": true,
        "engine_version": reg.engine_version,
        "migration_count": reg.migrations.len(),
        "migration_keys": migration_keys,
    })
}

// ── schema_registry_check ─────────────────────────────────────────────────────

/// `touring_generator_schema_registry_check` — verify version compatibility against SchemaRegistry.
///
/// Returns `{ok, compatible, engine_version, requested_version}`.
/// A version is compatible if it equals the current engine version or has a
/// registered migration path.
pub fn schema_registry_check(version: &str) -> Value {
    let ctx = make_context();
    let compatible = ctx.schema_registry.is_compatible(version);
    serde_json::json!({
        "ok": true,
        "compatible": compatible,
        "engine_version": ctx.schema_registry.engine_version,
        "requested_version": version,
    })
}

// ── replay ────────────────────────────────────────────────────────────────────

/// `touring_generator_replay_plan` — re-run the full pipeline (iteration incremented).
pub async fn replay_plan(plan_json: &str) -> Value {
    let plan = match parse_plan(plan_json) {
        Err(e) => return serde_json::json!({"ok": false, "error": e.to_string()}),
        Ok(p) => p,
    };
    let iteration = (plan.execution_trace.len() as u8).saturating_add(1);
    run_pipeline(plan, false, iteration).await
}

// ── S-7: ConsumerGenerator pipeline ──────────────────────────────────────────

/// Canonical "no plans" result shared by every degraded path of
/// [`build_consumer_generator_plans`]. Always `ok: true` with an empty `plans`
/// array and `degraded: true`, so callers stay robust while `note` records why
/// no plans were produced (fail-soft, but loud).
fn degraded_plans(note: impl Into<String>) -> Value {
    serde_json::json!({
        "ok": true,
        "count": 0,
        "plans": [],
        "degraded": true,
        "note": note.into(),
    })
}

/// Runs `touring wiring orphans -j` and returns its stdout as a JSON string.
///
/// On any degraded condition — the subprocess cannot be spawned, exits
/// non-zero, or writes empty stdout (the wiring CLI sends its error to stderr
/// and leaves stdout empty when the resolved project DB holds no wiring data) —
/// returns the canonical [`degraded_plans`] result as the `Err` payload, so the
/// caller can early-return it unchanged. Keeps the orphan array borrowed by the
/// caller (it can hold ~170 K entries) instead of cloning it.
fn fetch_wiring_orphans_raw() -> Result<String, Value> {
    let output = std::process::Command::new("touring")
        .args(["wiring", "orphans", "-j"])
        .output()
        .map_err(|e| {
            degraded_plans(format!(
                "wiring orphans subprocess failed ({e}); treating as no orphans"
            ))
        })?;

    let raw = String::from_utf8_lossy(&output.stdout);
    if !output.status.success() || raw.trim().is_empty() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(degraded_plans(format!(
            "wiring orphans unavailable (exit={:?}, stderr={:?}); treating as no orphans",
            output.status.code(),
            stderr.trim().chars().take(200).collect::<String>()
        )));
    }
    Ok(raw.into_owned())
}

/// Build `GeneratorPlan` suggestions for pending wiring opportunities.
///
/// Queries orphan pub symbols via `touring wiring orphans -j` and maps each
/// to a `ConsumerGenerator` plan. Returns up to `limit` plans as a JSON array.
/// Callers submit them individually via `run_pipeline` or
/// `touring generate plan-submit`.
///
/// Uses `SystemTime` for plan-id generation (no `uuid` dependency required).
///
/// # Degraded paths
/// Always returns `ok: true` with a (possibly empty) `plans` array. When the
/// wiring subprocess cannot be spawned, exits non-zero, emits empty/unparseable
/// output, or reports no orphans, the result is `{"ok": true, "count": 0,
/// "plans": [], "degraded": true, "note": ...}` — "no wiring data" means "no
/// orphans to wire", not a hard failure, so the command stays robust to a
/// degraded daemon or a near-empty per-project DB.
pub fn build_consumer_generator_plans(limit: usize) -> Value {
    // Query orphan pub symbols via the wiring CLI. Any degraded condition
    // (spawn failure, non-zero exit, empty stdout) yields the canonical empty
    // plan set instead of breaking — see `fetch_wiring_orphans_raw`.
    let raw = match fetch_wiring_orphans_raw() {
        Ok(r) => r,
        Err(degraded) => return degraded,
    };

    let parsed: serde_json::Value = match serde_json::from_str(&raw) {
        Err(e) => {
            // Non-empty but unparseable output ⇒ still fail soft, keeping the
            // raw prefix for diagnosis.
            return degraded_plans(format!(
                "wiring orphans parse error ({e}); raw prefix: {}",
                raw.chars().take(200).collect::<String>()
            ));
        }
        Ok(v) => v,
    };

    let orphans = match parsed.get("orphans").and_then(|v| v.as_array()) {
        None => {
            return serde_json::json!({"ok": true, "count": 0, "plans": [], "note": "no orphan symbols found"});
        }
        Some(s) => s,
    };

    // Filter to Rust pub symbols only (skip JSON/config orphans).
    let rust_orphans: Vec<&serde_json::Value> = orphans
        .iter()
        .filter(|o| {
            o.get("module_file")
                .and_then(|v| v.as_str())
                .map(|f| f.ends_with(".rs"))
                .unwrap_or(false)
        })
        .take(limit)
        .collect();

    let plans: Vec<serde_json::Value> = rust_orphans
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let module_file = s.get("module_file").and_then(|v| v.as_str()).unwrap_or("unknown");
            let symbol_name = s.get("symbol_name").and_then(|v| v.as_str()).unwrap_or("unknown");
            let symbol_kind = s.get("symbol_kind").and_then(|v| v.as_str()).unwrap_or("any");
            // Unique plan-id via SystemTime nanos + index (no uuid dep needed).
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .subsec_nanos();
            let plan_id = format!("consumer-wiring-{nanos:010}-{i:03}");

            serde_json::json!({
                "version": "1.0.0",
                "plan_id": plan_id,
                "intent": format!("Wire orphan {} '{}' from '{}' into a consumer", symbol_kind, symbol_name, module_file),
                "cila_level": "L2",
                "target": {"file_path": module_file},
                "kind": "ConsumerGenerator",
                "contracts": {
                    "symbols_must_exist": [{"name": symbol_name, "kind": symbol_kind}],
                    "symbols_must_not_exist": [],
                    "files_must_exist": [module_file],
                    "files_must_not_exist": []
                },
                "template": {"override_name": null},
                "assembly": {"merge_strategy": "merge"},
                "validation": {"run_clippy": true, "run_tests": false},
                "commit_policy": {"write_to_disk": true, "store_memory": true, "inject_rl_reward": true},
                "rollback": {"keep_backup": true},
                "learning": {"reward_on_success": 0.8, "reward_on_failure": -0.3},
                "spec_inputs": null,
                "capacity_hints": {"estimated_tokens": 800},
                "execution_trace": [],
                "metadata": {"tags": ["wiring", "consumer", symbol_name]},
            })
        })
        .collect();

    serde_json::json!({
        "ok": true,
        "count": plans.len(),
        "total_orphans": orphans.len(),
        "rust_orphans_filtered": rust_orphans.len(),
        "plans": plans,
        "note": "submit each plan via 'touring generate plan-submit' or run_pipeline()",
    })
}

// ── Private helpers ──────────────────────────────────────────────────────────

/// All GeneratorKind variants in declaration order.
fn all_kinds() -> Vec<GeneratorKind> {
    vec![
        GeneratorKind::RustModule,
        GeneratorKind::CliHandler,
        GeneratorKind::McpTool,
        GeneratorKind::HookHandler,
        GeneratorKind::Test,
        GeneratorKind::BenchmarkSuite,
        GeneratorKind::FuzzTarget,
        GeneratorKind::DeriveMacro,
        GeneratorKind::AttributeMacro,
        GeneratorKind::FunctionMacro,
        GeneratorKind::ErrorCatalog,
        GeneratorKind::IncrementalPatch,
        GeneratorKind::FfiBinding,
        GeneratorKind::Schema,
        GeneratorKind::MigrationScript,
        GeneratorKind::ProtoBufSchema,
        GeneratorKind::OpenApiSpec,
        GeneratorKind::AsyncApiSpec,
        GeneratorKind::PlanMarkdown,
        GeneratorKind::SkillDocument,
        GeneratorKind::DiaryEntry,
        GeneratorKind::ChangelogEntry,
        GeneratorKind::Adr,
        GeneratorKind::ShellCompletion,
        GeneratorKind::ManPage,
        GeneratorKind::PythonScript,
        GeneratorKind::TypeScriptModule,
        GeneratorKind::DockerImage,
        GeneratorKind::KubernetesManifest,
        GeneratorKind::TerraformModule,
        GeneratorKind::CiWorkflow,
    ]
}

/// Parse a string into a GeneratorKind (case-insensitive, dash/underscore-tolerant).
fn parse_generator_kind(s: &str) -> Option<GeneratorKind> {
    let normalized = s.to_lowercase().replace(['-', '_', ' '], "");
    let candidates: &[(&str, GeneratorKind)] = &[
        ("rustmodule", GeneratorKind::RustModule),
        ("clihandler", GeneratorKind::CliHandler),
        ("mcptool", GeneratorKind::McpTool),
        ("hookhandler", GeneratorKind::HookHandler),
        ("test", GeneratorKind::Test),
        ("benchmarksuite", GeneratorKind::BenchmarkSuite),
        ("fuzztarget", GeneratorKind::FuzzTarget),
        ("derivemacro", GeneratorKind::DeriveMacro),
        ("attributemacro", GeneratorKind::AttributeMacro),
        ("functionmacro", GeneratorKind::FunctionMacro),
        ("errorcatalog", GeneratorKind::ErrorCatalog),
        ("incrementalpatch", GeneratorKind::IncrementalPatch),
        ("ffibinding", GeneratorKind::FfiBinding),
        ("schema", GeneratorKind::Schema),
        ("migrationscript", GeneratorKind::MigrationScript),
        ("protobufschema", GeneratorKind::ProtoBufSchema),
        ("openapispec", GeneratorKind::OpenApiSpec),
        ("asyncapispec", GeneratorKind::AsyncApiSpec),
        ("planmarkdown", GeneratorKind::PlanMarkdown),
        ("skilldocument", GeneratorKind::SkillDocument),
        ("diaryentry", GeneratorKind::DiaryEntry),
        ("changelogentry", GeneratorKind::ChangelogEntry),
        ("adr", GeneratorKind::Adr),
        ("shellcompletion", GeneratorKind::ShellCompletion),
        ("manpage", GeneratorKind::ManPage),
        ("pythonscript", GeneratorKind::PythonScript),
        ("typescriptmodule", GeneratorKind::TypeScriptModule),
        ("typescript", GeneratorKind::TypeScriptModule),
        ("dockerimage", GeneratorKind::DockerImage),
        ("kubernetesmanifest", GeneratorKind::KubernetesManifest),
        ("terraformmodule", GeneratorKind::TerraformModule),
        ("ciworkflow", GeneratorKind::CiWorkflow),
    ];
    candidates
        .iter()
        .find(|(k, _)| normalized == *k)
        .map(|(_, kind)| kind.clone())
}
// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[path = "generator_tools_tests.rs"]
mod tests;
