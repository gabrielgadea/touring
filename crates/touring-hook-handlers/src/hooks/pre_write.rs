//! Pre-Write Hook — Comprehensive validation before Claude creates or overwrites a file.
//!
//! Validates full file content BEFORE it's written to disk:
//! 1. Speculative validation (syntax, symbols, structural, imports)
//! 2. Anti-pattern detection (unwrap, todo!, bare except)
//! 3. Import completeness
//! 4. Wiring prediction (new pub symbols)
//! 5. Quality baseline (complexity, async ratio)
//! 6. Naming conventions
//! 7. File size awareness
//!
//! Enhancements: error prediction in cold path (AF1), WilsonRanker gotcha scoring (AF7).
//!
//! Target latency: <50ms.

use super::error_predictor::ErrorPredictor;
use super::knowledge::FileKnowledgeDB;
use super::pre_edit_prevention;
use super::runtime::{HookResponse, HookRuntime, make_relative};
use crate::shared::signal_pipeline::{
    FnSignalLayer, SignalContext, SignalPipeline, StaticSignalLayer,
};
use crate::shared::signals::{
    blast_radius_signal, enrich_with_cognitive, rank_gotchas_by_relevance, wilson_adjusted_score,
};
#[cfg(feature = "tantivy-fts")]
use crate::shared::signals::{
    tantivy_crate_origin_signal, tantivy_fuzzy_file_signal, tantivy_fuzzy_symbol_signal,
    tantivy_kind_context_signal, tantivy_related_docs_signal,
};
use touring_code::ast::speculate_v2;
use touring_foundation::truncate_str;

use crate::schemas::{validate_payload, validation_deny};
use crate::shared::cila::cila_budget_write;
use crate::shared::hook_helpers;
use crate::triad_hook;

/// Run the pre-write hook (diverging version — for use by the CLI entry point).
#[tracing::instrument(skip(runtime, input), fields(hook = "pre_write"))]
pub fn run(
    runtime: &mut HookRuntime,
    input: &serde_json::Value,
) -> Result<(), touring_hook_runtime::hook_runtime::HookDispatchError> {
    run_returning(runtime, input).emit()
}

// ── PII Helper ─────────────────────────────────────────────────────────────

/// Format PII scan findings as a compact security signal for PreToolUse injection.
///
/// Shows count by severity: `PII: 2 high (cpf, cpf) | 1 medium (email)`.
/// High-severity findings (CPF, CNPJ, RG, CNH, SUS) are always shown.
/// Medium-severity (processos SEI, email) are shown if count is small.
/// Low-severity (phone) is suppressed to avoid noise.
fn format_pii_findings_context(findings: &[super::pii::PIIFinding]) -> String {
    let high: Vec<_> = findings.iter().filter(|f| f.severity == "high").collect();
    let medium: Vec<_> = findings.iter().filter(|f| f.severity == "medium").collect();

    let mut parts = Vec::new();

    if !high.is_empty() {
        let unique: std::collections::HashSet<_> =
            high.iter().map(|f| f.pattern_name.as_str()).collect();
        parts.push(format!(
            "PII: {} high ({})",
            high.len(),
            unique.into_iter().collect::<Vec<_>>().join(", ")
        ));
    }
    if !medium.is_empty() {
        let first = medium
            .first()
            .map(|f| f.pattern_name.as_str())
            .unwrap_or("unknown");
        let extra = if medium.len() > 1 {
            format!(" +{}", medium.len() - 1)
        } else {
            String::new()
        };
        parts.push(format!("PII: {} medium ({}{})", medium.len(), first, extra));
    }
    // Suppress low-severity (phone numbers) — too noisy for PreToolUse

    parts.join(" | ")
}

/// Run the pre-write hook, returning a `HookResponse` instead of diverging.
///
/// Used by the daemon to handle the hook without calling `process::exit`.
pub fn run_returning(runtime: &mut HookRuntime, input: &serde_json::Value) -> HookResponse {
    // D9: Validate payload with typed schema — fail fast on malformed input.
    // Extract /tool_input first since payload schemas model the inner object.
    let tool_input = match input.get("tool_input") {
        Some(v) => v,
        None => {
            return validation_deny(
                &{
                    let mut e = validator::ValidationErrors::new();
                    e.add(
                        "tool_input",
                        validator::ValidationError {
                            code: std::borrow::Cow::Borrowed("missing"),
                            message: Some(std::borrow::Cow::Borrowed(
                                "tool_input missing from input",
                            )),
                            params: std::collections::HashMap::new(),
                        },
                    );
                    e
                },
                "pre_write",
            );
        }
    };
    let validated = match validate_payload::<crate::schemas::PreWritePayload>(tool_input) {
        Ok(v) => v,
        Err(errors) => return validation_deny(&errors, "pre_write"),
    };
    let file_path = validated.file_path.as_str();
    let content = validated.content.as_deref().unwrap_or("");

    if file_path.is_empty() || content.is_empty() {
        return HookResponse::Allow;
    }

    // E14: Skip expensive analysis for high-durability files (vendor, node_modules, etc.).
    if crate::ast_bridge::is_high_durability_target(file_path) {
        return HookResponse::Allow;
    }

    let rel_path = make_relative(file_path, &runtime.project_root);

    // T-2: TRIAD pre-write — snapshot file before write for rollback protection.
    // Stores TriadState in runtime.triad_state via RefCell; post_write reads/resets.
    if let Some(triad_state) = triad_hook::run_pre_write(file_path) {
        *runtime.triad_state.borrow_mut() = Some(triad_state);
        tracing::debug!(path = %file_path, "TRIAD: armed — state stored in runtime");
    }

    // S6/E19: Read session CILA level — prefer stable session context,
    // fall back to result_cache for standalone/cold-start mode.
    let cila_level: u8 = hook_helpers::cila_level_from_runtime(runtime, 3);

    let budget = cila_budget_write(cila_level);

    // L7-B Alpha: Enrichment policy gate — decides whether to run the expensive
    // signal collection pipeline (DB queries, cognitive, error predictor, AST).
    // At L0/L1 or when enrichment_pipeline is inactive, skip to fast-path baseline.
    let enrich_ok =
        crate::shared::cila::should_enrich(runtime.enrichment_active, cila_level, "Write");

    let mut context = if !enrich_ok {
        // L7-B Gamma: record fast-path for gate observability metrics
        crate::shared::gate_metrics::record_pre_write_fast_path();
        // L7-B Alpha fast-path: minimal baseline, zero DB/AST calls.
        // Keeps pre_write latency < 2ms at reflexive CILA levels.
        let lang_str = detect_language(file_path);
        let line_count = content.lines().count();
        format!("pre_write: new file ({lang_str}, {line_count}L) [L7B:fast-path cila={cila_level}]")
    } else {
        // L7-B Gamma: record full-enrichment path
        crate::shared::gate_metrics::record_pre_write_full();
        // ── Layer 1: All non-'static signals collected up-front ──
        // (DB, error predictor, blast radius, antipatterns, cognitive cannot be
        // captured in 'static FnSignalLayer closures — same pattern as pre_edit.)
        let up_front_signals = collect_upfront_signals(runtime, file_path, &rel_path, content);

        // ── Layer 2: AST content signals — CPU-bound, on dedicated hook pool ──
        use crate::shared::thread_pool::with_hook_pool;
        let content_owned = content.to_owned();
        let fp_owned = file_path.to_owned();
        let rel_path_for_ast = rel_path.clone();

        // ── Assemble via SignalPipeline (normalize + sort + budget-truncate) ──
        let pipeline = SignalPipeline::new(budget)
            .add_layer(StaticSignalLayer::new("up_front", up_front_signals))
            .add_layer(FnSignalLayer::new("ast_content", move |_ctx| {
                with_hook_pool(|| ast_content_signals(&content_owned, &fp_owned, &rel_path_for_ast))
            }));

        match pipeline.execute(&SignalContext::new(&rel_path, "").with_cila(cila_level as usize)) {
            Some(ctx) => ctx,
            None => {
                // Pipeline produced zero signals — inject a baseline context
                // so pre-write ALWAYS returns useful information.
                let lang_str = detect_language(file_path);
                let line_count = content.lines().count();
                let score_info = if let Ok(lang) = lang_str.parse::<touring_code::ast::Lang>() {
                    let spec = speculate_v2(content, lang, None, None);
                    if spec.composite_score < 1.0 {
                        let failed: Vec<_> = spec
                            .layers
                            .iter()
                            .filter(|l| !l.passed)
                            .map(|l| format!("{:?}: {}", l.layer, l.diagnostics.join(", ")))
                            .collect();
                        format!(
                            " | speculate={:.2} issues=[{}]",
                            spec.composite_score,
                            failed.join("; ")
                        )
                    } else {
                        " | speculate=1.0 (clean)".to_string()
                    }
                } else {
                    String::new()
                };
                format!("pre_write: new file ({lang_str}, {line_count}L){score_info}")
            }
        }
    };

    // S7: Log the file for which context was injected.
    runtime.ctx.result_cache.cache_result(
        "__meta__",
        "__context_injection_file__",
        rel_path.to_string(),
    );

    // ── PII Scan: detect Brazilian PII in new file content (PreToolUse) ──
    // Scans the full file content before it is written.
    // High/medium findings are injected as security signals.
    if !content.is_empty() {
        let findings = runtime.ctx.pii_scanner.scan_text(content);
        if !findings.is_empty() {
            let pii_ctx = format_pii_findings_context(&findings);
            if !context.is_empty() {
                context.push_str(" | ");
            }
            context.push_str(&pii_ctx);
        }
    }

    // ── RL reward injection: complexity + unwrap penalties ───────────────
    // Fires when file has high cyclomatic complexity or unwrap density.
    // Feeds Pensieve/OnlineRL engine via inject_reward — RL learns to avoid
    // writing structurally fragile code. Mirrors post_edit delta_reward pattern.
    if !context.is_empty() || !content.is_empty() {
        let lang = detect_language(file_path);
        let line_count = content.lines().count();

        // Complexity penalty
        let complexity = touring_analysis::analyze_complexity(content, lang);
        if let Some(reward) =
            crate::health_delta::complexity_reward(complexity.max_complexity as u32)
            && reward < 0.0
        {
            runtime
                .learning
                .inject_reward("pre_write", reward, "high_complexity");
        }

        // Unwrap penalty
        let unwrap_audit = touring_analysis::analyze_unwraps(content);
        if let Some(reward) = crate::health_delta::unwrap_penalty(unwrap_audit.count, line_count)
            && reward < 0.0
        {
            runtime
                .learning
                .inject_reward("pre_write", reward, "high_unwrap_density");
        }
    }

    HookResponse::Context {
        context,
        event_name: Some("PreToolUse".to_string()),
    }
}

// ── Signal Functions ────────────────────────────────────────────────────

/// Build the error-prediction signal using the cached (or freshly-trained) predictor.
///
/// Avoids O(n) retrain by cloning the cached predictor from `ContextRuntime` when
/// available. Returns `None` when no prediction exceeds the confidence threshold.
fn collect_error_prediction_signal(runtime: &HookRuntime, rel_path: &str) -> Option<(f32, String)> {
    let mut predictor = match runtime.ctx.error_predictor.as_ref() {
        Some(cached) => cached.clone(),
        None => {
            let mut p = ErrorPredictor::new();
            let _ = p.train_from_db(&runtime.ctx.knowledge); // returns count of trained examples; value unused here
            p
        }
    };
    predictor.record_edit(rel_path, "Write");
    let pred = predictor.predict()?;
    Some((
        1.3,
        format!(
            "\u{26a0} PREDICTED: {}% chance of '{}' ({}x observed)",
            (pred.probability * 100.0) as u32,
            truncate_str(&pred.error_pattern, 60),
            pred.observations
        ),
    ))
}

/// Collect all Layer-1 (non-`'static`) signals for the pre-write hook.
///
/// Groups: knowledge DB · error prediction · blast radius · anti-patterns · cognitive.
/// These cannot be captured in `'static` `FnSignalLayer` closures, so they are
/// gathered up-front before the `SignalPipeline` is assembled.
fn collect_upfront_signals(
    runtime: &HookRuntime,
    file_path: &str,
    rel_path: &str,
    content: &str,
) -> Vec<(f32, String)> {
    let mut signals: Vec<(f32, String)> = Vec::new();

    // ── Knowledge DB signals (cache-first, DB fallback) ──────────────────
    // Identical cache-first pattern to pre_edit.rs
    let cache = &runtime.ctx.result_cache;
    let cache_key = format!("__precomputed:{}", rel_path);
    let db_signals = match cache.get_result("pre_write", &cache_key) {
        Some(json) => {
            match serde_json::from_str::<crate::precomputed_signals::PrecomputedSignals>(&json) {
                Ok(pre) => {
                    let mut sigs: Vec<(f32, String)> =
                        pre.signals.into_iter().map(|s| (s.0, s.1)).collect();
                    // Add functional chain signal (pre_write-specific)
                    if let Some(fc) = crate::functional_wiring::functional_chain_signal(
                        &runtime.ctx.knowledge,
                        rel_path,
                    ) {
                        sigs.push(fc);
                    }
                    // ErrorPredictor is runtime-only
                    if let Some(ref predictor) = runtime.ctx.error_predictor {
                        let pred = predictor.clone();
                        if let Some(p) = pred.predict() {
                            sigs.push((
                                1.3,
                                format!(
                                    "pred: {} \u{2014} likely next error (p={:.0}%)",
                                    p.error_pattern,
                                    p.probability * 100.0
                                ),
                            ));
                        }
                    }
                    sigs
                }
                Err(_) => knowledge_signals_with_runtime(
                    &runtime.ctx.knowledge,
                    rel_path,
                    content,
                    Some(runtime),
                ),
            }
        }
        None => {
            knowledge_signals_with_runtime(&runtime.ctx.knowledge, rel_path, content, Some(runtime))
        }
    };
    signals.extend(db_signals);

    // ── Signal 12: Co-edit neighbors (temporal coupling signal) ──
    // Files frequently written together are semantically coupled even without
    // explicit import edges. Mirrors the same signal added to pre_edit.rs (EC14).
    {
        let coedit_neighbors = runtime.ctx.knowledge.get_coedit_neighbors(rel_path, 5);
        if !coedit_neighbors.is_empty() {
            let names: Vec<&str> = coedit_neighbors.iter().map(|(p, _)| p.as_str()).collect();
            signals.push((
                1.1,
                format!(
                    "co-edits: {} file(s) frequently written together [{}]",
                    coedit_neighbors.len(),
                    names.join(", ")
                ),
            ));
        }
    }

    // Anti-pattern detection on full content.
    signals.extend(antipattern_signals(content, rel_path));

    // ── Signal I-5b: Callgraph enrichment — callers of functions in this file ──
    // Reads the new content being written to find which symbols have the most callers.
    // Language-guarded (Rust + Python). Budget: negligible — purely in-memory.
    if let Some(cg_sig) = callgraph_signal_for_write(content, file_path) {
        signals.push(cg_sig);
    }

    // Quality depth signals: complexity + unwrap audit on full write content.
    // Budget guard: skip when signals already rich to stay within <50ms target.
    if signals.len() < 8 {
        signals.extend(quality_depth_signals(content, rel_path));
    }

    // Cognitive enrichment (risk + gotchas from CognitiveRuntime).
    if let Some(ref cognitive) = runtime.cognitive {
        let enriched = enrich_with_cognitive(cognitive, file_path, false);
        if !enriched.is_empty() {
            signals.push((1.0, enriched));
        }
    }

    // ── Tantivy BM25: related docstrings from other files (same module concepts) ──
    // Mirrors pre_read.rs:collect_index_signals pattern. Feature-gated tantivy-fts.
    #[cfg(feature = "tantivy-fts")]
    if let Some(s) = tantivy_related_docs_signal(Some(&runtime.project_root), rel_path) {
        signals.push(s);
    }
    #[cfg(feature = "tantivy-fts")]
    if let Some(s) = tantivy_fuzzy_file_signal(Some(&runtime.project_root), rel_path) {
        signals.push(s);
    }
    #[cfg(feature = "tantivy-fts")]
    if let Some(s) = tantivy_kind_context_signal(Some(&runtime.project_root), rel_path) {
        signals.push(s);
    }
    #[cfg(feature = "tantivy-fts")]
    if let Some(s) = tantivy_crate_origin_signal(Some(&runtime.project_root), rel_path) {
        signals.push(s);
    }
    #[cfg(feature = "tantivy-fts")]
    if let Some(s) = tantivy_fuzzy_symbol_signal(Some(&runtime.project_root), rel_path) {
        signals.push(s);
    }

    // Blast radius: impact of this write on the broader codebase.
    // Passes real SymbolIndex from runtime (was None in ast_content_signals which
    // lacked runtime access — functional gap fixed by moving to collect_upfront_signals).
    if let Some((score, text)) =
        blast_radius_signal(runtime.infra.symbol_index.as_ref(), rel_path, false)
    {
        signals.push((score, text));
    }

    // ── Signal I-4: Similar symbol context ──
    // Mirrors pre_read.rs:collect_index_signals. Surfaces symbols with similar names
    // or module structure so Claude can follow naming conventions and patterns
    // already established in the codebase when writing new files.
    if let Some((score, text)) = crate::shared::signals::similar_symbol_signal_for_path(
        runtime.infra.symbol_index.as_ref(),
        rel_path,
    ) {
        signals.push((score, text));
    }

    // ── Signal I-5: ANN recall — semantically related past memories ──
    // Promotes ann_recall_signal from pre_read (pub(crate)) to serve pre_write.
    // Surfaces memories from past writes to files with similar path structure.
    if let Some((score, text)) = crate::pre_read::ann_recall_signal(runtime, rel_path) {
        signals.push((score, text));
    }

    // ── Signal I-6: Extended metadata — test coverage + community affinity ──
    // Surfaces coverage_pct (untested files need careful writing) and community_id
    // (module cluster membership) from the schema-v8 LEFT JOIN enrichment query.
    if let Ok(Some(ext)) = runtime.ctx.knowledge.query_extended(rel_path)
        && let Some(sig) = hook_helpers::build_file_meta_signal(&ext)
    {
        signals.push((0.5, sig));
    }

    // ── Signal I-7: Pensieve lookup — past failure pattern detection ──
    // Checks if this file's path resembles paths that triggered recorded failures.
    // Mirrors pre_bash.rs E15 pattern using command_to_states for path hashing.
    {
        let states = crate::shared::command_hash::command_to_states(rel_path);
        if !states.is_empty()
            && let Ok(pensieve) = runtime.learning.pensieve.try_borrow()
        {
            let penalty = match states.first() {
                Some(&single) if states.len() == 1 => pensieve.check_known_failure(single),
                _ => pensieve.check_known_failure_seq(&states),
            };
            if let Some(sim) = penalty {
                signals.push((
                    1.2,
                    format!(
                        "⚠ pensieve: similar path had recorded failures ({:.0}% match)",
                        sim * 100.0
                    ),
                ));
            }
        }
    }

    signals
}

/// Collect knowledge-DB signals for the target file.
///
/// Signals (scored by actionability):
/// - Quality gate failures (2.0) — prevents trial-and-error BLOCK loops
/// - File risk (1.8/1.2) — historical failure rate after edits
/// - Wiring orphans (1.6) — pub symbols without consumers
/// - Notes/gotchas (1.5) — accumulated non-visible knowledge
/// - Pre-edit prevention (1.4) — decay-weighted cross-session patterns
/// - Dependents (1.0) — impact awareness
/// - Error predictions (1.3) — Markov-based proactive warnings
#[cfg(test)]
fn knowledge_signals(db: &FileKnowledgeDB, rel_path: &str, content: &str) -> Vec<(f32, String)> {
    knowledge_signals_with_runtime(db, rel_path, content, None)
}

/// Inner implementation that optionally accepts `&HookRuntime` for error
/// prediction. When `runtime` is `None` (e.g. in unit tests), the error
/// prediction signal is skipped.
fn knowledge_signals_with_runtime(
    db: &FileKnowledgeDB,
    rel_path: &str,
    content: &str,
    runtime: Option<&HookRuntime>,
) -> Vec<(f32, String)> {
    let mut signals: Vec<(f32, String)> = Vec::new();

    // Signal 1: Dependents (who imports this file?)
    if let Some(s) = dependents_signal(db, rel_path) {
        signals.push(s);
    }

    // Signal 2: Quality gate / lint failures on THIS file (HIGHEST priority).
    if let Some(s) = lint_failure_signal(db, rel_path) {
        signals.push(s);
    }

    // Signal 3: Notes/gotchas (accumulated knowledge).
    if let Ok(Some(k)) = db.lookup(rel_path)
        && let Some(notes) = &k.notes
        && !notes.is_empty()
    {
        let short = truncate_str(notes, 100);
        signals.push((1.5, format!("note: {short}")));
    }

    // Signal 4: Gotcha patterns (audit-learned anti-patterns), ranked by
    // BM25+TF-IDF semantic relevance to the file path so the most contextually
    // relevant gotchas surface first. Score is Wilson-adjusted by hit history.
    // Uses content-based matching: patterns like "session_predictor::ToolInvocation"
    // are matched against the actual file content rather than the file path.
    let gotchas = db.get_gotchas_for_content(content, rel_path);
    if !gotchas.is_empty() {
        let pairs: Vec<(String, String)> = gotchas
            .iter()
            .map(|g| (g.pattern.clone(), g.gotcha.clone()))
            .collect();
        let ranked = rank_gotchas_by_relevance(&pairs, rel_path, 2);
        for msg in &ranked {
            let short_msg = truncate_str(msg, 120);
            // Look up severity and hit stats from the originating gotcha.
            let source = gotchas.iter().find(|g| &g.gotcha == msg);
            let severity = source.map(|g| g.severity.as_str()).unwrap_or("warning");
            let score = source
                .map(|g| {
                    wilson_adjusted_score(
                        1.5,
                        g.prevented_errors.max(0) as u32,
                        g.hit_count.max(0) as u32,
                    )
                })
                .unwrap_or(1.5 * 0.5);
            signals.push((
                score,
                format!("\u{26a0} GOTCHA [{}]: {}", severity, short_msg),
            ));
        }
    }

    // Signal 5: File risk score (RL-computed historical failure rate).
    if let Some(s) = risk_signal(db, rel_path) {
        signals.push(s);
    }

    // Signal 6: Wiring check — orphan pub symbols in this file.
    if let Some(s) = wiring_orphan_signal(db, rel_path) {
        signals.push(s);
    }

    // Signal 7: Pre-edit prevention (decay-weighted cross-session patterns).
    if let Some(s) = prevention_signal(db, rel_path, &signals) {
        signals.push(s);
    }

    // Signal 8: Error predictions — cold-path fallback using the cached (or
    // freshly-trained) predictor. The hot-path in `collect_upfront_signals`
    // uses the pre-cached predictor from `ContextRuntime`; this cold-path
    // ensures predictions are still emitted when the cache misses.
    // Skipped when `runtime` is None (unit tests).
    if let Some(rt) = runtime
        && let Some(s) = collect_error_prediction_signal(rt, rel_path)
    {
        signals.push(s);
    }

    signals
}

/// Build a dependents signal: lists files that import `rel_path`.
fn dependents_signal(db: &FileKnowledgeDB, rel_path: &str) -> Option<(f32, String)> {
    let dependents = db.get_dependents(rel_path).ok()?;
    if dependents.is_empty() {
        return None;
    }
    let dep_files: Vec<&str> = dependents
        .iter()
        .take(5)
        .map(|r| {
            std::path::Path::new(r.source.as_str())
                .file_name()
                .and_then(|f| f.to_str())
                .unwrap_or(r.source.as_str())
        })
        .collect();
    Some((
        1.0,
        format!(
            "impact: {} file(s) import this [{}]",
            dependents.len(),
            dep_files.join(", ")
        ),
    ))
}

/// Build a lint-failure signal from the most recent quality-gate failure.
fn lint_failure_signal(db: &FileKnowledgeDB, rel_path: &str) -> Option<(f32, String)> {
    let failures = db.recent_failures_for_file(rel_path, 5).ok()?;
    let latest = failures.iter().find(|f| {
        let cmd = f.command.to_lowercase();
        cmd.contains("ruff")
            || cmd.contains("pyright")
            || cmd.contains("lint")
            || cmd.contains("clippy")
            || cmd.contains("code_standards")
    })?;
    let cmd_short = latest.command.split_whitespace().next().unwrap_or("linter");
    let err = latest
        .error_pattern
        .as_deref()
        .unwrap_or("violations found");
    let short_err = truncate_str(err, 100);
    Some((
        2.0,
        format!(
            "\u{26a0}\u{fe0f} quality: `{cmd_short}` previously failed \u{2014} \
             fix pre-existing violations in new content: {short_err}"
        ),
    ))
}

/// Build a file-risk signal based on the RL-computed historical failure rate.
fn risk_signal(db: &FileKnowledgeDB, rel_path: &str) -> Option<(f32, String)> {
    let risk = db.file_risk_score(rel_path);
    if risk < 0.3 {
        return None;
    }
    let level = if risk >= 0.5 { "HIGH" } else { "MEDIUM" };
    let score = if risk >= 0.5 { 1.8 } else { 1.2 };
    Some((
        score,
        format!(
            "\u{26a0} file_risk: {level} ({:.0}% failure rate after edits \u{2014} verify carefully)",
            risk * 100.0
        ),
    ))
}

/// Build a wiring-orphan signal when pub symbols in `rel_path` have no consumers.
fn wiring_orphan_signal(db: &FileKnowledgeDB, rel_path: &str) -> Option<(f32, String)> {
    let status = db.module_wiring_status(rel_path).ok()?;
    if status.orphan_symbols.is_empty() || status.integration_score >= 1.0 {
        return None;
    }
    let orphan_list = status.orphan_symbols.join(", ");
    let short = truncate_str(&orphan_list, 80);
    Some((
        1.6,
        format!(
            "wiring({:.0}%): {} orphan pub symbol(s) [{}] \u{2014} wire into consumers or reduce to pub(crate)",
            status.integration_score * 100.0,
            status.orphan_symbols.len(),
            short,
        ),
    ))
}

/// Build a pre-edit-prevention signal, deduplicating against existing gotcha signals.
fn prevention_signal(
    db: &FileKnowledgeDB,
    rel_path: &str,
    existing: &[(f32, String)],
) -> Option<(f32, String)> {
    let prevention_ctx = pre_edit_prevention::compose_pre_edit_warning(db, rel_path)?;
    let has_gotcha = existing
        .iter()
        .any(|p| p.1.to_uppercase().contains("GOTCHA"));
    let prevention_has_gotcha = prevention_ctx.to_uppercase().contains("GOTCHA");
    if has_gotcha && prevention_has_gotcha {
        return None;
    }
    Some((
        1.4,
        format!("prevention: {}", truncate_str(&prevention_ctx, 200)),
    ))
}

/// Collect speculative-validation signals for a single layer result.
///
/// Maps `Syntax` failures to score 2.5 and all other layer failures to 1.8.
/// Returns one scored signal per failed layer (up to 3 diagnostics joined).
fn speculative_validation_signals(
    content: &str,
    lang: touring_code::ast::Lang,
) -> Vec<(f32, String)> {
    let result = speculate_v2(content, lang, None, None);
    if result.all_passed {
        return Vec::new();
    }
    result
        .layers
        .iter()
        .filter(|l| !l.passed)
        .map(|layer| {
            let score = match layer.layer {
                touring_code::ast::ValidationLayer::Syntax => 2.5,
                _ => 1.8,
            };
            let diag_summary = if layer.diagnostics.is_empty() {
                format!("{:?} failed", layer.layer)
            } else {
                layer
                    .diagnostics
                    .iter()
                    .take(3)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join("; ")
            };
            let short = truncate_str(&diag_summary, 120);
            (
                score,
                format!("\u{274c} speculate({:?}): {short}", layer.layer),
            )
        })
        .collect()
}

/// Collect quality-baseline signals (CC, async ratio) from AST metrics.
///
/// Returns a single scored signal at weight 0.8, or empty vec when within limits.
fn quality_baseline_signals(content: &str, file_path: &str) -> Vec<(f32, String)> {
    let Some(metrics) = super::ast_bridge::analyze_file_quality(content, file_path) else {
        return Vec::new();
    };
    let mut quality_parts: Vec<String> = Vec::new();
    if !metrics.complex_symbols.is_empty() {
        let names = metrics
            .complex_symbols
            .iter()
            .take(3)
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        quality_parts.push(format!("CC>10: [{}]", names));
    }
    if metrics.avg_complexity > 8.0 {
        quality_parts.push(format!("avg_CC={:.1}", metrics.avg_complexity));
    }
    if metrics.async_count > 0 {
        quality_parts.push(format!("async_ratio={:.0}%", metrics.async_ratio * 100.0));
    }
    if quality_parts.is_empty() {
        Vec::new()
    } else {
        vec![(0.8, format!("quality: {}", quality_parts.join(", ")))]
    }
}

/// AST-based content validation signals.
///
/// Runs on the rayon thread pool — reads no files from disk, operates on
/// the in-memory `content` string. Signals:
/// - Speculative validation (2.5 syntax, 1.8 symbol/import)
/// - Quality baseline via AST (0.8)
/// - Wiring prediction for new pub symbols (1.2)
/// - File size awareness (0.6)
/// - Entry-point awareness: lib.rs/main.rs/mod.rs at shallow depth (0.7)
/// - Deep module awareness: files ≥3 levels deep (0.5)
fn ast_content_signals(content: &str, file_path: &str, rel_path: &str) -> Vec<(f32, String)> {
    let mut signals: Vec<(f32, String)> = Vec::new();
    let lang_str = detect_language(file_path);

    // Signal A: Speculative validation (syntax + symbols + structural + imports).
    let lang = match lang_str.parse::<touring_code::ast::Lang>() {
        Ok(l) => l,
        Err(_) => return signals,
    };
    signals.extend(speculative_validation_signals(content, lang));

    // Signal B: Quality baseline (CC, async ratio) via ast_bridge.
    signals.extend(quality_baseline_signals(content, file_path));

    // Signal C: Wiring prediction — count new pub symbols being introduced.
    let pub_count = count_pub_symbols(content, lang_str);
    if pub_count > 0 {
        signals.push((
            1.2,
            format!(
                "wiring_predict: {pub_count} new pub symbol(s) \u{2014} \
                 ensure consumers exist or reduce visibility"
            ),
        ));
    }

    // Signal D: File size awareness.
    let line_count = content.lines().count();
    if line_count > 500 {
        signals.push((
            0.6,
            format!(
                "\u{26a0} large_file: {line_count} lines \u{2014} \
                 consider splitting into focused modules"
            ),
        ));
    }

    // Signal E: Bench/test target missing required-features in Cargo.toml.
    if let Some(sig) = bench_required_features_signal(file_path, content) {
        signals.push(sig);
    }

    // Signal F: Module depth awareness — deeply nested files are internal impl details.
    // Shallow paths (lib.rs, main.rs at root or src/) are high-coupling entry points.
    // Only fires when content is present — preserves "empty content = no signals" invariant.
    if !content.is_empty() {
        let depth = rel_path.matches('/').count();
        if depth <= 1 {
            let base = std::path::Path::new(rel_path)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(rel_path);
            if matches!(base, "lib.rs" | "main.rs" | "mod.rs" | "lib" | "main") {
                signals.push((
                    0.7,
                    format!(
                        "\u{1f4e6} entry_point: `{rel_path}` is a module root \u{2014} \
                         changes here propagate widely, ensure comprehensive test coverage"
                    ),
                ));
            }
        }
        if depth >= 3 {
            signals.push((
                0.5,
                format!(
                    "deep_module: `{rel_path}` is {depth} levels deep \u{2014} \
                     likely internal implementation, lower coupling risk"
                ),
            ));
        }
    }

    // Signal G (Wave 5, 2026-04-18): Rust workflow semantic advisory.
    // When writing a NEW `.rs` file, the `wave5_workflow` helper
    // inspects the about-to-be-written content (not disk — file does
    // not exist yet) and surfaces pub_surface, complexity band, and
    // unsafe/async counts. This lets Claude Code preview the
    // semantic weight of what it is about to introduce.
    //
    // Weight 1.3 (above wiring_predict=1.2, below speculative A) —
    // it is predictive but not blocking.
    // Wave 5.1: multi-lang advisory — Rust uses syn path, other
    // langs use tree-sitter `extract_symbols + analyze_quality`.
    if let Some(hint) = crate::wave5_workflow::code_workflow_hint(file_path, Some(content)) {
        signals.push((1.3, hint));
    }

    // ── Wave 15 (2026-04-18): Streak warning hints (parity w/ pre_edit) ──
    // Mirror of `pre_edit::compose_edit_context` Wave 14 wiring. When CC
    // is about to overwrite a file with a regression streak ≥ 3, the
    // ⚠ hint surfaces in pre_write signals at weight 1.4 (between
    // wave5_workflow=1.3 and the wiring_predict signals). Improvement
    // streaks emit a positive hint at weight 1.0.
    if let Some(warn) = crate::health_delta::streak_warning_hint(file_path) {
        signals.push((1.4_f32, warn));
    } else if let Some(positive) = crate::health_delta::improvement_streak_hint(file_path) {
        signals.push((1.0_f32, positive));
    }

    // ── Wave 11 (2026-04-18): Multi-lang health-delta pre-record ─────
    // Mirror of the `pre_edit` Wave 10 wiring: when CC overwrites an
    // existing file via the Write tool, capture the on-disk source's
    // unified quality score so the matching `post_write`/`post_edit`
    // (whichever fires next) can compute a signed delta. For a
    // brand-new file the disk read fails → silent no-op (delta only
    // makes sense for overwrites). Multi-lang via `record_pre_signals`
    // — covers every language `Lang::from_path` recognises.
    if let Ok(prev_src) = std::fs::read_to_string(file_path) {
        let _ = crate::health_delta::record_pre_signals(file_path, &prev_src);
    }

    signals
}

/// Detect anti-patterns in the content based on file language.
///
/// Delegates to [`crate::shared::antipatterns::detect_antipatterns`] which uses
/// `memchr::memmem` for SIMD-accelerated byte scanning (CC reduced from ~34 to ~8).
///
/// Returns scored signals. Empty vec = no issues found.
/// Test files are silently skipped (unwrap/assert are idiomatic in tests).
fn antipattern_signals(content: &str, rel_path: &str) -> Vec<(f32, String)> {
    // Skip test files — unwrap/assert/panic are idiomatic in tests.
    if crate::shared::quality::is_test_file(rel_path) {
        return Vec::new();
    }

    let lang = detect_language(rel_path);
    let mut issues = crate::shared::antipatterns::detect_antipatterns(content, lang);
    // Wire maybe_add_eval_check: appends dynamic code execution warning for JS/TS languages.
    crate::shared::antipatterns::maybe_add_eval_check(content.as_bytes(), lang, &mut issues);
    if issues.is_empty() {
        return Vec::new();
    }

    // Collapse all anti-pattern warnings into a single high-priority signal.
    vec![(2.0, issues.join(" | "))]
}

/// Quality depth signals: complexity and unwrap audit on write content.
///
/// Returns 0–2 signals: max_complexity (1.4), avg_complexity (1.2), or unwrap density (1.6).
/// `analyze_antipatterns` is intentionally excluded — already handled by `antipattern_signals`.
fn quality_depth_signals(content: &str, file_path: &str) -> Vec<(f32, String)> {
    // Skip test files — CC thresholds are not meaningful for test code.
    if crate::shared::quality::is_test_file(file_path) {
        return Vec::new();
    }

    let lang = detect_language(file_path);
    let mut signals: Vec<(f32, String)> = Vec::new();

    // Complexity gate: max_CC > 15 is write-blocking; avg_CC > 10 is informational.
    let complexity = touring_analysis::analyze_complexity(content, lang);
    if complexity.max_complexity > 15 {
        signals.push((
            1.4,
            format!(
                "quality_depth: max_CC={} (>15 threshold) — refactor before writing",
                complexity.max_complexity
            ),
        ));
    } else if complexity.avg_complexity > 10.0 {
        signals.push((
            1.2,
            format!(
                "quality_depth: avg_CC={:.1} — consider splitting complex functions",
                complexity.avg_complexity
            ),
        ));
    }

    // Unwrap density gate: risk_score > 0.3 means >3 unwraps per 100 lines.
    let unwrap_audit = touring_analysis::analyze_unwraps(content);
    if unwrap_audit.risk_score > 0.3 {
        let lines_preview = unwrap_audit
            .lines
            .iter()
            .take(3)
            .map(|l| l.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        signals.push((
            1.6,
            format!(
                "quality_depth: {} .unwrap() call(s) (risk={:.2}) at lines [{}] — use ? or .expect()",
                unwrap_audit.count, unwrap_audit.risk_score, lines_preview
            ),
        ));
    }

    signals
}

/// Signal I-5b: Build a callgraph context signal from the content being written.
///
/// Finds the symbol with the most callers in the new content, then delegates to
/// `callgraph_enrichment::enrich_with_callgraph` for full caller/callee/hotspot
/// detail formatted via `format_callgraph_context`. Score: 1.1.
///
/// Budget: pure in-memory (no I/O). Negligible latency (<1ms typical).
fn callgraph_signal_for_write(content: &str, file_path: &str) -> Option<(f32, String)> {
    // Detect language from extension — only Rust and Python supported
    let lang_str = match std::path::Path::new(file_path)
        .extension()
        .and_then(|e| e.to_str())
    {
        Some("rs") => "rust",
        Some("py") => "python",
        _ => return None,
    };

    // Find the top symbol by caller count, then enrich via the module.
    let lang: touring_code::ast::Lang = lang_str.parse().ok()?;
    let graph = touring_code::ast::call_graph::build_call_graph(content, lang);
    if graph.sites.is_empty() {
        return None;
    }

    let top_symbol: Option<String> = {
        let mut counts: Vec<(String, usize)> = graph
            .sites
            .iter()
            .fold(
                std::collections::HashMap::<String, std::collections::HashSet<String>>::new(),
                |mut acc, site| {
                    acc.entry(site.callee.clone())
                        .or_default()
                        .insert(site.caller.clone());
                    acc
                },
            )
            .into_iter()
            .map(|(callee, callers)| (callee, callers.len()))
            .collect();
        counts.sort_by_key(|b| std::cmp::Reverse(b.1));
        counts.into_iter().next().map(|(sym, _)| sym)
    };

    let sym = top_symbol.as_deref()?;
    let info = crate::callgraph_enrichment::enrich_with_callgraph(content, lang_str, Some(sym))?;
    Some((
        1.1,
        crate::callgraph_enrichment::format_callgraph_context(&info, sym),
    ))
}

/// Detect language from file extension.
///
/// Returns a language identifier suitable for `speculate_v2()` and other
/// AST utilities. Defaults to `"unknown"` for unrecognized extensions.
///
/// Delegates to [`crate::shared::detect_language::detect_language_or_unknown`].
fn detect_language(file_path: &str) -> &'static str {
    crate::shared::detect_language::detect_language_or_unknown(file_path)
}

/// Count pub symbols in source content for wiring prediction.
///
/// Heuristic scan — not full AST, but fast enough for a pre-hook.
/// Delegates per-language detection to focused predicate helpers.
fn count_pub_symbols(content: &str, language: &str) -> usize {
    match language {
        "rust" => content.lines().filter(|l| is_pub_rust_symbol(l)).count(),
        "python" => content.lines().filter(|l| is_pub_python_symbol(l)).count(),
        "typescript" | "javascript" => content.lines().filter(|l| is_export_js_symbol(l)).count(),
        _ => 0,
    }
}

/// Return `true` when `line` declares a public Rust symbol (fn, struct, enum, trait, type, const, static).
///
/// Excludes `pub(crate)` / `pub(super)` restricted visibility — those are internal.
#[inline]
fn is_pub_rust_symbol(line: &str) -> bool {
    let t = line.trim();
    (t.starts_with("pub fn ")
        || t.starts_with("pub struct ")
        || t.starts_with("pub enum ")
        || t.starts_with("pub trait ")
        || t.starts_with("pub type ")
        || t.starts_with("pub const ")
        || t.starts_with("pub static ")
        || t.starts_with("pub async fn "))
        && !t.starts_with("pub(")
}

/// Return `true` when `line` declares a top-level public Python symbol (def/class, non-private).
#[inline]
fn is_pub_python_symbol(line: &str) -> bool {
    let t = line.trim();
    (t.starts_with("def ") || t.starts_with("class "))
        && !t.starts_with("def _")
        && !t.starts_with("class _")
        && !line.starts_with(' ')
        && !line.starts_with('\t')
}

/// Return `true` when `line` is an ES module export declaration.
#[inline]
fn is_export_js_symbol(line: &str) -> bool {
    line.trim().starts_with("export ")
}

/// Detect bench/test targets that may need `required-features` in Cargo.toml.
///
/// When a file lives under `benches/` or `tests/` and imports crate symbols,
/// the corresponding `[[bench]]` or `[[test]]` entry in `Cargo.toml` should
/// declare `required-features` if those symbols are feature-gated. Without it,
/// the target compiles without the feature, causing cascading type errors.
///
/// Returns a scored signal at weight 1.2, or `None` when not applicable.
fn bench_required_features_signal(fp: &str, content: &str) -> Option<(f32, String)> {
    let target_kind = classify_bench_or_test_target(fp)?;
    if !has_non_std_imports(content) {
        return None;
    }

    let path = std::path::Path::new(fp);
    let filename = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    if filename.is_empty() {
        return None;
    }

    let cargo_toml = find_nearest_cargo_toml(path)?;
    let cargo_content = std::fs::read_to_string(&cargo_toml).ok()?;
    let section_key = if target_kind == "bench" {
        "[[bench]]"
    } else {
        "[[test]]"
    };

    if has_target_without_required_features(&cargo_content, section_key, filename) {
        Some((
            1.2,
            format!(
                "\u{26a0} CARGO: {target_kind} target '{filename}' uses imported symbols \
                 but [[{target_kind}]] entry lacks required-features. If those symbols are \
                 feature-gated, compilation will fail without the feature."
            ),
        ))
    } else {
        None
    }
}

/// Classify whether a file path is a bench or test target directory.
///
/// Returns `Some("bench")` or `Some("test")`, or `None` if neither.
fn classify_bench_or_test_target(fp: &str) -> Option<&'static str> {
    if fp.contains("/benches/") || fp.starts_with("benches/") {
        Some("bench")
    } else if fp.contains("/tests/") || fp.starts_with("tests/") {
        Some("test")
    } else {
        None
    }
}

/// Check if content has `use <crate>::` imports beyond std/core/alloc.
fn has_non_std_imports(content: &str) -> bool {
    content.lines().any(|line| {
        let t = line.trim();
        t.starts_with("use ")
            && !t.starts_with("use std::")
            && !t.starts_with("use core::")
            && !t.starts_with("use alloc::")
    })
}

/// Walk up from `start` to find the nearest `Cargo.toml`.
fn find_nearest_cargo_toml(start: &std::path::Path) -> Option<std::path::PathBuf> {
    let mut dir = if start.is_absolute() {
        start.parent()?
    } else {
        return None;
    };
    loop {
        let candidate = dir.join("Cargo.toml");
        if candidate.exists() {
            return Some(candidate);
        }
        dir = dir.parent()?;
    }
}

/// Check if `toml_src` has a `[[bench]]` or `[[test]]` entry matching
/// `target_name` that does NOT include `required-features`.
///
/// Uses heuristic line-by-line parsing to avoid a TOML crate dependency.
fn has_target_without_required_features(
    toml_src: &str,
    section_key: &str,
    target_name: &str,
) -> bool {
    let lines: Vec<&str> = toml_src.lines().collect();
    let mut i = 0;
    while i < lines.len() {
        let is_section = lines.get(i).is_some_and(|l| l.trim() == section_key);
        if is_section {
            let result = scan_toml_section(&lines, &mut i, target_name);
            if result {
                return true;
            }
            continue;
        }
        i += 1;
    }
    false
}

/// Scan a single TOML section starting after `lines[*pos]` (the header line).
///
/// Advances `*pos` to the start of the next section (or past EOF).
/// Returns `true` if the section matches `target_name` but lacks `required-features`.
fn scan_toml_section(lines: &[&str], pos: &mut usize, target_name: &str) -> bool {
    *pos += 1; // skip section header
    let mut has_matching_name = false;
    let mut has_required_features = false;
    while *pos < lines.len() {
        let Some(raw_line) = lines.get(*pos) else {
            break;
        };
        let line = raw_line.trim();
        if line.starts_with('[') {
            break;
        }
        if line.starts_with("name") && line.contains(target_name) {
            has_matching_name = true;
        }
        if line.starts_with("required-features") {
            has_required_features = true;
        }
        *pos += 1;
    }
    has_matching_name && !has_required_features
}

// ── P1.4 (2026-04-25): mpatch preview for pre_write hook ──────────────────

/// Returns a fuzzy patch preview when `TOURNING_MPATCH_PREVIEW=true` env var is set.
///
/// Used by the pre_write hook to surface diff confidence in the injected context,
/// helping Claude detect when a write will likely cause churn or fuzz-match failures.
/// Feature-gated by `mpatch-fuzzy` so touring-hooks remains lean when unused.
#[cfg(feature = "mpatch-fuzzy")]
pub fn mpatch_preview_if_enabled(
    source: &str,
    patch: &str,
) -> Option<crate::shared::mpatch_preview::PatchPreview> {
    if std::env::var("TOURNING_MPATCH_PREVIEW").as_deref() == Ok("true") {
        crate::shared::mpatch_preview::preview_patch(source, patch)
    } else {
        None
    }
}

/// Stub when `mpatch-fuzzy` feature is off.
#[cfg(not(feature = "mpatch-fuzzy"))]
pub fn mpatch_preview_if_enabled(
    _source: &str,
    _patch: &str,
) -> Option<crate::shared::mpatch_preview::PatchPreview> {
    None
}

// Wave C2 inversion (2026-06-10): `emit_b302_if_low_confidence_expansion` moved
// to touring-hooks-core::health_delta — it consumes only PatchComplexityDelta
// (core) + the leaf's mpatch_preview/gate_metrics. Re-exported here so the
// Wave-12 callers (`crate::pre_write::emit_b302_*`, tests) keep their paths.
pub use touring_hooks_core::health_delta::emit_b302_if_low_confidence_expansion;

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
#[path = "pre_write_tests.rs"]
mod tests;
