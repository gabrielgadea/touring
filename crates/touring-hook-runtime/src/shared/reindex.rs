// Shared file re-indexing for touring-hooks.
//
// Centralizes `reindex_file` which was duplicated in post_edit and post_write.

use std::path::Path;

use crate::runtime::HookRuntime;

use super::feature_flags::extract_features_auto;

/// Error from [`reindex_file`] / [`reindex_file_with_old`] (F-8 / RBP-03: typed
/// in place of `String`). `From<String>` lets existing `?`-propagated `format!`
/// messages convert transparently.
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct ReindexError(pub String);

impl From<String> for ReindexError {
    fn from(message: String) -> Self {
        Self(message)
    }
}

/// Compute byte offsets for an incremental edit from old→new content.
///
/// Returns `(start_byte, old_end_byte)` where:
/// - `start_byte` is the first position where `old` and `new` differ
/// - `old_end_byte` is the position in `old` where the common suffix begins
fn compute_edit_offsets(old: &str, new: &str) -> (usize, usize) {
    let old_bytes = old.as_bytes();
    let new_bytes = new.as_bytes();

    let start_byte = old_bytes
        .iter()
        .zip(new_bytes.iter())
        .take_while(|(o, n)| o == n)
        .count();

    let old_len = old_bytes.len();
    let new_len = new_bytes.len();
    let max_suffix = old_len
        .saturating_sub(start_byte)
        .min(new_len.saturating_sub(start_byte));
    let mut suffix_len = 0;
    while suffix_len < max_suffix {
        if old_bytes[old_len - 1 - suffix_len] == new_bytes[new_len - 1 - suffix_len] {
            suffix_len += 1;
        } else {
            break;
        }
    }

    let old_end_byte = old_len - suffix_len;
    (start_byte, old_end_byte)
}

/// Extract symbols using IncrementalPipeline (process_edit or process_file).
///
/// Returns `(symbols_json, symbol_count)` or falls back to `extract_symbols_fallback`
/// on any pipeline error.
///
/// # 2026-05-10 fix — incremental write to `symbols.db`
///
/// Before this revision, the pipeline result was only serialized into
/// `FileKnowledge.symbols_json` (knowledge.db). The actual `symbols` table in
/// `symbols.db` was **never updated** by the incremental path — only by the
/// full `cli_index_rebuild` walker, which explicitly calls
/// `store.replace_file_symbols(...)` after each `process_file`.
///
/// Consequence: `touring index find <NewSymbol>` returned 0 hits after
/// every edit, even though `post_edit` hooks reported success. The
/// daemon-level cache stayed warm but the queryable symbol store drifted
/// from reality after the very first edit.
///
/// Fix: when a `symbol_store` is available, mirror `cli_index_rebuild`'s
/// behaviour and atomically replace the file's symbol rows with
/// `result.symbols_added` immediately after the pipeline succeeds.
/// Mirrors `cli_index_rebuild`'s references branch (B 2026-06-21) on the
/// INCREMENTAL path: appends call-sites as `is_definition=false, kind="call"`
/// rows so find-references and wiring learn consumers from edits/ingest, not
/// only from full rebuilds. Root cause closed (cross-audit 2026-08-12): any
/// file only ever *edited* — never rebuilt — recorded zero references, so a
/// symbol used cross-crate via `module::symbol(...)` (e.g. `tags::derive_tags`)
/// read as 0 consumers and surfaced as a false wiring orphan.
/// Same kill-switch as the rebuild: `TOURING_INDEX_REFERENCES=0` disables.
fn with_call_sites(
    rel_path: &str,
    content: &str,
    mut symbols: Vec<touring_code::ast::SymbolLocation>,
) -> Vec<touring_code::ast::SymbolLocation> {
    let references_on = std::env::var("TOURING_INDEX_REFERENCES")
        .map(|v| v != "0" && v != "false")
        .unwrap_or(true);
    if !references_on {
        return symbols;
    }
    let Some(lang) = touring_code::ast::Lang::from_path(Path::new(rel_path)) else {
        return symbols;
    };
    for cs in touring_code::ast::build_call_graph(content, lang).sites {
        symbols.push(
            touring_code::ast::SymbolLocation::new(rel_path, cs.callee, cs.line, 0, false)
                .with_kind(Some("call".to_string())),
        );
    }
    symbols
}

fn extract_symbols_via_pipeline(
    pipeline: &touring_code::ast::incremental_pipeline::SharedPipeline,
    symbol_store: Option<&touring_code::ast::store::SymbolStore>,
    rel_path: &str,
    content: &str,
    old_content: Option<&str>,
    full_path: &str,
) -> (String, i64) {
    // Try process_edit first when we have cached tree + old content.
    if let Some(old) = old_content
        && pipeline.has_cached_tree(rel_path)
    {
        // Sprint 4.7 (2026-05-24) — upstream defense for ropey OOB panic:
        //
        // `compute_edit_offsets(old, content)` returns `old_end_byte ≤ old.len()`,
        // valid for the `old` snapshot. But if the cached pipeline doc has
        // drifted (cross-actor race, queued-after-mutation, file watcher
        // racing CC edit, full reparse intervening), `old.len()` may exceed
        // the cached rope's `len_bytes`. In that case `old_end_byte` is
        // out-of-bounds for the cached rope, and `Document::edit` would
        // historically panic in `Rope::byte_to_char`.
        //
        // Sprint 4.6 added a clamp+warn defense inside `Document::edit`
        // so the daemon no longer dies, but the resulting "edit" applied
        // to a divergent rope state produces a meaningless InputEdit. Far
        // better to detect the drift here and bail to a full reparse —
        // which is what cli_index_rebuild does and is known-good.
        //
        // Forensic anchor: Sprint 4.6 captured panic at byte_idx=84877
        // vs cached rope len=84279 (drift = 598 bytes) on thread
        // `touring-project-actor`.
        let cached_len = pipeline.with_read(|p| p.get_document(rel_path).map(|d| d.len_bytes()));
        if cached_len != Some(old.len()) {
            tracing::debug!(
                target: "touring::reindex",
                file = %rel_path,
                expected_old_len = old.len(),
                cached_doc_len = ?cached_len,
                "old content drift vs cached pipeline doc — bailing to full reparse \
                 (avoids stale-offset OOB in Document::edit; Sprint 4.7 upstream defense)"
            );
            // Skip the incremental block — fall through to full reparse below.
        } else {
            let (start_byte, old_end_byte) = compute_edit_offsets(old, content);
            if let Ok(result) =
                pipeline.with_write(|p| p.process_edit(rel_path, start_byte, old_end_byte, content))
            {
                if let Some(store) = symbol_store
                    && let Err(e) = store.replace_file_symbols(
                        rel_path,
                        &with_call_sites(rel_path, content, result.symbols_added.clone()),
                    )
                {
                    tracing::warn!(
                        target: "touring::reindex",
                        file = %rel_path,
                        error = %e,
                        "process_edit succeeded but replace_file_symbols failed — \
                         symbols.db will drift; try `touring index rebuild`",
                    );
                }
                let symbols_json = serde_json::to_string(&result.symbols_added)
                    .unwrap_or_else(|_| "[]".to_string());
                return (symbols_json, result.symbols_added.len() as i64);
            }
        } // close `} else {` block of Sprint 4.7 drift check
    }

    // Fall back to full reparse.
    match pipeline.with_write(|p| p.process_file(rel_path, content)) {
        Ok(result) => {
            if let Some(store) = symbol_store
                && let Err(e) = store.replace_file_symbols(
                    rel_path,
                    &with_call_sites(rel_path, content, result.symbols_added.clone()),
                )
            {
                tracing::warn!(
                    target: "touring::reindex",
                    file = %rel_path,
                    error = %e,
                    "process_file succeeded but replace_file_symbols failed — \
                     symbols.db will drift; try `touring index rebuild`",
                );
            }
            let symbols_json =
                serde_json::to_string(&result.symbols_added).unwrap_or_else(|_| "[]".to_string());
            (symbols_json, result.symbols_added.len() as i64)
        }
        Err(e) => {
            tracing::debug!("pipeline failed for {rel_path}: {e}, using fallback");
            extract_symbols_fallback(symbol_store, rel_path, content, full_path)
        }
    }
}

/// Fallback symbol extraction when pipeline is unavailable or fails.
///
/// Follow-up F-1 (cross-audit 2026-08-12): the old fallback returned
/// `(json, count)` for the knowledge DB but never wrote to the symbol
/// store — a file indexed through this path had NO rows in symbols.db at
/// all (not even definitions), so it was invisible to find-references and
/// its consumers never appeared in the wiring map. Now the fallback parses
/// once via `touring_code::ast::extract_symbols` and persists definitions +
/// call-sites through the same `replace_file_symbols` + `with_call_sites`
/// pair the pipeline paths use — all three reindex paths keep the index.
fn extract_symbols_fallback(
    symbol_store: Option<&touring_code::ast::store::SymbolStore>,
    rel_path: &str,
    content: &str,
    full_path: &str,
) -> (String, i64) {
    let Some(lang) = touring_code::ast::Lang::from_path(Path::new(full_path)) else {
        return (String::new(), 0);
    };
    let symbols = match touring_code::ast::extract_symbols(content, lang) {
        Ok(v) => v,
        Err(_) => return (String::new(), 0),
    };
    let count = symbols.len();
    // Same compact shape `enrich_file_knowledge` always produced — the
    // knowledge.symbols_json contract is unchanged.
    let compact: Vec<serde_json::Value> = symbols
        .iter()
        .map(|s| {
            serde_json::json!({
                "name": s.name,
                "kind": s.kind.as_str(),
                "is_public": s.is_public,
                "line": s.line,
            })
        })
        .collect();
    let json = serde_json::to_string(&compact).unwrap_or_else(|_| "[]".to_string());

    if let Some(store) = symbol_store {
        let locations: Vec<touring_code::ast::SymbolLocation> = symbols
            .iter()
            .map(|s| {
                touring_code::ast::SymbolLocation::new(
                    rel_path,
                    s.name.clone(),
                    s.line,
                    s.column,
                    true,
                )
                .with_kind(Some(s.kind.as_str().to_string()))
            })
            .collect();
        // Same document row the pipeline extractor adds (`ast::markdown_text`):
        // both writers of the store must agree on what a markdown file holds.
        let locations =
            touring_code::ast::markdown_text::with_document_symbol(rel_path, content, locations);
        let with_refs = with_call_sites(rel_path, content, locations);
        if let Err(e) = store.replace_file_symbols(rel_path, &with_refs) {
            tracing::warn!(
                target: "touring::reindex",
                file = %rel_path,
                error = %e,
                "fallback replace_file_symbols failed — symbols.db will drift;                  try `touring index rebuild`",
            );
        }
    }
    (json, count as i64)
}

/// Re-index a file after edit/write (update knowledge DB with current content).
///
/// Reads the file from disk, extracts symbols and imports via tree-sitter
/// (falling back to regex), upserts into the knowledge DB, updates relations,
/// and refreshes the wiring map. Also wires Pln2 extended data:
///
/// - Feature flags (Cargo.toml, pyproject.toml, package.json, shell scripts)
/// - BLAKE3 content hash
/// - TODOs/FIXMEs extracted from content
///
/// When `old_content` is provided AND the pipeline has a cached tree for this
/// file, uses `process_edit` (O(edit_region)) instead of `process_file` (O(file))
/// — up to 6.9× faster for cached-tree edits.
///
/// Files the walker would refuse are not written (see [`admission_refusal`]).
pub fn reindex_file(
    runtime: &HookRuntime,
    abs_path: &str,
    rel_path: &str,
) -> Result<(), ReindexError> {
    reindex_file_with_old(runtime, abs_path, rel_path, None)
}

/// The walker's verdict for a path a hook or an ingest is about to write.
/// `Ok(key)` is the ONE spelling the store accepts for it — the project-relative
/// path, or `@companion/<name>/<rel>` for a file under a companion root (the
/// global rules, skills, agents and commands, the auto-memory of the project and
/// of `~`; 13/09/2026); `Err(exclusion)` means the store must not receive it.
/// Shared so the `index ingest` handler can REPORT the refusal where the hook
/// only logs it — one predicate (`touring_hooks_shared::index_policy`), three
/// readers.
///
/// # Errors
///
/// The walker's exclusion (verdict + detail) when the file must not be written.
pub fn admission_refusal(
    runtime: &HookRuntime,
    rel_path: &str,
) -> Result<String, (touring_hooks_shared::index_policy::IndexVerdict, String)> {
    admission_key_for_root(&runtime.project_root, rel_path)
}

/// [`admission_refusal`] for a caller that holds only the project root (the
/// wiring refresh of `file_changed` and `task_output`, which receive a path and a
/// knowledge DB, not a runtime).
///
/// # Errors
///
/// The walker's exclusion (verdict + detail) when the file must not be written.
pub fn admission_key_for_root(
    project_root: &Path,
    rel_path: &str,
) -> Result<String, (touring_hooks_shared::index_policy::IndexVerdict, String)> {
    // `rel_path` is what `make_relative` produced: relative under the root, or
    // the untouched absolute path when the file lives outside it — exactly the
    // two shapes the policy judges (an absolute path under a companion root
    // becomes its companion key; anywhere else it is `outside_root`).
    let policy = touring_hooks_shared::index_policy::IndexPolicy::for_root(project_root);
    if let Some(refused) = policy.verdict_for_key(rel_path) {
        return Err(refused);
    }
    let key = if Path::new(rel_path).is_absolute() {
        policy
            .key_for(Path::new(rel_path))
            .unwrap_or_else(|| rel_path.to_string())
    } else {
        rel_path.trim_start_matches("./").to_string()
    };
    Ok(key)
}

/// Replaces the search documents of `key` with the rows the store now holds
/// for it, and commits. Fail-open: a search index that cannot be written never
/// fails the reindex — it is logged and the next rebuild converges it.
#[cfg(feature = "tantivy-fts")]
fn refresh_search_documents(runtime: &HookRuntime, key: &str, content: &str) {
    let Some(idx) = crate::tantivy_index::tantivy_for(Some(&runtime.project_root)) else {
        return;
    };
    let Some(store) = runtime.infra.symbol_store.as_ref() else {
        return;
    };
    let symbols = match store.find_symbols_in_file(key) {
        Ok(rows) => rows,
        Err(e) => {
            tracing::debug!(target: "touring::reindex", file = %key, error = %e, "search refresh skipped: store read failed");
            return;
        }
    };
    let docs = crate::shared::tantivy_docs::docs_for_file(key, &symbols, Some(content));
    if let Err(e) =
        crate::shared::tantivy_docs::refresh_file(idx, key, &docs).and_then(|_| idx.commit())
    {
        tracing::warn!(target: "touring::reindex", file = %key, error = %e, "search documents not refreshed; `touring index rebuild` converges them");
    }
}

/// Like `reindex_file` but accepts the old file content to enable incremental
/// re-parsing when a cached tree exists in the pipeline.
pub fn reindex_file_with_old(
    runtime: &HookRuntime,
    abs_path: &str,
    rel_path: &str,
    old_content: Option<&str>,
) -> Result<(), ReindexError> {
    let full_path = if std::path::Path::new(abs_path).is_absolute() {
        abs_path.to_string()
    } else {
        runtime
            .project_root
            .join(abs_path)
            .to_string_lossy()
            .to_string()
    };

    // I13/I16 (2026-09-13): the hook writers ask the SAME admission predicate the
    // rebuild walker applies. Until this gate every edit under `.claude/`, every
    // script in a session scratchpad and every `~/.claude/rules/*.md` landed in
    // THIS project's symbols.db (140 files under absolute paths, measured live),
    // and no rebuild ever removed them — the sweep only retired paths gone from
    // disk. Refused here, reported by `touring index why <path>`.
    let storage_key = match admission_refusal(runtime, rel_path) {
        Ok(key) => key,
        Err((verdict, detail)) => {
            tracing::info!(
                target: "touring::reindex",
                file = %rel_path,
                verdict = verdict.as_str(),
                "not an index candidate — not written ({detail}); `touring index why <path>` explains"
            );
            return Ok(());
        }
    };
    // From here on the file is known by its storage key: project-relative, or
    // `@companion/<name>/<rel>` for a rule, skill, command, agent or memory.
    let rel_path: &str = &storage_key;
    // A fatal signal while this file is parsed names it in daemon-crash.jsonl.
    let _crash_context = touring_hooks_core::panic_log::CrashContext::enter(rel_path);

    let content = match std::fs::read_to_string(&full_path) {
        Ok(c) => c,
        Err(_) => return Ok(()),
    };

    let path = Path::new(rel_path);
    let language = crate::shared::detect_language::detect_language_owned(rel_path);
    let line_count = content.lines().count() as i64;

    #[cfg(feature = "post-hooks")]
    let imports = crate::symbol_extractors::extract_imports_fast(&content, &language);
    #[cfg(not(feature = "post-hooks"))]
    let imports: Vec<String> = Vec::new();

    // P2-C / P4.3: Use IncrementalPipeline when available.
    //
    // Priority order:
    // 1. process_edit — O(edit_region) when we have a cached tree + old content.
    //    This is the hot path after the first reindex populates the tree cache.
    // 2. process_file — O(file) full reparse for cache misses or no old content.
    //    Also used on first edit after daemon startup (tree not yet cached).
    // 3. extract_symbols_fallback — regex-based when pipeline unavailable.
    let (symbols_json, symbol_count) = if let Some(ref pipeline) = runtime.infra.pipeline {
        extract_symbols_via_pipeline(
            pipeline,
            runtime.infra.symbol_store.as_ref(),
            rel_path,
            &content,
            old_content,
            &full_path,
        )
    } else {
        extract_symbols_fallback(
            runtime.infra.symbol_store.as_ref(),
            rel_path,
            &content,
            &full_path,
        )
    };

    // The search index follows the store for this file — names for code, text
    // for markdown (`shared::tantivy_docs`). Before this the hook path upserted
    // one `file` document and a memory written in the session stayed unsearchable
    // until someone ran `touring tantivy reindex`.
    #[cfg(feature = "tantivy-fts")]
    refresh_search_documents(runtime, rel_path, &content);

    let knowledge = crate::knowledge::FileKnowledge {
        file_path: rel_path.to_string(),
        language: Some(language.clone()),
        line_count,
        symbol_count,
        imports_json: Some(serde_json::to_string(&imports).unwrap_or_default()),
        symbols_json: Some(symbols_json),
        ..Default::default()
    };

    let _ = runtime.ctx.knowledge.upsert(&knowledge);

    // Update relations.
    #[cfg(feature = "post-hooks")]
    {
        let relations: Vec<crate::knowledge::FileRelation> = imports
            .iter()
            .filter_map(|imp| {
                crate::symbol_extractors::resolve_import_path(imp, &language).map(|target| {
                    crate::knowledge::FileRelation {
                        source: rel_path.to_string(),
                        target,
                        relation_type: "imports".to_string(),
                    }
                })
            })
            .collect();

        if !relations.is_empty() {
            let _ = runtime
                .ctx
                .knowledge
                .replace_relations_from(rel_path, &relations);
        }
    }

    // Wiring Intelligence: every row this file owns — producers with their real
    // visibility, `use` imports, direct-path calls (FIX-2: `crate::mod::fn(...)`
    // sites with no `use`) and the inferred edges — through the one function the
    // read, file-changed and task-output paths share (cross-audit 14/09/2026, B1/B2).
    crate::wiring::refresh_file_wiring(&runtime.ctx.knowledge, rel_path, &language, &content);

    // ── Pln2: Wire feature flags into file_feature_flags table ─────────────
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    let is_config_file = matches!(ext, "toml" | "pyproject" | "json" | "sh" | "bash" | "zsh")
        || rel_path.contains("package.json")
        || rel_path.ends_with("Cargo.toml")
        || rel_path.ends_with("pyproject.toml");
    if is_config_file || ext == "rs" {
        let features = extract_features_auto(path, &content);
        if !features.is_empty() {
            let feature_pairs: Vec<(&str, &str)> = features
                .iter()
                .map(|f| {
                    (
                        f.as_str(),
                        if ext == "py" || ext == "pyproject" {
                            "python"
                        } else {
                            "rust"
                        },
                    )
                })
                .collect();
            let _ = runtime
                .ctx
                .knowledge
                .upsert_feature_flags_batch(rel_path, &feature_pairs);
        }
    }

    // ── Pln2: Wire BLAKE3 hash into file_blake3_registry table ───────────
    //
    // Fast pre-filter: use AES-NI accelerated fast_content_hash (via
    // touring_analysis::quality) to skip the blake3 computation entirely when
    // old_content is available and the quick hash confirms the content is
    // unchanged.  The fast hash is ~3–10× faster than blake3 and covers the
    // common case where post_edit fires but the file bytes are identical
    // (e.g. whitespace-only edits or no-op saves).
    //
    // Collision probability is ~1/2^64 per pair — negligible for this use case.
    // When hashes agree we still update the symbol_count via a lightweight path
    // that skips the heavier blake3 I/O but preserves correctness.
    let skip_blake3 = if let Some(old) = old_content {
        !crate::shared::quality::quick_content_changed(old, &content)
    } else {
        false
    };

    if !skip_blake3 {
        use blake3::Hasher;
        let mut hasher = Hasher::new();
        hasher.update(content.as_bytes());
        let hash = hasher.finalize().to_hex().to_string();
        let _ = runtime
            .ctx
            .knowledge
            .upsert_blake3_registry(rel_path, &hash, symbol_count, None);
    }

    // ── Pln2: Wire TODOs/FIXMEs into file_todos table ────────────────────
    for (line_idx, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        let kind = if trimmed.starts_with("TODO") {
            "TODO"
        } else if trimmed.starts_with("FIXME") {
            "FIXME"
        } else if trimmed.starts_with("XXX") {
            "XXX"
        } else {
            continue;
        };
        // Extract the actual content after the tag
        let content_part = trimmed
            .find(':')
            .map(|p| trimmed[p + 1..].trim())
            .unwrap_or("");
        if !content_part.is_empty() {
            let _ = runtime.ctx.knowledge.insert_todo(
                rel_path,
                (line_idx + 1) as i64,
                kind,
                content_part,
            );
        }
    }

    Ok(())
}

#[cfg(test)]
mod fallback_store_tests {
    /// F-1 (cross-audit 2026-08-12): the fallback path must persist
    /// definitions AND call-sites to the symbol store — a file indexed
    /// through the fallback is no longer invisible to symbols.db.
    #[test]
    fn fallback_writes_defs_and_calls_to_store() {
        let tmp = tempfile::TempDir::new().unwrap();
        let db = tmp.path().join("sym.db");
        let store = touring_code::ast::store::SymbolStore::new(&db).unwrap();
        let src =
            "pub fn producer() {}\nfn caller() {\n    producer();\n    tags::derive_tags();\n}\n";
        let (json, count) = super::extract_symbols_fallback(
            Some(&store),
            "src/demo.rs",
            src,
            "/abs/proj/src/demo.rs",
        );
        assert!(count >= 2, "defs extracted: {json}");
        let defs = store.find_symbol("producer").unwrap();
        assert!(defs.iter().any(|l| l.is_definition), "def row persisted");
        let refs = store.find_references("derive_tags").unwrap();
        assert!(
            refs.iter()
                .any(|l| !l.is_definition && l.kind.as_deref() == Some("call")),
            "call-site row persisted: {refs:?}"
        );
    }
}

#[cfg(test)]
mod call_site_tests {
    use super::with_call_sites;

    /// Cross-audit 2026-08-12 regression guard: the incremental path must
    /// append call-site rows (kind="call", is_definition=false) for scoped
    /// calls — the gap that made cross-crate consumers read as orphans.
    #[test]
    fn with_call_sites_appends_scoped_calls() {
        let src = "fn f() {\n    tags::derive_tags(k);\n    plain_call();\n}\n";
        let out = with_call_sites("src/rlm.rs", src, Vec::new());
        let names: Vec<&str> = out.iter().map(|s| s.symbol_name.as_str()).collect();
        assert!(names.contains(&"derive_tags"), "scoped call: {names:?}");
        assert!(names.contains(&"plain_call"), "direct call: {names:?}");
        assert!(out.iter().all(|s| s.kind.as_deref() == Some("call")));
        assert!(out.iter().all(|s| !s.is_definition));
    }

    /// The kill-switch mirrors the rebuild: TOURING_INDEX_REFERENCES=0 keeps
    /// the incremental path definitions-only. Env access serialized (the
    /// Rust-2024 discipline, same as the crate-wide locks elsewhere).
    #[test]
    fn with_call_sites_respects_kill_switch() {
        static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _guard = ENV_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        unsafe { std::env::set_var("TOURING_INDEX_REFERENCES", "0") };
        let out = with_call_sites("src/a.rs", "fn f() { g::h(); }", Vec::new());
        unsafe { std::env::remove_var("TOURING_INDEX_REFERENCES") };
        drop(_guard);
        assert!(out.is_empty());
    }

    /// Non-source files yield no call rows (Lang::from_path → None).
    #[test]
    fn with_call_sites_skips_unknown_languages() {
        let out = with_call_sites("docs/guide.txt", "anything()", Vec::new());
        assert!(out.is_empty());
    }
}

/// The edit path end to end (14/09/2026): an edit re-derives the file's producer
/// rows from what is on disk. Before, it cleared them and re-registered from a
/// stored JSON without visibility, so every edited file ended with none.
#[cfg(test)]
mod edit_producer_tests {
    fn producers(rt: &crate::HookRuntime, file: &str) -> Vec<String> {
        let mut stmt = rt
            .ctx
            .knowledge
            .conn_ref()
            .prepare(
                "SELECT symbol_name FROM wiring_map \
                 WHERE module_file = ?1 AND consumer_file IS NULL ORDER BY symbol_name",
            )
            .expect("prepare");
        stmt.query_map([file], |r| r.get(0))
            .expect("query")
            .collect::<Result<_, _>>()
            .expect("rows")
    }

    #[test]
    fn an_edit_keeps_the_producers_its_content_declares() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();
        std::fs::create_dir_all(root.join("src")).expect("src");
        let file = root.join("src/flags.rs");
        std::fs::write(
            &file,
            "pub fn first() {}\npub fn second() {}\nfn private() {}\n",
        )
        .expect("write");
        let rt = crate::HookRuntime::new(root).expect("runtime");

        super::reindex_file_with_old(&rt, &file.to_string_lossy(), "src/flags.rs", None)
            .expect("reindex");
        assert_eq!(producers(&rt, "src/flags.rs"), ["first", "second"]);

        let before = std::fs::read_to_string(&file).expect("read");
        std::fs::write(&file, "pub fn first() {}\nfn private() {}\n").expect("edit");
        super::reindex_file_with_old(&rt, &file.to_string_lossy(), "src/flags.rs", Some(&before))
            .expect("reindex after edit");
        assert_eq!(
            producers(&rt, "src/flags.rs"),
            ["first"],
            "the removed pub fn is gone"
        );
    }
}
