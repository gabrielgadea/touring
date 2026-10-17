//! Post-Write Hook — Quality verification after Claude creates or overwrites a file.
//!
//! After Claude writes a file, this hook:
//! 1. Records the write event in edit_history
//! 2. Indexes the file (symbols, imports, knowledge) — also registers wiring
//! 3. Runs consolidated quality analysis (speculate_v2, complexity, anti-patterns)
//! 4. verify_wiring_status: read-only orphan query (E3, no double wiring update)
//! 5. ANN memory store for edit context embeddings (E6)
//! 6. Block gate: 4+ antipatterns triggers Block response (E6)
//! 7. Returns feedback via additionalContext if issues found
//!
//! Target latency: <40ms (was <80ms before double-wiring elimination).

use super::knowledge::FileKnowledgeDB;
use super::runtime::{HookResponse, HookRuntime, make_relative};
use crate::schemas::validate_payload;
use crate::shared::gate_metrics;
use crate::shared::metadata_collector::FastMetadata;
use crate::shared::metadata_dedup::{DedupKey, MetadataDedup};
use crate::shared::parser_cache_global::{global_cache, global_invalidate};
#[allow(unused_imports)]
// ResultExt trait needed in scope for .unwrap_or_debug() calls on deref
use crate::shared::result_ext::{OptionExt, ResultExt};
use crate::triad_hook;
use std::sync::OnceLock;
use touring_code::ast::speculate_v2;
use touring_foundation::truncate_str;

/// Process-lifetime dedup cache for post_write metadata.
///
/// Keyed by (file_path, content_hash) where content_hash is BLAKE3 of file content.
/// Phase 2.2: Avoids redundant quality checks when the same file content is
/// written more than once in a session (mtime can change without content changing).
static WRITE_DEDUP: OnceLock<MetadataDedup> = OnceLock::new();

/// Flush the post_write dedup cache at session boundaries.
///
/// Called from `run_session_stop` to drain stale mtime entries so the next
/// session starts with a clean dedup state (no false-positive skips). Its only
/// caller lives in `session_hooks`, so it exists under the same feature.
#[cfg(feature = "session-hooks")]
pub(crate) fn flush_dedup() {
    if let Some(dedup) = WRITE_DEDUP.get() {
        dedup.clear();
    }
}

/// Persist call edges discovered in `source` as FileRelation records.
///
/// For each unique callee function found in the file's call graph,
/// upserts a FileRelation { source: rel_path, target: callee, relation_type: "calls" }.
/// Silently skips on parse failure (unknown language or empty graph).
fn persist_call_edges(db: &FileKnowledgeDB, rel_path: &str, source: &str, lang_str: &str) {
    let Ok(lang) = lang_str.parse::<touring_code::ast::Lang>() else {
        return;
    };
    let graph = touring_code::ast::call_graph::build_call_graph(source, lang);
    if graph.sites.is_empty() {
        return;
    }
    // Deduplicate callees: only one (rel_path, callee) relation per file.
    let mut seen = std::collections::HashSet::new();
    for site in &graph.sites {
        if seen.insert(site.callee.as_str()) {
            let rel = super::knowledge::FileRelation {
                source: rel_path.to_string(),
                target: site.callee.clone(),
                relation_type: "calls".to_string(),
            };
            if let Err(e) = db.upsert_relation(&rel) {
                tracing::debug!("persist_call_edges upsert failed for {rel_path}: {e}");
            }
        }
    }
}

/// Run the post-write hook (diverging version — for use by the CLI entry point).
///
/// Always exits 0.
#[tracing::instrument(skip(runtime, input), fields(hook = "post_write"))]
pub fn run(
    runtime: &HookRuntime,
    input: &serde_json::Value,
) -> Result<(), touring_hook_runtime::hook_runtime::HookDispatchError> {
    run_returning(runtime, input).emit()
}

/// Run the post-write hook, returning a `HookResponse` instead of diverging.
///
/// Used by the daemon to handle the hook without calling `process::exit`.
pub fn run_returning(runtime: &HookRuntime, input: &serde_json::Value) -> HookResponse {
    let result = run_returning_impl(runtime, input);
    // F-A1 (01/09) — check-compile signal: spawn a debounced `cargo check`
    // for the written file and deliver any pending verdict (P9 verify-after
    // measured at 17% — the loop closes by affordance, not persuasion).
    let file_path = input
        .pointer("/tool_input/file_path")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    match super::check_compile::default_state_dir() {
        Some(dir) if !file_path.is_empty() => super::check_compile::attach_check_signal(
            file_path,
            result,
            &dir,
            &triad_hook::session_of(input),
        ),
        _ => result,
    }
}

fn run_returning_impl(runtime: &HookRuntime, input: &serde_json::Value) -> HookResponse {
    // D9: Validate payload with typed schema — fail fast on malformed input.
    // Extract /tool_input first since payload schemas model the inner object.
    // Note: Empty file_path is a skip case (return Allow), not a validation error.
    let tool_input = match input.get("tool_input") {
        Some(v) => v,
        None => return HookResponse::Allow,
    };
    let validated = match validate_payload::<crate::schemas::PostWritePayload>(tool_input) {
        Ok(v) => v,
        Err(_) => return HookResponse::Allow, // Malformed or empty → skip silently
    };
    let file_path = validated.file_path.as_str();

    if file_path.is_empty() {
        return HookResponse::Allow;
    }

    let rel_path = make_relative(file_path, &runtime.project_root);

    // Step 1: Record the write event in edit_history.
    record_write_event(runtime, input, &rel_path);

    // If the write errored, don't index or verify — just record and return.
    let is_error = input
        .pointer("/tool_use_result/is_error")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    if is_error {
        return HookResponse::Allow;
    }

    // Extract content from payload first — available for BLAKE3 hash computation
    // without a disk read (post_write receives content in the JSON tool_input).
    let input_content = input
        .pointer("/tool_input/content")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    // F2 (hashtag library, 2026-08-11): re-harvest `#tags:` codetag anchors
    // from the written content into snippet memories. The wrapper owns the
    // skip decision — markerless content still syncs (to tombstone) when the
    // file's last anchor was just removed (D1, cross-audit 2026-08-23); the
    // sync is fail-open — a tagging hiccup never blocks a write.
    if let Some(content) = input_content.as_deref() {
        super::runtime::sync_codetag_anchors(&runtime.project_root, &rel_path, content);
    }

    // Step 2: Re-index the file (symbols, imports, relations).
    // BLAKE3 early-exit: skip reindex_file if content matches stored hash.
    // Content is already in input_content — no disk read needed.
    // Fallback: any hash/lookup failure → proceed normally.
    let should_skip_reindex = (|| -> Option<bool> {
        let content = input_content.as_deref()?;
        let mut hasher = blake3::Hasher::new();
        hasher.update(content.as_bytes());
        let new_hash = hasher.finalize().to_hex().to_string();
        let stored = runtime.ctx.knowledge.get_blake3_hash(&rel_path).ok()??;
        Some(stored.0 == new_hash)
    })()
    .unwrap_or(false);

    if should_skip_reindex {
        tracing::debug!("post_write: BLAKE3 unchanged for {rel_path} — skipping reindex");
    } else {
        if let Err(e) = reindex_file(runtime, file_path, &rel_path) {
            tracing::debug!("reindex_file failed for {file_path}: {e}");
        }
        // Warm the parser cache so subsequent pre_edit/pre_read calls hit cache.
        // EC57: Invalidate the stale pipeline entry first — moka's get_with returns
        // the existing entry if present, so without invalidate the "warm" is a no-op
        // after a file write that changed content.
        let cache = global_cache();
        let path_buf = std::path::PathBuf::from(file_path);
        global_invalidate(&path_buf);
        let _ = cache.get_or_create(&path_buf);
        // EC5: Fire-and-forget async record_edit to AsyncFileKnowledgeDB.
        // Only fires when content actually changed (BLAKE3 miss path).
        if let Some(adb) = runtime.ctx.async_knowledge.as_ref().cloned()
            && let Ok(handle) = tokio::runtime::Handle::try_current()
        {
            let edit = crate::knowledge::EditEvent {
                file_path: rel_path.to_string(),
                edit_type: "write".to_string(),
                summary: None,
                error_pattern: None,
                edited_at: chrono::Local::now().to_rfc3339(),
            };
            drop(handle.spawn(async move {
                let _ = adb.record_edit(&edit).await;
            }));
        }
        // Tantivy FTS: upsert the written file into the full-text index.
        // Runs only on BLAKE3 miss (content actually changed). Graceful — failure
        // is logged and never propagates to the hook caller.
        // The file document is stored under the index key — project-relative or
        // `@companion/<name>/<rel>` — and never for a path the walker refuses: a
        // raw `make_relative` spelling left absolute-path documents no rebuild
        // could retire (2026-09-13).
        #[cfg(feature = "tantivy-fts")]
        if let Ok(key) = crate::shared::reindex::admission_refusal(runtime, &rel_path) {
            let doc = crate::tantivy_index::SymbolDoc {
                symbol_name: key.clone(),
                file_path: key.clone(),
                symbol_kind: "file".to_string(),
                module_path: None,
                docstring: None,
                line_number: 0,
                language: crate::tantivy_index::extension_to_language(&key),
                visibility: None,
                crate_name: None,
                blake3_hash: None,
                import_count: None,
                export_count: None,
                cognitive_score: None,
                functional_signature: None,
                community_id: None,
            };
            // Try async stream first (non-blocking, amortized commit).
            // Fall back to synchronous upsert when actor is not running or
            // channel is full (backpressure drop recorded in stream metrics).
            // A raiz acompanha o documento nos DOIS caminhos — ver post_edit.rs.
            if !crate::shared::tantivy_stream::try_send_symbol(
                runtime.project_root.clone(),
                doc.clone(),
            ) && let Some(tantivy_idx) =
                crate::tantivy_index::tantivy_for(Some(&runtime.project_root))
                && let Err(e) = tantivy_idx.upsert_symbol(&doc)
            {
                tracing::debug!("tantivy upsert failed for {key}: {e}");
            }
        }
    }

    // NLP enrichment: fire-and-forget analysis for text/markdown files.
    // Uses AsyncTaskBuilder via nlp_bridge to run NlpPipeline::process_document
    // in a Rayon background task — zero latency cost on the hook response path.
    #[cfg(feature = "nlp-enrichment")]
    if matches!(
        std::path::Path::new(file_path)
            .extension()
            .and_then(|e| e.to_str()),
        Some("md" | "txt" | "rst" | "adoc")
    ) && let Ok(text) = std::fs::read_to_string(file_path)
    {
        crate::nlp_bridge::analyze_text_async(rel_path.to_string(), text);
    }

    // EC_sev: Append a CRDT symbol-level event to `symbol_events_log` for auditing
    // and evolution tracking. sequence_id uniqueness: nanos timestamp + rel_path.
    // UNIQUE constraint violation (rare duplicate) is silently ignored via `let _`.
    {
        let ts_nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let seq_id = format!("write:{ts_nanos}:{rel_path}");
        let session_id_ev = std::env::var("CLAUDE_SESSION_ID").ok();
        let _ = runtime.ctx.knowledge.insert_symbol_event(
            &seq_id,
            &rel_path,
            None,
            "write",
            None,
            session_id_ev.as_deref(),
        );
    }

    // FastMetadata + MetadataDedup: record file state after write for dedup tracking.
    // Phase 2.2: Uses BLAKE3 content hash as primary key for accurate deduplication.
    // mtime can change without content changing (e.g., touch, copyover), causing
    // false-positive duplicate detection. BLAKE3 of content is stable.
    // Fallback: mtime_epoch if content hashing fails (preserves original behavior).
    // Graceful: never blocks the hook on failure.
    let key_hash = if let Some(content) = input_content.as_ref() {
        let hash = blake3::hash(content.as_bytes()).to_hex().to_string();
        tracing::debug!(
            "post_write: BLAKE3 content hash for {rel_path}: {}",
            &hash[..8]
        );
        hash
    } else if let Ok(meta) = FastMetadata::from_path(std::path::Path::new(file_path)) {
        meta.mtime_epoch.to_string()
    } else {
        String::new()
    };
    if !key_hash.is_empty() {
        let dedup = WRITE_DEDUP.get_or_init(MetadataDedup::new);
        let key = DedupKey {
            file_path: rel_path.to_string(),
            content_hash: key_hash,
        };
        if dedup.check_and_mark(key) {
            gate_metrics::record_metadata_cache_hit();
            tracing::debug!("post_write: duplicate write detected for {rel_path} (content match)");
        }
    }

    // Step 2b-c: Layer7 prediction + ACO wiring deposits.
    record_layer7_and_aco(runtime, file_path, &rel_path);

    // Step 3: Quality verification + snapshot.

    // E5: Persist call graph edges to knowledge DB for graph-based queries.
    let lang_str = crate::shared::detect_language::detect_language(&rel_path);
    if let (Some(source), Some(lang)) = (&input_content, lang_str) {
        persist_call_edges(&runtime.ctx.knowledge, &rel_path, source, lang);
    }

    let mut feedback = super::post_feedback::Feedback {
        issues: collect_quality_issues(runtime, file_path, &rel_path, input_content.as_deref()),
        signals: Vec::new(),
    };

    // ── Wave 18 (2026-04-18): Invalidate query cache for the written file ──
    // By the RELATIVE path: `ast meta` keys it relative, `ast overview` keys what
    // the caller sent (often absolute) — the absolute path contains the relative
    // one, the reverse is false, so only this form reaches both.
    crate::shared::query_cache::invalidate_by_path(&rel_path);

    // ── Wave 12 (2026-04-18): Health-delta hint via cache cleanup ─────
    // Mirror of post_edit V7. The Write tool may fire pre_write
    // (which records via Wave 11 multi-lang `record_pre_signals`)
    // without a matching post_edit (Edit tool), so post_write must
    // CONSUME the cache entry to prevent a leak across daemon
    // sessions. We push the formatted delta hint to `all_issues` so
    // CC sees the regression/improvement signal — same surface as
    // V1-V6 advisories. No reward inject because `&HookRuntime` is
    // immutable here; reward path remains owned by post_edit.
    if let Some(content) = input_content.as_deref()
        && let Some(delta) = crate::health_delta::compute_signals_delta(file_path, content)
    {
        let hint = crate::health_delta::format_delta_hint(&delta);
        // A regression is a defect of this write; a first observation or a flat delta is
        // information (S28 — it used to be counted as an issue).
        if delta.is_regression() {
            feedback.issues.push(hint);
        } else {
            feedback.signals.push(hint);
        }
    }

    // Step 4: Cognitive enrichment (risk + gotchas) — parity with post_edit.
    if let Some(ref cognitive) = runtime.cognitive {
        let enriched = crate::shared::signals::enrich_with_cognitive(
            cognitive,
            file_path,
            false,
            crate::shared::signals::payload_session(input),
        );
        if !enriched.is_empty() {
            feedback.signals.push(enriched);
        }
    }

    // Step 4b: PII Scan — detect Brazilian PII in written file (PostToolUse).
    // Scans the full file content after write for CPF, CNPJ, RG, CNH, SUS, SEI, email, phone.
    // All severities reported since the write has been applied.
    if let Some(ref content) = input_content {
        let findings = runtime.ctx.pii_scanner.scan_text(content);
        if !findings.is_empty() {
            let pii_ctx = format_pii_findings_context_post(&findings);
            feedback.issues.push(pii_ctx);
        }
    }

    // Step 5: Apply CILA-aware budget and return feedback.
    let cila_level: u8 = runtime
        .ctx
        .result_cache
        .get_result("__meta__", "__session_cila_level__")
        .and_then(|s| s.parse().ok())
        .unwrap_or_debug(2, "post_write: cila_level fallback");
    // Issues first; signals get what the budget leaves. What is elided is declared (S28).
    let budget = crate::shared::cila::cila_budget_write(cila_level);
    let issue_count = feedback.issues.len();
    let used = super::post_feedback::fit_budget(&mut feedback.issues, budget);
    let elided = issue_count - feedback.issues.len();
    super::post_feedback::fit_budget(&mut feedback.signals, budget.saturating_sub(used));

    // TRIAD (S12, D1-A): the S28 gate on NEW anti-patterns, plus the declared abstention.
    apply_s28_gate(
        input,
        file_path,
        &rel_path,
        lang_str,
        input_content.as_deref(),
        &feedback,
        elided,
    )
}

/// The S28 gate for `post_write`: count anti-patterns this write INTRODUCED (before × after),
/// block past the threshold, and — when no baseline was available — declare the abstention
/// instead of passing as if the check had run.
///
/// Split out of `run_returning_impl` (which was already at CC=44) so the gate's shape stays
/// readable: it has exactly two outcomes, judged or declared.
fn apply_s28_gate(
    input: &serde_json::Value,
    file_path: &str,
    rel_path: &str,
    lang_str: Option<&str>,
    input_content: Option<&str>,
    feedback: &super::post_feedback::Feedback,
    elided: usize,
) -> HookResponse {
    // The baseline is read from this process's memory first and from the per-user on-disk
    // record second, so a standalone pre-write still arms a daemon (or standalone)
    // post-write — 17/10/2026, where the split silently disabled this gate under load.
    let triad = triad_hook::take_lookup(&triad_hook::session_of(input), file_path);
    // Match the LOOKUP, not `Option<&str>`: `before()` yields `None` both for
    // `Baseline::Absent` (a new file — every occurrence IS new) and for a genuine
    // abstention, and routing both through one arm made a new file count zero silently.
    // `new_antipatterns` already encodes the right semantics for `None` ("the file did not
    // exist, so everything counts"), so `Absent` must reach it, not the abstain arm.
    let (new_antipatterns, abstention) = match (&triad.lookup, input_content) {
        (triad_hook::TriadLookup::Found(baseline), Some(after)) => (
            super::post_feedback::new_antipatterns(
                baseline.as_before(),
                after,
                lang_str.unwrap_or(""),
                file_path,
            ),
            None,
        ),
        // A baseline we hold but no content to compare against: abstain, never block blind.
        (triad_hook::TriadLookup::Found(_), None) => (
            0,
            Some(super::post_feedback::EDIT_BASELINE_ABSTAINED.to_string()),
        ),
        (triad_hook::TriadLookup::Absent(reason), _) => (0, Some(reason.note())),
    };

    let response = build_response(feedback, elided, rel_path, new_antipatterns);

    match abstention {
        // An abstention is a MEASUREMENT THAT DID NOT HAPPEN and must be visible — silently
        // passing is how 5 new antipatterns got written in under load. Declared as a signal,
        // never an issue: it is a property of the hook, not of the file.
        Some(note) => super::post_feedback::attach_signal(response, &note),
        // Never restores the file — PostToolUse cannot undo; a Block gains the write's
        // +/− summary. The baseline is enough for that (and survives the process boundary),
        // so the note appears whether or not this process armed it.
        None => match &triad.lookup {
            triad_hook::TriadLookup::Found(baseline) => {
                triad_hook::run_post_write(file_path, baseline, response)
            }
            // Unreachable: `Absent` always carries a reason, and every reason is declared
            // above. Kept so a future variant cannot reintroduce a silent pass.
            triad_hook::TriadLookup::Absent(reason) => {
                tracing::warn!(
                    path = %rel_path,
                    reason = ?reason,
                    "TRIAD: S28 absent without a declared abstention"
                );
                super::post_feedback::attach_signal(response, &reason.note())
            }
        },
    }
}

/// Layer7 prediction + ACO wiring deposits after a successful write.
///
/// Records the edit into the prediction engine, updates the co-edit graph
/// with the previously edited file, and deposits ACO pheromone for the
/// written path. Must be called after re-indexing and before quality checks.
fn record_layer7_and_aco(runtime: &HookRuntime, file_path: &str, rel_path: &str) {
    // Layer7: record edit for anticipatory context injection.
    runtime.infra.prediction.record_edit(file_path);

    // Co-edit graph: track prev→current file pairs.
    // Extract prev BEFORE aco_wiring.lock() to avoid double mutable borrow.
    let prev_file = runtime
        .infra
        .last_edited_file
        .replace(Some(file_path.to_string()));
    if let Some(ref pf) = prev_file {
        runtime.infra.prediction.record_co_edit(pf, file_path);
        // E12: also feed co-access into PredictiveFocusCache for ACO pheromone tracking.
        runtime
            .infra
            .predictive_focus
            .observe_co_access(pf, file_path);
    }

    // ANN Memory: persist write context for future semantic recall.
    // Stores file path + "wrote" as a memory with path-hash embedding —
    // parity with post_edit which stores "edited:<tool>:<path>".
    if let Ok(mut ann_guard) = runtime.ctx.ann_recall.try_borrow_mut()
        && let Some(ref mut ann) = *ann_guard
    {
        let embedding = crate::ann_memory::path_hash_embedding(file_path);
        let entry =
            crate::ann_memory::MemoryEntry::new(rel_path, format!("wrote:{}", rel_path), embedding);
        if let Err(e) = ann.add_memory(entry) {
            tracing::debug!("ANN add_memory failed for {rel_path}: {e}");
        }
    }

    // ACO Wiring deposits (fire-and-forget).
    // Extract heat OUTSIDE lock scope to avoid holding the lock longer than needed.
    let heat = match runtime.aco_wiring.lock() {
        Ok(wiring) => {
            wiring.deposit_file_edit(rel_path);
            wiring
                .bus
                .get(&touring_intelligence::rl::aco::PheroKey::FilePath(
                    rel_path.to_string(),
                ))
        }
        _ => 0.0,
    };
    if heat > 0.0 {
        runtime.infra.prediction.update_file_heat(rel_path, heat);
    }
}

// ── PII Helper ─────────────────────────────────────────────────────────────

/// Format PII scan findings as an issue string for PostToolUse injection.
///
/// Unlike PreToolUse (which suppresses low-severity), PostToolUse shows ALL findings
/// since the write has already been applied and full verification is needed.
/// Format: `PII: N finding(s) — 2 high (cpf, cpf), 1 medium (email), 1 low (phone)`
fn format_pii_findings_context_post(findings: &[crate::pii::PIIFinding]) -> String {
    let high: Vec<_> = findings.iter().filter(|f| f.severity == "high").collect();
    let medium: Vec<_> = findings.iter().filter(|f| f.severity == "medium").collect();
    let low: Vec<_> = findings.iter().filter(|f| f.severity == "low").collect();

    let total = findings.len();
    let mut parts = Vec::new();

    if !high.is_empty() {
        let unique: std::collections::HashSet<_> =
            high.iter().map(|f| f.pattern_name.as_str()).collect();
        parts.push(format!(
            "{} high ({})",
            high.len(),
            unique.into_iter().collect::<Vec<_>>().join(", ")
        ));
    }
    if !medium.is_empty() {
        let unique: std::collections::HashSet<_> =
            medium.iter().map(|f| f.pattern_name.as_str()).collect();
        parts.push(format!(
            "{} medium ({})",
            medium.len(),
            unique.into_iter().collect::<Vec<_>>().join(", ")
        ));
    }
    if !low.is_empty() {
        parts.push(format!("{} low", low.len()));
    }

    format!("PII: {} finding(s) — {}", total, parts.join(", "))
}

/// Record the write event in knowledge DB with full context.
fn record_write_event(runtime: &HookRuntime, input: &serde_json::Value, rel_path: &str) {
    let content = input
        .pointer("/tool_input/content")
        .and_then(|v| v.as_str())
        .unwrap_or_debug("", "post_write: content from tool_input");

    let language = crate::shared::detect_language::detect_language(rel_path).unwrap_or("unknown");
    let session_id = std::env::var("CLAUDE_SESSION_ID").ok();

    let line_count = content.lines().count();
    let summary = if line_count > 0 {
        Some(format!("wrote {line_count} lines"))
    } else {
        None
    };

    if let Err(e) = runtime.ctx.knowledge.record_edit_full(
        rel_path,
        "Write",
        summary.as_deref(),
        None, // error_pattern
        Some(language),
        None, // symbol_context
        session_id.as_deref(),
    ) {
        tracing::debug!("record_edit_full failed for {rel_path}: {e}");
    }
    // T1-S3: Record file access for file_access_log (feeds recent_accessed_files in post_edit).
    if let Some(ref sid) = session_id
        && let Err(e) = runtime.ctx.knowledge.record_access(rel_path, sid)
    {
        tracing::debug!("record_access failed for {rel_path}: {e}");
    }
    // D1.6: Emit activity event before record_write_event returns.
    crate::activity_hook::emit_post_write(&runtime.project_root, rel_path, language);
}

/// Run all quality verification checks and store quality snapshot.
///
/// Returns collected issues from speculate, antipatterns, complexity, and wiring.
fn collect_quality_issues(
    runtime: &HookRuntime,
    file_path: &str,
    rel_path: &str,
    content: Option<&str>,
) -> Vec<String> {
    let lang_str = crate::shared::detect_language::detect_language_or_unknown(file_path);
    let mut all_issues: Vec<String> = Vec::new();

    // V1 + V2: run in parallel — both are CPU-bound and read-only.
    let fp1 = file_path.to_owned();
    let fp2 = file_path.to_owned();
    let ls1 = lang_str.to_owned();
    let ls2 = lang_str.to_owned();
    let c1 = content.map(|s| s.to_owned());
    let c2 = content.map(|s| s.to_owned());

    let (spec_issues, ap_issues) = rayon::join(
        move || verify_speculative(&fp1, &ls1, c1.as_deref()),
        move || verify_antipatterns(&fp2, &ls2, c2.as_deref()),
    );
    all_issues.extend(spec_issues);
    all_issues.extend(ap_issues);

    // FIX7: Compute quality snapshot ONCE from pre-loaded content, then derive
    // both complexity issues AND evolution tracking from the same result —
    // eliminates a redundant `fs::read_to_string` + `analyze_file_quality` call.
    let quality_snapshot = content
        .and_then(|src| {
            crate::shared::quality::measure_quality_snapshot_from_source(src, file_path)
        })
        .or_else(|| crate::shared::quality::measure_quality_snapshot(file_path));

    // V3: Complexity baseline — derive from snapshot instead of a separate call.
    if let Some(ref qa) = quality_snapshot
        && qa.max_complexity > 10
    {
        let names = qa
            .complex_symbols
            .iter()
            .take(3)
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        all_issues.push(format!(
            "complexity: CC_max={} in [{}] — consider refactoring",
            qa.max_complexity, names
        ));
    }

    // V4: Wiring orphan detection (registration already done by reindex_file).
    all_issues.extend(verify_wiring_status(&runtime.ctx.knowledge, rel_path));

    // Quality snapshot for evolution tracking (reuses the same snapshot).
    if let Some(ref qa) = quality_snapshot {
        let note = format!(
            "quality: CC_max={}, CC_avg={:.1}, symbols={}, complex={}",
            qa.max_complexity, qa.avg_complexity, qa.symbol_count, qa.high_complexity_count
        );
        if let Err(e) = runtime.ctx.knowledge.replace_quality_note(rel_path, &note) {
            tracing::debug!("replace_quality_note failed for {rel_path}: {e}");
        }
    }

    // ── HEALTH signal: touring-analysis dimensional summary ──
    // Only emitted when issues are accumulating (< 6 to avoid noise) and
    // content is available — avoids a redundant disk read.
    // Uses hook_path() budget (<40ms) to stay within post_write latency SLA.
    if all_issues.len() < 6
        && let Some(src) = content
    {
        let files = vec![(file_path.to_string(), src.to_string(), lang_str.to_string())];
        let builder = touring_analysis::pipeline::AnalysisPipelineBuilder::new(
            runtime.ctx.knowledge.conn_ref(),
        )
        .config(touring_analysis::engine::AnalysisConfig::hook_path())
        .with_files(files);
        // Wire symbol index for blast-radius dimension (enables BFS blast analysis).
        // touring-analysis is always linked with default features (includes blast-radius).
        let builder = if let Some(ref idx) = runtime.infra.symbol_index {
            builder.with_symbol_index(std::sync::Arc::new(idx.clone()))
        } else {
            builder
        };
        let pipeline = builder.build();
        let report = pipeline.run(rel_path);
        let summary = report.to_analysis_summary();
        if !summary.passes {
            all_issues.push(format!("HEALTH {}", summary.one_liner()));
        }
    }

    all_issues
}

/// Build the final `HookResponse` from the typed feedback (S28).
///
/// The shared block gate runs first, on the NEW antipattern occurrences this write
/// introduced (before × after); otherwise the feedback renders with a header that counts
/// issues only — signals follow the `ℹ` marker, and budget elision is declared.
fn build_response(
    feedback: &super::post_feedback::Feedback,
    elided: usize,
    rel_path: &str,
    new_antipatterns: usize,
) -> HookResponse {
    use super::post_feedback::{Mutation, render_feedback};
    if let Some(block) = super::post_feedback::check_block_gate(
        new_antipatterns,
        rel_path,
        Mutation::Write,
        feedback,
    ) {
        return block;
    }
    let cap = |items: &[String]| -> Vec<String> {
        items
            .iter()
            .map(|s| truncate_str(s, 120).to_string())
            .collect()
    };
    match render_feedback(
        "post-write",
        &cap(&feedback.issues),
        elided,
        &cap(&feedback.signals),
    ) {
        Some(context) => HookResponse::Context {
            context,
            event_name: Some("PostToolUse".to_string()),
        },
        None => HookResponse::Allow,
    }
}

// ── Verification Functions ────────────────────────────────────────────────

/// V1: Speculative validation — run `speculate_v2` on the written file.
///
/// Collects diagnostics from any failed validation layers (syntax, symbol
/// resolution, structural invariants, import completeness).
fn verify_speculative(file_path: &str, lang_str: &str, content: Option<&str>) -> Vec<String> {
    let source = match load_source_for_verify(file_path, content) {
        Some(s) if !s.is_empty() => s,
        _ => return Vec::new(),
    };

    let lang = match lang_str.parse::<touring_code::ast::Lang>() {
        Ok(l) => l,
        Err(_) => return Vec::new(),
    };

    let result = speculate_v2(&source, lang, None, None);
    if result.all_passed {
        return Vec::new();
    }

    collect_failed_layer_issues(&result.layers)
}

/// Load file source for verification, preferring in-memory `content` over a
/// disk read. Returns `None` only when the disk read fails.
fn load_source_for_verify(file_path: &str, content: Option<&str>) -> Option<String> {
    if let Some(c) = content {
        return Some(c.to_string());
    }
    std::fs::read_to_string(file_path).ok()
}

/// Collect diagnostic messages from all failed speculate layers.
///
/// At most 3 diagnostics per failed layer are included to keep feedback
/// concise. When a layer has no diagnostics, a generic `"<Layer> failed"`
/// message is emitted instead.
fn collect_failed_layer_issues(layers: &[touring_code::ast::LayerResult]) -> Vec<String> {
    let mut issues = Vec::new();
    for layer in layers {
        if layer.passed {
            continue;
        }
        let diag = if layer.diagnostics.is_empty() {
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
        issues.push(format!("speculate({:?}): {}", layer.layer, diag));
    }
    issues
}

/// V2: Anti-pattern detection — SIMD-accelerated via `shared::antipatterns`.
///
/// Skips test files (unwrap/assert patterns are idiomatic in tests).
/// Uses `content` from input when available to avoid a disk read.
fn verify_antipatterns(file_path: &str, lang_str: &str, content: Option<&str>) -> Vec<String> {
    if crate::shared::quality::is_test_file(file_path) {
        return Vec::new();
    }
    let owned;
    let source: &str = if let Some(c) = content {
        c
    } else {
        owned = match std::fs::read_to_string(file_path) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        &owned
    };
    crate::shared::antipatterns::detect_antipatterns_at(
        source,
        lang_str,
        std::path::Path::new(file_path),
    )
}

/// V3: Complexity baseline — measure file quality and warn if CC > 10.
///
/// When `preloaded` is `Some`, uses the already-read content instead of
/// hitting the filesystem — avoids a redundant `fs::read_to_string` when
/// the caller already has the file content (e.g. from `tool_input/content`).
///
/// FIX7: Production code now uses the consolidated quality_snapshot path in
/// `collect_quality_issues`. This function is retained for direct unit testing.
#[cfg(test)]
fn verify_complexity(file_path: &str, preloaded: Option<&str>) -> Vec<String> {
    let owned;
    let source = if let Some(pre) = preloaded {
        pre
    } else {
        owned = match std::fs::read_to_string(file_path) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        &owned
    };

    let metrics = match super::ast_bridge::analyze_file_quality(source, file_path) {
        Some(m) => m,
        None => return Vec::new(),
    };

    let mut issues = Vec::new();

    if metrics.max_complexity > 10 {
        let names = metrics
            .complex_symbols
            .iter()
            .take(3)
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        issues.push(format!(
            "complexity: CC_max={} in [{}] — consider refactoring",
            metrics.max_complexity, names
        ));
    }

    issues
}

/// V4: Query wiring status and detect orphan pub symbols.
///
/// This is a **read-only** check — symbol registration is already performed
/// by [`reindex_file`] (via `wiring::update_wiring_after_edit`).  Calling
/// `update_wiring_after_edit` again here would be a redundant write.
fn verify_wiring_status(db: &FileKnowledgeDB, rel_path: &str) -> Vec<String> {
    let status = match db.module_wiring_status(rel_path) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };

    let mut issues = Vec::new();

    if !status.orphan_symbols.is_empty() && status.integration_score < 1.0 {
        let orphan_list = status.orphan_symbols.join(", ");
        let short = truncate_str(&orphan_list, 80);
        // R5-S4: Suggest `touring generate plan-suggest` CLI to scaffold a wiring plan.
        // Claude Code sees this context and can immediately invoke the generator to
        // create a consumer plan — automating the wiring step after code generation.
        let first_orphan = status
            .orphan_symbols
            .first()
            .map(String::as_str)
            .unwrap_or("symbol");
        issues.push(format!(
            "wiring({:.0}%): {} orphan pub symbol(s) [{}] — run: touring generate plan-suggest --intent \"wire {} into a consumer caller\"",
            status.integration_score * 100.0,
            status.orphan_symbols.len(),
            short,
            first_orphan,
        ));
    }

    issues
}

// ── Helper Functions ──────────────────────────────────────────────────────

/// Re-index a file after write (update knowledge with current content).
///
/// Delegates to [`crate::shared::reindex::reindex_file_with_old`] (no old_content
/// available in post_write — incremental parse applies only on the second edit).
fn reindex_file(
    runtime: &HookRuntime,
    abs_path: &str,
    rel_path: &str,
) -> Result<(), crate::shared::reindex::ReindexError> {
    crate::shared::reindex::reindex_file_with_old(runtime, abs_path, rel_path, None)
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    #![allow(clippy::indexing_slicing)]
    use super::*;
    use crate::knowledge::{FileKnowledge, FileKnowledgeDB};
    use crate::shared::detect_language::detect_language_or_unknown as detect_language;
    use tempfile::TempDir;

    fn setup() -> (TempDir, FileKnowledgeDB) {
        let tmp = TempDir::new().unwrap();
        let db = FileKnowledgeDB::new(&tmp.path().join("test.db")).unwrap();
        (tmp, db)
    }

    // ── Integration Tests (with HookRuntime) ─────────────────────────────

    #[test]
    fn test_post_write_silent_for_empty_path() {
        let input = serde_json::json!({
            "tool_name": "Write",
            "tool_input": {
                "file_path": "",
                "content": "some content"
            }
        });
        let tmp = TempDir::new().unwrap();
        let runtime = crate::runtime::HookRuntime::new(tmp.path()).unwrap();
        let response = run_returning(&runtime, &input);
        assert!(
            matches!(response, HookResponse::Allow),
            "empty path should produce Allow"
        );
    }

    #[test]
    fn test_post_write_records_event() {
        let tmp = TempDir::new().unwrap();
        let runtime = crate::runtime::HookRuntime::new(tmp.path()).unwrap();

        // Create a simple file to write.
        let file_path = tmp.path().join("src").join("main.rs");
        std::fs::create_dir_all(file_path.parent().unwrap()).unwrap();
        let content = "fn main() {\n    println!(\"hello\");\n}\n";
        std::fs::write(&file_path, content).unwrap();

        let input = serde_json::json!({
            "tool_name": "Write",
            "tool_input": {
                "file_path": file_path.to_str().unwrap(),
                "content": content
            }
        });

        let _response = run_returning(&runtime, &input);

        // The edit_history should have at least one entry.
        let rel = make_relative(file_path.to_str().unwrap(), &runtime.project_root);
        let edits = runtime
            .ctx
            .knowledge
            .recent_edits(&rel, 10)
            .unwrap_or_default();
        assert!(
            !edits.is_empty(),
            "write event should be recorded in edit_history"
        );
    }

    #[test]
    fn test_post_write_reindexes_file() {
        let tmp = TempDir::new().unwrap();
        let runtime = crate::runtime::HookRuntime::new(tmp.path()).unwrap();

        // Create a Rust file with symbols.
        let src_dir = tmp.path().join("src");
        std::fs::create_dir_all(&src_dir).unwrap();
        let file_path = src_dir.join("indexed.rs");
        let content = "pub fn greet(name: &str) -> String {\n    format!(\"Hello, {name}!\")\n}\n\nfn private_helper() {}\n";
        std::fs::write(&file_path, content).unwrap();

        let rel_path = "src/indexed.rs";
        let result = reindex_file(&runtime, file_path.to_str().unwrap(), rel_path);
        assert!(result.is_ok(), "reindex should succeed");

        // Verify the file is now in the knowledge DB.
        let lookup = runtime.ctx.knowledge.lookup(rel_path);
        assert!(lookup.is_ok(), "file should be indexed in knowledge DB");
        if let Ok(Some(k)) = lookup {
            assert!(k.symbol_count > 0, "should have extracted symbols");
        }
    }

    // ── Anti-pattern Detection Tests ────────────────────────────────────

    #[test]
    fn test_post_write_detects_unwrap() {
        let tmp = TempDir::new().unwrap();
        let file_path = tmp.path().join("risky.rs");
        let content =
            "pub fn risky() -> String {\n    std::fs::read_to_string(\"f\").unwrap()\n}\n";
        std::fs::write(&file_path, content).unwrap();

        let issues = verify_antipatterns(file_path.to_str().unwrap(), "rust", None);
        assert!(
            issues.iter().any(|i| i.contains(".unwrap()")),
            "should detect .unwrap(): {issues:?}"
        );
    }

    #[test]
    fn test_post_write_detects_todo() {
        let tmp = TempDir::new().unwrap();
        let file_path = tmp.path().join("placeholder.rs");
        let content = "pub fn placeholder() {\n    todo!()\n}\n";
        std::fs::write(&file_path, content).unwrap();

        let issues = verify_antipatterns(file_path.to_str().unwrap(), "rust", None);
        assert!(
            issues.iter().any(|i| i.contains("todo!()")),
            "should detect todo!(): {issues:?}"
        );
    }

    #[test]
    fn test_post_write_python_bare_except() {
        let tmp = TempDir::new().unwrap();
        let file_path = tmp.path().join("handler.py");
        let content = "def risky():\n    try:\n        do_something()\n    except:\n        pass\n";
        std::fs::write(&file_path, content).unwrap();

        let issues = verify_antipatterns(file_path.to_str().unwrap_or_default(), "python", None);
        assert!(
            issues.iter().any(|i| i.contains("except")),
            "should detect bare except: {issues:?}"
        );
    }

    #[test]
    fn test_post_write_clean_code_no_feedback() {
        let tmp = TempDir::new().unwrap();
        let file_path = tmp.path().join("clean.rs");
        let content = "pub fn add(a: i32, b: i32) -> i32 {\n    a + b\n}\n";
        std::fs::write(&file_path, content).unwrap();

        // Anti-patterns should be empty for clean code.
        let issues = verify_antipatterns(file_path.to_str().unwrap(), "rust", None);
        assert!(
            issues.is_empty(),
            "clean code should produce no antipattern issues: {issues:?}"
        );
    }

    #[test]
    fn test_post_write_detects_syntax_error() {
        let tmp = TempDir::new().unwrap();
        let file_path = tmp.path().join("broken.rs");
        // Incomplete function — missing closing brace.
        let content = "fn broken() {\n    let x = 42;\n";
        std::fs::write(&file_path, content).unwrap();

        let issues = verify_speculative(file_path.to_str().unwrap_or_default(), "rust", None);
        // speculate_v2 should flag syntax issues for incomplete code.
        // (Behavior depends on tree-sitter; at minimum no panic.)
        let _ = issues;
    }

    // ── Complexity Tests ────────────────────────────────────────────────

    #[test]
    fn test_post_write_complexity_warning() {
        let tmp = TempDir::new().unwrap();
        let file_path = tmp.path().join("complex.rs");
        // Generate a function with many branches to trigger CC > 10.
        let mut content = String::from("pub fn complex(x: i32) -> &'static str {\n");
        for i in 0..12 {
            content.push_str(&format!("    if x == {i} {{ return \"case_{i}\"; }}\n"));
        }
        content.push_str("    \"default\"\n}\n");
        std::fs::write(&file_path, content).unwrap();

        let issues = verify_complexity(file_path.to_str().unwrap(), None);
        // CC depends on tree-sitter analysis; the test verifies no panic.
        let _ = issues;
    }

    // ── Wiring Tests ────────────────────────────────────────────────────

    #[test]
    fn test_post_write_wiring_orphan_detection() {
        let (_tmp, db) = setup();

        // Simulate a file with a pub symbol and no consumers.
        let knowledge = FileKnowledge {
            file_path: "src/new_module.rs".into(),
            language: Some("rust".into()),
            symbols_json: None,
            imports_json: Some(r#"[]"#.into()),
            ..Default::default()
        };
        db.upsert(&knowledge).unwrap();

        // Registration must happen first (in production, reindex_file does this).
        crate::wiring::refresh_file_producers(
            &db,
            "src/new_module.rs",
            "rust",
            "pub struct NewService;\n",
        );
        crate::wiring::update_wiring_after_edit(&db, "src/new_module.rs");
        let issues = verify_wiring_status(&db, "src/new_module.rs");
        assert!(
            issues.iter().any(|i| i.contains("orphan")),
            "should detect orphan pub symbols: {issues:?}"
        );
    }

    // ── Verify Functions Directly ───────────────────────────────────────

    #[test]
    fn test_verify_antipatterns_rust() {
        let tmp = TempDir::new().unwrap();
        let file_path = tmp.path().join("bad.rs");
        let content = "pub fn bad() {\n    let x = get().unwrap();\n    todo!()\n}\n";
        std::fs::write(&file_path, content).unwrap();

        let issues = verify_antipatterns(file_path.to_str().unwrap_or_default(), "rust", None);
        // The shared antipatterns impl returns one issue per pattern.
        assert!(
            issues.iter().any(|i| i.contains("unwrap")),
            "should detect unwrap: {issues:?}"
        );
        assert!(
            issues.iter().any(|i| i.contains("todo")),
            "should detect todo: {issues:?}"
        );
    }

    #[test]
    fn test_verify_antipatterns_python() {
        let tmp = TempDir::new().unwrap();
        let file_path = tmp.path().join("bad.py");
        let content =
            "def process(items=[]):\n    try:\n        run()\n    except:\n        pass\n";
        std::fs::write(&file_path, content).unwrap();

        let issues = verify_antipatterns(file_path.to_str().unwrap_or_default(), "python", None);
        assert!(
            issues
                .iter()
                .any(|i| i.contains("except") || i.contains("mutable")),
            "should detect Python antipatterns: {issues:?}"
        );
    }

    #[test]
    fn test_detect_language_mapping() {
        assert_eq!(detect_language("src/main.rs"), "rust");
        assert_eq!(detect_language("app.py"), "python");
        assert_eq!(detect_language("index.ts"), "typescript");
        assert_eq!(detect_language("index.tsx"), "typescript");
        assert_eq!(detect_language("app.js"), "javascript");
        assert_eq!(detect_language("app.jsx"), "javascript");
        assert_eq!(detect_language("main.go"), "go");
        assert_eq!(detect_language("Main.java"), "java");
        assert_eq!(detect_language("README.md"), "markdown");
        assert_eq!(detect_language("Makefile"), "unknown");
        assert_eq!(detect_language(""), "unknown");
    }

    // ── load_source_for_verify ────────────────────────────────────────

    #[test]
    fn test_load_source_prefers_content_over_disk() {
        // Even with a nonexistent path, content is returned when provided.
        let result = load_source_for_verify("/nonexistent/file.rs", Some("fn main() {}"));
        assert_eq!(result, Some("fn main() {}".to_string()));
    }

    #[test]
    fn test_load_source_reads_disk_when_no_content() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("test.rs");
        std::fs::write(&path, "fn foo() {}").unwrap();
        let result = load_source_for_verify(path.to_str().unwrap(), None);
        assert_eq!(result, Some("fn foo() {}".to_string()));
    }

    #[test]
    fn test_load_source_returns_none_for_missing_file() {
        let result = load_source_for_verify("/nonexistent/file.rs", None);
        assert!(result.is_none());
    }

    // ── budget (S28: the shared post_feedback::fit_budget over cila_budget_write) ──

    fn write_budget(items: &mut Vec<String>, level: u8) -> usize {
        crate::post_feedback::fit_budget(items, crate::shared::cila::cila_budget_write(level))
    }

    #[test]
    fn test_write_budget_retains_within_budget() {
        let mut issues = vec!["short".to_string(), "also short".to_string()];
        write_budget(&mut issues, 3); // L3 → large budget
        assert_eq!(issues.len(), 2, "both issues should fit in a large budget");
    }

    #[test]
    fn test_write_budget_drops_excess_at_low_level() {
        // L0 budget is smallest — build a list that overflows it.
        let budget = crate::shared::cila::cila_budget_write(0);
        let big_issue = "x".repeat(budget + 10);
        let mut issues = vec![big_issue, "second".to_string()];
        write_budget(&mut issues, 0);
        assert!(issues.len() < 2, "oversized issue should be dropped at L0");
    }

    #[test]
    fn test_write_budget_empty_list() {
        let mut issues: Vec<String> = vec![];
        assert_eq!(write_budget(&mut issues, 2), 0);
        assert!(issues.is_empty());
    }

    // ── collect_failed_layer_issues ──────────────────────────────────

    #[test]
    fn test_collect_failed_layer_issues_empty_when_all_pass() {
        // All layers passed — no issues should be emitted.
        let layers: Vec<touring_code::ast::LayerResult> = vec![touring_code::ast::LayerResult {
            layer: touring_code::ast::ValidationLayer::Syntax,
            passed: true,
            diagnostics: vec![],
            score: 1.0,
        }];
        let issues = collect_failed_layer_issues(&layers);
        assert!(issues.is_empty());
    }

    #[test]
    fn test_collect_failed_layer_issues_generic_message_when_no_diagnostics() {
        let layers: Vec<touring_code::ast::LayerResult> = vec![touring_code::ast::LayerResult {
            layer: touring_code::ast::ValidationLayer::Syntax,
            passed: false,
            diagnostics: vec![],
            score: 0.0,
        }];
        let issues = collect_failed_layer_issues(&layers);
        assert_eq!(issues.len(), 1);
        assert!(
            issues[0].contains("failed"),
            "generic message expected: {:?}",
            issues[0]
        );
    }

    #[test]
    fn test_collect_failed_layer_issues_at_most_3_diagnostics() {
        let layers: Vec<touring_code::ast::LayerResult> = vec![touring_code::ast::LayerResult {
            layer: touring_code::ast::ValidationLayer::Syntax,
            passed: false,
            diagnostics: vec![
                "d1".to_string(),
                "d2".to_string(),
                "d3".to_string(),
                "d4".to_string(),
            ],
            score: 0.0,
        }];
        let issues = collect_failed_layer_issues(&layers);
        assert_eq!(issues.len(), 1);
        // Only 3 diagnostics joined; d4 must not appear.
        assert!(!issues[0].contains("d4"), "should cap at 3 diagnostics");
        assert!(issues[0].contains("d1") && issues[0].contains("d3"));
    }

    // ── build_response (S28: typed feedback, the header counts issues only) ──

    fn fb(issues: &[&str], signals: &[&str]) -> crate::post_feedback::Feedback {
        crate::post_feedback::Feedback {
            issues: issues.iter().map(|s| (*s).to_string()).collect(),
            signals: signals.iter().map(|s| (*s).to_string()).collect(),
        }
    }

    #[test]
    fn test_build_response_empty_feedback_returns_allow() {
        let response = build_response(&fb(&[], &[]), 0, "src/test.rs", 0);
        assert!(
            matches!(response, HookResponse::Allow),
            "nothing to report: {response:?}"
        );
    }

    #[test]
    fn test_build_response_counts_issues_not_signals() {
        let response = build_response(
            &fb(
                &["issue A", "issue B"],
                &["⚙ health-delta: first-observation new=1.00"],
            ),
            0,
            "src/test.rs",
            0,
        );
        match response {
            HookResponse::Context {
                context,
                event_name,
            } => {
                assert!(
                    context.starts_with("post-write: 2 issue(s) | issue A | issue B"),
                    "{context}"
                );
                assert!(context.contains("\u{2139} ⚙ health-delta"), "{context}");
                assert_eq!(event_name.as_deref(), Some("PostToolUse"));
            }
            other => panic!("expected Context variant, got {other:?}"),
        }
    }

    #[test]
    fn test_build_response_signal_only_has_no_issue_count() {
        match build_response(
            &fb(&[], &["⚙ health-delta: first-observation new=1.00"]),
            0,
            "src/test.rs",
            0,
        ) {
            HookResponse::Context { context, .. } => {
                assert!(!context.contains("issue(s)"), "{context}")
            }
            other => panic!("expected Context, got {other:?}"),
        }
    }

    #[test]
    fn test_build_response_truncates_long_issues() {
        let long_issue = "z".repeat(200);
        match build_response(&fb(&[long_issue.as_str()], &[]), 0, "src/test.rs", 0) {
            HookResponse::Context { context, .. } => {
                assert!(
                    !context.contains(&"z".repeat(200)),
                    "long issue should be truncated"
                );
            }
            other => panic!("expected Context, got {other:?}"),
        }
    }

    #[test]
    fn test_build_response_declares_elision() {
        match build_response(&fb(&["kept"], &[]), 3, "src/test.rs", 0) {
            HookResponse::Context { context, .. } => {
                assert!(
                    context.contains("4 issue(s)") && context.contains("+3 elided"),
                    "{context}"
                );
            }
            other => panic!("expected Context, got {other:?}"),
        }
    }

    // ── collect_failed_layer_issues: multi-layer coverage ───────────────────

    #[test]
    fn test_collect_failed_layer_issues_mixed_pass_fail() {
        let layers = vec![
            touring_code::ast::LayerResult {
                layer: touring_code::ast::ValidationLayer::Syntax,
                passed: true,
                diagnostics: vec![],
                score: 1.0,
            },
            touring_code::ast::LayerResult {
                layer: touring_code::ast::ValidationLayer::SymbolResolution,
                passed: false,
                diagnostics: vec!["undefined: Bar".to_string()],
                score: 0.0,
            },
        ];
        let issues = collect_failed_layer_issues(&layers);
        // Only the failed layer contributes.
        assert_eq!(issues.len(), 1);
        assert!(issues[0].contains("SymbolResolution") || issues[0].contains("Bar"));
    }

    #[test]
    fn test_collect_failed_layer_issues_multiple_failed_layers() {
        let layers = vec![
            touring_code::ast::LayerResult {
                layer: touring_code::ast::ValidationLayer::Syntax,
                passed: false,
                diagnostics: vec!["syntax err".to_string()],
                score: 0.0,
            },
            touring_code::ast::LayerResult {
                layer: touring_code::ast::ValidationLayer::ImportCheck,
                passed: false,
                diagnostics: vec!["missing import".to_string()],
                score: 0.0,
            },
        ];
        let issues = collect_failed_layer_issues(&layers);
        assert_eq!(issues.len(), 2, "one issue per failed layer");
    }

    // ── block gate (S28: NEW occurrences, before × after, shared with post_edit) ──

    #[test]
    fn test_build_response_blocks_on_new_antipatterns() {
        match build_response(&fb(&["[rust] `.unwrap()` (4x)"], &[]), 0, "src/bad.rs", 4) {
            HookResponse::Block {
                reason,
                context,
                event_name,
            } => {
                assert!(
                    reason.starts_with("Write blocked")
                        && reason.contains("too many new anti-patterns"),
                    "{reason}"
                );
                assert!(
                    reason.contains("src/bad.rs"),
                    "reason should contain path: {reason}"
                );
                assert!(
                    context.is_some_and(|c| c.contains("unwrap")),
                    "the issues travel as context"
                );
                assert_eq!(event_name.as_deref(), Some("PostToolUse"));
            }
            other => panic!("expected Block variant, got {other:?}"),
        }
    }

    #[test]
    fn test_build_response_below_threshold_is_context() {
        let response = build_response(&fb(&["[rust] `.unwrap()` (1x)"], &[]), 0, "src/ok.rs", 3);
        assert!(
            matches!(response, HookResponse::Context { .. }),
            "3 new is below the threshold: {response:?}"
        );
    }
}
